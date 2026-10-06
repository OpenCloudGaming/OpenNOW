pub(crate) mod contract;
pub(crate) mod gfn;

use crate::plugins::PluginManager;
use crate::requests::Cancellation;
use contract::{
    BuiltinModule, CatalogQuery, CatalogSource, Completion, PluginId, PluginSnapshot, PluginState,
    SessionOccupancy, SourceCatalogPage, SourceError,
};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

pub struct SourceHost {
    builtin: Arc<dyn BuiltinModule>,
    plugins: Arc<PluginManager>,
}

impl SourceHost {
    pub fn new(builtin: Arc<dyn BuiltinModule>, plugins: Arc<PluginManager>) -> Self {
        Self { builtin, plugins }
    }

    pub fn dispatch_builtin(
        &self,
        method: &str,
        params: &Value,
        cancellation: &Cancellation,
    ) -> Option<Completion> {
        if self.builtin.routes().contains(&method) {
            self.builtin.dispatch(method, params, cancellation)
        } else {
            None
        }
    }

    pub fn snapshot(&self) -> PluginSnapshot {
        let mut snapshot = self.plugins.snapshot();
        snapshot.plugins.insert(0, self.builtin.descriptor());
        snapshot
    }

    pub fn core_capabilities(&self) -> &'static [&'static str] {
        self.builtin.core_capabilities()
    }

    pub fn dispatch_plugins(
        &self,
        method: &str,
        params: &Value,
        cancellation: &Cancellation,
    ) -> Result<Value, SourceError> {
        cancellation.check()?;
        if method == "plugins.list" {
            if params.as_object().is_none_or(|params| !params.is_empty()) {
                return Err(SourceError::new(
                    "invalid_params",
                    "Plugin list takes no parameters",
                ));
            }
            self.plugins.dispatch(method, params, cancellation)?;
            return serialize(self.snapshot());
        }
        if matches!(method, "plugins.setEnabled" | "plugins.uninstall")
            && params["id"].as_str() == Some(self.builtin.descriptor().id.as_str())
        {
            return Err(SourceError::new(
                "required_plugin",
                "The built-in source is required",
            ));
        }
        let result = self.plugins.dispatch(method, params, cancellation)?;
        if matches!(
            method,
            "plugins.install.commit" | "plugins.setEnabled" | "plugins.uninstall"
        ) {
            serialize(self.snapshot())
        } else {
            Ok(result)
        }
    }

    pub fn catalog_page(
        &self,
        params: &Value,
        cancellation: &Cancellation,
    ) -> Result<SourceCatalogPage, SourceError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Request {
            source_id: PluginId,
            #[serde(default)]
            query: String,
            #[serde(default)]
            cursor: Option<String>,
            #[serde(default = "default_page_limit")]
            limit: u16,
        }

        let request: Request = serde_json::from_value(params.clone()).map_err(|_| {
            SourceError::new("invalid_params", "Catalog request parameters are invalid")
        })?;
        let query = CatalogQuery {
            query: request.query,
            cursor: request.cursor,
            limit: request.limit,
        };
        query
            .validate()
            .map_err(|error| SourceError::new("invalid_params", error.to_string()))?;
        cancellation.check()?;
        if request.source_id == self.builtin.descriptor().id {
            return read_page(
                self.builtin.as_ref(),
                request.source_id,
                &query,
                cancellation,
            );
        }
        let source = self.plugins.source(&request.source_id)?;
        let page = read_page(
            source.as_ref(),
            request.source_id.clone(),
            &query,
            cancellation,
        )?;
        let current = self.plugins.source(&request.source_id)?;
        if !Arc::ptr_eq(&source, &current) || current.generation() != page.generation {
            return Err(stale_source());
        }
        cancellation.check()?;
        Ok(page)
    }

    pub fn reporting_identity(&self) -> Value {
        self.builtin.reporting_identity()
    }

    pub fn session_occupancy(&self) -> SessionOccupancy {
        self.builtin.session_occupancy()
    }

    pub fn settings_changed(&self) {
        self.builtin.settings_changed();
    }

    pub fn shutdown(&self) {
        self.plugins.shutdown();
        self.builtin.shutdown();
    }
}

