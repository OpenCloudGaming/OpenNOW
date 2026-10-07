use super::{BuiltinModule, GfnModule, SourceError, cloudmatch, service, store_requests};
use crate::requests::{self, Cancellation};
use crate::sources::contract::{
    AllocationDisposition, NativePreparation, ProviderCompletion, ProviderContext, ProviderSource,
};
use crate::streamer::StreamerService;
use opennow_plugin_api::provider::{
    self as api, ProviderReply as Reply, ProviderRequest as Request,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};

#[derive(Default)]
pub(super) struct ProviderState {
    attempt: Option<DeviceAttempt>,
    cursors: VecDeque<Cursor>,
    sessions: BTreeMap<SessionCorrelationKey, OwnedSession>,
    allocations: BTreeMap<String, Allocation>,
}

struct DeviceAttempt {
    challenge: api::AuthChallenge,
    remember: bool,
    completed: Option<api::AuthState>,
}

struct Cursor {
    token: String,
    method: &'static str,
    scope: api::CatalogScope,
    query: String,
    params: Value,
}

struct OwnedSession {
    view: api::SessionView,
    scope: api::AccountScope,
}

type SessionCorrelationKey = (Option<(String, String)>, String);

fn session_correlation_key(key: &api::SessionKey) -> SessionCorrelationKey {
    (
        key.account.as_ref().map(|account| {
            (
                account.authority.as_str().into(),
                account.account.as_str().into(),
            )
        }),
        key.remote_id.as_str().into(),
    )
}

struct Allocation {
    ticket: api::AllocationTicket,
    settled: Option<api::Acceptance>,
}

pub(super) fn capabilities() -> Vec<api::Capability> {
    use api::Capability::*;
    vec![
        AuthDeviceCode,
        Accounts,
        PublicCatalog,
        LibraryCatalog,
        StoreCatalog,
        CatalogDetails,
        Launch,
        Sessions,
    ]
}

impl ProviderSource for GfnModule {
    fn provider_capabilities(&self) -> Vec<api::Capability> {
        capabilities()
    }

    fn auth_kinds(&self) -> Vec<api::AuthKind> {
        vec![api::AuthKind::DeviceCode]
    }

    fn prepare_native(
        &self,
        query: &api::PrepareSession,
        context: &ProviderContext<'_>,
    ) -> Result<NativePreparation, SourceError> {
        self.prepare_provider_session(query, context)
            .map(NativePreparation::Gfn)
    }

    fn provider_call(
        &self,
        request: &Request,
        context: &ProviderContext<'_>,
    ) -> ProviderCompletion {
        let mut disposition = AllocationDisposition::NotDispatched;
        let result = self.execute_provider_tracked(request, context, &mut disposition);
        let allocation = if let Request::SessionCreate(query) = request
            && matches!(disposition, AllocationDisposition::Allocated { .. })
        {
            requests::scope(Cancellation::default(), || {
                self.provider_allocation_ticket(&query.operation)
            })
            .ok()
        } else {
            None
        };
        let mut effects = Vec::new();
        if let Ok(reply) = &result {
            match reply {
                Reply::AuthComplete(_)
                | Reply::AuthLogout(_)
                | Reply::AccountsSelect(_)
                | Reply::AccountsRemove(_) => effects.push(api::ProviderEffect::AuthChanged {
                    revision: self.service.auth_generation(),
                }),
                Reply::SessionPoll(session) | Reply::SessionClaim(session) => {
                    effects.push(api::ProviderEffect::SessionChanged {
                        session: session.clone(),
                    })
                }
                _ => (),
            }
        }
        ProviderCompletion {
            result,
            effects,
            allocation,
            dispatched: !matches!(request, Request::SessionCreate(_))
                || disposition != AllocationDisposition::NotDispatched,
            allocation_disposition: matches!(request, Request::SessionCreate(_))
                .then_some(disposition),
            dispatched_generation: None,
        }
    }
}

impl GfnModule {
    #[cfg(test)]
    pub(crate) fn execute_provider(
        &self,
        request: &Request,
        cancellation: &Cancellation,
        runtime_capabilities: Option<&Value>,
    ) -> Result<Reply, SourceError> {
        self.execute_provider_tracked(
            request,
            &ProviderContext {
                cancellation,
                runtime_capabilities,
                gfn_settings: None,
            },
            &mut AllocationDisposition::NotDispatched,
        )
    }

    fn execute_provider_tracked(
        &self,
        request: &Request,
        context: &ProviderContext<'_>,
        disposition: &mut AllocationDisposition,
    ) -> Result<Reply, SourceError> {
        request.validate().map_err(invalid_request)?;
        requests::scope(context.cancellation.clone(), || {
            context.cancellation.check()?;
            let result = if matches!(
                request,
                Request::Hello(_)
                    | Request::AuthAuthorities(_)
                    | Request::AuthStatus(_)
                    | Request::AccountsList(_)
                    | Request::CatalogPublic(_)
                    | Request::CatalogLibrary(_)
                    | Request::CatalogStore(_)
                    | Request::CatalogDetails(_)
                    | Request::LaunchInspect(_)
            ) {
                self.execute_provider_read(request, context)
            } else {
                let mut state = store_requests::lock(&self.provider)?;
                self.execute_provider_locked(request, &mut state, context, disposition)
            };
            if matches!(
                request,
                Request::AuthComplete(_)
                    | Request::AuthLogout(_)
                    | Request::AccountsSelect(_)
                    | Request::AccountsRemove(_)
            ) {
                self.settings_changed();
            }
            let reply = result?;
            reply.validate().map_err(invalid_response)?;
            Ok(reply)
        })
    }

