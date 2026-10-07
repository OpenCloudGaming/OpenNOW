use super::transport::{Failure, Transport};
use crate::requests::Cancellation;
use crate::sources::contract::{ProviderNotification, SourceError};
use opennow_plugin_api::provider::{
    HostMessageV2, HostRequestV2, PluginMessageV2, ProviderRequest, ProviderResponseV2, Text,
    Version2,
};
use std::collections::{BTreeMap, VecDeque};
use std::num::NonZeroU64;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const NOTIFICATION_LIMIT: usize = 32;
const RETIREMENT_GRACE: Duration = Duration::from_secs(2);

#[cfg(test)]
#[derive(Debug)]
pub(super) struct DispatchFailure {
    pub error: SourceError,
    pub dispatched: bool,
}

pub(super) struct Submitted {
    id: String,
    deadline: Instant,
    receiver: Receiver<Result<ProviderResponseV2, SourceError>>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Class {
    Background,
    Control,
    Session,
    Receipt,
}

impl Class {
    fn of(request: &ProviderRequest) -> Self {
        use ProviderRequest::*;
        match request {
            SessionResolveAllocation(_) | SessionStop(_) => Self::Receipt,
            SessionCreate(_) | SessionPoll(_) | SessionDiscover(_) | SessionClaim(_)
            | SessionReconcile(_) | SessionPrepare(_) | SessionAdReport(_) => Self::Session,
            CatalogPublic(_)
            | CatalogLibrary(_)
            | CatalogStore(_)
            | CatalogDetails(_)
            | CatalogDefinitions(_)
            | FavoritesList(_) => Self::Background,
            _ => Self::Control,
        }
    }

    fn limit(self) -> usize {
        match self {
            Self::Receipt => 2,
            _ => 4,
        }
    }
}

struct Pending {
    request: HostRequestV2,
    result: Option<SyncSender<Result<ProviderResponseV2, SourceError>>>,
    deadline: Instant,
    cancelled: bool,
    class: Class,
}

pub(super) struct ProviderTransport {
    transport: Arc<Transport>,
    epoch: NonZeroU64,
    serial: AtomicU64,
    pending: Mutex<BTreeMap<String, Pending>>,
    notifications: Mutex<VecDeque<ProviderNotification>>,
    failed: AtomicBool,
}

impl ProviderTransport {
    pub(super) fn spawn(
        executable: &Path,
        data: &Path,
        epoch: NonZeroU64,
    ) -> Result<Arc<Self>, SourceError> {
        let transport = Transport::spawn_bounded(executable, data, 64)
            .map_err(|_| failure("plugin_start_failed"))?;
        let runtime = Arc::new(Self {
            transport,
            epoch,
            serial: AtomicU64::new(1),
            pending: Mutex::new(BTreeMap::new()),
            notifications: Mutex::new(VecDeque::new()),
            failed: AtomicBool::new(false),
        });
        let weak = Arc::downgrade(&runtime);
        if thread::Builder::new()
            .name("provider-control-replies".into())
            .spawn(move || {
                while let Some(runtime) = weak.upgrade() {
                    if runtime.failed.load(Ordering::Acquire) {
                        return;
                    }
                    runtime.poll();
                }
            })
            .is_err()
        {
            runtime.stop();
            return Err(failure("plugin_start_failed"));
        }
        Ok(runtime)
    }

    #[cfg(test)]
    pub(super) fn call(
        &self,
        request: &ProviderRequest,
        cancellation: &Cancellation,
        timeout: Duration,
    ) -> Result<ProviderResponseV2, DispatchFailure> {
        let submitted = self
            .submit(request, cancellation, timeout)
            .map_err(|error| DispatchFailure {
                error,
                dispatched: false,
            })?;
        self.finish(submitted, request, cancellation)
            .map_err(|error| DispatchFailure {
                error,
                dispatched: true,
            })
    }

    pub(super) fn finish(
        &self,
        submitted: Submitted,
        request: &ProviderRequest,
        cancellation: &Cancellation,
    ) -> Result<ProviderResponseV2, SourceError> {
        self.await_result(
            &submitted.id,
            request,
            cancellation,
            submitted.deadline,
            submitted.receiver,
        )
    }

