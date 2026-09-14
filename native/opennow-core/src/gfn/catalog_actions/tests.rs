use super::*;
use crate::gfn::tests::{auth_fixture, jwt, mock_requests, test_service};

fn service(url: &str) -> (GfnService, PathBuf) {
    let (mut service, path) = test_service(url);
    service.endpoints.graphql = format!("{url}/graphql");
    service.endpoints.server_info = Some(format!("{url}/vpc"));
    let mut state = service.state.lock().unwrap();
    state.session = Some(auth_fixture("fixture-account"));
    state.restore_attempted = true;
    drop(state);
    (service, path)
}

fn app(status: &str, selected: bool, favorite: bool) -> Value {
    json!({"id":"parent-app","title":"Fixture game","library":{"favorited":favorite},
    "gfn":{"playabilityState":"PLAYABLE"},"variants":[
        {"id":"123","appStore":"STEAM","gfn":{"status":"AVAILABLE","library":{"status":status,"selected":selected,"playStatus":"PLAYABLE"}}},
        {"id":"456","appStore":"EPIC","gfn":{"status":"AVAILABLE","library":{"status":"NOT_OWNED","selected":false,"playStatus":"UNKNOWN"}}}
    ]})
}

fn read(app: Value) -> (u16, Value) {
    (200, json!({"data":{"apps":{"items":[app]}}}))
}

fn vpc() -> (u16, Value) {
    (200, json!({"requestStatus":{"serverId":"fixture-vpc"}}))
}

fn scoped(mut params: Value) -> Value {
    params["scope"] =
        scoped_result(json!({}), &auth_fixture("fixture-account"), 0)["scope"].clone();
    params
}

fn access() -> Value {
    json!({"definitions":{"stores":{"status":"success"}},"accounts":[{"provider":"STEAM","isRequired":false,"supportsLinking":true,"isConnected":false}],"subscriptions":[]})
}

fn decision(game: &Value, app_id: &str, variant_id: &str, subscription: &Value) -> LaunchDecision {
    super::launch_decision(game, app_id, variant_id, subscription, &access())
}

#[test]
fn selected_store_link_and_subscription_requirements_use_current_account_metadata() {
    let mut game = app_to_game(&app("MANUAL", true, false)).unwrap();
    let mut account = access();
    account["accounts"][0]["isRequired"] = json!(true);
    assert_eq!(
        super::launch_decision(&game, "parent-app", "123", &Value::Null, &account).status,
        LaunchStatus::LinkRequired
    );
    account["accounts"][0]["isConnected"] = json!(true);
    assert_eq!(
        super::launch_decision(&game, "parent-app", "123", &Value::Null, &account).status,
        LaunchStatus::Ready
    );
    account["accounts"][0]["status"] = json!("expired");
    assert_eq!(
        super::launch_decision(&game, "parent-app", "123", &Value::Null, &account).status,
        LaunchStatus::LinkRequired
    );
    account["accounts"][0]["status"] = json!("connected");
    game["variants"][0]["subscription"] = json!("STORE_PASS");
    assert_eq!(
        super::launch_decision(&game, "parent-app", "123", &Value::Null, &account).status,
        LaunchStatus::SubscriptionRequired
    );
    account["subscriptions"] = json!([{"id":"STORE_PASS"}]);
    assert_eq!(
        super::launch_decision(&game, "parent-app", "123", &Value::Null, &account).status,
        LaunchStatus::Ready
    );
    account["definitions"]["stores"]["status"] = json!("stale");
    assert_eq!(
        super::launch_decision(&game, "parent-app", "123", &Value::Null, &account).status,
        LaunchStatus::MetadataUnconfirmed
    );
}

#[test]
fn nullable_variant_scope_preserves_required_account_linking() {
    let game = app_to_game(&app("PLATFORM_SYNC", true, false)).unwrap();
    for required in [false, true] {
        let definition: crate::catalog_types::StoreDefinition = serde_json::from_value(json!({
            "store":"STEAM","label":"Steam","features":[],
            "accountLinkingMetadata":{"supportedVariantIds":null,"isRequired":required}
        }))
        .unwrap();
        let mut account = access();
        account["accounts"][0] = definition.connection_definition();
        assert_eq!(
            super::launch_decision(&game, "parent-app", "123", &Value::Null, &account).status,
            if required {
                LaunchStatus::LinkRequired
            } else {
                LaunchStatus::Ready
            }
        );
    }
}

