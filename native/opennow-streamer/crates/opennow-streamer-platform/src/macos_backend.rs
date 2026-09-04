use opennow_streamer_platform_macos::{
    AudioFormat, Av1Format, BackendConfig, BorrowedNsView, H264Format, H264Framing,
    H264ParameterSets, H265Format, H265ParameterSets, MacOsBackend, OwnedOverlayConfig,
    QueueLimits, ScreenRect, StreamSink, SurfaceTarget, VideoColorSpace, probe_av1_hardware,
    probe_h264_hardware, probe_h265_hardware,
};
use opennow_streamer_protocol::{RenderSurface, RenderSurfaceRect};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::media::{CapturedInput, MediaStreamConfig};
use crate::output::{
    OutputControl, SdlInputCapture, external_renderer_enabled, native_input_capture_enabled,
};

const HIDDEN_SURFACE: ScreenRect = ScreenRect::new(0.0, 0.0, 2.0, 2.0);
const ORDERING_POLL_INTERVAL: Duration = Duration::from_millis(100);
type H264ParameterSetBytes = (Option<Vec<u8>>, Option<Vec<u8>>);
type H265ParameterSetBytes = (Option<Vec<u8>>, Option<Vec<u8>>, Option<Vec<u8>>);

pub(crate) fn available() -> bool {
    h264_available() || h265_available() || av1_available()
}

pub(crate) fn h264_available() -> bool {
    h264_availability().load(Ordering::Acquire)
}

pub(crate) fn h265_available() -> bool {
    h265_availability().load(Ordering::Acquire)
}

pub(crate) fn av1_available() -> bool {
    av1_availability().load(Ordering::Acquire)
}

pub(crate) fn disable() {
    h264_availability().store(false, Ordering::Release);
    h265_availability().store(false, Ordering::Release);
    av1_availability().store(false, Ordering::Release);
}

pub(crate) fn disable_h265() {
    h265_availability().store(false, Ordering::Release);
}

pub(crate) fn disable_av1() {
    av1_availability().store(false, Ordering::Release);
}

fn h264_availability() -> &'static AtomicBool {
    static AVAILABLE: OnceLock<AtomicBool> = OnceLock::new();
    AVAILABLE.get_or_init(|| AtomicBool::new(probe_h264_hardware()))
}

fn h265_availability() -> &'static AtomicBool {
    static AVAILABLE: OnceLock<AtomicBool> = OnceLock::new();
    AVAILABLE.get_or_init(|| AtomicBool::new(probe_h265_hardware()))
}

fn av1_availability() -> &'static AtomicBool {
    static AVAILABLE: OnceLock<AtomicBool> = OnceLock::new();
    AVAILABLE.get_or_init(|| AtomicBool::new(probe_av1_hardware()))
}

pub(crate) struct MacOutput {
    backend: Option<MacOsBackend>,
    // The backend must detach its CAMetalLayer before SDL destroys the NSView.
    external_surface: Option<MacExternalSurface>,
    screen_rect: ScreenRect,
    visible: bool,
    paused: bool,
    last_ordering_check: Instant,
    failure_reported: bool,
}

impl MacOutput {
    pub(crate) fn initialize(stream: MediaStreamConfig) -> Result<Self, String> {
        let external_surface = external_renderer_enabled()
            .then(|| MacExternalSurface::initialize(stream))
            .transpose()?;
        Ok(Self {
            backend: None,
            external_surface,
            screen_rect: HIDDEN_SURFACE,
            visible: false,
            paused: false,
            last_ordering_check: Instant::now(),
            failure_reported: false,
        })
    }

    pub(crate) fn start(&mut self, surface: Option<&RenderSurface>) -> Result<(), String> {
        self.paused = false;
        self.failure_reported = false;
        if let Some(surface) = surface {
            self.update_surface(surface)?;
        }
        Ok(())
    }

