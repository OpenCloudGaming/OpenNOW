use super::*;

use std::sync::mpsc::Sender;

fn progress_policy() -> NvstFrameProgressPolicy {
    NvstFrameProgressPolicy {
        stall: Duration::from_secs(2),
        keyframe_grace: Duration::from_secs(1),
    }
}

struct DeliveryHarness {
    media: MediaConsumer,
    events: Sender<NvstReceiveEvent>,
    feedback: SharedNvstFeedback,
    gaps: Vec<bool>,
    resumed: usize,
}

impl DeliveryHarness {
    fn new(feedback: SharedNvstFeedback) -> Self {
        let (media, _media_receiver) = std::sync::mpsc::sync_channel(512);
        let (events, _event_receiver) = std::sync::mpsc::channel();
        Self {
            media,
            events,
            feedback,
            gaps: Vec::new(),
            resumed: 0,
        }
    }

    fn deliver(
        &mut self,
        receiver: &mut NvstVideoReceiver,
        source: SocketAddr,
        datagram: &[u8],
        now: Instant,
    ) {
        self.gaps.push(false);
        let gap = self.gaps.last_mut().expect("gap slot");
        for event in receiver.process_datagram(source, datagram, now) {
            if matches!(event, NvstReceiveEvent::FrameProgressResumed) {
                self.resumed += 1;
            }
            let _ = forward_receive_event(
                &self.media,
                &self.events,
                &self.feedback,
                now,
                now,
                gap,
                event,
            );
        }
    }
}

fn protect_partial_frame(crypto: &SrtpReceiver, index: u16, frame_index: u32) -> Vec<u8> {
    let mut flags = FLAG_CONTAINS_PIC_DATA;
    if index == 0 {
        flags |= FLAG_SOF;
    }
    let mut payload = vec![0x55_u8; 64];
    if index == 0 {
        payload[..5].copy_from_slice(&[0, 0, 0, 1, 0x61]);
    }
    protect_for_test(
        crypto,
        build_plaintext_rtp(8_000 + index, flags, frame_index, &payload),
        0,
    )
}

fn protect_complete_frame(crypto: &SrtpReceiver, sequence: u16, frame_index: u32) -> Vec<u8> {
    protect_for_test(
        crypto,
        build_plaintext_rtp(
            sequence,
            FLAG_SOF | FLAG_EOF | FLAG_CONTAINS_PIC_DATA,
            frame_index,
            &[0, 0, 0, 1, 0x65, 0x11, 0x22],
        ),
        0,
    )
}

#[test]
fn first_keyframe_deadline_survives_continuing_non_keyframes() {
    let config = config();
    let crypto = test_srtp(&config);
    let feedback = config.feedback();
    let mut receiver = NvstVideoReceiver::new(config);
    let mut harness = DeliveryHarness::new(feedback.clone());
    let origin = receiver.timeout_origin;
    let policy = progress_policy();
    let mut events = Vec::new();

    for index in 0..9_u16 {
        let now = origin + Duration::from_millis(400) * u32::from(index);
        let packet = protect_for_test(
            &crypto,
            build_plaintext_rtp(
                400 + index,
                FLAG_SOF | FLAG_EOF | FLAG_CONTAINS_PIC_DATA,
                u32::from(index),
                &[0, 0, 0, 1, 0x61, 0x11],
            ),
            0,
        );
        harness.deliver(&mut receiver, peer(), &packet, now);
        if let Some(event) = receiver.poll_frame_progress(now, policy) {
            events.push((now.saturating_duration_since(origin), event));
        }
    }

    assert_eq!(feedback.frame_stage_timings().assembled_frames_total, 9);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].0, Duration::from_secs(2));
    assert!(matches!(events[0].1, NvstFrameProgressEvent::KeyframeRequested { .. }));
    assert_eq!(events[1].0, Duration::from_millis(3_200));
    assert!(matches!(events[1].1, NvstFrameProgressEvent::RecoveryNeeded { .. }));
    assert!(feedback.keyframe_request_pending());
}

