use crate::sources::gfn::push::PushError;
use crate::sources::gfn::push::decrypt::EceKeyPair;
use crate::sources::gfn::push::protocol::{
    CheckinRequest, decode_checkin_response, encode_checkin_request,
};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use std::io::Read as _;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushIdentity {
    pub project_id: String,
    pub api_key: String,
    pub sender_id: String,
    pub app_id: String,
    pub firebase_app_id: String,
    pub vapid_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushEndpoints {
    pub checkin: String,
    pub c2dm: String,
    pub fis: String,
    pub fcm_registration: String,
    pub fcm_send: String,
    pub mcs_host: String,
    pub mcs_port: u16,
    pub pns: String,
    pub pns_client_id: String,
}

impl Default for PushEndpoints {
    fn default() -> Self {
        Self {
            checkin: "https://android.clients.google.com/checkin".into(),
            c2dm: "https://android.clients.google.com/c2dm/register3".into(),
            fis: "https://firebaseinstallations.googleapis.com/v1".into(),
            fcm_registration: "https://fcmregistrations.googleapis.com/v1".into(),
            fcm_send: "https://fcm.googleapis.com/fcm/send".into(),
            mcs_host: "mtalk.google.com".into(),
            mcs_port: 5228,
            pns: String::new(),
            pns_client_id: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    #[cfg(test)]
    Get,
    Post,
}

pub struct HttpRequest {
    pub url: String,
    pub method: HttpMethod,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub timeout: Duration,
}

pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

pub trait PushHttp: Send + Sync {
    fn execute(&self, request: &HttpRequest) -> Result<HttpResponse, PushError>;
}

pub struct RegistrationClient<'a> {
    pub http: &'a dyn PushHttp,
    pub endpoints: &'a PushEndpoints,
    pub timeout: Duration,
}

pub struct ReqwestPushHttp {
    client: reqwest::blocking::Client,
}

impl ReqwestPushHttp {
    pub fn new() -> Result<Self, PushError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| PushError::new("push_http_failed", "Could not start the push requests"))?;
        Ok(Self { client })
    }
}

pub const MAXIMUM_RESPONSE_BYTES: u64 = 64 * 1024;
pub const DEVICE_IDENTITY_REJECTED: &str = "push_device_identity_rejected";

pub(crate) fn read_bounded(reader: impl std::io::Read, limit: u64) -> Result<Vec<u8>, PushError> {
    let mut bytes = Vec::new();
    reader
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| PushError::new("push_http_failed", "The push response could not be read"))?;
    if bytes.len() as u64 > limit {
        return Err(PushError::new(
            "push_http_failed",
            "The push response exceeded the accepted size",
        ));
    }
    Ok(bytes)
}

