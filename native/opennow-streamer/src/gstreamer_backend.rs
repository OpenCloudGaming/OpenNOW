use crate::backend::{
    normalize_bitrate_kbps, prepare_native_offer, prepared_offer_events,
    update_context_bitrate_limit, BackendReply, NativeStreamerBackend,
};
use crate::input::{
    GamepadInput, InputEncoder, KeyboardPayload, MouseButtonPayload, MouseMovePayload,
    MouseWheelPayload, GAMEPAD_MAX_CONTROLLERS, PARTIALLY_RELIABLE_GAMEPAD_MASK_ALL,
};
use crate::protocol::{
    missing_field, CommandEnvelope, Event, IceCandidatePayload, NativeQueueMode, NativeRenderRect,
    NativeRenderSurface, NativeStreamerCapabilities, NativeStreamerSessionContext,
    NativeVideoBackendCapability, NativeVideoCodecCapability, Response, SendAnswerRequest,
    StreamSettings, VideoStallEvent, VideoTransitionEvent, PROTOCOL_VERSION,
};
use crate::sdp::{
    build_nvst_sdp_for_answer, extract_negotiated_video_codec, munge_answer_sdp, IceCredentials,
};
use gst::glib;
use gst::prelude::*;
use gst_video::prelude::*;
use gstreamer as gst;
use gstreamer_sdp as gst_sdp;
use gstreamer_video as gst_video;
use gstreamer_webrtc as gst_webrtc;
use std::collections::HashSet;
use std::ffi::{c_void, CString};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const RELIABLE_INPUT_CHANNEL_LABEL: &str = "input_channel_v1";
const PARTIALLY_RELIABLE_INPUT_CHANNEL_LABEL: &str = "input_channel_partially_reliable";
const DEFAULT_PARTIAL_RELIABLE_THRESHOLD_MS: u32 = 300;
const WEBRTC_LATENCY_MS: u32 = 2;
const VIDEO_COMPRESSED_QUEUE_MAX_BUFFERS: u32 = 6;
const VIDEO_QUEUE_MAX_BUFFERS: u32 = 1;
const AUDIO_QUEUE_MAX_BUFFERS: u32 = 2;
const VIDEO_SINK_RATE_LOG_INTERVAL: Duration = Duration::from_secs(1);
const VIDEO_STALL_WARNING_MS: u64 = 2_500;
const VIDEO_STALL_SECOND_ATTEMPT_MS: u64 = 5_000;
const VIDEO_STALL_RESYNC_MS: u64 = 8_000;
const VIDEO_STALL_PARTIAL_FLUSH_MS: u64 = 12_000;
const VIDEO_STALL_COMPLETE_FLUSH_MS: u64 = 16_000;
const VIDEO_STALL_FATAL_MS: u64 = 20_000;
const VIDEO_STALL_MIN_KEYFRAME_REQUEST_MS: u64 = 2_000;
const VIDEO_STARTUP_KEYFRAME_MS: u64 = 2_500;
const VIDEO_STARTUP_RESYNC_MS: u64 = 5_000;
const VIDEO_STARTUP_FATAL_MS: u64 = 8_000;
const VIDEO_LIVENESS_POLL_INTERVAL: Duration = Duration::from_millis(250);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(2);
const HEARTBEAT_STOP_POLL_INTERVAL: Duration = Duration::from_millis(50);
const NATIVE_INPUT_BRIDGE_POLL_INTERVAL: Duration = Duration::from_millis(1);
const NATIVE_INPUT_DRAIN_MAX_EVENTS: usize = 512;
const NATIVE_GAMEPAD_POLL_INTERVAL: Duration = Duration::from_millis(4);
const NATIVE_GAMEPAD_KEEPALIVE_INTERVAL: Duration = Duration::from_millis(100);
const EXTERNAL_RENDERER_ENV: &str = "OPENNOW_NATIVE_EXTERNAL_RENDERER";
const NATIVE_VIDEO_API_ENV: &str = "OPENNOW_NATIVE_VIDEO_API";
const NATIVE_VIDEO_BACKEND_ENV: &str = "OPENNOW_NATIVE_VIDEO_BACKEND";
const NATIVE_ZERO_COPY_ENV: &str = "OPENNOW_NATIVE_ZERO_COPY";
const NATIVE_PRESENT_MAX_FPS_ENV: &str = "OPENNOW_NATIVE_PRESENT_MAX_FPS";
const NATIVE_D3D_FULLSCREEN_ENV: &str = "OPENNOW_NATIVE_D3D_FULLSCREEN";
const PRESENT_LIMITER_AUTO_SENTINEL: u32 = u32::MAX;

#[cfg(target_os = "windows")]
static NATIVE_INPUT_STARTED_AT: OnceLock<Instant> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq)]
struct VideoRateSnapshot {
    encoded_kbps: f64,
    decoded_fps: f64,
    sink_fps: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VideoStallAction {
    None,
    RequestKeyframe { attempt: u8, stall_ms: u64 },
    Resync { attempt: u8, stall_ms: u64 },
    PartialFlush { attempt: u8, stall_ms: u64 },
    CompleteFlush { attempt: u8, stall_ms: u64 },
    Fatal { attempt: u8, stall_ms: u64 },
    Recovered { stall_ms: u64 },
}

#[derive(Debug, Clone)]
struct VideoStallTracker {
    in_stall: bool,
    stall_started_ms: u64,
    last_request_ms: Option<u64>,
    next_attempt: u8,
}

impl Default for VideoStallTracker {
    fn default() -> Self {
        Self {
            in_stall: false,
            stall_started_ms: 0,
            last_request_ms: None,
            next_attempt: 1,
        }
    }
}

impl VideoStallTracker {
    fn evaluate(&mut self, now_ms: u64, last_video_ms: u64) -> VideoStallAction {
        let stall_ms = now_ms.saturating_sub(last_video_ms);
        if stall_ms < VIDEO_STALL_WARNING_MS {
            if self.in_stall {
                let recovered_ms = now_ms.saturating_sub(self.stall_started_ms);
                *self = Self::default();
                return VideoStallAction::Recovered {
                    stall_ms: recovered_ms,
                };
            }
            return VideoStallAction::None;
        }

        if !self.in_stall {
            self.in_stall = true;
            self.stall_started_ms = last_video_ms;
            self.next_attempt = 1;
        }

        let next_due_ms = match self.next_attempt {
            1 => VIDEO_STALL_WARNING_MS,
            2 => VIDEO_STALL_SECOND_ATTEMPT_MS,
            3 => VIDEO_STALL_RESYNC_MS,
            4 => VIDEO_STALL_PARTIAL_FLUSH_MS,
            5 => VIDEO_STALL_COMPLETE_FLUSH_MS,
            6 => VIDEO_STALL_FATAL_MS,
            _ => return VideoStallAction::None,
        };
        if stall_ms < next_due_ms {
            return VideoStallAction::None;
        }
        if self
            .last_request_ms
            .is_some_and(|last| now_ms.saturating_sub(last) < VIDEO_STALL_MIN_KEYFRAME_REQUEST_MS)
        {
            return VideoStallAction::None;
        }

        let attempt = self.next_attempt;
        self.next_attempt = self.next_attempt.saturating_add(1);
        self.last_request_ms = Some(now_ms);
        match attempt {
            1 | 2 => VideoStallAction::RequestKeyframe { attempt, stall_ms },
            3 => VideoStallAction::Resync { attempt, stall_ms },
            4 => VideoStallAction::PartialFlush { attempt, stall_ms },
            5 => VideoStallAction::CompleteFlush { attempt, stall_ms },
            _ => VideoStallAction::Fatal { attempt, stall_ms },
        }
    }
}

#[derive(Debug, Clone)]
struct TransitionSnapshot {
    transition_type: String,
    source: String,
    at_ms: u64,
    old_caps: Option<String>,
    new_caps: Option<String>,
    old_framerate: Option<String>,
    new_framerate: Option<String>,
    old_memory_mode: Option<String>,
    new_memory_mode: Option<String>,
    render_gap_ms: Option<u64>,
    requested_fps: Option<u32>,
    caps_framerate: Option<String>,
    high_fps_risk: bool,
    queue_mode: NativeQueueMode,
    summary: String,
}

impl TransitionSnapshot {
    fn to_event(&self) -> VideoTransitionEvent {
        VideoTransitionEvent {
            transition_type: self.transition_type.clone(),
            source: self.source.clone(),
            at_ms: self.at_ms,
            old_caps: self.old_caps.clone(),
            new_caps: self.new_caps.clone(),
            old_framerate: self.old_framerate.clone(),
            new_framerate: self.new_framerate.clone(),
            old_memory_mode: self.old_memory_mode.clone(),
            new_memory_mode: self.new_memory_mode.clone(),
            render_gap_ms: self.render_gap_ms,
            requested_fps: self.requested_fps,
            caps_framerate: self.caps_framerate.clone(),
            high_fps_risk: self.high_fps_risk,
            queue_mode: self.queue_mode.as_str().to_owned(),
            summary: self.summary.clone(),
        }
    }
}

#[derive(Debug, Clone)]
struct TransitionTelemetry {
    queue_mode: NativeQueueMode,
    queue_depth: u32,
    queue_depth_changes: u32,
    present_pacing_changes: u32,
    partial_flush_count: u32,
    complete_flush_count: u32,
    last_transition: Option<TransitionSnapshot>,
}

impl Default for TransitionTelemetry {
    fn default() -> Self {
        Self {
            queue_mode: NativeQueueMode::Auto,
            queue_depth: VIDEO_QUEUE_MAX_BUFFERS,
            queue_depth_changes: 0,
            present_pacing_changes: 0,
            partial_flush_count: 0,
            complete_flush_count: 0,
            last_transition: None,
        }
    }
}

#[derive(Debug)]
struct VideoLivenessState {
    started_at: Instant,
    codec: Mutex<String>,
    resolution: Mutex<String>,
    hardware_acceleration: Mutex<String>,
    memory_mode: Mutex<String>,
    caps_framerate: Mutex<Option<String>>,
    requested_streaming_features_summary: Mutex<String>,
    finalized_streaming_features_summary: Mutex<String>,
    transition_telemetry: Mutex<TransitionTelemetry>,
    stats_overlay: Mutex<Option<gst::Element>>,
    pre_decode_queue: Mutex<Option<gst::Element>>,
    decoder: Mutex<Option<gst::Element>>,
    post_decode_queue: Mutex<Option<gst::Element>>,
    stats_overlay_visible: AtomicBool,
    target_bitrate_kbps: AtomicU32,
    encoded_bytes_total: AtomicU64,
    last_encoded_ms: AtomicU64,
    last_decoded_ms: AtomicU64,
    last_sink_ms: AtomicU64,
    last_audio_ms: AtomicU64,
    first_startup_audio_ms: AtomicU64,
    decoded_total: AtomicU64,
    sink_total: AtomicU64,
    zero_copy_d3d11: AtomicBool,
    zero_copy_d3d12: AtomicBool,
    rtp_video_src_pad: Mutex<Option<gst::Pad>>,
    requested_fps: AtomicU32,
    framerate_mismatch_warned: AtomicBool,
    transition_flush_escalation_enabled: AtomicBool,
    first_encoded_logged: AtomicBool,
    startup_keyframe_requested: AtomicBool,
    startup_resync_requested: AtomicBool,
    startup_fatal_reported: AtomicBool,
}

impl VideoLivenessState {
    fn new() -> Self {
        Self {
            started_at: Instant::now(),
            codec: Mutex::new(String::new()),
            resolution: Mutex::new(String::new()),
            hardware_acceleration: Mutex::new(String::new()),
            memory_mode: Mutex::new("system-memory".to_owned()),
            caps_framerate: Mutex::new(None),
            requested_streaming_features_summary: Mutex::new("none".to_owned()),
            finalized_streaming_features_summary: Mutex::new("none".to_owned()),
            transition_telemetry: Mutex::new(TransitionTelemetry::default()),
            stats_overlay: Mutex::new(None),
            pre_decode_queue: Mutex::new(None),
            decoder: Mutex::new(None),
            post_decode_queue: Mutex::new(None),
            stats_overlay_visible: AtomicBool::new(false),
            target_bitrate_kbps: AtomicU32::new(0),
            encoded_bytes_total: AtomicU64::new(0),
            last_encoded_ms: AtomicU64::new(0),
            last_decoded_ms: AtomicU64::new(0),
            last_sink_ms: AtomicU64::new(0),
            last_audio_ms: AtomicU64::new(0),
            first_startup_audio_ms: AtomicU64::new(0),
            decoded_total: AtomicU64::new(0),
            sink_total: AtomicU64::new(0),
            zero_copy_d3d11: AtomicBool::new(false),
            zero_copy_d3d12: AtomicBool::new(false),
            rtp_video_src_pad: Mutex::new(None),
            requested_fps: AtomicU32::new(0),
            framerate_mismatch_warned: AtomicBool::new(false),
            transition_flush_escalation_enabled: AtomicBool::new(true),
            first_encoded_logged: AtomicBool::new(false),
            startup_keyframe_requested: AtomicBool::new(false),
            startup_resync_requested: AtomicBool::new(false),
            startup_fatal_reported: AtomicBool::new(false),
        }
    }

    fn now_ms(&self) -> u64 {
        self.started_at
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64
    }

    fn configure(&self, context: &NativeStreamerSessionContext, target_bitrate_kbps: u32) {
        let settings = &context.settings;
        if let Ok(mut codec) = self.codec.lock() {
            *codec = settings.codec.as_str().to_owned();
        }
        if let Ok(mut resolution) = self.resolution.lock() {
            *resolution = settings.resolution.clone();
        }
        if let Ok(mut caps_framerate) = self.caps_framerate.lock() {
            *caps_framerate = None;
        }
        if let Ok(mut requested_summary) = self.requested_streaming_features_summary.lock() {
            *requested_summary = context
                .session
                .requested_streaming_features
                .as_ref()
                .map(|features| features.summary())
                .unwrap_or_else(|| "none".to_owned());
        }
        if let Ok(mut finalized_summary) = self.finalized_streaming_features_summary.lock() {
            *finalized_summary = context
                .session
                .finalized_streaming_features
                .as_ref()
                .map(|features| features.summary())
                .unwrap_or_else(|| "none".to_owned());
        }
        if let Ok(mut telemetry) = self.transition_telemetry.lock() {
            telemetry.queue_mode = resolve_queue_mode(settings);
            telemetry.queue_depth = VIDEO_QUEUE_MAX_BUFFERS;
            telemetry.queue_depth_changes = 0;
            telemetry.present_pacing_changes = 0;
            telemetry.partial_flush_count = 0;
            telemetry.complete_flush_count = 0;
            telemetry.last_transition = None;
        }
        self.target_bitrate_kbps
            .store(target_bitrate_kbps, Ordering::Relaxed);
        self.requested_fps.store(settings.fps, Ordering::Relaxed);
        self.framerate_mismatch_warned
            .store(false, Ordering::Relaxed);
        self.first_encoded_logged.store(false, Ordering::Relaxed);
        self.first_startup_audio_ms.store(0, Ordering::Relaxed);
        self.transition_flush_escalation_enabled.store(
            settings
                .native_transition_diagnostics
                .as_ref()
                .map(|diagnostics| !diagnostics.disable_transition_flush_escalation)
                .unwrap_or(true),
            Ordering::Relaxed,
        );
        self.startup_keyframe_requested
            .store(false, Ordering::Relaxed);
        self.startup_resync_requested
            .store(false, Ordering::Relaxed);
        self.startup_fatal_reported.store(false, Ordering::Relaxed);
    }

    fn update_hardware_acceleration(&self, value: impl Into<String>) {
        if let Ok(mut hardware_acceleration) = self.hardware_acceleration.lock() {
            *hardware_acceleration = value.into();
        }
    }

    fn record_encoded_buffer(&self, size: usize) {
        self.last_encoded_ms.store(self.now_ms(), Ordering::Relaxed);
        self.encoded_bytes_total
            .fetch_add(size as u64, Ordering::Relaxed);
    }

    fn record_audio_buffer(&self) {
        let now_ms = self.now_ms();
        self.last_audio_ms.store(now_ms, Ordering::Relaxed);
        if self.last_sink_ms.load(Ordering::Relaxed) == 0 {
            let _ = self.first_startup_audio_ms.compare_exchange(
                0,
                now_ms,
                Ordering::Relaxed,
                Ordering::Relaxed,
            );
        }
    }

    fn log_first_encoded_once(&self) -> bool {
        !self.first_encoded_logged.swap(true, Ordering::Relaxed)
    }

    fn set_stats_overlay(&self, overlay: Option<gst::Element>) {
        if let Ok(mut current) = self.stats_overlay.lock() {
            *current = overlay;
        }
    }

    fn set_stats_overlay_visible(&self, visible: bool) {
        self.stats_overlay_visible.store(visible, Ordering::Relaxed);
        if let Ok(current) = self.stats_overlay.lock() {
            if let Some(overlay) = current.as_ref() {
                set_property_if_supported(overlay, "visible", visible);
            }
        }
    }

    fn update_stats_overlay_text(&self, text: &str) {
        if let Ok(current) = self.stats_overlay.lock() {
            if let Some(overlay) = current.as_ref() {
                overlay.set_property("text", text);
                set_property_if_supported(
                    overlay,
                    "visible",
                    self.stats_overlay_visible.load(Ordering::Relaxed) && !text.is_empty(),
                );
            }
        }
    }

    fn record_decoded_buffer(&self) {
        self.last_decoded_ms.store(self.now_ms(), Ordering::Relaxed);
        self.decoded_total.fetch_add(1, Ordering::Relaxed);
    }

    fn record_sink_buffer(&self) {
        self.last_sink_ms.store(self.now_ms(), Ordering::Relaxed);
        self.sink_total.fetch_add(1, Ordering::Relaxed);
    }

    fn update_caps(&self, caps: &str) {
        self.zero_copy_d3d11
            .store(caps.contains("memory:D3D11Memory"), Ordering::Relaxed);
        self.zero_copy_d3d12
            .store(caps.contains("memory:D3D12Memory"), Ordering::Relaxed);
        if let Ok(mut memory_mode) = self.memory_mode.lock() {
            *memory_mode = memory_mode_from_caps(caps).to_owned();
        }
        if let Ok(mut caps_framerate) = self.caps_framerate.lock() {
            *caps_framerate = caps_framerate_summary(caps);
        }
    }

    fn zero_copy_d3d11(&self) -> bool {
        self.zero_copy_d3d11.load(Ordering::Relaxed)
    }

    fn zero_copy_d3d12(&self) -> bool {
        self.zero_copy_d3d12.load(Ordering::Relaxed)
    }

    fn memory_mode(&self) -> String {
        self.memory_mode
            .lock()
            .map(|value| value.clone())
            .unwrap_or_else(|_| "unknown".to_owned())
    }

    fn zero_copy(&self) -> bool {
        is_zero_copy_memory_mode(&self.memory_mode())
    }

    fn set_rtp_video_src_pad(&self, pad: &gst::Pad) {
        if let Ok(mut current) = self.rtp_video_src_pad.lock() {
            *current = Some(pad.clone());
        }
    }

    fn requested_fps(&self) -> Option<u32> {
        let fps = self.requested_fps.load(Ordering::Relaxed);
        (fps > 0).then_some(fps)
    }

    fn caps_framerate(&self) -> Option<String> {
        self.caps_framerate
            .lock()
            .ok()
            .and_then(|value| value.clone())
    }

    fn warn_framerate_mismatch_once(&self) -> bool {
        !self.framerate_mismatch_warned.swap(true, Ordering::Relaxed)
    }

    fn rtp_video_src_pad(&self) -> Option<gst::Pad> {
        self.rtp_video_src_pad
            .lock()
            .ok()
            .and_then(|current| current.clone())
    }

    fn queue_mode(&self) -> NativeQueueMode {
        self.transition_telemetry
            .lock()
            .map(|telemetry| telemetry.queue_mode)
            .unwrap_or(NativeQueueMode::Auto)
    }

    fn set_post_decode_queue(&self, queue: gst::Element) {
        if let Ok(mut current) = self.post_decode_queue.lock() {
            *current = Some(queue);
        }
    }

    fn set_pre_decode_queue(&self, queue: gst::Element) {
        if let Ok(mut current) = self.pre_decode_queue.lock() {
            *current = Some(queue);
        }
    }

    fn set_decoder(&self, decoder: gst::Element) {
        if let Ok(mut current) = self.decoder.lock() {
            *current = Some(decoder);
        }
    }

    fn pre_decode_queue(&self) -> Option<gst::Element> {
        self.pre_decode_queue
            .lock()
            .ok()
            .and_then(|current| current.clone())
    }

    fn decoder(&self) -> Option<gst::Element> {
        self.decoder.lock().ok().and_then(|current| current.clone())
    }

    fn set_queue_depth(
        &self,
        max_buffers: u32,
        reason: &str,
        event_sender: &Option<Sender<Event>>,
    ) {
        let queue = self
            .post_decode_queue
            .lock()
            .ok()
            .and_then(|current| current.clone());
        if let Some(queue) = queue.as_ref() {
            configure_queue(queue, max_buffers, true);
        }

        let mut should_log = false;
        if let Ok(mut telemetry) = self.transition_telemetry.lock() {
            if telemetry.queue_depth != max_buffers {
                telemetry.queue_depth = max_buffers;
                telemetry.queue_depth_changes = telemetry.queue_depth_changes.saturating_add(1);
                should_log = true;
            }
        }

        if should_log {
            send_log(
                event_sender,
                "info",
                format!("Adjusted native post-decode queue depth to {max_buffers} ({reason})."),
            );
        }
    }

    fn queue_depth(&self) -> u32 {
        self.transition_telemetry
            .lock()
            .map(|telemetry| telemetry.queue_depth)
            .unwrap_or(VIDEO_QUEUE_MAX_BUFFERS)
    }

    fn record_present_pacing_change(&self) {
        if let Ok(mut telemetry) = self.transition_telemetry.lock() {
            telemetry.present_pacing_changes = telemetry.present_pacing_changes.saturating_add(1);
        }
    }

    fn transition_flush_escalation_enabled(&self) -> bool {
        self.transition_flush_escalation_enabled
            .load(Ordering::Relaxed)
    }

    fn transition_telemetry_snapshot(&self) -> TransitionTelemetry {
        self.transition_telemetry
            .lock()
            .map(|telemetry| telemetry.clone())
            .unwrap_or_default()
    }

    fn requested_streaming_features_summary(&self) -> String {
        self.requested_streaming_features_summary
            .lock()
            .map(|value| value.clone())
            .unwrap_or_else(|_| "none".to_owned())
    }

    fn finalized_streaming_features_summary(&self) -> String {
        self.finalized_streaming_features_summary
            .lock()
            .map(|value| value.clone())
            .unwrap_or_else(|_| "none".to_owned())
    }

    fn record_transition(
        &self,
        transition_type: &str,
        source: &str,
        old_caps: Option<String>,
        new_caps: Option<String>,
        old_framerate: Option<String>,
        new_framerate: Option<String>,
        old_memory_mode: Option<String>,
        new_memory_mode: Option<String>,
        event_sender: &Option<Sender<Event>>,
    ) {
        let requested_fps = self.requested_fps();
        let queue_mode = self.queue_mode();
        let render_gap_ms = age_since_ms(self.now_ms(), self.last_sink_ms.load(Ordering::Relaxed));
        let high_fps_risk = requested_fps.is_some_and(|fps| fps >= 240)
            && new_framerate
                .as_deref()
                .is_some_and(|value| value != format!("{}/1", requested_fps.unwrap_or_default()));
        let summary = format_transition_summary(
            transition_type,
            source,
            requested_fps,
            old_framerate.as_deref(),
            new_framerate.as_deref(),
            high_fps_risk,
        );
        let snapshot = TransitionSnapshot {
            transition_type: transition_type.to_owned(),
            source: source.to_owned(),
            at_ms: self.now_ms(),
            old_caps,
            new_caps,
            old_framerate,
            new_framerate: new_framerate.clone(),
            old_memory_mode,
            new_memory_mode,
            render_gap_ms,
            requested_fps,
            caps_framerate: new_framerate,
            high_fps_risk,
            queue_mode,
            summary: summary.clone(),
        };

        if let Ok(mut telemetry) = self.transition_telemetry.lock() {
            telemetry.last_transition = Some(snapshot.clone());
        }

        send_log(
            event_sender,
            "warn",
            format!("Native video transition: {summary}"),
        );
        if let Some(event_sender) = event_sender {
            let _ = event_sender.send(Event::VideoTransition {
                transition: snapshot.to_event(),
            });
        }
    }

    fn increment_partial_flush_count(&self) {
        if let Ok(mut telemetry) = self.transition_telemetry.lock() {
            telemetry.partial_flush_count = telemetry.partial_flush_count.saturating_add(1);
        }
    }

    fn increment_complete_flush_count(&self) {
        if let Ok(mut telemetry) = self.transition_telemetry.lock() {
            telemetry.complete_flush_count = telemetry.complete_flush_count.saturating_add(1);
        }
    }
}

#[derive(Debug, Clone)]
struct VideoLivenessMonitor {
    state: Arc<VideoLivenessState>,
    stop: Arc<AtomicBool>,
    started: Arc<AtomicBool>,
    thread: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl Default for VideoLivenessMonitor {
    fn default() -> Self {
        Self {
            state: Arc::new(VideoLivenessState::new()),
            stop: Arc::new(AtomicBool::new(false)),
            started: Arc::new(AtomicBool::new(false)),
            thread: Arc::new(Mutex::new(None)),
        }
    }
}

impl VideoLivenessMonitor {
    fn configure(&self, context: &NativeStreamerSessionContext, target_bitrate_kbps: u32) {
        self.state.configure(context, target_bitrate_kbps);
    }

    fn update_hardware_acceleration(&self, value: impl Into<String>) {
        self.state.update_hardware_acceleration(value);
    }

    fn record_encoded_buffer(&self, size: usize) {
        self.state.record_encoded_buffer(size);
    }

    fn record_audio_buffer(&self) {
        self.state.record_audio_buffer();
    }

    fn set_stats_overlay(&self, overlay: Option<gst::Element>) {
        self.state.set_stats_overlay(overlay);
    }

    fn set_stats_overlay_visible(&self, visible: bool) {
        self.state.set_stats_overlay_visible(visible);
    }

    fn record_decoded_buffer(&self) {
        self.state.record_decoded_buffer();
    }

    fn record_sink_buffer(&self) {
        self.state.record_sink_buffer();
    }

    fn update_caps(&self, caps: &str) {
        self.state.update_caps(caps);
    }

    fn set_rtp_video_src_pad(&self, pad: &gst::Pad) {
        self.state.set_rtp_video_src_pad(pad);
    }

    fn set_post_decode_queue(&self, queue: gst::Element) {
        self.state.set_post_decode_queue(queue);
    }

    fn set_pre_decode_queue(&self, queue: gst::Element) {
        self.state.set_pre_decode_queue(queue);
    }

    fn set_decoder(&self, decoder: gst::Element) {
        self.state.set_decoder(decoder);
    }

    fn start(
        &self,
        pipeline: gst::Pipeline,
        sink: gst::Element,
        event_sender: Option<Sender<Event>>,
    ) {
        if self.started.swap(true, Ordering::SeqCst) {
            return;
        }

        self.stop.store(false, Ordering::SeqCst);
        let state = self.state.clone();
        let stop = self.stop.clone();
        let thread = thread::spawn(move || {
            run_video_liveness_watchdog(state, stop, pipeline, sink, event_sender);
        });
        if let Ok(mut slot) = self.thread.lock() {
            *slot = Some(thread);
        }
    }

    fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        self.started.store(false, Ordering::SeqCst);
        let handle = self.thread.lock().ok().and_then(|mut slot| slot.take());
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }
}

// gstreamer-rs exposes the generic ICE transport but not the NICE stream that
// owns remote credentials. GFN uses UUID ICE passwords, so we need the actual
// NICE stream after GStreamer's SDP parser validates a sanitized copy.
#[repr(C)]
struct GstWebRTCNiceTransportCompat {
    parent: gst_webrtc::ffi::GstWebRTCICETransport,
    stream: *mut gst_webrtc::ffi::GstWebRTCICEStream,
    _priv: glib::ffi::gpointer,
}

#[derive(Debug, Clone, Copy)]
struct ActualNiceIceStream {
    ptr: *mut gst_webrtc::ffi::GstWebRTCICEStream,
    stream_id: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecodedMediaKind {
    Audio,
    Video,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RtpVideoChainRole {
    Depayloader,
    Parser,
    PreDecodeQueue,
    Decoder,
    PostDecodeRateSetter,
    PostDecodeConverter,
    PostDecodeCapsFilter,
    StatsOverlay,
    PostDecodeQueue,
    Sink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RtpVideoApi {
    D3D11,
    D3D12,
    VideoToolbox,
    Vaapi,
    V4L2,
    Vulkan,
    Software,
}

impl RtpVideoApi {
    fn label(self) -> &'static str {
        match self {
            Self::D3D11 => "D3D11",
            Self::D3D12 => "D3D12",
            Self::VideoToolbox => "VideoToolbox",
            Self::Vaapi => "VAAPI",
            Self::V4L2 => "V4L2",
            Self::Vulkan => "Vulkan",
            Self::Software => "software",
        }
    }

    fn capability_id(self) -> &'static str {
        match self {
            Self::D3D11 => "d3d11",
            Self::D3D12 => "d3d12",
            Self::VideoToolbox => "videotoolbox",
            Self::Vaapi => "vaapi",
            Self::V4L2 => "v4l2",
            Self::Vulkan => "vulkan",
            Self::Software => "software",
        }
    }

    fn platform(self) -> &'static str {
        match self {
            Self::D3D11 | Self::D3D12 => "windows",
            Self::VideoToolbox => "macos",
            Self::Vaapi | Self::V4L2 | Self::Vulkan => "linux",
            Self::Software => "cross-platform",
        }
    }

