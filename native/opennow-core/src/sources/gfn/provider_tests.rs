use super::*;
use std::sync::{Arc, Mutex};

fn scope() -> api::AccountScope {
    api::AccountScope {
        account: account_key("nvidia", "player").unwrap(),
        revision: 7,
    }
}

fn target() -> api::LaunchTarget {
    api::LaunchTarget {
        game: api::GameId::new("catalog-app").unwrap(),
        variant: api::VariantId::new("123").unwrap(),
    }
}

fn fixture(path: &std::path::Path, public_catalog: &str) -> GfnModule {
    let mut endpoints = service::Endpoints::default();
    endpoints.public_catalog = public_catalog.into();
    let service = Arc::new(service::GfnService::with_client(
        reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap(),
        endpoints,
        path.into(),
    ));
    let (output, _) = std::sync::mpsc::channel();
    let push =
        super::super::push_registry::PushRegistry::new(Arc::clone(&service), output, path.into());
    let closing = push.core_exit_signal();
    GfnModule {
        service,
        settings: Arc::new(Mutex::new(
            crate::settings::SettingsStore::load(Some(path.into())).unwrap(),
        )),
        streamer: Arc::new(StreamerService::new()),
        diagnostics: Arc::new(crate::diagnostics::DiagnosticsService::new(path).unwrap()),
        push: Mutex::new(push),
        community: super::super::community::CommunityService::new().unwrap(),
        closing,
        provider: Mutex::new(ProviderState::default()),
    }
}

#[test]
fn auth_projection_preserves_authority_generation_and_persistence_without_tokens() {
    let value = json!({
        "generation":7,"persistence":"secure-store",
        "session":{"provider":{"idpId":"nvidia","displayName":"NVIDIA","streamingServiceUrl":"https://private.example"},
            "user":{"userId":"player","displayName":"Player","email":"private@example.test"},
            "tokens":{"access_token":"must-not-escape"}}
    });
    let result = auth_state(&value).unwrap();
    let api::AuthState::SignedIn { account, revision } = &result else {
        panic!("expected account");
    };
    assert_eq!(account.key, scope().account);
    assert_eq!(*revision, 7);
    assert_eq!(account.persistence, api::Persistence::Durable);
    let encoded = serde_json::to_string(&result).unwrap();
    assert!(!encoded.contains("must-not-escape"));
    assert!(!encoded.contains("private.example"));
    assert!(!encoded.contains("private@example.test"));
    let mut temporary = value;
    temporary["persistence"] = json!("memory-only");
    let api::AuthState::SignedIn { account, .. } = auth_state(&temporary).unwrap() else {
        panic!("expected account");
    };
    assert_eq!(account.persistence, api::Persistence::Temporary);
}

#[test]
fn device_challenge_does_not_publish_device_code_or_complete_uri() {
    let challenge = device_challenge(&json!({
        "attemptId":"attempt-one","userCode":"ABCD-EFGH","device_code":"secret-device-code",
        "verificationUri":"https://login.example/device",
        "verificationUriComplete":"https://login.example/device?secret=private",
        "expiresAt":123_000,"intervalSeconds":5
    }))
    .unwrap();
    challenge.validate().unwrap();
    let json = serde_json::to_string(&challenge).unwrap();
    assert!(json.contains("ABCD-EFGH"));
    assert!(!json.contains("secret-device-code"));
    assert!(!json.contains("secret=private"));
    assert!(!format!("{challenge:?}").contains("ABCD-EFGH"));
    assert_eq!(
        poll_delay(&json!({"intervalSeconds":3600})).unwrap(),
        3_600_000
    );
}

