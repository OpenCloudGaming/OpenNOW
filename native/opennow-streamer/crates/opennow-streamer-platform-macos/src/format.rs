use std::ffi::c_void;
use std::ptr::NonNull;

use thiserror::Error;

const MAX_PARAMETER_SET_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum H264Framing {
    AnnexB,
    Avcc,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VideoColorSpace {
    Bt601,
    Bt709,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameTiming {
    pub presentation_value: i64,
    pub duration_value: i64,
    pub timescale: i32,
}

impl FrameTiming {
    pub const fn new(presentation_value: i64, duration_value: i64, timescale: i32) -> Self {
        Self {
            presentation_value,
            duration_value,
            timescale,
        }
    }

    pub const fn from_90khz(presentation_value: i64, duration_value: i64) -> Self {
        Self::new(presentation_value, duration_value, 90_000)
    }

    pub(crate) fn validate(self) -> Result<(), FormatError> {
        if self.timescale <= 0 {
            return Err(FormatError::InvalidTimescale);
        }
        if self.duration_value < 0 {
            return Err(FormatError::InvalidDuration);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct H264ParameterSets {
    sequence: Vec<u8>,
    picture: Vec<u8>,
}

impl H264ParameterSets {
    pub fn new(sequence: impl AsRef<[u8]>, picture: impl AsRef<[u8]>) -> Result<Self, FormatError> {
        let sequence = normalize_parameter_set(sequence.as_ref(), 7)?;
        let picture = normalize_parameter_set(picture.as_ref(), 8)?;
        Ok(Self { sequence, picture })
    }

    pub fn sequence(&self) -> &[u8] {
        &self.sequence
    }

    pub fn picture(&self) -> &[u8] {
        &self.picture
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct H264Format {
    pub parameter_sets: H264ParameterSets,
    pub color_space: VideoColorSpace,
}

impl H264Format {
    pub const fn new(parameter_sets: H264ParameterSets, color_space: VideoColorSpace) -> Self {
        Self {
            parameter_sets,
            color_space,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub channels: u8,
}

impl AudioFormat {
    pub const OPUS_STEREO_48KHZ: Self = Self {
        sample_rate: 48_000,
        channels: 2,
    };

    pub const fn new(sample_rate: u32, channels: u8) -> Self {
        Self {
            sample_rate,
            channels,
        }
    }

    pub(crate) fn validate(self) -> Result<(), FormatError> {
        if !matches!(self.sample_rate, 8_000 | 12_000 | 16_000 | 24_000 | 48_000) {
            return Err(FormatError::UnsupportedOpusSampleRate(self.sample_rate));
        }
        if !matches!(self.channels, 1 | 2) {
            return Err(FormatError::UnsupportedChannelCount(self.channels));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueueLimits {
    pub video_frames_in_flight: usize,
    pub decoded_video_frames: usize,
    pub opus_packets: usize,
    pub pcm_milliseconds: u32,
    pub max_video_access_unit_bytes: usize,
}

impl Default for QueueLimits {
    fn default() -> Self {
        Self {
            video_frames_in_flight: 8,
            decoded_video_frames: 3,
            opus_packets: 12,
            pcm_milliseconds: 120,
            max_video_access_unit_bytes: 8 * 1024 * 1024,
        }
    }
}

impl QueueLimits {
    pub(crate) fn validate(self) -> Result<(), FormatError> {
        if self.video_frames_in_flight == 0
            || self.decoded_video_frames == 0
            || self.opus_packets == 0
            || self.pcm_milliseconds == 0
            || self.max_video_access_unit_bytes == 0
        {
            return Err(FormatError::ZeroQueueLimit);
        }
        let pcm_ms =
            usize::try_from(self.pcm_milliseconds).map_err(|_| FormatError::QueueTooLarge)?;
        if pcm_ms > 5_000 {
            return Err(FormatError::QueueTooLarge);
        }
        Ok(())
    }
}

/// Absolute screen bounds in Electron's top-left, device-independent coordinate space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl ScreenRect {
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub(crate) fn validate(self) -> Result<(), FormatError> {
        if !self.x.is_finite()
            || !self.y.is_finite()
            || !self.width.is_finite()
            || !self.height.is_finite()
            || self.width <= 0.0
            || self.height <= 0.0
        {
            return Err(FormatError::InvalidScreenRect);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OwnedOverlayConfig {
    pub screen_rect: ScreenRect,
    pub visible: bool,
}

impl OwnedOverlayConfig {
    pub const fn new(screen_rect: ScreenRect, visible: bool) -> Self {
        Self {
            screen_rect,
            visible,
        }
    }

    pub(crate) fn validate(self) -> Result<(), FormatError> {
        self.screen_rect.validate()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BorrowedNsView(NonNull<c_void>);

impl BorrowedNsView {
    /// Creates a borrowed AppKit view handle.
    ///
    /// # Safety
    ///
    /// `view` must point to a live, dedicated `NSView` whose backing layer may be replaced for the
    /// backend's lifetime. The object must belong to the current process, and creating the handle
    /// and passing it to `MacOsBackend::start` must occur on AppKit's main thread. The backend
    /// retains the view before this call's borrowed lifetime can end.
    pub const unsafe fn from_raw(view: NonNull<c_void>) -> Self {
        Self(view)
    }

    pub const fn as_ptr(self) -> NonNull<c_void> {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BorrowedNsWindow(NonNull<c_void>);

impl BorrowedNsWindow {
    /// Creates a borrowed AppKit window handle.
    ///
    /// # Safety
    ///
    /// `window` must point to a live `NSWindow`. Construction and backend startup must occur on
    /// AppKit's main thread. The backend does not replace the window content view or its layer; it
    /// inserts a passive child view for stream presentation.
    pub const unsafe fn from_raw(window: NonNull<c_void>) -> Self {
        Self(window)
    }

    pub const fn as_ptr(self) -> NonNull<c_void> {
        self.0
    }
}

/// A rectangle in renderer-relative, top-left AppKit points.
///
/// Points match Electron's device-independent coordinates. Negative origins are allowed for
/// clipping, while width and height must be finite and non-negative.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RendererRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl RendererRect {
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub(crate) fn validate(self) -> Result<(), FormatError> {
        if !self.x.is_finite()
            || !self.y.is_finite()
            || !self.width.is_finite()
            || !self.height.is_finite()
            || self.width < 0.0
            || self.height < 0.0
        {
            return Err(FormatError::InvalidRendererRect);
        }
        Ok(())
    }

    pub(crate) fn to_parent_coordinates(
        self,
        parent: RendererRect,
        parent_is_flipped: bool,
    ) -> Self {
        let y = if parent_is_flipped {
            parent.y + self.y
        } else {
            parent.y + parent.height - self.y - self.height
        };
        Self::new(parent.x + self.x, y, self.width, self.height)
    }
}

/// Initial layout for a passive video child inside a supplied `NSWindow` content view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowSurfaceConfig {
    pub window: BorrowedNsWindow,
    pub bounds: RendererRect,
    pub visible: bool,
}

impl WindowSurfaceConfig {
    pub const fn new(window: BorrowedNsWindow, bounds: RendererRect) -> Self {
        Self {
            window,
            bounds,
            visible: true,
        }
    }

    pub(crate) fn validate(self) -> Result<(), FormatError> {
        self.bounds.validate()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SurfaceTarget {
    /// Creates a borderless, non-activating, mouse-ignoring overlay owned by this process.
    OwnedOverlay(OwnedOverlayConfig),
    /// Uses a caller-owned view that is explicitly dedicated to video presentation. Its backing
    /// layer is replaced until backend shutdown and restored afterwards.
    NsView(BorrowedNsView),
    /// Adds an owned, passive child view to the supplied window's content view. The existing
    /// content view and its backing layer are never replaced.
    NsWindow(WindowSurfaceConfig),
}

#[derive(Clone, Debug, PartialEq)]
pub struct BackendConfig {
    pub surface: SurfaceTarget,
    pub video: H264Format,
    pub audio: AudioFormat,
    pub queues: QueueLimits,
}

impl BackendConfig {
    pub(crate) fn validate(&self) -> Result<(), FormatError> {
        self.audio.validate()?;
        self.queues.validate()?;
        if let SurfaceTarget::OwnedOverlay(overlay) = self.surface {
            overlay.validate()?;
        }
        if let SurfaceTarget::NsWindow(window) = &self.surface {
            window.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum FormatError {
    #[error("H.264 parameter set is empty")]
    EmptyParameterSet,
    #[error("H.264 parameter set exceeds the supported size")]
    ParameterSetTooLarge,
    #[error("expected H.264 NAL type {expected}, got {actual}")]
    UnexpectedNalType { expected: u8, actual: u8 },
    #[error("multiple NAL units were supplied where one parameter set was expected")]
    MultipleParameterSets,
    #[error("H.264 access unit has no NAL units")]
    EmptyAccessUnit,
    #[error("H.264 access unit contains an empty NAL unit")]
    EmptyNalUnit,
    #[error("H.264 access unit has invalid AVCC framing")]
    InvalidAvcc,
    #[error("H.264 NAL unit is too large for AVCC framing")]
    NalUnitTooLarge,
    #[error("frame timescale must be positive")]
    InvalidTimescale,
    #[error("frame duration must not be negative")]
    InvalidDuration,
    #[error("unsupported Opus sample rate {0}")]
    UnsupportedOpusSampleRate(u32),
    #[error("unsupported Opus channel count {0}")]
    UnsupportedChannelCount(u8),
    #[error("queue limits must be non-zero")]
    ZeroQueueLimit,
    #[error("queue configuration is too large")]
    QueueTooLarge,
    #[error("absolute screen bounds must be finite with positive dimensions")]
    InvalidScreenRect,
    #[error("renderer-relative surface bounds must be finite with non-negative dimensions")]
    InvalidRendererRect,
}

pub(crate) fn access_unit_to_avcc(
    bytes: &[u8],
    framing: H264Framing,
) -> Result<Vec<u8>, FormatError> {
    match framing {
        H264Framing::AnnexB => annex_b_to_avcc(bytes),
        H264Framing::Avcc => {
            validate_avcc(bytes)?;
            Ok(bytes.to_vec())
        }
    }
}

fn normalize_parameter_set(bytes: &[u8], expected_type: u8) -> Result<Vec<u8>, FormatError> {
    let bytes = strip_start_code(bytes);
    if bytes.is_empty() {
        return Err(FormatError::EmptyParameterSet);
    }
    if bytes.len() > MAX_PARAMETER_SET_BYTES {
        return Err(FormatError::ParameterSetTooLarge);
    }
    if find_start_code(bytes, 1).is_some() {
        return Err(FormatError::MultipleParameterSets);
    }
    let actual = bytes[0] & 0x1f;
    if actual != expected_type {
        return Err(FormatError::UnexpectedNalType {
            expected: expected_type,
            actual,
        });
    }
    Ok(bytes.to_vec())
}

fn strip_start_code(bytes: &[u8]) -> &[u8] {
    if bytes.starts_with(&[0, 0, 0, 1]) {
        &bytes[4..]
    } else if bytes.starts_with(&[0, 0, 1]) {
        &bytes[3..]
    } else {
        bytes
    }
}

fn annex_b_to_avcc(bytes: &[u8]) -> Result<Vec<u8>, FormatError> {
    let Some((mut start, prefix_len)) = find_start_code(bytes, 0) else {
        return Err(FormatError::EmptyAccessUnit);
    };
    if bytes[..start].iter().any(|byte| *byte != 0) {
        return Err(FormatError::EmptyAccessUnit);
    }
    start += prefix_len;
    let mut output = Vec::with_capacity(bytes.len());
    loop {
        let next = find_start_code(bytes, start);
        let end = next.map_or(bytes.len(), |(offset, _)| offset);
        if end == start {
            return Err(FormatError::EmptyNalUnit);
        }
        write_avcc_nal(&mut output, &bytes[start..end])?;
        let Some((next_start, next_prefix)) = next else {
            break;
        };
        start = next_start + next_prefix;
    }
    if output.is_empty() {
        return Err(FormatError::EmptyAccessUnit);
    }
    Ok(output)
}

fn find_start_code(bytes: &[u8], from: usize) -> Option<(usize, usize)> {
    let mut index = from;
    while index + 3 <= bytes.len() {
        if bytes[index..].starts_with(&[0, 0, 1]) {
            return Some((index, 3));
        }
        if bytes[index..].starts_with(&[0, 0, 0, 1]) {
            return Some((index, 4));
        }
        index += 1;
    }
    None
}

fn write_avcc_nal(output: &mut Vec<u8>, nal: &[u8]) -> Result<(), FormatError> {
    if nal.is_empty() {
        return Err(FormatError::EmptyNalUnit);
    }
    let len = u32::try_from(nal.len()).map_err(|_| FormatError::NalUnitTooLarge)?;
    output.extend_from_slice(&len.to_be_bytes());
    output.extend_from_slice(nal);
    Ok(())
}

fn validate_avcc(bytes: &[u8]) -> Result<(), FormatError> {
    if bytes.is_empty() {
        return Err(FormatError::EmptyAccessUnit);
    }
    let mut offset = 0usize;
    while offset < bytes.len() {
        let header = bytes
            .get(offset..offset + 4)
            .ok_or(FormatError::InvalidAvcc)?;
        let len = u32::from_be_bytes(header.try_into().expect("four-byte AVCC length")) as usize;
        if len == 0 {
            return Err(FormatError::EmptyNalUnit);
        }
        offset = offset.checked_add(4).ok_or(FormatError::InvalidAvcc)?;
        offset = offset.checked_add(len).ok_or(FormatError::InvalidAvcc)?;
        if offset > bytes.len() {
            return Err(FormatError::InvalidAvcc);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_mixed_annex_b_start_codes_to_avcc() {
        let annex_b = [0, 0, 0, 1, 0x65, 0xaa, 0xbb, 0, 0, 1, 0x41, 0xcc];
        assert_eq!(
            annex_b_to_avcc(&annex_b).unwrap(),
            [0, 0, 0, 3, 0x65, 0xaa, 0xbb, 0, 0, 0, 2, 0x41, 0xcc]
        );
    }

    #[test]
    fn rejects_truncated_avcc_access_unit() {
        assert_eq!(
            access_unit_to_avcc(&[0, 0, 0, 3, 0x65], H264Framing::Avcc),
            Err(FormatError::InvalidAvcc)
        );
    }

    #[test]
    fn normalizes_parameter_set_start_codes() {
        let sets = H264ParameterSets::new(
            [0, 0, 0, 1, 0x67, 0x64, 0x00, 0x29],
            [0, 0, 1, 0x68, 0xee, 0x3c, 0x80],
        )
        .unwrap();
        assert_eq!(sets.sequence(), &[0x67, 0x64, 0x00, 0x29]);
        assert_eq!(sets.picture(), &[0x68, 0xee, 0x3c, 0x80]);
    }

    #[test]
    fn rejects_wrong_parameter_set_type() {
        assert_eq!(
            H264ParameterSets::new([0x68, 1], [0x68, 2]),
            Err(FormatError::UnexpectedNalType {
                expected: 7,
                actual: 8
            })
        );
    }

    #[test]
    fn validates_audio_and_queue_formats() {
        assert!(AudioFormat::OPUS_STEREO_48KHZ.validate().is_ok());
        assert_eq!(
            AudioFormat::new(44_100, 2).validate(),
            Err(FormatError::UnsupportedOpusSampleRate(44_100))
        );
        let limits = QueueLimits {
            opus_packets: 0,
            ..QueueLimits::default()
        };
        assert_eq!(limits.validate(), Err(FormatError::ZeroQueueLimit));
    }

    #[test]
    fn converts_renderer_top_left_bounds_for_unflipped_appkit_parent() {
        let parent = RendererRect::new(0.0, 0.0, 1280.0, 720.0);
        let renderer = RendererRect::new(20.0, 30.0, 640.0, 360.0);
        assert_eq!(
            renderer.to_parent_coordinates(parent, false),
            RendererRect::new(20.0, 330.0, 640.0, 360.0)
        );
    }

    #[test]
    fn preserves_renderer_y_for_flipped_parent_and_accounts_for_bounds_origin() {
        let parent = RendererRect::new(5.0, 10.0, 1280.0, 720.0);
        let renderer = RendererRect::new(20.0, 30.0, 640.0, 360.0);
        assert_eq!(
            renderer.to_parent_coordinates(parent, true),
            RendererRect::new(25.0, 40.0, 640.0, 360.0)
        );
    }

    #[test]
    fn rejects_invalid_renderer_geometry() {
        assert_eq!(
            RendererRect::new(0.0, 0.0, -1.0, 10.0).validate(),
            Err(FormatError::InvalidRendererRect)
        );
        assert_eq!(
            RendererRect::new(f64::NAN, 0.0, 10.0, 10.0).validate(),
            Err(FormatError::InvalidRendererRect)
        );
    }

    #[test]
    fn recomputes_unflipped_child_position_after_parent_resize() {
        let renderer = RendererRect::new(20.0, 30.0, 640.0, 360.0);
        let initial =
            renderer.to_parent_coordinates(RendererRect::new(0.0, 0.0, 1280.0, 720.0), false);
        let resized =
            renderer.to_parent_coordinates(RendererRect::new(0.0, 0.0, 1280.0, 920.0), false);
        assert_eq!(initial.y, 330.0);
        assert_eq!(resized.y, 530.0);
    }
}
