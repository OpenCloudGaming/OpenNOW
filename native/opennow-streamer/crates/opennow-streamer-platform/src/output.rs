use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Cursor as IoCursor;
#[cfg(target_os = "windows")]
use std::sync::atomic::AtomicBool;
#[cfg(target_os = "linux")]
use std::sync::atomic::AtomicU64;
#[cfg(any(target_os = "windows", target_os = "linux"))]
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
#[cfg(target_os = "linux")]
use std::time::Instant;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use image::ImageReader;
use opennow_streamer_protocol::RenderSurface;
use sdl2::audio::{AudioCallback, AudioDevice, AudioSpecDesired};
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::render::{Texture, WindowCanvas};

#[cfg(target_os = "linux")]
use crate::linux_frame_pacing::{FrameSelectionPolicy, LinuxFramePacer};
#[cfg(target_os = "linux")]
use crate::linux_xinput::LinuxXInputController;
use crate::media::{CapturedInput, CapturedInputQueue, MediaStreamConfig};
#[cfg(target_os = "linux")]
use crate::native_stats_overlay::NativeStatsOverlay;
use crate::native_surface::NativeSurface;
#[cfg(target_os = "windows")]
use crate::windows_debug_overlay::NativeDebugOverlay;
#[cfg(target_os = "windows")]
use crate::windows_raw_input::WindowsRawInputController;

#[cfg(target_os = "linux")]
use opennow_streamer_platform_linux::DecodedVideoFrame as LinuxDecodedVideoFrame;
#[cfg(target_os = "windows")]
use opennow_streamer_platform_windows::{
    AudioFormat, BackendConfig, BackendEvent, Bounds, ExistingWindow, OwnedWindow, Subsystem,
    SurfaceTarget, VideoCodec, VideoFormat, WindowHandle, WindowsBackend, WindowsGraphicsApi,
};

const AUDIO_SAMPLE_RATE: i32 = 48_000;
const AUDIO_CHANNELS: u8 = 2;
const AUDIO_BUFFER_FRAMES: u16 = 480;
const MAX_AUDIO_LATENCY_MS: usize = 120;
#[cfg(target_os = "linux")]
const LINUX_VIDEO_QUEUE_CAPACITY: usize = 6;

#[derive(Debug)]
pub(crate) struct DecodedVideoFrame {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rgb: Vec<u8>,
}

#[cfg(target_os = "linux")]
#[derive(Debug)]
struct QueuedLinuxVideoFrame {
    frame: LinuxDecodedVideoFrame,
    arrived_at: Instant,
}

#[derive(Debug)]
pub(crate) struct OutputBuffers {
    video: Mutex<Option<DecodedVideoFrame>>,
    #[cfg(target_os = "linux")]
    linux_video: Mutex<VecDeque<QueuedLinuxVideoFrame>>,
    #[cfg(target_os = "linux")]
    software_video_drops: AtomicU64,
    #[cfg(target_os = "linux")]
    hardware_video_drops: AtomicU64,
    #[cfg(target_os = "linux")]
    display_video_skips: AtomicU64,
    #[cfg(target_os = "linux")]
    received_video_bytes: AtomicU64,
    audio: Mutex<VecDeque<f32>>,
    audio_capacity: usize,
    captured_input: Arc<CapturedInputQueue>,
}

#[cfg(target_os = "windows")]
pub(crate) struct WindowsBridge {
    backend: Mutex<Option<Arc<WindowsBackend>>>,
    software_fallback: AtomicBool,
    keyframe_required: AtomicBool,
    last_video_mid: Mutex<String>,
}

#[cfg(target_os = "windows")]
impl WindowsBridge {
    pub(crate) fn new() -> Self {
        Self {
            backend: Mutex::new(None),
            software_fallback: AtomicBool::new(false),
            keyframe_required: AtomicBool::new(true),
            last_video_mid: Mutex::new("video".to_owned()),
        }
    }

