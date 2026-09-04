mod audio;
mod embedded;
mod mailbox;
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

use crate::failure::{BackendFailure, FailureReporter, VideoDecodeLoss};
use crate::format::{
    AudioFormat, Av1Format, BackendConfig, EmbeddedBackendConfig, FormatError, FrameTiming,
    H264Format, H264Framing, H265Format, RendererRect, ScreenRect, VideoFormat,
    access_unit_to_avcc,
};
use crate::lifecycle::{BackendState, Lifecycle};
use crate::queue::{BoundedQueue, PushResult};

use self::audio::AudioPipeline;
pub use self::embedded::{
    AdoptedMetalContext, EmbeddedFrameProducer, MetalFrame, MetalRecordedFrame,
};
use self::mailbox::LatestMailbox;
use self::presentation::PresenterHandle;
use self::surface::SurfaceOwner;
use self::video::{DecodedFrameOutput, VideoDecoder};

const MAX_OPUS_PACKET_BYTES: usize = 1_275;

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

/// Activates the standalone stream window as a regular macOS application.
/// LaunchServices starts it in the background for the protocol handshake; it
/// becomes the menu-bar and input owner only when media makes the window visible.
pub fn activate_stream_application() {
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

    let Some(main_thread) = MainThreadMarker::new() else {
        return;
    };
    let application = NSApplication::sharedApplication(main_thread);
    application.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    application.finishLaunching();
    #[allow(deprecated)]
    application.activateIgnoringOtherApps(true);
}

