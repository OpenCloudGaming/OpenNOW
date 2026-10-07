use super::*;
use crate::plugins::PluginManager;
use crate::settings::SettingsStore;
use crate::sources::contract::{
    BuiltinModule, CatalogSource, ProviderCompletion, ProviderSource, SessionOccupancy,
};
use crate::sources::journal::Phase;
use opennow_plugin_api::{
    BUILTIN_GFN_ID, CatalogPage, CatalogQuery, Coverage, PluginDescriptor, PluginState, PluginTrust,
};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;

const PRIVATE_AUTH_URL: &str = "https://login.example.invalid/authorize?code=fixture-private-grant";
const PRIVATE_MEDIA_SENTINEL: &str = "fixture-private-media-credential";

#[derive(Default)]
struct FixtureProvider {
    generation: AtomicU64,
    attempts: AtomicUsize,
    preparations: AtomicUsize,
    stops: AtomicUsize,
}

impl CatalogSource for FixtureProvider {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: source(),
            name: "Private flow fixture".into(),
            version: "1.0.0".into(),
            publisher: "Fixture".into(),
            description: "Local private-flow test fixture".into(),
            builtin: true,
            required: false,
            enabled: true,
            state: PluginState::Ready,
            capabilities: Vec::new(),
            trust: PluginTrust::Builtin,
            last_error: None,
        }
    }

    fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    fn catalog_page(&self, _: &CatalogQuery, _: &Cancellation) -> Result<CatalogPage, SourceError> {
        Ok(CatalogPage {
            items: Vec::new(),
            next_cursor: None,
            coverage: Coverage::Unknown,
        })
    }
}

impl ProviderSource for FixtureProvider {
    fn provider_capabilities(&self) -> Vec<api::Capability> {
        vec![
            api::Capability::AuthBrowser,
            api::Capability::Accounts,
            api::Capability::Sessions,
        ]
    }

    fn auth_kinds(&self) -> Vec<api::AuthKind> {
        vec![api::AuthKind::Browser]
    }

    fn provider_call(
        &self,
        request: &api::ProviderRequest,
        _: &ProviderContext<'_>,
    ) -> ProviderCompletion {
        let reply = match request {
            api::ProviderRequest::AuthBegin(_) => {
                let sequence = self.attempts.fetch_add(1, Ordering::AcqRel) + 1;
                api::ProviderReply::AuthBegin(browser_state(&format!("attempt-{sequence}")))
            }
            api::ProviderRequest::AuthPoll(request) => {
                api::ProviderReply::AuthPoll(browser_state(request.attempt.as_str()))
            }
            api::ProviderRequest::AuthComplete(request) => {
                api::ProviderReply::AuthComplete(api::AuthState::Authorized {
                    attempt: request.attempt.clone(),
                })
            }
            api::ProviderRequest::AuthCancel(_) => api::ProviderReply::AuthCancel(api::Empty {}),
            api::ProviderRequest::AuthStatus(_) => {
                api::ProviderReply::AuthStatus(api::AuthState::SignedOut)
            }
            api::ProviderRequest::SessionStop(_) => {
                self.stops.fetch_add(1, Ordering::AcqRel);
                api::ProviderReply::SessionStop(api::CleanupState::Resolved)
            }
            _ => {
                return ProviderCompletion::not_dispatched(SourceError::new(
                    "unsupported_feature",
                    "The fixture does not implement this operation",
                ));
            }
        };
        ProviderCompletion::reply(reply)
    }

    fn prepare_native(
        &self,
        request: &api::PrepareSession,
        _: &ProviderContext<'_>,
    ) -> Result<NativePreparation, SourceError> {
        assert_eq!(request.session, session());
        self.preparations.fetch_add(1, Ordering::AcqRel);
        Ok(NativePreparation::Gfn(json!({"context":{
            "session":{"sessionId":request.session.remote_id,"streamingToken":PRIVATE_MEDIA_SENTINEL},
            "settings":{"codec":"H265"}
        }})))
    }
}

