use super::*;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::time::Instant;

struct Exchange {
    path: &'static str,
    body: Value,
}

fn catalog_fixture(
    exchanges: Vec<Exchange>,
    inspect: impl Fn(usize, &str, &[u8]) + Send + 'static,
) -> (GfnService, tempfile::TempDir, std::thread::JoinHandle<()>) {
    let host = "prod.test.geforcenow.nvidiagrid.net";
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec![host.to_owned()]).unwrap();
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(
        vec![cert.der().clone()],
        rustls::pki_types::PrivatePkcs8KeyDer::from(signing_key.serialize_der()).into(),
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let base = format!("https://{host}:{}/", address.port());
    let worker = std::thread::spawn(move || {
        let config = Arc::new(config);
        for (index, exchange) in exchanges.into_iter().enumerate() {
            let deadline = Instant::now() + Duration::from_secs(10);
            let stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "missing catalog HTTP request");
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("{error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut stream = rustls::StreamOwned::new(
                rustls::ServerConnection::new(config.clone()).unwrap(),
                stream,
            );
            let mut reader = BufReader::new(&mut stream);
            let mut headers = String::new();
            let mut length = 0;
            loop {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse::<usize>().unwrap();
                }
                headers.push_str(&line);
            }
            assert_eq!(headers.split_whitespace().nth(1), Some(exchange.path));
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            inspect(index, &headers, &body);
            let response = exchange.body.to_string();
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{response}", response.len()).unwrap();
            stream.flush().unwrap();
        }
    });
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .resolve(host, address)
        .add_root_certificate(reqwest::Certificate::from_der(cert.der()).unwrap())
        .build()
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let service = GfnService::with_client(
        client,
        Endpoints {
            library_catalog: format!("{base}graphql"),
            token: format!("{base}token"),
            ..Endpoints::default()
        },
        directory.path().to_owned(),
    );
    let mut session: AuthSession = serde_json::from_value(json!({
        "provider":LoginProvider::default_nvidia(),
        "tokens":{"accessToken":"test-access","idToken":"test-id","refreshToken":"test-refresh",
            "clientToken":"test-client","authClientId":"test-client-id",
            "expiresAt":now_ms()+3_600_000,"clientTokenExpiresAt":now_ms()+3_600_000},
        "user":{"userId":"test-account","displayName":"Test","membershipTier":"FREE"}
    }))
    .unwrap();
    session.provider.code = "PARTNER".to_owned();
    session.provider.streaming_service_url = base;
    service.state.lock().unwrap().session = Some(session);
    (service, directory, worker)
}

fn server_info() -> Exchange {
    Exchange {
        path: "/v2/serverInfo",
        body: json!({"vpcId":"PARTNER-CATALOG","requestStatus":{"serverId":"different-server"}}),
    }
}

fn app(id: u64, title: &str) -> Value {
    json!({"id":id,"title":title,"variants":[{"id":id+1000,"appStore":"Steam","gfn":{"status":"AVAILABLE","library":{"status":"OWNED","selected":true}}}]})
}

fn library_page(items: Vec<Value>, next: Option<&str>) -> Exchange {
    Exchange {
        path: "/graphql",
        body: json!({"data":{"apps":{"items":items,"pageInfo":{
            "hasNextPage":next.is_some(),"endCursor":next,"totalCount":1200
        }}}}),
    }
}

#[test]
fn alliance_library_uses_discovered_vpc_and_preserves_numeric_ids_and_ownership() {
    let (service, _directory, server) = catalog_fixture(
        vec![
            server_info(),
            library_page(vec![app(42, "Partner game")], Some("next-page")),
        ],
        |index, headers, body| {
            assert!(
                headers
                    .to_ascii_lowercase()
                    .contains("authorization: gfnjwt test-id\r\n")
            );
            if index == 1 {
                let request: Value = serde_json::from_slice(body).unwrap();
                assert_eq!(request["variables"]["vpcId"], "PARTNER-CATALOG");
                assert_eq!(request["variables"]["fetchCount"], 100);
                assert_eq!(request["variables"]["cursor"], "");
                assert_eq!(
                    request["variables"]["filters"]["variants"]["gfn"]["library"]["status"]["notEquals"],
                    "NOT_OWNED"
                );
            }
        },
    );
    let result = service
        .library_catalog(&json!({"limit":1000}), &json!({}))
        .unwrap();
    assert_eq!(result["games"][0]["id"], "42");
    assert_eq!(result["games"][0]["launchAppId"], "1042");
    assert_eq!(result["games"][0]["isInLibrary"], true);
    assert_eq!(result["source"], "account-library");
    assert_eq!(result["hasNextPage"], true);
    assert_eq!(result["nextCursor"], "next-page");
    server.join().unwrap();
}

