//! Authentication Module
//!
//! OAuth flow and token management for NVIDIA accounts.
//! Supports multi-region login via Alliance Partners.

use anyhow::{Result, Context};
use log::{info, debug};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use std::sync::Arc;
use parking_lot::RwLock;

/// Service URLs API endpoint
const SERVICE_URLS_ENDPOINT: &str = "https://pcs.geforcenow.com/v1/serviceUrls";

/// OAuth client configuration
const CLIENT_ID: &str = "ZU7sPN-miLujMD95LfOQ453IB0AtjM8sMyvgJ9wCXEQ";
const SCOPES: &str = "openid consent email tk_client age";

/// Default NVIDIA IDP ID
const DEFAULT_IDP_ID: &str = "PDiAhv2kJTFeQ7WOPqiQ2tRZ7lGhR2X11dXvM4TZSxg";

/// GFN CEF User-Agent
const GFN_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36 NVIDIACEFClient/HEAD/debb5919f6 GFN-PC/2.0.80.173";

/// CEF Origin for CORS
const CEF_ORIGIN: &str = "https://nvfile";

/// Available redirect ports
const REDIRECT_PORTS: [u16; 5] = [2259, 6460, 7119, 8870, 9096];

// ============================================
// Login Provider (Alliance Partner) Support
// ============================================

/// Login provider from service URLs API
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginProvider {
    /// Unique IDP ID for OAuth
    pub idp_id: String,
    /// Provider code (e.g., "NVIDIA", "KDD", "TWM")
    pub login_provider_code: String,
    /// Display name (e.g., "NVIDIA", "au", "Taiwan Mobile")
    pub login_provider_display_name: String,
    /// Internal provider name
    pub login_provider: String,
    /// Streaming service base URL
    pub streaming_service_url: String,
    /// Priority for sorting
    #[serde(default)]
    pub login_provider_priority: i32,
}

impl LoginProvider {
    /// Create default NVIDIA provider
    pub fn nvidia_default() -> Self {
        Self {
            idp_id: DEFAULT_IDP_ID.to_string(),
            login_provider_code: "NVIDIA".to_string(),
            login_provider_display_name: "NVIDIA".to_string(),
            login_provider: "NVIDIA".to_string(),
            streaming_service_url: "https://prod.cloudmatchbeta.nvidiagrid.net/".to_string(),
            login_provider_priority: 0,
        }
    }

    /// Check if this is an Alliance Partner (non-NVIDIA)
    pub fn is_alliance_partner(&self) -> bool {
        self.login_provider_code != "NVIDIA"
    }
}