    pub(crate) fn backend(&self) -> Option<Arc<WindowsBackend>> {
        self.backend
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    fn replace_backend(&self, backend: Option<Arc<WindowsBackend>>) {
        *self
            .backend
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = backend;
    }

    pub(crate) fn use_software(&self) -> bool {
        self.software_fallback.load(Ordering::Acquire)
    }

    pub(crate) fn fall_back_to_software(&self) {
        self.software_fallback.store(true, Ordering::Release);
        self.keyframe_required.store(true, Ordering::Release);
        self.replace_backend(None);
    }

    pub(crate) fn reset(&self) {
        self.software_fallback.store(false, Ordering::Release);
        self.keyframe_required.store(true, Ordering::Release);
        self.replace_backend(None);
    }

    pub(crate) fn set_last_video_mid(&self, mid: &str) {
        *self
            .last_video_mid
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = mid.to_owned();
    }

    pub(crate) fn last_video_mid(&self) -> String {
        self.last_video_mid
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub(crate) fn keyframe_required(&self) -> bool {
        self.keyframe_required.load(Ordering::Acquire)
    }

    pub(crate) fn require_keyframe(&self) {
        self.keyframe_required.store(true, Ordering::Release);
    }

    pub(crate) fn accept_keyframe(&self) {
        self.keyframe_required.store(false, Ordering::Release);
    }
}

impl OutputBuffers {
    pub(crate) fn new() -> Self {
        Self {
            video: Mutex::new(None),
            #[cfg(target_os = "linux")]
            linux_video: Mutex::new(VecDeque::with_capacity(LINUX_VIDEO_QUEUE_CAPACITY)),
            #[cfg(target_os = "linux")]
            software_video_drops: AtomicU64::new(0),
            #[cfg(target_os = "linux")]
            hardware_video_drops: AtomicU64::new(0),
            #[cfg(target_os = "linux")]
            display_video_skips: AtomicU64::new(0),
            #[cfg(target_os = "linux")]
            received_video_bytes: AtomicU64::new(0),
            audio: Mutex::new(VecDeque::with_capacity(
                AUDIO_SAMPLE_RATE as usize * AUDIO_CHANNELS as usize * MAX_AUDIO_LATENCY_MS / 1_000,
            )),
            audio_capacity: AUDIO_SAMPLE_RATE as usize
                * AUDIO_CHANNELS as usize
                * MAX_AUDIO_LATENCY_MS
                / 1_000,
            captured_input: Arc::new(CapturedInputQueue::default()),
        }
    }

    pub(crate) fn captured_input(&self) -> Arc<CapturedInputQueue> {
        Arc::clone(&self.captured_input)
    }

    pub(crate) fn replace_video(&self, frame: DecodedVideoFrame) -> bool {
        let dropped = self
            .video
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .replace(frame)
            .is_some();
        #[cfg(target_os = "linux")]
        if dropped {
            self.software_video_drops.fetch_add(1, Ordering::Relaxed);
        }
        dropped
    }

    pub(crate) fn take_video(&self) -> Option<DecodedVideoFrame> {
        self.video
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn queue_linux_video(&self, frame: LinuxDecodedVideoFrame) -> bool {
        let mut queue = self
            .linux_video
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let dropped = if queue.len() == LINUX_VIDEO_QUEUE_CAPACITY {
            queue.pop_front();
            true
        } else {
            false
        };
        queue.push_back(QueuedLinuxVideoFrame {
            frame,
            arrived_at: Instant::now(),
        });
        if dropped {
            self.hardware_video_drops.fetch_add(1, Ordering::Relaxed);
        }
        dropped
    }

    #[cfg(target_os = "linux")]
    fn take_linux_video(&self) -> Option<QueuedLinuxVideoFrame> {
        self.linux_video
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .pop_front()
    }

    #[cfg(target_os = "linux")]
    fn take_linux_video_for_presentation(
        &self,
        selection: FrameSelectionPolicy,
        target_queue_depth: usize,
    ) -> Option<QueuedLinuxVideoFrame> {
        let mut queue = self
            .linux_video
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if queue.len() <= target_queue_depth {
            return None;
        }
        let (frame, skipped) = match selection {
            FrameSelectionPolicy::OldestReady => (queue.pop_front(), 0),
            FrameSelectionPolicy::LatestReady => {
                let frame = queue.pop_back();
                let skipped = queue.len();
                queue.clear();
                (frame, skipped)
            }
        };
        if skipped > 0 {
            self.display_video_skips.fetch_add(
                u64::try_from(skipped).unwrap_or(u64::MAX),
                Ordering::Relaxed,
            );
        }
        frame
    }

    #[cfg(target_os = "linux")]
    fn clear_linux_video(&self) {
        self.linux_video
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
    }

    #[cfg(target_os = "linux")]
    fn software_video_drops(&self) -> u64 {
        self.software_video_drops.load(Ordering::Relaxed)
    }

    #[cfg(target_os = "linux")]
    fn hardware_video_drops(&self) -> u64 {
        self.hardware_video_drops.load(Ordering::Relaxed)
    }

    #[cfg(target_os = "linux")]
    fn display_video_skips(&self) -> u64 {
        self.display_video_skips.load(Ordering::Relaxed)
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn record_received_video_bytes(&self, bytes: usize) {
        self.received_video_bytes
            .fetch_add(u64::try_from(bytes).unwrap_or(u64::MAX), Ordering::Relaxed);
    }

    #[cfg(target_os = "linux")]
    fn received_video_bytes(&self) -> u64 {
        self.received_video_bytes.load(Ordering::Relaxed)
    }

    pub(crate) fn push_audio(&self, samples: &[f32]) -> usize {
        let mut audio = self.audio.lock().unwrap_or_else(|error| error.into_inner());
        let overflow = audio
            .len()
            .saturating_add(samples.len())
            .saturating_sub(self.audio_capacity);
        if overflow > 0 {
            let drain_count = overflow.min(audio.len());
            audio.drain(..drain_count);
        }
        if samples.len() >= self.audio_capacity {
            audio.clear();
            audio.extend(
                samples[samples.len() - self.audio_capacity..]
                    .iter()
                    .copied(),
            );
        } else {
            audio.extend(samples.iter().copied());
        }
        overflow
    }

    pub(crate) fn clear(&self) {
        self.take_video();
        #[cfg(target_os = "linux")]
        {
            self.clear_linux_video();
            self.software_video_drops.store(0, Ordering::Relaxed);
            self.hardware_video_drops.store(0, Ordering::Relaxed);
            self.display_video_skips.store(0, Ordering::Relaxed);
        }
        self.audio
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
    }

    fn fill_audio(&self, destination: &mut [f32]) {
        let mut audio = self.audio.lock().unwrap_or_else(|error| error.into_inner());
        for sample in destination {
            *sample = audio.pop_front().unwrap_or(0.0);
        }
    }
}

#[cfg(target_os = "linux")]
pub(crate) struct LinuxHardwareOutput {
    // These fields own resources derived from the SDL Wayland/X11 display.
    // Rust drops fields in declaration order, so keep the presenter and
    // window ahead of the SDL root to preserve their native lifetimes even
    // when normal stop handling is bypassed during unwinding.
    presenter: Option<opennow_streamer_platform_linux::VulkanPresenter>,
    native_surface: Result<NativeSurface, String>,
    input_capture: SdlInputCapture,
    raw_input: Option<LinuxXInputController>,
    window: sdl2::video::Window,
    video: sdl2::VideoSubsystem,
    event_pump: sdl2::EventPump,
    audio: AudioDevice<StreamAudioCallback>,
    output: Arc<OutputBuffers>,
    external_renderer: bool,
    stream_size: (u32, u32),
    surface_size: Option<(u32, u32)>,
    stream_fps: u32,
    debug_overlay: NativeStatsOverlay,
    presented_frames: u64,
    frame_pacer: LinuxFramePacer,
    vrr_fullscreen_initialized: bool,
    visible: bool,
    paused: bool,
    _sdl: sdl2::Sdl,
}

#[cfg(target_os = "linux")]
impl LinuxHardwareOutput {
    fn initialize(output: Arc<OutputBuffers>, stream: MediaStreamConfig) -> Result<Self, String> {
        configure_linux_sdl_video_driver();
        let sdl = sdl2::init().map_err(|error| format!("SDL initialization failed: {error}"))?;
        let video = sdl
            .video()
            .map_err(|error| format!("SDL video initialization failed: {error}"))?;
        let audio_subsystem = sdl
            .audio()
            .map_err(|error| format!("SDL audio initialization failed: {error}"))?;
        let video_driver = video.current_video_driver();
        if !matches!(video_driver, "x11" | "wayland") {
            return Err(format!(
                "Linux Vulkan presentation requires X11 or Wayland, but SDL selected {video_driver}",
            ));
        }
        let external_renderer = external_renderer_enabled();
        if video_driver == "wayland" && !external_renderer {
            return Err(
                "Wayland presentation must use a compositor-managed top-level window".to_owned(),
            );
        }
        let mut window_builder = video.window("OpenNOW Stream", 1280, 720);
        window_builder
            .position_centered()
            .resizable()
            .allow_highdpi()
            .hidden()
            .vulkan();
        if !external_renderer {
            window_builder.borderless();
        }
        let window = window_builder
            .build()
            .map_err(|error| format!("native Vulkan window creation failed: {error}"))?;
        let display_refresh_hz = linux_display_refresh_hz(&video, &window);
        let frame_pacer = LinuxFramePacer::new(stream.fps, display_refresh_hz, stream.cloud_gsync);
        eprintln!(
            "Linux presentation pacing: stream={}fps display={} output={:.1}Hz mode={}",
            stream.fps,
            display_refresh_hz.map_or_else(|| "unknown".to_owned(), |value| format!("{value}Hz")),
            frame_pacer.presentation_hz(),
            if frame_pacer.vrr_enabled() {
                "cloud-gsync-vrr"
            } else if frame_pacer.fast_stream() {
                "nonblocking-fast-stream"
            } else {
                "adaptive-timestamp"
            },
        );
        let native_surface = if external_renderer {
            Err("external Linux output does not use child-window embedding".to_owned())
        } else {
            NativeSurface::new(&window)
        };
        let desired = AudioSpecDesired {
            freq: Some(AUDIO_SAMPLE_RATE),
            channels: Some(AUDIO_CHANNELS),
            samples: Some(AUDIO_BUFFER_FRAMES),
        };
        let callback_output = Arc::clone(&output);
        let audio = audio_subsystem
            .open_playback(None, &desired, move |_| StreamAudioCallback {
                output: callback_output,
            })
            .map_err(|error| format!("native audio output creation failed: {error}"))?;
        let event_pump = sdl
            .event_pump()
            .map_err(|error| format!("native window event pump creation failed: {error}"))?;
        let capture_input = external_renderer && native_input_capture_enabled();
        let raw_input = if capture_input && video_driver == "x11" {
            match LinuxXInputController::start(output.captured_input()) {
                Ok(controller) => Some(controller),
                Err(error) => {
                    eprintln!(
                        "Dedicated Linux XInput2 unavailable; retaining SDL raw motion: {error}"
                    );
                    None
                }
            }
        } else {
            None
        };
        let external_relative_motion = raw_input.is_some();
        Ok(Self {
            presenter: None,
            native_surface,
            input_capture: SdlInputCapture::new(capture_input, external_relative_motion),
            raw_input,
            window,
            video,
            event_pump,
            audio,
            output,
            external_renderer,
            stream_size: (stream.width.max(1), stream.height.max(1)),
            surface_size: None,
            stream_fps: stream.fps.max(1),
            debug_overlay: NativeStatsOverlay::new(
                stream,
                if stream.cloud_gsync {
                    "LINUX / VULKAN VRR"
                } else {
                    "LINUX / VULKAN"
                },
            ),
            presented_frames: 0,
            frame_pacer,
            vrr_fullscreen_initialized: false,
            visible: false,
            paused: false,
            _sdl: sdl,
        })
    }

    fn start(&mut self, surface: Option<&RenderSurface>) -> Result<(), String> {
        self.paused = false;
        self.presented_frames = 0;
        self.frame_pacer.reset();
        self.vrr_fullscreen_initialized = false;
        self.output.clear();
        self.audio.resume();
        if let Some(surface) = surface {
            self.update_surface(surface)?;
        }
        Ok(())
    }

    fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        self.frame_pacer.reset();
        self.vrr_fullscreen_initialized = false;
        if paused {
            self.input_capture.release(&self._sdl, &mut self.window);
            if let Some(raw_input) = self.raw_input.as_ref() {
                raw_input.set_enabled(false);
            }
            self.audio.pause();
            self.output.clear();
        } else {
            self.audio.resume();
        }
    }

    fn stop(&mut self) {
        self.input_capture.release(&self._sdl, &mut self.window);
        if let Some(raw_input) = self.raw_input.as_ref() {
            raw_input.set_enabled(false);
        }
        self.output.clear();
        self.audio.pause();
        self.presenter = None;
        self.surface_size = None;
        if let Ok(surface) = self.native_surface.as_mut() {
            surface.hide();
        }
        if self.external_renderer {
            self.window.hide();
        }
        self.visible = false;
        self.paused = false;
        self.frame_pacer.reset();
        self.vrr_fullscreen_initialized = false;
    }

    fn update_surface(&mut self, surface: &RenderSurface) -> Result<(), String> {
        let Some(rect) = surface.rect.filter(|_| surface.visible) else {
            self.input_capture.release(&self._sdl, &mut self.window);
            if let Some(raw_input) = self.raw_input.as_ref() {
                raw_input.set_enabled(false);
            }
            if let Ok(native_surface) = self.native_surface.as_mut() {
                native_surface.hide();
            }
            if self.external_renderer {
                self.window.hide();
            }
            self.presenter = None;
            self.surface_size = None;
            self.visible = false;
            return Ok(());
        };
        if self.external_renderer {
            let bounds = surface.screen_rect.unwrap_or(rect);
            self.window.set_position(
                sdl2::video::WindowPos::Positioned(bounds.x),
                sdl2::video::WindowPos::Positioned(bounds.y),
            );
            self.window
                .set_size(bounds.width.max(2), bounds.height.max(2))
                .map_err(|error| format!("failed to resize external Vulkan surface: {error}"))?;
            self.window.show();
            if !self.visible {
                self.window.raise();
            }
            if self.frame_pacer.vrr_enabled() && !self.vrr_fullscreen_initialized {
                use sdl2::video::FullscreenType;

                self.window
                    .set_fullscreen(FullscreenType::Desktop)
                    .map_err(|error| {
                        format!("Cloud G-SYNC could not enter compositor fullscreen: {error}")
                    })?;
                self.vrr_fullscreen_initialized = true;
                eprintln!(
                    "Linux Cloud G-SYNC presentation requested compositor fullscreen for VRR"
                );
            }
        } else {
            let parent_handle = surface.window_handle.as_deref().ok_or_else(|| {
                "visible native surface is missing Electron windowHandle".to_owned()
            })?;
            self.native_surface
                .as_mut()
                .map_err(|error| error.clone())?
                .attach_and_show(
                    parent_handle,
                    rect,
                    surface.screen_rect,
                    surface.device_scale_factor,
                )?;
        }
        let display_refresh_hz = linux_display_refresh_hz(&self.video, &self.window);
        if self
            .frame_pacer
            .reconfigure(self.stream_fps, display_refresh_hz)
        {
            eprintln!(
                "Linux presentation pacing changed: stream={}fps display={} output={:.1}Hz mode={}",
                self.stream_fps,
                display_refresh_hz
                    .map_or_else(|| "unknown".to_owned(), |value| format!("{value}Hz")),
                self.frame_pacer.presentation_hz(),
                if self.frame_pacer.vrr_enabled() {
                    "cloud-gsync-vrr"
                } else if self.frame_pacer.fast_stream() {
                    "nonblocking-fast-stream"
                } else {
                    "adaptive-timestamp"
                },
            );
            self.output.clear_linux_video();
        }
        let size = if self.external_renderer {
            let (width, height) = self.window.vulkan_drawable_size();
            (width.max(2), height.max(2))
        } else {
            (rect.width.max(2), rect.height.max(2))
        };
        if self.presenter.is_some() && self.surface_size != Some(size) {
            if let Some(presenter) = self.presenter.as_mut() {
                presenter
                    .reconfigure(size.0, size.1)
                    .map_err(|error| error.to_string())?;
            }
        }
        self.surface_size = Some(size);
        self.visible = true;
        Ok(())
    }

    fn pump(&mut self) -> Result<bool, String> {
        if !self.external_renderer
            && let Ok(surface) = self.native_surface.as_mut()
        {
            surface.refresh_ordering()?;
        }
        for event in self.event_pump.poll_iter().collect::<Vec<_>>() {
            if matches!(event, sdl2::event::Event::Quit { .. }) {
                self.input_capture.release(&self._sdl, &mut self.window);
                if let Ok(surface) = self.native_surface.as_mut() {
                    surface.hide();
                }
                if self.external_renderer {
                    self.window.hide();
                }
                self.presenter = None;
                self.visible = false;
            } else if handle_linux_stats_shortcut(&mut self.debug_overlay, &event)
                || (self.external_renderer
                    && handle_native_window_shortcut(&mut self.window, &event))
            {
                continue;
            } else {
                self.input_capture.handle_event(
                    &self._sdl,
                    &mut self.window,
                    self.stream_size,
                    event,
                );
            }
        }
        if let Some(raw_input) = self.raw_input.as_ref() {
            raw_input.set_enabled(
                self.visible && self.input_capture.focused && self.input_capture.relative_mouse,
            );
        }
        // NVST input must not sit behind Vulkan presentation. FIFO acquire can
        // legitimately report back-pressure, and queue_present may enter the
        // compositor, so publish every event while we are still ahead of WSI.
        // The runtime's normal post-pump drain remains as a fallback for input
        // generated by lifecycle operations outside this event batch.
        let captured_input = self.output.captured_input();
        for input in self.input_capture.take() {
            captured_input.push(input);
        }
        if self.paused || !self.visible {
            self.output.clear_linux_video();
            return Ok(false);
        }
        if self.external_renderer {
            let (width, height) = self.window.vulkan_drawable_size();
            let drawable_size = (width.max(2), height.max(2));
            if self.surface_size != Some(drawable_size) {
                if let Some(presenter) = self.presenter.as_mut() {
                    presenter
                        .reconfigure(drawable_size.0, drawable_size.1)
                        .map_err(|error| error.to_string())?;
                }
                self.surface_size = Some(drawable_size);
            }
        }
        self.debug_overlay
            .set_presentation_skips(self.output.display_video_skips());
        self.debug_overlay.update(
            self.presented_frames,
            self.output.hardware_video_drops(),
            self.input_capture.relative_mouse,
            self.output.received_video_bytes(),
        );
        let now = Instant::now();
        if !self.frame_pacer.is_due(now) {
            return Ok(false);
        }
        let decision = self.frame_pacer.decision(now);
        let frame = self
            .output
            .take_linux_video_for_presentation(decision.selection, decision.target_queue_depth);
        let Some(queued_frame) = frame else {
            return Ok(false);
        };
        self.frame_pacer
            .observe_frame(queued_frame.arrived_at, queued_frame.frame.timestamp_us);
        let mut frame = queued_frame.frame;
        self.stream_size = (frame.format.width.max(1), frame.format.height.max(1));
        self.debug_overlay.composite_linux_frame(&mut frame);
        if frame.vulkan.as_ref().is_some_and(|vulkan| {
            self.presenter
                .as_ref()
                .is_some_and(|presenter| !presenter.matches_vulkan_video_device(vulkan))
        }) {
            // Decoder recovery can replace the FFmpeg Vulkan device. Rebuild
            // the surface resources on the new device before touching it.
            self.presenter = None;
        }
        if self.presenter.is_none() {
            let size = self
                .surface_size
                .ok_or_else(|| "Linux Vulkan presenter has no visible surface extent".to_owned())?;
            self.presenter = Some(create_linux_presenter(
                &self.window,
                size.0,
                size.1,
                frame.vulkan.as_deref(),
            )?);
        }
        let presented = self
            .presenter
            .as_mut()
            .ok_or_else(|| {
                "Linux Vulkan presenter is not attached to a visible surface".to_owned()
            })?
            .present(&frame)
            .map_err(|error| error.to_string())?;
        if presented {
            self.presented_frames = self.presented_frames.saturating_add(1);
            self.frame_pacer.mark_presented(Instant::now());
        }
        Ok(presented)
    }

    fn take_captured_input(&mut self) -> Vec<CapturedInput> {
        self.input_capture.take()
    }

    fn update_cursor(&mut self, bytes: &[u8]) {
        if self.external_renderer {
            self.input_capture
                .apply_cursor(&self._sdl, &mut self.window, self.stream_size, bytes);
        }
    }
}

#[cfg(target_os = "linux")]
impl Drop for LinuxHardwareOutput {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(target_os = "linux")]
fn create_linux_presenter(
    window: &sdl2::video::Window,
    width: u32,
    height: u32,
    vulkan_video: Option<&opennow_streamer_platform_linux::VulkanVideoFrame>,
) -> Result<opennow_streamer_platform_linux::VulkanPresenter, String> {
    use std::ffi::c_void;
    use std::num::NonZeroU64;
    use std::ptr::NonNull;

    use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};

    let display_handle = window
        .display_handle()
        .map_err(|error| format!("SDL display handle unavailable: {error}"))?;
    let window_handle = window
        .window_handle()
        .map_err(|error| format!("SDL window handle unavailable: {error}"))?;
    let surface = match (display_handle.as_raw(), window_handle.as_raw()) {
        (RawDisplayHandle::Xlib(display), RawWindowHandle::Xlib(window)) => {
            let screen = display.screen;
            let display = display
                .display
                .ok_or_else(|| "SDL X11 display pointer is null".to_owned())?;
            let window = NonZeroU64::new(window.window)
                .ok_or_else(|| "SDL returned an empty X11 window handle".to_owned())?;
            let display = NonNull::<c_void>::new(display.as_ptr().cast())
                .ok_or_else(|| "SDL X11 display pointer is null".to_owned())?;
            unsafe {
                opennow_streamer_platform_linux::NativeSurface::borrow_x11(display, window, screen)
            }
        }
        (RawDisplayHandle::Wayland(display), RawWindowHandle::Wayland(window)) => unsafe {
            opennow_streamer_platform_linux::NativeSurface::borrow_wayland(
                display.display,
                window.surface,
            )
        },
        _ => {
            return Err(
                "SDL returned mismatched or unsupported Linux handles for Vulkan presentation"
                    .to_owned(),
            );
        }
    };
    match vulkan_video {
        Some(frame) => opennow_streamer_platform_linux::VulkanPresenter::new_for_vulkan_video(
            &surface, width, height, frame,
        ),
        None => opennow_streamer_platform_linux::VulkanPresenter::new(&surface, width, height),
    }
    .map_err(|error| error.to_string())
}

#[cfg(target_os = "linux")]
fn configure_linux_sdl_video_driver() {
    let native_wayland = std::env::var("OPENNOW_LINUX_NATIVE_WAYLAND")
        .ok()
        .is_some_and(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        });
    let explicit_driver = std::env::var("SDL_VIDEODRIVER")
        .ok()
        .is_some_and(|value| !value.trim().is_empty());
    let xwayland_available = std::env::var("DISPLAY")
        .ok()
        .is_some_and(|value| !value.trim().is_empty());
    if !native_wayland && !explicit_driver && xwayland_available {
        // NVIDIA's native Linux client deliberately runs through X11/XWayland
        // and couples its xlib Vulkan surface to a dedicated XInput2 raw-input
        // worker. Match that path by default; native Wayland remains available
        // on systems without XWayland or through the explicit opt-in above.
        let _ = sdl2::hint::set("SDL_VIDEODRIVER", "x11");
    }
}

#[cfg(target_os = "linux")]
fn linux_display_refresh_hz(
    video: &sdl2::VideoSubsystem,
    window: &sdl2::video::Window,
) -> Option<u32> {
    let display_index = window.display_index().ok()?;
    let refresh_rate = video.current_display_mode(display_index).ok()?.refresh_rate;
    u32::try_from(refresh_rate).ok().filter(|value| *value > 0)
}

struct StreamAudioCallback {
    output: Arc<OutputBuffers>,
}

impl AudioCallback for StreamAudioCallback {
    type Channel = f32;