    fn execute_provider_read(
        &self,
        request: &Request,
        context: &ProviderContext<'_>,
    ) -> Result<Reply, SourceError> {
        match request {
            Request::Hello(hello) => {
                if hello.plugin_id.as_str() != opennow_plugin_api::BUILTIN_GFN_ID {
                    return Err(scope_changed());
                }
                Ok(Reply::Hello(api::ProviderHelloReply {
                    plugin_id: hello.plugin_id.clone(),
                    version: text(crate::version::APPLICATION_VERSION)?,
                    protocol_version: api::PROVIDER_PROTOCOL_VERSION,
                    capabilities: list(capabilities())?,
                    auth_kinds: list(vec![api::AuthKind::DeviceCode])?,
                }))
            }
            Request::AuthAuthorities(_) => {
                let result = self.service.providers()?;
                Ok(Reply::AuthAuthorities(list(
                    array(&result, "providers")?
                        .iter()
                        .map(|authority| {
                            Ok(api::Authority {
                                id: api::AuthorityId::new(string(authority, "idpId")?)
                                    .map_err(invalid_response)?,
                                name: text(string(authority, "displayName")?)?,
                            })
                        })
                        .collect::<Result<_, SourceError>>()?,
                )?))
            }
            Request::AuthStatus(_) => Ok(Reply::AuthStatus(auth_state(&self.service.session()?)?)),
            Request::AccountsList(_) => Ok(Reply::AccountsList(self.provider_accounts()?)),
            Request::CatalogPublic(query)
            | Request::CatalogLibrary(query)
            | Request::CatalogStore(query) => {
                let public = matches!(request, Request::CatalogPublic(_));
                if public != matches!(query.scope, api::CatalogScope::Public) {
                    return Err(scope_changed());
                }
                if let api::CatalogScope::Account { scope } = &query.scope {
                    self.require_scope(scope)?;
                }
                if matches!(request, Request::CatalogLibrary(_)) && !query.query.query.is_empty() {
                    return Err(SourceError::new(
                        "unsupported_feature",
                        "GFN library search is not available in this catalog operation",
                    ));
                }
                let mut params = json!({"limit":query.query.limit,"searchQuery":query.query.query});
                if let Some(cursor) = &query.query.cursor {
                    let state = store_requests::lock(&self.provider)?;
                    let saved = state
                        .cursors
                        .iter()
                        .find(|saved| {
                            saved.token == *cursor
                                && saved.method == request.method()
                                && saved.scope == query.scope
                                && saved.query == query.query.query
                        })
                        .ok_or_else(|| {
                            SourceError::new("catalog_changed", "Restart the catalog traversal")
                        })?;
                    for (key, value) in saved.params.as_object().ok_or_else(invalid_data)? {
                        params[key] = value.clone();
                    }
                }
                let settings = self.provider_settings(context);
                let value = match request {
                    Request::CatalogPublic(_) => {
                        if query.query.cursor.is_some() {
                            return Err(unsupported());
                        }
                        self.service.public_catalog(&params, &settings)?
                    }
                    Request::CatalogLibrary(_) => {
                        self.service.library_catalog(&params, &settings)?
                    }
                    _ => self.service.store_catalog(&params, &settings)?,
                };
                check_catalog_scope(&query.scope, &value)?;
                let next_cursor = if value["hasNextPage"] == true {
                    let token = random_id();
                    let upstream = string(&value, "nextCursor")?;
                    if upstream.is_empty() {
                        return Err(invalid_data());
                    }
                    let mut state = store_requests::lock(&self.provider)?;
                    if state.cursors.len() == 64 {
                        state.cursors.pop_front();
                    }
                    state.cursors.push_back(Cursor { token: token.clone(), method: request.method(), scope: query.scope.clone(), query: query.query.query.clone(), params: json!({"cursor":upstream,"catalogRevision":value["catalogRevision"],"catalogContext":value["catalogContext"]}) });
                    Some(text(token)?)
                } else {
                    None
                };
                let page = api::GamePage {
                    items: list(
                        array(&value, "games")?
                            .iter()
                            .map(|game| game_summary(game, public))
                            .collect::<Result<_, _>>()?,
                    )?,
                    next_cursor,
                    coverage: opennow_plugin_api::Coverage::Unknown,
                    revision: catalog_revision(&value, public)?,
                    scope: query.scope.clone(),
                };
                Ok(match request {
                    Request::CatalogPublic(_) => Reply::CatalogPublic(page),
                    Request::CatalogLibrary(_) => Reply::CatalogLibrary(page),
                    _ => Reply::CatalogStore(page),
                })
            }
            Request::CatalogDetails(query) => {
                let api::CatalogScope::Account { scope } = &query.scope else {
                    return Err(unsupported());
                };
                self.require_scope(scope)?;
                canonical_game(&query.game)?;
                let settings = self.provider_settings(context);
                let value = self
                    .service
                    .catalog_game(&json!({"appId":query.game}), &settings)?;
                check_catalog_scope(&query.scope, &value)?;
                Ok(Reply::CatalogDetails(game_details(
                    &value,
                    query.scope.clone(),
                )?))
            }
            Request::LaunchInspect(query) => {
                let scope = required_scope(query.scope.as_ref())?;
                self.require_scope(scope)?;
                canonical_game(&query.target.game)?;
                let settings = self.provider_settings(context);
                let value = self.service.catalog_launch_inspect(
                    &json!({"appId":query.target.game,"variantId":query.target.variant}),
                    &settings,
                )?;
                check_scope(scope, &value["scope"])?;
                let revision = catalog_revision(&value, false)?;
                if revision != query.catalog_revision {
                    return Err(catalog_changed());
                }
                Ok(Reply::LaunchInspect(launch_decision(
                    &value,
                    query.target.clone(),
                    revision,
                )?))
            }
            _ => Err(unsupported()),
        }
    }

