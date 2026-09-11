use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub(super) struct OwnedApplication {
    pub child: std::process::Child,
    pub identity: ProcessIdentity,
    #[cfg(windows)]
    job: Job,
}

impl OwnedApplication {
    pub fn start(command: &mut std::process::Command) -> Result<Self, super::RestartFailure> {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x00000004);
        }
        let mut child = command.spawn().map_err(|error| {
            super::RestartFailure::RollbackSafe(format!(
                "Updated application could not start: {error}"
            ))
        })?;
        #[cfg(windows)]
        let job = match Job::assign(&child) {
            Ok(job) => job,
            Err(error) => {
                return Err(failed_start(&mut child, error));
            }
        };
        let identity = match ProcessIdentity::capture(child.id()) {
            Ok(identity) => identity,
            Err(error) => {
                return Err(failed_start(&mut child, error));
            }
        };
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            #[link(name = "ntdll")]
            unsafe extern "system" {
                fn NtResumeProcess(process: windows_sys::Win32::Foundation::HANDLE) -> i32;
            }
            if unsafe { NtResumeProcess(child.as_raw_handle()) } != 0 {
                return Err(failed_start(
                    &mut child,
                    "Cannot resume the owned updated application".to_owned(),
                ));
            }
        }
        Ok(Self {
            child,
            identity,
            #[cfg(windows)]
            job,
        })
    }

    pub fn owns(&self, process: &ProcessIdentity) -> Result<bool, String> {
        if !process.is_running()? || process.started < self.identity.started {
            return Ok(false);
        }
        #[cfg(unix)]
        {
            Ok(process.group == self.identity.group)
        }
        #[cfg(windows)]
        {
            self.job.contains(process.pid)
        }
    }

    pub fn stop(&mut self) -> Result<(), String> {
        #[cfg(unix)]
        {
            if capture(self.identity.pid)?.is_some_and(|current| {
                current.started != self.identity.started || current.boot_id != self.identity.boot_id
            }) {
                return Err(
                    "Updated application process identity was reused; refusing group shutdown"
                        .to_owned(),
                );
            }
            if unsafe { libc::kill(-(self.identity.group as i32), libc::SIGKILL) } < 0 {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::ESRCH) {
                    return Err(error.to_string());
                }
            }
        }
        #[cfg(windows)]
        self.job.stop()?;
        let start = Instant::now();
        loop {
            let exited = self
                .child
                .try_wait()
                .map_err(|error| error.to_string())?
                .is_some();
            #[cfg(unix)]
            let descendants = group_is_running(self.identity.group)?;
            #[cfg(windows)]
            let descendants = self.job.active()?;
            if exited && !descendants {
                return Ok(());
            }
            if start.elapsed() > Duration::from_secs(10) {
                return Err(
                    "Could not stop all updated application processes before rollback".to_owned(),
                );
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

pub(super) fn failed_start(
    child: &mut std::process::Child,
    error: String,
) -> super::RestartFailure {
    let cleanup = (|| {
        #[cfg(unix)]
        if unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) } < 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) {
                return Err(error.to_string());
            }
        }
        if let (Err(error), None) = (
            child.kill(),
            child.try_wait().map_err(|error| error.to_string())?,
        ) {
            return Err(error.to_string());
        }
        let start = Instant::now();
        loop {
            let exited = child
                .try_wait()
                .map_err(|error| error.to_string())?
                .is_some();
            #[cfg(unix)]
            let descendants = group_is_running(child.id())?;
            #[cfg(windows)]
            let descendants = false;
            if exited && !descendants {
                return Ok(());
            }
            if start.elapsed() >= Duration::from_secs(10) {
                return Err(
                    "Failed startup process tree did not stop before the deadline".to_owned(),
                );
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    })();
    match cleanup {
        Ok(()) => super::RestartFailure::RollbackSafe(error),
        Err(cleanup_error) => super::RestartFailure::ProcessesMayBeRunning(format!(
            "{error}; startup cleanup failed: {cleanup_error}"
        )),
    }
}

