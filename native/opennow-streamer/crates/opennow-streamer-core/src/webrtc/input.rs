use std::collections::{BTreeMap, BTreeSet};

use super::*;

#[derive(Default)]
pub(super) struct InputState {
    pub(super) paused: bool,
    keys: BTreeSet<u16>,
    buttons: BTreeSet<u8>,
    gamepads: BTreeMap<u8, u16>,
}

impl InputState {
    pub(super) fn sent(&mut self, input: &CapturedInput) -> Result<(), Failure> {
        match input {
            CapturedInput::Key {
                virtual_key,
                pressed,
                ..
            } => {
                if *pressed {
                    if self.keys.len() >= 256 && !self.keys.contains(virtual_key) {
                        return Err(Failure {
                            code: "webrtc-input-overflow",
                            message: "Too many concurrently held keys".to_owned(),
                        });
                    }
                    self.keys.insert(*virtual_key);
                } else {
                    self.keys.remove(virtual_key);
                }
            }
            CapturedInput::MouseButton { button, pressed } => {
                if *pressed {
                    self.buttons.insert(*button);
                } else {
                    self.buttons.remove(button);
                }
            }
            CapturedInput::Gamepad {
                controller_id,
                bitmap,
                ..
            } => {
                self.gamepads.insert(*controller_id & 3, *bitmap);
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn set_paused(&mut self, paused: bool) -> Vec<CapturedInput> {
        self.paused = paused;
        if !paused {
            return Vec::new();
        }
        let mut neutral = Vec::new();
        neutral.extend(
            std::mem::take(&mut self.keys)
                .into_iter()
                .map(|virtual_key| CapturedInput::Key {
                    virtual_key,
                    modifiers: 0,
                    pressed: false,
                }),
        );
        neutral.extend(std::mem::take(&mut self.buttons).into_iter().map(|button| {
            CapturedInput::MouseButton {
                button,
                pressed: false,
            }
        }));
        neutral.extend(std::mem::take(&mut self.gamepads).into_iter().map(
            |(controller_id, bitmap)| CapturedInput::Gamepad {
                controller_id,
                bitmap,
                buttons: 0,
                left_trigger: 0,
                right_trigger: 0,
                left_stick_x: 0,
                left_stick_y: 0,
                right_stick_x: 0,
                right_stick_y: 0,
            },
        ));
        neutral
    }
}

pub(super) fn send_and_flush(
    control: &TransportControl,
    inputs: Vec<CapturedInput>,
    timestamp: u64,
) -> Result<(), transport::TransportError> {
    if inputs.is_empty() {
        return Ok(());
    }
    let deadline = Instant::now() + Duration::from_millis(150);
    for input in inputs {
        let packet = captured_input_packet(input, timestamp);
        loop {
            if Instant::now() >= deadline {
                return Err(transport::TransportError::Backpressured);
            }
            match control.send_input(packet.clone(), false) {
                Ok(()) => break,
                Err(transport::TransportError::Backpressured) => {
                    control.flush_input(deadline.saturating_duration_since(Instant::now()))?
                }
                Err(error) => return Err(error),
            }
        }
    }
    control.flush_input(deadline.saturating_duration_since(Instant::now()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_pause_releases_keys_buttons_and_controller_without_repressing_on_resume() {
        let mut state = InputState::default();
        state
            .sent(&CapturedInput::Key {
                virtual_key: 65,
                modifiers: 1,
                pressed: true,
            })
            .unwrap();
        state
            .sent(&CapturedInput::MouseButton {
                button: 1,
                pressed: true,
            })
            .unwrap();
        state
            .sent(&CapturedInput::Gamepad {
                controller_id: 2,
                bitmap: 0x0404,
                buttons: 0xffff,
                left_trigger: 255,
                right_trigger: 255,
                left_stick_x: 32767,
                left_stick_y: 32767,
                right_stick_x: 32767,
                right_stick_y: 32767,
            })
            .unwrap();
        let release = state.set_paused(true);
        assert_eq!(release.len(), 3);
        assert!(matches!(
            release[0],
            CapturedInput::Key {
                virtual_key: 65,
                pressed: false,
                modifiers: 0
            }
        ));
        assert!(matches!(
            release[1],
            CapturedInput::MouseButton {
                button: 1,
                pressed: false
            }
        ));
        assert!(matches!(
            release[2],
            CapturedInput::Gamepad {
                controller_id: 2,
                bitmap: 0x0404,
                buttons: 0,
                left_trigger: 0,
                right_trigger: 0,
                left_stick_x: 0,
                left_stick_y: 0,
                right_stick_x: 0,
                right_stick_y: 0
            }
        ));
        assert!(state.set_paused(true).is_empty());
        assert!(state.set_paused(false).is_empty());
        assert!(!state.paused);
    }
}
