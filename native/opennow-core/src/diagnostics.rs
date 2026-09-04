use serde_json::{Value, json};
use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const ENTRY_LIMIT: usize = 1_200;
const LOG_LIMIT_BYTES: u64 = 1_500_000;

#[derive(Clone)]
struct Entry {
    at_ms: u128,
    area: String,
    event: String,
    detail: String,
}

pub struct DiagnosticsService {
    directory: PathBuf,
    current_path: PathBuf,
    previous_path: PathBuf,
    entries: Mutex<VecDeque<Entry>>,
}

impl DiagnosticsService {
    pub fn new(data_dir: &Path) -> io::Result<Self> {
        let directory = data_dir.join("diagnostics");
        fs::create_dir_all(&directory)?;
        let current_path = directory.join("current.log");
        let previous_path = directory.join("previous.log");
        if current_path
            .metadata()
            .is_ok_and(|metadata| metadata.len() > LOG_LIMIT_BYTES)
        {
            let _ = fs::remove_file(&previous_path);
            fs::rename(&current_path, &previous_path)?;
        }
        Ok(Self {
            directory,
            current_path,
            previous_path,
            entries: Mutex::new(VecDeque::with_capacity(ENTRY_LIMIT)),
        })
    }

    pub fn record(&self, area: &str, event: &str, detail: impl AsRef<str>) {
        let entry = Entry {
            at_ms: now_ms(),
            area: clean(area, 48),
            event: clean(event, 72),
            detail: redact(detail.as_ref(), 480),
        };
        {
            let mut entries = self.entries.lock().expect("diagnostics poisoned");
            if entries.len() == ENTRY_LIMIT {
                entries.pop_front();
            }
            entries.push_back(entry.clone());
        }
        let line = format_entry(&entry);
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.current_path)
        {
            let _ = file.write_all(line.as_bytes());
        }
    }

    pub fn snapshot(&self) -> Value {
        let entries = self.entries.lock().expect("diagnostics poisoned");
        let values = entries
            .iter()
            .rev()
            .take(200)
            .map(|entry| {
                json!({
                    "atMs": entry.at_ms.to_string(),
                    "area": entry.area,
                    "event": entry.event,
                    "detail": entry.detail
                })
            })
            .collect::<Vec<_>>();
        json!({
            "entries": values,
            "persistent": true,
            "redacted": true,
            "currentBytes": self.current_path.metadata().map(|value| value.len()).unwrap_or(0),
            "previousRunAvailable": self.previous_path.is_file()
        })
    }

    pub fn export(&self) -> io::Result<Value> {
        self.export_with_runtime(None)
    }

    pub fn export_with_runtime(&self, runtime: Option<&Value>) -> io::Result<Value> {
        fs::create_dir_all(&self.directory)?;
        let path = self
            .directory
            .join(format!("opennow-diagnostics-{}.txt", now_ms()));
        let temporary = path.with_extension("txt.tmp");
        let mut output = String::from(
            "OpenNOW Qt/Rust diagnostics\nSecrets, URLs, tokens, e-mail addresses and local user paths are redacted.\n\n",
        );
        if let Ok(previous) = fs::read_to_string(&self.previous_path) {
            output.push_str("Previous run\n------------\n");
            output.push_str(&redact(&previous, 500_000));
            output.push_str("\n\n");
        }
        output.push_str("Current run\n-----------\n");
        if let Ok(current) = fs::read_to_string(&self.current_path) {
            output.push_str(&redact(&current, 900_000));
        } else {
            let entries = self.entries.lock().expect("diagnostics poisoned");
            for entry in entries.iter() {
                output.push_str(&format_entry(entry));
            }
        }
        if let Some(runtime) = runtime {
            output.push_str("\n\nStructured runtime snapshot\n---------------------------\n");
            let rendered =
                serde_json::to_string_pretty(runtime).unwrap_or_else(|_| "{}".to_owned());
            output.push_str(&redact(&rendered, 200_000));
            output.push('\n');
        }
        fs::write(&temporary, output.as_bytes())?;
        fs::rename(&temporary, &path)?;
        Ok(json!({
            "path": path.to_string_lossy(),
            "sizeBytes": output.len(),
            "redacted": true,
            "runtimeSnapshotIncluded": runtime.is_some()
        }))
    }

    pub fn export_acceptance(&self, manifest: &Value) -> io::Result<Value> {
        if manifest["schemaVersion"].as_u64() != Some(1)
            || manifest["kind"].as_str() != Some("opennow.live-acceptance")
            || !acceptance_value_is_safe(manifest)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Acceptance manifest is invalid or contains sensitive values",
            ));
        }
        fs::create_dir_all(&self.directory)?;
        let path = self
            .directory
            .join(format!("opennow-live-acceptance-{}.json", now_ms()));
        let temporary = path.with_extension("json.tmp");
        let mut bytes = serde_json::to_vec_pretty(manifest).map_err(io::Error::other)?;
        bytes.push(b'\n');
        fs::write(&temporary, &bytes)?;
        fs::rename(&temporary, &path)?;
        Ok(json!({
            "path": path.to_string_lossy(),
            "sizeBytes": bytes.len(),
            "redacted": true,
            "schemaVersion": 1
        }))
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn clean(value: &str, limit: usize) -> String {
    value
        .chars()
        .filter(|value| !value.is_control())
        .take(limit)
        .collect()
}

