use crate::gfn::AuthSession;
use keyring::Entry;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[cfg(any(windows, test))]
mod encrypted_secret_store;

const SERVICE_NAME: &str = "app.opennow.auth";
#[cfg(windows)]
const KEY_SERVICE_NAME: &str = "app.opennow.auth.session-keys";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedIdentity {
    user_id: String,
    display_name: String,
    email: Option<String>,
    avatar_url: Option<String>,
    membership_tier: String,
    provider_code: String,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Metadata {
    active_user_id: Option<String>,
    accounts: Vec<SavedIdentity>,
    #[serde(default)]
    suppressed: Vec<String>,
    #[serde(default)]
    legacy_discarded: Vec<String>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyAuthState {
    #[serde(default)]
    sessions: Vec<AuthSession>,
    session: Option<AuthSession>,
    active_user_id: Option<String>,
}

pub struct CredentialVault {
    metadata_path: PathBuf,
    store: Box<dyn SecretStore>,
    warnings: Mutex<std::collections::BTreeMap<String, String>>,
    suppressed: Mutex<Vec<String>>,
}

trait SecretStore: Send + Sync {
    fn get(&self, user_id: &str) -> Result<Option<String>, String>;
    fn set(&self, user_id: &str, encoded: &str) -> Result<(), String>;
    fn delete(&self, user_id: &str) -> Result<(), String>;
}

struct OsSecretStore {
    service: &'static str,
}

impl SecretStore for OsSecretStore {
    fn get(&self, user_id: &str) -> Result<Option<String>, String> {
        match credential(self.service, user_id)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err("OS credential store is unavailable or locked".into()),
        }
    }

    fn set(&self, user_id: &str, encoded: &str) -> Result<(), String> {
        credential(self.service, user_id)?
            .set_password(encoded)
            .map_err(|_| "OS credential store could not save the session".into())
    }

    fn delete(&self, user_id: &str) -> Result<(), String> {
        match credential(self.service, user_id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err("OS credential store could not remove the session".into()),
        }
    }
}

impl CredentialVault {
    #[cfg(test)]
    pub(crate) fn memory(data_dir: PathBuf) -> Self {
        let mut vault = Self::new(data_dir);
        vault.store = Box::<MemorySecretStore>::default();
        vault
    }
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            metadata_path: data_dir.join("accounts.json"),
            #[cfg(windows)]
            store: Box::new(encrypted_secret_store::EncryptedSecretStore::new(
                data_dir.join("secure-sessions"),
                Box::new(OsSecretStore {
                    service: KEY_SERVICE_NAME,
                }),
                Box::new(OsSecretStore {
                    service: SERVICE_NAME,
                }),
            )),
            #[cfg(not(windows))]
            store: Box::new(OsSecretStore {
                service: SERVICE_NAME,
            }),
            warnings: Mutex::new(std::collections::BTreeMap::new()),
            suppressed: Mutex::new(Vec::new()),
        }
    }

    pub fn save(&self, session: &AuthSession) -> Result<(), String> {
        let encoded = serde_json::to_string(session).map_err(|error| error.to_string())?;
        let mut metadata = self.read_metadata()?;
        self.store.set(&session.user.user_id, &encoded)?;
        if self.store.get(&session.user.user_id)?.as_deref() != Some(&encoded) {
            return Err("OS credential store verification failed".into());
        }
        metadata.suppressed.retain(|id| id != &session.user.user_id);
        metadata.active_user_id = Some(session.user.user_id.clone());
        let identity = SavedIdentity {
            user_id: session.user.user_id.clone(),
            display_name: session.user.display_name.clone(),
            email: session.user.email.clone(),
            avatar_url: session.user.avatar_url.clone(),
            membership_tier: session.user.membership_tier.clone(),
            provider_code: session.provider.code.clone(),
        };
        if let Some(existing) = metadata
            .accounts
            .iter_mut()
            .find(|item| item.user_id == identity.user_id)
        {
            *existing = identity;
        } else {
            metadata.accounts.push(identity);
        }
        self.write_metadata(&metadata)
            .map_err(|error| format!("Could not save account metadata: {error}"))?;
        self.suppressed
            .lock()
            .expect("vault suppression poisoned")
            .retain(|id| id != &session.user.user_id);
        if let Err(error) = self.remove_session_file(&session.user.user_id) {
            self.warn(format!("cleanup:{}", session.user.user_id), error);
        } else {
            self.clear_warning(&format!("cleanup:{}", session.user.user_id));
        }
        self.clear_warning(&format!("migration:{}", session.user.user_id));
        Ok(())
    }

    pub fn migrate_legacy_electron_sessions(&self) -> Result<usize, String> {
        let metadata = self.read_metadata()?;
        let mut imported = 0;
        let mut failures = Vec::new();
        for user_id in &metadata.suppressed {
            if let Err(error) = self.remove(user_id) {
                failures.push(error);
            }
        }
        for user_id in self.discovered_user_ids()? {
            match self.load(&user_id) {
                Ok(Some(session)) if self.durable(&session) => imported += 1,
                Ok(Some(_)) => failures.push("Legacy credential migration is pending".into()),
                Ok(None) => {}
                Err(error) => failures.push(error),
            }
        }
        if self.has_discovery_warning() {
            failures.push("Some legacy credential sources need recovery".into());
        }
        if let Some(active) = metadata.active_user_id.as_deref() {
            if !metadata.suppressed.iter().any(|id| id == active) {
                self.set_active(active)?;
            }
        }
        let Some(parent) = self.metadata_path.parent() else {
            return Ok(0);
        };
        let legacy_path = parent.join("auth-state.json");
        let bytes = match read_bounded(&legacy_path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return if failures.is_empty() {
                    self.clear_warning("migration");
                    Ok(imported)
                } else {
                    Err(failures.join("; "))
                };
            }
            Err(error) => {
                return Err(format!(
                    "Could not read the Electron account state: {error}"
                ));
            }
        };
        if bytes.len() > 4 * 1024 * 1024 {
            return Err("Electron account state exceeds the migration size limit".to_owned());
        }
        let legacy = parse_legacy_auth_state(&bytes)?;
        if legacy.sessions.is_empty() {
            return Ok(0);
        }

        for session in &legacy.sessions {
            if metadata.suppressed.contains(&session.user.user_id)
                || metadata.legacy_discarded.contains(&session.user.user_id)
            {
                continue;
            }
            let result = self.store.get(&session.user.user_id).and_then(|stored| {
                let selected = match stored {
                    Some(encoded) => decode_session(&encoded, &session.user.user_id)?,
                    None => session.clone(),
                };
                self.save(&selected)
            });
            match result {
                Ok(()) => imported += 1,
                Err(error) => failures.push(error),
            }
        }
        if !failures.is_empty() {
            self.warn(
                "migration".into(),
                "Legacy credential migration is pending; recoverable source data was retained"
                    .into(),
            );
            return Err(failures.join("; "));
        }
        if let Some(active) = metadata.active_user_id.or(legacy.active_user_id) {
            if !metadata.suppressed.contains(&active) {
                self.set_active(&active)?;
            }
        }
        fs::remove_file(&legacy_path)
            .map_err(|_| "Legacy credential cleanup is pending".to_owned())?;
        self.clear_warning("migration");
        Ok(imported)
    }

    pub fn load_active(&self) -> Result<Option<AuthSession>, String> {
        let metadata = self.read_metadata()?;
        let candidates = self.discovered_user_ids()?;
        let mut last_error = self
            .has_discovery_warning()
            .then(|| "Legacy credential sources need recovery".to_owned());
        for user_id in &candidates {
            match self.load(user_id) {
                Ok(Some(session)) => {
                    if metadata.active_user_id.as_deref() != Some(user_id.as_str()) {
                        if let Err(error) = self.set_active(user_id) {
                            eprintln!(
                                "auth: recovered account could not be marked active: {error}"
                            );
                        }
                    }
                    return Ok(Some(session));
                }
                Ok(None) => {}
                Err(error) => last_error = Some(error),
            }
        }
        match last_error {
            Some(error) => Err(error),
            None => Ok(None),
        }
    }

    pub fn load(&self, user_id: &str) -> Result<Option<AuthSession>, String> {
        if self
            .suppressed
            .lock()
            .expect("vault suppression poisoned")
            .iter()
            .any(|id| id == user_id)
        {
            return Ok(None);
        }
        if self
            .read_metadata()?
            .suppressed
            .iter()
            .any(|id| id == user_id)
        {
            return Ok(None);
        }
        match self.store.get(user_id) {
            Ok(Some(encoded)) => {
                let session = decode_session(&encoded, user_id)?;
                if self.session_file(user_id).exists() {
                    if let Err(error) = self.save(&session) {
                        self.warn(
                            format!("cleanup:{user_id}"),
                            format!("Legacy credential cleanup is pending: {error}"),
                        );
                    }
                }
                self.clear_warning(&format!("migration:{user_id}"));
                return Ok(Some(session));
            }
            Err(error) => {
                if let Some(session) = self.load_legacy_session(user_id)? {
                    self.warn(format!("migration:{user_id}"), "Legacy credential migration is pending; this session is not securely persisted".into());
                    return Ok(Some(session));
                }
                return Err(error);
            }
            Ok(None) => {}
        }
        let session = self.load_legacy_session(user_id)?;
        if let Some(session) = &session {
            if let Err(error) = self.save(session) {
                self.warn(
                    format!("migration:{user_id}"),
                    format!("Legacy credential migration is pending: {error}"),
                );
            }
        }
        Ok(session)
    }

    pub fn durable(&self, session: &AuthSession) -> bool {
        self.store
            .get(&session.user.user_id)
            .ok()
            .flatten()
            .is_some_and(|encoded| serde_json::to_string(session).ok().as_deref() == Some(&encoded))
    }

    pub fn warnings(&self) -> Vec<String> {
        self.warnings
            .lock()
            .expect("vault warnings poisoned")
            .values()
            .cloned()
            .collect()
    }

    fn warn(&self, key: String, warning: String) {
        let mut warnings = self.warnings.lock().expect("vault warnings poisoned");
        if warnings.len() < 16 || warnings.contains_key(&key) {
            warnings.insert(key, warning);
        }
    }

    fn clear_warning(&self, key: &str) {
        self.warnings
            .lock()
            .expect("vault warnings poisoned")
            .remove(key);
    }

    fn has_discovery_warning(&self) -> bool {
        self.warnings
            .lock()
            .expect("vault warnings poisoned")
            .keys()
            .any(|key| key.starts_with("source:"))
    }

    pub fn list(&self) -> Result<Vec<Value>, String> {
        self.read_metadata()?
            .accounts
            .into_iter()
            .map(|identity| serde_json::to_value(identity).map_err(|error| error.to_string()))
            .collect()
    }

    pub fn set_active(&self, user_id: &str) -> Result<(), String> {
        let mut metadata = self.read_metadata()?;
        if !metadata
            .accounts
            .iter()
            .any(|identity| identity.user_id == user_id)
        {
            return Err("Saved account not found".to_owned());
        }
        metadata.active_user_id = Some(user_id.to_owned());
        self.write_metadata(&metadata)
            .map_err(|error| error.to_string())
    }

    pub fn remove(&self, user_id: &str) -> Result<(), String> {
        self.suppressed
            .lock()
            .expect("vault suppression poisoned")
            .push(user_id.to_owned());
        let suppression = self.read_metadata().and_then(|mut metadata| {
            if !metadata.suppressed.iter().any(|id| id == user_id) {
                metadata.suppressed.push(user_id.to_owned());
            }
            if !metadata.legacy_discarded.iter().any(|id| id == user_id) {
                metadata.legacy_discarded.push(user_id.to_owned());
            }
            metadata.accounts.retain(|item| item.user_id != user_id);
            if metadata.active_user_id.as_deref() == Some(user_id) {
                metadata.active_user_id =
                    metadata.accounts.first().map(|item| item.user_id.clone());
            }
            self.write_metadata(&metadata)
                .map_err(|error| error.to_string())
        });
        let secret = self.store.delete(user_id);
        let legacy = self.remove_session_file(user_id);
        let result = suppression.and(secret).and(legacy);
        if result.is_err() {
            self.warn(
                format!("cleanup:{user_id}"),
                "Removed account credential cleanup is pending".into(),
            );
        } else {
            self.clear_warning(&format!("cleanup:{user_id}"));
            self.clear_warning(&format!("migration:{user_id}"));
        }
        result
    }

    pub fn remove_all(&self) -> Result<(), String> {
        let metadata = self.read_metadata()?;
        let mut user_ids = self.discovered_user_ids()?;
        user_ids.extend(metadata.suppressed);
        let mut failures = Vec::new();
        for user_id in user_ids {
            if let Err(error) = self.remove(&user_id) {
                failures.push(error);
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }

    fn data_dir(&self) -> &Path {
        self.metadata_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
    }

    fn sessions_dir(&self) -> PathBuf {
        self.data_dir().join("sessions")
    }

    fn session_file(&self, user_id: &str) -> PathBuf {
        self.sessions_dir()
            .join(format!("{}.json", sanitize_user_id(user_id)))
    }

    fn remove_session_file(&self, user_id: &str) -> Result<(), String> {
        if self.load_session_file(user_id)?.is_none() {
            return Ok(());
        }
        fs::remove_file(self.session_file(user_id))
            .map_err(|_| "Legacy credential cleanup is pending".into())
    }

    fn load_session_file(&self, user_id: &str) -> Result<Option<AuthSession>, String> {
        let bytes = match read_bounded(&self.session_file(user_id)) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(format!("Could not read the local session store: {error}"));
            }
        };
        let encoded = String::from_utf8(bytes)
            .map_err(|error| format!("Saved session is invalid: {error}"))?;
        Ok(Some(decode_session(&encoded, user_id)?))
    }

    fn load_legacy_session(&self, user_id: &str) -> Result<Option<AuthSession>, String> {
        if self
            .read_metadata()?
            .legacy_discarded
            .iter()
            .any(|id| id == user_id)
        {
            return Ok(None);
        }
        if let Some(session) = self.load_session_file(user_id)? {
            return Ok(Some(session));
        }
        match read_bounded(&self.data_dir().join("auth-state.json")) {
            Ok(bytes) => Ok(parse_legacy_auth_state(&bytes)?
                .sessions
                .into_iter()
                .find(|session| session.user.user_id == user_id)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err("Legacy account state could not be read".into()),
        }
    }

    fn discovered_user_ids(&self) -> Result<Vec<String>, String> {
        let metadata = self.read_metadata()?;
        let mut ids = candidate_user_ids(&metadata);
        self.warnings
            .lock()
            .expect("vault warnings poisoned")
            .retain(|key, _| !key.starts_with("source:"));
        match read_bounded(&self.data_dir().join("auth-state.json")) {
            Ok(bytes) => match parse_legacy_auth_state(&bytes) {
                Ok(legacy) => {
                    for session in legacy.sessions {
                        if !ids.contains(&session.user.user_id) {
                            ids.push(session.user.user_id);
                        }
                    }
                }
                Err(error) => self.warn("source:legacy".into(), error),
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => self.warn(
                "source:legacy".into(),
                "Legacy account state could not be read".into(),
            ),
        }
        ids.retain(|id| !metadata.suppressed.contains(id));
        let entries = match fs::read_dir(self.sessions_dir()) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(ids),
            Err(_) => {
                self.warn(
                    "source:directory".into(),
                    "Legacy session directory could not be read".into(),
                );
                return Ok(ids);
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => {
                    self.warn(
                        "source:directory".into(),
                        "Legacy session entry could not be read".into(),
                    );
                    continue;
                }
            };
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let session = match read_bounded(&path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<AuthSession>(&bytes).ok())
            {
                Some(session) => session,
                None => {
                    self.warn(format!("source:{}", path.display()), "A legacy credential file is invalid or unavailable; it was retained for recovery".into());
                    continue;
                }
            };
            if session.user.user_id.is_empty() || self.session_file(&session.user.user_id) != path {
                self.warn(
                    format!("source:{}", path.display()),
                    "Legacy credential filename does not match its identity".into(),
                );
                continue;
            }
            if !ids.iter().any(|id| id == &session.user.user_id) {
                ids.push(session.user.user_id);
            }
        }
        ids.retain(|id| !metadata.suppressed.contains(id));
        Ok(ids)
    }

    fn read_metadata(&self) -> Result<Metadata, String> {
        match read_bounded(&self.metadata_path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|_| "Account metadata is invalid; recovery is required".into()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Metadata::default()),
            Err(_) => Err("Account metadata could not be read; recovery is required".into()),
        }
    }

    fn write_metadata(&self, metadata: &Metadata) -> io::Result<()> {
        let parent = self
            .metadata_path
            .parent()
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let temporary = self.metadata_path.with_extension("json.tmp");
        let data = serde_json::to_vec_pretty(metadata).map_err(io::Error::other)?;
        write_private_file(&temporary, &data)?;
        fs::rename(temporary, &self.metadata_path)?;
        #[cfg(unix)]
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    }
}

