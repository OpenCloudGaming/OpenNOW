use crate::service_error::ServiceError;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const FRESHNESS: Duration = Duration::from_secs(5 * 60);
const FAILURE_BACKOFF: Duration = Duration::from_secs(30);

struct Entry {
    result: Result<String, ServiceError>,
    expires: Instant,
}

type ScopeEntry = Arc<Mutex<Option<Entry>>>;

#[derive(Default)]
pub struct ServerVpcCache(Mutex<HashMap<[u8; 32], ScopeEntry>>);

impl ServerVpcCache {
    pub fn resolve(
        &self,
        provider: &str,
        account: &str,
        token: &str,
        fetch: impl FnOnce() -> Result<Option<String>, ServiceError>,
    ) -> Result<String, ServiceError> {
        let scope = Sha256::digest(json!([provider, account, token]).to_string().as_bytes()).into();
        let slot = {
            let mut entries = crate::sources::gfn::store_requests::lock(&self.0)?;
            if !entries.contains_key(&scope) && entries.len() >= 16 {
                entries.retain(|_, entry| Arc::strong_count(entry) > 1);
                if entries.len() >= 16 {
                    return Err(ServiceError {
                        code: "routing_busy",
                        message: "Too many provider lookups are in progress".into(),
                    });
                }
            }
            entries.entry(scope).or_default().clone()
        };
        let mut cached = crate::sources::gfn::store_requests::lock(&slot)?;
        if let Some(entry) = cached
            .as_ref()
            .filter(|entry| entry.expires > Instant::now())
        {
            return entry.result.clone();
        }
        let result = fetch().and_then(|value| {
            value
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| ServiceError {
                    code: "invalid_upstream_response",
                    message: "Server info did not include a verified VPC".into(),
                })
        });
        crate::requests::check()?;
        if result.as_ref().is_err_and(|error| {
            matches!(
                error.code,
                "http_unauthorized" | "rate_limited" | "cancelled" | "stale_account"
            )
        }) {
            return result;
        }
        let freshness = if result.is_ok() {
            FRESHNESS
        } else {
            FAILURE_BACKOFF
        };
        *cached = Some(Entry {
            result: result.clone(),
            expires: Instant::now() + freshness,
        });
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc, Barrier,
        atomic::{AtomicUsize, Ordering},
    };

    fn only_entry(cache: &ServerVpcCache) -> Arc<Mutex<Option<Entry>>> {
        cache.0.lock().unwrap().values().next().unwrap().clone()
    }

    #[test]
    fn unrelated_provider_lookup_does_not_wait_for_an_inflight_scope() {
        let cache = ServerVpcCache::default();
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        std::thread::scope(|threads| {
            let first = threads.spawn(|| {
                cache.resolve("provider-a", "account-a", "token-a", move || {
                    entered_tx.send(()).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    Ok(Some("vpc-a".into()))
                })
            });
            entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            assert_eq!(
                cache
                    .resolve("provider-b", "account-b", "token-b", || Ok(Some(
                        "vpc-b".into()
                    )))
                    .unwrap(),
                "vpc-b"
            );
            release_tx.send(()).unwrap();
            assert_eq!(first.join().unwrap().unwrap(), "vpc-a");
        });
    }

    #[test]
    fn concurrent_lookups_share_one_fetch_and_reuse_it_for_five_minutes() {
        let cache = ServerVpcCache::default();
        let ready = Barrier::new(4);
        let calls = AtomicUsize::new(0);
        let started = Instant::now();
        std::thread::scope(|threads| {
            for _ in 0..4 {
                let cache = &cache;
                let ready = &ready;
                let calls = &calls;
                threads.spawn(move || {
                    ready.wait();
                    let result = cache
                        .resolve("provider", "account", "token", || {
                            calls.fetch_add(1, Ordering::SeqCst);
                            std::thread::sleep(Duration::from_millis(100));
                            Ok(Some("region-a".into()))
                        })
                        .unwrap();
                    assert_eq!(result, "region-a");
                });
            }
        });
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            cache
                .resolve("provider", "account", "token", || panic!(
                    "fresh metadata refetched"
                ))
                .unwrap(),
            "region-a"
        );
        let slot = only_entry(&cache);
        let mut cached = slot.lock().unwrap();
        let entry = cached.as_mut().unwrap();
        assert!(entry.expires >= started + FRESHNESS);
        assert!(entry.expires <= Instant::now() + FRESHNESS);
        entry.expires = Instant::now() - Duration::from_secs(1);
        drop(cached);
        assert_eq!(
            cache
                .resolve("provider", "account", "token", || Ok(Some(
                    "region-b".into()
                )))
                .unwrap(),
            "region-b"
        );
    }

    #[test]
    fn providers_accounts_and_refreshed_tokens_do_not_share_metadata() {
        let cache = ServerVpcCache::default();
        for (index, (provider, account, token)) in [
            ("provider-a", "account-a", "token-a"),
            ("provider-b", "account-a", "token-a"),
            ("provider-b", "account-b", "token-a"),
            ("provider-b", "account-b", "token-b"),
        ]
        .into_iter()
        .enumerate()
        {
            let expected = format!("region-{index}");
            assert_eq!(
                cache
                    .resolve(provider, account, token, || Ok(Some(expected.clone())))
                    .unwrap(),
                expected
            );
        }
    }

    #[test]
    fn failures_preserve_known_metadata_with_a_short_retry_backoff() {
        let cache = ServerVpcCache::default();
        assert_eq!(
            cache.resolve("p", "a", "t", || Ok(None)).unwrap_err().code,
            "invalid_upstream_response"
        );
        assert!(
            only_entry(&cache)
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .expires
                .duration_since(Instant::now())
                <= FAILURE_BACKOFF
        );
        cache
            .resolve("p", "a", "t", || {
                panic!("failed lookup retried immediately")
            })
            .unwrap_err();
        only_entry(&cache).lock().unwrap().as_mut().unwrap().expires =
            Instant::now() - Duration::from_secs(1);
        assert_eq!(
            cache
                .resolve("p", "a", "t", || Ok(Some("known".into())))
                .unwrap(),
            "known"
        );
        only_entry(&cache).lock().unwrap().as_mut().unwrap().expires =
            Instant::now() - Duration::from_secs(1);
        assert_eq!(
            cache.resolve("p", "a", "t", || Ok(None)).unwrap_err().code,
            "invalid_upstream_response"
        );
        assert_eq!(
            cache
                .resolve("other", "a", "t", || Ok(None))
                .unwrap_err()
                .code,
            "invalid_upstream_response"
        );
    }

    #[test]
    fn rate_limit_errors_are_not_cached_as_successful_metadata() {
        let cache = ServerVpcCache::default();
        let error = cache
            .resolve("p", "a", "t", || {
                Err(ServiceError {
                    code: "rate_limited",
                    message: "Try later".into(),
                })
            })
            .unwrap_err();
        assert_eq!(error.code, "rate_limited");
        assert!(only_entry(&cache).lock().unwrap().is_none());
        assert_eq!(
            cache
                .resolve("p", "a", "t", || Ok(Some("recovered".into())))
                .unwrap(),
            "recovered"
        );
    }

    #[test]
    fn cancelled_lookups_neither_wait_for_the_cache_nor_publish_results() {
        let cache = ServerVpcCache::default();
        let requests = Arc::new(crate::requests::Requests::default());
        let permit = requests.admit("lookup", "catalog.store.local").unwrap();
        crate::requests::scope(permit.token.clone(), || {
            assert_eq!(
                cache
                    .resolve("p", "a", "t", || {
                        requests.cancel("lookup");
                        Ok(Some("cancelled".into()))
                    })
                    .unwrap_err()
                    .code,
                "cancelled"
            );
            let held = cache.0.lock().unwrap();
            assert!(held.values().all(|entry| entry.lock().unwrap().is_none()));
            assert_eq!(
                cache
                    .resolve("p", "a", "t", || panic!("cancelled fetch started"))
                    .unwrap_err()
                    .code,
                "cancelled"
            );
        });
    }
}
