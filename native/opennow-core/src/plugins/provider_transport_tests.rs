use super::*;
use crate::requests::Requests;
use opennow_plugin_api::provider::{CatalogRequest, CatalogScope, Empty, ProviderOutcome};
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

pub(in crate::plugins) fn fixture() -> &'static Path {
    static FIXTURE: OnceLock<(tempfile::TempDir, PathBuf)> = OnceLock::new();
    &FIXTURE.get_or_init(|| {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("worker.rs");
        let executable = temp.path().join(format!("worker{}", std::env::consts::EXE_SUFFIX));
        fs::write(&source, r#"
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
fn field<'a>(line: &'a str, key: &str) -> &'a str {
    let marker = format!("\"{}\":", key);
    let value = line.split(&marker).nth(1).unwrap();
    if let Some(value) = value.strip_prefix('"') { value.split('"').next().unwrap() }
    else { value.split([',', '}']).next().unwrap() }
}
fn main() {
    let data = PathBuf::from(std::env::var_os("OPENNOW_PLUGIN_DATA_DIR").unwrap());
    let pending_pid = data.join(format!("process-id-{}.tmp", std::process::id()));
    std::fs::write(&pending_pid, std::process::id().to_string()).unwrap();
    std::fs::rename(pending_pid, data.join("last-process-id")).unwrap();
    let names: Vec<_> = std::env::vars_os().map(|(key, _)| key).collect();
    assert!(names.iter().all(|key| key == "OPENNOW_PLUGIN_DATA_DIR" || key == "LANG" || key == "SystemRoot"));
    let output = Arc::new(Mutex::new(std::io::stdout()));
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        if line.contains("\"type\":\"cancel\"") { continue; }
        let id = field(&line,"id").to_owned();
        let epoch = field(&line,"epoch").to_owned();
        let delay = std::fs::read_to_string(data.join(format!("{}.delay", id))).ok()
            .and_then(|value| value.parse::<u64>().ok()).unwrap_or(0);
        let response = std::fs::read_to_string(data.join(format!("{}.reply", id))).unwrap_or_else(|_| {
            if line.contains("\"method\":\"provider.hello\"") {
                let capabilities = line.split("\"capabilities\":").nth(1).unwrap().split(']').next().unwrap();
                return format!("{{\"type\":\"response\",\"v\":2,\"epoch\":$EPOCH,\"id\":\"$ID\",\"outcome\":{{\"status\":\"success\",\"reply\":{{\"method\":\"provider.hello\",\"result\":{{\"pluginId\":\"{}\",\"version\":\"{}\",\"protocolVersion\":2,\"capabilities\":{}],\"authKinds\":[\"anonymous\"]}}}}}},\"effects\":[],\"allocation\":null}}", field(&line,"pluginId"), field(&line,"version"), capabilities);
            }
            "{\"type\":\"response\",\"v\":2,\"epoch\":$EPOCH,\"id\":\"$ID\",\"outcome\":{\"status\":\"failure\",\"error\":{\"code\":\"service_unavailable\",\"retryAfterMs\":null}},\"effects\":[],\"allocation\":null}".into()
        }).replace("$ID", &id).replace("$EPOCH", &epoch);
        let exit = data.join("exit-after-response").exists();
        let gated = data.join(format!("{}.gate", id)).exists();
        let release = data.join(format!("{}.release", id));
        let output = output.clone();
        std::thread::spawn(move || {
            while gated && !release.exists() { std::thread::sleep(Duration::from_millis(5)); }
            std::thread::sleep(Duration::from_millis(delay));
            let mut output = output.lock().unwrap();
            writeln!(output, "{}", response).unwrap();
            output.flush().unwrap();
            if exit { std::process::exit(0); }
        });
    }
}
"#).unwrap();
        let mut compiler = Command::new("rustc");
        compiler.arg("--edition=2021").arg(&source).arg("-o").arg(&executable);
        let target = opennow_plugin_package::current_target();
        let linker_key = format!("CARGO_TARGET_{}_LINKER", target.replace('-', "_").to_ascii_uppercase());
        if let Some(linker) = std::env::var_os(linker_key) {
            let mut argument = std::ffi::OsString::from("linker=");
            argument.push(linker);
            compiler.arg("-C").arg(argument);
        }
        assert!(compiler.status().unwrap().success());
        (temp, executable)
    }).1
}

