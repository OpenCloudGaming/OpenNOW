use std::ffi::c_int;

use ffmpeg_next::ffi;

use crate::{StreamFormat, VideoCodec};

#[repr(C)]
#[derive(Default)]
struct DecodeCaps {
    codec: u32,
    chroma: u32,
    bit_depth_minus_eight: u32,
    reserved1: [u32; 3],
    supported: u8,
    decoder_count: u8,
    output_format_mask: u16,
    max_width: u32,
    max_height: u32,
    max_macroblocks: u32,
    min_width: u16,
    min_height: u16,
    histogram_supported: u8,
    counter_bit_depth: u8,
    max_histogram_bins: u16,
    decode_stats_supported: u8,
    reserved4: [u8; 3],
    reserved3: [u32; 9],
}

const _: () = {
    assert!(std::mem::size_of::<DecodeCaps>() == 88);
    assert!(std::mem::offset_of!(DecodeCaps, supported) == 24);
    assert!(std::mem::offset_of!(DecodeCaps, max_macroblocks) == 36);
    assert!(std::mem::offset_of!(DecodeCaps, min_width) == 40);
};

impl DecodeCaps {
    fn query(codec: VideoCodec) -> Self {
        Self {
            codec: match codec {
                VideoCodec::H264 => 4,
                VideoCodec::H265 => 8,
                VideoCodec::Av1 => 11,
            },
            chroma: 1,
            ..Self::default()
        }
    }

    fn validate(&self, codec: VideoCodec, format: StreamFormat) -> Result<(), String> {
        if self.supported == 0 || self.output_format_mask & 1 == 0 {
            return Err(format!(
                "CUDA/NVDEC device does not support {} 8-bit 4:2:0 NV12 decoding",
                codec.label()
            ));
        }
        let macroblocks =
            u64::from(format.width.div_ceil(16)) * u64::from(format.height.div_ceil(16));
        if format.width < u32::from(self.min_width)
            || format.height < u32::from(self.min_height)
            || format.width > self.max_width
            || format.height > self.max_height
            || macroblocks > u64::from(self.max_macroblocks)
        {
            return Err(format!(
                "CUDA/NVDEC {} dimensions {}x{} exceed driver decode limits {}x{}..{}x{} with {} macroblocks",
                codec.label(),
                format.width,
                format.height,
                self.min_width,
                self.min_height,
                self.max_width,
                self.max_height,
                self.max_macroblocks
            ));
        }
        Ok(())
    }
}

type PushContext = unsafe extern "C" fn(ffi::CUcontext) -> c_int;
type PopContext = unsafe extern "C" fn(*mut ffi::CUcontext) -> c_int;
type GetDecoderCaps = unsafe extern "C" fn(*mut DecodeCaps) -> c_int;

struct CudaDriver {
    _cuda: libloading::Library,
    _cuvid: libloading::Library,
    push: PushContext,
    pop: PopContext,
    get_caps: GetDecoderCaps,
}

impl CudaDriver {
    fn load() -> Result<Self, String> {
        let load = || unsafe {
            let cuda = libloading::Library::new("libcuda.so.1")?;
            let cuvid = libloading::Library::new("libnvcuvid.so.1")?;
            Ok::<_, libloading::Error>(Self {
                push: *cuda.get(b"cuCtxPushCurrent_v2\0")?,
                pop: *cuda.get(b"cuCtxPopCurrent_v2\0")?,
                get_caps: *cuvid.get(b"cuvidGetDecoderCaps\0")?,
                _cuda: cuda,
                _cuvid: cuvid,
            })
        };
        load().map_err(|error| format!("CUDA/NVDEC capability query unavailable: {error}"))
    }

    fn validate(
        &self,
        context: ffi::CUcontext,
        codec: VideoCodec,
        format: StreamFormat,
    ) -> Result<(), String> {
        let status = unsafe { (self.push)(context) };
        if status != 0 {
            return Err(format!(
                "CUDA/NVDEC context push failed: CUDA status {status}"
            ));
        }
        let mut caps = DecodeCaps::query(codec);
        let query_status = unsafe { (self.get_caps)(&mut caps) };
        let mut popped = std::ptr::null_mut();
        let pop_status = unsafe { (self.pop)(&mut popped) };
        if pop_status != 0 {
            return Err(format!(
                "CUDA/NVDEC context pop failed: CUDA status {pop_status}"
            ));
        }
        if query_status != 0 {
            return Err(format!(
                "CUDA/NVDEC {} capability query failed: CUDA status {query_status}",
                codec.label()
            ));
        }
        caps.validate(codec, format)
    }
}

