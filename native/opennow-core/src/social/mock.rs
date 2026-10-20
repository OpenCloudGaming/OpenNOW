//! Local HTTP + WebSocket stand-in for the social backend. Tests only; it binds
//! 127.0.0.1 and never reaches an external network.

use serde_json::{Value, json};
use std::io::{self, BufRead, BufReader, Cursor, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tungstenite::{Message, WebSocket};

#[derive(Clone, Debug)]
pub(super) struct Recorded {
    pub method: String,
    pub path: String,
    pub authorization: Option<String>,
    pub body: Value,
}

pub(super) struct Reply {
    pub status: u16,
    pub body: Option<Value>,
}

impl Reply {
    pub fn json(status: u16, body: Value) -> Self {
        Self {
            status,
            body: Some(body),
        }
    }

    pub fn empty(status: u16) -> Self {
        Self { status, body: None }
    }

    pub fn error(status: u16, code: &str) -> Self {
        Self::json(
            status,
            json!({"error":{"code":code, "message":format!("mock {code}")}}),
        )
    }
}

type Handler = Box<dyn Fn(&Recorded) -> Reply + Send + Sync>;

pub(super) struct MockBackend {
    pub url: String,
    requests: Arc<Mutex<Vec<Recorded>>>,
    handler: Arc<Mutex<Handler>>,
    sockets: Receiver<WsPeer>,
}

impl MockBackend {
    pub fn start(handler: impl Fn(&Recorded) -> Reply + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let handler: Arc<Mutex<Handler>> = Arc::new(Mutex::new(Box::new(handler)));
        let (socket_tx, sockets) = mpsc::channel();
        let (thread_requests, thread_handler) = (Arc::clone(&requests), Arc::clone(&handler));
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                let (requests, handler, socket_tx) = (
                    Arc::clone(&thread_requests),
                    Arc::clone(&thread_handler),
                    socket_tx.clone(),
                );
                thread::spawn(move || serve(stream, &requests, &handler, &socket_tx));
            }
        });
        Self {
            url,
            requests,
            handler,
            sockets,
        }
    }

    pub fn set_handler(&self, handler: impl Fn(&Recorded) -> Reply + Send + Sync + 'static) {
        *self.handler.lock().unwrap() = Box::new(handler);
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.requests.lock().unwrap().clone()
    }

    pub fn clear_requests(&self) {
        self.requests.lock().unwrap().clear();
    }

    pub fn count(&self, method: &str, path: &str) -> usize {
        self.requests()
            .iter()
            .filter(|request| request.method == method && request.path == path)
            .count()
    }

    /// Waits for the next socket and completes the auth exchange.
    pub fn accept(&self, expected_token: &str, user: Value) -> WsPeer {
        let mut peer = self.next_socket();
        let auth = peer.recv(Duration::from_secs(5)).expect("auth frame");
        assert_eq!(auth, json!({"type":"auth", "token":expected_token}));
        peer.send(json!({"type":"ready", "user":user, "guest":null}));
        peer
    }

    pub fn next_socket_within(&self, timeout: Duration) -> Option<WsPeer> {
        self.sockets.recv_timeout(timeout).ok()
    }

    pub fn next_socket(&self) -> WsPeer {
        self.sockets
            .recv_timeout(Duration::from_secs(10))
            .expect("the core opened a socket")
    }
}

fn serve(
    stream: TcpStream,
    requests: &Mutex<Vec<Recorded>>,
    handler: &Mutex<Handler>,
    sockets: &Sender<WsPeer>,
) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut head = Vec::new();
    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        head.extend_from_slice(line.as_bytes());
        if line == "\r\n" {
            break;
        }
        headers.push(line.trim_end().to_owned());
    }
    let request_line = headers.first().cloned().unwrap_or_default();
    let mut parts = request_line.split(' ');
    let method = parts.next().unwrap_or_default().to_owned();
    let path = parts.next().unwrap_or_default().to_owned();
    let header = |name: &str| {
        headers.iter().skip(1).find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.trim()
                .eq_ignore_ascii_case(name)
                .then(|| value.trim().to_owned())
        })
    };
    if header("upgrade").is_some_and(|value| value.eq_ignore_ascii_case("websocket")) {
        let buffered = reader.buffer().to_vec();
        head.extend_from_slice(&buffered);
        let replay = Replay {
            prefix: Cursor::new(head),
            inner: stream,
        };
        if let Ok(socket) = tungstenite::accept(replay) {
            let _ = sockets.send(WsPeer { socket });
        }
        return;
    }
    let length = header("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = vec![0_u8; length];
    if reader.read_exact(&mut body).is_err() {
        return;
    }
    let recorded = Recorded {
        method,
        path,
        authorization: header("authorization"),
        body: serde_json::from_slice(&body).unwrap_or(Value::Null),
    };
    requests.lock().unwrap().push(recorded.clone());
    let reply = (handler.lock().unwrap())(&recorded);
    let payload = reply.body.map(|body| body.to_string()).unwrap_or_default();
    let mut stream = stream;
    let _ = write!(
        stream,
        "HTTP/1.1 {} Mock\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        reply.status,
        payload.len(),
        payload
    );
    let _ = stream.flush();
}

pub(super) struct Replay {
    prefix: Cursor<Vec<u8>>,
    pub inner: TcpStream,
}

impl Read for Replay {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count = self.prefix.read(buffer)?;
        if count > 0 {
            return Ok(count);
        }
        self.inner.read(buffer)
    }
}

impl Write for Replay {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.inner.write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

pub(super) struct WsPeer {
    socket: WebSocket<Replay>,
}

impl WsPeer {
    /// Next JSON text frame, or None when nothing arrives in time.
    pub fn recv(&mut self, timeout: Duration) -> Option<Value> {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None;
            }
            self.socket
                .get_ref()
                .inner
                .set_read_timeout(Some(remaining.min(Duration::from_millis(50))))
                .unwrap();
            match self.socket.read() {
                Ok(Message::Text(text)) => return serde_json::from_str(text.as_str()).ok(),
                Ok(_) => {}
                Err(tungstenite::Error::Io(error))
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => return None,
            }
        }
    }

    /// Next frame of `kind`, skipping others (pings, presence...).
    pub fn recv_type(&mut self, kind: &str, timeout: Duration) -> Option<Value> {
        let deadline = Instant::now() + timeout;
        loop {
            let frame = self.recv(deadline.saturating_duration_since(Instant::now()))?;
            if frame["type"] == kind {
                return Some(frame);
            }
        }
    }

    /// Every frame of `kind` received during `window`, with arrival times.
    pub fn collect(&mut self, kind: &str, window: Duration) -> Vec<(Instant, Value)> {
        let deadline = Instant::now() + window;
        let mut frames = Vec::new();
        while let Some(frame) = self.recv(deadline.saturating_duration_since(Instant::now())) {
            if frame["type"] == kind {
                frames.push((Instant::now(), frame));
            }
        }
        frames
    }

    pub fn send(&mut self, value: Value) {
        self.socket.send(Message::text(value.to_string())).unwrap();
    }

    /// Closes the socket with a backend close code.
    pub fn close_with(mut self, code: u16) {
        let _ = self
            .socket
            .close(Some(tungstenite::protocol::frame::CloseFrame {
                code: code.into(),
                reason: "".into(),
            }));
        let _ = self.socket.flush();
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline && self.recv(Duration::from_millis(50)).is_some() {}
    }

    /// Drops the TCP connection without a close handshake.
    pub fn sever(self) {
        let _ = self.socket.get_ref().inner.shutdown(Shutdown::Both);
    }
}