#[test]
fn first_keyframe_inside_grace_clears_pending_recovery() {
    let config = config();
    let crypto = test_srtp(&config);
    let feedback = config.feedback();
    let mut receiver = NvstVideoReceiver::new(config);
    let mut harness = DeliveryHarness::new(feedback.clone());
    let origin = receiver.timeout_origin;
    let policy = progress_policy();

    for index in 0..7_u16 {
        let now = origin + Duration::from_millis(400) * u32::from(index);
        let packet = protect_for_test(
            &crypto,
            build_plaintext_rtp(
                500 + index,
                FLAG_SOF | FLAG_EOF | FLAG_CONTAINS_PIC_DATA,
                u32::from(index),
                &[0, 0, 0, 1, 0x61, 0x11],
            ),
            0,
        );
        harness.deliver(&mut receiver, peer(), &packet, now);
        let result = receiver.poll_frame_progress(now, policy);
        if index == 5 {
            assert!(matches!(result, Some(NvstFrameProgressEvent::KeyframeRequested { .. })));
        } else {
            assert_eq!(result, None);
        }
    }
    assert!(feedback.keyframe_request_pending());
    let keyframe_at = origin + Duration::from_millis(2_800);
    let keyframe = protect_complete_frame(&crypto, 507, 7);
    harness.deliver(&mut receiver, peer(), &keyframe, keyframe_at);
    assert_eq!(harness.resumed, 1);
    assert!(!feedback.keyframe_request_pending());
    assert_eq!(receiver.poll_frame_progress(keyframe_at + Duration::from_millis(400), policy), None);
    assert_eq!(receiver.frame_progress().stage, NvstFrameProgressStage::Tracking);
}

#[test]
fn incomplete_initial_idr_does_not_close_grace_before_a_complete_reference() {
    let config = config();
    let crypto = test_srtp(&config);
    let feedback = config.feedback();
    let mut receiver = NvstVideoReceiver::new(config);
    let origin = receiver.timeout_origin;
    let policy = progress_policy();
    let partial = protect_for_test(
        &crypto,
        build_plaintext_rtp(900, FLAG_SOF | FLAG_CONTAINS_PIC_DATA, 1, &[0, 0, 0, 1, 0x65]),
        0,
    );
    assert!(receiver.process_datagram(peer(), &partial, origin).is_empty());
    assert_eq!(receiver.poll_frame_progress(origin, policy), None);
    assert!(matches!(
        receiver.poll_frame_progress(origin + Duration::from_secs(2), policy),
        Some(NvstFrameProgressEvent::KeyframeRequested { .. })
    ));
    assert!(feedback.keyframe_request_pending());

    let incomplete_at = origin + Duration::from_millis(2_400);
    let mut missing_middle = build_plaintext_rtp(901, FLAG_EOF, 1, &[0xaa]);
    missing_middle[16..20].copy_from_slice(&(902_u32 << 8).to_le_bytes());
    let missing_middle = protect_for_test(&crypto, missing_middle, 0);
    assert_eq!(
        receiver.process_datagram(peer(), &missing_middle, incomplete_at),
        [NvstReceiveEvent::Dropped(NvstDropReason::FrameDiscontinuity)]
    );
    assert_eq!(receiver.poll_frame_progress(incomplete_at, policy), None);
    assert_eq!(receiver.frame_progress().stage, NvstFrameProgressStage::KeyframePending);
    assert!(feedback.keyframe_request_pending());

    let complete_at = origin + Duration::from_millis(2_800);
    let next_idr = protect_complete_frame(&crypto, 902, 2);
    assert!(matches!(
        receiver.process_datagram(peer(), &next_idr, complete_at).as_slice(),
        [NvstReceiveEvent::FrameProgressResumed, NvstReceiveEvent::Frame(frame)]
            if frame.keyframe && !frame.contiguous
    ));
    feedback.publish_assembled_frame(2, complete_at);
    assert!(!feedback.keyframe_request_pending());
    assert_eq!(receiver.poll_frame_progress(complete_at, policy), None);
}

