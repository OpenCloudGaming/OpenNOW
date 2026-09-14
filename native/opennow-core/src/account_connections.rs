use crate::gfn::{AuthSession, ServiceError};
use rand::RngCore as _;
use reqwest::blocking::Client;
use reqwest::header::{
    ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue, ORIGIN, REFERER, USER_AGENT,
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use url::Url;

const USER_ACCOUNT_QUERY: &str = "{ userAccount { subscriptions { id } storesData { store accountLinkingData { userDisplayName expiresIn userIdentifier accountSyncingData { totalNumberOfSyncedGfnGames syncState syncDate } } } } }";
const CALLBACK_PORTS: [u16; 5] = [2259, 6460, 7119, 8870, 9096];
const LCARS_CLIENT_ID: &str = "ec7e38d4-03af-4b58-b131-cfb0495903ab";
const CLIENT_VERSION: &str = "2.0.87.131";
const USER_AGENT_VALUE: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/128.0.0.0 Safari/537.36 GFN-PC/2.0.87.131";

struct LinkAttempt {
    provider: String,
    scope: String,
    receiver: mpsc::Receiver<Result<Value, String>>,
    expires: Instant,
    cancelled: Arc<AtomicBool>,
    callback: Option<Value>,
    next_check: Instant,
}

impl Drop for LinkAttempt {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

pub struct AccountConnectionsService {
    attempts: Mutex<HashMap<String, LinkAttempt>>,
    syncs: Mutex<HashMap<String, SyncOperation>>,
}

pub struct AccountContext<'a> {
    pub client: &'a Client,
    pub auth: &'a AuthSession,
    pub generation: u64,
    pub graphql: &'a str,
    pub als: &'a str,
    pub definitions: &'a Value,
    pub requests: &'a crate::store_requests::StoreRequests,
    pub check: &'a dyn Fn() -> Result<(), ServiceError>,
}

impl AccountContext<'_> {
    fn scope(&self) -> String {
        json!([
            self.auth.user.user_id,
            self.auth.provider.idp_id,
            self.generation
        ])
        .to_string()
    }

    fn providers(&self) -> Vec<Value> {
        if let Some(items) = self.definitions["stores"]["items"].as_array() {
            return items
                .iter()
                .filter_map(|item| {
                    serde_json::from_value::<crate::catalog_types::StoreDefinition>(item.clone())
                        .ok()
                })
                .map(|definition| {
                    let mut value = definition.connection_definition();
                    value["capabilitySource"] = self.definitions["stores"]["status"].clone();
                    value
                })
                .collect();
        }
        provider_definitions()
            .into_iter()
            .map(|mut definition| {
                definition["capabilitySource"] = json!("fallback");
                definition["supportsLinking"] = json!(false);
                definition["supportsSync"] = json!(false);
                definition
            })
            .collect()
    }
}

#[derive(Clone)]
struct SyncOperation {
    id: String,
    provider: String,
    scope: String,
    baseline: Value,
    phase: &'static str,
    started: Instant,
    next_poll: Instant,
    message: String,
    invalidated: bool,
}

impl SyncOperation {
    fn result(&self) -> Value {
        json!({"operationId":self.id,"provider":self.provider,"phase":self.phase,
            "retryAfterMs":self.next_poll.saturating_duration_since(Instant::now()).as_millis() as u64,
            "message":self.message})
    }
}

impl AccountConnectionsService {
    pub fn cancel_pending(&self) {
        self.attempts
            .lock()
            .expect("account-link state poisoned")
            .clear();
        self.syncs
            .lock()
            .expect("account-sync state poisoned")
            .clear();
    }
    pub fn new() -> Self {
        Self {
            attempts: Mutex::new(HashMap::new()),
            syncs: Mutex::new(HashMap::new()),
        }
    }

