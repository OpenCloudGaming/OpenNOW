use opennow_plugin_api::media::*;
use opennow_plugin_api::provider::*;
use opennow_plugin_api::{EXAMPLE_PLUGIN_ID, PluginManifest};
use serde_json::{Value, json};

fn video() -> Value {
    json!({"encoding":"h264-annex-b","width":1280,"height":720,
        "fps":60,"bitDepth":8,"chroma":"yuv420",
        "color":{"primaries":"bt709","transfer":"bt709","matrix":"bt709","range":"limited","chromaLocation":"left"}})
}

fn input() -> Value {
    json!({"keyboard":true,"relativeMouse":true,"absoluteMouse":true,"text":false,"gamepadSlots":1,"rumble":true})
}

fn offer() -> Value {
    json!({"version":1,"offerId":"host-offer","runtimeEpoch":19,"expiresAtMs":10000,
        "videoFormats":[{"encoding":"h264-annex-b","bitDepth":8,"chroma":"yuv420","dynamicRange":"sdr","maxWidth":1920,"maxHeight":1080,"maxFps":60}],
        "audioFormats":[{"codec":"opus","sampleRate":48000,"channels":2}],
        "input":input(),"limits":{"maxVideoAccessUnitBytes":1048576,"maxAudioPacketBytes":65536,
        "maxControlMessageBytes":65536,"maxBufferedVideoBytes":2097152,"maxBufferedVideoFrames":4,"maxBufferedAudioMs":100,"maxPendingInputEvents":128}})
}

fn create() -> Value {
    json!({"scope":null,"operation":"original-create","target":{"game":"game1","variant":"default"},
        "catalogRevision":"revision1","settingsRevision":1,"preferences":{"video":{"width":1280,"height":720,"encoding":null,"fps":null,"bitDepth":8,"chroma":"yuv420","hdr":false},"bitrateKbps":10000},"offer":offer()})
}

fn session() -> Value {
    json!({"key":{"account":null,"remoteId":"remote-seat"},"target":{"game":"game1","variant":"default"},"state":{"state":"ready"}})
}

fn request(method: &str, params: Value) -> Value {
    json!({"v":2,"type":"request","epoch":7,"id":"rpc1","timeoutMs":10000,
        "request":{"method":method,"params":params}})
}

fn response(method: &str, result: Value) -> Value {
    json!({"v":2,"type":"response","epoch":7,"id":"rpc1",
        "outcome":{"status":"success","reply":{"method":method,"result":result}},"effects":[],"allocation":null})
}

fn decode_request(value: Value) -> HostRequestV2 {
    let HostMessageV2::Request(request) =
        HostMessageV2::decode(&serde_json::to_vec(&value).unwrap()).unwrap()
    else {
        panic!("not a request")
    };
    *request
}

fn decode_response(value: Value) -> ProviderResponseV2 {
    let PluginMessageV2::Response(response) =
        PluginMessageV2::decode(&serde_json::to_vec(&value).unwrap()).unwrap();
    response
}

fn manifest() -> Value {
    json!({"schemaVersion":2,"protocolVersion":2,"id":"org.opennow.example.provider",
        "name":"Example provider","version":"1.0.0","publisher":"Example author","description":"Local protocol fixture",
        "capabilities":["auth.anonymous.v2","catalog.public.v2","catalog.details.v2","launch.v2","sessions.v2","media.worker.v1"],
        "authKinds":["anonymous"],"entrypoints":{"x86_64-unknown-linux-gnu":{"control":"bin/provider","media":"bin/provider"}},
        "files":[{"path":"bin/provider","sha256":"0".repeat(64)}]})
}

#[test]
fn golden_anonymous_auth_request_and_response() {
    let source = br#"{"v":2,"type":"request","epoch":7,"id":"rpc1","timeoutMs":10000,"request":{"method":"auth.status","params":{}}}"#;
    let message = HostMessageV2::decode(source).unwrap();
    assert_eq!(
        serde_json::to_value(&message).unwrap(),
        serde_json::from_slice::<Value>(source).unwrap()
    );
    let HostMessageV2::Request(request) = message else {
        panic!("not a request")
    };
    let reply = decode_response(response("auth.status", json!({"state":"not-required"})));
    reply.validate_for(&request).unwrap();
    assert!(matches!(
        reply.outcome.reply(),
        Some(ProviderReply::AuthStatus(AuthState::NotRequired))
    ));
}

