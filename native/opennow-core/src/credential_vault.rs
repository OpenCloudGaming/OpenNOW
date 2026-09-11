use crate::gfn::AuthSession;
use keyring::Entry;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const SERVICE_NAME: &str = "app.opennow.auth";

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
}

impl CredentialVault {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            metadata_path: data_dir.join("accounts.json"),
        }
    }

    pub fn save(&self, session: &AuthSession) -> Result<(), String> {
        let encoded = serde_json::to_string(session).map_err(|error| error.to_string())?;
        self.write_session_file(&session.user.user_id, encoded.as_bytes())?;
        if let Err(error) = credential(&session.user.user_id).and_then(|entry| {
            entry
                .set_password(&encoded)
                .map_err(|error| format!("OS credential store rejected the session: {error}"))
        }) {
            eprintln!(
                "auth: OS credential store skipped ({error}); session kept in the local store"
            );
        }

        let mut metadata = self.read_metadata();
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
            .map_err(|error| format!("Could not save account metadata: {error}"))
    }

    pub fn migrate_legacy_electron_sessions(&self) -> Result<usize, String> {
        if !self.read_metadata().accounts.is_empty() {
            return Ok(0);
        }
        let Some(parent) = self.metadata_path.parent() else {
            return Ok(0);
        };
        let legacy_path = parent.join("auth-state.json");
        let bytes = match fs::read(&legacy_path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
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

        let first_user_id = legacy.sessions[0].user.user_id.clone();
        let mut imported = 0;
        for session in &legacy.sessions {
            self.save(session)?;
            imported += 1;
        }
        let active = legacy
            .active_user_id
            .as_deref()
            .filter(|user_id| {
                legacy
                    .sessions
                    .iter()
                    .any(|item| item.user.user_id == *user_id)
            })
            .unwrap_or(&first_user_id);
        self.set_active(active)?;
        Ok(imported)
    }

    pub fn load_active(&self) -> Result<Option<AuthSession>, String> {
        let metadata = self.read_metadata();
        let candidates = self.discovered_user_ids();
        if candidates.is_empty() {
            return Ok(None);
        }
        let mut last_error = None;
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
        if let Some(session) = self.load_session_file(user_id)? {
            return Ok(Some(session));
        }
        let encoded = match credential(user_id)?.get_password() {
            Ok(value) => value,
            Err(keyring::Error::NoEntry) => return Ok(None),
            Err(error) => {
                return Err(format!(
                    "OS credential store could not restore the session: {error}"
                ));
            }
        };
        let session = decode_session(&encoded, user_id)?;
        if let Err(error) = self.write_session_file(user_id, encoded.as_bytes()) {
            eprintln!("auth: could not mirror keychain session to the local store: {error}");
        }
        Ok(Some(session))
    }

    pub fn list(&self) -> Result<Vec<Value>, String> {
        self.read_metadata()
            .accounts
            .into_iter()
            .map(|identity| serde_json::to_value(identity).map_err(|error| error.to_string()))
            .collect()
    }

    pub fn set_active(&self, user_id: &str) -> Result<(), String> {
        let mut metadata = self.read_metadata();
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
        match credential(user_id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(error) => {
                return Err(format!(
                    "OS credential store could not remove the session: {error}"
                ));
            }
        }
        let _ = fs::remove_file(self.session_file(user_id));
        let mut metadata = self.read_metadata();
        metadata.accounts.retain(|item| item.user_id != user_id);
        if metadata.active_user_id.as_deref() == Some(user_id) {
            metadata.active_user_id = metadata.accounts.first().map(|item| item.user_id.clone());
        }
        self.write_metadata(&metadata)
            .map_err(|error| error.to_string())
    }

    pub fn remove_all(&self) -> Result<(), String> {
        let user_ids = self
            .read_metadata()
            .accounts
            .into_iter()
            .map(|identity| identity.user_id)
            .collect::<Vec<_>>();
        for user_id in user_ids {
            self.remove(&user_id)?;
        }
        self.write_metadata(&Metadata::default())
            .map_err(|error| error.to_string())
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

    fn write_session_file(&self, user_id: &str, encoded: &[u8]) -> Result<(), String> {
        let directory = self.sessions_dir();
        fs::create_dir_all(&directory)
            .map_err(|error| format!("Could not create the local session store: {error}"))?;
        let path = self.session_file(user_id);
        let temporary = path.with_extension("json.tmp");
        write_private_file(&temporary, encoded)
            .and_then(|_| fs::rename(&temporary, &path))
            .map_err(|error| format!("Could not write the local session store: {error}"))
    }

    fn load_session_file(&self, user_id: &str) -> Result<Option<AuthSession>, String> {
        let bytes = match fs::read(self.session_file(user_id)) {
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

    fn discovered_user_ids(&self) -> Vec<String> {
        let mut ids = candidate_user_ids(&self.read_metadata());
        let Ok(entries) = fs::read_dir(self.sessions_dir()) else {
            return ids;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let Ok(bytes) = fs::read(&path) else {
                continue;
            };
            let Ok(session) = serde_json::from_slice::<AuthSession>(&bytes) else {
                continue;
            };
            if session.user.user_id.is_empty() {
                continue;
            }
            if !ids.iter().any(|id| id == &session.user.user_id) {
                ids.push(session.user.user_id);
            }
        }
        ids
    }

    fn read_metadata(&self) -> Metadata {
        fs::read_to_string(&self.metadata_path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
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
        fs::rename(temporary, &self.metadata_path)
    }
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
    let session = serde_json::from_str::<AuthSession>(encoded)
        .map_err(|error| format!("Saved session is invalid: {error}"))?;
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
        .map_err(|error| format!("Electron account state is invalid: {error}"))?;
    if legacy.sessions.is_empty() {
        if let Some(session) = legacy.session.take() {
            legacy.sessions.push(session);
        }
    }
    legacy
        .sessions
        .retain(|session| !session.user.user_id.trim().is_empty());
    legacy
        .sessions
        .sort_by(|left, right| left.user.user_id.cmp(&right.user.user_id));
    legacy
        .sessions
        .dedup_by(|left, right| left.user.user_id == right.user.user_id);
    Ok(legacy)
}

fn credential(user_id: &str) -> Result<Entry, String> {
    Entry::new(SERVICE_NAME, &format!("session:{user_id}"))
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

    fn sample_identity(user_id: &str) -> SavedIdentity {
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
        let vault = CredentialVault::new(path.clone());
        assert!(vault.load_active().unwrap().is_none());
        let _ = fs::remove_dir_all(path);
    }

    fn sample_session(user_id: &str) -> AuthSession {
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
    fn save_and_load_roundtrip_uses_local_session_file() {
        let path = std::env::temp_dir().join(format!(
            "opennow-vault-file-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        let vault = CredentialVault::new(path.clone());
        vault.save(&sample_session("user-file")).unwrap();
        let loaded = vault
            .load_active()
            .unwrap()
            .expect("session file should restore");
        assert_eq!(loaded.user.user_id, "user-file");
        assert!(path.join("sessions").join("user-file.json").exists());
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
        let vault = CredentialVault::new(path.clone());
        vault
            .write_metadata(&Metadata {
                active_user_id: None,
                accounts: vec![sample_identity("user-a"), sample_identity("user-b")],
            })
            .unwrap();
        let metadata = vault.read_metadata();
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
