use rand::RngCore;
use serde_json::{Value, json};
use std::io;
#[cfg(unix)]
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

const CLIENT_ID: &str = "1479944467112001669";
const MAX_FRAME_BYTES: usize = 1024 * 1024;
const MAX_READ_CHUNK: usize = 64 * 1024;
const MAX_WAITERS: usize = 8;
const OPCODE_HANDSHAKE: u32 = 0;
const OPCODE_FRAME: u32 = 1;
const OPCODE_CLOSE: u32 = 2;
const OPCODE_PING: u32 = 3;
const OPCODE_PONG: u32 = 4;
const REPLY_TIMEOUT: Duration = Duration::from_secs(2);
const SERVICE_TICK: Duration = Duration::from_millis(500);
const SERVICE_FRAME_BUDGET: usize = 32;
const READ_SLICE: Duration = Duration::from_millis(100);
const ACKNOWLEDGE_TIMEOUT: Duration = Duration::from_millis(1000);
const WRITE_DEADLINE: Duration = Duration::from_secs(3);
#[cfg(windows)]
const CANCEL_REAP_TIMEOUT: Duration = Duration::from_secs(2);
const RETRY_SCHEDULE: [Duration; 7] = [
    Duration::from_millis(250),
    Duration::from_millis(500),
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
    Duration::from_secs(8),
    Duration::from_secs(15),
];
const UNAVAILABLE: &str = "Discord presence worker is unavailable";
const WORKER_STOPPED: &str = "Discord presence worker stopped";

pub struct DiscordService {
    directories: Vec<PathBuf>,
    mailbox: OnceLock<Option<Arc<Mailbox>>>,
    worker_alive: Arc<AtomicBool>,
    abandoned: Arc<AtomicBool>,
    #[cfg(test)]
    iterations: Arc<std::sync::atomic::AtomicUsize>,
}

impl DiscordService {
    pub fn new() -> Self {
        Self::with_directories(candidate_directories())
    }

    fn with_directories(directories: Vec<PathBuf>) -> Self {
        Self {
            directories,
            mailbox: OnceLock::new(),
            worker_alive: Arc::new(AtomicBool::new(true)),
            abandoned: Arc::new(AtomicBool::new(false)),
            #[cfg(test)]
            iterations: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    pub fn sync(&self, params: &Value) -> Result<Value, String> {
        if params["enabled"].as_bool() != Some(true) {
            return self.clear();
        }
        let activity = activity_payload(params)?;
        let signature = serde_json::to_string(&activity).map_err(|error| error.to_string())?;
        self.request(Intent::Set(Desired {
            activity,
            signature,
        }))
    }

    pub fn clear(&self) -> Result<Value, String> {
        self.request(Intent::Clear)
    }

    fn request(&self, intent: Intent) -> Result<Value, String> {
        let Some(mailbox) = self.mailbox() else {
            return Err(UNAVAILABLE.to_owned());
        };
        if !self.worker_alive.load(Ordering::SeqCst) {
            return Err(UNAVAILABLE.to_owned());
        }
        if self.abandoned.load(Ordering::SeqCst) {
            return Ok(terminal_status());
        }
        let (ack, response) = mpsc::channel();
        {
            let mut state = mailbox.state();
            match intent {
                Intent::Set(desired) => state.desired = Some(desired),
                Intent::Clear => state.desired = None,
            }
            if state.waiters.len() >= MAX_WAITERS {
                state.waiters.remove(0);
            }
            state.waiters.push(ack);
        }
        let _ = mailbox.wake.try_send(());
        match response.recv_timeout(ACKNOWLEDGE_TIMEOUT) {
            Ok(status) => Ok(status),
            Err(_) => Ok(pending_status()),
        }
    }

    fn mailbox(&self) -> &Option<Arc<Mailbox>> {
        self.mailbox.get_or_init(|| {
            let (wake, woken) = mpsc::sync_channel(1);
            let state = Arc::new(Mutex::new(MailState::default()));
            let mailbox = Arc::new(Mailbox {
                state: Arc::clone(&state),
                wake,
            });
            let directories = self.directories.clone();
            let alive = Arc::clone(&self.worker_alive);
            let abandoned = Arc::clone(&self.abandoned);
            #[cfg(test)]
            let iterations = Arc::clone(&self.iterations);
            match thread::Builder::new()
                .name("opennow-discord".to_owned())
                .spawn(move || {
                    let _guard = WorkerGuard(alive);
                    #[cfg(test)]
                    Worker::new(directories, state, woken, abandoned, iterations).run();
                    #[cfg(not(test))]
                    Worker::new(directories, state, woken, abandoned).run();
                }) {
                Ok(_) => Some(mailbox),
                Err(error) => {
                    eprintln!("opennow-core: discord presence worker failed to start: {error}");
                    self.worker_alive.store(false, Ordering::SeqCst);
                    None
                }
            }
        })
    }
}

struct WorkerGuard(Arc<AtomicBool>);

impl Drop for WorkerGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

enum Intent {
    Set(Desired),
    Clear,
}

#[derive(Clone)]
struct Desired {
    activity: Value,
    signature: String,
}

struct Mailbox {
    state: Arc<Mutex<MailState>>,
    wake: SyncSender<()>,
}

#[derive(Default)]
struct MailState {
    desired: Option<Desired>,
    waiters: Vec<Sender<Value>>,
}

impl Mailbox {
    fn state(&self) -> MutexGuard<'_, MailState> {
        mail_state(&self.state)
    }

    #[cfg(test)]
    fn desired(&self) -> Option<Desired> {
        self.state().desired.clone()
    }
}

fn mail_state(state: &Arc<Mutex<MailState>>) -> MutexGuard<'_, MailState> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Debug)]
enum Failure {
    NotRunning,
    TimedOut,
    HandshakeRejected(String),
    Rejected(String),
    Closed(String),
    Corrupt(String),
    Terminal,
}

impl Failure {
    fn status(&self) -> Value {
        match self {
            Self::NotRunning => json!({
                "connected":false,"message":"Discord is not running","pending":true
            }),
            Self::TimedOut => pending_status(),
            Self::HandshakeRejected(message) => {
                json!({"connected":false,"applied":false,"pending":true,"message":message})
            }
            Self::Rejected(message) => {
                json!({"connected":true,"applied":false,"pending":true,"message":message})
            }
            Self::Closed(message) | Self::Corrupt(message) => {
                json!({"connected":false,"pending":true,"message":message})
            }
            Self::Terminal => terminal_status(),
        }
    }
}

fn terminal_status() -> Value {
    json!({
        "connected":false,"applied":false,"terminal":true,"pending":false,
        "message":"Discord IPC was disabled after the transport could not cancel outstanding I/O"
    })
}

fn pending_status() -> Value {
    json!({"connected":false,"timedOut":true,"pending":true})
}

enum ServiceOutcome {
    Continue,
    Shutdown,
}

struct Snapshot {
    desired: Option<Desired>,
    waiters: Vec<Sender<Value>>,
}

struct Worker {
    directories: Vec<PathBuf>,
    state: Arc<Mutex<MailState>>,
    wake: Receiver<()>,
    abandoned: Arc<AtomicBool>,
    connection: Option<Connection>,
    retry_at: Option<Instant>,
    retry_index: usize,
    #[cfg(test)]
    iterations: Arc<std::sync::atomic::AtomicUsize>,
}

impl Worker {
    fn new(
        directories: Vec<PathBuf>,
        state: Arc<Mutex<MailState>>,
        wake: Receiver<()>,
        abandoned: Arc<AtomicBool>,
        #[cfg(test)] iterations: Arc<std::sync::atomic::AtomicUsize>,
    ) -> Self {
        Self {
            directories,
            state,
            wake,
            abandoned,
            connection: None,
            retry_at: None,
            retry_index: 0,
            #[cfg(test)]
            iterations,
        }
    }

