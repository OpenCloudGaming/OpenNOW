use super::*;

fn synthetic_sender_prefix() -> Vec<u8> {
    vec![
        0x81, 8, 1, 2, 0x78, 0x56, 0x34, 0x12, 17, 0, 0, 0, 0, 2, 1, 0, 1, 1, 2, 8, 0, 0xd2, 0x02,
        0x96, 0x49, 0, 0, 0, 0,
    ]
}

fn synthetic_packet(frame_index: u32, flags: u8) -> NvVideoPacket {
    NvVideoPacket {
        stream_packet_index: 1,
        frame_index,
        flags,
        fec_current_block: 0,
        fec_last_block: 0,
        is_fec: false,
    }
}

#[test]
fn synthetic_29_byte_sender_prefix_preserves_codec_boundary() {
    let mut payload = synthetic_sender_prefix();
    assert_eq!(payload.len(), 29);
    let codec = [0x12, 0x00, 0x32, 0x01, 0xaa];
    payload.extend_from_slice(&codec);
    let mut assembler = VideoAccessUnitAssembler::new(NvstVideoCodec::Av1, 4096);
    let frame = assembler
        .push(synthetic_packet(9, FLAG_SOF | FLAG_EOF), 90_000, &payload)
        .expect("synthetic sender prefix")
        .expect("complete frame");
    assert_eq!(frame.bytes, codec);
    assert_eq!(frame.timestamp, 90_000);
    assert_eq!(frame.frame_type, Some(NvstFrameType::I));
    assert_eq!(
        frame.sender_timing,
        Some(SenderFrameTiming {
            capture_time_us: 0x1234_5678,
            sender_paced: Some(1),
            presentation_time_us: Some(1_234_567_890),
        })
    );
}

fn synthetic_record(extensions: &mut Vec<u8>, tag: u16, value: &[u8]) {
    extensions.extend_from_slice(&tag.to_le_bytes());
    extensions.extend_from_slice(&(value.len() as u16).to_le_bytes());
    extensions.extend_from_slice(value);
}

fn synthetic_extended_payload(extensions: &[u8]) -> Vec<u8> {
    let mut payload = synthetic_sender_prefix()[..8].to_vec();
    payload.extend_from_slice(&(extensions.len() as u32).to_le_bytes());
    payload.extend_from_slice(extensions);
    payload.extend_from_slice(&[0x12, 0]);
    payload
}

