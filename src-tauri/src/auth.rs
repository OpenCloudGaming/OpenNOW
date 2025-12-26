use serde::{Deserialize, Serialize};
use tauri::command;
use reqwest::Client;
use chrono::{DateTime, Utc};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Sha256, Digest};
use std::path::PathBuf;
use std::fs;

/// Authentication state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthState {
    pub is_authenticated: bool,
    pub user: Option<User>,
    pub tokens: Option<Tokens>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub user_id: String,
    pub display_name: String,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
    pub membership_tier: MembershipTier,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MembershipTier {
    Free,
    Priority,
    Ultimate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub id_token: Option<String>,
    pub expires_at: DateTime<Utc>,
}

/// OAuth login response from Starfleet
#[derive(Debug, Deserialize)]
struct StarfleetTokenResponse {
    access_token: String,
    #[allow(dead_code)]
    token_type: String,
    expires_in: i64,
    refresh_token: Option<String>,
    id_token: Option<String>,
}

/// Jarvis user info response
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct JarvisUserInfo {
    sub: String,
    preferred_username: String,
    email: Option<String>,
    picture: Option<String>,
}

/// OAuth configuration for NVIDIA Starfleet
/// The official GFN client uses static-login.nvidia.com which redirects to the proper OAuth flow
#[allow(dead_code)]
const STARFLEET_AUTH_URL: &str = "https://static-login.nvidia.com/service/gfn/login-start";

// Token endpoint - discovered from Burp Suite capture of official client
// The official client POSTs to https://login.nvidia.com/token (NOT /oauth/token!)
const STARFLEET_TOKEN_URL: &str = "https://login.nvidia.com/token";

const LOGOUT_URL: &str = "https://static-login.nvidia.com/service/gfn/logout-start";

/// Starfleet Client ID from GFN client - this is for the public NVIDIA login
const STARFLEET_CLIENT_ID: &str = "ZU7sPN-miLujMD95LfOQ453IB0AtjM8sMyvgJ9wCXEQ";

/// OAuth scopes required by GFN
const OAUTH_SCOPES: &str = "openid consent email tk_client age";

/// Available redirect ports (from GFN config)
const REDIRECT_PORTS: [u16; 5] = [2259, 6460, 7119, 8870, 9096];

/// Token refresh duration: 27 days in milliseconds
#[allow(dead_code)]
const TOKEN_REFRESH_DURATION_MS: i64 = 2332800000;
/// Refresh threshold: 30% of remaining lifetime
#[allow(dead_code)]
const REFRESH_THRESHOLD_PERCENT: f64 = 0.30;

/// CEF Origin used by official client (required for CORS to work)
const CEF_ORIGIN: &str = "https://nvfile";

/// GFN CEF User-Agent (from Burp capture)
const GFN_CEF_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36 NVIDIACEFClient/HEAD/debb5919f6 GFN-PC/2.0.80.173";

/// IDP ID for NVIDIA identity provider
const IDP_ID: &str = "PDiAhv2kJTFeQ7WOPqiQ2tRZ7lGhR2X11dXvM4TZSxg";

/// Userinfo endpoint (from Burp capture - used when id_token is not available)
const USERINFO_URL: &str = "https://login.nvidia.com/userinfo";

/// Get or generate a stable device ID (SHA256 hash)
fn get_device_id() -> String {
    // Try to read device_id from official GFN client config
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
                        log::info!("Using device_id from official GFN client: {}", device_id);
                        return device_id.to_string();
                    }
                }
            }
        }
    }

    // Generate a stable device ID based on machine info
    generate_stable_device_id()
}

/// Generate a stable device ID based on machine identifiers
fn generate_stable_device_id() -> String {
    let mut hasher = Sha256::new();

    // Use hostname and username for a semi-stable ID
    if let Ok(hostname) = std::env::var("COMPUTERNAME") {
        hasher.update(hostname.as_bytes());
    }
    if let Ok(username) = std::env::var("USERNAME") {
        hasher.update(username.as_bytes());
    }
    // Add a salt specific to this app
    hasher.update(b"gfn-custom-client");

    let result = hasher.finalize();
    hex::encode(result)
}

