use reqwest::Proxy;
use reqwest::blocking::{Client, ClientBuilder};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::time::Duration;
use url::Url;

const INVALID_PROXY: &str = "Invalid session proxy URL. Use http://host:port, https://host:port, socks4://host:port, or socks5://host:port.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProxyConfig {
    pub normalized_url: String,
    pub cache_scope: String,
    pub has_credentials: bool,
}

pub fn config_from_settings(settings: &Value) -> Result<Option<ProxyConfig>, String> {
    if settings["sessionProxyEnabled"].as_bool() != Some(true) {
        return Ok(None);
    }
    let raw = settings["sessionProxyUrl"].as_str().unwrap_or("");
    normalize_proxy_url(raw).map(Some)
}

pub fn client_for_settings(base: &Client, settings: &Value) -> Result<Client, String> {
    client_for_settings_with(base, settings, |builder| builder)
}

pub fn client_for_settings_with(
    base: &Client,
    settings: &Value,
    configure: impl FnOnce(ClientBuilder) -> ClientBuilder,
) -> Result<Client, String> {
    let Some(config) = config_from_settings(settings)? else {
        return Ok(base.clone());
    };
    let proxy = Proxy::all(&config.normalized_url).map_err(|_| INVALID_PROXY.to_owned())?;
    configure(
        Client::builder()
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(20))
            .pool_idle_timeout(Duration::from_secs(60))
            .proxy(proxy),
    )
    .build()
    .map_err(|_| "Could not initialize the configured session proxy".to_owned())
}

pub fn normalize_proxy_url(raw: &str) -> Result<ProxyConfig, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(INVALID_PROXY.to_owned());
    }
    let candidate = if trimmed.contains("://") {
        trimmed.to_owned()
    } else {
        format!("http://{trimmed}")
    };
    let parsed = Url::parse(&candidate).map_err(|_| INVALID_PROXY.to_owned())?;
    if !matches!(parsed.scheme(), "http" | "https" | "socks4" | "socks5")
        || parsed.host_str().is_none()
        || parsed.port().is_none()
        || (parsed.password().is_some() && parsed.username().is_empty())
        || (parsed.path() != "" && parsed.path() != "/")
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(INVALID_PROXY.to_owned());
    }
    let endpoint = format!(
        "{}://{}:{}",
        parsed.scheme(),
        parsed.host_str().unwrap_or_default().to_ascii_lowercase(),
        parsed.port().unwrap_or_default()
    );
    let digest = Sha256::digest(endpoint.as_bytes());
    let digest = format!("{digest:x}");
    Ok(ProxyConfig {
        normalized_url: parsed.to_string(),
        cache_scope: format!("proxy:{}", &digest[..16]),
        has_credentials: !parsed.username().is_empty(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn proxy_configuration_is_strict_and_credential_safe() {
        let direct = json!({"sessionProxyEnabled":false,"sessionProxyUrl":"ignored:80"});
        assert_eq!(config_from_settings(&direct).unwrap(), None);

        let first = config_from_settings(&json!({
            "sessionProxyEnabled":true,
            "sessionProxyUrl":"user:secret@Proxy.Example:8080"
        }))
        .unwrap()
        .unwrap();
        let second = config_from_settings(&json!({
            "sessionProxyEnabled":true,
            "sessionProxyUrl":"http://other:password@proxy.example:8080"
        }))
        .unwrap()
        .unwrap();
        assert!(first.has_credentials);
        assert_eq!(first.cache_scope, second.cache_scope);
        assert!(!first.cache_scope.contains("secret"));
        assert!(
            config_from_settings(&json!({
                "sessionProxyEnabled":true,
                "sessionProxyUrl":"ftp://proxy.example:21"
            }))
            .is_err()
        );
        assert!(
            config_from_settings(&json!({
                "sessionProxyEnabled":true,
                "sessionProxyUrl":"proxy.example"
            }))
            .is_err()
        );
    }
}
