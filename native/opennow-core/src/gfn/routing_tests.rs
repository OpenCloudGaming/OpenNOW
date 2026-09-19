use super::tests::{auth_fixture, jwt, mock_requests, pending_attempt, test_service};
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

fn service(url: &str) -> (GfnService, PathBuf) {
    let (mut service, path) = test_service(url);
    service.endpoints.service_urls = format!("{url}/providers");
    service.endpoints.server_info = Some(format!("{url}/v2/serverInfo"));
    service.endpoints.graphql = format!("{url}/graphql");
    service.endpoints.subscription = format!("{url}/subscription");
    let mut state = service.state.lock().unwrap();
    state.session = Some(auth_fixture("account-a"));
    state.generation = 7;
    state.restore_attempted = true;
    drop(state);
    (service, path)
}

fn directory(id: &str, host: &str) -> Value {
    json!({"gfnServiceInfo":{"gfnServiceEndpoints":[{
        "idpId":id,"loginProviderCode":"ALLIANCE","loginProviderDisplayName":"Alliance fixture",
        "streamingServiceUrl":format!("https://{host}/"),"loginProviderPriority":1
    }]}})
}

fn expire_discovery(service: &GfnService) {
    let mut state = service.state.lock().unwrap();
    state.providers_expires = None;
    state.providers_retry = None;
}

fn launch_params(id: &str) -> Value {
    json!({"appId":id,"variantId":id,"catalogAppId":"launch-fixture",
        "scope":scoped_result(json!({}), &auth_fixture("account-a"), 7)["scope"]})
}

fn launch_metadata() -> Vec<(u16, Value)> {
    vec![
        (200, json!({"requestStatus":{"serverId":"fixture-vpc"}})),
        (
            200,
            json!({"data":{"apps":{"items":[{"id":"launch-fixture","title":"Launch fixture",
            "gfn":{"playabilityState":"PLAYABLE"},"variants":[{"id":"123","appStore":"STEAM",
                "gfn":{"status":"AVAILABLE","library":{"status":"MANUAL","selected":true,"playStatus":"PLAYABLE"}}}]}]}}}),
        ),
        (
            200,
            json!({"data":{"appStoreDefinitions":[{"store":"STEAM","label":"Steam","features":[],"accountLinkingMetadata":{"isRequired":false}}]}}),
        ),
        (200, json!({"data":{"genreDefinitions":[]}})),
        (200, json!({"data":{"subscriptionDefinitions":[]}})),
        (
            200,
            json!({"data":{"userAccount":{"storesData":[],"subscriptions":[]}}}),
        ),
    ]
}

