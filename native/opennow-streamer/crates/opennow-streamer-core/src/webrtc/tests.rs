use super::*;

#[test]
fn video_progress_requires_presentation_and_does_not_accept_audio_as_progress() {
    let now = Instant::now();
    let mut progress = Progress::new(now, None);
    assert!(
        !progress
            .poll(now + STARTUP_TIMEOUT - Duration::from_millis(1))
            .unwrap()
    );
    assert_eq!(
        progress.poll(now + STARTUP_TIMEOUT).unwrap_err().code,
        "webrtc-video-startup-timeout"
    );
}

#[test]
fn stalled_decoder_gets_one_keyframe_then_terminal_error() {
    let now = Instant::now();
    let mut progress = Progress::new(now, Some(0));
    progress.presented(now);
    assert!(progress.poll(now + VIDEO_TIMEOUT).unwrap());
    assert!(
        !progress
            .poll(now + VIDEO_TIMEOUT + Duration::from_millis(1))
            .unwrap()
    );
    assert_eq!(
        progress
            .poll(now + VIDEO_TIMEOUT + RECOVERY_GRACE)
            .unwrap_err()
            .code,
        "webrtc-video-stalled"
    );
    progress.decoded(now + VIDEO_TIMEOUT + RECOVERY_GRACE);
    assert!(!progress.poll(now + VIDEO_TIMEOUT + RECOVERY_GRACE).unwrap());
}

#[test]
fn published_decoded_output_remains_healthy_for_ninety_seconds() {
    let now = Instant::now();
    let mut progress = Progress::new(now, Some(1000));
    progress.observe_feedback(&MediaFeedback::PlaybackStarted { backend: "fixture" }, now);
    for second in 1..=90 {
        let at = now + Duration::from_secs(second);
        progress.observe_published(Some(1000 + second * 60), at);
        assert!(!progress.poll(at).unwrap());
    }
    assert!(progress.poll(now + Duration::from_secs(98)).unwrap());
    assert_eq!(
        progress
            .poll(now + Duration::from_secs(102))
            .unwrap_err()
            .code,
        "webrtc-video-stalled"
    );
}

#[test]
fn published_output_does_not_replace_first_presentation_or_count_prior_sessions() {
    let now = Instant::now();
    let mut progress = Progress::new(now, Some(1000));
    progress.observe_published(Some(1000), now + Duration::from_secs(1));
    assert!(progress.last_decoded.is_none());
    for second in 2..=19 {
        progress.observe_published(Some(1000 + second), now + Duration::from_secs(second));
    }
    assert!(!progress.presented);
    assert_eq!(
        progress.poll(now + STARTUP_TIMEOUT).unwrap_err().code,
        "webrtc-video-startup-timeout"
    );
}

#[test]
fn accepted_compressed_frames_do_not_refresh_observable_decoded_health() {
    let now = Instant::now();
    let mut progress = Progress::new(now, Some(100));
    progress.observe_feedback(&MediaFeedback::PlaybackStarted { backend: "fixture" }, now);
    let mut requests = 0;
    for second in 1..=12 {
        let at = now + Duration::from_secs(second);
        progress.observe_feedback(
            &MediaFeedback::VideoFrameAccepted {
                frame_index: None,
                timestamp: second * 90_000,
                bytes: 1000,
                keyframe: second == 1,
            },
            at,
        );
        progress.observe_published(Some(100), at);
        let result = progress.poll(at);
        if second == 12 {
            assert_eq!(result.unwrap_err().code, "webrtc-video-stalled");
        } else if result.unwrap() {
            requests += 1;
            assert_eq!(second, 8);
        }
    }
    assert_eq!(requests, 1);
}

#[test]
fn missing_output_observer_does_not_invent_a_decode_stall_after_presentation() {
    let now = Instant::now();
    let mut progress = Progress::new(now, None);
    progress.observe_feedback(&MediaFeedback::PlaybackStarted { backend: "fixture" }, now);
    for second in 1..=90 {
        let at = now + Duration::from_secs(second);
        progress.observe_published(None, at);
        assert!(!progress.poll(at).unwrap());
    }
}

