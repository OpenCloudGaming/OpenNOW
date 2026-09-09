use std::sync::{Arc, OnceLock};

use objc2_core_video::{
    CVPixelBufferGetHeightOfPlane, CVPixelBufferGetIOSurface, CVPixelBufferGetPlaneCount,
    CVPixelBufferGetWidthOfPlane,
};

use crate::failure::FailureReporter;
use crate::format::{
    FrameTiming, H265Format, H265ParameterSets, VideoBitDepth, VideoChroma, VideoColorSpace,
    VideoTransfer,
};

use super::Counters;
use super::mailbox::LatestMailbox;
use super::video::{DecodedFrameOutput, VideoDecoder};

const VPS: &[u8] = &[
    64, 1, 12, 1, 255, 255, 4, 8, 0, 0, 3, 0, 156, 8, 0, 0, 3, 0, 0, 30, 149, 152, 9,
];
const SPS: &[u8] = &[
    66, 1, 1, 4, 8, 0, 0, 3, 0, 156, 8, 0, 0, 3, 0, 0, 30, 144, 4, 16, 32, 155, 44, 172, 210, 73,
    149, 224, 45, 1, 0, 0, 3, 0, 1, 0, 0, 3, 0, 1, 8,
];
const PPS: &[u8] = &[68, 1, 193, 114, 134, 12, 66, 36];
const HDR_VPS: &[u8] = &[
    64, 1, 12, 1, 255, 255, 2, 32, 0, 0, 3, 0, 144, 0, 0, 3, 0, 0, 3, 0, 30, 149, 152, 9,
];
const HDR_SPS: &[u8] = &[
    66, 1, 1, 2, 32, 0, 0, 3, 0, 144, 0, 0, 3, 0, 0, 3, 0, 30, 160, 32, 129, 4, 217, 101, 102, 146,
    76, 175, 1, 106, 18, 32, 18, 8, 0, 0, 3, 0, 8, 0, 0, 3, 0, 8, 64,
];
const HDR_PPS: &[u8] = &[68, 1, 193, 114, 180, 34, 64];
const IDR_AVCC: &[u8] = &[
    0, 0, 0, 15, 40, 1, 175, 19, 128, 229, 50, 81, 253, 166, 213, 192, 40, 111, 254,
];

pub fn probe_h265_444_ten_bit_hardware() -> bool {
    static SUPPORTED: OnceLock<bool> = OnceLock::new();
    *SUPPORTED.get_or_init(|| probe(VideoChroma::Yuv444).unwrap_or(false))
}

pub fn probe_h265_hdr_hardware() -> bool {
    static SUPPORTED: OnceLock<bool> = OnceLock::new();
    *SUPPORTED.get_or_init(|| probe(VideoChroma::Yuv420).unwrap_or(false))
}

fn probe(chroma: VideoChroma) -> Result<bool, super::BackendError> {
    if !super::probe_h265_hardware() {
        return Ok(false);
    }
    let (parameters, color, transfer, chroma_size) = match chroma {
        VideoChroma::Yuv420 => (
            H265ParameterSets::new(HDR_VPS, HDR_SPS, HDR_PPS)?,
            VideoColorSpace::Bt2020,
            VideoTransfer::Pq,
            32,
        ),
        VideoChroma::Yuv444 => (
            H265ParameterSets::new(VPS, SPS, PPS)?,
            VideoColorSpace::Bt709,
            VideoTransfer::Sdr,
            64,
        ),
    };
    let format = H265Format::new(parameters, color)
        .with_bit_depth(VideoBitDepth::Ten)
        .with_chroma(chroma)
        .with_transfer(transfer);
    let mailbox = Arc::new(LatestMailbox::new());
    let failures = Arc::new(FailureReporter::default());
    let decoder = VideoDecoder::new(
        &format.into(),
        DecodedFrameOutput::EmbeddedMailbox {
            mailbox: Arc::clone(&mailbox),
            frame_available: None,
        },
        Arc::new(Counters::default()),
        Arc::clone(&failures),
        1,
    )?;
    decoder.submit(IDR_AVCC, FrameTiming::new(0, 1, 1))?;
    drop(decoder);
    let Some(frame) = mailbox.take() else {
        return Ok(false);
    };
    let valid = failures.fatal_failure().is_none()
        && CVPixelBufferGetPlaneCount(&frame.image) == 2
        && CVPixelBufferGetIOSurface(Some(&frame.image)).is_some()
        && CVPixelBufferGetWidthOfPlane(&frame.image, 0) == 64
        && CVPixelBufferGetHeightOfPlane(&frame.image, 0) == 64
        && CVPixelBufferGetWidthOfPlane(&frame.image, 1) == chroma_size
        && CVPixelBufferGetHeightOfPlane(&frame.image, 1) == chroma_size;
    if !valid {
        return Ok(false);
    }
    super::embedded::probe_frame_import(frame)
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "requires macOS VideoToolbox hardware; logs actual HDR decode support"]
    fn report_main10_hdr_hardware_probe() {
        eprintln!(
            "HEVC Main10 PQ decode/import: {}",
            super::probe_h265_hdr_hardware()
        );
    }

    #[test]
    #[ignore = "requires macOS VideoToolbox hardware; logs actual profile support"]
    fn report_main44410_hardware_probe() {
        eprintln!(
            "HEVC Main44410 hardware decode: {}",
            super::probe_h265_444_ten_bit_hardware()
        );
    }
}