#[test]
fn authorized_poll_does_not_commit_account_until_explicit_completion() {
    let directory = tempfile::tempdir().unwrap();
    let module = fixture(directory.path(), "http://127.0.0.1:1");
    module
        .closing
        .store(true, std::sync::atomic::Ordering::Release);
    module.service.seed_provider_authorization("approved-login");
    let attempt = api::AttemptId::new("approved-login").unwrap();
    let challenge = device_challenge(&json!({"attemptId":"approved-login","userCode":"ABCD","verificationUri":"https://login.example/device","expiresAt":123_000,"intervalSeconds":5})).unwrap();
    module.provider.lock().unwrap().attempt = Some(DeviceAttempt {
        challenge,
        remember: false,
        completed: None,
    });
    let poll = module
        .execute_provider(
            &Request::AuthPoll(api::AuthAttempt {
                attempt: attempt.clone(),
            }),
            &Cancellation::default(),
            None,
        )
        .unwrap();
    assert_eq!(
        poll,
        Reply::AuthPoll(api::AuthState::Authorized {
            attempt: attempt.clone()
        })
    );
    assert_eq!(module.service.auth_generation(), 0);
    assert!(module.service.session().unwrap()["session"].is_null());
    let complete = module
        .execute_provider(
            &Request::AuthComplete(api::CompleteAuth {
                attempt: attempt.clone(),
                proof: None,
            }),
            &Cancellation::default(),
            None,
        )
        .unwrap();
    let Reply::AuthComplete(api::AuthState::SignedIn { account, revision }) = complete else {
        panic!("expected committed account");
    };
    assert_eq!(account.key.account.as_str(), "typed-player");
    assert_eq!(account.persistence, api::Persistence::Temporary);
    assert_eq!(revision, 1);
    let repeat = module
        .execute_provider(
            &Request::AuthComplete(api::CompleteAuth {
                attempt,
                proof: None,
            }),
            &Cancellation::default(),
            None,
        )
        .unwrap();
    assert!(matches!(
        repeat,
        Reply::AuthComplete(api::AuthState::SignedIn { revision: 1, .. })
    ));
}

#[test]
fn missing_description_is_absent_and_multiline_prose_is_normalized() {
    let mut value = json!({"game":{"id":"game","title":"Game","variants":[]},"catalogRevision":4});
    let catalog_scope = api::CatalogScope::Account { scope: scope() };
    assert!(
        game_details(&value, catalog_scope.clone())
            .unwrap()
            .description
            .is_none()
    );
    value["game"]["shortDescription"] = json!("First line.\n\nSecond line.");
    assert_eq!(
        game_details(&value, catalog_scope)
            .unwrap()
            .description
            .unwrap()
            .as_str(),
        "First line. Second line."
    );
}

#[test]
fn missing_remote_session_is_terminal_only_with_matching_upstream_evidence() {
    let view = session_view(&json!({"sessionId":"seat","status":2}), &scope(), target()).unwrap();
    let owned = OwnedSession {
        view,
        scope: scope(),
    };
    let result = json!({"session":null,"termination":{"httpStatus":404,"sessionId":"seat","resumable":false}});
    assert!(matches!(
        polled_session(&result, &owned).unwrap().state,
        api::RemoteSessionState::Finished { .. }
    ));
    assert!(polled_session(&json!({"session":null}), &owned).is_err());
    let mut wrong = result;
    wrong["termination"]["sessionId"] = json!("other-seat");
    assert!(polled_session(&wrong, &owned).is_err());
}

#[test]
fn public_catalog_runs_existing_algorithm_with_non_launchable_public_ids() {
    let (url, worker) = service::tests::mock_requests(
        vec![(
            200,
            json!([
                {"id":"public-game","title":"Alpha","status":"AVAILABLE","steamUrl":"https://store.steampowered.com/app/730/game"},
                {"id":"not-available","title":"Beta","status":"UNAVAILABLE"}
            ]),
        )],
        |_, request| assert!(request.starts_with("GET / ")),
    );
    let directory = tempfile::tempdir().unwrap();
    let module = fixture(directory.path(), &url);
    let request = Request::CatalogPublic(api::CatalogRequest {
        scope: api::CatalogScope::Public,
        query: opennow_plugin_api::CatalogQuery {
            query: "Alpha".into(),
            cursor: None,
            limit: 10,
        },
    });
    let reply = module
        .execute_provider(&request, &Cancellation::default(), None)
        .unwrap();
    let Reply::CatalogPublic(page) = reply else {
        panic!("expected public catalog");
    };
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].id.as_str(), "public:public-game");
    assert_eq!(page.items[0].availability, api::Availability::Unknown);
    assert_eq!(page.coverage, opennow_plugin_api::Coverage::Unknown);
    assert!(canonical_game(&page.items[0].id).is_err());
    assert!(page.next_cursor.is_none());
    worker.join().unwrap();
}

