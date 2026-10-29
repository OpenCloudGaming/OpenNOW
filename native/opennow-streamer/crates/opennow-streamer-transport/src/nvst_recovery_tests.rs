use super::*;

#[test]
fn multiple_reorder_gaps_do_not_revalidate_post_gap_references() {
    for fec in [false, true] {
        let mut config = config();
        config.reorder_window_packets = DEFAULT_REORDER_WINDOW;
        let crypto = test_srtp(&config);
        let mut receiver = NvstVideoReceiver::new(config);
        let now = Instant::now();
        let sequences = if fec {
            [0, 4, 2055, 2057]
        } else {
            [0, 2, 1028, 1029]
        };
        let mut result = Vec::new();
        for (index, (sequence, frame, keyframe)) in sequences
            .into_iter()
            .zip([0, 2, 100, 101])
            .zip([true, true, false, false])
            .map(|((sequence, frame), keyframe)| (sequence, frame, keyframe))
            .enumerate()
        {
            let plaintext = if fec {
                reference_fec_block(sequence, frame, 1, 100, keyframe).remove(0)
            } else {
                build_plaintext_rtp(
                    sequence,
                    FLAG_SOF | FLAG_EOF | FLAG_CONTAINS_PIC_DATA,
                    frame,
                    &[0, 0, 0, 1, if keyframe { 0x65 } else { 0x61 }, 0x88],
                )
            };
            let at = if index == 3 {
                now + MJOLNIR_REORDER_DEQUEUE_TIMEOUT + Duration::from_millis(1)
            } else {
                now
            };
            let events =
                receiver.process_datagram(peer(), &protect_for_test(&crypto, plaintext, 0), at);
            if index == 3 {
                result = events;
            }
        }
        assert!(matches!(
            result.first(),
            Some(NvstReceiveEvent::RecoveryNeeded(
                NvstRecovery::PacketGap { .. }
            ))
        ));
        let frames: Vec<_> = result
            .into_iter()
            .filter_map(|event| match event {
                NvstReceiveEvent::Frame(frame) => {
                    Some((frame.frame_index, frame.keyframe, frame.contiguous))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            frames,
            [(100, false, false), (101, false, true)],
            "fec={fec}"
        );
        assert!(receiver.config.feedback.keyframe_request_pending());
    }
}

fn reference_fec_block(
    base: u16,
    frame: u32,
    count: usize,
    percent: u32,
    keyframe: bool,
) -> Vec<Vec<u8>> {
    const NANORS_INVERSE: [u8; 256] = [
        0, 1, 142, 244, 71, 167, 122, 186, 173, 157, 221, 152, 61, 170, 93, 150, 216, 114, 192, 88,
        224, 62, 76, 102, 144, 222, 85, 128, 160, 131, 75, 42, 108, 237, 57, 81, 96, 86, 44, 138,
        112, 208, 31, 74, 38, 139, 51, 110, 72, 137, 111, 46, 164, 195, 64, 94, 80, 34, 207, 169,
        171, 12, 21, 225, 54, 95, 248, 213, 146, 78, 166, 4, 48, 136, 43, 30, 22, 103, 69, 147, 56,
        35, 104, 140, 129, 26, 37, 97, 19, 193, 203, 99, 151, 14, 55, 65, 36, 87, 202, 91, 185,
        196, 23, 77, 82, 141, 239, 179, 32, 236, 47, 50, 40, 209, 17, 217, 233, 251, 218, 121, 219,
        119, 6, 187, 132, 205, 254, 252, 27, 84, 161, 29, 124, 204, 228, 176, 73, 49, 39, 45, 83,
        105, 2, 245, 24, 223, 68, 79, 155, 188, 15, 92, 11, 220, 189, 148, 172, 9, 199, 162, 28,
        130, 159, 198, 52, 194, 70, 5, 206, 59, 13, 60, 156, 8, 190, 183, 135, 229, 238, 107, 235,
        242, 191, 175, 197, 100, 7, 123, 149, 154, 174, 182, 18, 89, 165, 53, 101, 184, 163, 158,
        210, 247, 98, 90, 133, 125, 168, 58, 41, 113, 200, 246, 249, 67, 215, 214, 16, 115, 118,
        120, 153, 10, 25, 145, 20, 63, 230, 240, 134, 177, 226, 241, 250, 116, 243, 180, 109, 33,
        178, 106, 227, 231, 181, 234, 3, 143, 211, 201, 66, 212, 232, 117, 127, 255, 126, 253,
    ];
    let parity_count = (count * percent as usize).div_ceil(100);
    let mut packets = Vec::new();
    for index in 0..count {
        let mut payload = vec![0x55; 1264];
        let mut flags = FLAG_CONTAINS_PIC_DATA;
        if index == 0 {
            flags |= FLAG_SOF;
            payload[..5].copy_from_slice(&[0, 0, 0, 1, if keyframe { 0x65 } else { 0x61 }]);
        }
        if index == count - 1 {
            flags |= FLAG_EOF;
        }
        let mut packet = build_plaintext_rtp(base + index as u16, flags, frame, &payload);
        let fec_word = (percent << 4) | ((index as u32) << 12) | ((count as u32) << 22);
        packet[28..32].copy_from_slice(&fec_word.to_le_bytes());
        packets.push(packet);
    }
    for parity_index in 0..parity_count {
        let mut parity = vec![0; 1296];
        for (data_index, data) in packets[..count].iter().enumerate() {
            let coefficient = NANORS_INVERSE[(parity_count + data_index) ^ parity_index];
            for (target, source) in parity.iter_mut().zip(data) {
                let mut value = *source;
                let mut multiplier = coefficient;
                while multiplier != 0 {
                    if multiplier & 1 != 0 {
                        *target ^= value;
                    }
                    let high_bit = value & 0x80 != 0;
                    value <<= 1;
                    if high_bit {
                        value ^= 0x1d;
                    }
                    multiplier >>= 1;
                }
            }
        }
        parity[..16].copy_from_slice(&packets[0][..16]);
        parity[2..4].copy_from_slice(&(base + (count + parity_index) as u16).to_be_bytes());
        parity[20..24].copy_from_slice(&frame.to_le_bytes());
        parity[27] = 0;
        let fec_word =
            (percent << 4) | (((count + parity_index) as u32) << 12) | ((count as u32) << 22);
        parity[28..32].copy_from_slice(&fec_word.to_le_bytes());
        packets.push(parity);
    }
    packets
}

#[test]
fn recovered_sof_is_delivered_as_data_before_its_late_original() {
    for (count, percent) in [(1, 200), (2, 100), (3, 66), (100, 20)] {
        let mut config = config();
        config.max_access_unit_bytes = DEFAULT_MAX_ACCESS_UNIT_BYTES;
        let feedback = config.feedback();
        let crypto = test_srtp(&config);
        let mut receiver = NvstVideoReceiver::new(config);
        let mut frames = Vec::new();
        let initial = reference_fec_block(10, 5, 1, 200, true);
        let reference = reference_fec_block(13, 6, count, percent, false);
        let expected_reference = reference[..count]
            .iter()
            .flat_map(|packet| packet[32..].iter().copied())
            .collect::<Vec<_>>();
        let successor = reference_fec_block(13 + reference.len() as u16, 7, 1, 200, false);
        let reordered_reference = reference[1..=count]
            .iter()
            .chain(std::iter::once(&reference[0]))
            .chain(reference[count + 1..].iter());
        for packet in initial.iter().chain(reordered_reference).chain(&successor) {
            let encrypted = protect_for_test(&crypto, packet.clone(), 0);
            for event in receiver.process_datagram(peer(), &encrypted, Instant::now()) {
                let NvstReceiveEvent::Frame(frame) = event else {
                    panic!("unexpected {count}-data FEC event: {event:?}");
                };
                assert!(frame.contiguous);
                frames.push(frame);
            }
        }
        assert_eq!(
            frames
                .iter()
                .map(|frame| frame.frame_index)
                .collect::<Vec<_>>(),
            [5, 6, 7]
        );
        assert_eq!(frames[1].bytes, expected_reference);
        assert_eq!(receiver.fec_repaired_packets, 1);
        assert!(!feedback.keyframe_request_pending());
        assert_eq!(feedback.network_metrics().unwrap().1, 0.0);
    }
}

#[test]
fn rejected_picture_requires_a_new_keyframe_and_marks_the_next_frame_discontinuous() {
    let mut oversized = vec![0x55; 4097];
    oversized[..5].copy_from_slice(&[0, 0, 0, 1, 0x61]);
    for (flags, payload, reason) in [
        (
            FLAG_SOF | FLAG_EOF,
            oversized,
            NvstDropReason::AccessUnitTooLarge { limit: 4096 },
        ),
        (
            FLAG_SOF | FLAG_EOF,
            vec![0x55; 20],
            NvstDropReason::MissingAnnexBStartCode,
        ),
        (
            FLAG_EOF,
            vec![0x55; 20],
            NvstDropReason::AwaitingStartOfFrame,
        ),
    ] {
        let config = config();
        let feedback = config.feedback();
        let crypto = test_srtp(&config);
        let mut receiver = NvstVideoReceiver::new(config);
        let initial = protect_for_test(
            &crypto,
            build_plaintext_rtp(10, FLAG_SOF | FLAG_EOF, 5, &[0, 0, 0, 1, 0x65]),
            0,
        );
        assert!(
            matches!(receiver.process_datagram(peer(), &initial, Instant::now()).as_slice(), [NvstReceiveEvent::Frame(frame)] if frame.keyframe)
        );
        let rejected = protect_for_test(&crypto, build_plaintext_rtp(11, flags, 6, &payload), 0);
        assert_eq!(
            receiver.process_datagram(peer(), &rejected, Instant::now()),
            [NvstReceiveEvent::Dropped(reason)]
        );
        assert!(receiver.assembler.current_frame.is_none());
        assert!(feedback.keyframe_request_pending());
        let successor = protect_for_test(
            &crypto,
            build_plaintext_rtp(12, FLAG_SOF | FLAG_EOF, 7, &[0, 0, 0, 1, 0x61]),
            0,
        );
        assert!(
            matches!(receiver.process_datagram(peer(), &successor, Instant::now()).as_slice(), [NvstReceiveEvent::Frame(frame)] if !frame.contiguous && !frame.keyframe)
        );
        assert!(feedback.keyframe_request_pending());
        assert_eq!(feedback.network_metrics().unwrap().1, 0.0);
        let recovery = protect_for_test(
            &crypto,
            build_plaintext_rtp(13, FLAG_SOF | FLAG_EOF, 8, &[0, 0, 0, 1, 0x65]),
            0,
        );
        assert!(
            matches!(receiver.process_datagram(peer(), &recovery, Instant::now()).as_slice(), [NvstReceiveEvent::Frame(frame)] if frame.keyframe)
        );
        assert!(!feedback.keyframe_request_pending());
    }
}

#[test]
fn authenticated_unsupported_parity_and_replays_do_not_invalidate_picture_data() {
    let config = config();
    let feedback = config.feedback();
    let crypto = test_srtp(&config);
    let mut receiver = NvstVideoReceiver::new(config);
    let first = protect_for_test(
        &crypto,
        build_plaintext_rtp(10, FLAG_SOF, 5, &[0, 0, 0, 1, 0x65]),
        0,
    );
    assert!(
        receiver
            .process_datagram(peer(), &first, Instant::now())
            .is_empty()
    );
    assert_eq!(
        receiver.process_datagram(peer(), &first, Instant::now()),
        [NvstReceiveEvent::Dropped(NvstDropReason::ReplayRejected)]
    );
    let mut parity = build_plaintext_rtp(11, FLAG_CONTAINS_PIC_DATA, 5, &[0x55]);
    parity[28..32].copy_from_slice(&((100_u32 << 4) | (2_u32 << 12) | (1_u32 << 22)).to_le_bytes());
    let parity = protect_for_test(&crypto, parity, 0);
    assert_eq!(
        receiver.process_datagram(peer(), &parity, Instant::now()),
        [NvstReceiveEvent::Dropped(NvstDropReason::Unsupported(
            NvstUnsupportedFeature::FecRepair
        ))]
    );
    assert_eq!(receiver.assembler.current_frame, Some(5));
    assert!(!feedback.keyframe_request_pending());
}

#[test]
fn conflicting_fec_layout_invalidates_only_discarded_picture_data() {
    for parity in [false, true] {
        let config = config();
        let feedback = config.feedback();
        let crypto = test_srtp(&config);
        let mut receiver = NvstVideoReceiver::new(config);
        let block = reference_fec_block(10, 5, 2, 100, false);
        let first = protect_for_test(&crypto, block[0].clone(), 0);
        assert!(
            receiver
                .process_datagram(peer(), &first, Instant::now())
                .is_empty()
        );
        let conflicting = reference_fec_block(10, 5, 3, 66, false);
        let packet = protect_for_test(&crypto, conflicting[if parity { 3 } else { 1 }].clone(), 0);
        assert_eq!(
            receiver.process_datagram(peer(), &packet, Instant::now()),
            [NvstReceiveEvent::Dropped(NvstDropReason::Unsupported(
                NvstUnsupportedFeature::FecRepair
            ))]
        );
        assert_eq!(feedback.keyframe_request_pending(), !parity);
        assert_eq!(receiver.next_frame_contiguous, parity);
    }
}

fn typed_frame(sequence: u16, frame: u32, frame_type: u8, nal: u8) -> Vec<u8> {
    build_plaintext_rtp(
        sequence,
        FLAG_SOF | FLAG_EOF | FLAG_CONTAINS_PIC_DATA,
        frame,
        &[0x01, 0, 0, frame_type, 0, 0, 0, 0, 0, 0, 0, 1, nal, 0x88],
    )
}

struct TypedStream {
    receiver: NvstVideoReceiver,
    crypto: SrtpReceiver,
    feedback: SharedNvstFeedback,
    origin: Instant,
}

impl TypedStream {
    fn new() -> Self {
        let mut config = config();
        config.reorder_window_packets = DEFAULT_REORDER_WINDOW;
        Self {
            feedback: config.feedback(),
            crypto: test_srtp(&config),
            receiver: NvstVideoReceiver::new(config),
            origin: Instant::now(),
        }
    }

    fn push(&mut self, packet: Vec<u8>, after: Duration) -> Vec<NvstReceiveEvent> {
        let packet = protect_for_test(&self.crypto, packet, 0);
        self.receiver
            .process_datagram(peer(), &packet, self.origin + after)
    }

    fn frames(&mut self, packet: Vec<u8>, after: Duration) -> Vec<(u32, bool, bool)> {
        delivered(self.push(packet, after))
    }

    fn stats(&self) -> String {
        self.feedback.frame_invalidation.stats()
    }

    /// Frames 1 (I) and 2 (P) arrive, frame 3 is lost beyond NACK recovery, and frames 4 and
    /// 5 are released by the reorder timeout.
    fn lose_frame_three(&mut self) -> Vec<NvstReceiveEvent> {
        assert_eq!(
            self.frames(typed_frame(0, 1, 2, 0x65), Duration::ZERO),
            [(1, true, true)]
        );
        assert_eq!(
            self.frames(typed_frame(1, 2, 1, 0x41), Duration::ZERO),
            [(2, false, true)]
        );
        assert!(self.push(typed_frame(3, 4, 1, 0x41), Duration::ZERO).is_empty());
        assert_eq!(self.feedback.take_nack(self.origin, None), Some((2, 2)));
        self.push(
            typed_frame(4, 5, 1, 0x41),
            MJOLNIR_REORDER_DEQUEUE_TIMEOUT + Duration::from_millis(1),
        )
    }
}

fn delivered(events: Vec<NvstReceiveEvent>) -> Vec<(u32, bool, bool)> {
    events
        .into_iter()
        .filter_map(|event| match event {
            NvstReceiveEvent::Frame(frame) => {
                Some((frame.frame_index, frame.keyframe, frame.contiguous))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn assembled_frames_carry_the_gs_frame_type_and_sender_frame_index() {
    for (wire, expected) in [
        (1, NvstFrameType::P),
        (2, NvstFrameType::I),
        (3, NvstFrameType::Header),
        (4, NvstFrameType::IntraRefresh),
        (5, NvstFrameType::RefPicI),
        (6, NvstFrameType::NonRefP),
        (9, NvstFrameType::Unknown(9)),
    ] {
        let mut stream = TypedStream::new();
        let [NvstReceiveEvent::Frame(frame)] =
            &stream.push(typed_frame(0, 77, wire, 0x41), Duration::ZERO)[..]
        else {
            panic!("frame type {wire} was not delivered");
        };
        assert_eq!(frame.frame_type, Some(expected));
        assert_eq!(frame.frame_index, 77);
    }
    let mut stream = TypedStream::new();
    let [NvstReceiveEvent::Frame(frame)] = &stream.push(
        build_plaintext_rtp(0, FLAG_SOF | FLAG_EOF, 78, &[0, 0, 0, 1, 0x65]),
        Duration::ZERO,
    )[..] else {
        panic!("headerless frame was not delivered");
    };
    assert_eq!(frame.frame_type, None);
}

#[test]
fn unrecoverable_loss_invalidates_the_lost_range_and_resumes_on_a_clearing_frame() {
    for (frame_type, nal, keyframe) in [(5, 0x41, false), (4, 0x41, false), (2, 0x65, true)] {
        let mut stream = TypedStream::new();
        let events = stream.lose_frame_three();
        assert!(matches!(
            events.first(),
            Some(NvstReceiveEvent::RecoveryNeeded(NvstRecovery::PacketGap { .. }))
        ));
        assert!(delivered(events).is_empty());
        assert_eq!(stream.feedback.frame_invalidation.take(), Some((3, 3)));
        assert!(!stream.feedback.keyframe_request_pending());
        assert!(
            stream
                .frames(typed_frame(5, 6, 1, 0x41), Duration::from_millis(60))
                .is_empty()
        );
        assert_eq!(
            stream.frames(
                typed_frame(6, 7, frame_type, nal),
                Duration::from_millis(70)
            ),
            [(7, keyframe, true)],
            "frame type {frame_type}"
        );
        assert_eq!(
            stream.frames(typed_frame(7, 8, 1, 0x41), Duration::from_millis(80)),
            [(8, false, true)]
        );
        assert_eq!(stream.feedback.frame_invalidation.take(), None);
        assert!(!stream.feedback.keyframe_request_pending());
        let recovered = match frame_type {
            5 => "rfiRecoveredI=0 rfiRecoveredIntraRefresh=0 rfiRecoveredRefPicI=1",
            4 => "rfiRecoveredI=0 rfiRecoveredIntraRefresh=1 rfiRecoveredRefPicI=0",
            _ => "rfiRecoveredI=1 rfiRecoveredIntraRefresh=0 rfiRecoveredRefPicI=0",
        };
        assert_eq!(
            stream.stats(),
            format!("rfiSent=0 rfiDropped=3 {recovered} rfiIdrBackstop=0")
        );
        assert_eq!(stream.receiver.frames_emitted, 4);
    }
}

#[test]
fn further_loss_during_invalidation_extends_the_pending_range() {
    let mut stream = TypedStream::new();
    stream.lose_frame_three();
    assert_eq!(stream.feedback.frame_invalidation.take(), Some((3, 3)));
    assert!(stream.push(typed_frame(6, 8, 1, 0x41), Duration::from_millis(60)).is_empty());
    let events = stream.push(
        typed_frame(7, 9, 6, 0x01),
        Duration::from_millis(60) + MJOLNIR_REORDER_DEQUEUE_TIMEOUT + Duration::from_millis(1),
    );
    assert!(delivered(events).is_empty());
    assert_eq!(stream.feedback.frame_invalidation.take(), Some((3, 7)));
    assert!(stream.stats().contains("rfiDropped=4"));
    assert!(!stream.feedback.keyframe_request_pending());
}

#[test]
fn header_frames_pass_during_invalidation() {
    let mut stream = TypedStream::new();
    stream.lose_frame_three();
    assert_eq!(
        stream.frames(typed_frame(5, 6, 3, 0x67), Duration::from_millis(60)),
        [(6, false, true)]
    );
    assert!(
        stream
            .frames(typed_frame(6, 7, 1, 0x41), Duration::from_millis(61))
            .is_empty()
    );
    assert!(stream.stats().contains("rfiDropped=3"));
}

#[test]
fn a_lost_frame_followed_by_a_header_frame_is_still_invalidated() {
    let mut stream = TypedStream::new();
    stream.frames(typed_frame(0, 1, 2, 0x65), Duration::ZERO);
    stream.push(typed_frame(2, 3, 3, 0x67), Duration::ZERO);
    let events = stream.push(
        typed_frame(3, 4, 1, 0x41),
        MJOLNIR_REORDER_DEQUEUE_TIMEOUT + Duration::from_millis(1),
    );
    assert_eq!(delivered(events), [(3, false, true)]);
    assert_eq!(stream.feedback.frame_invalidation.take(), Some((2, 2)));
    assert!(stream.stats().contains("rfiDropped=1"));
}

#[test]
fn invalidation_without_a_clearing_frame_falls_back_to_idr_after_the_cooldown() {
    let mut stream = TypedStream::new();
    stream.lose_frame_three();
    let lost_at = MJOLNIR_REORDER_DEQUEUE_TIMEOUT + Duration::from_millis(1);
    assert!(
        stream
            .frames(
                typed_frame(5, 6, 1, 0x41),
                lost_at + RFI_BACKSTOP_TIMEOUT - Duration::from_millis(1)
            )
            .is_empty()
    );
    assert!(!stream.feedback.keyframe_request_pending());
    assert!(
        stream
            .frames(typed_frame(6, 7, 1, 0x41), lost_at + RFI_BACKSTOP_TIMEOUT)
            .is_empty()
    );
    assert!(stream.feedback.keyframe_request_pending());
    assert!(
        stream
            .frames(
                typed_frame(7, 8, 1, 0x41),
                lost_at + RFI_BACKSTOP_TIMEOUT * 2
            )
            .is_empty()
    );
    assert_eq!(
        stream.frames(
            typed_frame(8, 9, 2, 0x65),
            lost_at + RFI_BACKSTOP_TIMEOUT * 3
        ),
        [(9, true, true)]
    );
    assert!(!stream.feedback.keyframe_request_pending());
    assert_eq!(
        stream.stats(),
        "rfiSent=0 rfiDropped=5 rfiRecoveredI=1 rfiRecoveredIntraRefresh=0 rfiRecoveredRefPicI=0 rfiIdrBackstop=1"
    );
}

#[test]
fn an_incomplete_frame_is_invalidated_once_the_next_frame_assembles() {
    let mut stream = TypedStream::new();
    stream.frames(typed_frame(0, 1, 2, 0x65), Duration::ZERO);
    let mut truncated = typed_frame(1, 2, 1, 0x41);
    truncated[24] = FLAG_SOF | FLAG_CONTAINS_PIC_DATA;
    assert!(stream.push(truncated, Duration::ZERO).is_empty());
    let mut skipped_index = build_plaintext_rtp(2, FLAG_EOF, 2, &[0x55]);
    skipped_index[16..20].copy_from_slice(&(5_u32 << 8).to_le_bytes());
    assert_eq!(
        stream.push(skipped_index, Duration::ZERO),
        [NvstReceiveEvent::Dropped(NvstDropReason::FrameDiscontinuity)]
    );
    assert!(!stream.feedback.keyframe_request_pending());
    assert!(
        stream
            .frames(typed_frame(3, 3, 1, 0x41), Duration::ZERO)
            .is_empty()
    );
    assert_eq!(stream.feedback.frame_invalidation.take(), Some((2, 2)));
    assert!(!stream.feedback.keyframe_request_pending());
}

#[test]
fn loss_without_a_frame_number_gap_requests_an_idr_immediately() {
    let feedback = NvstFeedbackState::default();
    let mut invalidation = crate::nvst_invalidation::FrameInvalidation::default();
    let now = Instant::now();
    let frame = |frame_index, frame_type| EncodedVideoAccessUnit {
        sender_timing: None,
        codec: NvstVideoCodec::H264,
        timestamp: 0,
        frame_index,
        first_stream_packet_index: 0,
        frame_type: Some(frame_type),
        keyframe: false,
        contiguous: true,
        bytes: Vec::new(),
    };
    assert!(invalidation.admit(&mut frame(10, NvstFrameType::P), false, Instant::now(), &feedback));
    assert!(invalidation.lose(now));
    assert!(!invalidation.admit(&mut frame(11, NvstFrameType::P), true, Instant::now(), &feedback));
    assert_eq!(feedback.frame_invalidation.take(), None);
    assert!(feedback.keyframe_request_pending());
    invalidation.poll_backstop(now + RFI_BACKSTOP_TIMEOUT, RFI_BACKSTOP_TIMEOUT, &feedback);
    assert!(feedback.frame_invalidation.stats().ends_with("rfiIdrBackstop=1"));
    let mut recovery = frame(12, NvstFrameType::RefPicI);
    assert!(invalidation.admit(&mut recovery, false, Instant::now(), &feedback));
    assert!(recovery.contiguous);
}

#[test]
fn frames_without_a_type_end_invalidation_through_the_keyframe_path() {
    let feedback = NvstFeedbackState::default();
    let mut invalidation = crate::nvst_invalidation::FrameInvalidation::default();
    let frame = |frame_index, frame_type| EncodedVideoAccessUnit {
        sender_timing: None,
        codec: NvstVideoCodec::H264,
        timestamp: 0,
        frame_index,
        first_stream_packet_index: 0,
        frame_type,
        keyframe: false,
        contiguous: true,
        bytes: Vec::new(),
    };
    assert!(!invalidation.lose(Instant::now()));
    assert!(invalidation.admit(&mut frame(1, Some(NvstFrameType::P)), false, Instant::now(), &feedback));
    assert!(invalidation.lose(Instant::now()));
    let mut untyped = frame(3, None);
    assert!(invalidation.admit(&mut untyped, true, Instant::now(), &feedback));
    assert!(!untyped.contiguous);
    assert!(feedback.keyframe_request_pending());
    assert_eq!(feedback.frame_invalidation.take(), None);
    assert!(!invalidation.lose(Instant::now()));
}

#[test]
fn nack_recovered_frames_are_never_invalidated() {
    let mut stream = TypedStream::new();
    stream.frames(typed_frame(0, 1, 2, 0x65), Duration::ZERO);
    stream.frames(typed_frame(1, 2, 1, 0x41), Duration::ZERO);
    assert!(stream.push(typed_frame(3, 4, 1, 0x41), Duration::ZERO).is_empty());
    assert_eq!(stream.feedback.take_nack(stream.origin, None), Some((2, 2)));
    assert_eq!(
        stream.frames(typed_frame(2, 3, 1, 0x41), Duration::from_millis(20)),
        [(3, false, true), (4, false, true)]
    );
    assert_eq!(stream.receiver.recovered_retransmissions, 1);
    assert_eq!(stream.feedback.frame_invalidation.take(), None);
    assert!(!stream.feedback.keyframe_request_pending());
    assert_eq!(
        stream.stats(),
        "rfiSent=0 rfiDropped=0 rfiRecoveredI=0 rfiRecoveredIntraRefresh=0 rfiRecoveredRefPicI=0 rfiIdrBackstop=0"
    );
}

#[test]
fn headerless_streams_keep_the_immediate_keyframe_recovery() {
    let mut stream = TypedStream::new();
    let headerless = |sequence, frame, nal| {
        build_plaintext_rtp(
            sequence,
            FLAG_SOF | FLAG_EOF | FLAG_CONTAINS_PIC_DATA,
            frame,
            &[0, 0, 0, 1, nal, 0x88],
        )
    };
    stream.frames(headerless(0, 1, 0x65), Duration::ZERO);
    stream.push(headerless(2, 3, 0x41), Duration::ZERO);
    let events = stream.push(
        headerless(3, 4, 0x41),
        MJOLNIR_REORDER_DEQUEUE_TIMEOUT + Duration::from_millis(1),
    );
    assert_eq!(delivered(events), [(3, false, false), (4, false, true)]);
    assert!(stream.feedback.keyframe_request_pending());
    assert_eq!(stream.feedback.frame_invalidation.take(), None);
}

#[test]
fn an_unanswered_invalidation_does_not_disable_future_rfi_episodes() {
    let feedback = NvstFeedbackState::default();
    let mut invalidation = crate::nvst_invalidation::FrameInvalidation::default();
    let now = Instant::now();
    let frame = |frame_index, frame_type| EncodedVideoAccessUnit {
        sender_timing: None,
        codec: NvstVideoCodec::H264,
        timestamp: 0,
        frame_index,
        first_stream_packet_index: 0,
        frame_type: Some(frame_type),
        keyframe: false,
        contiguous: true,
        bytes: Vec::new(),
    };
    assert!(invalidation.admit(&mut frame(1, NvstFrameType::P), false, Instant::now(), &feedback));
    assert!(invalidation.lose(now));
    assert!(!invalidation.admit(&mut frame(3, NvstFrameType::P), true, Instant::now(), &feedback));
    assert_eq!(feedback.frame_invalidation.take(), Some((2, 2)));
    feedback.frame_invalidation.record_sent(now);
    invalidation.poll_backstop(now + RFI_BACKSTOP_TIMEOUT, RFI_BACKSTOP_TIMEOUT, &feedback);
    assert!(feedback.keyframe_request_pending());
    assert!(invalidation.admit(&mut frame(5, NvstFrameType::I), true, Instant::now(), &feedback));
    assert!(invalidation.lose(now + RFI_BACKSTOP_TIMEOUT));

    let feedback = NvstFeedbackState::default();
    let mut invalidation = crate::nvst_invalidation::FrameInvalidation::default();
    assert!(invalidation.admit(&mut frame(1, NvstFrameType::P), false, Instant::now(), &feedback));
    assert!(invalidation.lose(now));
    assert!(invalidation.admit(&mut frame(3, NvstFrameType::RefPicI), true, Instant::now(), &feedback));
    assert!(invalidation.lose(now));
    invalidation.poll_backstop(now + RFI_BACKSTOP_TIMEOUT, RFI_BACKSTOP_TIMEOUT, &feedback);
    assert!(invalidation.admit(&mut frame(9, NvstFrameType::I), true, Instant::now(), &feedback));
    assert!(invalidation.lose(now + RFI_BACKSTOP_TIMEOUT));
}

#[test]
fn an_invalidation_that_was_never_sent_keeps_rfi_enabled() {
    let feedback = NvstFeedbackState::default();
    let mut invalidation = crate::nvst_invalidation::FrameInvalidation::default();
    let now = Instant::now();
    let frame = |frame_index, frame_type| EncodedVideoAccessUnit {
        sender_timing: None,
        codec: NvstVideoCodec::H264,
        timestamp: 0,
        frame_index,
        first_stream_packet_index: 0,
        frame_type: Some(frame_type),
        keyframe: false,
        contiguous: true,
        bytes: Vec::new(),
    };
    assert!(invalidation.admit(&mut frame(1, NvstFrameType::P), false, Instant::now(), &feedback));
    assert!(invalidation.lose(now));
    assert!(!invalidation.admit(&mut frame(3, NvstFrameType::P), true, Instant::now(), &feedback));
    assert_eq!(feedback.frame_invalidation.take(), Some((2, 2)));
    feedback.frame_invalidation.restore((2, 2));
    invalidation.poll_backstop(now + RFI_BACKSTOP_TIMEOUT, RFI_BACKSTOP_TIMEOUT, &feedback);
    assert!(feedback.keyframe_request_pending());
    assert!(feedback.frame_invalidation.stats().ends_with("rfiIdrBackstop=1"));
    assert!(invalidation.admit(&mut frame(5, NvstFrameType::I), true, Instant::now(), &feedback));
    assert!(invalidation.lose(now + RFI_BACKSTOP_TIMEOUT));
}

#[test]
fn recovery_discards_an_invalidation_that_was_not_sent_yet() {
    let feedback = NvstFeedbackState::default();
    let mut invalidation = crate::nvst_invalidation::FrameInvalidation::default();
    let now = Instant::now();
    let frame = |frame_index, frame_type| EncodedVideoAccessUnit {
        sender_timing: None,
        codec: NvstVideoCodec::H264,
        timestamp: 0,
        frame_index,
        first_stream_packet_index: 0,
        frame_type: Some(frame_type),
        keyframe: false,
        contiguous: true,
        bytes: Vec::new(),
    };
    assert!(invalidation.admit(&mut frame(1, NvstFrameType::P), false, Instant::now(), &feedback));
    assert!(invalidation.lose(now));
    assert!(!invalidation.admit(&mut frame(3, NvstFrameType::P), true, Instant::now(), &feedback));
    assert!(invalidation.admit(&mut frame(4, NvstFrameType::RefPicI), false, Instant::now(), &feedback));
    assert_eq!(feedback.frame_invalidation.take(), None);

    assert!(invalidation.lose(now));
    assert!(!invalidation.admit(&mut frame(6, NvstFrameType::P), true, Instant::now(), &feedback));
    assert_eq!(feedback.frame_invalidation.take(), Some((5, 5)));
    assert!(invalidation.admit(&mut frame(7, NvstFrameType::IntraRefresh), false, Instant::now(), &feedback));
    feedback.frame_invalidation.restore((5, 5));
    assert_eq!(feedback.frame_invalidation.take(), None);

    assert!(invalidation.lose(now));
    assert!(!invalidation.admit(&mut frame(9, NvstFrameType::P), true, Instant::now(), &feedback));
    invalidation.reset(&feedback);
    assert_eq!(feedback.frame_invalidation.take(), None);
}

#[test]
fn a_sender_frame_gap_without_packet_loss_starts_invalidation() {
    let mut stream = TypedStream::new();
    assert_eq!(
        stream.frames(typed_frame(0, 10, 2, 0x65), Duration::ZERO),
        [(10, true, true)]
    );
    assert!(
        stream
            .frames(typed_frame(1, 12, 1, 0x41), Duration::ZERO)
            .is_empty()
    );
    assert_eq!(stream.feedback.frame_invalidation.take(), Some((11, 11)));
    assert!(!stream.feedback.keyframe_request_pending());
}

#[test]
fn canonical_gs_headers_mask_priority_and_reject_bad_signatures() {
    for priority in 0..16 {
        for (wire, expected) in [
            (2, NvstFrameType::I),
            (4, NvstFrameType::IntraRefresh),
            (5, NvstFrameType::RefPicI),
        ] {
            let header = [0x47, 0x53, 1, (priority << 4) | wire];
            assert_eq!(
                GsFrameHeader::parse(&header, header.len())
                    .unwrap()
                    .frame_type,
                expected
            );
            let mut stream = TypedStream::new();
            let mut payload = header.to_vec();
            payload.extend_from_slice(&[0, 0, 0, 1, 0x41, 0x88]);
            let [NvstReceiveEvent::Frame(frame)] = &stream.push(
                build_plaintext_rtp(0, FLAG_SOF | FLAG_EOF, 7, &payload),
                Duration::ZERO,
            )[..] else {
                panic!("prioritized canonical GS frame was not delivered");
            };
            assert_eq!(frame.frame_type, Some(expected));
        }
    }
    for header in [
        &[0x47, 0x52, 1, 2][..],
        &[0x46, 0x53, 1, 2],
        &[0x47, 0x53, 1],
    ] {
        assert_eq!(GsFrameHeader::parse(header, header.len()), None);
    }
    for version in [0x01, 0x81] {
        assert_eq!(
            GsFrameHeader::parse(&[version, 0, 0, 0x52], 4)
                .unwrap()
                .frame_type,
            NvstFrameType::Unknown(0x52)
        );
    }
    let mut stream = TypedStream::new();
    let [NvstReceiveEvent::Frame(frame)] = &stream.push(
        build_plaintext_rtp(0, FLAG_SOF | FLAG_EOF, 7, &[0x47, 0x53, 0, 0, 0, 1, 0x65]),
        Duration::ZERO,
    )[..] else {
        panic!("frame with truncated GS metadata was not delivered");
    };
    assert_eq!(frame.frame_type, None);
}

#[test]
fn timer_expiry_drains_a_buffered_successor_without_another_packet() {
    let mut stream = TypedStream::new();
    stream.frames(typed_frame(0, 10, 2, 0x65), Duration::ZERO);
    assert!(
        stream
            .push(typed_frame(2, 12, 1, 0x41), Duration::ZERO)
            .is_empty()
    );
    assert!(
        stream
            .receiver
            .poll_recovery(
                stream.origin + MJOLNIR_REORDER_DEQUEUE_TIMEOUT - Duration::from_nanos(1)
            )
            .is_empty()
    );
    let events = stream
        .receiver
        .poll_recovery(stream.origin + MJOLNIR_REORDER_DEQUEUE_TIMEOUT);
    assert_eq!(
        events,
        [NvstReceiveEvent::RecoveryNeeded(NvstRecovery::PacketGap {
            first_missing_index: 1,
            last_missing_index: 1
        })]
    );
    assert_eq!(stream.feedback.frame_invalidation.take(), Some((11, 11)));
    assert!(
        stream
            .receiver
            .poll_recovery(stream.origin + MJOLNIR_REORDER_DEQUEUE_TIMEOUT)
            .is_empty()
    );
}

#[test]
fn fec_timer_expiry_drains_complete_successors_without_another_packet() {
    let now = Instant::now();
    let mut reorder = FecReorderBuffer::new(48);
    let packet = |index: u64| {
        let plaintext = build_plaintext_rtp(
            index as u16,
            FLAG_SOF | FLAG_EOF,
            index as u32,
            &[0, 0, 0, 1, 0x65],
        );
        RtpPacket {
            index,
            header: RtpHeader::parse(&plaintext).unwrap(),
            plaintext,
            origin: RtpPacketOrigin::Authenticated,
        }
    };
    let layout = |frame_index, shard_index| FecPacketLayout {
        frame_index,
        block_index: 0,
        last_block_index: 0,
        shard_index,
        data_shards: 2,
        parity_shards: 0,
    };
    assert!(reorder.push(packet(0), layout(1, 0), now).ready.is_empty());
    assert_eq!(
        reorder.push(packet(2), layout(2, 0), now).nack,
        Some((1, 1))
    );
    assert!(reorder.push(packet(3), layout(2, 1), now).ready.is_empty());
    assert!(
        reorder
            .poll_deadline(now + MJOLNIR_REORDER_DEQUEUE_TIMEOUT - Duration::from_nanos(1))
            .ready
            .is_empty()
    );
    let result = reorder.poll_deadline(now + MJOLNIR_REORDER_DEQUEUE_TIMEOUT);
    assert_eq!(
        result.recovery,
        Some(NvstRecovery::PacketGap {
            first_missing_index: 1,
            last_missing_index: 1
        })
    );
    assert_eq!(
        result
            .ready
            .iter()
            .map(|packet| packet.index)
            .collect::<Vec<_>>(),
        [2, 3]
    );
    assert!(reorder.blocks.is_empty());
}

#[test]
fn rfi_retries_preserve_the_episode_deadline_and_stop_on_recovery() {
    let mut stream = TypedStream::new();
    stream.frames(typed_frame(0, 10, 2, 0x65), Duration::ZERO);
    stream.frames(typed_frame(1, 12, 1, 0x41), Duration::ZERO);
    assert_eq!(stream.feedback.frame_invalidation.take(), Some((11, 11)));
    stream
        .feedback
        .frame_invalidation
        .record_sent(stream.origin + Duration::from_millis(50));
    stream
        .receiver
        .poll_recovery(stream.origin + Duration::from_millis(149));
    assert_eq!(stream.feedback.frame_invalidation.take(), None);
    stream
        .receiver
        .poll_recovery(stream.origin + Duration::from_millis(150));
    assert_eq!(stream.feedback.frame_invalidation.take(), Some((11, 11)));
    stream
        .feedback
        .frame_invalidation
        .record_sent(stream.origin + Duration::from_millis(150));
    stream.frames(typed_frame(2, 14, 1, 0x41), Duration::from_millis(200));
    assert_eq!(stream.feedback.frame_invalidation.take(), Some((11, 13)));
    stream
        .feedback
        .frame_invalidation
        .record_sent(stream.origin + Duration::from_millis(200));
    stream
        .receiver
        .poll_recovery(stream.origin + RFI_BACKSTOP_TIMEOUT);
    assert!(stream.feedback.keyframe_request_pending());
    stream.frames(typed_frame(3, 15, 2, 0x65), Duration::from_millis(260));
    stream
        .receiver
        .poll_recovery(stream.origin + Duration::from_secs(1));
    assert_eq!(stream.feedback.frame_invalidation.take(), None);
    assert!(!stream.feedback.keyframe_request_pending());
}
