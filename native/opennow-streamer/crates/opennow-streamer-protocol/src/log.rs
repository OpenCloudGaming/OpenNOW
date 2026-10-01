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
        "media",
        "unit",
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
    for key in ["count", "sampleRate", "channels"] {
        if let Some(value) = value.get(key).and_then(serde_json::Value::as_u64) {
            fields.push(format!("{key}={value}"));
        }
    }
    if value["type"] == "log" && value["event"] == "keyframe-request" {
        let code = match value["reason"].as_str() {
            Some("compressed video discontinuity or queue overflow") => {
                Some("compressed-video-discontinuity-or-overflow")
            }
            Some("H.264 decoder rejected an access unit") => Some("decoder-access-unit-rejected"),
            Some("Linux decoder rejected encoded video framing") => Some("invalid-video-framing"),
            Some("embedded Linux decoder queue overflow") => Some("decoder-queue-overflow"),
            Some("embedded Linux video submission failed") => Some("video-submission-failed"),
            Some("Linux decoder requires a fresh keyframe") => Some("decoder-reference-required"),
            Some("Linux decoder device was lost") => Some("decoder-device-lost"),
            Some("Linux decoder fallback requires a fresh keyframe") => Some("decoder-fallback"),
            _ => None,
        };
        if let Some(code) = code {
            fields.push(format!("reasonCode={code}"));
        }
    }
    if value["type"] == "telemetry" {
        if let Some(stage @ ("tracking" | "keyframe-pending" | "recovery-required")) =
            value["decodeProgressStage"].as_str()
        {
            fields.push(format!("decodeProgressStage={stage}"));
        }
        if let Some(stalled) = value["transportFrameProgressStalled"].as_bool() {
            fields.push(format!("transportFrameProgressStalled={stalled}"));
        }
        for (group, keys) in [
            (
                "decodeTimings",
                [
                    "epoch",
                    "submissionsTotal",
                    "outputsTotal",
                    "outputCallsTotal",
                    "inFlight",
                    "unmatchedOutputs",
                    "unmatchedSubmissions",
                ],
            ),
            (
                "frameStageTimings",
                [
                    "assembledFramesTotal",
                    "admittedFramesTotal",
                    "queuedAckFramesTotal",
                    "undeliveredFramesTotal",
                    "pendingDeliveries",
                    "unmatchedDeliveries",
                    "unmatchedAdmissions",
                ],
            ),
        ] {
            for key in keys {
                if let Some(counter) = value
                    .get(group)
                    .and_then(|timings| timings.get(key))
                    .and_then(serde_json::Value::as_u64)
                {
                    fields.push(format!("{group}.{key}={counter}"));
                }
            }
        }
    }
    fields.join(" ")
}