#[test]
fn public_catalog_does_not_wait_for_session_correlation_lock() {
    let (url, worker) = service::tests::mock_requests(
        vec![(
            200,
            json!([{"id":"public-game","title":"Alpha","status":"AVAILABLE"}]),
        )],
        |_, _| {},
    );
    let directory = tempfile::tempdir().unwrap();
    let module = Arc::new(fixture(directory.path(), &url));
    let held = module.provider.lock().unwrap();
    let (sent, received) = std::sync::mpsc::channel();
    let worker_module = Arc::clone(&module);
    let catalog = std::thread::spawn(move || {
        let request = Request::CatalogPublic(api::CatalogRequest {
            scope: api::CatalogScope::Public,
            query: opennow_plugin_api::CatalogQuery {
                query: String::new(),
                cursor: None,
                limit: 10,
            },
        });
        sent.send(worker_module.execute_provider(&request, &Cancellation::default(), None))
            .unwrap();
    });
    let result = received.recv_timeout(std::time::Duration::from_secs(3));
    drop(held);
    catalog.join().unwrap();
    assert!(
        result
            .expect("catalog waited for unrelated session correlation")
            .is_ok()
    );
    worker.join().unwrap();
}

#[test]
fn launch_projection_preserves_strict_variant_membership_and_blocking_reason() {
    let value = json!({"game":{"id":"catalog-app","variants":[{"id":"123"}]},"decision":{"status":"ready","message":"Ready"}});
    assert!(matches!(
        launch_decision(&value, target(), text("5").unwrap()).unwrap(),
        api::LaunchDecision::Ready { .. }
    ));
    let mut foreign = target();
    foreign.variant = api::VariantId::new("456").unwrap();
    assert!(launch_decision(&value, foreign, text("5").unwrap()).is_err());
    let blocked = json!({"decision":{"status":"ownership_required","message":"An existing game license is required"}});
    assert!(matches!(
        launch_decision(&blocked, target(), text("5").unwrap()).unwrap(),
        api::LaunchDecision::Blocked {
            reason: api::Availability::OwnershipRequired,
            ..
        }
    ));
}

#[test]
fn session_projection_keeps_remote_id_and_original_owner_without_media_secrets() {
    let value = json!({"sessionId":"remote-seat","status":3,"appId":123,"ownerScope":legacy_scope(&scope()),"signalingUrl":"https://secret.example","token":"private-token"});
    let view = session_view(&value, &scope(), target()).unwrap();
    assert_eq!(view.key.remote_id.as_str(), "remote-seat");
    assert_eq!(view.key.account, Some(scope().account));
    assert_eq!(view.state, api::RemoteSessionState::Ready);
    let encoded = serde_json::to_string(&view).unwrap();
    assert!(!encoded.contains("secret.example"));
    assert!(!encoded.contains("private-token"));
    let mut wrong = scope();
    wrong.account.authority = api::AuthorityId::new("alliance").unwrap();
    assert_eq!(
        session_view(&value, &wrong, target()).unwrap_err().code,
        "stale_account"
    );
}

#[test]
fn queued_suspended_and_terminal_sessions_are_not_reported_as_playing() {
    for (status, expected) in [
        (
            1,
            api::RemoteSessionState::Queued {
                position: Some(8),
                wait_seconds: None,
            },
        ),
        (4, api::RemoteSessionState::Suspended),
        (
            7,
            api::RemoteSessionState::Finished {
                reason: api::TerminalReason::RemoteEnded,
            },
        ),
    ] {
        let value = json!({"sessionId":"seat","status":status,"queuePosition":8});
        assert_eq!(
            session_view(&value, &scope(), target()).unwrap().state,
            expected
        );
    }
}

#[test]
fn unsupported_operations_fail_and_capabilities_do_not_advertise_worker_or_extras() {
    let directory = tempfile::tempdir().unwrap();
    let module = fixture(directory.path(), "http://127.0.0.1:1");
    let request = Request::CatalogDefinitions(api::Empty {});
    assert_eq!(
        module
            .execute_provider(&request, &Cancellation::default(), None)
            .unwrap_err()
            .code,
        "unsupported_feature"
    );
    assert!(!capabilities().contains(&api::Capability::MediaWorker));
    assert!(!capabilities().contains(&api::Capability::AccountPin));
    assert!(!capabilities().contains(&api::Capability::Settings));
}