    fn execute_provider_locked(
        &self,
        request: &Request,
        state: &mut ProviderState,
        context: &ProviderContext<'_>,
        disposition: &mut AllocationDisposition,
    ) -> Result<Reply, SourceError> {
        match request {
            Request::Shutdown(_) => {
                self.shutdown();
                Ok(Reply::Shutdown(api::Empty {}))
            }
            Request::AuthBegin(begin) => {
                if begin.kind != api::AuthKind::DeviceCode {
                    return Err(unsupported());
                }
                let value = self
                    .service
                    .start_device_login(&json!({"providerIdpId":begin.authority}))?;
                let challenge = device_challenge(&value)?;
                state.attempt = Some(DeviceAttempt {
                    challenge: challenge.clone(),
                    remember: begin.remember,
                    completed: None,
                });
                Ok(Reply::AuthBegin(api::AuthState::Pending { challenge }))
            }
            Request::AuthPoll(query) => {
                let attempt = attempt_mut(state, &query.attempt)?;
                if attempt.completed.is_some() {
                    return Ok(Reply::AuthPoll(
                        self.complete_provider_auth(attempt, &query.attempt)?,
                    ));
                }
                let value = self
                    .service
                    .poll_device_login(&json!({"attemptId":query.attempt}))?;
                let auth = match string(&value, "status")? {
                    "authorized" => api::AuthState::Authorized {
                        attempt: query.attempt.clone(),
                    },
                    "pending" | "slow_down" => {
                        if let api::AuthChallenge::DeviceCode { poll_after_ms, .. } =
                            &mut attempt.challenge
                        {
                            *poll_after_ms = poll_delay(&value)?;
                        }
                        api::AuthState::Pending {
                            challenge: attempt.challenge.clone(),
                        }
                    }
                    "expired" => {
                        state.attempt = None;
                        api::AuthState::SignedOut
                    }
                    _ => {
                        return Err(SourceError::new(
                            "authentication_failed",
                            "Device authorization was declined or failed",
                        ));
                    }
                };
                Ok(Reply::AuthPoll(auth))
            }
            Request::AuthComplete(query) => {
                if query.proof.is_some() {
                    return Err(SourceError::new(
                        "invalid_params",
                        "Device authorization does not accept client credentials",
                    ));
                }
                let attempt = attempt_mut(state, &query.attempt)?;
                Ok(Reply::AuthComplete(
                    self.complete_provider_auth(attempt, &query.attempt)?,
                ))
            }
            Request::AuthCancel(query) => {
                self.service
                    .cancel_device_login(&json!({"attemptId":query.attempt}))?;
                if attempt_mut(state, &query.attempt).is_ok() {
                    state.attempt = None;
                }
                Ok(Reply::AuthCancel(api::Empty {}))
            }
            Request::AuthLogout(account) => {
                self.require_account(account)?;
                let value = self.service.logout_owned(
                    &json!({"userId":account.account,"providerIdpId":account.authority}),
                )?;
                state.attempt = None;
                Ok(Reply::AuthLogout(auth_state(&value)?))
            }
            Request::AccountsSelect(query) => {
                self.require_saved_account(&query.account)?;
                let value = self.service.switch_account(&json!({"userId":query.account.account,"providerIdpId":query.account.authority,"pin":query.pin.as_ref().map(api::SecretString::expose_secret)}))?;
                state.attempt = None;
                Ok(Reply::AccountsSelect(auth_state(&value)?))
            }
            Request::AccountsRemove(account) => {
                self.require_saved_account(account)?;
                self.service.remove_account(
                    &json!({"userId":account.account,"providerIdpId":account.authority}),
                )?;
                state.attempt = None;
                Ok(Reply::AccountsRemove(self.provider_accounts()?))
            }
            Request::SessionCreate(query) => {
                let scope = required_scope(query.scope.as_ref())?;
                self.require_scope(scope)?;
                canonical_game(&query.target.game)?;
                if state.allocations.contains_key(query.operation.as_str()) {
                    return Err(SourceError::new(
                        "operation_exists",
                        "Reconcile the existing allocation before retrying",
                    ));
                }
                state.sessions.retain(|_, owned| {
                    !matches!(owned.view.state, api::RemoteSessionState::Finished { .. })
                });
                if state.allocations.len() >= 64 {
                    if let Some(operation) = state
                        .allocations
                        .iter()
                        .find(|(_, allocation)| allocation.settled.is_some())
                        .map(|(operation, _)| operation.clone())
                    {
                        state.allocations.remove(&operation);
                    }
                }
                if state.allocations.len() >= 64 || state.sessions.len() >= 32 {
                    return Err(SourceError::new(
                        "session_update_busy",
                        "Too many outstanding session operations",
                    ));
                }
                let settings = self.provider_settings(context);
                let (auth, generation) = self
                    .service
                    .authenticated_snapshot(service::TokenPurpose::ServiceId, false)?;
                if auth.user.user_id != scope.account.account.as_str()
                    || auth.provider.idp_id != scope.account.authority.as_str()
                    || generation != scope.revision
                {
                    return Err(scope_changed());
                }
                let settings = Self::apply_stream_preferences(&settings, &query.preferences);
                let settings = cloudmatch::allocation_settings(&settings, &auth);
                let settings = if let Some(runtime) = context.runtime_capabilities {
                    StreamerService::embedded_session_settings(&settings, runtime)
                        .map_err(|error| SourceError::new(error.code, error.message))?
                } else {
                    self.streamer
                        .validate_codec(&settings)
                        .map_err(|error| SourceError::new(error.code, error.message))?;
                    settings
                };
                let inspection = self.service.catalog_launch_inspect(
                    &json!({"appId":query.target.game,"variantId":query.target.variant}),
                    &settings,
                )?;
                check_scope(scope, &inspection["scope"])?;
                if catalog_revision(&inspection, false)? != query.catalog_revision {
                    return Err(catalog_changed());
                }
                let result = self.service.create_session_tracked(&json!({"appId":query.target.variant,"variantId":query.target.variant,"catalogAppId":query.target.game,"scope":legacy_scope(scope),"runtimeCapabilities":context.runtime_capabilities}), &settings, disposition);
                if let AllocationDisposition::Allocated { session_id } = disposition {
                    let remote_id = match api::SessionId::new(session_id.clone()) {
                        Ok(id) => id,
                        Err(error) => {
                            requests::scope(Cancellation::default(), || {
                                self.service.finish_session_create(session_id, false)
                            })?;
                            *disposition = AllocationDisposition::CleanedUp {
                                session_id: session_id.clone(),
                            };
                            return Err(invalid_response(error));
                        }
                    };
                    state.allocations.insert(
                        query.operation.as_str().into(),
                        Allocation {
                            ticket: api::AllocationTicket {
                                operation: query.operation.clone(),
                                receipt: api::ReceiptId::new(random_id())
                                    .map_err(invalid_response)?,
                                session: api::SessionKey {
                                    account: Some(scope.account.clone()),
                                    remote_id,
                                },
                            },
                            settled: None,
                        },
                    );
                }
                let value = result?;
                let view = session_view(&value["session"], scope, query.target.clone())?;
                state.sessions.insert(
                    session_correlation_key(&view.key),
                    OwnedSession {
                        view: view.clone(),
                        scope: scope.clone(),
                    },
                );
                Ok(Reply::SessionCreate(api::CreateReply { session: view }))
            }
            Request::SessionPoll(key) => {
                let owned = owned_session(state, key)?;
                let value = self.service.poll_session(&session_params(owned))?;
                let view = polled_session(&value, owned)?;
                if view.key != *key {
                    return Err(scope_changed());
                }
                state
                    .sessions
                    .get_mut(&session_correlation_key(key))
                    .expect("owned session")
                    .view = view.clone();
                Ok(Reply::SessionPoll(view))
            }
            Request::SessionDiscover(query) => {
                let scope = required_scope(query.scope.as_ref())?;
                self.require_scope(scope)?;
                let settings = self.provider_settings(context);
                let value = self
                    .service
                    .remote_sessions(&json!({"ownerScope":legacy_scope(scope)}), &settings)?;
                check_scope(scope, &value["scope"])?;
                let mut discovered = Vec::new();
                for remote in array(&value, "sessions")? {
                    let target = self.remote_target(remote, &settings, scope)?;
                    discovered.push(session_view(remote, scope, target)?);
                }
                let discovered = list(discovered)?;
                if state.sessions.len()
                    + discovered
                        .iter()
                        .filter(|view| {
                            !state
                                .sessions
                                .contains_key(&session_correlation_key(&view.key))
                        })
                        .count()
                    > 32
                {
                    return Err(SourceError::new(
                        "session_update_busy",
                        "Too many discovered sessions",
                    ));
                }
                for view in discovered.iter() {
                    state.sessions.insert(
                        session_correlation_key(&view.key),
                        OwnedSession {
                            view: view.clone(),
                            scope: scope.clone(),
                        },
                    );
                }
                Ok(Reply::SessionDiscover(discovered))
            }
            Request::SessionClaim(query) => {
                let scope = required_scope(query.scope.as_ref())?;
                self.require_scope(scope)?;
                let owned = owned_session(state, &query.session)?;
                if owned.scope != *scope {
                    return Err(scope_changed());
                }
                let settings = self.provider_settings(context);
                let value = self
                    .service
                    .claim_session(&session_params(owned), &settings)?;
                let view = session_view(&value["session"], scope, owned.view.target.clone())?;
                if view.key != query.session {
                    return Err(scope_changed());
                }
                state
                    .sessions
                    .get_mut(&session_correlation_key(&view.key))
                    .expect("owned session")
                    .view = view.clone();
                Ok(Reply::SessionClaim(view))
            }
            Request::SessionReconcile(query) => {
                let scope = required_scope(query.scope.as_ref())?;
                self.require_scope(scope)?;
                let key = query.session.as_ref().or_else(|| {
                    state
                        .allocations
                        .get(query.operation.as_str())
                        .map(|allocation| &allocation.ticket.session)
                });
                let Some(key) = key else {
                    return Ok(Reply::SessionReconcile(api::Reconciliation::Unknown {
                        operation: query.operation.clone(),
                    }));
                };
                if key.account.as_ref() != Some(&scope.account) {
                    return Err(scope_changed());
                }
                let settings = self.provider_settings(context);
                let value = self.service.reconcile_active_session(
                    &json!({"sessionId":key.remote_id,"ownerScope":legacy_scope(scope)}),
                    &settings,
                )?;
                if value["session"].is_null() && confirmed_termination(&value, key) {
                    return Ok(Reply::SessionReconcile(api::Reconciliation::Terminal {
                        session: key.clone(),
                        reason: api::TerminalReason::RemoteEnded,
                    }));
                }
                if value["session"].is_null() {
                    return Ok(Reply::SessionReconcile(api::Reconciliation::Unknown {
                        operation: query.operation.clone(),
                    }));
                }
                let target = match state.sessions.get(&session_correlation_key(key)) {
                    Some(owned) if owned.view.key == *key => owned.view.target.clone(),
                    _ => self.remote_target(&value["session"], &settings, scope)?,
                };
                let view = session_view(&value["session"], scope, target)?;
                if view.key != *key {
                    return Err(scope_changed());
                }
                let reply = if let api::RemoteSessionState::Finished { reason } = view.state {
                    api::Reconciliation::Terminal {
                        session: view.key.clone(),
                        reason,
                    }
                } else {
                    api::Reconciliation::Active {
                        session: view.clone(),
                    }
                };
                if state.sessions.len() >= 32
                    && !state
                        .sessions
                        .contains_key(&session_correlation_key(&view.key))
                {
                    return Err(SourceError::new(
                        "session_update_busy",
                        "Too many tracked sessions",
                    ));
                }
                state.sessions.insert(
                    session_correlation_key(&view.key),
                    OwnedSession {
                        view,
                        scope: scope.clone(),
                    },
                );
                Ok(Reply::SessionReconcile(reply))
            }
            Request::SessionStop(query) => {
                let owned = owned_session(state, &query.session)?;
                let settings = self.provider_settings(context);
                self.service
                    .stop_session(&session_params(owned), &settings)?;
                state
                    .sessions
                    .get_mut(&session_correlation_key(&query.session))
                    .expect("owned session")
                    .view
                    .state = api::RemoteSessionState::Finished {
                    reason: api::TerminalReason::UserStopped,
                };
                Ok(Reply::SessionStop(api::CleanupState::Resolved))
            }
            Request::SessionResolveAllocation(query) => {
                let allocation = state
                    .allocations
                    .get_mut(query.operation.as_str())
                    .filter(|allocation| allocation.ticket.receipt == query.receipt)
                    .ok_or_else(|| {
                        SourceError::new(
                            "invalid_receipt",
                            "Allocation receipt does not match the original operation",
                        )
                    })?;
                if let Some(decision) = allocation.settled {
                    if decision != query.decision {
                        return Err(SourceError::new(
                            "invalid_receipt",
                            "Allocation already resolved with a different decision",
                        ));
                    }
                    return Ok(Reply::SessionResolveAllocation(api::CleanupState::Resolved));
                }
                self.service.finish_session_create(
                    allocation.ticket.session.remote_id.as_str(),
                    query.decision == api::Acceptance::Accepted,
                )?;
                if query.decision == api::Acceptance::Accepted
                    && self
                        .service
                        .session_owner_scope(allocation.ticket.session.remote_id.as_str())?
                        .is_none()
                {
                    allocation.settled = Some(api::Acceptance::Rejected);
                    return Err(scope_changed());
                }
                allocation.settled = Some(query.decision);
                Ok(Reply::SessionResolveAllocation(api::CleanupState::Resolved))
            }
            _ => Err(unsupported()),
        }
    }

