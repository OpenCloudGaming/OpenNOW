use super::contract::{SessionOccupancy, SourceError};
use opennow_plugin_api::PluginId;
use opennow_plugin_api::provider::{AccountKey, OperationId, ReceiptId, SessionKey};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

const MAX_JOURNAL_BYTES: u64 = 128 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Allocating,
    AwaitingAcceptance,
    Active,
    Claiming,
    CleanupPending,
    Unknown,
    RecoveryPending,
    ActiveReceiptConflict,
    RemoteEnded,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MediaPin {
    pub lease_id: String,
    pub package_sha256: Option<String>,
    pub runtime_epoch: u64,
    pub attempt_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionRecord {
    pub source_id: PluginId,
    pub account: Option<AccountKey>,
    pub operation: OperationId,
    pub session: Option<SessionKey>,
    pub receipt: Option<ReceiptId>,
    pub phase: Phase,
    pub media: Option<MediaPin>,
    #[serde(default)]
    pub profile: Option<opennow_plugin_api::provider::StreamPreferences>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    session: Option<SessionRecord>,
    #[serde(default)]
    legacy_media: bool,
    #[serde(default)]
    media_revision: u64,
}

struct State {
    session: Option<SessionRecord>,
    unavailable: bool,
    legacy_media: bool,
    media_revision: u64,
}

pub struct SessionJournal {
    path: PathBuf,
    state: Mutex<State>,
}

impl SessionJournal {
    pub fn open(data_dir: &Path) -> Self {
        let path = data_dir.join("provider-session-v2.json");
        let loaded = read_document(&path);
        let state = match loaded {
            Ok(document) => State {
                session: document.session,
                legacy_media: document.legacy_media,
                media_revision: document.media_revision,
                unavailable: false,
            },
            Err(_) => State {
                session: None,
                legacy_media: false,
                media_revision: 0,
                unavailable: true,
            },
        };
        Self {
            path,
            state: Mutex::new(state),
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, State>, SourceError> {
        self.state.lock().map_err(|_| unavailable())
    }

    pub fn snapshot(&self) -> Result<Option<SessionRecord>, SourceError> {
        let state = self.lock()?;
        if state.unavailable {
            return Err(unavailable());
        }
        Ok(state.session.clone())
    }

    pub fn occupancy(&self) -> SessionOccupancy {
        let Ok(state) = self.state.try_lock() else {
            return SessionOccupancy::Unknown;
        };
        if state.unavailable {
            return SessionOccupancy::Unknown;
        }
        match state.session.as_ref().map(|record| record.phase) {
            None if state.legacy_media => SessionOccupancy::Unknown,
            None => SessionOccupancy::Idle,
            Some(Phase::Allocating | Phase::AwaitingAcceptance | Phase::Unknown) => {
                SessionOccupancy::Unknown
            }
            Some(_) => SessionOccupancy::InUse,
        }
    }

    pub fn source_in_use(&self, source: &PluginId) -> bool {
        let Ok(state) = self.state.lock() else {
            return true;
        };
        state.unavailable
            || source.is_builtin() && state.legacy_media
            || state
                .session
                .as_ref()
                .is_some_and(|record| &record.source_id == source)
    }

    pub fn mark_legacy_media(
        &self,
        source: &PluginId,
        session: &SessionKey,
    ) -> Result<(), SourceError> {
        let mut state = self.lock()?;
        if state.unavailable || !source.is_builtin() {
            return Err(unavailable());
        }
        let mut next = state.session.clone();
        exact_session(&mut next, source, session)?;
        let revision = state
            .media_revision
            .checked_add(1)
            .ok_or_else(unavailable)?;
        write_document(&self.path, &next, true, revision)?;
        state.legacy_media = true;
        state.media_revision = revision;
        Ok(())
    }

    pub fn media_revision(&self) -> Result<u64, SourceError> {
        let state = self.lock()?;
        if state.unavailable {
            return Err(unavailable());
        }
        Ok(state.media_revision)
    }

    pub fn observe_native(
        &self,
        legacy_active: bool,
        native_idle: bool,
        expected_revision: u64,
    ) -> Result<Option<String>, SourceError> {
        if legacy_active && native_idle {
            return Err(owner_mismatch());
        }
        let mut state = self.lock()?;
        if state.unavailable {
            return Err(unavailable());
        }
        if state.media_revision != expected_revision {
            return Err(SourceError::new(
                "stale_media_observation",
                "Native ownership changed after the observation began",
            ));
        }
        let legacy = if native_idle {
            false
        } else {
            state.legacy_media || legacy_active
        };
        let mut next = state.session.clone();
        let released = if native_idle {
            next.as_mut()
                .and_then(|record| record.media.take())
                .map(|pin| pin.lease_id)
        } else {
            None
        };
        if native_idle
            && next
                .as_ref()
                .is_some_and(|record| record.phase == Phase::RemoteEnded)
        {
            next = None;
        }
        if next != state.session || legacy != state.legacy_media {
            let revision = state
                .media_revision
                .checked_add(1)
                .ok_or_else(unavailable)?;
            write_document(&self.path, &next, legacy, revision)?;
            state.legacy_media = legacy;
            state.session = next;
            state.media_revision = revision;
        }
        Ok(released)
    }

    pub fn begin(
        &self,
        source: PluginId,
        account: Option<AccountKey>,
        operation: OperationId,
    ) -> Result<(), SourceError> {
        self.begin_profile(source, account, operation, None)
    }

    pub fn begin_profile(
        &self,
        source: PluginId,
        account: Option<AccountKey>,
        operation: OperationId,
        profile: Option<opennow_plugin_api::provider::StreamPreferences>,
    ) -> Result<(), SourceError> {
        self.mutate(|record, legacy_media| {
            if record.is_some() || legacy_media {
                return Err(in_use());
            }
            *record = Some(SessionRecord {
                source_id: source,
                account,
                operation,
                session: None,
                receipt: None,
                phase: Phase::Allocating,
                media: None,
                profile,
            });
            Ok(())
        })
    }

    pub fn begin_claim(
        &self,
        source: PluginId,
        operation: OperationId,
        session: SessionKey,
    ) -> Result<(), SourceError> {
        self.mutate(|record, legacy| {
            if record.is_some() || legacy {
                return Err(in_use());
            }
            *record = Some(SessionRecord {
                source_id: source,
                account: session.account.clone(),
                operation,
                session: Some(session),
                receipt: None,
                phase: Phase::Claiming,
                media: None,
                profile: None,
            });
            Ok(())
        })
    }

    pub fn allocated(
        &self,
        source: &PluginId,
        operation: &OperationId,
        session: SessionKey,
        receipt: ReceiptId,
    ) -> Result<(), SourceError> {
        self.mutate(|record, _| {
            let current = exact_operation(record, source, operation)?;
            if !matches!(current.phase, Phase::Allocating | Phase::Unknown)
                || current.account != session.account
                || current
                    .session
                    .as_ref()
                    .is_some_and(|known| known != &session)
            {
                return Err(owner_mismatch());
            }
            current.session = Some(session);
            current.receipt = Some(receipt);
            current.phase = Phase::AwaitingAcceptance;
            Ok(())
        })
    }

    pub fn settled(
        &self,
        source: &PluginId,
        operation: &OperationId,
        accepted: bool,
    ) -> Result<(), SourceError> {
        self.mutate(|record, legacy_media| {
            let current = exact_operation(record, source, operation)?;
            if current.phase != Phase::AwaitingAcceptance {
                return Err(owner_mismatch());
            }
            if accepted {
                current.phase = Phase::Active;
                current.receipt = None;
            } else if current.media.is_some() || legacy_media {
                current.phase = Phase::RemoteEnded;
                current.receipt = None;
            } else {
                *record = None;
            }
            Ok(())
        })
    }

    pub fn rejected_allocation_resolved(
        &self,
        source: &PluginId,
        operation: &OperationId,
    ) -> Result<(), SourceError> {
        self.mutate(|record, legacy_media| {
            let current = exact_operation(record, source, operation)?;
            if current.media.is_some() || legacy_media {
                current.phase = Phase::RemoteEnded;
            } else {
                *record = None;
            }
            Ok(())
        })
    }

    pub fn unknown(&self, source: &PluginId, operation: &OperationId) -> Result<(), SourceError> {
        self.mutate(|record, _| {
            exact_operation(record, source, operation)?.phase = Phase::Unknown;
            Ok(())
        })
    }

    pub fn recovery_unknown(
        &self,
        source: &PluginId,
        operation: &OperationId,
    ) -> Result<(), SourceError> {
        self.mutate(|record, _| {
            let current = exact_operation(record, source, operation)?;
            current.phase = match current.phase {
                Phase::Active | Phase::RecoveryPending => Phase::RecoveryPending,
                Phase::Claiming => Phase::Claiming,
                Phase::ActiveReceiptConflict => Phase::ActiveReceiptConflict,
                Phase::CleanupPending => Phase::CleanupPending,
                Phase::RemoteEnded => Phase::RemoteEnded,
                _ => Phase::Unknown,
            };
            Ok(())
        })
    }

    pub fn retain_conflicting_ticket(
        &self,
        source: &PluginId,
        operation: &OperationId,
        session: &SessionKey,
        receipt: &ReceiptId,
    ) -> Result<(), SourceError> {
        self.mutate(|record, _| {
            let current = exact_operation(record, source, operation)?;
            if current.session.as_ref() != Some(session)
                || !matches!(
                    current.phase,
                    Phase::Active | Phase::RecoveryPending | Phase::ActiveReceiptConflict
                )
            {
                return Err(owner_mismatch());
            }
            current.receipt = Some(receipt.clone());
            current.phase = Phase::ActiveReceiptConflict;
            Ok(())
        })
    }

    pub fn recovered_ticket(
        &self,
        source: &PluginId,
        operation: &OperationId,
        session: &SessionKey,
        receipt: &ReceiptId,
    ) -> Result<(), SourceError> {
        self.mutate(|record, legacy| {
            let current = exact_operation(record, source, operation)?;
            if legacy
                || current.media.is_some()
                || matches!(
                    current.phase,
                    Phase::Active
                        | Phase::RecoveryPending
                        | Phase::ActiveReceiptConflict
                        | Phase::RemoteEnded
                )
                || current.account != session.account
                || current
                    .session
                    .as_ref()
                    .is_some_and(|known| known != session)
            {
                return Err(owner_mismatch());
            }
            current.session = Some(session.clone());
            current.receipt = Some(receipt.clone());
            current.phase = Phase::CleanupPending;
            Ok(())
        })
    }

    pub fn rejected_before_allocation(
        &self,
        source: &PluginId,
        operation: &OperationId,
    ) -> Result<(), SourceError> {
        self.mutate(|record, legacy_media| {
            let current = exact_operation(record, source, operation)?;
            if current.phase != Phase::Allocating
                || current.session.is_some()
                || current.media.is_some()
                || legacy_media
            {
                return Err(owner_mismatch());
            }
            *record = None;
            Ok(())
        })
    }

    pub fn not_allocated(
        &self,
        source: &PluginId,
        account: Option<&AccountKey>,
        operation: &OperationId,
    ) -> Result<(), SourceError> {
        self.mutate(|record, _| {
            let current = exact_operation(record, source, operation)?;
            if current.account.as_ref() != account
                || current.session.is_some()
                || current.receipt.is_some()
                || current.media.is_some()
                || !matches!(current.phase, Phase::Allocating | Phase::Unknown)
            {
                return Err(owner_mismatch());
            }
            *record = None;
            Ok(())
        })
    }

    pub fn require_session(
        &self,
        source: &PluginId,
        session: &SessionKey,
    ) -> Result<SessionRecord, SourceError> {
        self.snapshot()?
            .filter(|record| {
                &record.source_id == source && record.session.as_ref() == Some(session)
            })
            .ok_or_else(owner_mismatch)
    }

    pub fn learn_session(
        &self,
        source: &PluginId,
        operation: &OperationId,
        session: &SessionKey,
    ) -> Result<(), SourceError> {
        self.mutate(|record, _| {
            let current = exact_operation(record, source, operation)?;
            if current.account != session.account
                || current
                    .session
                    .as_ref()
                    .is_some_and(|known| known != session)
            {
                return Err(owner_mismatch());
            }
            current.session = Some(session.clone());
            Ok(())
        })
    }

    pub fn reconcile(
        &self,
        source: PluginId,
        account: Option<AccountKey>,
        operation: OperationId,
        session: SessionKey,
    ) -> Result<(), SourceError> {
        if session.account != account {
            return Err(owner_mismatch());
        }
        self.mutate(|record, _| {
            if let Some(current) = record {
                if current.source_id != source
                    || current.account != account
                    || current.operation != operation
                    || current
                        .session
                        .as_ref()
                        .is_some_and(|known| known != &session)
                {
                    return Err(owner_mismatch());
                }
                current.session = Some(session);
                current.phase = Phase::Active;
                current.receipt = None;
            } else {
                *record = Some(SessionRecord {
                    source_id: source,
                    account,
                    operation,
                    session: Some(session),
                    receipt: None,
                    phase: Phase::Active,
                    media: None,
                    profile: None,
                });
            }
            Ok(())
        })
    }

    pub fn cleanup_pending(
        &self,
        source: &PluginId,
        session: &SessionKey,
    ) -> Result<(), SourceError> {
        self.mutate(|record, _| {
            let current = exact_session(record, source, session)?;
            current.phase = Phase::CleanupPending;
            Ok(())
        })
    }

    pub fn remote_ended(&self, source: &PluginId, session: &SessionKey) -> Result<(), SourceError> {
        self.mutate(|record, legacy_media| {
            let current = exact_session(record, source, session)?;
            if current.media.is_some() || legacy_media {
                current.phase = Phase::RemoteEnded;
            } else {
                *record = None;
            }
            Ok(())
        })
    }

    pub fn pin_media(
        &self,
        source: &PluginId,
        session: &SessionKey,
        pin: MediaPin,
    ) -> Result<(), SourceError> {
        validate_pin(&pin)?;
        self.mutate(|record, legacy_media| {
            let current = exact_session(record, source, session)?;
            if current.phase != Phase::Active || current.media.is_some() || legacy_media {
                return Err(in_use());
            }
            current.media = Some(pin);
            Ok(())
        })
    }

    pub fn release_media(
        &self,
        source: &PluginId,
        session: &SessionKey,
        lease_id: &str,
    ) -> Result<(), SourceError> {
        self.mutate(|record, _| {
            let current = exact_session(record, source, session)?;
            if current
                .media
                .as_ref()
                .is_none_or(|pin| pin.lease_id != lease_id)
            {
                return Err(owner_mismatch());
            }
            current.media = None;
            if current.phase == Phase::RemoteEnded {
                *record = None;
            }
            Ok(())
        })
    }

    fn mutate(
        &self,
        change: impl FnOnce(&mut Option<SessionRecord>, bool) -> Result<(), SourceError>,
    ) -> Result<(), SourceError> {
        let mut state = self.lock()?;
        if state.unavailable {
            return Err(unavailable());
        }
        let mut next = state.session.clone();
        change(&mut next, state.legacy_media)?;
        if next == state.session {
            return Ok(());
        }
        let revision = state
            .media_revision
            .checked_add(1)
            .ok_or_else(unavailable)?;
        write_document(&self.path, &next, state.legacy_media, revision)?;
        state.session = next;
        state.media_revision = revision;
        Ok(())
    }
}

fn exact_operation<'a>(
    record: &'a mut Option<SessionRecord>,
    source: &PluginId,
    operation: &OperationId,
) -> Result<&'a mut SessionRecord, SourceError> {
    record
        .as_mut()
        .filter(|record| &record.source_id == source && &record.operation == operation)
        .ok_or_else(owner_mismatch)
}

fn exact_session<'a>(
    record: &'a mut Option<SessionRecord>,
    source: &PluginId,
    session: &SessionKey,
) -> Result<&'a mut SessionRecord, SourceError> {
    record
        .as_mut()
        .filter(|record| &record.source_id == source && record.session.as_ref() == Some(session))
        .ok_or_else(owner_mismatch)
}

