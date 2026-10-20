//! The 24-byte little-endian 'coop-input' record.
//!
//! | Offset | Type | Field |
//! | --- | --- | --- |
//! | 0 | u32 | seq, incremented per sent snapshot, wrapping |
//! | 4 | u16 | bitmap: the guest's local connected bitmap (bit 0 = its controller 0) |
//! | 6 | u16 | buttons |
//! | 8 | u8 | lt |
//! | 9 | u8 | rt |
//! | 10 | i16 | lx |
//! | 12 | i16 | ly |
//! | 14 | i16 | rx |
//! | 16 | i16 | ry |
//! | 18 | u16 | reserved, zero |
//! | 20 | u32 | ts: guest capture time in milliseconds since the guest started, wrapping |

pub const INPUT_RECORD_LEN: usize = 24;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GamepadState {
    pub bitmap: u16,
    pub buttons: u16,
    pub left_trigger: u8,
    pub right_trigger: u8,
    pub left_stick_x: i16,
    pub left_stick_y: i16,
    pub right_stick_x: i16,
    pub right_stick_y: i16,
}

impl GamepadState {
    pub fn is_neutral(&self) -> bool {
        Self {
            bitmap: self.bitmap,
            ..Self::default()
        } == *self
    }
}

/// Picks the one local controller a guest forwards. The first controller to leave neutral
/// owns the guest's slot until it returns to neutral (which is also how a disconnect is
/// reported); only then can another controller take over. Two controllers' states never
/// mix, and the old owner's neutral state always goes out before the new owner's first one.
#[derive(Debug, Default)]
pub struct GamepadSelector {
    owner: Option<u8>,
    release: Option<GamepadState>,
    latest: Option<GamepadState>,
}

impl GamepadSelector {
    pub fn observe(&mut self, controller: u8, state: GamepadState) {
        match self.owner {
            Some(owner) if owner == controller => {
                if state.is_neutral() {
                    self.owner = None;
                    self.latest = None;
                    self.release = Some(state);
                } else {
                    self.latest = Some(state);
                }
            }
            None if !state.is_neutral() => {
                self.owner = Some(controller);
                self.latest = Some(state);
            }
            _ => {}
        }
    }

    /// The next state to forward, if one is new since the previous call.
    pub fn take(&mut self) -> Option<GamepadState> {
        self.release.take().or_else(|| self.latest.take())
    }
}

/// Totals of local controller samples a guest took, by source: plain SDL gamepads, and
/// Sony controllers whose snapshots were admitted as ordinary gamepad reports.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InputSamples {
    pub plain: u64,
    pub sony: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InputRecord {
    pub seq: u32,
    pub state: GamepadState,
    pub ts_ms: u32,
}

impl InputRecord {
    pub fn encode(&self) -> [u8; INPUT_RECORD_LEN] {
        let mut bytes = [0_u8; INPUT_RECORD_LEN];
        bytes[0..4].copy_from_slice(&self.seq.to_le_bytes());
        bytes[4..6].copy_from_slice(&self.state.bitmap.to_le_bytes());
        bytes[6..8].copy_from_slice(&self.state.buttons.to_le_bytes());
        bytes[8] = self.state.left_trigger;
        bytes[9] = self.state.right_trigger;
        bytes[10..12].copy_from_slice(&self.state.left_stick_x.to_le_bytes());
        bytes[12..14].copy_from_slice(&self.state.left_stick_y.to_le_bytes());
        bytes[14..16].copy_from_slice(&self.state.right_stick_x.to_le_bytes());
        bytes[16..18].copy_from_slice(&self.state.right_stick_y.to_le_bytes());
        bytes[20..24].copy_from_slice(&self.ts_ms.to_le_bytes());
        bytes
    }

    /// Rejects anything but an exact 24-byte record.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let bytes: &[u8; INPUT_RECORD_LEN] = bytes.try_into().ok()?;
        let u16_at = |offset: usize| u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        let i16_at = |offset: usize| i16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        Some(Self {
            seq: u32::from_le_bytes(bytes[0..4].try_into().ok()?),
            state: GamepadState {
                bitmap: u16_at(4),
                buttons: u16_at(6),
                left_trigger: bytes[8],
                right_trigger: bytes[9],
                left_stick_x: i16_at(10),
                left_stick_y: i16_at(12),
                right_stick_x: i16_at(14),
                right_stick_y: i16_at(16),
            },
            ts_ms: u32::from_le_bytes(bytes[20..24].try_into().ok()?),
        })
    }
}