    fn callback(&mut self, output: &mut [f32]) {
        self.output.fill_audio(output);
    }
}

pub(crate) struct SdlInputCapture {
    captured: Vec<CapturedInput>,
    pressed_keys: HashMap<sdl2::keyboard::Scancode, u16>,
    pressed_buttons: HashSet<u8>,
    enabled: bool,
    focused: bool,
    relative_mouse: bool,
    external_relative_motion: bool,
    cursor_state: RemoteCursorState,
    cursors: HashMap<(u8, u8), sdl2::mouse::Cursor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemoteCursorState {
    Unknown,
    Hidden,
    Visible,
}

impl SdlInputCapture {
    pub(crate) fn new(enabled: bool, external_relative_motion: bool) -> Self {
        Self {
            captured: Vec::new(),
            pressed_keys: HashMap::new(),
            pressed_buttons: HashSet::new(),
            enabled,
            focused: false,
            relative_mouse: false,
            external_relative_motion,
            // Do not infer hidden-cursor gameplay before the first server
            // update. GFN sends a distinct predefined cursor ID 0 when the
            // game actually wants locked relative input.
            cursor_state: RemoteCursorState::Unknown,
            cursors: HashMap::new(),
        }
    }

    pub(crate) fn handle_event(
        &mut self,
        sdl: &sdl2::Sdl,
        window: &mut sdl2::video::Window,
        stream_size: (u32, u32),
        event: sdl2::event::Event,
    ) {
        use sdl2::event::{Event, WindowEvent};

        if !self.enabled {
            return;
        }
        let window_size = window.size();
        match event {
            Event::KeyDown {
                scancode: Some(scancode),
                keymod,
                repeat: false,
                ..
            } => {
                let Some(virtual_key) = sdl_virtual_key(scancode) else {
                    return;
                };
                if self.pressed_keys.insert(scancode, virtual_key).is_none() {
                    self.captured.push(CapturedInput::Key {
                        virtual_key,
                        modifiers: sdl_modifiers(scancode, keymod),
                        pressed: true,
                    });
                }
            }
            Event::KeyUp {
                scancode: Some(scancode),
                keymod,
                ..
            } => {
                if let Some(virtual_key) = self.pressed_keys.remove(&scancode) {
                    self.captured.push(CapturedInput::Key {
                        virtual_key,
                        modifiers: sdl_modifiers(scancode, keymod),
                        pressed: false,
                    });
                }
            }
            Event::MouseMotion { .. } if self.relative_mouse && self.external_relative_motion => {}
            Event::MouseMotion { xrel, yrel, .. } if self.relative_mouse => {
                push_mouse_motion(&mut self.captured, xrel, yrel);
            }
            Event::MouseMotion { x, y, .. } if self.cursor_state != RemoteCursorState::Hidden => {
                let absolute =
                    map_window_point_to_stream_viewport((x, y), stream_size, window_size);
                self.captured.push(CapturedInput::MouseAbsolute {
                    x: absolute.x,
                    y: absolute.y,
                    width: absolute.width,
                    height: absolute.height,
                });
            }
            Event::MouseButtonDown { .. } | Event::MouseButtonUp { .. }
                if self.relative_mouse && self.external_relative_motion => {}
            Event::MouseButtonDown { mouse_btn, .. } => {
                // A button event proves this window owns foreground input even
                // if SDL delivered FocusGained later in the same pump batch.
                self.focused = true;
                if self.cursor_state == RemoteCursorState::Hidden {
                    self.enable_relative_mouse(sdl, window);
                }
                if let Some(button) = sdl_mouse_button(mouse_btn)
                    && self.pressed_buttons.insert(button)
                {
                    self.captured.push(CapturedInput::MouseButton {
                        button,
                        pressed: true,
                    });
                }
            }
            Event::MouseButtonUp { mouse_btn, .. } => {
                if let Some(button) = sdl_mouse_button(mouse_btn)
                    && self.pressed_buttons.remove(&button)
                {
                    self.captured.push(CapturedInput::MouseButton {
                        button,
                        pressed: false,
                    });
                }
            }
            Event::MouseWheel { .. } if self.relative_mouse && self.external_relative_motion => {}
            Event::MouseWheel { y, direction, .. } if y != 0 => {
                let direction = if direction == sdl2::mouse::MouseWheelDirection::Flipped {
                    -1
                } else {
                    1
                };
                self.captured.push(CapturedInput::MouseWheel {
                    delta: clamp_i16(y.saturating_mul(120).saturating_mul(direction)),
                });
            }
            Event::Window {
                win_event: WindowEvent::FocusGained,
                ..
            } => {
                self.focused = true;
                if self.cursor_state == RemoteCursorState::Hidden {
                    self.enable_relative_mouse(sdl, window);
                }
            }
            Event::Window {
                win_event: WindowEvent::FocusLost,
                ..
            } => {
                self.focused = false;
                self.release(sdl, window);
            }
            _ => {}
        }
    }

    fn enable_relative_mouse(&mut self, sdl: &sdl2::Sdl, window: &mut sdl2::video::Window) {
        if self.focused && !self.relative_mouse {
            window.set_mouse_grab(true);
            sdl.mouse().set_relative_mouse_mode(true);
            sdl.mouse().show_cursor(false);
            self.relative_mouse = true;
            eprintln!("External SDL mouse control mode: locked relative");
        }
    }

    fn disable_relative_mouse(&mut self, sdl: &sdl2::Sdl, window: &mut sdl2::video::Window) {
        if self.relative_mouse {
            sdl.mouse().set_relative_mouse_mode(false);
            window.set_mouse_grab(false);
            self.relative_mouse = false;
            eprintln!("External SDL mouse control mode: absolute cursor");
        }
    }

    pub(crate) fn apply_cursor(
        &mut self,
        sdl: &sdl2::Sdl,
        window: &mut sdl2::video::Window,
        stream_size: (u32, u32),
        bytes: &[u8],
    ) {
        let Some((&message_type, remainder)) = bytes.split_first() else {
            return;
        };
        if !matches!(message_type, 0 | 1) {
            return;
        }
        let Some(&cursor_id) = remainder.first() else {
            return;
        };
        let was_cursor_visible = self.cursor_state == RemoteCursorState::Visible;
        self.cursor_state = cursor_state_for_message(message_type, cursor_id);
        eprintln!(
            "GFN cursor applied: sourceType={message_type} cursorId={cursor_id} state={:?} bytes={}",
            self.cursor_state,
            bytes.len(),
        );
        if self.cursor_state == RemoteCursorState::Hidden {
            self.enable_relative_mouse(sdl, window);
            return;
        }
        self.disable_relative_mouse(sdl, window);
        let cursor_key = (message_type, cursor_id);
        if message_type == 1 {
            match custom_sdl_cursor(bytes) {
                Ok(cursor) => {
                    self.cursors.insert(cursor_key, cursor);
                }
                Err(error) => {
                    eprintln!("GFN custom cursor {cursor_id} could not be decoded: {error}");
                }
            }
        } else if let std::collections::hash_map::Entry::Vacant(entry) =
            self.cursors.entry(cursor_key)
        {
            let system_cursor = match cursor_id {
                2 => sdl2::mouse::SystemCursor::IBeam,
                3 => sdl2::mouse::SystemCursor::Wait,
                4 => sdl2::mouse::SystemCursor::Crosshair,
                5 => sdl2::mouse::SystemCursor::WaitArrow,
                6 => sdl2::mouse::SystemCursor::SizeNWSE,
                7 => sdl2::mouse::SystemCursor::SizeNESW,
                8 => sdl2::mouse::SystemCursor::SizeWE,
                9 => sdl2::mouse::SystemCursor::SizeNS,
                10 => sdl2::mouse::SystemCursor::SizeAll,
                12 => sdl2::mouse::SystemCursor::Hand,
                _ => sdl2::mouse::SystemCursor::Arrow,
            };
            if let Ok(cursor) = sdl2::mouse::Cursor::from_system(system_cursor) {
                entry.insert(cursor);
            }
        }
        if let Some(cursor) = self.cursors.get(&cursor_key) {
            cursor.set();
        } else if let Ok(cursor) =
            sdl2::mouse::Cursor::from_system(sdl2::mouse::SystemCursor::Arrow)
        {
            cursor.set();
            self.cursors.insert(cursor_key, cursor);
        }
        sdl.mouse().show_cursor(true);
        if !was_cursor_visible && let Some(position) = parse_cursor_position(bytes) {
            let viewport = aspect_fit(
                stream_size.0.max(1),
                stream_size.1.max(1),
                window.size().0.max(1),
                window.size().1.max(1),
            );
            let x = viewport.x() + normalized_cursor_coordinate(position.0, viewport.width());
            let y = viewport.y() + normalized_cursor_coordinate(position.1, viewport.height());
            sdl.mouse().warp_mouse_in_window(window, x, y);
        }
    }

