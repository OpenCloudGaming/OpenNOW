use crate::gstreamer_backend::send_log;
use crate::gstreamer_config::use_external_renderer_window;
use crate::gstreamer_pipeline::{configure_queue, set_property_if_supported};
use crate::gstreamer_transitions::{
    format_transition_summary, resolve_queue_mode, TransitionSnapshot, TransitionTelemetry,
    DEFAULT_VIDEO_QUEUE_DEPTH,
};
use crate::protocol::{Event, NativeQueueMode, NativeStreamerSessionContext, VideoStallEvent};
use gst::prelude::*;
use gstreamer as gst;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub(crate) const VIDEO_SINK_RATE_LOG_INTERVAL: Duration = Duration::from_secs(1);
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct VideoRateSnapshot {
    encoded_kbps: f64,
    decoded_fps: f64,
    sink_fps: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VideoStallAction {
    None,
    RequestKeyframe { attempt: u8, stall_ms: u64 },
    Resync { attempt: u8, stall_ms: u64 },
    PartialFlush { attempt: u8, stall_ms: u64 },
    CompleteFlush { attempt: u8, stall_ms: u64 },
    Fatal { attempt: u8, stall_ms: u64 },
    Recovered { stall_ms: u64 },
}

#[derive(Debug, Clone)]
pub(crate) struct VideoStallTracker {
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
    pub(crate) fn evaluate(&mut self, now_ms: u64, last_video_ms: u64) -> VideoStallAction {
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

#[derive(Debug)]
pub(crate) struct VideoLivenessState {
    started_at: Instant,
    codec: Mutex<String>,
    resolution: Mutex<String>,
    hardware_acceleration: Mutex<String>,
    render_sink: Mutex<String>,
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
            render_sink: Mutex::new(String::new()),
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

    pub(crate) fn configure(
        &self,
        context: &NativeStreamerSessionContext,
        target_bitrate_kbps: u32,
    ) {
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
            telemetry.queue_depth = DEFAULT_VIDEO_QUEUE_DEPTH;
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

    pub(crate) fn update_hardware_acceleration(&self, value: impl Into<String>) {
        if let Ok(mut hardware_acceleration) = self.hardware_acceleration.lock() {
            *hardware_acceleration = value.into();
        }
    }

    pub(crate) fn update_render_sink(&self, value: impl Into<String>) {
        if let Ok(mut render_sink) = self.render_sink.lock() {
            *render_sink = value.into();
        }
    }

    pub(crate) fn record_encoded_buffer(&self, size: usize) {
        self.last_encoded_ms.store(self.now_ms(), Ordering::Relaxed);
        self.encoded_bytes_total
            .fetch_add(size as u64, Ordering::Relaxed);
    }

    pub(crate) fn record_audio_buffer(&self) {
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

    pub(crate) fn set_stats_overlay(&self, overlay: Option<gst::Element>) {
        let visible = self.stats_overlay_visible.load(Ordering::Relaxed);
        if let Ok(mut current) = self.stats_overlay.lock() {
            if let Some(overlay) = overlay.as_ref() {
                set_property_if_supported(overlay, "visible", visible);
                set_property_if_supported(overlay, "silent", !visible);
            }
            *current = overlay;
        }
    }

    pub(crate) fn set_stats_overlay_visible(&self, visible: bool) {
        self.stats_overlay_visible.store(visible, Ordering::Relaxed);
        if let Ok(current) = self.stats_overlay.lock() {
            if let Some(overlay) = current.as_ref() {
                set_property_if_supported(overlay, "visible", visible);
                set_property_if_supported(overlay, "silent", !visible);
            }
        }
    }

    fn update_stats_overlay_text(&self, text: &str) {
        if let Ok(current) = self.stats_overlay.lock() {
            if let Some(overlay) = current.as_ref() {
                let visible =
                    self.stats_overlay_visible.load(Ordering::Relaxed) && !text.is_empty();
                overlay.set_property("text", text);
                set_property_if_supported(overlay, "visible", visible);
                set_property_if_supported(overlay, "silent", !visible);
            }
        }
    }

    pub(crate) fn record_decoded_buffer(&self) {
        self.last_decoded_ms.store(self.now_ms(), Ordering::Relaxed);
        self.decoded_total.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_sink_buffer(&self) {
        self.last_sink_ms.store(self.now_ms(), Ordering::Relaxed);
        self.sink_total.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn update_caps(&self, caps: &str) {
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

    pub(crate) fn set_rtp_video_src_pad(&self, pad: &gst::Pad) {
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

    pub(crate) fn set_post_decode_queue(&self, queue: gst::Element) {
        if let Ok(mut current) = self.post_decode_queue.lock() {
            *current = Some(queue);
        }
    }

    pub(crate) fn set_pre_decode_queue(&self, queue: gst::Element) {
        if let Ok(mut current) = self.pre_decode_queue.lock() {
            *current = Some(queue);
        }
    }

    pub(crate) fn set_decoder(&self, decoder: gst::Element) {
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
            .unwrap_or(DEFAULT_VIDEO_QUEUE_DEPTH)
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
pub(crate) struct VideoLivenessMonitor {
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
    pub(crate) fn configure(
        &self,
        context: &NativeStreamerSessionContext,
        target_bitrate_kbps: u32,
    ) {
        self.state.configure(context, target_bitrate_kbps);
    }

    pub(crate) fn update_hardware_acceleration(&self, value: impl Into<String>) {
        self.state.update_hardware_acceleration(value);
    }

    pub(crate) fn update_render_sink(&self, value: impl Into<String>) {
        self.state.update_render_sink(value);
    }

    pub(crate) fn record_encoded_buffer(&self, size: usize) {
        self.state.record_encoded_buffer(size);
    }

    pub(crate) fn record_audio_buffer(&self) {
        self.state.record_audio_buffer();
    }

    pub(crate) fn set_stats_overlay(&self, overlay: Option<gst::Element>) {
        self.state.set_stats_overlay(overlay);
    }

    pub(crate) fn set_stats_overlay_visible(&self, visible: bool) {
        self.state.set_stats_overlay_visible(visible);
    }

    pub(crate) fn record_decoded_buffer(&self) {
        self.state.record_decoded_buffer();
    }

    pub(crate) fn record_sink_buffer(&self) {
        self.state.record_sink_buffer();
    }

    pub(crate) fn update_caps(&self, caps: &str) {
        self.state.update_caps(caps);
    }

    pub(crate) fn set_rtp_video_src_pad(&self, pad: &gst::Pad) {
        self.state.set_rtp_video_src_pad(pad);
    }

    pub(crate) fn set_post_decode_queue(&self, queue: gst::Element) {
        self.state.set_post_decode_queue(queue);
    }

    pub(crate) fn set_pre_decode_queue(&self, queue: gst::Element) {
        self.state.set_pre_decode_queue(queue);
    }

    pub(crate) fn set_decoder(&self, decoder: gst::Element) {
        self.state.set_decoder(decoder);
    }

    pub(crate) fn log_first_encoded_once(&self) -> bool {
        self.state.log_first_encoded_once()
    }

    pub(crate) fn requested_fps(&self) -> Option<u32> {
        self.state.requested_fps()
    }

    pub(crate) fn warn_framerate_mismatch_once(&self) -> bool {
        self.state.warn_framerate_mismatch_once()
    }

    pub(crate) fn record_present_pacing_change(&self) {
        self.state.record_present_pacing_change();
    }

    pub(crate) fn stop_flag(&self) -> Arc<AtomicBool> {
        self.stop.clone()
    }

    pub(crate) fn record_transition(
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
        self.state.record_transition(
            transition_type,
            source,
            old_caps,
            new_caps,
            old_framerate,
            new_framerate,
            old_memory_mode,
            new_memory_mode,
            event_sender,
        );
    }

    pub(crate) fn start(
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

    pub(crate) fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        self.started.store(false, Ordering::SeqCst);
        let handle = self.thread.lock().ok().and_then(|mut slot| slot.take());
        if let Some(handle) = handle {
            let _ = handle.join();
        }
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
                if state.queue_depth() > DEFAULT_VIDEO_QUEUE_DEPTH {
                    state.set_queue_depth(
                        DEFAULT_VIDEO_QUEUE_DEPTH,
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
pub(crate) enum TransitionFlushKind {
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
    let render_sink = state
        .render_sink
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
    let render_path = if render_sink.is_empty() {
        if hardware_acceleration.is_empty() {
            memory_path.clone()
        } else {
            format!("{hardware_acceleration} {memory_path}")
        }
    } else if hardware_acceleration.is_empty() {
        format!("{render_sink} {memory_path}")
    } else {
        format!("{render_sink} ({hardware_acceleration}) {memory_path}")
    };
    let text = format!(
        "{codec} {resolution}  Render: {render_path}\n{:.1}/{:.1} Mbps  Bit {:.0}%  Decode {:.0}fps  Present {:.0}fps  Drop {:.2}%",
        bitrate_mbps,
        target_mbps,
        bitrate_performance_percent,
        rates.decoded_fps,
        rates.sink_fps,
        drop_percent,
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

pub(crate) fn watch_audio_activity(sink: &gst::Element, video_liveness: &VideoLivenessMonitor) {
    let Some(sink_pad) = sink.static_pad("sink") else {
        return;
    };
    let monitor = video_liveness.clone();
    sink_pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
        monitor.record_audio_buffer();
        gst::PadProbeReturn::Ok
    });
}

pub(crate) fn watch_first_sink_buffer(
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
    #[cfg(target_os = "linux")]
    let sink_for_refresh = sink.clone();
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
            #[cfg(target_os = "linux")]
            crate::gstreamer_platform::linux_x11_refresh_video_overlay(&sink_for_refresh);
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

pub(crate) fn watch_rtp_video_bitrate(
    pad: &gst::Pad,
    video_liveness: VideoLivenessMonitor,
    event_sender: &Option<Sender<Event>>,
) {
    let sender = event_sender.clone();
    pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, info| {
        if let Some(buffer) = info.buffer() {
            video_liveness.record_encoded_buffer(buffer.size());
            if video_liveness.log_first_encoded_once() {
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

pub(crate) fn watch_video_sink_rate(
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

pub(crate) fn watch_video_decoded_rate(
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

pub(crate) fn watch_video_caps_transitions(
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
            monitor.record_transition(
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

pub(crate) fn watch_video_sink_caps_transitions(
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
            monitor.record_transition(
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

pub(crate) fn install_present_limiter(
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
                monitor.record_present_pacing_change();
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
pub(crate) enum VideoLivenessPadKind {
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
                .and_then(|(monitor, _)| monitor.requested_fps());
            let requested_fps_summary = requested_fps
                .map(|fps| format!("; requestedFps={fps}"))
                .unwrap_or_default();
            if let (Some((monitor, _)), Some(requested_fps), Some(caps_framerate_value)) = (
                video_liveness.as_ref(),
                requested_fps,
                caps_framerate_summary(&caps),
            ) {
                let expected = format!("{requested_fps}/1");
                if caps_framerate_value != expected && monitor.warn_framerate_mismatch_once() {
                    monitor.record_transition(
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

pub(crate) fn sink_stats_summary(sink: &gst::Element) -> String {
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

pub(crate) fn caps_framerate_summary(caps: &str) -> Option<String> {
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

pub(crate) fn memory_mode_from_caps(caps: &str) -> &'static str {
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

pub(crate) fn is_zero_copy_memory_mode(memory_mode: &str) -> bool {
    matches!(
        memory_mode,
        "D3D12Memory" | "D3D11Memory" | "VulkanImage" | "VAMemory" | "GLMemory"
    )
}
