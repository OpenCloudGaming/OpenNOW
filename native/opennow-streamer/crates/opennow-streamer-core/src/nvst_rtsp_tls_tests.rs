use super::*;
use std::net::TcpListener;

fn held_open_startup_peer(tls_upgrade: bool, cancel: bool) {
    use std::io::Read;
    ensure_tls_crypto_provider().unwrap();
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_owned()]).unwrap();
    let server_config = rustls::ServerConfig::builder()
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
    let (ready, accepted) = mpsc::channel();
    let (release, hold) = mpsc::channel();
    let peer = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        drop(listener);
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut stream: Box<dyn Read + Send> = if tls_upgrade {
            Box::new(rustls::StreamOwned::new(
                rustls::ServerConnection::new(Arc::new(server_config)).unwrap(),
                stream,
            ))
        } else {
            Box::new(stream)
        };
        let mut data = [0; 4096];
        assert!(stream.read(&mut data).unwrap() > 0);
        ready.send(()).unwrap();
        hold.recv().unwrap();
        drop(stream);
    });
    let (completed, completion) = mpsc::channel();
    let cancellation = crate::EngineCancellation::default();
    let token = cancellation.token();
    let worker = thread::spawn(move || {
        let result = RtspClient::connect_with_options(
            &format!("rtsps://127.0.0.1:{}", address.port()),
            "held-open-test",
            Some(tungstenite::Connector::Rustls(Arc::new(client_config))),
            StartupBudget {
                token,
                deadline: Instant::now()
                    + if cancel {
                        Duration::from_secs(5)
                    } else {
                        Duration::from_millis(200)
                    },
            },
        )
        .map(|_| ())
        .map_err(|error| error.code);
        completed.send(result).unwrap();
    });
    accepted.recv_timeout(Duration::from_secs(5)).unwrap();
    if cancel {
        cancellation.cancel();
    }
    let result = completion.recv_timeout(Duration::from_secs(1));
    release.send(()).unwrap();
    peer.join().unwrap();
    worker.join().unwrap();
    assert_eq!(
        result.unwrap(),
        Err(if cancel {
            "nvst-start-aborted"
        } else {
            "nvst-startup-timeout"
        })
    );
}

#[test]
fn startup_deadline_bounds_silent_tcp_tls_peer_while_peer_stays_open() {
    held_open_startup_peer(false, false);
}

#[test]
fn startup_deadline_bounds_silent_tls_upgrade_peer_while_peer_stays_open() {
    held_open_startup_peer(true, false);
}

#[test]
fn startup_cancellation_interrupts_silent_tcp_tls_peer_while_peer_stays_open() {
    held_open_startup_peer(false, true);
}

#[test]
fn startup_cancellation_interrupts_silent_tls_upgrade_peer_while_peer_stays_open() {
    held_open_startup_peer(true, true);
}

fn exchange_over_tls(
    server_versions: &[&'static rustls::SupportedProtocolVersion],
    expected_version: rustls::ProtocolVersion,
    redirect: bool,
) {
    use std::io::{Read, Write};
    ensure_tls_crypto_provider().unwrap();
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_owned()]).unwrap();
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
    let server = thread::spawn(move || {
        let server_config = Arc::new(server_config);
        if redirect {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let connection = rustls::ServerConnection::new(server_config.clone()).unwrap();
            let mut stream = rustls::StreamOwned::new(connection, stream);
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
                assert!(request.len() <= 8192);
            }
            stream.write_all(format!("HTTP/1.1 302 Found\r\nLocation: wss://127.0.0.1:{}/redirected\r\nContent-Length: 0\r\n\r\n", address.port()).as_bytes()).unwrap();
            stream.flush().unwrap();
        }
        let (server_stream, _) = listener.accept().unwrap();
        server_stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        server_stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let connection = rustls::ServerConnection::new(server_config).unwrap();
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
    let (mut client, _) = RtspClient::connect_with_options(
        &format!("rtsps://127.0.0.1:{}", address.port()),
        "tls-version-test",
        Some(tungstenite::Connector::Rustls(Arc::new(client_config))),
        StartupBudget {
            token: crate::EngineCancellation::default().token(),
            deadline: Instant::now() + Duration::from_secs(5),
        },
    )
    .unwrap();
    let MaybeTlsStream::Rustls(tls) = client.socket.get_ref() else {
        panic!("RTSPS must remain encrypted");
    };
    assert_eq!(tls.conn.protocol_version(), Some(expected_version));
    let response = client
        .request("OPTIONS", "rtsps://localhost:322", &[], "")
        .unwrap();
    assert_eq!(response.status, 200);
    server.join().unwrap();
}

#[test]
fn rtsps_exchanges_requests_with_tls12_only_servers() {
    exchange_over_tls(
        &[&rustls::version::TLS12],
        rustls::ProtocolVersion::TLSv1_2,
        false,
    );
}

#[test]
fn rtsps_exchanges_requests_with_tls13_only_servers() {
    exchange_over_tls(
        &[&rustls::version::TLS13],
        rustls::ProtocolVersion::TLSv1_3,
        false,
    );
}

#[test]
fn rtsps_prefers_tls13_when_both_versions_are_available() {
    exchange_over_tls(
        rustls::DEFAULT_VERSIONS,
        rustls::ProtocolVersion::TLSv1_3,
        false,
    );
}

#[test]
fn rtsps_preserves_bounded_upgrade_redirects() {
    exchange_over_tls(
        rustls::DEFAULT_VERSIONS,
        rustls::ProtocolVersion::TLSv1_3,
        true,
    );
}

#[test]
fn partner_endpoint_does_not_bypass_tls_hostname_verification() {
    ensure_tls_crypto_provider().unwrap();
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).unwrap();
    let server_config = rustls::ServerConfig::builder()
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
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let connection = rustls::ServerConnection::new(Arc::new(server_config)).unwrap();
        let _ = tungstenite::accept(rustls::StreamOwned::new(connection, stream));
    });
    let stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let failure = tungstenite::client_tls_with_config(
        format!("wss://partner.example:{}/rtsp", address.port()),
        stream,
        None,
        Some(tungstenite::Connector::Rustls(Arc::new(client_config))),
    );
    assert!(failure.is_err());
    server.join().unwrap();
}
