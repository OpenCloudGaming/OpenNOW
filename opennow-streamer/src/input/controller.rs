use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::Duration;
use parking_lot::Mutex;
use tokio::sync::mpsc;
use log::{info, warn, error, debug, trace};
use gilrs::{Gilrs, Event, EventType, Button, Axis};

use crate::webrtc::InputEvent;
use super::get_timestamp_us;

/// XInput button format (confirmed from web client analysis)
/// This is the standard XInput wButtons format used by GFN:
///
/// 0x0001 = DPad Up
/// 0x0002 = DPad Down
/// 0x0004 = DPad Left
/// 0x0008 = DPad Right
/// 0x0010 = Start
/// 0x0020 = Back/Select
/// 0x0040 = L3 (Left Stick Click)
/// 0x0080 = R3 (Right Stick Click)
/// 0x0100 = LB (Left Bumper)
/// 0x0200 = RB (Right Bumper)
/// 0x1000 = A
/// 0x2000 = B
/// 0x4000 = X
/// 0x8000 = Y
const XINPUT_DPAD_UP: u16    = 0x0001;
const XINPUT_DPAD_DOWN: u16  = 0x0002;
const XINPUT_DPAD_LEFT: u16  = 0x0004;
const XINPUT_DPAD_RIGHT: u16 = 0x0008;
const XINPUT_START: u16      = 0x0010;
const XINPUT_BACK: u16       = 0x0020;
const XINPUT_L3: u16         = 0x0040;
const XINPUT_R3: u16         = 0x0080;
const XINPUT_LB: u16         = 0x0100;
const XINPUT_RB: u16         = 0x0200;
const XINPUT_A: u16          = 0x1000;
const XINPUT_B: u16          = 0x2000;
const XINPUT_X: u16          = 0x4000;
const XINPUT_Y: u16          = 0x8000;

/// Deadzone for analog sticks (15% as per GFN docs)
const STICK_DEADZONE: f32 = 0.15;

/// Controller manager to handle gamepad input
pub struct ControllerManager {
    running: Arc<AtomicBool>,
    event_tx: Mutex<Option<mpsc::Sender<InputEvent>>>,
}

