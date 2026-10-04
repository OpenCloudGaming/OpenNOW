use super::*;

fn session(url: &str) -> Session {
    serde_json::from_value(
        json!({"sessionId":"seat/one","serverIp":"192.0.2.1","signalingUrl":url}),
    )
    .unwrap()
}

#[test]
fn sign_in_normalizes_path_query_and_reconnect() {
    let mut session = session("wss://example.test/nvst/?old=ignored");
    assert_eq!(
        sign_in_url(&session, "peer one").unwrap(),
        "wss://example.test/nvst/sign_in?peer_id=peer%20one&version=2&peer_role=1&pairing_id=seat%2Fone"
    );
    session.extra.insert(
        "signalingUrl".to_owned(),
        json!("wss://example.test/nvst/sign_in"),
    );
    session
        .extra
        .insert("signalingReconnect".to_owned(), json!(true));
    let url = sign_in_url(&session, "peer").unwrap();
    assert!(url.contains("version=3.0"));
    assert!(url.ends_with("&reconnect=1"));
    assert!(!url.contains("sign_in/sign_in"));
}

#[test]
fn signaling_rejects_insecure_and_credential_urls() {
    for url in [
        "http://example.test/nvst/",
        "ws://example.test/nvst/",
        "wss://user:password@example.test/nvst/",
        "wss://example.test/nvst/#fragment",
    ] {
        assert!(sign_in_url(&session(url), "peer").is_err());
    }
}

