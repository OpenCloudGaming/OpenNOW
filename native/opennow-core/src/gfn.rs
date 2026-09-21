use crate::account_connections::AccountConnectionsService;
use crate::cloudmatch::CloudMatchService;
use crate::console_profiles::ConsoleProfiles;
use crate::credential_vault::CredentialVault;
use crate::persistent_storage::PersistentStorageService;
use crate::proxy::{client_for_settings, config_from_settings};
use base64::Engine as _;
use qrcode::QrCode;
use reqwest::blocking::{Client, Response};
use reqwest::header::{
    ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue, ORIGIN, REFERER, USER_AGENT,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::env;
use std::io::Read;
use std::net::ToSocketAddrs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub(crate) mod catalog;
mod catalog_actions;
mod store_launch;
use catalog::*;

#[cfg(test)]
mod catalog_tests;
#[cfg(test)]
mod routing_tests;
#[cfg(test)]
mod store_launch_tests;

const DEFAULT_IDP_ID: &str = "PDiAhv2kJTFeQ7WOPqiQ2tRZ7lGhR2X11dXvM4TZSxg";
const DEFAULT_STREAMING_URL: &str = "https://prod.cloudmatchbeta.nvidiagrid.net/";
// Alliance `prod.*` discovery endpoints are geo-steered: they resolve to the
// nearest regional PoP from inside the partner footprint (via VPN or local
// presence) and return NODATA/NXDOMAIN from outside it. For partners below,
// discovery still advertises only the `prod.*` name, so out-of-footprint
// users get no DNS at all. Each entry maps that stale name to a globally
// reachable regional endpoint in the same `nvidiagrid.net` trust policy. The
// fallback engages only when the advertised host fails DNS resolution, so
// in-footprint users keep native geo-steering untouched.
struct ProviderFallback {
    idp_id: &'static str,
    stale_host: &'static str,
    fallback_url: &'static str,
}

const PROVIDER_FALLBACKS: &[ProviderFallback] = &[
    // Verified live Sep 2026: serverId NPA-DIG-SCL-01, region "LATAM West",
    // session create and first video frame at 1080p60 H264. Digevo also
    // serves LATAM North (Bogota) via geo-steered `prod.dig`.
    ProviderFallback {
        idp_id: "IsvVBA3Aj8KZ7gwwuRUhB6-tOF2o2F1wncD-XjYv100",
        stale_host: "prod.dig.geforcenow.nvidiagrid.net",
        fallback_url: "https://latam-west.dig.geforcenow.nvidiagrid.net/",
    },
];
const STEAM_DECK_CLIENT_ID: &str = "q61ddeJrVt7O90Nl-P-N7I36yctih4Ml6FyXLrb6j-U";
const SCOPES: &str = "openid consent email tk_client age";
const STEAM_DECK_USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64; Steam Deck) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36";
const GFN_USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36 NVIDIACEFClient/HEAD/7b92719716 GFN-PC/2.0.87.131";
const NVIDIA_FILE_ORIGIN: &str = "https://nvfile";
const TOKEN_REFRESH_WINDOW_MS: u64 = 10 * 60 * 1000;
const CLIENT_TOKEN_REFRESH_WINDOW_MS: u64 = 5 * 60 * 1000;
const LCARS_CLIENT_ID: &str = "ec7e38d4-03af-4b58-b131-cfb0495903ab";
const GFN_CLIENT_VERSION: &str = "2.0.87.131";
const GRAPHQL_URL: &str = "https://games.geforce.com/graphql";
const MES_URL: &str = "https://mes.geforcenow.com/v4/subscriptions";

#[derive(Clone)]
pub struct Endpoints {
    pub service_urls: String,
    pub device_authorize: String,
    pub token: String,
    pub client_token: String,
    pub userinfo: String,
    pub revoke: String,
    pub public_catalog: String,
    pub graphql: String,
    pub public_graphql: String,
    pub account_linking: String,
    pub subscription: String,
    #[cfg(test)]
    server_info: Option<String>,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            service_urls: "https://pcs.geforcenow.com/v1/serviceUrls".to_owned(),
            device_authorize: "https://login.nvidia.com/device/authorize".to_owned(),
            token: "https://login.nvidia.com/token".to_owned(),
            client_token: "https://login.nvidia.com/client_token".to_owned(),
            userinfo: "https://login.nvidia.com/userinfo".to_owned(),
            revoke: "https://login.nvidia.com/assets/v2/Tokens?level=client".to_owned(),
            public_catalog:
                "https://static.nvidiagrid.net/supported-public-game-list/locales/gfnpc-en-US.json"
                    .to_owned(),
            graphql: GRAPHQL_URL.into(),
            public_graphql: "https://public.games.geforce.com/graphql".into(),
            account_linking: "https://als.geforcenow.com/v1".into(),
            subscription: MES_URL.into(),
            #[cfg(test)]
            server_info: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginProvider {
    pub idp_id: String,
    pub code: String,
    pub display_name: String,
    pub streaming_service_url: String,
    pub priority: i64,
}

impl LoginProvider {
    fn default_nvidia() -> Self {
        Self {
            idp_id: DEFAULT_IDP_ID.to_owned(),
            code: "NVIDIA".to_owned(),
            display_name: "NVIDIA".to_owned(),
            streaming_service_url: DEFAULT_STREAMING_URL.to_owned(),
            priority: 0,
        }
    }

