mod embedded_input;
mod graphics;
#[cfg(target_os = "linux")]
mod linux_backend;
#[cfg(target_os = "linux")]
mod linux_frame_pacing;
#[cfg(target_os = "linux")]
mod linux_xinput;
#[cfg(target_os = "macos")]
mod macos_backend;
mod media;
mod native_surface;
mod output;
mod queue;
mod recording;
mod runtime;
#[cfg(target_os = "windows")]
mod windows_graphics;
#[cfg(target_os = "windows")]
mod windows_raw_input;

pub use embedded_input::{EmbeddedInputCapture, EmbeddedLocalAction};
pub use graphics::{
    GraphicsApi, GraphicsContext, GraphicsContextLease, GraphicsFrame, GraphicsFrameInfo,
    GraphicsFramePublisher, GraphicsFrameToken, GraphicsPublishOutcome, GraphicsRecordCommand,
    GraphicsRecordedFrame, GraphicsRuntimeError, GraphicsTextureFormat, RenderThreadGraphics,
};
pub use media::{
    CapturedInput, CapturedInputQueue, CapturedInputSample, EncodedFrame, EncodedRecordingReceiver,
    MediaCodec, MediaColorQuality, MediaControl, MediaFeedback, MediaSession, MediaSink,
    MediaStreamConfig, MediaVideoCodec, PushOutcome, ShortcutChord, StreamShortcutAction,
    StreamShortcutBindings,
};
#[cfg(target_os = "linux")]
pub use opennow_streamer_platform_linux::{LinuxGpuFrame, LinuxGpuFrameProducer};
#[cfg(target_os = "macos")]
pub use opennow_streamer_platform_macos::{
    AdoptedMetalContext, EmbeddedFrameProducer, MetalFrame, MetalRecordedFrame,
};
#[cfg(target_os = "windows")]
pub use opennow_streamer_platform_windows::{
    AdoptedD3d11Context, D3d11Frame, D3d11FrameProducer, D3d11FrameSubmitter, D3d11RecordedFrame,
    D3d11TextureFormat,
};
pub use recording::{RecordingSummary, record_matroska};
pub use runtime::{
    MainThreadHost, MediaRuntime, MediaRuntimeControl, create_embedded_runtime,
    create_embedded_runtime_with_input, create_runtime,
};
#[cfg(feature = "test-runtime")]
pub use runtime::{TestMediaRuntimeHost, create_test_runtime};

use opennow_streamer_protocol::{CodecCapability, VideoBackendCapability};

pub fn video_backends() -> Vec<VideoBackendCapability> {
    #[cfg(target_os = "linux")]
    let mut backends = {
        let mut backends = linux_backend::video_backends();
        backends.push(software_backend());
        backends
    };
    #[cfg(target_os = "windows")]
    let mut backends = {
        use opennow_streamer_platform_windows::WindowsGraphicsApi;
        vec![
            windows_hardware_backend(WindowsGraphicsApi::D3d12, "d3d12", "d3d11on12-nv12"),
            windows_hardware_backend(WindowsGraphicsApi::D3d11, "d3d11", "d3d11-nv12"),
            software_backend(),
        ]
    };
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    let mut backends = vec![hardware_backend(), software_backend()];
    apply_backend_policy(
        &mut backends,
        std::env::var("OPENNOW_NATIVE_VIDEO_BACKEND")
            .ok()
            .as_deref(),
    );
    backends
}

pub fn embedded_video_backends() -> Vec<VideoBackendCapability> {
    #[cfg(target_os = "linux")]
    let mut backends = linux_backend::video_backends();
    #[cfg(target_os = "windows")]
    let mut backends = {
        use opennow_streamer_platform_windows::WindowsGraphicsApi;
        vec![windows_hardware_backend(
            WindowsGraphicsApi::D3d11,
            "d3d11",
            "d3d11-nv12",
        )]
    };
    #[cfg(target_os = "macos")]
    let mut backends = vec![hardware_backend()];
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    let mut backends = Vec::new();
    apply_backend_policy(
        &mut backends,
        std::env::var("OPENNOW_NATIVE_VIDEO_BACKEND")
            .ok()
            .as_deref(),
    );
    backends
}

fn apply_backend_policy(backends: &mut [VideoBackendCapability], requested: Option<&str>) {
    let requested = requested
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("auto")
        .to_ascii_lowercase();
    if requested == "auto" {
        return;
    }
    for backend in backends {
        if backend_allowed_by_policy(&requested, backend.backend) {
            continue;
        }
        backend.available = false;
        backend.reason = Some("video backend was disabled by decoder policy");
        backend.zero_copy_modes.clear();
        for codec in &mut backend.codecs {
            codec.available = false;
            codec.reason = Some("video backend was disabled by decoder policy");
        }
    }
}

