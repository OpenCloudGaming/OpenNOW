use crate::protocol::{NativeQueueMode, StreamSettings, VideoTransitionEvent};

pub(crate) const DEFAULT_VIDEO_QUEUE_DEPTH: u32 = 1;

#[derive(Debug, Clone)]
pub(crate) struct TransitionSnapshot {
    pub(crate) transition_type: String,
    pub(crate) source: String,
    pub(crate) at_ms: u64,
    pub(crate) old_caps: Option<String>,
    pub(crate) new_caps: Option<String>,
    pub(crate) old_framerate: Option<String>,
    pub(crate) new_framerate: Option<String>,
    pub(crate) old_memory_mode: Option<String>,
    pub(crate) new_memory_mode: Option<String>,
    pub(crate) render_gap_ms: Option<u64>,
    pub(crate) requested_fps: Option<u32>,
    pub(crate) caps_framerate: Option<String>,
    pub(crate) high_fps_risk: bool,
    pub(crate) queue_mode: NativeQueueMode,
    pub(crate) summary: String,
}

impl TransitionSnapshot {
    pub(crate) fn to_event(&self) -> VideoTransitionEvent {
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
pub(crate) struct TransitionTelemetry {
    pub(crate) queue_mode: NativeQueueMode,
    pub(crate) queue_depth: u32,
    pub(crate) queue_depth_changes: u32,
    pub(crate) present_pacing_changes: u32,
    pub(crate) partial_flush_count: u32,
    pub(crate) complete_flush_count: u32,
    pub(crate) last_transition: Option<TransitionSnapshot>,
}

impl Default for TransitionTelemetry {
    fn default() -> Self {
        Self {
            queue_mode: NativeQueueMode::Auto,
            queue_depth: DEFAULT_VIDEO_QUEUE_DEPTH,
            queue_depth_changes: 0,
            present_pacing_changes: 0,
            partial_flush_count: 0,
            complete_flush_count: 0,
            last_transition: None,
        }
    }
}

pub(crate) fn resolve_queue_mode(settings: &StreamSettings) -> NativeQueueMode {
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

pub(crate) fn format_transition_summary(
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