    fn normalize(mut self) -> Self {
        self.streaming_service_url = effective_provider_url(&self);
        if !self.streaming_service_url.ends_with('/') {
            self.streaming_service_url.push('/');
        }
        self
    }
}

pub(crate) fn effective_provider_url(provider: &LoginProvider) -> String {
    effective_provider_url_with(provider, host_resolves)
}

fn host_resolves(host: &str) -> bool {
    // The port is irrelevant; this only exercises DNS resolution. NXDOMAIN
    // answers fast, and successes are OS-cached, so the probe stays cheap
    // next to the HTTPS calls every caller issues afterwards.
    format!("{host}:443")
        .to_socket_addrs()
        .is_ok_and(|mut addresses| addresses.next().is_some())
}

fn effective_provider_url_with(
    provider: &LoginProvider,
    resolves: impl Fn(&str) -> bool,
) -> String {
    let raw = provider.streaming_service_url.trim();
    let host = url::Url::parse(raw)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_default();
    let fallback = PROVIDER_FALLBACKS.iter().find(|entry| {
        provider.idp_id == entry.idp_id && host.eq_ignore_ascii_case(entry.stale_host)
    });
    match fallback {
        Some(entry) if !resolves(&host) => {
            eprintln!(
                "provider: {} discovery endpoint is unreachable; using fallback {}",
                provider.code, entry.fallback_url
            );
            entry.fallback_url.to_owned()
        }
        _ => raw.to_owned(),
    }
}

pub(crate) fn provider_streaming_base(provider: &LoginProvider) -> Result<url::Url, ServiceError> {
    trusted_streaming_base(&effective_provider_url(provider))
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthTokens {
    pub access_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_token_expires_at: Option<u64>,
    pub expires_at: u64,
    pub auth_client_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_token_expires_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_token_lifetime_ms: Option<u64>,
}

impl std::fmt::Debug for AuthTokens {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("AuthTokens([redacted])")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthUser {
    pub user_id: String,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    pub membership_tier: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthSession {
    pub provider: LoginProvider,
    pub tokens: AuthTokens,
    pub user: AuthUser,
}

#[derive(Serialize)]
struct PublicAuthSession<'a> {
    user: &'a AuthUser,
    provider: &'a LoginProvider,
}

impl AuthSession {
    fn public(&self) -> PublicAuthSession<'_> {
        PublicAuthSession {
            user: &self.user,
            provider: &self.provider,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum TokenPurpose {
    StarfleetAccess,
    ServiceId,
}

impl AuthTokens {
    fn service_token(&self) -> &str {
        self.id_token.as_deref().unwrap_or(&self.access_token)
    }

    fn expiry(&self, purpose: TokenPurpose) -> u64 {
        match purpose {
            TokenPurpose::StarfleetAccess => self.expires_at,
            TokenPurpose::ServiceId => self.id_token.as_deref().map_or(self.expires_at, |token| {
                self.id_token_expires_at
                    .or_else(|| jwt_expiry(token))
                    .unwrap_or(0)
            }),
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq)]
enum PersistenceIntent {
    #[default]
    MemoryOnly,
    SecureStore,
}

#[derive(Clone)]
struct DeviceAttempt {
    provider: LoginProvider,
    device_code: String,
    expires_at: u64,
    deadline: Instant,
    interval_seconds: u64,
    next_poll: Instant,
    in_flight: bool,
    pending_session: Option<AuthSession>,
}

#[derive(Default)]
struct ServiceState {
    providers: Vec<LoginProvider>,
    providers_expires: Option<Instant>,
    providers_retry: Option<Instant>,
    providers_error: Option<String>,
    providers_default: Option<String>,
    attempts: HashMap<String, DeviceAttempt>,
    session: Option<AuthSession>,
    public_games: Vec<Value>,
    public_games_proxy_scope: String,
    restore_attempted: bool,
    persistence_state: String,
    persistence_intent: PersistenceIntent,
    generation: u64,
    login_generation: u64,
    refresh_retry_at: Option<Instant>,
}

#[derive(Clone, Debug)]
pub struct ServiceError {
    pub code: &'static str,
    pub message: String,
}

impl ServiceError {
    pub(crate) fn network(context: &str, error: reqwest::Error) -> Self {
        Self {
            code: "network_error",
            message: format!("{context}: {}", error.without_url()),
        }
    }

    fn response(context: &str, response: Response) -> Self {
        let status = response.status();
        let mut body = Vec::new();
        let payload = response
            .take(16 * 1024 + 1)
            .read_to_end(&mut body)
            .ok()
            .filter(|_| body.len() <= 16 * 1024)
            .and_then(|_| serde_json::from_slice::<Value>(&body).ok());
        let detail = payload
            .as_ref()
            .and_then(|payload| payload["error"].as_str())
            .filter(|_| matches!(status.as_u16(), 400 | 401))
            .filter(|error| matches!(*error, "invalid_grant" | "invalid_token" | "token_revoked"));
        Self {
            code: if status.as_u16() == 401 {
                "http_unauthorized"
            } else {
                "upstream_error"
            },
            message: match detail {
                Some(detail) => format!("{context} ({status}): {detail}"),
                None => format!("{context} ({status})"),
            },
        }
    }

    fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: "invalid_params",
            message: message.into(),
        }
    }
}

#[derive(Default)]
struct SessionRouting {
    active_owner: Option<ActiveSeatOwner>,
    discovery_owner: Option<(String, String, u64)>,
}

struct ActiveSeatOwner {
    auth: AuthSession,
    session_id: String,
    last_published_generation: u64,
    allocation_generation: Option<u64>,
}

impl ActiveSeatOwner {
    fn capture(
        auth: AuthSession,
        generation: u64,
        session: &Value,
        allocation_generation: Option<u64>,
    ) -> Result<Self, ServiceError> {
        Ok(Self {
            auth,
            session_id: session["sessionId"]
                .as_str()
                .filter(|id| !id.is_empty())
                .ok_or_else(session_owner_error)?
                .to_owned(),
            last_published_generation: generation,
            allocation_generation,
        })
    }

    fn matches(&self, auth: &AuthSession, session_id: &str) -> bool {
        self.session_id == session_id
            && self.auth.user.user_id == auth.user.user_id
            && self.auth.provider.idp_id == auth.provider.idp_id
    }
}

pub struct GfnService {
    client: Client,
    endpoints: Endpoints,
    device_id: String,
    device_identity_error: Option<String>,
    vault: CredentialVault,
    profiles: ConsoleProfiles,
    cloudmatch: CloudMatchService,
    account_connections: AccountConnectionsService,
    persistent_storage: PersistentStorageService,
    store_cache: crate::store_cache::StoreCache,
    catalog_revision: std::sync::atomic::AtomicU64,
    catalog_mutations: Mutex<std::collections::HashSet<catalog_actions::CatalogActionKey>>,
    server_vpc_cache: crate::server_vpc_cache::ServerVpcCache,
    auth_operation: Mutex<()>,
    discovery_operation: Mutex<()>,
    session_routing: Mutex<SessionRouting>,
    state: Mutex<ServiceState>,
}

impl GfnService {
    pub fn new(data_dir: PathBuf) -> Result<Self, String> {
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(20))
            .pool_idle_timeout(Duration::from_secs(60))
            .build()
            .map_err(|error| error.to_string())?;
        Ok(Self::with_client(client, Endpoints::default(), data_dir))
    }

    fn with_client(client: Client, endpoints: Endpoints, data_dir: PathBuf) -> Self {
        let identity = crate::device_identity::load(&data_dir, stable_device_id);
        let (device_id, device_identity_error) = match identity {
            Ok(id) => (id, None),
            Err(error) => (stable_device_id(), Some(error)),
        };
        let vault = CredentialVault::new(data_dir.clone());
        match vault.migrate_legacy_electron_sessions() {
            Ok(count) if count > 0 => {
                eprintln!(
                    "auth: migrated {count} Electron account session(s) into the OS credential store"
                )
            }
            Ok(_) => {}
            Err(error) => eprintln!("auth: Electron account migration was deferred: {error}"),
        }
        let store_cache = crate::store_cache::StoreCache::new(data_dir.clone());
        let catalog_revision = store_cache.catalog_revision();
        Self {
            cloudmatch: CloudMatchService::with_cleanup_path(
                client.clone(),
                data_dir.join("pending-session-cleanup.json"),
            ),
            account_connections: AccountConnectionsService::new(),
            persistent_storage: PersistentStorageService::new(client.clone()),
            client,
            endpoints,
            device_id,
            device_identity_error,
            vault,
            profiles: ConsoleProfiles::load(&data_dir),
            store_cache,
            catalog_revision: std::sync::atomic::AtomicU64::new(catalog_revision),
            catalog_mutations: Mutex::new(std::collections::HashSet::new()),
            server_vpc_cache: crate::server_vpc_cache::ServerVpcCache::default(),
            auth_operation: Mutex::new(()),
            discovery_operation: Mutex::new(()),
            session_routing: Mutex::new(SessionRouting::default()),
            state: Mutex::new(ServiceState::default()),
        }
    }

    pub fn providers(&self) -> Result<Value, ServiceError> {
        let _discovery = crate::store_requests::lock(&self.discovery_operation)?;
        {
            let state = self.state.lock().expect("GFN state poisoned");
            if state
                .providers_expires
                .is_some_and(|time| time > Instant::now())
                || state
                    .providers_retry
                    .is_some_and(|time| time > Instant::now())
            {
                return Ok(provider_result(&state));
            }
        }
        let mut retry_delay = Duration::from_secs(30);
        let result = self
            .client
            .get(&self.endpoints.service_urls)
            .timeout(Duration::from_secs(5))
            .header(ACCEPT, "application/json")
            .header(USER_AGENT, GFN_USER_AGENT)
            .send()
            .map_err(|error| ServiceError::network("Provider discovery failed", error))
            .and_then(|response| {
                if !response.status().is_success() {
                    if response.status().as_u16() == 429 {
                        retry_delay =
                            response
                                .headers()
                                .get(reqwest::header::RETRY_AFTER)
                                .and_then(|value| value.to_str().ok())
                                .and_then(|value| {
                                    value.parse::<u64>().ok().map(Duration::from_secs).or_else(
                                        || {
                                            httpdate::parse_http_date(value).ok().and_then(|time| {
                                                time.duration_since(SystemTime::now()).ok()
                                            })
                                        },
                                    )
                                })
                                .unwrap_or(retry_delay)
                                .clamp(Duration::from_secs(30), Duration::from_secs(3600));
                    }
                    return Err(ServiceError::response(
                        "Provider discovery failed",
                        response,
                    ));
                }
                let payload = response
                    .json::<Value>()
                    .map_err(|error| ServiceError::network("Invalid provider discovery", error))?;
                let providers = parse_providers(&payload)
                    .into_iter()
                    .filter(|provider| {
                        trusted_streaming_base(&provider.streaming_service_url).is_ok()
                    })
                    .collect::<Vec<_>>();
                if providers.is_empty() {
                    return Err(ServiceError::invalid(
                        "Provider discovery returned no usable providers",
                    ));
                }
                let default = payload["gfnServiceInfo"]["defaultProvider"]
                    .as_str()
                    .and_then(|default| {
                        payload["gfnServiceInfo"]["gfnServiceEndpoints"]
                            .as_array()?
                            .iter()
                            .find(|entry| {
                                entry["loginProvider"] == default
                                    || entry["loginProviderCode"] == default
                            })
                            .and_then(|entry| entry["idpId"].as_str())
                            .filter(|id| providers.iter().any(|provider| provider.idp_id == *id))
                            .map(ToOwned::to_owned)
                    });
                Ok((providers, default))
            });
        crate::requests::check()?;
        let _operation = self
            .auth_operation
            .lock()
            .expect("GFN auth operation poisoned");
        let mut state = self.state.lock().expect("GFN state poisoned");
        match result {
            Ok((providers, default)) => {
                if let Some(session) = &mut state.session {
                    match providers
                        .iter()
                        .find(|provider| provider.idp_id == session.provider.idp_id)
                    {
                        Some(provider)
                            if session.provider.streaming_service_url
                                != provider.streaming_service_url =>
                        {
                            session.provider = provider.clone();
                            state.generation += 1;
                        }
                        None => state.generation += 1,
                        _ => {}
                    }
                }
                state.providers = providers;
                state.providers_default = default;
                state.providers_expires = Some(Instant::now() + Duration::from_secs(15 * 60));
                state.providers_retry = None;
                state.providers_error = None;
            }
            Err(error) => {
                state.providers_retry = Some(Instant::now() + retry_delay);
                state.providers_error = Some(error.message);
            }
        }
        Ok(provider_result(&state))
    }

    pub fn start_device_login(&self, params: &Value) -> Result<Value, ServiceError> {
        crate::requests::check()?;
        if let Some(message) = &self.device_identity_error {
            return Err(ServiceError {
                code: "device_identity_unavailable",
                message: message.clone(),
            });
        }
        let generation = {
            let mut state = self.state.lock().expect("GFN state poisoned");
            state.login_generation += 1;
            state.attempts.clear();
            state.login_generation
        };
        eprintln!("auth: starting device authorization");
        let discovery = self.providers()?;
        let providers = discovery["providers"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let provider_id = params["providerIdpId"]
            .as_str()
            .or_else(|| discovery["defaultProviderIdpId"].as_str());
        let provider = providers
            .iter()
            .filter_map(|value| serde_json::from_value::<LoginProvider>(value.clone()).ok())
            .find(|item| provider_id.is_some_and(|wanted| item.idp_id == wanted))
            .or_else(|| provider_id.is_none().then(|| {
                providers
                    .first()
                    .and_then(|value| serde_json::from_value(value.clone()).ok())
            }).flatten())
            .ok_or_else(|| ServiceError { code: "provider_unavailable", message: "The selected provider is unavailable. Refresh the provider list and select it again.".into() })?
            .normalize();

        let form = [
            ("client_id", STEAM_DECK_CLIENT_ID),
            ("scope", SCOPES),
            ("device_id", self.device_id.as_str()),
            ("display_name", "OpenNOW"),
            ("idp_id", provider.idp_id.as_str()),
        ];
        let response = self
            .client
            .post(&self.endpoints.device_authorize)
            .header(ACCEPT, "application/json, text/plain, */*")
            .header(
                CONTENT_TYPE,
                "application/x-www-form-urlencoded; charset=UTF-8",
            )
            .header(ORIGIN, "https://play.geforcenow.com")
            .header(REFERER, "https://play.geforcenow.com/")
            .header(USER_AGENT, STEAM_DECK_USER_AGENT)
            .header("x-device-id", &self.device_id)
            .header("nv-client-id", STEAM_DECK_CLIENT_ID)
            .header("nv-client-streamer", "WEBRTC")
            .header("nv-client-type", "BROWSER")
            .header("nv-client-platform-name", "browser")
            .header("nv-browser-type", "CHROME")
            .header("nv-device-os", "STEAMOS")
            .header("nv-device-type", "CONSOLE")
            .header("nv-device-model", "STEAMDECK")
            .header("nv-device-make", "VALVE")
            .form(&form)
            .send()
            .map_err(|error| ServiceError::network("Device authorization failed", error))?;
        eprintln!("auth: device authorization response {}", response.status());
        if !response.status().is_success() {
            return Err(ServiceError::response(
                "Device authorization failed",
                response,
            ));
        }
        let payload = bounded_auth_response(response)?;
        let device_code = required_string(&payload, "device_code")?;
        let user_code = required_string(&payload, "user_code")?;
        let verification_uri = required_string(&payload, "verification_uri")?;
        let verification_uri_complete = required_string(&payload, "verification_uri_complete")?;
        let lifetime = payload["expires_in"]
            .as_u64()
            .filter(|value| *value > 0 && *value <= 3600)
            .unwrap_or(600);
        let expires_at = now_ms().saturating_add(lifetime * 1000);
        let interval_seconds = payload["interval"]
            .as_u64()
            .filter(|value| *value > 0)
            .unwrap_or(5)
            .min(3600);
        let attempt_id = random_attempt_id();
        eprintln!("auth: prepared device authorization challenge");
        self.prune_attempts();
        crate::requests::check()?;
        let mut state = self.state.lock().expect("GFN state poisoned");
        if state.login_generation != generation {
            return Err(ServiceError::invalid("QR login was replaced or cancelled"));
        }
        state.attempts.insert(
            attempt_id.clone(),
            DeviceAttempt {
                provider,
                device_code: device_code.clone(),
                expires_at,
                deadline: Instant::now() + Duration::from_secs(lifetime),
                interval_seconds,
                next_poll: Instant::now() + Duration::from_secs(interval_seconds),
                in_flight: false,
                pending_session: None,
            },
        );
        Ok(json!({
            "attemptId": attempt_id,
            "userCode": user_code,
            "verificationUri": verification_uri,
            "verificationUriComplete": verification_uri_complete,
            "expiresAt": expires_at,
            "intervalSeconds": interval_seconds,
            "qrRows": qr_rows(&verification_uri_complete),
        }))
    }

    pub fn poll_device_login(&self, params: &Value) -> Result<Value, ServiceError> {
        crate::requests::check()?;
        let attempt_id = required_param(params, "attemptId")?;
        self.prune_attempts();
        let attempt = {
            let mut state = self.state.lock().expect("GFN state poisoned");
            let Some(attempt) = state.attempts.get_mut(attempt_id) else {
                return Ok(
                    json!({"status":"expired", "error":"QR login was cancelled or expired"}),
                );
            };
            if attempt.pending_session.is_some() {
                return Ok(json!({"status":"authorized"}));
            }
            if attempt.in_flight || Instant::now() < attempt.next_poll {
                return Ok(
                    json!({"status":"pending", "retryAfterMs": attempt.next_poll.saturating_duration_since(Instant::now()).as_millis().max(1000), "intervalSeconds":attempt.interval_seconds}),
                );
            }
            attempt.in_flight = true;
            attempt.clone()
        };
        let _poll = DevicePoll {
            service: self,
            attempt_id,
        };
        let form = [
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("device_code", attempt.device_code.as_str()),
            ("client_id", STEAM_DECK_CLIENT_ID),
        ];
        let response = self
            .client
            .post(&self.endpoints.token)
            .header(ACCEPT, "application/json, text/plain, */*")
            .header(
                CONTENT_TYPE,
                "application/x-www-form-urlencoded; charset=UTF-8",
            )
            .header(ORIGIN, "https://play.geforcenow.com")
            .header(REFERER, "https://play.geforcenow.com/")
            .header(USER_AGENT, STEAM_DECK_USER_AGENT)
            .form(&form)
            .send()
            .map_err(|error| ServiceError::network("Device token exchange failed", error))?;
        let status = response.status();
        let payload = bounded_auth_response(response)?;
        self.check_device_attempt(attempt_id)?;
        if !status.is_success() {
            let error = payload["error"]
                .as_str()
                .unwrap_or("device_token_exchange_failed");
            return Ok(match error {
                "authorization_pending" => {
                    json!({"status":"pending", "intervalSeconds":attempt.interval_seconds, "retryAfterMs":attempt.interval_seconds * 1000})
                }
                "slow_down" => {
                    let mut state = self.state.lock().expect("GFN state poisoned");
                    let stored = state
                        .attempts
                        .get_mut(attempt_id)
                        .ok_or_else(|| ServiceError::invalid("QR login was cancelled"))?;
                    stored.interval_seconds = stored.interval_seconds.saturating_add(5).min(3600);
                    json!({"status":"slow_down", "intervalSeconds":stored.interval_seconds, "retryAfterMs":stored.interval_seconds * 1000})
                }
                "expired_token" => {
                    self.cancel_device_login(params)?;
                    json!({"status":"expired", "error":"QR login expired"})
                }
                "access_denied" => {
                    self.cancel_device_login(params)?;
                    json!({"status":"access_denied", "error":"QR login was declined"})
                }
                _ => {
                    self.cancel_device_login(params)?;
                    json!({"status":"error", "error":"QR login token exchange failed"})
                }
            });
        }

        let access_token = required_string(&payload, "access_token")?;
        let tokens = AuthTokens {
            access_token,
            refresh_token: payload["refresh_token"].as_str().map(ToOwned::to_owned),
            id_token: payload["id_token"].as_str().map(ToOwned::to_owned),
            id_token_expires_at: payload["id_token"].as_str().and_then(jwt_expiry),
            expires_at: token_expiry(&payload),
            auth_client_id: STEAM_DECK_CLIENT_ID.to_owned(),
            client_token: payload["client_token"].as_str().map(ToOwned::to_owned),
            client_token_expires_at: None,
            client_token_lifetime_ms: None,
        };
        let tokens = self
            .ensure_client_token(tokens.clone())
            .unwrap_or_else(|error| {
                eprintln!("auth: client-token bootstrap deferred: {}", error.message);
                tokens
            });
        self.check_device_attempt(attempt_id)?;
        let user = self.fetch_user_info(&tokens)?;
        self.check_device_attempt(attempt_id)?;
        let session = AuthSession {
            provider: attempt.provider,
            tokens,
            user,
        };
        if let Some(stored) = self
            .state
            .lock()
            .expect("GFN state poisoned")
            .attempts
            .get_mut(attempt_id)
        {
            stored.pending_session = Some(session);
        } else {
            return Ok(json!({"status":"expired", "error":"QR login was cancelled"}));
        }
        Ok(json!({"status":"authorized"}))
    }

    pub fn complete_device_login(&self, params: &Value) -> Result<Value, ServiceError> {
        let _operation = self
            .auth_operation
            .lock()
            .expect("GFN auth operation poisoned");
        crate::requests::check()?;
        let attempt_id = required_param(params, "attemptId")?;
        crate::requests::current().commit(|| {
            let mut state = self.state.lock().expect("GFN state poisoned");
            let attempt = state
                .attempts
                .get(attempt_id)
                .filter(|attempt| {
                    attempt.deadline > Instant::now() && attempt.expires_at > now_ms()
                })
                .ok_or_else(|| ServiceError::invalid("QR login is no longer active"))?;
            let session = attempt
                .pending_session
                .clone()
                .ok_or_else(|| ServiceError::invalid("QR login has not been authorized yet"))?;
            let persist = params["staySignedIn"].as_bool().unwrap_or(true);
            state.persistence_intent = if persist {
                PersistenceIntent::SecureStore
            } else {
                PersistenceIntent::MemoryOnly
            };
            state.persistence_state = if persist && self.vault.save(&session).is_ok() {
                "secure-store"
            } else {
                if !persist {
                    let _ = self.vault.remove(&session.user.user_id);
                }
                "memory-only"
            }
            .into();
            state.attempts.clear();
            self.account_connections.cancel_pending();
            state.session = Some(session);
            state.restore_attempted = true;
            state.generation += 1;
            state.login_generation += 1;
            state.refresh_retry_at = None;
            Ok(self.auth_envelope(
                &state,
                state.session.as_ref(),
                json!({"attempted":false,"outcome":"not_attempted"}),
            ))
        })
    }

    pub fn cancel_device_login(&self, params: &Value) -> Result<Value, ServiceError> {
        let attempt_id = required_param(params, "attemptId")?;
        self.state
            .lock()
            .expect("GFN state poisoned")
            .attempts
            .remove(attempt_id);
        Ok(json!({"cancelled":true}))
    }

    fn check_device_attempt(&self, attempt_id: &str) -> Result<(), ServiceError> {
        crate::requests::check()?;
        if self
            .state
            .lock()
            .expect("GFN state poisoned")
            .attempts
            .get(attempt_id)
            .is_none_or(|attempt| {
                attempt.deadline <= Instant::now() || attempt.expires_at <= now_ms()
            })
        {
            return Err(ServiceError::invalid("QR login was cancelled or expired"));
        }
        Ok(())
    }

    fn auth_envelope(
        &self,
        state: &ServiceState,
        session: Option<&AuthSession>,
        refresh: Value,
    ) -> Value {
        json!({"session":session.map(AuthSession::public), "generation":state.generation,
            "persistence":state.persistence_state, "refresh":refresh, "warnings":self.vault.warnings(),
            "deviceIdentity": if self.device_identity_error.is_some() { "unavailable" } else { "durable" }})
    }

    pub fn session(&self) -> Result<Value, ServiceError> {
        let _operation = self
            .auth_operation
            .lock()
            .expect("GFN auth operation poisoned");
        crate::requests::check()?;
        self.session_locked()
    }

    fn session_locked(&self) -> Result<Value, ServiceError> {
        let (session, refresh) =
            self.resolve_session_locked(TokenPurpose::StarfleetAccess, false)?;
        let state = self.state.lock().expect("GFN state poisoned");
        Ok(self.auth_envelope(&state, session.as_ref(), refresh))
    }

    fn resolve_session_locked(
        &self,
        purpose: TokenPurpose,
        force: bool,
    ) -> Result<(Option<AuthSession>, Value), ServiceError> {
        {
            let mut state = self.state.lock().expect("GFN state poisoned");
            if state.session.is_none() && !state.restore_attempted {
                state.restore_attempted = true;
                drop(state);
                match self.vault.load_active() {
                    Ok(session) => {
                        let mut state = self.state.lock().expect("GFN state poisoned");
                        state.persistence_state = if session
                            .as_ref()
                            .is_some_and(|session| self.vault.durable(session))
                        {
                            "secure-store"
                        } else if session.is_some() {
                            "migration-pending"
                        } else {
                            "none"
                        }
                        .to_owned();
                        state.persistence_intent = PersistenceIntent::SecureStore;
                        state.generation += 1;
                        state.session = session;
                    }
                    Err(error) => {
                        eprintln!("auth: saved session unavailable: {error}");
                        self.state
                            .lock()
                            .expect("GFN state poisoned")
                            .persistence_state = "unavailable".to_owned();
                    }
                }
            }
        }
        let current = self
            .state
            .lock()
            .expect("GFN state poisoned")
            .session
            .clone();
        let Some(current) = current else {
            return Ok((
                None,
                json!({"attempted":false,"outcome":"not_attempted","message":"No saved session found."}),
            ));
        };

        let needs_refresh = force
            || current.tokens.expiry(purpose) <= now_ms().saturating_add(TOKEN_REFRESH_WINDOW_MS);
        let needs_client_token = current
            .tokens
            .client_token
            .as_deref()
            .unwrap_or("")
            .is_empty()
            || current
                .tokens
                .client_token_expires_at
                .is_none_or(|expiry| expiry <= now_ms() + CLIENT_TOKEN_REFRESH_WINDOW_MS);
        if self
            .state
            .lock()
            .expect("GFN state poisoned")
            .refresh_retry_at
            .is_some_and(|deadline| deadline > Instant::now())
        {
            return Ok((
                if !force && current.tokens.expiry(purpose) > now_ms() {
                    Some(current)
                } else {
                    None
                },
                json!({"attempted":false,"outcome":"deferred","message":"Authentication renewal is temporarily deferred."}),
            ));
        }
        let (session, refresh) = if needs_refresh {
            match self.refresh_session(&current) {
                Ok(session) if session.tokens.expiry(purpose) > now_ms() => (
                    Some(session),
                    json!({"attempted":true,"outcome":"refreshed","message":"Saved session token refreshed."}),
                ),
                Ok(_) => {
                    self.defer_refresh();
                    (
                        None,
                        json!({"attempted":true,"outcome":"expired","message":"The required authentication token was not renewed. Sign in again."}),
                    )
                }
                Err(error)
                    if !force
                        && current.tokens.expiry(purpose) > now_ms()
                        && !is_definitive_auth_revocation(&error) =>
                {
                    self.defer_refresh();
                    eprintln!(
                        "auth: refresh failed; using unexpired token: {}",
                        error.message
                    );
                    (
                        Some(current),
                        json!({"attempted":true,"outcome":"failed","message":"Refresh failed; using the unexpired saved token."}),
                    )
                }
                Err(error) if is_definitive_auth_revocation(&error) => {
                    eprintln!("auth: saved session was revoked: {}", error.message);
                    let _ = self.vault.remove(&current.user.user_id);
                    self.account_connections.cancel_pending();
                    let mut state = self.state.lock().expect("GFN state poisoned");
                    state.session = None;
                    state.persistence_state = "none".into();
                    state.persistence_intent = PersistenceIntent::MemoryOnly;
                    state.restore_attempted = true;
                    state.generation += 1;
                    (
                        None,
                        json!({"attempted":true,"outcome":"revoked","message":"Saved session is no longer valid. Sign in again."}),
                    )
                }
                Err(error) => {
                    self.defer_refresh();
                    eprintln!("auth: expired session could not refresh: {}", error.message);
                    (
                        None,
                        json!({"attempted":true,"outcome":"expired","message":"Saved session expired. Sign in again if this continues."}),
                    )
                }
            }
        } else if needs_client_token {
            match self.update_client_token(&current) {
                Ok(session) => (
                    Some(session),
                    json!({"attempted":true,"outcome":"refreshed","message":"Client token refreshed."}),
                ),
                Err(error) => {
                    self.defer_refresh();
                    eprintln!("auth: client-token bootstrap deferred: {}", error.message);
                    (
                        Some(current),
                        json!({"attempted":true,"outcome":"failed","message":"Session is valid; client-token refresh was deferred."}),
                    )
                }
            }
        } else {
            (
                Some(current),
                json!({"attempted":false,"outcome":"not_attempted","message":"Session token is still valid."}),
            )
        };
        Ok((session, refresh))
    }

    fn defer_refresh(&self) {
        self.state
            .lock()
            .expect("GFN state poisoned")
            .refresh_retry_at = Some(Instant::now() + Duration::from_secs(30));
    }

    pub fn logout(&self) -> Result<Value, ServiceError> {
        let _operation = self
            .auth_operation
            .lock()
            .expect("GFN auth operation poisoned");
        crate::requests::check()?;
        let session = self
            .state
            .lock()
            .expect("GFN state poisoned")
            .session
            .clone();
        self.invalidate_auth_work(true);
        let cleanup = session.as_ref().map(|session| {
            self.cleanup_account(
                &session.user.user_id,
                Some(session),
                Instant::now() + Duration::from_secs(5),
            )
        });
        self.restore_next_account();
        let state = self.state.lock().expect("GFN state poisoned");
        let mut result = self.auth_envelope(&state, state.session.as_ref(), Value::Null);
        result["ok"] = json!(true);
        result["cleanup"] = json!(cleanup);
        Ok(result)
    }

    pub fn logout_all(&self) -> Result<Value, ServiceError> {
        let _operation = self
            .auth_operation
            .lock()
            .expect("GFN auth operation poisoned");
        crate::requests::check()?;
        let active = self
            .state
            .lock()
            .expect("GFN state poisoned")
            .session
            .clone();
        self.invalidate_auth_work(true);
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut ids: Vec<String> = self
            .vault
            .list()
            .unwrap_or_default()
            .iter()
            .filter_map(|account| account["userId"].as_str().map(str::to_owned))
            .collect();
        if let Some(session) = &active {
            if !ids.contains(&session.user.user_id) {
                ids.push(session.user.user_id.clone());
            }
        }
        let mut cleanup = Vec::new();
        for id in ids {
            let session = active
                .as_ref()
                .filter(|session| session.user.user_id == id)
                .cloned()
                .or_else(|| self.vault.load(&id).ok().flatten());
            cleanup.push(self.cleanup_account(&id, session.as_ref(), deadline));
        }
        let local = self.vault.remove_all();
        let profiles = self.profiles.forget_all();
        let state = self.state.lock().expect("GFN state poisoned");
        let mut result = self.auth_envelope(&state, None, Value::Null);
        result["ok"] = json!(true);
        result["cleanup"] = json!(cleanup);
        result["localCleanup"] = json!(if local.is_ok() && profiles.is_ok() {
            "complete"
        } else {
            "pending"
        });
        Ok(result)
    }

    fn invalidate_auth_work(&self, clear_session: bool) {
        self.account_connections.cancel_pending();
        let mut state = self.state.lock().expect("GFN state poisoned");
        state.attempts.clear();
        state.login_generation += 1;
        state.generation += 1;
        state.refresh_retry_at = None;
        state.restore_attempted = true;
        if clear_session {
            state.session = None;
            state.persistence_state = "none".into();
            state.persistence_intent = PersistenceIntent::MemoryOnly;
        }
    }

    fn cleanup_account(
        &self,
        user_id: &str,
        session: Option<&AuthSession>,
        deadline: Instant,
    ) -> Value {
        let remote = if let Some(session) = session.filter(|_| Instant::now() < deadline) {
            match self
                .client
                .delete(&self.endpoints.revoke)
                .timeout(
                    deadline
                        .saturating_duration_since(Instant::now())
                        .max(Duration::from_millis(1)),
                )
                .bearer_auth(&session.tokens.access_token)
                .header(ACCEPT, "application/json")
                .send()
            {
                Ok(response) if response.status().is_success() => "complete",
                _ => "failed",
            }
        } else {
            "not_attempted"
        };
        let local = self.vault.remove(user_id);
        let profile = self.profiles.forget(user_id);
        json!({"remoteRevoke":remote, "localCleanup":if local.is_ok() && profile.is_ok() { "complete" } else { "pending" }})
    }

    fn restore_next_account(&self) {
        let next = self.vault.load_active().ok().flatten();
        let mut state = self.state.lock().expect("GFN state poisoned");
        state.persistence_state = match &next {
            Some(session) if self.vault.durable(session) => "secure-store",
            Some(_) => "migration-pending",
            None => "none",
        }
        .into();
        state.persistence_intent = if next.is_some() {
            PersistenceIntent::SecureStore
        } else {
            PersistenceIntent::MemoryOnly
        };
        state.session = next;
    }

    pub fn clear_cache(&self) -> Value {
        let mut state = self.state.lock().expect("GFN state poisoned");
        let catalog_entries = state.public_games.len();
        let provider_entries = state.providers.len();
        state.public_games.clear();
        state.providers.clear();
        state.providers_expires = None;
        state.providers_retry = None;
        state.providers_error = None;
        state.generation += 1;
        json!({"ok":true,"catalogEntries":catalog_entries,"providerEntries":provider_entries})
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn with_region_provider<T>(
        &self,
        provider: &str,
        write: impl FnOnce() -> Result<T, ServiceError>,
    ) -> Result<T, ServiceError> {
        let _operation = crate::store_requests::lock(&self.auth_operation)?;
        if self
            .state
            .lock()
            .expect("GFN state poisoned")
            .session
            .as_ref()
            .is_none_or(|session| session.provider.idp_id != provider)
        {
            return Err(ServiceError {
                code: "stale_account",
                message: "The provider changed before saving the region".into(),
            });
        }
        write()
    }

    pub fn saved_accounts(&self) -> Result<Value, ServiceError> {
        let mut accounts = self.vault.list().map_err(|message| ServiceError {
            code: "credential_store_error",
            message,
        })?;
        for account in &mut accounts {
            let user_id = account["userId"].as_str().unwrap_or_default();
            account["hasPin"] = Value::Bool(self.profiles.has_pin(user_id));
        }
        let active_user_id = self
            .state
            .lock()
            .expect("GFN state poisoned")
            .session
            .as_ref()
            .map(|session| session.user.user_id.clone());
        Ok(
            json!({"accounts":accounts,"activeUserId":active_user_id,"generation":self.auth_generation()}),
        )
    }

    pub fn switch_account(&self, params: &Value) -> Result<Value, ServiceError> {
        let _operation = self
            .auth_operation
            .lock()
            .expect("GFN auth operation poisoned");
        crate::requests::check()?;
        let user_id = required_param(params, "userId")?;
        if self.profiles.has_pin(user_id) {
            let verification = self
                .profiles
                .verify(user_id, params["pin"].as_str().unwrap_or(""))
                .map_err(|message| ServiceError {
                    code: "profile_storage_error",
                    message,
                })?;
            if verification["ok"].as_bool() != Some(true) {
                return Err(ServiceError {
                    code: if verification["reason"] == "locked_out" {
                        "profile_pin_locked"
                    } else {
                        "profile_pin_required"
                    },
                    message: if verification["reason"] == "locked_out" {
                        "Profile PIN is temporarily locked".to_owned()
                    } else {
                        "Profile PIN is required or incorrect".to_owned()
                    },
                });
            }
        }
        let session = self
            .vault
            .load(user_id)
            .map_err(|message| ServiceError {
                code: "credential_store_error",
                message,
            })?
            .ok_or_else(|| ServiceError {
                code: "saved_account_not_found",
                message: "Saved account not found".to_owned(),
            })?;
        if session.user.user_id != user_id {
            return Err(ServiceError {
                code: "session_identity_mismatch",
                message: "Saved session did not match the selected account".to_owned(),
            });
        }
        self.vault
            .set_active(user_id)
            .map_err(|message| ServiceError {
                code: "credential_store_error",
                message,
            })?;
        {
            let mut state = self.state.lock().expect("GFN state poisoned");
            state.persistence_state = if self.vault.durable(&session) {
                "secure-store"
            } else {
                "migration-pending"
            }
            .to_owned();
            state.persistence_intent = PersistenceIntent::SecureStore;
            state.session = Some(session);
        }
        self.invalidate_auth_work(false);
        let result = self.session_locked()?;
        Ok(result)
    }

    pub fn remove_account(&self, params: &Value) -> Result<Value, ServiceError> {
        let _operation = self
            .auth_operation
            .lock()
            .expect("GFN auth operation poisoned");
        crate::requests::check()?;
        let user_id = required_param(params, "userId")?;
        let active = self
            .state
            .lock()
            .expect("GFN state poisoned")
            .session
            .clone();
        let was_active = active
            .as_ref()
            .is_some_and(|session| session.user.user_id == user_id);
        let selected = active
            .filter(|session| session.user.user_id == user_id)
            .or_else(|| self.vault.load(user_id).ok().flatten());
        self.invalidate_auth_work(was_active);
        let cleanup = self.cleanup_account(
            user_id,
            selected.as_ref(),
            Instant::now() + Duration::from_secs(5),
        );
        if was_active {
            self.restore_next_account();
        }
        let state = self.state.lock().expect("GFN state poisoned");
        let mut result = self.auth_envelope(&state, state.session.as_ref(), Value::Null);
        result["ok"] = json!(true);
        result["cleanup"] = cleanup;
        Ok(result)
    }

    pub fn pin_status(&self, params: &Value) -> Result<Value, ServiceError> {
        let user_id = self.profile_user_id(params)?;
        Ok(self.profiles.status(&user_id))
    }

    pub fn set_pin(&self, params: &Value) -> Result<Value, ServiceError> {
        let user_id = self.profile_user_id(params)?;
        let pin = required_param(params, "pin")?;
        self.profiles
            .set_pin(&user_id, pin, params["currentPin"].as_str())
            .map_err(|message| ServiceError {
                code: "profile_storage_error",
                message,
            })
    }

    pub fn clear_pin(&self, params: &Value) -> Result<Value, ServiceError> {
        let user_id = self.profile_user_id(params)?;
        let pin = required_param(params, "currentPin")?;
        self.profiles
            .clear_pin(&user_id, pin)
            .map_err(|message| ServiceError {
                code: "profile_storage_error",
                message,
            })
    }

    pub fn verify_pin(&self, params: &Value) -> Result<Value, ServiceError> {
        let user_id = self.profile_user_id(params)?;
        let pin = params["pin"].as_str().unwrap_or("");
        self.profiles
            .verify(&user_id, pin)
            .map_err(|message| ServiceError {
                code: "profile_storage_error",
                message,
            })
    }

    fn profile_user_id(&self, params: &Value) -> Result<String, ServiceError> {
        if let Some(user_id) = params["userId"].as_str().filter(|value| !value.is_empty()) {
            return Ok(user_id.to_owned());
        }
        self.state
            .lock()
            .expect("GFN state poisoned")
            .session
            .as_ref()
            .map(|session| session.user.user_id.clone())
            .ok_or_else(|| ServiceError {
                code: "authentication_required",
                message: "Sign in to manage a profile PIN".to_owned(),
            })
    }

    pub fn public_catalog(&self, params: &Value, settings: &Value) -> Result<Value, ServiceError> {
        let limit = params["limit"].as_u64().unwrap_or(240).clamp(1, 1000) as usize;
        let query = params["searchQuery"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_lowercase();
        let proxy = config_from_settings(settings).map_err(ServiceError::invalid)?;
        let proxy_scope = proxy
            .as_ref()
            .map(|value| value.cache_scope.clone())
            .unwrap_or_else(|| "direct".to_owned());
        let bypass_cache = proxy.as_ref().is_some_and(|value| value.has_credentials);
        let client = client_for_settings(&self.client, settings).map_err(ServiceError::invalid)?;
        let refresh = params["refresh"].as_bool().unwrap_or(false) || bypass_cache;
        let mut cached = {
            let state = self.state.lock().expect("GFN state poisoned");
            if state.public_games_proxy_scope == proxy_scope {
                state.public_games.clone()
            } else {
                Vec::new()
            }
        };
        if cached.is_empty() || refresh {
            let response = client
                .get(&self.endpoints.public_catalog)
                .header(ACCEPT, "application/json")
                .header(USER_AGENT, GFN_USER_AGENT)
                .send()
                .map_err(|error| ServiceError::network("Public games fetch failed", error))?;
            if !response.status().is_success() {
                return Err(ServiceError::response(
                    "Public games fetch failed",
                    response,
                ));
            }
            let raw = response
                .json::<Vec<Value>>()
                .map_err(|error| ServiceError::network("Invalid public games response", error))?;
            cached = raw.iter().filter_map(public_game_to_info).collect();
            cached.sort_by(|left, right| {
                left["title"]
                    .as_str()
                    .unwrap_or("")
                    .to_lowercase()
                    .cmp(&right["title"].as_str().unwrap_or("").to_lowercase())
            });
            if !bypass_cache {
                let mut state = self.state.lock().expect("GFN state poisoned");
                state.public_games = cached.clone();
                state.public_games_proxy_scope = proxy_scope;
            }
        }
        let filtered = cached
            .iter()
            .filter(|game| {
                query.is_empty()
                    || game["searchText"]
                        .as_str()
                        .is_some_and(|text| text.contains(&query))
            })
            .take(limit)
            .cloned()
            .collect::<Vec<_>>();
        Ok(
            json!({"games":filtered, "count":filtered.len(), "totalCount":cached.len(), "fetchedAt":now_ms()}),
        )
    }

    pub fn regions(&self, settings: &Value) -> Result<Value, ServiceError> {
        let client = client_for_settings(&self.client, settings).map_err(ServiceError::invalid)?;
        self.authenticated_read(|session, generation| {
            self.check_scope(session, generation)?;
            self.provider_regions(&client, session)
        })
    }

    fn provider_regions(
        &self,
        client: &Client,
        session: &AuthSession,
    ) -> Result<Value, ServiceError> {
        let token = session.tokens.service_token();
        let base = provider_streaming_base(&session.provider)?;
        let url = self.server_info_url(&base)?;
        let response = client
            .get(url)
            .headers(lcars_headers(token, "BROWSER", "WEBRTC", false)?)
            .send()
            .map_err(|error| ServiceError::network("Region discovery failed", error))?;
        if !response.status().is_success() {
            return Err(ServiceError::response("Region discovery failed", response));
        }
        let payload = response
            .json::<Value>()
            .map_err(|error| ServiceError::network("Invalid region response", error))?;
        let vpc_id = verified_vpc(&payload)?;
        if payload["metaData"].as_array().is_none() {
            return Err(ServiceError {
                code: "invalid_upstream_response",
                message: "Provider server info has no regions or verified VPC".into(),
            });
        }
        let mut regions = payload["metaData"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                let name = entry["key"].as_str()?;
                let value = entry["value"].as_str()?;
                if !value.starts_with("https://")
                    || name == "gfn-regions"
                    || name.starts_with("gfn-")
                {
                    return None;
                }
                Some(json!({"name":name,"url":trusted_streaming_base(value).ok()?.as_str()}))
            })
            .collect::<Vec<_>>();
        regions.sort_by(|left, right| {
            left["name"]
                .as_str()
                .unwrap_or("")
                .cmp(right["name"].as_str().unwrap_or(""))
        });
        Ok(json!({"regions":regions,"vpcId":vpc_id,"providerIdpId":session.provider.idp_id}))
    }

    pub fn subscription(&self, settings: &Value) -> Result<Value, ServiceError> {
        self.authenticated_read(|session, generation| {
        let client = client_for_settings(&self.client, settings).map_err(ServiceError::invalid)?;
        let token = session.tokens.service_token();
        let vpc_id = self.vpc_id(&client, session, generation, settings, token, None)?;
        let steam_deck = settings["identifyAsSteamDeck"].as_bool().unwrap_or(false);
        let mut url = url::Url::parse(&self.endpoints.subscription).expect("MES URL is valid");
        url.query_pairs_mut()
            .append_pair("serviceName", "gfn_pc")
            .append_pair("languageCode", "en_US")
            .append_pair("vpcId", &vpc_id)
            .append_pair("userId", &session.user.user_id);
        let response = client
            .get(url)
            .headers(lcars_headers(
                token,
                "NATIVE",
                "NVIDIA-CLASSIC",
                steam_deck,
            )?)
            .send()
            .map_err(|error| ServiceError::network("Subscription request failed", error))?;
        if !response.status().is_success() {
            return Err(ServiceError::response(
                "Subscription request failed",
                response,
            ));
        }
        let data = response
            .json::<Value>()
            .map_err(|error| ServiceError::network("Invalid subscription response", error))?;
        let allotted = number_value(&data["allottedTimeInMinutes"]).unwrap_or(0.0);
        let purchased = number_value(&data["purchasedTimeInMinutes"]).unwrap_or(0.0);
        let rolled = number_value(&data["rolledOverTimeInMinutes"]).unwrap_or(0.0);
        let total =
            number_value(&data["totalTimeInMinutes"]).unwrap_or(allotted + purchased + rolled);
        let remaining = number_value(&data["remainingTimeInMinutes"]).unwrap_or(0.0);
        let mut resolutions = data["features"]["resolutions"].as_array().into_iter().flatten()
            .filter(|resolution| resolution["isEntitled"].as_bool() == Some(true))
            .map(|resolution| json!({"width":resolution["widthInPixels"],"height":resolution["heightInPixels"],"fps":resolution["framesPerSecond"]}))
            .collect::<Vec<_>>();
        resolutions.sort_by(|left, right| {
            right["width"]
                .as_i64()
                .cmp(&left["width"].as_i64())
                .then_with(|| right["height"].as_i64().cmp(&left["height"].as_i64()))
                .then_with(|| right["fps"].as_i64().cmp(&left["fps"].as_i64()))
        });
        let membership = data["membershipTier"].as_str().unwrap_or("FREE");
        let storage_addon = data["addons"].as_array().into_iter().flatten().find(|addon| {
            addon["type"].as_str() == Some("STORAGE")
                && addon["subType"].as_str() == Some("PERMANENT_STORAGE")
                && addon["status"].as_str() == Some("OK")
        }).map(|addon| {
            let attribute = |key: &str| addon["attributes"].as_array().into_iter().flatten()
                .find(|attribute| attribute["key"].as_str() == Some(key))
                .and_then(|attribute| attribute["textValue"].as_str());
            json!({
                "type":"PERMANENT_STORAGE",
                "sizeGb":attribute("TOTAL_STORAGE_SIZE_IN_GB").and_then(|value| value.parse::<f64>().ok()),
                "usedGb":attribute("USED_STORAGE_SIZE_IN_GB").and_then(|value| value.parse::<f64>().ok()),
                "regionName":attribute("STORAGE_METRO_REGION_NAME"),
                "regionCode":attribute("STORAGE_METRO_REGION")
            })
        });
        self.check_scope(session, generation)?;
        Ok(json!({"subscription":{
            "membershipTier":membership,"subscriptionType":data["type"],"subscriptionSubType":data["subType"],
            "allottedHours":allotted/60.0,"purchasedHours":purchased/60.0,"rolledOverHours":rolled/60.0,
            "usedHours":(total-remaining).max(0.0)/60.0,"remainingHours":remaining/60.0,"totalHours":total/60.0,
            "firstEntitlementStartDateTime":data["firstEntitlementStartDateTime"],"serverRegionId":vpc_id,
            "currentSpanStartDateTime":data["currentSpanStartDateTime"],"currentSpanEndDateTime":data["currentSpanEndDateTime"],
            "notifyUserWhenTimeRemainingInMinutes":data["notifications"]["notifyUserWhenTimeRemainingInMinutes"],
            "notifyUserOnSessionWhenRemainingTimeInMinutes":data["notifications"]["notifyUserOnSessionWhenRemainingTimeInMinutes"],
            "state":data["currentSubscriptionState"]["state"],"isGamePlayAllowed":data["currentSubscriptionState"]["isGamePlayAllowed"],
            "isUnlimited":data["subType"] == "UNLIMITED","entitledResolutions":resolutions,"storageAddon":storage_addon
        }}))
        })
    }

    pub fn create_session(&self, params: &Value, settings: &Value) -> Result<Value, ServiceError> {
        let admission = self.cloudmatch.admit_create()?;
        let app_id = params["catalogAppId"].as_str().unwrap_or_default();
        let variant_id = params["variantId"].as_str().unwrap_or_default();
        if params["appId"].as_str() != Some(variant_id) {
            return Err(ServiceError::invalid(
                "The launch ID must match the selected variant",
            ));
        }
        self.providers()?;
        let (intent_session, intent_generation) =
            self.authenticated_snapshot(TokenPurpose::ServiceId, false)?;
        if params["scope"] != scoped_result(json!({}), &intent_session, intent_generation)["scope"]
        {
            return Err(ServiceError {
                code: "stale_account",
                message: "This launch belongs to a different account context.".into(),
            });
        }
        let _catalog_action = self.admit_catalog_action(&intent_session, app_id)?;
        let store_launch = store_launch::store_launch_intent(params)?;
        let inspection = self.catalog_launch_inspect(
            &json!({"appId":app_id,"variantId":variant_id,"storeLaunch":store_launch}),
            settings,
        )?;
        if inspection["decision"]["status"] != "ready" {
            return Err(ServiceError {
                code: "launch_not_ready",
                message: inspection["decision"]["message"]
                    .as_str()
                    .unwrap_or("The selected store version cannot be launched")
                    .into(),
            });
        }
        self.providers()?;
        let mut routing = crate::store_requests::lock(&self.session_routing)?;
        let _operation = crate::store_requests::lock(&self.auth_operation)?;
        let (session, generation) = self.session_snapshot_locked()?;
        if inspection["scope"] != params["scope"]
            || inspection["scope"] != scoped_result(json!({}), &session, generation)["scope"]
            || inspection["catalogRevision"].as_u64()
                != Some(
                    self.catalog_revision
                        .load(std::sync::atomic::Ordering::Acquire),
                )
        {
            return Err(ServiceError {
                code: "stale_account",
                message: "The launch context changed. Try again.".into(),
            });
        }
        if !self.cloudmatch.active()["session"].is_null() {
            return Err(ServiceError {
                code: "session_update_busy",
                message: "End the active session before starting another game".into(),
            });
        }
        let variant = catalog_actions::selected_variant(&inspection["game"], variant_id)
            .expect("ready decision validated the exact variant");
        let (mut params, settings) = self.scoped_session_route(params, settings, &session)?;
        params["accountLinked"] = variant["inLibrary"].clone();
        params["supportsInGameSettingsPersistence"] =
            variant["supportsInGameSettingsPersistence"].clone();
        params["title"] = inspection["game"]["title"].clone();
        let result = admission
            .create(&params, &settings, &session, &self.device_id)
            .map(|result| scoped_result(result, &session, generation));
        if !self.cloudmatch.active()["session"].is_null() {
            routing.active_owner = Some(ActiveSeatOwner::capture(
                session,
                generation,
                &self.cloudmatch.active()["session"],
                Some(generation),
            )?);
        }
        result
    }

    pub fn poll_session(&self, params: &Value) -> Result<Value, ServiceError> {
        let mut routing = crate::store_requests::lock(&self.session_routing)?;
        let _operation = crate::store_requests::lock(&self.auth_operation)?;
        if let Some(owner) = &routing.active_owner
            && !self
                .state
                .lock()
                .expect("GFN state poisoned")
                .session
                .as_ref()
                .is_some_and(|current| owner.matches(current, &owner.session_id))
        {
            if owner.auth.tokens.expiry(TokenPurpose::ServiceId) <= now_ms() {
                return Err(ServiceError {
                    code: "session_owner_authentication_required",
                    message:
                        "Sign in to the active session's original account to continue managing it"
                            .into(),
                });
            }
            let active = self.cloudmatch.active()["session"].clone();
            if params["sessionId"]
                .as_str()
                .is_some_and(|id| active["sessionId"] != id)
                || active["sessionId"] != owner.session_id
            {
                return Err(session_owner_error());
            }
            let result = self
                .cloudmatch
                .poll(&active, &owner.auth, &self.device_id)
                .map(|result| scoped_result(result, &owner.auth, owner.last_published_generation))
                .map_err(|mut error| {
                    if error.code == "http_unauthorized" {
                        error.code = "session_owner_authentication_required";
                    }
                    error
                });
            if self.cloudmatch.active()["session"].is_null() {
                routing.active_owner = None;
            }
            return result;
        }
        let result = self.session_read_locked(|session, generation| {
            let params = self.owned_session_params(params, &routing, session, generation, false)?;
            self.cloudmatch.poll(&params, session, &self.device_id)
        });
        result.and_then(|(result, session, generation)| {
            self.publish_active_result(&mut routing, &session, generation, result)
        })
    }

    pub fn finish_session_create(
        &self,
        session_id: &str,
        accepted: bool,
    ) -> Result<(), ServiceError> {
        let mut routing = self
            .session_routing
            .lock()
            .expect("Session routing poisoned");
        let accepted = accepted
            && routing.active_owner.as_ref().is_some_and(|owner| {
                owner.session_id == session_id
                    && owner
                        .allocation_generation
                        .is_some_and(|generation| self.check_scope(&owner.auth, generation).is_ok())
            });
        let result = self.cloudmatch.finish_create(session_id, accepted);
        if self.cloudmatch.active()["session"].is_null() {
            routing.active_owner = None;
        } else if result.is_ok()
            && let Some(owner) = &mut routing.active_owner
            && owner.session_id == session_id
        {
            owner.allocation_generation = None;
        }
        result
    }

    pub fn stop_session(&self, params: &Value, settings: &Value) -> Result<Value, ServiceError> {
        let mut routing = crate::store_requests::lock(&self.session_routing)?;
        let _operation = crate::store_requests::lock(&self.auth_operation)?;
        let active = self.cloudmatch.active()["session"].clone();
        let requested = params["sessionId"].as_str().unwrap_or("");
        if !active.is_null() && (requested.is_empty() || active["sessionId"] == requested) {
            let owner = routing
                .active_owner
                .as_ref()
                .ok_or_else(session_owner_error)?;
            let selected_owner = self
                .state
                .lock()
                .expect("GFN state poisoned")
                .session
                .as_ref()
                .is_some_and(|current| owner.matches(current, &owner.session_id));
            let (session, generation) = if selected_owner {
                self.session_snapshot_locked()?
            } else {
                (owner.auth.clone(), owner.last_published_generation)
            };
            if session.tokens.expiry(TokenPurpose::ServiceId) <= now_ms() {
                return Err(ServiceError {
                    code: "authentication_required",
                    message: "Sign in to the session's original account to end it".into(),
                });
            }
            if active["sessionId"] != owner.session_id {
                return Err(session_owner_error());
            }
            let result = self
                .cloudmatch
                .stop(&active, settings, &session, &self.device_id)
                .and_then(|result| {
                    self.publish_active_result(&mut routing, &session, generation, result)
                });
            return result;
        }
        let (session, generation) = self.session_snapshot_locked()?;
        let params = self.owned_session_params(params, &routing, &session, generation, true)?;
        self.cloudmatch
            .stop(&params, settings, &session, &self.device_id)
            .map(|result| scoped_result(result, &session, generation))
    }

    pub fn active_session(&self) -> Result<Value, ServiceError> {
        let mut routing = crate::store_requests::lock(&self.session_routing)?;
        let _operation = crate::store_requests::lock(&self.auth_operation)?;
        let state = self.state.lock().expect("GFN state poisoned");
        let active = self.cloudmatch.active();
        let Some(session) = state.session.as_ref().filter(|session| {
            routing.active_owner.as_ref().is_some_and(|owner| {
                owner.matches(
                    session,
                    active["session"]["sessionId"].as_str().unwrap_or(""),
                )
            })
        }) else {
            return Ok(json!({"session":null}));
        };
        let session = session.clone();
        let generation = state.generation;
        drop(state);
        self.publish_active_result(&mut routing, &session, generation, active)
    }

    pub fn remote_sessions(&self, params: &Value, settings: &Value) -> Result<Value, ServiceError> {
        self.providers()?;
        let mut routing = crate::store_requests::lock(&self.session_routing)?;
        let _operation = crate::store_requests::lock(&self.auth_operation)?;
        let (result, session, generation) = self.session_read_locked(|session, _| {
            let (mut params, settings) = self.scoped_session_route(params, settings, session)?;
            if routing.active_owner.as_ref().is_none_or(|owner| {
                !owner.matches(session, params["sessionId"].as_str().unwrap_or(""))
            }) {
                if let Some(params) = params.as_object_mut() {
                    params.remove("sessionId");
                }
            }
            self.cloudmatch
                .remote_sessions(&params, &settings, session, &self.device_id)
        })?;
        routing.discovery_owner = Some((session.provider.idp_id, session.user.user_id, generation));
        Ok(result)
    }

    pub fn claim_session(&self, params: &Value, settings: &Value) -> Result<Value, ServiceError> {
        let mut routing = crate::store_requests::lock(&self.session_routing)?;
        let _operation = crate::store_requests::lock(&self.auth_operation)?;
        let (session, generation) = self.session_snapshot_locked()?;
        let params = self.owned_session_params(params, &routing, &session, generation, true)?;
        let active = self.cloudmatch.active()["session"].clone();
        if !active.is_null() && active["sessionId"] != params["sessionId"] {
            return Err(session_owner_error());
        }
        let result = scoped_result(
            self.cloudmatch
                .claim(&params, settings, &session, &self.device_id)?,
            &session,
            generation,
        );
        if routing.active_owner.is_none() && !self.cloudmatch.active()["session"].is_null() {
            routing.active_owner = Some(ActiveSeatOwner::capture(
                session.clone(),
                generation,
                &self.cloudmatch.active()["session"],
                None,
            )?);
        }
        self.publish_active_result(&mut routing, &session, generation, result)
    }

    pub fn report_session_ad(&self, params: &Value) -> Result<Value, ServiceError> {
        let mut routing = crate::store_requests::lock(&self.session_routing)?;
        let _operation = crate::store_requests::lock(&self.auth_operation)?;
        let (session, generation) = self.session_snapshot_locked()?;
        let mut owned = self.owned_session_params(params, &routing, &session, generation, false)?;
        for key in [
            "action",
            "adId",
            "watchedTimeInMs",
            "pausedTimeInMs",
            "cancelReason",
            "clientTimestamp",
        ] {
            if let Some(value) = params.get(key) {
                owned[key] = value.clone();
            }
        }
        self.cloudmatch
            .report_ad(&owned, &session, &self.device_id)
            .and_then(|result| {
                self.publish_active_result(&mut routing, &session, generation, result)
            })
    }

    fn publish_active_result(
        &self,
        routing: &mut SessionRouting,
        session: &AuthSession,
        generation: u64,
        result: Value,
    ) -> Result<Value, ServiceError> {
        let active = self.cloudmatch.active();
        if active["session"].is_null() {
            routing.active_owner = None;
        } else {
            self.check_scope(session, generation)?;
            let owner = routing
                .active_owner
                .as_mut()
                .filter(|owner| {
                    owner.matches(
                        session,
                        active["session"]["sessionId"].as_str().unwrap_or(""),
                    )
                })
                .ok_or_else(session_owner_error)?;
            owner.auth = session.clone();
            owner.last_published_generation = generation;
        }
        Ok(scoped_result(result, session, generation))
    }

    fn session_snapshot_locked(&self) -> Result<(AuthSession, u64), ServiceError> {
        let session = self
            .resolve_session_locked(TokenPurpose::ServiceId, false)?
            .0
            .ok_or_else(|| ServiceError {
                code: "authentication_required",
                message: "Sign in to manage streaming sessions".into(),
            })?;
        let state = self.state.lock().expect("GFN state poisoned");
        let mut session = session;
        if let Some(provider) = state
            .providers
            .iter()
            .find(|provider| provider.idp_id == session.provider.idp_id)
        {
            session.provider = provider.clone();
        }
        provider_streaming_base(&session.provider)?;
        Ok((session, state.generation))
    }

    fn session_read_locked(
        &self,
        mut read: impl FnMut(&AuthSession, u64) -> Result<Value, ServiceError>,
    ) -> Result<(Value, AuthSession, u64), ServiceError> {
        let (session, generation) = self.session_snapshot_locked()?;
        let result = read(&session, generation);
        if result
            .as_ref()
            .is_err_and(|error| error.code == "http_unauthorized")
        {
            let renewed = self
                .resolve_session_locked(TokenPurpose::ServiceId, true)?
                .0
                .ok_or_else(|| ServiceError {
                    code: "authentication_required",
                    message: "Sign in to renew the session credential".into(),
                })?;
            self.check_scope(&session, generation)?;
            if renewed.provider.idp_id != session.provider.idp_id
                || renewed.user.user_id != session.user.user_id
            {
                return Err(session_owner_error());
            }
            let result = read(&renewed, generation)?;
            return Ok((
                scoped_result(result, &renewed, generation),
                renewed,
                generation,
            ));
        }
        Ok((
            scoped_result(result?, &session, generation),
            session,
            generation,
        ))
    }

    fn owned_session_params(
        &self,
        params: &Value,
        routing: &SessionRouting,
        session: &AuthSession,
        generation: u64,
        allow_discovered: bool,
    ) -> Result<Value, ServiceError> {
        let active = self.cloudmatch.active()["session"].clone();
        let id = params["sessionId"]
            .as_str()
            .or_else(|| active["sessionId"].as_str())
            .ok_or_else(session_owner_error)?;
        if active["sessionId"] == id {
            if routing
                .active_owner
                .as_ref()
                .is_some_and(|owner| owner.matches(session, id))
            {
                return Ok(active);
            }
            return Err(session_owner_error());
        }
        if allow_discovered
            && routing
                .discovery_owner
                .as_ref()
                .is_some_and(|(provider, user, scope)| {
                    provider == &session.provider.idp_id
                        && user == &session.user.user_id
                        && *scope == generation
                })
            && let Some(discovered) = self.cloudmatch.discovered_session(id)
        {
            return Ok(discovered);
        }
        if allow_discovered && let Some(cleanup) = self.cloudmatch.cleanup_session(session, id) {
            return Ok(cleanup);
        }
        Err(session_owner_error())
    }

    pub fn prepare_owned_stream(
        &self,
        params: &Value,
        prepare: impl FnOnce(&Value) -> Result<Value, ServiceError>,
    ) -> Result<Value, ServiceError> {
        let mut routing = crate::store_requests::lock(&self.session_routing)?;
        let _operation = crate::store_requests::lock(&self.auth_operation)?;
        let (session, generation) = self.session_snapshot_locked()?;
        let owned =
            self.owned_session_params(&params["session"], &routing, &session, generation, false)?;
        if !matches!(owned["status"].as_i64(), Some(2 | 3)) {
            return Err(ServiceError {
                code: "session_not_ready",
                message: "The owned session is not ready for media attachment".into(),
            });
        }
        if !owned["rtspsEndpoints"].as_array().is_some_and(|endpoints| {
            endpoints.iter().any(|endpoint| {
                endpoint
                    .as_str()
                    .is_some_and(|endpoint| endpoint.starts_with("rtsps://"))
            })
        }) {
            return Err(ServiceError {
                code: "session_endpoint_missing",
                message: "The owned session has no RTSPS media endpoint".into(),
            });
        }
        let mut params = params.clone();
        params["session"] =
            scoped_result(json!({"session":owned}), &session, generation)["session"].clone();
        let mut result = prepare(&params)?;
        result["session"] = params["session"].clone();
        self.publish_active_result(&mut routing, &session, generation, result)
    }

    fn scoped_session_route(
        &self,
        params: &Value,
        settings: &Value,
        session: &AuthSession,
    ) -> Result<(Value, Value), ServiceError> {
        let state = self.state.lock().expect("GFN state poisoned");
        if state.providers_expires.is_some()
            && !state
                .providers
                .iter()
                .any(|provider| provider.idp_id == session.provider.idp_id)
        {
            return Err(ServiceError {
                code: "provider_unavailable",
                message: "The selected provider is unavailable".into(),
            });
        }
        drop(state);
        if session.provider.idp_id == DEFAULT_IDP_ID
            && session.provider.code.eq_ignore_ascii_case("NVIDIA")
            && let Some(zone) = params["zone"].as_str()
            && let Some(expected_url) = crate::queue_servers::zone_url(zone)
            && params["streamingBaseUrl"].as_str() == Some(expected_url.as_str())
        {
            let mut settings = settings.clone();
            settings["region"] = json!("");
            return Ok((params.clone(), settings));
        }
        let mut params = params.clone();
        let mut settings = settings.clone();
        let selected = params["streamingBaseUrl"]
            .as_str()
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| {
                settings["providerRegions"][&session.provider.idp_id]
                    .as_str()
                    .map(ToOwned::to_owned)
            })
            .or_else(|| {
                (settings["regionProviderIdpId"] == session.provider.idp_id
                    || (settings["regionProviderIdpId"]
                        .as_str()
                        .unwrap_or("")
                        .is_empty()
                        && session.provider.idp_id == DEFAULT_IDP_ID))
                    .then(|| settings["region"].as_str().unwrap_or("").to_owned())
            });
        if let Some(params) = params.as_object_mut() {
            params.remove("streamingBaseUrl");
            params.remove("zone");
        }
        settings["region"] = json!("");
        if let Some(selected) = selected.filter(|value| !value.is_empty()) {
            let client =
                client_for_settings(&self.client, &settings).map_err(ServiceError::invalid)?;
            match self.provider_regions(&client, session) {
                Ok(result) => {
                    if let Some(region) = result["regions"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find(|region| region["url"] == selected || region["name"] == selected)
                    {
                        params["streamingBaseUrl"] = region["url"].clone();
                        params["zone"] = region["name"].clone();
                    }
                }
                Err(error) if matches!(error.code, "http_unauthorized" | "cancelled") => {
                    return Err(error);
                }
                Err(_) => {}
            }
        }
        Ok((params, settings))
    }

    pub fn account_connections(&self, settings: &Value) -> Result<Value, ServiceError> {
        self.authenticated_read(|session, generation| {
            self.with_account_context(session, generation, settings, |context| {
                self.account_connections.list(context)
            })
        })
    }

    fn with_account_context(
        &self,
        session: &AuthSession,
        generation: u64,
        settings: &Value,
        operation: impl FnOnce(
            &crate::account_connections::AccountContext<'_>,
        ) -> Result<Value, ServiceError>,
    ) -> Result<Value, ServiceError> {
        let definitions = self.definitions_for(session, generation, &json!({}), settings)?;
        let client = client_for_settings(&self.client, settings).map_err(ServiceError::invalid)?;
        let check = || self.check_scope(session, generation);
        check()?;
        operation(&crate::account_connections::AccountContext {
            client: &client,
            auth: session,
            generation,
            graphql: &self.endpoints.graphql,
            als: &self.endpoints.account_linking,
            definitions: &definitions,
            requests: &self.store_cache.requests,
            check: &check,
        })
    }

    pub fn sync_account_connection(
        &self,
        params: &Value,
        settings: &Value,
    ) -> Result<Value, ServiceError> {
        self.providers()?;
        let (session, generation) = self.authenticated_snapshot(TokenPurpose::ServiceId, false)?;
        let result = self.with_account_context(&session, generation, settings, |context| {
            self.account_connections.sync(params, context)
        })?;
        self.check_scope(&session, generation)?;
        Ok(scoped_result(result, &session, generation))
    }

    pub fn account_sync_status(
        &self,
        params: &Value,
        settings: &Value,
    ) -> Result<Value, ServiceError> {
        self.authenticated_read(|session, generation| {
            let mut result =
                self.with_account_context(session, generation, settings, |context| {
                    self.account_connections.sync_status(params, context)
                })?;
            if result["phase"] == "refreshing_library" {
                let id = result["operationId"].as_str().unwrap_or("");
                self.account_connections
                    .invalidate_sync(id, || self.invalidate_catalog())?;
                result["catalogRevision"] = json!(
                    self.catalog_revision
                        .load(std::sync::atomic::Ordering::Acquire)
                );
            }
            Ok(result)
        })
    }

    pub fn unlink_account_connection(
        &self,
        params: &Value,
        settings: &Value,
    ) -> Result<Value, ServiceError> {
        self.providers()?;
        let (session, generation) = self.authenticated_snapshot(TokenPurpose::ServiceId, false)?;
        let result = self.with_account_context(&session, generation, settings, |context| {
            self.account_connections.unlink(params, context)
        })?;
        self.check_scope(&session, generation)?;
        self.invalidate_catalog()?;
        Ok(scoped_result(result, &session, generation))
    }

    pub fn start_account_link(
        &self,
        params: &Value,
        settings: &Value,
    ) -> Result<Value, ServiceError> {
        self.providers()?;
        let (session, generation) = self.authenticated_snapshot(TokenPurpose::ServiceId, false)?;
        let result = self.with_account_context(&session, generation, settings, |context| {
            self.account_connections.start_link(params, context)
        })?;
        self.check_scope(&session, generation)?;
        Ok(scoped_result(result, &session, generation))
    }

    pub fn poll_account_link(
        &self,
        params: &Value,
        settings: &Value,
    ) -> Result<Value, ServiceError> {
        self.authenticated_read(|session, generation| {
            let result = self.with_account_context(session, generation, settings, |context| {
                self.account_connections.poll_link(params, context)
            })?;
            if result["status"] == "complete" {
                self.invalidate_catalog()?;
            }
            Ok(result)
        })
    }

    pub fn persistent_storage_locations(&self, params: &Value) -> Result<Value, ServiceError> {
        self.authenticated_read(|session, _| self.persistent_storage.locations(params, session))
    }

    pub fn reset_persistent_storage(&self, params: &Value) -> Result<Value, ServiceError> {
        let _operation = crate::store_requests::lock(&self.auth_operation)?;
        let (session, generation) = self.session_snapshot_locked()?;
        self.persistent_storage
            .reset(params, &session)
            .map(|result| scoped_result(result, &session, generation))
    }

    pub(crate) fn authenticated_snapshot(
        &self,
        purpose: TokenPurpose,
        force: bool,
    ) -> Result<(AuthSession, u64), ServiceError> {
        let _operation = self
            .auth_operation
            .lock()
            .expect("GFN auth operation poisoned");
        crate::requests::check()?;
        let (session, _) = self.resolve_session_locked(purpose, force)?;
        session
            .map(|mut session| {
                let state = self.state.lock().expect("GFN state poisoned");
                if state.providers_expires.is_some() {
                    let provider = state
                        .providers
                        .iter()
                        .find(|provider| provider.idp_id == session.provider.idp_id)
                        .ok_or_else(|| ServiceError {
                            code: "provider_unavailable",
                            message:
                                "The signed-in provider is no longer in the provider directory"
                                    .into(),
                        })?;
                    session.provider = provider.clone();
                }
                provider_streaming_base(&session.provider)?;
                Ok((session, state.generation))
            })
            .ok_or_else(|| ServiceError {
                code: "authentication_required",
                message: "Sign in to renew authentication".into(),
            })?
    }

    fn check_scope(&self, session: &AuthSession, generation: u64) -> Result<(), ServiceError> {
        crate::requests::check()?;
        let state = self.state.lock().expect("GFN state poisoned");
        if state.generation != generation
            || !state.session.as_ref().is_some_and(|current| {
                current.user.user_id == session.user.user_id
                    && current.provider.idp_id == session.provider.idp_id
            })
        {
            return Err(ServiceError {
                code: "stale_account",
                message: "The account or provider changed. Retry this request.".into(),
            });
        }
        Ok(())
    }

    fn authenticated_snapshot_for(
        &self,
        owner: &AuthSession,
        generation: u64,
        purpose: TokenPurpose,
    ) -> Result<AuthSession, ServiceError> {
        self.check_scope(owner, generation)?;
        let (current, current_generation) = self.authenticated_snapshot(purpose, false)?;
        if current_generation != generation
            || current.user.user_id != owner.user.user_id
            || current.provider.idp_id != owner.provider.idp_id
        {
            return Err(ServiceError {
                code: "stale_account",
                message: "The account or provider changed. Retry this request.".into(),
            });
        }
        self.check_scope(owner, generation)?;
        Ok(current)
    }

    fn authenticated_read(
        &self,
        mut read: impl FnMut(&AuthSession, u64) -> Result<Value, ServiceError>,
    ) -> Result<Value, ServiceError> {
        self.providers()?;
        let (session, generation) = self.authenticated_snapshot(TokenPurpose::ServiceId, false)?;
        self.check_scope(&session, generation)?;
        let result = read(&session, generation);
        self.check_scope(&session, generation)?;
        if result
            .as_ref()
            .is_err_and(|error| error.code == "http_unauthorized")
        {
            let (renewed, renewed_generation) =
                self.authenticated_snapshot(TokenPurpose::ServiceId, true)?;
            self.check_scope(&session, generation)?;
            if renewed_generation != generation
                || renewed.provider.idp_id != session.provider.idp_id
                || renewed.user.user_id != session.user.user_id
            {
                return Err(ServiceError {
                    code: "stale_account",
                    message: "The account changed during authentication renewal".into(),
                });
            }
            let result = read(&renewed, generation);
            self.check_scope(&renewed, generation)?;
            return result.map(|result| scoped_result(result, &renewed, generation));
        }
        result.map(|result| scoped_result(result, &session, generation))
    }

    pub(crate) fn auth_generation(&self) -> u64 {
        self.state.lock().expect("GFN state poisoned").generation
    }

    pub(crate) fn push_scope(&self) -> Option<opennow_core::push::PushScope> {
        let state = self.state.lock().expect("GFN state poisoned");
        let session = state.session.as_ref()?;
        Some(opennow_core::push::PushScope {
            user_id: session.user.user_id.clone(),
            provider_id: session.provider.idp_id.clone(),
            generation: state.generation,
        })
    }

    pub(crate) fn push_token_for_scope(
        &self,
        expected: &opennow_core::push::PushScope,
    ) -> Option<String> {
        let (session, generation) = self
            .authenticated_snapshot(TokenPurpose::ServiceId, false)
            .ok()?;
        if session.user.user_id != expected.user_id
            || session.provider.idp_id != expected.provider_id
            || generation != expected.generation
        {
            return None;
        }
        Some(session.tokens.service_token().to_owned())
    }

    fn vpc_id(
        &self,
        client: &Client,
        session: &AuthSession,
        generation: u64,
        settings: &Value,
        token: &str,
        requests: Option<&crate::store_requests::StoreRequests>,
    ) -> Result<String, ServiceError> {
        self.check_scope(session, generation)?;
        let base = provider_streaming_base(&session.provider)?;
        let url = self.server_info_url(&base)?;
        let headers = lcars_headers(token, "NATIVE", "NVIDIA-CLASSIC", false)?;
        self.server_vpc_cache.resolve(
            &json!([
                base.as_str(),
                generation,
                config_from_settings(settings)
                    .map_err(ServiceError::invalid)?
                    .map(|proxy| proxy.cache_scope)
            ])
            .to_string(),
            &session.user.user_id,
            token,
            || {
                self.check_scope(session, generation)?;
                let request = client.get(url).headers(headers);
                let response = match requests {
                    Some(requests) => requests.send(request, "Store server info failed"),
                    None => request
                        .send()
                        .map_err(|error| ServiceError::network("Server info failed", error)),
                };
                let response = response?;
                if !response.status().is_success() {
                    return Err(ServiceError::response("Server info failed", response));
                }
                let payload = response
                    .json::<Value>()
                    .map_err(|error| ServiceError::network("Invalid server info", error))?;
                self.check_scope(session, generation)?;
                verified_vpc(&payload).map(Some)
            },
        )
    }

    fn server_info_url(&self, base: &url::Url) -> Result<url::Url, ServiceError> {
        #[cfg(test)]
        if let Some(url) = &self.endpoints.server_info {
            return url::Url::parse(url).map_err(|_| ServiceError::invalid("Invalid fixture URL"));
        }
        base.join("v2/serverInfo")
            .map_err(|_| ServiceError::invalid("Invalid provider URL"))
    }

    fn fetch_user_info(&self, tokens: &AuthTokens) -> Result<AuthUser, ServiceError> {
        if let Some(user) = tokens
            .id_token
            .as_deref()
            .or(Some(tokens.access_token.as_str()))
            .and_then(user_from_jwt)
        {
            if user.email.is_some() || user.avatar_url.is_some() {
                return Ok(user);
            }
        }
        let response = self
            .client
            .get(&self.endpoints.userinfo)
            .timeout(Duration::from_secs(5))
            .header(ACCEPT, "application/json")
            .header(AUTHORIZATION, format!("Bearer {}", tokens.access_token))
            .header(ORIGIN, NVIDIA_FILE_ORIGIN)
            .header(USER_AGENT, STEAM_DECK_USER_AGENT)
            .send()
            .map_err(|error| ServiceError::network("User info failed", error))?;
        if !response.status().is_success() {
            return Err(ServiceError::response("User info failed", response));
        }
        let payload = bounded_auth_response(response)?;
        let user_id = required_string(&payload, "sub")?;
        let email = payload["email"].as_str().map(ToOwned::to_owned);
        let display_name = payload["preferred_username"]
            .as_str()
            .map(ToOwned::to_owned)
            .or_else(|| {
                email
                    .as_ref()
                    .and_then(|value| value.split('@').next().map(ToOwned::to_owned))
            })
            .unwrap_or_else(|| "User".to_owned());
        let avatar_url = payload["picture"]
            .as_str()
            .map(ToOwned::to_owned)
            .or_else(|| email.as_deref().map(|value| gravatar_url(value, 80)));
        Ok(AuthUser {
            user_id,
            display_name,
            email,
            avatar_url,
            membership_tier: "FREE".to_owned(),
        })
    }

    fn ensure_client_token(&self, mut tokens: AuthTokens) -> Result<AuthTokens, ServiceError> {
        if tokens.expires_at <= now_ms() {
            return Ok(tokens);
        }
        if tokens.client_token.is_some()
            && tokens
                .client_token_expires_at
                .is_some_and(|expiry| expiry > now_ms() + CLIENT_TOKEN_REFRESH_WINDOW_MS)
        {
            return Ok(tokens);
        }
        let response = self
            .client
            .get(&self.endpoints.client_token)
            .timeout(Duration::from_secs(5))
            .header(ACCEPT, "application/json, text/plain, */*")
            .header(AUTHORIZATION, format!("Bearer {}", tokens.access_token))
            .header(ORIGIN, "https://play.geforcenow.com")
            .header(REFERER, "https://play.geforcenow.com/")
            .header(USER_AGENT, STEAM_DECK_USER_AGENT)
            .send()
            .map_err(|error| ServiceError::network("Client token request failed", error))?;
        if !response.status().is_success() {
            return Err(ServiceError::response(
                "Client token request failed",
                response,
            ));
        }
        let payload = bounded_auth_response(response)?;
        let client_token = required_string(&payload, "client_token")?;
        let lifetime = token_lifetime(&payload).unwrap_or(0);
        tokens.client_token = Some(client_token);
        tokens.client_token_expires_at = Some(now_ms() + lifetime);
        tokens.client_token_lifetime_ms = Some(lifetime);
        Ok(tokens)
    }

    fn update_client_token(&self, session: &AuthSession) -> Result<AuthSession, ServiceError> {
        let tokens = self.ensure_client_token(session.tokens.clone())?;
        let updated = AuthSession {
            provider: session.provider.clone(),
            tokens,
            user: session.user.clone(),
        };
        self.store_refreshed_session(updated)
    }

    fn refresh_session(&self, session: &AuthSession) -> Result<AuthSession, ServiceError> {
        let mut errors = Vec::new();
        if let Some(client_token) = session.tokens.client_token.as_deref().filter(|_| {
            session
                .tokens
                .client_token_expires_at
                .is_some_and(|expiry| expiry > now_ms())
        }) {
            let form = [
                (
                    "grant_type",
                    "urn:ietf:params:oauth:grant-type:client_token",
                ),
                ("client_token", client_token),
                ("client_id", session.tokens.auth_client_id.as_str()),
                ("sub", session.user.user_id.as_str()),
            ];
            match self.token_refresh_request(&form, "Client-token refresh failed") {
                Ok(payload) => return self.finish_token_refresh(session, &payload),
                Err(error) => errors.push(error),
            }
        }
        if let Some(refresh_token) = session.tokens.refresh_token.as_deref() {
            let form = [
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
                ("client_id", session.tokens.auth_client_id.as_str()),
            ];
            match self.token_refresh_request(&form, "Refresh-token exchange failed") {
                Ok(payload) => return self.finish_token_refresh(session, &payload),
                Err(error) => errors.push(error),
            }
        }
        Err(ServiceError {
            code: if !errors.is_empty() && errors.iter().all(is_definitive_auth_revocation) {
                "session_revoked"
            } else {
                "session_refresh_failed"
            },
            message: if errors.is_empty() {
                "Session has no refresh mechanism".to_owned()
            } else {
                errors
                    .into_iter()
                    .map(|error| error.message)
                    .collect::<Vec<_>>()
                    .join(" | ")
            },
        })
    }

    fn token_refresh_request(
        &self,
        form: &[(&str, &str)],
        context: &str,
    ) -> Result<Value, ServiceError> {
        let response = self
            .client
            .post(&self.endpoints.token)
            .timeout(Duration::from_secs(5))
            .header(ACCEPT, "application/json, text/plain, */*")
            .header(
                CONTENT_TYPE,
                "application/x-www-form-urlencoded; charset=UTF-8",
            )
            .header(ORIGIN, "https://play.geforcenow.com")
            .header(REFERER, "https://play.geforcenow.com/")
            .header(
                USER_AGENT,
                if form
                    .iter()
                    .any(|(key, value)| *key == "client_id" && *value == STEAM_DECK_CLIENT_ID)
                {
                    STEAM_DECK_USER_AGENT
                } else {
                    GFN_USER_AGENT
                },
            )
            .form(form)
            .send()
            .map_err(|error| ServiceError::network(context, error))?;
        if !response.status().is_success() {
            return Err(ServiceError::response(context, response));
        }
        bounded_auth_response(response)
    }

    fn finish_token_refresh(
        &self,
        session: &AuthSession,
        payload: &Value,
    ) -> Result<AuthSession, ServiceError> {
        let access_token = required_string(payload, "access_token")?;
        let mut tokens = AuthTokens {
            access_token,
            refresh_token: payload["refresh_token"]
                .as_str()
                .map(ToOwned::to_owned)
                .or_else(|| session.tokens.refresh_token.clone()),
            id_token: payload["id_token"]
                .as_str()
                .map(ToOwned::to_owned)
                .or_else(|| session.tokens.id_token.clone()),
            id_token_expires_at: if let Some(token) = payload["id_token"].as_str() {
                jwt_expiry(token)
            } else {
                session
                    .tokens
                    .id_token_expires_at
                    .or_else(|| session.tokens.id_token.as_deref().and_then(jwt_expiry))
            },
            expires_at: token_expiry(payload),
            auth_client_id: session.tokens.auth_client_id.clone(),
            client_token: payload["client_token"]
                .as_str()
                .map(ToOwned::to_owned)
                .or_else(|| session.tokens.client_token.clone()),
            client_token_expires_at: session.tokens.client_token_expires_at,
            client_token_lifetime_ms: session.tokens.client_token_lifetime_ms,
        };
        if payload["client_token"]
            .as_str()
            .is_some_and(|value| Some(value) != session.tokens.client_token.as_deref())
        {
            tokens.client_token_expires_at = None;
            tokens.client_token_lifetime_ms = None;
        }
        tokens = self.ensure_client_token(tokens.clone()).unwrap_or(tokens);
        let user = self
            .fetch_user_info(&tokens)
            .unwrap_or_else(|_| session.user.clone());
        if user.user_id != session.user.user_id {
            return Err(ServiceError {
                code: "session_identity_mismatch",
                message: "Refreshed token belongs to a different account".to_owned(),
            });
        }
        self.store_refreshed_session(AuthSession {
            provider: session.provider.clone(),
            tokens,
            user,
        })
    }

    fn store_refreshed_session(&self, session: AuthSession) -> Result<AuthSession, ServiceError> {
        let persist = {
            let state = self.state.lock().expect("GFN state poisoned");
            state.persistence_intent == PersistenceIntent::SecureStore
                && self.device_identity_error.is_none()
        };
        let persistence = if persist {
            match self.vault.save(&session) {
                Ok(()) => "secure-store",
                Err(error) => {
                    eprintln!("auth: refreshed session remains memory-only: {error}");
                    "memory-only"
                }
            }
        } else {
            "memory-only"
        };
        let mut state = self.state.lock().expect("GFN state poisoned");
        state.session = Some(session.clone());
        state.persistence_state = persistence.to_owned();
        state.refresh_retry_at = None;
        Ok(session)
    }

    fn prune_attempts(&self) {
        let now = now_ms();
        self.state
            .lock()
            .expect("GFN state poisoned")
            .attempts
            .retain(|_, attempt| attempt.expires_at > now && attempt.deadline > Instant::now());
    }
}

struct DevicePoll<'a> {
    service: &'a GfnService,
    attempt_id: &'a str,
}

impl Drop for DevicePoll<'_> {
    fn drop(&mut self) {
        if let Some(attempt) = self
            .service
            .state
            .lock()
            .expect("GFN state poisoned")
            .attempts
            .get_mut(self.attempt_id)
        {
            attempt.in_flight = false;
            attempt.next_poll = Instant::now() + Duration::from_secs(attempt.interval_seconds);
        }
    }
}

fn bounded_auth_response(response: Response) -> Result<Value, ServiceError> {
    let mut bytes = Vec::new();
    response
        .take(256 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ServiceError::invalid("Authentication response could not be read"))?;
    if bytes.len() > 256 * 1024 {
        return Err(ServiceError::invalid(
            "Authentication response exceeds size limit",
        ));
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| ServiceError::invalid("Authentication response is invalid"))
}

fn token_lifetime(payload: &Value) -> Option<u64> {
    payload["expires_in"]
        .as_u64()
        .filter(|seconds| *seconds > 0 && *seconds <= 366 * 24 * 3600)?
        .checked_mul(1000)
}

fn token_expiry(payload: &Value) -> u64 {
    token_lifetime(payload)
        .and_then(|duration| now_ms().checked_add(duration))
        .unwrap_or(0)
}

fn jwt_expiry(token: &str) -> Option<u64> {
    if token.len() > 64 * 1024 {
        return None;
    }
    let mut parts = token.split('.');
    parts.next()?;
    let payload = parts.next()?;
    parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    serde_json::from_slice::<Value>(&decoded).ok()?["exp"]
        .as_u64()?
        .checked_mul(1000)
}

fn is_definitive_auth_revocation(error: &ServiceError) -> bool {
    if error.code == "session_revoked" {
        return true;
    }
    if !matches!(error.code, "upstream_error" | "http_unauthorized") {
        return false;
    }
    let message = error.message.to_ascii_lowercase();
    message.contains("invalid_grant")
        || message.contains("invalid_token")
        || message.contains("token_revoked")
        || message.contains("revoked")
}

fn parse_providers(payload: &Value) -> Vec<LoginProvider> {
    let mut providers = payload["gfnServiceInfo"]["gfnServiceEndpoints"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let code = entry["loginProviderCode"].as_str()?;
            Some(
                LoginProvider {
                    idp_id: entry["idpId"].as_str()?.to_owned(),
                    code: code.to_owned(),
                    display_name: if code == "BPC" {
                        "bro.game"
                    } else {
                        entry["loginProviderDisplayName"].as_str()?
                    }
                    .to_owned(),
                    streaming_service_url: entry["streamingServiceUrl"].as_str()?.to_owned(),
                    priority: entry["loginProviderPriority"].as_i64().unwrap_or(0),
                }
                .normalize(),
            )
        })
        .collect::<Vec<_>>();
    providers.sort_by_key(|provider| provider.priority);
    providers
}

fn public_game_to_info(item: &Value) -> Option<Value> {
    if item["status"].as_str()? != "AVAILABLE" {
        return None;
    }
    let title = item["title"].as_str()?.trim();
    if title.is_empty() {
        return None;
    }
    let source_id = item["id"]
        .as_str()
        .map(ToOwned::to_owned)
        .or_else(|| item["id"].as_i64().map(|value| value.to_string()))
        .unwrap_or_else(|| title.to_owned());
    let steam_id = item["steamUrl"]
        .as_str()
        .and_then(|url| url.split("/app/").nth(1))
        .and_then(|tail| tail.split('/').next())
        .filter(|value| value.chars().all(|character| character.is_ascii_digit()))
        .map(ToOwned::to_owned);
    let id = steam_id.clone().unwrap_or_else(|| source_id.clone());
    let store = item["store"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            item["publisher"]
                .as_str()
                .filter(|value| value.to_lowercase().contains("ncsoft"))
                .map(|_| "NCSoft".to_owned())
        })
        .unwrap_or_else(|| "Unknown".to_owned());
    let image_url = steam_id.as_ref().map(|value| {
        format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{value}/header.jpg")
    });
    let hero_image_url = steam_id.as_ref().map(|value| {
        format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{value}/library_hero.jpg")
    });
    let publisher = item["publisher"].as_str().unwrap_or("");
    Some(json!({
        "id":id,
        "uuid":source_id,
        "launchAppId":if id.chars().all(|character| character.is_ascii_digit()) { Some(id.clone()) } else { None },
        "title":title,
        "searchText":format!("{title} {store} {publisher}").to_lowercase(),
        "selectedVariantIndex":0,
        "variants":[{"id":id, "store":store, "supportedControls":[]}],
        "imageUrl":image_url,
        "heroImageUrl":hero_image_url,
        "availableStores":[store],
        "isInLibrary":false,
    }))
}

fn gfn_feature_enabled(features: &Value, expected_key: &str) -> bool {
    let matches = |feature: &Value| {
        feature["key"].as_str() == Some(expected_key)
            && (feature["value"].as_bool() == Some(true)
                || feature["value"]
                    .as_str()
                    .is_some_and(|value| value.eq_ignore_ascii_case("true")))
    };
    features
        .as_array()
        .is_some_and(|features| features.iter().any(matches))
        || features.as_object().is_some_and(|_| matches(features))
}

fn image_values(value: &Value, width: u32) -> Vec<String> {
    let values = if let Some(items) = value.as_array() {
        items.iter().filter_map(Value::as_str).collect::<Vec<_>>()
    } else {
        value.as_str().into_iter().collect()
    };
    values
        .into_iter()
        .filter_map(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else if trimmed.contains("img.nvidiagrid.net") {
                Some(format!("{trimmed};f=jpg;w={width}"))
            } else {
                Some(trimmed.to_owned())
            }
        })
        .collect()
}

