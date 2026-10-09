use super::{GfnModule, cloudmatch, queue_servers, service};
use crate::diagnostics;
use crate::sources::contract::AllocationDisposition;
use crate::streamer::StreamerService;
use serde_json::{Value, json};

pub(super) type CompatibilityResult =
    Result<(Value, Option<(&'static str, Value)>), (String, String)>;

pub(super) const ROUTES: &[&str] = &[
    "social.capabilities.get",
    "auth.providers.list",
    "auth.device.start",
    "auth.device.poll",
    "auth.device.complete",
    "auth.device.cancel",
    "auth.session.get",
    "auth.logout",
    "auth.accounts.logoutAll",
    "auth.accounts.list",
    "auth.accounts.switch",
    "auth.accounts.remove",
    "auth.pin.status",
    "auth.pin.set",
    "auth.pin.clear",
    "auth.pin.verify",
    "catalog.public.list",
    "catalog.library.list",
    "catalog.game.get",
    "catalog.launch.inspect",
    "catalog.launch.store.inspect",
    "catalog.favorites.list",
    "catalog.favorites.add",
    "catalog.favorites.remove",
    "catalog.ownership.add",
    "catalog.ownership.remove",
    "catalog.ownership.select",
    "catalog.definitions.get",
    "catalog.languages.get",
    "catalog.store.local",
    "catalog.store.list",
    "catalog.store.presentation",
    "network.regions.list",
    "queue.servers.list",
    "account.subscription.get",
    "account.connections.list",
    "account.connections.sync",
    "account.connections.unlink",
    "account.connections.link.start",
    "account.connections.link.poll",
    "account.connections.sync.status",
    "account.connections.sync.cancel",
    "account.storage.locations",
    "account.storage.reset",
    "session.create",
    "session.poll",
    "session.stop",
    "session.active.get",
    "session.remote.list",
    "session.claim",
    "session.ad.report",
    "streamer.prepare",
    "cache.delete",
    "queue.status.get",
    "queue.serverMapping.get",
    "communityProxy.provision",
    "settings.set",
];

impl GfnModule {
    pub(super) fn execute_compatibility(
        &self,
        method: &str,
        params: &Value,
        disposition: &mut AllocationDisposition,
    ) -> CompatibilityResult {
        match method {
            "settings.set" => {
                let provider = params["providerIdpId"].as_str().unwrap_or("");
                let value = params.get("value").cloned().ok_or_else(|| {
                    (
                        "invalid_params".to_owned(),
                        "settings.set requires a value".to_owned(),
                    )
                })?;
                let event = self.service.with_region_provider(provider, || {
                let mut settings = self.settings.lock().expect("settings poisoned");
                let applied = settings.set_provider_region(provider, value).map_err(|message| crate::service_error::ServiceError { code: "invalid_setting", message })?;
                Ok(json!({"key":"region","value":applied,"changes":{
                    "regionProviderIdpId":provider,"providerRegions":settings.all()["providerRegions"]
                }}))
            }).map_err(service_error)?;
                Ok((event.clone(), Some(("settings.changed", event))))
            }
            "social.capabilities.get" => Ok((
                json!({
                    "friendsAvailable":false,
                    "presenceAvailable":false,
                    "invitesAvailable":false,
                    "localControllerJoin":true,
                    "reason":"NVIDIA does not expose the GeForce NOW friends, presence, or invitation service to third-party clients. OpenNOW will not display invented contacts or claim invitations were sent."
                }),
                None,
            )),
            "auth.providers.list" => self
                .service
                .providers()
                .map(|value| (value, None))
                .map_err(service_error),
            "auth.device.start" => self
                .service
                .start_device_login(params)
                .map(|value| (value, None))
                .map_err(service_error),
            "auth.device.poll" => self
                .service
                .poll_device_login(params)
                .map(|value| (value, None))
                .map_err(service_error),
            "auth.device.complete" => self
                .service
                .complete_device_login(params)
                .map(|value| (value.clone(), Some(("auth.session.changed", value))))
                .map_err(service_error),
            "auth.device.cancel" => self
                .service
                .cancel_device_login(params)
                .map(|value| (value, None))
                .map_err(service_error),
            "auth.session.get" => self
                .service
                .session()
                .map(|value| (value, None))
                .map_err(service_error),
            "auth.logout" => {
                let value = self.service.logout().map_err(service_error)?;
                Ok((value.clone(), Some(("auth.session.changed", value))))
            }
            "auth.accounts.logoutAll" => {
                let value = self.service.logout_all().map_err(service_error)?;
                Ok((value.clone(), Some(("auth.session.changed", value))))
            }
            "auth.accounts.list" => self
                .service
                .saved_accounts()
                .map(|value| (value, None))
                .map_err(service_error),
            "auth.accounts.switch" => self
                .service
                .switch_account(params)
                .map(|value| (value.clone(), Some(("auth.session.changed", value))))
                .map_err(service_error),
            "auth.accounts.remove" => self
                .service
                .remove_account(params)
                .map(|value| (value.clone(), Some(("auth.session.changed", value))))
                .map_err(service_error),
            "auth.pin.status" => self
                .service
                .pin_status(params)
                .map(|value| (value, None))
                .map_err(service_error),
            "auth.pin.set" => self
                .service
                .set_pin(params)
                .map(|value| (value, None))
                .map_err(service_error),
            "auth.pin.clear" => self
                .service
                .clear_pin(params)
                .map(|value| (value, None))
                .map_err(service_error),
            "auth.pin.verify" => self
                .service
                .verify_pin(params)
                .map(|value| (value, None))
                .map_err(service_error),
            "catalog.public.list" => {
                let settings = self.settings.lock().expect("settings poisoned").all();
                self.service
                    .public_catalog(params, &settings)
                    .map(|value| (value, None))
                    .map_err(service_error)
            }
            "catalog.library.list" => {
                let settings = self.settings.lock().expect("settings poisoned").all();
                self.service
                    .library_catalog(params, &settings)
                    .map(|value| (value, None))
                    .map_err(service_error)
            }
            "catalog.game.get" => {
                let settings = self.settings.lock().expect("settings poisoned").all();
                self.service
                    .catalog_game(params, &settings)
                    .map(|value| (value, None))
                    .map_err(service_error)
            }
            "catalog.launch.inspect" => {
                let settings = self.settings.lock().expect("settings poisoned").all();
                self.service
                    .catalog_launch_inspect(params, &settings)
                    .map(|value| (value, None))
                    .map_err(service_error)
            }
            "catalog.launch.store.inspect" => {
                let settings = self.settings.lock().expect("settings poisoned").all();
                self.service
                    .store_launch_inspect(params, None, &settings)
                    .map(|value| (value, None))
                    .map_err(service_error)
            }
            "catalog.favorites.list" => {
                let settings = self.settings.lock().expect("settings poisoned").all();
                self.service
                    .catalog_favorites(&settings)
                    .map(|value| (value, None))
                    .map_err(service_error)
            }
            "catalog.favorites.add"
            | "catalog.favorites.remove"
            | "catalog.ownership.add"
            | "catalog.ownership.remove"
            | "catalog.ownership.select" => {
                let settings = self.settings.lock().expect("settings poisoned").all();
                self.service
                    .catalog_mutate(method, params, &settings)
                    .map(|value| (value, None))
                    .map_err(service_error)
            }
            "catalog.definitions.get" => {
                let settings = self.settings.lock().expect("settings poisoned").all();
                self.service
                    .catalog_definitions(params, &settings)
                    .map(|value| (value, None))
                    .map_err(service_error)
            }
            "catalog.languages.get" => {
                let settings = self.settings.lock().expect("settings poisoned").all();
                self.service
                    .catalog_languages(params, &settings)
                    .map(|value| (value, None))
                    .map_err(service_error)
            }
            "catalog.store.local" => {
                let settings = self.settings.lock().expect("settings poisoned").all();
                self.service
                    .store_local_catalog(params, &settings)
                    .map(|value| (value, None))
                    .map_err(service_error)
            }
            "catalog.store.list" => {
                let settings = self.settings.lock().expect("settings poisoned").all();
                self.service
                    .store_catalog(params, &settings)
                    .map(|value| (value, None))
                    .map_err(service_error)
            }
            "catalog.store.presentation" => {
                let settings = self.settings.lock().expect("settings poisoned").all();
                self.service
                    .store_presentation(params, &settings)
                    .map(|value| (value, None))
                    .map_err(service_error)
            }
            "network.regions.list" => {
                let settings = self.settings.lock().expect("settings poisoned").all();
                self.service
                    .regions(&settings)
                    .map(|value| (value, None))
                    .map_err(service_error)
            }
            "queue.servers.list" => queue_servers::list()
                .map(|value| (value, None))
                .map_err(service_error),
            "account.subscription.get" => {
                let settings = self.settings.lock().expect("settings poisoned").all();
                self.service
                    .subscription(&settings)
                    .map(|value| (value, None))
                    .map_err(service_error)
            }
            "account.connections.list" => {
                let settings = self.settings.lock().expect("settings poisoned").all();
                self.service
                    .account_connections(&settings)
                    .map(|value| (value, None))
                    .map_err(service_error)
            }
            "account.connections.sync" => {
                let settings = self.settings.lock().expect("settings poisoned").all();
                self.service
                    .sync_account_connection(params, &settings)
                    .map(|value| (value, None))
                    .map_err(service_error)
            }
            "account.connections.unlink" => {
                let settings = self.settings.lock().expect("settings poisoned").all();
                self.service
                    .unlink_account_connection(params, &settings)
                    .map(|value| (value, None))
                    .map_err(service_error)
            }
            "account.connections.link.start" => {
                let settings = self.settings.lock().expect("settings poisoned").all();
                self.service
                    .start_account_link(params, &settings)
                    .map(|value| (value, None))
                    .map_err(service_error)
            }
            "account.connections.link.poll" => {
                let settings = self.settings.lock().expect("settings poisoned").all();
                self.service
                    .poll_account_link(params, &settings)
                    .map(|value| (value, None))
                    .map_err(service_error)
            }
            "account.connections.sync.status" => {
                let settings = self.settings.lock().expect("settings poisoned").all();
                self.service
                    .account_sync_status(params, &settings)
                    .map(|value| (value, None))
                    .map_err(service_error)
            }
            "account.connections.sync.cancel" => {
                let settings = self.settings.lock().expect("settings poisoned").all();
                self.service
                    .account_sync_status(
                        &json!({"operationId":params["operationId"],"cancelObservation":true}),
                        &settings,
                    )
                    .map(|value| (value, None))
                    .map_err(service_error)
            }
            "account.storage.locations" => self
                .service
                .persistent_storage_locations(params)
                .map(|value| (value, None))
                .map_err(service_error),
            "account.storage.reset" => self
                .service
                .reset_persistent_storage(params)
                .map(|value| (value, None))
                .map_err(service_error),
            "session.create" => {
                let settings = self.settings.lock().expect("settings poisoned").all();
                let (auth, _) = self
                    .service
                    .authenticated_snapshot(service::TokenPurpose::ServiceId, false)
                    .map_err(service_error)?;
                let settings = cloudmatch::allocation_settings(&settings, &auth);
                let settings = if !params["runtimeCapabilities"].is_null() {
                    StreamerService::embedded_session_settings(
                        &settings,
                        &params["runtimeCapabilities"],
                    )
                    .map_err(streamer_error)?
                } else {
                    self.streamer
                        .validate_codec(&settings)
                        .map_err(streamer_error)?;
                    settings
                };
                self.service
                    .create_session_tracked(params, &settings, disposition)
                    .map(|value| (value.clone(), Some(("session.changed", value))))
                    .map_err(service_error)
            }
            "session.poll" => self
                .service
                .poll_session(params)
                .map(|value| {
                    (
                        value.clone(),
                        (params["recoveryMode"] != true).then_some(("session.changed", value)),
                    )
                })
                .map_err(service_error),
            "session.stop" => {
                let settings = self.settings.lock().expect("settings poisoned").all();
                self.service
                    .stop_session(params, &settings)
                    .map(|value| (value.clone(), Some(("session.changed", value))))
                    .map_err(service_error)
            }
            "session.active.get" => {
                let settings = self.settings.lock().expect("settings poisoned").all();
                self.service
                    .reconcile_active_session(params, &settings)
                    .map(|value| (value, None))
                    .map_err(service_error)
            }
            "session.remote.list" => {
                let settings = self.settings.lock().expect("settings poisoned").all();
                self.service
                    .remote_sessions(params, &settings)
                    .map(|value| (value, None))
                    .map_err(service_error)
            }
            "session.claim" => {
                let settings = self.settings.lock().expect("settings poisoned").all();
                self.service
                    .claim_session(params, &settings)
                    .map(|value| (value.clone(), Some(("session.changed", value))))
                    .map_err(service_error)
            }
            "session.ad.report" => self
                .service
                .report_session_ad(params)
                .map(|value| (value.clone(), Some(("session.changed", value))))
                .map_err(service_error),
            "streamer.prepare" => {
                let settings = self.settings.lock().expect("settings poisoned").all();
                if params["session"]["transportMode"] == "webrtc" {
                    self.diagnostics.record(
                        "streamer",
                        "prepare_endpoints",
                        diagnostics::stream_endpoint_evidence(&params["session"]).to_string(),
                    );
                }
                self.service
                    .prepare_owned_stream(params, |owned| {
                        self.streamer
                            .prepare_embedded(owned, &settings)
                            .map_err(|error| crate::service_error::ServiceError {
                                code: error.code,
                                message: error.message,
                            })
                    })
                    .inspect(|prepared| {
                        self.diagnostics.record(
                            "streamer",
                            "prepare_profile",
                            diagnostics::stream_profile_evidence(&prepared["context"]["session"])
                                .to_string(),
                        );
                    })
                    .inspect_err(|error| {
                        self.diagnostics.record(
                            "streamer",
                            "prepare_profile",
                            diagnostics::stream_profile_evidence(&params["session"]).to_string(),
                        );
                        self.diagnostics.record(
                            "streamer",
                            "prepare_rejected",
                            diagnostics::runtime_failure_reason(&error.message),
                        );
                    })
                    .map(|value| (value, None))
                    .map_err(service_error)
            }
            "cache.delete" => Ok((
                self.service.clear_cache(),
                Some(("cache.changed", json!({}))),
            )),
            "queue.status.get" => self
                .community
                .queue()
                .map(|value| (value, None))
                .map_err(|message| ("queue_fetch_failed".to_owned(), message)),
            "queue.serverMapping.get" => self
                .community
                .server_mapping()
                .map(|value| (value, None))
                .map_err(|message| ("server_mapping_fetch_failed".to_owned(), message)),
            "communityProxy.provision" => self
                .community
                .provision_proxy(self.service.device_id())
                .map(|value| (value, None))
                .map_err(|message| ("community_proxy_failed".to_owned(), message)),
            _ => Err((
                "method_not_found".into(),
                "Unknown built-in operation".into(),
            )),
        }
    }
}

fn service_error(error: crate::service_error::ServiceError) -> (String, String) {
    (error.code.to_owned(), error.message)
}
fn streamer_error(error: crate::streamer::StreamerError) -> (String, String) {
    (error.code.to_owned(), error.message)
}
