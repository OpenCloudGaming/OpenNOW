use std::path::PathBuf;

use crate::{DecodedVideoFrame, EncodedVideoFrame, Result, StreamFormat, VideoCodec};

#[cfg(feature = "ffmpeg")]
mod ffmpeg;
mod mediacodec;
mod v4l2;
pub(crate) use mediacodec::MediaCodecDecoder;
pub use mediacodec::{MEDIACODEC_MAX_HEIGHT, MEDIACODEC_MAX_WIDTH, probe_mediacodec};
#[cfg(any(feature = "ffmpeg", test))]
#[cfg_attr(not(feature = "ffmpeg"), allow(dead_code))]
mod v4l2_request;
#[cfg(feature = "vaapi")]
mod vaapi;
#[cfg(feature = "ffmpeg")]
mod vaapi_probe;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct VaapiColorSupport {
    pub eight_bit_420: bool,
    pub ten_bit_420: bool,
}

pub fn vaapi_color_support(codec: VideoCodec) -> VaapiColorSupport {
    #[cfg(feature = "ffmpeg")]
    {
        FfmpegDecoder::supports_vaapi_color(codec)
    }
    #[cfg(not(feature = "ffmpeg"))]
    {
        let _ = codec;
        VaapiColorSupport::default()
    }
}

#[cfg(feature = "ffmpeg")]
pub(crate) use ffmpeg::{FfmpegDecoder, FfmpegMode};
pub(crate) use v4l2::probe_v4l2_devices;

pub(crate) fn probe_v4l2_request(
    codec: VideoCodec,
) -> std::result::Result<(String, Option<[u32; 2]>), String> {
    #[cfg(feature = "ffmpeg")]
    {
        let device = v4l2_request::probe(codec, 1920, 1080)?;
        let decoder = FfmpegDecoder::probe(codec, FfmpegMode::V4l2Request)?;
        Ok((
            format!("{}; {decoder}", device.detail),
            device.max_dimensions,
        ))
    }
    #[cfg(not(feature = "ffmpeg"))]
    {
        let _ = codec;
        Err("crate was built without the ffmpeg feature".to_owned())
    }
}

#[cfg(feature = "ffmpeg")]
pub(crate) fn probe_ffmpeg_vulkan(codec: VideoCodec) -> std::result::Result<String, String> {
    FfmpegDecoder::probe(codec, FfmpegMode::Vulkan)
}

#[cfg(feature = "ffmpeg")]
pub(crate) fn probe_ffmpeg_cuda(codec: VideoCodec) -> std::result::Result<String, String> {
    FfmpegDecoder::probe(codec, FfmpegMode::Cuda)
}

#[cfg(feature = "ffmpeg")]
pub(crate) fn probe_ffmpeg_software(codec: VideoCodec) -> std::result::Result<String, String> {
    FfmpegDecoder::probe(codec, FfmpegMode::Software)
}

#[cfg(not(feature = "ffmpeg"))]
pub(crate) fn probe_ffmpeg_vulkan(_: VideoCodec) -> std::result::Result<String, String> {
    Err("crate was built without the ffmpeg feature".to_owned())
}

#[cfg(not(feature = "ffmpeg"))]
pub(crate) fn probe_ffmpeg_cuda(_: VideoCodec) -> std::result::Result<String, String> {
    Err("crate was built without the ffmpeg feature".to_owned())
}

#[cfg(not(feature = "ffmpeg"))]
pub(crate) fn probe_ffmpeg_software(_: VideoCodec) -> std::result::Result<String, String> {
    Err("crate was built without the ffmpeg feature".to_owned())
}

pub(crate) trait VideoDecoder {
    fn close(&mut self) -> Result<()> {
        Ok(())
    }
    fn decode(&mut self, frame: &EncodedVideoFrame) -> Result<Vec<DecodedVideoFrame>>;
    fn poll(&mut self) -> Result<Vec<DecodedVideoFrame>> {
        Ok(Vec::new())
    }
    fn flush(&mut self) -> Result<Vec<DecodedVideoFrame>>;
    fn take_format_change(&mut self) -> Option<StreamFormat>;
}

pub(crate) fn open_v4l2(
    format: StreamFormat,
    device: Option<PathBuf>,
) -> Result<Box<dyn VideoDecoder>> {
    Ok(Box::new(v4l2::V4l2Decoder::open(format, device)?))
}

#[cfg(feature = "vaapi")]
pub(crate) fn open_vaapi(format: StreamFormat) -> Result<Box<dyn VideoDecoder>> {
    Ok(Box::new(vaapi::VaApiDecoder::open(format)?))
}

/// VA-API H.264 is available through the native decoder or, when that cannot open on the
/// driver (radeonsi rejects its 16x16 initial context on some APUs), through FFmpeg's VA-API
/// decoder. The session opens them in the same order.
pub(crate) fn probe_vaapi() -> std::result::Result<String, String> {
    #[cfg(feature = "vaapi")]
    let native = vaapi::VaApiDecoder::probe();
    #[cfg(not(feature = "vaapi"))]
    let native = Err("crate was built without the vaapi feature".to_owned());
    #[cfg(feature = "ffmpeg")]
    let ffmpeg = FfmpegDecoder::supports_vaapi_h264;
    #[cfg(not(feature = "ffmpeg"))]
    let ffmpeg = || false;
    vaapi_h264_probe(native, ffmpeg)
}

fn vaapi_h264_probe(
    native: std::result::Result<String, String>,
    ffmpeg_available: impl FnOnce() -> bool,
) -> std::result::Result<String, String> {
    native.or_else(|native| {
        if ffmpeg_available() {
            Ok(format!(
                "FFmpeg VA-API H.264 (native VA-API H.264: {native})"
            ))
        } else {
            Err(native)
        }
    })
}
#[cfg(all(feature = "ffmpeg", feature = "vulkan"))]
mod vulkan_copy;

#[cfg(test)]
mod tests {
    use super::vaapi_h264_probe;

    #[test]
    fn vaapi_h264_falls_back_to_ffmpeg_only_when_the_native_decoder_cannot_open() {
        assert_eq!(
            vaapi_h264_probe(Ok("native".to_owned()), || panic!("FFmpeg probed")),
            Ok("native".to_owned())
        );
        let rejected = || Err("initial context: resolution not supported".to_owned());
        assert_eq!(
            vaapi_h264_probe(rejected(), || true),
            Ok("FFmpeg VA-API H.264 (native VA-API H.264: initial context: resolution not supported)".to_owned())
        );
        assert_eq!(
            vaapi_h264_probe(rejected(), || false),
            Err("initial context: resolution not supported".to_owned())
        );
    }
}
