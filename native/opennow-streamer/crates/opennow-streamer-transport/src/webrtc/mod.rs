use std::io::ErrorKind;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use opennow_streamer_protocol::Session;
use serde::{Deserialize, Serialize};
use str0m::change::SdpOffer;
use str0m::format::Codec;
use str0m::media::{KeyframeRequestKind, MediaData, Mid};
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, IceConnectionState, Input, Output, Rtc, RtcConfig};
use thiserror::Error;

use crate::{EncodedMediaFrame, MediaConsumer, install_crypto};

mod input;
#[cfg(test)]
mod tests;

use input::{InputChannelState, InputChannels};

const COMMAND_CAPACITY: usize = 64;
const MAX_INPUT_BYTES: usize = 4096;
const MAX_SDP_BYTES: usize = 256 * 1024;
const MAX_CANDIDATE_BYTES: usize = 4096;
const MAX_CANDIDATES: usize = 64;
const MAX_VIDEO_BYTES: usize = 16 * 1024 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const INPUT_TIMEOUT: Duration = Duration::from_secs(5);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(2);
const KEYFRAME_INTERVAL: Duration = Duration::from_millis(500);
const MAX_FLUSH_TIME: Duration = Duration::from_millis(150);

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IceCandidate {
    pub candidate: String,
    #[serde(default)]
    pub sdp_mid: Option<String>,
    #[serde(default)]
    pub sdp_m_line_index: Option<u16>,
    #[serde(default)]
    pub username_fragment: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NegotiatedVideoCodec {
    H264,
}

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("invalid WebRTC offer: {0}")]
    Offer(String),
    #[error("invalid WebRTC endpoint: {0}")]
    Endpoint(String),
    #[error("WebRTC socket failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid remote ICE candidate: {0}")]
    RemoteCandidate(String),
    #[error(
        "native WebRTC currently supports direct UDP only; TURN relay gathering is unavailable"
    )]
    RelayUnsupported,
    #[error("input channel is not ready")]
    InputNotReady,
    #[error("WebRTC command queue is full")]
    Backpressured,
    #[error("WebRTC command exceeds its size limit")]
    OversizedCommand,
    #[error("input mode or opcode is unsupported in WebRTC compatibility mode")]
    UnsupportedInput,
    #[error("input packet has an invalid length")]
    MalformedInput,
    #[error("WebRTC transport is closed")]
    Closed,
    #[error("WebRTC input flush timed out")]
    FlushTimeout,
}

impl TransportError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Offer(_) => "invalid-offer",
            Self::Endpoint(_) => "invalid-media-endpoint",
            Self::Io(_) => "local-transport-failed",
            Self::RemoteCandidate(_) => "invalid-remote-candidate",
            Self::RelayUnsupported => "webrtc-relay-unsupported",
            Self::InputNotReady => "input-not-ready",
            Self::Backpressured => "transport-backpressured",
            Self::OversizedCommand => "transport-command-too-large",
            Self::UnsupportedInput => "webrtc-input-unsupported",
            Self::MalformedInput => "invalid-input",
            Self::Closed => "transport-closed",
            Self::FlushTimeout => "transport-flush-timeout",
        }
    }
}

#[derive(Debug)]
pub enum TransportEvent {
    Connected,
    Disconnected(String),
    InputReady(u16),
    InputUnavailable(String),
    Log(String),
}

enum TransportCommand {
    AddRemoteCandidate(Candidate),
    SendInput {
        bytes: Vec<u8>,
        partially_reliable: bool,
    },
    RequestKeyframe(Mid),
    FlushInput(SyncSender<Result<(), TransportError>>),
}

#[derive(Clone)]
pub struct TransportControl {
    commands: SyncSender<TransportCommand>,
    cancelled: Arc<AtomicBool>,
    input_ready: Arc<AtomicBool>,
}