#[test]
fn synthetic_sender_fields_preserve_raw_values_without_rtp_conversion() {
    for flag in [0, 1, 7, u8::MAX] {
        let mut extensions = Vec::new();
        synthetic_record(&mut extensions, 0x200, &[flag, 0xaa]);
        synthetic_record(&mut extensions, 0x201, &[8, 7, 6, 5, 4, 3, 2, 1, 0xff]);
        let mut payload = synthetic_extended_payload(&extensions);
        payload[3] = 0xf2;
        payload[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        let parsed = assembled_video_payload(NvstVideoCodec::Av1, &payload).unwrap();
        assert_eq!(parsed.frame_type, Some(NvstFrameType::I));
        assert_eq!(parsed.bytes, [0x12, 0]);
        assert_eq!(
            parsed.sender_timing,
            Some(SenderFrameTiming {
                capture_time_us: u32::MAX,
                sender_paced: Some(flag),
                presentation_time_us: Some(0x0102_0304_0506_0708),
            })
        );
    }
}

#[test]
fn synthetic_sender_extensions_accept_any_order_unknowns_and_last_complete_duplicates() {
    for reverse in [false, true] {
        let mut records = vec![
            (0x200, vec![3]),
            (0x201, 111_u64.to_le_bytes().to_vec()),
            (0x777, vec![0, 0, 1, 0x65]),
            (0, vec![]),
            (0x200, vec![9]),
            (0x201, 222_u64.to_le_bytes().to_vec()),
        ];
        if reverse {
            records.reverse();
        }
        let mut extensions = Vec::new();
        for (tag, value) in records {
            synthetic_record(&mut extensions, tag, &value);
        }
        let payload = synthetic_extended_payload(&extensions);
        let parsed = assembled_video_payload(NvstVideoCodec::Av1, &payload).unwrap();
        assert_eq!(parsed.bytes, [0x12, 0]);
        let timing = parsed.sender_timing.unwrap();
        assert_eq!(timing.sender_paced, Some(if reverse { 3 } else { 9 }));
        assert_eq!(
            timing.presentation_time_us,
            Some(if reverse { 111 } else { 222 })
        );
    }
}

#[test]
fn synthetic_sender_prefix_without_extensions_and_with_empty_extensions() {
    for has_extensions in [false, true] {
        for header_size in [8, 13, u8::MAX] {
            let mut payload = vec![0xaa; usize::from(header_size)];
            payload[..8].copy_from_slice(&[
                if has_extensions { 0x81 } else { 1 },
                header_size,
                255,
                0xf2,
                0,
                0,
                0,
                0,
            ]);
            if has_extensions {
                payload.extend_from_slice(&0_u32.to_le_bytes());
            }
            payload.push(0x12);
            let parsed = assembled_video_payload(NvstVideoCodec::Av1, &payload).unwrap();
            assert_eq!(parsed.bytes, [0x12]);
            assert_eq!(parsed.sender_timing, Some(SenderFrameTiming::default()));
            assert_eq!(parsed.frame_type, Some(NvstFrameType::I));
        }
    }
}

#[test]
fn synthetic_optional_metadata_fields_preserve_absence_and_explicit_zero() {
    for (tag, value) in [(0x200, vec![0]), (0x201, 0_u64.to_le_bytes().to_vec())] {
        let mut extensions = Vec::new();
        synthetic_record(&mut extensions, tag, &value);
        let payload = synthetic_extended_payload(&extensions);
        let timing = assembled_video_payload(NvstVideoCodec::Av1, &payload)
            .unwrap()
            .sender_timing
            .unwrap();
        assert_eq!(
            timing.sender_paced,
            if tag == 0x200 { Some(0) } else { None }
        );
        assert_eq!(
            timing.presentation_time_us,
            if tag == 0x201 { Some(0) } else { None }
        );
    }
}

#[test]
fn synthetic_base_capture_is_never_interpreted_as_a_legacy_av1_length_hint() {
    let suffix = [0x12, 0, 0x32, 1, 0xaa, 0xbb];
    let mut payload = vec![1, 8, 1, 2, 11, 0, 0, 0];
    payload.extend_from_slice(&suffix);
    let mut assembler = VideoAccessUnitAssembler::new(NvstVideoCodec::Av1, 4096);
    let frame = assembler
        .push(synthetic_packet(1, FLAG_SOF | FLAG_EOF), 0, &payload)
        .unwrap()
        .unwrap();
    assert_eq!(frame.bytes, suffix);
    assert_eq!(frame.sender_timing.unwrap().capture_time_us, 11);
}

#[test]
fn synthetic_unidentified_version_retains_legacy_annex_b_without_metadata() {
    let payload = [2, 8, 1, 2, 0x78, 0x56, 0x34, 0x12, 0, 0, 0, 1, 0x65];
    let parsed = assembled_video_payload(NvstVideoCodec::H264, &payload).unwrap();
    assert_eq!(parsed.bytes, [0, 0, 0, 1, 0x65]);
    assert_eq!(parsed.sender_timing, None);
}

#[test]
fn synthetic_sender_prefix_rejects_every_truncation_and_empty_codec() {
    let prefix = synthetic_sender_prefix();
    for codec in [
        NvstVideoCodec::Av1,
        NvstVideoCodec::H264,
        NvstVideoCodec::H265,
    ] {
        for end in 0..=prefix.len() {
            assert!(
                assembled_video_payload(codec, &prefix[..end]).is_err(),
                "codec={codec:?}, end={end}"
            );
        }
        for header_size in 1..8 {
            let mut payload = vec![1, header_size, 0, 2, 0, 0, 1, 0x65];
            payload.extend_from_slice(&[0, 0, 0, 1, 0x65]);
            assert!(assembled_video_payload(codec, &payload).is_err());
        }
    }
}

#[test]
fn synthetic_identified_prefix_never_scans_for_annex_b_inside_metadata() {
    let mut payload = synthetic_sender_prefix();
    payload[4..8].copy_from_slice(&[0, 0, 1, 0x65]);
    payload[8..12].copy_from_slice(&1025_u32.to_le_bytes());
    payload.extend_from_slice(&[0, 0, 0, 1, 0x65]);
    for codec in [
        NvstVideoCodec::H264,
        NvstVideoCodec::H265,
        NvstVideoCodec::Av1,
    ] {
        let mut assembler = VideoAccessUnitAssembler::new(codec, 4096);
        assert!(
            assembler
                .push(synthetic_packet(1, FLAG_SOF | FLAG_EOF), 0, &payload)
                .is_err()
        );
        assert!(assembler.bytes.is_empty());
        assert_eq!(assembler.current_frame, None);
    }
}

#[test]
fn synthetic_truncated_extended_header_rejects_original_picture_flag_fixture() {
    let mut assembler = VideoAccessUnitAssembler::new(NvstVideoCodec::H264, 4096);
    assert_eq!(
        assembler.push(
            synthetic_packet(12, FLAG_SOF | FLAG_EOF),
            90_000,
            &[0x81, 0x08, 0, 0, 0, 1, 0x61, 0xaa],
        ),
        Err(NvstDropReason::MalformedFrameHeader),
    );
}

#[test]
fn synthetic_sender_extensions_reject_short_tags_incomplete_records_and_oversize() {
    for tag in [0x200, 0x201] {
        let minimum = if tag == 0x200 { 1 } else { 8 };
        for length in 0..minimum {
            let mut extensions = Vec::new();
            synthetic_record(&mut extensions, tag, &vec![0; length]);
            let payload = synthetic_extended_payload(&extensions);
            assert!(assembled_video_payload(NvstVideoCodec::Av1, &payload).is_err());
        }
    }
    for trailing in 1..4 {
        let payload = synthetic_extended_payload(&vec![0; trailing]);
        assert!(assembled_video_payload(NvstVideoCodec::Av1, &payload).is_err());
    }
    for tag in [0x200_u16, 0x201, 0x777] {
        let mut extensions = tag.to_le_bytes().to_vec();
        extensions.extend_from_slice(&u16::MAX.to_le_bytes());
        let payload = synthetic_extended_payload(&extensions);
        assert!(assembled_video_payload(NvstVideoCodec::Av1, &payload).is_err());
    }
    for length in [1025_u32, u32::MAX] {
        let mut payload = synthetic_extended_payload(&[]);
        payload[8..12].copy_from_slice(&length.to_le_bytes());
        assert!(assembled_video_payload(NvstVideoCodec::Av1, &payload).is_err());
    }
    for malformed in [
        &[0, 2, 0, 0][..],
        &[1, 2, 7, 0, 0, 0, 0, 0, 0, 0, 0],
        &[0, 2, 1][..],
    ] {
        let mut extensions = Vec::new();
        synthetic_record(&mut extensions, 0x200, &[1]);
        synthetic_record(&mut extensions, 0x201, &99_u64.to_le_bytes());
        extensions.extend_from_slice(malformed);
        let payload = synthetic_extended_payload(&extensions);
        assert!(assembled_video_payload(NvstVideoCodec::Av1, &payload).is_err());
    }
}

#[test]
fn synthetic_sender_prefix_splits_at_every_packet_boundary() {
    for (codec, suffix) in [
        (NvstVideoCodec::Av1, vec![0x12, 0, 0x32, 1, 0xaa]),
        (NvstVideoCodec::H264, vec![0, 0, 0, 1, 0x65, 0xaa]),
        (NvstVideoCodec::H265, vec![0, 0, 0, 1, 19 << 1, 1, 0xaa]),
    ] {
        let mut payload = synthetic_sender_prefix();
        payload.extend_from_slice(&suffix);
        for split in 0..=payload.len() {
            let mut assembler = VideoAccessUnitAssembler::new(codec, 4096);
            assert!(
                assembler
                    .push(synthetic_packet(9, FLAG_SOF), 90_000, &payload[..split])
                    .unwrap()
                    .is_none()
            );
            let frame = assembler
                .push(synthetic_packet(9, FLAG_EOF), 90_000, &payload[split..])
                .unwrap()
                .unwrap();
            assert_eq!(frame.bytes, suffix, "split={split}");
            assert_eq!(frame.sender_timing.unwrap().capture_time_us, 0x1234_5678);
            assert_eq!(frame.sender_timing.unwrap().sender_paced, Some(1));
            assert_eq!(
                frame.sender_timing.unwrap().presentation_time_us,
                Some(1_234_567_890)
            );
            assert_eq!(frame.frame_type, Some(NvstFrameType::I));
        }
        let mut assembler = VideoAccessUnitAssembler::new(codec, 4096);
        let mut frame = None;
        for (index, byte) in payload.iter().enumerate() {
            let flags = if index == 0 {
                FLAG_SOF
            } else if index + 1 == payload.len() {
                FLAG_EOF
            } else {
                0
            };
            frame = assembler
                .push(synthetic_packet(9, flags), 90_000, &[*byte])
                .unwrap();
        }
        assert_eq!(frame.unwrap().bytes, suffix);
    }
}

#[test]
fn synthetic_sender_prefix_retains_codec_and_raw_assembly_limits() {
    let mut extensions = Vec::new();
    synthetic_record(&mut extensions, 0x777, &vec![0; 1020]);
    assert_eq!(extensions.len(), MAX_SENDER_EXTENSION_BYTES);
    let mut payload = vec![0; 255];
    payload[..8].copy_from_slice(&[0x81, 255, 1, 2, 0x78, 0x56, 0x34, 0x12]);
    payload.extend_from_slice(&1024_u32.to_le_bytes());
    payload.extend_from_slice(&extensions);
    assert_eq!(payload.len(), MAX_ASSEMBLED_FRAME_PREFIX_BYTES);
    payload.push(0x12);
    let mut assembler = VideoAccessUnitAssembler::new(NvstVideoCodec::Av1, 1);
    let frame = assembler
        .push(synthetic_packet(1, FLAG_SOF | FLAG_EOF), 0, &payload)
        .unwrap()
        .unwrap();
    assert_eq!(frame.bytes, [0x12]);
    assert_eq!(frame.sender_timing.unwrap().capture_time_us, 0x1234_5678);
    payload.push(0x34);
    assert_eq!(
        assembler.push(synthetic_packet(2, FLAG_SOF | FLAG_EOF), 0, &payload),
        Err(NvstDropReason::AccessUnitTooLarge { limit: 1 })
    );
    let mut small_prefix = synthetic_extended_payload(&[]);
    small_prefix.push(0x56);
    assert_eq!(
        assembler.push(synthetic_packet(3, FLAG_SOF | FLAG_EOF), 0, &small_prefix),
        Err(NvstDropReason::AccessUnitTooLarge { limit: 1 })
    );
    assert!(
        assembler
            .push(
                synthetic_packet(4, FLAG_SOF),
                0,
                &vec![0; MAX_ASSEMBLED_FRAME_PREFIX_BYTES + 1]
            )
            .unwrap()
            .is_none()
    );
    assert_eq!(
        assembler.push(synthetic_packet(4, 0), 0, &[0]),
        Err(NvstDropReason::AccessUnitTooLarge { limit: 1 })
    );
    assert!(assembler.bytes.is_empty());
    assert_eq!(assembler.current_frame, None);
}

#[test]
fn synthetic_sender_metadata_does_not_leak_across_reset_failure_or_legacy_frames() {
    let mut payload = synthetic_sender_prefix();
    payload.push(0x12);
    let legacy = [1, 0, 0, 2, 9, 0, 0, 0, 0x12];
    let mut assembler = VideoAccessUnitAssembler::new(NvstVideoCodec::Av1, 4096);
    let frame = assembler
        .push(synthetic_packet(1, FLAG_SOF | FLAG_EOF), 0, &payload)
        .unwrap()
        .unwrap();
    assert!(frame.sender_timing.is_some());
    assert!(
        assembler
            .push(synthetic_packet(2, FLAG_SOF), 0, &payload[..25])
            .unwrap()
            .is_none()
    );
    assembler.reset();
    assert_eq!(
        assembler.push(synthetic_packet(2, FLAG_EOF), 0, &payload[25..]),
        Err(NvstDropReason::AwaitingStartOfFrame)
    );
    assert!(
        assembler
            .push(synthetic_packet(3, FLAG_SOF), 0, &payload[..25])
            .unwrap()
            .is_none()
    );
    assert_eq!(
        assembler.push(synthetic_packet(4, FLAG_EOF), 0, &payload[25..]),
        Err(NvstDropReason::AwaitingStartOfFrame)
    );
    assert!(
        assembler
            .push(synthetic_packet(5, FLAG_SOF | FLAG_EOF), 0, &payload[..29])
            .is_err()
    );
    assert!(
        assembler
            .push(synthetic_packet(6, FLAG_SOF), 0, &payload[..25])
            .unwrap()
            .is_none()
    );
    for frame_index in [7, 8] {
        let frame = assembler
            .push(
                synthetic_packet(frame_index, FLAG_SOF | FLAG_EOF),
                0,
                &legacy,
            )
            .unwrap()
            .unwrap();
        assert_eq!(frame.sender_timing, None);
        assert_eq!(frame.bytes, [0x12]);
    }
    let frame = assembler
        .push(synthetic_packet(9, FLAG_SOF | FLAG_EOF), 0, &payload)
        .unwrap()
        .unwrap();
    assert_eq!(
        frame.sender_timing.unwrap().presentation_time_us,
        Some(1_234_567_890)
    );
}

#[test]
fn synthetic_legacy_av1_headers_keep_short_and_compact_length_hints_without_metadata() {
    let mut compact = vec![0; 20];
    compact[0] = 0x81;
    compact[3] = 2;
    compact[16..20].copy_from_slice(&3_u32.to_le_bytes());
    compact.extend_from_slice(&[0x12, 0x34, 0x56, 0, 0, 0]);
    let short = vec![1, 0, 0, 2, 11, 0, 0, 0, 0x12, 0x34, 0x56, 0, 0, 0];
    for payload in [compact, short] {
        let mut assembler = VideoAccessUnitAssembler::new(NvstVideoCodec::Av1, 4096);
        let frame = assembler
            .push(synthetic_packet(1, FLAG_SOF | FLAG_EOF), 0, &payload)
            .unwrap()
            .unwrap();
        assert_eq!(frame.bytes, [0x12, 0x34, 0x56]);
        assert_eq!(frame.sender_timing, None);
    }
    for split in 0..20 {
        let mut payload = vec![0; 20];
        payload[0] = 0x81;
        payload[3] = 2;
        payload[16..20].copy_from_slice(&3_u32.to_le_bytes());
        payload.extend_from_slice(&[0x12, 0x34, 0x56, 0, 0, 0]);
        let mut assembler = VideoAccessUnitAssembler::new(NvstVideoCodec::Av1, 4096);
        assembler
            .push(synthetic_packet(1, FLAG_SOF), 0, &payload[..split])
            .unwrap();
        let frame = assembler
            .push(synthetic_packet(1, FLAG_EOF), 0, &payload[split..])
            .unwrap()
            .unwrap();
        assert_eq!(frame.bytes, [0x12, 0x34, 0x56]);
        assert_eq!(frame.sender_timing, None);
    }
}

#[test]
fn synthetic_legacy_av1_padding_keeps_existing_codec_size_limit_semantics() {
    let mut short = vec![1, 0, 0, 2, 11, 0, 0, 0, 0x12, 0x34, 0x56];
    short.resize(1500, 0);
    let mut assembler = VideoAccessUnitAssembler::new(NvstVideoCodec::Av1, 3);
    let frame = assembler
        .push(synthetic_packet(1, FLAG_SOF | FLAG_EOF), 0, &short)
        .unwrap()
        .unwrap();
    assert_eq!(frame.bytes, [0x12, 0x34, 0x56]);
    assert_eq!(frame.sender_timing, None);
    let mut compact = vec![0; 20];
    compact[0] = 0x81;
    compact[16..20].copy_from_slice(&3_u32.to_le_bytes());
    compact.extend_from_slice(&[0x12, 0x34, 0x56, 0]);
    assert_eq!(
        assembler.push(synthetic_packet(2, FLAG_SOF | FLAG_EOF), 0, &compact),
        Err(NvstDropReason::AccessUnitTooLarge { limit: 3 })
    );
    assert!(assembler.bytes.is_empty());
    let raw_limit = 3 + MAX_ASSEMBLED_FRAME_PREFIX_BYTES + MAX_NVST_VIDEO_PACKET_SIZE;
    assert_eq!(
        assembler.push(
            synthetic_packet(3, FLAG_SOF | FLAG_EOF),
            0,
            &vec![0; raw_limit + 1]
        ),
        Err(NvstDropReason::AccessUnitTooLarge { limit: 3 })
    );
}

#[test]
fn synthetic_sender_metadata_is_forwarded_with_unchanged_rtp_timestamp() {
    let (consumer, receiver) = mpsc::sync_channel(1);
    let (events, _event_receiver) = mpsc::channel();
    let feedback: SharedNvstFeedback = Arc::new(NvstFeedbackState::default());
    let origin = Instant::now();
    let mut assembler = VideoAccessUnitAssembler::new(NvstVideoCodec::Av1, 4096);
    for payload in [
        synthetic_extended_payload(&synthetic_sender_prefix()[12..]),
        vec![1, 0, 0, 2, 9, 0, 0, 0, 0x12],
    ] {
        let frame = assembler
            .push(synthetic_packet(1, FLAG_SOF | FLAG_EOF), 90_000, &payload)
            .unwrap()
            .unwrap();
        let expected = frame.sender_timing;
        assert!(forward_receive_event(
            &consumer,
            &events,
            &feedback,
            origin,
            Instant::now(),
            &mut false,
            NvstReceiveEvent::Frame(frame)
        ));
        let delivered = receiver.try_recv().unwrap();
        assert_eq!(delivered.sender_timing, expected);
        assert_eq!(delivered.rtp_timestamp, 90_000);
        assert_eq!(delivered.clock_rate_hz, 90_000);
        assert_eq!(delivered.frame_index, Some(1));
    }
}

#[test]
fn synthetic_av1_length_tag_uses_shape_observed_in_real_hevc_capture() {
    let codec = [0x12, 0, 0x32, 1, 0xaa];
    let mut payload = vec![
        0x81, 8, 1, 2, 0x44, 0x33, 0x22, 0x11, 8, 0, 0, 0, 2, 2, 4, 0, 5, 0, 0, 0,
    ];
    assert_eq!(payload.len(), 20);
    payload.extend_from_slice(&codec);
    payload.extend_from_slice(&[0; 17]);
    for split in 0..=payload.len() {
        let mut assembler = VideoAccessUnitAssembler::new(NvstVideoCodec::Av1, codec.len());
        assert!(
            assembler
                .push(synthetic_packet(1, FLAG_SOF), 90_000, &payload[..split])
                .unwrap()
                .is_none()
        );
        let frame = assembler
            .push(synthetic_packet(1, FLAG_EOF), 90_000, &payload[split..])
            .unwrap()
            .unwrap();
        assert_eq!(frame.bytes, codec, "split={split}");
        assert_eq!(
            frame.sender_timing,
            Some(SenderFrameTiming {
                capture_time_us: 0x1122_3344,
                sender_paced: None,
                presentation_time_us: None,
            })
        );
    }
}

#[test]
fn synthetic_av1_length_tag_skips_unknowns_and_uses_last_complete_duplicate() {
    let mut extensions = Vec::new();
    synthetic_record(&mut extensions, 0x202, &999_u32.to_le_bytes());
    synthetic_record(&mut extensions, 0x777, &[0xaa]);
    synthetic_record(&mut extensions, 0x200, &[7]);
    synthetic_record(&mut extensions, 0x778, &[1, 2, 3]);
    synthetic_record(&mut extensions, 0x201, &123_u64.to_le_bytes());
    synthetic_record(&mut extensions, 0x202, &[2, 0, 0, 0, 0xff]);
    synthetic_record(&mut extensions, 0x779, &[]);
    let mut payload = synthetic_extended_payload(&extensions);
    payload.resize(payload.len() + 1500, 0);
    let parsed = assembled_video_payload(NvstVideoCodec::Av1, &payload).unwrap();
    assert_eq!(parsed.expected_access_unit_length, Some(2));
    let mut assembler = VideoAccessUnitAssembler::new(NvstVideoCodec::Av1, 2);
    let frame = assembler
        .push(synthetic_packet(1, FLAG_SOF | FLAG_EOF), 0, &payload)
        .unwrap()
        .unwrap();
    assert_eq!(frame.bytes, [0x12, 0]);
    assert_eq!(
        frame.sender_timing,
        Some(SenderFrameTiming {
            capture_time_us: 0x1234_5678,
            sender_paced: Some(7),
            presentation_time_us: Some(123),
        })
    );
    let payload = synthetic_extended_payload(&[]);
    let frame = assembler
        .push(synthetic_packet(2, FLAG_SOF | FLAG_EOF), 0, &payload)
        .unwrap()
        .unwrap();
    assert_eq!(frame.bytes, [0x12, 0]);
    assert_eq!(frame.sender_timing.unwrap().sender_paced, None);
    assert_eq!(frame.sender_timing.unwrap().presentation_time_us, None);
}

#[test]
fn synthetic_av1_length_tag_rejects_short_or_incomplete_records_even_after_valid_length() {
    for length in 0..4 {
        let mut extensions = Vec::new();
        synthetic_record(&mut extensions, 0x202, &2_u32.to_le_bytes());
        synthetic_record(&mut extensions, 0x202, &vec![0; length]);
        let payload = synthetic_extended_payload(&extensions);
        assert!(assembled_video_payload(NvstVideoCodec::Av1, &payload).is_err());
    }
    for trailing in [&[2, 2][..], &[2, 2, 4], &[2, 2, 4, 0, 2, 0, 0]] {
        let mut extensions = Vec::new();
        synthetic_record(&mut extensions, 0x202, &2_u32.to_le_bytes());
        extensions.extend_from_slice(trailing);
        let payload = synthetic_extended_payload(&extensions);
        assert!(assembled_video_payload(NvstVideoCodec::Av1, &payload).is_err());
    }
}

#[test]
fn synthetic_av1_length_tag_rejects_invalid_sizes_and_resets_without_length_leaks() {
    let mut assembler = VideoAccessUnitAssembler::new(NvstVideoCodec::Av1, 2);
    for reported in [0_u32, 5] {
        let mut extensions = Vec::new();
        synthetic_record(&mut extensions, 0x202, &reported.to_le_bytes());
        let mut payload = synthetic_extended_payload(&extensions);
        payload.extend_from_slice(&[0, 0]);
        let expected = if reported == 0 {
            NvstDropReason::AccessUnitTooLarge { limit: 2 }
        } else {
            NvstDropReason::InvalidLastPacketPayloadLength {
                reported: reported as usize,
                frame_header_size: 20,
                available: 4,
            }
        };
        assert_eq!(
            assembler.push(synthetic_packet(1, FLAG_SOF | FLAG_EOF), 0, &payload),
            Err(expected)
        );
        assert!(assembler.bytes.is_empty());
        assert_eq!(assembler.current_frame, None);
    }
    let mut extensions = Vec::new();
    synthetic_record(&mut extensions, 0x202, &3_u32.to_le_bytes());
    let mut payload = synthetic_extended_payload(&extensions);
    payload.extend_from_slice(&[0, 0]);
    assert_eq!(
        assembler.push(synthetic_packet(2, FLAG_SOF | FLAG_EOF), 0, &payload),
        Err(NvstDropReason::AccessUnitTooLarge { limit: 2 })
    );
    let payload = synthetic_extended_payload(&[]);
    let frame = assembler
        .push(synthetic_packet(3, FLAG_SOF | FLAG_EOF), 0, &payload)
        .unwrap()
        .unwrap();
    assert_eq!(frame.bytes, [0x12, 0]);
    assert_eq!(frame.sender_timing.unwrap().capture_time_us, 0x1234_5678);
}

#[test]
fn synthetic_normal_length_tag_trims_all_codecs_before_output_size_check() {
    for (codec, suffix) in [
        (NvstVideoCodec::Av1, vec![0x12, 0]),
        (NvstVideoCodec::H264, vec![0, 0, 0, 1, 0x61, 0xaa]),
        (NvstVideoCodec::H265, vec![0, 0, 0, 1, 0x26, 1, 0xaa]),
    ] {
        let mut extensions = Vec::new();
        synthetic_record(&mut extensions, 0x202, &(suffix.len() as u32).to_le_bytes());
        let mut payload = synthetic_extended_payload(&extensions);
        payload.truncate(payload.len() - 2);
        payload.extend_from_slice(&suffix);
        payload.extend_from_slice(&[0; 17]);
        for split in 0..=payload.len() {
            let mut assembler = VideoAccessUnitAssembler::new(codec, suffix.len());
            assembler
                .push(synthetic_packet(1, FLAG_SOF), 90_000, &payload[..split])
                .unwrap();
            let frame = assembler
                .push(synthetic_packet(1, FLAG_EOF), 90_000, &payload[split..])
                .unwrap()
                .unwrap();
            assert_eq!(frame.bytes, suffix, "codec={codec:?}, split={split}");
        }
    }
}

#[test]
fn synthetic_normal_zero_length_tag_is_ignored_and_last_complete_duplicate_wins() {
    let mut extensions = Vec::new();
    synthetic_record(&mut extensions, 0x202, &1_u32.to_le_bytes());
    synthetic_record(&mut extensions, 0x202, &0_u32.to_le_bytes());
    let payload = synthetic_extended_payload(&extensions);
    for codec in [
        NvstVideoCodec::Av1,
        NvstVideoCodec::H264,
        NvstVideoCodec::H265,
    ] {
        let mut assembler = VideoAccessUnitAssembler::new(codec, 2);
        let frame = assembler
            .push(synthetic_packet(1, FLAG_SOF | FLAG_EOF), 0, &payload)
            .unwrap()
            .unwrap();
        assert_eq!(frame.bytes, [0x12, 0]);
    }
}

#[test]
fn synthetic_normal_length_tag_never_increases_available_bytes_for_any_codec() {
    for codec in [
        NvstVideoCodec::Av1,
        NvstVideoCodec::H264,
        NvstVideoCodec::H265,
    ] {
        for reported in [3_u32, u32::MAX] {
            let mut extensions = Vec::new();
            synthetic_record(&mut extensions, 0x202, &reported.to_le_bytes());
            let payload = synthetic_extended_payload(&extensions);
            let mut assembler = VideoAccessUnitAssembler::new(codec, 4096);
            assert_eq!(
                assembler.push(synthetic_packet(1, FLAG_SOF | FLAG_EOF), 0, &payload),
                Err(NvstDropReason::InvalidLastPacketPayloadLength {
                    reported: reported as usize,
                    frame_header_size: 20,
                    available: 2,
                })
            );
            assert!(assembler.bytes.is_empty());
            assert_eq!(assembler.current_frame, None);
        }
    }
}