fn backend_allowed_by_policy(requested: &str, backend: &str) -> bool {
    match requested {
        "auto" | "" => true,
        "software" => matches!(backend, "software" | "ffmpeg"),
        "hardware" => !matches!(backend, "software" | "ffmpeg"),
        "nvdec" => backend == "cuda",
        explicit => backend == explicit,
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
    const UNAVAILABLE: &str = "VideoToolbox hardware decode or Metal is unavailable";
    let available = macos_backend::available();
    let h264_available = macos_backend::h264_available();
    let h265_available = macos_backend::h265_available();
    let av1_available = macos_backend::av1_available();
    VideoBackendCapability {
        backend: "videotoolbox",
        platform: "macos",
        codecs: vec![
            CodecCapability {
                codec: "h264",
                available: h264_available,
                reason: (!h264_available)
                    .then_some("H.264 VideoToolbox hardware decode or Metal is unavailable"),
            },
            CodecCapability {
                codec: "h265",
                available: h265_available,
                reason: (!h265_available)
                    .then_some("H.265 VideoToolbox hardware decode or Metal is unavailable"),
            },
            CodecCapability {
                codec: "av1",
                available: av1_available,
                reason: (!av1_available)
                    .then_some("AV1 VideoToolbox hardware decode or Metal is unavailable"),
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
    #[cfg(target_os = "windows")]
    {
        let probe = opennow_streamer_platform_windows::WindowsBackend::probe_for(
            opennow_streamer_platform_windows::WindowsGraphicsApi::D3d11,
        );
        let media_output_available = probe.d3d11_presentation && probe.wasapi_render;
        // OpenH264 is bundled for the guaranteed H.264 path. HEVC and AV1 can
        // additionally use a D3D11-aware synchronous Media Foundation decoder.
        let available = true;
        VideoBackendCapability {
            backend: "software",
            platform: "windows",
            codecs: vec![
                CodecCapability {
                    codec: "h264",
                    available: true,
                    reason: None,
                },
                CodecCapability {
                    codec: "h265",
                    available: media_output_available && probe.h265_software_decode,
                    reason: (!(media_output_available && probe.h265_software_decode)).then_some(
                        "H.265 Media Foundation software decode or media output is unavailable",
                    ),
                },
                CodecCapability {
                    codec: "av1",
                    available: media_output_available && probe.av1_software_decode,
                    reason: (!(media_output_available && probe.av1_software_decode)).then_some(
                        "AV1 Media Foundation software decode or media output is unavailable",
                    ),
                },
            ],
            zero_copy_modes: Vec::new(),
            available,
            reason: None,
        }
    }
    #[cfg(not(target_os = "windows"))]
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
    fn decoder_policy_separates_hardware_software_and_explicit_backends() {
        assert!(backend_allowed_by_policy("auto", "software"));
        assert!(backend_allowed_by_policy("software", "ffmpeg"));
        assert!(backend_allowed_by_policy("software", "software"));
        assert!(!backend_allowed_by_policy("software", "vulkan"));
        assert!(backend_allowed_by_policy("hardware", "d3d11"));
        assert!(!backend_allowed_by_policy("hardware", "software"));
        assert!(!backend_allowed_by_policy("hardware", "ffmpeg"));
        assert!(backend_allowed_by_policy("nvdec", "cuda"));
        assert!(!backend_allowed_by_policy("nvdec", "vulkan"));
        assert!(backend_allowed_by_policy("videotoolbox", "videotoolbox"));
    }

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
        #[cfg(not(target_os = "windows"))]
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
            let software_probe = opennow_streamer_platform_windows::WindowsBackend::probe_for(
                WindowsGraphicsApi::D3d11,
            );
            assert_eq!(
                software
                    .codecs
                    .iter()
                    .find(|codec| codec.codec == "h265")
                    .expect("h265")
                    .available,
                software_probe.h265_software_decode
                    && software_probe.d3d11_presentation
                    && software_probe.wasapi_render,
            );
            assert_eq!(
                software
                    .codecs
                    .iter()
                    .find(|codec| codec.codec == "av1")
                    .expect("av1")
                    .available,
                software_probe.av1_software_decode
                    && software_probe.d3d11_presentation
                    && software_probe.wasapi_render,
            );
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
            for (codec, expected) in [
                ("h264", macos_backend::h264_available()),
                ("h265", macos_backend::h265_available()),
                ("av1", macos_backend::av1_available()),
            ] {
                assert_eq!(
                    hardware
                        .codecs
                        .iter()
                        .find(|capability| capability.codec == codec)
                        .expect(codec)
                        .available,
                    expected,
                );
            }
            assert_eq!(hardware.available, macos_backend::available());
            assert_eq!(
                hardware.available,
                hardware
                    .codecs
                    .iter()
                    .any(|capability| capability.available)
            );
        }
    }

    #[test]
    fn embedded_capabilities_exclude_standalone_only_backends() {
        let backends = embedded_video_backends();
        assert!(backends.iter().all(|backend| backend.backend != "software"));
        #[cfg(target_os = "windows")]
        assert_eq!(
            backends
                .iter()
                .map(|backend| backend.backend)
                .collect::<Vec<_>>(),
            vec!["d3d11"]
        );
        #[cfg(target_os = "macos")]
        assert_eq!(
            backends
                .iter()
                .map(|backend| backend.backend)
                .collect::<Vec<_>>(),
            vec!["videotoolbox"]
        );
    }
}
