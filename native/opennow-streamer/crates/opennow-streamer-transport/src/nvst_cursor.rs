use std::time::{Duration, Instant};

const RETRY_INTERVAL: Duration = Duration::from_millis(250);
const MAX_ATTEMPTS: u8 = 8;
const SILENT_HOST_TIMEOUT: Duration = Duration::from_secs(3);

pub(crate) fn valid_cursor_channel_message(bytes: &[u8]) -> bool {
    if bytes.len() < 7 || !matches!(bytes[0], 0 | 1) {
        return false;
    }
    let image_length_offset = 5 + usize::from(bytes[4]);
    let Some(length) = bytes.get(image_length_offset..image_length_offset + 2) else {
        return false;
    };
    let image_length = usize::from(u16::from_le_bytes([length[0], length[1]]));
    let image_end = image_length_offset + 2 + image_length;
    bytes.len() >= image_end
        && matches!(bytes.len() - image_end, 0 | 4 | 6)
        && (bytes[0] != 0 || (bytes[4] == 0 && image_length == 0))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CursorCommand {
    Capture(bool),
    Tracking,
}

#[derive(Debug, Default)]
enum CaptureState {
    #[default]
    Inactive,
    Waiting {
        deadline: Instant,
        retry_at: Instant,
        attempts: u8,
    },
    Disabling {
        retry_at: Instant,
        attempts: u8,
        reason: &'static str,
    },
    Local,
    Exhausted,
}

#[derive(Debug, Default)]
pub(crate) struct NvstCursorCapture {
    state: CaptureState,
}

impl NvstCursorCapture {
    pub(crate) fn activate(&mut self, now: Instant) {
        self.state = CaptureState::Waiting {
            deadline: now + SILENT_HOST_TIMEOUT,
            retry_at: now + RETRY_INTERVAL,
            attempts: 1,
        };
    }

    pub(crate) fn reset(&mut self) {
        self.state = CaptureState::Inactive;
    }

    pub(crate) fn notify(&mut self, now: Instant) {
        if matches!(self.state, CaptureState::Waiting { .. }) {
            self.state = CaptureState::Disabling {
                retry_at: now,
                attempts: 0,
                reason: "cursor-notification",
            };
        }
    }

    pub(crate) fn update(
        &mut self,
        now: Instant,
        mut send: impl FnMut(CursorCommand) -> bool,
    ) -> bool {
        if let CaptureState::Waiting { deadline, .. } = self.state
            && now >= deadline
        {
            self.state = CaptureState::Disabling {
                retry_at: now,
                attempts: 0,
                reason: "silent-host-timeout",
            };
        }
        match &mut self.state {
            CaptureState::Waiting {
                retry_at, attempts, ..
            } if *attempts < MAX_ATTEMPTS && now >= *retry_at => {
                *attempts += 1;
                let capture_sent = send(CursorCommand::Capture(true));
                let tracking_sent = send(CursorCommand::Tracking);
                eprintln!(
                    "NVST cursor feature retry: attempt={attempts} captureSent={capture_sent} trackingSent={tracking_sent}"
                );
                *retry_at = now + RETRY_INTERVAL;
            }
            CaptureState::Disabling {
                retry_at,
                attempts,
                reason,
            } if now >= *retry_at => {
                *attempts += 1;
                let sent = send(CursorCommand::Capture(false));
                eprintln!(
                    "NVST cursor capture tx: command=0x0308 enabled=false reason={reason} attempt={attempts} queued={sent}"
                );
                if sent {
                    self.state = CaptureState::Local;
                    return true;
                }
                if *attempts == MAX_ATTEMPTS {
                    eprintln!(
                        "NVST cursor capture disable retries exhausted; retaining server-composited cursor until reactivation"
                    );
                    self.state = CaptureState::Exhausted;
                } else {
                    *retry_at = now + RETRY_INTERVAL;
                }
            }
            _ => {}
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_channel_requires_complete_supported_framing() {
        assert!(valid_cursor_channel_message(&[0, 0, 0, 0, 0, 0, 0]));
        assert!(valid_cursor_channel_message(&[
            0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0
        ]));
        let custom = [
            1, 0, 1, 2, 3, b'p', b'n', b'g', 4, 0, b'a', b'b', b'c', b'd',
        ];
        assert!(valid_cursor_channel_message(&custom));
        for len in 0..custom.len() {
            assert!(!valid_cursor_channel_message(&custom[..len]));
        }
        assert!(!valid_cursor_channel_message(&[2, 1, 0, 0, 0, 0, 0]));
        assert!(!valid_cursor_channel_message(&[0, 1, 0, 0, 0, 1, 0, 42]));
        assert!(!valid_cursor_channel_message(&[0, 1, 0, 0, 0, 0, 0, 0]));
    }

    #[test]
    fn truncated_native_bitmaps_do_not_trigger_handoff() {
        let now = Instant::now();
        let mut capture = NvstCursorCapture::default();
        capture.activate(now);
        for length in 0..8_u8 {
            let mut bytes = vec![0x10, 0x01, length, 0];
            bytes.resize(4 + usize::from(length), 0);
            assert!(crate::nvst_input::server_cursor_messages(&bytes).is_empty());
        }
        assert!(!capture.update(now, |_| panic!("no valid notification")));
    }

    #[test]
    fn notification_hands_off_once_and_stops_enable_retries() {
        let now = Instant::now();
        let mut capture = NvstCursorCapture::default();
        capture.activate(now);
        capture.notify(now);
        let mut commands = Vec::new();
        assert!(capture.update(now, |command| {
            commands.push(command);
            true
        }));
        capture.notify(now + RETRY_INTERVAL);
        assert!(!capture.update(now + SILENT_HOST_TIMEOUT, |command| {
            commands.push(command);
            true
        }));
        assert_eq!(commands, [CursorCommand::Capture(false)]);
    }

    #[test]
    fn bitmap_notification_hands_off_without_synthesizing_shape_or_mode() {
        let now = Instant::now();
        let mut capture = NvstCursorCapture::default();
        capture.activate(now);
        let messages = crate::nvst_input::server_cursor_messages(&[
            0x10, 0x01, 0x08, 0x00, 0xde, 0xad, 0xbe, 0xef, 0, 0, 0, 0,
        ]);
        assert_eq!(messages.len(), 1);
        for message in messages {
            assert_eq!(message.normalized, None);
            assert_eq!(message.visible, None);
            capture.notify(now);
        }
        assert!(capture.update(now, |command| {
            assert_eq!(command, CursorCommand::Capture(false));
            true
        }));
    }

    #[test]
    fn silent_host_falls_back_after_bounded_enable_retries() {
        let now = Instant::now();
        let mut capture = NvstCursorCapture::default();
        capture.activate(now);
        let mut commands = Vec::new();
        for tick in 0..12 {
            assert!(!capture.update(now + RETRY_INTERVAL * tick, |command| {
                commands.push(command);
                true
            }));
        }
        assert_eq!(commands.len(), usize::from(MAX_ATTEMPTS - 1) * 2);
        assert!(
            commands
                .chunks_exact(2)
                .all(|pair| { pair == [CursorCommand::Capture(true), CursorCommand::Tracking] })
        );
        assert!(capture.update(now + SILENT_HOST_TIMEOUT, |command| {
            assert_eq!(command, CursorCommand::Capture(false));
            true
        }));
    }

    #[test]
    fn failed_disable_is_paced_and_reports_handoff_only_on_success() {
        let now = Instant::now();
        let mut capture = NvstCursorCapture::default();
        capture.activate(now);
        capture.notify(now);
        assert!(!capture.update(now, |command| {
            assert_eq!(command, CursorCommand::Capture(false));
            false
        }));
        capture.notify(now);
        assert!(!capture.update(now, |_| panic!("retry must be paced")));
        assert!(capture.update(now + RETRY_INTERVAL, |command| {
            assert_eq!(command, CursorCommand::Capture(false));
            true
        }));
    }

    #[test]
    fn exhausted_disable_retries_do_not_report_local_composition() {
        let now = Instant::now();
        let mut capture = NvstCursorCapture::default();
        capture.activate(now);
        capture.notify(now);
        let mut attempts = 0;
        for tick in 0..20 {
            capture.notify(now + RETRY_INTERVAL * tick);
            assert!(!capture.update(now + RETRY_INTERVAL * tick, |command| {
                assert_eq!(command, CursorCommand::Capture(false));
                attempts += 1;
                false
            }));
        }
        assert_eq!(attempts, MAX_ATTEMPTS);
        capture.activate(now);
        capture.notify(now);
        assert!(capture.update(now, |_| true));
    }

    #[test]
    fn reset_cancels_retries_and_reactivation_rearms_local_handoff() {
        let now = Instant::now();
        let mut capture = NvstCursorCapture::default();
        capture.notify(now);
        assert!(!capture.update(now, |_| panic!("inactive capture")));
        capture.activate(now);
        capture.notify(now);
        capture.reset();
        assert!(!capture.update(now + SILENT_HOST_TIMEOUT, |_| panic!("closed channel")));
        capture.activate(now);
        capture.notify(now);
        assert!(capture.update(now, |_| true));
        capture.activate(now);
        assert!(capture.update(now + SILENT_HOST_TIMEOUT, |command| {
            assert_eq!(command, CursorCommand::Capture(false));
            true
        }));
    }

    #[test]
    fn failed_enable_retries_do_not_cancel_silent_host_deadline() {
        let now = Instant::now();
        let mut capture = NvstCursorCapture::default();
        capture.activate(now);
        assert!(!capture.update(now + RETRY_INTERVAL, |_| false));
        assert!(capture.update(now + SILENT_HOST_TIMEOUT, |command| {
            assert_eq!(command, CursorCommand::Capture(false));
            true
        }));
    }
}