    fn run(mut self) {
        loop {
            #[cfg(test)]
            self.iterations.fetch_add(1, Ordering::Relaxed);
            let tick = Instant::now() + SERVICE_TICK;
            let snapshot = self.snapshot();
            if !snapshot.waiters.is_empty() || self.retry_due(snapshot.desired.is_some()) {
                let status = self.converge(snapshot.desired);
                for waiter in snapshot.waiters {
                    let _ = waiter.send(status.clone());
                }
            }
            if self.abandoned.load(Ordering::SeqCst) {
                self.connection = None;
                self.terminate(Failure::Terminal);
            }
            let outcome = self.service_connection(tick);
            let desired_pending = mail_state(&self.state).desired.is_some();
            if matches!(outcome, ServiceOutcome::Shutdown) || !self.wait(tick, desired_pending) {
                break;
            }
        }
        self.shutdown();
    }

    fn snapshot(&self) -> Snapshot {
        let mut state = mail_state(&self.state);
        Snapshot {
            desired: state.desired.clone(),
            waiters: std::mem::take(&mut state.waiters),
        }
    }

    fn shutdown(&mut self) {
        self.connection = None;
        let message = json!({"connected":false,"pending":true,"message":WORKER_STOPPED});
        for waiter in std::mem::take(&mut mail_state(&self.state).waiters) {
            let _ = waiter.send(message.clone());
        }
    }

    fn converged(&self, desired: &Desired) -> bool {
        self.connection
            .as_ref()
            .and_then(|connection| connection.applied.as_deref())
            == Some(desired.signature.as_str())
    }

    fn converge(&mut self, desired: Option<Desired>) -> Value {
        let Some(desired) = desired else {
            return self.release();
        };
        if self.converged(&desired) {
            return json!({"connected":true,"unchanged":true});
        }
        if self.abandoned.load(Ordering::SeqCst) {
            return Failure::Terminal.status();
        }
        if self.connection.is_none() {
            match Connection::connect(&self.directories, &self.abandoned) {
                Ok(connection) => self.connection = Some(connection),
                Err(failure) => {
                    if matches!(failure, Failure::Terminal) {
                        return self.terminate(failure);
                    }
                    self.schedule_retry();
                    return failure.status();
                }
            }
        }
        let outcome = match self.connection.as_mut() {
            Some(connection) => connection.set_activity(&desired.activity),
            None => return Failure::NotRunning.status(),
        };
        if outcome.is_err() && self.abandoned.load(Ordering::SeqCst) {
            self.connection = None;
            return self.terminate(Failure::Terminal);
        }
        match outcome {
            Ok(()) => {
                if let Some(connection) = self.connection.as_mut() {
                    connection.applied = Some(desired.signature);
                }
                self.retry_at = None;
                self.retry_index = 0;
                json!({"connected":true,"unchanged":false})
            }
            Err(failure) => {
                self.connection = None;
                if matches!(failure, Failure::Terminal) {
                    return self.terminate(failure);
                }
                self.schedule_retry();
                failure.status()
            }
        }
    }

    fn terminate(&mut self, failure: Failure) -> Value {
        self.abandoned.store(true, Ordering::SeqCst);
        self.retry_at = None;
        self.retry_index = 0;
        let mut state = mail_state(&self.state);
        state.desired = None;
        failure.status()
    }

    fn release(&mut self) -> Value {
        let Some(mut connection) = self.connection.take() else {
            return json!({"connected":false,"cleared":true});
        };
        if connection.applied.is_some() {
            let _ = connection.set_activity(&Value::Null);
        }
        drop(connection);
        self.retry_at = None;
        self.retry_index = 0;
        json!({"connected":true,"cleared":true})
    }

    fn retry_due(&self, desired_pending: bool) -> bool {
        if self.connection.is_some() || !desired_pending || self.abandoned.load(Ordering::SeqCst) {
            return false;
        }
        match self.retry_at {
            Some(at) => Instant::now() >= at,
            None => true,
        }
    }

    fn schedule_retry(&mut self) {
        let delay = RETRY_SCHEDULE[self.retry_index.min(RETRY_SCHEDULE.len() - 1)];
        self.retry_index = self.retry_index.saturating_add(1);
        self.retry_at = Some(Instant::now() + delay);
    }

    fn service_connection(&mut self, tick: Instant) -> ServiceOutcome {
        let Some(mut connection) = self.connection.take() else {
            return ServiceOutcome::Continue;
        };
        let mut failure = None;
        let mut shutdown = false;
        for _ in 0..SERVICE_FRAME_BUDGET {
            if Instant::now() >= tick {
                break;
            }
            match self.wake.try_recv() {
                Ok(()) => break,
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    shutdown = true;
                    break;
                }
            }
            let slice = (Instant::now() + READ_SLICE).min(tick);
            match connection.poll(slice) {
                Ok(Some((opcode, value))) => match opcode {
                    OPCODE_FRAME => {}
                    OPCODE_PING => {
                        if let Err(error) = connection.send(OPCODE_PONG, &value) {
                            failure = Some(error);
                            break;
                        }
                    }
                    OPCODE_PONG => {}
                    OPCODE_CLOSE => {
                        failure = Some(close_failure(&value));
                        break;
                    }
                    _ => {
                        failure = Some(Failure::Corrupt(
                            "Discord sent an unexpected RPC opcode".to_owned(),
                        ));
                        break;
                    }
                },
                Ok(None) => break,
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }
        if failure.is_none() {
            self.connection = Some(connection);
        } else if mail_state(&self.state).desired.is_some() {
            self.schedule_retry();
        }
        if shutdown {
            ServiceOutcome::Shutdown
        } else {
            ServiceOutcome::Continue
        }
    }

    fn wait(&mut self, tick: Instant, desired_pending: bool) -> bool {
        let bound = if self.connection.is_some() {
            Some(tick.saturating_duration_since(Instant::now()))
        } else if desired_pending && !self.abandoned.load(Ordering::SeqCst) {
            Some(
                self.retry_at
                    .unwrap_or_else(Instant::now)
                    .saturating_duration_since(Instant::now()),
            )
        } else {
            None
        };
        match bound {
            Some(bound) => match self.wake.recv_timeout(bound) {
                Ok(()) | Err(RecvTimeoutError::Timeout) => true,
                Err(RecvTimeoutError::Disconnected) => false,
            },
            None => self.wake.recv().is_ok(),
        }
    }
}

struct Connection {
    stream: IpcStream,
    scratch: Vec<u8>,
    buffered: Vec<u8>,
    applied: Option<String>,
}

impl Connection {
    fn connect(directories: &[PathBuf], abandoned: &Arc<AtomicBool>) -> Result<Self, Failure> {
        let stream = connect(directories, abandoned).map_err(|_| Failure::NotRunning)?;
        let mut connection = Self {
            stream,
            scratch: Vec::new(),
            buffered: Vec::new(),
            applied: None,
        };
        connection.send(OPCODE_HANDSHAKE, &json!({"v":1,"client_id":CLIENT_ID}))?;
        let deadline = Instant::now() + REPLY_TIMEOUT;
        loop {
            match connection.poll(deadline)? {
                Some((OPCODE_FRAME, value)) => match value["evt"].as_str() {
                    Some("READY") => return Ok(connection),
                    Some("ERROR") => {
                        return Err(Failure::HandshakeRejected(protocol_error(
                            &value,
                            "Discord rejected the RPC handshake",
                        )));
                    }
                    _ => {}
                },
                Some((OPCODE_PING, value)) => connection.send(OPCODE_PONG, &value)?,
                Some((OPCODE_PONG, _)) => {}
                Some((OPCODE_CLOSE, value)) => return Err(close_failure(&value)),
                Some(_) => {
                    return Err(Failure::Corrupt(
                        "Discord sent an unexpected RPC opcode".to_owned(),
                    ));
                }
                None => return Err(Failure::TimedOut),
            }
        }
    }

