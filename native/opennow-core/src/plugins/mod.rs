mod package;
mod process;
mod transport;

use crate::requests::Cancellation;
use crate::sources::contract::{CatalogSource, SourceError};
use opennow_plugin_api::manifest::PluginManifest;
use opennow_plugin_api::{PluginDescriptor, PluginId, PluginSnapshot, PluginState, PluginTrust};
use process::ProcessModule;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MAX_INSTALLED: usize = 32;
const MAX_RUNNING: usize = 4;
const MAX_INSPECTIONS: usize = 4;
const REGISTRY_LIMIT: u64 = 4 * 1024 * 1024;
const MAX_GENERATION: u64 = i32::MAX as u64;

struct Changes {
    generation: AtomicU64,
    persisted: AtomicU64,
    exhausted: AtomicBool,
    output: Sender<Value>,
}

impl Changes {
    fn bump(&self) {
        let generation =
            match self
                .generation
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                    value.checked_add(1).filter(|next| *next <= MAX_GENERATION)
                }) {
                Ok(previous) => previous + 1,
                Err(value) => {
                    self.exhausted.store(true, Ordering::Release);
                    value
                }
            };
        let _ = self.output.send(
            json!({"type":"event","name":"plugins.changed","payload":{"generation":generation}}),
        );
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    manifest: PluginManifest,
    package_sha256: String,
    enabled: bool,
    consent_version: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Registry {
    version: u32,
    generation: u64,
    plugins: Vec<Record>,
}

struct Entry {
    record: Record,
    module: Arc<ProcessModule>,
}

struct Inspection {
    staged: package::StagedPackage,
    generation: u64,
    expires_at: u64,
}

struct Inner {
    root: PathBuf,
    entries: Mutex<BTreeMap<String, Entry>>,
    inspections: Mutex<BTreeMap<String, Inspection>>,
    mutation: Mutex<()>,
    persistence: Mutex<()>,
    changes: Arc<Changes>,
    closing: AtomicBool,
    init_error: Option<SourceError>,
}

pub struct PluginManager {
    inner: Arc<Inner>,
}

impl PluginManager {
    pub fn open(data_dir: &Path, output: Sender<Value>) -> Result<Self, SourceError> {
        match Self::open_checked(data_dir, output.clone()) {
            Ok(manager) => Ok(manager),
            Err(_) => Ok(Self {
                inner: Arc::new(Inner {
                    root: data_dir.join("plugins"),
                    entries: Mutex::new(BTreeMap::new()),
                    inspections: Mutex::new(BTreeMap::new()),
                    mutation: Mutex::new(()),
                    persistence: Mutex::new(()),
                    changes: Arc::new(Changes {
                        generation: AtomicU64::new(1),
                        persisted: AtomicU64::new(1),
                        exhausted: AtomicBool::new(false),
                        output,
                    }),
                    closing: AtomicBool::new(false),
                    init_error: Some(error("plugin_storage_error")),
                }),
            }),
        }
    }

    pub fn ensure_available(&self) -> Result<(), SourceError> {
        if let Some(failure) = &self.inner.init_error {
            return Err(failure.clone());
        }
        if self.inner.changes.exhausted.load(Ordering::Acquire) {
            self.shutdown();
            return Err(error("plugin_storage_error"));
        }
        if self.inner.closing.load(Ordering::Acquire) {
            return Err(error("plugins_unavailable"));
        }
        Ok(())
    }

