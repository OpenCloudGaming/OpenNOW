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

#[test]
fn rejected_audio_ssrcs_do_not_fill_admission_or_evict_authenticated_streams() {
    install_crypto();
    let mut rtc = RtcConfig::new().set_rtp_mode(true).build(Instant::now());
    let track = track();
    rtc.direct_api()
        .declare_media(Mid::from(track.mid.as_str()), MediaKind::Audio);
    let mut streams = NvstAudioStreams::default();
    let mut datagram = [0x80, GFN_OPUS_PAYLOAD_TYPE, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    for ssrc in 1_u32..=10_000 {
        datagram[8..12].copy_from_slice(&ssrc.to_be_bytes());
        assert!(streams.admit(&mut rtc, &track, &datagram));
        streams.finish_poll(&mut rtc);
        assert!(rtc.direct_api().stream_rx(&ssrc.into()).is_none());
    }
    assert!(streams.authenticated.is_empty());
    let mut pinned = track.clone();
    pinned.ssrc = Some(42);
    assert!(!streams.admit(&mut rtc, &pinned, &datagram));
    datagram[8..12].copy_from_slice(&42_u32.to_be_bytes());
    datagram[1] = 96;
    assert!(!streams.admit(&mut rtc, &pinned, &datagram));
    datagram[1] = GFN_RED_PAYLOAD_TYPE;
    assert!(streams.admit(&mut rtc, &pinned, &datagram));
    streams.finish_poll(&mut rtc);
}

#[test]
fn audio_ssrc_admission_recovers_after_invalid_tags_and_authenticated_source_changes() {
    fn exchange(
        from: &mut Rtc,
        to: &mut Rtc,
        now: Instant,
        inbound: bool,
        forged_ssrc: Option<u32>,
        streams: &mut NvstAudioStreams,
        received: &mut Vec<u32>,
    ) {
        loop {
            match from.poll_output().unwrap() {
                Output::Timeout(_) => {
                    if !inbound {
                        streams.finish_poll(from);
                    }
                    break;
                }
                Output::Transmit(packet) => {
                    let mut bytes = packet.contents.to_vec();
                    if inbound && looks_like_rtp(&bytes) {
                        if let Some(ssrc) = forged_ssrc {
                            bytes[8..12].copy_from_slice(&ssrc.to_be_bytes());
                        }
                        assert!(streams.admit(to, &track(), &bytes));
                    }
                    to.handle_input(Input::Receive(
                        now,
                        Receive {
                            proto: packet.proto,
                            source: packet.source,
                            destination: packet.destination,
                            contents: bytes.as_slice().try_into().unwrap(),
                        },
                    ))
                    .unwrap();
                }
                Output::Event(Event::RtpPacket(packet)) if !inbound => {
                    let ssrc = *packet.header.ssrc;
                    streams.authenticated(from, ssrc);
                    received.push(ssrc);
                }
                _ => {}
            }
        }
    }

    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let mut local = create_nvst_bundle_rtc(&socket).unwrap();
    let mut remote = create_nvst_bundle_rtc(&socket).unwrap();
    let local_candidate = Candidate::host("192.0.2.1:1000".parse().unwrap(), "udp").unwrap();
    let remote_candidate = Candidate::host("192.0.2.2:2000".parse().unwrap(), "udp").unwrap();
    local.add_local_candidate(local_candidate.clone());
    local.add_remote_candidate(remote_candidate.clone());
    remote.add_local_candidate(remote_candidate);
    remote.add_remote_candidate(local_candidate);
    let local_identity = local.direct_api().local_dtls_fingerprint().clone();
    let remote_identity = remote.direct_api().local_dtls_fingerprint().clone();
    let local_ice = local.direct_api().local_ice_credentials();
    let remote_ice = remote.direct_api().local_ice_credentials();
    local.direct_api().set_remote_fingerprint(remote_identity);
    remote.direct_api().set_remote_fingerprint(local_identity);
    local.direct_api().set_remote_ice_credentials(remote_ice);
    remote.direct_api().set_remote_ice_credentials(local_ice);
    local.direct_api().set_ice_controlling(true);
    remote.direct_api().set_ice_controlling(false);
    local.direct_api().start_dtls(true).unwrap();
    remote.direct_api().start_dtls(false).unwrap();
    let mid = Mid::from("audio");
    for rtc in [&mut local, &mut remote] {
        rtc.direct_api().declare_media(mid, MediaKind::Audio);
    }
    let mut streams = NvstAudioStreams::default();
    let mut received = Vec::new();
    let mut previous_sender = None;
    let origin = Instant::now();
    for tick in 0..1800_u64 {
        let now = origin + Duration::from_millis(tick);
        local.handle_input(Input::Timeout(now)).unwrap();
        remote.handle_input(Input::Timeout(now)).unwrap();
        let send_ssrc = match tick {
            1200 | 1250..=1349 | 1400 | 1600 => Some(1),
            1450 => Some(2),
            1500 => Some(3),
            _ => None,
        };
        if let Some(ssrc) = send_ssrc {
            if let Some(previous) = previous_sender.replace(ssrc)
                && previous != ssrc
            {
                remote.direct_api().remove_stream_tx(previous.into());
            }
            remote
                .direct_api()
                .declare_stream_tx(ssrc.into(), None, mid, None)
                .write_rtp(str0m::rtp::RtpWrite::new(
                    GFN_OPUS_PAYLOAD_TYPE.into(),
                    tick.into(),
                    (tick * 48) as u32,
                    now,
                    vec![0xf8, 0xff, 0xfe],
                ));
        }
        exchange(
            &mut local,
            &mut remote,
            now,
            false,
            None,
            &mut streams,
            &mut received,
        );
        let forged = (1250..=1349).contains(&tick).then_some(tick as u32);
        exchange(
            &mut remote,
            &mut local,
            now,
            true,
            forged,
            &mut streams,
            &mut received,
        );
        if (1251..=1350).contains(&tick) {
            assert!(local.direct_api().stream_rx(&1.into()).is_some());
            assert_eq!(streams.authenticated.len(), 1);
        }
        assert!(streams.authenticated.len() <= 2);
    }
    assert_eq!(received, [1, 1, 2, 3, 1]);
    assert_eq!(
        streams.authenticated.iter().copied().collect::<Vec<_>>(),
        [3, 1]
    );
    assert!(streams.provisional.is_none());
    for ssrc in 1250..=1349 {
        assert!(local.direct_api().stream_rx(&ssrc.into()).is_none());
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
        assert_eq!(frame.ssrc, Some(42));
    }
}

#[test]
fn late_originals_and_duplicates_never_replay_recovered_audio_or_rewind_sequence() {
    let mut receiver = NvstAudioReceiver::default();
    red_packet(&mut receiver, 10, 2_400, 1);
    let recovered = red_packet(&mut receiver, 12, 2_880, 1);
    assert_eq!(recovered.len(), 2);
    assert!(recovered.iter().all(|frame| frame.contiguous));
    assert!(recovered.iter().all(|frame| frame.ssrc == Some(42)));
    for sequence in [11, 12, 10] {
        assert!(red_packet(&mut receiver, sequence, u32::from(sequence) * 240, 1).is_empty());
    }
    let next = red_packet(&mut receiver, 13, 3_120, 1);
    assert_eq!(next.len(), 1);
    assert!(next[0].contiguous);
    assert_eq!(next[0].ssrc, Some(42));
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
    assert!(frames.iter().all(|frame| frame.ssrc == Some(42)));
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
        assert!(frames.iter().all(|frame| frame.ssrc == Some(42)));
    }
}

