use reqwest::blocking::Client;
use serde_json::{Map, Value, json};
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

const MAX_QUEUED_EVENTS: usize = 1_000;
const FLUSH_INTERVAL: Duration = Duration::from_secs(30);
const FLUSH_THRESHOLD: usize = 50;
const MAX_BATCH_EVENTS: usize = 100;
const MAX_SPOOL_BYTES: usize = 1024 * 1024;
const SHUTDOWN_BUDGET: Duration = Duration::from_millis(1_000);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Emitter {
    Core,
    Shell,
}

#[derive(Clone, Copy)]
enum Prop {
    Text(usize),
    Id,
    Code,
    Number(f64),
    Bool,
    Choice(&'static [&'static str]),
    Resolution,
}

const STAGES: &[&str] = &[
    "create",
    "poll",
    "claim",
    "prepare",
    "start",
    "stream",
    "presentation",
];
const REPORT_KINDS: &[&str] = &[
    "session_error",
    "stream_error",
    "frame_drops",
    "library_error",
    "manual",
];

type EventSpec = (&'static str, Emitter, &'static [(&'static str, Prop)]);

const EVENTS: &[EventSpec] = &[
    (
        "app_opened",
        Emitter::Core,
        &[
            ("gpu_vendor", Prop::Code),
            (
                "package_kind",
                Prop::Choice(&["msi", "appimage", "flatpak", "deb", "dmg", "zip"]),
            ),
        ],
    ),
    (
        "signed_in",
        Emitter::Core,
        &[
            ("provider_code", Prop::Id),
            ("alliance_partner", Prop::Bool),
            ("membership_tier", Prop::Id),
            ("restored", Prop::Bool),
        ],
    ),
    (
        "game_launch_requested",
        Emitter::Core,
        &[
            ("game_id", Prop::Id),
            ("game_title", Prop::Text(120)),
            ("store", Prop::Text(64)),
            ("zone", Prop::Id),
        ],
    ),
    (
        "session_started",
        Emitter::Shell,
        &[
            ("game_id", Prop::Id),
            ("game_title", Prop::Text(120)),
            ("codec", Prop::Choice(&["h264", "h265", "av1"])),
            ("resolution", Prop::Resolution),
            ("fps_target", Prop::Number(1_000.0)),
            ("hdr", Prop::Bool),
            ("decoder_backend", Prop::Code),
            ("first_frame_ms", Prop::Number(3_600_000.0)),
            ("queue_wait_s", Prop::Number(86_400.0)),
        ],
    ),
    (
        "session_ended",
        Emitter::Shell,
        &[
            ("game_id", Prop::Id),
            ("game_title", Prop::Text(120)),
            ("duration_s", Prop::Number(604_800.0)),
            (
                "outcome",
                Prop::Choice(&["clean", "user_stopped", "remote_ended", "error"]),
            ),
            ("error_code", Prop::Code),
            ("recoveries", Prop::Number(100_000.0)),
            ("video_drop_count", Prop::Number(1e12)),
            ("avg_fps", Prop::Number(1_000.0)),
            ("avg_ping_ms", Prop::Number(100_000.0)),
            ("avg_packet_loss_pct", Prop::Number(100.0)),
            ("decoder_errors", Prop::Number(1e12)),
        ],
    ),
    (
        "session_error",
        Emitter::Shell,
        &[
            ("stage", Prop::Choice(STAGES)),
            ("code", Prop::Code),
            ("game_id", Prop::Id),
        ],
    ),
    (
        "frame_drops_detected",
        Emitter::Shell,
        &[
            ("dropped", Prop::Number(1e12)),
            ("window_s", Prop::Number(3_600.0)),
            ("game_id", Prop::Id),
            ("decoder_backend", Prop::Code),
        ],
    ),
    (
        "library_load_failed",
        Emitter::Core,
        &[("code", Prop::Code)],
    ),
    (
        "bug_report_sent",
        Emitter::Core,
        &[
            ("report_id", Prop::Id),
            ("issue_id", Prop::Id),
            ("kind", Prop::Choice(REPORT_KINDS)),
            ("code", Prop::Code),
            ("automatic", Prop::Bool),
        ],
    ),
    (
        "consent_changed",
        Emitter::Core,
        &[
            ("value", Prop::Choice(&["enabled", "disabled"])),
            (
                "source",
                Prop::Choice(&["signin_notice", "first_run_sheet", "settings"]),
            ),
        ],
    ),
];

pub struct ShellEvent {
    pub name: &'static str,
    pub props: Map<String, Value>,
}

pub fn ui_surface(params: &Value) -> Result<Option<&'static str>, String> {
    match params["uiSurface"].as_str() {
        None => Ok(None),
        Some("desktop") => Ok(Some("desktop")),
        Some("console") => Ok(Some("console")),
        Some(_) => Err("Unsupported analytics surface".to_owned()),
    }
}

pub fn shell_event(params: &Value) -> Result<ShellEvent, String> {
    let name = params["event"].as_str().unwrap_or_default();
    let (name, emitter, _) = EVENTS
        .iter()
        .find(|(event, _, _)| *event == name)
        .ok_or_else(|| "Unsupported analytics event".to_owned())?;
    if *emitter != Emitter::Shell {
        return Err("Unsupported analytics event".to_owned());
    }
    Ok(ShellEvent {
        name,
        props: sanitize(name, &params["props"]),
    })
}

pub fn sanitize(event: &str, props: &Value) -> Map<String, Value> {
    let allowed = EVENTS
        .iter()
        .find(|(name, _, _)| *name == event)
        .map_or(&[][..], |(_, _, props)| *props);
    let mut clean = Map::new();
    for (key, prop) in allowed {
        let value = &props[*key];
        let accepted = match prop {
            Prop::Text(limit) => value
                .as_str()
                .map(|text| bounded_text(text, *limit))
                .filter(|text| !text.is_empty())
                .map(Value::from),
            Prop::Id => value
                .as_str()
                .filter(|text| {
                    (1..=64).contains(&text.len())
                        && text.bytes().all(|byte| {
                            byte.is_ascii_alphanumeric()
                                || matches!(byte, b'_' | b'-' | b'.' | b':')
                        })
                })
                .map(Value::from),
            Prop::Code => value
                .as_str()
                .filter(|text| valid_code(text))
                .map(Value::from),
            Prop::Number(maximum) => value
                .as_f64()
                .filter(|number| number.is_finite() && (0.0..=*maximum).contains(number))
                .map(|number| (number * 100.0).round() / 100.0)
                .map(|number| {
                    if number.fract() == 0.0 && number < 9_007_199_254_740_992.0 {
                        json!(number as u64)
                    } else {
                        json!(number)
                    }
                }),
            Prop::Bool => value.as_bool().map(Value::from),
            Prop::Choice(choices) => value
                .as_str()
                .filter(|text| choices.contains(text))
                .map(Value::from),
            Prop::Resolution => value
                .as_str()
                .filter(|text| {
                    text.split_once('x').is_some_and(|(width, height)| {
                        [width, height].iter().all(|part| {
                            (1..=5).contains(&part.len())
                                && part.bytes().all(|b| b.is_ascii_digit())
                        })
                    })
                })
                .map(Value::from),
        };
        if let Some(accepted) = accepted {
            clean.insert((*key).to_owned(), accepted);
        }
    }
    clean
}

pub fn valid_code(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

pub fn gpu_vendor(name: Option<&str>) -> &'static str {
    let Some(name) = name.map(str::to_ascii_lowercase) else {
        return "unknown";
    };
    [
        ("nvidia", &["nvidia", "geforce", "quadro", "rtx", "gtx"][..]),
        ("amd", &["amd", "radeon", "ati "]),
        ("intel", &["intel", "iris", "arc "]),
        ("apple", &["apple"]),
        ("qualcomm", &["qualcomm", "adreno"]),
    ]
    .iter()
    .find(|(_, needles)| needles.iter().any(|needle| name.contains(needle)))
    .map_or("other", |(vendor, _)| vendor)
}

fn bounded_text(value: &str, limit: usize) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .take(limit)
        .collect::<String>()
        .trim()
        .to_owned()
}

struct Queued {
    install_id: String,
    account: Value,
    event: Value,
}

struct State {
    queue: VecDeque<Queued>,
    dropped: u64,
    generation: u64,
    ui_surface: &'static str,
    signed_in_user: Option<String>,
    app_opened: bool,
    device: Value,
    shutdown: bool,
}

pub struct AnalyticsService {
    client: Client,
    endpoint: String,
    spool_path: PathBuf,
    state: Mutex<State>,
    wake: Condvar,
    spool_lock: Mutex<()>,
}

impl AnalyticsService {
    pub fn new(base: &str, data_dir: &Path) -> Result<Self, String> {
        Ok(Self {
            client: Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(20))
                .build()
                .map_err(|error| error.to_string())?,
            endpoint: format!("{base}/v1/events"),
            spool_path: data_dir.join("analytics").join("spool.jsonl"),
            state: Mutex::new(State {
                queue: VecDeque::new(),
                dropped: 0,
                generation: 0,
                ui_surface: "desktop",
                signed_in_user: None,
                app_opened: false,
                device: json!({"gpu":null, "decoderBackend":null}),
                shutdown: false,
            }),
            wake: Condvar::new(),
            spool_lock: Mutex::new(()),
        })
    }

    pub fn start(self: &Arc<Self>) -> Result<mpsc::Receiver<()>, String> {
        let (done, finished) = mpsc::channel();
        let service = Arc::clone(self);
        thread::Builder::new()
            .name("opennow-analytics".to_owned())
            .spawn(move || {
                service.run();
                let _ = done.send(());
            })
            .map_err(|error| error.to_string())?;
        Ok(finished)
    }

    pub fn shutdown(&self, finished: &mpsc::Receiver<()>) {
        self.state.lock().expect("analytics poisoned").shutdown = true;
        self.wake.notify_all();
        let _ = finished.recv_timeout(SHUTDOWN_BUDGET + Duration::from_millis(200));
    }

    pub fn enqueue(
        &self,
        install_id: &str,
        account: Value,
        event: &str,
        props: Map<String, Value>,
    ) -> u64 {
        let mut state = self.state.lock().expect("analytics poisoned");
        let mut props = props;
        props.insert(
            "app_version".to_owned(),
            json!(crate::version::APPLICATION_VERSION),
        );
        props.insert("os".to_owned(), json!(std::env::consts::OS));
        props.insert("arch".to_owned(), json!(std::env::consts::ARCH));
        props.insert("ui_surface".to_owned(), json!(state.ui_surface));
        props.insert(
            "channel".to_owned(),
            json!(crate::version::update_channel(
                crate::version::APPLICATION_VERSION
            )),
        );
        if state.queue.len() >= MAX_QUEUED_EVENTS {
            state.queue.pop_front();
            state.dropped += 1;
        }
        state.queue.push_back(Queued {
            install_id: install_id.to_owned(),
            account,
            event: json!({"event":event, "ts":now_ms(), "props":props}),
        });
        if state.queue.len() >= FLUSH_THRESHOLD {
            self.wake.notify_all();
        }
        state.dropped
    }

    pub fn wipe(&self) {
        let _spool = self.spool_lock.lock().expect("analytics spool poisoned");
        let mut state = self.state.lock().expect("analytics poisoned");
        state.queue.clear();
        state.generation += 1;
        drop(state);
        let _ = fs::remove_file(&self.spool_path);
    }

    pub fn set_ui_surface(&self, surface: &'static str) {
        self.state.lock().expect("analytics poisoned").ui_surface = surface;
    }

    pub fn observe_sign_in(&self, user_id: &str) -> bool {
        let mut state = self.state.lock().expect("analytics poisoned");
        if state.signed_in_user.as_deref() == Some(user_id) {
            return false;
        }
        state.signed_in_user = Some(user_id.to_owned());
        true
    }

    pub fn observe_sign_out(&self) {
        self.state
            .lock()
            .expect("analytics poisoned")
            .signed_in_user = None;
    }

    pub fn observe_runtime(&self, capabilities: &Value) {
        let gpu = crate::streamer::active_gpu_label(capabilities);
        let backend = capabilities["videoBackends"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|backend| backend["available"].as_bool() == Some(true))
            .find_map(|backend| backend["backend"].as_str())
            .filter(|backend| valid_code(backend));
        if gpu.is_some() || backend.is_some() {
            self.state.lock().expect("analytics poisoned").device =
                json!({"gpu":gpu, "decoderBackend":backend});
        }
    }

    pub fn open_app(&self, capabilities: &Value) -> Option<Map<String, Value>> {
        self.observe_runtime(capabilities);
        let mut state = self.state.lock().expect("analytics poisoned");
        if std::mem::replace(&mut state.app_opened, true) {
            return None;
        }
        let mut props = Map::new();
        props.insert(
            "gpu_vendor".to_owned(),
            json!(gpu_vendor(state.device["gpu"].as_str())),
        );
        if let Some(kind) = package_kind() {
            props.insert("package_kind".to_owned(), json!(kind));
        }
        Some(props)
    }

    pub fn device(&self) -> Value {
        self.state
            .lock()
            .expect("analytics poisoned")
            .device
            .clone()
    }

    fn run(&self) {
        let generation = self.state.lock().expect("analytics poisoned").generation;
        self.deliver(Vec::new(), generation, None);
        loop {
            let (batches, generation, deadline) = {
                let mut state = self.state.lock().expect("analytics poisoned");
                let wake_at = Instant::now() + FLUSH_INTERVAL;
                while !state.shutdown && state.queue.len() < FLUSH_THRESHOLD {
                    let now = Instant::now();
                    if now >= wake_at {
                        break;
                    }
                    state = self
                        .wake
                        .wait_timeout(state, wake_at - now)
                        .expect("analytics poisoned")
                        .0;
                }
                let deadline = state.shutdown.then(|| Instant::now() + SHUTDOWN_BUDGET);
                (batches(state.queue.drain(..)), state.generation, deadline)
            };
            if !batches.is_empty() || deadline.is_none() {
                self.deliver(batches, generation, deadline);
            }
            if deadline.is_some() {
                return;
            }
        }
    }

    fn deliver(&self, batches: Vec<Value>, generation: u64, deadline: Option<Instant>) {
        let mut failed = Vec::new();
        for batch in batches {
            if !failed.is_empty() || self.post(&batch, deadline) == Delivery::Retry {
                failed.push(batch);
            }
        }
        if !failed.is_empty() {
            self.append_spool(&failed, generation);
        } else if deadline.is_none() {
            self.resend_spool(generation);
        }
    }

    fn post(&self, batch: &Value, deadline: Option<Instant>) -> Delivery {
        let mut request = self.client.post(&self.endpoint).json(batch);
        if let Some(deadline) = deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Delivery::Retry;
            }
            request = request.timeout(remaining);
        }
        match request.send() {
            Ok(response) if response.status().is_success() => Delivery::Sent,
            Ok(response)
                if response.status().is_client_error()
                    && !matches!(response.status().as_u16(), 408 | 429) =>
            {
                Delivery::Rejected
            }
            _ => Delivery::Retry,
        }
    }

    fn append_spool(&self, batches: &[Value], generation: u64) {
        let _spool = self.spool_lock.lock().expect("analytics spool poisoned");
        if self.state.lock().expect("analytics poisoned").generation != generation {
            return;
        }
        let mut lines = read_lines(&self.spool_path);
        lines.extend(batches.iter().map(Value::to_string));
        self.write_spool(lines);
    }

    fn resend_spool(&self, generation: u64) {
        let lines = {
            let _spool = self.spool_lock.lock().expect("analytics spool poisoned");
            read_lines(&self.spool_path)
        };
        if lines.is_empty() {
            return;
        }
        let mut sent = 0;
        for line in &lines {
            let Ok(batch) = serde_json::from_str::<Value>(line) else {
                sent += 1;
                continue;
            };
            if self.post(&batch, None) == Delivery::Retry {
                break;
            }
            sent += 1;
        }
        let _spool = self.spool_lock.lock().expect("analytics spool poisoned");
        if self.state.lock().expect("analytics poisoned").generation != generation {
            return;
        }
        let mut current = read_lines(&self.spool_path);
        current.drain(..sent.min(current.len()));
        self.write_spool(current);
    }

    fn write_spool(&self, mut lines: Vec<String>) {
        let mut total = lines.iter().map(|line| line.len() + 1).sum::<usize>();
        let mut skip = 0;
        while total > MAX_SPOOL_BYTES && skip < lines.len() {
            total -= lines[skip].len() + 1;
            skip += 1;
        }
        lines.drain(..skip);
        if lines.is_empty() {
            let _ = fs::remove_file(&self.spool_path);
            return;
        }
        let Some(parent) = self.spool_path.parent() else {
            return;
        };
        let temporary = self.spool_path.with_extension("jsonl.tmp");
        let mut text = lines.join("\n");
        text.push('\n');
        if fs::create_dir_all(parent).is_ok() && fs::write(&temporary, text).is_ok() {
            let _ = fs::rename(&temporary, &self.spool_path);
        }
    }
}

