use ::windows::Win32::Graphics::Dxgi::Common::{
    DXGI_COLOR_SPACE_TYPE, DXGI_COLOR_SPACE_YCBCR_FULL_G22_LEFT_P601,
    DXGI_COLOR_SPACE_YCBCR_FULL_G22_LEFT_P709, DXGI_COLOR_SPACE_YCBCR_FULL_G22_LEFT_P2020,
    DXGI_COLOR_SPACE_YCBCR_FULL_GHLG_TOPLEFT_P2020, DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P601,
    DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P709, DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P2020,
    DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_TOPLEFT_P2020,
    DXGI_COLOR_SPACE_YCBCR_STUDIO_G2084_LEFT_P2020,
    DXGI_COLOR_SPACE_YCBCR_STUDIO_G2084_TOPLEFT_P2020,
    DXGI_COLOR_SPACE_YCBCR_STUDIO_GHLG_TOPLEFT_P2020,
};

use crate::{
    VideoChromaFormat, VideoChromaSiting, VideoColorMatrix, VideoFormat, VideoTransferFunction,
};

pub(super) fn input_color_space(format: VideoFormat) -> Result<DXGI_COLOR_SPACE_TYPE, String> {
    format.validate_color().map_err(|error| error.to_string())?;
    let chroma_siting = if format.chroma_format == VideoChromaFormat::Cs444 {
        match format.transfer_function {
            VideoTransferFunction::Hlg => VideoChromaSiting::TopLeft,
            _ => VideoChromaSiting::Left,
        }
    } else {
        format.chroma_siting
    };
    match (
        format.transfer_function,
        chroma_siting,
        format.color_matrix,
        format.full_range,
    ) {
        (VideoTransferFunction::Pq, VideoChromaSiting::Left, _, false) => {
            Ok(DXGI_COLOR_SPACE_YCBCR_STUDIO_G2084_LEFT_P2020)
        }
        (VideoTransferFunction::Pq, VideoChromaSiting::TopLeft, _, false) => {
            Ok(DXGI_COLOR_SPACE_YCBCR_STUDIO_G2084_TOPLEFT_P2020)
        }
        (VideoTransferFunction::Hlg, VideoChromaSiting::TopLeft, _, true) => {
            Ok(DXGI_COLOR_SPACE_YCBCR_FULL_GHLG_TOPLEFT_P2020)
        }
        (VideoTransferFunction::Hlg, VideoChromaSiting::TopLeft, _, false) => {
            Ok(DXGI_COLOR_SPACE_YCBCR_STUDIO_GHLG_TOPLEFT_P2020)
        }
        (VideoTransferFunction::Sdr, VideoChromaSiting::Left, VideoColorMatrix::Bt601, true) => {
            Ok(DXGI_COLOR_SPACE_YCBCR_FULL_G22_LEFT_P601)
        }
        (VideoTransferFunction::Sdr, VideoChromaSiting::Left, VideoColorMatrix::Bt601, false) => {
            Ok(DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P601)
        }
        (VideoTransferFunction::Sdr, VideoChromaSiting::Left, VideoColorMatrix::Bt709, true) => {
            Ok(DXGI_COLOR_SPACE_YCBCR_FULL_G22_LEFT_P709)
        }
        (VideoTransferFunction::Sdr, VideoChromaSiting::Left, VideoColorMatrix::Bt709, false) => {
            Ok(DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P709)
        }
        (VideoTransferFunction::Sdr, VideoChromaSiting::Left, VideoColorMatrix::Bt2020, true) => {
            Ok(DXGI_COLOR_SPACE_YCBCR_FULL_G22_LEFT_P2020)
        }
        (VideoTransferFunction::Sdr, VideoChromaSiting::Left, VideoColorMatrix::Bt2020, false) => {
            Ok(DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P2020)
        }
        (
            VideoTransferFunction::Sdr,
            VideoChromaSiting::TopLeft,
            VideoColorMatrix::Bt2020,
            false,
        ) => Ok(DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_TOPLEFT_P2020),
        _ => Err(format!(
            "no DXGI input color space for {:?} {:?} {:?} fullRange={}",
            format.transfer_function, format.chroma_siting, format.color_matrix, format.full_range
        )),
    }
}
