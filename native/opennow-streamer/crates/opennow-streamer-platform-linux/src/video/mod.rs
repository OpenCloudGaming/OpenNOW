use std::path::PathBuf;

use crate::{DecodedVideoFrame, EncodedVideoFrame, Result, StreamFormat, VideoCodec};

#[cfg(feature = "ffmpeg")]
mod ffmpeg;
mod v4l2;
#[cfg(feature = "ffmpeg")]
mod v4l2_request;
#[cfg(feature = "vaapi")]
mod vaapi;
#[cfg(feature = "ffmpeg")]
mod vaapi_probe;

pub fn supports_vaapi_ten_bit(codec: VideoCodec) -> bool {
    #[cfg(feature = "ffmpeg")]
    {
        FfmpegDecoder::supports_vaapi_ten_bit(codec)
    }
    #[cfg(not(feature = "ffmpeg"))]
    {
        let _ = codec;
        false
    }
}

#[cfg(feature = "ffmpeg")]
pub(crate) use ffmpeg::{FfmpegDecoder, FfmpegMode};
pub(crate) use v4l2::probe_v4l2_devices;

pub(crate) fn probe_v4l2_request() -> std::result::Result<String, String> {
    #[cfg(feature = "ffmpeg")]
    {
        let device = v4l2_request::probe()?;
        let decoder = FfmpegDecoder::probe(VideoCodec::H265, FfmpegMode::V4l2Request)?;
        Ok(format!("{device}; {decoder}"))
    }
    #[cfg(not(feature = "ffmpeg"))]
    {
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

#[cfg(feature = "vaapi")]
pub(crate) fn probe_vaapi() -> std::result::Result<String, String> {
    vaapi::VaApiDecoder::probe()
}

#[cfg(not(feature = "vaapi"))]
pub(crate) fn probe_vaapi() -> std::result::Result<String, String> {
    Err("crate was built without the vaapi feature".to_owned())
}
#[cfg(all(feature = "ffmpeg", feature = "vulkan"))]
mod vulkan_copy;
