use crate::gfn::{AuthSession, ServiceError};
use crate::proxy::client_for_settings;
use rand::RngCore as _;
use reqwest::blocking::{Client, Response};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue, USER_AGENT};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::Read;
use std::net::{IpAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};
use url::Url;

const LCARS_CLIENT_ID: &str = "ec7e38d4-03af-4b58-b131-cfb0495903ab";
const GFN_CLIENT_VERSION: &str = "2.0.87.131";
const DEFAULT_STREAMING_BASE: &str = "https://prod.cloudmatchbeta.nvidiagrid.net/";
const DEFAULT_STUN_SERVER: &str = "stun:s1.stun.gamestream.nvidia.com:19308";
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(12);
const DISCOVERY_REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
const NETWORK_TEST_REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
const MAXIMUM_NETWORK_TEST_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_DISCOVERY_REGIONS: usize = 32;
const DISCOVERY_CONCURRENCY: usize = 4;
const MAX_CLEANUP_RECORD_BYTES: usize = 16 * 1024;

#[derive(Clone)]
struct ActiveSession {
    session_id: String,
    control_base: String,
    zone: String,
    app_id: String,
    info: Value,
    client: Client,
}

struct SessionConflict {
    owner: (String, String),
    received: Instant,
    sessions: Vec<Value>,
}

struct FreshAllocation {
    info: Value,
    base: Url,
    client: Client,
    headers: HeaderMap,
    owner: (String, String),
}

pub struct CloudMatchService {
    client: Client,
    active: Mutex<Option<ActiveSession>>,
    discovered: Mutex<HashMap<String, Value>>,
    conflict: Mutex<Option<SessionConflict>>,
    allocation_admission: Mutex<()>,
    fresh: Mutex<Option<FreshAllocation>>,
    cleanup_path: Option<PathBuf>,
    retained_cleanup: Mutex<Option<Value>>,
    #[cfg(test)]
    test_control_base: Option<Url>,
}

pub(crate) struct CreateAdmission<'a> {
    service: &'a CloudMatchService,
    _guard: MutexGuard<'a, ()>,
}

impl CreateAdmission<'_> {
    pub(crate) fn create(
        self,
        params: &Value,
        settings: &Value,
        auth: &AuthSession,
        device_id: &str,
    ) -> Result<Value, ServiceError> {
        let service = self.service;
        self.create_at(params, settings, auth, device_id, || {
            let client = client_for_settings(&service.client, settings).map_err(invalid)?;
            #[cfg(test)]
            if let Some(base) = &service.test_control_base {
                return Ok((client, base.clone()));
            }
            let requested_base = requested_streaming_base(params, settings, auth)?;
            let base = service.create_base(
                &client,
                &requested_base,
                params,
                settings,
                session_token(auth),
                device_id,
            )?;
            Ok((client, base))
        })
    }

    fn create_at(
        self,
        params: &Value,
        settings: &Value,
        auth: &AuthSession,
        device_id: &str,
        connection: impl FnOnce() -> Result<(Client, Url), ServiceError>,
    ) -> Result<Value, ServiceError> {
        self.service
            .create_admitted(params, settings, auth, device_id, connection)
    }
}

impl CloudMatchService {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            active: Mutex::new(None),
            discovered: Mutex::new(HashMap::new()),
            conflict: Mutex::new(None),
            allocation_admission: Mutex::new(()),
            fresh: Mutex::new(None),
            cleanup_path: None,
            retained_cleanup: Mutex::new(None),
            #[cfg(test)]
            test_control_base: None,
        }
    }

    pub fn with_cleanup_path(client: Client, path: PathBuf) -> Self {
        let mut service = Self::new(client);
        if let Ok(file) = std::fs::File::open(&path)
            && let Some(record) = read_cleanup_record(file)
            && record["sessionId"]
                .as_str()
                .is_some_and(|id| !id.is_empty())
            && record["streamingBaseUrl"]
                .as_str()
                .is_some_and(|base| trusted_cloudmatch_base(base).is_ok())
            && record["owner"].as_array().is_some_and(|owner| {
                owner.len() == 2
                    && owner.iter().all(|part| {
                        part.as_str()
                            .is_some_and(|value| !value.is_empty() && value.len() <= 256)
                    })
            })
        {
            *service
                .retained_cleanup
                .lock()
                .expect("CloudMatch cleanup state poisoned") = Some(record);
        }
        service.cleanup_path = Some(path);
        service
    }

    #[cfg(test)]
    fn create(
        &self,
        params: &Value,
        settings: &Value,
        auth: &AuthSession,
        device_id: &str,
    ) -> Result<Value, ServiceError> {
        self.admit_create()?
            .create(params, settings, auth, device_id)
    }

    #[cfg(test)]
    fn create_at(
        &self,
        params: &Value,
        settings: &Value,
        auth: &AuthSession,
        device_id: &str,
        connection: impl FnOnce() -> Result<(Client, Url), ServiceError>,
    ) -> Result<Value, ServiceError> {
        self.admit_create()?
            .create_at(params, settings, auth, device_id, connection)
    }

    pub(crate) fn admit_create(&self) -> Result<CreateAdmission<'_>, ServiceError> {
        let guard = self
            .allocation_admission
            .try_lock()
            .map_err(|_| allocation_in_progress())?;
        crate::requests::check()?;
        if self
            .retained_cleanup
            .try_lock()
            .map_err(|_| allocation_in_progress())?
            .is_some()
        {
            return Err(ServiceError {code:"session_cleanup_pending", message:"A cancelled allocation still needs cleanup. End that session before starting another game.".to_owned()});
        }
        if let Some(allocation) = self
            .fresh
            .try_lock()
            .map_err(|_| allocation_in_progress())?
            .as_ref()
        {
            return Err(ServiceError {
                code: if allocation.info["cleanupPending"] == true { "session_cleanup_pending" } else { "session_update_busy" },
                message: "The previous allocation is awaiting confirmation or cleanup. End that session before starting another game.".to_owned(),
            });
        }
        Ok(CreateAdmission {
            service: self,
            _guard: guard,
        })
    }

    fn create_admitted(
        &self,
        params: &Value,
        settings: &Value,
        auth: &AuthSession,
        device_id: &str,
        connection: impl FnOnce() -> Result<(Client, Url), ServiceError>,
    ) -> Result<Value, ServiceError> {
        let (client, base) = connection()?;
        crate::requests::check()?;
        let app_id = launch_app_id(params)?;
        *self
            .conflict
            .lock()
            .expect("CloudMatch conflict state poisoned") = None;
        let token = session_token(auth);
        let mut session_params = params.clone();
        let network_test = if requests_network_test(params, settings) {
            acquire_network_test_session(&client, &base, token, device_id, params, settings)
        } else {
            json!({"status":"not_requested"})
        };
        match network_test["status"].as_str() {
            Some("measured") => eprintln!(
                "Network test measured path datagram {} bytes in {} probes",
                network_test["measuredDatagramBytes"], network_test["probes"]
            ),
            Some("unmeasured") => eprintln!(
                "Network test confirmed no probe datagram in {} probes",
                network_test["probes"]
            ),
            Some("unavailable") => eprintln!(
                "Network test session unavailable: {}",
                network_test["error"].as_str().unwrap_or_default()
            ),
            _ => {}
        }
        crate::requests::check()?;
        session_params["networkTestSessionId"] = json!(network_test["sessionId"].as_str());
        let body = build_create_body(&app_id, &session_params, settings, device_id);
        let mut url = base
            .join("v2/session")
            .map_err(|_| invalid("Invalid CloudMatch session URL"))?;
        crate::language::append_session_preferences(&mut url, settings);
        let response = client
            .post(url)
            .headers(cloudmatch_headers(token, device_id)?)
            .json(&body)
            .send()
            .map_err(|error| network("Session creation failed", error))?;
        let status = response.status();
        let payload = response.json::<Value>();
        if let Ok(payload) = &payload
            && let Some(error) = self.capture_session_conflict(status, payload, &base, auth)
        {
            return Err(error);
        }
        let payload =
            validate_cloudmatch_response("Session creation failed", status, payload, false)?;
        let zone = params["zone"]
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| base.host_str().map(ToOwned::to_owned))
            .unwrap_or_default();
        let mut info = session_info(&payload, &base, &zone, &app_id, device_id)?;
        info["keyboardLayout"] = json!(crate::language::session_keyboard_layout(settings));
        if let Some(session_id) = network_test["sessionId"].as_str() {
            info["networkTestSessionId"] = json!(session_id);
        }
        info["networkTest"] = network_test;
        let control_base = info["streamingBaseUrl"]
            .as_str()
            .and_then(|raw| trusted_cloudmatch_base(raw).ok())
            .unwrap_or_else(|| base.clone());
        *self
            .fresh
            .lock()
            .expect("CloudMatch allocation state poisoned") = Some(FreshAllocation {
            info: info.clone(),
            base: control_base,
            client: client.clone(),
            headers: cloudmatch_headers(token, device_id)?,
            owner: (auth.provider.idp_id.clone(), auth.user.user_id.clone()),
        });
        if crate::requests::current().cancelled() {
            self.finish_create(info["sessionId"].as_str().unwrap_or_default(), false)?;
            return Err(cancelled_allocation());
        }
        let mut request_profile = negotiated_profile(
            &body["sessionRequestData"]["clientRequestMonitorSettings"][0],
            &body["sessionRequestData"]["requestedStreamingFeatures"],
        );
        // The request no longer names a codec (the official client resolves it
        // locally and announces it at RTSP time), so only claim request
        // provenance when a codec value is actually present.
        if request_profile["codec"].is_null() {
            request_profile["codecSource"] = json!("unreported");
        } else {
            request_profile["codecSource"] = json!("request");
        }
        let request_codec = json!({
            "sessionId":info["sessionId"],
            "negotiatedStreamProfile":request_profile
        });
        preserve_session_profile(&mut info, &request_codec);

        if crate::requests::current().cancelled() {
            self.finish_create(info["sessionId"].as_str().unwrap_or_default(), false)?;
            return Err(cancelled_allocation());
        }
        self.store_active(&mut info, &base, &zone, &app_id, client)?;
        info["phase"] =
            Value::String(session_phase(info["status"].as_i64().unwrap_or_default()).to_owned());
        Ok(json!({"session":info}))
    }

    pub fn finish_create(
        &self,
        expected_session_id: &str,
        accepted: bool,
    ) -> Result<(), ServiceError> {
        let mut fresh = self
            .fresh
            .lock()
            .expect("CloudMatch allocation state poisoned");
        let Some(allocation) = fresh.as_mut() else {
            return Ok(());
        };
        let session_id = allocation.info["sessionId"]
            .as_str()
            .ok_or_else(|| upstream("Allocated session has no ID"))?
            .to_owned();
        if session_id != expected_session_id {
            return Ok(());
        }
        if !accepted {
            let url = allocation
                .base
                .join(&format!("v2/session/{session_id}"))
                .map_err(|_| invalid("Invalid allocation cleanup URL"))?;
            let result = delete_session(&allocation.client, url, allocation.headers.clone());
            self.clear_active(&session_id);
            if let Err(error) = result {
                allocation.info["cleanupPending"] = json!(true);
                allocation.info["cleanupErrorCode"] = json!(error.code);
                let mut pending = allocation.info.clone();
                pending["cleanupPending"] = json!(true);
                self.discovered
                    .lock()
                    .expect("CloudMatch discovery state poisoned")
                    .insert(session_id.to_owned(), pending);
                let record = json!({"sessionId":session_id,"appId":allocation.info["appId"],
                    "status":allocation.info["status"],"phase":allocation.info["phase"],
                    "streamingBaseUrl":allocation.base.origin().ascii_serialization(),
                    "cleanupPending":true,"cleanupErrorCode":error.code,"owner":allocation.owner});
                *self
                    .retained_cleanup
                    .lock()
                    .expect("CloudMatch cleanup state poisoned") = Some(record.clone());
                if let Some(path) = &self.cleanup_path {
                    let saved = (|| -> std::io::Result<()> {
                        if let Some(parent) = path.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        let temporary = path.with_extension("tmp");
                        std::fs::write(&temporary, serde_json::to_vec(&record)?)?;
                        std::fs::rename(temporary, path)
                    })();
                    if saved.is_err() {
                        eprintln!("CloudMatch could not persist pending allocation cleanup");
                    }
                }
                return Err(ServiceError {code:"session_cleanup_pending", message:"The cancelled cloud session could not be closed. End it before starting another game.".to_owned()});
            }
            self.discovered
                .lock()
                .expect("CloudMatch discovery state poisoned")
                .remove(&session_id);
            self.clear_cleanup(&session_id);
        }
        *fresh = None;
        Ok(())
    }

    fn pending_cleanup(&self, auth: &AuthSession) -> Option<Value> {
        let provider_base =
            trusted_cloudmatch_base(&crate::gfn::effective_provider_url(&auth.provider)).ok()?;
        self.retained_cleanup
            .lock()
            .expect("CloudMatch cleanup state poisoned")
            .as_ref()
            .filter(|record| record["owner"] == json!([auth.provider.idp_id, auth.user.user_id]))
            .map(|record| {
                let mut public = record.clone();
                if !record["streamingBaseUrl"]
                    .as_str()
                    .and_then(|raw| trusted_cloudmatch_base(raw).ok())
                    .is_some_and(|base| {
                        base == provider_base
                            || (auth.provider.code.eq_ignore_ascii_case("NVIDIA")
                                && base.host_str().is_some_and(|host| {
                                    host.ends_with(".cloudmatchbeta.nvidiagrid.net")
                                }))
                    })
                {
                    public["streamingBaseUrl"] = Value::Null;
                    public["cleanupEndpointUnverified"] = json!(true);
                }
                if let Some(object) = public.as_object_mut() {
                    object.remove("owner");
                }
                public
            })
    }

    fn clear_cleanup(&self, session_id: &str) {
        let mut retained = self
            .retained_cleanup
            .lock()
            .expect("CloudMatch cleanup state poisoned");
        if retained
            .as_ref()
            .is_some_and(|record| record["sessionId"] == session_id)
        {
            if let Some(path) = &self.cleanup_path
                && let Err(error) = std::fs::remove_file(path)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                eprintln!("CloudMatch could not remove completed allocation cleanup record");
                return;
            }
            *retained = None;
        }
    }

    pub fn poll(
        &self,
        params: &Value,
        auth: &AuthSession,
        device_id: &str,
    ) -> Result<Value, ServiceError> {
        let current = self
            .active
            .lock()
            .expect("CloudMatch state poisoned")
            .clone();
        let client = current
            .as_ref()
            .map(|state| state.client.clone())
            .unwrap_or_else(|| self.client.clone());
        let session_id = params["sessionId"]
            .as_str()
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| current.as_ref().map(|state| state.session_id.clone()))
            .ok_or_else(|| invalid("session.poll requires sessionId"))?;
        let control_base = current
            .as_ref()
            .map(|state| state.control_base.clone())
            .ok_or_else(|| invalid("No active session control endpoint"))?;
        let base = trusted_cloudmatch_base(&control_base)?;
        let token = session_token(auth);
        let headers = cloudmatch_headers(token, device_id)?;
        let payload = match self.get_session(&client, &base, &session_id, &headers) {
            Ok(payload) => payload,
            Err(error) if error.code == "session_not_found" => {
                self.clear_active(&session_id);
                return Ok(json!({"session":null,"termination":{
                    "source":"cloudmatch-http","httpStatus":404,
                    "sessionId":session_id,"resumable":false
                }}));
            }
            Err(error) => return Err(error),
        };
        let zone = current
            .as_ref()
            .map(|state| state.zone.clone())
            .unwrap_or_default();
        let app_id = current
            .as_ref()
            .map(|state| state.app_id.clone())
            .unwrap_or_default();
        let mut info = session_info(&payload, &base, &zone, &app_id, device_id)?;

        info["phase"] =
            Value::String(session_phase(info["status"].as_i64().unwrap_or_default()).to_owned());
        if current
            .as_ref()
            .is_some_and(|state| state.info["resumePending"] == true)
        {
            mark_resume_progress(&mut info);
        }
        self.store_active(&mut info, &base, &zone, &app_id, client)?;
        Ok(json!({"session":info}))
    }

    pub fn stop(
        &self,
        params: &Value,
        settings: &Value,
        auth: &AuthSession,
        device_id: &str,
    ) -> Result<Value, ServiceError> {
        let current = self
            .active
            .lock()
            .expect("CloudMatch state poisoned")
            .clone()
            .filter(|state| {
                params["sessionId"]
                    .as_str()
                    .is_none_or(|id| id == state.session_id)
            });
        let session_id = params["sessionId"]
            .as_str()
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| current.as_ref().map(|state| state.session_id.clone()));
        let Some(session_id) = session_id else {
            return Ok(json!({"session":null,"stopped":false}));
        };
        if self
            .retained_cleanup
            .lock()
            .expect("CloudMatch cleanup state poisoned")
            .as_ref()
            .is_some_and(|record| record["sessionId"] == session_id)
        {
            return Err(ServiceError {
                code: "session_cleanup_pending",
                message: "The pending session cannot be safely verified after restart. Cleanup remains pending."
                    .to_owned(),
            });
        }
        let client = current
            .as_ref()
            .map(|state| state.client.clone())
            .or_else(|| {
                self.fresh
                    .lock()
                    .expect("CloudMatch allocation state poisoned")
                    .as_ref()
                    .filter(|allocation| allocation.info["sessionId"] == session_id)
                    .map(|allocation| allocation.client.clone())
            })
            .map(Ok)
            .unwrap_or_else(|| client_for_settings(&self.client, settings).map_err(invalid))?;
        let discovered = self
            .discovered
            .lock()
            .expect("CloudMatch discovery state poisoned")
            .get(&session_id)
            .cloned();
        let base_value = current
            .as_ref()
            .map(|state| state.control_base.clone())
            .or_else(|| {
                discovered
                    .as_ref()
                    .and_then(|session| session["serverIp"].as_str())
                    .and_then(|host| trusted_learned_server_base(host).ok())
                    .map(|base| base.origin().ascii_serialization())
            })
            .or_else(|| {
                discovered
                    .as_ref()
                    .and_then(|session| session["streamingBaseUrl"].as_str())
                    .map(ToOwned::to_owned)
            })
            .ok_or_else(|| invalid("No active session control endpoint"))?;
        let base = trusted_cloudmatch_base(&base_value)?;
        let url = base
            .join(&format!("v2/session/{session_id}"))
            .map_err(|_| invalid("Invalid CloudMatch stop URL"))?;
        #[cfg(test)]
        let url = self
            .test_control_base
            .as_ref()
            .map_or(Ok(url), |base| {
                base.join(&format!("v2/session/{session_id}"))
            })
            .map_err(|_| invalid("Invalid test stop URL"))?;
        self.stop_at(
            &session_id,
            &client,
            url,
            cloudmatch_headers(session_token(auth), device_id)?,
        )
    }

    fn stop_at(
        &self,
        session_id: &str,
        client: &Client,
        url: Url,
        headers: HeaderMap,
    ) -> Result<Value, ServiceError> {
        let response = client
            .delete(url)
            .headers(headers)
            .send()
            .map_err(|error| network("Session stop failed", error))?;
        validate_delete_response("Session stop failed", response)?;
        self.clear_active(session_id);
        self.clear_cleanup(session_id);
        let mut fresh = self
            .fresh
            .lock()
            .expect("CloudMatch allocation state poisoned");
        if fresh
            .as_ref()
            .is_some_and(|allocation| allocation.info["sessionId"] == session_id)
        {
            *fresh = None;
        }
        drop(fresh);
        self.discovered
            .lock()
            .expect("CloudMatch discovery state poisoned")
            .remove(session_id);
        if let Some(conflict) = self
            .conflict
            .lock()
            .expect("CloudMatch conflict state poisoned")
            .as_mut()
        {
            conflict
                .sessions
                .retain(|session| session["sessionId"] != session_id);
        }
        Ok(json!({"session":self.active()["session"],"stopped":true,"sessionId":session_id}))
    }

    pub fn active(&self) -> Value {
        let state = self.active.lock().expect("CloudMatch state poisoned");
        json!({"session":state.as_ref().map(|session| session.info.clone())})
    }

    pub(crate) fn discovered_session(&self, session_id: &str) -> Option<Value> {
        self.discovered
            .lock()
            .expect("CloudMatch discovery state poisoned")
            .get(session_id)
            .cloned()
    }

    pub(crate) fn cleanup_session(&self, auth: &AuthSession, session_id: &str) -> Option<Value> {
        self.pending_cleanup(auth)
            .filter(|session| session["sessionId"] == session_id)
    }

    #[cfg(test)]
    pub(crate) fn seed_owned_session(&self, mut info: Value) {
        let base = trusted_cloudmatch_base(DEFAULT_STREAMING_BASE).unwrap();
        self.store_active(&mut info, &base, "", "fixture", self.client.clone())
            .unwrap();
    }

    #[cfg(test)]
    pub(crate) fn set_test_control_base(&mut self, base: Url) {
        self.test_control_base = Some(base);
    }

    #[cfg(test)]
    pub(crate) fn seed_discovered_sessions(&self, sessions: &[Value]) {
        self.store_discovered(sessions);
    }

    #[cfg(test)]
    fn fixture_url(&self, url: Url) -> Url {
        self.test_control_base.as_ref().map_or_else(
            || url.clone(),
            |base| {
                let mut fixture = base.join(url.path()).expect("valid fixture URL");
                fixture.set_query(url.query());
                fixture
            },
        )
    }

    fn capture_session_conflict(
        &self,
        status: reqwest::StatusCode,
        payload: &Value,
        base: &Url,
        auth: &AuthSession,
    ) -> Option<ServiceError> {
        if status == reqwest::StatusCode::UNAUTHORIZED || !is_session_conflict(payload) {
            return None;
        }
        let sessions = payload["otherUserSessions"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(payload.get("session"))
            .filter_map(|session| remote_session_info(session, base))
            .filter(|session| value_i64(&session["appId"]).is_some_and(|id| id > 0))
            .filter(|session| {
                session["serverIp"]
                    .as_str()
                    .is_some_and(|host| trusted_learned_server_base(host).is_ok())
            })
            .take(32)
            .collect();
        *self
            .conflict
            .lock()
            .expect("CloudMatch conflict state poisoned") = Some(SessionConflict {
            owner: (auth.provider.idp_id.clone(), auth.user.user_id.clone()),
            received: Instant::now(),
            sessions,
        });
        Some(ServiceError {
            code: "session_conflict",
            message: "A GeForce NOW session is already active. Resume it or end it before starting another game.".to_owned(),
        })
    }

    fn take_conflict_sessions(&self, auth: &AuthSession) -> Option<Vec<Value>> {
        self.conflict
            .lock()
            .expect("CloudMatch conflict state poisoned")
            .take()
            .filter(|conflict| {
                conflict.owner == (auth.provider.idp_id.clone(), auth.user.user_id.clone())
                    && conflict.received.elapsed() < Duration::from_secs(30)
                    && !conflict.sessions.is_empty()
            })
            .map(|conflict| conflict.sessions)
    }

    pub fn remote_sessions(
        &self,
        params: &Value,
        settings: &Value,
        auth: &AuthSession,
        device_id: &str,
    ) -> Result<Value, ServiceError> {
        let client = client_for_settings(&self.client, settings).map_err(invalid)?;
        crate::requests::check()?;
        if let Some(sessions) = self.take_conflict_sessions(auth) {
            self.store_discovered(&sessions);
            return Ok(json!({"sessions":sessions}));
        }
        let deadline = Instant::now() + DISCOVERY_TIMEOUT;
        let current = self
            .active
            .lock()
            .expect("CloudMatch state poisoned")
            .clone();
        let recovery_region = current
            .as_ref()
            .filter(|state| params["sessionId"].as_str() == Some(state.session_id.as_str()))
            .and_then(|state| {
                trusted_cloudmatch_base(&state.control_base)
                    .ok()
                    .or_else(|| trusted_cloudmatch_base(&format!("https://{}", state.zone)).ok())
            });
        let requested =
            recovery_region.map_or_else(|| requested_streaming_base(params, settings, auth), Ok)?;
        let headers = cloudmatch_headers(session_token(auth), device_id)?;
        let mut bases = vec![requested.clone()];
        let server_info = client
            .get(
                requested
                    .join("v2/serverInfo")
                    .map_err(|_| invalid("Invalid server-info URL"))?,
            )
            .headers(headers.clone())
            .timeout(DISCOVERY_REQUEST_TIMEOUT)
            .send()
            .map_err(|error| network("Region discovery failed", error))
            .and_then(|response| {
                if !response.status().is_success() {
                    return Err(response_error("Region discovery failed", response));
                }
                let payload = response
                    .json::<Value>()
                    .map_err(|error| network("Invalid region response", error))?;
                if payload["metaData"].as_array().is_none()
                    || (payload.get("requestStatus").is_some()
                        && value_i64(&payload["requestStatus"]["statusCode"]) != Some(1))
                {
                    return Err(upstream("Invalid region response"));
                }
                Ok(payload)
            });
        if let Err(error) = &server_info
            && matches!(error.code, "authentication_required" | "http_unauthorized")
        {
            return Err(error.clone());
        }
        if let Ok(payload) = &server_info {
            for base in regional_bases(payload) {
                if !bases.contains(&base) {
                    bases.push(base);
                }
            }
        }
        let incomplete = server_info.is_err() || bases.len() > MAX_DISCOVERY_REGIONS;
        bases.truncate(MAX_DISCOVERY_REGIONS);
        let mut sessions = discover_sessions(&bases, deadline, incomplete, |base, timeout| {
            let url = base
                .join("v2/session")
                .map_err(|_| invalid("Invalid active-session URL"))?;
            let response = client
                .get(url)
                .headers(headers.clone())
                .timeout(timeout)
                .send()
                .map_err(|error| network("Active-session discovery failed", error))?;
            let payload =
                read_cloudmatch_response("Active-session discovery failed", response, false)?;
            let sessions = payload["sessions"]
                .as_array()
                .ok_or_else(|| upstream("Invalid active-session response"))?;
            Ok(sessions
                .iter()
                .filter_map(|session| remote_session_info(session, base))
                .collect())
        })?;
        if let Some(pending) = self.pending_cleanup(auth)
            && !sessions
                .iter()
                .any(|session| session["sessionId"] == pending["sessionId"])
        {
            sessions.push(pending);
        }
        self.store_discovered(&sessions);
        Ok(json!({"sessions":sessions}))
    }

    fn store_discovered(&self, sessions: &[Value]) {
        let mut discovered = self
            .discovered
            .lock()
            .expect("CloudMatch discovery state poisoned");
        discovered.clear();
        for session in sessions {
            if let Some(session_id) = session["sessionId"].as_str() {
                discovered.insert(session_id.to_owned(), session.clone());
            }
        }
    }

    pub fn claim(
        &self,
        params: &Value,
        settings: &Value,
        auth: &AuthSession,
        device_id: &str,
    ) -> Result<Value, ServiceError> {
        let client = client_for_settings(&self.client, settings).map_err(invalid)?;
        let session_id = params["sessionId"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| invalid("session.claim requires sessionId"))?;
        let requested = requested_streaming_base(params, settings, auth)?;
        let headers = cloudmatch_headers(session_token(auth), device_id)?;
        let discovered = self
            .discovered
            .lock()
            .expect("CloudMatch discovery state poisoned")
            .get(session_id)
            .cloned();
        let zone_base = discovered
            .as_ref()
            .and_then(|session| session["streamingBaseUrl"].as_str())
            .or_else(|| params["streamingBaseUrl"].as_str())
            .map(trusted_cloudmatch_base)
            .transpose()?
            .unwrap_or(requested);
        let mut initial_base = claim_lookup_base(discovered.as_ref(), &zone_base);
        let initial_payload = self
            .get_session(&client, &initial_base, session_id, &headers)
            .or_else(|error| {
                if initial_base == zone_base
                    || matches!(error.code, "authentication_required" | "http_unauthorized")
                {
                    Err(error)
                } else {
                    let payload = self.get_session(&client, &zone_base, session_id, &headers)?;
                    initial_base = zone_base.clone();
                    Ok(payload)
                }
            })?;
        let session = &initial_payload["session"];
        let initial_status = value_i64(&session["status"]).unwrap_or_default();
        let learned_server = first_string(&session["sessionControlInfo"]["ip"]);
        let control_base = learned_server
            .as_deref()
            .and_then(|server| trusted_learned_server_base(server).ok())
            .unwrap_or(initial_base);

        let app_id = first_string(&session["sessionRequestData"]["appId"])
            .or_else(|| first_string(&params["appId"]))
            .unwrap_or_else(|| "0".to_owned());
        if initial_status == 7 {
            self.clear_active(session_id);
            let info = session_info(&initial_payload, &control_base, "", &app_id, device_id)?;
            return Ok(json!({"session":info}));
        }
        let resumed = session_requires_resume(initial_status)?;
        if resumed {
            let mut url = control_base
                .join(&format!("v2/session/{session_id}"))
                .map_err(|_| invalid("Invalid CloudMatch claim URL"))?;
            crate::language::append_session_preferences(&mut url, settings);
            let body = build_resume_body(&app_id, session, settings, device_id);
            #[cfg(test)]
            let url = self.fixture_url(url);
            let response = client
                .put(url)
                .headers(headers.clone())
                .json(&body)
                .send()
                .map_err(|error| network("Session claim failed", error))?;
            let _ = read_cloudmatch_response("Session claim failed", response, true)?;
            eprintln!(
                "CloudMatch RESUME handover completed; awaiting fresh ready status and stream endpoints"
            );
        }

        let zone = zone_base.host_str().unwrap_or_default();
        // Do not treat the pre-claim status or PUT acknowledgement as readiness.
        // Qt schedules cancellable session.poll requests until the fresh GET
        // reports status 2/3 with native endpoints. No long blocking RPC loop.
        let mut info = session_info(&initial_payload, &control_base, zone, &app_id, device_id)?;
        if resumed {
            info["keyboardLayout"] = json!(crate::language::session_keyboard_layout(settings));
        }
        info["resumePending"] = json!(true);
        info["phase"] = json!("resuming");
        self.store_active(&mut info, &control_base, zone, &app_id, client)?;
        Ok(json!({"session":info}))
    }

    pub fn report_ad(
        &self,
        params: &Value,
        auth: &AuthSession,
        device_id: &str,
    ) -> Result<Value, ServiceError> {
        let current = self
            .active
            .lock()
            .expect("CloudMatch state poisoned")
            .clone();
        let client = current
            .as_ref()
            .map(|state| state.client.clone())
            .unwrap_or_else(|| self.client.clone());
        let session_id = params["sessionId"]
            .as_str()
            .map(ToOwned::to_owned)
            .or_else(|| current.as_ref().map(|session| session.session_id.clone()))
            .ok_or_else(|| invalid("session.ad.report requires sessionId"))?;
        let action = match params["action"].as_str() {
            Some("start") => 1,
            Some("pause") => 2,
            Some("resume") => 3,
            Some("finish") => 4,
            Some("cancel") => 5,
            _ => return Err(invalid("Unknown session ad action")),
        };
        let ad_id = params["adId"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| invalid("session.ad.report requires adId"))?;
        let base = current
            .as_ref()
            .and_then(|session| trusted_cloudmatch_base(&session.control_base).ok())
            .ok_or_else(|| invalid("No active session control endpoint"))?;
        let url = base
            .join(&format!("v2/session/{session_id}"))
            .map_err(|_| invalid("Invalid session ad update URL"))?;
        #[cfg(test)]
        let url = self.fixture_url(url);
        let mut update = json!({
            "adId": ad_id,
            "adAction": action,
            "clientTimestamp": params["clientTimestamp"].as_i64().unwrap_or_else(unix_seconds)
        });
        for key in ["watchedTimeInMs", "pausedTimeInMs"] {
            if let Some(value) = params[key].as_i64() {
                update[key] = json!(value.max(0));
            }
        }
        if let Some(reason) = params["cancelReason"].as_str() {
            update["cancelReason"] = json!(reason);
        }
        let response = client
            .put(url)
            .headers(cloudmatch_headers(session_token(auth), device_id)?)
            .json(&json!({"action":6,"adUpdates":[update]}))
            .send()
            .map_err(|error| network("Session ad update failed", error))?;
        let payload = read_cloudmatch_response("Session ad update failed", response, false)?;
        let app_id = current
            .as_ref()
            .map(|session| session.app_id.as_str())
            .unwrap_or("0");
        let zone = current
            .as_ref()
            .map(|session| session.zone.as_str())
            .unwrap_or("");
        let mut info = session_info(&payload, &base, zone, app_id, device_id)?;
        info["phase"] =
            Value::String(session_phase(info["status"].as_i64().unwrap_or_default()).to_owned());
        self.store_active(&mut info, &base, zone, app_id, client)?;
        Ok(json!({"session":info}))
    }

    fn store_active(
        &self,
        info: &mut Value,
        fallback_base: &Url,
        zone: &str,
        app_id: &str,
        client: Client,
    ) -> Result<(), ServiceError> {
        let session_id = info["sessionId"]
            .as_str()
            .ok_or_else(|| upstream("Session result did not include an ID"))?
            .to_owned();
        let mut active = self.active.lock().expect("CloudMatch state poisoned");
        if info["status"] == 7 {
            if active
                .as_ref()
                .is_some_and(|state| state.session_id == session_id)
            {
                *active = None;
            }
            return Ok(());
        }
        if let Some(previous) = active.as_ref() {
            preserve_session_profile(info, &previous.info);
            if previous.session_id == session_id && info["keyboardLayout"].is_null() {
                info["keyboardLayout"] = previous.info["keyboardLayout"].clone();
            }
        }
        *active = Some(ActiveSession {
            session_id,
            control_base: info["streamingBaseUrl"]
                .as_str()
                .unwrap_or_else(|| fallback_base.as_str())
                .to_owned(),
            zone: zone.to_owned(),
            app_id: app_id.to_owned(),
            info: info.clone(),
            client,
        });
        Ok(())
    }

    fn clear_active(&self, session_id: &str) {
        let mut active = self.active.lock().expect("CloudMatch state poisoned");
        if active
            .as_ref()
            .is_some_and(|state| state.session_id == session_id)
        {
            *active = None;
        }
    }

    fn get_session(
        &self,
        client: &Client,
        base: &Url,
        session_id: &str,
        headers: &HeaderMap,
    ) -> Result<Value, ServiceError> {
        let url = base
            .join(&format!("v2/session/{session_id}"))
            .map_err(|_| invalid("Invalid CloudMatch polling URL"))?;
        #[cfg(test)]
        let url = self.fixture_url(url);
        let mut last_error = None;
        for attempt in 0..=2 {
            match client.get(url.clone()).headers(headers.clone()).send() {
                Ok(response)
                    if attempt < 2
                        && matches!(
                            response.status().as_u16(),
                            408 | 425 | 429 | 500 | 502 | 503 | 504
                        ) =>
                {
                    thread::sleep(Duration::from_millis(if attempt == 0 { 250 } else { 750 }));
                }
                Ok(response) => {
                    if response.status() == reqwest::StatusCode::NOT_FOUND {
                        return Err(ServiceError {
                            code: "session_not_found",
                            message: "The requested GeForce NOW session no longer exists."
                                .to_owned(),
                        });
                    }
                    return read_cloudmatch_response("Session polling failed", response, false);
                }
                Err(error) => {
                    last_error = Some(error);
                    if attempt < 2 {
                        thread::sleep(Duration::from_millis(if attempt == 0 { 250 } else { 750 }));
                    }
                }
            }
        }
        Err(network(
            "Session polling failed",
            last_error.expect("polling loop records its final error"),
        ))
    }

    fn create_base(
        &self,
        client: &Client,
        requested: &Url,
        params: &Value,
        settings: &Value,
        token: &str,
        device_id: &str,
    ) -> Result<Url, ServiceError> {
        if explicit_create_base(params, settings) {
            return Ok(requested.clone());
        }
        self.resolve_create_base(client, requested, token, device_id, true)
    }

    fn resolve_create_base(
        &self,
        client: &Client,
        requested: &Url,
        token: &str,
        device_id: &str,
        prefer_regional: bool,
    ) -> Result<Url, ServiceError> {
        let Ok(url) = requested.join("v2/serverInfo") else {
            return Ok(requested.clone());
        };
        let headers = cloudmatch_headers(token, device_id)?;
        let Ok(response) = client.get(url).headers(headers).send() else {
            return Ok(requested.clone());
        };
        if matches!(response.status().as_u16(), 401 | 403) {
            return Err(response_error("Session region discovery failed", response));
        }
        if !response.status().is_success() {
            return Ok(requested.clone());
        }
        let Ok(payload) = response.json::<Value>() else {
            return Ok(requested.clone());
        };
        Ok(regional_bases(&payload)
            .into_iter()
            .find(|base| {
                !prefer_regional || !base.host_str().unwrap_or_default().starts_with("np-")
            })
            .unwrap_or_else(|| requested.clone()))
    }
}

