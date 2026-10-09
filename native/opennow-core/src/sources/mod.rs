pub(crate) mod contract;
pub(crate) mod gfn;
mod journal;
mod preferences;
mod private;
mod session_manager;

use crate::plugins::PluginManager;
use crate::requests::Cancellation;
use contract::{
    BuiltinModule, CatalogQuery, CatalogSource, Completion, PluginId, PluginSnapshot, PluginState,
    SessionOccupancy, SourceCatalogPage, SourceError,
};
#[cfg(test)]
use contract::{NativePreparation, ProviderCompletion};
use contract::{ProviderContext, ProviderSource};
use opennow_plugin_api::provider as api;
use preferences::SourcePreferences;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;
use session_manager::SessionManager;
use std::path::Path;
use std::sync::{Arc, Mutex, mpsc::Sender};

pub struct SourceHost {
    builtin: Arc<dyn BuiltinModule>,
    plugins: Arc<PluginManager>,
    builtin_provider: Arc<dyn ProviderSource>,
    preferences: SourcePreferences,
    pub sessions: Arc<SessionManager>,
    output: Sender<Value>,
    settings: Arc<Mutex<crate::settings::SettingsStore>>,
    private: Arc<private::PrivateState>,
}

impl SourceHost {
    pub fn new<B: BuiltinModule + 'static>(
        builtin: Arc<B>,
        plugins: Arc<PluginManager>,
        path: &Path,
        output: Sender<Value>,
        settings: Arc<Mutex<crate::settings::SettingsStore>>,
    ) -> Result<Self, SourceError> {
        let preferences = SourcePreferences::open(path);
        builtin.set_enabled(preferences.builtin_enabled());
        let builtin_provider: Arc<dyn ProviderSource> = builtin.clone();
        Ok(Self {
            builtin,
            plugins,
            builtin_provider,
            preferences,
            sessions: SessionManager::open(path, output.clone())?,
            output,
            settings,
            private: Arc::new(private::PrivateState::default()),
        })
    }

    pub fn dispatch_builtin(
        &self,
        method: &str,
        params: &Value,
        cancellation: &Cancellation,
    ) -> Option<Completion> {
        if self.builtin.routes().contains(&method) {
            if !self.preferences.builtin_enabled() && method != "settings.set" {
                return Some(Completion::from(Err((
                    "source_unavailable".into(),
                    "The selected source is disabled".into(),
                ))));
            }
            self.dispatch_builtin_owned(method, params, cancellation)
        } else {
            None
        }
    }

    pub fn snapshot(&self) -> PluginSnapshot {
        let mut snapshot = self.plugins.snapshot();
        let mut builtin = self.builtin.descriptor();
        builtin.required = false;
        builtin.enabled = self.preferences.builtin_enabled();
        builtin.state = if builtin.enabled {
            PluginState::Ready
        } else {
            PluginState::Disabled
        };
        snapshot.plugins.insert(0, builtin);
        snapshot
    }

    pub fn core_capabilities(&self) -> &'static [&'static str] {
        self.builtin.core_capabilities()
    }

    pub fn dispatch_plugins(
        &self,
        method: &str,
        params: &Value,
        cancellation: &Cancellation,
    ) -> Result<Value, SourceError> {
        cancellation.check()?;
        if method == "plugins.list" {
            if params.as_object().is_none_or(|params| !params.is_empty()) {
                return Err(SourceError::new(
                    "invalid_params",
                    "Plugin list takes no parameters",
                ));
            }
            self.plugins.dispatch(method, params, cancellation)?;
            return serialize(self.snapshot());
        }
        let guarded = matches!(method, "plugins.setEnabled" | "plugins.uninstall");
        let target =
            if guarded {
                let id = params["id"].as_str().ok_or_else(|| {
                    SourceError::new("invalid_params", "Plugin identity is required")
                })?;
                Some(PluginId::new(id).map_err(|_| {
                    SourceError::new("invalid_params", "Plugin identity is invalid")
                })?)
            } else {
                None
            };
        let builtin_toggle = if target.as_ref().is_some_and(PluginId::is_builtin) {
            if method == "plugins.uninstall" {
                return Err(SourceError::new(
                    "builtin_plugin",
                    "Bundled sources can be disabled but not removed",
                ));
            }
            let expected = params["expectedGeneration"].as_u64().ok_or_else(|| {
                SourceError::new("invalid_params", "Registry generation is required")
            })?;
            let enabled = params["enabled"]
                .as_bool()
                .ok_or_else(|| SourceError::new("invalid_params", "Enable state is required"))?;
            Some((expected, enabled))
        } else {
            None
        };
        let _guard = if guarded {
            Some(self.sessions.guard()?)
        } else {
            None
        };
        if let Some(id) = target {
            if self.sessions.journal.source_in_use(&id)
                || (id.is_builtin() && self.builtin.session_occupancy() != SessionOccupancy::Idle)
            {
                return Err(SourceError::new(
                    "plugin_in_use",
                    "The source still owns a session or media lease",
                ));
            }
            if let Some((expected, enabled)) = builtin_toggle {
                if expected != self.plugins.snapshot().generation {
                    return Err(SourceError::new(
                        "stale_registry",
                        "The plugin registry changed",
                    ));
                }
                self.preferences.enable_builtin(enabled)?;
                self.builtin.set_enabled(enabled);
                self.notify_sources();
                return serialize(self.snapshot());
            }
        }
        let result = self.plugins.dispatch(method, params, cancellation)?;
        if matches!(
            method,
            "plugins.install.commit" | "plugins.setEnabled" | "plugins.uninstall"
        ) {
            serialize(self.snapshot())
        } else {
            Ok(result)
        }
    }

    pub fn catalog_page(
        &self,
        params: &Value,
        cancellation: &Cancellation,
    ) -> Result<SourceCatalogPage, SourceError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Request {
            source_id: PluginId,
            #[serde(default)]
            query: String,
            #[serde(default)]
            cursor: Option<String>,
            #[serde(default = "default_page_limit")]
            limit: u16,
        }

        let request: Request = serde_json::from_value(params.clone()).map_err(|_| {
            SourceError::new("invalid_params", "Catalog request parameters are invalid")
        })?;
        let query = CatalogQuery {
            query: request.query,
            cursor: request.cursor,
            limit: request.limit,
        };
        query
            .validate()
            .map_err(|error| SourceError::new("invalid_params", error.to_string()))?;
        cancellation.check()?;
        if request.source_id == self.builtin.descriptor().id {
            if !self.preferences.builtin_enabled() {
                return Err(source_unavailable());
            }
            return read_page(
                self.builtin.as_ref(),
                request.source_id,
                &query,
                cancellation,
            );
        }
        let source = self.plugins.source(&request.source_id)?;
        let page = read_page(
            source.as_ref(),
            request.source_id.clone(),
            &query,
            cancellation,
        )?;
        let current = self.plugins.source(&request.source_id)?;
        if !Arc::ptr_eq(&source, &current) || current.generation() != page.generation {
            return Err(stale_source());
        }
        cancellation.check()?;
        Ok(page)
    }

    pub fn reporting_identity(&self) -> Value {
        if self.allows_legacy_reporting() {
            self.builtin.reporting_identity()
        } else {
            Value::Null
        }
    }

    pub fn allows_legacy_reporting(&self) -> bool {
        if self
            .preferences
            .selected()
            .as_ref()
            .is_some_and(|id| !id.is_builtin())
        {
            return false;
        }
        self.sessions
            .journal
            .snapshot()
            .is_ok_and(|record| record.is_none_or(|record| record.source_id.is_builtin()))
    }

    pub fn session_occupancy(&self) -> SessionOccupancy {
        let journal = self.sessions.occupancy();
        if journal != SessionOccupancy::Idle {
            return journal;
        }
        self.builtin.session_occupancy()
    }

    pub fn settings_changed(&self) {
        if self.preferences.builtin_enabled() {
            self.builtin.settings_changed();
        }
    }

    pub fn shutdown(&self) {
        self.plugins.shutdown();
        self.sessions.shutdown();
        self.builtin.shutdown();
    }

    pub fn provider(&self, source: &PluginId) -> Result<Arc<dyn ProviderSource>, SourceError> {
        self.resolve_provider(source, false)
    }

    fn resolve_provider(
        &self,
        source: &PluginId,
        recovery: bool,
    ) -> Result<Arc<dyn ProviderSource>, SourceError> {
        if source == &self.builtin.descriptor().id {
            if !self.preferences.builtin_enabled() {
                return Err(source_unavailable());
            }
            return Ok(Arc::clone(&self.builtin_provider));
        }
        let provider = self.plugins.provider(source)?;
        let descriptor = provider.descriptor();
        if !descriptor.enabled
            || !(descriptor.state == PluginState::Ready
                || recovery && descriptor.state == PluginState::Failed)
        {
            return Err(source_unavailable());
        }
        Ok(provider)
    }

    pub fn source_snapshot(&self) -> Value {
        let sources = self
            .snapshot()
            .plugins
            .into_iter()
            .map(|descriptor| {
                let provider = self.provider(&descriptor.id).ok();
                let mut capabilities = provider
                    .as_ref()
                    .map(|provider| provider.provider_capabilities())
                    .unwrap_or_else(|| {
                        descriptor
                            .capabilities
                            .iter()
                            .filter_map(|capability| {
                                serde_json::from_value::<api::Capability>(json!(capability)).ok()
                            })
                            .collect()
                    });
                if !capabilities.is_empty() && !capabilities.contains(&api::Capability::Settings) {
                    capabilities.push(api::Capability::Settings);
                }
                let kinds = provider
                    .as_ref()
                    .map(|provider| provider.auth_kinds())
                    .unwrap_or_default();
                let mut value = serde_json::to_value(&descriptor).expect("source descriptor");
                let service = !capabilities.is_empty();
                value["protocolVersion"] = json!(if service { 2 } else { 1 });
                value["providerCapabilities"] = json!(capabilities);
                value["authKinds"] = json!(kinds);
                value["playback"] = if capabilities.contains(&api::Capability::Sessions) {
                    json!(if descriptor.builtin {
                        "gfn-native"
                    } else {
                        "media-worker-v1"
                    })
                } else {
                    Value::Null
                };
                value
            })
            .collect::<Vec<_>>();
        json!({"generation":self.plugins.snapshot().generation.saturating_add(self.preferences.generation()),
            "selectedSourceId":self.preferences.selected(), "sources":sources})
    }

    fn notify_sources(&self) {
        let _ = self
            .output
            .send(json!({"type":"event","name":"sources.changed", "payload":{}}));
        let _ = self.output.send(json!({"type":"event","name":"plugins.changed", "payload":{"generation":self.plugins.snapshot().generation}}));
    }

    pub fn dispatch_sources(
        &self,
        method: &str,
        params: &Value,
        cancellation: &Cancellation,
    ) -> Completion {
        let decode = || -> Result<Completion, SourceError> {
            if method == "sources.list" {
                return Ok(Completion::from(Ok((self.source_snapshot(), None))));
            }
            if method == "sources.session.current" {
                let record = self.sessions.journal.snapshot()?;
                let public = record.map(|record| json!({"sourceId":record.source_id,"account":record.account,"sessionHandle":record.operation.as_str(),
                    "operation":record.operation,"session":record.session,"phase":record.phase,
                    "leaseId":record.media.map(|pin| pin.lease_id)}));
                return Ok(Completion::from(Ok((json!({"session":public}), None))));
            }
            let source: PluginId =
                serde_json::from_value(params["sourceId"].clone()).map_err(|_| invalid_params())?;
            if method == "sources.select" {
                self.provider(&source)?;
                self.preferences.select(source)?;
                self.invalidate_browser(None);
                self.notify_sources();
                return Ok(Completion::from(Ok((self.source_snapshot(), None))));
            }
            let operation = provider_operation(method).ok_or_else(|| {
                SourceError::new("method_not_found", "Unknown provider operation")
            })?;
            let request: api::ProviderRequest =
                serde_json::from_value(json!({"method":operation,"params":params["request"]}))
                    .map_err(|_| invalid_params())?;
            request.validate().map_err(|_| invalid_params())?;
            let provider = self.resolve_provider(
                &source,
                matches!(
                    request,
                    api::ProviderRequest::SessionReconcile(_)
                        | api::ProviderRequest::SessionStop(_)
                ),
            )?;
            if matches!(
                method,
                "sources.auth.start"
                    | "sources.auth.cancel"
                    | "sources.auth.complete"
                    | "sources.auth.logout"
                    | "sources.accounts.select"
                    | "sources.accounts.remove"
            ) {
                self.invalidate_browser(Some(&source));
            }
            if method == "sources.settings.get" || method == "sources.settings.set" {
                let account = request_account(&request);
                if let Some(account) = account {
                    let expected_revision = match &request {
                        api::ProviderRequest::SettingsGet(query) => {
                            query.account.as_ref().map(|scope| scope.revision)
                        }
                        api::ProviderRequest::SettingsSet(query) => {
                            query.scope.account.as_ref().map(|scope| scope.revision)
                        }
                        _ => None,
                    };
                    let auth = provider.provider_call(
                        &api::ProviderRequest::AuthStatus(api::Empty {}),
                        &ProviderContext {
                            cancellation,
                            runtime_capabilities: None,
                            gfn_settings: None,
                        },
                    );
                    if !matches!(auth.result,Ok(api::ProviderReply::AuthStatus(api::AuthState::SignedIn {account:ref current,revision})) if current.key == *account && Some(revision)==expected_revision)
                    {
                        return Err(SourceError::new(
                            "scope_changed",
                            "Select the account before changing its source profile",
                        ));
                    }
                }
                if let api::ProviderRequest::SettingsSet(setting) = &request {
                    self.preferences
                        .set_profile(source.clone(), account.cloned(), setting)?;
                }
                let effective = self.effective_settings(&source, account)?;
                let view = preferences::profile_view(&effective, self.preferences.generation())?;
                return Ok(Completion::from(Ok((
                    json!({"sourceId":source,"generation":provider.generation(),"result":view}),
                    None,
                ))));
            }
            if !request.permits(&provider.provider_capabilities()) {
                return Err(SourceError::new(
                    "unsupported_feature",
                    "This source does not support the requested operation",
                ));
            }
            if matches!(request, api::ProviderRequest::SessionCreate(_))
                && self.sessions.occupancy() == SessionOccupancy::Idle
                && self.builtin.session_occupancy() != SessionOccupancy::Idle
            {
                return Err(SourceError::new(
                    "session_in_use",
                    "An existing session must be resolved before allocation",
                ));
            }
            let effective = self.effective_settings(&source, request_account(&request))?;
            let gfn_settings = source.is_builtin().then_some(&effective);
            let mut completion = self.sessions.execute(
                source.clone(),
                provider,
                request,
                &ProviderContext {
                    cancellation,
                    runtime_capabilities: None,
                    gfn_settings,
                },
            );
            if let Err(error) = self.project_public(&source, &mut completion) {
                completion.result = Err(error.into());
            }
            Ok(completion)
        };
        decode().unwrap_or_else(|error| Completion::from(Err(error.into())))
    }

    fn effective_settings(
        &self,
        source: &PluginId,
        account: Option<&api::AccountKey>,
    ) -> Result<Value, SourceError> {
        let global = self.settings.lock().expect("settings poisoned").all();
        self.preferences.effective(source, account, &global)
    }

    fn dispatch_builtin_owned(
        &self,
        method: &str,
        params: &Value,
        cancellation: &Cancellation,
    ) -> Option<Completion> {
        let session_operation = method.starts_with("session.") || method == "streamer.prepare";
        if !session_operation {
            return self.builtin.dispatch(method, params, cancellation);
        }
        let _guard = match self.sessions.guard() {
            Ok(guard) => guard,
            Err(error) => return Some(Completion::from(Err(error.into()))),
        };
        let source = self.builtin.descriptor().id;
        let previous = match self.sessions.journal.snapshot() {
            Ok(record) => record,
            Err(error) => return Some(Completion::from(Err(error.into()))),
        };
        if previous
            .as_ref()
            .is_some_and(|record| record.source_id != source)
        {
            return Some(Completion::from(Err((
                "session_in_use".into(),
                "Another source owns the active session".into(),
            ))));
        }
        if method == "streamer.prepare"
            && previous
                .as_ref()
                .is_some_and(|record| record.phase != journal::Phase::Active)
        {
            return Some(Completion::from(Err((
                "busy".into(),
                "Session ownership is not ready for presentation".into(),
            ))));
        }
        let operation = if method == "session.create" {
            if self.builtin.session_occupancy() != SessionOccupancy::Idle {
                return Some(Completion::from(Err((
                    "session_update_busy".into(),
                    "Resolve the existing session before starting another".into(),
                ))));
            }
            let account = match self.builtin.legacy_account(params) {
                Ok(account) => account,
                Err(error) => return Some(Completion::from(Err(error.into()))),
            };
            let operation = session_manager::new_operation();
            if let Err(error) =
                self.sessions
                    .journal
                    .begin(source.clone(), Some(account), operation.clone())
            {
                return Some(Completion::from(Err(error.into())));
            }
            operation
        } else {
            previous
                .as_ref()
                .map(|record| record.operation.clone())
                .unwrap_or_else(session_manager::new_operation)
        };
        let mut owned_params = params.clone();
        if method == "session.stop" {
            let target = match previous.as_ref().and_then(|record| record.session.clone()) {
                Some(key) => Some(key),
                None => match self.builtin.legacy_control_session(params) {
                    Ok(key) => key,
                    Err(error) => return Some(Completion::from(Err(error.into()))),
                },
            };
            if let Some(key) = target {
                if params["sessionId"]
                    .as_str()
                    .is_some_and(|id| !id.is_empty() && id != key.remote_id.as_str())
                {
                    return Some(Completion::from(Err((
                        "session_owner_mismatch".into(),
                        "The stop request does not own the current session".into(),
                    ))));
                }
                if previous.is_none()
                    && let Err(error) = self.sessions.journal.reconcile(
                        source.clone(),
                        key.account.clone(),
                        operation.clone(),
                        key.clone(),
                    )
                {
                    return Some(Completion::from(Err(error.into())));
                }
                if let Err(error) = self.sessions.journal.cleanup_pending(&source, &key) {
                    return Some(Completion::from(Err(error.into())));
                }
                owned_params["sessionId"] = json!(key.remote_id);
                if let Some(account) = &key.account {
                    owned_params["ownerScope"] =
                        json!({"providerIdpId":account.authority,"userId":account.account});
                }
            }
        }
        let params = &owned_params;
        let mut completion = self.builtin.dispatch(method, params, cancellation)?;
        if let Ok((value, _)) = &completion.result {
            match self.builtin.legacy_session_result(method, params, value) {
                Ok(Some((key, terminal))) => {
                    let saved = if method == "session.create" {
                        self.sessions.journal.allocated(
                            &source,
                            &operation,
                            key.clone(),
                            api::ReceiptId::new(operation.as_str())
                                .expect("allocation receipt identity"),
                        )
                    } else if terminal {
                        if self
                            .sessions
                            .journal
                            .snapshot()
                            .is_ok_and(|record| record.is_some())
                        {
                            self.sessions.journal.remote_ended(&source, &key)
                        } else {
                            Ok(())
                        }
                    } else if previous
                        .as_ref()
                        .is_some_and(|record| record.phase == journal::Phase::AwaitingAcceptance)
                    {
                        self.sessions
                            .journal
                            .require_session(&source, &key)
                            .map(|_| ())
                    } else {
                        self.sessions.journal.reconcile(
                            source.clone(),
                            key.account.clone(),
                            operation.clone(),
                            key.clone(),
                        )
                    };
                    if let Err(error) = saved {
                        completion.result = Err(error.into());
                    } else if method == "streamer.prepare"
                        && let Err(error) = self.sessions.journal.mark_legacy_media(&source, &key)
                    {
                        completion.result = Err(error.into());
                    }
                }
                Ok(None) => {}
                Err(error) => completion.result = Err(error.into()),
            }
        }
        if method == "session.create" {
            if let Some(receipt) = completion.receipt.take() {
                if let Some(contract::AllocationDisposition::Allocated { session_id }) =
                    &completion.allocation_disposition
                    && let Ok(Some(record)) = self.sessions.journal.snapshot()
                    && record.session.is_none()
                {
                    let key = api::SessionKey {
                        account: record.account,
                        remote_id: match api::SessionId::new(session_id) {
                            Ok(id) => id,
                            Err(_) => {
                                completion.result = Err((
                                    "session_owner_mismatch".into(),
                                    "Allocated session identity is invalid".into(),
                                ));
                                completion.receipt = Some(receipt);
                                return Some(completion);
                            }
                        },
                    };
                    if let Err(error) = self.sessions.journal.allocated(
                        &source,
                        &operation,
                        key,
                        api::ReceiptId::new(operation.as_str())
                            .expect("allocation receipt identity"),
                    ) {
                        completion.result = Err(error.into());
                    }
                }
                completion.receipt = Some(Box::new(LegacyReceipt {
                    receipt,
                    source,
                    operation,
                    journal: Arc::clone(&self.sessions.journal),
                    builtin: Arc::clone(&self.builtin),
                }));
            } else if completion.result.is_err() {
                let known = matches!(
                    completion.allocation_disposition,
                    Some(
                        contract::AllocationDisposition::NotDispatched
                            | contract::AllocationDisposition::Rejected
                            | contract::AllocationDisposition::CleanedUp { .. }
                    )
                );
                let result = if matches!(
                    completion.allocation_disposition,
                    Some(contract::AllocationDisposition::NotDispatched)
                ) {
                    self.sessions
                        .journal
                        .rejected_before_allocation(&source, &operation)
                } else if known {
                    self.sessions
                        .journal
                        .rejected_allocation_resolved(&source, &operation)
                } else {
                    self.sessions.journal.unknown(&source, &operation)
                };
                if let Err(error) = result {
                    completion.result = Err(error.into());
                }
            }
        }
        Some(completion)
    }
}

