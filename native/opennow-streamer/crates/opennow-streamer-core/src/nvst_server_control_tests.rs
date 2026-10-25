use super::*;
use opennow_streamer_transport::{NvstServerTermination, NvstSessionTimer};

#[test]
fn text_rejection_is_nonterminal_and_retries_after_callback_backpressure() {
    let (events, received) = mpsc::sync_channel(1);
    let output = EventSender::bounded(events);
    let lifecycle = connected_lifecycle();
    let resources = TestNvstResources::default();
    let mut control = NvstControlOutput { start_id: "paste-session".to_owned(), ..Default::default() };
    output.send(json!({"type":"log"})).unwrap();
    assert!(!forward_nvst_event(&output, &lifecycle, 7, &resources, &mut 0, &mut control, None, NvstReceiveEvent::TextRejected("paste-queue-full")));
    assert_eq!(control.pending_text_error, Some("paste-queue-full"));
    received.try_recv().unwrap();
    flush_control_output(&output, &lifecycle, 7, &mut control);
    assert_eq!(received.try_recv().unwrap(), json!({"type":"text-input-error", "startId":"paste-session", "reason":"paste-queue-full"}));
    assert_eq!(resources.stops.load(Ordering::Relaxed), 0);
    assert_eq!(lock_lifecycle(&lifecycle).state, State::Connected);
    assert!(control.pending_text_error.is_none());
}

#[test]
fn idle_timer_and_input_clear_events_do_not_disturb_media() {
    let (events, received) = mpsc::channel();
    let output = EventSender::unbounded(events);
    let lifecycle = connected_lifecycle();
    let resources = TestNvstResources::default();
    let mut control = NvstControlOutput {
        start_id: "timer-session".to_owned(),
        ..Default::default()
    };
    let mut attempts = 0;
    for event in [
        NvstReceiveEvent::SessionTimer(NvstSessionTimer {
            timer_type: 4,
            seconds_left: 60,
            active: true,
        }),
        NvstReceiveEvent::UserInput,
        NvstReceiveEvent::SessionTimer(NvstSessionTimer {
            timer_type: 5,
            seconds_left: 0,
            active: false,
        }),
    ] {
        assert!(!forward_nvst_event(
            &output,
            &lifecycle,
            7,
            &resources,
            &mut attempts,
            &mut control,
            None,
            event
        ));
    }
    let events = received.try_iter().collect::<Vec<_>>();
    assert_eq!(events.len(), 3);
    assert_eq!(
        events[0],
        json!({"type":"session-timer", "startId":"timer-session", "timerType":4,"secondsLeft":60,"active":true})
    );
    assert_eq!(events[1]["active"], false);
    assert_eq!(events[2]["timerType"], 5);
    assert_eq!(lock_lifecycle(&lifecycle).state, State::Connected);
    assert_eq!(resources.stops.load(Ordering::Relaxed), 0);
    assert_eq!(resources.recoveries.load(Ordering::Relaxed), 0);
    assert_eq!(resources.keyframe_requests.load(Ordering::Relaxed), 0);
}

#[test]
fn idle_timer_retries_only_the_latest_state_after_event_backpressure() {
    let (events, received) = mpsc::sync_channel(1);
    let output = EventSender::bounded(events);
    let lifecycle = connected_lifecycle();
    let resources = TestNvstResources::default();
    let mut control = NvstControlOutput::default();
    output.send(json!({"type":"log"})).unwrap();
    for event in [
        NvstReceiveEvent::SessionTimer(NvstSessionTimer {
            timer_type: 4,
            seconds_left: 60,
            active: true,
        }),
        NvstReceiveEvent::UserInput,
    ] {
        assert!(!forward_nvst_event(
            &output,
            &lifecycle,
            7,
            &resources,
            &mut 0,
            &mut control,
            None,
            event
        ));
    }
    assert_eq!(
        control.pending_timer,
        Some(NvstSessionTimer {
            timer_type: 4,
            seconds_left: 0,
            active: false,
        })
    );
    received.try_recv().unwrap();
    flush_control_output(&output, &lifecycle, 7, &mut control);
    assert_eq!(received.try_recv().unwrap()["active"], false);
    assert!(control.pending_timer.is_none());
}