    fn memory_caps(self) -> Option<&'static str> {
        match self {
            // D3D decoders and sinks can negotiate GPU memory directly. Keep
            // the capsfilter opt-in so startup does not fail when a live RTP
            // stream's raw caps are still settling.
            Self::D3D11 => zero_copy_requested().then_some("video/x-raw(memory:D3D11Memory)"),
            Self::D3D12 => zero_copy_requested().then_some("video/x-raw(memory:D3D12Memory)"),
            Self::VideoToolbox => zero_copy_requested().then_some("video/x-raw(memory:GLMemory)"),
            Self::Vaapi => zero_copy_requested().then_some("video/x-raw(memory:VAMemory)"),
            Self::Vulkan => Some("video/x-raw(memory:VulkanImage)"),
            _ => None,
        }
    }

    fn post_decode_converter_factory(self) -> Option<&'static str> {
        match self {
            Self::D3D11 | Self::D3D12 => None,
            Self::Vulkan => Some("vulkancolorconvert"),
            Self::VideoToolbox | Self::Vaapi if zero_copy_requested() => None,
            // Non-D3D hardware decoders are not guaranteed to negotiate directly with every
            // platform sink. Keep these paths reliable with an explicit raw-video conversion stage.
            Self::VideoToolbox | Self::Vaapi | Self::V4L2 | Self::Software => Some("videoconvert"),
        }
    }

    fn stats_overlay_factory(self) -> Option<&'static str> {
        match self {
            Self::D3D11 | Self::D3D12 => Some("dwritetextoverlay"),
            _ => None,
        }
    }

    fn sink_factory(self) -> &'static str {
        match self {
            Self::D3D11 => "d3d11videosink",
            Self::D3D12 => "d3d12videosink",
            Self::VideoToolbox => "glimagesink",
            Self::Vaapi => "glimagesink",
            Self::V4L2 => "glimagesink",
            Self::Vulkan => "vulkansink",
            Self::Software => "autovideosink",
        }
    }

    fn decoder_factory(self, codec: &str) -> Option<&'static str> {
        match (self, codec) {
            (Self::D3D11, "H265" | "HEVC") => Some("d3d11h265dec"),
            (Self::D3D11, "H264") => Some("d3d11h264dec"),
            (Self::D3D11, "AV1") => Some("d3d11av1dec"),
            (Self::D3D12, "H265" | "HEVC") => Some("d3d12h265dec"),
            (Self::D3D12, "H264") => Some("d3d12h264dec"),
            (Self::D3D12, "AV1") => Some("d3d12av1dec"),
            (Self::VideoToolbox, "H265" | "HEVC" | "H264") => Some("vtdec_hw"),
            (Self::Vaapi, "H265" | "HEVC") => Some("vah265dec"),
            (Self::Vaapi, "H264") => Some("vah264dec"),
            (Self::Vaapi, "AV1") => Some("vaav1dec"),
            (Self::V4L2, "H265" | "HEVC") => Some("v4l2slh265dec"),
            (Self::V4L2, "H264") => Some("v4l2slh264dec"),
            (Self::Vulkan, "H265" | "HEVC") => Some("vulkanh265dec"),
            (Self::Vulkan, "H264") => Some("vulkanh264dec"),
            (Self::Software, "H265" | "HEVC") => Some("avdec_h265"),
            (Self::Software, "H264") => Some("avdec_h264"),
            (Self::Software, "AV1") => Some("avdec_av1"),
            _ => None,
        }
    }

    fn fallback_decoder_factories(self, codec: &str) -> &'static [&'static str] {
        match (self, codec) {
            (Self::Vaapi, "H265" | "HEVC") => &["vaapih265dec"],
            (Self::Vaapi, "H264") => &["vaapih264dec"],
            (Self::Vaapi, "AV1") => &["vaapiav1dec"],
            (Self::V4L2, "H265" | "HEVC") => &["v4l2h265dec"],
            (Self::V4L2, "H264") => &["v4l2h264dec"],
            (Self::VideoToolbox, "H265" | "HEVC" | "H264") => &["vtdec"],
            _ => &[],
        }
    }

    fn sink_fallback_factories(self) -> &'static [&'static str] {
        match self {
            Self::VideoToolbox => &["osxvideosink", "autovideosink"],
            Self::Vaapi | Self::V4L2 => {
                &["waylandsink", "ximagesink", "xvimagesink", "autovideosink"]
            }
            Self::Software => &["glimagesink", "waylandsink", "ximagesink", "xvimagesink"],
            _ => &[],
        }
    }

    fn is_gpu_path(self) -> bool {
        !matches!(self, Self::Software)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RtpVideoChainSpec {
    factory: &'static str,
    role: RtpVideoChainRole,
    caps: Option<String>,
}

impl RtpVideoChainSpec {
    fn new(factory: &'static str, role: RtpVideoChainRole) -> Self {
        Self {
            factory,
            role,
            caps: None,
        }
    }

    fn with_caps(factory: &'static str, role: RtpVideoChainRole, caps: impl Into<String>) -> Self {
        Self {
            factory,
            role,
            caps: Some(caps.into()),
        }
    }
}

#[derive(Clone)]
struct GstreamerInputState {
    encoder: Arc<Mutex<InputEncoder>>,
    ready: Arc<AtomicBool>,
    heartbeat_stop: Arc<AtomicBool>,
    heartbeat_thread: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl std::fmt::Debug for GstreamerInputState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GstreamerInputState")
            .field("ready", &self.ready.load(Ordering::SeqCst))
            .finish_non_exhaustive()
    }
}

impl Default for GstreamerInputState {
    fn default() -> Self {
        Self {
            encoder: Arc::new(Mutex::new(InputEncoder::default())),
            ready: Arc::new(AtomicBool::new(false)),
            heartbeat_stop: Arc::new(AtomicBool::new(false)),
            heartbeat_thread: Arc::new(Mutex::new(None)),
        }
    }
}

impl GstreamerInputState {
    fn reset(&self) {
        self.ready.store(false, Ordering::SeqCst);
        if let Ok(mut encoder) = self.encoder.lock() {
            encoder.set_protocol_version(2);
            encoder.reset_gamepad_sequences();
        }
    }

    fn stop_heartbeat(&self) {
        self.heartbeat_stop.store(true, Ordering::SeqCst);
        let Some(handle) = self
            .heartbeat_thread
            .lock()
            .ok()
            .and_then(|mut thread| thread.take())
        else {
            return;
        };

        if let Err(error) = handle.join() {
            eprintln!("[NativeStreamer] Input heartbeat thread panicked: {error:?}");
        }
    }
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, Copy)]
enum NativeWindowInputEvent {
    Key {
        pressed: bool,
        keycode: u16,
        scancode: u16,
        modifiers: u16,
        timestamp_us: u64,
    },
    MouseMove {
        dx: i16,
        dy: i16,
        timestamp_us: u64,
    },
    MouseButton {
        pressed: bool,
        button: u8,
        timestamp_us: u64,
    },
    MouseWheel {
        delta: i16,
        timestamp_us: u64,
    },
}

#[cfg(target_os = "windows")]
mod win32_xinput {
    use std::ffi::{c_char, c_void};

    type Dword = u32;
    type Hmodule = *mut c_void;
    type XInputGetStateFn = unsafe extern "system" fn(Dword, *mut XInputStateRaw) -> Dword;