#[test]
fn cancellation_prevents_adapter_dispatch_without_network() {
    let directory = tempfile::tempdir().unwrap();
    let module = fixture(directory.path(), "http://127.0.0.1:1");
    let requests = Arc::new(requests::Requests::default());
    let permit = requests
        .admit("cancelled-provider", "sources.public.page")
        .unwrap();
    requests.cancel("cancelled-provider");
    let request = Request::AuthAuthorities(api::Empty {});
    assert_eq!(
        module
            .execute_provider(&request, &permit.token, None)
            .unwrap_err()
            .code,
        "cancelled"
    );
}

#[test]
fn rejected_create_preflight_has_no_allocation_dispatch_or_receipt() {
    let directory = tempfile::tempdir().unwrap();
    let module = fixture(directory.path(), "http://127.0.0.1:1");
    let request: Request = serde_json::from_value(json!({"method":"session.create","params":{
        "scope":null,"operation":"create-operation","target":{"game":"game","variant":"123"},
        "catalogRevision":"1","settingsRevision":0,
        "preferences":{"video":{"width":1280,"height":720,"encoding":null,"fps":null,"bitDepth":8,"chroma":"yuv420","hdr":false},"bitrateKbps":10000},
        "offer":{"version":1,"offerId":"offer","runtimeEpoch":1,"expiresAtMs":10000,
            "videoFormats":[{"encoding":"h264-annex-b","bitDepth":8,"chroma":"yuv420","dynamicRange":"sdr","maxWidth":1920,"maxHeight":1080,"maxFps":60}],
            "audioFormats":[],"input":{"keyboard":true,"relativeMouse":true,"absoluteMouse":true,"text":false,"gamepadSlots":1,"rumble":false},
            "limits":{"maxVideoAccessUnitBytes":1048576,"maxAudioPacketBytes":65536,"maxControlMessageBytes":65536,"maxBufferedVideoBytes":2097152,"maxBufferedVideoFrames":4,"maxBufferedAudioMs":100,"maxPendingInputEvents":128}}
    }})).unwrap();
    let cancellation = Cancellation::default();
    let result = module.provider_call(
        &request,
        &ProviderContext {
            cancellation: &cancellation,
            runtime_capabilities: None,
            gfn_settings: None,
        },
    );
    assert!(!result.dispatched);
    assert_eq!(
        result.allocation_disposition,
        Some(AllocationDisposition::NotDispatched)
    );
    assert!(result.allocation.is_none());
    assert_eq!(result.result.unwrap_err().code, "authentication_required");
}

#[test]
fn auto_hdr_preferences_reach_existing_native_codec_resolution_unchanged() {
    let preferences: api::StreamPreferences = serde_json::from_value(json!({
        "video":{"width":1920,"height":1080,"encoding":null,"fps":null,"bitDepth":10,"chroma":"yuv420","hdr":true},
        "bitrateKbps":42500
    })).unwrap();
    let global =
        json!({"codec":"h264","fps":120,"maxBitrateMbps":75,"audioOutputDevice":"device-owned"});
    let mapped = GfnModule::apply_stream_preferences(&global, &preferences);
    assert_eq!(mapped["codec"], "auto");
    assert_eq!(mapped["fps"], "auto");
    assert_eq!(mapped["colorQuality"], "10bit_420");
    assert_eq!(mapped["enableHdr"], true);
    assert_eq!(mapped["maxBitrateMbps"], 42.5);
    assert_eq!(mapped["audioOutputDevice"], "device-owned");
    for absent in ["color", "primaries", "transfer", "matrix", "range"] {
        assert!(mapped.get(absent).is_none());
    }
    assert_eq!(global["codec"], "h264");
    assert_eq!(global["fps"], 120);
    let capabilities = json!({"protocolVersion":8,"nativeHdrSupported":true,"videoBackends":[{
        "backend":"d3d11","platform":"windows","available":true,"codecs":[
            {"codec":"h264","available":true,"colorQualities":["8bit_420"]},
            {"codec":"h265","available":true,"colorQualities":["8bit_420","10bit_420"]},
            {"codec":"av1","available":true,"colorQualities":["8bit_420","10bit_420"]}
        ]
    }]});
    let resolved = StreamerService::embedded_session_settings(&mapped, &capabilities).unwrap();
    assert_eq!(resolved["codec"], "h265");
    assert_eq!(resolved["nativeHdrSupported"], true);
    assert_eq!(
        crate::frame_rate::request_frame_rate(&mapped, &json!({}), 1920, 1080),
        60
    );
}

