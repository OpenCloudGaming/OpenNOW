use super::*;

fn word(report: &QosReport, offset: usize) -> u32 {
    u32::from_le_bytes(
        report.command().payload[offset..offset + 4]
            .try_into()
            .unwrap(),
    )
}

fn halfword(report: &QosReport, offset: usize) -> u16 {
    u16::from_le_bytes(
        report.command().payload[offset..offset + 2]
            .try_into()
            .unwrap(),
    )
}

#[test]
fn qos_reports_measure_successive_intervals_at_50_and_75_mbps() {
    for bitrate_mbps in [50_u32, 75] {
        let mut config = config();
        config.max_access_unit_bytes = DEFAULT_MAX_ACCESS_UNIT_BYTES;
        let feedback = config.feedback();
        let crypto = test_srtp(&config);
        let mut receiver = NvstVideoReceiver::new(config);
        let mut previous = QosReport::default();
        let mut sequence = 10_u16;
        let interval_bytes = bitrate_mbps * 1_000_000 / 8 / 20;
        let mut media = vec![0x55; interval_bytes as usize];
        media[..5].copy_from_slice(&[0, 0, 0, 1, 0x65]);
        let origin = Instant::now();

        for frame_index in 1..=4 {
            let sender_frame = frame_index + 100;
            let timestamp = 90_000 + frame_index * 5_000;
            let now = origin + QOS_REPORT_INTERVAL * frame_index;
            let mut emitted = 0;
            let chunk_count = media.len().div_ceil(1_200);
            for (index, chunk) in media.chunks(1_200).enumerate() {
                let mut flags = FLAG_CONTAINS_PIC_DATA;
                if index == 0 {
                    flags |= FLAG_SOF;
                }
                if index + 1 == chunk_count {
                    flags |= FLAG_EOF;
                }
                let mut packet = build_plaintext_rtp(sequence, flags, sender_frame, chunk);
                packet[4..8].copy_from_slice(&timestamp.to_be_bytes());
                let encrypted = protect_for_test(&crypto, packet, 0);
                for event in receiver.process_datagram(peer(), &encrypted, now) {
                    let NvstReceiveEvent::Frame(frame) = event else {
                        panic!("unexpected receive event: {event:?}");
                    };
                    assert_eq!(frame.bytes, media);
                    emitted += 1;
                }
                sequence += 1;
            }
            assert_eq!(emitted, 1);
            let report = feedback.qos_report(&previous, QOS_REPORT_INTERVAL * frame_index);
            assert_eq!(report.sequence, frame_index);
            assert_eq!(report.sender_frame_number, sender_frame);
            assert_eq!(word(&report, 12), sender_frame);
            assert_eq!(report.bytes_received, frame_index * interval_bytes);
            assert_eq!(previous.bytes_received, (frame_index - 1) * interval_bytes);
            assert_eq!(word(&report, 16), 0);
            assert_eq!(word(&report, 48), 0);
            assert_eq!(word(&report, 44), 0);
            assert_eq!(word(&report, 36), frame_index * 4_500);
            assert_eq!(report.command().encoded().len(), 56);
            previous = report;
        }

        let idle = feedback.qos_report(&previous, QOS_REPORT_INTERVAL * 5);
        assert_eq!(idle.bytes_received, previous.bytes_received);
        assert_eq!(word(&idle, 16), 0);
        assert_eq!(word(&idle, 44), 0);
        assert_eq!(idle.sender_frame_number, previous.sender_frame_number);
    }
}

#[test]
fn qos_receive_progress_advances_before_frame_completion_but_not_on_invalid_packets() {
    let config = config();
    let feedback = config.feedback();
    let crypto = test_srtp(&config);
    let mut receiver = NvstVideoReceiver::new(config);
    let mut first = build_plaintext_rtp(10, FLAG_SOF, 1, &[0, 0, 0, 1, 0x65]);
    first[4..8].copy_from_slice(&90_000_u32.to_be_bytes());
    let first = protect_for_test(&crypto, first, 0);
    let now = Instant::now();
    assert!(receiver.process_datagram(peer(), &first, now).is_empty());
    let first_report = feedback.qos_report(&QosReport::default(), Duration::from_millis(50));
    assert_eq!(first_report.sender_frame_number, 0);
    assert_eq!(word(&first_report, 36), 4_500);
    assert_eq!(
        feedback
            .reception_timing
            .lock()
            .unwrap()
            .latest_rtp_timestamp,
        Some(90_000)
    );
    assert_eq!(first_report.bytes_received, 0);
    assert_eq!(halfword(&first_report, 26), 0);

    let mut later = build_plaintext_rtp(12, FLAG_SOF, 2, &[0, 0, 0, 1, 0x61]);
    later[4..8].copy_from_slice(&93_000_u32.to_be_bytes());
    let later = protect_for_test(&crypto, later, 0);
    let mut corrupted = later.clone();
    corrupted[4] ^= 1;
    assert!(matches!(
        receiver
            .process_datagram(peer(), &corrupted, now)
            .as_slice(),
        [NvstReceiveEvent::Dropped(
            NvstDropReason::AuthenticationFailed
        )]
    ));
    assert_eq!(
        feedback
            .reception_timing
            .lock()
            .unwrap()
            .latest_rtp_timestamp,
        Some(90_000)
    );
    let wrong_peer = SocketAddr::new(peer().ip(), peer().port() + 1);
    assert!(matches!(
        receiver
            .process_datagram(wrong_peer, &later, now)
            .as_slice(),
        [NvstReceiveEvent::Dropped(
            NvstDropReason::UnexpectedSource { .. }
        )]
    ));
    assert_eq!(
        feedback
            .reception_timing
            .lock()
            .unwrap()
            .latest_rtp_timestamp,
        Some(90_000)
    );
    assert!(receiver.process_datagram(peer(), &later, now).is_empty());
    let later_report = feedback.qos_report(&first_report, Duration::from_millis(100));
    assert_eq!(
        feedback
            .reception_timing
            .lock()
            .unwrap()
            .latest_rtp_timestamp,
        Some(93_000)
    );
    assert_eq!(later_report.sender_frame_number, 0);
    assert_eq!(word(&later_report, 36), 9_000);
    assert_eq!(halfword(&later_report, 26), 5_000);
    assert_eq!(word(&later_report, 44), 0);
    assert!(matches!(
        receiver.process_datagram(peer(), &first, now).as_slice(),
        [NvstReceiveEvent::Dropped(NvstDropReason::ReplayRejected)]
    ));
    assert_eq!(
        feedback
            .reception_timing
            .lock()
            .unwrap()
            .latest_rtp_timestamp,
        Some(93_000)
    );
}

