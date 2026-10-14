use super::*;
use crate::sources::contract::ProviderContext;
use opennow_plugin_api::provider::{Empty, ProviderRequest};
use opennow_plugin_package::Role;
use sha2::{Digest, Sha256};
use std::sync::mpsc;

const ID: &str = "org.opennow.test.provider";

pub(in crate::plugins) fn archive(root: &Path) -> PathBuf {
    let bytes = fs::read(super::provider_transport::tests::fixture()).unwrap();
    let digest = format!("{:x}", Sha256::digest(&bytes));
    let manifest = json!({
        "schemaVersion":2,"protocolVersion":2,"id":ID,
        "name":"Provider runtime test","version":"1.0.0","publisher":"Tests","description":"Synthetic lifecycle provider",
        "capabilities":["auth.anonymous.v2","catalog.library.v2","catalog.details.v2","launch.v2","sessions.v2","media.worker.v1"],
        "authKinds":["anonymous"],
        "entrypoints":{opennow_plugin_package::current_target():{"control":"control","media":"media"}},
        "files":[{"path":"control","sha256":digest},{"path":"media","sha256":digest}]
    });
    let path = root.join("fixture.opennow-plugin");
    let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
    zip.start_file("manifest.json", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(&serde_json::to_vec(&manifest).unwrap())
        .unwrap();
    for name in ["control", "media"] {
        zip.start_file(
            name,
            zip::write::SimpleFileOptions::default().unix_permissions(0o755),
        )
        .unwrap();
        zip.write_all(&bytes).unwrap();
    }
    zip.finish().unwrap();
    path
}

pub(in crate::plugins) fn install(manager: &PluginManager, package: &Path) {
    let inspected = manager
        .dispatch(
            "plugins.install.inspect",
            &json!({"path":package}),
            &Cancellation::default(),
        )
        .unwrap();
    assert_eq!(
        inspected["inspection"]["plugin"]["trust"],
        "unsigned-native"
    );
    let result = manager.dispatch("plugins.install.commit", &json!({
        "token":inspected["inspection"]["token"], "expectedGeneration":inspected["generation"], "consent":true
    }), &Cancellation::default()).unwrap();
    assert_eq!(result["plugins"][0]["state"], "disabled");
}

pub(in crate::plugins) fn enabled(
    manager: &PluginManager,
    enabled: bool,
) -> Result<Value, SourceError> {
    manager.dispatch(
        "plugins.setEnabled",
        &json!({"id":ID,"enabled":enabled,"expectedGeneration":manager.snapshot().generation}),
        &Cancellation::default(),
    )
}

fn wait_ready(manager: &PluginManager) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while manager.snapshot().plugins[0].state != PluginState::Ready {
        assert!(
            Instant::now() < deadline,
            "provider did not become ready: {:?}",
            manager.snapshot()
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn provider_package_installs_and_reuses_typed_control_across_calls() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("profile");
    let (output, _events) = mpsc::channel();
    let manager = PluginManager::open(&data, output).unwrap();
    install(&manager, &archive(temp.path()));
    assert!(!data.join("plugins/data").join(ID).exists());
    enabled(&manager, true).unwrap();
    let id = PluginId::new(ID).unwrap();
    let first = manager.provider(&id).unwrap();
    let second = manager.provider(&id).unwrap();
    assert!(Arc::ptr_eq(&first, &second));
    let cancellation = Cancellation::default();
    let context = ProviderContext {
        cancellation: &cancellation,
        runtime_capabilities: None,
        gfn_settings: None,
    };
    for _ in 0..2 {
        let completion = first.provider_call(&ProviderRequest::AuthStatus(Empty {}), &context);
        assert!(completion.dispatched);
        assert_eq!(completion.result.unwrap_err().code, "service_unavailable");
    }
    assert_eq!(manager.snapshot().plugins[0].state, PluginState::Ready);
    let package = manager.media_package(&id).unwrap();
    assert!(
        package
            .entrypoint(Role::Control)
            .unwrap()
            .ends_with("control")
    );
    assert!(package.entrypoint(Role::Media).unwrap().ends_with("media"));
    assert_eq!(enabled(&manager, false).unwrap_err().code, "plugin_in_use");
    drop(package);
    enabled(&manager, false).unwrap();
    manager
        .dispatch(
            "plugins.uninstall",
            &json!({"id":ID,"confirmed":true,"expectedGeneration":manager.snapshot().generation}),
            &cancellation,
        )
        .unwrap();
    assert!(manager.snapshot().plugins.is_empty());
    assert!(!data.join("plugins/installed").join(ID).exists());
}

#[test]
fn retained_native_package_pin_blocks_mutation_after_core_restart() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("profile");
    let (output, _events) = mpsc::channel();
    let manager = PluginManager::open(&data, output.clone()).unwrap();
    install(&manager, &archive(temp.path()));
    enabled(&manager, true).unwrap();
    let pin = manager.media_package(&PluginId::new(ID).unwrap()).unwrap();
    manager.shutdown();
    drop(manager);
    let restarted = PluginManager::open(&data, output).unwrap();
    wait_ready(&restarted);
    assert_eq!(
        enabled(&restarted, false).unwrap_err().code,
        "plugin_in_use"
    );
    drop(pin);
    enabled(&restarted, false).unwrap();
}

#[test]
fn tampered_media_role_prevents_control_start_too() {
    let temp = tempfile::tempdir().unwrap();
    let (output, _events) = mpsc::channel();
    let manager = PluginManager::open(&temp.path().join("profile"), output).unwrap();
    install(&manager, &archive(temp.path()));
    let id = PluginId::new(ID).unwrap();
    let entries = manager.inner.entries.lock().unwrap();
    let record = &entries.get(ID).unwrap().record;
    let root = manager
        .inner
        .root
        .join("installed")
        .join(ID)
        .join(&record.package_sha256);
    drop(entries);
    fs::write(root.join("media"), b"tampered").unwrap();
    assert!(enabled(&manager, true).is_err());
    assert_eq!(
        manager.provider(&id).unwrap().descriptor().state,
        PluginState::Failed
    );
}

#[test]
fn crashed_control_restarts_only_for_explicit_session_recovery() {
    let temp = tempfile::tempdir().unwrap();
    let (output, _events) = mpsc::channel();
    let manager = PluginManager::open(&temp.path().join("profile"), output).unwrap();
    install(&manager, &archive(temp.path()));
    enabled(&manager, true).unwrap();
    let data = manager.inner.root.join("data").join(ID);
    let original_pid = fs::read_to_string(data.join("last-process-id")).unwrap();
    fs::write(data.join("exit-after-response"), b"exit").unwrap();
    let provider = manager.provider(&PluginId::new(ID).unwrap()).unwrap();
    let cancellation = Cancellation::default();
    let context = ProviderContext {
        cancellation: &cancellation,
        runtime_capabilities: None,
        gfn_settings: None,
    };
    let _ = provider.provider_call(&ProviderRequest::AuthStatus(Empty {}), &context);
    let deadline = Instant::now() + Duration::from_secs(5);
    while provider.descriptor().state != PluginState::Failed {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    fs::remove_file(data.join("exit-after-response")).unwrap();
    assert_eq!(
        provider
            .provider_call(&ProviderRequest::AuthStatus(Empty {}), &context)
            .result
            .unwrap_err()
            .code,
        "provider_unavailable"
    );
    assert_eq!(
        fs::read_to_string(data.join("last-process-id")).unwrap(),
        original_pid
    );
    let request = ProviderRequest::SessionReconcile(
        serde_json::from_value(json!({"scope":null,"operation":"recover-original","session":null}))
            .unwrap(),
    );
    assert_eq!(
        provider
            .provider_call(&request, &context)
            .result
            .unwrap_err()
            .code,
        "service_unavailable"
    );
    assert_ne!(
        fs::read_to_string(data.join("last-process-id")).unwrap(),
        original_pid
    );
    assert_eq!(provider.descriptor().state, PluginState::Ready);
}

#[cfg(unix)]
#[test]
fn shutdown_during_provider_greeting_reaps_the_owned_child() {
    let temp = tempfile::tempdir().unwrap();
    let (output, _events) = mpsc::channel();
    let manager = Arc::new(PluginManager::open(&temp.path().join("profile"), output).unwrap());
    install(&manager, &archive(temp.path()));
    let data = manager.inner.root.join("data").join(ID);
    fs::create_dir_all(&data).unwrap();
    fs::write(data.join("1.delay"), b"1000").unwrap();
    let caller = Arc::clone(&manager);
    let enabling = thread::spawn(move || enabled(&caller, true));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !data.join("last-process-id").exists() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    let pid: i32 = fs::read_to_string(data.join("last-process-id"))
        .unwrap()
        .parse()
        .unwrap();
    manager.shutdown();
    assert!(enabling.join().unwrap().is_err());
    while unsafe { libc::kill(pid, 0) } == 0 {
        assert!(
            Instant::now() < deadline,
            "provider process survived shutdown"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn unanswered_stop_retires_control_then_explicit_reconcile_completes_without_releasing_media_pin() {
    let temp = tempfile::tempdir().unwrap();
    let (output, _events) = mpsc::channel();
    let manager = PluginManager::open(&temp.path().join("profile"), output).unwrap();
    install(&manager, &archive(temp.path()));
    enabled(&manager, true).unwrap();
    let id = PluginId::new(ID).unwrap();
    let data = manager.media_data_dir(&id).unwrap();
    let native_pin = manager.media_package(&id).unwrap();
    let original_pid = fs::read_to_string(data.join("last-process-id")).unwrap();
    fs::write(data.join("2.delay"), b"600000").unwrap();
    let provider = manager.provider(&id).unwrap();
    let cancellation = Cancellation::default();
    let context = ProviderContext {
        cancellation: &cancellation,
        runtime_capabilities: None,
        gfn_settings: None,
    };
    let session = json!({"account":null,"remoteId":"original-seat"});
    let stop = ProviderRequest::SessionStop(
        serde_json::from_value(json!({"session":session,"operation":"original-stop"})).unwrap(),
    );
    let started = Instant::now();
    let completion = provider.provider_call(&stop, &context);
    assert!(completion.dispatched);
    assert_eq!(completion.result.unwrap_err().code, "outcome_unknown");
    let deadline = Instant::now() + Duration::from_secs(5);
    while provider.descriptor().state != PluginState::Failed {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    assert!(started.elapsed() < Duration::from_secs(36));
    fs::remove_file(data.join("2.delay")).unwrap();
    let response = serde_json::to_string(&json!({"type":"response","v":2,"epoch":0,"id":"$ID",
        "outcome":{"status":"success","reply":{"method":"session.reconcile","result":{"state":"terminal","session":session,"reason":"user_stopped"}}},
        "effects":[],"allocation":null})).unwrap().replace("\"epoch\":0", "\"epoch\":$EPOCH");
    fs::write(data.join("2.reply"), response).unwrap();
    let reconcile = ProviderRequest::SessionReconcile(
        serde_json::from_value(json!({"scope":null,"operation":"original-stop","session":session}))
            .unwrap(),
    );
    let completion = provider.provider_call(&reconcile, &context);
    assert!(completion.dispatched);
    assert!(matches!(
        completion.result.unwrap(),
        opennow_plugin_api::provider::ProviderReply::SessionReconcile(
            opennow_plugin_api::provider::Reconciliation::Terminal { .. }
        )
    ));
    assert_ne!(
        fs::read_to_string(data.join("last-process-id")).unwrap(),
        original_pid
    );
    assert_eq!(enabled(&manager, false).unwrap_err().code, "plugin_in_use");
    assert!(data.is_dir());
    drop(native_pin);
    enabled(&manager, false).unwrap();
}

#[test]
fn orphan_replacement_cannot_clear_data_while_the_new_version_pin_is_held() {
    let temp = tempfile::tempdir().unwrap();
    let (output, _events) = mpsc::channel();
    let manager = PluginManager::open(&temp.path().join("profile"), output).unwrap();
    let package = archive(temp.path());
    let digest = format!("{:x}", Sha256::digest(fs::read(&package).unwrap()));
    let parent = manager.inner.root.join("installed").join(ID);
    fs::create_dir_all(&parent).unwrap();
    let pin = PackagePin::shared(&parent.join(digest)).unwrap();
    let data = manager.inner.root.join("data").join(ID);
    fs::create_dir_all(&data).unwrap();
    fs::write(data.join("owned-state"), b"preserve while pinned").unwrap();
    let inspection = manager
        .dispatch(
            "plugins.install.inspect",
            &json!({"path":package}),
            &Cancellation::default(),
        )
        .unwrap();
    let result = manager.dispatch("plugins.install.commit", &json!({"token":inspection["inspection"]["token"],"expectedGeneration":inspection["generation"],"consent":true}), &Cancellation::default());
    assert_eq!(result.unwrap_err().code, "plugin_in_use");
    assert_eq!(
        fs::read(data.join("owned-state")).unwrap(),
        b"preserve while pinned"
    );
    assert!(manager.snapshot().plugins.is_empty());
    drop(pin);
    install(&manager, &package);
    assert!(!data.exists());
}

#[test]
fn unanswered_polls_retire_control_then_reconcile_restarts_without_releasing_media_pin() {
    let temp = tempfile::tempdir().unwrap();
    let (output, _events) = mpsc::channel();
    let manager = PluginManager::open(&temp.path().join("profile"), output).unwrap();
    install(&manager, &archive(temp.path()));
    enabled(&manager, true).unwrap();
    let id = PluginId::new(ID).unwrap();
    let data = manager.media_data_dir(&id).unwrap();
    let native_pin = manager.media_package(&id).unwrap();
    let original_pid = fs::read_to_string(data.join("last-process-id")).unwrap();
    for request_id in 2..=5 {
        fs::write(data.join(format!("{request_id}.delay")), b"600000").unwrap();
    }
    let provider = manager.provider(&id).unwrap();
    let session = json!({"account":null,"remoteId":"original-seat"});
    let mut calls = Vec::new();
    let started = Instant::now();
    for _ in 0..4 {
        let provider = Arc::clone(&provider);
        let request =
            ProviderRequest::SessionPoll(serde_json::from_value(session.clone()).unwrap());
        calls.push(thread::spawn(move || {
            let cancellation = Cancellation::default();
            provider.provider_call(
                &request,
                &ProviderContext {
                    cancellation: &cancellation,
                    runtime_capabilities: None,
                    gfn_settings: None,
                },
            )
        }));
    }
    for call in calls {
        let completion = call.join().unwrap();
        assert!(completion.dispatched);
        assert_eq!(completion.result.unwrap_err().code, "provider_unavailable");
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while provider.descriptor().state != PluginState::Failed {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    assert!(started.elapsed() < Duration::from_secs(31));
    for request_id in 2..=5 {
        fs::remove_file(data.join(format!("{request_id}.delay"))).unwrap();
    }
    let response = serde_json::to_string(&json!({"type":"response","v":2,"epoch":0,"id":"$ID",
        "outcome":{"status":"success","reply":{"method":"session.reconcile","result":{"state":"terminal","session":session,"reason":"remote_ended"}}},
        "effects":[],"allocation":null})).unwrap().replace("\"epoch\":0", "\"epoch\":$EPOCH");
    fs::write(data.join("2.reply"), response).unwrap();
    let request = ProviderRequest::SessionReconcile(
        serde_json::from_value(json!({"scope":null,"operation":"recover-polls","session":session}))
            .unwrap(),
    );
    let cancellation = Cancellation::default();
    let completion = provider.provider_call(
        &request,
        &ProviderContext {
            cancellation: &cancellation,
            runtime_capabilities: None,
            gfn_settings: None,
        },
    );
    assert!(completion.dispatched);
    assert!(matches!(
        completion.result.unwrap(),
        opennow_plugin_api::provider::ProviderReply::SessionReconcile(
            opennow_plugin_api::provider::Reconciliation::Terminal { .. }
        )
    ));
    assert_ne!(
        fs::read_to_string(data.join("last-process-id")).unwrap(),
        original_pid
    );
    assert_eq!(enabled(&manager, false).unwrap_err().code, "plugin_in_use");
    drop(native_pin);
    enabled(&manager, false).unwrap();
}
