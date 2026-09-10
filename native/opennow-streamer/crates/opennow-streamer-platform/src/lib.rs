mod audio_playout;
mod embedded_input;
mod graphics;
mod h264;
#[cfg(target_os = "linux")]
mod linux_backend;
#[cfg(target_os = "linux")]
mod linux_frame_pacing;
#[cfg(target_os = "linux")]
mod linux_xinput;
#[cfg(target_os = "macos")]
mod macos_backend;
mod media;
mod microphone;
mod native_surface;
mod output;
mod queue;
mod recording;
mod replay;
mod runtime;
mod video_queue;
#[cfg(target_os = "windows")]
mod windows_graphics;
#[cfg(target_os = "windows")]
mod windows_raw_input;

pub use embedded_input::{EmbeddedInputCapture, EmbeddedLocalAction};
pub use graphics::{
    GraphicsApi, GraphicsColorSpace, GraphicsContext, GraphicsContextLease, GraphicsFrame,
    GraphicsFrameError, GraphicsFrameInfo, GraphicsFramePublisher, GraphicsFrameToken,
    GraphicsPublishOutcome, GraphicsRecordCommand, GraphicsRecordedFrame, GraphicsRenderResources,
    GraphicsRuntimeError, GraphicsTextureFormat, RenderThreadGraphics,
};
pub use media::{
    CapturedInput, CapturedInputQueue, CapturedInputSample, EncodedFrame, EncodedRecordingReceiver,
    MediaCodec, MediaColorQuality, MediaControl, MediaFeedback, MediaSession, MediaSink,
    MediaStreamConfig, MediaVideoCodec, PushOutcome, ShortcutChord, StreamShortcutAction,
    StreamShortcutBindings,
};
pub use microphone::{
    EncodedMicrophoneFrame, MICROPHONE_FRAME_SAMPLES, MICROPHONE_SAMPLE_RATE, MicrophoneReceiver,
    MicrophoneSession, MicrophoneStatus,
};
#[cfg(target_os = "linux")]
pub use opennow_streamer_platform_linux::{
    LinuxGpuFrame, LinuxGpuFrameProducer, SharedVulkanDevice,
};
#[cfg(not(target_os = "linux"))]
pub enum SharedVulkanDevice {}
#[cfg(target_os = "macos")]
pub use opennow_streamer_platform_macos::{
    AdoptedMetalContext, EmbeddedFrameProducer, MetalFrame, MetalRecordedFrame,
};
pub use opennow_streamer_platform_windows::WindowsAdapterLuid;
#[cfg(target_os = "windows")]
pub use opennow_streamer_platform_windows::{
    AdoptedD3d11Context, D3d11Frame, D3d11FrameProducer, D3d11FrameSubmitter, D3d11RecordedFrame,
    D3d11TextureFormat, d3d11_adapter_luid,
};
pub use recording::{RecordingSummary, record_matroska, record_replay_matroska};
pub use replay::ReplaySnapshot;
pub use runtime::{
    EmbeddedRuntimeConfig, MainThreadHost, MediaRuntime, MediaRuntimeControl,
    create_embedded_runtime, create_embedded_runtime_with_config,
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
            windows_hardware_backend(WindowsGraphicsApi::D3d12, "d3d12", "d3d11on12-nv12", None),
            windows_hardware_backend(WindowsGraphicsApi::D3d11, "d3d11", "d3d11-nv12", None),
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
    embedded_video_backends_with_config(None, None)
}