#[test]
fn partial_packets_that_never_complete_reach_keyframe_then_recovery() {
    let config = config();
    let crypto = test_srtp(&config);
    let feedback = config.feedback();
    let mut receiver = NvstVideoReceiver::new(config);
    let mut harness = DeliveryHarness::new(feedback.clone());
    let origin = receiver.timeout_origin;
    let step = Duration::from_millis(400);
    let policy = progress_policy();

    let mut events = Vec::new();
    for index in 0..25_u16 {
        let now = origin + step * u32::from(index);
        let protected = protect_partial_frame(&crypto, index, u32::from(index) / 3);
        harness.deliver(&mut receiver, peer(), &protected, now);
        if let Some(event) = receiver.poll_frame_progress(now, policy) {
            events.push((now.saturating_duration_since(origin), event));
        }
    }

    assert_eq!(feedback.frame_stage_timings().assembled_frames_total, 0);
    assert_eq!(
        events.len(),
        2,
        "escalation must be bounded to one keyframe stage and one recovery stage"
    );
    assert!(matches!(
        events[0].1,
        NvstFrameProgressEvent::KeyframeRequested { .. }
    ));
    assert_eq!(events[0].0, Duration::from_millis(2_000));
    assert!(matches!(
        events[1].1,
        NvstFrameProgressEvent::RecoveryNeeded { .. }
    ));
    assert_eq!(events[1].0, Duration::from_millis(3_200));
    assert!(feedback.keyframe_request_pending());
    assert_eq!(
        receiver.frame_progress().stage,
        NvstFrameProgressStage::RecoveryRequired
    );
}

#[test]
fn later_packet_arrivals_do_not_postpone_the_produced_frame_deadline() {
    let config = config();
    let crypto = test_srtp(&config);
    let feedback = config.feedback();
    let mut receiver = NvstVideoReceiver::new(config);
    let mut harness = DeliveryHarness::new(feedback);
    let origin = receiver.timeout_origin;
    let policy = progress_policy();

    let mut events = Vec::new();
    for index in 0..45_u16 {
        let now = origin + Duration::from_millis(100) * u32::from(index);
        let protected = protect_partial_frame(&crypto, index, u32::from(index) / 3);
        harness.deliver(&mut receiver, peer(), &protected, now);
        if let Some(event) = receiver.poll_frame_progress(now, policy) {
            events.push((now.saturating_duration_since(origin), event));
        }
    }

    assert_eq!(
        events,
        [
            (
                Duration::from_secs(2),
                NvstFrameProgressEvent::KeyframeRequested {
                    idle_for: Duration::from_secs(2),
                    last_assembled_frame_index: None,
                }
            ),
            (
                Duration::from_secs(3),
                NvstFrameProgressEvent::RecoveryNeeded {
                    idle_for: Duration::from_secs(3),
                    last_assembled_frame_index: None,
                }
            ),
        ],
        "the deadline is anchored at the first authenticated packet, so continuing packets \
         must not push it forward"
    );
}

#[test]
fn watchdog_stays_silent_before_the_first_authenticated_packet() {
    let config = config();
    let mut receiver = NvstVideoReceiver::new(config);
    let origin = receiver.timeout_origin;
    let policy = progress_policy();

    assert_eq!(
        receiver.poll_frame_progress(origin + Duration::from_secs(30), policy),
        None,
        "startup before any authenticated video belongs to the packet timeout"
    );
    assert_eq!(
        receiver.frame_progress().stage,
        NvstFrameProgressStage::Tracking
    );
}

