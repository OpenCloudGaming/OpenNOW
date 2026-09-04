use crate::proxy::client_for_settings_with;
use rand::RngCore as _;
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, CONTENT_LENGTH, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use url::Url;

const CACHE_VERSION: u32 = 1;
const DEFAULT_MAX_BYTES: u64 = 512 * 1024 * 1024;
const DEFAULT_MAX_ENTRIES: usize = 4096;
const DEFAULT_MAX_OBJECT_BYTES: u64 = 8 * 1024 * 1024;
const DEFAULT_WORKERS: usize = 3;
const DEFAULT_QUEUE_CAPACITY: usize = 256;
const MAX_SOURCE_URL_BYTES: usize = 4096;
const FAILURE_RETRY_BASE: Duration = Duration::from_secs(30);
const FAILURE_RETRY_MAX: Duration = Duration::from_secs(30 * 60);
const INDEX_FLUSH_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Clone, Copy)]
struct CacheLimits {
    max_bytes: u64,
    max_entries: usize,
    max_object_bytes: u64,
    workers: usize,
    queue_capacity: usize,
    allow_private_network: bool,
}

impl Default for CacheLimits {
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_MAX_BYTES,
            max_entries: DEFAULT_MAX_ENTRIES,
            max_object_bytes: DEFAULT_MAX_OBJECT_BYTES,
            workers: DEFAULT_WORKERS,
            queue_capacity: DEFAULT_QUEUE_CAPACITY,
            allow_private_network: false,
        }
    }
}

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CacheIndex {
    version: u32,
    entries: BTreeMap<String, CacheEntry>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CacheEntry {
    extension: String,
    size_bytes: u64,
    last_access_ms: u64,
}

struct IndexState {
    index: CacheIndex,
    dirty: bool,
    last_flush: Instant,
}

struct FailureState {
    attempts: u32,
    retry_at: Instant,
}

struct SharedState {
    objects_dir: Option<PathBuf>,
    index_path: Option<PathBuf>,
    limits: CacheLimits,
    index: Mutex<IndexState>,
    queued: Mutex<HashSet<String>>,
    failures: Mutex<HashMap<String, FailureState>>,
    event_tx: mpsc::Sender<Value>,
}

struct DownloadJob {
    key: String,
    source_url: String,
    client: Client,
}

struct CachedObject {
    file_url: String,
    file_path: String,
}

enum ScheduleResult {
    Pending,
    Saturated,
    RetryLater,
    Unavailable,
}

pub struct ArtworkCache {
    base_client: Option<Client>,
    state: Arc<SharedState>,
    sender: Option<mpsc::SyncSender<DownloadJob>>,
    _workers: Vec<JoinHandle<()>>,
}

impl ArtworkCache {
    pub fn new(data_dir: &Path, event_tx: mpsc::Sender<Value>) -> Self {
        Self::with_limits(data_dir, event_tx, CacheLimits::default())
    }

    fn with_limits(data_dir: &Path, event_tx: mpsc::Sender<Value>, limits: CacheLimits) -> Self {
        let base_client = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(12))
            .pool_idle_timeout(Duration::from_secs(30))
            .redirect(artwork_redirect_policy(limits.allow_private_network))
            .build()
            .map_err(|error| {
                eprintln!("artwork cache: HTTP client initialization failed: {error}");
                error
            })
            .ok();
        let root = data_dir.join("artwork-cache").join("v1");
        let objects_dir = root.join("objects");
        let index_path = root.join("index.json");
        let enabled = fs::create_dir_all(&objects_dir)
            .map_err(|error| {
                eprintln!("artwork cache: storage initialization failed: {error}");
                error
            })
            .is_ok();
        let state = Arc::new(SharedState {
            objects_dir: enabled.then_some(objects_dir),
            index_path: enabled.then_some(index_path),
            limits,
            index: Mutex::new(IndexState {
                index: CacheIndex::default(),
                dirty: false,
                last_flush: Instant::now(),
            }),
            queued: Mutex::new(HashSet::new()),
            failures: Mutex::new(HashMap::new()),
            event_tx,
        });
        if enabled {
            state.load_and_reconcile();
        }
        let (sender, receiver) = mpsc::sync_channel(limits.queue_capacity.max(1));
        let receiver = Arc::new(Mutex::new(receiver));
        let mut workers = Vec::new();
        if enabled {
            for index in 0..limits.workers.max(1) {
                let worker_state = Arc::clone(&state);
                let worker_receiver = Arc::clone(&receiver);
                match thread::Builder::new()
                    .name(format!("opennow-artwork-{index}"))
                    .spawn(move || worker_loop(worker_state, worker_receiver))
                {
                    Ok(worker) => workers.push(worker),
                    Err(error) => eprintln!("artwork cache: worker initialization failed: {error}"),
                }
            }
        }
        Self {
            base_client,
            state,
            sender: enabled.then_some(sender),
            _workers: workers,
        }
    }

    pub fn resolve(&self, params: &Value, settings: &Value) -> Result<Value, String> {
        let source_url = validate_source_url(
            params["sourceUrl"]
                .as_str()
                .or_else(|| params["url"].as_str())
                .unwrap_or_default(),
            self.state.limits.allow_private_network,
        )?;
        let key = cache_key(&source_url);
        if let Some(cached) = self.state.lookup(&key) {
            return Ok(json!({
                "version":CACHE_VERSION,
                "status":"ready",
                "sourceUrl":source_url,
                "artworkUrl":cached.file_url,
                "filePath":cached.file_path,
                "cached":true
            }));
        }
        let Some(base_client) = self.base_client.as_ref() else {
            return Ok(fallback(&source_url, "cache-unavailable"));
        };
        let client = client_for_settings_with(base_client, settings, |builder| {
            builder.redirect(artwork_redirect_policy(
                self.state.limits.allow_private_network,
            ))
        })?;
        let status = self.schedule(DownloadJob {
            key,
            source_url: source_url.clone(),
            client,
        });
        Ok(match status {
            ScheduleResult::Pending => json!({
                "version":CACHE_VERSION,
                "status":"pending",
                "sourceUrl":source_url
            }),
            ScheduleResult::Saturated => fallback(&source_url, "queue-full"),
            ScheduleResult::RetryLater => fallback(&source_url, "retry-later"),
            ScheduleResult::Unavailable => fallback(&source_url, "cache-unavailable"),
        })
    }

    fn schedule(&self, job: DownloadJob) -> ScheduleResult {
        let Some(sender) = self.sender.as_ref() else {
            return ScheduleResult::Unavailable;
        };
        {
            let failures = self
                .state
                .failures
                .lock()
                .expect("artwork failures poisoned");
            if failures
                .get(&job.key)
                .is_some_and(|failure| failure.retry_at > Instant::now())
            {
                return ScheduleResult::RetryLater;
            }
        }
        {
            let mut queued = self.state.queued.lock().expect("artwork queue poisoned");
            if !queued.insert(job.key.clone()) {
                return ScheduleResult::Pending;
            }
        }
        let key = job.key.clone();
        match sender.try_send(job) {
            Ok(()) => ScheduleResult::Pending,
            Err(mpsc::TrySendError::Full(_)) => {
                self.state
                    .queued
                    .lock()
                    .expect("artwork queue poisoned")
                    .remove(&key);
                ScheduleResult::Saturated
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                self.state
                    .queued
                    .lock()
                    .expect("artwork queue poisoned")
                    .remove(&key);
                ScheduleResult::Unavailable
            }
        }
    }
}

