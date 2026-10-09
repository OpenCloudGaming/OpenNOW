pub mod authorization;
mod journal;
pub mod package;

use journal::Journal;
use opennow_plugin_api::media::*;
use opennow_plugin_api::provider::*;
use opennow_plugin_api::{Coverage, PluginId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub const PLUGIN_ID: &str = "org.opennow.sdk-demo";
pub const CATALOG_REVISION: &str = "sdk-demo-catalog-v1";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const TARGET: &str = env!("DEMO_TARGET");
pub const CAPABILITIES: &[Capability] = &[
    Capability::AuthPairing,
    Capability::Accounts,
    Capability::PublicCatalog,
    Capability::LibraryCatalog,
    Capability::CatalogDetails,
    Capability::Launch,
    Capability::Sessions,
    Capability::MediaWorker,
];

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    serial: u64,
    revision: u64,
    selected: Option<AccountKey>,
    accounts: Vec<PublicAccount>,
    attempts: Vec<AuthRecord>,
    sessions: Vec<SessionRecord>,
    stops: Vec<StopRecord>,
    #[serde(default)]
    not_allocated: Vec<NotAllocatedRecord>,
}

impl State {
    fn valid(&self) -> bool {
        let account_ids: BTreeSet<_> = self
            .accounts
            .iter()
            .map(|account| (&account.key.authority, &account.key.account))
            .collect();
        let operations: BTreeSet<_> = self
            .sessions
            .iter()
            .map(|session| &session.operation)
            .chain(self.not_allocated.iter().map(|record| &record.operation))
            .collect();
        let receipts: BTreeSet<_> = self
            .sessions
            .iter()
            .map(|session| &session.receipt)
            .collect();
        let seats: BTreeSet<_> = self
            .sessions
            .iter()
            .map(|session| &session.view.key.remote_id)
            .collect();
        self.accounts.len() <= 2
            && self.attempts.len() <= 64
            && self.sessions.len() <= 64
            && self.stops.len() <= 128
            && self.not_allocated.len() <= 128
            && self.serial < u64::MAX - 1
            && self.revision < u64::MAX - 1
            && account_ids.len() == self.accounts.len()
            && operations.len() == self.sessions.len() + self.not_allocated.len()
            && receipts.len() == self.sessions.len()
            && seats.len() == self.sessions.len()
            && self
                .sessions
                .iter()
                .filter(|session| !terminal(&session.view))
                .count()
                <= 1
            && self
                .selected
                .as_ref()
                .is_none_or(|key| self.accounts.iter().any(|account| &account.key == key))
            && self.sessions.iter().all(|session| {
                session
                    .view
                    .key
                    .account
                    .as_ref()
                    .is_some_and(|key| self.accounts.iter().any(|account| &account.key == key))
            })
    }

