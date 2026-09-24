use super::SecretStore;
use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Read as _, Write as _};
use std::path::{Path, PathBuf};

const MAX_SESSION_BYTES: usize = 4 * 1024 * 1024;
const NONCE_BYTES: usize = 12;
const TAG_BYTES: usize = 16;

pub(super) struct EncryptedSecretStore {
    root: PathBuf,
    keys: Box<dyn SecretStore>,
    legacy: Box<dyn SecretStore>,
}

impl EncryptedSecretStore {
    pub(super) fn new(
        root: PathBuf,
        keys: Box<dyn SecretStore>,
        legacy: Box<dyn SecretStore>,
    ) -> Self {
        Self { root, keys, legacy }
    }

    fn path(&self, user_id: &str) -> PathBuf {
        let digest = Sha256::digest(user_id.as_bytes());
        self.root.join(format!("{digest:x}.bin"))
    }

    fn read_key(&self, user_id: &str) -> Result<Option<[u8; 32]>, String> {
        let Some(encoded) = self.keys.get(user_id)? else {
            return Ok(None);
        };
        if encoded.len() != 44 {
            return Err("Saved session encryption key is invalid".into());
        }
        let bytes = STANDARD
            .decode(encoded)
            .map_err(|_| "Saved session encryption key is invalid")?;
        let key = bytes
            .try_into()
            .map_err(|_| "Saved session encryption key is invalid")?;
        Ok(Some(key))
    }

