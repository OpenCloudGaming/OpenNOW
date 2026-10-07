use super::provider_transport::{ProviderTransport, Submitted};
use super::transport::Transport;
use super::{Changes, error};
use crate::requests::Cancellation;
use crate::sources::contract::{
    CatalogSource, NativePreparation, ProviderCompletion, ProviderContext, ProviderNotification,
    ProviderSource, SourceError,
};
use opennow_plugin_api::provider::{
    AuthKind, Capability, ProviderHello, ProviderManifest, ProviderOutcome, ProviderReply,
    ProviderRequest, Text,
};
use opennow_plugin_api::{CatalogPage, CatalogQuery, PluginDescriptor, PluginError, PluginState};
use opennow_plugin_package::{InstalledManifest, Role};
use rand::RngCore;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub(super) struct ProviderProcess {
    manifest: ProviderManifest,
    descriptor: Mutex<PluginDescriptor>,
    runtime: Mutex<Option<Arc<ProviderTransport>>>,
    lifecycle: Mutex<()>,
    launch: Mutex<Option<(PathBuf, PathBuf)>>,
    recovery: Mutex<(u8, Instant)>,
    generation: AtomicU64,
    closed: AtomicBool,
    changes: Arc<Changes>,
}

struct DispatchedCall {
    runtime: Arc<ProviderTransport>,
    generation: u64,
    submitted: Submitted,
}

impl DispatchedCall {
    fn complete(
        self,
        request: &ProviderRequest,
        cancellation: &Cancellation,
    ) -> ProviderCompletion {
        let (result, effects, allocation) =
            match self.runtime.finish(self.submitted, request, cancellation) {
                Ok(response) => (
                    match response.outcome {
                        ProviderOutcome::Success { reply } => Ok(*reply),
                        ProviderOutcome::Failure { error: remote } => {
                            let code = serde_json::to_value(remote.code)
                                .ok()
                                .and_then(|value| value.as_str().map(str::to_owned))
                                .unwrap_or_else(|| "provider_unavailable".into());
                            Err(SourceError::new(
                                code,
                                "The provider could not complete this operation",
                            ))
                        }
                    },
                    response.effects.iter().cloned().collect(),
                    response.allocation,
                ),
                Err(failure) => (Err(failure), Vec::new(), None),
            };
        ProviderCompletion {
            result,
            effects,
            allocation,
            dispatched: true,
            dispatched_generation: Some(self.generation),
            allocation_disposition: None,
        }
    }
}

impl ProviderProcess {
    pub(super) fn new(
        manifest: ProviderManifest,
        descriptor: PluginDescriptor,
        changes: Arc<Changes>,
    ) -> Self {
        Self {
            manifest,
            descriptor: Mutex::new(descriptor),
            runtime: Mutex::new(None),
            lifecycle: Mutex::new(()),
            launch: Mutex::new(None),
            recovery: Mutex::new((0, Instant::now())),
            generation: AtomicU64::new(1),
            closed: AtomicBool::new(false),
            changes,
        }
    }

    fn changed(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.changes.bump();
    }

    pub(super) fn enable(
        &self,
        root: &Path,
        data: &Path,
        cancellation: &Cancellation,
    ) -> Result<(), SourceError> {
        let _lifecycle = self
            .lifecycle
            .try_lock()
            .map_err(|_| error("plugin_busy"))?;
        if self.closed.load(Ordering::Acquire) {
            return Err(error("plugins_unavailable"));
        }
        if self.descriptor().state == PluginState::Ready {
            return Err(error("plugin_busy"));
        }
        if cancellation.cancelled() {
            return Err(SourceError::cancelled());
        }
        if self
            .runtime
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
        {
            return Err(error("plugin_in_use"));
        }
        *self.launch.lock().unwrap_or_else(|e| e.into_inner()) = Some((root.into(), data.into()));
        *self.recovery.lock().unwrap_or_else(|e| e.into_inner()) = (0, Instant::now());
        self.start(root, data, cancellation)
    }

