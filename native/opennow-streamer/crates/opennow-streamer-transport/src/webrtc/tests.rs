use super::*;
use str0m::change::SdpAnswer;
use str0m::channel::ChannelId;
use str0m::media::{Direction, Frequency, MediaKind, MediaTime};

struct Peer {
    rtc: Rtc,
    socket: UdpSocket,
    video: Mid,
    audio: Mid,
    transport: Option<TransportSession>,
    events: Receiver<TransportEvent>,
    event_sender: SyncSender<TransportEvent>,
    drain_events: bool,
    media: Receiver<EncodedMediaFrame>,
    connected: bool,
    input_ready: bool,
    received: Vec<Vec<u8>>,
    keyframe_requests: usize,
    input_channel: Option<ChannelId>,
    client_address: SocketAddr,
    answer_sdp: String,
    rtp_transmits: usize,
    drop_rtp_number: Option<usize>,
    dropped_rtp: usize,
}

impl Peer {
    fn connect(capacity: usize) -> Self {
        Self::connect_with_profile(capacity, None)
    }

    fn connect_with_profile(capacity: usize, profile: Option<u32>) -> Self {
        Self::connect_with_extra_media(capacity, profile, None)
    }

    fn connect_with_extra_media(
        capacity: usize,
        profile: Option<u32>,
        extra: Option<(MediaKind, Direction)>,
    ) -> Self {
        Self::connect_with_ice_lite_peer(capacity, profile, extra, false)
    }

    fn connect_with_ice_lite_peer(
        capacity: usize,
        profile: Option<u32>,
        extra: Option<(MediaKind, Direction)>,
        passive_ice_lite: bool,
    ) -> Self {
        install_crypto();
        let socket = bind_routed_socket("127.0.0.1".parse().unwrap()).unwrap();
        socket.set_nonblocking(true).unwrap();
        let address = socket.local_addr().unwrap();
        let mut config = RtcConfig::new().clear_codecs().enable_opus(true);
        let pcmu_microphone = extra == Some((MediaKind::Audio, Direction::RecvOnly));
        if pcmu_microphone {
            config.codec_config().enable_pcmu(true);
        }
        if let Some(profile) = profile {
            config
                .codec_config()
                .add_h264(96.into(), Some(97.into()), true, profile);
        } else {
            config = config.enable_h264(true);
        }
        let mut rtc = config.set_ice_lite(passive_ice_lite).build(Instant::now());
        if passive_ice_lite {
            rtc.direct_api().start_dtls(false).unwrap();
        }
        rtc.add_local_candidate(Candidate::host(address, "udp").unwrap());
        let mut changes = rtc.sdp_api();
        let extra_mid =
            extra.map(|(kind, direction)| changes.add_media(kind, direction, None, None, None));
        let video = changes.add_media(MediaKind::Video, Direction::SendOnly, None, None, None);
        let audio = changes.add_media(MediaKind::Audio, Direction::SendOnly, None, None, None);
        changes.add_channel("server-bootstrap".to_owned());
        let (offer, pending) = changes.apply().unwrap();
        let offer_sdp = if pcmu_microphone {
            let mut offer = (*offer).clone();
            let microphone = &mut offer.media_lines[0];
            microphone.pts = vec![0.into()];
            microphone.attrs.retain(|attribute| {
                let line = attribute.to_string();
                !line.starts_with("a=rtpmap:")
                    && !line.starts_with("a=fmtp:")
                    && !line.starts_with("a=rtcp-fb:")
            });
            offer.to_string()
        } else {
            offer.to_sdp_string()
        };
        let session: Session = serde_json::from_value(serde_json::json!({
            "sessionId": "local-peer",
            "serverIp": "127.0.0.1",
            "mediaConnectionInfo": {"ip":"127.0.0.1", "port":address.port(), "usage":15},
            "iceServers": [{"urls":["turn:unused.invalid"],"username":"test","credential":"test"}]
        }))
        .unwrap();
        let (events, event_rx) = mpsc::sync_channel(8);
        let (media, media_rx) = mpsc::sync_channel(capacity);
        let negotiated = negotiate(
            &offer_sdp,
            &session,
            NegotiatedVideoCodec::H264,
            300,
            events.clone(),
            media,
        )
        .unwrap();
        assert!(negotiated.answer_sdp.contains("H264/90000"));
        assert_eq!(negotiated.video_mid, video);
        assert!(negotiated.answer_sdp.contains("opus/48000/2"));
        assert!(!negotiated.answer_sdp.contains("VP8/90000"));
        if passive_ice_lite {
            assert!(negotiated.answer_sdp.contains("a=setup:active"));
        }
        rtc.sdp_api()
            .accept_answer(
                pending,
                SdpAnswer::from_sdp_string(&negotiated.answer_sdp).unwrap(),
            )
            .unwrap();
        if let Some(mid) = extra_mid {
            assert_eq!(rtc.media(mid).unwrap().direction(), Direction::Inactive);
            if pcmu_microphone {
                assert!(rtc.media(mid).unwrap().disabled());
            }
            assert_eq!(
                negotiated
                    .answer_sdp
                    .lines()
                    .filter(|line| line.starts_with("m="))
                    .count(),
                4
            );
        }
        let mut peer = Self {
            rtc,
            socket,
            video,
            audio,
            transport: Some(negotiated.session),
            events: event_rx,
            event_sender: events,
            drain_events: true,
            media: media_rx,
            connected: false,
            input_ready: false,
            received: Vec::new(),
            keyframe_requests: 0,
            input_channel: None,
            client_address: parse_candidate(&negotiated.local_candidate.candidate, None)
                .unwrap()
                .unwrap()
                .addr(),
            answer_sdp: negotiated.answer_sdp,
            rtp_transmits: 0,
            drop_rtp_number: None,
            dropped_rtp: 0,
        };
        peer.until(|peer| peer.connected && peer.input_ready);
        peer
    }

