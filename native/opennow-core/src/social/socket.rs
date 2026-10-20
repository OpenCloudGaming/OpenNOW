use super::{Config, Connection, Shared, Timing, Token};
use base64::Engine as _;
use opennow_socket_wait::TimedTcpStream;
use serde_json::{Value, json};
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tungstenite::protocol::WebSocketConfig;
use tungstenite::protocol::frame::CloseFrame;
use tungstenite::{Message, WebSocket};
use url::Url;

const MAXIMUM_FRAME_BYTES: usize = 64 * 1024;
const MAXIMUM_PROXY_RESPONSE_BYTES: usize = 8 * 1024;

/// Backend close codes with a meaning beyond "reconnect later".
const CLOSE_BAD_AUTH: u16 = 4003;
const CLOSE_SIGNED_OUT: u16 = 4010;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Ended {
    Shutdown,
    Superseded,
    /// `ready` is whether the backend accepted the auth frame; `close` is the
    /// backend's close code when it closed the socket itself.
    Failed {
        ready: bool,
        close: Option<u16>,
    },
}

impl Ended {
    fn failed(ready: bool) -> Self {
        Self::Failed { ready, close: None }
    }
}

/// Owns the backend socket for as long as the service lives: waits for a
/// token, connects, and reconnects with jittered exponential backoff. Close
/// code 4010 (signed out or account deleted) discards the token instead of
/// reconnecting; 4003 (bad auth) drops a guest token and rechecks an account
/// token with `GET /v1/me`, which unlinks it on 401.
pub(super) fn run(shared: &Shared) {
    let mut failures = 0_u32;
    loop {
        let (generation, config, token, account) = {
            let mut state = shared.lock();
            let (token, account) = loop {
                if state.shutdown {
                    return;
                }
                if let Some(found) = state.socket_token() {
                    break found;
                }
                shared.set_connection(&mut state, Connection::Unlinked);
                state = shared.wake.wait(state).expect("social state poisoned");
            };
            shared.set_connection(&mut state, Connection::Connecting);
            (state.generation, state.config.clone(), token, account)
        };
        let (ready, close) = match session(shared, generation, &config, &token, account) {
            Ended::Shutdown => return,
            Ended::Superseded => {
                failures = 0;
                continue;
            }
            Ended::Failed { ready, close } => (ready, close),
        };
        if close == Some(CLOSE_SIGNED_OUT) || (close == Some(CLOSE_BAD_AUTH) && !account) {
            failures = 0;
            shared.token_rejected(&token, !account);
            continue;
        }
        failures = if ready { 1 } else { failures.saturating_add(1) };
        let deadline = Instant::now() + backoff(&shared.timing, failures);
        let mut state = shared.lock();
        state.outbound.clear();
        if state.generation == generation {
            shared.set_connection(&mut state, Connection::Offline);
        }
        if close == Some(CLOSE_BAD_AUTH) {
            shared.request_jobs(&mut state, true, false);
        }
        while !state.shutdown && state.generation == generation {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            state = shared
                .wake
                .wait_timeout(state, remaining)
                .expect("social state poisoned")
                .0;
        }
    }
}

/// Delay before reconnect attempt `failures` (1-based): a random point in the
/// upper half of `base * 2^(failures - 1)`, capped at `backoff_max`.
pub(super) fn backoff(timing: &Timing, failures: u32) -> Duration {
    let exponent = failures.saturating_sub(1).min(16);
    let ceiling = timing
        .backoff_base
        .saturating_mul(1_u32 << exponent)
        .min(timing.backoff_max);
    let half = ceiling / 2;
    half + half.mul_f64(rand::random::<f64>())
}

fn superseded(shared: &Shared, generation: u64) -> Option<Ended> {
    let state = shared.lock();
    if state.shutdown {
        Some(Ended::Shutdown)
    } else if state.generation != generation {
        Some(Ended::Superseded)
    } else {
        None
    }
}

fn close_code(frame: Option<CloseFrame>) -> Option<u16> {
    frame.map(|frame| u16::from(frame.code))
}