    fn start(
        &self,
        root: &Path,
        data: &Path,
        cancellation: &Cancellation,
    ) -> Result<(), SourceError> {
        {
            let mut descriptor = self.descriptor.lock().unwrap_or_else(|e| e.into_inner());
            descriptor.enabled = true;
            descriptor.state = PluginState::Starting;
            descriptor.last_error = None;
            self.changed();
        }
        let package = opennow_plugin_package::verify(
            root,
            &InstalledManifest::Provider(self.manifest.clone()),
        )
        .map_err(|failure| {
            self.mark_failed(failure.code);
            SourceError::new(failure.code, failure.message)
        })?;
        super::private_dir(data).inspect_err(|failure| self.mark_failed(&failure.code))?;
        let executable = package
            .entrypoint(Role::Control)
            .ok_or_else(|| error("package_changed"))?;
        let epoch = loop {
            if let Some(epoch) = NonZeroU64::new(rand::rng().next_u64()) {
                break epoch;
            }
        };
        let runtime = {
            let mut slot = self.runtime.lock().unwrap_or_else(|e| e.into_inner());
            if self.closed.load(Ordering::Acquire) {
                return Err(error("plugins_unavailable"));
            }
            let runtime = ProviderTransport::spawn(executable, data, epoch)
                .inspect_err(|failure| self.mark_failed(&failure.code))?;
            *slot = Some(Arc::clone(&runtime));
            runtime
        };
        let hello = ProviderRequest::Hello(ProviderHello {
            plugin_id: self.manifest.id.clone(),
            version: Text::new(self.manifest.version.clone())
                .map_err(|_| error("incompatible_plugin"))?,
            capabilities: self.manifest.capabilities.clone(),
        });
        let result = runtime
            .submit(&hello, cancellation, Duration::from_secs(5))
            .and_then(|submitted| runtime.finish(submitted, &hello, cancellation))
            .and_then(|response| {
                let ProviderOutcome::Success { reply } = response.outcome else {
                    return Err(error("incompatible_plugin"));
                };
                let ProviderReply::Hello(reply) = *reply else {
                    return Err(error("plugin_protocol_error"));
                };
                if reply.protocol_version != 2
                    || reply.plugin_id != self.manifest.id
                    || reply.version.as_str() != self.manifest.version
                    || reply.capabilities != self.manifest.capabilities
                    || reply.auth_kinds != self.manifest.auth_kinds
                    || !response.effects.is_empty()
                    || response.allocation.is_some()
                {
                    return Err(error("incompatible_plugin"));
                }
                Ok(())
            });
        if let Err(failure) = result {
            runtime.stop();
            self.mark_failed(&failure.code);
            return Err(failure);
        }
        if cancellation.cancelled() || self.closed.load(Ordering::Acquire) {
            runtime.stop();
            self.mark_failed("cancelled");
            return Err(SourceError::cancelled());
        }
        let mut descriptor = self.descriptor.lock().unwrap_or_else(|e| e.into_inner());
        descriptor.state = PluginState::Ready;
        self.changed();
        Ok(())
    }

    fn recover(
        &self,
        request: &ProviderRequest,
        cancellation: &Cancellation,
    ) -> Result<(), SourceError> {
        if !matches!(
            request,
            ProviderRequest::SessionReconcile(_)
                | ProviderRequest::SessionStop(_)
                | ProviderRequest::SessionResolveAllocation(_)
        ) {
            return Ok(());
        }
        let _lifecycle = self
            .lifecycle
            .try_lock()
            .map_err(|_| error("busy_before_dispatch"))?;
        let descriptor = self.descriptor();
        if !descriptor.enabled || self.closed.load(Ordering::Acquire) {
            return Err(error("provider_unavailable"));
        }
        let mut slot = self.runtime.lock().unwrap_or_else(|e| e.into_inner());
        if slot.as_ref().is_some_and(|runtime| !runtime.unhealthy())
            && descriptor.state == PluginState::Ready
        {
            return Ok(());
        }
        if slot
            .as_ref()
            .is_some_and(|runtime| runtime.has_notifications())
        {
            return Err(error("busy_before_dispatch"));
        }
        if cancellation.cancelled() {
            return Err(SourceError::cancelled());
        }
        let mut recovery = self.recovery.lock().unwrap_or_else(|e| e.into_inner());
        if recovery.0 >= 3 || Instant::now() < recovery.1 {
            return Err(error("provider_unavailable"));
        }
        recovery.0 += 1;
        recovery.1 = Instant::now() + Duration::from_millis(250 << (recovery.0 - 1));
        drop(recovery);
        if let Some(runtime) = slot.take() {
            runtime
                .stop()
                .reap_until(Instant::now() + Duration::from_millis(500));
        }
        drop(slot);
        let (root, data) = self
            .launch
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or_else(|| error("provider_unavailable"))?;
        self.start(&root, &data, cancellation)
    }

    fn mark_failed(&self, code: &str) {
        let mut descriptor = self.descriptor.lock().unwrap_or_else(|e| e.into_inner());
        if descriptor.state == PluginState::Failed || !descriptor.enabled {
            return;
        }
        descriptor.state = PluginState::Failed;
        descriptor.last_error = Some(PluginError {
            code: code.to_owned(),
            message: "The provider control process is unavailable".to_owned(),
        });
        self.changed();
    }