/// Service URLs API response
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServiceUrlsResponse {
    request_status: RequestStatus,
    gfn_service_info: Option<GfnServiceInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RequestStatus {
    status_code: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GfnServiceInfo {
    gfn_service_endpoints: Vec<ServiceEndpoint>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServiceEndpoint {
    idp_id: String,
    login_provider_code: String,
    login_provider_display_name: String,
    login_provider: String,
    streaming_service_url: String,
    #[serde(default)]
    login_provider_priority: i32,
}

lazy_static::lazy_static! {
    static ref SELECTED_PROVIDER: Arc<RwLock<Option<LoginProvider>>> = Arc::new(RwLock::new(None));
    static ref CACHED_PROVIDERS: Arc<RwLock<Vec<LoginProvider>>> = Arc::new(RwLock::new(Vec::new()));
}

/// Fetch available login providers from GFN service URLs API
pub async fn fetch_login_providers() -> Result<Vec<LoginProvider>> {
    info!("Fetching login providers from {}", SERVICE_URLS_ENDPOINT);

    let client = reqwest::Client::builder()
        .user_agent(GFN_USER_AGENT)
        .build()?;

    let response = client
        .get(SERVICE_URLS_ENDPOINT)
        .header("Accept", "application/json")
        .send()
        .await
        .context("Failed to fetch service URLs")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("Service URLs request failed: {} - {}", status, body));
    }

    let service_response: ServiceUrlsResponse = response.json().await
        .context("Failed to parse service URLs response")?;

    if service_response.request_status.status_code != 1 {
        return Err(anyhow::anyhow!("Service URLs API error: status_code={}",
            service_response.request_status.status_code));
    }

    let service_info = service_response.gfn_service_info
        .ok_or_else(|| anyhow::anyhow!("No service info in response"))?;

    let mut providers: Vec<LoginProvider> = service_info.gfn_service_endpoints
        .into_iter()
        .map(|ep| {
            // Rename "Brothers Pictures" to "bro.game"
            let display_name = if ep.login_provider_code == "BPC" {
                "bro.game".to_string()
            } else {
                ep.login_provider_display_name
            };

            LoginProvider {
                idp_id: ep.idp_id,
                login_provider_code: ep.login_provider_code,
                login_provider_display_name: display_name,
                login_provider: ep.login_provider,
                streaming_service_url: ep.streaming_service_url,
                login_provider_priority: ep.login_provider_priority,
            }
        })
        .collect();

    // Sort by priority
    providers.sort_by_key(|p| p.login_provider_priority);

    info!("Found {} login providers", providers.len());
    for provider in &providers {
        debug!("  - {} ({})", provider.login_provider_display_name, provider.login_provider_code);
    }

    // Cache providers
    {
        let mut cache = CACHED_PROVIDERS.write();
        *cache = providers.clone();
    }

    Ok(providers)
}

/// Get cached login providers
pub fn get_cached_providers() -> Vec<LoginProvider> {
    CACHED_PROVIDERS.read().clone()
}

/// Set the selected login provider
pub fn set_login_provider(provider: LoginProvider) {
    info!("Setting login provider to: {} ({})",
        provider.login_provider_display_name, provider.idp_id);
    
    // Save to cache for persistence across restarts
    crate::app::cache::save_login_provider(&provider);
    
    let mut selected = SELECTED_PROVIDER.write();
    *selected = Some(provider);
}

/// Get the selected login provider (or default NVIDIA)
pub fn get_selected_provider() -> LoginProvider {
    SELECTED_PROVIDER.read()
        .clone()
        .unwrap_or_else(LoginProvider::nvidia_default)
}

/// Get the streaming base URL for the selected provider
pub fn get_streaming_base_url() -> String {
    let provider = get_selected_provider();
    let url = provider.streaming_service_url;
    if url.ends_with('/') { url } else { format!("{}/", url) }
}

/// Clear the selected provider (reset to NVIDIA default)
pub fn clear_login_provider() {
    let mut selected = SELECTED_PROVIDER.write();
    *selected = None;
}

// ============================================
// Authentication Tokens
// ============================================

/// Authentication tokens
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub id_token: Option<String>,
    pub expires_at: i64,
}

impl AuthTokens {
    /// Check if token is expired
    pub fn is_expired(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        now >= self.expires_at
    }

    /// Check if token should be refreshed (expires within 10 minutes)
    pub fn should_refresh(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        // Refresh if less than 10 minutes (600 seconds) remaining
        self.expires_at - now < 600
    }

    /// Check if we have a refresh token available
    pub fn can_refresh(&self) -> bool {
        self.refresh_token.is_some()
    }

    /// Get the JWT token for API calls (id_token if available, else access_token)
    pub fn jwt(&self) -> &str {
        self.id_token.as_deref().unwrap_or(&self.access_token)
    }

    /// Extract user_id from the JWT token
    pub fn user_id(&self) -> String {
        // Try to extract user_id from JWT payload
        let token = self.jwt();
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() == 3 {
            let payload_b64 = parts[1];
            // Add padding if needed
            let padded = match payload_b64.len() % 4 {
                2 => format!("{}==", payload_b64),
                3 => format!("{}=", payload_b64),
                _ => payload_b64.to_string(),
            };
            if let Ok(payload_bytes) = URL_SAFE_NO_PAD.decode(&padded)
                .or_else(|_| base64::engine::general_purpose::STANDARD.decode(&padded))
            {
                if let Ok(payload_str) = String::from_utf8(payload_bytes) {
                    #[derive(Deserialize)]
                    struct JwtSub { sub: String }
                    if let Ok(payload) = serde_json::from_str::<JwtSub>(&payload_str) {
                        return payload.sub;
                    }
                }
            }
        }
        // Fallback
        "unknown".to_string()
    }
}

