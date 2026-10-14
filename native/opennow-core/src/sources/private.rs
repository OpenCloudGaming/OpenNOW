use super::contract::{
    AllocationReceipt, Completion, NativePreparation, ProviderContext, ReceiptOutcome, SourceError,
};
use super::journal::{MediaPin, SessionJournal};
use super::{SourceHost, invalid_params, preferences, session_manager};
use crate::requests::{self, Cancellation};
use opennow_media_protocol::lease::{
    HostBoundPreparedLease, LocalMediaPolicy, NativeMediaStatus, PackageReference, PreparedMedia,
};
use opennow_plugin_api::{PluginId, media::NativeOffer, provider as api};
use opennow_plugin_package::VerifiedPackage;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

struct BrowserGrant {
    source: PluginId,
    attempt: String,
    generation: u64,
    expires: u64,
    url: api::SecretString,
}

#[derive(Default)]
pub(super) struct PrivateState {
    browsers: Mutex<BTreeMap<String, BrowserGrant>>,
    packages: Mutex<BTreeMap<String, VerifiedPackage>>,
}

impl SourceHost {
    pub fn dispatch_private(
        &self,
        method: &str,
        params: &Value,
        cancellation: &Cancellation,
    ) -> Option<Completion> {
        if method == "sources.session.create" {
            return Some(self.create_intent(params, cancellation));
        }
        let result = match method {
            "streamer.source.policy" => self
                .local_policy()
                .and_then(|policy| value(policy).map(|policy| json!({"localPolicy":policy}))),
            "streamer.source.prepare" => return Some(self.prepare_source(params, cancellation)),
            "streamer.source.release" => self.release_source(params),
            "streamer.source.reconcile" => self.reconcile_native(params),
            "streamer.source.observe" => self
                .sessions
                .journal
                .media_revision()
                .map(|revision| json!({"mediaRevision":revision})),
            "sources.auth.open" => self.open_browser(params),
            _ => return None,
        };
        Some(Completion::from(
            result.map(|value| (value, None)).map_err(Into::into),
        ))
    }

    fn local_policy(&self) -> Result<LocalMediaPolicy, SourceError> {
        let settings = self.settings.lock().expect("settings poisoned").all();
        let policy = LocalMediaPolicy {
            video_backend: settings["nativeVideoBackend"]
                .as_str()
                .unwrap_or("auto")
                .into(),
            audio_output_device: settings["audioOutputDevice"].as_str().unwrap_or("").into(),
            max_bitrate_mbps: settings["maxBitrateMbps"].as_f64().unwrap_or(75.0),
            replay_buffer_enabled: settings["replayBufferEnabled"] == true,
            replay_buffer_seconds: settings["replayBufferSeconds"]
                .as_u64()
                .unwrap_or(30)
                .try_into()
                .map_err(|_| invalid_params())?,
            replay_buffer_memory_mi_b: settings["replayBufferMemoryMiB"]
                .as_u64()
                .unwrap_or(256)
                .try_into()
                .map_err(|_| invalid_params())?,
            shortcuts: [
                ("toggleStats", "shortcutToggleStats"),
                ("togglePointerLock", "shortcutTogglePointerLock"),
                ("toggleFullscreen", "shortcutToggleFullscreen"),
                ("stopStream", "shortcutStopStream"),
                ("toggleAntiAfk", "shortcutToggleAntiAfk"),
                ("toggleMicrophone", "shortcutToggleMicrophone"),
                ("screenshot", "shortcutScreenshot"),
                ("toggleRecording", "shortcutToggleRecording"),
                ("saveClip", "shortcutSaveClip"),
            ]
            .into_iter()
            .map(|(action, key)| (action.into(), settings[key].as_str().unwrap_or("").into()))
            .collect(),
        };
        policy.validate().map_err(|_| invalid_params())?;
        Ok(policy)
    }

