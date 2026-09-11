use crate::gfn::{AuthSession, ServiceError};
use rand::RngCore as _;
use reqwest::blocking::Client;
use reqwest::header::{
    ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue, ORIGIN, REFERER, USER_AGENT,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use url::Url;

const GRAPHQL_URL: &str = "https://apps.gxn.nvidia.com/graphql";
const USER_ACCOUNT_QUERY_HASH: &str =
    "39fa5dbf8c14ac4c873857fd510f337cdc8710d5614038a0625487d41f98986b";
const ALS_BASE: &str = "https://als.geforcenow.com/v1";
const CALLBACK_PORTS: [u16; 5] = [2259, 6460, 7119, 8870, 9096];
const LCARS_CLIENT_ID: &str = "ec7e38d4-03af-4b58-b131-cfb0495903ab";
const CLIENT_VERSION: &str = "2.0.87.131";
const USER_AGENT_VALUE: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/128.0.0.0 Safari/537.36 GFN-PC/2.0.87.131";

struct LinkAttempt {
    provider: String,
    receiver: mpsc::Receiver<Result<Value, String>>,
    expires: Instant,
}

pub struct AccountConnectionsService {
    client: Client,
    attempts: Mutex<HashMap<String, LinkAttempt>>,
}

impl AccountConnectionsService {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            attempts: Mutex::new(HashMap::new()),
        }
    }

    pub fn list(&self, auth: &AuthSession) -> Result<Value, ServiceError> {
        let token = session_token(auth)?;
        let mut url = Url::parse(GRAPHQL_URL).expect("constant GraphQL URL is valid");
        let variables = json!({});
        let extensions = json!({"persistedQuery":{"sha256Hash":USER_ACCOUNT_QUERY_HASH}});
        url.query_pairs_mut()
            .append_pair("requestType", "userAccount")
            .append_pair("extensions", &extensions.to_string())
            .append_pair("variables", &variables.to_string())
            .append_pair("huId", &hex_sha256(&auth.user.user_id));
        let response = self
            .client
            .get(url)
            .headers(graphql_headers(token)?)
            .send()
            .map_err(|error| network("Game-account discovery failed", error))?;
        if !response.status().is_success() {
            return Err(response_error("Game-account discovery failed", response));
        }
        let payload = response
            .json::<Value>()
            .map_err(|error| network("Invalid game-account response", error))?;
        if let Some(message) = payload["errors"]
            .as_array()
            .and_then(|errors| errors.first())
            .and_then(|error| error["message"].as_str())
        {
            return Err(upstream(message));
        }
        let stores = payload["data"]["userAccount"]["storesData"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let fetched_at = unix_millis();
        let accounts = provider_definitions()
            .into_iter()
            .map(|definition| {
                let provider = definition["provider"].as_str().unwrap_or_default();
                let store = stores.iter().find(|store| {
                    normalize_provider(store["store"].as_str().unwrap_or_default()) == provider
                });
                connection_from_store(&definition, store, fetched_at)
            })
            .collect::<Vec<_>>();
        Ok(json!({"accounts":accounts,"fetchedAt":fetched_at}))
    }

    pub fn sync(&self, params: &Value, auth: &AuthSession) -> Result<Value, ServiceError> {
        let provider = required_provider(params)?;
        ensure_provider_feature(&provider, "supportsSync")?;
        let url = format!("{ALS_BASE}/sync/{provider}");
        let response = self
            .client
            .post(url)
            .headers(als_headers(session_token(auth)?, true)?)
            .json(&json!({}))
            .send()
            .map_err(|error| network("Game-account sync failed", error))?;
        if response.status().as_u16() != 202 {
            return Err(response_error("Game-account sync failed", response));
        }
        Ok(json!({"ok":true,"provider":provider,"message":"Library sync started"}))
    }

    pub fn unlink(&self, params: &Value, auth: &AuthSession) -> Result<Value, ServiceError> {
        let provider = required_provider(params)?;
        ensure_provider_feature(&provider, "supportsLinking")?;
        let url = format!("{ALS_BASE}/linking/{provider}");
        let response = self
            .client
            .delete(url)
            .headers(als_headers(session_token(auth)?, false)?)
            .send()
            .map_err(|error| network("Game-account unlink failed", error))?;
        if !response.status().is_success() && response.status().as_u16() != 404 {
            return Err(response_error("Game-account unlink failed", response));
        }
        Ok(json!({"ok":true,"provider":provider}))
    }

    pub fn start_link(&self, params: &Value, auth: &AuthSession) -> Result<Value, ServiceError> {
        let provider = required_provider(params)?;
        ensure_provider_feature(&provider, "supportsLinking")?;
        let (listener, port) = bind_callback()?;
        let redirect_uri = format!("http://localhost:{port}/");
        let mut url =
            Url::parse(&format!("{ALS_BASE}/login_url")).expect("constant ALS URL is valid");
        url.query_pairs_mut()
            .append_pair("platform", &provider)
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("client_id", "gfn-pc");
        let response = self
            .client
            .get(url)
            .headers(als_headers(session_token(auth)?, false)?)
            .send()
            .map_err(|error| network("Account-linking URL failed", error))?;
        if !response.status().is_success() {
            return Err(response_error("Account-linking URL failed", response));
        }
        let login_url = response
            .json::<Value>()
            .ok()
            .and_then(|payload| payload["login_url"].as_str().map(ToOwned::to_owned))
            .ok_or_else(|| upstream("Account-linking URL response was incomplete"))?;
        let attempt_id = random_id();
        let (sender, receiver) = mpsc::channel();
        let worker_provider = provider.clone();
        thread::Builder::new()
            .name(format!("opennow-account-link-{attempt_id}"))
            .spawn(move || {
                let result = wait_for_callback(listener, &worker_provider);
                let _ = sender.send(result);
            })
            .map_err(|error| network("Could not start account-link callback", error))?;
        self.attempts
            .lock()
            .expect("account-link state poisoned")
            .insert(
                attempt_id.clone(),
                LinkAttempt {
                    provider: provider.clone(),
                    receiver,
                    expires: Instant::now() + Duration::from_secs(300),
                },
            );
        Ok(
            json!({"attemptId":attempt_id,"provider":provider,"loginUrl":login_url,"expiresInSeconds":300}),
        )
    }

    pub fn poll_link(&self, params: &Value) -> Result<Value, ServiceError> {
        let attempt_id = params["attemptId"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| invalid("account.connections.link.poll requires attemptId"))?;
        let mut attempts = self.attempts.lock().expect("account-link state poisoned");
        let attempt = attempts.get(attempt_id).ok_or_else(|| ServiceError {
            code: "link_attempt_not_found",
            message: "Account-link attempt is no longer active".to_owned(),
        })?;
        if Instant::now() >= attempt.expires {
            attempts.remove(attempt_id);
            return Ok(json!({"status":"expired"}));
        }
        match attempt.receiver.try_recv() {
            Ok(Ok(result)) => {
                let provider = attempt.provider.clone();
                attempts.remove(attempt_id);
                Ok(json!({"status":"complete","provider":provider,"result":result}))
            }
            Ok(Err(message)) => {
                attempts.remove(attempt_id);
                Ok(json!({"status":"error","message":message}))
            }
            Err(mpsc::TryRecvError::Empty) => Ok(json!({"status":"pending"})),
            Err(mpsc::TryRecvError::Disconnected) => {
                attempts.remove(attempt_id);
                Ok(json!({"status":"error","message":"Account-link callback stopped unexpectedly"}))
            }
        }
    }
}