impl PushHttp for ReqwestPushHttp {
    fn execute(&self, request: &HttpRequest) -> Result<HttpResponse, PushError> {
        let mut builder = match request.method {
            #[cfg(test)]
            HttpMethod::Get => self.client.get(&request.url),
            HttpMethod::Post => self.client.post(&request.url),
        }
        .timeout(request.timeout);
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        let response = builder
            .body(request.body.clone())
            .send()
            .map_err(|_| PushError::new("push_http_failed", "The push request failed"))?;
        let status = response.status().as_u16();
        let body = read_bounded(response, MAXIMUM_RESPONSE_BYTES)?;
        Ok(HttpResponse { status, body })
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Registration {
    pub android_id: i64,
    pub security_token: u64,
    pub gcm_token: String,
    pub fcm_token: String,
    pub endpoint: String,
    pub private_key: Vec<u8>,
    pub public_key: Vec<u8>,
    pub auth_secret: Vec<u8>,
    #[serde(default)]
    pub received_persistent_ids: Vec<String>,
    #[serde(default)]
    pub created_at_seconds: u64,
    #[serde(default)]
    pub fingerprint: String,
}

pub struct CheckinResult {
    pub android_id: i64,
    pub security_token: u64,
}

pub fn checkin(
    client: &RegistrationClient<'_>,
    android_id: Option<i64>,
    security_token: Option<u64>,
) -> Result<CheckinResult, PushError> {
    let body = encode_checkin_request(&CheckinRequest {
        android_id,
        security_token,
    });
    let response = client.http.execute(&HttpRequest {
        url: client.endpoints.checkin.clone(),
        method: HttpMethod::Post,
        headers: vec![("Content-Type".into(), "application/x-protobuf".into())],
        body,
        timeout: client.timeout,
    })?;
    if response.status == 401 || response.status == 400 {
        return Err(PushError::new(
            DEVICE_IDENTITY_REJECTED,
            "The device registration service rejected the stored device identity",
        ));
    }
    if response.status != 200 {
        return Err(PushError::new(
            "push_checkin_refused",
            "The device registration service refused the check-in",
        ));
    }
    let decoded = decode_checkin_response(&response.body)?;
    let android_id = decoded.android_id.ok_or_else(|| {
        PushError::new(
            "push_checkin_incomplete",
            "The check-in response did not carry a device identity",
        )
    })?;
    let security_token = decoded.security_token.ok_or_else(|| {
        PushError::new(
            "push_checkin_incomplete",
            "The check-in response did not carry a device token",
        )
    })?;
    if android_id == 0 || security_token == 0 {
        return Err(PushError::new(
            "push_checkin_incomplete",
            "The check-in response carried an empty device identity",
        ));
    }
    Ok(CheckinResult {
        android_id: i64::try_from(android_id).map_err(|_| {
            PushError::new(
                "push_checkin_incomplete",
                "The device identity is outside the accepted range",
            )
        })?,
        security_token,
    })
}

pub fn gcm_register(
    client: &RegistrationClient<'_>,
    android_id: i64,
    security_token: u64,
    identity: &PushIdentity,
) -> Result<String, PushError> {
    let mut form = format!(
        "app=org.chromium.linux&X-subtype={}&device={android_id}",
        urlencode(&identity.app_id)
    );
    if !identity.sender_id.is_empty() {
        form.push_str(&format!("&sender={}", urlencode(&identity.sender_id)));
    }
    let response = client.http.execute(&HttpRequest {
        url: client.endpoints.c2dm.clone(),
        method: HttpMethod::Post,
        headers: vec![
            (
                "Content-Type".into(),
                "application/x-www-form-urlencoded".into(),
            ),
            (
                "Authorization".into(),
                format!("AidLogin {android_id}:{security_token}"),
            ),
        ],
        body: form.into_bytes(),
        timeout: client.timeout,
    })?;
    if response.status != 200 {
        return Err(PushError::new(
            "push_gcm_refused",
            "The messaging service refused the registration",
        ));
    }
    let text = String::from_utf8_lossy(&response.body);
    let mut parts = text.split('=');
    let name = parts.next().unwrap_or_default();
    let value = parts.next().unwrap_or_default();
    if name != "token" || value.is_empty() {
        return Err(PushError::new(
            "push_gcm_refused",
            "The messaging service did not return a token",
        ));
    }
    Ok(value.to_owned())
}

pub fn firebase_installation_token(
    client: &RegistrationClient<'_>,
    identity: &PushIdentity,
    fid: &str,
) -> Result<String, PushError> {
    if identity.project_id.is_empty() || identity.api_key.is_empty() {
        return Err(PushError::new(
            "push_config_incomplete",
            "The push identity is incomplete",
        ));
    }
    let body = serde_json::json!({
        "fid": fid,
        "authVersion": "FIS_v2",
        "appId": identity.firebase_app_id,
        "sdkVersion": "w:0.1.0",
    })
    .to_string();
    let response = client.http.execute(&HttpRequest {
        url: format!(
            "{}/projects/{}/installations",
            client.endpoints.fis.trim_end_matches('/'),
            identity.project_id
        ),
        method: HttpMethod::Post,
        headers: vec![
            ("Content-Type".into(), "application/json".into()),
            ("Accept".into(), "application/json".into()),
            ("x-goog-api-key".into(), identity.api_key.clone()),
            (
                "x-firebase-client".into(),
                URL_SAFE_NO_PAD.encode(b"{\"heartbeats\": [], \"version\": 2}"),
            ),
        ],
        body: body.into_bytes(),
        timeout: client.timeout,
    })?;
    if response.status != 200 {
        return Err(PushError::new(
            "push_fis_refused",
            "The installation service refused the request",
        ));
    }
    let value: serde_json::Value = serde_json::from_slice(&response.body).map_err(|_| {
        PushError::new(
            "push_fis_incomplete",
            "The installation response could not be read",
        )
    })?;
    value["authToken"]["token"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| {
            PushError::new(
                "push_fis_incomplete",
                "The installation response did not carry a token",
            )
        })
}

pub fn fcm_register(
    client: &RegistrationClient<'_>,
    identity: &PushIdentity,
    installation_token: &str,
    gcm_token: &str,
    public_key: &[u8],
    auth_secret: &[u8],
) -> Result<(String, String), PushError> {
    let endpoint = format!(
        "{}/{}",
        client.endpoints.fcm_send.trim_end_matches('/'),
        gcm_token
    );
    let mut web = serde_json::json!({
        "endpoint": endpoint,
        "auth": URL_SAFE_NO_PAD.encode(auth_secret),
        "p256dh": URL_SAFE_NO_PAD.encode(public_key),
    });
    if let Some(vapid) = identity.vapid_key.as_deref() {
        if !vapid.is_empty() {
            web["application_pub_key"] = serde_json::Value::String(vapid.to_owned());
        }
    }
    let body = serde_json::json!({ "web": web }).to_string();
    let response = client.http.execute(&HttpRequest {
        url: format!(
            "{}/projects/{}/registrations",
            client.endpoints.fcm_registration.trim_end_matches('/'),
            identity.project_id
        ),
        method: HttpMethod::Post,
        headers: vec![
            ("Content-Type".into(), "application/json".into()),
            ("Accept".into(), "application/json".into()),
            ("x-goog-api-key".into(), identity.api_key.clone()),
            (
                "x-goog-firebase-installations-auth".into(),
                format!("FIS {installation_token}"),
            ),
        ],
        body: body.into_bytes(),
        timeout: client.timeout,
    })?;
    if response.status != 200 {
        return Err(PushError::new(
            "push_fcm_refused",
            "The messaging service refused the client registration",
        ));
    }
    let value: serde_json::Value = serde_json::from_slice(&response.body).map_err(|_| {
        PushError::new(
            "push_fcm_incomplete",
            "The registration response could not be read",
        )
    })?;
    let token = value["token"].as_str().ok_or_else(|| {
        PushError::new(
            "push_fcm_incomplete",
            "The registration response did not carry a token",
        )
    })?;
    if token.is_empty() {
        return Err(PushError::new(
            "push_fcm_incomplete",
            "The registration response carried an empty token",
        ));
    }
    Ok((token.to_owned(), endpoint))
}

pub fn pns_register(
    client: &RegistrationClient<'_>,
    registration_token: &str,
    device_id: &str,
    previous: &[String],
    gfnjwt: &str,
) -> Result<(), PushError> {
    if client.endpoints.pns.is_empty() || client.endpoints.pns_client_id.is_empty() {
        return Err(PushError::new(
            "push_config_incomplete",
            "The notification service is not configured",
        ));
    }
    let body = serde_json::json!({
        "registrationToken": registration_token,
        "deviceId": device_id,
        "previousRegistrationTokens": previous,
    })
    .to_string();
    let response = client.http.execute(&HttpRequest {
        url: client.endpoints.pns_registrations_url(),
        method: HttpMethod::Post,
        headers: vec![
            ("Content-Type".into(), "application/json".into()),
            ("Accept".into(), "application/json".into()),
            (
                "NV-Client-ID".into(),
                client.endpoints.pns_client_id.clone(),
            ),
            ("Authorization".into(), format!("GFNJWT {gfnjwt}")),
        ],
        body: body.into_bytes(),
        timeout: client.timeout,
    })?;
    match response.status {
        200 | 201 | 202 | 204 => Ok(()),
        _ => Err(PushError::new(
            "push_pns_refused",
            "The notification service refused the registration",
        )),
    }
}

#[cfg(test)]
pub fn pns_unregister(
    client: &RegistrationClient<'_>,
    previous_tokens: &[String],
    gfnjwt: &str,
) -> Result<(), PushError> {
    if client.endpoints.pns.is_empty() || client.endpoints.pns_client_id.is_empty() {
        return Err(PushError::new(
            "push_config_incomplete",
            "The notification service is not configured",
        ));
    }
    let body = serde_json::json!({ "previousRegistrationTokens": previous_tokens }).to_string();
    let response = client.http.execute(&HttpRequest {
        url: client.endpoints.pns_unregister_url(),
        method: HttpMethod::Post,
        headers: vec![
            ("Content-Type".into(), "application/json".into()),
            ("Accept".into(), "application/json".into()),
            (
                "NV-Client-ID".into(),
                client.endpoints.pns_client_id.clone(),
            ),
            ("Authorization".into(), format!("GFNJWT {gfnjwt}")),
        ],
        body: body.into_bytes(),
        timeout: client.timeout,
    })?;
    match response.status {
        200 | 202 | 204 => Ok(()),
        _ => Err(PushError::new(
            "push_pns_refused",
            "The notification service refused the unregistration",
        )),
    }
}

impl PushEndpoints {
    pub fn pns_registrations_url(&self) -> String {
        format!("{}/registrations", self.pns.trim_end_matches('/'))
    }

    #[cfg(test)]
    pub fn pns_unregister_url(&self) -> String {
        format!("{}/unregister", self.pns.trim_end_matches('/'))
    }
}

pub fn register(
    client: &RegistrationClient<'_>,
    identity: &PushIdentity,
    existing: Option<&Registration>,
    gfnjwt: &str,
    device_id: &str,
    guard: &mut dyn FnMut() -> bool,
) -> Result<Registration, PushError> {
    if identity.project_id.is_empty() || identity.api_key.is_empty() || identity.app_id.is_empty() {
        return Err(PushError::new(
            "push_config_incomplete",
            "The push identity is incomplete",
        ));
    }
    let (android_id, security_token, received) = match existing {
        Some(registration) => (
            Some(registration.android_id),
            Some(registration.security_token),
            registration.received_persistent_ids.clone(),
        ),
        None => (None, None, Vec::new()),
    };
    stage(guard)?;
    let checked = checkin(client, android_id, security_token)?;
    stage(guard)?;
    let gcm_token = gcm_register(client, checked.android_id, checked.security_token, identity)?;
    let keys = EceKeyPair::generate()?;
    let fid = random_fid();
    stage(guard)?;
    let installation_token = firebase_installation_token(client, identity, &fid)?;
    stage(guard)?;
    let (fcm_token, endpoint) = fcm_register(
        client,
        identity,
        &installation_token,
        &gcm_token,
        keys.public_key(),
        keys.auth_secret(),
    )?;
    let previous: Vec<String> = existing
        .map(|registration| registration.fcm_token.as_str())
        .filter(|token| !token.is_empty() && *token != fcm_token)
        .map(str::to_owned)
        .into_iter()
        .collect();
    stage(guard)?;
    pns_register(client, &fcm_token, device_id, &previous, gfnjwt)?;
    Ok(Registration {
        android_id: checked.android_id,
        security_token: checked.security_token,
        gcm_token,
        fcm_token,
        endpoint,
        private_key: keys.private_key(),
        public_key: keys.public_key().to_vec(),
        auth_secret: keys.auth_secret().to_vec(),
        received_persistent_ids: received,
        created_at_seconds: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default(),
        fingerprint: String::new(),
    })
}

pub fn checkin_and_register(
    client: &RegistrationClient<'_>,
    identity: &PushIdentity,
    existing: Option<&Registration>,
    gfnjwt: &str,
    device_id: &str,
    guard: &mut dyn FnMut() -> bool,
) -> Result<Registration, PushError> {
    register(client, identity, existing, gfnjwt, device_id, guard)
}

fn stage(guard: &mut dyn FnMut() -> bool) -> Result<(), PushError> {
    if guard() {
        Ok(())
    } else {
        Err(PushError::new(
            "push_scope_stale",
            "The account changed while the push registration was running",
        ))
    }
}

fn random_fid() -> String {
    use rand::RngCore;
    let mut bytes = [0_u8; 17];
    rand::rng().fill_bytes(&mut bytes);
    bytes[0] = 0b0111_0000 + (bytes[0] % 0b0001_0000);
    let encoded = URL_SAFE_NO_PAD.encode(bytes);
    encoded[..22].to_owned()
}

fn urlencode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    encoded
}