fn reject_concurrent_create_during(discovery: bool) {
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let mut responses = if discovery { vec![] } else { launch_metadata() };
    responses.push((503, json!({})));
    let (url, worker) = mock_requests(responses, move |index, request| {
        if !discovery && index < 6 {
            return;
        }
        if discovery {
            assert!(request.starts_with("GET /providers "));
        } else {
            assert!(request.starts_with("POST /v2/session?"));
            let body: Value =
                serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
            assert_eq!(body["sessionRequestData"]["accountLinked"], true);
        }
        entered_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    let (mut service, path) = service(&url);
    service
        .cloudmatch
        .set_test_control_base(url::Url::parse(&url).unwrap());
    if discovery {
        expire_discovery(&service);
    }
    let requests = std::sync::Arc::new(crate::requests::Requests::default());
    let permit = requests.admit("first-create", "session.create").unwrap();
    std::thread::scope(|threads| {
        let first = threads.spawn(|| {
            crate::requests::scope(permit.token.clone(), || {
                service.create_session(&launch_params("123"), &json!({}))
            })
        });
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        let service = &service;
        let second = threads.spawn(move || {
            result_tx
                .send(service.create_session(&launch_params("456"), &json!({})))
                .unwrap()
        });
        let rejected = result_rx.recv_timeout(Duration::from_secs(1));
        if discovery {
            requests.cancel("first-create");
        }
        release_tx.send(()).unwrap();
        assert_eq!(
            first.join().unwrap().unwrap_err().code,
            if discovery {
                "cancelled"
            } else {
                "upstream_error"
            }
        );
        assert_eq!(
            rejected
                .expect("concurrent RPC queued behind the first create")
                .unwrap_err()
                .code,
            "session_update_busy"
        );
        second.join().unwrap();
    });
    assert!(service.cloudmatch.active()["session"].is_null());
    assert!(service.cloudmatch.admit_create().is_ok());
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn concurrent_rpc_create_is_rejected_before_cancelled_provider_preflight_finishes() {
    reject_concurrent_create_during(true);
}

#[test]
fn concurrent_rpc_create_is_rejected_before_failed_allocation_post_finishes() {
    reject_concurrent_create_during(false);
}

#[test]
fn stopping_same_owner_renews_expired_service_id_before_exactly_one_delete() {
    for status in [204, 401, 503] {
        let renewed_id = jwt("account-a", now_ms() + 3_600_000);
        let expected_id = renewed_id.clone();
        let delete_count = std::sync::Arc::new(AtomicUsize::new(0));
        let observed = delete_count.clone();
        let (url, worker) = mock_requests(
            vec![
                (
                    200,
                    json!({"access_token":"renewed-access","id_token":renewed_id,"expires_in":3600}),
                ),
                (status, json!({})),
            ],
            move |index, request| {
                if index == 0 {
                    assert!(request.starts_with("POST / "));
                    assert!(request.contains("client_id=test-client-id"));
                } else {
                    assert!(request.starts_with("DELETE /v2/session/owned-seat HTTP/1.1"));
                    assert!(request.contains(&format!("GFNJWT {expected_id}")));
                    assert!(!request.contains("expired-service-id"));
                    observed.fetch_add(1, Ordering::SeqCst);
                }
            },
        );
        let (mut service, path) = service(&url);
        let mut owner = auth_fixture("account-a");
        owner.tokens.id_token = Some("expired-service-id".into());
        owner.tokens.id_token_expires_at = Some(now_ms() - 1);
        service.state.lock().unwrap().session = Some(owner.clone());
        service.session_routing.lock().unwrap().active_owner = Some(
            ActiveSeatOwner::capture(owner, 7, &json!({"sessionId":"owned-seat"}), None).unwrap(),
        );
        service
            .cloudmatch
            .set_test_control_base(url::Url::parse(&url).unwrap());
        service
            .cloudmatch
            .seed_owned_session(json!({"sessionId":"owned-seat","status":3}));
        let result = service.stop_session(
            &json!({"sessionId":"owned-seat","streamingBaseUrl":"https://foreign.nvidiagrid.net/"}),
            &json!({}),
        );
        if status == 204 {
            assert_eq!(result.unwrap()["scope"]["userId"], "account-a");
            assert!(
                service
                    .session_routing
                    .lock()
                    .unwrap()
                    .active_owner
                    .is_none()
            );
        } else {
            assert_eq!(
                result.unwrap_err().code,
                if status == 401 {
                    "http_unauthorized"
                } else {
                    "upstream_error"
                }
            );
            assert_eq!(
                service.cloudmatch.active()["session"]["sessionId"],
                "owned-seat"
            );
        }
        worker.join().unwrap();
        assert_eq!(delete_count.load(Ordering::SeqCst), 1);
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[test]
fn stopping_expired_foreign_owner_never_renews_or_deletes_with_selected_credentials() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (mut service, path) = service(&url);
    let mut owner = auth_fixture("account-a");
    owner.tokens.id_token = Some("expired-service-id".into());
    owner.tokens.id_token_expires_at = Some(now_ms() - 1);
    let mut selected = auth_fixture("account-b");
    selected.tokens.access_token = "foreign-access".into();
    service.state.lock().unwrap().session = Some(selected);
    service.state.lock().unwrap().generation = 8;
    service.session_routing.lock().unwrap().active_owner =
        Some(ActiveSeatOwner::capture(owner, 7, &json!({"sessionId":"owned-seat"}), None).unwrap());
    service
        .cloudmatch
        .set_test_control_base(url::Url::parse(&url).unwrap());
    service
        .cloudmatch
        .seed_owned_session(json!({"sessionId":"owned-seat","status":3}));
    assert_eq!(
        service
            .stop_session(&json!({"sessionId":"owned-seat"}), &json!({}))
            .unwrap_err()
            .code,
        "authentication_required"
    );
    assert_eq!(
        service
            .stop_session(&json!({"sessionId":"foreign-seat"}), &json!({}))
            .unwrap_err()
            .code,
        "session_owner_mismatch"
    );
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(
        service.cloudmatch.active()["session"]["sessionId"],
        "owned-seat"
    );
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn discovery_transport_schema_and_untrusted_endpoints_remain_retryable_failures() {
    for payload in [
        json!("invalid discovery"),
        json!({"gfnServiceInfo":{"gfnServiceEndpoints":[]}}),
        directory("outside", "outside.invalid"),
    ] {
        let (url, worker) = mock_requests(vec![(200, payload)], |_, _| {});
        let (service, path) = service(&url);
        expire_discovery(&service);
        let result = service.providers().unwrap();
        assert_eq!(result["discovery"]["state"], "degraded");
        assert!(service.state.lock().unwrap().providers_expires.is_none());
        assert!(service.state.lock().unwrap().providers_retry.is_some());
        worker.join().unwrap();
        std::fs::remove_dir_all(path).unwrap();
    }
    let (service, path) = service("http://127.0.0.1:1");
    expire_discovery(&service);
    assert_eq!(
        service.providers().unwrap()["discovery"]["state"],
        "degraded"
    );
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn discovery_honors_bounded_retry_after_and_rejects_malformed_json() {
    use std::io::{BufRead, BufReader, Write};
    for (status, body, retry) in [
        (429, "{}", "120".to_owned()),
        (
            429,
            "{}",
            httpdate::fmt_http_date(SystemTime::now() + Duration::from_secs(125)),
        ),
        (200, "{", String::new()),
    ] {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(&mut stream);
            loop {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
            }
            write!(stream,"HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nRetry-After: {retry}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        });
        let (service, path) = service(&url);
        expire_discovery(&service);
        let result = service.providers().unwrap();
        assert_eq!(result["discovery"]["state"], "degraded");
        let delay = result["discovery"]["retryAfterMs"].as_u64().unwrap();
        assert!(delay <= 3_600_000);
        if status == 429 {
            assert!(delay > 115_000);
        } else {
            assert!(delay <= 30_000);
        }
        worker.join().unwrap();
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[test]
fn library_and_vpc_use_the_same_explicit_proxy_without_direct_fallback() {
    let (proxy, worker) = mock_requests(
        vec![
            (200, json!({"requestStatus":{"serverId":"proxy-vpc"}})),
            (
                200,
                json!({"data":{"apps":{"items":[],"pageInfo":{"hasNextPage":false}}}}),
            ),
        ],
        |index, request| {
            if index == 0 {
                assert!(request.starts_with("GET http://metadata.fixture.invalid/v2/serverInfo "));
            } else {
                assert!(request.starts_with("POST http://catalog.fixture.invalid/graphql "));
                assert!(request.contains("proxy-vpc"));
            }
            assert!(request.contains("GFNJWT test-access"));
        },
    );
    let (mut service, path) = service("http://127.0.0.1:1");
    service.endpoints.server_info = Some("http://metadata.fixture.invalid/v2/serverInfo".into());
    service.endpoints.graphql = "http://catalog.fixture.invalid/graphql".into();
    let result = service
        .library_catalog(
            &json!({}),
            &json!({"sessionProxyEnabled":true,"sessionProxyUrl":proxy}),
        )
        .unwrap();
    assert_eq!(result["source"], "account-library");
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn advertised_default_provider_does_not_replace_an_explicit_selection() {
    let mut payload = directory("alliance", "alliance.nvidiagrid.net");
    payload["gfnServiceInfo"]["defaultProvider"] = json!("ALLIANCE");
    let (url, worker) = mock_requests(vec![(200, payload)], |_, _| {});
    let (service, path) = service(&url);
    expire_discovery(&service);
    let result = service.providers().unwrap();
    assert_eq!(result["defaultProviderIdpId"], "alliance");
    assert_eq!(
        service
            .start_device_login(&json!({"providerIdpId":"deleted"}))
            .unwrap_err()
            .code,
        "provider_unavailable"
    );
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn failed_discovery_is_degraded_and_retries_without_remapping_explicit_identity() {
    let (url, worker) = mock_requests(
        vec![
            (503, json!({})),
            (200, directory("alliance", "alliance.nvidiagrid.net")),
        ],
        |_, request| {
            assert!(request.starts_with("GET /providers "));
            assert!(!request.to_ascii_lowercase().contains("authorization:"));
        },
    );
    let (service, path) = service(&url);
    expire_discovery(&service);
    service.state.lock().unwrap().providers.clear();
    let first = service.providers().unwrap();
    assert_eq!(first["discovery"]["state"], "degraded");
    assert!(service.state.lock().unwrap().providers.is_empty());
    assert!(first["discovery"]["retryAfterMs"].as_u64().unwrap() > 0);
    assert_eq!(
        service.providers().unwrap()["discovery"]["state"],
        "degraded"
    );
    assert_eq!(
        service
            .start_device_login(&json!({"providerIdpId":"missing-alliance"}))
            .unwrap_err()
            .code,
        "provider_unavailable"
    );
    expire_discovery(&service);
    let recovered = service.providers().unwrap();
    assert_eq!(recovered["discovery"]["state"], "ready");
    assert_eq!(recovered["providers"][0]["idpId"], "alliance");
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn discovery_reconciles_only_matching_provider_and_fences_changed_or_removed_routes() {
    let (url, worker) = mock_requests(
        vec![
            (503, json!({})),
            (200, directory("alliance", "new.nvidiagrid.net")),
            (200, directory("another", "other.nvidiagrid.net")),
        ],
        |_, _| {},
    );
    let (service, path) = service(&url);
    {
        let mut state = service.state.lock().unwrap();
        let session = state.session.as_mut().unwrap();
        session.provider.idp_id = "alliance".into();
        session.provider.streaming_service_url = "https://old.nvidiagrid.net/".into();
        state.providers.clear();
    }
    expire_discovery(&service);
    assert_eq!(
        service.providers().unwrap()["providers"][0]["idpId"],
        "alliance"
    );
    let (old, generation) = service
        .authenticated_snapshot(TokenPurpose::ServiceId, false)
        .unwrap();
    assert_eq!(
        old.provider.streaming_service_url,
        "https://old.nvidiagrid.net/"
    );
    expire_discovery(&service);
    service.providers().unwrap();
    assert_eq!(
        service.check_scope(&old, generation).unwrap_err().code,
        "stale_account"
    );
    let (new, generation) = service
        .authenticated_snapshot(TokenPurpose::ServiceId, false)
        .unwrap();
    assert_eq!(
        new.provider.streaming_service_url,
        "https://new.nvidiagrid.net/"
    );
    expire_discovery(&service);
    service.providers().unwrap();
    assert_eq!(
        service.check_scope(&new, generation).unwrap_err().code,
        "stale_account"
    );
    assert_eq!(
        service
            .authenticated_snapshot(TokenPurpose::ServiceId, false)
            .unwrap_err()
            .code,
        "provider_unavailable"
    );
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn server_info_failures_never_dispatch_a_generic_library_query() {
    for (status, body, expected) in [
        (403, json!({}), "upstream_error"),
        (429, json!({}), "upstream_error"),
        (200, json!({}), "invalid_upstream_response"),
        (503, json!({}), "upstream_error"),
    ] {
        let (url, worker) = mock_requests(vec![(status, body)], |_, request| {
            assert!(request.starts_with("GET /v2/serverInfo "));
            assert!(request.contains("GFNJWT test-access"));
            assert!(!request.contains("GFNPartnerJWT"));
        });
        let (service, path) = service(&url);
        assert_eq!(
            service
                .library_catalog(&json!({}), &json!({}))
                .unwrap_err()
                .code,
            expected
        );
        worker.join().unwrap();
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[test]
fn library_401_renews_once_and_replays_with_the_same_owner_and_new_vpc() {
    let (url, worker) = mock_requests(
        vec![
            (200, json!({"requestStatus":{"serverId":"vpc-old"}})),
            (401, json!({})),
            (
                200,
                json!({"access_token":"renewed-access","expires_in":3600}),
            ),
            (
                200,
                json!({"sub":"account-a","email":"fixture@example.invalid"}),
            ),
            (200, json!({"requestStatus":{"serverId":"vpc-renewed"}})),
            (
                200,
                json!({"data":{"apps":{"items":[],"pageInfo":{"totalCount":0,"hasNextPage":false}}}}),
            ),
        ],
        |index, request| {
            if matches!(index, 0 | 1 | 4 | 5) {
                let expected = if index < 2 {
                    "test-access"
                } else {
                    "renewed-access"
                };
                assert!(request.contains(&format!("GFNJWT {expected}")));
            }
            if index == 1 {
                assert!(request.starts_with("POST /graphql "));
                assert!(request.contains("vpc-old"));
            }
            if index == 5 {
                assert!(request.starts_with("POST /graphql "));
                assert!(request.contains("vpc-renewed"));
            }
            if index == 2 {
                assert!(request.contains("client_id=test-client-id"));
            }
        },
    );
    let (service, path) = service(&url);
    let result = service.library_catalog(&json!({}), &json!({})).unwrap();
    assert_eq!(result["scope"]["generation"], 7);
    assert_eq!(result["scope"]["userId"], "account-a");
    assert!(!result.to_string().contains("renewed-access"));
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn second_401_is_terminal_and_never_loops_renewal() {
    let (url, worker) = mock_requests(
        vec![
            (
                200,
                json!({"access_token":"renewed-access","expires_in":3600}),
            ),
            (
                200,
                json!({"sub":"account-a","email":"fixture@example.invalid"}),
            ),
        ],
        |_, _| {},
    );
    let (service, path) = service(&url);
    let calls = AtomicUsize::new(0);
    let error = service
        .authenticated_read(|_, _| {
            calls.fetch_add(1, Ordering::SeqCst);
            Err(ServiceError {
                code: "http_unauthorized",
                message: "Unauthorized fixture".into(),
            })
        })
        .unwrap_err();
    assert_eq!(error.code, "http_unauthorized");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn delayed_library_result_cannot_publish_after_account_replacement() {
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let (url, worker) = mock_requests(
        vec![
            (200, json!({"requestStatus":{"serverId":"vpc-a"}})),
            (
                200,
                json!({"data":{"apps":{"items":[],"pageInfo":{"hasNextPage":false}}}}),
            ),
        ],
        move |index, _| {
            if index == 1 {
                entered_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            }
        },
    );
    let (service, path) = service(&url);
    std::thread::scope(|threads| {
        let read = threads.spawn(|| service.library_catalog(&json!({}), &json!({})));
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        {
            let mut state = service.state.lock().unwrap();
            state.session = Some(auth_fixture("account-b"));
            state.generation += 1;
        }
        release_tx.send(()).unwrap();
        assert_eq!(read.join().unwrap().unwrap_err().code, "stale_account");
    });
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn dynamic_nvidia_queue_route_does_not_require_discovery() {
    let (service, path) = service("http://127.0.0.1:1");
    let session = auth_fixture("account-a");
    let params = json!({"zone":"NP-NEW9-01","streamingBaseUrl":"https://np-new9-01.cloudmatchbeta.nvidiagrid.net/"});
    let settings = json!({"region":"https://saved.nvidiagrid.net/"});
    let (routed, effective) = service
        .scoped_session_route(&params, &settings, &session)
        .unwrap();
    assert_eq!(routed, params);
    assert_eq!(effective["region"], "");
    assert_eq!(settings["region"], "https://saved.nvidiagrid.net/");
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn dynamic_queue_route_rejects_mismatched_urls_malformed_ids_and_alliance() {
    let (url, worker) = mock_requests(
        vec![(
            200,
            json!({"requestStatus":{"serverId":"fixture"},"metaData":[]}),
        )],
        |_, request| {
            assert!(request.starts_with("GET /v2/serverInfo "));
        },
    );
    let (service, path) = service(&url);
    let mut session = auth_fixture("account-a");
    for (zone, base) in [
        (
            "NP-NEW9-01",
            "https://np-new9-02.cloudmatchbeta.nvidiagrid.net/",
        ),
        (
            "NP-NEW9-01",
            "https://np-new9-01.cloudmatchbeta.nvidiagrid.net.evil/",
        ),
        (
            "NP-NEW9-01",
            "https://user@np-new9-01.cloudmatchbeta.nvidiagrid.net/",
        ),
        (
            "NP-NEW9-01",
            "https://np-new9-01.cloudmatchbeta.nvidiagrid.net/path",
        ),
        (
            "NP-NEW9-01",
            "http://np-new9-01.cloudmatchbeta.nvidiagrid.net/",
        ),
        (
            "NP-NEW9-01.evil",
            "https://np-new9-01.evil.cloudmatchbeta.nvidiagrid.net/",
        ),
        (
            "NPA-NEW9-01",
            "https://npa-new9-01.cloudmatchbeta.nvidiagrid.net/",
        ),
    ] {
        let (routed, _) = service
            .scoped_session_route(
                &json!({"zone":zone,"streamingBaseUrl":base}),
                &json!({}),
                &session,
            )
            .unwrap();
        assert!(routed["streamingBaseUrl"].is_null(), "{base}");
    }
    session.provider.idp_id = "alliance".into();
    session.provider.code = "ALLIANCE".into();
    service.state.lock().unwrap().providers = vec![session.provider.clone()];
    let (routed, _) = service.scoped_session_route(&json!({"zone":"NP-NEW9-01","streamingBaseUrl":"https://np-new9-01.cloudmatchbeta.nvidiagrid.net/"}), &json!({}), &session).unwrap();
    assert!(routed["streamingBaseUrl"].is_null());
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn region_overrides_require_current_provider_membership_and_preserve_saved_preferences() {
    let (url, worker) = mock_requests(
        vec![(
            200,
            json!({"requestStatus":{"serverId":"alliance-vpc"},"metaData":[
                {"key":"Alliance region","value":"https://alliance-region.nvidiagrid.net/"},
                {"key":"Untrusted","value":"https://outside.invalid/"}
            ]}),
        )],
        |_, request| assert!(request.starts_with("GET /v2/serverInfo ")),
    );
    let (service, path) = service(&url);
    let mut session = auth_fixture("account-a");
    session.provider.idp_id = "alliance".into();
    session.provider.streaming_service_url = "https://alliance.nvidiagrid.net/".into();
    service.state.lock().unwrap().providers = vec![session.provider.clone()];
    let settings = json!({"region":"https://nvidia-region.nvidiagrid.net/","regionProviderIdpId":DEFAULT_IDP_ID});
    let (params, effective) = service
        .scoped_session_route(&json!({}), &settings, &session)
        .unwrap();
    assert!(params["streamingBaseUrl"].is_null());
    assert_eq!(effective["region"], "");
    let (params, _) = service
        .scoped_session_route(
            &json!({"streamingBaseUrl":"https://nvidia-region.nvidiagrid.net/"}),
            &settings,
            &session,
        )
        .unwrap();
    assert!(params["streamingBaseUrl"].is_null());
    assert_eq!(settings["region"], "https://nvidia-region.nvidiagrid.net/");
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn endpoint_validation_is_shared_and_rejects_credential_exfiltration_shapes() {
    for url in [
        "https://prod.cloudmatchbeta.nvidiagrid.net/",
        "https://provider.nvidiagrid.net/",
        "https://region.geforcenow.nvidiagrid.net/",
    ] {
        assert!(trusted_streaming_base(url).is_ok());
    }
    for url in [
        "http://provider.nvidiagrid.net/",
        "https://provider.nvidiagrid.net.evil.test/",
        "https://user@provider.nvidiagrid.net/",
        "https://provider.nvidiagrid.net:8443/",
        "https://127.0.0.1/",
        "https://partner.invalid/",
    ] {
        assert!(trusted_streaming_base(url).is_err(), "{url}");
    }
}

#[test]
fn preparation_uses_owned_context_and_rejects_old_generations_or_other_seats() {
    let (service, path) = service("http://127.0.0.1:1");
    let owner = auth_fixture("account-a");
    service.cloudmatch.seed_owned_session(json!({"sessionId":"seat-a","subSessionId":"sub-a","status":3,"rtspsEndpoints":["rtsps://owned.nvidiagrid.net:443"],"connectionInfo":[{"protocol":"RTSPS","host":"owned.nvidiagrid.net","port":443}]}));
    service.session_routing.lock().unwrap().active_owner = Some(
        ActiveSeatOwner::capture(owner.clone(), 7, &json!({"sessionId":"seat-a"}), None).unwrap(),
    );
    let params = json!({"session":{"sessionId":"seat-a","status":3,"connectionInfo":[{"host":"evil.invalid"}]}});
    let calls = AtomicUsize::new(0);
    let result = service
        .prepare_owned_stream(&params, |params| {
            calls.fetch_add(1, Ordering::SeqCst);
            assert_eq!(
                params["session"]["connectionInfo"][0]["host"],
                "owned.nvidiagrid.net"
            );
            Ok(params.clone())
        })
        .unwrap();
    assert_eq!(result["session"]["sessionId"], "seat-a");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        service
            .prepare_owned_stream(&json!({"session":{"sessionId":"other"}}), |_| panic!(
                "foreign seat prepared"
            ))
            .unwrap_err()
            .code,
        "session_owner_mismatch"
    );
    service.state.lock().unwrap().generation += 1;
    let regained = service
        .prepare_owned_stream(&params, |params| Ok(params.clone()))
        .unwrap();
    assert_eq!(regained["scope"]["generation"], 8);
    assert_eq!(
        regained["session"]["connectionInfo"][0]["host"],
        "owned.nvidiagrid.net"
    );
    assert_eq!(
        service.check_scope(&owner, 7).unwrap_err().code,
        "stale_account"
    );
    service.state.lock().unwrap().session = Some(auth_fixture("account-b"));
    assert_eq!(
        service
            .prepare_owned_stream(&params, |_| panic!("foreign owner prepared"))
            .unwrap_err()
            .code,
        "session_owner_mismatch"
    );
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn original_identity_regains_each_exact_seat_operation_after_generation_changes() {
    for transition in ["switch-back", "clear-cache", "relogin"] {
        for operation in ["get", "poll", "prepare", "claim", "ad", "stop"] {
            let payload = json!({"requestStatus":{"statusCode":1},"session":{
                "sessionId":"seat-a","subSessionId":"sub-a","status":2,
                "connectionInfo":[{"usage":16,"ip":"owned.nvidiagrid.net","port":443}]
            }});
            let responses = match operation {
                "poll" | "ad" => vec![(200, payload.clone())],
                "claim" => vec![(200, payload.clone()), (200, payload)],
                "stop" => vec![(204, json!({}))],
                _ => vec![],
            };
            let (url, worker) = mock_requests(responses, |_, request| {
                assert!(request.contains(" /v2/session/seat-a"));
                assert!(request.contains("GFNJWT test-access"));
                assert!(!request.contains("foreign-access"));
                assert!(!request.contains("forged.nvidiagrid.net"));
            });
            let (mut service, path) = service(&url);
            let owner = auth_fixture("account-a");
            service.vault.save(&owner).unwrap();
            let mut foreign = auth_fixture("account-b");
            foreign.tokens.access_token = "foreign-access".into();
            service.vault.save(&foreign).unwrap();
            service
                .cloudmatch
                .set_test_control_base(url::Url::parse(&url).unwrap());
            let seat = json!({"sessionId":"seat-a","subSessionId":"sub-a","status":3,
                "streamingBaseUrl":"https://owned.nvidiagrid.net/","zone":"owned.nvidiagrid.net",
                "rtspsEndpoints":["rtsps://owned.nvidiagrid.net:443"],
                "connectionInfo":[{"protocol":"RTSPS","host":"owned.nvidiagrid.net","port":443}]
            });
            service.cloudmatch.seed_owned_session(seat.clone());
            service.session_routing.lock().unwrap().active_owner =
                Some(ActiveSeatOwner::capture(owner.clone(), 7, &seat, None).unwrap());
            match transition {
                "switch-back" => {
                    service
                        .switch_account(&json!({"userId":"account-b"}))
                        .unwrap();
                    assert!(service.active_session().unwrap()["session"].is_null());
                    let params = json!({"sessionId":"seat-a"});
                    assert_eq!(
                        service.claim_session(&params, &json!({})).unwrap_err().code,
                        "session_owner_mismatch"
                    );
                    assert_eq!(
                        service.report_session_ad(&params).unwrap_err().code,
                        "session_owner_mismatch"
                    );
                    assert_eq!(
                        service
                            .prepare_owned_stream(&json!({"session":params}), |_| panic!(
                                "foreign owner prepared"
                            ))
                            .unwrap_err()
                            .code,
                        "session_owner_mismatch"
                    );
                    service
                        .switch_account(&json!({"userId":"account-a"}))
                        .unwrap();
                }
                "clear-cache" => {
                    service.clear_cache();
                }
                "relogin" => {
                    service
                        .state
                        .lock()
                        .unwrap()
                        .attempts
                        .insert("relogin".into(), pending_attempt(Some(owner.clone())));
                    service
                        .complete_device_login(&json!({"attemptId":"relogin"}))
                        .unwrap();
                }
                _ => unreachable!(),
            }
            let generation = service.state.lock().unwrap().generation;
            assert!(generation > 7);
            assert_eq!(
                service.check_scope(&owner, 7).unwrap_err().code,
                "stale_account"
            );
            let params = json!({"sessionId":"seat-a","streamingBaseUrl":"https://forged.nvidiagrid.net/","action":"start","adId":"fixture"});
            let result = match operation {
                "get" => service.active_session(),
                "poll" => service.poll_session(&params),
                "prepare" => service.prepare_owned_stream(&json!({"session":params}), |params| {
                    assert_eq!(
                        params["session"]["connectionInfo"][0]["host"],
                        "owned.nvidiagrid.net"
                    );
                    assert_eq!(params["session"]["ownerScope"]["generation"], generation);
                    Ok(json!({"context":{}}))
                }),
                "claim" => service.claim_session(&params, &json!({})),
                "ad" => service.report_session_ad(&params),
                "stop" => service.stop_session(&params, &json!({})),
                _ => unreachable!(),
            }
            .unwrap_or_else(|error| panic!("{transition}/{operation}: {error:?}"));
            assert_eq!(
                result["scope"]["generation"], generation,
                "{transition}/{operation}"
            );
            assert_eq!(result["scope"]["userId"], "account-a");
            if operation != "stop" {
                assert_eq!(
                    service
                        .session_routing
                        .lock()
                        .unwrap()
                        .active_owner
                        .as_ref()
                        .unwrap()
                        .last_published_generation,
                    generation
                );
            }
            worker.join().unwrap();
            std::fs::remove_dir_all(path).unwrap();
        }
    }
}

#[test]
fn durable_seat_republication_does_not_renew_allocation_receipt_authority() {
    let mut responses = launch_metadata();
    responses.extend([
        (200, json!({"requestStatus":{"statusCode":1},"session":{"sessionId":"fresh-seat","status":1}})),
        (200, json!({})),
        (204, json!({})),
    ]);
    let (url, worker) = mock_requests(responses, |index, request| {
        if index == 8 {
            assert!(request.starts_with("DELETE /v2/session/fresh-seat "));
        }
    });
    let (mut service, path) = service(&url);
    {
        let mut state = service.state.lock().unwrap();
        state.providers = vec![auth_fixture("account-a").provider];
        state.providers_expires = Some(Instant::now() + Duration::from_secs(60));
    }
    service
        .cloudmatch
        .set_test_control_base(url::Url::parse(&url).unwrap());
    service
        .create_session(&launch_params("123"), &json!({}))
        .unwrap();
    service.clear_cache();
    assert_eq!(service.active_session().unwrap()["scope"]["generation"], 8);
    assert_eq!(
        service
            .session_routing
            .lock()
            .unwrap()
            .active_owner
            .as_ref()
            .unwrap()
            .allocation_generation,
        Some(7)
    );
    service.finish_session_create("fresh-seat", true).unwrap();
    assert!(service.cloudmatch.active()["session"].is_null());
    assert!(
        service
            .session_routing
            .lock()
            .unwrap()
            .active_owner
            .is_none()
    );
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn rediscovery_stays_generation_fenced_and_same_user_other_provider_cannot_manage_seat() {
    let (service, path) = service("http://127.0.0.1:1");
    let owner = auth_fixture("account-a");
    let seat = json!({"sessionId":"seat-a","status":3});
    service
        .cloudmatch
        .seed_discovered_sessions(std::slice::from_ref(&seat));
    service.session_routing.lock().unwrap().discovery_owner =
        Some((owner.provider.idp_id.clone(), owner.user.user_id.clone(), 7));
    service.clear_cache();
    assert_eq!(
        service.claim_session(&seat, &json!({})).unwrap_err().code,
        "session_owner_mismatch"
    );
    service.cloudmatch.seed_owned_session(seat.clone());
    service.session_routing.lock().unwrap().active_owner =
        Some(ActiveSeatOwner::capture(owner.clone(), 7, &seat, None).unwrap());
    service
        .state
        .lock()
        .unwrap()
        .session
        .as_mut()
        .unwrap()
        .provider
        .idp_id = "other-provider".into();
    assert!(service.active_session().unwrap()["session"].is_null());
    assert_eq!(
        service.claim_session(&seat, &json!({})).unwrap_err().code,
        "session_owner_mismatch"
    );
    assert_eq!(
        service.report_session_ad(&seat).unwrap_err().code,
        "session_owner_mismatch"
    );
    assert_eq!(
        service
            .prepare_owned_stream(&json!({"session":seat}), |_| panic!(
                "foreign provider prepared"
            ))
            .unwrap_err()
            .code,
        "session_owner_mismatch"
    );
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn delayed_poll_fences_ordinary_results_but_preserves_exact_seat_termination() {
    for foreign_selected in [false, true] {
        for status in [2, 7, 404] {
            let (entered_tx, entered_rx) = std::sync::mpsc::channel();
            let (release_tx, release_rx) = std::sync::mpsc::channel();
            let payload = json!({"requestStatus":{"statusCode":1},"session":{"sessionId":"seat-a","status":status}});
            let (url, worker) = mock_requests(
                vec![(if status == 404 { 404 } else { 200 }, payload)],
                move |_, request| {
                    assert!(request.contains(" /v2/session/seat-a "));
                    assert!(request.contains("GFNJWT test-access"));
                    assert!(!request.contains("foreign-access"));
                    entered_tx.send(()).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                },
            );
            let (mut service, path) = service(&url);
            let owner = auth_fixture("account-a");
            let seat = json!({"sessionId":"seat-a","status":3});
            service
                .cloudmatch
                .set_test_control_base(url::Url::parse(&url).unwrap());
            service.cloudmatch.seed_owned_session(seat.clone());
            service.session_routing.lock().unwrap().active_owner =
                Some(ActiveSeatOwner::capture(owner, 7, &seat, None).unwrap());
            if foreign_selected {
                let mut foreign = auth_fixture("account-b");
                foreign.tokens.access_token = "foreign-access".into();
                service.vault.save(&foreign).unwrap();
                service
                    .switch_account(&json!({"userId":"account-b"}))
                    .unwrap();
            }
            std::thread::scope(|threads| {
                let poll = threads.spawn(|| service.poll_session(&json!({"sessionId":"seat-a"})));
                entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                service.clear_cache();
                release_tx.send(()).unwrap();
                let result = poll.join().unwrap();
                if status == 2 && !foreign_selected {
                    assert_eq!(result.unwrap_err().code, "stale_account");
                } else {
                    let result = result.unwrap();
                    assert_eq!(result["scope"]["generation"], 7);
                    assert_eq!(result["scope"]["userId"], "account-a");
                    if status != 2 {
                        let termination = if status == 7 {
                            &result["session"]["termination"]
                        } else {
                            &result["termination"]
                        };
                        assert_eq!(termination["sessionId"], "seat-a");
                        assert_eq!(termination["resumable"], false);
                        assert!(service.cloudmatch.active()["session"].is_null());
                        assert!(
                            service
                                .session_routing
                                .lock()
                                .unwrap()
                                .active_owner
                                .is_none()
                        );
                    }
                }
            });
            worker.join().unwrap();
            std::fs::remove_dir_all(path).unwrap();
        }
    }
}

#[test]
fn digevo_unreachable_discovery_endpoint_falls_back_to_latam_west() {
    let stale = LoginProvider {
        idp_id: PROVIDER_FALLBACKS[0].idp_id.to_owned(),
        code: "DIG".to_owned(),
        display_name: "Digevo".to_owned(),
        streaming_service_url: "https://prod.DIG.geforcenow.nvidiagrid.net/".to_owned(),
        priority: 10,
    };
    // NXDOMAIN (out of footprint, no VPN): use the verified regional fallback.
    assert_eq!(
        effective_provider_url_with(&stale, |_| false),
        "https://latam-west.dig.geforcenow.nvidiagrid.net/"
    );
    // Reachable (VPN / in footprint): keep the geo-steered discovery endpoint.
    assert_eq!(
        effective_provider_url_with(&stale, |_| true),
        "https://prod.DIG.geforcenow.nvidiagrid.net/"
    );
    // Non-Digevo providers and already-regional Digevo URLs are untouched.
    let nvidia = LoginProvider::default_nvidia();
    assert_eq!(
        effective_provider_url_with(&nvidia, |_| false),
        "https://prod.cloudmatchbeta.nvidiagrid.net/"
    );
    let current = LoginProvider {
        streaming_service_url: "https://latam-west.dig.geforcenow.nvidiagrid.net/".to_owned(),
        ..stale
    };
    assert_eq!(
        effective_provider_url_with(&current, |_| false),
        "https://latam-west.dig.geforcenow.nvidiagrid.net/"
    );
}
