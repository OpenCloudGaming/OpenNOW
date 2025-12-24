use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::command;
use tokio::sync::Mutex;

/// Proxy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub enabled: bool,
    pub proxy_type: ProxyType,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub bypass_local: bool,
    pub bypass_list: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProxyType {
    Http,
    Https,
    Socks5,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            proxy_type: ProxyType::Http,
            host: String::new(),
            port: 8080,
            username: None,
            password: None,
            bypass_local: true,
            bypass_list: vec![
                "localhost".to_string(),
                "127.0.0.1".to_string(),
                "*.local".to_string(),
            ],
        }
    }
}

/// Global proxy configuration
static PROXY_CONFIG: std::sync::OnceLock<Arc<Mutex<ProxyConfig>>> = std::sync::OnceLock::new();

fn get_proxy_config() -> Arc<Mutex<ProxyConfig>> {
    PROXY_CONFIG
        .get_or_init(|| Arc::new(Mutex::new(ProxyConfig::default())))
        .clone()
}

/// Get current proxy configuration
#[command]
pub async fn get_proxy_settings() -> Result<ProxyConfig, String> {
    let storage = get_proxy_config();
    let config = storage.lock().await;
    Ok(config.clone())
}

/// Update proxy configuration
#[command]
pub async fn set_proxy_settings(config: ProxyConfig) -> Result<(), String> {
    // Validate configuration
    if config.enabled {
        if config.host.is_empty() {
            return Err("Proxy host is required".to_string());
        }
        if config.port == 0 {
            return Err("Proxy port is required".to_string());
        }
    }

    let storage = get_proxy_config();
    let mut guard = storage.lock().await;
    *guard = config;

    log::info!("Proxy settings updated");
    Ok(())
}

/// Enable proxy
#[command]
pub async fn enable_proxy() -> Result<(), String> {
    let storage = get_proxy_config();
    let mut config = storage.lock().await;

    if config.host.is_empty() {
        return Err("Proxy host not configured".to_string());
    }

    config.enabled = true;
    log::info!("Proxy enabled: {}:{}", config.host, config.port);
    Ok(())
}

/// Disable proxy
#[command]
pub async fn disable_proxy() -> Result<(), String> {
    let storage = get_proxy_config();
    let mut config = storage.lock().await;
    config.enabled = false;
    log::info!("Proxy disabled");
    Ok(())
}

/// Test proxy connection
#[command]
pub async fn test_proxy() -> Result<String, String> {
    let storage = get_proxy_config();
    let config = storage.lock().await;

    if !config.enabled {
        return Err("Proxy is not enabled".to_string());
    }

    let proxy_url = build_proxy_url(&config);

    // Create a client with the proxy
    let proxy = reqwest::Proxy::all(&proxy_url)
        .map_err(|e| format!("Invalid proxy configuration: {}", e))?;

    let client = reqwest::Client::builder()
        .proxy(proxy)
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to create client: {}", e))?;

    // Test connection to a known endpoint
    let start = std::time::Instant::now();
    let response = client
        .get("https://games.geforce.com/graphql")
        .header("Content-Type", "application/json")
        .body(r#"{"query":"{ __typename }"}"#)
        .send()
        .await
        .map_err(|e| format!("Proxy connection failed: {}", e))?;

    let latency = start.elapsed().as_millis();

    if response.status().is_success() || response.status().as_u16() == 400 {
        // 400 is expected for invalid query, but connection worked
        Ok(format!("Proxy working! Latency: {}ms", latency))
    } else {
        Err(format!(
            "Proxy returned status: {}",
            response.status()
        ))
    }
}

/// Build proxy URL from configuration
pub fn build_proxy_url(config: &ProxyConfig) -> String {
    let scheme = match config.proxy_type {
        ProxyType::Http => "http",
        ProxyType::Https => "https",
        ProxyType::Socks5 => "socks5",
    };

    match (&config.username, &config.password) {
        (Some(user), Some(pass)) => {
            format!("{}://{}:{}@{}:{}", scheme, user, pass, config.host, config.port)
        }
        _ => format!("{}://{}:{}", scheme, config.host, config.port),
    }
}

/// Create a reqwest client with proxy configuration
pub async fn create_proxied_client() -> Result<reqwest::Client, String> {
    let storage = get_proxy_config();
    let config = storage.lock().await;

    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30));

    if config.enabled && !config.host.is_empty() {
        let proxy_url = build_proxy_url(&config);
        let proxy = reqwest::Proxy::all(&proxy_url)
            .map_err(|e| format!("Invalid proxy configuration: {}", e))?;
        builder = builder.proxy(proxy);
    }

    builder
        .build()
        .map_err(|e| format!("Failed to create client: {}", e))
}

/// Check if a host should bypass the proxy
pub fn should_bypass_proxy(config: &ProxyConfig, host: &str) -> bool {
    if !config.enabled {
        return true;
    }

    if config.bypass_local {
        if host == "localhost" || host == "127.0.0.1" || host.ends_with(".local") {
            return true;
        }
    }

    for pattern in &config.bypass_list {
        if pattern.starts_with("*.") {
            // Wildcard pattern
            let suffix = &pattern[1..];
            if host.ends_with(suffix) {
                return true;
            }
        } else if host == pattern {
            return true;
        }
    }

    false
}
