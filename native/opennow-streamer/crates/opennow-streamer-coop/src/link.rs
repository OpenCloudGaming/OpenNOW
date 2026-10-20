//! One peer-to-peer link: a str0m 'Rtc' with its own DTLS-SRTP keys, host and
//! server-reflexive UDP candidates, and TURN relayed candidates added only after the direct
//! attempt times out.

use std::collections::VecDeque;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rustls::ClientConfig;
use str0m::change::{SdpAnswer, SdpOffer, SdpPendingOffer};
use str0m::channel::{ChannelConfig, ChannelId, Reliability};
use str0m::format::{Codec, FormatParams};
use str0m::media::{Direction, Frequency, MediaKind, Mid, Pt};
use str0m::net::{Protocol, Receive};
use str0m::rtp::{RtpPacket, RtpWrite, SeqNo, Ssrc};
use str0m::{Candidate, CandidateKind, Event, IceConnectionState, Input, Output, Rtc, RtcConfig};

use crate::config::{IceConfig, RelaySite, TurnServer, resolve};
use crate::log;
use crate::ring::RingEntry;
use crate::stun::{self, BINDING, CLASS_REQUEST, CLASS_SUCCESS, Message, MessageBuilder};
use crate::turn::{TurnClient, TurnEvent};

pub const CONTROL_LABEL: &str = "coop-control";
pub const INPUT_LABEL: &str = "coop-input";
pub const VIDEO_PT: u8 = 96;
pub const AUDIO_PT: u8 = 111;
pub const DEFAULT_DIRECT_TIMEOUT: Duration = Duration::from_secs(4);
const MAX_TURN_CLIENTS: usize = 6;
const MAX_HOST_SOCKETS: usize = 4;
const MAX_PENDING_REMOTE_CANDIDATES: usize = 32;
const MAX_EVENTS: usize = 1_024;
const SRFLX_RETRY: Duration = Duration::from_millis(500);
const SRFLX_ATTEMPTS: u32 = 4;
const MAX_OWN_IPS: usize = 4;
/// Longest gap between ICE checks on the nominated pair. Each side checks the pair at least
/// this often, so a live link always carries inbound traffic in both directions.
const MAX_CHECK_INTERVAL: Duration = Duration::from_secs(1);

/// Network policy for a link. Production uses the routed host address and platform TLS roots.
#[derive(Clone)]
pub struct NetConfig {
    pub host_ips: Option<Vec<IpAddr>>,
    pub allow_direct: bool,
    pub direct_timeout: Duration,
    pub tls: Option<Arc<ClientConfig>>,
}

impl Default for NetConfig {
    fn default() -> Self {
        Self {
            host_ips: None,
            allow_direct: true,
            direct_timeout: DEFAULT_DIRECT_TIMEOUT,
            tls: None,
        }
    }
}

impl std::fmt::Debug for NetConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NetConfig")
            .field("host_ips", &self.host_ips)
            .field("allow_direct", &self.allow_direct)
            .field("direct_timeout", &self.direct_timeout)
            .finish()
    }
}

/// ICE servers resolved once per co-op start, so DNS never runs on the media loop.
#[derive(Debug, Clone, Default)]
pub struct ResolvedIce {
    pub stun: Vec<SocketAddr>,
    pub turn: Vec<(TurnServer, SocketAddr)>,
    relays: Vec<RelaySite>,
    relay_ips: Vec<(IpAddr, String)>,
}

impl ResolvedIce {
    pub fn resolve(config: &IceConfig) -> Self {
        let mut stun: Vec<SocketAddr> = Vec::new();
        for server in &config.stun {
            for address in resolve(&server.host, server.port).into_iter().take(2) {
                if !stun.contains(&address) {
                    stun.push(address);
                }
            }
        }
        let turn = config
            .turn
            .iter()
            .filter_map(|server| {
                resolve(&server.host, server.port)
                    .into_iter()
                    .next()
                    .map(|address| (server.clone(), address))
            })
            .take(MAX_TURN_CLIENTS)
            .collect();
        Self {
            stun,
            turn,
            relays: config.relays.clone(),
            relay_ips: config.resolve_relay_addresses(),
        }
    }

