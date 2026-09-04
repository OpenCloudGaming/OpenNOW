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
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const DEFAULT_IDP_ID: &str = "PDiAhv2kJTFeQ7WOPqiQ2tRZ7lGhR2X11dXvM4TZSxg";
const DEFAULT_STREAMING_URL: &str = "https://prod.cloudmatchbeta.nvidiagrid.net/";
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
const STORE_PANELS_QUERY: &str = r#"query GetStorePanels($vpcId: String!, $locale: String!, $panelNames: [String]!) {
  panels(vpcId: $vpcId, language: $locale, names: $panelNames) {
    id
    name
    sections {
      id
      title
      items {
        __typename
        ... on GameItem {
          app {
            id
            title
            publisherName
            images { GAME_BOX_ART KEY_IMAGE KEY_ART HERO_IMAGE TV_BANNER MARQUEE_HERO_IMAGE }
            itemMetadata { campaignIds }
            variants {
              id
              appStore
              storeUrl
              supportedControls
              gfn {
                status
                library { status selected }
              }
            }
            gfn { playType playabilityState minimumMembershipTierLabel }
          }
        }
      }
    }
  }
}"#;

const STORE_MARQUEE_QUERY: &str = r#"query GetStoreMarquee($vpcId: String!, $locale: String!, $panelNames: [String]!) {
  panels(vpcId: $vpcId, language: $locale, names: $panelNames) {
    id
    name
    sections {
      id
      title
      items {
        __typename
        ... on MarketingItem {
          id
          title
          body
          images { MARQUEE_HERO_IMAGE HERO_IMAGE }
          action { uri label }
        }
        ... on GameItem {
          app {
            id
            title
            publisherName
            images { GAME_BOX_ART KEY_IMAGE KEY_ART HERO_IMAGE TV_BANNER MARQUEE_HERO_IMAGE }
            itemMetadata { campaignIds }
            variants {
              id
              appStore
              storeUrl
              supportedControls
              gfn {
                status
                library { status selected }
              }
            }
            gfn { playType playabilityState minimumMembershipTierLabel }
          }
        }
      }
    }
  }
}"#;

const STORE_DEFINITIONS_QUERY: &str = r#"query GetStoreFilterDefinitions($locale: String!) {
  filterGroupDefinitions(language: $locale) {
    id
    label
    filters {
      id
      label
    }
  }
  sortOrderDefinitions(language: $locale) {
    id
    label
    orderBy
  }
}"#;

const STORE_MARQUEE_SHA: &str = "dd4bddfdef4707dfe340cc2040d6bb9c4c45f706976fca15b2ef33221c385d7f";
const STORE_PANELS_SHA: &str = "46ec15f267a056e7d5e46e629efa929529e5e7542a4850faece90b9f8fa5f810";

const STORE_BROWSE_QUERY: &str = r#"query GetStoreBrowseApps(
  $vpcId: String!, $locale: String!, $sortString: String!,
  $fetchCount: Int!, $cursor: String!, $filters: AppFilterFields!
) {
  apps(vpcId: $vpcId, language: $locale, orderBy: $sortString, first: $fetchCount, after: $cursor, filters: $filters) {
    numberReturned numberSupported pageInfo { hasNextPage endCursor totalCount }
    items {
      id title developerName publisherName genres supportedControls
      images { KEY_ART KEY_IMAGE GAME_BOX_ART TV_BANNER HERO_IMAGE MARQUEE_HERO_IMAGE FEATURE_IMAGE GAME_LOGO SCREENSHOTS }
      variants {
        id appStore storeUrl supportedControls
        gfn {
          status
          features {
            __typename
            ... on GfnSubscriptionFeatureValue { key value }
            ... on GfnSubscriptionFeatureValueList { key values }
          }
          library { status selected lastPlayedDate }
        }
      }
      gfn { playType playabilityState minimumMembershipTierLabel catalogSkuStrings { SKU_BASED_TAG SKU_BASED_PLAYABILITY_TEXT } }
      itemMetadata { campaignIds }
    }
  }
}"#;

const STORE_SEARCH_QUERY: &str = r#"query GetStoreSearchApps(
  $vpcId: String!, $locale: String!, $sortString: String!,
  $fetchCount: Int!, $cursor: String!, $searchString: String!, $filters: AppFilterFields!
) {
  apps(vpcId: $vpcId, language: $locale, orderBy: $sortString, first: $fetchCount, after: $cursor, searchQuery: $searchString, filters: $filters) {
    numberReturned numberSupported pageInfo { hasNextPage endCursor totalCount }
    items {
      id title developerName publisherName genres supportedControls
      images { KEY_ART KEY_IMAGE GAME_BOX_ART TV_BANNER HERO_IMAGE MARQUEE_HERO_IMAGE FEATURE_IMAGE GAME_LOGO SCREENSHOTS }
      variants {
        id appStore storeUrl supportedControls
        gfn {
          status
          features {
            __typename
            ... on GfnSubscriptionFeatureValue { key value }
            ... on GfnSubscriptionFeatureValueList { key values }
          }
          library { status selected lastPlayedDate }
        }
      }
      gfn { playType playabilityState minimumMembershipTierLabel catalogSkuStrings { SKU_BASED_TAG SKU_BASED_PLAYABILITY_TEXT } }
      itemMetadata { campaignIds }
    }
  }
}"#;