#[test]
fn no_subscription_sentinel_is_normalized_without_bypassing_real_subscriptions() {
    for subscription in [
        Value::Null,
        json!("NONE"),
        json!("STORE_PASS"),
        json!("UNKNOWN_PASS"),
    ] {
        let mut metadata = app("PLATFORM_SYNC", true, false);
        metadata["variants"][0]["gfn"]["library"]["subscription"] = subscription.clone();
        let game = app_to_game(&metadata).unwrap();
        let requires_subscription = subscription.is_string() && subscription != "NONE";
        assert_eq!(
            game["variants"][0]["subscription"],
            if requires_subscription {
                subscription
            } else {
                Value::Null
            }
        );
        assert_eq!(
            decision(&game, "parent-app", "123", &Value::Null).status,
            if requires_subscription {
                LaunchStatus::SubscriptionRequired
            } else {
                LaunchStatus::Ready
            }
        );
    }
}

#[test]
fn exact_selected_variant_policy_never_infers_ownership_from_aggregate_or_labels() {
    let mut game = app_to_game(&app("MANUAL", true, true)).unwrap();
    for key in ["isInLibrary", "accountLinked", "favorited"] {
        game[key] = json!(true);
    }
    game["paymentModels"] = json!([{"__typename":"FreeToPlayPaymentModel"}]);
    game["variants"][1]["subscription"] = json!("GAME_PASS");
    assert_eq!(
        decision(&game, "parent-app", "123", &Value::Null).status,
        LaunchStatus::Ready
    );
    assert_eq!(
        decision(&game, "parent-app", "456", &Value::Null).status,
        LaunchStatus::OwnershipRequired
    );
    for id in ["789", "0", "2147483648", "parent-app"] {
        assert_eq!(
            decision(&game, "parent-app", id, &Value::Null).status,
            LaunchStatus::MetadataUnconfirmed
        );
    }
    assert_eq!(
        decision(&game, "other-app", "123", &Value::Null).status,
        LaunchStatus::MetadataUnconfirmed
    );
}

#[test]
fn library_status_and_missing_readiness_are_explicit() {
    for (status, expected) in [
        ("MANUAL", LaunchStatus::Ready),
        ("PLATFORM_SYNC", LaunchStatus::Ready),
        ("NOT_OWNED", LaunchStatus::OwnershipRequired),
        ("IN_LIBRARY", LaunchStatus::MetadataUnconfirmed),
        ("UNKNOWN", LaunchStatus::MetadataUnconfirmed),
    ] {
        let mut game = app_to_game(&app(status, true, false)).unwrap();
        for play in [Value::Null, json!("UNKNOWN"), json!("PLAYABLE")] {
            game["variants"][0]["playStatus"] = play;
            assert_eq!(
                decision(&game, "parent-app", "123", &Value::Null).status,
                expected
            );
        }
        game["variants"][0]["playStatus"] = json!("NOT_PLAYABLE");
        assert_eq!(
            decision(&game, "parent-app", "123", &Value::Null).status,
            LaunchStatus::Unavailable
        );
    }
    for key in ["libraryStatus", "gfnStatus", "librarySelected"] {
        let mut game = app_to_game(&app("MANUAL", true, false)).unwrap();
        game["variants"][0][key] = Value::Null;
        assert_eq!(
            decision(&game, "parent-app", "123", &Value::Null).status,
            LaunchStatus::MetadataUnconfirmed,
            "{key}"
        );
    }
    let game = app_to_game(&app("MANUAL", false, false)).unwrap();
    assert_eq!(
        decision(&game, "parent-app", "123", &Value::Null).status,
        LaunchStatus::SelectionRequired
    );
}

