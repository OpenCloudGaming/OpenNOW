use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

enum Response {
    Origin(Option<String>),
    Proxy(SocketAddr),
    Stall,
}

struct Server {
    address: SocketAddr,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

fn headers(stream: &mut TcpStream, stop: &AtomicBool) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    while !stop.load(Ordering::Acquire) {
        let mut byte = [0];
        match stream.read(&mut byte) {
            Ok(0) => return None,
            Ok(_) => {
                bytes.push(byte[0]);
                assert!(bytes.len() < 16_384);
                if bytes.ends_with(b"\r\n\r\n") {
                    return Some(bytes);
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return None,
        }
    }
    None
}

impl Server {
    fn start(response: Response) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let received = requests.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let worker = thread::spawn(move || {
            while !stopping.load(Ordering::Acquire) {
                let mut stream = match listener.accept() {
                    Ok((stream, _)) => stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(error) => panic!("fixture accept failed: {error}"),
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_millis(50)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                while let Some(request) = headers(&mut stream, &stopping) {
                    let request = String::from_utf8(request).unwrap();
                    received
                        .lock()
                        .unwrap()
                        .push(request.lines().next().unwrap().to_owned());
                    let reply = match &response {
                        Response::Origin(location) => {
                            let status = if location.is_some() {
                                "302 Found"
                            } else {
                                "503 Unavailable"
                            };
                            let location = location
                                .as_ref()
                                .map(|url| format!("Location: {url}\r\n"))
                                .unwrap_or_default();
                            format!("HTTP/1.1 {status}\r\n{location}Content-Length: 0\r\nConnection: keep-alive\r\n\r\n").into_bytes()
                        }
                        Response::Proxy(origin) => {
                            let mut upstream =
                                TcpStream::connect_timeout(origin, Duration::from_secs(1)).unwrap();
                            upstream
                                .set_read_timeout(Some(Duration::from_millis(50)))
                                .unwrap();
                            upstream
                                .set_write_timeout(Some(Duration::from_secs(1)))
                                .unwrap();
                            let target = request.split_whitespace().nth(1).unwrap();
                            let target = url::Url::parse(target)
                                .expect("proxy requires absolute-form target");
                            write!(
                                upstream,
                                "HEAD {} HTTP/1.1\r\nHost: {}\r\n\r\n",
                                target.path(),
                                origin
                            )
                            .unwrap();
                            headers(&mut upstream, &stopping).unwrap()
                        }
                        Response::Stall => continue,
                    };
                    if stream.write_all(&reply).is_err() {
                        break;
                    }
                }
            }
        });
        Self {
            address,
            requests,
            stop,
            worker: Some(worker),
        }
    }

    fn url(&self) -> String {
        format!("http://{}/probe", self.address)
    }

    fn proxy_url(&self) -> String {
        format!("http://localhost:{}", self.address.port())
    }

    fn count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.worker.take().unwrap().join().unwrap();
    }
}

struct Core {
    child: Child,
    messages: mpsc::Receiver<Value>,
    output: Arc<Mutex<String>>,
    readers: Vec<JoinHandle<()>>,
    serial: u32,
    _directory: tempfile::TempDir,
}

impl Core {
    fn start(settings: Value, environment: &[(&str, &str)]) -> Self {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("settings.json"), settings.to_string()).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_opennow-core"));
        command
            .arg("--data-dir")
            .arg(directory.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for key in [
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "NO_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
            "no_proxy",
            "REQUEST_METHOD",
        ] {
            command.env_remove(key);
        }
        for (key, value) in environment {
            command.env(key, value);
        }
        let mut child = command.spawn().unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let output = Arc::new(Mutex::new(String::new()));
        let transcript = output.clone();
        let (sender, messages) = mpsc::channel();
        let stdout_reader = thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let line = line.unwrap();
                transcript.lock().unwrap().push_str(&line);
                if sender.send(serde_json::from_str(&line).unwrap()).is_err() {
                    break;
                }
            }
        });
        let transcript = output.clone();
        let stderr_reader = thread::spawn(move || {
            let mut text = String::new();
            BufReader::new(stderr).read_to_string(&mut text).unwrap();
            transcript.lock().unwrap().push_str(&text);
        });
        Self {
            child,
            messages,
            output,
            readers: vec![stdout_reader, stderr_reader],
            serial: 0,
            _directory: directory,
        }
    }

    fn send(&mut self, method: &str, params: Value) -> String {
        self.serial += 1;
        let id = self.serial.to_string();
        writeln!(
            self.child.stdin.as_mut().unwrap(),
            "{}",
            json!({"type":"request","id":id,"method":method,"params":params})
        )
        .unwrap();
        id
    }

    fn receive(&self, id: &str, timeout: Duration) -> Value {
        let deadline = Instant::now() + timeout;
        loop {
            let response = self
                .messages
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("core response deadline");
            if response["id"] == id {
                return response;
            }
        }
    }

    fn ping(&mut self, url: &str) -> Value {
        let id = self.send("network.regions.ping", json!({"regions":[{"url":url}]}));
        self.receive(&id, Duration::from_secs(6))
    }

    fn stop(&mut self) {
        let _ = self.child.kill();
        self.child.wait().unwrap();
        for reader in self.readers.drain(..) {
            reader.join().unwrap();
        }
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        self.stop();
    }
}

fn measured(response: &Value) {
    assert_eq!(response["ok"], true, "{response}");
    assert!(
        response["result"]["results"][0]["pingMs"].is_u64(),
        "{response}"
    );
}