fn default_page_limit() -> u16 {
    CatalogQuery::default().limit
}

fn read_page<S: CatalogSource + ?Sized>(
    source: &S,
    source_id: PluginId,
    query: &CatalogQuery,
    cancellation: &Cancellation,
) -> Result<SourceCatalogPage, SourceError> {
    let descriptor = source.descriptor();
    if !descriptor.enabled || descriptor.state != PluginState::Ready {
        return Err(SourceError::new(
            "plugin_not_ready",
            "The catalog source is not ready",
        ));
    }
    let generation = source.generation();
    let page = source.catalog_page(query, cancellation)?;
    page.validate().map_err(|_| {
        SourceError::new(
            "plugin_protocol_error",
            "The catalog source returned an invalid page",
        )
    })?;
    if page.items.len() > usize::from(query.limit) {
        return Err(SourceError::new(
            "plugin_protocol_error",
            "The catalog source exceeded the requested page size",
        ));
    }
    cancellation.check()?;
    let current = source.descriptor();
    if source.generation() != generation || !current.enabled || current.state != PluginState::Ready
    {
        return Err(stale_source());
    }
    Ok(SourceCatalogPage::bind(source_id, generation, page))
}

fn stale_source() -> SourceError {
    SourceError::new(
        "stale_source",
        "The catalog source changed during this request",
    )
}