impl Drop for ArtworkCache {
    fn drop(&mut self) {
        self.sender.take();
        self.state.flush_index();
    }
}

impl SharedState {
    fn load_and_reconcile(&self) {
        let Some(objects_dir) = self.objects_dir.as_ref() else {
            return;
        };
        let mut index_state = self.index.lock().expect("artwork index poisoned");
        index_state.index = self
            .index_path
            .as_ref()
            .and_then(|path| load_index(path))
            .unwrap_or_default();
        index_state.index.version = CACHE_VERSION;
        let mut discovered = BTreeMap::new();
        if let Ok(files) = fs::read_dir(objects_dir) {
            for file in files.flatten() {
                let path = file.path();
                let Some((key, extension)) = parse_object_name(&file.file_name().to_string_lossy())
                else {
                    let _ = fs::remove_file(path);
                    continue;
                };
                let Ok(metadata) = file.metadata() else {
                    continue;
                };
                if !metadata.is_file() || metadata.len() > self.limits.max_object_bytes {
                    let _ = fs::remove_file(path);
                    continue;
                }
                let previous = index_state.index.entries.get(&key);
                discovered.insert(
                    key,
                    CacheEntry {
                        extension,
                        size_bytes: metadata.len(),
                        last_access_ms: previous
                            .map(|entry| entry.last_access_ms)
                            .unwrap_or_else(|| modified_ms(&metadata)),
                    },
                );
            }
        }
        index_state.index.entries = discovered;
        prune_to_limits(self, &mut index_state.index, 0, 0);
        index_state.dirty = true;
        self.persist_locked(&mut index_state);
    }