    fn complete_provider_auth(
        &self,
        attempt: &mut DeviceAttempt,
        id: &api::AttemptId,
    ) -> Result<api::AuthState, SourceError> {
        if let Some(completed) = &attempt.completed {
            if auth_state(&self.service.session()?)? != *completed {
                return Err(scope_changed());
            }
            return Ok(completed.clone());
        }
        let value = self
            .service
            .complete_device_login(&json!({"attemptId":id,"staySignedIn":attempt.remember}))?;
        let auth = auth_state(&value)?;
        attempt.completed = Some(auth.clone());
        Ok(auth)
    }

    fn require_account(&self, expected: &api::AccountKey) -> Result<(), SourceError> {
        match auth_state(&self.service.session()?)? {
            api::AuthState::SignedIn { account, .. } if account.key == *expected => Ok(()),
            _ => Err(scope_changed()),
        }
    }

    fn require_scope(&self, expected: &api::AccountScope) -> Result<(), SourceError> {
        match auth_state(&self.service.session()?)? {
            api::AuthState::SignedIn { account, revision }
                if account.key == expected.account && revision == expected.revision =>
            {
                Ok(())
            }
            _ => Err(scope_changed()),
        }
    }

    fn require_saved_account(&self, expected: &api::AccountKey) -> Result<(), SourceError> {
        if self
            .provider_accounts()?
            .accounts
            .iter()
            .any(|account| account.key == *expected)
        {
            Ok(())
        } else {
            Err(scope_changed())
        }
    }