#[derive(Debug, Error)]
pub enum BackendError {
    #[error(transparent)]
    Format(#[from] FormatError),
    #[error("the operation must run on the AppKit main thread")]
    MainThreadRequired,
    #[error("the backend is stopping or stopped")]
    Stopped,
    #[error("video access unit is {actual} bytes; configured maximum is {maximum}")]
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
    probe_hardware_codec(H264_CODEC_TYPE)
}

pub fn probe_h265_hardware() -> bool {
    const H265_CODEC_TYPE: u32 = u32::from_be_bytes(*b"hvc1");
    probe_hardware_codec(H265_CODEC_TYPE)
}

pub fn probe_av1_hardware() -> bool {
    const AV1_CODEC_TYPE: u32 = u32::from_be_bytes(*b"av01");
    probe_hardware_codec(AV1_CODEC_TYPE)
}

fn probe_hardware_codec(codec_type: u32) -> bool {
    if unsafe { VTIsHardwareDecodeSupported(codec_type) } == 0 {
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
    pub video_submitted_bytes: u64,
    pub video_backpressured: u64,
    pub video_decoded: u64,
    pub video_decode_errors: u64,
    pub video_frames_dropped: u64,
    pub video_decoded_queue_dropped: u64,
    pub video_metal_submitted: u64,
    pub video_display_underflows: u64,
    pub video_scanout_skipped: u64,
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
    video_submitted_bytes: AtomicU64,
    video_backpressured: AtomicU64,
    video_decoded: AtomicU64,
    video_decode_errors: AtomicU64,
    video_frames_dropped: AtomicU64,
    video_decoded_queue_dropped: AtomicU64,
    video_metal_submitted: AtomicU64,
    video_display_underflows: AtomicU64,
    video_scanout_skipped: AtomicU64,
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
            video_submitted_bytes: self.video_submitted_bytes.load(Ordering::Relaxed),
            video_backpressured: self.video_backpressured.load(Ordering::Relaxed),
            video_decoded: self.video_decoded.load(Ordering::Relaxed),
            video_decode_errors: self.video_decode_errors.load(Ordering::Relaxed),
            video_frames_dropped: self.video_frames_dropped.load(Ordering::Relaxed),
            video_decoded_queue_dropped: self.video_decoded_queue_dropped.load(Ordering::Relaxed),
            video_metal_submitted: self.video_metal_submitted.load(Ordering::Relaxed),
            video_display_underflows: self.video_display_underflows.load(Ordering::Relaxed),
            video_scanout_skipped: self.video_scanout_skipped.load(Ordering::Relaxed),
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
        self.submit_video(access_unit, framing, timing)
    }

    pub fn submit_h265(
        &self,
        access_unit: &[u8],
        framing: H264Framing,
        timing: FrameTiming,
    ) -> Result<SubmitOutcome, BackendError> {
        self.submit_video(access_unit, framing, timing)
    }

    pub fn submit_av1(
        &self,
        access_unit: &[u8],
        timing: FrameTiming,
    ) -> Result<SubmitOutcome, BackendError> {
        if access_unit.is_empty() {
            return Err(FormatError::EmptyAccessUnit.into());
        }
        self.submit_packetized_video(access_unit, timing)
    }

    fn submit_video(
        &self,
        access_unit: &[u8],
        framing: H264Framing,
        timing: FrameTiming,
    ) -> Result<SubmitOutcome, BackendError> {
        let avcc = access_unit_to_avcc(access_unit, framing)?;
        self.submit_packetized_video(&avcc, timing)
    }

    fn submit_packetized_video(
        &self,
        packetized_access_unit: &[u8],
        timing: FrameTiming,
    ) -> Result<SubmitOutcome, BackendError> {
        if self.shared.lifecycle.state() != BackendState::Running {
            return Err(BackendError::Stopped);
        }
        if self.shared.paused.load(Ordering::Acquire) {
            return Ok(SubmitOutcome::Paused);
        }
        timing.validate()?;
        if packetized_access_unit.len() > self.shared.max_video_access_unit_bytes {
            return Err(BackendError::AccessUnitTooLarge {
                actual: packetized_access_unit.len(),
                maximum: self.shared.max_video_access_unit_bytes,
            });
        }
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
        if !decoder.submit(packetized_access_unit, timing)? {
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
        self.shared
            .counters
            .video_submitted_bytes
            .fetch_add(packetized_access_unit.len() as u64, Ordering::Relaxed);
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
        self.reconfigure_video(format.into())
    }

    pub fn reconfigure_h265(&self, format: H265Format) -> Result<(), BackendError> {
        self.reconfigure_video(format.into())
    }

    pub fn reconfigure_av1(&self, format: Av1Format) -> Result<(), BackendError> {
        self.reconfigure_video(format.into())
    }

    fn reconfigure_video(&self, format: VideoFormat) -> Result<(), BackendError> {
        if self.shared.lifecycle.state() != BackendState::Running {
            return Err(BackendError::Stopped);
        }
        let replacement = VideoDecoder::new(
            &format,
            self.shared.video_output.clone(),
            Arc::clone(&self.shared.counters),
            Arc::clone(&self.shared.failures),
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
        let discarded = self.shared.video_output.clear();
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
            Arc::clone(&self.shared.failures),
        )?);
        Ok(())
    }

    pub fn state(&self) -> BackendState {
        self.shared.lifecycle.state()
    }

    pub fn stats(&self) -> BackendStats {
        self.shared.counters.snapshot()
    }

    pub fn pop_video_decode_loss(&self) -> Option<VideoDecodeLoss> {
        self.shared.failures.pop_video_decode_loss()
    }

    pub fn fatal_failure(&self) -> Option<BackendFailure> {
        self.shared.failures.fatal_failure()
    }
}

struct Shared {
    lifecycle: Lifecycle,
    paused: AtomicBool,
    counters: Arc<Counters>,
    failures: Arc<FailureReporter>,
    video_output: DecodedFrameOutput,
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
        let discarded = self.video_output.clear();
        self.counters
            .video_frames_dropped
            .fetch_add(discarded as u64, Ordering::Relaxed);
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
    frame_producer: Option<EmbeddedFrameProducer>,
    _main_thread_only: PhantomData<Rc<()>>,
}

impl MacOsBackend {
    pub fn start(config: BackendConfig) -> Result<Self, BackendError> {
        let main_thread = MainThreadMarker::new().ok_or(BackendError::MainThreadRequired)?;
        config.validate()?;
        let surface = SurfaceOwner::attach(config.surface, main_thread)?;
        let counters = Arc::new(Counters::default());
        let failures = Arc::new(FailureReporter::default());
        let video_queue = Arc::new(BoundedQueue::new(config.queues.decoded_video_frames));
        let video_output = DecodedFrameOutput::PresentationQueue(Arc::clone(&video_queue));
        let presenter = PresenterHandle::start(
            surface.metal_layer(),
            surface.presentation_visibility(),
            Arc::clone(&video_queue),
            Arc::clone(&counters),
            Arc::clone(&failures),
        )?;
        let video = VideoDecoder::new(
            &config.video,
            video_output.clone(),
            Arc::clone(&counters),
            Arc::clone(&failures),
            config.queues.video_frames_in_flight,
        )?;
        let audio = AudioPipeline::start(
            config.audio,
            config.queues.opus_packets,
            config.queues.pcm_milliseconds,
            Arc::clone(&counters),
            Arc::clone(&failures),
        )?;
        let shared = Arc::new(Shared {
            lifecycle: Lifecycle::running(),
            paused: AtomicBool::new(false),
            counters,
            failures,
            video_output,
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
            frame_producer: None,
            _main_thread_only: PhantomData,
        })
    }