    fn lookup(&self, key: &str) -> Option<CachedObject> {
        let objects_dir = self.objects_dir.as_ref()?;
        let mut index_state = self.index.lock().expect("artwork index poisoned");
        let entry = index_state.index.entries.get(key)?.clone();
        let path = objects_dir.join(object_name(key, &entry.extension));
        let metadata = fs::metadata(&path).ok();
        if !metadata.as_ref().is_some_and(|value| {
            value.is_file()
                && value.len() == entry.size_bytes
                && value.len() <= self.limits.max_object_bytes
        }) {
            index_state.index.entries.remove(key);
            index_state.dirty = true;
            return None;
        }
        if let Some(entry) = index_state.index.entries.get_mut(key) {
            entry.last_access_ms = now_ms();
        }
        index_state.dirty = true;
        if index_state.last_flush.elapsed() >= INDEX_FLUSH_INTERVAL {
            self.persist_locked(&mut index_state);
        }
        Some(CachedObject {
            file_url: Url::from_file_path(&path).ok()?.to_string(),
            file_path: path.to_string_lossy().into_owned(),
        })
    }

    fn store_download(
        &self,
        key: &str,
        extension: &str,
        temporary: &Path,
        size_bytes: u64,
    ) -> io::Result<CachedObject> {
        let objects_dir = self
            .objects_dir
            .as_ref()
            .ok_or_else(|| io::Error::other("cache unavailable"))?;
        let mut index_state = self.index.lock().expect("artwork index poisoned");
        if !prune_to_limits(self, &mut index_state.index, size_bytes, 1) {
            return Err(io::Error::other("cache limit could not be enforced"));
        }
        let path = objects_dir.join(object_name(key, extension));
        fs::rename(temporary, &path)?;
        index_state.index.entries.insert(
            key.to_owned(),
            CacheEntry {
                extension: extension.to_owned(),
                size_bytes,
                last_access_ms: now_ms(),
            },
        );
        index_state.dirty = true;
        self.persist_locked(&mut index_state);
        Ok(CachedObject {
            file_url: Url::from_file_path(&path)
                .map_err(|_| io::Error::other("cache path is not a file URL"))?
                .to_string(),
            file_path: path.to_string_lossy().into_owned(),
        })
    }

    fn persist_locked(&self, index_state: &mut IndexState) {
        if !index_state.dirty {
            return;
        }
        let Some(path) = self.index_path.as_ref() else {
            return;
        };
        match write_index_atomic(path, &index_state.index) {
            Ok(()) => {
                index_state.dirty = false;
                index_state.last_flush = Instant::now();
            }
            Err(error) => eprintln!("artwork cache: index write failed: {error}"),
        }
    }

    fn flush_index(&self) {
        let mut index_state = self.index.lock().expect("artwork index poisoned");
        self.persist_locked(&mut index_state);
    }

    fn record_failure(&self, key: &str) {
        let mut failures = self.failures.lock().expect("artwork failures poisoned");
        let attempts = failures
            .get(key)
            .map(|failure| failure.attempts.saturating_add(1))
            .unwrap_or(1);
        let factor = 1_u32 << attempts.saturating_sub(1).min(6);
        let delay = FAILURE_RETRY_BASE
            .checked_mul(factor)
            .unwrap_or(FAILURE_RETRY_MAX)
            .min(FAILURE_RETRY_MAX);
        failures.insert(
            key.to_owned(),
            FailureState {
                attempts,
                retry_at: Instant::now() + delay,
            },
        );
    }
}

