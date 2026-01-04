//! GFN WebSocket Signaling Protocol
//!
//! WebSocket-based signaling for WebRTC connection setup.

use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::tungstenite::Message;
use futures_util::{StreamExt, SinkExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use anyhow::{Result, Context};
use log::{info, debug, warn, error};
use base64::{Engine as _, engine::general_purpose::STANDARD};

/// Generate WebSocket key for handshake
fn generate_ws_key() -> String {
    let random_bytes: [u8; 16] = rand::random();
    STANDARD.encode(random_bytes)
}

/// Peer info sent to server
#[derive(Debug, Serialize, Deserialize)]
pub struct PeerInfo {
    pub browser: String,
    #[serde(rename = "browserVersion")]
    pub browser_version: String,
    pub connected: bool,
    pub id: u32,
    pub name: String,
    pub peer_role: u32,
    pub resolution: String,
    pub version: u32,
}

/// Message from signaling server
#[derive(Debug, Deserialize)]
pub struct SignalingMessage {
    pub ackid: Option<u32>,
    pub ack: Option<u32>,
    pub hb: Option<u32>,
    pub peer_info: Option<PeerInfo>,
    pub peer_msg: Option<PeerMessage>,
}

/// Peer-to-peer message wrapper
#[derive(Debug, Deserialize)]
pub struct PeerMessage {
    pub from: u32,
    pub to: u32,
    pub msg: String,
}

/// ICE candidate message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IceCandidate {
    pub candidate: String,
    #[serde(rename = "sdpMid")]
    pub sdp_mid: Option<String>,
    #[serde(rename = "sdpMLineIndex")]
    pub sdp_mline_index: Option<u32>,
}

/// Events emitted by the signaling client
#[derive(Debug)]
pub enum SignalingEvent {
    Connected,
    SdpOffer(String),
    IceCandidate(IceCandidate),
    Disconnected(String),
    Error(String),
}

/// GFN Signaling Client
pub struct GfnSignaling {
    server_ip: String,
    session_id: String,
    peer_id: u32,
    peer_name: String,
    ack_counter: Arc<Mutex<u32>>,
    event_tx: mpsc::Sender<SignalingEvent>,
    message_tx: Option<mpsc::Sender<Message>>,
}

impl GfnSignaling {
    pub fn new(
        server_ip: String,
        session_id: String,
        event_tx: mpsc::Sender<SignalingEvent>,
    ) -> Self {
        let peer_id = 2; // Client is always peer 2
        let random_suffix: u64 = rand::random::<u64>() % 10_000_000_000;
        let peer_name = format!("peer-{}", random_suffix);

        Self {
            server_ip,
            session_id,
            peer_id,
            peer_name,
            ack_counter: Arc::new(Mutex::new(0)),
            event_tx,
            message_tx: None,
        }
    }

