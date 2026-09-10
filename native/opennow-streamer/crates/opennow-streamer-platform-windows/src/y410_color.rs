use crate::{
    VideoColorMatrix, VideoColorPrimaries, VideoFormat, VideoPixelFormat, VideoTransferFunction,
};

#[repr(C)]
pub(crate) struct Y410Constants {
    pub(crate) scale_bias: [f32; 4],
    pub(crate) matrix: [f32; 4],
}

impl Y410Constants {
    pub(crate) fn new(format: VideoFormat) -> Result<Self, String> {
        format.validate_color().map_err(|error| error.to_string())?;
        if format.pixel_format != VideoPixelFormat::Y410 {
            return Err("Y410 shader conversion requires packed 10-bit 4:4:4 output".to_owned());
        }
        let matrix = match (
            format.transfer_function,
            format.color_primaries,
            format.color_matrix,
        ) {
            (VideoTransferFunction::Sdr, VideoColorPrimaries::Bt709, VideoColorMatrix::Bt709) => {
                [1.5748, -0.187_324_27, -0.468_124_27, 1.8556]
            }
            (VideoTransferFunction::Pq, VideoColorPrimaries::Bt2020, VideoColorMatrix::Bt2020) => {
                [1.4746, -0.164_553_12, -0.571_353_14, 1.8814]
            }
            _ => return Err("Y410 shader conversion requires SDR BT.709 or PQ BT.2020".to_owned()),
        };
        Ok(Self {
            scale_bias: if format.full_range {
                [1.0 / 1023.0, 0.0, 1.0 / 1023.0, -512.0 / 1023.0]
            } else {
                [1.0 / 876.0, -64.0 / 876.0, 1.0 / 896.0, -512.0 / 896.0]
            },
            matrix,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{VideoChromaFormat, VideoChromaSiting, VideoCodec};
    use std::num::NonZeroU32;

    fn hdr_format(full_range: bool) -> VideoFormat {
        VideoFormat {
            codec: VideoCodec::H265,
            width: 1920,
            height: 1080,
            frame_rate_numerator: NonZeroU32::new(60).unwrap(),
            frame_rate_denominator: NonZeroU32::new(1).unwrap(),
            average_bitrate: 10_000_000,
            pixel_format: VideoPixelFormat::Y410,
            chroma_format: VideoChromaFormat::Cs444,
            chroma_siting: VideoChromaSiting::Left,
            full_range,
            transfer_function: VideoTransferFunction::Pq,
            color_primaries: VideoColorPrimaries::Bt2020,
            color_matrix: VideoColorMatrix::Bt2020,
        }
    }

    #[test]
    fn hdr_y410_preserves_pq_codes_and_uses_bt2020_matrix_in_both_ranges() {
        assert_eq!(std::mem::size_of::<Y410Constants>(), 32);
        for full_range in [false, true] {
            let constants = Y410Constants::new(hdr_format(full_range)).unwrap();
            let [ys, yb, cs, cb] = constants.scale_bias;
            let [rv, gu, gv, bu] = constants.matrix;
            let (black, white) = if full_range { (0, 1023) } else { (64, 940) };
            for code in black..=white {
                let y = code as f32 * ys + yb;
                let chroma = 512.0 * cs + cb;
                assert!(chroma.abs() < 0.000001);
                let expected = (code - black) as f32 / (white - black) as f32;
                assert!((y - expected).abs() < 0.000001);
            }
            let y = 512.0 * ys + yb;
            let u = 384.0 * cs + cb;
            let v = 640.0 * cs + cb;
            let rgb = [y + rv * v, y + gu * u + gv * v, y + bu * u];
            let kr = 0.2627_f32;
            let kb = 0.0593_f32;
            let expected = [
                y + 2.0 * (1.0 - kr) * v,
                y - 2.0 * kb * (1.0 - kb) / (1.0 - kr - kb) * u
                    - 2.0 * kr * (1.0 - kr) / (1.0 - kr - kb) * v,
                y + 2.0 * (1.0 - kb) * u,
            ];
            for (actual, expected) in rgb.into_iter().zip(expected) {
                assert!((actual - expected).abs() < 0.000001);
            }
            assert!((rgb[0] - (y + 1.5748 * v)).abs() > 0.01);
        }
    }

    #[test]
    fn hdr_y410_rejects_mismatched_color_metadata_and_hlg() {
        for format in [
            VideoFormat {
                color_matrix: VideoColorMatrix::Bt709,
                ..hdr_format(false)
            },
            VideoFormat {
                transfer_function: VideoTransferFunction::Hlg,
                ..hdr_format(false)
            },
            VideoFormat {
                pixel_format: VideoPixelFormat::Ayuv,
                ..hdr_format(false)
            },
        ] {
            assert!(Y410Constants::new(format).is_err());
        }
    }
}
