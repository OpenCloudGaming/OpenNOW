#[cfg(all(test, unix))]
use rand::RngCore as _;
use serde_json::{Value, json};
use std::fs;
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const STREAMER_PROTOCOL_VERSION: u64 = 7;
const CHILD_MESSAGE_LIMIT: usize = 1024 * 1024;
const CHILD_START_TIMEOUT: Duration = Duration::from_secs(90);
const CAPABILITY_PROBE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug)]
pub struct StreamerError {
    pub code: &'static str,
    pub message: String,
}

#[derive(Clone)]
struct Snapshot {
    status: String,
    message: String,
    session_id: Option<String>,
    process_id: Option<u64>,
    transport: Option<String>,
    capabilities: Value,
    executable: Option<PathBuf>,
    error_code: Option<String>,
    overlay_request_generation: u64,
    screenshot_request_generation: u64,
    recording_toggle_request_generation: u64,
    shortcut_action_generation: u64,
    shortcut_action: Option<String>,
    microphone_state: String,
    microphone_enabled: bool,
    microphone_message: Option<String>,
    started_at: Option<Instant>,
    session_started_at_ms: Option<u128>,
    first_frame_latency_ms: Option<u64>,
    media_backend: Option<String>,
    frames_per_second: Option<f64>,
    bitrate_mbps: Option<f64>,
    peak_bitrate_mbps: Option<f64>,
    backend_fallback_count: u64,
    decoder_error_count: u64,
    output_error_count: u64,
    device_loss_count: u64,
    device_recovery_count: u64,
    queue_drop_count: u64,
    input_ready: bool,
    input_unavailable_reason: Option<String>,
    input_pause_count: u64,
    input_resume_count: u64,
    surface_update_count: u64,
    fullscreen_toggle_count: u64,
    stats_toggle_count: u64,
    recording_start_count: u64,
    recording_stop_count: u64,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            status: "stopped".to_owned(),
            message: "Native NVST streamer is not running".to_owned(),
            session_id: None,
            process_id: None,
            transport: None,
            capabilities: Value::Null,
            executable: None,
            error_code: None,
            overlay_request_generation: 0,
            screenshot_request_generation: 0,
            recording_toggle_request_generation: 0,
            shortcut_action_generation: 0,
            shortcut_action: None,
            microphone_state: "disabled".to_owned(),
            microphone_enabled: false,
            microphone_message: None,
            started_at: None,
            session_started_at_ms: None,
            first_frame_latency_ms: None,
            media_backend: None,
            frames_per_second: None,
            bitrate_mbps: None,
            peak_bitrate_mbps: None,
            backend_fallback_count: 0,
            decoder_error_count: 0,
            output_error_count: 0,
            device_loss_count: 0,
            device_recovery_count: 0,
            queue_drop_count: 0,
            input_ready: false,
            input_unavailable_reason: None,
            input_pause_count: 0,
            input_resume_count: 0,
            surface_update_count: 0,
            fullscreen_toggle_count: 0,
            stats_toggle_count: 0,
            recording_start_count: 0,
            recording_stop_count: 0,
        }
    }
}

impl Snapshot {
    fn value(&self) -> Value {
        json!({"streamer":{
            "status":self.status,
            "message":self.message,
            "sessionId":self.session_id,
            "processId":self.process_id,
            "transport":self.transport,
            "capabilities":self.capabilities,
            "executable":self.executable.as_ref().map(|path| path.to_string_lossy().into_owned()),
            "errorCode":self.error_code,
            "overlayRequestGeneration":self.overlay_request_generation,
            "screenshotRequestGeneration":self.screenshot_request_generation,
            "recordingToggleRequestGeneration":self.recording_toggle_request_generation,
            "shortcutActionGeneration":self.shortcut_action_generation,
            "shortcutAction":self.shortcut_action,
            "microphoneState":self.microphone_state,
            "microphoneEnabled":self.microphone_enabled,
            "microphoneMessage":self.microphone_message,
            "sessionUptimeMs":self.started_at.map(|started| elapsed_millis(started.elapsed())),
            "sessionStartedAtMs":self.session_started_at_ms.map(|value| value.to_string()),
            "firstFrameLatencyMs":self.first_frame_latency_ms,
            "mediaBackend":self.media_backend,
            "framesPerSecond":self.frames_per_second,
            "bitrateMbps":self.bitrate_mbps,
            "peakBitrateMbps":self.peak_bitrate_mbps,
            "backendFallbackCount":self.backend_fallback_count,
            "decoderErrorCount":self.decoder_error_count,
            "outputErrorCount":self.output_error_count,
            "deviceLossCount":self.device_loss_count,
            "deviceRecoveryCount":self.device_recovery_count,
            "queueDropCount":self.queue_drop_count,
            "inputReady":self.input_ready,
            "inputUnavailableReason":self.input_unavailable_reason,
            "inputPauseCount":self.input_pause_count,
            "inputResumeCount":self.input_resume_count,
            "surfaceUpdateCount":self.surface_update_count,
            "fullscreenToggleCount":self.fullscreen_toggle_count,
            "statsToggleCount":self.stats_toggle_count,
            "recordingStartCount":self.recording_start_count,
            "recordingStopCount":self.recording_stop_count
        }})
    }

    fn acceptance_value(&self) -> Value {
        let available_video_backends = self.capabilities["videoBackends"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|backend| backend["available"].as_bool().unwrap_or(false))
            .filter_map(|backend| backend["backend"].as_str())
            .collect::<Vec<_>>();
        json!({
            "schemaVersion": 1,
            "kind": "opennow.stream.acceptance",
            "status": self.status,
            "transport": self.transport,
            "errorCode": self.error_code,
            "sessionUptimeMs": self.started_at.map(|started| elapsed_millis(started.elapsed())),
            "sessionStartedAtMs": self.session_started_at_ms.map(|value| value.to_string()),
            "firstFrameLatencyMs": self.first_frame_latency_ms,
            "mediaBackend": self.media_backend,
            "framesPerSecond": self.frames_per_second,
            "bitrateMbps": self.bitrate_mbps,
            "peakBitrateMbps": self.peak_bitrate_mbps,
            "backendFallbackCount": self.backend_fallback_count,
            "decoderErrorCount": self.decoder_error_count,
            "outputErrorCount": self.output_error_count,
            "deviceLossCount": self.device_loss_count,
            "deviceRecoveryCount": self.device_recovery_count,
            "queueDropCount": self.queue_drop_count,
            "inputReady": self.input_ready,
            "inputUnavailableReason": self.input_unavailable_reason,
            "microphoneState": self.microphone_state,
            "microphoneEnabled": self.microphone_enabled,
            "inputPauseCount": self.input_pause_count,
            "inputResumeCount": self.input_resume_count,
            "surfaceUpdateCount": self.surface_update_count,
            "fullscreenToggleCount": self.fullscreen_toggle_count,
            "statsToggleCount": self.stats_toggle_count,
            "recordingStartCount": self.recording_start_count,
            "recordingStopCount": self.recording_stop_count,
            "availableVideoBackends": available_video_backends,
            "availableCodecs": available_codecs(&self.capabilities)
        })
    }
}

struct Worker {
    control: Sender<WorkerCommand>,
    join: JoinHandle<()>,
}

struct KillOnDrop(Child);