fn launch_app_id(params: &Value) -> Result<String, ServiceError> {
    let value = params["appId"]
        .as_str()
        .or_else(|| params["launchAppId"].as_str())
        .or_else(|| params["variantId"].as_str())
        .ok_or_else(|| invalid("The selected game does not have a launch app ID"))?;
    if value.is_empty() || !value.bytes().all(|character| character.is_ascii_digit()) {
        return Err(invalid("The selected game launch app ID must be numeric"));
    }
    Ok(value.to_owned())
}

fn requested_streaming_base(
    params: &Value,
    settings: &Value,
    auth: &AuthSession,
) -> Result<Url, ServiceError> {
    let raw: String = params["streamingBaseUrl"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            settings["region"]
                .as_str()
                .filter(|value| value.starts_with("https://"))
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| {
            let effective = crate::gfn::effective_provider_url(&auth.provider);
            if effective.trim().is_empty() {
                DEFAULT_STREAMING_BASE.to_owned()
            } else {
                effective
            }
        });
    trusted_cloudmatch_base(&raw)
}

fn explicit_create_base(params: &Value, settings: &Value) -> bool {
    params["streamingBaseUrl"]
        .as_str()
        .is_some_and(|value| !value.trim().is_empty())
        || settings["region"]
            .as_str()
            .is_some_and(|value| value.starts_with("https://"))
}

fn claim_lookup_base(discovered: Option<&Value>, zone_base: &Url) -> Url {
    discovered
        .and_then(|session| session["serverIp"].as_str())
        .and_then(|server| trusted_learned_server_base(server).ok())
        .unwrap_or_else(|| zone_base.clone())
}

fn session_requires_resume(status: i64) -> Result<bool, ServiceError> {
    match status {
        2..=5 => Ok(true),
        1 | 6 => Ok(false),
        _ => Err(upstream(
            "This GeForce NOW session is no longer resumable. End it and launch again.",
        )),
    }
}

fn mark_resume_progress(info: &mut Value) {
    if !matches!(info["status"].as_i64(), Some(1..=6)) {
        info["resumePending"] = json!(false);
        info["phase"] = json!(if info["status"] == 7 {
            "finished"
        } else {
            "failed"
        });
        return;
    }
    let ready = matches!(info["status"].as_i64(), Some(2 | 3))
        && info["rtspsEndpoints"]
            .as_array()
            .is_some_and(|endpoints| !endpoints.is_empty());
    info["resumePending"] = json!(!ready);
    if !ready {
        info["phase"] = json!("resuming");
    }
}

fn build_resume_body(app_id: &str, session: &Value, settings: &Value, device_id: &str) -> Value {
    let created = build_create_body(app_id, &json!({}), settings, device_id);
    let source = &created["sessionRequestData"];
    let mut request = serde_json::Map::new();
    // Resume must not renegotiate codec, monitor geometry, FPS or bitrate.
    for key in [
        "appId",
        "audioMode",
        "remoteControllersBitmap",
        "sdrHdrMode",
        "networkTestSessionId",
        "availableSupportedControllers",
        "preferredController",
        "clientVersion",
        "deviceHashId",
        "internalTitle",
        "clientPlatformName",
        "surroundAudioInfo",
        "clientTimezoneOffset",
        "clientIdentification",
        "parentSessionId",
        "streamerVersion",
        "secureRTSPSupported",
    ] {
        request.insert(key.to_owned(), source[key].clone());
    }
    request.insert(
        "sdrHdrMode".to_owned(),
        json!(accepted_hdr_mode(session).unwrap_or(0)),
    );
    request.insert(
        "metaData".to_owned(),
        json!(
            source["metaData"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|entry| entry["key"] != "clientPhysicalResolution")
                .cloned()
                .collect::<Vec<_>>()
        ),
    );
    for key in [
        "appLaunchMode",
        "enablePersistingInGameSettings",
        "clientPlatformName",
    ] {
        if !session["sessionRequestData"][key].is_null() {
            request.insert(key.to_owned(), session["sessionRequestData"][key].clone());
        }
    }
    json!({"action":2, "data":"RESUME", "sessionRequestData":request,
        "metaData":null, "adUpdates":null})
}

fn monitor_display_data(hdr: bool, settings: &Value) -> Value {
    if !hdr {
        return Value::Null;
    }
    let mut data = json!({
        "desiredContentMaxLuminance":1000,
        "desiredContentMinLuminance":0,
        "desiredContentMaxFrameAverageLuminance":400
    });
    if let Some(display) =
        crate::streamer::validated_native_hdr_display(&settings["nativeHdrDisplay"])
    {
        data["desiredContentMaxLuminance"] = json!(display.maximum_nits);
        data["desiredContentMinLuminance"] =
            json!((display.minimum_nits * 10_000.0).round() as u64);
        if let Some(object) = data.as_object_mut() {
            object.remove("desiredContentMaxFrameAverageLuminance");
        }
        if let Some(metadata) = display.metadata {
            data["desiredContentMaxFrameAverageLuminance"] =
                json!(metadata.maximum_full_frame_nits);
            for (key, coordinate) in [
                "displayPrimaryX0",
                "displayPrimaryY0",
                "displayPrimaryX1",
                "displayPrimaryY1",
                "displayPrimaryX2",
                "displayPrimaryY2",
                "displayWhitePointX",
                "displayWhitePointY",
            ]
            .iter()
            .zip(metadata.coordinates)
            {
                data[*key] = json!((coordinate * 50_000.0).round() as u64);
            }
        }
    }
    data
}

fn build_create_body(app_id: &str, params: &Value, settings: &Value, device_id: &str) -> Value {
    let (width, height) = parse_resolution(&setting_string(settings, "resolution", "1920x1080"));
    let fps = crate::frame_rate::request_frame_rate(settings, params, width, height);
    // The codec stays client-selected: the official Bifrost client resolves it from local
    // preferences ("Selected %s codec from client preferences") and announces the choice
    // at RTSP time (x-nv-vqos bitStreamFormat). It is only read here to constrain the
    // requested color envelope, never sent to CloudMatch.
    let codec = codec_wire(&setting_string(settings, "codec", "auto"));
    let hdr = setting_bool(settings, "enableHdr", false)
        && setting_bool(settings, "nativeHdrSupported", false)
        && matches!(codec, 2 | 3)
        && setting_string(settings, "decoderPreference", "auto") != "software"
        && !matches!(
            setting_string(settings, "nativeVideoBackend", "auto").as_str(),
            "software" | "ffmpeg"
        );
    let requested_color = color_quality_wire(&setting_string(settings, "colorQuality", "8bit_420"));
    // Keep a manually selected codec fixed. H.264 supports only 8-bit 4:2:0,
    // while the official Windows client exposes AV1 at 4:2:0 only. Constrain
    // color instead of silently switching an explicit codec back to Auto/HEVC.
    let (bit_depth, chroma) = match codec {
        2 if hdr => (1, requested_color.1),
        3 if hdr => (1, 0),
        1 => (0, 0),
        3 => (requested_color.0, 0),
        _ => requested_color,
    };
    let cloud_gsync = resolved_cloud_gsync(settings);
    let reflex = cloud_gsync || fps >= 120;
    let persistence = setting_bool(settings, "enablePersistingInGameSettings", true)
        && params["supportsInGameSettingsPersistence"].as_bool() == Some(true);
    let metadata = vec![
        json!({"key":"wssignaling","value":"1"}),
        json!({"key":"SubSessionId","value":random_uuid()}),
        json!({"key":"surroundAudioInfo","value":"2"}),
    ];
    // requestedStreamingFeatures, field-for-field with the official Bifrost request
    // builder: reflex, bitDepth, cloudGsync, enabledL4S, mouseMovementFlags, trueHdr,
    // supportedHidDevices, profile, fallbackToLogicalResolution, hidDevices,
    // chromaFormat, prefilterMode/Sharpness/NoiseReduction, hudStreamingMode.
    // Codec, bitrate ceiling, vsync, channel count, QoS policy, touch support, and the
    // dynamic quality policy are deliberately absent: the official client resolves the
    // codec locally and carries bitrate/policy purely in the RTSP ANNOUNCE.
    let mut features = json!({
        "reflex":reflex,
        "bitDepth":bit_depth,
        "cloudGsync":cloud_gsync,
        "enabledL4S":setting_bool(settings, "enableL4S", false),
        "supportedHidDevices":0,
        "profile":0,
        "fallbackToLogicalResolution":false,
        "chromaFormat":chroma,
        "prefilterMode":0,
        "prefilterSharpness":0,
        "prefilterNoiseReduction":0,
        "hudStreamingMode":0
    });
    features["mouseMovementFlags"] = json!(0);
    features["trueHdr"] = json!(false);
    features["hidDevices"] = Value::Null;
    json!({"sessionRequestData":{
        "appId":app_id.parse::<i64>().unwrap_or_default(),
        "internalTitle":params["title"].as_str(),
        "availableSupportedControllers":[2],
        "preferredController":2,
        "networkTestSessionId":params["networkTestSessionId"].as_str(),
        "parentSessionId":null,
        "clientIdentification":"GFN-PC",
        "deviceHashId":device_id,
        "clientVersion":"30.0",
        "sdkVersion":"2.0",
        "streamerVersion":"14",
        "clientPlatformName":platform_name(settings),
        "clientRequestMonitorSettings":[{
            "monitorId":0,"positionX":0,"positionY":0,
            "widthInPixels":width,"heightInPixels":height,"framesPerSecond":fps,
            "sdrHdrMode":if hdr { 1 } else { 0 },
            "displayData":monitor_display_data(hdr, settings),
            "hdr10PlusGamingData":null,
            "dpi":if cfg!(target_os = "macos") { 144 } else { 96 }
        }],
        "useOps":true,
        "audioMode":2,
        "metaData":metadata,
        "sdrHdrMode":if hdr { 1 } else { 0 },
        "clientDisplayHdrCapabilities":null,
        "surroundAudioInfo":0,
        "remoteControllersBitmap":0,
        "clientTimezoneOffset":chrono::Local::now().offset().utc_minus_local() * 1000,
        "enhancedStreamMode":0,
        "appLaunchMode":app_launch_mode(params),
        "secureRTSPSupported":true,
        "partnerCustomData":null,
        "accountLinked":params["accountLinked"].as_bool().unwrap_or(false),
        "enablePersistingInGameSettings":persistence,
        "requestedAudioFormat":0,
        "userAge":25,
        "requestedStreamingFeatures":features,
        "transport":null
    }})
}