    fn set_activity(&mut self, activity: &Value) -> Result<(), Failure> {
        let activity = if activity.is_null() {
            Value::Null
        } else {
            activity.clone()
        };
        let mut nonce = [0_u8; 12];
        rand::rng().fill_bytes(&mut nonce);
        let nonce = nonce
            .iter()
            .map(|value| format!("{value:02x}"))
            .collect::<String>();
        self.send(
            OPCODE_FRAME,
            &json!({
                "cmd":"SET_ACTIVITY",
                "args":{"pid":std::process::id(),"activity":activity},
                "nonce":nonce
            }),
        )?;
        let deadline = Instant::now() + REPLY_TIMEOUT;
        loop {
            match self.poll(deadline)? {
                Some((OPCODE_FRAME, value)) => {
                    if value["evt"] == "ERROR" {
                        let error_nonce = value["nonce"].as_str();
                        if error_nonce.is_none() || error_nonce == Some(nonce.as_str()) {
                            return Err(Failure::Rejected(protocol_error(
                                &value,
                                "Discord rejected the activity update",
                            )));
                        }
                        continue;
                    }
                    if value["cmd"] == "SET_ACTIVITY"
                        && value["nonce"].as_str() == Some(nonce.as_str())
                    {
                        return Ok(());
                    }
                }
                Some((OPCODE_PING, value)) => self.send(OPCODE_PONG, &value)?,
                Some((OPCODE_PONG, _)) => {}
                Some((OPCODE_CLOSE, value)) => return Err(close_failure(&value)),
                Some(_) => {
                    return Err(Failure::Corrupt(
                        "Discord sent an unexpected RPC opcode".to_owned(),
                    ));
                }
                None => return Err(Failure::TimedOut),
            }
        }
    }

    fn send(&mut self, opcode: u32, value: &Value) -> Result<(), Failure> {
        let body =
            serde_json::to_vec(value).map_err(|error| Failure::Corrupt(error.to_string()))?;
        if body.len() > MAX_FRAME_BYTES {
            return Err(Failure::Corrupt(
                "Discord RPC frame is too large".to_owned(),
            ));
        }
        let mut frame = Vec::with_capacity(8 + body.len());
        frame.extend_from_slice(&opcode.to_le_bytes());
        frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
        frame.extend_from_slice(&body);
        write_frame(&mut self.stream, &frame, Instant::now() + WRITE_DEADLINE)
            .map_err(|error| Failure::Closed(error.to_string()))
    }

    fn poll(&mut self, deadline: Instant) -> Result<Option<(u32, Value)>, Failure> {
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            if let Some(frame) = take_frame(&mut self.buffered)? {
                return Ok(Some(frame));
            }
            match read_some(&mut self.stream, &mut self.scratch, remaining)
                .map_err(|error| Failure::Closed(error.to_string()))?
            {
                ReadOutcome::Data(count) => self.buffered.extend_from_slice(&self.scratch[..count]),
                ReadOutcome::TimedOut => {}
                ReadOutcome::Eof => return Err(Failure::Closed(CLOSED_BY_DISCORD.to_owned())),
            }
        }
    }
}

const CLOSED_BY_DISCORD: &str = "Discord closed the connection";

fn close_failure(value: &Value) -> Failure {
    let code = value["code"].as_u64().unwrap_or_default();
    let message = value["message"].as_str().unwrap_or_default();
    Failure::Closed(if message.is_empty() {
        format!("Discord closed the connection (code {code})")
    } else {
        format!("Discord closed the connection (code {code}: {message})")
    })
}

fn protocol_error(value: &Value, fallback: &str) -> String {
    let message = value["data"]["message"].as_str().unwrap_or(fallback);
    match value["data"]["code"].as_u64() {
        Some(code) => format!("{message} (code {code})"),
        None => message.to_owned(),
    }
}

fn take_frame(buffered: &mut Vec<u8>) -> Result<Option<(u32, Value)>, Failure> {
    if buffered.len() < 8 {
        return Ok(None);
    }
    let opcode = u32::from_le_bytes(
        buffered[..4]
            .try_into()
            .expect("four-byte Discord RPC opcode"),
    );
    let length = u32::from_le_bytes(
        buffered[4..8]
            .try_into()
            .expect("four-byte Discord RPC length"),
    ) as usize;
    if length > MAX_FRAME_BYTES {
        return Err(Failure::Corrupt(
            "Discord RPC response is too large".to_owned(),
        ));
    }
    if buffered.len() < 8 + length {
        return Ok(None);
    }
    let value: Value = serde_json::from_slice(&buffered[8..8 + length])
        .map_err(|_| Failure::Corrupt("Invalid Discord RPC response".to_owned()))?;
    buffered.drain(..8 + length);
    Ok(Some((opcode, value)))
}

fn activity_payload(params: &Value) -> Result<Value, String> {
    let game_name = params["gameName"]
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Discord activity requires a game name".to_owned())?;
    let kind = params["kind"].as_str().unwrap_or("streaming");
    let state = match kind {
        "queued" => params["queuePosition"]
            .as_u64()
            .filter(|value| *value > 0)
            .map(|value| format!("In queue (#{value})"))
            .unwrap_or_else(|| "In queue".to_owned()),
        "starting" => "Starting stream".to_owned(),
        "streaming" => "Streaming via OpenNOW".to_owned(),
        _ => return Err("Unsupported Discord activity kind".to_owned()),
    };
    let mut activity = json!({
        "details": bounded(game_name, 128),
        "state": state,
        "instance": false
    });
    if kind == "streaming"
        && let Some(timestamp) = params["startTimestampMs"].as_u64()
        && timestamp > 0
    {
        activity["timestamps"] = json!({"start":timestamp / 1000});
    }
    if let Some(image) = params["gameImageUrl"].as_str().filter(|value| {
        value.starts_with("https://") && value.len() <= 2_048 && !value.contains(['\r', '\n'])
    }) {
        activity["assets"] = json!({
            "large_image": image,
            "large_text": bounded(game_name, 128)
        });
    }
    Ok(activity)
}

fn bounded(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

fn candidate_directories() -> Vec<PathBuf> {
    #[cfg(unix)]
    {
        use std::env;
        let mut directories = Vec::new();
        for name in ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"] {
            if let Some(path) = env::var_os(name).map(PathBuf::from)
                && !path.as_os_str().is_empty()
                && !directories.contains(&path)
            {
                directories.push(path);
            }
        }
        let fallback = PathBuf::from("/tmp");
        if !directories.contains(&fallback) {
            directories.push(fallback);
        }
        directories
    }
    #[cfg(windows)]
    {
        Vec::new()
    }
}

#[cfg(unix)]
type IpcStream = std::os::unix::net::UnixStream;

#[cfg(unix)]
fn connect(directories: &[PathBuf], _abandoned: &Arc<AtomicBool>) -> io::Result<IpcStream> {
    for directory in directories {
        for index in 0..10 {
            let path = directory.join(format!("discord-ipc-{index}"));
            if let Ok(stream) = IpcStream::connect(path) {
                return Ok(stream);
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "Discord IPC unavailable",
    ))
}

#[cfg(unix)]
fn write_frame(stream: &mut IpcStream, frame: &[u8], deadline: Instant) -> io::Result<()> {
    let remaining = deadline
        .saturating_duration_since(Instant::now())
        .max(Duration::from_millis(1));
    stream.set_write_timeout(Some(remaining))?;
    stream.write_all(frame)
}

#[cfg(unix)]
fn read_some(
    stream: &mut IpcStream,
    scratch: &mut Vec<u8>,
    timeout: Duration,
) -> io::Result<ReadOutcome> {
    if scratch.len() < MAX_READ_CHUNK {
        scratch.resize(MAX_READ_CHUNK, 0);
    }
    stream.set_read_timeout(Some(timeout))?;
    match std::io::Read::read(stream, scratch) {
        Ok(0) => Ok(ReadOutcome::Eof),
        Ok(count) => Ok(ReadOutcome::Data(count)),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            ) =>
        {
            Ok(ReadOutcome::TimedOut)
        }
        Err(error) => Err(error),
    }
}

enum ReadOutcome {
    Data(usize),
    TimedOut,
    Eof,
}

#[cfg(windows)]
struct IpcStream {
    handle: windows_sys::Win32::Foundation::HANDLE,
    pending_read: Option<PendingIo>,
    pending_write: Option<PendingIo>,
    abandoned: Arc<AtomicBool>,
}