    fn provider_accounts(&self) -> Result<api::Accounts, SourceError> {
        let snapshot = self.service.session()?;
        let current = auth_state(&snapshot)?;
        let value = self.service.provider_saved_accounts()?;
        if number(&value, "generation")? != number(&snapshot, "generation")? {
            return Err(scope_changed());
        }
        let mut accounts = array(&value, "accounts")?
            .iter()
            .map(|account| {
                Ok(api::PublicAccount {
                    key: account_key(
                        string(account, "providerIdpId")?,
                        string(account, "userId")?,
                    )?,
                    name: text(string(account, "displayName")?)?,
                    persistence: api::Persistence::Durable,
                    reauthentication_required: account["reauthenticationRequired"] == true,
                    pin_locked: account["hasPin"] == true,
                })
            })
            .collect::<Result<Vec<_>, SourceError>>()?;
        let selected = if let api::AuthState::SignedIn { account, .. } = current {
            accounts.retain(|saved| saved.key != account.key);
            let key = account.key.clone();
            accounts.push(account);
            Some(key)
        } else {
            None
        };
        Ok(api::Accounts {
            accounts: list(accounts)?,
            selected,
            revision: number(&value, "generation")?,
        })
    }

    fn remote_target(
        &self,
        remote: &Value,
        settings: &Value,
        scope: &api::AccountScope,
    ) -> Result<api::LaunchTarget, SourceError> {
        let id = remote["appId"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| remote["appId"].as_u64().map(|id| id.to_string()))
            .ok_or_else(invalid_data)?;
        let result = self
            .service
            .catalog_game(&json!({"variantId":id}), settings)?;
        check_scope(scope, &result["scope"])?;
        if !array(&result["game"], "variants")?
            .iter()
            .any(|variant| variant["id"] == id)
        {
            return Err(invalid_data());
        }
        Ok(api::LaunchTarget {
            game: api::GameId::new(string(&result["game"], "id")?).map_err(invalid_response)?,
            variant: api::VariantId::new(id).map_err(invalid_response)?,
        })
    }

