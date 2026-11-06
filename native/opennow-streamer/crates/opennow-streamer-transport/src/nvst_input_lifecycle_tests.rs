use super::*;

fn poll_input_peer(
    server: &UdpSocket,
    rtc: &mut Rtc,
    client: SocketAddr,
    channels: &mut BTreeMap<String, ChannelId>,
    messages: &mut Vec<Vec<u8>>,
) {
    let now = Instant::now();
    let mut packet = [0; 65_536];
    if let Ok((length, _)) = server.recv_from(&mut packet)
        && (!looks_like_stun(&packet[..length])
            || find_stun_attribute(&packet[..length], STUN_ATTR_PRIORITY).is_some())
    {
        rtc.handle_input(Input::Receive(
            now,
            Receive {
                proto: RtcProtocol::Udp,
                source: logical_ice_addr(client, 1),
                destination: logical_ice_addr(server.local_addr().unwrap(), 2),
                contents: packet[..length].try_into().unwrap(),
            },
        ))
        .unwrap();
    }
    rtc.handle_input(Input::Timeout(now)).unwrap();
    loop {
        match rtc.poll_output().unwrap() {
            Output::Timeout(_) => break,
            Output::Transmit(transmit) => {
                server.send_to(&transmit.contents, client).unwrap();
            }
            Output::Event(Event::Connected) => rtc.direct_api().start_sctp(false),
            Output::Event(Event::ChannelOpen(id, label)) => {
                if label == "control_channel_reliable" {
                    assert!(
                        rtc.channel(id)
                            .unwrap()
                            .write(true, &[0x0e, 0x02, 0x02, 0x00])
                            .unwrap()
                    );
                }
                channels.insert(label, id);
            }
            Output::Event(Event::ChannelData(data)) => messages.push(data.data),
            _ => {}
        }
    }
}

