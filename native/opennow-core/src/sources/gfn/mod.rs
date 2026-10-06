mod account_connections;
mod catalog_types;
mod cloudmatch;
mod community;
mod compat;
mod console_profiles;
mod credential_vault;
mod device_identity;
mod network_test;
mod persistent_storage;
mod push;
mod push_registry;
mod queue_servers;
mod server_vpc_cache;
mod service;
pub(crate) mod session_language;
mod store_cache;
mod store_catalog_page;
mod store_index;
mod store_requests;

use super::contract::{
    AllocationReceipt, BuiltinModule, CatalogItem, CatalogPage, CatalogQuery, CatalogSource,
    Completion, Coverage, PluginDescriptor, PluginId, PluginState, PluginTrust, ReceiptOutcome,
    ReportingEffect, SessionOccupancy, SourceError,
};
use crate::diagnostics::DiagnosticsService;
use crate::requests::{self, Cancellation};
use crate::settings::SettingsStore;
use crate::streamer::StreamerService;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

pub(crate) struct GfnModule {
    service: Arc<service::GfnService>,
    settings: Arc<Mutex<SettingsStore>>,
    streamer: Arc<StreamerService>,
    diagnostics: Arc<DiagnosticsService>,
    push: Mutex<push_registry::PushRegistry>,
    community: community::CommunityService,
    closing: Arc<AtomicBool>,
}

impl GfnModule {
    pub(crate) fn open(
        data_dir: PathBuf,
        output: Sender<Value>,
        settings: Arc<Mutex<SettingsStore>>,
        streamer: Arc<StreamerService>,
        diagnostics: Arc<DiagnosticsService>,
    ) -> Result<Self, String> {
        let service = Arc::new(service::GfnService::new(data_dir.clone())?);
        let push = push_registry::PushRegistry::new(Arc::clone(&service), output, data_dir);
        let closing = push.core_exit_signal();
        let module = Self {
            service,
            settings,
            streamer,
            diagnostics,
            push: Mutex::new(push),
            community: community::CommunityService::new()?,
            closing,
        };
        module.settings_changed();
        Ok(module)
    }
}

struct SessionReceipt {
    service: Arc<service::GfnService>,
    session_id: String,
}

impl AllocationReceipt for SessionReceipt {
    fn settle(self: Box<Self>, accepted: bool) -> ReceiptOutcome {
        let result = requests::scope(Cancellation::default(), || {
            self.service
                .finish_session_create(&self.session_id, accepted)
        });
        let required_events = result.as_ref().err().map(|error| {
            ("session.cleanup.pending", json!({"sessionId":self.session_id,
                "code":error.code,"message":"The cancelled cloud session could not be closed. End it before starting another game."}))
        }).into_iter().collect();
        ReceiptOutcome {
            result: result.map_err(SourceError::from),
            required_events,
        }
    }
}

impl CatalogSource for GfnModule {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: PluginId::new(opennow_plugin_api::BUILTIN_GFN_ID)
                .expect("built-in source identity"),
            name: "GeForce NOW".into(),
            version: crate::version::APPLICATION_VERSION.into(),
            publisher: "OpenNOW".into(),
            description: "Built-in GeForce NOW accounts, catalog and streaming".into(),
            builtin: true,
            required: true,
            enabled: true,
            state: PluginState::Ready,
            capabilities: vec![opennow_plugin_api::CATALOG_CAPABILITY.into()],
            trust: PluginTrust::Builtin,
            last_error: None,
        }
    }

    fn generation(&self) -> u64 {
        self.service.auth_generation()
    }

    fn catalog_page(
        &self,
        query: &CatalogQuery,
        cancellation: &Cancellation,
    ) -> Result<CatalogPage, SourceError> {
        query
            .validate()
            .map_err(|error| SourceError::new("invalid_params", error.to_string()))?;
        if query.cursor.is_some() {
            return Err(SourceError::new(
                "unsupported_cursor",
                "This catalog preview does not support continuation cursors",
            ));
        }
        let settings = self.settings.lock().expect("settings poisoned").all();
        let response = requests::scope(cancellation.clone(), || {
            cancellation.check()?;
            self.service.public_catalog(
                &json!({"searchQuery":query.query,"limit":query.limit}),
                &settings,
            )
        })?;
        cancellation.check()?;
        let items = response["games"]
            .as_array()
            .ok_or_else(|| {
                SourceError::new("catalog_invalid", "The catalog returned an invalid page")
            })?
            .iter()
            .map(|item| {
                Ok(CatalogItem {
                    id: item["id"]
                        .as_str()
                        .ok_or_else(|| {
                            SourceError::new("catalog_invalid", "A catalog item has no identity")
                        })?
                        .into(),
                    title: item["title"]
                        .as_str()
                        .ok_or_else(|| {
                            SourceError::new("catalog_invalid", "A catalog item has no title")
                        })?
                        .into(),
                })
            })
            .collect::<Result<Vec<_>, SourceError>>()?;
        let page = CatalogPage {
            items,
            next_cursor: None,
            coverage: Coverage::Unknown,
        };
        page.validate()
            .map_err(|error| SourceError::new("catalog_invalid", error.to_string()))?;
        Ok(page)
    }
}

