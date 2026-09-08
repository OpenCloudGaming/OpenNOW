use serde_json::{Value, json};
use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const ENTRY_LIMIT: usize = 1_200;
const LOG_LIMIT_BYTES: u64 = 1_500_000;

pub fn embedded_drop_evidence(params: &Value) -> Value {
    let mut evidence = serde_json::Map::new();
    for section in ["embeddedStream", "lastSessionReport"] {
        let Some(source) = params[section]["drops"].as_object() else {
            continue;
        };
        let mut drops = serde_json::Map::new();
        for field in [
            "videoDropCount",
            "audioDiscardedMs",
            "audioPacketDropCount",
            "callbackDropCount",
            "otherQueueDropCount",
        ] {
            if let Some(value) = source.get(field).filter(|value| {
                value.as_f64().is_some_and(|number| {
                    (0.0..=9_007_199_254_740_991.0).contains(&number)
                        && (field == "audioDiscardedMs" || number.fract() == 0.0)
                })
            }) {
                drops.insert(field.to_owned(), value.clone());
            }
        }
        evidence.insert(section.to_owned(), json!({"drops": drops}));
    }
    Value::Object(evidence)
}

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
        let mut values = entries
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
        drop(entries);
        // The in-app diagnostics screen uses this same bounded entry contract.
        // Read adjacent embedded-runtime traces only on an explicit snapshot request.
        for name in ["qt-native.log", "native-streamer.log"] {
            if let Ok(tail) = native_log_tail(&self.directory.join(name)) {
                for line in tail.lines().rev().take(60) {
                    let Some((timestamp, detail)) = line.split_once(' ') else {
                        continue;
                    };
                    let Ok(at_ms) = timestamp.parse::<u128>() else {
                        continue;
                    };
                    values.push(json!({
                        "atMs":at_ms.to_string(), "area":name,
                        "event":"trace", "detail":redact(detail, 480)
                    }));
                }
            }
        }
        values.sort_by_key(|entry| {
            std::cmp::Reverse(
                entry["atMs"]
                    .as_str()
                    .and_then(|value| value.parse::<u128>().ok())
                    .unwrap_or(0),
            )
        });
        values.truncate(200);
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
            output.push_str(&redact_lines(&previous, 500_000));
            output.push_str("\n\n");
        }
        output.push_str("Current run\n-----------\n");
        if let Ok(current) = fs::read_to_string(&self.current_path) {
            output.push_str(&redact_lines(&current, 900_000));
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
            output.push_str(&redact_lines(&rendered, 200_000));
            output.push('\n');
        }
        // The embedded streamer does not run as a child of the core. Include its
        // adjacent file sink explicitly, bounded and redacted like the RPC log.
        for name in [
            "native-streamer.log",
            "native-streamer.log.previous",
            "native-streamer.previous.log",
            "qt-native.log",
            "qt-native.log.previous",
        ] {
            if let Ok(tail) = native_log_tail(&self.directory.join(name)) {
                output.push_str(&format!(
                    "\n\nNative media: {name}\n---------------------------\n"
                ));
                output.push_str(&redact_lines(&tail, 262_144));
            }
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

fn native_log_tail(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let start = file.metadata()?.len().saturating_sub(262_144);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    file.take(262_144).read_to_end(&mut bytes)?;
    let text = String::from_utf8_lossy(&bytes);
    Ok(if start > 0 {
        text.split_once('\n')
            .map_or("", |(_, tail)| tail)
            .to_owned()
    } else {
        text.into_owned()
    })
}

fn redact_lines(value: &str, limit: usize) -> String {
    let mut output = String::new();
    for line in value.lines() {
        let remaining = limit.saturating_sub(output.len());
        if remaining < 4 {
            break;
        }
        let rendered = redact(line, remaining - 4);
        output.push_str(&rendered);
        output.push('\n');
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn diagnostics_export_includes_bounded_redacted_native_log() {
        let directory = env::temp_dir().join(format!("opennow-native-diagnostics-{}", now_ms()));
        let service = DiagnosticsService::new(&directory).unwrap();
        let path = directory.join("diagnostics/native-streamer.log");
        fs::write(
            &path,
            format!(
                "{}\ndecoder initialization failed token=secret https://example.com\n",
                "x".repeat(300_000)
            ),
        )
        .unwrap();
        let tail = native_log_tail(&path).unwrap();
        assert!(tail.len() < 262_144);
        let exported = service.export().unwrap();
        let text = fs::read_to_string(exported["path"].as_str().unwrap()).unwrap();
        assert!(text.contains("Native media: native-streamer.log"));
        assert!(text.contains("decoder initialization failed"));
        assert!(!text.contains("secret"));
        assert!(!text.contains("example.com"));
        let _ = fs::remove_dir_all(directory);
    }

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
        fs::write(
            directory.join("diagnostics/qt-native.log"),
            "first handshake\nsecond user@example.com\n",
        )
        .unwrap();
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
        assert!(text.contains("qt-native.log"));
        assert!(text.contains("first handshake\nsecond [redacted]\n"));
        assert!(!text.contains("user@example.com"));
        assert!(!text.contains("/home/alice"));
        assert!(!text.contains("Alice"));
        assert!(!text.contains("/Users/alice"));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn embedded_drop_evidence_preserves_only_bounded_typed_counters() {
        let evidence = embedded_drop_evidence(&json!({
            "embeddedStream": {"drops": {
                "videoDropCount": 7, "audioDiscardedMs": 50.5,
                "audioPacketDropCount": 2, "callbackDropCount": 3,
                "otherQueueDropCount": 1, "token": "must-not-be-exported"
            }},
            "lastSessionReport": {"drops": {
                "videoDropCount": -1, "audioDiscardedMs": "50",
                "audioPacketDropCount": 1.5, "callbackDropCount": 1e100,
                "otherQueueDropCount": null
            }, "sessionId": "must-not-be-exported"}
        }));
        assert_eq!(
            evidence["embeddedStream"]["drops"],
            json!({
                "videoDropCount": 7, "audioDiscardedMs": 50.5,
                "audioPacketDropCount": 2, "callbackDropCount": 3,
                "otherQueueDropCount": 1
            })
        );
        assert_eq!(evidence["lastSessionReport"], json!({"drops": {}}));
        assert_eq!(embedded_drop_evidence(&json!({})), json!({}));
    }

    #[test]
    fn diagnostics_export_retains_embedded_drops_after_native_stop() {
        let directory = env::temp_dir().join(format!("opennow-drop-diagnostics-{}", now_ms()));
        let service = DiagnosticsService::new(&directory).unwrap();
        let runtime = json!({
            "streamer": {"status": "stopped", "queueDropCount": 0},
            "shell": embedded_drop_evidence(&json!({
                "embeddedStream": {"drops": {"videoDropCount": 7, "audioDiscardedMs": 50}},
                "lastSessionReport": {"drops": {"videoDropCount": 7, "audioDiscardedMs": 50}}
            }))
        });
        let exported = service.export_with_runtime(Some(&runtime)).unwrap();
        let text = fs::read_to_string(exported["path"].as_str().unwrap()).unwrap();
        assert!(text.contains("\"videoDropCount\": 7"));
        assert!(text.contains("\"audioDiscardedMs\": 50"));
        assert!(text.contains("\"lastSessionReport\""));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn snapshot_includes_bounded_native_and_qt_handshake_entries() {
        let directory = env::temp_dir().join(format!("opennow-handshake-snapshot-{}", now_ms()));
        let service = DiagnosticsService::new(&directory).unwrap();
        let log = (1..=250)
            .map(|i| format!("{i} qt-native delivered type=ready user@example.com\n"))
            .collect::<String>();
        fs::write(directory.join("diagnostics/qt-native.log"), log).unwrap();
        let snapshot = service.snapshot();
        let entries = snapshot["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 60);
        assert_eq!(entries[0]["atMs"], "250");
        assert!(
            entries[0]["detail"]
                .as_str()
                .unwrap()
                .contains("type=ready")
        );
        assert!(!snapshot.to_string().contains("user@example.com"));
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
