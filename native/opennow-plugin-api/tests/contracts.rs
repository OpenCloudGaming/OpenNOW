use opennow_plugin_api::*;
use serde_json::{Value, json};

fn manifest() -> Value {
    json!({
        "schemaVersion":1,
        "id":EXAMPLE_PLUGIN_ID,
        "name":"Example catalog",
        "version":"1.0.0",
        "publisher":"Example author",
        "description":"An illustrative catalog",
        "protocolVersion":1,
        "capabilities":[CATALOG_CAPABILITY],
        "entrypoints":{"x86_64-unknown-linux-gnu":"bin/catalog-plugin"},
        "files":[{"path":"bin/catalog-plugin","sha256":"0".repeat(64)}]
    })
}

fn page() -> Value {
    json!({"items":[{"id":"same-local-id","title":"Example game"}],"nextCursor":null,"coverage":"unknown"})
}

fn response(result: Value) -> Value {
    json!({"v":1,"type":"response","epoch":7,"id":"1","ok":true,"result":result})
}

#[test]
fn plugin_ids_are_names_not_paths() {
    for valid in [BUILTIN_GFN_ID, EXAMPLE_PLUGIN_ID, "com.author.some-plugin2"] {
        assert_eq!(PluginId::new(valid).unwrap().as_str(), valid);
    }
    for invalid in [
        "", "a.b", "a..b", "A.b.c", "a.b.2c", "a.b.-c", "a.b.c-", "a/b/c", "a.b.é", "a.b.c\\x",
    ] {
        assert!(PluginId::new(invalid).is_err(), "{invalid}");
    }
    assert!(PluginId::new(format!("a.b.{}", "x".repeat(64))).is_err());
}

#[test]
fn catalog_defaults_do_not_accept_invalid_explicit_values() {
    let defaults: CatalogQuery = serde_json::from_value(json!({})).unwrap();
    assert_eq!(defaults, CatalogQuery::default());
    for invalid in [
        json!({"limit":0}),
        json!({"limit":101}),
        json!({"limit":1.5}),
        json!({"limit":null}),
        json!({"limit":"20"}),
        json!({"query":null}),
        json!({"query":"x".repeat(513)}),
        json!({"query":"é".repeat(257)}),
        json!({"query":"line\nline"}),
        json!({"cursor":""}),
        json!({"cursor":"x".repeat(4097)}),
        json!({"settings":{}}),
    ] {
        assert!(
            serde_json::from_value::<CatalogQuery>(invalid.clone()).is_err(),
            "{invalid}"
        );
    }
    assert!(
        serde_json::from_value::<CatalogQuery>(
            json!({"query":"é".repeat(256),"cursor":"x".repeat(4096),"limit":100})
        )
        .is_ok()
    );
}

#[test]
fn catalog_pages_reject_duplicate_ids_and_actions() {
    let accepted: CatalogPage = serde_json::from_value(page()).unwrap();
    assert_eq!(accepted.coverage, Coverage::Unknown);
    for invalid in [
        json!({"items":[{"id":"a","title":"One"},{"id":"a","title":"Two"}],"nextCursor":null,"coverage":"complete"}),
        json!({"items":[{"id":"","title":"One"}],"nextCursor":null,"coverage":"unknown"}),
        json!({"items":[{"id":"a","title":" "}],"nextCursor":null,"coverage":"unknown"}),
        json!({"items":[{"id":"a","title":"\u{0}"}],"nextCursor":null,"coverage":"unknown"}),
        json!({"items":[{"id":"a","title":"x".repeat(257)}],"nextCursor":null,"coverage":"unknown"}),
        json!({"items":[{"id":"a","title":"One","action":"launch"}],"nextCursor":null,"coverage":"unknown"}),
        json!({"items":[],"nextCursor":"","coverage":"unknown"}),
        json!({"items":[],"nextCursor":null,"coverage":"invented"}),
        json!({"items":[],"nextCursor":null,"coverage":"unknown","sourceId":BUILTIN_GFN_ID}),
    ] {
        assert!(
            serde_json::from_value::<CatalogPage>(invalid.clone()).is_err(),
            "{invalid}"
        );
    }
    let mut oversized = page();
    oversized["items"] = json!(
        (0..101)
            .map(|id| json!({"id":id.to_string(),"title":"Game"}))
            .collect::<Vec<_>>()
    );
    assert!(serde_json::from_value::<CatalogPage>(oversized).is_err());
}