fn session_info(
    payload: &Value,
    fallback_base: &Url,
    zone: &str,
    fallback_app_id: &str,
    device_id: &str,
) -> Result<Value, ServiceError> {
    let session = &payload["session"];
    let session_id = session["sessionId"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| upstream("CloudMatch response did not include a session ID"))?;
    let status = value_i64(&session["status"]).unwrap_or_default();
    let connections = session["connectionInfo"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let signaling_connection = connections.iter().find(|connection| {
        matches!(value_i64(&connection["usage"]), Some(16))
            || (value_i64(&connection["usage"]) == Some(14)
                && matches!(value_i64(&connection["appLevelProtocol"]), Some(1 | 6)))
    });
    let control_host = first_string(&session["sessionControlInfo"]["ip"]);
    let server_ip = signaling_connection
        .and_then(|connection| first_string(&connection["ip"]))
        .or_else(|| {
            signaling_connection
                .and_then(|connection| connection["resourcePath"].as_str())
                .and_then(host_from_resource)
        })
        .or_else(|| control_host.clone())
        .or_else(|| fallback_base.host_str().map(ToOwned::to_owned))
        .unwrap_or_default();
    let control_base = control_host
        .as_deref()
        .filter(|_| {
            value_i64(&session["sessionControlInfo"]["port"]).is_none_or(|port| port == 443)
        })
        .and_then(|host| trusted_learned_server_base(host).ok())
        .map(|base| base.origin().ascii_serialization())
        .unwrap_or_else(|| fallback_base.origin().ascii_serialization());
    let queue_position = queue_position(session);
    let seat_setup_step = value_i64(&session["seatSetupInfo"]["seatSetupStep"]);
    let app_id = first_string(&session["sessionRequestData"]["appId"])
        .unwrap_or_else(|| fallback_app_id.to_owned());
    let rtsps_endpoints = connections
        .iter()
        .filter(|connection| {
            value_i64(&connection["usage"]) == Some(16)
                || (value_i64(&connection["usage"]) == Some(14)
                    && matches!(value_i64(&connection["appLevelProtocol"]), Some(1 | 6)))
        })
        .filter_map(|connection| {
            let address = first_string(&connection["ip"])
                .or_else(|| first_string(&connection["resourcePath"]))?;
            if value_i64(&connection["port"]).is_some_and(|port| !(1..=65535).contains(&port)) {
                return None;
            }
            if address.starts_with("rtsps://") {
                let url = Url::parse(&address).ok()?;
                return (url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.port().is_some())
                .then_some(address);
            }
            if address.contains(&['/', '@', '?', '#'][..]) {
                return None;
            }
            let port = value_i64(&connection["port"])?;
            let host = if address.parse::<IpAddr>().is_ok_and(|ip| ip.is_ipv6()) {
                format!("[{address}]")
            } else {
                address
            };
            let endpoint = format!("rtsps://{host}:{port}");
            Url::parse(&endpoint).ok()?.host_str().map(|_| endpoint)
        })
        .collect::<Vec<_>>();
    let signaling_url = rtsps_endpoints
        .first()
        .map(|endpoint| endpoint.replacen("rtsps://", "wss://", 1));
    let ice_servers = normalize_ice_servers(session);
    let media = connections
        .iter()
        .find(|connection| matches!(value_i64(&connection["usage"]), Some(2 | 17)))
        .and_then(|connection| {
            let ip = first_string(&connection["ip"]).or_else(|| {
                connection["resourcePath"]
                    .as_str()
                    .and_then(host_from_resource)
            })?;
            let port = value_i64(&connection["port"])?;
            (port > 0).then(|| json!({"ip":ip,"port":port,"usage":connection["usage"]}))
        });
    let monitor = &session["sessionRequestData"]["clientRequestMonitorSettings"][0];
    let mut features = session["sessionRequestData"]["requestedStreamingFeatures"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    if let Some(finalized) = session["finalizedStreamingFeatures"].as_object() {
        features.extend(finalized.clone());
    }
    let codec_reported =
        session["negotiatedStreamProfile"].get("codec").is_some() || features.contains_key("codec");
    let mut negotiated = negotiated_profile(monitor, &Value::Object(features));
    for key in ["bitDepth", "chromaFormat"] {
        if session["finalizedStreamingFeatures"].get(key).is_some() {
            negotiated[format!("{key}Source")] = json!("finalized");
        }
    }
    if let Some(codec) = session["negotiatedStreamProfile"].get("codec") {
        negotiated["codec"] = json!(codec.as_str().and_then(|value| {
            match value.trim().to_ascii_uppercase().as_str() {
                "H264" | "AVC" => Some("H264"),
                "H265" | "HEVC" => Some("H265"),
                "AV1" => Some("AV1"),
                _ => None,
            }
        }));
    }
    negotiated["codecSource"] = json!(if codec_reported {
        "server"
    } else {
        "unreported"
    });
    let hdr = accepted_hdr_mode(session);
    negotiated["enableHdr"] = json!(hdr.map(|mode| mode == 1));
    negotiated["enableHdrSource"] = json!(if hdr_mode_value(session).is_some() {
        "server"
    } else {
        "unreported"
    });
    let ad_state = normalize_ad_state(session);
    Ok(json!({
        "sessionId":session_id,
        "subSessionId":session["subSessionId"],
        "appId":app_id,
        "status":status,
        "phase":session_phase(status),
        "termination":if status == 7 { json!({"source":"cloudmatch-session-status","status":7,"sessionId":session_id,"resumable":false}) } else { Value::Null },
        "queuePosition":queue_position,
        "seatSetupStep":seat_setup_step,
        "adState":ad_state,
        "zone":zone,
        "streamingBaseUrl":control_base,
        "serverIp":server_ip,
        "signalingServer":if server_ip.contains(':') { server_ip.clone() } else { format!("{server_ip}:443") },
        "signalingUrl":signaling_url,
        "serverLocation":session["serverLocation"],
        "gpuType":session["gpuType"],
        "appLaunchMode":session["sessionRequestData"]["appLaunchMode"],
        "enablePersistingInGameSettings":session["sessionRequestData"]["enablePersistingInGameSettings"],
        "connectionInfo":connections,
        "rtspsEndpoints":rtsps_endpoints,
        "iceServers":ice_servers,
        "mediaConnectionInfo":media,
        "negotiatedStreamProfile":negotiated,
        "requestedStreamingFeatures":session["sessionRequestData"]["requestedStreamingFeatures"],
        "finalizedStreamingFeatures":session["finalizedStreamingFeatures"],
        "clientId":LCARS_CLIENT_ID,
        "deviceId":device_id
    }))
}

fn normalize_ad_state(session: &Value) -> Value {
    let ads = session["sessionAds"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let required = session["sessionAdsRequired"]
        .as_bool()
        .or_else(|| session["isAdsRequired"].as_bool())
        .or_else(|| session["sessionProgress"]["isAdsRequired"].as_bool())
        .unwrap_or(!ads.is_empty());
    let opportunity = session["opportunity"].clone();
    if !required && ads.is_empty() && opportunity.is_null() {
        Value::Null
    } else {
        json!({
            "isAdsRequired":required,
            "sessionAdsRequired":required,
            "isQueuePaused":opportunity["queuePaused"].as_bool().unwrap_or(false),
            "gracePeriodSeconds":opportunity["gracePeriodSeconds"],
            "message":opportunity["message"],
            "sessionAds":ads,
            "ads":ads,
            "opportunity":opportunity
        })
    }
}

fn accepted_hdr_mode(session: &Value) -> Option<i64> {
    hdr_mode_value(session)
        .and_then(value_i64)
        .map(|mode| i64::from(mode == 1))
}

fn hdr_mode_value(session: &Value) -> Option<&Value> {
    session
        .get("sdrHdrMode")
        .or_else(|| {
            session["sessionRequestData"]["clientRequestMonitorSettings"][0].get("sdrHdrMode")
        })
        .or_else(|| session["sessionRequestData"].get("sdrHdrMode"))
}

fn preserve_session_profile(info: &mut Value, previous: &Value) {
    let Some(session_id) = info["sessionId"].as_str().filter(|id| !id.is_empty()) else {
        return;
    };
    let profile = &previous["negotiatedStreamProfile"];
    if previous["sessionId"].as_str() != Some(session_id) {
        return;
    }
    if (info["negotiatedStreamProfile"]["codecSource"] == "unreported"
        || (info["negotiatedStreamProfile"]["codecSource"] == "request"
            && profile["codecSource"] == "server"))
        && matches!(profile["codec"].as_str(), Some("H264" | "H265" | "AV1"))
        && matches!(profile["codecSource"].as_str(), Some("request" | "server"))
    {
        info["negotiatedStreamProfile"]["codec"] = profile["codec"].clone();
        info["negotiatedStreamProfile"]["codecSource"] = profile["codecSource"].clone();
    }
    for key in ["bitDepth", "chromaFormat", "enableHdr"] {
        let source = format!("{key}Source");
        let current = &info["negotiatedStreamProfile"][&source];
        if (current == "unreported" || (current == "request" && profile[&source] == "finalized"))
            && matches!(
                profile[&source].as_str(),
                Some("request" | "server" | "finalized")
            )
        {
            info["negotiatedStreamProfile"][key] = profile[key].clone();
            info["negotiatedStreamProfile"][&source] = profile[&source].clone();
        }
    }
    let updated = &mut info["negotiatedStreamProfile"];
    if updated.get("bitDepthSource").is_some() || updated.get("chromaFormatSource").is_some() {
        updated["colorQuality"] = json!(profile_color(
            &updated["bitDepth"],
            &updated["chromaFormat"]
        ));
    }
}

fn profile_color(depth: &Value, chroma: &Value) -> Option<&'static str> {
    match (depth.as_i64(), chroma.as_i64()) {
        (Some(8), Some(0)) => Some("8bit_420"),
        (Some(8), Some(1)) => Some("8bit_444"),
        (Some(10), Some(0)) => Some("10bit_420"),
        (Some(10), Some(1)) => Some("10bit_444"),
        _ => None,
    }
}

fn codec_from_wire(value: &Value) -> Option<&'static str> {
    match value_i64(value) {
        Some(1) => Some("H264"),
        Some(2) => Some("H265"),
        Some(3) => Some("AV1"),
        _ => None,
    }
}

fn negotiated_profile(monitor: &Value, features: &Value) -> Value {
    let width = value_i64(&monitor["widthInPixels"]);
    let height = value_i64(&monitor["heightInPixels"]);
    let resolution = width
        .zip(height)
        .map(|(width, height)| format!("{width}x{height}"));
    let codec = codec_from_wire(&features["codec"]);
    let bit_depth = value_i64(&features["bitDepth"]).and_then(|value| match value {
        0 | 8 => Some(8),
        1 | 10 => Some(10),
        _ => None,
    });
    let chroma = value_i64(&features["chromaFormat"]).and_then(|value| match value {
        0 => Some(0),
        1 => Some(1),
        _ => None,
    });
    let color = profile_color(&json!(bit_depth), &json!(chroma));
    let dynamic_streaming_mode =
        value_i64(&features["dynamicStreamingMode"]).filter(|value| (0..=3).contains(value));
    json!({
        "resolution":resolution,
        // Carried alongside `resolution` so consumers do not have to re-parse
        // the string. The stream view sizes its video item from these, and a
        // missing size makes it stretch the picture to the window instead of
        // preserving the aspect ratio.
        "width":width,
        "height":height,
        "fps":value_i64(&monitor["framesPerSecond"]),
        "codec":codec,
        "colorQuality":color,
        "bitDepth":bit_depth,
        "chromaFormat":chroma,
        "bitDepthSource":if features.get("bitDepth").is_some() { "request" } else { "unreported" },
        "chromaFormatSource":if features.get("chromaFormat").is_some() { "request" } else { "unreported" },
        "dynamicStreamingMode":dynamic_streaming_mode,
        "enableL4S":features["enabledL4S"],
        "enableCloudGsync":features["cloudGsync"],
        "enableReflex":features["reflex"]
    })
}

fn cloudmatch_headers(token: &str, device_id: &str) -> Result<HeaderMap, ServiceError> {
    let mut headers = HeaderMap::new();
    let user_agent = bifrost_user_agent();
    insert_header(&mut headers, USER_AGENT, &user_agent)?;
    insert_header(&mut headers, AUTHORIZATION, &format!("GFNJWT {token}"))?;
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("text/plain"));
    headers.insert("nv-client-id", HeaderValue::from_static(LCARS_CLIENT_ID));
    headers.insert(
        "nv-client-streamer",
        HeaderValue::from_static("NVIDIA-CLASSIC"),
    );
    headers.insert("nv-client-type", HeaderValue::from_static("NATIVE"));
    headers.insert(
        "nv-client-version",
        HeaderValue::from_static(GFN_CLIENT_VERSION),
    );
    headers.insert("nv-device-os", HeaderValue::from_static(device_os()));
    headers.insert("nv-device-type", HeaderValue::from_static(device_type()));
    headers.insert("nv-device-make", HeaderValue::from_static(device_make()));
    headers.insert("nv-device-model", HeaderValue::from_static(device_model()));
    insert_header(&mut headers, "x-device-id", device_id)?;
    insert_header(&mut headers, "x-nv-client-identity", &user_agent)?;
    Ok(headers)
}

fn insert_header(
    headers: &mut HeaderMap,
    name: impl reqwest::header::IntoHeaderName,
    value: &str,
) -> Result<(), ServiceError> {
    headers.insert(
        name,
        HeaderValue::from_str(value).map_err(|_| invalid("Invalid CloudMatch header value"))?,
    );
    Ok(())
}

fn read_cloudmatch_response(
    context: &str,
    response: Response,
    allow_not_paused: bool,
) -> Result<Value, ServiceError> {
    let status = response.status();
    let payload = response.json::<Value>();
    validate_cloudmatch_response(context, status, payload, allow_not_paused)
}

fn validate_cloudmatch_response(
    context: &str,
    status: reqwest::StatusCode,
    payload: Result<Value, reqwest::Error>,
    allow_not_paused: bool,
) -> Result<Value, ServiceError> {
    if allow_not_paused
        && status != reqwest::StatusCode::UNAUTHORIZED
        && status != reqwest::StatusCode::FORBIDDEN
        && payload.as_ref().is_ok_and(|payload| {
            value_i64(&payload["requestStatus"]["statusCode"]) == Some(34)
                || payload["requestStatus"]["statusDescription"]
                    .as_str()
                    .is_some_and(|description| description.contains("SESSION_NOT_PAUSED"))
        })
    {
        return payload.map_err(|error| network("CloudMatch returned invalid JSON", error));
    }
    if !status.is_success() {
        return Err(cloudmatch_http_error(
            context,
            status,
            payload.ok().as_ref(),
        ));
    }
    let payload = payload.map_err(|error| network("CloudMatch returned invalid JSON", error))?;
    if value_i64(&payload["requestStatus"]["statusCode"]) != Some(1) {
        let description = payload["requestStatus"]["statusDescription"]
            .as_str()
            .unwrap_or("CloudMatch rejected the request");
        let code = value_i64(&payload["requestStatus"]["unifiedErrorCode"])
            .or_else(|| value_i64(&payload["session"]["errorCode"]));
        return Err(ServiceError {
            code: "session_error",
            message: code.map_or_else(
                || description.to_owned(),
                |code| format!("{description} ({code})"),
            ),
        });
    }
    Ok(payload)
}

fn is_session_conflict(payload: &Value) -> bool {
    value_i64(&payload["requestStatus"]["statusCode"]) == Some(11)
        || payload["requestStatus"]["statusDescription"]
            .as_str()
            .is_some_and(|description| description.to_ascii_uppercase().contains("SESSION_LIMIT"))
        || [
            &payload["requestStatus"]["unifiedErrorCode"],
            &payload["session"]["errorCode"],
        ]
        .iter()
        .any(|code| {
            value_i64(code) == Some(0x4AF1201E)
                || code.as_str().is_some_and(|code| {
                    code.trim_start_matches("0x")
                        .eq_ignore_ascii_case("4AF1201E")
                })
        })
}

fn response_error(context: &str, response: Response) -> ServiceError {
    let status = response.status();
    cloudmatch_http_error(context, status, response.json::<Value>().ok().as_ref())
}

fn cloudmatch_http_error(
    context: &str,
    status: reqwest::StatusCode,
    payload: Option<&Value>,
) -> ServiceError {
    let detail = payload.and_then(|payload| payload["requestStatus"]["statusDescription"].as_str());
    ServiceError {
        code: if status.as_u16() == 401 {
            "http_unauthorized"
        } else if status.as_u16() == 403 {
            "authentication_required"
        } else {
            "upstream_error"
        },
        message: detail.map_or_else(
            || format!("{context} ({status})"),
            |detail| format!("{context} ({status}): {detail}"),
        ),
    }
}

pub(crate) fn trusted_cloudmatch_base(raw: &str) -> Result<Url, ServiceError> {
    let mut url = Url::parse(raw.trim()).map_err(|_| invalid("Invalid CloudMatch endpoint"))?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some_and(|port| port != 443)
        || !url.host().is_some_and(public_session_host)
    {
        return Err(invalid("Untrusted CloudMatch endpoint"));
    }
    url.set_path("/");
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

fn trusted_learned_server_base(server: &str) -> Result<Url, ServiceError> {
    let raw = if server.starts_with("https://") {
        server.to_owned()
    } else if server.contains(':') && server.parse::<IpAddr>().is_ok() {
        format!("https://[{server}]")
    } else {
        format!("https://{server}")
    };
    trusted_cloudmatch_base(&raw)
}

fn public_session_host(host: url::Host<&str>) -> bool {
    match host {
        url::Host::Domain(name) => name.contains('.') && !name.ends_with(".local"),
        url::Host::Ipv4(address) => public_session_ipv4(address),
        url::Host::Ipv6(address) => address.to_ipv4_mapped().map_or_else(
            || {
                !address.is_loopback()
                    && !address.is_unicast_link_local()
                    && !address.is_unspecified()
                    && !address.is_unique_local()
                    && !address.is_multicast()
            },
            public_session_ipv4,
        ),
    }
}

fn public_session_ipv4(address: std::net::Ipv4Addr) -> bool {
    let octets = address.octets();
    !address.is_private()
        && !address.is_loopback()
        && !address.is_link_local()
        && !address.is_unspecified()
        && !address.is_broadcast()
        && !address.is_multicast()
        && octets[0] != 0
        && octets[0] < 240
        && !(octets[0] == 100 && (64..=127).contains(&octets[1]))
}

fn session_server_ip(session: &Value) -> Option<String> {
    first_string(&session["sessionControlInfo"]["ip"]).or_else(|| {
        session["connectionInfo"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|connection| {
                value_i64(&connection["usage"]) == Some(14)
                    && first_string(&connection["ip"]).is_some()
            })
            .and_then(|connection| first_string(&connection["ip"]))
    })
}

fn remote_session_info(session: &Value, base: &Url) -> Option<Value> {
    let session_id = session["sessionId"]
        .as_str()
        .filter(|id| !id.trim().is_empty())?
        .to_owned();
    let status = value_i64(&session["status"])?;
    if !matches!(status, 1..=6) {
        return None;
    }
    let app_id = value_i64(&session["sessionRequestData"]["appId"]).unwrap_or_default();
    let server_ip = first_string(&session["sessionControlInfo"]["ip"])
        .or_else(|| session_server_ip(session))
        .or_else(|| {
            session["connectionInfo"]
                .as_array()?
                .iter()
                .filter(|connection| value_i64(&connection["usage"]) == Some(14))
                .find_map(|connection| {
                    let url = Url::parse(connection["resourcePath"].as_str()?).ok()?;
                    url.host_str().map(ToOwned::to_owned)
                })
        });
    let monitor = session["monitorSettings"]
        .as_array()
        .and_then(|values| values.first())
        .unwrap_or(&session["sessionRequestData"]["clientRequestMonitorSettings"][0]);
    let resolution = value_i64(&monitor["widthInPixels"])
        .zip(value_i64(&monitor["heightInPixels"]))
        .map(|(width, height)| format!("{width}x{height}"));
    Some(json!({
        "sessionId":session_id,
        "subSessionId":session["subSessionId"],
        "appId":app_id,
        "appLaunchMode":session["sessionRequestData"]["appLaunchMode"],
        "enablePersistingInGameSettings":session["sessionRequestData"]["enablePersistingInGameSettings"],
        "gpuType":session["gpuType"],
        "status":status,
        "phase":session_phase(status),
        "queuePosition":queue_position(session),
        "seatSetupStep":value_i64(&session["seatSetupInfo"]["seatSetupStep"]),
        "streamingBaseUrl":base.origin().ascii_serialization(),
        "serverIp":server_ip,
        "signalingUrl":null,
        "resolution":resolution,
        "fps":value_i64(&monitor["framesPerSecond"])
    }))
}

fn unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn discover_sessions(
    bases: &[Url],
    deadline: Instant,
    mut incomplete: bool,
    fetch: impl Fn(&Url, Duration) -> Result<Vec<Value>, ServiceError> + Sync,
) -> Result<Vec<Value>, ServiceError> {
    let fetch = &fetch;
    let cancellation = crate::requests::current();
    let mut results = thread::scope(|scope| {
        let workers = (0..DISCOVERY_CONCURRENCY.min(bases.len()))
            .map(|worker| {
                let cancellation = cancellation.clone();
                scope.spawn(move || {
                    bases
                        .iter()
                        .enumerate()
                        .skip(worker)
                        .step_by(DISCOVERY_CONCURRENCY)
                        .map(|(index, base)| {
                            let remaining = deadline.saturating_duration_since(Instant::now());
                            let result = cancellation.check().and_then(|()| {
                                if remaining.is_zero() {
                                    Err(discovery_failed())
                                } else {
                                    fetch(base, remaining.min(DISCOVERY_REQUEST_TIMEOUT))
                                }
                            });
                            (index, result)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("CloudMatch discovery worker panicked"))
            .collect::<Vec<_>>()
    });
    cancellation.check()?;
    results.sort_by_key(|(index, _)| *index);
    let mut sessions = Vec::new();
    for (_, result) in results {
        match result {
            Ok(found) => {
                for session in found {
                    if !sessions
                        .iter()
                        .any(|known: &Value| known["sessionId"] == session["sessionId"])
                    {
                        sessions.push(session);
                    }
                }
            }
            Err(error) if matches!(error.code, "authentication_required" | "http_unauthorized") => {
                return Err(error);
            }
            Err(_) => incomplete = true,
        }
    }
    if sessions.is_empty() && (incomplete || bases.is_empty()) {
        return Err(discovery_failed());
    }
    Ok(sessions)
}

fn discovery_failed() -> ServiceError {
    ServiceError {
        code: "session_discovery_failed",
        message: "Could not check all GeForce NOW regions for an existing session. Try again."
            .to_owned(),
    }
}

fn regional_bases(payload: &Value) -> Vec<Url> {
    let metadata = payload["metaData"].as_array().cloned().unwrap_or_default();
    let value_for = |key: &str| {
        metadata.iter().find_map(|entry| {
            (entry["key"].as_str() == Some(key))
                .then(|| entry["value"].as_str().map(ToOwned::to_owned))
                .flatten()
        })
    };
    let mut names = Vec::new();
    if let Some(local) = value_for("local-region") {
        names.push(local);
    }
    names.extend(
        value_for("gfn-regions")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
    );
    let mut result = Vec::new();
    for name in names {
        if let Some(raw) = value_for(&name)
            && let Ok(base) = trusted_cloudmatch_base(&raw)
            && !result.iter().any(|existing: &Url| existing == &base)
        {
            result.push(base);
        }
    }
    result
}

fn normalize_ice_servers(session: &Value) -> Vec<Value> {
    let mut servers = session["iceServerConfiguration"]["iceServers"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|entry| {
            let urls = if let Some(values) = entry["urls"].as_array() {
                values.clone()
            } else {
                entry["urls"].as_str().map(|value| vec![json!(value)])?
            };
            (!urls.is_empty()).then(|| {
                json!({"urls":urls,"username":entry["username"],"credential":entry["credential"]})
            })
        })
        .collect::<Vec<_>>();
    if servers.is_empty() {
        servers.push(json!({"urls":[DEFAULT_STUN_SERVER]}));
        servers.push(json!({"urls":["stun:stun.l.google.com:19302"]}));
        servers.push(json!({"urls":["stun:stun1.l.google.com:19302"]}));
    }
    servers
}

fn queue_position(session: &Value) -> Option<i64> {
    [
        &session["queuePosition"],
        &session["seatSetupInfo"]["queuePosition"],
        &session["sessionProgress"]["queuePosition"],
        &session["progressInfo"]["queuePosition"],
    ]
    .into_iter()
    .find_map(value_i64)
    .filter(|value| *value > 0)
}

fn first_string(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .or_else(|| value.as_i64().map(|value| value.to_string()))
        .or_else(|| {
            value
                .as_array()
                .and_then(|values| values.first())
                .and_then(first_string)
        })
        .filter(|value| !value.trim().is_empty())
}

fn value_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
        .or_else(|| value.as_str()?.parse().ok())
}

fn host_from_resource(resource: &str) -> Option<String> {
    let translated = resource
        .replacen("rtsps://", "https://", 1)
        .replacen("rtsp://", "http://", 1);
    Url::parse(&translated)
        .ok()?
        .host_str()
        .map(ToOwned::to_owned)
}

fn session_phase(status: i64) -> &'static str {
    match status {
        1 => "preparing",
        2 => "ready",
        3 => "streaming",
        4 | 5 => "paused",
        6 => "resuming",
        7 => "finished",
        status if status > 3 => "failed",
        _ => "requesting",
    }
}

fn cancelled_allocation() -> ServiceError {
    ServiceError {
        code: "cancelled",
        message: "Fresh allocation cancelled and cleaned up".to_owned(),
    }
}

fn delete_session(client: &Client, url: Url, headers: HeaderMap) -> Result<(), ServiceError> {
    let response = client
        .delete(url)
        .headers(headers)
        .timeout(Duration::from_secs(8))
        .send()
        .map_err(|error| network("Allocation cleanup failed", error))?;
    validate_delete_response("Allocation cleanup failed", response)
}

fn validate_delete_response(context: &str, response: Response) -> Result<(), ServiceError> {
    let status = response.status();
    if matches!(
        status,
        reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::NO_CONTENT
    ) {
        return Ok(());
    }
    if !status.is_success() {
        return Err(response_error(context, response));
    }
    let bytes = response.bytes().map_err(|error| network(context, error))?;
    if bytes.is_empty() {
        return Ok(());
    }
    let payload: Value = serde_json::from_slice(&bytes)
        .map_err(|_| upstream("Invalid session deletion response"))?;
    if payload.get("requestStatus").is_some() {
        validate_cloudmatch_response(context, status, Ok(payload), false)?;
    }
    Ok(())
}

fn requests_network_test(params: &Value, settings: &Value) -> bool {
    params["networkTest"]
        .as_bool()
        .or_else(|| settings["networkTest"].as_bool())
        .unwrap_or(false)
}

fn network_test_key_unavailable() -> ServiceError {
    ServiceError {
        code: "network-test-key-unavailable",
        message: "The session did not provision a network test HMAC key; refusing to probe without verified key material".to_owned(),
    }
}

fn acquire_network_test_session(
    client: &Client,
    base: &Url,
    token: &str,
    device_id: &str,
    params: &Value,
    settings: &Value,
) -> Value {
    match try_network_test_session(client, base, token, device_id, params, settings) {
        Ok(value) => value,
        Err(error) => json!({
            "status":"unavailable",
            "code":error.code,
            "error":error.message,
        }),
    }
}

fn network_test_display_profile(
    settings: &Value,
    params: &Value,
) -> crate::network_test::DisplayProfile {
    let (width, height) = parse_resolution(&setting_string(settings, "resolution", "1920x1080"));
    crate::network_test::DisplayProfile {
        width: u32::try_from(width).unwrap_or(1920),
        height: u32::try_from(height).unwrap_or(1080),
        fps: u32::try_from(crate::frame_rate::request_frame_rate(
            settings, params, width, height,
        ))
        .unwrap_or(60),
    }
}

fn try_network_test_session(
    client: &Client,
    base: &Url,
    token: &str,
    device_id: &str,
    params: &Value,
    settings: &Value,
) -> Result<Value, ServiceError> {
    let profile = network_test_display_profile(settings, params);
    let url = crate::network_test::nettest_url(base)?;
    let body = crate::network_test::allocation_body("GFN-PC", profile);
    let mut headers = cloudmatch_headers(token, device_id)?;
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    crate::requests::check()?;
    let response = client
        .post(url)
        .headers(headers)
        .timeout(NETWORK_TEST_REQUEST_TIMEOUT)
        .json(&body)
        .send()
        .map_err(|error| network("Network test session failed", error))?;
    let status = response.status();
    if response.content_length().unwrap_or(0) > MAXIMUM_NETWORK_TEST_RESPONSE_BYTES {
        return Err(ServiceError {
            code: "network-test-rejected",
            message: "Network test session response exceeded the size limit".to_owned(),
        });
    }
    let mut response = response;
    let mut body_bytes = Vec::new();
    response
        .by_ref()
        .take(MAXIMUM_NETWORK_TEST_RESPONSE_BYTES + 1)
        .read_to_end(&mut body_bytes)
        .map_err(|error| network("Network test session failed", error))?;
    if body_bytes.len() as u64 > MAXIMUM_NETWORK_TEST_RESPONSE_BYTES {
        return Err(ServiceError {
            code: "network-test-rejected",
            message: "Network test session response exceeded the size limit".to_owned(),
        });
    }
    let payload = serde_json::from_slice::<Value>(&body_bytes);
    if !status.is_success() {
        let detail = payload
            .ok()
            .and_then(|payload| {
                payload["requestStatus"]["statusDescription"]
                    .as_str()
                    .map(ToOwned::to_owned)
            })
            .unwrap_or_default();
        return Err(ServiceError {
            code: if matches!(status.as_u16(), 401 | 403) {
                "network-test-unauthorized"
            } else {
                "network-test-rejected"
            },
            message: format!(
                "Network test session returned HTTP {} {detail}",
                status.as_u16()
            ),
        });
    }
    let payload = payload.map_err(|_| invalid("Network test session returned invalid JSON"))?;
    let session = crate::network_test::parse_allocation(&payload)?;
    let Some(key) = session.hmac_key.as_deref() else {
        return Err(network_test_key_unavailable());
    };
    crate::requests::check()?;
    let outcome = probe_network_test_path(&session, key)?;
    crate::requests::check()?;
    let Some(measured_datagram_bytes) = outcome.measured_datagram_bytes else {
        return Ok(json!({
            "status":"unmeasured",
            "probes":outcome.probes,
            "error":"No probe datagram was confirmed on the measured path",
        }));
    };
    Ok(json!({
        "status":"measured",
        "sessionId":session.session_id,
        "serverId":session.server_id,
        "zone":base.host_str().unwrap_or_default(),
        "address":session.address,
        "port":session.port,
        "secure":session.secure,
        "measuredDatagramBytes":measured_datagram_bytes,
        "probes":outcome.probes,
        "thresholds":{
            "bandwidthRecommendedMbps":session.thresholds.bandwidth_recommended_mbps,
            "bandwidthLimitMbps":session.thresholds.bandwidth_limit_mbps,
            "latencyRecommendedMs":session.thresholds.latency_recommended_ms,
            "latencyLimitMs":session.thresholds.latency_limit_ms,
            "packetLossRecommendedPct":session.thresholds.packet_loss_recommended_pct,
            "packetLossLimitPct":session.thresholds.packet_loss_limit_pct,
        },
    }))
}

fn probe_network_test_path(
    session: &crate::network_test::NetworkTestSession,
    key: &[u8],
) -> Result<crate::network_test::ProbeOutcome, ServiceError> {
    let peer = std::net::SocketAddr::new(session.address, session.port);
    let socket = UdpSocket::bind(if peer.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    })
    .map_err(|_| invalid("Network test probe could not bind a UDP socket"))?;
    crate::network_test::probe_mtu(
        &socket,
        peer,
        key,
        session.session_id.as_bytes(),
        crate::network_test::PROBE_FLOOR_BYTES,
        crate::network_test::PROBE_CEILING_BYTES,
    )
}

fn session_token(auth: &AuthSession) -> &str {
    auth.tokens
        .id_token
        .as_deref()
        .unwrap_or(&auth.tokens.access_token)
}

fn parse_resolution(value: &str) -> (i64, i64) {
    value
        .split_once('x')
        .and_then(|(width, height)| Some((width.parse().ok()?, height.parse().ok()?)))
        .filter(|(width, height)| *width > 0 && *height > 0)
        .unwrap_or((1920, 1080))
}

fn setting_string(settings: &Value, key: &str, fallback: &str) -> String {
    settings[key]
        .as_str()
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
        .to_owned()
}

fn setting_bool(settings: &Value, key: &str, fallback: bool) -> bool {
    settings[key].as_bool().unwrap_or(fallback)
}

fn resolved_cloud_gsync(settings: &Value) -> bool {
    match settings["nativeCloudGsyncMode"].as_str().unwrap_or("auto") {
        "disabled" => false,
        "forced" => true,
        _ => setting_bool(settings, "enableCloudGsync", false),
    }
}

fn codec_wire(value: &str) -> i64 {
    match value.to_ascii_lowercase().as_str() {
        "h264" => 1,
        "h265" | "hevc" => 2,
        "av1" => 3,
        // Zero delegates the final choice to CloudMatch, matching the
        // official native client. Explicit user choices remain pinned.
        _ => 0,
    }
}

fn color_quality_wire(value: &str) -> (i64, i64) {
    match value {
        "10bit_420" => (1, 0),
        "8bit_444" => (0, 1),
        "10bit_444" => (1, 1),
        _ => (0, 0),
    }
}

fn app_launch_mode(params: &Value) -> i64 {
    match params["appLaunchMode"].as_str() {
        Some("gamepadFriendly") => 2,
        Some("touchFriendly") => 3,
        _ => 1,
    }
}

fn platform_name(settings: &Value) -> &'static str {
    if setting_bool(settings, "identifyAsSteamDeck", false) {
        "SteamOS"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else if cfg!(target_os = "macos") {
        "MacOSX"
    } else {
        "Linux"
    }
}

fn device_os() -> &'static str {
    if cfg!(target_os = "windows") {
        "WINDOWS"
    } else if cfg!(target_os = "macos") {
        "MACOS"
    } else {
        "LINUX"
    }
}

fn device_type() -> &'static str {
    "DESKTOP"
}

fn device_make() -> &'static str {
    if cfg!(target_os = "macos") {
        "Apple"
    } else {
        "UNKNOWN"
    }
}

