//! Bounded RPC ownership, cooperative cancellation, and allocation receipts.
use crate::service_error::ServiceError;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

const MAX_ACTIVE: usize = 8;
const MAX_BACKGROUND: usize = 4;

#[derive(Default)]
struct RequestState {
    cancelled: AtomicBool,
    accepted: Mutex<bool>,
    wake: Condvar,
}

#[derive(Clone, Default)]
pub struct Cancellation(Arc<RequestState>);
impl Cancellation {
    pub fn commit<T>(
        &self,
        work: impl FnOnce() -> Result<T, ServiceError>,
    ) -> Result<T, ServiceError> {
        let _commit = self.0.accepted.lock().expect("request commit poisoned");
        self.check()?;
        work()
    }
    pub fn cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Acquire)
    }

    pub fn await_acceptance(&self, timeout: Duration) -> bool {
        let accepted = self.0.accepted.lock().expect("request receipt poisoned");
        let (accepted, _) = self
            .0
            .wake
            .wait_timeout_while(accepted, timeout, |accepted| {
                !*accepted && !self.cancelled()
            })
            .expect("request receipt poisoned");
        *accepted && !self.cancelled()
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
            || method == "sources.catalog.page"
            || method == "network.regions.ping"
            || method == "queue.servers.list";
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
            let _accepted = token.0.accepted.lock().expect("request receipt poisoned");
            token.0.cancelled.store(true, Ordering::Release);
            token.0.wake.notify_all();
        }
    }

    pub fn acknowledge(&self, id: &str) {
        if let Some((_, token)) = self.0.lock().expect("request state poisoned").get(id) {
            *token.0.accepted.lock().expect("request receipt poisoned") = true;
            token.0.wake.notify_all();
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
    fn auth_commit_fence_rejects_prior_cancellation_and_finishes_entered_commit() {
        let requests = Arc::new(Requests::default());
        let permit = requests.admit("login", "auth.device.complete").unwrap();
        requests.cancel("login");
        assert!(
            permit
                .token
                .commit(|| -> Result<(), ServiceError> { panic!("cancelled commit ran") })
                .is_err()
        );
        let permit = requests.admit("second", "auth.device.complete").unwrap();
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let token = permit.token.clone();
        std::thread::scope(|scope| {
            let worker = scope.spawn(move || {
                token.commit(|| {
                    entered_tx.send(()).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    Ok("committed")
                })
            });
            entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            let cancellation = scope.spawn(|| requests.cancel("second"));
            release_tx.send(()).unwrap();
            assert_eq!(worker.join().unwrap().unwrap(), "committed");
            cancellation.join().unwrap();
        });
        assert!(permit.token.cancelled());
    }
    #[test]
    fn allocation_receipt_requires_acceptance_and_cancellation_wins() {
        let requests = Arc::new(Requests::default());
        let permit = requests.admit("create", "session.create").unwrap();
        assert!(!permit.token.await_acceptance(Duration::ZERO));
        requests.acknowledge("unknown");
        assert!(!permit.token.await_acceptance(Duration::ZERO));
        requests.acknowledge("create");
        assert!(permit.token.await_acceptance(Duration::ZERO));
        requests.cancel("create");
        assert!(!permit.token.await_acceptance(Duration::ZERO));
    }

    #[test]
    fn cancellation_wakes_pending_allocation_receipt() {
        let requests = Arc::new(Requests::default());
        let permit = requests.admit("create", "session.create").unwrap();
        std::thread::scope(|scope| {
            let token = permit.token.clone();
            let worker = scope.spawn(move || token.await_acceptance(Duration::from_secs(10)));
            requests.cancel("create");
            assert!(!worker.join().unwrap());
        });
    }
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
    fn source_catalog_shares_background_capacity_without_starving_session_control() {
        let requests = Arc::new(Requests::default());
        let mut permits = Vec::new();
        for index in 0..4 {
            permits.push(
                requests
                    .admit(&format!("catalog-{index}"), "sources.catalog.page")
                    .unwrap(),
            );
        }
        assert!(requests.admit("overflow", "catalog.library.list").is_none());
        assert!(requests.admit("control", "session.stop").is_some());
        requests.cancel("catalog-0");
        assert!(
            requests
                .admit("cancel-still-running", "sources.catalog.page")
                .is_none()
        );
        permits.pop();
        assert!(requests.admit("released", "sources.catalog.page").is_some());
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
