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
    ActiveNvstRtspSession::spawn(
        client,
        "rtsps://seat.nvidiagrid.net:322".to_owned(),
        vec![
            ("X-GS-Version", "14.2".to_owned()),
            ("x-nv-sessionid", "nv-session".to_owned()),
        ],
        "rtsp-session".to_owned(),
        ping.clone(),
    )
    .unwrap()
}

fn read_request(server: &mut WebSocket<TcpStream>, method: &str) -> u64 {
    let message = server.read().unwrap();
    let Message::Text(text) = message else {
        panic!("expected RTSP request, received {message:?}");
    };
    assert!(text.starts_with(&format!(
        "{method} rtsps://seat.nvidiagrid.net:322 RTSP/1.0\r\n"
    )));
    assert!(text.contains("\r\nSession: rtsp-session\r\n"));
    assert!(text.contains("\r\nX-GS-Version: 14.2\r\n"));
    assert!(text.contains("\r\nx-nv-sessionid: nv-session\r\n"));
    let cseq = text
        .lines()
        .find_map(|line| line.strip_prefix("CSeq: "))
        .unwrap()
        .parse()
        .unwrap();
    assert!(text.contains(&format!("\r\nRequest-Id: {cseq}\r\n")));
    cseq
}

fn response(server: &mut WebSocket<TcpStream>, cseq: u64, status: u16) {
    server
        .send(Message::Text(
            format!("RTSP/1.0 {status} Response\r\nCSeq: {cseq}\r\n\r\n").into(),
        ))
        .unwrap();
}

fn server_ping(server: &mut WebSocket<TcpStream>) {
    server.send(Message::Ping(vec![1, 2, 3].into())).unwrap();
    assert_eq!(server.read().unwrap(), Message::Pong(vec![1, 2, 3].into()));
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while !condition() {
        assert!(Instant::now() < deadline, "worker did not respond promptly");
        thread::sleep(Duration::from_millis(2));
    }
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
fn live_control_ping_uses_session_rtsp_and_accepts_551_not_wrong_sequences() {
    let (client, mut server) = socket_pair();
    let ping = NvstControlPing::default();
    let mut active = active_session(client, &ping);
    server_ping(&mut server);
    let cseq = read_request(&mut server, "GET_PARAMETER");
    assert_eq!(cseq, 8);
    response(&mut server, cseq + 1, 551);
    server_ping(&mut server);
    assert_eq!(ping.ping_ms(Instant::now()), None);
    server
        .send(Message::Binary(
            format!("RTSP/1.0 551 Option Not Supported\r\nRequest-Id: {cseq}\r\n")
                .into_bytes()
                .into(),
        ))
        .unwrap();
    server
        .send(Message::Binary(b"\r\n".to_vec().into()))
        .unwrap();
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
fn live_control_ping_times_out_without_accepting_late_replies() {
    let (client, mut server) = socket_pair();
    let ping = NvstControlPing::default();
    let mut active = active_session(client, &ping);
    let first = read_request(&mut server, "GET_PARAMETER");
    response(&mut server, first, 200);
    wait_until(|| ping.ping_ms(Instant::now()).is_some());
    let missing = read_request(&mut server, "GET_PARAMETER");
    assert_eq!(missing, first + 1);
    let next = read_request(&mut server, "GET_PARAMETER");
    assert_eq!(next, missing + 1);
    assert_eq!(ping.ping_ms(Instant::now()), None);
    response(&mut server, missing, 551);
    server_ping(&mut server);
    assert_eq!(ping.ping_ms(Instant::now()), None);
    response(&mut server, next, 551);
    wait_until(|| ping.ping_ms(Instant::now()).is_some());
    drop(server);
    wait_until(|| active.worker.as_ref().unwrap().is_finished());
    assert_eq!(ping.ping_ms(Instant::now()), None);
    active.shutdown();
}

#[test]
fn live_control_ping_shutdown_sends_teardown_with_pending_probe() {
    let (client, mut server) = socket_pair();
    let ping = NvstControlPing::default();
    let mut active = active_session(client, &ping);
    let cseq = read_request(&mut server, "GET_PARAMETER");
    let shutdown_started = Instant::now();
    let shutdown = thread::spawn(move || active.shutdown());
    assert_eq!(read_request(&mut server, "TEARDOWN"), cseq + 1);
    shutdown.join().unwrap();
    assert!(shutdown_started.elapsed() < Duration::from_secs(2));
    assert_eq!(ping.ping_ms(Instant::now()), None);
}

#[test]
fn live_control_ping_shutdown_waits_past_pending_probe_reply_for_teardown() {
    let (client, mut server) = socket_pair();
    let ping = NvstControlPing::default();
    let mut active = active_session(client, &ping);
    let probe = read_request(&mut server, "GET_PARAMETER");
    let shutdown_started = Instant::now();
    let shutdown = thread::spawn(move || active.shutdown());
    let teardown = read_request(&mut server, "TEARDOWN");
    response(&mut server, probe, 551);
    server_ping(&mut server);
    assert!(!shutdown.is_finished());
    assert_eq!(ping.ping_ms(Instant::now()), None);
    response(&mut server, teardown, 200);
    shutdown.join().unwrap();
    assert!(shutdown_started.elapsed() < Duration::from_secs(1));
}

#[test]
fn live_control_ping_shutdown_clears_sample_and_accepts_teardown_response() {
    let (client, mut server) = socket_pair();
    let ping = NvstControlPing::default();
    let mut active = active_session(client, &ping);
    let cseq = read_request(&mut server, "GET_PARAMETER");
    response(&mut server, cseq, 551);
    wait_until(|| ping.ping_ms(Instant::now()).is_some());
    let shutdown_started = Instant::now();
    let shutdown = thread::spawn(move || active.shutdown());
    let teardown = read_request(&mut server, "TEARDOWN");
    assert_eq!(ping.ping_ms(Instant::now()), None);
    response(&mut server, teardown, 200);
    shutdown.join().unwrap();
    assert!(shutdown_started.elapsed() < Duration::from_secs(1));
}

#[test]
fn live_control_ping_rejects_unbounded_partial_response() {
    let (client, mut server) = socket_pair();
    let ping = NvstControlPing::default();
    let mut active = active_session(client, &ping);
    let cseq = read_request(&mut server, "GET_PARAMETER");
    response(&mut server, cseq, 551);
    wait_until(|| ping.ping_ms(Instant::now()).is_some());
    server
        .send(Message::Text("x".repeat(MAX_CONTROL_RESPONSE_BYTES).into()))
        .unwrap();
    server.send(Message::Text("x".into())).unwrap();
    wait_until(|| active.worker.as_ref().unwrap().is_finished());
    assert_eq!(ping.ping_ms(Instant::now()), None);
    active.shutdown();
}