#[test]
fn explicit_proxy_wins_over_environment_and_no_proxy_without_origin_dns() {
    let origin = Server::start(Response::Origin(None));
    let explicit = Server::start(Response::Proxy(origin.address));
    let environment = Server::start(Response::Proxy(origin.address));
    let mut core = Core::start(
        json!({"sessionProxyEnabled":true,"sessionProxyUrl":explicit.proxy_url()}),
        &[("HTTP_PROXY", &environment.proxy_url()), ("NO_PROXY", "*")],
    );
    let url = format!("http://region.invalid:{}/probe", origin.address.port());
    measured(&core.ping(&url));
    assert_eq!(explicit.count(), 4);
    assert_eq!(environment.count(), 0);
    assert_eq!(origin.count(), 4);
    assert!(
        explicit
            .requests
            .lock()
            .unwrap()
            .iter()
            .all(|request| request == &format!("HEAD {url} HTTP/1.1"))
    );
}

#[test]
fn absent_proxy_uses_environment() {
    environment_routing(json!({}), false);
}

#[test]
fn disabled_proxy_uses_environment_and_ignores_stale_invalid_url() {
    environment_routing(
        json!({"sessionProxyEnabled":false,"sessionProxyUrl":"invalid"}),
        false,
    );
}

#[test]
fn environment_no_proxy_is_honored() {
    environment_routing(json!({}), true);
}

fn environment_routing(settings: Value, bypass: bool) {
    let origin = Server::start(Response::Origin(None));
    let proxy = Server::start(Response::Proxy(origin.address));
    let mut core = Core::start(
        settings,
        &[
            ("http_proxy", &proxy.proxy_url()),
            ("no_proxy", if bypass { "127.0.0.1" } else { "" }),
        ],
    );
    measured(&core.ping(&origin.url()));
    assert_eq!(origin.count(), 4);
    assert_eq!(proxy.count(), if bypass { 0 } else { 4 });
}

#[test]
fn invalid_enabled_proxy_never_falls_back_or_logs_credentials() {
    let origin = Server::start(Response::Origin(None));
    let fallback = Server::start(Response::Proxy(origin.address));
    for url in [
        "ftp://private-user:private-secret@localhost:21",
        "",
        "http://private-user:private-secret@localhost:invalid",
    ] {
        let mut core = Core::start(
            json!({"sessionProxyEnabled":true,"sessionProxyUrl":url}),
            &[("HTTP_PROXY", &fallback.proxy_url())],
        );
        let response = core.ping(&origin.url());
        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], "region_ping_failed");
        core.stop();
        let output = core.output.lock().unwrap();
        assert!(!output.contains("private-secret"));
        assert!(!output.contains("private-user"));
    }
    assert_eq!(origin.count(), 0);
    assert_eq!(fallback.count(), 0);
}

#[test]
fn failed_authenticated_proxy_is_bounded_and_does_not_fall_back_or_log_credentials() {
    let origin = Server::start(Response::Origin(None));
    let fallback = Server::start(Response::Proxy(origin.address));
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let mut core = Core::start(
        json!({"sessionProxyEnabled":true,"sessionProxyUrl":format!("http://private-user:private-secret@localhost:{port}")}),
        &[("HTTP_PROXY", &fallback.proxy_url())],
    );
    let started = Instant::now();
    let response = core.ping(&origin.url());
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(
        response["result"]["results"][0]["error"],
        "HTTPS warm-up request failed"
    );
    assert_eq!(response["result"]["results"][0]["pingMs"], Value::Null);
    core.stop();
    drop(listener);
    let output = core.output.lock().unwrap();
    assert!(!output.contains("private-secret"));
    assert!(!output.contains("private-user"));
    assert_eq!(fallback.count(), 0);
    assert_eq!(origin.count(), 0);
}

#[test]
fn stalled_proxy_is_bounded_without_holding_the_settings_lock() {
    let origin = Server::start(Response::Origin(None));
    let fallback = Server::start(Response::Proxy(origin.address));
    let proxy = Server::start(Response::Stall);
    let mut core = Core::start(
        json!({"sessionProxyEnabled":true,"sessionProxyUrl":proxy.proxy_url()}),
        &[("HTTP_PROXY", &fallback.proxy_url())],
    );
    let started = Instant::now();
    let ping = core.send(
        "network.regions.ping",
        json!({"regions":[{"url":"http://region.invalid/probe"}]}),
    );
    while proxy.count() == 0 {
        assert!(started.elapsed() < Duration::from_secs(3));
        thread::sleep(Duration::from_millis(5));
    }
    let settings = core.send(
        "settings.set",
        json!({"key":"sessionProxyEnabled","value":false}),
    );
    assert_eq!(core.receive(&settings, Duration::from_secs(1))["ok"], true);
    let response = core.receive(&ping, Duration::from_secs(5));
    assert_eq!(
        response["result"]["results"][0]["error"],
        "HTTPS warm-up request failed"
    );
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(proxy.count(), 1);
    assert_eq!(fallback.count(), 0);
    assert_eq!(origin.count(), 0);
    measured(&core.ping(&origin.url()));
    assert_eq!(fallback.count(), 4);
    assert_eq!(origin.count(), 4);
}

#[test]
fn proxied_redirects_are_measured_without_following_the_location() {
    let destination = Server::start(Response::Origin(None));
    let origin = Server::start(Response::Origin(Some(destination.url())));
    let proxy = Server::start(Response::Proxy(origin.address));
    let mut core = Core::start(
        json!({"sessionProxyEnabled":true,"sessionProxyUrl":proxy.proxy_url()}),
        &[],
    );
    measured(&core.ping(&origin.url()));
    assert_eq!(proxy.count(), 4);
    assert_eq!(origin.count(), 4);
    assert_eq!(destination.count(), 0);
}
