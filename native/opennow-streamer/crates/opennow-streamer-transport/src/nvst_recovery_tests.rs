use super::*;

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