#[test]
fn explicit_decode_timings_drive_health_without_a_graphics_publisher() {
    let now = Instant::now();
    let mut progress = Progress::new(now, None);
    progress.observe_feedback(&MediaFeedback::PlaybackStarted { backend: "fixture" }, now);
    for second in 1..=90 {
        let at = now + Duration::from_secs(second);
        let timings = DecodeTimingsReport {
            call: None,
            residence: None,
            call_window_samples: 0,
            residence_window_samples: 0,
            submissions_total: second * 60,
            outputs_total: second * 60,
            output_calls_total: second * 60,
            last_submission_at: Some(at),
            last_output_at: Some(at),
            in_flight: 0,
            oldest_in_flight_at: None,
            epoch: 1,
            epoch_started_at: Some(now),
            unmatched_outputs: 0,
            unmatched_submissions: 0,
        };
        progress.observe_feedback(&MediaFeedback::DecodeTimings(timings), at);
        progress.observe_published(None, at);
        assert!(!progress.poll(at).unwrap());
    }
    assert!(progress.poll(now + Duration::from_secs(98)).unwrap());
    assert_eq!(
        progress
            .poll(now + Duration::from_secs(102))
            .unwrap_err()
            .code,
        "webrtc-video-stalled"
    );
}

fn worker(output: EventSender) -> Worker {
    let context: SessionContext = serde_json::from_value(json!({
        "session":{"sessionId":"fixture","serverIp":"127.0.0.1"},"settings":{},"shortcuts":{}
    }))
    .unwrap();
    let (_, commands) = mpsc::sync_channel(4);
    Worker {
        lifecycle: Arc::new(Mutex::new(Lifecycle {
            state: State::Connected,
            context: Some(context.clone()),
            generation: 1,
        })),
        context,
        stream: MediaStreamConfig::default(),
        start_id: "fixture".to_owned(),
        generation: 1,
        cancelled: Arc::new(AtomicBool::new(false)),
        input_ready: Arc::new(AtomicBool::new(false)),
        output,
        feedback: None,
        captured_input: None,
        media: None,
        runtime: None,
        commands,
    }
}

#[test]
fn stopped_generation_rejects_late_status_and_terminal_events() {
    let (events, receiver) = mpsc::channel();
    let worker = worker(EventSender::unbounded(events));
    lock_lifecycle(&worker.lifecycle).generation = 2;
    worker.emit("status", json!({"status":"streaming"}));
    worker.terminal(Failure::signaling("late failure"));
    assert!(receiver.try_recv().is_err());
    assert_eq!(lock_lifecycle(&worker.lifecycle).state, State::Connected);
}

#[test]
fn explicit_cancellation_rejects_late_status_and_terminal_events() {
    let (events, receiver) = mpsc::channel();
    let worker = worker(EventSender::unbounded(events));
    worker.cancelled.store(true, Ordering::Release);
    worker.emit("status", json!({"status":"streaming"}));
    worker.terminal(Failure::signaling("late failure"));
    assert!(receiver.try_recv().is_err());
}

#[test]
fn terminal_failure_is_emitted_once() {
    let (events, receiver) = mpsc::channel();
    let worker = worker(EventSender::unbounded(events));
    worker.terminal(Failure::signaling("failed"));
    worker.terminal(Failure::signaling("duplicate"));
    assert_eq!(receiver.try_iter().count(), 2);
    assert_eq!(lock_lifecycle(&worker.lifecycle).state, State::Idle);
}

