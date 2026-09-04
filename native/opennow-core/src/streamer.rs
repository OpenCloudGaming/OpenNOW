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

const STREAMER_PROTOCOL_VERSION: u64 = 5;
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
                    snapshot.microphone_state = "unavailable".to_owned();
                    snapshot.microphone_enabled = false;
                    snapshot.microphone_message = Some(
                        "Microphone upstream is not available on the native NVST runtime"
                            .to_owned(),
                    );
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
            message: "Native streamer cannot start NVST: owned NVST negotiation and protocol 5 are required"
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
            "toggleRecording":settings["shortcutToggleRecording"]
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
            "#!/bin/sh\nread -r _line\nprintf '%s\\n' '{\"id\":\"hello\",\"type\":\"ready\",\"processId\":1,\"capabilities\":{\"protocolVersion\":5,\"supportsOwnedNvstNegotiation\":true,\"videoBackends\":[{\"available\":true,\"codecs\":[{\"codec\":\"h264\",\"available\":true}]}]}}'\nexit 23\n",
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
    fn codec_capabilities_require_an_available_backend_and_codec() {
        let capabilities = json!({
            "protocolVersion":5,
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
            "#!/bin/sh\nread -r _hello\nprintf '%s\\n' '{\"id\":\"hello\",\"type\":\"ready\",\"processId\":1,\"capabilities\":{\"protocolVersion\":5,\"supportsOwnedNvstNegotiation\":true,\"videoBackends\":[{\"backend\":\"software\",\"available\":true,\"codecs\":[{\"codec\":\"h264\",\"available\":true},{\"codec\":\"av1\",\"available\":false,\"reason\":\"not built\"}]}]}}'\nread -r _shutdown\nprintf '%s\\n' '{\"id\":\"shutdown\",\"type\":\"ok\"}'\n",
        )
        .expect("write capability fixture");
        fs::set_permissions(&fixture, fs::Permissions::from_mode(0o700))
            .expect("make capability fixture executable");

        let service = StreamerService::new();
        let detected = service
            .detect(&json!({"nativeStreamerExecutablePath":fixture}))
            .expect("capability probe");
        assert_eq!(detected["protocolVersion"], 5);
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
