use super::contract::{
    AllocationDisposition, AllocationReceipt, Completion, ProviderCompletion, ProviderContext,
    ProviderSource, ReceiptOutcome, SessionOccupancy, SourceError,
};
use super::journal::{Phase, SessionJournal};
use crate::requests::{self, Cancellation};
use opennow_plugin_api::{PluginId, provider as api};
use rand::RngCore;
use serde_json::{Value, json};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, mpsc::Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

struct Watched {
    source: PluginId,
    provider: Arc<dyn ProviderSource>,
}

pub struct SessionManager {
    pub journal: Arc<SessionJournal>,
    transition: Mutex<()>,
    watched: Mutex<Vec<Watched>>,
    closing: AtomicBool,
    monitor: Mutex<Option<JoinHandle<()>>>,
    output: Sender<Value>,
}

impl SessionManager {
    #[cfg(test)]
    pub(super) fn with_held_transition<T>(&self, test: impl FnOnce() -> T) -> T {
        let _guard = self.transition.lock().expect("session transition poisoned");
        test()
    }
    pub fn open(path: &Path, output: Sender<Value>) -> Result<Arc<Self>, SourceError> {
        let manager = Arc::new(Self {
            journal: Arc::new(SessionJournal::open(path)),
            transition: Mutex::new(()),
            watched: Mutex::new(Vec::new()),
            closing: AtomicBool::new(false),
            monitor: Mutex::new(None),
            output,
        });
        let weak = Arc::downgrade(&manager);
        let handle = thread::Builder::new()
            .name("provider-session-events".into())
            .spawn(move || {
                loop {
                    let Some(manager) = weak.upgrade() else {
                        return;
                    };
                    if manager.closing.load(Ordering::Acquire) {
                        return;
                    }
                    manager.drain_notifications();
                    drop(manager);
                    thread::sleep(Duration::from_millis(100));
                }
            })
            .map_err(|_| {
                SourceError::new(
                    "source_monitor_failed",
                    "Provider session monitoring could not start",
                )
            })?;
        *manager.monitor.lock().expect("session monitor poisoned") = Some(handle);
        Ok(manager)
    }