#[test]
fn server_termination_preserves_classification_and_stops_every_leg_once() {
    for (shutdown_reason, reason, session_alive, extended_code, resumable) in [
        (Some(0x104_u16), 0x38_u32, false, 0x8003_0007_u32, false),
        (Some(0x0206), 0x3b, true, 0x8003_000e, false),
        (None, 0x37, true, 0x8004_0010, true),
    ] {
        let (events, received) = mpsc::channel();
        let output = EventSender::unbounded(events);
        let lifecycle = connected_lifecycle();
        let resources = TestNvstResources::default();
        let termination = NvstServerTermination {
            wire_code: 0x8003_0023,
            shutdown_reason,
            reason,
            session_alive,
            extended_code,
        };
        for _ in 0..2 {
            assert!(forward_nvst_event(
                &output,
                &lifecycle,
                7,
                &resources,
                &mut 0,
                &mut NvstControlOutput::default(),
                None,
                NvstReceiveEvent::ServerTermination(termination)
            ));
        }
        let events = received.try_iter().collect::<Vec<_>>();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["type"], "error");
        assert_eq!(events[1]["status"], "stopped");
        for event in events {
            assert_eq!(event["termination"]["source"], "nvst-server");
            assert_eq!(event["termination"]["wireCode"], 0x8003_0023_u32);
            assert_eq!(event["termination"]["shutdownReason"], json!(shutdown_reason));
            assert_eq!(event["termination"]["reason"], json!(reason));
            assert_eq!(event["termination"]["sessionAlive"], session_alive);
            assert_eq!(event["termination"]["extendedCode"], json!(extended_code));
            assert_eq!(event["termination"]["resumable"], resumable);
        }
        assert_eq!(resources.stops.load(Ordering::Relaxed), 1);
        assert_eq!(resources.recoveries.load(Ordering::Relaxed), 0);
        assert_eq!(lock_lifecycle(&lifecycle).state, State::Idle);
    }
}

#[test]
fn stale_server_control_events_cannot_change_a_replacement_session() {
    let (events, received) = mpsc::channel();
    let output = EventSender::unbounded(events);
    let lifecycle = connected_lifecycle();
    let resources = TestNvstResources::default();
    for event in [
        NvstReceiveEvent::SessionTimer(NvstSessionTimer {
            timer_type: 4,
            seconds_left: 60,
            active: true,
        }),
        NvstReceiveEvent::ServerTermination(NvstServerTermination {
            wire_code: 0x8003_0023,
            shutdown_reason: Some(0x104),
            reason: 0x38,
            session_alive: false,
            extended_code: 0x8003_0007,
        }),
    ] {
        assert!(forward_nvst_event(
            &output,
            &lifecycle,
            6,
            &resources,
            &mut 0,
            &mut NvstControlOutput::default(),
            None,
            event
        ));
    }
    assert!(received.try_recv().is_err());
    assert_eq!(resources.stops.load(Ordering::Relaxed), 0);
    assert_eq!(lock_lifecycle(&lifecycle).state, State::Connected);
}

#[test]
fn server_termination_during_setup_is_not_mistaken_for_a_duplicate() {
    let (events, received) = mpsc::channel();
    let output = EventSender::unbounded(events);
    let lifecycle = connected_lifecycle();
    lock_lifecycle(&lifecycle).state = State::Idle;
    let resources = TestNvstResources::default();
    assert!(forward_nvst_event(&output, &lifecycle, 7, &resources, &mut 0, &mut NvstControlOutput::default(), None, NvstReceiveEvent::ServerTermination(NvstServerTermination {wire_code: 0x8003_0023, shutdown_reason: Some(0x104), reason: 0x38, session_alive: false, extended_code: 0x8003_0007})));
    assert_eq!(resources.stops.load(Ordering::Relaxed), 1);
    assert_eq!(received.try_iter().count(), 2);
}

#[test]
fn terminal_events_survive_a_full_bounded_callback_queue() {
    let (events, received) = mpsc::sync_channel(1);
    let output = EventSender::bounded(events);
    output.send(json!({"type":"log"})).unwrap();
    let worker = thread::spawn(move || {
        let resources = TestNvstResources::default();
        assert!(forward_nvst_event(
            &output,
            &connected_lifecycle(),
            7,
            &resources,
            &mut 0,
            &mut NvstControlOutput::default(),
            None,
            NvstReceiveEvent::ServerTermination(NvstServerTermination {
                wire_code: 0x8004_0010,
                shutdown_reason: None,
                reason: 0x37,
                session_alive: true,
                extended_code: 0x8004_0010
            })
        ));
        assert_eq!(resources.stops.load(Ordering::Relaxed), 1);
    });
    assert_eq!(
        received.recv_timeout(Duration::from_secs(2)).unwrap()["type"],
        "log"
    );
    assert_eq!(
        received.recv_timeout(Duration::from_secs(2)).unwrap()["termination"]["resumable"],
        true
    );
    assert_eq!(
        received.recv_timeout(Duration::from_secs(2)).unwrap()["status"],
        "stopped"
    );
    worker.join().unwrap();
}
