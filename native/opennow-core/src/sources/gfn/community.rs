use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::thread;
use std::time::Duration;
use url::Url;

const QUEUE_URL: &str = "https://api.printedwaste.com/gfn/queue/";
const SERVER_MAPPING_URL: &str =
    "https://remote.printedwaste.com/config/GFN_SERVERID_TO_REGION_MAPPING";
const PROVISION_URL: &str = "https://opennow-proxy-production.up.railway.app/api/public/proxy";
const COMMUNITY_PROXY_HOST: &str = "altaria.proxy.rlwy.net";
const COMMUNITY_PROXY_PORT: u16 = 51_545;
const MAXIMUM_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;

pub struct CommunityService {
    client: Client,
}

impl CommunityService {
    pub fn new() -> Result<Self, String> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(45))
            .pool_idle_timeout(Duration::from_secs(30))
            .build()
            .map_err(|error| error.to_string())?;
        Ok(Self { client })
    }

    pub fn queue(&self) -> Result<Value, String> {
        let payload = self.get_json(QUEUE_URL, Duration::from_secs(7))?;
        normalize_queue(&payload)
    }

    pub fn server_mapping(&self) -> Result<Value, String> {
        let payload = self.get_json(SERVER_MAPPING_URL, Duration::from_secs(7))?;
        normalize_server_mapping(&payload)
    }

    pub fn provision_proxy(&self, client_id: &str) -> Result<Value, String> {
        if client_id.len() < 16 || client_id.len() > 128 {
            return Err("Stable device identity is invalid".to_owned());
        }
        let mut response = self.provision_request(client_id)?;
        if matches!(
            response.status().as_u16(),
            408 | 425 | 429 | 500 | 502 | 503 | 504
        ) {
            thread::sleep(Duration::from_millis(1_500));
            response = self.provision_request(client_id)?;
        }
        if !response.status().is_success() {
            return Err(format!(
                "Community proxy provision failed ({})",
                response.status().as_u16()
            ));
        }
        let length = response.content_length().unwrap_or(0);
        if length > MAXIMUM_RESPONSE_BYTES {
            return Err("Community proxy response exceeded the size limit".to_owned());
        }
        let payload = response
            .json::<ProvisionResponse>()
            .map_err(|_| "Community proxy provision returned invalid JSON".to_owned())?;
        let proxy_url = if let Some(value) = payload.proxy_url {
            normalize_community_proxy_url(&value)?
        } else {
            build_community_proxy_url(
                payload
                    .username
                    .as_deref()
                    .ok_or_else(|| "Community proxy response omitted credentials".to_owned())?,
                payload
                    .password
                    .as_deref()
                    .ok_or_else(|| "Community proxy response omitted credentials".to_owned())?,
            )?
        };
        Ok(json!({"proxyUrl":proxy_url}))
    }

    fn get_json(&self, url: &str, timeout: Duration) -> Result<Value, String> {
        let response = self
            .client
            .get(url)
            .timeout(timeout)
            .header(
                "User-Agent",
                format!("opennow/{}", crate::version::APPLICATION_VERSION),
            )
            .header("Accept", "application/json")
            .send()
            .map_err(|_| "Community service is unavailable right now".to_owned())?;
        if !response.status().is_success() {
            return Err(format!(
                "Community service returned HTTP {}",
                response.status().as_u16()
            ));
        }
        if response.content_length().unwrap_or(0) > MAXIMUM_RESPONSE_BYTES {
            return Err("Community response exceeded the size limit".to_owned());
        }
        response
            .json::<Value>()
            .map_err(|_| "Community service returned invalid JSON".to_owned())
    }

    fn provision_request(&self, client_id: &str) -> Result<reqwest::blocking::Response, String> {
        self.client
            .post(PROVISION_URL)
            .timeout(Duration::from_secs(45))
            .header(
                "User-Agent",
                format!(
                    "OpenNOW-DesktopClient/{}",
                    crate::version::APPLICATION_VERSION
                ),
            )
            .header("Accept", "application/json")
            .json(&json!({"clientId":client_id}))
            .send()
            .map_err(|_| "Community proxy provisioning is unavailable right now".to_owned())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProvisionResponse {
    proxy_url: Option<String>,
    username: Option<String>,
    password: Option<String>,
}

fn data_object<'a>(payload: &'a Value, context: &str) -> Result<&'a Map<String, Value>, String> {
    if payload["status"].as_bool() != Some(true) {
        return Err(format!("{context} returned status:false"));
    }
    payload["data"]
        .as_object()
        .ok_or_else(|| format!("{context} response omitted its data object"))
}