/// True when 'candidate' is newer than 'latest' in wrapping u32 sequence space.
pub fn is_newer_seq(candidate: u32, latest: u32) -> bool {
    let delta = candidate.wrapping_sub(latest);
    delta != 0 && delta < 0x8000_0000
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pressed(buttons: u16) -> GamepadState {
        GamepadState {
            bitmap: 0x0303,
            buttons,
            ..GamepadState::default()
        }
    }

    fn released() -> GamepadState {
        GamepadState {
            bitmap: 0x0303,
            ..GamepadState::default()
        }
    }

    #[test]
    fn selector_forwards_a_controller_on_any_slot() {
        let mut selector = GamepadSelector::default();
        selector.observe(1, released());
        assert_eq!(selector.take(), None);
        selector.observe(1, pressed(0x1000));
        assert_eq!(selector.take(), Some(pressed(0x1000)));
        assert_eq!(selector.take(), None);
        selector.observe(1, released());
        assert_eq!(selector.take(), Some(released()));
    }

    #[test]
    fn selector_never_interleaves_two_controllers() {
        let mut selector = GamepadSelector::default();
        selector.observe(0, pressed(0x1000));
        selector.observe(2, pressed(0x2000));
        selector.observe(0, pressed(0x1001));
        selector.observe(2, pressed(0x2001));
        assert_eq!(selector.take(), Some(pressed(0x1001)));
        selector.observe(2, pressed(0x2002));
        assert_eq!(
            selector.take(),
            None,
            "the second pad cannot take over while the first is held"
        );
    }

    #[test]
    fn selector_sends_the_old_owners_neutral_before_switching() {
        let mut selector = GamepadSelector::default();
        selector.observe(0, pressed(0x1000));
        assert_eq!(selector.take(), Some(pressed(0x1000)));
        selector.observe(0, pressed(0x1001));
        selector.observe(0, released());
        selector.observe(3, pressed(0x2000));
        selector.observe(3, pressed(0x2001));
        assert_eq!(selector.take(), Some(released()), "neutral goes out first");
        assert_eq!(selector.take(), Some(pressed(0x2001)));
        selector.observe(0, pressed(0x1000));
        assert_eq!(selector.take(), None, "the new owner keeps the slot");
    }

    #[test]
    fn record_is_exact_little_endian_layout() {
        let record = InputRecord {
            seq: 0x0403_0201,
            state: GamepadState {
                bitmap: 0x0101,
                buttons: 0x1234,
                left_trigger: 0xaa,
                right_trigger: 0xbb,
                left_stick_x: -2,
                left_stick_y: 0x0506,
                right_stick_x: i16::MIN,
                right_stick_y: i16::MAX,
            },
            ts_ms: 0xdead_beef,
        };
        let bytes = record.encode();
        assert_eq!(
            bytes,
            [
                0x01, 0x02, 0x03, 0x04, 0x01, 0x01, 0x34, 0x12, 0xaa, 0xbb, 0xfe, 0xff, 0x06, 0x05,
                0x00, 0x80, 0xff, 0x7f, 0x00, 0x00, 0xef, 0xbe, 0xad, 0xde
            ]
        );
        assert_eq!(InputRecord::decode(&bytes), Some(record));
    }

    #[test]
    fn decode_rejects_wrong_lengths() {
        assert_eq!(InputRecord::decode(&[0; 23]), None);
        assert_eq!(InputRecord::decode(&[0; 25]), None);
        assert!(InputRecord::decode(&[0; 24]).unwrap().state.is_neutral());
    }

    #[test]
    fn sequence_comparison_wraps() {
        assert!(is_newer_seq(1, 0));
        assert!(is_newer_seq(0, u32::MAX));
        assert!(!is_newer_seq(5, 5));
        assert!(!is_newer_seq(4, 5));
        assert!(!is_newer_seq(0x8000_0005, 5));
    }
}