impl Deref for KillOnDrop {
    type Target = Child;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for KillOnDrop {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

enum WorkerCommand {
    Stop(String),
    InputPaused(bool),
    Control(String),
    Surface(Value),
    Recording {
        enabled: bool,
        output_path: Option<PathBuf>,
        reply: mpsc::SyncSender<Result<Value, StreamerError>>,
    },
}

pub struct StreamerService {
    state: Arc<Mutex<Snapshot>>,
    worker: Mutex<Option<Worker>>,
}

impl StreamerService {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(Snapshot::default())),
            worker: Mutex::new(None),
        }
    }

    pub fn detect(&self, settings: &Value) -> Result<Value, StreamerError> {
        let executable = resolve_executable(settings)?;
        let metadata = fs::metadata(&executable).map_err(|error| StreamerError {
            code: "streamer_not_found",
            message: format!(
                "Native streamer was not found at {}: {error}",
                executable.display()
            ),
        })?;
        if !metadata.is_file() {
            return Err(StreamerError {
                code: "streamer_not_found",
                message: format!(
                    "Native streamer path is not a file: {}",
                    executable.display()
                ),
            });
        }
        let capabilities = probe_capabilities(&executable, settings)?;
        let available_codecs = available_codecs(&capabilities);
        Ok(json!({
            "available":true,
            "protocolVersion":STREAMER_PROTOCOL_VERSION,
            "path":executable.to_string_lossy(),
            "sizeBytes":metadata.len(),
            "capabilities":capabilities,
            "availableCodecs":available_codecs
        }))
    }

    pub fn validate_codec(&self, settings: &Value) -> Result<(), StreamerError> {
        if settings["enableHdr"].as_bool() == Some(true) {
            return Err(invalid(
                "HDR requires current embedded window output capabilities",
            ));
        }
        let requested = settings["codec"]
            .as_str()
            .unwrap_or("auto")
            .trim()
            .to_ascii_lowercase();
        // Auto currently requests the portable H.264 baseline from CloudMatch. Probe it too:
        // an explicit hardware-only decoder policy can make even that baseline unavailable.
        let codec = match requested.as_str() {
            "" | "auto" => "h264",
            _ => normalize_codec_name(&requested).ok_or_else(|| invalid("Unknown video codec"))?,
        };
        let detection = self.detect(settings)?;
        ensure_codec_available(&detection["capabilities"], codec)
    }

    /// The in-process Qt streamer, not a separately installed executable, owns the usable
    /// decode/presentation capabilities. Resolve Auto before asking CloudMatch for a seat.
    pub fn embedded_session_settings(
        settings: &Value,
        capabilities: &Value,
    ) -> Result<Value, StreamerError> {
        if capabilities["protocolVersion"].as_u64() != Some(STREAMER_PROTOCOL_VERSION) {
            return Err(invalid(
                "Embedded streamer capabilities are missing or incompatible",
            ));
        }
        let hdr = settings["enableHdr"].as_bool() == Some(true);
        if hdr && capabilities["nativeHdrSupported"].as_bool() != Some(true) {
            return Err(invalid(
                "HDR requires an active HDR-capable window output. Disable HDR or select a supported display.",
            ));
        }
        if hdr && settings["decoderPreference"].as_str() == Some("software") {
            return Err(invalid(
                "HDR requires a 10-bit hardware decoder; software decoding is not supported",
            ));
        }
        let requested_backend = settings["nativeVideoBackend"].as_str().unwrap_or("auto");
        let mut selected = capabilities.clone();
        let backends = selected["videoBackends"]
            .as_array_mut()
            .ok_or_else(|| invalid("Embedded streamer did not report video backends"))?;
        for backend in backends.iter_mut() {
            let name = backend["backend"].as_str().unwrap_or("");
            let matches = !matches!(name, "software" | "ffmpeg")
                && (requested_backend == "auto"
                    || requested_backend == name
                    || (requested_backend == "nvdec" && name == "cuda"));
            if !matches {
                backend["available"] = json!(false);
            }
        }
        if !backends
            .iter()
            .any(|backend| backend["available"].as_bool() == Some(true))
        {
            let message = if requested_backend == "auto" {
                "No hardware video backend is available for embedded streaming on this device. Check the hardware drivers and export diagnostics for backend probe failures.".to_owned()
            } else {
                let mut message = format!(
                    "The {} backend is unavailable for embedded streaming on this device. Select Auto in Stream settings.",
                    crate::diagnostics::runtime_failure_reason(requested_backend)
                );
                let evidence = crate::diagnostics::native_runtime_evidence(capabilities);
                if let Some(reason) = evidence["videoBackends"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|backend| {
                        backend["backend"] == requested_backend
                            || (requested_backend == "nvdec" && backend["backend"] == "cuda")
                    })
                    .and_then(|backend| backend["reason"].as_str())
                    .filter(|reason| !reason.is_empty())
                {
                    message.push_str(&format!(" {reason}"));
                }
                message
            };
            return Err(StreamerError {
                code: "streamer_backend_unavailable",
                message,
            });
        }
        let videotoolbox = backends.iter().any(|backend| {
            backend["backend"] == "videotoolbox"
                && backend["platform"] == "macos"
                && backend["available"].as_bool() == Some(true)
        });
        let color = match (hdr, settings["colorQuality"].as_str().unwrap_or("8bit_420")) {
            (true, "8bit_444" | "10bit_444") => "10bit_444",
            (true, _) => "10bit_420",
            (false, color) => color,
        };
        if videotoolbox
            && !matches!(color, "8bit_420" | "10bit_420")
            && !backends.iter().any(|backend| {
                backend["backend"] == "videotoolbox"
                    && backend["available"].as_bool() == Some(true)
                    && backend["codecs"].as_array().is_some_and(|codecs| {
                        codecs.iter().any(|codec| {
                            codec["available"].as_bool() == Some(true)
                                && normalize_codec_name(codec["codec"].as_str().unwrap_or(""))
                                    == Some("h265")
                                && codec["colorQualities"].as_array().is_some_and(|qualities| {
                                    qualities
                                        .iter()
                                        .any(|quality| quality.as_str() == Some(color))
                                })
                        })
                    })
            })
        {
            return Err(StreamerError {
                code: "streamer_color_unavailable",
                message: "The macOS hardware decoder does not support the requested 4:4:4 profile. Select 4:2:0 in Stream settings.".to_owned(),
            });
        }
        for backend in backends.iter_mut() {
            let requires_profiles = hdr
                || backend["backend"] == "vulkan"
                || backend["platform"] == "linux"
                || backend["backend"] == "videotoolbox";
            for codec in backend["codecs"].as_array_mut().into_iter().flatten() {
                let hdr_supported = codec
                    .get("hdrSupported")
                    .map(|value| value.as_bool().unwrap_or(false));
                let wire_supported =
                    match normalize_codec_name(codec["codec"].as_str().unwrap_or("")) {
                        Some("h264") => color == "8bit_420",
                        Some("av1") => matches!(color, "8bit_420" | "10bit_420"),
                        Some("h265") => true,
                        _ => false,
                    };
                let supported = wire_supported
                    && (!hdr || hdr_supported != Some(false))
                    && (!hdr
                        || match codec.get("hdrColorQualities") {
                            Some(qualities) => qualities.as_array().is_some_and(|qualities| {
                                qualities
                                    .iter()
                                    .any(|quality| quality.as_str() == Some(color))
                            }),
                            None => color == "10bit_420",
                        })
                    && match codec.get("colorQualities") {
                        Some(qualities) => qualities.as_array().is_some_and(|qualities| {
                            qualities
                                .iter()
                                .any(|quality| quality.as_str() == Some(color))
                        }),
                        None => {
                            (hdr && color == "10bit_420" && hdr_supported == Some(true))
                                || !requires_profiles
                                || color == "8bit_420"
                        }
                    };
                if !supported {
                    codec["available"] = json!(false);
                    codec["reason"] = if wire_supported {
                        json!(format!(
                            "The embedded decoder does not support the requested {color} color mode"
                        ))
                    } else {
                        json!(format!(
                            "The selected GFN codec cannot request {color}. Select Auto or H.265 for advanced color."
                        ))
                    };
                }
            }
        }
        let requested = settings["codec"]
            .as_str()
            .unwrap_or("auto")
            .to_ascii_lowercase();
        let mut resolved = settings.clone();
        if hdr && requested != "auto" && normalize_codec_name(&requested) == Some("h264") {
            return Err(invalid(
                "HDR requires HEVC or AV1 with 10-bit hardware decoding; H.264 is SDR-only",
            ));
        }
        let codec = if requested == "auto" {
            let candidates: &[&str] = match color {
                _ if hdr => &["h265", "av1"],
                "8bit_444" | "10bit_444" => &["h265"],
                "10bit_420" if videotoolbox => &["h265", "av1"],
                _ if videotoolbox => macos_auto_codec_candidates(settings),
                "10bit_420" => &["av1", "h265"],
                _ => &["av1", "h265", "h264"],
            };
            candidates.iter().find(|codec| codec_available(&selected, codec)).copied()
                .ok_or_else(|| StreamerError { code: "streamer_codec_unavailable",
                    message: "No available hardware codec supports the requested color mode. Try 8-bit 4:2:0 in Stream settings.".to_owned() })?
        } else {
            let codec =
                normalize_codec_name(&requested).ok_or_else(|| invalid("Unknown video codec"))?;
            ensure_codec_available(&selected, codec)?;
            codec
        };
        resolved["codec"] = json!(codec);
        resolved["nativeHdrSupported"] = json!(
            capabilities["nativeHdrSupported"]
                .as_bool()
                .unwrap_or(false)
        );
        if hdr {
            resolved["colorQuality"] = json!(color);
        }
        Ok(resolved)
    }

    pub fn prepare_embedded(
        &self,
        params: &Value,
        settings: &Value,
    ) -> Result<Value, StreamerError> {
        let session = params["session"]
            .as_object()
            .map(|_| params["session"].clone())
            .ok_or_else(|| invalid("streamer.prepare requires a ready session"))?;
        let status = session["status"].as_i64().unwrap_or_default();
        if !matches!(status, 2 | 3) {
            return Err(invalid(
                "CloudMatch session is not ready for NVST media attachment",
            ));
        }
        let mut context = streamer_context(session, settings);
        if context["settings"]["enableHdr"].as_bool() == Some(true)
            && !matches!(
                (
                    context["session"]["negotiatedStreamProfile"]["codec"].as_str(),
                    context["session"]["negotiatedStreamProfile"]["colorQuality"].as_str(),
                ),
                (Some("H265" | "HEVC"), Some("10bit_420" | "10bit_444"))
                    | (Some("AV1"), Some("10bit_420"))
            )
        {
            return Err(invalid(
                "The accepted HDR session requires 10-bit HEVC 4:2:0/4:4:4 or AV1 4:2:0",
            ));
        }
        if context["settings"]["enableHdr"].as_bool() == Some(true)
            && params["runtimeCapabilities"].is_null()
        {
            return Err(invalid(
                "Resuming HDR requires current embedded window output capabilities",
            ));
        }
        if !params["runtimeCapabilities"].is_null() {
            // Re-check the server's negotiated codec on resume/attachment as well. Never
            // reinterpret compressed AV1 bytes as H.264 when a persisted session is resumed.
            let mut negotiated_settings = context["settings"].clone();
            if let Some(color) =
                context["session"]["negotiatedStreamProfile"]["colorQuality"].as_str()
            {
                negotiated_settings["colorQuality"] = json!(color);
            }
            let resolved = Self::embedded_session_settings(
                &negotiated_settings,
                &params["runtimeCapabilities"],
            )?;
            context["settings"]["nativeHdrSupported"] = resolved["nativeHdrSupported"].clone();
        }
        context["surface"] = Value::Null;
        Ok(json!({
            "protocolVersion": STREAMER_PROTOCOL_VERSION,
            "context": context
        }))
    }

    pub fn start(&self, params: &Value, settings: &Value) -> Result<Value, StreamerError> {
        self.reap_finished();
        let mut worker = self.worker.lock().expect("streamer worker poisoned");
        if worker.is_some() {
            return Err(StreamerError {
                code: "streamer_busy",
                message: "A native streamer session is already running".to_owned(),
            });
        }
        let executable = resolve_executable(settings)?;
        if !executable.is_file() {
            return Err(StreamerError {
                code: "streamer_not_found",
                message: format!(
                    "Native streamer is not installed at {}",
                    executable.display()
                ),
            });
        }
        let session = params["session"]
            .as_object()
            .map(|_| params["session"].clone())
            .ok_or_else(|| invalid("streamer.start requires a ready session"))?;
        let status = session["status"].as_i64().unwrap_or_default();
        if !matches!(status, 2 | 3) {
            return Err(invalid(
                "CloudMatch session is not ready for NVST media attachment",
            ));
        }
        let session_id = required_string(&session, "sessionId")?.to_owned();
        let microphone_mode = settings["microphoneMode"].as_str().unwrap_or("disabled");
        let mut context = streamer_context(session, settings);
        let surface = normalize_surface(params.get("surface"), context_resolution(&context))?;
        context["surface"] = surface;
        let (control_tx, control_rx) = mpsc::channel();
        let state = Arc::clone(&self.state);
        {
            let mut snapshot = state.lock().expect("streamer state poisoned");
            snapshot.status = "starting".to_owned();
            snapshot.message = "Launching the native NVST media runtime…".to_owned();
            snapshot.session_id = Some(session_id.clone());
            snapshot.process_id = None;
            snapshot.transport = Some("nvst".to_owned());
            snapshot.executable = Some(executable.clone());
            snapshot.error_code = None;
            snapshot.started_at = Some(Instant::now());
            snapshot.session_started_at_ms = Some(unix_time_millis());
            snapshot.first_frame_latency_ms = None;
            snapshot.media_backend = None;
            snapshot.frames_per_second = None;
            snapshot.bitrate_mbps = None;
            snapshot.peak_bitrate_mbps = None;
            snapshot.backend_fallback_count = 0;
            snapshot.decoder_error_count = 0;
            snapshot.output_error_count = 0;
            snapshot.device_loss_count = 0;
            snapshot.device_recovery_count = 0;
            snapshot.queue_drop_count = 0;
            snapshot.input_ready = false;
            snapshot.input_unavailable_reason = None;
            snapshot.input_pause_count = 0;
            snapshot.input_resume_count = 0;
            snapshot.surface_update_count = 0;
            snapshot.fullscreen_toggle_count = 0;
            snapshot.stats_toggle_count = 0;
            snapshot.recording_start_count = 0;
            snapshot.recording_stop_count = 0;
            match microphone_mode {
                "voice-activity" => {
                    snapshot.microphone_state = "muted".to_owned();
                    snapshot.microphone_enabled = false;
                    snapshot.microphone_message = None;
                }
                "push-to-talk" => {
                    snapshot.microphone_state = "unavailable".to_owned();
                    snapshot.microphone_enabled = false;
                    snapshot.microphone_message =
                        Some("Push-to-talk is not supported by the native NVST runtime".to_owned());
                }
                _ => {
                    snapshot.microphone_state = "disabled".to_owned();
                    snapshot.microphone_enabled = false;
                    snapshot.microphone_message = None;
                }
            }
        }
        let join = thread::Builder::new()
            .name("opennow-streamer-coordinator".to_owned())
            .spawn(move || {
                if let Err(error) = run_worker(&executable, context, control_rx, Arc::clone(&state))
                {
                    eprintln!(
                        "native-streamer worker failed [{}]: {}",
                        error.code, error.message
                    );
                    set_error(&state, error.code, error.message);
                }
            })
            .map_err(|error| StreamerError {
                code: "streamer_spawn_failed",
                message: error.to_string(),
            })?;
        *worker = Some(Worker {
            control: control_tx,
            join,
        });
        drop(worker);
        Ok(self.status())
    }

    pub fn status(&self) -> Value {
        self.reap_finished();
        self.state.lock().expect("streamer state poisoned").value()
    }

    pub fn acceptance_snapshot(&self) -> Value {
        self.reap_finished();
        self.state
            .lock()
            .expect("streamer state poisoned")
            .acceptance_value()
    }

    pub fn stop(&self, reason: &str) -> Result<Value, StreamerError> {
        let item = self.worker.lock().expect("streamer worker poisoned").take();
        if let Some(worker) = item {
            let _ = worker.control.send(WorkerCommand::Stop(reason.to_owned()));
            worker.join.join().map_err(|_| StreamerError {
                code: "streamer_stop_failed",
                message: "Native streamer coordinator panicked during shutdown".to_owned(),
            })?;
        }
        let mut snapshot = self.state.lock().expect("streamer state poisoned");
        snapshot.status = "stopped".to_owned();
        snapshot.message = reason.to_owned();
        snapshot.session_id = None;
        snapshot.process_id = None;
        snapshot.transport = None;
        snapshot.error_code = None;
        snapshot.microphone_state = "disabled".to_owned();
        snapshot.microphone_enabled = false;
        snapshot.microphone_message = None;
        snapshot.started_at = None;
        Ok(snapshot.value())
    }

    pub fn set_input_paused(&self, paused: bool) -> Result<Value, StreamerError> {
        self.reap_finished();
        let worker = self.worker.lock().expect("streamer worker poisoned");
        let Some(worker) = worker.as_ref() else {
            return Ok(json!({"paused":paused,"streamerRunning":false}));
        };
        worker
            .control
            .send(WorkerCommand::InputPaused(paused))
            .map_err(|_| StreamerError {
                code: "streamer_control_failed",
                message: "Native streamer control channel is closed".to_owned(),
            })?;
        let mut snapshot = self.state.lock().expect("streamer state poisoned");
        if paused {
            snapshot.input_pause_count = snapshot.input_pause_count.saturating_add(1);
        } else {
            snapshot.input_resume_count = snapshot.input_resume_count.saturating_add(1);
        }
        Ok(json!({"paused":paused,"streamerRunning":true}))
    }

    pub fn control(&self, action: &str) -> Result<Value, StreamerError> {
        let child_command = match action {
            "toggle-fullscreen" => "fullscreen-toggle",
            "toggle-microphone" => "microphone-toggle",
            "anti-afk-pulse" => "anti-afk-pulse",
            _ => return Err(invalid("Unknown native streamer control action")),
        };
        self.reap_finished();
        let worker = self.worker.lock().expect("streamer worker poisoned");
        let Some(worker) = worker.as_ref() else {
            return Err(StreamerError {
                code: "streamer_not_running",
                message: "Native streamer is not running".to_owned(),
            });
        };
        worker
            .control
            .send(WorkerCommand::Control(child_command.to_owned()))
            .map_err(|_| StreamerError {
                code: "streamer_control_failed",
                message: "Native streamer control channel is closed".to_owned(),
            })?;
        Ok(json!({"action":action,"streamerRunning":true}))
    }

    pub fn update_surface(&self, params: &Value) -> Result<Value, StreamerError> {
        let surface = normalize_surface(params.get("surface"), (1280, 720))?;
        self.reap_finished();
        let worker = self.worker.lock().expect("streamer worker poisoned");
        let Some(worker) = worker.as_ref() else {
            return Ok(json!({"applied":false,"streamerRunning":false}));
        };
        worker
            .control
            .send(WorkerCommand::Surface(surface))
            .map_err(|_| StreamerError {
                code: "streamer_control_failed",
                message: "Native streamer control channel is closed".to_owned(),
            })?;
        let mut snapshot = self.state.lock().expect("streamer state poisoned");
        snapshot.surface_update_count = snapshot.surface_update_count.saturating_add(1);
        Ok(json!({"applied":true,"streamerRunning":true}))
    }

    pub fn recording(&self, params: &Value, enabled: bool) -> Result<Value, StreamerError> {
        self.reap_finished();
        let output_path = if enabled {
            let path = PathBuf::from(
                params["outputPath"]
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| invalid("Native recording requires an outputPath"))?,
            );
            if !path.is_absolute()
                || path.extension().and_then(|value| value.to_str()) != Some("mkv")
            {
                return Err(invalid(
                    "Native recording requires an absolute .mkv outputPath",
                ));
            }
            Some(path)
        } else {
            None
        };
        let (reply, response) = mpsc::sync_channel(1);
        {
            let worker = self.worker.lock().expect("streamer worker poisoned");
            let Some(worker) = worker.as_ref() else {
                return Err(StreamerError {
                    code: "streamer_not_running",
                    message: "Native streamer is not running".to_owned(),
                });
            };
            worker
                .control
                .send(WorkerCommand::Recording {
                    enabled,
                    output_path,
                    reply,
                })
                .map_err(|_| StreamerError {
                    code: "streamer_control_failed",
                    message: "Native streamer control channel is closed".to_owned(),
                })?;
        }
        let result = response
            .recv_timeout(Duration::from_secs(15))
            .map_err(|_| StreamerError {
                code: "streamer_timeout",
                message: "Native streamer timed out finalizing the recording".to_owned(),
            })??;
        let mut snapshot = self.state.lock().expect("streamer state poisoned");
        if enabled {
            snapshot.recording_start_count = snapshot.recording_start_count.saturating_add(1);
        } else {
            snapshot.recording_stop_count = snapshot.recording_stop_count.saturating_add(1);
        }
        Ok(result)
    }

    fn reap_finished(&self) {
        let finished = self
            .worker
            .lock()
            .expect("streamer worker poisoned")
            .as_ref()
            .is_some_and(|worker| worker.join.is_finished());
        if finished
            && let Some(worker) = self.worker.lock().expect("streamer worker poisoned").take()
        {
            let _ = worker.join.join();
        }
    }
}

