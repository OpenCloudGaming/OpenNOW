#[cfg(target_os = "linux")]
mod linux_backend;
#[cfg(target_os = "linux")]
mod linux_frame_pacing;
#[cfg(target_os = "linux")]
mod linux_xinput;
#[cfg(target_os = "macos")]
mod macos_backend;
mod media;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod native_stats_overlay;
mod native_surface;
mod output;
mod queue;
mod runtime;
#[cfg(target_os = "windows")]
mod windows_debug_overlay;
#[cfg(target_os = "windows")]
mod windows_raw_input;

pub use media::{
    CapturedInput, CapturedInputQueue, CapturedInputSample, EncodedFrame, MediaCodec, MediaControl,
    MediaFeedback, MediaSession, MediaSink, MediaStreamConfig, MediaVideoCodec, PushOutcome,
};
pub use runtime::{MainThreadHost, MediaRuntime, create_runtime};

/// Shows a standalone overlay window through the exact production creation path.
/// Debug-only aid for isolating window-server behavior without a streaming session.
#[cfg(target_os = "macos")]
pub fn debug_show_overlay_window() {
    opennow_streamer_platform_macos::debug_show_overlay_window();
}

use opennow_streamer_protocol::{CodecCapability, VideoBackendCapability};

pub fn video_backends() -> Vec<VideoBackendCapability> {
    #[cfg(target_os = "linux")]
    {
        let mut backends = linux_backend::video_backends();
        backends.push(software_backend());
        backends
    }
    #[cfg(not(target_os = "linux"))]
    {
        #[cfg(target_os = "windows")]
        {
            use opennow_streamer_platform_windows::WindowsGraphicsApi;
            vec![
                windows_hardware_backend(WindowsGraphicsApi::D3d12, "d3d12", "d3d11on12-nv12"),
                windows_hardware_backend(WindowsGraphicsApi::D3d11, "d3d11", "d3d11-nv12"),
                software_backend(),
            ]
        }
        #[cfg(not(target_os = "windows"))]
        {
            vec![hardware_backend(), software_backend()]
        }
    }
}

pub const fn supports_audio_decode() -> bool {
    true
}

pub const fn supports_audio_output() -> bool {
    true
}

#[cfg(target_os = "windows")]
fn windows_hardware_backend(
    api: opennow_streamer_platform_windows::WindowsGraphicsApi,
    backend: &'static str,
    zero_copy_mode: &'static str,
) -> VideoBackendCapability {
    if !runtime::backend_preference_allows(backend) {
        return unavailable_backend(
            backend,
            "windows",
            "Direct3D hardware decode was disabled by configuration",
        );
    }
    let probe = opennow_streamer_platform_windows::WindowsBackend::probe_for(api);
    let available = probe.bundled_backend_available();
    let media_output_available = probe.d3d11_presentation && probe.wasapi_render;
    let reason = if available {
        None
    } else {
        Some("Direct3D hardware decode, presentation, or WASAPI output is unavailable")
    };
    VideoBackendCapability {
        backend,
        platform: "windows",
        codecs: vec![
            CodecCapability {
                codec: "h264",
                available: media_output_available && probe.h264_hardware_decode,
                reason: (!(media_output_available && probe.h264_hardware_decode)).then_some(
                    "H.264 Media Foundation hardware decode or media output is unavailable",
                ),
            },
            CodecCapability {
                codec: "h265",
                available: media_output_available && probe.h265_hardware_decode,
                reason: (!(media_output_available && probe.h265_hardware_decode)).then_some(
                    "H.265 Media Foundation hardware decode or media output is unavailable",
                ),
            },
            CodecCapability {
                codec: "av1",
                available: media_output_available && probe.av1_hardware_decode,
                reason: (!(media_output_available && probe.av1_hardware_decode)).then_some(
                    "AV1 Media Foundation hardware decode or media output is unavailable",
                ),
            },
        ],
        zero_copy_modes: available
            .then_some(vec![zero_copy_mode])
            .unwrap_or_default(),
        available,
        reason,
    }
}

#[cfg(target_os = "macos")]
fn hardware_backend() -> VideoBackendCapability {
    if !runtime::backend_preference_allows("videotoolbox") {
        return unavailable_backend(
            "videotoolbox",
            "macos",
            "VideoToolbox hardware decode was disabled by configuration",
        );
    }
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

#[cfg(not(target_os = "linux"))]
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
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        assert!(
            backends
                .iter()
                .filter(|backend| backend.backend != "software")
                .all(|backend| !backend.available)
        );
        #[cfg(target_os = "windows")]
        {
            use opennow_streamer_platform_windows::WindowsGraphicsApi;
            for (backend_name, api) in [
                ("d3d12", WindowsGraphicsApi::D3d12),
                ("d3d11", WindowsGraphicsApi::D3d11),
            ] {
                let hardware = backends
                    .iter()
                    .find(|backend| backend.backend == backend_name)
                    .expect("Direct3D backend");
                let probe = opennow_streamer_platform_windows::WindowsBackend::probe_for(api);
                assert_eq!(hardware.available, probe.bundled_backend_available());
                assert_eq!(
                    hardware
                        .codecs
                        .iter()
                        .find(|codec| codec.codec == "h265")
                        .expect("h265")
                        .available,
                    probe.h265_hardware_decode && probe.d3d11_presentation && probe.wasapi_render,
                );
                assert_eq!(
                    hardware
                        .codecs
                        .iter()
                        .find(|codec| codec.codec == "av1")
                        .expect("av1")
                        .available,
                    probe.av1_hardware_decode && probe.d3d11_presentation && probe.wasapi_render,
                );
            }
        }
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
