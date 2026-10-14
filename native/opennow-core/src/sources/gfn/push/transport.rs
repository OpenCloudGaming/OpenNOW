use crate::sources::gfn::push::PushError;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

pub trait PushTransport: Send {
    fn send(&mut self, frame: &[u8], timeout: Duration) -> Result<(), PushError>;
    fn recv(&mut self, timeout: Duration) -> Result<Vec<u8>, PushError>;
    fn close(&mut self);
}

pub trait PushTransportFactory: Send + Sync {
    fn connect(
        &self,
        host: &str,
        port: u16,
        timeout: Duration,
    ) -> Result<Box<dyn PushTransport>, PushError>;
}

pub struct TlsPushTransport {
    connection: rustls::ClientConnection,
    socket: TcpStream,
}

enum Resolution {
    Addrs(Vec<SocketAddr>),
    Failed,
}

pub struct TlsPushTransportFactory {
    pending: Mutex<Option<(String, u16, Receiver<Resolution>)>>,
}

impl Default for TlsPushTransportFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl TlsPushTransportFactory {
    pub fn new() -> Self {
        Self {
            pending: Mutex::new(None),
        }
    }

    fn resolve(
        &self,
        host: &str,
        port: u16,
        timeout: Duration,
    ) -> Result<Vec<SocketAddr>, PushError> {
        let mut pending = self.pending.lock().expect("push resolver poisoned");
        if let Some((pending_host, pending_port, receiver)) = pending.take() {
            match receiver.try_recv() {
                Ok(Resolution::Addrs(addrs)) => {
                    if pending_host == host && pending_port == port {
                        return Ok(addrs);
                    }
                }
                Ok(Resolution::Failed) | Err(TryRecvError::Disconnected) => {
                    if pending_host == host && pending_port == port {
                        return Err(PushError::new(
                            "push_transport_failed",
                            "The push service could not be resolved",
                        ));
                    }
                }
                Err(TryRecvError::Empty) => {
                    *pending = Some((pending_host, pending_port, receiver));
                    return Err(PushError::new(
                        "push_transport_resolving",
                        "A previous push service resolution is still in progress",
                    ));
                }
            }
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        let owned_host = host.to_owned();
        std::thread::Builder::new()
            .name("opennow-push-resolver".to_owned())
            .spawn(move || {
                let resolved = (owned_host.as_str(), port)
                    .to_socket_addrs()
                    .map(|addrs| Resolution::Addrs(addrs.collect()))
                    .unwrap_or(Resolution::Failed);
                let _ = sender.send(resolved);
            })
            .map_err(|_| {
                PushError::new(
                    "push_transport_failed",
                    "The push service resolver could not start",
                )
            })?;
        match receiver.recv_timeout(timeout) {
            Ok(Resolution::Addrs(addrs)) => {
                if addrs.is_empty() {
                    return Err(PushError::new(
                        "push_transport_failed",
                        "The push service could not be resolved",
                    ));
                }
                Ok(addrs)
            }
            Ok(Resolution::Failed) => Err(PushError::new(
                "push_transport_failed",
                "The push service could not be resolved",
            )),
            Err(_) => {
                *pending = Some((host.to_owned(), port, receiver));
                Err(PushError::new(
                    "push_transport_failed",
                    "The push service resolution timed out",
                ))
            }
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

fn is_timeout(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    )
}

fn apply_timeout(deadline: Instant) -> Option<Duration> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    (!remaining.is_zero()).then_some(remaining)
}

impl TlsPushTransport {
    fn open(host: &str, address: SocketAddr, deadline: Instant) -> Result<Self, PushError> {
        let server_name =
            rustls::pki_types::ServerName::try_from(host.to_owned()).map_err(|_| {
                PushError::new("push_transport_invalid", "The push service host is invalid")
            })?;
        let connection =
            rustls::ClientConnection::new(tls_config(), server_name).map_err(|_| {
                PushError::new(
                    "push_transport_failed",
                    "The push connection could not start",
                )
            })?;
        let Some(remaining) = apply_timeout(deadline) else {
            return Err(PushError::new(
                "push_transport_failed",
                "The push service connection timed out",
            ));
        };
        let socket = TcpStream::connect_timeout(&address, remaining).map_err(|_| {
            PushError::new("push_transport_failed", "The push service is unreachable")
        })?;
        let _ = socket.set_nodelay(true);
        Ok(Self { connection, socket })
    }

    fn finish_handshake(&mut self, deadline: Instant) -> Result<(), PushError> {
        while self.connection.is_handshaking() {
            let Some(remaining) = apply_timeout(deadline) else {
                return Err(PushError::new(
                    "push_transport_failed",
                    "The push service handshake timed out",
                ));
            };
            let _ = self.socket.set_read_timeout(Some(remaining));
            let _ = self.socket.set_write_timeout(Some(remaining));
            match self.connection.complete_io(&mut self.socket) {
                Ok(_) => {}
                Err(error) if is_timeout(&error) => continue,
                Err(_) => {
                    return Err(PushError::new(
                        "push_transport_failed",
                        "The push service handshake failed",
                    ));
                }
            }
        }
        Ok(())
    }

    fn drain(&mut self, buffer: &mut [u8]) -> Result<usize, PushError> {
        self.connection
            .reader()
            .read(buffer)
            .map_err(|_| PushError::new("push_transport_failed", "The push connection failed"))
    }
}

impl PushTransport for TlsPushTransport {
    fn send(&mut self, frame: &[u8], timeout: Duration) -> Result<(), PushError> {
        let deadline = Instant::now() + timeout;
        self.finish_handshake(deadline)?;
        self.connection
            .writer()
            .write_all(frame)
            .map_err(|_| PushError::new("push_transport_failed", "The push connection failed"))?;
        while self.connection.wants_write() {
            let Some(remaining) = apply_timeout(deadline) else {
                return Err(PushError::new(
                    "push_transport_failed",
                    "The push message could not be sent in time",
                ));
            };
            let _ = self.socket.set_write_timeout(Some(remaining));
            match self.connection.write_tls(&mut self.socket) {
                Ok(0) => {
                    return Err(PushError::new(
                        "push_transport_closed",
                        "The push connection ended",
                    ));
                }
                Ok(_) => {}
                Err(error) if is_timeout(&error) => continue,
                Err(_) => {
                    return Err(PushError::new(
                        "push_transport_failed",
                        "The push connection failed",
                    ));
                }
            }
        }
        Ok(())
    }

    fn recv(&mut self, timeout: Duration) -> Result<Vec<u8>, PushError> {
        let deadline = Instant::now() + timeout;
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            match self.drain(&mut buffer) {
                Ok(0) => {}
                Ok(count) => return Ok(buffer[..count].to_vec()),
                Err(error) => return Err(error),
            }
            let Some(remaining) = apply_timeout(deadline) else {
                return Ok(Vec::new());
            };
            let _ = self.socket.set_read_timeout(Some(remaining));
            match self.connection.read_tls(&mut self.socket) {
                Ok(0) => {
                    return Err(PushError::new(
                        "push_transport_closed",
                        "The push connection ended",
                    ));
                }
                Ok(_) => {
                    self.connection.process_new_packets().map_err(|_| {
                        PushError::new("push_transport_failed", "The push connection failed")
                    })?;
                }
                Err(error) if is_timeout(&error) => {}
                Err(_) => {
                    return Err(PushError::new(
                        "push_transport_failed",
                        "The push connection failed",
                    ));
                }
            }
        }
    }

    fn close(&mut self) {
        self.connection.send_close_notify();
        let deadline = Instant::now() + Duration::from_millis(500);
        while self.connection.wants_write() {
            let Some(remaining) = apply_timeout(deadline) else {
                break;
            };
            let _ = self.socket.set_write_timeout(Some(remaining));
            if self.connection.write_tls(&mut self.socket).is_err() {
                break;
            }
        }
        let _ = self.socket.shutdown(std::net::Shutdown::Both);
    }
}

impl PushTransportFactory for TlsPushTransportFactory {
    fn connect(
        &self,
        host: &str,
        port: u16,
        timeout: Duration,
    ) -> Result<Box<dyn PushTransport>, PushError> {
        let deadline = Instant::now() + timeout;
        let addresses = self.resolve(host, port, timeout)?;
        let mut last_error = None;
        for address in addresses.into_iter().take(8) {
            match TlsPushTransport::open(host, address, deadline) {
                Ok(mut transport) => {
                    transport.finish_handshake(deadline)?;
                    return Ok(Box::new(transport));
                }
                Err(error) => last_error = Some(error),
            }
            if apply_timeout(deadline).is_none() {
                break;
            }
        }
        Err(last_error.unwrap_or_else(|| {
            PushError::new("push_transport_failed", "The push service is unreachable")
        }))
    }
}