/// Find an available port from the allowed redirect ports
fn find_available_port() -> Option<u16> {
    for port in REDIRECT_PORTS {
        if std::net::TcpListener::bind(format!("127.0.0.1:{}", port)).is_ok() {
            return Some(port);
        }
    }
    None
}

/// Generate a random string of specified length (for PKCE and state)
fn generate_random_string(len: usize) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    // Use timestamp + counter for pseudo-randomness
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    // Create a longer seed by hashing
    let mut hasher = Sha256::new();
    hasher.update(timestamp.to_le_bytes());
    hasher.update(std::process::id().to_le_bytes());
    let hash = hasher.finalize();

    URL_SAFE_NO_PAD.encode(&hash[..len.min(32)])
        .chars()
        .take(len)
        .collect()
}

/// Generate PKCE code verifier (43-128 characters)
fn generate_code_verifier() -> String {
    // Generate a 64 character random string
    let mut result = generate_random_string(32);
    // Append more randomness
    result.push_str(&generate_random_string(32));
    result.chars().take(64).collect()
}

/// Generate PKCE code challenge from verifier (S256 method)
fn generate_code_challenge(verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let hash = hasher.finalize();
    URL_SAFE_NO_PAD.encode(hash)
}

/// Generate a nonce for OpenID Connect (UUID format like official client)
fn generate_nonce() -> String {
    // Generate UUID-like format: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
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

    // Format as UUID
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        u32::from_le_bytes([hash[0], hash[1], hash[2], hash[3]]),
        u16::from_le_bytes([hash[4], hash[5]]),
        u16::from_le_bytes([hash[6], hash[7]]),
        u16::from_le_bytes([hash[8], hash[9]]),
        u64::from_le_bytes([hash[10], hash[11], hash[12], hash[13], hash[14], hash[15], 0, 0]) & 0xffffffffffff
    )
}

/// Global auth state storage
static AUTH_STATE: std::sync::OnceLock<Arc<Mutex<Option<AuthState>>>> = std::sync::OnceLock::new();

fn get_auth_storage() -> Arc<Mutex<Option<AuthState>>> {
    AUTH_STATE.get_or_init(|| {
        // Try to load saved auth state on first access
        let saved_state = load_auth_from_file();
        Arc::new(Mutex::new(saved_state))
    }).clone()
}

/// Get the path to the auth storage file
fn get_auth_file_path() -> PathBuf {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."));
    let app_dir = config_dir.join("gfn-client");
    fs::create_dir_all(&app_dir).ok();
    app_dir.join("auth.json")
}

/// Save auth state to file
fn save_auth_to_file(state: &AuthState) {
    let path = get_auth_file_path();
    if let Ok(json) = serde_json::to_string_pretty(state) {
        if let Err(e) = fs::write(&path, json) {
            log::warn!("Failed to save auth state: {}", e);
        } else {
            log::info!("Auth state saved to {:?}", path);
        }
    }
}

/// Load auth state from file
fn load_auth_from_file() -> Option<AuthState> {
    let path = get_auth_file_path();
    if path.exists() {
        match fs::read_to_string(&path) {
            Ok(json) => {
                match serde_json::from_str::<AuthState>(&json) {
                    Ok(state) => {
                        // Validate the token is not expired
                        if let Some(tokens) = &state.tokens {
                            if tokens.expires_at > Utc::now() {
                                log::info!("Loaded saved auth state from {:?}", path);
                                return Some(state);
                            } else {
                                log::info!("Saved token expired, clearing auth file");
                                // Clear the expired auth file
                                if let Err(e) = fs::remove_file(&path) {
                                    log::warn!("Failed to remove expired auth file: {}", e);
                                }
                            }
                        }
                    }
                    Err(e) => log::warn!("Failed to parse auth file: {}", e),
                }
            }
            Err(e) => log::warn!("Failed to read auth file: {}", e),
        }
    }
    None
}

