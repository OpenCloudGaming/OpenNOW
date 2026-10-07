use super::channel::{Channels, ControlReader, MediaReader};
pub use super::channel::{MediaFrame, WorkerEvent};
use opennow_media_protocol::wire::{
    ControlMessage, VIDEO_TRACK_ID, WorkerBootstrap, encode_control,
};
use opennow_media_protocol::{MAX_BOOTSTRAP_BYTES, MEDIA_PROTOCOL_VERSION};
use opennow_plugin_api::provider::SecretBytes;
use opennow_plugin_package::{Role, VerifiedPackage};
use std::io::{self, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const START_TIMEOUT: Duration = Duration::from_secs(3);
const REAP_TIMEOUT: Duration = Duration::from_millis(300);

pub struct WorkerSession {
    state: Arc<State>,
    supervisor: Option<JoinHandle<()>>,
}

struct State {
    process: Mutex<Option<OwnedChild>>,
    stopped: AtomicBool,
    channels: Mutex<Channels>,
    attempt: u64,
    maximum_control: usize,
    _package: VerifiedPackage,
    data_root: PathBuf,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl WorkerSession {
    pub fn spawn(
        package: VerifiedPackage,
        data_root: &Path,
        mut bootstrap: WorkerBootstrap,
    ) -> io::Result<Self> {
        if bootstrap.binding.source_id.is_builtin()
            || &bootstrap.binding.source_id != package.manifest().id()
        {
            return Err(io::Error::other(
                "Worker binding does not match the verified provider",
            ));
        }
        let data_root = validate_data_root(data_root)?;
        if package.entrypoint(Role::Media).is_none() {
            return Err(io::Error::other("Verified package has no media entrypoint"));
        }
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        bootstrap.control_port = listener.local_addr()?.port();
        let mut token = vec![0; 32];
        getrandom::fill(&mut token)
            .map_err(|_| io::Error::other("Could not create worker authentication"))?;
        bootstrap.authentication = SecretBytes::new(token).map_err(io::Error::other)?;
        bootstrap.validate().map_err(io::Error::other)?;
        let mut bytes = serde_json::to_vec(&bootstrap)
            .map_err(|_| io::Error::other("Invalid worker bootstrap"))?;
        if bytes.len() >= MAX_BOOTSTRAP_BYTES {
            return Err(io::Error::other("Worker bootstrap exceeds its limit"));
        }
        bytes.push(b'\n');
        let state = Arc::new(State {
            process: Mutex::new(None),
            stopped: AtomicBool::new(false),
            channels: Mutex::new(Channels::new(bootstrap.limits.clone())),
            attempt: bootstrap.attempt_generation,
            maximum_control: bootstrap.limits.max_control_message_bytes as usize,
            _package: package,
            data_root,
        });
        let worker_state = state.clone();
        let supervisor = thread::Builder::new()
            .name("provider-media-worker".into())
            .spawn(move || {
                let result = supervise(&worker_state, &bootstrap, listener, bytes);
                lock(&worker_state.channels).terminal(match result {
                    Ok(()) => WorkerEvent::Exited,
                    Err(message) => WorkerEvent::Failed(message),
                });
                worker_state.signal_stop();
                worker_state.reap();
            })?;
        Ok(Self {
            state,
            supervisor: Some(supervisor),
        })
    }

    pub fn attempt_generation(&self) -> u64 {
        self.state.attempt
    }

    pub fn pid(&self) -> Option<u32> {
        lock(&self.state.process)
            .as_ref()
            .map(|process| process.child.id())
    }

    pub fn send(&self, message: ControlMessage) -> io::Result<()> {
        if self.state.stopped.load(Ordering::Acquire) {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        let valid = match &message {
            ControlMessage::Input {
                attempt_generation, ..
            }
            | ControlMessage::Neutral {
                attempt_generation, ..
            }
            | ControlMessage::Stop {
                attempt_generation, ..
            } => *attempt_generation == self.state.attempt,
            ControlMessage::Keyframe {
                attempt_generation,
                track_id,
            } => *attempt_generation == self.state.attempt && *track_id == VIDEO_TRACK_ID,
            ControlMessage::FrameProgress { provenance, .. } => {
                provenance.attempt_generation == self.state.attempt
                    && provenance.track_id == VIDEO_TRACK_ID
                    && provenance.validate().is_ok()
            }
            _ => false,
        };
        if !valid {
            return Err(io::Error::other("Invalid host worker control message"));
        }
        lock(&self.state.channels).send(message)
    }

    pub fn recv_video(&self) -> Option<MediaFrame> {
        lock(&self.state.channels).recv_video()
    }
    pub fn recv_audio(&self) -> Option<MediaFrame> {
        lock(&self.state.channels).recv_audio()
    }
    pub fn recv_control(&self) -> Option<WorkerEvent> {
        lock(&self.state.channels).recv_control()
    }
    pub fn coalesced_frame_progress(&self) -> u64 {
        lock(&self.state.channels).coalesced_frame_progress()
    }
    pub fn signal_stop(&self) {
        self.state.signal_stop();
    }

    pub fn reap(&mut self) {
        self.signal_stop();
        if let Some(supervisor) = self.supervisor.take() {
            let _ = supervisor.join();
        }
        self.state.reap();
    }
}

fn validate_data_root(path: &Path) -> io::Result<PathBuf> {
    if !path.is_absolute() {
        return Err(io::Error::other("Worker data directory must be absolute"));
    }
    for component in path.ancestors() {
        let metadata = std::fs::symlink_metadata(component)?;
        if !metadata.is_dir() || opennow_plugin_package::is_link(&metadata) {
            return Err(io::Error::other(
                "Worker data directory cannot traverse links",
            ));
        }
    }
    let canonical = path.canonicalize()?;
    if canonical != path {
        return Err(io::Error::other("Worker data directory must be canonical"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = std::fs::symlink_metadata(&canonical)?;
        if metadata.mode() & 0o7777 != 0o700 || metadata.uid() != unsafe { libc::geteuid() } {
            return Err(io::Error::other(
                "Worker data directory must be private and owned by the current user",
            ));
        }
    }
    Ok(canonical)
}

impl Drop for WorkerSession {
    fn drop(&mut self) {
        self.reap();
    }
}

impl State {
    fn signal_stop(&self) {
        let mut process = lock(&self.process);
        if !self.stopped.load(Ordering::Acquire) {
            if let Some(process) = process.as_mut() {
                process.signal_kill();
            }
            self.stopped.store(true, Ordering::Release);
        }
    }

    fn reap(&self) {
        let deadline = Instant::now() + REAP_TIMEOUT;
        loop {
            let mut process = lock(&self.process);
            let Some(child) = process.as_mut() else {
                return;
            };
            if matches!(child.child.try_wait(), Ok(Some(_))) {
                process.take();
                return;
            }
            drop(process);
            if Instant::now() >= deadline {
                return;
            }
            thread::sleep(Duration::from_millis(2));
        }
    }
}

fn supervise(
    state: &Arc<State>,
    bootstrap: &WorkerBootstrap,
    listener: TcpListener,
    bytes: Vec<u8>,
) -> Result<(), &'static str> {
    let (stdin, stdout) = {
        let mut registered = lock(&state.process);
        if state.stopped.load(Ordering::Acquire) {
            return Ok(());
        }
        let mut command = Command::new(state._package.entrypoint(Role::Media).unwrap());
        validate_data_root(&state.data_root)
            .map_err(|_| "Worker data directory changed before launch")?;
        command
            .env_clear()
            .env("OPENNOW_PLUGIN_DATA_DIR", &state.data_root)
            .current_dir(&state.data_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        if let Some(root) = std::env::var_os("SystemRoot") {
            command.env("SystemRoot", root);
        }
        let process = OwnedChild::spawn(&mut command)
            .map_err(|_| "Could not launch verified media worker")?;
        *registered = Some(process);
        let process = registered.as_mut().unwrap();
        (
            process.child.stdin.take().unwrap(),
            process.child.stdout.take().unwrap(),
        )
    };
    let mut stdout =
        MediaPipe::new(stdout).map_err(|_| "Could not configure bounded media pipe")?;
    let mut writer = BootstrapWriter::new(stdin, bytes)
        .map_err(|_| "Could not open private worker bootstrap")?;
    struct KillBeforePipes<'a>(&'a State);
    impl Drop for KillBeforePipes<'_> {
        fn drop(&mut self) {
            self.0.signal_stop();
        }
    }
    let _kill_before_pipes = KillBeforePipes(state);
    let deadline = Instant::now() + START_TIMEOUT;
    let mut stream = None;
    let mut reader = ControlReader::new();
    let mut authenticated = false;
    let mut ready = false;
    let mut media = MediaReader::new();
    let mut pending_write = Vec::new();
    let mut written = 0;
    let mut write_started = Instant::now();
    let mut control_started = None;
    let mut media_started = None;
    while !state.stopped.load(Ordering::Acquire) {
        let mut progressed = false;
        if lock(&state.process)
            .as_ref()
            .is_some_and(OwnedChild::exited)
        {
            return Err("Media worker exited");
        }
        writer
            .poll()
            .map_err(|_| "Private worker bootstrap failed")?;
        if !ready && Instant::now() >= deadline {
            return Err("Media worker attachment timed out");
        }
        if stream.is_none() {
            match listener.accept() {
                Ok((connected, _)) => {
                    connected
                        .set_nodelay(true)
                        .map_err(|_| "Could not configure worker control")?;
                    connected
                        .set_nonblocking(true)
                        .map_err(|_| "Could not configure worker control")?;
                    stream = Some(connected);
                    progressed = true;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(_) => return Err("Media worker control attachment failed"),
            }
        }
        if let Some(stream) = stream.as_mut() {
            for _ in 0..16 {
                if authenticated && !ready && !pending_write.is_empty() {
                    break;
                }
                let before = reader.in_progress();
                let position = reader.position();
                let message = reader
                    .poll(stream, state.maximum_control)
                    .map_err(|_| "Invalid or closed worker control channel")?;
                if reader.in_progress() && !before {
                    control_started = Some(Instant::now());
                }
                let Some(message) = message else {
                    if position != reader.position() {
                        progressed = true;
                        continue;
                    }
                    break;
                };
                progressed = true;
                control_started = None;
                if !authenticated {
                    match message {
                        ControlMessage::Hello {
                            version,
                            authentication,
                            attempt_generation,
                        } if version == MEDIA_PROTOCOL_VERSION
                            && attempt_generation == state.attempt
                            && authentication.expose_secret().len() == 32
                            && authentication
                                .expose_secret()
                                .iter()
                                .zip(bootstrap.authentication.expose_secret())
                                .fold(0u8, |difference, (left, right)| {
                                    difference | (left ^ right)
                                })
                                == 0 =>
                        {
                            authenticated = true;
                            pending_write = encode_control(
                                &ControlMessage::Attached {
                                    attempt_generation: state.attempt,
                                },
                                state.maximum_control,
                            )
                            .map_err(|_| "Invalid host attachment acknowledgment")?;
                            written = 0;
                            write_started = Instant::now();
                            break;
                        }
                        _ => return Err("Media worker authentication rejected"),
                    }
                } else if !ready {
                    match message {
                        ControlMessage::Ready {
                            attempt_generation,
                            input,
                        } if attempt_generation == state.attempt
                            && input.is_subset_of(&bootstrap.accepted.input) =>
                        {
                            lock(&state.channels).ready(input);
                            ready = true;
                        }
                        _ => return Err("Media worker capabilities rejected"),
                    }
                } else {
                    let valid = match &message {
                        ControlMessage::Ack {
                            attempt_generation, ..
                        }
                        | ControlMessage::Ended { attempt_generation } => {
                            *attempt_generation == state.attempt
                        }
                        ControlMessage::Rumble {
                            attempt_generation,
                            controller,
                            ..
                        } => {
                            *attempt_generation == state.attempt
                                && bootstrap.accepted.input.rumble
                                && *controller < bootstrap.accepted.input.gamepad_slots
                        }
                        _ => false,
                    };
                    if !valid {
                        return Err("Invalid worker control event");
                    }
                    let ended = matches!(message, ControlMessage::Ended { .. });
                    lock(&state.channels)
                        .incoming(message)
                        .map_err(|_| "Media worker control overflow")?;
                    if ended {
                        return Ok(());
                    }
                }
            }
            if control_started
                .is_some_and(|started| Instant::now().duration_since(started) >= START_TIMEOUT)
            {
                return Err("Media worker control frame timed out");
            }
            if authenticated {
                for _ in 0..16 {
                    if pending_write.is_empty() {
                        if !ready {
                            break;
                        }
                        let Some(message) = lock(&state.channels).outgoing() else {
                            break;
                        };
                        pending_write = encode_control(&message, state.maximum_control)
                            .map_err(|_| "Invalid host control message")?;
                        written = 0;
                        write_started = Instant::now();
                    }
                    match stream.write(&pending_write[written..]) {
                        Ok(0) => return Err("Media worker control channel closed"),
                        Ok(size) => {
                            progressed = true;
                            written += size;
                            if written == pending_write.len() {
                                pending_write.clear();
                            }
                        }
                        Err(error)
                            if matches!(
                                error.kind(),
                                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                            ) =>
                        {
                            break;
                        }
                        Err(_) => return Err("Media worker control write failed"),
                    }
                }
                if !pending_write.is_empty() && write_started.elapsed() >= START_TIMEOUT {
                    return Err("Media worker control write timed out");
                }
            }
        }
        if ready {
            for _ in 0..16 {
                if state.stopped.load(Ordering::Acquire) {
                    break;
                }
                let position = media.position();
                match media.poll(
                    &mut stdout,
                    &bootstrap.limits,
                    state.attempt,
                    bootstrap.accepted.audio.is_some(),
                ) {
                    Ok(Some(frame)) => {
                        media_started = None;
                        progressed = true;
                        lock(&state.channels)
                            .push_media(frame)
                            .map_err(|_| "Invalid media payload")?;
                    }
                    Ok(None) => {
                        if media.position() != (0, 0) && media_started.is_none() {
                            media_started = Some(Instant::now());
                        }
                        if media.position() != position {
                            progressed = true;
                            continue;
                        }
                        break;
                    }
                    Err(_) => return Err("Invalid or closed worker media channel"),
                }
            }
            if media_started
                .is_some_and(|started| Instant::now().duration_since(started) >= START_TIMEOUT)
            {
                return Err("Media worker frame timed out");
            }
        }
        if progressed {
            thread::yield_now();
        } else {
            thread::sleep(Duration::from_millis(1));
        }
    }
    Ok(())
}

struct OwnedChild {
    child: Child,
    #[cfg(windows)]
    job: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
unsafe impl Send for OwnedChild {}

#[cfg(unix)]
impl OwnedChild {
    fn spawn(command: &mut Command) -> io::Result<Self> {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
        #[cfg(target_os = "linux")]
        unsafe {
            let parent = libc::getpid();
            command.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                    return Err(io::Error::last_os_error());
                }
                if libc::getppid() != parent {
                    libc::_exit(1);
                }
                Ok(())
            });
        }
        Ok(Self {
            child: command.spawn()?,
        })
    }

    fn signal_kill(&mut self) {
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        let _ = self.child.kill();
    }

    fn exited(&self) -> bool {
        let mut information: libc::siginfo_t = unsafe { std::mem::zeroed() };
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                self.child.id(),
                &mut information,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        };
        result == 0 && unsafe { information.si_pid() } != 0
    }
}

#[cfg(windows)]
impl OwnedChild {
    fn spawn(command: &mut Command) -> io::Result<Self> {
        use std::os::windows::{io::AsRawHandle, process::CommandExt};
        use windows_sys::Win32::{
            Foundation::*,
            System::{Diagnostics::ToolHelp::*, JobObjects::*, Threading::*},
        };
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Err(io::Error::last_os_error());
            }
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&limits) as u32,
            ) == 0
            {
                CloseHandle(job);
                return Err(io::Error::last_os_error());
            }
            command.creation_flags(CREATE_SUSPENDED | CREATE_NO_WINDOW);
            let child = match command.spawn() {
                Ok(child) => child,
                Err(error) => {
                    CloseHandle(job);
                    return Err(error);
                }
            };
            let mut process = Self { child, job };
            let attach = || -> io::Result<()> {
                if AssignProcessToJobObject(job, process.child.as_raw_handle().cast()) == 0 {
                    return Err(io::Error::last_os_error());
                }
                let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
                if snapshot == INVALID_HANDLE_VALUE {
                    return Err(io::Error::last_os_error());
                }
                let mut entry: THREADENTRY32 = std::mem::zeroed();
                entry.dwSize = std::mem::size_of_val(&entry) as u32;
                let mut found = false;
                let mut more = Thread32First(snapshot, &mut entry);
                while more != 0 {
                    if entry.th32OwnerProcessID == process.child.id() {
                        let thread = OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID);
                        if !thread.is_null() {
                            found = ResumeThread(thread) != u32::MAX;
                            CloseHandle(thread);
                        }
                        break;
                    }
                    more = Thread32Next(snapshot, &mut entry);
                }
                CloseHandle(snapshot);
                if found {
                    Ok(())
                } else {
                    Err(io::Error::other("Could not resume owned worker"))
                }
            };
            if let Err(error) = attach() {
                process.signal_kill();
                let _ = process.child.wait();
                return Err(error);
            }
            Ok(process)
        }
    }

    fn signal_kill(&mut self) {
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(self.job, 1);
        }
        let _ = self.child.kill();
    }

    fn exited(&self) -> bool {
        use std::os::windows::io::AsRawHandle;
        unsafe {
            windows_sys::Win32::System::Threading::WaitForSingleObject(
                self.child.as_raw_handle().cast(),
                0,
            ) == 0
        }
    }
}

