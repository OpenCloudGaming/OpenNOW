use crate::ValidationError;
use crate::provider::{List, OfferId, SecretBytes};
use serde::{Deserialize, Serialize};

pub const MEDIA_PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VideoEncoding {
    H264AnnexB,
    HevcAnnexB,
    Av1Obu,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Chroma {
    Yuv420,
    Yuv444,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DynamicRange {
    Sdr,
    Hdr10,
    Hlg,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Primaries {
    Bt709,
    Bt2020,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Transfer {
    Bt709,
    Srgb,
    Pq,
    Hlg,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Matrix {
    #[serde(rename = "bt601")]
    Bt601,
    #[serde(rename = "bt709")]
    Bt709,
    #[serde(rename = "bt2020-ncl")]
    Bt2020NonConstant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorRange {
    Limited,
    Full,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChromaLocation {
    Left,
    Center,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ColorDescription {
    pub range: ColorRange,
    pub primaries: Primaries,
    pub transfer: Transfer,
    pub matrix: Matrix,
    pub chroma_location: ChromaLocation,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VideoSupport {
    pub encoding: VideoEncoding,
    pub bit_depth: u8,
    pub chroma: Chroma,
    pub dynamic_range: DynamicRange,
    pub max_width: u32,
    pub max_height: u32,
    pub max_fps: u32,
}

impl VideoSupport {
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_dimensions(self.max_width, self.max_height, self.max_fps)?;
        validate_encoding(self.encoding, self.bit_depth)?;
        if self.dynamic_range != DynamicRange::Sdr && self.bit_depth != 10 {
            return Err(ValidationError("HDR media support requires ten-bit video"));
        }
        Ok(())
    }

    pub fn supports(&self, format: &VideoFormat) -> bool {
        self.encoding == format.encoding
            && self.bit_depth == format.bit_depth
            && self.chroma == format.chroma
            && self.dynamic_range == format.dynamic_range()
            && format.width <= self.max_width
            && format.height <= self.max_height
            && format.fps <= self.max_fps
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VideoFormat {
    pub encoding: VideoEncoding,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bit_depth: u8,
    pub chroma: Chroma,
    pub color: ColorDescription,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestedVideo {
    pub width: u32,
    pub height: u32,
    pub encoding: Option<VideoEncoding>,
    pub fps: Option<u32>,
    pub bit_depth: u8,
    pub chroma: Chroma,
    pub hdr: bool,
}

impl RequestedVideo {
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_dimensions(self.width, self.height, self.fps.unwrap_or(1))?;
        if !matches!(self.bit_depth, 8 | 10) || (self.hdr && self.bit_depth != 10) {
            return Err(ValidationError(
                "Requested video bit depth or HDR preference is invalid",
            ));
        }
        if let Some(encoding) = self.encoding {
            validate_encoding(encoding, self.bit_depth)?;
        }
        Ok(())
    }
}

impl VideoFormat {
    pub fn dynamic_range(&self) -> DynamicRange {
        match self.color.transfer {
            Transfer::Bt709 | Transfer::Srgb => DynamicRange::Sdr,
            Transfer::Pq => DynamicRange::Hdr10,
            Transfer::Hlg => DynamicRange::Hlg,
        }
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_dimensions(self.width, self.height, self.fps)?;
        validate_encoding(self.encoding, self.bit_depth)?;
        if self.dynamic_range() != DynamicRange::Sdr
            && (self.bit_depth != 10
                || self.color.primaries != Primaries::Bt2020
                || self.color.matrix != Matrix::Bt2020NonConstant)
        {
            return Err(ValidationError(
                "HDR transfer requires ten-bit BT.2020 video",
            ));
        }
        Ok(())
    }
}

fn validate_dimensions(width: u32, height: u32, fps: u32) -> Result<(), ValidationError> {
    if width == 0
        || height == 0
        || width > 8192
        || height > 8192
        || u64::from(width) * u64::from(height) > 35_389_440
        || !(1..=360).contains(&fps)
    {
        return Err(ValidationError(
            "Video dimensions or frame rate exceed format bounds",
        ));
    }
    Ok(())
}

fn validate_encoding(encoding: VideoEncoding, bit_depth: u8) -> Result<(), ValidationError> {
    if !matches!(bit_depth, 8 | 10) || (encoding == VideoEncoding::H264AnnexB && bit_depth != 8) {
        return Err(ValidationError(
            "Video encoding and bit depth are incompatible",
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioCodec {
    Opus,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AudioFormat {
    pub codec: AudioCodec,
    pub sample_rate: u32,
    pub channels: u8,
}

impl AudioFormat {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.sample_rate != 48_000 || !matches!(self.channels, 1 | 2) {
            return Err(ValidationError("Opus requires 48 kHz mono or stereo"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InputCapabilities {
    pub keyboard: bool,
    pub relative_mouse: bool,
    pub absolute_mouse: bool,
    pub text: bool,
    pub gamepad_slots: u8,
    pub rumble: bool,
}

impl InputCapabilities {
    pub fn is_subset_of(&self, offered: &Self) -> bool {
        (!self.keyboard || offered.keyboard)
            && (!self.relative_mouse || offered.relative_mouse)
            && (!self.absolute_mouse || offered.absolute_mouse)
            && (!self.text || offered.text)
            && (!self.rumble || offered.rumble)
            && self.gamepad_slots <= offered.gamepad_slots
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MediaLimits {
    pub max_video_access_unit_bytes: u32,
    pub max_audio_packet_bytes: u32,
    pub max_buffered_video_bytes: u32,
    pub max_buffered_video_frames: u16,
    pub max_buffered_audio_ms: u16,
    pub max_control_message_bytes: u32,
    pub max_pending_input_events: u16,
}

impl MediaLimits {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.max_video_access_unit_bytes == 0
            || self.max_video_access_unit_bytes > 16 * 1024 * 1024
            || self.max_audio_packet_bytes == 0
            || self.max_audio_packet_bytes > 64 * 1024
            || self.max_control_message_bytes == 0
            || self.max_control_message_bytes > 64 * 1024
            || self.max_buffered_video_bytes < self.max_video_access_unit_bytes
            || self.max_buffered_video_bytes > 32 * 1024 * 1024
            || !(1..=8).contains(&self.max_buffered_video_frames)
            || !(1..=100).contains(&self.max_buffered_audio_ms)
            || !(1..=1024).contains(&self.max_pending_input_events)
        {
            return Err(ValidationError("Media channel limits are invalid"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeOffer {
    pub version: u32,
    pub offer_id: OfferId,
    pub runtime_epoch: u64,
    pub expires_at_ms: u64,
    pub video_formats: List<VideoSupport, 64>,
    pub audio_formats: List<AudioFormat, 8>,
    pub input: InputCapabilities,
    pub limits: MediaLimits,
}

impl NativeOffer {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.version != MEDIA_PROTOCOL_VERSION
            || self.runtime_epoch == 0
            || self.expires_at_ms == 0
            || self.video_formats.is_empty()
            || self.input.gamepad_slots > 4
        {
            return Err(ValidationError("Native media offer is invalid"));
        }
        self.limits.validate()?;
        for video in self.video_formats.iter() {
            video.validate()?;
            if video.dynamic_range != DynamicRange::Sdr {
                return Err(ValidationError(
                    "External media protocol 1 currently offers SDR only",
                ));
            }
        }
        for audio in self.audio_formats.iter() {
            audio.validate()?;
        }
        Ok(())
    }

    pub fn supports_video(&self, video: &VideoFormat) -> bool {
        self.video_formats
            .iter()
            .any(|supported| supported.supports(video))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AcceptedMedia {
    pub offer_id: OfferId,
    pub runtime_epoch: u64,
    pub video: VideoFormat,
    pub audio: Option<AudioFormat>,
    pub input: InputCapabilities,
}

impl AcceptedMedia {
    pub fn validate_against(
        &self,
        offer: &NativeOffer,
        now_ms: u64,
    ) -> Result<(), ValidationError> {
        offer.validate()?;
        self.video.validate()?;
        if self.video.dynamic_range() != DynamicRange::Sdr {
            return Err(ValidationError(
                "External media protocol 1 currently accepts SDR only",
            ));
        }
        if let Some(audio) = &self.audio {
            audio.validate()?;
        }
        if self.offer_id != offer.offer_id
            || self.runtime_epoch != offer.runtime_epoch
            || now_ms >= offer.expires_at_ms
            || !offer.supports_video(&self.video)
            || self
                .audio
                .as_ref()
                .is_some_and(|audio| !offer.audio_formats.contains(audio))
            || !self.input.is_subset_of(&offer.input)
        {
            return Err(ValidationError(
                "Prepared media does not match the active native offer",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreparedWorker {
    pub accepted: AcceptedMedia,
    pub bootstrap: SecretBytes,
}
