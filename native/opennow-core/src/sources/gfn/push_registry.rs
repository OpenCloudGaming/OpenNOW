use crate::sources::gfn::push::{
    GfnTokenSource, PUSH_EVENT_NAME, PushConfig, PushError, PushOwner, PushOwnerConfig,
    PushOwnerDeps, PushScope, PushSink, RegistrationStore, ReqwestPushHttp, ScopeSource,
    TlsPushTransportFactory, bundled_push_config, push_config_for_provider, push_event_payload,
};
use crate::sources::gfn::service::GfnService;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::Duration;

pub struct DesiredPush {
    pub config: PushOwnerConfig,
}

pub fn desired_push(data_dir: &Path, scope: &PushScope, device_id: &str) -> Option<DesiredPush> {
    let config = match push_config_for_provider(data_dir, &scope.provider_id, device_id.to_owned())
    {
        PushConfig::Absent => bundled_push_config(&scope.provider_id, device_id.to_owned()),
        PushConfig::Configured(config) => *config,
        PushConfig::Suppressed => return None,
    };
    if config.endpoints.pns.is_empty() || config.endpoints.pns_client_id.is_empty() {
        return None;
    }
    Some(DesiredPush { config })
}

pub type DepsFactory = dyn Fn(Arc<GfnService>, ScopeSource, Sender<Value>) -> Result<PushOwnerDeps, PushError>
    + Send
    + Sync;

pub struct PushRegistry {
    gfn: Arc<GfnService>,
    output: Sender<Value>,
    data_dir: PathBuf,
    scope_source: ScopeSource,
    owner: Option<PushOwner>,
    deps_factory: Arc<DepsFactory>,
    core_exit: Arc<AtomicBool>,
}

impl PushRegistry {
    pub fn new(gfn: Arc<GfnService>, output: Sender<Value>, data_dir: PathBuf) -> Self {
        let scope_source: ScopeSource = {
            let gfn = Arc::clone(&gfn);
            Arc::new(move || gfn.push_scope())
        };
        Self::with_deps_factory(
            gfn,
            output,
            data_dir,
            scope_source,
            Arc::new(production_deps),
        )
    }

