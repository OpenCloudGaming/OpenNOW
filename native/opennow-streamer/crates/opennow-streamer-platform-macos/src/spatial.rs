use crate::format::MetalFrameFormat;

const MAX_SPATIAL_DIMENSION: usize = 8192;
const MAX_SPATIAL_PIXELS: usize = 7680 * 4320;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SpatialControls {
    pub sharpness: f32,
    pub denoise: f32,
}

impl SpatialControls {
    pub fn new(sharpness: u32, denoise: u32) -> Self {
        Self {
            sharpness: sharpness.min(15) as f32 / 10.0,
            denoise: (denoise.min(20) as f32 / 10.0 * 0.65).min(1.0),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SpatialConfig {
    pub input_width: usize,
    pub input_height: usize,
    pub output_width: usize,
    pub output_height: usize,
    pub format: MetalFrameFormat,
}

impl SpatialConfig {
    pub fn new(
        input_width: usize,
        input_height: usize,
        output_width: u32,
        output_height: u32,
        format: MetalFrameFormat,
    ) -> Option<Self> {
        let output_width = usize::try_from(output_width).ok()?;
        let output_height = usize::try_from(output_height).ok()?;
        if input_width == 0
            || input_height == 0
            || output_width < input_width
            || output_height < input_height
            || (output_width == input_width && output_height == input_height)
            || output_width > MAX_SPATIAL_DIMENSION
            || output_height > MAX_SPATIAL_DIMENSION
            || output_width.checked_mul(output_height)? > MAX_SPATIAL_PIXELS
        {
            return None;
        }
        Some(Self {
            input_width,
            input_height,
            output_width,
            output_height,
            format,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spatial_controls_match_mac_source_stage_uniforms() {
        assert_eq!(std::mem::size_of::<SpatialControls>(), 8);
        assert_eq!(std::mem::offset_of!(SpatialControls, denoise), 4);
        for (sharpness, denoise, expected_sharpness, expected_denoise) in [
            (0, 0, 0.0, 0.0),
            (10, 0, 1.0, 0.0),
            (15, 10, 1.5, 0.65),
            (5, 15, 0.5, 0.975),
            (15, 16, 1.5, 1.0),
            (15, 20, 1.5, 1.0),
            (u32::MAX, u32::MAX, 1.5, 1.0),
        ] {
            let controls = SpatialControls::new(sharpness, denoise);
            assert!((controls.sharpness - expected_sharpness).abs() < 0.000001);
            assert!((controls.denoise - expected_denoise).abs() < 0.000001);
        }
    }

    #[test]
    fn spatial_scaling_requires_enlargement_on_at_least_one_axis() {
        for (width, height) in [
            (0, 0),
            (0, 1440),
            (2560, 0),
            (1920, 1080),
            (1280, 720),
            (2560, 720),
        ] {
            assert!(
                SpatialConfig::new(1920, 1080, width, height, MetalFrameFormat::Rgba8Unorm)
                    .is_none()
            );
        }
        for (width, height) in [(2560, 1440), (3840, 2160), (2560, 1080), (1920, 1440)] {
            assert!(
                SpatialConfig::new(1920, 1080, width, height, MetalFrameFormat::Rgba8Unorm)
                    .is_some()
            );
        }
    }

    #[test]
    fn spatial_allocations_are_bounded_and_reject_invalid_source_sizes() {
        for (input_width, input_height, width, height) in [
            (0, 1080, 3840, 2160),
            (1920, 0, 3840, 2160),
            (1920, 1080, u32::MAX, u32::MAX),
            (1920, 1080, 8192, 8192),
            (usize::MAX, 1080, 3840, 2160),
        ] {
            assert!(
                SpatialConfig::new(
                    input_width,
                    input_height,
                    width,
                    height,
                    MetalFrameFormat::Rgba8Unorm
                )
                .is_none()
            );
        }
        assert!(SpatialConfig::new(3840, 2160, 7680, 4320, MetalFrameFormat::Rgba8Unorm).is_some());
    }

    #[test]
    fn spatial_configuration_preserves_sdr_bit_depth_and_keys_size_changes() {
        let eight =
            SpatialConfig::new(1920, 1080, 3840, 2160, MetalFrameFormat::Rgba8Unorm).unwrap();
        let ten =
            SpatialConfig::new(1920, 1080, 3840, 2160, MetalFrameFormat::Rgb10a2Unorm).unwrap();
        assert_ne!(eight, ten);
        assert_eq!(ten.format, MetalFrameFormat::Rgb10a2Unorm);
        assert_ne!(
            eight,
            SpatialConfig::new(1920, 1080, 2560, 1440, eight.format).unwrap()
        );
        assert_ne!(
            eight,
            SpatialConfig::new(1280, 720, 3840, 2160, eight.format).unwrap()
        );
    }
}