    pub(crate) fn configure_h264(
        &mut self,
        parameter_sets: H264ParameterSets,
    ) -> Result<StreamSink, String> {
        if self.backend.is_some() {
            return Err("macOS VideoToolbox backend is already configured".to_owned());
        }
        let surface = match self.external_surface.as_ref() {
            Some(surface) => surface.target()?,
            None => SurfaceTarget::OwnedOverlay(OwnedOverlayConfig::new(
                self.screen_rect,
                self.visible && !self.paused,
            )),
        };
        let mut backend = MacOsBackend::start(BackendConfig {
            surface,
            video: H264Format::new(parameter_sets, VideoColorSpace::Bt709).into(),
            audio: AudioFormat::OPUS_STEREO_48KHZ,
            queues: QueueLimits::default(),
        })
        .map_err(|error| format!("VideoToolbox backend initialization failed: {error}"))?;
        backend
            .set_paused(self.paused)
            .map_err(|error| format!("VideoToolbox pause state failed: {error}"))?;
        let sink = backend.sink();
        self.backend = Some(backend);
        Ok(sink)
    }

    pub(crate) fn configure_h265(
        &mut self,
        parameter_sets: H265ParameterSets,
    ) -> Result<StreamSink, String> {
        if self.backend.is_some() {
            return Err("macOS VideoToolbox backend is already configured".to_owned());
        }
        let surface = match self.external_surface.as_ref() {
            Some(surface) => surface.target()?,
            None => SurfaceTarget::OwnedOverlay(OwnedOverlayConfig::new(
                self.screen_rect,
                self.visible && !self.paused,
            )),
        };
        let mut backend = MacOsBackend::start(BackendConfig {
            surface,
            video: H265Format::new(parameter_sets, VideoColorSpace::Bt709).into(),
            audio: AudioFormat::OPUS_STEREO_48KHZ,
            queues: QueueLimits::default(),
        })
        .map_err(|error| format!("VideoToolbox HEVC backend initialization failed: {error}"))?;
        backend
            .set_paused(self.paused)
            .map_err(|error| format!("VideoToolbox pause state failed: {error}"))?;
        let sink = backend.sink();
        self.backend = Some(backend);
        Ok(sink)
    }

    pub(crate) fn configure_av1(&mut self, format: Av1Format) -> Result<StreamSink, String> {
        if self.backend.is_some() {
            return Err("macOS VideoToolbox backend is already configured".to_owned());
        }
        let surface = match self.external_surface.as_ref() {
            Some(surface) => surface.target()?,
            None => SurfaceTarget::OwnedOverlay(OwnedOverlayConfig::new(
                self.screen_rect,
                self.visible && !self.paused,
            )),
        };
        let mut backend = MacOsBackend::start(BackendConfig {
            surface,
            video: format.into(),
            audio: AudioFormat::OPUS_STEREO_48KHZ,
            queues: QueueLimits::default(),
        })
        .map_err(|error| format!("VideoToolbox AV1 backend initialization failed: {error}"))?;
        backend
            .set_paused(self.paused)
            .map_err(|error| format!("VideoToolbox pause state failed: {error}"))?;
        let sink = backend.sink();
        self.backend = Some(backend);
        Ok(sink)
    }

    pub(crate) fn set_paused(&mut self, paused: bool) -> Result<(), String> {
        self.paused = paused;
        if let Some(surface) = self.external_surface.as_mut() {
            surface
                .input_capture
                .set_input_paused(paused, &surface.sdl, &mut surface.window);
            surface.set_visible(self.visible && !paused);
        }
        if let Some(backend) = self.backend.as_mut() {
            backend
                .set_paused(paused)
                .map_err(|error| format!("macOS media pause failed: {error}"))?;
            if self.external_surface.is_none() {
                backend
                    .update_owned_overlay(self.screen_rect, self.visible && !paused)
                    .map_err(|error| format!("macOS overlay pause failed: {error}"))?;
            }
        }
        Ok(())
    }

    pub(crate) fn stop(&mut self) {
        if let Some(mut backend) = self.backend.take() {
            backend.stop();
        }
        if let Some(surface) = self.external_surface.as_mut() {
            surface.hide();
        }
        self.paused = false;
    }