#[test]
fn library_pages_shrink_without_skipping_the_original_cursor_or_exceeding_ipc() {
    let title = "x".repeat(20_000);
    let mut exchanges = vec![server_info()];
    for count in [100, 50, 25, 12] {
        exchanges.push(library_page(
            (0..count).map(|id| app(id, &title)).collect(),
            Some(&format!("after-{count}")),
        ));
    }
    let (service, _directory, server) = catalog_fixture(exchanges, |index, _, body| {
        if index > 0 {
            let request: Value = serde_json::from_slice(body).unwrap();
            assert_eq!(
                request["variables"]["fetchCount"],
                [100, 50, 25, 12][index - 1]
            );
            assert_eq!(request["variables"]["cursor"], "original-cursor");
        }
    });
    let result = service
        .library_catalog(&json!({"limit":100,"cursor":"original-cursor"}), &json!({}))
        .unwrap();
    assert_eq!(result["games"].as_array().unwrap().len(), 12);
    assert_eq!(result["nextCursor"], "after-12");
    assert!(serde_json::to_vec(&result).unwrap().len() <= crate::catalog_page::RESULT_BUDGET);
    server.join().unwrap();
}

#[test]
fn library_search_keeps_empty_filtered_pages_and_server_pagination() {
    let (service, _directory, server) = catalog_fixture(
        vec![
            server_info(),
            library_page(vec![app(1, "Other game")], Some("after-other")),
            library_page(vec![app(2, "Wanted game")], None),
        ],
        |index, _, body| {
            if index > 0 {
                let request: Value = serde_json::from_slice(body).unwrap();
                assert_eq!(
                    request["variables"]["cursor"],
                    if index == 1 { "" } else { "after-other" }
                );
            }
        },
    );
    let first = service
        .library_catalog(&json!({"searchQuery":"wanted"}), &json!({}))
        .unwrap();
    assert_eq!(first["count"], 0);
    assert_eq!(first["hasNextPage"], true);
    let last = service
        .library_catalog(
            &json!({"searchQuery":"wanted","cursor":first["nextCursor"]}),
            &json!({}),
        )
        .unwrap();
    assert_eq!(last["games"][0]["title"], "Wanted game");
    assert_eq!(last["hasNextPage"], false);
    server.join().unwrap();
}

#[test]
fn library_rejects_missing_data_and_nonadvancing_pages_instead_of_empty_success() {
    for body in [
        json!({}),
        json!({"data":{"apps":null}}),
        json!({"data":{"apps":{"items":[]}}}),
        json!({"data":{"apps":{"items":[],"pageInfo":{"hasNextPage":true,"endCursor":"same"}}}}),
    ] {
        let (service, _directory, server) = catalog_fixture(
            vec![
                server_info(),
                Exchange {
                    path: "/graphql",
                    body,
                },
            ],
            |_, _, _| {},
        );
        let error = service
            .library_catalog(&json!({"cursor":"same"}), &json!({}))
            .unwrap_err();
        assert_eq!(error.code, "invalid_upstream_response");
        server.join().unwrap();
    }
}

#[test]
fn explicit_catalog_vpcs_precede_server_ids_and_legacy_responses_still_work() {
    for (payload, expected) in [
        (
            json!({"vpcId":"ally","vpc_id":"snake","requestStatus":{"serverId":"legacy"}}),
            Some("ally"),
        ),
        (
            json!({"vpcId":" ","vpc_id":"snake","requestStatus":{"serverId":"legacy"}}),
            Some("snake"),
        ),
        (
            json!({"requestStatus":{"serverId":"NPA-TKC-IST-01"}}),
            Some("NPA-TKC-IST-01"),
        ),
        (
            json!({"requestStatus":{"serverId":"NP-NWK-03"}}),
            Some("NP-NWK-03"),
        ),
        (json!({}), None),
    ] {
        assert_eq!(server_catalog_vpc(&payload).as_deref(), expected);
    }
}

#[test]
fn library_ownership_matches_mac_for_server_defined_states_and_selected_variants() {
    for (status, selected, owned) in [
        ("MANUAL", false, true),
        ("PLATFORM_SYNC", false, true),
        ("OWNED", false, true),
        (" installed ", false, true),
        ("NOT_OWNED", false, false),
        ("not_owned", false, false),
        ("", false, false),
        ("", true, true),
        ("NOT_OWNED", true, true),
    ] {
        let mut item = app(1, "Test");
        item["variants"][0]["gfn"]["library"] = json!({"status":status,"selected":selected});
        let game = app_to_game(&item).unwrap();
        assert_eq!(game["isInLibrary"], owned);
        assert_eq!(game["variants"][0]["libraryStatus"], status);
    }
    for invalid in [Value::Null, json!(true), json!(-5), json!(1.5), json!(" ")] {
        let mut item = app(1, "Test");
        item["variants"][0]["id"] = invalid;
        assert!(app_to_game(&item).is_none());
    }
}

