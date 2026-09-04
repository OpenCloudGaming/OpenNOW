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
use std::time::Duration;
use url::Url;

const LCARS_CLIENT_ID: &str = "ec7e38d4-03af-4b58-b131-cfb0495903ab";
const GFN_CLIENT_VERSION: &str = "2.0.87.131";
const DEFAULT_STREAMING_BASE: &str = "https://prod.cloudmatchbeta.nvidiagrid.net/";
const DEFAULT_STUN_SERVER: &str = "stun:s1.stun.gamestream.nvidia.com:19308";

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

pub struct CloudMatchService {
    client: Client,
    active: Mutex<Option<ActiveSession>>,
    discovered: Mutex<HashMap<String, Value>>,
}

impl CloudMatchService {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            active: Mutex::new(None),
            discovered: Mutex::new(HashMap::new()),
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
        let payload = read_cloudmatch_response("Session creation failed", response)?;
        let zone = params["zone"]
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| base.host_str().map(ToOwned::to_owned))
            .unwrap_or_default();
        let mut info = session_info(&payload, &base, &zone, &app_id, device_id)?;

        if let Some(session_id) = info["sessionId"].as_str() {
            let mut resume_url = base
                .join(&format!("v2/session/{session_id}"))
                .map_err(|_| invalid("Invalid CloudMatch resume URL"))?;
            resume_url
                .query_pairs_mut()
                .append_pair("keyboardLayout", &keyboard_layout)
                .append_pair("languageCode", &language);
            let resume = json!({
                "action": 2,
                "data": "RESUME",
                "sessionRequestData": build_create_body(&app_id, params, settings, device_id)["sessionRequestData"],
                "metaData": null,
                "adUpdates": null
            });
            // Fresh native sessions remain pollable even if this compatibility
            // mutation is not accepted by an older CloudMatch pool.
            let _ = client
                .put(resume_url)
                .headers(cloudmatch_headers(token, device_id)?)
                .json(&resume)
                .send();
        }

        let active = active_from_info(&info, &base, &zone, &app_id, client)?;
        *self.active.lock().expect("CloudMatch state poisoned") = Some(active);
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
            .unwrap_or(trusted_cloudmatch_base(&control_base)?);
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
            && let Ok(direct_info) =
                session_info(&direct_payload, &direct, &zone, &app_id, device_id)
        {
            info = direct_info;
        }

        info["phase"] =
            Value::String(session_phase(info["status"].as_i64().unwrap_or_default()).to_owned());
        let active = active_from_info(&info, &base, &zone, &app_id, client)?;
        *self.active.lock().expect("CloudMatch state poisoned") = Some(active);
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
            .unwrap_or(trusted_cloudmatch_base(&base_value)?);
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
        Ok(json!({"session":null,"stopped":true,"sessionId":session_id}))
    }

    pub fn active(&self) -> Value {
        let state = self.active.lock().expect("CloudMatch state poisoned");
        json!({"session":state.as_ref().map(|session| session.info.clone())})
    }

    pub fn remote_sessions(
        &self,
        params: &Value,
        settings: &Value,
        auth: &AuthSession,
        device_id: &str,
    ) -> Result<Value, ServiceError> {
        let client = client_for_settings(&self.client, settings).map_err(invalid)?;
        let requested = requested_streaming_base(params, settings, auth)?;
        let headers = cloudmatch_headers(session_token(auth), device_id)?;
        let mut bases = vec![requested.clone()];
        if let Ok(server_info_url) = requested.join("v2/serverInfo")
            && let Ok(response) = client.get(server_info_url).headers(headers.clone()).send()
            && response.status().is_success()
            && let Ok(payload) = response.json::<Value>()
        {
            for base in regional_bases(&payload) {
                if !bases.contains(&base) {
                    bases.push(base);
                }
            }
        }

        let mut last_failure = None;
        for base in bases {
            let Ok(url) = base.join("v2/session") else {
                continue;
            };
            match client.get(url).headers(headers.clone()).send() {
                Ok(response) if response.status().is_success() => {
                    let payload = response
                        .json::<Value>()
                        .map_err(|error| network("Invalid active-session response", error))?;
                    if value_i64(&payload["requestStatus"]["statusCode"]) != Some(1) {
                        continue;
                    }
                    let sessions = payload["sessions"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|session| matches!(value_i64(&session["status"]), Some(1..=3)))
                        .filter_map(|session| remote_session_info(session, &base))
                        .collect::<Vec<_>>();
                    let mut discovered = self
                        .discovered
                        .lock()
                        .expect("CloudMatch discovery state poisoned");
                    discovered.clear();
                    for session in &sessions {
                        if let Some(session_id) = session["sessionId"].as_str() {
                            discovered.insert(session_id.to_owned(), session.clone());
                        }
                    }
                    return Ok(json!({"sessions":sessions}));
                }
                Ok(response) => {
                    last_failure = Some(response_error("Active-session discovery failed", response))
                }
                Err(error) => {
                    last_failure = Some(network("Active-session discovery failed", error))
                }
            }
        }
        if let Some(error) = last_failure {
            eprintln!(
                "opennow-core: remote session discovery degraded: {}",
                error.message
            );
        }
        Ok(json!({"sessions":[]}))
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
        let initial_payload = self.get_session(&client, &zone_base, session_id, &headers)?;
        let session = &initial_payload["session"];
        let initial_status = value_i64(&session["status"]).unwrap_or_default();
        let learned_server = session_server_ip(session);
        let control_base = learned_server
            .as_deref()
            .and_then(|server| trusted_learned_server_base(server).ok())
            .unwrap_or_else(|| zone_base.clone());

        let app_id = first_string(&session["sessionRequestData"]["appId"])
            .or_else(|| first_string(&params["appId"]))
            .unwrap_or_else(|| "0".to_owned());
        let recovery_mode = params["recoveryMode"].as_bool() == Some(true);
        if initial_status != 1 && !(recovery_mode && matches!(initial_status, 2 | 3)) {
            let keyboard_layout = setting_string(settings, "keyboardLayout", "en-US");
            let language = setting_string(settings, "gameLanguage", "en_US");
            let mut url = control_base
                .join(&format!("v2/session/{session_id}"))
                .map_err(|_| invalid("Invalid CloudMatch claim URL"))?;
            url.query_pairs_mut()
                .append_pair("keyboardLayout", &keyboard_layout)
                .append_pair("languageCode", &language);
            let body = json!({
                "action": 2,
                "data": "RESUME",
                "sessionRequestData": build_create_body(&app_id, params, settings, device_id)["sessionRequestData"],
                "metaData": null,
                "adUpdates": null
            });
            let response = client
                .put(url)
                .headers(headers.clone())
                .json(&body)
                .send()
                .map_err(|error| network("Session claim failed", error))?;
            let _ = read_cloudmatch_response("Session claim failed", response)?;
        }

        let payload = self.get_session(&client, &control_base, session_id, &headers)?;
        let zone = zone_base.host_str().unwrap_or_default();
        let mut info = session_info(&payload, &control_base, zone, &app_id, device_id)?;
        info["phase"] =
            Value::String(session_phase(info["status"].as_i64().unwrap_or_default()).to_owned());
        let active = active_from_info(&info, &control_base, zone, &app_id, client)?;
        *self.active.lock().expect("CloudMatch state poisoned") = Some(active);
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
        let payload = read_cloudmatch_response("Session ad update failed", response)?;
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
        let active = active_from_info(&info, &base, zone, app_id, client)?;
        *self.active.lock().expect("CloudMatch state poisoned") = Some(active);
        Ok(json!({"session":info}))
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
                    return read_cloudmatch_response("Session polling failed", response);
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

fn build_create_body(app_id: &str, params: &Value, settings: &Value, device_id: &str) -> Value {
    let (width, height) = parse_resolution(&setting_string(settings, "resolution", "1920x1080"));
    let fps = setting_i64(settings, "fps", 60).clamp(30, 240);
    let bitrate = setting_i64(settings, "maxBitrateMbps", 75).clamp(1, 200) * 1000;
    let codec = codec_wire(&setting_string(settings, "codec", "auto"));
    let requested_color = color_quality_wire(&setting_string(settings, "colorQuality", "8bit_420"));
    // Keep a manually selected codec fixed. H.264 supports only 8-bit 4:2:0,
    // while the official Windows client exposes AV1 at 4:2:0 only. Constrain
    // color instead of silently switching an explicit codec back to Auto/HEVC.
    let (bit_depth, chroma) = match codec {
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
        json!({"key":"networkType","value":if cfg!(target_os = "macos") { "WiFi5.0" } else { "Unknown" }}),
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
    features["trueHdr"] = json!(false);
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
            "sdrHdrMode":0,
            "displayData":{
                "displayPrimaryX0":0,"displayPrimaryY0":0,"displayPrimaryX1":0,"displayPrimaryY1":0,
                "displayPrimaryX2":0,"displayPrimaryY2":0,"displayWhitePointX":0,"displayWhitePointY":0,
                "desiredContentMaxLuminance":0,"desiredContentMinLuminance":0,"desiredContentMaxFrameAverageLuminance":0
            },
            "hdr10PlusGamingData":null,
            "dpi":if cfg!(target_os = "macos") { 144 } else { 96 }
        }],
        "useOps":true,
        "audioMode":2,
        "metaData":metadata,
        "sdrHdrMode":0,
        "clientDisplayHdrCapabilities":null,
        "surroundAudioInfo":0,
        "remoteControllersBitmap":0,
        "clientTimezoneOffset":0,
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
    let features = if session["finalizedStreamingFeatures"].is_object() {
        &session["finalizedStreamingFeatures"]
    } else {
        &session["sessionRequestData"]["requestedStreamingFeatures"]
    };
    let negotiated = negotiated_profile(monitor, features);
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

fn negotiated_profile(monitor: &Value, features: &Value) -> Value {
    let width = value_i64(&monitor["widthInPixels"]);
    let height = value_i64(&monitor["heightInPixels"]);
    let resolution = width
        .zip(height)
        .map(|(width, height)| format!("{width}x{height}"));
    let codec = match value_i64(&features["codec"]) {
        Some(1) => Some("H264"),
        Some(2) => Some("H265"),
        Some(3) => Some("AV1"),
        _ => None,
    };
    let bit_depth = value_i64(&features["bitDepth"]).and_then(|value| match value {
        0 | 8 => Some(0),
        1 | 10 => Some(1),
        _ => None,
    });
    let chroma = value_i64(&features["chromaFormat"]).and_then(|value| match value {
        0 => Some(0),
        1..=3 => Some(1),
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

fn active_from_info(
    info: &Value,
    fallback_base: &Url,
    zone: &str,
    app_id: &str,
    client: Client,
) -> Result<ActiveSession, ServiceError> {
    let session_id = info["sessionId"]
        .as_str()
        .ok_or_else(|| upstream("Session result did not include an ID"))?
        .to_owned();
    Ok(ActiveSession {
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

fn read_cloudmatch_response(context: &str, response: Response) -> Result<Value, ServiceError> {
    if !response.status().is_success() {
        return Err(response_error(context, response));
    }
    let payload = response
        .json::<Value>()
        .map_err(|error| network("CloudMatch returned invalid JSON", error))?;
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

fn response_error(context: &str, response: Response) -> ServiceError {
    let status = response.status();
    let detail = response.json::<Value>().ok().and_then(|payload| {
        payload["requestStatus"]["statusDescription"]
            .as_str()
            .map(ToOwned::to_owned)
    });
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
    let session_id = session["sessionId"].as_str()?.to_owned();
    let status = value_i64(&session["status"])?;
    let app_id = value_i64(&session["sessionRequestData"]["appId"]).unwrap_or_default();
    let server_ip = session_server_ip(session);
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
        6 => "stopping",
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
    fn rejects_untrusted_session_endpoints() {
        assert!(trusted_cloudmatch_base("http://prod.cloudmatchbeta.nvidiagrid.net").is_err());
        assert!(trusted_cloudmatch_base("https://example.com").is_err());
        assert!(
            trusted_cloudmatch_base("https://prod.cloudmatchbeta.nvidiagrid.net.evil.test")
                .is_err()
        );
    }
}