#[test]
fn incompatible_profile_is_rejected_before_opening_signaling() {
    let (output, _) = mpsc::channel();
    let (consumer, _receiver) = mpsc::sync_channel(8);
    let mut engine = Engine::with_media_consumer(output, consumer);
    for (codec, color) in [
        ("H265", "8bit_420"),
        ("H264", "10bit_420"),
        ("AV1", "8bit_420"),
    ] {
        let command = serde_json::from_value(json!({"id":"invalid-profile","type":"start","context":{
            "session":{"sessionId":"fixture","serverIp":"127.0.0.1","signalingUrl":"wss://127.0.0.1/nvst/"},
            "settings":{"transportMode":"webrtc","codec":codec,"colorQuality":color},"shortcuts":{}
        }})).unwrap();
        let (response, _) = engine.handle(command);
        assert_eq!(
            response[0]["code"], "webrtc-profile-unsupported",
            "{response:?}"
        );
        assert!(engine.webrtc_session.is_none());
    }
}

struct PeerFixture {
    signaling_url: String,
    media_port: u16,
    stopped: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    input: Receiver<Vec<u8>>,
}

impl PeerFixture {
    fn start(video: Vec<u8>) -> Self {
        use std::io::ErrorKind;
        use str0m::change::SdpAnswer;
        use str0m::format::Codec;
        use str0m::media::{Direction, Frequency, MediaKind, MediaTime};
        use str0m::net::{Protocol, Receive};
        use str0m::{Candidate, Event, Input, Output, RtcConfig};
        use tungstenite::Message;

        opennow_streamer_transport::install_crypto();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let signaling_url = format!("ws://{}/nvst/", listener.local_addr().unwrap());
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket.set_nonblocking(true).unwrap();
        let address = socket.local_addr().unwrap();
        let media_port = address.port();
        let stopped = Arc::new(AtomicBool::new(false));
        let cancellation = stopped.clone();
        let (input_sender, input) = mpsc::channel();
        let worker = thread::spawn(move || {
            let mut rtc = RtcConfig::new()
                .clear_codecs()
                .enable_h264(true)
                .enable_opus(true)
                .build(Instant::now());
            rtc.add_local_candidate(Candidate::host(address, "udp").unwrap());
            let mut changes = rtc.sdp_api();
            let video_mid =
                changes.add_media(MediaKind::Video, Direction::SendOnly, None, None, None);
            let audio_mid =
                changes.add_media(MediaKind::Audio, Direction::SendOnly, None, None, None);
            changes.add_channel("server-bootstrap".to_owned());
            let (offer, pending) = changes.apply().unwrap();
            let mut pending = Some(pending);
            let deadline = Instant::now() + Duration::from_secs(10);
            let stream = loop {
                if cancellation.load(Ordering::Acquire) {
                    return;
                }
                assert!(
                    Instant::now() < deadline,
                    "mock signaling was not contacted"
                );
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(1))
                    }
                    Err(error) => panic!("mock accept failed: {error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            #[allow(clippy::result_large_err)]
            let mut websocket = tungstenite::accept_hdr(
                stream,
                |request: &tungstenite::handshake::server::Request,
                 mut response: tungstenite::handshake::server::Response| {
                    assert!(request.uri().path().ends_with("/nvst/sign_in"));
                    assert!(
                        request
                            .uri()
                            .query()
                            .unwrap()
                            .contains("version=2&peer_role=1&pairing_id=fixture-seat")
                    );
                    assert_eq!(request.headers()["Origin"], "https://play.geforcenow.com");
                    assert_eq!(
                        request.headers()["Sec-WebSocket-Protocol"],
                        "x-nv-sessionid.fixture-seat"
                    );
                    response.headers_mut().insert(
                        "Sec-WebSocket-Protocol",
                        request.headers()["Sec-WebSocket-Protocol"].clone(),
                    );
                    Ok(response)
                },
            )
            .unwrap();
            let peer_info = websocket.read().unwrap().into_text().unwrap();
            let peer_info: Value = serde_json::from_str(&peer_info).unwrap();
            assert_eq!(peer_info["peer_info"]["resolution"], "64x64");
            websocket
                .send(Message::Text(
                    json!({"peer_info":{"name":peer_info["peer_info"]["name"],"id":7},"ackid":1})
                        .to_string()
                        .into(),
                ))
                .unwrap();
            websocket.send(Message::Text(json!({"peer_msg":{"from":9,"to":7,"msg":json!({"type":"offer","sdp":offer.to_sdp_string()}).to_string()},"ackid":2}).to_string().into())).unwrap();
            websocket
                .get_mut()
                .set_read_timeout(Some(Duration::from_millis(1)))
                .unwrap();
            let mut connected = false;
            let mut signaling_open = true;
            let mut next_media = Instant::now();
            let mut timestamp = 900_123_u64;
            while !cancellation.load(Ordering::Acquire) {
                if signaling_open {
                    match websocket.read() {
                        Ok(Message::Text(packet)) => {
                            let packet: Value = serde_json::from_str(&packet).unwrap();
                            if let Some(nested) = packet["peer_msg"]["msg"].as_str() {
                                let message: Value = serde_json::from_str(nested).unwrap();
                                if message["type"] == "answer" {
                                    assert_eq!(packet["peer_msg"]["from"], 7);
                                    assert_eq!(packet["peer_msg"]["to"], 9);
                                    let answer = message["sdp"].as_str().unwrap();
                                    let nvst = message["nvstSdp"].as_str().unwrap();
                                    let local_ufrag = answer
                                        .lines()
                                        .find_map(|line| line.strip_prefix("a=ice-ufrag:"))
                                        .unwrap();
                                    assert!(nvst.contains(&format!(
                                        "a=general.iceUserNameFragment:{local_ufrag}"
                                    )));
                                    assert!(nvst.contains("a=video.clientViewportWd:64"));
                                    assert!(nvst.contains("a=video.bitDepth:8"));
                                    rtc.sdp_api()
                                        .accept_answer(
                                            pending.take().unwrap(),
                                            SdpAnswer::from_sdp_string(answer).unwrap(),
                                        )
                                        .unwrap();
                                }
                            }
                        }
                        Err(tungstenite::Error::Io(error))
                            if matches!(
                                error.kind(),
                                ErrorKind::WouldBlock | ErrorKind::TimedOut
                            ) => {}
                        Err(_) | Ok(Message::Close(_)) => signaling_open = false,
                        Ok(_) => {}
                    }
                }
                rtc.handle_input(Input::Timeout(Instant::now())).unwrap();
                loop {
                    match rtc.poll_output().unwrap() {
                        Output::Timeout(_) => break,
                        Output::Transmit(packet) => {
                            socket
                                .send_to(&packet.contents, packet.destination)
                                .unwrap();
                        }
                        Output::Event(Event::Connected) => connected = true,
                        Output::Event(Event::ChannelOpen(id, label))
                            if label == "input_channel_v1" =>
                        {
                            assert!(
                                rtc.channel(id)
                                    .unwrap()
                                    .write(true, &[0x0e, 0x02, 3, 0])
                                    .unwrap()
                            );
                        }
                        Output::Event(Event::ChannelData(data)) => {
                            let _ = input_sender.send(data.data);
                        }
                        _ => {}
                    }
                }
                let mut buffer = [0; 65_536];
                loop {
                    match socket.recv_from(&mut buffer) {
                        Ok((length, source)) => rtc
                            .handle_input(Input::Receive(
                                Instant::now(),
                                Receive {
                                    proto: Protocol::Udp,
                                    source,
                                    destination: address,
                                    contents: (&buffer[..length]).try_into().unwrap(),
                                },
                            ))
                            .unwrap(),
                        Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                        Err(error) => panic!("fixture RTP receive: {error}"),
                    }
                }
                if connected && Instant::now() >= next_media {
                    for (mid, codec, time, bytes) in [
                        (
                            video_mid,
                            Codec::H264,
                            MediaTime::from_90khz(timestamp),
                            video.clone(),
                        ),
                        (
                            audio_mid,
                            Codec::Opus,
                            MediaTime::new(48_321, Frequency::FORTY_EIGHT_KHZ),
                            vec![0xf8, 0xff, 0xfe],
                        ),
                    ] {
                        let pt = rtc
                            .writer(mid)
                            .unwrap()
                            .payload_params()
                            .find(|params| params.spec().codec == codec)
                            .unwrap()
                            .pt();
                        rtc.writer(mid)
                            .unwrap()
                            .write(pt, Instant::now(), time, bytes)
                            .unwrap();
                    }
                    timestamp += 9_000;
                    next_media = Instant::now() + Duration::from_millis(100);
                }
                thread::sleep(Duration::from_millis(1));
            }
        });
        Self {
            signaling_url,
            media_port,
            stopped,
            worker: Some(worker),
            input,
        }
    }
}

