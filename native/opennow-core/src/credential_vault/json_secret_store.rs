use super::{SecretStore, decode_session, read_bounded};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

pub(super) struct JsonFallbackStore {
    root: PathBuf,
    secure: Box<dyn SecretStore>,
}

impl JsonFallbackStore {
    pub(super) fn new(root: PathBuf, secure: Box<dyn SecretStore>) -> Self {
        Self { root, secure }
    }

    fn path(&self, user_id: &str) -> PathBuf {
        let digest = Sha256::digest(user_id.as_bytes());
        self.root.join(format!("{digest:x}.json"))
    }

    pub(super) fn cleanup_temporary_files(&self) -> Result<(), String> {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(_) => return Err("Local session temporary-file cleanup is pending".into()),
        };
        for entry in entries {
            let entry = entry.map_err(|_| "Local session temporary-file cleanup is pending")?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.strip_suffix(".tmp").is_some_and(|stem| {
                stem.len() == 64 && stem.bytes().all(|byte| byte.is_ascii_hexdigit())
            }) {
                fs::remove_file(entry.path())
                    .map_err(|_| "Local session temporary-file cleanup is pending")?;
            }
        }
        Ok(())
    }

    fn write(&self, user_id: &str, encoded: &str) -> Result<(), String> {
        let result = (|| -> io::Result<()> {
            fs::create_dir_all(&self.root)?;
            if !fs::symlink_metadata(&self.root)?.is_dir() {
                return Err(io::Error::other("Invalid credential directory"));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&self.root, fs::Permissions::from_mode(0o700))?;
            }
            let temporary = self.path(user_id).with_extension("tmp");
            match fs::remove_file(&temporary) {
                Ok(()) => (),
                Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                Err(error) => return Err(error),
            }
            let result = (|| -> io::Result<()> {
                let mut file = create_private_file(&temporary)?;
                file.write_all(encoded.as_bytes())?;
                file.sync_all()?;
                drop(file);
                fs::rename(&temporary, self.path(user_id))?;
                #[cfg(unix)]
                fs::File::open(&self.root)?.sync_all()?;
                Ok(())
            })();
            if result.is_err() {
                let _ = fs::remove_file(&temporary);
            }
            result
        })();
        result.map_err(|_| "Could not save the local session file".into())
    }

    fn remove_file(&self, user_id: &str) -> Result<(), String> {
        let mut result = Ok(());
        for path in [self.path(user_id), self.path(user_id).with_extension("tmp")] {
            match fs::remove_file(path) {
                Ok(()) => (),
                Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                Err(_) => result = Err("Could not remove the local session file".into()),
            }
        }
        result
    }
}