    fn create_intent(&self, params: &Value, cancellation: &Cancellation) -> Completion {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Intent {
            scope: Option<api::AccountScope>,
            target: api::LaunchTarget,
            catalog_revision: api::Text<256>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Request {
            source_id: PluginId,
            request: Intent,
            offer: NativeOffer,
            runtime_capabilities: Option<Value>,
        }
        let create = || -> Result<Completion, SourceError> {
            let request: Request =
                serde_json::from_value(params.clone()).map_err(|_| invalid_params())?;
            request.offer.validate().map_err(|_| invalid_params())?;
            if request.offer.expires_at_ms <= now_ms() {
                return Err(expired_offer());
            }
            let provider = self.provider(&request.source_id)?;
            if !provider
                .provider_capabilities()
                .contains(&api::Capability::Sessions)
            {
                return Err(unsupported());
            }
            let settings = self.effective_settings(
                &request.source_id,
                request.request.scope.as_ref().map(|scope| &scope.account),
            )?;
            let request_body = api::CreateSession {
                scope: request.request.scope,
                operation: session_manager::new_operation(),
                target: request.request.target,
                catalog_revision: request.request.catalog_revision,
                settings_revision: self.preferences.generation(),
                preferences: preferences::stream_preferences(&settings)?,
                offer: request.offer,
            };
            let body = api::ProviderRequest::SessionCreate(request_body);
            if !request.source_id.is_builtin() {
                body.validate().map_err(|_| {
                    SourceError::new(
                        "profile_unsupported",
                        "The requested source profile is not supported by this device",
                    )
                })?;
            }
            if self.sessions.occupancy() == super::contract::SessionOccupancy::Idle
                && self.builtin.session_occupancy() != super::contract::SessionOccupancy::Idle
            {
                return Err(SourceError::new(
                    "session_in_use",
                    "Resolve the existing session before starting another",
                ));
            }
            let mut completion = self.sessions.execute(
                request.source_id.clone(),
                provider,
                body,
                &ProviderContext {
                    cancellation,
                    runtime_capabilities: request.runtime_capabilities.as_ref(),
                    gfn_settings: Some(&settings),
                },
            );
            if let Err(error) = self.project_public(&request.source_id, &mut completion) {
                completion.result = Err(error.into());
            }
            Ok(completion)
        };
        create().unwrap_or_else(failed)
    }

    fn prepare_source(&self, params: &Value, cancellation: &Cancellation) -> Completion {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Request {
            session_handle: String,
            offer: NativeOffer,
            runtime_capabilities: Option<Value>,
        }
        let prepare = || -> Result<Completion, SourceError> {
            let request: Request =
                serde_json::from_value(params.clone()).map_err(|_| invalid_params())?;
            request.offer.validate().map_err(|_| invalid_params())?;
            if request.offer.expires_at_ms <= now_ms() {
                return Err(expired_offer());
            }
            let record = self
                .sessions
                .journal
                .snapshot()?
                .filter(|record| record.operation.as_str() == request.session_handle)
                .ok_or_else(ownership)?;
            let session = record.session.clone().ok_or_else(ownership)?;
            let query = api::PrepareSession {
                session: session.clone(),
                offer: request.offer.clone(),
            };
            self.sessions.check_preparation(&record.source_id, &query)?;
            let provider = self.provider(&record.source_id)?;
            let mut settings =
                self.effective_settings(&record.source_id, session.account.as_ref())?;
            if record.source_id.is_builtin()
                && let Some(profile) = &record.profile
            {
                settings = self.builtin.apply_profile(&settings, profile);
            }
            let preparation = provider.prepare_native(
                &query,
                &ProviderContext {
                    cancellation,
                    runtime_capabilities: request.runtime_capabilities.as_ref(),
                    gfn_settings: Some(&settings),
                },
            )?;
            cancellation.check()?;
            let _guard = self.sessions.guard()?;
            self.sessions.check_preparation(&record.source_id, &query)?;
            if self
                .sessions
                .journal
                .require_session(&record.source_id, &session)?
                .operation
                != record.operation
            {
                return Err(ownership());
            }
            let lease_id =
                api::Text::new(session_manager::new_operation().as_str()).expect("bounded lease");
            let mut retained = None;
            let media = match preparation {
                NativePreparation::Gfn(prepared) => {
                    if !record.source_id.is_builtin() {
                        return Err(ownership());
                    }
                    PreparedMedia::Gfn {
                        context: prepared.get("context").cloned().ok_or_else(ownership)?,
                    }
                }
                NativePreparation::External(prepared) => {
                    if record.source_id.is_builtin() {
                        return Err(ownership());
                    }
                    prepared
                        .accepted
                        .validate_against(&request.offer, now_ms())
                        .map_err(|_| invalid_params())?;
                    let package = self.plugins.media_package(&record.source_id)?;
                    let reference = PackageReference {
                        version_root: package.root().into(),
                        expected_manifest: package.manifest().clone(),
                        data_root: self.plugins.media_data_dir(&record.source_id)?,
                    };
                    retained = Some(package);
                    PreparedMedia::Worker {
                        package: Box::new(reference),
                        prepared,
                    }
                }
            };
            let lease = HostBoundPreparedLease {
                version: 1,
                lease_id: lease_id.clone(),
                offer_id: request.offer.offer_id.clone(),
                runtime_epoch: request.offer.runtime_epoch,
                source_id: record.source_id.clone(),
                session: session.clone(),
                attempt_id: api::AttemptId::new(session_manager::new_operation().as_str())
                    .expect("attempt identity"),
                expires_at_ms: request.offer.expires_at_ms,
                media,
            };
            lease
                .validate_against(&request.offer, now_ms())
                .map_err(|_| invalid_params())?;
            let digest = retained
                .as_ref()
                .map(|package| {
                    package
                        .root()
                        .file_name()
                        .and_then(|value| value.to_str())
                        .ok_or_else(ownership)
                        .map(str::to_owned)
                })
                .transpose()?;
            self.sessions.journal.pin_media(
                &record.source_id,
                &session,
                MediaPin {
                    lease_id: lease_id.as_str().into(),
                    package_sha256: digest,
                    runtime_epoch: lease.runtime_epoch,
                    attempt_id: lease.attempt_id.as_str().into(),
                },
            )?;
            if let Some(package) = retained {
                self.private
                    .packages
                    .lock()
                    .expect("media pins poisoned")
                    .insert(lease_id.as_str().into(), package);
            }
            let mut completion =
                Completion::from(value(lease).map(|value| (value, None)).map_err(Into::into));
            completion.receipt = Some(Box::new(PreparationReceipt {
                journal: Arc::clone(&self.sessions.journal),
                private: Arc::clone(&self.private),
                source: record.source_id,
                session,
                lease_id: lease_id.as_str().into(),
            }));
            Ok(completion)
        };
        prepare().unwrap_or_else(failed)
    }

    fn release_source(&self, params: &Value) -> Result<Value, SourceError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Request {
            source_id: PluginId,
            session: api::SessionKey,
            lease_id: String,
        }
        let request: Request =
            serde_json::from_value(params.clone()).map_err(|_| invalid_params())?;
        let _guard = self.sessions.guard()?;
        self.sessions.journal.release_media(
            &request.source_id,
            &request.session,
            &request.lease_id,
        )?;
        self.private
            .packages
            .lock()
            .expect("media pins poisoned")
            .remove(&request.lease_id);
        Ok(json!({"released":true}))
    }

