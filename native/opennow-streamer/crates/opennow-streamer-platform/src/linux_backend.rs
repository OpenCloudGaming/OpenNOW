use std::sync::OnceLock;

use opennow_streamer_platform_linux::{
    BackendCapability, DecoderPreference, PresentationCapability, probe_video_capabilities,
};
use opennow_streamer_protocol::{CodecCapability, VideoBackendCapability};

const VIDEO_BACKEND_ENV: &str = "OPENNOW_NATIVE_VIDEO_BACKEND";
const WINDOW_SYSTEM_ENV: &str = "OPENNOW_NATIVE_WINDOW_SYSTEM";
const CODECS: [&str; 3] = ["h264", "h265", "av1"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LinuxVideoPath {
    Hardware(DecoderPreference),
    Software,
}

#[derive(Debug, Clone)]
pub(crate) struct LinuxVideoSelection {
    pub(crate) path: LinuxVideoPath,
    pub(crate) use_vulkan_output: bool,
    pub(crate) fallback_reason: Option<String>,
}

#[derive(Debug, Clone)]
struct LinuxCapabilitySnapshot {
    decoders: Vec<BackendCapability>,
    presentation: PresentationCapability,
}

impl LinuxCapabilitySnapshot {
    fn probe() -> Self {
        let (decoders, presentation) = probe_video_capabilities();
        Self {
            decoders,
            presentation,
        }
    }

    fn decoder(&self, name: &str) -> BackendCapability {
        self.decoders
            .iter()
            .find(|capability| capability.name == name)
            .cloned()
            .unwrap_or(BackendCapability {
                name: "missing-decoder-probe",
                available: false,
                detail: format!("capability probe did not return {name}"),
            })
    }

    fn codec_decoder(&self, prefix: &str, codec: &str) -> BackendCapability {
        self.decoder(&format!("{prefix}-{codec}"))
    }

    fn backend_supports_all_codecs(&self, prefix: &str) -> bool {
        CODECS
            .iter()
            .all(|codec| self.codec_decoder(prefix, codec).available)
    }

    fn presentation_available(&self, window_system: &str) -> bool {
        presentation_available(&self.presentation, window_system)
    }
}

fn presentation_available(presentation: &PresentationCapability, window_system: &str) -> bool {
    presentation.available && presentation.window_systems.contains(&window_system)
}

fn presentation_unavailable_reason(
    presentation: &PresentationCapability,
    window_system: &str,
) -> String {
    if !presentation.available {
        return presentation.detail.clone();
    }
    format!(
        "Vulkan presentation lacks {window_system} WSI required by the active Linux window system"
    )
}

fn selected_window_system() -> &'static str {
    selected_window_system_for(
        std::env::var(WINDOW_SYSTEM_ENV).ok().as_deref(),
        std::env::var("SDL_VIDEODRIVER").ok().as_deref(),
        std::env::var_os("WAYLAND_DISPLAY").is_some(),
        std::env::var_os("DISPLAY").is_some(),
    )
}

fn selected_window_system_for(
    requested: Option<&str>,
    sdl_driver: Option<&str>,
    has_wayland_display: bool,
    has_x11_display: bool,
) -> &'static str {
    for value in [requested, sdl_driver].into_iter().flatten() {
        match value.trim().to_ascii_lowercase().as_str() {
            "wayland" => return "wayland",
            "x11" => return "x11",
            _ => {}
        }
    }
    if has_wayland_display && !has_x11_display {
        "wayland"
    } else {
        "x11"
    }
}

fn capabilities() -> &'static LinuxCapabilitySnapshot {
    static CAPABILITIES: OnceLock<LinuxCapabilitySnapshot> = OnceLock::new();
    CAPABILITIES.get_or_init(LinuxCapabilitySnapshot::probe)
}

pub(crate) fn select_video_path() -> LinuxVideoSelection {
    select_video_path_for(
        std::env::var(VIDEO_BACKEND_ENV).ok().as_deref(),
        capabilities(),
        selected_window_system(),
    )
}

