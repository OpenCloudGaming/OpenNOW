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
        h264_capability("v4l2", "v4l2-h264", capabilities, window_system),
        multi_codec_capability(
            "ffmpeg",
            "ffmpeg-software",
            capabilities,
            window_system,
            false,
        ),
    ]
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
                codec: "h264",
                available: h264_available,
                reason: h264_reason,
            },
            CodecCapability {
                codec: "h265",
                available: false,
                reason: Some("native backend currently supports H.264 only"),
            },
            CodecCapability {
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
