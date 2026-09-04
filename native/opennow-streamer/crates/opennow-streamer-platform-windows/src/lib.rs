#![deny(unsafe_op_in_unsafe_fn)]

mod format;
mod queue;

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::{
    AdoptedD3d11Context, D3d11Frame, D3d11FrameProducer, D3d11FrameSubmitter, D3d11RecordedFrame,
    D3d11TextureFormat,
};

use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;

use opennow_streamer_protocol::log;

use crate::queue::BoundedQueue;

pub use format::{
    AudioFormat, BackendConfig, Bounds, EncodedVideoFrame, ExistingWindow, OwnedWindow, PcmFrame,
    SurfaceTarget, VideoChromaFormat, VideoCodec, VideoFormat, VideoPixelFormat, WindowHandle,
};
pub use queue::PushOutcome;

/// Burst allowance for adaptive video delivery. The decoded presenter uses
/// the same bound and trims stale frames before presentation, keeping latency
/// low without treating ordinary game-FPS fluctuations as decoder loss.
pub const ADAPTIVE_VIDEO_QUEUE_CAPACITY: usize = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum LifecycleState {
    Starting = 0,
    Running = 1,
    Reconfiguring = 2,
    Recovering = 3,
    Stopping = 4,
    Stopped = 5,
    Failed = 6,
}

