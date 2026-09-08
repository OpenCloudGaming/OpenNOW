use super::super::*;

fn track() -> NvstAudioTrack {
    NvstAudioTrack {
        payload_type: GFN_OPUS_PAYLOAD_TYPE,
        clock_rate_hz: 48_000,
        channels: 2,
        mid: "audio".to_owned(),
        ssrc: None,
    }
}

fn header(sequence_number: u16) -> BundleRtpHeader {
    BundleRtpHeader {
        payload_type: GFN_OPUS_PAYLOAD_TYPE.into(),
        sequence_number,
        timestamp: u32::from(sequence_number) * 240,
        ssrc: 42.into(),
        ..BundleRtpHeader::default()
    }
}

fn red_packet(
    receiver: &mut NvstAudioReceiver,
    sequence: u16,
    timestamp: u32,
    redundant_blocks: usize,
) -> Vec<EncodedMediaFrame> {
    let mut header = header(sequence);
    header.payload_type = GFN_RED_PAYLOAD_TYPE.into();
    header.timestamp = timestamp;
    let mut payload = Vec::new();
    for index in (1..=redundant_blocks).rev() {
        let offset = index as u16 * 240;
        payload.extend_from_slice(&[
            0x80 | GFN_OPUS_PAYLOAD_TYPE,
            (offset >> 6) as u8,
            ((offset & 0x3f) << 2) as u8,
            1,
        ]);
    }
    payload.push(GFN_OPUS_PAYLOAD_TYPE);
    payload.extend((0..=redundant_blocks).map(|index| index as u8));
    receiver
        .depacketize(&track(), &header, payload.into(), 123_456)
        .unwrap()
}

#[test]
fn ordered_audio_preserves_payload_and_sender_metadata() {
    let mut receiver = NvstAudioReceiver::default();
    let payload: Arc<[u8]> = Arc::from([0xf8, 0xff, 0xfe]);
    for sequence in 10..13 {
        let frames = receiver
            .depacketize(&track(), &header(sequence), Arc::clone(&payload), 123_456)
            .unwrap();
        assert_eq!(frames.len(), 1);
        let frame = &frames[0];
        assert!(Arc::ptr_eq(&frame.payload, &payload));
        assert_eq!(frame.mid, "audio");
        assert_eq!(frame.codec, "opus");
        assert_eq!(frame.frame_index, None);
        assert_eq!(frame.rtp_timestamp, u64::from(sequence) * 240);
        assert_eq!(frame.clock_rate_hz, 48_000);
        assert_eq!(frame.channels, Some(2));
        assert_eq!(frame.received_at_us, 123_456);
        assert!(frame.contiguous);
    }
}

#[test]
fn late_originals_and_duplicates_never_replay_recovered_audio_or_rewind_sequence() {
    let mut receiver = NvstAudioReceiver::default();
    red_packet(&mut receiver, 10, 2_400, 1);
    let recovered = red_packet(&mut receiver, 12, 2_880, 1);
    assert_eq!(recovered.len(), 2);
    assert!(recovered.iter().all(|frame| frame.contiguous));
    for sequence in [11, 12, 10] {
        assert!(red_packet(&mut receiver, sequence, u32::from(sequence) * 240, 1).is_empty());
    }
    let next = red_packet(&mut receiver, 13, 3_120, 1);
    assert_eq!(next.len(), 1);
    assert!(next[0].contiguous);
}

#[test]
fn partial_red_recovery_marks_the_gap_before_the_first_recovered_packet() {
    let mut receiver = NvstAudioReceiver::default();
    red_packet(&mut receiver, 10, 2_400, 1);
    let frames = red_packet(&mut receiver, 14, 3_360, 2);
    assert_eq!(frames.len(), 3);
    assert_eq!(
        frames
            .iter()
            .map(|frame| frame.contiguous)
            .collect::<Vec<_>>(),
        [false, true, true]
    );
    assert_eq!(
        frames
            .iter()
            .map(|frame| frame.rtp_timestamp)
            .collect::<Vec<_>>(),
        [2_880, 3_120, 3_360]
    );
    for (index, frame) in frames.iter().enumerate() {
        assert_eq!(&*frame.payload, &[index as u8]);
    }
}

