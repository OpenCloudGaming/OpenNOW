use super::*;
use std::net::TcpListener;

const TARGET: &str = "rtsps://seat.nvidiagrid.net:322";
const VALID_PEER: &str = "Transport: unicast;source=192.0.2.10;X-GS-ServerPort=5004-5005\r\nX-Nv-Ping: 6\r\nX-Nv-Ping-Payload: 00ff\r\n";
const EMPTY_TRANSPORT_URIS: [&str; 4] = [
    "streamid=video/0/0",
    "streamid=video/0",
    "rtsps://seat.nvidiagrid.net:322/streamid=video/0/0",
    "rtsps://seat.nvidiagrid.net:322/streamid=video/0",
];

#[derive(Clone, Copy)]
struct Reply {
    uri: &'static str,
    transport: &'static str,
    status: u16,
    headers: &'static str,
}

fn scripted_setup(replies: Vec<Reply>) -> Result<VideoSetup, NvstRtspError> {
    scripted_setup_with_retry(replies, 0, Duration::ZERO, None)
}

fn scripted_setup_with_retry(
    replies: Vec<Reply>,
    max_peer_retries: u32,
    peer_retry_delay: Duration,
    bundle_video_peer: Option<(String, u16, u16)>,
) -> Result<VideoSetup, NvstRtspError> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let stream = TcpStream::connect(address).unwrap();
    let (server_stream, _) = listener.accept().unwrap();
    for socket in [&stream, &server_stream] {
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
    }
    let server = thread::spawn(move || {
        let mut socket = tungstenite::accept(server_stream).unwrap();
        for (index, reply) in replies.iter().enumerate() {
            let Message::Text(request) = socket.read().unwrap() else {
                panic!("expected a SETUP request");
            };
            assert!(
                request.starts_with(&format!("SETUP {} RTSP/1.0\r\n", reply.uri)),
                "{request}"
            );
            assert!(request.contains(&format!("\r\nTransport: {}\r\n", reply.transport)));
            assert!(request.contains("\r\nSession: rtsp-session\r\n"));
            assert!(request.contains("\r\nx-nv-sessionid: nv-session\r\n"));
            assert!(request.contains("\r\nx-nv-ping: 6\r\n"));
            let cseq = index + 3;
            assert!(request.contains(&format!("\r\nCSeq: {cseq}\r\n")));
            socket
                .send(Message::Text(
                    format!(
                        "RTSP/1.0 {} Response\r\nCSeq: {cseq}\r\n{}Content-Length: 0\r\n\r\n",
                        reply.status, reply.headers
                    )
                    .into(),
                ))
                .unwrap();
        }
    });
    let (socket, _) = tungstenite::client(
        format!("ws://{address}/rtsp"),
        MaybeTlsStream::Plain(stream),
    )
    .unwrap();
    let mut client = RtspClient {
        socket,
        cseq: 2,
        buffer: String::new(),
    };
    let result = client.setup_video_with_retry(
        "streamid=video/0",
        TARGET,
        &[
            ("Session", "rtsp-session".to_owned()),
            ("x-nv-sessionid", "nv-session".to_owned()),
            ("x-nv-ping", "6".to_owned()),
        ],
        49005,
        max_peer_retries,
        peer_retry_delay,
        bundle_video_peer,
    );
    drop(client);
    server.join().unwrap();
    result
}

#[test]
fn video_setup_keeps_the_official_first_attempt_and_its_metadata() {
    let setup = scripted_setup(vec![Reply {
        uri: EMPTY_TRANSPORT_URIS[0],
        transport: "",
        status: 200,
        headers: VALID_PEER,
    }])
    .unwrap();
    assert_eq!(setup.peer, ("192.0.2.10".to_owned(), 5004, 5005));
    assert_eq!(
        header_value(&setup.response, "x-nv-ping-payload"),
        Some("00ff")
    );
}

#[test]
fn video_setup_stops_at_first_peerless_success_to_protect_announce() {
    // Live alliance rigs accept the first SETUP but poison the session once
    // further forms are tried (later rounds degrade to pure 400s and ANNOUNCE
    // is then rejected), while a single SETUP followed by ANNOUNCE succeeds.
    // The scripted server holds exactly one reply: any extra request fails it.
    let error = match scripted_setup(vec![Reply {
        uri: EMPTY_TRANSPORT_URIS[0],
        transport: "",
        status: 200,
        headers: "",
    }]) {
        Ok(_) => panic!("a peerless 200 must not yield a peer"),
        Err(error) => error,
    };
    assert_eq!(error.code, "missing-video-peer");
    assert!(error.message.contains("peerless 200"));
}

#[test]
fn video_setup_tries_absolute_forms_after_uri_rejections() {
    let replies = EMPTY_TRANSPORT_URIS
        .iter()
        .enumerate()
        .map(|(index, uri)| Reply {
            uri,
            transport: "",
            status: if index == 3 { 200 } else { 404 },
            headers: if index == 3 { VALID_PEER } else { "" },
        })
        .collect();
    let setup = scripted_setup(replies).unwrap();
    assert_eq!(setup.peer.1, 5004);
}

