//! Minimal RFC 8656 TURN client: one UDP relayed allocation reached over UDP, TCP or TLS,
//! with permissions, channel bindings and refreshes. str0m does not allocate relays, so this
//! client produces the relayed candidate and carries str0m's datagrams through the server.

use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream, UdpSocket};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};
use std::time::{Duration, Instant};

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection};

use crate::config::{TurnServer, TurnTransport};
use crate::stun::{
    self, ALLOCATE, ATTR_CHANNEL_NUMBER, ATTR_DATA, ATTR_LIFETIME, ATTR_NONCE, ATTR_REALM,
    ATTR_REQUESTED_TRANSPORT, ATTR_USERNAME, ATTR_XOR_PEER_ADDRESS, ATTR_XOR_RELAYED_ADDRESS,
    CHANNEL_BIND, CLASS_ERROR, CLASS_INDICATION, CLASS_REQUEST, CLASS_SUCCESS, CREATE_PERMISSION,
    DATA, Message, MessageBuilder, REFRESH, SEND, TransactionId,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const UDP_RTO: Duration = Duration::from_millis(250);
const MAX_UDP_ATTEMPTS: u32 = 6;
const STREAM_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const PERMISSION_REFRESH: Duration = Duration::from_secs(240);
const CHANNEL_REFRESH: Duration = Duration::from_secs(480);
const MAX_PENDING_REQUESTS: usize = 16;
const MAX_PEERS: usize = 8;
const MAX_QUEUED_SENDS: usize = 32;
const MAX_STREAM_BUFFER: usize = 128 * 1024;
const MAX_READS_PER_POLL: usize = 256;
const FIRST_CHANNEL: u16 = 0x4000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnEvent {
    /// The relayed address, and this client's address as the server saw it.
    Allocated {
        relayed: SocketAddr,
        mapped: Option<SocketAddr>,
    },
    Data {
        peer: SocketAddr,
        data: Vec<u8>,
    },
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequestKind {
    Allocate,
    Refresh { lifetime: u32 },
    Permission(IpAddr),
    ChannelBind(SocketAddr, u16),
}

struct Pending {
    transaction: TransactionId,
    kind: RequestKind,
    bytes: Vec<u8>,
    sent_at: Instant,
    attempts: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PeerBinding {
    None,
    Requested,
    Installed(Instant),
}

struct Peer {
    ip: IpAddr,
    permission: PeerBinding,
    channels: Vec<(SocketAddr, u16, PeerBinding)>,
}

enum Link {
    Udp(UdpSocket),
    Connecting {
        result: Receiver<io::Result<TcpStream>>,
        tls: Option<Arc<ClientConfig>>,
        host: String,
    },
    Tcp(TcpStream),
    Tls(Box<rustls::StreamOwned<ClientConnection, TcpStream>>),
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Allocating,
    Allocated {
        relayed: SocketAddr,
        refresh_at: Instant,
    },
    Failed,
}

pub struct TurnClient {
    pub server: SocketAddr,
    pub server_host: String,
    pub transport: TurnTransport,
    link: Link,
    username: String,
    password: String,
    realm: Option<String>,
    nonce: Option<String>,
    key: Option<[u8; 16]>,
    state: State,
    pending: Vec<Pending>,
    peers: Vec<Peer>,
    queued: Vec<(SocketAddr, Vec<u8>)>,
    next_channel: u16,
    read_buffer: Vec<u8>,
    write_backlog: Vec<u8>,
    local_addr: SocketAddr,
    bytes_sent: u64,
    credentials_rejected: bool,
}

impl TurnClient {
    /// Starts an allocation toward 'server'. Stream transports connect on a helper thread
    /// so the media worker never blocks on TCP or TLS setup.
    pub fn start(
        server: &TurnServer,
        address: SocketAddr,
        tls: Option<Arc<ClientConfig>>,
        now: Instant,
    ) -> io::Result<Self> {
        let (link, local_addr) = match server.transport {
            TurnTransport::Udp => {
                let bind: SocketAddr = match address {
                    SocketAddr::V4(_) => (Ipv4Addr::UNSPECIFIED, 0).into(),
                    SocketAddr::V6(_) => (Ipv6Addr::UNSPECIFIED, 0).into(),
                };
                let socket = UdpSocket::bind(bind)?;
                socket.connect(address)?;
                socket.set_nonblocking(true)?;
                let local = socket.local_addr()?;
                (Link::Udp(socket), local)
            }
            TurnTransport::Tcp | TurnTransport::Tls => {
                let (sender, result) = sync_channel(1);
                std::thread::Builder::new()
                    .name("opennow-coop-turn-connect".to_owned())
                    .spawn(move || {
                        let stream = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT)
                            .and_then(|stream| {
                                stream.set_nodelay(true)?;
                                stream.set_nonblocking(true)?;
                                Ok(stream)
                            });
                        let _ = sender.send(stream);
                    })?;
                let tls = (server.transport == TurnTransport::Tls)
                    .then(|| tls.unwrap_or_else(default_tls_config));
                (
                    Link::Connecting {
                        result,
                        tls,
                        host: server.host.clone(),
                    },
                    address,
                )
            }
        };
        let mut client = Self {
            server: address,
            server_host: server.host.clone(),
            transport: server.transport,
            link,
            username: server.username.clone(),
            password: server.credential.clone(),
            realm: None,
            nonce: None,
            key: None,
            state: State::Allocating,
            pending: Vec::new(),
            peers: Vec::new(),
            queued: Vec::new(),
            next_channel: FIRST_CHANNEL,
            read_buffer: Vec::new(),
            write_backlog: Vec::new(),
            local_addr,
            bytes_sent: 0,
            credentials_rejected: false,
        };
        client.request(RequestKind::Allocate, now);
        Ok(client)
    }

    pub fn relayed(&self) -> Option<SocketAddr> {
        match self.state {
            State::Allocated { relayed, .. } => Some(relayed),
            _ => None,
        }
    }

    pub fn is_failed(&self) -> bool {
        self.state == State::Failed
    }

    /// The local address used to reach the TURN server (str0m's relayed candidate 'local').
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    pub fn bytes_sent(&self) -> u64 {
        self.bytes_sent
    }

    /// The server refused the allocation credentials (401 after authenticating, or 437,
    /// which coturn returns for a wrong realm). The caller needs fresh ICE servers.
    pub fn credentials_rejected(&self) -> bool {
        self.credentials_rejected
    }

    fn fail(&mut self, reason: String, events: &mut Vec<TurnEvent>) {
        if self.state != State::Failed {
            self.state = State::Failed;
            self.pending.clear();
            self.queued.clear();
            events.push(TurnEvent::Failed(reason));
        }
    }

    fn build_request(&self, kind: RequestKind, transaction: &TransactionId) -> Vec<u8> {
        let method = match kind {
            RequestKind::Allocate => ALLOCATE,
            RequestKind::Refresh { .. } => REFRESH,
            RequestKind::Permission(_) => CREATE_PERMISSION,
            RequestKind::ChannelBind(..) => CHANNEL_BIND,
        };
        let mut builder = MessageBuilder::new(method, CLASS_REQUEST, transaction);
        match kind {
            RequestKind::Allocate => {
                builder = builder.attribute(ATTR_REQUESTED_TRANSPORT, &[17, 0, 0, 0]);
            }
            RequestKind::Refresh { lifetime } => {
                builder = builder.attribute(ATTR_LIFETIME, &lifetime.to_be_bytes());
            }
            RequestKind::Permission(ip) => {
                builder = builder.xor_address(ATTR_XOR_PEER_ADDRESS, SocketAddr::new(ip, 0));
            }
            RequestKind::ChannelBind(peer, channel) => {
                let mut number = channel.to_be_bytes().to_vec();
                number.extend_from_slice(&[0, 0]);
                builder = builder
                    .attribute(ATTR_CHANNEL_NUMBER, &number)
                    .xor_address(ATTR_XOR_PEER_ADDRESS, peer);
            }
        }
        if let (Some(realm), Some(nonce), Some(key)) = (&self.realm, &self.nonce, &self.key) {
            builder = builder
                .attribute(ATTR_USERNAME, self.username.as_bytes())
                .attribute(ATTR_REALM, realm.as_bytes())
                .attribute(ATTR_NONCE, nonce.as_bytes())
                .integrity(key);
        }
        builder.build()
    }

    fn request(&mut self, kind: RequestKind, now: Instant) {
        if self.pending.len() == MAX_PENDING_REQUESTS {
            return;
        }
        let transaction = stun::new_transaction_id();
        let bytes = self.build_request(kind, &transaction);
        self.write(&bytes);
        self.pending.push(Pending {
            transaction,
            kind,
            bytes,
            sent_at: now,
            attempts: 1,
        });
    }

    fn write(&mut self, bytes: &[u8]) {
        self.bytes_sent += bytes.len() as u64;
        match &mut self.link {
            Link::Udp(socket) => {
                let _ = socket.send(bytes);
            }
            Link::Connecting { .. } | Link::Tcp(_) | Link::Tls(_) => {
                admit_frame(&mut self.write_backlog, bytes);
                self.flush_stream();
            }
            Link::Closed => {}
        }
    }

    fn flush_stream(&mut self) {
        let result = match &mut self.link {
            Link::Tcp(stream) => write_some(stream, &mut self.write_backlog),
            Link::Tls(stream) => {
                let stream = &mut **stream;
                flush_tls(&mut stream.conn, &mut stream.sock, &mut self.write_backlog)
            }
            _ => Ok(()),
        };
        if result.is_err() {
            self.link = Link::Closed;
        }
    }

    fn peer_mut(&mut self, ip: IpAddr) -> Option<&mut Peer> {
        if !self.peers.iter().any(|peer| peer.ip == ip) {
            if self.peers.len() == MAX_PEERS {
                return None;
            }
            self.peers.push(Peer {
                ip,
                permission: PeerBinding::None,
                channels: Vec::new(),
            });
        }
        self.peers.iter_mut().find(|peer| peer.ip == ip)
    }

    /// Sends one datagram to 'peer' through the relay.
    pub fn send_to(&mut self, peer: SocketAddr, data: &[u8], now: Instant) {
        if self.relayed().is_none() {
            return;
        }
        let Some(entry) = self.peer_mut(peer.ip()) else {
            return;
        };
        let permission = entry.permission;
        let channel = entry
            .channels
            .iter()
            .find(|(addr, _, _)| *addr == peer)
            .map(|(_, number, binding)| (*number, *binding));
        match permission {
            PeerBinding::None => {
                if let Some(entry) = self.peer_mut(peer.ip()) {
                    entry.permission = PeerBinding::Requested;
                }
                self.request(RequestKind::Permission(peer.ip()), now);
                self.queue(peer, data);
                return;
            }
            PeerBinding::Requested => {
                self.queue(peer, data);
                return;
            }
            PeerBinding::Installed(_) => {}
        }
        match channel {
            Some((number, PeerBinding::Installed(_))) => {
                let mut frame = Vec::with_capacity(4 + data.len());
                frame.extend_from_slice(&number.to_be_bytes());
                frame.extend_from_slice(&(data.len() as u16).to_be_bytes());
                frame.extend_from_slice(data);
                self.write(&frame);
            }
            Some(_) => self.send_indication(peer, data),
            None => {
                self.send_indication(peer, data);
                if self.next_channel <= 0x4fff {
                    let number = self.next_channel;
                    self.next_channel += 1;
                    if let Some(entry) = self.peer_mut(peer.ip())
                        && entry.channels.len() < MAX_PEERS
                    {
                        entry.channels.push((peer, number, PeerBinding::Requested));
                        self.request(RequestKind::ChannelBind(peer, number), now);
                    }
                }
            }
        }
    }

    fn queue(&mut self, peer: SocketAddr, data: &[u8]) {
        if self.queued.len() < MAX_QUEUED_SENDS {
            self.queued.push((peer, data.to_vec()));
        }
    }

    fn send_indication(&mut self, peer: SocketAddr, data: &[u8]) {
        let bytes = MessageBuilder::new(SEND, CLASS_INDICATION, &stun::new_transaction_id())
            .xor_address(ATTR_XOR_PEER_ADDRESS, peer)
            .attribute(ATTR_DATA, data)
            .build();
        self.write(&bytes);
    }

    /// Reads everything available and appends resulting events.
    pub fn poll_read(&mut self, now: Instant, events: &mut Vec<TurnEvent>) {
        if let Link::Connecting { result, tls, host } = &mut self.link {
            match result.try_recv() {
                Ok(Ok(stream)) => {
                    let tls = tls.take();
                    let host = host.clone();
                    self.link = match tls {
                        None => Link::Tcp(stream),
                        Some(config) => {
                            let name = match host.parse::<IpAddr>() {
                                Ok(ip) => ServerName::IpAddress(ip.into()),
                                Err(_) => match ServerName::try_from(host) {
                                    Ok(name) => name,
                                    Err(_) => {
                                        self.fail("invalid TURNS host name".to_owned(), events);
                                        return;
                                    }
                                },
                            };
                            match ClientConnection::new(config, name) {
                                Ok(mut connection) => {
                                    connection.set_buffer_limit(Some(MAX_STREAM_BUFFER));
                                    Link::Tls(Box::new(rustls::StreamOwned::new(
                                        connection, stream,
                                    )))
                                }
                                Err(error) => {
                                    self.fail(format!("TLS setup failed: {error}"), events);
                                    return;
                                }
                            }
                        }
                    };
                    self.flush_stream();
                }
                Ok(Err(error)) => {
                    self.fail(format!("TURN connect failed: {}", error.kind()), events);
                    return;
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.fail("TURN connect thread ended".to_owned(), events);
                    return;
                }
            }
        }
        let mut datagrams: Vec<Vec<u8>> = Vec::new();
        let mut closed = false;
        match &mut self.link {
            Link::Udp(socket) => {
                let mut buffer = [0_u8; 2048];
                for _ in 0..MAX_READS_PER_POLL {
                    match socket.recv(&mut buffer) {
                        Ok(length) => datagrams.push(buffer[..length].to_vec()),
                        Err(_) => break,
                    }
                }
            }
            Link::Tcp(stream) => closed = read_stream(stream, &mut self.read_buffer),
            Link::Tls(stream) => {
                let mut chunk = [0_u8; 16 * 1024];
                loop {
                    match stream.read(&mut chunk) {
                        Ok(0) => {
                            closed = true;
                            break;
                        }
                        Ok(length) => {
                            if self.read_buffer.len() + length > MAX_STREAM_BUFFER {
                                closed = true;
                                break;
                            }
                            self.read_buffer.extend_from_slice(&chunk[..length]);
                        }
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                        Err(_) => {
                            closed = true;
                            break;
                        }
                    }
                }
            }
            Link::Connecting { .. } | Link::Closed => {}
        }
        if matches!(self.link, Link::Tls(_)) {
            self.flush_stream();
        }
        if !matches!(self.link, Link::Udp(_)) {
            datagrams.extend(take_stream_frames(&mut self.read_buffer));
        }
        for datagram in datagrams {
            self.handle(&datagram, now, events);
        }
        if closed {
            self.link = Link::Closed;
            self.fail("TURN connection closed".to_owned(), events);
        }
    }

    fn handle(&mut self, bytes: &[u8], now: Instant, events: &mut Vec<TurnEvent>) {
        if stun::is_channel_data(bytes) {
            let number = u16::from_be_bytes([bytes[0], bytes[1]]);
            let length = usize::from(u16::from_be_bytes([bytes[2], bytes[3]]));
            let Some(data) = bytes.get(4..4 + length) else {
                return;
            };
            let peer = self.peers.iter().find_map(|peer| {
                peer.channels
                    .iter()
                    .find(|(_, channel, _)| *channel == number)
                    .map(|(addr, _, _)| *addr)
            });
            if let Some(peer) = peer {
                events.push(TurnEvent::Data {
                    peer,
                    data: data.to_vec(),
                });
            }
            return;
        }
        let Some(message) = Message::parse(bytes) else {
            return;
        };
        if message.method == DATA && message.class == CLASS_INDICATION {
            if let (Some(peer), Some(data)) = (
                message.xor_address(ATTR_XOR_PEER_ADDRESS),
                message.attribute(ATTR_DATA),
            ) {
                events.push(TurnEvent::Data {
                    peer,
                    data: data.to_vec(),
                });
            }
            return;
        }
        let Some(index) = self
            .pending
            .iter()
            .position(|pending| pending.transaction == message.transaction)
        else {
            return;
        };
        if let Some(key) = self.key
            && message.class == CLASS_SUCCESS
            && message.attribute(stun::ATTR_MESSAGE_INTEGRITY).is_some()
            && !message.verify_integrity(&key)
        {
            return;
        }
        let pending = self.pending.remove(index);
        if message.class == CLASS_ERROR {
            let code = message.error_code().unwrap_or(0);
            let can_retry_auth = (code == 401 && self.key.is_none()) || code == 438;
            if can_retry_auth
                && let (Some(realm), Some(nonce)) = (
                    message.text(ATTR_REALM).or(self.realm.clone()),
                    message.text(ATTR_NONCE),
                )
            {
                self.key = Some(stun::long_term_key(&self.username, &realm, &self.password));
                self.realm = Some(realm);
                self.nonce = Some(nonce);
                self.request(pending.kind, now);
                return;
            }
            match pending.kind {
                RequestKind::Allocate => {
                    self.credentials_rejected = matches!(code, 401 | 437);
                    self.fail(format!("TURN allocation rejected with {code}"), events)
                }
                RequestKind::Permission(ip) => {
                    self.peers.retain(|peer| peer.ip != ip);
                    self.queued.retain(|(peer, _)| peer.ip() != ip);
                }
                RequestKind::ChannelBind(peer, _) => {
                    for entry in &mut self.peers {
                        entry.channels.retain(|(addr, _, _)| *addr != peer);
                    }
                }
                RequestKind::Refresh { .. } => {}
            }
            return;
        }
        if message.class != CLASS_SUCCESS {
            return;
        }
        match pending.kind {
            RequestKind::Allocate => {
                let Some(relayed) = message.xor_address(ATTR_XOR_RELAYED_ADDRESS) else {
                    self.fail("TURN allocation has no relayed address".to_owned(), events);
                    return;
                };
                let lifetime = message.u32_attribute(ATTR_LIFETIME).unwrap_or(600);
                self.state = State::Allocated {
                    relayed,
                    refresh_at: now + refresh_delay(lifetime),
                };
                events.push(TurnEvent::Allocated {
                    relayed,
                    mapped: message.mapped_address(),
                });
            }
            RequestKind::Refresh { lifetime } => {
                if let State::Allocated { relayed, .. } = self.state {
                    let granted = message.u32_attribute(ATTR_LIFETIME).unwrap_or(lifetime);
                    self.state = State::Allocated {
                        relayed,
                        refresh_at: now + refresh_delay(granted),
                    };
                }
            }
            RequestKind::Permission(ip) => {
                if let Some(peer) = self.peers.iter_mut().find(|peer| peer.ip == ip) {
                    peer.permission = PeerBinding::Installed(now);
                }
                let (ready, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.queued)
                    .into_iter()
                    .partition(|(peer, _)| peer.ip() == ip);
                self.queued = waiting;
                for (peer, data) in ready {
                    self.send_to(peer, &data, now);
                }
            }
            RequestKind::ChannelBind(addr, number) => {
                for peer in &mut self.peers {
                    for channel in &mut peer.channels {
                        if channel.0 == addr && channel.1 == number {
                            channel.2 = PeerBinding::Installed(now);
                        }
                    }
                }
            }
        }
    }

    /// Retransmits UDP requests and keeps the allocation, permissions and channels alive.
    pub fn tick(&mut self, now: Instant, events: &mut Vec<TurnEvent>) {
        if self.state == State::Failed {
            return;
        }
        let stream = !matches!(self.link, Link::Udp(_));
        let mut resend = Vec::new();
        let mut expired = None;
        for pending in &mut self.pending {
            let age = now.saturating_duration_since(pending.sent_at);
            if stream {
                if age >= STREAM_REQUEST_TIMEOUT {
                    expired = Some(pending.kind);
                }
                continue;
            }
            let rto = UDP_RTO * 2_u32.pow(pending.attempts.saturating_sub(1).min(4));
            if age >= rto {
                if pending.attempts >= MAX_UDP_ATTEMPTS {
                    expired = Some(pending.kind);
                } else {
                    pending.attempts += 1;
                    pending.sent_at = now;
                    resend.push(pending.bytes.clone());
                }
            }
        }
        for bytes in resend {
            self.write(&bytes);
        }
        if let Some(kind) = expired {
            if kind == RequestKind::Allocate {
                self.fail("TURN allocation timed out".to_owned(), events);
                return;
            }
            self.pending.retain(|pending| pending.kind != kind);
        }
        if stream {
            self.flush_stream();
        }
        let State::Allocated { refresh_at, .. } = self.state else {
            return;
        };
        let pending_kinds: Vec<RequestKind> =
            self.pending.iter().map(|pending| pending.kind).collect();
        if now >= refresh_at
            && !pending_kinds
                .iter()
                .any(|kind| matches!(kind, RequestKind::Refresh { .. }))
        {
            self.request(RequestKind::Refresh { lifetime: 600 }, now);
        }
        let mut renewals = Vec::new();
        for peer in &self.peers {
            if let PeerBinding::Installed(at) = peer.permission
                && now.saturating_duration_since(at) >= PERMISSION_REFRESH
                && !pending_kinds.contains(&RequestKind::Permission(peer.ip))
            {
                renewals.push(RequestKind::Permission(peer.ip));
            }
            for (addr, number, binding) in &peer.channels {
                if let PeerBinding::Installed(at) = binding
                    && now.saturating_duration_since(*at) >= CHANNEL_REFRESH
                    && !pending_kinds.contains(&RequestKind::ChannelBind(*addr, *number))
                {
                    renewals.push(RequestKind::ChannelBind(*addr, *number));
                }
            }
        }
        for kind in renewals {
            self.request(kind, now);
        }
    }

    /// Releases the allocation; best effort, no response awaited.
    pub fn close(&mut self) {
        if self.relayed().is_some() {
            let transaction = stun::new_transaction_id();
            let bytes = self.build_request(RequestKind::Refresh { lifetime: 0 }, &transaction);
            self.write(&bytes);
        }
        self.state = State::Failed;
        self.link = Link::Closed;
    }
}

fn refresh_delay(lifetime: u32) -> Duration {
    Duration::from_secs(u64::from(lifetime.saturating_sub(60).max(30)))
}

/// Queues one whole STUN message or ChannelData frame, padded to 4 bytes as streams require.
/// A frame that does not fit the bound is dropped whole, so the stream framing never breaks.
fn admit_frame(backlog: &mut Vec<u8>, frame: &[u8]) -> bool {
    let padding = if stun::is_channel_data(frame) {
        (4 - frame.len() % 4) % 4
    } else {
        0
    };
    if backlog.len() + frame.len() + padding > MAX_STREAM_BUFFER {
        return false;
    }
    backlog.extend_from_slice(frame);
    backlog.resize(backlog.len() + padding, 0);
    true
}

/// Moves as much of 'backlog' into rustls as it accepts and writes TLS records to 'socket'.
/// Bytes rustls refuses stay at the front of 'backlog' for the next flush.
fn flush_tls(
    connection: &mut ClientConnection,
    socket: &mut impl Write,
    backlog: &mut Vec<u8>,
) -> io::Result<()> {
    loop {
        while !backlog.is_empty() {
            let accepted = connection.writer().write(backlog)?;
            if accepted == 0 {
                break;
            }
            backlog.drain(..accepted);
        }
        let mut wrote = false;
        while connection.wants_write() {
            match connection.write_tls(socket) {
                Ok(0) => break,
                Ok(_) => wrote = true,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
                Err(error) => return Err(error),
            }
        }
        if backlog.is_empty() || !wrote {
            return Ok(());
        }
    }
}

fn write_some(stream: &mut TcpStream, backlog: &mut Vec<u8>) -> io::Result<()> {
    while !backlog.is_empty() {
        match stream.write(backlog) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(written) => {
                backlog.drain(..written);
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn read_stream(stream: &mut TcpStream, buffer: &mut Vec<u8>) -> bool {
    let mut chunk = [0_u8; 16 * 1024];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => return true,
            Ok(length) => {
                if buffer.len() + length > MAX_STREAM_BUFFER {
                    return true;
                }
                buffer.extend_from_slice(&chunk[..length]);
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return false,
            Err(_) => return true,
        }
    }
}

/// Splits STUN messages and padded ChannelData frames out of a TCP/TLS byte stream.
fn take_stream_frames(buffer: &mut Vec<u8>) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    let mut cursor = 0;
    while buffer.len() - cursor >= 4 {
        let head = &buffer[cursor..];
        let length = if stun::is_channel_data(head) {
            let data = usize::from(u16::from_be_bytes([head[2], head[3]]));
            (4 + data).div_ceil(4) * 4
        } else {
            stun::HEADER_LEN + usize::from(u16::from_be_bytes([head[2], head[3]]))
        };
        if buffer.len() - cursor < length {
            break;
        }
        frames.push(buffer[cursor..cursor + length].to_vec());
        cursor += length;
    }
    buffer.drain(..cursor);
    frames
}

/// Platform trust store, used for production TURNS.
pub fn default_tls_config() -> Arc<ClientConfig> {
    let mut roots = rustls::RootCertStore::empty();
    for certificate in rustls_native_certs::load_native_certs().certs {
        let _ = roots.add(certificate);
    }
    tls_config_with_roots(roots)
}

pub fn tls_config_with_roots(roots: rustls::RootCertStore) -> Arc<ClientConfig> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("ring supports the default TLS versions")
        .with_root_certificates(roots)
        .with_no_client_auth();
    Arc::new(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_framing_splits_stun_and_padded_channel_data() {
        let stun = MessageBuilder::new(ALLOCATE, CLASS_SUCCESS, &[1; 12])
            .attribute(ATTR_LIFETIME, &600_u32.to_be_bytes())
            .build();
        let mut buffer = stun.clone();
        buffer.extend_from_slice(&[0x40, 0x00, 0x00, 0x03, 9, 8, 7, 0]);
        buffer.extend_from_slice(&[0x40, 0x01, 0x00]);
        let frames = take_stream_frames(&mut buffer);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0], stun);
        assert_eq!(frames[1], [0x40, 0x00, 0x00, 0x03, 9, 8, 7, 0]);
        assert_eq!(buffer, [0x40, 0x01, 0x00]);
    }

    /// Accepts at most three bytes per write and refuses every other call.
    #[derive(Default)]
    struct TrickleSocket {
        written: Vec<u8>,
        calls: usize,
    }

    impl Write for TrickleSocket {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.calls += 1;
            if self.calls % 2 == 0 {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            let accepted = bytes.len().min(3);
            self.written.extend_from_slice(&bytes[..accepted]);
            Ok(accepted)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    macro_rules! feed {
        ($source:expr, $connection:expr) => {{
            let mut bytes = &$source[..];
            while !bytes.is_empty() {
                $connection.read_tls(&mut bytes).unwrap();
                $connection.process_new_packets().unwrap();
            }
            $source.clear();
        }};
    }

    #[test]
    fn tls_flush_keeps_refused_bytes_and_whole_frames() {
        let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).unwrap();
        let mut roots = rustls::RootCertStore::empty();
        roots.add(certified.cert.der().clone()).unwrap();
        let mut client = ClientConnection::new(
            tls_config_with_roots(roots),
            ServerName::try_from("localhost").unwrap(),
        )
        .unwrap();
        client.set_buffer_limit(Some(4 * 1024));
        let key =
            rustls::pki_types::PrivateKeyDer::Pkcs8(certified.signing_key.serialize_der().into());
        let server_config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![certified.cert.der().clone()], key)
        .unwrap();
        let mut server = rustls::ServerConnection::new(Arc::new(server_config)).unwrap();

        let mut backlog = Vec::new();
        let mut expected = Vec::new();
        let mut admitted = 0;
        for index in 0..200_u16 {
            let mut frame = vec![0x40, 0x00];
            frame.extend_from_slice(&1_001_u16.to_be_bytes());
            frame.extend(std::iter::repeat_n(index as u8, 1_001));
            let before = backlog.len();
            if admit_frame(&mut backlog, &frame) {
                admitted += 1;
                expected.extend_from_slice(&backlog[before..]);
            } else {
                assert_eq!(
                    backlog.len(),
                    before,
                    "a refused frame leaves no partial bytes"
                );
            }
        }
        assert!(admitted < 200 && backlog.len() <= MAX_STREAM_BUFFER);
        assert_eq!(expected.len() % 4, 0);

        let mut socket = TrickleSocket::default();
        let mut received = Vec::new();
        for _ in 0..2_000_000 {
            flush_tls(&mut client, &mut socket, &mut backlog).unwrap();
            feed!(socket.written, server);
            let mut to_client = Vec::new();
            while server.wants_write() {
                server.write_tls(&mut to_client).unwrap();
            }
            feed!(to_client, client);
            let mut chunk = [0_u8; 16 * 1024];
            loop {
                match server.reader().read(&mut chunk) {
                    Ok(0) => break,
                    Ok(length) => received.extend_from_slice(&chunk[..length]),
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(error) => panic!("{error}"),
                }
            }
            if received.len() >= expected.len() && backlog.is_empty() {
                break;
            }
        }
        assert_eq!(received.len(), expected.len());
        assert!(
            received == expected,
            "the TLS stream carries every admitted frame intact"
        );
        let frames = take_stream_frames(&mut received);
        assert_eq!(frames.len(), admitted);
        for (index, frame) in frames.iter().enumerate() {
            assert!(frame[4..4 + 1_001].iter().all(|byte| *byte == index as u8));
        }
    }
}