fn wait_for_callback(listener: TcpListener, provider: &str) -> Result<Value, String> {
    listener
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(300);
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
                let mut buffer = [0_u8; 8192];
                let count = stream
                    .read(&mut buffer)
                    .map_err(|error| error.to_string())?;
                let request = String::from_utf8_lossy(&buffer[..count]);
                let target = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");
                let callback = Url::parse(&format!("http://localhost{target}"))
                    .map_err(|_| "Invalid account-link callback".to_owned())?;
                if !callback.query_pairs().any(|(key, _)| {
                    matches!(
                        key.as_ref(),
                        "platform" | "error" | "display_name" | "expires_in"
                    )
                }) {
                    let _ =
                        stream.write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n");
                    continue;
                }
                let error = callback
                    .query_pairs()
                    .find(|(key, _)| key == "error")
                    .map(|(_, value)| value.into_owned());
                if let Some(error) = error {
                    redirect_callback(&mut stream, provider, true);
                    return Err(error);
                }
                let actual = callback
                    .query_pairs()
                    .find(|(key, _)| key == "platform")
                    .map(|(_, value)| normalize_provider(&value))
                    .ok_or_else(|| "Account-link callback did not include a provider".to_owned())?;
                if actual != provider {
                    redirect_callback(&mut stream, provider, true);
                    return Err("Account-link callback provider mismatch".to_owned());
                }
                let display_name = callback
                    .query_pairs()
                    .find(|(key, _)| key == "display_name")
                    .map(|(_, value)| value.into_owned());
                let expires_in = callback
                    .query_pairs()
                    .find(|(key, _)| key == "expires_in")
                    .map(|(_, value)| value.into_owned());
                redirect_callback(&mut stream, provider, false);
                return Ok(
                    json!({"platform":provider,"displayName":display_name,"expiresIn":expires_in}),
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(100))
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("Timed out waiting for account-linking callback".to_owned())
}

