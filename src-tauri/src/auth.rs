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
const STARFLEET_TOKEN_URL: &str = "https://login.nvidia.com/oauth/token";
const LOGOUT_URL: &str = "https://static-login.nvidia.com/service/gfn/logout-start";

/// Starfleet Client ID from GFN client - this is for the public NVIDIA login
const STARFLEET_CLIENT_ID: &str = "ZU7sPN-miLujMD95LfOQ453IB0AtjM8sMyvgJ9wCXEQ";

/// OAuth scopes required by GFN
const OAUTH_SCOPES: &str = "openid consent email tk_client age";

/// Available redirect ports (from GFN config)
const REDIRECT_PORTS: [u16; 5] = [2259, 6460, 7119, 8870, 9096];

/// Token refresh duration: 27 days in milliseconds
const TOKEN_REFRESH_DURATION_MS: i64 = 2332800000;
/// Refresh threshold: 30% of remaining lifetime
const REFRESH_THRESHOLD_PERCENT: f64 = 0.30;

/// GFN Client User-Agent
const GFN_USER_AGENT: &str = "NVIDIA GeForce NOW/2.0.64 (Windows; Win64; x64)";

/// Device ID for GFN client identification
const DEVICE_ID: &str = "gfnclient";

/// Origin header for OAuth requests
const GFN_ORIGIN: &str = "https://play.geforcenow.com";

/// Find an available port from the allowed redirect ports
fn find_available_port() -> Option<u16> {
    for port in REDIRECT_PORTS {
        if std::net::TcpListener::bind(format!("127.0.0.1:{}", port)).is_ok() {
            return Some(port);
        }
    }
    None
}