    pub(super) fn submit(
        &self,
        request: &ProviderRequest,
        cancellation: &Cancellation,
        timeout: Duration,
    ) -> Result<Submitted, SourceError> {
        if cancellation.cancelled() {
            return Err(SourceError::cancelled());
        }
        request.validate().map_err(|_| failure("invalid_params"))?;
        let timeout_ms = timeout.as_millis().clamp(1, 120_000) as u32;
        let id = self
            .serial
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |id| id.checked_add(1))
            .map_err(|_| failure("provider_unavailable"))?
            .to_string();
        let wire = HostRequestV2 {
            v: Version2,
            epoch: self.epoch,
            id: Text::new(id.clone()).map_err(|_| failure("invalid_params"))?,
            timeout_ms,
            request: request.clone(),
        };
        let message = serde_json::to_value(HostMessageV2::Request(Box::new(wire.clone())))
            .map_err(|_| failure("invalid_params"))?;
        let (sender, receiver) = mpsc::sync_channel(1);
        let deadline = Instant::now() + Duration::from_millis(u64::from(timeout_ms));
        {
            let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
            if self.failed.load(Ordering::Acquire) {
                return Err(failure("provider_unavailable"));
            }
            let class = Class::of(request);
            let notification_budget = if class == Class::Receipt {
                NOTIFICATION_LIMIT
            } else {
                NOTIFICATION_LIMIT - Class::Receipt.limit()
            };
            if pending.len()
                + self
                    .notifications
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .len()
                >= notification_budget
            {
                return Err(failure("busy_before_dispatch"));
            }
            if pending.values().filter(|call| call.class == class).count() >= class.limit() {
                return Err(failure("busy_before_dispatch"));
            }
            pending.insert(
                id.clone(),
                Pending {
                    request: wire,
                    result: Some(sender),
                    deadline,
                    cancelled: false,
                    class,
                },
            );
            if let Err(error) = self.transport.send(&message) {
                pending.remove(&id);
                return Err(failure(match error {
                    Failure::Busy => "busy_before_dispatch",
                    _ => "provider_unavailable",
                }));
            }
        }
        Ok(Submitted {
            id,
            deadline,
            receiver,
        })
    }

    fn await_result(
        &self,
        id: &str,
        request: &ProviderRequest,
        cancellation: &Cancellation,
        deadline: Instant,
        receiver: Receiver<Result<ProviderResponseV2, SourceError>>,
    ) -> Result<ProviderResponseV2, SourceError> {
        loop {
            match receiver.recv_timeout(Duration::from_millis(10)) {
                Ok(result) => return result,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(interrupted(request));
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            if cancellation.cancelled() || Instant::now() >= deadline {
                self.cancel(id);
                if read_only(request) || Instant::now() >= deadline {
                    let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(call) = pending.get_mut(id) {
                        call.result = None;
                    }
                    drop(pending);
                    if let Ok(result) = receiver.try_recv() {
                        return result;
                    }
                    return Err(if cancellation.cancelled() && read_only(request) {
                        SourceError::cancelled()
                    } else {
                        interrupted(request)
                    });
                }
            }
        }
    }

    fn cancel(&self, id: &str) {
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        let Some(call) = pending.get_mut(id) else {
            return;
        };
        if call.cancelled {
            return;
        }
        call.cancelled = true;
        let message = HostMessageV2::Cancel {
            v: Version2,
            epoch: self.epoch,
            id: call.request.id.clone(),
        };
        if let Ok(message) = serde_json::to_value(message) {
            let _ = self.transport.send(&message);
        }
    }

    fn poll(&self) {
        match self.transport.receive_frame(Duration::from_millis(20)) {
            Ok(Some(frame)) => match PluginMessageV2::decode(&frame) {
                Ok(PluginMessageV2::Response(response)) => self.complete(response),
                Err(_) => self.fail(),
            },
            Ok(None) => {}
            Err(_) => self.fail(),
        }
        let expired: Vec<_> = self
            .pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|(_, pending)| !pending.cancelled && Instant::now() >= pending.deadline)
            .map(|(id, _)| id.clone())
            .collect();
        for id in expired {
            self.cancel(&id);
        }
        let overdue = self
            .pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .any(|pending| {
                pending.class != Class::Background
                    && Instant::now() >= pending.deadline + RETIREMENT_GRACE
            });
        if overdue {
            self.fail();
        }
    }