    const ERROR_SUCCESS: Dword = 0;
    const XINPUT_DLLS: [&str; 3] = ["xinput1_4.dll", "xinput9_1_0.dll", "xinput1_3.dll"];

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct XInputGamepadRaw {
        buttons: u16,
        left_trigger: u8,
        right_trigger: u8,
        thumb_lx: i16,
        thumb_ly: i16,
        thumb_rx: i16,
        thumb_ry: i16,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct XInputStateRaw {
        packet_number: Dword,
        gamepad: XInputGamepadRaw,
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct XInputGamepadSnapshot {
        pub buttons: u16,
        pub left_trigger: u8,
        pub right_trigger: u8,
        pub left_stick_x: i16,
        pub left_stick_y: i16,
        pub right_stick_x: i16,
        pub right_stick_y: i16,
    }

    #[derive(Clone, Copy)]
    pub struct XInput {
        get_state: XInputGetStateFn,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetProcAddress(module: Hmodule, proc_name: *const c_char) -> *mut c_void;
        fn LoadLibraryW(filename: *const u16) -> Hmodule;
    }

    impl XInput {
        pub unsafe fn load() -> Option<Self> {
            for dll in XINPUT_DLLS {
                let wide = wide_null(dll);
                let module = LoadLibraryW(wide.as_ptr());
                if module.is_null() {
                    continue;
                }

                let address = GetProcAddress(module, b"XInputGetState\0".as_ptr() as *const c_char);
                if !address.is_null() {
                    return Some(Self {
                        get_state: std::mem::transmute::<*mut c_void, XInputGetStateFn>(address),
                    });
                }
            }

            None
        }

        pub unsafe fn get_state(self, controller_id: u32) -> Option<XInputGamepadSnapshot> {
            let mut state = XInputStateRaw::default();
            if (self.get_state)(controller_id, &mut state) != ERROR_SUCCESS {
                return None;
            }

            Some(XInputGamepadSnapshot {
                buttons: state.gamepad.buttons,
                left_trigger: apply_trigger_deadzone(state.gamepad.left_trigger),
                right_trigger: apply_trigger_deadzone(state.gamepad.right_trigger),
                left_stick_x: apply_stick_deadzone(state.gamepad.thumb_lx, 7849),
                left_stick_y: apply_stick_deadzone(state.gamepad.thumb_ly, 7849),
                right_stick_x: apply_stick_deadzone(state.gamepad.thumb_rx, 8689),
                right_stick_y: apply_stick_deadzone(state.gamepad.thumb_ry, 8689),
            })
        }
    }

    fn wide_null(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn apply_trigger_deadzone(value: u8) -> u8 {
        if value <= 30 {
            0
        } else {
            value
        }
    }

    fn apply_stick_deadzone(value: i16, deadzone: i16) -> i16 {
        if (value as i32).abs() <= deadzone as i32 {
            0
        } else {
            value
        }
    }
}

#[derive(Clone, Debug)]
struct GstreamerInputChannels {
    reliable: gst_webrtc::WebRTCDataChannel,
    partially_reliable: gst_webrtc::WebRTCDataChannel,
}

impl GstreamerInputChannels {
    fn labels(&self) -> (String, String) {
        (
            channel_label(&self.reliable),
            channel_label(&self.partially_reliable),
        )
    }

    fn send_packet(&self, payload: &[u8], partially_reliable: bool) -> bool {
        if payload.is_empty() {
            return false;
        }

        let channel = if partially_reliable {
            if self.partially_reliable.ready_state() != gst_webrtc::WebRTCDataChannelState::Open {
                return false;
            }
            &self.partially_reliable
        } else {
            &self.reliable
        };

        if channel.ready_state() != gst_webrtc::WebRTCDataChannelState::Open {
            return false;
        }

        let bytes = glib::Bytes::from_owned(payload.to_vec());
        channel.send_data_full(Some(&bytes)).is_ok()
    }
}

#[cfg(target_os = "windows")]
#[derive(Debug)]
struct NativeWindowInputBridge {
    stop: Arc<AtomicBool>,
    input_thread: Option<JoinHandle<()>>,
    gamepad_thread: Option<JoinHandle<()>>,
}

#[cfg(target_os = "windows")]
impl NativeWindowInputBridge {
    fn start(
        input_state: GstreamerInputState,
        input_channels: GstreamerInputChannels,
        event_sender: Option<Sender<Event>>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel::<NativeWindowInputEvent>();
        unsafe {
            win32_renderer_window::set_input_event_sender(Some(sender));
        }

        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let thread_sender = event_sender.clone();
        let input_thread_state = input_state.clone();
        let input_thread_channels = input_channels.clone();
        let input_thread = thread::spawn(move || {
            let mut pending_events = Vec::with_capacity(NATIVE_INPUT_DRAIN_MAX_EVENTS);
            send_log(
                &thread_sender,
                "info",
                "Native DX11 window input capture bridge armed.".to_owned(),
            );

            while !thread_stop.load(Ordering::SeqCst) {
                match receiver.recv_timeout(NATIVE_INPUT_BRIDGE_POLL_INTERVAL) {
                    Ok(event) => {
                        pending_events.clear();
                        pending_events.push(event);
                        let mut disconnected = false;
                        while pending_events.len() < NATIVE_INPUT_DRAIN_MAX_EVENTS {
                            match receiver.try_recv() {
                                Ok(event) => pending_events.push(event),
                                Err(TryRecvError::Empty) => break,
                                Err(TryRecvError::Disconnected) => {
                                    disconnected = true;
                                    break;
                                }
                            }
                        }
                        send_native_window_input_events(
                            &input_thread_state,
                            &input_thread_channels,
                            &pending_events,
                        );
                        if disconnected {
                            break;
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
        });
        let gamepad_thread = Some(spawn_native_gamepad_thread(
            input_state,
            input_channels,
            event_sender,
            stop.clone(),
        ));

        Self {
            stop,
            input_thread: Some(input_thread),
            gamepad_thread,
        }
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        unsafe {
            win32_renderer_window::release_current_input_capture();
            win32_renderer_window::set_input_event_sender(None);
        }

        if let Some(thread) = self.input_thread.take() {
            if let Err(error) = thread.join() {
                eprintln!("[NativeStreamer] Native window input bridge thread panicked: {error:?}");
            }
        }
        if let Some(thread) = self.gamepad_thread.take() {
            if let Err(error) = thread.join() {
                eprintln!("[NativeStreamer] Native XInput gamepad thread panicked: {error:?}");
            }
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for NativeWindowInputBridge {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(target_os = "windows")]
fn send_native_window_input_events(
    input_state: &GstreamerInputState,
    input_channels: &GstreamerInputChannels,
    events: &[NativeWindowInputEvent],
) {
    if events.is_empty() || !input_state.ready.load(Ordering::SeqCst) {
        return;
    }

    let Ok(encoder) = input_state.encoder.lock() else {
        return;
    };

    let mut pending_mouse_move: Option<(i32, i32, u64)> = None;
    for event in events.iter().copied() {
        if let NativeWindowInputEvent::MouseMove {
            dx,
            dy,
            timestamp_us,
        } = event
        {
            let (pending_dx, pending_dy, pending_timestamp_us) =
                pending_mouse_move.get_or_insert((0, 0, timestamp_us));
            *pending_dx = pending_dx.saturating_add(i32::from(dx));
            *pending_dy = pending_dy.saturating_add(i32::from(dy));
            *pending_timestamp_us = timestamp_us;
            continue;
        }

        flush_pending_mouse_move(&encoder, input_channels, &mut pending_mouse_move);
        send_encoded_native_window_input_event(&encoder, input_channels, event);
    }
    flush_pending_mouse_move(&encoder, input_channels, &mut pending_mouse_move);
}

#[cfg(target_os = "windows")]
fn flush_pending_mouse_move(
    encoder: &InputEncoder,
    input_channels: &GstreamerInputChannels,
    pending_mouse_move: &mut Option<(i32, i32, u64)>,
) {
    let Some((mut dx, mut dy, timestamp_us)) = pending_mouse_move.take() else {
        return;
    };

    while dx != 0 || dy != 0 {
        let chunk_dx = dx.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
        let chunk_dy = dy.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
        let payload = encoder.encode_mouse_move(MouseMovePayload {
            dx: chunk_dx,
            dy: chunk_dy,
            timestamp_us,
        });
        let _ = input_channels.send_packet(&payload, true);
        dx = dx.saturating_sub(i32::from(chunk_dx));
        dy = dy.saturating_sub(i32::from(chunk_dy));
    }
}

#[cfg(target_os = "windows")]
fn send_encoded_native_window_input_event(
    encoder: &InputEncoder,
    input_channels: &GstreamerInputChannels,
    event: NativeWindowInputEvent,
) {
    let (payload, partially_reliable) = match event {
        NativeWindowInputEvent::Key {
            pressed,
            keycode,
            scancode,
            modifiers,
            timestamp_us,
        } => {
            let payload = KeyboardPayload {
                keycode,
                scancode,
                modifiers,
                timestamp_us,
            };
            let bytes = if pressed {
                encoder.encode_key_down(payload)
            } else {
                encoder.encode_key_up(payload)
            };
            (bytes, false)
        }
        NativeWindowInputEvent::MouseMove {
            dx,
            dy,
            timestamp_us,
        } => (
            encoder.encode_mouse_move(MouseMovePayload {
                dx,
                dy,
                timestamp_us,
            }),
            true,
        ),
        NativeWindowInputEvent::MouseButton {
            pressed,
            button,
            timestamp_us,
        } => {
            let payload = MouseButtonPayload {
                button,
                timestamp_us,
            };
            let bytes = if pressed {
                encoder.encode_mouse_button_down(payload)
            } else {
                encoder.encode_mouse_button_up(payload)
            };
            (bytes, false)
        }
        NativeWindowInputEvent::MouseWheel {
            delta,
            timestamp_us,
        } => (
            encoder.encode_mouse_wheel(MouseWheelPayload {
                delta,
                timestamp_us,
            }),
            false,
        ),
    };

    let _ = input_channels.send_packet(&payload, partially_reliable);
}

#[cfg(target_os = "windows")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct NativeGamepadSnapshot {
    connected: bool,
    buttons: u16,
    left_trigger: u8,
    right_trigger: u8,
    left_stick_x: i16,
    left_stick_y: i16,
    right_stick_x: i16,
    right_stick_y: i16,
}

#[cfg(target_os = "windows")]
impl NativeGamepadSnapshot {
    fn from_xinput(snapshot: win32_xinput::XInputGamepadSnapshot) -> Self {
        Self {
            connected: true,
            buttons: snapshot.buttons,
            left_trigger: snapshot.left_trigger,
            right_trigger: snapshot.right_trigger,
            left_stick_x: snapshot.left_stick_x,
            left_stick_y: snapshot.left_stick_y,
            right_stick_x: snapshot.right_stick_x,
            right_stick_y: snapshot.right_stick_y,
        }
    }
}

#[cfg(target_os = "windows")]
fn spawn_native_gamepad_thread(
    input_state: GstreamerInputState,
    input_channels: GstreamerInputChannels,
    event_sender: Option<Sender<Event>>,
    stop: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let Some(xinput) = (unsafe { win32_xinput::XInput::load() }) else {
            send_log(
                &event_sender,
                "warn",
                "Native XInput gamepad bridge unavailable; controller input will require the web renderer fallback.".to_owned(),
            );
            return;
        };

        send_log(
            &event_sender,
            "info",
            "Native XInput gamepad bridge armed.".to_owned(),
        );

        let mut previous = [NativeGamepadSnapshot::default(); GAMEPAD_MAX_CONTROLLERS as usize];
        let mut last_sent = [Instant::now(); GAMEPAD_MAX_CONTROLLERS as usize];

        while !stop.load(Ordering::SeqCst) {
            if input_state.ready.load(Ordering::SeqCst) {
                let mut snapshots =
                    [NativeGamepadSnapshot::default(); GAMEPAD_MAX_CONTROLLERS as usize];
                let mut bitmap = 0u16;

                for controller_id in 0..GAMEPAD_MAX_CONTROLLERS as usize {
                    if let Some(snapshot) = unsafe { xinput.get_state(controller_id as u32) } {
                        snapshots[controller_id] = NativeGamepadSnapshot::from_xinput(snapshot);
                        bitmap |= 1 << controller_id;
                    }
                }

                for controller_id in 0..GAMEPAD_MAX_CONTROLLERS as usize {
                    let snapshot = snapshots[controller_id];
                    let state_changed = snapshot != previous[controller_id];
                    let keepalive_due = snapshot.connected
                        && last_sent[controller_id].elapsed() >= NATIVE_GAMEPAD_KEEPALIVE_INTERVAL;

                    if state_changed || keepalive_due {
                        send_native_gamepad_snapshot(
                            &input_state,
                            &input_channels,
                            controller_id as u8,
                            bitmap,
                            snapshot,
                        );
                        last_sent[controller_id] = Instant::now();

                        if snapshot.connected != previous[controller_id].connected {
                            send_log(
                                &event_sender,
                                "info",
                                format!(
                                    "Native XInput controller {controller_id} {}.",
                                    if snapshot.connected {
                                        "connected"
                                    } else {
                                        "disconnected"
                                    }
                                ),
                            );
                        }
                    }

                    previous[controller_id] = snapshot;
                }
            }

            thread::sleep(NATIVE_GAMEPAD_POLL_INTERVAL);
        }
    })
}

#[cfg(target_os = "windows")]
fn send_native_gamepad_snapshot(
    input_state: &GstreamerInputState,
    input_channels: &GstreamerInputChannels,
    controller_id: u8,
    bitmap: u16,
    snapshot: NativeGamepadSnapshot,
) {
    if !input_state.ready.load(Ordering::SeqCst) {
        return;
    }

    let use_partially_reliable =
        (PARTIALLY_RELIABLE_GAMEPAD_MASK_ALL & (1_u32 << u32::from(controller_id))) != 0;
    let input = GamepadInput {
        controller_id,
        buttons: snapshot.buttons,
        left_trigger: snapshot.left_trigger,
        right_trigger: snapshot.right_trigger,
        left_stick_x: snapshot.left_stick_x,
        left_stick_y: snapshot.left_stick_y,
        right_stick_x: snapshot.right_stick_x,
        right_stick_y: snapshot.right_stick_y,
        connected: snapshot.connected,
        timestamp_us: native_input_timestamp_us(),
    };

    let Ok(mut encoder) = input_state.encoder.lock() else {
        return;
    };
    let payload = encoder.encode_gamepad_state(bitmap, input, use_partially_reliable);
    drop(encoder);

    let _ = input_channels.send_packet(&payload, use_partially_reliable);
}

#[cfg(target_os = "windows")]
fn native_input_timestamp_us() -> u64 {
    NATIVE_INPUT_STARTED_AT
        .get_or_init(Instant::now)
        .elapsed()
        .as_micros()
        .min(u128::from(u64::MAX)) as u64
}

#[derive(Clone, Debug, Default)]
struct GstreamerRenderState {
    surface: Arc<Mutex<Option<NativeRenderSurface>>>,
    video_sink: Arc<Mutex<Option<gst::Element>>>,
    external_renderer_logged: Arc<AtomicBool>,
    external_window_guard_started: Arc<AtomicBool>,
    external_window_guard_stop: Arc<AtomicBool>,
}

impl GstreamerRenderState {
    fn set_surface(&self, surface: NativeRenderSurface, event_sender: &Option<Sender<Event>>) {
        if let Ok(mut current) = self.surface.lock() {
            *current = Some(surface);
        }
        self.apply(event_sender);
    }

    fn set_video_sink(&self, sink: gst::Element, event_sender: &Option<Sender<Event>>) {
        if let Ok(mut current) = self.video_sink.lock() {
            *current = Some(sink);
        }
        self.apply(event_sender);
    }

    fn apply(&self, event_sender: &Option<Sender<Event>>) {
        let sink = self.video_sink.lock().ok().and_then(|sink| sink.clone());
        let Some(sink) = sink else {
            return;
        };

        if use_external_renderer_window() {
            if !self
                .external_window_guard_started
                .swap(true, Ordering::SeqCst)
            {
                self.external_window_guard_stop
                    .store(false, Ordering::SeqCst);
                start_external_renderer_window_guard(
                    event_sender.clone(),
                    self.external_window_guard_stop.clone(),
                );
            }
            if !self.external_renderer_logged.swap(true, Ordering::SeqCst) {
                send_log(
                    event_sender,
                    "info",
                    format!(
                        "Using external native GStreamer renderer window; set {EXTERNAL_RENDERER_ENV}=0 to retry Electron HWND embedding."
                    ),
                );
            }
            return;
        }

        let surface = self.surface.lock().ok().and_then(|surface| surface.clone());
        let Some(surface) = surface else {
            return;
        };

        if let Err(message) = apply_render_surface_to_video_sink(&sink, &surface) {
            send_log(event_sender, "warn", message);
        }
    }

    fn stop_external_renderer_window_guard(&self) {
        self.external_window_guard_stop
            .store(true, Ordering::SeqCst);
        self.external_window_guard_started
            .store(false, Ordering::SeqCst);
    }
}

#[derive(Debug)]
struct GstreamerPipeline {
    pipeline: gst::Pipeline,
    webrtc: gst::Element,
    input_state: GstreamerInputState,
    input_channels: Option<GstreamerInputChannels>,
    #[cfg(target_os = "windows")]
    native_window_input_bridge: Option<NativeWindowInputBridge>,
    render_state: GstreamerRenderState,
    present_max_fps: Arc<AtomicU32>,
    d3d_fullscreen_sink: Arc<AtomicBool>,
    video_liveness: VideoLivenessMonitor,
    event_sender: Option<Sender<Event>>,
    original_remote_ice_credentials: Option<IceCredentials>,
    original_remote_ice_credentials_restored: bool,
}

impl GstreamerPipeline {
    fn build(event_sender: Option<Sender<Event>>) -> Result<Self, String> {
        init_gstreamer()?;

        let pipeline = gst::Pipeline::new();
        let webrtc = gst::ElementFactory::make("webrtcbin")
            .name("opennow-webrtcbin")
            .property_from_str("bundle-policy", "max-bundle")
            .build()
            .map_err(|error| format!("Failed to create webrtcbin: {error}"))?;
        configure_webrtc_low_latency(&webrtc);

        let input_state = GstreamerInputState::default();
        let render_state = GstreamerRenderState::default();
        let video_liveness = VideoLivenessMonitor::default();
        wire_local_ice_events(&webrtc, event_sender.clone())?;
        wire_webrtc_state_events(&webrtc, event_sender.clone());
        wire_remote_data_channels(&webrtc, event_sender.clone());
        start_gstreamer_bus_diagnostics(
            &pipeline,
            event_sender.clone(),
            video_liveness.stop.clone(),
            video_liveness.clone(),
        );
        let present_max_fps = Arc::new(AtomicU32::new(0));
        let d3d_fullscreen_sink = Arc::new(AtomicBool::new(false));
        wire_incoming_media_sink(
            &pipeline,
            &webrtc,
            event_sender.clone(),
            render_state.clone(),
            present_max_fps.clone(),
            d3d_fullscreen_sink.clone(),
            video_liveness.clone(),
        );

        pipeline
            .add(&webrtc)
            .map_err(|error| format!("Failed to add webrtcbin to pipeline: {error}"))?;
        pipeline
            .set_state(gst::State::Ready)
            .map_err(|error| format!("Failed to set GStreamer pipeline to Ready: {error:?}"))?;

        Ok(Self {
            pipeline,
            webrtc,
            input_state,
            input_channels: None,
            #[cfg(target_os = "windows")]
            native_window_input_bridge: None,
            render_state,
            present_max_fps,
            d3d_fullscreen_sink,
            video_liveness,
            event_sender,
            original_remote_ice_credentials: None,
            original_remote_ice_credentials_restored: false,
        })
    }

    fn parse_offer_sdp(sdp: &str) -> Result<gst_sdp::SDPMessage, String> {
        init_gstreamer()?;
        gst_sdp::SDPMessage::parse_buffer(sdp.as_bytes())
            .map_err(|error| format!("GStreamer rejected the remote SDP offer: {error:?}"))
    }

    fn webrtc_name(&self) -> String {
        self.webrtc.name().to_string()
    }

    fn set_present_max_fps(&self, fps: u32) {
        self.present_max_fps.store(fps, Ordering::SeqCst);
    }

    fn set_d3d_fullscreen_sink(&self, enabled: bool) {
        self.d3d_fullscreen_sink.store(enabled, Ordering::SeqCst);
    }

    fn configure_stats(&self, context: &NativeStreamerSessionContext, target_bitrate_kbps: u32) {
        self.video_liveness.configure(context, target_bitrate_kbps);
    }

    fn ensure_input_data_channels(
        &mut self,
        partial_reliable_threshold_ms: u32,
    ) -> Result<(), String> {
        if self.input_channels.is_some() {
            return Ok(());
        }

        self.input_state.reset();
        let channels = create_input_data_channels(
            &self.webrtc,
            self.input_state.clone(),
            self.event_sender.clone(),
            partial_reliable_threshold_ms,
        )?;
        let _ = channels.labels();
        self.input_channels = Some(channels);
        self.ensure_native_window_input_bridge();
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn ensure_native_window_input_bridge(&mut self) {
        if self.native_window_input_bridge.is_some() {
            return;
        }
        let Some(input_channels) = self.input_channels.clone() else {
            return;
        };

        self.native_window_input_bridge = Some(NativeWindowInputBridge::start(
            self.input_state.clone(),
            input_channels,
            self.event_sender.clone(),
        ));
    }

    #[cfg(not(target_os = "windows"))]
    fn ensure_native_window_input_bridge(&mut self) {
        send_log(
            &self.event_sender,
            "warn",
            format!(
                "Native OS-level input capture is not implemented for {}; Electron input forwarding remains active.",
                std::env::consts::OS
            ),
        );
    }

    fn negotiate_answer(
        &mut self,
        offer_sdp: gst_sdp::SDPMessage,
        original_remote_credentials: Option<&IceCredentials>,
        partial_reliable_threshold_ms: u32,
    ) -> Result<String, String> {
        let offer =
            gst_webrtc::WebRTCSessionDescription::new(gst_webrtc::WebRTCSDPType::Offer, offer_sdp);
        self.pipeline
            .set_state(gst::State::Playing)
            .map_err(|error| {
                format!("Failed to set GStreamer pipeline to Playing before negotiation: {error:?}")
            })?;
        self.set_description("set-remote-description", &offer)?;
        if let Some(credentials) = original_remote_credentials {
            self.original_remote_ice_credentials = Some(credentials.clone());
            self.try_restore_original_remote_ice_credentials("after remote description")?;
        }
        self.ensure_input_data_channels(partial_reliable_threshold_ms)?;
        let answer = self.create_answer()?;
        let answer_sdp = answer
            .sdp()
            .as_text()
            .map_err(|error| format!("Failed to serialize GStreamer answer SDP: {error}"))?;
        self.set_description("set-local-description", &answer)?;
        self.try_restore_original_remote_ice_credentials("after local description")?;
        Ok(answer_sdp)
    }

    fn try_restore_original_remote_ice_credentials(&mut self, stage: &str) -> Result<bool, String> {
        if self.original_remote_ice_credentials_restored {
            return Ok(true);
        }

        let Some(credentials) = self.original_remote_ice_credentials.clone() else {
            return Ok(false);
        };

        if credentials.ufrag.is_empty() || credentials.pwd.is_empty() {
            return Err(
                "Cannot restore original remote ICE credentials: offer credentials are empty."
                    .to_owned(),
            );
        }

        let Some(ice_agent) = self
            .webrtc
            .property::<Option<gst_webrtc::WebRTCICE>>("ice-agent")
        else {
            return Err(
                "Cannot restore original remote ICE credentials: webrtcbin has no ICE agent."
                    .to_owned(),
            );
        };
        let ice_agent_ptr = ice_agent.as_ptr() as *mut gst_webrtc::ffi::GstWebRTCICE;
        let ufrag = CString::new(credentials.ufrag.as_str())
            .map_err(|_| "Cannot restore original remote ICE credentials: ufrag contains NUL.")?;
        let pwd = CString::new(credentials.pwd.as_str())
            .map_err(|_| "Cannot restore original remote ICE credentials: pwd contains NUL.")?;

        let streams = self.negotiated_nice_streams();
        if streams.is_empty() {
            send_log(
                &self.event_sender,
                "warn",
                format!(
                    "GStreamer has not exposed actual NICE ICE streams {stage}; deferring GFN remote ICE credential restoration."
                ),
            );
            return Ok(false);
        }

        let mut restored = 0usize;
        let stream_ids = streams
            .iter()
            .map(|stream| stream.stream_id)
            .collect::<Vec<_>>();
        for stream in &streams {
            let accepted = unsafe {
                gst_webrtc::ffi::gst_webrtc_ice_set_remote_credentials(
                    ice_agent_ptr,
                    stream.ptr,
                    ufrag.as_ptr(),
                    pwd.as_ptr(),
                ) != glib::ffi::GFALSE
            };
            if accepted {
                restored += 1;
            } else {
                send_log(
                    &self.event_sender,
                    "warn",
                    format!(
                        "GStreamer ICE agent rejected original remote credentials for actual stream {}.",
                        stream.stream_id
                    ),
                );
            }
        }

        if restored == 0 {
            send_log(
                &self.event_sender,
                "warn",
                format!(
                    "GStreamer rejected original GFN remote ICE credentials on all actual streams {stage}; ICE may fail."
                ),
            );
            return Ok(false);
        }

        self.original_remote_ice_credentials_restored = true;
        send_log(
            &self.event_sender,
            "info",
            format!(
                "Restored original GFN remote ICE credentials on {restored}/{} actual GStreamer NICE ICE stream(s) {stage}; streamIds={stream_ids:?}.",
                streams.len()
            ),
        );
        Ok(true)
    }

    fn negotiated_nice_streams(&self) -> Vec<ActualNiceIceStream> {
        let mut streams = Vec::new();
        let mut seen_stream_pointers = HashSet::new();
        let mut seen_transport_summaries = Vec::new();
        for index in 0..8 {
            let transceiver = self
                .webrtc
                .emit_by_name::<Option<gst_webrtc::WebRTCRTPTransceiver>>(
                    "get-transceiver",
                    &[&(index as i32)],
                );
            let Some(transceiver) = transceiver else {
                continue;
            };

            if let Some(receiver) = transceiver.receiver() {
                if let Some(transport) = receiver.transport() {
                    self.collect_nice_stream_from_dtls_transport(
                        &transport,
                        index,
                        "receiver",
                        &mut streams,
                        &mut seen_stream_pointers,
                        &mut seen_transport_summaries,
                    );
                }
            }
            if let Some(sender) = transceiver.sender() {
                if let Some(transport) = sender.transport() {
                    self.collect_nice_stream_from_dtls_transport(
                        &transport,
                        index,
                        "sender",
                        &mut streams,
                        &mut seen_stream_pointers,
                        &mut seen_transport_summaries,
                    );
                }
            }
        }

        if !seen_transport_summaries.is_empty() {
            send_log(
                &self.event_sender,
                "debug",
                format!(
                    "GStreamer negotiated ICE transports: {}.",
                    seen_transport_summaries.join(", ")
                ),
            );
        }
        streams
    }

    fn collect_nice_stream_from_dtls_transport(
        &self,
        dtls_transport: &gst_webrtc::WebRTCDTLSTransport,
        transceiver_index: u32,
        direction: &str,
        streams: &mut Vec<ActualNiceIceStream>,
        seen_stream_pointers: &mut HashSet<usize>,
        seen_transport_summaries: &mut Vec<String>,
    ) {
        let session_id = dtls_transport.session_id();
        let Some(ice_transport) = dtls_transport.transport() else {
            seen_transport_summaries.push(format!(
                "transceiver {transceiver_index} {direction} dtlsSession={session_id} iceTransport=none"
            ));
            return;
        };

        let transport_type = ice_transport.type_().name().to_owned();
        let component = ice_transport.component();
        let state = ice_transport.state();
        let Some(stream) = nice_stream_from_ice_transport(&ice_transport) else {
            seen_transport_summaries.push(format!(
                "transceiver {transceiver_index} {direction} dtlsSession={session_id} iceTransportType={transport_type} component={component:?} state={state:?} stream=none"
            ));
            return;
        };

        seen_transport_summaries.push(format!(
            "transceiver {transceiver_index} {direction} dtlsSession={session_id} iceTransportType={transport_type} component={component:?} state={state:?} streamId={}",
            stream.stream_id
        ));

        let stream_pointer = stream.ptr as usize;
        if seen_stream_pointers.insert(stream_pointer) {
            streams.push(stream);
        }
    }

    fn set_description(
        &self,
        signal_name: &'static str,
        description: &gst_webrtc::WebRTCSessionDescription,
    ) -> Result<(), String> {
        let promise = gst::Promise::new();
        self.webrtc
            .emit_by_name::<()>(signal_name, &[description, &promise]);
        wait_for_promise(&promise, signal_name)
    }

    fn create_answer(&self) -> Result<gst_webrtc::WebRTCSessionDescription, String> {
        let promise = gst::Promise::new();
        self.webrtc
            .emit_by_name::<()>("create-answer", &[&None::<gst::Structure>, &promise]);
        wait_for_promise(&promise, "create-answer")?;
        let reply = promise
            .get_reply()
            .ok_or_else(|| "GStreamer create-answer resolved without a reply.".to_owned())?;
        reply
            .get::<gst_webrtc::WebRTCSessionDescription>("answer")
            .map_err(|error| {
                format!(
                    "GStreamer create-answer reply did not contain an answer: {error}; reply={}",
                    describe_structure(reply)
                )
            })
    }

    fn add_remote_ice(&mut self, candidate: &IceCandidatePayload) -> Result<(), String> {
        if candidate.candidate.trim().is_empty() {
            return Err("Remote ICE candidate is empty.".to_owned());
        }
        self.try_restore_original_remote_ice_credentials("before adding remote ICE candidate")?;
        let sdp_m_line_index = candidate.sdp_m_line_index.unwrap_or(0);
        self.webrtc.emit_by_name::<()>(
            "add-ice-candidate",
            &[&sdp_m_line_index, &candidate.candidate],
        );
        Ok(())
    }

    fn send_input_packet(&self, payload: &[u8], partially_reliable: bool) -> bool {
        if !self.input_state.ready.load(Ordering::SeqCst) {
            return false;
        }

        let Some(input_channels) = &self.input_channels else {
            return false;
        };

        input_channels.send_packet(payload, partially_reliable)
    }

    fn update_render_surface(&self, surface: NativeRenderSurface) {
        self.video_liveness
            .set_stats_overlay_visible(surface.visible && surface.show_stats);
        self.render_state.set_surface(surface, &self.event_sender);
    }

    fn stop(mut self) -> Result<(), String> {
        self.video_liveness.set_stats_overlay_visible(false);
        self.render_state.stop_external_renderer_window_guard();
        #[cfg(target_os = "windows")]
        if let Some(mut bridge) = self.native_window_input_bridge.take() {
            bridge.stop();
        }
        self.input_state.stop_heartbeat();
        self.video_liveness.stop();
        self.pipeline
            .set_state(gst::State::Null)
            .map(|_| ())
            .map_err(|error| format!("Failed to stop GStreamer pipeline: {error:?}"))
    }
}

fn nice_stream_from_ice_transport(
    transport: &gst_webrtc::WebRTCICETransport,
) -> Option<ActualNiceIceStream> {
    if transport.type_().name() != "GstWebRTCNiceTransport" {
        return None;
    }

    unsafe {
        let transport_ptr = transport.as_ptr() as *mut GstWebRTCNiceTransportCompat;
        if transport_ptr.is_null() {
            return None;
        }

        let stream_ptr = (*transport_ptr).stream;
        if stream_ptr.is_null() {
            return None;
        }

        Some(ActualNiceIceStream {
            ptr: stream_ptr,
            stream_id: (*stream_ptr).stream_id,
        })
    }
}

fn init_gstreamer() -> Result<(), String> {
    gst::init().map_err(|error| format!("Failed to initialize GStreamer: {error}"))
}

fn set_property_if_supported<T: Into<glib::Value>>(element: &gst::Element, name: &str, value: T) {
    if let Some(property) = element.find_property(name) {
        if !property.flags().contains(glib::ParamFlags::WRITABLE) {
            return;
        }

        let value = value.into();
        let value_type = value.type_();
        let property_type = property.value_type();
        if value_type == property_type || value_type.is_a(property_type) {
            element.set_property_from_value(name, &value);
        }
    }
}

fn set_property_from_str_if_supported(element: &gst::Element, name: &str, value: &str) {
    if element.find_property(name).is_some() {
        element.set_property_from_str(name, value);
    }
}

fn configure_webrtc_low_latency(webrtc: &gst::Element) {
    set_property_if_supported(webrtc, "latency", WEBRTC_LATENCY_MS);
}

fn configure_queue_for_low_latency(element: &gst::Element, media_label: &str) {
    let max_buffers = if media_label == "video" {
        VIDEO_QUEUE_MAX_BUFFERS
    } else {
        AUDIO_QUEUE_MAX_BUFFERS
    };

    configure_queue(element, max_buffers, true);
}

fn configure_queue(element: &gst::Element, max_buffers: u32, leaky_downstream: bool) {
    set_property_if_supported(element, "max-size-buffers", max_buffers);
    set_property_if_supported(element, "max-size-bytes", 0u32);
    set_property_if_supported(element, "max-size-time", 0u64);
    if leaky_downstream {
        set_property_from_str_if_supported(element, "leaky", "downstream");
    } else {
        set_property_from_str_if_supported(element, "leaky", "no");
    }
}

fn resolve_queue_mode(settings: &StreamSettings) -> NativeQueueMode {
    if let Some(force_queue_mode) = settings
        .native_transition_diagnostics
        .as_ref()
        .and_then(|diagnostics| diagnostics.force_queue_mode)
    {
        return force_queue_mode;
    }

    if settings.enable_cloud_gsync {
        return NativeQueueMode::Vrr;
    }
    if settings.fps >= 240 {
        return NativeQueueMode::Adaptive;
    }
    NativeQueueMode::Fixed
}

fn format_transition_summary(
    transition_type: &str,
    source: &str,
    requested_fps: Option<u32>,
    old_framerate: Option<&str>,
    new_framerate: Option<&str>,
    high_fps_risk: bool,
) -> String {
    let fps_summary = match (old_framerate, new_framerate) {
        (Some(old), Some(new)) if old != new => format!("framerate {old} -> {new}"),
        (_, Some(new)) => format!("framerate {new}"),
        _ => "framerate unchanged/unknown".to_owned(),
    };
    if high_fps_risk {
        return format!(
            "{transition_type} on {source}: {fps_summary} while requestedFps={} (high-fps transition risk).",
            requested_fps
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_owned())
        );
    }
    format!("{transition_type} on {source}: {fps_summary}.")
}

fn configure_sink_for_low_latency(element: &gst::Element) {
    set_property_if_supported(element, "sync", false);
    set_property_if_supported(element, "async", false);
    set_property_if_supported(element, "qos", false);
    set_property_if_supported(element, "max-lateness", -1i64);
    set_property_if_supported(element, "processing-deadline", 0u64);
    set_property_if_supported(element, "render-delay", 0u64);
    set_property_if_supported(element, "throttle-time", 0u64);
    set_property_if_supported(element, "enable-last-sample", false);
    set_property_if_supported(element, "show-preroll-frame", false);
    set_property_if_supported(element, "redraw-on-update", true);
    set_property_if_supported(element, "force-aspect-ratio", true);
}

fn configure_stats_overlay_element(element: &gst::Element) {
    set_property_if_supported(element, "visible", false);
    set_property_if_supported(element, "text", "");
    set_property_if_supported(element, "auto-resize", true);
    set_property_if_supported(element, "layout-x", 0.018f64);
    set_property_if_supported(element, "layout-y", 0.018f64);
    set_property_if_supported(element, "layout-width", 0.55f64);
    set_property_if_supported(element, "layout-height", 0.18f64);
    set_property_if_supported(element, "font-family", "Cascadia Mono");
    set_property_if_supported(element, "font-size", 18f32);
    set_property_from_str_if_supported(element, "text-alignment", "leading");
    set_property_from_str_if_supported(element, "paragraph-alignment", "near");
    set_property_if_supported(element, "foreground-color", 0xF2FF_FFFFu32);
    set_property_if_supported(element, "outline-color", 0xD000_0000u32);
}

#[cfg(target_os = "windows")]
fn parse_window_handle(value: &str) -> Result<usize, String> {
    let trimmed = value.trim();
    let hex = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"));
    let parsed = if let Some(hex) = hex {
        usize::from_str_radix(hex, 16)
    } else {
        trimmed.parse::<usize>()
    }
    .map_err(|error| format!("Invalid native render window handle {value:?}: {error}"))?;

    if parsed == 0 {
        return Err("Native render window handle is zero.".to_owned());
    }

    Ok(parsed)
}

#[cfg(target_os = "windows")]
fn normalized_render_rect(rect: Option<&NativeRenderRect>) -> NativeRenderRect {
    let Some(rect) = rect else {
        return NativeRenderRect {
            x: 0,
            y: 0,
            width: 2,
            height: 2,
        };
    };

    NativeRenderRect {
        x: rect.x.max(0),
        y: rect.y.max(0),
        width: rect.width.max(2),
        height: rect.height.max(2),
    }
}

fn use_external_renderer_window() -> bool {
    std::env::var(EXTERNAL_RENDERER_ENV)
        .map(|value| {
            !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "no" | "off"
            )
        })
        .unwrap_or(true)
}

#[cfg(target_os = "windows")]
fn start_external_renderer_window_guard(
    event_sender: Option<Sender<Event>>,
    stop: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        let mut logged = false;
        for _ in 0..200 {
            if stop.load(Ordering::SeqCst) {
                break;
            }

            let configured = unsafe { win32_renderer_window::protect_process_renderer_window() };
            if configured && !logged {
                send_log(
                    &event_sender,
                    "info",
                    "Configured external native renderer window for fullscreen DX11 input capture."
                        .to_owned(),
                );
                logged = true;
            }
            thread::sleep(if logged {
                Duration::from_millis(500)
            } else {
                Duration::from_millis(100)
            });
        }
    });
}

#[cfg(not(target_os = "windows"))]
fn start_external_renderer_window_guard(
    _event_sender: Option<Sender<Event>>,
    _stop: Arc<AtomicBool>,
) {
}

#[cfg(target_os = "windows")]
mod win32_renderer_window {
    use super::NativeWindowInputEvent;
    use std::collections::HashMap;
    use std::ffi::c_void;
    use std::ptr::{null, null_mut};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::mpsc::Sender;
    use std::sync::{Mutex, OnceLock};
    use std::thread;
    use std::time::{Duration, Instant};

    type Bool = i32;
    type Dword = u32;
    type Hcursor = *mut c_void;
    type Hmonitor = *mut c_void;
    type Hrawinput = *mut c_void;
    type Hwnd = *mut c_void;
    type Lparam = isize;
    type Lresult = isize;
    type Uint = u32;
    type Wparam = usize;

    const GWL_STYLE: i32 = -16;
    const GWL_EXSTYLE: i32 = -20;
    const GWLP_WNDPROC: i32 = -4;
    const GW_OWNER: Uint = 4;
    const HTCLIENT: isize = 1;
    const HWND_NOTOPMOST: Hwnd = -2isize as Hwnd;
    const MA_ACTIVATE: isize = 1;
    const MONITOR_DEFAULTTONEAREST: Dword = 0x0000_0002;
    const RID_INPUT: Uint = 0x1000_0003;
    const RIDEV_REMOVE: Dword = 0x0000_0001;
    const RIDEV_NOLEGACY: Dword = 0x0000_0030;
    const RIDEV_CAPTUREMOUSE: Dword = 0x0000_0200;
    const RIM_TYPEMOUSE: Dword = 0;
    const RIM_TYPEKEYBOARD: Dword = 1;
    const RI_KEY_BREAK: u16 = 0x0001;
    const RI_KEY_E0: u16 = 0x0002;
    const RI_KEY_E1: u16 = 0x0004;
    const RI_MOUSE_LEFT_BUTTON_DOWN: u16 = 0x0001;
    const RI_MOUSE_LEFT_BUTTON_UP: u16 = 0x0002;
    const RI_MOUSE_RIGHT_BUTTON_DOWN: u16 = 0x0004;
    const RI_MOUSE_RIGHT_BUTTON_UP: u16 = 0x0008;
    const RI_MOUSE_MIDDLE_BUTTON_DOWN: u16 = 0x0010;
    const RI_MOUSE_MIDDLE_BUTTON_UP: u16 = 0x0020;
    const RI_MOUSE_BUTTON_4_DOWN: u16 = 0x0040;
    const RI_MOUSE_BUTTON_4_UP: u16 = 0x0080;
    const RI_MOUSE_BUTTON_5_DOWN: u16 = 0x0100;
    const RI_MOUSE_BUTTON_5_UP: u16 = 0x0200;
    const RI_MOUSE_WHEEL: u16 = 0x0400;
    const VK_SHIFT: u16 = 0x10;
    const VK_ESCAPE: u16 = 0x1B;
    const VK_TAB: u16 = 0x09;
    const VK_CONTROL: u16 = 0x11;
    const VK_MENU: u16 = 0x12;
    const VK_CAPITAL: i32 = 0x14;
    const VK_NUMLOCK: i32 = 0x90;
    const VK_LSHIFT: u16 = 0xA0;
    const VK_RSHIFT: u16 = 0xA1;
    const VK_LCONTROL: u16 = 0xA2;
    const VK_RCONTROL: u16 = 0xA3;
    const VK_LMENU: u16 = 0xA4;
    const VK_RMENU: u16 = 0xA5;
    const VK_LWIN: u16 = 0x5B;
    const VK_RWIN: u16 = 0x5C;
    const WM_INPUT: Uint = 0x00FF;
    const WM_NCHITTEST: Uint = 0x0084;
    const WM_MOUSEACTIVATE: Uint = 0x0021;
    const WM_SETCURSOR: Uint = 0x0020;
    const WM_KILLFOCUS: Uint = 0x0008;
    const WM_ACTIVATE: Uint = 0x0006;
    const WA_INACTIVE: usize = 0;
    const WM_KEYDOWN: Uint = 0x0100;
    const WM_KEYUP: Uint = 0x0101;
    const WM_SYSKEYDOWN: Uint = 0x0104;
    const WM_SYSKEYUP: Uint = 0x0105;
    const WM_LBUTTONDOWN: Uint = 0x0201;
    const WM_LBUTTONUP: Uint = 0x0202;
    const WM_RBUTTONDOWN: Uint = 0x0204;
    const WM_RBUTTONUP: Uint = 0x0205;
    const WM_MBUTTONDOWN: Uint = 0x0207;
    const WM_MBUTTONUP: Uint = 0x0208;
    const WM_XBUTTONDOWN: Uint = 0x020B;
    const WM_XBUTTONUP: Uint = 0x020C;
    const XBUTTON1: u16 = 0x0001;
    const XBUTTON2: u16 = 0x0002;
    const WS_CAPTION: isize = 0x00C0_0000;
    const WS_MAXIMIZEBOX: isize = 0x0001_0000;
    const WS_MINIMIZEBOX: isize = 0x0002_0000;
    const WS_SYSMENU: isize = 0x0008_0000;
    const WS_THICKFRAME: isize = 0x0004_0000;
    const WS_EX_NOACTIVATE: isize = 0x0800_0000;
    const WS_EX_TOOLWINDOW: isize = 0x0000_0080;
    const WS_EX_TRANSPARENT: isize = 0x0000_0020;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SWP_FRAMECHANGED: u32 = 0x0020;
    const SW_MINIMIZE: i32 = 6;
    const ESCAPE_SCANCODE: u16 = 0x0001;
    const ESCAPE_HOLD_TO_MINIMIZE: Duration = Duration::from_secs(5);

    struct EnumState {
        process_id: u32,
        candidates: Vec<WindowCandidate>,
    }

    #[derive(Clone, Copy)]
    struct WindowCandidate {
        hwnd: Hwnd,
        area: i64,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[repr(C)]
    struct MonitorInfo {
        cb_size: Dword,
        rc_monitor: Rect,
        rc_work: Rect,
        dw_flags: Dword,
    }

    #[repr(C)]
    struct RawInputDevice {
        us_usage_page: u16,
        us_usage: u16,
        dw_flags: Dword,
        hwnd_target: Hwnd,
    }

    #[repr(C)]
    struct RawInputHeader {
        dw_type: Dword,
        dw_size: Dword,
        h_device: *mut c_void,
        w_param: Wparam,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct RawMouse {
        us_flags: u16,
        buttons: u32,
        ul_raw_buttons: u32,
        l_last_x: i32,
        l_last_y: i32,
        ul_extra_information: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct RawKeyboard {
        make_code: u16,
        flags: u16,
        reserved: u16,
        vkey: u16,
        message: Uint,
        extra_information: u32,
    }

    #[derive(Clone, Copy)]
    struct PressedKey {
        keycode: u16,
        scancode: u16,
    }

    #[derive(Clone, Copy)]
    struct EscapeKeyPress {
        scancode: u16,
        hold_timer_armed: bool,
    }

    static INPUT_EVENT_SENDER: OnceLock<Mutex<Option<Sender<NativeWindowInputEvent>>>> =
        OnceLock::new();
    static ORIGINAL_WNDPROCS: OnceLock<Mutex<HashMap<isize, isize>>> = OnceLock::new();
    static CAPTURED_HWND: OnceLock<Mutex<Option<isize>>> = OnceLock::new();
    static PRESSED_KEYS: OnceLock<Mutex<HashMap<u16, PressedKey>>> = OnceLock::new();
    static STARTED_AT: OnceLock<Instant> = OnceLock::new();
    static ESCAPE_HOLD_HWND: OnceLock<Mutex<Option<isize>>> = OnceLock::new();
    static ESCAPE_HOLD_TOKEN: OnceLock<AtomicU64> = OnceLock::new();
    static ESCAPE_KEY_PRESS: OnceLock<Mutex<Option<EscapeKeyPress>>> = OnceLock::new();

    #[link(name = "user32")]
    unsafe extern "system" {
        fn CallWindowProcW(
            previous: isize,
            hwnd: Hwnd,
            message: Uint,
            wparam: Wparam,
            lparam: Lparam,
        ) -> Lresult;
        fn ClipCursor(rect: *const Rect) -> Bool;
        fn DefWindowProcW(hwnd: Hwnd, message: Uint, wparam: Wparam, lparam: Lparam) -> Lresult;
        fn EnumWindows(
            callback: Option<unsafe extern "system" fn(Hwnd, Lparam) -> Bool>,
            lparam: Lparam,
        ) -> Bool;
        fn GetMonitorInfoW(monitor: Hmonitor, info: *mut MonitorInfo) -> Bool;
        fn GetRawInputData(
            raw_input: Hrawinput,
            command: Uint,
            data: *mut c_void,
            size: *mut u32,
            header_size: u32,
        ) -> u32;
        fn GetKeyState(virtual_key: i32) -> i16;
        fn GetWindow(hwnd: Hwnd, command: Uint) -> Hwnd;
        fn GetWindowLongPtrW(hwnd: Hwnd, index: i32) -> isize;
        fn GetWindowRect(hwnd: Hwnd, rect: *mut Rect) -> Bool;
        fn GetWindowThreadProcessId(hwnd: Hwnd, process_id: *mut u32) -> u32;
        fn IsIconic(hwnd: Hwnd) -> Bool;
        fn IsWindowVisible(hwnd: Hwnd) -> Bool;
        fn MonitorFromWindow(hwnd: Hwnd, flags: Dword) -> Hmonitor;
        fn RegisterRawInputDevices(devices: *const RawInputDevice, count: u32, size: u32) -> Bool;
        fn ReleaseCapture() -> Bool;
        fn SetCapture(hwnd: Hwnd) -> Hwnd;
        fn SetCursor(cursor: Hcursor) -> Hcursor;
        fn SetFocus(hwnd: Hwnd) -> Hwnd;
        fn SetForegroundWindow(hwnd: Hwnd) -> Bool;
        fn SetWindowLongPtrW(hwnd: Hwnd, index: i32, new_long: isize) -> isize;
        fn SetWindowPos(
            hwnd: Hwnd,
            insert_after: Hwnd,
            x: i32,
            y: i32,
            cx: i32,
            cy: i32,
            flags: u32,
        ) -> Bool;
        fn ShowWindow(hwnd: Hwnd, command: i32) -> Bool;
        fn ShowCursor(show: Bool) -> i32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcessId() -> u32;
    }

    pub unsafe fn set_input_event_sender(sender: Option<Sender<NativeWindowInputEvent>>) {
        let slot = INPUT_EVENT_SENDER.get_or_init(|| Mutex::new(None));
        if let Ok(mut current) = slot.lock() {
            *current = sender;
        }
    }

    pub unsafe fn release_current_input_capture() {
        let Some(captured) = CAPTURED_HWND
            .get()
            .and_then(|captured| captured.lock().ok().and_then(|captured| *captured))
        else {
            unregister_raw_input_devices();
            return;
        };

        release_input_capture(captured as Hwnd);
    }

    pub unsafe fn protect_process_renderer_window() -> bool {
        let mut state = EnumState {
            process_id: GetCurrentProcessId(),
            candidates: Vec::new(),
        };
        EnumWindows(
            Some(collect_renderer_window_candidate),
            &mut state as *mut EnumState as Lparam,
        );

        let Some(candidate) = state
            .candidates
            .into_iter()
            .max_by_key(|candidate| candidate.area)
        else {
            return false;
        };

        protect_renderer_window(candidate.hwnd)
    }

    unsafe extern "system" fn collect_renderer_window_candidate(
        hwnd: Hwnd,
        lparam: Lparam,
    ) -> Bool {
        let state = &mut *(lparam as *mut EnumState);
        let mut process_id = 0;
        GetWindowThreadProcessId(hwnd, &mut process_id);
        if process_id != state.process_id || IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) != 0 {
            return 1;
        }

        if !GetWindow(hwnd, GW_OWNER).is_null() {
            return 1;
        }

        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if (ex_style & (WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE)) != 0 {
            return 1;
        }

        let mut rect = Rect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return 1;
        }
        let width = rect.right.saturating_sub(rect.left);
        let height = rect.bottom.saturating_sub(rect.top);
        if width < 320 || height < 180 {
            return 1;
        }

        state.candidates.push(WindowCandidate {
            hwnd,
            area: i64::from(width) * i64::from(height),
        });
        1
    }

    unsafe fn protect_renderer_window(hwnd: Hwnd) -> bool {
        let mut configured = false;
        let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let desired = current & !(WS_EX_NOACTIVATE | WS_EX_TRANSPARENT);
        if desired != current {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, desired);
            configured = true;
        }

        let current_style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        let fullscreen_style = current_style
            & !(WS_CAPTION | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_SYSMENU);
        if fullscreen_style != current_style {
            SetWindowLongPtrW(hwnd, GWL_STYLE, fullscreen_style);
            configured = true;
        }

        if install_input_wndproc(hwnd) {
            SetForegroundWindow(hwnd);
            SetFocus(hwnd);
            configured = true;
        }

        if let Some(rect) = monitor_rect_for_window(hwnd) {
            SetWindowPos(
                hwnd,
                HWND_NOTOPMOST,
                rect.left,
                rect.top,
                rect.right.saturating_sub(rect.left).max(2),
                rect.bottom.saturating_sub(rect.top).max(2),
                SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
            configured = true;
        } else {
            SetWindowPos(
                hwnd,
                HWND_NOTOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }

        configured
    }

    unsafe fn install_input_wndproc(hwnd: Hwnd) -> bool {
        let key = hwnd as isize;
        let map = ORIGINAL_WNDPROCS.get_or_init(|| Mutex::new(HashMap::new()));
        let Ok(mut map) = map.lock() else {
            return false;
        };
        if map.contains_key(&key) {
            return false;
        }

        let previous = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, renderer_window_wndproc as isize);
        if previous == 0 {
            return false;
        }
        map.insert(key, previous);
        true
    }

    unsafe extern "system" fn renderer_window_wndproc(
        hwnd: Hwnd,
        message: Uint,
        wparam: Wparam,
        lparam: Lparam,
    ) -> Lresult {
        if message == WM_NCHITTEST {
            return HTCLIENT;
        }
        if message == WM_MOUSEACTIVATE {
            begin_input_capture(hwnd);
            return MA_ACTIVATE;
        }
        if message == WM_SETCURSOR && is_input_captured(hwnd) {
            SetCursor(null_mut());
            return 1;
        }
        if message == WM_INPUT {
            handle_raw_input(lparam as Hrawinput);
            return 0;
        }
        if is_escape_keyboard_message(message, wparam) {
            if !is_input_captured(hwnd) {
                begin_input_capture(hwnd);
            }
            handle_legacy_escape_keyboard(message, lparam);
            return 0;
        }
        if message == WM_KILLFOCUS || (message == WM_ACTIVATE && (wparam & 0xffff) == WA_INACTIVE) {
            release_input_capture(hwnd);
        }
        if let Some((button, pressed)) = legacy_mouse_button(message, wparam) {
            let was_captured = is_input_captured(hwnd);
            if pressed && !was_captured {
                begin_input_capture(hwnd);
                emit_input_event(NativeWindowInputEvent::MouseButton {
                    pressed,
                    button,
                    timestamp_us: timestamp_us(),
                });
            }
        }

        let key = hwnd as isize;
        let previous = ORIGINAL_WNDPROCS
            .get()
            .and_then(|map| map.lock().ok().and_then(|map| map.get(&key).copied()));
        if let Some(previous) = previous {
            return CallWindowProcW(previous, hwnd, message, wparam, lparam);
        }

        DefWindowProcW(hwnd, message, wparam, lparam)
    }

    unsafe fn monitor_rect_for_window(hwnd: Hwnd) -> Option<Rect> {
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        if monitor.is_null() {
            return None;
        }

        let mut info = MonitorInfo {
            cb_size: std::mem::size_of::<MonitorInfo>() as Dword,
            rc_monitor: Rect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            rc_work: Rect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            dw_flags: 0,
        };
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return None;
        }

        Some(info.rc_monitor)
    }

    unsafe fn begin_input_capture(hwnd: Hwnd) {
        SetForegroundWindow(hwnd);
        SetFocus(hwnd);
        SetCapture(hwnd);
        register_raw_input_devices(hwnd);
        if let Some(rect) = monitor_rect_for_window(hwnd) {
            ClipCursor(&rect);
        }
        hide_cursor();

        let slot = CAPTURED_HWND.get_or_init(|| Mutex::new(None));
        if let Ok(mut captured) = slot.lock() {
            *captured = Some(hwnd as isize);
        }
    }

    unsafe fn release_input_capture(hwnd: Hwnd) {
        cancel_escape_hold_to_minimize_timer();
        clear_escape_key_press();
        let slot = CAPTURED_HWND.get_or_init(|| Mutex::new(None));
        let mut should_release = false;
        if let Ok(mut captured) = slot.lock() {
            should_release = captured.is_some_and(|captured| captured == hwnd as isize);
            if should_release {
                *captured = None;
            }
        }

        if !should_release {
            return;
        }

        release_pressed_keys();
        ReleaseCapture();
        ClipCursor(null());
        show_cursor();
        unregister_raw_input_devices();
        SetWindowPos(
            hwnd,
            HWND_NOTOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }

    fn is_input_captured(hwnd: Hwnd) -> bool {
        CAPTURED_HWND
            .get()
            .and_then(|captured| captured.lock().ok().and_then(|captured| *captured))
            .is_some_and(|captured| captured == hwnd as isize)
    }

    fn captured_hwnd() -> Option<isize> {
        CAPTURED_HWND
            .get()
            .and_then(|captured| captured.lock().ok().and_then(|captured| *captured))
    }

    unsafe fn start_escape_hold_to_minimize_timer() {
        let Some(hwnd) = captured_hwnd() else {
            return;
        };

        let token = ESCAPE_HOLD_TOKEN
            .get_or_init(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::SeqCst)
            .wrapping_add(1);
        let slot = ESCAPE_HOLD_HWND.get_or_init(|| Mutex::new(None));
        if let Ok(mut held_hwnd) = slot.lock() {
            *held_hwnd = Some(hwnd);
        }

        thread::spawn(move || {
            thread::sleep(ESCAPE_HOLD_TO_MINIMIZE);
            unsafe {
                minimize_window_if_escape_still_held(hwnd, token);
            }
        });
    }

    fn cancel_escape_hold_to_minimize_timer() {
        ESCAPE_HOLD_TOKEN
            .get_or_init(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::SeqCst);
        let slot = ESCAPE_HOLD_HWND.get_or_init(|| Mutex::new(None));
        if let Ok(mut held_hwnd) = slot.lock() {
            *held_hwnd = None;
        }
    }

    unsafe fn minimize_window_if_escape_still_held(hwnd: isize, token: u64) {
        let current_token = ESCAPE_HOLD_TOKEN
            .get_or_init(|| AtomicU64::new(0))
            .load(Ordering::SeqCst);
        if current_token != token {
            return;
        }

        let still_held = ESCAPE_HOLD_HWND
            .get()
            .and_then(|held_hwnd| held_hwnd.lock().ok().and_then(|held_hwnd| *held_hwnd))
            .is_some_and(|held_hwnd| held_hwnd == hwnd);
        if !still_held {
            return;
        }

        let hwnd = hwnd as Hwnd;
        release_input_capture(hwnd);
        ShowWindow(hwnd, SW_MINIMIZE);
    }

    unsafe fn register_raw_input_devices(hwnd: Hwnd) -> bool {
        let devices = [
            RawInputDevice {
                us_usage_page: 0x01,
                us_usage: 0x02,
                dw_flags: RIDEV_NOLEGACY | RIDEV_CAPTUREMOUSE,
                hwnd_target: hwnd,
            },
            RawInputDevice {
                us_usage_page: 0x01,
                us_usage: 0x06,
                dw_flags: 0,
                hwnd_target: hwnd,
            },
        ];

        RegisterRawInputDevices(
            devices.as_ptr(),
            devices.len() as u32,
            std::mem::size_of::<RawInputDevice>() as u32,
        ) != 0
    }

    unsafe fn unregister_raw_input_devices() -> bool {
        let devices = [
            RawInputDevice {
                us_usage_page: 0x01,
                us_usage: 0x02,
                dw_flags: RIDEV_REMOVE,
                hwnd_target: null_mut(),
            },
            RawInputDevice {
                us_usage_page: 0x01,
                us_usage: 0x06,
                dw_flags: RIDEV_REMOVE,
                hwnd_target: null_mut(),
            },
        ];

        RegisterRawInputDevices(
            devices.as_ptr(),
            devices.len() as u32,
            std::mem::size_of::<RawInputDevice>() as u32,
        ) != 0
    }

    unsafe fn handle_raw_input(raw_input: Hrawinput) {
        let mut size = 0u32;
        let header_size = std::mem::size_of::<RawInputHeader>() as u32;
        let query = GetRawInputData(raw_input, RID_INPUT, null_mut(), &mut size, header_size);
        if query == u32::MAX || size < header_size {
            return;
        }

        let mut buffer = vec![0u8; size as usize];
        let read = GetRawInputData(
            raw_input,
            RID_INPUT,
            buffer.as_mut_ptr() as *mut c_void,
            &mut size,
            header_size,
        );
        if read == u32::MAX || read == 0 || buffer.len() < header_size as usize {
            return;
        }

        let header = &*(buffer.as_ptr() as *const RawInputHeader);
        let data = buffer.as_ptr().add(std::mem::size_of::<RawInputHeader>());
        match header.dw_type {
            RIM_TYPEMOUSE => handle_raw_mouse(&*(data as *const RawMouse)),
            RIM_TYPEKEYBOARD => handle_raw_keyboard(&*(data as *const RawKeyboard)),
            _ => {}
        }
    }

    unsafe fn handle_raw_mouse(raw: &RawMouse) {
        if CAPTURED_HWND
            .get()
            .and_then(|captured| captured.lock().ok().and_then(|captured| *captured))
            .is_none()
        {
            return;
        }

        let timestamp_us = timestamp_us();
        let dx = clamp_i32_to_i16(raw.l_last_x);
        let dy = clamp_i32_to_i16(raw.l_last_y);
        if dx != 0 || dy != 0 {
            emit_input_event(NativeWindowInputEvent::MouseMove {
                dx,
                dy,
                timestamp_us,
            });
        }

        let button_flags = (raw.buttons & 0xffff) as u16;
        let button_data = (raw.buttons >> 16) as u16;
        emit_raw_mouse_button_events(button_flags, timestamp_us);
        if (button_flags & RI_MOUSE_WHEEL) != 0 {
            emit_input_event(NativeWindowInputEvent::MouseWheel {
                delta: button_data as i16,
                timestamp_us,
            });
        }
    }

    unsafe fn emit_raw_mouse_button_events(flags: u16, timestamp_us: u64) {
        let pairs = [
            (RI_MOUSE_LEFT_BUTTON_DOWN, 1, true),
            (RI_MOUSE_LEFT_BUTTON_UP, 1, false),
            (RI_MOUSE_MIDDLE_BUTTON_DOWN, 2, true),
            (RI_MOUSE_MIDDLE_BUTTON_UP, 2, false),
            (RI_MOUSE_RIGHT_BUTTON_DOWN, 3, true),
            (RI_MOUSE_RIGHT_BUTTON_UP, 3, false),
            (RI_MOUSE_BUTTON_4_DOWN, 4, true),
            (RI_MOUSE_BUTTON_4_UP, 4, false),
            (RI_MOUSE_BUTTON_5_DOWN, 5, true),
            (RI_MOUSE_BUTTON_5_UP, 5, false),
        ];

        for (flag, button, pressed) in pairs {
            if (flags & flag) != 0 {
                emit_input_event(NativeWindowInputEvent::MouseButton {
                    pressed,
                    button,
                    timestamp_us,
                });
            }
        }
    }

    unsafe fn handle_raw_keyboard(raw: &RawKeyboard) {
        if raw.vkey == 0xff {
            return;
        }

        let pressed = match raw.message {
            WM_KEYDOWN | WM_SYSKEYDOWN => true,
            WM_KEYUP | WM_SYSKEYUP => false,
            _ => (raw.flags & RI_KEY_BREAK) == 0,
        };
        let keycode = normalize_virtual_key(raw.vkey, raw.make_code, raw.flags);
        let mut scancode = normalize_scancode(raw.make_code, raw.flags);
        if keycode == VK_ESCAPE && scancode == 0 {
            scancode = ESCAPE_SCANCODE;
        }
        if keycode == 0 || scancode == 0 {
            return;
        }
        handle_keyboard_state(keycode, scancode, pressed);
    }

    unsafe fn handle_legacy_escape_keyboard(message: Uint, lparam: Lparam) {
        let pressed = matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN);
        let mut scancode = legacy_keyboard_scancode(lparam);
        if scancode == 0 {
            scancode = ESCAPE_SCANCODE;
        }
        handle_keyboard_state(VK_ESCAPE, scancode, pressed);
    }

    fn is_escape_keyboard_message(message: Uint, wparam: Wparam) -> bool {
        matches!(message, WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP)
            && (wparam as u16) == VK_ESCAPE
    }

    fn legacy_keyboard_scancode(lparam: Lparam) -> u16 {
        let scancode = ((lparam >> 16) & 0xff) as u16;
        if scancode == 0 {
            return 0;
        }
        if ((lparam >> 24) & 0x01) != 0 {
            0xe000 | scancode
        } else {
            scancode
        }
    }

    unsafe fn handle_keyboard_state(keycode: u16, scancode: u16, pressed: bool) {
        let keys = PRESSED_KEYS.get_or_init(|| Mutex::new(HashMap::new()));
        let Ok(mut keys) = keys.lock() else {
            return;
        };
        if pressed && keycode == VK_TAB && is_alt_modifier_down(&keys) {
            drop(keys);
            release_current_input_capture();
            return;
        }
        if keycode == VK_ESCAPE {
            drop(keys);
            handle_escape_keyboard_state(scancode, pressed);
            return;
        }
        let was_present = keys.contains_key(&scancode);
        if pressed {
            if was_present {
                return;
            }
            keys.insert(scancode, PressedKey { keycode, scancode });
        } else {
            if !was_present {
                return;
            }
            keys.remove(&scancode);
        }
        let modifiers = current_modifier_flags(&keys);
        drop(keys);

        emit_input_event(NativeWindowInputEvent::Key {
            pressed,
            keycode,
            scancode,
            modifiers,
            timestamp_us: timestamp_us(),
        });
    }

    unsafe fn handle_escape_keyboard_state(scancode: u16, pressed: bool) {
        let slot = ESCAPE_KEY_PRESS.get_or_init(|| Mutex::new(None));
        let Ok(mut escape_press) = slot.lock() else {
            return;
        };

        if pressed {
            let should_start_hold_timer = if let Some(current) = escape_press.as_mut() {
                let should_start = !current.hold_timer_armed && captured_hwnd().is_some();
                if should_start {
                    current.hold_timer_armed = true;
                }
                should_start
            } else {
                let hold_timer_armed = captured_hwnd().is_some();
                *escape_press = Some(EscapeKeyPress {
                    scancode,
                    hold_timer_armed,
                });
                hold_timer_armed
            };
            drop(escape_press);
            if should_start_hold_timer {
                start_escape_hold_to_minimize_timer();
            }
            return;
        }

        let Some(escape_press) = escape_press.take() else {
            cancel_escape_hold_to_minimize_timer();
            return;
        };
        let scancode = escape_press.scancode;

        cancel_escape_hold_to_minimize_timer();
        send_escape_tap(scancode);
    }

    fn clear_escape_key_press() {
        let slot = ESCAPE_KEY_PRESS.get_or_init(|| Mutex::new(None));
        if let Ok(mut escape_press) = slot.lock() {
            *escape_press = None;
        }
    }

    fn send_escape_tap(scancode: u16) {
        let keydown_timestamp_us = timestamp_us();
        emit_input_event(NativeWindowInputEvent::Key {
            pressed: true,
            keycode: VK_ESCAPE,
            scancode,
            modifiers: 0,
            timestamp_us: keydown_timestamp_us,
        });
        emit_input_event(NativeWindowInputEvent::Key {
            pressed: false,
            keycode: VK_ESCAPE,
            scancode,
            modifiers: 0,
            timestamp_us: timestamp_us(),
        });
    }

    unsafe fn release_pressed_keys() {
        let keys = PRESSED_KEYS.get_or_init(|| Mutex::new(HashMap::new()));
        let Ok(mut keys) = keys.lock() else {
            return;
        };
        let pressed = keys.values().copied().collect::<Vec<_>>();
        keys.clear();
        drop(keys);

        let timestamp_us = timestamp_us();
        for key in pressed {
            emit_input_event(NativeWindowInputEvent::Key {
                pressed: false,
                keycode: key.keycode,
                scancode: key.scancode,
                modifiers: 0,
                timestamp_us,
            });
        }
    }

    fn normalize_virtual_key(vkey: u16, make_code: u16, flags: u16) -> u16 {
        match vkey {
            VK_SHIFT => match make_code {
                0x36 => VK_RSHIFT,
                _ => VK_LSHIFT,
            },
            VK_CONTROL => {
                if (flags & RI_KEY_E0) != 0 {
                    VK_RCONTROL
                } else {
                    VK_LCONTROL
                }
            }
            VK_MENU => {
                if (flags & RI_KEY_E0) != 0 {
                    VK_RMENU
                } else {
                    VK_LMENU
                }
            }
            _ => vkey,
        }
    }

    fn normalize_scancode(make_code: u16, flags: u16) -> u16 {
        if make_code == 0 {
            return 0;
        }
        if (flags & RI_KEY_E0) != 0 {
            0xe000 | make_code
        } else if (flags & RI_KEY_E1) != 0 {
            0xe100 | make_code
        } else {
            make_code
        }
    }

    unsafe fn current_modifier_flags(keys: &HashMap<u16, PressedKey>) -> u16 {
        let mut modifiers = 0u16;
        if keys
            .values()
            .any(|key| matches!(key.keycode, VK_LSHIFT | VK_RSHIFT | VK_SHIFT))
        {
            modifiers |= 0x01;
        }
        if keys
            .values()
            .any(|key| matches!(key.keycode, VK_LCONTROL | VK_RCONTROL | VK_CONTROL))
        {
            modifiers |= 0x02;
        }
        if keys
            .values()
            .any(|key| matches!(key.keycode, VK_LMENU | VK_RMENU | VK_MENU))
        {
            modifiers |= 0x04;
        }
        if keys
            .values()
            .any(|key| matches!(key.keycode, VK_LWIN | VK_RWIN))
        {
            modifiers |= 0x08;
        }
        if (GetKeyState(VK_CAPITAL) & 0x0001) != 0 {
            modifiers |= 0x10;
        }
        if (GetKeyState(VK_NUMLOCK) & 0x0001) != 0 {
            modifiers |= 0x20;
        }
        modifiers
    }

    unsafe fn is_alt_modifier_down(keys: &HashMap<u16, PressedKey>) -> bool {
        keys.values()
            .any(|key| matches!(key.keycode, VK_LMENU | VK_RMENU | VK_MENU))
            || ((GetKeyState(VK_MENU as i32) as u16) & 0x8000) != 0
    }

    fn legacy_mouse_button(message: Uint, wparam: Wparam) -> Option<(u8, bool)> {
        match message {
            WM_LBUTTONDOWN => Some((1, true)),
            WM_LBUTTONUP => Some((1, false)),
            WM_MBUTTONDOWN => Some((2, true)),
            WM_MBUTTONUP => Some((2, false)),
            WM_RBUTTONDOWN => Some((3, true)),
            WM_RBUTTONUP => Some((3, false)),
            WM_XBUTTONDOWN | WM_XBUTTONUP => {
                let xbutton = ((wparam >> 16) & 0xffff) as u16;
                let button = match xbutton {
                    XBUTTON1 => 4,
                    XBUTTON2 => 5,
                    _ => return None,
                };
                Some((button, message == WM_XBUTTONDOWN))
            }
            _ => None,
        }
    }

    fn emit_input_event(event: NativeWindowInputEvent) {
        let Some(sender) = INPUT_EVENT_SENDER
            .get()
            .and_then(|sender| sender.lock().ok().and_then(|sender| sender.clone()))
        else {
            return;
        };
        let _ = sender.send(event);
    }

    fn clamp_i32_to_i16(value: i32) -> i16 {
        value.clamp(i16::MIN as i32, i16::MAX as i32) as i16
    }

    fn timestamp_us() -> u64 {
        STARTED_AT
            .get_or_init(Instant::now)
            .elapsed()
            .as_micros()
            .min(u128::from(u64::MAX)) as u64
    }

    unsafe fn hide_cursor() {
        while ShowCursor(0) >= 0 {}
    }

    unsafe fn show_cursor() {
        while ShowCursor(1) < 0 {}
    }
}

#[cfg(target_os = "windows")]
fn apply_render_surface_to_video_sink(
    sink: &gst::Element,
    surface: &NativeRenderSurface,
) -> Result<(), String> {
    let Some(window_handle) = surface.window_handle.as_deref() else {
        return Ok(());
    };

    let handle = parse_window_handle(window_handle)?;
    let overlay = sink
        .clone()
        .dynamic_cast::<gst_video::VideoOverlay>()
        .map_err(|_| {
            format!(
                "Native render sink {} does not implement GstVideoOverlay.",
                sink.name()
            )
        })?;
    let rect = normalized_render_rect(surface.visible.then_some(()).and(surface.rect.as_ref()));

    unsafe {
        overlay.set_window_handle(handle);
    }
    overlay.handle_events(false);
    overlay
        .set_render_rectangle(rect.x, rect.y, rect.width, rect.height)
        .map_err(|error| format!("Failed to set native render rectangle: {error}"))?;
    overlay.expose();
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn apply_render_surface_to_video_sink(
    _sink: &gst::Element,
    _surface: &NativeRenderSurface,
) -> Result<(), String> {
    Ok(())
}

fn wait_for_promise(promise: &gst::Promise, operation: &str) -> Result<(), String> {
    match promise.wait() {
        gst::PromiseResult::Replied => {
            if let Some(reply) = promise.get_reply() {
                if reply.has_field("error") {
                    return Err(format!(
                        "GStreamer promise returned an error during {operation}: {}",
                        describe_structure(reply)
                    ));
                }
            }
            Ok(())
        }
        gst::PromiseResult::Interrupted => {
            Err(format!("GStreamer promise interrupted during {operation}."))
        }
        gst::PromiseResult::Expired => {
            Err(format!("GStreamer promise expired during {operation}."))
        }
        gst::PromiseResult::Pending => Err(format!(
            "GStreamer promise still pending during {operation}."
        )),
        other => Err(format!(
            "GStreamer promise failed during {operation}: {other:?}"
        )),
    }
}

fn describe_structure(structure: &gst::StructureRef) -> String {
    let fields = structure
        .iter()
        .map(|(name, value)| {
            let rendered = value
                .get::<&glib::Error>()
                .map(|error| format!("{error:?}"))
                .unwrap_or_else(|_| format!("{value:?}"));
            format!("{}={rendered}", name.as_str())
        })
        .collect::<Vec<_>>();

    format!("{} {{{}}}", structure.name().as_str(), fields.join(", "))
}

fn wire_local_ice_events(
    webrtc: &gst::Element,
    event_sender: Option<Sender<Event>>,
) -> Result<(), String> {
    let Some(event_sender) = event_sender else {
        return Ok(());
    };

    webrtc.connect("on-ice-candidate", false, move |values| {
        let sdp_m_line_index = values.get(1).and_then(glib_value_to_u32).unwrap_or(0);
        let candidate = values
            .get(2)
            .and_then(|value| value.get::<String>().ok())
            .unwrap_or_default();

        if !candidate.trim().is_empty() {
            let _ = event_sender.send(Event::LocalIce {
                candidate: IceCandidatePayload {
                    candidate,
                    sdp_mid: Some(sdp_m_line_index.to_string()),
                    sdp_m_line_index: Some(sdp_m_line_index),
                    username_fragment: None,
                },
            });
        }

        None
    });
    Ok(())
}

fn glib_value_to_u32(value: &glib::Value) -> Option<u32> {
    let value_type = value.type_();
    if value_type == u32::static_type() {
        return value.get::<u32>().ok();
    }
    if value_type == i32::static_type() {
        return value
            .get::<i32>()
            .ok()
            .and_then(|value| u32::try_from(value).ok());
    }
    if value_type == u64::static_type() {
        return value
            .get::<u64>()
            .ok()
            .and_then(|value| u32::try_from(value).ok());
    }
    if value_type == i64::static_type() {
        return value
            .get::<i64>()
            .ok()
            .and_then(|value| u32::try_from(value).ok());
    }
    None
}

fn wire_webrtc_state_events(webrtc: &gst::Element, event_sender: Option<Sender<Event>>) {
    wire_webrtc_property_event(
        webrtc,
        event_sender.clone(),
        "ice-connection-state",
        "ICE connection state",
    );
    wire_webrtc_property_event(
        webrtc,
        event_sender.clone(),
        "ice-gathering-state",
        "ICE gathering state",
    );
    wire_webrtc_property_event(
        webrtc,
        event_sender,
        "connection-state",
        "peer connection state",
    );
}

fn wire_webrtc_property_event(
    webrtc: &gst::Element,
    event_sender: Option<Sender<Event>>,
    property_name: &'static str,
    label: &'static str,
) {
    if event_sender.is_none() || webrtc.find_property(property_name).is_none() {
        return;
    }

    webrtc.connect_notify(Some(property_name), move |element, _| {
        let value = element.property_value(property_name);
        send_log(
            &event_sender,
            "debug",
            format!("GStreamer WebRTC {label}: {value:?}."),
        );
    });
}

fn wire_remote_data_channels(webrtc: &gst::Element, event_sender: Option<Sender<Event>>) {
    webrtc.connect("on-data-channel", false, move |values| {
        let Some(channel) = values
            .get(1)
            .and_then(|value| value.get::<gst_webrtc::WebRTCDataChannel>().ok())
        else {
            send_log(
                &event_sender,
                "warn",
                "GStreamer emitted on-data-channel without a channel.".to_owned(),
            );
            return None;
        };

        let label = channel_label(&channel);
        send_log(
            &event_sender,
            "info",
            format!(
                "Remote WebRTC data channel received: label={}, ordered={}.",
                label,
                channel.is_ordered()
            ),
        );
        connect_remote_data_channel_callbacks(&label, &channel, event_sender.clone());
        None
    });
}

fn create_input_data_channels(
    webrtc: &gst::Element,
    input_state: GstreamerInputState,
    event_sender: Option<Sender<Event>>,
    partial_reliable_threshold_ms: u32,
) -> Result<GstreamerInputChannels, String> {
    let reliable = create_data_channel(webrtc, RELIABLE_INPUT_CHANNEL_LABEL, None)?;
    connect_input_channel_callbacks(
        RELIABLE_INPUT_CHANNEL_LABEL,
        &reliable,
        input_state.clone(),
        event_sender.clone(),
    );

    let clamped_threshold_ms = if partial_reliable_threshold_ms == 0 {
        DEFAULT_PARTIAL_RELIABLE_THRESHOLD_MS
    } else {
        partial_reliable_threshold_ms.clamp(1, 5000)
    };
    let options = gst::Structure::builder("data-channel-options")
        .field("ordered", false)
        .field("max-packet-lifetime", clamped_threshold_ms as i32)
        .build();
    let partially_reliable = create_data_channel(
        webrtc,
        PARTIALLY_RELIABLE_INPUT_CHANNEL_LABEL,
        Some(options),
    )?;
    connect_input_channel_callbacks(
        PARTIALLY_RELIABLE_INPUT_CHANNEL_LABEL,
        &partially_reliable,
        input_state,
        event_sender.clone(),
    );

    send_log(
        &event_sender,
        "info",
        format!(
            "Created WebRTC input data channels ({}, {} maxPacketLifeTime={}ms).",
            RELIABLE_INPUT_CHANNEL_LABEL,
            PARTIALLY_RELIABLE_INPUT_CHANNEL_LABEL,
            clamped_threshold_ms
        ),
    );

    Ok(GstreamerInputChannels {
        reliable,
        partially_reliable,
    })
}

fn create_data_channel(
    webrtc: &gst::Element,
    label: &'static str,
    options: Option<gst::Structure>,
) -> Result<gst_webrtc::WebRTCDataChannel, String> {
    let channel = match options {
        Some(options) => {
            let options = Some(options);
            webrtc.emit_by_name::<gst_webrtc::WebRTCDataChannel>(
                "create-data-channel",
                &[&label, &options],
            )
        }
        None => webrtc.emit_by_name::<gst_webrtc::WebRTCDataChannel>(
            "create-data-channel",
            &[&label, &None::<gst::Structure>],
        ),
    };

    let actual_label = channel_label(&channel);
    if actual_label != label {
        return Err(format!(
            "GStreamer created data channel with unexpected label: expected {label}, got {actual_label}."
        ));
    }

    Ok(channel)
}

fn connect_input_channel_callbacks(
    label: &'static str,
    channel: &gst_webrtc::WebRTCDataChannel,
    input_state: GstreamerInputState,
    event_sender: Option<Sender<Event>>,
) {
    let open_sender = event_sender.clone();
    channel.connect_on_open(move |channel| {
        send_log(
            &open_sender,
            "info",
            format!(
                "Input data channel open: label={}, id={}, ordered={}, maxPacketLifeTime={}.",
                label,
                channel.id(),
                channel.is_ordered(),
                channel.max_packet_lifetime()
            ),
        );
    });

    let close_sender = event_sender.clone();
    let close_state = input_state.clone();
    channel.connect_on_close(move |_| {
        if label == RELIABLE_INPUT_CHANNEL_LABEL {
            close_state.ready.store(false, Ordering::SeqCst);
            close_state.heartbeat_stop.store(true, Ordering::SeqCst);
        }
        send_log(
            &close_sender,
            "info",
            format!("Input data channel closed: label={label}."),
        );
    });

    let error_sender = event_sender.clone();
    channel.connect_on_error(move |_, error| {
        send_log(
            &error_sender,
            "warn",
            format!("Input data channel error on {label}: {error}."),
        );
    });

    if label == RELIABLE_INPUT_CHANNEL_LABEL {
        let data_sender = event_sender.clone();
        let data_state = input_state.clone();
        channel.connect_on_message_data(move |channel, data| {
            let Some(bytes) = data else {
                return;
            };
            handle_input_handshake_message(
                channel,
                bytes.as_ref(),
                data_state.clone(),
                data_sender.clone(),
            );
        });

        let string_sender = event_sender.clone();
        let string_state = input_state;
        channel.connect_on_message_string(move |channel, message| {
            let Some(message) = message else {
                return;
            };
            handle_input_handshake_message(
                channel,
                message.as_bytes(),
                string_state.clone(),
                string_sender.clone(),
            );
        });
    }
}

fn connect_remote_data_channel_callbacks(
    label: &str,
    channel: &gst_webrtc::WebRTCDataChannel,
    event_sender: Option<Sender<Event>>,
) {
    let label = label.to_owned();
    let open_sender = event_sender.clone();
    let open_label = label.clone();
    channel.connect_on_open(move |_| {
        send_log(
            &open_sender,
            "info",
            format!("Remote data channel open: label={open_label}."),
        );
    });

    let close_sender = event_sender.clone();
    let close_label = label.clone();
    channel.connect_on_close(move |_| {
        send_log(
            &close_sender,
            "info",
            format!("Remote data channel closed: label={close_label}."),
        );
    });

    let error_sender = event_sender;
    channel.connect_on_error(move |_, error| {
        send_log(
            &error_sender,
            "warn",
            format!("Remote data channel error on {label}: {error}."),
        );
    });
}

fn handle_input_handshake_message(
    channel: &gst_webrtc::WebRTCDataChannel,
    bytes: &[u8],
    input_state: GstreamerInputState,
    event_sender: Option<Sender<Event>>,
) {
    let Some(protocol_version) = parse_input_handshake_version(bytes) else {
        return;
    };

    let encoder_version = protocol_version.min(u8::MAX as u16) as u8;
    if let Ok(mut encoder) = input_state.encoder.lock() {
        encoder.set_protocol_version(encoder_version);
    }
    let was_ready = input_state.ready.swap(true, Ordering::SeqCst);
    if was_ready {
        return;
    }

    send_log(
        &event_sender,
        "info",
        format!(
            "Input handshake complete on {} (protocol v{}).",
            channel_label(channel),
            protocol_version
        ),
    );
    if let Some(sender) = event_sender.as_ref() {
        let _ = sender.send(Event::InputReady { protocol_version });
    }
    start_input_heartbeat(input_state, channel.clone(), event_sender);
}

fn parse_input_handshake_version(bytes: &[u8]) -> Option<u16> {
    if bytes.len() < 2 {
        return None;
    }

    let first_word = u16::from_le_bytes([bytes[0], bytes[1]]);
    if first_word == 526 {
        return Some(if bytes.len() >= 4 {
            u16::from_le_bytes([bytes[2], bytes[3]])
        } else {
            2
        });
    }

    if bytes[0] == 0x0e {
        return Some(first_word);
    }

    None
}

fn start_input_heartbeat(
    input_state: GstreamerInputState,
    channel: gst_webrtc::WebRTCDataChannel,
    event_sender: Option<Sender<Event>>,
) {
    let Ok(mut heartbeat_thread) = input_state.heartbeat_thread.lock() else {
        send_log(
            &event_sender,
            "warn",
            "Failed to acquire input heartbeat thread lock.".to_owned(),
        );
        return;
    };
    if heartbeat_thread
        .as_ref()
        .is_some_and(|thread| !thread.is_finished())
    {
        return;
    }
    if let Some(thread) = heartbeat_thread.take() {
        let _ = thread.join();
    }

    input_state.heartbeat_stop.store(false, Ordering::SeqCst);
    let encoder = input_state.encoder.clone();
    let stop = input_state.heartbeat_stop.clone();
    let thread_sender = event_sender.clone();
    *heartbeat_thread = Some(thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            send_input_heartbeat(&channel, &encoder, &thread_sender);

            let mut slept = Duration::ZERO;
            while slept < HEARTBEAT_INTERVAL {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let remaining = HEARTBEAT_INTERVAL.saturating_sub(slept);
                let interval = remaining.min(HEARTBEAT_STOP_POLL_INTERVAL);
                thread::sleep(interval);
                slept += interval;
            }
        }
    }));
}

fn send_input_heartbeat(
    channel: &gst_webrtc::WebRTCDataChannel,
    encoder: &Arc<Mutex<InputEncoder>>,
    event_sender: &Option<Sender<Event>>,
) {
    if channel.ready_state() != gst_webrtc::WebRTCDataChannelState::Open {
        return;
    }

    let Ok(encoder) = encoder.lock() else {
        send_log(
            event_sender,
            "warn",
            "Failed to acquire input encoder for heartbeat.".to_owned(),
        );
        return;
    };
    let bytes = glib::Bytes::from_owned(encoder.encode_heartbeat());
    if let Err(error) = channel.send_data_full(Some(&bytes)) {
        send_log(
            event_sender,
            "warn",
            format!("Failed to send input heartbeat: {error}."),
        );
    }
}

fn channel_label(channel: &gst_webrtc::WebRTCDataChannel) -> String {
    channel
        .label()
        .map(|label| label.to_string())
        .unwrap_or_else(|| "<unlabeled>".to_owned())
}

fn send_log(event_sender: &Option<Sender<Event>>, level: &'static str, message: String) {
    if let Some(event_sender) = event_sender {
        let _ = event_sender.send(Event::Log { level, message });
    } else {
        eprintln!("[NativeStreamer] {message}");
    }
}

fn run_video_liveness_watchdog(
    state: Arc<VideoLivenessState>,
    stop: Arc<AtomicBool>,
    pipeline: gst::Pipeline,
    sink: gst::Element,
    event_sender: Option<Sender<Event>>,
) {
    let mut tracker = VideoStallTracker::default();
    let mut last_rate_at = Instant::now();
    let mut last_encoded_bytes_total = state.encoded_bytes_total.load(Ordering::Relaxed);
    let mut last_decoded_total = state.decoded_total.load(Ordering::Relaxed);
    let mut last_sink_total = state.sink_total.load(Ordering::Relaxed);
    let mut rates = VideoRateSnapshot {
        encoded_kbps: 0.0,
        decoded_fps: 0.0,
        sink_fps: 0.0,
    };

    while !stop.load(Ordering::SeqCst) {
        thread::sleep(VIDEO_LIVENESS_POLL_INTERVAL);

        let elapsed = last_rate_at.elapsed();
        if elapsed >= VIDEO_SINK_RATE_LOG_INTERVAL {
            let encoded_bytes_total = state.encoded_bytes_total.load(Ordering::Relaxed);
            let decoded_total = state.decoded_total.load(Ordering::Relaxed);
            let sink_total = state.sink_total.load(Ordering::Relaxed);
            let elapsed_secs = elapsed.as_secs_f64().max(0.001);
            let bitrate_kbps = encoded_bytes_total
                .saturating_sub(last_encoded_bytes_total)
                .saturating_mul(8) as f64
                / elapsed_secs
                / 1000.0;
            rates = VideoRateSnapshot {
                encoded_kbps: bitrate_kbps.max(0.0),
                decoded_fps: decoded_total.saturating_sub(last_decoded_total) as f64 / elapsed_secs,
                sink_fps: sink_total.saturating_sub(last_sink_total) as f64 / elapsed_secs,
            };
            update_native_stats_overlay(
                &sink,
                &state,
                rates.encoded_kbps.round() as u32,
                rates,
                decoded_total,
                sink_total,
            );
            emit_native_stats_event(
                &event_sender,
                &sink,
                &state,
                rates.encoded_kbps.round() as u32,
                rates,
                decoded_total,
                sink_total,
            );
            last_encoded_bytes_total = encoded_bytes_total;
            last_decoded_total = decoded_total;
            last_sink_total = sink_total;
            last_rate_at = Instant::now();
        }

        let last_sink_ms = state.last_sink_ms.load(Ordering::Relaxed);
        if last_sink_ms == 0 {
            maybe_recover_video_startup(&state, &pipeline, &event_sender);
            continue;
        }

        let now_ms = state.now_ms();
        let encoded_age_ms = age_since_ms(now_ms, state.last_encoded_ms.load(Ordering::Relaxed));
        let decoded_age_ms = age_since_ms(now_ms, state.last_decoded_ms.load(Ordering::Relaxed));
        let sink_age_ms = age_since_ms(now_ms, last_sink_ms);
        let likely_stage = classify_video_stall(encoded_age_ms, decoded_age_ms, sink_age_ms);
        let transition_stall = likely_stage == "decode-chain-stalled"
            && encoded_age_ms.is_some_and(|age| age <= 1_000);

        match tracker.evaluate(now_ms, last_sink_ms) {
            VideoStallAction::None => {}
            VideoStallAction::RequestKeyframe { attempt, stall_ms } => {
                request_upstream_key_unit(&state, &event_sender);
                emit_video_stall_event(
                    &event_sender,
                    &sink,
                    &state,
                    rates,
                    attempt,
                    stall_ms,
                    false,
                );
            }
            VideoStallAction::Resync { attempt, stall_ms } => {
                request_upstream_key_unit(&state, &event_sender);
                emit_video_stall_event(
                    &event_sender,
                    &sink,
                    &state,
                    rates,
                    attempt,
                    stall_ms,
                    true,
                );
                match pipeline.recalculate_latency() {
                    Ok(()) => send_log(
                        &event_sender,
                        "warn",
                        "Requested GStreamer latency recalculation after native video stall.".to_owned(),
                    ),
                    Err(error) => send_log(
                        &event_sender,
                        "warn",
                        format!(
                            "Failed to request GStreamer latency recalculation after native video stall: {error}."
                        ),
                    ),
                }
            }
            VideoStallAction::PartialFlush { attempt, stall_ms } => {
                if transition_stall && state.transition_flush_escalation_enabled() {
                    request_upstream_key_unit(&state, &event_sender);
                    perform_transition_flush(&state, &event_sender, TransitionFlushKind::Partial);
                }
                emit_video_stall_event(
                    &event_sender,
                    &sink,
                    &state,
                    rates,
                    attempt,
                    stall_ms,
                    false,
                );
            }
            VideoStallAction::CompleteFlush { attempt, stall_ms } => {
                if transition_stall && state.transition_flush_escalation_enabled() {
                    request_upstream_key_unit(&state, &event_sender);
                    perform_transition_flush(&state, &event_sender, TransitionFlushKind::Complete);
                }
                emit_video_stall_event(
                    &event_sender,
                    &sink,
                    &state,
                    rates,
                    attempt,
                    stall_ms,
                    false,
                );
            }
            VideoStallAction::Fatal { attempt, stall_ms } => {
                emit_video_stall_event(
                    &event_sender,
                    &sink,
                    &state,
                    rates,
                    attempt,
                    stall_ms,
                    false,
                );
                send_log(
                    &event_sender,
                    "error",
                    format!(
                        "Native video stall recovery exhausted after {stall_ms}ms; stage={likely_stage} queueMode={} transitionFlushEscalation={}.",
                        state.queue_mode().as_str(),
                        state.transition_flush_escalation_enabled(),
                    ),
                );
                if let Some(event_sender) = &event_sender {
                    let _ = event_sender.send(Event::Error {
                        code: "native-video-stall-fatal".to_owned(),
                        message: format!(
                            "Native video stall recovery exhausted after {stall_ms}ms ({likely_stage})."
                        ),
                    });
                }
            }
            VideoStallAction::Recovered { stall_ms } => {
                if state.queue_depth() > VIDEO_QUEUE_MAX_BUFFERS {
                    state.set_queue_depth(
                        VIDEO_QUEUE_MAX_BUFFERS,
                        "transition recovery completed",
                        &event_sender,
                    );
                }
                send_log(
                    &event_sender,
                    "info",
                    format!("Native video recovered after {stall_ms} ms."),
                );
            }
        }
    }
}

fn maybe_recover_video_startup(
    state: &VideoLivenessState,
    pipeline: &gst::Pipeline,
    event_sender: &Option<Sender<Event>>,
) {
    let now_ms = state.now_ms();
    let last_audio_ms = state.last_audio_ms.load(Ordering::Relaxed);
    let first_audio_ms = state.first_startup_audio_ms.load(Ordering::Relaxed);
    let last_encoded_ms = state.last_encoded_ms.load(Ordering::Relaxed);
    if first_audio_ms == 0
        || last_audio_ms == 0
        || now_ms.saturating_sub(last_audio_ms) > VIDEO_STARTUP_KEYFRAME_MS
    {
        return;
    }
    let audio_active_ms = now_ms.saturating_sub(first_audio_ms);

    let decoded_total = state.decoded_total.load(Ordering::Relaxed);
    let sink_total = state.sink_total.load(Ordering::Relaxed);
    let encoded_age = if last_encoded_ms == 0 {
        "never".to_owned()
    } else {
        format!("{}ms", now_ms.saturating_sub(last_encoded_ms))
    };

    if audio_active_ms >= VIDEO_STARTUP_KEYFRAME_MS
        && !state
            .startup_keyframe_requested
            .swap(true, Ordering::Relaxed)
    {
        send_log(
            event_sender,
            "warn",
            format!(
                "Native video startup has no rendered frame after {audio_active_ms}ms of active audio; startupAge={now_ms}ms encodedAge={encoded_age} decoded={decoded_total} sink={sink_total}. Requesting keyframe."
            ),
        );
        request_upstream_key_unit(state, event_sender);
    }

    if audio_active_ms >= VIDEO_STARTUP_RESYNC_MS
        && !state.startup_resync_requested.swap(true, Ordering::Relaxed)
    {
        send_log(
            event_sender,
            "warn",
            format!(
                "Native video startup still has no rendered frame after {audio_active_ms}ms of active audio; startupAge={now_ms}ms encodedAge={encoded_age} decoded={decoded_total} sink={sink_total}. Requesting keyframe and GStreamer latency resync."
            ),
        );
        request_upstream_key_unit(state, event_sender);
        if let Err(error) = pipeline.recalculate_latency() {
            send_log(
                event_sender,
                "warn",
                format!("Failed to resync GStreamer latency during native video startup recovery: {error}."),
            );
        }
    }

    if audio_active_ms >= VIDEO_STARTUP_FATAL_MS
        && !state.startup_fatal_reported.swap(true, Ordering::Relaxed)
    {
        send_log(
            event_sender,
            "error",
            format!(
                "Native video startup still has no rendered frame after {audio_active_ms}ms of active audio; startupAge={now_ms}ms encodedAge={encoded_age} decoded={decoded_total} sink={sink_total}. Treating startup as failed instead of restarting the WebRTC pipeline."
            ),
        );
        request_upstream_key_unit(state, event_sender);
        if let Some(event_sender) = event_sender {
            let _ = event_sender.send(Event::Error {
                code: "native-video-startup-timeout".to_owned(),
                message: "Native video startup timed out before the first rendered frame."
                    .to_owned(),
            });
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransitionFlushKind {
    Partial,
    Complete,
}

fn perform_transition_flush(
    state: &VideoLivenessState,
    event_sender: &Option<Sender<Event>>,
    flush_kind: TransitionFlushKind,
) {
    let label = match flush_kind {
        TransitionFlushKind::Partial => "partial",
        TransitionFlushKind::Complete => "complete",
    };
    let mut flushed = Vec::new();

    if matches!(
        flush_kind,
        TransitionFlushKind::Partial | TransitionFlushKind::Complete
    ) {
        if let Some(queue) = state.pre_decode_queue() {
            flush_element(&queue);
            flushed.push("pre-decode queue");
        }
    }

    if matches!(flush_kind, TransitionFlushKind::Complete) {
        if let Some(decoder) = state.decoder() {
            flush_element(&decoder);
            flushed.push("decoder");
        }
    }

    if let Some(queue) = state
        .post_decode_queue
        .lock()
        .ok()
        .and_then(|current| current.clone())
    {
        flush_element(&queue);
        flushed.push("post-decode queue");
    }

    if flushed.is_empty() {
        send_log(
            event_sender,
            "warn",
            "Cannot flush native transition path because no video branch elements are registered."
                .to_owned(),
        );
        return;
    }

    match flush_kind {
        TransitionFlushKind::Partial => {
            state.increment_partial_flush_count();
            state.set_queue_depth(2, "transition partial flush", event_sender);
        }
        TransitionFlushKind::Complete => {
            state.increment_complete_flush_count();
            state.set_queue_depth(2, "transition complete flush", event_sender);
        }
    }

    send_log(
        event_sender,
        "warn",
        format!(
            "Performed {label} native transition flush on {}.",
            flushed.join(", ")
        ),
    );
}

fn flush_element(element: &gst::Element) {
    let _ = element.send_event(gst::event::FlushStart::new());
    let _ = element.send_event(gst::event::FlushStop::new(false));
}

fn request_upstream_key_unit(state: &VideoLivenessState, event_sender: &Option<Sender<Event>>) {
    let Some(src_pad) = state.rtp_video_src_pad() else {
        send_log(
            event_sender,
            "warn",
            "Unable to request upstream video key unit: no RTP video source pad registered."
                .to_owned(),
        );
        return;
    };

    let event = gst::event::CustomUpstream::builder(
        gst::Structure::builder("GstForceKeyUnit")
            .field("all-headers", true)
            .build(),
    )
    .build();

    if src_pad.send_event(event) {
        send_log(
            event_sender,
            "debug",
            "Requested upstream video key unit via RTP source pad.".to_owned(),
        );
    } else {
        send_log(
            event_sender,
            "warn",
            "Upstream video key-unit request was not accepted by the RTP source pad.".to_owned(),
        );
    }
}

fn emit_native_stats_event(
    event_sender: &Option<Sender<Event>>,
    sink: &gst::Element,
    state: &VideoLivenessState,
    bitrate_kbps: u32,
    rates: VideoRateSnapshot,
    frames_decoded: u64,
    frames_rendered: u64,
) {
    let Some(event_sender) = event_sender else {
        return;
    };

    let target_bitrate_kbps = state.target_bitrate_kbps.load(Ordering::Relaxed);
    let bitrate_performance_percent = if target_bitrate_kbps > 0 {
        (f64::from(bitrate_kbps) / f64::from(target_bitrate_kbps)) * 100.0
    } else {
        0.0
    };
    let codec = state
        .codec
        .lock()
        .map(|codec| codec.clone())
        .unwrap_or_default();
    let resolution = state
        .resolution
        .lock()
        .map(|resolution| resolution.clone())
        .unwrap_or_default();
    let hardware_acceleration = state
        .hardware_acceleration
        .lock()
        .map(|value| value.clone())
        .unwrap_or_default();
    let sink_stats = read_sink_stats(sink);
    let telemetry = state.transition_telemetry_snapshot();
    let _ = event_sender.send(Event::Stats {
        stats: crate::protocol::NativeStatsEvent {
            codec,
            resolution,
            hardware_acceleration,
            requested_fps: state.requested_fps(),
            caps_framerate: state.caps_framerate(),
            bitrate_kbps,
            target_bitrate_kbps,
            bitrate_performance_percent,
            decoded_fps: rates.decoded_fps,
            render_fps: rates.sink_fps,
            frames_decoded,
            frames_rendered,
            frames_pending_to_present: frames_decoded.saturating_sub(frames_rendered),
            sink_rendered: sink_stats.rendered,
            sink_dropped: sink_stats.dropped,
            memory_mode: state.memory_mode(),
            zero_copy: state.zero_copy(),
            queue_mode: telemetry.queue_mode.as_str().to_owned(),
            queue_depth_changes: telemetry.queue_depth_changes,
            present_pacing_changes: telemetry.present_pacing_changes,
            partial_flush_count: telemetry.partial_flush_count,
            complete_flush_count: telemetry.complete_flush_count,
            last_transition_type: telemetry
                .last_transition
                .as_ref()
                .map(|transition| transition.transition_type.clone()),
            last_transition_at_ms: telemetry
                .last_transition
                .as_ref()
                .map(|transition| transition.at_ms),
            last_transition_summary: telemetry
                .last_transition
                .as_ref()
                .map(|transition| transition.summary.clone()),
            requested_streaming_features_summary: state.requested_streaming_features_summary(),
            finalized_streaming_features_summary: state.finalized_streaming_features_summary(),
            zero_copy_d3d11: state.zero_copy_d3d11(),
            zero_copy_d3d12: state.zero_copy_d3d12(),
        },
    });
}

fn update_native_stats_overlay(
    sink: &gst::Element,
    state: &VideoLivenessState,
    bitrate_kbps: u32,
    rates: VideoRateSnapshot,
    _frames_decoded: u64,
    frames_rendered: u64,
) {
    let target_bitrate_kbps = state.target_bitrate_kbps.load(Ordering::Relaxed);
    let bitrate_performance_percent = if target_bitrate_kbps > 0 {
        (f64::from(bitrate_kbps) / f64::from(target_bitrate_kbps)) * 100.0
    } else {
        0.0
    };
    let codec = state
        .codec
        .lock()
        .map(|codec| codec.clone())
        .unwrap_or_default();
    let resolution = state
        .resolution
        .lock()
        .map(|resolution| resolution.clone())
        .unwrap_or_default();
    let hardware_acceleration = state
        .hardware_acceleration
        .lock()
        .map(|value| value.clone())
        .unwrap_or_default();
    let sink_stats = read_sink_stats(sink);
    let sink_dropped = sink_stats.dropped.unwrap_or(0);
    let sink_rendered = sink_stats.rendered.unwrap_or(frames_rendered);
    let sink_total = sink_rendered.saturating_add(sink_dropped);
    let drop_percent = if sink_total > 0 {
        (sink_dropped as f64 / sink_total as f64) * 100.0
    } else {
        0.0
    };
    let target_mbps = f64::from(target_bitrate_kbps) / 1000.0;
    let bitrate_mbps = f64::from(bitrate_kbps) / 1000.0;
    let memory_mode = state.memory_mode();
    let memory_path = if state.zero_copy() {
        format!("{memory_mode} zero-copy")
    } else {
        memory_mode
    };
    let text = format!(
        "{} {}  {:.1}/{:.1} Mbps  Bit {:.0}%\nDecode {:.0}fps  Render {:.0}fps  Drop {:.2}%  {}",
        codec,
        resolution,
        bitrate_mbps,
        target_mbps,
        bitrate_performance_percent,
        rates.decoded_fps,
        rates.sink_fps,
        drop_percent,
        if hardware_acceleration.is_empty() {
            memory_path
        } else {
            format!("{hardware_acceleration} {memory_path}")
        },
    );
    state.update_stats_overlay_text(&text);
}

fn emit_video_stall_event(
    event_sender: &Option<Sender<Event>>,
    sink: &gst::Element,
    state: &VideoLivenessState,
    rates: VideoRateSnapshot,
    recovery_attempt: u8,
    stall_ms: u64,
    will_resync: bool,
) {
    let stats = read_sink_stats(sink);
    let now_ms = state.now_ms();
    let last_encoded_ms = state.last_encoded_ms.load(Ordering::Relaxed);
    let last_decoded_ms = state.last_decoded_ms.load(Ordering::Relaxed);
    let last_sink_ms = state.last_sink_ms.load(Ordering::Relaxed);
    let encoded_age_ms = age_since_ms(now_ms, last_encoded_ms);
    let decoded_age_ms = age_since_ms(now_ms, last_decoded_ms);
    let sink_age_ms = age_since_ms(now_ms, last_sink_ms);
    let likely_stage = classify_video_stall(encoded_age_ms, decoded_age_ms, sink_age_ms);
    let memory_mode = state.memory_mode();
    let zero_copy = state.zero_copy();
    let telemetry = state.transition_telemetry_snapshot();
    let resync_suffix = if will_resync {
        " Requesting keyframe and resyncing GStreamer latency."
    } else {
        " Requesting keyframe."
    };
    send_log(
        event_sender,
        "warn",
        format!(
            "Native video stall detected: stall={stall_ms}ms stage={likely_stage} encoded={:.0}kbps decoded={:.1}fps sink={:.1}fps requestedFps={} capsFramerate={} queueMode={} partialFlushes={} completeFlushes={} lastTransition={} ages=encoded:{} decoded:{} sink:{} rendered={} dropped={} memoryMode={} zeroCopy={} zeroCopyD3D11={} zeroCopyD3D12={}. If decoded/sink/rendered counters are still flowing but the visible frame is stale, suspect a server-driven mid-stream transition the native decode/present chain failed to absorb rather than pure RTP loss.{}",
            rates.encoded_kbps,
            rates.decoded_fps,
            rates.sink_fps,
            state
                .requested_fps()
                .map(|value| value.to_string())
                .unwrap_or_else(|| "n/a".to_owned()),
            state.caps_framerate().unwrap_or_else(|| "unknown".to_owned()),
            telemetry.queue_mode.as_str(),
            telemetry.partial_flush_count,
            telemetry.complete_flush_count,
            telemetry
                .last_transition
                .as_ref()
                .map(|transition| transition.transition_type.as_str())
                .unwrap_or("none"),
            format_age_ms(encoded_age_ms),
            format_age_ms(decoded_age_ms),
            format_age_ms(sink_age_ms),
            stats
                .rendered
                .map(|value| value.to_string())
                .unwrap_or_else(|| "n/a".to_owned()),
            stats
                .dropped
                .map(|value| value.to_string())
                .unwrap_or_else(|| "n/a".to_owned()),
            memory_mode.as_str(),
            zero_copy,
            state.zero_copy_d3d11(),
            state.zero_copy_d3d12(),
            resync_suffix
        ),
    );
    if let Some(event_sender) = event_sender {
        let _ = event_sender.send(Event::VideoStall(VideoStallEvent {
            stall_ms,
            encoded_kbps: rates.encoded_kbps,
            decoded_fps: rates.decoded_fps,
            sink_fps: rates.sink_fps,
            encoded_age_ms,
            decoded_age_ms,
            sink_age_ms,
            likely_stage: likely_stage.to_owned(),
            sink_rendered: stats.rendered,
            sink_dropped: stats.dropped,
            memory_mode,
            zero_copy,
            requested_fps: state.requested_fps(),
            caps_framerate: state.caps_framerate(),
            queue_mode: telemetry.queue_mode.as_str().to_owned(),
            partial_flush_count: telemetry.partial_flush_count,
            complete_flush_count: telemetry.complete_flush_count,
            last_transition_type: telemetry
                .last_transition
                .as_ref()
                .map(|transition| transition.transition_type.clone()),
            last_transition_at_ms: telemetry
                .last_transition
                .as_ref()
                .map(|transition| transition.at_ms),
            requested_streaming_features_summary: state.requested_streaming_features_summary(),
            finalized_streaming_features_summary: state.finalized_streaming_features_summary(),
            zero_copy_d3d11: state.zero_copy_d3d11(),
            zero_copy_d3d12: state.zero_copy_d3d12(),
            recovery_attempt,
        }));
    }
}

fn age_since_ms(now_ms: u64, last_ms: u64) -> Option<u64> {
    (last_ms != 0).then_some(now_ms.saturating_sub(last_ms))
}

fn format_age_ms(age_ms: Option<u64>) -> String {
    age_ms
        .map(|value| format!("{value}ms"))
        .unwrap_or_else(|| "n/a".to_owned())
}

fn classify_video_stall(
    encoded_age_ms: Option<u64>,
    decoded_age_ms: Option<u64>,
    sink_age_ms: Option<u64>,
) -> &'static str {
    const ACTIVE_RECENT_MS: u64 = 1_000;
    match (encoded_age_ms, decoded_age_ms, sink_age_ms) {
        (Some(encoded), _, _) if encoded > VIDEO_STALL_WARNING_MS => "video-rtp-idle",
        (Some(encoded), Some(decoded), _)
            if encoded <= ACTIVE_RECENT_MS && decoded > VIDEO_STALL_WARNING_MS =>
        {
            "decode-chain-stalled"
        }
        (_, Some(decoded), Some(sink))
            if decoded <= ACTIVE_RECENT_MS && sink > VIDEO_STALL_WARNING_MS =>
        {
            "present-chain-stalled"
        }
        (None, _, _) => "video-rtp-not-observed",
        _ => "video-output-stalled",
    }
}

fn start_gstreamer_bus_diagnostics(
    pipeline: &gst::Pipeline,
    event_sender: Option<Sender<Event>>,
    stop: Arc<AtomicBool>,
    video_liveness: VideoLivenessMonitor,
) {
    let Some(bus) = pipeline.bus() else {
        send_log(
            &event_sender,
            "warn",
            "GStreamer pipeline has no bus; native diagnostics will be limited.".to_owned(),
        );
        return;
    };

    thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            let Some(message) = bus.timed_pop_filtered(
                gst::ClockTime::from_mseconds(250),
                &[
                    gst::MessageType::Error,
                    gst::MessageType::Warning,
                    gst::MessageType::Qos,
                    gst::MessageType::Latency,
                    gst::MessageType::StateChanged,
                    gst::MessageType::Eos,
                ],
            ) else {
                continue;
            };

            match message.view() {
                gst::MessageView::Error(error) => send_log(
                    &event_sender,
                    "error",
                    format!(
                        "GStreamer bus error from {}: {}; debug={:?}.",
                        message_src_name(&message),
                        error.error(),
                        error.debug()
                    ),
                ),
                gst::MessageView::Warning(warning) => send_log(
                    &event_sender,
                    "warn",
                    format!(
                        "GStreamer bus warning from {}: {}; debug={:?}.",
                        message_src_name(&message),
                        warning.error(),
                        warning.debug()
                    ),
                ),
                gst::MessageView::Qos(_) => send_log(
                    &event_sender,
                    "debug",
                    format!(
                        "GStreamer bus QoS from {}: {}.",
                        message_src_name(&message),
                        message_structure_summary(&message)
                    ),
                ),
                gst::MessageView::Latency(_) => send_log(
                    &event_sender,
                    "debug",
                    format!(
                        "GStreamer bus latency update from {}.",
                        message_src_name(&message)
                    ),
                ),
                gst::MessageView::StateChanged(state) => {
                    if message
                        .src()
                        .and_then(|src| src.clone().downcast::<gst::Pipeline>().ok())
                        .is_some()
                    {
                        send_log(
                            &event_sender,
                            "debug",
                            format!(
                                "GStreamer pipeline state changed: {:?} -> {:?} pending {:?}.",
                                state.old(),
                                state.current(),
                                state.pending()
                            ),
                        );
                        video_liveness.state.record_transition(
                            "pipeline-state-change",
                            "pipeline",
                            Some(format!("{:?}", state.old())),
                            Some(format!("{:?}", state.current())),
                            None,
                            None,
                            None,
                            None,
                            &event_sender,
                        );
                    }
                }
                gst::MessageView::Eos(_) => send_log(
                    &event_sender,
                    "warn",
                    format!("GStreamer bus EOS from {}.", message_src_name(&message)),
                ),
                _ => {}
            }
        }
    });
}

fn message_src_name(message: &gst::Message) -> String {
    message
        .src()
        .map(|src| src.path_string().to_string())
        .unwrap_or_else(|| "unknown".to_owned())
}

fn message_structure_summary(message: &gst::Message) -> String {
    message
        .structure()
        .map(|structure| structure.to_string())
        .unwrap_or_else(|| "no structure".to_owned())
}

fn resolve_present_max_fps(requested_fps: u32) -> u32 {
    if let Ok(value) = std::env::var(NATIVE_PRESENT_MAX_FPS_ENV) {
        let value = value.trim().to_ascii_lowercase();
        if value == "0" || value == "off" || value == "false" || value == "unlimited" {
            return 0;
        }
        if value == "auto" {
            return PRESENT_LIMITER_AUTO_SENTINEL;
        }
        if let Ok(fps) = value.parse::<u32>() {
            return fps;
        }
    }
    let _ = requested_fps;
    PRESENT_LIMITER_AUTO_SENTINEL
}

fn automatic_present_max_fps(requested_fps: u32, display_hz: Option<u32>) -> u32 {
    display_hz
        .filter(|display_hz| *display_hz >= 30 && *display_hz < requested_fps)
        .unwrap_or(0)
}

fn effective_present_max_fps(
    configured_present_max_fps: u32,
    requested_fps: Option<u32>,
    video_api: RtpVideoApi,
    display_hz: Option<u32>,
) -> u32 {
    if configured_present_max_fps != PRESENT_LIMITER_AUTO_SENTINEL {
        return configured_present_max_fps;
    }

    if !matches!(video_api, RtpVideoApi::D3D11) {
        return 0;
    }

    requested_fps
        .filter(|fps| *fps > 0)
        .map(|fps| automatic_present_max_fps(fps, display_hz))
        .unwrap_or(0)
}

fn resolve_d3d_fullscreen_sink(cloud_gsync_enabled: bool) -> bool {
    if let Ok(value) = std::env::var(NATIVE_D3D_FULLSCREEN_ENV) {
        let value = value.trim().to_ascii_lowercase();
        if value == "1" || value == "on" || value == "true" || value == "yes" {
            return true;
        }
        if value == "0" || value == "off" || value == "false" || value == "no" {
            return false;
        }
    }

    cloud_gsync_enabled
}

#[cfg(target_os = "windows")]
fn primary_display_refresh_hz() -> Option<u32> {
    const VREFRESH: i32 = 116;

    #[link(name = "user32")]
    extern "system" {
        fn GetDC(hwnd: *mut c_void) -> *mut c_void;
        fn ReleaseDC(hwnd: *mut c_void, hdc: *mut c_void) -> i32;
    }

    #[link(name = "gdi32")]
    extern "system" {
        fn GetDeviceCaps(hdc: *mut c_void, index: i32) -> i32;
    }

    let hdc = unsafe { GetDC(std::ptr::null_mut()) };
    if hdc.is_null() {
        return None;
    }

    let refresh = unsafe { GetDeviceCaps(hdc, VREFRESH) };
    unsafe {
        ReleaseDC(std::ptr::null_mut(), hdc);
    }

    (refresh > 1).then_some(refresh as u32)
}

#[cfg(not(target_os = "windows"))]
fn primary_display_refresh_hz() -> Option<u32> {
    None
}

fn wire_incoming_media_sink(
    pipeline: &gst::Pipeline,
    webrtc: &gst::Element,
    event_sender: Option<Sender<Event>>,
    render_state: GstreamerRenderState,
    present_max_fps: Arc<AtomicU32>,
    d3d_fullscreen_sink: Arc<AtomicBool>,
    video_liveness: VideoLivenessMonitor,
) {
    let pipeline = pipeline.downgrade();
    let streaming_reported = Arc::new(AtomicBool::new(false));
    webrtc.connect_pad_added(move |_webrtc, src_pad| {
        let Some(pipeline) = pipeline.upgrade() else {
            return;
        };
        let event_sender = event_sender.clone();

        if !is_rtp_pad(src_pad) {
            send_log(
                &event_sender,
                "debug",
                format!(
                    "Ignoring non-RTP WebRTC pad with caps {:?}.",
                    pad_caps_name(src_pad)
                ),
            );
            return;
        }

        if let Some(encoding) = rtp_video_encoding(src_pad) {
            match link_rtp_video_pad(
                &pipeline,
                src_pad,
                &encoding,
                &render_state,
                &event_sender,
                &streaming_reported,
                present_max_fps.clone(),
                d3d_fullscreen_sink.load(Ordering::SeqCst),
                video_liveness.clone(),
            ) {
                Ok(()) => return,
                Err(error) => send_log(
                    &event_sender,
                    "warn",
                    format!("{error}; falling back to decodebin."),
                ),
            }
        }

        let decodebin = match make_element("decodebin") {
            Ok(decodebin) => decodebin,
            Err(error) => {
                send_log(&event_sender, "warn", error);
                return;
            }
        };

        let decode_pipeline = pipeline.downgrade();
        let decode_sender = event_sender.clone();
        let decode_render_state = render_state.clone();
        let decode_streaming_reported = streaming_reported.clone();
        let decode_video_liveness = video_liveness.clone();
        decodebin.connect_pad_added(move |_decodebin, decoded_pad| {
            let Some(pipeline) = decode_pipeline.upgrade() else {
                return;
            };
            let media_kind = decoded_media_kind(decoded_pad);
            if let Err(error) = link_decoded_media_pad(
                &pipeline,
                decoded_pad,
                &decode_render_state,
                &decode_sender,
                &decode_streaming_reported,
                &decode_video_liveness,
            ) {
                send_log(&decode_sender, "warn", error);
                if let Err(fallback_error) =
                    link_decoded_media_to_fakesink(&pipeline, decoded_pad, "decoded media fallback")
                {
                    send_log(&decode_sender, "warn", fallback_error);
                }
                return;
            }

            send_log(
                &decode_sender,
                "info",
                format!(
                    "Linked decoded {} stream to native sink chain.",
                    media_kind.label()
                ),
            );
        });

        if let Err(error) = pipeline.add(&decodebin) {
            send_log(
                &event_sender,
                "warn",
                format!("Failed to add decodebin: {error}"),
            );
            return;
        }
        if let Err(error) = decodebin.sync_state_with_parent() {
            send_log(
                &event_sender,
                "warn",
                format!("Failed to sync decodebin state: {error}"),
            );
            return;
        }

        let Some(sink_pad) = decodebin.static_pad("sink") else {
            send_log(
                &event_sender,
                "warn",
                "decodebin has no sink pad.".to_owned(),
            );
            return;
        };
        if let Err(error) = src_pad.link(&sink_pad) {
            send_log(
                &event_sender,
                "warn",
                format!("Failed to link WebRTC RTP pad to decodebin: {error:?}"),
            );
        } else if rtp_video_encoding(src_pad).is_some() {
            video_liveness.set_rtp_video_src_pad(src_pad);
        }
    });
}

impl DecodedMediaKind {
    fn label(self) -> &'static str {
        match self {
            Self::Audio => "audio",
            Self::Video => "video",
            Self::Unknown => "unknown",
        }
    }
}