fn serialize(value: impl serde::Serialize) -> Result<Value, SourceError> {
    serde_json::to_value(value).map_err(|_| {
        SourceError::new(
            "plugin_protocol_error",
            "The plugin response could not be encoded",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use contract::{CatalogItem, CatalogPage, Coverage, PluginDescriptor, PluginTrust};
    use opennow_plugin_api::{BUILTIN_GFN_ID, CATALOG_CAPABILITY};
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
    use std::sync::mpsc;

    #[derive(Clone, Copy)]
    enum Behavior {
        Normal,
        ChangeGeneration,
        InvalidPage,
        OversizedPage,
    }

    struct Fixture {
        generation: AtomicU64,
        calls: AtomicUsize,
        behavior: Behavior,
    }

    impl Fixture {
        fn new(behavior: Behavior) -> Self {
            Self {
                generation: AtomicU64::new(1),
                calls: AtomicUsize::new(0),
                behavior,
            }
        }
    }

    impl CatalogSource for Fixture {
        fn descriptor(&self) -> PluginDescriptor {
            PluginDescriptor {
                id: PluginId::new(BUILTIN_GFN_ID).unwrap(),
                name: "Fixture".into(),
                version: "1.0.0".into(),
                publisher: "Fixture".into(),
                description: "Test catalog".into(),
                builtin: true,
                required: true,
                enabled: true,
                state: PluginState::Ready,
                capabilities: vec![CATALOG_CAPABILITY.into()],
                trust: PluginTrust::Builtin,
                last_error: None,
            }
        }

        fn generation(&self) -> u64 {
            self.generation.load(Ordering::Acquire)
        }

        fn catalog_page(
            &self,
            query: &CatalogQuery,
            _: &Cancellation,
        ) -> Result<CatalogPage, SourceError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            if matches!(self.behavior, Behavior::ChangeGeneration) {
                self.generation.fetch_add(1, Ordering::AcqRel);
            }
            let items = if matches!(self.behavior, Behavior::OversizedPage) {
                (0..=query.limit)
                    .map(|id| CatalogItem {
                        id: id.to_string(),
                        title: "Game".into(),
                    })
                    .collect()
            } else {
                vec![CatalogItem {
                    id: "one".into(),
                    title: if matches!(self.behavior, Behavior::InvalidPage) {
                        String::new()
                    } else {
                        "Game".into()
                    },
                }]
            };
            Ok(CatalogPage {
                items,
                next_cursor: None,
                coverage: Coverage::Partial,
            })
        }
    }

    impl BuiltinModule for Fixture {
        fn routes(&self) -> &'static [&'static str] {
            &["settings.set"]
        }

        fn core_capabilities(&self) -> &'static [&'static str] {
            &["fixture.v1"]
        }

        fn dispatch(&self, _: &str, params: &Value, _: &Cancellation) -> Option<Completion> {
            (params["key"] == "region")
                .then(|| Completion::from(Ok((json!({"accepted":true}), None))))
        }

        fn reporting_identity(&self) -> Value {
            Value::Null
        }

        fn session_occupancy(&self) -> SessionOccupancy {
            SessionOccupancy::Idle
        }

        fn settings_changed(&self) {}

        fn shutdown(&self) {}
    }

    fn host(source: Arc<Fixture>) -> (tempfile::TempDir, SourceHost) {
        let directory = tempfile::tempdir().unwrap();
        let (output, _) = mpsc::channel();
        let manager = Arc::new(PluginManager::open(directory.path(), output).unwrap());
        (directory, SourceHost::new(source, manager))
    }

    #[test]
    fn builtins_can_decline_selective_compatibility_routes() {
        let (_directory, host) = host(Arc::new(Fixture::new(Behavior::Normal)));
        let cancellation = Cancellation::default();
        assert!(
            host.dispatch_builtin("settings.set", &json!({"key":"volume"}), &cancellation)
                .is_none()
        );
        assert!(
            host.dispatch_builtin("unknown", &json!({}), &cancellation)
                .is_none()
        );
        assert!(
            host.dispatch_builtin("settings.set", &json!({"key":"region"}), &cancellation)
                .unwrap()
                .result
                .is_ok()
        );
        assert_eq!(host.core_capabilities(), &["fixture.v1"]);
    }

    #[test]
    fn catalog_reads_are_generation_fenced_and_validate_source_results() {
        let query = CatalogQuery {
            limit: 1,
            ..CatalogQuery::default()
        };
        for (behavior, expected) in [
            (Behavior::ChangeGeneration, "stale_source"),
            (Behavior::InvalidPage, "plugin_protocol_error"),
            (Behavior::OversizedPage, "plugin_protocol_error"),
        ] {
            let source = Fixture::new(behavior);
            let result = read_page(
                &source,
                source.descriptor().id,
                &query,
                &Cancellation::default(),
            );
            assert_eq!(result.unwrap_err().code, expected);
        }
        let source = Fixture::new(Behavior::Normal);
        let page = read_page(
            &source,
            source.descriptor().id,
            &query,
            &Cancellation::default(),
        )
        .unwrap();
        assert_eq!(page.source_id.as_str(), BUILTIN_GFN_ID);
        assert_eq!(page.items[0].id.source_id, page.source_id);
        assert_eq!(page.items[0].id.local_id, "one");
        assert_eq!(page.generation, 1);
        assert_eq!(page.coverage, Coverage::Partial);
    }

    #[test]
    fn malformed_catalog_queries_fail_before_invocation() {
        let source = Arc::new(Fixture::new(Behavior::Normal));
        let (_directory, host) = host(Arc::clone(&source));
        for invalid in [
            json!({"sourceId":BUILTIN_GFN_ID,"limit":0}),
            json!({"sourceId":BUILTIN_GFN_ID,"limit":101}),
            json!({"sourceId":BUILTIN_GFN_ID,"query":"x".repeat(513)}),
            json!({"sourceId":BUILTIN_GFN_ID,"cursor":"x".repeat(4097)}),
            json!({"sourceId":BUILTIN_GFN_ID,"settings":{}}),
        ] {
            assert_eq!(
                host.catalog_page(&invalid, &Cancellation::default())
                    .unwrap_err()
                    .code,
                "invalid_params"
            );
        }
        assert_eq!(source.calls.load(Ordering::Relaxed), 0);
        assert!(
            host.catalog_page(
                &json!({"sourceId":BUILTIN_GFN_ID}),
                &Cancellation::default()
            )
            .is_ok()
        );
        assert_eq!(source.calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn builtin_metadata_is_merged_but_cannot_be_mutated() {
        let (_directory, host) = host(Arc::new(Fixture::new(Behavior::Normal)));
        let snapshot = host.snapshot();
        assert_eq!(snapshot.plugins.len(), 1);
        assert_eq!(snapshot.plugins[0].id.as_str(), BUILTIN_GFN_ID);
        for method in ["plugins.setEnabled", "plugins.uninstall"] {
            assert_eq!(
                host.dispatch_plugins(
                    method,
                    &json!({"id":BUILTIN_GFN_ID}),
                    &Cancellation::default()
                )
                .unwrap_err()
                .code,
                "required_plugin"
            );
        }
        assert!(
            host.dispatch_plugins(
                "plugins.list",
                &json!({"unexpected":true}),
                &Cancellation::default()
            )
            .is_err()
        );
    }

    #[test]
    fn source_error_strings_remain_bounded_without_static_leaks() {
        let error = SourceError::new("x".repeat(65), "é".repeat(1025));
        assert_eq!(error.code, "source_error");
        assert_eq!(error.message.len(), 1024);
        assert!(error.message.is_char_boundary(error.message.len()));
    }

    #[test]
    fn cancelled_catalog_requests_do_not_invoke_the_source() {
        let source = Arc::new(Fixture::new(Behavior::Normal));
        let (_directory, host) = host(Arc::clone(&source));
        let requests = Arc::new(crate::requests::Requests::default());
        let permit = requests.admit("catalog", "sources.catalog.page").unwrap();
        requests.cancel("catalog");
        let result = host.catalog_page(&json!({"sourceId":BUILTIN_GFN_ID}), &permit.token);
        assert_eq!(result.unwrap_err().code, "cancelled");
        assert_eq!(source.calls.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn unavailable_manager_does_not_hide_management_errors_or_break_builtin_routes() {
        let directory = tempfile::tempdir().unwrap();
        let (output, _) = mpsc::channel();
        let manager = Arc::new(PluginManager::open(directory.path(), output).unwrap());
        manager.shutdown();
        let host = SourceHost::new(Arc::new(Fixture::new(Behavior::Normal)), manager);
        let cancellation = Cancellation::default();
        assert_eq!(
            host.dispatch_plugins("plugins.list", &json!({}), &cancellation)
                .unwrap_err()
                .code,
            "plugins_unavailable"
        );
        assert!(
            host.catalog_page(&json!({"sourceId":BUILTIN_GFN_ID}), &cancellation)
                .is_ok()
        );
        assert!(
            host.dispatch_builtin("settings.set", &json!({"key":"region"}), &cancellation)
                .unwrap()
                .result
                .is_ok()
        );
    }

    #[test]
    fn corrupt_registry_preserves_bytes_and_builtin_routes_remain_available() {
        let directory = tempfile::tempdir().unwrap();
        let plugins = directory.path().join("plugins");
        std::fs::create_dir(&plugins).unwrap();
        let registry = plugins.join("registry.json");
        let corrupt = b"{invalid registry: preserve these bytes}";
        std::fs::write(&registry, corrupt).unwrap();
        let (output, events) = mpsc::channel();
        let manager = Arc::new(PluginManager::open(directory.path(), output).unwrap());
        let host = SourceHost::new(Arc::new(Fixture::new(Behavior::Normal)), manager);
        let cancellation = Cancellation::default();

        for method in ["plugins.list", "plugins.install.cancel"] {
            let error = host
                .dispatch_plugins(method, &json!({}), &cancellation)
                .unwrap_err();
            assert_eq!(error.code, "plugin_storage_error");
            assert!(!error.message.contains("preserve these bytes"));
        }
        assert_eq!(host.snapshot().plugins[0].id.as_str(), BUILTIN_GFN_ID);
        assert!(
            host.catalog_page(&json!({"sourceId":BUILTIN_GFN_ID}), &cancellation)
                .is_ok()
        );
        assert!(
            host.dispatch_builtin("settings.set", &json!({"key":"region"}), &cancellation)
                .unwrap()
                .result
                .is_ok()
        );
        host.shutdown();
        assert_eq!(std::fs::read(&registry).unwrap(), corrupt);
        assert!(events.try_recv().is_err());
    }
}
