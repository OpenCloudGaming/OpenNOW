use super::*;
use std::net::TcpListener;

fn socket_pair() -> (RtspClient, WebSocket<TcpStream>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(6)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        tungstenite::accept(stream).unwrap()
    });
    let stream = TcpStream::connect(address).unwrap();
    stream.set_nodelay(true).unwrap();
    let (socket, _) = tungstenite::client(
        format!("ws://{address}/rtsp"),
        MaybeTlsStream::Plain(stream),
    )
    .unwrap();
    (
        RtspClient {
            socket,
            cseq: 7,
            buffer: String::new(),
        },
        server.join().unwrap(),
    )
}

fn active_session(client: RtspClient, ping: &NvstControlPing) -> ActiveNvstRtspSession {
    ActiveNvstRtspSession::spawn(client, ping.clone()).unwrap()
}

fn server_ping(server: &mut WebSocket<TcpStream>) {
    server.send(Message::Ping(vec![1, 2, 3].into())).unwrap();
    assert_eq!(server.read().unwrap(), Message::Pong(vec![1, 2, 3].into()));
}

fn client_ping(server: &mut WebSocket<TcpStream>) -> Vec<u8> {
    let message = server.read().unwrap();
    let Message::Ping(bytes) = message else {
        panic!("expected WebSocket ping, received {message:?}");
    };
    assert_eq!(bytes.len(), 8);
    bytes.to_vec()
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while !condition() {
        assert!(Instant::now() < deadline, "worker did not respond promptly");
        thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn describe_accepts_large_text_binary_and_split_responses() {
    for (binary, chunk_size) in [
        (false, usize::MAX),
        (true, usize::MAX),
        (false, 16 * 1024),
        (true, 16 * 1024),
    ] {
        let (mut client, mut server) = socket_pair();
        let body = "a=x-nv-test:description\r\n".repeat(12_000);
        assert!(body.len() > MAX_CONTROL_RESPONSE_BYTES);
        let expected_body = body.clone();
        let sender = thread::spawn(move || {
            let Message::Text(request) = server.read().unwrap() else {
                panic!("expected DESCRIBE request");
            };
            assert!(request.starts_with("DESCRIBE "));
            assert!(request.contains("\r\nCSeq: 8\r\n"));
            let response = format!(
                "RTSP/1.0 200 OK\r\nCSeq: 8\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
            for chunk in response.as_bytes().chunks(chunk_size) {
                let message = if binary {
                    Message::Binary(chunk.to_vec().into())
                } else {
                    Message::Text(String::from_utf8(chunk.to_vec()).unwrap().into())
                };
                server.send(message).unwrap();
            }
        });
        let response = client
            .request("DESCRIBE", "rtsps://seat.nvidiagrid.net:322", &[], "")
            .unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, expected_body);
        assert!(client.buffer.is_empty());
        sender.join().unwrap();
    }
}

#[test]
fn request_rejects_unbounded_partial_response() {
    let (mut client, mut server) = socket_pair();
    let sender = thread::spawn(move || {
        server.read().unwrap();
        server
            .send(Message::Text("x".repeat(MAX_REQUEST_RESPONSE_BYTES).into()))
            .unwrap();
        server.send(Message::Text("x".into())).unwrap();
    });
    let error = client
        .request("DESCRIBE", "rtsps://seat.nvidiagrid.net:322", &[], "")
        .err()
        .unwrap();
    assert_eq!(error.code, "nvst-rtsp-failed");
    assert!(
        error
            .message
            .contains("DESCRIBE response exceeds request buffer limit")
    );
    sender.join().unwrap();
}

#[test]
fn request_bounds_websocket_message_size() {
    let (mut client, mut server) = socket_pair();
    let sender = thread::spawn(move || {
        server.read().unwrap();
        let _ = server.send(Message::Text(
            "x".repeat(MAX_REQUEST_RESPONSE_BYTES + 1).into(),
        ));
    });
    let error = client
        .request("DESCRIBE", "rtsps://seat.nvidiagrid.net:322", &[], "")
        .err()
        .unwrap();
    assert_eq!(error.code, "nvst-rtsp-failed");
    assert!(error.message.contains("Space limit exceeded"));
    assert!(client.buffer.is_empty());
    sender.join().unwrap();
}

#[test]
fn rtsp_parser_accepts_response_at_request_size_limit() {
    let prefix = "RTSP/1.0 200 OK\r\nCSeq: 8\r\nContent-Length: ";
    let body_length = MAX_REQUEST_RESPONSE_BYTES
        - prefix.len()
        - MAX_REQUEST_RESPONSE_BYTES.to_string().len()
        - 4;
    let body = "x".repeat(body_length);
    let mut buffer = format!("{prefix}{body_length}\r\n\r\n{body}");
    assert_eq!(buffer.len(), MAX_REQUEST_RESPONSE_BYTES);
    let response = take_rtsp_response(&mut buffer, 8).unwrap().unwrap();
    assert_eq!(response.body, body);
    assert!(buffer.is_empty());
}

#[test]
fn rtsp_parser_bounds_declared_response_size_before_receiving_body() {
    let mut oversized = format!(
        "RTSP/1.0 200 OK\r\nCSeq: 8\r\nContent-Length: {MAX_REQUEST_RESPONSE_BYTES}\r\n\r\n"
    );
    let error = take_rtsp_response(&mut oversized, 8).err().unwrap();
    assert!(
        error
            .message
            .contains("response exceeds request buffer limit")
    );

    let mut bounded = "RTSP/1.0 200 OK\r\nCSeq: 8\r\nContent-Length: 262144\r\n\r\n".to_owned();
    assert!(take_rtsp_response(&mut bounded, 8).unwrap().is_none());
}

#[test]
fn control_ping_parser_rejects_overflowing_and_invalid_body_boundaries() {
    let mut overflowing = format!(
        "RTSP/1.0 551 Response\r\nCSeq: 8\r\nContent-Length: {}\r\n\r\n",
        usize::MAX
    );
    assert!(take_rtsp_response(&mut overflowing, 8).is_err());
    let mut invalid = "RTSP/1.0 551 Response\r\nCSeq: 8\r\nContent-Length: 1\r\n\r\né".to_owned();
    assert!(take_rtsp_response(&mut invalid, 8).is_err());
}

#[test]
fn control_ping_samples_are_shared_expiring_and_clearable() {
    let ping = NvstControlPing::default();
    let clone = ping.clone();
    let sent = Instant::now();
    let received = sent + Duration::from_micros(4_750);
    assert_eq!(ping.ping_ms(sent), None);
    ping.record(sent, received);
    assert_eq!(clone.ping_ms(received), Some(4.75));
    assert_eq!(clone.ping_ms(sent), None);
    assert_eq!(ping.ping_ms(received + Duration::from_secs(4)), Some(4.75));
    assert_eq!(ping.ping_ms(received + CONTROL_PING_EXPIRY), None);
    assert_eq!(clone.ping_ms(received), None);
    ping.record(sent, received);
    clone.clear();
    assert_eq!(ping.ping_ms(received), None);
    ping.record(sent, sent);
    assert_eq!(ping.ping_ms(sent), Some(0.0));
}

#[test]
fn live_control_ping_uses_websocket_without_rtsp_requests() {
    let (client, mut server) = socket_pair();
    let ping = NvstControlPing::default();
    let mut active = active_session(client, &ping);
    server_ping(&mut server);
    let payload = client_ping(&mut server);
    server.send(Message::Pong(vec![0; 8].into())).unwrap();
    assert_eq!(ping.ping_ms(Instant::now()), None);
    server.send(Message::Pong(payload.into())).unwrap();
    wait_until(|| ping.ping_ms(Instant::now()).is_some());
    assert!(ping.ping_ms(Instant::now()).unwrap() < 1000.0);
    server_ping(&mut server);
    server.close(None).unwrap();
    wait_until(|| active.worker.as_ref().unwrap().is_finished());
    assert_eq!(ping.ping_ms(Instant::now()), None);
    let shutdown_started = Instant::now();
    active.shutdown();
    assert!(shutdown_started.elapsed() < Duration::from_secs(1));
}

#[test]
fn live_control_ping_times_out_without_accepting_late_pongs() {
    let (client, mut server) = socket_pair();
    let ping = NvstControlPing::default();
    let mut active = active_session(client, &ping);
    let first = client_ping(&mut server);
    server.send(Message::Pong(first.into())).unwrap();
    wait_until(|| ping.ping_ms(Instant::now()).is_some());
    let missing = client_ping(&mut server);
    server.send(Message::Pong(vec![0; 8].into())).unwrap();
    let next = client_ping(&mut server);
    server.send(Message::Pong(vec![0; 8].into())).unwrap();
    assert_eq!(
        u64::from_be_bytes(next.clone().try_into().unwrap()),
        u64::from_be_bytes(missing.clone().try_into().unwrap()) + 1
    );
    assert_eq!(ping.ping_ms(Instant::now()), None);
    server.send(Message::Pong(missing.into())).unwrap();
    server_ping(&mut server);
    assert_eq!(ping.ping_ms(Instant::now()), None);
    server.send(Message::Pong(next.into())).unwrap();
    wait_until(|| ping.ping_ms(Instant::now()).is_some());
    drop(server);
    wait_until(|| active.worker.as_ref().unwrap().is_finished());
    assert_eq!(ping.ping_ms(Instant::now()), None);
    active.shutdown();
}

#[test]
fn unanswered_websocket_ping_does_not_end_the_session() {
    let (client, mut server) = socket_pair();
    let ping = NvstControlPing::default();
    let mut active = active_session(client, &ping);
    let first = client_ping(&mut server);
    server.send(Message::Pong(vec![0; 8].into())).unwrap();
    let second = client_ping(&mut server);
    server.send(Message::Pong(vec![0; 8].into())).unwrap();
    assert_ne!(first, second);
    assert_eq!(ping.ping_ms(Instant::now()), None);
    assert!(!active.worker.as_ref().unwrap().is_finished());
    let shutdown = thread::spawn(move || active.shutdown());
    assert!(matches!(server.read().unwrap(), Message::Close(_)));
    shutdown.join().unwrap();
}

#[test]
fn live_control_ping_shutdown_closes_without_rtsp_teardown() {
    let (client, mut server) = socket_pair();
    let ping = NvstControlPing::default();
    let mut active = active_session(client, &ping);
    client_ping(&mut server);
    let shutdown_started = Instant::now();
    let shutdown = thread::spawn(move || active.shutdown());
    assert!(matches!(server.read().unwrap(), Message::Close(_)));
    shutdown.join().unwrap();
    assert!(shutdown_started.elapsed() < Duration::from_secs(1));
    assert_eq!(ping.ping_ms(Instant::now()), None);
}

#[test]
fn live_control_ping_shutdown_does_not_wait_for_pending_pong() {
    let (client, mut server) = socket_pair();
    let ping = NvstControlPing::default();
    let mut active = active_session(client, &ping);
    client_ping(&mut server);
    let shutdown_started = Instant::now();
    let shutdown = thread::spawn(move || active.shutdown());
    assert!(matches!(server.read().unwrap(), Message::Close(_)));
    assert_eq!(ping.ping_ms(Instant::now()), None);
    shutdown.join().unwrap();
    assert!(shutdown_started.elapsed() < Duration::from_secs(1));
}

#[test]
fn live_control_ping_shutdown_clears_sample() {
    let (client, mut server) = socket_pair();
    let ping = NvstControlPing::default();
    let mut active = active_session(client, &ping);
    let payload = client_ping(&mut server);
    server.send(Message::Pong(payload.into())).unwrap();
    wait_until(|| ping.ping_ms(Instant::now()).is_some());
    let shutdown_started = Instant::now();
    let shutdown = thread::spawn(move || active.shutdown());
    assert!(matches!(server.read().unwrap(), Message::Close(_)));
    assert_eq!(ping.ping_ms(Instant::now()), None);
    shutdown.join().unwrap();
    assert!(shutdown_started.elapsed() < Duration::from_secs(1));
}

#[test]
fn live_control_ping_rejects_unbounded_partial_response() {
    let (client, mut server) = socket_pair();
    let ping = NvstControlPing::default();
    let mut active = active_session(client, &ping);
    let payload = client_ping(&mut server);
    server.send(Message::Pong(payload.into())).unwrap();
    wait_until(|| ping.ping_ms(Instant::now()).is_some());
    server
        .send(Message::Text("x".repeat(MAX_CONTROL_RESPONSE_BYTES).into()))
        .unwrap();
    server.send(Message::Text("x".into())).unwrap();
    wait_until(|| active.worker.as_ref().unwrap().is_finished());
    assert_eq!(ping.ping_ms(Instant::now()), None);
    active.shutdown();
}
