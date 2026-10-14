//! Platform entry point for generic raw-HID forwarding.
//!
//! Linux owns the only implementation (a hidraw reader thread). Every other
//! target compiles the same API and reports the lane as unavailable so the
//! engine can log one line and continue without it.

use std::sync::Arc;

use opennow_streamer_hid::{GenericHidDevice, HidRuntime};

pub struct GenericHidForwarder {
    #[cfg(target_os = "linux")]
    _inner: opennow_streamer_platform_linux::HidrawForwarder,
    opened: usize,
}

impl GenericHidForwarder {
    /// Start forwarding `devices` (already entered into `runtime` through
    /// `replace_generic_inventory`, so each carries its incarnation).
    pub fn start(devices: &[GenericHidDevice], runtime: Arc<HidRuntime>) -> Result<Self, String> {
        #[cfg(target_os = "linux")]
        {
            let targets = devices
                .iter()
                .map(|device| opennow_streamer_platform_linux::HidrawTarget {
                    request_id: device.request_id,
                    incarnation: device.incarnation,
                    path: device.hidraw_path.clone(),
                    expected_hid_id: device.sysfs_hid_id.clone(),
                    interface: device.identity.interface,
                })
                .collect();
            let inner = opennow_streamer_platform_linux::HidrawForwarder::start(targets, runtime)?;
            Ok(Self {
                opened: inner.open_count(),
                _inner: inner,
            })
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (devices, runtime);
            Err("generic HID forwarding is Linux-only".to_owned())
        }
    }

    /// Device nodes that were opened successfully.
    pub fn open_count(&self) -> usize {
        self.opened
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn forwarding_is_reported_unavailable_off_linux() {
        let runtime = Arc::new(HidRuntime::new());
        let error = GenericHidForwarder::start(&[], runtime).unwrap_err();
        assert!(error.contains("Linux-only"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn an_empty_device_list_does_not_start_a_thread() {
        let runtime = Arc::new(HidRuntime::new());
        assert!(GenericHidForwarder::start(&[], runtime).is_err());
    }
}
