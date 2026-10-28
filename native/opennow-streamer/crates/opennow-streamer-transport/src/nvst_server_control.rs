use crate::nvst_shutdown_reason::shutdown_reason_outcome;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ControlKeepaliveHealth {
    pub(crate) failures: u64,
    pub(crate) consecutive_failures: u32,
    pub(crate) recoveries: u64,
}

impl ControlKeepaliveHealth {
    pub(crate) fn record(&mut self, admitted: bool) {
        if !admitted {
            self.failures = self.failures.saturating_add(1);
            self.consecutive_failures = self.consecutive_failures.saturating_add(1);
            return;
        }
        if self.consecutive_failures != 0 {
            self.recoveries = self.recoveries.saturating_add(1);
            self.consecutive_failures = 0;
        }
    }
}

pub(crate) fn session_resumable(reason: Option<u32>, extended_code: Option<u32>) -> Option<bool> {
    match reason? {
        0x37 => Some(!matches!(
            extended_code?,
            0x8004_0003 | 0x8004_000b..=0x8004_000f | 0x8004_0011..=0x8004_0016
        )),
        0x3b => Some(matches!(
            extended_code?,
            0x8004_0004 | 0x8004_0007 | 0x8004_0010
        )),
        _ => Some(false),
    }
}

pub(crate) const MAX_SERVER_CONTROL_BYTES: usize = 4 + u16::MAX as usize;
pub(crate) const MAX_SERVER_CONTROL_COMMANDS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ServerControlParseError {
    Oversized,
    TooManyCommands,
    Truncated,
    InvalidLength,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvstSessionTimer {
    pub timer_type: u8,
    pub seconds_left: u32,
    pub active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvstServerTermination {
    pub wire_code: u32,
    pub shutdown_reason: Option<u16>,
    pub reason: u32,
    pub session_alive: bool,
    pub extended_code: u32,
}

const EXTENDED_TEARDOWN: u32 = 0x8003_0023;

impl NvstServerTermination {
    pub(crate) fn from_wire(wire_code: u32, extra: &[u8]) -> Self {
        let shutdown_reason = if wire_code == EXTENDED_TEARDOWN {
            first_session_termination_reason(extra)
        } else {
            None
        };
        let (reason, session_alive, extended_code) = match (wire_code, shutdown_reason) {
            (EXTENDED_TEARDOWN, None) => (0x35, false, 0xc0f5_2003),
            (EXTENDED_TEARDOWN, Some(shutdown_reason)) => shutdown_reason_outcome(shutdown_reason),
            (0x8003_0000 | 0x8011_0000..=0x8011_0001, _) => (0x34, true, wire_code),
            (0x8003_0006, _) => (0x3d, true, wire_code),
            (0x8003_001a, _) => (0x44, true, wire_code),
            _ => (0x35, true, wire_code),
        };
        Self {
            wire_code,
            shutdown_reason,
            reason,
            session_alive,
            extended_code,
        }
    }

    pub fn resumable(self) -> bool {
        session_resumable(Some(self.reason), Some(self.extended_code)) == Some(true)
    }

    pub fn disposition(self) -> &'static str {
        match self.reason {
            0x34 => "normal",
            0x38 => "idle",
            0x39 => "time-limit",
            _ => "error",
        }
    }
}

fn first_session_termination_reason(mut records: &[u8]) -> Option<u16> {
    while let [type_high, type_low, value_high, value_low, rest @ ..] = records {
        let value = u16::from_be_bytes([*value_high, *value_low]);
        match u16::from_be_bytes([*type_high, *type_low]) {
            0 => return Some(value),
            1 => records = rest.get(usize::from(value)..)?,
            _ => return None,
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FramePacingFeedbackMode {
    Never,
    Interval,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ServerControlMessage<'a> {
    Timer(NvstSessionTimer),
    Termination {
        wire_code: u32,
        extra: &'a [u8],
    },
    FramePacingMode {
        stream_index: u32,
        mode: FramePacingFeedbackMode,
    },
    Unhandled,
    Unknown(u16),
}

pub(crate) struct ServerControlMessages<'a> {
    bytes: &'a [u8],
    remaining: usize,
    error: Option<ServerControlParseError>,
}

pub(crate) fn server_control_messages(bytes: &[u8]) -> ServerControlMessages<'_> {
    ServerControlMessages {
        bytes,
        remaining: MAX_SERVER_CONTROL_COMMANDS,
        error: (bytes.len() > MAX_SERVER_CONTROL_BYTES)
            .then_some(ServerControlParseError::Oversized),
    }
}

impl<'a> Iterator for ServerControlMessages<'a> {
    type Item = Result<ServerControlMessage<'a>, ServerControlParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(error) = self.error.take() {
            self.bytes = &[];
            return Some(Err(error));
        }
        if self.bytes.is_empty() {
            return None;
        }
        let error = if self.remaining == 0 {
            Some(ServerControlParseError::TooManyCommands)
        } else if self.bytes.len() < 4 {
            Some(ServerControlParseError::Truncated)
        } else {
            None
        };
        if let Some(error) = error {
            self.bytes = &[];
            return Some(Err(error));
        }
        let code = u16::from_le_bytes([self.bytes[0], self.bytes[1]]);
        let length = usize::from(u16::from_le_bytes([self.bytes[2], self.bytes[3]]));
        let Some(payload) = self.bytes.get(4..4 + length) else {
            self.bytes = &[];
            return Some(Err(ServerControlParseError::Truncated));
        };
        self.bytes = &self.bytes[4 + length..];
        self.remaining -= 1;
        Some(decode_control_message(code, payload))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ServerControlDiagnostics {
    pub(crate) unknown: u64,
    pub(crate) last_unknown: Option<u16>,
    pub(crate) malformed: u64,
    pub(crate) invalid_stream: u64,
}

fn decode_control_message(
    code: u16,
    payload: &[u8],
) -> Result<ServerControlMessage<'_>, ServerControlParseError> {
    let word = |offset| {
        payload
            .get(offset..offset + 4)
            .and_then(|bytes| bytes.try_into().ok())
            .map(u32::from_le_bytes)
            .ok_or(ServerControlParseError::Truncated)
    };
    Ok(match code {
        0x104 | 0x105 => ServerControlMessage::Timer(NvstSessionTimer {
            timer_type: if code == 0x104 { 4 } else { 5 },
            seconds_left: word(0)?,
            active: code == 0x104,
        }),
        0x109 => ServerControlMessage::Termination {
            wire_code: u32::from_be_bytes(
                payload
                    .get(..4)
                    .and_then(|bytes| bytes.try_into().ok())
                    .ok_or(ServerControlParseError::Truncated)?,
            ),
            extra: &payload[4..],
        },
        0x112 => {
            if payload.len() != 8 {
                return Err(ServerControlParseError::InvalidLength);
            }
            ServerControlMessage::FramePacingMode {
                stream_index: word(0)?,
                mode: if word(4)? == 1 {
                    FramePacingFeedbackMode::Interval
                } else {
                    FramePacingFeedbackMode::Never
                },
            }
        }
        0x101..=0x103 | 0x106 | 0x10a..=0x111 | 0x113 | 0x206 | 0x325 | 0x408 => {
            ServerControlMessage::Unhandled
        }
        _ => ServerControlMessage::Unknown(code),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_control_envelopes_are_walked_without_searching_payload_bytes() {
        let bytes = [
            0x99, 0x01, 4, 0, 0x09, 0x01, 0, 0, 0x05, 0x01, 4, 0, 0, 0, 0, 0,
        ];
        assert_eq!(
            server_control_messages(&bytes).collect::<Vec<_>>(),
            vec![
                Ok(ServerControlMessage::Unknown(0x199)),
                Ok(ServerControlMessage::Timer(NvstSessionTimer {
                    timer_type: 5,
                    seconds_left: 0,
                    active: false,
                }))
            ]
        );
    }

    #[test]
    fn server_control_envelopes_reject_truncation_without_partial_payloads() {
        for bytes in [&[4_u8, 1][..], &[4, 1, 4, 0, 60][..]] {
            let mut messages = server_control_messages(bytes);
            assert_eq!(
                messages.next(),
                Some(Err(ServerControlParseError::Truncated))
            );
            assert_eq!(messages.next(), None);
        }
    }

    #[test]
    fn server_control_work_and_bytes_are_bounded() {
        let maximum = command(0x199, &vec![0; usize::from(u16::MAX)]);
        assert_eq!(
            server_control_messages(&maximum).next(),
            Some(Ok(ServerControlMessage::Unknown(0x199)))
        );
        assert_eq!(
            server_control_messages(&vec![0; MAX_SERVER_CONTROL_BYTES + 1]).next(),
            Some(Err(ServerControlParseError::Oversized))
        );
        let bytes = [0x00, 0x01, 0, 0].repeat(MAX_SERVER_CONTROL_COMMANDS + 1);
        let messages = server_control_messages(&bytes).collect::<Vec<_>>();
        assert_eq!(messages.len(), MAX_SERVER_CONTROL_COMMANDS + 1);
        assert_eq!(
            messages.last(),
            Some(&Err(ServerControlParseError::TooManyCommands))
        );
        assert!(
            messages[..MAX_SERVER_CONTROL_COMMANDS]
                .iter()
                .all(Result::is_ok)
        );
    }

    fn command(code: u16, payload: &[u8]) -> Vec<u8> {
        let mut bytes = code.to_le_bytes().to_vec();
        bytes.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    #[test]
    fn idle_timers_preserve_little_endian_seconds_and_timer_type() {
        for (code, timer_type) in [(0x104, 4), (0x105, 5)] {
            let bytes = command(code, &60_u32.to_le_bytes());
            assert_eq!(
                server_control_messages(&bytes).next(),
                Some(Ok(ServerControlMessage::Timer(NvstSessionTimer {
                    timer_type,
                    seconds_left: 60,
                    active: timer_type == 4,
                })))
            );
            for length in 0..4 {
                assert_eq!(
                    server_control_messages(&command(code, &vec![0; length])).next(),
                    Some(Err(ServerControlParseError::Truncated))
                );
            }
        }
    }

    #[test]
    fn termination_preserves_network_order_result_and_opaque_tail() {
        let bytes = command(0x109, &[0x80, 0x03, 0x00, 0x23, 1, 2, 3]);
        assert_eq!(
            server_control_messages(&bytes).next(),
            Some(Ok(ServerControlMessage::Termination {
                wire_code: 0x8003_0023,
                extra: &bytes[8..],
            }))
        );
        assert_eq!(
            server_control_messages(&command(0x109, &[0; 3])).next(),
            Some(Err(ServerControlParseError::Truncated))
        );
    }

    fn extended(records: &[u8]) -> NvstServerTermination {
        NvstServerTermination::from_wire(EXTENDED_TEARDOWN, records)
    }

    #[test]
    fn shutdown_records_use_the_first_session_termination_reason() {
        for (records, shutdown_reason) in [
            (&[0, 0, 0x02, 0x05, 0, 0, 0x01, 0x04][..], 0x205),
            (&[0, 1, 0, 0, 0, 0, 0x02, 0x05][..], 0x205),
            (&[0, 1, 0, 2, 0xaa, 0xbb, 0, 0, 0x01, 0x04][..], 0x104),
            (&[0, 0, 0xff, 0xff, 0, 0, 0x01, 0x04][..], 0xffff),
            (
                &[0, 0, 0x01, 0x04, 0, 2, 0xff, 0xff, 0, 0, 0x02, 0x05][..],
                0x104,
            ),
        ] {
            assert_eq!(extended(records).shutdown_reason, Some(shutdown_reason));
        }
    }

    #[test]
    fn malformed_or_unknown_shutdown_records_fall_back_to_unknown_reason() {
        for records in [
            &[][..],
            &[0][..],
            &[0, 0, 0x01][..],
            &[0, 2][..],
            &[0, 2, 0xff, 0xff, 0, 0, 0x01, 0x04][..],
            &[0, 1, 0, 2, 0xaa, 0xbb][..],
            &[0, 1, 0, 0xff, 0, 0, 0x01, 0x04][..],
        ] {
            assert_eq!(
                extended(records),
                NvstServerTermination {
                    wire_code: EXTENDED_TEARDOWN,
                    shutdown_reason: None,
                    reason: 0x35,
                    session_alive: false,
                    extended_code: 0xc0f5_2003,
                }
            );
        }
    }

    #[test]
    fn shutdown_reasons_select_notification_liveness_and_extended_result() {
        for (shutdown_reason, reason, session_alive, extended_code) in [
            (0x0000_u16, 0x40, false, 0x8011_0000),
            (0x0001, 0x4a, true, 0x8011_0001),
            (0x0100, 0x34, false, 0x8003_0000),
            (0x0102, 0x4b, true, 0x8003_0004),
            (0x0104, 0x38, false, 0x8003_0007),
            (0x010a, 0x45, true, 0x8003_001e),
            (0x0201, 0x36, false, 0x8003_0009),
            (0x0205, 0x39, false, 0x8003_000c),
            (0x0206, 0x3b, true, 0x8003_000e),
            (0x810f, 0x44, false, 0x8003_001a),
            (0x8006, 0x35, false, 0x8004_0010),
            (0x8605, 0x35, false, 0x800e_8605),
            (0x0300, 0x48, false, 0x00f1_3001),
            (0x0209, 0x49, false, 0x00f1_3002),
            (0x010e, 0x35, false, 0x00f1_3003),
            (0x010f, 0x4c, false, 0x00f1_3004),
            (0xfffe, 0x35, false, 0x8003_0000),
        ] {
            let termination = extended(&[[0, 0], shutdown_reason.to_be_bytes()].concat());
            assert_eq!(
                termination.disposition(),
                match shutdown_reason {
                    0x0100 => "normal",
                    0x0104 => "idle",
                    0x0205 => "time-limit",
                    _ => "error",
                }
            );
            assert_eq!(
                termination,
                NvstServerTermination {
                    wire_code: EXTENDED_TEARDOWN,
                    shutdown_reason: Some(shutdown_reason),
                    reason,
                    session_alive,
                    extended_code,
                }
            );
            assert!(!termination.resumable());
        }
    }

    #[test]
    fn direct_server_results_pass_through_alive_with_their_notification() {
        for (wire_code, reason) in [
            (0x0000_0000_u32, 0x35),
            (0x8003_0000, 0x34),
            (0x8003_0007, 0x35),
            (0x8011_0000, 0x34),
            (0x8011_0001, 0x34),
            (0x8011_0002, 0x35),
            (0x8003_0006, 0x3d),
            (0x8003_001a, 0x44),
            (0x8003_0024, 0x35),
            (0x8004_0010, 0x35),
            (0xffff_ffff, 0x35),
        ] {
            assert_eq!(
                NvstServerTermination::from_wire(wire_code, &[0, 0, 0x01, 0x04]),
                NvstServerTermination {
                    wire_code,
                    shutdown_reason: None,
                    reason,
                    session_alive: true,
                    extended_code: wire_code,
                }
            );
        }
    }

    #[test]
    fn server_feedback_mode_requires_exact_payload_and_normalizes_invalid_modes() {
        for (raw, mode) in [
            (0_u32, FramePacingFeedbackMode::Never),
            (1, FramePacingFeedbackMode::Interval),
            (2, FramePacingFeedbackMode::Never),
            (u32::MAX, FramePacingFeedbackMode::Never),
        ] {
            let payload = [2_u32.to_le_bytes(), raw.to_le_bytes()].concat();
            assert_eq!(
                server_control_messages(&command(0x112, &payload)).next(),
                Some(Ok(ServerControlMessage::FramePacingMode {
                    stream_index: 2,
                    mode
                }))
            );
        }
        for length in [0, 4, 7, 9, 12] {
            assert_eq!(
                server_control_messages(&command(0x112, &vec![0; length])).next(),
                Some(Err(ServerControlParseError::InvalidLength))
            );
        }
    }

    #[test]
    fn control_keepalive_failures_are_counted_and_reset_on_admission() {
        let mut health = ControlKeepaliveHealth::default();
        health.record(false);
        health.record(false);
        assert_eq!(health.failures, 2);
        assert_eq!(health.consecutive_failures, 2);
        health.record(true);
        assert_eq!(health.failures, 2);
        assert_eq!(health.consecutive_failures, 0);
        assert_eq!(health.recoveries, 1);
        health.record(true);
        assert_eq!(health.recoveries, 1);
    }

    #[test]
    fn control_keepalive_counters_saturate() {
        let mut health = ControlKeepaliveHealth {
            failures: u64::MAX,
            consecutive_failures: u32::MAX,
            recoveries: u64::MAX,
        };
        health.record(false);
        assert_eq!(health.failures, u64::MAX);
        assert_eq!(health.consecutive_failures, u32::MAX);
        health.record(true);
        assert_eq!(health.recoveries, u64::MAX);
        assert_eq!(health.consecutive_failures, 0);
    }

    #[test]
    fn geronimo_resumability_matches_reason_and_extended_code_predicate() {
        for code in 0x8004_0000..=0x8004_0017 {
            assert_eq!(
                session_resumable(Some(0x37), Some(code)),
                Some(!matches!(
                    code,
                    0x8004_0003 | 0x8004_000b..=0x8004_000f | 0x8004_0011..=0x8004_0016
                ))
            );
            assert_eq!(
                session_resumable(Some(0x3b), Some(code)),
                Some(matches!(code, 0x8004_0004 | 0x8004_0007 | 0x8004_0010))
            );
        }
        assert_eq!(
            session_resumable(Some(0x38), Some(0x8003_0007)),
            Some(false)
        );
        assert_eq!(session_resumable(None, Some(0x8004_0010)), None);
        assert_eq!(session_resumable(Some(0x37), None), None);
        assert_eq!(session_resumable(Some(0x3b), None), None);
    }
}