    pub fn list(&self, context: &AccountContext<'_>) -> Result<Value, ServiceError> {
        (context.check)()?;
        let token = session_token(context.auth)?;
        let response = context.requests.send(
            context
                .client
                .post(context.graphql)
                .headers(graphql_headers(token)?)
                .json(&json!({"query":USER_ACCOUNT_QUERY})),
            "Game-account discovery failed",
        )?;
        let payload = crate::gfn::catalog::catalog_payload(response, Some(token))?;
        (context.check)()?;
        let account = &payload["data"]["userAccount"];
        let stores = account["storesData"]
            .as_array()
            .filter(|items| items.len() <= 128)
            .ok_or_else(crate::catalog_types::invalid_metadata)?;
        let subscriptions = account["subscriptions"]
            .as_array()
            .filter(|items| items.len() <= 128)
            .ok_or_else(crate::catalog_types::invalid_metadata)?;
        let fetched_at = unix_millis();
        let accounts = context
            .providers()
            .iter()
            .map(|definition| {
                let provider = definition["provider"].as_str().unwrap_or_default();
                let store = stores.iter().find(|store| {
                    normalize_provider(store["store"].as_str().unwrap_or_default()) == provider
                });
                connection_from_store(definition, store, fetched_at)
            })
            .collect::<Vec<_>>();
        crate::store_catalog_page::bounded_result(
            json!({"accounts":accounts,"subscriptions":subscriptions,"definitions":context.definitions,"fetchedAt":fetched_at}),
        )
    }

    pub fn sync(
        &self,
        params: &Value,
        context: &AccountContext<'_>,
    ) -> Result<Value, ServiceError> {
        let provider = required_provider(params)?;
        ensure_provider_feature(&context.providers(), &provider, "supportsSync")?;
        let id = random_id();
        {
            let mut operations = self.syncs.lock().expect("account-sync state poisoned");
            operations.retain(|_, operation| {
                operation.scope == context.scope()
                    && operation.started.elapsed() < Duration::from_secs(180)
            });
            if let Some(operation) = operations.values().find(|operation| {
                operation.provider == provider
                    && matches!(
                        operation.phase,
                        "starting" | "waiting_remote" | "refreshing_library"
                    )
            }) {
                return Ok(operation.result());
            }
            if operations.len() >= 8 {
                return Err(invalid("Too many account-sync operations"));
            }
            operations.insert(
                id.clone(),
                SyncOperation {
                    id: id.clone(),
                    provider: provider.clone(),
                    scope: context.scope(),
                    baseline: Value::Null,
                    phase: "starting",
                    started: Instant::now(),
                    next_poll: Instant::now(),
                    message: String::new(),
                    invalidated: false,
                },
            );
        }
        let outcome = (|| {
            let baseline = self.list(context)?;
            let account = baseline["accounts"]
                .as_array()
                .and_then(|items| items.iter().find(|account| account["provider"] == provider))
                .cloned()
                .ok_or_else(|| invalid("Game-account provider is unavailable"))?;
            (context.check)()?;
            let response = context.requests.send(
                context
                    .client
                    .post(format!("{}/sync/{provider}", context.als))
                    .headers(als_headers(session_token(context.auth)?, true)?)
                    .json(&json!({})),
                "Game-account sync failed",
            )?;
            if response.status().as_u16() != 202 {
                return Err(response_error("Game-account sync failed", response));
            }
            (context.check)()?;
            Ok(account)
        })();
        let mut operations = self.syncs.lock().expect("account-sync state poisoned");
        let operation = operations
            .get_mut(&id)
            .ok_or_else(|| invalid("Account-sync observation was cancelled"))?;
        match outcome {
            Ok(baseline) => {
                operation.baseline = baseline;
                operation.phase = "waiting_remote";
                operation.next_poll = Instant::now() + Duration::from_secs(2);
            }
            Err(error) => {
                operation.phase = "failed";
                operation.message =
                    "Could not confirm sync acceptance. Refresh before retrying.".into();
                return Err(error);
            }
        }
        Ok(operation.result())
    }