    fn open_checked(data_dir: &Path, output: Sender<Value>) -> Result<Self, SourceError> {
        let root = data_dir.join("plugins");
        private_dir(&root)?;
        for directory in ["installed", "data", "staging"] {
            private_dir(&root.join(directory))?;
        }
        for entry in
            fs::read_dir(root.join("staging")).map_err(|_| error("plugin_storage_error"))?
        {
            remove_entry(&entry.map_err(|_| error("plugin_storage_error"))?.path())?;
        }
        let registry = match File::open(root.join("registry.json")) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(REGISTRY_LIMIT + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|_| error("plugin_storage_error"))?;
                if bytes.len() as u64 > REGISTRY_LIMIT {
                    return Err(error("plugin_storage_error"));
                }
                let registry: Registry =
                    serde_json::from_slice(&bytes).map_err(|_| error("plugin_storage_error"))?;
                if registry.version != 1
                    || registry.plugins.len() > MAX_INSTALLED
                    || registry.generation == 0
                    || registry.generation >= MAX_GENERATION
                {
                    return Err(error("plugin_storage_error"));
                }
                registry
            }
            Err(failure) if failure.kind() == std::io::ErrorKind::NotFound => Registry {
                version: 1,
                generation: 1,
                plugins: vec![],
            },
            Err(_) => return Err(error("plugin_storage_error")),
        };
        let changes = Arc::new(Changes {
            generation: AtomicU64::new(registry.generation.max(1)),
            persisted: AtomicU64::new(registry.generation.max(1)),
            exhausted: AtomicBool::new(false),
            output,
        });
        let mut entries = BTreeMap::new();
        let mut restore = Vec::new();
        for record in registry.plugins {
            if record.consent_version != 1 || !valid_digest(&record.package_sha256) {
                return Err(error("plugin_storage_error"));
            }
            let id = record.manifest.id.to_string();
            if entries.contains_key(&id) {
                return Err(error("plugin_storage_error"));
            }
            if record.enabled {
                restore.push(id.clone());
            }
            let mut initial = descriptor(&record.manifest);
            if record.enabled {
                initial.enabled = true;
                initial.state = PluginState::Starting;
            }
            let module = Arc::new(ProcessModule::new(initial, Arc::clone(&changes)));
            entries.insert(id, Entry { record, module });
        }
        if restore.len() > MAX_RUNNING {
            return Err(error("plugin_storage_error"));
        }
        let inner = Arc::new(Inner {
            root,
            entries: Mutex::new(entries),
            inspections: Mutex::new(BTreeMap::new()),
            mutation: Mutex::new(()),
            persistence: Mutex::new(()),
            changes,
            closing: AtomicBool::new(false),
            init_error: None,
        });
        let weak = Arc::downgrade(&inner);
        thread::Builder::new()
            .name("plugin-monitor".into())
            .spawn(move || {
                let Some(inner) = weak.upgrade() else {
                    return;
                };
                for id in restore.into_iter().take(MAX_RUNNING) {
                    if inner.closing.load(Ordering::Acquire) {
                        return;
                    }
                    if let Ok(_mutation) = inner.mutation.lock() {
                        let _ = inner.enable(&id, &Cancellation::default());
                    }
                }
                drop(inner);
                loop {
                    thread::sleep(Duration::from_millis(100));
                    let Some(inner) = weak.upgrade() else {
                        return;
                    };
                    if inner.closing.load(Ordering::Acquire) {
                        return;
                    }
                    let modules: Vec<_> = inner
                        .entries
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .values()
                        .map(|entry| Arc::clone(&entry.module))
                        .collect();
                    for module in modules {
                        module.check_health();
                    }
                    let generation = inner.changes.generation.load(Ordering::Acquire);
                    if generation != inner.changes.persisted.load(Ordering::Acquire) {
                        if let Ok(_mutation) = inner.mutation.try_lock() {
                            let _ = inner.save();
                        }
                    }
                }
            })
            .map_err(|_| error("plugin_start_failed"))?;
        Ok(Self { inner })
    }

    pub fn snapshot(&self) -> PluginSnapshot {
        let plugins = self
            .inner
            .entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .map(|entry| entry.module.descriptor())
            .collect();
        PluginSnapshot {
            generation: self.inner.changes.generation.load(Ordering::Acquire),
            plugins,
        }
    }

    pub fn source(&self, id: &PluginId) -> Result<Arc<dyn CatalogSource>, SourceError> {
        self.ensure_available()?;
        if self.inner.closing.load(Ordering::Acquire) {
            return Err(error("plugins_unavailable"));
        }
        self.inner
            .entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id.as_str())
            .map(|entry| Arc::clone(&entry.module) as Arc<dyn CatalogSource>)
            .ok_or_else(|| error("plugin_not_found"))
    }

    pub fn dispatch(
        &self,
        method: &str,
        params: &Value,
        cancellation: &Cancellation,
    ) -> Result<Value, SourceError> {
        self.ensure_available()?;
        if cancellation.cancelled() {
            return Err(SourceError::cancelled());
        }
        if self.inner.closing.load(Ordering::Acquire) {
            return Err(error("plugins_unavailable"));
        }
        if method == "plugins.list" {
            return Ok(json!(self.snapshot()));
        }
        let admission_deadline = Instant::now() + Duration::from_millis(100);
        let _mutation = loop {
            if let Ok(guard) = self.inner.mutation.try_lock() {
                break guard;
            }
            if cancellation.cancelled() {
                return Err(SourceError::cancelled());
            }
            if Instant::now() >= admission_deadline {
                return Err(error("plugin_busy"));
            }
            thread::sleep(Duration::from_millis(5));
        };
        self.expire_inspections();
        let result = match method {
            "plugins.install.inspect" => self.inspect(params, cancellation),
            "plugins.install.cancel" => {
                let request: CancelInspection = parse(params)?;
                if let Some(inspection) = self
                    .inner
                    .inspections
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&request.token)
                {
                    remove_entry(&inspection.staged.root)?;
                }
                Ok(json!({"cancelled":true}))
            }
            "plugins.install.commit" => {
                let request: Commit = parse(params)?;
                self.expected(request.expected_generation)?;
                if !request.consent {
                    return Err(error("consent_required"));
                }
                let inspection = self
                    .inner
                    .inspections
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&request.token)
                    .ok_or_else(|| error("inspection_expired"))?;
                let result = self.commit(&inspection, cancellation);
                if result.is_err() {
                    let _ = remove_entry(&inspection.staged.root);
                }
                result
            }
            "plugins.setEnabled" => {
                let request: SetEnabled = parse(params)?;
                self.expected(request.expected_generation)?;
                reject_builtin(&request.id)?;
                if request.enabled {
                    self.inner.enable(request.id.as_str(), cancellation)?;
                } else {
                    let module = self.module(&request.id)?;
                    module.disable();
                }
                if let Err(failure) = self.inner.save() {
                    self.module(&request.id)?.disable();
                    return Err(failure);
                }
                Ok(json!(self.snapshot()))
            }
            "plugins.uninstall" => {
                let request: Uninstall = parse(params)?;
                self.expected(request.expected_generation)?;
                reject_builtin(&request.id)?;
                if !request.confirmed {
                    return Err(error("consent_required"));
                }
                let module = self.module(&request.id)?;
                if module.descriptor().enabled
                    || module.busy()
                    || module.descriptor().state == PluginState::Starting
                {
                    return Err(error("plugin_in_use"));
                }
                module.disable();
                remove_entry(&self.inner.root.join("data").join(request.id.as_str()))?;
                remove_entry(&self.inner.root.join("installed").join(request.id.as_str()))?;
                let removed = self
                    .inner
                    .entries
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(request.id.as_str());
                if let Err(failure) = self.inner.save() {
                    if let Some(entry) = removed {
                        self.inner
                            .entries
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .insert(request.id.to_string(), entry);
                    }
                    return Err(failure);
                }
                self.inner.changes.bump();
                Ok(json!(self.snapshot()))
            }
            _ => Err(error("unsupported_capability")),
        };
        self.ensure_available()?;
        result
    }

    fn module(&self, id: &PluginId) -> Result<Arc<ProcessModule>, SourceError> {
        self.inner
            .entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id.as_str())
            .map(|entry| Arc::clone(&entry.module))
            .ok_or_else(|| error("plugin_not_found"))
    }

    fn expected(&self, generation: u64) -> Result<(), SourceError> {
        if generation != self.inner.changes.generation.load(Ordering::Acquire) {
            return Err(error("stale_registry"));
        }
        Ok(())
    }

    fn inspect(&self, params: &Value, cancellation: &Cancellation) -> Result<Value, SourceError> {
        let request: Inspect = parse(params)?;
        if self
            .inner
            .inspections
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
            >= MAX_INSPECTIONS
        {
            return Err(error("plugin_busy"));
        }
        let source = if request.path.starts_with("file:") {
            url::Url::parse(&request.path)
                .ok()
                .and_then(|url| url.to_file_path().ok())
                .ok_or_else(|| error("invalid_plugin_package"))?
        } else {
            PathBuf::from(request.path)
        };
        if !source.is_absolute()
            || source.extension().and_then(|value| value.to_str()) != Some("opennow-plugin")
        {
            return Err(error("invalid_plugin_package"));
        }
        let mut bytes = [0u8; 24];
        rand::rng().fill_bytes(&mut bytes);
        let token: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        let root = self.inner.root.join("staging").join(&token);
        private_dir(&root)?;
        let staged = match package::inspect(&source, &root) {
            Ok(staged) => staged,
            Err(failure) => {
                let _ = remove_entry(&root);
                return Err(SourceError::new(failure.code, failure.message));
            }
        };
        if cancellation.cancelled() {
            let _ = remove_entry(&root);
            return Err(SourceError::cancelled());
        }
        if self
            .inner
            .entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(staged.manifest.id.as_str())
        {
            let _ = remove_entry(&root);
            return Err(error("plugin_already_installed"));
        }
        let generation = self.inner.changes.generation.load(Ordering::Acquire);
        let expires_at = now_ms() + 300_000;
        let result = json!({"generation":generation,"inspection":{"token":token,"expiresAt":expires_at,
            "plugin":descriptor(&staged.manifest),"packageSha256":staged.package_sha256}});
        self.inner
            .inspections
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(
                token,
                Inspection {
                    staged,
                    generation,
                    expires_at,
                },
            );
        Ok(result)
    }

    fn commit(
        &self,
        inspection: &Inspection,
        cancellation: &Cancellation,
    ) -> Result<Value, SourceError> {
        self.expected(inspection.generation)?;
        if inspection.expires_at <= now_ms() {
            return Err(error("inspection_expired"));
        }
        let staged = &inspection.staged;
        package::verify(&staged.root, &staged.manifest).map_err(|_| error("package_changed"))?;
        if cancellation.cancelled() {
            return Err(SourceError::cancelled());
        }
        let mut entries = self.inner.entries.lock().unwrap_or_else(|e| e.into_inner());
        if entries.contains_key(staged.manifest.id.as_str()) {
            return Err(error("plugin_already_installed"));
        }
        if entries.len() >= MAX_INSTALLED {
            return Err(error("plugin_busy"));
        }
        let parent = self
            .inner
            .root
            .join("installed")
            .join(staged.manifest.id.as_str());
        private_dir(&parent)?;
        let destination = parent.join(&staged.package_sha256);
        remove_entry(&destination)?;
        remove_entry(
            &self
                .inner
                .root
                .join("data")
                .join(staged.manifest.id.as_str()),
        )?;
        fs::rename(&staged.root, &destination).map_err(|_| error("plugin_storage_error"))?;
        let record = Record {
            manifest: staged.manifest.clone(),
            package_sha256: staged.package_sha256.clone(),
            enabled: false,
            consent_version: 1,
        };
        let module = Arc::new(ProcessModule::new(
            descriptor(&record.manifest),
            Arc::clone(&self.inner.changes),
        ));
        entries.insert(record.manifest.id.to_string(), Entry { record, module });
        drop(entries);
        if let Err(failure) = self.inner.save() {
            self.inner
                .entries
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(staged.manifest.id.as_str());
            let _ = remove_entry(&destination);
            return Err(failure);
        }
        self.inner.changes.bump();
        Ok(json!(self.snapshot()))
    }

    fn expire_inspections(&self) {
        self.inner
            .inspections
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|_, inspection| {
                if inspection.expires_at > now_ms() {
                    return true;
                }
                let _ = remove_entry(&inspection.staged.root);
                false
            });
    }

    pub fn shutdown(&self) {
        if self.inner.closing.swap(true, Ordering::AcqRel) {
            return;
        }
        if self.inner.init_error.is_some() {
            return;
        }
        let modules: Vec<_> = self
            .inner
            .entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .map(|entry| Arc::clone(&entry.module))
            .collect();
        let runtimes: Vec<_> = modules
            .into_iter()
            .filter_map(|module| module.shutdown())
            .collect();
        let deadline = Instant::now() + Duration::from_millis(500);
        for runtime in runtimes {
            runtime.reap_until(deadline);
        }
        let _ = self.inner.save();
    }
}

