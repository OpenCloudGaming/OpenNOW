use opennow_streamer_platform_macos::{
    AudioFormat, BackendConfig, BackendStats, BorrowedNsView, GpuOverlayFrame, GpuOverlayPlacement,
    H264Format, H264Framing, H264ParameterSets, MacOsBackend, OwnedOverlayConfig, QueueLimits,
    ScreenRect, StreamSink, SurfaceTarget, VideoColorSpace, probe_h264_hardware,
};
use opennow_streamer_protocol::{RenderSurface, RenderSurfaceRect};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::media::{CapturedInput, MediaStreamConfig};
use crate::native_stats_overlay::{NativeStatsOverlay, OverlayMode};
use crate::output::{
    SdlInputCapture, external_renderer_enabled, handle_native_window_shortcut,
    native_input_capture_enabled,
};

const HIDDEN_SURFACE: ScreenRect = ScreenRect::new(0.0, 0.0, 2.0, 2.0);
const ORDERING_POLL_INTERVAL: Duration = Duration::from_millis(100);

pub(crate) fn available() -> bool {
    availability().load(Ordering::Acquire)
}

pub(crate) fn disable() {
    availability().store(false, Ordering::Release);
}

fn availability() -> &'static AtomicBool {
    static AVAILABLE: OnceLock<AtomicBool> = OnceLock::new();
    AVAILABLE.get_or_init(|| AtomicBool::new(probe_h264_hardware()))
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
            video: H264Format::new(parameter_sets, VideoColorSpace::Bt709),
            audio: AudioFormat::OPUS_STEREO_48KHZ,
            queues: QueueLimits::default(),
        })
        .map_err(|error| format!("VideoToolbox backend initialization failed: {error}"))?;
        if let Some(surface) = self.external_surface.as_ref() {
            backend.set_gpu_overlay(surface.gpu_overlay());
        }
        backend
            .set_paused(self.paused)
            .map_err(|error| format!("VideoToolbox pause state failed: {error}"))?;
        let sink = backend.sink();
        self.backend = Some(backend);
        Ok(sink)
    }

    pub(crate) fn set_paused(&mut self, paused: bool) -> Result<(), String> {
        self.paused = paused;
        if paused && let Some(surface) = self.external_surface.as_mut() {
            surface.release_input();
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
        let backend_stats = self.backend.as_ref().map(MacOsBackend::stats);
        let overlay_changed = self
            .external_surface
            .as_mut()
            .is_some_and(|surface| surface.pump(backend_stats));
        if overlay_changed {
            let overlay = self
                .external_surface
                .as_ref()
                .and_then(MacExternalSurface::gpu_overlay);
            if let Some(backend) = self.backend.as_mut() {
                backend.set_gpu_overlay(overlay);
            }
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
}

struct MacExternalSurface {
    // Input-owned SDL cursors must drop before the window and SDL root.
    input_capture: SdlInputCapture,
    event_pump: sdl2::EventPump,
    window: sdl2::video::Window,
    sdl: sdl2::Sdl,
    stream_size: (u32, u32),
    debug_overlay: NativeStatsOverlay,
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
        let window = video
            .window("OpenNOW Stream", 1280, 800)
            .position_centered()
            .resizable()
            .allow_highdpi()
            .hidden()
            .metal_view()
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
        Ok(Self {
            input_capture: SdlInputCapture::new(capture_input, false),
            event_pump,
            window,
            sdl,
            stream_size: (stream.width.max(1), stream.height.max(1)),
            debug_overlay: NativeStatsOverlay::new(stream, "MACOS / VIDEOTOOLBOX / METAL"),
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

    fn pump(&mut self, stats: Option<BackendStats>) -> bool {
        let window_id = self.window.id();
        let mut overlay_changed = false;
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
            } else if handle_macos_stats_shortcut(&mut self.debug_overlay, &event) {
                overlay_changed = true;
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
        if let Some(stats) = stats {
            let dropped = stats
                .video_backpressured
                .saturating_add(stats.video_frames_dropped)
                .saturating_add(stats.video_present_errors);
            overlay_changed |= self.debug_overlay.update(
                stats.video_presented,
                dropped,
                self.input_capture.relative_mouse_enabled(),
                stats.video_submitted_bytes,
            );
        }
        overlay_changed
    }

    fn gpu_overlay(&self) -> Option<GpuOverlayFrame> {
        let frame = self.debug_overlay.frame()?;
        let placement = match self.debug_overlay.mode() {
            OverlayMode::Minimal => GpuOverlayPlacement::TopRight,
            OverlayMode::Full => GpuOverlayPlacement::TopLeft,
            OverlayMode::Hidden => return None,
        };
        let mut rgba = Vec::with_capacity(frame.rgb.len() / 3 * 4);
        for rgb in frame.rgb.chunks_exact(3) {
            rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 245]);
        }
        Some(GpuOverlayFrame {
            width: frame.width,
            height: frame.height,
            rgba: rgba.into(),
            placement,
        })
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

fn handle_macos_stats_shortcut(
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

    fn parameter_sets_from_annex_b(
        access_unit: &[u8],
    ) -> Result<(Option<Vec<u8>>, Option<Vec<u8>>), String> {
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

    fn parameter_sets_from_avcc(
        access_unit: &[u8],
    ) -> Result<(Option<Vec<u8>>, Option<Vec<u8>>), String> {
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
}