fn first_image(images: &Value, keys: &[&str], width: u32) -> Option<String> {
    keys.iter()
        .find_map(|key| image_values(&images[*key], width).into_iter().next())
}

fn string_array(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            item.as_str().map(ToOwned::to_owned).or_else(|| {
                ["name", "label", "title", "displayName"]
                    .iter()
                    .find_map(|key| item[*key].as_str().map(ToOwned::to_owned))
            })
        })
        .collect()
}

fn graphql_error_message(payload: &Value) -> Option<String> {
    let messages = payload["errors"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|error| error["message"].as_str())
        .collect::<Vec<_>>();
    (!messages.is_empty()).then(|| messages.join(", "))
}

fn game_identity(value: &Value) -> Option<String> {
    for key in ["uuid", "id", "launchAppId"] {
        if let Some(id) = value[key].as_str().filter(|id| !id.is_empty()) {
            return Some(id.to_owned());
        }
    }
    None
}

/// GETs a CMS panels document (persisted query with full-text fallback,
/// mirroring Electron's fetchLcarsGraphQl). Used for the storefront
/// marquee hero and the official Main shelves.
fn fetch_panels_document(
    requests: &crate::store_requests::StoreRequests,
    client: &Client,
    token: &str,
    variables: Value,
    request_type: &str,
    sha: &str,
    fallback_query: &str,
) -> Result<Value, ServiceError> {
    let context = "GFN storefront query";
    crate::requests::check()?;
    let extensions = json!({"persistedQuery":{"sha256Hash":sha}}).to_string();
    let variables_text = variables.to_string();
    let hu_id = random_attempt_id();
    let mut url = url::Url::parse(GRAPHQL_URL).expect("GraphQL URL is valid");
    url.query_pairs_mut()
        .append_pair("extensions", &extensions)
        .append_pair("huId", &hu_id)
        .append_pair("variables", &variables_text)
        .append_pair("requestType", request_type);
    let mut headers = graphql_headers(token)?;
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("application/graphql"),
    );
    let response = requests.send(
        client.get(url.clone()).headers(headers),
        &format!("{context} failed"),
    )?;
    let payload = if response.status().as_u16() == 400 {
        crate::requests::check()?;
        url.query_pairs_mut().append_pair("query", fallback_query);
        let mut retry_headers = graphql_headers(token)?;
        retry_headers.insert(
            reqwest::header::CONTENT_TYPE,
            HeaderValue::from_static("application/graphql"),
        );
        let response = requests.send(
            client.get(url).headers(retry_headers),
            &format!("{context} failed"),
        )?;
        if !response.status().is_success() {
            return Err(ServiceError::response(
                &format!("{context} failed"),
                response,
            ));
        }
        response
            .json::<Value>()
            .map_err(|error| ServiceError::network(&format!("Invalid {context} response"), error))?
    } else {
        if !response.status().is_success() {
            return Err(ServiceError::response(
                &format!("{context} failed"),
                response,
            ));
        }
        response
            .json::<Value>()
            .map_err(|error| ServiceError::network(&format!("Invalid {context} response"), error))?
    };
    if let Some(message) = graphql_error_message(&payload) {
        return Err(ServiceError {
            code: "graphql_error",
            message,
        });
    }
    Ok(payload)
}