    pub(crate) fn update_surface(&mut self, surface: &RenderSurface) -> Result<(), String> {
        if let Some(external) = self.external_surface.as_mut() {
            self.visible = surface.visible;
            if surface.visible {
                external.update_geometry(surface)?;
            }
            external.set_visible(surface.visible && !self.paused);
            return Ok(());
        }
        if surface.visible {
            self.screen_rect = surface.screen_rect.map(screen_rect).ok_or_else(|| {
                "visible macOS overlay is missing absolute screen bounds".to_owned()
            })?;
        }
        self.visible = surface.visible && surface.screen_rect.is_some();
        if let Some(backend) = self.backend.as_mut() {
            backend
                .update_owned_overlay(self.screen_rect, self.visible && !self.paused)
                .map_err(|error| format!("macOS overlay update failed: {error}"))?;
        }
        Ok(())
    }

    pub(crate) fn pump(&mut self) -> Result<(), String> {
        if let Some(surface) = self.external_surface.as_mut() {
            surface.pump();
        }
        if !self.failure_reported
            && let Some(failure) = self.backend.as_ref().and_then(MacOsBackend::fatal_failure)
        {
            self.failure_reported = true;
            return Err(format!(
                "{} backend failed: {}",
                failure.subsystem.name(),
                failure.message
            ));
        }
        if self.external_surface.is_some()
            || self.last_ordering_check.elapsed() < ORDERING_POLL_INTERVAL
        {
            return Ok(());
        }
        self.last_ordering_check = Instant::now();
        if let Some(backend) = self.backend.as_mut() {
            backend
                .refresh_overlay_ordering()
                .map_err(|error| format!("macOS overlay ordering refresh failed: {error}"))?;
        }
        Ok(())
    }

    pub(crate) fn take_captured_input(&mut self) -> Vec<CapturedInput> {
        self.external_surface
            .as_mut()
            .map(MacExternalSurface::take_captured_input)
            .unwrap_or_default()
    }

    pub(crate) fn update_cursor(&mut self, bytes: &[u8]) {
        if let Some(surface) = self.external_surface.as_mut() {
            surface.update_cursor(bytes);
        }
    }

    pub(crate) fn control(&mut self, control: OutputControl) -> Result<(), String> {
        let Some(surface) = self.external_surface.as_mut() else {
            return Err("Runtime controls require the external stream window".to_owned());
        };
        match control {
            OutputControl::PointerLock => {
                surface
                    .input_capture
                    .toggle_pointer_lock(&surface.sdl, &mut surface.window);
                Ok(())
            }
        }
    }
}

struct MacExternalSurface {
    // Input-owned SDL cursors must drop before the window and SDL root.
    input_capture: SdlInputCapture,
    event_pump: sdl2::EventPump,
    window: sdl2::video::Window,
    sdl: sdl2::Sdl,
    stream_size: (u32, u32),
    visible: bool,
}

impl MacExternalSurface {
    fn initialize(stream: MediaStreamConfig) -> Result<Self, String> {
        sdl2::hint::set("SDL_MOUSE_RELATIVE_MODE_WARP", "0");
        sdl2::hint::set("SDL_MOUSE_RELATIVE_SCALING", "0");
        let sdl = sdl2::init().map_err(|error| format!("SDL initialization failed: {error}"))?;
        let video = sdl
            .video()
            .map_err(|error| format!("SDL video initialization failed: {error}"))?;
        let mut window_builder = video.window("OpenNOW Stream", 1280, 800);
        window_builder
            .position_centered()
            .resizable()
            .allow_highdpi()
            .hidden()
            .metal_view();
        let window = window_builder
            .build()
            .map_err(|error| format!("external macOS stream window creation failed: {error}"))?;
        // Resolve the AppKit handle now so startup fails clearly instead of waiting for the first
        // keyframe to discover that VideoToolbox has nowhere to present.
        Self::target_for_window(&window)?;
        let event_pump = sdl
            .event_pump()
            .map_err(|error| format!("external macOS event pump creation failed: {error}"))?;
        let capture_input = native_input_capture_enabled();
        eprintln!(
            "External macOS stream window ready (native keyboard/mouse capture: {capture_input})"
        );
        let mut input_capture = SdlInputCapture::new(capture_input, false, false, stream.shortcuts);
        input_capture.enable_gamepads(&sdl);
        Ok(Self {
            input_capture,
            event_pump,
            window,
            sdl,
            stream_size: (stream.width.max(1), stream.height.max(1)),
            visible: false,
        })
    }

