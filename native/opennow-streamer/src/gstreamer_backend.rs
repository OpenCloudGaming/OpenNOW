use crate::backend::{
    normalize_bitrate_kbps, prepare_native_offer, prepared_offer_events,
    update_context_bitrate_limit, BackendReply, NativeStreamerBackend,
};
use crate::gstreamer_config::{
    resolve_d3d_fullscreen_sink, resolve_present_max_fps, NATIVE_D3D_FULLSCREEN_ENV,
    NATIVE_PRESENT_MAX_FPS_ENV, PRESENT_LIMITER_AUTO_SENTINEL,
};
use crate::gstreamer_platform::{clear_native_shortcut_bindings, set_native_shortcut_bindings};
use crate::gstreamer_pipeline::{
    current_platform_label, init_gstreamer, native_video_backend_capabilities, GstreamerPipeline,
};
use crate::protocol::{
    missing_field, CommandEnvelope, Event, IceCandidatePayload, NativeRenderSurface,
    NativeStreamerCapabilities, NativeStreamerSessionContext, NativeVideoBackendCapability,
    Response, SendAnswerRequest, PROTOCOL_VERSION,
};
use crate::sdp::{build_nvst_sdp_for_answer, extract_negotiated_video_codec, munge_answer_sdp};
use std::sync::mpsc::Sender;

pub(crate) fn send_log(event_sender: &Option<Sender<Event>>, level: &'static str, message: String) {
    if let Some(event_sender) = event_sender {
        let _ = event_sender.send(Event::Log { level, message });
    } else {
        eprintln!("[NativeStreamer] {message}");
    }
}

#[derive(Debug)]
pub struct GstreamerBackend {
    active_context: Option<NativeStreamerSessionContext>,
    pending_remote_ice: Vec<IceCandidatePayload>,
    pipeline: Option<GstreamerPipeline>,
    event_sender: Option<Sender<Event>>,
    remote_description_set: bool,
    render_surface: Option<NativeRenderSurface>,
}

impl GstreamerBackend {
    pub fn new(event_sender: Option<Sender<Event>>) -> Self {
        Self {
            active_context: None,
            pending_remote_ice: Vec::new(),
            pipeline: None,
            event_sender,
            remote_description_set: false,
            render_surface: None,
        }
    }

    fn replay_pending_remote_ice(&mut self) -> Vec<Event> {
        let candidates = std::mem::take(&mut self.pending_remote_ice);
        let Some(pipeline) = self.pipeline.as_mut() else {
            self.pending_remote_ice = candidates;
            return Vec::new();
        };

        let mut events = Vec::new();
        for candidate in candidates {
            if let Err(message) = pipeline.add_remote_ice(&candidate) {
                events.push(Event::Error {
                    code: "remote-ice-failed".to_owned(),
                    message,
                });
            }
        }
        events
    }
}

impl NativeStreamerBackend for GstreamerBackend {
    fn capabilities(&self) -> NativeStreamerCapabilities {
        NativeStreamerCapabilities {
            protocol_version: PROTOCOL_VERSION,
            backend: "gstreamer",
            requested_backend: None,
            fallback_reason: None,
            supports_offer_answer: true,
            supports_remote_ice: true,
            supports_local_ice: true,
            supports_input: true,
            video_backends: match init_gstreamer() {
                Ok(()) => native_video_backend_capabilities(),
                Err(error) => vec![NativeVideoBackendCapability {
                    backend: "gstreamer".to_owned(),
                    platform: current_platform_label().to_owned(),
                    codecs: Vec::new(),
                    zero_copy_modes: Vec::new(),
                    sink: None,
                    available: false,
                    reason: Some(error),
                }],
            },
        }
    }

