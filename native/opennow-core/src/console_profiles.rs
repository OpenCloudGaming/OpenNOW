use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use rand::RngCore as _;
use scrypt::{Params, scrypt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use subtle::ConstantTimeEq as _;

const PIN_MAX_ATTEMPTS: u32 = 5;
const LOCKOUT_STEPS_MS: [u64; 4] = [30_000, 60_000, 300_000, 900_000];
const SCRYPT_LOG_N: u8 = 15;
const SCRYPT_R: u32 = 8;
const SCRYPT_P: u32 = 1;
const KEY_LENGTH: usize = 32;
const SALT_LENGTH: usize = 16;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PinRecord {
    version: u32,
    algorithm: String,
    log_n: u8,
    r: u32,
    p: u32,
    key_length: usize,
    salt: String,
    hash: String,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AttemptState {
    failed_attempts: u32,
    lockout_level: usize,
    locked_until_ms: Option<u64>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Profile {
    user_id: String,
    pin: Option<PinRecord>,
    attempts: AttemptState,
    updated_at_ms: u64,
}

#[derive(Default, Serialize, Deserialize)]
struct Document {
    version: u32,
    profiles: Vec<Profile>,
}

pub struct ConsoleProfiles {
    path: PathBuf,
    profiles: Mutex<HashMap<String, Profile>>,
}

impl ConsoleProfiles {
    pub fn load(data_dir: &Path) -> Self {
        // Electron owns console-profiles.json and wraps it with safeStorage.
        // Keep the Qt/Rust format separate so a migration trial never makes
        // the shipping rollback client unable to read its profile locks.
        let path = data_dir.join("console-profiles-qt.json");
        let profiles = fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<Document>(&text).ok())
            .map(|document| {
                document
                    .profiles
                    .into_iter()
                    .filter(|profile| !profile.user_id.is_empty())
                    .map(|profile| (profile.user_id.clone(), profile))
                    .collect()
            })
            .unwrap_or_default();
        Self {
            path,
            profiles: Mutex::new(profiles),
        }
    }

    pub fn has_pin(&self, user_id: &str) -> bool {
        self.profiles
            .lock()
            .expect("console profiles poisoned")
            .get(user_id)
            .is_some_and(|profile| profile.pin.is_some())
    }

    pub fn status(&self, user_id: &str) -> Value {
        let profiles = self.profiles.lock().expect("console profiles poisoned");
        let Some(profile) = profiles
            .get(user_id)
            .filter(|profile| profile.pin.is_some())
        else {
            return json!({
                "userId":user_id,"hasPin":false,"lockedUntilMs":null,
                "remainingAttempts":PIN_MAX_ATTEMPTS
            });
        };
        let gate = gate(&profile.attempts, now_ms());
        json!({
            "userId":user_id,"hasPin":true,"lockedUntilMs":gate.locked_until_ms,
            "remainingAttempts":gate.remaining_attempts
        })
    }

    pub fn set_pin(
        &self,
        user_id: &str,
        pin: &str,
        current_pin: Option<&str>,
    ) -> Result<Value, String> {
        if !valid_pin(pin) {
            return Ok(
                json!({"ok":false,"reason":"invalid_format","hasPin":self.has_pin(user_id)}),
            );
        }
        let mut next = self
            .profiles
            .lock()
            .expect("console profiles poisoned")
            .clone();
        let now = now_ms();
        let profile = next.entry(user_id.to_owned()).or_insert_with(|| Profile {
            user_id: user_id.to_owned(),
            pin: None,
            attempts: AttemptState::default(),
            updated_at_ms: 0,
        });
        if let Some(existing) = profile.pin.as_ref() {
            let gate = gate(&profile.attempts, now);
            if !gate.allowed {
                return Ok(
                    json!({"ok":false,"reason":"locked_out","hasPin":true,"lockedUntilMs":gate.locked_until_ms}),
                );
            }
            let Some(current_pin) = current_pin.filter(|pin| valid_pin(pin)) else {
                return Ok(json!({"ok":false,"reason":"invalid_format","hasPin":true}));
            };
            if !verify_hash(current_pin, existing) {
                register_failure(&mut profile.attempts, now);
                self.persist_and_replace(next)?;
                return Ok(json!({"ok":false,"reason":"invalid_pin","hasPin":true}));
            }
        }
        profile.pin = Some(hash_pin(pin)?);
        profile.attempts = AttemptState::default();
        profile.updated_at_ms = now;
        self.persist_and_replace(next)?;
        Ok(json!({"ok":true,"hasPin":true}))
    }

    pub fn clear_pin(&self, user_id: &str, current_pin: &str) -> Result<Value, String> {
        let mut next = self
            .profiles
            .lock()
            .expect("console profiles poisoned")
            .clone();
        let Some(profile) = next.get_mut(user_id) else {
            return Ok(json!({"ok":false,"reason":"no_pin_set","hasPin":false}));
        };
        let Some(existing) = profile.pin.as_ref() else {
            return Ok(json!({"ok":false,"reason":"no_pin_set","hasPin":false}));
        };
        let now = now_ms();
        let gate = gate(&profile.attempts, now);
        if !gate.allowed {
            return Ok(
                json!({"ok":false,"reason":"locked_out","hasPin":true,"lockedUntilMs":gate.locked_until_ms}),
            );
        }
        if !valid_pin(current_pin) {
            return Ok(json!({"ok":false,"reason":"invalid_format","hasPin":true}));
        }
        if !verify_hash(current_pin, existing) {
            register_failure(&mut profile.attempts, now);
            self.persist_and_replace(next)?;
            return Ok(json!({"ok":false,"reason":"invalid_pin","hasPin":true}));
        }
        profile.pin = None;
        profile.attempts = AttemptState::default();
        profile.updated_at_ms = now;
        self.persist_and_replace(next)?;
        Ok(json!({"ok":true,"hasPin":false}))
    }

    pub fn verify(&self, user_id: &str, pin: &str) -> Result<Value, String> {
        let mut next = self
            .profiles
            .lock()
            .expect("console profiles poisoned")
            .clone();
        let Some(profile) = next.get_mut(user_id) else {
            return Ok(
                json!({"ok":true,"reason":"no_pin_set","remainingAttempts":PIN_MAX_ATTEMPTS,"lockedUntilMs":null}),
            );
        };
        let Some(record) = profile.pin.as_ref() else {
            return Ok(
                json!({"ok":true,"reason":"no_pin_set","remainingAttempts":PIN_MAX_ATTEMPTS,"lockedUntilMs":null}),
            );
        };
        let now = now_ms();
        let current_gate = gate(&profile.attempts, now);
        if !current_gate.allowed {
            return Ok(
                json!({"ok":false,"reason":"locked_out","remainingAttempts":0,"lockedUntilMs":current_gate.locked_until_ms}),
            );
        }
        if !valid_pin(pin) {
            return Ok(
                json!({"ok":false,"reason":"invalid_format","remainingAttempts":current_gate.remaining_attempts,"lockedUntilMs":null}),
            );
        }
        if verify_hash(pin, record) {
            profile.attempts = AttemptState::default();
            self.persist_and_replace(next)?;
            return Ok(
                json!({"ok":true,"remainingAttempts":PIN_MAX_ATTEMPTS,"lockedUntilMs":null}),
            );
        }
        register_failure(&mut profile.attempts, now);
        let next_gate = gate(&profile.attempts, now);
        self.persist_and_replace(next)?;
        Ok(json!({
            "ok":false,
            "reason":if next_gate.allowed { "invalid_pin" } else { "locked_out" },
            "remainingAttempts":next_gate.remaining_attempts,
            "lockedUntilMs":next_gate.locked_until_ms
        }))
    }

    pub fn forget(&self, user_id: &str) -> Result<(), String> {
        let mut next = self
            .profiles
            .lock()
            .expect("console profiles poisoned")
            .clone();
        if next.remove(user_id).is_some() {
            self.persist_and_replace(next)?;
        }
        Ok(())
    }

    pub fn forget_all(&self) -> Result<(), String> {
        self.persist_and_replace(HashMap::new())
    }

    fn persist_and_replace(&self, profiles: HashMap<String, Profile>) -> Result<(), String> {
        let mut values = profiles.values().cloned().collect::<Vec<_>>();
        values.sort_by(|left, right| left.user_id.cmp(&right.user_id));
        let document = Document {
            version: 1,
            profiles: values,
        };
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let temporary = self.path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(&document).map_err(|error| error.to_string())?;
        write_private_file(&temporary, &bytes).map_err(|error| error.to_string())?;
        fs::rename(&temporary, &self.path).map_err(|error| error.to_string())?;
        *self.profiles.lock().expect("console profiles poisoned") = profiles;
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct Gate {
    allowed: bool,
    locked_until_ms: Option<u64>,
    remaining_attempts: u32,
}

fn gate(state: &AttemptState, now: u64) -> Gate {
    let locked = state.locked_until_ms.is_some_and(|until| now < until);
    Gate {
        allowed: !locked,
        locked_until_ms: locked.then_some(state.locked_until_ms).flatten(),
        remaining_attempts: if locked {
            0
        } else {
            PIN_MAX_ATTEMPTS.saturating_sub(state.failed_attempts)
        },
    }
}

fn register_failure(state: &mut AttemptState, now: u64) {
    state.failed_attempts += 1;
    if state.failed_attempts < PIN_MAX_ATTEMPTS {
        state.locked_until_ms = None;
        return;
    }
    let duration = LOCKOUT_STEPS_MS[state.lockout_level.min(LOCKOUT_STEPS_MS.len() - 1)];
    state.failed_attempts = 0;
    state.lockout_level = (state.lockout_level + 1).min(LOCKOUT_STEPS_MS.len());
    state.locked_until_ms = Some(now.saturating_add(duration));
}

fn hash_pin(pin: &str) -> Result<PinRecord, String> {
    let mut salt = [0_u8; SALT_LENGTH];
    rand::rng().fill_bytes(&mut salt);
    let params = Params::new(SCRYPT_LOG_N, SCRYPT_R, SCRYPT_P, KEY_LENGTH)
        .map_err(|error| error.to_string())?;
    let mut hash = [0_u8; KEY_LENGTH];
    scrypt(pin.as_bytes(), &salt, &params, &mut hash).map_err(|error| error.to_string())?;
    Ok(PinRecord {
        version: 1,
        algorithm: "scrypt".to_owned(),
        log_n: SCRYPT_LOG_N,
        r: SCRYPT_R,
        p: SCRYPT_P,
        key_length: KEY_LENGTH,
        salt: BASE64.encode(salt),
        hash: BASE64.encode(hash),
    })
}

fn verify_hash(pin: &str, record: &PinRecord) -> bool {
    if record.version != 1
        || record.algorithm != "scrypt"
        || record.key_length == 0
        || record.key_length > 64
    {
        return false;
    }
    let Ok(salt) = BASE64.decode(&record.salt) else {
        return false;
    };
    let Ok(expected) = BASE64.decode(&record.hash) else {
        return false;
    };
    if expected.len() != record.key_length {
        return false;
    }
    let Ok(params) = Params::new(record.log_n, record.r, record.p, record.key_length) else {
        return false;
    };
    let mut actual = vec![0_u8; record.key_length];
    if scrypt(pin.as_bytes(), &salt, &params, &mut actual).is_err() {
        return false;
    }
    bool::from(actual.ct_eq(&expected))
}

fn valid_pin(pin: &str) -> bool {
    pin.len() == 4 && pin.bytes().all(|character| character.is_ascii_digit())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
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

    fn temporary_store() -> (ConsoleProfiles, PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "opennow-profiles-{}-{}",
            std::process::id(),
            now_ms()
        ));
        (ConsoleProfiles::load(&path), path)
    }

    #[test]
    fn pin_round_trip_and_lockout_are_persisted() {
        let (profiles, path) = temporary_store();
        assert_eq!(profiles.set_pin("user", "1234", None).unwrap()["ok"], true);
        assert!(profiles.has_pin("user"));
        assert_eq!(profiles.verify("user", "1234").unwrap()["ok"], true);
        for _ in 0..PIN_MAX_ATTEMPTS {
            let _ = profiles.verify("user", "0000").unwrap();
        }
        assert_eq!(profiles.status("user")["remainingAttempts"], 0);
        let loaded = ConsoleProfiles::load(&path);
        assert_eq!(loaded.status("user")["remainingAttempts"], 0);
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn malformed_pin_does_not_consume_an_attempt() {
        let (profiles, path) = temporary_store();
        profiles.set_pin("user", "1234", None).unwrap();
        let result = profiles.verify("user", "１２３４").unwrap();
        assert_eq!(result["reason"], "invalid_format");
        assert_eq!(result["remainingAttempts"], PIN_MAX_ATTEMPTS);
        let _ = fs::remove_dir_all(path);
    }
}