#[test]
fn peer_info_ack_heartbeat_and_nested_answer_match_wire_contract() {
    let mut protocol = Protocol::new("peer-local".to_owned());
    let info = protocol.peer_info(1920, 1080);
    assert_eq!(info["peer_info"]["resolution"], "1920x1080");
    assert_eq!(info["peer_info"]["peerRole"], 0);
    let (replies, _) = protocol
        .receive(r#"{"ackid":1,"peer_info":{"id":7,"name":"peer-local"}}"#)
        .unwrap();
    assert!(replies.is_empty());
    let (replies, _) = protocol.receive(r#"{"ackid":2,"hb":1}"#).unwrap();
    assert_eq!(replies, vec![json!({"ack":2}), json!({"hb":1})]);
    let offer = json!({"peer_msg":{"from":9,"to":7,"msg":json!({"type":"offer","sdp":"v=0\r\n"}).to_string()},"ackid":3});
    let (replies, incoming) = protocol.receive(&offer.to_string()).unwrap();
    assert_eq!(replies, vec![json!({"ack":3})]);
    assert!(matches!(incoming, Some(Incoming::Offer(sdp)) if sdp == "v=0\r\n"));
    let answer =
        protocol.peer_message(json!({"type":"answer","sdp":"local","nvstSdp":"attributes"}));
    assert_eq!(answer["peer_msg"]["from"], 7);
    assert_eq!(answer["peer_msg"]["to"], 9);
    let nested: Value = serde_json::from_str(answer["peer_msg"]["msg"].as_str().unwrap()).unwrap();
    assert_eq!(nested["nvstSdp"], "attributes");
}

#[test]
fn candidate_defaults_and_terminal_messages_are_parsed() {
    let mut protocol = Protocol::new("peer".to_owned());
    let candidate = json!({"peer_msg":{"from":1,"msg":json!({"candidate":"candidate:1 1 udp 1 127.0.0.1 9999 typ host"}).to_string()}});
    let (_, incoming) = protocol.receive(&candidate.to_string()).unwrap();
    assert!(
        matches!(incoming, Some(Incoming::Candidate(candidate)) if candidate.sdp_m_line_index == Some(0))
    );
    for packet in [
        json!({"error":"peerRemoved"}),
        json!({"peer_msg":{"msg":"BYE"}}),
    ] {
        assert!(matches!(
            protocol.receive(&packet.to_string()).unwrap().1,
            Some(Incoming::Closed)
        ));
    }
    assert!(protocol.receive("not JSON").is_err());
    assert!(
        protocol
            .receive(&"x".repeat(MAX_SIGNALING_BYTES + 1))
            .is_err()
    );
}

#[derive(Default)]
struct NonblockingStream {
    incoming: std::io::Cursor<Vec<u8>>,
    written: Vec<u8>,
    write_budget: usize,
}

impl Read for NonblockingStream {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        let count = self.incoming.read(output)?;
        if count == 0 {
            return Err(std::io::Error::from(ErrorKind::WouldBlock));
        }
        Ok(count)
    }
}

impl Write for NonblockingStream {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.write_budget == 0 {
            return Err(std::io::Error::from(ErrorKind::WouldBlock));
        }
        let count = bytes.len().min(self.write_budget);
        self.written.extend_from_slice(&bytes[..count]);
        self.write_budget -= count;
        Ok(count)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn nonblocking_signaling(
    write_buffer_size: usize,
    max_write_buffer_size: usize,
) -> Signaling<NonblockingStream> {
    Signaling {
        socket: WebSocket::from_raw_socket(
            NonblockingStream::default(),
            tungstenite::protocol::Role::Client,
            Some(
                WebSocketConfig::default()
                    .write_buffer_size(write_buffer_size)
                    .max_write_buffer_size(max_write_buffer_size),
            ),
        ),
        protocol: Protocol::new("fixture".to_owned()),
        heartbeat_at: Instant::now(),
        write_pending_since: None,
    }
}

#[test]
fn partial_writes_are_flushed_once_in_order_without_resending_messages() {
    for write_buffer_size in [0, 128] {
        let mut signaling = nonblocking_signaling(write_buffer_size, 1024);
        signaling.socket.get_mut().write_budget = 3;
        signaling.send(json!({"order":1})).unwrap();
        let pending_since = signaling.write_pending_since.unwrap();
        signaling.send(json!({"order":2})).unwrap();
        assert_eq!(signaling.write_pending_since, Some(pending_since));
        for _ in 0..8 {
            assert!(signaling.poll().unwrap().is_none());
        }
        assert_eq!(signaling.socket.get_ref().written.len(), 3);
        signaling.socket.get_mut().write_budget = usize::MAX;
        assert!(signaling.poll().unwrap().is_none());
        assert!(signaling.write_pending_since.is_none());
        let bytes = signaling.socket.get_ref().written.clone();
        let mut server = WebSocket::from_raw_socket(
            std::io::Cursor::new(bytes),
            tungstenite::protocol::Role::Server,
            None,
        );
        for order in [1, 2] {
            let message: Value =
                serde_json::from_str(&server.read().unwrap().into_text().unwrap()).unwrap();
            assert_eq!(message, json!({"order":order}));
        }
        assert!(server.read().is_err());
    }
}

#[test]
fn incomplete_incoming_message_does_not_prevent_pending_write_flush() {
    let mut signaling = nonblocking_signaling(0, 1024);
    signaling.socket.get_mut().incoming = std::io::Cursor::new(vec![0x01, 0x01, b'{']);
    assert!(signaling.poll().unwrap().is_none());
    signaling.send(json!({"hb":1})).unwrap();
    assert!(signaling.write_pending_since.is_some());
    signaling.socket.get_mut().write_budget = usize::MAX;
    assert!(signaling.poll().unwrap().is_none());
    assert!(signaling.write_pending_since.is_none());
    assert!(!signaling.socket.get_ref().written.is_empty());
}

#[test]
fn pending_writes_have_a_fixed_deadline_and_a_bounded_buffer() {
    let mut signaling = nonblocking_signaling(0, 128);
    signaling.send(json!("a".repeat(80))).unwrap();
    assert!(
        signaling
            .send(json!("b".repeat(80)))
            .unwrap_err()
            .message
            .contains("buffer exceeds")
    );
    signaling.write_pending_since = Some(Instant::now() - WRITE_TIMEOUT);
    assert!(
        signaling
            .poll()
            .err()
            .unwrap()
            .message
            .contains("timed out")
    );
    let mut signaling = nonblocking_signaling(0, 128);
    assert!(
        signaling
            .send(json!("a".repeat(MAX_SIGNALING_BYTES)))
            .unwrap_err()
            .message
            .contains("message exceeds")
    );
    assert!(signaling.socket.get_ref().written.is_empty());
}