    fn control(&self) -> TransportControl {
        self.transport.as_ref().unwrap().control()
    }

    fn tick(&mut self) {
        self.rtc
            .handle_input(Input::Timeout(Instant::now()))
            .unwrap();
        loop {
            match self.rtc.poll_output().unwrap() {
                Output::Timeout(_) => break,
                Output::Transmit(packet) => {
                    if packet.contents.len() >= 12
                        && packet.contents[0] & 0xc0 == 0x80
                        && !(192..=223).contains(&packet.contents[1])
                    {
                        self.rtp_transmits += 1;
                        if self.drop_rtp_number == Some(self.rtp_transmits) {
                            self.drop_rtp_number = None;
                            self.dropped_rtp += 1;
                            continue;
                        }
                    }
                    match self.socket.send_to(&packet.contents, packet.destination) {
                        Ok(_) => {}
                        Err(error) if transient_udp_error(&error) => {}
                        Err(error) => panic!("peer send: {error}"),
                    }
                }
                Output::Event(Event::ChannelOpen(id, label)) if label == "input_channel_v1" => {
                    self.input_channel = Some(id);
                    assert!(
                        self.rtc
                            .channel(id)
                            .unwrap()
                            .write(true, &[0x0e, 0x02, 3, 0])
                            .unwrap()
                    );
                }
                Output::Event(Event::ChannelData(data)) => self.received.push(data.data),
                Output::Event(Event::KeyframeRequest(_)) => self.keyframe_requests += 1,
                _ => {}
            }
        }
        let mut buffer = [0; 65_536];
        loop {
            match self.socket.recv_from(&mut buffer) {
                Ok((length, source)) => {
                    let contents = (&buffer[..length]).try_into().unwrap();
                    self.rtc
                        .handle_input(Input::Receive(
                            Instant::now(),
                            Receive {
                                proto: Protocol::Udp,
                                source,
                                destination: self.socket.local_addr().unwrap(),
                                contents,
                            },
                        ))
                        .unwrap();
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(error) if transient_udp_error(&error) => continue,
                Err(error) => panic!("peer receive: {error}"),
            }
        }
        if self.drain_events {
            for event in self.events.try_iter() {
                match event {
                    TransportEvent::Connected => self.connected = true,
                    TransportEvent::InputReady(version) => {
                        assert_eq!(version, 3);
                        self.input_ready = true;
                    }
                    TransportEvent::Disconnected(reason) => {
                        panic!("unexpected disconnect: {reason}")
                    }
                    _ => {}
                }
            }
        }
        thread::sleep(Duration::from_millis(1));
    }

    fn until(&mut self, condition: impl Fn(&Self) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !condition(self) {
            assert!(Instant::now() < deadline, "local peer timed out");
            self.tick();
        }
    }

    fn pump(&mut self) {
        for _ in 0..30 {
            self.tick();
        }
    }

    fn write(&mut self, video: bool, timestamp: u64, data: &[u8]) -> u32 {
        let mid = if video { self.video } else { self.audio };
        let codec = if video { Codec::H264 } else { Codec::Opus };
        let pt = self
            .rtc
            .writer(mid)
            .unwrap()
            .payload_params()
            .find(|params| params.spec().codec == codec)
            .unwrap()
            .pt();
        let time = if video {
            MediaTime::from_90khz(timestamp)
        } else {
            MediaTime::new(timestamp, Frequency::FORTY_EIGHT_KHZ)
        };
        self.rtc
            .writer(mid)
            .unwrap()
            .write(pt, Instant::now(), time, data.to_vec())
            .unwrap();
        *self
            .rtc
            .direct_api()
            .stream_tx_by_mid(mid, None)
            .unwrap()
            .ssrc()
    }

    fn frame(&mut self) -> EncodedMediaFrame {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok(frame) = self.media.try_recv() {
                return frame;
            }
            assert!(Instant::now() < deadline, "no compressed media delivered");
            self.tick();
        }
    }
}

