//! hidraw reader/writer thread for generic raw-HID forwarding.
//!
//! One `opennow-hidraw` thread owns every forwarded `/dev/hidrawN` descriptor.
//! It verifies the node's sysfs `HID_ID` before opening, polls the open
//! descriptors with a short timeout, pushes each input report into the
//! [`HidRuntime`] generic lane and drains the runtime's output mailbox back
//! into `write(2)`. No ioctls, no hidapi, no libudev: the engine already
//! enumerated the devices through sysfs.

use std::ffi::{CString, c_void};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use opennow_streamer_hid::{GenericAdmission, HidRuntime};

const POLL_INTERVAL_MS: libc::c_int = 10;
const READ_BUFFER_BYTES: usize = 256;
const WRITE_ERROR_LOG_HEAD: u64 = 5;
const WRITE_ERROR_LOG_EVERY: u64 = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HidrawTarget {
    pub request_id: u8,
    pub incarnation: u64,
    pub path: String,
    /// `HID_ID=` value of `/sys/class/hidraw/<node>/device/uevent`, e.g.
    /// `0003:00000738:00002221`; the node is skipped when it no longer matches.
    pub expected_hid_id: String,
    pub interface: u8,
}

pub struct HidrawForwarder {
    stop_fd: libc::c_int,
    worker: Option<JoinHandle<()>>,
    opened: usize,
}

impl HidrawForwarder {
    pub fn start(targets: Vec<HidrawTarget>, runtime: Arc<HidRuntime>) -> Result<Self, String> {
        if targets.is_empty() {
            return Err("no generic HID targets to forward".to_owned());
        }
        let stop_fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
        if stop_fd < 0 {
            return Err(format!(
                "failed to create hidraw wake event: {}",
                std::io::Error::last_os_error()
            ));
        }
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let worker = match thread::Builder::new()
            .name("opennow-hidraw".to_owned())
            .spawn(move || run_hidraw_thread(targets, runtime, stop_fd, ready_sender))
        {
            Ok(worker) => worker,
            Err(error) => {
                unsafe { libc::close(stop_fd) };
                return Err(format!("failed to spawn hidraw thread: {error}"));
            }
        };
        match ready_receiver.recv() {
            Ok(opened) => Ok(Self {
                stop_fd,
                worker: Some(worker),
                opened,
            }),
            Err(_) => {
                let _ = worker.join();
                unsafe { libc::close(stop_fd) };
                Err("hidraw thread exited during initialization".to_owned())
            }
        }
    }

    /// Number of hidraw nodes that were opened successfully at start.
    pub fn open_count(&self) -> usize {
        self.opened
    }
}