    fn target(&self) -> Result<SurfaceTarget, String> {
        Self::target_for_window(&self.window)
    }

    fn target_for_window(window: &sdl2::video::Window) -> Result<SurfaceTarget, String> {
        let raw = window
            .window_handle()
            .map_err(|error| format!("SDL AppKit handle unavailable: {error}"))?
            .as_raw();
        let RawWindowHandle::AppKit(handle) = raw else {
            return Err("SDL did not create an AppKit stream window".to_owned());
        };
        // SAFETY: the SDL window owns this dedicated NSView, remains alive after the backend is
        // constructed, and all calls occur on the streamer's AppKit main thread.
        Ok(SurfaceTarget::NsView(unsafe {
            BorrowedNsView::from_raw(handle.ns_view)
        }))
    }

    fn set_visible(&mut self, visible: bool) {
        if visible {
            opennow_streamer_platform_macos::activate_stream_application();
            self.window.show();
            if !self.visible {
                self.window.raise();
            }
        } else {
            self.release_input();
            self.window.hide();
        }
        self.visible = visible;
    }

    fn update_geometry(&mut self, surface: &RenderSurface) -> Result<(), String> {
        let bounds = surface
            .screen_rect
            .ok_or_else(|| "visible external macOS surface is missing screen bounds".to_owned())?;
        let scale = surface.device_scale_factor.clamp(0.5, 8.0);
        self.window.set_position(
            sdl2::video::WindowPos::Positioned((bounds.x as f32 / scale).round() as i32),
            sdl2::video::WindowPos::Positioned((bounds.y as f32 / scale).round() as i32),
        );
        self.window
            .set_size(
                ((bounds.width as f32 / scale).round() as u32).max(2),
                ((bounds.height as f32 / scale).round() as u32).max(2),
            )
            .map_err(|error| format!("failed to resize external macOS surface: {error}"))
    }

    fn pump(&mut self) {
        let window_id = self.window.id();
        for event in self.event_pump.poll_iter().collect::<Vec<_>>() {
            if matches!(event, sdl2::event::Event::Quit { .. })
                || matches!(
                    event,
                    sdl2::event::Event::Window {
                        window_id: event_window_id,
                        win_event: sdl2::event::WindowEvent::Close,
                        ..
                    } if event_window_id == window_id
                )
            {
                self.release_input();
                self.window.hide();
                self.visible = false;
            } else {
                self.input_capture.handle_event(
                    &self.sdl,
                    &mut self.window,
                    self.stream_size,
                    event,
                );
            }
        }
    }

    fn release_input(&mut self) {
        self.input_capture.release(&self.sdl, &mut self.window);
    }

    fn hide(&mut self) {
        self.release_input();
        self.window.hide();
        self.visible = false;
    }

    fn take_captured_input(&mut self) -> Vec<CapturedInput> {
        self.input_capture.take()
    }

    fn update_cursor(&mut self, bytes: &[u8]) {
        self.input_capture
            .apply_cursor(&self.sdl, &mut self.window, self.stream_size, bytes);
    }
}

fn screen_rect(rect: RenderSurfaceRect) -> ScreenRect {
    ScreenRect::new(
        f64::from(rect.x),
        f64::from(rect.y),
        f64::from(rect.width),
        f64::from(rect.height),
    )
}