impl BuiltinModule for FixtureProvider {
    fn routes(&self) -> &'static [&'static str] {
        &[]
    }
    fn core_capabilities(&self) -> &'static [&'static str] {
        &[]
    }
    fn dispatch(&self, _: &str, _: &Value, _: &Cancellation) -> Option<Completion> {
        None
    }
    fn reporting_identity(&self) -> Value {
        Value::Null
    }
    fn session_occupancy(&self) -> SessionOccupancy {
        SessionOccupancy::Idle
    }
    fn settings_changed(&self) {}
    fn shutdown(&self) {}
    fn set_enabled(&self, _: bool) {}
    fn legacy_control_session(&self, _: &Value) -> Result<Option<api::SessionKey>, SourceError> {
        Ok(None)
    }
    fn legacy_account(&self, _params: &Value) -> Result<api::AccountKey, SourceError> {
        Ok(session().account.unwrap())
    }
    fn legacy_session_result(
        &self,
        _: &str,
        _: &Value,
        _: &Value,
    ) -> Result<Option<(api::SessionKey, bool)>, SourceError> {
        Ok(None)
    }
    fn apply_profile(&self, base: &Value, _: &api::StreamPreferences) -> Value {
        base.clone()
    }
}

struct Fixture {
    host: SourceHost,
    provider: Arc<FixtureProvider>,
    directory: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let provider = Arc::new(FixtureProvider::default());
        provider.generation.store(1, Ordering::Release);
        Self {
            host: host(directory.path(), Arc::clone(&provider)),
            provider,
            directory,
        }
    }

    fn restart(&mut self) {
        self.host.shutdown();
        self.host = host(self.directory.path(), Arc::clone(&self.provider));
    }

    fn active_session(&self) -> api::OperationId {
        let operation = api::OperationId::new("host-owned-session").unwrap();
        self.host
            .sessions
            .journal
            .reconcile(
                source(),
                session().account.clone(),
                operation.clone(),
                session(),
            )
            .unwrap();
        operation
    }

    fn browser(&self) -> Value {
        public_value(self.host.dispatch_sources(
            "sources.auth.start",
            &json!({
                "sourceId":source(),"request":{"authority":null,"kind":"browser","remember":false}
            }),
            &Cancellation::default(),
        ))
    }

    fn prepare(&self, operation: &api::OperationId, offer: &NativeOffer) -> Completion {
        self.host
            .dispatch_private(
                "streamer.source.prepare",
                &json!({
                    "sessionHandle":operation,"offer":offer,"runtimeCapabilities":null
                }),
                &Cancellation::default(),
            )
            .unwrap()
    }

    fn reconcile(&self, params: &Value) -> Completion {
        let request = if params.get("status").is_some() {
            params.clone()
        } else {
            json!({"mediaRevision":self.host.sessions.journal.media_revision().unwrap(),"status":params})
        };
        self.host
            .dispatch_private(
                "streamer.source.reconcile",
                &request,
                &Cancellation::default(),
            )
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.host.shutdown();
    }
}

fn host(path: &std::path::Path, provider: Arc<FixtureProvider>) -> SourceHost {
    let (output, _) = mpsc::channel();
    let plugins = Arc::new(PluginManager::open(path, output.clone()).unwrap());
    let settings = Arc::new(Mutex::new(SettingsStore::load(Some(path.into())).unwrap()));
    SourceHost::new(provider, plugins, path, output, settings).unwrap()
}

fn source() -> PluginId {
    PluginId::new(BUILTIN_GFN_ID).unwrap()
}

fn session() -> api::SessionKey {
    api::SessionKey {
        account: Some(api::AccountKey {
            authority: api::AuthorityId::new("fixture-authority").unwrap(),
            account: api::AccountId::new("fixture-account").unwrap(),
        }),
        remote_id: api::SessionId::new("remote-seat").unwrap(),
    }
}

fn browser_state(attempt: &str) -> api::AuthState {
    api::AuthState::Pending {
        challenge: api::AuthChallenge::Browser {
            attempt: api::AttemptId::new(attempt).unwrap(),
            authorization: api::SecretString::new(PRIVATE_AUTH_URL).unwrap(),
            expires_at_ms: now_ms() + 60_000,
            poll_after_ms: 1000,
        },
    }
}