fn worker_loop(state: Arc<SharedState>, receiver: Arc<Mutex<mpsc::Receiver<DownloadJob>>>) {
    loop {
        let job = receiver.lock().expect("artwork receiver poisoned").recv();
        let Ok(job) = job else {
            break;
        };
        let result = download(&state, &job);
        let payload = match result {
            Ok(cached) => {
                state
                    .failures
                    .lock()
                    .expect("artwork failures poisoned")
                    .remove(&job.key);
                json!({
                    "version":CACHE_VERSION,
                    "sourceUrl":job.source_url,
                    "artworkUrl":cached.file_url,
                    "filePath":cached.file_path,
                    "cached":true
                })
            }
            Err(reason) => {
                state.record_failure(&job.key);
                json!({
                    "version":CACHE_VERSION,
                    "sourceUrl":job.source_url,
                    "artworkUrl":job.source_url,
                    "cached":false,
                    "fallbackReason":reason
                })
            }
        };
        state
            .queued
            .lock()
            .expect("artwork queue poisoned")
            .remove(&job.key);
        let _ = state
            .event_tx
            .send(json!({"type":"event", "name":"artwork.ready", "payload":payload}));
    }
}

fn download(state: &SharedState, job: &DownloadJob) -> Result<CachedObject, &'static str> {
    let objects_dir = state.objects_dir.as_ref().ok_or("cache-unavailable")?;
    let response = job
        .client
        .get(&job.source_url)
        .header(
            ACCEPT,
            "image/avif,image/webp,image/png,image/jpeg,image/gif;q=0.9,*/*;q=0.1",
        )
        .send()
        .map_err(|_| "network-error")?;
    if !response.status().is_success() {
        return Err("http-error");
    }
    if validate_response_url(response.url(), state.limits.allow_private_network).is_err() {
        return Err("unsafe-redirect");
    }
    if response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|size| size > state.limits.max_object_bytes)
    {
        return Err("too-large");
    }
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_ascii_lowercase());
    if content_type.as_deref().is_some_and(|value| {
        !value.starts_with("image/") && !value.starts_with("application/octet-stream")
    }) {
        return Err("unsupported-content");
    }
    let temporary = temporary_path(objects_dir, &job.key);
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|_| "storage-error")?;
    let mut reader = response;
    let mut buffer = [0_u8; 32 * 1024];
    let mut prefix = Vec::with_capacity(16);
    let mut total = 0_u64;
    let write_result = (|| {
        loop {
            let read = reader.read(&mut buffer).map_err(|_| "network-error")?;
            if read == 0 {
                break;
            }
            total = total.saturating_add(read as u64);
            if total > state.limits.max_object_bytes {
                return Err("too-large");
            }
            if prefix.len() < 16 {
                let needed = 16 - prefix.len();
                prefix.extend_from_slice(&buffer[..read.min(needed)]);
            }
            file.write_all(&buffer[..read])
                .map_err(|_| "storage-error")?;
        }
        if total == 0 {
            return Err("empty-response");
        }
        file.sync_all().map_err(|_| "storage-error")?;
        Ok(())
    })();
    if let Err(error) = write_result {
        drop(file);
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    drop(file);
    let extension = sniff_extension(&prefix).ok_or_else(|| {
        let _ = fs::remove_file(&temporary);
        "unsupported-content"
    })?;
    let result = state
        .store_download(&job.key, extension, &temporary, total)
        .map_err(|_| "storage-error");
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn validate_source_url(raw: &str, allow_private_network: bool) -> Result<String, String> {
    let source_url = raw.trim();
    if source_url.is_empty() || source_url.len() > MAX_SOURCE_URL_BYTES {
        return Err("Artwork sourceUrl must be a non-empty HTTP(S) URL".to_owned());
    }
    let parsed = Url::parse(source_url)
        .map_err(|_| "Artwork sourceUrl must be a valid HTTP(S) URL".to_owned())?;
    validate_response_url(&parsed, allow_private_network)?;
    Ok(source_url.to_owned())
}

fn artwork_redirect_policy(allow_private_network: bool) -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= 5 {
            return attempt.error("too many artwork redirects");
        }
        if validate_response_url(attempt.url(), allow_private_network).is_err() {
            return attempt.error("unsafe artwork redirect");
        }
        attempt.follow()
    })
}

fn validate_response_url(url: &Url, allow_private_network: bool) -> Result<(), String> {
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Artwork URLs must use HTTP(S) without embedded credentials".to_owned());
    }
    if !allow_private_network && !public_artwork_host(url) {
        return Err("Artwork URLs must not target a local or private network".to_owned());
    }
    Ok(())
}