#[test]
fn video_setup_tries_client_udp_transport_after_all_empty_transport_forms() {
    let mut replies: Vec<_> = EMPTY_TRANSPORT_URIS
        .iter()
        .map(|uri| Reply {
            uri,
            transport: "",
            status: 461,
            headers: "",
        })
        .collect();
    replies.extend(
        EMPTY_TRANSPORT_URIS
            .iter()
            .enumerate()
            .map(|(index, uri)| Reply {
                uri,
                transport: "unicast;X-GS-ClientPort=49005-49006",
                status: if index == 3 { 200 } else { 400 },
                headers: if index == 3 { VALID_PEER } else { "" },
            }),
    );
    let setup = scripted_setup(replies).unwrap();
    assert_eq!(setup.peer.1, 5004);
    assert_eq!(
        header_value(&setup.response, "x-nv-ping-payload"),
        Some("00ff")
    );
}

#[test]
fn video_setup_never_invents_a_peer_when_success_omits_transport() {
    // Single peerless 200: the sweep stops immediately (one scripted reply)
    // instead of trying further forms that would poison strict sessions.
    let error = match scripted_setup(vec![Reply {
        uri: EMPTY_TRANSPORT_URIS[0],
        transport: "",
        status: 200,
        headers: "",
    }]) {
        Ok(_) => panic!("SETUP without a peer must not succeed"),
        Err(error) => error,
    };
    assert_eq!(error.code, "missing-video-peer");
    assert!(error.message.contains("peerless 200"));
}

#[test]
fn video_setup_rejects_partial_and_invalid_transport_metadata() {
    // Present-but-unusable Transport (missing/invalid source or ports) also
    // stops the sweep: no live rig has ever yielded a peer on a later form
    // after a peerless 200, while extra SETUPs poison strict sessions.
    for headers in [
        "Transport: unicast;X-GS-ServerPort=5004\r\n",
        "Transport: unicast;source=192.0.2.10\r\n",
        "Transport: unicast;source=not-an-ip;X-GS-ServerPort=5004\r\n",
    ] {
        let error = match scripted_setup(vec![Reply {
            uri: EMPTY_TRANSPORT_URIS[0],
            transport: "",
            status: 200,
            headers,
        }]) {
            Ok(_) => panic!("unusable Transport must not yield a peer"),
            Err(error) => error,
        };
        assert_eq!(error.code, "missing-video-peer");
    }
}

#[test]
fn video_setup_stops_on_auth_session_and_server_errors() {
    for status in [401, 403, 454, 455, 500, 503] {
        let result = scripted_setup(vec![Reply {
            uri: EMPTY_TRANSPORT_URIS[0],
            transport: "",
            status,
            headers: "",
        }]);
        let error = match result {
            Ok(_) => panic!("a fatal SETUP response must not succeed"),
            Err(error) => error,
        };
        assert_eq!(error.code, "nvst-rtsp-failed");
        assert!(error.message.contains(&status.to_string()));
    }
}

#[test]
fn video_setup_candidates_match_mac_order_without_duplicates() {
    assert_eq!(
        video_setup_candidates("streamid=video/0", TARGET),
        EMPTY_TRANSPORT_URIS
    );
    assert_eq!(
        video_setup_candidates("streamid=video/0/0", TARGET),
        vec![
            "streamid=video/0/0",
            "rtsps://seat.nvidiagrid.net:322/streamid=video/0/0",
        ]
    );
    assert_eq!(
        video_setup_candidates("/streamid=video/0", TARGET),
        vec![
            "/streamid=video/0",
            "rtsps://seat.nvidiagrid.net:322/streamid=video/0",
        ]
    );
    for absolute in [
        "rtsp://seat.nvidiagrid.net:322/video",
        "rtsps://seat.nvidiagrid.net:322/video",
    ] {
        assert_eq!(video_setup_candidates(absolute, TARGET), vec![absolute]);
    }
}

#[test]
fn rtsp_request_deadline_bounds_a_partial_response() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let stream = TcpStream::connect(address).unwrap();
    let (server_stream, _) = listener.accept().unwrap();
    for socket in [&stream, &server_stream] {
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
    }
    let (release, released) = mpsc::channel();
    let server = thread::spawn(move || {
        let mut socket = tungstenite::accept(server_stream).unwrap();
        assert!(matches!(socket.read().unwrap(), Message::Text(_)));
        socket
            .send(Message::Text("RTSP/1.0 200 OK\r\nCSeq: 1\r\n".into()))
            .unwrap();
        let _ = released.recv_timeout(Duration::from_secs(2));
    });
    let (socket, _) = tungstenite::client(
        format!("ws://{address}/rtsp"),
        MaybeTlsStream::Plain(stream),
    )
    .unwrap();
    let mut client = RtspClient {
        socket,
        cseq: 0,
        buffer: String::new(),
    };
    let start = Instant::now();
    let result = client.request_with_timeout(
        "SETUP",
        "streamid=video/0/0",
        &[],
        "",
        Duration::from_millis(80),
    );
    let elapsed = start.elapsed();
    let _ = release.send(());
    server.join().unwrap();
    let error = match result {
        Ok(_) => panic!("an incomplete RTSP response must time out"),
        Err(error) => error,
    };
    assert_eq!(error.code, "nvst-rtsp-timeout");
    assert!(
        elapsed < Duration::from_secs(1),
        "deadline exceeded: {elapsed:?}"
    );
}

