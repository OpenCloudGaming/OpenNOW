//! Minimal file logger for the in-process streamer.
//!
//! The Qt shell writes the legacy child-process streamer's stdout to
//! `diagnostics/native-streamer.log`, but the embedded FFI path never spawns
//! that child, so video-pipeline failures were invisible. This module gives
//! the FFI runtime its own append-only sink into the same file.
//!
//! Deliberately dependency-free and redaction-by-construction: call sites
//! pass pre-formatted `area`/`message` strings, so secrets (tokens, URLs)
//! can never reach the log unless a call site formats them in.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Matches the Qt shell's rotation policy for `native-streamer.log`.
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;
/// Hot-path failures (per-frame acquire/record) log the first hit and then
/// every Nth repeat so a wedged pipeline cannot flood the disk.
const THROTTLE_EVERY: u64 = 600;

struct LogState {
    file: File,
    path: String,
    bytes: u64,
}

fn state() -> &'static Mutex<Option<LogState>> {
    static STATE: OnceLock<Mutex<Option<LogState>>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(None))
}

fn throttle_counts() -> &'static Mutex<HashMap<&'static str, u64>> {
    static COUNTS: OnceLock<Mutex<HashMap<&'static str, u64>>> = OnceLock::new();
    COUNTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// Points the file sink at `path`, creating parent directories and rotating
/// a previous log over 2 MiB to `<path>.previous`. Replaces any earlier sink
/// so tests and restarts never append to a stale handle. Safe to call more
/// than once; logging must never break streaming, so failures are returned
/// for the caller to ignore or surface.
pub fn set_log_file(path: &str) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("log file path is empty".to_owned());
    }
    let file_path = Path::new(trimmed);
    if let Some(parent) = file_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create log directory: {error}"))?;
    }
    if file_path.exists() {
        let size = std::fs::metadata(file_path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        if size > MAX_LOG_BYTES {
            let previous = format!("{trimmed}.previous");
            let _ = std::fs::remove_file(&previous);
            let _ = std::fs::rename(file_path, &previous);
        }
    }
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(file_path)
        .map_err(|error| format!("cannot open log file: {error}"))?;
    let mut guard = state().lock().expect("log state poisoned");
    *guard = Some(LogState {
        bytes: file.metadata().map(|metadata| metadata.len()).unwrap_or(0),
        file,
        path: trimmed.to_owned(),
    });
    Ok(())
}

/// Returns the currently configured log path, if any.
pub fn log_file_path() -> Option<String> {
    state()
        .lock()
        .expect("log state poisoned")
        .as_ref()
        .map(|state| state.path.clone())
}

fn write_line(level: &str, area: &str, message: &str) {
    let mut guard = match state().lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    let Some(state) = guard.as_mut() else {
        return;
    };
    // Single-line discipline: embedded newlines would break log parsers.
    let flat: String = message
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .take(2048)
        .collect();
    let line = format!("{} {level} {area} {flat}\n", now_ms());
    if state.bytes + line.len() as u64 > MAX_LOG_BYTES {
        // Keep the append handle valid on Windows, including other existing writers.
        if std::fs::copy(&state.path, format!("{}.previous", state.path)).is_ok()
            && state.file.set_len(0).is_ok()
        {
            state.bytes = 0;
        } else {
            return; // Do not grow an unbounded log if rotation fails.
        }
    }
    if state.file.write_all(line.as_bytes()).is_ok() {
        state.bytes += line.len() as u64;
    }
}

/// Timed, payload-free connection stage. An early return is visible as incomplete.
pub struct Stage {
    name: &'static str,
    started: Instant,
    complete: bool,
}

impl Stage {
    pub fn begin(name: &'static str) -> Self {
        log_line("INFO", "stage", &format!("{name} begin"));
        Self {
            name,
            started: Instant::now(),
            complete: false,
        }
    }

    pub fn complete(&mut self) {
        self.complete = true;
    }
}

impl Drop for Stage {
    fn drop(&mut self) {
        log_line(
            if self.complete { "INFO" } else { "WARN" },
            "stage",
            &format!(
                "{} {} elapsed_ms={}",
                self.name,
                if self.complete {
                    "complete"
                } else {
                    "incomplete"
                },
                self.started.elapsed().as_millis()
            ),
        );
    }
}