    pub(crate) fn release(&mut self, sdl: &sdl2::Sdl, window: &mut sdl2::video::Window) {
        for (_, virtual_key) in self.pressed_keys.drain() {
            self.captured.push(CapturedInput::Key {
                virtual_key,
                modifiers: 0,
                pressed: false,
            });
        }
        for button in self.pressed_buttons.drain() {
            self.captured.push(CapturedInput::MouseButton {
                button,
                pressed: false,
            });
        }
        self.disable_relative_mouse(sdl, window);
        window.set_mouse_grab(false);
        sdl.mouse().show_cursor(true);
    }

    pub(crate) fn take(&mut self) -> Vec<CapturedInput> {
        std::mem::take(&mut self.captured)
    }

    pub(crate) const fn relative_mouse_enabled(&self) -> bool {
        self.relative_mouse
    }
}

pub(crate) struct SoftwareOutput {
    // Texture and window-owned resources must be released before the SDL
    // renderer/window, and SDL itself must outlive every derived resource.
    texture: Option<Texture>,
    native_surface: Result<NativeSurface, String>,
    input_capture: SdlInputCapture,
    #[cfg(target_os = "linux")]
    raw_input: Option<LinuxXInputController>,
    canvas: WindowCanvas,
    texture_size: Option<(u32, u32)>,
    texture_format: Option<PixelFormatEnum>,
    event_pump: sdl2::EventPump,
    audio: AudioDevice<StreamAudioCallback>,
    output: Arc<OutputBuffers>,
    external_renderer: bool,
    #[cfg(target_os = "linux")]
    debug_overlay: NativeStatsOverlay,
    #[cfg(target_os = "linux")]
    presented_frames: u64,
    #[cfg(target_os = "linux")]
    presented_linux_frame: bool,
    visible: bool,
    paused: bool,
    _sdl: sdl2::Sdl,
}

impl SoftwareOutput {
    fn initialize(output: Arc<OutputBuffers>, _stream: MediaStreamConfig) -> Result<Self, String> {
        #[cfg(target_os = "linux")]
        configure_linux_sdl_video_driver();
        let sdl = sdl2::init().map_err(|error| format!("SDL initialization failed: {error}"))?;
        let video = sdl
            .video()
            .map_err(|error| format!("SDL video initialization failed: {error}"))?;
        let audio_subsystem = sdl
            .audio()
            .map_err(|error| format!("SDL audio initialization failed: {error}"))?;
        let external_renderer = external_renderer_enabled();
        let mut window_builder = video.window("OpenNOW Stream", 1280, 720);
        window_builder
            .position_centered()
            .resizable()
            .hidden()
            .metal_view();
        if external_renderer && cfg!(target_os = "linux") {
            window_builder.allow_highdpi();
        }
        if !external_renderer {
            window_builder.borderless();
        }
        let window = window_builder
            .build()
            .map_err(|error| format!("native video window creation failed: {error}"))?;
        let mut canvas = window
            .into_canvas()
            .build()
            .map_err(|error| format!("native video renderer creation failed: {error}"))?;
        canvas.set_draw_color(Color::BLACK);
        canvas.clear();
        canvas.present();

        let desired = AudioSpecDesired {
            freq: Some(AUDIO_SAMPLE_RATE),
            channels: Some(AUDIO_CHANNELS),
            samples: Some(AUDIO_BUFFER_FRAMES),
        };
        let callback_output = Arc::clone(&output);
        let audio = audio_subsystem
            .open_playback(None, &desired, move |_| StreamAudioCallback {
                output: callback_output,
            })
            .map_err(|error| format!("native audio output creation failed: {error}"))?;
        if audio.spec().freq != AUDIO_SAMPLE_RATE || audio.spec().channels != AUDIO_CHANNELS {
            return Err(format!(
                "native audio output returned unsupported format: {} Hz, {} channels",
                audio.spec().freq,
                audio.spec().channels
            ));
        }
        let event_pump = sdl
            .event_pump()
            .map_err(|error| format!("native window event pump creation failed: {error}"))?;
        let native_surface = if video.current_video_driver() == "dummy" {
            Err("dummy SDL video driver has no native presentation handle".to_owned())
        } else {
            NativeSurface::new(canvas.window())
        };
        let capture_input = cfg!(any(target_os = "windows", target_os = "linux"))
            && external_renderer
            && native_input_capture_enabled();
        #[cfg(target_os = "linux")]
        let raw_input = if capture_input && video.current_video_driver() == "x11" {
            match LinuxXInputController::start(output.captured_input()) {
                Ok(controller) => Some(controller),
                Err(error) => {
                    eprintln!(
                        "Dedicated Linux XInput2 unavailable; retaining SDL raw motion: {error}"
                    );
                    None
                }
            }
        } else {
            None
        };
        #[cfg(target_os = "linux")]
        let external_relative_motion = raw_input.is_some();
        #[cfg(not(target_os = "linux"))]
        let external_relative_motion = false;

        Ok(Self {
            texture: None,
            native_surface,
            input_capture: SdlInputCapture::new(capture_input, external_relative_motion),
            #[cfg(target_os = "linux")]
            raw_input,
            canvas,
            texture_size: None,
            texture_format: None,
            event_pump,
            audio,
            output,
            external_renderer,
            #[cfg(target_os = "linux")]
            debug_overlay: NativeStatsOverlay::new(_stream, "LINUX / SDL"),
            #[cfg(target_os = "linux")]
            presented_frames: 0,
            #[cfg(target_os = "linux")]
            presented_linux_frame: false,
            visible: false,
            paused: false,
            _sdl: sdl,
        })
    }

    fn start(&mut self, surface: Option<&RenderSurface>) -> Result<(), String> {
        self.paused = false;
        #[cfg(target_os = "linux")]
        {
            self.presented_frames = 0;
            self.presented_linux_frame = false;
        }
        self.output.clear();
        self.audio.resume();
        if let Some(surface) = surface {
            self.update_surface(surface)?;
        }
        Ok(())
    }

    fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        if paused {
            self.input_capture
                .release(&self._sdl, self.canvas.window_mut());
            #[cfg(target_os = "linux")]
            if let Some(raw_input) = self.raw_input.as_ref() {
                raw_input.set_enabled(false);
            }
            self.audio.pause();
            self.output.clear();
        } else {
            self.audio.resume();
        }
    }

    fn stop(&mut self) {
        self.input_capture
            .release(&self._sdl, self.canvas.window_mut());
        #[cfg(target_os = "linux")]
        if let Some(raw_input) = self.raw_input.as_ref() {
            raw_input.set_enabled(false);
        }
        self.flush_captured_input();
        self.audio.pause();
        self.output.clear();
        if let Ok(surface) = self.native_surface.as_mut() {
            surface.hide();
        }
        if self.external_renderer {
            self.canvas.window_mut().hide();
        }
        self.visible = false;
        self.paused = false;
        self.texture = None;
        self.texture_size = None;
        self.texture_format = None;
        #[cfg(target_os = "linux")]
        {
            self.presented_linux_frame = false;
        }
    }

    fn update_surface(&mut self, surface: &RenderSurface) -> Result<(), String> {
        let Some(rect) = surface.rect.filter(|_| surface.visible) else {
            #[cfg(target_os = "linux")]
            if let Some(raw_input) = self.raw_input.as_ref() {
                raw_input.set_enabled(false);
            }
            if let Ok(native_surface) = self.native_surface.as_mut() {
                native_surface.hide();
            }
            if self.external_renderer {
                self.canvas.window_mut().hide();
            }
            self.visible = false;
            return Ok(());
        };
        if self.external_renderer {
            let bounds = surface.screen_rect.unwrap_or(rect);
            let window = self.canvas.window_mut();
            window.set_position(
                sdl2::video::WindowPos::Positioned(bounds.x),
                sdl2::video::WindowPos::Positioned(bounds.y),
            );
            window
                .set_size(bounds.width.max(2), bounds.height.max(2))
                .map_err(|error| format!("failed to resize external SDL video surface: {error}"))?;
            window.show();
            window.raise();
            self.visible = true;
            return Ok(());
        }
        let parent_handle = surface
            .window_handle
            .as_deref()
            .ok_or_else(|| "visible native surface is missing Electron windowHandle".to_owned())?;
        self.native_surface
            .as_mut()
            .map_err(|error| error.clone())?
            .attach_and_show(
                parent_handle,
                rect,
                surface.screen_rect,
                surface.device_scale_factor,
            )?;
        self.visible = true;
        Ok(())
    }

    fn pump(&mut self) -> Result<bool, String> {
        if !self.external_renderer
            && let Ok(surface) = self.native_surface.as_mut()
        {
            surface.refresh_ordering()?;
        }
        let events = self.event_pump.poll_iter().collect::<Vec<_>>();
        for event in events {
            if matches!(event, sdl2::event::Event::Quit { .. }) {
                self.input_capture
                    .release(&self._sdl, self.canvas.window_mut());
                if let Ok(surface) = self.native_surface.as_mut() {
                    surface.hide();
                }
                if self.external_renderer {
                    self.canvas.window_mut().hide();
                }
                self.visible = false;
                continue;
            }
            #[cfg(target_os = "linux")]
            if handle_linux_stats_shortcut(&mut self.debug_overlay, &event) {
                continue;
            }
            if self.external_renderer
                && handle_native_window_shortcut(self.canvas.window_mut(), &event)
            {
                continue;
            }
            let window_size = self.canvas.window().size();
            let stream_size = self.texture_size.unwrap_or(window_size);
            self.input_capture.handle_event(
                &self._sdl,
                self.canvas.window_mut(),
                stream_size,
                event,
            );
        }
        #[cfg(target_os = "linux")]
        if let Some(raw_input) = self.raw_input.as_ref() {
            raw_input.set_enabled(
                self.visible && self.input_capture.focused && self.input_capture.relative_mouse,
            );
        }
        if self.paused || !self.visible {
            self.output.take_video();
            #[cfg(target_os = "linux")]
            self.output.clear_linux_video();
            return Ok(false);
        }
        #[cfg(target_os = "linux")]
        if let Some(frame) = self.output.take_linux_video() {
            return self.present_linux_frame(frame.frame);
        }
        let Some(frame) = self.output.take_video() else {
            return Ok(false);
        };
        #[cfg(target_os = "linux")]
        let frame = {
            let mut frame = frame;
            self.presented_linux_frame = false;
            self.debug_overlay.update(
                self.presented_frames,
                self.output.software_video_drops(),
                self.input_capture.relative_mouse,
                self.output.received_video_bytes(),
            );
            self.debug_overlay.composite_rgb24(
                &mut frame.rgb,
                frame.width,
                frame.height,
                frame.width as usize * 3,
            );
            frame
        };
        if self.texture_size != Some((frame.width, frame.height))
            || self.texture_format != Some(PixelFormatEnum::RGB24)
        {
            self.texture = Some(
                self.canvas
                    .texture_creator()
                    .create_texture_streaming(PixelFormatEnum::RGB24, frame.width, frame.height)
                    .map_err(|error| format!("video texture creation failed: {error}"))?,
            );
            self.texture_size = Some((frame.width, frame.height));
            self.texture_format = Some(PixelFormatEnum::RGB24);
        }
        let texture = self.texture.as_mut().expect("texture was just created");
        texture
            .update(None, &frame.rgb, frame.width as usize * 3)
            .map_err(|error| format!("video texture upload failed: {error}"))?;
        let (output_width, output_height) = self.canvas.output_size()?;
        let target = aspect_fit(frame.width, frame.height, output_width, output_height);
        self.canvas.set_draw_color(Color::BLACK);
        self.canvas.clear();
        self.canvas.copy(texture, None, target)?;
        self.canvas.present();
        #[cfg(target_os = "linux")]
        {
            self.presented_frames = self.presented_frames.saturating_add(1);
        }
        Ok(true)
    }