#[cfg(windows)]
struct PendingIo {
    overlapped: Box<windows_sys::Win32::System::IO::OVERLAPPED>,
    buffer: Vec<u8>,
    event: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
fn close_pending(pending: PendingIo) -> Vec<u8> {
    use windows_sys::Win32::Foundation::CloseHandle;

    let PendingIo {
        overlapped,
        buffer,
        event,
    } = pending;
    unsafe { CloseHandle(event) };
    drop(overlapped);
    buffer
}

#[cfg(windows)]
fn abandon_pending(pending: PendingIo) {
    std::mem::forget(pending);
}

#[cfg(windows)]
fn finish_overlapped(
    handle: windows_sys::Win32::Foundation::HANDLE,
    pending: &PendingIo,
) -> (bool, u32, Option<i32>) {
    use windows_sys::Win32::System::IO::GetOverlappedResult;

    let mut transferred = 0_u32;
    let success =
        unsafe { GetOverlappedResult(handle, &*pending.overlapped, &mut transferred, 0) } != 0;
    let raw_error = if success {
        None
    } else {
        io::Error::last_os_error().raw_os_error()
    };
    (success, transferred, raw_error)
}

#[cfg(windows)]
fn reap_cancelled(
    handle: windows_sys::Win32::Foundation::HANDLE,
    pending: PendingIo,
) -> Result<OverlappedStatus, PendingIo> {
    use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
    use windows_sys::Win32::System::Threading::WaitForSingleObject;

    let milliseconds = CANCEL_REAP_TIMEOUT.as_millis().min(u32::MAX as u128) as u32;
    if unsafe { WaitForSingleObject(pending.event, milliseconds) } != WAIT_OBJECT_0 {
        return Err(pending);
    }
    let (success, transferred, raw_error) = finish_overlapped(handle, &pending);
    close_pending(pending);
    Ok(classify_overlapped(success, transferred, raw_error))
}

#[cfg(any(windows, test))]
const ERROR_OPERATION_ABORTED_CODE: i32 = 995;

#[cfg(any(windows, test))]
#[derive(Debug, PartialEq)]
enum OverlappedStatus {
    Completed(usize),
    Aborted,
    Failed(i32),
}

#[cfg(any(windows, test))]
fn classify_overlapped(
    success: bool,
    transferred: u32,
    raw_error: Option<i32>,
) -> OverlappedStatus {
    if success {
        return OverlappedStatus::Completed(transferred as usize);
    }
    match raw_error {
        Some(code) if code == ERROR_OPERATION_ABORTED_CODE => OverlappedStatus::Aborted,
        Some(code) => OverlappedStatus::Failed(code),
        None => OverlappedStatus::Failed(0),
    }
}

#[cfg(windows)]
impl IpcStream {
    fn start(&mut self, read: bool, mut buffer: Vec<u8>) -> io::Result<Option<(Vec<u8>, usize)>> {
        use windows_sys::Win32::Foundation::ERROR_IO_PENDING;
        use windows_sys::Win32::Storage::FileSystem::{ReadFile, WriteFile};
        use windows_sys::Win32::System::IO::OVERLAPPED;
        use windows_sys::Win32::System::Threading::{CreateEventW, ResetEvent};

        let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        if event.is_null() {
            return Err(io::Error::last_os_error());
        }
        let mut overlapped: Box<OVERLAPPED> = Box::new(unsafe { std::mem::zeroed() });
        overlapped.hEvent = event;
        unsafe { ResetEvent(event) };

        let length = buffer.len() as u32;
        let mut transferred = 0_u32;
        let started = unsafe {
            let overlapped_ptr = &mut *overlapped;
            if read {
                ReadFile(
                    self.handle,
                    buffer.as_mut_ptr(),
                    length,
                    &mut transferred,
                    overlapped_ptr,
                )
            } else {
                WriteFile(
                    self.handle,
                    buffer.as_ptr(),
                    length,
                    &mut transferred,
                    overlapped_ptr,
                )
            }
        };
        if started != 0 {
            unsafe { windows_sys::Win32::Foundation::CloseHandle(event) };
            drop(overlapped);
            return Ok(Some((buffer, transferred as usize)));
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(ERROR_IO_PENDING as i32) {
            unsafe { windows_sys::Win32::Foundation::CloseHandle(event) };
            drop((overlapped, buffer));
            return Err(error);
        }
        let pending = PendingIo {
            overlapped,
            buffer,
            event,
        };
        if read {
            self.pending_read = Some(pending);
        } else {
            self.pending_write = Some(pending);
        }
        Ok(None)
    }

    fn take_pending(&mut self, read: bool) -> io::Result<PendingIo> {
        let pending = if read {
            self.pending_read.take()
        } else {
            self.pending_write.take()
        };
        pending.ok_or_else(|| io::Error::other("no pending Discord IPC operation"))
    }

    fn restore_pending(&mut self, read: bool, pending: PendingIo) {
        if read {
            self.pending_read = Some(pending);
        } else {
            self.pending_write = Some(pending);
        }
    }

    fn poll_pending(
        &mut self,
        read: bool,
        deadline: Instant,
    ) -> io::Result<Option<(Vec<u8>, usize)>> {
        use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
        use windows_sys::Win32::System::Threading::WaitForSingleObject;

        let pending = self.take_pending(read)?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        let milliseconds = remaining.as_millis().min(u32::MAX as u128) as u32;
        if unsafe { WaitForSingleObject(pending.event, milliseconds) } != WAIT_OBJECT_0 {
            self.restore_pending(read, pending);
            return Ok(None);
        }
        let (success, transferred, raw_error) = finish_overlapped(self.handle, &pending);
        match classify_overlapped(success, transferred, raw_error) {
            OverlappedStatus::Completed(count) => Ok(Some((close_pending(pending), count))),
            OverlappedStatus::Aborted => {
                close_pending(pending);
                Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Discord IPC operation was cancelled",
                ))
            }
            OverlappedStatus::Failed(code) => {
                close_pending(pending);
                Err(io::Error::from_raw_os_error(code))
            }
        }
    }

    fn cancel_pending(&mut self, read: bool) -> io::Result<OverlappedStatus> {
        use windows_sys::Win32::System::IO::CancelIoEx;

        let pending = self.take_pending(read)?;
        unsafe { CancelIoEx(self.handle, &*pending.overlapped) };
        match reap_cancelled(self.handle, pending) {
            Ok(status) => Ok(status),
            Err(pending) => {
                self.abandoned.store(true, Ordering::SeqCst);
                abandon_pending(pending);
                Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Discord IPC operation could not be cancelled",
                ))
            }
        }
    }
}

#[cfg(windows)]
impl Drop for IpcStream {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::IO::CancelIoEx;

        for pending in [self.pending_read.take(), self.pending_write.take()]
            .into_iter()
            .flatten()
        {
            unsafe { CancelIoEx(self.handle, &*pending.overlapped) };
            if let Err(pending) = reap_cancelled(self.handle, pending) {
                self.abandoned.store(true, Ordering::SeqCst);
                abandon_pending(pending);
            }
        }
        unsafe { CloseHandle(self.handle) };
    }
}

#[cfg(windows)]
fn connect(_directories: &[PathBuf], abandoned: &Arc<AtomicBool>) -> io::Result<IpcStream> {
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::IntoRawHandle;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OVERLAPPED;

    for index in 0..10 {
        let path = format!(r"\\?\pipe\discord-ipc-{index}");
        let opened = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(FILE_FLAG_OVERLAPPED)
            .open(path);
        let Ok(file) = opened else {
            continue;
        };
        let handle = file.into_raw_handle() as isize as _;
        if handle == INVALID_HANDLE_VALUE {
            continue;
        }
        return Ok(IpcStream {
            handle,
            pending_read: None,
            pending_write: None,
            abandoned: Arc::clone(abandoned),
        });
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "Discord IPC unavailable",
    ))
}

#[cfg(windows)]
fn deliver_read(scratch: &mut Vec<u8>, buffer: Vec<u8>, count: usize) -> io::Result<ReadOutcome> {
    if count == 0 {
        return Ok(ReadOutcome::Eof);
    }
    if scratch.len() < count {
        scratch.resize(count, 0);
    }
    scratch[..count].copy_from_slice(&buffer[..count]);
    Ok(ReadOutcome::Data(count))
}

