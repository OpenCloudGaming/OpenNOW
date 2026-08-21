use std::io::ErrorKind;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TryRecvError, TrySendError};
use std::sync::{Arc, Once};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use opennow_streamer_protocol::{IceCandidate, IceServer, MediaConnectionInfo, Session};
use str0m::change::SdpOffer;
use str0m::channel::{ChannelConfig, ChannelId, Reliability};
use str0m::crypto::from_feature_flags;
use str0m::media::{KeyframeRequestKind, Mid};
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, IceConnectionState, Input, Output, Rtc, RtcConfig};
use thiserror::Error;

pub mod nvst;

pub use nvst::{
    BoundedFrameQueue, EncodedH264Frame, NvstBundleIdentity, NvstConfigError, NvstDropReason,
    NvstFallbackReason, NvstReceiveEvent, NvstReceiverState, NvstRecovery, NvstSrtpProfile,
    NvstUdpReceiverError, NvstUdpReceiverSession, NvstUnsupportedFeature, NvstVideoCodec,
    NvstVideoConfig, NvstVideoReceiver, PreferredVideoTransport, ReservedNvstBundle,
    advertised_nvst_ipv4, nack_transmission_support, parse_nvst_video_handoff,
    reserve_nvst_mjolnir_udp_socket, reserve_nvst_udp_socket,
    select_preferred_video_transport, spawn_nvst_mjolnir_receiver, spawn_nvst_udp_receiver,
    spawn_nvst_udp_receiver_with_socket,
};

static INSTALL_CRYPTO: Once = Once::new();
const RELIABLE_INPUT_LABEL: &str = "input_channel_v1";
const PARTIAL_INPUT_LABEL: &str = "input_channel_partially_reliable";
const STATS_LABEL: &str = "stats_channel";

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("invalid server endpoint: {0}")]
    InvalidServerEndpoint(String),
    #[error("failed to resolve server endpoint {endpoint}: {source}")]
    ResolveServerEndpoint {
        endpoint: String,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid WebRTC media endpoint: {0}")]
    InvalidMediaEndpoint(String),
    #[error("failed to resolve WebRTC media endpoint {endpoint}: {source}")]
    ResolveMediaEndpoint {
        endpoint: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to bind media UDP socket: {0}")]
    Bind(#[source] std::io::Error),
    #[error("invalid WebRTC offer: {0}")]
    Offer(String),
    #[error("failed to configure local ICE candidate: {0}")]
    LocalCandidate(String),
    #[error("invalid remote ICE candidate: {0}")]
    RemoteCandidate(String),
    #[error("input channel is not ready")]
    InputNotReady,
    #[error("encoded media consumer is no longer running")]
    MediaConsumerClosed,
    #[error("encoded media consumer is backpressured")]
    MediaConsumerBackpressured,
    #[error("transport worker is no longer running")]
    Closed,
}

impl TransportError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidServerEndpoint(_) | Self::ResolveServerEndpoint { .. } => {
                "invalid-server-endpoint"
            }
            Self::InvalidMediaEndpoint(_) | Self::ResolveMediaEndpoint { .. } => {
                "invalid-media-endpoint"
            }
            Self::Bind(_) | Self::LocalCandidate(_) => "local-transport-failed",
            Self::Offer(_) => "invalid-offer",
            Self::RemoteCandidate(_) => "invalid-remote-candidate",
            Self::InputNotReady => "input-not-ready",
            Self::MediaConsumerClosed => "media-consumer-closed",
            Self::MediaConsumerBackpressured => "media-consumer-backpressured",
            Self::Closed => "transport-closed",
        }
    }
}

#[derive(Debug)]
pub enum TransportEvent {
    Connected,
    Disconnected(String),
    InputReady(u16),
    Log(String),
}

#[derive(Debug, Clone)]
pub struct EncodedMediaFrame {
    pub mid: String,
    pub codec: String,
    pub payload: Arc<[u8]>,
    pub rtp_timestamp: u64,
    pub clock_rate_hz: u32,
    pub received_at_us: u64,
    pub keyframe: bool,
    pub contiguous: bool,
}

