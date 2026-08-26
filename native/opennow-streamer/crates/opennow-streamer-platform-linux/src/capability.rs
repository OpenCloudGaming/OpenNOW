use crate::VideoCodec;
use crate::audio::{AudioBackend, probe_audio_backend};
use crate::video::{
    probe_ffmpeg_cuda, probe_ffmpeg_software, probe_ffmpeg_vulkan, probe_v4l2_devices, probe_vaapi,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendCapability {
    pub name: &'static str,
    pub available: bool,
    pub detail: String,
}

impl BackendCapability {
    fn from_probe(name: &'static str, probe: std::result::Result<String, String>) -> Self {
        match probe {
            Ok(detail) => Self {
                name,
                available: true,
                detail,
            },
            Err(detail) => Self {
                name,
                available: false,
                detail,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentationCapability {
    pub available: bool,
    pub api: &'static str,
    pub window_systems: Vec<&'static str>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityReport {
    pub architecture: &'static str,
    pub video_decoders: Vec<BackendCapability>,
    pub presentation: PresentationCapability,
    pub audio_outputs: Vec<BackendCapability>,
}

pub fn probe_video_capabilities() -> (Vec<BackendCapability>, PresentationCapability) {
    let v4l2_devices = probe_v4l2_devices();
    let v4l2 = if v4l2_devices.is_empty() {
        Err("no usable stateful H.264 M2M node under /dev/video*".to_owned())
    } else {
        Ok(v4l2_devices
            .iter()
            .map(|device| device.description())
            .collect::<Vec<_>>()
            .join(", "))
    };

    #[cfg(feature = "vulkan")]
    let presentation = match crate::presentation::probe_vulkan() {
        Ok((window_systems, detail)) => PresentationCapability {
            available: true,
            api: "vulkan",
            window_systems,
            detail,
        },
        Err(detail) => PresentationCapability {
            available: false,
            api: "vulkan",
            window_systems: Vec::new(),
            detail,
        },
    };
    #[cfg(not(feature = "vulkan"))]
    let presentation = PresentationCapability {
        available: false,
        api: "vulkan",
        window_systems: Vec::new(),
        detail: "crate was built without the vulkan feature".to_owned(),
    };

    (
        vec![
            BackendCapability::from_probe("vaapi-h264", probe_vaapi()),
            BackendCapability::from_probe("v4l2-h264", v4l2),
            BackendCapability::from_probe(
                "vulkan-video-h264",
                probe_ffmpeg_vulkan(VideoCodec::H264),
            ),
            BackendCapability::from_probe(
                "vulkan-video-h265",
                probe_ffmpeg_vulkan(VideoCodec::H265),
            ),
            BackendCapability::from_probe("vulkan-video-av1", probe_ffmpeg_vulkan(VideoCodec::Av1)),
            BackendCapability::from_probe("cuda-h264", probe_ffmpeg_cuda(VideoCodec::H264)),
            BackendCapability::from_probe("cuda-h265", probe_ffmpeg_cuda(VideoCodec::H265)),
            BackendCapability::from_probe("cuda-av1", probe_ffmpeg_cuda(VideoCodec::Av1)),
            BackendCapability::from_probe(
                "ffmpeg-software-h264",
                probe_ffmpeg_software(VideoCodec::H264),
            ),
            BackendCapability::from_probe(
                "ffmpeg-software-h265",
                probe_ffmpeg_software(VideoCodec::H265),
            ),
            BackendCapability::from_probe(
                "ffmpeg-software-av1",
                probe_ffmpeg_software(VideoCodec::Av1),
            ),
        ],
        presentation,
    )
}

pub fn probe_capabilities() -> CapabilityReport {
    let (video_decoders, presentation) = probe_video_capabilities();
    CapabilityReport {
        architecture: std::env::consts::ARCH,
        video_decoders,
        presentation,
        audio_outputs: vec![
            BackendCapability::from_probe("pipewire", probe_audio_backend(AudioBackend::PipeWire)),
            BackendCapability::from_probe("alsa", probe_audio_backend(AudioBackend::Alsa)),
        ],
    }
}