impl Drop for StreamerService {
    fn drop(&mut self) {
        if let Some(worker) = self
            .worker
            .get_mut()
            .expect("streamer worker poisoned")
            .take()
        {
            let _ = worker
                .control
                .send(WorkerCommand::Stop("OpenNOW core shutdown".to_owned()));
            let _ = worker.join.join();
        }
    }
}

fn streamer_command(executable: &Path, settings: &Value) -> Command {
    let requested_backend = settings["nativeVideoBackend"]
        .as_str()
        .unwrap_or("auto")
        .trim()
        .to_ascii_lowercase();
    let decoder_preference = settings["decoderPreference"]
        .as_str()
        .unwrap_or("auto")
        .trim()
        .to_ascii_lowercase();
    let decoder_backend = match requested_backend.as_str() {
        "auto" | "" => match decoder_preference.as_str() {
            "hardware" => "hardware",
            "software" => "software",
            _ => "auto",
        },
        explicit => explicit,
    };
    let cursor_overlay = if settings["nativeCursorOverlay"].as_bool().unwrap_or(true) {
        "1"
    } else {
        "0"
    };
    let mouse_sensitivity = settings["mouseSensitivity"]
        .as_f64()
        .unwrap_or(1.0)
        .clamp(0.1, 3.0)
        .to_string();
    let mouse_acceleration = settings["mouseAcceleration"]
        .as_f64()
        .unwrap_or(1.0)
        .clamp(1.0, 150.0)
        .to_string();
    let mut command = Command::new(executable);
    command
        .env("OPENNOW_NATIVE_EXTERNAL_RENDERER", "1")
        // The separate SDL stream window owns foreground keyboard/mouse events.
        // Keep this paired with EXTERNAL_RENDERER, matching native-streamer-v2;
        // otherwise the streamer deliberately disables capture to avoid duplicate
        // events from an embedded UI owner.
        .env("OPENNOW_NATIVE_INPUT_OWNER", "native")
        .env("OPENNOW_NATIVE_VIDEO_BACKEND", decoder_backend)
        .env("OPENNOW_NATIVE_CURSOR_OVERLAY", cursor_overlay)
        .env("OPENNOW_MOUSE_SENSITIVITY", mouse_sensitivity)
        .env("OPENNOW_MOUSE_ACCELERATION", mouse_acceleration)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn probe_capabilities(executable: &Path, settings: &Value) -> Result<Value, StreamerError> {
    let mut child = KillOnDrop(streamer_command(executable, settings).spawn().map_err(
        |error| StreamerError {
            code: "streamer_spawn_failed",
            message: format!("Could not probe {}: {error}", executable.display()),
        },
    )?);
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| internal("Streamer probe stdin is unavailable"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| internal("Streamer probe stdout is unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| internal("Streamer probe stderr is unavailable"))?;
    let (child_tx, child_rx) = mpsc::channel::<Value>();
    let reader = spawn_stdout_reader(stdout, child_tx)?;
    let stderr_reader = thread::Builder::new()
        .name("opennow-streamer-probe-stderr".to_owned())
        .spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                eprintln!("native-streamer probe: {line}");
            }
        })
        .map_err(|error| StreamerError {
            code: "streamer_spawn_failed",
            message: error.to_string(),
        })?;
    let state = Arc::new(Mutex::new(Snapshot::default()));
    write_child(
        &mut stdin,
        &json!({"id":"hello","type":"hello","protocolVersion":STREAMER_PROTOCOL_VERSION}),
    )?;
    let hello = wait_for_child(&child_rx, "hello", CAPABILITY_PROBE_TIMEOUT, &state)?;
    if hello["type"] != "ready" {
        return Err(child_error(
            &hello,
            "Native streamer capability probe failed",
        ));
    }
    let capabilities = hello["capabilities"].clone();
    if !capabilities.is_object()
        || capabilities["protocolVersion"].as_u64() != Some(STREAMER_PROTOCOL_VERSION)
    {
        return Err(StreamerError {
            code: "streamer_protocol_mismatch",
            message: format!(
                "Native streamer did not report protocol {STREAMER_PROTOCOL_VERSION} capabilities"
            ),
        });
    }
    let _ = write_child(
        &mut stdin,
        &json!({"id":"shutdown","type":"shutdown","reason":"capability probe complete"}),
    );
    wait_or_kill(&mut child);
    drop(stdin);
    let _ = reader.join();
    let _ = stderr_reader.join();
    Ok(capabilities)
}

fn normalize_codec_name(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "h264" | "avc" | "auto" | "" => Some("h264"),
        "h265" | "hevc" => Some("h265"),
        "av1" => Some("av1"),
        _ => None,
    }
}

fn available_codecs(capabilities: &Value) -> Vec<&'static str> {
    ["h264", "h265", "av1"]
        .into_iter()
        .filter(|codec| codec_available(capabilities, codec))
        .collect()
}

fn macos_auto_codec_candidates(settings: &Value) -> &'static [&'static str] {
    let (width, height) = settings["resolution"]
        .as_str()
        .and_then(|value| value.split_once('x'))
        .and_then(|(width, height)| Some((width.parse::<u64>().ok()?, height.parse::<u64>().ok()?)))
        .filter(|(width, height)| *width > 0 && *height > 0)
        .unwrap_or((1920, 1080));
    let pixels = width.saturating_mul(height);
    let fps = settings["fps"].as_u64().unwrap_or(60);
    let bitrate = settings["maxBitrateMbps"].as_u64().unwrap_or(75);
    if fps >= 144 {
        &["h264", "h265", "av1"]
    } else if pixels >= 3840 * 2160 || ((1..=30).contains(&bitrate) && pixels >= 2560 * 1440) {
        &["av1", "h265", "h264"]
    } else if (1..=30).contains(&bitrate) {
        &["av1", "h264", "h265"]
    } else if pixels >= 2560 * 1440 || bitrate >= 75 {
        &["h265", "h264", "av1"]
    } else {
        &["h264", "h265", "av1"]
    }
}

fn codec_available(capabilities: &Value, codec: &str) -> bool {
    capabilities["videoBackends"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|backend| backend["available"].as_bool().unwrap_or(false))
        .flat_map(|backend| backend["codecs"].as_array().into_iter().flatten())
        .any(|entry| {
            entry["codec"]
                .as_str()
                .is_some_and(|value| value.eq_ignore_ascii_case(codec))
                && entry["available"].as_bool().unwrap_or(false)
        })
}

fn ensure_codec_available(capabilities: &Value, codec: &str) -> Result<(), StreamerError> {
    if codec_available(capabilities, codec) {
        return Ok(());
    }
    let reasons = capabilities["videoBackends"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|backend| backend["codecs"].as_array().into_iter().flatten())
        .filter(|entry| {
            entry["codec"]
                .as_str()
                .is_some_and(|value| value.eq_ignore_ascii_case(codec))
        })
        .filter_map(|entry| entry["reason"].as_str())
        .collect::<Vec<_>>();
    let detail = reasons
        .first()
        .copied()
        .unwrap_or("no selected native backend advertises this codec");
    Err(StreamerError {
        code: "streamer_codec_unavailable",
        message: format!("{} is unavailable: {detail}", codec.to_ascii_uppercase()),
    })
}

fn run_worker(
    executable: &Path,
    context: Value,
    control: Receiver<WorkerCommand>,
    state: Arc<Mutex<Snapshot>>,
) -> Result<(), StreamerError> {
    let mut child = KillOnDrop(
        streamer_command(executable, &context["settings"])
            .spawn()
            .map_err(|error| StreamerError {
                code: "streamer_spawn_failed",
                message: format!("Could not launch {}: {error}", executable.display()),
            })?,
    );
    let process_id = u64::from(child.id());
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| internal("Streamer stdin is unavailable"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| internal("Streamer stdout is unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| internal("Streamer stderr is unavailable"))?;
    let (child_tx, child_rx) = mpsc::channel::<Value>();
    let reader = spawn_stdout_reader(stdout, child_tx)?;
    let stderr_reader = thread::Builder::new()
        .name("opennow-streamer-stderr".to_owned())
        .spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                eprintln!("native-streamer: {line}");
            }
        })
        .map_err(|error| StreamerError {
            code: "streamer_spawn_failed",
            message: error.to_string(),
        })?;

    write_child(
        &mut stdin,
        &json!({"id":"hello","type":"hello","protocolVersion":STREAMER_PROTOCOL_VERSION}),
    )?;
    let hello = wait_for_child(&child_rx, "hello", CHILD_START_TIMEOUT, &state)?;
    if hello["type"] != "ready" {
        return Err(child_error(&hello, "Native streamer handshake failed"));
    }
    let negotiated_codec = context["settings"]["codec"]
        .as_str()
        .and_then(normalize_codec_name)
        .unwrap_or("h264");
    ensure_codec_available(&hello["capabilities"], negotiated_codec)?;
    {
        let mut snapshot = state.lock().expect("streamer state poisoned");
        snapshot.process_id = hello["processId"].as_u64().or(Some(process_id));
        snapshot.capabilities = hello["capabilities"].clone();
        snapshot.status = "starting".to_owned();
        snapshot.message = "Preparing native video and audio output…".to_owned();
    }
    if hello["capabilities"]["supportsOwnedNvstNegotiation"].as_bool() != Some(true) {
        return Err(StreamerError {
            code: "streamer_protocol_mismatch",
            message: "Native streamer cannot start NVST: owned NVST negotiation and protocol 7 are required"
                .to_owned(),
        });
    }
    let result = run_owned_nvst_child(
        &context, &mut stdin, &mut child, &child_rx, &control, &state,
    );
    let _ = write_child(
        &mut stdin,
        &json!({"id":"shutdown","type":"shutdown","reason":"OpenNOW session ended"}),
    );
    wait_or_kill(&mut child);
    drop(stdin);
    let _ = reader.join();
    let _ = stderr_reader.join();
    result
}

