//! Bounded, account/provider-scoped Store responses persisted across core restarts.
//! Only successful protocol-sized results are cached; credentials never enter files.
use super::store_catalog_page::RESULT_BUDGET;
use crate::service_error::ServiceError;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, Weak};

const MAX_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 512;
const PAGE_TTL_MS: u64 = 15 * 60 * 1000;

type FetchLocks = HashMap<(PathBuf, u64), Weak<Mutex<()>>>;

pub struct CachePolicy {
    pub ttl_ms: u64,
    pub allow_stale: bool,
}

pub struct StoreCache {
    root: PathBuf,
    index: Mutex<Option<(String, crate::sources::gfn::store_index::StoreIndex, u64)>>,
    // IO is serialized, but network work never holds the lock. Invalidation
    // advances the epoch so an older in-flight fetch cannot refill cleared data.
    epoch: Mutex<u64>,
    fetches: Mutex<FetchLocks>,
    pub requests: crate::sources::gfn::store_requests::StoreRequests,
}

fn digest(value: &Value) -> String {
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}

impl StoreCache {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            root: data_dir.join("store-cache-v2"),
            index: Mutex::new(None),
            epoch: Mutex::new(0),
            fetches: Mutex::new(HashMap::new()),
            requests: crate::sources::gfn::store_requests::StoreRequests::default(),
        }
    }

    pub fn load_or_fetch(
        &self,
        scope: &Value,
        key: &Value,
        refresh: bool,
        fetch: impl FnOnce() -> Result<Value, ServiceError>,
    ) -> Result<Value, ServiceError> {
        self.load_with_policy(
            scope,
            key,
            refresh,
            CachePolicy {
                ttl_ms: PAGE_TTL_MS,
                allow_stale: false,
            },
            fetch,
        )
    }

    pub fn catalog_revision(&self) -> u64 {
        let path = self.root.join("revision");
        let file = match File::open(&path) {
            Ok(file) => file,
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && fs::symlink_metadata(&path)
                        .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
            {
                return 0;
            }
            Err(_) => return now_ms(),
        };
        let mut bytes = Vec::new();
        if file.take(32).read_to_end(&mut bytes).is_err() {
            return now_ms();
        }
        serde_json::from_slice::<u64>(&bytes)
            .ok()
            .filter(|revision| *revision < (1_u64 << 52))
            .unwrap_or_else(now_ms)
    }

    pub fn invalidate(&self, revision: u64) -> Result<(), ServiceError> {
        let mut epoch = self.epoch.lock().expect("Store cache poisoned");
        *epoch = epoch.wrapping_add(1);
        *self.index.lock().expect("Store index poisoned") = None;
        self.write(
            &self.root.join("revision"),
            &serde_json::json!(revision.max(self.catalog_revision())),
        )
        .map_err(|_| ServiceError {
            code: "catalog_cache_invalidation_failed",
            message: "The catalog cache revision could not be saved. Retry the refresh.".into(),
        })
    }

    pub fn invalidate_key(&self, scope: &Value, key: &Value) {
        let mut epoch = self.epoch.lock().expect("Store cache poisoned");
        *epoch = epoch.wrapping_add(1);
        let _ = fs::remove_file(
            self.root
                .join(format!("{}-{}.json", digest(scope), digest(key))),
        );
        *self.index.lock().expect("Store index poisoned") = None;
    }

    pub fn load_with_policy(
        &self,
        scope: &Value,
        key: &Value,
        refresh: bool,
        policy: CachePolicy,
        fetch: impl FnOnce() -> Result<Value, ServiceError>,
    ) -> Result<Value, ServiceError> {
        crate::requests::check()?;
        let CachePolicy {
            ttl_ms,
            allow_stale,
        } = policy;
        let prefix = format!("{}-", digest(scope));
        let path = self.root.join(format!("{prefix}{}.json", digest(key)));
        let prior_fetch = if refresh && allow_stale {
            self.read(&path).map(|value| value["fetchedAt"].clone())
        } else {
            None
        };
        let epoch = {
            let mut epoch = self.epoch.lock().expect("Store cache poisoned");
            if refresh && !allow_stale {
                *self.index.lock().expect("Store index poisoned") = None;
                *epoch = epoch.wrapping_add(1);
                // Only this cache's generated, flat response files are targets.
                if let Ok(entries) = fs::read_dir(&self.root) {
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().into_owned();
                        if name.starts_with(&prefix) && name.ends_with(".json") {
                            let _ = fs::remove_file(entry.path());
                        }
                    }
                }
            } else if !refresh && let Some(mut value) = self.read_fresh(&path) {
                value["cacheHit"] = Value::Bool(true);
                return Ok(value);
            }
            *epoch
        };
        let fetch_lock = {
            let mut fetches = self.fetches.lock().expect("Store fetches poisoned");
            fetches.retain(|_, lock| lock.strong_count() > 0);
            let entry = fetches.entry((path.clone(), epoch)).or_default();
            match entry.upgrade() {
                Some(lock) => lock,
                None => {
                    let lock = Arc::new(Mutex::new(()));
                    *entry = Arc::downgrade(&lock);
                    lock
                }
            }
        };
        let _fetch = crate::sources::gfn::store_requests::lock(&fetch_lock)?;
        if let Some(mut value) = self.read_fresh(&path)
            && (!refresh || (allow_stale && prior_fetch.as_ref() != Some(&value["fetchedAt"])))
        {
            value["cacheHit"] = Value::Bool(true);
            return Ok(value);
        }
        let mut value = match fetch() {
            Ok(value) => value,
            Err(error)
                if allow_stale
                    && !matches!(
                        error.code,
                        "cancelled" | "stale_account" | "http_unauthorized"
                    ) =>
            {
                let mut value = self.read(&path).unwrap_or_else(|| serde_json::json!({}));
                value["status"] = serde_json::json!(if value["fetchedAt"].is_number() {
                    "stale"
                } else {
                    "error"
                });
                value["freshness"] = value["status"].clone();
                value["error"] = serde_json::json!({"code":error.code,"message":error.message});
                return Ok(value);
            }
            Err(error) => return Err(error),
        };
        crate::requests::check()?;
        value["cacheHit"] = Value::Bool(false);
        let fetched_at = now_ms();
        value["fetchedAt"] = serde_json::json!(fetched_at);
        value["expiresAt"] = serde_json::json!(fetched_at.saturating_add(ttl_ms));
        value["freshness"] = serde_json::json!("fresh");
        value["status"] = serde_json::json!("success");
        value["error"] = Value::Null;
        let value = crate::sources::gfn::store_catalog_page::bounded_result(value)?;
        let current = self.epoch.lock().expect("Store cache poisoned");
        if epoch == *current {
            // Cache failures must not turn a successful catalog fetch into an
            // unavailable Store (read-only disk, low space, interrupted writes).
            if self.write(&path, &value).is_err() {
                eprintln!("store cache: response could not be persisted");
            }
            *self.index.lock().expect("Store index poisoned") = None;
        }
        Ok(value)
    }

    pub fn local_query(&self, scope: &Value, params: &Value) -> Result<Value, ServiceError> {
        crate::requests::check()?;
        let scope_key = digest(scope);
        let read_key = |key: &Value| {
            self.read_fresh(&self.root.join(format!("{scope_key}-{}.json", digest(key))))
        };
        let mut cached = self.index.lock().expect("Store index poisoned");
        if params["refresh"] == true
            || cached
                .as_ref()
                .is_none_or(|(key, _, expires)| key != &scope_key || *expires <= now_ms())
        {
            let mut index = crate::sources::gfn::store_index::StoreIndex::default();
            let mut expires = u64::MAX;
            let mut cursor = String::new();
            let mut seen = std::collections::HashSet::new();
            for _ in 0..100 {
                crate::requests::check()?;
                let key = serde_json::json!(["page", 100, cursor, ""]);
                let Some(page) = read_key(&key) else {
                    break;
                };
                expires = expires.min(page["expiresAt"].as_u64().unwrap_or(0));
                for (at, game) in page["games"].as_array().into_iter().flatten().enumerate() {
                    index.add(game, key.clone(), format!("/games/{at}"), None);
                }
                index.pages += 1;
                if page["hasNextPage"] == false {
                    index.complete = true;
                    break;
                }
                cursor = page["nextCursor"].as_str().unwrap_or("").to_owned();
                index.next_upstream = cursor.clone();
                if cursor.is_empty() || !seen.insert(cursor.clone()) {
                    break;
                }
            }
            let key = serde_json::json!(["presentation", "panels"]);
            if let Some(panels) = read_key(&key) {
                expires = expires.min(panels["expiresAt"].as_u64().unwrap_or(0));
                index.add_panels(&panels, &key);
            }
            *cached = Some((scope_key.clone(), index, expires));
        }
        let (_, index, _) = cached.as_ref().expect("Store index initialized");
        if index.pages == 0 {
            return Err(ServiceError {
                code: "store_cache_missing",
                message: "No saved Store catalog yet. Load a catalog page first.".into(),
            });
        }
        index.query(params, read_key)
    }

    fn read(&self, path: &PathBuf) -> Option<Value> {
        let file = File::open(path).ok()?;
        if !file.metadata().ok()?.is_file() {
            return None;
        }
        let mut bytes = Vec::new();
        file.take(RESULT_BUDGET as u64 + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() > RESULT_BUDGET {
            return None;
        }
        let value: Value = serde_json::from_slice(&bytes).ok()?;
        // Versioned directory + bounded structural validation before crossing IPC.
        let page = value["games"].is_array()
            && value["hasNextPage"].is_boolean()
            && value["nextCursor"].is_string();
        let section = matches!(
            value["section"].as_str(),
            Some("panels" | "marquee" | "filters")
        ) && value["items"].is_array();
        let metadata = value["metadataKind"].is_string();
        (page || section || metadata).then_some(value)
    }

    fn read_fresh(&self, path: &PathBuf) -> Option<Value> {
        let value = self.read(path)?;
        (value["expiresAt"].as_u64()? > now_ms()).then_some(value)
    }

    fn write(&self, path: &PathBuf, value: &Value) -> std::io::Result<()> {
        fs::create_dir_all(&self.root)?;
        let bytes = serde_json::to_vec(value)?;
        let temp = path.with_extension(format!(
            "{}-{}.tmp",
            std::process::id(),
            rand::random::<u64>()
        ));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temp, path)?;
            self.prune();
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }

    fn prune(&self) {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return;
        };
        let mut files: Vec<_> = entries
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !(name.ends_with(".json") || name.ends_with(".tmp")) {
                    return None;
                }
                let meta = entry.metadata().ok()?;
                meta.is_file()
                    .then_some((entry.path(), meta.len(), meta.modified().ok()))
            })
            .collect();
        files.sort_by_key(|entry| entry.2);
        let mut bytes: u64 = files.iter().map(|entry| entry.1).sum();
        let mut count = files.len();
        for (path, size, _) in files {
            if count <= MAX_ENTRIES && bytes <= MAX_BYTES {
                break;
            }
            if fs::remove_file(path).is_ok() {
                count -= 1;
                bytes = bytes.saturating_sub(size);
            }
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn cache() -> StoreCache {
        StoreCache::new(
            std::env::temp_dir().join(format!("opennow-store-test-{}", rand::random::<u64>())),
        )
    }
    fn page(id: &str) -> Value {
        json!({"games":[{"id":id}],"hasNextPage":false,"nextCursor":""})
    }
    #[test]
    fn restart_reads_disk_without_calling_the_fetcher() {
        let cache = cache();
        let scope = json!(["account-a", "provider-a", "direct", "en_US"]);
        let key = json!(["page", 100, "", ""]);
        assert_eq!(
            cache
                .load_or_fetch(&scope, &key, false, || Ok(page("saved")))
                .unwrap()["cacheHit"],
            false
        );
        let restarted = StoreCache::new(cache.root.parent().unwrap().to_path_buf());
        let hit = restarted
            .load_or_fetch(&scope, &key, false, || panic!("network called on restart"))
            .unwrap();
        assert_eq!(hit["games"][0]["id"], "saved");
        assert_eq!(hit["cacheHit"], true);
        fs::remove_dir_all(cache.root.parent().unwrap()).unwrap();
    }

    #[test]
    fn expired_disk_pages_require_network_and_metadata_refreshes_coalesce() {
        let cache = cache();
        let scope = json!("scope");
        let key = json!("page");
        cache
            .load_or_fetch(&scope, &key, false, || Ok(page("old")))
            .unwrap();
        let path = cache
            .root
            .join(format!("{}-{}.json", digest(&scope), digest(&key)));
        let mut expired = cache.read(&path).unwrap();
        expired["expiresAt"] = json!(0);
        cache.write(&path, &expired).unwrap();
        let refreshed = cache
            .load_or_fetch(&scope, &key, false, || Ok(page("new")))
            .unwrap();
        assert_eq!(refreshed["cacheHit"], false);
        assert_eq!(refreshed["games"][0]["id"], "new");
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let ready = std::sync::Barrier::new(4);
        std::thread::scope(|threads| {
            let mut workers = Vec::new();
            for _ in 0..4 {
                workers.push(threads.spawn(|| {
                    ready.wait();
                    cache
                        .load_with_policy(
                            &scope,
                            &key,
                            true,
                            CachePolicy {
                                ttl_ms: 1000,
                                allow_stale: true,
                            },
                            || {
                                calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                                std::thread::sleep(std::time::Duration::from_millis(100));
                                Ok(page("coalesced"))
                            },
                        )
                        .unwrap()
                }));
            }
            for worker in workers {
                assert_eq!(worker.join().unwrap()["games"][0]["id"], "coalesced");
            }
        });
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        fs::remove_dir_all(cache.root.parent().unwrap()).unwrap();
    }

    #[test]
    fn catalog_invalidation_survives_restart_and_out_of_order_writers() {
        let cache = cache();
        cache.invalidate(2).unwrap();
        cache.invalidate(1).unwrap();
        let restarted = StoreCache::new(cache.root.parent().unwrap().to_path_buf());
        assert_eq!(restarted.catalog_revision(), 2);
        fs::write(cache.root.join("revision"), b"corrupt").unwrap();
        assert!(restarted.catalog_revision() > 2);
        fs::remove_dir_all(cache.root.parent().unwrap()).unwrap();
    }

    #[test]
    fn absent_revision_starts_in_the_initial_namespace() {
        let cache = cache();
        assert_eq!(cache.catalog_revision(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_revision_cannot_reuse_pre_sync_pages_or_erase_reference_metadata() {
        let cache = cache();
        let page_key = json!(["page", 100, "", ""]);
        cache
            .load_or_fetch(&json!(["account", 0]), &page_key, false, || {
                Ok(page("pre-sync"))
            })
            .unwrap();
        let reference_scope = json!(["definitions", "account"]);
        let reference_key = json!(["genres"]);
        cache
            .load_or_fetch(&reference_scope, &reference_key, false, || {
                Ok(json!({"metadataKind":"genres","items":[{"genre":"ACTION","label":"Action"}]}))
            })
            .unwrap();
        cache.invalidate(1).unwrap();
        let revision_path = cache.root.join("revision");
        fs::remove_file(&revision_path).unwrap();
        std::os::unix::fs::symlink("revision", &revision_path).unwrap();
        assert!(File::open(&revision_path).is_err());

        let restarted = StoreCache::new(cache.root.parent().unwrap().to_path_buf());
        let revision = restarted.catalog_revision();
        assert_ne!(revision, 0);
        let result = restarted
            .load_or_fetch(&json!(["account", revision]), &page_key, false, || {
                Ok(page("fresh-network"))
            })
            .unwrap();
        assert_eq!(result["games"][0]["id"], "fresh-network");
        assert_eq!(result["cacheHit"], false);
        let reference = restarted
            .load_or_fetch(&reference_scope, &reference_key, false, || {
                panic!("reference metadata was invalidated")
            })
            .unwrap();
        assert_eq!(reference["items"][0]["genre"], "ACTION");
        fs::remove_dir_all(cache.root.parent().unwrap()).unwrap();
    }

    #[test]
    fn simultaneous_cache_misses_fetch_each_page_only_once() {
        let cache = cache();
        let ready = std::sync::Barrier::new(4);
        let calls = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|threads| {
            let mut workers = Vec::new();
            for _ in 0..4 {
                workers.push(threads.spawn(|| {
                    ready.wait();
                    cache
                        .load_or_fetch(&json!("a"), &json!("first"), false, || {
                            calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            std::thread::sleep(std::time::Duration::from_millis(100));
                            Ok(page("shared"))
                        })
                        .unwrap()
                }));
            }
            let results: Vec<_> = workers
                .into_iter()
                .map(|worker| worker.join().unwrap())
                .collect();
            assert_eq!(
                results
                    .iter()
                    .filter(|value| value["cacheHit"] == false)
                    .count(),
                1
            );
            assert!(
                results
                    .iter()
                    .all(|value| value["games"][0]["id"] == "shared")
            );
        });
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        fs::remove_dir_all(cache.root.parent().unwrap()).unwrap();
    }

    #[test]
    fn failed_fetches_leave_saved_pages_available() {
        let cache = cache();
        cache
            .load_or_fetch(&json!("a"), &json!("saved"), false, || Ok(page("saved")))
            .unwrap();
        assert!(
            cache
                .load_or_fetch(&json!("a"), &json!("missing"), false, || {
                    Err(ServiceError {
                        code: "rate_limited",
                        message: "Try later".into(),
                    })
                })
                .is_err()
        );
        let result = cache
            .load_or_fetch(&json!("a"), &json!("saved"), false, || {
                panic!("cached page fetched again")
            })
            .unwrap();
        assert_eq!(result["cacheHit"], true);
        fs::remove_dir_all(cache.root.parent().unwrap()).unwrap();
    }
    #[test]
    fn accounts_providers_queries_and_cursors_do_not_share_results() {
        let cache = cache();
        for scope in [
            json!(["a", "nvidia"]),
            json!(["b", "nvidia"]),
            json!(["a", "alliance"]),
        ] {
            for key in [json!(["", ""]), json!(["next", ""]), json!(["", "search"])] {
                let result = cache
                    .load_or_fetch(&scope, &key, false, || Ok(page("new")))
                    .unwrap();
                assert_eq!(result["cacheHit"], false);
            }
        }
        fs::remove_dir_all(cache.root.parent().unwrap()).unwrap();
    }
    #[test]
    fn refresh_invalidates_all_pages_and_chrome_only_for_this_account() {
        let cache = cache();
        for scope in [json!("a"), json!("b")] {
            for key in [json!("first"), json!("next"), json!("chrome")] {
                cache
                    .load_or_fetch(&scope, &key, false, || Ok(page("old")))
                    .unwrap();
            }
        }
        cache
            .load_or_fetch(&json!("a"), &json!("first"), true, || Ok(page("fresh")))
            .unwrap();
        for key in [json!("next"), json!("chrome")] {
            assert_eq!(
                cache
                    .load_or_fetch(&json!("a"), &key, false, || Ok(page("new")))
                    .unwrap()["cacheHit"],
                false
            );
            assert_eq!(
                cache
                    .load_or_fetch(&json!("b"), &key, false, || panic!())
                    .unwrap()["cacheHit"],
                true
            );
        }
        fs::remove_dir_all(cache.root.parent().unwrap()).unwrap();
    }
    #[test]
    fn failures_corruption_and_oversized_files_are_cache_misses() {
        let cache = cache();
        let scope = json!("a");
        let key = json!("first");
        assert!(
            cache
                .load_or_fetch(&scope, &key, false, || Err(ServiceError {
                    code: "network_error",
                    message: "offline".into()
                }))
                .is_err()
        );
        cache
            .load_or_fetch(&scope, &key, false, || Ok(page("new")))
            .unwrap();
        let path = cache
            .root
            .join(format!("{}-{}.json", digest(&scope), digest(&key)));
        for bytes in [b"broken json".to_vec(), vec![b' '; RESULT_BUDGET + 1]] {
            fs::write(&path, bytes).unwrap();
            assert_eq!(
                cache
                    .load_or_fetch(&scope, &key, false, || Ok(page("recovered")))
                    .unwrap()["cacheHit"],
                false
            );
        }
        fs::remove_dir_all(cache.root.parent().unwrap()).unwrap();
    }
    #[test]
    fn obsolete_fetch_cannot_repopulate_after_refresh() {
        let cache = cache();
        cache
            .load_or_fetch(&json!("a"), &json!("old"), false, || {
                cache.load_or_fetch(&json!("a"), &json!("first"), true, || Ok(page("fresh")))?;
                Ok(page("obsolete"))
            })
            .unwrap();
        assert_eq!(
            cache
                .load_or_fetch(&json!("a"), &json!("old"), false, || Ok(page("new")))
                .unwrap()["cacheHit"],
            false
        );
        fs::remove_dir_all(cache.root.parent().unwrap()).unwrap();
    }
    #[test]
    fn disk_entry_count_is_bounded() {
        let cache = cache();
        fs::create_dir_all(&cache.root).unwrap();
        for index in 0..MAX_ENTRIES + 3 {
            fs::write(cache.root.join(format!("{index}.json")), b"{}").unwrap();
        }
        cache.prune();
        assert_eq!(fs::read_dir(&cache.root).unwrap().count(), MAX_ENTRIES);
        fs::remove_dir_all(cache.root.parent().unwrap()).unwrap();
    }
}