    pub fn guard(&self) -> Result<MutexGuard<'_, ()>, SourceError> {
        if self.closing.load(Ordering::Acquire) {
            return Err(SourceError::new(
                "core_stopping",
                "OpenNOW is shutting down",
            ));
        }
        self.transition.try_lock().map_err(|_| {
            SourceError::new("busy", "Another provider session transition is in progress")
        })
    }

    pub fn track(
        &self,
        source: &PluginId,
        provider: &Arc<dyn ProviderSource>,
    ) -> Result<(), SourceError> {
        let mut watched = self.watched.lock().expect("provider observations poisoned");
        if watched
            .iter()
            .any(|item| Arc::ptr_eq(&item.provider, provider))
        {
            return Ok(());
        }
        if watched.len() >= 64 {
            return Err(SourceError::new(
                "busy",
                "Provider observation capacity is exhausted",
            ));
        }
        watched.push(Watched {
            source: source.clone(),
            provider: Arc::clone(provider),
        });
        Ok(())
    }

    pub fn occupancy(&self) -> SessionOccupancy {
        self.journal.occupancy()
    }

    pub fn execute(
        self: &Arc<Self>,
        source: PluginId,
        provider: Arc<dyn ProviderSource>,
        mut request: api::ProviderRequest,
        context: &ProviderContext<'_>,
    ) -> Completion {
        let _guard = if matches!(
            request,
            api::ProviderRequest::SessionCreate(_)
                | api::ProviderRequest::SessionClaim(_)
                | api::ProviderRequest::SessionStop(_)
                | api::ProviderRequest::SessionReconcile(_)
                | api::ProviderRequest::AuthLogout(_)
                | api::ProviderRequest::AccountsRemove(_)
        ) {
            match self.guard() {
                Ok(guard) => Some(guard),
                Err(error) => return failed(error),
            }
        } else {
            None
        };
        if let Err(error) = self.track(&source, &provider) {
            return failed(error);
        }
        if let Err(error) = context.cancellation.check() {
            return failed(error.into());
        }
        if let Err(error) = self.authorize(&source, &request) {
            return failed(error);
        }
        let expected_operation = match &request {
            api::ProviderRequest::SessionPoll(key)
            | api::ProviderRequest::SessionStop(api::StopSession { session: key, .. }) => {
                match self.journal.require_session(&source, key) {
                    Ok(record) => Some(record.operation),
                    Err(error) => return failed(error),
                }
            }
            _ => None,
        };
        if let api::ProviderRequest::SessionCreate(create) = &mut request {
            create.operation = new_operation();
            if let Err(error) = self.journal.begin_profile(
                source.clone(),
                create.scope.as_ref().map(|scope| scope.account.clone()),
                create.operation.clone(),
                Some(create.preferences.clone()),
            ) {
                return failed(error);
            }
        }
        if let api::ProviderRequest::SessionStop(stop) = &request
            && let Err(error) = self.journal.cleanup_pending(&source, &stop.session)
        {
            return failed(error);
        }
        if let api::ProviderRequest::SessionClaim(claim) = &request
            && self.journal.snapshot().is_ok_and(|record| record.is_none())
            && let Err(error) =
                self.journal
                    .begin_claim(source.clone(), new_operation(), claim.session.clone())
        {
            return failed(error);
        }
        let generation = provider.generation();
        let response = provider.provider_call(&request, context);
        self.finish(
            source,
            provider,
            generation,
            expected_operation,
            &request,
            response,
        )
    }

    fn authorize(
        &self,
        source: &PluginId,
        request: &api::ProviderRequest,
    ) -> Result<(), SourceError> {
        match request {
            api::ProviderRequest::SessionPoll(key)
            | api::ProviderRequest::SessionPrepare(api::PrepareSession { session: key, .. })
            | api::ProviderRequest::SessionStop(api::StopSession { session: key, .. }) => {
                let record = self.journal.require_session(source, key)?;
                if matches!(request, api::ProviderRequest::SessionPrepare(_))
                    && record.phase != Phase::Active
                {
                    return Err(SourceError::new(
                        "busy",
                        "The provider session is not accepted for playback yet",
                    ));
                }
            }
            api::ProviderRequest::SessionClaim(claim) => {
                if let Some(record) = self.journal.snapshot()? {
                    if record.source_id != *source
                        || record.session.as_ref() != Some(&claim.session)
                        || record.phase != Phase::Active
                        || record.receipt.is_some()
                    {
                        return Err(SourceError::new(
                            "session_in_use",
                            "Another provider session is still owned",
                        ));
                    }
                }
            }
            api::ProviderRequest::SessionReconcile(query) => {
                let record = self.journal.snapshot()?.ok_or_else(owner_mismatch)?;
                if record.source_id != *source
                    || record.operation != query.operation
                    || record.account.as_ref() != query.scope.as_ref().map(|scope| &scope.account)
                    || query.session != record.session
                {
                    return Err(owner_mismatch());
                }
            }
            api::ProviderRequest::AuthLogout(account)
            | api::ProviderRequest::AccountsRemove(account) => {
                if self.journal.snapshot()?.is_some_and(|record| {
                    record.source_id == *source && record.account.as_ref() == Some(account)
                }) {
                    return Err(SourceError::new(
                        "session_in_use",
                        "End the account's session before removing its credentials",
                    ));
                }
            }
            api::ProviderRequest::SessionResolveAllocation(_) => {
                return Err(SourceError::new(
                    "private_operation",
                    "Only the session owner can settle an allocation",
                ));
            }
            _ => {}
        }
        Ok(())
    }

    fn finish(
        self: &Arc<Self>,
        source: PluginId,
        provider: Arc<dyn ProviderSource>,
        generation: u64,
        expected_operation: Option<api::OperationId>,
        request: &api::ProviderRequest,
        response: ProviderCompletion,
    ) -> Completion {
        let generation = response.dispatched_generation.unwrap_or(generation);
        let current_generation = provider.generation();
        let auth_projection = source.is_builtin()
            && response.result.as_ref().is_ok_and(|reply| {
                matches!(
                    reply,
                    api::ProviderReply::AuthStatus(_)
                        | api::ProviderReply::AuthBegin(_)
                        | api::ProviderReply::AuthPoll(_)
                        | api::ProviderReply::AuthComplete(_)
                        | api::ProviderReply::AuthLogout(_)
                        | api::ProviderReply::AccountsSelect(_)
                        | api::ProviderReply::AccountsRemove(_)
                )
            });
        let mut completion = Completion::from(
            response
                .result
                .as_ref()
                .map(|reply| (public_reply(&source, generation, reply), None))
                .map_err(|error| (error.code.clone(), error.message.clone())),
        );
        if current_generation != generation && !auth_projection {
            completion.result = Err(SourceError::new(
                "stale_source",
                "The provider changed before this result could be delivered",
            )
            .into());
        }
        if let api::ProviderRequest::SessionCreate(create) = request {
            if let Some(ticket) = response.allocation {
                if ticket.operation != create.operation
                    || ticket.session.account
                        != create.scope.as_ref().map(|scope| scope.account.clone())
                {
                    let _ = self.journal.unknown(&source, &create.operation);
                    return failed(SourceError::new(
                        "provider_protocol_error",
                        "Provider allocation ownership did not match its request",
                    ));
                }
                let persisted = self.journal.allocated(
                    &source,
                    &create.operation,
                    ticket.session.clone(),
                    ticket.receipt.clone(),
                );
                if let Err(error) = persisted {
                    completion.result = Err(error.into());
                }
                completion.receipt = Some(Box::new(OwnedReceipt {
                    manager: Arc::clone(self),
                    source: source.clone(),
                    provider: Arc::clone(&provider),
                    generation,
                    ticket,
                }));
            } else if response.result.is_ok() {
                let _ = self.journal.unknown(&source, &create.operation);
                completion.result = Err(SourceError::new(
                    "provider_protocol_error",
                    "A fresh allocation returned no ownership receipt",
                )
                .into());
            } else if response.result.is_err() {
                let proved = source.is_builtin()
                    && matches!(
                        response.allocation_disposition,
                        Some(
                            AllocationDisposition::NotDispatched
                                | AllocationDisposition::Rejected
                                | AllocationDisposition::CleanedUp { .. }
                        )
                    );
                let result = if !response.dispatched
                    || matches!(
                        response.allocation_disposition,
                        Some(AllocationDisposition::NotDispatched)
                    ) {
                    self.journal
                        .rejected_before_allocation(&source, &create.operation)
                } else if proved {
                    self.journal
                        .rejected_allocation_resolved(&source, &create.operation)
                } else {
                    completion.result=Err(SourceError::new("outcome_unknown","The provider allocation outcome is unknown; reconcile the original request before retrying").into());
                    completion.required_events.push((
                        "sources.session.cleanup",
                        json!({"sourceId":source,"state":"unknown"}),
                    ));
                    self.journal.unknown(&source, &create.operation)
                };
                if let Err(error) = result {
                    completion.result = Err(error.into());
                }
            }
        } else if let Ok(reply) = &response.result {
            if let Some(expected) = expected_operation
                && self
                    .journal
                    .snapshot()
                    .ok()
                    .flatten()
                    .is_none_or(|record| record.source_id != source || record.operation != expected)
            {
                completion.result = Err(owner_mismatch().into());
                return completion;
            }
            if let (
                api::ProviderRequest::SessionReconcile(query),
                api::ProviderReply::SessionReconcile(recovered),
            ) = (request, reply)
            {
                let handled = self.recover_obligation(&source, &provider, query, recovered);
                match handled {
                    Ok(Some(reply)) => {
                        completion.result = Ok((
                            public_reply(
                                &source,
                                generation,
                                &api::ProviderReply::SessionReconcile(reply),
                            ),
                            None,
                        ));
                        completion
                            .required_events
                            .push(("sources.session.changed", json!({"sourceId":source})));
                        return completion;
                    }
                    Err(error) => {
                        completion.result = Err(error.into());
                        completion.required_events.push((
                            "sources.session.cleanup",
                            json!({"sourceId":source,"state":"unknown"}),
                        ));
                        return completion;
                    }
                    Ok(None) => {}
                }
            }
            let result = self.apply_session_reply(&source, request, reply);
            if let Err(error) = result {
                completion.result = Err(error.into());
            }
        }
        if !response.effects.is_empty() {
            completion.required_events.push((
                "sources.changed",
                json!({"sourceId":source,"generation":provider.generation()}),
            ));
        }
        completion
    }

    fn apply_session_reply(
        &self,
        source: &PluginId,
        request: &api::ProviderRequest,
        reply: &api::ProviderReply,
    ) -> Result<(), SourceError> {
        match (request, reply) {
            (
                api::ProviderRequest::SessionStop(stop),
                api::ProviderReply::SessionStop(api::CleanupState::Resolved),
            ) => self.journal.remote_ended(source, &stop.session),
            (api::ProviderRequest::SessionStop(stop), _) => {
                self.journal.cleanup_pending(source, &stop.session)
            }
            (api::ProviderRequest::SessionPoll(key), api::ProviderReply::SessionPoll(view)) => {
                if view.key != *key {
                    return Err(owner_mismatch());
                }
                if matches!(view.state, api::RemoteSessionState::Finished { .. }) {
                    self.journal.remote_ended(source, key)?;
                }
                Ok(())
            }
            (api::ProviderRequest::SessionClaim(claim), api::ProviderReply::SessionClaim(view)) => {
                if claim.session != view.key {
                    return Err(owner_mismatch());
                }
                let operation = self
                    .journal
                    .snapshot()?
                    .map(|record| record.operation)
                    .unwrap_or_else(new_operation);
                self.journal.reconcile(
                    source.clone(),
                    view.key.account.clone(),
                    operation,
                    view.key.clone(),
                )
            }
            (
                api::ProviderRequest::SessionReconcile(query),
                api::ProviderReply::SessionReconcile(result),
            ) => match result {
                api::Reconciliation::Active { session } => {
                    let record = self.journal.snapshot()?.ok_or_else(owner_mismatch)?;
                    if !matches!(
                        record.phase,
                        Phase::Active
                            | Phase::Claiming
                            | Phase::RecoveryPending
                            | Phase::ActiveReceiptConflict
                    ) {
                        self.journal
                            .learn_session(source, &query.operation, &session.key)?;
                        return Err(SourceError::new(
                            "session_cleanup_pending",
                            "The recovered seat still has an unresolved allocation or cleanup obligation",
                        ));
                    }
                    self.journal.reconcile(
                        source.clone(),
                        session.key.account.clone(),
                        query.operation.clone(),
                        session.key.clone(),
                    )
                }
                api::Reconciliation::Terminal { session, .. } => {
                    self.journal.remote_ended(source, session)
                }
                api::Reconciliation::Unknown { operation } => {
                    self.journal.recovery_unknown(source, operation)
                }
                api::Reconciliation::NotAllocated { operation } => self.journal.not_allocated(
                    source,
                    query.scope.as_ref().map(|scope| &scope.account),
                    operation,
                ),
                api::Reconciliation::PendingAllocation { .. } => Err(owner_mismatch()),
            },
            _ => Ok(()),
        }
    }

    fn recover_obligation(
        self: &Arc<Self>,
        source: &PluginId,
        provider: &Arc<dyn ProviderSource>,
        query: &api::ReconcileSession,
        recovered: &api::Reconciliation,
    ) -> Result<Option<api::Reconciliation>, SourceError> {
        let record = self.journal.snapshot()?.ok_or_else(owner_mismatch)?;
        match recovered {
            api::Reconciliation::PendingAllocation { session, ticket } => {
                if ticket.operation != query.operation
                    || ticket.session != session.key
                    || ticket.session.account != record.account
                {
                    return Err(owner_mismatch());
                }
                if matches!(
                    record.phase,
                    Phase::Active
                        | Phase::Claiming
                        | Phase::RecoveryPending
                        | Phase::ActiveReceiptConflict
                ) {
                    self.journal.retain_conflicting_ticket(
                        source,
                        &query.operation,
                        &session.key,
                        &ticket.receipt,
                    )?;
                    return Err(SourceError::new(
                        "recovery_conflict",
                        "The provider reported pending allocation for an accepted session; existing media remains owned",
                    ));
                }
                self.journal.recovered_ticket(
                    source,
                    &query.operation,
                    &session.key,
                    &ticket.receipt,
                )?;
                let receipt = OwnedReceipt {
                    manager: Arc::clone(self),
                    source: source.clone(),
                    provider: Arc::clone(provider),
                    generation: provider.generation(),
                    ticket: ticket.clone(),
                };
                receipt.resolve(false).result?;
                Ok(Some(api::Reconciliation::Terminal {
                    session: session.key.clone(),
                    reason: api::TerminalReason::AllocationRejected,
                }))
            }
            api::Reconciliation::Active { session }
                if !matches!(
                    record.phase,
                    Phase::Active
                        | Phase::Claiming
                        | Phase::RecoveryPending
                        | Phase::ActiveReceiptConflict
                        | Phase::RemoteEnded
                ) =>
            {
                if record.media.is_some() && record.phase != Phase::CleanupPending {
                    return Err(owner_mismatch());
                }
                self.journal
                    .learn_session(source, &query.operation, &session.key)?;
                self.journal.cleanup_pending(source, &session.key)?;
                let cancellation = Cancellation::default();
                let stopped = provider.provider_call(
                    &api::ProviderRequest::SessionStop(api::StopSession {
                        session: session.key.clone(),
                        operation: new_operation(),
                    }),
                    &ProviderContext {
                        cancellation: &cancellation,
                        runtime_capabilities: None,
                        gfn_settings: None,
                    },
                );
                match stopped.result {
                    Ok(api::ProviderReply::SessionStop(api::CleanupState::Resolved)) => {
                        self.journal.remote_ended(source, &session.key)?;
                        Ok(Some(api::Reconciliation::Terminal {
                            session: session.key.clone(),
                            reason: api::TerminalReason::UserStopped,
                        }))
                    }
                    Err(error) => Err(error),
                    _ => Err(SourceError::new(
                        "session_cleanup_pending",
                        "The recovered session could not be stopped",
                    )),
                }
            }
            _ => Ok(None),
        }
    }

    pub fn check_preparation(
        &self,
        source: &PluginId,
        request: &api::PrepareSession,
    ) -> Result<(), SourceError> {
        self.authorize(
            source,
            &api::ProviderRequest::SessionPrepare(request.clone()),
        )
    }

    fn drain_notifications(self: &Arc<Self>) {
        let Ok(_guard) = self.transition.try_lock() else {
            return;
        };
        let watched = {
            let watched = self.watched.lock().expect("provider observations poisoned");
            watched
                .iter()
                .map(|item| (item.source.clone(), Arc::clone(&item.provider)))
                .collect::<Vec<_>>()
        };
        for (source, provider) in watched {
            for notification in provider.take_notifications() {
                if let api::ProviderRequest::SessionCreate(create) = &notification.request
                    && let Some(ticket) = notification.response.allocation
                    && ticket.operation == create.operation
                    && self
                        .journal
                        .snapshot()
                        .ok()
                        .flatten()
                        .is_some_and(|record| {
                            record.source_id == source && record.operation == create.operation
                        })
                {
                    if let Ok(Some(record)) = self.journal.snapshot()
                        && matches!(
                            record.phase,
                            Phase::Active
                                | Phase::RecoveryPending
                                | Phase::ActiveReceiptConflict
                                | Phase::RemoteEnded
                        )
                    {
                        if record.phase != Phase::RemoteEnded {
                            let _ = self.journal.retain_conflicting_ticket(
                                &source,
                                &create.operation,
                                &ticket.session,
                                &ticket.receipt,
                            );
                        }
                        let _=self.output.send(json!({"type":"event","name":"sources.session.changed","payload":{"sourceId":source}}));
                        continue;
                    }
                    let _ = self.journal.allocated(
                        &source,
                        &create.operation,
                        ticket.session.clone(),
                        ticket.receipt.clone(),
                    );
                    let receipt = OwnedReceipt {
                        manager: Arc::clone(self),
                        source: source.clone(),
                        generation: notification.response.epoch.get(),
                        provider: Arc::clone(&provider),
                        ticket,
                    };
                    let outcome = receipt.resolve(false);
                    for (name, payload) in outcome.required_events {
                        let _ = self
                            .output
                            .send(json!({"type":"event","name":name,"payload":payload}));
                    }
                }
                let _ = self.output.send(json!({"type":"event","name":"sources.changed", "payload":{"sourceId":source,"generation":provider.generation()}}));
            }
        }
        self.watched
            .lock()
            .expect("provider observations poisoned")
            .retain(|item| {
                let descriptor = item.provider.descriptor();
                self.journal.source_in_use(&item.source)
                    || descriptor.enabled
                    || descriptor.state != opennow_plugin_api::PluginState::Disabled
            });
    }

    pub fn shutdown(&self) {
        self.closing.store(true, Ordering::Release);
        let Some(handle) = self
            .monitor
            .lock()
            .expect("session monitor poisoned")
            .take()
        else {
            return;
        };
        let deadline = Instant::now() + Duration::from_millis(150);
        while !handle.is_finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        if handle.is_finished() {
            let _ = handle.join();
        }
    }
}