struct LegacyReceipt {
    receipt: Box<dyn contract::AllocationReceipt>,
    builtin: Arc<dyn BuiltinModule>,
    journal: Arc<journal::SessionJournal>,
    source: PluginId,
    operation: api::OperationId,
}

impl contract::AllocationReceipt for LegacyReceipt {
    fn settle(self: Box<Self>, accepted: bool) -> contract::ReceiptOutcome {
        if self.journal.snapshot().is_ok_and(|record| {
            record.is_none_or(|record| {
                record.source_id != self.source || record.operation != self.operation
            })
        }) {
            return contract::ReceiptOutcome {
                result: Ok(()),
                required_events: Vec::new(),
            };
        }
        if !accepted
            && let Ok(Some(record)) = self.journal.snapshot()
            && let Some(key) = record.session
        {
            let _ = self.journal.cleanup_pending(&self.source, &key);
        }
        let mut outcome = self.receipt.settle(accepted);
        let recorded = match &outcome.result {
            Ok(()) if accepted && self.builtin.session_occupancy() != SessionOccupancy::Idle => {
                self.journal.settled(&self.source, &self.operation, true)
            }
            Ok(()) => self
                .journal
                .rejected_allocation_resolved(&self.source, &self.operation),
            Err(_) => self.journal.unknown(&self.source, &self.operation),
        };
        if let Err(error) = recorded {
            outcome.result = Err(error);
            outcome.required_events.push(("session.cleanup.pending", json!({"code":"session_journal_unavailable","message":"Session ownership could not be saved"})));
        }
        outcome
    }
}