    fn write_encrypted(&self, path: &Path, user_id: &str, encoded: &str) -> Result<(), String> {
        let key = match self.read_key(user_id)? {
            Some(key) => key,
            None => {
                let key = rand::random::<[u8; 32]>();
                if path.exists() {
                    return Err("Saved session encryption key is missing".into());
                }
                self.keys.set(user_id, &STANDARD.encode(key))?;
                key
            }
        };
        let nonce = rand::random::<[u8; NONCE_BYTES]>();
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|_| "Saved session encryption key is invalid")?;
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: encoded.as_bytes(),
                    aad: user_id.as_bytes(),
                },
            )
            .map_err(|_| "Could not encrypt saved session")?;
        fs::create_dir_all(&self.root).map_err(|_| "Could not create secure session directory")?;
        let temporary = self
            .root
            .join(format!("{:032x}.tmp", rand::random::<u128>()));
        let result = (|| -> io::Result<()> {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(&nonce)?;
            file.write_all(&ciphertext)?;
            file.sync_all()?;
            fs::rename(&temporary, path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result.map_err(|_| "Could not write encrypted session".into())
    }
}

impl SecretStore for EncryptedSecretStore {
    fn get(&self, user_id: &str) -> Result<Option<String>, String> {
        let file = match fs::File::open(self.path(user_id)) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let legacy = self.legacy.get(user_id)?;
                if let Some(encoded) = &legacy {
                    let _ = self.set(user_id, encoded);
                }
                return Ok(legacy);
            }
            Err(_) => return Err("Encrypted session could not be read".into()),
        };
        let mut bytes = Vec::new();
        file.take((MAX_SESSION_BYTES + NONCE_BYTES + TAG_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| "Encrypted session could not be read")?;
        if bytes.len() < NONCE_BYTES + TAG_BYTES
            || bytes.len() > MAX_SESSION_BYTES + NONCE_BYTES + TAG_BYTES
        {
            return Err("Encrypted session exceeds size or format limits".into());
        }
        let key = self
            .read_key(user_id)?
            .ok_or("Saved session encryption key is missing")?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|_| "Saved session encryption key is invalid")?;
        let plaintext = cipher
            .decrypt(
                Nonce::from_slice(&bytes[..NONCE_BYTES]),
                Payload {
                    msg: &bytes[NONCE_BYTES..],
                    aad: user_id.as_bytes(),
                },
            )
            .map_err(|_| "Encrypted session authentication failed")?;
        let encoded =
            String::from_utf8(plaintext).map_err(|_| "Encrypted session contains invalid text")?;
        let _ = self.legacy.delete(user_id);
        Ok(Some(encoded))
    }

    fn set(&self, user_id: &str, encoded: &str) -> Result<(), String> {
        if encoded.len() > MAX_SESSION_BYTES || user_id.trim().is_empty() {
            return Err("Saved credential exceeds size or identity limits".into());
        }
        self.write_encrypted(&self.path(user_id), user_id, encoded)
    }

    fn delete(&self, user_id: &str) -> Result<(), String> {
        let path = self.path(user_id);
        let file = match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("Encrypted session could not be removed".into()),
        };
        let key = self.keys.delete(user_id);
        let legacy = self.legacy.delete(user_id);
        file.and(key).and(legacy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct TestStore {
        entries: Arc<Mutex<HashMap<String, String>>>,
        limit: Option<usize>,
        fail_set: bool,
    }

    impl SecretStore for TestStore {
        fn get(&self, user_id: &str) -> Result<Option<String>, String> {
            Ok(self.entries.lock().unwrap().get(user_id).cloned())
        }

        fn set(&self, user_id: &str, encoded: &str) -> Result<(), String> {
            if self.fail_set
                || self
                    .limit
                    .is_some_and(|limit| encoded.encode_utf16().count() * 2 > limit)
            {
                return Err("credential write failed".into());
            }
            self.entries
                .lock()
                .unwrap()
                .insert(user_id.into(), encoded.into());
            Ok(())
        }

        fn delete(&self, user_id: &str) -> Result<(), String> {
            self.entries.lock().unwrap().remove(user_id);
            Ok(())
        }
    }

    fn store(root: &Path, keys: TestStore, legacy: TestStore) -> EncryptedSecretStore {
        EncryptedSecretStore::new(root.into(), Box::new(keys), Box::new(legacy))
    }

    #[test]
    fn large_session_reopens_without_plaintext_or_credential_blob_overflow() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("secure-sessions");
        let keys = TestStore {
            limit: Some(2560),
            ..Default::default()
        };
        let legacy = TestStore {
            limit: Some(2560),
            ..Default::default()
        };
        let session = "opaque-payload".repeat(300);
        assert!(session.encode_utf16().count() * 2 > 2560);
        let first = store(&root, keys.clone(), legacy.clone());
        first.set("account", &session).unwrap();
        assert!(
            !fs::read(first.path("account"))
                .unwrap()
                .windows(session.len())
                .any(|window| window == session.as_bytes())
        );
        assert_eq!(keys.get("account").unwrap().unwrap().len(), 44);
        assert_eq!(
            store(&root, keys, legacy).get("account").unwrap(),
            Some(session)
        );
    }

    #[test]
    fn legacy_migration_survives_failed_write_and_removes_old_entry_after_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("secure-sessions");
        let keys = TestStore::default();
        let legacy = TestStore::default();
        legacy.set("account", "legacy-session").unwrap();
        let blocked = store(
            &root,
            TestStore {
                fail_set: true,
                ..keys.clone()
            },
            legacy.clone(),
        );
        assert_eq!(
            blocked.get("account").unwrap().as_deref(),
            Some("legacy-session")
        );
        assert_eq!(
            legacy.get("account").unwrap().as_deref(),
            Some("legacy-session")
        );
        assert!(keys.get("account").unwrap().is_none());
        keys.set("account", &STANDARD.encode(rand::random::<[u8; 32]>()))
            .unwrap();
        assert_eq!(
            store(&root, keys.clone(), legacy.clone())
                .get("account")
                .unwrap()
                .as_deref(),
            Some("legacy-session")
        );
        assert_eq!(
            store(&root, keys, legacy.clone())
                .get("account")
                .unwrap()
                .as_deref(),
            Some("legacy-session")
        );
        assert!(legacy.get("account").unwrap().is_none());
    }

    #[test]
    fn corrupted_or_missing_key_does_not_fall_back_or_rekey_ciphertext() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("secure-sessions");
        let keys = TestStore::default();
        let legacy = TestStore::default();
        let first = store(&root, keys.clone(), legacy.clone());
        first.set("account", "current-session").unwrap();
        legacy.set("account", "stale-session").unwrap();
        let path = first.path("account");
        let original = fs::read(&path).unwrap();
        keys.delete("account").unwrap();
        assert!(first.get("account").unwrap_err().contains("key is missing"));
        assert!(
            first
                .set("account", "replacement")
                .unwrap_err()
                .contains("key is missing")
        );
        assert_eq!(fs::read(&path).unwrap(), original);
        keys.set("account", &STANDARD.encode(rand::random::<[u8; 32]>()))
            .unwrap();
        assert!(
            first
                .get("account")
                .unwrap_err()
                .contains("authentication failed")
        );
        assert_eq!(
            legacy.get("account").unwrap().as_deref(),
            Some("stale-session")
        );
        first.delete("account").unwrap();
        assert!(!path.exists());
        assert!(keys.get("account").unwrap().is_none());
        assert!(legacy.get("account").unwrap().is_none());
    }

    #[test]
    fn oversized_input_and_ciphertext_are_rejected_without_migration() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("secure-sessions");
        let keys = TestStore::default();
        let legacy = TestStore::default();
        let first = store(&root, keys.clone(), legacy.clone());
        assert!(
            first
                .set("account", &"x".repeat(MAX_SESSION_BYTES + 1))
                .is_err()
        );
        assert!(keys.get("account").unwrap().is_none());
        fs::create_dir_all(&root).unwrap();
        fs::write(
            first.path("account"),
            vec![0; MAX_SESSION_BYTES + NONCE_BYTES + TAG_BYTES + 1],
        )
        .unwrap();
        legacy.set("account", "old-session").unwrap();
        assert!(first.get("account").unwrap_err().contains("size or format"));
        assert_eq!(
            legacy.get("account").unwrap().as_deref(),
            Some("old-session")
        );
    }

    #[test]
    fn sign_out_deletes_both_stores_and_next_sign_in_uses_new_key() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("secure-sessions");
        let keys = TestStore::default();
        let legacy = TestStore::default();
        let vault = store(&root, keys.clone(), legacy.clone());
        vault.set("account", "old-session").unwrap();
        let old_key = keys.get("account").unwrap().unwrap();
        vault.delete("account").unwrap();
        assert!(vault.get("account").unwrap().is_none());
        vault.set("account", "new-session").unwrap();
        assert_ne!(keys.get("account").unwrap().unwrap(), old_key);
        assert_eq!(
            store(&root, keys, legacy)
                .get("account")
                .unwrap()
                .as_deref(),
            Some("new-session")
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_credential_manager_migrates_and_restores_after_restart() {
        use crate::credential_vault::{
            CredentialVault, KEY_SERVICE_NAME, SERVICE_NAME, credential,
        };

        struct Cleanup(String);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = credential(SERVICE_NAME, &self.0)
                    .and_then(|entry| entry.delete_credential().map_err(|error| error.to_string()));
                let _ = credential(KEY_SERVICE_NAME, &self.0)
                    .and_then(|entry| entry.delete_credential().map_err(|error| error.to_string()));
            }
        }

        let directory = tempfile::tempdir().unwrap();
        let user_id = format!("opennow-vault-test-{:032x}", rand::random::<u128>());
        let _cleanup = Cleanup(user_id.clone());
        let mut session = super::super::tests::sample_session(&user_id);
        let legacy = serde_json::to_string(&session).unwrap();
        credential(SERVICE_NAME, &user_id)
            .unwrap()
            .set_password(&legacy)
            .unwrap();
        let vault = CredentialVault::new(directory.path().into());
        vault
            .write_metadata(&super::super::Metadata {
                active_user_id: Some(user_id.clone()),
                accounts: vec![super::super::tests::sample_identity(&user_id)],
                ..Default::default()
            })
            .unwrap();
        assert_eq!(vault.load_active().unwrap().unwrap().user.user_id, user_id);
        drop(vault);
        let reopened = CredentialVault::new(directory.path().into());
        assert_eq!(
            reopened.load_active().unwrap().unwrap().user.user_id,
            user_id
        );
        assert!(matches!(
            credential(SERVICE_NAME, &user_id).unwrap().get_password(),
            Err(keyring::Error::NoEntry)
        ));

        session.tokens.access_token = "x".repeat(1800);
        let size = serde_json::to_string(&session).unwrap().len();
        assert!((1944..2560).contains(&size), "fixture size: {size}");
        reopened.save(&session).unwrap();
        drop(reopened);
        let restored = CredentialVault::new(directory.path().into());
        assert_eq!(
            restored.load_active().unwrap().unwrap().tokens.access_token,
            session.tokens.access_token
        );
        let encrypted = directory
            .path()
            .join("secure-sessions")
            .join(format!("{:x}.bin", Sha256::digest(user_id.as_bytes())));
        assert!(
            !fs::read(encrypted)
                .unwrap()
                .windows(1800)
                .any(|window| window == session.tokens.access_token.as_bytes())
        );
        session.tokens.access_token = "x".repeat(3200);
        assert!(
            serde_json::to_string(&session)
                .unwrap()
                .encode_utf16()
                .count()
                * 2
                > 2560
        );
        restored.save(&session).unwrap();
        drop(restored);
        let restored = CredentialVault::new(directory.path().into());
        assert_eq!(
            restored.load_active().unwrap().unwrap().tokens.access_token,
            session.tokens.access_token
        );
        restored.remove(&user_id).unwrap();
        assert!(
            CredentialVault::new(directory.path().into())
                .load(&user_id)
                .unwrap()
                .is_none()
        );
    }
}