#[cfg(windows)]
impl Drop for OwnedChild {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.job);
        }
    }
}

struct MediaPipe(ChildStdout);

impl MediaPipe {
    fn new(stdout: ChildStdout) -> io::Result<Self> {
        #[cfg(unix)]
        set_nonblocking(&stdout)?;
        Ok(Self(stdout))
    }
}

impl Read for MediaPipe {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            let mut available = 0;
            let result = unsafe {
                windows_sys::Win32::System::Pipes::PeekNamedPipe(
                    self.0.as_raw_handle().cast(),
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    &mut available,
                    std::ptr::null_mut(),
                )
            };
            if result == 0 {
                return Err(io::Error::last_os_error());
            }
            if available == 0 {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            let length = bytes.len().min(available as usize);
            return self.0.read(&mut bytes[..length]);
        }
        #[cfg(unix)]
        self.0.read(bytes)
    }
}

#[cfg(unix)]
fn set_nonblocking(pipe: &impl std::os::fd::AsRawFd) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(pipe.as_raw_fd(), libc::F_GETFL) };
    if flags < 0
        || unsafe { libc::fcntl(pipe.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(unix)]
struct BootstrapWriter {
    pipe: Option<ChildStdin>,
    bytes: Vec<u8>,
    written: usize,
}

#[cfg(unix)]
impl BootstrapWriter {
    fn new(pipe: ChildStdin, bytes: Vec<u8>) -> io::Result<Self> {
        set_nonblocking(&pipe)?;
        Ok(Self {
            pipe: Some(pipe),
            bytes,
            written: 0,
        })
    }

    fn poll(&mut self) -> io::Result<()> {
        let Some(pipe) = self.pipe.as_mut() else {
            return Ok(());
        };
        match pipe.write(&self.bytes[self.written..]) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(count) => self.written += count,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error),
        }
        if self.written == self.bytes.len() {
            self.bytes.fill(0);
            self.bytes.clear();
            self.pipe.take();
        }
        Ok(())
    }
}

