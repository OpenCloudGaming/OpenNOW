use super::*;

#[test]
fn selected_ice_ping_does_not_count_duplicate_replies_as_new_round_trips() {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let mut rtc = create_nvst_bundle_rtc(&socket).unwrap();
    let local: SocketAddr = "192.0.2.1:1000".parse().unwrap();
    let remote: SocketAddr = "192.0.2.2:2000".parse().unwrap();
    let credentials = stun_credentials();
    rtc.add_local_candidate(Candidate::host(local, "udp").unwrap());
    rtc.add_remote_candidate(Candidate::host(remote, "udp").unwrap());
    rtc.direct_api().set_remote_ice_credentials(IceCreds {
        ufrag: credentials.remote_username_fragment.clone(),
        pass: credentials.remote_password.clone(),
    });
    rtc.direct_api().set_ice_controlling(true);
    let feedback = NvstFeedbackState::default();
    let mut previous_responses = 0;
    let start = Instant::now();
    let mut last_response = None;
    let mut samples = Vec::new();
    let mut ice_responses = IceResponseTracker::default();

    for tick in 0..=350 {
        let now = start + Duration::from_millis(tick * 10);
        rtc.handle_input(Input::Timeout(now)).unwrap();
        loop {
            match rtc.poll_output().unwrap() {
                Output::Timeout(_) => break,
                Output::Transmit(packet)
                    if packet.contents.get(..2) == Some(&STUN_BINDING_REQUEST.to_be_bytes()) =>
                {
                    ice_responses.sent(&packet.contents);
                    if tick >= 50 {
                        continue;
                    }
                    let transaction_id = packet.contents[8..20].try_into().unwrap();
                    let response = synthesize_ice_binding_success(
                        &transaction_id,
                        local,
                        &credentials.remote_password,
                    );
                    assert!(ice_responses.accept(&response, &credentials.remote_password));
                    rtc.handle_input(Input::Receive(
                        now + Duration::from_millis(5),
                        Receive {
                            proto: RtcProtocol::Udp,
                            source: remote,
                            destination: local,
                            contents: response.as_slice().try_into().unwrap(),
                        },
                    ))
                    .unwrap();
                    last_response = Some(response);
                }
                Output::Event(Event::PeerStats(stats)) => {
                    feedback.update_ice_ping(
                        stats.selected_candidate_pair.as_ref(),
                        &mut previous_responses,
                        now,
                    );
                    if let Some(sample) = feedback.ping_ms(now) {
                        samples.push(sample);
                    }
                }
                _ => {}
            }
        }
        if tick == 100 || tick == 200 {
            let response = last_response.as_ref().unwrap();
            if !ice_responses.accept(response, &credentials.remote_password) {
                continue;
            }
            rtc.handle_input(Input::Receive(
                now,
                Receive {
                    proto: RtcProtocol::Udp,
                    source: remote,
                    destination: local,
                    contents: response.as_slice().try_into().unwrap(),
                },
            ))
            .unwrap();
        }
    }
    assert_eq!(samples.len(), 3);
    assert!(samples.iter().all(|sample| *sample <= 5.0), "{samples:?}");
}

#[test]
fn ice_response_tracking_ignores_only_authenticated_duplicates_of_sent_requests() {
    let credentials = stun_credentials();
    let request = build_stun_binding_request(&credentials, &[1; 12]);
    let response = success([1; 12]);
    let mut tracker = IceResponseTracker::default();
    assert!(tracker.accept(&response, &credentials.remote_password));
    tracker.sent(&request);
    let invalid_integrity = build_authenticated_stun_packet(
        STUN_BINDING_SUCCESS_RESPONSE,
        &[1; 12],
        b"wrong-password",
        &[],
    );
    let mut invalid_fingerprint = response.clone();
    *invalid_fingerprint.last_mut().unwrap() ^= 1;
    for packet in [vec![], request.clone(), invalid_integrity, invalid_fingerprint] {
        assert!(tracker.accept(&packet, &credentials.remote_password));
        assert!(!tracker.requests.front().unwrap().1);
    }
    assert!(tracker.accept(&response, &credentials.remote_password));
    assert!(!tracker.accept(&response, &credentials.remote_password));
    tracker.sent(&request);
    assert!(!tracker.accept(&response, &credentials.remote_password));
    assert_eq!(tracker.requests.len(), 1);
    assert!(tracker.accept(&success([2; 12]), &credentials.remote_password));
}