fn marquee_hero_image(item: &Value) -> Option<String> {
    first_image(&item["images"], &["MARQUEE_HERO_IMAGE", "HERO_IMAGE"], 1600)
}

fn parse_store_marquee(payload: &Value, browse_by_id: &HashMap<String, Value>) -> Vec<Value> {
    let mut slides = Vec::new();
    let panels = payload["data"]["panels"].as_array().into_iter().flatten();
    for panel in panels {
        let sections = panel["sections"].as_array().into_iter().flatten();
        for section in sections {
            let items = section["items"].as_array().into_iter().flatten();
            for item in items {
                if slides.len() >= 8 {
                    return slides;
                }
                match item["__typename"].as_str().unwrap_or("") {
                    "MarketingItem" => {
                        let title = item["title"].as_str().unwrap_or("").trim();
                        if title.is_empty() {
                            continue;
                        }
                        slides.push(json!({
                            "kind":"marketing",
                            "title":title,
                            "body":item["body"].as_str().unwrap_or(""),
                            "image":marquee_hero_image(item),
                            "actionLabel":item["action"]["label"].as_str().unwrap_or(""),
                            "actionUri":item["action"]["uri"].as_str().unwrap_or(""),
                        }));
                    }
                    "GameItem" => {
                        let Some(game) = app_to_game(&item["app"]).map(|mut game| {
                            if game["heroImageUrl"].is_null() {
                                if let Some(art) = marquee_hero_image(&item["app"]) {
                                    game["heroImageUrl"] = Value::String(art);
                                }
                            }
                            game
                        }) else {
                            continue;
                        };
                        let identity = game_identity(&game);
                        let resolved = identity
                            .as_ref()
                            .and_then(|id| browse_by_id.get(id))
                            .cloned()
                            .unwrap_or(game);
                        let title = resolved["title"].as_str().unwrap_or("").to_owned();
                        if title.is_empty() {
                            continue;
                        }
                        slides.push(json!({
                            "kind":"game",
                            "title":title,
                            "body":resolved["publisherName"].as_str().unwrap_or(""),
                            "image":marquee_hero_image(&item["app"]),
                            "game":resolved,
                        }));
                    }
                    _ => {}
                }
            }
        }
    }
    slides
}

