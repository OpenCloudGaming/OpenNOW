//! WebRTC Peer Connection
//!
//! Handles WebRTC peer connection, media streams, and data channels.

use std::sync::Arc;
use tokio::sync::mpsc;
use parking_lot::Mutex;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::setting_engine::SettingEngine;
use webrtc::api::APIBuilder;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::data_channel::RTCDataChannel;
use webrtc::dtls_transport::dtls_role::DTLSRole;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::ice_transport::ice_gatherer_state::RTCIceGathererState;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtp_transceiver::rtp_codec::{RTCRtpCodecCapability, RTCRtpCodecParameters, RTPCodecType};
use webrtc::rtp_transceiver::rtp_transceiver_direction::RTCRtpTransceiverDirection;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpHeaderExtensionCapability;
use webrtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use anyhow::{Result, Context};
use log::{info, debug, warn, error};
use bytes::Bytes;

/// MIME type for H265/HEVC video codec
const MIME_TYPE_H265: &str = "video/H265";
/// MIME type for AV1 video codec
const MIME_TYPE_AV1: &str = "video/AV1";

use super::InputEncoder;
use super::sdp::is_ice_lite;

/// Events from WebRTC connection
#[derive(Debug)]
pub enum WebRtcEvent {
    Connected,
    Disconnected,
    /// Video frame with RTP timestamp (90kHz clock) and marker bit
    VideoFrame { payload: Vec<u8>, rtp_timestamp: u32, marker: bool },
    AudioFrame(Vec<u8>),
    DataChannelOpen(String),
    DataChannelMessage(String, Vec<u8>),
    IceCandidate(String, Option<String>, Option<u16>),
    Error(String),
    /// Stream stalled due to SSRC change - reconnection recommended
    /// This happens when GFN server changes resolution and switches to a new SSRC
    /// that webrtc-rs can't handle without MID header extensions.
    SsrcChangeDetected { stall_duration_ms: u64 },
}

/// Shared peer connection for PLI requests (static to allow access from decoder)
static PEER_CONNECTION: Mutex<Option<Arc<RTCPeerConnection>>> = Mutex::new(None);
/// Track SSRC for PLI
static VIDEO_SSRC: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// WebRTC peer for GFN streaming
pub struct WebRtcPeer {
    peer_connection: Option<Arc<RTCPeerConnection>>,
    input_channel: Option<Arc<RTCDataChannel>>,
    /// Partially reliable channel for mouse (lower latency, unordered)
    mouse_channel: Option<Arc<RTCDataChannel>>,
    event_tx: mpsc::Sender<WebRtcEvent>,
    input_encoder: InputEncoder,
    handshake_complete: bool,
}

/// Request a keyframe (PLI - Picture Loss Indication)
/// Call this when decode errors occur to recover the stream
pub async fn request_keyframe() {
    let pc = PEER_CONNECTION.lock().clone();
    let ssrc = VIDEO_SSRC.load(std::sync::atomic::Ordering::Relaxed);

    if let Some(pc) = pc {
        if ssrc != 0 {
            let pli = PictureLossIndication {
                sender_ssrc: 0,
                media_ssrc: ssrc,
            };

            match pc.write_rtcp(&[Box::new(pli)]).await {
                Ok(_) => info!("Sent PLI (keyframe request) for SSRC {}", ssrc),
                Err(e) => warn!("Failed to send PLI: {:?}", e),
            }
        } else {
            debug!("Cannot send PLI: no video SSRC yet");
        }
    } else {
        debug!("Cannot send PLI: no peer connection");
    }
}

impl WebRtcPeer {
    pub fn new(event_tx: mpsc::Sender<WebRtcEvent>) -> Self {
        Self {
            peer_connection: None,
            input_channel: None,
            mouse_channel: None,
            event_tx,
            input_encoder: InputEncoder::new(),
            handshake_complete: false,
        }
    }