impl Drop for PluginManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Inner {
    fn enable(&self, id: &str, cancellation: &Cancellation) -> Result<(), SourceError> {
        let (module, record) = {
            let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            if entries
                .values()
                .filter(|entry| {
                    entry.record.manifest.id.as_str() != id
                        && matches!(
                            entry.module.descriptor().state,
                            PluginState::Ready | PluginState::Starting
                        )
                })
                .count()
                >= MAX_RUNNING
            {
                return Err(error("plugin_busy"));
            }
            let entry = entries.get(id).ok_or_else(|| error("plugin_not_found"))?;
            (Arc::clone(&entry.module), entry.record.clone())
        };
        let root = self
            .root
            .join("installed")
            .join(id)
            .join(&record.package_sha256);
        let data = self.root.join("data").join(id);
        module.enable(&root, &record.manifest, &data, cancellation)
    }

    fn save(&self) -> Result<(), SourceError> {
        let _persistence = self.persistence.lock().unwrap_or_else(|e| e.into_inner());
        let plugins = self
            .entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .map(|entry| {
                let mut record = entry.record.clone();
                record.enabled = entry.module.descriptor().enabled;
                record
            })
            .collect();
        let registry = Registry {
            version: 1,
            generation: self.changes.generation.load(Ordering::Acquire),
            plugins,
        };
        let bytes = serde_json::to_vec(&registry).map_err(|_| error("plugin_storage_error"))?;
        let temporary = self
            .root
            .join(format!("registry-{:016x}.next", rand::rng().next_u64()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .map_err(|_| error("plugin_storage_error"))?;
        if file
            .write_all(&bytes)
            .and_then(|_| file.sync_all())
            .is_err()
        {
            drop(file);
            let _ = fs::remove_file(&temporary);
            return Err(error("plugin_storage_error"));
        }
        drop(file);
        if fs::rename(&temporary, self.root.join("registry.json")).is_err() {
            let _ = fs::remove_file(&temporary);
            return Err(error("plugin_storage_error"));
        }
        self.changes
            .persisted
            .store(registry.generation, Ordering::Release);
        Ok(())
    }
}

fn descriptor(manifest: &PluginManifest) -> PluginDescriptor {
    PluginDescriptor {
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        publisher: manifest.publisher.clone(),
        description: manifest.description.clone(),
        builtin: false,
        required: false,
        enabled: false,
        state: PluginState::Disabled,
        capabilities: manifest.capabilities.clone(),
        trust: PluginTrust::UnsignedNative,
        last_error: None,
    }
}

fn private_dir(path: &Path) -> Result<(), SourceError> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(error("plugin_storage_error"));
        }
    }
    fs::create_dir_all(path).map_err(|_| error("plugin_storage_error"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| error("plugin_storage_error"))?;
    }
    Ok(())
}

