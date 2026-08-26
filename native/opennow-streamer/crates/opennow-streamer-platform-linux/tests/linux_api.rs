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