#[test]
fn refuses_unresolved_hostname_without_dns() {
    assert!(matches!(
        resolve_host("not-a-real-host.invalid"),
        Err(TransportError::Endpoint(_))
    ));
    assert_eq!(
        resolve_host("192-0-2-10.example.com").unwrap(),
        "192.0.2.10".parse::<IpAddr>().unwrap()
    );
}

#[test]
fn local_peer_delivers_h264_opus_and_versioned_input() {
    let mut peer = Peer::connect(8);
    let video = [0, 0, 0, 1, 0x65, 0x88, 0x84, 0x21];
    let video_ssrc = peer.write(true, 900_123, &video);
    let frame = peer.frame();
    assert_eq!(frame.codec, "H264");
    assert_eq!(frame.payload.as_ref(), video);
    assert_eq!(frame.frame_index, None);
    assert_eq!(frame.rtp_timestamp, 900_123);
    assert_eq!(frame.clock_rate_hz, 90_000);
    assert_eq!(frame.ssrc, Some(video_ssrc));
    assert!(frame.keyframe);
    let opus = [0xf8, 0xff, 0xfe];
    let audio_ssrc = peer.write(false, 48_321, &opus);
    let frame = peer.frame();
    assert_eq!(frame.codec, "Opus");
    assert_eq!(frame.payload.as_ref(), opus);
    assert_eq!(frame.rtp_timestamp, 48_321);
    assert_eq!(frame.clock_rate_hz, 48_000);
    assert_eq!(frame.channels, Some(2));
    assert_eq!(frame.ssrc, Some(audio_ssrc));
    let mut key = vec![0; 18];
    key[..4].copy_from_slice(&3_u32.to_le_bytes());
    key[4..6].copy_from_slice(&65_u16.to_be_bytes());
    peer.control().send_input(key.clone(), false).unwrap();
    peer.until(|peer| {
        peer.received
            .iter()
            .any(|packet| packet.first() == Some(&0x23))
    });
    let packet = peer
        .received
        .iter()
        .find(|packet| packet.first() == Some(&0x23))
        .unwrap();
    assert_eq!(packet[9], 0x22);
    assert_eq!(&packet[10..], key);
    let send_timestamp = u64::from_be_bytes(packet[1..9].try_into().unwrap());
    assert!(send_timestamp < 5_000_000);
    assert!(
        !peer
            .received
            .iter()
            .any(|packet| packet == &[0x0e, 0x02, 3, 0])
    );
    let started = Instant::now();
    peer.transport.take().unwrap().stop();
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn media_overload_drops_delta_frames_until_reference_and_requests_pli() {
    let mut peer = Peer::connect(1);
    let key = [0, 0, 0, 1, 0x65, 0x88, 0x84, 0x21];
    let delta = [0, 0, 0, 1, 0x41, 0x88, 0x84, 0x21];
    peer.write(true, 90_000, &key);
    peer.pump();
    peer.write(true, 93_000, &delta);
    peer.until(|peer| peer.keyframe_requests > 0);
    assert_eq!(peer.frame().rtp_timestamp, 90_000);
    peer.write(true, 96_000, &delta);
    peer.pump();
    assert!(matches!(
        peer.media.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    peer.write(true, 99_000, &key);
    let recovered = peer.frame();
    assert_eq!(recovered.rtp_timestamp, 99_000);
    assert!(recovered.keyframe);
    assert!(!recovered.contiguous);
    peer.write(true, 102_000, &delta);
    assert_eq!(peer.frame().rtp_timestamp, 102_000);
}

#[test]
fn command_bounds_and_stop_do_not_depend_on_queue_space() {
    let (commands, receiver) = mpsc::sync_channel(1);
    let control = TransportControl {
        commands,
        cancelled: Arc::new(AtomicBool::new(false)),
        input_ready: Arc::new(AtomicBool::new(true)),
    };
    control.request_keyframe("video").unwrap();
    assert!(matches!(
        control.request_keyframe("video"),
        Err(TransportError::Backpressured)
    ));
    assert!(matches!(
        control.send_input(vec![0; MAX_INPUT_BYTES + 1], false),
        Err(TransportError::OversizedCommand)
    ));
    control.stop();
    assert!(control.cancelled.load(Ordering::Acquire));
    assert!(!control.input_ready.load(Ordering::Acquire));
    assert!(matches!(
        control.request_keyframe("video"),
        Err(TransportError::Closed)
    ));
    assert!(receiver.try_recv().is_ok());
}

#[test]
fn candidate_normalization_preserves_relay_addresses_and_uses_usage_15() {
    let endpoint: SocketAddr = "192.0.2.42:4444".parse().unwrap();
    let host = parse_candidate(
        "candidate:1 1 udp 2130706431 127.0.0.1 9 typ host",
        Some(endpoint),
    )
    .unwrap()
    .unwrap();
    assert!(host.to_sdp_string().contains("192.0.2.42 4444 typ host"));
    let relay = parse_candidate(
        "candidate:2 1 udp 16777215 192.0.2.50 5000 typ relay raddr 192.0.2.51 rport 5001",
        Some(endpoint),
    )
    .unwrap()
    .unwrap();
    assert!(relay.to_sdp_string().contains("192.0.2.50 5000 typ relay"));
    let session: Session = serde_json::from_value(serde_json::json!({
        "sessionId":"test", "serverIp":"unresolved.invalid", "mediaConnectionInfo": {"ip":"192.0.2.42","port":4444,"usage":15}
    })).unwrap();
    assert_eq!(media_endpoint(&session).unwrap(), Some(endpoint));
}

#[test]
fn candidates_keep_public_endpoints_and_ignore_tcp_in_mixed_offers() {
    let endpoint = "192.0.2.42:4444".parse().unwrap();
    let public = "candidate:1 1 udp 2130706431 203.0.113.8 5555 typ host";
    assert!(
        parse_candidate(public, Some(endpoint))
            .unwrap()
            .unwrap()
            .to_sdp_string()
            .contains("203.0.113.8 5555 typ host")
    );
    let tcp = "candidate:2 1 tcp 2130706431 203.0.113.8 9 typ host tcptype active";
    assert!(parse_candidate(tcp, Some(endpoint)).unwrap().is_none());
    let offer = format!("a={tcp}\r\na={public}\r\n");
    let normalized = normalize_offer(&offer, Some(endpoint)).unwrap();
    assert!(!normalized.contains(" tcp "));
    assert!(normalized.contains("203.0.113.8 5555 typ host"));
    assert!(matches!(
        normalize_offer(
            "a=candidate:2 1 udp 16777215 192.0.2.50 5000 typ relay\r\n",
            None
        ),
        Err(TransportError::RelayUnsupported)
    ));
}

#[test]
fn local_candidate_targets_negotiated_mid_instead_of_assuming_zero() {
    let sdp = "a=group:BUNDLE audio-main video-main\r\nm=video 9 UDP/TLS/RTP/SAVPF 96\r\na=mid:video-main\r\nm=audio 9 UDP/TLS/RTP/SAVPF 111\r\na=mid:audio-main\r\n";
    assert_eq!(candidate_target(sdp).unwrap(), ("audio-main".to_owned(), 1));
    let rejected = "m=video 0 UDP/TLS/RTP/SAVPF 96\r\na=mid:rejected\r\nm=audio 9 UDP/TLS/RTP/SAVPF 111\r\na=mid:active\r\n";
    assert_eq!(
        candidate_target(rejected).unwrap(),
        ("active".to_owned(), 1)
    );
}

#[test]
fn relay_only_policy_is_explicitly_unsupported() {
    let session: Session = serde_json::from_value(serde_json::json!({
        "sessionId":"test", "serverIp":"127.0.0.1", "iceTransportPolicy":"relay"
    }))
    .unwrap();
    let (events, _) = mpsc::sync_channel(1);
    let (media, _) = mpsc::sync_channel(1);
    assert!(matches!(
        negotiate("", &session, NegotiatedVideoCodec::H264, 300, events, media),
        Err(TransportError::RelayUnsupported)
    ));
}

#[test]
fn full_event_queue_cannot_block_session_drop_or_accept_new_input() {
    let mut peer = Peer::connect(1);
    peer.drain_events = false;
    for _ in 0..8 {
        peer.event_sender
            .try_send(TransportEvent::Log("full".to_owned()))
            .unwrap();
    }
    peer.rtc
        .direct_api()
        .close_data_channel(peer.input_channel.unwrap());
    peer.pump();
    let control = peer.control();
    assert!(!control.input_ready.load(Ordering::Acquire));
    let started = Instant::now();
    drop(peer.transport.take().unwrap());
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(matches!(
        control.request_keyframe("video"),
        Err(TransportError::Closed)
    ));
}

#[test]
fn terminal_event_survives_event_backpressure_when_consumer_resumes() {
    let mut peer = Peer::connect(1);
    peer.drain_events = false;
    for _ in 0..8 {
        peer.event_sender
            .try_send(TransportEvent::Log("full".to_owned()))
            .unwrap();
    }
    peer.rtc
        .direct_api()
        .close_data_channel(peer.input_channel.unwrap());
    peer.pump();
    for _ in 0..8 {
        assert!(matches!(
            peer.events.try_recv().unwrap(),
            TransportEvent::Log(_)
        ));
    }
    let terminal = peer.events.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(matches!(terminal, TransportEvent::Disconnected(_)));
}

#[test]
fn sender_rtp_timestamp_wraps_without_becoming_a_local_frame_counter() {
    let mut peer = Peer::connect(4);
    let key = [0, 0, 0, 1, 0x65, 0x88, 0x84, 0x21];
    let delta = [0, 0, 0, 1, 0x41, 0x88, 0x84, 0x21];
    let first = u64::from(u32::MAX) - 2999;
    peer.write(true, first, &key);
    assert_eq!(peer.frame().rtp_timestamp, first);
    peer.write(true, first + 3000, &delta);
    assert_eq!(peer.frame().rtp_timestamp, 0);
    peer.write(true, first + 6000, &delta);
    assert_eq!(peer.frame().rtp_timestamp, 3000);
}

#[test]
fn audio_rtp_wrap_preserves_wire_timestamps_accepted_by_native_audio() {
    let mut peer = Peer::connect(4);
    let opus = [0xf8, 0xff, 0xfe];
    let first = u64::from(u32::MAX) - 959;
    let source = peer.write(false, first, &opus);
    let frame = peer.frame();
    assert_eq!(frame.rtp_timestamp, first);
    assert_eq!(frame.ssrc, Some(source));
    for (sent, expected) in [(first + 960, 0_u32), (first + 1920, 960)] {
        peer.write(false, sent, &opus);
        let frame = peer.frame();
        assert_eq!(u32::try_from(frame.rtp_timestamp).unwrap(), expected);
        assert_eq!(frame.clock_rate_hz, 48_000);
        assert_eq!(frame.ssrc, Some(source));
    }
}

#[test]
fn unsolicited_udp_cannot_disconnect_an_authenticated_media_peer() {
    let mut peer = Peer::connect(4);
    let outsider = UdpSocket::bind("127.0.0.1:0").unwrap();
    for packet in [
        &[0x80, 0x66, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1][..],
        &[0xff, 1, 2][..],
    ] {
        outsider.send_to(packet, peer.client_address).unwrap();
    }
    peer.pump();
    let key = [0, 0, 0, 1, 0x65, 0x88, 0x84, 0x21];
    peer.write(true, 90_000, &key);
    assert_eq!(peer.frame().payload.as_ref(), key);
}

#[test]
fn flush_sends_queued_neutral_input_before_session_teardown() {
    let mut peer = Peer::connect(1);
    let control = peer.control();
    let mut down = vec![0; 18];
    down[..4].copy_from_slice(&3_u32.to_le_bytes());
    down[4..6].copy_from_slice(&65_u16.to_be_bytes());
    for _ in 0..32 {
        control.send_input(down.clone(), false).unwrap();
    }
    let mut key_up = down.clone();
    key_up[..4].copy_from_slice(&4_u32.to_le_bytes());
    let mut mouse_up = vec![0; 18];
    mouse_up[..4].copy_from_slice(&9_u32.to_le_bytes());
    mouse_up[4] = 1;
    let mut gamepad = vec![0; 38];
    gamepad[..4].copy_from_slice(&12_u32.to_le_bytes());
    for body in [&key_up, &mouse_up, &gamepad] {
        control.send_input(body.clone(), false).unwrap();
    }
    control.flush_input(Duration::from_millis(150)).unwrap();
    peer.drain_events = false;
    peer.transport.take().unwrap().stop();
    peer.until(|peer| {
        [&key_up, &mouse_up, &gamepad]
            .iter()
            .all(|body| peer.received.iter().any(|packet| packet.ends_with(body)))
    });
}

#[test]
fn flush_is_bounded_when_command_consumer_is_stalled_or_cancelled() {
    let (commands, _receiver) = mpsc::sync_channel(1);
    let control = TransportControl {
        commands,
        cancelled: Arc::new(AtomicBool::new(false)),
        input_ready: Arc::new(AtomicBool::new(true)),
    };
    control.request_keyframe("video").unwrap();
    let start = Instant::now();
    assert!(matches!(
        control.flush_input(Duration::from_secs(30)),
        Err(TransportError::FlushTimeout)
    ));
    assert!(start.elapsed() < Duration::from_millis(500));
    control.stop();
    let start = Instant::now();
    assert!(matches!(
        control.flush_input(Duration::from_secs(30)),
        Err(TransportError::Closed)
    ));
    assert!(start.elapsed() < Duration::from_millis(100));
}

#[test]
fn full_event_consumer_cannot_block_input_flush() {
    let mut peer = Peer::connect(1);
    peer.drain_events = false;
    for _ in 0..8 {
        peer.event_sender
            .try_send(TransportEvent::Log("full".to_owned()))
            .unwrap();
    }
    let start = Instant::now();
    peer.control()
        .flush_input(Duration::from_millis(150))
        .unwrap();
    assert!(start.elapsed() < Duration::from_millis(500));
}

#[test]
fn cancellation_interrupts_a_flush_waiting_for_its_local_acknowledgement() {
    let (commands, receiver) = mpsc::sync_channel(1);
    let control = TransportControl {
        commands,
        cancelled: Arc::new(AtomicBool::new(false)),
        input_ready: Arc::new(AtomicBool::new(true)),
    };
    let waiting = control.clone();
    let worker = thread::spawn(move || waiting.flush_input(Duration::from_secs(30)));
    let barrier = receiver.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(matches!(barrier, TransportCommand::FlushInput(_)));
    let start = Instant::now();
    control.stop();
    assert!(matches!(
        worker.join().unwrap(),
        Err(TransportError::Closed)
    ));
    assert!(start.elapsed() < Duration::from_millis(100));
    drop(barrier);
}

#[test]
fn end_of_candidates_is_a_nonfatal_noop() {
    for value in ["", "  ", "end-of-candidates", "a=end-of-candidates\r\n"] {
        assert!(parse_candidate(value, None).unwrap().is_none());
    }
    let peer = Peer::connect(1);
    peer.transport
        .as_ref()
        .unwrap()
        .add_remote_candidate(&IceCandidate {
            candidate: String::new(),
            sdp_mid: None,
            sdp_m_line_index: None,
            username_fragment: None,
        })
        .unwrap();
    peer.control()
        .flush_input(Duration::from_millis(150))
        .unwrap();
}

#[test]
fn icmp_and_interruption_are_transient_but_device_errors_are_not() {
    for kind in [
        ErrorKind::ConnectionReset,
        ErrorKind::ConnectionRefused,
        ErrorKind::Interrupted,
    ] {
        assert!(transient_udp_error(&std::io::Error::from(kind)));
    }
    assert!(!transient_udp_error(&std::io::Error::from(
        ErrorKind::PermissionDenied
    )));
}

#[cfg(windows)]
#[test]
fn windows_closed_udp_port_does_not_poison_later_receives() {
    let closed = UdpSocket::bind("127.0.0.1:0").unwrap();
    let closed_address = closed.local_addr().unwrap();
    drop(closed);
    let socket = bind_routed_socket("127.0.0.1".parse().unwrap()).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    socket.send_to(b"probe", closed_address).unwrap();
    thread::sleep(Duration::from_millis(50));
    let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
    sender
        .send_to(b"valid", socket.local_addr().unwrap())
        .unwrap();
    let mut bytes = [0; 32];
    let (length, _) = socket.recv_from(&mut bytes).unwrap();
    assert_eq!(&bytes[..length], b"valid");
}

#[test]
fn ice_lite_server_waiting_for_client_hello_connects_and_delivers_media() {
    let mut peer = Peer::connect_with_ice_lite_peer(4, None, None, true);
    let video = [0, 0, 0, 1, 0x65, 0x88, 0x84, 0x21];
    peer.write(true, 90_000, &video);
    assert_eq!(peer.frame().payload.as_ref(), video);
    peer.write(false, 48_000, &[0xf8, 0xff, 0xfe]);
    assert_eq!(peer.frame().codec, "Opus");
    let mut key = vec![0; 18];
    key[..4].copy_from_slice(&3_u32.to_le_bytes());
    peer.control().send_input(key.clone(), false).unwrap();
    peer.until(|peer| {
        peer.received
            .iter()
            .any(|packet| packet.get(10..) == Some(key.as_slice()))
    });
}

#[test]
fn extra_microphone_section_does_not_block_media_or_input() {
    let mut peer =
        Peer::connect_with_extra_media(4, None, Some((MediaKind::Audio, Direction::RecvOnly)));
    let video = [0, 0, 0, 1, 0x65, 0x88, 0x84, 0x21];
    peer.write(true, 90_000, &video);
    assert_eq!(peer.frame().payload.as_ref(), video);
    peer.write(false, 48_000, &[0xf8, 0xff, 0xfe]);
    assert_eq!(peer.frame().codec, "Opus");
    let mut key = vec![0; 18];
    key[..4].copy_from_slice(&3_u32.to_le_bytes());
    peer.control().send_input(key.clone(), false).unwrap();
    peer.until(|peer| {
        peer.received
            .iter()
            .any(|packet| packet.get(10..) == Some(key.as_slice()))
    });
}

#[test]
fn inactive_video_section_does_not_replace_the_receiving_video_mid() {
    let mut peer =
        Peer::connect_with_extra_media(4, None, Some((MediaKind::Video, Direction::Inactive)));
    let video = [0, 0, 0, 1, 0x65, 0x88, 0x84, 0x21];
    peer.write(true, 90_000, &video);
    assert_eq!(peer.frame().payload.as_ref(), video);
}

#[test]
fn extra_incoming_tracks_are_rejected_by_role_not_section_count() {
    install_crypto();
    for kind in [MediaKind::Video, MediaKind::Audio] {
        let mut remote = RtcConfig::new()
            .clear_codecs()
            .enable_h264(true)
            .enable_opus(true)
            .build(Instant::now());
        remote.add_local_candidate(
            Candidate::host("127.0.0.1:49153".parse().unwrap(), "udp").unwrap(),
        );
        let mut changes = remote.sdp_api();
        changes.add_media(MediaKind::Video, Direction::SendOnly, None, None, None);
        changes.add_media(MediaKind::Audio, Direction::SendOnly, None, None, None);
        changes.add_media(kind, Direction::SendOnly, None, None, None);
        changes.add_channel("data".to_owned());
        let (offer, _) = changes.apply().unwrap();
        let session: Session = serde_json::from_value(
            serde_json::json!({"sessionId":"fixture","serverIp":"127.0.0.1"}),
        )
        .unwrap();
        let (events, _) = mpsc::sync_channel(8);
        let (media, _) = mpsc::sync_channel(8);
        let error = negotiate(
            &offer.to_sdp_string(),
            &session,
            NegotiatedVideoCodec::H264,
            300,
            events,
            media,
        )
        .err()
        .unwrap();
        let expected = if kind == MediaKind::Video {
            "multiple incoming video"
        } else {
            "multiple incoming audio"
        };
        assert!(error.to_string().contains(expected), "{error}");
    }
}

#[test]
fn bidirectional_playback_offer_is_answered_receive_only() {
    install_crypto();
    for (offered_role, answer_role) in [
        ("actpass", "active"),
        ("active", "passive"),
        ("passive", "active"),
    ] {
        let mut remote = RtcConfig::new()
            .clear_codecs()
            .enable_h264(true)
            .enable_opus(true)
            .build(Instant::now());
        remote.add_local_candidate(
            Candidate::host("127.0.0.1:49153".parse().unwrap(), "udp").unwrap(),
        );
        let mut changes = remote.sdp_api();
        let video = changes.add_media(MediaKind::Video, Direction::SendRecv, None, None, None);
        let audio = changes.add_media(MediaKind::Audio, Direction::SendRecv, None, None, None);
        changes.add_channel("data".to_owned());
        let (offer, pending) = changes.apply().unwrap();
        let session: Session = serde_json::from_value(
            serde_json::json!({"sessionId":"fixture","serverIp":"127.0.0.1"}),
        )
        .unwrap();
        let (events, _events) = mpsc::sync_channel(8);
        let (media, _media) = mpsc::sync_channel(8);
        let negotiated = negotiate(
            &offer
                .to_sdp_string()
                .replace("a=setup:actpass", &format!("a=setup:{offered_role}")),
            &session,
            NegotiatedVideoCodec::H264,
            300,
            events,
            media,
        )
        .unwrap();
        let roles: Vec<_> = negotiated
            .answer_sdp
            .lines()
            .filter_map(|line| line.strip_prefix("a=setup:"))
            .collect();
        assert!(!roles.is_empty());
        assert!(roles.iter().all(|role| *role == answer_role));
        remote
            .sdp_api()
            .accept_answer(
                pending,
                SdpAnswer::from_sdp_string(&negotiated.answer_sdp).unwrap(),
            )
            .unwrap();
        assert_eq!(
            remote.media(video).unwrap().direction(),
            Direction::SendOnly
        );
        assert_eq!(
            remote.media(audio).unwrap().direction(),
            Direction::SendOnly
        );
        negotiated.session.stop();
    }
}

#[test]
fn media_section_limit_remains_bounded() {
    let section = "m=audio 9 UDP/TLS/RTP/SAVPF 111\r\na=inactive\r\n";
    assert!(normalize_offer(&section.repeat(MAX_MEDIA_SECTIONS), None).is_ok());
    assert!(
        normalize_offer(&section.repeat(MAX_MEDIA_SECTIONS + 1), None)
            .unwrap_err()
            .to_string()
            .contains("too many SDP media sections")
    );
}

#[test]
fn high_profile_h264_offer_negotiates_and_delivers_fragmented_reference() {
    let mut peer = Peer::connect_with_profile(4, Some(0x640032));
    assert!(
        peer.answer_sdp.contains("profile-level-id=64002a"),
        "answered fmtp: {:?}",
        peer.answer_sdp
            .lines()
            .filter(|line| line.starts_with("a=fmtp:"))
            .collect::<Vec<_>>()
    );
    assert!(
        peer.answer_sdp
            .lines()
            .any(|line| line.starts_with("a=fmtp:")
                && line.contains("profile-level-id=64002a")
                && line.contains("packetization-mode=1"))
    );
    let mut access_unit = vec![
        0, 0, 0, 1, 0x67, 0x64, 0, 0x2a, 0xac, 0xd9, 0x40, 0, 0, 0, 1, 0x68, 0xeb, 0xec, 0xb2,
        0x2c, 0, 0, 0, 1, 0x65, 0x88, 0x84, 0x21,
    ];
    access_unit.extend(std::iter::repeat_n(0x55, 6000));
    peer.write(true, 90_000, &access_unit);
    let frame = peer.frame();
    assert!(frame.keyframe);
    assert_eq!(frame.payload.as_ref(), access_unit);
    assert!(peer.rtp_transmits > 2);
}

#[test]
fn lost_idr_fragment_is_retransmitted_before_complete_reference_delivery() {
    let mut peer = Peer::connect(4);
    let mut access_unit = vec![
        0, 0, 0, 1, 0x67, 0x42, 0, 0x1f, 0xac, 0xd9, 0x40, 0, 0, 0, 1, 0x68, 0xce, 0x06, 0xe2, 0,
        0, 0, 1, 0x65, 0x88, 0x84, 0x21,
    ];
    access_unit.extend(std::iter::repeat_n(0x55, 6000));
    peer.drop_rtp_number = Some(3);
    peer.write(true, 90_000, &access_unit);
    let frame = peer.frame();
    assert_eq!(peer.dropped_rtp, 1);
    assert!(frame.keyframe);
    assert_eq!(frame.payload.as_ref(), access_unit);
    assert!(peer.media.try_recv().is_err());
}

#[test]
fn historical_usage_14_media_endpoint_remains_authoritative() {
    let session: Session = serde_json::from_value(serde_json::json!({
        "sessionId":"test", "serverIp":"unresolved.invalid", "mediaConnectionInfo": {"ip":"192.0.2.42","port":4444,"usage":14}
    })).unwrap();
    assert_eq!(
        media_endpoint(&session).unwrap(),
        Some("192.0.2.42:4444".parse().unwrap())
    );
}
