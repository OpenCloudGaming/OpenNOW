use crate::gfn::{AuthSession, ServiceError};
use crate::proxy::client_for_settings;
use rand::RngCore as _;
use reqwest::blocking::{Client, Response};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue, USER_AGENT};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};
use url::Url;

const LCARS_CLIENT_ID: &str = "ec7e38d4-03af-4b58-b131-cfb0495903ab";
const GFN_CLIENT_VERSION: &str = "2.0.87.131";
const DEFAULT_STREAMING_BASE: &str = "https://prod.cloudmatchbeta.nvidiagrid.net/";
const DEFAULT_STUN_SERVER: &str = "stun:s1.stun.gamestream.nvidia.com:19308";
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(12);
const DISCOVERY_REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_DISCOVERY_REGIONS: usize = 32;
const DISCOVERY_CONCURRENCY: usize = 4;

#[derive(Clone)]
struct ActiveSession {
    session_id: String,
    control_base: String,
    server_ip: Option<String>,
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

pub struct CloudMatchService {
    client: Client,
    active: Mutex<Option<ActiveSession>>,
    discovered: Mutex<HashMap<String, Value>>,
    conflict: Mutex<Option<SessionConflict>>,
}

impl CloudMatchService {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            active: Mutex::new(None),
            discovered: Mutex::new(HashMap::new()),
            conflict: Mutex::new(None),
        }
    }

    pub fn create(
        &self,
        params: &Value,
        settings: &Value,
        auth: &AuthSession,
        device_id: &str,
    ) -> Result<Value, ServiceError> {
        let client = client_for_settings(&self.client, settings).map_err(invalid)?;
        let app_id = launch_app_id(params)?;
        *self
            .conflict
            .lock()
            .expect("CloudMatch conflict state poisoned") = None;
        let token = session_token(auth);
        let requested_base = requested_streaming_base(params, settings, auth)?;
        let base = self.resolve_create_base(&client, &requested_base, token, device_id, true);
        let body = build_create_body(&app_id, params, settings, device_id);
        let keyboard_layout = setting_string(settings, "keyboardLayout", "en-US");
        let language = setting_string(settings, "gameLanguage", "en_US");
        let mut url = base
            .join("v2/session")
            .map_err(|_| invalid("Invalid CloudMatch session URL"))?;
        url.query_pairs_mut()
            .append_pair("keyboardLayout", &keyboard_layout)
            .append_pair("languageCode", &language);
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
        let request_codec = json!({
            "sessionId":info["sessionId"],
            "negotiatedStreamProfile":{
                "codec":codec_from_wire(&body["sessionRequestData"]["requestedStreamingFeatures"]["codec"]),
                "codecSource":"request"
            }
        });
        preserve_session_codec(&mut info, &request_codec);

        if let Some(session_id) = info["sessionId"].as_str() {
            let mut resume_url = base
                .join(&format!("v2/session/{session_id}"))
                .map_err(|_| invalid("Invalid CloudMatch resume URL"))?;
            resume_url
                .query_pairs_mut()
                .append_pair("keyboardLayout", &keyboard_layout)
                .append_pair("languageCode", &language);
            let mut resume = json!({
                "action": 2,
                "data": "RESUME",
                "sessionRequestData": build_create_body(&app_id, params, settings, device_id)["sessionRequestData"],
                "metaData": null,
                "adUpdates": null
            });
            if let Some(hdr_mode) = accepted_hdr_mode(&payload["session"]) {
                resume["sessionRequestData"]["sdrHdrMode"] = json!(hdr_mode);
                resume["sessionRequestData"]["clientRequestMonitorSettings"][0]["sdrHdrMode"] =
                    json!(hdr_mode);
                resume["sessionRequestData"]["clientRequestMonitorSettings"][0]["displayData"] =
                    monitor_display_data(hdr_mode == 1);
                resume["sessionRequestData"]["requestedStreamingFeatures"]["trueHdr"] =
                    json!(hdr_mode == 1);
            }
            // Fresh native sessions remain pollable even if this compatibility
            // mutation is not accepted by an older CloudMatch pool.
            let _ = client
                .put(resume_url)
                .headers(cloudmatch_headers(token, device_id)?)
                .json(&resume)
                .send();
        }

        self.store_active(&mut info, &base, &zone, &app_id, client)?;
        info["phase"] =
            Value::String(session_phase(info["status"].as_i64().unwrap_or_default()).to_owned());
        Ok(json!({"session":info}))
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
        let control_base = params["streamingBaseUrl"]
            .as_str()
            .map(ToOwned::to_owned)
            .or_else(|| current.as_ref().map(|state| state.control_base.clone()))
            .ok_or_else(|| invalid("No active session control endpoint"))?;
        let base = current
            .as_ref()
            .and_then(|state| state.server_ip.as_deref())
            .filter(|server| control_base.contains(*server))
            .and_then(|server| trusted_learned_server_base(server).ok())
            .map_or_else(|| trusted_cloudmatch_base(&control_base), Ok)?;
        let token = session_token(auth);
        let headers = cloudmatch_headers(token, device_id)?;
        let payload = self.get_session(&client, &base, &session_id, &headers)?;
        let zone = current
            .as_ref()
            .map(|state| state.zone.clone())
            .unwrap_or_default();
        let app_id = current
            .as_ref()
            .map(|state| state.app_id.clone())
            .unwrap_or_default();
        let mut info = session_info(&payload, &base, &zone, &app_id, device_id)?;

        if matches!(info["status"].as_i64(), Some(2 | 3))
            && is_zone_hostname(base.host_str().unwrap_or_default())
            && let Some(server_ip) = info["serverIp"].as_str()
            && !server_ip.is_empty()
            && !is_zone_hostname(server_ip)
            && let Ok(direct) = trusted_learned_server_base(server_ip)
            && let Ok(direct_payload) = self.get_session(&client, &direct, &session_id, &headers)
            && let Ok(mut direct_info) =
                session_info(&direct_payload, &direct, &zone, &app_id, device_id)
        {
            preserve_session_codec(&mut direct_info, &info);
            info = direct_info;
        }

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
            .or_else(|| current.as_ref().map(|state| state.session_id.clone()));
        let Some(session_id) = session_id else {
            return Ok(json!({"session":null,"stopped":false}));
        };
        let discovered = self
            .discovered
            .lock()
            .expect("CloudMatch discovery state poisoned")
            .get(&session_id)
            .cloned();
        let base_value = params["streamingBaseUrl"]
            .as_str()
            .map(ToOwned::to_owned)
            .or_else(|| {
                discovered
                    .as_ref()
                    .and_then(|session| session["streamingBaseUrl"].as_str())
                    .map(ToOwned::to_owned)
            })
            .or_else(|| {
                current.as_ref().map(|state| {
                    state
                        .server_ip
                        .as_deref()
                        .filter(|host| !is_zone_hostname(host))
                        .map(|host| format!("https://{host}"))
                        .unwrap_or_else(|| state.control_base.clone())
                })
            })
            .ok_or_else(|| invalid("No active session control endpoint"))?;
        let base = discovered
            .as_ref()
            .and_then(|session| session["serverIp"].as_str())
            .and_then(|server| trusted_learned_server_base(server).ok())
            .or_else(|| {
                current
                    .as_ref()
                    .and_then(|state| state.server_ip.as_deref())
                    .filter(|server| base_value.contains(*server))
                    .and_then(|server| trusted_learned_server_base(server).ok())
            })
            .map_or_else(|| trusted_cloudmatch_base(&base_value), Ok)?;
        let url = base
            .join(&format!("v2/session/{session_id}"))
            .map_err(|_| invalid("Invalid CloudMatch stop URL"))?;
        let response = client
            .delete(url)
            .headers(cloudmatch_headers(session_token(auth), device_id)?)
            .send()
            .map_err(|error| network("Session stop failed", error))?;
        if !response.status().is_success() && response.status().as_u16() != 404 {
            return Err(response_error("Session stop failed", response));
        }
        *self.active.lock().expect("CloudMatch state poisoned") = None;
        self.discovered
            .lock()
            .expect("CloudMatch discovery state poisoned")
            .remove(&session_id);
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
        Ok(json!({"session":null,"stopped":true,"sessionId":session_id}))
    }

    pub fn active(&self) -> Value {
        let state = self.active.lock().expect("CloudMatch state poisoned");
        json!({"session":state.as_ref().map(|session| session.info.clone())})
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
            && error.code == "authentication_required"
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
        let sessions = discover_sessions(&bases, deadline, incomplete, |base, timeout| {
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
                if initial_base == zone_base || error.code == "authentication_required" {
                    Err(error)
                } else {
                    let payload = self.get_session(&client, &zone_base, session_id, &headers)?;
                    initial_base = zone_base.clone();
                    Ok(payload)
                }
            })?;
        let session = &initial_payload["session"];
        let initial_status = value_i64(&session["status"]).unwrap_or_default();
        let learned_server = session_server_ip(session);
        let control_base = learned_server
            .as_deref()
            .and_then(|server| trusted_learned_server_base(server).ok())
            .unwrap_or(initial_base);

        let app_id = first_string(&session["sessionRequestData"]["appId"])
            .or_else(|| first_string(&params["appId"]))
            .unwrap_or_else(|| "0".to_owned());
        if session_requires_resume(initial_status)? {
            let keyboard_layout = setting_string(settings, "keyboardLayout", "en-US");
            let language = setting_string(settings, "gameLanguage", "en_US");
            let mut url = control_base
                .join(&format!("v2/session/{session_id}"))
                .map_err(|_| invalid("Invalid CloudMatch claim URL"))?;
            url.query_pairs_mut()
                .append_pair("keyboardLayout", &keyboard_layout)
                .append_pair("languageCode", &language);
            let body = build_resume_body(&app_id, session, settings, device_id);
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
            .and_then(|session| session.server_ip.as_deref())
            .and_then(|server| trusted_learned_server_base(server).ok())
            .or_else(|| {
                current
                    .as_ref()
                    .and_then(|session| trusted_cloudmatch_base(&session.control_base).ok())
            })
            .ok_or_else(|| invalid("No active session control endpoint"))?;
        let url = base
            .join(&format!("v2/session/{session_id}"))
            .map_err(|_| invalid("Invalid session ad update URL"))?;
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
        if let Some(previous) = active.as_ref() {
            preserve_session_codec(info, &previous.info);
        }
        *active = Some(ActiveSession {
            session_id,
            control_base: info["streamingBaseUrl"]
                .as_str()
                .unwrap_or_else(|| fallback_base.as_str())
                .to_owned(),
            server_ip: info["serverIp"].as_str().map(ToOwned::to_owned),
            zone: zone.to_owned(),
            app_id: app_id.to_owned(),
            info: info.clone(),
            client,
        });
        Ok(())
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

    fn resolve_create_base(
        &self,
        client: &Client,
        requested: &Url,
        token: &str,
        device_id: &str,
        prefer_regional: bool,
    ) -> Url {
        let host = requested.host_str().unwrap_or_default();
        if host != "prod.cloudmatchbeta.nvidiagrid.net" {
            return requested.clone();
        }
        let Ok(url) = requested.join("v2/serverInfo") else {
            return requested.clone();
        };
        let Ok(headers) = cloudmatch_headers(token, device_id) else {
            return requested.clone();
        };
        let Ok(response) = client.get(url).headers(headers).send() else {
            return requested.clone();
        };
        if !response.status().is_success() {
            return requested.clone();
        }
        let Ok(payload) = response.json::<Value>() else {
            return requested.clone();
        };
        regional_bases(&payload)
            .into_iter()
            .find(|base| {
                !prefer_regional || !base.host_str().unwrap_or_default().starts_with("np-")
            })
            .unwrap_or_else(|| requested.clone())
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
    let raw = params["streamingBaseUrl"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            settings["region"]
                .as_str()
                .filter(|value| value.starts_with("https://"))
        })
        .unwrap_or_else(|| {
            if auth.provider.streaming_service_url.trim().is_empty() {
                DEFAULT_STREAMING_BASE
            } else {
                &auth.provider.streaming_service_url
            }
        });
    trusted_cloudmatch_base(raw)
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
        info["phase"] = json!("failed");
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

fn monitor_display_data(hdr: bool) -> Value {
    json!({
        "displayPrimaryX0":0,"displayPrimaryY0":0,"displayPrimaryX1":0,"displayPrimaryY1":0,
        "displayPrimaryX2":0,"displayPrimaryY2":0,"displayWhitePointX":0,"displayWhitePointY":0,
        "desiredContentMaxLuminance":if hdr { 1000 } else { 0 },
        "desiredContentMinLuminance":0,
        "desiredContentMaxFrameAverageLuminance":if hdr { 400 } else { 0 }
    })
}

fn build_create_body(app_id: &str, params: &Value, settings: &Value, device_id: &str) -> Value {
    let (width, height) = parse_resolution(&setting_string(settings, "resolution", "1920x1080"));
    let fps = setting_i64(settings, "fps", 60).clamp(30, 240);
    let bitrate = setting_i64(settings, "maxBitrateMbps", 75).clamp(1, 200) * 1000;
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
    let persistence = setting_bool(settings, "enablePersistingInGameSettings", false)
        && params["supportsInGameSettingsPersistence"].as_bool() == Some(true);
    let physical_resolution = json!({
        "horizontalPixels": width,
        "verticalPixels": height
    })
    .to_string();
    let metadata = vec![
        json!({"key":"ClientImeSupport","value":"0"}),
        json!({"key":"SubSessionId","value":random_uuid()}),
        json!({"key":"clientPhysicalResolution","value":physical_resolution}),
        json!({"key":"networkType","value":"Unknown"}),
        json!({"key":"wssignaling","value":"1"}),
        json!({"key":"surroundAudioInfo","value":"2"}),
    ];
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
        "hudStreamingMode":0,
        "codec":codec,
        "maxBitrateKbps":bitrate,
        "vsync":false,
        "audioChannelCount":2
    });
    features["mouseMovementFlags"] = json!(0);
    features["trueHdr"] = json!(hdr);
    features["hidDevices"] = Value::Null;
    features["qosPolicy"] = json!(0);
    features["touchSupport"] = json!(false);
    features["dynamicStreamingMode"] = json!(0);
    json!({"sessionRequestData":{
        "appId":app_id.parse::<i64>().unwrap_or_default(),
        "externalAppId":null,
        "internalTitle":params["title"].as_str(),
        "availableSupportedControllers":[2],
        "preferredController":2,
        "networkTestSessionId":null,
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
            "displayData":monitor_display_data(hdr),
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
    let signaling_connection = connections
        .iter()
        .find(|connection| value_i64(&connection["usage"]) == Some(14))
        .or_else(|| {
            connections
                .iter()
                .find(|connection| connection["ip"].is_string())
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
    let resource = signaling_connection
        .and_then(|connection| connection["resourcePath"].as_str())
        .unwrap_or("/nvst/");
    let signaling_url = signaling_url(resource, &server_ip);
    let control_base = control_host
        .as_deref()
        .filter(|host| is_zone_hostname(host))
        .map(|host| format!("https://{}", host.to_lowercase()))
        // Keep regional discovery separate from the learned native/control IP.
        // Direct polling responses do not always repeat sessionControlInfo.
        .or_else(|| {
            trusted_cloudmatch_base(&format!("https://{zone}"))
                .ok()
                .map(|base| base.origin().ascii_serialization())
        })
        .unwrap_or_else(|| fallback_base.origin().ascii_serialization());
    let queue_position = queue_position(session);
    let seat_setup_step = value_i64(&session["seatSetupInfo"]["seatSetupStep"]);
    let app_id = first_string(&session["sessionRequestData"]["appId"])
        .unwrap_or_else(|| fallback_app_id.to_owned());
    let rtsps_endpoints = connections
        .iter()
        .filter(|connection| {
            value_i64(&connection["usage"]) == Some(16)
                || value_i64(&connection["appLevelProtocol"]) == Some(6)
                || connection["resourcePath"].as_str().is_some_and(|value| {
                    value.starts_with("rtsps://") || value.starts_with("rtsp://")
                })
        })
        .filter_map(|connection| {
            connection["resourcePath"]
                .as_str()
                .filter(|value| value.starts_with("rtsps://") || value.starts_with("rtsp://"))
                .map(ToOwned::to_owned)
                .or_else(|| {
                    first_string(&connection["ip"]).map(|host| {
                        let port = value_i64(&connection["port"]).unwrap_or(322);
                        format!("rtsps://{host}:{port}")
                    })
                })
        })
        .collect::<Vec<_>>();
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
    negotiated["enableHdr"] = json!(accepted_hdr_mode(session) == Some(1));
    let ad_state = normalize_ad_state(session);
    Ok(json!({
        "sessionId":session_id,
        "subSessionId":session["subSessionId"],
        "appId":app_id,
        "status":status,
        "phase":session_phase(status),
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
    value_i64(&session["sdrHdrMode"])
        .or_else(|| {
            value_i64(
                &session["sessionRequestData"]["clientRequestMonitorSettings"][0]["sdrHdrMode"],
            )
        })
        .or_else(|| value_i64(&session["sessionRequestData"]["sdrHdrMode"]))
        .map(|mode| i64::from(mode == 1))
}

fn preserve_session_codec(info: &mut Value, previous: &Value) {
    let Some(session_id) = info["sessionId"].as_str().filter(|id| !id.is_empty()) else {
        return;
    };
    let profile = &previous["negotiatedStreamProfile"];
    if previous["sessionId"].as_str() != Some(session_id)
        || !(info["negotiatedStreamProfile"]["codecSource"] == "unreported"
            || (info["negotiatedStreamProfile"]["codecSource"] == "request"
                && profile["codecSource"] == "server"))
        || !matches!(profile["codec"].as_str(), Some("H264" | "H265" | "AV1"))
        || !matches!(profile["codecSource"].as_str(), Some("request" | "server"))
    {
        return;
    }
    info["negotiatedStreamProfile"]["codec"] = profile["codec"].clone();
    info["negotiatedStreamProfile"]["codecSource"] = profile["codecSource"].clone();
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
        0 | 8 => Some(0),
        1 | 10 => Some(1),
        _ => None,
    });
    let chroma = value_i64(&features["chromaFormat"]).and_then(|value| match value {
        0 => Some(0),
        1 => Some(1),
        _ => None,
    });
    let color = match (bit_depth, chroma) {
        (Some(0), Some(0)) => Some("8bit_420"),
        (Some(0), Some(1)) => Some("8bit_444"),
        (Some(1), Some(0)) => Some("10bit_420"),
        (Some(1), Some(1)) => Some("10bit_444"),
        _ => None,
    };
    json!({
        "resolution":resolution,
        "fps":value_i64(&monitor["framesPerSecond"]),
        "codec":codec,
        "colorQuality":color,
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
        code: if status.as_u16() == 401 || status.as_u16() == 403 {
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

fn trusted_cloudmatch_base(raw: &str) -> Result<Url, ServiceError> {
    let mut url = Url::parse(raw.trim()).map_err(|_| invalid("Invalid CloudMatch endpoint"))?;
    let host = url
        .host_str()
        .unwrap_or_default()
        .trim_end_matches('.')
        .to_lowercase();
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some_and(|port| port != 443)
        || !(host == "nvidiagrid.net" || host.ends_with(".nvidiagrid.net"))
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
    let mut url = Url::parse(&raw).map_err(|_| invalid("Invalid learned session endpoint"))?;
    let host = url
        .host_str()
        .unwrap_or_default()
        .trim_end_matches('.')
        .to_lowercase();
    let trusted_hostname = host == "nvidiagrid.net" || host.ends_with(".nvidiagrid.net");
    let trusted_ip = host.parse::<IpAddr>().is_ok_and(|address| match address {
        IpAddr::V4(address) => {
            !address.is_private()
                && !address.is_loopback()
                && !address.is_link_local()
                && !address.is_unspecified()
        }
        IpAddr::V6(address) => {
            !address.is_loopback() && !address.is_unicast_link_local() && !address.is_unspecified()
        }
    });
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some_and(|port| port != 443)
        || (!trusted_hostname && !trusted_ip)
    {
        return Err(invalid("Untrusted learned session endpoint"));
    }
    url.set_path("/");
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

fn session_server_ip(session: &Value) -> Option<String> {
    session["connectionInfo"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|connection| {
            value_i64(&connection["usage"]) == Some(14) && first_string(&connection["ip"]).is_some()
        })
        .and_then(|connection| first_string(&connection["ip"]))
        .or_else(|| first_string(&session["sessionControlInfo"]["ip"]))
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
        "signalingUrl":server_ip.as_deref().map(|server| format!("wss://{server}:443/nvst/")),
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
            Err(error) if error.code == "authentication_required" => return Err(error),
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

fn signaling_url(resource: &str, server_ip: &str) -> String {
    if resource.starts_with("rtsps://") || resource.starts_with("rtsp://") {
        return format!(
            "wss://{}",
            resource
                .split_once("://")
                .map(|pair| pair.1)
                .unwrap_or_default()
        );
    }
    if resource.starts_with("wss://") {
        return resource.to_owned();
    }
    if resource.starts_with('/') {
        return format!("wss://{server_ip}:443{resource}");
    }
    format!("wss://{server_ip}:443/nvst/")
}

fn session_phase(status: i64) -> &'static str {
    match status {
        1 => "preparing",
        2 => "ready",
        3 => "streaming",
        4 | 5 => "paused",
        6 => "resuming",
        status if status > 3 => "failed",
        _ => "requesting",
    }
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

fn setting_i64(settings: &Value, key: &str, fallback: i64) -> i64 {
    value_i64(&settings[key]).unwrap_or(fallback)
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

fn is_zone_hostname(value: &str) -> bool {
    let host = value.trim().trim_end_matches('.').to_ascii_lowercase();
    host == "cloudmatchbeta.nvidiagrid.net"
        || host.ends_with(".cloudmatchbeta.nvidiagrid.net")
        || host == "cloudmatch.nvidiagrid.net"
        || host.ends_with(".cloudmatch.nvidiagrid.net")
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
            Some(json!({"serverIp":"https://example.com"})),
        ] {
            assert_eq!(
                claim_lookup_base(session.as_ref(), &create_region),
                create_region
            );
        }
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
            assert_eq!(error.code, "authentication_required");
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
            assert_eq!(error.code, "authentication_required");
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
            assert_eq!(info["phase"], "failed");
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
            "https://np-sof-01.cloudmatchbeta.nvidiagrid.net"
        );
        assert_eq!(info["serverIp"], "80.84.160.10");
        assert_eq!(info["rtspsEndpoints"][0], "rtsps://80.84.160.10:322");
        assert!(
            trusted_cloudmatch_base(direct.as_str()).is_err(),
            "arbitrary caller-supplied IPs must remain rejected"
        );
    }

    #[test]
    fn hdr_444_request_and_accepted_session_preserve_wire_chroma() {
        let capabilities = json!({"protocolVersion":6,"nativeHdrSupported":true,"videoBackends":[{
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
            assert_eq!(request["requestedStreamingFeatures"]["trueHdr"], true);
            assert_eq!(request["requestedStreamingFeatures"]["codec"], 2);
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
        let capabilities = json!({"protocolVersion":6,"nativeHdrSupported":true,"videoBackends":[{
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
        assert_eq!(request["requestedStreamingFeatures"]["trueHdr"], true);
        assert_eq!(request["requestedStreamingFeatures"]["codec"], 2);
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
            assert_eq!(display_data["desiredContentMaxLuminance"], 0);
            assert_eq!(display_data["desiredContentMaxFrameAverageLuminance"], 0);
            assert_eq!(display_data["desiredContentMinLuminance"], 0);
            assert_eq!(
                body["sessionRequestData"]["requestedStreamingFeatures"]["trueHdr"],
                false
            );
        }
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
        assert_eq!(request["requestedStreamingFeatures"]["codec"], 3);
        assert_eq!(
            request["requestedStreamingFeatures"]["maxBitrateKbps"],
            80000
        );
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
        assert_eq!(
            request["requestedStreamingFeatures"]["dynamicStreamingMode"],
            0
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
    fn native_request_does_not_invent_the_clients_network_type() {
        let body = build_create_body("12345", &json!({}), &json!({}), "device-id");
        let network = body["sessionRequestData"]["metaData"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["key"] == "networkType")
            .unwrap();
        assert_eq!(network["value"], "Unknown");
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
        assert_eq!(features["codec"], 0);
        assert_eq!(features["bitDepth"], 1);
        assert_eq!(features["chromaFormat"], 1);
        assert_eq!(features["maxBitrateKbps"], 75_000);
        assert_eq!(features["dynamicStreamingMode"], 0);
        assert_eq!(features["audioChannelCount"], 2);
        assert_eq!(features["vsync"], false);
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
        assert_eq!(request["requestedStreamingFeatures"]["codec"], 3);
        assert_eq!(request["requestedStreamingFeatures"]["bitDepth"], 1);
        assert_eq!(request["requestedStreamingFeatures"]["chromaFormat"], 0);
        assert_eq!(
            request["requestedStreamingFeatures"]["dynamicStreamingMode"],
            0
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
        assert_eq!(features["codec"], 3);
        assert_eq!(features["bitDepth"], 1);
        assert_eq!(features["chromaFormat"], 0);
    }

    #[test]
    fn manual_h265_preserves_codec_and_ten_bit_color_on_native_nvst() {
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
        assert_eq!(request["requestedStreamingFeatures"]["codec"], 2);
        assert_eq!(request["requestedStreamingFeatures"]["bitDepth"], 1);
        assert_eq!(request["requestedStreamingFeatures"]["chromaFormat"], 0);
        assert_eq!(
            request["requestedStreamingFeatures"]["dynamicStreamingMode"],
            0
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
        assert_eq!(features["codec"], 1);
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
                "connectionInfo":[{"usage":14,"ip":"80.1.2.3","port":443,"resourcePath":"/nvst/"}]}
        });
        let info = session_info(&ready, &base, "auto", "123", "device").unwrap();
        assert_eq!(info["signalingUrl"], "wss://80.1.2.3:443/nvst/");
        assert_eq!(info["serverIp"], "80.1.2.3");
    }

    #[test]
    fn omitted_codec_preserves_the_exact_session_request_through_hdr_preparation() {
        let base = Url::parse(DEFAULT_STREAMING_BASE).unwrap();
        let requested = json!({"codec":"h265","colorQuality":"10bit_420","enableHdr":true,"nativeHdrSupported":true});
        let body = build_create_body("123", &json!({}), &requested, "device");
        let mut initial = session_info(
            &json!({"session":{"sessionId":"omitted-codec","status":1,"sdrHdrMode":1}}),
            &base,
            "auto",
            "123",
            "device",
        )
        .unwrap();
        let request = json!({"sessionId":initial["sessionId"],"negotiatedStreamProfile":{
            "codec":codec_from_wire(&body["sessionRequestData"]["requestedStreamingFeatures"]["codec"]),
            "codecSource":"request"
        }});
        preserve_session_codec(&mut initial, &request);
        assert_eq!(initial["negotiatedStreamProfile"]["codec"], "H265");
        let capabilities = json!({"protocolVersion":6,"nativeHdrSupported":true,"videoBackends":[{
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
            preserve_session_codec(&mut ready, &initial);
            assert_eq!(ready["negotiatedStreamProfile"]["codec"], "H265");
            assert_eq!(ready["negotiatedStreamProfile"]["codecSource"], "request");
            let prepared = crate::streamer::StreamerService::new()
                .prepare_embedded(
                    &json!({"session":ready,"runtimeCapabilities":capabilities}),
                    &json!({"codec":"h264","colorQuality":"8bit_420","enableHdr":false}),
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
        preserve_session_codec(&mut different, &previous);
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
            preserve_session_codec(&mut info, &previous);
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
            preserve_session_codec(&mut info, &previous);
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
        preserve_session_codec(&mut regional, &request);
        let mut direct = session_info(&json!({"session":{
            "sessionId":"same-seat","status":2,"finalizedStreamingFeatures":{"bitDepth":1,"chromaFormat":0}
        }}), &base, "auto", "123", "device").unwrap();
        preserve_session_codec(&mut direct, &regional);
        preserve_session_codec(&mut direct, &request);
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
    fn nested_negotiated_codec_reaches_hdr_preparation() {
        let base = Url::parse(DEFAULT_STREAMING_BASE).unwrap();
        let capabilities = json!({"protocolVersion":6,"nativeHdrSupported":true,"videoBackends":[{
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
        assert!(trusted_cloudmatch_base("https://example.com").is_err());
        assert!(
            trusted_cloudmatch_base("https://prod.cloudmatchbeta.nvidiagrid.net.evil.test")
                .is_err()
        );
    }
}