#[cfg(unix)]
impl Drop for BootstrapWriter {
    fn drop(&mut self) {
        self.bytes.fill(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opennow_media_protocol::wire::{AckKind, InputEvent};
    use opennow_plugin_package::{InstalledManifest, PackagePin, current_target, verify};
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::OnceLock;

    const FIXTURE: &str = r#"
use std::{io::{self, Read, Write, BufRead}, net::TcpStream, time::Duration, thread};
fn number(s: &str, key: &str) -> u64 {
    s.split(&format!("\"{key}\":" )).nth(1).unwrap().split(|c:char| !c.is_ascii_digit()).next().unwrap().parse().unwrap()
}
fn text<'a>(s: &'a str, key: &str) -> &'a str {
    s.split(&format!("\"{key}\":\"" )).nth(1).unwrap().split('"').next().unwrap()
}
fn send(s: &mut TcpStream, msg: &str) { s.write_all(&(msg.len() as u32).to_le_bytes()).unwrap(); s.write_all(msg.as_bytes()).unwrap(); }
fn main() {
    if std::env::args().nth(1).as_deref() == Some("descendant") { thread::sleep(Duration::from_secs(60)); return; }
    assert_eq!(std::env::args().count(), 1);
    assert!(std::env::vars_os().all(|(key,_)| key == "SystemRoot" || key == "OPENNOW_PLUGIN_DATA_DIR"));
    let data_root = std::path::PathBuf::from(std::env::var_os("OPENNOW_PLUGIN_DATA_DIR").unwrap());
    assert_eq!(data_root, std::env::current_dir().unwrap());
    std::fs::write(data_root.join("worker-directory-proof"), b"private data").unwrap();
    let exe = std::env::current_exe().unwrap();
    let mode = exe.file_stem().unwrap().to_str().unwrap();
    if mode == "unread" { thread::sleep(Duration::from_secs(60)); return; }
    if mode == "crash" { std::process::exit(23); }
    let mut boot = String::new(); io::stdin().lock().read_line(&mut boot).unwrap();
    assert_eq!(text(&boot, "sourceId"), "org.opennow.test.media");
    assert_eq!(text(&boot, "leaseId"), "fixture-lease");
    assert_eq!(text(&boot, "attemptId"), "fixture-attempt");
    assert_eq!(text(&boot, "remoteId"), "fixture-session");
    if mode == "hang" { thread::sleep(Duration::from_secs(60)); return; }
    let attempt = number(&boot, "attemptGeneration");
    let mut stream = TcpStream::connect(("127.0.0.1", number(&boot, "controlPort") as u16)).unwrap();
    stream.set_nodelay(true).unwrap();
    let auth = if mode == "bad-auth" { "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=" } else { text(&boot, "authentication") };
    send(&mut stream, &format!("{{\"type\":\"hello\",\"version\":1,\"authentication\":\"{auth}\",\"attemptGeneration\":{attempt}}}"));
    if mode == "bad-auth" { thread::sleep(Duration::from_secs(60)); return; }
    let mut prefix=[0;4]; stream.read_exact(&mut prefix).unwrap();
    let mut attached=vec![0;u32::from_le_bytes(prefix) as usize]; stream.read_exact(&mut attached).unwrap();
    let attached=String::from_utf8(attached).unwrap();
    assert_eq!(text(&attached,"type"), "attached");
    assert_eq!(number(&attached,"attemptGeneration"), attempt);
    let mut input = boot.split("\"input\":").nth(1).unwrap().split('}').next().unwrap().to_string() + "}";
    if mode == "bad-ready" { input = input.replace("\"gamepadSlots\":1", "\"gamepadSlots\":4"); }
    send(&mut stream, &format!("{{\"type\":\"ready\",\"attemptGeneration\":{attempt},\"input\":{input}}}"));
    if mode == "partial-control" {
        stream.write_all(&1024u32.to_le_bytes()).unwrap();
        stream.write_all(b"{").unwrap();
        thread::sleep(Duration::from_secs(60)); return;
    }
    if mode == "oversize-control" {
        stream.write_all(&u32::MAX.to_le_bytes()).unwrap();
        thread::sleep(Duration::from_secs(60)); return;
    }
    if mode == "descendants" {
        let child = std::process::Command::new(&exe).arg("descendant").spawn().unwrap();
        send(&mut stream, &format!("{{\"type\":\"ack\",\"attemptGeneration\":{attempt},\"sequence\":{},\"kind\":\"input\"}}", child.id()));
    }
    if mode == "oversize" || mode == "malformed" || mode == "flood" || mode == "partial-media" {
        let mode = mode.to_owned();
        thread::spawn(move || {
            let mut out = io::stdout().lock();
            let mut index = 0u64;
            loop {
                let mut header = [0u8;56]; header[..4].copy_from_slice(b"ONW1");
                header[4] = 14 | u8::from(index % 100 == 0);
                header[8..12].copy_from_slice(&1u32.to_le_bytes());
                header[12..16].copy_from_slice(&(if mode == "oversize" { u32::MAX } else { 4096u32 }).to_le_bytes());
                header[16..24].copy_from_slice(&attempt.to_le_bytes());
                header[24..32].copy_from_slice(&(u64::MAX-index).to_le_bytes());
                header[32..40].copy_from_slice(&(u64::MAX-index*90).to_le_bytes());
                header[40..44].copy_from_slice(&90000u32.to_le_bytes());
                header[44..48].copy_from_slice(&u32::MAX.to_le_bytes());
                if mode == "malformed" { header[0] = 0; }
                if mode == "partial-media" {
                    out.write_all(&header).unwrap(); out.write_all(&[1;17]).unwrap(); out.flush().unwrap();
                    thread::sleep(Duration::from_secs(60)); return;
                }
                if out.write_all(&header).and_then(|_|out.write_all(&[1;4096])).and_then(|_|out.flush()).is_err() { return; }
                index += 1;
            }
        });
    }
    loop {
        let mut prefix=[0;4]; if stream.read_exact(&mut prefix).is_err() { return; }
        let mut body=vec![0;u32::from_le_bytes(prefix) as usize]; stream.read_exact(&mut body).unwrap();
        let body=String::from_utf8(body).unwrap();
        let kind=text(&body,"type");
        let sequence=if kind == "keyframe" {0} else {number(&body,"sequence")};
        if ["input","neutral","keyframe","stop"].contains(&kind) {
            send(&mut stream,&format!("{{\"type\":\"ack\",\"attemptGeneration\":{attempt},\"sequence\":{sequence},\"kind\":\"{kind}\"}}"));
        }
    }
}
"#;