fn device_model() -> &'static str {
    "UNKNOWN"
}

fn bifrost_user_agent() -> String {
    let platform = if cfg!(target_os = "windows") {
        "Windows NT 10.0"
    } else if cfg!(target_os = "macos") {
        "MacOSX"
    } else {
        "Linux"
    };
    format!("GFN-PC/30.0 ({platform}) BifrostClientSDK/4.9 (38495286)")
}

fn random_uuid() -> String {
    let mut bytes = [0_u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

fn read_cleanup_record(reader: impl Read) -> Option<Value> {
    let mut bytes = Vec::new();
    reader
        .take((MAX_CLEANUP_RECORD_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > MAX_CLEANUP_RECORD_BYTES {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}

fn allocation_in_progress() -> ServiceError {
    ServiceError {
        code: "session_update_busy",
        message: "A fresh allocation is already in progress. Wait for it to finish before starting another game.".to_owned(),
    }
}

fn invalid(message: impl Into<String>) -> ServiceError {
    ServiceError {
        code: "invalid_params",
        message: message.into(),
    }
}

fn upstream(message: impl Into<String>) -> ServiceError {
    ServiceError {
        code: "upstream_error",
        message: message.into(),
    }
}

fn network(context: &str, error: impl std::fmt::Display) -> ServiceError {
    ServiceError {
        code: "network_error",
        message: format!("{context}: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_server(
        replies: Vec<(u16, Value)>,
        on_request: impl Fn(usize) + Send + 'static,
    ) -> (Url, thread::JoinHandle<Vec<String>>) {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        let worker = thread::spawn(move || {
            let mut requests = Vec::new();
            for (index, (status, body)) in replies.into_iter().enumerate() {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(&stream);
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                requests.push(line.trim().to_owned());
                let mut length = 0;
                loop {
                    line.clear();
                    assert!(reader.read_line(&mut line).unwrap() > 0);
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap();
                    }
                }
                reader.read_exact(&mut vec![0; length]).unwrap();
                on_request(index);
                let body = body.to_string();
                write!(stream, "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
            requests
        });
        (base, worker)
    }

    #[test]
    fn create_and_claim_use_exact_independent_language_query_pairs() {
        for (game, keyboard, expected_game, expected_keyboard) in [
            ("es_419", "de-DE", "es_419", "de-DE"),
            ("zh_Hant_TW", "ja-JP", "zh_Hant_TW", "ja-106"),
            ("system", "es-ES", "en_US", "es-ES_tradnl"),
            ("uk_UA", "uk-UA", "uk_UA", "uk-UA"),
        ] {
            let settings =
                json!({"appLanguage":"fr", "gameLanguage":game, "keyboardLayout":keyboard});
            let (base, server) = session_server(
                vec![
                    (
                        200,
                        json!({"requestStatus":{"statusCode":1},"session":{"sessionId":"A","status":1}}),
                    ),
                    (204, json!({})),
                    (
                        200,
                        json!({"requestStatus":{"statusCode":1},"session":{"sessionId":"B","status":2}}),
                    ),
                    (200, json!({"requestStatus":{"statusCode":1}})),
                ],
                |_| {},
            );
            let client = Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap();
            let mut service = CloudMatchService::new(client.clone());
            let created = service
                .create_at(
                    &json!({"appId":"123"}),
                    &settings,
                    &conflict_auth(),
                    "device",
                    || Ok((client, base.clone())),
                )
                .unwrap();
            assert_eq!(created["session"]["keyboardLayout"], expected_keyboard);
            service.finish_create("A", false).unwrap();
            service.set_test_control_base(base);
            let claimed = service
                .claim(
                    &json!({"sessionId":"B"}),
                    &settings,
                    &conflict_auth(),
                    "device",
                )
                .unwrap();
            assert_eq!(claimed["session"]["keyboardLayout"], expected_keyboard);
            let received = server.join().unwrap();
            assert_eq!(received.len(), 4);
            for index in [0, 3] {
                let target = received[index].split_whitespace().nth(1).unwrap();
                let url = Url::parse(&format!("https://fixture.invalid{target}")).unwrap();
                let pairs: std::collections::HashMap<_, _> = url.query_pairs().collect();
                assert_eq!(pairs["languageCode"], expected_game);
                assert_eq!(pairs["keyboardLayout"], expected_keyboard);
                assert_eq!(pairs.len(), 2);
            }
            assert!(received[0].starts_with("POST "));
            assert!(received[1].starts_with("DELETE /v2/session/A"));
            assert!(received[3].starts_with("PUT /v2/session/B?"));
        }
    }

    #[test]
    fn concurrent_create_is_rejected_before_network_and_cannot_replace_handoff_owner() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            mpsc,
        };
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (base, server) = session_server(
            vec![
                (
                    200,
                    json!({"requestStatus":{"statusCode":1},"session":{"sessionId":"A","status":1}}),
                ),
                (204, json!({})),
            ],
            move |index| {
                if index == 0 {
                    entered_tx.send(()).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                }
            },
        );
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let service = CloudMatchService::new(client.clone());
        let connection_calls = AtomicUsize::new(0);
        let (result_tx, result_rx) = mpsc::channel();
        thread::scope(|scope| {
            let first = scope.spawn(|| {
                service.create_at(
                    &json!({"appId":"123"}),
                    &json!({}),
                    &conflict_auth(),
                    "device",
                    || {
                        connection_calls.fetch_add(1, Ordering::SeqCst);
                        Ok((client.clone(), base.clone()))
                    },
                )
            });
            entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            scope.spawn(|| {
                result_tx
                    .send(service.create_at(
                        &json!({"appId":"456"}),
                        &json!({}),
                        &conflict_auth(),
                        "device",
                        || {
                            connection_calls.fetch_add(1, Ordering::SeqCst);
                            Ok((client.clone(), base.clone()))
                        },
                    ))
                    .unwrap();
            });
            let second = result_rx.recv_timeout(Duration::from_secs(2));
            release_tx.send(()).unwrap();
            assert_eq!(
                second
                    .expect("another create must not wait for the delayed POST")
                    .unwrap_err()
                    .code,
                "session_update_busy"
            );
            assert_eq!(first.join().unwrap().unwrap()["session"]["sessionId"], "A");
        });
        assert_eq!(connection_calls.load(Ordering::SeqCst), 1);
        let second = service.create_at(
            &json!({"appId":"456"}),
            &json!({}),
            &conflict_auth(),
            "device",
            || panic!("an unaccepted allocation must keep its reservation"),
        );
        assert_eq!(second.unwrap_err().code, "session_update_busy");
        service.finish_create("B", false).unwrap();
        assert_eq!(
            service.fresh.lock().unwrap().as_ref().unwrap().info["sessionId"],
            "A"
        );
        {
            let _cleanup = service.fresh.lock().unwrap();
            assert_eq!(
                service
                    .create_at(
                        &json!({"appId":"456"}),
                        &json!({}),
                        &conflict_auth(),
                        "device",
                        || { panic!("allocation admission must not wait for a cleanup lock") }
                    )
                    .unwrap_err()
                    .code,
                "session_update_busy"
            );
        }
        service.finish_create("A", false).unwrap();
        assert!(service.fresh.lock().unwrap().is_none());
        let received = server.join().unwrap();
        assert_eq!(received.len(), 2);
        assert!(received[0].starts_with("POST /v2/session?"));
        assert_eq!(received[1], "DELETE /v2/session/A HTTP/1.1");
    }

    #[test]
    fn pre_id_failures_release_allocation_admission_for_the_next_attempt() {
        let (base, server) = session_server(
            vec![
                (503, json!({})),
                (200, json!({"requestStatus":{"statusCode":4}})),
                (200, json!({"requestStatus":{"statusCode":1},"session":{}})),
                (
                    200,
                    json!({"requestStatus":{"statusCode":1},"session":{"sessionId":"A","status":1}}),
                ),
            ],
            |_| {},
        );
        let client = Client::new();
        let service = CloudMatchService::new(client.clone());
        let requests = std::sync::Arc::new(crate::requests::Requests::default());
        let permit = requests.admit("cancelled", "session.create").unwrap();
        requests.cancel("cancelled");
        let cancelled = crate::requests::scope(permit.token.clone(), || {
            service.create_at(
                &json!({"appId":"123"}),
                &json!({}),
                &conflict_auth(),
                "device",
                || panic!("cancelled admission cannot resolve an endpoint"),
            )
        });
        assert_eq!(cancelled.unwrap_err().code, "cancelled");
        assert!(service.allocation_admission.try_lock().is_ok());
        let preparing = requests.admit("preparing", "session.create").unwrap();
        let cancelled = crate::requests::scope(preparing.token.clone(), || {
            service.create_at(
                &json!({"appId":"123"}),
                &json!({}),
                &conflict_auth(),
                "device",
                || {
                    requests.cancel("preparing");
                    Ok((client.clone(), base.clone()))
                },
            )
        });
        assert_eq!(cancelled.unwrap_err().code, "cancelled");
        assert!(service.allocation_admission.try_lock().is_ok());
        let unavailable = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let unavailable_base =
            Url::parse(&format!("http://{}/", unavailable.local_addr().unwrap())).unwrap();
        let timed_client = Client::builder()
            .timeout(Duration::from_millis(20))
            .build()
            .unwrap();
        assert_eq!(
            service
                .create_at(
                    &json!({"appId":"123"}),
                    &json!({}),
                    &conflict_auth(),
                    "device",
                    || { Ok((timed_client, unavailable_base)) }
                )
                .unwrap_err()
                .code,
            "network_error"
        );
        assert!(service.allocation_admission.try_lock().is_ok());
        assert!(service.fresh.lock().unwrap().is_none());
        assert!(
            service
                .create_at(
                    &json!({"appId":"123"}),
                    &json!({}),
                    &conflict_auth(),
                    "device",
                    || { Err(invalid("fixture connection preparation failure")) }
                )
                .is_err()
        );
        assert!(
            service
                .create_at(&json!({}), &json!({}), &conflict_auth(), "device", || {
                    Ok((client.clone(), base.clone()))
                })
                .is_err()
        );
        for code in ["upstream_error", "session_error", "upstream_error"] {
            assert_eq!(
                service
                    .create_at(
                        &json!({"appId":"123"}),
                        &json!({}),
                        &conflict_auth(),
                        "device",
                        || { Ok((client.clone(), base.clone())) }
                    )
                    .unwrap_err()
                    .code,
                code
            );
            assert!(service.allocation_admission.try_lock().is_ok());
            assert!(service.fresh.lock().unwrap().is_none());
        }
        let result = service
            .create_at(
                &json!({"appId":"123"}),
                &json!({}),
                &conflict_auth(),
                "device",
                || Ok((client.clone(), base.clone())),
            )
            .unwrap();
        assert_eq!(result["session"]["sessionId"], "A");
        service.finish_create("A", true).unwrap();
        let received = server.join().unwrap();
        assert_eq!(received.len(), 4);
        assert!(
            received[..4]
                .iter()
                .all(|request| request.starts_with("POST /v2/session?"))
        );
    }

    #[test]
    fn cleanup_record_reader_bounds_consumption_before_parsing() {
        struct EndlessReader(usize);
        impl Read for EndlessReader {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                self.0 += buffer.len();
                buffer.fill(b' ');
                Ok(buffer.len())
            }
        }
        let mut endless = EndlessReader(0);
        assert!(read_cleanup_record(&mut endless).is_none());
        assert_eq!(endless.0, MAX_CLEANUP_RECORD_BYTES + 1);
        let mut boundary = vec![b' '; MAX_CLEANUP_RECORD_BYTES];
        boundary[..2].copy_from_slice(b"{}");
        assert_eq!(read_cleanup_record(boundary.as_slice()), Some(json!({})));
        assert!(read_cleanup_record(b"not JSON".as_slice()).is_none());
    }

    #[test]
    fn fresh_allocation_uses_validated_response_control_host() {
        for (control, expected) in [
            (
                json!({"ip":"control.partner.example","port":443}),
                "https://control.partner.example/",
            ),
            (
                json!({"ip":"forwarded.partner.example"}),
                "https://forwarded.partner.example/",
            ),
            (json!({"ip":"localhost","port":443}), ""),
            (json!({"ip":"control.partner.example","port":8443}), ""),
            (json!({"ip":"https://user@control.partner.example"}), ""),
            (Value::Null, ""),
        ] {
            let (base, server) = session_server(
                vec![(
                    200,
                    json!({"requestStatus":{"statusCode":1},"session":{
                        "sessionId":"fresh-seat","status":1,"sessionControlInfo":control
                    }}),
                )],
                |_| {},
            );
            let client = Client::new();
            let service = CloudMatchService::new(client.clone());
            let result = service
                .create_at(
                    &json!({"appId":"123"}),
                    &json!({}),
                    &conflict_auth(),
                    "device",
                    || Ok((client, base.clone())),
                )
                .unwrap();
            assert_eq!(
                result["session"]["streamingBaseUrl"],
                if expected.is_empty() {
                    base.origin().ascii_serialization()
                } else {
                    expected.trim_end_matches('/').to_owned()
                }
            );
            assert_eq!(
                service
                    .fresh
                    .lock()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .base
                    .as_str(),
                if expected.is_empty() {
                    base.as_str()
                } else {
                    expected
                }
            );
            service.finish_create("fresh-seat", true).unwrap();
            assert_eq!(server.join().unwrap().len(), 1);
        }
    }

    #[test]
    fn cancelled_allocation_retries_response_control_host_and_persists_guarded_cleanup() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("pending-session-cleanup.json");
        let requests = std::sync::Arc::new(crate::requests::Requests::default());
        let permit = requests.admit("create", "session.create").unwrap();
        let (base, post_server) = session_server(
            vec![(
                200,
                json!({"requestStatus":{"statusCode":1},"session":{
                    "sessionId":"fresh-seat","status":1,
                    "sessionControlInfo":{"ip":"203.0.113.20","port":443}
                }}),
            )],
            move |_| requests.cancel("create"),
        );
        let proxy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let proxy_base = format!("http://{}", proxy.local_addr().unwrap());
        let proxy_server = thread::spawn(move || {
            use std::io::{BufRead, BufReader, Write};
            let mut targets = Vec::new();
            for _ in 0..2 {
                let (mut stream, _) = proxy.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(&stream);
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                targets.push(line.trim().to_owned());
                loop {
                    line.clear();
                    assert!(reader.read_line(&mut line).unwrap() > 0);
                    if line == "\r\n" {
                        break;
                    }
                    assert!(!line.to_ascii_lowercase().starts_with("authorization:"));
                }
                let _ = stream.write_all(
                    b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
            }
            targets
        });
        let client = Client::builder()
            .proxy(reqwest::Proxy::https(&proxy_base).unwrap())
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();
        let service = CloudMatchService::with_cleanup_path(client.clone(), path.clone());
        let result = crate::requests::scope(permit.token.clone(), || {
            service.create_at(
                &json!({"appId":"123"}),
                &json!({}),
                &conflict_auth(),
                "device",
                || Ok((client, base)),
            )
        });
        assert_eq!(result.unwrap_err().code, "session_cleanup_pending");
        assert_eq!(
            service
                .fresh
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .base
                .as_str(),
            "https://203.0.113.20/"
        );
        assert!(service.active()["session"].is_null());
        assert_eq!(
            service.discovered.lock().unwrap()["fresh-seat"]["cleanupPending"],
            true
        );
        assert_eq!(
            service.finish_create("fresh-seat", false).unwrap_err().code,
            "session_cleanup_pending"
        );
        assert_eq!(
            service.retained_cleanup.lock().unwrap().as_ref().unwrap()["streamingBaseUrl"],
            "https://203.0.113.20"
        );
        let persisted: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(persisted["streamingBaseUrl"], "https://203.0.113.20");
        let restarted = CloudMatchService::with_cleanup_path(Client::new(), path);
        let pending = restarted.pending_cleanup(&conflict_auth()).unwrap();
        assert_eq!(pending["sessionId"], "fresh-seat");
        assert!(pending["streamingBaseUrl"].is_null());
        assert_eq!(pending["cleanupEndpointUnverified"], true);
        assert_eq!(
            restarted.admit_create().err().unwrap().code,
            "session_cleanup_pending"
        );
        assert_eq!(post_server.join().unwrap().len(), 1);
        assert_eq!(
            proxy_server.join().unwrap(),
            [
                "CONNECT 203.0.113.20:443 HTTP/1.1",
                "CONNECT 203.0.113.20:443 HTTP/1.1",
            ]
        );
    }

    #[test]
    fn cancelled_fresh_post_is_compensated_without_resume() {
        let requests = std::sync::Arc::new(crate::requests::Requests::default());
        let permit = requests.admit("create", "session.create").unwrap();
        let mut replies = vec![(
            200,
            json!({"requestStatus":{"statusCode":1},"session":{"sessionId":"fresh-seat","status":1}}),
        )];
        replies.push((204, json!({})));
        let (base, server) = session_server(replies, move |index| {
            if index == 0 {
                requests.cancel("create");
            }
        });
        let client = Client::new();
        let service = CloudMatchService::new(client.clone());
        let result = crate::requests::scope(permit.token.clone(), || {
            service.create_at(
                &json!({"appId":"123"}),
                &json!({}),
                &conflict_auth(),
                "device",
                || Ok((client, base)),
            )
        });
        assert_eq!(result.unwrap_err().code, "cancelled");
        let received = server.join().unwrap();
        assert!(received[0].starts_with("POST /v2/session?"));
        assert_eq!(received.len(), 2);
        assert_eq!(
            received.last().unwrap(),
            "DELETE /v2/session/fresh-seat HTTP/1.1"
        );
        assert!(service.active()["session"].is_null());
        assert!(service.fresh.lock().unwrap().is_none());
    }

    #[test]
    fn unaccepted_allocation_retains_failed_cleanup_and_retries_exact_seat() {
        let directory = tempfile::tempdir().unwrap();
        let cleanup_path = directory.path().join("pending-session-cleanup.json");
        let (base, server) = session_server(
            vec![
                (
                    200,
                    json!({"requestStatus":{"statusCode":1},"session":{"sessionId":"fresh-seat","status":1}}),
                ),
                (503, json!({})),
                (404, json!({})),
            ],
            |_| {},
        );
        let client = Client::new();
        let service = CloudMatchService::with_cleanup_path(client.clone(), cleanup_path.clone());
        let result = service
            .create_at(
                &json!({"appId":"123"}),
                &json!({}),
                &conflict_auth(),
                "device",
                || Ok((client.clone(), base.clone())),
            )
            .unwrap();
        assert_eq!(result["session"]["sessionId"], "fresh-seat");
        assert_eq!(
            service.finish_create("fresh-seat", false).unwrap_err().code,
            "session_cleanup_pending"
        );
        assert!(service.active()["session"].is_null());
        assert_eq!(
            service.discovered.lock().unwrap()["fresh-seat"]["cleanupPending"],
            true
        );
        assert!(cleanup_path.exists());
        assert_eq!(
            service
                .stop(
                    &json!({"sessionId":"fresh-seat"}),
                    &json!({}),
                    &conflict_auth(),
                    "device"
                )
                .unwrap_err()
                .code,
            "session_cleanup_pending"
        );
        assert_eq!(
            service
                .create_at(
                    &json!({"appId":"456"}),
                    &json!({}),
                    &conflict_auth(),
                    "device",
                    || Ok((client, base))
                )
                .unwrap_err()
                .code,
            "session_cleanup_pending"
        );
        service.finish_create("other-seat", false).unwrap();
        assert!(service.fresh.lock().unwrap().is_some());
        service.finish_create("fresh-seat", false).unwrap();
        assert!(!cleanup_path.exists());
        assert!(service.fresh.lock().unwrap().is_none());
        assert!(service.discovered.lock().unwrap().is_empty());
        let received = server.join().unwrap();
        assert_eq!(received.len(), 3);
        assert_eq!(received[1], received[2]);
    }

    #[test]
    fn accepted_allocation_is_not_deleted_and_unrelated_terminal_keeps_active_slot() {
        let (base, server) = session_server(
            vec![(
                200,
                json!({"requestStatus":{"statusCode":1},"session":{"sessionId":"A","status":1}}),
            )],
            |_| {},
        );
        let client = Client::new();
        let service = CloudMatchService::new(client.clone());
        service
            .create_at(
                &json!({"appId":"123"}),
                &json!({}),
                &conflict_auth(),
                "device",
                || Ok((client.clone(), base.clone())),
            )
            .unwrap();
        service.finish_create("A", true).unwrap();
        assert!(service.fresh.lock().unwrap().is_none());
        assert_eq!(server.join().unwrap().len(), 1);
        service.clear_active("B");
        assert_eq!(service.active()["session"]["sessionId"], "A");
        let mut finished = session_info(
            &json!({"session":{"sessionId":"B","status":7}}),
            &base,
            "",
            "",
            "device",
        )
        .unwrap();
        assert_eq!(finished["phase"], "finished");
        assert_eq!(
            finished["termination"]["source"],
            "cloudmatch-session-status"
        );
        service
            .store_active(&mut finished, &base, "", "", client)
            .unwrap();
        assert_eq!(service.active()["session"]["sessionId"], "A");
        service.clear_active("A");
        assert!(service.active()["session"].is_null());
    }

    #[test]
    fn stopping_discovered_b_keeps_active_a_and_other_discovered_sessions() {
        let (base, server) = session_server(vec![(204, json!({}))], |_| {});
        let client = Client::new();
        let service = CloudMatchService::new(client.clone());
        let mut active = json!({"sessionId":"A","status":3});
        service
            .store_active(&mut active, &base, "", "123", client.clone())
            .unwrap();
        service.store_discovered(&[json!({"sessionId":"B"}), json!({"sessionId":"C"})]);
        let result = service
            .stop_at(
                "B",
                &client,
                base.join("v2/session/B").unwrap(),
                HeaderMap::new(),
            )
            .unwrap();
        assert_eq!(result["session"]["sessionId"], "A");
        assert_eq!(service.active()["session"]["sessionId"], "A");
        assert!(!service.discovered.lock().unwrap().contains_key("B"));
        assert!(service.discovered.lock().unwrap().contains_key("C"));
        assert_eq!(server.join().unwrap(), ["DELETE /v2/session/B HTTP/1.1"]);
    }

    #[test]
    fn deletion_rejection_does_not_forget_the_active_seat() {
        let (base, server) = session_server(
            vec![(200, json!({"requestStatus":{"statusCode":4}}))],
            |_| {},
        );
        let client = Client::new();
        let service = CloudMatchService::new(client.clone());
        let mut active = json!({"sessionId":"A","status":3});
        service
            .store_active(&mut active, &base, "", "123", client.clone())
            .unwrap();
        assert_eq!(
            service
                .stop_at(
                    "A",
                    &client,
                    base.join("v2/session/A").unwrap(),
                    HeaderMap::new()
                )
                .unwrap_err()
                .code,
            "session_error"
        );
        assert_eq!(service.active()["session"]["sessionId"], "A");
        assert_eq!(server.join().unwrap(), ["DELETE /v2/session/A HTTP/1.1"]);
    }

    #[test]
    fn pending_cleanup_survives_restart_and_is_scoped_to_original_account() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("pending-session-cleanup.json");
        let auth = conflict_auth();
        let record = json!({"sessionId":"cancelled","appId":"123","status":1,"phase":"preparing",
            "streamingBaseUrl":DEFAULT_STREAMING_BASE,"cleanupPending":true,
            "owner":[auth.provider.idp_id,auth.user.user_id]});
        std::fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
        let service = CloudMatchService::with_cleanup_path(Client::new(), path.clone());
        let pending = service.pending_cleanup(&auth).unwrap();
        assert_eq!(pending["sessionId"], "cancelled");
        assert!(pending.get("owner").is_none());
        let mut other = auth.clone();
        other.user.user_id = "other".to_owned();
        assert!(service.pending_cleanup(&other).is_none());
        assert_eq!(
            service
                .create(&json!({"appId":"123"}), &json!({}), &auth, "device")
                .unwrap_err()
                .code,
            "session_cleanup_pending"
        );
        service.clear_cleanup("other-seat");
        assert!(path.exists());
        service.clear_cleanup("cancelled");
        assert!(!path.exists());
        assert!(service.pending_cleanup(&auth).is_none());
    }

    #[test]
    fn persisted_cleanup_cannot_redirect_token_to_unverified_partner_host() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("pending-session-cleanup.json");
        let auth = conflict_auth();
        let record = json!({"sessionId":"seat","streamingBaseUrl":"https://attacker.example/",
            "owner":[auth.provider.idp_id,auth.user.user_id]});
        std::fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
        let service = CloudMatchService::with_cleanup_path(Client::new(), path);
        let pending = service.pending_cleanup(&auth).unwrap();
        assert!(pending["streamingBaseUrl"].is_null());
        assert_eq!(pending["cleanupEndpointUnverified"], true);
        assert_eq!(
            service.admit_create().err().unwrap().code,
            "session_cleanup_pending"
        );
    }

    #[test]
    fn persisted_cleanup_stop_never_uses_marker_or_synthetic_endpoint() {
        let auth = conflict_auth();
        for (id, marker_url) in [
            ("seat", DEFAULT_STREAMING_BASE),
            ("seat/other?forged=1", "https://attacker.example/"),
        ] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("pending-session-cleanup.json");
            let record = json!({"sessionId":id,"streamingBaseUrl":marker_url,
                "owner":[auth.provider.idp_id,auth.user.user_id]});
            std::fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
            let service = CloudMatchService::with_cleanup_path(Client::new(), path.clone());
            service.store_discovered(&[json!({"sessionId":id,
                "serverIp":"seat.nvidiagrid.net","streamingBaseUrl":marker_url})]);
            assert_eq!(
                service
                    .stop(&json!({"sessionId":id}), &json!({}), &auth, "device")
                    .unwrap_err()
                    .code,
                "session_cleanup_pending"
            );
            assert!(path.exists());
            assert_eq!(
                service.admit_create().err().unwrap().code,
                "session_cleanup_pending"
            );
        }
    }

    #[test]
    fn targeted_not_found_is_distinct_from_authentication_and_invalid_payloads() {
        for (status, body, code) in [
            (404, json!({}), "session_not_found"),
            (401, json!({}), "http_unauthorized"),
            (403, json!({}), "authentication_required"),
            (
                200,
                json!({"requestStatus":{"statusCode":32}}),
                "session_error",
            ),
        ] {
            let (base, server) = session_server(vec![(status, body)], |_| {});
            let client = Client::new();
            let service = CloudMatchService::new(client.clone());
            assert_eq!(
                service
                    .get_session(&client, &base, "seat", &HeaderMap::new())
                    .unwrap_err()
                    .code,
                code
            );
            assert_eq!(server.join().unwrap(), ["GET /v2/session/seat HTTP/1.1"]);
        }
    }

    #[test]
    fn same_seat_partial_finalized_color_preserves_components_not_preferences() {
        let base = trusted_cloudmatch_base(DEFAULT_STREAMING_BASE).unwrap();
        let parse = |session| {
            session_info(&json!({"session":session}), &base, "", "123", "device").unwrap()
        };
        for (color, depth, chroma) in [("8bit_420", 0, 0), ("10bit_420", 1, 0), ("10bit_444", 1, 1)]
        {
            let body = build_create_body(
                "123",
                &json!({}),
                &json!({"codec":"h265", "colorQuality":color}),
                "device",
            );
            assert_eq!(
                body["sessionRequestData"]["requestedStreamingFeatures"]["bitDepth"],
                depth
            );
            assert_eq!(
                body["sessionRequestData"]["requestedStreamingFeatures"]["chromaFormat"],
                chroma
            );
            let previous = parse(json!({"sessionId":"seat","status":2,"sdrHdrMode":1,
                "finalizedStreamingFeatures":{"codec":2,"bitDepth":depth,"chromaFormat":chroma}}));
            let mut partial = parse(json!({"sessionId":"seat","status":2}));
            preserve_session_profile(&mut partial, &previous);
            assert_eq!(partial["negotiatedStreamProfile"]["colorQuality"], color);
            assert_eq!(partial["negotiatedStreamProfile"]["enableHdr"], true);
            let mut sdr = partial.clone();
            sdr["negotiatedStreamProfile"]["enableHdr"] = json!(false);
            let prepared = crate::streamer::StreamerService::new().prepare_embedded(
                &json!({"session":sdr,"runtimeCapabilities":{"protocolVersion":7,"videoBackends":[{
                    "backend":"vaapi","platform":"linux","available":true,"codecs":[{
                        "codec":"h265","available":true,"colorQualities":["8bit_420","10bit_420","10bit_444"]
                    }]
                }]}}), &json!({"codec":"h265","colorQuality":"8bit_420"}),
            ).unwrap();
            assert_eq!(prepared["context"]["settings"]["colorQuality"], color);
            assert_eq!(
                prepared["context"]["session"]["negotiatedStreamProfile"]["colorQuality"],
                color
            );
            let mut downgrade = parse(json!({"sessionId":"seat","status":2,"sdrHdrMode":0,
                "finalizedStreamingFeatures":{"bitDepth":0}}));
            preserve_session_profile(&mut downgrade, &previous);
            assert_eq!(downgrade["negotiatedStreamProfile"]["bitDepth"], 8);
            assert_eq!(downgrade["negotiatedStreamProfile"]["chromaFormat"], chroma);
            assert_eq!(downgrade["negotiatedStreamProfile"]["enableHdr"], false);
            let mut chroma_only = parse(json!({"sessionId":"seat","status":2,
                "finalizedStreamingFeatures":{"chromaFormat":0}}));
            preserve_session_profile(&mut chroma_only, &previous);
            assert_eq!(chroma_only["negotiatedStreamProfile"]["chromaFormat"], 0);
            assert_eq!(
                chroma_only["negotiatedStreamProfile"]["bitDepth"],
                if depth == 1 { 10 } else { 8 }
            );
            let mut invalid = parse(json!({"sessionId":"seat","status":2,
                "finalizedStreamingFeatures":{"bitDepth":99}}));
            preserve_session_profile(&mut invalid, &previous);
            assert!(invalid["negotiatedStreamProfile"]["colorQuality"].is_null());
            let mut other = parse(json!({"sessionId":"other","status":2}));
            preserve_session_profile(&mut other, &previous);
            assert!(other["negotiatedStreamProfile"]["colorQuality"].is_null());
        }
    }

    fn conflict_auth() -> AuthSession {
        serde_json::from_value(json!({
            "provider":{"idpId":"provider", "code":"NVIDIA", "displayName":"NVIDIA", "streamingServiceUrl":DEFAULT_STREAMING_BASE, "priority":0},
            "tokens":{"accessToken":"test-token", "expiresAt":0, "authClientId":"test"},
            "user":{"userId":"test-user", "displayName":"Test", "membershipTier":""}
        })).unwrap()
    }

    fn conflict_payload() -> Value {
        json!({
            "requestStatus":{"statusCode":11,"statusDescription":"SESSION_LIMIT_PER_DEVICE_EXCEEDED_STATUS 4AF1201E"},
            "otherUserSessions":[{
                "sessionId":"existing-seat", "status":5,
                "sessionRequestData":{"appId":456},
                "sessionControlInfo":{"ip":"seat.nvidiagrid.net"}
            }]
        })
    }

    #[test]
    fn create_conflicts_preserve_resumable_details_for_one_discovery() {
        for status in [200, 400, 403, 409, 500] {
            let service = CloudMatchService::new(Client::new());
            let auth = conflict_auth();
            let base = trusted_cloudmatch_base(DEFAULT_STREAMING_BASE).unwrap();
            let error = service
                .capture_session_conflict(
                    reqwest::StatusCode::from_u16(status).unwrap(),
                    &conflict_payload(),
                    &base,
                    &auth,
                )
                .unwrap();
            assert_eq!(error.code, "session_conflict");
            assert!(!error.message.contains("4AF1201E"));
            assert!(service.active()["session"].is_null());
            let response = service
                .remote_sessions(&json!({}), &json!({}), &auth, "device")
                .unwrap();
            assert_eq!(response["sessions"][0]["sessionId"], "existing-seat");
            assert_eq!(response["sessions"][0]["appId"], 456);
            assert_eq!(response["sessions"][0]["status"], 5);
            assert_eq!(response["sessions"][0]["serverIp"], "seat.nvidiagrid.net");
            assert_eq!(
                response["sessions"][0]["streamingBaseUrl"],
                base.origin().ascii_serialization()
            );
            assert!(
                service
                    .discovered
                    .lock()
                    .unwrap()
                    .contains_key("existing-seat")
            );
            assert!(service.take_conflict_sessions(&auth).is_none());
        }
    }

    #[test]
    fn conflict_handoff_claims_the_existing_host_instead_of_the_create_region() {
        let service = CloudMatchService::new(Client::new());
        let auth = conflict_auth();
        let create_region = trusted_cloudmatch_base("https://create.nvidiagrid.net").unwrap();
        for host in ["other-region-seat.nvidiagrid.net", "80.84.160.10"] {
            let mut payload = conflict_payload();
            payload["otherUserSessions"][0]["sessionControlInfo"]["ip"] = json!(host);
            service
                .capture_session_conflict(
                    reqwest::StatusCode::FORBIDDEN,
                    &payload,
                    &create_region,
                    &auth,
                )
                .unwrap();
            service
                .remote_sessions(&json!({}), &json!({}), &auth, "device")
                .unwrap();
            let discovered = service.discovered.lock().unwrap();
            let session = discovered.get("existing-seat");
            assert_eq!(
                claim_lookup_base(session, &create_region),
                trusted_learned_server_base(host).unwrap()
            );
        }
        for session in [
            None,
            Some(json!({})),
            Some(json!({"serverIp":"localhost"})),
            Some(json!({"serverIp":"https://127.0.0.1"})),
        ] {
            assert_eq!(
                claim_lookup_base(session.as_ref(), &create_region),
                create_region
            );
        }
        assert_eq!(
            claim_lookup_base(
                Some(&json!({"serverIp":"https://partner.example.com"})),
                &create_region
            ),
            trusted_cloudmatch_base("https://partner.example.com").unwrap()
        );
    }

    #[test]
    fn conflict_handoff_expires_and_is_scoped_to_the_account() {
        let service = CloudMatchService::new(Client::new());
        let auth = conflict_auth();
        let base = trusted_cloudmatch_base(DEFAULT_STREAMING_BASE).unwrap();
        for different_account in [false, true] {
            service
                .capture_session_conflict(
                    reqwest::StatusCode::BAD_REQUEST,
                    &conflict_payload(),
                    &base,
                    &auth,
                )
                .unwrap();
            let mut next_auth = auth.clone();
            if different_account {
                next_auth.user.user_id = "other-user".to_owned();
            } else {
                service.conflict.lock().unwrap().as_mut().unwrap().received =
                    Instant::now() - Duration::from_secs(31);
            }
            assert!(service.take_conflict_sessions(&next_auth).is_none());
            assert!(service.take_conflict_sessions(&auth).is_none());
        }
    }

    #[test]
    fn conflict_detection_supports_vendor_codes_and_preserves_unauthorized_responses() {
        for payload in [
            json!({"requestStatus":{"statusCode":"11"}}),
            json!({"requestStatus":{"statusDescription":"SESSION_LIMIT_PER_DEVICE_EXCEEDED_STATUS"}}),
            json!({"requestStatus":{"unifiedErrorCode":"4AF1201E"}}),
            json!({"session":{"errorCode":0x4AF1201E_i64}}),
        ] {
            assert!(is_session_conflict(&payload));
            let service = CloudMatchService::new(Client::new());
            let auth = conflict_auth();
            let base = trusted_cloudmatch_base(DEFAULT_STREAMING_BASE).unwrap();
            let status = reqwest::StatusCode::UNAUTHORIZED;
            assert!(
                service
                    .capture_session_conflict(status, &payload, &base, &auth)
                    .is_none()
            );
            let error = validate_cloudmatch_response("create", status, Ok(payload.clone()), false)
                .unwrap_err();
            assert_eq!(error.code, "http_unauthorized");
        }
        assert!(!is_session_conflict(
            &json!({"requestStatus":{"statusCode":4,"statusDescription":"INTERNAL_ERROR_STATUS"}})
        ));
    }

    #[test]
    fn forbidden_session_limit_is_a_conflict_but_unrecognized_forbidden_is_authentication() {
        let service = CloudMatchService::new(Client::new());
        let auth = conflict_auth();
        let base = trusted_cloudmatch_base(DEFAULT_STREAMING_BASE).unwrap();
        let status = reqwest::StatusCode::FORBIDDEN;
        let payload = json!({"requestStatus":{"statusDescription":"SESSION_LIMIT_PER_DEVICE_EXCEEDED_STATUS 4AF1201E"}});
        assert_eq!(
            service
                .capture_session_conflict(status, &payload, &base, &auth)
                .unwrap()
                .code,
            "session_conflict"
        );
        for payload in [
            json!({"requestStatus":{"statusDescription":"Forbidden"}}),
            json!({}),
            json!("SESSION_LIMIT_PER_DEVICE_EXCEEDED_STATUS 4AF1201E"),
        ] {
            assert!(
                service
                    .capture_session_conflict(status, &payload, &base, &auth)
                    .is_none()
            );
            assert_eq!(
                validate_cloudmatch_response("create", status, Ok(payload), false)
                    .unwrap_err()
                    .code,
                "authentication_required"
            );
        }
        assert_eq!(
            read_cloudmatch_response("create", cloudmatch_response(403, "Forbidden"), false)
                .unwrap_err()
                .code,
            "authentication_required"
        );
    }

    #[test]
    fn conflict_handoff_rejects_unusable_seats_and_accepts_signaling_resource_paths() {
        let service = CloudMatchService::new(Client::new());
        let auth = conflict_auth();
        let base = trusted_cloudmatch_base(DEFAULT_STREAMING_BASE).unwrap();
        for patch in [
            json!({"sessionId":""}),
            json!({"status":7}),
            json!({"sessionRequestData":{"appId":0}}),
            json!({"sessionControlInfo":{"ip":"localhost"}}),
        ] {
            let mut payload = conflict_payload();
            for (key, value) in patch.as_object().unwrap() {
                payload["otherUserSessions"][0][key] = value.clone();
            }
            assert!(
                service
                    .capture_session_conflict(
                        reqwest::StatusCode::BAD_REQUEST,
                        &payload,
                        &base,
                        &auth
                    )
                    .is_some()
            );
            assert!(service.take_conflict_sessions(&auth).is_none());
        }
        let mut payload = conflict_payload();
        payload["otherUserSessions"][0]["sessionControlInfo"] = Value::Null;
        payload["otherUserSessions"][0]["connectionInfo"] =
            json!([{"usage":14,"resourcePath":"wss://signal.nvidiagrid.net/nvst/"}]);
        service
            .capture_session_conflict(reqwest::StatusCode::BAD_REQUEST, &payload, &base, &auth)
            .unwrap();
        assert_eq!(
            service.take_conflict_sessions(&auth).unwrap()[0]["serverIp"],
            "signal.nvidiagrid.net"
        );
    }

    #[test]
    fn discovery_continues_after_empty_regions_and_deduplicates_sessions() {
        let bases = [
            "https://first.nvidiagrid.net",
            "https://second.nvidiagrid.net",
            "https://third.nvidiagrid.net",
        ]
        .map(|url| trusted_cloudmatch_base(url).unwrap());
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let sessions = discover_sessions(
            &bases,
            Instant::now() + DISCOVERY_TIMEOUT,
            false,
            |base, timeout| {
                calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                assert!(timeout <= DISCOVERY_REQUEST_TIMEOUT);
                if base == &bases[0] {
                    Ok(vec![])
                } else {
                    Ok(vec![
                        json!({"sessionId":"seat", "streamingBaseUrl":base.as_str()}),
                    ])
                }
            },
        )
        .unwrap();
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 3);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0]["streamingBaseUrl"], bases[1].as_str());
    }

    #[test]
    fn discovery_reports_incomplete_absence_but_keeps_found_sessions() {
        let bases = [
            trusted_cloudmatch_base(DEFAULT_STREAMING_BASE).unwrap(),
            trusted_cloudmatch_base("https://region.nvidiagrid.net").unwrap(),
        ];
        for found in [false, true] {
            let result = discover_sessions(
                &bases,
                Instant::now() + DISCOVERY_TIMEOUT,
                false,
                |base, _| {
                    if base == &bases[0] {
                        Err(upstream("failed region"))
                    } else {
                        Ok(if found {
                            vec![json!({"sessionId":"seat"})]
                        } else {
                            vec![]
                        })
                    }
                },
            );
            if found {
                assert_eq!(result.unwrap().len(), 1);
            } else {
                assert_eq!(result.unwrap_err().code, "session_discovery_failed");
            }
        }
        assert!(
            discover_sessions(
                &bases,
                Instant::now() + DISCOVERY_TIMEOUT,
                false,
                |_, _| Ok(vec![])
            )
            .unwrap()
            .is_empty()
        );
        assert_eq!(
            discover_sessions(&bases, Instant::now() + DISCOVERY_TIMEOUT, true, |_, _| Ok(
                vec![]
            ))
            .unwrap_err()
            .code,
            "session_discovery_failed"
        );
        assert_eq!(
            discover_sessions(&bases, Instant::now() + DISCOVERY_TIMEOUT, false, |_, _| {
                Err(upstream("failed"))
            })
            .unwrap_err()
            .code,
            "session_discovery_failed"
        );
    }

    #[test]
    fn discovery_respects_deadline_and_concurrency_bound() {
        let base = trusted_cloudmatch_base(DEFAULT_STREAMING_BASE).unwrap();
        assert_eq!(
            discover_sessions(
                std::slice::from_ref(&base),
                Instant::now(),
                false,
                |_, _| panic!("expired search must not send requests")
            )
            .unwrap_err()
            .code,
            "session_discovery_failed"
        );
        let active = std::sync::atomic::AtomicUsize::new(0);
        let peak = std::sync::atomic::AtomicUsize::new(0);
        let barrier = std::sync::Barrier::new(DISCOVERY_CONCURRENCY);
        discover_sessions(
            &vec![base; 8],
            Instant::now() + DISCOVERY_TIMEOUT,
            false,
            |_, _| {
                let current = active.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                peak.fetch_max(current, std::sync::atomic::Ordering::SeqCst);
                barrier.wait();
                active.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                Ok(vec![])
            },
        )
        .unwrap();
        assert_eq!(
            peak.load(std::sync::atomic::Ordering::SeqCst),
            DISCOVERY_CONCURRENCY
        );
    }

    #[test]
    fn discovery_preserves_authentication_failures_and_cancellation() {
        let bases = [trusted_cloudmatch_base(DEFAULT_STREAMING_BASE).unwrap()];
        let error = discover_sessions(&bases, Instant::now() + DISCOVERY_TIMEOUT, false, |_, _| {
            Err(ServiceError {
                code: "authentication_required",
                message: "Expired credentials".to_owned(),
            })
        })
        .unwrap_err();
        assert_eq!(error.code, "authentication_required");

        let requests = std::sync::Arc::new(crate::requests::Requests::default());
        let permit = requests.admit("discovery", "session.remote.list").unwrap();
        requests.cancel("discovery");
        let error = crate::requests::scope(permit.token.clone(), || {
            discover_sessions(&bases, Instant::now() + DISCOVERY_TIMEOUT, false, |_, _| {
                panic!("cancelled discovery must not send requests")
            })
        })
        .unwrap_err();
        assert_eq!(error.code, "cancelled");
    }

    fn cloudmatch_response(status: u16, body: &str) -> Response {
        use std::io::{BufRead, BufReader, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let body = body.to_owned();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(&stream);
            loop {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
            }
            write!(stream,
                "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()).unwrap();
        });
        let response = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap()
            .get(format!("http://{address}"))
            .send()
            .unwrap();
        server.join().unwrap();
        response
    }

    #[test]
    fn resume_claims_paused_and_live_seats_but_only_polls_transitions() {
        for status in [2, 3, 4, 5] {
            assert!(session_requires_resume(status).unwrap(), "status {status}");
        }
        for status in [1, 6] {
            assert!(!session_requires_resume(status).unwrap(), "status {status}");
        }
        for status in [0, 7, 8, -1] {
            assert!(session_requires_resume(status).is_err(), "status {status}");
        }
        assert_eq!(session_phase(4), "paused");
        assert_eq!(session_phase(5), "paused");
        assert_eq!(session_phase(6), "resuming");
    }

    #[test]
    fn resume_discovery_keeps_paused_and_resuming_seats() {
        let base = trusted_cloudmatch_base(DEFAULT_STREAMING_BASE).unwrap();
        for status in 0..=8 {
            let session = json!({"sessionId":"seat", "status":status});
            let info = remote_session_info(&session, &base);
            assert_eq!(info.is_some(), (1..=6).contains(&status));
            if let Some(info) = info {
                assert_eq!(info["status"], status);
                assert_eq!(info["phase"], session_phase(status));
            }
        }
    }

    #[test]
    fn resume_not_paused_response_continues_polling_for_http_and_api_rejections() {
        for status in [200, 400, 409, 500] {
            for request_status in [
                json!({"statusCode":34}),
                json!({"statusCode":"34"}),
                json!({"statusCode":0,"statusDescription":"SESSION_NOT_PAUSED"}),
            ] {
                let body = json!({"requestStatus":request_status}).to_string();
                let result = read_cloudmatch_response(
                    "Session claim failed",
                    cloudmatch_response(status, &body),
                    true,
                );
                assert!(result.is_ok(), "HTTP {status}: {body}");
                let result = read_cloudmatch_response(
                    "Session polling failed",
                    cloudmatch_response(status, &body),
                    false,
                );
                assert!(
                    result.is_err(),
                    "poll must not accept HTTP {status}: {body}"
                );
            }
        }
    }

    #[test]
    fn resume_response_preserves_other_failures_and_success() {
        for status in [401, 403] {
            let error = read_cloudmatch_response(
                "Session claim failed",
                cloudmatch_response(status, r#"{"requestStatus":{"statusCode":34}}"#),
                true,
            )
            .unwrap_err();
            assert_eq!(
                error.code,
                if status == 401 {
                    "http_unauthorized"
                } else {
                    "authentication_required"
                }
            );
        }
        for (status, body, code) in [
            (
                200,
                r#"{"requestStatus":{"statusCode":32,"statusDescription":"SESSION_EXPIRED"}}"#,
                "session_error",
            ),
            (
                409,
                r#"{"requestStatus":{"statusCode":32,"statusDescription":"SESSION_EXPIRED"}}"#,
                "upstream_error",
            ),
            (502, "not JSON", "upstream_error"),
        ] {
            let error = read_cloudmatch_response(
                "Session claim failed",
                cloudmatch_response(status, body),
                true,
            )
            .unwrap_err();
            assert_eq!(error.code, code);
        }
        let response = read_cloudmatch_response(
            "Session claim failed",
            cloudmatch_response(200, r#"{"requestStatus":{"statusCode":1}}"#),
            true,
        )
        .unwrap();
        assert_eq!(response["requestStatus"]["statusCode"], 1);
    }

    #[test]
    fn resume_poll_preserves_paused_progress_and_stops_on_terminal_states() {
        for status in [4, 5] {
            let mut info = json!({"status":status, "phase":session_phase(status),
                "rtspsEndpoints":["rtsps://example.invalid:322"]});
            mark_resume_progress(&mut info);
            assert_eq!(info["resumePending"], true);
            assert_eq!(info["phase"], "resuming");
        }
        for status in [0, 7, 8] {
            let mut info = json!({"status":status, "resumePending":true,
                "rtspsEndpoints":["rtsps://example.invalid:322"]});
            mark_resume_progress(&mut info);
            assert_eq!(info["resumePending"], false);
            assert_eq!(
                info["phase"],
                if status == 7 { "finished" } else { "failed" }
            );
        }
    }

    #[test]
    fn direct_resume_poll_retains_regional_discovery_endpoint() {
        let direct = trusted_learned_server_base("80.84.160.10").unwrap();
        let payload = json!({"session": {"sessionId":"resumed-seat", "status":2,
            "connectionInfo":[{"usage":14, "ip":"80.84.160.10"},
                {"usage":16, "ip":"80.84.160.10", "port":322}]}});
        let info = session_info(
            &payload,
            &direct,
            "np-sof-01.cloudmatchbeta.nvidiagrid.net",
            "123",
            "device",
        )
        .unwrap();
        assert_eq!(
            info["streamingBaseUrl"],
            direct.origin().ascii_serialization()
        );
        assert_eq!(info["serverIp"], "80.84.160.10");
        assert_eq!(info["rtspsEndpoints"][0], "rtsps://80.84.160.10:322");
        assert!(trusted_cloudmatch_base(direct.as_str()).is_ok());
    }

    #[test]
    fn hdr_444_request_and_accepted_session_preserve_wire_chroma() {
        let capabilities = json!({"protocolVersion":7,"nativeHdrSupported":true,"videoBackends":[{
            "backend":"d3d11","available":true,"codecs":[
                {"codec":"h265","available":true,"hdrSupported":true,
                    "colorQualities":["10bit_444"],"hdrColorQualities":["10bit_444"]}
            ]
        }]});
        for color in ["8bit_444", "10bit_444"] {
            let settings = crate::streamer::StreamerService::embedded_session_settings(
                &json!({"codec":"h265","enableHdr":true,"colorQuality":color}),
                &capabilities,
            )
            .unwrap();
            let body = build_create_body("123", &json!({}), &settings, "device");
            let request = &body["sessionRequestData"];
            assert_eq!(request["sdrHdrMode"], 1);
            assert_eq!(request["clientRequestMonitorSettings"][0]["sdrHdrMode"], 1);
            assert_eq!(request["requestedStreamingFeatures"]["trueHdr"], false);
            assert_eq!(request["requestedStreamingFeatures"]["bitDepth"], 1);
            assert_eq!(request["requestedStreamingFeatures"]["chromaFormat"], 1);
        }
        let payload = json!({"session":{"sessionId":"hdr-444","status":2,"sdrHdrMode":1,
            "finalizedStreamingFeatures":{"codec":2,"bitDepth":1,"chromaFormat":1}}});
        let base = trusted_cloudmatch_base(DEFAULT_STREAMING_BASE).unwrap();
        let info = session_info(&payload, &base, "auto", "123", "device").unwrap();
        assert_eq!(info["negotiatedStreamProfile"]["colorQuality"], "10bit_444");
        assert_eq!(info["negotiatedStreamProfile"]["enableHdr"], true);
        let prepared = crate::streamer::StreamerService::new()
            .prepare_embedded(
                &json!({"session":info,"runtimeCapabilities":capabilities}),
                &json!({"codec":"auto","colorQuality":"8bit_420","enableHdr":false}),
            )
            .unwrap();
        assert_eq!(prepared["context"]["settings"]["colorQuality"], "10bit_444");
        assert_eq!(prepared["context"]["settings"]["enableHdr"], true);
    }

    #[test]
    fn hdr_request_requires_resolved_runtime_opt_in_and_uses_cloudmatch_enums() {
        let capabilities = json!({"protocolVersion":7,"nativeHdrSupported":true,"videoBackends":[{
            "backend":"d3d11","available":true,"codecs":[
                {"codec":"h265","available":true,"colorQualities":["8bit_420","10bit_420"]}
            ]
        }]});
        let settings = crate::streamer::StreamerService::embedded_session_settings(
            &json!({"enableHdr":true}),
            &capabilities,
        )
        .unwrap();
        let body = build_create_body("123", &json!({}), &settings, "device");
        let request = &body["sessionRequestData"];
        assert_eq!(request["sdrHdrMode"], 1);
        assert_eq!(request["clientRequestMonitorSettings"][0]["sdrHdrMode"], 1);
        assert_eq!(request["requestedStreamingFeatures"]["trueHdr"], false);
        assert_eq!(request["requestedStreamingFeatures"]["bitDepth"], 1);
        assert_eq!(request["requestedStreamingFeatures"]["chromaFormat"], 0);
        let display_data = &request["clientRequestMonitorSettings"][0]["displayData"];
        assert_eq!(display_data["desiredContentMaxLuminance"], 1000);
        assert_eq!(display_data["desiredContentMaxFrameAverageLuminance"], 400);
        assert_eq!(display_data["desiredContentMinLuminance"], 0);
        for settings in [
            json!({}),
            json!({"codec":"h265","enableHdr":true}),
            json!({"codec":"h265","nativeHdrSupported":true}),
            json!({"codec":"h264","enableHdr":true,"nativeHdrSupported":true}),
            json!({"codec":"h265","enableHdr":true,"nativeHdrSupported":true,"decoderPreference":"software"}),
        ] {
            let body = build_create_body("123", &json!({}), &settings, "device");
            assert_eq!(body["sessionRequestData"]["sdrHdrMode"], 0);
            let display_data =
                &body["sessionRequestData"]["clientRequestMonitorSettings"][0]["displayData"];
            assert!(display_data.is_null());
            assert_eq!(
                body["sessionRequestData"]["requestedStreamingFeatures"]["trueHdr"],
                false
            );
        }
    }

    #[test]
    fn validated_display_luminance_replaces_requested_content_defaults() {
        let capabilities = json!({"protocolVersion":7,"nativeHdrSupported":true,"videoBackends":[{
            "backend":"vaapi","available":true,"codecs":[
                {"codec":"h265","available":true,"colorQualities":["8bit_420","10bit_420"]}
            ]
        }],"nativeHdrDisplay":{"minimumNits":0.005,"maximumNits":620}});
        let settings = crate::streamer::StreamerService::embedded_session_settings(
            &json!({"enableHdr":true}),
            &capabilities,
        )
        .unwrap();
        let body = build_create_body("123", &json!({}), &settings, "device");
        let display_data =
            &body["sessionRequestData"]["clientRequestMonitorSettings"][0]["displayData"];
        assert_eq!(display_data["desiredContentMaxLuminance"], 620.0);
        assert_eq!(display_data["desiredContentMinLuminance"], 50);
        assert!(
            display_data
                .get("desiredContentMaxFrameAverageLuminance")
                .is_none()
        );
        assert!(display_data.get("displayPrimaryX0").is_none());
        assert!(display_data.get("displayWhitePointY").is_none());
        assert_eq!(body["sessionRequestData"]["sdrHdrMode"], 1);
        let sdr = build_create_body(
            "123",
            &json!({}),
            &json!({"enableHdr":false,"nativeHdrDisplay":{"minimumNits":0.005,"maximumNits":620}}),
            "device",
        );
        let sdr_data = &sdr["sessionRequestData"]["clientRequestMonitorSettings"][0]["displayData"];
        assert!(sdr_data.is_null());
    }

    #[test]
    fn measured_monitor_metadata_survives_resolution_and_uses_wire_units() {
        for peak in [400, 620, 1068] {
            let full_frame = if peak == 620 { peak } else { peak - 80 };
            let display = json!({"minimumNits":0.005,"maximumNits":peak,
                "maximumFullFrameNits":full_frame,
                "redX":0.64,"redY":0.33,"greenX":0.30,"greenY":0.60,
                "blueX":0.15,"blueY":0.06,"whiteX":0.3127,"whiteY":0.329});
            let capabilities = json!({"protocolVersion":7,"nativeHdrSupported":true,"videoBackends":[{
                "backend":"vaapi","available":true,"codecs":[
                    {"codec":"h265","available":true,"colorQualities":["8bit_420","10bit_420"]}
                ]}],"nativeHdrDisplay":display});
            let resolved = crate::streamer::StreamerService::embedded_session_settings(
                &json!({"codec":"h265","enableHdr":true,"nativeHdrDisplay":{
                    "minimumNits":0.0,"maximumNits":1000,"maximumFullFrameNits":400,
                    "redX":0.7}}),
                &capabilities,
            )
            .unwrap();
            for (key, value) in display.as_object().unwrap() {
                assert_eq!(
                    resolved["nativeHdrDisplay"][key].as_f64(),
                    value.as_f64(),
                    "{key}"
                );
            }
            let body = build_create_body("123", &json!({}), &resolved, "device");
            let request = &body["sessionRequestData"];
            let data = &request["clientRequestMonitorSettings"][0]["displayData"];
            assert_eq!(request["requestedStreamingFeatures"]["trueHdr"], false);
            assert_eq!(
                data["desiredContentMaxLuminance"].as_f64(),
                Some(peak as f64)
            );
            assert_eq!(data["desiredContentMinLuminance"], 50);
            assert_eq!(
                data["desiredContentMaxFrameAverageLuminance"].as_f64(),
                Some(full_frame as f64)
            );
            for (key, expected) in [
                ("displayPrimaryX0", 32000),
                ("displayPrimaryY0", 16500),
                ("displayPrimaryX1", 15000),
                ("displayPrimaryY1", 30000),
                ("displayPrimaryX2", 7500),
                ("displayPrimaryY2", 3000),
                ("displayWhitePointX", 15635),
                ("displayWhitePointY", 16450),
            ] {
                assert_eq!(data[key], expected, "{key} at {peak} nits");
            }
            let sdr = build_create_body(
                "123",
                &json!({}),
                &json!({"codec":"h265",
                "enableHdr":false,"nativeHdrSupported":true,"nativeHdrDisplay":display}),
                "device",
            );
            assert!(
                sdr["sessionRequestData"]["clientRequestMonitorSettings"][0]["displayData"]
                    .is_null()
            );
            assert_eq!(
                sdr["sessionRequestData"]["requestedStreamingFeatures"]["trueHdr"],
                false
            );
        }
    }

    #[test]
    fn incomplete_or_invalid_monitor_metadata_keeps_only_valid_luminance() {
        let valid = json!({"minimumNits":0.005,"maximumNits":620,
            "maximumFullFrameNits":400,
            "redX":0.64,"redY":0.33,"greenX":0.30,"greenY":0.60,
            "blueX":0.15,"blueY":0.06,"whiteX":0.3127,"whiteY":0.329});
        let capabilities = |display: Value| {
            json!({"protocolVersion":7,"nativeHdrSupported":true,
            "videoBackends":[{"backend":"vaapi","available":true,"codecs":[
                {"codec":"h265","available":true,"colorQualities":["8bit_420","10bit_420"]}
            ]}],"nativeHdrDisplay":display})
        };
        for (key, value) in [
            ("maximumFullFrameNits", json!(0.005)),
            ("maximumFullFrameNits", json!(621)),
            ("maximumFullFrameNits", json!(null)),
            ("redX", json!(1.1)),
            ("redY", json!(-0.1)),
            ("greenX", json!("0.3")),
            ("blueY", json!(null)),
            ("whiteX", json!(0.9)),
            ("whiteY", json!(0.0)),
            ("greenX", json!(0.64)),
            ("greenX", json!(0.640001)),
        ] {
            let mut display = valid.clone();
            display[key] = value;
            if key == "blueY" {
                display.as_object_mut().unwrap().remove(key);
            }
            if key == "greenX" && display[key] == json!(0.64) {
                display["greenY"] = json!(0.33);
            }
            if key == "greenX" && display[key] == json!(0.640001) {
                display["greenY"] = json!(0.330001);
            }
            let resolved = crate::streamer::StreamerService::embedded_session_settings(
                &json!({"codec":"h265","enableHdr":true,"nativeHdrDisplay":valid}),
                &capabilities(display),
            )
            .unwrap();
            assert_eq!(
                resolved["nativeHdrDisplay"],
                json!({"minimumNits":0.005,"maximumNits":620.0}),
                "{key}"
            );
            let body = build_create_body("123", &json!({}), &resolved, "device");
            let data =
                &body["sessionRequestData"]["clientRequestMonitorSettings"][0]["displayData"];
            assert_eq!(data["desiredContentMaxLuminance"].as_f64(), Some(620.0));
            assert_eq!(data["desiredContentMinLuminance"], 50);
            assert!(data.get("desiredContentMaxFrameAverageLuminance").is_none());
            assert!(data.get("displayPrimaryX0").is_none());
            assert!(data.get("displayWhitePointY").is_none());
        }
        let no_display = crate::streamer::StreamerService::embedded_session_settings(
            &json!({"codec":"h265","enableHdr":true,"nativeHdrDisplay":valid}),
            &json!({"protocolVersion":7,"nativeHdrSupported":true,"videoBackends":[{
            "backend":"vaapi","available":true,"codecs":[
                {"codec":"h265","available":true,"colorQualities":["8bit_420","10bit_420"]}
            ]}]}),
        )
        .unwrap();
        assert!(no_display.get("nativeHdrDisplay").is_none());
        let fallback = build_create_body("123", &json!({}), &no_display, "device");
        let data =
            &fallback["sessionRequestData"]["clientRequestMonitorSettings"][0]["displayData"];
        assert_eq!(data["desiredContentMaxLuminance"], 1000);
        assert_eq!(data["desiredContentMinLuminance"], 0);
        assert_eq!(data["desiredContentMaxFrameAverageLuminance"], 400);
    }

    #[test]
    fn malformed_display_luminance_keeps_documented_defaults() {
        for display in [
            json!({"minimumNits":600.0,"maximumNits":400.0}),
            json!({"minimumNits":-1.0,"maximumNits":400.0}),
            json!({"minimumNits":0.0,"maximumNits":10001.0}),
            json!({"minimumNits":0.0}),
            json!({"minimumNits":"0","maximumNits":400.0}),
            json!([0.0, 400.0]),
        ] {
            let settings = json!({"enableHdr":true,"nativeHdrSupported":true,
                "codec":"h265","nativeHdrDisplay":display});
            let body = build_create_body("123", &json!({}), &settings, "device");
            let display_data =
                &body["sessionRequestData"]["clientRequestMonitorSettings"][0]["displayData"];
            assert_eq!(display_data["desiredContentMaxLuminance"], 1000);
            assert_eq!(display_data["desiredContentMinLuminance"], 0);
            assert_eq!(display_data["desiredContentMaxFrameAverageLuminance"], 400);
        }
    }

    #[test]
    fn native_hdr_display_capability_requires_a_validated_pair() {
        let capabilities = |display: Value| {
            json!({"protocolVersion":7,"nativeHdrSupported":true,"videoBackends":[{
                "backend":"vaapi","available":true,"codecs":[
                    {"codec":"h265","available":true,"colorQualities":["8bit_420","10bit_420"]}
                ]
            }],"nativeHdrDisplay":display})
        };
        for (display, minimum, maximum) in [
            (json!({"minimumNits":0.005,"maximumNits":620}), 0.005, 620.0),
            (json!({"minimumNits":0,"maximumNits":400}), 0.0, 400.0),
        ] {
            let resolved = crate::streamer::StreamerService::embedded_session_settings(
                &json!({"enableHdr":true,"nativeHdrSupported":true}),
                &capabilities(display),
            )
            .unwrap();
            assert_eq!(resolved["nativeHdrDisplay"]["minimumNits"], minimum);
            assert_eq!(resolved["nativeHdrDisplay"]["maximumNits"], maximum);
        }
        for display in [
            json!({"minimumNits":400.0,"maximumNits":400.0}),
            json!({"minimumNits":0.0,"maximumNits":10001.0}),
            json!({"maximumNits":620}),
            json!({"minimumNits":0.005}),
            json!("620"),
        ] {
            let resolved = crate::streamer::StreamerService::embedded_session_settings(
                &json!({"enableHdr":true,"nativeHdrSupported":true}),
                &capabilities(display),
            )
            .unwrap();
            assert!(resolved.get("nativeHdrDisplay").is_none());
        }
    }

    #[test]
    fn stale_display_snapshot_is_dropped_across_output_transitions() {
        let capabilities = |display: Value| {
            json!({"protocolVersion":7,"nativeHdrSupported":true,"videoBackends":[{
                "backend":"vaapi","available":true,"codecs":[
                    {"codec":"h265","available":true,"colorQualities":["8bit_420","10bit_420"]}
                ]
            }],"nativeHdrDisplay":display})
        };
        let previous = json!({"enableHdr":true,"nativeHdrSupported":true,"codec":"h265",
            "nativeHdrDisplay":{"minimumNits":0.005,"maximumNits":620}});
        let resolved = crate::streamer::StreamerService::embedded_session_settings(
            &previous,
            &json!({"protocolVersion":7,"nativeHdrSupported":true,"videoBackends":[{
                "backend":"vaapi","available":true,"codecs":[
                    {"codec":"h265","available":true,"colorQualities":["8bit_420","10bit_420"]}
                ]
            }]}),
        )
        .unwrap();
        assert!(resolved.get("nativeHdrDisplay").is_none());
        let body = build_create_body("123", &json!({}), &resolved, "device");
        let display_data =
            &body["sessionRequestData"]["clientRequestMonitorSettings"][0]["displayData"];
        assert_eq!(display_data["desiredContentMaxLuminance"], 1000);
        assert_eq!(display_data["desiredContentMinLuminance"], 0);
        assert_eq!(display_data["desiredContentMaxFrameAverageLuminance"], 400);
        for invalid in [
            json!({"minimumNits":620,"maximumNits":620}),
            json!({"minimumNits":0.005,"maximumNits":10001}),
            json!("unavailable"),
        ] {
            let stale = crate::streamer::StreamerService::embedded_session_settings(
                &previous,
                &capabilities(invalid),
            )
            .unwrap();
            assert!(stale.get("nativeHdrDisplay").is_none());
            let body = build_create_body("123", &json!({}), &stale, "device");
            let display_data =
                &body["sessionRequestData"]["clientRequestMonitorSettings"][0]["displayData"];
            assert_eq!(display_data["desiredContentMaxLuminance"], 1000);
            assert_eq!(display_data["desiredContentMinLuminance"], 0);
            assert_eq!(display_data["desiredContentMaxFrameAverageLuminance"], 400);
        }
        let migrated = crate::streamer::StreamerService::embedded_session_settings(
            &previous,
            &capabilities(json!({"minimumNits":0.0005,"maximumNits":400})),
        )
        .unwrap();
        let body = build_create_body("123", &json!({}), &migrated, "device");
        let display_data =
            &body["sessionRequestData"]["clientRequestMonitorSettings"][0]["displayData"];
        assert_eq!(display_data["desiredContentMaxLuminance"], 400.0);
        assert_eq!(display_data["desiredContentMinLuminance"], 5);
        assert!(
            display_data
                .get("desiredContentMaxFrameAverageLuminance")
                .is_none()
        );
        let resume = build_resume_body(
            "123",
            &json!({"sessionId":"s","status":2,
                "sessionRequestData":{"sdrHdrMode":1,"clientRequestMonitorSettings":[{"sdrHdrMode":1}]}}),
            &resolved,
            "device",
        );
        assert!(resume.to_string().find("desiredContent").is_none());
    }

    #[test]
    fn accepted_hdr_mode_survives_resume_and_explicit_sdr_fallback_wins() {
        let base = trusted_cloudmatch_base(DEFAULT_STREAMING_BASE).unwrap();
        let mut payload = json!({"session":{"sessionId":"hdr-seat","status":2,
            "sessionRequestData":{"sdrHdrMode":1,"clientRequestMonitorSettings":[{"sdrHdrMode":1}]},
            "finalizedStreamingFeatures":{"codec":2,"bitDepth":1,"chromaFormat":0}}});
        let info = session_info(&payload, &base, "auto", "123", "device").unwrap();
        assert_eq!(info["negotiatedStreamProfile"]["enableHdr"], true);
        let resumed = build_resume_body(
            "123",
            &payload["session"],
            &json!({"enableHdr":false}),
            "device",
        );
        assert_eq!(resumed["sessionRequestData"]["sdrHdrMode"], 1);
        assert!(
            resumed["sessionRequestData"]
                .get("clientRequestMonitorSettings")
                .is_none()
        );
        assert!(
            resumed["sessionRequestData"]
                .get("requestedStreamingFeatures")
                .is_none()
        );
        payload["session"]["sdrHdrMode"] = json!(0);
        let info = session_info(&payload, &base, "auto", "123", "device").unwrap();
        assert_eq!(info["negotiatedStreamProfile"]["enableHdr"], false);
        let resumed = build_resume_body(
            "123",
            &payload["session"],
            &json!({"enableHdr":true,"nativeHdrSupported":true}),
            "device",
        );
        assert_eq!(resumed["sessionRequestData"]["sdrHdrMode"], 0);
        assert!(
            resumed["sessionRequestData"]
                .get("clientRequestMonitorSettings")
                .is_none()
        );
        assert!(
            resumed["sessionRequestData"]
                .get("requestedStreamingFeatures")
                .is_none()
        );
        assert_eq!(accepted_hdr_mode(&json!({})), None);
        assert_eq!(accepted_hdr_mode(&json!({"sdrHdrMode":2})), Some(0));
    }

    #[test]
    fn in_game_settings_persistence_defaults_on_and_requires_game_support() {
        for preference in [Value::Null, json!(false), json!(true)] {
            for support in [Value::Null, json!(false), json!(true)] {
                let mut params = json!({});
                let mut settings = json!({});
                if !support.is_null() {
                    params["supportsInGameSettingsPersistence"] = support.clone();
                }
                if !preference.is_null() {
                    settings["enablePersistingInGameSettings"] = preference.clone();
                }
                let body = build_create_body("123", &params, &settings, "stable-device");
                assert_eq!(
                    body["sessionRequestData"]["enablePersistingInGameSettings"],
                    preference != false && support == true
                );
            }
        }
    }

    #[test]
    fn resume_preserves_in_game_settings_persistence_despite_preference_changes() {
        for enabled in [false, true] {
            let original = json!({"sessionRequestData": {
                "enablePersistingInGameSettings": enabled
            }});
            let body = build_resume_body(
                "123",
                &original,
                &json!({"enablePersistingInGameSettings": !enabled}),
                "stable-device",
            );
            assert_eq!(
                body["sessionRequestData"]["enablePersistingInGameSettings"],
                enabled
            );
        }
    }

    #[test]
    fn resume_does_not_renegotiate_allocated_video_parameters() {
        let original = json!({"sessionRequestData": {
            "appLaunchMode":2, "enablePersistingInGameSettings":true,
            "clientPlatformName":"windows"}});
        let body = build_resume_body(
            "123",
            &original,
            &json!({"resolution":"3840x2160", "fps":240, "codec":"av1"}),
            "stable-device",
        );
        assert_eq!(body["action"], 2);
        assert_eq!(body["data"], "RESUME");
        let request = &body["sessionRequestData"];
        assert_eq!(request["deviceHashId"], "stable-device");
        assert_eq!(request["appLaunchMode"], 2);
        assert_eq!(request["enablePersistingInGameSettings"], true);
        assert!(request.get("clientRequestMonitorSettings").is_none());
        assert!(request.get("requestedStreamingFeatures").is_none());
        assert!(
            request["metaData"]
                .as_array()
                .unwrap()
                .iter()
                .all(|entry| entry["key"] != "clientPhysicalResolution")
        );
    }

    #[test]
    fn resume_waits_for_fresh_ready_status_and_native_endpoints() {
        for status in [1, 6, 2, 3] {
            let mut info =
                json!({"status":status, "phase":session_phase(status), "rtspsEndpoints":[]});
            mark_resume_progress(&mut info);
            assert_eq!(info["resumePending"], true);
            assert_eq!(info["phase"], "resuming");
        }
        for status in [2, 3] {
            let mut info = json!({"status":status, "phase":session_phase(status),
                "rtspsEndpoints":["rtsps://example.invalid:48010"]});
            mark_resume_progress(&mut info);
            assert_eq!(info["resumePending"], false);
            assert_eq!(info["phase"], session_phase(status));
        }
    }

    #[test]
    fn session_requests_keep_the_classic_nvst_client_identity() {
        let headers = cloudmatch_headers("token", "device-id").unwrap();
        assert_eq!(headers["nv-client-type"], "NATIVE");
        assert_eq!(headers["nv-client-streamer"], "NVIDIA-CLASSIC");
    }

    #[test]
    fn builds_a_stable_official_session_shape() {
        let body = build_create_body(
            "12345",
            &json!({
                "supportsInGameSettingsPersistence":true,
                "title":"Portal 2",
                "accountLinked":true,
                "appLaunchMode":"gamepadFriendly"
            }),
            &json!({
                "resolution":"2560x1440","fps":120,"maxBitrateMbps":80,
                "codec":"av1","colorQuality":"10bit_420","transportMode":"webrtc",
                "enableCloudGsync":true,"enableL4S":true,"enablePersistingInGameSettings":true
            }),
            "device-id",
        );
        let request = &body["sessionRequestData"];
        assert_eq!(request["appId"], 12345);
        assert_eq!(request["deviceHashId"], "device-id");
        assert_eq!(
            request["clientRequestMonitorSettings"][0]["widthInPixels"],
            2560
        );
        // Codec, bitrate ceiling, and the dynamic quality policy stay
        // client-side: the official request carries neither, resolving the codec
        // locally and the policy at RTSP ANNOUNCE time.
        assert!(request["requestedStreamingFeatures"].get("codec").is_none());
        assert!(
            request["requestedStreamingFeatures"]
                .get("maxBitrateKbps")
                .is_none()
        );
        assert!(request.get("externalAppId").is_none());
        assert_eq!(request["enablePersistingInGameSettings"], true);
        assert_eq!(request["internalTitle"], "Portal 2");
        assert_eq!(request["accountLinked"], true);
        assert_eq!(request["appLaunchMode"], 2);
        assert_eq!(request["secureRTSPSupported"], true);
        assert!(
            request["metaData"]
                .as_array()
                .expect("metadata")
                .iter()
                .all(|entry| entry["key"] != "GSStreamerType")
        );
        assert!(
            request["requestedStreamingFeatures"]
                .get("dynamicStreamingMode")
                .is_none()
        );
    }

    #[test]
    fn launch_mode_defaults_to_normal_and_maps_explicit_requests() {
        for (mode, expected) in [
            (Value::Null, 1),
            (json!("default"), 1),
            (json!("gamepadFriendly"), 2),
            (json!("touchFriendly"), 3),
            (json!("unknown"), 1),
        ] {
            let body = build_create_body(
                "12345",
                &json!({"appLaunchMode": mode}),
                &json!({"controllerMode":true,"launchInConsoleMode":true}),
                "device-id",
            );
            assert_eq!(body["sessionRequestData"]["appLaunchMode"], expected);
        }
    }

    #[test]
    fn native_session_requests_use_the_current_local_timezone_in_milliseconds() {
        let offset_before = chrono::Local::now().offset().utc_minus_local() * 1000;
        let created = build_create_body("12345", &json!({}), &json!({}), "device-id");
        let resumed = build_resume_body("12345", &json!({}), &json!({}), "device-id");
        let offset_after = chrono::Local::now().offset().utc_minus_local() * 1000;
        for body in [created, resumed] {
            let offset = body["sessionRequestData"]["clientTimezoneOffset"]
                .as_i64()
                .unwrap();
            assert!(offset == i64::from(offset_before) || offset == i64::from(offset_after));
        }
    }

    #[test]
    fn session_metadata_matches_the_official_pair_set() {
        let body = build_create_body("12345", &json!({}), &json!({}), "device-id");
        let metadata = body["sessionRequestData"]["metaData"].as_array().unwrap();
        assert_eq!(metadata[1]["key"], "SubSessionId");
        assert_eq!(metadata[1]["value"].as_str().unwrap().len(), 36);
        assert_eq!(metadata[2], json!({"key":"surroundAudioInfo","value":"2"}));
    }

    #[test]
    fn session_metadata_contains_only_owned_values() {
        let body = build_create_body(
            "12345",
            &json!({"zone":"eu-netherlands-north.cloudmatchbeta.nvidiagrid.net",
                "networkType":"Ethernet", "ClientImeSupport":"1",
                "latency@eu-netherlands-north.cloudmatchbeta.nvidiagrid.net":12}),
            &json!({"resolution":"2560x1440", "windowWidth":2560, "windowHeight":1440,
                "nativeHdrDisplay":{"minimumNits":0.005,"maximumNits":620}}),
            "device-id",
        );
        let metadata = body["sessionRequestData"]["metaData"].as_array().unwrap();
        assert_eq!(metadata.len(), 3);
        assert_eq!(metadata[0], json!({"key":"wssignaling","value":"1"}));
        assert_eq!(metadata[1]["key"], "SubSessionId");
        assert_eq!(metadata[2]["key"], "surroundAudioInfo");
        assert!(metadata.iter().all(|entry| {
            !matches!(
                entry["key"].as_str(),
                Some("networkType" | "ClientImeSupport" | "clientPhysicalResolution")
            ) && !entry["key"].as_str().unwrap().starts_with("latency@")
        }));
    }

    #[test]
    fn concrete_create_zones_bypass_local_region_but_auto_discovers_it() {
        let selected = json!({"metaData":[
            {"key":"local-region","value":"nearest"},
            {"key":"nearest","value":"https://nearest.partner.example/"},
            {"key":"gfn-regions","value":"queue"},
            {"key":"queue","value":"https://np-queue.nvidiagrid.net/"}
        ]});
        let (requested, server) = session_server(vec![(200, selected)], |_| {});
        let client = Client::builder().no_proxy().build().unwrap();
        let service = CloudMatchService::new(client.clone());
        let auth = conflict_auth();
        for (params, settings, expected) in [
            (
                json!({"streamingBaseUrl":"https://chosen.partner.example/", "zone":"chosen"}),
                json!({}),
                "https://chosen.partner.example/",
            ),
            (
                json!({}),
                json!({"region":"https://chosen.partner.example/"}),
                "https://chosen.partner.example/",
            ),
            (
                json!({"streamingBaseUrl":"https://np-queue.nvidiagrid.net/", "zone":"np-queue"}),
                json!({}),
                "https://np-queue.nvidiagrid.net/",
            ),
        ] {
            let chosen = requested_streaming_base(&params, &settings, &auth).unwrap();
            assert_eq!(chosen.as_str(), expected);
            assert_eq!(
                service
                    .create_base(&client, &chosen, &params, &settings, "token", "device")
                    .unwrap(),
                chosen
            );
        }
        assert_eq!(
            service
                .create_base(
                    &client,
                    &requested,
                    &json!({}),
                    &json!({}),
                    "token",
                    "device"
                )
                .unwrap()
                .as_str(),
            "https://nearest.partner.example/"
        );
        assert_eq!(server.join().unwrap(), ["GET /v2/serverInfo HTTP/1.1"]);
    }

    #[test]
    fn automatic_codec_delegates_selection_to_cloudmatch() {
        assert_eq!(codec_wire("auto"), 0);
        assert_eq!(codec_wire("unknown"), 0);
        assert_eq!(codec_wire("h264"), 1);
        assert_eq!(codec_wire("h265"), 2);
        assert_eq!(codec_wire("av1"), 3);
    }

    #[test]
    fn native_request_preserves_stream_quality_and_bandwidth() {
        let body = build_create_body(
            "12345",
            &json!({"title":"Portal 2"}),
            &json!({
                "resolution":"2560x1440",
                "fps":120,
                "maxBitrateMbps":75,
                "codec":"auto",
                "colorQuality":"10bit_444",
                "transportMode":"nvst"
            }),
            "device-id",
        );
        let features = &body["sessionRequestData"]["requestedStreamingFeatures"];
        assert!(features.get("codec").is_none());
        assert_eq!(features["bitDepth"], 1);
        assert_eq!(features["chromaFormat"], 1);
        assert!(features.get("maxBitrateKbps").is_none());
        assert!(features.get("dynamicStreamingMode").is_none());
        assert!(features.get("audioChannelCount").is_none());
        assert!(features.get("vsync").is_none());
        assert!(features.get("qosPolicy").is_none());
        assert!(features.get("touchSupport").is_none());
    }

    #[test]
    fn session_create_requests_the_documented_frame_rate_ceiling() {
        let hardware = json!({"protocolVersion":7, "videoBackends":[{"backend":"vaapi",
            "available":true, "codecs":[{"codec":"h265", "available":true,
                "colorQualities":["8bit_420"]}]}]});
        let software = json!({"protocolVersion":7, "videoBackends":[{"backend":"software",
            "available":true, "codecs":[{"codec":"h265", "available":true,
                "colorQualities":["8bit_420"]}]}]});
        let request = |resolution: &str, fps: i64, capabilities: &Value, entitled: i64| {
            let body = build_create_body(
                "12345",
                &json!({"title":"Portal 2", "runtimeCapabilities":capabilities,
                    "maxEntitledFps":entitled}),
                &json!({"resolution":resolution, "fps":fps, "codec":"h265"}),
                "device-id",
            );
            body["sessionRequestData"]["clientRequestMonitorSettings"][0]["framesPerSecond"].clone()
        };
        assert_eq!(request("1920x1080", 360, &hardware, 360), json!(360));
        assert_eq!(request("1920x1200", 360, &hardware, 360), json!(360));
        for resolution in [
            "2560x1440",
            "2560x1600",
            "3440x1440",
            "3840x2160",
            "3840x1080",
        ] {
            assert_eq!(
                request(resolution, 360, &hardware, 360),
                json!(240),
                "{resolution} must not request the full-HD-only tier"
            );
            assert_eq!(request(resolution, 240, &hardware, 360), json!(240));
        }
        assert_eq!(request("1920x1080", 999, &hardware, 360), json!(360));
        assert_eq!(request("1920x1080", 1, &hardware, 360), json!(30));
        assert_eq!(
            request("1920x1080", 360, &software, 360),
            json!(240),
            "a software-only decode path cannot request the top tier"
        );
        assert_eq!(
            request("1920x1080", 360, &json!({}), 360),
            json!(240),
            "an unreported capability probe is not affirmative support"
        );
        assert_eq!(
            request("1920x1080", 360, &hardware, 0),
            json!(240),
            "unconfirmed entitlement cannot request the top tier"
        );
        assert_eq!(
            request("1920x1080", 360, &hardware, 240),
            json!(240),
            "a 240 FPS entitlement cannot request the top tier"
        );
        assert_eq!(
            request("1920x1080", 360, &hardware, 120),
            json!(120),
            "a lower entitlement bounds the request"
        );
        assert_eq!(
            request("1920x1080", 240, &software, 0),
            json!(240),
            "base rates stay unaffected by the capability verdict"
        );
    }

    #[test]
    fn the_network_test_profile_matches_the_session_profile() {
        let hardware = json!({"protocolVersion":7, "videoBackends":[{"backend":"vaapi",
            "available":true, "codecs":[{"codec":"h265", "available":true,
                "colorQualities":["8bit_420"]}]}]});
        let software = json!({"protocolVersion":7, "videoBackends":[{"backend":"software",
            "available":true, "codecs":[{"codec":"h265", "available":true,
                "colorQualities":["8bit_420"]}]}]});
        let settings = json!({"resolution":"1920x1080", "fps":360, "codec":"h265"});
        for (capabilities, entitled, expected) in [
            (&hardware, 360_i64, 360_i64),
            (&software, 360, 240),
            (&json!({}), 360, 240),
            (&hardware, 0, 240),
            (&hardware, 120, 120),
        ] {
            let params = json!({"runtimeCapabilities":capabilities, "maxEntitledFps":entitled});
            let session = build_create_body("12345", &params, &settings, "device-id");
            let session_fps =
                session["sessionRequestData"]["clientRequestMonitorSettings"][0]["framesPerSecond"]
                    .clone();
            let profile = network_test_display_profile(&settings, &params);
            let allocation = crate::network_test::allocation_body("GFN-PC", profile);
            assert_eq!(profile.width, 1920);
            assert_eq!(profile.height, 1080);
            assert_eq!(
                allocation["netTestRequestData"]["netTestProfile"]["framesPerSecond"], session_fps,
                "the allocation profile must match the session profile for {capabilities}"
            );
            assert_eq!(session_fps, json!(expected), "{capabilities}");
        }
    }

    #[test]
    fn manual_av1_uses_native_nvst_even_with_a_legacy_transport_value() {
        let body = build_create_body(
            "12345",
            &json!({"title":"Portal 2"}),
            &json!({
                "codec":"av1",
                "colorQuality":"10bit_420",
                "transportMode":"webrtc"
            }),
            "device-id",
        );
        let request = &body["sessionRequestData"];
        assert!(request["requestedStreamingFeatures"].get("codec").is_none());
        assert_eq!(request["requestedStreamingFeatures"]["bitDepth"], 1);
        assert_eq!(request["requestedStreamingFeatures"]["chromaFormat"], 0);
        assert!(
            request["requestedStreamingFeatures"]
                .get("dynamicStreamingMode")
                .is_none()
        );
        assert_eq!(request["secureRTSPSupported"], true);
    }

    #[test]
    fn manual_av1_preserves_ten_bit_but_constrains_unsupported_444_chroma() {
        let body = build_create_body(
            "12345",
            &json!({"title":"Portal 2"}),
            &json!({"codec":"av1", "colorQuality":"10bit_444"}),
            "device-id",
        );
        let features = &body["sessionRequestData"]["requestedStreamingFeatures"];
        assert!(features.get("codec").is_none());
        assert_eq!(features["bitDepth"], 1);
        assert_eq!(features["chromaFormat"], 0);
    }

    #[test]
    fn bandwidth_saving_stays_client_side_and_out_of_the_cloudmatch_request() {
        // The official client never sends dynamicStreamingMode to CloudMatch: the
        // bandwidth policy is an RTSP ANNOUNCE value resolved from live settings.
        for saved in [None, Some(false), Some(true)] {
            let mut settings = json!({
                "resolution":"1920x1080",
                "fps":60,
                "maxBitrateMbps":75
            });
            if let Some(saved) = saved {
                settings["saveBandwidth"] = json!(saved);
            }
            let body = build_create_body(
                "12345",
                &json!({"title":"Portal 2"}),
                &settings,
                "device-id",
            );
            assert!(
                body["sessionRequestData"]["requestedStreamingFeatures"]
                    .get("dynamicStreamingMode")
                    .is_none()
            );
        }
    }

    #[test]
    fn negotiated_profile_carries_the_session_dynamic_quality_policy() {
        let base = trusted_cloudmatch_base(DEFAULT_STREAMING_BASE).unwrap();
        let parse = |session| {
            session_info(&json!({"session":session}), &base, "", "123", "device").unwrap()
        };
        for (echoed, finalized, mode) in [
            (None, None, Value::Null),
            (Some(1), None, json!(1)),
            (Some(1), Some(0), json!(0)),
            (None, Some(3), json!(3)),
            (Some(7), None, Value::Null),
        ] {
            let mut session = json!({"sessionId":"seat","status":2});
            if let Some(echoed) = echoed {
                session["sessionRequestData"]["requestedStreamingFeatures"]["dynamicStreamingMode"] =
                    json!(echoed);
            }
            if let Some(finalized) = finalized {
                session["finalizedStreamingFeatures"]["dynamicStreamingMode"] = json!(finalized);
            }
            assert_eq!(
                parse(session)["negotiatedStreamProfile"]["dynamicStreamingMode"],
                mode
            );
        }
    }

    #[test]
    fn manual_h265_preserves_ten_bit_color_on_native_nvst() {
        let body = build_create_body(
            "12345",
            &json!({"title":"Portal 2"}),
            &json!({
                "codec":"h265",
                "colorQuality":"10bit_420",
                "transportMode":"nvst"
            }),
            "device-id",
        );
        let request = &body["sessionRequestData"];
        assert!(request["requestedStreamingFeatures"].get("codec").is_none());
        assert_eq!(request["requestedStreamingFeatures"]["bitDepth"], 1);
        assert_eq!(request["requestedStreamingFeatures"]["chromaFormat"], 0);
        assert!(
            request["requestedStreamingFeatures"]
                .get("dynamicStreamingMode")
                .is_none()
        );
        assert_eq!(request["secureRTSPSupported"], true);
    }

    #[test]
    fn manual_h264_stays_fixed_and_constrains_unsupported_color() {
        let body = build_create_body(
            "12345",
            &json!({"title":"Portal 2"}),
            &json!({
                "codec":"h264",
                "colorQuality":"10bit_444",
                "transportMode":"nvst"
            }),
            "device-id",
        );
        let features = &body["sessionRequestData"]["requestedStreamingFeatures"];
        assert!(features.get("codec").is_none());
        assert_eq!(features["bitDepth"], 0);
        assert_eq!(features["chromaFormat"], 0);
    }

    #[test]
    fn native_cloud_gsync_policy_overrides_the_general_toggle() {
        assert!(!resolved_cloud_gsync(&json!({
            "enableCloudGsync": true,
            "nativeCloudGsyncMode": "disabled"
        })));
        assert!(resolved_cloud_gsync(&json!({
            "enableCloudGsync": false,
            "nativeCloudGsyncMode": "forced"
        })));
        assert!(resolved_cloud_gsync(&json!({
            "enableCloudGsync": true,
            "nativeCloudGsyncMode": "auto"
        })));
    }

    #[test]
    fn parses_pending_and_ready_session_responses() {
        let base = trusted_cloudmatch_base(DEFAULT_STREAMING_BASE).unwrap();
        let pending = json!({
            "requestStatus":{"statusCode":1},
            "session":{"sessionId":"one","status":1,"queuePosition":42,
                "sessionControlInfo":{"ip":"np-ams-01.cloudmatchbeta.nvidiagrid.net"}}
        });
        let info = session_info(&pending, &base, "auto", "123", "device").unwrap();
        assert_eq!(info["phase"], "preparing");
        assert_eq!(info["queuePosition"], 42);
        assert_eq!(
            info["streamingBaseUrl"],
            "https://np-ams-01.cloudmatchbeta.nvidiagrid.net"
        );

        let ready = json!({
            "requestStatus":{"statusCode":1},
            "session":{"sessionId":"one","status":2,
                "connectionInfo":[{"usage":14,"appLevelProtocol":6,"ip":"80.1.2.3","port":443,"resourcePath":"/nvst/"}]}
        });
        let info = session_info(&ready, &base, "auto", "123", "device").unwrap();
        assert_eq!(info["signalingUrl"], "wss://80.1.2.3:443");
        assert_eq!(info["serverIp"], "80.1.2.3");
    }

    #[test]
    fn omitted_codec_keeps_the_client_selected_codec_through_hdr_preparation() {
        let base = Url::parse(DEFAULT_STREAMING_BASE).unwrap();
        let requested = json!({"codec":"h265","colorQuality":"10bit_420","enableHdr":true,"nativeHdrSupported":true});
        let body = build_create_body("123", &json!({}), &requested, "device");
        // The official request names no codec: color is negotiated, the codec
        // stays a client selection until the RTSP ANNOUNCE.
        assert!(
            body["sessionRequestData"]["requestedStreamingFeatures"]
                .get("codec")
                .is_none()
        );
        let initial = session_info(
            &json!({"session":{"sessionId":"omitted-codec","status":1,"sdrHdrMode":1}}),
            &base,
            "auto",
            "123",
            "device",
        )
        .unwrap();
        assert_eq!(initial["negotiatedStreamProfile"]["codec"], Value::Null);
        let capabilities = json!({"protocolVersion":7,"nativeHdrSupported":true,"videoBackends":[{
            "backend":"videotoolbox","platform":"macos","available":true,"codecs":[{
                "codec":"h265","available":true,"hdrSupported":true,
                "colorQualities":["10bit_420"],"hdrColorQualities":["10bit_420"]
            }]
        }]});
        for status in [2, 3] {
            let mut ready = session_info(
                &json!({"session":{
                    "sessionId":"omitted-codec","status":status,"sdrHdrMode":1,
                    "finalizedStreamingFeatures":{"bitDepth":1,"chromaFormat":0}
                }}),
                &base,
                "auto",
                "123",
                "device",
            )
            .unwrap();
            assert_eq!(ready["negotiatedStreamProfile"]["codec"], Value::Null);
            preserve_session_profile(&mut ready, &initial);
            assert_eq!(ready["negotiatedStreamProfile"]["codec"], Value::Null);
            let prepared = crate::streamer::StreamerService::new()
                .prepare_embedded(
                    &json!({"session":ready,"runtimeCapabilities":capabilities}),
                    &json!({"codec":"h265","colorQuality":"10bit_420","enableHdr":true}),
                )
                .unwrap();
            assert_eq!(prepared["context"]["settings"]["codec"], "H265");
            assert_eq!(prepared["context"]["settings"]["enableHdr"], true);
        }
    }

    #[test]
    fn codec_inheritance_never_crosses_sessions_or_overrides_reported_values() {
        let base = Url::parse(DEFAULT_STREAMING_BASE).unwrap();
        let previous = json!({"sessionId":"same-seat","negotiatedStreamProfile":{
            "codec":"H265","codecSource":"request"
        }});
        let mut different = session_info(
            &json!({"session":{
                "sessionId":"other-seat","status":2
            }}),
            &base,
            "auto",
            "123",
            "device",
        )
        .unwrap();
        preserve_session_profile(&mut different, &previous);
        assert_eq!(different["negotiatedStreamProfile"]["codec"], Value::Null);
        for reported in [
            Value::Null,
            json!("unsupported"),
            json!("H264"),
            json!("AV1"),
        ] {
            let mut info = session_info(
                &json!({"session":{
                    "sessionId":"same-seat","status":2,"negotiatedStreamProfile":{"codec":reported}
                }}),
                &base,
                "auto",
                "123",
                "device",
            )
            .unwrap();
            let before = info.clone();
            preserve_session_profile(&mut info, &previous);
            assert_eq!(info, before);
            assert_eq!(info["negotiatedStreamProfile"]["codecSource"], "server");
        }
        for value in [Value::Null, json!(0), json!(99)] {
            let mut info = session_info(
                &json!({"session":{
                    "sessionId":"same-seat","status":2,"finalizedStreamingFeatures":{"codec":value}
                }}),
                &base,
                "auto",
                "123",
                "device",
            )
            .unwrap();
            preserve_session_profile(&mut info, &previous);
            assert_eq!(info["negotiatedStreamProfile"]["codec"], Value::Null);
            assert_eq!(info["negotiatedStreamProfile"]["codecSource"], "server");
        }
    }

    #[test]
    fn reported_codec_survives_later_partial_responses_without_reverting_to_request() {
        let base = Url::parse(DEFAULT_STREAMING_BASE).unwrap();
        let request = json!({"sessionId":"same-seat","negotiatedStreamProfile":{
            "codec":"H265","codecSource":"request"
        }});
        let mut regional = session_info(
            &json!({"session":{
                "sessionId":"same-seat","status":2,"negotiatedStreamProfile":{"codec":"AV1"}
            }}),
            &base,
            "auto",
            "123",
            "device",
        )
        .unwrap();
        preserve_session_profile(&mut regional, &request);
        let mut direct = session_info(&json!({"session":{
            "sessionId":"same-seat","status":2,"finalizedStreamingFeatures":{"bitDepth":1,"chromaFormat":0}
        }}), &base, "auto", "123", "device").unwrap();
        preserve_session_profile(&mut direct, &regional);
        preserve_session_profile(&mut direct, &request);
        assert_eq!(direct["negotiatedStreamProfile"]["codec"], "AV1");
        assert_eq!(direct["negotiatedStreamProfile"]["codecSource"], "server");
        assert_eq!(
            direct["negotiatedStreamProfile"]["colorQuality"],
            "10bit_420"
        );
    }

    #[test]
    fn active_updates_use_latest_codec_evidence_and_keep_returned_info_in_sync() {
        let base = Url::parse(DEFAULT_STREAMING_BASE).unwrap();
        let client = Client::new();
        let service = CloudMatchService::new(client.clone());
        let mut initial = json!({"sessionId":"same-seat","negotiatedStreamProfile":{
            "codec":"H265","codecSource":"request"
        }});
        service
            .store_active(&mut initial, &base, "auto", "123", client.clone())
            .unwrap();
        let mut stale = initial.clone();
        let mut reported = session_info(
            &json!({"session":{
                "sessionId":"same-seat","status":2,"negotiatedStreamProfile":{"codec":"AV1"}
            }}),
            &base,
            "auto",
            "123",
            "device",
        )
        .unwrap();
        service
            .store_active(&mut reported, &base, "auto", "123", client.clone())
            .unwrap();
        service
            .store_active(&mut stale, &base, "auto", "123", client.clone())
            .unwrap();
        assert_eq!(stale["negotiatedStreamProfile"]["codec"], "AV1");
        assert_eq!(stale["negotiatedStreamProfile"]["codecSource"], "server");
        assert_eq!(service.active.lock().unwrap().as_ref().unwrap().info, stale);
        let mut invalid = session_info(
            &json!({"session":{
                "sessionId":"same-seat","status":2,"negotiatedStreamProfile":{"codec":null}
            }}),
            &base,
            "auto",
            "123",
            "device",
        )
        .unwrap();
        service
            .store_active(&mut invalid, &base, "auto", "123", client.clone())
            .unwrap();
        let mut omitted = session_info(
            &json!({"session":{
                "sessionId":"same-seat","status":2
            }}),
            &base,
            "auto",
            "123",
            "device",
        )
        .unwrap();
        service
            .store_active(&mut omitted, &base, "auto", "123", client)
            .unwrap();
        assert_eq!(omitted["negotiatedStreamProfile"]["codec"], Value::Null);
    }

    #[test]
    fn active_keyboard_layout_survives_polling_but_not_a_different_session() {
        let client = Client::new();
        let service = CloudMatchService::new(client.clone());
        let base = Url::parse("https://prod.cloudmatchbeta.nvidiagrid.net/").unwrap();
        let mut created = json!({"sessionId":"seat-one", "keyboardLayout":"fr-FR"});
        service
            .store_active(&mut created, &base, "auto", "123", client.clone())
            .unwrap();

        let mut polled = json!({"sessionId":"seat-one", "status":2});
        service
            .store_active(&mut polled, &base, "auto", "123", client.clone())
            .unwrap();
        assert_eq!(polled["keyboardLayout"], "fr-FR");

        let mut resumed = json!({"sessionId":"seat-one", "keyboardLayout":"de-DE"});
        service
            .store_active(&mut resumed, &base, "auto", "123", client.clone())
            .unwrap();
        let mut polled = json!({"sessionId":"seat-one", "status":3});
        service
            .store_active(&mut polled, &base, "auto", "123", client.clone())
            .unwrap();
        assert_eq!(polled["keyboardLayout"], "de-DE");

        let mut different = json!({"sessionId":"seat-two"});
        service
            .store_active(&mut different, &base, "auto", "123", client)
            .unwrap();
        assert!(different["keyboardLayout"].is_null());
    }

    #[test]
    fn nested_negotiated_codec_reaches_hdr_preparation() {
        let base = Url::parse(DEFAULT_STREAMING_BASE).unwrap();
        let capabilities = json!({"protocolVersion":7,"nativeHdrSupported":true,"videoBackends":[{
            "backend":"videotoolbox","platform":"macos","available":true,"codecs":[{
                "codec":"h265","available":true,"hdrSupported":true,
                "colorQualities":["10bit_420"],"hdrColorQualities":["10bit_420"]
            }]
        }]});
        let settings = json!({"codec":"h264","colorQuality":"8bit_420","enableHdr":false});
        for status in [2, 3] {
            for codec in ["H265", "HEVC", "hevc"] {
                let payload = json!({"session":{
                    "sessionId":"nested-codec", "status":status, "sdrHdrMode":1,
                    "negotiatedStreamProfile":{"codec":codec},
                    "finalizedStreamingFeatures":{"bitDepth":1,"chromaFormat":0}
                }});
                let info = session_info(&payload, &base, "auto", "123", "device").unwrap();
                assert_eq!(info["negotiatedStreamProfile"]["codec"], "H265");
                assert_eq!(info["negotiatedStreamProfile"]["colorQuality"], "10bit_420");
                let prepared = crate::streamer::StreamerService::new()
                    .prepare_embedded(
                        &json!({"session":info,"runtimeCapabilities":capabilities}),
                        &settings,
                    )
                    .unwrap();
                assert_eq!(prepared["context"]["settings"]["codec"], "H265");
                assert_eq!(prepared["context"]["settings"]["enableHdr"], true);
            }
        }
    }

    #[test]
    fn nested_negotiated_codec_overrides_feature_hints_without_guessing() {
        let base = Url::parse(DEFAULT_STREAMING_BASE).unwrap();
        for (codec, expected) in [
            (json!("H264"), json!("H264")),
            (json!("avc"), json!("H264")),
            (json!("AV1"), json!("AV1")),
            (json!("unsupported"), Value::Null),
            (json!(2), Value::Null),
            (json!(""), Value::Null),
            (Value::Null, Value::Null),
        ] {
            let payload = json!({"session":{
                "sessionId":"nested-codec", "status":2, "sdrHdrMode":1,
                "negotiatedStreamProfile":{"codec":codec},
                "sessionRequestData":{"requestedStreamingFeatures":{"codec":2}},
                "finalizedStreamingFeatures":{"codec":2,"bitDepth":1,"chromaFormat":0}
            }});
            let info = session_info(&payload, &base, "auto", "123", "device").unwrap();
            assert_eq!(info["negotiatedStreamProfile"]["codec"], expected);
        }
    }

    #[test]
    fn partial_finalized_features_preserve_returned_session_color_fields() {
        let base = Url::parse(DEFAULT_STREAMING_BASE).unwrap();
        let mut payload = json!({"session":{
            "sessionId":"color-seat", "status":2, "sdrHdrMode":1,
            "sessionRequestData":{
                "clientRequestMonitorSettings":[{"widthInPixels":2560,"heightInPixels":1440}],
                "requestedStreamingFeatures":{"codec":2,"bitDepth":1,"chromaFormat":0}
            },
            "finalizedStreamingFeatures":{}
        }});
        for finalized in [
            json!({}),
            json!({"maxBitrateKbps":50000}),
            json!({"codec":2}),
        ] {
            payload["session"]["finalizedStreamingFeatures"] = finalized;
            let info = session_info(&payload, &base, "auto", "123", "device").unwrap();
            assert_eq!(info["negotiatedStreamProfile"]["codec"], "H265");
            assert_eq!(info["negotiatedStreamProfile"]["colorQuality"], "10bit_420");
            assert_eq!(info["negotiatedStreamProfile"]["enableHdr"], true);
        }
        payload["session"]["sdrHdrMode"] = json!(0);
        payload["session"]["finalizedStreamingFeatures"] = json!({"chromaFormat":1});
        let info = session_info(&payload, &base, "auto", "123", "device").unwrap();
        assert_eq!(info["negotiatedStreamProfile"]["colorQuality"], "10bit_444");
        assert_eq!(info["negotiatedStreamProfile"]["enableHdr"], false);
        payload["session"]["finalizedStreamingFeatures"] = json!({"bitDepth":0});
        let info = session_info(&payload, &base, "auto", "123", "device").unwrap();
        assert_eq!(info["negotiatedStreamProfile"]["colorQuality"], "8bit_420");
        payload["session"]["finalizedStreamingFeatures"] = json!({"bitDepth":null});
        let info = session_info(&payload, &base, "auto", "123", "device").unwrap();
        assert!(info["negotiatedStreamProfile"]["colorQuality"].is_null());
    }

    #[test]
    fn cloudmatch_chroma_enums_are_not_nvst_chroma_format_ids() {
        for (chroma, expected) in [
            (0, json!("10bit_420")),
            (1, json!("10bit_444")),
            (2, Value::Null),
            (3, Value::Null),
        ] {
            let profile =
                negotiated_profile(&json!({}), &json!({"bitDepth":1,"chromaFormat":chroma}));
            assert_eq!(profile["colorQuality"], expected);
        }
    }

    #[test]
    fn rejects_untrusted_session_endpoints() {
        assert!(trusted_cloudmatch_base("http://prod.cloudmatchbeta.nvidiagrid.net").is_err());
        assert_eq!(
            trusted_cloudmatch_base("https://partner.example.com/v2/serverInfo?q=1")
                .unwrap()
                .as_str(),
            "https://partner.example.com/"
        );
        assert!(trusted_cloudmatch_base("https://partner.example.com:8443").is_err());
        assert!(trusted_cloudmatch_base("https://user@partner.example.com").is_err());
        assert!(trusted_cloudmatch_base("https://127.0.0.1").is_err());
        assert!(trusted_cloudmatch_base("https://seat.local").is_err());
        for address in [
            "10.0.0.1",
            "127.0.0.1",
            "169.254.1.1",
            "100.64.0.1",
            "224.0.0.1",
            "255.255.255.255",
            "::1",
            "fc00::1",
            "fe80::1",
            "ff02::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(trusted_learned_server_base(address).is_err(), "{address}");
        }
        assert!(trusted_learned_server_base("203.0.113.20").is_ok());
        assert!(trusted_learned_server_base("2001:db8::20").is_ok());
    }

    #[test]
    fn partner_control_host_and_status_take_precedence_over_early_signaling() {
        let zone = trusted_cloudmatch_base("https://zone.partner.example").unwrap();
        let payload = json!({"session":{"sessionId":"seat","status":1,
            "sessionControlInfo":{"ip":"control.partner.example","port":443},
            "connectionInfo":[
                {"usage":15,"appLevelProtocol":6,"ip":"media.partner.example","port":322},
                {"usage":14,"appLevelProtocol":6,"ip":"signaling.partner.example","port":322,
                    "resourcePath":"rtsps://other.partner.example:48322"}]}});
        let info = session_info(&payload, &zone, "zone.partner.example", "123", "device").unwrap();
        assert_eq!(info["streamingBaseUrl"], "https://control.partner.example");
        assert_eq!(info["phase"], "preparing");
        assert_eq!(
            info["rtspsEndpoints"],
            json!(["rtsps://signaling.partner.example:322"])
        );
        let mut ready = payload;
        ready["session"]["status"] = json!(2);
        ready["session"]["sessionControlInfo"]["ip"] = json!("forwarded.partner.example");
        let next = session_info(&ready, &zone, "zone.partner.example", "123", "device").unwrap();
        assert_eq!(
            next["streamingBaseUrl"],
            "https://forwarded.partner.example"
        );
        assert_eq!(next["phase"], "ready");
        ready["session"]["sessionControlInfo"]["port"] = json!(8443);
        assert_eq!(
            session_info(&ready, &zone, "zone.partner.example", "123", "device").unwrap()["streamingBaseUrl"],
            "https://zone.partner.example"
        );
    }

    #[test]
    fn signaling_entries_keep_cloudmatch_order_and_ignore_irrelevant_ports() {
        let base = trusted_cloudmatch_base(DEFAULT_STREAMING_BASE).unwrap();
        let payload = json!({"session":{"sessionId":"seat","status":2,"connectionInfo":[
            {"usage":14,"appLevelProtocol":5,"ip":"not-native.example","port":322},
            {"usage":15,"appLevelProtocol":6,"ip":"media.example","port":322},
            {"usage":14,"appLevelProtocol":1,"ip":"first.example","port":322},
            {"usage":16,"ip":"second.example","port":48322,"resourcePath":"rtsps://wrong.example:322"},
            {"usage":16,"ip":"bad.example","port":0},
            {"usage":16,"resourcePath":"rtsps://third.example:443","port":443}]}});
        let info = session_info(&payload, &base, "", "123", "device").unwrap();
        assert_eq!(
            info["rtspsEndpoints"],
            json!([
                "rtsps://first.example:322",
                "rtsps://second.example:48322",
                "rtsps://third.example:443"
            ])
        );
        assert_eq!(info["signalingUrl"], "wss://first.example:322");
    }

    fn network_test_udp_server(
        cap: u32,
        key: &[u8],
        session_id: &str,
    ) -> (std::net::SocketAddr, thread::JoinHandle<usize>) {
        let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let address = socket.local_addr().unwrap();
        let key = key.to_vec();
        let session_id = session_id.as_bytes().to_vec();
        let worker = thread::spawn(move || {
            let mut served = 0_usize;
            let mut buffer = vec![0_u8; 4096];
            while let Ok((length, peer)) = socket.recv_from(&mut buffer) {
                let Ok(request) =
                    crate::network_test::NetworkTestMessage::decode(&buffer[..length])
                else {
                    continue;
                };
                let Some(size) = request.payload_size() else {
                    continue;
                };
                if !request.verify(&key).unwrap_or(false) || size > cap {
                    continue;
                }
                let mut reply = crate::network_test::NetworkTestMessage::default();
                reply.set_message_type(crate::network_test::MESSAGE_TYPE_MTU_RESPONSE);
                reply.set_session_id(session_id.clone());
                reply.set_payload_size(size);
                let datagram = reply.encode_response(size as usize);
                let _ = socket.send_to(&datagram, peer);
                served += 1;
            }
            served
        });
        (address, worker)
    }

    #[test]
    fn measured_network_test_session_reaches_allocation_and_the_session_context() {
        use std::io::{BufRead, BufReader, Read, Write};
        use std::sync::{Arc, Mutex};

        let key: [u8; 32] = [0x7e; 32];
        let (udp_address, udp_server) = network_test_udp_server(1_200, &key, "nt-1");

        let allocation = json!({
            "requestStatus":{"requestId":"req-1","serverId":"zone-1","statusCode":0},
            "netTestSession":{
                "sessionId":"nt-1",
                "serverId":"zone-1",
                "hmacKey":"~".repeat(32),
                "connectionInfo":[{
                    "ip":udp_address.ip().to_string(),
                    "port":udp_address.port(),
                    "appLevelProtocol":5
                }],
                "netTestThresholds":{
                    "recommendedBandwidthMBPS":50.0,
                    "requiredBandwidthMBPS":25.0,
                    "recommendedLatencyMS":40.0,
                    "requiredLatencyMS":80.0,
                    "recommendedPacketLossPct":1.0,
                    "requiredPacketLossPct":3.0
                }
            }
        });
        let create_reply = json!({
            "requestStatus":{"statusCode":1},
            "session":{
                "sessionId":"seat-1",
                "status":2,
                "connectionInfo":[{
                    "ip":"127.0.0.1","port":49_100,"usage":14,"resourcePath":"/nvst/"
                }]
            }
        });

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        let recorded: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let server_records = Arc::clone(&recorded);
        let server = thread::spawn(move || {
            for (status, body) in [
                (200_u16, allocation.to_string()),
                (200, create_reply.to_string()),
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(&stream);
                let mut request_line = String::new();
                reader.read_line(&mut request_line).unwrap();
                let mut length = 0_usize;
                let mut line = String::new();
                loop {
                    line.clear();
                    assert!(reader.read_line(&mut line).unwrap() > 0);
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap();
                    }
                }
                let mut payload = vec![0_u8; length];
                reader.read_exact(&mut payload).unwrap();
                server_records.lock().unwrap().push((
                    request_line.trim().to_owned(),
                    String::from_utf8_lossy(&payload).into_owned(),
                ));
                write!(
                    stream,
                    "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
        });

        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();
        let service = CloudMatchService::new(client.clone());
        let created = service
            .create_at(
                &json!({"appId":"123", "networkTest":true}),
                &json!({}),
                &conflict_auth(),
                "device",
                || Ok((client, base)),
            )
            .unwrap();

        let info = &created["session"];
        assert_eq!(info["sessionId"], "seat-1");
        assert_eq!(info["networkTest"]["sessionId"], "nt-1");
        assert_eq!(info["networkTest"]["status"], "measured");
        assert_eq!(info["networkTestSessionId"], "nt-1");
        assert_eq!(info["networkTest"]["zone"], "127.0.0.1");
        let measured = info["networkTest"]["measuredDatagramBytes"]
            .as_u64()
            .expect("measured datagram size");
        assert!(measured <= 1_200, "measured {measured}");
        assert!(measured + 32 >= 1_200, "measured {measured}");
        assert!(info["networkTest"]["probes"].as_u64().unwrap_or_default() > 0);

        server.join().unwrap();
        let received = recorded.lock().unwrap().clone();
        assert_eq!(received.len(), 2);
        assert!(received[0].0.starts_with("POST /v2/nettestsession"));
        assert!(received[0].1.contains("\"clientPlatformName\""));
        assert!(received[1].0.starts_with("POST /v2/session"));
        assert!(
            received[1].1.contains("\"networkTestSessionId\":\"nt-1\""),
            "allocation body carries the measured session: {}",
            received[1].1
        );

        assert!(
            udp_server.join().unwrap() > 0,
            "the probe never reached the authenticated server"
        );
    }

    #[test]
    fn a_session_without_a_response_key_refuses_to_probe() {
        let (base, server) = session_server(
            vec![
                (
                    200,
                    json!({"netTestSession":{
                        "sessionId":"nt-nokey",
                        "connectionInfo":[{
                            "ip":"127.0.0.1","port":49_100,"appLevelProtocol":5
                        }],
                        "netTestThresholds":{
                            "recommendedBandwidthMBPS":50.0,"requiredBandwidthMBPS":25.0,
                            "recommendedLatencyMS":40.0,"requiredLatencyMS":80.0,
                            "recommendedPacketLossPct":1.0,"requiredPacketLossPct":3.0
                        }
                    }}),
                ),
                (
                    200,
                    json!({"requestStatus":{"statusCode":1},"session":{"sessionId":"B","status":2}}),
                ),
            ],
            |_| {},
        );
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let service = CloudMatchService::new(client.clone());
        let created = service
            .create_at(
                &json!({"appId":"123", "networkTest":true}),
                &json!({}),
                &conflict_auth(),
                "device",
                || Ok((client, base)),
            )
            .unwrap();
        assert_eq!(created["session"]["networkTest"]["status"], "unavailable");
        assert_eq!(
            created["session"]["networkTest"]["code"],
            "network-test-key-unavailable"
        );
        assert!(
            created["session"]["networkTestSessionId"].is_null(),
            "no session is advertised without a verified measurement"
        );
        let received = server.join().unwrap();
        assert_eq!(received.len(), 2);
        assert!(
            received[0].starts_with("POST /v2/nettestsession"),
            "{}",
            received[0]
        );
        assert!(received[1].starts_with("POST /v2/session"));
    }

    #[test]
    fn a_stalled_network_test_allocation_times_out() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        let worker = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            thread::sleep(Duration::from_secs(12));
            drop(stream);
        });
        let client = Client::builder().no_proxy().build().unwrap();
        let error =
            try_network_test_session(&client, &base, "token", "device", &json!({}), &json!({}))
                .unwrap_err();
        assert_eq!(error.code, "network_error");
        worker.join().unwrap();
    }

    #[test]
    fn a_cancelled_request_never_reaches_the_network_test_allocation() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        listener.set_nonblocking(true).unwrap();
        let client = Client::builder().no_proxy().build().unwrap();
        let requests = std::sync::Arc::new(crate::requests::Requests::default());
        let permit = requests.admit("nettest", "session.create").unwrap();
        requests.cancel("nettest");
        let result = crate::requests::scope(permit.token.clone(), || {
            try_network_test_session(&client, &base, "token", "device", &json!({}), &json!({}))
        });
        assert_eq!(result.unwrap_err().code, "cancelled");
        assert!(
            matches!(listener.accept(), Err(ref error) if error.kind() == std::io::ErrorKind::WouldBlock),
            "a cancelled request must not open a connection"
        );
    }

    #[test]
    fn the_network_test_setting_enables_the_probe() {
        use std::io::Write;
        use std::sync::{Arc, Mutex};

        let key: [u8; 32] = [0x7e; 32];
        let (udp_address, udp_server) = network_test_udp_server(1_200, &key, "nt-1");
        let allocation = json!({
            "netTestSession":{
                "sessionId":"nt-1",
                "serverId":"zone-1",
                "hmacKey":"~".repeat(32),
                "connectionInfo":[{
                    "ip":udp_address.ip().to_string(),
                    "port":udp_address.port(),
                    "appLevelProtocol":5
                }],
                "netTestThresholds":{
                    "recommendedBandwidthMBPS":50.0,"requiredBandwidthMBPS":25.0,
                    "recommendedLatencyMS":40.0,"requiredLatencyMS":80.0,
                    "recommendedPacketLossPct":1.0,"requiredPacketLossPct":3.0
                }
            }
        });
        let create_reply = json!({
            "requestStatus":{"statusCode":1},
            "session":{"sessionId":"seat-1","status":2}
        });
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        let recorded: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let server_records = Arc::clone(&recorded);
        let server = thread::spawn(move || {
            for (status, body) in [
                (200_u16, allocation.to_string()),
                (200, create_reply.to_string()),
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = std::io::BufReader::new(&stream);
                let mut request_line = String::new();
                std::io::BufRead::read_line(&mut reader, &mut request_line).unwrap();
                let mut length = 0_usize;
                let mut line = String::new();
                loop {
                    line.clear();
                    assert!(std::io::BufRead::read_line(&mut reader, &mut line).unwrap() > 0);
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap();
                    }
                }
                server_records
                    .lock()
                    .unwrap()
                    .push(request_line.trim().to_owned());
                let mut payload = vec![0_u8; length];
                std::io::Read::read_exact(&mut reader, &mut payload).unwrap();
                write!(
                    stream,
                    "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
        });

        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();
        let service = CloudMatchService::new(client.clone());
        let created = service
            .create_at(
                &json!({"appId":"123"}),
                &json!({"networkTest":true}),
                &conflict_auth(),
                "device",
                || Ok((client, base)),
            )
            .unwrap();
        assert_eq!(created["session"]["networkTest"]["status"], "measured");
        assert_eq!(created["session"]["networkTestSessionId"], "nt-1");
        server.join().unwrap();
        let received = recorded.lock().unwrap().clone();
        assert_eq!(received.len(), 2);
        assert!(
            received[0].starts_with("POST /v2/nettestsession"),
            "{}",
            received[0]
        );
        assert!(udp_server.join().unwrap() > 0, "the setting never probed");
    }

    #[test]
    fn a_zone_without_network_test_keeps_the_previous_allocation_body() {
        let (base, server) = session_server(
            vec![(
                200,
                json!({"requestStatus":{"statusCode":1},"session":{"sessionId":"A","status":1}}),
            )],
            |_| {},
        );
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let service = CloudMatchService::new(client.clone());
        let created = service
            .create_at(
                &json!({"appId":"123"}),
                &json!({}),
                &conflict_auth(),
                "device",
                || Ok((client, base)),
            )
            .unwrap();
        assert_eq!(created["session"]["networkTest"]["status"], "not_requested");
        assert!(created["session"]["networkTestSessionId"].is_null());
        server.join().unwrap();
    }

    #[test]
    fn an_oversized_chunked_allocation_response_is_rejected() {
        use std::io::Write;

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = vec![0_u8; 4096];
            let _ = std::io::Read::read(&mut stream, &mut request);
            let _ = stream.write_all(
                b"HTTP/1.1 200 Fixture\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
            );
            let chunk = vec![b'x'; 64 * 1024];
            let mut written = 0_usize;
            while written <= MAXIMUM_NETWORK_TEST_RESPONSE_BYTES as usize {
                let _ = stream.write_all(format!("{:x}\r\n", chunk.len()).as_bytes());
                let _ = stream.write_all(&chunk);
                let _ = stream.write_all(b"\r\n");
                written += chunk.len();
            }
        });
        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();
        let error =
            try_network_test_session(&client, &base, "token", "device", &json!({}), &json!({}))
                .unwrap_err();
        assert_eq!(error.code, "network-test-rejected", "{}", error.message);
        assert!(error.message.contains("size limit"), "{}", error.message);
        worker.join().unwrap();
    }

    #[test]
    fn the_persisted_network_test_setting_reaches_session_creation() {
        let directory = tempfile::tempdir().unwrap();
        let mut store =
            crate::settings::SettingsStore::load(Some(directory.path().to_path_buf())).unwrap();
        assert_eq!(store.all()["networkTest"], false, "the setting ships off");

        let key: [u8; 32] = [0x7e; 32];
        let (udp_address, udp_server) = network_test_udp_server(1_200, &key, "nt-1");
        let allocation = json!({
            "netTestSession":{
                "sessionId":"nt-1",
                "serverId":"zone-1",
                "hmacKey":"~".repeat(32),
                "connectionInfo":[{
                    "ip":udp_address.ip().to_string(),
                    "port":udp_address.port(),
                    "appLevelProtocol":5
                }],
                "netTestThresholds":{
                    "recommendedBandwidthMBPS":50.0,"requiredBandwidthMBPS":25.0,
                    "recommendedLatencyMS":40.0,"requiredLatencyMS":80.0,
                    "recommendedPacketLossPct":1.0,"requiredPacketLossPct":3.0
                }
            }
        });
        let (base, server) = session_server(
            vec![
                (
                    200,
                    json!({"requestStatus":{"statusCode":1},"session":{"sessionId":"A","status":1}}),
                ),
                (200, json!({})),
                (200, allocation),
                (
                    200,
                    json!({"requestStatus":{"statusCode":1},"session":{"sessionId":"B","status":2}}),
                ),
            ],
            |_| {},
        );
        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let service = CloudMatchService::new(client.clone());

        let created = service
            .create_at(
                &json!({"appId":"123"}),
                &store.all(),
                &conflict_auth(),
                "device",
                || Ok((client.clone(), base.clone())),
            )
            .unwrap();
        assert_eq!(created["session"]["networkTest"]["status"], "not_requested");
        service.finish_create("A", false).unwrap();

        store.set("networkTest", json!(true)).unwrap();
        let restored =
            crate::settings::SettingsStore::load(Some(directory.path().to_path_buf())).unwrap();
        assert_eq!(restored.all()["networkTest"], true, "the setting persists");

        let created = service
            .create_at(
                &json!({"appId":"123"}),
                &restored.all(),
                &conflict_auth(),
                "device",
                || Ok((client, base)),
            )
            .unwrap();
        assert_eq!(created["session"]["networkTest"]["status"], "measured");
        assert_eq!(created["session"]["networkTestSessionId"], "nt-1");

        let received = server.join().unwrap();
        assert_eq!(received.len(), 4, "{received:?}");
        assert!(received[0].starts_with("POST /v2/session"), "{received:?}");
        let probe = received
            .iter()
            .position(|line| line.starts_with("POST /v2/nettestsession"))
            .expect("the opt-in probe runs");
        assert_eq!(
            probe, 2,
            "the default-off create must not probe: {received:?}"
        );
        assert!(udp_server.join().unwrap() > 0);
    }

    #[test]
    fn a_path_without_a_confirmed_datagram_is_reported_unmeasured() {
        let silent = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let silent_address = silent.local_addr().unwrap();
        let (base, server) = session_server(
            vec![
                (
                    200,
                    json!({"netTestSession":{
                        "sessionId":"nt-silent",
                        "serverId":"zone-1",
                        "hmacKey":"~".repeat(32),
                        "connectionInfo":[{
                            "ip":silent_address.ip().to_string(),
                            "port":silent_address.port(),
                            "appLevelProtocol":5
                        }],
                        "netTestThresholds":{
                            "recommendedBandwidthMBPS":50.0,"requiredBandwidthMBPS":25.0,
                            "recommendedLatencyMS":40.0,"requiredLatencyMS":80.0,
                            "recommendedPacketLossPct":1.0,"requiredPacketLossPct":3.0
                        }
                    }}),
                ),
                (
                    200,
                    json!({"requestStatus":{"statusCode":1},"session":{"sessionId":"A","status":2}}),
                ),
            ],
            |_| {},
        );
        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let service = CloudMatchService::new(client.clone());
        let created = service
            .create_at(
                &json!({"appId":"123", "networkTest":true}),
                &json!({}),
                &conflict_auth(),
                "device",
                || Ok((client, base)),
            )
            .unwrap();
        let measured = &created["session"]["networkTest"];
        assert_eq!(measured["status"], "unmeasured");
        assert!(measured["measuredDatagramBytes"].is_null());
        assert!(measured["probes"].as_u64().unwrap_or_default() > 0);
        assert!(
            created["session"]["networkTestSessionId"].is_null(),
            "an unconfirmed path must not advertise an unmeasured session"
        );
        assert_eq!(created["session"]["sessionId"], "A");
        server.join().unwrap();
    }
}
