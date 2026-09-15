use super::catalog_actions::LaunchStatus;
use super::tests::{auth_fixture, mock_requests, test_service};
use super::*;

fn service(url: &str) -> (GfnService, PathBuf) {
    let (mut service, path) = test_service(url);
    service.endpoints.graphql = format!("{url}/graphql");
    service.endpoints.account_linking = format!("{url}/graphql");
    service.endpoints.subscription = format!("{url}/subscription");
    service.endpoints.server_info = Some(format!("{url}/vpc"));
    let mut state = service.state.lock().unwrap();
    state.session = Some(auth_fixture("fixture-account"));
    state.restore_attempted = true;
    drop(state);
    (service, path)
}

fn vpc() -> (u16, Value) {
    (200, json!({"requestStatus":{"serverId":"fixture-vpc"}}))
}

fn platform_client(
    id: &str,
    title: &str,
    app_store: &str,
    variant_id: &str,
    status: Value,
) -> Value {
    json!({"id":id,"title":title,
        "images":{"HERO_IMAGE":[{"url":format!("https://cdn.example.invalid/{id}.jpg"),"width":1920,"height":1080}]},
        "variants":[{"appStore":app_store,"id":variant_id,"gfn":{"status":status}}],
        "gfn":{"playType":"PLATFORM_CLIENT"}})
}

fn store_apps(items: Vec<Value>) -> (u16, Value) {
    (200, json!({"data":{"apps":{"items":items}}}))
}

fn subscription(addon: bool) -> (u16, Value) {
    let addons = if addon {
        json!([{"type":"STORAGE","subType":"PERMANENT_STORAGE","status":"OK",
            "attributes":[{"key":"TOTAL_STORAGE_SIZE_IN_GB","textValue":"512"}]}])
    } else {
        json!([])
    };
    (
        200,
        json!({"allottedTimeInMinutes":600.0,"purchasedTimeInMinutes":0.0,"rolledOverTimeInMinutes":0.0,
            "totalTimeInMinutes":600.0,"remainingTimeInMinutes":500.0,"membershipTier":"ULTIMATE",
            "type":"SUBSCRIPTION","subType":"UNLIMITED","addons":addons,
            "features":{"resolutions":[{"isEntitled":true,"widthInPixels":1920,"heightInPixels":1080,"framesPerSecond":60}]},
            "currentSubscriptionState":{"state":"ACTIVE","isGamePlayAllowed":true},
            "notifications":{"notifyUserWhenTimeRemainingInMinutes":15,"notifyUserOnSessionWhenRemainingTimeInMinutes":5},
            "firstEntitlementStartDateTime":"2026-01-01T00:00:00Z",
            "currentSpanStartDateTime":"2026-01-01T00:00:00Z",
            "currentSpanEndDateTime":"2026-02-01T00:00:00Z"}),
    )
}

fn store_definitions() -> Vec<(u16, Value)> {
    vec![
        (
            200,
            json!({"data":{"appStoreDefinitions":[{"store":"STEAM","label":"Steam","features":[],
                "accountLinkingMetadata":{"isRequired":false,"isSupported":true,"supportedVariantIds":[]}}]}}),
        ),
        (200, json!({"data":{"genreDefinitions":[]}})),
        (200, json!({"data":{"subscriptionDefinitions":[]}})),
        (
            200,
            json!({"data":{"userAccount":{"storesData":[{"store":"STEAM","isConnected":true,"status":"connected"}],"subscriptions":[]}}}),
        ),
    ]
}

fn detail_game(app_id: &str, variant_id: &str, playability: &str) -> (u16, Value) {
    (
        200,
        json!({"data":{"apps":{"items":[{"id":app_id,"title":"Steam",
            "gfn":{"playabilityState":playability},"variants":[{"id":variant_id,"appStore":"STEAM",
                "gfn":{"status":"AVAILABLE","library":{"status":"MANUAL","selected":true,"playStatus":"PLAYABLE"}}}]}]}}}),
    )
}

fn no_game() -> (u16, Value) {
    (200, json!({"data":{"apps":{"items":[]}}}))
}