    #[cfg(target_os = "linux")]
    fn present_linux_frame(&mut self, mut frame: LinuxDecodedVideoFrame) -> Result<bool, String> {
        use opennow_streamer_platform_linux::PixelFormat;

        if frame.format.pixel_format != PixelFormat::Nv12 || frame.planes.len() != 2 {
            return Err(format!(
                "SDL Linux presentation requires NV12 with two planes, received {:?}",
                frame.format.pixel_format
            ));
        }
        let width = frame.format.width;
        let height = frame.format.height;
        self.debug_overlay
            .set_presentation_skips(self.output.display_video_skips());
        self.debug_overlay.update(
            self.presented_frames,
            self.output.hardware_video_drops(),
            self.input_capture.relative_mouse,
            self.output.received_video_bytes(),
        );
        self.debug_overlay.composite_linux_frame(&mut frame);
        if self.texture_size != Some((width, height))
            || self.texture_format != Some(PixelFormatEnum::NV12)
        {
            self.texture = Some(
                self.canvas
                    .texture_creator()
                    .create_texture_streaming(PixelFormatEnum::NV12, width, height)
                    .map_err(|error| format!("NV12 texture creation failed: {error}"))?,
            );
            self.texture_size = Some((width, height));
            self.texture_format = Some(PixelFormatEnum::NV12);
        }
        let y_plane = &frame.planes[0];
        let uv_plane = &frame.planes[1];
        self.texture
            .as_mut()
            .expect("NV12 texture was just created")
            .with_lock(None, |destination, pitch| {
                for row in 0..height as usize {
                    let source_start = row * y_plane.stride;
                    let destination_start = row * pitch;
                    destination[destination_start..destination_start + width as usize]
                        .copy_from_slice(
                            &y_plane.data[source_start..source_start + width as usize],
                        );
                }
                let uv_base = pitch * height as usize;
                for row in 0..height as usize / 2 {
                    let source_start = row * uv_plane.stride;
                    let destination_start = uv_base + row * pitch;
                    destination[destination_start..destination_start + width as usize]
                        .copy_from_slice(
                            &uv_plane.data[source_start..source_start + width as usize],
                        );
                }
            })
            .map_err(|error| format!("NV12 texture upload failed: {error}"))?;
        let (output_width, output_height) = self.canvas.output_size()?;
        let target = aspect_fit(width, height, output_width, output_height);
        self.canvas.set_draw_color(Color::BLACK);
        self.canvas.clear();
        self.canvas.copy(
            self.texture.as_ref().expect("NV12 texture exists"),
            None,
            target,
        )?;
        self.canvas.present();
        self.presented_frames = self.presented_frames.saturating_add(1);
        self.presented_linux_frame = true;
        Ok(true)
    }

    fn backend_label(&self) -> &'static str {
        #[cfg(target_os = "linux")]
        if self.presented_linux_frame {
            return "Linux decoder/SDL NV12";
        }
        "OpenH264/SDL"
    }

    fn take_captured_input(&mut self) -> Vec<CapturedInput> {
        self.input_capture.take()
    }

    fn update_cursor(&mut self, bytes: &[u8]) {
        if self.external_renderer {
            let stream_size = self
                .texture_size
                .unwrap_or_else(|| self.canvas.window().size());
            self.input_capture.apply_cursor(
                &self._sdl,
                self.canvas.window_mut(),
                stream_size,
                bytes,
            );
        }
    }

    fn flush_captured_input(&mut self) {
        let captured_input = self.output.captured_input();
        for input in self.take_captured_input() {
            captured_input.push(input);
        }
    }
}

impl Drop for SoftwareOutput {
    fn drop(&mut self) {
        self.stop();
    }
}

fn sdl_virtual_key(scancode: sdl2::keyboard::Scancode) -> Option<u16> {
    use sdl2::keyboard::Scancode;

    Some(match scancode {
        Scancode::A => 0x41,
        Scancode::B => 0x42,
        Scancode::C => 0x43,
        Scancode::D => 0x44,
        Scancode::E => 0x45,
        Scancode::F => 0x46,
        Scancode::G => 0x47,
        Scancode::H => 0x48,
        Scancode::I => 0x49,
        Scancode::J => 0x4a,
        Scancode::K => 0x4b,
        Scancode::L => 0x4c,
        Scancode::M => 0x4d,
        Scancode::N => 0x4e,
        Scancode::O => 0x4f,
        Scancode::P => 0x50,
        Scancode::Q => 0x51,
        Scancode::R => 0x52,
        Scancode::S => 0x53,
        Scancode::T => 0x54,
        Scancode::U => 0x55,
        Scancode::V => 0x56,
        Scancode::W => 0x57,
        Scancode::X => 0x58,
        Scancode::Y => 0x59,
        Scancode::Z => 0x5a,
        Scancode::Num0 => 0x30,
        Scancode::Num1 => 0x31,
        Scancode::Num2 => 0x32,
        Scancode::Num3 => 0x33,
        Scancode::Num4 => 0x34,
        Scancode::Num5 => 0x35,
        Scancode::Num6 => 0x36,
        Scancode::Num7 => 0x37,
        Scancode::Num8 => 0x38,
        Scancode::Num9 => 0x39,
        Scancode::Return | Scancode::KpEnter => 0x0d,
        Scancode::Escape => 0x1b,
        Scancode::Backspace => 0x08,
        Scancode::Tab => 0x09,
        Scancode::Space => 0x20,
        Scancode::Minus => 0xbd,
        Scancode::Equals | Scancode::KpEquals => 0xbb,
        Scancode::LeftBracket => 0xdb,
        Scancode::RightBracket => 0xdd,
        Scancode::Backslash => 0xdc,
        Scancode::NonUsBackslash => 0xe2,
        Scancode::Semicolon => 0xba,
        Scancode::Apostrophe => 0xde,
        Scancode::Grave => 0xc0,
        Scancode::Comma => 0xbc,
        Scancode::Period => 0xbe,
        Scancode::Slash => 0xbf,
        Scancode::F1 => 0x70,
        Scancode::F2 => 0x71,
        Scancode::F3 => 0x72,
        Scancode::F4 => 0x73,
        Scancode::F5 => 0x74,
        Scancode::F6 => 0x75,
        Scancode::F7 => 0x76,
        Scancode::F8 => 0x77,
        Scancode::F9 => 0x78,
        Scancode::F10 => 0x79,
        Scancode::F11 => 0x7a,
        Scancode::F12 => 0x7b,
        Scancode::F13 => 0x7c,
        Scancode::F14 => 0x7d,
        Scancode::F15 => 0x7e,
        Scancode::F16 => 0x7f,
        Scancode::F17 => 0x80,
        Scancode::F18 => 0x81,
        Scancode::F19 => 0x82,
        Scancode::F20 => 0x83,
        Scancode::F21 => 0x84,
        Scancode::F22 => 0x85,
        Scancode::F23 => 0x86,
        Scancode::F24 => 0x87,
        Scancode::Right => 0x27,
        Scancode::Left => 0x25,
        Scancode::Down => 0x28,
        Scancode::Up => 0x26,
        Scancode::LCtrl => 0xa2,
        Scancode::LShift => 0xa0,
        Scancode::LAlt => 0xa4,
        Scancode::LGui => 0x5b,
        Scancode::RCtrl => 0xa3,
        Scancode::RShift => 0xa1,
        Scancode::RAlt => 0xa5,
        Scancode::RGui => 0x5c,
        Scancode::CapsLock => 0x14,
        Scancode::NumLockClear => 0x90,
        Scancode::Insert => 0x2d,
        Scancode::Delete => 0x2e,
        Scancode::Home => 0x24,
        Scancode::End => 0x23,
        Scancode::PageUp => 0x21,
        Scancode::PageDown => 0x22,
        Scancode::PrintScreen => 0x2a,
        Scancode::ScrollLock => 0x91,
        Scancode::Pause => 0x13,
        Scancode::Application => 0x5d,
        Scancode::Kp0 => 0x60,
        Scancode::Kp1 => 0x61,
        Scancode::Kp2 => 0x62,
        Scancode::Kp3 => 0x63,
        Scancode::Kp4 => 0x64,
        Scancode::Kp5 => 0x65,
        Scancode::Kp6 => 0x66,
        Scancode::Kp7 => 0x67,
        Scancode::Kp8 => 0x68,
        Scancode::Kp9 => 0x69,
        Scancode::KpPlus => 0x6b,
        Scancode::KpMinus => 0x6d,
        Scancode::KpMultiply => 0x6a,
        Scancode::KpDivide => 0x6f,
        Scancode::KpPeriod => 0x6e,
        _ => return None,
    })
}

fn sdl_modifiers(scancode: sdl2::keyboard::Scancode, modifiers: sdl2::keyboard::Mod) -> u16 {
    use sdl2::keyboard::{Mod, Scancode};

    let mut flags = 0;
    if !matches!(scancode, Scancode::LShift | Scancode::RShift)
        && modifiers.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD)
    {
        flags |= 0x01;
    }
    if !matches!(scancode, Scancode::LCtrl | Scancode::RCtrl)
        && modifiers.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD)
    {
        flags |= 0x02;
    }
    if !matches!(scancode, Scancode::LAlt | Scancode::RAlt)
        && modifiers.intersects(Mod::LALTMOD | Mod::RALTMOD)
    {
        flags |= 0x04;
    }
    if !matches!(scancode, Scancode::LGui | Scancode::RGui)
        && modifiers.intersects(Mod::LGUIMOD | Mod::RGUIMOD)
    {
        flags |= 0x08;
    }
    flags
}

fn sdl_mouse_button(button: sdl2::mouse::MouseButton) -> Option<u8> {
    use sdl2::mouse::MouseButton;

    match button {
        MouseButton::Left => Some(1),
        MouseButton::Middle => Some(2),
        MouseButton::Right => Some(3),
        MouseButton::X1 => Some(4),
        MouseButton::X2 => Some(5),
        MouseButton::Unknown => None,
    }
}

fn push_mouse_motion(input: &mut Vec<CapturedInput>, delta_x: i32, delta_y: i32) {
    if delta_x == 0 && delta_y == 0 {
        return;
    }
    input.push(CapturedInput::MouseMove {
        delta_x: clamp_i16(delta_x),
        delta_y: clamp_i16(delta_y),
    });
}

fn clamp_i16(value: i32) -> i16 {
    value.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

fn clamp_i32_u16(value: i32) -> u16 {
    value.clamp(0, i32::from(u16::MAX)) as u16
}

fn clamp_u32_u16(value: u32) -> u16 {
    value.min(u32::from(u16::MAX)) as u16
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AbsoluteMouseViewport {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
}

struct CustomCursorPayload<'a> {
    hotspot_x: u8,
    hotspot_y: u8,
    image_base64: &'a [u8],
    scale: f32,
}

fn cursor_state_for_message(message_type: u8, cursor_id: u8) -> RemoteCursorState {
    if message_type == 0 && cursor_id == 0 {
        RemoteCursorState::Hidden
    } else {
        // Custom cursor ID 0 is still a visible bitmap. Only predefined ID 0
        // is the protocol's hidden/raw-input cursor.
        RemoteCursorState::Visible
    }
}

fn parse_custom_cursor_payload(bytes: &[u8]) -> Result<CustomCursorPayload<'_>, String> {
    if bytes.first() != Some(&1) || bytes.len() < 7 {
        return Err("not a complete custom cursor message".to_owned());
    }
    let mime_length = usize::from(bytes[4]);
    let mime_end = 5_usize
        .checked_add(mime_length)
        .ok_or_else(|| "custom cursor MIME length overflow".to_owned())?;
    let image_length_bytes = bytes
        .get(mime_end..mime_end + 2)
        .ok_or_else(|| "custom cursor is missing its image length".to_owned())?;
    let image_length = usize::from(u16::from_le_bytes([
        image_length_bytes[0],
        image_length_bytes[1],
    ]));
    let image_start = mime_end + 2;
    let image_end = image_start
        .checked_add(image_length)
        .ok_or_else(|| "custom cursor image length overflow".to_owned())?;
    let image_base64 = bytes
        .get(image_start..image_end)
        .ok_or_else(|| "custom cursor image is truncated".to_owned())?;
    let position_end = image_end.saturating_add(4);
    let scale = bytes
        .get(position_end..position_end + 2)
        .map(|raw| f32::from(u16::from_le_bytes([raw[0], raw[1]])) / 100.0)
        .filter(|scale| scale.is_finite() && *scale > 0.0)
        .unwrap_or(1.0);
    Ok(CustomCursorPayload {
        hotspot_x: bytes[2],
        hotspot_y: bytes[3],
        image_base64,
        scale,
    })
}

