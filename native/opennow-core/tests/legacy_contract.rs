use serde_json::Value;
use std::collections::HashSet;

const MANIFEST: &str = include_str!("../contracts/legacy-open-now-api.json");
const SCHEMA: &str = include_str!("../contracts/legacy-open-now-api.schema.json");
const CORE_DISPATCH: &str = include_str!("../src/main.rs");

#[test]
fn legacy_api_manifest_is_complete_unique_and_fixture_backed() {
    let manifest: Value = serde_json::from_str(MANIFEST).expect("contract manifest must be JSON");
    let schema: Value = serde_json::from_str(SCHEMA).expect("contract schema must be JSON");
    assert_eq!(manifest["version"], 1);
    assert_eq!(schema["properties"]["operations"]["minItems"], 109);
    let expected_columns = [
        "legacy",
        "kind",
        "requestSchema",
        "resultSchema",
        "owner",
        "replacement",
        "status",
        "requestFixture",
        "resultFixture",
    ];
    assert_eq!(
        manifest["columns"].as_array().expect("columns"),
        &expected_columns.map(Value::from)
    );

    let operations = manifest["operations"].as_array().expect("operations array");
    assert_eq!(
        operations.len(),
        109,
        "OpenNowApi surface changed without a contract update"
    );
    let mut names = HashSet::new();
    for operation in operations {
        let fields = operation.as_array().expect("operation tuple");
        assert_eq!(fields.len(), 9);
        let name = fields[0].as_str().expect("legacy name");
        assert!(names.insert(name), "duplicate legacy operation {name}");
        assert!(matches!(
            fields[1].as_str(),
            Some("request" | "send" | "event")
        ));
        assert!(
            !fields[2].as_str().unwrap_or_default().is_empty(),
            "{name} request schema"
        );
        assert!(
            !fields[3].as_str().unwrap_or_default().is_empty(),
            "{name} result schema"
        );
        assert!(matches!(
            fields[4].as_str(),
            Some("core" | "qt" | "streamer")
        ));
        assert!(
            !fields[5].as_str().unwrap_or_default().is_empty(),
            "{name} replacement"
        );
        assert!(matches!(
            fields[6].as_str(),
            Some("ported" | "superseded" | "pending")
        ));
        assert!(fields.get(7).is_some(), "{name} request fixture");
        assert!(fields.get(8).is_some(), "{name} result fixture");
        let serialized = serde_json::to_string(operation).expect("fixture must serialize");
        for secret in ["access_token", "refresh_token", "client_token", "password"] {
            assert!(
                !serialized.to_ascii_lowercase().contains(secret),
                "{name} fixture leaks {secret}"
            );
        }
    }
}

#[test]
fn legacy_api_manifest_has_no_pending_implementation() {
    let manifest: Value = serde_json::from_str(MANIFEST).expect("contract manifest must be JSON");
    for operation in manifest["operations"].as_array().expect("operations array") {
        let fields = operation.as_array().expect("operation tuple");
        assert_ne!(
            fields[6].as_str(),
            Some("pending"),
            "{} still has no Qt/Rust parity disposition",
            fields[0].as_str().unwrap_or("unnamed operation")
        );
    }
}

#[test]
fn every_ported_core_rpc_is_present_in_the_dispatcher() {
    let manifest: Value = serde_json::from_str(MANIFEST).unwrap();
    for operation in manifest["operations"].as_array().unwrap() {
        let fields = operation.as_array().unwrap();
        if fields[4] != "core" || fields[6] != "ported" {
            continue;
        }
        let replacement = fields[5].as_str().unwrap();
        for method in replacement.split(" + ") {
            if !method.contains('.') || method.contains(' ') {
                continue;
            }
            assert!(
                CORE_DISPATCH.contains(&format!("\"{method}\"")),
                "{} claims missing core method {method}",
                fields[0]
            );
        }
    }
}