#[test]
fn protocol_versions_are_independent_and_v1_remains_valid() {
    assert_eq!(opennow_plugin_api::PROTOCOL_VERSION, 1);
    assert_eq!(PROVIDER_PROTOCOL_VERSION, 2);
    assert_eq!(MEDIA_PROTOCOL_VERSION, 1);
    let v1 = json!({"v":1,"type":"request","epoch":7,"id":"rpc1","op":"catalog.page","args":{},"timeoutMs":10000});
    assert!(serde_json::from_value::<opennow_plugin_api::HostMessage>(v1.clone()).is_ok());
    assert!(HostMessageV2::decode(&serde_json::to_vec(&v1).unwrap()).is_err());
    for version in [0, 1, 3, 6] {
        let mut invalid = request("auth.status", json!({}));
        invalid["v"] = json!(version);
        assert!(HostMessageV2::decode(&serde_json::to_vec(&invalid).unwrap()).is_err());
    }
}

#[test]
fn finite_protocol_rejects_unknown_methods_fields_and_host_identity_injection() {
    for value in [
        request("execute", json!({"method":"native.start"})),
        request("auth.status", json!({"sourceId":"org.opennow.geforce-now"})),
        request(
            "session.poll",
            json!({"account":null,"remoteId":"x","instanceEpoch":4}),
        ),
        request(
            "settings.set",
            json!({"scope":{"account":null},"expectedRevision":1,"key":"script","value":{"kind":"code","value":"x"}}),
        ),
    ] {
        assert!(
            HostMessageV2::decode(&serde_json::to_vec(&value).unwrap()).is_err(),
            "{value}"
        );
    }
    for field in ["sourceId", "path", "arguments", "qml"] {
        let mut value = response("auth.status", json!({"state":"not-required"}));
        value[field] = json!("forged");
        assert!(PluginMessageV2::decode(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    assert!(PluginMessageV2::decode(&serde_json::to_vec(&response("session.poll",json!({"key":{"account":null,"remoteId":"x"},"target":{"game":"g","variant":"v"},"state":{"state":"streaming"}}))).unwrap()).is_err());
}

#[test]
fn frame_ids_deadlines_and_collections_are_bounded() {
    for (field, value) in [
        ("epoch", json!(0)),
        ("id", json!("")),
        ("id", json!("x".repeat(65))),
        ("timeoutMs", json!(0)),
        ("timeoutMs", json!(120001)),
    ] {
        let mut invalid = request("auth.status", json!({}));
        invalid[field] = value;
        assert!(HostMessageV2::decode(&serde_json::to_vec(&invalid).unwrap()).is_err());
    }
    assert!(HostMessageV2::decode(&vec![b' '; opennow_plugin_api::MAX_FRAME_BYTES + 1]).is_err());
    assert!(SessionId::new("x".repeat(257)).is_err());
    assert!(SessionId::new("remote\u{0}id").is_err());
    assert!(Text::<256>::new("é".repeat(129)).is_err());
    assert!(List::<u8, 4>::new(vec![1; 5]).is_err());
    assert!(SecretBytes::new(vec![1; 262145]).is_err());
}

#[test]
fn private_auth_navigation_and_bootstrap_are_redacted() {
    let marker = "SYNTHETIC_SECRET_SENTINEL";
    let secret = SecretString::new(marker).unwrap();
    assert!(!format!("{secret:?}").contains(marker));
    assert_eq!(serde_json::to_value(&secret).unwrap(), marker);
    let challenge = AuthChallenge::Browser {
        attempt: AttemptId::new(marker).unwrap(),
        authorization: SecretString::new(format!(
            "https://example.invalid/authorize?state={marker}"
        ))
        .unwrap(),
        expires_at_ms: 10000,
        poll_after_ms: 1000,
    };
    challenge.validate().unwrap();
    assert!(!format!("{challenge:?}").contains(marker));
    let outcome =
        ProviderOutcome::success(ProviderReply::AuthBegin(AuthState::Pending { challenge }));
    assert!(!format!("{outcome:?}").contains(marker));
    let private = SecretBytes::new(marker.as_bytes().to_vec()).unwrap();
    assert!(!format!("{private:?}").contains(marker));
    assert_eq!(private.expose_secret(), marker.as_bytes());
    assert!(PublicUrl::new(format!("https://example.invalid/?token={marker}")).is_err());
    assert!(PublicUrl::new("file:///private".into()).is_err());
    assert!(PublicUrl::new("http://127.0.0.1:1234/pair".into()).is_ok());
    let mut malformed = request("auth.status", json!({}));
    malformed[marker] = json!(marker);
    let error = HostMessageV2::decode(&serde_json::to_vec(&malformed).unwrap()).unwrap_err();
    assert!(!format!("{error:?}").contains(marker));
}

#[test]
fn allocation_receipts_bind_original_operation_and_anonymous_owner() {
    let create_request = decode_request(request("session.create", create()));
    let mut accepted = response("session.create", json!({"session":session()}));
    accepted["allocation"] = json!({"operation":"original-create","receipt":"receipt1","session":{"account":null,"remoteId":"remote-seat"}});
    decode_response(accepted.clone())
        .validate_for(&create_request)
        .unwrap();
    for replacement in [
        json!({"operation":"other-create","receipt":"receipt1","session":{"account":null,"remoteId":"remote-seat"}}),
        json!({"operation":"original-create","receipt":"receipt1","session":{"account":{"authority":"x","account":"other"},"remoteId":"remote-seat"}}),
        json!({"operation":"original-create","receipt":"receipt1","session":{"account":null,"remoteId":"other-seat"}}),
        Value::Null,
    ] {
        let mut invalid = accepted.clone();
        invalid["allocation"] = replacement;
        assert!(
            decode_response(invalid)
                .validate_for(&create_request)
                .is_err()
        );
    }
    let mut cancelled = accepted;
    cancelled["outcome"] =
        json!({"status":"failure","error":{"code":"cancelled","retryAfterMs":null}});
    cancelled["effects"] = json!([{"kind":"cleanup-required","operation":"original-create"}]);
    let cancelled = decode_response(cancelled);
    cancelled.validate_for(&create_request).unwrap();
    assert!(cancelled.allocation.is_some());
    assert_eq!(cancelled.effects.len(), 1);
    assert!(
        cancelled
            .validate_for(&decode_request(request("auth.status", json!({}))))
            .is_err()
    );
}

#[test]
fn responses_cannot_change_correlation_method_scope_or_session() {
    let pending = decode_request(request(
        "session.poll",
        json!({"account":null,"remoteId":"remote-seat"}),
    ));
    decode_response(response("session.poll", session()))
        .validate_for(&pending)
        .unwrap();
    for (field, value) in [("id", json!("another")), ("epoch", json!(8))] {
        let mut invalid = response("session.poll", session());
        invalid[field] = value;
        assert!(decode_response(invalid).validate_for(&pending).is_err());
    }
    assert!(
        decode_response(response("auth.status", json!({"state":"not-required"})))
            .validate_for(&pending)
            .is_err()
    );
    let mut wrong_seat = session();
    wrong_seat["key"]["remoteId"] = json!("other");
    assert!(
        decode_response(response("session.poll", wrong_seat))
            .validate_for(&pending)
            .is_err()
    );
}

#[test]
fn anonymous_library_uses_public_scope_without_an_account() {
    let page = decode_request(request(
        "catalog.library",
        json!({"scope":{"kind":"public"},"query":{"query":"","cursor":null,"limit":20}}),
    ));
    let public_page = json!({"items":[],"nextCursor":null,"coverage":"complete","revision":"r1","scope":{"kind":"public"}});
    decode_response(response("catalog.library", public_page.clone()))
        .validate_for(&page)
        .unwrap();
    let mut account_page = public_page;
    account_page["scope"] = json!({"kind":"account","scope":{"account":{"authority":"example","account":"player-1"},"revision":1}});
    assert!(
        decode_response(response("catalog.library", account_page))
            .validate_for(&page)
            .is_err()
    );
    let inspect = decode_request(request(
        "launch.inspect",
        json!({"scope":null,"target":{"game":"game1","variant":"default"},"catalogRevision":"r1"}),
    ));
    decode_response(response(
        "launch.inspect",
        json!({"state":"ready","target":{"game":"game1","variant":"default"},"revision":"r1"}),
    ))
    .validate_for(&inspect)
    .unwrap();
}

#[test]
fn prepared_worker_requires_current_supported_offer_and_no_path() {
    let expected: NativeOffer = serde_json::from_value(offer()).unwrap();
    expected.validate().unwrap();
    let accepted = json!({"offerId":"host-offer","runtimeEpoch":19,"video":video(),"audio":{"codec":"opus","sampleRate":48000,"channels":2},"input":input()});
    let media: AcceptedMedia = serde_json::from_value(accepted.clone()).unwrap();
    media.validate_against(&expected, 9999).unwrap();
    assert!(media.validate_against(&expected, 10000).is_err());
    for (field, value) in [
        ("offerId", json!("other-offer")),
        ("runtimeEpoch", json!(20)),
    ] {
        let mut invalid = accepted.clone();
        invalid[field] = value;
        assert!(
            serde_json::from_value::<AcceptedMedia>(invalid)
                .unwrap()
                .validate_against(&expected, 9999)
                .is_err()
        );
    }
    let pending = decode_request(request(
        "session.prepare",
        json!({"session":{"account":null,"remoteId":"remote-seat"},"offer":offer()}),
    ));
    let plan = json!({"accepted":accepted,"bootstrap":"AQID"});
    let returned = decode_response(response("session.prepare", plan.clone()));
    returned.validate_for_at(&pending, 9999).unwrap();
    assert!(returned.validate_for_at(&pending, 10000).is_err());
    let mut path_injection = plan;
    path_injection["executable"] = json!("/bin/sh");
    assert!(
        PluginMessageV2::decode(
            &serde_json::to_vec(&response("session.prepare", path_injection)).unwrap()
        )
        .is_err()
    );
}

#[test]
fn formats_and_settings_refuse_nonfinite_and_inconsistent_values() {
    assert!(FiniteNumber::new(f64::NAN).is_err());
    assert!(FiniteNumber::new(f64::INFINITY).is_err());
    assert!(FiniteNumber::new(1e13).is_err());
    assert!(SettingText::new("").is_ok());
    let mut hdr = video();
    hdr["color"]["transfer"] = json!("pq");
    assert!(
        serde_json::from_value::<VideoFormat>(hdr)
            .unwrap()
            .validate()
            .is_err()
    );
    let mut fractional = video();
    fractional["fps"] = json!(59.94);
    assert!(serde_json::from_value::<VideoFormat>(fractional).is_err());
    let invalid = json!({"revision":1,"settings":[{"key":"fps","label":"FPS","control":{"kind":"integer","min":1,"max":60,"step":1},"value":{"kind":"integer","value":120}}]});
    assert!(
        PluginMessageV2::decode(&serde_json::to_vec(&response("settings.get", invalid)).unwrap())
            .is_err()
    );
}

#[test]
fn v2_manifest_requires_full_provider_roles_and_rejects_v1_confusion() {
    let valid: ProviderManifest = serde_json::from_value(manifest()).unwrap();
    valid.validate().unwrap();
    assert_eq!(serde_json::to_value(valid).unwrap(), manifest());
    assert!(serde_json::from_value::<PluginManifest>(manifest()).is_err());
    for (field, value) in [
        ("protocolVersion", json!(1)),
        ("schemaVersion", json!(1)),
        ("capabilities", json!(["catalog.v1"])),
        ("authKinds", json!([])),
        ("arguments", json!(["-c", "evil"])),
        ("qml", json!("View.qml")),
    ] {
        let mut invalid = manifest();
        invalid[field] = value;
        assert!(
            serde_json::from_value::<ProviderManifest>(invalid).is_err(),
            "{field}"
        );
    }
    for path in ["../escape", "/bin/provider", "missing", "bin\\provider"] {
        let mut invalid = manifest();
        invalid["entrypoints"]["x86_64-unknown-linux-gnu"]["media"] = json!(path);
        assert!(serde_json::from_value::<ProviderManifest>(invalid).is_err());
    }
    let v1 = json!({"schemaVersion":1,"protocolVersion":1,"id":EXAMPLE_PLUGIN_ID,"name":"Catalog","version":"1.0.0","publisher":"Example","description":"Read only","capabilities":["catalog.v1"],"entrypoints":{"x86_64-unknown-linux-gnu":"bin/catalog"},"files":[{"path":"bin/catalog","sha256":"0".repeat(64)}]});
    assert!(serde_json::from_value::<PluginManifest>(v1.clone()).is_ok());
    assert!(serde_json::from_value::<ProviderManifest>(v1).is_err());
}

#[test]
fn capability_gates_do_not_fabricate_optional_service_support() {
    let auth = decode_request(request(
        "auth.begin",
        json!({"authority":null,"kind":"device-code","remember":true}),
    ));
    assert!(!auth.request.permits(&[Capability::AuthAnonymous]));
    assert!(auth.request.permits(&[Capability::AuthDeviceCode]));
    let call = decode_request(request(
        "account.storage",
        json!({"account":{"authority":"a","account":"b"},"revision":1}),
    ));
    assert!(!call.request.permits(&[Capability::Sessions]));
    assert!(call.request.permits(&[Capability::Storage]));
}

#[test]
fn bootstrap_uses_bounded_canonical_base64_not_arrays_or_debug_payloads() {
    let payload = SecretBytes::new(vec![1, 2, 3]).unwrap();
    assert_eq!(serde_json::to_value(&payload).unwrap(), "AQID");
    assert_eq!(
        serde_json::from_value::<SecretBytes>(json!("AQID"))
            .unwrap()
            .expose_secret(),
        &[1, 2, 3]
    );
    for invalid in [json!([1, 2, 3]), json!("not base64!"), json!("AQID\n")] {
        assert!(serde_json::from_value::<SecretBytes>(invalid).is_err());
    }
    let maximum = SecretBytes::new(vec![1; 256 * 1024]).unwrap();
    let encoded = serde_json::to_vec(&maximum).unwrap();
    assert!(encoded.len() < opennow_plugin_api::MAX_FRAME_BYTES);
    assert_eq!(
        serde_json::from_slice::<SecretBytes>(&encoded)
            .unwrap()
            .expose_secret()
            .len(),
        256 * 1024
    );
    assert!(serde_json::from_value::<SecretBytes>(json!("A".repeat(400_000))).is_err());
    assert_eq!(format!("{maximum:?}"), "SecretBytes([redacted])");
}

#[test]
fn media_support_is_a_tuple_and_input_can_only_narrow() {
    let offered: NativeOffer = serde_json::from_value(offer()).unwrap();
    let accepted = json!({"offerId":"host-offer","runtimeEpoch":19,"video":video(),"audio":null,"input":input()});
    for (field, value) in [
        ("width", json!(1921)),
        ("fps", json!(61)),
        ("encoding", json!("hevc-annex-b")),
        ("chroma", json!("yuv444")),
    ] {
        let mut invalid = accepted.clone();
        invalid["video"][field] = value;
        assert!(
            serde_json::from_value::<AcceptedMedia>(invalid)
                .unwrap()
                .validate_against(&offered, 1)
                .is_err(),
            "{field}"
        );
    }
    let mut excess_input = accepted.clone();
    excess_input["input"]["gamepadSlots"] = json!(2);
    assert!(
        serde_json::from_value::<AcceptedMedia>(excess_input)
            .unwrap()
            .validate_against(&offered, 1)
            .is_err()
    );
    let mut narrowed = accepted;
    narrowed["input"]["keyboard"] = json!(false);
    narrowed["input"]["gamepadSlots"] = json!(0);
    serde_json::from_value::<AcceptedMedia>(narrowed)
        .unwrap()
        .validate_against(&offered, 1)
        .unwrap();
    assert!(OfferId::new("x".repeat(129)).is_err());
    let mut unsupported_hdr = video();
    unsupported_hdr["hdrStatic"] = json!({"maxLuminance":1000});
    assert!(serde_json::from_value::<VideoFormat>(unsupported_hdr).is_err());
}

#[test]
fn private_auth_navigation_refuses_executable_schemes_and_hot_polling() {
    for authorization in [
        "file:///private",
        "javascript:alert(1)",
        "https://name:password@example.invalid",
        "http://remote.example.invalid/auth",
    ] {
        let challenge = AuthChallenge::Browser {
            attempt: AttemptId::new("login").unwrap(),
            authorization: SecretString::new(authorization).unwrap(),
            expires_at_ms: 10000,
            poll_after_ms: 1000,
        };
        assert!(challenge.validate().is_err());
    }
    let challenge = AuthChallenge::DeviceCode {
        attempt: AttemptId::new("login").unwrap(),
        user_code: SecretString::new("TESTCODE").unwrap(),
        verification_uri: PublicUrl::new("https://example.invalid/pair".into()).unwrap(),
        expires_at_ms: 10000,
        poll_after_ms: 0,
    };
    assert!(challenge.validate().is_err());
}

#[test]
fn authorization_approval_does_not_commit_and_long_provider_backoff_is_preserved() {
    let approved = decode_response(response(
        "auth.poll",
        json!({"state":"authorized","attempt":"pending-login"}),
    ));
    assert!(matches!(
        approved.outcome.reply(),
        Some(ProviderReply::AuthPoll(AuthState::Authorized { .. }))
    ));
    let challenge = AuthChallenge::DeviceCode {
        attempt: AttemptId::new("pending-login").unwrap(),
        user_code: SecretString::new("EXAMPLE").unwrap(),
        verification_uri: PublicUrl::new("https://example.invalid/pair".into()).unwrap(),
        expires_at_ms: 7_200_000,
        poll_after_ms: 3_600_000,
    };
    challenge.validate().unwrap();
    assert_eq!(
        serde_json::to_value(challenge).unwrap()["pollAfterMs"],
        3_600_000
    );
}

#[test]
fn optional_description_is_honest_and_external_hdr_is_not_advertised() {
    let details = json!({"game":{"id":"game","title":"Game","artwork":null,"subtitle":null,"badges":[],"availability":"available"},"description":null,"variants":[],"revision":"1","scope":{"kind":"public"}});
    let parsed = decode_response(response("catalog.details", details));
    assert!(matches!(
        parsed.outcome.reply(),
        Some(ProviderReply::CatalogDetails(GameDetails {
            description: None,
            ..
        }))
    ));
    let mut hdr_offer = offer();
    hdr_offer["videoFormats"][0]["encoding"] = json!("hevc-annex-b");
    hdr_offer["videoFormats"][0]["bitDepth"] = json!(10);
    hdr_offer["videoFormats"][0]["dynamicRange"] = json!("hdr10");
    assert!(
        serde_json::from_value::<NativeOffer>(hdr_offer)
            .unwrap()
            .validate()
            .is_err()
    );
    let mut removed_field = video();
    removed_field["hdrStatic"] = Value::Null;
    assert!(serde_json::from_value::<VideoFormat>(removed_field).is_err());
}

#[test]
fn requested_video_preserves_auto_without_inventing_accepted_color() {
    let pending = decode_request(request("session.create", create()));
    let ProviderRequest::SessionCreate(pending) = pending.request else {
        panic!("not create")
    };
    assert_eq!(pending.preferences.video.encoding, None);
    assert_eq!(pending.preferences.video.fps, None);
    let encoded = serde_json::to_value(&pending.preferences.video).unwrap();
    assert_eq!(encoded["encoding"], Value::Null);
    assert_eq!(encoded["fps"], Value::Null);
    for field in ["color", "primaries", "transfer", "matrix", "range"] {
        assert!(encoded.get(field).is_none());
        let mut injected = encoded.clone();
        injected[field] = json!("bt709");
        assert!(serde_json::from_value::<RequestedVideo>(injected).is_err());
    }
    let mut hdr = encoded;
    hdr["bitDepth"] = json!(10);
    hdr["hdr"] = json!(true);
    let desired: RequestedVideo = serde_json::from_value(hdr).unwrap();
    desired.validate().unwrap();
    assert_eq!(desired.encoding, None);
    assert_eq!(desired.fps, None);
    assert!(desired.hdr);
}

#[test]
fn requested_video_rejects_invalid_explicit_preferences_without_clamping() {
    let valid = create();
    for (field, value) in [
        ("width", json!(0)),
        ("height", json!(8193)),
        ("fps", json!(0)),
        ("fps", json!(361)),
        ("fps", json!(59.94)),
        ("bitDepth", json!(12)),
        ("hdr", json!(true)),
    ] {
        let mut invalid = valid.clone();
        invalid["preferences"]["video"][field] = value;
        assert!(
            HostMessageV2::decode(
                &serde_json::to_vec(&request("session.create", invalid)).unwrap()
            )
            .is_err(),
            "{field}"
        );
    }
    let mut incompatible = valid.clone();
    incompatible["preferences"]["video"]["encoding"] = json!("h264-annex-b");
    incompatible["preferences"]["video"]["bitDepth"] = json!(10);
    assert!(
        HostMessageV2::decode(
            &serde_json::to_vec(&request("session.create", incompatible)).unwrap()
        )
        .is_err()
    );
    let mut explicit = valid;
    explicit["preferences"]["video"]["encoding"] = json!("hevc-annex-b");
    explicit["preferences"]["video"]["fps"] = json!(120);
    let ProviderRequest::SessionCreate(parsed) =
        decode_request(request("session.create", explicit)).request
    else {
        panic!("not create")
    };
    assert_eq!(
        parsed.preferences.video.encoding,
        Some(VideoEncoding::HevcAnnexB)
    );
    assert_eq!(parsed.preferences.video.fps, Some(120));
}

#[test]
fn no_allocation_proof_requires_exact_operation_and_no_known_session() {
    let pending = decode_request(request(
        "session.reconcile",
        json!({"scope":null,"operation":"not-created","session":null}),
    ));
    let proof = decode_response(response(
        "session.reconcile",
        json!({"state":"not-allocated","operation":"not-created"}),
    ));
    proof.validate_for(&pending).unwrap();
    let wrong = decode_response(response(
        "session.reconcile",
        json!({"state":"not-allocated","operation":"another-operation"}),
    ));
    assert!(wrong.validate_for(&pending).is_err());
    let known = decode_request(request(
        "session.reconcile",
        json!({"scope":null,"operation":"not-created","session":{"account":null,"remoteId":"known-seat"}}),
    ));
    assert!(proof.validate_for(&known).is_err());
}

#[test]
fn pending_allocation_recovery_binds_ticket_operation_owner_and_exact_session() {
    let owner = json!({"authority":"demo","account":"original-account"});
    let mut seat = session();
    seat["key"]["account"] = owner.clone();
    let params = json!({"scope":{"account":owner,"revision":1},"operation":"original-create","session":null});
    let pending = decode_request(request("session.reconcile", params.clone()));
    let recovered = json!({"state":"pending-allocation","session":seat,
        "ticket":{"operation":"original-create","receipt":"original-receipt","session":seat["key"]}});
    decode_response(response("session.reconcile", recovered.clone()))
        .validate_for(&pending)
        .unwrap();
    let mut known = params.clone();
    known["session"] = seat["key"].clone();
    decode_response(response("session.reconcile", recovered.clone()))
        .validate_for(&decode_request(request("session.reconcile", known.clone())))
        .unwrap();
    let mut wrong_operation = recovered.clone();
    wrong_operation["ticket"]["operation"] = json!("different-create");
    assert!(
        decode_response(response("session.reconcile", wrong_operation))
            .validate_for(&pending)
            .is_err()
    );
    let mut wrong_ticket_seat = recovered.clone();
    wrong_ticket_seat["ticket"]["session"]["remoteId"] = json!("other-seat");
    assert!(
        decode_response(response("session.reconcile", wrong_ticket_seat))
            .validate_for(&pending)
            .is_err()
    );
    let mut wrong_owner = recovered.clone();
    wrong_owner["session"]["key"]["account"]["account"] = json!("other-account");
    wrong_owner["ticket"]["session"] = wrong_owner["session"]["key"].clone();
    assert!(
        decode_response(response("session.reconcile", wrong_owner))
            .validate_for(&pending)
            .is_err()
    );
    known["session"]["remoteId"] = json!("already-known-other-seat");
    assert!(
        decode_response(response("session.reconcile", recovered))
            .validate_for(&decode_request(request("session.reconcile", known)))
            .is_err()
    );
}

#[test]
fn public_revision_response_rejects_values_that_qml_cannot_round_trip() {
    let revision = (1_u64 << 53) + 1;
    let qml_number = revision as f64;
    assert_ne!(qml_number as u64, revision);
    let payload = response(
        "accounts.list",
        json!({"accounts":[],"selected":null,"revision":revision}),
    );
    assert!(
        PluginMessageV2::decode(&serde_json::to_vec(&payload).unwrap()).is_err(),
        "SDK accepted a public revision that changes through QML Number"
    );
}

#[test]
fn public_revision_request_rejects_an_unsafe_account_scope() {
    let payload = request(
        "account.subscription",
        json!({"account":{"authority":"provider","account":"user"},"revision":(1_u64<<53)+1}),
    );
    assert!(
        HostMessageV2::decode(&serde_json::to_vec(&payload).unwrap()).is_err(),
        "SDK accepted an unsafe Qt-facing scope revision"
    );
}

#[test]
fn public_revision_safe_boundaries_round_trip_without_changing_wire_values() {
    for revision in [0, MAX_PUBLIC_REVISION] {
        let payload = response(
            "accounts.list",
            json!({"accounts":[],"selected":null,"revision":revision}),
        );
        let message = PluginMessageV2::decode(&serde_json::to_vec(&payload).unwrap()).unwrap();
        assert_eq!(serde_json::to_value(&message).unwrap(), payload);
        let round_trip = (revision as f64) as u64;
        assert_eq!(round_trip, revision);
        let request = decode_request(request(
            "account.subscription",
            json!({"account":{"authority":"provider","account":"user"},"revision":round_trip}),
        ));
        let ProviderRequest::SubscriptionGet(scope) = request.request else {
            panic!("unexpected operation")
        };
        assert_eq!(scope.revision, revision);
    }
}

#[test]
fn all_public_numeric_revision_fields_and_effect_scopes_reject_unsafe_values() {
    for revision in [MAX_PUBLIC_REVISION + 1, MAX_PUBLIC_REVISION + 2, u64::MAX] {
        let scope =
            json!({"account":{"authority":"provider","account":"user"},"revision":revision});
        let account = json!({"key":scope["account"],"name":"Demo account","persistence":"durable","reauthenticationRequired":false,"pinLocked":false});
        let mut create_request = create();
        create_request["settingsRevision"] = json!(revision);
        for payload in [
            request("account.subscription", scope.clone()),
            request(
                "catalog.library",
                json!({"scope":{"kind":"account","scope":scope},"query":{}}),
            ),
            request(
                "settings.set",
                json!({"scope":{"account":null},"expectedRevision":revision,"key":"enabled","value":{"kind":"boolean","value":true}}),
            ),
            request("settings.get", json!({"account":scope})),
            request("session.create", create_request),
        ] {
            assert!(HostMessageV2::decode(&serde_json::to_vec(&payload).unwrap()).is_err());
        }
        let mut effect_response = response("provider.shutdown", json!({}));
        effect_response["effects"] = json!([{"kind":"auth-changed","revision":revision}]);
        let mut invalidation = response("provider.shutdown", json!({}));
        invalidation["effects"] = json!([{"kind":"catalog-invalidated","scope":{"kind":"account","scope":scope},"revision":"opaque"}]);
        for payload in [
            response(
                "auth.status",
                json!({"state":"signed-in","account":account,"revision":revision}),
            ),
            response(
                "accounts.list",
                json!({"accounts":[],"selected":null,"revision":revision}),
            ),
            response("settings.get", json!({"revision":revision,"settings":[]})),
            response(
                "catalog.library",
                json!({"items":[],"nextCursor":null,"coverage":"unknown","revision":"opaque","scope":{"kind":"account","scope":scope}}),
            ),
            effect_response,
            invalidation,
        ] {
            assert!(PluginMessageV2::decode(&serde_json::to_vec(&payload).unwrap()).is_err());
        }
    }
}

#[test]
fn constructed_requests_replies_and_effects_validate_revisions_before_serialization() {
    let scope = AccountScope {
        account: AccountKey {
            authority: AuthorityId::new("provider").unwrap(),
            account: AccountId::new("user").unwrap(),
        },
        revision: MAX_PUBLIC_REVISION + 1,
    };
    let call = ProviderRequest::SubscriptionGet(scope);
    assert!(call.validate().is_err());
    assert!(serde_json::to_value(&call).is_err());
    let mut create_request = decode_request(request("session.create", create()));
    let ProviderRequest::SessionCreate(create) = &mut create_request.request else {
        panic!("unexpected operation")
    };
    create.settings_revision = MAX_PUBLIC_REVISION + 1;
    assert!(create_request.validate().is_err());
    let mut setting = decode_request(request(
        "settings.set",
        json!({"scope":{"account":null},"expectedRevision":0,"key":"enabled","value":{"kind":"boolean","value":true}}),
    ));
    let ProviderRequest::SettingsSet(set) = &mut setting.request else {
        panic!("unexpected operation")
    };
    set.expected_revision = MAX_PUBLIC_REVISION + 1;
    assert!(setting.validate().is_err());
    let result = ProviderReply::AccountsList(Accounts {
        accounts: List::default(),
        selected: None,
        revision: MAX_PUBLIC_REVISION + 1,
    });
    assert!(result.validate().is_err());
    assert!(serde_json::to_value(&result).is_err());
    assert!(
        SettingsView {
            revision: MAX_PUBLIC_REVISION + 1,
            settings: List::default()
        }
        .validate()
        .is_err()
    );
    let mut reply = decode_response(response("provider.shutdown", json!({})));
    reply.effects = List::new(vec![ProviderEffect::AuthChanged {
        revision: MAX_PUBLIC_REVISION + 1,
    }])
    .unwrap();
    assert!(reply.validate().is_err());
    assert!(serde_json::to_value(&reply).is_err());
}

#[test]
fn private_process_and_media_epochs_remain_full_u64_and_catalog_revisions_stay_opaque() {
    let mut payload = request("auth.status", json!({}));
    payload["epoch"] = json!(u64::MAX);
    let pending = decode_request(payload);
    assert_eq!(pending.epoch.get(), u64::MAX);
    let mut answer = response("auth.status", json!({"state":"not-required"}));
    answer["epoch"] = json!(u64::MAX);
    let reply = decode_response(answer.clone());
    reply.validate_for(&pending).unwrap();
    assert_eq!(
        serde_json::to_value(PluginMessageV2::Response(reply)).unwrap(),
        answer
    );
    let mut private_offer = offer();
    private_offer["runtimeEpoch"] = json!(u64::MAX);
    let private_offer: NativeOffer = serde_json::from_value(private_offer).unwrap();
    private_offer.validate().unwrap();
    assert_eq!(private_offer.runtime_epoch, u64::MAX);
    let payload = response(
        "catalog.public",
        json!({"items":[],"nextCursor":null,"coverage":"unknown","revision":u64::MAX.to_string(),"scope":{"kind":"public"}}),
    );
    let decoded = decode_response(payload.clone());
    assert_eq!(
        serde_json::to_value(PluginMessageV2::Response(decoded)).unwrap(),
        payload
    );
}
