use super::tests::{auth_fixture, mock_requests, test_service};
use super::*;

fn service(url: &str) -> (GfnService, PathBuf) {
    let (mut service, path) = test_service(url);
    service.endpoints.graphql = format!("{url}/graphql");
    service.endpoints.public_graphql = format!("{url}/public");
    service.endpoints.server_info = Some(format!("{url}/vpc"));
    let mut state = service.state.lock().unwrap();
    state.session = Some(auth_fixture("fixture-account"));
    state.restore_attempted = true;
    drop(state);
    (service, path)
}

fn app(id: usize) -> Value {
    json!({"id":format!("app-{id}"),"title":format!("Game {id}"),"library":{"favorited":true},
        "gfn":{"playabilityState":"PLAYABLE","catalogSkuStrings":{"SKU_BASED_TAG":["Fixture"]}},
        "variants":[{"id":"123","appStore":"STEAM","paymentModels":[{"__typename":"Paid"}],
            "gfn":{"status":"PATCHING","library":{"status":"MANUAL","selected":true,"playStatus":"NOT_PLAYABLE"},
            "stateDetails":{"__typename":"VariantGfnAutoPatchingMetadata","historicalEtaMins":12.5}}}]})
}

#[test]
fn library_traverses_thirteen_complete_pages_without_a_thousand_game_ceiling() {
    let mut responses = vec![(200, json!({"requestStatus":{"serverId":"fixture-vpc"}}))];
    for page in 0..13 {
        responses.push((200,json!({"data":{"apps":{"items":(page*100..page*100+100).map(app).collect::<Vec<_>>(),
            "pageInfo":{"hasNextPage":page < 12,"endCursor":format!("cursor-{}",page+1),"totalCount":1300}}}})));
    }
    let (url, worker) = mock_requests(responses, |index, request| {
        if index == 0 {
            assert!(request.starts_with("GET /vpc "));
            return;
        }
        let payload: Value =
            serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(payload["variables"]["fetchCount"], 100);
        assert_eq!(
            payload["variables"]["cursor"],
            if index == 1 {
                String::new()
            } else {
                format!("cursor-{}", index - 1)
            }
        );
        assert!(payload["query"].as_str().unwrap().contains("playStatus"));
    });
    let (service, path) = service(&url);
    let mut cursor = String::new();
    let mut context = Value::Null;
    let mut count = 0;
    for _ in 0..13 {
        let result = service
            .library_catalog(
                &json!({"limit":100,"cursor":cursor,"catalogRevision":0,"catalogContext":context,"traversalId":"fixture"}),
                &json!({}),
            )
            .unwrap();
        count += result["games"].as_array().unwrap().len();
        cursor = result["nextCursor"].as_str().unwrap().into();
        context = result["catalogContext"].clone();
        assert_eq!(result["traversalId"], "fixture");
    }
    assert_eq!(count, 1300);
    assert!(cursor.is_empty());
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn exact_game_lookup_preserves_selected_variant_readiness_and_rejects_wrong_identity() {
    let (url, worker) = mock_requests(
        vec![
            (200, json!({"requestStatus":{"serverId":"fixture-vpc"}})),
            (200, json!({"data":{"apps":{"items":[app(1)]}}})),
            (200, json!({"data":{"apps":{"items":[app(2)]}}})),
        ],
        |index, request| {
            if index == 0 {
                return;
            }
            let payload: Value =
                serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
            let query = payload["query"].as_str().unwrap();
            assert_eq!(query.matches('{').count(), query.matches('}').count());
            assert!(query.contains("appIds:$ids"));
            assert!(query.contains("supportedLanguages"));
            assert_eq!(payload["variables"]["ids"], json!(["app-1"]));
        },
    );
    let (service, path) = service(&url);
    let result = service
        .catalog_game(&json!({"appId":"app-1"}), &json!({}))
        .unwrap();
    assert_eq!(result["game"]["favorited"], true);
    assert_eq!(result["game"]["variants"][0]["playStatus"], "NOT_PLAYABLE");
    assert_eq!(
        result["game"]["variants"][0]["stateDetails"]["historicalEtaMins"],
        12.5
    );
    assert_eq!(
        service
            .catalog_game(&json!({"appId":"app-1"}), &json!({}))
            .unwrap_err()
            .code,
        "catalog_game_not_found"
    );
    assert!(
        service
            .catalog_game(&json!({"appId":"app-1","variantId":"123"}), &json!({}))
            .is_err()
    );
    assert!(
        service
            .catalog_game(&json!({"variantId":"2147483648"}), &json!({}))
            .is_err()
    );
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn targeted_game_lookups_omit_campaign_dependencies_and_keep_launch_metadata() {
    let (url, worker) = mock_requests(
        vec![
            (200, json!({"requestStatus":{"serverId":"fixture-vpc"}})),
            (200, json!({"data":{"apps":{"items":[app(1)]}}})),
            (200, json!({"data":{"apps":{"items":[app(1)]}}})),
        ],
        |index, request| {
            if index == 0 {
                return;
            }
            let payload: Value =
                serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
            let query = payload["query"].as_str().unwrap();
            assert!(!query.contains("itemMetadata"));
            assert!(!query.contains("campaignIds"));
            for field in [
                "favorited",
                "playStatus",
                "selected",
                "installed",
                "subscription",
                "stateDetails",
                "supportedLanguages",
                "paymentModels",
                "minimumMembershipTierLabel",
                "playabilityState",
            ] {
                assert!(query.contains(field), "missing {field}");
            }
            assert_eq!(payload["variables"]["vpcId"], "fixture-vpc");
            assert_eq!(payload["variables"]["locale"], "en_US");
            if index == 1 {
                assert!(query.contains("appIds:$ids"));
                assert_eq!(payload["variables"]["ids"], json!(["app-1"]));
            } else {
                assert!(query.contains("variantIds:$ids"));
                assert_eq!(payload["variables"]["ids"], json!([123]));
            }
        },
    );
    let (service, path) = service(&url);
    for params in [json!({"appId":"app-1"}), json!({"variantId":"123"})] {
        let result = service.catalog_game(&params, &json!({})).unwrap();
        assert_eq!(result["game"]["id"], "app-1");
        assert_eq!(result["game"]["variants"][0]["libraryStatus"], "MANUAL");
        assert_eq!(result["game"]["variants"][0]["playStatus"], "NOT_PLAYABLE");
        assert_eq!(result["game"]["variants"][0]["gfnStatus"], "PATCHING");
    }
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn overall_languages_are_anonymous_variable_free_cached_and_truthfully_stale() {
    let (url, worker) = mock_requests(
        vec![
            (
                200,
                json!({"data":{"overallGfnSupportedLanguages":[{"language":"en_US"},{"language":"es_419"},{"language":"zh_Hant_TW"},{"language":"en_US"}]}}),
            ),
            (
                200,
                json!({"data":{"overallGfnSupportedLanguages":[]},"errors":[{"message":"fixture failure"}]}),
            ),
        ],
        |_, request| {
            assert!(request.starts_with("POST /public "));
            assert!(!request.to_ascii_lowercase().contains("authorization:"));
            let payload: Value =
                serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
            assert_eq!(
                payload,
                json!({"query":"{ overallGfnSupportedLanguages { language } }"})
            );
        },
    );
    let (service, path) = service(&url);
    service.state.lock().unwrap().session = None;
    let result = service.catalog_languages(&json!({}), &json!({})).unwrap();
    assert_eq!(
        result["languages"],
        json!(["en_US", "es_419", "zh_Hant_TW"])
    );
    assert_eq!(result["status"], "success");
    assert_eq!(
        result["expiresAt"].as_u64().unwrap() - result["fetchedAt"].as_u64().unwrap(),
        14 * 24 * 60 * 60 * 1000
    );
    assert_eq!(
        service.catalog_languages(&json!({}), &json!({})).unwrap()["cacheHit"],
        true
    );
    let stale = service
        .catalog_languages(&json!({"refresh":true}), &json!({}))
        .unwrap();
    assert_eq!(stale["status"], "stale");
    assert_eq!(stale["languages"], result["languages"]);
    assert_eq!(stale["error"]["code"], "graphql_error");
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn invalid_language_metadata_is_not_cached_as_success() {
    for value in [
        Value::Null,
        json!([]),
        json!([{"language":""}]),
        json!([{"language":"auto"}]),
        json!([{"language":"system"}]),
        json!([{"language":"en__US"}]),
        json!([{"language":"en_US/../../"}]),
        json!([{"language":"x".repeat(65)}]),
        json!([{"language":42}]),
    ] {
        let (url, worker) = mock_requests(
            vec![(200, json!({"data":{"overallGfnSupportedLanguages":value}}))],
            |_, _| {},
        );
        let (service, path) = service(&url);
        let result = service.catalog_languages(&json!({}), &json!({})).unwrap();
        assert_eq!(result["status"], "error");
        assert_eq!(result["languages"], json!([]));
        worker.join().unwrap();
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[test]
fn revised_catalog_rejects_continuation_before_network_dispatch() {
    let (service, path) = service("http://127.0.0.1:1");
    service.invalidate_catalog().unwrap();
    assert_eq!(
        service
            .library_catalog(&json!({"cursor":"old","catalogRevision":0}), &json!({}))
            .unwrap_err()
            .code,
        "catalog_changed"
    );
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn local_store_refresh_revalidates_a_warm_disk_chain_and_reports_coverage() {
    let (url, worker) = mock_requests(
        vec![
            (200, json!({"requestStatus":{"serverId":"fixture-vpc"}})),
            (
                200,
                json!({"data":{"apps":{"items":[app(1)],"pageInfo":{"hasNextPage":true,"endCursor":"more","totalCount":50}}}}),
            ),
            (
                200,
                json!({"data":{"apps":{"items":[app(2)],"pageInfo":{"hasNextPage":false,"totalCount":1}}}}),
            ),
        ],
        |_, _| {},
    );
    let (service, path) = service(&url);
    let first = service.store_local_catalog(&json!({}), &json!({})).unwrap();
    assert_eq!(first["games"][0]["id"], "app-1");
    assert_eq!(first["cacheComplete"], false);
    assert_eq!(first["upstreamCoverage"], "partial");
    assert_eq!(first["localHasNextPage"], false);
    let refreshed = service
        .store_local_catalog(&json!({"refresh":true}), &json!({}))
        .unwrap();
    assert_eq!(refreshed["games"][0]["id"], "app-2");
    assert_eq!(refreshed["cacheComplete"], true);
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn local_store_bootstrap_uses_scope_refreshed_during_nested_read() {
    for params in [json!({}), json!({"refresh":true})] {
        let renewed_id = super::tests::jwt("fixture-account", now_ms() + 7_200_000);
        let (url, worker) = mock_requests(
            vec![
                (200, json!({"requestStatus":{"serverId":"old-vpc"}})),
                (401, json!({})),
                (
                    200,
                    json!({"access_token":"renewed-access","id_token":renewed_id,"expires_in":7200}),
                ),
                (200, json!({"requestStatus":{"serverId":"new-vpc"}})),
                (
                    200,
                    json!({"data":{"apps":{"items":[app(1)],"pageInfo":{"hasNextPage":false,"totalCount":1}}}}),
                ),
            ],
            |index, request| {
                if index == 1 {
                    assert!(request.contains("old-vpc"));
                }
                if index == 4 {
                    assert!(request.contains("new-vpc"));
                }
            },
        );
        let (service, path) = service(&url);
        let result = service.store_local_catalog(&params, &json!({}));
        worker.join().unwrap();
        assert!(
            result.is_ok(),
            "nested renewal left the local index in the old catalog scope: {:?}",
            result.err()
        );
        let result = result.unwrap();
        assert_eq!(result["games"][0]["id"], "app-1");
        assert_eq!(result["scope"]["generation"], 0);
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[test]
fn optional_definition_failure_does_not_hide_server_disabled_store_actions() {
    let (url, worker) = mock_requests(
        vec![
            (
                200,
                json!({"data":{"appStoreDefinitions":[{"store":"STEAM","label":"Steam","features":[
            {"__typename":"AccountGamesSyncing","supported":false}]}]}}),
            ),
            (503, json!({})),
            (
                200,
                json!({"data":{"subscriptionDefinitions":[{"subscription":"GAMEPASS","label":"Game Pass","primaryStore":"XBOX"}]}}),
            ),
            (
                200,
                json!({"data":{"userAccount":{"subscriptions":[{"id":"GAMEPASS"}],"storesData":[]}}}),
            ),
        ],
        |_, _| {},
    );
    let (service, path) = service(&url);
    let result = service.account_connections(&json!({})).unwrap();
    assert_eq!(result["accounts"][0]["supportsSync"], false);
    assert_eq!(result["accounts"][0]["capabilitySource"], "success");
    assert_eq!(result["definitions"]["genres"]["status"], "error");
    assert_eq!(result["subscriptions"][0]["id"], "GAMEPASS");
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn delayed_anonymous_metadata_is_rejected_after_a_context_change() {
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let (url, worker) = mock_requests(
        vec![(
            200,
            json!({"data":{"overallGfnSupportedLanguages":[{"language":"en_US"}]}}),
        )],
        move |_, _| {
            entered_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        },
    );
    let (service, path) = service(&url);
    std::thread::scope(|threads| {
        let pending = threads.spawn(|| service.catalog_languages(&json!({}), &json!({})));
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        service.state.lock().unwrap().generation += 1;
        release_tx.send(()).unwrap();
        assert_eq!(pending.join().unwrap().unwrap_err().code, "stale_account");
    });
    worker.join().unwrap();
    assert!(!path.join("store-cache-v2").exists());
    std::fs::remove_dir_all(path).unwrap();
}