/// User info from JWT or userinfo endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub user_id: String,
    pub display_name: String,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
    pub membership_tier: String,
}

// ============================================
// PKCE Challenge
// ============================================

/// PKCE code verifier and challenge
pub struct PkceChallenge {
    pub verifier: String,
    pub challenge: String,
}

impl PkceChallenge {
    /// Generate a new PKCE challenge
    pub fn new() -> Self {
        // Generate random 64-character verifier
        let verifier: String = (0..64)
            .map(|_| {
                let idx = rand::random::<usize>() % 62;
                "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789"
                    .chars()
                    .nth(idx)
                    .unwrap()
            })
            .collect();

        // Generate SHA256 challenge
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());

        Self { verifier, challenge }
    }
}

impl Default for PkceChallenge {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================
// OAuth Flow
// ============================================

/// Find an available port for OAuth callback
pub fn find_available_port() -> Option<u16> {
    for port in REDIRECT_PORTS {
        if std::net::TcpListener::bind(format!("127.0.0.1:{}", port)).is_ok() {
            return Some(port);
        }
    }
    None
}

/// Build the OAuth authorization URL with provider-specific IDP ID
pub fn build_auth_url(pkce: &PkceChallenge, port: u16) -> String {
    let provider = get_selected_provider();
    let redirect_uri = format!("http://localhost:{}", port);
    let nonce = generate_nonce();
    let device_id = get_device_id();

    format!(
        "https://login.nvidia.com/authorize?\
        response_type=code&\
        device_id={}&\
        scope={}&\
        client_id={}&\
        redirect_uri={}&\
        ui_locales=en_US&\
        nonce={}&\
        prompt=select_account&\
        code_challenge={}&\
        code_challenge_method=S256&\
        idp_id={}",
        device_id,
        urlencoding::encode(SCOPES),
        CLIENT_ID,
        urlencoding::encode(&redirect_uri),
        nonce,
        pkce.challenge,
        provider.idp_id
    )
}

/// Generate a UUID-like nonce
fn generate_nonce() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    let mut hasher = Sha256::new();
    hasher.update(timestamp.to_le_bytes());
    hasher.update(std::process::id().to_le_bytes());
    hasher.update(b"nonce");
    let hash = hasher.finalize();

    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        u32::from_le_bytes([hash[0], hash[1], hash[2], hash[3]]),
        u16::from_le_bytes([hash[4], hash[5]]),
        u16::from_le_bytes([hash[6], hash[7]]),
        u16::from_le_bytes([hash[8], hash[9]]),
        u64::from_le_bytes([hash[10], hash[11], hash[12], hash[13], hash[14], hash[15], 0, 0]) & 0xffffffffffff
    )
}

/// Get or generate device ID
fn get_device_id() -> String {
    // Try to read from official GFN client config
    if let Some(app_data) = std::env::var_os("LOCALAPPDATA") {
        let gfn_config = std::path::PathBuf::from(app_data)
            .join("NVIDIA Corporation")
            .join("GeForceNOW")
            .join("sharedstorage.json");

        if gfn_config.exists() {
            if let Ok(content) = std::fs::read_to_string(&gfn_config) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(device_id) = json.get("gfnTelemetry")
                        .and_then(|t| t.get("deviceId"))
                        .and_then(|d| d.as_str()) {
                        return device_id.to_string();
                    }
                }
            }
        }
    }

    // Generate stable device ID
    let mut hasher = Sha256::new();
    if let Ok(hostname) = std::env::var("COMPUTERNAME") {
        hasher.update(hostname.as_bytes());
    }
    if let Ok(username) = std::env::var("USERNAME") {
        hasher.update(username.as_bytes());
    }
    hasher.update(b"opennow-streamer");
    hex::encode(hasher.finalize())
}