fn run_owned_nvst_child(
    context: &Value,
    stdin: &mut ChildStdin,
    child: &mut Child,
    child_rx: &Receiver<Value>,
    control: &Receiver<WorkerCommand>,
    state: &Arc<Mutex<Snapshot>>,
) -> Result<(), StreamerError> {
    {
        let mut snapshot = state.lock().expect("streamer state poisoned");
        snapshot.status = "negotiating".to_owned();
        snapshot.message =
            "Native streamer is negotiating the GeForce NOW NVST session…".to_owned();
    }
    start_child(stdin, child_rx, context, state)?;
    ensure_child_running(child)?;
    write_surface(stdin, &context["surface"])?;
    {
        let mut snapshot = state.lock().expect("streamer state poisoned");
        snapshot.transport = Some("nvst".to_owned());
        snapshot.status = "streaming".to_owned();
        snapshot.message = "Native-owned NVST media transport is active".to_owned();
    }

    loop {
        if let Ok(command) = control.try_recv() {
            match command {
                WorkerCommand::Stop(reason) => {
                    let _ = write_child(stdin, &json!({"id":"stop","type":"stop","reason":reason}));
                    return Ok(());
                }
                WorkerCommand::InputPaused(paused) => {
                    write_child(
                        stdin,
                        &json!({"id":"input-paused","type":"input-paused","paused":paused}),
                    )?;
                }
                WorkerCommand::Control(command_type) => {
                    write_child(stdin, &json!({"id":"runtime-control","type":command_type}))?;
                }
                WorkerCommand::Surface(surface) => write_surface(stdin, &surface)?,
                WorkerCommand::Recording {
                    enabled,
                    output_path,
                    reply,
                } => {
                    let result =
                        execute_recording(stdin, child_rx, state, enabled, output_path.as_deref());
                    let _ = reply.send(result);
                }
            }
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| internal(error.to_string()))?
        {
            return Err(StreamerError {
                code: "streamer_exited",
                message: format!("Native streamer exited unexpectedly ({status})"),
            });
        }
        while let Ok(message) = child_rx.try_recv() {
            handle_nvst_child_event(&message, state)?;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn start_child(
    stdin: &mut ChildStdin,
    child_rx: &Receiver<Value>,
    context: &Value,
    state: &Arc<Mutex<Snapshot>>,
) -> Result<(), StreamerError> {
    write_child(
        stdin,
        &json!({"id":"start","type":"start","context":context}),
    )?;
    let started = wait_for_child(child_rx, "start", CHILD_START_TIMEOUT, state)?;
    if started["type"] != "ok" {
        return Err(child_error(&started, "Native NVST media startup failed"));
    }
    state.lock().expect("streamer state poisoned").transport =
        started["transport"].as_str().map(ToOwned::to_owned);
    Ok(())
}

fn handle_nvst_child_event(
    message: &Value,
    state: &Arc<Mutex<Snapshot>>,
) -> Result<(), StreamerError> {
    apply_child_telemetry(message, state);
    match message["type"].as_str().unwrap_or_default() {
        "status" => {
            let mut snapshot = state.lock().expect("streamer state poisoned");
            if let Some(value) = message["status"].as_str() {
                snapshot.status = value.to_owned()
            }
            if let Some(value) = message["message"].as_str() {
                snapshot.message = value.to_owned()
            }
            Ok(())
        }
        "error" => Err(StreamerError {
            code: "native_stream_error",
            message: message["message"]
                .as_str()
                .unwrap_or("Native NVST media error")
                .to_owned(),
        }),
        "overlay-request" => {
            let mut snapshot = state.lock().expect("streamer state poisoned");
            snapshot.overlay_request_generation =
                snapshot.overlay_request_generation.wrapping_add(1);
            Ok(())
        }
        "screenshot-request" => {
            let mut snapshot = state.lock().expect("streamer state poisoned");
            snapshot.screenshot_request_generation =
                snapshot.screenshot_request_generation.wrapping_add(1);
            Ok(())
        }
        "recording-toggle-request" => {
            let mut snapshot = state.lock().expect("streamer state poisoned");
            snapshot.recording_toggle_request_generation =
                snapshot.recording_toggle_request_generation.wrapping_add(1);
            Ok(())
        }
        "shortcut-action" => {
            let action = message["action"]
                .as_str()
                .filter(|action| {
                    matches!(
                        *action,
                        "toggle-stats"
                            | "toggle-fullscreen"
                            | "stop-stream"
                            | "toggle-anti-afk"
                            | "toggle-microphone"
                            | "screenshot"
                            | "toggle-recording"
                    )
                })
                .ok_or_else(|| StreamerError {
                    code: "streamer_protocol_error",
                    message: "Native streamer emitted an invalid shortcut action".to_owned(),
                })?;
            let mut snapshot = state.lock().expect("streamer state poisoned");
            snapshot.shortcut_action_generation =
                snapshot.shortcut_action_generation.wrapping_add(1);
            if action == "toggle-stats" {
                snapshot.stats_toggle_count = snapshot.stats_toggle_count.saturating_add(1);
            } else if action == "toggle-fullscreen" {
                snapshot.fullscreen_toggle_count =
                    snapshot.fullscreen_toggle_count.saturating_add(1);
            }
            snapshot.shortcut_action = Some(action.to_owned());
            Ok(())
        }
        "microphone-state" => {
            apply_microphone_state(message, state);
            Ok(())
        }
        _ => Ok(()),
    }
}

fn apply_microphone_state(message: &Value, state: &Arc<Mutex<Snapshot>>) {
    let mut snapshot = state.lock().expect("streamer state poisoned");
    snapshot.microphone_state = message["state"]
        .as_str()
        .unwrap_or("unavailable")
        .to_owned();
    snapshot.microphone_enabled = message["enabled"].as_bool().unwrap_or(false);
    snapshot.microphone_message = message["message"].as_str().map(ToOwned::to_owned);
}

fn apply_child_telemetry(message: &Value, state: &Arc<Mutex<Snapshot>>) {
    let mut snapshot = state.lock().expect("streamer state poisoned");
    match message["type"].as_str().unwrap_or_default() {
        "input-ready" => {
            snapshot.input_ready = true;
            snapshot.input_unavailable_reason = None;
        }
        "input-unavailable" => {
            snapshot.input_ready = false;
            snapshot.input_unavailable_reason = message["reason"].as_str().map(ToOwned::to_owned);
        }
        "telemetry" => {
            let metric = |key: &str, maximum: f64| {
                message[key]
                    .as_f64()
                    .filter(|value| value.is_finite() && (0.0..=maximum).contains(value))
            };
            if let Some(value) = metric("framesPerSecond", 1_000.0) {
                snapshot.frames_per_second = Some(value);
            }
            if let Some(value) = metric("bitrateMbps", 10_000.0) {
                snapshot.bitrate_mbps = Some(value);
            }
            if let Some(value) = metric("peakBitrateMbps", 10_000.0) {
                snapshot.peak_bitrate_mbps = Some(value);
            }
        }
        _ => {}
    }

    match message["event"].as_str().unwrap_or_default() {
        "first-frame" => {
            if snapshot.first_frame_latency_ms.is_none() {
                snapshot.first_frame_latency_ms = snapshot
                    .started_at
                    .map(|started| elapsed_millis(started.elapsed()));
            }
            snapshot.media_backend = message["backend"].as_str().map(ToOwned::to_owned);
        }
        "backend-fallback" => {
            snapshot.backend_fallback_count = snapshot.backend_fallback_count.saturating_add(1);
        }
        "decoder-error" => {
            snapshot.decoder_error_count = snapshot.decoder_error_count.saturating_add(1);
        }
        "output-error" => {
            snapshot.output_error_count = snapshot.output_error_count.saturating_add(1);
        }
        "device-state" => {
            if message["recovered"].as_bool().unwrap_or(false) {
                snapshot.device_recovery_count = snapshot.device_recovery_count.saturating_add(1);
            } else {
                snapshot.device_loss_count = snapshot.device_loss_count.saturating_add(1);
            }
        }
        "queue-dropped" => {
            snapshot.queue_drop_count = snapshot
                .queue_drop_count
                .saturating_add(message["count"].as_u64().unwrap_or_default());
        }
        _ => {}
    }
}

fn streamer_context(mut session: Value, settings: &Value) -> Value {
    let mut normalized = settings.clone();
    let negotiated_codec = session["negotiatedStreamProfile"]["codec"].as_str();
    let requested_codec = normalized["codec"].as_str().unwrap_or("auto").to_owned();
    let codec = negotiated_codec.map(ToOwned::to_owned).unwrap_or_else(|| {
        if requested_codec.eq_ignore_ascii_case("auto") {
            "H264".to_owned()
        } else {
            requested_codec
        }
    });
    normalized["codec"] = Value::String(codec.to_ascii_uppercase());
    normalized["enableHdr"] = json!(
        session["negotiatedStreamProfile"]["enableHdr"]
            .as_bool()
            .unwrap_or(false)
    );
    normalized["nativeHdrSupported"] = json!(false);
    if let Some(color) = session["negotiatedStreamProfile"]["colorQuality"].as_str() {
        normalized["colorQuality"] = json!(color);
    }
    // The Qt/native client owns negotiation and media over NVST. Ignore old
    // persisted WebRTC values so manual HEVC/AV1 selections cannot be routed
    // through the retired browser transport.
    normalized["transportMode"] = Value::String("nvst".to_owned());
    if session["negotiatedStreamProfile"].is_null() {
        session["negotiatedStreamProfile"] = json!({"codec":codec.to_ascii_uppercase()});
    }
    json!({
        "session":session,
        "settings":normalized,
        "shortcuts":{
            "toggleStats":settings["shortcutToggleStats"],
            "togglePointerLock":settings["shortcutTogglePointerLock"],
            "toggleFullscreen":settings["shortcutToggleFullscreen"],
            "stopStream":settings["shortcutStopStream"],
            "toggleAntiAfk":settings["shortcutToggleAntiAfk"],
            "toggleMicrophone":settings["shortcutToggleMicrophone"],
            "screenshot":settings["shortcutScreenshot"],
            "toggleRecording":settings["shortcutToggleRecording"],
            "saveClip":settings["shortcutSaveClip"]
        }
    })
}

fn context_resolution(context: &Value) -> (u64, u64) {
    context["settings"]["resolution"]
        .as_str()
        .and_then(|value| value.split_once('x'))
        .and_then(|(width, height)| Some((width.parse().ok()?, height.parse().ok()?)))
        .filter(|(width, height)| *width > 0 && *height > 0)
        .unwrap_or((1280, 720))
}

fn normalize_surface(
    raw: Option<&Value>,
    fallback_size: (u64, u64),
) -> Result<Value, StreamerError> {
    let fallback = json!({
        "rect":{"x":0,"y":0,"width":fallback_size.0,"height":fallback_size.1},
        "screenRect":{"x":0,"y":0,"width":fallback_size.0,"height":fallback_size.1},
        "visible":true,
        "deviceScaleFactor":1.0
    });
    let value = raw.unwrap_or(&fallback);
    let object = value
        .as_object()
        .ok_or_else(|| invalid("streamer surface must be an object"))?;
    let visible = object
        .get("visible")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let screen = object
        .get("screenRect")
        .and_then(Value::as_object)
        .or_else(|| object.get("rect").and_then(Value::as_object))
        .ok_or_else(|| invalid("streamer surface requires screenRect"))?;
    let parse_rect = |value: &serde_json::Map<String, Value>, label: &str| {
        let width = value
            .get("width")
            .and_then(Value::as_u64)
            .filter(|value| (64..=16_384).contains(value))
            .ok_or_else(|| invalid(format!("streamer {label} width is out of range")))?;
        let height = value
            .get("height")
            .and_then(Value::as_u64)
            .filter(|value| (64..=16_384).contains(value))
            .ok_or_else(|| invalid(format!("streamer {label} height is out of range")))?;
        let x = value.get("x").and_then(Value::as_i64).unwrap_or(0);
        let y = value.get("y").and_then(Value::as_i64).unwrap_or(0);
        if !(-100_000..=100_000).contains(&x) || !(-100_000..=100_000).contains(&y) {
            return Err(invalid(format!(
                "streamer {label} position is out of range"
            )));
        }
        Ok((x, y, width, height))
    };
    let (screen_x, screen_y, screen_width, screen_height) = parse_rect(screen, "screenRect")?;
    let (local_x, local_y, local_width, local_height) = object
        .get("rect")
        .and_then(Value::as_object)
        .map(|rect| parse_rect(rect, "rect"))
        .transpose()?
        .unwrap_or((0, 0, screen_width, screen_height));
    let scale = object
        .get("deviceScaleFactor")
        .and_then(Value::as_f64)
        .unwrap_or(1.0);
    if !scale.is_finite() || !(0.5..=8.0).contains(&scale) {
        return Err(invalid("streamer surface scale is out of range"));
    }
    let window_handle = object
        .get("windowHandle")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 512);
    let mut normalized = json!({
        "rect":{"x":local_x,"y":local_y,"width":local_width,"height":local_height},
        "screenRect":{"x":screen_x,"y":screen_y,"width":screen_width,"height":screen_height},
        "visible":visible,
        "deviceScaleFactor":scale
    });
    if let Some(window_handle) = window_handle {
        normalized["windowHandle"] = Value::String(window_handle.to_owned());
    }
    Ok(normalized)
}

fn write_surface(stdin: &mut ChildStdin, surface: &Value) -> Result<(), StreamerError> {
    write_child(
        stdin,
        &json!({"id":"surface","type":"surface","surface":surface}),
    )
}

fn execute_recording(
    stdin: &mut ChildStdin,
    child_rx: &Receiver<Value>,
    state: &Arc<Mutex<Snapshot>>,
    enabled: bool,
    output_path: Option<&Path>,
) -> Result<Value, StreamerError> {
    const ID: &str = "recording-control";
    write_child(
        stdin,
        &json!({
            "id":ID,
            "type":if enabled { "recording-start" } else { "recording-stop" },
            "outputPath":output_path.map(|path| path.to_string_lossy().into_owned()),
        }),
    )?;
    let response = wait_for_child(child_rx, ID, Duration::from_secs(12), state)?;
    if response["type"] == "error" {
        return Err(StreamerError {
            code: "recording_failed",
            message: response["message"]
                .as_str()
                .unwrap_or("Native recording failed")
                .to_owned(),
        });
    }
    Ok(json!({
        "enabled":enabled,
        "path":response["path"],
        "videoPackets":response["videoPackets"],
        "audioPackets":response["audioPackets"],
        "streamerRunning":true,
    }))
}

fn spawn_stdout_reader(
    stdout: impl std::io::Read + Send + 'static,
    messages: Sender<Value>,
) -> Result<JoinHandle<()>, StreamerError> {
    thread::Builder::new()
        .name("opennow-streamer-stdout".to_owned())
        .spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if line.len() > CHILD_MESSAGE_LIMIT {
                    break;
                }
                if let Ok(message) = serde_json::from_str(&line) {
                    let _ = messages.send(message);
                }
            }
        })
        .map_err(|error| StreamerError {
            code: "streamer_spawn_failed",
            message: error.to_string(),
        })
}