fn public_artwork_host(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Ipv4(address)) => public_ipv4(address),
        Some(url::Host::Ipv6(address)) => public_ipv6(address),
        Some(url::Host::Domain(domain)) => {
            let domain = domain.trim_end_matches('.').to_ascii_lowercase();
            domain != "localhost" && !domain.ends_with(".localhost") && !domain.ends_with(".local")
        }
        None => false,
    }
}

fn public_ipv4(address: Ipv4Addr) -> bool {
    let octets = address.octets();
    !address.is_private()
        && !address.is_loopback()
        && !address.is_link_local()
        && !address.is_broadcast()
        && !address.is_documentation()
        && !address.is_unspecified()
        && !address.is_multicast()
        && octets[0] != 0
        && !(octets[0] == 100 && (64..=127).contains(&octets[1]))
        && !(octets[0] == 198 && (octets[1] == 18 || octets[1] == 19))
}

fn public_ipv6(address: Ipv6Addr) -> bool {
    if let Some(mapped) = address.to_ipv4_mapped() {
        return public_ipv4(mapped);
    }
    let segments = address.segments();
    !(address.is_loopback()
        || address.is_unspecified()
        || address.is_multicast()
        || address.is_unique_local()
        || address.is_unicast_link_local()
        || (segments[0] == 0x2001 && segments[1] == 0x0db8))
}

fn fallback(source_url: &str, reason: &str) -> Value {
    json!({
        "version":CACHE_VERSION,
        "status":"fallback",
        "sourceUrl":source_url,
        "artworkUrl":source_url,
        "cached":false,
        "fallbackReason":reason
    })
}

fn cache_key(source_url: &str) -> String {
    format!("{:x}", Sha256::digest(source_url.as_bytes()))
}

fn object_name(key: &str, extension: &str) -> String {
    format!("{key}.{extension}")
}

fn parse_object_name(name: &str) -> Option<(String, String)> {
    let (key, extension) = name.rsplit_once('.')?;
    if key.len() != 64
        || !key.bytes().all(|byte| byte.is_ascii_hexdigit())
        || !matches!(extension, "jpg" | "png" | "webp" | "gif" | "avif")
    {
        return None;
    }
    Some((key.to_owned(), extension.to_owned()))
}

fn temporary_path(objects_dir: &Path, key: &str) -> PathBuf {
    let mut nonce = [0_u8; 8];
    rand::rng().fill_bytes(&mut nonce);
    let nonce = nonce
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    objects_dir.join(format!(".{key}-{nonce}.part"))
}

fn sniff_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("jpg")
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else if bytes.len() >= 12
        && &bytes[4..8] == b"ftyp"
        && matches!(&bytes[8..12], b"avif" | b"avis")
    {
        Some("avif")
    } else {
        None
    }
}

fn load_index(path: &Path) -> Option<CacheIndex> {
    [path.to_path_buf(), path.with_extension("json.backup")]
        .into_iter()
        .find_map(|candidate| {
            fs::read(&candidate)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<CacheIndex>(&bytes).ok())
                .filter(|index| index.version == CACHE_VERSION)
        })
}

fn write_index_atomic(path: &Path, index: &CacheIndex) -> io::Result<()> {
    let bytes = serde_json::to_vec(index).map_err(io::Error::other)?;
    let temporary = path.with_extension("json.tmp");
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    match fs::rename(&temporary, path) {
        Ok(()) => Ok(()),
        Err(_first_error) if path.exists() => {
            let backup = path.with_extension("json.backup");
            let _ = fs::remove_file(&backup);
            fs::rename(path, &backup)?;
            if let Err(error) = fs::rename(&temporary, path) {
                let _ = fs::rename(&backup, path);
                return Err(error);
            }
            let _ = fs::remove_file(backup);
            Ok(())
        }
        Err(error) => Err(error),
    }
}

