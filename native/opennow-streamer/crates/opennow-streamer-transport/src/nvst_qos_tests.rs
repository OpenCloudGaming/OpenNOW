use super::*;

fn word(report: &QosReport, offset: usize) -> u32 {
    u32::from_le_bytes(
        report.command().payload[offset..offset + 4]
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
        let interval_bytes = bitrate_mbps * 1_000_000 / 8 / 18;
        let mut media = vec![0x55; interval_bytes as usize];
        media[..5].copy_from_slice(&[0, 0, 0, 1, 0x65]);
        let origin = Instant::now();

        for frame_index in 1..=4 {
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
                let mut packet = build_plaintext_rtp(sequence, flags, frame_index, chunk);
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
            let report = feedback.qos_report(&previous, true);
            assert_eq!(report.sequence, frame_index);
            assert_eq!(report.frames_received, frame_index);
            assert_eq!(word(&report, 16), frame_index * interval_bytes);
            assert_eq!(word(&report, 48), (frame_index - 1) * interval_bytes);
            assert_eq!(word(&report, 44), interval_bytes * 8);
            assert_eq!(word(&report, 36), timestamp);
            assert_eq!(report.command().encoded().len(), 56);
            previous = report;
        }

        let idle = feedback.qos_report(&previous, true);
        assert_eq!(word(&idle, 16), word(&idle, 48));
        assert_eq!(word(&idle, 44), 0);
        assert_eq!(idle.rtp_timestamp, previous.rtp_timestamp);
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
    let first_report = feedback.qos_report(&QosReport::default(), true);
    assert_eq!(first_report.rtp_timestamp, 90_000);
    assert_eq!(first_report.frames_received, 0);
    assert_eq!(first_report.bytes_received, 0);

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
        feedback.qos_report(&first_report, true).rtp_timestamp,
        90_000
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
        feedback.qos_report(&first_report, true).rtp_timestamp,
        90_000
    );
    assert!(receiver.process_datagram(peer(), &later, now).is_empty());
    let later_report = feedback.qos_report(&first_report, true);
    assert_eq!(later_report.rtp_timestamp, 93_000);
    assert_eq!(later_report.frames_received, 0);
    assert_eq!(word(&later_report, 44), 0);
    assert!(matches!(
        receiver.process_datagram(peer(), &first, now).as_slice(),
        [NvstReceiveEvent::Dropped(NvstDropReason::ReplayRejected)]
    ));
    assert_eq!(
        feedback.qos_report(&later_report, true).rtp_timestamp,
        93_000
    );
}

#[test]
fn qos_receive_timestamp_handles_reordering_and_sender_clock_wrap() {
    let feedback = NvstFeedbackState::default();
    let previous = QosReport::default();
    let now = Instant::now();
    feedback.publish_stream(7, 10, u32::MAX - 2_000, now);
    assert_eq!(
        feedback.qos_report(&previous, true).rtp_timestamp,
        u32::MAX - 2_000
    );
    feedback.publish_stream(7, 12, 1_000, now + Duration::from_millis(33));
    assert_eq!(feedback.qos_report(&previous, true).rtp_timestamp, 1_000);
    feedback.publish_stream(7, 12, u32::MAX - 500, now + Duration::from_millis(34));
    assert_eq!(feedback.qos_report(&previous, true).rtp_timestamp, 1_000);
    feedback.publish_stream(7, 13, 4_000, now + Duration::from_millis(66));
    assert_eq!(feedback.qos_report(&previous, true).rtp_timestamp, 4_000);
}

#[test]
fn qos_unsent_samples_do_not_advance_the_successful_report_baseline() {
    let feedback = NvstFeedbackState::default();
    feedback
        .completed_frame_bytes
        .store(1_000, Ordering::Release);
    let sent = feedback.qos_report(&QosReport::default(), true);
    feedback
        .completed_frame_bytes
        .store(2_000, Ordering::Release);
    let unsent = feedback.qos_report(&sent, true);
    assert_eq!(word(&unsent, 44), 8_000);
    feedback
        .completed_frame_bytes
        .store(3_000, Ordering::Release);
    let retry = feedback.qos_report(&sent, true);
    assert_eq!(retry.sequence, unsent.sequence);
    assert_eq!(word(&retry, 48), 1_000);
    assert_eq!(word(&retry, 44), 16_000);
    let next = feedback.qos_report(&retry, true);
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
    let report = feedback.qos_report(&previous, true);
    assert_eq!(report.sequence, 0);
    assert_eq!(word(&report, 16), 100);
    assert_eq!(word(&report, 48), u32::MAX - 99);
    assert_eq!(word(&report, 44), 1_600);

    feedback
        .completed_frame_bytes
        .store(u64::from(u32::MAX), Ordering::Release);
    let saturated = feedback.qos_report(&QosReport::default(), true);
    assert_eq!(word(&saturated, 44), u32::MAX);
}

#[test]
fn qos_warmup_and_new_sessions_keep_independent_baselines() {
    let feedback = NvstFeedbackState::default();
    feedback
        .completed_frame_bytes
        .store(500_000, Ordering::Release);
    let warmup = feedback.qos_report(&QosReport::default(), false);
    assert_eq!(word(&warmup, 44), 0);
    feedback
        .completed_frame_bytes
        .store(1_000_000, Ordering::Release);
    let warmed_up = feedback.qos_report(&warmup, true);
    assert_eq!(word(&warmed_up, 48), 500_000);
    assert_eq!(word(&warmed_up, 44), 4_000_000);

    let next_session = NvstFeedbackState::default();
    let report = next_session.qos_report(&QosReport::default(), false);
    assert_eq!(report.sequence, 1);
    assert_eq!(report.frames_received, 0);
    assert_eq!(word(&report, 16), 0);
    assert_eq!(word(&report, 48), 0);
    assert_eq!(word(&report, 44), 0);
    assert_eq!(word(&report, 36), 0);
}