#[test]
fn assembled_frames_inside_the_threshold_keep_the_watchdog_tracking() {
    let config = config();
    let crypto = test_srtp(&config);
    let feedback = config.feedback();
    let mut receiver = NvstVideoReceiver::new(config);
    let mut harness = DeliveryHarness::new(feedback.clone());
    let origin = receiver.timeout_origin;
    let step = Duration::from_millis(400);
    let policy = progress_policy();

    for index in 0..20_u16 {
        let now = origin + step * u32::from(index);
        let protected = protect_complete_frame(&crypto, 500 + index, u32::from(index));
        harness.deliver(&mut receiver, peer(), &protected, now);
        assert_eq!(receiver.poll_frame_progress(now, policy), None);
    }
    assert_eq!(
        feedback.frame_stage_timings().assembled_frames_total,
        20,
        "every healthy frame must be assembled"
    );
    assert_eq!(
        receiver.frame_progress().stage,
        NvstFrameProgressStage::Tracking
    );
    assert_eq!(receiver.frame_progress().last_assembled_frame_index, Some(19));
    assert_eq!(receiver.frame_progress().last_assembled_at, Some(origin + step * 19));
}

#[test]
fn keyframe_inside_the_grace_restores_progress_without_recovery() {
    let config = config();
    let crypto = test_srtp(&config);
    let feedback = config.feedback();
    let mut receiver = NvstVideoReceiver::new(config);
    let mut harness = DeliveryHarness::new(feedback);
    let origin = receiver.timeout_origin;
    let step = Duration::from_millis(400);
    let policy = progress_policy();

    let mut events = Vec::new();
    for index in 0..9_u16 {
        let now = origin + step * u32::from(index);
        let protected = if index == 7 {
            protect_complete_frame(&crypto, 9_000, 42)
        } else {
            protect_partial_frame(&crypto, index, u32::from(index) / 3)
        };
        harness.deliver(&mut receiver, peer(), &protected, now);
        if let Some(event) = receiver.poll_frame_progress(now, policy) {
            events.push(event);
        }
    }

    assert_eq!(
        events,
        [NvstFrameProgressEvent::KeyframeRequested {
            idle_for: Duration::from_millis(2_000),
            last_assembled_frame_index: None,
        }]
    );
    assert_eq!(
        receiver.frame_progress().stage,
        NvstFrameProgressStage::Tracking
    );
    assert_eq!(receiver.frame_progress().last_assembled_frame_index, Some(42));
    assert_eq!(
        receiver.frame_progress().last_assembled_at,
        Some(origin + step * 7)
    );
}

#[test]
fn packet_silence_stays_with_the_packet_timeout_and_not_the_progress_watchdog() {
    let config = config();
    let mut receiver = NvstVideoReceiver::new(config);
    let origin = receiver.timeout_origin;
    let policy = progress_policy();
    let now = origin + receiver.config.timeout;

    assert!(matches!(
        receiver.poll_timeout(now),
        Some(NvstReceiveEvent::RecoveryNeeded(NvstRecovery::Timeout { .. }))
    ));
    assert_eq!(receiver.poll_frame_progress(now, policy), None);
}

#[test]
fn pause_and_stop_never_report_a_progress_stall() {
    let config = config();
    let crypto = test_srtp(&config);
    let feedback = config.feedback();
    let mut receiver = NvstVideoReceiver::new(config);
    let mut harness = DeliveryHarness::new(feedback);
    let origin = receiver.timeout_origin;
    let policy = progress_policy();

    let protected = protect_partial_frame(&crypto, 0, 0);
    harness.deliver(&mut receiver, peer(), &protected, origin);

    receiver.pause();
    assert_eq!(
        receiver.poll_frame_progress(origin + Duration::from_secs(30), policy),
        None
    );

    receiver.resume();
    let resumed_origin = receiver.timeout_origin;
    assert_eq!(
        receiver.poll_frame_progress(resumed_origin + Duration::from_millis(1_900), policy),
        None,
        "resume must open a fresh stall window instead of inheriting the paused interval"
    );

    receiver.stop();
    assert_eq!(
        receiver.poll_frame_progress(origin + Duration::from_secs(60), policy),
        None
    );
}