pub(crate) fn select_embedded_video_path(
    requested: &str,
    device: Option<&crate::SharedVulkanDevice>,
    stream: crate::MediaStreamConfig,
) -> LinuxVideoSelection {
    let policy = std::env::var(VIDEO_BACKEND_ENV).unwrap_or_else(|_| "auto".to_owned());
    let requested = if requested == "auto" {
        policy.as_str()
    } else {
        requested
    };
    let requested = requested.trim().to_ascii_lowercase();
    let requested = if requested.is_empty() {
        "auto"
    } else {
        &requested
    };
    let codec = match stream.codec {
        crate::MediaVideoCodec::H264 => opennow_streamer_platform_linux::VideoCodec::H264,
        crate::MediaVideoCodec::H265 => opennow_streamer_platform_linux::VideoCodec::H265,
        crate::MediaVideoCodec::Av1 => opennow_streamer_platform_linux::VideoCodec::Av1,
    };
    if requested == "vulkan"
        || (matches!(requested, "auto" | "hardware")
            && device.is_some_and(|device| {
                device.supports_format(
                    codec,
                    stream.color_quality.linux_pixel_format(),
                    stream.width,
                    stream.height,
                )
            }))
    {
        return LinuxVideoSelection {
            path: LinuxVideoPath::Hardware(DecoderPreference::VulkanOnly),
            use_vulkan_output: true,
            fallback_reason: None,
        };
    }
    select_embedded_fallback(
        requested,
        stream,
        &crate::embedded_video_backends_with_config(device, None),
    )
}

fn select_embedded_fallback(
    requested: &str,
    stream: crate::MediaStreamConfig,
    backends: &[VideoBackendCapability],
) -> LinuxVideoSelection {
    if !stream.color_quality.is_444() {
        let color = if stream.color_quality.bit_depth() == 10 {
            "10bit_420"
        } else {
            "8bit_420"
        };
        for (name, preference) in [
            ("cuda", DecoderPreference::CudaOnly),
            ("vaapi", DecoderPreference::VaApiOnly),
            ("v4l2", DecoderPreference::V4l2Only),
        ] {
            if name == "v4l2" && stream.hdr {
                continue;
            }
            if stream.color_quality.bit_depth() == 10 && name != "vaapi" {
                continue;
            }
            if !matches!(requested, "auto" | "hardware")
                && requested != name
                && !(requested == "nvdec" && name == "cuda")
            {
                continue;
            }
            if backends.iter().any(|backend| {
                backend.backend == name
                    && backend.available
                    && backend.codecs.iter().any(|codec| {
                        codec.codec == stream.codec.label()
                            && codec.available
                            && codec
                                .color_qualities
                                .as_ref()
                                .map_or(color == "8bit_420", |qualities| qualities.contains(&color))
                    })
            }) {
                return LinuxVideoSelection {
                    path: LinuxVideoPath::Hardware(
                        if stream.color_quality.bit_depth() == 8
                            && matches!(requested, "auto" | "hardware")
                        {
                            DecoderPreference::HardwareOnly
                        } else {
                            preference
                        },
                    ),
                    use_vulkan_output: true,
                    fallback_reason: None,
                };
            }
        }
    }
    LinuxVideoSelection {
        path: LinuxVideoPath::Software,
        use_vulkan_output: false,
        fallback_reason: Some(format!(
            "No compatible embedded {requested} decoder supports the negotiated {} color profile",
            stream.codec.label(),
        )),
    }
}