fn prune_to_limits(
    state: &SharedState,
    index: &mut CacheIndex,
    incoming_bytes: u64,
    incoming_entries: usize,
) -> bool {
    let mut total_bytes = index
        .entries
        .values()
        .map(|entry| entry.size_bytes)
        .sum::<u64>();
    let mut entries = index.entries.len();
    let mut oldest = index
        .entries
        .iter()
        .map(|(key, entry)| (entry.last_access_ms, key.clone()))
        .collect::<Vec<_>>();
    oldest.sort();
    for (_, key) in oldest {
        if total_bytes.saturating_add(incoming_bytes) <= state.limits.max_bytes
            && entries.saturating_add(incoming_entries) <= state.limits.max_entries
        {
            break;
        }
        let Some(entry) = index.entries.get(&key).cloned() else {
            continue;
        };
        let Some(objects_dir) = state.objects_dir.as_ref() else {
            return false;
        };
        let path = objects_dir.join(object_name(&key, &entry.extension));
        if fs::remove_file(&path).is_ok() || !path.exists() {
            index.entries.remove(&key);
            total_bytes = total_bytes.saturating_sub(entry.size_bytes);
            entries = entries.saturating_sub(1);
        }
    }
    total_bytes.saturating_add(incoming_bytes) <= state.limits.max_bytes
        && entries.saturating_add(incoming_entries) <= state.limits.max_entries
}