    pub fn with_deps_factory(
        gfn: Arc<GfnService>,
        output: Sender<Value>,
        data_dir: PathBuf,
        scope_source: ScopeSource,
        deps_factory: Arc<DepsFactory>,
    ) -> Self {
        Self {
            gfn,
            output,
            data_dir,
            scope_source,
            owner: None,
            deps_factory,
            core_exit: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn reconcile(&mut self) -> Result<(), PushError> {
        if self.core_exit.load(Ordering::Acquire) {
            self.stop_for_core_exit();
            return Ok(());
        }
        let scope = (self.scope_source)();
        let desired = scope
            .as_ref()
            .and_then(|scope| desired_push(&self.data_dir, scope, self.gfn.device_id()));
        if let Some(owner) = self.owner.as_mut() {
            let Some(desired) = desired else {
                owner.stop();
                return Ok(());
            };
            if owner.config().0 != desired.config {
                owner.update_config(desired.config);
            }
            return owner.start();
        }
        let Some(desired) = desired else {
            return Ok(());
        };
        let deps = (self.deps_factory)(
            Arc::clone(&self.gfn),
            Arc::clone(&self.scope_source),
            self.output.clone(),
        )?;
        let mut owner =
            PushOwner::with_core_exit(desired.config, deps, Arc::clone(&self.core_exit));
        owner.start()?;
        self.owner = Some(owner);
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(mut owner) = self.owner.take() {
            owner.shutdown(Duration::from_secs(5));
        }
    }

    pub fn pause(&mut self) {
        if let Some(owner) = &mut self.owner {
            owner.stop();
        }
    }

    pub fn core_exit_signal(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.core_exit)
    }

    pub fn stop_for_core_exit(&mut self) {
        self.core_exit.store(true, Ordering::Release);
        if let Some(mut owner) = self.owner.take() {
            owner.shutdown_for_core_exit();
        }
    }
}

pub fn production_deps(
    gfn: Arc<GfnService>,
    scope_source: ScopeSource,
    output: Sender<Value>,
) -> Result<PushOwnerDeps, PushError> {
    let token_source: GfnTokenSource = {
        let gfn = Arc::clone(&gfn);
        Arc::new(move |scope: &PushScope| gfn.push_token_for_scope(scope))
    };
    let sink: PushSink = Arc::new(move |event, generation| {
        let _ = output.send(serde_json::json!({
            "type": "event",
            "name": PUSH_EVENT_NAME,
            "payload": push_event_payload(&event, generation),
        }));
    });
    Ok(PushOwnerDeps {
        http: Arc::new(ReqwestPushHttp::new()?),
        transport: Arc::new(TlsPushTransportFactory::new()),
        store: Arc::new(RegistrationStore::default_service()),
        sink,
        scope_source,
        token_source,
    })
}

impl Drop for PushRegistry {
    fn drop(&mut self) {
        if self.core_exit.load(Ordering::Acquire) {
            self.stop_for_core_exit();
        } else {
            self.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::gfn::push::{
        HttpRequest, HttpResponse, PushEvent, PushHttp, PushScope, PushStateStore, PushTransport,
        PushTransportFactory, Registration,
    };
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Condvar, Mutex};
    use std::time::{Duration, Instant};

    const HOLD_DEADLINE: Duration = Duration::from_secs(30);

    fn scope() -> PushScope {
        PushScope {
            user_id: "user-1".into(),
            provider_id: "nvidia".into(),
            generation: 3,
        }
    }

    fn config_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("opennow-push-registry-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(crate::sources::gfn::push::PUSH_CONFIG_FILE),
            raw_config().to_string(),
        )
        .unwrap();
        dir
    }

    fn empty_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("opennow-push-registry-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn raw_config() -> Value {
        json!({
            "providerIdpId": "nvidia",
            "projectId": "project",
            "apiKey": "api-key",
            "senderId": "123456789012",
            "appId": "app",
            "firebaseAppId": "1:1:web:abc",
            "pnsServer": "https://pns.example",
            "pnsClientId": "client",
        })
    }

    fn registration_body() -> Vec<u8> {
        let mut body = vec![0x39];
        body.extend_from_slice(&42_u64.to_le_bytes());
        body.push(0x41);
        body.extend_from_slice(&7_u64.to_le_bytes());
        body
    }

    fn registration_responses() -> Vec<HttpResponse> {
        vec![
            HttpResponse {
                status: 200,
                body: registration_body(),
            },
            HttpResponse {
                status: 200,
                body: b"token=gcm-token".to_vec(),
            },
            HttpResponse {
                status: 200,
                body: br#"{"authToken":{"token":"fis-token"}}"#.to_vec(),
            },
            HttpResponse {
                status: 200,
                body: br#"{"token":"fcm-token"}"#.to_vec(),
            },
            HttpResponse {
                status: 204,
                body: Vec::new(),
            },
        ]
    }

    struct FakeHttp {
        responses: Mutex<Vec<HttpResponse>>,
        calls: Arc<AtomicUsize>,
        release: Option<Arc<Blocking>>,
    }

    struct Blocking {
        released: Mutex<bool>,
        wake: Condvar,
        abandoned: AtomicBool,
    }