#[test]
fn gaps_beyond_red_capacity_remain_discontinuous_and_bounded() {
    for redundant_blocks in [0, MAX_REDUNDANT_AUDIO_BLOCKS] {
        let mut receiver = NvstAudioReceiver::default();
        red_packet(&mut receiver, 10, 2_400, 0);
        let frames = red_packet(&mut receiver, 20_000, 4_800_000, redundant_blocks);
        assert_eq!(frames.len(), redundant_blocks + 1);
        assert!(!frames[0].contiguous);
        assert!(frames[1..].iter().all(|frame| frame.contiguous));
    }
}

#[test]
fn plain_opus_gaps_are_marked_without_fabricating_packets() {
    let mut receiver = NvstAudioReceiver::default();
    red_packet(&mut receiver, 10, 2_400, 0);
    let frames = receiver
        .depacketize(&track(), &header(30), Arc::from([0xf8]), 0)
        .unwrap();
    assert_eq!(frames.len(), 1);
    assert!(!frames[0].contiguous);
}

#[test]
fn sequence_and_timestamp_wrap_preserve_recovery_order() {
    let mut receiver = NvstAudioReceiver::default();
    red_packet(&mut receiver, u16::MAX - 1, u32::MAX - 239, 0);
    let frames = red_packet(&mut receiver, 1, 480, 2);
    assert_eq!(frames.len(), 3);
    assert!(frames.iter().all(|frame| frame.contiguous));
    assert_eq!(
        frames
            .iter()
            .map(|frame| frame.rtp_timestamp)
            .collect::<Vec<_>>(),
        [0, 240, 480]
    );
    assert!(red_packet(&mut receiver, u16::MAX, 0, 0).is_empty());
    assert_eq!(red_packet(&mut receiver, 2, 720, 1).len(), 1);

    let mut receiver = NvstAudioReceiver::default();
    red_packet(&mut receiver, 10, u32::MAX - 479, 0);
    let frames = red_packet(&mut receiver, 12, 0, 1);
    assert_eq!(frames[0].rtp_timestamp, u64::from(u32::MAX - 239));
    assert_eq!(frames[1].rtp_timestamp, 0);
    assert!(frames.iter().all(|frame| frame.contiguous));
}

#[test]
fn malformed_red_does_not_prevent_recovery_from_the_next_packet() {
    let mut receiver = NvstAudioReceiver::default();
    red_packet(&mut receiver, 10, 2_400, 0);
    let mut malformed_header = header(11);
    malformed_header.payload_type = GFN_RED_PAYLOAD_TYPE.into();
    assert!(matches!(
        receiver.depacketize(&track(), &malformed_header, Arc::from([0xff]), 0),
        Err(NvstDropReason::MalformedRedAudio)
    ));
    let frames = red_packet(&mut receiver, 12, 2_880, 1);
    assert_eq!(frames.len(), 2);
    assert!(frames.iter().all(|frame| frame.contiguous));
    assert_eq!(frames[0].rtp_timestamp, 2_640);
}

#[test]
fn consumer_backpressure_marks_the_next_recovered_frame_not_the_primary() {
    let mut receiver = NvstAudioReceiver::default();
    let (consumer, delivered) = mpsc::sync_channel(1);
    for sequence in [10, 11] {
        let frame = red_packet(&mut receiver, sequence, u32::from(sequence) * 240, 0)
            .pop()
            .unwrap();
        let result = receiver.deliver(&consumer, frame);
        if sequence == 10 {
            assert!(result.is_ok());
        } else {
            assert!(matches!(
                result,
                Err(TransportError::MediaConsumerBackpressured)
            ));
        }
    }
    assert!(delivered.recv().unwrap().contiguous);
    let frames = red_packet(&mut receiver, 13, 3_120, 1);
    for (index, frame) in frames.into_iter().enumerate() {
        receiver.deliver(&consumer, frame).unwrap();
        let frame = delivered.recv().unwrap();
        assert_eq!(frame.contiguous, index != 0);
    }
    drop(delivered);
    let frame = red_packet(&mut receiver, 14, 3_360, 0).pop().unwrap();
    assert!(matches!(
        receiver.deliver(&consumer, frame),
        Err(TransportError::MediaConsumerClosed)
    ));
}

#[test]
fn a_new_source_starts_a_discontinuous_sequence_baseline() {
    let mut receiver = NvstAudioReceiver::default();
    red_packet(&mut receiver, 100, 24_000, 0);
    for sequence in [1, 2] {
        let mut header = header(sequence);
        header.ssrc = 43.into();
        let frames = receiver
            .depacketize(&track(), &header, Arc::from([0xf8]), 0)
            .unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].contiguous, sequence == 2);
    }
}