/// Clear saved auth state
fn clear_auth_file() {
    let path = get_auth_file_path();
    if path.exists() {
        if let Err(e) = fs::remove_file(&path) {
            log::warn!("Failed to remove auth file: {}", e);
        }
    }
}

/// Parse query string from URL
fn parse_query_string(query: &str) -> std::collections::HashMap<String, String> {
    query
        .split('&')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next()?;
            let value = parts.next().unwrap_or("");
            Some((
                urlencoding::decode(key).ok()?.into_owned(),
                urlencoding::decode(value).ok()?.into_owned(),
            ))
        })
        .collect()
}

/// HTML response for successful login
const SUCCESS_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
    <title>Login Successful</title>
    <style>
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
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
            backdrop-filter: blur(10px);
        }
        h1 { color: #76b900; margin-bottom: 10px; }
        p { color: #aaa; }
    </style>
</head>
<body>
    <div class="container">
        <h1>Login Successful!</h1>
        <p>You can close this window and return to the GFN Client.</p>
    </div>
    <script>setTimeout(() => window.close(), 3000);</script>
</body>
</html>"#;

/// HTML response for failed login
const ERROR_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
    <title>Login Failed</title>
    <style>
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
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
            backdrop-filter: blur(10px);
        }
        .error-icon {
            font-size: 64px;
            color: #ff4444;
            margin-bottom: 20px;
        }
        h1 { color: #ff4444; margin-bottom: 10px; }
        p { color: #aaa; }
    </style>
</head>
<body>
    <div class="container">
        <div class="error-icon">✗</div>
        <h1>Login Failed</h1>
        <p>Please try again or check your credentials.</p>
    </div>
</body>
</html>"#;

/// Start OAuth login flow - opens GFN web to let user login
/// Then user can extract their session tokens from the browser
#[command]
pub async fn login() -> Result<AuthState, String> {
    log::info!("Starting OAuth login flow (GFN Web Login)");

    // Open the GFN web client for user to login
    // The user will login there and we'll provide instructions to extract tokens
    let gfn_url = "https://play.geforcenow.com";

    log::info!("Opening GFN web client for authentication");
    if let Err(e) = open::that(gfn_url) {
        log::warn!("Failed to open browser: {}", e);
    }

    // For now, return a message indicating manual token entry is needed
    // In a future update, we could use browser automation or extension
    Err("Please login at play.geforcenow.com, then use 'Set Token' in settings to enter your access token. You can find it in browser DevTools > Application > Local Storage > NVAUTHTOKEN".to_string())
}

/// Set access token manually (for users who extract it from browser)
#[command]
pub async fn set_access_token(token: String) -> Result<AuthState, String> {
    log::info!("Setting access token manually");

    // Try to get user info - first try JWT decode, then /userinfo endpoint
    let user = match decode_jwt_user_info(&token) {
        Ok(user) => user,
        Err(_) => {
            log::info!("Token is not a JWT, trying /userinfo endpoint...");
            fetch_userinfo(&token).await?
        }
    };

    let auth_state = AuthState {
        is_authenticated: true,
        user: Some(user),
        tokens: Some(Tokens {
            access_token: token,
            refresh_token: None,
            id_token: None,
            expires_at: Utc::now() + chrono::Duration::days(27), // Assume 27 day expiry
        }),
    };

    // Store auth state in memory
    {
        let storage = get_auth_storage();
        let mut guard = storage.lock().await;
        *guard = Some(auth_state.clone());
    }

    // Persist to file
    save_auth_to_file(&auth_state);

    log::info!("Token validated and stored successfully");
    Ok(auth_state)
}

/// Get the current access token if authenticated
/// Note: For GFN API calls, use get_gfn_jwt() instead which returns the id_token
#[command]
pub async fn get_access_token() -> Result<String, String> {
    let storage = get_auth_storage();
    let guard = storage.lock().await;

    guard.as_ref()
        .and_then(|state| state.tokens.as_ref())
        .map(|tokens| tokens.access_token.clone())
        .ok_or_else(|| "Not authenticated - please login first".to_string())
}

/// Get the GFN JWT token for API calls (this is the id_token, which is a JWT)
/// The GFN API (games.geforce.com) expects a JWT with the GFNJWT auth scheme
#[command]
pub async fn get_gfn_jwt() -> Result<String, String> {
    let storage = get_auth_storage();
    let guard = storage.lock().await;

    guard.as_ref()
        .and_then(|state| state.tokens.as_ref())
        .and_then(|tokens| tokens.id_token.clone())
        .ok_or_else(|| "Not authenticated or no JWT token available - please login first".to_string())
}

/// OAuth callback result - can be either authorization code or implicit token
enum OAuthCallbackResult {
    Code(String),
    Token { access_token: String, expires_in: Option<i64>, id_token: Option<String> },
}

/// Extract OAuth callback data - handles both code and token responses
fn extract_oauth_callback(request: &str) -> Option<OAuthCallbackResult> {
    // Parse the GET request line
    let first_line = request.lines().next()?;
    let path = first_line.split_whitespace().nth(1)?;

    // The token might be in a URL fragment (#) which browsers don't send to server
    // So we serve an HTML page that extracts it and posts it back

    // Check for query string
    let query_start = match path.find('?') {
        Some(pos) => pos,
        None => return None,
    };

    let query = &path[query_start + 1..];
    let params = parse_query_string(query);

    log::debug!("Parsing OAuth callback, path: {}, params: {:?}", path, params.keys().collect::<Vec<_>>());

    // Check for error response first
    if let Some(error) = params.get("error") {
        log::error!("OAuth error: {}", error);
        if let Some(desc) = params.get("error_description") {
            log::error!("Error description: {}", desc);
        }
        return None;
    }

    // Check for implicit token first (from our /callback redirect)
    // This is higher priority since implicit flow should return token directly
    if let Some(token) = params.get("access_token") {
        log::info!("Found access_token in callback params");
        let expires_in = params.get("expires_in").and_then(|s| s.parse().ok());
        let id_token = params.get("id_token").cloned();
        return Some(OAuthCallbackResult::Token {
            access_token: token.clone(),
            expires_in,
            id_token,
        });
    }

    // Check for authorization code (fallback if NVIDIA returns code instead of token)
    if let Some(code) = params.get("code") {
        log::info!("Found authorization code in callback params");
        return Some(OAuthCallbackResult::Code(code.clone()));
    }

    None
}

/// NVIDIA OAuth login flow with localhost callback
/// Uses authorization code flow with PKCE (same as official GFN client)
#[command]
pub async fn login_oauth() -> Result<AuthState, String> {
    log::info!("=== Starting NVIDIA OAuth login flow (Authorization Code + PKCE) ===");

    // Find an available redirect port
    log::info!("Finding available port from: {:?}", REDIRECT_PORTS);
    let port = find_available_port()
        .ok_or_else(|| {
            log::error!("No available ports found!");
            "No available ports for OAuth callback. Ports 2259, 6460, 7119, 8870, 9096 are all in use.".to_string()
        })?;
    log::info!("Found available port: {}", port);

    let redirect_uri = format!("http://localhost:{}", port);
    let nonce = generate_nonce();

    // Generate PKCE code verifier and challenge (required by NVIDIA OAuth)
    let code_verifier = generate_code_verifier();
    let code_challenge = generate_code_challenge(&code_verifier);

    // Get device ID (from official client or generate)
    let device_id = get_device_id();
    log::info!("Using device_id: {}", device_id);

    // Build OAuth authorization URL using authorization code flow with PKCE
    // This matches the official GFN client format exactly
    let auth_url = format!(
        "https://login.nvidia.com/authorize?response_type=code&device_id={}&scope={}&client_id={}&redirect_uri={}&ui_locales=en_US&nonce={}&prompt=select_account&code_challenge={}&code_challenge_method=S256&idp_id={}",
        device_id,
        urlencoding::encode(OAUTH_SCOPES),
        STARFLEET_CLIENT_ID,
        urlencoding::encode(&redirect_uri),
        nonce,
        code_challenge,
        IDP_ID
    );

    log::info!("OAuth URL: {}", auth_url);

    // Start TCP listener FIRST so it's ready when browser redirects back
    log::info!("Starting OAuth callback server on port {}", port);
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port))
        .await
        .map_err(|e| {
            log::error!("Failed to bind to port {}: {}", port, e);
            format!("Failed to start callback server on port {}: {}", port, e)
        })?;
    log::info!("Callback server listening on port {}", port);

    // Open browser with auth URL
    log::info!("Opening browser for authentication...");
    log::info!("Full OAuth URL: {}", auth_url);

    // Use open crate which handles URL escaping properly on all platforms
    match open::that(&auth_url) {
        Ok(_) => log::info!("Browser opened successfully"),
        Err(e) => {
            log::error!("Failed to open browser: {}", e);
            return Err(format!("Failed to open browser: {}. Please open manually: {}", e, auth_url));
        }
    }

    log::info!("Waiting for OAuth callback...");

    // Clone values needed inside the async block
    let redirect_uri_clone = redirect_uri.clone();
    let code_verifier_clone = code_verifier.clone();

    // Wait for callback (with 5 minute timeout)
    let callback_result = tokio::time::timeout(
        std::time::Duration::from_secs(300),
        async move {
            loop {
                let (mut socket, _) = listener.accept().await
                    .map_err(|e| format!("Failed to accept connection: {}", e))?;

                let mut buffer = vec![0u8; 8192];
                let n = socket.read(&mut buffer).await
                    .map_err(|e| format!("Failed to read request: {}", e))?;

                let request = String::from_utf8_lossy(&buffer[..n]);
                log::debug!("Received callback request: {}", &request[..request.len().min(200)]);

                // Check if this is a valid OAuth callback with authorization code
                if let Some(result) = extract_oauth_callback(&request) {
                    match result {
                        OAuthCallbackResult::Token { access_token, expires_in, id_token } => {
                            log::info!("Received access token directly");

                            // Send success response
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}",
                                SUCCESS_HTML.len(),
                                SUCCESS_HTML
                            );
                            let _ = socket.write_all(response.as_bytes()).await;

                            let expires_at = Utc::now() + chrono::Duration::seconds(expires_in.unwrap_or(86400));
                            return Ok::<Tokens, String>(Tokens {
                                access_token,
                                refresh_token: None,
                                id_token,
                                expires_at,
                            });
                        }
                        OAuthCallbackResult::Code(code) => {
                            log::info!("Received authorization code, attempting token exchange with PKCE verifier");

                            // Try token exchange with the PKCE code_verifier
                            match exchange_code(&code, &redirect_uri_clone, &code_verifier_clone).await {
                                Ok(tokens) => {
                                    let response = format!(
                                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}",
                                        SUCCESS_HTML.len(),
                                        SUCCESS_HTML
                                    );
                                    let _ = socket.write_all(response.as_bytes()).await;
                                    return Ok(tokens);
                                }
                                Err(e) => {
                                    log::error!("Token exchange failed: {}", e);
                                    let response = format!(
                                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}",
                                        ERROR_HTML.len(),
                                        ERROR_HTML
                                    );
                                    let _ = socket.write_all(response.as_bytes()).await;
                                    return Err(format!("Token exchange failed: {}. Please use manual token entry.", e));
                                }
                            }
                        }
                    }
                }

                // Handle favicon and other requests - just return 200 for the main page
                let first_line = request.lines().next().unwrap_or("");
                let path = first_line.split_whitespace().nth(1).unwrap_or("");

                if path == "/favicon.ico" {
                    let response = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
                    let _ = socket.write_all(response.as_bytes()).await;
                } else {
                    // For any other request, return a simple waiting page
                    let waiting_html = r#"<!DOCTYPE html><html><head><title>Processing...</title></head><body style="background:#1a1a2e;color:#fff;display:flex;justify-content:center;align-items:center;height:100vh;font-family:sans-serif;"><h1>Processing login...</h1></body></html>"#;
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}",
                        waiting_html.len(),
                        waiting_html
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                }
            }
        }
    ).await;

    let tokens = match callback_result {
        Ok(Ok(tokens)) => tokens,
        Ok(Err(e)) => return Err(e),
        Err(_) => return Err("Login timeout - please try again".to_string()),
    };

    log::info!("Tokens received, fetching user info");

    // Get user info from tokens (prefer id_token which is JWT, fallback to /userinfo endpoint)
    let user = get_user_info_from_tokens(&tokens).await?;

    let auth_state = AuthState {
        is_authenticated: true,
        user: Some(user),
        tokens: Some(tokens),
    };

    // Store auth state
    {
        let storage = get_auth_storage();
        let mut guard = storage.lock().await;
        *guard = Some(auth_state.clone());
    }

    // Persist to file
    save_auth_to_file(&auth_state);

    log::info!("Login successful");
    Ok(auth_state)
}