pub type MediaConsumer = SyncSender<EncodedMediaFrame>;

pub fn install_crypto() {
    INSTALL_CRYPTO.call_once(|| from_feature_flags().install_process_default());
}

fn deliver_media_frame(
    consumer: &MediaConsumer,
    frame: EncodedMediaFrame,
) -> Result<(), TransportError> {
    consumer.try_send(frame).map_err(|error| match error {
        TrySendError::Full(_) => TransportError::MediaConsumerBackpressured,
        TrySendError::Disconnected(_) => TransportError::MediaConsumerClosed,
    })
}

enum TransportCommand {
    AddRemoteCandidate(Candidate),
    SendInput {
        bytes: Vec<u8>,
        partially_reliable: bool,
    },
    RequestKeyframe {
        mid: String,
    },
    Stop,
}

pub struct NegotiatedTransport {
    pub answer_sdp: String,
    pub local_candidate: IceCandidate,
    pub session: TransportSession,
}

pub struct TransportSession {
    commands: Sender<TransportCommand>,
    join: Option<JoinHandle<()>>,
    media_endpoint: Option<SocketAddr>,
    input_ready: Arc<AtomicBool>,
}

#[derive(Clone)]
pub struct TransportControl {
    commands: Sender<TransportCommand>,
}

impl TransportSession {
    pub fn control(&self) -> TransportControl {
        TransportControl {
            commands: self.commands.clone(),
        }
    }

    pub fn add_remote_candidate(&self, candidate: &IceCandidate) -> Result<(), TransportError> {
        let candidate = normalize_remote_candidate(&candidate.candidate, self.media_endpoint);
        let candidate = candidate.strip_prefix("a=").unwrap_or(&candidate);
        let candidate = Candidate::from_sdp_string(candidate)
            .map_err(|error| TransportError::RemoteCandidate(error.to_string()))?;
        self.commands
            .send(TransportCommand::AddRemoteCandidate(candidate))
            .map_err(|_| TransportError::Closed)
    }

    pub fn send_input(
        &self,
        bytes: Vec<u8>,
        partially_reliable: bool,
    ) -> Result<(), TransportError> {
        if !self.input_ready.load(Ordering::Acquire) {
            return Err(TransportError::InputNotReady);
        }
        self.commands
            .send(TransportCommand::SendInput {
                bytes,
                partially_reliable,
            })
            .map_err(|_| TransportError::Closed)
    }

