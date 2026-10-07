use opennow_plugin_api::wire::PluginMessage;
use serde_json::Value;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const FRAME_LIMIT: usize = opennow_plugin_api::MAX_FRAME_BYTES;
const QUEUE_LIMIT: usize = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Failure {
    Spawn,
    Protocol,
    Closed,
    Busy,
}

pub(super) struct Transport {
    child: Mutex<Child>,
    owner: ProcessOwner,
    writes: SyncSender<Vec<u8>>,
    replies: Mutex<Receiver<Vec<u8>>>,
    stopped: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
}

impl Transport {
    pub(super) fn spawn(executable: &Path, data: &Path) -> Result<Arc<Self>, Failure> {
        Self::spawn_checked(executable, data, QUEUE_LIMIT, |frame| {
            serde_json::from_slice::<PluginMessage>(frame).is_ok()
        })
    }

    pub(super) fn spawn_bounded(
        executable: &Path,
        data: &Path,
        queue_limit: usize,
    ) -> Result<Arc<Self>, Failure> {
        Self::spawn_checked(executable, data, queue_limit, |frame| {
            serde_json::from_slice::<Value>(frame).is_ok()
        })
    }

    fn spawn_checked(
        executable: &Path,
        data: &Path,
        queue_limit: usize,
        valid_frame: fn(&[u8]) -> bool,
    ) -> Result<Arc<Self>, Failure> {
        let mut command = Command::new(executable);
        command
            .env_clear()
            .env("OPENNOW_PLUGIN_DATA_DIR", data)
            .current_dir(data)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        configure_command(&mut command);
        let mut child = command.spawn().map_err(|_| Failure::Spawn)?;
        let owner = match ProcessOwner::attach(&child) {
            Ok(owner) => owner,
            Err(error) => {
                let _ = child.kill();
                reap_spawn_failure(&mut child);
                return Err(error);
            }
        };
        let stdin = child.stdin.take().ok_or(Failure::Spawn)?;
        let mut stdout = child.stdout.take().ok_or(Failure::Spawn)?;
        let mut stderr = child.stderr.take().ok_or(Failure::Spawn)?;
        if configure_pipe(&stdin).is_err()
            || configure_pipe(&stdout).is_err()
            || configure_pipe(&stderr).is_err()
        {
            owner.signal(&mut child);
            reap_spawn_failure(&mut child);
            return Err(Failure::Spawn);
        }
        let stopped = Arc::new(AtomicBool::new(false));
        let failed = Arc::new(AtomicBool::new(false));
        let (writes, writer_rx) = mpsc::sync_channel(queue_limit);
        let (reader_tx, replies) = mpsc::sync_channel(queue_limit);
        let state = Arc::new(Self {
            child: Mutex::new(child),
            owner,
            writes,
            replies: Mutex::new(replies),
            stopped,
            failed,
        });
        let stop = Arc::clone(&state.stopped);
        let failure = Arc::clone(&state.failed);
        if thread::Builder::new()
            .name("plugin-stdin".into())
            .spawn(move || {
                write_loop(stdin, writer_rx, &stop, &failure);
            })
            .is_err()
        {
            state.terminate();
            return Err(Failure::Spawn);
        }
        let stop = Arc::clone(&state.stopped);
        let failure = Arc::clone(&state.failed);
        if thread::Builder::new()
            .name("plugin-stdout".into())
            .spawn(move || {
                let mut frame = Vec::new();
                let mut buffer = [0u8; 8192];
                while !stop.load(Ordering::Acquire) {
                    let count = match read_pipe(&mut stdout, &mut buffer) {
                        Ok(0) => {
                            failure.store(true, Ordering::Release);
                            return;
                        }
                        Ok(count) => count,
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(5));
                            continue;
                        }
                        Err(_) => {
                            failure.store(true, Ordering::Release);
                            return;
                        }
                    };
                    for byte in &buffer[..count] {
                        if *byte == b'\n' {
                            let valid = valid_frame(&frame);
                            if reader_tx.try_send(std::mem::take(&mut frame)).is_err() || !valid {
                                failure.store(true, Ordering::Release);
                                return;
                            }
                        } else if frame.len() == FRAME_LIMIT {
                            failure.store(true, Ordering::Release);
                            return;
                        } else {
                            frame.push(*byte);
                        }
                    }
                }
            })
            .is_err()
        {
            state.terminate();
            return Err(Failure::Spawn);
        }
        let stop = Arc::clone(&state.stopped);
        if thread::Builder::new()
            .name("plugin-stderr".into())
            .spawn(move || {
                let mut buffer = [0u8; 4096];
                while !stop.load(Ordering::Acquire) {
                    match read_pipe(&mut stderr, &mut buffer) {
                        Ok(0) => return,
                        Ok(_) => {}
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(5))
                        }
                        Err(_) => return,
                    }
                }
            })
            .is_err()
        {
            state.terminate();
            return Err(Failure::Spawn);
        }
        Ok(state)
    }

    pub(super) fn send(&self, value: &Value) -> Result<(), Failure> {
        if self.stopped.load(Ordering::Acquire) || self.failed.load(Ordering::Acquire) {
            return Err(Failure::Closed);
        }
        let mut bytes = serde_json::to_vec(value).map_err(|_| Failure::Protocol)?;
        if bytes.len() > FRAME_LIMIT {
            return Err(Failure::Protocol);
        }
        bytes.push(b'\n');
        self.writes.try_send(bytes).map_err(|error| match error {
            TrySendError::Full(_) => Failure::Busy,
            TrySendError::Disconnected(_) => Failure::Closed,
        })
    }

    pub(super) fn receive(&self, timeout: Duration) -> Result<Option<PluginMessage>, Failure> {
        self.receive_frame(timeout)?
            .map(|frame| serde_json::from_slice(&frame).map_err(|_| Failure::Protocol))
            .transpose()
    }

    pub(super) fn receive_frame(&self, timeout: Duration) -> Result<Option<Vec<u8>>, Failure> {
        let replies = self.replies.lock().unwrap_or_else(|e| e.into_inner());
        match replies.try_recv() {
            Ok(frame) => return Ok(Some(frame)),
            Err(mpsc::TryRecvError::Disconnected) => return Err(Failure::Closed),
            Err(mpsc::TryRecvError::Empty) => {}
        }
        if self.stopped.load(Ordering::Acquire) || self.failed.load(Ordering::Acquire) {
            return Err(Failure::Closed);
        }
        match replies.recv_timeout(timeout) {
            Ok(value) => Ok(Some(value)),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(Failure::Closed),
        }
    }

    pub(super) fn unhealthy(&self) -> bool {
        self.failed.load(Ordering::Acquire)
            || child_exited(&mut self.child.lock().unwrap_or_else(|e| e.into_inner()))
    }

    pub(super) fn terminate(&self) {
        self.signal_termination();
        self.reap_until(Instant::now() + Duration::from_secs(2));
    }

    pub(super) fn signal_termination(&self) {
        let mut child = self.child.lock().unwrap_or_else(|e| e.into_inner());
        if !self.stopped.load(Ordering::Acquire) {
            self.owner.signal(&mut child);
            self.stopped.store(true, Ordering::Release);
        }
    }

    pub(super) fn reap_until(&self, deadline: Instant) {
        while Instant::now() < deadline {
            let result = self
                .child
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .try_wait();
            match result {
                Ok(Some(_)) | Err(_) => return,
                Ok(None) => thread::sleep(Duration::from_millis(5)),
            }
        }
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        self.signal_termination();
        self.reap_until(Instant::now() + Duration::from_millis(50));
    }
}

