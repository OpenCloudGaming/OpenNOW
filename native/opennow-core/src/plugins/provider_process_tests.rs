use super::*;
use crate::plugins::PluginManager;
use crate::plugins::module::Module;
use crate::plugins::provider_registry_tests::{archive, enabled, install};
use opennow_plugin_api::PluginId;
use opennow_plugin_api::provider::{self as api, Empty};
use serde_json::json;
use std::fs;
use std::sync::mpsc;
use std::thread;

fn wait_for(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !predicate() {
        assert!(
            Instant::now() < deadline,
            "provider transition did not settle"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn dispatch_generation_binds_recovered_requests_and_older_inflight_completions() {
    let temp = tempfile::tempdir().unwrap();
    let (output, _events) = mpsc::channel();
    let manager = PluginManager::open(&temp.path().join("profile"), output).unwrap();
    install(&manager, &archive(temp.path()));
    enabled(&manager, true).unwrap();
    let id = PluginId::new("org.opennow.test.provider").unwrap();
    let module = manager.module(&id).unwrap();
    let Module::Provider(provider) = module.as_ref() else {
        panic!("expected provider control process")
    };
    let data = manager.media_data_dir(&id).unwrap();
    let before = provider.generation();
    let cancellation = Cancellation::default();
    let context = ProviderContext {
        cancellation: &cancellation,
        runtime_capabilities: None,
        gfn_settings: None,
    };

    let unsupported = ProviderRequest::CatalogPublic(api::CatalogRequest {
        scope: api::CatalogScope::Public,
        query: CatalogQuery {
            query: String::new(),
            cursor: None,
            limit: 1,
        },
    });
    let rejected = provider.provider_call(&unsupported, &context);
    assert!(!rejected.dispatched);
    assert_eq!(rejected.dispatched_generation, None);

    fs::write(data.join("2.gate"), b"wait for both dispatches").unwrap();
    fs::write(data.join("3.delay"), b"600000").unwrap();
    fs::write(data.join("exit-after-response"), b"exit").unwrap();
    let response = serde_json::to_string(&json!({"type":"response","v":2,"epoch":0,"id":"$ID",
        "outcome":{"status":"success","reply":{"method":"auth.status","result":{"state":"not-required"}}},
        "effects":[],"allocation":null})).unwrap().replace("\"epoch\":0", "\"epoch\":$EPOCH");
    fs::write(data.join("2.reply"), response).unwrap();

    let old_request = ProviderRequest::AuthStatus(Empty {});
    let old_reply = provider
        .dispatch(&old_request, &cancellation, Duration::from_secs(5))
        .unwrap();
    let session = json!({"account":null,"remoteId":"original-seat"});
    let interrupted_request =
        ProviderRequest::SessionPoll(serde_json::from_value(session.clone()).unwrap());
    let interrupted = provider
        .dispatch(&interrupted_request, &cancellation, Duration::from_secs(5))
        .unwrap();
    assert_eq!(old_reply.generation, before);
    assert_eq!(interrupted.generation, before);
    assert!(Arc::ptr_eq(&old_reply.runtime, &interrupted.runtime));
    fs::write(data.join("2.release"), b"deliver old reply then exit").unwrap();
    wait_for(|| old_reply.runtime.unhealthy() && !old_reply.runtime.busy());
    wait_for(|| provider.descriptor().state == PluginState::Failed);

    for name in [
        "2.gate",
        "2.release",
        "2.reply",
        "3.delay",
        "exit-after-response",
    ] {
        fs::remove_file(data.join(name)).unwrap();
    }
    let response = serde_json::to_string(&json!({"type":"response","v":2,"epoch":0,"id":"$ID",
        "outcome":{"status":"success","reply":{"method":"session.reconcile","result":{"state":"terminal","session":session,"reason":"remote_ended"}}},
        "effects":[],"allocation":null})).unwrap().replace("\"epoch\":0", "\"epoch\":$EPOCH");
    fs::write(data.join("2.reply"), response).unwrap();
    let recovery = ProviderRequest::SessionReconcile(
        serde_json::from_value(
            json!({"scope":null,"operation":"recover-original","session":session}),
        )
        .unwrap(),
    );
    let recovered = provider.provider_call(&recovery, &context);
    assert!(matches!(
        recovered.result.unwrap(),
        ProviderReply::SessionReconcile(api::Reconciliation::Terminal { .. })
    ));
    let after = provider.generation();
    assert!(after > before);
    assert_eq!(recovered.dispatched_generation, Some(after));

    let old = old_reply.complete(&old_request, &cancellation);
    assert!(matches!(
        old.result.unwrap(),
        ProviderReply::AuthStatus(api::AuthState::NotRequired)
    ));
    assert!(old.dispatched);
    assert_eq!(old.dispatched_generation, Some(before));
    assert_ne!(old.dispatched_generation, recovered.dispatched_generation);

    let interrupted = interrupted.complete(&interrupted_request, &cancellation);
    assert!(interrupted.dispatched);
    assert!(interrupted.result.is_err());
    assert_eq!(interrupted.dispatched_generation, Some(before));
}