fn wait_for_child(
    receiver: &Receiver<Value>,
    id: &str,
    timeout: Duration,
    state: &Arc<Mutex<Snapshot>>,
) -> Result<Value, StreamerError> {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(StreamerError {
                code: "streamer_timeout",
                message: format!("Native streamer timed out waiting for {id}"),
            });
        }
        let message = match receiver.recv_timeout(remaining) {
            Ok(message) => message,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                return Err(StreamerError {
                    code: "streamer_timeout",
                    message: format!("Native streamer timed out waiting for {id}"),
                });
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(StreamerError {
                    code: "streamer_exited",
                    message: format!(
                        "Native streamer closed its protocol output while waiting for {id}"
                    ),
                });
            }
        };
        if message["id"].as_str() == Some(id) {
            return Ok(message);
        }
        if message["type"].as_str() == Some("status") {
            let mut snapshot = state.lock().expect("streamer state poisoned");
            snapshot.status = message["status"].as_str().unwrap_or("starting").to_owned();
            snapshot.message = message["message"].as_str().unwrap_or("").to_owned();
        }
    }
}

fn write_child(stdin: &mut ChildStdin, message: &Value) -> Result<(), StreamerError> {
    serde_json::to_writer(&mut *stdin, message).map_err(|error| {
        error
            .io_error_kind()
            .map(|kind| child_io_error(kind, error.to_string()))
            .unwrap_or_else(|| internal(error.to_string()))
    })?;
    stdin
        .write_all(b"\n")
        .map_err(|error| child_io_error(error.kind(), error.to_string()))?;
    stdin
        .flush()
        .map_err(|error| child_io_error(error.kind(), error.to_string()))
}

fn child_io_error(kind: ErrorKind, message: String) -> StreamerError {
    let code = if matches!(
        kind,
        ErrorKind::BrokenPipe
            | ErrorKind::ConnectionAborted
            | ErrorKind::ConnectionReset
            | ErrorKind::UnexpectedEof
    ) {
        "streamer_exited"
    } else {
        "streamer_io_failed"
    };
    StreamerError { code, message }
}

