use crate::sources::gfn::push::MessageType;
use crate::sources::gfn::push::PushError;
use crate::sources::gfn::push::decrypt::{EceKeyPair, decrypt_web_push};
use crate::sources::gfn::push::protocol::{
    Frame, FrameReader, decode_frame, encode_close, encode_heartbeat_ack, encode_heartbeat_ping,
    encode_login_request, encode_stream_ack,
};
use crate::sources::gfn::push::registration::{
    DEVICE_IDENTITY_REJECTED, PushEndpoints, PushHttp, PushIdentity, Registration,
    RegistrationClient, checkin_and_register,
};
use crate::sources::gfn::push::store::PushStateStore;
use crate::sources::gfn::push::transport::PushTransportFactory;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const SESSION_SLICE: Duration = Duration::from_millis(250);
const PAUSED_SLICE: Duration = Duration::from_secs(1);
const STORED_REGISTRATION_RETIRED: &str = "push_registration_retired";
const REGISTRATION_MAXIMUM_AGE_SECONDS: u64 = 7 * 24 * 60 * 60;
const UNACKNOWLEDGED_PERSISTENT_STREAM_ACK: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushScope {
    pub user_id: String,
    pub provider_id: String,
    pub generation: u64,
}

impl PushScope {
    pub fn account_key(&self) -> String {
        format!("{}:{}", self.provider_id, self.user_id)
    }