impl Drop for HidrawForwarder {
    fn drop(&mut self) {
        let wake = 1_u64;
        unsafe {
            let _ = libc::write(
                self.stop_fd,
                (&wake as *const u64).cast::<c_void>(),
                std::mem::size_of::<u64>(),
            );
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        unsafe { libc::close(self.stop_fd) };
    }
}

struct OpenDevice {
    request_id: u8,
    incarnation: u64,
    fd: libc::c_int,
    path: String,
    first_report_logged: bool,
    write_errors: u64,
}

impl OpenDevice {
    fn close(&mut self) {
        if self.fd >= 0 {
            unsafe { libc::close(self.fd) };
            self.fd = -1;
        }
    }
}

/// Read the `HID_ID=` line of the hidraw node's sysfs uevent.
pub fn sysfs_hid_id(hidraw_path: &str) -> Option<String> {
    let node = std::path::Path::new(hidraw_path).file_name()?.to_str()?;
    let uevent = std::fs::read_to_string(format!("/sys/class/hidraw/{node}/device/uevent")).ok()?;
    uevent
        .lines()
        .find_map(|line| line.strip_prefix("HID_ID="))
        .map(|value| value.trim().to_owned())
}

fn open_target(target: &HidrawTarget) -> Result<libc::c_int, String> {
    match sysfs_hid_id(&target.path) {
        Some(actual) if actual.eq_ignore_ascii_case(&target.expected_hid_id) => {}
        Some(actual) => {
            return Err(format!(
                "HID_ID mismatch expected {} got {actual}",
                target.expected_hid_id
            ));
        }
        None => return Err("sysfs uevent unreadable (node vanished?)".to_owned()),
    }
    let path = CString::new(target.path.as_str()).map_err(|_| "path contains NUL".to_owned())?;
    let fd = unsafe {
        libc::open(
            path.as_ptr(),
            libc::O_RDWR | libc::O_NONBLOCK | libc::O_CLOEXEC,
        )
    };
    if fd >= 0 {
        return Ok(fd);
    }
    let error = std::io::Error::last_os_error();
    let hint = match error.raw_os_error() {
        Some(libc::EACCES) | Some(libc::EPERM) => {
            " (EACCES: install the udev rule for hidraw, e.g. KERNEL==\"hidraw*\", ATTRS{idVendor}==\"0738\", TAG+=\"uaccess\")"
        }
        _ => "",
    };
    Err(format!(
        "errno={} {error}{hint}",
        error.raw_os_error().unwrap_or(0)
    ))
}

fn run_hidraw_thread(
    targets: Vec<HidrawTarget>,
    runtime: Arc<HidRuntime>,
    stop_fd: libc::c_int,
    ready: mpsc::SyncSender<usize>,
) {
    let origin = Instant::now();
    let mut devices: Vec<OpenDevice> = Vec::with_capacity(targets.len());
    for target in &targets {
        let (vendor, product) = split_hid_id(&target.expected_hid_id);
        match open_target(target) {
            Ok(fd) => {
                eprintln!(
                    "generic hid: open {} {vendor:04x}:{product:04x} iface {} requestId={} -> ok fd={fd}",
                    target.path, target.interface, target.request_id
                );
                devices.push(OpenDevice {
                    request_id: target.request_id,
                    incarnation: target.incarnation,
                    fd,
                    path: target.path.clone(),
                    first_report_logged: false,
                    write_errors: 0,
                });
                // The lane was published dormant; only an open node goes live
                // (and thereby gets its attach sent).
                if !runtime.mark_generic_live(target.request_id, target.incarnation) {
                    eprintln!(
                        "generic hid: requestId={} opened but its lane is not dormant (already live or replaced)",
                        target.request_id
                    );
                }
            }
            Err(reason) => {
                eprintln!(
                    "generic hid: open {} {vendor:04x}:{product:04x} iface {} requestId={} -> {reason}; device skipped",
                    target.path, target.interface, target.request_id
                );
                // Never announce a device we cannot read: a dormant lane stays
                // dormant, a live one (legacy path) is withdrawn.
                runtime.mark_generic_removed(target.request_id);
            }
        }
    }
    let _ = ready.send(devices.len());

    let mut buffer = [0_u8; READ_BUFFER_BYTES];
    loop {
        let mut poll_fds: Vec<libc::pollfd> = devices
            .iter()
            .filter(|device| device.fd >= 0)
            .map(|device| libc::pollfd {
                fd: device.fd,
                events: libc::POLLIN,
                revents: 0,
            })
            .collect();
        poll_fds.push(libc::pollfd {
            fd: stop_fd,
            events: libc::POLLIN,
            revents: 0,
        });
        let poll_result = unsafe {
            libc::poll(
                poll_fds.as_mut_ptr(),
                poll_fds.len() as libc::nfds_t,
                POLL_INTERVAL_MS,
            )
        };
        if poll_result < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            break;
        }
        if poll_fds.last().is_some_and(|stop| stop.revents & libc::POLLIN != 0) {
            break;
        }
        for entry in &poll_fds[..poll_fds.len() - 1] {
            let Some(device) = devices
                .iter_mut()
                .find(|device| device.fd == entry.fd && device.fd >= 0)
            else {
                continue;
            };
            if entry.revents & libc::POLLIN != 0 {
                read_reports(device, &runtime, &mut buffer, origin);
            }
            if entry.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                eprintln!(
                    "generic hid: device gone requestId={} revents=0x{:x} ({})",
                    device.request_id, entry.revents, device.path
                );
                device.close();
                runtime.mark_generic_removed(device.request_id);
            }
        }
        for output in runtime.take_generic_outputs() {
            let Some(device) = devices
                .iter_mut()
                .find(|device| device.request_id == output.request_id && device.fd >= 0)
            else {
                runtime.record_generic_write(output.request_id, false);
                continue;
            };
            write_output(device, &runtime, output.interface, &output.bytes);
        }
    }
    for device in &mut devices {
        device.close();
    }
}