fn validate_pin(pin: &MediaPin) -> Result<(), SourceError> {
    if pin.lease_id.is_empty()
        || pin.lease_id.len() > 256
        || pin.lease_id.chars().any(char::is_control)
        || pin.runtime_epoch == 0
        || pin.attempt_id.is_empty()
        || pin.attempt_id.len() > 256
        || pin.attempt_id.chars().any(char::is_control)
        || pin.package_sha256.as_ref().is_some_and(|digest| {
            digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
    {
        return Err(owner_mismatch());
    }
    Ok(())
}

fn read_document(path: &Path) -> Result<Document, SourceError> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Document {
                version: 1,
                session: None,
                legacy_media: false,
                media_revision: 0,
            });
        }
        Err(_) => return Err(unavailable()),
    };
    let mut bytes = Vec::new();
    file.take(MAX_JOURNAL_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| unavailable())?;
    if bytes.len() as u64 > MAX_JOURNAL_BYTES {
        return Err(unavailable());
    }
    let document: Document = serde_json::from_slice(&bytes).map_err(|_| unavailable())?;
    if document.version != 1 {
        return Err(unavailable());
    }
    if let Some(record) = &document.session {
        if record
            .session
            .as_ref()
            .is_some_and(|session| session.account != record.account)
            || (record.media.is_some() && record.session.is_none())
            || (matches!(
                record.phase,
                Phase::Active
                    | Phase::AwaitingAcceptance
                    | Phase::RemoteEnded
                    | Phase::RecoveryPending
                    | Phase::ActiveReceiptConflict
            ) && record.session.is_none())
        {
            return Err(unavailable());
        }
        if let Some(pin) = &record.media {
            validate_pin(pin)?;
        }
    }
    Ok(document)
}