fn custom_sdl_cursor(bytes: &[u8]) -> Result<sdl2::mouse::Cursor, String> {
    const MAX_CURSOR_EXTENT: u32 = 256;
    let payload = parse_custom_cursor_payload(bytes)?;
    let encoded = std::str::from_utf8(payload.image_base64)
        .map_err(|error| format!("cursor image base64 is not UTF-8: {error}"))?;
    let compressed = BASE64
        .decode(encoded)
        .map_err(|error| format!("cursor image base64 is invalid: {error}"))?;
    let image = ImageReader::new(IoCursor::new(compressed))
        .with_guessed_format()
        .map_err(|error| format!("cursor image format is unknown: {error}"))?
        .decode()
        .map_err(|error| format!("cursor image decode failed: {error}"))?
        .to_rgba8();
    let (source_width, source_height) = image.dimensions();
    if source_width == 0
        || source_height == 0
        || source_width > MAX_CURSOR_EXTENT
        || source_height > MAX_CURSOR_EXTENT
    {
        return Err(format!(
            "cursor image dimensions are outside 1..={MAX_CURSOR_EXTENT}: {source_width}x{source_height}"
        ));
    }
    let width = ((source_width as f32 / payload.scale).round() as u32).clamp(1, MAX_CURSOR_EXTENT);
    let height =
        ((source_height as f32 / payload.scale).round() as u32).clamp(1, MAX_CURSOR_EXTENT);
    let rgba = if (width, height) == (source_width, source_height) {
        image
    } else {
        image::imageops::resize(&image, width, height, image::imageops::FilterType::Triangle)
    };
    let row_bytes = width as usize * 4;
    let mut surface = sdl2::surface::Surface::new(width, height, PixelFormatEnum::RGBA32)?;
    let pitch = surface.pitch() as usize;
    surface.with_lock_mut(|destination| {
        for (source, destination) in rgba
            .as_raw()
            .chunks_exact(row_bytes)
            .zip(destination.chunks_mut(pitch))
        {
            destination[..row_bytes].copy_from_slice(source);
        }
    });
    let hotspot_x = ((f32::from(payload.hotspot_x) / payload.scale).round() as i32)
        .clamp(0, width.saturating_sub(1) as i32);
    let hotspot_y = ((f32::from(payload.hotspot_y) / payload.scale).round() as i32)
        .clamp(0, height.saturating_sub(1) as i32);
    sdl2::mouse::Cursor::from_surface(surface, hotspot_x, hotspot_y)
}

fn map_window_point_to_stream_viewport(
    point: (i32, i32),
    stream_size: (u32, u32),
    window_size: (u32, u32),
) -> AbsoluteMouseViewport {
    let viewport = aspect_fit(
        stream_size.0.max(1),
        stream_size.1.max(1),
        window_size.0.max(1),
        window_size.1.max(1),
    );
    let width = viewport.width().max(1);
    let height = viewport.height().max(1);
    AbsoluteMouseViewport {
        x: clamp_i32_u16((point.0 - viewport.x()).clamp(0, width.saturating_sub(1) as i32)),
        y: clamp_i32_u16((point.1 - viewport.y()).clamp(0, height.saturating_sub(1) as i32)),
        width: clamp_u32_u16(width),
        height: clamp_u32_u16(height),
    }
}

fn parse_cursor_position(bytes: &[u8]) -> Option<(u16, u16)> {
    if bytes.len() < 7 || !matches!(bytes[0], 0 | 1) {
        return None;
    }
    let mime_length = usize::from(bytes[4]);
    let image_length_offset = 5_usize.checked_add(mime_length)?;
    let image_length_bytes = bytes.get(image_length_offset..image_length_offset + 2)?;
    let image_length = usize::from(u16::from_le_bytes([
        image_length_bytes[0],
        image_length_bytes[1],
    ]));
    let position_offset = image_length_offset
        .checked_add(2)?
        .checked_add(image_length)?;
    let position = bytes.get(position_offset..position_offset + 4)?;
    Some((
        u16::from_le_bytes([position[0], position[1]]),
        u16::from_le_bytes([position[2], position[3]]),
    ))
}

fn normalized_cursor_coordinate(value: u16, viewport_extent: u32) -> i32 {
    let extent = viewport_extent.max(1);
    let coordinate = (u64::from(value) * u64::from(extent) / u64::from(u16::MAX)) as u32;
    coordinate.min(extent.saturating_sub(1)) as i32
}

#[cfg(target_os = "linux")]
fn handle_linux_stats_shortcut(
    overlay: &mut NativeStatsOverlay,
    event: &sdl2::event::Event,
) -> bool {
    use sdl2::event::Event;
    use sdl2::keyboard::Scancode;

    match event {
        Event::KeyDown {
            scancode: Some(Scancode::F3),
            repeat: false,
            ..
        } => {
            overlay.toggle();
            true
        }
        Event::KeyUp {
            scancode: Some(Scancode::F3),
            ..
        } => true,
        _ => false,
    }
}

pub(crate) fn handle_native_window_shortcut(
    window: &mut sdl2::video::Window,
    event: &sdl2::event::Event,
) -> bool {
    use sdl2::event::Event;
    use sdl2::keyboard::Scancode;
    use sdl2::video::FullscreenType;

    match event {
        Event::KeyDown {
            scancode: Some(Scancode::F11),
            repeat: false,
            ..
        } => {
            let next = if window.fullscreen_state() == FullscreenType::Off {
                FullscreenType::Desktop
            } else {
                FullscreenType::Off
            };
            match window.set_fullscreen(next) {
                Ok(()) => eprintln!(
                    "External SDL fullscreen changed: {}",
                    if next == FullscreenType::Off {
                        "off"
                    } else {
                        "desktop"
                    }
                ),
                Err(error) => eprintln!("External SDL fullscreen toggle failed: {error}"),
            }
            true
        }
        Event::KeyUp {
            scancode: Some(Scancode::F11),
            ..
        } => true,
        _ => false,
    }
}

pub(crate) fn native_input_capture_enabled() -> bool {
    native_input_capture_enabled_value(std::env::var("OPENNOW_NATIVE_INPUT_OWNER").ok().as_deref())
}

fn native_input_capture_enabled_value(owner: Option<&str>) -> bool {
    owner.is_some_and(|owner| owner.trim().eq_ignore_ascii_case("native"))
}

pub(crate) enum ActiveOutput {
    Software(Box<SoftwareOutput>),
    #[cfg(target_os = "windows")]
    Windows(WindowsOutput),
    #[cfg(target_os = "linux")]
    LinuxHardware(Box<LinuxHardwareOutput>),
    #[cfg(target_os = "macos")]
    Mac(Box<crate::macos_backend::MacOutput>),
}