pub(super) fn session(
    shared: &Shared,
    generation: u64,
    config: &Config,
    token: &Token,
    account: bool,
) -> Ended {
    let timing = shared.timing;
    let Ok(mut socket) = connect(config, timing.connect) else {
        return Ended::failed(false);
    };
    let auth = json!({"type":"auth", "token":token.expose()}).to_string();
    if socket.send(Message::text(auth)).is_err() {
        return Ended::failed(false);
    }
    let _ = socket
        .get_mut()
        .tcp_mut()
        .set_read_timeout(Some(timing.tick));
    let deadline = Instant::now() + timing.auth;
    let ready = loop {
        if let Some(ended) = superseded(shared, generation) {
            close(&mut socket);
            return ended;
        }
        if Instant::now() >= deadline {
            close(&mut socket);
            return Ended::failed(false);
        }
        match socket.read() {
            Ok(Message::Text(text)) => match serde_json::from_str::<Value>(text.as_str()) {
                Ok(frame) if frame["type"] == "ready" => break frame,
                Ok(frame) if frame["type"] == "error" => super::log_error_frame(&frame),
                _ => {}
            },
            Ok(Message::Close(frame)) => {
                return Ended::Failed {
                    ready: false,
                    close: close_code(frame),
                };
            }
            Ok(_) => {}
            Err(error) if is_timeout(&error) => {}
            Err(_) => return Ended::failed(false),
        }
    };
    {
        let mut state = shared.lock();
        if state.generation != generation || state.shutdown {
            drop(state);
            close(&mut socket);
            return superseded(shared, generation).unwrap_or(Ended::Superseded);
        }
        shared.connected(&mut state, &ready, account);
    }
    let mut last_ping = Instant::now();
    let mut last_heard = Instant::now();
    loop {
        let now = Instant::now();
        let frames = {
            let mut state = shared.lock();
            if state.shutdown {
                drop(state);
                close(&mut socket);
                return Ended::Shutdown;
            }
            if state.generation != generation {
                drop(state);
                close(&mut socket);
                return Ended::Superseded;
            }
            let mut frames = state.outbound.drain(..).collect::<Vec<_>>();
            if account && let Some(presence) = state.presence.take_due(now, timing.presence) {
                frames.push(presence);
            }
            frames
        };
        if now.duration_since(last_ping) >= timing.ping {
            last_ping = now;
            if socket.send(Message::text(r#"{"type":"ping"}"#)).is_err() {
                return Ended::failed(true);
            }
        }
        for frame in frames {
            if socket.send(Message::text(frame)).is_err() {
                return Ended::failed(true);
            }
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                last_heard = Instant::now();
                shared.handle_frame(text.as_str());
            }
            Ok(Message::Close(frame)) => {
                return Ended::Failed {
                    ready: true,
                    close: close_code(frame),
                };
            }
            Ok(_) => last_heard = Instant::now(),
            Err(error) if is_timeout(&error) => {}
            Err(_) => return Ended::failed(true),
        }
        if last_heard.elapsed() > timing.silence {
            close(&mut socket);
            return Ended::failed(true);
        }
    }
}

fn is_timeout(error: &tungstenite::Error) -> bool {
    matches!(error, tungstenite::Error::Io(error) if matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut))
}

fn close(socket: &mut WebSocket<Transport>) {
    let _ = socket.close(None);
    let _ = socket.flush();
}

pub(super) enum Transport {
    Plain(TimedTcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TimedTcpStream>>),
}

impl Transport {
    fn tcp_mut(&mut self) -> &mut TimedTcpStream {
        match self {
            Self::Plain(stream) => stream,
            Self::Tls(stream) => stream.get_mut(),
        }
    }
}

impl Read for Transport {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.read(buffer),
            Self::Tls(stream) => stream.read(buffer),
        }
    }
}

impl Write for Transport {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.write(buffer),
            Self::Tls(stream) => stream.write(buffer),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Plain(stream) => stream.flush(),
            Self::Tls(stream) => stream.flush(),
        }
    }
}

fn tls_config() -> Arc<rustls::ClientConfig> {
    static CONFIG: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    Arc::clone(CONFIG.get_or_init(|| {
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        Arc::new(
            rustls::ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth(),
        )
    }))
}

fn other(message: &str) -> io::Error {
    io::Error::other(message.to_owned())
}

