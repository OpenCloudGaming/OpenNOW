#[cfg(target_os = "macos")]
mod macos_backend;
mod media;
mod native_surface;
mod output;
mod queue;
mod runtime;

pub use media::{EncodedFrame, MediaCodec, MediaFeedback, MediaSession, MediaSink, PushOutcome};
pub use runtime::{MainThreadHost, MediaRuntime, create_runtime};

/// Shows a standalone overlay window through the exact production creation path.
/// Debug-only aid for isolating window-server behavior without a streaming session.
#[cfg(target_os = "macos")]
pub fn debug_show_overlay_window() {
    opennow_streamer_platform_macos::debug_show_overlay_window();
}

use opennow_streamer_protocol::{CodecCapability, VideoBackendCapability};

pub fn video_backends() -> Vec<VideoBackendCapability> {
    vec![hardware_backend(), software_backend()]
}

pub const fn supports_audio_decode() -> bool {
    true
}

pub const fn supports_audio_output() -> bool {
    true
}

#[cfg(target_os = "windows")]
fn hardware_backend() -> VideoBackendCapability {
    unavailable_backend(
        "d3d11",
        "windows",
        "D3D11 hardware decode is not built into this binary",
    )
}

#[cfg(target_os = "macos")]
fn hardware_backend() -> VideoBackendCapability {
    const UNAVAILABLE: &str = "VideoToolbox H.264 hardware decode or Metal is unavailable";
    let available = macos_backend::available();
    VideoBackendCapability {
        backend: "videotoolbox",
        platform: "macos",
        codecs: vec![
            CodecCapability {
                codec: "h264",
                available,
                reason: (!available).then_some(UNAVAILABLE),
            },
            CodecCapability {
                codec: "h265",
                available: false,
                reason: Some("H.265 VideoToolbox decode is not implemented"),
            },
            CodecCapability {
                codec: "av1",
                available: false,
                reason: Some("AV1 VideoToolbox decode is not implemented"),
            },
        ],
        zero_copy_modes: available
            .then_some(vec!["cvpixelbuffer-iosurface-metal"])
            .unwrap_or_default(),
        available,
        reason: (!available).then_some(UNAVAILABLE),
    }
}

#[cfg(target_os = "linux")]
fn hardware_backend() -> VideoBackendCapability {
    unavailable_backend(
        "vaapi",
        "linux",
        "VA-API/V4L2 hardware decode is not built into this binary",
    )
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn hardware_backend() -> VideoBackendCapability {
    unavailable_backend("unsupported", "other", "Unsupported operating system")
}

fn software_backend() -> VideoBackendCapability {
    VideoBackendCapability {
        backend: "software",
        platform: "cross-platform",
        codecs: vec![
            CodecCapability {
                codec: "h264",
                available: true,
                reason: None,
            },
            CodecCapability {
                codec: "h265",
                available: false,
                reason: Some("H.265 decoder is not built into this binary"),
            },
            CodecCapability {
                codec: "av1",
                available: false,
                reason: Some("AV1 decoder is not built into this binary"),
            },
        ],
        zero_copy_modes: Vec::new(),
        available: true,
        reason: None,
    }
}

#[cfg(not(target_os = "macos"))]
fn unavailable_backend(
    backend: &'static str,
    platform: &'static str,
    reason: &'static str,
) -> VideoBackendCapability {
    VideoBackendCapability {
        backend,
        platform,
        codecs: ["h264", "h265", "av1"]
            .into_iter()
            .map(|codec| CodecCapability {
                codec,
                available: false,
                reason: Some(reason),
            })
            .collect(),
        zero_copy_modes: Vec::new(),
        available: false,
        reason: Some(reason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advertises_only_the_linked_software_codec() {
        let backends = video_backends();
        let software = backends
            .iter()
            .find(|backend| backend.backend == "software")
            .expect("software backend");
        assert!(software.available);
        assert!(
            software
                .codecs
                .iter()
                .find(|codec| codec.codec == "h264")
                .expect("h264")
                .available
        );
        assert!(
            software
                .codecs
                .iter()
                .filter(|codec| codec.codec != "h264")
                .all(|codec| !codec.available)
        );
        #[cfg(not(target_os = "macos"))]
        assert!(
            backends
                .iter()
                .filter(|backend| backend.backend != "software")
                .all(|backend| !backend.available)
        );
        #[cfg(target_os = "macos")]
        {
            let hardware = backends
                .iter()
                .find(|backend| backend.backend == "videotoolbox")
                .expect("VideoToolbox backend");
            assert!(
                hardware
                    .codecs
                    .iter()
                    .filter(|codec| codec.codec != "h264")
                    .all(|codec| !codec.available)
            );
            assert_eq!(
                hardware.available,
                hardware
                    .codecs
                    .iter()
                    .find(|codec| codec.codec == "h264")
                    .expect("h264")
                    .available
            );
        }
    }
}