fn is_rtp_pad(pad: &gst::Pad) -> bool {
    pad_caps_name(pad)
        .as_deref()
        .is_some_and(|name| name == "application/x-rtp")
}

fn pad_caps_name(pad: &gst::Pad) -> Option<String> {
    let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
    caps.structure(0)
        .map(|structure| structure.name().to_string())
}

fn decoded_media_kind(pad: &gst::Pad) -> DecodedMediaKind {
    match pad_caps_name(pad).as_deref() {
        Some(name) if name.starts_with("video/") => DecodedMediaKind::Video,
        Some(name) if name.starts_with("audio/") => DecodedMediaKind::Audio,
        _ => DecodedMediaKind::Unknown,
    }
}

fn rtp_video_encoding(pad: &gst::Pad) -> Option<String> {
    let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
    let structure = caps.structure(0)?;
    if structure.name() != "application/x-rtp" {
        return None;
    }

    let media = structure.get::<String>("media").ok()?;
    if media != "video" {
        return None;
    }

    structure
        .get::<String>("encoding-name")
        .ok()
        .map(|encoding| encoding.to_ascii_uppercase())
}

fn rtp_video_depayloader_factory(codec: &str) -> Option<&'static str> {
    match codec {
        "H265" | "HEVC" => Some("rtph265depay"),
        "H264" => Some("rtph264depay"),
        "AV1" => Some("rtpav1depay"),
        _ => None,
    }
}