#[cfg(test)]
#[derive(Default)]
struct MemorySecretStore {
    values: Mutex<std::collections::HashMap<String, String>>,
    unavailable: bool,
    fail_user: Option<String>,
}

#[cfg(test)]
impl SecretStore for MemorySecretStore {
    fn get(&self, user_id: &str) -> Result<Option<String>, String> {
        if self.unavailable || self.fail_user.as_deref() == Some(user_id) {
            return Err("locked".into());
        }
        Ok(self.values.lock().unwrap().get(user_id).cloned())
    }
    fn set(&self, user_id: &str, encoded: &str) -> Result<(), String> {
        if self.unavailable || self.fail_user.as_deref() == Some(user_id) {
            return Err("locked".into());
        }
        self.values
            .lock()
            .unwrap()
            .insert(user_id.into(), encoded.into());
        Ok(())
    }
    fn delete(&self, user_id: &str) -> Result<(), String> {
        if self.unavailable || self.fail_user.as_deref() == Some(user_id) {
            return Err("locked".into());
        }
        self.values.lock().unwrap().remove(user_id);
        Ok(())
    }
}

fn read_bounded(path: &Path) -> io::Result<Vec<u8>> {
    use std::io::Read as _;
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(io::Error::other("Account document exceeds size limit"));
    }
    Ok(bytes)
}