    pub(crate) fn provider_allocation_ticket(
        &self,
        operation: &api::OperationId,
    ) -> Result<api::AllocationTicket, SourceError> {
        store_requests::lock(&self.provider)?
            .allocations
            .get(operation.as_str())
            .map(|allocation| allocation.ticket.clone())
            .ok_or_else(|| SourceError::new("invalid_receipt", "Allocation operation is not known"))
    }

    pub(crate) fn prepare_provider_session(
        &self,
        query: &api::PrepareSession,
        context: &ProviderContext<'_>,
    ) -> Result<Value, SourceError> {
        query.offer.validate().map_err(invalid_request)?;
        requests::scope(context.cancellation.clone(), || {
            let state = store_requests::lock(&self.provider)?;
            let owned = owned_session(&state, &query.session)?;
            let settings = self.provider_settings(context);
            self.service.prepare_owned_stream(&json!({"session":session_params(owned),"runtimeCapabilities":context.runtime_capabilities}), |params| {
                self.streamer.prepare_embedded(params, &settings).map_err(|error| crate::service_error::ServiceError { code: error.code, message: error.message })
            }).map_err(SourceError::from)
        })
    }

    fn provider_settings(&self, context: &ProviderContext<'_>) -> Value {
        context
            .gfn_settings
            .cloned()
            .unwrap_or_else(|| self.settings.lock().expect("settings poisoned").all())
    }

    pub(crate) fn apply_stream_preferences(
        settings: &Value,
        preferences: &api::StreamPreferences,
    ) -> Value {
        use opennow_plugin_api::media::{Chroma, VideoEncoding};
        let mut settings = settings.clone();
        let video = &preferences.video;
        settings["resolution"] = json!(format!("{}x{}", video.width, video.height));
        settings["codec"] = json!(match video.encoding {
            None => "auto",
            Some(VideoEncoding::H264AnnexB) => "h264",
            Some(VideoEncoding::HevcAnnexB) => "h265",
            Some(VideoEncoding::Av1Obu) => "av1",
        });
        settings["fps"] = video
            .fps
            .map(|fps| json!(fps))
            .unwrap_or_else(|| json!("auto"));
        settings["colorQuality"] = json!(format!(
            "{}bit_{}",
            video.bit_depth,
            match video.chroma {
                Chroma::Yuv420 => "420",
                Chroma::Yuv444 => "444",
            }
        ));
        settings["enableHdr"] = json!(video.hdr);
        settings["maxBitrateMbps"] = json!(f64::from(preferences.bitrate_kbps) / 1000.0);
        settings
    }
}

fn attempt_mut<'a>(
    state: &'a mut ProviderState,
    id: &api::AttemptId,
) -> Result<&'a mut DeviceAttempt, SourceError> {
    state.attempt.as_mut().filter(|attempt| matches!(&attempt.challenge, api::AuthChallenge::DeviceCode { attempt, .. } if attempt == id)).ok_or_else(|| SourceError::new("authentication_failed", "Device authorization was replaced or expired"))
}

fn device_challenge(value: &Value) -> Result<api::AuthChallenge, SourceError> {
    Ok(api::AuthChallenge::DeviceCode {
        attempt: api::AttemptId::new(string(value, "attemptId")?).map_err(invalid_response)?,
        user_code: api::SecretString::new(string(value, "userCode")?).map_err(invalid_response)?,
        verification_uri: api::PublicUrl::new(string(value, "verificationUri")?.into())
            .map_err(invalid_response)?,
        expires_at_ms: number(value, "expiresAt")?,
        poll_after_ms: poll_delay(value)?,
    })
}