#[test]
fn store_launch_discovers_the_eligible_steam_client_and_carries_the_exact_target() {
    let (url, worker) = mock_requests(
        vec![
            vpc(),
            store_apps(vec![
                platform_client(
                    "platform-steam",
                    "Steam",
                    "STEAM",
                    "12345",
                    json!("AVAILABLE"),
                ),
                platform_client("platform-epic", "Epic", "EPIC", "999", json!("AVAILABLE")),
            ]),
            subscription(true),
            store_definitions()[0].clone(),
            store_definitions()[1].clone(),
            store_definitions()[2].clone(),
            store_definitions()[3].clone(),
            detail_game("platform-steam", "12345", "PLAYABLE"),
        ],
        |index, request| match index {
            0 => assert!(request.starts_with("GET /vpc ")),
            1 => {
                let payload: Value =
                    serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
                let query = payload["query"].as_str().unwrap();
                assert!(query.contains("PLATFORM_CLIENT"));
                assert!(query.contains("appStore"));
                assert!(query.contains("HERO_IMAGE"));
                assert!(query.contains("filters"));
                assert!(!query.contains("appIds"));
            }
            2 => assert!(request.starts_with("GET /subscription")),
            7 => {
                let payload: Value =
                    serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
                assert_eq!(payload["variables"]["ids"], json!(["platform-steam"]));
            }
            _ => {}
        },
    );
    let (service, path) = service(&url);
    let result = service
        .store_launch_inspect(&json!({}), None, &json!({}))
        .unwrap();
    assert_eq!(result["store"], "STEAM");
    assert_eq!(result["appId"], "platform-steam");
    assert_eq!(result["variantId"], "12345");
    assert_eq!(result["game"]["id"], "platform-steam");
    assert_eq!(result["game"]["title"], "Steam");
    assert_eq!(result["decision"]["status"], "ready");
    assert_eq!(result["catalogRevision"], 0);
    assert_eq!(result["freshness"], "fresh");
    assert_eq!(result["scope"]["userId"], "fixture-account");
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn store_launch_requires_the_persistent_storage_addon() {
    let (url, worker) = mock_requests(
        vec![
            vpc(),
            store_apps(vec![platform_client(
                "platform-steam",
                "Steam",
                "STEAM",
                "12345",
                json!("AVAILABLE"),
            )]),
            subscription(false),
            store_definitions()[0].clone(),
            store_definitions()[1].clone(),
            store_definitions()[2].clone(),
            store_definitions()[3].clone(),
            detail_game("platform-steam", "12345", "PLAYABLE"),
        ],
        |_, _| {},
    );
    let (service, path) = service(&url);
    let result = service
        .store_launch_inspect(&json!({}), None, &json!({}))
        .unwrap();
    assert_eq!(result["decision"]["status"], "subscription_required");
    assert_eq!(result["appId"], "platform-steam");
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn store_launch_ephemeral_only_storage_does_not_authorize_the_store_launch() {
    let (url, worker) = mock_requests(
        vec![
            vpc(),
            store_apps(vec![platform_client(
                "platform-steam",
                "Steam",
                "STEAM",
                "12345",
                json!("AVAILABLE"),
            )]),
            (
                200,
                json!({"allottedTimeInMinutes":600.0,"membershipTier":"ULTIMATE","type":"SUBSCRIPTION",
                    "subType":"UNLIMITED","addons":[{"type":"STORAGE","subType":"EPHEMERAL_STORAGE","status":"OK"}],
                    "features":{"resolutions":[]},"currentSubscriptionState":{"state":"ACTIVE","isGamePlayAllowed":true},
                    "notifications":{}}),
            ),
            store_definitions()[0].clone(),
            store_definitions()[1].clone(),
            store_definitions()[2].clone(),
            store_definitions()[3].clone(),
            detail_game("platform-steam", "12345", "PLAYABLE"),
        ],
        |_, _| {},
    );
    let (service, path) = service(&url);
    let result = service
        .store_launch_inspect(&json!({}), None, &json!({}))
        .unwrap();
    assert_eq!(result["decision"]["status"], "subscription_required");
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn store_launch_reports_unavailable_without_an_eligible_steam_variant() {
    let (url, worker) = mock_requests(
        vec![
            vpc(),
            store_apps(vec![platform_client(
                "platform-epic",
                "Epic",
                "EPIC",
                "999",
                json!("AVAILABLE"),
            )]),
            subscription(true),
        ],
        |index, _| assert!(index < 3),
    );
    let (service, path) = service(&url);
    let result = service
        .store_launch_inspect(&json!({}), None, &json!({}))
        .unwrap();
    assert_eq!(result["decision"]["status"], "unavailable");
    assert!(result["appId"].is_null());
    assert!(result["variantId"].is_null());
    assert!(result["game"].is_null());
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn store_launch_reports_unavailable_without_platform_clients() {
    let (url, worker) = mock_requests(
        vec![vpc(), store_apps(vec![]), subscription(true)],
        |_, _| {},
    );
    let (service, path) = service(&url);
    let result = service
        .store_launch_inspect(&json!({}), None, &json!({}))
        .unwrap();
    assert_eq!(result["decision"]["status"], "unavailable");
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn store_launch_rejects_unsupported_stores_and_non_boolean_intent() {
    let (url, worker) = mock_requests(vec![], |_, _| {});
    let (service, path) = service(&url);
    assert_eq!(
        service
            .store_launch_inspect(&json!({"store":"EPIC"}), None, &json!({}))
            .unwrap_err()
            .code,
        "invalid_params"
    );
    assert_eq!(
        service
            .store_launch_inspect(&json!({"store":7}), None, &json!({}))
            .unwrap_err()
            .code,
        "invalid_params"
    );
    assert_eq!(
        store_launch::store_launch_intent(&json!({"storeLaunch":"yes"}))
            .unwrap_err()
            .code,
        "invalid_params"
    );
    assert!(!store_launch::store_launch_intent(&json!({})).unwrap());
    assert!(!store_launch::store_launch_intent(&json!({"storeLaunch":false})).unwrap());
    assert!(store_launch::store_launch_intent(&json!({"storeLaunch":true})).unwrap());
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn store_launch_enforces_variant_readiness_from_the_discovery_source() {
    let (url, worker) = mock_requests(
        vec![
            vpc(),
            store_apps(vec![platform_client(
                "platform-steam",
                "Steam",
                "STEAM",
                "12345",
                json!("PATCHING"),
            )]),
            subscription(true),
            store_definitions()[0].clone(),
            store_definitions()[1].clone(),
            store_definitions()[2].clone(),
            store_definitions()[3].clone(),
            no_game(),
        ],
        |_, _| {},
    );
    let (service, path) = service(&url);
    let result = service
        .store_launch_inspect(&json!({}), None, &json!({}))
        .unwrap();
    assert_eq!(result["decision"]["status"], "patching");
    assert_eq!(result["variantId"], "12345");
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn store_launch_reports_unconfirmed_readiness_when_the_status_is_missing() {
    let (url, worker) = mock_requests(
        vec![
            vpc(),
            store_apps(vec![
                json!({"id":"platform-steam","title":"Steam","images":{},
                "variants":[{"appStore":"STEAM","id":"12345","gfn":{}}],"gfn":{"playType":"PLATFORM_CLIENT"}}),
            ]),
            subscription(true),
            store_definitions()[0].clone(),
            store_definitions()[1].clone(),
            store_definitions()[2].clone(),
            store_definitions()[3].clone(),
            no_game(),
        ],
        |_, _| {},
    );
    let (service, path) = service(&url);
    let result = service
        .store_launch_inspect(&json!({}), None, &json!({}))
        .unwrap();
    assert_eq!(result["decision"]["status"], "metadata_unconfirmed");
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn store_launch_falls_back_to_discovery_metadata_when_the_exact_game_is_not_browsable() {
    let (url, worker) = mock_requests(
        vec![
            vpc(),
            store_apps(vec![platform_client(
                "platform-steam",
                "Steam",
                "STEAM",
                "12345",
                json!("AVAILABLE"),
            )]),
            subscription(true),
            store_definitions()[0].clone(),
            store_definitions()[1].clone(),
            store_definitions()[2].clone(),
            store_definitions()[3].clone(),
            no_game(),
        ],
        |_, _| {},
    );
    let (service, path) = service(&url);
    let result = service
        .store_launch_inspect(&json!({}), None, &json!({}))
        .unwrap();
    assert_eq!(result["decision"]["status"], "ready");
    assert_eq!(result["appId"], "platform-steam");
    assert_eq!(result["variantId"], "12345");
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn store_launch_blocks_explicit_unplayability_from_the_exact_resolution() {
    let (url, worker) = mock_requests(
        vec![
            vpc(),
            store_apps(vec![platform_client(
                "platform-steam",
                "Steam",
                "STEAM",
                "12345",
                json!("AVAILABLE"),
            )]),
            subscription(true),
            store_definitions()[0].clone(),
            store_definitions()[1].clone(),
            store_definitions()[2].clone(),
            store_definitions()[3].clone(),
            detail_game("platform-steam", "12345", "UNPLAYABLE_DUE_TO_UPGRADE"),
        ],
        |_, _| {},
    );
    let (service, path) = service(&url);
    let result = service
        .store_launch_inspect(&json!({}), None, &json!({}))
        .unwrap();
    assert_eq!(result["decision"]["status"], "subscription_required");
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn store_launch_requires_a_linked_store_account_when_the_store_requires_it() {
    let (url, worker) = mock_requests(
        vec![
            vpc(),
            store_apps(vec![platform_client(
                "platform-steam",
                "Steam",
                "STEAM",
                "12345",
                json!("AVAILABLE"),
            )]),
            subscription(true),
            (
                200,
                json!({"data":{"appStoreDefinitions":[{"store":"STEAM","label":"Steam","features":[],
                    "accountLinkingMetadata":{"isRequired":true,"isSupported":true,"supportedVariantIds":["12345"]}}]}}),
            ),
            store_definitions()[1].clone(),
            store_definitions()[2].clone(),
            (
                200,
                json!({"data":{"userAccount":{"storesData":[{"store":"STEAM","isConnected":false,"status":"disconnected"}],"subscriptions":[]}}}),
            ),
            detail_game("platform-steam", "12345", "PLAYABLE"),
        ],
        |_, _| {},
    );
    let (service, path) = service(&url);
    let result = service
        .store_launch_inspect(&json!({}), None, &json!({}))
        .unwrap();
    assert_eq!(result["decision"]["status"], "link_required");
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn targeted_store_launch_requires_the_exact_platform_client_and_variant() {
    let (url, worker) = mock_requests(
        vec![
            vpc(),
            store_apps(vec![platform_client(
                "platform-steam",
                "Steam",
                "STEAM",
                "12345",
                json!("AVAILABLE"),
            )]),
            subscription(true),
        ],
        |_, _| {},
    );
    let (service, path) = service(&url);
    let result = service
        .store_launch_inspect(&json!({}), Some(("platform-epic", "999")), &json!({}))
        .unwrap();
    assert_eq!(result["decision"]["status"], "metadata_unconfirmed");
    assert_eq!(result["appId"], "platform-epic");
    assert_eq!(result["variantId"], "999");
    assert!(result["game"].is_null());
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn store_launch_requires_an_identifier_and_the_supported_store_for_the_exact_variant() {
    let (url, worker) = mock_requests(
        vec![
            vpc(),
            store_apps(vec![
                json!({"id":"platform-steam","title":"Steam","images":{},
                "variants":[{"appStore":"STEAM","id":"not-numeric","gfn":{"status":"AVAILABLE"}}],
                "gfn":{"playType":"PLATFORM_CLIENT"}}),
            ]),
            subscription(true),
            store_definitions()[0].clone(),
            store_definitions()[1].clone(),
            store_definitions()[2].clone(),
            store_definitions()[3].clone(),
            no_game(),
        ],
        |_, _| {},
    );
    let (service, path) = service(&url);
    let result = service
        .store_launch_inspect(&json!({}), None, &json!({}))
        .unwrap();
    assert_eq!(result["decision"]["status"], "metadata_unconfirmed");
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn store_launch_rejects_a_changed_membership_context() {
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let (url, worker) = mock_requests(
        vec![
            vpc(),
            store_apps(vec![platform_client(
                "platform-steam",
                "Steam",
                "STEAM",
                "12345",
                json!("AVAILABLE"),
            )]),
            subscription(true),
        ],
        move |index, _| {
            if index != 2 {
                return;
            }
            entered_tx.send(()).unwrap();
            release_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
        },
    );
    let (service, path) = service(&url);
    std::thread::scope(|threads| {
        let pending = threads.spawn(|| service.store_launch_inspect(&json!({}), None, &json!({})));
        entered_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        service.state.lock().unwrap().generation += 1;
        release_tx.send(()).unwrap();
        assert_eq!(pending.join().unwrap().unwrap_err().code, "stale_account");
    });
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn store_launch_never_borrows_a_registered_launch_target() {
    let game = json!({"id":"platform-steam","title":"Steam",
        "variants":[{"id":"12345","store":"STEAM","gfnStatus":"AVAILABLE"}]});
    let subscription =
        json!({"storageAddon":{"type":"PERMANENT_STORAGE"},"isGamePlayAllowed":true});
    let access = json!({"definitions":{"stores":{"status":"success"}},
        "accounts":[{"provider":"STEAM","isRequired":false,"supportsLinking":true,"isConnected":true}],
        "subscriptions":[]});
    assert_eq!(
        store_launch::store_launch_decision(
            &game,
            "platform-steam",
            "999",
            "STEAM",
            &subscription,
            &access,
            false,
        )
        .status,
        LaunchStatus::MetadataUnconfirmed
    );
    assert_eq!(
        store_launch::store_launch_decision(
            &game,
            "other-app",
            "12345",
            "STEAM",
            &subscription,
            &access,
            false,
        )
        .status,
        LaunchStatus::MetadataUnconfirmed
    );
    let epic_variant = json!({"id":"999","store":"EPIC","gfnStatus":"AVAILABLE"});
    let games = vec![
        json!({"id":"platform-epic","variants":[epic_variant.clone()]}),
        json!({"id":"platform-steam","variants":game["variants"].clone()}),
    ];
    let (owner, variant) =
        store_launch::eligible_store_target(&games, "STEAM").expect("steam client present");
    assert_eq!(owner["id"], "platform-steam");
    assert_eq!(variant["id"], "12345");
    assert!(store_launch::eligible_store_target(&games, "XBOX").is_none());
    assert_eq!(
        store_launch::store_launch_decision(
            &game,
            "platform-steam",
            "12345",
            "EPIC",
            &subscription,
            &access,
            false,
        )
        .status,
        LaunchStatus::MetadataUnconfirmed
    );
    assert_eq!(
        store_launch::store_launch_decision(
            &game,
            "platform-steam",
            "12345",
            "STEAM",
            &subscription,
            &access,
            false,
        )
        .status,
        LaunchStatus::Ready
    );
}

#[test]
fn store_launch_preserves_explicit_gameplay_denial_without_a_membership_tier() {
    let (_, mut subscription) = subscription(true);
    subscription["currentSubscriptionState"]["isGamePlayAllowed"] = json!(false);
    let (url, worker) = mock_requests(
        vec![
            vpc(),
            store_apps(vec![platform_client(
                "platform-steam",
                "Steam",
                "STEAM",
                "12345",
                json!("AVAILABLE"),
            )]),
            (200, subscription),
            store_definitions()[0].clone(),
            store_definitions()[1].clone(),
            store_definitions()[2].clone(),
            store_definitions()[3].clone(),
            no_game(),
        ],
        |_, _| {},
    );
    let (service, path) = service(&url);
    let result = service
        .store_launch_inspect(&json!({}), None, &json!({}))
        .unwrap();
    assert_eq!(result["decision"]["status"], "subscription_required");
    assert_eq!(result["appId"], "platform-steam");
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn store_launch_propagates_detail_failures_that_are_not_missing_games() {
    let (url, worker) = mock_requests(
        vec![
            vpc(),
            store_apps(vec![platform_client(
                "platform-steam",
                "Steam",
                "STEAM",
                "12345",
                json!("AVAILABLE"),
            )]),
            subscription(true),
            store_definitions()[0].clone(),
            store_definitions()[1].clone(),
            store_definitions()[2].clone(),
            store_definitions()[3].clone(),
            (503, json!({})),
        ],
        |_, _| {},
    );
    let (service, path) = service(&url);
    let result = service.store_launch_inspect(&json!({}), None, &json!({}));
    assert_eq!(result.unwrap_err().code, "upstream_error");
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn store_launch_preserves_the_active_seat_guard() {
    let (url, worker) = mock_requests(
        vec![
            vpc(),
            store_apps(vec![platform_client(
                "platform-steam",
                "Steam",
                "STEAM",
                "12345",
                json!("AVAILABLE"),
            )]),
            subscription(true),
            store_definitions()[0].clone(),
            store_definitions()[1].clone(),
            store_definitions()[2].clone(),
            store_definitions()[3].clone(),
            detail_game("platform-steam", "12345", "PLAYABLE"),
        ],
        |_, _| {},
    );
    let (service, path) = service(&url);
    service
        .cloudmatch
        .seed_owned_session(json!({"sessionId":"owned-seat","status":3}));
    let result = service.create_session(
        &json!({"appId":"12345","variantId":"12345","catalogAppId":"platform-steam",
            "storeLaunch":true,
            "scope":scoped_result(json!({}), &auth_fixture("fixture-account"), 0)["scope"]}),
        &json!({}),
    );
    assert_eq!(result.unwrap_err().code, "session_update_busy");
    assert_eq!(
        service.cloudmatch.active()["session"]["sessionId"],
        "owned-seat"
    );
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}