fn runtime(data: &Path) -> Arc<ProviderTransport> {
    ProviderTransport::spawn(fixture(), data, NonZeroU64::new(7).unwrap()).unwrap()
}

fn catalog() -> ProviderRequest {
    ProviderRequest::CatalogPublic(CatalogRequest {
        scope: CatalogScope::Public,
        query: opennow_plugin_api::CatalogQuery {
            query: String::new(),
            cursor: None,
            limit: 10,
        },
    })
}

fn wait_for(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !predicate() {
        assert!(Instant::now() < deadline, "condition did not settle");
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn exhausted_request_ids_fail_before_dispatch_without_wrapping() {
    let data = tempfile::tempdir().unwrap();
    let runtime = runtime(data.path());
    runtime.serial.store(u64::MAX, Ordering::Release);
    let result = runtime.submit(&catalog(), &Cancellation::default(), Duration::from_secs(1));
    assert!(matches!(result, Err(error) if error.code == "provider_unavailable"));
    assert_eq!(runtime.serial.load(Ordering::Acquire), u64::MAX);
    assert!(!runtime.busy());
    runtime.stop();
}

#[test]
fn child_responses_are_correlated_when_they_arrive_out_of_order() {
    let data = tempfile::tempdir().unwrap();
    fs::write(data.path().join("1.delay"), "200").unwrap();
    let runtime = runtime(data.path());
    let first_runtime = Arc::clone(&runtime);
    let first = thread::spawn(move || {
        first_runtime
            .call(&catalog(), &Cancellation::default(), Duration::from_secs(2))
            .unwrap()
    });
    wait_for(|| runtime.pending.lock().unwrap().len() == 1);
    let second = runtime
        .call(
            &ProviderRequest::AuthStatus(Empty {}),
            &Cancellation::default(),
            Duration::from_secs(2),
        )
        .unwrap();
    assert_eq!(second.id.as_str(), "2");
    assert_eq!(first.join().unwrap().id.as_str(), "1");
    assert!(!runtime.unhealthy());
    runtime.stop().terminate();
}

#[test]
fn catalog_cancellation_keeps_control_alive_and_reserves_the_abandoned_slot() {
    let data = tempfile::tempdir().unwrap();
    fs::write(data.path().join("1.delay"), "300").unwrap();
    let runtime = runtime(data.path());
    let requests = Arc::new(Requests::default());
    let permit = requests.admit("cancel", "catalog.public").unwrap();
    let cancellation = permit.token.clone();
    let caller = Arc::clone(&runtime);
    let pending =
        thread::spawn(move || caller.call(&catalog(), &cancellation, Duration::from_secs(2)));
    wait_for(|| runtime.busy());
    requests.cancel("cancel");
    let cancelled = pending.join().unwrap().unwrap_err();
    assert!(cancelled.dispatched);
    assert_eq!(cancelled.error.code, "cancelled");
    assert!(runtime.busy());
    let response = runtime
        .call(
            &ProviderRequest::AuthStatus(Empty {}),
            &Cancellation::default(),
            Duration::from_secs(1),
        )
        .unwrap();
    assert_eq!(response.id.as_str(), "2");
    wait_for(|| !runtime.busy());
    assert!(!runtime.unhealthy());
    runtime.stop().terminate();
}

#[test]
fn saturated_background_and_session_work_cannot_consume_receipt_capacity() {
    let data = tempfile::tempdir().unwrap();
    for id in 1..=9 {
        fs::write(data.path().join(format!("{id}.delay")), "700").unwrap();
    }
    let runtime = runtime(data.path());
    let mut calls = Vec::new();
    for count in 1..=4 {
        let caller = Arc::clone(&runtime);
        calls.push(thread::spawn(move || {
            caller.call(&catalog(), &Cancellation::default(), Duration::from_secs(2))
        }));
        wait_for(|| runtime.pending.lock().unwrap().len() == count);
    }
    let rejected = runtime
        .call(&catalog(), &Cancellation::default(), Duration::from_secs(1))
        .unwrap_err();
    assert!(!rejected.dispatched);
    assert_eq!(rejected.error.code, "busy_before_dispatch");
    let session: opennow_plugin_api::provider::SessionKey =
        serde_json::from_value(json!({"account":null,"remoteId":"seat"})).unwrap();
    for count in 5..=8 {
        let caller = Arc::clone(&runtime);
        let session = session.clone();
        calls.push(thread::spawn(move || {
            caller.call(
                &ProviderRequest::SessionPoll(session),
                &Cancellation::default(),
                Duration::from_secs(2),
            )
        }));
        wait_for(|| runtime.pending.lock().unwrap().len() == count);
    }
    let receipt = ProviderRequest::SessionResolveAllocation(
        serde_json::from_value(json!({"operation":"op","receipt":"receipt","decision":"rejected"}))
            .unwrap(),
    );
    let start = Instant::now();
    assert!(
        runtime
            .call(&receipt, &Cancellation::default(), Duration::from_secs(1))
            .is_ok()
    );
    assert!(start.elapsed() < Duration::from_millis(500));
    for call in calls {
        assert!(call.join().unwrap().is_ok());
    }
    runtime.stop().terminate();
}

fn create() -> ProviderRequest {
    serde_json::from_value(json!({"method":"session.create","params":{
        "scope":null,"operation":"original-create","target":{"game":"game1","variant":"default"},
        "catalogRevision":"revision1","settingsRevision":1,
        "preferences":{"video":{"encoding":null,"width":1280,"height":720,"fps":null,"bitDepth":8,"chroma":"yuv420","hdr":false},"bitrateKbps":10000},
        "offer":{"version":1,"offerId":"offer","runtimeEpoch":19,"expiresAtMs":10000,
            "videoFormats":[{"encoding":"h264-annex-b","bitDepth":8,"chroma":"yuv420","dynamicRange":"sdr","maxWidth":1920,"maxHeight":1080,"maxFps":60}],
            "audioFormats":[],"input":{"keyboard":true,"relativeMouse":true,"absoluteMouse":false,"text":false,"gamepadSlots":0,"rumble":false},
            "limits":{"maxVideoAccessUnitBytes":1048576,"maxAudioPacketBytes":65536,"maxControlMessageBytes":65536,"maxBufferedVideoBytes":2097152,
                "maxBufferedVideoFrames":4,"maxBufferedAudioMs":100,"maxPendingInputEvents":128}}
    }})).unwrap()
}

fn create_failure_with_ticket() -> Value {
    json!({"type":"response","v":2,"epoch":7,"id":"1",
        "outcome":{"status":"failure","error":{"code":"cleanup_required","retryAfterMs":null}},
        "effects":[{"kind":"cleanup-required","operation":"original-create"}],
        "allocation":{"operation":"original-create","receipt":"original-receipt","session":{"account":null,"remoteId":"original-seat"}}})
}

#[test]
fn late_failed_create_preserves_original_receipt_and_effects_without_killing_child() {
    let data = tempfile::tempdir().unwrap();
    fs::write(data.path().join("1.delay"), "200").unwrap();
    fs::write(
        data.path().join("1.reply"),
        serde_json::to_vec(&create_failure_with_ticket()).unwrap(),
    )
    .unwrap();
    let runtime = runtime(data.path());
    let failure = runtime
        .call(
            &create(),
            &Cancellation::default(),
            Duration::from_millis(20),
        )
        .unwrap_err();
    assert!(failure.dispatched);
    assert_eq!(failure.error.code, "outcome_unknown");
    assert!(runtime.busy());
    wait_for(|| runtime.has_notifications());
    let notifications = runtime.take_completions();
    assert_eq!(notifications.len(), 1);
    assert!(matches!(
        notifications[0].request,
        ProviderRequest::SessionCreate(_)
    ));
    let ticket = notifications[0].response.allocation.as_ref().unwrap();
    assert_eq!(ticket.operation.as_str(), "original-create");
    assert_eq!(ticket.receipt.as_str(), "original-receipt");
    assert_eq!(ticket.session.remote_id.as_str(), "original-seat");
    assert_eq!(notifications[0].response.effects.len(), 1);
    assert!(!runtime.unhealthy());
    runtime.stop().terminate();
}

#[test]
fn cancellation_of_allocation_waits_for_its_bounded_obligation_response() {
    let data = tempfile::tempdir().unwrap();
    fs::write(data.path().join("1.delay"), "150").unwrap();
    fs::write(
        data.path().join("1.reply"),
        serde_json::to_vec(&create_failure_with_ticket()).unwrap(),
    )
    .unwrap();
    let runtime = runtime(data.path());
    let requests = Arc::new(Requests::default());
    let permit = requests.admit("allocate", "session.create").unwrap();
    let cancellation = permit.token.clone();
    let caller = Arc::clone(&runtime);
    let pending =
        thread::spawn(move || caller.call(&create(), &cancellation, Duration::from_secs(1)));
    wait_for(|| runtime.busy());
    requests.cancel("allocate");
    let response = pending.join().unwrap().unwrap();
    assert!(response.allocation.is_some());
    assert!(matches!(response.outcome, ProviderOutcome::Failure { .. }));
    assert!(!runtime.unhealthy());
    runtime.stop().terminate();
}

#[test]
fn complete_response_before_eof_is_delivered_and_child_exit_is_observed() {
    let data = tempfile::tempdir().unwrap();
    fs::write(data.path().join("exit-after-response"), b"exit").unwrap();
    let runtime = runtime(data.path());
    assert!(
        runtime
            .call(
                &ProviderRequest::AuthStatus(Empty {}),
                &Cancellation::default(),
                Duration::from_secs(1)
            )
            .is_ok()
    );
    wait_for(|| runtime.unhealthy());
    runtime.stop().terminate();
}

#[test]
fn wrong_epoch_and_unsolicited_response_fail_closed() {
    for (epoch, id) in [(8, "1"), (7, "not-issued")] {
        let data = tempfile::tempdir().unwrap();
        fs::write(data.path().join("1.reply"), serde_json::to_vec(&json!({"type":"response","v":2,"epoch":epoch,"id":id,
            "outcome":{"status":"failure","error":{"code":"service_unavailable","retryAfterMs":null}},"effects":[],"allocation":null})).unwrap()).unwrap();
        let runtime = runtime(data.path());
        assert!(
            runtime
                .call(
                    &ProviderRequest::AuthStatus(Empty {}),
                    &Cancellation::default(),
                    Duration::from_secs(1)
                )
                .is_err()
        );
        wait_for(|| runtime.unhealthy());
        runtime.stop().terminate();
    }
}

#[test]
fn unanswered_obligation_retires_after_deadline_and_grace_but_catalog_does_not() {
    let data = tempfile::tempdir().unwrap();
    fs::write(data.path().join("1.delay"), "600000").unwrap();
    let runtime = runtime(data.path());
    let request = ProviderRequest::SessionStop(serde_json::from_value(json!({"session":{"account":null,"remoteId":"retained-seat"},"operation":"stop-original"})).unwrap());
    let failure = runtime
        .call(
            &request,
            &Cancellation::default(),
            Duration::from_millis(20),
        )
        .unwrap_err();
    assert!(failure.dispatched);
    assert_eq!(failure.error.code, "outcome_unknown");
    wait_for(|| runtime.unhealthy());
    runtime.stop().terminate();

    let data = tempfile::tempdir().unwrap();
    fs::write(data.path().join("1.delay"), "600000").unwrap();
    let runtime = self::runtime(data.path());
    assert!(
        runtime
            .call(
                &catalog(),
                &Cancellation::default(),
                Duration::from_millis(20)
            )
            .is_err()
    );
    thread::sleep(RETIREMENT_GRACE + Duration::from_millis(50));
    assert!(!runtime.unhealthy());
    assert!(
        runtime
            .call(&request, &Cancellation::default(), Duration::from_secs(1))
            .is_ok()
    );
    runtime.stop().terminate();
}

#[test]
fn abandoned_poll_discovery_and_control_calls_cannot_permanently_exhaust_recovery_capacity() {
    let session =
        serde_json::from_value(json!({"account":null,"remoteId":"original-seat"})).unwrap();
    let requests = [
        ProviderRequest::SessionPoll(session),
        ProviderRequest::SessionDiscover(serde_json::from_value(json!({"scope":null})).unwrap()),
        ProviderRequest::AuthStatus(Empty {}),
    ];
    for request in requests {
        let data = tempfile::tempdir().unwrap();
        for id in 1..=4 {
            fs::write(data.path().join(format!("{id}.delay")), "600000").unwrap();
        }
        let runtime = runtime(data.path());
        for _ in 0..4 {
            let result = runtime.call(
                &request,
                &Cancellation::default(),
                Duration::from_millis(40),
            );
            assert!(result.unwrap_err().dispatched);
        }
        assert!(runtime.busy());
        wait_for(|| runtime.unhealthy());
        assert!(!runtime.busy());
        runtime.stop().terminate();
    }
}