fn poll_delay(value: &Value) -> Result<u32, SourceError> {
    let delay = value["retryAfterMs"]
        .as_u64()
        .or_else(|| {
            value["intervalSeconds"]
                .as_u64()
                .and_then(|seconds| seconds.checked_mul(1000))
        })
        .ok_or_else(invalid_data)?;
    u32::try_from(delay).map_err(|_| invalid_data())
}

fn auth_state(value: &Value) -> Result<api::AuthState, SourceError> {
    if value["session"].is_null() {
        return Ok(api::AuthState::SignedOut);
    }
    let session = &value["session"];
    Ok(api::AuthState::SignedIn {
        account: api::PublicAccount {
            key: account_key(
                string(&session["provider"], "idpId")?,
                string(&session["user"], "userId")?,
            )?,
            name: text(string(&session["user"], "displayName")?)?,
            persistence: match value["persistence"].as_str() {
                Some("local-file" | "secure-store") => api::Persistence::Durable,
                Some("memory-only") => api::Persistence::Temporary,
                _ => api::Persistence::Unavailable,
            },
            reauthentication_required: false,
            pin_locked: false,
        },
        revision: number(value, "generation")?,
    })
}

fn account_key(authority: &str, account: &str) -> Result<api::AccountKey, SourceError> {
    Ok(api::AccountKey {
        authority: api::AuthorityId::new(authority).map_err(invalid_response)?,
        account: api::AccountId::new(account).map_err(invalid_response)?,
    })
}

fn required_scope(scope: Option<&api::AccountScope>) -> Result<&api::AccountScope, SourceError> {
    scope.ok_or_else(|| SourceError::new("authentication_required", "Sign in to GeForce NOW first"))
}

fn legacy_scope(scope: &api::AccountScope) -> Value {
    json!({"providerIdpId":scope.account.authority,"userId":scope.account.account,"generation":scope.revision})
}

fn check_scope(scope: &api::AccountScope, value: &Value) -> Result<(), SourceError> {
    if value != &legacy_scope(scope) {
        return Err(scope_changed());
    }
    Ok(())
}

fn check_catalog_scope(scope: &api::CatalogScope, value: &Value) -> Result<(), SourceError> {
    if let api::CatalogScope::Account { scope } = scope {
        check_scope(scope, &value["scope"])?;
    }
    Ok(())
}

fn canonical_game(id: &api::GameId) -> Result<(), SourceError> {
    if id.as_str().starts_with("public:") {
        return Err(SourceError::new(
            "invalid_params",
            "Select the exact game from the signed-in catalog before launching",
        ));
    }
    Ok(())
}

fn catalog_revision(value: &Value, public: bool) -> Result<api::Text<256>, SourceError> {
    if public {
        return text("public");
    }
    text(number(value, "catalogRevision")?.to_string())
}

fn game_summary(game: &Value, public: bool) -> Result<api::GameSummary, SourceError> {
    let id = if public {
        game["uuid"].as_str().unwrap_or(string(game, "id")?)
    } else {
        string(game, "id")?
    };
    Ok(api::GameSummary {
        id: api::GameId::new(if public {
            format!("public:{id}")
        } else {
            id.into()
        })
        .map_err(invalid_response)?,
        title: text(string(game, "title")?)?,
        artwork: game["imageUrl"]
            .as_str()
            .filter(|value| !value.is_empty())
            .map(|url| api::PublicUrl::new(url.into()))
            .transpose()
            .map_err(invalid_response)?,
        subtitle: None,
        badges: api::List::default(),
        availability: match game["playabilityState"].as_str() {
            Some("PLAYABLE") => api::Availability::Available,
            Some("UNPLAYABLE_DUE_TO_UPGRADE" | "UNPLAYABLE_DUE_TO_TIME_CAPPED_LIMIT") => {
                api::Availability::SubscriptionRequired
            }
            _ => api::Availability::Unknown,
        },
    })
}

fn game_details(value: &Value, scope: api::CatalogScope) -> Result<api::GameDetails, SourceError> {
    let game = &value["game"];
    Ok(api::GameDetails {
        game: game_summary(game, false)?,
        description: game["shortDescription"]
            .as_str()
            .or_else(|| game["description"].as_str())
            .map(|description| description.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|description| !description.is_empty())
            .map(text)
            .transpose()?,
        variants: list(
            array(game, "variants")?
                .iter()
                .map(|variant| {
                    Ok(api::GameVariant {
                        id: api::VariantId::new(string(variant, "id")?)
                            .map_err(invalid_response)?,
                        label: text(string(variant, "store")?)?,
                        availability: match variant["gfnStatus"].as_str() {
                            Some("AVAILABLE") => api::Availability::Available,
                            Some("PATCHING") => api::Availability::Patching,
                            Some("SERVER_MAINTENANCE") => api::Availability::Maintenance,
                            Some(_) => api::Availability::Unavailable,
                            None => api::Availability::Unknown,
                        },
                    })
                })
                .collect::<Result<_, SourceError>>()?,
        )?,
        revision: catalog_revision(value, false)?,
        scope,
    })
}