#[test]
fn explicit_hevc_profile_preserves_requested_dimensions_rate_and_chroma() {
    let preferences: api::StreamPreferences = serde_json::from_value(json!({
        "video":{"width":3840,"height":2160,"encoding":"hevc-annex-b","fps":120,"bitDepth":10,"chroma":"yuv444","hdr":false},
        "bitrateKbps":75000
    })).unwrap();
    let settings = GfnModule::apply_stream_preferences(&json!({}), &preferences);
    assert_eq!(settings["codec"], "h265");
    assert_eq!(settings["resolution"], "3840x2160");
    assert_eq!(settings["fps"], 120);
    assert_eq!(settings["colorQuality"], "10bit_444");
    assert_eq!(settings["enableHdr"], false);
    assert_eq!(settings["maxBitrateMbps"], 75.0);
}

#[test]
fn host_scoped_snapshot_is_read_only_and_global_fallback_is_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let module = fixture(directory.path(), "http://127.0.0.1:1");
    let global = module.settings.lock().unwrap().all();
    let cancellation = Cancellation::default();
    let scoped = json!({"resolution":"3840x2160","codec":"auto","enableHdr":true});
    let context = ProviderContext {
        cancellation: &cancellation,
        runtime_capabilities: None,
        gfn_settings: Some(&scoped),
    };
    let mut effective = module.provider_settings(&context);
    assert_eq!(effective, scoped);
    effective["codec"] = json!("h265");
    assert_eq!(scoped["codec"], "auto");
    assert_eq!(module.settings.lock().unwrap().all(), global);
    let fallback = ProviderContext {
        gfn_settings: None,
        ..context
    };
    assert_eq!(module.provider_settings(&fallback), global);
}

#[test]
fn session_identity_and_receipt_cannot_be_rebound_by_the_request() {
    let view = session_view(&json!({"sessionId":"seat","status":2}), &scope(), target()).unwrap();
    let mut state = ProviderState::default();
    state.sessions.insert(
        session_correlation_key(&view.key),
        OwnedSession {
            view: view.clone(),
            scope: scope(),
        },
    );
    let mut foreign = view.key.clone();
    foreign.account.as_mut().unwrap().account = api::AccountId::new("another-player").unwrap();
    assert!(owned_session(&state, &foreign).is_err());
    assert!(owned_session(&state, &view.key).is_ok());
    assert_eq!(
        session_params(owned_session(&state, &view.key).unwrap())["ownerScope"],
        legacy_scope(&scope())
    );
    let directory = tempfile::tempdir().unwrap();
    let module = fixture(directory.path(), "http://127.0.0.1:1");
    let operation = api::OperationId::new("original-operation").unwrap();
    module.provider.lock().unwrap().allocations.insert(
        operation.as_str().into(),
        Allocation {
            ticket: api::AllocationTicket {
                operation: operation.clone(),
                receipt: api::ReceiptId::new("original-receipt").unwrap(),
                session: view.key,
            },
            settled: None,
        },
    );
    let request = Request::SessionResolveAllocation(api::ResolveAllocation {
        operation,
        receipt: api::ReceiptId::new("foreign-receipt").unwrap(),
        decision: api::Acceptance::Rejected,
    });
    assert_eq!(
        module
            .execute_provider(&request, &Cancellation::default(), None)
            .unwrap_err()
            .code,
        "invalid_receipt"
    );
    assert!(
        module.provider.lock().unwrap().allocations["original-operation"]
            .settled
            .is_none()
    );
}