/// Exchange authorization code for tokens (with PKCE code_verifier)
/// Uses exact same request format as official GFN client (captured via Burp Suite)
async fn exchange_code(code: &str, redirect_uri: &str, code_verifier: &str) -> Result<Tokens, String> {
    log::info!("Exchanging authorization code for tokens...");
    log::info!("Token endpoint: {}", STARFLEET_TOKEN_URL);
    log::info!("Redirect URI: {}", redirect_uri);

    let client = Client::builder()
        .user_agent(GFN_CEF_USER_AGENT)
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    // NOTE: Official client does NOT include client_id in token request!
    // Only: grant_type, code, redirect_uri, code_verifier
    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("code_verifier", code_verifier),
    ];

    log::info!("Sending token exchange request...");

    let response = client
        .post(STARFLEET_TOKEN_URL)
        .header("Content-Type", "application/x-www-form-urlencoded; charset=UTF-8")
        .header("Origin", CEF_ORIGIN)
        .header("Referer", format!("{}/", CEF_ORIGIN))
        .header("Accept", "application/json, text/plain, */*")
        .header("Sec-Fetch-Site", "cross-site")
        .header("Sec-Fetch-Mode", "cors")
        .header("Sec-Fetch-Dest", "empty")
        .form(&params)
        .send()
        .await;

    match response {
        Ok(resp) => {
            let status = resp.status();
            log::info!("Token exchange response status: {}", status);

            if status.is_success() {
                let token_response: StarfleetTokenResponse = resp
                    .json()
                    .await
                    .map_err(|e| format!("Failed to parse token response: {}", e))?;

                let expires_at = Utc::now() + chrono::Duration::seconds(token_response.expires_in);

                log::info!("Token exchange successful! Token expires in {} seconds", token_response.expires_in);
                return Ok(Tokens {
                    access_token: token_response.access_token,
                    refresh_token: token_response.refresh_token,
                    id_token: token_response.id_token,
                    expires_at,
                });
            } else {
                let body = resp.text().await.unwrap_or_default();
                let error_msg = format!("Token exchange failed with status {}: {}", status, body);
                log::error!("{}", error_msg);
                return Err(error_msg);
            }
        }
        Err(e) => {
            let error_msg = format!("Token exchange request failed: {}", e);
            log::error!("{}", error_msg);
            return Err(error_msg);
        }
    }
}