/// Only protocol discriminators and numeric counters, never payloads, free-form
/// server messages, session IDs, addresses, credentials, SDP or user paths.
pub fn message_summary(value: &serde_json::Value) -> String {
    let mut fields = Vec::new();
    for key in [
        "id",
        "type",
        "event",
        "status",
        "code",
        "backend",
        "phase",
        "protocolVersion",
        "protocol",
        "paused",
        "fps",
        "width",
        "height",
        "framesReceived",
        "framesDecoded",
        "framesDropped",
    ] {
        let Some(value) = value.get(key) else {
            continue;
        };
        let text = match value {
            serde_json::Value::String(text)
                if text.len() <= 64
                    && text
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "-_. /".contains(c))
                    && !text.contains('/') =>
            {
                text.clone()
            }
            serde_json::Value::Number(_) | serde_json::Value::Bool(_) => value.to_string(),
            _ => continue,
        };
        fields.push(format!("{key}={text}"));
    }
    fields.join(" ")
}

#[cfg(target_os = "android")]
fn write_android_line(level: &str, area: &str, message: &str) {
    use std::ffi::CString;
    #[link(name = "log")]
    unsafe extern "C" {
        fn __android_log_write(
            priority: i32,
            tag: *const std::ffi::c_char,
            text: *const std::ffi::c_char,
        ) -> i32;
    }
    let flat: String = format!("{area}: {message}")
        .chars()
        .take(2048)
        .map(|c| {
            if matches!(c, '\0' | '\r' | '\n') {
                ' '
            } else {
                c
            }
        })
        .collect();
    if let Ok(text) = CString::new(flat) {
        let priority = match level {
            "ERROR" => 6,
            "WARN" => 5,
            _ => 4,
        };
        // Android copies the text during this call; both C strings remain live.
        unsafe {
            __android_log_write(priority, c"OpenNOWNvst".as_ptr(), text.as_ptr());
        }
    }
}

/// Appends one line to the configured file and, on Android, to logcat.
pub fn log_line(level: &str, area: &str, message: &str) {
    #[cfg(target_os = "android")]
    write_android_line(level, area, message);
    write_line(level, area, message);
}

/// Periodic media/transport counters must not wait for disk I/O. The diagnostics
/// worker has a fixed queue; under slow storage it drops traces, never media.
pub fn log_async(level: &'static str, area: &'static str, message: &str) {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::mpsc::{SyncSender, sync_channel};
    type Line = (&'static str, &'static str, String);
    static DROPPED: AtomicU64 = AtomicU64::new(0);
    static SENDER: OnceLock<Option<SyncSender<Line>>> = OnceLock::new();
    let sender = SENDER.get_or_init(|| {
        let (sender, receiver) = sync_channel::<Line>(64);
        std::thread::Builder::new()
            .name("opennow-diagnostics".to_owned())
            .spawn(move || {
                while let Ok((level, area, message)) = receiver.recv() {
                    log_line(level, area, &message);
                    let dropped = DROPPED.swap(0, Ordering::Relaxed);
                    if dropped > 0 {
                        log_line(
                            "WARN",
                            "diagnostics",
                            &format!("trace_queue_dropped={dropped}"),
                        );
                    }
                }
            })
            .ok()
            .map(|_| sender)
    });
    if let Some(sender) = sender
        && sender
            .try_send((level, area, message.chars().take(2048).collect()))
            .is_err()
    {
        DROPPED.fetch_add(1, Ordering::Relaxed);
    }
}

/// Low-frequency pipeline diagnostics must be visible both in standalone stderr
/// captures and in the embedded Qt file sink. Never pass credentials or payloads.
pub fn diagnostic(level: &'static str, area: &'static str, message: &str) {
    log_async(level, area, message);
    eprintln!("{message}");
}

/// Appends the first hit for `key` and then every [`THROTTLE_EVERY`]th
/// repeat, tagging repeats with their running count. For hot paths where
/// the same failure recurs per frame.
pub fn log_throttled(key: &'static str, level: &str, area: &str, message: &str) {
    let count = {
        let mut counts = throttle_counts().lock().expect("log counts poisoned");
        let entry = counts.entry(key).or_insert(0);
        *entry += 1;
        *entry
    };
    if count == 1 {
        write_line(level, area, message);
    } else if count % THROTTLE_EVERY == 0 {
        write_line(level, area, &format!("{message} (repeat #{count})"));
    }
}

