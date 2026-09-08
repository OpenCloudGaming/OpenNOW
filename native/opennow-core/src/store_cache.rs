//! Bounded, account/provider-scoped Store responses persisted across core restarts.
//! Only successful protocol-sized results are cached; credentials never enter files.
use crate::{gfn::ServiceError, store_catalog_page::RESULT_BUDGET};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, Weak};

const MAX_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 512;

type FetchLocks = HashMap<(PathBuf, u64), Weak<Mutex<()>>>;

pub struct StoreCache {
    root: PathBuf,
    index: Mutex<Option<(String, crate::store_index::StoreIndex)>>,
    // IO is serialized, but network work never holds the lock. Invalidation
    // advances the epoch so an older in-flight fetch cannot refill cleared data.
    epoch: Mutex<u64>,
    fetches: Mutex<FetchLocks>,
    pub requests: crate::store_requests::StoreRequests,
}

fn digest(value: &Value) -> String {
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}

impl StoreCache {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            root: data_dir.join("store-cache-v1"),
            index: Mutex::new(None),
            epoch: Mutex::new(0),
            fetches: Mutex::new(HashMap::new()),
            requests: crate::store_requests::StoreRequests::default(),
        }
    }

    pub fn load_or_fetch(
        &self,
        scope: &Value,
        key: &Value,
        refresh: bool,
        fetch: impl FnOnce() -> Result<Value, ServiceError>,
    ) -> Result<Value, ServiceError> {
        crate::requests::check()?;
        let prefix = format!("{}-", digest(scope));
        let path = self.root.join(format!("{prefix}{}.json", digest(key)));
        let epoch = {
            let mut epoch = self.epoch.lock().expect("Store cache poisoned");
            if refresh {
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
            } else if let Some(mut value) = self.read(&path) {
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
        let _fetch = crate::store_requests::lock(&fetch_lock)?;
        if let Some(mut value) = self.read(&path) {
            value["cacheHit"] = Value::Bool(true);
            return Ok(value);
        }
        let mut value = fetch()?;
        crate::requests::check()?;
        value["cacheHit"] = Value::Bool(false);
        let value = crate::store_catalog_page::bounded_result(value)?;
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
        let read_key =
            |key: &Value| self.read(&self.root.join(format!("{scope_key}-{}.json", digest(key))));
        let mut cached = self.index.lock().expect("Store index poisoned");
        if params["refresh"] == true || cached.as_ref().is_none_or(|(key, _)| key != &scope_key) {
            let mut index = crate::store_index::StoreIndex::default();
            let mut cursor = String::new();
            let mut seen = std::collections::HashSet::new();
            for _ in 0..100 {
                crate::requests::check()?;
                let key = serde_json::json!(["page", 100, cursor, ""]);
                let Some(page) = read_key(&key) else {
                    break;
                };
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
                index.add_panels(&panels, &key);
            }
            *cached = Some((scope_key.clone(), index));
        }
        let (_, index) = cached.as_ref().expect("Store index initialized");
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
        (page || section).then_some(value)
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