fn parse_store_panels(payload: &Value, browse_by_id: &HashMap<String, Value>) -> Vec<Value> {
    let mut panels = Vec::new();
    let incoming = payload["data"]["panels"].as_array().into_iter().flatten();
    for panel in incoming {
        let mut sections = Vec::new();
        let panel_sections = panel["sections"].as_array().into_iter().flatten();
        for section in panel_sections {
            let title = section["title"].as_str().unwrap_or("").trim().to_owned();
            let mut games = Vec::new();
            let items = section["items"].as_array().into_iter().flatten();
            for item in items {
                if item["__typename"].as_str() != Some("GameItem") {
                    continue;
                }
                let Some(game) = app_to_game(&item["app"]) else {
                    continue;
                };
                if game["id"].as_str().unwrap_or("").is_empty()
                    || game["title"].as_str().unwrap_or("").is_empty()
                    || game["variants"]
                        .as_array()
                        .is_none_or(|variants| variants.is_empty())
                {
                    continue;
                }
                let resolved = game_identity(&game)
                    .as_ref()
                    .and_then(|id| browse_by_id.get(id))
                    .cloned()
                    .unwrap_or(game);
                if games.len() < 24
                    && !games
                        .iter()
                        .any(|existing: &Value| game_identity(existing) == game_identity(&resolved))
                {
                    games.push(resolved);
                }
            }
            if title.is_empty() || games.is_empty() {
                continue;
            }
            sections.push(json!({
                "id":section["id"].as_str().unwrap_or(&title),
                "title":title,
                "games":games,
            }));
        }
        if sections.is_empty() {
            continue;
        }
        panels.push(json!({
            "id":panel["id"].as_str().or_else(|| panel["name"].as_str()).unwrap_or(""),
            "title":panel["name"].as_str().unwrap_or(""),
            "sections":sections,
        }));
    }
    panels
}

