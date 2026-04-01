use std::{collections::HashMap, sync::{atomic::{AtomicBool, AtomicU16, Ordering}, Arc, mpsc::Sender as StdSender}};

use anyhow::{anyhow, Context};
use interceptor::registry::Registry;
use tokio::{sync::{mpsc, Mutex}, time::{timeout, Duration}};
use webrtc::{
    api::{interceptor_registry::register_default_interceptors, media_engine::MediaEngine, setting_engine::SettingEngine, APIBuilder},
    data_channel::{data_channel_init::RTCDataChannelInit, RTCDataChannel},
    dtls_transport::dtls_role::DTLSRole,
    ice_transport::{ice_candidate::RTCIceCandidateInit, ice_server::RTCIceServer},
    peer_connection::{configuration::RTCConfiguration, sdp::session_description::RTCSessionDescription, RTCPeerConnection},
    track::track_remote::TrackRemote,
};

use crate::{
    input,
    media::{MediaEvent, MediaPipeline, VideoSettings},
    messages::{ControlMessage, SessionInfo, StreamSettings, StreamerMessage, StreamerState},
    sdp::{build_nvst_sdp, extract_ice_credentials, extract_ice_ufrag_from_offer, extract_public_ip, fix_server_ip, munge_answer_sdp, normalize_answer_transport_sdp, parse_partial_reliable_threshold_ms, prefer_codec, rewrite_h265_offer},
};

pub struct StreamSession {
    peer: Arc<RTCPeerConnection>,
    reliable: Arc<RTCDataChannel>,
    hid_partially_reliable: Arc<Mutex<Option<Arc<RTCDataChannel>>>>,
    gamepad_channel: Arc<Mutex<Option<Arc<RTCDataChannel>>>>,
    input_protocol_version: Arc<AtomicU16>,
    gamepad_sequence: Arc<Mutex<HashMap<u8, u16>>>,
    control_tx: mpsc::Sender<StreamerMessage>,
    session: SessionInfo,
    settings: StreamSettings,
    media: MediaPipeline,
}