fn launch_decision(
    value: &Value,
    target: api::LaunchTarget,
    revision: api::Text<256>,
) -> Result<api::LaunchDecision, SourceError> {
    let decision = &value["decision"];
    let status = string(decision, "status")?;
    if status == "ready" {
        if value["game"]["id"] != target.game.as_str()
            || !array(&value["game"], "variants")?
                .iter()
                .any(|variant| variant["id"] == target.variant.as_str())
        {
            return Err(invalid_data());
        }
        return Ok(api::LaunchDecision::Ready { target, revision });
    }
    Ok(api::LaunchDecision::Blocked {
        reason: match status {
            "ownership_required" => api::Availability::OwnershipRequired,
            "link_required" => api::Availability::AccountLinkRequired,
            "subscription_required" => api::Availability::SubscriptionRequired,
            "patching" => api::Availability::Patching,
            "maintenance" => api::Availability::Maintenance,
            "unavailable" => api::Availability::Unavailable,
            _ => api::Availability::Unknown,
        },
        message: text(string(decision, "message")?)?,
    })
}

fn owned_session<'a>(
    state: &'a ProviderState,
    key: &api::SessionKey,
) -> Result<&'a OwnedSession, SourceError> {
    state
        .sessions
        .get(&session_correlation_key(key))
        .filter(|owned| owned.view.key == *key)
        .ok_or_else(|| {
            SourceError::new(
                "session_owner_mismatch",
                "The session has not been claimed for this account",
            )
        })
}

fn session_params(owned: &OwnedSession) -> Value {
    json!({"sessionId":owned.view.key.remote_id,"ownerScope":legacy_scope(&owned.scope)})
}

fn confirmed_termination(value: &Value, key: &api::SessionKey) -> bool {
    value["termination"]["sessionId"] == key.remote_id.as_str()
        && value["termination"]["httpStatus"] == 404
        && value["termination"]["resumable"] == false
}

fn polled_session(value: &Value, owned: &OwnedSession) -> Result<api::SessionView, SourceError> {
    if value["session"].is_null() && confirmed_termination(value, &owned.view.key) {
        let mut view = owned.view.clone();
        view.state = api::RemoteSessionState::Finished {
            reason: api::TerminalReason::RemoteEnded,
        };
        return Ok(view);
    }
    session_view(&value["session"], &owned.scope, owned.view.target.clone())
}

fn session_view(
    value: &Value,
    scope: &api::AccountScope,
    target: api::LaunchTarget,
) -> Result<api::SessionView, SourceError> {
    if !value["ownerScope"].is_null() {
        let owner = &value["ownerScope"];
        if owner["providerIdpId"] != scope.account.authority.as_str()
            || owner["userId"] != scope.account.account.as_str()
        {
            return Err(scope_changed());
        }
    }
    let status = value["status"].as_i64().ok_or_else(invalid_data)?;
    let state = match status {
        1 if value["queuePosition"].as_u64().is_some() => api::RemoteSessionState::Queued {
            position: value["queuePosition"]
                .as_u64()
                .and_then(|position| u32::try_from(position).ok()),
            wait_seconds: None,
        },
        0 | 1 | 6 => api::RemoteSessionState::Allocating,
        2 | 3 => api::RemoteSessionState::Ready,
        4 | 5 => api::RemoteSessionState::Suspended,
        7 => api::RemoteSessionState::Finished {
            reason: api::TerminalReason::RemoteEnded,
        },
        _ => api::RemoteSessionState::Failed {
            error: api::ProviderErrorCode::ServiceUnavailable,
        },
    };
    Ok(api::SessionView {
        key: api::SessionKey {
            account: Some(scope.account.clone()),
            remote_id: api::SessionId::new(string(value, "sessionId")?)
                .map_err(invalid_response)?,
        },
        target,
        state,
    })
}

fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str, SourceError> {
    value[key].as_str().ok_or_else(invalid_data)
}
fn number(value: &Value, key: &str) -> Result<u64, SourceError> {
    value[key].as_u64().ok_or_else(invalid_data)
}
fn array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>, SourceError> {
    value[key].as_array().ok_or_else(invalid_data)
}
fn text<const N: usize>(value: impl Into<String>) -> Result<api::Text<N>, SourceError> {
    api::Text::new(value).map_err(invalid_response)
}
fn list<T, const N: usize>(items: Vec<T>) -> Result<api::List<T, N>, SourceError> {
    api::List::new(items).map_err(invalid_response)
}
fn invalid_data() -> SourceError {
    SourceError::new(
        "invalid_upstream_response",
        "GFN returned an invalid typed response",
    )
}
fn invalid_response(error: opennow_plugin_api::ValidationError) -> SourceError {
    SourceError::new("invalid_upstream_response", error.to_string())
}
fn invalid_request(error: opennow_plugin_api::ValidationError) -> SourceError {
    SourceError::new("invalid_params", error.to_string())
}
fn scope_changed() -> SourceError {
    SourceError::new("stale_account", "The account context changed")
}
fn catalog_changed() -> SourceError {
    SourceError::new(
        "catalog_changed",
        "The catalog changed; inspect the launch again",
    )
}
fn unsupported() -> SourceError {
    SourceError::new(
        "unsupported_feature",
        "This typed GFN operation is not supported",
    )
}
fn random_id() -> String {
    format!("{:032x}", rand::random::<u128>())
}

#[cfg(test)]
#[path = "provider_tests.rs"]
mod tests;