/// Exchange authorization code for tokens
pub async fn exchange_code(code: &str, verifier: &str, port: u16) -> Result<AuthTokens> {
    let redirect_uri = format!("http://localhost:{}", port);

    info!("Exchanging authorization code for tokens...");

    let client = reqwest::Client::builder()
        .user_agent(GFN_USER_AGENT)
        .build()?;

    // Official client does NOT include client_id in token request
    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri.as_str()),
        ("code_verifier", verifier),
    ];

    let response = client
        .post("https://login.nvidia.com/token")
        .header("Content-Type", "application/x-www-form-urlencoded; charset=UTF-8")
        .header("Origin", CEF_ORIGIN)
        .header("Referer", format!("{}/", CEF_ORIGIN))
        .header("Accept", "application/json, text/plain, */*")
        .form(&params)
        .send()
        .await
        .context("Token exchange request failed")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("Token exchange failed: {} - {}", status, error_text));
    }

    #[derive(Deserialize)]
    struct TokenResponse {
        access_token: String,
        refresh_token: Option<String>,
        id_token: Option<String>,
        expires_in: Option<i64>,
    }

    let token_response: TokenResponse = response.json().await
        .context("Failed to parse token response")?;

    let expires_at = chrono::Utc::now().timestamp()
        + token_response.expires_in.unwrap_or(86400);

    info!("Token exchange successful!");

    Ok(AuthTokens {
        access_token: token_response.access_token,
        refresh_token: token_response.refresh_token,
        id_token: token_response.id_token,
        expires_at,
    })
}

/// Refresh an expired token
pub async fn refresh_token(refresh_token: &str) -> Result<AuthTokens> {
    let client = reqwest::Client::builder()
        .user_agent(GFN_USER_AGENT)
        .build()?;

    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", CLIENT_ID),
    ];

    let response = client
        .post("https://login.nvidia.com/token")
        .header("Content-Type", "application/x-www-form-urlencoded; charset=UTF-8")
        .header("Origin", CEF_ORIGIN)
        .header("Accept", "application/json, text/plain, */*")
        .form(&params)
        .send()
        .await
        .context("Token refresh request failed")?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("Token refresh failed: {}", error_text));
    }

    #[derive(Deserialize)]
    struct TokenResponse {
        access_token: String,
        refresh_token: Option<String>,
        id_token: Option<String>,
        expires_in: Option<i64>,
    }

    let token_response: TokenResponse = response.json().await
        .context("Failed to parse refresh response")?;

    let expires_at = chrono::Utc::now().timestamp()
        + token_response.expires_in.unwrap_or(86400);

    Ok(AuthTokens {
        access_token: token_response.access_token,
        refresh_token: token_response.refresh_token,
        id_token: token_response.id_token,
        expires_at,
    })
}

/// Decode JWT and extract user info
pub fn decode_jwt_user_info(token: &str) -> Result<UserInfo> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(anyhow::anyhow!("Invalid JWT format"));
    }

    let payload_b64 = parts[1];
    let padded = match payload_b64.len() % 4 {
        2 => format!("{}==", payload_b64),
        3 => format!("{}=", payload_b64),
        _ => payload_b64.to_string(),
    };

    let payload_bytes = URL_SAFE_NO_PAD.decode(&padded)
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(&padded))
        .context("Failed to decode JWT payload")?;

    let payload_str = String::from_utf8(payload_bytes)
        .context("Invalid UTF-8 in JWT")?;

    #[derive(Deserialize)]
    struct JwtPayload {
        sub: String,
        email: Option<String>,
        preferred_username: Option<String>,
        gfn_tier: Option<String>,
        picture: Option<String>,
    }

    let payload: JwtPayload = serde_json::from_str(&payload_str)
        .context("Failed to parse JWT payload")?;

    let display_name = payload.preferred_username
        .or_else(|| payload.email.as_ref().map(|e| e.split('@').next().unwrap_or("User").to_string()))
        .unwrap_or_else(|| "User".to_string());

    let membership_tier = payload.gfn_tier.unwrap_or_else(|| "FREE".to_string());

    Ok(UserInfo {
        user_id: payload.sub,
        display_name,
        email: payload.email,
        avatar_url: payload.picture,
        membership_tier,
    })
}

