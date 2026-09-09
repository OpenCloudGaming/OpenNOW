use std::sync::Mutex;

pub(crate) const HAPTIC_COMMAND_CODE: u16 = 0x010b;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvstControllerRumble {
    pub controller_id: u8,
    pub low_frequency: u16,
    pub high_frequency: u16,
    pub duration_ms: u32,
}

#[derive(Debug, Default)]
pub struct NvstHaptics {
    pending: Mutex<([Option<NvstControllerRumble>; 4], usize)>,
}

impl NvstHaptics {
    pub(crate) fn receive(&self, mut bytes: &[u8]) {
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while bytes.len() >= 4 {
            let code = u16::from_le_bytes([bytes[0], bytes[1]]);
            let length = usize::from(u16::from_le_bytes([bytes[2], bytes[3]]));
            let Some(payload) = bytes.get(4..4 + length) else {
                break;
            };
            bytes = &bytes[4 + length..];
            if code != HAPTIC_COMMAND_CODE || payload.len() < 4 {
                continue;
            }
            let record_length = match u16::from_le_bytes([payload[0], payload[1]]) {
                1 => 6,
                2 => 8,
                _ => continue,
            };
            let available =
                usize::from(u16::from_le_bytes([payload[2], payload[3]])).min(payload.len() - 4);
            for record in payload[4..4 + available].chunks_exact(record_length) {
                let controller_id = u16::from_le_bytes([record[0], record[1]]);
                if controller_id >= 4 {
                    continue;
                }
                let duration = if record_length == 8 {
                    u16::from_le_bytes([record[6], record[7]])
                } else {
                    0
                };
                let command = NvstControllerRumble {
                    controller_id: controller_id as u8,
                    low_frequency: u16::from_le_bytes([record[2], record[3]]),
                    high_frequency: u16::from_le_bytes([record[4], record[5]]),
                    duration_ms: if duration == 0 {
                        1000
                    } else {
                        u32::from(duration)
                    },
                };
                if pending.0[usize::from(controller_id)]
                    .replace(command)
                    .is_some()
                {
                    pending.1 = pending.1.saturating_add(1);
                }
            }
        }
    }

    pub fn take(&self) -> ([Option<NvstControllerRumble>; 4], usize) {
        std::mem::take(
            &mut *self
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(kind: u16, declared_length: u16, records: &[u16]) -> Vec<u8> {
        let mut payload = Vec::new();
        for value in [kind, declared_length].iter().chain(records) {
            payload.extend_from_slice(&value.to_le_bytes());
        }
        super::super::nvst_control::NvstControlCommand {
            code: 0x010b,
            payload,
        }
        .encoded()
    }

    #[test]
    fn reference_pulse_vector_is_little_endian_and_framed() {
        let haptics = NvstHaptics::default();
        haptics.receive(&[
            0x0b, 0x01, 0x0c, 0x00, 0x02, 0x00, 0x08, 0x00, 0x02, 0x00, 0xff, 0xff, 0x00, 0x00,
            0xfa, 0x00,
        ]);
        assert_eq!(
            haptics.take().0[2],
            Some(NvstControllerRumble {
                controller_id: 2,
                low_frequency: 65535,
                high_frequency: 0,
                duration_ms: 250,
            })
        );
    }

    #[test]
    fn state_and_pulse_preserve_slots_motors_and_effective_duration() {
        let haptics = NvstHaptics::default();
        let mut bytes = command(1, 12, &[0, 0x1234, 0xabcd, 3, 0xffff, 0]);
        bytes.extend(command(2, 16, &[1, 9, 8, 321, 2, 7, 6, 0]));
        haptics.receive(&bytes);
        let (commands, coalesced) = haptics.take();
        assert_eq!(coalesced, 0);
        assert_eq!(
            commands[0],
            Some(NvstControllerRumble {
                controller_id: 0,
                low_frequency: 0x1234,
                high_frequency: 0xabcd,
                duration_ms: 1000
            })
        );
        assert_eq!(commands[1].unwrap().duration_ms, 321);
        assert_eq!(commands[2].unwrap().duration_ms, 1000);
        assert_eq!(commands[3].unwrap().low_frequency, 65535);
        assert_eq!(haptics.take(), ([None; 4], 0));
    }

    #[test]
    fn malformed_frames_and_unknown_kinds_or_slots_do_not_create_rumble() {
        let haptics = NvstHaptics::default();
        let valid = command(2, 8, &[0, 1, 2, 3]);
        for length in 0..valid.len() {
            haptics.receive(&valid[..length]);
        }
        haptics.receive(&command(3, 8, &[0, 1, 2, 3]));
        haptics.receive(&command(1, 6, &[256, 1, 2]));
        haptics.receive(&command(1, 5, &[0, 1, 2]));
        assert_eq!(haptics.take(), ([None; 4], 0));
    }

    #[test]
    fn blob_lengths_limit_records_and_complete_short_blob_records_survive() {
        let haptics = NvstHaptics::default();
        haptics.receive(&command(1, 6, &[0, 1, 2, 1, 3, 4]));
        let (commands, _) = haptics.take();
        assert!(commands[0].is_some());
        assert!(commands[1].is_none());
        haptics.receive(&command(1, 12, &[2, 5, 6, 3]));
        let (commands, _) = haptics.take();
        assert!(commands[2].is_some());
        assert!(commands[3].is_none());
    }

    #[test]
    fn backlog_is_four_latest_states_including_stop() {
        let haptics = NvstHaptics::default();
        for _ in 0..1000 {
            haptics.receive(&command(1, 6, &[0, 65535, 65535]));
        }
        haptics.receive(&command(1, 6, &[0, 0, 0]));
        let (commands, coalesced) = haptics.take();
        assert_eq!(coalesced, 1000);
        assert_eq!(commands.iter().flatten().count(), 1);
        assert_eq!(commands[0].unwrap().low_frequency, 0);
        assert_eq!(commands[0].unwrap().high_frequency, 0);
    }
}