fn rtp_video_parser_factory(codec: &str) -> Option<&'static str> {
    match codec {
        "H265" | "HEVC" => Some("h265parse"),
        "H264" => Some("h264parse"),
        "AV1" => Some("av1parse"),
        _ => None,
    }
}

fn rtp_video_chain_definition(
    encoding: &str,
    video_api: RtpVideoApi,
) -> Option<Vec<RtpVideoChainSpec>> {
    let codec = encoding.to_ascii_uppercase();
    let mut specs = vec![
        RtpVideoChainSpec::new(
            rtp_video_depayloader_factory(codec.as_str())?,
            RtpVideoChainRole::Depayloader,
        ),
        RtpVideoChainSpec::new(
            rtp_video_parser_factory(codec.as_str())?,
            RtpVideoChainRole::Parser,
        ),
        RtpVideoChainSpec::new("queue", RtpVideoChainRole::PreDecodeQueue),
        RtpVideoChainSpec::new(
            video_api.decoder_factory(codec.as_str())?,
            RtpVideoChainRole::Decoder,
        ),
    ];

    if let Some(memory_caps) = video_api.memory_caps() {
        specs.push(RtpVideoChainSpec::with_caps(
            "capsfilter",
            RtpVideoChainRole::PostDecodeCapsFilter,
            memory_caps,
        ));
    }
    if let Some(converter) = video_api.post_decode_converter_factory() {
        specs.push(RtpVideoChainSpec::new(
            converter,
            RtpVideoChainRole::PostDecodeConverter,
        ));
    }
    if let Some(overlay) = video_api.stats_overlay_factory() {
        specs.push(RtpVideoChainSpec::new(
            overlay,
            RtpVideoChainRole::StatsOverlay,
        ));
    }
    specs.push(RtpVideoChainSpec::new(
        "queue",
        RtpVideoChainRole::PostDecodeQueue,
    ));
    specs.push(RtpVideoChainSpec::new(
        video_api.sink_factory(),
        RtpVideoChainRole::Sink,
    ));

    Some(specs)
}