impl StreamSession {
    pub async fn new(
        control_tx: mpsc::Sender<StreamerMessage>,
        session: SessionInfo,
        settings: StreamSettings,
        media_tx: StdSender<MediaEvent>,
    ) -> anyhow::Result<Self> {
        let mut media_engine = MediaEngine::default();
        media_engine.register_default_codecs().context("register_default_codecs")?;
        let mut registry = Registry::new();
        registry = register_default_interceptors(registry, &mut media_engine).context("register_default_interceptors")?;
        let mut setting_engine = SettingEngine::default();
        setting_engine
            .set_answering_dtls_role(DTLSRole::Client)
            .context("set_answering_dtls_role")?;
        setting_engine.set_sdp_media_level_fingerprints(true);
        let api = APIBuilder::new()
            .with_media_engine(media_engine)
            .with_setting_engine(setting_engine)
            .with_interceptor_registry(registry)
            .build();
        let config = RTCConfiguration {
            ice_servers: session.ice_servers.iter().map(|server| RTCIceServer {
                urls: server.urls.clone(),
                username: server.username.clone().unwrap_or_default(),
                credential: server.credential.clone().unwrap_or_default(),
                ..Default::default()
            }).collect(),
            ..Default::default()
        };
        let peer = Arc::new(api.new_peer_connection(config).await.context("new_peer_connection")?);
        let reliable = peer.create_data_channel("input_channel_v1", Some(RTCDataChannelInit { ordered: Some(true), ..Default::default() })).await?;
        let hid_partially_reliable = Arc::new(Mutex::new(None));
        let gamepad_channel = Arc::new(Mutex::new(None));
        let input_protocol_version = Arc::new(AtomicU16::new(2));
        let gamepad_sequence = Arc::new(Mutex::new(HashMap::new()));

        let width = settings.resolution.split('x').next().and_then(|v| v.parse::<u32>().ok()).unwrap_or(1920);
        let height = settings.resolution.split('x').nth(1).and_then(|v| v.parse::<u32>().ok()).unwrap_or(1080);
        let media = MediaPipeline::new(media_tx, control_tx.clone(), VideoSettings { width, height, codec: settings.codec.clone() });
        control_tx.send(StreamerMessage::Log {
            level: "info".into(),
            message: format!(
                "configured native session {} {} {}fps codec={} bitrate={}mbps",
                session.session_id,
                settings.resolution,
                settings.fps,
                settings.codec,
                settings.max_bitrate_mbps,
                ),
        }).await.ok();

        let handshake_log_tx = control_tx.clone();
        let handshake_channel = reliable.clone();
        let handshake_protocol_version = input_protocol_version.clone();
        let heartbeat_started = Arc::new(AtomicBool::new(false));
        let heartbeat_started_for_handler = heartbeat_started.clone();
        reliable.on_message(Box::new(move |message| {
            let sender = handshake_log_tx.clone();
            let channel = handshake_channel.clone();
            let protocol_version = handshake_protocol_version.clone();
            let heartbeat_started = heartbeat_started_for_handler.clone();
            Box::pin(async move {
                if let Some(version) = input::parse_input_handshake(message.data.as_ref()) {
                    protocol_version.store(version, Ordering::Relaxed);
                    let _ = sender.send(StreamerMessage::Log {
                        level: "info".into(),
                        message: format!("input handshake complete (protocol v{version})"),
                    }).await;
                    if !heartbeat_started.swap(true, Ordering::SeqCst) {
                        let sender = sender.clone();
                        tokio::spawn(async move {
                            let mut interval = tokio::time::interval(Duration::from_secs(2));
                            loop {
                                interval.tick().await;
                                if channel.ready_state().to_string().as_str() != "open" {
                                    break;
                                }
                                if channel.send(&input::encode_heartbeat().into()).await.is_err() {
                                    break;
                                }
                            }
                            let _ = sender.send(StreamerMessage::Log {
                                level: "debug".into(),
                                message: "input heartbeat loop stopped".into(),
                            }).await;
                        });
                    }
                    return;
                }
                let hex = message
                    .data
                    .iter()
                    .take(16)
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                let _ = sender.send(StreamerMessage::Log {
                    level: "debug".into(),
                    message: format!("input channel message {} bytes [{}]", message.data.len(), hex),
                }).await;
            })
        }));

        let control_clone = control_tx.clone();
        peer.on_ice_candidate(Box::new(move |candidate| {
            let sender = control_clone.clone();
            Box::pin(async move {
                if let Some(candidate) = candidate {
                    if let Ok(json) = candidate.to_json() {
                        let normalized_mid = json.sdp_mid.and_then(|mid| if mid.is_empty() { None } else { Some(mid) })
                            .or_else(|| json.sdp_mline_index.map(|_| "0".to_string()));
                        let _ = sender.send(StreamerMessage::LocalIce {
                            candidate: json.candidate,
                            sdp_mid: normalized_mid,
                            sdp_m_line_index: json.sdp_mline_index,
                        }).await;
                    }
                }
            })
        }));

        let control_clone = control_tx.clone();
        peer.on_ice_connection_state_change(Box::new(move |state| {
            let sender = control_clone.clone();
            Box::pin(async move {
                let _ = sender.send(StreamerMessage::Log {
                    level: "info".into(),
                    message: format!("ice connection state {state}"),
                }).await;
            })
        }));

        let control_clone = control_tx.clone();
        peer.on_signaling_state_change(Box::new(move |state| {
            let sender = control_clone.clone();
            Box::pin(async move {
                let _ = sender.send(StreamerMessage::Log {
                    level: "info".into(),
                    message: format!("signaling state {state}"),
                }).await;
            })
        }));

        let control_clone = control_tx.clone();
        peer.dtls_transport().on_state_change(Box::new(move |state| {
            let sender = control_clone.clone();
            Box::pin(async move {
                let _ = sender.send(StreamerMessage::Log {
                    level: "info".into(),
                    message: format!("dtls transport state {state}"),
                }).await;
            })
        }));

        let control_clone = control_tx.clone();
        peer.on_peer_connection_state_change(Box::new(move |state| {
            let sender = control_clone.clone();
            Box::pin(async move {
                let _ = sender.send(StreamerMessage::Log {
                    level: "info".into(),
                    message: format!("peer connection state {state}"),
                }).await;
                let mapped = match state.to_string().as_str() {
                    "connected" => StreamerState::Connected,
                    "failed" => StreamerState::Failed,
                    "disconnected" | "closed" => StreamerState::Disconnected,
                    _ => StreamerState::Connecting,
                };
                let _ = sender.send(StreamerMessage::State { state: mapped, detail: Some(state.to_string()) }).await;
            })
        }));

        let control_clone = control_tx.clone();
        peer.on_data_channel(Box::new(move |channel| {
            let sender = control_clone.clone();
            Box::pin(async move {
                let label = channel.label().to_string();
                let _ = sender.send(StreamerMessage::Log {
                    level: "info".into(),
                    message: format!("remote data channel label={label}"),
                }).await;
                let open_sender = sender.clone();
                let open_label = label.clone();
                channel.on_open(Box::new(move || {
                    let sender = open_sender.clone();
                    let label = open_label.clone();
                    Box::pin(async move {
                        let _ = sender.send(StreamerMessage::Log {
                            level: "info".into(),
                            message: format!("remote data channel opened label={label}"),
                        }).await;
                    })
                }));
                let message_sender = sender.clone();
                let message_label = label.clone();
                channel.on_message(Box::new(move |message| {
                    let sender = message_sender.clone();
                    let label = message_label.clone();
                    Box::pin(async move {
                        if message.is_string {
                            let text = String::from_utf8_lossy(message.data.as_ref()).into_owned();
                            let _ = sender.send(StreamerMessage::Log {
                                level: "info".into(),
                                message: format!("remote data channel {label}: {text}"),
                            }).await;
                            return;
                        }
                        let hex = message
                            .data
                            .iter()
                            .take(16)
                            .map(|byte| format!("{byte:02x}"))
                            .collect::<Vec<_>>()
                            .join(" ");
                        let _ = sender.send(StreamerMessage::Log {
                            level: "debug".into(),
                            message: format!("remote data channel {label}: {} bytes [{}]", message.data.len(), hex),
                        }).await;
                    })
                }));
            })
        }));

        let media_clone = media.clone();
        let control_clone = control_tx.clone();
        peer.on_track(Box::new(move |track: Arc<TrackRemote>, _, _| {
            let media = media_clone.clone();
            let control = control_clone.clone();
            Box::pin(async move {
                let mime = track.codec().capability.mime_type.clone();
                let _ = control.send(StreamerMessage::Log { level: "info".into(), message: format!("track {mime}") }).await;
                if mime.to_lowercase().contains("video") {
                    if let Err(error) = media.attach_video_track(track).await {
                        let _ = control.send(StreamerMessage::Error { message: format!("attach video track failed: {error:#}") }).await;
                    }
                } else if mime.to_lowercase().contains("audio") {
                    if let Err(error) = media.attach_audio_track(track).await {
                        let _ = control.send(StreamerMessage::Error { message: format!("attach audio track failed: {error:#}") }).await;
                    }
                }
            })
        }));

        Ok(Self {
            peer,
            reliable,
            hid_partially_reliable,
            gamepad_channel,
            input_protocol_version,
            gamepad_sequence,
            control_tx,
            session,
            settings,
            media,
        })
    }