#[test]
fn burst_losses_stay_measurable_from_the_preserved_timestamps() {
    let mut receiver = NvstAudioReceiver::default();
    let mut sequence = 1_000_u16;
    red_packet(&mut receiver, sequence, u32::from(sequence) * 240, 0);
    for jump in [2_u16, 4, 6, 20] {
        sequence = sequence.wrapping_add(jump);
        let frames = receiver
            .depacketize(&track(), &header(sequence), Arc::from([0xf8]), 123_456)
            .unwrap();
        assert_eq!(frames.len(), 1);
        assert!(!frames[0].contiguous);
        assert_eq!(frames[0].ssrc, Some(42));
        assert_eq!(frames[0].rtp_timestamp, u64::from(sequence) * 240);
    }
    let frames = receiver
        .depacketize(&track(), &header(sequence + 1), Arc::from([0xf8]), 123_456)
        .unwrap();
    assert!(frames[0].contiguous);
    assert_eq!(frames[0].ssrc, Some(42));
}

#[test]
fn redundantly_recovered_packets_are_never_flagged_for_concealment() {
    let mut fully_recovered = NvstAudioReceiver::default();
    red_packet(&mut fully_recovered, 40, 9_600, 0);
    let frames = red_packet(&mut fully_recovered, 44, 10_560, 3);
    assert_eq!(frames.len(), 4);
    assert!(frames.iter().all(|frame| frame.contiguous));
    assert!(frames.iter().all(|frame| frame.ssrc == Some(42)));

    let mut partly_recovered = NvstAudioReceiver::default();
    red_packet(&mut partly_recovered, 40, 9_600, 0);
    let frames = red_packet(&mut partly_recovered, 44, 10_560, 2);
    assert_eq!(frames.len(), 3);
    assert_eq!(
        frames
            .iter()
            .map(|frame| (frame.contiguous, frame.ssrc))
            .collect::<Vec<_>>(),
        [(false, Some(42)), (true, Some(42)), (true, Some(42))]
    );
    assert_eq!(
        frames
            .iter()
            .map(|frame| frame.rtp_timestamp)
            .collect::<Vec<_>>(),
        [10_080, 10_320, 10_560]
    );
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
    assert_eq!(frames[0].ssrc, Some(42));
}

#[test]
fn sequence_and_timestamp_wrap_preserve_recovery_order() {
    let mut receiver = NvstAudioReceiver::default();
    red_packet(&mut receiver, u16::MAX - 1, u32::MAX - 239, 0);
    let frames = red_packet(&mut receiver, 1, 480, 2);
    assert_eq!(frames.len(), 3);
    assert!(frames.iter().all(|frame| frame.contiguous));
    assert!(frames.iter().all(|frame| frame.ssrc == Some(42)));
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
    assert!(frames.iter().all(|frame| frame.ssrc == Some(42)));
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
    assert!(frames.iter().all(|frame| frame.ssrc == Some(42)));
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
        assert_eq!(frame.ssrc, Some(42));
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
        assert_eq!(frames[0].ssrc, Some(43));
    }
}