fn preferred_rtp_video_apis(requested_fps: Option<u32>) -> Vec<RtpVideoApi> {
    let requested = std::env::var(NATIVE_VIDEO_BACKEND_ENV)
        .or_else(|_| std::env::var(NATIVE_VIDEO_API_ENV))
        .unwrap_or_else(|_| "auto".to_owned())
        .to_ascii_lowercase();
    match requested.as_str() {
        "d3d11" => vec![RtpVideoApi::D3D11],
        "d3d12" => vec![RtpVideoApi::D3D12],
        "videotoolbox" | "vt" => vec![RtpVideoApi::VideoToolbox],
        "vaapi" | "va" => vec![RtpVideoApi::Vaapi],
        "v4l2" | "v4l2stateless" => vec![RtpVideoApi::V4L2],
        "vulkan" | "vk" => vec![RtpVideoApi::Vulkan],
        "software" | "sw" => vec![RtpVideoApi::Software],
        _ => default_rtp_video_api_priority(requested_fps),
    }
}

fn zero_copy_requested() -> bool {
    matches!(
        std::env::var(NATIVE_ZERO_COPY_ENV)
            .unwrap_or_else(|_| "auto".to_owned())
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "forced"
    )
}

fn default_rtp_video_api_priority(requested_fps: Option<u32>) -> Vec<RtpVideoApi> {
    #[cfg(target_os = "windows")]
    {
        if should_prefer_d3d12_for_high_fps(requested_fps) {
            return vec![
                RtpVideoApi::D3D12,
                RtpVideoApi::D3D11,
                RtpVideoApi::Software,
            ];
        }
        vec![
            RtpVideoApi::D3D11,
            RtpVideoApi::D3D12,
            RtpVideoApi::Software,
        ]
    }
    #[cfg(target_os = "macos")]
    {
        let _ = requested_fps;
        vec![RtpVideoApi::VideoToolbox, RtpVideoApi::Software]
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        let _ = requested_fps;
        vec![
            RtpVideoApi::V4L2,
            RtpVideoApi::Vaapi,
            RtpVideoApi::Vulkan,
            RtpVideoApi::Software,
        ]
    }
    #[cfg(all(target_os = "linux", not(target_arch = "aarch64")))]
    {
        let _ = requested_fps;
        vec![
            RtpVideoApi::Vaapi,
            RtpVideoApi::Vulkan,
            RtpVideoApi::V4L2,
            RtpVideoApi::Software,
        ]
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = requested_fps;
        vec![RtpVideoApi::Software]
    }
}

fn should_prefer_d3d12_for_high_fps(requested_fps: Option<u32>) -> bool {
    requested_fps.is_some_and(|fps| fps >= 200)
}

fn rtp_video_chain_specs(
    encoding: &str,
    requested_fps: Option<u32>,
) -> Option<(RtpVideoApi, Vec<RtpVideoChainSpec>)> {
    preferred_rtp_video_apis(requested_fps)
        .into_iter()
        .find_map(|video_api| {
            let codec = encoding.to_ascii_uppercase();
            let decoder = select_decoder_factory(video_api, codec.as_str())?;
            let sink = select_sink_factory(video_api)?;
            let mut specs = rtp_video_chain_definition(encoding, video_api)?;
            for spec in &mut specs {
                if spec.role == RtpVideoChainRole::Decoder {
                    spec.factory = decoder;
                } else if spec.role == RtpVideoChainRole::Sink {
                    spec.factory = sink;
                }
            }
            insert_requested_fps_capssetter(&mut specs, requested_fps);
            specs.retain(|spec| {
                spec.role != RtpVideoChainRole::StatsOverlay
                    || gst::ElementFactory::find(spec.factory).is_some()
            });
            required_video_chain_elements_available(&specs).then_some((video_api, specs))
        })
}

fn insert_requested_fps_capssetter(specs: &mut Vec<RtpVideoChainSpec>, requested_fps: Option<u32>) {
    let Some(fps) = requested_fps.filter(|fps| *fps > 0) else {
        return;
    };
    if gst::ElementFactory::find("capssetter").is_none() {
        return;
    }
    let Some(decoder_index) = specs
        .iter()
        .position(|spec| spec.role == RtpVideoChainRole::Decoder)
    else {
        return;
    };

    specs.insert(
        decoder_index + 1,
        RtpVideoChainSpec::with_caps(
            "capssetter",
            RtpVideoChainRole::PostDecodeRateSetter,
            format!("video/x-raw,framerate=(fraction){fps}/1"),
        ),
    );
}

fn select_decoder_factory(video_api: RtpVideoApi, codec: &str) -> Option<&'static str> {
    let primary = video_api.decoder_factory(codec)?;
    std::iter::once(primary)
        .chain(video_api.fallback_decoder_factories(codec).iter().copied())
        .find(|factory| gst::ElementFactory::find(factory).is_some())
}

fn select_sink_factory(video_api: RtpVideoApi) -> Option<&'static str> {
    std::iter::once(video_api.sink_factory())
        .chain(video_api.sink_fallback_factories().iter().copied())
        .find(|factory| gst::ElementFactory::find(factory).is_some())
}

fn required_video_chain_elements_available(specs: &[RtpVideoChainSpec]) -> bool {
    specs
        .iter()
        .all(|spec| gst::ElementFactory::find(spec.factory).is_some())
}

fn all_rtp_video_apis() -> &'static [RtpVideoApi] {
    &[
        RtpVideoApi::D3D12,
        RtpVideoApi::D3D11,
        RtpVideoApi::VideoToolbox,
        RtpVideoApi::Vaapi,
        RtpVideoApi::V4L2,
        RtpVideoApi::Vulkan,
        RtpVideoApi::Software,
    ]
}

fn all_video_codec_labels() -> &'static [&'static str] {
    &["H264", "H265", "AV1"]
}

fn current_platform_label() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "windows"
    }
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(target_os = "linux")]
    {
        "linux"
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        "other"
    }
}

fn backend_runs_on_current_platform(video_api: RtpVideoApi) -> bool {
    video_api.platform() == current_platform_label() || video_api.platform() == "cross-platform"
}

fn native_video_backend_capabilities() -> Vec<NativeVideoBackendCapability> {
    all_rtp_video_apis()
        .iter()
        .copied()
        .map(native_video_backend_capability)
        .collect()
}

fn native_video_backend_capability(video_api: RtpVideoApi) -> NativeVideoBackendCapability {
    let platform_supported = backend_runs_on_current_platform(video_api);
    let sink_factory = platform_supported
        .then(|| select_sink_factory(video_api))
        .flatten();
    let codecs = all_video_codec_labels()
        .iter()
        .map(|codec| {
            native_video_codec_capability(video_api, codec, platform_supported, sink_factory)
        })
        .collect::<Vec<_>>();
    let available =
        platform_supported && sink_factory.is_some() && codecs.iter().any(|codec| codec.available);
    let reason = if !platform_supported {
        Some(format!(
            "{} is a {} backend and does not run on {}.",
            video_api.label(),
            video_api.platform(),
            current_platform_label()
        ))
    } else if sink_factory.is_none() {
        Some(format!(
            "{} sink is unavailable; install the platform GStreamer video sink plugins.",
            video_api.label()
        ))
    } else if !available {
        Some(format!(
            "{} decoders are unavailable for H.264, H.265, and AV1.",
            video_api.label()
        ))
    } else {
        None
    };

    NativeVideoBackendCapability {
        backend: video_api.capability_id().to_owned(),
        platform: video_api.platform().to_owned(),
        codecs,
        zero_copy_modes: zero_copy_modes_for_backend(video_api),
        sink: sink_factory.map(str::to_owned),
        available,
        reason,
    }
}

fn native_video_codec_capability(
    video_api: RtpVideoApi,
    codec: &str,
    platform_supported: bool,
    sink: Option<&'static str>,
) -> NativeVideoCodecCapability {
    let depayloader = rtp_video_depayloader_factory(codec);
    let parser = rtp_video_parser_factory(codec);
    let decoder = platform_supported
        .then(|| select_decoder_factory(video_api, codec))
        .flatten();
    let definition = rtp_video_chain_definition(codec, video_api);
    let available = platform_supported
        && sink.is_some()
        && decoder.is_some()
        && depayloader.is_some_and(|factory| gst::ElementFactory::find(factory).is_some())
        && parser.is_some_and(|factory| gst::ElementFactory::find(factory).is_some())
        && definition.is_some_and(|mut specs| {
            for spec in &mut specs {
                if spec.role == RtpVideoChainRole::Decoder {
                    if let Some(decoder) = decoder {
                        spec.factory = decoder;
                    }
                } else if spec.role == RtpVideoChainRole::Sink {
                    if let Some(sink) = sink {
                        spec.factory = sink;
                    }
                }
            }
            specs.retain(|spec| {
                spec.role != RtpVideoChainRole::StatsOverlay
                    || gst::ElementFactory::find(spec.factory).is_some()
            });
            required_video_chain_elements_available(&specs)
        });

    let reason = if !platform_supported {
        Some("Backend is not available on this platform.".to_owned())
    } else if depayloader.is_none() || parser.is_none() {
        Some("RTP depayloader or parser is not mapped for this codec.".to_owned())
    } else if decoder.is_none() {
        Some(format!(
            "{} decoder for {codec} is not installed.",
            video_api.label()
        ))
    } else if sink.is_none() {
        Some(format!(
            "{} video sink is not installed.",
            video_api.label()
        ))
    } else if !available {
        Some("Required GStreamer elements are not all available.".to_owned())
    } else {
        None
    };

    NativeVideoCodecCapability {
        codec: codec.to_ascii_lowercase(),
        available,
        decoder: decoder.map(str::to_owned),
        parser: parser.map(str::to_owned),
        depayloader: depayloader.map(str::to_owned),
        reason,
    }
}

fn zero_copy_modes_for_backend(video_api: RtpVideoApi) -> Vec<String> {
    match video_api {
        RtpVideoApi::D3D11 => vec!["D3D11Memory".to_owned()],
        RtpVideoApi::D3D12 => vec!["D3D12Memory".to_owned()],
        RtpVideoApi::VideoToolbox => vec!["GLMemory".to_owned()],
        RtpVideoApi::Vaapi => vec!["VAMemory".to_owned()],
        RtpVideoApi::Vulkan => vec!["VulkanImage".to_owned()],
        RtpVideoApi::V4L2 | RtpVideoApi::Software => Vec::new(),
    }
}

fn configure_rtp_video_chain_element(
    element: &gst::Element,
    spec: RtpVideoChainSpec,
    _video_api: RtpVideoApi,
    d3d_fullscreen_sink: bool,
) {
    match spec.role {
        RtpVideoChainRole::Depayloader => {
            set_property_if_supported(element, "request-keyframe", true);
            // Hard-waiting after packet loss can freeze the visible frame while RTP is still flowing.
            set_property_if_supported(element, "wait-for-keyframe", false);
        }
        RtpVideoChainRole::Parser => {
            set_property_if_supported(element, "disable-passthrough", true);
            set_property_if_supported(element, "config-interval", -1i32);
        }
        RtpVideoChainRole::PreDecodeQueue => {
            configure_queue(element, VIDEO_COMPRESSED_QUEUE_MAX_BUFFERS, false);
        }
        RtpVideoChainRole::Decoder => {
            set_property_if_supported(element, "automatic-request-sync-points", true);
            set_property_if_supported(element, "discard-corrupted-frames", true);
            set_property_if_supported(element, "min-force-key-unit-interval", 100_000_000u64);
            set_property_if_supported(element, "qos", false);
        }
        RtpVideoChainRole::PostDecodeRateSetter => {
            if let Some(caps) = spec
                .caps
                .as_deref()
                .and_then(|caps| caps.parse::<gst::Caps>().ok())
            {
                element.set_property("caps", &caps);
            }
            set_property_if_supported(element, "join", true);
            set_property_if_supported(element, "replace", false);
            set_property_if_supported(element, "qos", false);
        }
        RtpVideoChainRole::PostDecodeCapsFilter => {
            if let Some(caps) = spec
                .caps
                .as_deref()
                .and_then(|caps| caps.parse::<gst::Caps>().ok())
            {
                element.set_property("caps", &caps);
            }
        }
        RtpVideoChainRole::PostDecodeConverter => {
            set_property_if_supported(element, "qos", false);
        }
        RtpVideoChainRole::StatsOverlay => {
            configure_stats_overlay_element(element);
        }
        RtpVideoChainRole::PostDecodeQueue => {
            configure_queue_for_low_latency(element, "video");
        }
        RtpVideoChainRole::Sink => {
            configure_sink_for_low_latency(element);
            // Direct swapchain can turn a window/present stall into upstream decode backpressure.
            set_property_if_supported(element, "direct-swapchain", false);
            set_property_if_supported(element, "error-on-closed", false);
            set_property_if_supported(element, "fullscreen", d3d_fullscreen_sink);
            set_property_if_supported(element, "fullscreen-on-alt-enter", false);
            set_property_from_str_if_supported(element, "fullscreen-toggle-mode", "none");
        }
    }
}