/// JWT payload structure (decoded from id_token)
#[derive(Debug, Deserialize)]
struct JwtPayload {
    sub: String,
    email: Option<String>,
    preferred_username: Option<String>,
    #[serde(default)]
    exp: i64,
    // GFN-specific claims
    #[serde(rename = "gfn_tier")]
    gfn_tier: Option<String>,
    picture: Option<String>,
}

/// Userinfo endpoint response (from /userinfo API call)
#[derive(Debug, Deserialize)]
struct UserinfoResponse {
    sub: String,
    preferred_username: Option<String>,
    email: Option<String>,
    email_verified: Option<bool>,
    picture: Option<String>,
}

/// Get user info from tokens - prefer id_token (JWT), fallback to /userinfo endpoint
/// NVIDIA's access_token is NOT a JWT, but id_token is
async fn get_user_info_from_tokens(tokens: &Tokens) -> Result<User, String> {
    // First try to decode id_token if available (it's a JWT)
    if let Some(id_token) = &tokens.id_token {
        log::info!("Attempting to decode id_token as JWT...");
        match decode_jwt_user_info(id_token) {
            Ok(user) => {
                log::info!("Successfully decoded user info from id_token");
                return Ok(user);
            }
            Err(e) => {
                log::warn!("Failed to decode id_token: {}, falling back to /userinfo endpoint", e);
            }
        }
    } else {
        log::info!("No id_token available, will call /userinfo endpoint");
    }

    // Fallback: call /userinfo endpoint with access_token as Bearer
    log::info!("Fetching user info from /userinfo endpoint...");
    fetch_userinfo(&tokens.access_token).await
}

