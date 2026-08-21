mod audio;
mod presentation;
mod surface;
mod video;

use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use objc2::MainThreadMarker;
use objc2_metal::{MTLCreateSystemDefaultDevice, MTLDevice};
use thiserror::Error;

use crate::format::{
    AudioFormat, BackendConfig, FormatError, FrameTiming, H264Format, H264Framing, RendererRect,
    ScreenRect, access_unit_to_avcc,
};
use crate::lifecycle::{BackendState, Lifecycle};
use crate::queue::{BoundedQueue, PushResult};

use self::audio::AudioPipeline;
use self::presentation::PresenterHandle;
use self::surface::SurfaceOwner;
use self::video::{DecodedFrame, VideoDecoder};

const MAX_OPUS_PACKET_BYTES: usize = 1_275;

/// Shows a standalone overlay window through the exact production creation path.
/// Debug-only aid for isolating window-server behavior without a streaming session.
pub fn debug_show_overlay_window() {
    let Some(main_thread) = MainThreadMarker::new() else {
        return;
    };
    surface::debug_overlay_window(main_thread);
}

/// Drains pending AppKit events and window-server work on the main thread.
///
/// The streamer's main thread runs the host command loop instead of `NSApplication.run()`, so
/// window ordering, compositing, and window controls only make progress when this is called.
pub fn pump_app_events() {
    use objc2_app_kit::{NSApplication, NSEventMask};

    let Some(main_thread) = MainThreadMarker::new() else {
        return;
    };
    let application = NSApplication::sharedApplication(main_thread);
    unsafe {
        while let Some(event) = application.nextEventMatchingMask_untilDate_inMode_dequeue(
            NSEventMask::Any,
            None,
            objc2_foundation::NSDefaultRunLoopMode,
            true,
        ) {
            application.sendEvent(&event);
        }
    }
    application.updateWindows();
}