/// Opens the backend socket, through the configured session proxy when one is
/// enabled.
pub(super) fn connect(config: &Config, timeout: Duration) -> io::Result<WebSocket<Transport>> {
    let secure = config.base.scheme() == "https";
    let authority = config
        .base
        .host_str()
        .ok_or_else(|| other("missing host"))?;
    let host = authority.trim_start_matches('[').trim_end_matches(']');
    let port = config
        .base
        .port_or_known_default()
        .ok_or_else(|| other("missing port"))?;
    let proxy =
        crate::proxy::config_from_settings(&config.proxy).map_err(|message| other(&message))?;
    let stream = match proxy {
        Some(proxy) => tunnel(&proxy.normalized_url, authority, host, port, timeout)?,
        None => tcp_connect(host, port, timeout)?,
    };
    let _ = stream.get_ref().set_nodelay(true);
    let transport = if secure {
        let name = rustls::pki_types::ServerName::try_from(host.to_owned())
            .map_err(|_| other("invalid host"))?;
        let connection = rustls::ClientConnection::new(tls_config(), name)
            .map_err(|_| other("TLS setup failed"))?;
        Transport::Tls(Box::new(rustls::StreamOwned::new(connection, stream)))
    } else {
        Transport::Plain(stream)
    };
    let mut url = config.base.clone();
    url.set_scheme(if secure { "wss" } else { "ws" })
        .map_err(|()| other("invalid socket scheme"))?;
    url.set_path("/v1/ws");
    let mut socket_config = WebSocketConfig::default();
    socket_config.max_message_size = Some(MAXIMUM_FRAME_BYTES);
    socket_config.max_frame_size = Some(MAXIMUM_FRAME_BYTES);
    let (socket, _) =
        tungstenite::client::client_with_config(url.as_str(), transport, Some(socket_config))
            .map_err(|_| other("WebSocket handshake failed"))?;
    Ok(socket)
}

fn tcp_connect(host: &str, port: u16, timeout: Duration) -> io::Result<TimedTcpStream> {
    let addresses = (host, port)
        .to_socket_addrs()?
        .take(4)
        .collect::<Vec<SocketAddr>>();
    let mut last = other("host did not resolve");
    for address in addresses {
        match TcpStream::connect_timeout(&address, timeout) {
            Ok(stream) => {
                let mut stream = TimedTcpStream::new(stream)?;
                stream.set_read_timeout(Some(timeout))?;
                stream.set_write_timeout(Some(timeout))?;
                return Ok(stream);
            }
            Err(error) => last = error,
        }
    }
    Err(last)
}

fn tunnel(
    proxy_url: &str,
    authority: &str,
    host: &str,
    port: u16,
    timeout: Duration,
) -> io::Result<TimedTcpStream> {
    let proxy = Url::parse(proxy_url).map_err(|_| other("invalid proxy"))?;
    let proxy_host = proxy
        .host_str()
        .ok_or_else(|| other("invalid proxy"))?
        .trim_start_matches('[')
        .trim_end_matches(']');
    let proxy_port = proxy.port().ok_or_else(|| other("invalid proxy"))?;
    let mut stream = tcp_connect(proxy_host, proxy_port, timeout)?;
    let username = percent_decode(proxy.username());
    let password = percent_decode(proxy.password().unwrap_or(""));
    match proxy.scheme() {
        "http" => http_connect(&mut stream, authority, port, &username, &password)?,
        "socks5" => socks5_connect(&mut stream, host, port, &username, &password)?,
        "socks4" => socks4a_connect(&mut stream, host, port, &username)?,
        _ => return Err(other("this proxy type cannot carry the social connection")),
    }
    Ok(stream)
}