pub(super) fn validate(
    context: ffi::CUcontext,
    codec: VideoCodec,
    format: StreamFormat,
) -> Result<(), String> {
    CudaDriver::load()?.validate(context, codec, format)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn supported() -> DecodeCaps {
        DecodeCaps {
            supported: 1,
            output_format_mask: 1,
            min_width: 48,
            min_height: 16,
            max_width: 4096,
            max_height: 4096,
            max_macroblocks: 32768,
            ..DecodeCaps::query(VideoCodec::H264)
        }
    }

    #[test]
    fn queries_exact_supported_codec_profiles() {
        for (codec, value) in [
            (VideoCodec::H264, 4),
            (VideoCodec::H265, 8),
            (VideoCodec::Av1, 11),
        ] {
            let caps = DecodeCaps::query(codec);
            assert_eq!(caps.codec, value);
            assert_eq!(caps.chroma, 1);
            assert_eq!(caps.bit_depth_minus_eight, 0);
            assert_eq!(caps.reserved1, [0; 3]);
            assert_eq!(caps.reserved3, [0; 9]);
            assert_eq!(caps.reserved4, [0; 3]);
        }
    }

    #[test]
    fn rejects_unsupported_av1_even_when_cuda_device_exists() {
        let caps = DecodeCaps::query(VideoCodec::Av1);
        let error = caps
            .validate(
                VideoCodec::Av1,
                StreamFormat::video_default(1920, 1080).unwrap(),
            )
            .unwrap_err();
        assert!(error.contains("does not support av1"), "{error}");
    }

    #[test]
    fn requires_nv12_output() {
        let mut caps = supported();
        let format = StreamFormat::video_default(1920, 1080).unwrap();
        assert!(caps.validate(VideoCodec::H264, format).is_ok());
        caps.output_format_mask = 2;
        assert!(caps.validate(VideoCodec::H264, format).is_err());
    }

    #[test]
    fn enforces_dimensions_and_rounded_macroblock_limit() {
        let mut caps = supported();
        for (width, height) in [
            (32, 1080),
            (1920, 8),
            (4112, 1080),
            (1920, 4112),
            (4096, 4096),
        ] {
            assert!(
                caps.validate(
                    VideoCodec::H264,
                    StreamFormat::video_default(width, height).unwrap()
                )
                .is_err()
            );
        }
        caps.max_macroblocks = 120 * 68;
        assert!(
            caps.validate(
                VideoCodec::H264,
                StreamFormat::video_default(1920, 1080).unwrap()
            )
            .is_ok()
        );
        caps.max_macroblocks -= 1;
        assert!(
            caps.validate(
                VideoCodec::H264,
                StreamFormat::video_default(1920, 1080).unwrap()
            )
            .is_err()
        );
    }

    #[test]
    #[ignore = "requires the CUDA mock driver from tests/run_cuda_mock.py"]
    fn mocked_driver_checks_caps_on_the_selected_context() {
        let driver = CudaDriver::load().unwrap();
        let context = std::ptr::without_provenance_mut(0x1234);
        let format = StreamFormat::video_default(1920, 1080).unwrap();
        driver.validate(context, VideoCodec::H264, format).unwrap();
        assert!(
            driver
                .validate(context, VideoCodec::Av1, format)
                .unwrap_err()
                .contains("does not support av1")
        );
        assert!(
            driver
                .validate(context, VideoCodec::H265, format)
                .unwrap_err()
                .contains("CUDA status 801")
        );
        let depth = unsafe {
            driver
                ._cuda
                .get::<unsafe extern "C" fn() -> c_int>(b"mock_context_depth\0")
        }
        .unwrap();
        assert_eq!(unsafe { depth() }, 0);
    }
}
