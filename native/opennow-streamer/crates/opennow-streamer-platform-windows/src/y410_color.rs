use crate::{
    VideoChromaSiting, VideoColorMatrix, VideoColorPrimaries, VideoFormat, VideoPixelFormat,
    VideoTransferFunction,
};

#[repr(C)]
pub(crate) struct Y410Constants {
    pub(crate) scale_bias: [f32; 4],
    pub(crate) matrix: [f32; 4],
}

impl Y410Constants {
    pub(crate) fn new(format: VideoFormat) -> Result<Self, String> {
        format.validate_color().map_err(|error| error.to_string())?;
        let centered = format.chroma_siting == VideoChromaSiting::Center
            && matches!(
                format.pixel_format,
                VideoPixelFormat::Nv12 | VideoPixelFormat::P010
            )
            && format.transfer_function.is_sdr();
        if format.pixel_format != VideoPixelFormat::Y410 && !centered {
            return Err(
                "shader conversion requires Y410 or centered SDR NV12/P010 output".to_owned(),
            );
        }
        let matrix = match (
            format.transfer_function,
            format.color_primaries,
            format.color_matrix,
        ) {
            (
                VideoTransferFunction::Sdr | VideoTransferFunction::Srgb,
                VideoColorPrimaries::Bt709,
                VideoColorMatrix::Bt709,
            ) => [1.5748, -0.187_324_27, -0.468_124_27, 1.8556],
            (
                VideoTransferFunction::Sdr | VideoTransferFunction::Srgb,
                VideoColorPrimaries::Bt709,
                VideoColorMatrix::Bt601,
            ) => [1.402, -0.344_136_3, -0.714_136_3, 1.772],
            (VideoTransferFunction::Pq, VideoColorPrimaries::Bt2020, VideoColorMatrix::Bt2020) => {
                [1.4746, -0.164_553_12, -0.571_353_14, 1.8814]
            }
            _ => {
                return Err(
                    "shader conversion requires SDR BT.709 primaries or PQ BT.2020".to_owned(),
                );
            }
        };
        let (maximum, black, white, midpoint, chroma_span, sample_scale) = match format.pixel_format
        {
            VideoPixelFormat::Nv12 => (255.0, 16.0, 235.0, 128.0, 224.0, 255.0),
            VideoPixelFormat::P010 => (1023.0, 64.0, 940.0, 512.0, 896.0, 65535.0 / 64.0),
            _ => (1023.0, 64.0, 940.0, 512.0, 896.0, 1.0),
        };
        Ok(Self {
            scale_bias: if format.full_range {
                [
                    sample_scale / maximum,
                    0.0,
                    sample_scale / maximum,
                    -midpoint / maximum,
                ]
            } else {
                [
                    sample_scale / (white - black),
                    -black / (white - black),
                    sample_scale / chroma_span,
                    -midpoint / chroma_span,
                ]
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
    fn centered_chroma_uses_quarter_weights_and_clamps_both_edges() {
        let shader = include_str!("windows/y410.hlsl");
        assert!(shader.contains("float2 chroma_position = position.xy * 0.5 - 0.5;"));
        assert!(shader.contains("int2 first = int2(floor(chroma_position));"));
        assert!(shader.contains("float2 weight = frac(chroma_position);"));
        for offset in [
            "first",
            "first + int2(1, 0)",
            "first + int2(0, 1)",
            "first + int2(1, 1)",
        ] {
            assert!(shader.contains(&format!("clamp({offset}, 0, last)")));
        }
        let mut taps = Vec::new();
        for (pixel, expected) in [
            (0, (0, 0, 0.75)),
            (1, (0, 1, 0.25)),
            (2, (0, 1, 0.75)),
            (3, (1, 2, 0.25)),
            (4, (1, 2, 0.75)),
            (5, (2, 2, 0.25)),
        ] {
            let position = pixel as f32 + 0.5;
            let chroma_position = position * 0.5 - 0.5;
            let first = chroma_position.floor() as i32;
            let weight = chroma_position - chroma_position.floor();
            let actual = (
                first.clamp(0, 2) as usize,
                (first + 1).clamp(0, 2) as usize,
                weight,
            );
            assert_eq!(actual, expected);
            taps.push(actual);
        }
        let impulse = [[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 0.0]];
        let expected_axis = [0.0, 0.25, 0.75, 0.75, 0.25, 0.0];
        for (y, &(top, bottom, vertical)) in taps.iter().enumerate() {
            for (x, &(left, right, horizontal)) in taps.iter().enumerate() {
                let upper =
                    impulse[top][left] * (1.0 - horizontal) + impulse[top][right] * horizontal;
                let lower = impulse[bottom][left] * (1.0 - horizontal)
                    + impulse[bottom][right] * horizontal;
                let actual = upper * (1.0 - vertical) + lower * vertical;
                assert_eq!(actual, expected_axis[x] * expected_axis[y]);
            }
        }
    }

    #[test]
    fn centered_sdr_planes_preserve_range_and_matrix_for_both_transfers() {
        for pixel_format in [VideoPixelFormat::Nv12, VideoPixelFormat::P010] {
            for full_range in [false, true] {
                for transfer_function in [VideoTransferFunction::Sdr, VideoTransferFunction::Srgb] {
                    for (color_matrix, kr, kb) in [
                        (VideoColorMatrix::Bt601, 0.299_f32, 0.114_f32),
                        (VideoColorMatrix::Bt709, 0.2126, 0.0722),
                    ] {
                        let format = VideoFormat {
                            pixel_format,
                            chroma_format: VideoChromaFormat::Cs420,
                            chroma_siting: VideoChromaSiting::Center,
                            full_range,
                            transfer_function,
                            color_primaries: VideoColorPrimaries::Bt709,
                            color_matrix,
                            ..hdr_format(false)
                        };
                        let constants = Y410Constants::new(format).unwrap();
                        let (black, white, midpoint, normalization) =
                            if pixel_format == VideoPixelFormat::Nv12 {
                                (
                                    if full_range { 0 } else { 16 },
                                    if full_range { 255 } else { 235 },
                                    128.0,
                                    255.0,
                                )
                            } else {
                                (
                                    if full_range { 0 } else { 64 },
                                    if full_range { 1023 } else { 940 },
                                    512.0,
                                    65535.0 / 64.0,
                                )
                            };
                        let [ys, yb, cs, cb] = constants.scale_bias;
                        for code in black..=white {
                            let actual = code as f32 / normalization * ys + yb;
                            let expected = (code - black) as f32 / (white - black) as f32;
                            assert!((actual - expected).abs() < 0.000001);
                        }
                        assert!((midpoint / normalization * cs + cb).abs() < 0.000001);
                        let expected = [
                            2.0 * (1.0 - kr),
                            -2.0 * kb * (1.0 - kb) / (1.0 - kr - kb),
                            -2.0 * kr * (1.0 - kr) / (1.0 - kr - kb),
                            2.0 * (1.0 - kb),
                        ];
                        for (actual, expected) in constants.matrix.into_iter().zip(expected) {
                            assert!((actual - expected).abs() < 0.000001);
                        }
                    }
                }
            }
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