#[derive(Default)]
pub(crate) struct H264ParameterSetTracker {
    committed: Option<H264ParameterSets>,
    bootstrap_sequence: Option<Vec<u8>>,
    bootstrap_picture: Option<Vec<u8>>,
    candidate: Option<H264ParameterSets>,
}

impl H264ParameterSetTracker {
    pub(crate) fn observe(&mut self, access_unit: &[u8]) -> Result<H264Framing, String> {
        // Parameter-set replacement is transactional. A damaged access unit must never combine a
        // new SPS with the previously committed PPS (or vice versa), because that creates a pair
        // which VideoToolbox quite correctly rejects. During initial bootstrap we may accumulate
        // SPS/PPS across access units; after a decoder is committed, only a complete pair from the
        // same access unit is eligible to replace it.
        self.candidate = None;
        let (framing, sequence, picture) = if find_start_code(access_unit, 0).is_some() {
            let (sequence, picture) = Self::parameter_sets_from_annex_b(access_unit)?;
            (H264Framing::AnnexB, sequence, picture)
        } else {
            let (sequence, picture) = Self::parameter_sets_from_avcc(access_unit)?;
            (H264Framing::Avcc, sequence, picture)
        };

        if let (Some(sequence), Some(picture)) = (sequence.as_ref(), picture.as_ref()) {
            self.candidate = Some(
                H264ParameterSets::new(sequence, picture)
                    .map_err(|error| format!("invalid H.264 parameter sets: {error}"))?,
            );
        } else if self.committed.is_none() {
            if sequence.is_some() {
                self.bootstrap_sequence = sequence;
            }
            if picture.is_some() {
                self.bootstrap_picture = picture;
            }
            if let (Some(sequence), Some(picture)) =
                (&self.bootstrap_sequence, &self.bootstrap_picture)
            {
                self.candidate = Some(
                    H264ParameterSets::new(sequence, picture)
                        .map_err(|error| format!("invalid H.264 parameter sets: {error}"))?,
                );
            }
        }
        Ok(framing)
    }

    pub(crate) fn parameter_sets(&self) -> Option<H264ParameterSets> {
        self.candidate.clone().or_else(|| self.committed.clone())
    }

    pub(crate) fn commit_parameter_sets(&mut self, parameter_sets: H264ParameterSets) {
        self.committed = Some(parameter_sets);
        self.bootstrap_sequence = None;
        self.bootstrap_picture = None;
        self.candidate = None;
    }

    fn parameter_sets_from_annex_b(access_unit: &[u8]) -> Result<H264ParameterSetBytes, String> {
        let Some((mut start, prefix)) = find_start_code(access_unit, 0) else {
            return Err("H.264 access unit has no Annex B start code".to_owned());
        };
        if access_unit[..start].iter().any(|byte| *byte != 0) {
            return Err("H.264 access unit has invalid Annex B framing".to_owned());
        }
        start += prefix;
        let mut sequence = None;
        let mut picture = None;
        loop {
            let next = find_start_code(access_unit, start);
            let end = next.map_or(access_unit.len(), |(offset, _)| offset);
            Self::observe_nal(&access_unit[start..end], &mut sequence, &mut picture)?;
            let Some((next_start, next_prefix)) = next else {
                return Ok((sequence, picture));
            };
            start = next_start + next_prefix;
        }
    }

    fn parameter_sets_from_avcc(access_unit: &[u8]) -> Result<H264ParameterSetBytes, String> {
        let mut offset = 0usize;
        let mut sequence = None;
        let mut picture = None;
        while offset < access_unit.len() {
            let length_bytes = access_unit
                .get(offset..offset + 4)
                .ok_or_else(|| "H.264 access unit has truncated AVCC framing".to_owned())?;
            let length = u32::from_be_bytes(
                length_bytes
                    .try_into()
                    .expect("AVCC length was checked as four bytes"),
            ) as usize;
            offset += 4;
            let end = offset
                .checked_add(length)
                .filter(|end| *end <= access_unit.len())
                .ok_or_else(|| "H.264 access unit has invalid AVCC framing".to_owned())?;
            Self::observe_nal(&access_unit[offset..end], &mut sequence, &mut picture)?;
            offset = end;
        }
        if offset == 0 {
            return Err("H.264 access unit is empty".to_owned());
        }
        Ok((sequence, picture))
    }