impl Drop for PeerFixture {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let result = worker.join();
            if !thread::panicking() {
                result.unwrap();
            }
        }
    }
}

fn start_command(peer: &PeerFixture, id: &str) -> Command {
    serde_json::from_value(json!({"id":id,"type":"start","context":{
        "session":{"sessionId":"fixture-seat","serverIp":"127.0.0.1","signalingUrl":peer.signaling_url,
            "mediaConnectionInfo":{"ip":"127.0.0.1","port":peer.media_port,"usage":15}},
        "settings":{"transportMode":"webrtc","codec":"H264","colorQuality":"8bit_420","resolution":"64x64","fps":60,"maxBitrateMbps":10},"shortcuts":{}
    }})).unwrap()
}

#[test]
fn engine_signaling_to_real_h264_opus_decode_input_stop_and_restart() {
    use openh264::formats::{RgbSliceU8, YUVBuffer, YUVSource};
    let rgb = vec![96_u8; 64 * 64 * 3];
    let yuv = YUVBuffer::from_rgb_source(RgbSliceU8::new(&rgb, (64, 64)));
    let video = openh264::encoder::Encoder::new()
        .unwrap()
        .encode(&yuv)
        .unwrap()
        .to_vec();
    let (output, events) = mpsc::channel();
    let (consumer, received) = mpsc::sync_channel(8);
    let mut engine = Engine::with_media_consumer(output, consumer);
    for id in ["first", "restart"] {
        let peer = PeerFixture::start(video.clone());
        let (feedback, feedback_receiver) = mpsc::channel();
        engine.media_feedback = Some(feedback_receiver);
        let (responses, _) = engine.handle(start_command(&peer, id));
        assert_eq!(responses[0]["transport"], "webrtc", "{responses:?}");
        let mut decoder = openh264::decoder::Decoder::new().unwrap();
        let mut decoded = false;
        let mut audio = false;
        let deadline = Instant::now() + Duration::from_secs(8);
        while !(decoded && audio) {
            assert!(
                Instant::now() < deadline,
                "media did not arrive: {:?}",
                events.try_iter().collect::<Vec<_>>()
            );
            let Ok(frame) = received.recv_timeout(Duration::from_millis(50)) else {
                continue;
            };
            assert_eq!(frame.frame_index, None);
            assert!(frame.ssrc.is_some());
            if frame.codec == "H264" {
                assert_eq!(frame.clock_rate_hz, 90_000);
                let picture = decoder.decode(&frame.payload).unwrap().unwrap();
                assert_eq!(picture.dimensions(), (64, 64));
                decoded = true;
            } else if frame.codec == "Opus" {
                assert_eq!(frame.payload.as_ref(), &[0xf8, 0xff, 0xfe]);
                assert_eq!(frame.clock_rate_hz, 48_000);
                audio = true;
            }
        }
        assert!(
            !events
                .try_iter()
                .any(|event| event["status"] == "streaming")
        );
        feedback
            .send(MediaFeedback::PlaybackStarted {
                backend: "fixture-decoder",
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            assert!(
                Instant::now() < deadline,
                "first decoded frame event was not forwarded"
            );
            if let Ok(event) = events.recv_timeout(Duration::from_millis(20)) {
                if event["status"] == "streaming" {
                    assert_eq!(event["startId"], id);
                    break;
                }
            }
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while !engine
            .webrtc_session
            .as_ref()
            .unwrap()
            .input_ready
            .load(Ordering::Acquire)
        {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        engine.webrtc_session.as_ref().unwrap().anti_afk().unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            assert!(Instant::now() < deadline, "input did not arrive");
            if let Ok(packet) = peer.input.recv_timeout(Duration::from_millis(20)) {
                if packet.first() == Some(&0x23) {
                    assert_eq!(packet[9], 0x22);
                    assert_eq!(&packet[10..14], &3_u32.to_le_bytes());
                    break;
                }
            }
        }
        let before = Instant::now();
        engine.stop("fixture stop");
        assert!(before.elapsed() < Duration::from_secs(1));
        assert_eq!(lock_lifecycle(&engine.lifecycle).state, State::Idle);
        drop(peer);
        while received.try_recv().is_ok() {}
        while events.try_recv().is_ok() {}
    }
}

#[test]
fn engine_stop_cancels_an_incomplete_signaling_handshake() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}/nvst/", listener.local_addr().unwrap());
    let (accepted, accepted_receiver) = mpsc::channel();
    let (finish, finished) = mpsc::channel();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        accepted.send(()).unwrap();
        let _ = finished.recv_timeout(Duration::from_secs(3));
        drop(stream);
    });
    let (output, events) = mpsc::channel();
    let (consumer, _received) = mpsc::sync_channel(8);
    let mut engine = Engine::with_media_consumer(output, consumer);
    let command: Command = serde_json::from_value(json!({"id":"cancelled","type":"start","context":{
        "session":{"sessionId":"fixture-seat","serverIp":"127.0.0.1","signalingUrl":url},
        "settings":{"transportMode":"webrtc","codec":"H264","colorQuality":"8bit_420"},"shortcuts":{}
    }})).unwrap();
    let (responses, _) = engine.handle(command);
    assert_eq!(responses[0]["transport"], "webrtc");
    accepted_receiver
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    let before = Instant::now();
    engine.stop("cancel handshake");
    assert!(before.elapsed() < Duration::from_millis(500));
    assert_eq!(lock_lifecycle(&engine.lifecycle).state, State::Idle);
    finish.send(()).unwrap();
    server.join().unwrap();
    let events = events.try_iter().collect::<Vec<_>>();
    assert!(
        !events
            .iter()
            .any(|event| event["type"] == "error" || event["status"] == "streaming")
    );
}

