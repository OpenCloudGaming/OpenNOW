use std::num::{NonZeroIsize, NonZeroU32};

use crate::BackendError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoCodec {
    H264,
    H265,
    Av1,
}

impl VideoCodec {
    pub const fn label(self) -> &'static str {
        match self {
            Self::H264 => "H.264",
            Self::H265 => "H.265",
            Self::Av1 => "AV1",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoFormat {
    pub codec: VideoCodec,
    pub width: u32,
    pub height: u32,
    pub frame_rate_numerator: NonZeroU32,
    pub frame_rate_denominator: NonZeroU32,
    pub average_bitrate: u32,
}

impl VideoFormat {
    pub fn validate(self) -> Result<(), BackendError> {
        if !(48..=4096).contains(&self.width) || !(48..=2304).contains(&self.height) {
            return Err(BackendError::InvalidConfig(format!(
                "{} dimensions must be at least 48x48 and no larger than 4096x2304",
                self.codec.label()
            )));
        }
        let fps = self.frame_rate_numerator.get() as f64 / self.frame_rate_denominator.get() as f64;
        if !(1.0..=240.0).contains(&fps) {
            return Err(BackendError::InvalidConfig(
                "video frame rate must be between 1 and 240 fps".to_owned(),
            ));
        }
        if self.average_bitrate == 0 {
            return Err(BackendError::InvalidConfig(
                "average video bitrate must be non-zero".to_owned(),
            ));
        }
        Ok(())
    }

    pub fn frame_duration_100ns(self) -> i64 {
        (10_000_000_u64 * self.frame_rate_denominator.get() as u64
            / self.frame_rate_numerator.get() as u64) as i64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

impl AudioFormat {
    pub fn validate(self) -> Result<(), BackendError> {
        if !(8_000..=192_000).contains(&self.sample_rate) {
            return Err(BackendError::InvalidConfig(
                "PCM sample rate must be between 8 kHz and 192 kHz".to_owned(),
            ));
        }
        if !(1..=8).contains(&self.channels) {
            return Err(BackendError::InvalidConfig(
                "PCM channel count must be between 1 and 8".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct WindowHandle(NonZeroIsize);

impl WindowHandle {
    pub fn new(raw: isize) -> Option<Self> {
        NonZeroIsize::new(raw).map(Self)
    }

    pub fn get(self) -> isize {
        self.0.get()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Bounds {
    pub fn validate(self) -> Result<(), BackendError> {
        if self.width == 0 || self.height == 0 {
            return Err(BackendError::InvalidConfig(
                "surface bounds must be non-empty".to_owned(),
            ));
        }
        if self.width > i32::MAX as u32 || self.height > i32::MAX as u32 {
            return Err(BackendError::InvalidConfig(
                "surface bounds exceed Win32 coordinate limits".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExistingWindow {
    pub hwnd: WindowHandle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnedWindow {
    pub parent: Option<WindowHandle>,
    pub bounds: Bounds,
    pub visible: bool,
}

impl OwnedWindow {
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) fn has_same_parent(self, other: Self) -> bool {
        self.parent == other.parent
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceTarget {
    /// A dedicated renderer HWND whose ownership, activation, and input policy belongs to the caller.
    Existing(ExistingWindow),
    /// An input-transparent, non-activating HWND owned by this backend.
    Owned(OwnedWindow),
}

impl SurfaceTarget {
    pub fn validate(self) -> Result<(), BackendError> {
        if let Self::Owned(window) = self {
            window.bounds.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedVideoFrame {
    pub codec: VideoCodec,
    pub data: Vec<u8>,
    pub timestamp_100ns: i64,
    pub duration_100ns: i64,
    pub key_frame: bool,
}

impl EncodedVideoFrame {
    pub fn validate(&self) -> Result<(), BackendError> {
        if self.data.is_empty() {
            return Err(BackendError::InvalidFrame(format!(
                "{} access unit is empty",
                self.codec.label()
            )));
        }
        if matches!(self.codec, VideoCodec::H264 | VideoCodec::H265)
            && !self.data.starts_with(&[0, 0, 1])
            && !self.data.starts_with(&[0, 0, 0, 1])
        {
            return Err(BackendError::InvalidFrame(format!(
                "{} input must use Annex B start codes",
                self.codec.label()
            )));
        }
        if self.timestamp_100ns < 0 || self.duration_100ns <= 0 {
            return Err(BackendError::InvalidFrame(
                "video timestamps must be non-negative with positive duration".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PcmFrame {
    pub samples: Vec<f32>,
    pub format: AudioFormat,
}

impl PcmFrame {
    pub fn validate(&self) -> Result<(), BackendError> {
        self.format.validate().map_err(|error| {
            BackendError::InvalidFrame(format!("PCM packet format is invalid: {error}"))
        })?;
        if self.samples.is_empty() {
            return Err(BackendError::InvalidFrame(
                "PCM frame must contain samples and at least one channel".to_owned(),
            ));
        }
        if self.samples.len() % self.format.channels as usize != 0 {
            return Err(BackendError::InvalidFrame(
                "interleaved PCM sample count is not divisible by channel count".to_owned(),
            ));
        }
        if self.samples.iter().any(|sample| !sample.is_finite()) {
            return Err(BackendError::InvalidFrame(
                "PCM samples must be finite f32 values".to_owned(),
            ));
        }
        let maximum_frames = self.format.sample_rate as usize * 120 / 1_000;
        if self.frame_count() > maximum_frames {
            return Err(BackendError::InvalidFrame(
                "PCM packet exceeds the 120 ms Opus frame limit".to_owned(),
            ));
        }
        Ok(())
    }

    pub fn frame_count(&self) -> usize {
        self.samples.len() / self.format.channels as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendConfig {
    pub video: VideoFormat,
    pub audio: AudioFormat,
    pub surface: SurfaceTarget,
    pub video_queue_capacity: usize,
    pub audio_queue_capacity: usize,
}

impl BackendConfig {
    pub fn validate(self) -> Result<(), BackendError> {
        self.video.validate()?;
        self.audio.validate()?;
        self.surface.validate()?;
        if !(1..=32).contains(&self.video_queue_capacity) {
            return Err(BackendError::InvalidConfig(
                "video queue capacity must be between 1 and 32 frames".to_owned(),
            ));
        }
        if !(1..=256).contains(&self.audio_queue_capacity) {
            return Err(BackendError::InvalidConfig(
                "audio queue capacity must be between 1 and 256 packets".to_owned(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video_format() -> VideoFormat {
        VideoFormat {
            codec: VideoCodec::H264,
            width: 1920,
            height: 1080,
            frame_rate_numerator: NonZeroU32::new(120).unwrap(),
            frame_rate_denominator: NonZeroU32::new(1).unwrap(),
            average_bitrate: 75_000_000,
        }
    }

    #[test]
    fn validates_supported_h264_format() {
        let format = video_format();
        assert!(format.validate().is_ok());
        assert_eq!(format.frame_duration_100ns(), 83_333);
    }

    #[test]
    fn rejects_out_of_range_h264_format() {
        assert!(
            VideoFormat {
                width: 16,
                ..video_format()
            }
            .validate()
            .is_err()
        );
        assert!(
            VideoFormat {
                average_bitrate: 0,
                ..video_format()
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn requires_annex_b_access_units() {
        let valid = EncodedVideoFrame {
            codec: VideoCodec::H264,
            data: vec![0, 0, 0, 1, 0x67],
            timestamp_100ns: 0,
            duration_100ns: 166_667,
            key_frame: true,
        };
        assert!(valid.validate().is_ok());
        assert!(
            EncodedVideoFrame {
                data: vec![1, 2, 3],
                ..valid
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn validates_interleaved_pcm_shape() {
        let frame = PcmFrame {
            samples: vec![0.0, 0.25, -0.25, 0.0],
            format: AudioFormat {
                sample_rate: 48_000,
                channels: 2,
            },
        };
        assert_eq!(frame.frame_count(), 2);
        assert!(frame.validate().is_ok());
        assert!(
            PcmFrame {
                samples: vec![0.0],
                format: frame.format,
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn owned_surface_updates_preserve_parent_ownership() {
        let parent = WindowHandle::new(1).unwrap();
        let original = OwnedWindow {
            parent: Some(parent),
            bounds: Bounds {
                x: 0,
                y: 0,
                width: 1280,
                height: 720,
            },
            visible: true,
        };
        let moved_and_hidden = OwnedWindow {
            bounds: Bounds {
                x: 40,
                y: 20,
                width: 960,
                height: 540,
            },
            visible: false,
            ..original
        };
        let reparented = OwnedWindow {
            parent: WindowHandle::new(2),
            ..moved_and_hidden
        };

        assert!(original.has_same_parent(moved_and_hidden));
        assert!(!original.has_same_parent(reparented));
    }
}