fn read_reports(
    device: &mut OpenDevice,
    runtime: &HidRuntime,
    buffer: &mut [u8; READ_BUFFER_BYTES],
    origin: Instant,
) {
    loop {
        let read = unsafe {
            libc::read(
                device.fd,
                buffer.as_mut_ptr().cast::<c_void>(),
                buffer.len(),
            )
        };
        if read < 0 {
            let error = std::io::Error::last_os_error();
            match error.raw_os_error() {
                Some(libc::EAGAIN) => return,
                Some(libc::EINTR) => continue,
                errno => {
                    eprintln!(
                        "generic hid: device gone requestId={} errno={} ({})",
                        device.request_id,
                        errno.unwrap_or(0),
                        device.path
                    );
                    device.close();
                    runtime.mark_generic_removed(device.request_id);
                    return;
                }
            }
        }
        if read == 0 {
            eprintln!(
                "generic hid: device gone requestId={} (EOF on {})",
                device.request_id, device.path
            );
            device.close();
            runtime.mark_generic_removed(device.request_id);
            return;
        }
        let report = &buffer[..read as usize];
        let observed_at_us = origin.elapsed().as_micros() as u64;
        let admission = runtime.submit_generic_report(
            device.request_id,
            device.incarnation,
            observed_at_us,
            report,
        );
        if !device.first_report_logged {
            device.first_report_logged = true;
            eprintln!(
                "generic hid: first report requestId={} bytes={} admission={admission:?} raw={}",
                device.request_id,
                report.len(),
                hex(report, 32)
            );
        }
        if admission == GenericAdmission::Oversize {
            eprintln!(
                "generic hid: report of {} bytes exceeds the forwarding cap; dropped (requestId={})",
                report.len(),
                device.request_id
            );
        }
    }
}

fn write_output(device: &mut OpenDevice, runtime: &HidRuntime, interface: u8, bytes: &[u8]) {
    if bytes.is_empty() {
        runtime.record_generic_write(device.request_id, false);
        return;
    }
    let written = unsafe { libc::write(device.fd, bytes.as_ptr().cast::<c_void>(), bytes.len()) };
    // Capture errno before anything else (mutex, formatting) can clobber it.
    let error = std::io::Error::last_os_error();
    let ok = written >= 0 && written as usize == bytes.len();
    runtime.record_generic_write(device.request_id, ok);
    if ok {
        eprintln!(
            "generic hid: output written requestId={} iface={interface} bytes={}",
            device.request_id,
            bytes.len()
        );
        return;
    }
    device.write_errors = device.write_errors.saturating_add(1);
    if device.write_errors <= WRITE_ERROR_LOG_HEAD
        || device.write_errors % WRITE_ERROR_LOG_EVERY == 0
    {
        eprintln!(
            "generic hid: output write failed requestId={} iface={interface} bytes={} result={written} error={error} (count={})",
            device.request_id,
            bytes.len(),
            device.write_errors
        );
    }
}

fn split_hid_id(hid_id: &str) -> (u16, u16) {
    let mut parts = hid_id.split(':').skip(1);
    let vendor = parts
        .next()
        .and_then(|value| u32::from_str_radix(value, 16).ok())
        .unwrap_or(0) as u16;
    let product = parts
        .next()
        .and_then(|value| u32::from_str_radix(value, 16).ok())
        .unwrap_or(0) as u16;
    (vendor, product)
}

fn hex(bytes: &[u8], limit: usize) -> String {
    let mut value = bytes
        .iter()
        .take(limit)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if bytes.len() > limit {
        value.push_str("...");
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hid_id_splits_into_vendor_and_product() {
        assert_eq!(split_hid_id("0003:00000738:00002221"), (0x0738, 0x2221));
        assert_eq!(split_hid_id("garbage"), (0, 0));
    }

    #[test]
    fn start_refuses_an_empty_target_list() {
        let runtime = Arc::new(HidRuntime::new());
        assert!(HidrawForwarder::start(Vec::new(), runtime).is_err());
    }

    #[test]
    fn unreadable_targets_are_skipped_and_marked_removed_without_failing() {
        let runtime = Arc::new(HidRuntime::new());
        let (devices, _) = runtime.replace_generic_inventory(&[opennow_streamer_hid::GenericHidDevice {
            request_id: 0,
            incarnation: 0,
            identity: Default::default(),
            hidraw_path: "/dev/hidraw-does-not-exist".to_owned(),
            sysfs_hid_id: "0003:00000738:00002221".to_owned(),
            name: "missing".to_owned(),
        }]);
        let forwarder = HidrawForwarder::start(
            vec![HidrawTarget {
                request_id: 0,
                incarnation: devices[0].incarnation,
                path: "/dev/hidraw-does-not-exist".to_owned(),
                expected_hid_id: "0003:00000738:00002221".to_owned(),
                interface: 0,
            }],
            Arc::clone(&runtime),
        )
        .expect("a missing node is skipped, not fatal");
        assert_eq!(forwarder.open_count(), 0);
        assert_eq!(runtime.take_generic_removals(), vec![0]);
        drop(forwarder);
    }
}
