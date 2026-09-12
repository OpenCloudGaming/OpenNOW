use super::*;
use std::net::TcpListener;

fn exchange_over_tls(
    server_versions: &[&'static rustls::SupportedProtocolVersion],
    expected_version: rustls::ProtocolVersion,
) {
    ensure_tls_crypto_provider().unwrap();
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).unwrap();
    let server_config = rustls::ServerConfig::builder_with_protocol_versions(server_versions)
        .with_no_client_auth()
        .with_single_cert(
            vec![cert.der().clone()],
            rustls::pki_types::PrivatePkcs8KeyDer::from(signing_key.serialize_der()).into(),
        )
        .unwrap();
    let mut roots = rustls::RootCertStore::empty();
    roots.add(cert.der().clone()).unwrap();
    let client_config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let (server_stream, _) = listener.accept().unwrap();
    server_stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    server_stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let server = thread::spawn(move || {
        let connection = rustls::ServerConnection::new(Arc::new(server_config)).unwrap();
        let mut socket =
            tungstenite::accept(rustls::StreamOwned::new(connection, server_stream)).unwrap();
        assert_eq!(
            socket.get_ref().conn.protocol_version(),
            Some(expected_version)
        );
        let Message::Text(request) = socket.read().unwrap() else {
            panic!("expected an RTSP request");
        };
        assert!(request.starts_with("OPTIONS rtsps://localhost:322 RTSP/1.0\r\n"));
        assert!(request.contains("\r\nCSeq: 1\r\n"));
        socket
            .send(Message::Text(
                "RTSP/1.0 200 OK\r\nCSeq: 1\r\nContent-Length: 0\r\n\r\n".into(),
            ))
            .unwrap();
    });
    let (socket, _) = tungstenite::client_tls_with_config(
        format!("wss://localhost:{}/rtsp", address.port()),
        stream,
        None,
        Some(tungstenite::Connector::Rustls(Arc::new(client_config))),
    )
    .unwrap();
    let MaybeTlsStream::Rustls(tls) = socket.get_ref() else {
        panic!("RTSPS must remain encrypted");
    };
    assert_eq!(tls.conn.protocol_version(), Some(expected_version));
    let mut client = RtspClient {
        socket,
        cseq: 0,
        buffer: String::new(),
    };
    let response = client
        .request("OPTIONS", "rtsps://localhost:322", &[], "")
        .unwrap();
    assert_eq!(response.status, 200);
    server.join().unwrap();
}

#[test]
fn rtsps_exchanges_requests_with_tls12_only_servers() {
    exchange_over_tls(&[&rustls::version::TLS12], rustls::ProtocolVersion::TLSv1_2);
}

#[test]
fn rtsps_exchanges_requests_with_tls13_only_servers() {
    exchange_over_tls(&[&rustls::version::TLS13], rustls::ProtocolVersion::TLSv1_3);
}

#[test]
fn rtsps_prefers_tls13_when_both_versions_are_available() {
    exchange_over_tls(rustls::DEFAULT_VERSIONS, rustls::ProtocolVersion::TLSv1_3);
}