#[derive(Debug, Error)]
pub enum BackendError {
    #[error(transparent)]
    Format(#[from] FormatError),
    #[error("the operation must run on the AppKit main thread")]
    MainThreadRequired,
    #[error("the backend is stopping or stopped")]
    Stopped,
    #[error("H.264 access unit is {actual} bytes; configured maximum is {maximum}")]
    AccessUnitTooLarge { actual: usize, maximum: usize },
    #[error("Opus packet is {0} bytes; the maximum is 1275")]
    OpusPacketTooLarge(usize),
    #[error("Opus packet is empty")]
    EmptyOpusPacket,
    #[error("{api} failed with OSStatus {status}")]
    AppleApi { api: &'static str, status: i32 },
    #[error("{0}")]
    Metal(String),
    #[error("Opus decoder failed: {0}")]
    Opus(String),
    #[error("failed to start {0} worker thread")]
    Thread(&'static str),
    #[error("the supplied NSWindow has no content view")]
    MissingContentView,
    #[error("surface layout updates require a supplied NSWindow target")]
    NotWindowSurface,
    #[error("surface updates require an owned overlay target")]
    NotOwnedOverlay,
    #[error("macOS did not report a primary screen")]
    MissingPrimaryScreen,
}

#[link(name = "VideoToolbox", kind = "framework")]
unsafe extern "C" {
    fn VTIsHardwareDecodeSupported(codec_type: u32) -> u8;
}

pub fn probe_h264_hardware() -> bool {
    const H264_CODEC_TYPE: u32 = u32::from_be_bytes(*b"avc1");
    if unsafe { VTIsHardwareDecodeSupported(H264_CODEC_TYPE) } == 0 {
        return false;
    }
    MTLCreateSystemDefaultDevice().is_some_and(|device| device.newCommandQueue().is_some())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubmitOutcome {
    Accepted,
    ReplacedOldest,
    Backpressured,
    Paused,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BackendStats {
    pub video_submitted: u64,
    pub video_backpressured: u64,
    pub video_decoded: u64,
    pub video_decode_errors: u64,
    pub video_frames_dropped: u64,
    pub video_presented: u64,
    pub video_present_errors: u64,
    pub opus_submitted: u64,
    pub opus_packets_dropped: u64,
    pub opus_decode_errors: u64,
    pub pcm_samples_dropped: u64,
    pub pcm_underrun_frames: u64,
}

#[derive(Default)]
pub(super) struct Counters {
    video_submitted: AtomicU64,
    video_backpressured: AtomicU64,
    video_decoded: AtomicU64,
    video_decode_errors: AtomicU64,
    video_frames_dropped: AtomicU64,
    video_presented: AtomicU64,
    video_present_errors: AtomicU64,
    opus_submitted: AtomicU64,
    opus_packets_dropped: AtomicU64,
    opus_decode_errors: AtomicU64,
    pcm_samples_dropped: AtomicU64,
    pcm_underrun_frames: AtomicU64,
}

impl Counters {
    fn snapshot(&self) -> BackendStats {
        BackendStats {
            video_submitted: self.video_submitted.load(Ordering::Relaxed),
            video_backpressured: self.video_backpressured.load(Ordering::Relaxed),
            video_decoded: self.video_decoded.load(Ordering::Relaxed),
            video_decode_errors: self.video_decode_errors.load(Ordering::Relaxed),
            video_frames_dropped: self.video_frames_dropped.load(Ordering::Relaxed),
            video_presented: self.video_presented.load(Ordering::Relaxed),
            video_present_errors: self.video_present_errors.load(Ordering::Relaxed),
            opus_submitted: self.opus_submitted.load(Ordering::Relaxed),
            opus_packets_dropped: self.opus_packets_dropped.load(Ordering::Relaxed),
            opus_decode_errors: self.opus_decode_errors.load(Ordering::Relaxed),
            pcm_samples_dropped: self.pcm_samples_dropped.load(Ordering::Relaxed),
            pcm_underrun_frames: self.pcm_underrun_frames.load(Ordering::Relaxed),
        }
    }
}

/// Raw AppKit handles owned or retained by a running backend.
///
/// Both pointers remain valid only until [`MacOsBackend::stop`] or `Drop`. Callers must use them
/// on AppKit's main thread and must not transfer ownership or release them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeSurfaceHandle {
    pub ns_window: Option<std::ptr::NonNull<std::ffi::c_void>>,
    /// The dedicated presentation view. For `SurfaceTarget::NsWindow`, this is the backend-owned
    /// passive child view, never the supplied window's content view.
    pub ns_view: std::ptr::NonNull<std::ffi::c_void>,
}

/// Thread-safe encoded media input for a running [`MacOsBackend`].
#[derive(Clone)]
pub struct StreamSink {
    shared: Arc<Shared>,
}

impl StreamSink {
    pub fn submit_h264(
        &self,
        access_unit: &[u8],
        framing: H264Framing,
        timing: FrameTiming,
    ) -> Result<SubmitOutcome, BackendError> {
        if self.shared.lifecycle.state() != BackendState::Running {
            return Err(BackendError::Stopped);
        }
        if self.shared.paused.load(Ordering::Acquire) {
            return Ok(SubmitOutcome::Paused);
        }
        timing.validate()?;
        if access_unit.len() > self.shared.max_video_access_unit_bytes {
            return Err(BackendError::AccessUnitTooLarge {
                actual: access_unit.len(),
                maximum: self.shared.max_video_access_unit_bytes,
            });
        }
        let avcc = access_unit_to_avcc(access_unit, framing)?;
        let decoder = self
            .shared
            .video
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if self.shared.lifecycle.state() != BackendState::Running {
            return Err(BackendError::Stopped);
        }
        if self.shared.paused.load(Ordering::Acquire) {
            return Ok(SubmitOutcome::Paused);
        }
        let decoder = decoder.as_ref().ok_or(BackendError::Stopped)?;
        if !decoder.submit(&avcc, timing)? {
            self.shared
                .counters
                .video_backpressured
                .fetch_add(1, Ordering::Relaxed);
            return Ok(SubmitOutcome::Backpressured);
        }
        self.shared
            .counters
            .video_submitted
            .fetch_add(1, Ordering::Relaxed);
        Ok(SubmitOutcome::Accepted)
    }

    pub fn submit_opus(&self, packet: &[u8]) -> Result<SubmitOutcome, BackendError> {
        if packet.is_empty() {
            return Err(BackendError::EmptyOpusPacket);
        }
        if packet.len() > MAX_OPUS_PACKET_BYTES {
            return Err(BackendError::OpusPacketTooLarge(packet.len()));
        }
        self.submit_opus_owned(packet.to_vec())
    }

    fn submit_opus_owned(&self, packet: Vec<u8>) -> Result<SubmitOutcome, BackendError> {
        if self.shared.lifecycle.state() != BackendState::Running {
            return Err(BackendError::Stopped);
        }
        if self.shared.paused.load(Ordering::Acquire) {
            return Ok(SubmitOutcome::Paused);
        }
        let audio = self
            .shared
            .audio
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if self.shared.lifecycle.state() != BackendState::Running {
            return Err(BackendError::Stopped);
        }
        if self.shared.paused.load(Ordering::Acquire) {
            return Ok(SubmitOutcome::Paused);
        }
        let audio = audio.as_ref().ok_or(BackendError::Stopped)?;
        let outcome = match audio.submit(packet) {
            PushResult::Pushed => SubmitOutcome::Accepted,
            PushResult::Replaced(_) => {
                self.shared
                    .counters
                    .opus_packets_dropped
                    .fetch_add(1, Ordering::Relaxed);
                SubmitOutcome::ReplacedOldest
            }
            PushResult::Closed(_) => return Err(BackendError::Stopped),
        };
        self.shared
            .counters
            .opus_submitted
            .fetch_add(1, Ordering::Relaxed);
        Ok(outcome)
    }

    pub fn reconfigure_h264(&self, format: H264Format) -> Result<(), BackendError> {
        if self.shared.lifecycle.state() != BackendState::Running {
            return Err(BackendError::Stopped);
        }
        let replacement = VideoDecoder::new(
            &format,
            self.shared.video_queue.clone(),
            Arc::clone(&self.shared.counters),
            self.shared.video_frames_in_flight,
        )?;
        let mut decoder = self
            .shared
            .video
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if self.shared.lifecycle.state() != BackendState::Running {
            drop(replacement);
            return Err(BackendError::Stopped);
        }
        let previous = decoder.replace(replacement);
        drop(previous);
        let discarded = self.shared.video_queue.clear();
        self.shared
            .counters
            .video_frames_dropped
            .fetch_add(discarded as u64, Ordering::Relaxed);
        drop(decoder);
        Ok(())
    }

    pub fn reconfigure_audio(&self, format: AudioFormat) -> Result<(), BackendError> {
        format.validate()?;
        if self.shared.lifecycle.state() != BackendState::Running {
            return Err(BackendError::Stopped);
        }
        let mut audio = self
            .shared
            .audio
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if self.shared.lifecycle.state() != BackendState::Running {
            return Err(BackendError::Stopped);
        }
        if let Some(previous) = audio.take() {
            previous.stop();
        }
        *audio = Some(AudioPipeline::start(
            format,
            self.shared.opus_packets,
            self.shared.pcm_milliseconds,
            Arc::clone(&self.shared.counters),
        )?);
        Ok(())
    }

    pub fn state(&self) -> BackendState {
        self.shared.lifecycle.state()
    }

    pub fn stats(&self) -> BackendStats {
        self.shared.counters.snapshot()
    }
}

struct Shared {
    lifecycle: Lifecycle,
    paused: AtomicBool,
    counters: Arc<Counters>,
    video_queue: Arc<BoundedQueue<DecodedFrame>>,
    video: Mutex<Option<VideoDecoder>>,
    audio: Mutex<Option<AudioPipeline>>,
    presenter: Mutex<Option<PresenterHandle>>,
    video_frames_in_flight: usize,
    opus_packets: usize,
    pcm_milliseconds: u32,
    max_video_access_unit_bytes: usize,
}

impl Shared {
    fn stop(&self) {
        if !self.lifecycle.begin_stop() {
            return;
        }
        let decoder = self
            .video
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        drop(decoder);
        if let Some(audio) = self
            .audio
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
        {
            audio.stop();
        }
        if let Some(presenter) = self
            .presenter
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
        {
            presenter.stop();
        }
        self.lifecycle.finish_stop();
    }
}

/// Main-thread owner for the native macOS backend.
pub struct MacOsBackend {
    shared: Arc<Shared>,
    surface: Option<SurfaceOwner>,
    _main_thread_only: PhantomData<Rc<()>>,
}

impl MacOsBackend {
    pub fn start(config: BackendConfig) -> Result<Self, BackendError> {
        let main_thread = MainThreadMarker::new().ok_or(BackendError::MainThreadRequired)?;
        config.validate()?;
        let surface = SurfaceOwner::attach(config.surface, main_thread)?;
        let counters = Arc::new(Counters::default());
        let video_queue = Arc::new(BoundedQueue::new(config.queues.decoded_video_frames));
        let presenter = PresenterHandle::start(
            surface.metal_layer(),
            surface.presentation_visibility(),
            Arc::clone(&video_queue),
            Arc::clone(&counters),
        )?;
        let video = VideoDecoder::new(
            &config.video,
            Arc::clone(&video_queue),
            Arc::clone(&counters),
            config.queues.video_frames_in_flight,
        )?;
        let audio = AudioPipeline::start(
            config.audio,
            config.queues.opus_packets,
            config.queues.pcm_milliseconds,
            Arc::clone(&counters),
        )?;
        let shared = Arc::new(Shared {
            lifecycle: Lifecycle::running(),
            paused: AtomicBool::new(false),
            counters,
            video_queue,
            video: Mutex::new(Some(video)),
            audio: Mutex::new(Some(audio)),
            presenter: Mutex::new(Some(presenter)),
            video_frames_in_flight: config.queues.video_frames_in_flight,
            opus_packets: config.queues.opus_packets,
            pcm_milliseconds: config.queues.pcm_milliseconds,
            max_video_access_unit_bytes: config.queues.max_video_access_unit_bytes,
        });
        Ok(Self {
            shared,
            surface: Some(surface),
            _main_thread_only: PhantomData,
        })
    }

    pub fn sink(&self) -> StreamSink {
        StreamSink {
            shared: Arc::clone(&self.shared),
        }
    }

    pub fn native_surface(&self) -> Option<NativeSurfaceHandle> {
        self.surface.as_ref().map(SurfaceOwner::native_handle)
    }

    /// Updates a supplied-window child surface in renderer-relative, top-left AppKit points.
    ///
    /// This method is available only for `SurfaceTarget::NsWindow` and returns
    /// `MainThreadRequired` instead of dispatching implicitly when called off AppKit's main thread.
    pub fn update_window_surface(
        &mut self,
        bounds: RendererRect,
        visible: bool,
    ) -> Result<(), BackendError> {
        let main_thread = MainThreadMarker::new().ok_or(BackendError::MainThreadRequired)?;
        if self.shared.lifecycle.state() != BackendState::Running {
            return Err(BackendError::Stopped);
        }
        bounds.validate()?;
        self.surface
            .as_mut()
            .ok_or(BackendError::Stopped)?
            .update_window_child(bounds, visible, main_thread)
    }

    /// Repositions the process-owned passive overlay using absolute Electron screen coordinates.
    pub fn update_owned_overlay(
        &mut self,
        screen_rect: ScreenRect,
        visible: bool,
    ) -> Result<(), BackendError> {
        let main_thread = MainThreadMarker::new().ok_or(BackendError::MainThreadRequired)?;
        if self.shared.lifecycle.state() != BackendState::Running {
            return Err(BackendError::Stopped);
        }
        screen_rect.validate()?;
        self.surface
            .as_mut()
            .ok_or(BackendError::Stopped)?
            .update_owned_overlay(screen_rect, visible, main_thread)
    }

    pub fn refresh_overlay_ordering(&mut self) -> Result<(), BackendError> {
        let _main_thread = MainThreadMarker::new().ok_or(BackendError::MainThreadRequired)?;
        if self.shared.lifecycle.state() != BackendState::Running {
            return Err(BackendError::Stopped);
        }
        self.surface
            .as_mut()
            .ok_or(BackendError::Stopped)?
            .refresh_overlay_ordering()
    }

    pub fn state(&self) -> BackendState {
        self.shared.lifecycle.state()
    }

    pub fn stats(&self) -> BackendStats {
        self.shared.counters.snapshot()
    }

    pub fn set_paused(&mut self, paused: bool) -> Result<(), BackendError> {
        let _main_thread = MainThreadMarker::new().ok_or(BackendError::MainThreadRequired)?;
        if self.shared.lifecycle.state() != BackendState::Running {
            return Err(BackendError::Stopped);
        }
        if paused {
            self.shared.paused.store(true, Ordering::Release);
        }
        let discarded = self.shared.video_queue.clear();
        self.shared
            .counters
            .video_frames_dropped
            .fetch_add(discarded as u64, Ordering::Relaxed);
        if let Some(audio) = self
            .shared
            .audio
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_mut()
        {
            audio.set_paused(paused)?;
        }
        if !paused {
            self.shared.paused.store(false, Ordering::Release);
        }
        Ok(())
    }

    pub fn stop(&mut self) {
        self.shared.stop();
        if let Some(mut surface) = self.surface.take() {
            surface.detach();
        }
    }
}

impl Drop for MacOsBackend {
    fn drop(&mut self) {
        self.stop();
    }
}

#[allow(dead_code)]
fn assert_stream_sink_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<StreamSink>();
}
