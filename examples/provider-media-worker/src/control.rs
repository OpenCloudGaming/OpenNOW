use opennow_media_protocol::SourceStamp;
use opennow_media_protocol::wire::{
    AUDIO_TRACK_ID, AckKind, ControlMessage, FrameStage, InputEvent, VIDEO_TRACK_ID,
};
use opennow_plugin_api::media::InputCapabilities;
use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Default)]
pub struct InputState {
    keys: BTreeMap<u16, u16>,
    mouse_buttons: BTreeSet<u8>,
    relative: (i64, i64),
    absolute: Option<(u16, u16, u16, u16)>,
    wheel: (i64, i64),
    text: String,
    pending_text: Option<PendingText>,
    last_paste_id: Option<u64>,
    gamepads: BTreeMap<u8, Gamepad>,
    progress: BTreeMap<(u32, u8), SourceStamp>,
}

struct PendingText {
    id: u64,
    utf8: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Gamepad {
    bitmap: u16,
    buttons: u16,
    triggers: [u8; 2],
    axes: [i16; 4],
    incarnation: u64,
}

impl InputState {
    fn clear_input(&mut self) {
        self.keys.clear();
        self.mouse_buttons.clear();
        self.relative = (0, 0);
        self.absolute = None;
        self.wheel = (0, 0);
        self.text.clear();
        self.pending_text = None;
        self.gamepads.clear();
    }