    fn complete(&self, response: ProviderResponseV2) {
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        let Some(call) = pending.get(response.id.as_str()) else {
            drop(pending);
            self.fail();
            return;
        };
        if response.validate_for(&call.request).is_err() {
            drop(pending);
            self.fail();
            return;
        }
        let call = pending
            .remove(response.id.as_str())
            .expect("validated request");
        let response = match call.result {
            Some(sender) => match sender.try_send(Ok(response)) {
                Ok(()) => return,
                Err(mpsc::TrySendError::Disconnected(Ok(response)))
                | Err(mpsc::TrySendError::Full(Ok(response))) => response,
                _ => unreachable!(),
            },
            None => response,
        };
        if response.effects.is_empty()
            && response.allocation.is_none()
            && read_only(&call.request.request)
        {
            return;
        }
        let retained = self.retain_completion(call.request.request, response);
        drop(pending);
        if !retained {
            self.fail();
        }
    }

    pub(super) fn retain_completion(
        &self,
        request: ProviderRequest,
        response: ProviderResponseV2,
    ) -> bool {
        let mut notifications = self.notifications.lock().unwrap_or_else(|e| e.into_inner());
        if notifications.len() == NOTIFICATION_LIMIT {
            return false;
        }
        notifications.push_back(ProviderNotification { request, response });
        true
    }

    fn fail(&self) {
        if self.failed.swap(true, Ordering::AcqRel) {
            return;
        }
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        for (_, call) in std::mem::take(&mut *pending) {
            if let Some(sender) = call.result {
                let _ = sender.try_send(Err(interrupted(&call.request.request)));
            }
        }
        drop(pending);
        self.transport.signal_termination();
    }

    pub(super) fn take_completions(&self) -> Vec<ProviderNotification> {
        self.notifications
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain(..)
            .collect()
    }

    pub(super) fn has_notifications(&self) -> bool {
        !self
            .notifications
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_empty()
    }

    pub(super) fn busy(&self) -> bool {
        !self
            .pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_empty()
    }

    pub(super) fn unhealthy(&self) -> bool {
        self.failed.load(Ordering::Acquire) || self.transport.unhealthy()
    }

    pub(super) fn stop(&self) -> Arc<Transport> {
        self.fail();
        Arc::clone(&self.transport)
    }
}

impl Drop for ProviderTransport {
    fn drop(&mut self) {
        self.transport.signal_termination();
    }
}

fn read_only(request: &ProviderRequest) -> bool {
    use ProviderRequest::*;
    matches!(
        request,
        Hello(_)
            | AuthAuthorities(_)
            | AuthStatus(_)
            | AccountsList(_)
            | PinStatus(_)
            | CatalogPublic(_)
            | CatalogLibrary(_)
            | CatalogStore(_)
            | CatalogDetails(_)
            | CatalogDefinitions(_)
            | FavoritesList(_)
            | LaunchInspect(_)
            | SettingsGet(_)
            | SessionPoll(_)
            | SessionDiscover(_)
            | SubscriptionGet(_)
            | ConnectionsList(_)
            | RegionsList(_)
            | StorageList(_)
    )
}

fn interrupted(request: &ProviderRequest) -> SourceError {
    failure(if read_only(request) {
        "provider_unavailable"
    } else {
        "outcome_unknown"
    })
}

fn failure(code: &'static str) -> SourceError {
    SourceError::new(code, "The provider control request could not be completed")
}

#[cfg(test)]
#[path = "provider_transport_tests.rs"]
pub(super) mod tests;