fn redirect_callback(stream: &mut impl Write, provider: &str, failed: bool) {
    let location = if failed {
        format!(
            "https://static-als.nvidia.com/result?platform={provider}&ui_locales=en_US&error=accountlink_fail"
        )
    } else {
        format!("https://static-als.nvidia.com/result?platform={provider}&ui_locales=en_US")
    };
    let response = format!(
        "HTTP/1.1 302 Found\r\nLocation: {location}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
    );
    let _ = stream.write_all(response.as_bytes());
}

fn bind_callback() -> Result<(TcpListener, u16), ServiceError> {
    for port in CALLBACK_PORTS {
        if let Ok(listener) = TcpListener::bind(("127.0.0.1", port)) {
            return Ok((listener, port));
        }
    }
    Err(upstream("No account-linking callback port is available"))
}

fn connection_from_store(definition: &Value, store: Option<&Value>, fetched_at: i64) -> Value {
    let linking = store.map(|value| &value["accountLinkingData"]);
    let sync = linking.map(|value| &value["accountSyncingData"]);
    let connected = store.is_some();
    let expires_in = linking
        .and_then(|value| value["expiresIn"].as_str())
        .and_then(|value| value.parse::<i64>().ok());
    let expires_at = expires_in
        .filter(|value| *value >= 0)
        .map(|value| fetched_at + value * 1000);
    let sync_state = sync.and_then(|value| value["syncState"].as_str());
    let expired = connected
        && definition["supportsLinking"].as_bool() == Some(true)
        && expires_at.is_some_and(|value| value <= fetched_at);
    let sync_error = connected
        && definition["supportsSync"].as_bool() == Some(true)
        && sync_state.is_some_and(|value| value != "SYNC_SUCCESS");
    json!({
        "provider":definition["provider"],"label":definition["label"],"sortOrder":definition["sortOrder"],
        "supportsLinking":definition["supportsLinking"],"supportsSync":definition["supportsSync"],"isRequired":definition["isRequired"],
        "isConnected":connected,"status":if !connected { "not_connected" } else if expired { "expired" } else if sync_error { "sync_error" } else { "connected" },
        "displayName":linking.and_then(|value| value["userDisplayName"].as_str()),
        "userIdentifier":linking.and_then(|value| value["userIdentifier"].as_str()),
        "expiresIn":linking.and_then(|value| value["expiresIn"].as_str()),"expiresAt":expires_at,
        "syncState":sync_state,"syncDate":sync.and_then(|value| value["syncDate"].as_str()),
        "syncedGames":sync.and_then(|value| value["totalNumberOfSyncedGfnGames"].as_i64()).unwrap_or_default()
    })
}

fn provider_definitions() -> Vec<Value> {
    vec![
        json!({"provider":"UPLAY","label":"Ubisoft","sortOrder":100,"supportsLinking":true,"supportsSync":true,"isRequired":true}),
        json!({"provider":"BATTLENET","label":"Battle.net","sortOrder":101,"supportsLinking":true,"supportsSync":true,"isRequired":true}),
        json!({"provider":"EPIC","label":"Epic Games","sortOrder":104,"supportsLinking":true,"supportsSync":false,"isRequired":true}),
        json!({"provider":"GAIJIN","label":"Gaijin.net","sortOrder":105,"supportsLinking":true,"supportsSync":true,"isRequired":true}),
        json!({"provider":"STEAM","label":"Steam","sortOrder":108,"supportsLinking":false,"supportsSync":true,"isRequired":false}),
        json!({"provider":"XBOX","label":"Xbox","sortOrder":120,"supportsLinking":true,"supportsSync":true,"isRequired":true}),
    ]
}

