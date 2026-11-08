use super::tests::{auth_fixture, test_service};
use super::*;
use std::cell::Cell;

#[test]
fn unavailable_restore_retries_after_backoff_and_installs_the_saved_scope() {
    let (service, path) = test_service("http://127.0.0.1:1");
    let now = Instant::now();
    let loads = Cell::new(0);
    service.restore_session_locked_with(now, || {
        loads.set(loads.get() + 1);
        Err("fixture store is locked".into())
    });
    assert_eq!(
        service.state.lock().unwrap().persistence_state,
        "unavailable"
    );
    service.restore_session_locked_with(now + Duration::from_millis(999), || {
        panic!("restore bypassed backoff");
    });
    service.restore_session_locked_with(now + Duration::from_secs(1), || {
        loads.set(loads.get() + 1);
        Ok(Some(auth_fixture("saved-account")))
    });
    assert_eq!(loads.get(), 2);
    let state = service.state.lock().unwrap();
    assert_eq!(
        state.session.as_ref().unwrap().user.user_id,
        "saved-account"
    );
    assert_eq!(state.generation, 1);
    drop(state);
    service.restore_session_locked_with(now + Duration::from_secs(100), || {
        panic!("restored account loaded twice");
    });
    std::fs::remove_dir_all(path).ok();
}

#[test]
fn explicit_logout_suppresses_pending_restore_retry() {
    let (service, path) = test_service("http://127.0.0.1:1");
    let now = Instant::now();
    service.restore_session_locked_with(now, || Err("locked".into()));
    service.invalidate_auth_work(true);
    service.restore_session_locked_with(now + Duration::from_secs(100), || {
        panic!("explicit logout restored a saved account");
    });
    assert!(service.state.lock().unwrap().session.is_none());
    std::fs::remove_dir_all(path).ok();
}

#[test]
fn repeated_restore_failures_use_capped_backoff() {
    let (service, path) = test_service("http://127.0.0.1:1");
    let mut now = Instant::now();
    for seconds in [1, 2, 4, 8, 16, 30, 30] {
        service.restore_session_locked_with(now, || Err("locked".into()));
        let state = service.state.lock().unwrap();
        assert_eq!(
            state.restore_retry_at,
            Some(now + Duration::from_secs(seconds))
        );
        assert_eq!(state.generation, 0);
        drop(state);
        service.restore_session_locked_with(
            now + Duration::from_secs(seconds) - Duration::from_nanos(1),
            || {
                panic!("retry occurred before its deadline");
            },
        );
        now += Duration::from_secs(seconds);
    }
    service.restore_session_locked_with(now, || Ok(Some(auth_fixture("recovered"))));
    assert_eq!(
        service
            .state
            .lock()
            .unwrap()
            .session
            .as_ref()
            .unwrap()
            .user
            .user_id,
        "recovered"
    );
    std::fs::remove_dir_all(path).ok();
}

#[test]
fn successful_empty_or_pin_protected_restore_is_not_retried() {
    for protected in [false, true] {
        let (service, path) = test_service("http://127.0.0.1:1");
        if protected {
            assert_eq!(
                service.profiles.set_pin("saved", "1234", None).unwrap()["ok"],
                true
            );
        }
        let now = Instant::now();
        service.restore_session_locked_with(now, || Err("locked".into()));
        service.restore_session_locked_with(now + Duration::from_secs(1), || {
            Ok(protected.then(|| auth_fixture("saved")))
        });
        service.restore_session_locked_with(now + Duration::from_secs(100), || {
            panic!("completed or PIN-protected restore was retried");
        });
        assert!(service.state.lock().unwrap().session.is_none());
        std::fs::remove_dir_all(path).ok();
    }
}

#[test]
fn retry_keeps_vault_tombstones_suppressed() {
    let (service, path) = test_service("http://127.0.0.1:1");
    service.vault.save(&auth_fixture("removed")).unwrap();
    service.vault.remove("removed").unwrap();
    let now = Instant::now();
    service.restore_session_locked_with(now, || Err("locked".into()));
    service
        .restore_session_locked_with(now + Duration::from_secs(1), || service.vault.load_active());
    assert!(service.state.lock().unwrap().session.is_none());
    std::fs::remove_dir_all(path).ok();
}