    fn reconcile_native(&self, params: &Value) -> Result<Value, SourceError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Request {
            media_revision: u64,
            status: NativeMediaStatus,
        }
        let envelope: Request =
            serde_json::from_value(params.clone()).map_err(|_| invalid_params())?;
        let expected_revision = envelope.media_revision;
        let request = envelope.status;
        if request.runtime_epoch == 0 {
            return Err(invalid_params());
        }
        let _guard = self.sessions.guard()?;
        let previous = self.sessions.journal.snapshot()?;
        let Some(binding) = request.active else {
            if request.native_idle && request.legacy_active {
                return Err(invalid_params());
            }
            if let Some(lease) = self.sessions.journal.observe_native(
                request.legacy_active,
                request.native_idle,
                expected_revision,
            )? {
                self.private
                    .packages
                    .lock()
                    .expect("media pins poisoned")
                    .remove(&lease);
            }
            return Ok(
                json!({"attached":request.legacy_active,"legacyActive":request.legacy_active,"nativeIdle":request.native_idle,
                "sessionHandle":previous.as_ref().map(|record|record.operation.as_str())}),
            );
        };
        if request.native_idle
            || request.legacy_active
            || binding.runtime_epoch != request.runtime_epoch
        {
            return Err(ownership());
        }
        let record = match previous {
            Some(record) => {
                if record.source_id != binding.source_id
                    || record.session.as_ref() != Some(&binding.session)
                    || record.media.as_ref().is_none_or(|pin| {
                        pin.lease_id != binding.lease_id.as_str()
                            || pin.runtime_epoch != binding.runtime_epoch
                            || pin.attempt_id != binding.attempt_id.as_str()
                    })
                {
                    return Err(ownership());
                }
                record
            }
            None => {
                let operation = session_manager::new_operation();
                self.sessions.journal.reconcile(
                    binding.source_id.clone(),
                    binding.session.account.clone(),
                    operation,
                    binding.session.clone(),
                )?;
                self.sessions.journal.pin_media(
                    &binding.source_id,
                    &binding.session,
                    MediaPin {
                        lease_id: binding.lease_id.as_str().into(),
                        package_sha256: None,
                        runtime_epoch: binding.runtime_epoch,
                        attempt_id: binding.attempt_id.as_str().into(),
                    },
                )?;
                self.sessions
                    .journal
                    .require_session(&binding.source_id, &binding.session)?
            }
        };
        Ok(
            json!({"attached":true,"sourceId":binding.source_id,"sessionHandle":record.operation.as_str(),"session":binding.session,
            "leaseId":binding.lease_id,"runtimeEpoch":binding.runtime_epoch,"startId":binding.start_id,"state":binding.state}),
        )
    }

    pub(super) fn invalidate_browser(&self, source: Option<&PluginId>) {
        self.private
            .browsers
            .lock()
            .expect("browser grants poisoned")
            .retain(|_, grant| source.is_some_and(|source| grant.source != *source));
    }

    fn open_browser(&self, params: &Value) -> Result<Value, SourceError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Request {
            source_id: PluginId,
            open_handle: String,
        }
        let request: Request =
            serde_json::from_value(params.clone()).map_err(|_| invalid_params())?;
        let grant = self
            .private
            .browsers
            .lock()
            .expect("browser grants poisoned")
            .remove(&request.open_handle)
            .ok_or_else(ownership)?;
        if grant.source != request.source_id
            || grant.expires <= now_ms()
            || self.provider(&grant.source)?.generation() != grant.generation
        {
            return Err(ownership());
        }
        Ok(json!({"url":grant.url.expose_secret(),"attempt":grant.attempt}))
    }

    pub(super) fn project_public(
        &self,
        source: &PluginId,
        completion: &mut Completion,
    ) -> Result<(), SourceError> {
        let Ok((payload, _)) = &mut completion.result else {
            return Ok(());
        };
        let generation = payload["generation"].as_u64().unwrap_or(0);
        let result = &mut payload["result"];
        if result["state"] == "pending" && result["challenge"]["kind"] == "browser" {
            let challenge = result["challenge"]
                .as_object_mut()
                .ok_or_else(invalid_params)?;
            let raw = challenge
                .remove("authorization")
                .ok_or_else(invalid_params)?;
            let url = api::SecretString::new(raw.as_str().ok_or_else(invalid_params)?)
                .map_err(|_| invalid_params())?;
            let parsed = url::Url::parse(url.expose_secret()).map_err(|_| invalid_params())?;
            if parsed.scheme() != "https"
                || parsed.host_str().is_none()
                || !parsed.username().is_empty()
                || parsed.password().is_some()
            {
                return Err(invalid_params());
            }
            let attempt = challenge
                .get("attempt")
                .and_then(Value::as_str)
                .ok_or_else(invalid_params)?
                .to_owned();
            let expires = challenge
                .get("expiresAtMs")
                .and_then(Value::as_u64)
                .ok_or_else(invalid_params)?
                .min(now_ms().saturating_add(3_600_000));
            if expires <= now_ms() {
                return Err(ownership());
            }
            let mut grants = self
                .private
                .browsers
                .lock()
                .expect("browser grants poisoned");
            grants.retain(|_, grant| {
                grant.expires > now_ms() && (&grant.source != source || grant.attempt == attempt)
            });
            if let Some((handle, _)) = grants.iter().find(|(_, grant)| {
                &grant.source == source
                    && grant.attempt == attempt
                    && grant.generation == generation
                    && grant.url == url
            }) {
                challenge.insert("openHandle".into(), json!(handle));
                return Ok(());
            }
            if grants.len() >= 16 {
                return Err(SourceError::new(
                    "busy",
                    "Browser authorization capacity is exhausted",
                ));
            }
            let handle = session_manager::new_operation().as_str().to_owned();
            grants.insert(
                handle.clone(),
                BrowserGrant {
                    source: source.clone(),
                    attempt,
                    generation,
                    expires,
                    url,
                },
            );
            challenge.insert("openHandle".into(), json!(handle));
        }
        if matches!(
            result["state"].as_str(),
            Some("authorized" | "signed-in" | "signed-out" | "not-required")
        ) {
            self.invalidate_browser(Some(source));
        }
        if result
            .get("session")
            .is_some_and(|session| session.get("key").is_some())
        {
            let key: api::SessionKey = serde_json::from_value(result["session"]["key"].clone())
                .map_err(|_| ownership())?;
            let record = self.sessions.journal.require_session(source, &key)?;
            result["sessionHandle"] = json!(record.operation.as_str());
        }
        Ok(())
    }
}