fn wait_or_kill(child: &mut Child) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if child.try_wait().ok().flatten().is_some() {
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn ensure_child_running(child: &mut Child) -> Result<(), StreamerError> {
    match child.try_wait() {
        Ok(None) => Ok(()),
        Ok(Some(status)) => Err(StreamerError {
            code: "streamer_exited",
            message: format!("Native streamer exited unexpectedly ({status})"),
        }),
        Err(error) => Err(internal(format!(
            "Could not inspect native streamer state: {error}"
        ))),
    }
}

fn resolve_executable(settings: &Value) -> Result<PathBuf, StreamerError> {
    if let Some(configured) = settings["nativeStreamerExecutablePath"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
    {
        return Ok(PathBuf::from(configured));
    }
    let current = std::env::current_exe().map_err(|error| StreamerError {
        code: "streamer_not_found",
        message: error.to_string(),
    })?;
    Ok(current
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(if cfg!(target_os = "windows") {
            "opennow-streamer.exe"
        } else {
            "opennow-streamer"
        }))
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str, StreamerError> {
    value[key]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid(format!("Missing session {key}")))
}

fn child_error(value: &Value, fallback: &str) -> StreamerError {
    StreamerError {
        code: "native_stream_error",
        message: value["message"].as_str().unwrap_or(fallback).to_owned(),
    }
}

fn set_error(state: &Arc<Mutex<Snapshot>>, code: &str, message: String) {
    let mut snapshot = state.lock().expect("streamer state poisoned");
    snapshot.status = "error".to_owned();
    snapshot.message = message;
    snapshot.error_code = Some(code.to_owned());
    snapshot.process_id = None;
    snapshot.microphone_enabled = false;
    if snapshot.microphone_state != "disabled" {
        snapshot.microphone_state = "unavailable".to_owned();
    }
}

fn elapsed_millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn unix_time_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(all(test, unix))]
fn random_u64() -> u64 {
    let mut bytes = [0_u8; 8];
    rand::rng().fill_bytes(&mut bytes);
    u64::from_le_bytes(bytes)
}

fn invalid(message: impl Into<String>) -> StreamerError {
    StreamerError {
        code: "invalid_params",
        message: message.into(),
    }
}

fn internal(message: impl Into<String>) -> StreamerError {
    StreamerError {
        code: "streamer_internal_error",
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_stream_window_is_the_native_input_owner() {
        let command = streamer_command(
            Path::new("opennow-streamer"),
            &json!({
                "nativeVideoBackend":"auto",
                "decoderPreference":"auto",
                "nativeCursorOverlay":true,
                "mouseSensitivity":1.0,
                "mouseAcceleration":1.0
            }),
        );
        let environment = command
            .get_envs()
            .filter_map(|(name, value)| Some((name.to_str()?, value?.to_str()?)))
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            environment.get("OPENNOW_NATIVE_EXTERNAL_RENDERER"),
            Some(&"1")
        );
        assert_eq!(
            environment.get("OPENNOW_NATIVE_INPUT_OWNER"),
            Some(&"native")
        );
    }

    #[test]
    fn crashed_child_is_reported_as_a_typed_streamer_failure() {
        #[cfg(target_os = "windows")]
        let mut child = Command::new("cmd")
            .args(["/C", "exit", "23"])
            .spawn()
            .expect("crash fixture");
        #[cfg(not(target_os = "windows"))]
        let mut child = Command::new("sh")
            .args(["-c", "exit 23"])
            .spawn()
            .expect("crash fixture");
        child.wait().expect("crash fixture exit");

        let failure = ensure_child_running(&mut child).expect_err("exited child must fail");
        assert_eq!(failure.code, "streamer_exited");
        assert!(failure.message.contains("23"));
    }

    #[test]
    fn closed_child_protocol_is_not_misreported_as_a_timeout() {
        let (sender, receiver) = mpsc::channel();
        drop(sender);
        let state = Arc::new(Mutex::new(Snapshot::default()));
        let failure = wait_for_child(&receiver, "hello", Duration::from_secs(1), &state)
            .expect_err("closed protocol must fail");
        assert_eq!(failure.code, "streamer_exited");
    }

    #[cfg(unix)]
    #[test]
    fn streamer_service_contains_a_crashing_child_and_publishes_error_state() {
        use std::os::unix::fs::PermissionsExt as _;

        let fixture = std::env::temp_dir().join(format!(
            "opennow-crashing-streamer-{}-{}",
            std::process::id(),
            random_u64()
        ));
        fs::write(
            &fixture,
            "#!/bin/sh\nread -r _line\nprintf '%s\\n' '{\"id\":\"hello\",\"type\":\"ready\",\"processId\":1,\"capabilities\":{\"protocolVersion\":7,\"supportsOwnedNvstNegotiation\":true,\"videoBackends\":[{\"available\":true,\"codecs\":[{\"codec\":\"h264\",\"available\":true}]}]}}'\nexit 23\n",
        )
        .expect("write crash fixture");
        fs::set_permissions(&fixture, fs::Permissions::from_mode(0o700))
            .expect("make crash fixture executable");

        let service = StreamerService::new();
        service
            .start(
                &json!({
                    "session":{
                        "status":2,
                        "sessionId":"crash-fixture",
                        "rtspsEndpoints":["rtsps://seat.nvidiagrid.net:322/session"]
                    }
                }),
                &json!({
                    "nativeStreamerExecutablePath":fixture,
                    "transportMode":"nvst",
                    "codec":"H264",
                    "resolution":"1920x1080"
                }),
            )
            .expect("coordinator starts asynchronously");

        let deadline = Instant::now() + Duration::from_secs(2);
        let state = loop {
            let state = service.status();
            if state["streamer"]["status"] == "error" || Instant::now() >= deadline {
                break state;
            }
            thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(state["streamer"]["status"], "error");
        assert_eq!(state["streamer"]["errorCode"], "streamer_exited");
        assert_eq!(state["streamer"]["processId"], Value::Null);

        fs::remove_file(fixture).expect("remove crash fixture");
    }

    #[test]
    fn embedded_auto_selects_only_supported_codecs_and_preserves_manual_choices() {
        let mut caps = json!({"protocolVersion":7,"videoBackends":[{
            "backend":"d3d11","available":true,"codecs":[
                {"codec":"h264","available":true}, {"codec":"h265","available":true},
                {"codec":"av1","available":false}]}]});
        let settings =
            json!({"codec":"auto","nativeVideoBackend":"auto","colorQuality":"8bit_420"});
        let resolve = |settings: &Value, caps: &Value| {
            StreamerService::embedded_session_settings(settings, caps)
        };
        assert_eq!(resolve(&settings, &caps).unwrap()["codec"], "h265");
        assert_eq!(settings["codec"], "auto"); // never rewrite the user's persisted Auto preference
        caps["videoBackends"][0]["codecs"][1]["available"] = json!(false);
        assert_eq!(resolve(&settings, &caps).unwrap()["codec"], "h264");
        assert!(resolve(&json!({"codec":"av1"}), &caps).is_err());
        assert!(resolve(&json!({"codec":"auto","colorQuality":"10bit_420"}), &caps).is_err());
        for backend in ["d3d12", "vulkan", "software"] {
            let error = resolve(&json!({"nativeVideoBackend":backend}), &caps).unwrap_err();
            assert_eq!(error.code, "streamer_backend_unavailable");
        }
        assert_eq!(
            resolve(&json!({"codec":"h264","nativeVideoBackend":"d3d11"}), &caps).unwrap()["codec"],
            "h264"
        );
        caps["videoBackends"][0]["codecs"][2]["available"] = json!(true);
        assert_eq!(resolve(&settings, &caps).unwrap()["codec"], "av1");
        caps["videoBackends"][0]["available"] = json!(false);
        assert!(resolve(&settings, &caps).is_err());
        caps["videoBackends"][0]["available"] = json!(true);
        caps["videoBackends"][0]["backend"] = json!("ffmpeg");
        assert!(resolve(&settings, &caps).is_err());
        assert!(resolve(&settings, &json!({})).is_err());
    }

    #[test]
    fn embedded_color_selection_cannot_be_silently_reduced_by_cloudmatch() {
        for profiles in [
            None,
            Some(json!(["8bit_420", "10bit_420", "8bit_444", "10bit_444"])),
        ] {
            let mut caps = json!({"protocolVersion":7,"videoBackends":[{
                "backend":"d3d11","platform":"windows","available":true,"codecs":[
                    {"codec":"h264","available":true},
                    {"codec":"h265","available":true},
                    {"codec":"av1","available":true}
                ]
            }]});
            if let Some(profiles) = profiles {
                for codec in caps["videoBackends"][0]["codecs"].as_array_mut().unwrap() {
                    codec["colorQualities"] = profiles.clone();
                }
            }
            for (codec, color) in [
                ("h264", "10bit_420"),
                ("h264", "8bit_444"),
                ("h264", "10bit_444"),
                ("av1", "8bit_444"),
                ("av1", "10bit_444"),
            ] {
                let result = StreamerService::embedded_session_settings(
                    &json!({"codec":codec,"colorQuality":color}),
                    &caps,
                );
                assert_eq!(result.unwrap_err().code, "streamer_codec_unavailable");
            }
            let resolved = StreamerService::embedded_session_settings(
                &json!({"codec":"auto","colorQuality":"10bit_444"}),
                &caps,
            )
            .unwrap();
            assert_eq!(resolved["codec"], "h265");
            assert_eq!(resolved["colorQuality"], "10bit_444");
        }
    }

    #[test]
    fn hdr_preserves_requested_chroma_and_requires_explicit_444_support() {
        let capabilities = json!({"protocolVersion":7,"nativeHdrSupported":true,"videoBackends":[{
            "backend":"d3d11","platform":"windows","available":true,"codecs":[
                {"codec":"h265","available":true,"hdrSupported":true,
                    "colorQualities":["8bit_420","10bit_420","8bit_444","10bit_444"],
                    "hdrColorQualities":["10bit_420","10bit_444"]},
                {"codec":"av1","available":true,"hdrSupported":true,
                    "colorQualities":["8bit_420","10bit_420"]}
            ]
        }]});
        for codec in ["auto", "h265", "hevc"] {
            for color in ["8bit_444", "10bit_444"] {
                let settings = json!({"codec":codec,"colorQuality":color,"enableHdr":true});
                let resolved =
                    StreamerService::embedded_session_settings(&settings, &capabilities).unwrap();
                assert_eq!(resolved["codec"], "h265");
                assert_eq!(resolved["colorQuality"], "10bit_444");
                assert_eq!(settings["colorQuality"], color);
                for qualities in [Value::Null, json!([]), json!(["10bit_420"])] {
                    let mut unsupported = capabilities.clone();
                    unsupported["videoBackends"][0]["codecs"][0]["colorQualities"] = qualities;
                    assert!(
                        StreamerService::embedded_session_settings(&settings, &unsupported)
                            .is_err()
                    );
                }
                let mut unknown = capabilities.clone();
                unknown["videoBackends"][0]["codecs"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("colorQualities");
                assert!(StreamerService::embedded_session_settings(&settings, &unknown).is_err());
                for qualities in [
                    Value::Null,
                    json!([]),
                    json!(["10bit_420"]),
                    json!("10bit_444"),
                ] {
                    let mut unsupported = capabilities.clone();
                    unsupported["videoBackends"][0]["codecs"][0]["hdrColorQualities"] = qualities;
                    assert!(
                        StreamerService::embedded_session_settings(&settings, &unsupported)
                            .is_err()
                    );
                }
                let mut unknown = capabilities.clone();
                unknown["videoBackends"][0]["codecs"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("hdrColorQualities");
                assert!(StreamerService::embedded_session_settings(&settings, &unknown).is_err());
                let mut unsupported = capabilities.clone();
                unsupported["videoBackends"][0]["codecs"][0]["hdrSupported"] = json!(false);
                assert!(
                    StreamerService::embedded_session_settings(&settings, &unsupported).is_err()
                );
            }
        }
        let av1 = json!({"codec":"av1","colorQuality":"10bit_444","enableHdr":true});
        assert!(StreamerService::embedded_session_settings(&av1, &capabilities).is_err());
    }

    #[test]
    fn hdr_444_attachment_preserves_accepted_profile_and_rechecks_capabilities() {
        let capabilities = json!({"protocolVersion":7,"nativeHdrSupported":true,"videoBackends":[{
            "backend":"d3d11","platform":"windows","available":true,"codecs":[
                {"codec":"h265","available":true,"hdrSupported":true,
                    "colorQualities":["10bit_420","10bit_444"],"hdrColorQualities":["10bit_420","10bit_444"]}
            ]
        }]});
        let settings = json!({"codec":"h264","colorQuality":"8bit_420","enableHdr":false});
        let service = StreamerService::new();
        for status in [2, 3] {
            let params = json!({"session":{"sessionId":"hdr-444","status":status,
                "negotiatedStreamProfile":{"codec":"H265","colorQuality":"10bit_444","enableHdr":true}},
                "runtimeCapabilities":capabilities});
            let prepared = service.prepare_embedded(&params, &settings).unwrap();
            assert_eq!(prepared["context"]["settings"]["codec"], "H265");
            assert_eq!(prepared["context"]["settings"]["colorQuality"], "10bit_444");
            assert_eq!(prepared["context"]["settings"]["enableHdr"], true);
            assert_eq!(prepared["context"]["settings"]["nativeHdrSupported"], true);
            for color in ["8bit_420", "8bit_444", "10bit_422"] {
                let mut unsupported = params.clone();
                unsupported["session"]["negotiatedStreamProfile"]["colorQuality"] = json!(color);
                assert!(service.prepare_embedded(&unsupported, &settings).is_err());
            }
            let mut unavailable = params.clone();
            unavailable["runtimeCapabilities"]["videoBackends"][0]["codecs"][0]["colorQualities"] =
                json!(["10bit_420"]);
            assert!(service.prepare_embedded(&unavailable, &settings).is_err());
            unavailable = params.clone();
            unavailable["runtimeCapabilities"]["videoBackends"][0]["codecs"][0]["hdrColorQualities"] =
                json!(["10bit_420"]);
            assert!(service.prepare_embedded(&unavailable, &settings).is_err());
            unavailable = params.clone();
            unavailable["runtimeCapabilities"]["nativeHdrSupported"] = json!(false);
            assert!(service.prepare_embedded(&unavailable, &settings).is_err());
            unavailable["runtimeCapabilities"] = Value::Null;
            assert!(service.prepare_embedded(&unavailable, &settings).is_err());
        }
    }

    #[test]
    fn explicit_hdr_profiles_gate_hdr_without_affecting_sdr() {
        let mut capabilities = json!({"protocolVersion":7,"nativeHdrSupported":true,"videoBackends":[{
            "backend":"d3d11","available":true,"codecs":[
                {"codec":"h265","available":true,"hdrSupported":true,
                    "colorQualities":["8bit_420","10bit_420"],"hdrColorQualities":["10bit_420"]}
            ]
        }]});
        let hdr = json!({"codec":"h265","enableHdr":true});
        assert!(StreamerService::embedded_session_settings(&hdr, &capabilities).is_ok());
        for profiles in [
            Value::Null,
            json!([]),
            json!(["10bit_444"]),
            json!("10bit_420"),
        ] {
            capabilities["videoBackends"][0]["codecs"][0]["hdrColorQualities"] = profiles;
            assert!(StreamerService::embedded_session_settings(&hdr, &capabilities).is_err());
            let sdr = json!({"codec":"h265","enableHdr":false,"colorQuality":"10bit_420"});
            assert!(StreamerService::embedded_session_settings(&sdr, &capabilities).is_ok());
        }
    }

    #[test]
    fn hdr_requires_explicit_output_and_ten_bit_hardware_support() {
        let capabilities = json!({"protocolVersion":7,"nativeHdrSupported":true,"videoBackends":[{
            "backend":"d3d11","available":true,"codecs":[
                {"codec":"h264","available":true,"colorQualities":["8bit_420"]},
                {"codec":"h265","available":true,"colorQualities":["8bit_420","10bit_420"]},
                {"codec":"av1","available":true,"colorQualities":["8bit_420","10bit_420"]}
            ]
        }]});
        let settings = json!({"codec":"auto","colorQuality":"8bit_420","enableHdr":true});
        let resolved =
            StreamerService::embedded_session_settings(&settings, &capabilities).unwrap();
        assert_eq!(resolved["codec"], "h265");
        assert_eq!(resolved["colorQuality"], "10bit_420");
        assert_eq!(resolved["nativeHdrSupported"], true);
        assert_eq!(settings["colorQuality"], "8bit_420");
        for capability in [Value::Null, json!(false), json!("true")] {
            let mut unavailable = capabilities.clone();
            unavailable["nativeHdrSupported"] = capability;
            assert!(StreamerService::embedded_session_settings(&settings, &unavailable).is_err());
        }
        for profile in [Value::Null, json!([]), json!(["8bit_420"])] {
            let mut unavailable = capabilities.clone();
            for codec in unavailable["videoBackends"][0]["codecs"]
                .as_array_mut()
                .unwrap()
            {
                codec["colorQualities"] = profile.clone();
            }
            assert!(StreamerService::embedded_session_settings(&settings, &unavailable).is_err());
        }
        for (key, value) in [
            ("codec", "h264"),
            ("nativeVideoBackend", "software"),
            ("decoderPreference", "software"),
        ] {
            let mut unsupported = settings.clone();
            unsupported[key] = json!(value);
            assert!(
                StreamerService::embedded_session_settings(&unsupported, &capabilities).is_err()
            );
        }
        let mut sdr = settings.clone();
        sdr["enableHdr"] = json!(false);
        let resolved = StreamerService::embedded_session_settings(&sdr, &capabilities).unwrap();
        assert_eq!(resolved["codec"], "av1");
        assert_eq!(resolved["colorQuality"], "8bit_420");
    }

    #[test]
    fn windows_hdr_capability_gates_hdr_without_changing_unknown_sdr_profiles() {
        let mut capabilities = json!({"protocolVersion":7,"nativeHdrSupported":true,"videoBackends":[{
            "backend":"d3d11","platform":"windows","available":true,"codecs":[
                {"codec":"h265","available":true,"hdrSupported":true}
            ]
        }]});
        let hdr = json!({"codec":"auto","enableHdr":true});
        let resolved = StreamerService::embedded_session_settings(&hdr, &capabilities).unwrap();
        assert_eq!(resolved["codec"], "h265");
        assert_eq!(resolved["colorQuality"], "10bit_420");
        for supported in [json!(false), Value::Null, json!("true")] {
            capabilities["videoBackends"][0]["codecs"][0]["hdrSupported"] = supported;
            assert!(StreamerService::embedded_session_settings(&hdr, &capabilities).is_err());
            let sdr = json!({"codec":"h265","colorQuality":"10bit_444","enableHdr":false});
            let resolved = StreamerService::embedded_session_settings(&sdr, &capabilities).unwrap();
            assert_eq!(resolved["colorQuality"], "10bit_444");
        }
        capabilities["videoBackends"][0]["codecs"][0]["hdrSupported"] = json!(false);
        capabilities["videoBackends"][0]["codecs"][0]["colorQualities"] = json!(["10bit_420"]);
        assert!(StreamerService::embedded_session_settings(&hdr, &capabilities).is_err());
        capabilities["videoBackends"][0]["codecs"][0]["hdrSupported"] = json!(true);
        capabilities["videoBackends"][0]["codecs"][0]["colorQualities"] = json!([]);
        assert!(StreamerService::embedded_session_settings(&hdr, &capabilities).is_err());
    }

    #[test]
    fn hdr_resume_uses_accepted_profile_and_rechecks_current_output() {
        let capabilities = json!({"protocolVersion":7,"nativeHdrSupported":true,"videoBackends":[{
            "backend":"d3d11","available":true,"codecs":[
                {"codec":"h265","available":true,"colorQualities":["8bit_420","10bit_420"]}
            ]
        }]});
        let mut params = json!({"session":{"sessionId":"hdr-resume","status":2,
            "negotiatedStreamProfile":{"codec":"H265","colorQuality":"10bit_420","enableHdr":true}},
            "runtimeCapabilities":capabilities});
        let service = StreamerService::new();
        let settings = json!({"codec":"h264","colorQuality":"8bit_420","enableHdr":false});
        let prepared = service.prepare_embedded(&params, &settings).unwrap();
        assert_eq!(prepared["context"]["settings"]["enableHdr"], true);
        assert_eq!(prepared["context"]["settings"]["colorQuality"], "10bit_420");
        assert_eq!(prepared["context"]["settings"]["nativeHdrSupported"], true);
        params["runtimeCapabilities"]["nativeHdrSupported"] = json!(false);
        assert!(service.prepare_embedded(&params, &settings).is_err());
        params["runtimeCapabilities"] = Value::Null;
        assert!(service.prepare_embedded(&params, &settings).is_err());
        params["session"]["negotiatedStreamProfile"]["enableHdr"] = json!(false);
        let stale_hdr = json!({"enableHdr":true,"nativeHdrSupported":true});
        let prepared = service.prepare_embedded(&params, &stale_hdr).unwrap();
        assert_eq!(prepared["context"]["settings"]["enableHdr"], false);
        assert_eq!(prepared["context"]["settings"]["nativeHdrSupported"], false);
    }

    #[test]
    fn embedded_macos_auto_matches_hardware_stream_quality_policy() {
        let caps = json!({"protocolVersion":STREAMER_PROTOCOL_VERSION,"videoBackends":[{
            "backend":"videotoolbox","platform":"macos","available":true,"codecs":[
                {"codec":"h264","available":true,"colorQualities":["8bit_420"]}, {"codec":"h265","available":true,"colorQualities":["8bit_420","10bit_420"]},
                {"codec":"av1","available":true,"colorQualities":["8bit_420","10bit_420"]}]}]});
        for (resolution, fps, bitrate, color, expected) in [
            ("1920x1080", 60, 75, "8bit_420", "h265"),
            ("1920x1080", 60, 50, "8bit_420", "h264"),
            ("2560x1440", 60, 80, "8bit_420", "h265"),
            ("1920x1080", 60, 30, "8bit_420", "av1"),
            ("1920x1080", 60, 31, "8bit_420", "h264"),
            ("3840x2160", 120, 80, "8bit_420", "av1"),
            ("3840x2160", 144, 20, "8bit_420", "h264"),
            ("1920x1080", 240, 75, "8bit_420", "h264"),
            ("1920x1080", 60, 20, "10bit_420", "h265"),
            ("1920x1080", 144, 75, "10bit_420", "h265"),
        ] {
            let settings = json!({"codec":"auto", "resolution":resolution,
                "fps":fps, "maxBitrateMbps":bitrate, "colorQuality":color,
                "transportMode":"nvst"});
            let resolved = StreamerService::embedded_session_settings(&settings, &caps).unwrap();
            assert_eq!(resolved["codec"], expected, "{settings}");
            assert_eq!(resolved["colorQuality"], color);
            assert_eq!(resolved["transportMode"], "nvst");
            assert_eq!(settings["codec"], "auto");
        }
        assert_eq!(
            StreamerService::embedded_session_settings(&json!({"codec":"auto"}), &caps).unwrap()["codec"],
            "h265"
        );
    }

    #[test]
    fn unavailable_embedded_backends_distinguish_auto_from_explicit_selection() {
        let capabilities = json!({"protocolVersion":STREAMER_PROTOCOL_VERSION,"videoBackends":[
            {"backend":"v4l2", "available":false, "reason":"HEVC topology probe failed: errno 13 (Permission denied) token=private-value"},
            {"backend":"cuda", "available":false, "reason":"CUDA unavailable"},
            {"backend":"software", "available":true, "reason":"unused software reason"}
        ]});
        for selection in ["auto", "v4l2", "nvdec"] {
            let error = StreamerService::embedded_session_settings(
                &json!({"nativeVideoBackend":selection}),
                &capabilities,
            )
            .unwrap_err();
            assert_eq!(error.code, "streamer_backend_unavailable");
            assert_eq!(error.message.contains("Select Auto"), selection != "auto");
            assert_eq!(
                error
                    .message
                    .contains("HEVC topology probe failed: errno 13 (Permission denied)"),
                selection == "v4l2"
            );
            assert_eq!(
                error.message.contains("CUDA unavailable"),
                selection == "nvdec"
            );
            assert!(!error.message.contains("private-value"));
            assert!(!error.message.contains("unused software reason"));
        }
        let error =
            StreamerService::embedded_session_settings(&json!({}), &capabilities).unwrap_err();
        assert!(error.message.starts_with("No hardware video backend"));
        let mut oversized = capabilities.clone();
        oversized["videoBackends"][0]["reason"] = json!("failure ".repeat(10000));
        let concise =
            StreamerService::embedded_session_settings(&json!({}), &oversized).unwrap_err();
        assert_eq!(concise.message, error.message);
        assert!(concise.message.len() < 200);
        let error = StreamerService::embedded_session_settings(
            &json!({}),
            &json!({"protocolVersion":STREAMER_PROTOCOL_VERSION,"videoBackends":[]}),
        )
        .unwrap_err();
        assert!(!error.message.contains("Select Auto"));
    }

    #[test]
    fn embedded_macos_codec_policy_never_falls_back_to_unsupported_hardware() {
        let mut caps = json!({"protocolVersion":STREAMER_PROTOCOL_VERSION,"videoBackends":[{
            "backend":"videotoolbox","platform":"macos","available":true,"codecs":[
                {"codec":"h264","available":true,"colorQualities":["8bit_420"]}, {"codec":"h265","available":true,"colorQualities":["8bit_420","10bit_420"]},
                {"codec":"av1","available":false}]},
            {"backend":"software","available":true,"codecs":[
                {"codec":"av1","available":true,"colorQualities":["8bit_420","10bit_420"]}]}]});
        let settings = json!({"codec":"auto", "maxBitrateMbps":20});
        assert_eq!(
            StreamerService::embedded_session_settings(&settings, &caps).unwrap()["codec"],
            "h264"
        );
        assert_eq!(
            StreamerService::embedded_session_settings(
                &json!({"codec":"auto", "maxBitrateMbps":20, "resolution":"3840x2160"}),
                &caps
            )
            .unwrap()["codec"],
            "h265"
        );
        assert_eq!(
            StreamerService::embedded_session_settings(&json!({"codec":"av1"}), &caps)
                .unwrap_err()
                .code,
            "streamer_codec_unavailable"
        );
        assert_eq!(
            StreamerService::embedded_session_settings(&json!({"codec":"h264"}), &caps).unwrap()["codec"],
            "h264"
        );
        caps["videoBackends"][0]["codecs"][1]["available"] = json!(false);
        assert_eq!(
            StreamerService::embedded_session_settings(&settings, &caps).unwrap()["codec"],
            "h264"
        );
        for codec in ["auto", "h264", "h265"] {
            assert_eq!(
                StreamerService::embedded_session_settings(
                    &json!({"codec":codec,"colorQuality":"10bit_420"}),
                    &caps,
                )
                .unwrap_err()
                .code,
                "streamer_codec_unavailable",
            );
        }
        for codec in ["auto", "h264", "h265"] {
            for color in ["8bit_444", "10bit_444"] {
                assert_eq!(
                    StreamerService::embedded_session_settings(
                        &json!({"codec":codec,"colorQuality":color}),
                        &caps
                    )
                    .unwrap_err()
                    .code,
                    "streamer_color_unavailable"
                );
            }
        }
        caps["videoBackends"][0]["available"] = json!(false);
        assert_eq!(
            StreamerService::embedded_session_settings(&settings, &caps)
                .unwrap_err()
                .code,
            "streamer_backend_unavailable"
        );
    }

    #[test]
    fn embedded_macos_accepts_only_probed_hevc_444_profiles() {
        let mut caps = json!({"protocolVersion":7,"videoBackends":[{
            "backend":"videotoolbox","platform":"macos","available":true,
            "codecs":[{"codec":"h265","available":true,"colorQualities":["8bit_420","10bit_420","10bit_444"]}]
        }]});
        for codec in ["auto", "h265"] {
            let settings = json!({"codec":codec,"colorQuality":"10bit_444"});
            let resolved = StreamerService::embedded_session_settings(&settings, &caps).unwrap();
            assert_eq!(resolved["codec"], "h265");
            assert_eq!(resolved["colorQuality"], "10bit_444");
        }
        assert_eq!(
            StreamerService::embedded_session_settings(
                &json!({"codec":"auto","colorQuality":"8bit_444"}),
                &caps
            )
            .unwrap_err()
            .code,
            "streamer_color_unavailable"
        );
        caps["videoBackends"][0]["codecs"][0]["colorQualities"] = json!(["8bit_420", "10bit_420"]);
        assert_eq!(
            StreamerService::embedded_session_settings(
                &json!({"codec":"auto","colorQuality":"10bit_444"}),
                &caps
            )
            .unwrap_err()
            .code,
            "streamer_color_unavailable"
        );
    }

    #[test]
    fn embedded_macos_prepare_checks_negotiated_color_before_attachment() {
        let service = StreamerService::new();
        for color in ["8bit_444", "10bit_444"] {
            let result = service.prepare_embedded(
                &json!({"session": {
                    "sessionId":"resume", "status":2,
                    "negotiatedStreamProfile":{"codec":"H265", "colorQuality":color}
                }, "runtimeCapabilities":{"protocolVersion":STREAMER_PROTOCOL_VERSION,"videoBackends":[{
                    "backend":"videotoolbox","platform":"macos","available":true,
                    "codecs":[{"codec":"h265","available":true,"colorQualities":["8bit_420","10bit_420"]}]}]}}),
                &json!({"codec":"auto", "colorQuality":"8bit_420"}),
            );
            assert_eq!(result.unwrap_err().code, "streamer_color_unavailable");
        }
        let prepared = service.prepare_embedded(
            &json!({"session": {
                "sessionId":"resume-ten-bit", "status":2,
                "negotiatedStreamProfile":{"codec":"H265", "colorQuality":"10bit_420"}
            }, "runtimeCapabilities":{"protocolVersion":STREAMER_PROTOCOL_VERSION,"videoBackends":[{
                "backend":"videotoolbox","platform":"macos","available":true,
                "codecs":[{"codec":"h265","available":true,"colorQualities":["8bit_420","10bit_420"]}]}]}}),
            &json!({"codec":"auto", "colorQuality":"8bit_420"}),
        ).unwrap();
        assert_eq!(prepared["context"]["settings"]["colorQuality"], "10bit_420");
    }

    #[test]
    fn embedded_prepare_rejects_an_incompatible_resumed_session() {
        let service = StreamerService::new();
        let result = service.prepare_embedded(
            &json!({"session": {
            "sessionId":"resume", "status":2, "negotiatedStreamProfile":{"codec":"AV1"}
        }, "runtimeCapabilities":{"protocolVersion":7,"videoBackends":[{
            "backend":"d3d11","available":true,"codecs":[{"codec":"h264","available":true}]}]}}),
            &json!({"codec":"auto"}),
        );
        assert_eq!(result.unwrap_err().code, "streamer_codec_unavailable");
    }

    #[test]
    fn embedded_color_profiles_gate_auto_and_manual_before_allocation() {
        let mut capabilities = json!({"protocolVersion":7,"videoBackends":[{
            "backend":"vulkan", "platform":"linux", "available":true, "codecs":[
                {"codec":"h264","available":true,"colorQualities":["8bit_420"]},
                {"codec":"h265","available":true,"colorQualities":["8bit_420"]},
                {"codec":"av1","available":true,"colorQualities":["8bit_420"]}
            ]
        }]});
        for codec in ["auto", "h265", "av1"] {
            let settings =
                json!({"codec":codec,"nativeVideoBackend":"auto","colorQuality":"10bit_420"});
            let error =
                StreamerService::embedded_session_settings(&settings, &capabilities).unwrap_err();
            assert_eq!(error.code, "streamer_codec_unavailable");
        }
        capabilities["videoBackends"][0]["codecs"][1]["colorQualities"] =
            json!(["8bit_420", "10bit_420"]);
        let settings =
            json!({"codec":"auto","nativeVideoBackend":"auto","colorQuality":"10bit_420"});
        let resolved =
            StreamerService::embedded_session_settings(&settings, &capabilities).unwrap();
        assert_eq!(resolved["codec"], "h265");
        assert_eq!(resolved["colorQuality"], "10bit_420");
        assert_eq!(settings["codec"], "auto");
        for color in ["8bit_444", "10bit_444"] {
            for codec in ["auto", "h265"] {
                let settings = json!({"codec":codec,"colorQuality":color});
                assert_eq!(
                    StreamerService::embedded_session_settings(&settings, &capabilities)
                        .unwrap_err()
                        .code,
                    "streamer_codec_unavailable"
                );
            }
        }
        let settings =
            json!({"codec":"h265","nativeVideoBackend":"vulkan","colorQuality":"10bit_420"});
        assert!(StreamerService::embedded_session_settings(&settings, &capabilities).is_ok());
        capabilities["videoBackends"][0]["codecs"][1]["available"] = json!(false);
        assert!(StreamerService::embedded_session_settings(&settings, &capabilities).is_err());
    }

    #[test]
    fn embedded_linux_advanced_color_requires_explicit_well_formed_profiles() {
        for backend in ["vulkan", "cuda"] {
            for profiles in [
                None,
                Some(Value::Null),
                Some(json!([])),
                Some(json!("10bit_420")),
            ] {
                let mut capabilities = json!({"protocolVersion":7,"videoBackends":[{
                    "backend":backend,"platform":"linux","available":true,
                    "codecs":[{"codec":"h265","available":true}]
                }]});
                if let Some(profiles) = profiles {
                    capabilities["videoBackends"][0]["codecs"][0]["colorQualities"] = profiles;
                }
                for color in ["10bit_420", "8bit_444", "10bit_444"] {
                    let settings = json!({"codec":"h265","colorQuality":color});
                    assert_eq!(
                        StreamerService::embedded_session_settings(&settings, &capabilities)
                            .unwrap_err()
                            .code,
                        "streamer_codec_unavailable"
                    );
                }
            }
        }
    }

    #[test]
    fn embedded_color_profiles_cannot_be_borrowed_from_another_backend() {
        let capabilities = json!({"protocolVersion":7,"videoBackends":[
            {"backend":"vulkan","platform":"linux","available":true,
                "codecs":[{"codec":"h265","available":true,"colorQualities":["8bit_420"]}]},
            {"backend":"other","available":true,
                "codecs":[{"codec":"h265","available":true,"colorQualities":["10bit_420"]}]}
        ]});
        let settings =
            json!({"codec":"h265","nativeVideoBackend":"vulkan","colorQuality":"10bit_420"});
        assert_eq!(
            StreamerService::embedded_session_settings(&settings, &capabilities)
                .unwrap_err()
                .code,
            "streamer_codec_unavailable"
        );
    }

    #[test]
    fn codec_capabilities_require_an_available_backend_and_codec() {
        let capabilities = json!({
            "protocolVersion":7,
            "videoBackends":[
                {"backend":"hardware","available":true,"codecs":[
                    {"codec":"h264","available":true},
                    {"codec":"h265","available":false,"reason":"not installed"}
                ]},
                {"backend":"software","available":false,"codecs":[
                    {"codec":"av1","available":true}
                ]}
            ]
        });
        assert_eq!(available_codecs(&capabilities), vec!["h264"]);
        assert!(ensure_codec_available(&capabilities, "h264").is_ok());
        let hevc = ensure_codec_available(&capabilities, "h265").expect_err("HEVC must fail");
        assert_eq!(hevc.code, "streamer_codec_unavailable");
        assert!(hevc.message.contains("not installed"));
        assert!(ensure_codec_available(&capabilities, "av1").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn detect_performs_a_protocol_handshake_and_reports_real_codecs() {
        use std::os::unix::fs::PermissionsExt as _;

        let fixture = std::env::temp_dir().join(format!(
            "opennow-capability-streamer-{}-{}",
            std::process::id(),
            random_u64()
        ));
        fs::write(
            &fixture,
            "#!/bin/sh\nread -r _hello\nprintf '%s\\n' '{\"id\":\"hello\",\"type\":\"ready\",\"processId\":1,\"capabilities\":{\"protocolVersion\":7,\"supportsOwnedNvstNegotiation\":true,\"videoBackends\":[{\"backend\":\"software\",\"available\":true,\"codecs\":[{\"codec\":\"h264\",\"available\":true},{\"codec\":\"av1\",\"available\":false,\"reason\":\"not built\"}]}]}}'\nread -r _shutdown\nprintf '%s\\n' '{\"id\":\"shutdown\",\"type\":\"ok\"}'\n",
        )
        .expect("write capability fixture");
        fs::set_permissions(&fixture, fs::Permissions::from_mode(0o700))
            .expect("make capability fixture executable");

        let service = StreamerService::new();
        let detected = service
            .detect(&json!({"nativeStreamerExecutablePath":fixture}))
            .expect("capability probe");
        assert_eq!(detected["protocolVersion"], 7);
        assert_eq!(detected["availableCodecs"], json!(["h264"]));
        assert_eq!(
            detected["capabilities"]["videoBackends"][0]["backend"],
            "software"
        );

        fs::remove_file(fixture).expect("remove capability fixture");
    }

    #[test]
    fn context_resolves_auto_to_concrete_h264() {
        let context = streamer_context(
            json!({"sessionId":"one","status":2,"signalingUrl":"wss://server.nvidiagrid.net/nvst/"}),
            &json!({"codec":"auto","transportMode":"webrtc","resolution":"1920x1080"}),
        );
        assert_eq!(context["settings"]["codec"], "H264");
        assert_eq!(context["settings"]["transportMode"], "nvst");
        assert_eq!(context_resolution(&context), (1920, 1080));
    }

    #[test]
    fn embedded_prepare_returns_nvst_context_without_a_native_surface() {
        let service = StreamerService::new();
        let prepared = service
            .prepare_embedded(
                &json!({
                    "session": {
                        "sessionId": "session-one",
                        "status": 2,
                        "signalingUrl": "wss://server.nvidiagrid.net/nvst/"
                    }
                }),
                &json!({
                    "codec":"auto",
                    "transportMode":"webrtc",
                    "resolution":"1920x1080",
                    "maxBitrateMbps":200
                }),
            )
            .expect("embedded context");

        assert_eq!(prepared["protocolVersion"], STREAMER_PROTOCOL_VERSION);
        assert_eq!(prepared["context"]["session"]["sessionId"], "session-one");
        assert_eq!(prepared["context"]["settings"]["codec"], "H264");
        assert_eq!(prepared["context"]["settings"]["transportMode"], "nvst");
        assert_eq!(prepared["context"]["settings"]["maxBitrateMbps"], 200);
        assert_eq!(prepared["context"]["surface"], Value::Null);
        assert!(service.worker.lock().expect("streamer worker").is_none());
    }

    #[test]
    fn embedded_prepare_preserves_replay_opt_in_and_capture_bindings() {
        for enabled in [false, true] {
            let service = StreamerService::new();
            let prepared = service
                .prepare_embedded(
                    &json!({"session": {"sessionId": "replay-test", "status": 2}}),
                    &json!({"codec": "h264", "replayBufferEnabled": enabled,
                        "replayBufferSeconds": 60, "replayBufferMemoryMiB": 128,
                        "shortcutToggleRecording": "F12", "shortcutSaveClip": "Alt+F9"}),
                )
                .unwrap();
            assert_eq!(
                prepared["context"]["settings"]["replayBufferEnabled"],
                enabled
            );
            assert_eq!(prepared["context"]["settings"]["replayBufferSeconds"], 60);
            assert_eq!(
                prepared["context"]["settings"]["replayBufferMemoryMiB"],
                128
            );
            assert_eq!(prepared["context"]["shortcuts"]["toggleRecording"], "F12");
            assert_eq!(prepared["context"]["shortcuts"]["saveClip"], "Alt+F9");
        }
    }

    #[test]
    fn embedded_prepare_preserves_selected_audio_output() {
        for device in [
            "",
            "pipewire:alsa_output.usb-headphones",
            "coreaudio:BuiltInSpeakerDevice",
            "USB Headphones",
        ] {
            let service = StreamerService::new();
            let prepared = service
                .prepare_embedded(
                    &json!({"session": {"sessionId": "audio-test", "status": 2}}),
                    &json!({"codec": "h264", "audioOutputDevice": device}),
                )
                .unwrap();
            assert_eq!(prepared["context"]["settings"]["audioOutputDevice"], device);
        }
    }

    #[test]
    fn microphone_child_state_is_exposed_without_losing_the_message() {
        let state = Arc::new(Mutex::new(Snapshot::default()));
        apply_microphone_state(
            &json!({
                "type":"microphone-state",
                "state":"ready",
                "enabled":true,
                "message":"Microphone is streaming"
            }),
            &state,
        );

        let value = state.lock().expect("snapshot").value();
        assert_eq!(value["streamer"]["microphoneState"], "ready");
        assert_eq!(value["streamer"]["microphoneEnabled"], true);
        assert_eq!(
            value["streamer"]["microphoneMessage"],
            "Microphone is streaming"
        );
    }

    #[test]
    fn native_shell_shortcuts_are_forwarded_to_qt_and_counted_once() {
        let state = Arc::new(Mutex::new(Snapshot::default()));
        handle_nvst_child_event(
            &json!({"type":"shortcut-action","action":"toggle-stats"}),
            &state,
        )
        .expect("supported shortcut action");
        let value = state.lock().expect("snapshot").value();
        assert_eq!(value["streamer"]["shortcutAction"], "toggle-stats");
        assert_eq!(value["streamer"]["shortcutActionGeneration"], 1);
        assert_eq!(value["streamer"]["statsToggleCount"], 1);

        handle_nvst_child_event(
            &json!({"type":"shortcut-action","action":"toggle-fullscreen"}),
            &state,
        )
        .expect("supported fullscreen shortcut action");
        let value = state.lock().expect("snapshot").value();
        assert_eq!(value["streamer"]["shortcutAction"], "toggle-fullscreen");
        assert_eq!(value["streamer"]["shortcutActionGeneration"], 2);
        assert_eq!(value["streamer"]["fullscreenToggleCount"], 1);

        let failure = handle_nvst_child_event(
            &json!({"type":"shortcut-action","action":"launch-command"}),
            &state,
        )
        .expect_err("unsupported shortcut action must fail closed");
        assert_eq!(failure.code, "streamer_protocol_error");
        assert_eq!(
            state.lock().expect("snapshot").shortcut_action_generation,
            2
        );
    }

    #[test]
    fn structured_child_events_accumulate_redacted_acceptance_evidence() {
        let state = Arc::new(Mutex::new(Snapshot {
            started_at: Instant::now().checked_sub(Duration::from_millis(25)),
            ..Snapshot::default()
        }));
        for event in [
            json!({"type":"input-ready","protocolVersion":1}),
            json!({"type":"log","event":"backend-fallback","fromBackend":"hardware","toBackend":"software"}),
            json!({"type":"log","event":"queue-dropped","media":"video","count":3}),
            json!({"type":"log","event":"device-state","subsystem":"video","recovered":false}),
            json!({"type":"log","event":"device-state","subsystem":"video","recovered":true}),
            json!({"type":"status","event":"first-frame","backend":"ffmpeg","status":"streaming"}),
            json!({"type":"telemetry","framesPerSecond":59.8,"bitrateMbps":42.5,"peakBitrateMbps":51.0}),
        ] {
            apply_child_telemetry(&event, &state);
        }

        let value = state.lock().expect("snapshot").acceptance_value();
        assert_eq!(value["kind"], "opennow.stream.acceptance");
        assert_eq!(value["mediaBackend"], "ffmpeg");
        assert!(value["firstFrameLatencyMs"].as_u64().unwrap_or_default() >= 25);
        assert_eq!(value["backendFallbackCount"], 1);
        assert_eq!(value["queueDropCount"], 3);
        assert_eq!(value["deviceLossCount"], 1);
        assert_eq!(value["deviceRecoveryCount"], 1);
        assert_eq!(value["inputReady"], true);
        assert_eq!(value["framesPerSecond"], 59.8);
        assert_eq!(value["bitrateMbps"], 42.5);
        assert_eq!(value["peakBitrateMbps"], 51.0);
        assert!(value.get("sessionId").is_none());
        assert!(value.get("executable").is_none());
        assert!(value.get("processId").is_none());
    }

    #[test]
    fn dynamic_surface_preserves_embedding_handle_and_normalizes_geometry() {
        let surface = normalize_surface(
            Some(&json!({
                "rect":{"x":32,"y":64,"width":1600,"height":900},
                "screenRect":{"x":-120,"y":48,"width":1600,"height":900},
                "visible":true,
                "deviceScaleFactor":1.5,
                "windowHandle":"0x1234"
            })),
            (1280, 720),
        )
        .unwrap();
        assert_eq!(
            surface["rect"],
            json!({"x":32,"y":64,"width":1600,"height":900})
        );
        assert_eq!(surface["screenRect"]["x"], -120);
        assert_eq!(surface["deviceScaleFactor"], 1.5);
        assert_eq!(surface["windowHandle"], "0x1234");
        assert!(
            normalize_surface(
                Some(&json!({"screenRect":{"width":32,"height":900}})),
                (1280, 720)
            )
            .is_err()
        );
    }

    #[test]
    fn nvst_context_preserves_explicit_transport() {
        let context = streamer_context(
            json!({"sessionId":"one","status":2,"rtspsEndpoints":["rtsps://203.0.113.20:322/session"]}),
            &json!({"codec":"H264","transportMode":"nvst","resolution":"1920x1080"}),
        );
        assert_eq!(context["settings"]["transportMode"], "nvst");
    }
}