#[test]
fn ice_response_tracking_handles_optional_fingerprints_and_resets_per_session() {
    let credentials = stun_credentials();
    let mut tracker = IceResponseTracker::default();
    tracker.sent(&build_stun_binding_request(&credentials, &[1; 12]));
    let mut response = success([1; 12]);
    response.truncate(response.len() - 8);
    let length = (response.len() - STUN_HEADER_LEN) as u16;
    response[2..4].copy_from_slice(&length.to_be_bytes());
    assert!(tracker.accept(&response, &credentials.remote_password));
    assert!(!tracker.accept(&response, &credentials.remote_password));
    let mut restarted = IceResponseTracker::default();
    restarted.sent(&build_stun_binding_request(&credentials, &[1; 12]));
    assert!(restarted.accept(&response, &credentials.remote_password));
}

#[test]
fn ice_response_tracking_is_bounded_without_tracking_non_ice_traffic() {
    let credentials = stun_credentials();
    let mut tracker = IceResponseTracker::default();
    for packet in [vec![], b"PING".to_vec(), success([0; 12])] {
        tracker.sent(&packet);
    }
    assert!(tracker.requests.is_empty());
    for id in 0..=MAX_PENDING_STREAM_PINGS {
        tracker.sent(&build_stun_binding_request(&credentials, &[id as u8; 12]));
    }
    assert_eq!(tracker.requests.len(), MAX_PENDING_STREAM_PINGS);
    assert_eq!(tracker.requests.front().unwrap().0, [1; 12]);
    assert!(tracker.accept(&success([1; 12]), &credentials.remote_password));
    assert!(!tracker.accept(&success([1; 12]), &credentials.remote_password));
}

#[test]
fn ice_response_tracking_preserves_reordering_and_genuinely_slow_first_replies() {
    let credentials = stun_credentials();
    let mut ice_responses = IceResponseTracker::default();
    let mut ping = StreamPingTracker::default();
    let start = Instant::now();
    for id in [1, 2] {
        ice_responses.sent(&build_stun_binding_request(&credentials, &[id; 12]));
        ping.sent([id; 12], start);
    }
    for (id, elapsed) in [(2, Duration::from_millis(5)), (1, Duration::from_secs(2))] {
        let response = success([id; 12]);
        assert!(ice_responses.accept(&response, &credentials.remote_password));
        assert_eq!(
            ping.receive(&response, peer(), &credentials, start + elapsed),
            Some(elapsed)
        );
        assert!(!ice_responses.accept(&response, &credentials.remote_password));
    }
}

fn success(transaction_id: [u8; 12]) -> Vec<u8> {
    build_authenticated_stun_packet(
        STUN_BINDING_SUCCESS_RESPONSE,
        &transaction_id,
        stun_credentials().remote_password.as_bytes(),
        &[],
    )
}

#[test]
fn stream_ping_matches_authenticated_transactions_once_and_handles_reordering() {
    let mut tracker = StreamPingTracker::default();
    let start = Instant::now();
    let credentials = stun_credentials();
    tracker.sent([1; 12], start);
    tracker.sent([2; 12], start + Duration::from_millis(100));
    let received = start + Duration::from_micros(125_500);
    assert_eq!(
        tracker.receive(&success([3; 12]), peer(), &credentials, received),
        None
    );
    assert_eq!(
        tracker.receive(&success([2; 12]), peer(), &credentials, received),
        Some(Duration::from_micros(25_500))
    );
    assert_eq!(
        tracker.receive(&success([2; 12]), peer(), &credentials, received),
        None
    );
    assert_eq!(
        tracker.receive(&success([1; 12]), peer(), &credentials, received),
        Some(Duration::from_micros(125_500))
    );
    assert!(tracker.pending.is_empty());
}

#[test]
fn stream_ping_rejects_invalid_packets_without_consuming_the_request() {
    let mut tracker = StreamPingTracker::default();
    let start = Instant::now();
    let credentials = stun_credentials();
    tracker.sent([1; 12], start);
    let mut bad_fingerprint = success([1; 12]);
    *bad_fingerprint.last_mut().unwrap() ^= 1;
    let bad_integrity =
        build_authenticated_stun_packet(STUN_BINDING_SUCCESS_RESPONSE, &[1; 12], b"wrong-key", &[]);
    let request = build_stun_binding_request(&credentials, &[1; 12]);
    let error_response = build_authenticated_stun_packet(
        0x0111,
        &[1; 12],
        credentials.remote_password.as_bytes(),
        &[],
    );
    for packet in [
        vec![],
        vec![1; 19],
        bad_fingerprint,
        bad_integrity,
        request,
        error_response,
    ] {
        assert_eq!(tracker.receive(&packet, peer(), &credentials, start), None);
        assert_eq!(tracker.pending.len(), 1);
    }
    assert_eq!(
        tracker.receive(&success([1; 12]), peer(), &credentials, start),
        Some(Duration::ZERO)
    );
}

