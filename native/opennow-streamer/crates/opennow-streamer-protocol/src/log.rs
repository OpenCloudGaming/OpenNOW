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
use std::time::{SystemTime, UNIX_EPOCH};

/// Matches the Qt shell's rotation policy for `native-streamer.log`.
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;
/// Hot-path failures (per-frame acquire/record) log the first hit and then
/// every Nth repeat so a wedged pipeline cannot flood the disk.
const THROTTLE_EVERY: u64 = 600;

struct LogState {
    file: File,
    path: String,
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
    if let Some(parent) = file_path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
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
    let _ = writeln!(state.file, "{} {level} {area} {flat}", now_ms());
    let _ = state.file.flush();
}

/// Appends one line. No-op until [`set_log_file`] succeeds.
pub fn log_line(level: &str, area: &str, message: &str) {
    write_line(level, area, message);
}

/// Low-frequency pipeline diagnostics must be visible both in standalone stderr
/// captures and in the embedded Qt file sink. Never pass credentials or payloads.
pub fn diagnostic(level: &str, area: &str, message: &str) {
    log_line(level, area, message);
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
        reset_for_tests();
        let path = temp_path("lines.log");
        let _ = std::fs::remove_file(&path);
        set_log_file(&path).unwrap();
        assert_eq!(log_file_path().as_deref(), Some(path.as_str()));
        log_line("INFO", "engine", "hello");
        diagnostic("INFO", "transport", "NVST rx-stats inbound=0 pings=97");
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.contains("INFO engine hello"), "body was: {body}");
        assert!(body.contains("INFO transport NVST rx-stats inbound=0 pings=97"));
        reset_for_tests();
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn oversized_logs_rotate_to_previous() {
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
        reset_for_tests();
        log_line("INFO", "engine", "nowhere");
        log_throttled("noop-key", "WARN", "decode", "nowhere");
    }
}