fn ensure_provider_feature(provider: &str, feature: &str) -> Result<(), ServiceError> {
    let supported = provider_definitions().iter().any(|definition| {
        definition["provider"] == provider && definition[feature].as_bool() == Some(true)
    });
    if supported {
        Ok(())
    } else {
        Err(invalid(
            "This game-account provider does not support that action",
        ))
    }
}

fn required_provider(params: &Value) -> Result<String, ServiceError> {
    let provider = params["provider"]
        .as_str()
        .map(normalize_provider)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid("A game-account provider is required"))?;
    if provider_definitions()
        .iter()
        .any(|definition| definition["provider"] == provider)
    {
        Ok(provider)
    } else {
        Err(invalid("Unsupported game-account provider"))
    }
}

fn normalize_provider(value: &str) -> String {
    match value
        .trim()
        .to_ascii_uppercase()
        .replace([' ', '-'], "_")
        .as_str()
    {
        "UBISOFT" | "UBISOFT_CONNECT" => "UPLAY".to_owned(),
        "BATTLE_NET" | "BLIZZARD" => "BATTLENET".to_owned(),
        "EPIC_GAMES" | "EPIC_GAMES_STORE" => "EPIC".to_owned(),
        value => value.to_owned(),
    }
}

fn graphql_headers(token: &str) -> Result<HeaderMap, ServiceError> {
    let mut headers = base_headers()?;
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/graphql"),
    );
    headers.insert(
        ORIGIN,
        HeaderValue::from_static("https://play.geforcenow.com"),
    );
    headers.insert(
        REFERER,
        HeaderValue::from_static("https://play.geforcenow.com/"),
    );
    headers.insert("nv-browser-type", HeaderValue::from_static("CHROME"));
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("GFNJWT {token}"))
            .map_err(|_| invalid("Invalid authentication token"))?,
    );
    Ok(headers)
}

fn als_headers(token: &str, json_body: bool) -> Result<HeaderMap, ServiceError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/json, text/plain, */*"),
    );
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| invalid("Invalid authentication token"))?,
    );
    headers.insert(
        ORIGIN,
        HeaderValue::from_static("https://play.geforcenow.com"),
    );
    headers.insert(
        REFERER,
        HeaderValue::from_static("https://play.geforcenow.com/"),
    );
    headers.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_VALUE));
    if json_body {
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    }
    Ok(headers)
}

fn base_headers() -> Result<HeaderMap, ServiceError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/json, text/plain, */*"),
    );
    headers.insert("nv-client-id", HeaderValue::from_static(LCARS_CLIENT_ID));
    headers.insert("nv-client-type", HeaderValue::from_static("NATIVE"));
    headers.insert(
        "nv-client-version",
        HeaderValue::from_static(CLIENT_VERSION),
    );
    headers.insert(
        "nv-client-streamer",
        HeaderValue::from_static("NVIDIA-CLASSIC"),
    );
    headers.insert(
        "nv-device-os",
        HeaderValue::from_static(if cfg!(target_os = "windows") {
            "WINDOWS"
        } else if cfg!(target_os = "macos") {
            "MACOS"
        } else {
            "LINUX"
        }),
    );
    headers.insert("nv-device-type", HeaderValue::from_static("DESKTOP"));
    headers.insert("nv-device-make", HeaderValue::from_static("GENERIC"));
    headers.insert("nv-device-model", HeaderValue::from_static("PC"));
    headers.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_VALUE));
    Ok(headers)
}

fn session_token(auth: &AuthSession) -> Result<&str, ServiceError> {
    auth.tokens
        .id_token
        .as_deref()
        .or(Some(auth.tokens.access_token.as_str()))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ServiceError {
            code: "authentication_required",
            message: "No authenticated token is available".to_owned(),
        })
}

fn hex_sha256(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn random_id() -> String {
    let mut bytes = [0_u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn unix_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
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
fn response_error(context: &str, response: reqwest::blocking::Response) -> ServiceError {
    let status = response.status();
    ServiceError {
        code: if matches!(status.as_u16(), 401 | 403) {
            "authentication_required"
        } else {
            "upstream_error"
        },
        message: format!("{context} ({status})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_aliases_are_stable() {
        assert_eq!(normalize_provider("Ubisoft Connect"), "UPLAY");
        assert_eq!(normalize_provider("battle-net"), "BATTLENET");
    }
    #[test]
    fn unsupported_actions_are_rejected() {
        assert!(ensure_provider_feature("STEAM", "supportsLinking").is_err());
        assert!(ensure_provider_feature("STEAM", "supportsSync").is_ok());
    }
}