fn sanitize_user_id(user_id: &str) -> String {
    user_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn decode_session(encoded: &str, user_id: &str) -> Result<AuthSession, String> {
    if encoded.len() > 4 * 1024 * 1024 || user_id.trim().is_empty() {
        return Err("Saved credential exceeds size or identity limits".into());
    }
    let session = serde_json::from_str::<AuthSession>(encoded)
        .map_err(|_| "Saved session is invalid".to_owned())?;
    if session.user.user_id != user_id {
        return Err("Saved credential identity does not match account metadata".to_owned());
    }
    Ok(session)
}

fn candidate_user_ids(metadata: &Metadata) -> Vec<String> {
    let mut ids = Vec::new();
    if let Some(active) = metadata
        .active_user_id
        .as_deref()
        .filter(|user_id| !user_id.is_empty())
    {
        ids.push(active.to_owned());
    }
    for account in &metadata.accounts {
        if account.user_id.is_empty() {
            continue;
        }
        if !ids.iter().any(|id| id == &account.user_id) {
            ids.push(account.user_id.clone());
        }
    }
    ids
}

fn parse_legacy_auth_state(bytes: &[u8]) -> Result<LegacyAuthState, String> {
    let mut legacy = serde_json::from_slice::<LegacyAuthState>(bytes)
        .map_err(|_| "Legacy account state is invalid".to_owned())?;
    if let Some(session) = legacy.session.take() {
        legacy.sessions.push(session);
    }
    if legacy
        .sessions
        .iter()
        .any(|session| session.user.user_id.trim().is_empty())
    {
        return Err("Legacy account identity is invalid".into());
    }
    legacy
        .sessions
        .sort_by(|left, right| left.user.user_id.cmp(&right.user.user_id));
    for pair in legacy.sessions.windows(2) {
        if pair[0].user.user_id == pair[1].user.user_id
            && serde_json::to_value(&pair[0]).ok() != serde_json::to_value(&pair[1]).ok()
        {
            return Err(
                "Legacy account state contains conflicting grants; recovery is required".into(),
            );
        }
    }
    legacy
        .sessions
        .dedup_by(|left, right| left.user.user_id == right.user.user_id);
    Ok(legacy)
}

fn credential(service: &str, user_id: &str) -> Result<Entry, String> {
    Entry::new(service, &format!("session:{user_id}"))
        .map_err(|error| format!("OS credential store is unavailable: {error}"))
}

#[cfg(unix)]
fn write_private_file(path: &Path, data: &[u8]) -> io::Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(data)?;
    file.sync_all()
}