#[test]
fn recovery_is_reported_once_per_episode_and_reset_by_recover() {
    let config = config();
    let crypto = test_srtp(&config);
    let feedback = config.feedback();
    let mut receiver = NvstVideoReceiver::new(config);
    let mut harness = DeliveryHarness::new(feedback.clone());
    let origin = receiver.timeout_origin;
    let policy = progress_policy();

    let mut index = 0_u16;
    let mut now = origin;
    while now < origin + Duration::from_secs(10) {
        now = origin + Duration::from_millis(400) * u32::from(index);
        let protected = protect_partial_frame(&crypto, index, u32::from(index) / 3);
        harness.deliver(&mut receiver, peer(), &protected, now);
        receiver.poll_frame_progress(now, policy);
        index += 1;
    }
    assert_eq!(
        receiver.frame_progress().stage,
        NvstFrameProgressStage::RecoveryRequired
    );
    assert_eq!(
        receiver.poll_frame_progress(now + Duration::from_secs(60), policy),
        None,
        "a terminal episode must not re-fire until the media state is reset"
    );

    assert!(receiver.recover().is_some());
    assert_eq!(
        receiver.frame_progress().stage,
        NvstFrameProgressStage::Tracking
    );
    let recovered_origin = receiver.timeout_origin;
    assert_eq!(receiver.poll_frame_progress(recovered_origin, policy), None);
    assert_eq!(
        receiver.poll_frame_progress(recovered_origin + Duration::from_secs(2), policy),
        None,
        "recovery clears the authenticated marker, so the packet timeout owns the next window"
    );
}

#[test]
fn packet_timeout_supersedes_a_pending_keyframe_grace_without_double_reporting() {
    let config = config();
    let crypto = test_srtp(&config);
    let feedback = config.feedback();
    let mut receiver = NvstVideoReceiver::new(config);
    let mut harness = DeliveryHarness::new(feedback);
    let origin = receiver.timeout_origin;
    let policy = progress_policy();

    for index in 0..7_u16 {
        let now = origin + Duration::from_millis(400) * u32::from(index);
        let protected = protect_partial_frame(&crypto, index, u32::from(index) / 3);
        harness.deliver(&mut receiver, peer(), &protected, now);
        receiver.poll_frame_progress(now, policy);
    }
    assert_eq!(
        receiver.frame_progress().stage,
        NvstFrameProgressStage::KeyframePending
    );

    let timeout_at = origin + Duration::from_millis(2_400) + receiver.config.timeout;
    assert!(matches!(
        receiver.poll_timeout(timeout_at),
        Some(NvstReceiveEvent::RecoveryNeeded(NvstRecovery::Timeout { .. }))
    ));
    assert_eq!(
        receiver.frame_progress().stage,
        NvstFrameProgressStage::Tracking,
        "the packet timeout must clear the pending progress episode"
    );
    assert_eq!(
        receiver.poll_frame_progress(timeout_at + Duration::from_secs(60), policy),
        None
    );
}

