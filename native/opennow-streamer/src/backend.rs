use crate::input::{PARTIALLY_RELIABLE_GAMEPAD_MASK_ALL, PARTIALLY_RELIABLE_HID_DEVICE_MASK_ALL};
use crate::protocol::{
    missing_field, ColorQuality, CommandEnvelope, Event, MediaConnectionInfo,
    NativeStreamerCapabilities, NativeStreamerSessionContext, Response, VideoCodec,
    PROTOCOL_VERSION,
};
use crate::sdp::{
    duplicate_session_webrtc_attributes_to_media, extract_ice_credentials, fix_server_ip,
    parse_resolution, prefer_codec, sanitize_ice_pwd_for_gstreamer,
    summarize_media_transport_attributes, NvstParams, PreferCodecOptions,
};
use std::env;
use std::sync::mpsc::Sender;

pub trait NativeStreamerBackend {
    fn capabilities(&self) -> NativeStreamerCapabilities;
    fn start(&mut self, command: CommandEnvelope) -> BackendReply;
    fn handle_offer(&mut self, command: CommandEnvelope) -> BackendReply;
    fn add_remote_ice(&mut self, command: CommandEnvelope) -> BackendReply;
    fn send_input(&mut self, command: CommandEnvelope) -> BackendReply;
    fn update_render_surface(&mut self, command: CommandEnvelope) -> BackendReply;
    fn update_bitrate_limit(&mut self, command: CommandEnvelope) -> BackendReply;
    fn stop(&mut self, command: CommandEnvelope) -> BackendReply;
}

const BACKEND_ENV: &str = "OPENNOW_NATIVE_STREAMER_BACKEND";
const NATIVE_CODEC_ENV: &str = "OPENNOW_NATIVE_CODEC";
const MIN_BITRATE_KBPS: u32 = 5_000;
const MAX_BITRATE_KBPS: u32 = 150_000;

pub fn create_backend(event_sender: Option<Sender<Event>>) -> Box<dyn NativeStreamerBackend> {
    let requested = env::var(BACKEND_ENV)
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());
    create_backend_for_name(requested.as_deref(), event_sender)
}

fn create_backend_for_name(
    requested: Option<&str>,
    event_sender: Option<Sender<Event>>,
) -> Box<dyn NativeStreamerBackend> {
    #[cfg(not(feature = "gstreamer"))]
    let _ = &event_sender;

    match requested.unwrap_or(default_backend_name()) {
        "stub" => Box::<StubBackend>::default(),
        #[cfg(feature = "gstreamer")]
        "gstreamer" => Box::new(crate::gstreamer_backend::GstreamerBackend::new(
            event_sender,
        )),
        #[cfg(not(feature = "gstreamer"))]
        "gstreamer" => Box::new(StubBackend::with_fallback(
            "gstreamer",
            "GStreamer backend was requested, but this binary was built without the gstreamer feature.",
        )),
        other => Box::new(StubBackend::with_fallback(
            other,
            format!("Unknown native streamer backend \"{other}\"; using stub."),
        )),
    }
}

fn default_backend_name() -> &'static str {
    #[cfg(feature = "gstreamer")]
    {
        "gstreamer"
    }
    #[cfg(not(feature = "gstreamer"))]
    {
        "stub"
    }
}

#[derive(Debug, Default)]
pub struct BackendReply {
    pub events: Vec<Event>,
    pub response: Option<Response>,
    pub should_continue: bool,
}

impl BackendReply {
    pub fn response(response: Response) -> Self {
        Self {
            events: Vec::new(),
            response: Some(response),
            should_continue: true,
        }
    }

    pub fn continue_without_response() -> Self {
        Self {
            events: Vec::new(),
            response: None,
            should_continue: true,
        }
    }