fn provider_operation(method: &str) -> Option<&'static str> {
    Some(match method {
        "sources.auth.authorities" => "auth.authorities",
        "sources.auth.state" => "auth.status",
        "sources.auth.start" => "auth.begin",
        "sources.auth.poll" => "auth.poll",
        "sources.auth.complete" => "auth.complete",
        "sources.auth.cancel" => "auth.cancel",
        "sources.auth.logout" => "auth.logout",
        "sources.accounts.list" => "accounts.list",
        "sources.accounts.select" => "accounts.select",
        "sources.accounts.remove" => "accounts.remove",
        "sources.public.page" => "catalog.public",
        "sources.library.page" => "catalog.library",
        "sources.store.page" => "catalog.store",
        "sources.game.get" => "catalog.details",
        "sources.launch.inspect" => "launch.inspect",
        "sources.session.create" => "session.create",
        "sources.session.poll" => "session.poll",
        "sources.session.discover" => "session.discover",
        "sources.session.claim" => "session.claim",
        "sources.session.reconcile" => "session.reconcile",
        "sources.session.stop" => "session.stop",
        "sources.settings.get" => "settings.get",
        "sources.settings.set" => "settings.set",
        "sources.providerSettings.get" => "settings.get",
        "sources.providerSettings.set" => "settings.set",
        _ => return None,
    })
}