fn redact(value: &str, limit: usize) -> String {
    let mut result = String::with_capacity(value.len().min(limit));
    for token in value.split_whitespace() {
        let normalized = token.trim_start_matches(['"', '\'', '{', '[', '(', ',', ':']);
        let lower = normalized.to_ascii_lowercase();
        let sensitive = lower.contains("token")
            || lower.contains("authorization")
            || lower.contains("password")
            || lower.contains("secret")
            || lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("wss://")
            || normalized.contains('@')
            || lower.starts_with("/home/")
            || lower.starts_with("/users/")
            || lower.starts_with("c:\\users\\")
            || lower.starts_with("c:\\\\users\\\\");
        let rendered = if sensitive { "[redacted]" } else { token };
        if !result.is_empty() {
            result.push(' ');
        }
        if result.len() + rendered.len() > limit {
            result.push('…');
            break;
        }
        result.push_str(rendered);
    }
    result
}

fn format_entry(entry: &Entry) -> String {
    format!(
        "{} [{}] {}: {}\n",
        entry.at_ms, entry.area, entry.event, entry.detail
    )
}

fn acceptance_value_is_safe(value: &Value) -> bool {
    match value {
        Value::Object(values) => values.iter().all(|(key, value)| {
            let key = key.to_ascii_lowercase();
            ![
                "token",
                "authorization",
                "password",
                "secret",
                "sessionid",
                "processid",
                "executable",
                "filepath",
                "url",
            ]
            .iter()
            .any(|needle| key.contains(needle))
                && acceptance_value_is_safe(value)
        }),
        Value::Array(values) => values.iter().all(acceptance_value_is_safe),
        Value::String(value) => {
            let lower = value.to_ascii_lowercase();
            !value.contains('@')
                && !lower.starts_with("http://")
                && !lower.starts_with("https://")
                && !lower.starts_with("wss://")
                && !lower.starts_with("/home/")
                && !lower.starts_with("/users/")
                && !lower.starts_with("c:\\users\\")
                && !lower.contains("c:\\\\users\\\\")
        }
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn diagnostics_redact_and_export_atomically() {
        let directory = env::temp_dir().join(format!("opennow-diagnostics-{}", now_ms()));
        let service = DiagnosticsService::new(&directory).unwrap();
        service.record(
            "auth",
            "failure",
            "token=abc user@example.com https://example.com /home/alice/file",
        );
        let exported = service.export().unwrap();
        let text = fs::read_to_string(exported["path"].as_str().unwrap()).unwrap();
        assert!(!text.contains("abc"));
        assert!(!text.contains("user@example.com"));
        assert!(!text.contains("example.com"));
        assert!(!text.contains("alice"));
        assert!(text.contains("[redacted]"));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn diagnostics_export_embeds_a_redacted_structured_runtime_snapshot() {
        let directory = env::temp_dir().join(format!("opennow-runtime-diagnostics-{}", now_ms()));
        let service = DiagnosticsService::new(&directory).unwrap();
        let exported = service
            .export_with_runtime(Some(&json!({
                "kind":"opennow.acceptance",
                "streamer":{"mediaBackend":"ffmpeg","queueDropCount":2},
                "unsafe":"user@example.com /home/alice/recording.mkv C:\\Users\\Alice\\capture.mkv /Users/alice/capture.mkv"
            })))
            .unwrap();
        let text = fs::read_to_string(exported["path"].as_str().unwrap()).unwrap();
        assert_eq!(exported["runtimeSnapshotIncluded"], true);
        assert!(text.contains("Structured runtime snapshot"));
        assert!(text.contains("opennow.acceptance"));
        assert!(text.contains("queueDropCount"));
        assert!(!text.contains("user@example.com"));
        assert!(!text.contains("/home/alice"));
        assert!(!text.contains("Alice"));
        assert!(!text.contains("/Users/alice"));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn live_acceptance_export_is_atomic_machine_readable_and_secret_free() {
        let directory = env::temp_dir().join(format!("opennow-live-acceptance-{}", now_ms()));
        let service = DiagnosticsService::new(&directory).unwrap();
        let manifest = json!({
            "schemaVersion":1,
            "kind":"opennow.live-acceptance",
            "platform":{"os":"linux","cpuArchitecture":"x86_64","windowSystem":"wayland"},
            "stream":{"status":"streaming","firstFrameLatencyMs":1420},
            "media":{"complete":true}
        });
        let exported = service.export_acceptance(&manifest).unwrap();
        let bytes = fs::read(exported["path"].as_str().unwrap()).unwrap();
        let parsed: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(parsed, manifest);
        assert_eq!(exported["redacted"], true);
        assert!(
            service
                .export_acceptance(&json!({
                    "schemaVersion":1,
                    "kind":"opennow.live-acceptance",
                    "sessionId":"secret-session"
                }))
                .is_err()
        );
        assert!(
            service
                .export_acceptance(&json!({
                    "schemaVersion":1,
                    "kind":"opennow.live-acceptance",
                    "message":"/home/alice/private"
                }))
                .is_err()
        );
        let _ = fs::remove_dir_all(directory);
    }
}