    fn observe_nal(
        nal: &[u8],
        sequence: &mut Option<Vec<u8>>,
        picture: &mut Option<Vec<u8>>,
    ) -> Result<(), String> {
        let header = *nal
            .first()
            .ok_or_else(|| "H.264 access unit contains an empty NAL unit".to_owned())?;
        match header & 0x1f {
            7 => *sequence = Some(nal.to_vec()),
            8 => *picture = Some(nal.to_vec()),
            _ => {}
        }
        Ok(())
    }
}

#[derive(Default)]
pub(crate) struct H265ParameterSetTracker {
    committed: Option<H265ParameterSets>,
    bootstrap_video: Option<Vec<u8>>,
    bootstrap_sequence: Option<Vec<u8>>,
    bootstrap_picture: Option<Vec<u8>>,
    candidate: Option<H265ParameterSets>,
}

impl H265ParameterSetTracker {
    pub(crate) fn observe(&mut self, access_unit: &[u8]) -> Result<H264Framing, String> {
        self.candidate = None;
        let (framing, video, sequence, picture) = if find_start_code(access_unit, 0).is_some() {
            let (video, sequence, picture) = Self::parameter_sets_from_annex_b(access_unit)?;
            (H264Framing::AnnexB, video, sequence, picture)
        } else {
            let (video, sequence, picture) = Self::parameter_sets_from_avcc(access_unit)?;
            (H264Framing::Avcc, video, sequence, picture)
        };

        if let (Some(video), Some(sequence), Some(picture)) =
            (video.as_ref(), sequence.as_ref(), picture.as_ref())
        {
            self.candidate = Some(
                H265ParameterSets::new(video, sequence, picture)
                    .map_err(|error| format!("invalid H.265 parameter sets: {error}"))?,
            );
        } else if self.committed.is_none() {
            if video.is_some() {
                self.bootstrap_video = video;
            }
            if sequence.is_some() {
                self.bootstrap_sequence = sequence;
            }
            if picture.is_some() {
                self.bootstrap_picture = picture;
            }
            if let (Some(video), Some(sequence), Some(picture)) = (
                &self.bootstrap_video,
                &self.bootstrap_sequence,
                &self.bootstrap_picture,
            ) {
                self.candidate = Some(
                    H265ParameterSets::new(video, sequence, picture)
                        .map_err(|error| format!("invalid H.265 parameter sets: {error}"))?,
                );
            }
        }
        Ok(framing)
    }

    pub(crate) fn parameter_sets(&self) -> Option<H265ParameterSets> {
        self.candidate.clone().or_else(|| self.committed.clone())
    }

    pub(crate) fn commit_parameter_sets(&mut self, parameter_sets: H265ParameterSets) {
        self.committed = Some(parameter_sets);
        self.bootstrap_video = None;
        self.bootstrap_sequence = None;
        self.bootstrap_picture = None;
        self.candidate = None;
    }

    fn parameter_sets_from_annex_b(access_unit: &[u8]) -> Result<H265ParameterSetBytes, String> {
        let Some((mut start, prefix)) = find_start_code(access_unit, 0) else {
            return Err("H.265 access unit has no Annex B start code".to_owned());
        };
        if access_unit[..start].iter().any(|byte| *byte != 0) {
            return Err("H.265 access unit has invalid Annex B framing".to_owned());
        }
        start += prefix;
        let mut video = None;
        let mut sequence = None;
        let mut picture = None;
        loop {
            let next = find_start_code(access_unit, start);
            let end = next.map_or(access_unit.len(), |(offset, _)| offset);
            Self::observe_nal(
                &access_unit[start..end],
                &mut video,
                &mut sequence,
                &mut picture,
            )?;
            let Some((next_start, next_prefix)) = next else {
                return Ok((video, sequence, picture));
            };
            start = next_start + next_prefix;
        }
    }