#[test]
fn qos_receive_timestamp_handles_reordering_and_sender_clock_wrap() {
    let feedback = NvstFeedbackState::default();
    let previous = QosReport::default();
    let now = Instant::now();
    feedback.publish_stream(7, 10, u32::MAX - 2_000, now);
    assert_eq!(
        feedback
            .reception_timing
            .lock()
            .unwrap()
            .latest_rtp_timestamp,
        Some(u32::MAX - 2_000)
    );
    feedback.publish_stream(7, 12, 1_000, now + Duration::from_millis(33));
    assert_eq!(
        feedback
            .reception_timing
            .lock()
            .unwrap()
            .latest_rtp_timestamp,
        Some(1_000)
    );
    feedback.publish_stream(7, 12, u32::MAX - 500, now + Duration::from_millis(34));
    assert_eq!(
        feedback
            .reception_timing
            .lock()
            .unwrap()
            .latest_rtp_timestamp,
        Some(1_000)
    );
    feedback.publish_stream(7, 13, 4_000, now + Duration::from_millis(66));
    assert_eq!(
        feedback
            .reception_timing
            .lock()
            .unwrap()
            .latest_rtp_timestamp,
        Some(4_000)
    );
    assert_eq!(
        word(
            &feedback.qos_report(&previous, Duration::from_millis(125)),
            36
        ),
        11_250
    );
}

#[test]
fn qos_loss_tracks_only_authenticated_packets_since_the_last_successful_report() {
    let config = config();
    let feedback = config.feedback();
    let crypto = test_srtp(&config);
    let mut receiver = NvstVideoReceiver::new(config);
    let now = Instant::now();
    let packet = |sequence| {
        protect_for_test(
            &crypto,
            build_plaintext_rtp(sequence, FLAG_SOF, u32::from(sequence), &[0, 0, 0, 1, 0x65]),
            0,
        )
    };

    assert!(
        receiver
            .process_datagram(peer(), &packet(10), now)
            .is_empty()
    );
    let sent = feedback.qos_report(&QosReport::default(), Duration::from_millis(50));
    assert_eq!(halfword(&sent, 26), 0);

    let mut rejected = packet(11);
    rejected[4] ^= 1;
    assert!(matches!(
        receiver.process_datagram(peer(), &rejected, now).as_slice(),
        [NvstReceiveEvent::Dropped(
            NvstDropReason::AuthenticationFailed
        )]
    ));
    assert!(
        receiver
            .process_datagram(peer(), &packet(12), now)
            .is_empty()
    );
    let unsent = feedback.qos_report(&sent, Duration::from_millis(100));
    assert_eq!(halfword(&unsent, 26), 5_000);

    assert!(
        receiver
            .process_datagram(peer(), &packet(14), now)
            .is_empty()
    );
    let retried = feedback.qos_report(&sent, Duration::from_millis(150));
    assert_eq!(retried.sequence, unsent.sequence);
    assert_eq!(halfword(&retried, 26), 5_000);

    let _ = receiver.process_datagram(peer(), &packet(11), now);
    assert_eq!(feedback.received_packets.load(Ordering::Acquire), 4);
    let recovered = feedback.qos_report(&retried, Duration::from_millis(200));
    assert_eq!(halfword(&recovered, 26), 0);
    assert_eq!(word(&recovered, 36), 18_000);
}