fn link_rtp_video_pad(
    pipeline: &gst::Pipeline,
    src_pad: &gst::Pad,
    encoding: &str,
    render_state: &GstreamerRenderState,
    event_sender: &Option<Sender<Event>>,
    streaming_reported: &Arc<AtomicBool>,
    present_max_fps: Arc<AtomicU32>,
    d3d_fullscreen_sink: bool,
    video_liveness: VideoLivenessMonitor,
) -> Result<(), String> {
    if src_pad.is_linked() {
        return Ok(());
    }

    let requested_fps = video_liveness.state.requested_fps();
    let (video_api, specs) = rtp_video_chain_specs(encoding, requested_fps).ok_or_else(|| {
        format!(
            "Explicit low-latency decode chain is unavailable for RTP {encoding}; install the platform GStreamer plugin packages or set {NATIVE_VIDEO_BACKEND_ENV}=software to force software decode."
        )
    })?;
    video_liveness.update_hardware_acceleration(format!("GStreamer {}", video_api.label()));
    video_liveness.set_stats_overlay(None);
    let mut elements = Vec::with_capacity(specs.len());

    let result = (|| -> Result<(), String> {
        send_log(
            event_sender,
            "info",
            format_video_chain_selection(encoding, video_api, &specs),
        );
        if video_api == RtpVideoApi::D3D12 {
            send_log(
                event_sender,
                "info",
                format_d3d12_selection_summary(requested_fps),
            );
        }
        let configured_present_max_fps = present_max_fps.load(Ordering::SeqCst);
        let effective_present_max_fps = effective_present_max_fps(
            configured_present_max_fps,
            requested_fps,
            video_api,
            primary_display_refresh_hz(),
        );
        present_max_fps.store(effective_present_max_fps, Ordering::SeqCst);
        if effective_present_max_fps > 0 {
            let reason = if configured_present_max_fps == PRESENT_LIMITER_AUTO_SENTINEL {
                "auto-enabled for the D3D11 path to prevent display-rate present backpressure"
                    .to_owned()
            } else {
                format!("configured by {NATIVE_PRESENT_MAX_FPS_ENV}")
            };
            send_log(
                event_sender,
                "info",
                format!(
                    "Native present limiter enabled at {effective_present_max_fps} fps for {} video path; reason: {reason}.",
                    video_api.label()
                ),
            );
        }
        if d3d_fullscreen_sink {
            send_log(
                event_sender,
                "info",
                format!(
                    "Native D3D sink fullscreen presentation enabled for Cloud G-Sync/VRR; set {NATIVE_D3D_FULLSCREEN_ENV}=0 to disable."
                ),
            );
        }
        for spec in &specs {
            let element = make_element(spec.factory)?;
            configure_rtp_video_chain_element(
                &element,
                spec.clone(),
                video_api,
                d3d_fullscreen_sink,
            );
            if spec.role == RtpVideoChainRole::StatsOverlay {
                video_liveness.set_stats_overlay(Some(element.clone()));
            }
            pipeline.add(&element).map_err(|error| {
                format!(
                    "Failed to add {} for RTP {encoding} video chain: {error}",
                    spec.factory
                )
            })?;
            elements.push(element);
        }

        for pair in elements.windows(2) {
            pair[0].link(&pair[1]).map_err(|error| {
                format!(
                    "Failed to link {} -> {} for RTP {encoding} video chain: {error:?}",
                    element_factory_name(&pair[0]),
                    element_factory_name(&pair[1])
                )
            })?;
        }

        let first = elements
            .first()
            .ok_or_else(|| format!("No elements created for RTP {encoding} video chain."))?;
        let Some(first_sink_pad) = first.static_pad("sink") else {
            return Err(format!(
                "First RTP {encoding} video-chain element has no sink pad."
            ));
        };
        let sink = elements
            .last()
            .ok_or_else(|| format!("RTP {encoding} video chain has no sink element."))?;
        if let Some(post_decode_queue) =
            specs
                .iter()
                .zip(elements.iter())
                .find_map(|(spec, element)| {
                    (spec.role == RtpVideoChainRole::PostDecodeQueue).then_some(element)
                })
        {
            video_liveness.set_post_decode_queue(post_decode_queue.clone());
            watch_video_decoded_rate(
                post_decode_queue,
                event_sender,
                Some(video_liveness.clone()),
            );
        }
        if let Some(pre_decode_queue) =
            specs
                .iter()
                .zip(elements.iter())
                .find_map(|(spec, element)| {
                    (spec.role == RtpVideoChainRole::PreDecodeQueue).then_some(element)
                })
        {
            video_liveness.set_pre_decode_queue(pre_decode_queue.clone());
        }
        if let Some(parser) = specs
            .iter()
            .zip(elements.iter())
            .find_map(|(spec, element)| (spec.role == RtpVideoChainRole::Parser).then_some(element))
        {
            watch_video_caps_transitions(parser, "parser", event_sender, video_liveness.clone());
        }
        if let Some(decoder) = specs
            .iter()
            .zip(elements.iter())
            .find_map(|(spec, element)| {
                (spec.role == RtpVideoChainRole::Decoder).then_some(element)
            })
        {
            video_liveness.set_decoder(decoder.clone());
            watch_video_caps_transitions(decoder, "decoder", event_sender, video_liveness.clone());
        }
        render_state.set_video_sink(sink.clone(), event_sender);
        install_present_limiter(
            sink,
            present_max_fps,
            event_sender,
            Some(video_liveness.clone()),
        );
        watch_video_sink_caps_transitions(sink, event_sender, Some(video_liveness.clone()));
        watch_first_sink_buffer(sink, "video", event_sender, streaming_reported);
        watch_video_sink_rate(sink, event_sender, Some(video_liveness.clone()));

        for element in &elements {
            element.sync_state_with_parent().map_err(|error| {
                format!("Failed to sync RTP {encoding} video-chain element state: {error}")
            })?;
        }
        src_pad
            .link(&first_sink_pad)
            .map_err(|error| format!("Failed to link RTP {encoding} video pad: {error:?}"))?;
        video_liveness.set_rtp_video_src_pad(src_pad);
        watch_rtp_video_bitrate(src_pad, video_liveness.clone(), event_sender);
        video_liveness.start(pipeline.clone(), sink.clone(), event_sender.clone());

        Ok(())
    })();

    if result.is_err() {
        for element in &elements {
            let _ = element.set_state(gst::State::Null);
            let _ = pipeline.remove(element);
        }
    }

    result?;
    send_log(
        event_sender,
        "info",
        format!(
            "Linked RTP {encoding} video through explicit low-latency {} decode chain.",
            video_api.label()
        ),
    );
    Ok(())
}

fn format_video_chain_selection(
    encoding: &str,
    video_api: RtpVideoApi,
    specs: &[RtpVideoChainSpec],
) -> String {
    let decoder = specs
        .iter()
        .find(|spec| spec.role == RtpVideoChainRole::Decoder)
        .map(|spec| spec.factory)
        .unwrap_or("unknown");
    let sink = specs
        .iter()
        .find(|spec| spec.role == RtpVideoChainRole::Sink)
        .map(|spec| spec.factory)
        .unwrap_or("unknown");
    let converter = specs
        .iter()
        .find(|spec| spec.role == RtpVideoChainRole::PostDecodeConverter)
        .map(|spec| spec.factory)
        .unwrap_or("none");
    let memory = specs
        .iter()
        .find(|spec| spec.role == RtpVideoChainRole::PostDecodeCapsFilter)
        .and_then(|spec| spec.caps.as_deref())
        .unwrap_or(if video_api.is_gpu_path() {
            "auto-negotiated"
        } else {
            "system-memory"
        });
    let acceleration = if video_api.is_gpu_path() {
        "hardware"
    } else {
        "software"
    };

    format!(
        "Selected native {acceleration} video path for RTP {encoding}: backend={}, decoder={decoder}, converter={converter}, renderer={sink}, memory={memory}.",
        video_api.label()
    )
}

fn format_d3d12_selection_summary(requested_fps: Option<u32>) -> String {
    let backend_env = std::env::var(NATIVE_VIDEO_BACKEND_ENV).ok();
    let api_env = std::env::var(NATIVE_VIDEO_API_ENV).ok();
    let reason = if backend_env
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("d3d12"))
    {
        format!("forced by {NATIVE_VIDEO_BACKEND_ENV}=d3d12")
    } else if api_env
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("d3d12"))
    {
        format!("forced by {NATIVE_VIDEO_API_ENV}=d3d12")
    } else if should_prefer_d3d12_for_high_fps(requested_fps) {
        format!(
            "auto-selected for {} fps stream to avoid D3D11 display-rate present backpressure",
            requested_fps
                .map(|fps| fps.to_string())
                .unwrap_or_else(|| "high-FPS".to_owned())
        )
    } else {
        "D3D11 was unavailable/probe failed".to_owned()
    };

    format!(
        "Native D3D12 video path selected; reason: {reason}. env {NATIVE_VIDEO_BACKEND_ENV}={backend_env:?}, {NATIVE_VIDEO_API_ENV}={api_env:?}. If D3D12 stalls on a specific driver, force {NATIVE_VIDEO_BACKEND_ENV}=d3d11."
    )
}

fn element_factory_name(element: &gst::Element) -> String {
    element
        .factory()
        .map(|factory| factory.name().to_string())
        .unwrap_or_else(|| element.name().to_string())
}

fn link_decoded_media_pad(
    pipeline: &gst::Pipeline,
    src_pad: &gst::Pad,
    render_state: &GstreamerRenderState,
    event_sender: &Option<Sender<Event>>,
    streaming_reported: &Arc<AtomicBool>,
    video_liveness: &VideoLivenessMonitor,
) -> Result<(), String> {
    if src_pad.is_linked() {
        return Ok(());
    }

    match decoded_media_kind(src_pad) {
        DecodedMediaKind::Video => link_media_chain(
            pipeline,
            src_pad,
            &video_sink_factories(),
            "video",
            Some(render_state),
            event_sender,
            streaming_reported,
            Some(video_liveness),
        ),
        DecodedMediaKind::Audio => link_media_chain(
            pipeline,
            src_pad,
            &[
                ("queue", None),
                ("audioconvert", None),
                ("audioresample", None),
                ("autoaudiosink", Some(false)),
            ],
            "audio",
            None,
            event_sender,
            streaming_reported,
            None,
        ),
        DecodedMediaKind::Unknown => Err(format!(
            "Unsupported decoded media caps {:?}; routing to fallback sink.",
            pad_caps_name(src_pad)
        )),
    }
}

fn video_sink_factories() -> Vec<(&'static str, Option<bool>)> {
    #[cfg(target_os = "windows")]
    {
        if gst::ElementFactory::find("d3d11videosink").is_some() {
            let mut factories = vec![("queue", None)];
            if gst::ElementFactory::find("dwritetextoverlay").is_some() {
                factories.push(("dwritetextoverlay", None));
            }
            factories.push(("d3d11videosink", Some(false)));
            return factories;
        }
    }

    let mut factories = vec![("queue", None), ("videoconvert", None)];
    if gst::ElementFactory::find("dwritetextoverlay").is_some() {
        factories.push(("dwritetextoverlay", None));
    }
    factories.push(("autovideosink", Some(false)));
    factories
}

fn link_media_chain(
    pipeline: &gst::Pipeline,
    src_pad: &gst::Pad,
    factories: &[(&str, Option<bool>)],
    media_label: &str,
    render_state: Option<&GstreamerRenderState>,
    event_sender: &Option<Sender<Event>>,
    streaming_reported: &Arc<AtomicBool>,
    video_liveness: Option<&VideoLivenessMonitor>,
) -> Result<(), String> {
    if media_label == "video" {
        if let Some(video_liveness) = video_liveness {
            video_liveness.set_stats_overlay(None);
        }
    }

    let mut elements = Vec::with_capacity(factories.len());
    for (factory, sync_property) in factories {
        let factory = *factory;
        let element = make_element(factory)?;
        if factory == "queue" {
            configure_queue_for_low_latency(&element, media_label);
        }
        if factory == "dwritetextoverlay" {
            configure_stats_overlay_element(&element);
            if media_label == "video" {
                if let Some(video_liveness) = video_liveness {
                    video_liveness.set_stats_overlay(Some(element.clone()));
                }
            }
        }
        if sync_property.is_some() || factory.ends_with("sink") {
            configure_sink_for_low_latency(&element);
        }
        pipeline
            .add(&element)
            .map_err(|error| format!("Failed to add {factory} for {media_label}: {error}"))?;
        elements.push(element);
    }

    for pair in elements.windows(2) {
        pair[0].link(&pair[1]).map_err(|error| {
            format!(
                "Failed to link {} -> {} for {media_label}: {error:?}",
                pair[0]
                    .factory()
                    .map(|factory| factory.name())
                    .unwrap_or_default(),
                pair[1]
                    .factory()
                    .map(|factory| factory.name())
                    .unwrap_or_default()
            )
        })?;
    }

    let first = elements
        .first()
        .ok_or_else(|| format!("No elements created for {media_label} sink chain."))?;
    let Some(first_sink_pad) = first.static_pad("sink") else {
        return Err(format!(
            "First {media_label} sink-chain element has no sink pad."
        ));
    };
    src_pad
        .link(&first_sink_pad)
        .map_err(|error| format!("Failed to link decoded {media_label} pad: {error:?}"))?;

    if let Some(sink) = elements.last() {
        if media_label == "video" {
            if let Some(render_state) = render_state {
                render_state.set_video_sink(sink.clone(), event_sender);
            }
        }
        watch_first_sink_buffer(sink, media_label, event_sender, streaming_reported);
        if media_label == "audio" {
            if let Some(video_liveness) = video_liveness {
                watch_audio_activity(sink, video_liveness);
            }
        }
        if media_label == "video" {
            if let Some(video_liveness) = video_liveness {
                watch_video_sink_rate(sink, event_sender, Some(video_liveness.clone()));
                video_liveness.start(pipeline.clone(), sink.clone(), event_sender.clone());
            }
        }
    }

    for element in &elements {
        element.sync_state_with_parent().map_err(|error| {
            format!("Failed to sync {media_label} sink-chain element state: {error}")
        })?;
    }

    Ok(())
}

fn watch_audio_activity(sink: &gst::Element, video_liveness: &VideoLivenessMonitor) {
    let Some(sink_pad) = sink.static_pad("sink") else {
        return;
    };
    let monitor = video_liveness.clone();
    sink_pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
        monitor.record_audio_buffer();
        gst::PadProbeReturn::Ok
    });
}

fn watch_first_sink_buffer(
    sink: &gst::Element,
    media_label: &str,
    event_sender: &Option<Sender<Event>>,
    streaming_reported: &Arc<AtomicBool>,
) {
    let Some(sink_pad) = sink.static_pad("sink") else {
        return;
    };
    let sender = event_sender.clone();
    let label = media_label.to_owned();
    let reported = streaming_reported.clone();
    sink_pad.add_probe(gst::PadProbeType::BUFFER, move |pad, _info| {
        let caps = pad
            .current_caps()
            .map(|caps| caps.to_string())
            .unwrap_or_else(|| "unknown caps".to_owned());
        let zero_copy_d3d11 = caps.contains("memory:D3D11Memory");
        let zero_copy_d3d12 = caps.contains("memory:D3D12Memory");
        let memory_mode = memory_mode_from_caps(&caps);
        let zero_copy = is_zero_copy_memory_mode(memory_mode);
        send_log(
            &sender,
            "info",
            format!(
                "First decoded {label} buffer reached native sink; caps={caps}; memoryMode={memory_mode}; zeroCopy={zero_copy}; zeroCopyD3D11={zero_copy_d3d11}; zeroCopyD3D12={zero_copy_d3d12}."
            ),
        );

        if label == "video" && !reported.swap(true, Ordering::SeqCst) {
            if let Some(event_sender) = &sender {
                let message = if use_external_renderer_window() {
                    "Native video frames reached the external low-latency GStreamer renderer window."
                } else {
                    "Native video frames reached the embedded low-latency GStreamer sink."
                };
                let _ = event_sender.send(Event::Status {
                    status: "streaming",
                    message: Some(message.to_owned()),
                });
            }
        }

        gst::PadProbeReturn::Remove
    });
}

fn watch_rtp_video_bitrate(
    pad: &gst::Pad,
    video_liveness: VideoLivenessMonitor,
    event_sender: &Option<Sender<Event>>,
) {
    let sender = event_sender.clone();
    pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, info| {
        if let Some(buffer) = info.buffer() {
            video_liveness.record_encoded_buffer(buffer.size());
            if video_liveness.state.log_first_encoded_once() {
                send_log(
                    &sender,
                    "info",
                    format!(
                        "First encoded RTP video buffer arrived; size={} bytes.",
                        buffer.size()
                    ),
                );
            }
        }
        gst::PadProbeReturn::Ok
    });
}

fn watch_video_sink_rate(
    sink: &gst::Element,
    event_sender: &Option<Sender<Event>>,
    video_liveness: Option<VideoLivenessMonitor>,
) {
    let Some(sink_pad) = sink.static_pad("sink") else {
        return;
    };
    let sink = sink.clone();
    watch_video_pad_rate(
        &sink_pad,
        "Native video sink rate",
        Some(sink),
        event_sender,
        video_liveness.map(|monitor| (monitor, VideoLivenessPadKind::Sink)),
    );
}

fn watch_video_decoded_rate(
    queue: &gst::Element,
    event_sender: &Option<Sender<Event>>,
    video_liveness: Option<VideoLivenessMonitor>,
) {
    let Some(queue_sink_pad) = queue.static_pad("sink") else {
        return;
    };
    watch_video_pad_rate(
        &queue_sink_pad,
        "Native decoded video rate before present queue",
        None,
        event_sender,
        video_liveness.map(|monitor| (monitor, VideoLivenessPadKind::Decoded)),
    );
}

fn watch_video_caps_transitions(
    element: &gst::Element,
    source: &'static str,
    event_sender: &Option<Sender<Event>>,
    video_liveness: VideoLivenessMonitor,
) {
    let Some(src_pad) = element.static_pad("src") else {
        return;
    };
    let sender = event_sender.clone();
    let monitor = video_liveness.clone();
    let last_caps = Arc::new(Mutex::new(None::<String>));
    let last_framerate = Arc::new(Mutex::new(None::<String>));
    let last_memory_mode = Arc::new(Mutex::new(None::<String>));
    let last_caps_for_probe = last_caps.clone();
    let last_framerate_for_probe = last_framerate.clone();
    let last_memory_mode_for_probe = last_memory_mode.clone();

    src_pad.add_probe(gst::PadProbeType::BUFFER, move |pad, _info| {
        let caps = pad
            .current_caps()
            .map(|caps| caps.to_string())
            .unwrap_or_else(|| "unknown caps".to_owned());
        let framerate = caps_framerate_summary(&caps);
        let memory_mode = Some(memory_mode_from_caps(&caps).to_owned());

        let Ok(mut old_caps) = last_caps_for_probe.lock() else {
            return gst::PadProbeReturn::Ok;
        };
        let Ok(mut old_framerate) = last_framerate_for_probe.lock() else {
            return gst::PadProbeReturn::Ok;
        };
        let Ok(mut old_memory_mode) = last_memory_mode_for_probe.lock() else {
            return gst::PadProbeReturn::Ok;
        };

        if old_caps.is_none() {
            *old_caps = Some(caps);
            *old_framerate = framerate;
            *old_memory_mode = memory_mode;
            return gst::PadProbeReturn::Ok;
        }

        let caps_changed = old_caps.as_ref() != Some(&caps);
        let framerate_changed = *old_framerate != framerate;
        let memory_changed = *old_memory_mode != memory_mode;
        if caps_changed || framerate_changed || memory_changed {
            monitor.state.record_transition(
                &format!("{source}-caps-change"),
                source,
                old_caps.clone(),
                Some(caps.clone()),
                old_framerate.clone(),
                framerate.clone(),
                old_memory_mode.clone(),
                memory_mode.clone(),
                &sender,
            );
            *old_caps = Some(caps);
            *old_framerate = framerate;
            *old_memory_mode = memory_mode;
        }

        gst::PadProbeReturn::Ok
    });
}

fn watch_video_sink_caps_transitions(
    sink: &gst::Element,
    event_sender: &Option<Sender<Event>>,
    video_liveness: Option<VideoLivenessMonitor>,
) {
    let Some(monitor) = video_liveness else {
        return;
    };
    let Some(sink_pad) = sink.static_pad("sink") else {
        return;
    };
    let sender = event_sender.clone();
    let last_caps = Arc::new(Mutex::new(None::<String>));
    let last_framerate = Arc::new(Mutex::new(None::<String>));
    let last_memory_mode = Arc::new(Mutex::new(None::<String>));
    let last_caps_for_probe = last_caps.clone();
    let last_framerate_for_probe = last_framerate.clone();
    let last_memory_mode_for_probe = last_memory_mode.clone();

    sink_pad.add_probe(gst::PadProbeType::BUFFER, move |pad, _info| {
        let caps = pad
            .current_caps()
            .map(|caps| caps.to_string())
            .unwrap_or_else(|| "unknown caps".to_owned());
        let framerate = caps_framerate_summary(&caps);
        let memory_mode = Some(memory_mode_from_caps(&caps).to_owned());

        let Ok(mut old_caps) = last_caps_for_probe.lock() else {
            return gst::PadProbeReturn::Ok;
        };
        let Ok(mut old_framerate) = last_framerate_for_probe.lock() else {
            return gst::PadProbeReturn::Ok;
        };
        let Ok(mut old_memory_mode) = last_memory_mode_for_probe.lock() else {
            return gst::PadProbeReturn::Ok;
        };

        if old_caps.is_none() {
            *old_caps = Some(caps);
            *old_framerate = framerate;
            *old_memory_mode = memory_mode;
            return gst::PadProbeReturn::Ok;
        }

        let caps_changed = old_caps.as_ref() != Some(&caps);
        let framerate_changed = *old_framerate != framerate;
        let memory_changed = *old_memory_mode != memory_mode;
        if caps_changed || framerate_changed || memory_changed {
            monitor.state.record_transition(
                "sink-caps-change",
                "sink",
                old_caps.clone(),
                Some(caps.clone()),
                old_framerate.clone(),
                framerate.clone(),
                old_memory_mode.clone(),
                memory_mode.clone(),
                &sender,
            );
            *old_caps = Some(caps);
            *old_framerate = framerate;
            *old_memory_mode = memory_mode;
        }

        gst::PadProbeReturn::Ok
    });
}

fn install_present_limiter(
    sink: &gst::Element,
    present_max_fps: Arc<AtomicU32>,
    event_sender: &Option<Sender<Event>>,
    video_liveness: Option<VideoLivenessMonitor>,
) {
    let Some(sink_pad) = sink.static_pad("sink") else {
        return;
    };

    let sender = event_sender.clone();
    let monitor = video_liveness.clone();
    let state = Arc::new(Mutex::new(PresentLimiterState {
        next_present_at: Instant::now(),
        last_log_at: Instant::now(),
        passed: 0,
        dropped: 0,
        active_fps: 0,
    }));

    sink_pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
        let target_fps = present_max_fps.load(Ordering::Relaxed);
        if target_fps == 0 {
            return gst::PadProbeReturn::Ok;
        }

        let Ok(mut state) = state.lock() else {
            return gst::PadProbeReturn::Ok;
        };

        let now = Instant::now();
        if state.active_fps != target_fps {
            state.active_fps = target_fps;
            state.next_present_at = now;
            state.last_log_at = now;
            state.passed = 0;
            state.dropped = 0;
            if let Some(monitor) = &monitor {
                monitor.state.record_present_pacing_change();
            }
        }

        let frame_interval = Duration::from_secs_f64(1.0 / f64::from(target_fps.max(1)));
        if now < state.next_present_at {
            state.dropped = state.dropped.saturating_add(1);
            return gst::PadProbeReturn::Drop;
        }

        state.passed = state.passed.saturating_add(1);
        while state.next_present_at <= now {
            state.next_present_at += frame_interval;
        }
        let elapsed = state.last_log_at.elapsed();
        if elapsed >= VIDEO_SINK_RATE_LOG_INTERVAL {
            let passed = state.passed;
            let dropped = state.dropped;
            send_log(
                &sender,
                "debug",
                format!(
                    "Native present limiter: target={target_fps} fps; passed={passed}; dropped={dropped} over {:.1}s.",
                    elapsed.as_secs_f64()
                ),
            );
            state.last_log_at = now;
            state.passed = 0;
            state.dropped = 0;
        }

        gst::PadProbeReturn::Ok
    });
}

#[derive(Debug)]
struct PresentLimiterState {
    next_present_at: Instant,
    last_log_at: Instant,
    passed: u32,
    dropped: u32,
    active_fps: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VideoLivenessPadKind {
    Decoded,
    Sink,
}

fn watch_video_pad_rate(
    pad: &gst::Pad,
    label: &'static str,
    sink: Option<gst::Element>,
    event_sender: &Option<Sender<Event>>,
    video_liveness: Option<(VideoLivenessMonitor, VideoLivenessPadKind)>,
) {
    let sender = event_sender.clone();
    let state = Arc::new(Mutex::new((Instant::now(), 0u32)));

    pad.add_probe(gst::PadProbeType::BUFFER, move |pad, _info| {
        if let Some((monitor, kind)) = &video_liveness {
            match kind {
                VideoLivenessPadKind::Decoded => monitor.record_decoded_buffer(),
                VideoLivenessPadKind::Sink => monitor.record_sink_buffer(),
            }
        }

        let Ok(mut state) = state.lock() else {
            return gst::PadProbeReturn::Ok;
        };

        state.1 = state.1.saturating_add(1);
        let elapsed = state.0.elapsed();
        if elapsed >= VIDEO_SINK_RATE_LOG_INTERVAL {
            let frames = state.1;
            let fps = f64::from(frames) / elapsed.as_secs_f64();
            let caps = pad
                .current_caps()
                .map(|caps| caps.to_string())
                .unwrap_or_else(|| "unknown caps".to_owned());
            let zero_copy_d3d11 = caps.contains("memory:D3D11Memory");
            let zero_copy_d3d12 = caps.contains("memory:D3D12Memory");
            let memory_mode = memory_mode_from_caps(&caps);
            let zero_copy = is_zero_copy_memory_mode(memory_mode);
            if let Some((monitor, _)) = &video_liveness {
                monitor.update_caps(&caps);
            }
            let caps_framerate =
                caps_framerate_summary(&caps).unwrap_or_else(|| "unknown".to_owned());
            let requested_fps = video_liveness
                .as_ref()
                .and_then(|(monitor, _)| monitor.state.requested_fps());
            let requested_fps_summary = requested_fps
                .map(|fps| format!("; requestedFps={fps}"))
                .unwrap_or_default();
            if let (Some((monitor, _)), Some(requested_fps), Some(caps_framerate_value)) = (
                video_liveness.as_ref(),
                requested_fps,
                caps_framerate_summary(&caps),
            ) {
                let expected = format!("{requested_fps}/1");
                if caps_framerate_value != expected && monitor.state.warn_framerate_mismatch_once() {
                    monitor.state.record_transition(
                        "high-fps-transition-risk",
                        label,
                        None,
                        Some(caps.clone()),
                        None,
                        Some(caps_framerate_value.clone()),
                        None,
                        Some(memory_mode.to_owned()),
                        &sender,
                    );
                    send_log(
                        &sender,
                        "warn",
                        format!(
                            "Native video caps framerate {caps_framerate_value} does not match requestedFps={requested_fps}; this can destabilize high-FPS native playback scheduling and buffer pools."
                        ),
                    );
                }
            }
            let sink_stats = sink
                .as_ref()
                .map(|sink| format!("; {}", sink_stats_summary(sink)))
                .unwrap_or_default();

            send_log(
                &sender,
                "debug",
                format!(
                    "{label}: {fps:.1} fps; capsFramerate={caps_framerate}{requested_fps_summary}; memoryMode={memory_mode}; zeroCopy={zero_copy}; zeroCopyD3D11={zero_copy_d3d11}; zeroCopyD3D12={zero_copy_d3d12}{sink_stats}."
                ),
            );

            *state = (Instant::now(), 0);
        }

        gst::PadProbeReturn::Ok
    });
}

fn sink_stats_summary(sink: &gst::Element) -> String {
    let stats = read_sink_stats(sink);
    if !stats.available {
        return "sinkStats=unavailable".to_owned();
    }

    format!(
        "sinkStats rendered={} dropped={} averageRate={}",
        stats
            .rendered
            .map(|value| value.to_string())
            .unwrap_or_else(|| "n/a".to_owned()),
        stats
            .dropped
            .map(|value| value.to_string())
            .unwrap_or_else(|| "n/a".to_owned()),
        stats
            .average_rate
            .map(|value| format!("{value:.1}"))
            .unwrap_or_else(|| "n/a".to_owned())
    )
}

#[derive(Debug, Clone, Copy, Default)]
struct VideoSinkStats {
    available: bool,
    rendered: Option<u64>,
    dropped: Option<u64>,
    average_rate: Option<f64>,
}

fn read_sink_stats(sink: &gst::Element) -> VideoSinkStats {
    if sink.find_property("stats").is_none() {
        return VideoSinkStats::default();
    }

    let stats = sink.property::<gst::Structure>("stats");
    VideoSinkStats {
        available: true,
        rendered: stats.get::<u64>("rendered").ok(),
        dropped: stats.get::<u64>("dropped").ok(),
        average_rate: stats.get::<f64>("average-rate").ok(),
    }
}

fn caps_framerate_summary(caps: &str) -> Option<String> {
    let marker = "framerate=(fraction)";
    let start = caps.find(marker)? + marker.len();
    let rest = &caps[start..];
    let semicolon = rest.find(';');
    let comma = rest.find(',');
    let end = match (semicolon, comma) {
        (Some(left), Some(right)) => left.min(right),
        (Some(index), None) | (None, Some(index)) => index,
        (None, None) => rest.len(),
    };
    Some(rest[..end].trim().to_owned())
}

fn memory_mode_from_caps(caps: &str) -> &'static str {
    if caps.contains("memory:D3D12Memory") {
        "D3D12Memory"
    } else if caps.contains("memory:D3D11Memory") {
        "D3D11Memory"
    } else if caps.contains("memory:VulkanImage") {
        "VulkanImage"
    } else if caps.contains("memory:VAMemory") {
        "VAMemory"
    } else if caps.contains("memory:GLMemory") {
        "GLMemory"
    } else {
        "system-memory"
    }
}

fn is_zero_copy_memory_mode(memory_mode: &str) -> bool {
    matches!(
        memory_mode,
        "D3D12Memory" | "D3D11Memory" | "VulkanImage" | "VAMemory" | "GLMemory"
    )
}

fn link_decoded_media_to_fakesink(
    pipeline: &gst::Pipeline,
    src_pad: &gst::Pad,
    label: &str,
) -> Result<(), String> {
    if src_pad.is_linked() {
        return Ok(());
    }

    let sink = gst::ElementFactory::make("fakesink")
        .property("sync", false)
        .property("async", false)
        .build()
        .map_err(|error| format!("Failed to create {label}: {error}"))?;
    configure_sink_for_low_latency(&sink);
    pipeline
        .add(&sink)
        .map_err(|error| format!("Failed to add {label}: {error}"))?;
    sink.sync_state_with_parent()
        .map_err(|error| format!("Failed to sync {label} state: {error}"))?;

    let Some(sink_pad) = sink.static_pad("sink") else {
        return Err(format!("{label} has no sink pad."));
    };
    src_pad
        .link(&sink_pad)
        .map(|_| ())
        .map_err(|error| format!("Failed to link {label}: {error:?}"))
}