#[derive(PartialEq, Eq)]
enum Delivery {
    Sent,
    Rejected,
    Retry,
}

fn batches(queue: impl Iterator<Item = Queued>) -> Vec<Value> {
    let mut batches: Vec<(String, Value, Vec<Value>)> = Vec::new();
    for item in queue {
        match batches.last_mut() {
            Some((install_id, account, events))
                if *install_id == item.install_id
                    && *account == item.account
                    && events.len() < MAX_BATCH_EVENTS =>
            {
                events.push(item.event)
            }
            _ => batches.push((item.install_id, item.account, vec![item.event])),
        }
    }
    batches
        .into_iter()
        .map(|(install_id, account, events)| {
            json!({"installId":install_id, "account":account, "events":events})
        })
        .collect()
}

fn read_lines(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .map(|text| {
            text.lines()
                .filter(|line| !line.trim().is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn package_kind() -> Option<&'static str> {
    if opennow_core::update_apply::external_update_message().is_some() {
        return Some("flatpak");
    }
    opennow_core::update_apply::compatible_package_extension().ok()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;

    const INSTALL: &str = "0123456789abcdef0123456789abcdef";

    fn service(base: &str) -> (tempfile::TempDir, AnalyticsService) {
        let directory = tempfile::tempdir().unwrap();
        let service = AnalyticsService::new(base, directory.path()).unwrap();
        (directory, service)
    }

    fn server(statuses: Vec<u16>) -> (String, thread::JoinHandle<Vec<Value>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let worker = thread::spawn(move || {
            let mut bodies = Vec::new();
            for status in statuses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(&stream);
                let mut length = 0;
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                assert_eq!(line.trim(), "POST /v1/events HTTP/1.1");
                loop {
                    line.clear();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap();
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                bodies.push(serde_json::from_slice(&body).unwrap());
                write!(stream, "HTTP/1.1 {status} Fixture\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}").unwrap();
            }
            bodies
        });
        (base, worker)
    }

    fn drain(service: &AnalyticsService) -> (Vec<Value>, u64) {
        let mut state = service.state.lock().unwrap();
        (batches(state.queue.drain(..)), state.generation)
    }

    #[test]
    fn shell_events_are_allowlisted_and_validated() {
        let event = shell_event(&json!({"event":"session_ended", "uiSurface":"console", "props":{
            "game_id":"100", "game_title":"Portal\u{0007} 2", "duration_s":61.256, "outcome":"clean",
            "error_code":"Bad Code", "avg_fps":-1, "avg_ping_ms":23.4, "token":"secret", "recoveries":0.0,
            "avg_packet_loss_pct":140
        }}))
        .unwrap();
        assert_eq!(event.name, "session_ended");
        assert_eq!(
            ui_surface(&json!({"uiSurface":"console"})),
            Ok(Some("console"))
        );
        assert!(ui_surface(&json!({"uiSurface":"web"})).is_err());
        assert_eq!(
            Value::Object(event.props),
            json!({"game_id":"100", "game_title":"Portal  2", "duration_s":61.26,
                "outcome":"clean", "avg_ping_ms":23.4, "recoveries":0})
        );
        let started = shell_event(&json!({"event":"session_started", "props":{
            "resolution":"1920x1080", "codec":"h265", "hdr":true}}))
        .unwrap();
        assert_eq!(
            Value::Object(started.props),
            json!({"codec":"h265", "resolution":"1920x1080", "hdr":true})
        );
        assert!(
            shell_event(&json!({"event":"session_started", "props":{"resolution":"1920*1080"}}))
                .unwrap()
                .props
                .is_empty()
        );
        for rejected in [
            json!({"event":"app_opened"}),
            json!({"event":"consent_changed"}),
            json!({"event":"purchase"}),
        ] {
            assert!(shell_event(&rejected).is_err());
        }
        assert_eq!(
            sanitize(
                "bug_report_sent",
                &json!({"report_id":"br-7F3A21", "kind":"manual",
                "code":"manual", "automatic":false, "issue_id":"iss_01J"})
            ),
            json!({"report_id":"br-7F3A21", "issue_id":"iss_01J", "kind":"manual",
                "code":"manual", "automatic":false})
            .as_object()
            .unwrap()
            .clone()
        );
    }

    #[test]
    fn gpu_vendors_are_coarse() {
        assert_eq!(gpu_vendor(Some("NVIDIA GeForce RTX 4070")), "nvidia");
        assert_eq!(gpu_vendor(Some("AMD Radeon RX 7900")), "amd");
        assert_eq!(gpu_vendor(Some("Intel(R) Iris(R) Xe Graphics")), "intel");
        assert_eq!(gpu_vendor(Some("Virtual Display")), "other");
        assert_eq!(gpu_vendor(None), "unknown");
    }

    #[test]
    fn the_queue_is_bounded_and_drops_the_oldest_events() {
        let (_directory, service) = service("http://127.0.0.1:9");
        for index in 0..MAX_QUEUED_EVENTS + 3 {
            let mut props = Map::new();
            props.insert("code".to_owned(), json!(format!("e{index}")));
            service.enqueue(INSTALL, Value::Null, "library_load_failed", props);
        }
        let state = service.state.lock().unwrap();
        assert_eq!(state.queue.len(), MAX_QUEUED_EVENTS);
        assert_eq!(state.dropped, 3);
        assert_eq!(state.queue[0].event["props"]["code"], "e3");
        assert_eq!(state.queue[0].event["props"]["ui_surface"], "desktop");
    }

    #[test]
    fn batches_group_by_account_and_cap_at_one_hundred_events() {
        let (_directory, service) = service("http://127.0.0.1:9");
        let account = json!({"userId":"sub", "providerIdpId":"idp"});
        for _ in 0..150 {
            service.enqueue(INSTALL, account.clone(), "library_load_failed", Map::new());
        }
        service.enqueue(INSTALL, Value::Null, "consent_changed", Map::new());
        let (batches, _) = drain(&service);
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0]["events"].as_array().unwrap().len(), 100);
        assert_eq!(batches[1]["events"].as_array().unwrap().len(), 50);
        assert_eq!(batches[2]["account"], Value::Null);
        assert_eq!(batches[2]["installId"], INSTALL);
        assert_eq!(batches[2]["events"][0]["event"], "consent_changed");
        assert!(batches[2]["events"][0]["ts"].as_u64().unwrap() > 0);
    }

    #[test]
    fn failed_batches_spool_and_resend_once_the_service_recovers() {
        let (base, worker) = server(vec![503, 202, 202]);
        let (_directory, service) = service(&base);
        service.enqueue(INSTALL, Value::Null, "library_load_failed", Map::new());
        let (batches, generation) = drain(&service);
        service.deliver(batches, generation, None);
        assert_eq!(read_lines(&service.spool_path).len(), 1);
        service.enqueue(INSTALL, Value::Null, "app_opened", Map::new());
        let (batches, generation) = drain(&service);
        service.deliver(batches, generation, None);
        assert!(!service.spool_path.exists());
        let bodies = worker.join().unwrap();
        assert_eq!(bodies[0]["events"][0]["event"], "library_load_failed");
        assert_eq!(bodies[1]["events"][0]["event"], "app_opened");
        assert_eq!(bodies[2], bodies[0]);
    }

    #[test]
    fn the_spool_is_capped_at_one_megabyte_keeping_the_newest_batches() {
        let (_directory, service) = service("http://127.0.0.1:9");
        let line = "x".repeat(300 * 1024);
        service.write_spool((0..5).map(|index| format!("{index}{line}")).collect());
        let lines = read_lines(&service.spool_path);
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with('2'));
        assert!(fs::metadata(&service.spool_path).unwrap().len() <= MAX_SPOOL_BYTES as u64);
    }

    #[test]
    fn withdrawing_consent_wipes_the_queue_spool_and_in_flight_batches() {
        let (_directory, service) = service("http://127.0.0.1:9");
        service.enqueue(INSTALL, Value::Null, "library_load_failed", Map::new());
        service.write_spool(vec![json!({"events":[]}).to_string()]);
        let (batches, generation) = drain(&service);
        service.enqueue(INSTALL, Value::Null, "library_load_failed", Map::new());
        service.wipe();
        assert!(service.state.lock().unwrap().queue.is_empty());
        assert!(!service.spool_path.exists());
        service.append_spool(&batches, generation);
        assert!(!service.spool_path.exists());
    }

    #[test]
    fn runtime_capabilities_open_the_app_once_and_describe_the_device() {
        let (_directory, service) = service("http://127.0.0.1:9");
        let capabilities = json!({
            "graphicsAdapters":[{"name":"NVIDIA GeForce RTX 4070", "active":true}],
            "videoBackends":[{"backend":"software","available":false},{"backend":"d3d11","available":true}]
        });
        let opened = service.open_app(&capabilities).unwrap();
        assert_eq!(opened["gpu_vendor"], "nvidia");
        assert!(service.open_app(&capabilities).is_none());
        assert_eq!(
            service.device(),
            json!({"gpu":"NVIDIA GeForce RTX 4070", "decoderBackend":"d3d11"})
        );
        assert!(service.observe_sign_in("sub"));
        assert!(!service.observe_sign_in("sub"));
        service.observe_sign_out();
        assert!(service.observe_sign_in("sub"));
    }
}