impl TransportControl {
    fn enqueue(&self, command: TransportCommand) -> Result<(), TransportError> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err(TransportError::Closed);
        }
        self.commands
            .try_send(command)
            .map_err(|error| match error {
                TrySendError::Full(_) => TransportError::Backpressured,
                TrySendError::Disconnected(_) => TransportError::Closed,
            })
    }

    pub fn send_input(
        &self,
        bytes: Vec<u8>,
        partially_reliable: bool,
    ) -> Result<(), TransportError> {
        if partially_reliable {
            return Err(TransportError::UnsupportedInput);
        }
        if bytes.len() > MAX_INPUT_BYTES {
            return Err(TransportError::OversizedCommand);
        }
        input::validate_packet(&bytes)?;
        if !self.input_ready.load(Ordering::Acquire) {
            return Err(TransportError::InputNotReady);
        }
        self.enqueue(TransportCommand::SendInput {
            bytes,
            partially_reliable,
        })
    }

    pub fn request_keyframe(&self, mid: impl Into<String>) -> Result<(), TransportError> {
        let mid = mid.into();
        if mid.is_empty() || mid.len() > 16 || !mid.is_ascii() {
            return Err(TransportError::OversizedCommand);
        }
        self.enqueue(TransportCommand::RequestKeyframe(Mid::from(mid.as_str())))
    }

    pub fn flush_input(&self, timeout: Duration) -> Result<(), TransportError> {
        let deadline = Instant::now() + timeout.min(MAX_FLUSH_TIME);
        let (acknowledge, completed) = mpsc::sync_channel(1);
        let mut command = TransportCommand::FlushInput(acknowledge);
        loop {
            if self.cancelled.load(Ordering::Acquire) {
                return Err(TransportError::Closed);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(TransportError::FlushTimeout);
            }
            match self.commands.try_send(command) {
                Ok(()) => break,
                Err(TrySendError::Disconnected(_)) => return Err(TransportError::Closed),
                Err(TrySendError::Full(pending)) => command = pending,
            }
            thread::sleep(remaining.min(Duration::from_millis(1)));
        }
        loop {
            if self.cancelled.load(Ordering::Acquire) {
                return Err(TransportError::Closed);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(TransportError::FlushTimeout);
            }
            match completed.recv_timeout(remaining.min(POLL_INTERVAL)) {
                Ok(result) => return result,
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err(TransportError::Closed),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }

    pub fn stop(&self) {
        self.input_ready.store(false, Ordering::Release);
        self.cancelled.store(true, Ordering::Release);
    }
}

pub struct TransportSession {
    control: TransportControl,
    worker: Option<JoinHandle<()>>,
    media_endpoint: Option<SocketAddr>,
}

impl TransportSession {
    pub fn control(&self) -> TransportControl {
        self.control.clone()
    }

    pub fn add_remote_candidate(&self, candidate: &IceCandidate) -> Result<(), TransportError> {
        if let Some(candidate) = parse_candidate(&candidate.candidate, self.media_endpoint)? {
            self.control
                .enqueue(TransportCommand::AddRemoteCandidate(candidate))?;
        }
        Ok(())
    }

    pub fn stop(self) {
        drop(self);
    }
}

impl Drop for TransportSession {
    fn drop(&mut self) {
        self.control.stop();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub struct NegotiatedTransport {
    pub answer_sdp: String,
    pub local_candidate: IceCandidate,
    pub session: TransportSession,
}

pub fn negotiate(
    offer_sdp: &str,
    session: &Session,
    codec: NegotiatedVideoCodec,
    partial_reliable_lifetime_ms: u16,
    events: SyncSender<TransportEvent>,
    media_consumer: MediaConsumer,
) -> Result<NegotiatedTransport, TransportError> {
    install_crypto();
    if offer_sdp.len() > MAX_SDP_BYTES {
        return Err(TransportError::Offer("SDP exceeds 256 KiB".to_owned()));
    }
    if session
        .extra
        .get("iceTransportPolicy")
        .and_then(serde_json::Value::as_str)
        == Some("relay")
    {
        return Err(TransportError::RelayUnsupported);
    }
    let media_endpoint = media_endpoint(session)?;
    let normalized = normalize_offer(offer_sdp, media_endpoint)?;
    let peer = match media_endpoint {
        Some(endpoint) => endpoint.ip(),
        None => resolve_host(&session.server_ip)?,
    };
    let socket = bind_routed_socket(peer)?;
    socket.set_write_timeout(Some(POLL_INTERVAL))?;
    let local = Candidate::host(socket.local_addr()?, "udp")
        .map_err(|error| TransportError::Endpoint(error.to_string()))?;
    let mut builder = RtcConfig::new()
        .clear_codecs()
        .enable_opus(true)
        .set_reordering_size_audio(32)
        .set_reordering_size_video(512)
        .set_send_buffer_audio(1)
        .set_send_buffer_video(1);
    match codec {
        NegotiatedVideoCodec::H264 => {
            for (payload, profile) in [
                (96_u8, 0x42002a),
                (98, 0x42e02a),
                (100, 0x4d002a),
                (102, 0x64002a),
            ] {
                builder.codec_config().add_h264(
                    payload.into(),
                    Some((payload + 1).into()),
                    true,
                    profile,
                );
            }
        }
    }
    let mut rtc = builder.build(Instant::now());
    rtc.add_local_candidate(local.clone());
    let offer = SdpOffer::from_sdp_string(&normalized)
        .map_err(|error| TransportError::Offer(error.to_string()))?;
    let answer = rtc
        .sdp_api()
        .accept_offer(offer)
        .map_err(|error| TransportError::Offer(error.to_string()))?;
    let answer_sdp = answer.to_sdp_string();
    let (candidate_mid, candidate_index) = candidate_target(&answer_sdp)?;
    let active_track = |kind: &str| {
        answer_sdp.lines().any(|line| {
            let mut tokens = line.split_ascii_whitespace();
            tokens.next() == Some(kind) && tokens.next().is_some_and(|port| port != "0")
        })
    };
    if !active_track("m=video")
        || !answer_sdp
            .lines()
            .any(|line| line.starts_with("a=rtpmap:") && line.contains("H264/90000"))
    {
        return Err(TransportError::Offer(
            "peer did not negotiate H264 video".to_owned(),
        ));
    }
    if !active_track("m=application") {
        return Err(TransportError::Offer(
            "peer did not negotiate input data channels".to_owned(),
        ));
    }
    let channels = InputChannels::create(&mut rtc, partial_reliable_lifetime_ms);
    let (commands, receiver) = mpsc::sync_channel(COMMAND_CAPACITY);
    let control = TransportControl {
        commands,
        cancelled: Arc::new(AtomicBool::new(false)),
        input_ready: Arc::new(AtomicBool::new(false)),
    };
    let cancelled = Arc::clone(&control.cancelled);
    let input_ready = Arc::clone(&control.input_ready);
    let worker = thread::Builder::new()
        .name("opennow-webrtc".to_owned())
        .spawn(move || {
            let mut runtime = TransportWorker {
                rtc,
                socket,
                commands: receiver,
                events,
                media_consumer,
                channels,
                input: InputChannelState::default(),
                input_ready,
                cancelled,
                origin: Instant::now(),
                connected_at: None,
                next_heartbeat: Instant::now() + HEARTBEAT_INTERVAL,
                video: VideoRecovery::default(),
                remote_candidates: normalized
                    .lines()
                    .filter(|line| line.starts_with("a=candidate:"))
                    .count(),
                video_drops: 0,
                audio_drops: 0,
                last_drop_report: Instant::now(),
            };
            let result = runtime.run();
            runtime.input_ready.store(false, Ordering::Release);
            runtime.rtc.disconnect();
            drop(runtime.commands);
            drop(runtime.socket);
            drop(runtime.rtc);
            let mut event =
                TransportEvent::Disconnected(result.err().unwrap_or_else(|| "stopped".to_owned()));
            loop {
                match runtime.events.try_send(event) {
                    Ok(()) | Err(TrySendError::Disconnected(_)) => break,
                    Err(TrySendError::Full(pending)) => event = pending,
                }
                if runtime.cancelled.load(Ordering::Acquire) {
                    break;
                }
                thread::sleep(POLL_INTERVAL);
            }
        })?;
    Ok(NegotiatedTransport {
        answer_sdp,
        local_candidate: IceCandidate {
            candidate: local.to_sdp_string(),
            sdp_mid: Some(candidate_mid),
            sdp_m_line_index: Some(candidate_index),
            username_fragment: None,
        },
        session: TransportSession {
            control,
            worker: Some(worker),
            media_endpoint,
        },
    })
}

fn resolve_host(host: &str) -> Result<IpAddr, TransportError> {
    if host.is_empty() || host.len() > 253 {
        return Err(TransportError::Endpoint(
            "invalid hostname length".to_owned(),
        ));
    }
    if let Ok(ip) = host.parse() {
        return Ok(ip);
    }
    if let Some(label) = host.split('.').next() {
        let octets = label
            .split('-')
            .map(str::parse::<u8>)
            .collect::<Result<Vec<_>, _>>();
        if let Ok(octets) = octets {
            if let Ok(octets) = <[u8; 4]>::try_from(octets) {
                return Ok(IpAddr::V4(octets.into()));
            }
        }
    }
    Err(TransportError::Endpoint(
        "caller must resolve media hostname before starting WebRTC".to_owned(),
    ))
}

fn media_endpoint(session: &Session) -> Result<Option<SocketAddr>, TransportError> {
    let Some(endpoint) = session
        .media_connection_info
        .as_ref()
        .filter(|endpoint| matches!(endpoint.usage, Some(2 | 14 | 15 | 17)))
    else {
        return Ok(None);
    };
    let port = u16::try_from(endpoint.port)
        .ok()
        .filter(|port| *port != 0)
        .ok_or_else(|| TransportError::Endpoint("media port is outside 1..=65535".to_owned()))?;
    Ok(Some(SocketAddr::new(resolve_host(&endpoint.ip)?, port)))
}

fn normalize_offer(sdp: &str, endpoint: Option<SocketAddr>) -> Result<String, TransportError> {
    let mut lines = Vec::new();
    let mut candidates = 0;
    let mut direct_candidates = 0;
    let mut relay_candidates = 0;
    let mut media_lines = 0;
    for line in sdp.lines() {
        if line.starts_with("a=candidate:") {
            candidates += 1;
            if candidates > MAX_CANDIDATES {
                return Err(TransportError::Offer("too many ICE candidates".to_owned()));
            }
            if let Some(candidate) = parse_candidate(line, endpoint)? {
                if line.split_ascii_whitespace().nth(7) == Some("relay") {
                    relay_candidates += 1;
                } else {
                    direct_candidates += 1;
                }
                lines.push(format!("a={}", candidate.to_sdp_string()));
            }
        } else {
            if line.starts_with("m=") {
                media_lines += 1;
                if media_lines > 3 {
                    return Err(TransportError::Offer(
                        "only one video, audio and data track are supported".to_owned(),
                    ));
                }
            }
            lines.push(line.to_owned());
        }
    }
    if relay_candidates > 0 && direct_candidates == 0 {
        return Err(TransportError::RelayUnsupported);
    }
    Ok(lines.join("\r\n") + "\r\n")
}

fn parse_candidate(
    text: &str,
    endpoint: Option<SocketAddr>,
) -> Result<Option<Candidate>, TransportError> {
    if text.len() > MAX_CANDIDATE_BYTES {
        return Err(TransportError::OversizedCommand);
    }
    let text = text.trim();
    let text = text.strip_prefix("a=").unwrap_or(text);
    if text.is_empty() || text == "end-of-candidates" {
        return Ok(None);
    }
    let mut tokens: Vec<_> = text.split_ascii_whitespace().map(str::to_owned).collect();
    if tokens.len() < 8 || !tokens[0].starts_with("candidate:") {
        return Err(TransportError::RemoteCandidate(
            "malformed candidate".to_owned(),
        ));
    }
    if !tokens[2].eq_ignore_ascii_case("udp") {
        return Ok(None);
    }
    if let Some(endpoint) = endpoint {
        let needs_rewrite = match tokens[4].parse::<IpAddr>() {
            Ok(IpAddr::V4(ip)) => {
                ip.is_private() || ip.is_loopback() || ip.is_unspecified() || ip.is_link_local()
            }
            Ok(IpAddr::V6(ip)) => {
                ip.is_unique_local()
                    || ip.is_loopback()
                    || ip.is_unspecified()
                    || ip.is_unicast_link_local()
            }
            Err(_) => true,
        };
        if tokens[7] == "host" && needs_rewrite {
            tokens[4] = endpoint.ip().to_string();
            tokens[5] = endpoint.port().to_string();
        }
    }
    Candidate::from_sdp_string(&tokens.join(" "))
        .map(Some)
        .map_err(|error| TransportError::RemoteCandidate(error.to_string()))
}

fn candidate_target(sdp: &str) -> Result<(String, u16), TransportError> {
    let bundled = sdp
        .lines()
        .find_map(|line| line.strip_prefix("a=group:BUNDLE "))
        .and_then(|mids| mids.split_ascii_whitespace().next());
    let mut index = None;
    let mut active = false;
    let mut first = None;
    for line in sdp.lines() {
        if line.starts_with("m=") {
            index = Some(index.map_or(0_u16, |index| index + 1));
            active = line.split_ascii_whitespace().nth(1) != Some("0");
        } else if active {
            if let Some(mid) = line.strip_prefix("a=mid:") {
                let target = (mid.to_owned(), index.unwrap_or(0));
                if bundled == Some(mid) {
                    return Ok(target);
                }
                first.get_or_insert(target);
            }
        }
    }
    first.ok_or_else(|| TransportError::Offer("answer has no active media id".to_owned()))
}

fn bind_routed_socket(peer: IpAddr) -> Result<UdpSocket, TransportError> {
    let unspecified = if peer.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    };
    let probe = UdpSocket::bind(unspecified)?;
    probe.connect(SocketAddr::new(peer, 9))?;
    let socket = UdpSocket::bind(SocketAddr::new(probe.local_addr()?.ip(), 0))?;
    #[cfg(windows)]
    let socket = {
        let socket = socket2::Socket::from(socket);
        crate::nvst::disable_udp_connreset(&socket)?;
        UdpSocket::from(socket)
    };
    Ok(socket)
}

fn transient_udp_error(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::ConnectionReset | ErrorKind::ConnectionRefused | ErrorKind::Interrupted
    )
}

#[derive(Default)]
struct VideoRecovery {
    mid: Option<Mid>,
    ssrc: Option<u32>,
    reference_ready: bool,
    last_request: Option<Instant>,
}

struct TransportWorker {
    rtc: Rtc,
    socket: UdpSocket,
    commands: Receiver<TransportCommand>,
    events: SyncSender<TransportEvent>,
    media_consumer: MediaConsumer,
    channels: InputChannels,
    input: InputChannelState,
    input_ready: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
    origin: Instant,
    connected_at: Option<Instant>,
    next_heartbeat: Instant,
    video: VideoRecovery,
    remote_candidates: usize,
    video_drops: u64,
    audio_drops: u64,
    last_drop_report: Instant,
}

impl TransportWorker {
    fn emit(&self, event: TransportEvent) -> Result<(), String> {
        self.events
            .try_send(event)
            .map_err(|_| "WebRTC event consumer is unavailable or backpressured".to_owned())
    }

    fn request_keyframe(&mut self, mid: Mid) {
        let now = Instant::now();
        if self
            .video
            .last_request
            .is_some_and(|last| now.duration_since(last) < KEYFRAME_INTERVAL)
        {
            return;
        }
        if let Some(mut writer) = self.rtc.writer(mid) {
            let kind = [KeyframeRequestKind::Pli, KeyframeRequestKind::Fir]
                .into_iter()
                .find(|kind| writer.is_request_keyframe_possible(*kind));
            if let Some(kind) = kind {
                if writer.request_keyframe(None, kind).is_ok() {
                    self.video.last_request = Some(now);
                }
            }
        }
    }

    fn media(&mut self, data: MediaData) -> Result<(), String> {
        let codec = data.params.spec().codec;
        if !matches!(codec, Codec::H264 | Codec::Opus) {
            return Err("received an unnegotiated media codec".to_owned());
        }
        let is_video = codec == Codec::H264;
        let keyframe = data.is_keyframe();
        let ssrc = self
            .rtc
            .direct_api()
            .stream_rx_by_mid(data.mid, data.rid)
            .map(|stream| *stream.ssrc());
        if is_video {
            if self.video.mid.is_some_and(|mid| mid != data.mid) {
                return Err("multiple video tracks are unsupported".to_owned());
            }
            self.video.mid = Some(data.mid);
            if self.video.ssrc != ssrc || !data.contiguous || data.data.len() > MAX_VIDEO_BYTES {
                self.video.reference_ready = false;
            }
            self.video.ssrc = ssrc;
            if data.data.len() > MAX_VIDEO_BYTES || (!self.video.reference_ready && !keyframe) {
                self.video_drops = self.video_drops.saturating_add(1);
                self.request_keyframe(data.mid);
                return Ok(());
            }
        }
        let frame = EncodedMediaFrame {
            mid: data.mid.to_string(),
            codec: if is_video { "H264" } else { "Opus" }.to_owned(),
            payload: data.data,
            frame_index: None,
            rtp_timestamp: data.time.numer() & u64::from(u32::MAX),
            clock_rate_hz: data.time.denom(),
            channels: data.params.spec().channels,
            received_at_us: data
                .network_time
                .saturating_duration_since(self.origin)
                .as_micros()
                .try_into()
                .unwrap_or(u64::MAX),
            keyframe,
            contiguous: data.contiguous && (!is_video || self.video.reference_ready),
            ssrc,
        };
        match self.media_consumer.try_send(frame) {
            Ok(()) => {
                if is_video && keyframe {
                    self.video.reference_ready = true;
                }
                Ok(())
            }
            Err(TrySendError::Full(_)) => {
                if is_video {
                    self.video_drops = self.video_drops.saturating_add(1);
                    self.video.reference_ready = false;
                    self.request_keyframe(data.mid);
                } else {
                    self.audio_drops = self.audio_drops.saturating_add(1);
                }
                Ok(())
            }
            Err(TrySendError::Disconnected(_)) => Err("encoded media consumer closed".to_owned()),
        }
    }

    fn event(&mut self, event: Event) -> Result<(), String> {
        match event {
            Event::Connected => {
                self.connected_at = Some(Instant::now());
                self.emit(TransportEvent::Connected)?;
            }
            Event::IceConnectionStateChange(IceConnectionState::Disconnected) => {
                return Err(
                    "ICE disconnected (direct UDP only; TURN relay gathering is unavailable)"
                        .to_owned(),
                );
            }
            Event::ChannelOpen(id, _) => {
                if let Some(version) = self.input.channel_opened(self.channels, id) {
                    self.input_ready.store(true, Ordering::Release);
                    self.emit(TransportEvent::InputReady(version))?;
                }
            }
            Event::ChannelData(data) => {
                if let Some(version) = self.input.channel_data(self.channels, data.id, &data.data) {
                    self.input_ready.store(true, Ordering::Release);
                    self.emit(TransportEvent::InputReady(version))?;
                }
            }
            Event::ChannelClose(id) if self.input.channel_closed(self.channels, id) => {
                self.input_ready.store(false, Ordering::Release);
                self.emit(TransportEvent::InputUnavailable(
                    "input data channel closed".to_owned(),
                ))?;
                return Err("input data channel closed".to_owned());
            }
            Event::MediaData(data) => self.media(data)?,
            _ => {}
        }
        Ok(())
    }

    fn run(&mut self) -> Result<(), String> {
        let mut bytes = [0_u8; 65_536];
        let destination = self
            .socket
            .local_addr()
            .map_err(|error| error.to_string())?;
        loop {
            if self.cancelled.load(Ordering::Acquire) {
                return Ok(());
            }
            let now = Instant::now();
            match self.connected_at {
                None if now.duration_since(self.origin) >= CONNECT_TIMEOUT => return Err("WebRTC connection timed out (direct UDP only; TURN relay gathering is unavailable)".to_owned()),
                Some(connected) if !self.input.is_ready() && now.duration_since(connected) >= INPUT_TIMEOUT => return Err("WebRTC input handshake timed out".to_owned()),
                _ => {}
            }
            let mut flush = None;
            for _ in 0..COMMAND_CAPACITY {
                match self.commands.try_recv() {
                    Ok(TransportCommand::AddRemoteCandidate(candidate)) => {
                        self.remote_candidates += 1;
                        if self.remote_candidates > MAX_CANDIDATES {
                            return Err("too many trickled ICE candidates".to_owned());
                        }
                        self.rtc.add_remote_candidate(candidate);
                    }
                    Ok(TransportCommand::SendInput {
                        bytes,
                        partially_reliable,
                    }) => {
                        let bytes = self.input.encode(&bytes, Instant::now())?;
                        if !self.input.is_ready()
                            || !self
                                .channels
                                .send(&mut self.rtc, &bytes, partially_reliable)
                        {
                            return Err("input channel is unavailable or backpressured".to_owned());
                        }
                    }
                    Ok(TransportCommand::RequestKeyframe(mid)) => {
                        self.video.reference_ready = false;
                        self.request_keyframe(mid);
                    }
                    Ok(TransportCommand::FlushInput(acknowledge)) => {
                        flush = Some(acknowledge);
                        self.rtc
                            .handle_input(Input::Timeout(Instant::now()))
                            .map_err(|error| error.to_string())?;
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
                }
            }
            if self.input.is_ready() && now >= self.next_heartbeat {
                if !self.channels.send(&mut self.rtc, &[2, 0, 0, 0], false) {
                    return Err("input heartbeat is backpressured".to_owned());
                }
                self.next_heartbeat = now + HEARTBEAT_INTERVAL;
            }
            if !self.video.reference_ready {
                if let Some(mid) = self.video.mid {
                    self.request_keyframe(mid);
                }
            }
            if now.duration_since(self.last_drop_report) >= Duration::from_secs(1)
                && (self.video_drops > 0 || self.audio_drops > 0)
            {
                if self
                    .events
                    .try_send(TransportEvent::Log(format!(
                        "WebRTC media dropped video={} audio={} awaiting_keyframe={}",
                        self.video_drops, self.audio_drops, !self.video.reference_ready,
                    )))
                    .is_ok()
                {
                    self.video_drops = 0;
                    self.audio_drops = 0;
                }
                self.last_drop_report = now;
            }
            let mut flush_error = None;
            let timeout = loop {
                if self.cancelled.load(Ordering::Acquire) {
                    return Ok(());
                }
                match self.rtc.poll_output().map_err(|error| error.to_string())? {
                    Output::Timeout(timeout) => break timeout,
                    Output::Transmit(packet) => {
                        if let Err(error) =
                            self.socket.send_to(&packet.contents, packet.destination)
                        {
                            if !transient_udp_error(&error) {
                                return Err(error.to_string());
                            }
                            flush_error = Some(error);
                        }
                    }
                    Output::Event(event) => self.event(event)?,
                }
            };
            if let Some(acknowledge) = flush {
                let _ = acknowledge
                    .try_send(flush_error.map_or(Ok(()), |error| Err(TransportError::Io(error))));
            }
            let now = Instant::now();
            let wait = timeout.saturating_duration_since(now).min(POLL_INTERVAL);
            if wait.is_zero() {
                self.rtc
                    .handle_input(Input::Timeout(now))
                    .map_err(|error| error.to_string())?;
                continue;
            }
            self.socket
                .set_read_timeout(Some(wait))
                .map_err(|error| error.to_string())?;
            match self.socket.recv_from(&mut bytes) {
                Ok((length, source)) => {
                    let Ok(contents) = (&bytes[..length]).try_into() else {
                        continue;
                    };
                    let input = Input::Receive(
                        Instant::now(),
                        Receive {
                            proto: Protocol::Udp,
                            source,
                            destination,
                            contents,
                        },
                    );
                    if self.rtc.accepts(&input) {
                        self.rtc
                            .handle_input(input)
                            .map_err(|error| error.to_string())?;
                    }
                }
                Err(error)
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)
                        || transient_udp_error(&error) =>
                {
                    self.rtc
                        .handle_input(Input::Timeout(Instant::now()))
                        .map_err(|error| error.to_string())?;
                }
                Err(error) => return Err(error.to_string()),
            }
        }
    }
}
