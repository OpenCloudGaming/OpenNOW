#![cfg(target_os = "linux")]

use opennow_streamer_platform_linux::{
    AudioConfig, AudioPacket, EncodedVideoFrame, LinuxSession, SessionConfig, StreamFormat,
};
use static_assertions::assert_impl_all;
#[cfg(feature = "vulkan")]
use static_assertions::assert_not_impl_any;
use std::sync::Arc;

assert_impl_all!(LinuxSession: Send);
#[cfg(feature = "vulkan")]
assert_impl_all!(opennow_streamer_platform_linux::LinuxGpuFrame: Send, Sync);
#[cfg(feature = "vulkan")]
assert_impl_all!(opennow_streamer_platform_linux::LinuxGpuFrameProducer: Send, Sync);
#[cfg(feature = "vulkan")]
assert_not_impl_any!(opennow_streamer_platform_linux::NativeSurface<'static>: Send, Sync);

#[test]
fn public_media_inputs_are_typed_and_validated() {
    let format = StreamFormat::h264_default(1920, 1080).unwrap();
    let config = SessionConfig::new(format);
    assert_eq!(config.stream_format, format);
    assert!(EncodedVideoFrame::new(Arc::<[u8]>::from([]), 0, true).is_err());
    assert!(AudioPacket::new(Arc::<[u8]>::from([]), 0).is_err());
    assert!(AudioConfig::default().validate().is_ok());
}

#[test]
fn linux_compile_target_is_supported() {
    assert!(matches!(std::env::consts::ARCH, "x86_64" | "aarch64"));
    let _: fn() -> opennow_streamer_platform_linux::CapabilityReport =
        opennow_streamer_platform_linux::probe_capabilities;
}

#[cfg(feature = "vulkan")]
#[test]
fn embedded_gpu_producer_bounds_qrhi_frame_slots() {
    use opennow_streamer_platform_linux::LinuxGpuFrameProducer;

    assert!(LinuxGpuFrameProducer::new(1).is_ok());
    assert!(LinuxGpuFrameProducer::new(8).is_ok());
    assert!(LinuxGpuFrameProducer::new(0).is_err());
    assert!(LinuxGpuFrameProducer::new(9).is_err());
}

#[cfg(feature = "vulkan")]
#[test]
fn embedded_gpu_frames_keep_sequence_and_presentation_time() {
    use opennow_streamer_platform_linux::{
        ChromaLocation, ColorMatrix, ColorRange, DecodedVideoFrame, FramePlane,
        LinuxGpuFrameProducer, PixelFormat,
    };

    let producer = LinuxGpuFrameProducer::new(3).unwrap();
    let decoded = DecodedVideoFrame {
        format: StreamFormat {
            width: 4,
            height: 4,
            pixel_format: PixelFormat::Nv12,
            color_range: ColorRange::Limited,
            color_matrix: ColorMatrix::Bt709,
            chroma_location: ChromaLocation::Left,
        },
        planes: vec![
            FramePlane {
                data: Arc::from(vec![16_u8; 16]),
                stride: 4,
                rows: 4,
            },
            FramePlane {
                data: Arc::from(vec![128_u8; 8]),
                stride: 4,
                rows: 2,
            },
        ],
        dmabuf: None,
        vulkan: None,
        timestamp_us: 42,
    };
    let first = producer.frame(decoded.clone()).unwrap();
    let second = producer.frame(decoded).unwrap();

    assert_eq!(first.sequence(), 1);
    assert_eq!(second.sequence(), 2);
    assert_eq!(first.presentation_time_ns(), 42_000);
    assert_eq!((first.width(), first.height()), (4, 4));
}