#[test]
fn udp_receive_loop_reports_frame_progress_recovery_while_partial_packets_continue() {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").expect("receiver socket");
    let receiver_port = socket.local_addr().expect("local addr").port();
    let peer_socket = std::net::UdpSocket::bind("127.0.0.1:0").expect("peer socket");
    let peer_port = peer_socket.local_addr().expect("peer addr").port();

    let handoff = json!({
        "clientUdpPort": receiver_port,
        "videoPeerIp": "127.0.0.1",
        "videoPeerPort": peer_port,
        "srtpAesKeyHex": "000102030405060708090A0B0C0D0E0F101112131415161718191A1B1C1D1E1F",
        "srtpSaltHex": "00000000000000009ECA935E",
        "codec": "H264",
        "rtpPayloadType": 96,
        "rtpSsrc": 0x11223344u32,
        "reorderWindowPackets": 4,
        "maxAccessUnitBytes": 4096,
        "timeoutMs": 1_000
    });
    let config = NvstVideoConfig::from_legacy_handoff(&handoff, None).expect("valid config");
    let crypto = test_srtp(&config);

    let (media_sender, _media_receiver) = std::sync::mpsc::sync_channel(64);
    let (event_sender, event_receiver) = std::sync::mpsc::channel();
    let session =
        spawn_nvst_udp_receiver_with_socket(
            config,
            media_sender,
            event_sender,
            Some(socket),
            None,
            Arc::new(HidRuntime::new()),
            None,
        )
            .expect("receiver spawns");

    let destination = format!("127.0.0.1:{receiver_port}");
    let feeder = std::thread::spawn(move || {
        for index in 0..50_u16 {
            let mut flags = FLAG_CONTAINS_PIC_DATA;
            if index == 0 {
                flags |= FLAG_SOF;
            }
            let mut payload = vec![0x55_u8; 64];
            if index == 0 {
                payload[..5].copy_from_slice(&[0, 0, 0, 1, 0x61]);
            }
            let packet = protect_for_test(
                &crypto,
                build_plaintext_rtp(1_000 + index, flags, 7, &payload),
                0,
            );
            if peer_socket.send_to(&packet, &destination).is_err() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });

    let deadline = Instant::now() + Duration::from_secs(20);
    let mut recovery = None;
    while Instant::now() < deadline {
        if let Ok(NvstReceiveEvent::RecoveryNeeded(NvstRecovery::FrameProgress {
            idle_for,
            last_assembled_frame_index,
        })) = event_receiver.recv_timeout(Duration::from_millis(500))
        {
            recovery = Some((idle_for, last_assembled_frame_index));
            break;
        }
    }

    session.stop();
    let _ = feeder.join();
    let (idle_for, last_assembled_frame_index) =
        recovery.expect("the receive loop must report a produced-frame stall");
    assert!(
        idle_for >= Duration::from_millis(900),
        "idle_for={idle_for:?}"
    );
    assert_eq!(last_assembled_frame_index, None);
}

#[test]
fn resumed_progress_is_reported_once_when_an_episode_closes_on_assembly() {
    let config = config();
    let crypto = test_srtp(&config);
    let feedback = config.feedback();
    let mut receiver = NvstVideoReceiver::new(config);
    let mut harness = DeliveryHarness::new(feedback.clone());
    let origin = receiver.timeout_origin;
    let policy = progress_policy();
    let mut sequence = 6_000_u16;

    for index in 0..6_u16 {
        let now = origin + Duration::from_millis(400) * u32::from(index);
        let mut flags = FLAG_CONTAINS_PIC_DATA;
        if index == 0 {
            flags |= FLAG_SOF;
        }
        let mut payload = vec![0x55_u8; 64];
        if index == 0 {
            payload[..5].copy_from_slice(&[0, 0, 0, 1, 0x61]);
        }
        let protected = protect_for_test(
            &crypto,
            build_plaintext_rtp(sequence, flags, 1, &payload),
            0,
        );
        sequence += 1;
        harness.deliver(&mut receiver, peer(), &protected, now);
        receiver.poll_frame_progress(now, policy);
    }
    assert_eq!(harness.resumed, 0, "no episode may close before one opens");
    assert_eq!(
        receiver.frame_progress().stage,
        NvstFrameProgressStage::KeyframePending,
        "the partial stream must open an episode inside its grace window"
    );

    let mut closed_at = None;
    for index in 6..20_u16 {
        let now = origin + Duration::from_millis(400) * u32::from(index);
        let protected = protect_for_test(
            &crypto,
            build_plaintext_rtp(
                sequence,
                FLAG_SOF | FLAG_EOF | FLAG_CONTAINS_PIC_DATA,
                1,
                &[0, 0, 0, 1, 0x65, 0x11],
            ),
            0,
        );
        sequence += 1;
        let before = harness.resumed;
        harness.deliver(&mut receiver, peer(), &protected, now);
        receiver.poll_frame_progress(now, policy);
        if harness.resumed > before {
            assert!(closed_at.is_none(), "only one episode may close in this run");
            closed_at = Some(index);
        }
    }

    assert_eq!(
        harness.resumed, 1,
        "exactly one resumed transition for one closed episode"
    );
    assert_eq!(closed_at, Some(6));
    assert_eq!(
        feedback.frame_stage_timings().assembled_frames_total,
        14,
        "every complete frame in the second phase must be assembled"
    );
    assert_eq!(
        receiver.frame_progress().stage,
        NvstFrameProgressStage::Tracking
    );
}