fn select_video_path_for(
    requested: Option<&str>,
    capabilities: &LinuxCapabilitySnapshot,
    window_system: &str,
) -> LinuxVideoSelection {
    let requested = requested
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("auto")
        .to_ascii_lowercase();
    let presentation = capabilities.presentation_available(window_system);
    let vulkan = capabilities.backend_supports_all_codecs("vulkan-video");
    let cuda = capabilities.backend_supports_all_codecs("cuda");
    let ffmpeg = capabilities.backend_supports_all_codecs("ffmpeg-software");
    let vaapi = capabilities.decoder("vaapi-h264").available;
    let v4l2 = capabilities.decoder("v4l2-h264").available;
    let hardware = vulkan || cuda || vaapi || v4l2;

    let preference = match requested.as_str() {
        "vulkan" if vulkan => Some(DecoderPreference::VulkanOnly),
        "cuda" | "nvdec" if cuda => Some(DecoderPreference::CudaOnly),
        "vaapi" if vaapi => Some(DecoderPreference::VaApiOnly),
        "v4l2" if v4l2 => Some(DecoderPreference::V4l2Only),
        "software" | "ffmpeg" if ffmpeg => Some(DecoderPreference::SoftwareOnly),
        "hardware" if hardware => Some(DecoderPreference::HardwareOnly),
        "auto" if vulkan || cuda || vaapi || v4l2 || ffmpeg => Some(DecoderPreference::Automatic),
        _ => None,
    };
    if let Some(preference) = preference {
        return LinuxVideoSelection {
            path: LinuxVideoPath::Hardware(preference),
            use_vulkan_output: presentation,
            fallback_reason: (!presentation).then(|| {
                format!(
                    "{}; using SDL NV12 presentation",
                    presentation_unavailable_reason(&capabilities.presentation, window_system)
                )
            }),
        };
    }

    let reason = match requested.as_str() {
        "vulkan" => backend_unavailable_reason(capabilities, "vulkan-video"),
        "cuda" | "nvdec" => backend_unavailable_reason(capabilities, "cuda"),
        "vaapi" => capabilities.decoder("vaapi-h264").detail,
        "v4l2" => capabilities.decoder("v4l2-h264").detail,
        "software" | "ffmpeg" => backend_unavailable_reason(capabilities, "ffmpeg-software"),
        "hardware" => format!(
            "no Linux hardware decoder is available (Vulkan Video: {}; CUDA/NVDEC: {}; VA-API: {}; V4L2: {})",
            backend_unavailable_reason(capabilities, "vulkan-video"),
            backend_unavailable_reason(capabilities, "cuda"),
            capabilities.decoder("vaapi-h264").detail,
            capabilities.decoder("v4l2-h264").detail,
        ),
        "auto" => format!(
            "no Linux decoder is available (Vulkan Video: {}; CUDA/NVDEC: {}; FFmpeg software: {})",
            backend_unavailable_reason(capabilities, "vulkan-video"),
            backend_unavailable_reason(capabilities, "cuda"),
            backend_unavailable_reason(capabilities, "ffmpeg-software"),
        ),
        other => format!("video backend {other:?} is not supported on Linux"),
    };
    LinuxVideoSelection {
        path: LinuxVideoPath::Software,
        use_vulkan_output: false,
        fallback_reason: Some(format!("{reason}; using OpenH264/SDL fallback")),
    }
}