/// Fetch user info from NVIDIA's /userinfo endpoint
async fn fetch_userinfo(access_token: &str) -> Result<User, String> {
    let client = Client::builder()
        .user_agent(GFN_CEF_USER_AGENT)
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .get(USERINFO_URL)
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Origin", CEF_ORIGIN)
        .header("Referer", format!("{}/", CEF_ORIGIN))
        .header("Accept", "application/json, text/plain, */*")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch userinfo: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Userinfo request failed with status {}: {}", status, body));
    }

    let userinfo: UserinfoResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse userinfo response: {}", e))?;

    log::info!("Userinfo response: user_id={}, username={:?}", userinfo.sub, userinfo.preferred_username);

    // Convert to User struct
    let display_name = userinfo.preferred_username
        .or_else(|| userinfo.email.as_ref().map(|e| e.split('@').next().unwrap_or("User").to_string()))
        .unwrap_or_else(|| "User".to_string());

    Ok(User {
        user_id: userinfo.sub,
        display_name,
        email: userinfo.email,
        avatar_url: userinfo.picture,
        membership_tier: MembershipTier::Free, // /userinfo doesn't return tier, default to Free
    })
}

/// Decode JWT and extract user info (used for id_token)
fn decode_jwt_user_info(token: &str) -> Result<User, String> {
    // JWT format: header.payload.signature
    let parts: Vec<&str> = token.split('.').collect();

    if parts.len() != 3 {
        return Err("Invalid JWT token format".to_string());
    }

    // Decode the payload (second part)
    let payload_b64 = parts[1];

    // Add padding if needed for base64 decoding
    let padded = match payload_b64.len() % 4 {
        2 => format!("{}==", payload_b64),
        3 => format!("{}=", payload_b64),
        _ => payload_b64.to_string(),
    };

    // Use URL-safe base64 decoding (JWT uses URL-safe base64)
    let payload_bytes = URL_SAFE_NO_PAD.decode(&padded)
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(&padded))
        .map_err(|e| format!("Failed to decode JWT payload: {}", e))?;

    let payload_str = String::from_utf8(payload_bytes)
        .map_err(|e| format!("Invalid UTF-8 in JWT payload: {}", e))?;

    let payload: JwtPayload = serde_json::from_str(&payload_str)
        .map_err(|e| format!("Failed to parse JWT payload: {}", e))?;

    // Check if token is expired
    let now = Utc::now().timestamp();
    if payload.exp > 0 && payload.exp < now {
        return Err("Token has expired".to_string());
    }

    // Parse membership tier from JWT claims
    let membership_tier = match payload.gfn_tier.as_deref() {
        Some("PRIORITY") | Some("priority") => MembershipTier::Priority,
        Some("ULTIMATE") | Some("ultimate") => MembershipTier::Ultimate,
        _ => MembershipTier::Free,
    };

    // Extract display name from email or preferred_username
    let display_name = payload.preferred_username
        .or_else(|| payload.email.as_ref().map(|e| e.split('@').next().unwrap_or("User").to_string()))
        .unwrap_or_else(|| "User".to_string());

    Ok(User {
        user_id: payload.sub,
        display_name,
        email: payload.email,
        avatar_url: payload.picture,
        membership_tier,
    })
}

