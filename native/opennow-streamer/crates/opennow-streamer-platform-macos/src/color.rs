use crate::format::{VideoBitDepth, VideoColorSpace};

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct ConversionParameters {
    pub sample_scale: f32,
    pub luma_offset: f32,
    pub luma_scale: f32,
    pub chroma_offset: f32,
    pub chroma_scale: f32,
    pub red_cr: f32,
    pub green_cb: f32,
    pub green_cr: f32,
    pub blue_cb: f32,
}

impl ConversionParameters {
    pub(crate) fn new(
        bit_depth: VideoBitDepth,
        full_range: bool,
        color_space: VideoColorSpace,
    ) -> Self {
        let (maximum, black, luma_range, midpoint, chroma_range, sample_scale) = match bit_depth {
            VideoBitDepth::Eight => (255.0, 16.0, 219.0, 128.0, 224.0, 1.0),
            VideoBitDepth::Ten => (1023.0, 64.0, 876.0, 512.0, 896.0, 65535.0 / 65472.0),
        };
        let (kr, kb) = match color_space {
            VideoColorSpace::Bt601 => (0.299, 0.114),
            VideoColorSpace::Bt709 => (0.2126, 0.0722),
            VideoColorSpace::Bt2020 => (0.2627, 0.0593),
        };
        let kg = 1.0 - kr - kb;
        Self {
            sample_scale,
            luma_offset: if full_range { 0.0 } else { black / maximum },
            luma_scale: if full_range {
                1.0
            } else {
                maximum / luma_range
            },
            chroma_offset: midpoint / maximum,
            chroma_scale: if full_range {
                1.0
            } else {
                maximum / chroma_range
            },
            red_cr: 2.0 * (1.0 - kr),
            green_cb: -2.0 * kb * (1.0 - kb) / kg,
            green_cr: -2.0 * kr * (1.0 - kr) / kg,
            blue_cb: 2.0 * (1.0 - kb),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn convert_codes(
        parameters: ConversionParameters,
        depth: VideoBitDepth,
        codes: [f32; 3],
    ) -> [f32; 3] {
        let storage_scale = match depth {
            VideoBitDepth::Eight => 1.0 / 255.0,
            VideoBitDepth::Ten => 64.0 / 65535.0,
        };
        let samples = codes.map(|code| code * storage_scale * parameters.sample_scale);
        let y = (samples[0] - parameters.luma_offset) * parameters.luma_scale;
        let cb = (samples[1] - parameters.chroma_offset) * parameters.chroma_scale;
        let cr = (samples[2] - parameters.chroma_offset) * parameters.chroma_scale;
        [
            y + parameters.red_cr * cr,
            y + parameters.green_cb * cb + parameters.green_cr * cr,
            y + parameters.blue_cb * cb,
        ]
    }

    fn assert_rgb(actual: [f32; 3], expected: [f32; 3]) {
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!(
                (actual - expected).abs() < 0.000002,
                "{actual} != {expected}"
            );
        }
    }

    #[test]
    fn exact_neutral_chroma_and_black_white_endpoints_in_both_ranges_and_depths() {
        for depth in [VideoBitDepth::Eight, VideoBitDepth::Ten] {
            for full_range in [false, true] {
                let (black, white, midpoint) = match (depth, full_range) {
                    (VideoBitDepth::Eight, false) => (16.0, 235.0, 128.0),
                    (VideoBitDepth::Eight, true) => (0.0, 255.0, 128.0),
                    (VideoBitDepth::Ten, false) => (64.0, 940.0, 512.0),
                    (VideoBitDepth::Ten, true) => (0.0, 1023.0, 512.0),
                };
                for matrix in [VideoColorSpace::Bt601, VideoColorSpace::Bt709] {
                    let parameters = ConversionParameters::new(depth, full_range, matrix);
                    for level in [0.0, 0.25, 0.5, 0.75, 1.0] {
                        assert_rgb(
                            convert_codes(
                                parameters,
                                depth,
                                [black + level * (white - black), midpoint, midpoint],
                            ),
                            [level; 3],
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn bt709_color_vectors_use_range_specific_chroma_gain() {
        for depth in [VideoBitDepth::Eight, VideoBitDepth::Ten] {
            for full_range in [false, true] {
                let (maximum, midpoint, black, y_range, c_range) = match (depth, full_range) {
                    (VideoBitDepth::Eight, false) => (255.0, 128.0, 16.0, 219.0, 224.0),
                    (VideoBitDepth::Eight, true) => (255.0, 128.0, 0.0, 255.0, 255.0),
                    (VideoBitDepth::Ten, false) => (1023.0, 512.0, 64.0, 876.0, 896.0),
                    (VideoBitDepth::Ten, true) => (1023.0, 512.0, 0.0, 1023.0, 1023.0),
                };
                let parameters =
                    ConversionParameters::new(depth, full_range, VideoColorSpace::Bt709);
                for rgb in [[0.8, 0.1, 0.2], [0.1, 0.8, 0.2], [0.2, 0.1, 0.8]] {
                    let y = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
                    let cb = (rgb[2] - y) / 1.8556;
                    let cr = (rgb[0] - y) / 1.5748;
                    let codes = [
                        black + y * y_range,
                        midpoint + cb * c_range,
                        midpoint + cr * c_range,
                    ];
                    assert!(codes.iter().all(|code| (0.0..=maximum).contains(code)));
                    assert_rgb(convert_codes(parameters, depth, codes), rgb);
                }
            }
        }
    }

    #[test]
    fn p010_retains_every_legal_luma_step_in_rgb10_output() {
        for (full_range, black, white) in [(false, 64, 940), (true, 0, 1023)] {
            let parameters =
                ConversionParameters::new(VideoBitDepth::Ten, full_range, VideoColorSpace::Bt709);
            let mut previous = None;
            for code in black..=white {
                let rgb =
                    convert_codes(parameters, VideoBitDepth::Ten, [code as f32, 512.0, 512.0]);
                let expected = (code - black) as f32 / (white - black) as f32;
                assert_rgb(rgb, [expected; 3]);
                let quantized = (rgb[0] * 1023.0).round() as u32;
                if let Some(previous) = previous {
                    assert!(quantized > previous);
                }
                previous = Some(quantized);
            }
            assert_eq!(previous, Some(1023));
        }
    }

    #[test]
    fn metal_uniform_layout_is_nine_packed_floats() {
        assert_eq!(std::mem::size_of::<ConversionParameters>(), 9 * 4);
        assert_eq!(std::mem::align_of::<ConversionParameters>(), 4);
    }

    #[test]
    fn bt2020_pq_code_values_preserve_matrix_precision_in_both_ranges() {
        for (full_range, black, y_range, c_range) in
            [(false, 64.0, 876.0, 896.0), (true, 0.0, 1023.0, 1023.0)]
        {
            let parameters =
                ConversionParameters::new(VideoBitDepth::Ten, full_range, VideoColorSpace::Bt2020);
            for rgb in [
                [0.5080784; 3],
                [0.7518271; 3],
                [1.0; 3],
                [0.7, 0.2, 0.4],
                [0.2, 0.8, 0.4],
            ] {
                let y = 0.2627 * rgb[0] + 0.6780 * rgb[1] + 0.0593 * rgb[2];
                let cb = (rgb[2] - y) / 1.8814;
                let cr = (rgb[0] - y) / 1.4746;
                assert_rgb(
                    convert_codes(
                        parameters,
                        VideoBitDepth::Ten,
                        [
                            black + y * y_range,
                            512.0 + cb * c_range,
                            512.0 + cr * c_range,
                        ],
                    ),
                    rgb,
                );
            }
        }
    }
}