fn source_unavailable() -> SourceError {
    SourceError::new(
        "source_unavailable",
        "The requested provider is unavailable or disabled",
    )
}
fn invalid_params() -> SourceError {
    SourceError::new("invalid_params", "Provider request parameters are invalid")
}

fn request_account(request: &api::ProviderRequest) -> Option<&api::AccountKey> {
    match request {
        api::ProviderRequest::CatalogPublic(query)
        | api::ProviderRequest::CatalogLibrary(query)
        | api::ProviderRequest::CatalogStore(query)
        | api::ProviderRequest::FavoritesList(query) => match &query.scope {
            api::CatalogScope::Account { scope } => Some(&scope.account),
            _ => None,
        },
        api::ProviderRequest::CatalogDetails(query) => match &query.scope {
            api::CatalogScope::Account { scope } => Some(&scope.account),
            _ => None,
        },
        api::ProviderRequest::LaunchInspect(query) => {
            query.scope.as_ref().map(|scope| &scope.account)
        }
        api::ProviderRequest::SessionCreate(query) => {
            query.scope.as_ref().map(|scope| &scope.account)
        }
        api::ProviderRequest::SessionPoll(key)
        | api::ProviderRequest::SessionStop(api::StopSession { session: key, .. })
        | api::ProviderRequest::SessionPrepare(api::PrepareSession { session: key, .. }) => {
            key.account.as_ref()
        }
        api::ProviderRequest::SessionReconcile(query) => {
            query.scope.as_ref().map(|scope| &scope.account)
        }
        api::ProviderRequest::SessionClaim(query) => {
            query.scope.as_ref().map(|scope| &scope.account)
        }
        api::ProviderRequest::SessionDiscover(query) => {
            query.scope.as_ref().map(|scope| &scope.account)
        }
        api::ProviderRequest::SettingsGet(query) => {
            query.account.as_ref().map(|scope| &scope.account)
        }
        api::ProviderRequest::SettingsSet(query) => {
            query.scope.account.as_ref().map(|scope| &scope.account)
        }
        _ => None,
    }
}