fn remove_entry(path: &Path) -> Result<(), SourceError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            fs::remove_dir_all(path).map_err(|_| error("plugin_storage_error"))
        }
        Ok(_) => fs::remove_file(path).map_err(|_| error("plugin_storage_error")),
        Err(failure) if failure.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(error("plugin_storage_error")),
    }
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn reject_builtin(id: &PluginId) -> Result<(), SourceError> {
    if id.is_builtin() {
        Err(error("required_plugin"))
    } else {
        Ok(())
    }
}
fn parse<T: for<'de> Deserialize<'de>>(value: &Value) -> Result<T, SourceError> {
    serde_json::from_value(value.clone()).map_err(|_| error("invalid_params"))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Inspect {
    path: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CancelInspection {
    token: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Commit {
    token: String,
    expected_generation: u64,
    consent: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SetEnabled {
    id: PluginId,
    enabled: bool,
    expected_generation: u64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Uninstall {
    id: PluginId,
    expected_generation: u64,
    confirmed: bool,
}

fn error(code: &str) -> SourceError {
    let (code, message) = match code {
        "invalid_params" => ("invalid_params", "Plugin request parameters are invalid"),
        "plugins_unavailable" => ("plugins_unavailable", "Plugin services are unavailable"),
        "invalid_plugin_package" => ("invalid_plugin_package", "The plugin package is invalid"),
        "incompatible_plugin" => (
            "incompatible_plugin",
            "The plugin is not compatible with this host",
        ),
        "unsupported_capability" => (
            "unsupported_capability",
            "This plugin capability is not supported",
        ),
        "plugin_already_installed" => (
            "plugin_already_installed",
            "This plugin is already installed; uninstall it first",
        ),
        "plugin_not_found" => ("plugin_not_found", "The plugin was not found"),
        "plugin_disabled" => ("plugin_disabled", "The plugin is not enabled"),
        "plugin_in_use" => (
            "plugin_in_use",
            "Disable the plugin and finish its requests before removing it",
        ),
        "required_plugin" => (
            "required_plugin",
            "The built-in plugin is required and cannot be removed",
        ),
        "stale_registry" => (
            "stale_registry",
            "Plugin state changed; refresh and try again",
        ),
        "consent_required" => ("consent_required", "Explicit confirmation is required"),
        "inspection_expired" => (
            "inspection_expired",
            "The package inspection expired; choose the file again",
        ),
        "package_changed" => (
            "package_changed",
            "Installed plugin files no longer match the inspected package",
        ),
        "plugin_busy" => (
            "plugin_busy",
            "The plugin host is busy; try again after the current request",
        ),
        "plugin_start_failed" => ("plugin_start_failed", "The plugin process could not start"),
        "plugin_protocol_error" => (
            "plugin_protocol_error",
            "The plugin sent an invalid protocol message",
        ),
        "plugin_timeout" => (
            "plugin_timeout",
            "The plugin did not respond before its deadline",
        ),
        "plugin_crashed" => ("plugin_crashed", "The plugin process stopped unexpectedly"),
        "plugin_request_failed" => (
            "plugin_request_failed",
            "The plugin could not complete the request",
        ),
        "cancelled" => ("cancelled", "Request cancelled"),
        _ => (
            "plugin_storage_error",
            "Plugin storage could not be updated",
        ),
    };
    SourceError::new(code, message)
}