impl BuiltinModule for GfnModule {
    fn core_capabilities(&self) -> &'static [&'static str] {
        &[
            "gfn.deviceAuth",
            "gfn.providers",
            "gfn.publicCatalog",
            "catalog.storePages.v1",
            "catalog.libraryPages.v1",
            "catalog.metadata.v1",
            "account.syncObservation.v1",
            "account.pushInvalidation.v1",
            "catalog.languages.v1",
            "queue.servers.v1",
            "catalog.storeLocal.v1",
            "gfn.accountLibrary",
            "gfn.regions",
            "gfn.subscription",
            "gfn.cloudmatch",
            "sessionProxy",
            "osCredentialStore",
            "electronAccountMigration",
            "social.capabilitySurface",
        ]
    }

    fn routes(&self) -> &'static [&'static str] {
        compat::ROUTES
    }

    fn dispatch(
        &self,
        method: &str,
        params: &Value,
        cancellation: &Cancellation,
    ) -> Option<Completion> {
        if !compat::ROUTES.contains(&method)
            || (method == "settings.set" && params["key"] != "region")
        {
            return None;
        }
        let mut completion = Completion::from(requests::scope(cancellation.clone(), || {
            cancellation
                .check()
                .map_err(|error| (error.code.to_owned(), error.message))?;
            self.execute_compatibility(method, params)
        }));
        match (method, &completion.result) {
            ("session.create", _) => completion.reporting.push(ReportingEffect::LaunchRequested {
                params: json!({"appId":params["appId"], "title":params["title"], "store":params["store"], "zone":params["zone"]}),
            }),
            ("session.stop", Ok(_)) => completion.reporting.push(ReportingEffect::SessionStopped),
            ("streamer.prepare", _) => completion.reporting.push(ReportingEffect::RuntimeObserved { capabilities: params["runtimeCapabilities"].clone() }),
            ("auth.device.complete", Ok(_)) => completion.reporting.push(ReportingEffect::SignedIn { restored: false }),
            ("auth.session.get" | "auth.accounts.switch", Ok(_)) => completion.reporting.push(ReportingEffect::SignedIn { restored: true }),
            ("auth.logout" | "auth.accounts.logoutAll", Ok(_)) => completion.reporting.push(ReportingEffect::SignedOut),
            _ => {}
        }
        if let Err((code, message)) = &completion.result {
            completion.reporting.push(ReportingEffect::RpcFailure {
                method: method.into(),
                code: code.clone(),
                message: message.clone(),
            });
            if method == "session.create" && code == "session_cleanup_pending" {
                completion.required_events.push((
                    "session.cleanup.pending",
                    json!({"code":code,"message":message}),
                ));
            }
        }
        if method == "session.create"
            && let Ok((value, event)) = &mut completion.result
            && let Some(session_id) = value["session"]["sessionId"].as_str()
        {
            *event = None;
            completion.receipt = Some(Box::new(SessionReceipt {
                service: Arc::clone(&self.service),
                session_id: session_id.into(),
            }));
        }
        if matches!(
            method,
            "auth.device.complete"
                | "auth.logout"
                | "auth.accounts.logoutAll"
                | "auth.accounts.switch"
                | "auth.accounts.remove"
                | "settings.set"
        ) {
            self.settings_changed();
        }
        Some(completion)
    }

    fn reporting_identity(&self) -> Value {
        self.service.bug_report_identity()
    }

    fn session_occupancy(&self) -> SessionOccupancy {
        match self.service.occupied() {
            Some(false) => SessionOccupancy::Idle,
            Some(true) => SessionOccupancy::InUse,
            None => SessionOccupancy::Unknown,
        }
    }

    fn settings_changed(&self) {
        let mut push = self.push.lock().expect("push registry poisoned");
        if !self.closing.load(Ordering::Acquire) {
            let _ = push.reconcile();
        }
        if self.closing.load(Ordering::Acquire) {
            push.stop_for_core_exit();
        }
    }

    fn shutdown(&self) {
        self.closing.store(true, Ordering::Release);
        if let Ok(mut push) = self.push.try_lock() {
            push.stop_for_core_exit();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(path: &std::path::Path, public_catalog: &str) -> GfnModule {
        let mut endpoints = service::Endpoints::default();
        endpoints.public_catalog = public_catalog.into();
        let service = Arc::new(service::GfnService::with_client(
            reqwest::blocking::Client::builder()
                .no_proxy()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap(),
            endpoints,
            path.into(),
        ));
        let (output, _) = std::sync::mpsc::channel();
        let push = push_registry::PushRegistry::new(Arc::clone(&service), output, path.into());
        let closing = push.core_exit_signal();
        GfnModule {
            push: Mutex::new(push),
            service,
            settings: Arc::new(Mutex::new(SettingsStore::load(Some(path.into())).unwrap())),
            streamer: Arc::new(StreamerService::new()),
            diagnostics: Arc::new(DiagnosticsService::new(path).unwrap()),
            community: community::CommunityService::new().unwrap(),
            closing,
        }
    }

    #[test]
    fn builtin_catalog_executes_the_real_service_and_does_not_invent_coverage() {
        let (url, worker) = service::tests::mock_requests(
            vec![(
                200,
                json!([
                    {"id":"game-a","title":"Alpha","status":"AVAILABLE"},
                    {"id":"game-b","title":"Beta","status":"AVAILABLE"}
                ]),
            )],
            |_, request| assert!(request.starts_with("GET / ")),
        );
        let directory = tempfile::tempdir().unwrap();
        let module = fixture(directory.path(), &url);
        let page = module
            .catalog_page(
                &CatalogQuery {
                    query: "Alpha".into(),
                    cursor: None,
                    limit: 1,
                },
                &Cancellation::default(),
            )
            .unwrap();
        assert_eq!(
            page.items,
            vec![CatalogItem {
                id: "game-a".into(),
                title: "Alpha".into()
            }]
        );
        assert_eq!(page.coverage, Coverage::Unknown);
        assert_eq!(page.next_cursor, None);
        let error = module
            .catalog_page(
                &CatalogQuery {
                    cursor: Some("unsupported".into()),
                    ..CatalogQuery::default()
                },
                &Cancellation::default(),
            )
            .unwrap_err();
        assert_eq!(error.code, "unsupported_cursor");
        worker.join().unwrap();
    }

    #[test]
    fn only_region_settings_are_owned_by_the_compatibility_lane() {
        let directory = tempfile::tempdir().unwrap();
        let module = fixture(directory.path(), "http://127.0.0.1:1");
        let before = module.settings.lock().unwrap().all();
        assert!(
            module
                .dispatch(
                    "settings.set",
                    &json!({"key":"fps","value":120}),
                    &Cancellation::default()
                )
                .is_none()
        );
        let rejected = module
            .dispatch(
                "settings.set",
                &json!({"key":"region","value":"invalid","providerIdpId":"foreign"}),
                &Cancellation::default(),
            )
            .unwrap();
        assert_eq!(rejected.result.unwrap_err().0, "stale_account");
        assert_eq!(module.settings.lock().unwrap().all(), before);
    }

    #[test]
    fn legacy_device_identity_stays_at_the_original_profile_path() {
        let directory = tempfile::tempdir().unwrap();
        let identity = "a".repeat(64);
        std::fs::write(
            directory.path().join("device-identity.json"),
            json!({"version":1,"id":identity}).to_string(),
        )
        .unwrap();
        let module = fixture(directory.path(), "http://127.0.0.1:1");
        assert_eq!(module.service.device_id(), identity);
        assert_eq!(
            module.descriptor().id.as_str(),
            opennow_plugin_api::BUILTIN_GFN_ID
        );
        assert!(!directory.path().join("providers").exists());
        assert_eq!(module.session_occupancy(), SessionOccupancy::Idle);
    }

    #[test]
    fn core_exit_does_not_wait_for_a_contended_push_registry() {
        let directory = tempfile::tempdir().unwrap();
        let module = fixture(directory.path(), "http://127.0.0.1:1");
        let held = module.push.lock().unwrap();
        assert!(Arc::ptr_eq(&module.closing, &held.core_exit_signal()));
        let started = std::time::Instant::now();
        module.shutdown();
        assert!(started.elapsed() < std::time::Duration::from_millis(250));
        assert!(held.core_exit_signal().load(Ordering::Acquire));
        drop(held);
        module.settings_changed();
        module.shutdown();
        assert_eq!(module.session_occupancy(), SessionOccupancy::Idle);
    }

    #[test]
    fn failed_create_keeps_reporting_effect_without_a_receipt_or_change_event() {
        let directory = tempfile::tempdir().unwrap();
        let module = fixture(directory.path(), "http://127.0.0.1:1");
        let requests = Arc::new(requests::Requests::default());
        let permit = requests.admit("cancelled", "session.create").unwrap();
        requests.cancel("cancelled");
        let completion = module
            .dispatch("session.create", &json!({"appId":"fixture"}), &permit.token)
            .unwrap();
        assert_eq!(completion.result.unwrap_err().0, "cancelled");
        assert!(completion.receipt.is_none());
        assert!(completion.required_events.is_empty());
        assert!(matches!(
            completion.reporting.first(),
            Some(ReportingEffect::LaunchRequested { .. })
        ));
    }
}