    fn start(&mut self, command: CommandEnvelope) -> BackendReply {
        let id = command.id;
        let Some(context) = command.context else {
            return BackendReply::response(missing_field(&id, "context"));
        };

        let session_id = context.session.session_id.clone();
        let pipeline = match GstreamerPipeline::build(self.event_sender.clone()) {
            Ok(pipeline) => pipeline,
            Err(message) => {
                return BackendReply {
                    events: vec![Event::Error {
                        code: "gstreamer-start-failed".to_owned(),
                        message: message.clone(),
                    }],
                    response: Some(Response::Error {
                        id: Some(id),
                        code: "gstreamer-start-failed".to_owned(),
                        message,
                    }),
                    should_continue: true,
                };
            }
        };

        if let Some(old_pipeline) = self.pipeline.take() {
            if let Err(message) = old_pipeline.stop() {
                eprintln!("[NativeStreamer] {message}");
            }
        }

        set_native_shortcut_bindings(&context.shortcuts);
        self.active_context = Some(context);
        self.pending_remote_ice.clear();
        self.remote_description_set = false;
        let webrtc_name = pipeline.webrtc_name();
        self.pipeline = Some(pipeline);
        if let (Some(surface), Some(pipeline)) =
            (self.render_surface.clone(), self.pipeline.as_ref())
        {
            pipeline.update_render_surface(surface);
        }

        BackendReply {
            events: vec![Event::Status {
                status: "ready",
                message: Some(format!(
                    "GStreamer backend selected for session {session_id}; {} pipeline is ready.",
                    webrtc_name
                )),
            }],
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

        let mut events = prepared_offer_events(&prepared);
        let parsed_offer = match GstreamerPipeline::parse_offer_sdp(&prepared.gstreamer_offer_sdp) {
            Ok(offer) => offer,
            Err(message) => {
                return BackendReply {
                    events,
                    response: Some(Response::Error {
                        id: Some(id),
                        code: "invalid-remote-sdp".to_owned(),
                        message,
                    }),
                    should_continue: true,
                };
            }
        };

        let Some(pipeline) = self.pipeline.as_mut() else {
            return BackendReply {
                events,
                response: Some(Response::Error {
                    id: Some(id),
                    code: "gstreamer-not-started".to_owned(),
                    message: "GStreamer pipeline is not started.".to_owned(),
                }),
                should_continue: true,
            };
        };

        let present_max_fps = resolve_present_max_fps(context.settings.fps);
        let d3d_fullscreen_sink = resolve_d3d_fullscreen_sink(context.settings.enable_cloud_gsync);
        set_native_shortcut_bindings(&context.shortcuts);
        pipeline.set_present_max_fps(present_max_fps);
        pipeline.set_d3d_fullscreen_sink(d3d_fullscreen_sink);
        pipeline.configure_stats(&context, prepared.nvst_params.max_bitrate_kbps);
        if present_max_fps > 0 && present_max_fps != PRESENT_LIMITER_AUTO_SENTINEL {
            events.push(Event::Log {
                level: "info",
                message: format!(
                    "Native present limiter enabled at {present_max_fps} fps for {} fps stream; set {NATIVE_PRESENT_MAX_FPS_ENV}=0 to disable.",
                    context.settings.fps
                ),
            });
        }
        if d3d_fullscreen_sink {
            events.push(Event::Log {
                level: "info",
                message: format!(
                    "Native D3D fullscreen presentation is enabled for Cloud G-Sync/VRR; set {NATIVE_D3D_FULLSCREEN_ENV}=0 to disable."
                ),
            });
        }

        let answer_sdp = match pipeline.negotiate_answer(
            parsed_offer,
            (prepared.gstreamer_ice_pwd_replacements > 0)
                .then_some(&prepared.nvst_params.credentials),
            prepared.nvst_params.partial_reliable_threshold_ms,
        ) {
            Ok(answer_sdp) => munge_answer_sdp(&answer_sdp, prepared.nvst_params.max_bitrate_kbps),
            Err(message) => {
                return BackendReply {
                    events,
                    response: Some(Response::Error {
                        id: Some(id),
                        code: "gstreamer-negotiation-failed".to_owned(),
                        message,
                    }),
                    should_continue: true,
                };
            }
        };
        self.remote_description_set = true;
        events.extend(self.replay_pending_remote_ice());

        events.push(Event::Log {
            level: "info",
            message:
                "GStreamer created a local WebRTC answer and replayed queued remote ICE candidates."
                    .to_owned(),
        });

        if let Some(negotiated_codec) = extract_negotiated_video_codec(&answer_sdp) {
            if negotiated_codec != prepared.nvst_params.codec {
                events.push(Event::Log {
                    level: "warn",
                    message: format!(
                        "Negotiated video codec is {} while requested codec was {}; building NVST SDP for the negotiated codec to avoid server/client codec mismatch.",
                        negotiated_codec.as_str(),
                        prepared.nvst_params.codec.as_str(),
                    ),
                });
            } else {
                events.push(Event::Log {
                    level: "debug",
                    message: format!(
                        "Negotiated video codec confirmed as {}.",
                        negotiated_codec.as_str()
                    ),
                });
            }
        }

        let nvst_sdp = match build_nvst_sdp_for_answer(&prepared.nvst_params, &answer_sdp) {
            Ok(nvst_sdp) => nvst_sdp,
            Err(message) => {
                return BackendReply {
                    events,
                    response: Some(Response::Error {
                        id: Some(id),
                        code: "invalid-local-answer-sdp".to_owned(),
                        message,
                    }),
                    should_continue: true,
                };
            }
        };

        events.push(Event::Log {
            level: "debug",
            message: "Built native NVST SDP from the local WebRTC answer transport credentials."
                .to_owned(),
        });

        BackendReply {
            events,
            response: Some(Response::Answer {
                id,
                answer: SendAnswerRequest {
                    sdp: answer_sdp,
                    nvst_sdp: Some(nvst_sdp),
                },
            }),
            should_continue: true,
        }
    }

    fn add_remote_ice(&mut self, command: CommandEnvelope) -> BackendReply {
        let Some(candidate) = command.candidate else {
            return BackendReply::response(missing_field(&command.id, "candidate"));
        };

        if self.remote_description_set {
            if let Some(pipeline) = self.pipeline.as_mut() {
                if let Err(message) = pipeline.add_remote_ice(&candidate) {
                    return BackendReply::response(Response::Error {
                        id: Some(command.id),
                        code: "remote-ice-failed".to_owned(),
                        message,
                    });
                }
            } else {
                self.pending_remote_ice.push(candidate);
            }
        } else {
            self.pending_remote_ice.push(candidate);
        }
        BackendReply::response(Response::Ok { id: command.id })
    }

    fn send_input(&mut self, command: CommandEnvelope) -> BackendReply {
        let Some(packet) = command.input else {
            return BackendReply::continue_without_response();
        };

        let Ok(payload) = packet.payload_bytes() else {
            return BackendReply::continue_without_response();
        };

        if payload.is_empty() || payload.len() > 4096 {
            return BackendReply::continue_without_response();
        }

        if let Some(pipeline) = self.pipeline.as_ref() {
            let _ = pipeline.send_input_packet(&payload, packet.partially_reliable);
        }

        BackendReply::continue_without_response()
    }

    fn update_render_surface(&mut self, command: CommandEnvelope) -> BackendReply {
        let Some(surface) = command.surface else {
            return BackendReply::response(missing_field(&command.id, "surface"));
        };

        self.render_surface = Some(surface.clone());
        if let Some(pipeline) = self.pipeline.as_ref() {
            pipeline.update_render_surface(surface);
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
                    "Updated native bitrate limit to {max_bitrate_kbps} Kbps. The active GFN server bitrate cap is negotiated in NVST SDP and will apply on the next native offer/reconnect."
                ),
            }],
            response: Some(Response::Ok { id: command.id }),
            should_continue: true,
        }
    }

    fn stop(&mut self, command: CommandEnvelope) -> BackendReply {
        self.active_context = None;
        self.pending_remote_ice.clear();
        self.remote_description_set = false;
        clear_native_shortcut_bindings();
        if let Some(pipeline) = self.pipeline.take() {
            if let Err(message) = pipeline.stop() {
                return BackendReply {
                    events: vec![Event::Error {
                        code: "gstreamer-stop-failed".to_owned(),
                        message: message.clone(),
                    }],
                    response: Some(Response::Error {
                        id: Some(command.id),
                        code: "gstreamer-stop-failed".to_owned(),
                        message,
                    }),
                    should_continue: true,
                };
            }
        }
        let message = command
            .reason
            .unwrap_or_else(|| "stop requested".to_owned());
        BackendReply::stop(command.id, message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gstreamer_config::{automatic_present_max_fps, PRESENT_LIMITER_AUTO_SENTINEL};
    use crate::gstreamer_input::parse_input_handshake_version;
    use crate::gstreamer_liveness::{
        caps_framerate_summary, sink_stats_summary, VideoStallAction, VideoStallTracker,
    };
    use crate::gstreamer_pipeline::{
        configure_stats_overlay_element, effective_present_max_fps, format_video_chain_selection,
        rtp_video_chain_definition, RtpVideoApi, RtpVideoChainRole,
    };
    use crate::gstreamer_transitions::resolve_queue_mode;
    use crate::protocol::{NativeQueueMode, StreamSettings, VideoCodec};
    use crate::sdp::IceCredentials;
    use gst::prelude::*;
    use gstreamer as gst;
    use gstreamer_webrtc as gst_webrtc;

    #[test]
    fn builds_and_stops_webrtc_pipeline() {
        let pipeline = GstreamerPipeline::build(None).expect("GStreamer webrtcbin pipeline");
        assert_eq!(pipeline.webrtc.name(), "opennow-webrtcbin");
        pipeline.stop().expect("pipeline stops");
    }

    #[test]
    fn configures_dwrite_stats_overlay_without_type_panics() {
        gst::init().expect("gstreamer init");
        let Some(overlay) = gst::ElementFactory::make("dwritetextoverlay").build().ok() else {
            return;
        };

        configure_stats_overlay_element(&overlay);
        overlay.set_property("text", "OpenNOW native stats");
    }

    #[test]
    fn parses_basic_remote_offer_sdp() {
        let sdp = "v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\nm=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\nc=IN IP4 127.0.0.1\r\na=mid:0\r\na=sctp-port:5000\r\n";
        let parsed = GstreamerPipeline::parse_offer_sdp(sdp).expect("valid SDP");
        assert_eq!(parsed.medias_len(), 1);
    }

    #[test]
    fn defers_gfn_uuid_ice_password_until_actual_ice_stream_exists() {
        let mut pipeline = GstreamerPipeline::build(None).expect("GStreamer webrtcbin pipeline");
        let credentials = IceCredentials {
            ufrag: "2efecf37".to_owned(),
            pwd: "26b335b8-6cb2-4c18-96d0-963e5e586c9a".to_owned(),
            fingerprint: String::new(),
        };

        pipeline.original_remote_ice_credentials = Some(credentials);
        assert!(!pipeline
            .try_restore_original_remote_ice_credentials("without negotiated streams")
            .expect("remote ICE credential restoration can be deferred"));
        pipeline.stop().expect("pipeline stops");
    }

    #[test]
    fn remote_ice_credential_restore_after_remote_description_does_not_probe_fake_streams() {
        let mut pipeline = GstreamerPipeline::build(None).expect("GStreamer webrtcbin pipeline");
        let sdp = concat!(
            "v=0\r\n",
            "o=- 4373647202393833435 2 IN IP4 127.0.0.1\r\n",
            "s=-\r\n",
            "t=0 0\r\n",
            "a=group:BUNDLE 0 1 2 3\r\n",
            "a=ice-options:trickle\r\n",
            "a=ice-lite\r\n",
            "m=audio 9 UDP/TLS/RTP/SAVPF 111\r\n",
            "c=IN IP4 0.0.0.0\r\n",
            "a=mid:0\r\n",
            "a=ice-ufrag:2efecf37\r\n",
            "a=ice-pwd:26b335b899a84ffab9aaf38ddad1e2b4\r\n",
            "a=fingerprint:sha-256 94:6C:60:66:35:B9:F6:B4:BC:46:60:EF:81:AC:AB:87:A9:45:4A:09:92:E4:3E:16:28:7E:BD:6D:8C:1A:7D:6B\r\n",
            "a=setup:actpass\r\n",
            "a=rtcp-mux\r\n",
            "a=rtpmap:111 OPUS/48000/2\r\n",
            "m=video 9 UDP/TLS/RTP/SAVPF 96\r\n",
            "c=IN IP4 0.0.0.0\r\n",
            "a=mid:1\r\n",
            "a=ice-ufrag:2efecf37\r\n",
            "a=ice-pwd:26b335b899a84ffab9aaf38ddad1e2b4\r\n",
            "a=fingerprint:sha-256 94:6C:60:66:35:B9:F6:B4:BC:46:60:EF:81:AC:AB:87:A9:45:4A:09:92:E4:3E:16:28:7E:BD:6D:8C:1A:7D:6B\r\n",
            "a=setup:actpass\r\n",
            "a=rtcp-mux\r\n",
            "a=rtpmap:96 H264/90000\r\n",
            "m=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\n",
            "c=IN IP4 0.0.0.0\r\n",
            "a=mid:2\r\n",
            "a=ice-ufrag:2efecf37\r\n",
            "a=ice-pwd:26b335b899a84ffab9aaf38ddad1e2b4\r\n",
            "a=fingerprint:sha-256 94:6C:60:66:35:B9:F6:B4:BC:46:60:EF:81:AC:AB:87:A9:45:4A:09:92:E4:3E:16:28:7E:BD:6D:8C:1A:7D:6B\r\n",
            "a=setup:actpass\r\n",
            "a=sctp-port:5000\r\n",
            "m=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\n",
            "c=IN IP4 0.0.0.0\r\n",
            "a=mid:3\r\n",
            "a=ice-ufrag:2efecf37\r\n",
            "a=ice-pwd:26b335b899a84ffab9aaf38ddad1e2b4\r\n",
            "a=fingerprint:sha-256 94:6C:60:66:35:B9:F6:B4:BC:46:60:EF:81:AC:AB:87:A9:45:4A:09:92:E4:3E:16:28:7E:BD:6D:8C:1A:7D:6B\r\n",
            "a=setup:actpass\r\n",
            "a=sctp-port:5000\r\n",
        );
        let offer_sdp = GstreamerPipeline::parse_offer_sdp(sdp).expect("valid SDP");
        let offer =
            gst_webrtc::WebRTCSessionDescription::new(gst_webrtc::WebRTCSDPType::Offer, offer_sdp);
        pipeline
            .pipeline
            .set_state(gst::State::Playing)
            .expect("pipeline plays");
        pipeline
            .set_description("set-remote-description", &offer)
            .expect("remote description");

        let credentials = IceCredentials {
            ufrag: "2efecf37".to_owned(),
            pwd: "26b335b8-99a8-4ffa-b9aa-f38ddad1e2b4".to_owned(),
            fingerprint: String::new(),
        };
        pipeline.original_remote_ice_credentials = Some(credentials);
        pipeline
            .try_restore_original_remote_ice_credentials("after remote description")
            .expect("remote ICE credential restoration does not fail without actual streams");
        pipeline.stop().expect("pipeline stops");
    }

    #[test]
    fn reports_offer_answer_and_local_ice_capabilities() {
        let backend = GstreamerBackend::new(None);
        let capabilities = backend.capabilities();
        assert!(capabilities.supports_offer_answer);
        assert!(capabilities.supports_local_ice);
        assert!(capabilities.supports_input);
    }

    #[test]
    fn parses_input_handshake_versions() {
        assert_eq!(
            parse_input_handshake_version(&[0x0e, 0x02, 0x03, 0x00]),
            Some(3)
        );
        assert_eq!(parse_input_handshake_version(&[0x0e, 0x02]), Some(2));
        assert_eq!(parse_input_handshake_version(&[0x0e, 0x03]), Some(0x030e));
        assert_eq!(parse_input_handshake_version(&[0x01, 0x02, 0x03]), None);
        assert_eq!(parse_input_handshake_version(&[0x0e]), None);
    }

    #[test]
    fn maps_rtp_video_codecs_to_explicit_gpu_decode_chains() {
        let h265 =
            rtp_video_chain_definition("H265", RtpVideoApi::D3D11).expect("H265 D3D11 chain");
        assert_eq!(h265[0].factory, "rtph265depay");
        assert_eq!(h265[3].factory, "d3d11h265dec");
        assert_eq!(h265[4].factory, "dwritetextoverlay");
        assert_eq!(h265[6].factory, "d3d11videosink");
        assert!(!h265
            .iter()
            .any(|spec| spec.role == RtpVideoChainRole::PostDecodeCapsFilter));

        let h264 =
            rtp_video_chain_definition("h264", RtpVideoApi::D3D12).expect("H264 D3D12 chain");
        assert_eq!(h264[0].factory, "rtph264depay");
        assert_eq!(h264[3].factory, "d3d12h264dec");
        assert_eq!(h264[4].factory, "dwritetextoverlay");
        assert_eq!(h264[6].factory, "d3d12videosink");
        assert!(!h264
            .iter()
            .any(|spec| spec.role == RtpVideoChainRole::PostDecodeCapsFilter));

        let av1 = rtp_video_chain_definition("AV1", RtpVideoApi::D3D11).expect("AV1 D3D11 chain");
        assert_eq!(av1[0].factory, "rtpav1depay");
        assert_eq!(av1[3].factory, "d3d11av1dec");
        assert_eq!(av1[4].factory, "dwritetextoverlay");
        assert_eq!(av1[6].factory, "d3d11videosink");
    }

    #[test]
    fn does_not_force_d3d_memory_caps_by_default() {
        let d3d11 =
            rtp_video_chain_definition("H265", RtpVideoApi::D3D11).expect("H265 D3D11 chain");
        let d3d12 =
            rtp_video_chain_definition("H264", RtpVideoApi::D3D12).expect("H264 D3D12 chain");

        assert!(!d3d11
            .iter()
            .any(|spec| spec.role == RtpVideoChainRole::PostDecodeCapsFilter));
        assert!(!d3d12
            .iter()
            .any(|spec| spec.role == RtpVideoChainRole::PostDecodeCapsFilter));
    }

    #[test]
    fn maps_cross_platform_video_paths_to_expected_decoders() {
        let vt =
            rtp_video_chain_definition("H264", RtpVideoApi::VideoToolbox).expect("VideoToolbox");
        assert_eq!(vt[3].factory, "vtdec_hw");
        assert!(vt.iter().any(|spec| spec.factory == "videoconvert"));
        assert_eq!(vt.last().map(|spec| spec.factory), Some("glimagesink"));
        assert!(!vt.iter().any(|spec| spec.factory == "capsfilter"));

        let vaapi = rtp_video_chain_definition("AV1", RtpVideoApi::Vaapi).expect("VAAPI AV1");
        assert_eq!(vaapi[3].factory, "vaav1dec");
        assert!(vaapi.iter().any(|spec| spec.factory == "videoconvert"));
        assert_eq!(vaapi.last().map(|spec| spec.factory), Some("glimagesink"));

        let v4l2 = rtp_video_chain_definition("H265", RtpVideoApi::V4L2).expect("V4L2 H265");
        assert_eq!(v4l2[3].factory, "v4l2slh265dec");
        assert!(v4l2.iter().any(|spec| spec.factory == "videoconvert"));

        let vulkan = rtp_video_chain_definition("H265", RtpVideoApi::Vulkan).expect("Vulkan H265");
        assert_eq!(vulkan[3].factory, "vulkanh265dec");
        assert!(vulkan
            .iter()
            .any(|spec| spec.factory == "vulkancolorconvert"));
        assert_eq!(vulkan.last().map(|spec| spec.factory), Some("vulkansink"));
        assert!(rtp_video_chain_definition("AV1", RtpVideoApi::Vulkan).is_none());

        let software =
            rtp_video_chain_definition("H264", RtpVideoApi::Software).expect("software H264");
        assert_eq!(software[3].factory, "avdec_h264");
        assert!(software.iter().any(|spec| spec.factory == "videoconvert"));
        assert_eq!(
            software.last().map(|spec| spec.factory),
            Some("autovideosink")
        );
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn windows_default_video_api_prefers_d3d12_for_high_fps() {
        assert_eq!(
            default_rtp_video_api_priority(Some(240)),
            vec![
                RtpVideoApi::D3D12,
                RtpVideoApi::D3D11,
                RtpVideoApi::Software
            ]
        );
        assert_eq!(
            default_rtp_video_api_priority(Some(120)),
            vec![
                RtpVideoApi::D3D11,
                RtpVideoApi::D3D12,
                RtpVideoApi::Software
            ]
        );
    }

    #[test]
    fn automatic_present_limiter_uses_display_refresh_below_requested_fps() {
        assert_eq!(automatic_present_max_fps(240, Some(165)), 165);
        assert_eq!(automatic_present_max_fps(240, Some(240)), 0);
        assert_eq!(automatic_present_max_fps(240, Some(1)), 0);
        assert_eq!(automatic_present_max_fps(240, None), 0);
    }

    #[test]
    fn automatic_present_limiter_only_targets_d3d11() {
        assert_eq!(
            effective_present_max_fps(
                PRESENT_LIMITER_AUTO_SENTINEL,
                Some(240),
                RtpVideoApi::D3D11,
                Some(165)
            ),
            165
        );
        assert_eq!(
            effective_present_max_fps(
                PRESENT_LIMITER_AUTO_SENTINEL,
                Some(240),
                RtpVideoApi::D3D12,
                Some(165)
            ),
            0
        );
        assert_eq!(
            effective_present_max_fps(144, Some(240), RtpVideoApi::D3D12, Some(165)),
            144
        );
        assert_eq!(
            effective_present_max_fps(0, Some(240), RtpVideoApi::D3D11, Some(165)),
            0
        );
    }

    #[test]
    fn formats_selected_video_chain_diagnostics() {
        let specs =
            rtp_video_chain_definition("H264", RtpVideoApi::Software).expect("software H264");
        let message = format_video_chain_selection("H264", RtpVideoApi::Software, &specs);

        assert!(message.contains("backend=software"));
        assert!(message.contains("decoder=avdec_h264"));
        assert!(message.contains("converter=videoconvert"));
        assert!(message.contains("memory=system-memory"));
    }

    #[test]
    fn extracts_caps_framerate_summary() {
        let caps = "video/x-raw(memory:D3D11Memory), format=(string)NV12, framerate=(fraction)240/1; zeroCopyD3D11=true";
        assert_eq!(caps_framerate_summary(caps).as_deref(), Some("240/1"));
        assert_eq!(caps_framerate_summary("video/x-raw").as_deref(), None);
    }

    #[test]
    fn video_stall_tracker_waits_until_threshold() {
        let mut tracker = VideoStallTracker::default();

        assert_eq!(tracker.evaluate(2_499, 0), VideoStallAction::None);
    }

    #[test]
    fn video_stall_tracker_progresses_recovery_attempts() {
        let mut tracker = VideoStallTracker::default();

        assert_eq!(
            tracker.evaluate(2_500, 0),
            VideoStallAction::RequestKeyframe {
                attempt: 1,
                stall_ms: 2_500,
            },
        );
        assert_eq!(tracker.evaluate(3_000, 0), VideoStallAction::None);
        assert_eq!(
            tracker.evaluate(5_000, 0),
            VideoStallAction::RequestKeyframe {
                attempt: 2,
                stall_ms: 5_000,
            },
        );
        assert_eq!(
            tracker.evaluate(8_000, 0),
            VideoStallAction::Resync {
                attempt: 3,
                stall_ms: 8_000,
            },
        );
        assert_eq!(
            tracker.evaluate(12_000, 0),
            VideoStallAction::PartialFlush {
                attempt: 4,
                stall_ms: 12_000,
            },
        );
        assert_eq!(
            tracker.evaluate(16_000, 0),
            VideoStallAction::CompleteFlush {
                attempt: 5,
                stall_ms: 16_000,
            },
        );
        assert_eq!(
            tracker.evaluate(20_000, 0),
            VideoStallAction::Fatal {
                attempt: 6,
                stall_ms: 20_000,
            },
        );
    }

    #[test]
    fn video_stall_tracker_resets_after_recovery() {
        let mut tracker = VideoStallTracker::default();

        assert_eq!(
            tracker.evaluate(2_500, 0),
            VideoStallAction::RequestKeyframe {
                attempt: 1,
                stall_ms: 2_500,
            },
        );
        assert_eq!(
            tracker.evaluate(2_600, 2_600),
            VideoStallAction::Recovered { stall_ms: 2_600 },
        );
        assert_eq!(tracker.evaluate(3_000, 2_600), VideoStallAction::None);
        assert_eq!(
            tracker.evaluate(5_100, 2_600),
            VideoStallAction::RequestKeyframe {
                attempt: 1,
                stall_ms: 2_500,
            },
        );
    }

    #[test]
    fn resolve_queue_mode_prefers_adaptive_for_240_fps_and_vrr_for_cloud_gsync() {
        let adaptive = resolve_queue_mode(&StreamSettings {
            resolution: "2560x1440".to_owned(),
            fps: 240,
            max_bitrate_mbps: 75,
            codec: VideoCodec::H265,
            color_quality: crate::protocol::ColorQuality::TenBit420,
            enable_cloud_gsync: false,
            native_transition_diagnostics: None,
        });
        assert_eq!(adaptive, NativeQueueMode::Adaptive);

        let vrr = resolve_queue_mode(&StreamSettings {
            resolution: "2560x1440".to_owned(),
            fps: 120,
            max_bitrate_mbps: 75,
            codec: VideoCodec::H265,
            color_quality: crate::protocol::ColorQuality::TenBit420,
            enable_cloud_gsync: true,
            native_transition_diagnostics: None,
        });
        assert_eq!(vrr, NativeQueueMode::Vrr);
    }

    #[test]
    fn reports_missing_sink_stats_as_unavailable() {
        gst::init().expect("gstreamer init");
        let sink = gst::ElementFactory::make("fakesink")
            .build()
            .expect("fakesink");
        assert_eq!(
            sink_stats_summary(&sink),
            "sinkStats rendered=0 dropped=0 averageRate=0.0"
        );
    }
}
