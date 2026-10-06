use super::transport::{Failure, Transport};
use super::{Changes, error};
use crate::requests::Cancellation;
use crate::sources::contract::{CatalogSource, SourceError};
use opennow_plugin_api::wire::{
    HelloRequest, HostMessage, HostRequest, PluginFailureCode, ReplyPayload, RequestPayload,
};
use opennow_plugin_api::{
    CatalogPage, CatalogQuery, PluginDescriptor, PluginError, PluginManifest, PluginState,
};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub(super) struct ProcessModule {
    descriptor: Mutex<PluginDescriptor>,
    runtime: Mutex<Option<Arc<Transport>>>,
    calls: Mutex<()>,
    epoch: AtomicU64,
    next_id: AtomicU64,
    generation: AtomicU64,
    closed: AtomicBool,
    changes: Arc<Changes>,
    launch: Mutex<Option<Launch>>,
}

#[derive(Clone)]
struct Launch {
    root: PathBuf,
    data: PathBuf,
    manifest: PluginManifest,
}

enum CallFailure {
    Request(SourceError),
    Runtime(SourceError),
    Cancelled { retire: bool },
}

impl CallFailure {
    fn error(&self) -> SourceError {
        match self {
            Self::Request(error) | Self::Runtime(error) => error.clone(),
            Self::Cancelled { .. } => SourceError::cancelled(),
        }
    }
}

impl ProcessModule {
    pub(super) fn new(descriptor: PluginDescriptor, changes: Arc<Changes>) -> Self {
        Self {
            descriptor: Mutex::new(descriptor),
            runtime: Mutex::new(None),
            calls: Mutex::new(()),
            epoch: AtomicU64::new(0),
            next_id: AtomicU64::new(1),
            generation: AtomicU64::new(1),
            closed: AtomicBool::new(false),
            changes,
            launch: Mutex::new(None),
        }
    }

    fn changed(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.changes.bump();
    }

    pub(super) fn enable(
        &self,
        root: &Path,
        manifest: &PluginManifest,
        data: &Path,
        cancellation: &Cancellation,
    ) -> Result<(), SourceError> {
        let _call = self.calls.try_lock().map_err(|_| error("plugin_busy"))?;
        if self.closed.load(Ordering::Acquire) {
            return Err(error("plugins_unavailable"));
        }
        if cancellation.cancelled() {
            return Err(SourceError::cancelled());
        }
        if self.descriptor().state == PluginState::Ready {
            return Err(error("plugin_busy"));
        }
        let launch = Launch {
            root: root.into(),
            data: data.into(),
            manifest: manifest.clone(),
        };
        *self.launch.lock().unwrap_or_else(|e| e.into_inner()) = Some(launch.clone());
        self.start_locked(&launch, self.epoch.load(Ordering::Acquire), cancellation)
    }