    pub fn sync_status(
        &self,
        params: &Value,
        context: &AccountContext<'_>,
    ) -> Result<Value, ServiceError> {
        let id = params["operationId"]
            .as_str()
            .ok_or_else(|| invalid("operationId is required"))?;
        let mut operation = {
            let mut operations = self.syncs.lock().expect("account-sync state poisoned");
            let operation = operations
                .get_mut(id)
                .ok_or_else(|| invalid("Account-sync observation is no longer active"))?;
            if operation.scope != context.scope() {
                return Err(ServiceError {
                    code: "stale_account",
                    message: "The sync belongs to a previous account context".into(),
                });
            }
            if params["cancelObservation"] == true {
                operation.phase = "cancelled_observation";
            }
            if params["libraryRefreshed"] == true && operation.phase == "refreshing_library" {
                operation.phase = "complete";
            }
            if operation.phase == "waiting_remote"
                && operation.started.elapsed() >= Duration::from_secs(120)
            {
                operation.phase = "timed_out";
                operation.message =
                    "Sync completion could not be confirmed. The remote sync may still finish."
                        .into();
            }
            if operation.phase != "waiting_remote" || Instant::now() < operation.next_poll {
                return Ok(operation.result());
            }
            operation.next_poll = Instant::now()
                + Duration::from_secs(if operation.started.elapsed() < Duration::from_secs(20) {
                    2
                } else {
                    5
                });
            operation.clone()
        };
        let account = match self.list(context) {
            Ok(account) => account,
            Err(error) => {
                if error.code == "http_unauthorized"
                    && let Some(operation) = self
                        .syncs
                        .lock()
                        .expect("account-sync state poisoned")
                        .get_mut(id)
                {
                    operation.next_poll = Instant::now();
                }
                return Err(error);
            }
        };
        let current = account["accounts"].as_array().and_then(|items| {
            items
                .iter()
                .find(|item| item["provider"] == operation.provider)
        });
        operation.phase = observed_sync_phase(&operation.baseline, current);
        (context.check)()?;
        let mut operations = self.syncs.lock().expect("account-sync state poisoned");
        let current = operations
            .get_mut(id)
            .ok_or_else(|| invalid("Account-sync observation was cancelled"))?;
        if current.phase == "waiting_remote" {
            current.phase = operation.phase;
            if operation.phase == "failed" {
                current.message =
                    "The store reported a sync or connection failure. Check the account status."
                        .into();
            }
        }
        Ok(current.result())
    }

    pub fn invalidate_sync(
        &self,
        id: &str,
        invalidate: impl FnOnce() -> Result<(), ServiceError>,
    ) -> Result<bool, ServiceError> {
        let mut operations = self.syncs.lock().expect("account-sync state poisoned");
        let Some(operation) = operations.get_mut(id) else {
            return Ok(false);
        };
        if operation.invalidated {
            return Ok(false);
        }
        invalidate()?;
        operation.invalidated = true;
        Ok(true)
    }

    pub fn unlink(
        &self,
        params: &Value,
        context: &AccountContext<'_>,
    ) -> Result<Value, ServiceError> {
        let provider = required_provider(params)?;
        ensure_provider_feature(&context.providers(), &provider, "supportsLinking")?;
        let url = format!("{}/linking/{provider}", context.als);
        (context.check)()?;
        let response = context
            .client
            .delete(url)
            .headers(als_headers(session_token(context.auth)?, false)?)
            .send()
            .map_err(|error| network("Game-account unlink failed", error))?;
        if !response.status().is_success() && response.status().as_u16() != 404 {
            return Err(response_error("Game-account unlink failed", response));
        }
        let account = self.list(context)?;
        let disconnected = account["accounts"].as_array().is_some_and(|items| {
            items
                .iter()
                .any(|item| item["provider"] == provider && item["isConnected"] == false)
        });
        Ok(
            json!({"ok":disconnected,"provider":provider,"phase":if disconnected {"complete"} else {"unconfirmed"},
            "message":if disconnected {"Account disconnected"} else {"Disconnect accepted; account update could not yet be confirmed"}}),
        )
    }