struct OwnedReceipt {
    manager: Arc<SessionManager>,
    source: PluginId,
    provider: Arc<dyn ProviderSource>,
    generation: u64,
    ticket: api::AllocationTicket,
}

impl OwnedReceipt {
    fn resolve(&self, accepted: bool) -> ReceiptOutcome {
        if self.manager.journal.snapshot().is_ok_and(|record| {
            record.is_none_or(|record| {
                record.source_id != self.source || record.operation != self.ticket.operation
            })
        }) {
            return ReceiptOutcome {
                result: Ok(()),
                required_events: Vec::new(),
            };
        }
        let accepted = accepted && self.provider.generation() == self.generation;
        if !accepted {
            let _ = self.manager.journal.learn_session(
                &self.source,
                &self.ticket.operation,
                &self.ticket.session,
            );
            let _ = self
                .manager
                .journal
                .cleanup_pending(&self.source, &self.ticket.session);
        }
        let cancellation = Cancellation::default();
        let result = self.provider.provider_call(
            &api::ProviderRequest::SessionResolveAllocation(api::ResolveAllocation {
                operation: self.ticket.operation.clone(),
                receipt: self.ticket.receipt.clone(),
                decision: if accepted {
                    api::Acceptance::Accepted
                } else {
                    api::Acceptance::Rejected
                },
            }),
            &ProviderContext {
                cancellation: &cancellation,
                runtime_capabilities: None,
                gfn_settings: None,
            },
        );
        match result.result {
            Ok(api::ProviderReply::SessionResolveAllocation(api::CleanupState::Resolved)) => {
                if self.manager.journal.snapshot().is_ok_and(|record| {
                    record.is_none_or(|record| {
                        record.source_id != self.source || record.operation != self.ticket.operation
                    })
                }) {
                    return ReceiptOutcome {
                        result: Ok(()),
                        required_events: Vec::new(),
                    };
                }
                let saved = if accepted {
                    self.manager
                        .journal
                        .settled(&self.source, &self.ticket.operation, true)
                } else {
                    self.manager
                        .journal
                        .rejected_allocation_resolved(&self.source, &self.ticket.operation)
                };
                if let Err(error) = saved {
                    return self.failed(error);
                }
                ReceiptOutcome {
                    result: Ok(()),
                    required_events: vec![(
                        "sources.session.changed",
                        json!({"sourceId":self.source}),
                    )],
                }
            }
            Err(error) => {
                let _ = self
                    .manager
                    .journal
                    .unknown(&self.source, &self.ticket.operation);
                self.failed(error)
            }
            _ => {
                let _ = self
                    .manager
                    .journal
                    .unknown(&self.source, &self.ticket.operation);
                self.failed(SourceError::new(
                    "session_cleanup_pending",
                    "Provider session ownership could not be settled",
                ))
            }
        }
    }