#[test]
fn source_binding_preserves_local_ids_without_collisions() {
    let page: CatalogPage = serde_json::from_value(page()).unwrap();
    let first = SourceCatalogPage::bind(PluginId::new(BUILTIN_GFN_ID).unwrap(), 1, page.clone());
    let second = SourceCatalogPage::bind(PluginId::new(EXAMPLE_PLUGIN_ID).unwrap(), 2, page);
    assert_ne!(first.items[0].id, second.items[0].id);
    assert_eq!(first.items[0].id.local_id, second.items[0].id.local_id);
    let encoded = serde_json::to_value(second).unwrap();
    assert_eq!(
        encoded["items"][0]["id"],
        json!({"sourceId":EXAMPLE_PLUGIN_ID,"localId":"same-local-id"})
    );
    assert_eq!(encoded["generation"], 2);
}

#[test]
fn manifest_accepts_only_catalog_and_unreserved_identity() {
    let parsed: PluginManifest = serde_json::from_value(manifest()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), manifest());
    for (key, value) in [
        ("id", json!(BUILTIN_GFN_ID)),
        ("capabilities", json!(["session.v1"])),
        ("capabilities", json!(["catalog.v1", "catalog.v1"])),
        ("schemaVersion", json!(2)),
        ("protocolVersion", json!(2)),
        ("description", json!("x".repeat(1025))),
        ("version", json!("latest")),
        ("qml", json!("Plugin.qml")),
        ("builtin", json!(true)),
    ] {
        let mut invalid = manifest();
        invalid[key] = value;
        assert!(
            serde_json::from_value::<PluginManifest>(invalid).is_err(),
            "{key}"
        );
    }
}

#[test]
fn manifest_rejects_aliases_traversal_and_unlisted_entrypoints() {
    for path in [
        "/bin/run",
        "../run",
        "bin/../run",
        "C:/run",
        "bin\\run",
        "bin//run",
        "bin/./run",
        "bin/CON.exe",
        "bin/run.",
        "bin/run ",
        "bin/run:stream",
        "bin/rün",
        "bin/ru?n",
    ] {
        assert!(validate_package_path(path).is_err(), "{path}");
    }
    let mut duplicate = manifest();
    duplicate["files"]
        .as_array_mut()
        .unwrap()
        .push(json!({"path":"BIN/catalog-plugin","sha256":"0".repeat(64)}));
    assert!(serde_json::from_value::<PluginManifest>(duplicate).is_err());
    let mut unlisted = manifest();
    unlisted["entrypoints"]["x86_64-unknown-linux-gnu"] = json!("other");
    assert!(serde_json::from_value::<PluginManifest>(unlisted).is_err());
    let mut invalid_hash = manifest();
    invalid_hash["files"][0]["sha256"] = json!("F".repeat(64));
    assert!(serde_json::from_value::<PluginManifest>(invalid_hash).is_err());
    let encoded = manifest().to_string().replace(
        "\"entrypoints\":{\"x86_64-unknown-linux-gnu\":\"bin/catalog-plugin\"}",
        "\"entrypoints\":{\"x86_64-unknown-linux-gnu\":\"bin/catalog-plugin\",\"x86_64-unknown-linux-gnu\":\"bin/catalog-plugin\"}",
    );
    assert!(serde_json::from_str::<PluginManifest>(&encoded).is_err());
}