fn reap_spawn_failure(child: &mut Child) {
    let deadline = Instant::now() + Duration::from_millis(100);
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(_)) | Err(_) => return,
            Ok(None) => thread::sleep(Duration::from_millis(5)),
        }
    }
}

fn write_loop(
    mut stdin: ChildStdin,
    receiver: Receiver<Vec<u8>>,
    stop: &AtomicBool,
    failed: &AtomicBool,
) {
    while !stop.load(Ordering::Acquire) {
        let bytes = match receiver.recv_timeout(Duration::from_millis(20)) {
            Ok(bytes) => bytes,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(_) => return,
        };
        let mut offset = 0;
        while offset < bytes.len() && !stop.load(Ordering::Acquire) {
            match stdin.write(&bytes[offset..]) {
                Ok(0) => {
                    failed.store(true, Ordering::Release);
                    return;
                }
                Ok(count) => offset += count,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5))
                }
                Err(_) => {
                    failed.store(true, Ordering::Release);
                    return;
                }
            }
        }
    }
}

#[cfg(unix)]
fn configure_command(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
    command.env("LANG", "C.UTF-8");
}

#[cfg(windows)]
fn configure_command(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x08000000);
    if let Some(root) = std::env::var_os("SystemRoot") {
        command.env("SystemRoot", root);
    }
}