#[cfg(target_os = "linux")]
fn group_is_running(group: u32) -> Result<bool, String> {
    for entry in std::fs::read_dir("/proc").map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        if !entry
            .file_name()
            .to_string_lossy()
            .bytes()
            .all(|byte| byte.is_ascii_digit())
        {
            continue;
        }
        let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
            continue;
        };
        let Some((_, fields)) = stat.rsplit_once(')') else {
            continue;
        };
        let fields: Vec<_> = fields.split_whitespace().collect();
        if fields.first() != Some(&"Z")
            && fields.get(2).and_then(|value| value.parse::<u32>().ok()) == Some(group)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(target_os = "macos")]
fn group_is_running(group: u32) -> Result<bool, String> {
    unsafe extern "C" {
        fn proc_listpids(kind: u32, info: u32, buffer: *mut std::ffi::c_void, size: i32) -> i32;
    }
    let mut pids = vec![0u32; 100_000];
    let length =
        unsafe { proc_listpids(2, group, pids.as_mut_ptr().cast(), (pids.len() * 4) as i32) };
    if length < 0 || length as usize >= pids.len() * 4 {
        return Err("Cannot bound the updated application's process group".to_owned());
    }
    for pid in pids
        .into_iter()
        .take(length as usize / 4)
        .filter(|pid| *pid != 0)
    {
        if capture(pid)?.is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(windows)]
struct Job(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(windows)]
impl Job {
    fn assign(child: &std::process::Child) -> Result<Self, String> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::*;
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let job = Self(handle);
        if unsafe { AssignProcessToJobObject(handle, child.as_raw_handle()) } == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(job)
    }
    fn contains(&self, pid: u32) -> Result<bool, String> {
        use windows_sys::Win32::System::{JobObjects::*, Threading::*};
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let mut member = 0;
        let result = unsafe { IsProcessInJob(process, self.0, &mut member) };
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(process);
        }
        if result == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(member != 0)
    }
    fn stop(&self) -> Result<(), String> {
        if unsafe { windows_sys::Win32::System::JobObjects::TerminateJobObject(self.0, 1) } == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(())
    }
    fn active(&self) -> Result<bool, String> {
        use windows_sys::Win32::System::JobObjects::*;
        let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe {
            QueryInformationJobObject(
                self.0,
                JobObjectBasicAccountingInformation,
                (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                std::mem::size_of_val(&info) as u32,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(info.ActiveProcesses != 0)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub started: u64,
    pub executable: PathBuf,
    pub group: u32,
    pub boot_id: String,
}

impl ProcessIdentity {
    pub fn capture(pid: u32) -> Result<Self, String> {
        capture(pid)?.ok_or_else(|| format!("Process {pid} is not running"))
    }

    pub fn matches_executable(&self, executable: &Path) -> Result<(), String> {
        if std::fs::canonicalize(executable).map_err(|error| error.to_string())? != self.executable
        {
            return Err("Application process does not match its trusted executable".to_owned());
        }
        Ok(())
    }

    pub fn is_running(&self) -> Result<bool, String> {
        Ok(capture(self.pid)?.is_some_and(|current| {
            current.started == self.started && current.boot_id == self.boot_id
        }))
    }
}

pub(super) fn wait_for_exit(
    processes: &[ProcessIdentity],
    timeout: Duration,
) -> Result<(), String> {
    let start = Instant::now();
    loop {
        let mut running = false;
        for process in processes {
            running |= process.is_running()?;
        }
        if !running {
            return Ok(());
        }
        if start.elapsed() >= timeout {
            return Err(
                "Timed out waiting for the original application and core to exit".to_owned(),
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

pub(super) fn owned_tree_is_running(identity: &ProcessIdentity) -> Result<bool, String> {
    #[cfg(target_os = "linux")]
    if std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .map_err(|error| error.to_string())?
        .trim()
        != identity.boot_id
    {
        return Ok(false);
    }
    #[cfg(unix)]
    {
        group_is_running(identity.group)
    }
    #[cfg(windows)]
    {
        identity.is_running()
    }
}

#[cfg(target_os = "linux")]
fn capture(pid: u32) -> Result<Option<ProcessIdentity>, String> {
    let path = PathBuf::from(format!("/proc/{pid}"));
    let stat = match std::fs::read_to_string(path.join("stat")) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let fields: Vec<_> = stat
        .rsplit_once(')')
        .ok_or("Invalid process identity")?
        .1
        .split_whitespace()
        .collect();
    if fields.first() == Some(&"Z") {
        return Ok(None);
    }
    let started = fields
        .get(19)
        .ok_or("Missing process start identity")?
        .parse()
        .map_err(|_| "Invalid process start identity")?;
    let executable = match std::fs::read_link(path.join("exe")) {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    Ok(Some(ProcessIdentity {
        pid,
        started,
        executable,
        group: fields
            .get(2)
            .ok_or("Missing process group")?
            .parse()
            .map_err(|_| "Invalid process group")?,
        boot_id: std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
            .map_err(|error| error.to_string())?
            .trim()
            .to_owned(),
    }))
}

#[cfg(target_os = "macos")]
fn capture(pid: u32) -> Result<Option<ProcessIdentity>, String> {
    #[repr(C)]
    #[derive(Default)]
    struct BsdInfo {
        flags: u32,
        status: u32,
        xstatus: u32,
        pid: u32,
        ppid: u32,
        uid: u32,
        gid: u32,
        ruid: u32,
        rgid: u32,
        svuid: u32,
        svgid: u32,
        rfu_1: u32,
        comm: [u8; 16],
        name: [u8; 32],
        nfiles: u32,
        pgid: u32,
        pjobc: u32,
        e_tdev: u32,
        e_tpgid: u32,
        nice: i32,
        start_tvsec: u64,
        start_tvusec: u64,
    }
    unsafe extern "C" {
        fn proc_pidinfo(
            pid: i32,
            flavor: i32,
            arg: u64,
            buffer: *mut std::ffi::c_void,
            size: i32,
        ) -> i32;
        fn proc_pidpath(pid: i32, buffer: *mut std::ffi::c_void, size: u32) -> i32;
    }
    let mut info = BsdInfo::default();
    let length = unsafe {
        proc_pidinfo(
            pid as i32,
            3,
            0,
            (&mut info as *mut BsdInfo).cast(),
            std::mem::size_of::<BsdInfo>() as i32,
        )
    };
    if length == 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ESRCH) {
            return Ok(None);
        }
        return Err(error.to_string());
    }
    if length as usize != std::mem::size_of::<BsdInfo>() {
        return Err("Incomplete macOS process identity".to_owned());
    }
    if info.status == 5 {
        return Ok(None);
    }
    let mut path = [0u8; 4096];
    let length = unsafe { proc_pidpath(pid as i32, path.as_mut_ptr().cast(), path.len() as u32) };
    if length <= 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ESRCH) {
            return Ok(None);
        }
        return Err(error.to_string());
    }
    use std::os::unix::ffi::OsStrExt;
    let end = path
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(length as usize);
    let executable = std::fs::canonicalize(Path::new(std::ffi::OsStr::from_bytes(&path[..end])))
        .map_err(|error| error.to_string())?;
    Ok(Some(ProcessIdentity {
        pid,
        started: info.start_tvsec * 1_000_000 + info.start_tvusec,
        executable,
        group: info.pgid,
        boot_id: String::new(),
    }))
}

#[cfg(windows)]
fn capture(pid: u32) -> Result<Option<ProcessIdentity>, String> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, FILETIME};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
            return Ok(None);
        }
        return Err(error.to_string());
    }
    let result = (|| {
        let mut exit = 0;
        if unsafe { GetExitCodeProcess(handle, &mut exit) } == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        if exit != 259 {
            return Ok(None);
        }
        let mut creation = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let mut end = creation;
        let mut kernel = creation;
        let mut user = creation;
        if unsafe { GetProcessTimes(handle, &mut creation, &mut end, &mut kernel, &mut user) } == 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let mut buffer = vec![0u16; 32768];
        let mut length = buffer.len() as u32;
        if unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut length) } == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let executable = std::fs::canonicalize(PathBuf::from(std::ffi::OsString::from_wide(
            &buffer[..length as usize],
        )))
        .map_err(|error| error.to_string())?;
        Ok(Some(ProcessIdentity {
            pid,
            started: (creation.dwHighDateTime as u64) << 32 | creation.dwLowDateTime as u64,
            executable,
            group: 0,
            boot_id: String::new(),
        }))
    })();
    unsafe {
        CloseHandle(handle);
    }
    result
}