    pub fn start_link(
        &self,
        params: &Value,
        context: &AccountContext<'_>,
    ) -> Result<Value, ServiceError> {
        let provider = required_provider(params)?;
        ensure_provider_feature(&context.providers(), &provider, "supportsLinking")?;
        let (listener, port) = bind_callback()?;
        let redirect_uri = format!("http://localhost:{port}/");
        let mut url =
            Url::parse(&format!("{}/login_url", context.als)).expect("constant ALS URL is valid");
        url.query_pairs_mut()
            .append_pair("platform", &provider)
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("client_id", "gfn-pc");
        (context.check)()?;
        let response = context
            .client
            .get(url)
            .headers(als_headers(session_token(context.auth)?, false)?)
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
        crate::requests::check()?;
        (context.check)()?;
        if self
            .attempts
            .lock()
            .expect("account-link state poisoned")
            .len()
            >= 8
        {
            return Err(invalid("Too many account-link attempts"));
        }
        let attempt_id = random_id();
        let (sender, receiver) = mpsc::channel();
        let worker_provider = provider.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        thread::Builder::new()
            .name(format!("opennow-account-link-{attempt_id}"))
            .spawn(move || {
                let result = wait_for_callback(listener, &worker_provider, &worker_cancelled);
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
                    scope: context.scope(),
                    receiver,
                    expires: Instant::now() + Duration::from_secs(300),
                    cancelled,
                    callback: None,
                    next_check: Instant::now(),
                },
            );
        Ok(
            json!({"attemptId":attempt_id,"provider":provider,"loginUrl":login_url,"expiresInSeconds":300}),
        )
    }

    pub fn poll_link(
        &self,
        params: &Value,
        context: &AccountContext<'_>,
    ) -> Result<Value, ServiceError> {
        let attempt_id = params["attemptId"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| invalid("account.connections.link.poll requires attemptId"))?;
        let mut attempts = self.attempts.lock().expect("account-link state poisoned");
        let attempt = attempts.get_mut(attempt_id).ok_or_else(|| ServiceError {
            code: "link_attempt_not_found",
            message: "Account-link attempt is no longer active".to_owned(),
        })?;
        if attempt.scope != context.scope() {
            return Err(ServiceError {
                code: "stale_account",
                message: "The link belongs to a previous account context".into(),
            });
        }
        if Instant::now() >= attempt.expires {
            attempts.remove(attempt_id);
            return Ok(json!({"status":"expired"}));
        }
        if attempt.callback.is_none() {
            match attempt.receiver.try_recv() {
                Ok(Ok(result)) => attempt.callback = Some(result),
                Ok(Err(message)) => {
                    attempts.remove(attempt_id);
                    return Ok(json!({"status":"error","message":message}));
                }
                Err(mpsc::TryRecvError::Empty) => return Ok(json!({"status":"pending"})),
                Err(mpsc::TryRecvError::Disconnected) => {
                    attempts.remove(attempt_id);
                    return Ok(
                        json!({"status":"error","message":"Account-link callback stopped unexpectedly"}),
                    );
                }
            }
        }
        if Instant::now() < attempt.next_check {
            return Ok(json!({"status":"pending","phase":"verifying_account"}));
        }
        attempt.next_check = Instant::now() + Duration::from_secs(2);
        let provider = attempt.provider.clone();
        drop(attempts);
        let account = match self.list(context) {
            Ok(account) => account,
            Err(error) => {
                if error.code == "http_unauthorized"
                    && let Some(attempt) = self
                        .attempts
                        .lock()
                        .expect("account-link state poisoned")
                        .get_mut(attempt_id)
                {
                    attempt.next_check = Instant::now();
                }
                return Err(error);
            }
        };
        let connected = account["accounts"].as_array().is_some_and(|items| {
            items.iter().any(|item| {
                item["provider"] == provider
                    && item["isConnected"] == true
                    && item["status"] != "expired"
            })
        });
        if connected {
            self.attempts
                .lock()
                .expect("account-link state poisoned")
                .remove(attempt_id);
        }
        Ok(
            json!({"status":if connected {"complete"} else {"pending"},"phase":"verifying_account","provider":provider,"account":account}),
        )
    }
}