fn parse_store_definitions(payload: &Value) -> Vec<Value> {
    payload["data"]["filterGroupDefinitions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|group| {
            let options = group["filters"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|entry| {
                    Some(json!({
                        "id":entry["id"].as_str()?,
                        "label":entry["label"].as_str().unwrap_or(entry["id"].as_str()?),
                        "filters":entry["filters"],
                        "expression":crate::catalog_types::filter_expression(&entry["filters"]),
                    }))
                })
                .collect::<Vec<_>>();
            if options.is_empty() {
                return None;
            }
            Some(json!({
                "id":group["id"].as_str()?,
                "label":group["label"].as_str().unwrap_or(group["id"].as_str()?),
                "options":options,
            }))
        })
        .collect()
}

fn trusted_streaming_base(value: &str) -> Result<url::Url, ServiceError> {
    crate::cloudmatch::trusted_cloudmatch_base(value)
}

fn provider_result(state: &ServiceState) -> Value {
    let mut providers = state.providers.clone();
    if providers.is_empty() {
        if let Some(session) = &state.session {
            providers.push(session.provider.clone());
        }
        if !providers
            .iter()
            .any(|provider| provider.idp_id == DEFAULT_IDP_ID)
        {
            providers.push(LoginProvider::default_nvidia());
        }
    }
    json!({"providers":providers,"defaultProviderIdpId":state.providers_default,"generation":state.generation,"discovery":{
        "state":if state.providers_error.is_some() {"degraded"} else {"ready"},
        "message":state.providers_error,
        "retryAfterMs":state.providers_retry.map(|time| time.saturating_duration_since(Instant::now()).as_millis() as u64).unwrap_or(0)
    }})
}

fn session_owner_error() -> ServiceError {
    ServiceError { code: "session_owner_mismatch", message: "This session is not owned by the current account and provider. Refresh active sessions.".into() }
}

fn verified_vpc(payload: &Value) -> Result<String, ServiceError> {
    let status = &payload["requestStatus"];
    if status.get("statusCode").is_some() && number_value(&status["statusCode"]) != Some(1.0) {
        return Err(ServiceError {
            code: "invalid_upstream_response",
            message: "Provider server info reported an unsuccessful status".into(),
        });
    }
    status["serverId"]
        .as_str()
        .filter(|value| !value.trim().is_empty() && value.len() <= 256)
        .map(ToOwned::to_owned)
        .ok_or_else(|| ServiceError {
            code: "invalid_upstream_response",
            message: "Provider server info did not include a verified VPC".into(),
        })
}

fn scoped_result(mut result: Value, session: &AuthSession, generation: u64) -> Value {
    result["scope"] = json!({"generation":generation,"providerIdpId":session.provider.idp_id,"userId":session.user.user_id});
    if result["session"].is_object() {
        result["session"]["ownerScope"] = result["scope"].clone();
    }
    result
}

fn number_value(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.trim().parse().ok())
}

fn graphql_headers(token: &str) -> Result<HeaderMap, ServiceError> {
    let mut headers = lcars_headers(token, "NATIVE", "NVIDIA-CLASSIC", false)?;
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
    Ok(headers)
}