    fn fixture() -> &'static PathBuf {
        static FIXTURE_PATH: OnceLock<PathBuf> = OnceLock::new();
        FIXTURE_PATH.get_or_init(|| {
            let root = tempfile::tempdir().unwrap().keep();
            let source = root.join("fixture.rs");
            let executable = root.join(if cfg!(windows) {
                "fixture.exe"
            } else {
                "fixture"
            });
            fs::write(&source, FIXTURE).unwrap();
            let output = Command::new("rustc")
                .args(["--edition=2024", "-O"])
                .arg(&source)
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "fixture build failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            executable
        })
    }

    fn package(mode: &str) -> (tempfile::TempDir, VerifiedPackage) {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        fs::create_dir(&data).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&data, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let root = temp.path().join("version");
        fs::create_dir(&root).unwrap();
        let executable = fs::read(fixture()).unwrap();
        let filename = if cfg!(windows) {
            format!("{mode}.exe")
        } else {
            mode.into()
        };
        let path = root.join(&filename);
        fs::write(&path, &executable).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let manifest = json!({
            "schemaVersion":2,"protocolVersion":2,"id":"org.opennow.test.media",
            "name":"Media fixture","version":"1.0.0","publisher":"Test","description":"A media test fixture",
            "authKinds":["anonymous"],
            "capabilities":["auth.anonymous.v2","catalog.library.v2","catalog.details.v2","launch.v2","sessions.v2","media.worker.v1"],
            "entrypoints":{current_target():{"control":filename,"media":filename}},
            "files":[{"path":filename,"sha256":format!("{:x}",Sha256::digest(&executable))}]
        });
        fs::write(
            root.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        let manifest: InstalledManifest = serde_json::from_value(manifest).unwrap();
        (temp, verify(&root, &manifest).unwrap())
    }

    fn data_root(temp: &tempfile::TempDir) -> PathBuf {
        temp.path().join("data").canonicalize().unwrap()
    }

    fn bootstrap() -> WorkerBootstrap {
        serde_json::from_value(json!({
            "version":1,"attemptGeneration":7,"controlPort":0,"authentication":"", "providerBootstrap":"",
            "binding":{"leaseId":"fixture-lease","sourceId":"org.opennow.test.media",
                "session":{"account":null,"remoteId":"fixture-session"},"attemptId":"fixture-attempt"},
            "accepted":{"offerId":"test-offer","runtimeEpoch":7,
                "video":{"encoding":"h264-annex-b","width":320,"height":180,"fps":50,"bitDepth":8,"chroma":"yuv420",
                    "color":{"range":"limited","primaries":"bt709","transfer":"bt709","matrix":"bt709","chromaLocation":"left"}},
                "audio":null,"input":{"keyboard":true,"relativeMouse":true,"absoluteMouse":false,"text":false,"gamepadSlots":1,"rumble":false}},
            "limits":{"maxVideoAccessUnitBytes":4096,"maxAudioPacketBytes":4096,"maxBufferedVideoBytes":8192,
                "maxBufferedVideoFrames":2,"maxBufferedAudioMs":20,"maxControlMessageBytes":1024,"maxPendingInputEvents":8}
        })).unwrap()
    }

    fn event_until(
        session: &WorkerSession,
        timeout: Duration,
        predicate: impl Fn(&WorkerEvent) -> bool,
    ) -> WorkerEvent {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(event) = session.recv_control() {
                if predicate(&event) {
                    return event;
                }
                assert!(
                    !matches!(event, WorkerEvent::Failed(_)),
                    "unexpected worker failure: {event:?}"
                );
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for worker event"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn native_worker_rejects_auth_capabilities_crash_and_bad_bulk_headers() {
        for mode in [
            "bad-auth",
            "bad-ready",
            "crash",
            "oversize",
            "malformed",
            "oversize-control",
        ] {
            let (_temp, package) = package(mode);
            let mut session =
                WorkerSession::spawn(package, &data_root(&_temp), bootstrap()).unwrap();
            let event = event_until(&session, Duration::from_secs(4), |event| {
                matches!(event, WorkerEvent::Failed(_))
            });
            let expected = match mode {
                "bad-auth" => "Media worker authentication rejected",
                "bad-ready" => "Media worker capabilities rejected",
                "crash" => "Media worker exited",
                "oversize-control" => "Invalid or closed worker control channel",
                _ => "Invalid or closed worker media channel",
            };
            assert!(
                matches!(event, WorkerEvent::Failed(message) if message == expected),
                "{mode}: {event:?}"
            );
            session.reap();
            assert!(session.pid().is_none());
        }
    }

    #[test]
    fn source_binding_mismatch_and_builtin_are_rejected_before_exec() {
        let (temp, package) = package("idle");
        let root = data_root(&temp);
        for source in [
            "org.opennow.other.provider",
            opennow_plugin_api::BUILTIN_GFN_ID,
        ] {
            let mut boot = bootstrap();
            boot.binding.source_id = opennow_plugin_api::PluginId::new(source).unwrap();
            let error = match WorkerSession::spawn(package.clone(), &root, boot) {
                Err(error) => error,
                Ok(_) => panic!("invalid source binding spawned a worker"),
            };
            assert_eq!(
                error.to_string(),
                "Worker binding does not match the verified provider"
            );
            assert!(!root.join("worker-directory-proof").exists());
        }
    }

    #[test]
    fn startup_is_async_and_stop_cancels_unread_bootstrap_and_missing_hello() {
        for mode in ["unread", "hang"] {
            let (_temp, package) = package(mode);
            let mut boot = bootstrap();
            boot.provider_bootstrap = SecretBytes::new(vec![42; 256 * 1024]).unwrap();
            let start = Instant::now();
            let mut session = WorkerSession::spawn(package, &data_root(&_temp), boot).unwrap();
            assert!(start.elapsed() < Duration::from_millis(500));
            let deadline = Instant::now() + Duration::from_secs(2);
            while session.pid().is_none() {
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(1));
            }
            thread::sleep(Duration::from_millis(30));
            let stop = Instant::now();
            session.signal_stop();
            session.reap();
            assert!(stop.elapsed() < Duration::from_millis(500));
            assert!(session.pid().is_none());
            session.signal_stop();
        }
    }

    #[test]
    fn missing_hello_has_a_bounded_startup_deadline() {
        let (_temp, package) = package("hang");
        let start = Instant::now();
        let mut session = WorkerSession::spawn(package, &data_root(&_temp), bootstrap()).unwrap();
        let event = event_until(&session, Duration::from_secs(5), |event| {
            matches!(event, WorkerEvent::Failed(_))
        });
        assert!(matches!(
            event,
            WorkerEvent::Failed("Media worker attachment timed out")
        ));
        assert!(start.elapsed() < Duration::from_secs(5));
        session.reap();
    }

    #[test]
    fn stalled_partial_control_and_media_frames_have_bounded_deadlines() {
        for (mode, expected) in [
            ("partial-control", "Media worker control frame timed out"),
            ("partial-media", "Media worker frame timed out"),
        ] {
            let (_temp, package) = package(mode);
            let mut session =
                WorkerSession::spawn(package, &data_root(&_temp), bootstrap()).unwrap();
            event_until(&session, Duration::from_secs(4), |event| {
                matches!(event, WorkerEvent::Ready { .. })
            });
            let event = event_until(&session, Duration::from_secs(4), |event| {
                matches!(event, WorkerEvent::Failed(_))
            });
            assert!(matches!(event, WorkerEvent::Failed(message) if message == expected));
            session.reap();
            assert!(session.pid().is_none());
        }
    }

    #[test]
    fn package_pin_survives_spawn_and_is_released_only_after_session_drop() {
        let (_temp, package) = package("idle");
        let root = package.root().to_owned();
        let mut session = WorkerSession::spawn(package, &data_root(&_temp), bootstrap()).unwrap();
        event_until(&session, Duration::from_secs(4), |event| {
            matches!(event, WorkerEvent::Ready { .. })
        });
        assert_eq!(
            fs::read(data_root(&_temp).join("worker-directory-proof")).unwrap(),
            b"private data"
        );
        assert!(!root.join("worker-directory-proof").exists());
        assert_eq!(
            PackagePin::try_exclusive(&root).unwrap_err().code,
            "plugin_in_use"
        );
        session.reap();
        assert_eq!(
            PackagePin::try_exclusive(&root).unwrap_err().code,
            "plugin_in_use"
        );
        drop(session);
        assert!(PackagePin::try_exclusive(&root).is_ok());
    }

    #[test]
    fn lifecycle_poll_retires_a_failed_worker_and_lease_before_emitting_stopped() {
        use super::super::{ActiveLease, ExternalSession};
        use crate::{Engine, State as LifecycleState, lock_lifecycle};
        use std::sync::atomic::AtomicU64;
        use std::sync::mpsc;

        let (temp, package) = package("crash");
        let root = package.root().to_owned();
        let boot = bootstrap();
        let binding = serde_json::to_value(&boot.binding).unwrap();
        let worker = Arc::new(WorkerSession::spawn(package, &data_root(&temp), boot).unwrap());
        event_until(&worker, Duration::from_secs(4), |event| {
            matches!(event, WorkerEvent::Failed("Media worker exited"))
        });
        let worker_state = Arc::clone(&worker.state);
        let retained_worker = Arc::downgrade(&worker);
        let bridge = thread::spawn(|| {});
        let deadline = Instant::now() + Duration::from_secs(1);
        while !bridge.is_finished() {
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        let (events, emitted) = mpsc::channel();
        let (presented, _presentation_receiver) = mpsc::sync_channel(8);
        let mut engine = Engine::new(events);
        lock_lifecycle(&engine.lifecycle).generation = worker.attempt_generation();
        assert_eq!(
            lock_lifecycle(&engine.lifecycle).state,
            LifecycleState::Idle
        );
        engine.active_lease = Some(ActiveLease {
            binding,
            start_id: "retiring-worker-start".into(),
            published_baseline: None,
        });
        engine.external_session = Some(ExternalSession {
            worker,
            paused: Arc::new(AtomicBool::new(false)),
            running: Arc::new(AtomicBool::new(false)),
            presented,
            thread: Some(bridge),
            presentation_drops: Arc::new(AtomicU64::new(0)),
        });
        let status = |engine: &Engine| {
            engine.media_status(serde_json::from_value(json!({
                "id":"retirement-status","type":"media-status","protocolVersion":crate::PROTOCOL_VERSION,
            })).unwrap()).unwrap().remove(0)
        };
        assert!(engine.needs_lifecycle_poll());
        assert!(engine.native_session_occupied());
        assert_eq!(status(&engine)["nativeIdle"], false);
        assert_eq!(status(&engine)["active"]["leaseId"], "fixture-lease");
        assert_eq!(
            PackagePin::try_exclusive(&root).unwrap_err().code,
            "plugin_in_use"
        );

        let ownership = lock(&worker_state.process);
        let (poll_started, polling) = mpsc::sync_channel(0);
        let owner = thread::spawn(move || {
            poll_started.send(()).unwrap();
            engine.poll_lifecycle();
            engine
        });
        polling.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(matches!(
            emitted.recv_timeout(Duration::from_millis(50)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        drop(ownership);
        drop(worker_state);
        let mut engine = owner.join().unwrap();

        assert!(retained_worker.upgrade().is_none());
        assert!(engine.external_session.is_none());
        assert!(engine.active_lease.is_none());
        assert!(!engine.native_session_occupied());
        assert!(!engine.needs_lifecycle_poll());
        assert!(PackagePin::try_exclusive(&root).is_ok());
        assert_eq!(status(&engine)["nativeIdle"], true);
        assert!(status(&engine)["active"].is_null());
        let events: Vec<_> = emitted.try_iter().collect();
        let stopped: Vec<_> = events
            .iter()
            .filter(|event| event["type"] == "status" && event["status"] == "stopped")
            .collect();
        assert_eq!(stopped.len(), 1);
        assert_eq!(stopped[0]["leaseId"], "fixture-lease");
        assert_eq!(stopped[0]["startId"], "retiring-worker-start");
        engine.poll_lifecycle();
        assert!(emitted.try_recv().is_err());
    }

    fn exercise_transport_gap(gap_has_keyframe: bool) {
        use super::super::Bridge;
        use crate::{EventSender, Lifecycle, State as LifecycleState};
        use opennow_media_protocol::{SourceStamp, wire::MediaHeader};
        use opennow_streamer_hid::HidRuntime;
        use opennow_streamer_platform::{MediaFeedback, MediaStreamConfig, create_test_runtime};
        use opennow_streamer_protocol::{RecordingCompletion, RecordingCutReason};
        use std::collections::BTreeMap;
        use std::sync::atomic::AtomicU64;
        use std::sync::mpsc::{self, Receiver};

        fn pump_through_telemetry(
            bridge: &mut Bridge,
            events: Receiver<serde_json::Value>,
        ) -> Receiver<serde_json::Value> {
            bridge.last_telemetry = Instant::now() - Duration::from_secs(2);
            bridge.running.store(true, Ordering::Release);
            let running = Arc::clone(&bridge.running);
            let observer = thread::spawn(move || {
                let event = events.recv_timeout(Duration::from_secs(2));
                running.store(false, Ordering::Release);
                (events, event)
            });
            let result = bridge.pump();
            let (events, event) = observer.join().unwrap();
            result.unwrap();
            let event = event.unwrap();
            assert_eq!(event["type"], "telemetry");
            assert_eq!(event["transport"], "provider-worker");
            events
        }

        let (temp, package) = package("idle");
        let boot = bootstrap();
        let worker = Arc::new(WorkerSession {
            state: Arc::new(State {
                process: Mutex::new(None),
                stopped: AtomicBool::new(false),
                channels: Mutex::new(Channels::new(boot.limits.clone())),
                attempt: boot.attempt_generation,
                maximum_control: boot.limits.max_control_message_bytes as usize,
                _package: package,
                data_root: data_root(&temp),
            }),
            supervisor: None,
        });
        let frame = |sender_frame_id, keyframe, contiguous| MediaFrame {
            header: MediaHeader {
                attempt_generation: boot.attempt_generation,
                track_id: VIDEO_TRACK_ID,
                payload_bytes: 1,
                source: SourceStamp {
                    sender_frame_id: Some(sender_frame_id),
                    timestamp: sender_frame_id * 1800,
                    clock_rate_hz: 90_000,
                    ssrc: Some(41),
                },
                keyframe,
                contiguous,
            },
            payload: vec![1],
        };
        lock(&worker.state.channels)
            .push_media(frame(1, true, true))
            .unwrap();
        assert!(worker.recv_video().unwrap().header.keyframe);
        lock(&worker.state.channels)
            .push_media(frame(2, gap_has_keyframe, false))
            .unwrap();
        let gap_keyframe = worker.recv_video();
        assert_eq!(gap_keyframe.is_some(), gap_has_keyframe);
        let mut transport_requests = 0;
        if let Some(message) = lock(&worker.state.channels).outgoing() {
            assert!(matches!(
                message,
                ControlMessage::Keyframe {
                    attempt_generation: 7,
                    track_id: VIDEO_TRACK_ID
                }
            ));
            transport_requests += 1;
        }
        assert_eq!(transport_requests, usize::from(!gap_has_keyframe));

        let (host, runtime) = create_test_runtime();
        let (feedback_tx, feedback) = mpsc::channel();
        let session = runtime
            .start(feedback_tx.clone(), MediaStreamConfig::default())
            .unwrap();
        let (_, recording) = session.control().subscribe_recording().unwrap();
        let (events_tx, events) = mpsc::channel();
        let (_presented_tx, presented) = mpsc::channel();
        let mut bridge = Bridge {
            worker: Arc::clone(&worker),
            runtime: runtime.clone(),
            sink: session.sink(),
            media: session.control(),
            input: runtime.captured_input(),
            feedback,
            presented,
            paused: Arc::new(AtomicBool::new(false)),
            running: Arc::new(AtomicBool::new(true)),
            output: EventSender::unbounded(events_tx),
            lifecycle: Arc::new(Mutex::new(Lifecycle {
                state: LifecycleState::Connected,
                context: None,
                generation: 7,
            })),
            hid: Arc::new(HidRuntime::new()),
            generation: 7,
            start_id: "transport-gap-test".into(),
            binding: serde_json::to_value(&boot.binding).unwrap(),
            accepted: boot.accepted,
            limits: boot.limits,
            input_ready: false,
            last_paused: false,
            pending: BTreeMap::new(),
            next_sequence: 1,
            retired_input: 0,
            text: None,
            started: Instant::now(),
            last_telemetry: Instant::now(),
            accepted_frames: 0,
            accepted_bytes: 0,
            presented_frames: 0,
            started_playback: false,
            presentation_drops: Arc::new(AtomicU64::new(0)),
            last_decoded: None,
            video_recovery: None,
        };
        let events = pump_through_telemetry(&mut bridge, events);
        while let Some(message) = lock(&worker.state.channels).outgoing() {
            assert!(matches!(
                message,
                ControlMessage::Keyframe {
                    attempt_generation: 7,
                    track_id: VIDEO_TRACK_ID
                }
            ));
            transport_requests += 1;
        }
        assert!(matches!(
            recording.recv(),
            Err(RecordingCompletion::Cut {
                reason: RecordingCutReason::Discontinuity
            })
        ));
        drop(recording);

        let keyframe = gap_keyframe.unwrap_or_else(|| {
            lock(&worker.state.channels)
                .push_media(frame(3, true, true))
                .unwrap();
            worker.recv_video().unwrap()
        });
        assert!(keyframe.header.keyframe);
        assert!(!keyframe.header.contiguous);
        let keyframe_id = keyframe.header.source.sender_frame_id.unwrap();
        bridge.push(keyframe, true).unwrap();
        assert!(
            matches!(bridge.feedback.recv_timeout(Duration::from_secs(2)).unwrap(),
            MediaFeedback::VideoFrameAccepted { provenance, keyframe: true, .. }
                if provenance.source.unwrap().sender_frame_id == Some(keyframe_id))
        );
        lock(&worker.state.channels)
            .push_media(frame(4, false, true))
            .unwrap();
        let delta = worker.recv_video().unwrap();
        assert!(delta.header.contiguous);
        bridge.push(delta, true).unwrap();
        assert!(
            matches!(bridge.feedback.recv_timeout(Duration::from_secs(2)).unwrap(),
            MediaFeedback::VideoFrameAccepted { provenance, keyframe: false, .. }
                if provenance.source.unwrap().sender_frame_id == Some(4))
        );
        assert!(lock(&worker.state.channels).outgoing().is_none());

        feedback_tx
            .send(MediaFeedback::RequestKeyframe {
                mid: "video".into(),
                reason: "independent decoder recovery".into(),
            })
            .unwrap();
        let _events = pump_through_telemetry(&mut bridge, events);
        assert!(matches!(
            lock(&worker.state.channels).outgoing(),
            Some(ControlMessage::Keyframe {
                attempt_generation: 7,
                track_id: VIDEO_TRACK_ID
            })
        ));
        assert!(lock(&worker.state.channels).outgoing().is_none());
        drop(bridge);
        session.stop();
        runtime.shutdown();
        host.join().unwrap();
        assert_eq!(
            transport_requests,
            usize::from(!gap_has_keyframe),
            "Bridge duplicated the transport owner's keyframe request"
        );
    }

    #[test]
    fn transport_gap_bridge_does_not_repeat_an_already_drained_keyframe_request() {
        exercise_transport_gap(false);
    }

    #[test]
    fn transport_gap_with_a_fresh_keyframe_needs_no_additional_request() {
        exercise_transport_gap(true);
    }

    #[test]
    fn recording_start_requests_one_worker_keyframe_without_invalidating_media() {
        use super::super::ExternalSession;
        use crate::{Engine, State as LifecycleState, lock_lifecycle};
        use opennow_streamer_platform::{
            EncodedFrame, MediaCodec, MediaFeedback, MediaStreamConfig, create_test_runtime,
        };
        use std::sync::atomic::AtomicU64;
        use std::sync::mpsc;

        let (temp, package) = package("idle");
        let boot = bootstrap();
        let worker = Arc::new(WorkerSession {
            state: Arc::new(State {
                process: Mutex::new(None),
                stopped: AtomicBool::new(false),
                channels: Mutex::new(Channels::new(boot.limits.clone())),
                attempt: boot.attempt_generation,
                maximum_control: boot.limits.max_control_message_bytes as usize,
                _package: package,
                data_root: data_root(&temp),
            }),
            supervisor: None,
        });
        let (host, runtime) = create_test_runtime();
        let (events, _received) = mpsc::channel();
        let mut engine = Engine::with_media_runtime(events, runtime.clone());
        let (feedback, feedback_rx) = mpsc::channel();
        let session = runtime.start(feedback, MediaStreamConfig {
            width: 64,
            height: 64,
            audio_enabled: false,
            ..Default::default()
        }).unwrap();
        let sink = session.sink();
        engine.media_session = Some(session);
        lock_lifecycle(&engine.lifecycle).state = LifecycleState::Connected;
        let (presented, _presented_rx) = mpsc::sync_channel(1);
        engine.external_session = Some(ExternalSession {
            worker: Arc::clone(&worker),
            paused: Arc::new(AtomicBool::new(false)),
            running: Arc::new(AtomicBool::new(true)),
            presented,
            thread: None,
            presentation_drops: Arc::new(AtomicU64::new(0)),
        });
        let mut encoder = openh264::encoder::Encoder::new().unwrap();
        let picture = openh264::formats::YUVBuffer::new(64, 64);
        let mut frame = EncodedFrame {
            provenance: Default::default(),
            mid: "video".into(),
            codec: MediaCodec::H264,
            data: Arc::from(encoder.encode(&picture).unwrap().to_vec()),
            frame_index: None,
            timestamp: 90_000,
            clock_rate_hz: 90_000,
            ssrc: None,
            keyframe: true,
            contiguous: true,
        };
        sink.push(frame.clone());
        assert!(matches!(feedback_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            MediaFeedback::VideoFrameAccepted { keyframe: true, .. }));
        let output = temp.path().join("mid-gop.mkv");
        let (started, _) = engine.handle(serde_json::from_value(json!({
            "id":"mid-gop-start","type":"recording-start","outputPath":output
        })).unwrap());
        assert_eq!(started[0]["type"], "recording-started");
        let requested = lock(&worker.state.channels).outgoing();
        assert!(lock(&worker.state.channels).outgoing().is_none());
        let (duplicate, _) = engine.handle(serde_json::from_value(json!({
            "id":"duplicate-start","type":"recording-start","outputPath":temp.path().join("duplicate.mkv")
        })).unwrap());
        assert_eq!(duplicate[0]["code"], "recording-already-active");
        assert!(lock(&worker.state.channels).outgoing().is_none());
        frame.data = Arc::from(encoder.encode(&picture).unwrap().to_vec());
        frame.timestamp += 1800;
        frame.keyframe = false;
        sink.push(frame.clone());
        assert!(matches!(feedback_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            MediaFeedback::VideoFrameAccepted { keyframe: false, .. }));
        encoder.force_intra_frame();
        frame.data = Arc::from(encoder.encode(&picture).unwrap().to_vec());
        frame.timestamp += 1800;
        frame.keyframe = true;
        sink.push(frame);
        assert!(matches!(feedback_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            MediaFeedback::VideoFrameAccepted { keyframe: true, .. }));
        let (stopped, _) = engine.handle(serde_json::from_value(json!({
            "id":"record-stop","type":"recording-stop"
        })).unwrap());
        assert_eq!(stopped[0]["type"], "recording-stopped");
        assert_eq!(stopped[0]["completion"]["kind"], "complete");
        assert_eq!(stopped[0]["videoPackets"], 1);
        assert!(output.is_file());
        engine.stop("recording test complete");
        runtime.shutdown();
        host.join().unwrap();
        assert!(matches!(requested, Some(ControlMessage::Keyframe {
            attempt_generation: 7, track_id: VIDEO_TRACK_ID,
        })), "recording must request a fresh worker keyframe");
    }

    #[cfg(unix)]
    #[test]
    fn host_data_root_rejects_nonprivate_noncanonical_and_linked_directories() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let private = root.join("private");
        fs::create_dir(&private).unwrap();
        fs::set_permissions(&private, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(validate_data_root(&private).unwrap(), private);
        assert!(validate_data_root(Path::new("private")).is_err());
        assert!(validate_data_root(&private.join("..")).is_err());
        let linked = root.join("linked");
        symlink(&private, &linked).unwrap();
        assert!(validate_data_root(&linked).is_err());
        fs::create_dir(private.join("child")).unwrap();
        fs::set_permissions(private.join("child"), fs::Permissions::from_mode(0o700)).unwrap();
        assert!(validate_data_root(&linked.join("child")).is_err());
        fs::set_permissions(&private, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(validate_data_root(&private).is_err());
        fs::write(root.join("file"), b"not a directory").unwrap();
        assert!(validate_data_root(&root.join("file")).is_err());
        assert!(validate_data_root(&root.join("missing")).is_err());
    }

    #[test]
    fn flooded_bulk_pipe_keeps_input_neutral_keyframe_and_stop_responsive() {
        let (_temp, package) = package("flood");
        let mut session = WorkerSession::spawn(package, &data_root(&_temp), bootstrap()).unwrap();
        event_until(&session, Duration::from_secs(4), |event| {
            matches!(event, WorkerEvent::Ready { .. })
        });
        thread::sleep(Duration::from_millis(50));
        for (message, expected) in [
            (
                ControlMessage::Input {
                    attempt_generation: 7,
                    sequence: 1,
                    captured_us: 0,
                    event: InputEvent::Key {
                        virtual_key: 65,
                        modifiers: 0,
                        pressed: true,
                    },
                },
                AckKind::Input,
            ),
            (
                ControlMessage::Neutral {
                    attempt_generation: 7,
                    sequence: 2,
                },
                AckKind::Neutral,
            ),
            (
                ControlMessage::Keyframe {
                    attempt_generation: 7,
                    track_id: 1,
                },
                AckKind::Keyframe,
            ),
            (
                ControlMessage::Stop {
                    attempt_generation: 7,
                    sequence: 3,
                },
                AckKind::Stop,
            ),
        ] {
            session.send(message).unwrap();
            event_until(&session, Duration::from_secs(1), |event| {
                matches!(event,
                WorkerEvent::Control(ControlMessage::Ack { kind, .. }) if std::mem::discriminant(kind) == std::mem::discriminant(&expected))
            });
        }
        let deadline = Instant::now() + Duration::from_secs(1);
        let frame = loop {
            if let Some(frame) = session.recv_video() {
                break frame;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        };
        let source = frame.header.source;
        assert!(source.sender_frame_id.unwrap() > u64::from(u32::MAX));
        assert_eq!(
            source.timestamp,
            u64::MAX - (u64::MAX - source.sender_frame_id.unwrap()) * 90
        );
        assert_eq!(source.ssrc, Some(u32::MAX));
        let stop = Instant::now();
        session.reap();
        assert!(stop.elapsed() < Duration::from_millis(500));
        assert!(session.pid().is_none());
    }

    #[test]
    fn cancellation_during_spawn_never_loses_child_registration() {
        let (_temp, package) = package("unread");
        for _ in 0..20 {
            let mut session =
                WorkerSession::spawn(package.clone(), &data_root(&_temp), bootstrap()).unwrap();
            session.signal_stop();
            session.reap();
            assert!(session.pid().is_none());
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn stop_kills_descendants_before_reaping_the_group_leader() {
        let (_temp, package) = package("descendants");
        let mut session = WorkerSession::spawn(package, &data_root(&_temp), bootstrap()).unwrap();
        let event = event_until(&session, Duration::from_secs(4), |event| {
            matches!(event, WorkerEvent::Control(ControlMessage::Ack { .. }))
        });
        let WorkerEvent::Control(ControlMessage::Ack {
            sequence: child, ..
        }) = event
        else {
            unreachable!()
        };
        assert!(fs::read_to_string(format!("/proc/{child}/stat")).is_ok());
        session.reap();
        let deadline = Instant::now() + Duration::from_millis(500);
        loop {
            match fs::read_to_string(format!("/proc/{child}/stat")) {
                Err(_) => break,
                Ok(stat) if stat.split(") ").nth(1).unwrap().starts_with('Z') => break,
                _ => {
                    assert!(Instant::now() < deadline);
                    thread::sleep(Duration::from_millis(2));
                }
            }
        }
        assert!(session.pid().is_none());
    }
}

#[cfg(windows)]
struct BootstrapWriter {
    writer: Option<JoinHandle<io::Result<()>>>,
}

#[cfg(windows)]
impl BootstrapWriter {
    fn new(mut pipe: ChildStdin, mut bytes: Vec<u8>) -> io::Result<Self> {
        let writer = thread::Builder::new()
            .name("provider-private-bootstrap".into())
            .spawn(move || {
                let result = pipe.write_all(&bytes);
                bytes.fill(0);
                result
            })?;
        Ok(Self {
            writer: Some(writer),
        })
    }

    fn poll(&mut self) -> io::Result<()> {
        if self.writer.as_ref().is_some_and(JoinHandle::is_finished) {
            return self
                .writer
                .take()
                .unwrap()
                .join()
                .map_err(|_| io::Error::other("Bootstrap writer failed"))?;
        }
        Ok(())
    }
}

#[cfg(windows)]
impl Drop for BootstrapWriter {
    fn drop(&mut self) {
        use std::os::windows::io::AsRawHandle;
        if let Some(writer) = self.writer.take() {
            while !writer.is_finished() {
                unsafe {
                    windows_sys::Win32::System::IO::CancelSynchronousIo(
                        writer.as_raw_handle().cast(),
                    );
                }
                thread::sleep(Duration::from_millis(1));
            }
            let _ = writer.join();
        }
    }
}