    /// Connect to the signaling server
    pub async fn connect(&mut self) -> Result<()> {
        let url = format!(
            "wss://{}/nvst/sign_in?peer_id={}&version=2",
            self.server_ip, self.peer_name
        );
        let subprotocol = format!("x-nv-sessionid.{}", self.session_id);

        info!("Connecting to signaling: {}", url);
        info!("Using subprotocol: {}", subprotocol);

        // Use TLS connector that accepts self-signed certs
        let tls_connector = native_tls::TlsConnector::builder()
            .danger_accept_invalid_certs(true)
            .danger_accept_invalid_hostnames(true)
            .build()
            .context("Failed to build TLS connector")?;

        // Connect TCP first
        let host = self.server_ip.split(':').next().unwrap_or(&self.server_ip);
        let port = 443;
        let addr = format!("{}:{}", host, port);

        info!("Connecting TCP to: {}", addr);
        let tcp_stream = tokio::net::TcpStream::connect(&addr).await
            .context("TCP connection failed")?;

        info!("TCP connected, starting TLS handshake...");
        let tls_stream = tokio_native_tls::TlsConnector::from(tls_connector)
            .connect(host, tcp_stream)
            .await
            .context("TLS handshake failed")?;

        info!("TLS connected, starting WebSocket handshake...");

        let ws_key = generate_ws_key();

        let request = http::Request::builder()
            .uri(&url)
            .header("Host", &self.server_ip)
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header("Sec-WebSocket-Key", &ws_key)
            .header("Sec-WebSocket-Protocol", &subprotocol)
            .header("Origin", "https://play.geforcenow.com")
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/131.0.0.0")
            .body(())
            .context("Failed to build request")?;

        let ws_config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig {
            max_message_size: Some(64 << 20),
            max_frame_size: Some(16 << 20),
            accept_unmasked_frames: false,
            ..Default::default()
        };

        let (ws_stream, response) = tokio_tungstenite::client_async_with_config(
            request,
            tls_stream,
            Some(ws_config),
        )
        .await
        .map_err(|e| {
            error!("WebSocket handshake error: {:?}", e);
            anyhow::anyhow!("WebSocket handshake failed: {}", e)
        })?;

        info!("Connected! Response: {:?}", response.status());

        let (mut write, mut read) = ws_stream.split();

        // Channel for sending messages
        let (msg_tx, mut msg_rx) = mpsc::channel::<Message>(64);
        self.message_tx = Some(msg_tx.clone());

        // Send initial peer_info
        let peer_info = self.create_peer_info();
        let peer_info_msg = json!({
            "ackid": self.next_ack_id().await,
            "peer_info": peer_info
        });
        write.send(Message::Text(peer_info_msg.to_string())).await?;
        info!("Sent peer_info");

        let event_tx = self.event_tx.clone();
        let peer_id = self.peer_id;

        // Spawn message sender task
        tokio::spawn(async move {
            while let Some(msg) = msg_rx.recv().await {
                if let Err(e) = write.send(msg).await {
                    error!("Failed to send message: {}", e);
                    break;
                }
            }
        });

        // Spawn message receiver task
        let msg_tx_clone = msg_tx.clone();
        let event_tx_clone = event_tx.clone();
        tokio::spawn(async move {
            while let Some(msg_result) = read.next().await {
                match msg_result {
                    Ok(Message::Text(text)) => {
                        info!("Received: {}", &text[..text.len().min(1000)]);

                        if let Ok(msg) = serde_json::from_str::<SignalingMessage>(&text) {
                            // Send ACK for messages with ackid
                            if let Some(ackid) = msg.ackid {
                                if msg.peer_info.as_ref().map(|p| p.id) != Some(peer_id) {
                                    let ack = json!({ "ack": ackid });
                                    let _ = msg_tx_clone.send(Message::Text(ack.to_string())).await;
                                }
                            }

                            // Handle heartbeat
                            if msg.hb.is_some() {
                                let hb = json!({ "hb": 1 });
                                let _ = msg_tx_clone.send(Message::Text(hb.to_string())).await;
                                continue;
                            }

                            // Handle peer messages
                            if let Some(peer_msg) = msg.peer_msg {
                                if let Ok(inner) = serde_json::from_str::<Value>(&peer_msg.msg) {
                                    // SDP Offer
                                    if inner.get("type").and_then(|t| t.as_str()) == Some("offer") {
                                        if let Some(sdp) = inner.get("sdp").and_then(|s| s.as_str()) {
                                            info!("Received SDP offer, length: {}", sdp.len());
                                            // Log full SDP for debugging (color space info, codec params)
                                            for line in sdp.lines() {
                                                debug!("SDP: {}", line);
                                            }
                                            let _ = event_tx_clone.send(SignalingEvent::SdpOffer(sdp.to_string())).await;
                                        }
                                    }
                                    // ICE Candidate
                                    else if inner.get("candidate").is_some() {
                                        if let Ok(candidate) = serde_json::from_value::<IceCandidate>(inner) {
                                            info!("Received ICE candidate: {}", candidate.candidate);
                                            let _ = event_tx_clone.send(SignalingEvent::IceCandidate(candidate)).await;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Ok(Message::Close(frame)) => {
                        warn!("WebSocket closed: {:?}", frame);
                        let _ = event_tx_clone.send(SignalingEvent::Disconnected(
                            frame.map(|f| f.reason.to_string()).unwrap_or_default()
                        )).await;
                        break;
                    }
                    Err(e) => {
                        error!("WebSocket error: {}", e);
                        let _ = event_tx_clone.send(SignalingEvent::Error(e.to_string())).await;
                        break;
                    }
                    _ => {}
                }
            }
        });

        // Notify connected
        self.event_tx.send(SignalingEvent::Connected).await?;

        // Start heartbeat task
        let hb_tx = msg_tx.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
            loop {
                interval.tick().await;
                let hb = json!({ "hb": 1 });
                if hb_tx.send(Message::Text(hb.to_string())).await.is_err() {
                    break;
                }
            }
        });

        Ok(())
    }

    /// Send SDP answer to server
    pub async fn send_answer(&self, sdp: &str, nvst_sdp: Option<&str>) -> Result<()> {
        let msg_tx = self.message_tx.as_ref().context("Not connected")?;

        let mut answer = json!({
            "type": "answer",
            "sdp": sdp
        });

        if let Some(nvst) = nvst_sdp {
            // Try to parse as JSON object (for nvstSdp wrapper), otherwise treat as string
            if let Ok(val) = serde_json::from_str::<Value>(nvst) {
                answer["nvstSdp"] = val;
            } else {
                answer["nvstSdp"] = json!(nvst);
            }
        }

        let peer_msg = json!({
            "peer_msg": {
                "from": self.peer_id,
                "to": 1,
                "msg": answer.to_string()
            },
            "ackid": self.next_ack_id().await
        });

        msg_tx.send(Message::Text(peer_msg.to_string())).await?;
        info!("Sent SDP answer");
        Ok(())
    }

    /// Send ICE candidate to server
    pub async fn send_ice_candidate(&self, candidate: &str, sdp_mid: Option<&str>, sdp_mline_index: Option<u32>) -> Result<()> {
        let msg_tx = self.message_tx.as_ref().context("Not connected")?;

        let ice = json!({
            "candidate": candidate,
            "sdpMid": sdp_mid,
            "sdpMLineIndex": sdp_mline_index
        });

        let peer_msg = json!({
            "peer_msg": {
                "from": self.peer_id,
                "to": 1,
                "msg": ice.to_string()
            },
            "ackid": self.next_ack_id().await
        });

        msg_tx.send(Message::Text(peer_msg.to_string())).await?;
        info!("Sent ICE candidate: {}", candidate);
        Ok(())
    }

    fn create_peer_info(&self) -> PeerInfo {
        PeerInfo {
            browser: "Chrome".to_string(),
            browser_version: "131".to_string(),
            connected: true,
            id: self.peer_id,
            name: self.peer_name.clone(),
            peer_role: 0,
            resolution: "1920x1080".to_string(),
            version: 2,
        }
    }

    async fn next_ack_id(&self) -> u32 {
        let mut counter = self.ack_counter.lock().await;
        *counter += 1;
        *counter
    }
}