/// Fetch user info from /userinfo endpoint
pub async fn fetch_userinfo(access_token: &str) -> Result<UserInfo> {
    let client = reqwest::Client::builder()
        .user_agent(GFN_USER_AGENT)
        .build()?;

    let response = client
        .get("https://login.nvidia.com/userinfo")
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Origin", CEF_ORIGIN)
        .header("Accept", "application/json")
        .send()
        .await
        .context("Userinfo request failed")?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!("Userinfo failed: {}", response.status()));
    }

    #[derive(Deserialize)]
    struct UserinfoResponse {
        sub: String,
        preferred_username: Option<String>,
        email: Option<String>,
        picture: Option<String>,
    }

    let userinfo: UserinfoResponse = response.json().await
        .context("Failed to parse userinfo")?;

    let display_name = userinfo.preferred_username
        .or_else(|| userinfo.email.as_ref().map(|e| e.split('@').next().unwrap_or("User").to_string()))
        .unwrap_or_else(|| "User".to_string());

    Ok(UserInfo {
        user_id: userinfo.sub,
        display_name,
        email: userinfo.email,
        avatar_url: userinfo.picture,
        membership_tier: "FREE".to_string(), // /userinfo doesn't return tier
    })
}

/// Get user info from tokens (prefer id_token JWT, fallback to /userinfo)
pub async fn get_user_info(tokens: &AuthTokens) -> Result<UserInfo> {
    // Try id_token first
    if let Some(ref id_token) = tokens.id_token {
        if let Ok(user) = decode_jwt_user_info(id_token) {
            return Ok(user);
        }
    }

    // Fallback to /userinfo
    fetch_userinfo(&tokens.access_token).await
}

/// Start OAuth callback server and wait for code
pub async fn start_callback_server(port: u16) -> Result<String> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await
        .context("Failed to bind callback server")?;

    info!("OAuth callback server listening on http://127.0.0.1:{}", port);

    let (mut socket, _) = listener.accept().await
        .context("Failed to accept connection")?;

    let mut reader = BufReader::new(&mut socket);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).await?;

    // Parse the code from: GET /callback?code=abc123 HTTP/1.1
    let code = request_line
        .split_whitespace()
        .nth(1)
        .and_then(|path| {
            path.split('?')
                .nth(1)
                .and_then(|query| {
                    query.split('&')
                        .find(|param| param.starts_with("code="))
                        .map(|param| param.trim_start_matches("code=").to_string())
                })
        })
        .context("No authorization code in callback")?;

    // Send success response
    let response = r#"HTTP/1.1 200 OK
Content-Type: text/html

<!DOCTYPE html>
<html>
<head>
    <title>Login Successful</title>
    <style>
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
            background: linear-gradient(135deg, #1a1a2e 0%, #16213e 100%);
            color: #fff;
            display: flex;
            justify-content: center;
            align-items: center;
            height: 100vh;
            margin: 0;
        }
        .container {
            text-align: center;
            padding: 40px;
            background: rgba(255,255,255,0.1);
            border-radius: 16px;
        }
        h1 { color: #76b900; }
    </style>
</head>
<body>
    <div class="container">
        <h1>Login Successful!</h1>
        <p>You can close this window and return to OpenNow Streamer.</p>
    </div>
    <script>setTimeout(() => window.close(), 3000);</script>
</body>
</html>"#;

    socket.write_all(response.as_bytes()).await?;

    Ok(code)
}