    pub(crate) fn same_account(&self, other: &PushScope) -> bool {
        self.user_id == other.user_id && self.provider_id == other.provider_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushEvent {
    Library {
        changed_ids: Vec<String>,
    },
    Favorites {
        changed_ids: Vec<String>,
    },
    Subscription {
        changed_ids: Vec<String>,
    },
    LinkedAccount {
        account_type: Option<String>,
        linked: Option<bool>,
    },
    PlatformSync {
        platform_code: Option<String>,
        sync_state: Option<String>,
        sync_date: Option<String>,
        sync_game_count: Option<u64>,
    },
}

pub type PushSink = Arc<dyn Fn(PushEvent, u64) + Send + Sync>;
pub type ScopeSource = Arc<dyn Fn() -> Option<PushScope> + Send + Sync>;
pub type GfnTokenSource = Arc<dyn Fn(&PushScope) -> Option<String> + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushOwnerConfig {
    pub endpoints: PushEndpoints,
    pub identity: PushIdentity,
    pub provider_id: String,
    pub device_id: String,
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
    pub login_timeout: Duration,
    pub heartbeat_interval: Duration,
    pub heartbeat_ack_timeout: Duration,
    pub minimum_heartbeat_interval: Duration,
    pub maximum_heartbeat_interval: Duration,
    pub coalescing_window: Duration,
    pub backoff_initial: Duration,
    pub backoff_maximum: Duration,
    pub stop_deadline: Duration,
    pub maximum_frame_bytes: usize,
    pub maximum_changed_ids: usize,
    pub maximum_persistent_ids: usize,
    pub registration_maximum_age: Duration,
    pub registration_timeout: Duration,
}

impl PushOwnerConfig {
    pub fn bounded(
        endpoints: PushEndpoints,
        identity: PushIdentity,
        provider_id: String,
        device_id: String,
    ) -> Self {
        Self {
            endpoints,
            identity,
            provider_id,
            device_id,
            connect_timeout: Duration::from_secs(20),
            read_timeout: Duration::from_secs(5),
            login_timeout: Duration::from_secs(20),
            heartbeat_interval: Duration::from_secs(600),
            heartbeat_ack_timeout: Duration::from_secs(60),
            minimum_heartbeat_interval: Duration::from_secs(30),
            maximum_heartbeat_interval: Duration::from_secs(28 * 60),
            coalescing_window: Duration::from_secs(2),
            backoff_initial: Duration::from_secs(2),
            backoff_maximum: Duration::from_secs(60),
            stop_deadline: Duration::from_secs(5),
            maximum_frame_bytes: crate::sources::gfn::push::MAXIMUM_FRAME_BYTES,
            maximum_changed_ids: crate::sources::gfn::push::MAXIMUM_CHANGED_IDS,
            maximum_persistent_ids: 32,
            registration_maximum_age: Duration::from_secs(REGISTRATION_MAXIMUM_AGE_SECONDS),
            registration_timeout: Duration::from_secs(10),
        }
    }

    pub(crate) fn registration_fingerprint(&self, scope: &PushScope) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(b"opennow-push-registration\0");
        hasher.update(scope.account_key().as_bytes());
        hasher.update(b"\0");
        hasher.update(self.device_id.as_bytes());
        hasher.update(b"\0");
        hasher.update(self.identity.project_id.as_bytes());
        hasher.update(b"\0");
        hasher.update(self.identity.api_key.as_bytes());
        hasher.update(b"\0");
        hasher.update(self.identity.sender_id.as_bytes());
        hasher.update(b"\0");
        hasher.update(self.identity.app_id.as_bytes());
        hasher.update(b"\0");
        hasher.update(self.identity.firebase_app_id.as_bytes());
        hasher.update(b"\0");
        hasher.update(
            self.identity
                .vapid_key
                .as_deref()
                .unwrap_or_default()
                .as_bytes(),
        );
        hasher.update(b"\0");
        hasher.update(self.endpoints.pns.as_bytes());
        hasher.update(b"\0");
        hasher.update(self.endpoints.pns_client_id.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

pub struct PushOwnerDeps {
    pub http: Arc<dyn PushHttp>,
    pub transport: Arc<dyn PushTransportFactory>,
    pub store: Arc<dyn PushStateStore>,
    pub sink: PushSink,
    pub scope_source: ScopeSource,
    pub token_source: GfnTokenSource,
}

struct IntentState {
    active: Option<PushScope>,
    supervised: bool,
}

struct ConfigState {
    config: PushOwnerConfig,
    generation: u64,
}

struct Intent {
    state: Mutex<IntentState>,
    wake: Condvar,
    shutdown: AtomicBool,
    core_exit: Arc<AtomicBool>,
    paused: AtomicBool,
    workers: AtomicUsize,
    generation: AtomicU64,
    refresh_pending: AtomicBool,
    config: Mutex<ConfigState>,
}

impl Intent {
    fn new(config: PushOwnerConfig) -> Self {
        Self {
            state: Mutex::new(IntentState {
                active: None,
                supervised: false,
            }),
            wake: Condvar::new(),
            shutdown: AtomicBool::new(false),
            core_exit: Arc::new(AtomicBool::new(false)),
            paused: AtomicBool::new(false),
            workers: AtomicUsize::new(0),
            generation: AtomicU64::new(0),
            refresh_pending: AtomicBool::new(false),
            config: Mutex::new(ConfigState {
                config,
                generation: 1,
            }),
        }
    }

    fn config(&self) -> (PushOwnerConfig, u64) {
        let state = self.config.lock().expect("push config poisoned");
        (state.config.clone(), state.generation)
    }

    fn replace_config(&self, config: PushOwnerConfig) {
        {
            let mut state = self.config.lock().expect("push config poisoned");
            state.config = config;
            state.generation += 1;
        }
        self.wake.notify_all();
    }

    fn accepts_provider(&self, provider_id: &str) -> bool {
        self.config
            .lock()
            .expect("push config poisoned")
            .config
            .provider_id
            == provider_id
    }

    fn paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    fn request_registration_refresh(&self) {
        self.refresh_pending.store(true, Ordering::SeqCst);
    }

    fn take_registration_refresh(&self) -> bool {
        self.refresh_pending.swap(false, Ordering::SeqCst)
    }

    fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::SeqCst);
        self.wake.notify_all();
    }

    fn desired(&self, deps: &PushOwnerDeps) -> Option<PushScope> {
        if self.paused() {
            return None;
        }
        let scope = (deps.scope_source)()?;
        if scope.user_id.is_empty() || scope.provider_id.is_empty() {
            return None;
        }
        if !self.accepts_provider(&scope.provider_id) {
            return None;
        }
        self.generation.store(scope.generation, Ordering::SeqCst);
        Some(scope)
    }

    fn shutdown(&self) -> bool {
        self.shutdown.load(Ordering::SeqCst) || self.core_exit.load(Ordering::Acquire)
    }

    fn set_active(&self, active: Option<PushScope>) {
        self.state.lock().expect("push intent poisoned").active = active;
    }

    fn wait(&self, duration: Duration) -> bool {
        let state = self.state.lock().expect("push intent poisoned");
        if self.shutdown() {
            return false;
        }
        let (state, _) = self
            .wake
            .wait_timeout(state, duration)
            .expect("push intent poisoned");
        drop(state);
        !self.shutdown()
    }
}

pub struct PushOwner {
    deps: PushOwnerDeps,
    intent: Arc<Intent>,
    supervisor: Option<JoinHandle<()>>,
}

impl PushOwner {
    #[cfg(test)]
    pub fn new(config: PushOwnerConfig, deps: PushOwnerDeps) -> Self {
        Self::with_core_exit(config, deps, Arc::new(AtomicBool::new(false)))
    }

    pub fn with_core_exit(
        config: PushOwnerConfig,
        deps: PushOwnerDeps,
        core_exit: Arc<AtomicBool>,
    ) -> Self {
        let mut intent = Intent::new(config);
        intent.core_exit = core_exit;
        Self {
            deps,
            intent: Arc::new(intent),
            supervisor: None,
        }
    }

    pub fn update_config(&self, config: PushOwnerConfig) {
        self.intent.replace_config(config);
    }

    pub fn config(&self) -> (PushOwnerConfig, u64) {
        self.intent.config()
    }

    pub fn start(&mut self) -> Result<(), PushError> {
        if self.intent.core_exit.load(Ordering::Acquire) {
            return Err(PushError::new(
                "push_worker_stopped",
                "The push worker has been shut down",
            ));
        }
        if self.supervisor.is_some() {
            if self.intent.shutdown() {
                return Err(PushError::new(
                    "push_worker_stopped",
                    "The push worker has been shut down",
                ));
            }
            self.intent.set_paused(false);
            return Ok(());
        }
        let deps = PushOwnerDeps {
            http: Arc::clone(&self.deps.http),
            transport: Arc::clone(&self.deps.transport),
            store: Arc::clone(&self.deps.store),
            sink: Arc::clone(&self.deps.sink),
            scope_source: Arc::clone(&self.deps.scope_source),
            token_source: Arc::clone(&self.deps.token_source),
        };
        let intent = Arc::clone(&self.intent);
        intent.shutdown.store(false, Ordering::SeqCst);
        intent.paused.store(false, Ordering::SeqCst);
        intent.workers.store(1, Ordering::SeqCst);
        intent
            .state
            .lock()
            .expect("push intent poisoned")
            .supervised = true;
        let worker_intent = Arc::clone(&intent);
        let handle = thread::Builder::new()
            .name("opennow-push".to_owned())
            .spawn(move || {
                let _guard = WorkerGuard(Arc::clone(&worker_intent));
                run_supervisor(&deps, &worker_intent);
            })
            .map_err(|_| {
                intent.workers.store(0, Ordering::SeqCst);
                intent
                    .state
                    .lock()
                    .expect("push intent poisoned")
                    .supervised = false;
                PushError::new("push_worker_failed", "The push worker could not start")
            })?;
        self.supervisor = Some(handle);
        Ok(())
    }

    pub fn stop(&mut self) {
        self.intent.set_paused(true);
        self.intent.set_active(None);
    }

    pub fn shutdown(&mut self, timeout: Duration) -> bool {
        self.intent.shutdown.store(true, Ordering::SeqCst);
        self.intent.set_active(None);
        self.intent.wake.notify_all();
        let Some(handle) = self.supervisor.take() else {
            return true;
        };
        let deadline = Instant::now() + timeout;
        while !handle.is_finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        if handle.is_finished() {
            let _ = handle.join();
            true
        } else {
            false
        }
    }

    pub fn shutdown_for_core_exit(&mut self) {
        self.intent.core_exit.store(true, Ordering::Release);
        self.intent.wake.notify_all();
        self.supervisor.take();
    }

    #[cfg(test)]
    pub fn wait_idle(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while self.is_running() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        !self.is_running()
    }

    #[cfg(test)]
    pub fn is_supervised(&self) -> bool {
        self.intent
            .state
            .lock()
            .expect("push intent poisoned")
            .supervised
    }

    #[cfg(test)]
    pub fn is_running(&self) -> bool {
        self.is_supervised() && self.active_scope().is_some()
    }

    #[cfg(test)]
    pub fn active_scope(&self) -> Option<PushScope> {
        self.intent
            .state
            .lock()
            .expect("push intent poisoned")
            .active
            .clone()
    }

    #[cfg(test)]
    pub fn worker_count(&self) -> usize {
        self.intent.workers.load(Ordering::SeqCst)
    }

    #[cfg(test)]
    pub fn worker_count_probe(&self) -> Box<dyn Fn() -> usize + Send> {
        let intent = Arc::clone(&self.intent);
        Box::new(move || intent.workers.load(Ordering::SeqCst))
    }
}

impl Drop for PushOwner {
    fn drop(&mut self) {
        if self.intent.core_exit.load(Ordering::Acquire) {
            self.shutdown_for_core_exit();
            return;
        }
        let deadline = self.intent.config().0.stop_deadline;
        let _ = self.shutdown(deadline);
    }
}

struct WorkerGuard(Arc<Intent>);

impl Drop for WorkerGuard {
    fn drop(&mut self) {
        self.0.workers.store(0, Ordering::SeqCst);
        let mut state = self.0.state.lock().expect("push intent poisoned");
        state.supervised = false;
        state.active = None;
        drop(state);
        self.0.wake.notify_all();
    }
}

enum SessionOutcome {
    ScopeChanged,
    Completed,
    Failed,
    RegistrationRetired,
}

fn run_supervisor(deps: &PushOwnerDeps, intent: &Arc<Intent>) {
    let mut backoff = intent.config().0.backoff_initial;
    loop {
        if intent.shutdown() {
            return;
        }
        let (config, config_generation) = intent.config();
        let Some(scope) = intent.desired(deps) else {
            let idle = if intent.paused() {
                PAUSED_SLICE
            } else {
                SESSION_SLICE
            };
            if !intent.wait(idle) {
                return;
            }
            continue;
        };
        match run_scoped_session(&config, config_generation, deps, intent, &scope) {
            SessionOutcome::ScopeChanged => {
                backoff = config.backoff_initial;
            }
            SessionOutcome::Completed => {
                backoff = config.backoff_initial;
                if intent.shutdown() || !intent.wait(SESSION_SLICE) {
                    return;
                }
            }
            SessionOutcome::Failed | SessionOutcome::RegistrationRetired => {
                if intent.shutdown() || !intent.wait(backoff) {
                    return;
                }
                backoff = (backoff * 2).min(config.backoff_maximum);
            }
        }
    }
}

fn run_scoped_session(
    config: &PushOwnerConfig,
    config_generation: u64,
    deps: &PushOwnerDeps,
    intent: &Arc<Intent>,
    scope: &PushScope,
) -> SessionOutcome {
    let registration = match ensure_registration(config, config_generation, deps, intent, scope) {
        Ok(Some(registration)) => registration,
        Ok(None) => return SessionOutcome::ScopeChanged,
        Err(error) => return session_failure_outcome(&error),
    };
    match run_session(
        config,
        config_generation,
        deps,
        intent,
        scope,
        &registration,
    ) {
        Ok(true) => SessionOutcome::ScopeChanged,
        Ok(false) => SessionOutcome::Completed,
        Err(error) => session_failure_outcome(&error),
    }
}

fn session_failure_outcome(error: &PushError) -> SessionOutcome {
    if error.code == "push_scope_stale" {
        SessionOutcome::ScopeChanged
    } else if error.code == STORED_REGISTRATION_RETIRED {
        SessionOutcome::RegistrationRetired
    } else {
        SessionOutcome::Failed
    }
}

fn clamped_heartbeat_interval(config: &PushOwnerConfig, interval: Duration) -> Duration {
    interval.clamp(
        config.minimum_heartbeat_interval,
        config.maximum_heartbeat_interval,
    )
}

fn advance_advertised_stream_id(
    stream_id_in: u64,
    advertised: &mut u64,
    unacknowledged: &mut usize,
) {
    if stream_id_in > *advertised {
        *advertised = stream_id_in;
        *unacknowledged = 0;
    }
}

fn session_is_current(
    config: &PushOwnerConfig,
    config_generation: u64,
    deps: &PushOwnerDeps,
    intent: &Arc<Intent>,
    scope: &PushScope,
) -> bool {
    if intent.shutdown() {
        return false;
    }
    let (current, current_generation) = intent.config();
    if current_generation != config_generation
        || current.provider_id != config.provider_id
        || current.provider_id != scope.provider_id
    {
        return false;
    }
    intent
        .desired(deps)
        .is_some_and(|desired| desired.same_account(scope))
}

fn ensure_registration(
    config: &PushOwnerConfig,
    config_generation: u64,
    deps: &PushOwnerDeps,
    intent: &Arc<Intent>,
    scope: &PushScope,
) -> Result<Option<Registration>, PushError> {
    if !session_is_current(config, config_generation, deps, intent, scope) {
        return Ok(None);
    }
    let account = scope.account_key();
    let fingerprint = config.registration_fingerprint(scope);
    let refresh = intent.take_registration_refresh();
    if !refresh
        && let Some(registration) = deps.store.load(&account)?
        && registration_is_usable(&registration, &fingerprint, config)
    {
        return Ok(Some(registration));
    }
    let Some(token) = (deps.token_source)(scope) else {
        return Err(PushError::new(
            "push_token_unavailable",
            "The account token is not available for the push registration",
        ));
    };
    let existing = deps.store.load(&account)?;
    let client = RegistrationClient {
        http: deps.http.as_ref(),
        endpoints: &config.endpoints,
        timeout: config.registration_timeout,
    };
    let mut guard = || session_is_current(config, config_generation, deps, intent, scope);
    let mut registration = checkin_and_register(
        &client,
        &config.identity,
        existing.as_ref(),
        &token,
        &config.device_id,
        &mut guard,
    )
    .map_err(|error| {
        if error.code == DEVICE_IDENTITY_REJECTED {
            retire_registration(deps, scope, error.message.clone())
        } else {
            error
        }
    })?;
    registration.fingerprint = fingerprint;
    if !session_is_current(config, config_generation, deps, intent, scope) {
        return Ok(None);
    }
    deps.store.save(&account, &registration)?;
    Ok(Some(registration))
}

fn registration_is_usable(
    registration: &Registration,
    fingerprint: &str,
    config: &PushOwnerConfig,
) -> bool {
    if registration.fcm_token.is_empty()
        || registration.public_key.is_empty()
        || registration.private_key.is_empty()
        || registration.auth_secret.is_empty()
    {
        return false;
    }
    if EceKeyPair::from_parts(
        &registration.private_key,
        &registration.public_key,
        &registration.auth_secret,
    )
    .is_err()
    {
        return false;
    }
    if registration.fingerprint != fingerprint {
        return false;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    now.saturating_sub(registration.created_at_seconds) <= config.registration_maximum_age.as_secs()
}

fn run_session(
    config: &PushOwnerConfig,
    config_generation: u64,
    deps: &PushOwnerDeps,
    intent: &Arc<Intent>,
    scope: &PushScope,
    registration: &Registration,
) -> Result<bool, PushError> {
    if !session_is_current(config, config_generation, deps, intent, scope) {
        return Ok(true);
    }
    let mut transport = deps.transport.connect(
        &config.endpoints.mcs_host,
        config.endpoints.mcs_port,
        config.connect_timeout,
    )?;
    let login = encode_login_request(
        registration.android_id,
        registration.security_token,
        &registration.received_persistent_ids,
    );
    transport.send(&login, config.connect_timeout)?;
    intent.set_active(Some(scope.clone()));

    let mut reader = FrameReader::new();
    let mut pending: BTreeMap<MessageType, PendingEvent> = BTreeMap::new();
    let mut persistent_ids = registration.received_persistent_ids.clone();
    let mut stream_id_in: u64 = 0;
    let mut advertised_stream_id: u64 = 0;
    let mut unacknowledged_persistent: usize = 0;
    let mut heartbeat_interval = clamped_heartbeat_interval(config, config.heartbeat_interval);
    let mut next_heartbeat = Instant::now() + heartbeat_interval;
    let mut heartbeat_ack_deadline: Option<Instant> = None;
    let mut login_deadline = Some(Instant::now() + config.login_timeout);
    let keys = match EceKeyPair::from_parts(
        &registration.private_key,
        &registration.public_key,
        &registration.auth_secret,
    ) {
        Ok(keys) => keys,
        Err(_) => {
            return Err(retire_registration(
                deps,
                scope,
                "push_key_invalid".to_owned(),
            ));
        }
    };

    let result = (|| -> Result<bool, PushError> {
        loop {
            if !session_is_current(config, config_generation, deps, intent, scope) {
                return Ok(true);
            }
            flush_due_events(
                config,
                config_generation,
                deps,
                intent,
                scope,
                &mut pending,
                Instant::now(),
            );
            let now = Instant::now();
            if login_deadline.is_some_and(|deadline| now >= deadline) {
                return Err(PushError::new(
                    "push_login_timeout",
                    "The push service did not answer the login request",
                ));
            }
            if heartbeat_ack_deadline.is_some_and(|deadline| now >= deadline) {
                return Err(PushError::new(
                    "push_heartbeat_timeout",
                    "The push service stopped acknowledging heartbeats",
                ));
            }
            let until_heartbeat = if heartbeat_ack_deadline.is_some() {
                config.read_timeout
            } else {
                next_heartbeat.saturating_duration_since(now)
            };
            let until_flush = pending
                .values()
                .map(|event| event.deadline.saturating_duration_since(now))
                .min()
                .unwrap_or(config.read_timeout);
            let until_login = login_deadline
                .map(|deadline| deadline.saturating_duration_since(now))
                .unwrap_or(config.read_timeout);
            let slice = until_heartbeat
                .min(until_flush)
                .min(until_login)
                .min(config.read_timeout)
                .min(SESSION_SLICE)
                .max(Duration::from_millis(20));
            let bytes = transport.recv(slice)?;
            if !bytes.is_empty() {
                reader.extend(&bytes);
                while let Some((tag, payload)) = reader.next_frame(config.maximum_frame_bytes)? {
                    stream_id_in += 1;
                    heartbeat_ack_deadline = None;
                    next_heartbeat = Instant::now() + heartbeat_interval;
                    match decode_frame(tag, &payload)? {
                        Frame::HeartbeatPing => {
                            transport.send(
                                &encode_heartbeat_ack(Some(stream_id_in)),
                                config.connect_timeout,
                            )?;
                            advance_advertised_stream_id(
                                stream_id_in,
                                &mut advertised_stream_id,
                                &mut unacknowledged_persistent,
                            );
                        }
                        Frame::DataMessage(message) => {
                            if message
                                .persistent_id
                                .as_deref()
                                .is_some_and(|id| !id.is_empty())
                            {
                                unacknowledged_persistent += 1;
                            }
                            let immediate_ack = message.immediate_ack;
                            handle_data_message(
                                &message,
                                &keys,
                                &mut pending,
                                &mut persistent_ids,
                                config,
                            );
                            if immediate_ack
                                || (unacknowledged_persistent > 0
                                    && unacknowledged_persistent
                                        % UNACKNOWLEDGED_PERSISTENT_STREAM_ACK
                                        == 0)
                            {
                                transport.send(
                                    &encode_stream_ack(Some(stream_id_in)),
                                    config.connect_timeout,
                                )?;
                                advance_advertised_stream_id(
                                    stream_id_in,
                                    &mut advertised_stream_id,
                                    &mut unacknowledged_persistent,
                                );
                            }
                        }
                        Frame::LoginResponse(response) => {
                            if let Some(code) = response.error_code.filter(|code| *code != 0) {
                                intent.request_registration_refresh();
                                return Err(PushError::new(
                                    "push_login_refused",
                                    format!("push_login_refused ({code})"),
                                ));
                            }
                            login_deadline = None;
                            if let Some(millis) =
                                response.heartbeat_interval_ms.filter(|millis| *millis > 0)
                            {
                                heartbeat_interval = clamped_heartbeat_interval(
                                    config,
                                    Duration::from_millis(millis as u64),
                                );
                                next_heartbeat = Instant::now() + heartbeat_interval;
                            }
                        }
                        Frame::Close => return Ok(false),
                        _ => {}
                    }
                }
            }
            if heartbeat_ack_deadline.is_none() && Instant::now() >= next_heartbeat {
                transport.send(
                    &encode_heartbeat_ping(Some(stream_id_in)),
                    config.connect_timeout,
                )?;
                heartbeat_ack_deadline = Some(Instant::now() + config.heartbeat_ack_timeout);
                advance_advertised_stream_id(
                    stream_id_in,
                    &mut advertised_stream_id,
                    &mut unacknowledged_persistent,
                );
            }
        }
    })();

    if result.is_ok() {
        let _ = transport.send(&encode_close(), config.connect_timeout);
    }
    transport.close();
    intent.set_active(None);
    if session_is_current(config, config_generation, deps, intent, scope) {
        let deadline = Instant::now()
            .checked_sub(config.coalescing_window)
            .unwrap_or_else(Instant::now);
        flush_due_events(
            config,
            config_generation,
            deps,
            intent,
            scope,
            &mut pending,
            deadline,
        );
    }
    result
}

struct PendingEvent {
    deadline: Instant,
    changed_ids: Vec<String>,
    account_type: Option<String>,
    linked: Option<bool>,
    platform_code: Option<String>,
    sync_state: Option<String>,
    sync_date: Option<String>,
    sync_game_count: Option<u64>,
}

fn handle_data_message(
    message: &crate::sources::gfn::push::protocol::DataMessage,
    keys: &EceKeyPair,
    pending: &mut BTreeMap<MessageType, PendingEvent>,
    persistent_ids: &mut Vec<String>,
    config: &PushOwnerConfig,
) {
    if let Some(id) = &message.persistent_id {
        if persistent_ids.len() >= config.maximum_persistent_ids {
            persistent_ids.remove(0);
        }
        persistent_ids.push(id.clone());
    }
    let Some(parsed) = parse_message(message, keys) else {
        return;
    };
    let Some(message_type) = parsed.message_type else {
        return;
    };
    if !matches!(
        message_type,
        MessageType::LibraryChange
            | MessageType::FavoritesChange
            | MessageType::SubscriptionChange
            | MessageType::LinkedAccountChange
            | MessageType::PlatformSyncChange
    ) {
        return;
    }
    let deadline = Instant::now() + config.coalescing_window;
    let entry = pending.entry(message_type).or_insert_with(|| PendingEvent {
        deadline,
        changed_ids: Vec::new(),
        account_type: None,
        linked: None,
        platform_code: None,
        sync_state: None,
        sync_date: None,
        sync_game_count: None,
    });
    for id in parsed.changed_ids {
        if entry.changed_ids.len() >= config.maximum_changed_ids {
            break;
        }
        if !entry.changed_ids.contains(&id) {
            entry.changed_ids.push(id);
        }
    }
    if parsed.account_type.is_some() {
        entry.account_type = parsed.account_type;
    }
    if parsed.account_linked.is_some() {
        entry.linked = parsed.account_linked;
    }
    if parsed.platform_code.is_some() {
        entry.platform_code = parsed.platform_code;
    }
    if parsed.sync_state.is_some() {
        entry.sync_state = parsed.sync_state;
    }
    if parsed.sync_date.is_some() {
        entry.sync_date = parsed.sync_date;
    }
    if parsed.sync_game_count.is_some() {
        entry.sync_game_count = parsed.sync_game_count;
    }
}

fn parse_message(
    message: &crate::sources::gfn::push::protocol::DataMessage,
    keys: &EceKeyPair,
) -> Option<crate::sources::gfn::push::PushMessage> {
    let raw_data = message.raw_data.as_ref()?;
    let property = |name: &str| {
        message
            .app_data
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    };
    let body = decrypt_web_push(
        keys,
        property("content-encoding"),
        property("crypto-key"),
        property("encryption"),
        raw_data,
    )
    .ok()?;
    let value: serde_json::Value = serde_json::from_slice(&body).ok()?;
    let data = value.get("data")?.as_object()?;
    crate::sources::gfn::push::PushMessage::parse(data)
}

fn retire_registration(deps: &PushOwnerDeps, scope: &PushScope, detail: String) -> PushError {
    let _ = deps.store.clear(&scope.account_key());
    PushError::new(
        STORED_REGISTRATION_RETIRED,
        format!("The stored push registration was retired ({detail})"),
    )
}

fn flush_due_events(
    config: &PushOwnerConfig,
    config_generation: u64,
    deps: &PushOwnerDeps,
    intent: &Arc<Intent>,
    scope: &PushScope,
    pending: &mut BTreeMap<MessageType, PendingEvent>,
    now: Instant,
) {
    let mut ready: Vec<MessageType> = pending
        .iter()
        .filter(|(_, event)| event.deadline <= now)
        .map(|(message_type, _)| *message_type)
        .collect();
    ready.sort();
    for message_type in ready {
        if !session_is_current(config, config_generation, deps, intent, scope) {
            pending.clear();
            return;
        }
        let Some(event) = pending.remove(&message_type) else {
            continue;
        };
        let event = match message_type {
            MessageType::LibraryChange => Some(PushEvent::Library {
                changed_ids: event.changed_ids,
            }),
            MessageType::FavoritesChange => Some(PushEvent::Favorites {
                changed_ids: event.changed_ids,
            }),
            MessageType::SubscriptionChange => Some(PushEvent::Subscription {
                changed_ids: event.changed_ids,
            }),
            MessageType::LinkedAccountChange => Some(PushEvent::LinkedAccount {
                account_type: event.account_type,
                linked: event.linked,
            }),
            MessageType::PlatformSyncChange => Some(PushEvent::PlatformSync {
                platform_code: event.platform_code,
                sync_state: event.sync_state,
                sync_date: event.sync_date,
                sync_game_count: event.sync_game_count,
            }),
            _ => None,
        };
        if let Some(event) = event {
            let generation = intent.generation.load(Ordering::SeqCst);
            (deps.sink)(event, generation);
        }
    }
}

#[cfg(test)]
mod guard_tests {
    use super::*;
    use crate::sources::gfn::push::registration::{
        HttpRequest, HttpResponse, PushEndpoints, PushHttp, PushIdentity,
    };
    use crate::sources::gfn::push::store::PushStateStore;
    use crate::sources::gfn::push::transport::{PushTransport, PushTransportFactory};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingHttp(Arc<AtomicUsize>);

    impl PushHttp for CountingHttp {
        fn execute(&self, _request: &HttpRequest) -> Result<HttpResponse, PushError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Err(PushError::new("push_http_failed", "unexpected request"))
        }
    }

    struct EmptyStore;

    impl PushStateStore for EmptyStore {
        fn load(&self, _account: &str) -> Result<Option<Registration>, PushError> {
            Ok(None)
        }

        fn save(&self, _account: &str, _registration: &Registration) -> Result<(), PushError> {
            Ok(())
        }

        fn clear(&self, _account: &str) -> Result<(), PushError> {
            Ok(())
        }
    }

    struct NoTransport;

    impl PushTransport for NoTransport {
        fn send(&mut self, _frame: &[u8], _timeout: Duration) -> Result<(), PushError> {
            Ok(())
        }

        fn recv(&mut self, _timeout: Duration) -> Result<Vec<u8>, PushError> {
            Ok(Vec::new())
        }

        fn close(&mut self) {}
    }

    struct NoFactory;

    impl PushTransportFactory for NoFactory {
        fn connect(
            &self,
            _host: &str,
            _port: u16,
            _timeout: Duration,
        ) -> Result<Box<dyn PushTransport>, PushError> {
            Ok(Box::new(NoTransport))
        }
    }

    fn config(provider_id: &str) -> PushOwnerConfig {
        PushOwnerConfig::bounded(
            PushEndpoints::default(),
            PushIdentity {
                project_id: "project".into(),
                api_key: "api-key".into(),
                sender_id: "123456789012".into(),
                app_id: "app".into(),
                firebase_app_id: "1:1:web:abc".into(),
                vapid_key: None,
            },
            provider_id.into(),
            "device".into(),
        )
    }

    fn scope(provider_id: &str) -> PushScope {
        PushScope {
            user_id: "user-1".into(),
            provider_id: provider_id.into(),
            generation: 4,
        }
    }

    #[test]
    fn a_stale_config_pair_never_reaches_registration_http() {
        let stale_config = config("nvidia");
        let intent = Arc::new(Intent::new(stale_config.clone()));
        let desired = Arc::new(Mutex::new(Some(scope("alliance"))));
        let token_calls = Arc::new(AtomicUsize::new(0));
        let http_calls = Arc::new(AtomicUsize::new(0));
        let deps = PushOwnerDeps {
            http: Arc::new(CountingHttp(Arc::clone(&http_calls))),
            transport: Arc::new(NoFactory),
            store: Arc::new(EmptyStore),
            sink: Arc::new(|_: PushEvent, _: u64| {}),
            scope_source: {
                let desired = Arc::clone(&desired);
                Arc::new(move || desired.lock().unwrap().clone())
            },
            token_source: {
                let token_calls = Arc::clone(&token_calls);
                Arc::new(move |_: &PushScope| {
                    token_calls.fetch_add(1, Ordering::SeqCst);
                    Some("token".to_owned())
                })
            },
        };
        let (before, before_generation) = intent.config();
        assert_eq!(before.provider_id, "nvidia");
        intent.replace_config(config("alliance"));
        let (current, current_generation) = intent.config();
        assert_eq!(current.provider_id, "alliance");
        assert!(current_generation > before_generation);
        let outcome = run_scoped_session(
            &stale_config,
            before_generation,
            &deps,
            &intent,
            &scope("alliance"),
        );
        assert!(matches!(outcome, SessionOutcome::ScopeChanged));
        assert_eq!(
            token_calls.load(Ordering::SeqCst),
            0,
            "no token may be requested"
        );
        assert_eq!(
            http_calls.load(Ordering::SeqCst),
            0,
            "no registration HTTP may run"
        );
        assert!(!session_is_current(
            &stale_config,
            before_generation,
            &deps,
            &intent,
            &scope("alliance")
        ));
        assert!(
            !session_is_current(
                &stale_config,
                current_generation,
                &deps,
                &intent,
                &scope("alliance")
            ),
            "a stale provider must not be accepted"
        );
        assert!(
            session_is_current(
                &current,
                current_generation,
                &deps,
                &intent,
                &scope("alliance")
            ),
            "the current pair must be accepted"
        );
    }
}
