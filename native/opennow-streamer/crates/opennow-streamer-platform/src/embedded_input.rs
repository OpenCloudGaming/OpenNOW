use std::sync::Arc;
#[cfg(any(target_os = "linux", target_os = "windows"))]
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::{CapturedInput, CapturedInputQueue};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddedLocalAction {
    Guide,
    Screenshot,
    RecordingToggle,
}

pub struct EmbeddedInputCapture {
    queue: Arc<CapturedInputQueue>,
    active: AtomicBool,
    #[cfg(target_os = "linux")]
    raw: Mutex<Option<crate::linux_xinput::LinuxXInputController>>,
    #[cfg(target_os = "windows")]
    raw: Mutex<Option<crate::windows_raw_input::WindowsRawInputController>>,
}

impl EmbeddedInputCapture {
    pub fn new(queue: Arc<CapturedInputQueue>) -> Self {
        Self {
            queue,
            active: AtomicBool::new(false),
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            raw: Mutex::new(None),
        }
    }

    pub fn submit(&self, input: CapturedInput) {
        if self.active.load(Ordering::Acquire) {
            self.queue.push(input);
        }
    }

    pub fn submit_local_action(&self, action: EmbeddedLocalAction) {
        self.submit(match action {
            EmbeddedLocalAction::Guide => CapturedInput::Guide,
            EmbeddedLocalAction::Screenshot => CapturedInput::Screenshot,
            EmbeddedLocalAction::RecordingToggle => CapturedInput::RecordingToggle,
        });
    }

    pub fn set_active(&self, active: bool, relative_mouse: bool, window_handle: usize) -> bool {
        if !active {
            self.active.store(false, Ordering::Release);
        } else {
            self.active.store(true, Ordering::Release);
        }

        #[cfg(target_os = "linux")]
        {
            let _ = window_handle;
            let raw_enabled = raw_capture_enabled(active, relative_mouse);
            let mut raw = self
                .raw
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if raw_enabled && raw.is_none() {
                match crate::linux_xinput::LinuxXInputController::start(Arc::clone(&self.queue)) {
                    Ok(controller) => *raw = Some(controller),
                    Err(error) => eprintln!("Embedded XInput2 capture unavailable: {error}"),
                }
            }
            if let Some(raw) = raw.as_ref() {
                raw.set_enabled(raw_enabled);
                raw_enabled
            } else {
                false
            }
        }

        #[cfg(target_os = "windows")]
        {
            // Qt owns absolute position, buttons, and wheel as one ordered event
            // stream. Raw Input is required only for relative motion. Letting the
            // raw thread own buttons in absolute mode races the Qt position event,
            // so the host can apply a click at its previous cursor coordinate.
            let raw_enabled = raw_capture_enabled(active, relative_mouse);
            let mut raw = self
                .raw
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if active && raw.is_none() && window_handle != 0 {
                match crate::windows_raw_input::WindowsRawInputController::start(
                    window_handle as isize,
                    Arc::clone(&self.queue),
                ) {
                    Ok(controller) => *raw = Some(controller),
                    Err(error) => eprintln!("Embedded Raw Input capture unavailable: {error}"),
                }
            }
            if let Some(raw) = raw.as_ref() {
                if window_handle != 0 {
                    raw.set_foreground_owner(window_handle as isize);
                }
                raw.set_capture(raw_enabled, relative_mouse);
                raw_enabled
            } else {
                false
            }
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            let _ = (relative_mouse, window_handle);
            false
        }
    }

    pub fn queue(&self) -> Arc<CapturedInputQueue> {
        Arc::clone(&self.queue)
    }
}

#[cfg(any(target_os = "linux", target_os = "windows", test))]
const fn raw_capture_enabled(active: bool, relative_mouse: bool) -> bool {
    active && relative_mouse
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inactive_capture_drops_input_and_active_capture_preserves_typed_events() {
        let queue = Arc::new(CapturedInputQueue::default());
        let capture = EmbeddedInputCapture::new(Arc::clone(&queue));
        capture.submit(CapturedInput::Key {
            virtual_key: 0x57,
            modifiers: 0,
            pressed: true,
        });
        assert_eq!(queue.take(), None);

        capture.active.store(true, Ordering::Release);
        capture.submit_local_action(EmbeddedLocalAction::Guide);
        assert_eq!(queue.take(), Some(CapturedInput::Guide));
    }

    #[test]
    fn raw_capture_is_reserved_for_relative_mouse_mode() {
        assert!(!raw_capture_enabled(false, false));
        assert!(!raw_capture_enabled(true, false));
        assert!(!raw_capture_enabled(false, true));
        assert!(raw_capture_enabled(true, true));
    }
}