    fn failed(&self, error: SourceError) -> ReceiptOutcome {
        ReceiptOutcome {
            result: Err(error),
            required_events: vec![(
                "sources.session.cleanup",
                json!({"sourceId":self.source,"state":"unknown"}),
            )],
        }
    }
}

impl AllocationReceipt for OwnedReceipt {
    fn settle(self: Box<Self>, accepted: bool) -> ReceiptOutcome {
        requests::scope(Cancellation::default(), || self.resolve(accepted))
    }
}

pub fn public_reply(source: &PluginId, generation: u64, reply: &api::ProviderReply) -> Value {
    let mut payload = serde_json::to_value(reply).expect("typed provider reply");
    let mut result = payload["result"].take();
    if result["state"] == "pending-allocation"
        && let Some(fields) = result.as_object_mut()
    {
        fields.remove("ticket");
    }
    json!({"sourceId":source,"generation":generation,"result":result})
}

pub fn new_operation() -> api::OperationId {
    let mut bytes = [0u8; 24];
    rand::rng().fill_bytes(&mut bytes);
    api::OperationId::new(
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
    )
    .expect("bounded operation identity")
}

fn failed(error: SourceError) -> Completion {
    Completion::from(Err(error.into()))
}
fn owner_mismatch() -> SourceError {
    SourceError::new(
        "session_owner_mismatch",
        "Provider session ownership changed",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::contract::{CatalogSource, NativePreparation, ProviderNotification};
    use opennow_plugin_api::{
        CatalogPage, CatalogQuery, Coverage, PluginDescriptor, PluginState, PluginTrust,
    };
    use std::num::NonZeroU64;
    use std::sync::atomic::{AtomicU64, AtomicUsize};

    struct Fixture {
        generation: AtomicU64,
        cleanup: AtomicUsize,
        recovery_mode: AtomicUsize,
        captured: Mutex<Option<api::ProviderRequest>>,
        notifications: Mutex<Vec<ProviderNotification>>,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                generation: AtomicU64::new(1),
                cleanup: AtomicUsize::new(0),
                recovery_mode: AtomicUsize::new(0),
                captured: Mutex::new(None),
                notifications: Mutex::new(Vec::new()),
            }
        }
    }

    impl CatalogSource for Fixture {
        fn descriptor(&self) -> PluginDescriptor {
            PluginDescriptor {
                id: source(),
                name: "Fixture".into(),
                version: "1.0.0".into(),
                publisher: "Fixture".into(),
                description: "Fixture".into(),
                builtin: false,
                required: false,
                enabled: true,
                state: PluginState::Failed,
                capabilities: vec![],
                trust: PluginTrust::UnsignedNative,
                last_error: None,
            }
        }
        fn generation(&self) -> u64 {
            self.generation.load(Ordering::Acquire)
        }
        fn catalog_page(
            &self,
            _: &CatalogQuery,
            _: &Cancellation,
        ) -> Result<CatalogPage, SourceError> {
            Ok(CatalogPage {
                items: vec![],
                next_cursor: None,
                coverage: Coverage::Unknown,
            })
        }
    }

    impl ProviderSource for Fixture {
        fn provider_capabilities(&self) -> Vec<api::Capability> {
            vec![api::Capability::Sessions]
        }
        fn auth_kinds(&self) -> Vec<api::AuthKind> {
            vec![api::AuthKind::Anonymous]
        }
        fn provider_call(
            &self,
            request: &api::ProviderRequest,
            _: &ProviderContext<'_>,
        ) -> ProviderCompletion {
            match request {
                api::ProviderRequest::SessionCreate(_) => {
                    *self.captured.lock().unwrap() = Some(request.clone());
                    ProviderCompletion::failed(SourceError::new(
                        "busy",
                        "Untrusted provider says no allocation",
                    ))
                }
                api::ProviderRequest::SessionResolveAllocation(query) => {
                    assert_eq!(query.decision, api::Acceptance::Rejected);
                    self.cleanup.fetch_add(1, Ordering::AcqRel);
                    reply(api::ProviderReply::SessionResolveAllocation(
                        api::CleanupState::Resolved,
                    ))
                }
                api::ProviderRequest::SessionReconcile(query) => {
                    let recovered = if self.recovery_mode.load(Ordering::Acquire) == 1 {
                        api::Reconciliation::PendingAllocation {
                            session: api::SessionView {
                                key: key(),
                                target: api::LaunchTarget {
                                    game: api::GameId::new("game").unwrap(),
                                    variant: api::VariantId::new("variant").unwrap(),
                                },
                                state: api::RemoteSessionState::Ready,
                            },
                            ticket: api::AllocationTicket {
                                operation: query.operation.clone(),
                                receipt: api::ReceiptId::new("private-receipt-sentinel").unwrap(),
                                session: key(),
                            },
                        }
                    } else {
                        api::Reconciliation::NotAllocated {
                            operation: query.operation.clone(),
                        }
                    };
                    reply(api::ProviderReply::SessionReconcile(recovered))
                }
                _ => ProviderCompletion::failed(SourceError::new(
                    "unsupported_feature",
                    "Unsupported fixture operation",
                )),
            }
        }
        fn prepare_native(
            &self,
            _: &api::PrepareSession,
            _: &ProviderContext<'_>,
        ) -> Result<NativePreparation, SourceError> {
            Err(owner_mismatch())
        }
        fn take_notifications(&self) -> Vec<ProviderNotification> {
            std::mem::take(&mut *self.notifications.lock().unwrap())
        }
    }

    fn reply(value: api::ProviderReply) -> ProviderCompletion {
        ProviderCompletion {
            result: Ok(value),
            effects: vec![],
            allocation: None,
            dispatched: true,
            allocation_disposition: None,
            dispatched_generation: None,
        }
    }
    fn source() -> PluginId {
        PluginId::new("org.example.provider").unwrap()
    }
    fn key() -> api::SessionKey {
        api::SessionKey {
            account: None,
            remote_id: api::SessionId::new("seat").unwrap(),
        }
    }

    fn create_request() -> api::ProviderRequest {
        serde_json::from_value(json!({"method":"session.create","params":{
            "scope":null,"operation":"intent","target":{"game":"game","variant":"variant"},"catalogRevision":"1","settingsRevision":1,
            "preferences":{"video":{"width":640,"height":360,"encoding":null,"fps":null,"bitDepth":8,"chroma":"yuv420","hdr":false},"bitrateKbps":75000},
            "offer":{"version":1,"offerId":"offer","runtimeEpoch":1,"expiresAtMs":u64::MAX,
                "videoFormats":[{"encoding":"h264-annex-b","bitDepth":8,"chroma":"yuv420","dynamicRange":"sdr","maxWidth":1920,"maxHeight":1080,"maxFps":60}],
                "audioFormats":[],"input":{"keyboard":true,"relativeMouse":true,"absoluteMouse":false,"text":false,"gamepadSlots":0,"rumble":false},
                "limits":{"maxVideoAccessUnitBytes":1048576,"maxAudioPacketBytes":65536,"maxBufferedVideoBytes":2097152,"maxBufferedVideoFrames":2,"maxBufferedAudioMs":100,"maxControlMessageBytes":65536,"maxPendingInputEvents":64}}
        }})).unwrap()
    }

    #[test]
    fn dispatched_failure_never_clears_allocation_from_a_provider_error_name() {
        let directory = tempfile::tempdir().unwrap();
        let (output, _) = std::sync::mpsc::channel();
        let manager = SessionManager::open(directory.path(), output).unwrap();
        let provider = Arc::new(Fixture::new());
        let cancellation = Cancellation::default();
        let completion = manager.execute(
            source(),
            provider,
            create_request(),
            &ProviderContext {
                cancellation: &cancellation,
                runtime_capabilities: None,
                gfn_settings: None,
            },
        );
        assert_eq!(completion.result.unwrap_err().0, "outcome_unknown");
        assert_eq!(manager.occupancy(), SessionOccupancy::Unknown);
        manager.shutdown();
    }

    #[test]
    fn late_ticket_after_eof_is_rejected_on_original_owner_without_dropping_tail() {
        let directory = tempfile::tempdir().unwrap();
        let (output, _) = std::sync::mpsc::channel();
        let manager = SessionManager::open(directory.path(), output).unwrap();
        let provider = Arc::new(Fixture::new());
        let cancellation = Cancellation::default();
        manager.execute(
            source(),
            provider.clone(),
            create_request(),
            &ProviderContext {
                cancellation: &cancellation,
                runtime_capabilities: None,
                gfn_settings: None,
            },
        );
        let request = provider.captured.lock().unwrap().clone().unwrap();
        let api::ProviderRequest::SessionCreate(create) = &request else {
            panic!("captured create")
        };
        let mut batch = Vec::new();
        for index in 0..35 {
            batch.push(ProviderNotification {
                request: api::ProviderRequest::AuthStatus(api::Empty {}),
                response: api::ProviderResponseV2 {
                    v: api::Version2,
                    epoch: NonZeroU64::new(1).unwrap(),
                    id: api::Text::new(format!("auth-{index}")).unwrap(),
                    outcome: api::ProviderOutcome::Success {
                        reply: Box::new(api::ProviderReply::AuthStatus(api::AuthState::SignedOut)),
                    },
                    effects: api::List::default(),
                    allocation: None,
                },
            });
        }
        batch.push(ProviderNotification {
            request: request.clone(),
            response: api::ProviderResponseV2 {
                v: api::Version2,
                epoch: NonZeroU64::new(1).unwrap(),
                id: api::Text::new("late").unwrap(),
                outcome: api::ProviderOutcome::Failure {
                    error: api::ProviderError {
                        code: api::ProviderErrorCode::OutcomeUnknown,
                        retry_after_ms: None,
                    },
                },
                effects: api::List::default(),
                allocation: Some(api::AllocationTicket {
                    operation: create.operation.clone(),
                    receipt: api::ReceiptId::new("receipt").unwrap(),
                    session: key(),
                }),
            },
        });
        provider.generation.store(2, Ordering::Release);
        *provider.notifications.lock().unwrap() = batch;
        let until = Instant::now() + Duration::from_secs(2);
        while manager.occupancy() != SessionOccupancy::Idle && Instant::now() < until {
            manager.drain_notifications();
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(provider.cleanup.load(Ordering::Acquire), 1);
        assert_eq!(manager.occupancy(), SessionOccupancy::Idle);
        assert!(provider.notifications.lock().unwrap().is_empty());
        manager.shutdown();
    }

    #[test]
    fn only_positive_exact_operation_no_allocation_evidence_retires_unknown() {
        let directory = tempfile::tempdir().unwrap();
        let (output, _) = std::sync::mpsc::channel();
        let manager = SessionManager::open(directory.path(), output).unwrap();
        let provider = Arc::new(Fixture::new());
        let cancellation = Cancellation::default();
        manager.execute(
            source(),
            provider.clone(),
            create_request(),
            &ProviderContext {
                cancellation: &cancellation,
                runtime_capabilities: None,
                gfn_settings: None,
            },
        );
        let record = manager.journal.snapshot().unwrap().unwrap();
        let wrong = api::ProviderRequest::SessionReconcile(api::ReconcileSession {
            scope: None,
            operation: api::OperationId::new("wrong").unwrap(),
            session: None,
        });
        assert!(
            manager
                .execute(
                    source(),
                    provider.clone(),
                    wrong,
                    &ProviderContext {
                        cancellation: &cancellation,
                        runtime_capabilities: None,
                        gfn_settings: None
                    }
                )
                .result
                .is_err()
        );
        assert_eq!(manager.occupancy(), SessionOccupancy::Unknown);
        let correct = api::ProviderRequest::SessionReconcile(api::ReconcileSession {
            scope: None,
            operation: record.operation,
            session: None,
        });
        assert!(
            manager
                .execute(
                    source(),
                    provider,
                    correct,
                    &ProviderContext {
                        cancellation: &cancellation,
                        runtime_capabilities: None,
                        gfn_settings: None
                    }
                )
                .result
                .is_ok()
        );
        assert_eq!(manager.occupancy(), SessionOccupancy::Idle);
        manager.shutdown();
    }

    #[test]
    fn recovered_pending_ticket_is_rejected_privately_and_never_leaks_to_the_shell() {
        let directory = tempfile::tempdir().unwrap();
        let (output, _) = std::sync::mpsc::channel();
        let manager = SessionManager::open(directory.path(), output).unwrap();
        let provider = Arc::new(Fixture::new());
        let cancellation = Cancellation::default();
        let context = ProviderContext {
            cancellation: &cancellation,
            runtime_capabilities: None,
            gfn_settings: None,
        };
        manager.execute(source(), provider.clone(), create_request(), &context);
        let operation = manager.journal.snapshot().unwrap().unwrap().operation;
        provider.recovery_mode.store(1, Ordering::Release);
        let completion = manager.execute(
            source(),
            provider.clone(),
            api::ProviderRequest::SessionReconcile(api::ReconcileSession {
                scope: None,
                operation,
                session: None,
            }),
            &context,
        );
        let public = completion.result.unwrap().0;
        assert_eq!(public["result"]["state"], "terminal");
        assert!(!public.to_string().contains("private-receipt-sentinel"));
        assert_eq!(provider.cleanup.load(Ordering::Acquire), 1);
        assert_eq!(manager.occupancy(), SessionOccupancy::Idle);
        manager.shutdown();
    }

    #[test]
    fn pending_ticket_conflict_never_rejects_accepted_native_media() {
        let directory = tempfile::tempdir().unwrap();
        let (output, _) = std::sync::mpsc::channel();
        let manager = SessionManager::open(directory.path(), output).unwrap();
        let provider = Arc::new(Fixture::new());
        let operation = api::OperationId::new("accepted").unwrap();
        manager
            .journal
            .reconcile(source(), None, operation.clone(), key())
            .unwrap();
        manager
            .journal
            .pin_media(
                &source(),
                &key(),
                super::super::journal::MediaPin {
                    lease_id: "native-lease".into(),
                    package_sha256: Some("a".repeat(64)),
                    runtime_epoch: 1,
                    attempt_id: "native-attempt".into(),
                },
            )
            .unwrap();
        provider.recovery_mode.store(1, Ordering::Release);
        let cancellation = Cancellation::default();
        let completion = manager.execute(
            source(),
            provider.clone(),
            api::ProviderRequest::SessionReconcile(api::ReconcileSession {
                scope: None,
                operation,
                session: Some(key()),
            }),
            &ProviderContext {
                cancellation: &cancellation,
                runtime_capabilities: None,
                gfn_settings: None,
            },
        );
        assert_eq!(completion.result.unwrap_err().0, "recovery_conflict");
        let retained = manager.journal.require_session(&source(), &key()).unwrap();
        assert!(retained.media.is_some());
        assert_eq!(retained.phase, Phase::ActiveReceiptConflict);
        assert_eq!(provider.cleanup.load(Ordering::Acquire), 0);
        manager.shutdown();
    }
}