fn lcars_headers(
    token: &str,
    client_type: &str,
    streamer: &str,
    steam_deck: bool,
) -> Result<HeaderMap, ServiceError> {
    let mut headers = HeaderMap::new();
    let authorization = HeaderValue::from_str(&format!("GFNJWT {token}"))
        .map_err(|_| ServiceError::invalid("Session token contains invalid header bytes"))?;
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/json, text/plain, */*"),
    );
    headers.insert(AUTHORIZATION, authorization);
    headers.insert("nv-client-id", HeaderValue::from_static(LCARS_CLIENT_ID));
    headers.insert(
        "nv-client-type",
        HeaderValue::from_str(client_type)
            .map_err(|_| ServiceError::invalid("Invalid client type"))?,
    );
    headers.insert(
        "nv-client-version",
        HeaderValue::from_static(GFN_CLIENT_VERSION),
    );
    headers.insert(
        "nv-client-streamer",
        HeaderValue::from_str(streamer)
            .map_err(|_| ServiceError::invalid("Invalid streamer type"))?,
    );
    headers.insert(
        "nv-device-os",
        HeaderValue::from_static(if steam_deck {
            "STEAMOS"
        } else if cfg!(target_os = "windows") {
            "WINDOWS"
        } else if cfg!(target_os = "macos") {
            "MACOS"
        } else {
            "LINUX"
        }),
    );
    // Mirrors Electron's Steam Deck device profile: MES returns the Deck
    // resolution catalog (including 90 FPS tuples) under these headers.
    headers.insert(
        "nv-device-type",
        HeaderValue::from_static(if steam_deck { "CONSOLE" } else { "DESKTOP" }),
    );
    headers.insert(
        "nv-device-make",
        HeaderValue::from_static(if steam_deck { "VALVE" } else { "GENERIC" }),
    );
    headers.insert(
        "nv-device-model",
        HeaderValue::from_static(if steam_deck { "STEAMDECK" } else { "PC" }),
    );
    headers.insert("x-nv-client-identity", HeaderValue::from_static("GFN-PC"));
    headers.insert(USER_AGENT, HeaderValue::from_static(GFN_USER_AGENT));
    Ok(headers)
}

fn user_from_jwt(token: &str) -> Option<AuthUser> {
    let encoded = token.split('.').nth(1)?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded)
        .ok()?;
    let payload = serde_json::from_slice::<Value>(&decoded).ok()?;
    let user_id = payload["sub"].as_str()?.to_owned();
    let email = payload["email"].as_str().map(ToOwned::to_owned);
    let avatar_url = payload["picture"]
        .as_str()
        .map(ToOwned::to_owned)
        .or_else(|| email.as_deref().map(|value| gravatar_url(value, 80)));
    let display_name = payload["preferred_username"]
        .as_str()
        .map(ToOwned::to_owned)
        .or_else(|| {
            email
                .as_ref()
                .and_then(|value| value.split('@').next().map(ToOwned::to_owned))
        })
        .unwrap_or_else(|| "User".to_owned());
    Some(AuthUser {
        user_id,
        display_name,
        email,
        avatar_url,
        membership_tier: payload["gfn_tier"].as_str().unwrap_or("FREE").to_owned(),
    })
}

fn qr_rows(value: &str) -> Vec<String> {
    QrCode::new(value.as_bytes())
        .map(|code| {
            let width = code.width();
            code.to_colors()
                .chunks(width)
                .map(|row| {
                    row.iter()
                        .map(|color| {
                            if matches!(color, qrcode::Color::Dark) {
                                '1'
                            } else {
                                '0'
                            }
                        })
                        .collect()
                })
                .collect()
        })
        .unwrap_or_default()
}

fn stable_device_id() -> String {
    let host = env::var("HOSTNAME").unwrap_or_else(|_| "unknown-host".to_owned());
    let user = env::var("USER")
        .or_else(|_| env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown-user".to_owned());
    let mut hasher = Sha256::new();
    hasher.update(format!("{host}:{user}:opennow-stable"));
    format!("{:x}", hasher.finalize())
}

fn random_attempt_id() -> String {
    use rand::RngCore as _;
    let mut bytes = [0_u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn gravatar_url(email: &str, size: u32) -> String {
    let normalized = email.trim().to_lowercase();
    format!(
        "https://www.gravatar.com/avatar/{:x}?s={size}&d=identicon",
        md5::compute(normalized)
    )
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn required_param<'a>(params: &'a Value, key: &str) -> Result<&'a str, ServiceError> {
    params[key]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ServiceError::invalid(format!("Missing {key}")))
}

fn required_string(payload: &Value, key: &str) -> Result<String, ServiceError> {
    payload[key]
        .as_str()
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| ServiceError {
            code: "invalid_upstream_response",
            message: format!("Response did not include {key}"),
        })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn tokens(id_token: Option<&str>) -> AuthTokens {
        AuthTokens {
            access_token: "access-token".to_owned(),
            refresh_token: None,
            id_token: id_token.map(str::to_owned),
            id_token_expires_at: None,
            expires_at: 0,
            auth_client_id: "client".to_owned(),
            client_token: None,
            client_token_expires_at: None,
            client_token_lifetime_ms: None,
        }
    }

    #[test]
    fn the_service_token_prefers_the_id_token_and_falls_back_to_access() {
        assert_eq!(tokens(Some("id-token")).service_token(), "id-token");
        assert_eq!(tokens(None).service_token(), "access-token");
    }

    fn mock_responses(
        responses: Vec<(u16, Value)>,
        before_response: impl Fn(usize) + Send + 'static,
    ) -> (String, std::thread::JoinHandle<()>) {
        mock_requests(responses, move |index, _| before_response(index))
    }

    pub(crate) fn mock_requests(
        responses: Vec<(u16, Value)>,
        before_response: impl Fn(usize, &str) + Send + 'static,
    ) -> (String, std::thread::JoinHandle<()>) {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let worker = std::thread::spawn(move || {
            for (index, (status, body)) in responses.into_iter().enumerate() {
                let deadline = std::time::Instant::now() + Duration::from_secs(10);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(std::time::Instant::now() < deadline, "missing HTTP request");
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("{error}"),
                    }
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(&mut stream);
                let mut length = 0;
                let mut request = String::new();
                loop {
                    let mut line = String::new();
                    assert!(reader.read_line(&mut line).unwrap() > 0);
                    request.push_str(&line);
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse::<usize>().unwrap();
                    }
                }
                let mut body_bytes = vec![0; length];
                reader.read_exact(&mut body_bytes).unwrap();
                request.push_str(std::str::from_utf8(&body_bytes).unwrap());
                before_response(index, &request);
                let body = body.to_string();
                write!(stream, "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        (url, worker)
    }

    pub(crate) fn auth_fixture(user: &str) -> AuthSession {
        serde_json::from_value(json!({
            "provider": LoginProvider::default_nvidia(),
            "tokens": {"accessToken":"test-access", "refreshToken":"test-refresh",
                "clientToken":"test-client", "expiresAt":now_ms() + 3_600_000,
                "clientTokenExpiresAt":now_ms() + 3_600_000, "authClientId":"test-client-id"},
            "user":{"userId":user,"displayName":"Test","membershipTier":"FREE"}
        }))
        .unwrap()
    }

    pub(super) fn jwt(user: &str, expiry: u64) -> String {
        format!(
            "header.{}.signature",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
                serde_json::to_vec(
                    &json!({"sub":user,"email":"fixture@example.invalid","exp":expiry / 1000})
                )
                .unwrap()
            )
        )
    }

    pub(super) fn pending_attempt(session: Option<AuthSession>) -> DeviceAttempt {
        DeviceAttempt {
            provider: LoginProvider::default_nvidia(),
            device_code: "private-device-sentinel".into(),
            expires_at: now_ms() + 60_000,
            deadline: Instant::now() + Duration::from_secs(60),
            interval_seconds: 7,
            next_poll: Instant::now(),
            in_flight: false,
            pending_session: session,
        }
    }

    fn assert_public_auth(value: &Value) {
        let encoded = value.to_string();
        for private in [
            "accessToken",
            "idToken",
            "refreshToken",
            "clientToken",
            "authClientId",
            "deviceCode",
            "test-access",
            "test-refresh",
            "test-client",
            "private-device-sentinel",
        ] {
            assert!(
                !encoded.contains(private),
                "private field escaped: {private}"
            );
        }
    }

    #[test]
    fn cancelled_authorization_does_not_install_a_late_challenge() {
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let challenge = json!({"device_code":"private-device-sentinel","user_code":"ABCD", "verification_uri":"https://example.invalid", "verification_uri_complete":"https://example.invalid/code", "expires_in":600,"interval":0});
        let (url, server) = mock_responses(vec![(200, challenge)], move |_| {
            entered_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        });
        let (service, path) = test_service(&url);
        let requests = std::sync::Arc::new(crate::requests::Requests::default());
        let permit = requests.admit("start", "auth.device.start").unwrap();
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| {
                crate::requests::scope(permit.token.clone(), || {
                    service.start_device_login(&json!({}))
                })
            });
            entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            requests.cancel("start");
            release_tx.send(()).unwrap();
            assert_eq!(worker.join().unwrap().unwrap_err().code, "cancelled");
        });
        assert!(service.state.lock().unwrap().attempts.is_empty());
        server.join().unwrap();
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn challenge_hides_device_grant_and_defaults_zero_interval() {
        let challenge = json!({"device_code":"private-device-sentinel","user_code":"ABCD", "verification_uri":"https://example.invalid", "verification_uri_complete":"https://example.invalid/code", "expires_in":600,"interval":0});
        let (url, server) = mock_requests(vec![(200, challenge)], |_, request| {
            assert!(request.contains("device_id="));
            assert!(request.contains("client_id="));
        });
        let (service, path) = test_service(&url);
        let result = service.start_device_login(&json!({})).unwrap();
        assert_eq!(result["intervalSeconds"], 5);
        assert_public_auth(&result);
        let state = service.state.lock().unwrap();
        assert_eq!(
            state.attempts[result["attemptId"].as_str().unwrap()].device_code,
            "private-device-sentinel"
        );
        drop(state);
        server.join().unwrap();
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn expiration_during_profile_io_prevents_completion() {
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let (url, server) = mock_responses(
            vec![
                (200, json!({"access_token":"test-access","expires_in":3600})),
                (200, json!({"client_token":"test-client","expires_in":3600})),
                (200, json!({"sub":"user","email":"fixture@example.invalid"})),
            ],
            move |index| {
                if index == 2 {
                    entered_tx.send(()).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                }
            },
        );
        let (service, path) = test_service(&url);
        service
            .state
            .lock()
            .unwrap()
            .attempts
            .insert("login".into(), pending_attempt(None));
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| service.poll_device_login(&json!({"attemptId":"login"})));
            entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            service
                .state
                .lock()
                .unwrap()
                .attempts
                .get_mut("login")
                .unwrap()
                .deadline = Instant::now();
            release_tx.send(()).unwrap();
            assert!(worker.join().unwrap().is_err());
        });
        assert!(
            service
                .complete_device_login(&json!({"attemptId":"login"}))
                .is_err()
        );
        assert!(service.state.lock().unwrap().session.is_none());
        assert!(!path.join("accounts.json").exists());
        server.join().unwrap();
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn public_auth_is_token_free_and_temporary_intent_survives_refresh() {
        let (service, path) = test_service("http://127.0.0.1:1");
        service.vault.save(&auth_fixture("user")).unwrap();
        assert!(!format!("{:?}", auth_fixture("user")).contains("test-access"));
        service
            .state
            .lock()
            .unwrap()
            .attempts
            .insert("login".into(), pending_attempt(Some(auth_fixture("user"))));
        let response = service
            .complete_device_login(&json!({"attemptId":"login","staySignedIn":false}))
            .unwrap();
        assert_public_auth(&response);
        assert_eq!(response["persistence"], "memory-only");
        assert_eq!(response["generation"], 1);
        service
            .store_refreshed_session(auth_fixture("user"))
            .unwrap();
        assert!(service.vault.load("user").unwrap().is_none());
        let refreshed = service.session().unwrap();
        assert_eq!(refreshed["persistence"], "memory-only");
        assert_public_auth(&refreshed);
        assert!(
            service
                .authenticated_snapshot(TokenPurpose::ServiceId, false)
                .map(|(session, _)| session)
                .is_ok()
        );
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn token_expiries_are_independent_and_do_not_invent_lifetimes() {
        assert_eq!(token_expiry(&json!({})), 0);
        assert_eq!(token_expiry(&json!({"expires_in":u64::MAX})), 0);
        assert_eq!(token_expiry(&json!({"expires_in":-1})), 0);
        assert_eq!(jwt_expiry("malformed"), None);
        let mut session = auth_fixture("user");
        session.tokens.id_token = Some(jwt("user", 1000));
        assert_eq!(session.tokens.expiry(TokenPurpose::ServiceId), 1000);
        assert!(session.tokens.expiry(TokenPurpose::StarfleetAccess) > now_ms());
        session.tokens.id_token = Some(jwt("user", now_ms() + 3_600_000));
        session.tokens.expires_at = 0;
        assert_eq!(session.tokens.expiry(TokenPurpose::StarfleetAccess), 0);
        assert!(session.tokens.expiry(TokenPurpose::ServiceId) > now_ms());
    }

    #[test]
    fn expired_id_renews_with_issuing_client_even_when_access_is_valid() {
        let token = jwt("user", now_ms() + 3_600_000);
        let (url, server) = mock_requests(
            vec![(
                200,
                json!({"access_token":"renewed","id_token":token,"expires_in":3600}),
            )],
            |_, request| {
                assert!(request.contains(
                    "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Aclient_token"
                ));
                assert!(request.contains("client_id=test-client-id"));
                assert!(request.contains("sub=user"));
            },
        );
        let (service, path) = test_service(&url);
        let mut session = auth_fixture("user");
        session.tokens.id_token = Some(jwt("user", 1000));
        service.state.lock().unwrap().session = Some(session);
        let session = service
            .authenticated_snapshot(TokenPurpose::ServiceId, false)
            .map(|(session, _)| session)
            .unwrap();
        assert_eq!(session.tokens.access_token, "renewed");
        assert!(session.tokens.expiry(TokenPurpose::ServiceId) > now_ms());
        assert_eq!(session.tokens.auth_client_id, "test-client-id");
        server.join().unwrap();
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn omitted_id_refresh_does_not_extend_old_id_or_hammer_auth() {
        let (url, server) = mock_responses(
            vec![(200, json!({"access_token":"renewed","expires_in":3600}))],
            |_| {},
        );
        let (service, path) = test_service(&url);
        let mut session = auth_fixture("user");
        session.tokens.id_token = Some(jwt("user", 1000));
        service.state.lock().unwrap().session = Some(session);
        assert!(
            service
                .authenticated_snapshot(TokenPurpose::ServiceId, false)
                .map(|(session, _)| session)
                .is_err()
        );
        assert!(
            service
                .authenticated_snapshot(TokenPurpose::ServiceId, false)
                .map(|(session, _)| session)
                .is_err()
        );
        assert!(
            service
                .authenticated_snapshot(TokenPurpose::StarfleetAccess, false)
                .map(|(session, _)| session)
                .is_ok()
        );
        assert_eq!(
            service
                .state
                .lock()
                .unwrap()
                .session
                .as_ref()
                .unwrap()
                .tokens
                .id_token_expires_at,
            Some(1000)
        );
        server.join().unwrap();
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn expired_client_grant_uses_refresh_token_and_transient_errors_are_paced() {
        let (url, server) = mock_requests(
            vec![(503, json!({"error":"temporarily_unavailable"}))],
            |_, request| {
                assert!(request.contains("grant_type=refresh_token"));
                assert!(!request.contains("client_token="));
            },
        );
        let (service, path) = test_service(&url);
        let mut session = auth_fixture("user");
        session.tokens.expires_at = 0;
        session.tokens.client_token_expires_at = Some(1);
        service.vault.save(&session).unwrap();
        service.state.lock().unwrap().session = Some(session);
        assert!(
            service
                .authenticated_snapshot(TokenPurpose::StarfleetAccess, false)
                .map(|(session, _)| session)
                .is_err()
        );
        assert!(
            service
                .authenticated_snapshot(TokenPurpose::StarfleetAccess, false)
                .map(|(session, _)| session)
                .is_err()
        );
        assert!(service.vault.load("user").unwrap().is_some());
        server.join().unwrap();
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn device_slow_down_is_cumulative_and_early_polls_stay_local() {
        let (url, server) = mock_requests(
            vec![
                (
                    400,
                    json!({"error":"slow_down","error_description":"private-device-sentinel"}),
                ),
                (400, json!({"error":"slow_down"})),
            ],
            |_, request| {
                assert!(request.contains("device_code=private-device-sentinel"));
            },
        );
        let (service, path) = test_service(&url);
        service
            .state
            .lock()
            .unwrap()
            .attempts
            .insert("login".into(), pending_attempt(None));
        let params = json!({"attemptId":"login"});
        let first = service.poll_device_login(&params).unwrap();
        assert_eq!(first["intervalSeconds"], 12);
        assert_public_auth(&first);
        let early = service.poll_device_login(&params).unwrap();
        assert_eq!(early["status"], "pending");
        assert!(early["retryAfterMs"].as_u64().unwrap() >= 11_000);
        service
            .state
            .lock()
            .unwrap()
            .attempts
            .get_mut("login")
            .unwrap()
            .next_poll = Instant::now();
        assert_eq!(
            service.poll_device_login(&params).unwrap()["intervalSeconds"],
            17
        );
        server.join().unwrap();
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn cancellation_during_device_token_io_cannot_authorize_or_publish() {
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let (url, server) = mock_responses(
            vec![(200, json!({"access_token":"test-access","expires_in":3600}))],
            move |_| {
                entered_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            },
        );
        let (service, path) = test_service(&url);
        service
            .state
            .lock()
            .unwrap()
            .attempts
            .insert("login".into(), pending_attempt(None));
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| service.poll_device_login(&json!({"attemptId":"login"})));
            entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            assert_eq!(
                service
                    .poll_device_login(&json!({"attemptId":"login"}))
                    .unwrap()["status"],
                "pending"
            );
            service
                .cancel_device_login(&json!({"attemptId":"login"}))
                .unwrap();
            release_tx.send(()).unwrap();
            assert!(worker.join().unwrap().is_err());
        });
        assert!(service.state.lock().unwrap().session.is_none());
        assert!(
            service
                .complete_device_login(&json!({"attemptId":"login"}))
                .is_err()
        );
        assert!(!path.join("accounts.json").exists());
        server.join().unwrap();
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn revoke_uses_selected_access_grant_and_local_logout_survives_rejection() {
        let (url, server) = mock_requests(
            vec![(401, json!({"error":"invalid_token"}))],
            |_, request| {
                assert!(request.starts_with("DELETE /assets/v2/Tokens?level=client HTTP/1.1"));
                assert!(
                    request
                        .to_ascii_lowercase()
                        .contains("authorization: bearer test-access")
                );
                assert!(!request.contains("test-client"));
            },
        );
        let (service, path) = test_service(&url);
        service.vault.save(&auth_fixture("next")).unwrap();
        service.vault.save(&auth_fixture("current")).unwrap();
        {
            let mut state = service.state.lock().unwrap();
            state.session = Some(auth_fixture("current"));
            state
                .attempts
                .insert("stale".into(), pending_attempt(Some(auth_fixture("late"))));
        }
        let result = service.logout().unwrap();
        assert_eq!(result["session"]["user"]["userId"], "next");
        assert_eq!(result["cleanup"]["remoteRevoke"], "failed");
        assert_eq!(result["cleanup"]["localCleanup"], "complete");
        assert!(service.vault.load("current").unwrap().is_none());
        assert!(service.state.lock().unwrap().attempts.is_empty());
        assert_public_auth(&result);
        server.join().unwrap();
        std::fs::remove_dir_all(path).unwrap();
    }

    pub(super) fn test_service(url: &str) -> (GfnService, PathBuf) {
        let path =
            std::env::temp_dir().join(format!("opennow-auth-test-{}", rand::random::<u64>()));
        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let mut service = GfnService::with_client(
            client,
            Endpoints {
                token: url.to_owned(),
                client_token: url.to_owned(),
                userinfo: url.to_owned(),
                device_authorize: url.to_owned(),
                revoke: format!("{url}/assets/v2/Tokens?level=client"),
                ..Endpoints::default()
            },
            path.clone(),
        );
        service.vault = CredentialVault::memory(path.clone());
        service.state.lock().unwrap().providers = vec![LoginProvider::default_nvidia()];
        service.state.lock().unwrap().providers_expires =
            Some(Instant::now() + Duration::from_secs(900));
        (service, path)
    }

    #[test]
    fn refresh_cannot_overwrite_a_new_login() {
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let (url, server) = mock_responses(
            vec![(200, json!({"client_token":"updated", "expires_in":3600}))],
            move |_| {
                entered_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            },
        );
        let (service, path) = test_service(&url);
        let service = std::sync::Arc::new(service);
        {
            let mut state = service.state.lock().unwrap();
            let mut old = auth_fixture("old-account");
            old.tokens.client_token_expires_at = None;
            state.session = Some(old);
            state.persistence_state = "none".into();
            state.attempts.insert(
                "new-login".into(),
                DeviceAttempt {
                    provider: LoginProvider::default_nvidia(),
                    device_code: "test-device".into(),
                    expires_at: now_ms() + 60_000,
                    deadline: Instant::now() + Duration::from_secs(60),
                    interval_seconds: 5,
                    next_poll: Instant::now(),
                    in_flight: false,
                    pending_session: Some(auth_fixture("new-account")),
                },
            );
        }
        let refreshing = service.clone();
        let refresh = std::thread::spawn(move || refreshing.session().unwrap());
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let signing_in = service.clone();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let login = std::thread::spawn(move || {
            let result = signing_in
                .complete_device_login(&json!({"attemptId":"new-login", "staySignedIn":false}));
            done_tx.send(()).unwrap();
            result.unwrap()
        });
        let completed_during_refresh = done_rx.recv_timeout(Duration::from_millis(100)).is_ok();
        release_tx.send(()).unwrap();
        refresh.join().unwrap();
        login.join().unwrap();
        server.join().unwrap();
        assert_eq!(
            service
                .state
                .lock()
                .unwrap()
                .session
                .as_ref()
                .unwrap()
                .user
                .user_id,
            "new-account"
        );
        assert!(!completed_during_refresh);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn cancelled_auth_mutations_do_not_run_after_waiting_for_refresh() {
        type Mutation = fn(&GfnService) -> Result<Value, ServiceError>;
        let mutations: [Mutation; 5] = [
            |service| {
                service.complete_device_login(&json!({"attemptId":"pending", "staySignedIn":false}))
            },
            GfnService::logout,
            GfnService::logout_all,
            |service| service.switch_account(&json!({"userId":"new-account"})),
            |service| service.remove_account(&json!({"userId":"old-account"})),
        ];
        let (service, path) = test_service("http://127.0.0.1:1");
        {
            let mut state = service.state.lock().unwrap();
            state.session = Some(auth_fixture("old-account"));
            state.persistence_state = "none".into();
            state.attempts.insert(
                "pending".into(),
                DeviceAttempt {
                    provider: LoginProvider::default_nvidia(),
                    device_code: "test-device".into(),
                    expires_at: now_ms() + 60_000,
                    deadline: Instant::now() + Duration::from_secs(60),
                    interval_seconds: 5,
                    next_poll: Instant::now(),
                    in_flight: false,
                    pending_session: Some(auth_fixture("new-account")),
                },
            );
        }
        let requests = std::sync::Arc::new(crate::requests::Requests::default());
        for mutation in mutations {
            let operation = service.auth_operation.lock().unwrap();
            let permit = requests.admit("mutation", "auth.mutation").unwrap();
            let token = permit.token.clone();
            let (started_tx, started_rx) = std::sync::mpsc::channel();
            std::thread::scope(|scope| {
                let worker = scope.spawn(|| {
                    crate::requests::scope(token, || {
                        crate::requests::check().unwrap();
                        started_tx.send(()).unwrap();
                        mutation(&service)
                    })
                });
                started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                requests.cancel("mutation");
                drop(operation);
                assert_eq!(worker.join().unwrap().unwrap_err().code, "cancelled");
            });
            let state = service.state.lock().unwrap();
            assert_eq!(state.session.as_ref().unwrap().user.user_id, "old-account");
            assert!(state.attempts.contains_key("pending"));
            assert!(!path.join("accounts.json").exists());
            drop(permit);
        }
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn transient_fallback_failure_does_not_revoke_saved_session() {
        let (url, server) = mock_responses(
            vec![
                (400, json!({"error":"invalid_grant"})),
                (503, json!({"error":"temporarily_unavailable"})),
            ],
            |_| {},
        );
        let (service, path) = test_service(&url);
        let error = service
            .refresh_session(&auth_fixture("test-account"))
            .unwrap_err();
        server.join().unwrap();
        assert_eq!(error.code, "session_refresh_failed");
        assert!(!is_definitive_auth_revocation(&error));
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn all_revoked_refresh_mechanisms_are_definitive() {
        let (url, server) = mock_responses(
            vec![
                (400, json!({"error":"invalid_grant"})),
                (401, json!({"error":"invalid_token"})),
            ],
            |_| {},
        );
        let (service, path) = test_service(&url);
        let error = service
            .refresh_session(&auth_fixture("test-account"))
            .unwrap_err();
        server.join().unwrap();
        assert!(is_definitive_auth_revocation(&error));
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn upstream_errors_do_not_echo_credentials_or_trust_server_error_revocation() {
        let (url, server) = mock_responses(
            vec![
                (
                    400,
                    json!({"error":"invalid_grant", "error_description":"test-secret-token"}),
                ),
                (
                    503,
                    json!({"error":"invalid_grant", "access_token":"test-secret-token"}),
                ),
            ],
            |_| {},
        );
        let client = Client::builder().no_proxy().build().unwrap();
        let error = ServiceError::response("Test", client.get(&url).send().unwrap());
        assert!(!error.message.contains("test-secret-token"));
        assert!(is_definitive_auth_revocation(&error));
        let error = ServiceError::response("Test", client.get(&url).send().unwrap());
        assert!(!error.message.contains("test-secret-token"));
        assert!(!is_definitive_auth_revocation(&error));
        server.join().unwrap();
    }

    #[test]
    fn network_errors_do_not_include_request_urls() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let error = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(1))
            .build()
            .unwrap()
            .get(format!("http://{address}/?access_token=test-secret-token"))
            .send()
            .unwrap_err();
        let error = ServiceError::network("Test request", error);
        assert_eq!(error.code, "network_error");
        assert!(!error.message.contains("test-secret-token"));
        assert!(!error.message.contains(&address.to_string()));
    }

    #[test]
    fn browser_service_identity_keeps_nvidia_required_webrtc_label() {
        let headers = lcars_headers("token", "BROWSER", "WEBRTC", false).unwrap();
        assert_eq!(headers["nv-client-type"], "BROWSER");
        assert_eq!(headers["nv-client-streamer"], "WEBRTC");
    }

    #[test]
    fn steam_deck_identity_advertises_valve_console_profile() {
        let headers = lcars_headers("token", "NATIVE", "NVIDIA-CLASSIC", true).unwrap();
        assert_eq!(headers["nv-device-os"], "STEAMOS");
        assert_eq!(headers["nv-device-type"], "CONSOLE");
        assert_eq!(headers["nv-device-make"], "VALVE");
        assert_eq!(headers["nv-device-model"], "STEAMDECK");
    }

    #[test]
    fn desktop_identity_keeps_generic_pc_profile() {
        let headers = lcars_headers("token", "NATIVE", "NVIDIA-CLASSIC", false).unwrap();
        assert_eq!(headers["nv-device-type"], "DESKTOP");
        assert_eq!(headers["nv-device-make"], "GENERIC");
        assert_eq!(headers["nv-device-model"], "PC");
    }

    #[test]
    fn store_marquee_parses_marketing_and_game_slides() {
        let payload: Value = serde_json::from_str(
            r#"{"data":{"panels":[{
                "id":"marquee","name":"Marquee","sections":[{
                    "id":"s1","title":"Hero","items":[
                        {"__typename":"MarketingItem","title":"GFN Thursday","body":"New drops",
                         "images":{"MARQUEE_HERO_IMAGE":"https://img.example/hero.jpg"},
                         "action":{"label":"View details","uri":"gfn://x"}},
                        {"__typename":"GameItem","app":{
                            "id":"123","title":"Doom","publisherName":"Bethesda",
                            "images":{"MARQUEE_HERO_IMAGE":"https://img.example/doom.jpg"},
                            "variants":[{"id":"123","appStore":"Steam",
                                         "gfn":{"library":{"status":"NOT_OWNED"}}}],
                            "gfn":{"playabilityState":"PLAYABLE"}}},
                        {"__typename":"FilterItem","id":"f","title":"Shop"}
                    ]}]}]}}"#,
        )
        .unwrap();
        let slides = parse_store_marquee(&payload, &HashMap::new());
        assert_eq!(slides.len(), 2);
        assert_eq!(slides[0]["kind"], "marketing");
        assert_eq!(slides[0]["title"], "GFN Thursday");
        assert_eq!(slides[0]["image"], "https://img.example/hero.jpg");
        assert_eq!(slides[0]["actionLabel"], "View details");
        assert_eq!(slides[1]["kind"], "game");
        assert_eq!(slides[1]["game"]["title"], "Doom");
    }

    #[test]
    fn store_panels_keep_titled_sections_with_valid_games() {
        let payload: Value = serde_json::from_str(
            r#"{"data":{"panels":[{
                "id":"main","name":"Main","sections":[
                    {"id":"gfn-thu","title":"GFN Thursday","items":[
                        {"__typename":"GameItem","app":{
                            "id":"7","title":"Hades",
                            "variants":[{"id":"7","appStore":"Steam",
                                         "gfn":{"library":{"status":"NOT_OWNED"}}}],
                            "gfn":{"playabilityState":"PLAYABLE"}}},
                        {"__typename":"GameItem","app":{"id":"8","title":"","variants":[]}}
                    ]},
                    {"id":"empty","title":"","items":[]}
                ]}]}}"#,
        )
        .unwrap();
        let panels = parse_store_panels(&payload, &HashMap::new());
        assert_eq!(panels.len(), 1);
        assert_eq!(panels[0]["sections"].as_array().unwrap().len(), 1);
        let games = panels[0]["sections"][0]["games"].as_array().unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0]["title"], "Hades");
    }

    #[test]
    fn store_shelves_prefer_posters_without_changing_hero_art() {
        let payload = json!({"data":{"panels":[{"id":"main","name":"Main","sections":[{
            "id":"featured","title":"Featured","items":[{"__typename":"GameItem","app":{
                "id":"7","title":"Game","variants":[{"id":"7","appStore":"STEAM"}],
                "images":{"GAME_BOX_ART":"https://img.example/poster.jpg","HERO_IMAGE":"https://img.example/hero.jpg"}
            }}]
        }]}]}});
        let panels = parse_store_panels(&payload, &HashMap::new());
        let game = &panels[0]["sections"][0]["games"][0];
        assert_eq!(game["imageUrl"], "https://img.example/poster.jpg");
        assert_eq!(game["heroImageUrl"], "https://img.example/hero.jpg");
        assert!(STORE_PANELS_QUERY.contains("GAME_BOX_ART"));
    }

    #[test]
    fn home_key_art_is_separate_from_posters_and_heroes() {
        let game = app_to_game(&json!({
            "id":"7","title":"Game","variants":[{"id":"7","appStore":"STEAM"}],
            "images":{
                "GAME_BOX_ART":"https://img.example/poster.jpg",
                "KEY_IMAGE":"https://img.example/key-image.jpg",
                "KEY_ART":"https://img.nvidiagrid.net/apps/game/ZZ/KEY_ART.jpg",
                "HERO_IMAGE":"https://img.example/hero.jpg"
            }
        }))
        .unwrap();
        assert_eq!(game["imageUrl"], "https://img.example/poster.jpg");
        assert_eq!(game["heroImageUrl"], "https://img.example/hero.jpg");
        assert_eq!(
            game["keyArtUrl"],
            "https://img.nvidiagrid.net/apps/game/ZZ/KEY_ART.jpg;f=jpg;w=900"
        );
    }

    #[test]
    fn home_key_art_handles_missing_and_empty_images() {
        for (images, expected) in [
            (
                json!({"KEY_ART":"  ", "KEY_IMAGE":"https://img.example/key.jpg"}),
                json!("https://img.example/key.jpg"),
            ),
            (
                json!({"KEY_ART":["", "https://img.example/key-art.jpg"]}),
                json!("https://img.example/key-art.jpg"),
            ),
            (
                json!({"GAME_BOX_ART":"https://img.example/poster.jpg"}),
                Value::Null,
            ),
            (Value::Null, Value::Null),
        ] {
            let game = app_to_game(&json!({
                "id":"7","title":"Game","variants":[{"id":"7","appStore":"STEAM"}],
                "images":images
            }))
            .unwrap();
            assert_eq!(game["keyArtUrl"], expected);
        }
    }

    #[test]
    fn store_definitions_keep_groups_with_options() {
        let payload = json!({"data":{"filterGroupDefinitions":[
            {"id":"digital_store","label":"Stores","filters":[
                {"id":"steam","label":"Steam"},
                {"id":"epic","label":"Epic Games"}]},
            {"id":"empty","label":"Empty","filters":[]},
        ]}});
        let groups = parse_store_definitions(&payload);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0]["id"], "digital_store");
        assert_eq!(groups[0]["options"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn provider_discovery_is_normalized_and_sorted() {
        let payload = json!({"gfnServiceInfo":{"gfnServiceEndpoints":[
            {"idpId":"two","loginProviderCode":"BPC","loginProviderDisplayName":"BPC","streamingServiceUrl":"https://two.example","loginProviderPriority":20},
            {"idpId":"one","loginProviderCode":"NVIDIA","loginProviderDisplayName":"NVIDIA","streamingServiceUrl":"https://one.example/","loginProviderPriority":1}
        ]}});
        let providers = parse_providers(&payload);
        assert_eq!(providers[0].idp_id, "one");
        assert_eq!(providers[1].display_name, "bro.game");
        assert!(providers[1].streaming_service_url.ends_with('/'));
    }

    #[test]
    fn cached_provider_lookup_does_not_reenter_state_lock() {
        let client = Client::builder().build().unwrap();
        let service = GfnService::with_client(client, Endpoints::default(), std::env::temp_dir());
        service.state.lock().unwrap().providers = vec![LoginProvider::default_nvidia()];
        service.state.lock().unwrap().providers_expires =
            Some(Instant::now() + Duration::from_secs(900));
        let result = service.providers().unwrap();
        assert_eq!(result["providers"][0]["code"], "NVIDIA");
    }

    #[test]
    fn public_catalog_mapping_matches_electron_contract() {
        let game = public_game_to_info(&json!({
            "id":"ignored", "title":"Portal 2", "status":"AVAILABLE",
            "steamUrl":"https://store.steampowered.com/app/620/Portal_2/", "store":"Steam"
        }))
        .unwrap();
        assert_eq!(game["id"], "620");
        assert_eq!(game["launchAppId"], "620");
        assert_eq!(game["variants"][0]["store"], "Steam");
        assert!(
            game["imageUrl"]
                .as_str()
                .unwrap()
                .contains("/620/header.jpg")
        );
        assert!(public_game_to_info(&json!({"title":"Gone", "status":"MAINTENANCE"})).is_none());
    }

    #[test]
    fn account_library_mapping_preserves_launch_and_ownership() {
        let game = app_to_game(&json!({
            "id":"cms-portal",
            "title":"Portal 2",
            "publisherName":"Valve",
            "genres":["Puzzle"],
            "images":{"GAME_BOX_ART":"https://img.nvidiagrid.net/apps/portal"},
            "variants":[{
                "id":"620",
                "appStore":"STEAM",
                "supportedControls":["GAMEPAD"],
                "gfn":{"status":"AVAILABLE","features":[{"key":"IN_GAME_SETTINGS_PERSISTENCE_ENABLED","value":"true"}],"library":{"status":"PLATFORM_SYNC","selected":true,"lastPlayedDate":"2026-01-01"}}
            }],
            "gfn":{"playabilityState":"PLAYABLE"}
        })).unwrap();
        assert_eq!(game["launchAppId"], "620");
        assert_eq!(game["selectedVariantIndex"], 0);
        assert_eq!(game["isInLibrary"], true);
        assert_eq!(game["variants"][0]["inLibrary"], true);
        assert_eq!(
            game["variants"][0]["supportsInGameSettingsPersistence"],
            true
        );
        assert!(game["imageUrl"].as_str().unwrap().ends_with(";f=jpg;w=900"));
        assert!(game["searchText"].as_str().unwrap().contains("valve"));
        assert!(!gfn_feature_enabled(
            &json!({"key":"IN_GAME_SETTINGS_PERSISTENCE_ENABLED","value":"false"}),
            "IN_GAME_SETTINGS_PERSISTENCE_ENABLED"
        ));
    }

    #[test]
    fn account_library_mapping_preserves_every_platform_ownership_state() {
        let game = app_to_game(&json!({
            "id":"cms-multi-store",
            "title":"Multi Store Game",
            "variants":[
                {"id":"1001","appStore":"Steam","gfn":{"library":{"status":"PLATFORM_SYNC","selected":true}}},
                {"id":"1002","appStore":"Epic Games Store","gfn":{"library":{"status":"NOT_OWNED","selected":false}}},
                {"id":"1003","appStore":"Xbox","gfn":{"library":{"status":"MANUAL","selected":false}}}
            ],
            "gfn":{"playabilityState":"PLAYABLE"}
        }))
        .unwrap();

        assert_eq!(
            game["availableStores"],
            json!(["Steam", "Epic Games Store", "Xbox"])
        );
        assert_eq!(game["selectedVariantIndex"], 0);
        assert_eq!(game["variants"][0]["inLibrary"], true);
        assert_eq!(game["variants"][1]["inLibrary"], false);
        assert_eq!(game["variants"][2]["inLibrary"], true);
    }

    #[test]
    fn account_library_mapping_prefers_owned_variant_without_saved_selection() {
        for status in ["MANUAL", "PLATFORM_SYNC", "IN_LIBRARY"] {
            let game = app_to_game(&json!({
                "id":"cms-multi-store", "title":"Multi Store Game",
                "variants":[
                    {"id":"1001","appStore":"Steam","gfn":{"library":{"status":"NOT_OWNED"}}},
                    {"id":"1003","appStore":"Xbox","gfn":{"library":{"status":status}}}
                ]
            }))
            .unwrap();
            assert_eq!(game["selectedVariantIndex"], 1);
            assert_eq!(game["launchAppId"], "1003");
        }
    }

    #[test]
    fn account_library_mapping_preserves_saved_selection_over_ownership() {
        for selected in [false, true] {
            let game = app_to_game(&json!({
                "id":"cms-multi-store", "title":"Multi Store Game",
                "variants":[
                    {"id":"1001","appStore":"Steam","gfn":{"library":{"status":"NOT_OWNED","selected":selected}}},
                    {"id":"1003","appStore":"Xbox","gfn":{"library":{"status":"MANUAL"}}}
                ]
            })).unwrap();
            assert_eq!(game["selectedVariantIndex"], if selected { 0 } else { 1 });
            assert_eq!(game["launchAppId"], if selected { "1001" } else { "1003" });
        }
    }

    #[test]
    fn account_library_mapping_keeps_first_variant_when_none_owned() {
        let game = app_to_game(&json!({
            "id":"cms-multi-store", "title":"Multi Store Game",
            "variants":[
                {"id":"1001","appStore":"Steam"},
                {"id":"1003","appStore":"Xbox"}
            ]
        }))
        .unwrap();
        assert_eq!(game["selectedVariantIndex"], 0);
        assert_eq!(game["launchAppId"], "1001");
    }

    #[test]
    fn creates_real_qr_matrix() {
        let rows = qr_rows("https://login.nvidia.com/device?user_code=ABCD");
        assert!(rows.len() >= 21);
        assert!(rows.iter().all(|row| row.len() == rows.len()));
        assert!(rows.iter().any(|row| row.contains('1')));
    }

    #[test]
    fn jwt_user_info_uses_claims_without_network() {
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
            br#"{"sub":"42","email":"player@example.com","preferred_username":"Player","gfn_tier":"ULTIMATE"}"#,
        );
        let user = user_from_jwt(&format!("header.{payload}.signature")).unwrap();
        assert_eq!(user.user_id, "42");
        assert_eq!(user.display_name, "Player");
        assert_eq!(user.membership_tier, "ULTIMATE");
        assert!(user.avatar_url.unwrap().contains("gravatar.com/avatar/"));
    }

    #[test]
    fn refresh_revocation_is_detected_from_oauth_errors() {
        assert!(is_definitive_auth_revocation(&ServiceError {
            code: "upstream_error",
            message: "Refresh-token exchange failed (400): {\"error\":\"invalid_grant\"}"
                .to_owned(),
        }));
        assert!(is_definitive_auth_revocation(&ServiceError {
            code: "upstream_error",
            message: "token has been revoked by the user".to_owned(),
        }));
        assert!(!is_definitive_auth_revocation(&ServiceError {
            code: "network_error",
            message: "Refresh-token exchange failed: connection reset".to_owned(),
        }));
    }
}