    pub fn apply(
        &mut self,
        message: ControlMessage,
        attempt: u64,
        capabilities: &InputCapabilities,
        keyframe: &AtomicBool,
    ) -> io::Result<(Vec<ControlMessage>, bool)> {
        let mut replies = Vec::with_capacity(2);
        let (sequence, kind, stop) = match message {
            ControlMessage::Input {
                attempt_generation,
                sequence,
                event,
                ..
            } if attempt_generation == attempt => {
                match event {
                    InputEvent::Key {
                        virtual_key,
                        modifiers,
                        pressed,
                    } if capabilities.keyboard => {
                        if pressed {
                            self.keys.insert(virtual_key, modifiers);
                        } else {
                            self.keys.remove(&virtual_key);
                        }
                    }
                    InputEvent::MouseRelative { x, y } if capabilities.relative_mouse => {
                        self.relative.0 = self.relative.0.saturating_add(i64::from(x));
                        self.relative.1 = self.relative.1.saturating_add(i64::from(y));
                    }
                    InputEvent::MouseAbsolute {
                        x,
                        y,
                        width,
                        height,
                    } if capabilities.absolute_mouse
                        && width > 0
                        && height > 0
                        && x < width
                        && y < height =>
                    {
                        self.absolute = Some((x, y, width, height));
                    }
                    InputEvent::MouseButton { button, pressed }
                        if capabilities.relative_mouse || capabilities.absolute_mouse =>
                    {
                        if pressed {
                            self.mouse_buttons.insert(button);
                        } else {
                            self.mouse_buttons.remove(&button);
                        }
                    }
                    InputEvent::MouseWheel { x, y }
                        if capabilities.relative_mouse || capabilities.absolute_mouse =>
                    {
                        self.wheel.0 = self.wheel.0.saturating_add(i64::from(x));
                        self.wheel.1 = self.wheel.1.saturating_add(i64::from(y));
                    }
                    InputEvent::Text {
                        paste_id,
                        offset,
                        final_chunk,
                        utf8,
                    } if capabilities.text => {
                        if paste_id == 0 || utf8.len() > 8192 || (utf8.is_empty() && !final_chunk) {
                            return Err(io::Error::other("Invalid paste chunk"));
                        }
                        match &self.pending_text {
                            Some(pending)
                                if pending.id == paste_id
                                    && pending.utf8.len() == offset as usize
                                    && pending.utf8.len() + utf8.len() <= 65536 => {}
                            None if offset == 0 && self.last_paste_id != Some(paste_id) => {}
                            _ => {
                                return Err(io::Error::other(
                                    "Paste identity, offset, or size is invalid",
                                ));
                            }
                        }
                        let pending = self.pending_text.get_or_insert_with(|| PendingText {
                            id: paste_id,
                            utf8: String::new(),
                        });
                        pending.utf8.push_str(&utf8);
                        if final_chunk {
                            self.text = self.pending_text.take().unwrap().utf8;
                            self.last_paste_id = Some(paste_id);
                        }
                    }
                    InputEvent::Gamepad {
                        controller,
                        bitmap,
                        buttons,
                        left_trigger,
                        right_trigger,
                        left_x,
                        left_y,
                        right_x,
                        right_y,
                        incarnation,
                    } if controller < capabilities.gamepad_slots && incarnation != 0 => {
                        self.gamepads.insert(
                            controller,
                            Gamepad {
                                bitmap,
                                buttons,
                                triggers: [left_trigger, right_trigger],
                                axes: [left_x, left_y, right_x, right_y],
                                incarnation,
                            },
                        );
                        if capabilities.rumble && buttons != 0 {
                            replies.push(ControlMessage::Rumble {
                                attempt_generation: attempt,
                                controller,
                                incarnation,
                                low: u16::from(left_trigger) * 257,
                                high: u16::from(right_trigger) * 257,
                                duration_ms: 50,
                            });
                        }
                    }
                    _ => return Err(io::Error::other("Input is outside accepted capabilities")),
                }
                (sequence, AckKind::Input, false)
            }
            ControlMessage::Neutral {
                attempt_generation,
                sequence,
            } if attempt_generation == attempt => {
                self.clear_input();
                (sequence, AckKind::Neutral, false)
            }
            ControlMessage::Keyframe {
                attempt_generation,
                track_id: VIDEO_TRACK_ID,
            } if attempt_generation == attempt => {
                keyframe.store(true, Ordering::Release);
                (0, AckKind::Keyframe, false)
            }
            ControlMessage::FrameProgress {
                provenance, stage, ..
            } if provenance.attempt_generation == attempt
                && matches!(provenance.track_id, VIDEO_TRACK_ID | AUDIO_TRACK_ID) =>
            {
                provenance.validate().map_err(io::Error::other)?;
                let stage = match stage {
                    FrameStage::Accepted => 0,
                    FrameStage::Decoded => 1,
                    FrameStage::Presented => 2,
                };
                if let Some(source) = provenance.source {
                    self.progress.insert((provenance.track_id, stage), source);
                } else {
                    self.progress.remove(&(provenance.track_id, stage));
                }
                return Ok((replies, false));
            }
            ControlMessage::Stop {
                attempt_generation,
                sequence,
            } if attempt_generation == attempt => {
                self.clear_input();
                (sequence, AckKind::Stop, true)
            }
            _ => return Err(io::Error::other("Unexpected control or attempt")),
        };
        replies.insert(
            0,
            ControlMessage::Ack {
                attempt_generation: attempt,
                sequence,
                kind,
            },
        );
        Ok((replies, stop))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opennow_media_protocol::FrameProvenance;

    fn text_capabilities() -> InputCapabilities {
        InputCapabilities {
            keyboard: true,
            relative_mouse: false,
            absolute_mouse: false,
            text: true,
            gamepad_slots: 1,
            rumble: false,
        }
    }

    fn chunk(
        sequence: u64,
        paste_id: u64,
        offset: u32,
        final_chunk: bool,
        utf8: &str,
    ) -> ControlMessage {
        ControlMessage::Input {
            attempt_generation: 7,
            sequence,
            captured_us: 0,
            event: InputEvent::Text {
                paste_id,
                offset,
                final_chunk,
                utf8: utf8.into(),
            },
        }
    }

    #[test]
    fn unicode_paste_acks_each_chunk_and_applies_only_the_complete_64k_value() {
        let mut state = InputState::default();
        let keyframe = AtomicBool::new(false);
        let value = "🦀".repeat(16384);
        for index in 0..8 {
            let offset = index * 8192;
            let (replies, stop) = state
                .apply(
                    chunk(
                        index as u64 + 1,
                        99,
                        offset as u32,
                        index == 7,
                        &value[offset..offset + 8192],
                    ),
                    7,
                    &text_capabilities(),
                    &keyframe,
                )
                .unwrap();
            assert!(!stop);
            assert!(
                matches!(replies[0], ControlMessage::Ack { sequence, kind: AckKind::Input, .. } if sequence == index as u64 + 1)
            );
            if index < 7 {
                assert!(state.text.is_empty());
            }
        }
        assert_eq!(state.text, value);
        assert!(state.pending_text.is_none());
        assert!(
            state
                .apply(
                    chunk(9, 99, 57344, true, &value[57344..]),
                    7,
                    &text_capabilities(),
                    &keyframe
                )
                .is_err()
        );
        assert!(
            state
                .apply(
                    chunk(10, 99, 0, true, "duplicate"),
                    7,
                    &text_capabilities(),
                    &keyframe
                )
                .is_err()
        );
        assert_eq!(state.text, value);
    }

    #[test]
    fn paste_rejects_wrong_id_offset_and_bounds_without_applying_partial_text() {
        let mut state = InputState::default();
        let keyframe = AtomicBool::new(false);
        let caps = text_capabilities();
        assert!(
            state
                .apply(chunk(1, 1, 0, true, &"x".repeat(8193)), 7, &caps, &keyframe)
                .is_err()
        );
        state
            .apply(chunk(2, 1, 0, false, "🦀"), 7, &caps, &keyframe)
            .unwrap();
        assert!(
            state
                .apply(chunk(3, 1, 1, true, "x"), 7, &caps, &keyframe)
                .is_err()
        );
        assert!(
            state
                .apply(chunk(4, 2, 4, true, "x"), 7, &caps, &keyframe)
                .is_err()
        );
        assert!(
            state
                .apply(chunk(5, 2, 0, true, "x"), 7, &caps, &keyframe)
                .is_err()
        );
        assert_eq!(state.pending_text.as_ref().unwrap().utf8, "🦀");
        assert!(state.text.is_empty());
        state
            .apply(
                ControlMessage::Neutral {
                    attempt_generation: 7,
                    sequence: 6,
                },
                7,
                &caps,
                &keyframe,
            )
            .unwrap();
        for index in 0..8 {
            state
                .apply(
                    chunk(7 + index, 3, index as u32 * 8192, false, &"x".repeat(8192)),
                    7,
                    &caps,
                    &keyframe,
                )
                .unwrap();
        }
        assert!(
            state
                .apply(chunk(15, 3, 65536, true, "x"), 7, &caps, &keyframe)
                .is_err()
        );
        assert!(state.text.is_empty());
    }

    #[test]
    fn neutral_and_stop_clear_pending_paste_and_held_input() {
        for stop in [false, true] {
            let mut state = InputState::default();
            let keyframe = AtomicBool::new(false);
            let caps = text_capabilities();
            state
                .apply(chunk(1, 5, 0, false, "partial 🦀"), 7, &caps, &keyframe)
                .unwrap();
            state
                .apply(
                    ControlMessage::Input {
                        attempt_generation: 7,
                        sequence: 2,
                        captured_us: 0,
                        event: InputEvent::Key {
                            virtual_key: 65,
                            modifiers: 0,
                            pressed: true,
                        },
                    },
                    7,
                    &caps,
                    &keyframe,
                )
                .unwrap();
            state
                .apply(
                    ControlMessage::Input {
                        attempt_generation: 7,
                        sequence: 3,
                        captured_us: 0,
                        event: InputEvent::Gamepad {
                            controller: 0,
                            bitmap: 1,
                            buttons: 1,
                            left_trigger: 1,
                            right_trigger: 2,
                            left_x: 3,
                            left_y: 4,
                            right_x: 5,
                            right_y: 6,
                            incarnation: 7,
                        },
                    },
                    7,
                    &caps,
                    &keyframe,
                )
                .unwrap();
            let message = if stop {
                ControlMessage::Stop {
                    attempt_generation: 7,
                    sequence: 4,
                }
            } else {
                ControlMessage::Neutral {
                    attempt_generation: 7,
                    sequence: 4,
                }
            };
            let (_, stopped) = state.apply(message, 7, &caps, &keyframe).unwrap();
            assert_eq!(stopped, stop);
            assert!(
                state.pending_text.is_none()
                    && state.text.is_empty()
                    && state.keys.is_empty()
                    && state.gamepads.is_empty()
            );
        }
    }

    #[test]
    fn typed_input_is_consumed_and_neutral_clears_all_pressed_state() {
        let caps = InputCapabilities {
            keyboard: true,
            relative_mouse: true,
            absolute_mouse: true,
            text: true,
            gamepad_slots: 1,
            rumble: true,
        };
        let events = [
            InputEvent::Key {
                virtual_key: 65,
                modifiers: 2,
                pressed: true,
            },
            InputEvent::MouseRelative { x: 3, y: -4 },
            InputEvent::MouseAbsolute {
                x: 10,
                y: 20,
                width: 320,
                height: 180,
            },
            InputEvent::MouseButton {
                button: 1,
                pressed: true,
            },
            InputEvent::MouseWheel { x: 0, y: 120 },
            InputEvent::Text {
                paste_id: 1,
                offset: 0,
                final_chunk: true,
                utf8: "demo text".into(),
            },
            InputEvent::Gamepad {
                controller: 0,
                bitmap: 1,
                buttons: 1,
                left_trigger: 4,
                right_trigger: 5,
                left_x: 6,
                left_y: 7,
                right_x: 8,
                right_y: 9,
                incarnation: 99,
            },
        ];
        let mut state = InputState::default();
        let keyframe = AtomicBool::new(false);
        for (sequence, event) in events.into_iter().enumerate() {
            let (replies, stop) = state
                .apply(
                    ControlMessage::Input {
                        attempt_generation: 7,
                        sequence: sequence as u64,
                        captured_us: 0,
                        event,
                    },
                    7,
                    &caps,
                    &keyframe,
                )
                .unwrap();
            assert!(!stop);
            assert!(
                matches!(replies[0], ControlMessage::Ack { sequence: received, kind: AckKind::Input, .. } if received == sequence as u64)
            );
            if sequence == 6 {
                assert!(matches!(
                    replies[1],
                    ControlMessage::Rumble {
                        controller: 0,
                        incarnation: 99,
                        low: 1028,
                        high: 1285,
                        ..
                    }
                ));
            }
        }
        assert_eq!(state.keys.get(&65), Some(&2));
        assert!(state.mouse_buttons.contains(&1));
        assert_eq!(state.relative, (3, -4));
        assert_eq!(state.absolute, Some((10, 20, 320, 180)));
        assert_eq!(state.wheel, (0, 120));
        assert_eq!(state.text, "demo text");
        assert_eq!(state.gamepads[&0].axes, [6, 7, 8, 9]);
        state
            .apply(
                ControlMessage::Neutral {
                    attempt_generation: 7,
                    sequence: 8,
                },
                7,
                &caps,
                &keyframe,
            )
            .unwrap();
        assert!(
            state.keys.is_empty() && state.mouse_buttons.is_empty() && state.gamepads.is_empty()
        );
        assert!(state.text.is_empty() && state.absolute.is_none());
        assert_eq!(state.relative, (0, 0));
        assert_eq!(state.wheel, (0, 0));
    }

    #[test]
    fn progress_preserves_full_width_optional_sender_and_ssrc() {
        let caps = InputCapabilities {
            keyboard: false,
            relative_mouse: false,
            absolute_mouse: false,
            text: false,
            gamepad_slots: 0,
            rumble: false,
        };
        let mut state = InputState::default();
        let keyframe = AtomicBool::new(false);
        for sender_frame_id in [Some(u32::MAX as u64 + 1234), None] {
            let source = SourceStamp {
                sender_frame_id,
                timestamp: u32::MAX as u64 + 90000,
                clock_rate_hz: 90000,
                ssrc: None,
            };
            let provenance = FrameProvenance {
                attempt_generation: 7,
                track_id: VIDEO_TRACK_ID,
                source: Some(source),
            };
            state
                .apply(
                    ControlMessage::FrameProgress {
                        provenance,
                        stage: FrameStage::Presented,
                        local_us: 9,
                    },
                    7,
                    &caps,
                    &keyframe,
                )
                .unwrap();
            assert_eq!(state.progress[&(VIDEO_TRACK_ID, 2)], source);
        }
        assert!(
            state
                .apply(
                    ControlMessage::Input {
                        attempt_generation: 7,
                        sequence: 1,
                        captured_us: 0,
                        event: InputEvent::Key {
                            virtual_key: 1,
                            modifiers: 0,
                            pressed: true
                        }
                    },
                    7,
                    &caps,
                    &keyframe
                )
                .is_err()
        );
        assert!(
            state
                .apply(
                    ControlMessage::Neutral {
                        attempt_generation: 8,
                        sequence: 2
                    },
                    7,
                    &caps,
                    &keyframe
                )
                .is_err()
        );
    }
}
