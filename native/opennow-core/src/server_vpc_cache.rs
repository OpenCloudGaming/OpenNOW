use crate::gfn::ServiceError;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const FRESHNESS: Duration = Duration::from_secs(5 * 60);
const FAILURE_BACKOFF: Duration = Duration::from_secs(30);

struct Entry {
    scope: [u8; 32],
    vpc_id: String,
    expires: Instant,
}

#[derive(Default)]
pub struct ServerVpcCache(Mutex<Option<Entry>>);

impl ServerVpcCache {
    pub fn resolve(
        &self,
        provider: &str,
        account: &str,
        token: &str,
        fetch: impl FnOnce() -> Result<Option<String>, ServiceError>,
    ) -> Result<String, ServiceError> {
        let scope = Sha256::digest(json!([provider, account, token]).to_string().as_bytes()).into();
        let mut cached = crate::store_requests::lock(&self.0)?;
        if let Some(entry) = cached
            .as_ref()
            .filter(|entry| entry.scope == scope && entry.expires > Instant::now())
        {
            return Ok(entry.vpc_id.clone());
        }
        let result = fetch()?;
        crate::requests::check()?;
        let (vpc_id, freshness) = match result.filter(|value| !value.is_empty()) {
            Some(value) => (value, FRESHNESS),
            None => (
                cached
                    .as_ref()
                    .filter(|entry| entry.scope == scope)
                    .map(|entry| entry.vpc_id.clone())
                    .unwrap_or_else(|| "GFN-PC".into()),
                FAILURE_BACKOFF,
            ),
        };
        *cached = Some(Entry {
            scope,
            vpc_id: vpc_id.clone(),
            expires: Instant::now() + freshness,
        });
        Ok(vpc_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc, Barrier,
        atomic::{AtomicUsize, Ordering},
    };

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
        let mut cached = cache.0.lock().unwrap();
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
        assert_eq!(cache.resolve("p", "a", "t", || Ok(None)).unwrap(), "GFN-PC");
        assert!(
            cache
                .0
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
            .unwrap();
        cache.0.lock().unwrap().as_mut().unwrap().expires = Instant::now() - Duration::from_secs(1);
        assert_eq!(
            cache
                .resolve("p", "a", "t", || Ok(Some("known".into())))
                .unwrap(),
            "known"
        );
        cache.0.lock().unwrap().as_mut().unwrap().expires = Instant::now() - Duration::from_secs(1);
        assert_eq!(cache.resolve("p", "a", "t", || Ok(None)).unwrap(), "known");
        assert_eq!(
            cache.resolve("other", "a", "t", || Ok(None)).unwrap(),
            "GFN-PC"
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
        assert!(cache.0.lock().unwrap().is_none());
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
            assert!(held.is_none());
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
