use std::io::ErrorKind;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use opennow_streamer_protocol::Session;
use opennow_streamer_transport::webrtc::IceCandidate;
use serde_json::{Value, json};
use tungstenite::client::IntoClientRequest;
use tungstenite::http::{HeaderValue, Uri};
use tungstenite::protocol::WebSocketConfig;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

use super::{Failure, MAX_SDP_BYTES};

pub(super) const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_SIGNALING_BYTES: usize = 512 * 1024;

struct ResolveRequest {
    host: String,
    port: u16,
    result: mpsc::SyncSender<std::io::Result<Vec<SocketAddr>>>,
}

pub(super) fn resolve(
    host: String,
    port: u16,
    cancelled: &AtomicBool,
) -> Result<Vec<SocketAddr>, Failure> {
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return Ok(vec![SocketAddr::new(ip, port)]);
    }
    static RESOLVER: OnceLock<Option<mpsc::SyncSender<ResolveRequest>>> = OnceLock::new();
    let resolver = RESOLVER
        .get_or_init(|| {
            let (sender, receiver) = mpsc::sync_channel::<ResolveRequest>(1);
            thread::Builder::new()
                .name("opennow-webrtc-dns".to_owned())
                .spawn(move || {
                    while let Ok(request) = receiver.recv() {
                        let result = (request.host.as_str(), request.port)
                            .to_socket_addrs()
                            .map(|addresses| addresses.take(8).collect::<Vec<_>>());
                        let _ = request.result.try_send(result);
                    }
                })
                .ok()
                .map(|_| sender)
        })
        .as_ref()
        .ok_or_else(|| Failure::signaling("Could not start bounded DNS resolver"))?;
    let (sender, receiver) = mpsc::sync_channel(1);
    resolver
        .try_send(ResolveRequest {
            host,
            port,
            result: sender,
        })
        .map_err(|_| Failure::signaling("WebRTC DNS resolver is busy"))?;
    let deadline = Instant::now() + CONNECT_TIMEOUT;
    loop {
        if cancelled.load(Ordering::Acquire) || Instant::now() >= deadline {
            return Err(Failure::signaling("WebRTC DNS cancelled or timed out"));
        }
        match receiver.recv_timeout(Duration::from_millis(25)) {
            Ok(Ok(addresses)) if !addresses.is_empty() => return Ok(addresses),
            Ok(_) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(Failure::signaling("WebRTC DNS failed"));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

pub(super) enum Incoming {
    Offer(String),
    Candidate(IceCandidate),
    Closed,
}

pub(super) struct Protocol {
    peer_name: String,
    peer_id: u64,
    remote_id: u64,
    ack: u64,
}

impl Protocol {
    pub(super) fn new(peer_name: String) -> Self {
        Self {
            peer_name,
            peer_id: 0,
            remote_id: 1,
            ack: 0,
        }
    }

    pub(super) fn peer_info(&mut self, width: u32, height: u32) -> Value {
        self.ack += 1;
        json!({"ackid":self.ack,"peer_info":{
            "browser":"Chrome","browserVersion":"131","connected":true,
            "id":self.peer_id,"name":self.peer_name,"peerRole":0,
            "resolution":format!("{width}x{height}"),"version":2
        }})
    }

    pub(super) fn peer_message(&mut self, payload: Value) -> Value {
        self.ack += 1;
        json!({"ackid":self.ack,"peer_msg":{
            "from":self.peer_id,"to":self.remote_id,"msg":payload.to_string()
        }})
    }

    pub(super) fn receive(
        &mut self,
        text: &str,
    ) -> Result<(Vec<Value>, Option<Incoming>), Failure> {
        if text.len() > MAX_SIGNALING_BYTES {
            return Err(Failure::signaling("Signaling message exceeds size limit"));
        }
        let packet: Value =
            serde_json::from_str(text).map_err(|_| Failure::signaling("Invalid signaling JSON"))?;
        if packet["peer_info"]["name"].as_str() == Some(&self.peer_name)
            && let Some(id) = packet["peer_info"]["id"].as_u64()
        {
            self.peer_id = id;
        }
        let mut replies = Vec::new();
        if let Some(ack) = packet["ackid"].as_u64()
            && packet["peer_info"]["id"].as_u64() != Some(self.peer_id)
        {
            replies.push(json!({"ack":ack}));
        }
        if packet["hb"].as_u64().is_some_and(|hb| hb != 0) {
            replies.push(json!({"hb":1}));
            return Ok((replies, None));
        }
        if packet["error"].as_str() == Some("peerRemoved") {
            return Ok((replies, Some(Incoming::Closed)));
        }
        if packet.get("error").is_some_and(|value| !value.is_null()) {
            return Err(Failure::signaling("Server rejected signaling request"));
        }
        let Some(message) = packet["peer_msg"]["msg"].as_str() else {
            return Ok((replies, None));
        };
        if let Some(id) = packet["peer_msg"]["from"].as_u64() {
            self.remote_id = id;
        }
        if message.trim() == "BYE" {
            return Ok((replies, Some(Incoming::Closed)));
        }
        let payload: Value = serde_json::from_str(message)
            .map_err(|_| Failure::signaling("Invalid nested peer message"))?;
        if payload["type"].as_str() == Some("offer") {
            let sdp = payload["sdp"]
                .as_str()
                .filter(|sdp| !sdp.is_empty() && sdp.len() <= MAX_SDP_BYTES)
                .ok_or_else(|| Failure::signaling("Offer SDP is missing or exceeds size limit"))?;
            return Ok((replies, Some(Incoming::Offer(sdp.to_owned()))));
        }
        if payload["candidate"].is_string() {
            let mut candidate: IceCandidate = serde_json::from_value(payload)
                .map_err(|_| Failure::signaling("Invalid remote ICE candidate"))?;
            if candidate.candidate.len() > 4096 {
                return Err(Failure::signaling(
                    "Remote ICE candidate exceeds size limit",
                ));
            }
            if candidate.sdp_mid.is_none() && candidate.sdp_m_line_index.is_none() {
                candidate.sdp_m_line_index = Some(0);
            }
            return Ok((replies, Some(Incoming::Candidate(candidate))));
        }
        Ok((replies, None))
    }
}

fn query_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write;
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

pub(super) fn sign_in_url(session: &Session, peer: &str) -> Result<String, Failure> {
    let base = session
        .extra
        .get("signalingUrl")
        .and_then(Value::as_str)
        .filter(|url| !url.trim().is_empty())
        .ok_or_else(|| Failure::signaling("WebRTC requires an explicit signalingUrl"))?;
    let uri: Uri = base
        .parse()
        .map_err(|_| Failure::signaling("Invalid signalingUrl"))?;
    let authority = uri
        .authority()
        .ok_or_else(|| Failure::signaling("Signaling URL has no host"))?;
    let test_plaintext =
        cfg!(test) && uri.scheme_str() == Some("ws") && uri.host() == Some("127.0.0.1");
    if (uri.scheme_str() != Some("wss") && !test_plaintext)
        || authority.as_str().contains('@')
        || base.contains('#')
    {
        return Err(Failure::signaling(
            "Signaling requires a credential-free WSS endpoint",
        ));
    }
    let path = uri.path().trim_end_matches('/');
    let path = if path.ends_with("/sign_in") {
        path.to_owned()
    } else {
        format!("{path}/sign_in")
    };
    let reconnect = session
        .extra
        .get("signalingReconnect")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let scheme = if test_plaintext { "ws" } else { "wss" };
    Ok(format!(
        "{scheme}://{authority}{path}?peer_id={}&version={}&peer_role=1&pairing_id={}{}",
        query_component(peer),
        if reconnect { "3.0" } else { "2" },
        query_component(&session.session_id),
        if reconnect { "&reconnect=1" } else { "" }
    ))
}

pub(super) struct Signaling {
    socket: WebSocket<MaybeTlsStream<TcpStream>>,
    pub(super) protocol: Protocol,
    heartbeat_at: Instant,
}

impl Signaling {
    pub(super) fn connect(session: &Session, cancelled: &AtomicBool) -> Result<Self, Failure> {
        let mut random = [0_u8; 12];
        getrandom::fill(&mut random)
            .map_err(|_| Failure::signaling("Could not generate signaling identity"))?;
        let peer = format!(
            "peer-{}",
            random
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        let url = sign_in_url(session, &peer)?;
        let mut request = url
            .into_client_request()
            .map_err(|_| Failure::signaling("Invalid WSS request"))?;
        request.headers_mut().insert(
            "Origin",
            HeaderValue::from_static("https://play.geforcenow.com"),
        );
        request.headers_mut().insert(
            "User-Agent",
            HeaderValue::from_static(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/131.0.0.0 Safari/537.36",
            ),
        );
        request.headers_mut().insert(
            "Sec-WebSocket-Protocol",
            HeaderValue::from_str(&format!("x-nv-sessionid.{}", session.session_id))
                .map_err(|_| Failure::signaling("Invalid signaling session ID"))?,
        );
        let host = request
            .uri()
            .host()
            .ok_or_else(|| Failure::signaling("Missing signaling host"))?
            .to_owned();
        let port = request.uri().port_u16().unwrap_or(443);
        let addresses = resolve(host, port, cancelled)?;
        let deadline = Instant::now() + CONNECT_TIMEOUT;
        let mut connected = None;
        for address in addresses {
            if cancelled.load(Ordering::Acquire) || Instant::now() >= deadline {
                break;
            }
            let budget = deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(500));
            if let Ok(stream) = TcpStream::connect_timeout(&address, budget) {
                connected = Some(stream);
                break;
            }
        }
        let stream = connected
            .ok_or_else(|| Failure::signaling("Signaling TCP connection failed or timed out"))?;
        stream
            .set_nonblocking(true)
            .map_err(|_| Failure::signaling("Could not bound signaling handshake"))?;
        let config = WebSocketConfig::default()
            .max_write_buffer_size(MAX_SIGNALING_BYTES * 2)
            .max_message_size(Some(MAX_SIGNALING_BYTES))
            .max_frame_size(Some(MAX_SIGNALING_BYTES));
        let mut handshake =
            tungstenite::client_tls_with_config(request, stream, Some(config), None);
        let mut socket = loop {
            if cancelled.load(Ordering::Acquire) || Instant::now() >= deadline {
                return Err(Failure::signaling(
                    "Signaling handshake cancelled or timed out",
                ));
            }
            match handshake {
                Ok((socket, _)) => break socket,
                Err(tungstenite::HandshakeError::Interrupted(pending)) => {
                    thread::sleep(Duration::from_millis(5));
                    handshake = pending.handshake();
                }
                Err(tungstenite::HandshakeError::Failure(_)) => {
                    return Err(Failure::signaling(
                        "Signaling TLS/WebSocket handshake failed",
                    ));
                }
            }
        };
        let stream = match socket.get_mut() {
            MaybeTlsStream::Plain(stream) => stream,
            MaybeTlsStream::Rustls(stream) => stream.get_mut(),
            _ => return Err(Failure::signaling("Unsupported signaling TLS backend")),
        };
        stream
            .set_nonblocking(false)
            .map_err(|_| Failure::signaling("Could not configure signaling socket"))?;
        stream
            .set_read_timeout(Some(Duration::from_millis(2)))
            .map_err(|_| Failure::signaling("Could not configure signaling poll"))?;
        stream
            .set_write_timeout(Some(Duration::from_millis(250)))
            .map_err(|_| Failure::signaling("Could not configure signaling write"))?;
        Ok(Self {
            socket,
            protocol: Protocol::new(peer),
            heartbeat_at: Instant::now(),
        })
    }

    pub(super) fn send(&mut self, payload: Value) -> Result<(), Failure> {
        self.socket
            .send(Message::Text(payload.to_string().into()))
            .map_err(|_| Failure::signaling("Signaling write failed"))
    }

    pub(super) fn send_peer(&mut self, payload: Value) -> Result<(), Failure> {
        let packet = self.protocol.peer_message(payload);
        self.send(packet)
    }

    pub(super) fn poll(&mut self) -> Result<Option<Incoming>, Failure> {
        if self.heartbeat_at.elapsed() >= HEARTBEAT_INTERVAL {
            self.send(json!({"hb":1}))?;
            self.heartbeat_at = Instant::now();
        }
        let packet = match self.socket.read() {
            Ok(Message::Text(text)) => text.to_string(),
            Ok(Message::Binary(bytes)) => String::from_utf8(bytes.to_vec())
                .map_err(|_| Failure::signaling("Invalid signaling UTF-8"))?,
            Ok(Message::Close(_)) => return Ok(Some(Incoming::Closed)),
            Ok(_) => return Ok(None),
            Err(tungstenite::Error::Io(error))
                if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
            {
                return Ok(None);
            }
            Err(_) => return Err(Failure::signaling("Signaling connection closed")),
        };
        let (replies, incoming) = self.protocol.receive(&packet)?;
        for reply in replies {
            self.send(reply)?;
        }
        Ok(incoming)
    }
}

#[cfg(test)]
mod tests;