fn offer() -> NativeOffer {
    serde_json::from_value(json!({"version":1,"offerId":"native-offer","runtimeEpoch":41,"expiresAtMs":now_ms()+60_000,
        "videoFormats":[{"encoding":"hevc-annex-b","bitDepth":8,"chroma":"yuv420","dynamicRange":"sdr","maxWidth":1920,"maxHeight":1080,"maxFps":60}],
        "audioFormats":[],"input":{"keyboard":true,"relativeMouse":true,"absoluteMouse":true,"text":false,"gamepadSlots":1,"rumble":false},
        "limits":{"maxVideoAccessUnitBytes":1048576,"maxAudioPacketBytes":65536,"maxControlMessageBytes":65536,"maxBufferedVideoBytes":2097152,"maxBufferedVideoFrames":4,"maxBufferedAudioMs":100,"maxPendingInputEvents":128}
    })).unwrap()
}

fn public_value(completion: Completion) -> Value {
    completion.result.unwrap().0
}

fn open(host: &SourceHost, source: &PluginId, handle: &str) -> Completion {
    host.dispatch_private(
        "sources.auth.open",
        &json!({"sourceId":source,"openHandle":handle}),
        &Cancellation::default(),
    )
    .unwrap()
}

fn browser_handle(value: &Value) -> &str {
    value["result"]["challenge"]["openHandle"].as_str().unwrap()
}

fn assert_private_absent(value: &Value) {
    let encoded = value.to_string();
    assert!(!encoded.contains(PRIVATE_AUTH_URL));
    assert!(!encoded.contains("fixture-private-grant"));
    assert!(!encoded.contains(PRIVATE_MEDIA_SENTINEL));
    assert!(!encoded.contains("\"authorization\""));
    assert!(!encoded.contains("\"context\""));
}

fn native_binding(lease: &HostBoundPreparedLease) -> Value {
    json!({"runtimeEpoch":lease.runtime_epoch,"nativeIdle":false,"legacyActive":false,"active":{
        "sourceId":lease.source_id,"session":lease.session,"leaseId":lease.lease_id,"attemptId":lease.attempt_id,
        "runtimeEpoch":lease.runtime_epoch,"startId":"native-start","state":"streaming"
    }})
}

fn accept_preparation(fixture: &Fixture, operation: &api::OperationId) -> HostBoundPreparedLease {
    let offer = offer();
    let mut prepared = fixture.prepare(operation, &offer);
    let lease: HostBoundPreparedLease =
        serde_json::from_value(prepared.result.as_ref().unwrap().0.clone()).unwrap();
    lease.validate_against(&offer, now_ms()).unwrap();
    prepared
        .receipt
        .take()
        .unwrap()
        .settle(true)
        .result
        .unwrap();
    lease
}

#[test]
fn browser_authorization_is_projected_to_a_single_use_private_handle() {
    let fixture = Fixture::new();
    let public = fixture.browser();
    assert_private_absent(&public);
    assert_eq!(public["result"]["challenge"]["kind"], "browser");
    let handle = browser_handle(&public);
    let opened = public_value(open(&fixture.host, &source(), handle));
    assert_eq!(opened["url"], PRIVATE_AUTH_URL);
    assert_eq!(opened["attempt"], public["result"]["challenge"]["attempt"]);
    assert_eq!(
        open(&fixture.host, &source(), handle).result.unwrap_err().0,
        "session_owner_mismatch"
    );
}

#[test]
fn browser_handle_is_bound_to_source_and_provider_generation() {
    let fixture = Fixture::new();
    let public = fixture.browser();
    let wrong = PluginId::new("org.example.other-provider").unwrap();
    assert_eq!(
        open(&fixture.host, &wrong, browser_handle(&public))
            .result
            .unwrap_err()
            .0,
        "session_owner_mismatch"
    );
    assert!(
        open(&fixture.host, &source(), browser_handle(&public))
            .result
            .is_err()
    );
    let current = fixture.browser();
    fixture.provider.generation.fetch_add(1, Ordering::AcqRel);
    assert_eq!(
        open(&fixture.host, &source(), browser_handle(&current))
            .result
            .unwrap_err()
            .0,
        "session_owner_mismatch"
    );
}

