use std::sync::Arc;
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
    gamepads: Mutex<[Option<u16>; 4]>,
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
            gamepads: Mutex::new([None; 4]),
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            raw: Mutex::new(None),
        }
    }

    pub fn submit(&self, input: CapturedInput) {
        let mut gamepads = self
            .gamepads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let CapturedInput::Gamepad {
            controller_id,
            bitmap,
            buttons,
            left_trigger,
            right_trigger,
            left_stick_x,
            left_stick_y,
            right_stick_x,
            right_stick_y,
        } = &input
        {
            if usize::from(*controller_id) >= gamepads.len() {
                return;
            }
            let neutral = (
                *buttons,
                *left_trigger,
                *right_trigger,
                *left_stick_x,
                *left_stick_y,
                *right_stick_x,
                *right_stick_y,
            ) == (0, 0, 0, 0, 0, 0, 0);
            if !self.active.load(Ordering::Acquire) && !neutral {
                return;
            }
            for value in gamepads.iter_mut().flatten() {
                *value = *bitmap;
            }
            gamepads[usize::from(*controller_id)] = Some(*bitmap);
            if neutral && !self.active.load(Ordering::Acquire) {
                self.queue.release_gamepad(*controller_id, *bitmap);
                return;
            }
        }
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

    pub fn submit_text(
        &self,
        bytes: &[u8],
    ) -> Result<(), opennow_streamer_protocol::text_input::TextInputError> {
        let _guard = self
            .gamepads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !self.active.load(Ordering::Acquire) {
            return Err(opennow_streamer_protocol::text_input::TextInputError::Unavailable);
        }
        self.queue.submit_text(bytes)
    }

    pub fn set_active(&self, active: bool, relative_mouse: bool, window_handle: usize) -> bool {
        let mut gamepads = self
            .gamepads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !active {
            self.queue.discard_text();
            if self.active.load(Ordering::Acquire) {
                for (controller_id, bitmap) in gamepads.iter_mut().enumerate() {
                    if let Some(bitmap) = bitmap.take() {
                        self.queue.release_gamepad(controller_id as u8, bitmap);
                    }
                }
            }
            self.active.store(false, Ordering::Release);
        } else {
            self.active.store(true, Ordering::Release);
        }

        #[cfg(target_os = "linux")]
        {
            let raw_enabled = x11_raw_capture_enabled(active, relative_mouse, window_handle);
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

#[cfg(any(target_os = "linux", test))]
const fn x11_raw_capture_enabled(active: bool, relative_mouse: bool, x11_window: usize) -> bool {
    raw_capture_enabled(active, relative_mouse) && x11_window != 0
}

#[cfg(any(target_os = "linux", target_os = "windows", test))]
const fn raw_capture_enabled(active: bool, relative_mouse: bool) -> bool {
    active && relative_mouse
}

#[cfg(test)]
mod tests {
    #[test]
    fn old_session_teardown_cannot_cancel_or_enable_new_session_text() {
        use opennow_streamer_protocol::text_input::TextInputError;
        let queue = std::sync::Arc::new(crate::CapturedInputQueue::default());
        let capture = super::EmbeddedInputCapture::new(queue.clone());
        capture.set_active(true, false, 0);
        queue.set_text_ready(1, true);
        capture.submit_text(b"old").unwrap();
        let Some(crate::CapturedInput::Text(old)) = queue.take() else {
            panic!("missing old paste")
        };
        queue.set_text_ready(2, false);
        assert!(old.is_cancelled());
        drop(old);
        queue.set_text_ready(1, true);
        assert_eq!(
            capture.submit_text(b"new"),
            Err(TextInputError::Unavailable)
        );
        queue.set_text_ready(2, true);
        capture.submit_text(b"new").unwrap();
        queue.set_text_ready(1, false);
        let Some(crate::CapturedInput::Text(new)) = queue.take() else {
            panic!("missing new paste")
        };
        assert!(!new.is_cancelled());
    }

    #[test]
    fn text_requires_capture_and_session_and_cancels_in_flight_on_focus_loss() {
        use opennow_streamer_protocol::text_input::TextInputError;
        let queue = std::sync::Arc::new(crate::CapturedInputQueue::default());
        let capture = super::EmbeddedInputCapture::new(queue.clone());
        assert_eq!(
            capture.submit_text(b"paste"),
            Err(TextInputError::Unavailable)
        );
        capture.set_active(true, false, 0);
        assert_eq!(
            capture.submit_text(b"paste"),
            Err(TextInputError::Unavailable)
        );
        queue.set_text_ready(1, true);
        assert_eq!(capture.submit_text(b"paste"), Ok(()));
        assert_eq!(capture.submit_text(b"again"), Err(TextInputError::Busy));
        let Some(crate::CapturedInput::Text(text)) = queue.take() else {
            panic!("missing text")
        };
        assert_eq!(capture.submit_text(b"again"), Err(TextInputError::Busy));
        capture.set_active(false, false, 0);
        assert!(text.is_cancelled());
        drop(text);
        capture.set_active(true, false, 0);
        assert_eq!(capture.submit_text(b"again"), Ok(()));
        queue.set_text_ready(1, false);
        assert!(queue.take().is_none());
        assert_eq!(
            capture.submit_text(b"again"),
            Err(TextInputError::Unavailable)
        );
    }

    #[test]
    fn full_capture_queue_rejects_paste_atomically_without_control_overflow() {
        use opennow_streamer_protocol::text_input::TextInputError;
        let queue = std::sync::Arc::new(crate::CapturedInputQueue::default());
        let capture = super::EmbeddedInputCapture::new(queue.clone());
        capture.set_active(true, false, 0);
        queue.set_text_ready(1, true);
        for _ in 0..256 {
            capture.submit(crate::CapturedInput::Guide);
        }
        assert_eq!(capture.submit_text(b"paste"), Err(TextInputError::Busy));
        assert!(!queue.take_overflowed());
        for _ in 0..256 {
            assert_eq!(queue.take(), Some(crate::CapturedInput::Guide));
        }
        assert!(queue.take().is_none());
        assert_eq!(capture.submit_text(b"paste"), Ok(()));
    }

    use super::*;

    fn gamepad(controller_id: u8, bitmap: u16, buttons: u16) -> CapturedInput {
        CapturedInput::Gamepad {
            controller_id,
            bitmap,
            buttons,
            left_trigger: 0,
            right_trigger: 0,
            left_stick_x: 0,
            left_stick_y: 0,
            right_stick_x: 0,
            right_stick_y: 0,
        }
    }

    #[test]
    fn closing_capture_neutralizes_all_controllers_before_rejecting_held_input() {
        let queue = Arc::new(CapturedInputQueue::default());
        let capture = EmbeddedInputCapture::new(Arc::clone(&queue));
        capture.set_active(true, false, 0);
        for id in 0..4 {
            capture.submit(gamepad(id, 0x0f0f, 0x1000));
            assert_eq!(queue.take(), Some(gamepad(id, 0x0f0f, 0x1000)));
        }
        capture.set_active(false, false, 0);
        capture.submit(gamepad(0, 0x0f0f, 0x1000));
        for id in 0..4 {
            assert_eq!(queue.take(), Some(gamepad(id, 0x0f0f, 0)));
        }
        assert_eq!(queue.take(), None);
        capture.set_active(false, false, 0);
        assert_eq!(queue.take(), None);
    }

    #[test]
    fn controller_disconnect_is_preserved_after_capture_closes() {
        let queue = Arc::new(CapturedInputQueue::default());
        let capture = EmbeddedInputCapture::new(Arc::clone(&queue));
        capture.set_active(true, false, 0);
        capture.submit(gamepad(0, 0x0101, 0x1000));
        capture.set_active(false, false, 0);
        capture.submit(gamepad(0, 0, 0));
        assert_eq!(queue.take(), Some(gamepad(0, 0, 0)));
        assert_eq!(queue.take(), None);
    }

    #[test]
    fn concurrent_controller_submission_cannot_follow_a_capture_closure_neutral() {
        let queue = Arc::new(CapturedInputQueue::default());
        let capture = Arc::new(EmbeddedInputCapture::new(Arc::clone(&queue)));
        capture.set_active(true, false, 0);
        capture.submit(gamepad(0, 0x0101, 0x1000));
        queue.take();
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let producer = {
            let capture = Arc::clone(&capture);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                for _ in 0..64 {
                    capture.submit(gamepad(0, 0x0101, 0x1000));
                }
            })
        };
        barrier.wait();
        capture.set_active(false, false, 0);
        producer.join().unwrap();
        assert_eq!(queue.take(), Some(gamepad(0, 0x0101, 0)));
        assert_eq!(queue.take(), None);
        assert!(!queue.take_overflowed());
    }

    #[test]
    fn capture_closure_resets_triggers_and_both_sticks_without_a_button_press() {
        let queue = Arc::new(CapturedInputQueue::default());
        let capture = EmbeddedInputCapture::new(Arc::clone(&queue));
        capture.set_active(true, false, 0);
        capture.submit(CapturedInput::Gamepad {
            controller_id: 0,
            bitmap: 0x0101,
            buttons: 0,
            left_trigger: 255,
            right_trigger: 128,
            left_stick_x: 32767,
            left_stick_y: -32768,
            right_stick_x: 24000,
            right_stick_y: -24000,
        });
        queue.take();
        capture.set_active(false, false, 0);
        assert_eq!(queue.take(), Some(gamepad(0, 0x0101, 0)));
        assert_eq!(queue.take(), None);
    }

    #[test]
    fn capture_closure_reserves_neutral_slots_even_when_normal_queue_is_full() {
        let queue = Arc::new(CapturedInputQueue::default());
        let capture = EmbeddedInputCapture::new(Arc::clone(&queue));
        capture.set_active(true, false, 0);
        for id in 0..4 {
            capture.submit(gamepad(id, 0x0f0f, 0x1000));
            queue.take();
        }
        for _ in 0..256 {
            capture.submit_local_action(EmbeddedLocalAction::Guide);
        }
        capture.set_active(false, false, 0);
        assert!(!queue.take_overflowed());
        for _ in 0..256 {
            assert_eq!(queue.take(), Some(CapturedInput::Guide));
        }
        for id in 0..4 {
            assert_eq!(queue.take(), Some(gamepad(id, 0x0f0f, 0)));
        }
        assert_eq!(queue.take(), None);
    }

    #[test]
    fn capture_reacquisition_does_not_replay_stale_controller_state() {
        let queue = Arc::new(CapturedInputQueue::default());
        let capture = EmbeddedInputCapture::new(Arc::clone(&queue));
        for _ in 0..3 {
            capture.set_active(true, false, 0);
            assert_eq!(queue.take(), None);
            capture.submit(gamepad(0, 0x0101, 0x1000));
            capture.set_active(false, false, 0);
            assert_eq!(queue.take(), Some(gamepad(0, 0x0101, 0)));
            assert_eq!(queue.take(), None);
        }
    }

    #[test]
    fn xinput_requires_an_explicit_x11_window_even_with_relative_mode_enabled() {
        assert!(!x11_raw_capture_enabled(true, true, 0));
        assert!(x11_raw_capture_enabled(true, true, 42));
        assert!(!x11_raw_capture_enabled(true, false, 42));
        assert!(!x11_raw_capture_enabled(false, true, 42));
    }

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