#[test]
fn patch_maintenance_membership_and_missing_app_metadata_block_launch() {
    let baseline = app_to_game(&app("MANUAL", true, false)).unwrap();
    for (status, expected) in [
        ("PATCHING", LaunchStatus::Patching),
        ("SERVER_MAINTENANCE", LaunchStatus::Maintenance),
        ("UNAVAILABLE", LaunchStatus::Unavailable),
        ("UNKNOWN", LaunchStatus::MetadataUnconfirmed),
    ] {
        let mut game = baseline.clone();
        game["variants"][0]["gfnStatus"] = json!(status);
        assert_eq!(
            decision(&game, "parent-app", "123", &Value::Null).status,
            expected
        );
    }
    for (state, expected) in [
        (Value::Null, LaunchStatus::MetadataUnconfirmed),
        (json!("UNKNOWN"), LaunchStatus::MetadataUnconfirmed),
        (
            json!("UNPLAYABLE_DUE_TO_UPGRADE"),
            LaunchStatus::SubscriptionRequired,
        ),
        (
            json!("UNPLAYABLE_DUE_TO_TIME_CAPPED_LIMIT"),
            LaunchStatus::SubscriptionRequired,
        ),
    ] {
        let mut game = baseline.clone();
        game["playabilityState"] = state;
        assert_eq!(
            decision(&game, "parent-app", "123", &Value::Null).status,
            expected
        );
    }
    let mut game = baseline;
    game["membershipTierLabel"] = json!("Performance");
    for (subscription, expected) in [
        (Value::Null, LaunchStatus::MetadataUnconfirmed),
        (
            json!({"membershipTier":"FREE"}),
            LaunchStatus::SubscriptionRequired,
        ),
        (
            json!({"membershipTier":"ULTIMATE","isGamePlayAllowed":true}),
            LaunchStatus::Ready,
        ),
        (
            json!({"membershipTier":"ULTIMATE","isGamePlayAllowed":false}),
            LaunchStatus::SubscriptionRequired,
        ),
    ] {
        assert_eq!(
            decision(&game, "parent-app", "123", &subscription).status,
            expected
        );
    }
}