impl LifecycleState {
    fn from_raw(value: u8) -> Self {
        match value {
            0 => Self::Starting,
            1 => Self::Running,
            2 => Self::Reconfiguring,
            3 => Self::Recovering,
            4 => Self::Stopping,
            5 => Self::Stopped,
            _ => Self::Failed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subsystem {
    VideoDecode,
    VideoPresentation,
    Audio,
}

/// Graphics API that owns the Windows decode/presentation device. D3D12 uses
/// Microsoft's D3D11-on-12 layer so Media Foundation can keep its required
/// D3D11 device-manager contract while work is submitted to a D3D12 queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsGraphicsApi {
    D3d11,
    D3d12,
}

/// Restricts Media Foundation decoder discovery to the requested execution
/// class. Keeping this explicit prevents a synchronous software MFT from being
/// reported as hardware merely because it can publish D3D11 surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsDecoderMode {
    Hardware,
    Software,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendEvent {
    StateChanged(LifecycleState),
    FirstFramePresented,
    QueueOverflow(Subsystem),
    VideoFormatChanged(VideoFormat),
    DeviceLost {
        subsystem: Subsystem,
        message: String,
    },
    DeviceRecovered(Subsystem),
    KeyFrameRequired,
    Fatal(BackendError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityProbe {
    pub available: bool,
    pub h264_hardware_decode: bool,
    pub h265_hardware_decode: bool,
    pub av1_hardware_decode: bool,
    pub h264_software_decode: bool,
    pub h265_software_decode: bool,
    pub av1_software_decode: bool,
    pub d3d11_presentation: bool,
    pub wasapi_render: bool,
    pub reason: Option<String>,
}

impl CapabilityProbe {
    pub const fn bundled_backend_available(&self) -> bool {
        (self.h264_hardware_decode || self.h265_hardware_decode || self.av1_hardware_decode)
            && self.d3d11_presentation
            && self.wasapi_render
    }

    pub const fn software_backend_available(&self) -> bool {
        (self.h264_software_decode || self.h265_software_decode || self.av1_software_decode)
            && self.d3d11_presentation
            && self.wasapi_render
    }
}

#[cfg(any(windows, test))]
struct DefaultEndpointTracker {
    active_id: String,
    reported_id: Option<String>,
}

#[cfg(any(windows, test))]
impl DefaultEndpointTracker {
    fn new(active_id: String) -> Self {
        Self {
            active_id,
            reported_id: None,
        }
    }

    fn observe(&mut self, current_id: String) -> bool {
        if current_id == self.active_id {
            self.reported_id = None;
            return false;
        }
        if self.reported_id.as_ref() == Some(&current_id) {
            return false;
        }
        self.reported_id = Some(current_id);
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendError {
    UnsupportedPlatform,
    InvalidConfig(String),
    InvalidFrame(String),
    NotRunning(LifecycleState),
    Startup(String),
    Reconfigure(String),
    DeviceLost {
        subsystem: Subsystem,
        message: String,
    },
    WorkerDisconnected,
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedPlatform => {
                formatter.write_str("the Windows media backend is unavailable on this platform")
            }
            Self::InvalidConfig(message) => {
                write!(formatter, "invalid backend configuration: {message}")
            }
            Self::InvalidFrame(message) => write!(formatter, "invalid media frame: {message}"),
            Self::NotRunning(state) => write!(formatter, "backend is not running ({state:?})"),
            Self::Startup(message) => write!(formatter, "backend startup failed: {message}"),
            Self::Reconfigure(message) => {
                write!(formatter, "backend reconfiguration failed: {message}")
            }
            Self::DeviceLost { subsystem, message } => {
                write!(formatter, "{subsystem:?} device was lost: {message}")
            }
            Self::WorkerDisconnected => formatter.write_str("backend worker disconnected"),
        }
    }
}

impl std::error::Error for BackendError {}

#[derive(Debug)]
#[cfg_attr(not(windows), allow(dead_code))]
enum Control {
    ReconfigureVideo(VideoFormat),
    ReconfigureAudio(AudioFormat),
    SetSurface(SurfaceTarget),
    SetPaused(bool),
    Stop,
}

#[derive(Debug)]
struct Shared {
    video: BoundedQueue<EncodedVideoFrame>,
    audio: BoundedQueue<PcmFrame>,
    video_format: Mutex<VideoFormat>,
    audio_format: Mutex<AudioFormat>,
    surface: Mutex<SurfaceTarget>,
    state: AtomicU8,
    paused: std::sync::atomic::AtomicBool,
    events: BoundedQueue<BackendEvent>,
    presented_frames: AtomicU64,
}

impl Shared {
    fn state(&self) -> LifecycleState {
        LifecycleState::from_raw(self.state.load(Ordering::Acquire))
    }

    fn set_state(&self, state: LifecycleState) {
        self.state.store(state as u8, Ordering::Release);
        let _ = self.events.push(BackendEvent::StateChanged(state));
    }
}

pub struct WindowsBackend {
    shared: Arc<Shared>,
    control: mpsc::Sender<Control>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl WindowsBackend {
    pub fn probe() -> CapabilityProbe {
        Self::probe_for(WindowsGraphicsApi::D3d11)
    }

    pub fn probe_for(api: WindowsGraphicsApi) -> CapabilityProbe {
        #[cfg(windows)]
        {
            windows::probe(api)
        }
        #[cfg(not(windows))]
        {
            let _ = api;
            CapabilityProbe {
                available: false,
                h264_hardware_decode: false,
                h265_hardware_decode: false,
                av1_hardware_decode: false,
                h264_software_decode: false,
                h265_software_decode: false,
                av1_software_decode: false,
                d3d11_presentation: false,
                wasapi_render: false,
                reason: Some("the current target is not Windows".to_owned()),
            }
        }
    }

    pub fn start(config: BackendConfig) -> Result<Self, BackendError> {
        Self::start_for(WindowsGraphicsApi::D3d11, config)
    }

    pub fn start_for(api: WindowsGraphicsApi, config: BackendConfig) -> Result<Self, BackendError> {
        Self::start_for_mode(api, WindowsDecoderMode::Hardware, config)
    }

    pub fn start_for_mode(
        api: WindowsGraphicsApi,
        decoder_mode: WindowsDecoderMode,
        config: BackendConfig,
    ) -> Result<Self, BackendError> {
        config.validate()?;
        let video = config.video;
        let describe = || {
            let fps = video.frame_rate_numerator.get() as f64
                / video.frame_rate_denominator.get() as f64;
            format!(
                "windows backend starting (api={api:?} decoder={decoder_mode:?} codec={} {}x{} {:.0}fps {}kbps pixel={:?} chroma={:?} full_range={})",
                video.codec.label(),
                video.width,
                video.height,
                fps,
                video.average_bitrate,
                video.pixel_format,
                video.chroma_format,
                video.full_range,
            )
        };

        #[cfg(not(windows))]
        {
            let _ = api;
            let _ = (decoder_mode, config);
            log::log_line("WARN", "decode", "windows backend requested on non-windows build");
            Err(BackendError::UnsupportedPlatform)
        }

        #[cfg(windows)]
        {
            log::log_line("INFO", "decode", &describe());
            let shared = Arc::new(Shared {
                video: BoundedQueue::new(config.video_queue_capacity),
                audio: BoundedQueue::new(config.audio_queue_capacity),
                video_format: Mutex::new(config.video),
                audio_format: Mutex::new(config.audio),
                surface: Mutex::new(config.surface),
                state: AtomicU8::new(LifecycleState::Starting as u8),
                paused: std::sync::atomic::AtomicBool::new(false),
                events: BoundedQueue::new(64),
                presented_frames: AtomicU64::new(0),
            });
            let (control_sender, control_receiver) = mpsc::channel();
            let worker = match windows::spawn(
                api,
                decoder_mode,
                config,
                Arc::clone(&shared),
                control_receiver,
            ) {
                Ok(worker) => worker,
                Err(error) => {
                    log::log_line("WARN", "decode", &format!("windows backend spawn failed: {error:?}"));
                    return Err(error);
                }
            };
            log::log_line("INFO", "decode", "windows backend worker spawned");
            Ok(Self {
                shared,
                control: control_sender,
                worker: Mutex::new(Some(worker)),
            })
        }
    }

    pub fn state(&self) -> LifecycleState {
        self.shared.state()
    }

    pub fn presented_frames(&self) -> u64 {
        self.shared.presented_frames.load(Ordering::Relaxed)
    }

    pub fn submit_video(&self, frame: EncodedVideoFrame) -> Result<PushOutcome, BackendError> {
        if let Err(error) = frame.validate().and(self.ensure_media_accepting()) {
            log::log_throttled(
                "submit-video-reject",
                "WARN",
                "decode",
                &format!("video frame rejected: {error:?}"),
            );
            return Err(error);
        }
        if self.shared.paused.load(Ordering::Acquire) {
            return Ok(PushOutcome::Paused);
        }
        let key_frame = frame.key_frame;
        let outcome = self
            .shared
            .video
            .push_or_clear_on_overflow(frame, key_frame)
            .map_err(|_| BackendError::NotRunning(self.state()))?;
        if outcome == PushOutcome::DroppedOldest {
            let _ = self
                .shared
                .events
                .push(BackendEvent::QueueOverflow(Subsystem::VideoDecode));
            log::log_throttled(
                "submit-video-overflow",
                "WARN",
                "decode",
                "video queue overflow, oldest frame dropped",
            );
        }
        Ok(outcome)
    }

    pub fn submit_audio(&self, frame: PcmFrame) -> Result<PushOutcome, BackendError> {
        frame.validate()?;
        self.ensure_media_accepting()?;
        if self.shared.paused.load(Ordering::Acquire) {
            return Ok(PushOutcome::Paused);
        }
        let expected_format = *self
            .shared
            .audio_format
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if frame.format != expected_format {
            return Err(BackendError::InvalidFrame(format!(
                "PCM packet format {:?} does not match active format {:?}",
                frame.format, expected_format
            )));
        }
        let outcome = self
            .shared
            .audio
            .push(frame)
            .map_err(|_| BackendError::NotRunning(self.state()))?;
        if outcome == PushOutcome::DroppedOldest {
            let _ = self
                .shared
                .events
                .push(BackendEvent::QueueOverflow(Subsystem::Audio));
        }
        Ok(outcome)
    }

    pub fn reconfigure_video(&self, format: VideoFormat) -> Result<(), BackendError> {
        format.validate()?;
        self.ensure_controllable()?;
        self.shared.video.clear();
        *self
            .shared
            .video_format
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = format;
        self.shared.set_state(LifecycleState::Reconfiguring);
        self.control
            .send(Control::ReconfigureVideo(format))
            .map_err(|_| BackendError::WorkerDisconnected)
    }

    pub fn reconfigure_audio(&self, format: AudioFormat) -> Result<(), BackendError> {
        format.validate()?;
        self.ensure_controllable()?;
        self.shared.audio.clear();
        *self
            .shared
            .audio_format
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = format;
        self.shared.set_state(LifecycleState::Reconfiguring);
        self.control
            .send(Control::ReconfigureAudio(format))
            .map_err(|_| BackendError::WorkerDisconnected)
    }

    pub fn set_surface(&self, surface: SurfaceTarget) -> Result<(), BackendError> {
        surface.validate()?;
        self.ensure_controllable()?;
        *self
            .shared
            .surface
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = surface;
        self.shared.set_state(LifecycleState::Reconfiguring);
        self.control
            .send(Control::SetSurface(surface))
            .map_err(|_| BackendError::WorkerDisconnected)
    }

    pub fn set_paused(&self, paused: bool) -> Result<(), BackendError> {
        self.ensure_controllable()?;
        if self.shared.paused.swap(paused, Ordering::AcqRel) == paused {
            return Ok(());
        }
        self.shared.video.clear();
        self.shared.audio.clear();
        self.control
            .send(Control::SetPaused(paused))
            .map_err(|_| BackendError::WorkerDisconnected)
    }

    pub fn try_event(&self) -> Option<BackendEvent> {
        self.shared.events.try_pop()
    }

    pub fn stop(&self) {
        let state = self.state();
        if matches!(state, LifecycleState::Stopped | LifecycleState::Stopping) {
            return;
        }
        let was_failed = state == LifecycleState::Failed;
        if !was_failed {
            self.shared.set_state(LifecycleState::Stopping);
        }
        self.shared.video.close();
        self.shared.audio.close();
        let _ = self.control.send(Control::Stop);
        if let Some(worker) = self
            .worker
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
        {
            let _ = worker.join();
        }
        if !was_failed && self.state() != LifecycleState::Failed {
            self.shared.set_state(LifecycleState::Stopped);
        }
    }

    fn ensure_media_accepting(&self) -> Result<(), BackendError> {
        let state = self.state();
        if matches!(
            state,
            LifecycleState::Running | LifecycleState::Reconfiguring
        ) {
            Ok(())
        } else {
            Err(BackendError::NotRunning(state))
        }
    }

    fn ensure_controllable(&self) -> Result<(), BackendError> {
        let state = self.state();
        if matches!(
            state,
            LifecycleState::Running | LifecycleState::Reconfiguring | LifecycleState::Recovering
        ) {
            Ok(())
        } else {
            Err(BackendError::NotRunning(state))
        }
    }
}

impl Drop for WindowsBackend {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_backend(state: LifecycleState) -> (WindowsBackend, Arc<Shared>) {
        let (control_sender, _control_receiver) = mpsc::channel();
        let shared = Arc::new(Shared {
            video: BoundedQueue::new(2),
            audio: BoundedQueue::new(2),
            video_format: Mutex::new(VideoFormat {
                codec: VideoCodec::H264,
                width: 1920,
                height: 1080,
                frame_rate_numerator: std::num::NonZeroU32::new(60).unwrap(),
                frame_rate_denominator: std::num::NonZeroU32::new(1).unwrap(),
                average_bitrate: 10_000_000,
                pixel_format: VideoPixelFormat::Nv12,
                chroma_format: VideoChromaFormat::Cs420,
                full_range: false,
            }),
            audio_format: Mutex::new(AudioFormat {
                sample_rate: 48_000,
                channels: 2,
            }),
            surface: Mutex::new(SurfaceTarget::Owned(OwnedWindow {
                parent: None,
                bounds: Bounds {
                    x: 0,
                    y: 0,
                    width: 1280,
                    height: 720,
                },
                visible: false,
            })),
            state: AtomicU8::new(state as u8),
            paused: std::sync::atomic::AtomicBool::new(false),
            events: BoundedQueue::new(8),
            presented_frames: AtomicU64::new(0),
        });
        let backend = WindowsBackend {
            shared: Arc::clone(&shared),
            control: control_sender,
            worker: Mutex::new(None),
        };
        (backend, shared)
    }

    #[test]
    fn non_windows_probe_never_advertises_capabilities() {
        if !cfg!(windows) {
            let probe = WindowsBackend::probe();
            assert!(!probe.available);
            assert!(!probe.h264_hardware_decode);
            assert!(!probe.h265_hardware_decode);
            assert!(!probe.av1_hardware_decode);
            assert!(!probe.h264_software_decode);
            assert!(!probe.h265_software_decode);
            assert!(!probe.av1_software_decode);
            assert!(!probe.d3d11_presentation);
            assert!(!probe.wasapi_render);
        }
    }

    #[test]
    fn bundled_backend_requires_wasapi_start() {
        let mut probe = CapabilityProbe {
            available: false,
            h264_hardware_decode: true,
            h265_hardware_decode: true,
            av1_hardware_decode: true,
            h264_software_decode: true,
            h265_software_decode: true,
            av1_software_decode: true,
            d3d11_presentation: true,
            wasapi_render: false,
            reason: Some("WASAPI failed to start".to_owned()),
        };

        assert!(!probe.bundled_backend_available());
        probe.wasapi_render = true;
        assert!(probe.bundled_backend_available());
        assert!(probe.software_backend_available());
    }

    #[test]
    fn endpoint_change_is_reported_once_per_transition() {
        let mut tracker = DefaultEndpointTracker::new("speakers".to_owned());

        assert!(!tracker.observe("speakers".to_owned()));
        assert!(tracker.observe("headset".to_owned()));
        assert!(!tracker.observe("headset".to_owned()));
        assert!(!tracker.observe("speakers".to_owned()));
        assert!(tracker.observe("headset".to_owned()));
    }

    #[test]
    fn lifecycle_state_round_trips_atomic_values() {
        for state in [
            LifecycleState::Starting,
            LifecycleState::Running,
            LifecycleState::Reconfiguring,
            LifecycleState::Recovering,
            LifecycleState::Stopping,
            LifecycleState::Stopped,
            LifecycleState::Failed,
        ] {
            assert_eq!(LifecycleState::from_raw(state as u8), state);
        }
    }

    #[test]
    fn stop_closes_media_queues_and_is_idempotent() {
        let (backend, shared) = test_backend(LifecycleState::Running);

        backend.stop();
        backend.stop();

        assert_eq!(backend.state(), LifecycleState::Stopped);
        assert!(shared.video.is_closed());
        assert!(shared.audio.is_closed());

        shared
            .state
            .store(LifecycleState::Failed as u8, Ordering::Release);
        backend.stop();
        assert_eq!(backend.state(), LifecycleState::Failed);
    }

    #[test]
    fn surface_reconfiguration_keeps_accepting_media_frames() {
        let (backend, shared) = test_backend(LifecycleState::Reconfiguring);
        let video_outcome = backend.submit_video(EncodedVideoFrame {
            codec: VideoCodec::H264,
            data: vec![0, 0, 0, 1, 0x67],
            timestamp_100ns: 0,
            duration_100ns: 166_667,
            key_frame: true,
            reset_decoder: false,
        });
        let audio_outcome = backend.submit_audio(PcmFrame {
            samples: vec![0.0, 0.0],
            format: AudioFormat {
                sample_rate: 48_000,
                channels: 2,
            },
        });

        assert_eq!(video_outcome, Ok(PushOutcome::Queued));
        assert_eq!(audio_outcome, Ok(PushOutcome::Queued));
        assert!(shared.video.try_pop().is_some());
        assert!(shared.audio.try_pop().is_some());
    }
}