#[test]
fn host_wire_round_trips_all_finite_operations() {
    for payload in [
        RequestPayload::Hello(HelloRequest {
            plugin_id: PluginId::new(EXAMPLE_PLUGIN_ID).unwrap(),
            capabilities: vec![CATALOG_CAPABILITY.into()],
        }),
        RequestPayload::CatalogPage(CatalogQuery::default()),
        RequestPayload::Shutdown,
    ] {
        let message = HostMessage::Request(HostRequest {
            v: 1,
            epoch: 7,
            id: "request-1".into(),
            payload,
            timeout_ms: 5000,
        });
        let encoded = serde_json::to_value(&message).unwrap();
        assert_eq!(encoded["type"], "request");
        assert!(encoded.get("op").is_some());
        assert_eq!(encoded["timeoutMs"], 5000);
        assert_eq!(
            serde_json::from_value::<HostMessage>(encoded).unwrap(),
            message
        );
    }
    let cancel = HostMessage::Cancel {
        v: 1,
        epoch: 7,
        id: "request-1".into(),
    };
    assert_eq!(
        serde_json::from_value::<HostMessage>(serde_json::to_value(&cancel).unwrap()).unwrap(),
        cancel
    );
}

#[test]
fn host_wire_rejects_escape_hatches_and_unknown_fields() {
    let valid = json!({"v":1,"type":"request","epoch":7,"id":"1","op":"catalog.page","args":{},"timeoutMs":10000});
    for (key, value) in [
        ("v", json!(2)),
        ("epoch", json!(0)),
        ("id", json!("x".repeat(65))),
        ("op", json!("session.create")),
        ("timeoutMs", json!(0)),
        ("timeoutMs", json!(10001)),
        ("args", json!({"nativeCommands":[]})),
        ("environment", json!({})),
    ] {
        let mut invalid = valid.clone();
        invalid[key] = value;
        assert!(
            serde_json::from_value::<HostMessage>(invalid).is_err(),
            "{key}"
        );
    }
    assert!(
        serde_json::from_value::<HostMessage>(
            json!({"v":1,"type":"cancel","epoch":7,"id":"1","op":"catalog.page"})
        )
        .is_err()
    );
}

#[test]
fn plugin_wire_round_trips_and_requires_single_outcome() {
    let valid = response(page());
    let parsed: PluginMessage = serde_json::from_value(valid.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), valid);
    for (key, value) in [
        ("ok", json!(false)),
        ("error", json!({"code":"cancelled"})),
        ("error", Value::Null),
        ("v", json!(2)),
        ("epoch", json!(0)),
        ("result", Value::Null),
        ("event", json!("auth.session.changed")),
    ] {
        let mut invalid = valid.clone();
        invalid[key] = value;
        assert!(
            serde_json::from_value::<PluginMessage>(invalid).is_err(),
            "{key}"
        );
    }
    let failure = PluginMessage {
        v: 1,
        epoch: 7,
        id: "1".into(),
        outcome: Err(PluginFailure {
            code: PluginFailureCode::Cancelled,
        }),
    };
    assert_eq!(
        serde_json::from_value::<PluginMessage>(serde_json::to_value(&failure).unwrap()).unwrap(),
        failure
    );
    assert!(serde_json::from_value::<PluginMessage>(json!({"v":1,"type":"response","epoch":7,"id":"1","ok":false,"error":{"code":"cancelled","message":"secret"}})).is_err());
}

#[test]
fn hello_requires_version_and_catalog_only_capabilities() {
    let valid = json!({"pluginId":EXAMPLE_PLUGIN_ID,"version":"1.0.0","protocolVersion":1,"capabilities":["catalog.v1"]});
    assert!(serde_json::from_value::<PluginMessage>(response(valid.clone())).is_ok());
    for (key, value) in [
        ("version", json!("bad")),
        ("protocolVersion", json!(2)),
        ("capabilities", json!(["catalog.v1", "auth.v1"])),
    ] {
        let mut invalid = valid.clone();
        invalid[key] = value;
        assert!(serde_json::from_value::<PluginMessage>(response(invalid)).is_err());
    }
    let mut missing = valid;
    missing.as_object_mut().unwrap().remove("version");
    assert!(serde_json::from_value::<PluginMessage>(response(missing)).is_err());
}