#[cfg(windows)]
fn read_some(
    stream: &mut IpcStream,
    scratch: &mut Vec<u8>,
    timeout: Duration,
) -> io::Result<ReadOutcome> {
    use windows_sys::Win32::Foundation::{ERROR_BROKEN_PIPE, ERROR_NO_DATA};

    let deadline = Instant::now() + timeout;
    if stream.pending_read.is_none()
        && let Some((buffer, count)) = stream.start(true, vec![0_u8; MAX_READ_CHUNK])?
    {
        return deliver_read(scratch, buffer, count);
    }
    match stream.poll_pending(true, deadline) {
        Ok(Some((buffer, count))) => deliver_read(scratch, buffer, count),
        Ok(None) => Ok(ReadOutcome::TimedOut),
        Err(error) => match error.raw_os_error() {
            Some(code) if code == ERROR_BROKEN_PIPE as i32 || code == ERROR_NO_DATA as i32 => {
                Ok(ReadOutcome::Eof)
            }
            _ => Err(error),
        },
    }
}

#[cfg(windows)]
fn write_frame(stream: &mut IpcStream, frame: &[u8], deadline: Instant) -> io::Result<()> {
    let mut written = 0;
    while written < frame.len() {
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Discord IPC write timed out",
            ));
        }
        if stream.pending_write.is_none() {
            let chunk = frame[written..].to_vec();
            if let Some((_buffer, count)) = stream.start(false, chunk)? {
                if count == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "Discord IPC write made no progress",
                    ));
                }
                written += count;
                continue;
            }
        }
        match stream.poll_pending(false, deadline)? {
            Some((_buffer, count)) => {
                if count == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "Discord IPC write made no progress",
                    ));
                }
                written += count;
            }
            None => match stream.cancel_pending(false)? {
                OverlappedStatus::Completed(count) => {
                    written += count;
                }
                OverlappedStatus::Aborted => {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "Discord IPC write timed out",
                    ));
                }
                OverlappedStatus::Failed(code) => {
                    return Err(io::Error::from_raw_os_error(code));
                }
            },
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_payload_is_bounded_and_privacy_scoped() {
        let payload = activity_payload(&json!({
            "gameName":"Example",
            "kind":"queued",
            "queuePosition":14,
            "gameImageUrl":"http://unsafe.example/image"
        }))
        .unwrap();
        assert_eq!(payload["state"], "In queue (#14)");
        assert!(payload.get("assets").is_none());
        assert!(activity_payload(&json!({"gameName":"X","kind":"unknown"})).is_err());
    }

    #[test]
    fn streaming_activity_reports_second_timestamps() {
        let payload = activity_payload(&json!({
            "gameName":"Example",
            "kind":"streaming",
            "startTimestampMs":1_528_245_984_000_u64
        }))
        .unwrap();
        assert_eq!(payload["timestamps"]["start"], 1_528_245_984_u64);
        let without_start = activity_payload(&json!({
            "gameName":"Example",
            "kind":"streaming",
            "startTimestampMs":0
        }))
        .unwrap();
        assert!(without_start.get("timestamps").is_none());
    }

    fn frame_bytes(opcode: u32, value: &Value) -> Vec<u8> {
        let body = serde_json::to_vec(value).unwrap();
        let mut frame = Vec::new();
        frame.extend_from_slice(&opcode.to_le_bytes());
        frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
        frame.extend_from_slice(&body);
        frame
    }

    #[test]
    fn discord_frames_are_little_endian_and_bounded() {
        let frame = frame_bytes(OPCODE_FRAME, &json!({"evt":"READY"}));
        assert_eq!(
            u32::from_le_bytes(frame[..4].try_into().unwrap()),
            OPCODE_FRAME
        );
        let body = serde_json::to_vec(&json!({"evt":"READY"})).unwrap();
        assert_eq!(
            u32::from_le_bytes(frame[4..8].try_into().unwrap()) as usize,
            body.len()
        );
        let mut buffered = frame;
        let (opcode, value) = take_frame(&mut buffered).unwrap().unwrap();
        assert_eq!(opcode, OPCODE_FRAME);
        assert_eq!(value["evt"], "READY");

        let oversized = MAX_FRAME_BYTES + 1;
        assert!(oversized <= u32::MAX as usize);
    }

    #[test]
    fn partial_frames_are_buffered_until_complete() {
        let first = frame_bytes(OPCODE_FRAME, &json!({"evt":"READY"}));
        let second = frame_bytes(OPCODE_FRAME, &json!({"cmd":"SET_ACTIVITY"}));

        let mut buffered = Vec::new();
        buffered.extend_from_slice(&first[..3]);
        assert!(take_frame(&mut buffered).unwrap().is_none());
        buffered.extend_from_slice(&first[3..first.len() - 2]);
        assert!(take_frame(&mut buffered).unwrap().is_none());
        buffered.extend_from_slice(&first[first.len() - 2..]);
        buffered.extend_from_slice(&second);

        let (opcode, value) = take_frame(&mut buffered).unwrap().unwrap();
        assert_eq!(opcode, OPCODE_FRAME);
        assert_eq!(value["evt"], "READY");
        let (opcode, value) = take_frame(&mut buffered).unwrap().unwrap();
        assert_eq!(opcode, OPCODE_FRAME);
        assert_eq!(value["cmd"], "SET_ACTIVITY");
        assert!(buffered.is_empty());
    }

    #[test]
    fn oversized_frames_are_refused() {
        let mut buffered = Vec::new();
        buffered.extend_from_slice(&OPCODE_FRAME.to_le_bytes());
        buffered.extend_from_slice(&((MAX_FRAME_BYTES + 1) as u32).to_le_bytes());
        assert!(matches!(
            take_frame(&mut buffered),
            Err(Failure::Corrupt(_))
        ));
    }

    #[test]
    fn a_panicking_exchange_still_releases_the_in_flight_marker() {
        let marker = Arc::new(AtomicBool::new(true));
        drop(WorkerGuard(Arc::clone(&marker)));
        assert!(!marker.load(Ordering::SeqCst));
    }

    #[test]
    fn overlapped_completion_is_classified_by_documented_outcome() {
        assert_eq!(
            classify_overlapped(true, 42, None),
            OverlappedStatus::Completed(42)
        );
        assert_eq!(
            classify_overlapped(false, 0, Some(ERROR_OPERATION_ABORTED_CODE)),
            OverlappedStatus::Aborted
        );
        assert_eq!(
            classify_overlapped(false, 0, Some(6)),
            OverlappedStatus::Failed(6)
        );
        assert_eq!(
            classify_overlapped(false, 0, Some(ERROR_OPERATION_ABORTED_CODE)),
            OverlappedStatus::Aborted
        );
    }

    #[test]
    fn a_cancelled_operation_that_completed_still_delivers_its_bytes() {
        assert_eq!(
            classify_overlapped(true, 7, None),
            OverlappedStatus::Completed(7)
        );
    }

    #[test]
    fn the_mailbox_keeps_one_intent_and_bounds_its_waiters() {
        let (wake, _woken) = mpsc::sync_channel(1);
        let mailbox = Mailbox {
            state: Arc::new(Mutex::new(MailState::default())),
            wake,
        };
        for position in 0..(MAX_WAITERS as u64 * 3) {
            let mut state = mailbox.state();
            state.desired = Some(Desired {
                activity: json!({"state":position}),
                signature: position.to_string(),
            });
            if state.waiters.len() >= MAX_WAITERS {
                state.waiters.remove(0);
            }
            state.waiters.push(mpsc::channel().0);
        }
        assert_eq!(mail_state(&mailbox.state).waiters.len(), MAX_WAITERS);
        assert_eq!(
            mailbox.desired().unwrap().signature,
            (MAX_WAITERS as u64 * 3 - 1).to_string()
        );
    }

    #[cfg(unix)]
    mod fake_ipc {
        use super::*;
        use std::os::unix::net::{UnixListener, UnixStream};
        use std::sync::Mutex;
        use tempfile::TempDir;

        struct FakeDiscord {
            directory: TempDir,
            state: Arc<Mutex<FakeState>>,
            streams: Arc<Mutex<Vec<UnixStream>>>,
            running: Arc<AtomicBool>,
            accept: Option<thread::JoinHandle<()>>,
        }

        #[derive(Default)]
        struct FakeState {
            connections: usize,
            open: usize,
            peak_open: usize,
            handshakes: usize,
            applied: Option<Value>,
            pongs: usize,
            reject_handshake: bool,
            reject_activity: bool,
            silent: bool,
            split_replies: bool,
            interleave_events: bool,
            foreign_error: bool,
            delay_ready: bool,
        }

        impl FakeDiscord {
            fn start(directory: TempDir) -> Self {
                let mut fake = Self {
                    directory,
                    state: Arc::new(Mutex::new(FakeState::default())),
                    streams: Arc::new(Mutex::new(Vec::new())),
                    running: Arc::new(AtomicBool::new(true)),
                    accept: None,
                };
                fake.bind();
                fake
            }

            fn bind(&mut self) {
                let listener =
                    UnixListener::bind(self.directory.path().join("discord-ipc-0")).unwrap();
                listener.set_nonblocking(true).unwrap();
                let state = Arc::clone(&self.state);
                let streams = Arc::clone(&self.streams);
                let running = Arc::clone(&self.running);
                self.accept = Some(thread::spawn(move || {
                    while running.load(Ordering::SeqCst) {
                        match listener.accept() {
                            Ok((stream, _)) => {
                                let state = Arc::clone(&state);
                                let streams = Arc::clone(&streams);
                                let running = Arc::clone(&running);
                                thread::spawn(move || serve(stream, state, streams, running));
                            }
                            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                                thread::sleep(Duration::from_millis(2));
                            }
                            Err(_) => break,
                        }
                    }
                }));
            }

            fn directories(&self) -> Vec<PathBuf> {
                vec![self.directory.path().to_path_buf()]
            }

            fn state(&self) -> std::sync::MutexGuard<'_, FakeState> {
                self.state.lock().unwrap_or_else(PoisonError::into_inner)
            }

            fn send_frame(&self, opcode: u32, value: &Value) {
                if let Some(stream) = self.streams.lock().unwrap().last_mut() {
                    let _ = stream.write_all(&frame_bytes(opcode, value));
                }
            }

            fn restart(&mut self) {
                self.shutdown();
                self.state = Arc::new(Mutex::new(FakeState::default()));
                self.streams = Arc::new(Mutex::new(Vec::new()));
                self.running = Arc::new(AtomicBool::new(true));
                self.bind();
            }

            fn shutdown(&mut self) {
                self.running.store(false, Ordering::SeqCst);
                for stream in self.streams.lock().unwrap().drain(..) {
                    let _ = stream.shutdown(std::net::Shutdown::Both);
                }
                if let Some(accept) = self.accept.take() {
                    let _ = accept.join();
                }
                let _ = std::fs::remove_file(self.directory.path().join("discord-ipc-0"));
            }
        }

        impl Drop for FakeDiscord {
            fn drop(&mut self) {
                self.shutdown();
            }
        }

        fn serve(
            mut stream: UnixStream,
            state: Arc<Mutex<FakeState>>,
            streams: Arc<Mutex<Vec<UnixStream>>>,
            running: Arc<AtomicBool>,
        ) {
            let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
            streams.lock().unwrap().push(stream.try_clone().unwrap());
            {
                let mut state = state.lock().unwrap();
                state.connections += 1;
                state.open += 1;
                state.peak_open = state.peak_open.max(state.open);
            }
            if let Some((OPCODE_HANDSHAKE, _)) = read_frame(&mut stream) {
                let (reject, split, delay_ready) = {
                    let mut state = state.lock().unwrap();
                    state.handshakes += 1;
                    (
                        state.reject_handshake,
                        state.split_replies,
                        state.delay_ready,
                    )
                };
                if reject {
                    let _ = write_frame(
                        &mut stream,
                        OPCODE_FRAME,
                        &json!({
                            "cmd":"DISPATCH","evt":"ERROR",
                            "data":{"code":4000,"message":"Invalid Client ID"}
                        }),
                        split,
                    );
                } else {
                    if delay_ready {
                        let _ = write_frame(
                            &mut stream,
                            OPCODE_FRAME,
                            &json!({
                                "cmd":"DISPATCH","evt":"CURRENT_USER_UPDATE",
                                "data":{"id":"1"}
                            }),
                            split,
                        );
                    }
                    if write_frame(
                        &mut stream,
                        OPCODE_FRAME,
                        &json!({"cmd":"DISPATCH","evt":"READY","data":{"v":1}}),
                        split,
                    )
                    .is_ok()
                    {
                        while running.load(Ordering::SeqCst) {
                            let Some((opcode, value)) = read_frame(&mut stream) else {
                                break;
                            };
                            match opcode {
                                OPCODE_FRAME => {
                                    let (silent, reject, split, interleave, foreign) = {
                                        let state = state.lock().unwrap();
                                        (
                                            state.silent,
                                            state.reject_activity,
                                            state.split_replies,
                                            state.interleave_events,
                                            state.foreign_error,
                                        )
                                    };
                                    if silent {
                                        continue;
                                    }
                                    if value["cmd"] == "SET_ACTIVITY" {
                                        if foreign {
                                            let _ = write_frame(
                                                &mut stream,
                                                OPCODE_FRAME,
                                                &json!({
                                                    "cmd":"SET_ACTIVITY","evt":"ERROR",
                                                    "data":{"code":4002,"message":"Other command"},
                                                    "nonce":"foreign-nonce"
                                                }),
                                                split,
                                            );
                                        }
                                        if interleave {
                                            let _ = write_frame(
                                                &mut stream,
                                                OPCODE_FRAME,
                                                &json!({
                                                    "cmd":"DISPATCH","evt":"CURRENT_USER_UPDATE",
                                                    "data":{"id":"1"}
                                                }),
                                                split,
                                            );
                                        }
                                        if reject {
                                            let _ = write_frame(
                                                &mut stream,
                                                OPCODE_FRAME,
                                                &json!({
                                                    "cmd":"SET_ACTIVITY","evt":"ERROR",
                                                    "data":{"code":4002,"message":"Invalid activity"},
                                                    "nonce":value["nonce"]
                                                }),
                                                split,
                                            );
                                            continue;
                                        }
                                        {
                                            let mut state = state.lock().unwrap();
                                            let activity = value["args"]["activity"].clone();
                                            state.applied = if activity.is_null() {
                                                None
                                            } else {
                                                Some(activity)
                                            };
                                        }
                                        let _ = write_frame(
                                            &mut stream,
                                            OPCODE_FRAME,
                                            &json!({
                                                "cmd":"SET_ACTIVITY",
                                                "data":value["args"]["activity"],
                                                "evt":Value::Null,
                                                "nonce":value["nonce"]
                                            }),
                                            split,
                                        );
                                    }
                                }
                                OPCODE_PING => {
                                    let split = state.lock().unwrap().split_replies;
                                    let _ =
                                        write_frame(&mut stream, OPCODE_PONG, &json!({}), split);
                                }
                                OPCODE_PONG => state.lock().unwrap().pongs += 1,
                                OPCODE_CLOSE => break,
                                _ => {}
                            }
                        }
                    }
                }
            }
            let mut state = state.lock().unwrap_or_else(PoisonError::into_inner);
            state.open -= 1;
            state.applied = None;
        }

        fn read_frame(stream: &mut UnixStream) -> Option<(u32, Value)> {
            let mut header = [0_u8; 8];
            let mut filled = 0;
            while filled < header.len() {
                match std::io::Read::read(stream, &mut header[filled..]) {
                    Ok(0) => return None,
                    Ok(count) => filled += count,
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                        ) =>
                    {
                        continue;
                    }
                    Err(_) => return None,
                }
            }
            let opcode = u32::from_le_bytes(header[..4].try_into().unwrap());
            let length = u32::from_le_bytes(header[4..].try_into().unwrap()) as usize;
            if length > MAX_FRAME_BYTES {
                return None;
            }
            let mut body = vec![0_u8; length];
            std::io::Read::read_exact(stream, &mut body).ok()?;
            serde_json::from_slice(&body)
                .ok()
                .map(|value| (opcode, value))
        }

        fn write_frame(
            stream: &mut UnixStream,
            opcode: u32,
            value: &Value,
            split: bool,
        ) -> io::Result<()> {
            let frame = frame_bytes(opcode, value);
            if split {
                stream.write_all(&frame[..3])?;
                stream.flush()?;
                thread::sleep(Duration::from_millis(20));
            }
            stream.write_all(&frame[if split { 3 } else { 0 }..])
        }

        fn wait_until(condition: impl Fn() -> bool) {
            let deadline = Instant::now() + Duration::from_secs(10);
            while Instant::now() < deadline {
                if condition() {
                    return;
                }
                thread::sleep(Duration::from_millis(10));
            }
            panic!("timed out waiting for the fake Discord state to settle");
        }

        fn streaming_params() -> Value {
            json!({
                "enabled":true,
                "gameName":"Example Game",
                "kind":"streaming",
                "startTimestampMs":1_528_245_984_000_u64
            })
        }

        #[test]
        fn presence_stays_visible_while_opennow_holds_the_session() {
            let fake = FakeDiscord::start(TempDir::new().unwrap());
            let service = DiscordService::with_directories(fake.directories());

            let status = service.sync(&streaming_params()).unwrap();
            assert_eq!(status["connected"], true);
            assert_eq!(status["unchanged"], false);

            wait_until(|| {
                let state = fake.state();
                state.open == 1 && state.applied.is_some()
            });

            let repeated = service.sync(&streaming_params()).unwrap();
            assert_eq!(repeated["unchanged"], true);
            let state = fake.state();
            assert_eq!(state.connections, 1);
            assert_eq!(state.handshakes, 1);
            assert_eq!(state.open, 1);
            let applied = state.applied.as_ref().unwrap();
            assert_eq!(applied["details"], "Example Game");
            assert_eq!(applied["timestamps"]["start"], 1_528_245_984_u64);
        }

        #[test]
        fn a_discord_restart_reapplies_the_activity_without_a_new_request() {
            let mut fake = FakeDiscord::start(TempDir::new().unwrap());
            let service = DiscordService::with_directories(fake.directories());

            service.sync(&streaming_params()).unwrap();
            wait_until(|| fake.state().applied.is_some());

            fake.restart();
            wait_until(|| {
                let state = fake.state();
                state.open == 1
                    && state
                        .applied
                        .as_ref()
                        .map(|activity| activity["details"] == "Example Game")
                        .unwrap_or(false)
            });
        }

        #[test]
        fn a_close_frame_is_recovered_from_and_the_activity_reapplied() {
            let fake = FakeDiscord::start(TempDir::new().unwrap());
            let service = DiscordService::with_directories(fake.directories());

            service.sync(&streaming_params()).unwrap();
            wait_until(|| fake.state().applied.is_some());

            fake.send_frame(OPCODE_CLOSE, &json!({"code":4000,"message":"Overtaken"}));
            wait_until(|| fake.state().connections >= 2);
            wait_until(|| {
                fake.state()
                    .applied
                    .as_ref()
                    .map(|activity| activity["details"] == "Example Game")
                    .unwrap_or(false)
            });
        }

        #[test]
        fn a_wedged_discord_does_not_block_the_caller_and_recovers() {
            let fake = FakeDiscord::start(TempDir::new().unwrap());
            fake.state().silent = true;
            let service = DiscordService::with_directories(fake.directories());

            let started = Instant::now();
            let status = service.sync(&streaming_params()).unwrap();
            let elapsed = started.elapsed();
            assert!(
                elapsed < Duration::from_secs(4),
                "sync blocked a caller for {elapsed:?}"
            );
            assert_eq!(status["pending"], true);
            assert!(fake.state().applied.is_none());

            fake.state().silent = false;
            wait_until(|| fake.state().applied.is_some());
        }

        #[test]
        fn clearing_removes_the_activity_and_releases_the_connection() {
            let fake = FakeDiscord::start(TempDir::new().unwrap());
            let service = DiscordService::with_directories(fake.directories());

            service.sync(&streaming_params()).unwrap();
            wait_until(|| fake.state().applied.is_some());

            let status = service.clear().unwrap();
            assert_eq!(status["cleared"], true);
            wait_until(|| {
                let state = fake.state();
                state.open == 0 && state.applied.is_none()
            });
            assert_eq!(fake.state().connections, 1);
        }

        #[test]
        fn a_disabled_presence_never_opens_a_socket() {
            let fake = FakeDiscord::start(TempDir::new().unwrap());
            let service = DiscordService::with_directories(fake.directories());

            let status = service
                .sync(&json!({"enabled":false,"gameName":"Example"}))
                .unwrap();
            assert_eq!(status["cleared"], true);
            let status = service.clear().unwrap();
            assert_eq!(status["cleared"], true);
            thread::sleep(Duration::from_millis(100));
            assert_eq!(fake.state().connections, 0);
        }

        #[test]
        fn a_rejected_activity_is_not_cached_as_applied_and_is_retried() {
            let fake = FakeDiscord::start(TempDir::new().unwrap());
            fake.state().reject_activity = true;
            let service = DiscordService::with_directories(fake.directories());

            let status = service.sync(&streaming_params()).unwrap();
            assert_eq!(status["applied"], false);
            assert!(fake.state().applied.is_none());

            fake.state().reject_activity = false;
            wait_until(|| fake.state().applied.is_some());
        }

        #[test]
        fn a_rejected_handshake_is_reported_and_retried() {
            let fake = FakeDiscord::start(TempDir::new().unwrap());
            fake.state().reject_handshake = true;
            let service = DiscordService::with_directories(fake.directories());

            let status = service.sync(&streaming_params()).unwrap();
            assert_eq!(status["connected"], false);
            assert_eq!(status["applied"], false);
            assert!(
                status["message"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("Invalid Client ID")
            );

            fake.state().reject_handshake = false;
            wait_until(|| fake.state().applied.is_some());
        }

        #[test]
        fn a_handshake_must_deliver_ready() {
            let fake = FakeDiscord::start(TempDir::new().unwrap());
            fake.state().delay_ready = true;
            let service = DiscordService::with_directories(fake.directories());

            let status = service.sync(&streaming_params()).unwrap();
            assert_eq!(status["connected"], true);
            wait_until(|| fake.state().applied.is_some());
        }

        #[test]
        fn unrelated_and_foreign_error_frames_are_ignored() {
            let fake = FakeDiscord::start(TempDir::new().unwrap());
            fake.state().interleave_events = true;
            fake.state().foreign_error = true;
            let service = DiscordService::with_directories(fake.directories());

            let status = service.sync(&streaming_params()).unwrap();
            assert_eq!(status["connected"], true);
            assert_eq!(status["unchanged"], false);
            wait_until(|| fake.state().applied.is_some());
        }

        #[test]
        fn fragmented_replies_are_reassembled() {
            let fake = FakeDiscord::start(TempDir::new().unwrap());
            fake.state().split_replies = true;
            let service = DiscordService::with_directories(fake.directories());

            let status = service.sync(&streaming_params()).unwrap();
            assert_eq!(status["connected"], true);
            wait_until(|| fake.state().applied.is_some());
            assert_eq!(
                fake.state().applied.as_ref().unwrap()["details"],
                "Example Game"
            );
        }

        #[test]
        fn a_discord_ping_is_answered_with_a_pong() {
            let fake = FakeDiscord::start(TempDir::new().unwrap());
            let service = DiscordService::with_directories(fake.directories());

            service.sync(&streaming_params()).unwrap();
            wait_until(|| fake.state().applied.is_some());

            fake.send_frame(OPCODE_PING, &json!({"cmd":"PING"}));
            wait_until(|| fake.state().pongs >= 1);
            assert!(fake.state().applied.is_some());
        }

        #[test]
        fn an_exchange_is_refused_while_another_is_in_flight() {
            let fake = FakeDiscord::start(TempDir::new().unwrap());
            fake.state().silent = true;
            let service = DiscordService::with_directories(fake.directories());

            let first = service.sync(&streaming_params()).unwrap();
            assert_eq!(first["pending"], true);
            assert_eq!(first["unchanged"], Value::Null);

            let started = Instant::now();
            let second = service.sync(&streaming_params()).unwrap();
            let elapsed = started.elapsed();
            assert!(
                elapsed < Duration::from_secs(4),
                "a request behind an in-flight exchange was not answered: {elapsed:?}"
            );
            assert_eq!(second["pending"], true);
            assert_eq!(second["unchanged"], Value::Null);
            assert_eq!(fake.state().peak_open, 1);

            fake.state().silent = false;
            wait_until(|| fake.state().applied.is_some());
            let state = fake.state();
            assert_eq!(state.peak_open, 1);
            assert_eq!(state.open, 1);
        }

        #[test]
        fn abandonment_after_a_failed_exchange_stops_the_worker_without_spinning() {
            let fake = FakeDiscord::start(TempDir::new().unwrap());
            fake.state().reject_activity = true;
            let service = DiscordService::with_directories(fake.directories());

            let status = service.sync(&streaming_params()).unwrap();
            assert_eq!(status["applied"], false);
            wait_until(|| fake.state().open == 0);

            service.abandoned.store(true, Ordering::SeqCst);
            let observed = {
                let before = service.iterations.load(std::sync::atomic::Ordering::SeqCst);
                thread::sleep(Duration::from_millis(300));
                service
                    .iterations
                    .load(std::sync::atomic::Ordering::SeqCst)
                    .saturating_sub(before)
            };
            assert!(
                observed < 50,
                "worker spun {observed} times after abandonment with no live connection"
            );

            let status = service.sync(&streaming_params()).unwrap();
            assert_eq!(status["terminal"], true);
            assert_eq!(status["pending"], false);
        }

        #[test]
        fn abandonment_after_a_failed_initial_connect_stops_the_worker_without_spinning() {
            let directory = TempDir::new().unwrap();
            let service = DiscordService::with_directories(vec![directory.path().to_path_buf()]);

            let status = service.sync(&streaming_params()).unwrap();
            assert_eq!(status["connected"], false);

            service.abandoned.store(true, Ordering::SeqCst);
            let observed = {
                let before = service.iterations.load(std::sync::atomic::Ordering::SeqCst);
                thread::sleep(Duration::from_millis(300));
                service
                    .iterations
                    .load(std::sync::atomic::Ordering::SeqCst)
                    .saturating_sub(before)
            };
            assert!(
                observed < 50,
                "worker spun {observed} times after abandonment with no connection"
            );

            let status = service.clear().unwrap();
            assert_eq!(status["terminal"], true);
        }

        #[test]
        fn buffered_frames_do_not_bypass_the_poll_deadline() {
            let (stream, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
            let mut connection = Connection {
                stream,
                scratch: Vec::new(),
                buffered: Vec::new(),
                applied: None,
            };
            for _ in 0..5 {
                connection.buffered.extend_from_slice(&frame_bytes(
                    OPCODE_FRAME,
                    &json!({"cmd":"DISPATCH","evt":"CURRENT_USER_UPDATE","data":{"id":"1"}}),
                ));
            }
            let buffered_before = connection.buffered.len();

            let expired = Instant::now()
                .checked_sub(Duration::from_millis(1))
                .expect("monotonic clock has advanced past one millisecond");
            assert!(connection.poll(expired).unwrap().is_none());
            assert_eq!(connection.buffered.len(), buffered_before);

            let first = connection
                .poll(Instant::now() + Duration::from_secs(1))
                .unwrap();
            assert!(first.is_some());
            assert!(connection.buffered.len() < buffered_before);
        }

        #[test]
        fn abandoning_the_transport_stops_further_connections_and_bounds_abandoned_work() {
            let fake = FakeDiscord::start(TempDir::new().unwrap());
            let service = DiscordService::with_directories(fake.directories());
            let alive = Arc::clone(&service.worker_alive);

            service.sync(&streaming_params()).unwrap();
            wait_until(|| fake.state().applied.is_some());
            let connections_before = fake.state().connections;

            service.abandoned.store(true, Ordering::SeqCst);

            let started = Instant::now();
            for _ in 0..10 {
                let synced = service.sync(&streaming_params()).unwrap();
                assert_eq!(synced["terminal"], true);
                assert_eq!(synced["applied"], false);
                let cleared = service.clear().unwrap();
                assert_eq!(cleared["terminal"], true);
            }
            assert!(
                started.elapsed() < Duration::from_secs(1),
                "callers were not answered immediately after abandonment"
            );

            wait_until(|| fake.state().open == 0);
            thread::sleep(Duration::from_millis(700));
            assert_eq!(
                fake.state().connections,
                connections_before,
                "a new connection was attempted after the transport was abandoned"
            );
            assert!(alive.load(Ordering::SeqCst));

            drop(service);
            wait_until(|| !alive.load(Ordering::SeqCst));
        }

        #[test]
        fn a_newer_intent_is_never_lost_behind_an_older_exchange() {
            let fake = FakeDiscord::start(TempDir::new().unwrap());
            fake.state().silent = true;
            let service = DiscordService::with_directories(fake.directories());

            let _older = service.sync(&json!({
                "enabled":true,
                "gameName":"Example Game",
                "kind":"queued",
                "queuePosition":1
            }));
            assert!(fake.state().applied.is_none());

            fake.state().silent = false;
            let _newer = service.sync(&json!({
                "enabled":true,
                "gameName":"Example Game",
                "kind":"queued",
                "queuePosition":2
            }));

            wait_until(|| {
                fake.state()
                    .applied
                    .as_ref()
                    .map(|activity| activity["state"] == "In queue (#2)")
                    .unwrap_or(false)
            });
        }

        #[test]
        fn a_flood_of_updates_coalesces_and_a_clear_supersedes_them() {
            let fake = FakeDiscord::start(TempDir::new().unwrap());
            let service = DiscordService::with_directories(fake.directories());

            for position in 1..=40_u64 {
                let _ = service.sync(&json!({
                    "enabled":true,
                    "gameName":"Example Game",
                    "kind":"queued",
                    "queuePosition":position
                }));
            }
            let status = service.clear().unwrap();
            assert_eq!(status["cleared"], true);
            wait_until(|| {
                let state = fake.state();
                state.applied.is_none()
            });
            let state = fake.state();
            assert_eq!(state.peak_open, 1);
            assert_eq!(state.connections, 1);
        }

        #[test]
        fn dropping_the_service_stops_the_worker_and_releases_the_connection() {
            let fake = FakeDiscord::start(TempDir::new().unwrap());
            let service = DiscordService::with_directories(fake.directories());
            let alive = Arc::clone(&service.worker_alive);

            service.sync(&streaming_params()).unwrap();
            wait_until(|| fake.state().applied.is_some());

            drop(service);
            wait_until(|| !alive.load(Ordering::SeqCst));
            wait_until(|| {
                let state = fake.state();
                state.open == 0 && state.applied.is_none()
            });
        }

        #[test]
        fn dropping_an_idle_service_stops_the_worker() {
            let fake = FakeDiscord::start(TempDir::new().unwrap());
            let service = DiscordService::with_directories(fake.directories());
            let alive = Arc::clone(&service.worker_alive);

            service.sync(&streaming_params()).unwrap();
            wait_until(|| fake.state().applied.is_some());
            service.clear().unwrap();
            wait_until(|| {
                let state = fake.state();
                state.open == 0
            });

            drop(service);
            wait_until(|| !alive.load(Ordering::SeqCst));
            assert_eq!(fake.state().open, 0);
        }

        #[test]
        fn candidate_directories_are_deduplicated_and_end_with_the_tmp_fallback() {
            let directories = candidate_directories();
            assert_eq!(directories.last().unwrap(), std::path::Path::new("/tmp"));
            let mut sorted = directories.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(sorted.len(), directories.len());
            assert!(
                directories
                    .iter()
                    .all(|directory| !directory.as_os_str().is_empty())
            );
        }
    }
}