impl ControllerManager {
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            event_tx: Mutex::new(None),
        }
    }

    /// Set the input event sender
    pub fn set_event_sender(&self, tx: mpsc::Sender<InputEvent>) {
        *self.event_tx.lock() = Some(tx);
    }

    /// Start the controller input loop
    pub fn start(&self) {
        if self.running.load(Ordering::SeqCst) {
            return;
        }

        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();

        let tx_opt = self.event_tx.lock().clone();

        if tx_opt.is_none() {
            warn!("ControllerManager started without event sender!");
            return;
        }
        let tx = tx_opt.unwrap();

        std::thread::spawn(move || {
            info!("Controller input thread starting...");

            let mut gilrs = match Gilrs::new() {
                Ok(g) => {
                    info!("gilrs initialized successfully");
                    g
                }
                Err(e) => {
                    error!("Failed to initialize gilrs: {}", e);
                    return;
                }
            };

            // Report connected gamepads
            let mut gamepad_count = 0;
            for (id, gamepad) in gilrs.gamepads() {
                gamepad_count += 1;
                info!("Gamepad {} detected: '{}' (UUID: {:?})",
                    id, gamepad.name(), gamepad.uuid());

                // Log supported features
                debug!("  Power info: {:?}", gamepad.power_info());
                debug!("  Is connected: {}", gamepad.is_connected());
            }

            if gamepad_count == 0 {
                warn!("No gamepads detected at startup. Connect a controller to use gamepad input.");
            } else {
                info!("Found {} gamepad(s)", gamepad_count);
            }

            let mut last_button_flags: u16 = 0;
            let mut event_count: u64 = 0;

            while running.load(Ordering::Relaxed) {
                // Poll events
                while let Some(Event { id, event, time, .. }) = gilrs.next_event() {
                    let gamepad = gilrs.gamepad(id);
                    event_count += 1;

                    // Log first few events for debugging
                    if event_count <= 10 {
                        debug!("Controller event #{}: {:?} from '{}' at {:?}",
                            event_count, event, gamepad.name(), time);
                    }

                    // Use gamepad index as controller ID (0-3)
                    // GamepadId is opaque, but we can use usize conversion
                    let controller_id: u8 = usize::from(id) as u8;

                    match event {
                        EventType::Connected => {
                            info!("Gamepad connected: {} (id={})", gamepad.name(), controller_id);
                        }
                        EventType::Disconnected => {
                            info!("Gamepad disconnected: {} (id={})", gamepad.name(), controller_id);
                        }
                        _ => {
                            // Build XInput button bitmap (confirmed from web client)
                            let mut button_flags: u16 = 0;

                            // D-Pad (bits 0-3)
                            if gamepad.is_pressed(Button::DPadUp) { button_flags |= XINPUT_DPAD_UP; }
                            if gamepad.is_pressed(Button::DPadDown) { button_flags |= XINPUT_DPAD_DOWN; }
                            if gamepad.is_pressed(Button::DPadLeft) { button_flags |= XINPUT_DPAD_LEFT; }
                            if gamepad.is_pressed(Button::DPadRight) { button_flags |= XINPUT_DPAD_RIGHT; }

                            // Center buttons (bits 4-5)
                            if gamepad.is_pressed(Button::Start) { button_flags |= XINPUT_START; }
                            if gamepad.is_pressed(Button::Select) { button_flags |= XINPUT_BACK; }

                            // Stick clicks (bits 6-7)
                            if gamepad.is_pressed(Button::LeftThumb) { button_flags |= XINPUT_L3; }
                            if gamepad.is_pressed(Button::RightThumb) { button_flags |= XINPUT_R3; }

                            // Shoulder buttons / bumpers (bits 8-9)
                            // gilrs: LeftTrigger = L1/LB (digital bumper)
                            // gilrs: RightTrigger = R1/RB (digital bumper)
                            if gamepad.is_pressed(Button::LeftTrigger) { button_flags |= XINPUT_LB; }
                            if gamepad.is_pressed(Button::RightTrigger) { button_flags |= XINPUT_RB; }

                            // Face buttons (bits 12-15)
                            // gilrs uses cardinal directions: South=A, East=B, West=X, North=Y
                            if gamepad.is_pressed(Button::South) { button_flags |= XINPUT_A; }
                            if gamepad.is_pressed(Button::East) { button_flags |= XINPUT_B; }
                            if gamepad.is_pressed(Button::West) { button_flags |= XINPUT_X; }
                            if gamepad.is_pressed(Button::North) { button_flags |= XINPUT_Y; }

                            // Analog triggers (0-255)
                            // gilrs uses different axes for different controllers
                            // Try LeftZ/RightZ first (common), then fall back to trigger buttons
                            let lt_axis = gamepad.value(Axis::LeftZ);
                            let rt_axis = gamepad.value(Axis::RightZ);

                            // Triggers typically range from 0.0 to 1.0 (or -1.0 to 1.0 on some controllers)
                            // Normalize to 0-255
                            let left_trigger = if lt_axis.abs() < 0.01 && gamepad.is_pressed(Button::LeftTrigger2) {
                                255u8  // Fallback: if no axis but button pressed, assume full
                            } else {
                                // Handle both 0..1 and -1..1 ranges
                                let normalized = if lt_axis < 0.0 { (lt_axis + 1.0) / 2.0 } else { lt_axis };
                                (normalized.clamp(0.0, 1.0) * 255.0) as u8
                            };

                            let right_trigger = if rt_axis.abs() < 0.01 && gamepad.is_pressed(Button::RightTrigger2) {
                                255u8
                            } else {
                                let normalized = if rt_axis < 0.0 { (rt_axis + 1.0) / 2.0 } else { rt_axis };
                                (normalized.clamp(0.0, 1.0) * 255.0) as u8
                            };

                            // Analog sticks (-32768 to 32767)
                            let lx_val = gamepad.value(Axis::LeftStickX);
                            let ly_val = gamepad.value(Axis::LeftStickY);
                            let rx_val = gamepad.value(Axis::RightStickX);
                            let ry_val = gamepad.value(Axis::RightStickY);

                            // Apply deadzone
                            let apply_deadzone = |val: f32| -> f32 {
                                if val.abs() < STICK_DEADZONE {
                                    0.0
                                } else {
                                    // Scale remaining range to full range
                                    let sign = val.signum();
                                    let magnitude = (val.abs() - STICK_DEADZONE) / (1.0 - STICK_DEADZONE);
                                    sign * magnitude
                                }
                            };

                            let lx = apply_deadzone(lx_val);
                            let ly = apply_deadzone(ly_val);
                            let rx = apply_deadzone(rx_val);
                            let ry = apply_deadzone(ry_val);

                            // Convert to i16 range
                            let left_stick_x = (lx * 32767.0).clamp(-32768.0, 32767.0) as i16;
                            let left_stick_y = (ly * 32767.0).clamp(-32768.0, 32767.0) as i16;
                            let right_stick_x = (rx * 32767.0).clamp(-32768.0, 32767.0) as i16;
                            let right_stick_y = (ry * 32767.0).clamp(-32768.0, 32767.0) as i16;

                            // Log button changes
                            if button_flags != last_button_flags {
                                debug!("Button state changed: 0x{:04X} -> 0x{:04X}",
                                    last_button_flags, button_flags);
                                last_button_flags = button_flags;
                            }

                            // Log stick movement occasionally
                            if left_stick_x != 0 || left_stick_y != 0 || right_stick_x != 0 || right_stick_y != 0 {
                                trace!("Sticks: L({}, {}) R({}, {}) Triggers: L={} R={}",
                                    left_stick_x, left_stick_y, right_stick_x, right_stick_y,
                                    left_trigger, right_trigger);
                            }

                            let event = InputEvent::Gamepad {
                                controller_id,
                                button_flags,
                                left_trigger,
                                right_trigger,
                                left_stick_x,
                                left_stick_y,
                                right_stick_x,
                                right_stick_y,
                                flags: 1, // 1 = controller connected
                                timestamp_us: get_timestamp_us(),
                            };

                            // Send event
                            if let Err(e) = tx.try_send(event) {
                                trace!("Controller event channel full: {:?}", e);
                            }
                        }
                    }
                }

                // Poll sleep - 1ms for 1000Hz polling rate (low latency)
                std::thread::sleep(Duration::from_millis(1));
            }

            info!("Controller input thread stopped (processed {} events)", event_count);
        });
    }

    /// Stop the controller input loop
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

impl Default for ControllerManager {
    fn default() -> Self {
        Self::new()
    }
}