#[test]
fn qos_loss_resets_on_stream_and_counter_epoch_changes() {
    let feedback = NvstFeedbackState::default();
    let now = Instant::now();
    feedback.publish_stream(7, 10, 90_000, now);
    let baseline = feedback.qos_report(&QosReport::default(), Duration::ZERO);
    feedback.publish_stream(7, 12, 93_000, now);
    let loss = feedback.qos_report(&baseline, Duration::from_millis(50));
    assert_eq!(halfword(&loss, 26), 5_000);

    feedback.publish_stream(8, 13, 94_000, now);
    let new_stream = feedback.qos_report(&loss, Duration::from_millis(100));
    assert_eq!(halfword(&new_stream, 26), 0);
    feedback.publish_stream(8, 14, 95_000, now);
    let healthy = feedback.qos_report(&new_stream, Duration::from_millis(150));
    assert_eq!(halfword(&healthy, 26), 0);

    feedback
        .reception_timing
        .lock()
        .unwrap()
        .qos_packet_snapshot
        .as_mut()
        .unwrap()
        .received = 0;
    let reset = feedback.qos_report(&healthy, Duration::from_millis(200));
    assert_eq!(halfword(&reset, 26), 0);
}

#[test]
fn qos_client_clock_truncates_to_milliseconds_and_wraps_at_u32_ticks() {
    let feedback = NvstFeedbackState::default();
    let baseline = QosReport::default();
    assert_eq!(
        word(
            &feedback.qos_report(&baseline, Duration::from_micros(1_999)),
            36
        ),
        90
    );
    let elapsed = Duration::from_millis(u64::from(u32::MAX) / 90 + 1);
    assert_eq!(
        word(&feedback.qos_report(&baseline, elapsed), 36),
        14
    );
}

#[test]
fn qos_unsent_samples_do_not_advance_the_successful_report_baseline() {
    let feedback = NvstFeedbackState::default();
    feedback
        .completed_frame_bytes
        .store(1_000, Ordering::Release);
    let sent = feedback.qos_report(&QosReport::default(), Duration::ZERO);
    feedback
        .completed_frame_bytes
        .store(2_000, Ordering::Release);
    let unsent = feedback.qos_report(&sent, Duration::ZERO);
    assert_eq!(sent.bytes_received, 1_000);
    assert_eq!(unsent.bytes_received, 2_000);
    assert_eq!(word(&unsent, 44), 0);
    feedback
        .completed_frame_bytes
        .store(3_000, Ordering::Release);
    let retry = feedback.qos_report(&sent, Duration::ZERO);
    assert_eq!(retry.sequence, unsent.sequence);
    assert_eq!(sent.bytes_received, 1_000);
    assert_eq!(retry.bytes_received, 3_000);
    assert_eq!(word(&retry, 48), 0);
    assert_eq!(word(&retry, 44), 0);
    let next = feedback.qos_report(&retry, Duration::ZERO);
    assert_eq!(next.sequence, retry.sequence + 1);
    assert_eq!(word(&next, 44), 0);
}

#[test]
fn qos_counters_wrap_without_zeroing_or_overflowing_interval_bits() {
    let feedback = NvstFeedbackState::default();
    feedback
        .completed_frame_bytes
        .store(u64::from(u32::MAX) + 101, Ordering::Release);
    let previous = QosReport {
        sequence: u32::MAX,
        bytes_received: u32::MAX - 99,
        ..QosReport::default()
    };
    let report = feedback.qos_report(&previous, Duration::ZERO);
    assert_eq!(report.sequence, 0);
    assert_eq!(report.bytes_received, 100);
    assert_eq!(previous.bytes_received, u32::MAX - 99);
    assert_eq!(word(&report, 16), 0);
    assert_eq!(word(&report, 48), 0);
    assert_eq!(word(&report, 44), 0);

    feedback
        .completed_frame_bytes
        .store(u64::from(u32::MAX), Ordering::Release);
    let saturated = feedback.qos_report(&QosReport::default(), Duration::ZERO);
    assert_eq!(saturated.bytes_received, u32::MAX);
    assert_eq!(word(&saturated, 44), 0);
}

#[test]
fn qos_warmup_and_new_sessions_keep_independent_baselines() {
    let feedback = NvstFeedbackState::default();
    feedback
        .completed_frame_bytes
        .store(500_000, Ordering::Release);
    let warmup = feedback.qos_report(&QosReport::default(), Duration::ZERO);
    assert_eq!(word(&warmup, 44), 0);
    feedback
        .completed_frame_bytes
        .store(1_000_000, Ordering::Release);
    let warmed_up = feedback.qos_report(&warmup, Duration::ZERO);
    assert_eq!(warmup.bytes_received, 500_000);
    assert_eq!(word(&warmed_up, 48), 0);
    assert_eq!(word(&warmed_up, 44), 0);

    let next_session = NvstFeedbackState::default();
    let report = next_session.qos_report(&QosReport::default(), Duration::ZERO);
    assert_eq!(report.sequence, 1);
    assert_eq!(report.sender_frame_number, 0);
    assert_eq!(word(&report, 16), 0);
    assert_eq!(word(&report, 48), 0);
    assert_eq!(word(&report, 44), 0);
    assert_eq!(word(&report, 36), 0);
}