    impl Blocking {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                released: Mutex::new(false),
                wake: Condvar::new(),
                abandoned: AtomicBool::new(false),
            })
        }

        fn release(&self) {
            *self.released.lock().unwrap() = true;
            self.wake.notify_all();
        }

        fn abandoned(&self) -> bool {
            self.abandoned.load(Ordering::SeqCst)
        }

        fn wait(&self) {
            let mut released = self.released.lock().unwrap();
            let deadline = Instant::now() + HOLD_DEADLINE;
            loop {
                if *released {
                    return;
                }
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    drop(released);
                    self.abandoned.store(true, Ordering::SeqCst);
                    panic!("the blocked fixture hold was not released within {HOLD_DEADLINE:?}");
                }
                let (state, _) = self.wake.wait_timeout(released, remaining).unwrap();
                released = state;
            }
        }

        fn wait_bounded(&self, timeout: Duration) {
            let mut released = self.released.lock().unwrap();
            let deadline = Instant::now() + timeout;
            while !*released {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return;
                }
                let (state, _) = self.wake.wait_timeout(released, remaining).unwrap();
                released = state;
            }
        }
    }

    impl PushHttp for FakeHttp {
        fn execute(&self, _request: &HttpRequest) -> Result<HttpResponse, PushError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if let Some(release) = &self.release {
                release.wait();
            }
            let mut responses = self.responses.lock().unwrap();
            if responses.is_empty() {
                return Err(PushError::new("push_http_failed", "No scripted response"));
            }
            Ok(responses.remove(0))
        }
    }

    struct MemoryStore(Mutex<BTreeMap<String, Registration>>);

    impl MemoryStore {
        fn empty() -> Arc<Self> {
            Arc::new(Self(Mutex::new(BTreeMap::new())))
        }
    }

    impl PushStateStore for MemoryStore {
        fn load(&self, account: &str) -> Result<Option<Registration>, PushError> {
            Ok(self.0.lock().unwrap().get(account).cloned())
        }

        fn save(&self, account: &str, registration: &Registration) -> Result<(), PushError> {
            self.0
                .lock()
                .unwrap()
                .insert(account.to_owned(), registration.clone());
            Ok(())
        }

        fn clear(&self, account: &str) -> Result<(), PushError> {
            self.0.lock().unwrap().remove(account);
            Ok(())
        }
    }

    struct BlockingTransport {
        active: Arc<AtomicUsize>,
    }

    impl PushTransport for BlockingTransport {
        fn send(&mut self, _frame: &[u8], _timeout: Duration) -> Result<(), PushError> {
            Ok(())
        }

        fn recv(&mut self, _timeout: Duration) -> Result<Vec<u8>, PushError> {
            std::thread::sleep(Duration::from_millis(5));
            Ok(Vec::new())
        }

        fn close(&mut self) {}
    }

    impl Drop for BlockingTransport {
        fn drop(&mut self) {
            self.active.fetch_sub(1, Ordering::SeqCst);
        }
    }

    struct BlockingFactory {
        connection: Arc<Blocking>,
        connects: Arc<AtomicUsize>,
        active: Arc<AtomicUsize>,
        maximum: Arc<AtomicUsize>,
    }

    impl PushTransportFactory for BlockingFactory {
        fn connect(
            &self,
            _host: &str,
            _port: u16,
            _timeout: Duration,
        ) -> Result<Box<dyn PushTransport>, PushError> {
            self.connects.fetch_add(1, Ordering::SeqCst);
            self.connection.wait_bounded(Duration::from_millis(20));
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.maximum.fetch_max(active, Ordering::SeqCst);
            Ok(Box::new(BlockingTransport {
                active: Arc::clone(&self.active),
            }))
        }
    }

    struct Harness {
        registry: PushRegistry,
        scope: Arc<Mutex<Option<PushScope>>>,
        tokens: Arc<Mutex<Vec<PushScope>>>,
        connect: Arc<Blocking>,
        connects: Arc<AtomicUsize>,
        active: Arc<AtomicUsize>,
        maximum: Arc<AtomicUsize>,
        factories: Arc<AtomicUsize>,
        http_calls: Arc<AtomicUsize>,
    }

    impl Drop for Harness {
        fn drop(&mut self) {
            self.connect.release();
            if !std::thread::panicking() {
                assert!(
                    !self.connect.abandoned(),
                    "the blocked fixture hold must be released before its safety deadline"
                );
            }
        }
    }

    fn workers(harness: &Harness) -> usize {
        harness
            .registry
            .owner
            .as_ref()
            .map_or(0, PushOwner::worker_count)
    }

    fn harness(data_dir: PathBuf, script_registration: bool) -> Harness {
        harness_with(data_dir, script_registration, false)
    }

    fn harness_with(
        data_dir: PathBuf,
        script_registration: bool,
        hold_first_stage: bool,
    ) -> Harness {
        let (output, _rx) = std::sync::mpsc::channel();
        let gfn = Arc::new(GfnService::new(std::env::temp_dir()).unwrap());
        let scope_state = Arc::new(Mutex::new(Some(scope())));
        let registry_scope: ScopeSource = {
            let scope_state = Arc::clone(&scope_state);
            Arc::new(move || scope_state.lock().unwrap().clone())
        };
        let tokens = Arc::new(Mutex::new(Vec::new()));
        let connect = Blocking::new();
        let connects = Arc::new(AtomicUsize::new(0));
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let factories = Arc::new(AtomicUsize::new(0));
        let http_calls = Arc::new(AtomicUsize::new(0));
        let holds = Arc::new(AtomicBool::new(hold_first_stage));
        let scripted = Arc::new(AtomicBool::new(script_registration));
        let deps_scope = Arc::clone(&scope_state);
        let deps_tokens = Arc::clone(&tokens);
        let deps_connect = Arc::clone(&connect);
        let deps_connects = Arc::clone(&connects);
        let deps_active = Arc::clone(&active);
        let deps_maximum = Arc::clone(&maximum);
        let deps_factories = Arc::clone(&factories);
        let deps_http_calls = Arc::clone(&http_calls);
        let deps_holds = Arc::clone(&holds);
        let deps_scripted = Arc::clone(&scripted);
        let deps_factory: Arc<DepsFactory> = Arc::new(move |_gfn, _scope_source, _output| {
            deps_factories.fetch_add(1, Ordering::SeqCst);
            let scope_source: ScopeSource = {
                let scope_state = Arc::clone(&deps_scope);
                Arc::new(move || scope_state.lock().unwrap().clone())
            };
            let token_source: GfnTokenSource = {
                let scope_state = Arc::clone(&deps_scope);
                let tokens = Arc::clone(&deps_tokens);
                Arc::new(move |scope: &PushScope| {
                    let current = scope_state.lock().unwrap().clone()?;
                    if current.user_id != scope.user_id
                        || current.provider_id != scope.provider_id
                        || current.generation != scope.generation
                    {
                        return None;
                    }
                    tokens.lock().unwrap().push(scope.clone());
                    Some("gfnjwt".to_owned())
                })
            };
            let release = deps_holds
                .load(Ordering::SeqCst)
                .then(|| Arc::clone(&deps_connect));
            let http: Arc<dyn PushHttp> = Arc::new(FakeHttp {
                responses: Mutex::new(if deps_scripted.load(Ordering::SeqCst) {
                    registration_responses()
                } else {
                    Vec::new()
                }),
                calls: Arc::clone(&deps_http_calls),
                release,
            });
            Ok(PushOwnerDeps {
                http,
                transport: Arc::new(BlockingFactory {
                    connection: Arc::clone(&deps_connect),
                    connects: Arc::clone(&deps_connects),
                    active: Arc::clone(&deps_active),
                    maximum: Arc::clone(&deps_maximum),
                }),
                store: MemoryStore::empty(),
                sink: Arc::new(|_: PushEvent, _: u64| {}),
                scope_source,
                token_source,
            })
        });
        let registry =
            PushRegistry::with_deps_factory(gfn, output, data_dir, registry_scope, deps_factory);
        Harness {
            registry,
            scope: scope_state,
            tokens,
            connect,
            connects,
            active,
            maximum,
            factories,
            http_calls,
        }
    }

    #[test]
    fn desired_push_requires_a_matching_provider_and_service_configuration() {
        let dir = config_dir("provider");
        assert!(desired_push(&dir, &scope(), "device").is_some());
        let mut other_provider = scope();
        other_provider.provider_id = "alliance".into();
        assert!(desired_push(&dir, &other_provider, "device").is_none());
        let missing_dir = config_dir("missing-client");
        let mut missing_client = raw_config();
        missing_client["pnsClientId"] = json!("");
        std::fs::write(
            missing_dir.join(crate::sources::gfn::push::PUSH_CONFIG_FILE),
            missing_client.to_string(),
        )
        .unwrap();
        assert!(desired_push(&missing_dir, &scope(), "device").is_none());
    }

    #[test]
    fn desired_push_uses_the_bundled_default_without_an_override() {
        let dir = empty_dir("bundled-default");
        let desired = desired_push(&dir, &scope(), "device").expect("the bundled default applies");
        assert_eq!(desired.config.provider_id, "nvidia");
        assert_eq!(desired.config.device_id, "device");
        assert!(!desired.config.endpoints.pns.is_empty());
        assert!(!desired.config.endpoints.pns_client_id.is_empty());
        assert!(!desired.config.identity.api_key.is_empty());
        assert!(
            dir.read_dir().unwrap().next().is_none(),
            "the bundled default must never write to the data directory"
        );
        let mut other_provider = scope();
        other_provider.provider_id = "alliance".into();
        let switched = desired_push(&dir, &other_provider, "device")
            .expect("the bundled default follows the authenticated provider");
        assert_eq!(switched.config.provider_id, "alliance");
        assert_eq!(
            desired.config.identity, switched.config.identity,
            "the bundled public identity is provider independent"
        );
    }

    #[test]
    fn desired_push_config_changes_with_device_and_configuration() {
        let dir = config_dir("key");
        let first = desired_push(&dir, &scope(), "device").unwrap();
        assert_eq!(
            first.config,
            desired_push(&dir, &scope(), "device").unwrap().config
        );
        let other_device = desired_push(&dir, &scope(), "other").unwrap();
        assert_ne!(first.config, other_device.config);
        let mut changed = raw_config();
        changed["pnsServer"] = json!("https://other.example");
        std::fs::write(
            dir.join(crate::sources::gfn::push::PUSH_CONFIG_FILE),
            changed.to_string(),
        )
        .unwrap();
        assert_ne!(
            first.config,
            desired_push(&dir, &scope(), "device").unwrap().config
        );
        let mut rotated = raw_config();
        rotated["apiKey"] = json!("rotated-api-key");
        std::fs::write(
            dir.join(crate::sources::gfn::push::PUSH_CONFIG_FILE),
            rotated.to_string(),
        )
        .unwrap();
        assert_ne!(
            first.config,
            desired_push(&dir, &scope(), "device").unwrap().config,
            "a rotated identity key must change the desired configuration"
        );
    }

    #[test]
    fn registry_propagates_identity_changes_across_the_same_owner() {
        let dir = config_dir("identity-churn");
        let mut harness = harness(dir.clone(), true);
        harness.registry.reconcile().unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while harness.tokens.lock().unwrap().is_empty() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut rotated = raw_config();
        rotated["apiKey"] = json!("rotated-api-key");
        std::fs::write(
            dir.join(crate::sources::gfn::push::PUSH_CONFIG_FILE),
            rotated.to_string(),
        )
        .unwrap();
        harness.registry.reconcile().unwrap();
        let owner_config = harness.registry.owner.as_ref().unwrap().config().0;
        assert_eq!(
            owner_config.identity.api_key, "rotated-api-key",
            "a changed identity must reach the live owner"
        );
        assert_eq!(
            harness.factories.load(Ordering::SeqCst),
            1,
            "the configuration change must reuse the single owner"
        );
        harness.connect.release();
        harness.registry.stop();
    }

    #[test]
    fn registry_creates_no_owner_without_a_signed_in_scope() {
        let mut harness = harness(config_dir("no-scope"), true);
        *harness.scope.lock().unwrap() = None;
        harness.registry.reconcile().unwrap();
        harness.registry.reconcile().unwrap();
        assert_eq!(harness.factories.load(Ordering::SeqCst), 0);
        assert!(harness.registry.owner.is_none());
        harness.registry.stop();
    }

    #[test]
    fn registry_stays_inactive_without_an_authenticated_scope() {
        let mut harness = harness(empty_dir("no-auth"), true);
        *harness.scope.lock().unwrap() = None;
        harness.registry.reconcile().unwrap();
        assert!(
            harness.registry.owner.is_none(),
            "the bundled default must not activate while signed out"
        );
        assert_eq!(harness.factories.load(Ordering::SeqCst), 0);
        *harness.scope.lock().unwrap() = Some(scope());
        harness.registry.reconcile().unwrap();
        assert!(
            harness.registry.owner.is_some(),
            "signing in must activate the bundled default"
        );
        *harness.scope.lock().unwrap() = None;
        harness.registry.reconcile().unwrap();
        assert!(!harness.registry.owner.as_ref().unwrap().is_running());
        harness.connect.release();
        harness.registry.stop();
    }

    #[test]
    fn registry_activates_the_bundled_default_without_an_override_file() {
        let mut harness = harness(empty_dir("bundled-activation"), true);
        harness.registry.reconcile().unwrap();
        let config = harness
            .registry
            .owner
            .as_ref()
            .expect("the bundled default must activate the owner")
            .config()
            .0;
        assert_eq!(config.provider_id, "nvidia");
        assert!(!config.identity.api_key.is_empty());
        assert!(!config.endpoints.pns.is_empty());
        assert_eq!(harness.factories.load(Ordering::SeqCst), 1);
        harness.connect.release();
        harness.registry.stop();
    }

    #[test]
    fn registry_prefers_an_explicit_override_over_the_bundled_default() {
        let mut harness = harness(config_dir("override-wins"), true);
        harness.registry.reconcile().unwrap();
        let config = harness
            .registry
            .owner
            .as_ref()
            .expect("the override must activate the owner")
            .config()
            .0;
        assert_eq!(config.identity.api_key, "api-key");
        assert_eq!(config.endpoints.pns, "https://pns.example/v1");
        harness.connect.release();
        harness.registry.stop();
    }

    #[test]
    fn desired_push_rejects_an_insecure_override_base() {
        let dir = empty_dir("insecure-base");
        let mut config = raw_config();
        config["pnsServer"] = json!("http://pns.example");
        std::fs::write(
            dir.join(crate::sources::gfn::push::PUSH_CONFIG_FILE),
            config.to_string(),
        )
        .unwrap();
        assert!(
            desired_push(&dir, &scope(), "device").is_none(),
            "an insecure PNS override must not activate a subscription"
        );
    }

    #[cfg(unix)]
    #[test]
    fn desired_push_never_activates_a_dangling_override_link() {
        let dir = empty_dir("dangling-link");
        std::os::unix::fs::symlink(
            dir.join("missing.json"),
            dir.join(crate::sources::gfn::push::PUSH_CONFIG_FILE),
        )
        .unwrap();
        assert!(
            desired_push(&dir, &scope(), "device").is_none(),
            "a dangling override link must suppress the bundled default"
        );
    }

    #[cfg(unix)]
    #[test]
    fn desired_push_returns_without_a_writer_on_a_non_regular_override() {
        use std::os::unix::ffi::OsStrExt as _;
        let dir = empty_dir("fifo");
        let path = dir.join(crate::sources::gfn::push::PUSH_CONFIG_FILE);
        let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(desired_push(&dir, &scope(), "device").is_none());
        });
        assert!(
            receiver
                .recv_timeout(Duration::from_secs(2))
                .expect("a non-regular override entry must not block reconciliation"),
            "a non-regular override must suppress the bundled default without a writer"
        );
    }

    #[test]
    fn registry_respects_an_explicitly_disabled_override() {
        let dir = empty_dir("disabled");
        std::fs::write(
            dir.join(crate::sources::gfn::push::PUSH_CONFIG_FILE),
            json!({"enabled": false}).to_string(),
        )
        .unwrap();
        let mut harness = harness(dir, true);
        harness.registry.reconcile().unwrap();
        assert!(
            harness.registry.owner.is_none(),
            "an explicit disable must not fall back to the bundled default"
        );
        assert_eq!(harness.factories.load(Ordering::SeqCst), 0);
        harness.registry.stop();
    }

    #[test]
    fn registry_binds_the_bundled_default_to_the_authenticated_provider() {
        let mut harness = harness(empty_dir("provider-change"), true);
        harness.registry.reconcile().unwrap();
        assert_eq!(
            harness
                .registry
                .owner
                .as_ref()
                .unwrap()
                .config()
                .0
                .provider_id,
            "nvidia"
        );
        let mut switched = scope();
        switched.provider_id = "alliance".into();
        switched.generation = 4;
        *harness.scope.lock().unwrap() = Some(switched);
        harness.registry.reconcile().unwrap();
        let config = harness.registry.owner.as_ref().unwrap().config().0;
        assert_eq!(config.provider_id, "alliance");
        assert_eq!(
            harness.factories.load(Ordering::SeqCst),
            1,
            "a provider change must reuse the single owner"
        );
        harness.connect.release();
        harness.registry.stop();
    }

    #[test]
    fn registry_reuses_one_owner_across_logout_and_login_cycles() {
        let mut harness = harness(config_dir("cycles"), true);
        harness.registry.reconcile().unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while harness.connects.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            harness.connects.load(Ordering::SeqCst) >= 1,
            "the worker must connect"
        );
        assert_eq!(harness.factories.load(Ordering::SeqCst), 1);
        for _ in 0..12 {
            *harness.scope.lock().unwrap() = None;
            harness.registry.reconcile().unwrap();
            *harness.scope.lock().unwrap() = Some(scope());
            harness.registry.reconcile().unwrap();
        }
        assert_eq!(
            harness.factories.load(Ordering::SeqCst),
            1,
            "a logout/login cycle must not create a second owner"
        );
        assert!(workers(&harness) <= 1);
        assert!(harness.maximum.load(Ordering::SeqCst) <= 1);
        harness.connect.release();
        let deadline = Instant::now() + Duration::from_secs(3);
        while workers(&harness) != 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        harness.registry.stop();
        assert_eq!(harness.active.load(Ordering::SeqCst), 0);
        assert!(harness.maximum.load(Ordering::SeqCst) <= 1);
    }

    #[test]
    fn registry_never_registers_a_new_provider_with_the_old_config() {
        let dir = config_dir("switch");
        let mut harness = harness(dir.clone(), true);
        harness.registry.reconcile().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while harness.connects.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            harness.connects.load(Ordering::SeqCst) >= 1,
            "the nvidia session connects only after its registration finishes"
        );
        assert_eq!(
            harness.tokens.lock().unwrap().as_slice(),
            &[scope()],
            "the nvidia account registers"
        );
        let http_before = harness.http_calls.load(Ordering::SeqCst);
        assert_eq!(
            http_before, 5,
            "a completed nvidia registration runs every registration request"
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while !harness.registry.owner.as_ref().unwrap().is_running() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            harness.registry.owner.as_ref().unwrap().is_running(),
            "the nvidia session is live before the provider switches"
        );
        let mut switched = scope();
        switched.provider_id = "alliance".into();
        switched.generation = 4;
        *harness.scope.lock().unwrap() = Some(switched.clone());
        let deadline = Instant::now() + Duration::from_secs(5);
        while harness.registry.owner.as_ref().unwrap().is_running() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            !harness.registry.owner.as_ref().unwrap().is_running(),
            "the retired scope must end the nvidia session"
        );
        assert!(
            !harness
                .tokens
                .lock()
                .unwrap()
                .iter()
                .any(|seen| seen.provider_id == "alliance"),
            "the old provider config must never request the new account's token"
        );
        assert_eq!(
            harness.http_calls.load(Ordering::SeqCst),
            http_before,
            "the old provider config must not run registration HTTP for the new provider"
        );
        assert_eq!(harness.factories.load(Ordering::SeqCst), 1);
        *harness.scope.lock().unwrap() = None;
        harness.registry.reconcile().unwrap();
        harness.connect.release();
        harness.registry.stop();
    }

    #[test]
    fn registry_stops_a_blocked_registration_before_later_stages() {
        let mut harness = harness_with(config_dir("blocked-registration"), true, true);
        harness.registry.reconcile().unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while harness.http_calls.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            harness.http_calls.load(Ordering::SeqCst),
            1,
            "the first stage must start"
        );
        *harness.scope.lock().unwrap() = None;
        harness.registry.reconcile().unwrap();
        harness.connect.release();
        let deadline = Instant::now() + Duration::from_secs(3);
        while workers(&harness) != 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        harness.registry.stop();
        assert_eq!(
            harness.http_calls.load(Ordering::SeqCst),
            1,
            "no later registration stage may run after the scope retired"
        );
    }

    #[test]
    fn core_exit_signals_blocked_registration_without_waiting_or_restarting() {
        let mut harness = harness_with(config_dir("core-exit-registration"), true, true);
        harness.registry.reconcile().unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while harness.http_calls.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(harness.http_calls.load(Ordering::SeqCst), 1);
        let active_workers = harness
            .registry
            .owner
            .as_ref()
            .unwrap()
            .worker_count_probe();
        let signal = harness.registry.core_exit_signal();
        let started = Instant::now();
        harness.registry.stop_for_core_exit();
        assert!(started.elapsed() < Duration::from_millis(250));
        assert!(signal.load(Ordering::Acquire));
        assert!(harness.registry.owner.is_none());
        assert_eq!(
            active_workers(),
            1,
            "the fake HTTP operation is still blocked"
        );
        harness.registry.reconcile().unwrap();
        assert!(harness.registry.owner.is_none());
        harness.connect.release();
        let deadline = Instant::now() + Duration::from_secs(3);
        while active_workers() != 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(active_workers(), 0);
        assert_eq!(harness.http_calls.load(Ordering::SeqCst), 1);
        assert_eq!(harness.connects.load(Ordering::SeqCst), 0);
        assert_eq!(harness.factories.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn registry_sink_emits_the_documented_event_envelope() {
        let (output, rx) = std::sync::mpsc::channel();
        let gfn = Arc::new(GfnService::new(std::env::temp_dir()).unwrap());
        let registry = PushRegistry::new(gfn, output, std::env::temp_dir());
        let sink: PushSink = {
            let output = registry.output.clone();
            Arc::new(move |event, generation| {
                let _ = output.send(json!({
                    "type": "event",
                    "name": PUSH_EVENT_NAME,
                    "payload": push_event_payload(&event, generation),
                }));
            })
        };
        sink(
            PushEvent::Favorites {
                changed_ids: vec!["app-1".into()],
            },
            11,
        );
        let message = rx.recv().unwrap();
        assert_eq!(message["type"], "event");
        assert_eq!(message["name"], "account.push.changed");
        assert_eq!(message["payload"]["generation"], 11);
        assert_eq!(message["payload"]["kind"], "favorites");
    }
}