fn make_element(factory: &str) -> Result<gst::Element, String> {
    gst::ElementFactory::make(factory)
        .build()
        .map_err(|error| format!("Failed to create GStreamer element {factory}: {error}"))
}

#[derive(Debug)]
pub struct GstreamerBackend {
    active_context: Option<NativeStreamerSessionContext>,
    pending_remote_ice: Vec<IceCandidatePayload>,
    pipeline: Option<GstreamerPipeline>,
    event_sender: Option<Sender<Event>>,
    remote_description_set: bool,
    render_surface: Option<NativeRenderSurface>,
}

impl GstreamerBackend {
    pub fn new(event_sender: Option<Sender<Event>>) -> Self {
        Self {
            active_context: None,
            pending_remote_ice: Vec::new(),
            pipeline: None,
            event_sender,
            remote_description_set: false,
            render_surface: None,
        }
    }

    fn replay_pending_remote_ice(&mut self) -> Vec<Event> {
        let candidates = std::mem::take(&mut self.pending_remote_ice);
        let Some(pipeline) = self.pipeline.as_mut() else {
            self.pending_remote_ice = candidates;
            return Vec::new();
        };

        let mut events = Vec::new();
        for candidate in candidates {
            if let Err(message) = pipeline.add_remote_ice(&candidate) {
                events.push(Event::Error {
                    code: "remote-ice-failed".to_owned(),
                    message,
                });
            }
        }
        events
    }
}

impl NativeStreamerBackend for GstreamerBackend {
    fn capabilities(&self) -> NativeStreamerCapabilities {
        NativeStreamerCapabilities {
            protocol_version: PROTOCOL_VERSION,
            backend: "gstreamer",
            requested_backend: None,
            fallback_reason: None,
            supports_offer_answer: true,
            supports_remote_ice: true,
            supports_local_ice: true,
            supports_input: true,
            video_backends: match init_gstreamer() {
                Ok(()) => native_video_backend_capabilities(),
                Err(error) => vec![NativeVideoBackendCapability {
                    backend: "gstreamer".to_owned(),
                    platform: current_platform_label().to_owned(),
                    codecs: Vec::new(),
                    zero_copy_modes: Vec::new(),
                    sink: None,
                    available: false,
                    reason: Some(error),
                }],
            },
        }
    }

    fn start(&mut self, command: CommandEnvelope) -> BackendReply {
        let id = command.id;
        let Some(context) = command.context else {
            return BackendReply::response(missing_field(&id, "context"));
        };

        let session_id = context.session.session_id.clone();
        let pipeline = match GstreamerPipeline::build(self.event_sender.clone()) {
            Ok(pipeline) => pipeline,
            Err(message) => {
                return BackendReply {
                    events: vec![Event::Error {
                        code: "gstreamer-start-failed".to_owned(),
                        message: message.clone(),
                    }],
                    response: Some(Response::Error {
                        id: Some(id),
                        code: "gstreamer-start-failed".to_owned(),
                        message,
                    }),
                    should_continue: true,
                };
            }
        };

        if let Some(old_pipeline) = self.pipeline.take() {
            if let Err(message) = old_pipeline.stop() {
                eprintln!("[NativeStreamer] {message}");
            }
        }

        self.active_context = Some(context);
        self.pending_remote_ice.clear();
        self.remote_description_set = false;
        let webrtc_name = pipeline.webrtc_name();
        self.pipeline = Some(pipeline);
        if let (Some(surface), Some(pipeline)) =
            (self.render_surface.clone(), self.pipeline.as_ref())
        {
            pipeline.update_render_surface(surface);
        }

        BackendReply {
            events: vec![Event::Status {
                status: "ready",
                message: Some(format!(
                    "GStreamer backend selected for session {session_id}; {} pipeline is ready.",
                    webrtc_name
                )),
            }],
            response: Some(Response::Ok { id }),
            should_continue: true,
        }
    }

    fn handle_offer(&mut self, command: CommandEnvelope) -> BackendReply {
        let id = command.id.clone();
        let Some(context) = command.context else {
            return BackendReply::response(missing_field(&id, "context"));
        };
        let Some(offer_sdp) = command.sdp else {
            return BackendReply::response(missing_field(&id, "sdp"));
        };

        let prepared = match prepare_native_offer(&context, &offer_sdp) {
            Ok(prepared) => prepared,
            Err(error) => return BackendReply::response(error.into_response(id)),
        };

        let mut events = prepared_offer_events(&prepared);
        let parsed_offer = match GstreamerPipeline::parse_offer_sdp(&prepared.gstreamer_offer_sdp) {
            Ok(offer) => offer,
            Err(message) => {
                return BackendReply {
                    events,
                    response: Some(Response::Error {
                        id: Some(id),
                        code: "invalid-remote-sdp".to_owned(),
                        message,
                    }),
                    should_continue: true,
                };
            }
        };

        let Some(pipeline) = self.pipeline.as_mut() else {
            return BackendReply {
                events,
                response: Some(Response::Error {
                    id: Some(id),
                    code: "gstreamer-not-started".to_owned(),
                    message: "GStreamer pipeline is not started.".to_owned(),
                }),
                should_continue: true,
            };
        };

        let present_max_fps = resolve_present_max_fps(context.settings.fps);
        let d3d_fullscreen_sink = resolve_d3d_fullscreen_sink(context.settings.enable_cloud_gsync);
        pipeline.set_present_max_fps(present_max_fps);
        pipeline.set_d3d_fullscreen_sink(d3d_fullscreen_sink);
        pipeline.configure_stats(&context, prepared.nvst_params.max_bitrate_kbps);
        if present_max_fps > 0 && present_max_fps != PRESENT_LIMITER_AUTO_SENTINEL {
            events.push(Event::Log {
                level: "info",
                message: format!(
                    "Native present limiter enabled at {present_max_fps} fps for {} fps stream; set {NATIVE_PRESENT_MAX_FPS_ENV}=0 to disable.",
                    context.settings.fps
                ),
            });
        }
        if d3d_fullscreen_sink {
            events.push(Event::Log {
                level: "info",
                message: format!(
                    "Native D3D fullscreen presentation is enabled for Cloud G-Sync/VRR; set {NATIVE_D3D_FULLSCREEN_ENV}=0 to disable."
                ),
            });
        }

        let answer_sdp = match pipeline.negotiate_answer(
            parsed_offer,
            (prepared.gstreamer_ice_pwd_replacements > 0)
                .then_some(&prepared.nvst_params.credentials),
            prepared.nvst_params.partial_reliable_threshold_ms,
        ) {
            Ok(answer_sdp) => munge_answer_sdp(&answer_sdp, prepared.nvst_params.max_bitrate_kbps),
            Err(message) => {
                return BackendReply {
                    events,
                    response: Some(Response::Error {
                        id: Some(id),
                        code: "gstreamer-negotiation-failed".to_owned(),
                        message,
                    }),
                    should_continue: true,
                };
            }
        };
        self.remote_description_set = true;
        events.extend(self.replay_pending_remote_ice());

        events.push(Event::Log {
            level: "info",
            message:
                "GStreamer created a local WebRTC answer and replayed queued remote ICE candidates."
                    .to_owned(),
        });

        if let Some(negotiated_codec) = extract_negotiated_video_codec(&answer_sdp) {
            if negotiated_codec != prepared.nvst_params.codec {
                events.push(Event::Log {
                    level: "warn",
                    message: format!(
                        "Negotiated video codec is {} while requested codec was {}; building NVST SDP for the negotiated codec to avoid server/client codec mismatch.",
                        negotiated_codec.as_str(),
                        prepared.nvst_params.codec.as_str(),
                    ),
                });
            } else {
                events.push(Event::Log {
                    level: "debug",
                    message: format!(
                        "Negotiated video codec confirmed as {}.",
                        negotiated_codec.as_str()
                    ),
                });
            }
        }

        let nvst_sdp = match build_nvst_sdp_for_answer(&prepared.nvst_params, &answer_sdp) {
            Ok(nvst_sdp) => nvst_sdp,
            Err(message) => {
                return BackendReply {
                    events,
                    response: Some(Response::Error {
                        id: Some(id),
                        code: "invalid-local-answer-sdp".to_owned(),
                        message,
                    }),
                    should_continue: true,
                };
            }
        };

        events.push(Event::Log {
            level: "debug",
            message: "Built native NVST SDP from the local WebRTC answer transport credentials."
                .to_owned(),
        });

        BackendReply {
            events,
            response: Some(Response::Answer {
                id,
                answer: SendAnswerRequest {
                    sdp: answer_sdp,
                    nvst_sdp: Some(nvst_sdp),
                },
            }),
            should_continue: true,
        }
    }

    fn add_remote_ice(&mut self, command: CommandEnvelope) -> BackendReply {
        let Some(candidate) = command.candidate else {
            return BackendReply::response(missing_field(&command.id, "candidate"));
        };

        if self.remote_description_set {
            if let Some(pipeline) = self.pipeline.as_mut() {
                if let Err(message) = pipeline.add_remote_ice(&candidate) {
                    return BackendReply::response(Response::Error {
                        id: Some(command.id),
                        code: "remote-ice-failed".to_owned(),
                        message,
                    });
                }
            } else {
                self.pending_remote_ice.push(candidate);
            }
        } else {
            self.pending_remote_ice.push(candidate);
        }
        BackendReply::response(Response::Ok { id: command.id })
    }

    fn send_input(&mut self, command: CommandEnvelope) -> BackendReply {
        let Some(packet) = command.input else {
            return BackendReply::continue_without_response();
        };

        let Ok(payload) = packet.payload_bytes() else {
            return BackendReply::continue_without_response();
        };

        if payload.is_empty() || payload.len() > 4096 {
            return BackendReply::continue_without_response();
        }

        if let Some(pipeline) = self.pipeline.as_ref() {
            let _ = pipeline.send_input_packet(&payload, packet.partially_reliable);
        }

        BackendReply::continue_without_response()
    }

    fn update_render_surface(&mut self, command: CommandEnvelope) -> BackendReply {
        let Some(surface) = command.surface else {
            return BackendReply::response(missing_field(&command.id, "surface"));
        };

        self.render_surface = Some(surface.clone());
        if let Some(pipeline) = self.pipeline.as_ref() {
            pipeline.update_render_surface(surface);
        }

        BackendReply::response(Response::Ok { id: command.id })
    }

    fn update_bitrate_limit(&mut self, command: CommandEnvelope) -> BackendReply {
        let Some(max_bitrate_kbps) = command.max_bitrate_kbps else {
            return BackendReply::response(missing_field(&command.id, "maxBitrateKbps"));
        };

        let max_bitrate_kbps = normalize_bitrate_kbps(max_bitrate_kbps);
        update_context_bitrate_limit(&mut self.active_context, max_bitrate_kbps);

        BackendReply {
            events: vec![Event::Log {
                level: "info",
                message: format!(
                    "Updated native bitrate limit to {max_bitrate_kbps} Kbps. The active GFN server bitrate cap is negotiated in NVST SDP and will apply on the next native offer/reconnect."
                ),
            }],
            response: Some(Response::Ok { id: command.id }),
            should_continue: true,
        }
    }

    fn stop(&mut self, command: CommandEnvelope) -> BackendReply {
        self.active_context = None;
        self.pending_remote_ice.clear();
        self.remote_description_set = false;
        if let Some(pipeline) = self.pipeline.take() {
            if let Err(message) = pipeline.stop() {
                return BackendReply {
                    events: vec![Event::Error {
                        code: "gstreamer-stop-failed".to_owned(),
                        message: message.clone(),
                    }],
                    response: Some(Response::Error {
                        id: Some(command.id),
                        code: "gstreamer-stop-failed".to_owned(),
                        message,
                    }),
                    should_continue: true,
                };
            }
        }
        let message = command
            .reason
            .unwrap_or_else(|| "stop requested".to_owned());
        BackendReply::stop(command.id, message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::VideoCodec;

    #[test]
    fn builds_and_stops_webrtc_pipeline() {
        let pipeline = GstreamerPipeline::build(None).expect("GStreamer webrtcbin pipeline");
        assert_eq!(pipeline.webrtc.name(), "opennow-webrtcbin");
        pipeline.stop().expect("pipeline stops");
    }

    #[test]
    fn configures_dwrite_stats_overlay_without_type_panics() {
        gst::init().expect("gstreamer init");
        let Some(overlay) = gst::ElementFactory::make("dwritetextoverlay").build().ok() else {
            return;
        };

        configure_stats_overlay_element(&overlay);
        overlay.set_property("text", "OpenNOW native stats");
    }

    #[test]
    fn parses_basic_remote_offer_sdp() {
        let sdp = "v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\nm=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\nc=IN IP4 127.0.0.1\r\na=mid:0\r\na=sctp-port:5000\r\n";
        let parsed = GstreamerPipeline::parse_offer_sdp(sdp).expect("valid SDP");
        assert_eq!(parsed.medias_len(), 1);
    }

    #[test]
    fn defers_gfn_uuid_ice_password_until_actual_ice_stream_exists() {
        let mut pipeline = GstreamerPipeline::build(None).expect("GStreamer webrtcbin pipeline");
        let credentials = IceCredentials {
            ufrag: "2efecf37".to_owned(),
            pwd: "26b335b8-6cb2-4c18-96d0-963e5e586c9a".to_owned(),
            fingerprint: String::new(),
        };

        pipeline.original_remote_ice_credentials = Some(credentials);
        assert!(!pipeline
            .try_restore_original_remote_ice_credentials("without negotiated streams")
            .expect("remote ICE credential restoration can be deferred"));
        pipeline.stop().expect("pipeline stops");
    }

    #[test]
    fn remote_ice_credential_restore_after_remote_description_does_not_probe_fake_streams() {
        let mut pipeline = GstreamerPipeline::build(None).expect("GStreamer webrtcbin pipeline");
        let sdp = concat!(
            "v=0\r\n",
            "o=- 4373647202393833435 2 IN IP4 127.0.0.1\r\n",
            "s=-\r\n",
            "t=0 0\r\n",
            "a=group:BUNDLE 0 1 2 3\r\n",
            "a=ice-options:trickle\r\n",
            "a=ice-lite\r\n",
            "m=audio 9 UDP/TLS/RTP/SAVPF 111\r\n",
            "c=IN IP4 0.0.0.0\r\n",
            "a=mid:0\r\n",
            "a=ice-ufrag:2efecf37\r\n",
            "a=ice-pwd:26b335b899a84ffab9aaf38ddad1e2b4\r\n",
            "a=fingerprint:sha-256 94:6C:60:66:35:B9:F6:B4:BC:46:60:EF:81:AC:AB:87:A9:45:4A:09:92:E4:3E:16:28:7E:BD:6D:8C:1A:7D:6B\r\n",
            "a=setup:actpass\r\n",
            "a=rtcp-mux\r\n",
            "a=rtpmap:111 OPUS/48000/2\r\n",
            "m=video 9 UDP/TLS/RTP/SAVPF 96\r\n",
            "c=IN IP4 0.0.0.0\r\n",
            "a=mid:1\r\n",
            "a=ice-ufrag:2efecf37\r\n",
            "a=ice-pwd:26b335b899a84ffab9aaf38ddad1e2b4\r\n",
            "a=fingerprint:sha-256 94:6C:60:66:35:B9:F6:B4:BC:46:60:EF:81:AC:AB:87:A9:45:4A:09:92:E4:3E:16:28:7E:BD:6D:8C:1A:7D:6B\r\n",
            "a=setup:actpass\r\n",
            "a=rtcp-mux\r\n",
            "a=rtpmap:96 H264/90000\r\n",
            "m=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\n",
            "c=IN IP4 0.0.0.0\r\n",
            "a=mid:2\r\n",
            "a=ice-ufrag:2efecf37\r\n",
            "a=ice-pwd:26b335b899a84ffab9aaf38ddad1e2b4\r\n",
            "a=fingerprint:sha-256 94:6C:60:66:35:B9:F6:B4:BC:46:60:EF:81:AC:AB:87:A9:45:4A:09:92:E4:3E:16:28:7E:BD:6D:8C:1A:7D:6B\r\n",
            "a=setup:actpass\r\n",
            "a=sctp-port:5000\r\n",
            "m=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\n",
            "c=IN IP4 0.0.0.0\r\n",
            "a=mid:3\r\n",
            "a=ice-ufrag:2efecf37\r\n",
            "a=ice-pwd:26b335b899a84ffab9aaf38ddad1e2b4\r\n",
            "a=fingerprint:sha-256 94:6C:60:66:35:B9:F6:B4:BC:46:60:EF:81:AC:AB:87:A9:45:4A:09:92:E4:3E:16:28:7E:BD:6D:8C:1A:7D:6B\r\n",
            "a=setup:actpass\r\n",
            "a=sctp-port:5000\r\n",
        );
        let offer_sdp = GstreamerPipeline::parse_offer_sdp(sdp).expect("valid SDP");
        let offer =
            gst_webrtc::WebRTCSessionDescription::new(gst_webrtc::WebRTCSDPType::Offer, offer_sdp);
        pipeline
            .pipeline
            .set_state(gst::State::Playing)
            .expect("pipeline plays");
        pipeline
            .set_description("set-remote-description", &offer)
            .expect("remote description");

        let credentials = IceCredentials {
            ufrag: "2efecf37".to_owned(),
            pwd: "26b335b8-99a8-4ffa-b9aa-f38ddad1e2b4".to_owned(),
            fingerprint: String::new(),
        };
        pipeline.original_remote_ice_credentials = Some(credentials);
        pipeline
            .try_restore_original_remote_ice_credentials("after remote description")
            .expect("remote ICE credential restoration does not fail without actual streams");
        pipeline.stop().expect("pipeline stops");
    }

    #[test]
    fn reports_offer_answer_and_local_ice_capabilities() {
        let backend = GstreamerBackend::new(None);
        let capabilities = backend.capabilities();
        assert!(capabilities.supports_offer_answer);
        assert!(capabilities.supports_local_ice);
        assert!(capabilities.supports_input);
    }

    #[test]
    fn parses_input_handshake_versions() {
        assert_eq!(
            parse_input_handshake_version(&[0x0e, 0x02, 0x03, 0x00]),
            Some(3)
        );
        assert_eq!(parse_input_handshake_version(&[0x0e, 0x02]), Some(2));
        assert_eq!(parse_input_handshake_version(&[0x0e, 0x03]), Some(0x030e));
        assert_eq!(parse_input_handshake_version(&[0x01, 0x02, 0x03]), None);
        assert_eq!(parse_input_handshake_version(&[0x0e]), None);
    }

    #[test]
    fn maps_rtp_video_codecs_to_explicit_gpu_decode_chains() {
        let h265 =
            rtp_video_chain_definition("H265", RtpVideoApi::D3D11).expect("H265 D3D11 chain");
        assert_eq!(h265[0].factory, "rtph265depay");
        assert_eq!(h265[3].factory, "d3d11h265dec");
        assert_eq!(h265[4].factory, "dwritetextoverlay");
        assert_eq!(h265[6].factory, "d3d11videosink");
        assert!(!h265
            .iter()
            .any(|spec| spec.role == RtpVideoChainRole::PostDecodeCapsFilter));

        let h264 =
            rtp_video_chain_definition("h264", RtpVideoApi::D3D12).expect("H264 D3D12 chain");
        assert_eq!(h264[0].factory, "rtph264depay");
        assert_eq!(h264[3].factory, "d3d12h264dec");
        assert_eq!(h264[4].factory, "dwritetextoverlay");
        assert_eq!(h264[6].factory, "d3d12videosink");
        assert!(!h264
            .iter()
            .any(|spec| spec.role == RtpVideoChainRole::PostDecodeCapsFilter));

        let av1 = rtp_video_chain_definition("AV1", RtpVideoApi::D3D11).expect("AV1 D3D11 chain");
        assert_eq!(av1[0].factory, "rtpav1depay");
        assert_eq!(av1[3].factory, "d3d11av1dec");
        assert_eq!(av1[4].factory, "dwritetextoverlay");
        assert_eq!(av1[6].factory, "d3d11videosink");
    }

    #[test]
    fn does_not_force_d3d_memory_caps_by_default() {
        let d3d11 =
            rtp_video_chain_definition("H265", RtpVideoApi::D3D11).expect("H265 D3D11 chain");
        let d3d12 =
            rtp_video_chain_definition("H264", RtpVideoApi::D3D12).expect("H264 D3D12 chain");

        assert!(!d3d11
            .iter()
            .any(|spec| spec.role == RtpVideoChainRole::PostDecodeCapsFilter));
        assert!(!d3d12
            .iter()
            .any(|spec| spec.role == RtpVideoChainRole::PostDecodeCapsFilter));
    }

    #[test]
    fn maps_cross_platform_video_paths_to_expected_decoders() {
        let vt =
            rtp_video_chain_definition("H264", RtpVideoApi::VideoToolbox).expect("VideoToolbox");
        assert_eq!(vt[3].factory, "vtdec_hw");
        assert!(vt.iter().any(|spec| spec.factory == "videoconvert"));
        assert_eq!(vt.last().map(|spec| spec.factory), Some("glimagesink"));
        assert!(!vt.iter().any(|spec| spec.factory == "capsfilter"));

        let vaapi = rtp_video_chain_definition("AV1", RtpVideoApi::Vaapi).expect("VAAPI AV1");
        assert_eq!(vaapi[3].factory, "vaav1dec");
        assert!(vaapi.iter().any(|spec| spec.factory == "videoconvert"));
        assert_eq!(vaapi.last().map(|spec| spec.factory), Some("glimagesink"));

        let v4l2 = rtp_video_chain_definition("H265", RtpVideoApi::V4L2).expect("V4L2 H265");
        assert_eq!(v4l2[3].factory, "v4l2slh265dec");
        assert!(v4l2.iter().any(|spec| spec.factory == "videoconvert"));

        let vulkan = rtp_video_chain_definition("H265", RtpVideoApi::Vulkan).expect("Vulkan H265");
        assert_eq!(vulkan[3].factory, "vulkanh265dec");
        assert!(vulkan
            .iter()
            .any(|spec| spec.factory == "vulkancolorconvert"));
        assert_eq!(vulkan.last().map(|spec| spec.factory), Some("vulkansink"));
        assert!(rtp_video_chain_definition("AV1", RtpVideoApi::Vulkan).is_none());

        let software =
            rtp_video_chain_definition("H264", RtpVideoApi::Software).expect("software H264");
        assert_eq!(software[3].factory, "avdec_h264");
        assert!(software.iter().any(|spec| spec.factory == "videoconvert"));
        assert_eq!(
            software.last().map(|spec| spec.factory),
            Some("autovideosink")
        );
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn windows_default_video_api_prefers_d3d12_for_high_fps() {
        assert_eq!(
            default_rtp_video_api_priority(Some(240)),
            vec![
                RtpVideoApi::D3D12,
                RtpVideoApi::D3D11,
                RtpVideoApi::Software
            ]
        );
        assert_eq!(
            default_rtp_video_api_priority(Some(120)),
            vec![
                RtpVideoApi::D3D11,
                RtpVideoApi::D3D12,
                RtpVideoApi::Software
            ]
        );
    }

    #[test]
    fn automatic_present_limiter_uses_display_refresh_below_requested_fps() {
        assert_eq!(automatic_present_max_fps(240, Some(165)), 165);
        assert_eq!(automatic_present_max_fps(240, Some(240)), 0);
        assert_eq!(automatic_present_max_fps(240, Some(1)), 0);
        assert_eq!(automatic_present_max_fps(240, None), 0);
    }

    #[test]
    fn automatic_present_limiter_only_targets_d3d11() {
        assert_eq!(
            effective_present_max_fps(
                PRESENT_LIMITER_AUTO_SENTINEL,
                Some(240),
                RtpVideoApi::D3D11,
                Some(165)
            ),
            165
        );
        assert_eq!(
            effective_present_max_fps(
                PRESENT_LIMITER_AUTO_SENTINEL,
                Some(240),
                RtpVideoApi::D3D12,
                Some(165)
            ),
            0
        );
        assert_eq!(
            effective_present_max_fps(144, Some(240), RtpVideoApi::D3D12, Some(165)),
            144
        );
        assert_eq!(
            effective_present_max_fps(0, Some(240), RtpVideoApi::D3D11, Some(165)),
            0
        );
    }

    #[test]
    fn formats_selected_video_chain_diagnostics() {
        let specs =
            rtp_video_chain_definition("H264", RtpVideoApi::Software).expect("software H264");
        let message = format_video_chain_selection("H264", RtpVideoApi::Software, &specs);

        assert!(message.contains("backend=software"));
        assert!(message.contains("decoder=avdec_h264"));
        assert!(message.contains("converter=videoconvert"));
        assert!(message.contains("memory=system-memory"));
    }

    #[test]
    fn extracts_caps_framerate_summary() {
        let caps = "video/x-raw(memory:D3D11Memory), format=(string)NV12, framerate=(fraction)240/1; zeroCopyD3D11=true";
        assert_eq!(caps_framerate_summary(caps).as_deref(), Some("240/1"));
        assert_eq!(caps_framerate_summary("video/x-raw").as_deref(), None);
    }

    #[test]
    fn video_stall_tracker_waits_until_threshold() {
        let mut tracker = VideoStallTracker::default();

        assert_eq!(tracker.evaluate(2_499, 0), VideoStallAction::None);
    }

    #[test]
    fn video_stall_tracker_progresses_recovery_attempts() {
        let mut tracker = VideoStallTracker::default();

        assert_eq!(
            tracker.evaluate(2_500, 0),
            VideoStallAction::RequestKeyframe {
                attempt: 1,
                stall_ms: 2_500,
            },
        );
        assert_eq!(tracker.evaluate(3_000, 0), VideoStallAction::None);
        assert_eq!(
            tracker.evaluate(5_000, 0),
            VideoStallAction::RequestKeyframe {
                attempt: 2,
                stall_ms: 5_000,
            },
        );
        assert_eq!(
            tracker.evaluate(8_000, 0),
            VideoStallAction::Resync {
                attempt: 3,
                stall_ms: 8_000,
            },
        );
        assert_eq!(
            tracker.evaluate(12_000, 0),
            VideoStallAction::PartialFlush {
                attempt: 4,
                stall_ms: 12_000,
            },
        );
        assert_eq!(
            tracker.evaluate(16_000, 0),
            VideoStallAction::CompleteFlush {
                attempt: 5,
                stall_ms: 16_000,
            },
        );
        assert_eq!(
            tracker.evaluate(20_000, 0),
            VideoStallAction::Fatal {
                attempt: 6,
                stall_ms: 20_000,
            },
        );
    }

    #[test]
    fn video_stall_tracker_resets_after_recovery() {
        let mut tracker = VideoStallTracker::default();

        assert_eq!(
            tracker.evaluate(2_500, 0),
            VideoStallAction::RequestKeyframe {
                attempt: 1,
                stall_ms: 2_500,
            },
        );
        assert_eq!(
            tracker.evaluate(2_600, 2_600),
            VideoStallAction::Recovered { stall_ms: 2_600 },
        );
        assert_eq!(tracker.evaluate(3_000, 2_600), VideoStallAction::None);
        assert_eq!(
            tracker.evaluate(5_100, 2_600),
            VideoStallAction::RequestKeyframe {
                attempt: 1,
                stall_ms: 2_500,
            },
        );
    }

    #[test]
    fn resolve_queue_mode_prefers_adaptive_for_240_fps_and_vrr_for_cloud_gsync() {
        let adaptive = resolve_queue_mode(&StreamSettings {
            resolution: "2560x1440".to_owned(),
            fps: 240,
            max_bitrate_mbps: 75,
            codec: VideoCodec::H265,
            color_quality: crate::protocol::ColorQuality::TenBit420,
            enable_cloud_gsync: false,
            native_transition_diagnostics: None,
        });
        assert_eq!(adaptive, NativeQueueMode::Adaptive);

        let vrr = resolve_queue_mode(&StreamSettings {
            resolution: "2560x1440".to_owned(),
            fps: 120,
            max_bitrate_mbps: 75,
            codec: VideoCodec::H265,
            color_quality: crate::protocol::ColorQuality::TenBit420,
            enable_cloud_gsync: true,
            native_transition_diagnostics: None,
        });
        assert_eq!(vrr, NativeQueueMode::Vrr);
    }

    #[test]
    fn reports_missing_sink_stats_as_unavailable() {
        gst::init().expect("gstreamer init");
        let sink = gst::ElementFactory::make("fakesink")
            .build()
            .expect("fakesink");
        assert_eq!(
            sink_stats_summary(&sink),
            "sinkStats rendered=0 dropped=0 averageRate=0.0"
        );
    }
}