/// Appends one line. No-op until [`set_log_file`] succeeds.
pub fn log_line(level: &str, area: &str, message: &str) {
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
                    write_line(level, area, &message);
                    let dropped = DROPPED.swap(0, Ordering::Relaxed);
                    if dropped > 0 {
                        write_line(
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
    fn queue_drop_summaries_include_only_safe_units_and_numeric_counts() {
        let summary = message_summary(&serde_json::json!({
            "type": "log", "event": "queue-dropped", "media": "audio-output",
            "unit": "samples", "count": 96000, "sampleRate": 48000, "channels": 2,
            "message": "private payload", "context": {"token": "credential"}
        }));
        for field in [
            "media=audio-output",
            "unit=samples",
            "count=96000",
            "sampleRate=48000",
            "channels=2",
        ] {
            assert!(summary.contains(field), "{summary}");
        }
        for forbidden in ["private", "payload", "credential", "context", "message"] {
            assert!(!summary.contains(forbidden));
        }
        let summary = message_summary(&serde_json::json!({
            "media": "https://private.example", "unit": "invalid\nunit",
            "count": "credential", "sampleRate": -1, "channels": {"token": "secret"}
        }));
        assert!(summary.is_empty(), "{summary}");
    }

    #[test]
    fn telemetry_summaries_preserve_typed_pipeline_progress() {
        let summary = message_summary(&serde_json::json!({
            "type": "telemetry",
            "decodeProgressStage": "keyframe-pending",
            "transportFrameProgressStalled": false,
            "frameStageTimings": {
                "assembledFramesTotal": 900, "admittedFramesTotal": 897,
                "queuedAckFramesTotal": 896, "undeliveredFramesTotal": 1,
                "pendingDeliveries": 2, "unmatchedDeliveries": 1,
                "unmatchedAdmissions": 3
            },
            "decodeTimings": {
                "epoch": 2, "submissionsTotal": 850, "outputsTotal": 820,
                "outputCallsTotal": 800, "inFlight": 30,
                "unmatchedOutputs": 1, "unmatchedSubmissions": 2
            }
        }));
        for field in [
            "decodeProgressStage=keyframe-pending",
            "transportFrameProgressStalled=false",
            "frameStageTimings.assembledFramesTotal=900",
            "frameStageTimings.admittedFramesTotal=897",
            "frameStageTimings.queuedAckFramesTotal=896",
            "frameStageTimings.undeliveredFramesTotal=1",
            "frameStageTimings.pendingDeliveries=2",
            "frameStageTimings.unmatchedDeliveries=1",
            "frameStageTimings.unmatchedAdmissions=3",
            "decodeTimings.epoch=2",
            "decodeTimings.submissionsTotal=850",
            "decodeTimings.outputsTotal=820",
            "decodeTimings.outputCallsTotal=800",
            "decodeTimings.inFlight=30",
            "decodeTimings.unmatchedOutputs=1",
            "decodeTimings.unmatchedSubmissions=2",
        ] {
            assert!(summary.split(' ').any(|entry| entry == field), "{summary}");
        }
    }

    #[test]
    fn keyframe_summaries_map_only_exact_known_reasons_to_codes() {
        for (reason, code) in [
            (
                "compressed video discontinuity or queue overflow",
                "compressed-video-discontinuity-or-overflow",
            ),
            (
                "H.264 decoder rejected an access unit",
                "decoder-access-unit-rejected",
            ),
            (
                "Linux decoder rejected encoded video framing",
                "invalid-video-framing",
            ),
            (
                "embedded Linux decoder queue overflow",
                "decoder-queue-overflow",
            ),
            (
                "embedded Linux video submission failed",
                "video-submission-failed",
            ),
            (
                "Linux decoder requires a fresh keyframe",
                "decoder-reference-required",
            ),
            ("Linux decoder device was lost", "decoder-device-lost"),
            (
                "Linux decoder fallback requires a fresh keyframe",
                "decoder-fallback",
            ),
        ] {
            let summary = message_summary(&serde_json::json!({
                "type": "log", "event": "keyframe-request", "reason": reason,
                "message": "private payload", "sessionId": "secret"
            }));
            assert_eq!(
                summary,
                format!("type=log event=keyframe-request reasonCode={code}")
            );
        }
        for reason in [
            "credential",
            "https://private.example",
            "/home/private/file",
            "embedded Linux decoder queue overflow\nsecret",
            "embedded Linux decoder queue overflow secret",
        ] {
            let summary = message_summary(&serde_json::json!({
                "type": "log", "event": "keyframe-request", "reason": reason,
                "reasonCode": "credential"
            }));
            assert_eq!(summary, "type=log event=keyframe-request");
        }
        for value in [
            serde_json::json!({"reason": "embedded Linux decoder queue overflow"}),
            serde_json::json!({"type": "error", "event": "keyframe-request", "reason": "embedded Linux decoder queue overflow"}),
            serde_json::json!({"type": "log", "event": "backend-fallback", "reason": "embedded Linux decoder queue overflow"}),
        ] {
            assert!(!message_summary(&value).contains("reasonCode="));
        }
    }

    #[test]
    fn pipeline_summaries_reject_untrusted_types_and_nested_payloads() {
        for invalid in [
            serde_json::json!("credential"),
            serde_json::json!("123"),
            serde_json::json!(-1),
            serde_json::json!(1.5),
            serde_json::json!(true),
            serde_json::json!(null),
            serde_json::json!([1]),
            serde_json::json!({"token": "secret"}),
        ] {
            let summary = message_summary(&serde_json::json!({
                "type": "telemetry", "decodeProgressStage": invalid,
                "transportFrameProgressStalled": invalid,
                "frameStageTimings": {
                    "assembledFramesTotal": invalid, "admittedFramesTotal": invalid,
                    "queuedAckFramesTotal": invalid, "undeliveredFramesTotal": invalid,
                    "pendingDeliveries": invalid, "unmatchedDeliveries": invalid,
                    "unmatchedAdmissions": invalid
                },
                "decodeTimings": {
                    "epoch": invalid, "submissionsTotal": invalid, "outputsTotal": invalid,
                    "outputCallsTotal": invalid, "inFlight": invalid,
                    "unmatchedOutputs": invalid, "unmatchedSubmissions": invalid
                }
            }));
            let expected = if invalid.is_boolean() {
                "type=telemetry transportFrameProgressStalled=true"
            } else {
                "type=telemetry"
            };
            assert_eq!(summary, expected);
        }
        let summary = message_summary(&serde_json::json!({
            "type": "telemetry", "decodeProgressStage": "credential",
            "startId": "secret", "sessionId": "secret", "accountId": "secret",
            "reason": "private", "context": {"token": "credential"},
            "frameStageTimings": {"assembledFramesTotal": 0, "url": "https://private.example"},
            "decodeTimings": {"outputsTotal": 0, "path": "/home/private/file", "token": "credential"}
        }));
        assert_eq!(
            summary,
            "type=telemetry decodeTimings.outputsTotal=0 frameStageTimings.assembledFramesTotal=0"
        );
        for group in [
            serde_json::json!(null),
            serde_json::json!("secret"),
            serde_json::json!([1]),
        ] {
            assert_eq!(
                message_summary(&serde_json::json!({
                    "type": "telemetry", "decodeTimings": group, "frameStageTimings": group
                })),
                "type=telemetry"
            );
        }
        assert_eq!(
            message_summary(&serde_json::json!({
                "type": "log", "decodeProgressStage": "tracking",
                "transportFrameProgressStalled": true,
                "decodeTimings": {"inFlight": 1}, "frameStageTimings": {"pendingDeliveries": 1}
            })),
            "type=log"
        );
    }

    #[test]
    fn pipeline_summary_size_is_bounded_and_missing_values_stay_unavailable() {
        let mut telemetry = serde_json::json!({"type": "telemetry"});
        assert_eq!(message_summary(&telemetry), "type=telemetry");
        telemetry["decodeTimings"] = serde_json::json!({
            "epoch": u64::MAX, "submissionsTotal": u64::MAX, "outputsTotal": u64::MAX,
            "outputCallsTotal": u64::MAX, "inFlight": u64::MAX,
            "unmatchedOutputs": u64::MAX, "unmatchedSubmissions": u64::MAX
        });
        telemetry["frameStageTimings"] = serde_json::json!({
            "assembledFramesTotal": u64::MAX, "admittedFramesTotal": u64::MAX,
            "queuedAckFramesTotal": u64::MAX, "undeliveredFramesTotal": u64::MAX,
            "pendingDeliveries": u64::MAX, "unmatchedDeliveries": u64::MAX,
            "unmatchedAdmissions": u64::MAX
        });
        telemetry["decodeProgressStage"] = serde_json::json!("recovery-required");
        telemetry["transportFrameProgressStalled"] = serde_json::json!(true);
        let summary = message_summary(&telemetry);
        assert!(summary.len() < 1024, "{}", summary.len());
        assert_eq!(summary.split(' ').count(), 17);
        assert!(summary.contains(&format!("decodeTimings.outputsTotal={}", u64::MAX)));
        telemetry["decodeTimings"]["private"] = serde_json::json!("secret".repeat(10_000));
        assert_eq!(message_summary(&telemetry), summary);
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