#[test]
fn replacement_attempt_and_explicit_completion_invalidate_browser_handles() {
    let fixture = Fixture::new();
    let first = fixture.browser();
    let second = fixture.browser();
    assert_ne!(
        first["result"]["challenge"]["attempt"],
        second["result"]["challenge"]["attempt"]
    );
    assert!(
        open(&fixture.host, &source(), browser_handle(&first))
            .result
            .is_err()
    );
    let completed = public_value(fixture.host.dispatch_sources("sources.auth.complete", &json!({
        "sourceId":source(),"request":{"attempt":second["result"]["challenge"]["attempt"],"proof":null}
    }), &Cancellation::default()));
    assert_eq!(completed["result"]["state"], "authorized");
    assert_private_absent(&completed);
    assert!(
        open(&fixture.host, &source(), browser_handle(&second))
            .result
            .is_err()
    );
}

#[test]
fn polling_the_same_browser_attempt_reuses_its_handle_until_source_selection() {
    let fixture = Fixture::new();
    let first = fixture.browser();
    let poll = public_value(fixture.host.dispatch_sources(
        "sources.auth.poll",
        &json!({
            "sourceId":source(),"request":{"attempt":first["result"]["challenge"]["attempt"]}
        }),
        &Cancellation::default(),
    ));
    assert_private_absent(&poll);
    assert_eq!(browser_handle(&first), browser_handle(&poll));
    fixture
        .host
        .dispatch_sources(
            "sources.select",
            &json!({"sourceId":source()}),
            &Cancellation::default(),
        )
        .result
        .unwrap();
    assert!(
        open(&fixture.host, &source(), browser_handle(&poll))
            .result
            .is_err()
    );
}

#[test]
fn canonical_gfn_lease_is_private_and_rejected_preparation_only_retires_its_pin() {
    let fixture = Fixture::new();
    let operation = fixture.active_session();
    let offered = offer();
    let mut prepared = fixture.prepare(&operation, &offered);
    let lease: HostBoundPreparedLease =
        serde_json::from_value(prepared.result.as_ref().unwrap().0.clone()).unwrap();
    lease.validate_against(&offered, now_ms()).unwrap();
    assert_eq!(lease.session, session());
    let PreparedMedia::Gfn { context } = &lease.media else {
        panic!("expected private GFN context");
    };
    assert_eq!(context["session"]["streamingToken"], PRIVATE_MEDIA_SENTINEL);
    assert!(!format!("{lease:?}").contains(PRIVATE_MEDIA_SENTINEL));
    let public = public_value(fixture.host.dispatch_sources(
        "sources.session.current",
        &json!({}),
        &Cancellation::default(),
    ));
    assert_eq!(public["session"]["sessionHandle"], operation.as_str());
    assert_private_absent(&public);
    let public_prepare = fixture.host.dispatch_sources(
        "streamer.source.prepare",
        &json!({"sourceId":source(),"request":{"sessionHandle":operation,"offer":offered}}),
        &Cancellation::default(),
    );
    assert_eq!(public_prepare.result.unwrap_err().0, "method_not_found");
    assert!(
        fixture
            .host
            .sessions
            .journal
            .snapshot()
            .unwrap()
            .unwrap()
            .media
            .is_some()
    );
    prepared
        .receipt
        .take()
        .unwrap()
        .settle(false)
        .result
        .unwrap();
    let retained = fixture.host.sessions.journal.snapshot().unwrap().unwrap();
    assert_eq!(retained.operation, operation);
    assert_eq!(retained.phase, Phase::Active);
    assert!(retained.media.is_none());
    assert_eq!(fixture.provider.preparations.load(Ordering::Acquire), 1);
    assert_eq!(fixture.provider.stops.load(Ordering::Acquire), 0);
    assert_eq!(fixture.host.sessions.occupancy(), SessionOccupancy::InUse);
}

#[test]
fn healthy_exact_native_binding_survives_core_restart_without_preparing_again() {
    let mut fixture = Fixture::new();
    let operation = fixture.active_session();
    let lease = accept_preparation(&fixture, &operation);
    let before = fixture.host.sessions.journal.snapshot().unwrap();
    fixture.restart();
    let attached = public_value(fixture.reconcile(&native_binding(&lease)));
    assert_eq!(attached["attached"], true);
    assert_eq!(attached["sessionHandle"], operation.as_str());
    assert_eq!(attached["leaseId"], lease.lease_id.as_str());
    assert_private_absent(&attached);
    assert_eq!(fixture.host.sessions.journal.snapshot().unwrap(), before);
    assert_eq!(fixture.provider.preparations.load(Ordering::Acquire), 1);
    assert_eq!(fixture.provider.stops.load(Ordering::Acquire), 0);
}