impl SecretStore for JsonFallbackStore {
    fn get(&self, user_id: &str) -> Result<Option<String>, String> {
        match read_bounded(&self.path(user_id)) {
            Ok(bytes) => {
                let encoded = String::from_utf8(bytes).map_err(|_| "Local session is invalid")?;
                decode_session(&encoded, user_id)?;
                Ok(Some(encoded))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => self.secure.get(user_id),
            Err(_) => Err("Could not read the local session file".into()),
        }
    }

    fn set(&self, user_id: &str, encoded: &str) -> Result<(), String> {
        decode_session(encoded, user_id)?;
        let local = self
            .path(user_id)
            .try_exists()
            .map_err(|_| "Could not inspect local session")?;
        if local {
            self.write(user_id, encoded)?;
        }
        if self.secure.set(user_id, encoded).is_ok()
            && self.secure.get(user_id).ok().flatten().as_deref() == Some(encoded)
        {
            self.remove_file(user_id)?;
            return Ok(());
        }
        if !local {
            self.write(user_id, encoded)?;
        }
        Ok(())
    }

    fn delete(&self, user_id: &str) -> Result<(), String> {
        let local = self.remove_file(user_id);
        let secure = self.secure.delete(user_id);
        local.and(secure)
    }

    fn is_plaintext(&self, user_id: &str) -> bool {
        self.path(user_id).exists()
    }
}

#[cfg(unix)]
fn create_private_file(path: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

#[cfg(windows)]
fn create_private_file(path: &Path) -> io::Result<fs::File> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::FromRawHandle;
    use windows_sys::Win32::Foundation::{INVALID_HANDLE_VALUE, LocalFree};
    use windows_sys::Win32::Security::Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW;
    use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
    use windows_sys::Win32::Storage::FileSystem::{
        CREATE_NEW, CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_WRITE,
    };

    let text: Vec<u16> = "D:P(A;;FA;;;OW)(A;;FA;;;SY)"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let mut descriptor = std::ptr::null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            text.as_ptr(),
            1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor,
        bInheritHandle: 0,
    };
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_GENERIC_WRITE,
            0,
            &attributes,
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        )
    };
    let error = io::Error::last_os_error();
    unsafe {
        LocalFree(descriptor);
    }
    if handle == INVALID_HANDLE_VALUE {
        return Err(error);
    }
    Ok(unsafe { fs::File::from_raw_handle(handle) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credential_vault::{CredentialVault, MemorySecretStore, tests::sample_session};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct Store {
        entries: Arc<Mutex<HashMap<String, String>>>,
        unavailable: Arc<AtomicBool>,
        discard_writes: Arc<AtomicBool>,
    }

    impl SecretStore for Store {
        fn get(&self, user_id: &str) -> Result<Option<String>, String> {
            if self.unavailable.load(Ordering::SeqCst) {
                return Err("unavailable".into());
            }
            Ok(self.entries.lock().unwrap().get(user_id).cloned())
        }

        fn set(&self, user_id: &str, encoded: &str) -> Result<(), String> {
            if self.unavailable.load(Ordering::SeqCst) {
                return Err("unavailable".into());
            }
            if !self.discard_writes.load(Ordering::SeqCst) {
                self.entries
                    .lock()
                    .unwrap()
                    .insert(user_id.into(), encoded.into());
            }
            Ok(())
        }

        fn delete(&self, user_id: &str) -> Result<(), String> {
            if self.unavailable.load(Ordering::SeqCst) {
                return Err("unavailable".into());
            }
            self.entries.lock().unwrap().remove(user_id);
            Ok(())
        }
    }

    #[test]
    fn secure_store_is_preferred_without_creating_json() {
        let directory = tempfile::tempdir().unwrap();
        let vault = CredentialVault::with_store(directory.path().into(), Box::<Store>::default());
        let session = sample_session("user");
        vault.save(&session).unwrap();
        assert_eq!(vault.persistence_state(&session), "secure-store");
        assert!(!directory.path().join("fallback-sessions").exists());
    }

    #[test]
    fn metadata_failure_does_not_hide_plaintext_storage() {
        let directory = tempfile::tempdir().unwrap();
        let vault = CredentialVault::without_os_store(directory.path().into());
        let mut session = sample_session("user");
        vault.save(&session).unwrap();
        fs::create_dir(directory.path().join("accounts.json.tmp")).unwrap();
        session.tokens.refresh_token = Some("new-test-token".into());
        assert!(vault.save(&session).is_err());
        assert_eq!(vault.failed_save_state(&session), "local-file");
        assert!(!vault.warnings().is_empty());
        drop(vault);
        let restored = CredentialVault::without_os_store(directory.path().into());
        assert_eq!(
            restored
                .load_active()
                .unwrap()
                .unwrap()
                .tokens
                .refresh_token,
            session.tokens.refresh_token
        );
        fs::remove_dir(directory.path().join("accounts.json.tmp")).unwrap();
        restored.save(&session).unwrap();
        assert!(restored.warnings().is_empty());
    }

    #[test]
    fn restart_cleans_interrupted_plaintext_writes_without_account_metadata() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("fallback-sessions");
        fs::create_dir(&root).unwrap();
        let store = JsonFallbackStore::new(root.clone(), Box::<Store>::default());
        let session = serde_json::to_string(&sample_session("unindexed")).unwrap();
        let pending = store.path("unindexed").with_extension("tmp");
        fs::write(&pending, &session).unwrap();
        let committed = store.path("other");
        fs::write(&committed, "not-temporary").unwrap();
        CredentialVault::without_os_store(directory.path().into());
        assert!(!pending.exists());
        assert!(committed.exists());
        assert!(!directory.path().join("accounts.json").exists());
    }

    #[test]
    fn logout_and_save_remove_only_the_accounts_interrupted_write() {
        let directory = tempfile::tempdir().unwrap();
        let store = JsonFallbackStore::new(
            directory.path().into(),
            Box::new(MemorySecretStore {
                unavailable: true,
                ..Default::default()
            }),
        );
        let encoded = serde_json::to_string(&sample_session("user")).unwrap();
        let pending = store.path("user").with_extension("tmp");
        let other = store.path("other").with_extension("tmp");
        fs::write(&pending, &encoded).unwrap();
        fs::write(&other, &encoded).unwrap();
        assert!(store.delete("user").is_err());
        assert!(!pending.exists());
        assert!(other.exists());
        fs::write(&pending, &encoded).unwrap();
        store.set("user", &encoded).unwrap();
        assert!(!pending.exists());
        assert!(other.exists());
        assert_eq!(
            store.get("user").unwrap().as_deref(),
            Some(encoded.as_str())
        );
    }

    #[test]
    fn latest_json_wins_until_a_verified_secure_save() {
        let directory = tempfile::tempdir().unwrap();
        let secure = Store::default();
        let vault = CredentialVault::with_store(directory.path().into(), Box::new(secure.clone()));
        let mut session = sample_session("user");
        vault.save(&session).unwrap();
        secure.unavailable.store(true, Ordering::SeqCst);
        session.tokens.refresh_token = Some("refreshed-test-token".into());
        vault.save(&session).unwrap();
        assert_eq!(vault.persistence_state(&session), "local-file");
        drop(vault);
        secure.unavailable.store(false, Ordering::SeqCst);
        let restored =
            CredentialVault::with_store(directory.path().into(), Box::new(secure.clone()));
        assert_eq!(
            restored
                .load_active()
                .unwrap()
                .unwrap()
                .tokens
                .refresh_token,
            Some("refreshed-test-token".into())
        );
        assert_eq!(restored.persistence_state(&session), "local-file");
        secure.discard_writes.store(true, Ordering::SeqCst);
        restored.save(&session).unwrap();
        assert_eq!(restored.persistence_state(&session), "local-file");
        secure.discard_writes.store(false, Ordering::SeqCst);
        restored.save(&session).unwrap();
        assert_eq!(restored.persistence_state(&session), "secure-store");
        assert_eq!(
            fs::read_dir(directory.path().join("fallback-sessions"))
                .unwrap()
                .count(),
            0
        );
        assert_eq!(
            restored
                .load_active()
                .unwrap()
                .unwrap()
                .tokens
                .refresh_token,
            Some("refreshed-test-token".into())
        );
        restored.remove("user").unwrap();
        assert!(secure.get("user").unwrap().is_none());
    }

    #[test]
    fn logout_removes_json_and_suppresses_old_secure_grant() {
        let directory = tempfile::tempdir().unwrap();
        let secure = Store::default();
        let vault = CredentialVault::with_store(directory.path().into(), Box::new(secure.clone()));
        let mut session = sample_session("user");
        vault.save(&session).unwrap();
        secure.unavailable.store(true, Ordering::SeqCst);
        session.tokens.refresh_token = Some("new-test-token".into());
        vault.save(&session).unwrap();
        assert!(vault.remove("user").is_err());
        assert_eq!(
            fs::read_dir(directory.path().join("fallback-sessions"))
                .unwrap()
                .count(),
            0
        );
        secure.unavailable.store(false, Ordering::SeqCst);
        let restored = CredentialVault::with_store(directory.path().into(), Box::new(secure));
        assert!(restored.load("user").unwrap().is_none());
        assert!(restored.load_active().unwrap().is_none());
        restored.remove("user").unwrap();
        assert!(restored.warnings().is_empty());
    }

    #[test]
    fn windows_encryption_store_without_vault_keys_uses_json() {
        let directory = tempfile::tempdir().unwrap();
        let unavailable = || {
            Box::new(MemorySecretStore {
                unavailable: true,
                ..Default::default()
            }) as Box<dyn SecretStore>
        };
        let secure = super::super::encrypted_secret_store::EncryptedSecretStore::new(
            directory.path().join("secure-sessions"),
            unavailable(),
            unavailable(),
        );
        let vault = CredentialVault::with_store(directory.path().into(), Box::new(secure));
        let session = sample_session("user");
        vault.save(&session).unwrap();
        assert_eq!(vault.persistence_state(&session), "local-file");
        assert!(vault.durable(&session));
        assert!(!directory.path().join("secure-sessions").exists());
    }

    #[test]
    fn malformed_or_mismatched_local_json_does_not_restore_stale_secure_tokens() {
        let directory = tempfile::tempdir().unwrap();
        let store = JsonFallbackStore::new(directory.path().into(), Box::<Store>::default());
        let encoded = serde_json::to_string(&sample_session("user")).unwrap();
        store.secure.set("user", &encoded).unwrap();
        fs::write(store.path("user"), "invalid-json").unwrap();
        assert!(store.get("user").is_err());
        fs::write(
            store.path("user"),
            serde_json::to_string(&sample_session("other")).unwrap(),
        )
        .unwrap();
        assert!(store.get("user").is_err());
        assert!(store.set("other", &encoded).is_err());
        assert!(store.set("user", &"x".repeat(4 * 1024 * 1024 + 1)).is_err());
        assert_ne!(store.path("a/b"), store.path("a_b"));
        assert_eq!(store.path("../../outside").parent(), Some(directory.path()));
    }

    #[test]
    fn unwritable_fallback_reports_failure_without_temporary_files() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("not-a-directory");
        fs::write(&root, "blocked").unwrap();
        let store = JsonFallbackStore::new(
            root,
            Box::new(MemorySecretStore {
                unavailable: true,
                ..Default::default()
            }),
        );
        assert!(
            store
                .set(
                    "user",
                    &serde_json::to_string(&sample_session("user")).unwrap()
                )
                .is_err()
        );
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn json_permissions_remain_private_across_atomic_replacements() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let store = JsonFallbackStore::new(
            directory.path().join("fallback"),
            Box::new(MemorySecretStore {
                unavailable: true,
                ..Default::default()
            }),
        );
        let mut session = sample_session("user");
        for token in ["first-test-token", "second-test-token"] {
            session.tokens.refresh_token = Some(token.into());
            let encoded = serde_json::to_string(&session).unwrap();
            store.set("user", &encoded).unwrap();
            assert_eq!(
                fs::metadata(store.path("user"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(&store.root).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                store.get("user").unwrap().as_deref(),
                Some(encoded.as_str())
            );
            assert_eq!(fs::read_dir(&store.root).unwrap().count(), 1);
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_json_has_a_protected_owner_and_system_acl_after_replacement() {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Foundation::LocalFree;
        use windows_sys::Win32::Security::Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT};
        use windows_sys::Win32::Security::{
            DACL_SECURITY_INFORMATION, GetSecurityDescriptorControl, SE_DACL_PROTECTED,
        };
        let directory = tempfile::tempdir().unwrap();
        let store = JsonFallbackStore::new(
            directory.path().join("fallback"),
            Box::new(MemorySecretStore {
                unavailable: true,
                ..Default::default()
            }),
        );
        let mut session = sample_session("user");
        for token in ["first-test-token", "second-test-token"] {
            session.tokens.refresh_token = Some(token.into());
            let encoded = serde_json::to_string(&session).unwrap();
            store.set("user", &encoded).unwrap();
            assert_eq!(
                store.get("user").unwrap().as_deref(),
                Some(encoded.as_str())
            );
            let path: Vec<u16> = store
                .path("user")
                .as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect();
            let mut descriptor = std::ptr::null_mut();
            let mut acl = std::ptr::null_mut();
            assert_eq!(
                unsafe {
                    GetNamedSecurityInfoW(
                        path.as_ptr(),
                        SE_FILE_OBJECT,
                        DACL_SECURITY_INFORMATION,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        &mut acl,
                        std::ptr::null_mut(),
                        &mut descriptor,
                    )
                },
                0
            );
            let mut control = 0;
            let mut revision = 0;
            let success =
                unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) };
            let entries = unsafe { (*acl).AceCount };
            unsafe {
                LocalFree(descriptor);
            }
            assert_ne!(success, 0);
            assert_ne!(control & SE_DACL_PROTECTED, 0);
            assert_eq!(entries, 2);
        }
    }
}