fn normalize_queue(payload: &Value) -> Result<Value, String> {
    let data = data_object(payload, "PrintedWaste queue")?;
    let mut output = Map::new();
    for (zone_id, raw) in data {
        let Some(zone) = raw.as_object() else {
            continue;
        };
        let (Some(position), Some(updated), Some(region)) = (
            zone.get("QueuePosition").and_then(Value::as_f64),
            zone.get("Last Updated").and_then(Value::as_f64),
            zone.get("Region")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty()),
        ) else {
            continue;
        };
        if !position.is_finite() || !updated.is_finite() {
            continue;
        }
        let mut normalized = json!({
            "QueuePosition":position,
            "Last Updated":updated,
            "Region":region
        });
        if let Some(eta) = zone
            .get("eta")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite())
        {
            normalized["eta"] = json!(eta);
        }
        output.insert(zone_id.clone(), normalized);
    }
    if output.is_empty() {
        return Err("PrintedWaste queue returned no valid zones".to_owned());
    }
    Ok(Value::Object(output))
}

fn normalize_server_mapping(payload: &Value) -> Result<Value, String> {
    let data = data_object(payload, "PrintedWaste server mapping")?;
    let mut output = Map::new();
    for (zone_id, raw) in data {
        let Some(zone) = raw.as_object() else {
            continue;
        };
        let mut normalized = Map::new();
        for key in ["title", "region"] {
            if let Some(value) = zone.get(key).and_then(Value::as_str) {
                normalized.insert(key.to_owned(), json!(value));
            }
        }
        for key in ["is4080Server", "is5080Server", "nuked"] {
            if let Some(value) = zone.get(key).and_then(Value::as_bool) {
                normalized.insert(key.to_owned(), json!(value));
            }
        }
        output.insert(zone_id.clone(), Value::Object(normalized));
    }
    Ok(Value::Object(output))
}

fn build_community_proxy_url(username: &str, password: &str) -> Result<String, String> {
    if username.is_empty() {
        return Err("Community proxy response omitted credentials".to_owned());
    }
    let mut url = Url::parse(&format!(
        "http://{COMMUNITY_PROXY_HOST}:{COMMUNITY_PROXY_PORT}"
    ))
    .map_err(|_| "Community proxy endpoint is invalid".to_owned())?;
    url.set_username(username)
        .map_err(|_| "Community proxy username is invalid".to_owned())?;
    url.set_password(Some(password))
        .map_err(|_| "Community proxy password is invalid".to_owned())?;
    Ok(url.to_string())
}

fn normalize_community_proxy_url(raw: &str) -> Result<String, String> {
    let url = Url::parse(raw).map_err(|_| "Community proxy returned an invalid URL".to_owned())?;
    if url.scheme() != "http"
        || url.host_str() != Some(COMMUNITY_PROXY_HOST)
        || url.port() != Some(COMMUNITY_PROXY_PORT)
        || url.username().is_empty()
        || url.password().is_none()
        || (url.path() != "" && url.path() != "/")
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("Community proxy returned an invalid URL".to_owned());
    }
    Ok(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printed_waste_payloads_are_strictly_normalized() {
        let queue = normalize_queue(&json!({"status":true,"data":{
            "zone-a":{"QueuePosition":42,"Last Updated":123,"Region":"EU","eta":9},
            "bad":{"QueuePosition":"many"}
        }}))
        .unwrap();
        assert_eq!(queue.as_object().unwrap().len(), 1);
        assert_eq!(queue["zone-a"]["Region"], "EU");
        let mapping = normalize_server_mapping(&json!({"status":true,"data":{
            "zone-a":{"title":"Paris","region":"EU","nuked":false,"ignored":"x"}
        }}))
        .unwrap();
        assert_eq!(mapping["zone-a"]["title"], "Paris");
        assert!(mapping["zone-a"].get("ignored").is_none());
    }

    #[test]
    fn community_proxy_credentials_are_encoded_and_endpoint_scoped() {
        let value = build_community_proxy_url("player name", "p@ss/word").unwrap();
        let parsed = Url::parse(&value).unwrap();
        assert_eq!(parsed.host_str(), Some(COMMUNITY_PROXY_HOST));
        assert_eq!(parsed.port(), Some(COMMUNITY_PROXY_PORT));
        assert_eq!(parsed.username(), "player%20name");
        assert!(normalize_community_proxy_url(&value).is_ok());
        assert!(normalize_community_proxy_url("http://evil.example:51545/u").is_err());
    }
}