    /// Starts VideoToolbox and CoreAudio for a shell-owned Qt/Metal renderer.
    ///
    /// This path creates no AppKit object, SDL window, `CAMetalLayer`, `CVDisplayLink`, Metal
    /// device, or Metal command queue. Decoded IOSurface-backed frames are retained in a
    /// latest-frame mailbox until [`EmbeddedFrameProducer::acquire_latest`] transfers them to Qt's
    /// render thread.
    pub fn start_embedded(config: EmbeddedBackendConfig) -> Result<Self, BackendError> {
        Self::start_embedded_inner(config, None::<fn(MetalFrame) -> bool>)
    }

    /// Starts embedded output and forwards each newest retained frame to a shell mailbox.
    ///
    /// The publisher runs on VideoToolbox's callback thread and must return promptly. Returning
    /// `true` indicates that the shell replaced an older unconsumed frame.
    pub fn start_embedded_with_publisher(
        config: EmbeddedBackendConfig,
        publish: impl Fn(MetalFrame) -> bool + Send + Sync + 'static,
    ) -> Result<Self, BackendError> {
        Self::start_embedded_inner(config, Some(publish))
    }

    fn start_embedded_inner(
        config: EmbeddedBackendConfig,
        publish: Option<impl Fn(MetalFrame) -> bool + Send + Sync + 'static>,
    ) -> Result<Self, BackendError> {
        config.validate()?;
        let counters = Arc::new(Counters::default());
        let failures = Arc::new(FailureReporter::default());
        let mailbox = Arc::new(LatestMailbox::new());
        let frame_producer = EmbeddedFrameProducer::new(mailbox, Arc::clone(&counters));
        let frame_available = publish.map(|publish| {
            let producer = frame_producer.clone();
            Arc::new(move || {
                if let Some(frame) = producer.acquire_latest() {
                    let replaced = publish(frame);
                    if replaced {
                        let counters = producer.counters();
                        counters
                            .video_decoded_queue_dropped
                            .fetch_add(1, Ordering::Relaxed);
                        counters
                            .video_frames_dropped
                            .fetch_add(1, Ordering::Relaxed);
                    }
                }
            }) as Arc<dyn Fn() + Send + Sync>
        });
        let video_output = DecodedFrameOutput::EmbeddedMailbox {
            mailbox: Arc::clone(frame_producer.mailbox()),
            frame_available,
        };
        let video = VideoDecoder::new(
            &config.video,
            video_output.clone(),
            Arc::clone(&counters),
            Arc::clone(&failures),
            config.queues.video_frames_in_flight,
        )?;
        let audio = AudioPipeline::start(
            config.audio,
            config.queues.opus_packets,
            config.queues.pcm_milliseconds,
            Arc::clone(&counters),
            Arc::clone(&failures),
        )?;
        let shared = Arc::new(Shared {
            lifecycle: Lifecycle::running(),
            paused: AtomicBool::new(false),
            counters,
            failures,
            video_output,
            video: Mutex::new(Some(video)),
            audio: Mutex::new(Some(audio)),
            presenter: Mutex::new(None),
            video_frames_in_flight: config.queues.video_frames_in_flight,
            opus_packets: config.queues.opus_packets,
            pcm_milliseconds: config.queues.pcm_milliseconds,
            max_video_access_unit_bytes: config.queues.max_video_access_unit_bytes,
        });
        Ok(Self {
            shared,
            surface: None,
            frame_producer: Some(frame_producer),
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

    pub fn frame_producer(&self) -> Option<EmbeddedFrameProducer> {
        self.frame_producer.clone()
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

    /// Repositions the process-owned passive overlay using absolute shell screen coordinates.
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

    pub fn fatal_failure(&self) -> Option<BackendFailure> {
        self.shared.failures.fatal_failure()
    }

    pub fn set_paused(&mut self, paused: bool) -> Result<(), BackendError> {
        if self.surface.is_some() && MainThreadMarker::new().is_none() {
            return Err(BackendError::MainThreadRequired);
        }
        if self.shared.lifecycle.state() != BackendState::Running {
            return Err(BackendError::Stopped);
        }
        if paused {
            self.shared.paused.store(true, Ordering::Release);
        }
        let discarded = self.shared.video_output.clear();
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