fn required_input_channel_close(label: &str) {
    let server = UdpSocket::bind("127.0.0.1:0").unwrap();
    server
        .set_read_timeout(Some(Duration::from_millis(2)))
        .unwrap();
    let client = UdpSocket::bind("127.0.0.1:0").unwrap();
    let client_addr = client.local_addr().unwrap();
    let server_addr = server.local_addr().unwrap();
    let mut client_rtc = create_nvst_bundle_rtc(&client).unwrap();
    let mut remote = create_nvst_bundle_rtc(&server).unwrap();
    let client_identity = nvst_local_bundle_identity(&mut client_rtc);
    let server_identity = nvst_local_bundle_identity(&mut remote);
    let credentials = stun_credentials();
    remote.add_local_candidate(Candidate::host(logical_ice_addr(server_addr, 2), "udp").unwrap());
    remote.add_remote_candidate(Candidate::host(logical_ice_addr(client_addr, 1), "udp").unwrap());
    {
        let mut api = remote.direct_api();
        api.set_ice_controlling(false);
        api.set_local_ice_credentials(IceCreds {
            ufrag: credentials.remote_username_fragment.clone(),
            pass: credentials.remote_password.clone(),
        });
        api.set_remote_ice_credentials(IceCreds {
            ufrag: credentials.local_username_fragment.clone(),
            pass: credentials.local_password.clone(),
        });
        api.set_remote_fingerprint(
            parse_nvst_fingerprint(&client_identity.dtls_fingerprint).unwrap(),
        );
        api.start_dtls(false).unwrap();
    }
    let mut config = config();
    config.client_udp_port = client_addr.port();
    config.video_peer = server_addr;
    config.remote_dtls_fingerprint = Some(server_identity.dtls_fingerprint);
    config.stun_credentials = Some(credentials);
    let (media, _frames) = mpsc::sync_channel(1);
    let (events, received) = mpsc::channel();
    let session = spawn_nvst_udp_receiver_with_socket(
        config,
        media,
        events,
        Some(client),
        Some(client_rtc),
        Arc::new(HidRuntime::new()),
        None,
    )
    .unwrap();
    let mut channels = BTreeMap::new();
    let mut messages = Vec::new();
    let until = Instant::now() + Duration::from_secs(5);
    while Instant::now() < until
        && (!session.input_ready.load(Ordering::Acquire) || !channels.contains_key(label))
    {
        poll_input_peer(
            &server,
            &mut remote,
            client_addr,
            &mut channels,
            &mut messages,
        );
    }
    assert!(
        session.input_ready.load(Ordering::Acquire),
        "fixture must reach input readiness"
    );
    let mut key = vec![0; 18];
    key[..4].copy_from_slice(&3_u32.to_le_bytes());
    key[4..6].copy_from_slice(&65_u16.to_be_bytes());
    let mut button = vec![0; 18];
    button[..4].copy_from_slice(&8_u32.to_le_bytes());
    button[4] = 1;
    session.control().queue_input(key.clone(), false).unwrap();
    session
        .control()
        .queue_input(button.clone(), false)
        .unwrap();
    let mut gamepad = vec![0; 38];
    gamepad[..4].copy_from_slice(&12_u32.to_le_bytes());
    gamepad[8..10].copy_from_slice(&1_u16.to_le_bytes());
    gamepad[12..14].copy_from_slice(&1_u16.to_le_bytes());
    session.control().queue_input(gamepad.clone(), true).unwrap();
    let expected_key = NvstInputCodec::default()
        .encode(&key, 0)
        .unwrap()
        .remove(0)
        .bytes;
    let expected_button = NvstInputCodec::default()
        .encode(&button, 0)
        .unwrap()
        .remove(0)
        .bytes;
    let expected_gamepad = NvstInputCodec::default().encode(&gamepad, 0).unwrap().pop().unwrap().bytes;
    let until = Instant::now() + Duration::from_secs(2);
    while Instant::now() < until
        && (!messages.contains(&expected_key) || !messages.contains(&expected_button) || !messages.contains(&expected_gamepad))
    {
        poll_input_peer(
            &server,
            &mut remote,
            client_addr,
            &mut channels,
            &mut messages,
        );
    }
    assert!(
        messages.contains(&expected_key) && messages.contains(&expected_button),
        "held inputs must reach the peer before closing a channel"
    );
    assert!(messages.contains(&expected_gamepad), "gamepad queued immediately after readiness must survive startup");
    remote.direct_api().close_data_channel(channels[label]);
    let mut terminal = false;
    let mut unavailable = false;
    let until = Instant::now() + Duration::from_secs(2);
    while Instant::now() < until {
        poll_input_peer(
            &server,
            &mut remote,
            client_addr,
            &mut channels,
            &mut messages,
        );
        while let Ok(event) = received.try_recv() {
            terminal |= matches!(
                event,
                NvstReceiveEvent::Lifecycle(NvstReceiverState::Stopped(
                    NvstStopCause::InputDeliveryFailed
                ))
            );
            unavailable |= matches!(event, NvstReceiveEvent::InputUnavailable(_));
        }
        if terminal {
            break;
        }
    }
    assert!(
        terminal,
        "required input closure must settle without closing the peer or sending another input"
    );
    assert!(unavailable);
    assert!(!session.input_ready.load(Ordering::Acquire));
    assert!(matches!(
        session.control().queue_input(key.clone(), false),
        Err(TransportError::InputNotReady)
    ));
    session.stop();
    if label == "input_channel_partially_reliable" {
        let until = Instant::now() + Duration::from_millis(200);
        while Instant::now() < until {
            poll_input_peer(
                &server,
                &mut remote,
                client_addr,
                &mut channels,
                &mut messages,
            );
        }
        key[..4].copy_from_slice(&4_u32.to_le_bytes());
        button[..4].copy_from_slice(&9_u32.to_le_bytes());
        let release_key = NvstInputCodec::default()
            .encode(&key, 0)
            .unwrap()
            .remove(0)
            .bytes;
        let release_button = NvstInputCodec::default()
            .encode(&button, 0)
            .unwrap()
            .remove(0)
            .bytes;
        assert!(
            messages
                .iter()
                .any(|message| message.get(..36) == Some(&release_key[..36])),
            "held key must be released over surviving reliable control"
        );
        assert!(
            messages
                .iter()
                .any(|message| message.get(..36) == Some(&release_button[..36])),
            "held mouse button must be released over surviving reliable control"
        );
        assert!(messages.iter().any(|message| message.len() == 52 && message[0..2] == 0x020d_u16.to_le_bytes() && message[13] == 0x22 && message[14..18] == 12_u32.to_le_bytes() && message[26..38] == [0; 12]), "held gamepad must be neutralized over surviving reliable control");
    }
}

#[test]
fn partial_input_channel_close_with_held_inputs_settles() {
    required_input_channel_close("input_channel_partially_reliable");
}

#[test]
fn reliable_input_channel_close_with_held_inputs_settles() {
    required_input_channel_close("control_channel_reliable");
}