struct PreparationReceipt {
    journal: Arc<SessionJournal>,
    private: Arc<PrivateState>,
    source: PluginId,
    session: api::SessionKey,
    lease_id: String,
}

impl AllocationReceipt for PreparationReceipt {
    fn settle(self: Box<Self>, accepted: bool) -> ReceiptOutcome {
        if accepted {
            return ReceiptOutcome {
                result: Ok(()),
                required_events: Vec::new(),
            };
        }
        let result = requests::scope(Cancellation::default(), || {
            self.journal
                .release_media(&self.source, &self.session, &self.lease_id)
        });
        self.private
            .packages
            .lock()
            .expect("media pins poisoned")
            .remove(&self.lease_id);
        ReceiptOutcome {
            result,
            required_events: Vec::new(),
        }
    }
}

fn value<T: serde::Serialize>(value: T) -> Result<Value, SourceError> {
    serde_json::to_value(value).map_err(|_| {
        SourceError::new(
            "invalid_response",
            "Private media response could not be encoded",
        )
    })
}
fn failed(error: SourceError) -> Completion {
    Completion::from(Err(error.into()))
}
fn ownership() -> SourceError {
    SourceError::new(
        "session_owner_mismatch",
        "The private source handle is unavailable or no longer owned",
    )
}
fn unsupported() -> SourceError {
    SourceError::new(
        "unsupported_feature",
        "This source does not support playback",
    )
}
fn expired_offer() -> SourceError {
    SourceError::new("media_offer_expired", "Create a fresh native media offer")
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests;