    pub async fn apply_offer(&self, offer_sdp: String) -> anyhow::Result<()> {
        self.control_tx.send(StreamerMessage::Log {
            level: "info".into(),
            message: format!("applying remote offer ({} chars)", offer_sdp.len()),
        }).await.ok();
        let partial_reliable = parse_partial_reliable_threshold_ms(&offer_sdp).unwrap_or(30);
        if self.hid_partially_reliable.lock().await.is_none() {
            let channel = self.peer.create_data_channel(
                "input_channel_partially_reliable",
                Some(RTCDataChannelInit { ordered: Some(false), max_packet_life_time: Some(partial_reliable), ..Default::default() }),
            ).await?;
            *self.hid_partially_reliable.lock().await = Some(channel);
        }
        if self.gamepad_channel.lock().await.is_none() {
            let channel = self.peer.create_data_channel(
                "gamepad_channel_v1",
                Some(RTCDataChannelInit { ordered: Some(false), max_packet_life_time: Some(partial_reliable), ..Default::default() }),
            ).await?;
            *self.gamepad_channel.lock().await = Some(channel);
        }
        let server_ip_for_sdp = self.session.media_connection_info.as_ref().map(|m| m.ip.as_str()).unwrap_or(self.session.server_ip.as_str());
        let mut processed = fix_server_ip(&offer_sdp, server_ip_for_sdp);
        if self.settings.codec.eq_ignore_ascii_case("H265") {
            processed = rewrite_h265_offer(&processed);
        }
        processed = prefer_codec(&processed, &self.settings.codec);
        let server_ufrag = extract_ice_ufrag_from_offer(&processed);

        self.control_tx.send(StreamerMessage::Log {
            level: "info".into(),
            message: format!("setting remote description ({} chars)", processed.len()),
        }).await.ok();
        timeout(Duration::from_secs(5), self.peer.set_remote_description(RTCSessionDescription::offer(processed)?))
            .await
            .map_err(|_| anyhow!("timed out setting remote description"))??;
        self.control_tx.send(StreamerMessage::Log {
            level: "info".into(),
            message: "remote description applied".into(),
        }).await.ok();
        let transceivers = self.peer.get_transceivers().await;
        self.control_tx.send(StreamerMessage::Log {
            level: "info".into(),
            message: format!("transceivers after remote description: {}", transceivers.len()),
        }).await.ok();

        self.control_tx.send(StreamerMessage::Log {
            level: "info".into(),
            message: "creating local answer".into(),
        }).await.ok();
        let answer = timeout(Duration::from_secs(5), self.peer.create_answer(None))
            .await
            .map_err(|_| anyhow!("timed out creating answer"))??;
        let mut gather_complete = self.peer.gathering_complete_promise().await;
        self.control_tx.send(StreamerMessage::Log {
            level: "info".into(),
            message: format!("setting local description ({} chars)", answer.sdp.len()),
        }).await.ok();
        timeout(Duration::from_secs(5), self.peer.set_local_description(answer))
            .await
            .map_err(|_| anyhow!("timed out setting local description"))??;
        self.control_tx.send(StreamerMessage::Log {
            level: "info".into(),
            message: "local description applied".into(),
        }).await.ok();
        let _ = timeout(Duration::from_secs(5), gather_complete.recv())
            .await
            .map_err(|_| anyhow!("timed out waiting for ICE gathering"))?;
        self.control_tx.send(StreamerMessage::Log {
            level: "info".into(),
            message: "ice gathering completed".into(),
        }).await.ok();
        let local = self.peer.local_description().await.ok_or_else(|| anyhow!("missing local description"))?;
        let width = self.settings.resolution.split('x').next().and_then(|v| v.parse::<u32>().ok()).unwrap_or(1920);
        let height = self.settings.resolution.split('x').nth(1).and_then(|v| v.parse::<u32>().ok()).unwrap_or(1080);
        if let Some(setup_line) = local.sdp.lines().find(|line| line.trim().starts_with("a=setup:")) {
            self.control_tx.send(StreamerMessage::Log {
                level: "info".into(),
                message: format!("local answer DTLS setup line {setup_line}"),
            }).await.ok();
        }
        let credentials = extract_ice_credentials(&local.sdp);
        self.control_tx.send(StreamerMessage::Log {
            level: "info".into(),
            message: format!(
                "local ICE credentials ufrag={} pwd={}... fingerprint={}...",
                credentials.ufrag,
                credentials.pwd.chars().take(8).collect::<String>(),
                credentials.fingerprint.chars().take(20).collect::<String>(),
            ),
        }).await.ok();
        let munged_local_sdp = munge_answer_sdp(&local.sdp, u32::from(self.settings.max_bitrate_mbps) * 1000);
        let normalized_local_sdp = normalize_answer_transport_sdp(&munged_local_sdp);
        let negotiated_video_lines = normalized_local_sdp
            .lines()
            .scan(false, |in_video, line| {
                if line.starts_with("m=video") {
                    *in_video = true;
                    return Some(Some(line.to_string()));
                }
                if line.starts_with("m=") && *in_video {
                    *in_video = false;
                    return Some(None);
                }
                if *in_video && (line.starts_with("a=rtpmap:") || line.starts_with("a=fmtp:") || line.starts_with("a=rtcp-fb:")) {
                    return Some(Some(line.to_string()));
                }
                Some(None)
            })
            .flatten()
            .collect::<Vec<_>>()
            .join(" | ");
        self.control_tx.send(StreamerMessage::Log {
            level: "info".into(),
            message: format!("negotiated local video SDP: {negotiated_video_lines}"),
        }).await.ok();
        if let Some(setup_line) = normalized_local_sdp.lines().find(|line| line.trim().starts_with("a=setup:")) {
            self.control_tx.send(StreamerMessage::Log {
                level: "info".into(),
                message: format!("normalized answer DTLS setup line {setup_line}"),
            }).await.ok();
        }
        let fingerprint_count = normalized_local_sdp.lines().filter(|line| line.trim().starts_with("a=fingerprint:")).count();
        self.control_tx.send(StreamerMessage::Log {
            level: "info".into(),
            message: format!("normalized answer fingerprint lines {fingerprint_count}"),
        }).await.ok();
        let nvst = build_nvst_sdp(
            &self.settings.resolution,
            width,
            height,
            self.settings.fps,
            self.settings.max_bitrate_mbps,
            &self.settings.codec,
            &self.settings.color_quality,
            partial_reliable,
            &credentials,
        );
        self.control_tx.send(StreamerMessage::Log {
            level: "info".into(),
            message: format!("sending local answer ({} chars) and nvst blob ({} chars)", normalized_local_sdp.len(), nvst.len()),
        }).await.ok();
        self.control_tx.send(StreamerMessage::Answer { sdp: normalized_local_sdp, nvst_sdp: nvst }).await.ok();

        if let Some(mci) = &self.session.media_connection_info {
            if let Some(ip) = extract_public_ip(&mci.ip) {
                let candidate = format!("candidate:1 1 udp 2130706431 {ip} {} typ host", mci.port);
                self.control_tx.send(StreamerMessage::Log {
                    level: "info".into(),
                    message: format!("injecting manual ICE candidate {candidate}"),
                }).await.ok();
                for mid in ["0", "1", "2", "3"] {
                    let res = self.peer.add_ice_candidate(RTCIceCandidateInit {
                        candidate: candidate.clone(),
                        sdp_mid: Some(mid.to_string()),
                        sdp_mline_index: mid.parse::<u16>().ok(),
                        username_fragment: Some(server_ufrag.clone()),
                    }).await;
                    if res.is_ok() {
                        self.control_tx.send(StreamerMessage::Log {
                            level: "info".into(),
                            message: format!("manual ICE candidate accepted on sdpMid={mid}"),
                        }).await.ok();
                        break;
                    } else if let Err(error) = res {
                        self.control_tx.send(StreamerMessage::Log {
                            level: "warn".into(),
                            message: format!("manual ICE candidate failed on sdpMid={mid}: {error}"),
                        }).await.ok();
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn add_remote_ice(&self, candidate: String, sdp_mid: Option<String>, sdp_m_line_index: Option<u16>) -> anyhow::Result<()> {
        let normalized_mid = sdp_mid.or_else(|| sdp_m_line_index.map(|_| "0".to_string()));
        self.control_tx.send(StreamerMessage::Log {
            level: "info".into(),
            message: format!(
                "adding remote ICE candidate (mid={}, mline={})",
                normalized_mid.clone().unwrap_or_else(|| "null".into()),
                sdp_m_line_index.map(|value| value.to_string()).unwrap_or_else(|| "null".into()),
            ),
        }).await.ok();
        self.peer.add_ice_candidate(RTCIceCandidateInit {
            candidate,
            sdp_mid: normalized_mid,
            sdp_mline_index: sdp_m_line_index,
            username_fragment: None,
        }).await?;
        Ok(())
    }

    pub async fn send_input(&self, payload: InputPayload) {
        match payload {
            InputPayload::Key { key_code, scan_code, modifiers, down } => {
                let bytes = input::encode_key(key_code, scan_code, modifiers, down, self.input_protocol_version.load(Ordering::Relaxed));
                let _ = self.reliable.send(&bytes.into()).await;
            }
            InputPayload::MouseMove { dx, dy } => {
                let bytes = input::encode_mouse_move(dx, dy, self.input_protocol_version.load(Ordering::Relaxed));
                let _ = self.reliable.send(&bytes.into()).await;
            }
            InputPayload::MouseButton { button, down } => {
                let bytes = input::encode_mouse_button(button, down, self.input_protocol_version.load(Ordering::Relaxed));
                let _ = self.reliable.send(&bytes.into()).await;
            }
            InputPayload::MouseWheel { delta } => {
                let bytes = input::encode_mouse_wheel(delta, self.input_protocol_version.load(Ordering::Relaxed));
                let _ = self.reliable.send(&bytes.into()).await;
            }
            InputPayload::Gamepad { controller_id, bitmap, buttons, left_trigger, right_trigger, left_x, left_y, right_x, right_y } => {
                let protocol_version = self.input_protocol_version.load(Ordering::Relaxed);
                let use_partially_reliable = self.gamepad_channel.lock().await.is_some();
                let sequence = {
                    let mut sequences = self.gamepad_sequence.lock().await;
                    let current = sequences.get(&controller_id).copied().unwrap_or(1);
                    sequences.insert(controller_id, current.wrapping_add(1));
                    current
                };
                let bytes = input::encode_gamepad(&input::GamepadState {
                    controller_id,
                    bitmap,
                    buttons,
                    left_trigger,
                    right_trigger,
                    left_x,
                    left_y,
                    right_x,
                    right_y,
                }, use_partially_reliable, sequence, protocol_version);
                if let Some(channel) = self.gamepad_channel.lock().await.clone() {
                    let _ = channel.send(&bytes.into()).await;
                } else {
                    let _ = self.reliable.send(&bytes.into()).await;
                }
            }
        }
    }

    pub async fn close(&self) {
        let _ = self.peer.close().await;
    }
}

#[derive(Clone)]
pub enum InputPayload {
    Key { key_code: u16, scan_code: u16, modifiers: u16, down: bool },
    MouseMove { dx: i16, dy: i16 },
    MouseButton { button: u8, down: bool },
    MouseWheel { delta: i16 },
    Gamepad { controller_id: u8, bitmap: u16, buttons: u16, left_trigger: u8, right_trigger: u8, left_x: i16, left_y: i16, right_x: i16, right_y: i16 },
}

pub type SharedSession = Arc<Mutex<Option<Arc<StreamSession>>>>;

pub async fn handle_control_message(
    active: &SharedSession,
    control_tx: &mpsc::Sender<StreamerMessage>,
    media_tx: &StdSender<MediaEvent>,
    message: ControlMessage,
) -> anyhow::Result<bool> {
    match message {
        ControlMessage::Configure { session, settings } => {
            if let Some(current) = active.lock().await.take() {
                current.close().await;
            }
            let session = Arc::new(StreamSession::new(control_tx.clone(), session, settings, media_tx.clone()).await?);
            *active.lock().await = Some(session);
            control_tx.send(StreamerMessage::State { state: StreamerState::Connecting, detail: Some("configured".into()) }).await.ok();
            Ok(true)
        }
        ControlMessage::SignalingOffer { sdp } => {
            let session = active.lock().await.clone().ok_or_else(|| anyhow!("no active session"))?;
            session.apply_offer(sdp).await?;
            Ok(true)
        }
        ControlMessage::SignalingRemoteIce { candidate, sdp_mid, sdp_m_line_index } => {
            if let Some(session) = active.lock().await.clone() {
                session.add_remote_ice(candidate, sdp_mid, sdp_m_line_index).await?;
            }
            Ok(true)
        }
        ControlMessage::Stop => {
            if let Some(session) = active.lock().await.take() {
                session.close().await;
            }
            control_tx.send(StreamerMessage::State { state: StreamerState::Idle, detail: Some("stopped".into()) }).await.ok();
            Ok(false)
        }
        ControlMessage::Ping => {
            control_tx.send(StreamerMessage::Log { level: "debug".into(), message: "pong".into() }).await.ok();
            Ok(true)
        }
    }
}
