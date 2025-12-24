//! WebRTC Client using webrtc-rs
//!
//! Handles WebRTC peer connection, media streams, and data channels
//! for GFN streaming.

use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::APIBuilder;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::data_channel::RTCDataChannel;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::ice_transport::ice_gatherer_state::RTCIceGathererState;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use anyhow::{Result, Context};
use log::{info, debug, warn};
use bytes::Bytes;

use super::input::InputEncoder;

/// Events from WebRTC connection
#[derive(Debug)]
pub enum WebRtcEvent {
    Connected,
    Disconnected,
    VideoFrame(Vec<u8>),
    AudioFrame(Vec<u8>),
    DataChannelOpen(String),
    DataChannelMessage(String, Vec<u8>),
    IceCandidate(String, Option<String>, Option<u16>),
    Error(String),
}

/// WebRTC client for GFN streaming
pub struct WebRtcClient {
    peer_connection: Option<Arc<RTCPeerConnection>>,
    input_channel: Option<Arc<RTCDataChannel>>,
    event_tx: mpsc::Sender<WebRtcEvent>,
    input_encoder: InputEncoder,
    handshake_complete: bool,
}

impl WebRtcClient {
    pub fn new(event_tx: mpsc::Sender<WebRtcEvent>) -> Self {
        Self {
            peer_connection: None,
            input_channel: None,
            event_tx,
            input_encoder: InputEncoder::new(),
            handshake_complete: false,
        }
    }

    /// Create peer connection and set remote SDP offer
    pub async fn handle_offer(&mut self, sdp_offer: &str, ice_servers: Vec<RTCIceServer>) -> Result<String> {
        info!("Setting up WebRTC peer connection");

        // Create media engine
        let mut media_engine = MediaEngine::default();

        // Register H264 codec
        media_engine.register_default_codecs()?;

        // Create interceptor registry
        let mut registry = Registry::new();
        registry = register_default_interceptors(registry, &mut media_engine)?;

        // Create API
        let api = APIBuilder::new()
            .with_media_engine(media_engine)
            .with_interceptor_registry(registry)
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

        // On track (video/audio)
        let event_tx_track = event_tx.clone();
        peer_connection.on_track(Box::new(move |track, _receiver, _transceiver| {
            let tx = event_tx_track.clone();
            let track = track.clone();
            info!("Track received: kind={:?}, id={}", track.kind(), track.id());

            Box::pin(async move {
                // Read RTP packets from track
                let mut buffer = vec![0u8; 1500];
                loop {
                    match track.read(&mut buffer).await {
                        Ok((rtp_packet, _)) => {
                            // Send frame data to event handler
                            // In a real implementation, we'd decode H264 here
                            if track.kind() == webrtc::rtp_transceiver::rtp_codec::RTPCodecType::Video {
                                let _ = tx.send(WebRtcEvent::VideoFrame(rtp_packet.payload.to_vec())).await;
                            } else {
                                let _ = tx.send(WebRtcEvent::AudioFrame(rtp_packet.payload.to_vec())).await;
                            }
                        }
                        Err(e) => {
                            warn!("Track read error: {}", e);
                            break;
                        }
                    }
                }
            })
        }));

        // On data channel
        let event_tx_dc = event_tx.clone();
        peer_connection.on_data_channel(Box::new(move |dc| {
            let tx = event_tx_dc.clone();
            let dc_label = dc.label().to_string();
            info!("Data channel received: {}", dc_label);

            Box::pin(async move {
                let label = dc_label.clone();

                // On open
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

                // On message
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

        // Set remote description (offer)
        let offer = RTCSessionDescription::offer(sdp_offer.to_string())?;
        peer_connection.set_remote_description(offer).await?;
        info!("Remote description set");

        // Set up ICE gathering completion channel
        let (gather_tx, gather_rx) = oneshot::channel::<()>();
        let gather_tx = Arc::new(std::sync::Mutex::new(Some(gather_tx)));

        // On ICE gathering state change
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

        // Create answer
        let answer = peer_connection.create_answer(None).await?;
        peer_connection.set_local_description(answer.clone()).await?;
        info!("Local description set, waiting for ICE gathering...");

        // Wait for ICE gathering to complete (with timeout)
        let gather_result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            gather_rx
        ).await;

        match gather_result {
            Ok(_) => info!("ICE gathering complete"),
            Err(_) => warn!("ICE gathering timeout - proceeding with current candidates"),
        }

        // Get the final SDP with all gathered candidates
        let final_sdp = peer_connection.local_description().await
            .map(|d| d.sdp)
            .unwrap_or_else(|| answer.sdp.clone());

        info!("Final SDP length: {}", final_sdp.len());

        self.peer_connection = Some(peer_connection);

        Ok(final_sdp)
    }

    /// Create input data channel
    pub async fn create_input_channel(&mut self) -> Result<()> {
        let pc = self.peer_connection.as_ref().context("No peer connection")?;

        // Create input channel matching GFN protocol
        let dc = pc.create_data_channel(
            "input_channel_v1",
            Some(webrtc::data_channel::data_channel_init::RTCDataChannelInit {
                ordered: Some(true),
                max_packet_life_time: Some(300), // 300ms partial reliability
                ..Default::default()
            }),
        ).await?;

        info!("Created input data channel: {}", dc.label());

        // Set up handlers (dc is already Arc<RTCDataChannel>)
        let event_tx = self.event_tx.clone();
        let dc_for_handler = dc.clone();

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

                // Check for handshake: [0x0e, major, minor, flags]
                if data.len() == 4 && data[0] == 0x0e {
                    info!("Received input handshake: version {}.{}, flags {}",
                          data[1], data[2], data[3]);
                    let _ = tx.send(WebRtcEvent::DataChannelMessage(
                        "input_handshake".to_string(),
                        data,
                    )).await;
                }
            })
        }));

        self.input_channel = Some(dc_for_handler);
        Ok(())
    }

    /// Send input event over data channel
    pub async fn send_input(&mut self, data: &[u8]) -> Result<()> {
        let dc = self.input_channel.as_ref().context("No input channel")?;
        dc.send(&Bytes::copy_from_slice(data)).await?;
        Ok(())
    }

    /// Send handshake response
    pub async fn send_handshake_response(&mut self, major: u8, minor: u8, flags: u8) -> Result<()> {
        let response = InputEncoder::encode_handshake_response(major, minor, flags);
        self.send_input(&response).await?;
        self.handshake_complete = true;
        info!("Sent handshake response, input ready");
        Ok(())
    }

    /// Add remote ICE candidate
    pub async fn add_ice_candidate(&self, candidate: &str, sdp_mid: Option<&str>, sdp_mline_index: Option<u16>) -> Result<()> {
        let pc = self.peer_connection.as_ref().context("No peer connection")?;

        let candidate = webrtc::ice_transport::ice_candidate::RTCIceCandidateInit {
            candidate: candidate.to_string(),
            sdp_mid: sdp_mid.map(|s| s.to_string()),
            sdp_mline_index: sdp_mline_index,
            username_fragment: None,
        };

        pc.add_ice_candidate(candidate).await?;
        info!("Added remote ICE candidate");
        Ok(())
    }

    pub fn is_handshake_complete(&self) -> bool {
        self.handshake_complete
    }
}