/// Logout and clear tokens
#[command]
pub async fn logout() -> Result<(), String> {
    log::info!("Logging out");

    // Clear stored auth state
    {
        let storage = get_auth_storage();
        let mut guard = storage.lock().await;
        *guard = None;
    }

    // Clear saved auth file
    clear_auth_file();

    // Optionally open NVIDIA logout URL
    let _ = open::that(LOGOUT_URL);

    Ok(())
}

/// Get current authentication status
#[command]
pub async fn get_auth_status() -> Result<AuthState, String> {
    // First, check if we need to refresh the token
    let needs_refresh = {
        let storage = get_auth_storage();
        let guard = storage.lock().await;

        match &*guard {
            Some(state) => {
                if let Some(tokens) = &state.tokens {
                    if should_refresh_token(tokens) {
                        tokens.refresh_token.clone()
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            None => None,
        }
    };

    // If we need to refresh, do it outside the lock
    if let Some(refresh) = needs_refresh {
        match refresh_token(refresh).await {
            Ok(new_tokens) => {
                let storage = get_auth_storage();
                let mut guard = storage.lock().await;
                if let Some(state) = guard.as_mut() {
                    state.tokens = Some(new_tokens);
                }
            }
            Err(_) => {
                // Refresh failed, need to re-login
                return Ok(AuthState {
                    is_authenticated: false,
                    user: None,
                    tokens: None,
                });
            }
        }
    }

    // Now return the current state
    let storage = get_auth_storage();
    let guard = storage.lock().await;

    match &*guard {
        Some(state) => Ok(state.clone()),
        None => Ok(AuthState {
            is_authenticated: false,
            user: None,
            tokens: None,
        }),
    }
}

/// Check if token needs refresh based on GFN refresh threshold (30% remaining)
pub fn should_refresh_token(tokens: &Tokens) -> bool {
    let now = Utc::now();
    let total_lifetime = chrono::Duration::milliseconds(TOKEN_REFRESH_DURATION_MS);
    let remaining = tokens.expires_at - now;

    if remaining <= chrono::Duration::zero() {
        return true; // Already expired
    }

    let threshold = total_lifetime.num_milliseconds() as f64 * REFRESH_THRESHOLD_PERCENT;
    remaining.num_milliseconds() < threshold as i64
}

/// Refresh access token using Starfleet
#[command]
pub async fn refresh_token(refresh_token: String) -> Result<Tokens, String> {
    let client = Client::builder()
        .user_agent(GFN_CEF_USER_AGENT)
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    // Refresh token request - may need client_id, keeping for now
    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", &refresh_token),
        ("client_id", STARFLEET_CLIENT_ID),
    ];

    let response = client
        .post(STARFLEET_TOKEN_URL)
        .header("Content-Type", "application/x-www-form-urlencoded; charset=UTF-8")
        .header("Origin", CEF_ORIGIN)
        .header("Referer", format!("{}/", CEF_ORIGIN))
        .header("Accept", "application/json, text/plain, */*")
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Token refresh failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Token refresh failed with status {}: {}", status, body));
    }

    let token_response: StarfleetTokenResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse token response: {}", e))?;

    let expires_at = Utc::now() + chrono::Duration::seconds(token_response.expires_in);

    Ok(Tokens {
        access_token: token_response.access_token,
        refresh_token: token_response.refresh_token,
        id_token: token_response.id_token,
        expires_at,
    })
}