fn modified_ms(metadata: &fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_millis() as u64)
        .unwrap_or_else(now_ms)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn temporary_directory(name: &str) -> PathBuf {
        let mut nonce = [0_u8; 8];
        rand::rng().fill_bytes(&mut nonce);
        let suffix = nonce
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let path = std::env::temp_dir().join(format!("opennow-artwork-{name}-{suffix}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn test_limits(max_bytes: u64, max_object_bytes: u64) -> CacheLimits {
        CacheLimits {
            max_bytes,
            max_entries: 16,
            max_object_bytes,
            workers: 1,
            queue_capacity: 4,
            allow_private_network: true,
        }
    }

    fn serve_once(body: Vec<u8>, content_type: &str) -> (String, Arc<AtomicUsize>) {
        serve_once_after(body, content_type, Duration::ZERO)
    }

    fn serve_once_after(
        body: Vec<u8>,
        content_type: &str,
        delay: Duration,
    ) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let server_count = Arc::clone(&count);
        let content_type = content_type.to_owned();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request);
            server_count.fetch_add(1, Ordering::SeqCst);
            thread::sleep(delay);
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(&body).unwrap();
        });
        (
            format!("http://{address}/artwork/../image.jpg?width=900"),
            count,
        )
    }

    fn jpeg(size: usize) -> Vec<u8> {
        let mut bytes = vec![0_u8; size.max(4)];
        bytes[..4].copy_from_slice(&[0xff, 0xd8, 0xff, 0xd9]);
        bytes
    }

    fn wait_for_event(receiver: &mpsc::Receiver<Value>) -> Value {
        receiver.recv_timeout(Duration::from_secs(3)).unwrap()
    }

    #[test]
    fn only_safe_http_urls_are_accepted_and_paths_are_hashed() {
        for value in [
            "file:///tmp/image.jpg",
            "data:image/png;base64,AAAA",
            "https://user:secret@example.com/image.jpg",
            "../image.jpg",
        ] {
            assert!(
                validate_source_url(value, false).is_err(),
                "accepted {value}"
            );
        }
        for value in [
            "http://127.0.0.1/image.jpg",
            "http://[::1]/image.jpg",
            "http://[::ffff:127.0.0.1]/image.jpg",
            "http://[::ffff:10.0.0.1]/image.jpg",
            "http://[2001:db8::1]/image.jpg",
            "http://169.254.169.254/latest/meta-data",
            "https://artwork.local/image.jpg",
        ] {
            assert!(
                validate_source_url(value, false).is_err(),
                "accepted {value}"
            );
        }
        let source = "https://img.nvidiagrid.net/../../escape.jpg;f=webp;w=900";
        assert_eq!(validate_source_url(source, false).unwrap(), source);
        let key = cache_key(source);
        assert_eq!(key.len(), 64);
        assert!(!object_name(&key, "webp").contains(".."));
    }

    #[test]
    fn resolve_does_not_wait_for_the_network_worker() {
        let root = temporary_directory("nonblocking");
        let (source_url, _) = serve_once_after(jpeg(64), "image/jpeg", Duration::from_millis(300));
        let (event_tx, event_rx) = mpsc::channel();
        let cache = ArtworkCache::with_limits(&root, event_tx, test_limits(1024, 128));
        let started = Instant::now();
        let result = cache
            .resolve(&json!({"sourceUrl":source_url}), &json!({}))
            .unwrap();
        assert_eq!(result["status"], "pending");
        assert!(started.elapsed() < Duration::from_millis(100));
        assert_eq!(wait_for_event(&event_rx)["payload"]["cached"], true);
        drop(cache);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn miss_returns_pending_then_persists_an_atomic_file() {
        let root = temporary_directory("persistent");
        let (source_url, requests) = serve_once(jpeg(128), "image/jpeg");
        let (event_tx, event_rx) = mpsc::channel();
        {
            let cache = ArtworkCache::with_limits(&root, event_tx.clone(), test_limits(1024, 512));
            let result = cache
                .resolve(&json!({"sourceUrl":source_url}), &json!({}))
                .unwrap();
            assert_eq!(result["status"], "pending");
            assert!(result.get("artworkUrl").is_none());
            let event = wait_for_event(&event_rx);
            assert_eq!(event["name"], "artwork.ready");
            assert_eq!(event["payload"]["cached"], true);
            assert!(
                event["payload"]["artworkUrl"]
                    .as_str()
                    .unwrap()
                    .starts_with("file:")
            );
        }
        let cache = ArtworkCache::with_limits(&root, event_tx, test_limits(1024, 512));
        let hit = cache
            .resolve(&json!({"sourceUrl":source_url}), &json!({}))
            .unwrap();
        assert_eq!(hit["status"], "ready");
        assert_eq!(hit["cached"], true);
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        let objects = fs::read_dir(root.join("artwork-cache/v1/objects"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(objects.len(), 1);
        assert!(objects[0].ends_with(".jpg"));
        assert!(!objects[0].contains(".part"));
        drop(cache);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn oversized_download_emits_safe_remote_fallback() {
        let root = temporary_directory("oversized");
        let (source_url, _) = serve_once(jpeg(256), "image/jpeg");
        let (event_tx, event_rx) = mpsc::channel();
        let cache = ArtworkCache::with_limits(&root, event_tx, test_limits(1024, 64));
        let result = cache
            .resolve(&json!({"sourceUrl":source_url}), &json!({}))
            .unwrap();
        assert_eq!(result["status"], "pending");
        let event = wait_for_event(&event_rx);
        assert_eq!(event["payload"]["cached"], false);
        assert_eq!(event["payload"]["artworkUrl"], source_url);
        assert_eq!(event["payload"]["fallbackReason"], "too-large");
        assert!(
            fs::read_dir(root.join("artwork-cache/v1/objects"))
                .unwrap()
                .next()
                .is_none()
        );
        drop(cache);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn pruning_removes_the_least_recently_resolved_entry() {
        let root = temporary_directory("pruning");
        let (first_url, _) = serve_once(jpeg(80), "image/jpeg");
        let (event_tx, event_rx) = mpsc::channel();
        let cache = ArtworkCache::with_limits(&root, event_tx, test_limits(120, 100));
        cache
            .resolve(&json!({"sourceUrl":first_url}), &json!({}))
            .unwrap();
        wait_for_event(&event_rx);
        thread::sleep(Duration::from_millis(2));
        let (second_url, _) = serve_once(jpeg(80), "image/jpeg");
        cache
            .resolve(&json!({"sourceUrl":second_url}), &json!({}))
            .unwrap();
        wait_for_event(&event_rx);
        assert!(cache.state.lookup(&cache_key(&first_url)).is_none());
        assert!(cache.state.lookup(&cache_key(&second_url)).is_some());
        let index = cache.state.index.lock().unwrap();
        assert_eq!(index.index.entries.len(), 1);
        assert!(
            index
                .index
                .entries
                .values()
                .map(|entry| entry.size_bytes)
                .sum::<u64>()
                <= 120
        );
        drop(index);
        drop(cache);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn duplicate_resolves_share_one_background_download() {
        let root = temporary_directory("dedupe");
        let (source_url, requests) = serve_once(jpeg(64), "image/jpeg");
        let (event_tx, event_rx) = mpsc::channel();
        let cache = ArtworkCache::with_limits(&root, event_tx, test_limits(1024, 128));
        let first = cache
            .resolve(&json!({"sourceUrl":source_url}), &json!({}))
            .unwrap();
        let second = cache
            .resolve(&json!({"sourceUrl":source_url}), &json!({}))
            .unwrap();
        assert_eq!(first["status"], "pending");
        assert_eq!(second["status"], "pending");
        wait_for_event(&event_rx);
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        assert!(event_rx.try_recv().is_err());
        drop(cache);
        let _ = fs::remove_dir_all(root);
    }
}
