use opennow_streamer_platform_macos::{
    AudioFormat, BackendConfig, H264Format, H264Framing, H264ParameterSets, MacOsBackend,
    OwnedOverlayConfig, QueueLimits, ScreenRect, StreamSink, SurfaceTarget, VideoColorSpace,
    probe_h264_hardware,
};
use opennow_streamer_protocol::{RenderSurface, RenderSurfaceRect};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

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
    screen_rect: ScreenRect,
    visible: bool,
    paused: bool,
    last_ordering_check: Instant,
}

impl MacOutput {
    pub(crate) fn initialize() -> Self {
        Self {
            backend: None,
            screen_rect: HIDDEN_SURFACE,
            visible: false,
            paused: false,
            last_ordering_check: Instant::now(),
        }
    }

    pub(crate) fn start(&mut self, surface: Option<&RenderSurface>) -> Result<(), String> {
        self.paused = false;
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
        let mut backend = MacOsBackend::start(BackendConfig {
            surface: SurfaceTarget::OwnedOverlay(OwnedOverlayConfig::new(
                self.screen_rect,
                self.visible && !self.paused,
            )),
            video: H264Format::new(parameter_sets, VideoColorSpace::Bt709),
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

    pub(crate) fn set_paused(&mut self, paused: bool) -> Result<(), String> {
        self.paused = paused;
        if let Some(backend) = self.backend.as_mut() {
            backend
                .set_paused(paused)
                .map_err(|error| format!("macOS media pause failed: {error}"))?;
            backend
                .update_owned_overlay(self.screen_rect, self.visible && !paused)
                .map_err(|error| format!("macOS overlay pause failed: {error}"))?;
        }
        Ok(())
    }

    pub(crate) fn stop(&mut self) {
        if let Some(mut backend) = self.backend.take() {
            backend.stop();
        }
        self.paused = false;
    }

    pub(crate) fn update_surface(&mut self, surface: &RenderSurface) -> Result<(), String> {
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
        if self.last_ordering_check.elapsed() < ORDERING_POLL_INTERVAL {
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
    sequence: Option<Vec<u8>>,
    picture: Option<Vec<u8>>,
}

impl H264ParameterSetTracker {
    pub(crate) fn observe(&mut self, access_unit: &[u8]) -> Result<H264Framing, String> {
        let framing = if find_start_code(access_unit, 0).is_some() {
            self.observe_annex_b(access_unit)?;
            H264Framing::AnnexB
        } else {
            self.observe_avcc(access_unit)?;
            H264Framing::Avcc
        };
        Ok(framing)
    }

    pub(crate) fn parameter_sets(&self) -> Result<Option<H264ParameterSets>, String> {
        match (&self.sequence, &self.picture) {
            (Some(sequence), Some(picture)) => H264ParameterSets::new(sequence, picture)
                .map(Some)
                .map_err(|error| format!("invalid H.264 parameter sets: {error}")),
            _ => Ok(None),
        }
    }

    fn observe_annex_b(&mut self, access_unit: &[u8]) -> Result<(), String> {
        let Some((mut start, prefix)) = find_start_code(access_unit, 0) else {
            return Err("H.264 access unit has no Annex B start code".to_owned());
        };
        if access_unit[..start].iter().any(|byte| *byte != 0) {
            return Err("H.264 access unit has invalid Annex B framing".to_owned());
        }
        start += prefix;
        loop {
            let next = find_start_code(access_unit, start);
            let end = next.map_or(access_unit.len(), |(offset, _)| offset);
            self.observe_nal(&access_unit[start..end])?;
            let Some((next_start, next_prefix)) = next else {
                return Ok(());
            };
            start = next_start + next_prefix;
        }
    }

    fn observe_avcc(&mut self, access_unit: &[u8]) -> Result<(), String> {
        let mut offset = 0usize;
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
            self.observe_nal(&access_unit[offset..end])?;
            offset = end;
        }
        if offset == 0 {
            return Err("H.264 access unit is empty".to_owned());
        }
        Ok(())
    }

    fn observe_nal(&mut self, nal: &[u8]) -> Result<(), String> {
        let header = *nal
            .first()
            .ok_or_else(|| "H.264 access unit contains an empty NAL unit".to_owned())?;
        match header & 0x1f {
            7 => self.sequence = Some(nal.to_vec()),
            8 => self.picture = Some(nal.to_vec()),
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
        let parameters = tracker.parameter_sets().unwrap().unwrap();
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
        assert!(tracker.parameter_sets().unwrap().is_none());
        tracker.observe(&[0, 0, 0, 2, 0x68, 2]).unwrap();
        assert!(tracker.parameter_sets().unwrap().is_some());
    }
}