    pub fn stop(id: String, message: String) -> Self {
        Self {
            events: vec![Event::Status {
                status: "stopped",
                message: Some(message),
            }],
            response: Some(Response::Ok { id }),
            should_continue: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PreparedNativeOffer {
    pub original_sdp_len: usize,
    pub fixed_offer_sdp: String,
    pub gstreamer_offer_sdp: String,
    pub gstreamer_ice_pwd_replacements: usize,
    pub gstreamer_framerate_adjusted: bool,
    pub nvst_params: NvstParams,
    pub media_connection_info: Option<MediaConnectionInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrepareNativeOfferError {
    InvalidResolution { resolution: String },
}

impl PrepareNativeOfferError {
    pub fn into_response(self, id: String) -> Response {
        match self {
            Self::InvalidResolution { resolution } => Response::Error {
                id: Some(id),
                code: "invalid-resolution".to_owned(),
                message: format!("Invalid stream resolution: {resolution}"),
            },
        }
    }
}

pub fn prepare_native_offer(
    context: &NativeStreamerSessionContext,
    offer_sdp: &str,
) -> Result<PreparedNativeOffer, PrepareNativeOfferError> {
    let Some((width, height)) = parse_resolution(&context.settings.resolution) else {
        return Err(PrepareNativeOfferError::InvalidResolution {
            resolution: context.settings.resolution.clone(),
        });
    };

    let fixed_offer_sdp = duplicate_session_webrtc_attributes_to_media(&fix_server_ip(
        offer_sdp,
        &context.session.server_ip,
    ));
    let codec = resolve_native_codec(context.settings.codec);
    let fixed_offer_sdp = prefer_codec(
        &fixed_offer_sdp,
        codec,
        PreferCodecOptions {
            prefer_hevc_profile_id: Some(preferred_hevc_profile_id(context.settings.color_quality)),
        },
    );
    let (gstreamer_framerate_offer_sdp, gstreamer_framerate_adjusted) =
        align_video_sdp_framerate_for_gstreamer(&fixed_offer_sdp, context.settings.fps);
    let (gstreamer_offer_sdp, gstreamer_ice_pwd_replacements) =
        sanitize_ice_pwd_for_gstreamer(&gstreamer_framerate_offer_sdp);
    let credentials = extract_ice_credentials(&fixed_offer_sdp);
    let nvst_params = NvstParams {
        width,
        height,
        fps: context.settings.fps,
        max_bitrate_kbps: context.settings.max_bitrate_mbps.saturating_mul(1000),
        partial_reliable_threshold_ms: 16,
        codec,
        color_quality: context.settings.color_quality,
        credentials,
        hid_device_mask: Some(PARTIALLY_RELIABLE_HID_DEVICE_MASK_ALL),
        enable_partially_reliable_transfer_gamepad: Some(PARTIALLY_RELIABLE_GAMEPAD_MASK_ALL),
        enable_partially_reliable_transfer_hid: Some(PARTIALLY_RELIABLE_HID_DEVICE_MASK_ALL),
    };

    Ok(PreparedNativeOffer {
        original_sdp_len: offer_sdp.len(),
        fixed_offer_sdp,
        gstreamer_offer_sdp,
        gstreamer_ice_pwd_replacements,
        gstreamer_framerate_adjusted,
        nvst_params,
        media_connection_info: context.session.media_connection_info.clone(),
    })
}

fn align_video_sdp_framerate_for_gstreamer(sdp: &str, fps: u32) -> (String, bool) {
    if fps == 0 {
        return (sdp.to_owned(), false);
    }

    let line_ending = if sdp.contains("\r\n") { "\r\n" } else { "\n" };
    let has_trailing_ending = sdp.ends_with(line_ending);
    let mut lines: Vec<String> = sdp
        .split(line_ending)
        .filter(|line| !line.is_empty() || !has_trailing_ending)
        .map(ToOwned::to_owned)
        .collect();
    let mut output = Vec::with_capacity(lines.len() + 1);
    let mut in_video = false;
    let mut video_has_framerate = false;
    let mut changed = false;
    let target = format!("a=framerate:{fps}");

    for line in lines.drain(..) {
        if line.starts_with("m=") {
            if in_video && !video_has_framerate {
                output.push(target.clone());
                changed = true;
            }
            in_video = line.starts_with("m=video");
            video_has_framerate = false;
            output.push(line);
            continue;
        }

        if in_video && line.starts_with("a=framerate:") {
            video_has_framerate = true;
            if line != target {
                output.push(target.clone());
                changed = true;
            } else {
                output.push(line);
            }
            continue;
        }

        output.push(line);
    }

    if in_video && !video_has_framerate {
        output.push(target);
        changed = true;
    }

    let mut result = output.join(line_ending);
    if has_trailing_ending {
        result.push_str(line_ending);
    }
    (result, changed)
}

fn resolve_native_codec(configured: VideoCodec) -> VideoCodec {
    match env::var(NATIVE_CODEC_ENV)
        .unwrap_or_else(|_| "auto".to_owned())
        .to_ascii_lowercase()
        .as_str()
    {
        "h264" | "avc" => VideoCodec::H264,
        "h265" | "hevc" => VideoCodec::H265,
        "av1" => VideoCodec::AV1,
        _ => configured,
    }
}

pub fn prepared_offer_events(prepared: &PreparedNativeOffer) -> Vec<Event> {
    let nvst = &prepared.nvst_params;
    let mut events = vec![Event::Log {
        level: "info",
        message: format!(
            "Prepared native offer for {}x{}@{} {} {}bit; SDP {} -> {} bytes.",
            nvst.width,
            nvst.height,
            nvst.fps,
            codec_label(nvst.codec),
            color_quality_bit_depth(nvst.color_quality),
            prepared.original_sdp_len,
            prepared.fixed_offer_sdp.len(),
        ),
    }];

    if prepared.gstreamer_framerate_adjusted {
        events.push(Event::Log {
            level: "info",
            message: format!(
                "Aligned native WebRTC video SDP framerate to {} fps for GStreamer caps negotiation.",
                nvst.fps
            ),
        });
    }

    if let Some(media_connection_info) = &prepared.media_connection_info {
        events.push(Event::Log {
            level: "debug",
            message: format!(
                "GFN media connection hint {}:{}.",
                media_connection_info.ip, media_connection_info.port
            ),
        });
    }
    events.push(Event::Log {
        level: "debug",
        message: format!(
            "Prepared native WebRTC SDP transport summary: {}.",
            summarize_media_transport_attributes(&prepared.fixed_offer_sdp)
        ),
    });
    if prepared.gstreamer_ice_pwd_replacements > 0 {
        events.push(Event::Log {
            level: "warn",
            message: format!(
                "GFN offer uses non-standard ICE password characters; sanitized {} ice-pwd line(s) for GStreamer validation only.",
                prepared.gstreamer_ice_pwd_replacements
            ),
        });
    }

    events
}

pub fn normalize_bitrate_kbps(value: u32) -> u32 {
    value.clamp(MIN_BITRATE_KBPS, MAX_BITRATE_KBPS)
}

pub fn bitrate_kbps_to_mbps(value: u32) -> u32 {
    normalize_bitrate_kbps(value).div_ceil(1000)
}

pub fn update_context_bitrate_limit(
    context: &mut Option<NativeStreamerSessionContext>,
    max_bitrate_kbps: u32,
) {
    if let Some(context) = context {
        context.settings.max_bitrate_mbps = bitrate_kbps_to_mbps(max_bitrate_kbps);
    }
}

#[derive(Debug, Default)]
pub struct StubBackend {
    active_context: Option<NativeStreamerSessionContext>,
    startup_warning: Option<String>,
    requested_backend: Option<String>,
    fallback_reason: Option<String>,
}

impl StubBackend {
    fn with_fallback(requested_backend: impl Into<String>, reason: impl Into<String>) -> Self {
        let requested_backend = requested_backend.into();
        let reason = reason.into();
        Self {
            active_context: None,
            startup_warning: Some(reason.clone()),
            requested_backend: Some(requested_backend),
            fallback_reason: Some(reason),
        }
    }
}

impl NativeStreamerBackend for StubBackend {
    fn capabilities(&self) -> NativeStreamerCapabilities {
        NativeStreamerCapabilities {
            protocol_version: PROTOCOL_VERSION,
            backend: "stub",
            requested_backend: self.requested_backend.clone(),
            fallback_reason: self.fallback_reason.clone(),
            supports_offer_answer: false,
            supports_remote_ice: true,
            supports_local_ice: false,
            supports_input: false,
            video_backends: Vec::new(),
        }
    }

    fn start(&mut self, command: CommandEnvelope) -> BackendReply {
        let id = command.id;
        let Some(context) = command.context else {
            return BackendReply::response(missing_field(&id, "context"));
        };
        let session_id = context.session.session_id.clone();
        self.active_context = Some(context);
        let mut events = Vec::new();
        if let Some(message) = self.startup_warning.take() {
            events.push(Event::Log {
                level: "warn",
                message,
            });
        }
        events.push(Event::Status {
            status: "ready",
            message: Some(format!(
                "Native streamer process is running for session {session_id}; media backend is not enabled."
            )),
        });
        BackendReply {
            events,
            response: Some(Response::Ok { id }),
            should_continue: true,
        }
    }

    fn handle_offer(&mut self, command: CommandEnvelope) -> BackendReply {
        let id = command.id.clone();
        let Some(context) = command.context else {
            return BackendReply::response(missing_field(&id, "context"));
        };
        let Some(offer_sdp) = command.sdp else {
            return BackendReply::response(missing_field(&id, "sdp"));
        };

        let prepared = match prepare_native_offer(&context, &offer_sdp) {
            Ok(prepared) => prepared,
            Err(error) => return BackendReply::response(error.into_response(id)),
        };

        BackendReply {
            events: prepared_offer_events(&prepared),
            response: Some(Response::Error {
                id: Some(id),
                code: "backend-unavailable".to_owned(),
                message: "The native streamer parsed the GFN offer, but no WebRTC media backend is enabled yet.".to_owned(),
            }),
            should_continue: true,
        }
    }

    fn add_remote_ice(&mut self, command: CommandEnvelope) -> BackendReply {
        if command.candidate.is_none() {
            return BackendReply::response(missing_field(&command.id, "candidate"));
        }

        BackendReply::response(Response::Ok { id: command.id })
    }

    fn send_input(&mut self, command: CommandEnvelope) -> BackendReply {
        if let Some(packet) = command.input {
            let _ = packet.payload_bytes();
        }
        BackendReply::continue_without_response()
    }

    fn update_render_surface(&mut self, command: CommandEnvelope) -> BackendReply {
        if command.surface.is_none() {
            return BackendReply::response(missing_field(&command.id, "surface"));
        }

        BackendReply::response(Response::Ok { id: command.id })
    }

    fn update_bitrate_limit(&mut self, command: CommandEnvelope) -> BackendReply {
        let Some(max_bitrate_kbps) = command.max_bitrate_kbps else {
            return BackendReply::response(missing_field(&command.id, "maxBitrateKbps"));
        };

        let max_bitrate_kbps = normalize_bitrate_kbps(max_bitrate_kbps);
        update_context_bitrate_limit(&mut self.active_context, max_bitrate_kbps);

        BackendReply {
            events: vec![Event::Log {
                level: "info",
                message: format!(
                    "Updated native bitrate limit to {max_bitrate_kbps} Kbps for the next native offer."
                ),
            }],
            response: Some(Response::Ok { id: command.id }),
            should_continue: true,
        }
    }

    fn stop(&mut self, command: CommandEnvelope) -> BackendReply {
        self.active_context = None;
        let message = command
            .reason
            .unwrap_or_else(|| "stop requested".to_owned());
        BackendReply::stop(command.id, message)
    }
}

fn codec_label(codec: VideoCodec) -> &'static str {
    match codec {
        VideoCodec::H264 => "H264",
        VideoCodec::H265 => "H265",
        VideoCodec::AV1 => "AV1",
    }
}

fn color_quality_bit_depth(color_quality: ColorQuality) -> u8 {
    color_quality.bit_depth()
}

fn preferred_hevc_profile_id(color_quality: ColorQuality) -> u8 {
    if color_quality.bit_depth() >= 10 {
        2
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ColorQuality, SessionInfo, StreamSettings};

    fn context(resolution: &str) -> NativeStreamerSessionContext {
        NativeStreamerSessionContext {
            session: SessionInfo {
                session_id: "session-1".to_owned(),
                server_ip: "80-250-97-40.cloudmatchbeta.nvidiagrid.net".to_owned(),
                media_connection_info: Some(MediaConnectionInfo {
                    ip: "10.0.0.7".to_owned(),
                    port: 49003,
                }),
            },
            settings: StreamSettings {
                resolution: resolution.to_owned(),
                fps: 120,
                max_bitrate_mbps: 75,
                codec: VideoCodec::H265,
                color_quality: ColorQuality::TenBit420,
                enable_cloud_gsync: false,
            },
        }
    }

    #[test]
    fn prepares_offer_once_for_all_backends() {
        let offer = "v=0\nc=IN IP4 0.0.0.0\na=ice-ufrag:user\na=ice-pwd:pass\na=fingerprint:sha-256 AA:BB\n";
        let prepared = prepare_native_offer(&context("1920x1080"), offer).expect("valid offer");

        assert!(prepared.fixed_offer_sdp.contains("c=IN IP4 80.250.97.40"));
        assert!(prepared
            .gstreamer_offer_sdp
            .contains("c=IN IP4 80.250.97.40"));
        assert_eq!(prepared.nvst_params.width, 1920);
        assert_eq!(prepared.nvst_params.height, 1080);
        assert_eq!(prepared.nvst_params.fps, 120);
        assert_eq!(prepared.nvst_params.max_bitrate_kbps, 75_000);
        assert_eq!(prepared.nvst_params.credentials.ufrag, "user");
        assert_eq!(
            prepared
                .media_connection_info
                .as_ref()
                .map(|info| info.port),
            Some(49003),
        );
    }

    #[test]
    fn prepares_offer_filters_remote_video_to_requested_codec() {
        let offer = [
            "v=0",
            "a=group:BUNDLE 0 1",
            "a=ice-ufrag:user",
            "a=ice-pwd:pass",
            "a=fingerprint:sha-256 AA:BB",
            "m=audio 9 UDP/TLS/RTP/SAVPF 111",
            "a=rtpmap:111 OPUS/48000/2",
            "m=video 9 UDP/TLS/RTP/SAVPF 96 97 98 99 100",
            "a=rtpmap:96 AV1/90000",
            "a=rtpmap:97 rtx/90000",
            "a=fmtp:97 apt=96",
            "a=rtpmap:98 H265/90000",
            "a=fmtp:98 profile-id=2;level-id=186",
            "a=rtpmap:99 rtx/90000",
            "a=fmtp:99 apt=98",
            "a=rtpmap:100 flexfec-03/90000",
        ]
        .join("\n");

        let prepared = prepare_native_offer(&context("1920x1080"), &offer).expect("valid offer");

        assert!(prepared
            .gstreamer_offer_sdp
            .contains("a=rtpmap:98 H265/90000"));
        assert!(prepared
            .gstreamer_offer_sdp
            .contains("a=rtpmap:99 rtx/90000"));
        assert!(!prepared
            .gstreamer_offer_sdp
            .contains("a=rtpmap:96 AV1/90000"));
        assert!(!prepared
            .gstreamer_offer_sdp
            .contains("a=rtpmap:97 rtx/90000"));
        assert!(prepared
            .gstreamer_offer_sdp
            .contains("m=video 9 UDP/TLS/RTP/SAVPF 98 99 100"));
        assert!(prepared
            .gstreamer_offer_sdp
            .contains("a=rtpmap:100 flexfec-03/90000"));
    }

    #[test]
    fn aligns_gstreamer_video_sdp_framerate() {
        let sdp = "v=0\nm=video 9 UDP/TLS/RTP/SAVPF 96\na=framerate:60\na=rtpmap:96 H265/90000\n";

        let (aligned, changed) = align_video_sdp_framerate_for_gstreamer(sdp, 240);

        assert!(changed);
        assert!(aligned.contains("a=framerate:240\n"));
        assert!(!aligned.contains("a=framerate:60"));
    }

    #[test]
    fn inserts_gstreamer_video_sdp_framerate_when_absent() {
        let sdp = "v=0\nm=audio 9 UDP/TLS/RTP/SAVPF 111\na=rtpmap:111 OPUS/48000/2\nm=video 9 UDP/TLS/RTP/SAVPF 96\na=rtpmap:96 H265/90000\nm=application 9 UDP/DTLS/SCTP webrtc-datachannel\n";

        let (aligned, changed) = align_video_sdp_framerate_for_gstreamer(sdp, 120);

        assert!(changed);
        assert!(aligned.contains(
            "m=video 9 UDP/TLS/RTP/SAVPF 96\na=rtpmap:96 H265/90000\na=framerate:120\nm=application"
        ));
    }

    #[test]
    fn preserves_gstreamer_video_sdp_framerate_line_endings() {
        let sdp = "v=0\r\nm=video 9 UDP/TLS/RTP/SAVPF 96\r\na=rtpmap:96 H265/90000\r\n";

        let (aligned, changed) = align_video_sdp_framerate_for_gstreamer(sdp, 240);

        assert!(changed);
        assert!(aligned.contains("a=framerate:240\r\n"));
        assert!(!aligned.contains('\n') || aligned.contains("\r\n"));
    }

    #[test]
    fn rejects_invalid_resolution_during_offer_preparation() {
        let error = prepare_native_offer(&context("bad"), "v=0").expect_err("invalid resolution");
        assert_eq!(
            error,
            PrepareNativeOfferError::InvalidResolution {
                resolution: "bad".to_owned(),
            },
        );
    }

    #[test]
    fn normalizes_native_bitrate_limits_to_slider_bounds() {
        assert_eq!(normalize_bitrate_kbps(1_000), 5_000);
        assert_eq!(normalize_bitrate_kbps(75_000), 75_000);
        assert_eq!(normalize_bitrate_kbps(250_000), 150_000);
        assert_eq!(bitrate_kbps_to_mbps(75_500), 76);
    }

    #[test]
    fn creates_expected_default_backend_for_build_features() {
        let backend = create_backend_for_name(None, None);
        let capabilities = backend.capabilities();
        #[cfg(not(feature = "gstreamer"))]
        assert_eq!(capabilities.backend, "stub");
        #[cfg(feature = "gstreamer")]
        assert_eq!(capabilities.backend, "gstreamer");
    }

    #[test]
    fn reports_unknown_backend_fallback_in_capabilities() {
        let backend = create_backend_for_name(Some("missing"), None);
        let capabilities = backend.capabilities();
        assert_eq!(capabilities.backend, "stub");
        assert_eq!(capabilities.requested_backend.as_deref(), Some("missing"));
        assert!(capabilities
            .fallback_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("Unknown native streamer backend")),);
    }

    #[cfg(not(feature = "gstreamer"))]
    #[test]
    fn reports_gstreamer_feature_fallback_in_capabilities() {
        let backend = create_backend_for_name(Some("gstreamer"), None);
        let capabilities = backend.capabilities();
        assert_eq!(capabilities.backend, "stub");
        assert_eq!(capabilities.requested_backend.as_deref(), Some("gstreamer"));
        assert!(capabilities
            .fallback_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("without the gstreamer feature")),);
    }
}