fn percent_decode(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let hex = bytes
            .get(index + 1..index + 3)
            .and_then(|pair| std::str::from_utf8(pair).ok())
            .and_then(|pair| u8::from_str_radix(pair, 16).ok());
        match (bytes[index], hex) {
            (b'%', Some(byte)) => {
                decoded.push(byte);
                index += 3;
            }
            (byte, _) => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    decoded
}

fn http_connect(
    stream: &mut TimedTcpStream,
    authority: &str,
    port: u16,
    username: &[u8],
    password: &[u8],
) -> io::Result<()> {
    let mut request =
        format!("CONNECT {authority}:{port} HTTP/1.1\r\nHost: {authority}:{port}\r\n");
    if !username.is_empty() {
        let credentials = [username, b":", password].concat();
        request.push_str(&format!(
            "Proxy-Authorization: Basic {}\r\n",
            base64::engine::general_purpose::STANDARD.encode(credentials)
        ));
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes())?;
    let mut head = Vec::new();
    let mut byte = [0_u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if head.len() >= MAXIMUM_PROXY_RESPONSE_BYTES || stream.read(&mut byte)? == 0 {
            return Err(other("proxy closed the tunnel"));
        }
        head.push(byte[0]);
    }
    let status = head
        .split(|byte| *byte == b' ')
        .nth(1)
        .and_then(|code| std::str::from_utf8(code).ok());
    if status == Some("200") {
        Ok(())
    } else {
        Err(other("proxy refused the tunnel"))
    }
}

fn socks5_connect(
    stream: &mut TimedTcpStream,
    host: &str,
    port: u16,
    username: &[u8],
    password: &[u8],
) -> io::Result<()> {
    let credentials = !username.is_empty();
    stream.write_all(if credentials {
        &[5, 2, 0, 2]
    } else {
        &[5, 1, 0]
    })?;
    let mut reply = [0_u8; 2];
    stream.read_exact(&mut reply)?;
    match reply {
        [5, 0] => {}
        [5, 2] if credentials => {
            let (Ok(user_length), Ok(password_length)) =
                (u8::try_from(username.len()), u8::try_from(password.len()))
            else {
                return Err(other("proxy credentials are too long"));
            };
            let mut negotiation = vec![1, user_length];
            negotiation.extend_from_slice(username);
            negotiation.push(password_length);
            negotiation.extend_from_slice(password);
            stream.write_all(&negotiation)?;
            stream.read_exact(&mut reply)?;
            if reply[1] != 0 {
                return Err(other("proxy rejected the credentials"));
            }
        }
        _ => return Err(other("proxy offered no usable authentication")),
    }
    let host_length = u8::try_from(host.len()).map_err(|_| other("host is too long"))?;
    let mut request = vec![5, 1, 0, 3, host_length];
    request.extend_from_slice(host.as_bytes());
    request.extend_from_slice(&port.to_be_bytes());
    stream.write_all(&request)?;
    let mut header = [0_u8; 4];
    stream.read_exact(&mut header)?;
    if header[0] != 5 || header[1] != 0 {
        return Err(other("proxy refused the tunnel"));
    }
    let address_length = match header[3] {
        1 => 4,
        4 => 16,
        3 => {
            let mut length = [0_u8; 1];
            stream.read_exact(&mut length)?;
            usize::from(length[0])
        }
        _ => return Err(other("proxy returned an invalid address")),
    };
    let mut bound = vec![0_u8; address_length + 2];
    stream.read_exact(&mut bound)
}

fn socks4a_connect(
    stream: &mut TimedTcpStream,
    host: &str,
    port: u16,
    username: &[u8],
) -> io::Result<()> {
    let mut request = vec![4, 1];
    request.extend_from_slice(&port.to_be_bytes());
    request.extend_from_slice(&[0, 0, 0, 1]);
    request.extend_from_slice(username);
    request.push(0);
    request.extend_from_slice(host.as_bytes());
    request.push(0);
    stream.write_all(&request)?;
    let mut reply = [0_u8; 8];
    stream.read_exact(&mut reply)?;
    if reply[1] == 0x5a {
        Ok(())
    } else {
        Err(other("proxy refused the tunnel"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_is_jittered_exponential_and_capped() {
        let timing = Timing {
            backoff_base: Duration::from_secs(1),
            backoff_max: Duration::from_secs(60),
            ..Timing::default()
        };
        for (failures, ceiling) in [(1, 1), (2, 2), (3, 4), (6, 32), (7, 60), (40, 60)] {
            let ceiling = Duration::from_secs(ceiling);
            let samples = (0..64)
                .map(|_| backoff(&timing, failures))
                .collect::<Vec<_>>();
            assert!(
                samples
                    .iter()
                    .all(|delay| *delay >= ceiling / 2 && *delay <= ceiling)
            );
            assert!(
                samples.iter().any(|delay| *delay != samples[0]),
                "delays are jittered"
            );
        }
    }

    #[test]
    fn proxy_credentials_are_percent_decoded() {
        assert_eq!(percent_decode("us%40er"), b"us@er");
        assert_eq!(percent_decode("p%3Aw%zz"), b"p:w%zz");
    }
}
