use crate::{
    VideoChromaSiting, VideoColorMatrix, VideoColorPrimaries, VideoFormat, VideoTransferFunction,
};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) enum Descriptor<T> {
    #[default]
    Unspecified,
    Recognized(T),
    Unsupported(u32),
}

impl<T: Copy + std::fmt::Debug> Descriptor<T> {
    fn resolve(self, field_bit: u32, field: &str, fallback: T) -> T {
        match self {
            Self::Unspecified => fallback,
            Self::Recognized(value) => value,
            Self::Unsupported(value) => {
                static REPORTED: std::sync::atomic::AtomicU32 =
                    std::sync::atomic::AtomicU32::new(0);
                let bit = 1 << field_bit;
                if REPORTED.fetch_or(bit, std::sync::atomic::Ordering::Relaxed) & bit == 0 {
                    eprintln!(
                        "warning: decoded Media Foundation {field} {value} is not presentable; using negotiated {fallback:?}"
                    );
                }
                fallback
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ColorDescriptors {
    pub transfer: Descriptor<VideoTransferFunction>,
    pub primaries: Descriptor<VideoColorPrimaries>,
    pub matrix: Descriptor<VideoColorMatrix>,
    pub range: Descriptor<bool>,
    pub chroma_siting: Descriptor<VideoChromaSiting>,
}

impl ColorDescriptors {
    pub fn resolve(self, fallback: VideoFormat) -> VideoFormat {
        let mut format = VideoFormat {
            transfer_function: self.transfer.resolve(
                0,
                "transfer function",
                fallback.transfer_function,
            ),
            color_primaries: self
                .primaries
                .resolve(1, "color primaries", fallback.color_primaries),
            color_matrix: self
                .matrix
                .resolve(2, "YCbCr matrix", fallback.color_matrix),
            full_range: self.range.resolve(3, "nominal range", fallback.full_range),
            chroma_siting: self
                .chroma_siting
                .resolve(4, "chroma siting", fallback.chroma_siting),
            ..fallback
        };
        if matches!(self.matrix, Descriptor::Unspecified)
            && format.transfer_function.is_sdr()
            && format.color_primaries == VideoColorPrimaries::Bt709
            && fallback.color_matrix == VideoColorMatrix::Bt2020
        {
            format.color_matrix = VideoColorMatrix::Bt709;
        }
        format
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{VideoChromaFormat, VideoCodec, VideoPixelFormat};
    use std::num::NonZeroU32;

    fn hdr_format() -> VideoFormat {
        VideoFormat {
            codec: VideoCodec::H265,
            width: 1920,
            height: 1080,
            frame_rate_numerator: NonZeroU32::new(60).unwrap(),
            frame_rate_denominator: NonZeroU32::new(1).unwrap(),
            average_bitrate: 20_000_000,
            pixel_format: VideoPixelFormat::P010,
            chroma_format: VideoChromaFormat::Cs420,
            chroma_siting: VideoChromaSiting::TopLeft,
            full_range: true,
            transfer_function: VideoTransferFunction::Pq,
            color_primaries: VideoColorPrimaries::Bt2020,
            color_matrix: VideoColorMatrix::Bt2020,
        }
    }

    #[test]
    fn unsupported_descriptors_fall_back_independently() {
        let fallback = hdr_format();
        for descriptors in [
            ColorDescriptors {
                transfer: Descriptor::Unsupported(999),
                ..Default::default()
            },
            ColorDescriptors {
                primaries: Descriptor::Unsupported(999),
                ..Default::default()
            },
            ColorDescriptors {
                matrix: Descriptor::Unsupported(999),
                ..Default::default()
            },
            ColorDescriptors {
                range: Descriptor::Unsupported(3),
                ..Default::default()
            },
            ColorDescriptors {
                chroma_siting: Descriptor::Unsupported(999),
                ..Default::default()
            },
            ColorDescriptors {
                transfer: Descriptor::Unsupported(u32::MAX),
                primaries: Descriptor::Unsupported(u32::MAX),
                matrix: Descriptor::Unsupported(u32::MAX),
                range: Descriptor::Unsupported(u32::MAX),
                chroma_siting: Descriptor::Unsupported(u32::MAX),
            },
        ] {
            assert_eq!(descriptors.resolve(fallback), fallback);
        }
    }

    #[test]
    fn unsupported_fields_do_not_discard_recognized_fields() {
        let fallback = hdr_format();
        let descriptors = ColorDescriptors {
            transfer: Descriptor::Recognized(VideoTransferFunction::Hlg),
            primaries: Descriptor::Unsupported(999),
            matrix: Descriptor::Unsupported(999),
            range: Descriptor::Recognized(false),
            chroma_siting: Descriptor::Recognized(VideoChromaSiting::Left),
        };
        assert_eq!(
            descriptors.resolve(fallback),
            VideoFormat {
                transfer_function: VideoTransferFunction::Hlg,
                full_range: false,
                chroma_siting: VideoChromaSiting::Left,
                ..fallback
            }
        );
    }

    #[test]
    fn unsupported_matrix_does_not_use_the_unspecified_sdr_rule() {
        let fallback = hdr_format();
        let descriptors = ColorDescriptors {
            transfer: Descriptor::Recognized(VideoTransferFunction::Sdr),
            primaries: Descriptor::Recognized(VideoColorPrimaries::Bt709),
            matrix: Descriptor::Unsupported(999),
            ..Default::default()
        };
        let output = descriptors.resolve(fallback);
        assert_eq!(output.color_matrix, fallback.color_matrix);
        assert!(output.validate_color().is_err());
        let output = ColorDescriptors {
            matrix: Descriptor::Unspecified,
            ..descriptors
        }
        .resolve(fallback);
        assert_eq!(output.color_matrix, VideoColorMatrix::Bt709);
        assert!(output.validate_color().is_ok());
    }

    #[test]
    fn recognized_incompatible_fields_are_not_repaired() {
        let fallback = hdr_format();
        for descriptors in [
            ColorDescriptors {
                primaries: Descriptor::Recognized(VideoColorPrimaries::Bt709),
                ..Default::default()
            },
            ColorDescriptors {
                matrix: Descriptor::Recognized(VideoColorMatrix::Bt709),
                transfer: Descriptor::Unsupported(999),
                ..Default::default()
            },
        ] {
            assert!(descriptors.resolve(fallback).validate_color().is_err());
        }
        let output = ColorDescriptors::default().resolve(VideoFormat {
            pixel_format: VideoPixelFormat::Nv12,
            ..fallback
        });
        assert!(output.validate_color().is_err());
    }

    #[test]
    fn unspecified_fields_and_recognized_transitions_are_preserved() {
        let hdr = hdr_format();
        assert_eq!(ColorDescriptors::default().resolve(hdr), hdr);
        let sdr = VideoFormat {
            transfer_function: VideoTransferFunction::Sdr,
            color_primaries: VideoColorPrimaries::Bt709,
            color_matrix: VideoColorMatrix::Bt709,
            ..hdr
        };
        for format in [
            sdr,
            hdr,
            VideoFormat {
                transfer_function: VideoTransferFunction::Hlg,
                ..hdr
            },
        ] {
            let descriptors = ColorDescriptors {
                transfer: Descriptor::Recognized(format.transfer_function),
                primaries: Descriptor::Recognized(format.color_primaries),
                matrix: Descriptor::Recognized(format.color_matrix),
                range: Descriptor::Recognized(format.full_range),
                chroma_siting: Descriptor::Recognized(format.chroma_siting),
            };
            assert_eq!(descriptors.resolve(sdr), format);
            assert_eq!(descriptors.resolve(hdr), format);
        }
    }

    #[test]
    fn negotiated_sdr_matrix_is_preserved_and_hdr_transitions_include_srgb() {
        for transfer in [VideoTransferFunction::Sdr, VideoTransferFunction::Srgb] {
            let sdr = VideoFormat {
                transfer_function: transfer,
                color_primaries: VideoColorPrimaries::Bt709,
                color_matrix: VideoColorMatrix::Bt601,
                ..hdr_format()
            };
            assert_eq!(ColorDescriptors::default().resolve(sdr), sdr);
            let descriptors = ColorDescriptors {
                transfer: Descriptor::Recognized(transfer),
                primaries: Descriptor::Recognized(VideoColorPrimaries::Bt709),
                ..Default::default()
            };
            let output = descriptors.resolve(hdr_format());
            assert_eq!(output.transfer_function, transfer);
            assert_eq!(output.color_matrix, VideoColorMatrix::Bt709);
            assert!(output.validate_color().is_ok());
        }
    }
}