fn wait_for_callback(
    listener: TcpListener,
    provider: &str,
    cancelled: &AtomicBool,
) -> Result<Value, String> {
    listener
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(300);
    while Instant::now() < deadline && !cancelled.load(Ordering::Acquire) {
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
    let connected = linking.is_some_and(|value| value.is_object());
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
        "capabilitySource":definition["capabilitySource"],"features":definition["features"],"accountLinkingMetadata":definition["accountLinkingMetadata"],
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

fn ensure_provider_feature(
    definitions: &[Value],
    provider: &str,
    feature: &str,
) -> Result<(), ServiceError> {
    let supported = definitions.iter().any(|definition| {
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
    crate::catalog_types::normalize_store(value)
}

fn observed_sync_phase(baseline: &Value, current: Option<&Value>) -> &'static str {
    let Some(current) = current else {
        return "failed";
    };
    if current["isConnected"] != true || current["status"] == "expired" {
        return "failed";
    }
    let state = current["syncState"].as_str().unwrap_or("");
    let date = current["syncDate"].as_str();
    let changed_date = date
        .and_then(|date| chrono::DateTime::parse_from_rfc3339(date).ok())
        .is_some_and(|date| {
            baseline["syncDate"]
                .as_str()
                .and_then(|baseline| chrono::DateTime::parse_from_rfc3339(baseline).ok())
                .is_none_or(|baseline| date > baseline)
        });
    if state == "SYNC_SUCCESS" && changed_date {
        return "refreshing_library";
    }
    if !state.is_empty()
        && state != "SYNC_SUCCESS"
        && (changed_date || current["syncState"] != baseline["syncState"])
    {
        return "failed";
    }
    "waiting_remote"
}

fn graphql_headers(token: &str) -> Result<HeaderMap, ServiceError> {
    let mut headers = base_headers()?;
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
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
        code: if status.as_u16() == 401 {
            "http_unauthorized"
        } else if status.as_u16() == 403 {
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
        assert!(
            ensure_provider_feature(&provider_definitions(), "STEAM", "supportsLinking").is_err()
        );
        assert!(ensure_provider_feature(&provider_definitions(), "STEAM", "supportsSync").is_ok());
    }

    fn account(date: &str, state: &str) -> Value {
        json!({"data":{"userAccount":{"subscriptions":[{"id":"store-subscription"}],"storesData":[{
            "store":"STEAM","accountLinkingData":{"userDisplayName":"Fixture","accountSyncingData":{
                "syncDate":date,"syncState":state,"totalNumberOfSyncedGfnGames":5}}}]}}})
    }

    #[test]
    fn account_discovery_posts_a_json_graphql_document() {
        let (url, worker) = crate::gfn::tests::mock_requests(
            vec![(200, account("2026-09-14T00:00:00Z", "SYNC_SUCCESS"))],
            |_, request| {
                assert!(request.starts_with("POST / HTTP/1.1\r\n"));
                let (headers, body) = request.split_once("\r\n\r\n").unwrap();
                assert!(
                    headers
                        .to_ascii_lowercase()
                        .contains("\r\ncontent-type: application/json\r\n")
                );
                assert_eq!(
                    serde_json::from_str::<Value>(body).unwrap(),
                    json!({"query":USER_ACCOUNT_QUERY})
                );
            },
        );
        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap();
        let auth = crate::gfn::tests::auth_fixture("account-a");
        let definitions = json!({"stores":{"status":"success","items":[{"store":"STEAM","label":"Steam",
            "features":[{"__typename":"AccountGamesSyncing","supported":true}]}]}});
        let requests = crate::store_requests::StoreRequests::default();
        let context = AccountContext {
            client: &client,
            auth: &auth,
            generation: 1,
            graphql: &url,
            als: &url,
            definitions: &definitions,
            requests: &requests,
            check: &|| Ok(()),
        };
        let result = AccountConnectionsService::new().list(&context).unwrap();
        assert_eq!(
            result["subscriptions"],
            json!([{"id":"store-subscription"}])
        );
        worker.join().unwrap();
    }

    #[test]
    fn sync_acceptance_requires_new_remote_success_then_library_acknowledgement() {
        let old = "2026-09-14T00:00:00Z";
        let fresh = "2026-09-14T01:00:00Z";
        let (url, worker) = crate::gfn::tests::mock_requests(
            vec![
                (200, account(old, "SYNC_SUCCESS")),
                (202, json!({})),
                (200, account(old, "SYNC_SUCCESS")),
                (200, account(fresh, "SYNC_SUCCESS")),
            ],
            |index, request| {
                if index == 1 {
                    assert!(request.starts_with("POST /sync/STEAM "));
                } else {
                    assert!(request.contains("userAccount"));
                }
            },
        );
        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap();
        let auth = crate::gfn::tests::auth_fixture("account-a");
        let definitions = json!({"stores":{"status":"success","items":[{"store":"STEAM","label":"Steam",
            "features":[{"__typename":"AccountGamesSyncing","supported":true}]}]}});
        let requests = crate::store_requests::StoreRequests::default();
        let context = AccountContext {
            client: &client,
            auth: &auth,
            generation: 4,
            graphql: &url,
            als: &url,
            definitions: &definitions,
            requests: &requests,
            check: &|| Ok(()),
        };
        let service = AccountConnectionsService::new();
        let started = service
            .sync(&json!({"provider":"STEAM"}), &context)
            .unwrap();
        assert_eq!(started["phase"], "waiting_remote");
        let id = started["operationId"].as_str().unwrap();
        assert_eq!(
            service
                .sync(&json!({"provider":"STEAM"}), &context)
                .unwrap()["operationId"],
            id
        );
        let params = json!({"operationId":id});
        assert_eq!(
            service.sync_status(&params, &context).unwrap()["phase"],
            "waiting_remote"
        );
        for phase in ["waiting_remote", "refreshing_library"] {
            service.syncs.lock().unwrap().get_mut(id).unwrap().next_poll = Instant::now();
            assert_eq!(
                service.sync_status(&params, &context).unwrap()["phase"],
                phase
            );
        }
        assert!(service.invalidate_sync(id, || Ok(())).unwrap());
        assert!(
            !service
                .invalidate_sync(id, || panic!("duplicate invalidation"))
                .unwrap()
        );
        assert_eq!(
            service
                .sync_status(&json!({"operationId":id,"libraryRefreshed":true}), &context)
                .unwrap()["phase"],
            "complete"
        );
        worker.join().unwrap();
    }

    #[test]
    fn sync_failure_baselines_unknown_dates_and_disconnect_are_distinct() {
        let baseline = json!({"isConnected":true,"status":"connected","syncDate":"2026-09-14T00:00:00Z","syncState":"SYNC_SUCCESS"});
        for (change, expected) in [
            (json!({"syncDate":"not-a-date"}), "waiting_remote"),
            (json!({"syncState":"SYNC_DENIED"}), "failed"),
            (json!({"syncState":"NEW_UNKNOWN_FAILURE"}), "failed"),
            (json!({"status":"expired"}), "failed"),
            (json!({"isConnected":false}), "failed"),
            (json!({"syncedGames":900}), "waiting_remote"),
        ] {
            let mut current = baseline.clone();
            for (key, value) in change.as_object().unwrap() {
                current[key] = value.clone();
            }
            assert_eq!(observed_sync_phase(&baseline, Some(&current)), expected);
        }
        let failed =
            json!({"isConnected":true,"syncState":"SYNC_DENIED","syncDate":"2026-09-14T00:00:00Z"});
        assert_eq!(
            observed_sync_phase(&failed, Some(&failed)),
            "waiting_remote"
        );
        assert_eq!(observed_sync_phase(&baseline, None), "failed");
    }

    #[test]
    fn server_false_and_unknown_stores_never_inherit_fallback_actions() {
        for (store, supported, expected) in [
            ("STEAM", false, false),
            ("STEAM", true, true),
            ("NEW_STORE", true, false),
        ] {
            let definition: crate::catalog_types::StoreDefinition = serde_json::from_value(json!({
                "store":store,"label":"Fixture","features":[{"__typename":"AccountGamesSyncing","supported":supported}]
            })).unwrap();
            assert_eq!(definition.connection_definition()["supportsSync"], expected);
        }
    }

    #[test]
    fn observation_timeout_cancel_and_scope_rejection_do_not_send_network_requests() {
        let client = Client::builder().no_proxy().build().unwrap();
        let auth = crate::gfn::tests::auth_fixture("account-a");
        let definitions = json!({});
        let requests = crate::store_requests::StoreRequests::default();
        let mut context = AccountContext {
            client: &client,
            auth: &auth,
            generation: 4,
            graphql: "http://127.0.0.1:1",
            als: "http://127.0.0.1:1",
            definitions: &definitions,
            requests: &requests,
            check: &|| Ok(()),
        };
        let service = AccountConnectionsService::new();
        service.syncs.lock().unwrap().insert(
            "fixture".into(),
            SyncOperation {
                id: "fixture".into(),
                provider: "STEAM".into(),
                scope: context.scope(),
                baseline: json!({}),
                phase: "waiting_remote",
                started: Instant::now() - Duration::from_secs(121),
                next_poll: Instant::now(),
                message: String::new(),
                invalidated: false,
            },
        );
        let params = json!({"operationId":"fixture"});
        assert_eq!(
            service.sync_status(&params, &context).unwrap()["phase"],
            "timed_out"
        );
        assert_eq!(
            service
                .sync_status(
                    &json!({"operationId":"fixture","cancelObservation":true}),
                    &context
                )
                .unwrap()["phase"],
            "cancelled_observation"
        );
        context.generation += 1;
        assert_eq!(
            service.sync_status(&params, &context).unwrap_err().code,
            "stale_account"
        );
        service.cancel_pending();
        assert!(service.sync_status(&params, &context).is_err());
    }

    #[test]
    fn link_callback_survives_unauthorized_and_waits_for_authoritative_connection() {
        let (url, worker) = crate::gfn::tests::mock_requests(
            vec![
                (401, json!({})),
                (
                    200,
                    json!({"data":{"userAccount":{"subscriptions":[],"storesData":[]}}}),
                ),
                (200, account("2026-09-14T01:00:00Z", "SYNC_SUCCESS")),
            ],
            |_, request| assert!(request.contains("userAccount")),
        );
        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap();
        let auth = crate::gfn::tests::auth_fixture("account-a");
        let definitions = json!({"stores":{"status":"success","items":[{"store":"STEAM","label":"Steam","features":[]}]}});
        let requests = crate::store_requests::StoreRequests::default();
        let context = AccountContext {
            client: &client,
            auth: &auth,
            generation: 4,
            graphql: &url,
            als: &url,
            definitions: &definitions,
            requests: &requests,
            check: &|| Ok(()),
        };
        let service = AccountConnectionsService::new();
        let (sender, receiver) = mpsc::channel();
        sender.send(Ok(json!({"platform":"STEAM"}))).unwrap();
        drop(sender);
        service.attempts.lock().unwrap().insert(
            "fixture".into(),
            LinkAttempt {
                provider: "STEAM".into(),
                scope: context.scope(),
                receiver,
                expires: Instant::now() + Duration::from_secs(300),
                cancelled: Arc::new(AtomicBool::new(false)),
                callback: None,
                next_check: Instant::now(),
            },
        );
        let params = json!({"attemptId":"fixture"});
        assert_eq!(
            service.poll_link(&params, &context).unwrap_err().code,
            "http_unauthorized"
        );
        let pending = service.poll_link(&params, &context).unwrap();
        assert_eq!(pending["status"], "pending");
        assert_eq!(pending["phase"], "verifying_account");
        assert_eq!(
            service.poll_link(&params, &context).unwrap()["status"],
            "pending"
        );
        service
            .attempts
            .lock()
            .unwrap()
            .get_mut("fixture")
            .unwrap()
            .next_check = Instant::now();
        assert_eq!(
            service.poll_link(&params, &context).unwrap()["status"],
            "complete"
        );
        assert!(service.attempts.lock().unwrap().is_empty());
        worker.join().unwrap();
    }
}
