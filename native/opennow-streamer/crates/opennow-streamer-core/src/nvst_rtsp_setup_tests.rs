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

struct Reply {
    uri: &'static str,
    transport: &'static str,
    status: u16,
    headers: &'static str,
}

fn scripted_setup(replies: Vec<Reply>) -> Result<VideoSetup, NvstRtspError> {
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
    let result = client.setup_video(
        "streamid=video/0",
        TARGET,
        &[
            ("Session", "rtsp-session".to_owned()),
            ("x-nv-sessionid", "nv-session".to_owned()),
            ("x-nv-ping", "6".to_owned()),
        ],
        49005,
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
fn video_setup_retries_advertised_control_after_success_without_a_peer() {
    let setup = scripted_setup(vec![
        Reply {
            uri: EMPTY_TRANSPORT_URIS[0],
            transport: "",
            status: 200,
            headers: "",
        },
        Reply {
            uri: EMPTY_TRANSPORT_URIS[1],
            transport: "",
            status: 200,
            headers: VALID_PEER,
        },
    ])
    .unwrap();
    assert_eq!(setup.peer, ("192.0.2.10".to_owned(), 5004, 5005));
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
fn video_setup_never_invents_a_peer_when_all_successes_omit_transport() {
    let replies = ["", "unicast;X-GS-ClientPort=49005-49006"]
        .iter()
        .flat_map(|transport| {
            EMPTY_TRANSPORT_URIS.iter().map(move |uri| Reply {
                uri,
                transport,
                status: 200,
                headers: "",
            })
        })
        .collect();
    let error = match scripted_setup(replies) {
        Ok(_) => panic!("SETUP without a peer must not succeed"),
        Err(error) => error,
    };
    assert_eq!(error.code, "missing-video-peer");
    assert!(error.message.contains("4 URI forms and 2 Transport forms"));
}

#[test]
fn video_setup_retries_partial_and_invalid_transport_metadata() {
    let replies = [
        "Transport: unicast;X-GS-ServerPort=5004\r\n",
        "Transport: unicast;source=192.0.2.10\r\n",
        "Transport: unicast;source=not-an-ip;X-GS-ServerPort=5004\r\n",
        VALID_PEER,
    ]
    .iter()
    .zip(EMPTY_TRANSPORT_URIS)
    .map(|(headers, uri)| Reply {
        uri,
        transport: "",
        status: 200,
        headers,
    })
    .collect();
    assert_eq!(scripted_setup(replies).unwrap().peer.1, 5004);
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