fn default_page_limit() -> u16 {
    CatalogQuery::default().limit
}

fn read_page<S: CatalogSource + ?Sized>(
    source: &S,
    source_id: PluginId,
    query: &CatalogQuery,
    cancellation: &Cancellation,
) -> Result<SourceCatalogPage, SourceError> {
    let descriptor = source.descriptor();
    if !descriptor.enabled || descriptor.state != PluginState::Ready {
        return Err(SourceError::new(
            "plugin_not_ready",
            "The catalog source is not ready",
        ));
    }
    let generation = source.generation();
    let page = source.catalog_page(query, cancellation)?;
    page.validate().map_err(|_| {
        SourceError::new(
            "plugin_protocol_error",
            "The catalog source returned an invalid page",
        )
    })?;
    if page.items.len() > usize::from(query.limit) {
        return Err(SourceError::new(
            "plugin_protocol_error",
            "The catalog source exceeded the requested page size",
        ));
    }
    cancellation.check()?;
    let current = source.descriptor();
    if source.generation() != generation || !current.enabled || current.state != PluginState::Ready
    {
        return Err(stale_source());
    }
    Ok(SourceCatalogPage::bind(source_id, generation, page))
}

fn stale_source() -> SourceError {
    SourceError::new(
        "stale_source",
        "The catalog source changed during this request",
    )
}