    fn parameter_sets_from_avcc(access_unit: &[u8]) -> Result<H265ParameterSetBytes, String> {
        let mut offset = 0usize;
        let mut video = None;
        let mut sequence = None;
        let mut picture = None;
        while offset < access_unit.len() {
            let length_bytes = access_unit
                .get(offset..offset + 4)
                .ok_or_else(|| "H.265 access unit has truncated HVCC framing".to_owned())?;
            let length = u32::from_be_bytes(
                length_bytes
                    .try_into()
                    .expect("HVCC length was checked as four bytes"),
            ) as usize;
            offset += 4;
            let end = offset
                .checked_add(length)
                .filter(|end| *end <= access_unit.len())
                .ok_or_else(|| "H.265 access unit has invalid HVCC framing".to_owned())?;
            Self::observe_nal(
                &access_unit[offset..end],
                &mut video,
                &mut sequence,
                &mut picture,
            )?;
            offset = end;
        }
        if offset == 0 {
            return Err("H.265 access unit is empty".to_owned());
        }
        Ok((video, sequence, picture))
    }

    fn observe_nal(
        nal: &[u8],
        video: &mut Option<Vec<u8>>,
        sequence: &mut Option<Vec<u8>>,
        picture: &mut Option<Vec<u8>>,
    ) -> Result<(), String> {
        let header = *nal
            .first()
            .ok_or_else(|| "H.265 access unit contains an empty NAL unit".to_owned())?;
        match (header >> 1) & 0x3f {
            32 => *video = Some(nal.to_vec()),
            33 => *sequence = Some(nal.to_vec()),
            34 => *picture = Some(nal.to_vec()),
            _ => {}
        }
        Ok(())
    }
}