pub(crate) fn embedded_video_backends_with_config(
    _device: Option<&SharedVulkanDevice>,
    _windows_adapter_luid: Option<WindowsAdapterLuid>,
) -> Vec<VideoBackendCapability> {
    #[cfg(target_os = "linux")]
    let mut backends = linux_backend::video_backends();
    #[cfg(target_os = "linux")]
    for backend in &mut backends {
        if backend.backend == "vulkan" {
            use opennow_streamer_platform_linux::{PixelFormat, VideoCodec};
            for codec in &mut backend.codecs {
                let profile = match codec.codec {
                    "h264" => VideoCodec::H264,
                    "h265" => VideoCodec::H265,
                    _ => VideoCodec::Av1,
                };
                let colors = [
                    ("8bit_420", PixelFormat::Nv12),
                    ("10bit_420", PixelFormat::P010),
                    ("8bit_444", PixelFormat::Nv24),
                    ("10bit_444", PixelFormat::P410),
                ]
                .into_iter()
                .filter(|(color, _)| match codec.codec {
                    "h264" => *color == "8bit_420",
                    "av1" => !color.ends_with("444"),
                    _ => true,
                })
                .filter_map(|(color, format)| {
                    _device
                        .is_some_and(|device| device.codec_format_support(profile, format))
                        .then_some(color)
                })
                .collect::<Vec<_>>();
                codec.available = !colors.is_empty();
                codec.color_qualities = Some(colors);
                codec.reason = (!codec.available)
                    .then_some("the attached Vulkan device does not support this decode profile");
            }
            backend.available = backend.codecs.iter().any(|codec| codec.available);
            backend.reason = (!backend.available)
                .then_some("no compatible shared Vulkan decode device is attached");
            backend.zero_copy_modes.clear();
        } else {
            for codec in &mut backend.codecs {
                let mut colors = if codec.available {
                    vec!["8bit_420"]
                } else {
                    Vec::new()
                };
                if backend.backend == "vaapi" {
                    let profile = match codec.codec {
                        "h265" => opennow_streamer_platform_linux::VideoCodec::H265,
                        "av1" => opennow_streamer_platform_linux::VideoCodec::Av1,
                        _ => opennow_streamer_platform_linux::VideoCodec::H264,
                    };
                    if opennow_streamer_platform_linux::supports_vaapi_ten_bit(profile) {
                        colors.push("10bit_420");
                        codec.available = true;
                        codec.reason = None;
                    }
                }
                codec.color_qualities = Some(colors);
            }
            if backend.backend == "vaapi" {
                backend.available = backend.codecs.iter().any(|codec| codec.available);
                if backend.available {
                    backend.reason = None;
                }
            }
        }
    }
    #[cfg(target_os = "linux")]
    for backend in &mut backends {
        for codec in &mut backend.codecs {
            codec.hdr_color_qualities = Some(
                codec
                    .color_qualities
                    .as_ref()
                    .into_iter()
                    .flatten()
                    .copied()
                    .filter(|color| {
                        backend.available
                            && codec.available
                            && matches!(backend.backend, "vulkan" | "vaapi")
                            && matches!(codec.codec, "h265" | "av1")
                            && color.starts_with("10bit_")
                    })
                    .collect(),
            );
            codec.hdr_supported = Some(
                backend.available
                    && codec.available
                    && matches!(backend.backend, "vulkan" | "vaapi")
                    && matches!(codec.codec, "h265" | "av1")
                    && codec
                        .color_qualities
                        .as_ref()
                        .is_some_and(|colors| colors.contains(&"10bit_420")),
            );
        }
    }
    #[cfg(target_os = "windows")]
    let mut backends = {
        use opennow_streamer_platform_windows::WindowsGraphicsApi;
        vec![windows_hardware_backend(
            WindowsGraphicsApi::D3d11,
            "d3d11",
            "d3d11-nv12",
            _windows_adapter_luid,
        )]
    };
    #[cfg(target_os = "macos")]
    let mut backends = vec![hardware_backend()];
    #[cfg(target_os = "macos")]
    for backend in &mut backends {
        for codec in &mut backend.codecs {
            let mut colors = if !codec.available {
                Vec::new()
            } else if codec.codec == "h264" {
                vec!["8bit_420"]
            } else {
                vec!["8bit_420", "10bit_420"]
            };
            if codec.available
                && codec.codec == "h265"
                && opennow_streamer_platform_macos::probe_h265_444_ten_bit_hardware()
            {
                colors.push("10bit_444");
            }
            codec.hdr_supported = Some(
                codec.available
                    && codec.codec == "h265"
                    && opennow_streamer_platform_macos::probe_h265_hdr_hardware(),
            );
            let mut hdr_colors = Vec::new();
            if codec.hdr_supported == Some(true) {
                hdr_colors.push("10bit_420");
            }
            if codec.available
                && codec.codec == "h265"
                && opennow_streamer_platform_macos::probe_h265_hdr_444_hardware()
            {
                hdr_colors.push("10bit_444");
            }
            codec.hdr_color_qualities = Some(hdr_colors);
            codec.color_qualities = Some(colors);
        }
    }
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
            codec.hdr_supported = Some(false);
            codec.hdr_color_qualities = Some(Vec::new());
            codec.color_qualities = Some(Vec::new());
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
    adapter_luid: Option<WindowsAdapterLuid>,
) -> VideoBackendCapability {
    use opennow_streamer_platform_windows::{VideoCodec, VideoPixelFormat};

    if !runtime::backend_preference_allows(backend) {
        return unavailable_backend(
            backend,
            "windows",
            "Direct3D hardware decode was disabled by configuration",
        );
    }
    let probe = opennow_streamer_platform_windows::WindowsBackend::probe_for(api, adapter_luid);
    let available = probe.bundled_backend_available();
    let media_output_available = probe.d3d11_presentation && probe.wasapi_render;
    let color_qualities = |codec| {
        [
            ("8bit_420", VideoPixelFormat::Nv12),
            ("10bit_420", VideoPixelFormat::P010),
            ("8bit_444", VideoPixelFormat::Ayuv),
            ("10bit_444", VideoPixelFormat::Y410),
        ]
        .into_iter()
        .filter_map(|(color, format)| {
            (media_output_available && probe.supports_format(codec, format, false)).then_some(color)
        })
        .collect::<Vec<_>>()
    };
    let hdr_color_qualities = |codec| {
        [
            ("10bit_420", VideoPixelFormat::P010),
            ("10bit_444", VideoPixelFormat::Y410),
        ]
        .into_iter()
        .filter_map(|(color, format)| {
            (media_output_available && probe.supports_format(codec, format, true)).then_some(color)
        })
        .collect::<Vec<_>>()
    };
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
                hdr_supported: Some(false),
                hdr_color_qualities: Some(hdr_color_qualities(VideoCodec::H264)),
                color_qualities: Some(color_qualities(VideoCodec::H264)),
                codec: "h264",
                available: media_output_available && probe.h264_hardware_decode,
                reason: (!(media_output_available && probe.h264_hardware_decode)).then_some(
                    "H.264 Media Foundation hardware decode or media output is unavailable",
                ),
            },
            CodecCapability {
                hdr_supported: Some(media_output_available && probe.h265_hdr),
                hdr_color_qualities: Some(hdr_color_qualities(VideoCodec::H265)),
                color_qualities: Some(color_qualities(VideoCodec::H265)),
                codec: "h265",
                available: media_output_available && probe.h265_hardware_decode,
                reason: (!(media_output_available && probe.h265_hardware_decode)).then_some(
                    "H.265 Media Foundation hardware decode or media output is unavailable",
                ),
            },
            CodecCapability {
                hdr_supported: Some(media_output_available && probe.av1_hdr),
                hdr_color_qualities: Some(hdr_color_qualities(VideoCodec::Av1)),
                color_qualities: Some(color_qualities(VideoCodec::Av1)),
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
                hdr_supported: Some(false),
                hdr_color_qualities: None,
                color_qualities: Some(vec!["8bit_420"]),
                codec: "h264",
                available: h264_available,
                reason: (!h264_available)
                    .then_some("H.264 VideoToolbox hardware decode or Metal is unavailable"),
            },
            CodecCapability {
                hdr_supported: Some(false),
                hdr_color_qualities: None,
                color_qualities: Some(vec!["8bit_420", "10bit_420"]),
                codec: "h265",
                available: h265_available,
                reason: (!h265_available)
                    .then_some("H.265 VideoToolbox hardware decode or Metal is unavailable"),
            },
            CodecCapability {
                hdr_supported: Some(false),
                hdr_color_qualities: None,
                color_qualities: Some(vec!["8bit_420", "10bit_420"]),
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
            None,
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
                    hdr_supported: None,
                    hdr_color_qualities: None,
                    color_qualities: None,
                    codec: "h264",
                    available: true,
                    reason: None,
                },
                CodecCapability {
                    hdr_supported: None,
                    hdr_color_qualities: None,
                    color_qualities: None,
                    codec: "h265",
                    available: media_output_available && probe.h265_software_decode,
                    reason: (!(media_output_available && probe.h265_software_decode)).then_some(
                        "H.265 Media Foundation software decode or media output is unavailable",
                    ),
                },
                CodecCapability {
                    hdr_supported: None,
                    hdr_color_qualities: None,
                    color_qualities: None,
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
                hdr_supported: None,
                hdr_color_qualities: None,
                color_qualities: None,
                codec: "h264",
                available: true,
                reason: None,
            },
            CodecCapability {
                hdr_supported: None,
                hdr_color_qualities: None,
                color_qualities: None,
                codec: "h265",
                available: false,
                reason: Some("H.265 decoder is not built into this binary"),
            },
            CodecCapability {
                hdr_supported: None,
                hdr_color_qualities: None,
                color_qualities: None,
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
                hdr_supported: None,
                hdr_color_qualities: None,
                color_qualities: None,
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
    fn disabled_backends_do_not_retain_hdr_or_color_profiles() {
        let mut backends = vec![VideoBackendCapability {
            backend: "vulkan",
            platform: "linux",
            codecs: vec![CodecCapability {
                codec: "h265",
                available: true,
                hdr_supported: Some(true),
                hdr_color_qualities: None,
                color_qualities: Some(vec!["8bit_420", "10bit_420"]),
                reason: None,
            }],
            zero_copy_modes: vec!["vulkan"],
            available: true,
            reason: None,
        }];
        apply_backend_policy(&mut backends, Some("software"));
        assert!(!backends[0].available);
        assert!(backends[0].zero_copy_modes.is_empty());
        assert!(!backends[0].codecs[0].available);
        assert_eq!(backends[0].codecs[0].hdr_supported, Some(false));
        assert_eq!(backends[0].codecs[0].color_qualities, Some(Vec::new()));
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
                None,
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
                let probe = opennow_streamer_platform_windows::WindowsBackend::probe_for(api, None);
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
        #[cfg(target_os = "linux")]
        {
            let vulkan = backends
                .iter()
                .find(|backend| backend.backend == "vulkan")
                .unwrap();
            assert!(!vulkan.available);
            assert!(vulkan.codecs.iter().all(|codec| !codec.available));
            assert!(vulkan.zero_copy_modes.is_empty());
        }
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

    #[cfg(target_os = "linux")]
    #[test]
    fn embedded_linux_hdr_requires_an_available_ten_bit_output_path() {
        for backend in embedded_video_backends_with_config(None, None) {
            for codec in backend.codecs {
                assert_eq!(
                    codec.hdr_supported,
                    Some(
                        backend.available
                            && codec.available
                            && matches!(backend.backend, "vulkan" | "vaapi")
                            && matches!(codec.codec, "h265" | "av1")
                            && codec
                                .color_qualities
                                .as_ref()
                                .is_some_and(|colors| colors.contains(&"10bit_420"))
                    )
                );
            }
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn embedded_windows_advertises_only_probed_color_profiles() {
        use opennow_streamer_platform_windows::{
            VideoCodec, VideoPixelFormat, WindowsBackend, WindowsGraphicsApi,
        };
        let probe = WindowsBackend::probe_for(WindowsGraphicsApi::D3d11, None);
        for backend in embedded_video_backends() {
            for codec in backend.codecs {
                let profile = match codec.codec {
                    "h264" => VideoCodec::H264,
                    "h265" => VideoCodec::H265,
                    _ => VideoCodec::Av1,
                };
                let colors = codec.color_qualities.expect("explicit Windows profiles");
                for (color, format) in [
                    ("8bit_420", VideoPixelFormat::Nv12),
                    ("10bit_420", VideoPixelFormat::P010),
                    ("8bit_444", VideoPixelFormat::Ayuv),
                    ("10bit_444", VideoPixelFormat::Y410),
                ] {
                    assert_eq!(
                        colors.contains(&color),
                        codec.available && probe.supports_format(profile, format, false),
                        "{} {color}",
                        codec.codec
                    );
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn embedded_linux_reports_explicit_color_profiles_without_claiming_zero_copy() {
        for backend in embedded_video_backends_with_config(None, None) {
            for codec in backend.codecs {
                let colors = codec
                    .color_qualities
                    .expect("embedded Linux color profiles");
                if backend.backend == "vulkan" {
                    assert!(colors.is_empty());
                    assert!(backend.zero_copy_modes.is_empty());
                } else {
                    assert!(colors.iter().all(|color| *color == "8bit_420"));
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn embedded_macos_reports_only_biplanar_color_profiles() {
        for backend in embedded_video_backends() {
            for codec in backend.codecs {
                let colors = codec
                    .color_qualities
                    .expect("embedded macOS color profiles");
                assert_eq!(
                    codec.hdr_supported,
                    Some(
                        codec.available
                            && codec.codec == "h265"
                            && opennow_streamer_platform_macos::probe_h265_hdr_hardware()
                    )
                );
                if !codec.available {
                    assert!(colors.is_empty());
                } else if codec.codec == "h264" {
                    assert_eq!(colors, ["8bit_420"]);
                } else {
                    let mut expected = vec!["8bit_420", "10bit_420"];
                    if codec.codec == "h265"
                        && opennow_streamer_platform_macos::probe_h265_444_ten_bit_hardware()
                    {
                        expected.push("10bit_444");
                    }
                    assert_eq!(colors, expected);
                }
            }
        }
    }
}