const LIBRARY_QUERY: &str = r#"query GetLibraryApps(
  $vpcId: String!, $locale: String!, $sortString: String!,
  $fetchCount: Int!, $cursor: String!, $filters: AppFilterFields!
) {
  apps(vpcId: $vpcId, language: $locale, orderBy: $sortString, first: $fetchCount, after: $cursor, filters: $filters) {
    numberReturned numberSupported pageInfo { hasNextPage endCursor totalCount }
    items {
      id title developerName publisherName genres supportedControls
      images { KEY_ART KEY_IMAGE GAME_BOX_ART TV_BANNER HERO_IMAGE MARQUEE_HERO_IMAGE FEATURE_IMAGE GAME_LOGO SCREENSHOTS }
      variants {
        id appStore storeUrl supportedControls
        gfn {
          status
          features {
            __typename
            ... on GfnSubscriptionFeatureValue { key value }
            ... on GfnSubscriptionFeatureValueList { key values }
          }
          library { status selected lastPlayedDate }
        }
      }
      gfn { playType playabilityState minimumMembershipTierLabel catalogSkuStrings { SKU_BASED_TAG SKU_BASED_PLAYABILITY_TEXT } }
      itemMetadata { campaignIds }
    }
  }
}"#;

#[derive(Clone)]
pub struct Endpoints {
    pub service_urls: String,
    pub device_authorize: String,
    pub token: String,
    pub client_token: String,
    pub userinfo: String,
    pub public_catalog: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            service_urls: "https://pcs.geforcenow.com/v1/serviceUrls".to_owned(),
            device_authorize: "https://login.nvidia.com/device/authorize".to_owned(),
            token: "https://login.nvidia.com/token".to_owned(),
            client_token: "https://login.nvidia.com/client_token".to_owned(),
            userinfo: "https://login.nvidia.com/userinfo".to_owned(),
            public_catalog:
                "https://static.nvidiagrid.net/supported-public-game-list/locales/gfnpc-en-US.json"
                    .to_owned(),
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
        if !self.streaming_service_url.ends_with('/') {
            self.streaming_service_url.push('/');
        }
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthTokens {
    pub access_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
    pub expires_at: u64,
    pub auth_client_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_token_expires_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_token_lifetime_ms: Option<u64>,
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

#[derive(Clone)]
struct DeviceAttempt {
    provider: LoginProvider,
    device_code: String,
    expires_at: u64,
    pending_session: Option<AuthSession>,
}

#[derive(Default)]
struct ServiceState {
    providers: Vec<LoginProvider>,
    attempts: HashMap<String, DeviceAttempt>,
    session: Option<AuthSession>,
    public_games: Vec<Value>,
    public_games_proxy_scope: String,
    restore_attempted: bool,
    persistence_state: String,
}

#[derive(Clone, Debug)]
pub struct ServiceError {
    pub code: &'static str,
    pub message: String,
}

impl ServiceError {
    fn network(context: &str, error: impl std::fmt::Display) -> Self {
        Self {
            code: "network_error",
            message: format!("{context}: {error}"),
        }
    }

    fn response(context: &str, response: Response) -> Self {
        let status = response.status();
        let detail = response.text().unwrap_or_default();
        let detail = detail.chars().take(400).collect::<String>();
        Self {
            code: "upstream_error",
            message: if detail.is_empty() {
                format!("{context} ({status})")
            } else {
                format!("{context} ({status}): {detail}")
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

pub struct GfnService {
    client: Client,
    endpoints: Endpoints,
    device_id: String,
    vault: CredentialVault,
    profiles: ConsoleProfiles,
    cloudmatch: CloudMatchService,
    account_connections: AccountConnectionsService,
    persistent_storage: PersistentStorageService,
    state: Mutex<ServiceState>,
}

impl GfnService {
    pub fn new(data_dir: PathBuf) -> Result<Self, String> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(20))
            .pool_idle_timeout(Duration::from_secs(60))
            .build()
            .map_err(|error| error.to_string())?;
        Ok(Self::with_client(client, Endpoints::default(), data_dir))
    }

    fn with_client(client: Client, endpoints: Endpoints, data_dir: PathBuf) -> Self {
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
        Self {
            cloudmatch: CloudMatchService::new(client.clone()),
            account_connections: AccountConnectionsService::new(client.clone()),
            persistent_storage: PersistentStorageService::new(client.clone()),
            client,
            endpoints,
            device_id: stable_device_id(),
            vault,
            profiles: ConsoleProfiles::load(&data_dir),
            state: Mutex::new(ServiceState::default()),
        }
    }

    pub fn providers(&self) -> Result<Value, ServiceError> {
        let cached = self
            .state
            .lock()
            .expect("GFN state poisoned")
            .providers
            .clone();
        if !cached.is_empty() {
            return Ok(json!({"providers": cached}));
        }

        let providers = match self
            .client
            .get(&self.endpoints.service_urls)
            .header(ACCEPT, "application/json")
            .header(USER_AGENT, GFN_USER_AGENT)
            .send()
        {
            Ok(response) if response.status().is_success() => {
                let payload = response.json::<Value>().unwrap_or(Value::Null);
                parse_providers(&payload)
            }
            _ => Vec::new(),
        };
        let providers = if providers.is_empty() {
            vec![LoginProvider::default_nvidia()]
        } else {
            providers
        };
        self.state.lock().expect("GFN state poisoned").providers = providers.clone();
        Ok(json!({"providers": providers}))
    }

    pub fn start_device_login(&self, params: &Value) -> Result<Value, ServiceError> {
        eprintln!("auth: starting device authorization");
        let providers = self.providers()?["providers"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let provider_id = params["providerIdpId"].as_str();
        let provider = providers
            .iter()
            .filter_map(|value| serde_json::from_value::<LoginProvider>(value.clone()).ok())
            .find(|item| provider_id.is_some_and(|wanted| item.idp_id == wanted))
            .or_else(|| {
                providers
                    .first()
                    .and_then(|value| serde_json::from_value(value.clone()).ok())
            })
            .unwrap_or_else(LoginProvider::default_nvidia)
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
        let payload = response.json::<Value>().map_err(|error| {
            ServiceError::network("Invalid device authorization response", error)
        })?;
        let device_code = required_string(&payload, "device_code")?;
        let user_code = required_string(&payload, "user_code")?;
        let verification_uri = required_string(&payload, "verification_uri")?;
        let verification_uri_complete = required_string(&payload, "verification_uri_complete")?;
        let expires_at = now_ms() + payload["expires_in"].as_u64().unwrap_or(600) * 1000;
        let interval_seconds = payload["interval"].as_u64().unwrap_or(5).max(1);
        let attempt_id = random_attempt_id();
        eprintln!("auth: prepared device authorization challenge");
        self.prune_attempts();
        self.state
            .lock()
            .expect("GFN state poisoned")
            .attempts
            .insert(
                attempt_id.clone(),
                DeviceAttempt {
                    provider,
                    device_code: device_code.clone(),
                    expires_at,
                    pending_session: None,
                },
            );
        Ok(json!({
            "attemptId": attempt_id,
            "deviceCode": device_code,
            "userCode": user_code,
            "verificationUri": verification_uri,
            "verificationUriComplete": verification_uri_complete,
            "expiresAt": expires_at,
            "intervalSeconds": interval_seconds,
            "qrRows": qr_rows(&verification_uri_complete),
        }))
    }

    pub fn poll_device_login(&self, params: &Value) -> Result<Value, ServiceError> {
        let attempt_id = required_param(params, "attemptId")?;
        let device_code = required_param(params, "deviceCode")?;
        self.prune_attempts();
        let attempt = self
            .state
            .lock()
            .expect("GFN state poisoned")
            .attempts
            .get(attempt_id)
            .cloned();
        let Some(attempt) = attempt else {
            return Ok(json!({"status":"expired", "error":"QR login was cancelled or expired"}));
        };
        if attempt.device_code != device_code {
            return Ok(json!({"status":"expired", "error":"QR login was cancelled or expired"}));
        }
        if attempt.pending_session.is_some() {
            return Ok(json!({"status":"authorized"}));
        }

        let form = [
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("device_code", device_code),
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
        let payload = response.json::<Value>().unwrap_or(Value::Null);
        if !status.is_success() {
            let error = payload["error"]
                .as_str()
                .unwrap_or("device_token_exchange_failed");
            let description = payload["error_description"].as_str().unwrap_or(error);
            return Ok(match error {
                "authorization_pending" => json!({"status":"pending", "error":description}),
                "slow_down" => {
                    json!({"status":"slow_down", "error":description, "intervalSeconds":10})
                }
                "expired_token" => {
                    self.cancel_device_login(params)?;
                    json!({"status":"expired", "error":description})
                }
                "access_denied" => {
                    self.cancel_device_login(params)?;
                    json!({"status":"access_denied", "error":description})
                }
                _ => {
                    self.cancel_device_login(params)?;
                    json!({"status":"error", "error":description})
                }
            });
        }

        let access_token = required_string(&payload, "access_token")?;
        let tokens = AuthTokens {
            access_token,
            refresh_token: payload["refresh_token"].as_str().map(ToOwned::to_owned),
            id_token: payload["id_token"].as_str().map(ToOwned::to_owned),
            expires_at: now_ms() + payload["expires_in"].as_u64().unwrap_or(86_400) * 1000,
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
        let user = self.fetch_user_info(&tokens)?;
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
        let attempt_id = required_param(params, "attemptId")?;
        let mut state = self.state.lock().expect("GFN state poisoned");
        let attempt = state
            .attempts
            .remove(attempt_id)
            .ok_or_else(|| ServiceError::invalid("QR login is no longer active"))?;
        let session = attempt
            .pending_session
            .ok_or_else(|| ServiceError::invalid("QR login has not been authorized yet"))?;
        state.session = Some(session.clone());
        drop(state);
        let persist = params
            .get("staySignedIn")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let persistence = if persist {
            match self.vault.save(&session) {
                Ok(()) => "local-store",
                Err(error) => {
                    eprintln!("auth: session remains memory-only: {error}");
                    "memory-only"
                }
            }
        } else {
            "none"
        };
        self.state
            .lock()
            .expect("GFN state poisoned")
            .persistence_state = persistence.to_owned();
        Ok(json!({"session": session, "persistence":persistence}))
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

    pub fn session(&self) -> Result<Value, ServiceError> {
        {
            let mut state = self.state.lock().expect("GFN state poisoned");
            if state.session.is_none() && !state.restore_attempted {
                state.restore_attempted = true;
                drop(state);
                match self.vault.load_active() {
                    Ok(session) => {
                        let mut state = self.state.lock().expect("GFN state poisoned");
                        state.persistence_state = if session.is_some() {
                            "local-store"
                        } else {
                            "none"
                        }
                        .to_owned();
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
            let persistence = self
                .state
                .lock()
                .expect("GFN state poisoned")
                .persistence_state
                .clone();
            return Ok(json!({
                "session":null,
                "persistence":persistence,
                "refresh":{"attempted":false,"outcome":"not_attempted","message":"No saved session found."}
            }));
        };

        let needs_refresh = current.tokens.expires_at <= now_ms() + TOKEN_REFRESH_WINDOW_MS;
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
        let (session, refresh) = if needs_refresh {
            match self.refresh_session(&current) {
                Ok(session) => (
                    Some(session),
                    json!({"attempted":true,"outcome":"refreshed","message":"Saved session token refreshed."}),
                ),
                Err(error) if current.tokens.expires_at > now_ms() => {
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
                    self.state.lock().expect("GFN state poisoned").session = None;
                    (
                        None,
                        json!({"attempted":true,"outcome":"revoked","message":"Saved session is no longer valid. Sign in again."}),
                    )
                }
                Err(error) => {
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
        let persistence = self
            .state
            .lock()
            .expect("GFN state poisoned")
            .persistence_state
            .clone();
        Ok(json!({"session":session, "persistence":persistence, "refresh":refresh}))
    }

    pub fn logout(&self) -> Result<Value, ServiceError> {
        let user_id = self
            .state
            .lock()
            .expect("GFN state poisoned")
            .session
            .as_ref()
            .map(|session| session.user.user_id.clone());
        if let Some(user_id) = user_id {
            self.vault
                .remove(&user_id)
                .map_err(|message| ServiceError {
                    code: "credential_store_error",
                    message,
                })?;
            self.profiles
                .forget(&user_id)
                .map_err(|message| ServiceError {
                    code: "profile_storage_error",
                    message,
                })?;
        }
        let next_session = self.vault.load_active().unwrap_or_else(|error| {
            eprintln!("auth: next saved account unavailable: {error}");
            None
        });
        let mut state = self.state.lock().expect("GFN state poisoned");
        state.session = next_session.clone();
        state.persistence_state = if next_session.is_some() {
            "local-store"
        } else {
            "none"
        }
        .to_owned();
        Ok(json!({"ok":true,"session":next_session}))
    }

    pub fn logout_all(&self) -> Result<Value, ServiceError> {
        self.vault.remove_all().map_err(|message| ServiceError {
            code: "credential_store_error",
            message,
        })?;
        self.profiles.forget_all().map_err(|message| ServiceError {
            code: "profile_storage_error",
            message,
        })?;
        let mut state = self.state.lock().expect("GFN state poisoned");
        state.session = None;
        state.persistence_state = "none".to_owned();
        Ok(json!({"ok":true,"session":null}))
    }

    pub fn clear_cache(&self) -> Value {
        let mut state = self.state.lock().expect("GFN state poisoned");
        let catalog_entries = state.public_games.len();
        let provider_entries = state.providers.len();
        state.public_games.clear();
        state.providers.clear();
        json!({"ok":true,"catalogEntries":catalog_entries,"providerEntries":provider_entries})
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
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
        Ok(json!({"accounts":accounts,"activeUserId":active_user_id}))
    }

    pub fn switch_account(&self, params: &Value) -> Result<Value, ServiceError> {
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
            state.session = Some(session);
            state.persistence_state = "local-store".to_owned();
        }
        let result = self.session()?;
        Ok(
            json!({"session":result["session"],"persistence":result["persistence"],"refresh":result["refresh"]}),
        )
    }

    pub fn remove_account(&self, params: &Value) -> Result<Value, ServiceError> {
        let user_id = required_param(params, "userId")?;
        let was_active = self
            .state
            .lock()
            .expect("GFN state poisoned")
            .session
            .as_ref()
            .is_some_and(|session| session.user.user_id == user_id);
        self.vault.remove(user_id).map_err(|message| ServiceError {
            code: "credential_store_error",
            message,
        })?;
        self.profiles
            .forget(user_id)
            .map_err(|message| ServiceError {
                code: "profile_storage_error",
                message,
            })?;
        if was_active {
            let next = self.vault.load_active().unwrap_or(None);
            let mut state = self.state.lock().expect("GFN state poisoned");
            state.session = next;
            state.persistence_state = if state.session.is_some() {
                "local-store"
            } else {
                "none"
            }
            .to_owned();
        }
        Ok(json!({"ok":true}))
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

    pub fn library_catalog(&self, params: &Value, settings: &Value) -> Result<Value, ServiceError> {
        let client = client_for_settings(&self.client, settings).map_err(ServiceError::invalid)?;
        let session_payload = self.session()?;
        let session = serde_json::from_value::<AuthSession>(session_payload["session"].clone())
            .map_err(|_| ServiceError {
                code: "authentication_required",
                message: "Sign in to load your GeForce NOW library".to_owned(),
            })?;
        let token = session
            .tokens
            .id_token
            .as_deref()
            .unwrap_or(&session.tokens.access_token);
        let vpc_id = self.vpc_id(&session, token);
        let limit = params["limit"].as_u64().unwrap_or(600).clamp(1, 2000) as usize;
        let search = params["searchQuery"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_lowercase();
        let mut cursor = String::new();
        let mut games = Vec::new();
        let mut total_count = 0_u64;

        for _ in 0..25 {
            let variables = json!({
                "vpcId":vpc_id,
                "locale":"en_US",
                "sortString":"variants.gfn.library.lastPlayedDate:DESC,computedValues.libraryAddedDate:DESC,sortName:ASC",
                "fetchCount":200,
                "cursor":cursor,
                "filters":{"variants":{"gfn":{"library":{"status":{"notEquals":"NOT_OWNED"}}}}}
            });
            let response = client
                .post(GRAPHQL_URL)
                .headers(graphql_headers(token)?)
                .json(&json!({"query":LIBRARY_QUERY,"variables":variables}))
                .send()
                .map_err(|error| ServiceError::network("GFN library query failed", error))?;
            if !response.status().is_success() {
                return Err(ServiceError::response("GFN library query failed", response));
            }
            let payload = response
                .json::<Value>()
                .map_err(|error| ServiceError::network("Invalid GFN library response", error))?;
            if let Some(message) = graphql_error_message(&payload) {
                return Err(ServiceError {
                    code: "graphql_error",
                    message,
                });
            }
            let apps = &payload["data"]["apps"];
            total_count = apps["pageInfo"]["totalCount"]
                .as_u64()
                .unwrap_or(total_count);
            for app in apps["items"].as_array().into_iter().flatten() {
                if let Some(game) = app_to_game(app) {
                    if search.is_empty()
                        || game["searchText"]
                            .as_str()
                            .is_some_and(|text| text.contains(&search))
                    {
                        games.push(game);
                    }
                }
                if games.len() >= limit {
                    break;
                }
            }
            if games.len() >= limit || !apps["pageInfo"]["hasNextPage"].as_bool().unwrap_or(false) {
                break;
            }
            let Some(next_cursor) = apps["pageInfo"]["endCursor"].as_str() else {
                break;
            };
            if next_cursor.is_empty() || next_cursor == cursor {
                break;
            }
            cursor = next_cursor.to_owned();
        }
        Ok(json!({
            "games":games,
            "count":games.len(),
            "totalCount":total_count.max(games.len() as u64),
            "source":"account-library",
            "fetchedAt":now_ms()
        }))
    }

    pub fn store_catalog(&self, params: &Value, settings: &Value) -> Result<Value, ServiceError> {
        let client = client_for_settings(&self.client, settings).map_err(ServiceError::invalid)?;
        let session_payload = self.session()?;
        let session = serde_json::from_value::<AuthSession>(session_payload["session"].clone())
            .map_err(|_| ServiceError {
                code: "authentication_required",
                message: "Sign in to browse the GeForce NOW store catalog".to_owned(),
            })?;
        let token = session
            .tokens
            .id_token
            .as_deref()
            .unwrap_or(&session.tokens.access_token);
        let vpc_id = self.vpc_id(&session, token);
        let limit = params["limit"].as_u64().unwrap_or(1500).clamp(1, 3000) as usize;
        let search = params["searchQuery"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_owned();
        let mut cursor = String::new();
        let mut games = Vec::new();
        let mut total_count = 0_u64;

        for _ in 0..15 {
            let searching = !search.is_empty();
            let query = if searching {
                STORE_SEARCH_QUERY
            } else {
                STORE_BROWSE_QUERY
            };
            let mut variables = json!({
                "vpcId":vpc_id,
                "locale":"en_US",
                "sortString":"itemMetadata.relevance:DESC,sortName:ASC",
                "fetchCount":200,
                "cursor":cursor,
                "filters":{},
            });
            if searching {
                variables["searchString"] = Value::String(search.clone());
            }
            let response = client
                .post(GRAPHQL_URL)
                .headers(graphql_headers(token)?)
                .json(&json!({"query":query,"variables":variables}))
                .send()
                .map_err(|error| ServiceError::network("GFN store query failed", error))?;
            if !response.status().is_success() {
                return Err(ServiceError::response(
                    "GFN store query failed",
                    response,
                ));
            }
            let payload = response
                .json::<Value>()
                .map_err(|error| ServiceError::network("Invalid GFN store response", error))?;
            if let Some(message) = graphql_error_message(&payload) {
                return Err(ServiceError {
                    code: "graphql_error",
                    message,
                });
            }
            let apps = &payload["data"]["apps"];
            total_count = apps["pageInfo"]["totalCount"]
                .as_u64()
                .unwrap_or(total_count);
            for app in apps["items"].as_array().into_iter().flatten() {
                if let Some(game) = app_to_game(app) {
                    games.push(game);
                }
                if games.len() >= limit {
                    break;
                }
            }
            if games.len() >= limit || !apps["pageInfo"]["hasNextPage"].as_bool().unwrap_or(false) {
                break;
            }
            let Some(next_cursor) = apps["pageInfo"]["endCursor"].as_str() else {
                break;
            };
            if next_cursor.is_empty() || next_cursor == cursor {
                break;
            }
            cursor = next_cursor.to_owned();
        }
        let browse_by_id: HashMap<String, Value> = games
            .iter()
            .filter_map(|game| game_identity(game).map(|id| (id, game.clone())))
            .collect();
        // Storefront chrome is best-effort: panels, hero and categories must
        // never fail the game list itself.
        let panel_variables = json!({"vpcId":vpc_id,"locale":"en_US","panelNames":["MAIN"]});
        let marquee_variables = json!({"vpcId":vpc_id,"locale":"en_US","panelNames":["MARQUEE"]});
        let panels = fetch_panels_document(
            &client,
            token,
            panel_variables,
            "panels/MainV2",
            STORE_PANELS_SHA,
            STORE_PANELS_QUERY,
            "GFN store panels query",
        )
        .map(|payload| parse_store_panels(&payload, &browse_by_id))
        .unwrap_or_default();
        let marquee = fetch_panels_document(
            &client,
            token,
            marquee_variables,
            "panels/Marquee",
            STORE_MARQUEE_SHA,
            STORE_MARQUEE_QUERY,
            "GFN store marquee query",
        )
        .map(|payload| parse_store_marquee(&payload, &browse_by_id))
        .unwrap_or_default();
        let definition_variables = json!({"locale":"en_US"});
        let filter_groups = fetch_panels_document(
            &client,
            token,
            definition_variables,
            "filterGroupAndSortOrderDefinitions",
            "ef725de5e93b093de1ac7418fed0ffb4f6ae2b9c14f743ab274a791521488eb9",
            STORE_DEFINITIONS_QUERY,
            "GFN store filter definitions query",
        )
        .map(|payload| parse_store_definitions(&payload))
        .unwrap_or_default();
        Ok(json!({
            "games":games,
            "count":games.len(),
            "totalCount":total_count.max(games.len() as u64),
            "source":"store-browse",
            "marquee":marquee,
            "panels":panels,
            "filterGroups":filter_groups,
            "fetchedAt":now_ms()
        }))
    }

    pub fn regions(&self) -> Result<Value, ServiceError> {
        let session = self.authenticated_session("Sign in to discover streaming regions")?;
        let token = session
            .tokens
            .id_token
            .as_deref()
            .unwrap_or(&session.tokens.access_token);
        let base = trusted_streaming_base(&session.provider.streaming_service_url)?;
        let url = base
            .join("v2/serverInfo")
            .map_err(|_| ServiceError::invalid("Invalid streaming service URL"))?;
        let response = self
            .client
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
        let mut regions = payload["metaData"].as_array().into_iter().flatten().filter_map(|entry| {
            let name = entry["key"].as_str()?;
            let value = entry["value"].as_str()?;
            if !value.starts_with("https://") || name == "gfn-regions" || name.starts_with("gfn-") { return None }
            Some(json!({"name":name,"url":if value.ends_with('/') { value.to_owned() } else { format!("{value}/") }}))
        }).collect::<Vec<_>>();
        regions.sort_by(|left, right| {
            left["name"]
                .as_str()
                .unwrap_or("")
                .cmp(right["name"].as_str().unwrap_or(""))
        });
        Ok(json!({"regions":regions,"vpcId":payload["requestStatus"]["serverId"]}))
    }

    pub fn subscription(&self, settings: &Value) -> Result<Value, ServiceError> {
        let session = self.authenticated_session("Sign in to load subscription details")?;
        let token = session
            .tokens
            .id_token
            .as_deref()
            .unwrap_or(&session.tokens.access_token);
        let vpc_id = self.vpc_id(&session, token);
        let steam_deck = settings["identifyAsSteamDeck"].as_bool().unwrap_or(false);
        let mut url = url::Url::parse(MES_URL).expect("MES URL is valid");
        url.query_pairs_mut()
            .append_pair("serviceName", "gfn_pc")
            .append_pair("languageCode", "en_US")
            .append_pair("vpcId", &vpc_id)
            .append_pair("userId", &session.user.user_id);
        let response = self
            .client
            .get(url)
            .headers(lcars_headers(token, "NATIVE", "NVIDIA-CLASSIC", steam_deck)?)
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
    }

    pub fn create_session(&self, params: &Value, settings: &Value) -> Result<Value, ServiceError> {
        let session = self.authenticated_session("Sign in to start a streaming session")?;
        self.cloudmatch
            .create(params, settings, &session, &self.device_id)
    }

    pub fn poll_session(&self, params: &Value) -> Result<Value, ServiceError> {
        let session = self.authenticated_session("Sign in to continue the streaming session")?;
        self.cloudmatch.poll(params, &session, &self.device_id)
    }

    pub fn stop_session(&self, params: &Value) -> Result<Value, ServiceError> {
        let session = self.authenticated_session("Sign in to stop the streaming session")?;
        self.cloudmatch.stop(params, &session, &self.device_id)
    }

    pub fn active_session(&self) -> Result<Value, ServiceError> {
        Ok(self.cloudmatch.active())
    }

    pub fn remote_sessions(&self, params: &Value, settings: &Value) -> Result<Value, ServiceError> {
        let session = self.authenticated_session("Sign in to discover active sessions")?;
        self.cloudmatch
            .remote_sessions(params, settings, &session, &self.device_id)
    }

    pub fn claim_session(&self, params: &Value, settings: &Value) -> Result<Value, ServiceError> {
        let session = self.authenticated_session("Sign in to resume the streaming session")?;
        self.cloudmatch
            .claim(params, settings, &session, &self.device_id)
    }

    pub fn report_session_ad(&self, params: &Value) -> Result<Value, ServiceError> {
        let session = self.authenticated_session("Sign in to update the streaming session")?;
        self.cloudmatch.report_ad(params, &session, &self.device_id)
    }

    pub fn account_connections(&self) -> Result<Value, ServiceError> {
        let session = self.authenticated_session("Sign in to load connected game accounts")?;
        self.account_connections.list(&session)
    }

    pub fn sync_account_connection(&self, params: &Value) -> Result<Value, ServiceError> {
        let session = self.authenticated_session("Sign in to sync a game account")?;
        self.account_connections.sync(params, &session)
    }

    pub fn unlink_account_connection(&self, params: &Value) -> Result<Value, ServiceError> {
        let session = self.authenticated_session("Sign in to disconnect a game account")?;
        self.account_connections.unlink(params, &session)
    }

    pub fn start_account_link(&self, params: &Value) -> Result<Value, ServiceError> {
        let session = self.authenticated_session("Sign in to connect a game account")?;
        self.account_connections.start_link(params, &session)
    }

    pub fn poll_account_link(&self, params: &Value) -> Result<Value, ServiceError> {
        self.account_connections.poll_link(params)
    }

    pub fn persistent_storage_locations(&self, params: &Value) -> Result<Value, ServiceError> {
        let session = self.authenticated_session("Sign in to load persistent storage locations")?;
        self.persistent_storage.locations(params, &session)
    }

    pub fn reset_persistent_storage(&self, params: &Value) -> Result<Value, ServiceError> {
        let session = self.authenticated_session("Sign in to reset persistent storage")?;
        self.persistent_storage.reset(params, &session)
    }

    fn authenticated_session(&self, message: &str) -> Result<AuthSession, ServiceError> {
        let payload = self.session()?;
        serde_json::from_value(payload["session"].clone()).map_err(|_| ServiceError {
            code: "authentication_required",
            message: message.to_owned(),
        })
    }

    fn vpc_id(&self, session: &AuthSession, token: &str) -> String {
        let Ok(base) = trusted_streaming_base(&session.provider.streaming_service_url) else {
            return "GFN-PC".to_owned();
        };
        let Ok(url) = base.join("v2/serverInfo") else {
            return "GFN-PC".to_owned();
        };
        let Ok(headers) = lcars_headers(token, "NATIVE", "NVIDIA-CLASSIC", false) else {
            return "GFN-PC".to_owned();
        };
        let Ok(response) = self.client.get(url).headers(headers).send() else {
            return "GFN-PC".to_owned();
        };
        if !response.status().is_success() {
            return "GFN-PC".to_owned();
        }
        response
            .json::<Value>()
            .ok()
            .and_then(|payload| {
                payload["requestStatus"]["serverId"]
                    .as_str()
                    .map(ToOwned::to_owned)
            })
            .unwrap_or_else(|| "GFN-PC".to_owned())
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
            .header(ACCEPT, "application/json")
            .header(AUTHORIZATION, format!("Bearer {}", tokens.access_token))
            .header(ORIGIN, NVIDIA_FILE_ORIGIN)
            .header(USER_AGENT, STEAM_DECK_USER_AGENT)
            .send()
            .map_err(|error| ServiceError::network("User info failed", error))?;
        if !response.status().is_success() {
            return Err(ServiceError::response("User info failed", response));
        }
        let payload = response
            .json::<Value>()
            .map_err(|error| ServiceError::network("Invalid user info response", error))?;
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
        let payload = response
            .json::<Value>()
            .map_err(|error| ServiceError::network("Invalid client token response", error))?;
        let client_token = required_string(&payload, "client_token")?;
        let lifetime = payload["expires_in"].as_u64().unwrap_or(86_400) * 1000;
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
        if let Some(client_token) = session.tokens.client_token.as_deref() {
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
                Err(error) => errors.push(error.message),
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
                Err(error) => errors.push(error.message),
            }
        }
        Err(ServiceError {
            code: "session_refresh_failed",
            message: if errors.is_empty() {
                "Session has no refresh mechanism".to_owned()
            } else {
                errors.join(" | ")
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
        response
            .json::<Value>()
            .map_err(|error| ServiceError::network("Invalid token refresh response", error))
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
            expires_at: now_ms() + payload["expires_in"].as_u64().unwrap_or(86_400) * 1000,
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
        tokens = self.ensure_client_token(tokens)?;
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
            state.persistence_state != "none"
        };
        let persistence = if persist {
            match self.vault.save(&session) {
                Ok(()) => "local-store",
                Err(error) => {
                    eprintln!("auth: refreshed session remains memory-only: {error}");
                    "memory-only"
                }
            }
        } else {
            "none"
        };
        let mut state = self.state.lock().expect("GFN state poisoned");
        state.session = Some(session.clone());
        state.persistence_state = persistence.to_owned();
        Ok(session)
    }

    fn prune_attempts(&self) {
        let now = now_ms();
        self.state
            .lock()
            .expect("GFN state poisoned")
            .attempts
            .retain(|_, attempt| attempt.expires_at > now);
    }
}

fn is_definitive_auth_revocation(error: &ServiceError) -> bool {
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

fn app_to_game(app: &Value) -> Option<Value> {
    let id = app["id"].as_str()?.to_owned();
    let title = app["title"].as_str()?.trim().to_owned();
    if title.is_empty() {
        return None;
    }
    let variants = app["variants"].as_array().into_iter().flatten().filter_map(|variant| {
        let variant_id = variant["id"].as_str()?.to_owned();
        let store = variant["appStore"].as_str().unwrap_or("Unknown").to_owned();
        let library_status = variant["gfn"]["library"]["status"].as_str().map(ToOwned::to_owned);
        let in_library = library_status.as_deref().is_some_and(|status| matches!(status, "MANUAL" | "PLATFORM_SYNC" | "IN_LIBRARY"));
        let supports_persistence = gfn_feature_enabled(
            &variant["gfn"]["features"],
            "IN_GAME_SETTINGS_PERSISTENCE_ENABLED",
        );
        Some(json!({
            "id":variant_id,
            "store":store,
            "storeUrl":variant["storeUrl"],
            "supportedControls":variant["supportedControls"].as_array().cloned().unwrap_or_default(),
            "librarySelected":variant["gfn"]["library"]["selected"].as_bool().unwrap_or(false),
            "inLibrary":in_library,
            "libraryStatus":library_status,
            "lastPlayedDate":variant["gfn"]["library"]["lastPlayedDate"],
            "gfnStatus":variant["gfn"]["status"],
            "supportsInGameSettingsPersistence":supports_persistence,
        }))
    }).collect::<Vec<_>>();
    if variants.is_empty() {
        return None;
    }
    let selected_index = variants
        .iter()
        .position(|variant| variant["librarySelected"].as_bool() == Some(true))
        .unwrap_or(0);
    let launch_id = variants
        .get(selected_index)
        .and_then(|variant| variant["id"].as_str())
        .filter(|value| value.chars().all(|character| character.is_ascii_digit()))
        .or_else(|| {
            variants
                .iter()
                .filter_map(|variant| variant["id"].as_str())
                .find(|value| value.chars().all(|character| character.is_ascii_digit()))
        })
        .or_else(|| {
            id.chars()
                .all(|character| character.is_ascii_digit())
                .then_some(id.as_str())
        })
        .map(ToOwned::to_owned);
    let available_stores = variants
        .iter()
        .filter_map(|variant| variant["store"].as_str().map(ToOwned::to_owned))
        .collect::<Vec<_>>();
    let genres = string_array(&app["genres"]);
    let controls = string_array(&app["supportedControls"]);
    let image_url = first_image(
        &app["images"],
        &[
            "GAME_BOX_ART",
            "KEY_IMAGE",
            "KEY_ART",
            "HERO_IMAGE",
            "TV_BANNER",
        ],
        900,
    );
    let hero_image_url = first_image(
        &app["images"],
        &[
            "MARQUEE_HERO_IMAGE",
            "HERO_IMAGE",
            "TV_BANNER",
            "FEATURE_IMAGE",
            "KEY_IMAGE",
            "KEY_ART",
        ],
        1200,
    );
    let screenshots = image_values(&app["images"]["SCREENSHOTS"], 1200);
    let publisher = app["publisherName"].as_str().map(ToOwned::to_owned);
    let developer = app["developerName"].as_str().map(ToOwned::to_owned);
    let search_text = [
        vec![title.clone()],
        publisher.clone().into_iter().collect(),
        developer.clone().into_iter().collect(),
        available_stores.clone(),
        genres.clone(),
    ]
    .concat()
    .join(" ")
    .to_lowercase();
    let is_in_library = variants
        .iter()
        .any(|variant| variant["inLibrary"].as_bool() == Some(true));
    let last_played = variants
        .iter()
        .filter_map(|variant| variant["lastPlayedDate"].as_str())
        .next()
        .map(ToOwned::to_owned);
    Some(json!({
        "id":id,
        "uuid":id,
        "launchAppId":launch_id,
        "title":title,
        "developerName":developer,
        "publisherName":publisher,
        "genres":genres,
        "supportedControls":controls,
        "imageUrl":image_url,
        "heroImageUrl":hero_image_url,
        "screenshotUrl":screenshots.first(),
        "screenshotUrls":screenshots,
        "playType":app["gfn"]["playType"],
        "membershipTierLabel":app["gfn"]["minimumMembershipTierLabel"],
        "playabilityState":app["gfn"]["playabilityState"],
        "availableStores":available_stores,
        "searchText":search_text,
        "lastPlayed":last_played,
        "isInLibrary":is_in_library,
        "selectedVariantIndex":selected_index,
        "variants":variants,
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
    client: &Client,
    token: &str,
    variables: Value,
    request_type: &str,
    sha: &str,
    fallback_query: &str,
    context: &str,
) -> Result<Value, ServiceError> {
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
    let response = client
        .get(url.clone())
        .headers(headers)
        .send()
        .map_err(|error| ServiceError::network(&format!("{context} failed"), error))?;
    let payload = if response.status().as_u16() == 400 {
        url.query_pairs_mut().append_pair("query", fallback_query);
        let mut retry_headers = graphql_headers(token)?;
        retry_headers.insert(
            reqwest::header::CONTENT_TYPE,
            HeaderValue::from_static("application/graphql"),
        );
        client
            .get(url)
            .headers(retry_headers)
            .send()
            .map_err(|error| ServiceError::network(&format!("{context} failed"), error))?
            .json::<Value>()
            .map_err(|error| ServiceError::network(&format!("Invalid {context} response"), error))?
    } else {
        if !response.status().is_success() {
            return Err(ServiceError::response(&format!("{context} failed"), response));
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
    first_image(
        &item["images"],
        &["MARQUEE_HERO_IMAGE", "HERO_IMAGE"],
        1600,
    )
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
                    || game["variants"].as_array().is_none_or(|variants| variants.is_empty())
                {
                    continue;
                }
                let resolved = game_identity(&game)
                    .as_ref()
                    .and_then(|id| browse_by_id.get(id))
                    .cloned()
                    .unwrap_or(game);
                if games.len() < 24
                    && !games.iter().any(|existing: &Value| {
                        game_identity(existing) == game_identity(&resolved)
                    })
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
    let base = url::Url::parse(value)
        .map_err(|_| ServiceError::invalid("Invalid streaming service URL"))?;
    let hostname = base.host_str().unwrap_or("").to_lowercase();
    if base.scheme() != "https"
        || !(hostname == "prod.cloudmatchbeta.nvidiagrid.net"
            || hostname.ends_with(".geforcenow.nvidiagrid.net"))
    {
        return Err(ServiceError::invalid("Untrusted streaming service URL"));
    }
    Ok(base)
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
mod tests {
    use super::*;

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
