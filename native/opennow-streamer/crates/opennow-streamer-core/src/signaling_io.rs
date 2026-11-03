use std::io::{self, ErrorKind};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::{OnceLock, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use tungstenite::handshake::client::Request;
use tungstenite::protocol::WebSocketConfig;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Connector, WebSocket};

pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(5);

struct ResolveRequest {
    host: String,
    port: u16,
    result: mpsc::SyncSender<io::Result<Vec<SocketAddr>>>,
}

fn check(deadline: Instant, cancelled: &impl Fn() -> bool) -> io::Result<()> {
    if cancelled() {
        return Err(ErrorKind::Interrupted.into());
    }
    if Instant::now() >= deadline {
        return Err(ErrorKind::TimedOut.into());
    }
    Ok(())
}

pub(crate) fn resolve(
    host: String,
    port: u16,
    deadline: Instant,
    cancelled: impl Fn() -> bool,
) -> io::Result<Vec<SocketAddr>> {
    check(deadline, &cancelled)?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return Ok(vec![SocketAddr::new(ip, port)]);
    }
    static RESOLVER: OnceLock<Option<mpsc::SyncSender<ResolveRequest>>> = OnceLock::new();
    let resolver = RESOLVER
        .get_or_init(|| {
            let (sender, receiver) = mpsc::sync_channel::<ResolveRequest>(1);
            thread::Builder::new()
                .name("opennow-signaling-dns".to_owned())
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
        .ok_or_else(|| io::Error::other("Could not start bounded DNS resolver"))?;
    let (sender, receiver) = mpsc::sync_channel(1);
    resolver
        .try_send(ResolveRequest {
            host: host.to_owned(),
            port,
            result: sender,
        })
        .map_err(|_| io::Error::new(ErrorKind::WouldBlock, "Signaling DNS resolver is busy"))?;
    loop {
        check(deadline, &cancelled)?;
        let result = receiver.recv_timeout(
            deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(25)),
        );
        check(deadline, &cancelled)?;
        match result {
            Ok(Ok(addresses)) if !addresses.is_empty() => return Ok(addresses),
            Ok(Err(error)) => return Err(error),
            Ok(Ok(_)) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(io::Error::new(ErrorKind::NotFound, "Signaling DNS failed"));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

pub(crate) fn connect(
    request: Request,
    deadline: Instant,
    cancelled: impl Fn() -> bool,
    config: WebSocketConfig,
    connector: Option<Connector>,
) -> Result<WebSocket<MaybeTlsStream<TcpStream>>, tungstenite::Error> {
    let host = request
        .uri()
        .host()
        .ok_or_else(|| io::Error::other("Missing signaling host"))?;
    let port = request.uri().port_u16().unwrap_or_else(|| {
        if request.uri().scheme_str() == Some("ws") {
            80
        } else {
            443
        }
    });
    let addresses = resolve(host.to_owned(), port, deadline, &cancelled)?;
    let mut connected = None;
    let mut last_error = io::Error::from(ErrorKind::NotConnected);
    for address in addresses {
        check(deadline, &cancelled)?;
        let budget = deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(500));
        match TcpStream::connect_timeout(&address, budget) {
            Ok(stream) => {
                connected = Some(stream);
                break;
            }
            Err(error) => last_error = error,
        }
    }
    check(deadline, &cancelled)?;
    let stream = connected.ok_or(last_error)?;
    stream.set_nodelay(true)?;
    stream.set_nonblocking(true)?;
    let mut handshake =
        tungstenite::client_tls_with_config(request, stream, Some(config), connector);
    loop {
        check(deadline, &cancelled)?;
        match handshake {
            Ok((socket, _)) => return Ok(socket),
            Err(tungstenite::HandshakeError::Interrupted(pending)) => {
                thread::sleep(
                    POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())),
                );
                check(deadline, &cancelled)?;
                handshake = pending.handshake();
            }
            Err(tungstenite::HandshakeError::Failure(error)) => return Err(error),
        }
    }
}
