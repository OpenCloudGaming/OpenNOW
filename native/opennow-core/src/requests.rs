//! Bounded RPC ownership and cooperative cancellation for read-only work.
use crate::gfn::ServiceError;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const MAX_ACTIVE: usize = 8;
const MAX_BACKGROUND: usize = 4;

#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    pub fn check(&self) -> Result<(), ServiceError> {
        if self.cancelled() {
            Err(ServiceError {
                code: "cancelled",
                message: "Request cancelled".into(),
            })
        } else {
            Ok(())
        }
    }
}

#[derive(Default)]
pub struct Requests(Mutex<HashMap<String, (bool, Cancellation)>>);
impl Requests {
    pub fn admit(self: &Arc<Self>, id: &str, method: &str) -> Option<Permit> {
        let background = method.starts_with("catalog.")
            || method.starts_with("artwork.")
            || method == "network.regions.ping";
        let mut active = self.0.lock().expect("request state poisoned");
        if active.contains_key(id)
            || active.len() >= MAX_ACTIVE
            || (background && active.values().filter(|(bg, _)| *bg).count() >= MAX_BACKGROUND)
        {
            return None;
        }
        let token = Cancellation::default();
        active.insert(id.into(), (background, token.clone()));
        Some(Permit {
            owner: self.clone(),
            id: id.into(),
            token,
        })
    }
    pub fn cancel(&self, id: &str) {
        if let Some((_, token)) = self.0.lock().expect("request state poisoned").get(id) {
            token.0.store(true, Ordering::Release);
        }
    }
}

pub struct Permit {
    owner: Arc<Requests>,
    id: String,
    pub token: Cancellation,
}
impl Drop for Permit {
    fn drop(&mut self) {
        self.owner
            .0
            .lock()
            .expect("request state poisoned")
            .remove(&self.id);
    }
}

thread_local! { static CURRENT: RefCell<Cancellation> = RefCell::default(); }
pub fn current() -> Cancellation {
    CURRENT.with(|token| token.borrow().clone())
}
pub fn check() -> Result<(), ServiceError> {
    current().check()
}

// A request worker is synchronous. Scoped child threads explicitly clone the
// token; cache/download workers intentionally have independent lifetimes.
pub fn scope<T>(token: Cancellation, work: impl FnOnce() -> T) -> T {
    struct Restore(Cancellation);
    impl Drop for Restore {
        fn drop(&mut self) {
            CURRENT.with(|token| *token.borrow_mut() = self.0.clone());
        }
    }
    let _restore = Restore(CURRENT.with(|current| current.replace(token)));
    work()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn background_load_reserves_control_capacity_until_workers_really_exit() {
        let requests = Arc::new(Requests::default());
        let mut permits = Vec::new();
        for id in 0..4 {
            permits.push(
                requests
                    .admit(&id.to_string(), "catalog.store.local")
                    .unwrap(),
            );
        }
        requests.cancel("0");
        assert!(permits[0].token.cancelled());
        assert!(requests.admit("more", "catalog.store.local").is_none());
        for id in 4..8 {
            permits.push(requests.admit(&id.to_string(), "session.poll").unwrap());
        }
        assert!(requests.admit("overflow", "session.stop").is_none());
        permits.clear();
        assert!(requests.admit("new", "catalog.store.local").is_some());
    }
    #[test]
    fn unknown_cancels_do_not_accumulate_and_scopes_restore() {
        let requests = Arc::new(Requests::default());
        for id in 0..10000 {
            requests.cancel(&id.to_string());
        }
        assert!(requests.0.lock().unwrap().is_empty());
        let permit = requests.admit("work", "catalog.store.local").unwrap();
        assert!(requests.admit("work", "session.poll").is_none());
        requests.cancel("work");
        scope(permit.token.clone(), || {
            assert_eq!(check().unwrap_err().code, "cancelled")
        });
        assert!(check().is_ok());
    }
}