/// Generate a random state parameter for OAuth security
fn generate_state() -> String {
    generate_random_string(32)
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

/// Generate a nonce for OpenID Connect
fn generate_nonce() -> String {
    generate_random_string(32)
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

/// Extract authorization code from HTTP request
fn extract_auth_code(request: &str) -> Option<(String, String)> {
    // Parse the GET request line
    let first_line = request.lines().next()?;
    let path = first_line.split_whitespace().nth(1)?;

    // Extract query string
    let query_start = path.find('?')?;
    let query = &path[query_start + 1..];
    let params = parse_query_string(query);

    let code = params.get("code")?.clone();
    let state = params.get("state")?.clone();

    Some((code, state))
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
        .success-icon {
            font-size: 64px;
            color: #76b900;
            margin-bottom: 20px;
        }
        h1 { color: #76b900; margin-bottom: 10px; }
        p { color: #aaa; }
    </style>
</head>
<body>
    <div class="container">
        <div class="success-icon">✓</div>
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

    // Try to get user info with this token to validate it
    let user = get_user_info(&token).await?;

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
#[command]
pub async fn get_access_token() -> Result<String, String> {
    let storage = get_auth_storage();
    let guard = storage.lock().await;

    guard.as_ref()
        .and_then(|state| state.tokens.as_ref())
        .map(|tokens| tokens.access_token.clone())
        .ok_or_else(|| "Not authenticated - please login first".to_string())
}

/// Legacy OAuth login flow with localhost callback (may get 403 from NVIDIA)
#[command]
pub async fn login_oauth() -> Result<AuthState, String> {
    log::info!("Starting OAuth login flow (Authorization Code Flow with PKCE)");

    // Find an available redirect port
    let port = find_available_port()
        .ok_or_else(|| "No available ports for OAuth callback".to_string())?;

    let redirect_uri = format!("http://localhost:{}", port);
    let expected_state = generate_state();
    let nonce = generate_nonce();

    // Generate PKCE code verifier and challenge
    let code_verifier = generate_code_verifier();
    let code_challenge = generate_code_challenge(&code_verifier);

    // Build OAuth authorization URL with PKCE
    let auth_url = format!(
        "https://login.nvidia.com/oauth/authorize?\
        client_id={}&\
        redirect_uri={}&\
        response_type=code&\
        scope={}&\
        state={}&\
        nonce={}&\
        code_challenge={}&\
        code_challenge_method=S256&\
        device_id={}",
        STARFLEET_CLIENT_ID,
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(OAUTH_SCOPES),
        expected_state,
        nonce,
        code_challenge,
        DEVICE_ID
    );

    log::info!("Starting OAuth callback server on port {}", port);

    // Start TCP listener for OAuth callback
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port))
        .await
        .map_err(|e| format!("Failed to start callback server: {}", e))?;

    // Open browser with auth URL
    log::info!("Opening browser for authentication");
    if let Err(e) = open::that(&auth_url) {
        log::warn!("Failed to open browser automatically: {}. Please open: {}", e, auth_url);
    }

    // Wait for callback (with 5 minute timeout)
    let callback_result = tokio::time::timeout(
        std::time::Duration::from_secs(300),
        async {
            loop {
                let (mut socket, _) = listener.accept().await
                    .map_err(|e| format!("Failed to accept connection: {}", e))?;

                let mut buffer = vec![0u8; 4096];
                let n = socket.read(&mut buffer).await
                    .map_err(|e| format!("Failed to read request: {}", e))?;

                let request = String::from_utf8_lossy(&buffer[..n]);

                // Check if this is a valid OAuth callback
                if let Some((code, state)) = extract_auth_code(&request) {
                    // Verify state parameter
                    if state != expected_state {
                        log::warn!("State mismatch: expected {}, got {}", expected_state, state);
                        let response = format!(
                            "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}",
                            ERROR_HTML.len(),
                            ERROR_HTML
                        );
                        let _ = socket.write_all(response.as_bytes()).await;
                        continue;
                    }

                    // Send success response
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}",
                        SUCCESS_HTML.len(),
                        SUCCESS_HTML
                    );
                    let _ = socket.write_all(response.as_bytes()).await;

                    return Ok::<String, String>(code);
                }

                // Handle favicon and other requests
                let response = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
                let _ = socket.write_all(response.as_bytes()).await;
            }
        }
    ).await;

    let code = match callback_result {
        Ok(Ok(code)) => code,
        Ok(Err(e)) => return Err(e),
        Err(_) => return Err("Login timeout - please try again".to_string()),
    };

    log::info!("Received authorization code, exchanging for tokens");

    // Exchange code for tokens (include code_verifier for PKCE)
    let tokens = exchange_code(&code, &redirect_uri, &code_verifier).await?;

    log::info!("Tokens received, fetching user info");

    // Get user info
    let user = get_user_info(&tokens.access_token).await?;

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

    log::info!("Login successful");
    Ok(auth_state)
}

/// Exchange authorization code for tokens (with PKCE code_verifier)
async fn exchange_code(code: &str, redirect_uri: &str, code_verifier: &str) -> Result<Tokens, String> {
    let client = Client::builder()
        .user_agent(GFN_USER_AGENT)
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("client_id", STARFLEET_CLIENT_ID),
        ("redirect_uri", redirect_uri),
        ("code_verifier", code_verifier),
    ];

    let response = client
        .post(STARFLEET_TOKEN_URL)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Origin", GFN_ORIGIN)
        .header("Referer", "https://play.geforcenow.com/")
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Token exchange failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Token exchange failed with status {}: {}", status, body));
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

/// JWT payload structure (decoded from access token)
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

/// Get user info by decoding the JWT token
/// The JWT contains user info in its payload, no API call needed
async fn get_user_info(access_token: &str) -> Result<User, String> {
    // JWT format: header.payload.signature
    let parts: Vec<&str> = access_token.split('.').collect();

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

    // Use standard base64 decoding (JWT uses URL-safe base64)
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
        .user_agent(GFN_USER_AGENT)
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", &refresh_token),
        ("client_id", STARFLEET_CLIENT_ID),
    ];

    let response = client
        .post(STARFLEET_TOKEN_URL)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Origin", GFN_ORIGIN)
        .header("Referer", "https://play.geforcenow.com/")
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
