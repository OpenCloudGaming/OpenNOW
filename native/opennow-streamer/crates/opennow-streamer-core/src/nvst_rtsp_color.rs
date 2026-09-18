use opennow_streamer_platform::{MediaColorQuality, MediaStreamConfig, MediaVideoCodec};

use super::{NvstRtspError, sdp_attribute};

#[derive(Default, PartialEq)]
struct SdpColor {
    bit_depth: Option<u8>,
    chroma_format: Option<u8>,
    dynamic_range: Option<u8>,
}

impl SdpColor {
    fn parse(sdp: &str) -> Result<Self, NvstRtspError> {
        let mut color = Self::default();
        for line in sdp.lines() {
            let Some((name, value)) = line.trim().split_once(':') else {
                continue;
            };
            let (field, target) = if name.eq_ignore_ascii_case("a=x-nv-video[0].bitDepth")
                || name.eq_ignore_ascii_case("a=video[0].bitDepth")
            {
                ("bitDepth", &mut color.bit_depth)
            } else if name.eq_ignore_ascii_case("a=x-nv-video[0].chromaFormat")
                || name.eq_ignore_ascii_case("a=video[0].chromaFormat")
            {
                ("chromaFormat", &mut color.chroma_format)
            } else if name.eq_ignore_ascii_case("a=x-nv-video[0].dynamicRangeMode")
                || name.eq_ignore_ascii_case("a=video[0].dynamicRangeMode")
            {
                ("dynamicRangeMode", &mut color.dynamic_range)
            } else {
                continue;
            };
            let value = value.trim().parse::<u8>().map_err(|_| {
                NvstRtspError::new(
                    "nvst-color-invalid",
                    format!("NVST {field} must be an unsigned byte"),
                )
            })?;
            if target.is_some_and(|previous| previous != value) {
                return Err(NvstRtspError::new(
                    "nvst-color-invalid",
                    format!("NVST {field} has conflicting values"),
                ));
            }
            *target = Some(value);
        }
        Ok(color)
    }
}

#[derive(Clone, Copy)]
struct WireColor {
    bit_depth: u8,
    chroma_format: u8,
    dynamic_range: u8,
}

impl WireColor {
    fn from_stream(stream: MediaStreamConfig) -> Self {
        Self {
            bit_depth: stream.color_quality.bit_depth(),
            chroma_format: u8::from(stream.color_quality.is_444()),
            dynamic_range: u8::from(stream.hdr),
        }
    }

    fn overlay(self, color: SdpColor) -> Self {
        Self {
            bit_depth: color.bit_depth.unwrap_or(self.bit_depth),
            chroma_format: color.chroma_format.unwrap_or(self.chroma_format),
            dynamic_range: color.dynamic_range.unwrap_or(self.dynamic_range),
        }
    }
}

pub(super) struct NvstColorNegotiation {
    pub(super) stream: MediaStreamConfig,
    baseline: WireColor,
    suppress_defaults: bool,
}

impl NvstColorNegotiation {
    pub(super) fn resolve(
        mut stream: MediaStreamConfig,
        describe: &str,
    ) -> Result<Self, NvstRtspError> {
        let (baseline_sdp, override_sdp) = describe.split_once(";;").unwrap_or((describe, ""));
        let baseline = SdpColor::parse(baseline_sdp)?;
        let overrides = SdpColor::parse(override_sdp)?;
        let suppress_defaults = baseline != SdpColor::default()
            || overrides != SdpColor::default()
            || sdp_attribute(baseline_sdp, "general.nativeRtcOnBundlePort").is_some();
        let baseline = WireColor {
            bit_depth: 8,
            chroma_format: 0,
            dynamic_range: 0,
        }
        .overlay(baseline);
        let effective = WireColor::from_stream(stream).overlay(overrides);
        stream.color_quality = match (effective.bit_depth, effective.chroma_format) {
            (8, 0) => MediaColorQuality::EightBit420,
            (8, 1) => MediaColorQuality::EightBit444,
            (10, 0) => MediaColorQuality::TenBit420,
            (10, 1) => MediaColorQuality::TenBit444,
            _ => {
                return Err(NvstRtspError::new(
                    "nvst-color-unsupported",
                    "NVST requires 8-bit or 10-bit video with chromaFormat 0 (4:2:0) or 1 (4:4:4)",
                ));
            }
        };
        stream.hdr = match effective.dynamic_range {
            0 => false,
            1 if effective.bit_depth == 10 => true,
            _ => {
                return Err(NvstRtspError::new(
                    "nvst-color-unsupported",
                    "NVST requires SDR or 10-bit HDR; the server color override is unsupported",
                ));
            }
        };
        if (stream.codec == MediaVideoCodec::H264
            && (stream.color_quality != MediaColorQuality::EightBit420 || stream.hdr))
            || (stream.codec == MediaVideoCodec::Av1 && stream.color_quality.is_444())
        {
            return Err(NvstRtspError::new(
                "nvst-color-unsupported",
                "The NVST color override is incompatible with the selected codec",
            ));
        }
        Ok(Self {
            stream,
            baseline,
            suppress_defaults,
        })
    }

    pub(super) fn announce_lines(&self) -> Vec<String> {
        let effective = WireColor::from_stream(self.stream);
        [
            ("bitDepth", effective.bit_depth, self.baseline.bit_depth),
            (
                "chromaFormat",
                effective.chroma_format,
                self.baseline.chroma_format,
            ),
            (
                "dynamicRangeMode",
                effective.dynamic_range,
                self.baseline.dynamic_range,
            ),
        ]
        .into_iter()
        .filter(|(_, value, baseline)| !self.suppress_defaults || value != baseline)
        .map(|(name, value, _)| format!("a=x-nv-video[0].{name}:{value}"))
        .collect()
    }
}

#[cfg(test)]
#[path = "nvst_rtsp_color_tests.rs"]
mod tests;