#[cfg(test)]
pub(crate) fn reset_for_tests() {
    *state().lock().expect("log state poisoned") = None;
    throttle_counts()
        .lock()
        .expect("log counts poisoned")
        .clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summaries_exclude_payloads_and_untrusted_messages() {
        let summary = message_summary(&serde_json::json!({
            "id":"native-12", "type":"error", "code":"nvst-rtsp-timeout",
            "message":"secret https://private.example", "context":{"token":"credential"},
            "phase":"https://private.example", "width":1920
        }));
        assert!(summary.contains("id=native-12"));
        assert!(summary.contains("code=nvst-rtsp-timeout"));
        assert!(summary.contains("width=1920"));
        for forbidden in ["secret", "private", "credential", "context", "message"] {
            assert!(!summary.contains(forbidden));
        }
    }

    #[test]
    fn active_log_rotation_and_incomplete_stages_are_visible() {
        let _guard = TEST_LOG.lock().unwrap_or_else(|error| error.into_inner());
        reset_for_tests();
        let path = temp_path("active-rotation.log");
        let _ = std::fs::remove_file(&path);
        set_log_file(&path).unwrap();
        for _ in 0..1100 {
            log_line("INFO", "test", &"x".repeat(2048));
        }
        {
            let _stage = Stage::begin("test.incomplete");
        }
        {
            let mut stage = Stage::begin("test.complete");
            stage.complete();
        }
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.len() as u64 <= MAX_LOG_BYTES);
        assert!(Path::new(&format!("{path}.previous")).exists());
        assert!(body.contains("test.incomplete incomplete elapsed_ms="));
        assert!(body.contains("test.complete complete elapsed_ms="));
        reset_for_tests();
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{path}.previous"));
    }

    // These tests replace the process-global log sink and throttle counters.
    // Hold one guard across setup, assertions and teardown, not just each write.
    static TEST_LOG: Mutex<()> = Mutex::new(());

    #[test]
    fn asynchronous_counters_reach_the_sink() {
        let _guard = TEST_LOG.lock().unwrap_or_else(|error| error.into_inner());
        reset_for_tests();
        let path = temp_path("async.log");
        let _ = std::fs::remove_file(&path);
        set_log_file(&path).unwrap();
        log_async("INFO", "transport", "inbound=0 frames=0");
        diagnostic("INFO", "transport", "NVST rx-stats inbound=0 pings=97");
        let deadline = Instant::now() + std::time::Duration::from_secs(2);
        loop {
            let body = std::fs::read_to_string(&path).unwrap();
            if body.contains("inbound=0 frames=0")
                && body.contains("INFO transport NVST rx-stats inbound=0 pings=97")
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "diagnostic worker did not write the trace"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        reset_for_tests();
        let _ = std::fs::remove_file(&path);
    }

    fn temp_path(name: &str) -> String {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "opennow-streamer-log-test-{}-{name}",
            std::process::id()
        ));
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn log_lines_reach_the_configured_file() {
        let _guard = TEST_LOG.lock().unwrap_or_else(|error| error.into_inner());
        reset_for_tests();
        let path = temp_path("lines.log");
        let _ = std::fs::remove_file(&path);
        set_log_file(&path).unwrap();
        assert_eq!(log_file_path().as_deref(), Some(path.as_str()));
        log_line("INFO", "engine", "hello");
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.contains("INFO engine hello"), "body was: {body}");
        reset_for_tests();
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn oversized_logs_rotate_to_previous() {
        let _guard = TEST_LOG.lock().unwrap_or_else(|error| error.into_inner());
        reset_for_tests();
        let path = temp_path("rotate.log");
        let previous = format!("{path}.previous");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&previous);
        std::fs::write(&path, vec![b'x'; (MAX_LOG_BYTES + 8) as usize]).unwrap();
        set_log_file(&path).unwrap();
        assert!(
            std::fs::metadata(&previous).unwrap().len() > MAX_LOG_BYTES,
            "previous log should hold the oversized body"
        );
        assert!(
            std::fs::metadata(&path).unwrap().len() < 1024,
            "fresh log should start near-empty"
        );
        reset_for_tests();
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&previous);
    }

    #[test]
    fn throttled_repeats_stay_bounded() {
        let _guard = TEST_LOG.lock().unwrap_or_else(|error| error.into_inner());
        reset_for_tests();
        let path = temp_path("throttle.log");
        let _ = std::fs::remove_file(&path);
        set_log_file(&path).unwrap();
        for _ in 0..(THROTTLE_EVERY * 2 + 10) {
            log_throttled("unit-test-key", "WARN", "decode", "boom");
        }
        let body = std::fs::read_to_string(&path).unwrap();
        assert_eq!(body.lines().count(), 3, "body was: {body}");
        assert!(body.contains("(repeat #1200)"));
        reset_for_tests();
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn logging_without_a_sink_is_a_silent_noop() {
        let _guard = TEST_LOG.lock().unwrap_or_else(|error| error.into_inner());
        reset_for_tests();
        log_line("INFO", "engine", "nowhere");
        log_throttled("noop-key", "WARN", "decode", "nowhere");
    }
}