fn token(expiry: u64) -> String {
    format!(
        "header.{}.signature",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
            json!({
                "sub":"test-account","email":"test@example.invalid","exp":expiry
            })
            .to_string()
        )
    )
}

#[test]
fn expired_id_tokens_are_refreshed_even_with_an_unexpired_access_token() {
    let refreshed = token(now_ms() / 1000 + 3600);
    let (service, _directory, server) = catalog_fixture(
        vec![Exchange {
            path: "/token",
            body: json!({
                "access_token":refreshed,"expires_in":3600
            }),
        }],
        |_, _, body| {
            let form: HashMap<_, _> = url::form_urlencoded::parse(body).collect();
            assert_eq!(
                form["grant_type"],
                "urn:ietf:params:oauth:grant-type:client_token"
            );
        },
    );
    {
        let mut state = service.state.lock().unwrap();
        let tokens = &mut state.session.as_mut().unwrap().tokens;
        tokens.id_token = Some(token(now_ms() / 1000 - 60));
        assert_eq!(tokens.session_token(), "test-access");
    }
    let result = service.session().unwrap();
    assert_eq!(result["refresh"]["outcome"], "refreshed");
    assert!(result["session"]["tokens"].get("idToken").is_none());
    assert_eq!(result["session"]["tokens"]["accessToken"], refreshed);
    server.join().unwrap();
}

#[test]
fn proxy_route_changes_do_not_reuse_direct_catalog_vpc_metadata() {
    let (service, _directory, server) = catalog_fixture(vec![server_info()], |_, _, _| {});
    let session = service.state.lock().unwrap().session.clone().unwrap();
    assert_eq!(
        service
            .vpc_id(&session, "test-id", &json!({}), None)
            .unwrap(),
        "PARTNER-CATALOG"
    );
    server.join().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let proxy = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut stream: TcpStream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        Instant::now() < deadline,
                        "proxy was bypassed or the direct VPC was reused"
                    );
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("{error}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut reader = BufReader::new(&mut stream);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert!(line.starts_with("CONNECT prod.test.geforcenow.nvidiagrid.net:"));
        loop {
            line.clear();
            assert!(reader.read_line(&mut line).unwrap() > 0);
            if line == "\r\n" {
                break;
            }
        }
        write!(
            stream,
            "HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
    });
    let settings =
        json!({"sessionProxyEnabled":true,"sessionProxyUrl":format!("http://{address}")});
    assert_eq!(
        service
            .vpc_id(&session, "test-id", &settings, None)
            .unwrap(),
        "GFN-PC"
    );
    proxy.join().unwrap();
}

#[test]
fn library_cancellation_after_a_response_never_returns_a_completed_page() {
    let requests = Arc::new(crate::requests::Requests::default());
    let permit = requests.admit("library", "catalog.library.list").unwrap();
    let cancellation = requests.clone();
    let (service, _directory, server) = catalog_fixture(
        vec![server_info(), library_page(vec![app(1, "Test")], None)],
        move |index, _, _| {
            if index == 1 {
                cancellation.cancel("library");
            }
        },
    );
    let error = crate::requests::scope(permit.token.clone(), || {
        service.library_catalog(&json!({}), &json!({}))
    })
    .unwrap_err();
    assert_eq!(error.code, "cancelled");
    server.join().unwrap();
}

#[test]
fn library_rejects_a_server_ignoring_the_requested_page_size() {
    let (service, _directory, server) = catalog_fixture(
        vec![
            server_info(),
            library_page(vec![app(1, "One"), app(2, "Two")], None),
        ],
        |_, _, _| {},
    );
    assert_eq!(
        service
            .library_catalog(&json!({"limit":1}), &json!({}))
            .unwrap_err()
            .code,
        "invalid_upstream_response"
    );
    server.join().unwrap();
}

#[test]
fn session_token_prefers_only_a_nonempty_unexpired_id_token() {
    let (service, _directory, server) = catalog_fixture(vec![], |_, _, _| {});
    let mut tokens = service
        .state
        .lock()
        .unwrap()
        .session
        .clone()
        .unwrap()
        .tokens;
    let valid = token(now_ms() / 1000 + 3600);
    for (id, expected) in [
        (Some(valid.clone()), valid.as_str()),
        (Some(token(now_ms() / 1000 - 60)), "test-access"),
        (Some(String::new()), "test-access"),
        (None, "test-access"),
    ] {
        tokens.id_token = id;
        assert_eq!(tokens.session_token(), expected);
    }
    server.join().unwrap();
}