fn write_document(
    path: &Path,
    session: &Option<SessionRecord>,
    legacy_media: bool,
    media_revision: u64,
) -> Result<(), SourceError> {
    let encoded = serde_json::to_vec(&Document {
        version: 1,
        session: session.clone(),
        legacy_media,
        media_revision,
    })
    .map_err(|_| unavailable())?;
    if encoded.len() as u64 > MAX_JOURNAL_BYTES {
        return Err(unavailable());
    }
    let temporary = path.with_extension("json.tmp");
    let write = || -> std::io::Result<()> {
        fs::create_dir_all(path.parent().expect("journal parent"))?;
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(&encoded)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        #[cfg(unix)]
        fs::File::open(path.parent().expect("journal parent"))?.sync_all()?;
        Ok(())
    };
    write().map_err(|_| {
        SourceError::new(
            "session_journal_write_failed",
            "The provider session journal could not be saved",
        )
    })
}

fn unavailable() -> SourceError {
    SourceError::new(
        "session_journal_unavailable",
        "Provider session recovery state is unavailable",
    )
}
fn owner_mismatch() -> SourceError {
    SourceError::new(
        "session_owner_mismatch",
        "The request does not own this provider session",
    )
}
fn in_use() -> SourceError {
    SourceError::new(
        "session_in_use",
        "Resolve the existing provider session before continuing",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use opennow_plugin_api::provider::{AccountId, AuthorityId, SessionId};

    fn owner() -> (PluginId, AccountKey, OperationId, SessionKey) {
        let source = PluginId::new("org.example.provider").unwrap();
        let account = AccountKey {
            authority: AuthorityId::new("service").unwrap(),
            account: AccountId::new("account").unwrap(),
        };
        let operation = OperationId::new("allocation").unwrap();
        let session = SessionKey {
            account: Some(account.clone()),
            remote_id: SessionId::new("seat").unwrap(),
        };
        (source, account, operation, session)
    }

    #[test]
    fn allocation_intent_survives_restart_and_rejects_cross_source_cleanup() {
        let dir = tempfile::tempdir().unwrap();
        let journal = SessionJournal::open(dir.path());
        let (source, account, operation, session) = owner();
        journal
            .begin(source.clone(), Some(account.clone()), operation.clone())
            .unwrap();
        drop(journal);
        let journal = SessionJournal::open(dir.path());
        assert_eq!(journal.occupancy(), SessionOccupancy::Unknown);
        assert!(
            journal
                .begin(source.clone(), Some(account), operation.clone())
                .is_err()
        );
        journal
            .allocated(
                &source,
                &operation,
                session.clone(),
                ReceiptId::new("receipt").unwrap(),
            )
            .unwrap();
        journal.settled(&source, &operation, true).unwrap();
        assert!(
            journal
                .remote_ended(&PluginId::new("org.other.provider").unwrap(), &session)
                .is_err()
        );
        assert_eq!(journal.occupancy(), SessionOccupancy::InUse);
    }

    #[test]
    fn native_media_pin_survives_remote_stop_and_core_restart() {
        let dir = tempfile::tempdir().unwrap();
        let journal = SessionJournal::open(dir.path());
        let (source, account, operation, session) = owner();
        journal
            .reconcile(source.clone(), Some(account), operation, session.clone())
            .unwrap();
        journal
            .pin_media(
                &source,
                &session,
                MediaPin {
                    lease_id: "lease".into(),
                    package_sha256: Some("a".repeat(64)),
                    runtime_epoch: 1,
                    attempt_id: "attempt".into(),
                },
            )
            .unwrap();
        journal.remote_ended(&source, &session).unwrap();
        drop(journal);
        let journal = SessionJournal::open(dir.path());
        assert!(journal.source_in_use(&source));
        assert!(journal.release_media(&source, &session, "wrong").is_err());
        journal.release_media(&source, &session, "lease").unwrap();
        assert_eq!(journal.occupancy(), SessionOccupancy::Idle);
    }

    #[test]
    fn corrupt_journal_fails_closed_without_replacing_evidence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("provider-session-v2.json");
        fs::write(&path, "corrupt").unwrap();
        let journal = SessionJournal::open(dir.path());
        let (source, account, operation, _) = owner();
        assert_eq!(journal.occupancy(), SessionOccupancy::Unknown);
        assert!(journal.begin(source, Some(account), operation).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "corrupt");
    }

    #[test]
    fn stale_idle_probe_cannot_retire_a_new_pin_or_legacy_preparation() {
        let dir = tempfile::tempdir().unwrap();
        let journal = SessionJournal::open(dir.path());
        let (source, account, operation, session) = owner();
        journal
            .reconcile(source.clone(), Some(account), operation, session.clone())
            .unwrap();
        let observed = journal.media_revision().unwrap();
        journal
            .pin_media(
                &source,
                &session,
                MediaPin {
                    lease_id: "new-lease".into(),
                    package_sha256: Some("b".repeat(64)),
                    runtime_epoch: 1,
                    attempt_id: "attempt".into(),
                },
            )
            .unwrap();
        assert_eq!(
            journal
                .observe_native(false, true, observed)
                .unwrap_err()
                .code,
            "stale_media_observation"
        );
        assert!(
            journal
                .require_session(&source, &session)
                .unwrap()
                .media
                .is_some()
        );
        journal.remote_ended(&source, &session).unwrap();
        journal
            .observe_native(false, true, journal.media_revision().unwrap())
            .unwrap();
        assert_eq!(journal.occupancy(), SessionOccupancy::Idle);

        let source = PluginId::new(opennow_plugin_api::BUILTIN_GFN_ID).unwrap();
        journal
            .reconcile(
                source.clone(),
                session.account.clone(),
                OperationId::new("legacy").unwrap(),
                session.clone(),
            )
            .unwrap();
        journal.mark_legacy_media(&source, &session).unwrap();
        let observed = journal.media_revision().unwrap();
        journal.mark_legacy_media(&source, &session).unwrap();
        assert_eq!(
            journal
                .observe_native(false, true, observed)
                .unwrap_err()
                .code,
            "stale_media_observation"
        );
        journal.remote_ended(&source, &session).unwrap();
        assert_ne!(journal.occupancy(), SessionOccupancy::Idle);
        journal
            .observe_native(false, true, journal.media_revision().unwrap())
            .unwrap();
        assert_eq!(journal.occupancy(), SessionOccupancy::Idle);
    }

    #[test]
    fn no_allocation_evidence_cannot_clear_a_known_seat_or_other_account() {
        let dir = tempfile::tempdir().unwrap();
        let journal = SessionJournal::open(dir.path());
        let (source, account, operation, session) = owner();
        journal
            .begin(source.clone(), Some(account.clone()), operation.clone())
            .unwrap();
        journal.unknown(&source, &operation).unwrap();
        assert!(journal.not_allocated(&source, None, &operation).is_err());
        journal
            .allocated(
                &source,
                &operation,
                session,
                ReceiptId::new("receipt").unwrap(),
            )
            .unwrap();
        assert!(
            journal
                .not_allocated(&source, Some(&account), &operation)
                .is_err()
        );
        assert_ne!(journal.occupancy(), SessionOccupancy::Idle);
    }
}