#[test]
fn native_binding_cannot_replace_the_original_source_account_session_or_attempt() {
    let fixture = Fixture::new();
    let operation = fixture.active_session();
    let lease = accept_preparation(&fixture, &operation);
    let before = fixture.host.sessions.journal.snapshot().unwrap();
    for pointer in [
        "/active/sourceId",
        "/active/session/account/account",
        "/active/session/remoteId",
        "/active/leaseId",
        "/active/attemptId",
    ] {
        let mut changed = native_binding(&lease);
        *changed.pointer_mut(pointer).unwrap() = json!(if pointer.ends_with("sourceId") {
            "org.example.other-provider"
        } else {
            "foreign-binding"
        });
        assert_eq!(
            fixture.reconcile(&changed).result.unwrap_err().0,
            "session_owner_mismatch",
            "{pointer}"
        );
        assert_eq!(fixture.host.sessions.journal.snapshot().unwrap(), before);
    }
    let mut epoch = native_binding(&lease);
    epoch["runtimeEpoch"] = json!(lease.runtime_epoch + 1);
    assert!(fixture.reconcile(&epoch).result.is_err());
    assert_eq!(fixture.provider.preparations.load(Ordering::Acquire), 1);
}

#[test]
fn missing_native_binding_is_not_idle_and_explicit_idle_releases_ended_media() {
    let fixture = Fixture::new();
    let operation = fixture.active_session();
    let lease = accept_preparation(&fixture, &operation);
    fixture
        .host
        .sessions
        .journal
        .remote_ended(&source(), &session())
        .unwrap();
    let before = fixture.host.sessions.journal.snapshot().unwrap();
    let unresolved = public_value(fixture.reconcile(&json!({"runtimeEpoch":lease.runtime_epoch,"nativeIdle":false,"legacyActive":false,"active":null})));
    assert_eq!(unresolved["nativeIdle"], false);
    assert_eq!(fixture.host.sessions.journal.snapshot().unwrap(), before);
    assert_eq!(fixture.host.sessions.occupancy(), SessionOccupancy::InUse);
    let released = public_value(fixture.reconcile(&json!({"runtimeEpoch":lease.runtime_epoch,"nativeIdle":true,"legacyActive":false,"active":null})));
    assert_eq!(released["nativeIdle"], true);
    assert!(fixture.host.sessions.journal.snapshot().unwrap().is_none());
    assert_eq!(fixture.host.sessions.occupancy(), SessionOccupancy::Idle);
    assert_eq!(fixture.provider.preparations.load(Ordering::Acquire), 1);
    assert_eq!(fixture.provider.stops.load(Ordering::Acquire), 0);
}

#[test]
fn legacy_media_obligation_survives_absent_binding_until_explicit_native_idle() {
    let mut fixture = Fixture::new();
    fixture.active_session();
    fixture
        .host
        .sessions
        .journal
        .mark_legacy_media(&source(), &session())
        .unwrap();
    fixture
        .host
        .sessions
        .journal
        .remote_ended(&source(), &session())
        .unwrap();
    fixture.restart();
    let before = fixture.host.sessions.journal.snapshot().unwrap();
    assert_eq!(before.as_ref().unwrap().phase, Phase::RemoteEnded);
    assert!(before.as_ref().unwrap().media.is_none());
    fixture
        .reconcile(
            &json!({"runtimeEpoch":41,"nativeIdle":false,"legacyActive":false,"active":null}),
        )
        .result
        .unwrap();
    assert_eq!(fixture.host.sessions.journal.snapshot().unwrap(), before);
    assert_eq!(fixture.host.sessions.occupancy(), SessionOccupancy::InUse);
    fixture
        .reconcile(&json!({"runtimeEpoch":41,"nativeIdle":true,"legacyActive":false,"active":null}))
        .result
        .unwrap();
    assert!(fixture.host.sessions.journal.snapshot().unwrap().is_none());
    assert_eq!(fixture.host.sessions.occupancy(), SessionOccupancy::Idle);
    assert_eq!(fixture.provider.preparations.load(Ordering::Acquire), 0);
    assert_eq!(fixture.provider.stops.load(Ordering::Acquire), 0);
}