#[cfg(not(unix))]
fn write_private_file(path: &Path, data: &[u8]) -> io::Result<()> {
    fs::write(path, data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn secure_restore_survives_failed_legacy_cleanup_and_retries() {
        let directory = tempfile::tempdir().unwrap();
        let vault = CredentialVault::memory(directory.path().into());
        let session = sample_session("selected");
        vault.save(&session).unwrap();
        fs::create_dir(directory.path().join("sessions")).unwrap();
        let mut legacy = session.clone();
        legacy.tokens.access_token = "superseded-legacy-grant".into();
        let legacy_bytes = serde_json::to_vec(&legacy).unwrap();
        fs::write(vault.session_file("selected"), &legacy_bytes).unwrap();
        let blocked_metadata = directory.path().join("accounts.json.tmp");
        fs::create_dir(&blocked_metadata).unwrap();

        let restored = vault.load_active().unwrap().unwrap();
        assert_eq!(restored.tokens.access_token, session.tokens.access_token);
        assert_eq!(
            fs::read(vault.session_file("selected")).unwrap(),
            legacy_bytes
        );
        assert!(!vault.warnings().is_empty());

        fs::remove_dir(blocked_metadata).unwrap();
        let restored = vault.load_active().unwrap().unwrap();
        assert_eq!(restored.tokens.access_token, session.tokens.access_token);
        assert!(!vault.session_file("selected").exists());
        assert!(vault.warnings().is_empty());
    }

    #[test]
    fn migration_preserves_selection_and_conflicting_sources() {
        let directory = tempfile::tempdir().unwrap();
        let vault = CredentialVault::memory(directory.path().into());
        vault.save(&sample_session("selected")).unwrap();
        fs::create_dir(directory.path().join("sessions")).unwrap();
        fs::write(
            vault.session_file("later"),
            serde_json::to_vec(&sample_session("later")).unwrap(),
        )
        .unwrap();
        vault.migrate_legacy_electron_sessions().unwrap();
        assert_eq!(
            vault.load_active().unwrap().unwrap().user.user_id,
            "selected"
        );
        let mut other = sample_session("selected");
        other.tokens.access_token = "other-grant".into();
        let bytes =
            serde_json::to_vec(&json!({"sessions":[sample_session("selected")],"session":other}))
                .unwrap();
        let source = directory.path().join("auth-state.json");
        fs::write(&source, &bytes).unwrap();
        assert!(vault.migrate_legacy_electron_sessions().is_err());
        assert_eq!(fs::read(source).unwrap(), bytes);
    }

    #[test]
    fn damaged_legacy_sources_do_not_strand_other_restorable_accounts() {
        let directory = tempfile::tempdir().unwrap();
        let vault = CredentialVault::memory(directory.path().into());
        vault.save(&sample_session("selected")).unwrap();
        fs::create_dir(directory.path().join("sessions")).unwrap();
        fs::write(vault.session_file("damaged"), "invalid credential").unwrap();
        fs::write(
            vault.session_file("recoverable"),
            serde_json::to_vec(&sample_session("recoverable")).unwrap(),
        )
        .unwrap();
        assert!(vault.migrate_legacy_electron_sessions().is_err());
        assert_eq!(
            vault.load_active().unwrap().unwrap().user.user_id,
            "selected"
        );
        assert!(vault.durable(&sample_session("recoverable")));
        assert!(vault.session_file("damaged").exists());
        fs::write(
            directory.path().join("auth-state.json"),
            "damaged legacy document",
        )
        .unwrap();
        assert_eq!(
            vault.load_active().unwrap().unwrap().user.user_id,
            "selected"
        );
        assert!(vault.has_discovery_warning());
    }

    #[test]
    fn unavailable_store_retains_legacy_data_and_new_saves_write_no_credentials() {
        let directory = tempfile::tempdir().unwrap();
        let mut vault = CredentialVault::memory(directory.path().into());
        vault.store = Box::new(MemorySecretStore {
            unavailable: true,
            ..Default::default()
        });
        let session = sample_session("legacy");
        let legacy = serde_json::to_vec(&json!({"sessions":[session]})).unwrap();
        let source = directory.path().join("auth-state.json");
        fs::write(&source, &legacy).unwrap();
        assert!(vault.migrate_legacy_electron_sessions().is_err());
        assert_eq!(vault.load_active().unwrap().unwrap().user.user_id, "legacy");
        assert_eq!(fs::read(&source).unwrap(), legacy);
        assert!(vault.save(&sample_session("new")).is_err());
        assert!(!directory.path().join("sessions").exists());
        assert!(!directory.path().join("accounts.json").exists());
        vault.store = Box::<MemorySecretStore>::default();
        assert!(vault.migrate_legacy_electron_sessions().unwrap() > 0);
        assert!(!source.exists());
        assert!(vault.warnings().is_empty());
        assert_eq!(vault.load_active().unwrap().unwrap().user.user_id, "legacy");
        assert_eq!(vault.migrate_legacy_electron_sessions().unwrap(), 1);
    }

    #[test]
    fn partial_migration_keeps_source_and_does_not_skip_later_accounts() {
        let directory = tempfile::tempdir().unwrap();
        let mut vault = CredentialVault::memory(directory.path().into());
        vault.store = Box::new(MemorySecretStore {
            fail_user: Some("b".into()),
            ..Default::default()
        });
        vault.save(&sample_session("existing")).unwrap();
        let source = directory.path().join("auth-state.json");
        let bytes = serde_json::to_vec(
            &json!({"sessions":[sample_session("a"),sample_session("b"),sample_session("c")]}),
        )
        .unwrap();
        fs::write(&source, &bytes).unwrap();
        assert!(vault.migrate_legacy_electron_sessions().is_err());
        assert!(vault.durable(&sample_session("a")));
        assert!(vault.durable(&sample_session("c")));
        assert_eq!(fs::read(source).unwrap(), bytes);
    }

    #[test]
    fn valid_secure_entry_wins_and_metadata_failure_preserves_source() {
        let directory = tempfile::tempdir().unwrap();
        let vault = CredentialVault::memory(directory.path().into());
        let session = sample_session("user");
        vault.save(&session).unwrap();
        fs::create_dir(directory.path().join("sessions")).unwrap();
        let mut stale = session.clone();
        stale.tokens.access_token = "obsolete".into();
        let source = vault.session_file("user");
        fs::write(&source, serde_json::to_vec(&stale).unwrap()).unwrap();
        assert_eq!(
            vault.load("user").unwrap().unwrap().tokens.access_token,
            "access"
        );
        assert!(!source.exists());
        fs::write(&source, serde_json::to_vec(&stale).unwrap()).unwrap();
        fs::write(directory.path().join("accounts.json"), "corrupt metadata").unwrap();
        assert!(vault.save(&session).is_err());
        assert!(vault.list().is_err());
        assert!(source.exists());
        assert_eq!(
            fs::read_to_string(directory.path().join("accounts.json")).unwrap(),
            "corrupt metadata"
        );
    }

    #[test]
    fn failed_deletion_is_suppressed_and_old_legacy_grants_never_return() {
        let directory = tempfile::tempdir().unwrap();
        let mut vault = CredentialVault::memory(directory.path().into());
        let session = sample_session("user");
        vault.save(&session).unwrap();
        fs::write(
            directory.path().join("auth-state.json"),
            serde_json::to_vec(&json!({"sessions":[session]})).unwrap(),
        )
        .unwrap();
        vault.store = Box::new(MemorySecretStore {
            unavailable: true,
            ..Default::default()
        });
        assert!(vault.remove("user").is_err());
        assert!(vault.load_active().unwrap().is_none());
        assert!(!vault.warnings().is_empty());
        vault.store = Box::<MemorySecretStore>::default();
        vault.remove("user").unwrap();
        assert!(vault.warnings().is_empty());
        let mut restarted = CredentialVault::memory(directory.path().into());
        assert!(restarted.load("user").unwrap().is_none());
        restarted.save(&sample_session("user")).unwrap();
        restarted.store = Box::new(MemorySecretStore {
            unavailable: true,
            ..Default::default()
        });
        assert!(restarted.load("user").is_err());
    }

    #[test]
    fn mismatched_legacy_identity_is_not_imported_or_deleted() {
        let directory = tempfile::tempdir().unwrap();
        let vault = CredentialVault::memory(directory.path().into());
        fs::create_dir(directory.path().join("sessions")).unwrap();
        let source = vault.session_file("other-user");
        fs::write(
            &source,
            serde_json::to_vec(&sample_session("real-user")).unwrap(),
        )
        .unwrap();
        assert!(vault.migrate_legacy_electron_sessions().is_err());
        assert!(source.exists());
        assert!(vault.load("other-user").is_err());
    }

    pub(super) fn sample_identity(user_id: &str) -> SavedIdentity {
        SavedIdentity {
            user_id: user_id.to_owned(),
            display_name: user_id.to_owned(),
            email: None,
            avatar_url: None,
            membership_tier: "FREE".to_owned(),
            provider_code: "NVIDIA".to_owned(),
        }
    }

    #[test]
    fn missing_metadata_has_no_active_session() {
        let path = std::env::temp_dir().join(format!("opennow-vault-{}", std::process::id()));
        let vault = CredentialVault::memory(path.clone());
        assert!(vault.load_active().unwrap().is_none());
        let _ = fs::remove_dir_all(path);
    }

    pub(super) fn sample_session(user_id: &str) -> AuthSession {
        serde_json::from_value(serde_json::json!({
            "provider": {
                "idpId": "idp", "code": "NVIDIA", "displayName": "NVIDIA",
                "streamingServiceUrl": "https://example.invalid/", "priority": 0
            },
            "tokens": {
                "accessToken": "access", "refreshToken": "refresh",
                "expiresAt": 9_999_999_999_999u64, "authClientId": "client"
            },
            "user": {
                "userId": user_id, "displayName": "Player",
                "membershipTier": "free"
            }
        }))
        .unwrap()
    }

    #[test]
    fn save_and_load_roundtrip_uses_secure_store_without_plaintext_mirror() {
        let path = std::env::temp_dir().join(format!(
            "opennow-vault-file-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        let vault = CredentialVault::memory(path.clone());
        vault.save(&sample_session("user-file")).unwrap();
        let loaded = vault
            .load_active()
            .unwrap()
            .expect("secure session should restore");
        assert_eq!(loaded.user.user_id, "user-file");
        assert!(!path.join("sessions").join("user-file.json").exists());
        assert!(
            !fs::read_to_string(path.join("accounts.json"))
                .unwrap()
                .contains("accessToken")
        );
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn load_active_falls_back_to_first_account_when_active_user_id_is_missing() {
        let path = std::env::temp_dir().join(format!(
            "opennow-vault-fallback-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        let vault = CredentialVault::memory(path.clone());
        vault
            .write_metadata(&Metadata {
                active_user_id: None,
                accounts: vec![sample_identity("user-a"), sample_identity("user-b")],
                suppressed: Vec::new(),
                legacy_discarded: Vec::new(),
            })
            .unwrap();
        let metadata = vault.read_metadata().unwrap();
        assert_eq!(metadata.active_user_id, None);
        assert_eq!(metadata.accounts.len(), 2);
        assert_eq!(
            candidate_user_ids(&metadata),
            vec!["user-a".to_string(), "user-b".to_string()]
        );
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn parses_current_and_legacy_electron_account_documents() {
        let session = json!({
            "provider": {
                "idpId": "idp", "code": "NVIDIA", "displayName": "NVIDIA",
                "streamingServiceUrl": "https://example.invalid/", "priority": 0
            },
            "tokens": {
                "accessToken": "access", "refreshToken": "refresh",
                "expiresAt": 1234, "authClientId": "client"
            },
            "user": {
                "userId": "user-1", "displayName": "Player",
                "membershipTier": "free"
            }
        });
        let current = serde_json::to_vec(&json!({
            "sessions": [session.clone(), session.clone()],
            "activeUserId": "user-1"
        }))
        .unwrap();
        let parsed = parse_legacy_auth_state(&current).unwrap();
        assert_eq!(parsed.sessions.len(), 1);
        assert_eq!(parsed.active_user_id.as_deref(), Some("user-1"));

        let legacy = serde_json::to_vec(&json!({"session": session})).unwrap();
        assert_eq!(parse_legacy_auth_state(&legacy).unwrap().sessions.len(), 1);
    }
}