    fn relay_id(&self, host: Option<&str>, ip: IpAddr) -> Option<String> {
        host.and_then(|host| {
            self.relays.iter().find(|relay| {
                relay
                    .hosts
                    .iter()
                    .any(|candidate| candidate.eq_ignore_ascii_case(host))
            })
        })
        .map(|relay| relay.id.clone())
        .or_else(|| {
            self.relay_ips
                .iter()
                .find(|(candidate, _)| *candidate == ip)
                .map(|(_, id)| id.clone())
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Path {
    Direct,
    Relay(Option<String>),
}

#[derive(Debug)]
pub enum LinkEvent {
    LocalCandidate(String),
    DirectBlocked,
    /// Every TURN allocation failed; true when a server rejected the credentials.
    RelayUnavailable {
        credentials_rejected: bool,
    },
    Ice(IceConnectionState),
    Connected,
    ChannelOpen(ChannelId, String),
    ChannelData(ChannelId, Vec<u8>),
    ChannelClosed(ChannelId),
    Rtp(Box<RtpPacket>),
    AppFeedback {
        media_ssrc: Ssrc,
        payload: Arc<[u8]>,
    },
    KeyframeRequest,
    Path(Path),
    Closed,
}

struct HostSocket {
    socket: UdpSocket,
    local: SocketAddr,
}

struct SrflxProbe {
    transaction: stun::TransactionId,
    server: SocketAddr,
    socket: usize,
    sent_at: Instant,
    attempts: u32,
    done: bool,
}

pub struct PeerLink {
    pub rtc: Rtc,
    ice: Arc<ResolvedIce>,
    net: NetConfig,
    hosts: Vec<HostSocket>,
    probes: Vec<SrflxProbe>,
    turns: Vec<TurnClient>,
    relay_started: bool,
    started_at: Instant,
    ice_connected: bool,
    dtls_connected: bool,
    pending_offer: Option<SdpPendingOffer>,
    remote_described: bool,
    pending_remote: Vec<Candidate>,
    remote_relays: Vec<SocketAddr>,
    selected_pair: Option<(SocketAddr, SocketAddr)>,
    /// This client's public addresses, from STUN and TURN mapped addresses.
    own_ips: Vec<IpAddr>,
    events: VecDeque<LinkEvent>,
    next_timeout: Instant,
    path: Option<Path>,
    rtt: Option<Duration>,
    egress_loss: Option<f32>,
    pub video_mid: Option<Mid>,
    pub audio_mid: Option<Mid>,
    pub control: Option<ChannelId>,
    pub input: Option<ChannelId>,
    bytes_sent: u64,
    bytes_received: u64,
    last_inbound: Instant,
}

fn routed_host_ips() -> Vec<IpAddr> {
    opennow_streamer_transport::advertised_nvst_ipv4()
        .into_iter()
        .collect()
}

impl PeerLink {
    pub fn new(ice: Arc<ResolvedIce>, net: NetConfig, now: Instant) -> std::io::Result<Self> {
        opennow_streamer_transport::install_crypto();
        let mut config = RtcConfig::new()
            .set_rtp_mode(true)
            .set_stats_interval(Some(Duration::from_secs(1)))
            .clear_codecs();
        config.set_max_stun_rto(MAX_CHECK_INTERVAL);
        config.codec_config().add_config(
            Pt::from(VIDEO_PT),
            None,
            Codec::H264,
            Frequency::NINETY_KHZ,
            None,
            FormatParams::default(),
        );
        config.codec_config().add_config(
            Pt::from(AUDIO_PT),
            None,
            Codec::Opus,
            Frequency::FORTY_EIGHT_KHZ,
            Some(2),
            FormatParams::default(),
        );
        let mut rtc = config.build(now);
        let mut hosts = Vec::new();
        if net.allow_direct {
            let ips = net.host_ips.clone().unwrap_or_else(routed_host_ips);
            for ip in ips.into_iter().take(MAX_HOST_SOCKETS) {
                let socket = UdpSocket::bind(SocketAddr::new(ip, 0))?;
                socket.set_nonblocking(true)?;
                let local = socket.local_addr()?;
                if let Ok(candidate) = Candidate::host(local, "udp") {
                    rtc.add_local_candidate(candidate);
                }
                hosts.push(HostSocket { socket, local });
            }
        }
        let mut link = Self {
            rtc,
            ice,
            net,
            hosts,
            probes: Vec::new(),
            turns: Vec::new(),
            relay_started: false,
            started_at: now,
            ice_connected: false,
            dtls_connected: false,
            pending_offer: None,
            remote_described: false,
            pending_remote: Vec::new(),
            remote_relays: Vec::new(),
            selected_pair: None,
            own_ips: Vec::new(),
            events: VecDeque::new(),
            next_timeout: now,
            path: None,
            rtt: None,
            egress_loss: None,
            video_mid: None,
            audio_mid: None,
            control: None,
            input: None,
            bytes_sent: 0,
            bytes_received: 0,
            last_inbound: now,
        };
        link.start_srflx(now);
        if !link.net.allow_direct {
            link.start_relays(now);
        }
        Ok(link)
    }

    fn push(&mut self, event: LinkEvent) {
        if self.events.len() < MAX_EVENTS {
            self.events.push_back(event);
        }
    }

    fn start_srflx(&mut self, now: Instant) {
        for (index, host) in self.hosts.iter().enumerate() {
            for server in self.ice.stun.iter().take(4) {
                if server.is_ipv4() != host.local.is_ipv4() {
                    continue;
                }
                let transaction = stun::new_transaction_id();
                let request = MessageBuilder::new(BINDING, CLASS_REQUEST, &transaction)
                    .fingerprint()
                    .build();
                let _ = host.socket.send_to(&request, server);
                self.probes.push(SrflxProbe {
                    transaction,
                    server: *server,
                    socket: index,
                    sent_at: now,
                    attempts: 1,
                    done: false,
                });
            }
        }
    }

    fn start_relays(&mut self, now: Instant) {
        if self.relay_started {
            return;
        }
        self.relay_started = true;
        for (server, address) in &self.ice.turn {
            if self.own_ips.contains(&address.ip()) {
                self.log_own_relay(&server.host, address.ip());
                continue;
            }
            if let Ok(client) = TurnClient::start(server, *address, self.net.tls.clone(), now) {
                self.turns.push(client);
            }
        }
        if self.turns.is_empty() {
            self.push(LinkEvent::RelayUnavailable {
                credentials_rejected: false,
            });
        }
    }

    pub fn is_connected(&self) -> bool {
        self.dtls_connected
    }

    /// Records one of this client's public addresses and drops relays that sit on it: a
    /// TURN server refuses peers on its own public address, so a relay on this client's
    /// network cannot reach a peer that uses it too. Loopback relays are kept for tests.
    fn note_own_ip(&mut self, ip: IpAddr) {
        if ip.is_loopback() || self.own_ips.contains(&ip) || self.own_ips.len() >= MAX_OWN_IPS {
            return;
        }
        self.own_ips.push(ip);
        if self.dtls_connected {
            return;
        }
        let before = self.turns.len();
        let mut kept = Vec::with_capacity(before);
        for mut turn in std::mem::take(&mut self.turns) {
            if turn.server.ip() == ip {
                self.log_own_relay(&turn.server_host, ip);
                turn.close();
            } else {
                kept.push(turn);
            }
        }
        self.turns = kept;
        if before > 0 && self.turns.is_empty() {
            self.push(LinkEvent::RelayUnavailable {
                credentials_rejected: false,
            });
        }
    }

    fn log_own_relay(&self, host: &str, ip: IpAddr) {
        log(&format!(
            "relay {} skipped: it resolves to this client's own public address, and the relay refuses peers on its own address",
            self.ice
                .relay_id(Some(host), ip)
                .unwrap_or_else(|| host.chars().take(64).collect())
        ));
    }

    pub fn relay_started(&self) -> bool {
        self.relay_started
    }

    /// Allocates TURN relays now instead of after the direct timeout. A replacement link uses
    /// it so a dead direct path falls back to a relay without another wait.
    pub fn start_relays_now(&mut self, now: Instant) {
        if !self.ice.turn.is_empty() {
            self.start_relays(now);
        }
    }

    /// Time since the peer last sent anything this link accepted: media, RTCP, data channel
    /// traffic or ICE checks.
    pub fn inbound_silence(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.last_inbound)
    }

    pub fn rtt(&self) -> Option<Duration> {
        self.rtt
    }

    /// Fraction of sent RTP the remote side reported lost, from RTCP receiver reports.
    pub fn egress_loss(&self) -> Option<f32> {
        self.egress_loss
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_ref()
    }

    pub fn bytes_sent(&self) -> u64 {
        self.bytes_sent
    }

    pub fn bytes_received(&self) -> u64 {
        self.bytes_received
    }

    /// Host side: the offer with two send-only RTP streams and both data channels.
    pub fn create_offer(&mut self) -> Option<String> {
        let mut api = self.rtc.sdp_api();
        let video = api.add_media(MediaKind::Video, Direction::SendOnly, None, None, None);
        let audio = api.add_media(MediaKind::Audio, Direction::SendOnly, None, None, None);
        let control = api.add_channel(CONTROL_LABEL.to_owned());
        let input = api.add_channel_with_config(ChannelConfig {
            label: INPUT_LABEL.to_owned(),
            ordered: false,
            reliability: Reliability::MaxRetransmits { retransmits: 0 },
            ..ChannelConfig::default()
        });
        let (offer, pending) = api.apply()?;
        self.video_mid = Some(video);
        self.audio_mid = Some(audio);
        self.control = Some(control);
        self.input = Some(input);
        self.pending_offer = Some(pending);
        Some(offer.to_sdp_string())
    }

    pub fn accept_answer(&mut self, sdp: &str) -> Result<(), String> {
        let answer = SdpAnswer::from_sdp_string(sdp).map_err(|error| error.to_string())?;
        let pending = self
            .pending_offer
            .take()
            .ok_or_else(|| "no offer is pending".to_owned())?;
        self.rtc
            .sdp_api()
            .accept_answer(pending, answer)
            .map_err(|error| error.to_string())?;
        self.remote_described = true;
        self.flush_remote_candidates();
        Ok(())
    }

    /// Guest side.
    pub fn accept_offer(&mut self, sdp: &str) -> Result<String, String> {
        let offer = SdpOffer::from_sdp_string(sdp).map_err(|error| error.to_string())?;
        let answer = self
            .rtc
            .sdp_api()
            .accept_offer(offer)
            .map_err(|error| error.to_string())?;
        self.remote_described = true;
        self.flush_remote_candidates();
        Ok(answer.to_sdp_string())
    }

    pub fn add_remote_candidate(&mut self, candidate: &str) -> Result<(), String> {
        let text = candidate.trim().trim_start_matches("a=");
        let candidate = Candidate::from_sdp_string(text).map_err(|error| error.to_string())?;
        if candidate.kind() == CandidateKind::Relayed && self.remote_relays.len() < 16 {
            self.remote_relays.push(candidate.addr());
            self.selected_pair = None;
        }
        if self.remote_described {
            self.rtc.add_remote_candidate(candidate);
        } else if self.pending_remote.len() < MAX_PENDING_REMOTE_CANDIDATES {
            self.pending_remote.push(candidate);
        }
        Ok(())
    }

    fn flush_remote_candidates(&mut self) {
        for candidate in std::mem::take(&mut self.pending_remote) {
            self.rtc.add_remote_candidate(candidate);
        }
    }

    /// Local candidates already in the SDP need no trickle; later ones are announced.
    fn announce_local(&mut self, candidate: Candidate) {
        if let Some(added) = self.rtc.add_local_candidate(candidate) {
            let text = added.to_sdp_string();
            let text = text.trim().trim_start_matches("a=").to_owned();
            self.push(LinkEvent::LocalCandidate(text));
        }
    }

    pub fn write_rtp(&mut self, mid: Mid, pt: u8, packet: &RingEntry, now: Instant) -> bool {
        let mut api = self.rtc.direct_api();
        let Some(stream) = api.stream_tx_by_mid(mid, None) else {
            return false;
        };
        stream.write_rtp(
            RtpWrite::new(
                Pt::from(pt),
                SeqNo::from(packet.seq),
                packet.rtp_time,
                now,
                Arc::clone(&packet.packet),
            )
            .marker(packet.marker),
        );
        true
    }

    pub fn send_channel(&mut self, id: Option<ChannelId>, binary: bool, data: &[u8]) -> bool {
        let Some(id) = id else {
            return false;
        };
        self.rtc
            .channel(id)
            .is_some_and(|mut channel| channel.write(binary, data).unwrap_or(false))
    }

    pub fn send_app_feedback(&mut self, media_ssrc: Ssrc, payload: Vec<u8>) {
        let sender = self.rtc.direct_api().new_ssrc();
        self.rtc
            .direct_api()
            .send_app_specific_feedback(sender, media_ssrc, payload);
    }

    /// Reads every socket without blocking and feeds str0m.
    pub fn poll_network(&mut self, now: Instant) {
        let mut buffer = [0_u8; 2048];
        let mut received: Vec<(SocketAddr, SocketAddr, Vec<u8>)> = Vec::new();
        let mut reflexive: Vec<(SocketAddr, SocketAddr)> = Vec::new();
        for (index, host) in self.hosts.iter().enumerate() {
            for _ in 0..256 {
                let (length, source) = match host.socket.recv_from(&mut buffer) {
                    Ok(value) => value,
                    Err(_) => break,
                };
                let datagram = &buffer[..length];
                if stun::is_stun(datagram)
                    && let Some(message) = Message::parse(datagram)
                    && message.method == BINDING
                    && message.class == CLASS_SUCCESS
                    && let Some(probe) = self.probes.iter_mut().find(|probe| {
                        probe.socket == index && probe.transaction == message.transaction
                    })
                {
                    probe.done = true;
                    if let Some(mapped) = message.mapped_address()
                        && mapped != host.local
                    {
                        reflexive.push((mapped, host.local));
                    }
                    continue;
                }
                received.push((source, host.local, datagram.to_vec()));
            }
        }
        let mut turn_events = Vec::new();
        for turn in &mut self.turns {
            let mut events = Vec::new();
            turn.poll_read(now, &mut events);
            turn_events.push((turn.relayed(), turn.local_addr(), events));
        }
        for (mapped, base) in reflexive {
            self.note_own_ip(mapped.ip());
            if let Ok(candidate) = Candidate::server_reflexive(mapped, base, "udp") {
                self.announce_local(candidate);
            }
        }
        for (source, destination, data) in received {
            self.receive(source, destination, &data, now);
        }
        for (relayed, local, events) in turn_events {
            for event in events {
                match event {
                    TurnEvent::Allocated { relayed, mapped } => {
                        if let Some(mapped) = mapped {
                            self.note_own_ip(mapped.ip());
                        }
                        if self
                            .turns
                            .iter()
                            .any(|turn| turn.relayed() == Some(relayed))
                            && let Ok(candidate) = Candidate::relayed(relayed, local, "udp")
                        {
                            self.announce_local(candidate);
                        }
                    }
                    TurnEvent::Data { peer, data } => {
                        if let Some(relayed) = relayed {
                            self.receive(peer, relayed, &data, now);
                        }
                    }
                    TurnEvent::Failed(_) => {}
                }
            }
        }
        if self.relay_started
            && !self.turns.is_empty()
            && self.turns.iter().all(TurnClient::is_failed)
            && !self.dtls_connected
        {
            let credentials_rejected = self.turns.iter().any(TurnClient::credentials_rejected);
            self.turns.clear();
            self.push(LinkEvent::RelayUnavailable {
                credentials_rejected,
            });
        }
    }

    fn receive(&mut self, source: SocketAddr, destination: SocketAddr, data: &[u8], now: Instant) {
        let Ok(receive) = Receive::new(Protocol::Udp, source, destination, data) else {
            return;
        };
        self.bytes_received += data.len() as u64;
        let input = Input::Receive(now, receive);
        if self.rtc.accepts(&input) && self.rtc.handle_input(input).is_ok() {
            self.last_inbound = self.last_inbound.max(now);
            self.pump(now);
        }
    }

    /// Advances timers and drains str0m output.
    pub fn drive(&mut self, now: Instant) {
        if now >= self.next_timeout {
            let _ = self.rtc.handle_input(Input::Timeout(now));
        }
        for probe in &mut self.probes {
            if !probe.done
                && probe.attempts < SRFLX_ATTEMPTS
                && now.saturating_duration_since(probe.sent_at) >= SRFLX_RETRY
            {
                probe.attempts += 1;
                probe.sent_at = now;
                let request = MessageBuilder::new(BINDING, CLASS_REQUEST, &probe.transaction)
                    .fingerprint()
                    .build();
                if let Some(host) = self.hosts.get(probe.socket) {
                    let _ = host.socket.send_to(&request, probe.server);
                }
            }
        }
        let mut turn_events = Vec::new();
        for turn in &mut self.turns {
            turn.tick(now, &mut turn_events);
        }
        if !self.relay_started
            && !self.ice_connected
            && now.saturating_duration_since(self.started_at) >= self.net.direct_timeout
        {
            self.push(LinkEvent::DirectBlocked);
            self.start_relays(now);
        }
        self.pump(now);
    }

    /// Drains str0m until it waits for time or input. In RTP mode str0m holds one received
    /// packet at a time, so this runs after every datagram.
    fn pump(&mut self, now: Instant) {
        loop {
            match self.rtc.poll_output() {
                Ok(Output::Timeout(at)) => {
                    self.next_timeout = at;
                    break;
                }
                Ok(Output::Transmit(transmit)) => {
                    let contents: &[u8] = &transmit.contents;
                    self.bytes_sent += contents.len() as u64;
                    if let Some(host) = self.hosts.iter().find(|host| host.local == transmit.source)
                    {
                        let _ = host.socket.send_to(contents, transmit.destination);
                    } else if let Some(turn) = self
                        .turns
                        .iter_mut()
                        .find(|turn| turn.relayed() == Some(transmit.source))
                    {
                        turn.send_to(transmit.destination, contents, now);
                    }
                }
                Ok(Output::Event(event)) => self.handle_event(event),
                Err(_) => {
                    self.push(LinkEvent::Closed);
                    break;
                }
            }
        }
        self.update_path();
    }

    /// Labels the path from the nominated pair: relayed when either end is a TURN relay
    /// address, so a peer-reflexive remote is relabelled once its relay candidate arrives.
    fn update_path(&mut self) {
        let selected = self.rtc.selected_pair();
        if selected.is_none() || selected == self.selected_pair {
            return;
        }
        self.selected_pair = selected;
        let Some((local, remote)) = selected else {
            return;
        };
        let path = if let Some(turn) = self.turns.iter().find(|turn| turn.relayed() == Some(local))
        {
            Path::Relay(self.ice.relay_id(Some(&turn.server_host), turn.server.ip()))
        } else if self.remote_relays.contains(&remote) {
            Path::Relay(self.ice.relay_id(None, remote.ip()))
        } else {
            Path::Direct
        };
        if self.path.as_ref() != Some(&path) {
            self.path = Some(path.clone());
            self.push(LinkEvent::Path(path));
        }
    }

    pub fn next_timeout(&self) -> Instant {
        self.next_timeout
    }

    fn handle_event(&mut self, event: Event) {
        match event {
            Event::IceConnectionStateChange(state) => {
                if matches!(
                    state,
                    IceConnectionState::Connected | IceConnectionState::Completed
                ) {
                    self.ice_connected = true;
                }
                self.push(LinkEvent::Ice(state));
            }
            Event::Connected => {
                self.dtls_connected = true;
                self.push(LinkEvent::Connected);
            }
            Event::ChannelOpen(id, label) => {
                match label.as_str() {
                    CONTROL_LABEL => self.control = Some(id),
                    INPUT_LABEL => self.input = Some(id),
                    _ => {}
                }
                self.push(LinkEvent::ChannelOpen(id, label));
            }
            Event::ChannelData(data) => self.push(LinkEvent::ChannelData(data.id, data.data)),
            Event::ChannelClose(id) => self.push(LinkEvent::ChannelClosed(id)),
            Event::RtpPacket(packet) => self.push(LinkEvent::Rtp(Box::new(packet))),
            Event::AppSpecificFeedback(feedback) => self.push(LinkEvent::AppFeedback {
                media_ssrc: feedback.media_ssrc,
                payload: feedback.payload,
            }),
            Event::KeyframeRequest(_) => self.push(LinkEvent::KeyframeRequest),
            Event::MediaAdded(added) => match added.kind {
                MediaKind::Video => self.video_mid = Some(added.mid),
                MediaKind::Audio => self.audio_mid = Some(added.mid),
            },
            Event::PeerStats(stats) => {
                self.egress_loss = stats.egress_loss_fraction.or(self.egress_loss);
                if let Some(pair) = stats.selected_candidate_pair {
                    self.rtt = pair.current_round_trip_time.or(self.rtt);
                }
            }
            Event::Closed => self.push(LinkEvent::Closed),
            _ => {}
        }
    }

    pub fn take_events(&mut self) -> impl Iterator<Item = LinkEvent> + '_ {
        self.events.drain(..)
    }

    pub fn close(&mut self) {
        self.rtc.disconnect();
        self.abandon();
    }

    /// Releases the relays of a link being replaced without closing DTLS: a close alert that
    /// still reaches the peer would end its session instead of letting it switch links.
    pub fn abandon(&mut self) {
        for turn in &mut self.turns {
            turn.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TurnTransport;

    fn relay_link(allow_direct: bool) -> PeerLink {
        let server = TurnServer {
            host: "relay.example.test".to_owned(),
            port: 3478,
            transport: TurnTransport::Udp,
            username: "user".to_owned(),
            credential: "secret".to_owned(),
        };
        let ice = ResolvedIce {
            turn: vec![(server, "203.0.113.7:3478".parse().unwrap())],
            ..ResolvedIce::default()
        };
        let net = NetConfig {
            host_ips: Some(vec![IpAddr::from([127, 0, 0, 1])]),
            allow_direct,
            ..NetConfig::default()
        };
        PeerLink::new(Arc::new(ice), net, Instant::now()).unwrap()
    }

    fn relay_unavailable(link: &mut PeerLink) -> bool {
        link.take_events()
            .any(|event| matches!(event, LinkEvent::RelayUnavailable { .. }))
    }

    #[test]
    fn a_relay_on_this_clients_public_address_is_never_started() {
        let mut link = relay_link(true);
        link.note_own_ip("203.0.113.7".parse().unwrap());
        link.start_relays(Instant::now());
        assert!(link.turns.is_empty());
        assert!(relay_unavailable(&mut link));
    }

    #[test]
    fn a_relay_found_on_this_clients_public_address_later_is_dropped() {
        let mut link = relay_link(false);
        assert_eq!(link.turns.len(), 1);
        link.note_own_ip("198.51.100.9".parse().unwrap());
        assert_eq!(
            link.turns.len(),
            1,
            "another public address keeps the relay"
        );
        link.note_own_ip("203.0.113.7".parse().unwrap());
        assert!(link.turns.is_empty());
        assert!(relay_unavailable(&mut link));
    }

    #[test]
    fn loopback_relays_are_kept() {
        let mut link = relay_link(false);
        link.note_own_ip(IpAddr::from([127, 0, 0, 1]));
        assert_eq!(link.turns.len(), 1);
    }
}
