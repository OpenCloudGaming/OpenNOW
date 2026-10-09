use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

#[derive(Serialize, Deserialize)]
struct Identity {
    version: u8,
    id: String,
}

pub fn load(data_dir: &Path, legacy_id: impl FnOnce() -> String) -> Result<String, String> {
    let load = || -> Result<String, Box<dyn std::error::Error>> {
        fs::create_dir_all(data_dir)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(data_dir.join("device-identity.lock"))?;
        lock.lock_exclusive()?;
        let path = data_dir.join("device-identity.json");
        match fs::File::open(&path) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(4097).read_to_end(&mut bytes)?;
                let identity: Identity = serde_json::from_slice(&bytes)?;
                if identity.version != 1
                    || identity.id.len() != 64
                    || !identity.id.bytes().all(|byte| byte.is_ascii_hexdigit())
                {
                    return Err("Device identity is invalid".into());
                }
                Ok(identity.id)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let upgraded = [
                    "accounts.json",
                    "auth-state.json",
                    "sessions",
                    "pending-session-cleanup.json",
                ]
                .iter()
                .any(|name| data_dir.join(name).exists());
                let id = if upgraded {
                    legacy_id()
                } else {
                    use rand::RngCore as _;
                    let mut bytes = [0u8; 32];
                    rand::rng().fill_bytes(&mut bytes);
                    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
                };
                let temporary = path.with_extension("json.tmp");
                let mut file = fs::File::create(&temporary)?;
                file.write_all(&serde_json::to_vec(&Identity {
                    version: 1,
                    id: id.clone(),
                })?)?;
                file.sync_all()?;
                fs::rename(temporary, path)?;
                #[cfg(unix)]
                fs::File::open(data_dir)?.sync_all()?;
                Ok(id)
            }
            Err(error) => Err(error.into()),
        }
    };
    load().map_err(|_| "Persistent device identity is unavailable or invalid; restore access to the data directory before signing in".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freezes_legacy_identity_and_refuses_corruption() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("accounts.json"), "{}").unwrap();
        let original = "a".repeat(64);
        assert_eq!(
            load(directory.path(), || original.clone()).unwrap(),
            original
        );
        assert_eq!(load(directory.path(), || "b".repeat(64)).unwrap(), original);
        fs::write(directory.path().join("device-identity.json"), "broken").unwrap();
        assert!(load(directory.path(), || "c".repeat(64)).is_err());
    }

    #[test]
    fn concurrent_fresh_initialization_chooses_one_identity() {
        let directory = tempfile::tempdir().unwrap();
        std::thread::scope(|scope| {
            let workers: Vec<_> = (0..8)
                .map(|_| scope.spawn(|| load(directory.path(), || panic!("fresh installation"))))
                .collect();
            let ids: Vec<_> = workers
                .into_iter()
                .map(|worker| worker.join().unwrap().unwrap())
                .collect();
            assert!(ids.iter().all(|id| id == &ids[0] && id.len() == 64));
        });
    }
}