    pub(super) fn disable(&self) {
        let _lifecycle = self.lifecycle.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(runtime) = self
            .runtime
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            runtime
                .stop()
                .reap_until(std::time::Instant::now() + Duration::from_millis(500));
        }
        let mut descriptor = self.descriptor.lock().unwrap_or_else(|e| e.into_inner());
        descriptor.enabled = false;
        descriptor.state = PluginState::Disabled;
        descriptor.last_error = None;
        self.changed();
    }

    pub(super) fn shutdown(&self) -> Option<Arc<Transport>> {
        self.closed.store(true, Ordering::Release);
        self.runtime
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|runtime| runtime.stop())
    }

    pub(super) fn busy(&self) -> bool {
        self.runtime
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .is_some_and(|runtime| runtime.busy())
    }

    pub(super) fn check_health(&self) {
        let runtime = self.runtime.lock().unwrap_or_else(|e| e.into_inner());
        let unhealthy = runtime.as_ref().is_some_and(|runtime| runtime.unhealthy());
        if unhealthy {
            self.mark_failed("provider_unavailable");
        }
    }

    fn dispatch(
        &self,
        request: &ProviderRequest,
        cancellation: &Cancellation,
        timeout: Duration,
    ) -> Result<DispatchedCall, SourceError> {
        let slot = self.runtime.lock().unwrap_or_else(|e| e.into_inner());
        let descriptor = self.descriptor.lock().unwrap_or_else(|e| e.into_inner());
        if self.closed.load(Ordering::Acquire) || descriptor.state != PluginState::Ready {
            return Err(error("provider_unavailable"));
        }
        let runtime = slot.as_ref().ok_or_else(|| error("provider_unavailable"))?;
        let generation = self.generation.load(Ordering::Acquire);
        let submitted = runtime.submit(request, cancellation, timeout)?;
        Ok(DispatchedCall {
            runtime: Arc::clone(runtime),
            generation,
            submitted,
        })
    }
}

impl CatalogSource for ProviderProcess {
    fn descriptor(&self) -> PluginDescriptor {
        self.descriptor
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    fn catalog_page(&self, _: &CatalogQuery, _: &Cancellation) -> Result<CatalogPage, SourceError> {
        Err(error("unsupported_capability"))
    }
}

impl ProviderSource for ProviderProcess {
    fn take_notifications(&self) -> Vec<ProviderNotification> {
        self.runtime
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map_or_else(Vec::new, |runtime| runtime.take_completions())
    }

    fn provider_capabilities(&self) -> Vec<Capability> {
        self.manifest.capabilities.iter().copied().collect()
    }

    fn auth_kinds(&self) -> Vec<AuthKind> {
        self.manifest.auth_kinds.iter().copied().collect()
    }

    fn provider_call(
        &self,
        request: &ProviderRequest,
        context: &ProviderContext<'_>,
    ) -> ProviderCompletion {
        if !request.permits(&self.manifest.capabilities) {
            return ProviderCompletion::not_dispatched(error("unsupported_capability"));
        }
        if let Err(failure) = self.recover(request, context.cancellation) {
            return ProviderCompletion::not_dispatched(failure);
        }
        let timeout = if matches!(
            request,
            ProviderRequest::SessionResolveAllocation(_) | ProviderRequest::SessionStop(_)
        ) {
            Duration::from_secs(30)
        } else {
            Duration::from_secs(25)
        };
        match self.dispatch(request, context.cancellation, timeout) {
            Ok(dispatched) => dispatched.complete(request, context.cancellation),
            Err(failure) => ProviderCompletion::not_dispatched(failure),
        }
    }

    fn prepare_native(
        &self,
        request: &opennow_plugin_api::provider::PrepareSession,
        context: &ProviderContext<'_>,
    ) -> Result<NativePreparation, SourceError> {
        let request = ProviderRequest::SessionPrepare(request.clone());
        let dispatched = self.dispatch(&request, context.cancellation, Duration::from_secs(25))?;
        let runtime = dispatched.runtime;
        let response = runtime.finish(dispatched.submitted, &request, context.cancellation)?;
        if !response.effects.is_empty() && !runtime.retain_completion(request, response.clone()) {
            runtime.stop();
            return Err(error("provider_unavailable"));
        }
        match response.outcome {
            ProviderOutcome::Success { reply } => match *reply {
                ProviderReply::SessionPrepare(prepared) => {
                    Ok(NativePreparation::External(prepared))
                }
                _ => Err(error("plugin_protocol_error")),
            },
            ProviderOutcome::Failure { error: remote } => {
                let code = serde_json::to_value(remote.code)
                    .ok()
                    .and_then(|value| value.as_str().map(str::to_owned))
                    .unwrap_or_else(|| "provider_unavailable".into());
                Err(SourceError::new(
                    code,
                    "The provider could not prepare media",
                ))
            }
        }
    }
}

#[cfg(test)]
#[path = "provider_process_tests.rs"]
mod tests;