    /// Create peer connection and set remote SDP offer
    pub async fn handle_offer(&mut self, sdp_offer: &str, ice_servers: Vec<RTCIceServer>) -> Result<String> {
        info!("Setting up WebRTC peer connection");

        // Detect ice-lite BEFORE creating peer connection - this affects DTLS role
        let offer_is_ice_lite = is_ice_lite(sdp_offer);
        if offer_is_ice_lite {
            info!("Server is ice-lite - will configure active DTLS role (Client)");
        }

        // Create media engine with all required codecs
        let mut media_engine = MediaEngine::default();

        // Register default codecs (H264, VP8, VP9, Opus, etc.)
        media_engine.register_default_codecs()?;

        // Register H265/HEVC codec (not in default codecs!)
        // Use payload_type 0 for dynamic payload type negotiation from SDP
        media_engine.register_codec(
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_H265.to_string(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "".to_string(),
                    rtcp_feedback: vec![],
                },
                payload_type: 0, // Dynamic - will be negotiated from SDP
                ..Default::default()
            },
            RTPCodecType::Video,
        )?;
        info!("Registered H265/HEVC codec");

        // Register AV1 codec (for future use)
        media_engine.register_codec(
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_AV1.to_string(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "".to_string(),
                    rtcp_feedback: vec![],
                },
                payload_type: 0, // Dynamic - will be negotiated from SDP
                ..Default::default()
            },
            RTPCodecType::Video,
        )?;
        info!("Registered AV1 codec");

        // Register RTP header extensions for SSRC demuxing
        // These are required to handle mid-stream SSRC changes and simulcast
        // MID extension - identifies which media section an RTP packet belongs to
        media_engine.register_header_extension(
            RTCRtpHeaderExtensionCapability {
                uri: "urn:ietf:params:rtp-hdrext:sdes:mid".to_string(),
            },
            RTPCodecType::Video,
            Some(RTCRtpTransceiverDirection::Recvonly),
        )?;
        media_engine.register_header_extension(
            RTCRtpHeaderExtensionCapability {
                uri: "urn:ietf:params:rtp-hdrext:sdes:mid".to_string(),
            },
            RTPCodecType::Audio,
            Some(RTCRtpTransceiverDirection::Recvonly),
        )?;
        info!("Registered SDES MID header extension for video and audio");

        // RTP Stream ID extension - identifies specific streams in simulcast
        media_engine.register_header_extension(
            RTCRtpHeaderExtensionCapability {
                uri: "urn:ietf:params:rtp-hdrext:sdes:rtp-stream-id".to_string(),
            },
            RTPCodecType::Video,
            Some(RTCRtpTransceiverDirection::Recvonly),
        )?;
        info!("Registered SDES RTP-Stream-ID header extension");

        // Repaired RTP Stream ID extension - required for SSRC changes during stream
        // This allows webrtc-rs to handle mid-stream SSRC switches (e.g., HDR mode changes)
        media_engine.register_header_extension(
            RTCRtpHeaderExtensionCapability {
                uri: "urn:ietf:params:rtp-hdrext:sdes:repaired-rtp-stream-id".to_string(),
            },
            RTPCodecType::Video,
            Some(RTCRtpTransceiverDirection::Recvonly),
        )?;
        info!("Registered SDES Repaired-RTP-Stream-ID header extension");

        // Create interceptor registry
        let mut registry = Registry::new();
        registry = register_default_interceptors(registry, &mut media_engine)?;

        // Create setting engine - configure DTLS role for ice-lite
        let mut setting_engine = SettingEngine::default();
        if offer_is_ice_lite {
            // When server is ice-lite, we MUST be DTLS Client (active/initiator)
            // This makes us send the DTLS ClientHello to start the handshake
            setting_engine.set_answering_dtls_role(DTLSRole::Client)?;
            info!("Configured DTLS role to Client (active) for ice-lite server");
        }

        // Enable provisional SSRC support for GFN resolution changes
        // GFN server uses new SSRCs when changing resolution, and doesn't send MID extensions
        // This allows undeclared SSRCs to be routed to existing video transceivers
        setting_engine.set_allow_provisional_ssrc(true);
        info!("Enabled provisional SSRC support for GFN compatibility");

        // Create API with setting engine
        let api = APIBuilder::new()
            .with_media_engine(media_engine)
            .with_interceptor_registry(registry)
            .with_setting_engine(setting_engine)
            .build();

        // Create RTCConfiguration
        let config = RTCConfiguration {
            ice_servers,
            ..Default::default()
        };

        // Create peer connection
        let peer_connection = Arc::new(api.new_peer_connection(config).await?);
        info!("Peer connection created");

        // Set up event handlers
        let event_tx = self.event_tx.clone();

        // On ICE candidate
        let event_tx_ice = event_tx.clone();
        peer_connection.on_ice_candidate(Box::new(move |candidate| {
            let tx = event_tx_ice.clone();
            Box::pin(async move {
                if let Some(c) = candidate {
                    let candidate_str = c.to_json().map(|j| j.candidate).unwrap_or_default();
                    info!("Gathered local ICE candidate: {}", candidate_str);
                    let sdp_mid = c.to_json().ok().and_then(|j| j.sdp_mid);
                    let sdp_mline_index = c.to_json().ok().and_then(|j| j.sdp_mline_index);
                    let _ = tx.send(WebRtcEvent::IceCandidate(
                        candidate_str,
                        sdp_mid,
                        sdp_mline_index,
                    )).await;
                }
            })
        }));

        // On ICE connection state change
        let event_tx_state = event_tx.clone();
        peer_connection.on_ice_connection_state_change(Box::new(move |state| {
            let tx = event_tx_state.clone();
            info!("ICE connection state: {:?}", state);
            Box::pin(async move {
                match state {
                    webrtc::ice_transport::ice_connection_state::RTCIceConnectionState::Connected => {
                        let _ = tx.send(WebRtcEvent::Connected).await;
                    }
                    webrtc::ice_transport::ice_connection_state::RTCIceConnectionState::Disconnected |
                    webrtc::ice_transport::ice_connection_state::RTCIceConnectionState::Failed => {
                        let _ = tx.send(WebRtcEvent::Disconnected).await;
                    }
                    _ => {}
                }
            })
        }));

        // On peer connection state change (includes DTLS state)
        let _pc_for_state = peer_connection.clone();
        peer_connection.on_peer_connection_state_change(Box::new(move |state| {
            info!("Peer connection state: {:?}", state);
            Box::pin(async move {
                match state {
                    webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState::Connected => {
                        info!("=== DTLS HANDSHAKE COMPLETE - FULLY CONNECTED ===");
                    }
                    webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState::Failed => {
                        warn!("Peer connection FAILED (likely DTLS handshake failure)");
                    }
                    _ => {}
                }
            })
        }));

        // On track (video/audio)
        let event_tx_track = event_tx.clone();
        peer_connection.on_track(Box::new(move |track, _receiver, _transceiver| {
            let tx = event_tx_track.clone();
            let track = track.clone();
            let track_kind = track.kind();
            let track_id = track.id().to_string();
            info!("Track received: kind={:?}, id={}, codec={:?}", track_kind, track_id, track.codec());

            // IMPORTANT: Spawn a separate tokio task for reading from the track
            // The Future returned from on_track callback may not be properly spawned by webrtc-rs
            let tx_clone = tx.clone();
            let track_clone = track.clone();
            let track_id_clone = track_id.clone();
            tokio::spawn(async move {
                let mut buffer = vec![0u8; 1500];
                let mut packet_count: u64 = 0;
                let mut last_packet_time = std::time::Instant::now();
                let mut stall_warning_sent = false;
                const STALL_TIMEOUT_MS: u64 = 2000; // 2 seconds without packets = stall

                info!("=== Starting track read loop for {} ({}) ===",
                    track_id_clone,
                    if track_kind == webrtc::rtp_transceiver::rtp_codec::RTPCodecType::Video { "VIDEO" } else { "AUDIO" });

                loop {
                    // Use timeout on read to detect stalls (e.g., SSRC change that we can't handle)
                    let read_timeout = tokio::time::Duration::from_millis(500);
                    match tokio::time::timeout(read_timeout, track_clone.read(&mut buffer)).await {
                        Ok(Ok((rtp_packet, _))) => {
                            packet_count += 1;
                            last_packet_time = std::time::Instant::now();
                            stall_warning_sent = false;

                            // Store SSRC for PLI on first video packet
                            if packet_count == 1 {
                                info!("[{}] First RTP packet: {} bytes payload, SSRC: {}",
                                    track_id_clone, rtp_packet.payload.len(), rtp_packet.header.ssrc);

                                if track_kind == webrtc::rtp_transceiver::rtp_codec::RTPCodecType::Video {
                                    VIDEO_SSRC.store(rtp_packet.header.ssrc, std::sync::atomic::Ordering::Relaxed);

                                    // Request keyframe immediately when video track starts
                                    // This ensures we get an IDR frame to begin decoding
                                    info!("Video track started - requesting initial keyframe");
                                    let pc_clone = PEER_CONNECTION.lock().clone();
                                    if let Some(pc) = pc_clone {
                                        let pli = PictureLossIndication {
                                            sender_ssrc: 0,
                                            media_ssrc: rtp_packet.header.ssrc,
                                        };
                                        if let Err(e) = pc.write_rtcp(&[Box::new(pli)]).await {
                                            warn!("Failed to send initial PLI: {:?}", e);
                                        } else {
                                            info!("Sent initial PLI for SSRC {}", rtp_packet.header.ssrc);
                                        }
                                    }
                                }
                            }

                            if track_kind == webrtc::rtp_transceiver::rtp_codec::RTPCodecType::Video {
                                if let Err(e) = tx_clone.send(WebRtcEvent::VideoFrame {
                                    payload: rtp_packet.payload.to_vec(),
                                    rtp_timestamp: rtp_packet.header.timestamp,
                                    marker: rtp_packet.header.marker,
                                }).await {
                                    warn!("Failed to send video frame event: {:?}", e);
                                    break;
                                }
                            } else {
                                if let Err(e) = tx_clone.send(WebRtcEvent::AudioFrame(rtp_packet.payload.to_vec())).await {
                                    warn!("Failed to send audio frame event: {:?}", e);
                                    break;
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            // Check if this is a transient error or connection closed
                            let error_str = format!("{:?}", e);
                            if error_str.contains("EOF") || error_str.contains("closed") {
                                info!("Track {} connection closed: {}", track_id_clone, e);
                                break;
                            } else {
                                error!("Track {} read error (will retry): {}", track_id_clone, e);
                                // Don't break on transient errors - continue reading
                                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                                continue;
                            }
                        }
                        Err(_timeout) => {
                            // Read timed out - check for stall
                            let elapsed_ms = last_packet_time.elapsed().as_millis() as u64;

                            if elapsed_ms > STALL_TIMEOUT_MS && !stall_warning_sent {
                                // Video track has stalled - likely SSRC changed
                                if track_kind == webrtc::rtp_transceiver::rtp_codec::RTPCodecType::Video {
                                    warn!("Video track stalled for {}ms - SSRC change detected (server switched stream ID). This is a webrtc-rs limitation.", elapsed_ms);
                                    stall_warning_sent = true;

                                    // Send SSRC change event to trigger auto-reconnect
                                    let _ = tx_clone.send(WebRtcEvent::SsrcChangeDetected {
                                        stall_duration_ms: elapsed_ms,
                                    }).await;

                                    // Also send user-visible error
                                    let _ = tx_clone.send(WebRtcEvent::Error(
                                        "Stream interrupted: Server changed video stream (SSRC change). Auto-reconnect recommended.".to_string()
                                    )).await;

                                    // Try to request keyframe in case it helps (unlikely but worth trying)
                                    let ssrc = VIDEO_SSRC.load(std::sync::atomic::Ordering::Relaxed);
                                    if ssrc != 0 {
                                        let pc_clone = PEER_CONNECTION.lock().clone();
                                        if let Some(pc) = pc_clone {
                                            let pli = PictureLossIndication {
                                                sender_ssrc: 0,
                                                media_ssrc: ssrc,
                                            };
                                            let _ = pc.write_rtcp(&[Box::new(pli)]).await;
                                        }
                                    }
                                }
                            }
                            // Continue loop to try reading again
                            continue;
                        }
                    }
                }
                error!("Track {} read loop ended after {} packets - THIS SHOULD NOT HAPPEN DURING STREAM",
                    track_id_clone, packet_count);
            });

            // Return empty future since we spawned the actual work
            Box::pin(async {})
        }));

        // On data channel
        let event_tx_dc = event_tx.clone();
        peer_connection.on_data_channel(Box::new(move |dc| {
            let tx = event_tx_dc.clone();
            let dc_label = dc.label().to_string();
            info!("Data channel received: {}", dc_label);

            Box::pin(async move {
                let label = dc_label.clone();

                let tx_open = tx.clone();
                let label_open = label.clone();
                dc.on_open(Box::new(move || {
                    let tx = tx_open.clone();
                    let label = label_open.clone();
                    Box::pin(async move {
                        info!("Data channel '{}' opened", label);
                        let _ = tx.send(WebRtcEvent::DataChannelOpen(label)).await;
                    })
                }));

                let tx_msg = tx.clone();
                let label_msg = label.clone();
                dc.on_message(Box::new(move |msg| {
                    let tx = tx_msg.clone();
                    let label = label_msg.clone();
                    Box::pin(async move {
                        debug!("Data channel '{}' message: {} bytes", label, msg.data.len());
                        let _ = tx.send(WebRtcEvent::DataChannelMessage(label, msg.data.to_vec())).await;
                    })
                }));
            })
        }));

        // Log offer SDP for debugging
        debug!("=== OFFER SDP (from server) ===");
        for line in sdp_offer.lines() {
            debug!("OFFER: {}", line);
        }
        debug!("=== END OFFER SDP ===");

        // Set remote description (offer)
        let offer = RTCSessionDescription::offer(sdp_offer.to_string())?;
        peer_connection.set_remote_description(offer).await?;
        info!("Remote description set");

        // Wait for ICE gathering
        let (gather_tx, gather_rx) = tokio::sync::oneshot::channel::<()>();
        let gather_tx = Arc::new(std::sync::Mutex::new(Some(gather_tx)));

        peer_connection.on_ice_gathering_state_change(Box::new({
            let gather_tx = gather_tx.clone();
            move |state| {
                info!("ICE gathering state: {:?}", state);
                if state == RTCIceGathererState::Complete {
                    if let Some(tx) = gather_tx.lock().unwrap().take() {
                        let _ = tx.send(());
                    }
                }
                Box::pin(async {})
            }
        }));

        // Create answer (DTLS role is already configured via SettingEngine if ice-lite)
        let answer = peer_connection.create_answer(None).await?;
        peer_connection.set_local_description(answer.clone()).await?;
        info!("Local description set, waiting for ICE gathering...");

        // Wait for ICE gathering (with timeout)
        let gather_result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            gather_rx
        ).await;

        match gather_result {
            Ok(_) => info!("ICE gathering complete"),
            Err(_) => warn!("ICE gathering timeout - proceeding"),
        }

        // Get final SDP (already has DTLS setup fixed if ice-lite)
        let final_sdp = peer_connection.local_description().await
            .map(|d| d.sdp)
            .unwrap_or_else(|| answer.sdp.clone());

        info!("Final SDP length: {}", final_sdp.len());

        // Log SDP content for debugging
        debug!("=== ANSWER SDP ===");
        for line in final_sdp.lines() {
            debug!("SDP: {}", line);
        }
        debug!("=== END SDP ===");

        // Store in static for PLI requests
        *PEER_CONNECTION.lock() = Some(peer_connection.clone());

        self.peer_connection = Some(peer_connection);

        Ok(final_sdp)
    }

    /// Create input data channels (reliable for keyboard, partially reliable for mouse)
    pub async fn create_input_channel(&mut self) -> Result<()> {
        let pc = self.peer_connection.as_ref().context("No peer connection")?;

        // Reliable channel for keyboard and handshake
        let dc = pc.create_data_channel(
            "input_channel_v1",
            Some(webrtc::data_channel::data_channel_init::RTCDataChannelInit {
                ordered: Some(true),  // Keyboard needs ordering
                max_retransmits: Some(0),
                ..Default::default()
            }),
        ).await?;

        info!("Created reliable input channel: {}", dc.label());

        let event_tx = self.event_tx.clone();

        dc.on_open(Box::new(move || {
            info!("Input channel opened");
            Box::pin(async {})
        }));

        let event_tx_msg = event_tx.clone();
        dc.on_message(Box::new(move |msg| {
            let tx = event_tx_msg.clone();
            let data = msg.data.to_vec();
            Box::pin(async move {
                debug!("Input channel message: {} bytes", data.len());
                if data.len() >= 2 && data[0] == 0x0e {
                    let _ = tx.send(WebRtcEvent::DataChannelMessage(
                        "input_handshake".to_string(),
                        data,
                    )).await;
                }
            })
        }));

        self.input_channel = Some(dc);

        // Partially reliable channel for mouse - lower latency!
        // Uses maxPacketLifeTime instead of retransmits for time-sensitive data
        let mouse_dc = pc.create_data_channel(
            "input_channel_partially_reliable",
            Some(webrtc::data_channel::data_channel_init::RTCDataChannelInit {
                ordered: Some(false),           // Unordered for lower latency
                max_packet_life_time: Some(8), // 8ms lifetime for low-latency mouse
                ..Default::default()
            }),
        ).await?;

        info!("Created partially reliable mouse channel: {}", mouse_dc.label());

        mouse_dc.on_open(Box::new(move || {
            info!("Mouse channel opened (partially reliable)");
            Box::pin(async {})
        }));

        self.mouse_channel = Some(mouse_dc);

        Ok(())
    }

    /// Send input event over reliable data channel (keyboard, handshake)
    pub async fn send_input(&mut self, data: &[u8]) -> Result<()> {
        let dc = self.input_channel.as_ref().context("No input channel")?;
        dc.send(&Bytes::copy_from_slice(data)).await?;
        Ok(())
    }

    /// Explicitly send controller input (aliases send_input/input_channel_v1 for now)
    /// Used to enforce logical separation
    pub async fn send_controller_input(&mut self, data: &[u8]) -> Result<()> {
        // "input_channel_v1 needs to be only controller"
        // We use the reliable channel (v1) for controller
        self.send_input(data).await
    }

    /// Send mouse input over partially reliable channel (lower latency)
    /// Falls back to reliable channel if mouse channel not ready
    pub async fn send_mouse_input(&mut self, data: &[u8]) -> Result<()> {
        // Prefer the partially reliable channel for mouse
        if let Some(ref mouse_dc) = self.mouse_channel {
            if mouse_dc.ready_state() == webrtc::data_channel::data_channel_state::RTCDataChannelState::Open {
                mouse_dc.send(&Bytes::copy_from_slice(data)).await?;
                return Ok(());
            }
        }
        // Fall back to reliable channel?
        // User reports "controller needs to be only path not same as mouse"
        // Removing fallback to ensure mouse never pollutes controller channel
        // self.send_input(data).await
        warn!("Mouse channel not ready, dropping mouse event");
        Ok(())
    }

    /// Check if mouse channel is ready
    pub fn is_mouse_channel_ready(&self) -> bool {
        self.mouse_channel.as_ref()
            .map(|dc| dc.ready_state() == webrtc::data_channel::data_channel_state::RTCDataChannelState::Open)
            .unwrap_or(false)
    }

    /// Send handshake response
    pub async fn send_handshake_response(&mut self, major: u8, minor: u8, flags: u8) -> Result<()> {
        let response = vec![0x0e, major, minor, flags];
        self.send_input(&response).await?;
        self.handshake_complete = true;
        info!("Sent handshake response, input ready");
        Ok(())
    }

    /// Add remote ICE candidate
    pub async fn add_ice_candidate(&self, candidate: &str, sdp_mid: Option<&str>, sdp_mline_index: Option<u16>, ufrag: Option<String>) -> Result<()> {
        let pc = self.peer_connection.as_ref().context("No peer connection")?;

        let candidate = webrtc::ice_transport::ice_candidate::RTCIceCandidateInit {
            candidate: candidate.to_string(),
            sdp_mid: sdp_mid.map(|s| s.to_string()),
            sdp_mline_index,
            username_fragment: ufrag,
        };

        pc.add_ice_candidate(candidate).await?;
        info!("Added remote ICE candidate");
        Ok(())
    }

    pub fn is_handshake_complete(&self) -> bool {
        self.handshake_complete
    }

    /// Get RTT (round-trip time) from ICE candidate pair stats
    /// Returns None if no active candidate pair or stats unavailable
    pub async fn get_rtt_ms(&self) -> Option<f32> {
        let pc = self.peer_connection.as_ref()?;
        let stats = pc.get_stats().await;

        // Look for ICE candidate pair stats with RTT
        for (_, stat) in stats.reports.iter() {
            if let webrtc::stats::StatsReportType::CandidatePair(pair) = stat {
                // Only use nominated/active pairs
                if pair.nominated && pair.current_round_trip_time > 0.0 {
                    // current_round_trip_time is in seconds, convert to ms
                    return Some((pair.current_round_trip_time * 1000.0) as f32);
                }
            }
        }
        None
    }

    /// Get comprehensive network stats (RTT, jitter, packet loss)
    pub async fn get_network_stats(&self) -> NetworkStats {
        let mut stats = NetworkStats::default();

        let Some(pc) = self.peer_connection.as_ref() else {
            return stats;
        };

        let report = pc.get_stats().await;

        // Debug: log candidate pair stats once
        static LOGGED_STATS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        let should_log = !LOGGED_STATS.swap(true, std::sync::atomic::Ordering::Relaxed);

        for (id, stat) in report.reports.iter() {
            match stat {
                webrtc::stats::StatsReportType::CandidatePair(pair) => {
                    if should_log {
                        info!("CandidatePair {}: nominated={}, state={:?}, rtt={}s",
                              id, pair.nominated, pair.state, pair.current_round_trip_time);
                    }
                    // Use any pair with RTT data (not just nominated - ice-lite may behave differently)
                    if pair.current_round_trip_time > 0.0 && stats.rtt_ms == 0.0 {
                        stats.rtt_ms = (pair.current_round_trip_time * 1000.0) as f32;
                    }
                    if pair.nominated {
                        stats.bytes_received = pair.bytes_received;
                        stats.bytes_sent = pair.bytes_sent;
                        stats.packets_received = pair.packets_received as u64;
                    }
                }
                webrtc::stats::StatsReportType::InboundRTP(inbound) => {
                    // Video track stats - packets_received available
                    if inbound.kind == "video" {
                        stats.video_packets_received = inbound.packets_received;
                    }
                }
                _ => {}
            }
        }

        stats
    }
}

/// Network statistics from WebRTC
#[derive(Debug, Clone, Default)]
pub struct NetworkStats {
    pub rtt_ms: f32,
    pub packets_received: u64,
    pub video_packets_received: u64,
    pub bytes_received: u64,
    pub bytes_sent: u64,
}