fn find_start_code(bytes: &[u8], from: usize) -> Option<(usize, usize)> {
    let mut index = from;
    while index + 3 <= bytes.len() {
        if bytes[index..].starts_with(&[0, 0, 0, 1]) {
            return Some((index, 4));
        }
        if bytes[index..].starts_with(&[0, 0, 1]) {
            return Some((index, 3));
        }
        index += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_parameter_sets_from_annex_b_access_unit() {
        let mut tracker = H264ParameterSetTracker::default();
        let framing = tracker
            .observe(&[
                0, 0, 0, 1, 0x67, 0x64, 0, 0x29, 0, 0, 1, 0x68, 0xee, 0x3c, 0x80, 0, 0, 0, 1, 0x65,
                1,
            ])
            .unwrap();
        assert_eq!(framing, H264Framing::AnnexB);
        let parameters = tracker.parameter_sets().unwrap();
        assert_eq!(parameters.sequence(), &[0x67, 0x64, 0, 0x29]);
        assert_eq!(parameters.picture(), &[0x68, 0xee, 0x3c, 0x80]);
    }

    #[test]
    fn extracts_parameter_sets_across_avcc_access_units() {
        let mut tracker = H264ParameterSetTracker::default();
        assert_eq!(
            tracker.observe(&[0, 0, 0, 2, 0x67, 1]).unwrap(),
            H264Framing::Avcc
        );
        assert!(tracker.parameter_sets().is_none());
        tracker.observe(&[0, 0, 0, 2, 0x68, 2]).unwrap();
        assert!(tracker.parameter_sets().is_some());
    }

    #[test]
    fn committed_parameter_sets_are_not_mixed_with_a_single_set_update() {
        let mut tracker = H264ParameterSetTracker::default();
        tracker
            .observe(&[
                0, 0, 0, 1, 0x67, 0x64, 0, 0x29, 0, 0, 0, 1, 0x68, 0xee, 0x3c, 0x80,
            ])
            .unwrap();
        let committed = tracker.parameter_sets().unwrap();
        tracker.commit_parameter_sets(committed.clone());

        // A lone SPS may be damaged or belong to an incomplete stream transition. It must not be
        // paired with the old PPS and offered to VideoToolbox as a reconfiguration.
        tracker
            .observe(&[0, 0, 0, 1, 0x67, 0x42, 0, 0x1f, 0, 0, 0, 1, 0x65, 1])
            .unwrap();
        assert_eq!(tracker.parameter_sets(), Some(committed));
    }

    #[test]
    fn candidate_parameter_sets_do_not_replace_committed_sets_until_commit() {
        let mut tracker = H264ParameterSetTracker::default();
        tracker
            .observe(&[0, 0, 0, 1, 0x67, 1, 0, 0, 0, 1, 0x68, 2])
            .unwrap();
        let committed = tracker.parameter_sets().unwrap();
        tracker.commit_parameter_sets(committed.clone());

        tracker
            .observe(&[0, 0, 0, 1, 0x67, 3, 0, 0, 0, 1, 0x68, 4])
            .unwrap();
        assert_ne!(tracker.parameter_sets(), Some(committed.clone()));

        // Observing the next regular frame discards the uncommitted candidate and restores the
        // last known-good pair, which models a failed decoder reconfiguration.
        tracker.observe(&[0, 0, 0, 1, 0x65, 5]).unwrap();
        assert_eq!(tracker.parameter_sets(), Some(committed));
    }

    #[test]
    fn extracts_hevc_parameter_sets_from_annex_b_access_unit() {
        let mut tracker = H265ParameterSetTracker::default();
        let framing = tracker
            .observe(&[
                0, 0, 0, 1, 0x40, 0x01, 0x0c, 0, 0, 1, 0x42, 0x01, 0x01, 0, 0, 0, 1, 0x44, 0x01,
                0xc0, 0, 0, 1, 0x26, 0x01,
            ])
            .unwrap();
        assert_eq!(framing, H264Framing::AnnexB);
        let parameters = tracker.parameter_sets().unwrap();
        assert_eq!(parameters.video(), &[0x40, 0x01, 0x0c]);
        assert_eq!(parameters.sequence(), &[0x42, 0x01, 0x01]);
        assert_eq!(parameters.picture(), &[0x44, 0x01, 0xc0]);
    }

    #[test]
    fn extracts_hevc_parameter_sets_across_hvcc_access_units() {
        let mut tracker = H265ParameterSetTracker::default();
        assert_eq!(
            tracker.observe(&[0, 0, 0, 3, 0x40, 1, 1]).unwrap(),
            H264Framing::Avcc
        );
        assert!(tracker.parameter_sets().is_none());
        tracker.observe(&[0, 0, 0, 3, 0x42, 1, 2]).unwrap();
        assert!(tracker.parameter_sets().is_none());
        tracker.observe(&[0, 0, 0, 3, 0x44, 1, 3]).unwrap();
        assert!(tracker.parameter_sets().is_some());
    }

    #[test]
    fn committed_hevc_sets_are_replaced_only_as_a_complete_triplet() {
        let mut tracker = H265ParameterSetTracker::default();
        tracker
            .observe(&[
                0, 0, 0, 1, 0x40, 1, 1, 0, 0, 1, 0x42, 1, 2, 0, 0, 1, 0x44, 1, 3,
            ])
            .unwrap();
        let committed = tracker.parameter_sets().unwrap();
        tracker.commit_parameter_sets(committed.clone());

        tracker
            .observe(&[0, 0, 0, 1, 0x42, 1, 9, 0, 0, 0, 1, 0x26, 1])
            .unwrap();
        assert_eq!(tracker.parameter_sets(), Some(committed.clone()));

        tracker
            .observe(&[
                0, 0, 0, 1, 0x40, 1, 7, 0, 0, 1, 0x42, 1, 8, 0, 0, 1, 0x44, 1, 9,
            ])
            .unwrap();
        assert_ne!(tracker.parameter_sets(), Some(committed.clone()));

        tracker.observe(&[0, 0, 0, 1, 0x26, 1]).unwrap();
        assert_eq!(tracker.parameter_sets(), Some(committed));
    }
}