impl ActiveOutput {
    #[cfg(target_os = "macos")]
    pub(crate) fn is_macos_hardware(&self) -> bool {
        matches!(self, Self::Mac(_))
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn is_linux_hardware(&self) -> bool {
        matches!(self, Self::LinuxHardware(_))
    }

    pub(crate) fn initialize(
        output: Arc<OutputBuffers>,
        use_hardware: bool,
        stream: MediaStreamConfig,
        #[cfg(target_os = "windows")] windows_bridge: Arc<WindowsBridge>,
        use_linux_hardware: bool,
    ) -> Result<Self, String> {
        #[cfg(target_os = "macos")]
        if use_hardware {
            return crate::macos_backend::MacOutput::initialize(stream)
                .map(Box::new)
                .map(Self::Mac);
        }
        #[cfg(target_os = "windows")]
        if use_hardware {
            return WindowsOutput::initialize(windows_bridge, output, stream).map(Self::Windows);
        }
        #[cfg(target_os = "linux")]
        if use_linux_hardware {
            return LinuxHardwareOutput::initialize(output, stream)
                .map(Box::new)
                .map(Self::LinuxHardware);
        }
        let _ = use_hardware;
        let _ = use_linux_hardware;
        SoftwareOutput::initialize(output, stream)
            .map(Box::new)
            .map(Self::Software)
    }

    pub(crate) fn start(&mut self, surface: Option<&RenderSurface>) -> Result<(), String> {
        match self {
            Self::Software(output) => output.start(surface),
            #[cfg(target_os = "windows")]
            Self::Windows(output) => output.start(surface),
            #[cfg(target_os = "linux")]
            Self::LinuxHardware(output) => output.start(surface),
            #[cfg(target_os = "macos")]
            Self::Mac(output) => output.start(surface),
        }
    }

    pub(crate) fn set_paused(&mut self, paused: bool) -> Result<(), String> {
        match self {
            Self::Software(output) => {
                output.set_paused(paused);
                Ok(())
            }
            #[cfg(target_os = "windows")]
            Self::Windows(output) => output.set_paused(paused),
            #[cfg(target_os = "linux")]
            Self::LinuxHardware(output) => {
                output.set_paused(paused);
                Ok(())
            }
            #[cfg(target_os = "macos")]
            Self::Mac(output) => output.set_paused(paused),
        }
    }

    pub(crate) fn stop(&mut self) {
        match self {
            Self::Software(output) => output.stop(),
            #[cfg(target_os = "windows")]
            Self::Windows(output) => output.stop(),
            #[cfg(target_os = "linux")]
            Self::LinuxHardware(output) => output.stop(),
            #[cfg(target_os = "macos")]
            Self::Mac(output) => output.stop(),
        }
    }

    pub(crate) fn update_surface(&mut self, surface: &RenderSurface) -> Result<(), String> {
        match self {
            Self::Software(output) => output.update_surface(surface),
            #[cfg(target_os = "windows")]
            Self::Windows(output) => output.update_surface(surface),
            #[cfg(target_os = "linux")]
            Self::LinuxHardware(output) => output.update_surface(surface),
            #[cfg(target_os = "macos")]
            Self::Mac(output) => output.update_surface(surface),
        }
    }

    pub(crate) fn pump(&mut self) -> Result<OutputEvent, String> {
        match self {
            Self::Software(output) => {
                let presented = output.pump()?;
                Ok(if presented {
                    OutputEvent::Presented(output.backend_label())
                } else {
                    OutputEvent::None
                })
            }
            #[cfg(target_os = "windows")]
            Self::Windows(output) => output.pump(),
            #[cfg(target_os = "linux")]
            Self::LinuxHardware(output) => output.pump().map(|presented| {
                if presented {
                    OutputEvent::Presented("Linux decoder/Vulkan")
                } else {
                    OutputEvent::None
                }
            }),
            #[cfg(target_os = "macos")]
            Self::Mac(output) => output.pump().map(|_| OutputEvent::None),
        }
    }

    pub(crate) fn take_captured_input(&mut self) -> Vec<CapturedInput> {
        match self {
            Self::Software(output) => output.take_captured_input(),
            #[cfg(target_os = "windows")]
            Self::Windows(output) => output.take_captured_input(),
            #[cfg(target_os = "linux")]
            Self::LinuxHardware(output) => output.take_captured_input(),
            #[cfg(target_os = "macos")]
            Self::Mac(output) => output.take_captured_input(),
        }
    }

    pub(crate) fn update_cursor(&mut self, bytes: &[u8]) {
        match self {
            Self::Software(output) => output.update_cursor(bytes),
            #[cfg(target_os = "windows")]
            Self::Windows(output) => output.update_cursor(bytes),
            #[cfg(target_os = "linux")]
            Self::LinuxHardware(output) => output.update_cursor(bytes),
            #[cfg(target_os = "macos")]
            Self::Mac(output) => output.update_cursor(bytes),
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn configure_macos_h264(
        &mut self,
        parameter_sets: opennow_streamer_platform_macos::H264ParameterSets,
    ) -> Result<opennow_streamer_platform_macos::StreamSink, String> {
        match self {
            Self::Mac(output) => output.configure_h264(parameter_sets),
            Self::Software(_) => Err("VideoToolbox is not the selected media backend".to_owned()),
        }
    }
}

pub(crate) enum OutputEvent {
    None,
    Presented(&'static str),
    #[cfg(target_os = "windows")]
    RequestKeyframe,
    #[cfg(target_os = "windows")]
    DeviceLost {
        subsystem: &'static str,
        recovered: bool,
        message: Option<String>,
    },
    #[cfg(target_os = "windows")]
    QueueDropped(&'static str),
    #[cfg(target_os = "windows")]
    Fatal(String),
}

#[cfg(target_os = "windows")]
struct WindowsExternalSdlSurface {
    sdl: sdl2::Sdl,
    window: sdl2::video::Window,
    event_pump: sdl2::EventPump,
    native_surface: NativeSurface,
    input_capture: SdlInputCapture,
    raw_input: Option<WindowsRawInputController>,
    debug_overlay: NativeDebugOverlay,
    stream_size: (u32, u32),
    visible: bool,
    focused: bool,
}

#[cfg(target_os = "windows")]
impl WindowsExternalSdlSurface {
    fn initialize(
        stream: MediaStreamConfig,
        graphics_api: WindowsGraphicsApi,
        captured_input: Arc<CapturedInputQueue>,
    ) -> Result<Self, String> {
        // Force the Windows RawInput path. Warp-relative motion is quantized by
        // cursor recentering and Windows pointer scaling, which is particularly
        // noticeable in 120 FPS first-person games.
        sdl2::hint::set("SDL_MOUSE_RELATIVE_MODE_WARP", "0");
        sdl2::hint::set("SDL_MOUSE_RELATIVE_SCALING", "0");
        let sdl = sdl2::init().map_err(|error| format!("SDL initialization failed: {error}"))?;
        let video = sdl
            .video()
            .map_err(|error| format!("SDL video initialization failed: {error}"))?;
        let window = video
            .window("OpenNOW Stream", 1280, 720)
            .position_centered()
            .resizable()
            .hidden()
            .build()
            .map_err(|error| format!("external SDL video window creation failed: {error}"))?;
        let native_surface = NativeSurface::new(&window)?;
        let debug_overlay = NativeDebugOverlay::new(
            &video,
            native_surface.window_handle(),
            stream,
            match graphics_api {
                WindowsGraphicsApi::D3d12 => "D3D11VA / D3D12",
                WindowsGraphicsApi::D3d11 => "D3D11VA ZERO-COPY",
            },
        )?;
        let event_pump = sdl
            .event_pump()
            .map_err(|error| format!("external SDL event pump creation failed: {error}"))?;
        let capture_input = native_input_capture_enabled();
        let raw_input = if capture_input {
            match WindowsRawInputController::start(native_surface.window_handle(), captured_input) {
                Ok(controller) => {
                    eprintln!("Dedicated Windows Raw Input mouse thread ready");
                    Some(controller)
                }
                Err(error) => {
                    eprintln!(
                        "Dedicated Windows Raw Input unavailable; retaining SDL motion: {error}"
                    );
                    None
                }
            }
        } else {
            None
        };
        eprintln!(
            "External SDL stream window ready (native keyboard/mouse capture: {capture_input})"
        );
        let external_relative_motion = raw_input.is_some();
        Ok(Self {
            sdl,
            window,
            event_pump,
            native_surface,
            input_capture: SdlInputCapture::new(capture_input, external_relative_motion),
            raw_input,
            debug_overlay,
            stream_size: (stream.width, stream.height),
            visible: false,
            focused: false,
        })
    }

    fn target(&self) -> Result<SurfaceTarget, String> {
        let handle = WindowHandle::new(self.native_surface.window_handle())
            .ok_or_else(|| "external SDL window returned an empty HWND".to_owned())?;
        Ok(SurfaceTarget::Existing(ExistingWindow { hwnd: handle }))
    }

    fn update(&mut self, surface: &RenderSurface) -> Result<(), String> {
        let Some(rect) = surface.rect.filter(|_| surface.visible) else {
            self.input_capture.release(&self.sdl, &mut self.window);
            if let Some(raw_input) = self.raw_input.as_ref() {
                raw_input.set_enabled(false);
            }
            self.debug_overlay.hide();
            self.window.hide();
            self.visible = false;
            return Ok(());
        };
        let bounds = surface.screen_rect.unwrap_or(rect);
        self.window.set_position(
            sdl2::video::WindowPos::Positioned(bounds.x),
            sdl2::video::WindowPos::Positioned(bounds.y),
        );
        self.window
            .set_size(bounds.width.max(2), bounds.height.max(2))
            .map_err(|error| format!("failed to resize external SDL video surface: {error}"))?;
        self.window.show();
        if !self.visible {
            self.window.raise();
        }
        self.visible = true;
        self.focused = self.window.has_input_focus();
        if let Some(raw_input) = self.raw_input.as_ref() {
            raw_input.set_target(self.native_surface.window_handle());
        }
        if self.focused {
            self.debug_overlay.show_if_enabled();
        }
        Ok(())
    }

    fn pump(&mut self, presented_frames: u64, dropped_frames: u64) {
        let stream_window_id = self.window.id();
        for event in self.event_pump.poll_iter().collect::<Vec<_>>() {
            if matches!(event, sdl2::event::Event::Quit { .. }) {
                self.input_capture.release(&self.sdl, &mut self.window);
                self.window.hide();
                self.debug_overlay.hide();
                self.visible = false;
                self.focused = false;
            } else if matches!(
                event,
                sdl2::event::Event::Window {
                    window_id,
                    win_event: sdl2::event::WindowEvent::FocusLost,
                    ..
                } if window_id == stream_window_id
            ) {
                self.focused = false;
                self.debug_overlay.hide();
            } else if matches!(
                event,
                sdl2::event::Event::Window {
                    window_id,
                    win_event: sdl2::event::WindowEvent::FocusGained,
                    ..
                } if window_id == stream_window_id
            ) {
                self.focused = true;
                self.debug_overlay.show_if_enabled();
            } else if matches!(
                event,
                sdl2::event::Event::KeyDown {
                    scancode: Some(sdl2::keyboard::Scancode::F3),
                    repeat: false,
                    ..
                }
            ) {
                self.debug_overlay.toggle();
                continue;
            } else if matches!(
                event,
                sdl2::event::Event::KeyUp {
                    scancode: Some(sdl2::keyboard::Scancode::F3),
                    ..
                }
            ) {
                continue;
            } else if handle_native_window_shortcut(&mut self.window, &event) {
                continue;
            } else {
                self.input_capture.handle_event(
                    &self.sdl,
                    &mut self.window,
                    self.stream_size,
                    event,
                );
            }
        }
        if let Some(raw_input) = self.raw_input.as_ref() {
            raw_input
                .set_enabled(self.visible && self.focused && self.input_capture.relative_mouse);
        }
        self.debug_overlay.update(
            presented_frames,
            dropped_frames,
            self.input_capture.relative_mouse,
        );
    }

    fn release(&mut self) {
        self.input_capture.release(&self.sdl, &mut self.window);
        if let Some(raw_input) = self.raw_input.as_ref() {
            raw_input.set_enabled(false);
        }
        self.debug_overlay.hide();
        self.window.hide();
        self.visible = false;
        self.focused = false;
    }

    fn take_captured_input(&mut self) -> Vec<CapturedInput> {
        self.input_capture.take()
    }

    fn update_cursor(&mut self, bytes: &[u8]) {
        self.input_capture
            .apply_cursor(&self.sdl, &mut self.window, self.stream_size, bytes);
    }
}

#[cfg(target_os = "windows")]
pub(crate) struct WindowsOutput {
    backend: Arc<WindowsBackend>,
    graphics_api: WindowsGraphicsApi,
    bridge: Arc<WindowsBridge>,
    output: Arc<OutputBuffers>,
    external_surface: Option<WindowsExternalSdlSurface>,
    dropped_video_frames: u64,
    stopped: bool,
}

#[cfg(target_os = "windows")]
impl WindowsOutput {
    fn initialize(
        bridge: Arc<WindowsBridge>,
        output: Arc<OutputBuffers>,
        stream: MediaStreamConfig,
    ) -> Result<Self, String> {
        bridge.reset();
        let external_renderer = external_renderer_enabled();
        let graphics_api = selected_windows_graphics_api(stream.codec);
        let external_surface = if external_renderer {
            Some(WindowsExternalSdlSurface::initialize(
                stream,
                graphics_api,
                output.captured_input(),
            )?)
        } else {
            None
        };
        let initial_surface = match external_surface.as_ref() {
            Some(surface) => surface.target()?,
            None => hidden_windows_surface(),
        };
        let backend = Arc::new(
            WindowsBackend::start_for(
                graphics_api,
                BackendConfig {
                    video: VideoFormat {
                        codec: match stream.codec {
                            crate::media::MediaVideoCodec::H264 => VideoCodec::H264,
                            crate::media::MediaVideoCodec::H265 => VideoCodec::H265,
                            crate::media::MediaVideoCodec::Av1 => VideoCodec::Av1,
                        },
                        width: stream.width,
                        height: stream.height,
                        frame_rate_numerator: std::num::NonZeroU32::new(stream.fps.max(1))
                            .expect("fps is clamped non-zero"),
                        frame_rate_denominator: std::num::NonZeroU32::new(1)
                            .expect("one is non-zero"),
                        average_bitrate: stream.bitrate_bps.max(1),
                    },
                    audio: AudioFormat {
                        sample_rate: AUDIO_SAMPLE_RATE as u32,
                        channels: AUDIO_CHANNELS as u16,
                    },
                    surface: initial_surface,
                    video_queue_capacity:
                        opennow_streamer_platform_windows::ADAPTIVE_VIDEO_QUEUE_CAPACITY,
                    audio_queue_capacity: 4,
                },
            )
            .map_err(|error| error.to_string())?,
        );
        bridge.replace_backend(Some(Arc::clone(&backend)));
        Ok(Self {
            backend,
            graphics_api,
            bridge,
            output,
            external_surface,
            dropped_video_frames: 0,
            stopped: false,
        })
    }

    fn start(&mut self, surface: Option<&RenderSurface>) -> Result<(), String> {
        if let Some(surface) = surface {
            self.update_surface(surface)?;
        }
        Ok(())
    }

    fn set_paused(&mut self, paused: bool) -> Result<(), String> {
        if paused && let Some(surface) = self.external_surface.as_mut() {
            surface
                .input_capture
                .release(&surface.sdl, &mut surface.window);
        }
        self.backend
            .set_paused(paused)
            .map_err(|error| error.to_string())
    }

    fn update_surface(&mut self, surface: &RenderSurface) -> Result<(), String> {
        if let Some(external_surface) = self.external_surface.as_mut() {
            external_surface.update(surface)?;
            return self
                .backend
                .set_surface(external_surface.target()?)
                .map_err(|error| error.to_string());
        }
        self.backend
            .set_surface(windows_surface(surface)?)
            .map_err(|error| error.to_string())
    }

    fn pump(&mut self) -> Result<OutputEvent, String> {
        if let Some(surface) = self.external_surface.as_mut() {
            surface.pump(self.backend.presented_frames(), self.dropped_video_frames);
        }
        let Some(event) = self.backend.try_event() else {
            return Ok(OutputEvent::None);
        };
        Ok(match event {
            BackendEvent::FirstFramePresented => OutputEvent::Presented(match self.graphics_api {
                WindowsGraphicsApi::D3d12 => "Media Foundation/D3D11-on-12/D3D12/WASAPI",
                WindowsGraphicsApi::D3d11 => "Media Foundation/D3D11/WASAPI",
            }),
            BackendEvent::KeyFrameRequired => {
                self.bridge.require_keyframe();
                OutputEvent::RequestKeyframe
            }
            BackendEvent::DeviceLost { subsystem, message } => OutputEvent::DeviceLost {
                subsystem: subsystem_label(subsystem),
                recovered: false,
                message: Some(message),
            },
            BackendEvent::DeviceRecovered(subsystem) => OutputEvent::DeviceLost {
                subsystem: subsystem_label(subsystem),
                recovered: true,
                message: None,
            },
            BackendEvent::Fatal(error) => OutputEvent::Fatal(error.to_string()),
            BackendEvent::QueueOverflow(subsystem) => {
                if matches!(
                    subsystem,
                    Subsystem::VideoDecode | Subsystem::VideoPresentation
                ) {
                    self.dropped_video_frames = self.dropped_video_frames.saturating_add(1);
                }
                if subsystem == Subsystem::VideoDecode {
                    self.bridge.require_keyframe();
                }
                OutputEvent::QueueDropped(subsystem_label(subsystem))
            }
            BackendEvent::StateChanged(_) | BackendEvent::VideoFormatChanged(_) => {
                OutputEvent::None
            }
        })
    }

    fn stop(&mut self) {
        if self.stopped {
            return;
        }
        self.bridge.replace_backend(None);
        if let Some(surface) = self.external_surface.as_mut() {
            surface.release();
            let captured_input = self.output.captured_input();
            for input in surface.take_captured_input() {
                captured_input.push(input);
            }
        }
        self.backend.stop();
        self.stopped = true;
    }

    fn take_captured_input(&mut self) -> Vec<CapturedInput> {
        self.external_surface
            .as_mut()
            .map(WindowsExternalSdlSurface::take_captured_input)
            .unwrap_or_default()
    }

    fn update_cursor(&mut self, bytes: &[u8]) {
        if let Some(surface) = self.external_surface.as_mut() {
            surface.update_cursor(bytes);
        }
    }
}

#[cfg(target_os = "windows")]
fn selected_windows_graphics_api(codec: crate::media::MediaVideoCodec) -> WindowsGraphicsApi {
    let d3d12_allowed = crate::runtime::backend_preference_allows("d3d12");
    let d3d11_allowed = crate::runtime::backend_preference_allows("d3d11");
    select_windows_graphics_api(
        codec,
        d3d12_allowed,
        d3d11_allowed,
        WindowsBackend::probe_for(WindowsGraphicsApi::D3d12).bundled_backend_available(),
    )
}

#[cfg(target_os = "windows")]
fn select_windows_graphics_api(
    codec: crate::media::MediaVideoCodec,
    d3d12_allowed: bool,
    d3d11_allowed: bool,
    d3d12_available: bool,
) -> WindowsGraphicsApi {
    // Media Foundation exposes AV1 decode surfaces through D3D11. Under Auto,
    // keep those surfaces on D3D11 instead of synchronizing and flushing them
    // through D3D11-on-12 for every 120 Hz frame. An explicit D3D12 selection
    // remains authoritative for diagnostics and user preference.
    if d3d12_allowed
        && (!d3d11_allowed || (codec != crate::media::MediaVideoCodec::Av1 && d3d12_available))
    {
        WindowsGraphicsApi::D3d12
    } else {
        WindowsGraphicsApi::D3d11
    }
}

#[cfg(target_os = "windows")]
impl Drop for WindowsOutput {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(target_os = "windows")]
fn hidden_windows_surface() -> SurfaceTarget {
    SurfaceTarget::Owned(OwnedWindow {
        parent: None,
        bounds: Bounds {
            x: 0,
            y: 0,
            width: 2,
            height: 2,
        },
        visible: false,
    })
}

#[cfg(target_os = "windows")]
fn windows_surface(surface: &RenderSurface) -> Result<SurfaceTarget, String> {
    let Some(rect) = surface.rect.filter(|_| surface.visible) else {
        return Ok(hidden_windows_surface());
    };
    let parent = surface
        .window_handle
        .as_deref()
        .ok_or_else(|| "visible native surface is missing Electron windowHandle".to_owned())?;
    let parent = parse_windows_handle(parent)?;
    Ok(SurfaceTarget::Owned(OwnedWindow {
        parent: Some(parent),
        bounds: Bounds {
            x: rect.x,
            y: rect.y,
            width: rect.width.max(2),
            height: rect.height.max(2),
        },
        visible: true,
    }))
}

pub(crate) fn external_renderer_enabled() -> bool {
    external_renderer_enabled_value(
        std::env::var("OPENNOW_NATIVE_EXTERNAL_RENDERER")
            .ok()
            .as_deref(),
    )
}

fn external_renderer_enabled_value(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

#[cfg(target_os = "windows")]
fn parse_windows_handle(value: &str) -> Result<WindowHandle, String> {
    let trimmed = value.trim();
    let raw = if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        isize::from_str_radix(hex, 16)
    } else {
        trimmed.parse::<isize>()
    }
    .map_err(|error| format!("invalid Electron HWND {value:?}: {error}"))?;
    WindowHandle::new(raw).ok_or_else(|| "Electron HWND must be non-zero".to_owned())
}

#[cfg(target_os = "windows")]
fn subsystem_label(subsystem: Subsystem) -> &'static str {
    match subsystem {
        Subsystem::VideoDecode => "video-decode",
        Subsystem::VideoPresentation => "video-presentation",
        Subsystem::Audio => "audio",
    }
}

fn aspect_fit(source_width: u32, source_height: u32, width: u32, height: u32) -> Rect {
    let scale = (width as f64 / source_width as f64).min(height as f64 / source_height as f64);
    let target_width = (source_width as f64 * scale).round() as u32;
    let target_height = (source_height as f64 * scale).round() as u32;
    Rect::new(
        ((width - target_width) / 2) as i32,
        ((height - target_height) / 2) as i32,
        target_width,
        target_height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "windows")]
    #[test]
    fn automatic_av1_uses_direct_d3d11_but_explicit_d3d12_is_preserved() {
        use crate::media::MediaVideoCodec;

        assert_eq!(
            select_windows_graphics_api(MediaVideoCodec::Av1, true, true, true),
            WindowsGraphicsApi::D3d11
        );
        assert_eq!(
            select_windows_graphics_api(MediaVideoCodec::Av1, true, false, true),
            WindowsGraphicsApi::D3d12
        );
        assert_eq!(
            select_windows_graphics_api(MediaVideoCodec::H265, true, true, true),
            WindowsGraphicsApi::D3d12
        );
    }

    #[test]
    fn audio_buffer_drops_oldest_samples() {
        let output = OutputBuffers {
            video: Mutex::new(None),
            #[cfg(target_os = "linux")]
            linux_video: Mutex::new(VecDeque::with_capacity(LINUX_VIDEO_QUEUE_CAPACITY)),
            #[cfg(target_os = "linux")]
            software_video_drops: AtomicU64::new(0),
            #[cfg(target_os = "linux")]
            hardware_video_drops: AtomicU64::new(0),
            #[cfg(target_os = "linux")]
            display_video_skips: AtomicU64::new(0),
            #[cfg(target_os = "linux")]
            received_video_bytes: AtomicU64::new(0),
            audio: Mutex::new(VecDeque::new()),
            audio_capacity: 4,
            captured_input: Arc::new(CapturedInputQueue::default()),
        };
        assert_eq!(output.push_audio(&[1.0, 2.0, 3.0]), 0);
        assert_eq!(output.push_audio(&[4.0, 5.0, 6.0]), 2);
        let mut values = [0.0; 4];
        output.fill_audio(&mut values);
        assert_eq!(values, [3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn video_slot_keeps_only_the_latest_frame() {
        let output = OutputBuffers::new();
        assert!(!output.replace_video(DecodedVideoFrame {
            width: 1,
            height: 1,
            rgb: vec![1, 2, 3],
        }));
        assert!(output.replace_video(DecodedVideoFrame {
            width: 2,
            height: 1,
            rgb: vec![4; 6],
        }));
        assert_eq!(output.take_video().expect("frame").width, 2);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_video_queue_smooths_bursts_and_bounds_recovery_latency() {
        use opennow_streamer_platform_linux::StreamFormat;

        let frame = |timestamp_us| LinuxDecodedVideoFrame {
            format: StreamFormat::video_default(2, 2).expect("format"),
            planes: Vec::new(),
            dmabuf: None,
            vulkan: None,
            overlay: None,
            timestamp_us,
        };
        let output = OutputBuffers::new();
        for timestamp_us in 0..LINUX_VIDEO_QUEUE_CAPACITY as u64 {
            assert!(!output.queue_linux_video(frame(timestamp_us)));
        }
        assert!(output.queue_linux_video(frame(LINUX_VIDEO_QUEUE_CAPACITY as u64)));
        assert_eq!(output.hardware_video_drops(), 1);
        assert_eq!(
            output
                .take_linux_video()
                .expect("oldest frame")
                .frame
                .timestamp_us,
            1
        );
        assert_eq!(
            output
                .take_linux_video()
                .expect("next frame")
                .frame
                .timestamp_us,
            2
        );

        let recovery = OutputBuffers::new();
        for timestamp_us in 10..12 {
            assert!(!recovery.queue_linux_video(frame(timestamp_us)));
        }
        assert_eq!(
            recovery
                .take_linux_video_for_presentation(FrameSelectionPolicy::LatestReady, 0)
                .expect("latest recovery frame")
                .frame
                .timestamp_us,
            11,
        );
        assert_eq!(recovery.hardware_video_drops(), 0);
        assert_eq!(recovery.display_video_skips(), 1);
        assert!(recovery.take_linux_video().is_none());
    }

    #[test]
    fn aspect_fit_letterboxes_without_stretching() {
        assert_eq!(
            aspect_fit(1920, 1080, 1000, 1000),
            Rect::new(0, 218, 1000, 563)
        );
    }

    #[test]
    fn absolute_mouse_coordinates_exclude_letterbox_bars() {
        assert_eq!(
            map_window_point_to_stream_viewport((500, 500), (1920, 1080), (1000, 1000)),
            AbsoluteMouseViewport {
                x: 500,
                y: 282,
                width: 1000,
                height: 563,
            }
        );
        assert_eq!(
            map_window_point_to_stream_viewport((500, 0), (1920, 1080), (1000, 1000)),
            AbsoluteMouseViewport {
                x: 500,
                y: 0,
                width: 1000,
                height: 563,
            }
        );
    }

    #[test]
    fn cursor_channel_position_uses_normalized_stream_coordinates() {
        let message = [0, 12, 0, 0, 0, 0, 0, 0x00, 0x80, 0xff, 0xff];
        assert_eq!(parse_cursor_position(&message), Some((32768, 65535)));
        assert_eq!(normalized_cursor_coordinate(32768, 2560), 1280);
        assert_eq!(normalized_cursor_coordinate(65535, 1440), 1439);
    }

    #[test]
    fn sdl_keys_map_to_windows_virtual_keys_and_gfn_modifiers() {
        use sdl2::keyboard::{Mod, Scancode};

        assert_eq!(sdl_virtual_key(Scancode::W), Some(0x57));
        assert_eq!(sdl_virtual_key(Scancode::Escape), Some(0x1b));
        assert_eq!(sdl_virtual_key(Scancode::LCtrl), Some(0xa2));
        assert_eq!(
            sdl_modifiers(Scancode::W, Mod::LSHIFTMOD | Mod::LCTRLMOD),
            0x03
        );
        assert_eq!(sdl_modifiers(Scancode::LShift, Mod::LSHIFTMOD), 0);
    }

    #[test]
    fn adjacent_sdl_mouse_motion_is_preserved_and_clamped() {
        let mut input = Vec::new();
        push_mouse_motion(&mut input, 10, -20);
        push_mouse_motion(&mut input, i32::MAX, -30);

        assert_eq!(
            input,
            vec![
                CapturedInput::MouseMove {
                    delta_x: 10,
                    delta_y: -20,
                },
                CapturedInput::MouseMove {
                    delta_x: i16::MAX,
                    delta_y: -30,
                },
            ]
        );
    }

    #[test]
    fn native_input_capture_requires_explicit_ownership() {
        assert!(!native_input_capture_enabled_value(None));
        assert!(!native_input_capture_enabled_value(Some("electron")));
        assert!(native_input_capture_enabled_value(Some(" native ")));
    }

    #[test]
    fn native_input_capture_waits_for_the_first_server_cursor_mode() {
        let capture = SdlInputCapture::new(true, false);
        assert!(!capture.focused);
        assert!(!capture.relative_mouse);
        assert_eq!(capture.cursor_state, RemoteCursorState::Unknown);
    }

    #[test]
    fn only_predefined_cursor_zero_enters_relative_mouse_mode() {
        assert_eq!(cursor_state_for_message(0, 0), RemoteCursorState::Hidden);
        assert_eq!(cursor_state_for_message(0, 1), RemoteCursorState::Visible);
        assert_eq!(cursor_state_for_message(1, 0), RemoteCursorState::Visible);
    }

    #[test]
    fn custom_cursor_parser_preserves_bitmap_hotspot_and_scale() {
        let mime = b"image/png";
        let image = b"AAAA";
        let mut bytes = vec![1, 0, 3, 4, mime.len() as u8];
        bytes.extend_from_slice(mime);
        bytes.extend_from_slice(&(image.len() as u16).to_le_bytes());
        bytes.extend_from_slice(image);
        bytes.extend_from_slice(&10_u16.to_le_bytes());
        bytes.extend_from_slice(&20_u16.to_le_bytes());
        bytes.extend_from_slice(&150_u16.to_le_bytes());

        let cursor = parse_custom_cursor_payload(&bytes).expect("custom cursor");
        assert_eq!(cursor.hotspot_x, 3);
        assert_eq!(cursor.hotspot_y, 4);
        assert_eq!(cursor.image_base64, image);
        assert_eq!(cursor.scale, 1.5);
    }

    #[test]
    fn external_renderer_environment_values_are_explicit() {
        assert!(external_renderer_enabled_value(Some("1")));
        assert!(external_renderer_enabled_value(Some(" TRUE ")));
        assert!(!external_renderer_enabled_value(Some("0")));
        assert!(!external_renderer_enabled_value(None));
    }
}
