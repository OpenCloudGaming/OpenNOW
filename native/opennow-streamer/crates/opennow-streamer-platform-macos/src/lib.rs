//! Native macOS media backend for OpenNOW.
//!
//! `opennow-streamer-platform` conditionally uses this crate on macOS and passes encoded media
//! through [`StreamSink`]. Video is decoded by VideoToolbox into IOSurface
//! backed NV12 pixel buffers and sampled directly by Metal. Opus is decoded by the statically
//! built reference libopus implementation and written to a CoreAudio output unit as interleaved
//! `f32` PCM.
//!
//! # Threading and safety invariants
//!
//! - [`MacOsBackend::start`] must run on the AppKit main thread. `MacOsBackend` is deliberately
//!   `!Send` and owns all AppKit objects, so its `stop` and `Drop` paths also run on that thread.
//! - The native streamer integration uses only the process-owned overlay target and never
//!   dereferences a foreign-process AppKit pointer. A supplied [`BorrowedNsView`] must be dedicated to
//!   video because its layer is temporarily
//!   replaced. A supplied [`BorrowedNsWindow`] keeps its existing content view and renderer layer;
//!   the backend inserts a passive, non-focusable child view and removes it on shutdown.
//! - Supplied-window geometry and visibility changes go through
//!   [`MacOsBackend::update_window_surface`] and are applied only on the AppKit main thread.
//! - [`StreamSink`] is `Send + Sync`. VideoToolbox, CoreAudio, and Metal callbacks never borrow
//!   caller memory. Encoded access units and packets are copied before a submit call returns.
//! - Every media queue is bounded. Video admission rejects new work at the configured in-flight
//!   limit, decoded video and Opus queues drop their oldest item, and the PCM ring drops new
//!   samples rather than blocking CoreAudio's real-time callback.
//! - VideoToolbox callbacks retain immutable `CVPixelBuffer`s before enqueueing them. The Metal
//!   presenter retains `CVMetalTexture`s until their command buffer has completed. Shutdown waits
//!   for VideoToolbox callbacks and GPU work before releasing either callback context.

#![deny(unsafe_op_in_unsafe_fn)]
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

mod failure;
mod format;
mod lifecycle;
mod queue;
mod ring;

#[cfg(target_os = "macos")]
mod macos;

pub use failure::{BackendFailure, BackendSubsystem, VideoDecodeLoss};
pub use format::{
    AudioFormat, Av1Format, BackendConfig, BorrowedNsView, BorrowedNsWindow, EmbeddedBackendConfig,
    FrameTiming, H264Format, H264Framing, H264ParameterSets, H265Format, H265ParameterSets,
    OwnedOverlayConfig, QueueLimits, RendererRect, ScreenRect, SurfaceTarget, VideoColorSpace,
    VideoFormat, WindowSurfaceConfig,
};
pub use lifecycle::BackendState;

pub(crate) const fn overlay_should_be_ordered(
    requested_visible: bool,
    parent_frontmost: bool,
) -> bool {
    requested_visible && parent_frontmost
}

#[cfg(target_os = "macos")]
pub use macos::{
    AdoptedMetalContext, BackendError, BackendStats, EmbeddedFrameProducer, MacOsBackend,
    MetalFrame, MetalRecordedFrame, NativeSurfaceHandle, StreamSink, SubmitOutcome,
    activate_stream_application, probe_av1_hardware, probe_h264_hardware, probe_h265_hardware,
    pump_app_events,
};

#[cfg(test)]
mod tests {
    use super::overlay_should_be_ordered;

    #[test]
    fn overlay_requires_requested_visibility_and_frontmost_parent() {
        assert!(!overlay_should_be_ordered(false, false));
        assert!(!overlay_should_be_ordered(false, true));
        assert!(!overlay_should_be_ordered(true, false));
        assert!(overlay_should_be_ordered(true, true));
    }
}
