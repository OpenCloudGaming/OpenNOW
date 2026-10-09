const CORE: &str = include_str!("../src/main.rs");
const REQUESTS: &str = include_str!("../src/requests.rs");

#[test]
fn application_dispatch_does_not_own_geforce_now_services() {
    for dependency in [
        "GfnService",
        "PushRegistry",
        "core.gfn",
        "worker_core.gfn",
        "authenticated_snapshot",
        "finish_session_create",
        "cloudmatch::allocation_settings",
        "prepare_owned_stream",
    ] {
        assert!(
            !CORE.contains(dependency),
            "application dispatch must delegate {dependency} to its owning module"
        );
    }
}

#[test]
fn request_admission_and_cancellation_do_not_depend_on_a_provider() {
    for dependency in ["crate::gfn", "sources::gfn", "GfnService"] {
        assert!(
            !REQUESTS.contains(dependency),
            "shared request ownership must not depend on {dependency}"
        );
    }
}