fn input_bodies(peer: &PeerFixture, expected: &[u32]) -> Vec<Vec<u8>> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut bodies = Vec::new();
    while bodies.len() < expected.len() {
        assert!(
            Instant::now() < deadline,
            "missing input bodies, received {bodies:?}"
        );
        if let Ok(packet) = peer.input.recv_timeout(Duration::from_millis(10)) {
            if packet.first() != Some(&0x23) {
                continue;
            }
            let offset = match packet[9] {
                0x22 => 10,
                0x21 => 12,
                other => panic!("unexpected wrapper {other}"),
            };
            let body = packet[offset..].to_vec();
            assert_eq!(
                u32::from_le_bytes(body[..4].try_into().unwrap()),
                expected[bodies.len()]
            );
            bodies.push(body);
        }
    }
    bodies
}

fn held_controller() -> CapturedInput {
    CapturedInput::Gamepad {
        controller_id: 0,
        bitmap: 0x0101,
        buttons: 0xffff,
        left_trigger: 255,
        right_trigger: 255,
        left_stick_x: 32767,
        left_stick_y: 32767,
        right_stick_x: 32767,
        right_stick_y: 32767,
    }
}

#[test]
fn engine_input_pause_preserves_media_releases_controls_and_stop_flushes_neutral_state() {
    use openh264::formats::{RgbSliceU8, YUVBuffer};
    let rgb = vec![96_u8; 64 * 64 * 3];
    let yuv = YUVBuffer::from_rgb_source(RgbSliceU8::new(&rgb, (64, 64)));
    let video = openh264::encoder::Encoder::new()
        .unwrap()
        .encode(&yuv)
        .unwrap()
        .to_vec();
    let peer = PeerFixture::start(video);
    let (host, runtime) = opennow_streamer_platform::create_test_runtime();
    let (output, events) = mpsc::channel();
    let mut engine = Engine::with_media_runtime(output, runtime.clone());
    let (responses, _) = engine.handle(start_command(&peer, "pause"));
    assert_eq!(responses[0]["transport"], "webrtc", "{responses:?}");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !engine
        .webrtc_session
        .as_ref()
        .unwrap()
        .input_ready
        .load(Ordering::Acquire)
    {
        assert!(
            Instant::now() < deadline,
            "input not ready: {:?}",
            events.try_iter().collect::<Vec<_>>()
        );
        thread::sleep(Duration::from_millis(5));
    }
    let input = runtime.captured_input();
    input.push(CapturedInput::Key {
        virtual_key: 65,
        modifiers: 0,
        pressed: true,
    });
    input.push(CapturedInput::MouseButton {
        button: 1,
        pressed: true,
    });
    input.push(held_controller());
    input_bodies(&peer, &[3, 8, 12]);
    let command =
        serde_json::from_value(json!({"id":"pause-input","type":"input-paused","paused":true}))
            .unwrap();
    let (response, _) = engine.handle(command);
    assert_eq!(response[0]["type"], "ok");
    let releases = input_bodies(&peer, &[4, 9, 12]);
    assert_eq!(&releases[2][12..24], &[0; 12]);
    let (_, recording) = engine
        .media_session
        .as_ref()
        .unwrap()
        .control()
        .subscribe_recording()
        .unwrap();
    let (frame_sender, frame_receiver) = mpsc::channel();
    let frame_worker = thread::spawn(move || {
        let _ = frame_sender.send(recording.recv());
    });
    let frame = frame_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("media stopped during input pause")
        .unwrap();
    assert!(matches!(
        frame.codec,
        MediaCodec::H264 | MediaCodec::Opus { .. }
    ));
    frame_worker.join().unwrap();
    engine
        .media_session
        .as_ref()
        .unwrap()
        .control()
        .unsubscribe_recording();
    input.push(CapturedInput::Key {
        virtual_key: 66,
        modifiers: 0,
        pressed: true,
    });
    let command =
        serde_json::from_value(json!({"id":"resume-input","type":"input-paused","paused":false}))
            .unwrap();
    engine.handle(command);
    engine.webrtc_session.as_ref().unwrap().anti_afk().unwrap();
    let pulse = input_bodies(&peer, &[3, 4]);
    assert_eq!(&pulse[0][4..6], &0x7c_u16.to_be_bytes());
    assert_eq!(&pulse[1][4..6], &0x7c_u16.to_be_bytes());
    input.push(CapturedInput::Key {
        virtual_key: 67,
        modifiers: 0,
        pressed: true,
    });
    input.push(CapturedInput::MouseButton {
        button: 2,
        pressed: true,
    });
    input.push(held_controller());
    input_bodies(&peer, &[3, 8, 12]);
    let before = Instant::now();
    engine.stop("held input stop");
    assert!(before.elapsed() < Duration::from_secs(1));
    let releases = input_bodies(&peer, &[4, 9, 12]);
    assert_eq!(&releases[0][4..6], &67_u16.to_be_bytes());
    assert_eq!(&releases[2][12..24], &[0; 12]);
    assert!(!events.try_iter().any(|event| event["type"] == "error"));
    drop(engine);
    runtime.shutdown();
    host.join().unwrap();
}