#[test]
fn every_mutation_uses_exact_wire_identity_and_fresh_reconciliation() {
    for (method, mutation, desired) in [
        (
            "catalog.favorites.add",
            Mutation::AddFavorite,
            app("MANUAL", true, true),
        ),
        (
            "catalog.favorites.remove",
            Mutation::RemoveFavorite,
            app("MANUAL", true, false),
        ),
        (
            "catalog.ownership.add",
            Mutation::AddOwned,
            app("MANUAL", true, false),
        ),
        (
            "catalog.ownership.remove",
            Mutation::RemoveOwned,
            app("NOT_OWNED", false, false),
        ),
        (
            "catalog.ownership.select",
            Mutation::SelectOwned,
            app("MANUAL", true, false),
        ),
    ] {
        let root = mutation.root();
        let (url, worker) = mock_requests(
            vec![
                vpc(),
                read(app("MANUAL", false, false)),
                (200, json!({"data":{root:{"app":{"id":"parent-app"}}}})),
                read(desired),
            ],
            move |index, request| {
                if index != 2 {
                    return;
                }
                let body: Value =
                    serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
                let query = body["query"].as_str().unwrap();
                assert!(query.starts_with("mutation "));
                assert!(query.contains(&format!("{root}(language: $locale,")));
                assert_eq!(body["variables"]["locale"], "en_US");
                assert_eq!(
                    body["variables"][if mutation.favorite() {
                        "appId"
                    } else {
                        "cmsId"
                    }],
                    if mutation.favorite() {
                        "parent-app"
                    } else {
                        "123"
                    }
                );
                assert_eq!(body["variables"].as_object().unwrap().len(), 2);
            },
        );
        let (service, path) = service(&url);
        let result = service
            .catalog_mutate(
                method,
                &scoped(
                    json!({"appId":"parent-app","variantId":"123","confirmedExistingLicense":true}),
                ),
                &json!({}),
            )
            .unwrap();
        assert_eq!(result["outcome"], "acknowledged");
        assert_eq!(result["reconciliation"], "confirmed");
        assert_eq!(result["catalogRevision"], 2);
        worker.join().unwrap();
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[test]
fn ambiguous_or_invalid_acknowledgements_are_never_replayed_or_mislabeled() {
    for (status, payload) in [
        (401, json!({})),
        (404, json!({})),
        (503, json!({})),
        (
            200,
            json!({"data":{"addFavoriteApp":{"app":{"id":"other-app"}}}}),
        ),
        (200, json!({"data":{"addFavoriteApp":null}})),
        (
            200,
            json!({"data":{"addFavoriteApp":{"app":{"id":"parent-app"}}},"errors":{}}),
        ),
        (
            200,
            json!({"errors":[{"message":"not public","extensions":{"code":"DENIED"},"path":["addFavoriteApp"]}]}),
        ),
    ] {
        let (url, worker) = mock_requests(
            vec![
                vpc(),
                read(app("MANUAL", true, false)),
                (status, payload),
                read(app("MANUAL", true, true)),
            ],
            |_, _| {},
        );
        let (service, path) = service(&url);
        let result = service
            .catalog_mutate(
                "catalog.favorites.add",
                &scoped(json!({"appId":"parent-app"})),
                &json!({}),
            )
            .unwrap();
        assert_eq!(result["outcome"], "unconfirmed");
        assert_eq!(result["reconciliation"], "confirmed");
        assert_eq!(result["error"]["httpStatus"], status);
        assert!(result["error"]["code"].is_string());
        assert!(!result.to_string().contains("not public"));
        worker.join().unwrap();
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[test]
fn acknowledgement_without_convergence_or_failed_read_remains_unconfirmed() {
    for last in [read(app("MANUAL", true, false)), (503, json!({}))] {
        let (url, worker) = mock_requests(
            vec![
                vpc(),
                read(app("MANUAL", true, false)),
                (
                    200,
                    json!({"data":{"addFavoriteApp":{"app":{"id":"parent-app"}}}}),
                ),
                last,
            ],
            |_, _| {},
        );
        let (service, path) = service(&url);
        let result = service
            .catalog_mutate(
                "catalog.favorites.add",
                &scoped(json!({"appId":"parent-app"})),
                &json!({}),
            )
            .unwrap();
        assert_eq!(result["outcome"], "acknowledged");
        assert_eq!(result["reconciliation"], "unconfirmed");
        worker.join().unwrap();
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[test]
fn ownership_requires_explicit_confirmation_and_parent_variant_match() {
    let (url, worker) = mock_requests(vec![vpc(), read(app("MANUAL", true, false))], |_, _| {});
    let (service, path) = service(&url);
    assert!(
        service
            .catalog_mutate(
                "catalog.ownership.add",
                &scoped(json!({"appId":"parent-app","variantId":"123"})),
                &json!({})
            )
            .is_err()
    );
    assert!(
        service
            .catalog_mutate(
                "catalog.ownership.add",
                &scoped(
                    json!({"appId":"parent-app","variantId":"999","confirmedExistingLicense":true})
                ),
                &json!({})
            )
            .is_err()
    );
    assert_eq!(
        service
            .catalog_revision
            .load(std::sync::atomic::Ordering::Acquire),
        0
    );
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn favorites_preserve_more_than_twenty_four_and_unknown_coverage() {
    let items: Vec<_> = (0..75)
        .map(|index| {
            let mut game = app("MANUAL", true, true);
            game["id"] = json!(format!("parent-{index}"));
            json!({"__typename":"GameItem","app":game})
        })
        .collect();
    let (url, worker) = mock_requests(
        vec![
            vpc(),
            (
                200,
                json!({"data":{"panels":[{"id":"favorites","name":"FAVORITES","sections":[{"id":"section","seeMoreInfo":{"filterIds":["favorites"]},"items":items}]}]}}),
            ),
        ],
        |index, request| {
            if index == 1 {
                let payload: Value =
                    serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
                assert_eq!(payload["variables"]["panelNames"], json!(["FAVORITES"]));
                assert!(payload["variables"].get("cursor").is_none());
                assert!(payload["query"].as_str().unwrap().contains("seeMoreInfo"));
            }
        },
    );
    let (service, path) = service(&url);
    let result = service.catalog_favorites(&json!({})).unwrap();
    assert_eq!(result["games"].as_array().unwrap().len(), 75);
    assert_eq!(result["coverage"], "unknown");
    assert_eq!(result["complete"], false);
    assert_eq!(
        result["sections"][0]["seeMoreInfo"]["filterIds"],
        json!(["favorites"])
    );
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn fresh_create_rejects_unowned_variant_before_allocation() {
    let (url, worker) = mock_requests(vec![vpc(), read(app("MANUAL", true, false))], |_, _| {});
    let (service, path) = service(&url);
    let result = service.create_session(
        &scoped(json!({"catalogAppId":"parent-app","variantId":"456","appId":"456"})),
        &json!({}),
    );
    assert_eq!(result.unwrap_err().code, "launch_not_ready");
    assert!(service.cloudmatch.active()["session"].is_null());
    assert!(service.cloudmatch.admit_create().is_ok());
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn create_rejects_mismatched_launch_identity_without_network() {
    let (service, path) = service("http://127.0.0.1:1");
    assert!(
        service
            .create_session(
                &json!({"catalogAppId":"parent-app","variantId":"456","appId":"123"}),
                &json!({})
            )
            .is_err()
    );
    assert!(service.cloudmatch.admit_create().is_ok());
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn old_account_mutations_are_rejected_before_network() {
    let (service, path) = service("http://127.0.0.1:1");
    let mut params = scoped(json!({"appId":"parent-app"}));
    params["scope"]["generation"] = json!(99);
    assert_eq!(
        service
            .catalog_mutate("catalog.favorites.add", &params, &json!({}))
            .unwrap_err()
            .code,
        "stale_account"
    );
    assert_eq!(
        service
            .catalog_revision
            .load(std::sync::atomic::Ordering::Acquire),
        0
    );
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn mutation_and_create_admission_serialize_only_the_same_account_app() {
    let (service, path) = service("http://127.0.0.1:1");
    let auth = auth_fixture("fixture-account");
    let permit = service.admit_catalog_action(&auth, "parent-app").unwrap();
    assert!(service.admit_catalog_action(&auth, "parent-app").is_err());
    assert!(service.admit_catalog_action(&auth, "different-app").is_ok());
    drop(permit);
    assert!(service.admit_catalog_action(&auth, "parent-app").is_ok());
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn account_change_during_mutation_cannot_reconcile_under_the_new_account() {
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let (url, worker) = mock_requests(
        vec![
            vpc(),
            read(app("MANUAL", true, false)),
            (
                200,
                json!({"data":{"addFavoriteApp":{"app":{"id":"parent-app"}}}}),
            ),
        ],
        move |index, _| {
            if index == 2 {
                entered_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            }
        },
    );
    let (service, path) = service(&url);
    std::thread::scope(|threads| {
        let mutation = threads.spawn(|| {
            service.catalog_mutate(
                "catalog.favorites.add",
                &scoped(json!({"appId":"parent-app"})),
                &json!({}),
            )
        });
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        service.state.lock().unwrap().generation += 1;
        release_tx.send(()).unwrap();
        assert_eq!(mutation.join().unwrap().unwrap_err().code, "stale_account");
    });
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

fn held_mutation_remains_admitted_after_generation_change(switch_accounts: bool) {
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let (url, worker) = mock_requests(
        vec![
            vpc(),
            read(app("MANUAL", true, false)),
            (
                200,
                json!({"data":{"addFavoriteApp":{"app":{"id":"parent-app"}}}}),
            ),
        ],
        move |index, request| {
            if index == 2 {
                assert!(request.contains("mutation AddFavoriteApp"));
                entered_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            }
        },
    );
    let (service, path) = service(&url);
    let owner = auth_fixture("fixture-account");
    std::thread::scope(|threads| {
        let mutation = threads.spawn(|| {
            service.catalog_mutate(
                "catalog.favorites.add",
                &scoped(json!({"appId":"parent-app"})),
                &json!({}),
            )
        });
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        if switch_accounts {
            let other = auth_fixture("other-account");
            {
                let mut state = service.state.lock().unwrap();
                state.session = Some(other.clone());
                state.generation += 1;
            }
            assert!(service.admit_catalog_action(&other, "parent-app").is_ok());
            let mut state = service.state.lock().unwrap();
            state.session = Some(owner.clone());
            state.generation += 1;
        } else {
            service.clear_cache();
            let mut state = service.state.lock().unwrap();
            state.providers = vec![owner.provider.clone()];
            state.providers_expires = Some(Instant::now() + Duration::from_secs(60));
        }
        let generation = service.auth_generation();
        let blocked = service
            .admit_catalog_action(&owner, "parent-app")
            .err()
            .map(|error| error.code);
        assert!(service.admit_catalog_action(&owner, "another-app").is_ok());
        assert!(
            service
                .admit_catalog_action(&auth_fixture("another-account"), "parent-app")
                .is_ok()
        );
        let mut partner = owner.clone();
        partner.provider.idp_id = "another-provider".into();
        assert!(service.admit_catalog_action(&partner, "parent-app").is_ok());
        if blocked.is_some() {
            let scope = scoped_result(json!({}), &owner, generation)["scope"].clone();
            assert_eq!(
                service
                    .catalog_mutate(
                        "catalog.favorites.remove",
                        &json!({"appId":"parent-app","scope":scope}),
                        &json!({})
                    )
                    .unwrap_err()
                    .code,
                "catalog_mutation_busy"
            );
            assert_eq!(service.create_session(&json!({"catalogAppId":"parent-app","variantId":"123","appId":"123","scope":scope}), &json!({})).unwrap_err().code, "catalog_mutation_busy");
        }
        release_tx.send(()).unwrap();
        assert_eq!(mutation.join().unwrap().unwrap_err().code, "stale_account");
        assert_eq!(blocked, Some("catalog_mutation_busy"));
        assert!(service.admit_catalog_action(&owner, "parent-app").is_ok());
    });
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn held_mutation_blocks_same_app_after_account_switch_back() {
    held_mutation_remains_admitted_after_generation_change(true);
}

#[test]
fn held_mutation_blocks_same_app_after_cache_generation_bump() {
    held_mutation_remains_admitted_after_generation_change(false);
}

#[test]
fn mutation_preflight_401_renews_before_exactly_one_send() {
    for mutation_status in [200, 401] {
        let old_id = jwt("fixture-account", now_ms() + 3_600_000);
        let renewed_id = jwt("fixture-account", now_ms() + 7_200_000);
        let requests = std::sync::Arc::new(Mutex::new(Vec::new()));
        let observed = requests.clone();
        let (url, worker) = mock_requests(
            vec![
                vpc(),
                (401, json!({})),
                (
                    200,
                    json!({"access_token":"renewed-access","id_token":renewed_id,"expires_in":7200}),
                ),
                vpc(),
                read(app("MANUAL", true, false)),
                (
                    mutation_status,
                    json!({"data":{"addFavoriteApp":{"app":{"id":"parent-app"}}}}),
                ),
                read(app("MANUAL", true, true)),
            ],
            move |_, request| observed.lock().unwrap().push(request.to_owned()),
        );
        let (service, path) = service(&url);
        {
            let mut state = service.state.lock().unwrap();
            let session = state.session.as_mut().unwrap();
            session.tokens.id_token = Some(old_id.clone());
            session.tokens.id_token_expires_at = Some(now_ms() + 3_600_000);
        }
        let result = service
            .catalog_mutate(
                "catalog.favorites.add",
                &scoped(json!({"appId":"parent-app"})),
                &json!({}),
            )
            .unwrap();
        worker.join().unwrap();
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 7);
        assert!(requests[0].contains(&format!("GFNJWT {old_id}")));
        assert!(requests[1].contains(&format!("GFNJWT {old_id}")));
        assert!(requests[2].starts_with("POST / "));
        let refresh: HashMap<_, _> =
            url::form_urlencoded::parse(requests[2].split("\r\n\r\n").nth(1).unwrap().as_bytes())
                .into_owned()
                .collect();
        assert_eq!(
            refresh["grant_type"],
            "urn:ietf:params:oauth:grant-type:client_token"
        );
        assert_eq!(refresh["client_id"], "test-client-id");
        assert!(requests[4].contains(&format!("GFNJWT {renewed_id}")));
        let sends: Vec<_> = requests
            .iter()
            .filter(|request| request.contains("mutation AddFavoriteApp"))
            .collect();
        assert_eq!(sends.len(), 1);
        assert!(
            sends[0].contains(&format!("GFNJWT {renewed_id}")),
            "mutation retained the preflight credential after renewal"
        );
        assert!(!sends[0].contains(&format!("GFNJWT {old_id}")));
        assert!(requests[6].contains(&format!("GFNJWT {renewed_id}")));
        assert_eq!(result["scope"]["generation"], 0);
        assert_eq!(
            result["outcome"],
            if mutation_status == 200 {
                "acknowledged"
            } else {
                "unconfirmed"
            }
        );
        assert_eq!(result["reconciliation"], "confirmed");
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[test]
fn membership_requirements_preserve_all_existing_tier_cases_in_the_shared_policy() {
    for (required, membership, expected) in [
        ("Premium", "FREE", LaunchStatus::SubscriptionRequired),
        (
            "Performance",
            " Free Tier ",
            LaunchStatus::SubscriptionRequired,
        ),
        ("Ultimate", "free-tier", LaunchStatus::SubscriptionRequired),
        ("Premium", "PERFORMANCE", LaunchStatus::Ready),
        ("Premium", "Priority", LaunchStatus::Ready),
        ("", "FREE", LaunchStatus::Ready),
        ("  ", "", LaunchStatus::Ready),
        (" FREE ", "FREE", LaunchStatus::Ready),
        ("Free Tier", "", LaunchStatus::Ready),
        ("free-tier", "", LaunchStatus::Ready),
        ("Premium", "", LaunchStatus::MetadataUnconfirmed),
        ("Premium", " ", LaunchStatus::MetadataUnconfirmed),
    ] {
        let mut game = app_to_game(&app("MANUAL", true, false)).unwrap();
        game["membershipTierLabel"] = json!(required);
        assert_eq!(
            decision(
                &game,
                "parent-app",
                "123",
                &json!({"membershipTier":membership})
            )
            .status,
            expected,
            "{required}/{membership}"
        );
    }
}