    fn next(&mut self) -> u64 {
        self.serial += 1;
        self.serial
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthRecord {
    attempt: AttemptId,
    authority: AuthorityId,
    remember: bool,
    expires_at_ms: u64,
    state: LoginState,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case", deny_unknown_fields)]
enum LoginState {
    Pending,
    Authorized,
    Completed { revision: u64 },
    Cancelled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionRecord {
    operation: OperationId,
    receipt: ReceiptId,
    decision: Option<Acceptance>,
    view: SessionView,
    media: Option<authorization::Grant>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StopRecord {
    operation: OperationId,
    session: SessionKey,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NotAllocatedRecord {
    operation: OperationId,
    account: Option<AccountKey>,
    target: LaunchTarget,
    error: ProviderErrorCode,
}

pub struct DemoProvider {
    journal: Journal,
    state: State,
    faulted: bool,
}

struct Completed {
    reply: ProviderReply,
    allocation: Option<AllocationTicket>,
}

impl From<ProviderReply> for Completed {
    fn from(reply: ProviderReply) -> Self {
        Self {
            reply,
            allocation: None,
        }
    }
}

impl DemoProvider {
    pub fn open(directory: &Path) -> io::Result<Self> {
        let (mut journal, mut state) = Journal::open(directory)?;
        let mut changed = false;
        for account in &mut state.accounts {
            if account.persistence == Persistence::Temporary && !account.reauthentication_required {
                account.reauthentication_required = true;
                changed = true;
            }
        }
        if state.selected.as_ref().is_some_and(|key| {
            state
                .accounts
                .iter()
                .any(|account| &account.key == key && account.reauthentication_required)
        }) {
            state.selected = None;
            state.revision += 1;
            changed = true;
        }
        if changed {
            journal.append(&state)?;
        }
        Ok(Self {
            journal,
            state,
            faulted: false,
        })
    }

    pub fn handle(&mut self, request: &HostRequestV2) -> ProviderResponseV2 {
        let result = if self.faulted {
            Err(failure(ProviderErrorCode::OutcomeUnknown))
        } else if request.validate().is_err() {
            Err(failure(ProviderErrorCode::InvalidRequest))
        } else if !request.request.permits(CAPABILITIES) {
            Err(failure(ProviderErrorCode::UnsupportedFeature))
        } else {
            self.execute(&request.request)
        };
        let (outcome, allocation) = match result {
            Ok(completed) => (
                ProviderOutcome::success(completed.reply),
                completed.allocation,
            ),
            Err(error) => (ProviderOutcome::Failure { error }, None),
        };
        ProviderResponseV2 {
            v: Version2,
            epoch: request.epoch,
            id: request.id.clone(),
            outcome,
            effects: List::default(),
            allocation,
        }
    }

    fn save(&mut self, state: State) -> Result<(), ProviderError> {
        if self.journal.append(&state).is_err() {
            self.faulted = true;
            return Err(failure(ProviderErrorCode::OutcomeUnknown));
        }
        self.state = state;
        Ok(())
    }

    fn execute(&mut self, request: &ProviderRequest) -> Result<Completed, ProviderError> {
        use ProviderReply as R;
        use ProviderRequest as Q;
        let reply = match request {
            Q::Hello(hello) => {
                if hello.plugin_id.as_str() != PLUGIN_ID
                    || hello.version.as_str() != VERSION
                    || CAPABILITIES
                        .iter()
                        .any(|cap| !hello.capabilities.contains(cap))
                {
                    return Err(failure(ProviderErrorCode::InvalidRequest));
                }
                R::Hello(ProviderHelloReply {
                    plugin_id: PluginId::new(PLUGIN_ID).unwrap(),
                    version: text(VERSION),
                    protocol_version: 2,
                    capabilities: list(CAPABILITIES.to_vec()),
                    auth_kinds: list(vec![AuthKind::Pairing]),
                })
            }
            Q::Shutdown(_) => R::Shutdown(Empty {}),
            Q::AuthAuthorities(_) => R::AuthAuthorities(list(
                ["demo-blue", "demo-orange"]
                    .into_iter()
                    .map(|authority| Authority {
                        id: AuthorityId::new(authority).unwrap(),
                        name: text(authority),
                    })
                    .collect(),
            )),
            Q::AuthStatus(_) => R::AuthStatus(self.auth_state()),
            Q::AuthBegin(begin) => {
                if begin.kind != AuthKind::Pairing {
                    return Err(failure(ProviderErrorCode::UnsupportedFeature));
                }
                let authority = begin
                    .authority
                    .clone()
                    .unwrap_or_else(|| AuthorityId::new("demo-blue").unwrap());
                if !matches!(authority.as_str(), "demo-blue" | "demo-orange") {
                    return Err(failure(ProviderErrorCode::InvalidRequest));
                }
                if self.state.attempts.len() >= 64 {
                    return Err(failure(ProviderErrorCode::BusyBeforeDispatch));
                }
                let mut next = self.state.clone();
                let record = AuthRecord {
                    attempt: AttemptId::new(format!("demo-login-{}", next.next())).unwrap(),
                    authority,
                    remember: begin.remember,
                    expires_at_ms: now_ms() + 300_000,
                    state: LoginState::Pending,
                };
                let challenge = challenge(&record);
                next.attempts.push(record);
                self.save(next)?;
                R::AuthBegin(AuthState::Pending { challenge })
            }
            Q::AuthPoll(poll) => {
                let index = self.attempt_index(&poll.attempt)?;
                let record = &self.state.attempts[index];
                if matches!(record.state, LoginState::Cancelled) {
                    return Err(failure(ProviderErrorCode::Cancelled));
                }
                if now_ms() >= record.expires_at_ms {
                    return Err(failure(ProviderErrorCode::AuthenticationFailed));
                }
                if matches!(record.state, LoginState::Pending) {
                    let mut next = self.state.clone();
                    next.attempts[index].state = LoginState::Authorized;
                    self.save(next)?;
                }
                R::AuthPoll(AuthState::Authorized {
                    attempt: poll.attempt.clone(),
                })
            }
            Q::AuthComplete(complete) => {
                if complete
                    .proof
                    .as_ref()
                    .is_some_and(|proof| proof.expose_secret() != "SDK-DEMO")
                {
                    return Err(failure(ProviderErrorCode::AuthenticationFailed));
                }
                let index = self.attempt_index(&complete.attempt)?;
                let record = self.state.attempts[index].clone();
                let key = account_key(&record.authority);
                if let LoginState::Completed { revision } = record.state {
                    let account = self.account(&key)?.clone();
                    if account.reauthentication_required
                        || self.state.selected.as_ref() != Some(&key)
                    {
                        return Err(failure(ProviderErrorCode::AuthRequired));
                    }
                    return Ok(R::AuthComplete(AuthState::SignedIn { account, revision }).into());
                }
                if matches!(record.state, LoginState::Cancelled) {
                    return Err(failure(ProviderErrorCode::Cancelled));
                }
                if !matches!(record.state, LoginState::Authorized)
                    || now_ms() >= record.expires_at_ms
                {
                    return Err(failure(ProviderErrorCode::AuthRequired));
                }
                let mut next = self.state.clone();
                let account = PublicAccount {
                    key: key.clone(),
                    name: text(&format!(
                        "SDK demo {} player",
                        record.authority.as_str().trim_start_matches("demo-")
                    )),
                    persistence: if record.remember {
                        Persistence::Durable
                    } else {
                        Persistence::Temporary
                    },
                    reauthentication_required: false,
                    pin_locked: false,
                };
                next.accounts.retain(|account| account.key != key);
                next.accounts.push(account.clone());
                next.selected = Some(key);
                next.revision += 1;
                let revision = next.revision;
                next.attempts[index].state = LoginState::Completed { revision };
                self.save(next)?;
                R::AuthComplete(AuthState::SignedIn { account, revision })
            }
            Q::AuthCancel(cancel) => {
                if let Some(index) = self
                    .state
                    .attempts
                    .iter()
                    .position(|record| record.attempt == cancel.attempt)
                    && !matches!(
                        self.state.attempts[index].state,
                        LoginState::Completed { .. } | LoginState::Cancelled
                    )
                {
                    let mut next = self.state.clone();
                    next.attempts[index].state = LoginState::Cancelled;
                    self.save(next)?;
                }
                R::AuthCancel(Empty {})
            }
            Q::AuthLogout(account) => {
                self.account(account)?;
                if self.state.selected.as_ref() == Some(account) {
                    let mut next = self.state.clone();
                    next.selected = None;
                    next.revision += 1;
                    self.save(next)?;
                }
                R::AuthLogout(self.auth_state())
            }
            Q::AccountsList(_) => R::AccountsList(self.accounts()),
            Q::AccountsSelect(select) => {
                if select.pin.is_some() {
                    return Err(failure(ProviderErrorCode::UnsupportedFeature));
                }
                if self.account(&select.account)?.reauthentication_required {
                    return Err(failure(ProviderErrorCode::AuthRequired));
                }
                if self.state.selected.as_ref() != Some(&select.account) {
                    let mut next = self.state.clone();
                    next.selected = Some(select.account.clone());
                    next.revision += 1;
                    self.save(next)?;
                }
                R::AccountsSelect(self.auth_state())
            }
            Q::AccountsRemove(account) => {
                self.account(account)?;
                if self.state.sessions.iter().any(|session| {
                    session.view.key.account.as_ref() == Some(account) && !terminal(&session.view)
                }) {
                    return Err(failure(ProviderErrorCode::CleanupRequired));
                }
                let mut next = self.state.clone();
                next.accounts.retain(|saved| &saved.key != account);
                next.sessions
                    .retain(|session| session.view.key.account.as_ref() != Some(account));
                next.stops
                    .retain(|stop| stop.session.account.as_ref() != Some(account));
                next.attempts
                    .retain(|attempt| account_key(&attempt.authority) != *account);
                if next.selected.as_ref() == Some(account) {
                    next.selected = None;
                }
                next.revision += 1;
                self.save(next)?;
                R::AccountsRemove(self.accounts())
            }
            Q::CatalogPublic(query) => R::CatalogPublic(self.catalog(query)?),
            Q::CatalogLibrary(query) => {
                if !matches!(query.scope, CatalogScope::Account { .. }) {
                    return Err(failure(ProviderErrorCode::AuthRequired));
                }
                R::CatalogLibrary(self.catalog(query)?)
            }
            Q::CatalogDetails(query) => {
                self.catalog_scope(&query.scope)?;
                let game =
                    game(&query.game).ok_or_else(|| failure(ProviderErrorCode::InvalidRequest))?;
                R::CatalogDetails(GameDetails {
                    game,
                    description: Some(text(
                        "OpenNOW SDK demonstration: local generated H.264 motion and Opus tone. No commercial game or cloud service.",
                    )),
                    variants: list(vec![GameVariant {
                        id: VariantId::new("fixture").unwrap(),
                        label: text("Native media fixture"),
                        availability: Availability::Available,
                    }]),
                    revision: text(CATALOG_REVISION),
                    scope: query.scope.clone(),
                })
            }
            Q::LaunchInspect(inspect) => {
                self.current_scope(inspect.scope.as_ref())?;
                check_target(&inspect.target)?;
                if inspect.catalog_revision.as_str() != CATALOG_REVISION {
                    return Err(failure(ProviderErrorCode::ScopeChanged));
                }
                R::LaunchInspect(LaunchDecision::Ready {
                    target: inspect.target.clone(),
                    revision: text(CATALOG_REVISION),
                })
            }
            Q::SessionCreate(create) => return self.create(create),
            Q::SessionPoll(key) => R::SessionPoll(self.session(key)?.view.clone()),
            Q::SessionDiscover(query) => {
                let owner = self.retained_scope(query.scope.as_ref())?;
                R::SessionDiscover(list(
                    self.state
                        .sessions
                        .iter()
                        .filter(|session| {
                            session.view.key.account.as_ref() == Some(owner)
                                && !terminal(&session.view)
                        })
                        .map(|session| session.view.clone())
                        .collect(),
                ))
            }
            Q::SessionClaim(claim) => {
                let owner = self.retained_scope(claim.scope.as_ref())?;
                if claim.session.account.as_ref() != Some(owner) {
                    return Err(failure(ProviderErrorCode::ScopeChanged));
                }
                let session = self.session(&claim.session)?;
                if session.decision != Some(Acceptance::Accepted) || terminal(&session.view) {
                    return Err(failure(ProviderErrorCode::SessionNotReady));
                }
                R::SessionClaim(session.view.clone())
            }
            Q::SessionReconcile(query) => R::SessionReconcile(self.reconcile(query)?),
            Q::SessionResolveAllocation(resolve) => {
                let index = self
                    .state
                    .sessions
                    .iter()
                    .position(|session| {
                        session.operation == resolve.operation && session.receipt == resolve.receipt
                    })
                    .ok_or_else(|| failure(ProviderErrorCode::InvalidRequest))?;
                match self.state.sessions[index].decision {
                    Some(prior) if prior != resolve.decision => {
                        return Err(failure(ProviderErrorCode::InvalidRequest));
                    }
                    Some(_) => {}
                    None => {
                        let mut next = self.state.clone();
                        next.sessions[index].decision = Some(resolve.decision);
                        if resolve.decision == Acceptance::Rejected {
                            next.sessions[index].view.state = RemoteSessionState::Finished {
                                reason: TerminalReason::AllocationRejected,
                            };
                        }
                        self.save(next)?;
                    }
                }
                R::SessionResolveAllocation(CleanupState::Resolved)
            }
            Q::SessionPrepare(prepare) => R::SessionPrepare(self.prepare(prepare)?),
            Q::SessionStop(stop) => {
                if let Some(prior) = self
                    .state
                    .stops
                    .iter()
                    .find(|prior| prior.operation == stop.operation)
                {
                    if prior.session != stop.session {
                        return Err(failure(ProviderErrorCode::InvalidRequest));
                    }
                    return Ok(R::SessionStop(CleanupState::Resolved).into());
                }
                let index = self
                    .state
                    .sessions
                    .iter()
                    .position(|session| session.view.key == stop.session)
                    .ok_or_else(|| failure(ProviderErrorCode::SessionNotFound))?;
                if self.state.stops.len() >= 128 {
                    return Err(failure(ProviderErrorCode::BusyBeforeDispatch));
                }
                let mut next = self.state.clone();
                next.sessions[index].view.state = RemoteSessionState::Finished {
                    reason: TerminalReason::UserStopped,
                };
                if next.sessions[index].decision.is_none() {
                    next.sessions[index].decision = Some(Acceptance::Rejected);
                }
                next.stops.push(StopRecord {
                    operation: stop.operation.clone(),
                    session: stop.session.clone(),
                });
                self.save(next)?;
                R::SessionStop(CleanupState::Resolved)
            }
            _ => return Err(failure(ProviderErrorCode::UnsupportedFeature)),
        };
        Ok(reply.into())
    }

    fn create(&mut self, create: &CreateSession) -> Result<Completed, ProviderError> {
        if let Some(session) = self
            .state
            .sessions
            .iter()
            .find(|session| session.operation == create.operation)
        {
            if session.view.key.account != create.scope.as_ref().map(|scope| scope.account.clone())
                || session.view.target != create.target
            {
                return Err(failure(ProviderErrorCode::InvalidRequest));
            }
            return Ok(created(session));
        }
        if let Some(record) = self
            .state
            .not_allocated
            .iter()
            .find(|record| record.operation == create.operation)
        {
            if record.account != create.scope.as_ref().map(|scope| scope.account.clone())
                || record.target != create.target
            {
                return Err(failure(ProviderErrorCode::InvalidRequest));
            }
            return Err(failure(record.error));
        }
        let owner = match self.preflight_create(create) {
            Ok(owner) => owner,
            Err(error) => {
                if self.state.not_allocated.len() >= 128 {
                    return Err(failure(ProviderErrorCode::OutcomeUnknown));
                }
                let mut next = self.state.clone();
                next.not_allocated.push(NotAllocatedRecord {
                    operation: create.operation.clone(),
                    account: create.scope.as_ref().map(|scope| scope.account.clone()),
                    target: create.target.clone(),
                    error: error.code,
                });
                self.save(next)?;
                return Err(error);
            }
        };
        let mut next = self.state.clone();
        let serial = next.next();
        let session = SessionRecord {
            operation: create.operation.clone(),
            receipt: ReceiptId::new(format!("demo-receipt-{serial}")).unwrap(),
            decision: None,
            view: SessionView {
                key: SessionKey {
                    account: Some(owner),
                    remote_id: SessionId::new(format!("demo-session-{serial}")).unwrap(),
                },
                target: create.target.clone(),
                state: RemoteSessionState::Ready,
            },
            media: None,
        };
        let completed = created(&session);
        next.sessions.push(session);
        self.save(next)?;
        Ok(completed)
    }

    fn preflight_create(&self, create: &CreateSession) -> Result<AccountKey, ProviderError> {
        let owner = self.current_scope(create.scope.as_ref())?.clone();
        check_target(&create.target)?;
        if create.catalog_revision.as_str() != CATALOG_REVISION {
            return Err(failure(ProviderErrorCode::ScopeChanged));
        }
        if create.preferences.video.hdr
            || create.preferences.video.bit_depth != 8
            || create.preferences.video.chroma != Chroma::Yuv420
            || create
                .preferences
                .video
                .encoding
                .is_some_and(|encoding| encoding != VideoEncoding::H264AnnexB)
        {
            return Err(failure(ProviderErrorCode::UnsupportedFeature));
        }
        accepted_media(&create.offer)?;
        if self.state.sessions.len() >= 64
            || self
                .state
                .sessions
                .iter()
                .any(|session| !terminal(&session.view))
        {
            return Err(failure(ProviderErrorCode::BusyBeforeDispatch));
        }
        Ok(owner)
    }

    fn prepare(&mut self, prepare: &PrepareSession) -> Result<PreparedWorker, ProviderError> {
        let index = self
            .state
            .sessions
            .iter()
            .position(|session| session.view.key == prepare.session)
            .ok_or_else(|| failure(ProviderErrorCode::SessionNotFound))?;
        let session = &self.state.sessions[index];
        if session.decision != Some(Acceptance::Accepted) || terminal(&session.view) {
            return Err(failure(ProviderErrorCode::SessionNotReady));
        }
        let accepted = accepted_media(&prepare.offer)?;
        let (grant, bootstrap) =
            authorization::issue(&prepare.session, &accepted, prepare.offer.expires_at_ms)
                .map_err(|_| failure(ProviderErrorCode::ServiceUnavailable))?;
        let mut next = self.state.clone();
        next.sessions[index].media = Some(grant);
        self.save(next)?;
        Ok(PreparedWorker {
            accepted,
            bootstrap,
        })
    }

    fn reconcile(&self, query: &ReconcileSession) -> Result<Reconciliation, ProviderError> {
        if let Some(record) = self
            .state
            .not_allocated
            .iter()
            .find(|record| record.operation == query.operation)
        {
            if query.session.is_some()
                || record.account != query.scope.as_ref().map(|scope| scope.account.clone())
            {
                return Err(failure(ProviderErrorCode::ScopeChanged));
            }
            return Ok(Reconciliation::NotAllocated {
                operation: query.operation.clone(),
            });
        }
        let owner = self.retained_scope(query.scope.as_ref())?;
        let operation_session = self
            .state
            .sessions
            .iter()
            .find(|session| session.operation == query.operation)
            .or_else(|| {
                self.state
                    .stops
                    .iter()
                    .find(|stop| stop.operation == query.operation)
                    .and_then(|stop| {
                        self.state
                            .sessions
                            .iter()
                            .find(|session| session.view.key == stop.session)
                    })
            });
        let Some(session) = operation_session else {
            return Ok(Reconciliation::Unknown {
                operation: query.operation.clone(),
            });
        };
        if session.view.key.account.as_ref() != Some(owner)
            || query
                .session
                .as_ref()
                .is_some_and(|key| key != &session.view.key)
        {
            return Err(failure(ProviderErrorCode::ScopeChanged));
        }
        Ok(match (&session.view.state, session.decision) {
            (RemoteSessionState::Finished { reason }, _) => Reconciliation::Terminal {
                session: session.view.key.clone(),
                reason: *reason,
            },
            (_, None) => Reconciliation::PendingAllocation {
                session: session.view.clone(),
                ticket: AllocationTicket {
                    operation: session.operation.clone(),
                    receipt: session.receipt.clone(),
                    session: session.view.key.clone(),
                },
            },
            (_, Some(Acceptance::Accepted)) => Reconciliation::Active {
                session: session.view.clone(),
            },
            (_, Some(Acceptance::Rejected)) => {
                return Err(failure(ProviderErrorCode::OutcomeUnknown));
            }
        })
    }

    fn session(&self, key: &SessionKey) -> Result<&SessionRecord, ProviderError> {
        self.state
            .sessions
            .iter()
            .find(|session| &session.view.key == key)
            .ok_or_else(|| failure(ProviderErrorCode::SessionNotFound))
    }

    fn attempt_index(&self, attempt: &AttemptId) -> Result<usize, ProviderError> {
        self.state
            .attempts
            .iter()
            .position(|record| &record.attempt == attempt)
            .ok_or_else(|| failure(ProviderErrorCode::AuthenticationFailed))
    }
    fn account(&self, key: &AccountKey) -> Result<&PublicAccount, ProviderError> {
        self.state
            .accounts
            .iter()
            .find(|account| &account.key == key)
            .ok_or_else(|| failure(ProviderErrorCode::AuthRequired))
    }
    fn retained_scope<'a>(
        &'a self,
        scope: Option<&AccountScope>,
    ) -> Result<&'a AccountKey, ProviderError> {
        let scope = scope.ok_or_else(|| failure(ProviderErrorCode::AuthRequired))?;
        Ok(&self.account(&scope.account)?.key)
    }
    fn current_scope<'a>(
        &'a self,
        scope: Option<&AccountScope>,
    ) -> Result<&'a AccountKey, ProviderError> {
        let scope = scope.ok_or_else(|| failure(ProviderErrorCode::AuthRequired))?;
        if scope.revision != self.state.revision
            || self.state.selected.as_ref() != Some(&scope.account)
        {
            return Err(failure(ProviderErrorCode::ScopeChanged));
        }
        self.retained_scope(Some(scope))
    }
    fn auth_state(&self) -> AuthState {
        self.state
            .selected
            .as_ref()
            .and_then(|key| {
                self.state
                    .accounts
                    .iter()
                    .find(|account| &account.key == key)
            })
            .map_or(AuthState::SignedOut, |account| AuthState::SignedIn {
                account: account.clone(),
                revision: self.state.revision,
            })
    }
    fn accounts(&self) -> Accounts {
        Accounts {
            accounts: list(self.state.accounts.clone()),
            selected: self.state.selected.clone(),
            revision: self.state.revision,
        }
    }
    fn catalog_scope(&self, scope: &CatalogScope) -> Result<(), ProviderError> {
        if let CatalogScope::Account { scope } = scope {
            self.current_scope(Some(scope))?;
        }
        Ok(())
    }

    fn catalog(&self, request: &CatalogRequest) -> Result<GamePage, ProviderError> {
        self.catalog_scope(&request.scope)?;
        let query = request.query.query.to_lowercase();
        let scope = serde_json::to_vec(&request.scope)
            .map_err(|_| failure(ProviderErrorCode::InternalError))?;
        let mut hash = Sha256::new();
        hash.update(query.as_bytes());
        hash.update(scope);
        let context = format!("{:x}", hash.finalize());
        let start = if let Some(cursor) = &request.query.cursor {
            let (offset, binding) = cursor
                .split_once(':')
                .ok_or_else(|| failure(ProviderErrorCode::InvalidRequest))?;
            if binding != context {
                return Err(failure(ProviderErrorCode::ScopeChanged));
            }
            offset
                .parse::<usize>()
                .map_err(|_| failure(ProviderErrorCode::InvalidRequest))?
        } else {
            0
        };
        let games: Vec<_> = (1..=24)
            .filter_map(|number| game(&GameId::new(format!("demo-{number:02}")).unwrap()))
            .filter(|game| game.title.as_str().to_lowercase().contains(&query))
            .collect();
        if start > games.len() {
            return Err(failure(ProviderErrorCode::InvalidRequest));
        }
        let items: Vec<_> = games
            .iter()
            .skip(start)
            .take(request.query.limit.into())
            .cloned()
            .collect();
        let end = start + items.len();
        Ok(GamePage {
            items: list(items),
            next_cursor: (end < games.len()).then(|| text(&format!("{end}:{context}"))),
            coverage: Coverage::Complete,
            revision: text(CATALOG_REVISION),
            scope: request.scope.clone(),
        })
    }
}

fn created(session: &SessionRecord) -> Completed {
    Completed {
        reply: ProviderReply::SessionCreate(CreateReply {
            session: session.view.clone(),
        }),
        allocation: Some(AllocationTicket {
            operation: session.operation.clone(),
            receipt: session.receipt.clone(),
            session: session.view.key.clone(),
        }),
    }
}
fn terminal(session: &SessionView) -> bool {
    matches!(
        session.state,
        RemoteSessionState::Finished { .. } | RemoteSessionState::Failed { .. }
    )
}
fn failure(code: ProviderErrorCode) -> ProviderError {
    ProviderError {
        code,
        retry_after_ms: None,
    }
}
fn text<const N: usize>(value: &str) -> Text<N> {
    Text::new(value).expect("bounded demo text")
}
fn list<T, const N: usize>(value: Vec<T>) -> List<T, N> {
    List::new(value).expect("bounded demo collection")
}
fn account_key(authority: &AuthorityId) -> AccountKey {
    AccountKey {
        authority: authority.clone(),
        account: AccountId::new("demo-player").unwrap(),
    }
}
fn challenge(record: &AuthRecord) -> AuthChallenge {
    AuthChallenge::Pairing {
        attempt: record.attempt.clone(),
        code: Some(SecretString::new("SDK-DEMO").unwrap()),
        expires_at_ms: record.expires_at_ms,
        poll_after_ms: 250,
    }
}
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}
fn game(id: &GameId) -> Option<GameSummary> {
    let number = id.as_str().strip_prefix("demo-")?.parse::<u8>().ok()?;
    if !(1..=24).contains(&number) || id.as_str() != format!("demo-{number:02}") {
        return None;
    }
    Some(GameSummary {
        id: id.clone(),
        title: text(&format!("OpenNOW SDK demo {number:02}")),
        artwork: None,
        subtitle: Some(text("Local generated media, not a cloud game")),
        badges: list(vec![text("SDK demonstration")]),
        availability: Availability::Available,
    })
}
fn check_target(target: &LaunchTarget) -> Result<(), ProviderError> {
    if game(&target.game).is_none() || target.variant.as_str() != "fixture" {
        return Err(failure(ProviderErrorCode::InvalidRequest));
    }
    Ok(())
}

pub fn fixture_video() -> VideoFormat {
    VideoFormat {
        encoding: VideoEncoding::H264AnnexB,
        width: 320,
        height: 180,
        fps: 50,
        bit_depth: 8,
        chroma: Chroma::Yuv420,
        color: ColorDescription {
            range: ColorRange::Limited,
            primaries: Primaries::Bt709,
            transfer: Transfer::Bt709,
            matrix: Matrix::Bt709,
            chroma_location: ChromaLocation::Left,
        },
    }
}

fn accepted_media(offer: &NativeOffer) -> Result<AcceptedMedia, ProviderError> {
    let accepted = AcceptedMedia {
        offer_id: offer.offer_id.clone(),
        runtime_epoch: offer.runtime_epoch,
        video: fixture_video(),
        audio: Some(AudioFormat {
            codec: AudioCodec::Opus,
            sample_rate: 48_000,
            channels: 2,
        }),
        input: InputCapabilities {
            keyboard: offer.input.keyboard,
            relative_mouse: false,
            absolute_mouse: false,
            text: false,
            gamepad_slots: offer.input.gamepad_slots.min(1),
            rumble: false,
        },
    };
    accepted
        .validate_against(offer, now_ms())
        .map_err(|_| failure(ProviderErrorCode::UnsupportedFeature))?;
    Ok(accepted)
}