#[test]
fn stream_ping_bounds_pending_requests_and_expires_late_replies() {
    let mut tracker = StreamPingTracker::default();
    let start = Instant::now();
    let credentials = stun_credentials();
    for id in 0..=MAX_PENDING_STREAM_PINGS {
        tracker.sent([id as u8; 12], start);
    }
    assert_eq!(tracker.pending.len(), MAX_PENDING_STREAM_PINGS);
    assert_eq!(
        tracker.receive(&success([0; 12]), peer(), &credentials, start),
        None
    );
    assert_eq!(
        tracker.receive(
            &success([1; 12]),
            peer(),
            &credentials,
            start + STREAM_PING_TIMEOUT
        ),
        None
    );
    tracker.sent([99; 12], start + STREAM_PING_TIMEOUT);
    assert_eq!(tracker.pending.len(), 1);
    assert_eq!(tracker.pending.front().unwrap().0, [99; 12]);
}

#[test]
fn stream_ping_keeps_fresh_source_priority_when_other_samples_arrive_later() {
    let feedback = NvstFeedbackState::default();
    let start = Instant::now();
    *feedback.ice_ping.lock().unwrap() = Some((start, Duration::from_millis(10)));
    feedback.publish_ping(
        true,
        start + Duration::from_secs(1),
        Duration::from_millis(30),
    );
    feedback.publish_ping(
        false,
        start + Duration::from_secs(2),
        Duration::from_millis(50),
    );
    assert_eq!(feedback.ping_ms(start + Duration::from_secs(2)), Some(10.0));
    assert_eq!(feedback.ping_ms(start + STREAM_PING_TIMEOUT), Some(30.0));
    feedback.publish_ping(
        false,
        start + STREAM_PING_TIMEOUT,
        Duration::from_millis(60),
    );
    assert_eq!(feedback.ping_ms(start + STREAM_PING_TIMEOUT), Some(30.0));
    assert_eq!(
        feedback.ping_ms(start + STREAM_PING_TIMEOUT + Duration::from_secs(1)),
        Some(60.0)
    );
}

#[test]
fn stream_ping_prefers_video_with_fresh_bundle_fallback_and_resets_per_session() {
    let feedback = NvstFeedbackState::default();
    let start = Instant::now();
    assert_eq!(feedback.ping_ms(start), None);
    feedback.publish_ping(false, start, Duration::from_micros(22_500));
    assert_eq!(feedback.ping_ms(start), Some(22.5));
    feedback.publish_ping(true, start, Duration::from_millis(30));
    assert_eq!(feedback.ping_ms(start), Some(30.0));
    feedback.publish_ping(
        false,
        start + Duration::from_secs(1),
        Duration::from_millis(24),
    );
    assert_eq!(feedback.ping_ms(start + STREAM_PING_TIMEOUT), Some(24.0));
    assert_eq!(
        feedback.ping_ms(start + STREAM_PING_TIMEOUT + Duration::from_secs(1)),
        None
    );
    assert_eq!(NvstFeedbackState::default().ping_ms(start), None);
}

#[test]
fn mjolnir_receiver_publishes_ping_from_its_real_udp_keepalive_reply() {
    let server = UdpSocket::bind("127.0.0.1:0").unwrap();
    server
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let client = UdpSocket::bind("127.0.0.1:0").unwrap();
    let mut config = config();
    config.video_peer = server.local_addr().unwrap();
    config.stun_credentials = Some(stun_credentials());
    let feedback = config.feedback();
    let (media_consumer, _media_receiver) = mpsc::sync_channel(1);
    let (event_sender, _event_receiver) = mpsc::channel();
    let session =
        spawn_nvst_mjolnir_receiver(client, config, media_consumer, event_sender).unwrap();
    let mut packet = [0; 512];
    let (length, source) = server.recv_from(&mut packet).unwrap();
    assert!(length >= STUN_HEADER_LEN);
    let transaction_id = packet[8..20].try_into().unwrap();
    let response = success(transaction_id);
    let unexpected_peer = UdpSocket::bind("127.0.0.1:0").unwrap();
    unexpected_peer.send_to(&response, source).unwrap();
    server.recv_from(&mut packet).unwrap();
    assert_eq!(feedback.ping_ms(Instant::now()), None);
    server.send_to(&response, source).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while feedback.ping_ms(Instant::now()).is_none() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    let measured = feedback.ping_ms(Instant::now());
    session.stop();
    assert!(measured.is_some_and(|value| value > 0.0 && value < 2000.0));
}