    fn start_locked(
        &self,
        launch: &Launch,
        previous_epoch: u64,
        cancellation: &Cancellation,
    ) -> Result<(), SourceError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(error("plugins_unavailable"));
        }
        let epoch = previous_epoch + 1;
        self.epoch
            .compare_exchange(previous_epoch, epoch, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| error("plugin_disabled"))?;
        {
            let mut descriptor = self.descriptor.lock().unwrap_or_else(|e| e.into_inner());
            if self.closed.load(Ordering::Acquire) || self.epoch.load(Ordering::Acquire) != epoch {
                return Err(error("plugin_disabled"));
            }
            descriptor.enabled = true;
            descriptor.state = PluginState::Starting;
            descriptor.last_error = None;
        }
        self.changed();
        let executable = match super::package::verify(&launch.root, &launch.manifest) {
            Ok(executable) => executable,
            Err(_) => {
                self.fail(epoch, "package_changed");
                return Err(error("package_changed"));
            }
        };
        if let Err(failure) = super::private_dir(&launch.data) {
            self.fail(epoch, &failure.code);
            return Err(failure);
        }
        let runtime = {
            let mut slot = self.runtime.lock().unwrap_or_else(|e| e.into_inner());
            if self.closed.load(Ordering::Acquire) || self.epoch.load(Ordering::Acquire) != epoch {
                return Err(error("plugins_unavailable"));
            }
            let runtime = match Transport::spawn(&executable, &launch.data) {
                Ok(runtime) => runtime,
                Err(_) => {
                    drop(slot);
                    self.fail(epoch, "plugin_start_failed");
                    return Err(error("plugin_start_failed"));
                }
            };
            *slot = Some(Arc::clone(&runtime));
            runtime
        };
        let descriptor = self.descriptor();
        let result = self
            .request(
                &runtime,
                epoch,
                RequestPayload::Hello(HelloRequest {
                    plugin_id: descriptor.id.clone(),
                    capabilities: descriptor.capabilities.clone(),
                }),
                Duration::from_secs(5),
                cancellation,
            )
            .map_err(|failure| failure.error())
            .and_then(|result| {
                let ReplyPayload::Hello(hello) = result else {
                    return Err(error("plugin_protocol_error"));
                };
                if hello.plugin_id != descriptor.id
                    || hello.version != descriptor.version
                    || hello.protocol_version != 1
                    || hello.capabilities != descriptor.capabilities
                {
                    return Err(error("incompatible_plugin"));
                }
                Ok(())
            });
        if let Err(failure) = result {
            if failure.code == "cancelled" {
                self.disable();
            } else {
                self.fail(epoch, &failure.code);
            }
            return Err(failure);
        }
        if cancellation.cancelled() || self.epoch.load(Ordering::Acquire) != epoch {
            if self.epoch.load(Ordering::Acquire) == epoch {
                self.disable();
            }
            return Err(SourceError::cancelled());
        }
        {
            let mut descriptor = self.descriptor.lock().unwrap_or_else(|e| e.into_inner());
            if self.closed.load(Ordering::Acquire) || self.epoch.load(Ordering::Acquire) != epoch {
                return Err(error("plugin_disabled"));
            }
            descriptor.enabled = true;
            descriptor.state = PluginState::Ready;
        }
        self.changed();
        Ok(())
    }

    pub(super) fn disable(&self) {
        let idle = self.calls.try_lock().ok();
        let epoch = self.epoch.fetch_add(1, Ordering::AcqRel);
        let runtime = self
            .runtime
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        {
            let mut descriptor = self.descriptor.lock().unwrap_or_else(|e| e.into_inner());
            descriptor.enabled = false;
            descriptor.state = PluginState::Disabled;
            descriptor.last_error = None;
        }
        self.changed();
        if let Some(runtime) = runtime {
            self.stop_transport(&runtime, epoch, idle.is_some());
            let mut slot = self.runtime.lock().unwrap_or_else(|e| e.into_inner());
            if slot
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &runtime))
            {
                slot.take();
            }
        }
    }

    pub(super) fn shutdown(&self) -> Option<Arc<Transport>> {
        self.closed.store(true, Ordering::Release);
        self.epoch.fetch_add(1, Ordering::AcqRel);
        self.retire_runtime()
    }

    fn retire_runtime(&self) -> Option<Arc<Transport>> {
        let mut slot = self.runtime.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(runtime) = slot.as_ref() {
            runtime.signal_termination();
        }
        slot.take()
    }

    fn stop_transport(&self, runtime: &Transport, epoch: u64, idle: bool) {
        if idle {
            let id = self.next_id.fetch_add(1, Ordering::Relaxed).to_string();
            let message = HostMessage::Request(HostRequest {
                v: 1,
                epoch,
                id: id.clone(),
                payload: RequestPayload::Shutdown,
                timeout_ms: 2000,
            });
            if runtime.send(&json!(message)).is_ok() {
                let deadline = Instant::now() + Duration::from_secs(2);
                while Instant::now() < deadline {
                    match runtime.receive(Duration::from_millis(10)) {
                        Ok(None) => {}
                        Ok(Some(reply)) if reply.epoch == epoch && reply.id == id => break,
                        _ => break,
                    }
                }
            }
        }
        runtime.terminate();
    }

    pub(super) fn busy(&self) -> bool {
        self.calls.try_lock().is_err()
    }

    pub(super) fn check_health(&self) {
        let Ok(_call) = self.calls.try_lock() else {
            return;
        };
        if self.descriptor().state != PluginState::Ready {
            return;
        }
        let runtime = self
            .runtime
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(runtime) = runtime {
            let epoch = self.epoch.load(Ordering::Acquire);
            if runtime.unhealthy() {
                self.fail(epoch, "plugin_crashed");
            } else if !matches!(runtime.receive(Duration::ZERO), Ok(None)) {
                self.fail(epoch, "plugin_protocol_error");
            }
        }
    }

    fn fail(&self, epoch: u64, code: &str) {
        if self
            .epoch
            .compare_exchange(epoch, epoch + 1, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let runtime = self.retire_runtime();
        let failure = error(code);
        {
            let mut descriptor = self.descriptor.lock().unwrap_or_else(|e| e.into_inner());
            descriptor.enabled = false;
            descriptor.state = PluginState::Failed;
            descriptor.last_error = Some(PluginError {
                code: failure.code,
                message: failure.message,
            });
        }
        self.changed();
        if let Some(runtime) = runtime {
            runtime.terminate();
        }
    }

    fn request(
        &self,
        runtime: &Transport,
        epoch: u64,
        payload: RequestPayload,
        timeout: Duration,
        cancellation: &Cancellation,
    ) -> Result<ReplyPayload, CallFailure> {
        let catalog = matches!(&payload, RequestPayload::CatalogPage(_));
        let id = self.next_id.fetch_add(1, Ordering::Relaxed).to_string();
        runtime
            .send(&json!(HostMessage::Request(HostRequest {
                v: 1,
                epoch,
                id: id.clone(),
                payload,
                timeout_ms: timeout.as_millis() as u64
            })))
            .map_err(|failure| CallFailure::Runtime(transport_error(failure)))?;
        let deadline = Instant::now() + timeout;
        loop {
            if self.epoch.load(Ordering::Acquire) != epoch {
                return Err(CallFailure::Request(error("plugin_disabled")));
            }
            if cancellation.cancelled() {
                let sent = runtime.send(&json!(HostMessage::Cancel {
                    v: 1,
                    epoch,
                    id: id.clone()
                }));
                if sent.is_err() {
                    return Err(CallFailure::Cancelled { retire: true });
                }
                let cancel_deadline = Instant::now() + Duration::from_millis(500);
                while Instant::now() < cancel_deadline {
                    if self.epoch.load(Ordering::Acquire) != epoch {
                        return Err(CallFailure::Request(error("plugin_disabled")));
                    }
                    match runtime.receive(Duration::from_millis(10)) {
                        Ok(None) => {}
                        Ok(Some(reply)) => {
                            if reply.epoch != epoch || reply.id != id {
                                return Err(CallFailure::Runtime(error("plugin_protocol_error")));
                            }
                            if let Ok(payload) = reply.outcome {
                                if !matches!(
                                    (catalog, payload),
                                    (true, ReplyPayload::CatalogPage(_))
                                        | (false, ReplyPayload::Hello(_))
                                ) {
                                    return Err(CallFailure::Runtime(error(
                                        "plugin_protocol_error",
                                    )));
                                }
                            }
                            return Err(CallFailure::Cancelled { retire: false });
                        }
                        Err(_) => return Err(CallFailure::Cancelled { retire: true }),
                    }
                }
                return Err(CallFailure::Cancelled { retire: true });
            }
            if Instant::now() >= deadline {
                return Err(CallFailure::Runtime(error("plugin_timeout")));
            }
            if let Some(reply) = runtime
                .receive(Duration::from_millis(10))
                .map_err(|failure| CallFailure::Runtime(transport_error(failure)))?
            {
                if reply.epoch != epoch || reply.id != id {
                    return Err(CallFailure::Runtime(error("plugin_protocol_error")));
                }
                return reply.outcome.map_err(|failure| {
                    CallFailure::Request(if failure.code == PluginFailureCode::Cancelled {
                        SourceError::cancelled()
                    } else {
                        error("plugin_request_failed")
                    })
                });
            }
        }
    }

    fn recover_cancelled(&self, epoch: u64) {
        if self.closed.load(Ordering::Acquire)
            || self
                .epoch
                .compare_exchange(epoch, epoch + 1, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
        {
            return;
        }
        {
            let mut descriptor = self.descriptor.lock().unwrap_or_else(|e| e.into_inner());
            if !descriptor.enabled
                || self.closed.load(Ordering::Acquire)
                || self.epoch.load(Ordering::Acquire) != epoch + 1
            {
                return;
            }
            descriptor.state = PluginState::Starting;
            descriptor.last_error = None;
        }
        self.changed();
        let runtime = self.retire_runtime();
        if let Some(runtime) = runtime {
            runtime.terminate();
        }
        if self.closed.load(Ordering::Acquire) || self.epoch.load(Ordering::Acquire) != epoch + 1 {
            return;
        }
        let launch = self
            .launch
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(launch) = launch {
            let _ = self.start_locked(&launch, epoch + 1, &Cancellation::default());
        }
    }
}

impl CatalogSource for ProcessModule {
    fn descriptor(&self) -> PluginDescriptor {
        self.descriptor
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    fn catalog_page(
        &self,
        query: &CatalogQuery,
        cancellation: &Cancellation,
    ) -> Result<CatalogPage, SourceError> {
        query.validate().map_err(|_| error("invalid_params"))?;
        let admission_deadline = Instant::now() + Duration::from_secs(1);
        let _call = loop {
            if let Ok(call) = self.calls.try_lock() {
                break call;
            }
            if cancellation.cancelled() {
                return Err(SourceError::cancelled());
            }
            if Instant::now() >= admission_deadline {
                return Err(error("plugin_busy"));
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        if cancellation.cancelled() {
            return Err(SourceError::cancelled());
        }
        if self.descriptor().state != PluginState::Ready {
            return Err(error("plugin_disabled"));
        }
        let epoch = self.epoch.load(Ordering::Acquire);
        let runtime = self
            .runtime
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or_else(|| error("plugin_disabled"))?;
        let response = self.request(
            &runtime,
            epoch,
            RequestPayload::CatalogPage(query.clone()),
            Duration::from_secs(10),
            cancellation,
        );
        let result = match response {
            Ok(result) => (|| {
                let ReplyPayload::CatalogPage(page) = result else {
                    return Err(error("plugin_protocol_error"));
                };
                if page.items.len() > usize::from(query.limit) {
                    return Err(error("plugin_protocol_error"));
                }
                if self.epoch.load(Ordering::Acquire) != epoch {
                    return Err(error("plugin_disabled"));
                }
                if cancellation.cancelled() {
                    return Err(SourceError::cancelled());
                }
                Ok(page)
            })(),
            Err(CallFailure::Request(failure)) => return Err(failure),
            Err(CallFailure::Cancelled { retire }) => {
                if retire {
                    self.recover_cancelled(epoch);
                }
                return Err(SourceError::cancelled());
            }
            Err(CallFailure::Runtime(failure)) => Err(failure),
        };
        if let Err(failure) = &result {
            if failure.code != "cancelled" && failure.code != "plugin_disabled" {
                self.fail(epoch, &failure.code);
            }
        }
        result
    }
}

fn transport_error(failure: Failure) -> SourceError {
    error(match failure {
        Failure::Spawn => "plugin_start_failed",
        Failure::Protocol => "plugin_protocol_error",
        Failure::Closed => "plugin_crashed",
        Failure::Busy => "plugin_busy",
    })
}