    pub fn stop(mut self) {
        let _ = self.commands.send(TransportCommand::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl TransportControl {
    pub fn request_keyframe(&self, mid: impl Into<String>) -> Result<(), TransportError> {
        self.commands
            .send(TransportCommand::RequestKeyframe { mid: mid.into() })
            .map_err(|_| TransportError::Closed)
    }
}

impl Drop for TransportSession {
    fn drop(&mut self) {
        let _ = self.commands.send(TransportCommand::Stop);
    }
}

pub fn negotiate(
    offer_sdp: &str,
    session: &Session,
    partial_reliable_lifetime_ms: u16,
    events: Sender<TransportEvent>,
    media_consumer: MediaConsumer,
) -> Result<NegotiatedTransport, TransportError> {
    install_crypto();

    if let Some(schemes) = configured_ice_schemes(&session.ice_servers) {
        let _ = events.send(TransportEvent::Log(format!(
            "Configured ICE services ({schemes}) are not gathered locally; continuing with a direct host candidate for the ICE-lite GFN peer"
        )));
    }
    let normalized_offer = normalize_offer_endpoints(offer_sdp, session)?;
    let server_ip = match normalized_offer.media_endpoint {
        Some(endpoint) => endpoint.ip(),
        None => resolve_server_endpoint(&session.server_ip)?,
    };
    let socket = bind_routed_socket(server_ip).map_err(TransportError::Bind)?;
    let local_addr = socket.local_addr().map_err(TransportError::Bind)?;
    let local_candidate = Candidate::host(local_addr, "udp")
        .map_err(|error| TransportError::LocalCandidate(error.to_string()))?;

    let mut rtc = RtcConfig::new().build(Instant::now());
    rtc.add_local_candidate(local_candidate.clone());
    let offer = SdpOffer::from_sdp_string(&normalized_offer.sdp)
        .map_err(|error| TransportError::Offer(error.to_string()))?;
    let answer = rtc
        .sdp_api()
        .accept_offer(offer)
        .map_err(|error| TransportError::Offer(error.to_string()))?;

    let reliable = rtc.direct_api().create_data_channel(ChannelConfig {
        label: RELIABLE_INPUT_LABEL.to_owned(),
        ..Default::default()
    });
    let partial = rtc.direct_api().create_data_channel(ChannelConfig {
        label: PARTIAL_INPUT_LABEL.to_owned(),
        ordered: false,
        reliability: Reliability::MaxPacketLifetime {
            lifetime: partial_reliable_lifetime_ms,
        },
        ..Default::default()
    });
    let stats = rtc.direct_api().create_data_channel(ChannelConfig {
        label: STATS_LABEL.to_owned(),
        ordered: false,
        reliability: Reliability::MaxRetransmits { retransmits: 0 },
        ..Default::default()
    });

    let answer_sdp = answer.to_sdp_string();
    let candidate_text = local_candidate.to_sdp_string();
    let (command_tx, command_rx) = mpsc::channel();
    let input_ready = Arc::new(AtomicBool::new(false));
    let worker_input_ready = input_ready.clone();
    let transport_origin = Instant::now();
    let join = thread::Builder::new()
        .name("opennow-webrtc".to_owned())
        .spawn(move || {
            run_transport(
                rtc,
                socket,
                command_rx,
                TransportOutputs {
                    events,
                    media_consumer,
                },
                TransportChannels {
                    reliable,
                    partial,
                    stats,
                },
                worker_input_ready,
                transport_origin,
            );
        })
        .map_err(TransportError::Bind)?;

    Ok(NegotiatedTransport {
        answer_sdp,
        local_candidate: IceCandidate {
            candidate: candidate_text,
            sdp_mid: Some("0".to_owned()),
            sdp_m_line_index: Some(0),
            username_fragment: None,
        },
        session: TransportSession {
            commands: command_tx,
            join: Some(join),
            media_endpoint: normalized_offer.media_endpoint,
            input_ready,
        },
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedOffer {
    pub sdp: String,
    pub replacements: usize,
    pub media_endpoint: Option<SocketAddr>,
}

pub fn normalize_offer_endpoints(
    offer_sdp: &str,
    session: &Session,
) -> Result<NormalizedOffer, TransportError> {
    let media_endpoint = resolve_media_endpoint(session.media_connection_info.as_ref())?;
    let Some(endpoint) = media_endpoint else {
        return Ok(NormalizedOffer {
            sdp: offer_sdp.to_owned(),
            replacements: 0,
            media_endpoint: None,
        });
    };

    let mut sdp = String::with_capacity(offer_sdp.len());
    let mut replacements = 0;
    for chunk in offer_sdp.split_inclusive('\n') {
        let (line, ending) = chunk
            .strip_suffix("\r\n")
            .map(|line| (line, "\r\n"))
            .or_else(|| chunk.strip_suffix('\n').map(|line| (line, "\n")))
            .unwrap_or((chunk, ""));
        let rewritten = rewrite_candidate_endpoint(line, endpoint);
        replacements += usize::from(rewritten != line);
        sdp.push_str(&rewritten);
        sdp.push_str(ending);
    }

    Ok(NormalizedOffer {
        sdp,
        replacements,
        media_endpoint: Some(endpoint),
    })
}

pub fn resolve_server_endpoint(endpoint: &str) -> Result<IpAddr, TransportError> {
    resolve_host(endpoint, 9).map_err(|source| {
        if endpoint.trim().is_empty() {
            TransportError::InvalidServerEndpoint("endpoint is empty".to_owned())
        } else {
            TransportError::ResolveServerEndpoint {
                endpoint: endpoint.to_owned(),
                source,
            }
        }
    })
}

fn resolve_media_endpoint(
    endpoint: Option<&MediaConnectionInfo>,
) -> Result<Option<SocketAddr>, TransportError> {
    let Some(endpoint) = endpoint.filter(|endpoint| matches!(endpoint.usage, Some(2 | 17))) else {
        return Ok(None);
    };
    let port = u16::try_from(endpoint.port)
        .ok()
        .filter(|port| *port != 0)
        .ok_or_else(|| {
            TransportError::InvalidMediaEndpoint(format!(
                "port {} is outside 1..=65535",
                endpoint.port
            ))
        })?;
    if endpoint.ip.trim().is_empty() {
        return Err(TransportError::InvalidMediaEndpoint(
            "hostname is empty".to_owned(),
        ));
    }
    resolve_host(&endpoint.ip, port)
        .map(|ip| Some(SocketAddr::new(ip, port)))
        .map_err(|source| TransportError::ResolveMediaEndpoint {
            endpoint: format!("{}:{port}", endpoint.ip),
            source,
        })
}

fn resolve_host(host: &str, port: u16) -> std::io::Result<IpAddr> {
    let host = host.trim();
    if host.is_empty() {
        return Err(std::io::Error::new(
            ErrorKind::InvalidInput,
            "endpoint is empty",
        ));
    }
    if let Ok(ip) = host.parse() {
        return Ok(ip);
    }
    if let Some(ip) = dashed_ipv4_prefix(host) {
        return Ok(IpAddr::V4(ip));
    }
    (host, port)
        .to_socket_addrs()?
        .next()
        .map(|address| address.ip())
        .ok_or_else(|| std::io::Error::new(ErrorKind::AddrNotAvailable, "no addresses resolved"))
}

fn dashed_ipv4_prefix(host: &str) -> Option<std::net::Ipv4Addr> {
    let first_label = host.split('.').next()?;
    let octets = first_label
        .split('-')
        .map(str::parse::<u8>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    let octets: [u8; 4] = octets.try_into().ok()?;
    Some(octets.into())
}

fn configured_ice_schemes(servers: &[IceServer]) -> Option<String> {
    let mut schemes = servers
        .iter()
        .flat_map(|server| &server.urls)
        .map(|url| {
            url.split_once(':')
                .map_or("unknown", |(scheme, _)| scheme)
                .to_ascii_lowercase()
        })
        .collect::<Vec<_>>();
    if schemes.is_empty() {
        return None;
    }
    schemes.sort_unstable();
    schemes.dedup();
    Some(schemes.join(", "))
}

fn normalize_remote_candidate(candidate: &str, endpoint: Option<SocketAddr>) -> String {
    endpoint.map_or_else(
        || candidate.to_owned(),
        |endpoint| rewrite_candidate_endpoint(candidate, endpoint),
    )
}

fn rewrite_candidate_endpoint(candidate: &str, endpoint: SocketAddr) -> String {
    let candidate_body = candidate.strip_prefix("a=").unwrap_or(candidate);
    if !candidate_body.starts_with("candidate:") {
        return candidate.to_owned();
    }
    let Some(address_range) = token_range(candidate, 4) else {
        return candidate.to_owned();
    };
    let Some(port_range) = token_range(candidate, 5) else {
        return candidate.to_owned();
    };
    let address = endpoint.ip().to_string();
    let port = endpoint.port().to_string();
    if candidate[address_range.clone()] == address && candidate[port_range.clone()] == port {
        return candidate.to_owned();
    }

    let mut rewritten = candidate.to_owned();
    rewritten.replace_range(port_range, &port);
    rewritten.replace_range(address_range, &address);
    rewritten
}

fn token_range(value: &str, target: usize) -> Option<Range<usize>> {
    let mut token = 0;
    let mut start = None;
    for (index, character) in value.char_indices() {
        if character.is_ascii_whitespace() {
            if let Some(start) = start.take() {
                if token == target {
                    return Some(start..index);
                }
                token += 1;
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    start
        .filter(|_| token == target)
        .map(|start| start..value.len())
}

fn bind_routed_socket(server_ip: IpAddr) -> std::io::Result<UdpSocket> {
    let unspecified = match server_ip {
        IpAddr::V4(_) => "0.0.0.0:0",
        IpAddr::V6(_) => "[::]:0",
    };
    let route_probe = UdpSocket::bind(unspecified)?;
    route_probe.connect(SocketAddr::new(server_ip, 9))?;
    let local_ip = route_probe.local_addr()?.ip();
    UdpSocket::bind(SocketAddr::new(local_ip, 0))
}

struct TransportChannels {
    reliable: ChannelId,
    partial: ChannelId,
    stats: ChannelId,
}

struct TransportOutputs {
    events: Sender<TransportEvent>,
    media_consumer: MediaConsumer,
}

fn run_transport(
    mut rtc: Rtc,
    socket: UdpSocket,
    commands: Receiver<TransportCommand>,
    outputs: TransportOutputs,
    channels: TransportChannels,
    input_ready_state: Arc<AtomicBool>,
    transport_origin: Instant,
) {
    struct ResetInputReady(Arc<AtomicBool>);

    impl Drop for ResetInputReady {
        fn drop(&mut self) {
            self.0.store(false, Ordering::Release);
        }
    }

    let _reset_input_ready = ResetInputReady(input_ready_state.clone());
    let mut receive_buffer = vec![0_u8; 65_536];
    let mut input_ready = false;
    let mut next_heartbeat = Instant::now() + Duration::from_secs(2);

    loop {
        loop {
            match commands.try_recv() {
                Ok(TransportCommand::AddRemoteCandidate(candidate)) => {
                    rtc.add_remote_candidate(candidate);
                }
                Ok(TransportCommand::SendInput {
                    bytes,
                    partially_reliable,
                }) => {
                    if input_ready {
                        let channel_id = if partially_reliable {
                            channels.partial
                        } else {
                            channels.reliable
                        };
                        if let Some(mut channel) = rtc.channel(channel_id) {
                            let _ = channel.write(true, &bytes);
                        }
                    }
                }
                Ok(TransportCommand::RequestKeyframe { mid }) => {
                    let mid = Mid::from(mid.as_str());
                    let Some(mut writer) = rtc.writer(mid) else {
                        let _ = outputs.events.send(TransportEvent::Log(format!(
                            "Unable to request keyframe for unknown media id {mid}"
                        )));
                        continue;
                    };
                    let kind = if writer.is_request_keyframe_possible(KeyframeRequestKind::Pli) {
                        Some(KeyframeRequestKind::Pli)
                    } else if writer.is_request_keyframe_possible(KeyframeRequestKind::Fir) {
                        Some(KeyframeRequestKind::Fir)
                    } else {
                        None
                    };
                    if let Some(kind) = kind
                        && let Err(error) = writer.request_keyframe(None, kind)
                    {
                        let _ = outputs.events.send(TransportEvent::Log(format!(
                            "Failed to request keyframe for {mid}: {error}"
                        )));
                    }
                }
                Ok(TransportCommand::Stop) | Err(TryRecvError::Disconnected) => {
                    rtc.disconnect();
                    let _ = outputs
                        .events
                        .send(TransportEvent::Disconnected("stopped".to_owned()));
                    return;
                }
                Err(TryRecvError::Empty) => break,
            }
        }

        if input_ready && Instant::now() >= next_heartbeat {
            if let Some(mut channel) = rtc.channel(channels.reliable) {
                let _ = channel.write(true, &[2, 0, 0, 0]);
            }
            next_heartbeat = Instant::now() + Duration::from_secs(2);
        }

        let timeout = loop {
            match rtc.poll_output() {
                Ok(Output::Timeout(timeout)) => break timeout,
                Ok(Output::Transmit(transmit)) => {
                    let _ = socket.send_to(&transmit.contents, transmit.destination);
                }
                Ok(Output::Event(event)) => match event {
                    Event::IceConnectionStateChange(IceConnectionState::Connected) => {
                        let _ = outputs.events.send(TransportEvent::Connected);
                    }
                    Event::IceConnectionStateChange(IceConnectionState::Disconnected) => {
                        let _ = outputs
                            .events
                            .send(TransportEvent::Disconnected("ICE disconnected".to_owned()));
                        return;
                    }
                    Event::ChannelData(data) if data.id == channels.reliable => {
                        if let Some(version) = input_protocol_version(&data.data) {
                            input_ready = true;
                            input_ready_state.store(true, Ordering::Release);
                            let _ = outputs.events.send(TransportEvent::InputReady(version));
                        }
                    }
                    Event::ChannelData(data) if data.id == channels.stats => {}
                    Event::MediaData(data) => {
                        let keyframe = data.is_keyframe();
                        let received_at_us = data
                            .network_time
                            .saturating_duration_since(transport_origin)
                            .as_micros()
                            .try_into()
                            .unwrap_or(u64::MAX);
                        let frame = EncodedMediaFrame {
                            mid: data.mid.to_string(),
                            codec: format!("{:?}", data.params.spec().codec),
                            payload: data.data,
                            rtp_timestamp: data.time.numer(),
                            clock_rate_hz: data.time.denom(),
                            received_at_us,
                            keyframe,
                            contiguous: data.contiguous,
                        };
                        if let Err(error) = deliver_media_frame(&outputs.media_consumer, frame) {
                            let _ = outputs
                                .events
                                .send(TransportEvent::Disconnected(error.to_string()));
                            return;
                        }
                    }
                    _ => {}
                },
                Err(error) => {
                    let _ = outputs
                        .events
                        .send(TransportEvent::Disconnected(error.to_string()));
                    return;
                }
            }
        };

        let wait = timeout
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(10));
        if wait.is_zero() {
            let _ = rtc.handle_input(Input::Timeout(Instant::now()));
            continue;
        }

        let _ = socket.set_read_timeout(Some(wait));
        let input = match socket.recv_from(&mut receive_buffer) {
            Ok((length, source)) => {
                let destination = match socket.local_addr() {
                    Ok(value) => value,
                    Err(error) => {
                        let _ = outputs
                            .events
                            .send(TransportEvent::Disconnected(error.to_string()));
                        return;
                    }
                };
                let contents = match receive_buffer[..length].try_into() {
                    Ok(value) => value,
                    Err(error) => {
                        let _ = outputs.events.send(TransportEvent::Log(format!(
                            "Dropping oversized UDP packet: {error}"
                        )));
                        continue;
                    }
                };
                Input::Receive(
                    Instant::now(),
                    Receive {
                        proto: Protocol::Udp,
                        source,
                        destination,
                        contents,
                    },
                )
            }
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                Input::Timeout(Instant::now())
            }
            Err(error) => {
                let _ = outputs
                    .events
                    .send(TransportEvent::Disconnected(error.to_string()));
                return;
            }
        };
        if let Err(error) = rtc.handle_input(input) {
            let _ = outputs
                .events
                .send(TransportEvent::Disconnected(error.to_string()));
            return;
        }
    }
}

fn input_protocol_version(bytes: &[u8]) -> Option<u16> {
    if bytes.len() < 2 {
        return None;
    }
    let first = u16::from_le_bytes([bytes[0], bytes[1]]);
    if first == 526 {
        return Some(if bytes.len() >= 4 {
            u16::from_le_bytes([bytes[2], bytes[3]])
        } else {
            2
        });
    }
    (bytes[0] == 0x0e).then_some(first)
}

#[cfg(test)]
mod tests {
    use super::*;
    use opennow_streamer_protocol::Session;
    use serde_json::json;
    use str0m::media::{Direction, MediaKind};

    fn synthetic_session(media_connection_info: serde_json::Value) -> Session {
        serde_json::from_value(json!({
            "sessionId": "synthetic-session",
            "serverIp": "127-0-0-1.session.synthetic.invalid",
            "iceServers": [],
            "mediaConnectionInfo": media_connection_info,
        }))
        .expect("synthetic session")
    }

    #[test]
    fn parses_both_input_handshake_layouts() {
        assert_eq!(input_protocol_version(&[0x0e, 0x02, 0x03, 0x00]), Some(3));
        assert_eq!(input_protocol_version(&[0x0e, 0x03]), Some(0x030e));
        assert_eq!(input_protocol_version(&[1]), None);
    }

    #[test]
    fn normalizes_synthetic_gfn_media_endpoint_candidates() {
        let session = synthetic_session(json!({
            "ip": "198-51-100-42.media.synthetic.invalid",
            "port": 18_784,
            "usage": 17,
        }));
        let offer = [
            "v=0",
            "c=IN IP4 0.0.0.0",
            "a=candidate:udp 1 udp 2122260223 0.0.0.0 47998 typ host",
            "a=candidate:tcp 1 tcp 1518214911 203.0.113.9 9 typ host tcptype active",
        ]
        .join("\r\n");

        let normalized = normalize_offer_endpoints(&offer, &session).expect("normalized offer");

        assert_eq!(normalized.replacements, 2);
        assert_eq!(
            normalized.media_endpoint,
            Some("198.51.100.42:18784".parse().expect("socket address"))
        );
        assert!(normalized.sdp.contains("c=IN IP4 0.0.0.0"));
        assert!(
            normalized
                .sdp
                .contains("a=candidate:udp 1 udp 2122260223 198.51.100.42 18784 typ host")
        );
        assert!(normalized.sdp.contains(
            "a=candidate:tcp 1 tcp 1518214911 198.51.100.42 18784 typ host tcptype active"
        ));
        assert!(normalized.sdp.contains("\r\n"));
    }

    #[test]
    fn rewrites_trickled_candidate_to_the_normalized_media_endpoint() {
        let endpoint = Some("198.51.100.42:18784".parse().expect("socket address"));
        let candidate = "candidate:remote 1 udp 2122260223 203.0.113.9 47998 typ host";

        assert_eq!(
            normalize_remote_candidate(candidate, endpoint),
            "candidate:remote 1 udp 2122260223 198.51.100.42 18784 typ host"
        );
        assert_eq!(
            normalize_remote_candidate("candidate:malformed", endpoint),
            "candidate:malformed"
        );
    }

    #[test]
    fn skips_non_webrtc_media_endpoint_usage() {
        let session = synthetic_session(json!({
            "ip": "198.51.100.42",
            "port": 18_784,
            "usage": 14,
        }));
        let offer = "v=0\na=candidate:udp 1 udp 1 203.0.113.9 47998 typ host\n";

        let normalized = normalize_offer_endpoints(offer, &session).expect("normalized offer");

        assert_eq!(normalized.sdp, offer);
        assert_eq!(normalized.replacements, 0);
        assert_eq!(normalized.media_endpoint, None);
    }

    #[test]
    fn accepts_ip_encoded_and_dns_hostnames_as_server_endpoints() {
        assert_eq!(
            resolve_server_endpoint("127-0-0-1.session.synthetic.invalid")
                .expect("encoded hostname"),
            "127.0.0.1".parse::<IpAddr>().expect("IP address")
        );
        assert!(
            resolve_server_endpoint("localhost")
                .expect("DNS hostname")
                .is_loopback()
        );
    }

    #[test]
    fn rejects_invalid_media_endpoint_port() {
        let session = synthetic_session(json!({
            "ip": "198.51.100.42",
            "port": 70_000,
            "usage": 2,
        }));

        let error = normalize_offer_endpoints("v=0\n", &session).expect_err("invalid endpoint");

        assert_eq!(error.code(), "invalid-media-endpoint");
        assert!(error.to_string().contains("1..=65535"));
    }

    #[test]
    fn summarizes_configured_ice_schemes_without_credentials() {
        let servers: Vec<IceServer> = serde_json::from_value(json!([
            {
                "urls": ["stun:stun.synthetic.invalid:3478"],
            },
            {
                "urls": ["turns:turn.synthetic.invalid:5349"],
                "username": "synthetic-user",
                "credential": "synthetic-secret"
            }
        ]))
        .expect("ICE servers");

        let schemes = configured_ice_schemes(&servers).expect("configured schemes");

        assert_eq!(schemes, "stun, turns");
        assert!(!schemes.contains("synthetic-secret"));
    }

    #[test]
    fn negotiates_a_synthetic_offer_with_a_hostname_server_endpoint() {
        install_crypto();
        let mut offerer = RtcConfig::new().build(Instant::now());
        offerer.add_local_candidate(
            Candidate::host("127.0.0.1:49152".parse().expect("candidate address"), "udp")
                .expect("local candidate"),
        );
        let mut change = offerer.sdp_api();
        change.add_media(MediaKind::Video, Direction::SendOnly, None, None, None);
        let (offer, _pending) = change.apply().expect("offer");
        let offer_sdp = offer.to_sdp_string();
        let session = synthetic_session(serde_json::Value::Null);
        let (events, _receiver) = mpsc::channel();
        let (media_consumer, _media_receiver) = mpsc::sync_channel(4);

        let negotiated = negotiate(&offer_sdp, &session, 300, events, media_consumer)
            .expect("negotiated answer");

        assert!(negotiated.answer_sdp.contains("m=video"));
        assert!(!negotiated.answer_sdp.contains("m=video 0"));
        negotiated.session.stop();
    }

    #[test]
    fn delivers_encoded_payload_to_typed_consumer_without_copying_arc() {
        let (consumer, receiver) = mpsc::sync_channel(1);
        let payload: Arc<[u8]> = Arc::from([1_u8, 2, 3, 4]);
        let frame = EncodedMediaFrame {
            mid: "video-0".to_owned(),
            codec: "H264".to_owned(),
            payload: payload.clone(),
            rtp_timestamp: 180_000,
            clock_rate_hz: 90_000,
            received_at_us: 2_500,
            keyframe: true,
            contiguous: true,
        };

        deliver_media_frame(&consumer, frame).expect("frame delivery");

        let delivered = receiver.recv().expect("delivered frame");
        assert!(Arc::ptr_eq(&delivered.payload, &payload));
        assert_eq!(delivered.rtp_timestamp, 180_000);
        assert_eq!(delivered.clock_rate_hz, 90_000);
        assert_eq!(delivered.received_at_us, 2_500);
    }

    #[test]
    fn reports_media_consumer_backpressure_instead_of_growing_an_unbounded_queue() {
        let (consumer, _receiver) = mpsc::sync_channel(1);
        let frame = || EncodedMediaFrame {
            mid: "video-0".to_owned(),
            codec: "H264".to_owned(),
            payload: Arc::from([1_u8, 2, 3, 4]),
            rtp_timestamp: 180_000,
            clock_rate_hz: 90_000,
            received_at_us: 2_500,
            keyframe: true,
            contiguous: true,
        };
        deliver_media_frame(&consumer, frame()).expect("first frame delivery");

        let error = deliver_media_frame(&consumer, frame()).expect_err("bounded queue is full");

        assert_eq!(error.code(), "media-consumer-backpressured");
    }
}
