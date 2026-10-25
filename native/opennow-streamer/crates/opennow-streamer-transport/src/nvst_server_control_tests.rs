use super::*;

fn server_command(code: u16, payload: &[u8]) -> Vec<u8> {
    let mut bytes = code.to_le_bytes().to_vec();
    bytes.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

#[test]
fn inbound_idle_warning_and_expiry_update_state_without_ending_media() {
    let feedback = NvstFeedbackState::default();
    let mut diagnostics = ServerControlDiagnostics::default();
    let (events, received) = mpsc::channel();
    assert!(!feedback.receive_server_control(
        &server_command(0x104, &60_u32.to_le_bytes()),
        &mut diagnostics,
        &events,
    ));
    assert!(feedback.idle_warning_active.load(Ordering::Acquire));
    assert_eq!(
        received.try_recv().unwrap(),
        NvstReceiveEvent::SessionTimer(NvstSessionTimer {
            timer_type: 4,
            seconds_left: 60,
            active: true,
        })
    );
    assert!(!feedback.receive_server_control(
        &server_command(0x105, &0_u32.to_le_bytes()),
        &mut diagnostics,
        &events,
    ));
    assert!(!feedback.idle_warning_active.load(Ordering::Acquire));
    assert_eq!(
        received.try_recv().unwrap(),
        NvstReceiveEvent::SessionTimer(NvstSessionTimer {
            timer_type: 5,
            seconds_left: 0,
            active: false,
        })
    );
    assert_eq!(diagnostics, ServerControlDiagnostics::default());
}

#[test]
fn zero_second_warning_remains_active_until_clear_or_expiry() {
    let feedback = NvstFeedbackState::default();
    let (events, received) = mpsc::channel();
    assert!(!feedback.receive_server_control(
        &server_command(0x104, &0_u32.to_le_bytes()),
        &mut ServerControlDiagnostics::default(),
        &events,
    ));
    assert!(feedback.idle_warning_active.load(Ordering::Acquire));
    assert!(matches!(received.try_recv().unwrap(), NvstReceiveEvent::SessionTimer(NvstSessionTimer { active: true, seconds_left: 0, .. })));
}

#[test]
fn server_termination_preserves_wire_code_and_is_emitted_before_shutdown() {
    for (wire_code, records, shutdown_reason, reason, session_alive, extended_code) in [
        (0x8003_0023_u32, &[][..], None, 0x35, false, 0xc0f5_2003),
        (
            0x8003_0023,
            &[0, 1, 0, 2, 0xaa, 0xbb, 0, 0, 0x01, 0x04][..],
            Some(0x104),
            0x38,
            false,
            0x8003_0007,
        ),
        (0x8003_0023, &[0, 0, 0x80, 0x06][..], Some(0x8006), 0x35, false, 0x8004_0010),
        (0x8004_0010, &[0, 0, 0x01, 0x04][..], None, 0x35, true, 0x8004_0010),
        (0xdead_beef, &[][..], None, 0x35, true, 0xdead_beef),
    ] {
        let feedback = NvstFeedbackState::default();
        feedback.idle_warning_active.store(true, Ordering::Release);
        let (events, received) = mpsc::channel();
        assert!(feedback.receive_server_control(
            &server_command(0x109, &[&wire_code.to_be_bytes()[..], records].concat()),
            &mut ServerControlDiagnostics::default(),
            &events
        ));
        let NvstReceiveEvent::ServerTermination(termination) = received.try_recv().unwrap() else {
            panic!("termination event required")
        };
        assert_eq!(
            termination,
            NvstServerTermination {
                wire_code,
                shutdown_reason,
                reason,
                session_alive,
                extended_code,
            }
        );
        assert!(!termination.resumable());
        assert!(!feedback.idle_warning_active.load(Ordering::Acquire));
    }
}

#[test]
fn server_feedback_mode_updates_only_the_owned_video_stream() {
    let feedback = NvstFeedbackState::default();
    let mut diagnostics = ServerControlDiagnostics::default();
    let (events, received) = mpsc::channel();
    for (stream, mode, enabled) in [
        (0_u32, 0_u32, false),
        (0, 1, true),
        (1, 0, true),
        (0, 99, false),
    ] {
        assert!(!feedback.receive_server_control(
            &server_command(0x112, &[stream.to_le_bytes(), mode.to_le_bytes()].concat()),
            &mut diagnostics,
            &events
        ));
        assert_eq!(
            feedback.frame_pacing_enabled.load(Ordering::Acquire),
            enabled
        );
    }
    assert_eq!(diagnostics.invalid_stream, 1);
    assert!(received.try_recv().is_err());
}

#[test]
fn unknown_and_malformed_server_commands_are_counted_not_terminal() {
    let feedback = NvstFeedbackState::default();
    let mut diagnostics = ServerControlDiagnostics::default();
    let (events, received) = mpsc::channel();
    let bytes = [
        server_command(0x999, &[1, 2, 3]),
        server_command(0x104, &[0]),
        server_command(0x112, &[0; 8]),
    ]
    .concat();
    assert!(!feedback.receive_server_control(&bytes, &mut diagnostics, &events));
    assert_eq!(diagnostics.unknown, 1);
    assert_eq!(diagnostics.last_unknown, Some(0x999));
    assert_eq!(diagnostics.malformed, 1);
    assert!(!feedback.frame_pacing_enabled.load(Ordering::Acquire));
    assert!(received.try_recv().is_err());
    diagnostics.unknown = u64::MAX;
    assert!(!feedback.receive_server_control(
        &server_command(0x999, &[]),
        &mut diagnostics,
        &events
    ));
    assert_eq!(diagnostics.unknown, u64::MAX);
}