fn backend_unavailable_reason(capabilities: &LinuxCapabilitySnapshot, prefix: &str) -> String {
    CODECS
        .iter()
        .filter_map(|codec| {
            let capability = capabilities.codec_decoder(prefix, codec);
            (!capability.available).then(|| format!("{codec}: {}", capability.detail))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn video_backends() -> Vec<VideoBackendCapability> {
    let capabilities = capabilities();
    let window_system = selected_window_system();
    vec![
        multi_codec_capability("vulkan", "vulkan-video", capabilities, window_system, false),
        multi_codec_capability("cuda", "cuda", capabilities, window_system, false),
        h264_capability("vaapi", "vaapi-h264", capabilities, window_system),
        v4l2_capability(capabilities, window_system),
        multi_codec_capability(
            "ffmpeg",
            "ffmpeg-software",
            capabilities,
            window_system,
            false,
        ),
    ]
}

fn v4l2_capability(
    capabilities: &LinuxCapabilitySnapshot,
    window_system: &str,
) -> VideoBackendCapability {
    let mut backend = h264_capability("v4l2", "v4l2-h264", capabilities, window_system);
    let decoder = capabilities.decoder("v4l2-h265");
    let presentation = capabilities.presentation_available(window_system);
    let codec = backend
        .codecs
        .iter_mut()
        .find(|codec| codec.codec == "h265")
        .unwrap();
    codec.available = decoder.available && presentation;
    codec.color_qualities = Some(if codec.available {
        vec!["8bit_420"]
    } else {
        Vec::new()
    });
    codec.hdr_supported = Some(false);
    codec.reason = if !decoder.available {
        Some(static_reason(decoder.detail))
    } else if !presentation {
        Some(static_reason(presentation_unavailable_reason(
            &capabilities.presentation,
            window_system,
        )))
    } else {
        None
    };
    backend.available = backend.codecs.iter().any(|codec| codec.available);
    if backend.available {
        backend.reason = None;
    } else {
        backend.reason = Some(static_reason(
            backend
                .codecs
                .iter()
                .filter(|codec| matches!(codec.codec, "h264" | "h265"))
                .filter_map(|codec| {
                    codec
                        .reason
                        .map(|reason| format!("{}: {reason}", codec.codec))
                })
                .collect::<Vec<_>>()
                .join("; "),
        ));
    }
    backend
}

fn multi_codec_capability(
    backend: &'static str,
    prefix: &str,
    capabilities: &LinuxCapabilitySnapshot,
    window_system: &str,
    requires_presentation: bool,
) -> VideoBackendCapability {
    let presentation = !requires_presentation || capabilities.presentation_available(window_system);
    let codecs = CODECS
        .into_iter()
        .map(|codec| {
            let decoder = capabilities.codec_decoder(prefix, codec);
            let available = decoder.available && presentation;
            let reason = if !decoder.available {
                Some(static_reason(decoder.detail))
            } else if !presentation {
                Some(static_reason(presentation_unavailable_reason(
                    &capabilities.presentation,
                    window_system,
                )))
            } else {
                None
            };
            CodecCapability {
                hdr_supported: None,
                hdr_color_qualities: None,
                color_qualities: None,
                codec,
                available,
                reason,
            }
        })
        .collect::<Vec<_>>();
    let available = codecs.iter().any(|codec| codec.available);
    let reason = (!available).then(|| {
        codecs
            .iter()
            .find_map(|codec| codec.reason)
            .unwrap_or("no supported codecs")
    });
    VideoBackendCapability {
        backend,
        platform: "linux",
        codecs,
        zero_copy_modes: (backend == "vulkan"
            && available
            && capabilities.presentation_available(window_system))
        .then_some(vec!["vulkan-video-same-device-nv12"])
        .unwrap_or_default(),
        available,
        reason,
    }
}

fn h264_capability(
    backend: &'static str,
    decoder_name: &str,
    capabilities: &LinuxCapabilitySnapshot,
    window_system: &str,
) -> VideoBackendCapability {
    let decoder = capabilities.decoder(decoder_name);
    let h264_available = decoder.available;
    let h264_reason = if !decoder.available {
        Some(static_reason(decoder.detail))
    } else {
        None
    };
    VideoBackendCapability {
        backend,
        platform: "linux",
        codecs: vec![
            CodecCapability {
                hdr_supported: None,
                hdr_color_qualities: None,
                color_qualities: None,
                codec: "h264",
                available: h264_available,
                reason: h264_reason,
            },
            CodecCapability {
                hdr_supported: None,
                hdr_color_qualities: None,
                color_qualities: None,
                codec: "h265",
                available: false,
                reason: Some("native backend currently supports H.264 only"),
            },
            CodecCapability {
                hdr_supported: None,
                hdr_color_qualities: None,
                color_qualities: None,
                codec: "av1",
                available: false,
                reason: Some("native backend currently supports H.264 only"),
            },
        ],
        zero_copy_modes: (backend == "vaapi"
            && h264_available
            && capabilities.presentation_available(window_system))
        .then_some(vec!["vaapi-drm-prime-dmabuf-vulkan"])
        .unwrap_or_default(),
        available: h264_available,
        reason: h264_reason,
    }
}

fn static_reason(reason: String) -> &'static str {
    Box::leak(reason.into_boxed_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_v4l2_summary_preserves_the_hevc_failure() {
        let mut capabilities = snapshot(false, false, false, true, vec!["wayland"]);
        capabilities.decoders.push(BackendCapability {
            name: "v4l2-h265",
            available: false,
            detail: "MEDIA_IOC_REQUEST_ALLOC failed: Permission denied (os error 13)".to_owned(),
        });
        let backend = v4l2_capability(&capabilities, "wayland");
        assert!(!backend.available);
        let reason = backend.reason.unwrap();
        assert!(reason.contains("h264:"));
        assert!(reason.contains("h265: MEDIA_IOC_REQUEST_ALLOC failed"));
        assert!(reason.contains("os error 13"));
    }

    #[test]
    fn hevc_request_is_available_without_stateful_h264_and_never_ten_bit() {
        let mut capabilities = snapshot(false, false, false, true, vec!["wayland"]);
        capabilities.decoders.push(capability("v4l2-h265", true));
        let backend = v4l2_capability(&capabilities, "wayland");
        assert!(backend.available);
        assert!(
            !backend
                .codecs
                .iter()
                .find(|codec| codec.codec == "h264")
                .unwrap()
                .available
        );
        let codec = backend
            .codecs
            .iter()
            .find(|codec| codec.codec == "h265")
            .unwrap();
        assert!(codec.available);
        assert_eq!(codec.color_qualities, Some(vec!["8bit_420"]));
        assert_eq!(codec.hdr_supported, Some(false));
        let mut stream = crate::MediaStreamConfig {
            codec: crate::MediaVideoCodec::H265,
            color_quality: crate::MediaColorQuality::EightBit420,
            ..Default::default()
        };
        assert_eq!(
            select_embedded_fallback("v4l2", stream, std::slice::from_ref(&backend)).path,
            LinuxVideoPath::Hardware(DecoderPreference::V4l2Only)
        );
        stream.color_quality = crate::MediaColorQuality::TenBit420;
        assert_eq!(
            select_embedded_fallback("v4l2", stream, std::slice::from_ref(&backend)).path,
            LinuxVideoPath::Software
        );
        stream.color_quality = crate::MediaColorQuality::EightBit420;
        stream.hdr = true;
        assert_eq!(
            select_embedded_fallback("v4l2", stream, &[backend]).path,
            LinuxVideoPath::Software
        );
        assert!(!v4l2_capability(&capabilities, "x11").available);
    }

    #[test]
    fn ten_bit_fallback_uses_only_explicit_vaapi_profiles() {
        let stream = crate::MediaStreamConfig {
            codec: crate::MediaVideoCodec::H265,
            color_quality: crate::MediaColorQuality::TenBit420,
            hdr: true,
            ..Default::default()
        };
        let mut backend = VideoBackendCapability {
            backend: "vaapi",
            platform: "linux",
            codecs: vec![CodecCapability {
                codec: "h265",
                available: true,
                hdr_color_qualities: None,
                color_qualities: Some(vec!["8bit_420", "10bit_420"]),
                hdr_supported: None,
                reason: None,
            }],
            zero_copy_modes: vec![],
            available: true,
            reason: None,
        };
        for requested in ["auto", "hardware", "vaapi"] {
            assert_eq!(
                select_embedded_fallback(requested, stream, &[backend.clone()]).path,
                LinuxVideoPath::Hardware(DecoderPreference::VaApiOnly)
            );
        }
        for name in ["cuda", "v4l2", "ffmpeg"] {
            backend.backend = name;
            assert_eq!(
                select_embedded_fallback("auto", stream, &[backend.clone()]).path,
                LinuxVideoPath::Software
            );
        }
        backend.backend = "vaapi";
        for qualities in [None, Some(vec![]), Some(vec!["8bit_420"])] {
            backend.codecs[0].color_qualities = qualities;
            assert_eq!(
                select_embedded_fallback("auto", stream, &[backend.clone()]).path,
                LinuxVideoPath::Software
            );
        }
    }

    #[test]
    fn embedded_fallback_uses_the_selected_codec_not_all_codec_support() {
        let capabilities = snapshot(true, false, true, true, vec!["x11"]);
        let mut cuda = multi_codec_capability("cuda", "cuda", &capabilities, "x11", false);
        cuda.available = true;
        cuda.codecs
            .iter_mut()
            .find(|codec| codec.codec == "h265")
            .unwrap()
            .available = true;
        let stream = crate::MediaStreamConfig {
            codec: crate::MediaVideoCodec::H265,
            ..Default::default()
        };
        for (requested, preference) in [
            ("auto", DecoderPreference::HardwareOnly),
            ("hardware", DecoderPreference::HardwareOnly),
            ("cuda", DecoderPreference::CudaOnly),
            ("nvdec", DecoderPreference::CudaOnly),
        ] {
            assert_eq!(
                select_embedded_fallback(requested, stream, &[cuda.clone()]).path,
                LinuxVideoPath::Hardware(preference),
            );
        }
        let stream = crate::MediaStreamConfig {
            codec: crate::MediaVideoCodec::Av1,
            ..stream
        };
        assert_eq!(
            select_embedded_fallback("auto", stream, &[cuda]).path,
            LinuxVideoPath::Software
        );
    }

    #[test]
    fn embedded_fallback_never_reselects_standalone_vulkan_or_software() {
        let capabilities = snapshot(true, false, true, true, vec!["x11"]);
        let mut backends = vec![
            multi_codec_capability("vulkan", "vulkan-video", &capabilities, "x11", false),
            multi_codec_capability("ffmpeg", "ffmpeg-software", &capabilities, "x11", false),
            h264_capability("vaapi", "vaapi-h264", &capabilities, "x11"),
        ];
        let stream = crate::MediaStreamConfig::default();
        let unavailable = select_embedded_fallback("auto", stream, &backends);
        assert_eq!(unavailable.path, LinuxVideoPath::Software);
        assert!(
            unavailable
                .fallback_reason
                .unwrap()
                .contains("No compatible embedded")
        );
        backends[2].available = true;
        backends[2].codecs[0].available = true;
        assert_eq!(
            select_embedded_fallback("auto", stream, &backends).path,
            LinuxVideoPath::Hardware(DecoderPreference::HardwareOnly),
        );
        for color_quality in [
            crate::MediaColorQuality::TenBit420,
            crate::MediaColorQuality::EightBit444,
        ] {
            assert_eq!(
                select_embedded_fallback(
                    "auto",
                    crate::MediaStreamConfig {
                        color_quality,
                        ..stream
                    },
                    &backends
                )
                .path,
                LinuxVideoPath::Software,
            );
        }
    }

    fn capability(name: &'static str, available: bool) -> BackendCapability {
        BackendCapability {
            name,
            available,
            detail: format!("{name} probe"),
        }
    }

    fn snapshot(
        vulkan_video: bool,
        cuda: bool,
        software: bool,
        presentation: bool,
        window_systems: Vec<&'static str>,
    ) -> LinuxCapabilitySnapshot {
        let mut decoders = Vec::new();
        for codec in CODECS {
            decoders.push(capability(
                match codec {
                    "h264" => "vulkan-video-h264",
                    "h265" => "vulkan-video-h265",
                    _ => "vulkan-video-av1",
                },
                vulkan_video,
            ));
            decoders.push(capability(
                match codec {
                    "h264" => "cuda-h264",
                    "h265" => "cuda-h265",
                    _ => "cuda-av1",
                },
                cuda,
            ));
            decoders.push(capability(
                match codec {
                    "h264" => "ffmpeg-software-h264",
                    "h265" => "ffmpeg-software-h265",
                    _ => "ffmpeg-software-av1",
                },
                software,
            ));
        }
        decoders.push(capability("vaapi-h264", false));
        decoders.push(capability("v4l2-h264", false));
        LinuxCapabilitySnapshot {
            decoders,
            presentation: PresentationCapability {
                available: presentation,
                api: "vulkan",
                window_systems,
                detail: "vulkan presentation probe".to_owned(),
            },
        }
    }

    #[test]
    fn explicit_backends_select_distinct_decoders() {
        let available = snapshot(true, true, true, true, vec!["x11"]);
        assert_eq!(
            select_video_path_for(Some("vulkan"), &available, "x11").path,
            LinuxVideoPath::Hardware(DecoderPreference::VulkanOnly)
        );
        assert_eq!(
            select_video_path_for(Some("nvdec"), &available, "x11").path,
            LinuxVideoPath::Hardware(DecoderPreference::CudaOnly)
        );
        assert_eq!(
            select_video_path_for(Some("software"), &available, "x11").path,
            LinuxVideoPath::Hardware(DecoderPreference::SoftwareOnly)
        );
        assert_eq!(
            select_video_path_for(Some("hardware"), &available, "x11").path,
            LinuxVideoPath::Hardware(DecoderPreference::HardwareOnly)
        );
    }

    #[test]
    fn automatic_selection_uses_the_complete_codec_pipeline() {
        let available = snapshot(true, true, true, true, vec!["wayland"]);
        assert_eq!(
            select_video_path_for(None, &available, "wayland").path,
            LinuxVideoPath::Hardware(DecoderPreference::Automatic)
        );
    }

    #[test]
    fn presentation_must_match_the_active_window_system() {
        let available = snapshot(true, true, true, true, vec!["wayland"]);
        let selected = select_video_path_for(None, &available, "x11");
        assert_eq!(
            selected.path,
            LinuxVideoPath::Hardware(DecoderPreference::Automatic)
        );
        assert!(!selected.use_vulkan_output);
        assert!(
            selected
                .fallback_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("lacks x11 WSI"))
        );
    }

    #[test]
    fn partial_codec_support_is_not_treated_as_a_complete_explicit_backend() {
        let mut available = snapshot(true, true, true, true, vec!["x11"]);
        available
            .decoders
            .iter_mut()
            .find(|capability| capability.name == "vulkan-video-av1")
            .unwrap()
            .available = false;
        let selected = select_video_path_for(Some("vulkan"), &available, "x11");
        assert_eq!(selected.path, LinuxVideoPath::Software);
        assert!(selected.fallback_reason.unwrap().contains("av1"));
    }

    #[test]
    fn window_system_selection_prefers_explicit_runtime_contract() {
        assert_eq!(
            selected_window_system_for(Some("wayland"), Some("x11"), false, true),
            "wayland"
        );
        assert_eq!(
            selected_window_system_for(None, Some("wayland"), true, true),
            "wayland"
        );
        assert_eq!(
            selected_window_system_for(None, None, true, false),
            "wayland"
        );
        assert_eq!(selected_window_system_for(None, None, true, true), "x11");
    }
}