#[test]
fn video_setup_resweeps_when_successes_omit_a_peer_until_the_rig_is_ready() {
    // Round 1 draws one peerless 200 and stops at it; round 2 re-sends the
    // same form and the rig has a peer by then. Mirrors a late-starting
    // encoder without spraying further forms that poison strict sessions.
    let replies = vec![
        Reply {
            uri: EMPTY_TRANSPORT_URIS[0],
            transport: "",
            status: 200,
            headers: "",
        },
        Reply {
            uri: EMPTY_TRANSPORT_URIS[0],
            transport: "",
            status: 200,
            headers: VALID_PEER,
        },
    ];
    let setup = scripted_setup_with_retry(replies, 3, Duration::ZERO, None).unwrap();
    assert_eq!(setup.peer, ("192.0.2.10".to_owned(), 5004, 5005));
}

#[test]
fn video_setup_peer_retry_stays_bounded_and_keeps_the_terminal_code() {
    // Every round draws one peerless 200 and stops there: retries exhaust,
    // then the original missing-video-peer error (not a timeout, not a new
    // code) is returned.
    let replies = vec![
        Reply {
            uri: EMPTY_TRANSPORT_URIS[0],
            transport: "",
            status: 200,
            headers: "",
        };
        3
    ];
    let error = match scripted_setup_with_retry(replies, 2, Duration::ZERO, None) {
        Ok(_) => panic!("SETUP without a peer must not succeed"),
        Err(error) => error,
    };
    assert_eq!(error.code, "missing-video-peer");
    assert!(error.message.contains("peerless 200"));
}

#[test]
fn video_setup_never_retries_pure_rejections() {
    // No 200 seen at all: the forms are wrong for this server, so fail fast
    // without burning retry rounds.
    let replies: Vec<_> = ["", "unicast;X-GS-ClientPort=49005-49006"]
        .iter()
        .flat_map(|transport| {
            EMPTY_TRANSPORT_URIS.iter().map(move |uri| Reply {
                uri,
                transport,
                status: 400,
                headers: "",
            })
        })
        .collect();
    let error = match scripted_setup_with_retry(replies, 3, Duration::ZERO, None) {
        Ok(_) => panic!("rejected SETUP forms must not succeed"),
        Err(error) => error,
    };
    assert_eq!(error.code, "nvst-rtsp-failed");
}

#[test]
fn video_setup_degraded_retry_round_keeps_missing_peer_code() {
    // Round 1 draws a peerless 200 and stops, so a retry is scheduled; round
    // 2 then degrades to pure 400s (repeat SETUPs rejected). The terminal code
    // must stay missing-video-peer so callers can try the bundle peer instead
    // of treating it as wrong forms. Round 2 sweeps all forms because it drew
    // no 200 at all.
    let mut replies = vec![Reply {
        uri: EMPTY_TRANSPORT_URIS[0],
        transport: "",
        status: 200,
        headers: "",
    }];
    replies.extend(
        ["", "unicast;X-GS-ClientPort=49005-49006"]
            .iter()
            .flat_map(|transport| {
                EMPTY_TRANSPORT_URIS.iter().map(move |uri| Reply {
                    uri,
                    transport,
                    status: 400,
                    headers: "",
                })
            }),
    );
    let error = match scripted_setup_with_retry(replies, 1, Duration::ZERO, None) {
        Ok(_) => panic!("peerless SETUP must not succeed"),
        Err(error) => error,
    };
    assert_eq!(error.code, "missing-video-peer");
}

#[test]
fn video_setup_uses_bundle_peer_on_first_peerless_success_without_retry() {
    let peerless = vec![Reply {
        uri: EMPTY_TRANSPORT_URIS[0],
        transport: "",
        status: 200,
        headers: "Transport: \r\nX-Nv-Ping: 6\r\nX-Nv-Ping-Payload: 00ff\r\n",
    }];
    let setup = scripted_setup_with_retry(
        peerless,
        3,
        Duration::ZERO,
        Some(("192.0.2.99".to_owned(), 13749, 13749)),
    )
    .unwrap();
    assert_eq!(setup.peer, ("192.0.2.99".to_owned(), 13749, 13749));
    assert_eq!(
        header_value(&setup.response, "x-nv-ping-payload"),
        Some("00ff")
    );
}