#[test]
fn bundle_receiver_publishes_ping_from_its_real_udp_keepalive_reply() {
    let server = UdpSocket::bind("127.0.0.1:0").unwrap();
    server
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let client = UdpSocket::bind("127.0.0.1:0").unwrap();
    let mut config = config();
    config.video_peer = server.local_addr().unwrap();
    config.stun_credentials = Some(stun_credentials());
    config.remote_dtls_fingerprint = Some(["00"; 32].join(":"));
    config.ping_payload = b"ping-fixture".to_vec();
    let feedback = config.feedback();
    let (media_consumer, _media_receiver) = mpsc::sync_channel(1);
    let (event_sender, _event_receiver) = mpsc::channel();
    let session = spawn_nvst_udp_receiver_with_socket(
        config,
        media_consumer,
        event_sender,
        Some(client),
        None,
    )
    .unwrap();
    let mut packet = [0; 2048];
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut replied = false;
    while Instant::now() < deadline {
        let (length, source) = server.recv_from(&mut packet).unwrap();
        if find_stun_attribute(&packet[..length], STUN_ATTR_USERNAME)
            .is_some_and(|(_, username)| username == b"PING:loc1")
        {
            let transaction_id = packet[8..20].try_into().unwrap();
            server.send_to(&success(transaction_id), source).unwrap();
            replied = true;
            break;
        }
    }
    while feedback.ping_ms(Instant::now()).is_none() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    let measured = feedback.ping_ms(Instant::now());
    session.stop();
    assert!(replied);
    assert!(measured.is_some_and(|value| (0.0..2000.0).contains(&value)));
    assert!(feedback.video_ping.lock().unwrap().is_none());
}

#[test]
fn selected_ice_pair_ping_takes_priority_without_refreshing_old_samples() {
    let feedback = NvstFeedbackState::default();
    let now = Instant::now();
    let mut previous_responses = 0;
    let mut pair = CandidatePairStats {
        protocol: RtcProtocol::Udp,
        local: str0m::stats::CandidateStats {
            addr: "127.0.0.1:5000".parse().unwrap(),
        },
        remote: str0m::stats::CandidateStats { addr: peer() },
        current_round_trip_time: None,
        total_round_trip_time: Duration::ZERO,
        responses_received: 0,
    };
    feedback.update_ice_ping(Some(&pair), &mut previous_responses, now);
    assert_eq!(feedback.ping_ms(now), None);
    feedback.publish_ping(true, now, Duration::from_millis(40));
    pair.responses_received = 1;
    pair.current_round_trip_time = Some(Duration::from_micros(25_500));
    feedback.update_ice_ping(Some(&pair), &mut previous_responses, now);
    assert_eq!(feedback.ping_ms(now), Some(25.5));
    feedback.publish_ping(
        true,
        now + Duration::from_secs(1),
        Duration::from_millis(30),
    );
    feedback.update_ice_ping(
        Some(&pair),
        &mut previous_responses,
        now + STREAM_PING_TIMEOUT,
    );
    assert_eq!(feedback.ping_ms(now + STREAM_PING_TIMEOUT), Some(30.0));
    pair.responses_received = 2;
    pair.current_round_trip_time = Some(Duration::ZERO);
    feedback.update_ice_ping(
        Some(&pair),
        &mut previous_responses,
        now + STREAM_PING_TIMEOUT,
    );
    assert_eq!(feedback.ping_ms(now + STREAM_PING_TIMEOUT), Some(0.0));
    feedback.update_ice_ping(None, &mut previous_responses, now + STREAM_PING_TIMEOUT);
    assert_eq!(feedback.ping_ms(now + STREAM_PING_TIMEOUT), Some(30.0));
    assert_eq!(previous_responses, 0);
    feedback.update_ice_ping(
        Some(&pair),
        &mut previous_responses,
        now + STREAM_PING_TIMEOUT,
    );
    assert_eq!(feedback.ping_ms(now + STREAM_PING_TIMEOUT), Some(0.0));
}