fn serialize(value: impl serde::Serialize) -> Result<Value, SourceError> {
    serde_json::to_value(value).map_err(|_| {
        SourceError::new(
            "plugin_protocol_error",
            "The plugin response could not be encoded",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use contract::{CatalogItem, CatalogPage, Coverage, PluginDescriptor, PluginTrust};
    use opennow_plugin_api::{BUILTIN_GFN_ID, CATALOG_CAPABILITY};
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
    use std::sync::mpsc;

    #[derive(Clone, Copy)]
    enum Behavior {
        Normal,
        ChangeGeneration,
        InvalidPage,
        OversizedPage,
    }

    struct Fixture {
        generation: AtomicU64,
        calls: AtomicUsize,
        behavior: Behavior,
    }

    impl Fixture {
        fn new(behavior: Behavior) -> Self {
            Self {
                generation: AtomicU64::new(1),
                calls: AtomicUsize::new(0),
                behavior,
            }
        }
    }

    impl CatalogSource for Fixture {
        fn descriptor(&self) -> PluginDescriptor {
            PluginDescriptor {
                id: PluginId::new(BUILTIN_GFN_ID).unwrap(),
                name: "Fixture".into(),
                version: "1.0.0".into(),
                publisher: "Fixture".into(),
                description: "Test catalog".into(),
                builtin: true,
                required: true,
                enabled: true,
                state: PluginState::Ready,
                capabilities: vec![CATALOG_CAPABILITY.into()],
                trust: PluginTrust::Builtin,
                last_error: None,
            }
        }

        fn generation(&self) -> u64 {
            self.generation.load(Ordering::Acquire)
        }

        fn catalog_page(
            &self,
            query: &CatalogQuery,
            _: &Cancellation,
        ) -> Result<CatalogPage, SourceError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            if matches!(self.behavior, Behavior::ChangeGeneration) {
                self.generation.fetch_add(1, Ordering::AcqRel);
            }
            let items = if matches!(self.behavior, Behavior::OversizedPage) {
                (0..=query.limit)
                    .map(|id| CatalogItem {
                        id: id.to_string(),
                        title: "Game".into(),
                    })
                    .collect()
            } else {
                vec![CatalogItem {
                    id: "one".into(),
                    title: if matches!(self.behavior, Behavior::InvalidPage) {
                        String::new()
                    } else {
                        "Game".into()
                    },
                }]
            };
            Ok(CatalogPage {
                items,
                next_cursor: None,
                coverage: Coverage::Partial,
            })
        }
    }

    impl ProviderSource for Fixture {
        fn provider_capabilities(&self) -> Vec<api::Capability> {
            vec![api::Capability::PublicCatalog]
        }
        fn auth_kinds(&self) -> Vec<api::AuthKind> {
            Vec::new()
        }
        fn provider_call(
            &self,
            request: &api::ProviderRequest,
            context: &ProviderContext<'_>,
        ) -> ProviderCompletion {
            let api::ProviderRequest::CatalogPublic(query) = request else {
                return ProviderCompletion::not_dispatched(SourceError::new(
                    "unsupported_feature",
                    "Fixture is catalog-only",
                ));
            };
            match self.catalog_page(&query.query, context.cancellation) {
                Ok(page) => {
                    ProviderCompletion::reply(api::ProviderReply::CatalogPublic(api::GamePage {
                        items: api::List::new(
                            page.items
                                .into_iter()
                                .map(|item| api::GameSummary {
                                    id: api::GameId::new(item.id).unwrap(),
                                    title: api::Text::new(item.title).unwrap(),
                                    artwork: None,
                                    subtitle: None,
                                    badges: api::List::default(),
                                    availability: api::Availability::Unknown,
                                })
                                .collect(),
                        )
                        .unwrap(),
                        next_cursor: None,
                        coverage: page.coverage,
                        revision: api::Text::new("fixture").unwrap(),
                        scope: query.scope.clone(),
                    }))
                }
                Err(error) => ProviderCompletion::failed(error),
            }
        }
        fn prepare_native(
            &self,
            _: &api::PrepareSession,
            _: &ProviderContext<'_>,
        ) -> Result<NativePreparation, SourceError> {
            Err(SourceError::new(
                "unsupported_feature",
                "Fixture has no playback",
            ))
        }
    }

    impl BuiltinModule for Fixture {
        fn routes(&self) -> &'static [&'static str] {
            &["settings.set"]
        }

        fn core_capabilities(&self) -> &'static [&'static str] {
            &["fixture.v1"]
        }

        fn dispatch(&self, _: &str, params: &Value, _: &Cancellation) -> Option<Completion> {
            (params["key"] == "region")
                .then(|| Completion::from(Ok((json!({"accepted":true}), None))))
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
        fn legacy_control_session(
            &self,
            _: &Value,
        ) -> Result<Option<api::SessionKey>, SourceError> {
            Ok(None)
        }
        fn apply_profile(&self, base: &Value, _: &api::StreamPreferences) -> Value {
            base.clone()
        }
        fn legacy_account(&self, _params: &Value) -> Result<api::AccountKey, SourceError> {
            Err(source_unavailable())
        }
        fn legacy_session_result(
            &self,
            _: &str,
            _: &Value,
            _: &Value,
        ) -> Result<Option<(api::SessionKey, bool)>, SourceError> {
            Ok(None)
        }
    }

    fn host(source: Arc<Fixture>) -> (tempfile::TempDir, SourceHost) {
        let directory = tempfile::tempdir().unwrap();
        let (output, _) = mpsc::channel();
        let manager = Arc::new(PluginManager::open(directory.path(), output.clone()).unwrap());
        let host = SourceHost::new(
            source,
            manager,
            directory.path(),
            output,
            Arc::new(Mutex::new(
                crate::settings::SettingsStore::load(Some(directory.path().into())).unwrap(),
            )),
        )
        .unwrap();
        (directory, host)
    }

    #[test]
    fn builtins_can_decline_selective_compatibility_routes() {
        let (_directory, host) = host(Arc::new(Fixture::new(Behavior::Normal)));
        let cancellation = Cancellation::default();
        assert!(
            host.dispatch_builtin("settings.set", &json!({"key":"volume"}), &cancellation)
                .is_none()
        );
        assert!(
            host.dispatch_builtin("unknown", &json!({}), &cancellation)
                .is_none()
        );
        assert!(
            host.dispatch_builtin("settings.set", &json!({"key":"region"}), &cancellation)
                .unwrap()
                .result
                .is_ok()
        );
        assert_eq!(host.core_capabilities(), &["fixture.v1"]);
    }

    #[test]
    fn catalog_reads_are_generation_fenced_and_validate_source_results() {
        let query = CatalogQuery {
            limit: 1,
            ..CatalogQuery::default()
        };
        for (behavior, expected) in [
            (Behavior::ChangeGeneration, "stale_source"),
            (Behavior::InvalidPage, "plugin_protocol_error"),
            (Behavior::OversizedPage, "plugin_protocol_error"),
        ] {
            let source = Fixture::new(behavior);
            let result = read_page(
                &source,
                source.descriptor().id,
                &query,
                &Cancellation::default(),
            );
            assert_eq!(result.unwrap_err().code, expected);
        }
        let source = Fixture::new(Behavior::Normal);
        let page = read_page(
            &source,
            source.descriptor().id,
            &query,
            &Cancellation::default(),
        )
        .unwrap();
        assert_eq!(page.source_id.as_str(), BUILTIN_GFN_ID);
        assert_eq!(page.items[0].id.source_id, page.source_id);
        assert_eq!(page.items[0].id.local_id, "one");
        assert_eq!(page.generation, 1);
        assert_eq!(page.coverage, Coverage::Partial);
    }

    #[test]
    fn malformed_catalog_queries_fail_before_invocation() {
        let source = Arc::new(Fixture::new(Behavior::Normal));
        let (_directory, host) = host(Arc::clone(&source));
        for invalid in [
            json!({"sourceId":BUILTIN_GFN_ID,"limit":0}),
            json!({"sourceId":BUILTIN_GFN_ID,"limit":101}),
            json!({"sourceId":BUILTIN_GFN_ID,"query":"x".repeat(513)}),
            json!({"sourceId":BUILTIN_GFN_ID,"cursor":"x".repeat(4097)}),
            json!({"sourceId":BUILTIN_GFN_ID,"settings":{}}),
        ] {
            assert_eq!(
                host.catalog_page(&invalid, &Cancellation::default())
                    .unwrap_err()
                    .code,
                "invalid_params"
            );
        }
        assert_eq!(source.calls.load(Ordering::Relaxed), 0);
        assert!(
            host.catalog_page(
                &json!({"sourceId":BUILTIN_GFN_ID}),
                &Cancellation::default()
            )
            .is_ok()
        );
        assert_eq!(source.calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn v2_router_does_not_relabel_old_catalog_with_a_new_source_generation() {
        let source = Arc::new(Fixture::new(Behavior::ChangeGeneration));
        let (_directory, host) = host(Arc::clone(&source));
        let before = source.generation();
        let result=host.dispatch_sources("sources.public.page",&json!({"sourceId":BUILTIN_GFN_ID,"request":{"scope":{"kind":"public"},"query":{"query":"","cursor":null,"limit":20}}}),&Cancellation::default());
        assert!(source.generation() > before);
        assert_eq!(result.result.unwrap_err().0, "stale_source");
        host.shutdown();
    }

    #[test]
    fn builtin_metadata_is_merged_but_cannot_be_mutated() {
        let (_directory, host) = host(Arc::new(Fixture::new(Behavior::Normal)));
        let snapshot = host.snapshot();
        assert_eq!(snapshot.plugins.len(), 1);
        assert_eq!(snapshot.plugins[0].id.as_str(), BUILTIN_GFN_ID);
        for (method, expected) in [
            ("plugins.setEnabled", "invalid_params"),
            ("plugins.uninstall", "builtin_plugin"),
        ] {
            assert_eq!(
                host.dispatch_plugins(
                    method,
                    &json!({"id":BUILTIN_GFN_ID}),
                    &Cancellation::default()
                )
                .unwrap_err()
                .code,
                expected
            );
        }
        assert!(
            host.dispatch_plugins(
                "plugins.list",
                &json!({"unexpected":true}),
                &Cancellation::default()
            )
            .is_err()
        );
    }

    #[test]
    fn immutable_builtin_rejections_precede_session_lock_contention() {
        let (_directory, host) = host(Arc::new(Fixture::new(Behavior::Normal)));
        host.sessions.with_held_transition(|| {
            let removal=host.dispatch_plugins("plugins.uninstall",&json!({"id":BUILTIN_GFN_ID}),&Cancellation::default()).unwrap_err();
            assert_eq!(removal.code,"builtin_plugin");
            let malformed=host.dispatch_plugins("plugins.setEnabled",&json!({"id":BUILTIN_GFN_ID}),&Cancellation::default()).unwrap_err();
            assert_eq!(malformed.code,"invalid_params");
            let mutable=host.dispatch_plugins("plugins.setEnabled",&json!({"id":BUILTIN_GFN_ID,"enabled":false,"expectedGeneration":host.snapshot().generation}),&Cancellation::default()).unwrap_err();
            assert_eq!(mutable.code,"busy");
        });
        host.shutdown();
    }

    #[test]
    fn source_error_strings_remain_bounded_without_static_leaks() {
        let error = SourceError::new("x".repeat(65), "é".repeat(1025));
        assert_eq!(error.code, "source_error");
        assert_eq!(error.message.len(), 1024);
        assert!(error.message.is_char_boundary(error.message.len()));
    }

    #[test]
    fn cancelled_catalog_requests_do_not_invoke_the_source() {
        let source = Arc::new(Fixture::new(Behavior::Normal));
        let (_directory, host) = host(Arc::clone(&source));
        let requests = Arc::new(crate::requests::Requests::default());
        let permit = requests.admit("catalog", "sources.catalog.page").unwrap();
        requests.cancel("catalog");
        let result = host.catalog_page(&json!({"sourceId":BUILTIN_GFN_ID}), &permit.token);
        assert_eq!(result.unwrap_err().code, "cancelled");
        assert_eq!(source.calls.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn unavailable_manager_does_not_hide_management_errors_or_break_builtin_routes() {
        let directory = tempfile::tempdir().unwrap();
        let (output, _) = mpsc::channel();
        let manager = Arc::new(PluginManager::open(directory.path(), output.clone()).unwrap());
        manager.shutdown();
        let host = SourceHost::new(
            Arc::new(Fixture::new(Behavior::Normal)),
            manager,
            directory.path(),
            output,
            Arc::new(Mutex::new(
                crate::settings::SettingsStore::load(Some(directory.path().into())).unwrap(),
            )),
        )
        .unwrap();
        let cancellation = Cancellation::default();
        assert_eq!(
            host.dispatch_plugins("plugins.list", &json!({}), &cancellation)
                .unwrap_err()
                .code,
            "plugins_unavailable"
        );
        assert!(
            host.catalog_page(&json!({"sourceId":BUILTIN_GFN_ID}), &cancellation)
                .is_ok()
        );
        assert!(
            host.dispatch_builtin("settings.set", &json!({"key":"region"}), &cancellation)
                .unwrap()
                .result
                .is_ok()
        );
    }

    #[test]
    fn corrupt_registry_preserves_bytes_and_builtin_routes_remain_available() {
        let directory = tempfile::tempdir().unwrap();
        let plugins = directory.path().join("plugins");
        std::fs::create_dir(&plugins).unwrap();
        let registry = plugins.join("registry.json");
        let corrupt = b"{invalid registry: preserve these bytes}";
        std::fs::write(&registry, corrupt).unwrap();
        let (output, events) = mpsc::channel();
        let manager = Arc::new(PluginManager::open(directory.path(), output.clone()).unwrap());
        let host = SourceHost::new(
            Arc::new(Fixture::new(Behavior::Normal)),
            manager,
            directory.path(),
            output,
            Arc::new(Mutex::new(
                crate::settings::SettingsStore::load(Some(directory.path().into())).unwrap(),
            )),
        )
        .unwrap();
        let cancellation = Cancellation::default();

        for method in ["plugins.list", "plugins.install.cancel"] {
            let error = host
                .dispatch_plugins(method, &json!({}), &cancellation)
                .unwrap_err();
            assert_eq!(error.code, "plugin_storage_error");
            assert!(!error.message.contains("preserve these bytes"));
        }
        assert_eq!(host.snapshot().plugins[0].id.as_str(), BUILTIN_GFN_ID);
        assert!(
            host.catalog_page(&json!({"sourceId":BUILTIN_GFN_ID}), &cancellation)
                .is_ok()
        );
        assert!(
            host.dispatch_builtin("settings.set", &json!({"key":"region"}), &cancellation)
                .unwrap()
                .result
                .is_ok()
        );
        host.shutdown();
        assert_eq!(std::fs::read(&registry).unwrap(), corrupt);
        assert!(events.try_recv().is_err());
    }

    #[test]
    fn source_router_stops_a_session_while_a_catalog_call_is_blocked() {
        struct Blocking {
            inner: Fixture,
            entered: mpsc::Sender<()>,
            release: Mutex<mpsc::Receiver<()>>,
        }
        impl CatalogSource for Blocking {
            fn descriptor(&self) -> PluginDescriptor {
                self.inner.descriptor()
            }
            fn generation(&self) -> u64 {
                1
            }
            fn catalog_page(
                &self,
                query: &CatalogQuery,
                cancel: &Cancellation,
            ) -> Result<CatalogPage, SourceError> {
                self.inner.catalog_page(query, cancel)
            }
        }
        impl ProviderSource for Blocking {
            fn provider_capabilities(&self) -> Vec<api::Capability> {
                vec![api::Capability::PublicCatalog, api::Capability::Sessions]
            }
            fn auth_kinds(&self) -> Vec<api::AuthKind> {
                vec![api::AuthKind::Anonymous]
            }
            fn provider_call(
                &self,
                request: &api::ProviderRequest,
                _: &ProviderContext<'_>,
            ) -> ProviderCompletion {
                let reply = match request {
                    api::ProviderRequest::CatalogPublic(query) => {
                        self.entered.send(()).unwrap();
                        self.release
                            .lock()
                            .unwrap()
                            .recv_timeout(std::time::Duration::from_secs(3))
                            .unwrap();
                        api::ProviderReply::CatalogPublic(api::GamePage {
                            items: api::List::default(),
                            next_cursor: None,
                            coverage: Coverage::Unknown,
                            revision: api::Text::new("1").unwrap(),
                            scope: query.scope.clone(),
                        })
                    }
                    api::ProviderRequest::SessionStop(_) => {
                        api::ProviderReply::SessionStop(api::CleanupState::Resolved)
                    }
                    _ => return ProviderCompletion::failed(source_unavailable()),
                };
                ProviderCompletion {
                    result: Ok(reply),
                    effects: vec![],
                    allocation: None,
                    dispatched: true,
                    allocation_disposition: None,
                    dispatched_generation: None,
                }
            }
            fn prepare_native(
                &self,
                _: &api::PrepareSession,
                _: &ProviderContext<'_>,
            ) -> Result<NativePreparation, SourceError> {
                Err(source_unavailable())
            }
        }
        impl BuiltinModule for Blocking {
            fn routes(&self) -> &'static [&'static str] {
                self.inner.routes()
            }
            fn core_capabilities(&self) -> &'static [&'static str] {
                self.inner.core_capabilities()
            }
            fn dispatch(
                &self,
                method: &str,
                params: &Value,
                cancel: &Cancellation,
            ) -> Option<Completion> {
                self.inner.dispatch(method, params, cancel)
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
            fn legacy_control_session(
                &self,
                _: &Value,
            ) -> Result<Option<api::SessionKey>, SourceError> {
                Ok(None)
            }
            fn legacy_account(&self, _params: &Value) -> Result<api::AccountKey, SourceError> {
                Err(source_unavailable())
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
        let directory = tempfile::tempdir().unwrap();
        let (entered_tx, entered) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let (output, _) = mpsc::channel();
        let provider = Arc::new(Blocking {
            inner: Fixture::new(Behavior::Normal),
            entered: entered_tx,
            release: Mutex::new(release_rx),
        });
        let plugins = Arc::new(PluginManager::open(directory.path(), output.clone()).unwrap());
        let settings = Arc::new(Mutex::new(
            crate::settings::SettingsStore::load(Some(directory.path().into())).unwrap(),
        ));
        let host = SourceHost::new(provider, plugins, directory.path(), output, settings).unwrap();
        let key = api::SessionKey {
            account: None,
            remote_id: api::SessionId::new("seat").unwrap(),
        };
        host.sessions
            .journal
            .reconcile(
                PluginId::new(BUILTIN_GFN_ID).unwrap(),
                None,
                api::OperationId::new("owned").unwrap(),
                key.clone(),
            )
            .unwrap();
        std::thread::scope(|scope| {
            let catalog=scope.spawn(||host.dispatch_sources("sources.public.page",&json!({"sourceId":BUILTIN_GFN_ID,"request":{"scope":{"kind":"public"},"query":{"query":"","cursor":null,"limit":1}}}),&Cancellation::default()));
            entered
                .recv_timeout(std::time::Duration::from_secs(2))
                .unwrap();
            let stopped = host.dispatch_sources(
                "sources.session.stop",
                &json!({"sourceId":BUILTIN_GFN_ID,"request":{"session":key,"operation":"stop"}}),
                &Cancellation::default(),
            );
            release.send(()).unwrap();
            assert!(stopped.result.is_ok());
            assert!(catalog.join().unwrap().result.is_ok());
        });
        assert_eq!(host.sessions.occupancy(), SessionOccupancy::Idle);
        host.shutdown();
    }
}