#[cfg(unix)]
fn configure_pipe(pipe: &impl std::os::fd::AsRawFd) -> io::Result<()> {
    let fd = pipe.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(windows)]
fn configure_pipe(_pipe: &impl std::os::windows::io::AsRawHandle) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn read_pipe(pipe: &mut impl Read, buffer: &mut [u8]) -> io::Result<usize> {
    pipe.read(buffer)
}

#[cfg(windows)]
fn read_pipe(
    pipe: &mut (impl Read + std::os::windows::io::AsRawHandle),
    buffer: &mut [u8],
) -> io::Result<usize> {
    use windows_sys::Win32::System::Pipes::PeekNamedPipe;
    let mut available = 0;
    let ok = unsafe {
        PeekNamedPipe(
            pipe.as_raw_handle(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            &mut available,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    if available == 0 {
        return Err(io::ErrorKind::WouldBlock.into());
    }
    let limit = buffer.len().min(available as usize);
    pipe.read(&mut buffer[..limit])
}

#[cfg(unix)]
struct ProcessOwner(u32);

#[cfg(unix)]
fn child_exited(child: &mut Child) -> bool {
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    if unsafe {
        libc::waitid(
            libc::P_PID,
            child.id(),
            &mut info,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    } != 0
    {
        return true;
    }
    #[cfg(target_os = "macos")]
    {
        info.si_pid != 0
    }
    #[cfg(not(target_os = "macos"))]
    {
        unsafe { info.si_pid() != 0 }
    }
}

#[cfg(windows)]
fn child_exited(child: &mut Child) -> bool {
    child.try_wait().map_or(true, |status| status.is_some())
}

#[cfg(unix)]
impl ProcessOwner {
    fn attach(child: &Child) -> Result<Self, Failure> {
        Ok(Self(child.id()))
    }
    fn signal(&self, child: &mut Child) {
        unsafe {
            libc::kill(-(self.0 as i32), libc::SIGKILL);
        }
        let _ = child.kill();
    }
}

#[cfg(windows)]
struct ProcessOwner(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
unsafe impl Send for ProcessOwner {}
#[cfg(windows)]
unsafe impl Sync for ProcessOwner {}

#[cfg(windows)]
impl ProcessOwner {
    fn attach(child: &Child) -> Result<Self, Failure> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::JobObjects::*;
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return Err(Failure::Spawn);
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of_val(&info) as u32,
            )
        } == 0
            || unsafe { AssignProcessToJobObject(job, child.as_raw_handle()) } == 0
        {
            unsafe {
                CloseHandle(job);
            }
            return Err(Failure::Spawn);
        }
        Ok(Self(job))
    }
    fn signal(&self, child: &mut Child) {
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(self.0, 1);
        }
        let _ = child.kill();
    }
}

#[cfg(windows)]
impl Drop for ProcessOwner {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
