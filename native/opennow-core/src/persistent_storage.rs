use crate::gfn::{AuthSession, ServiceError};
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde_json::{Value, json};
use url::Url;

const PAYWALL_BASE: &str = "https://api-prod.nvidia.com/gfn-paywall-api/api/v2";

pub struct PersistentStorageService {
    client: Client,
}

impl PersistentStorageService {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    pub fn locations(&self, params: &Value, auth: &AuthSession) -> Result<Value, ServiceError> {
        let current_code = params["currentRegionCode"]
            .as_str()
            .filter(|value| !value.trim().is_empty());
        let current_name = params["currentRegionName"]
            .as_str()
            .filter(|value| !value.trim().is_empty());
        let mut url =
            Url::parse(&format!("{PAYWALL_BASE}/products")).expect("constant paywall URL is valid");
        url.query_pairs_mut()
            .append_pair("locale", params["locale"].as_str().unwrap_or("en_US"));
        if let Some(vpc_id) = params["serverRegionId"]
            .as_str()
            .filter(|value| !value.trim().is_empty())
        {
            url.query_pairs_mut().append_pair("vpcId", vpc_id);
        }
        let token = session_token(auth)?;
        let response = self.client.get(url).headers(paywall_headers(token)?).send();
        let mut locations = response
            .ok()
            .and_then(|response| {
                if !response.status().is_success() {
                    return None;
                }
                response
                    .json::<Value>()
                    .ok()
                    .map(|payload| locations_from_products(&payload, current_code))
            })
            .unwrap_or_default();
        if locations.is_empty() {
            locations = fallback_locations(current_code, current_name);
        }
        if let Some(code) = current_code
            && !locations
                .iter()
                .any(|location| location["code"].as_str() == Some(code))
        {
            locations.insert(0, json!({"code":code,"name":current_name.unwrap_or(code),"isAvailable":true,"isCurrent":true}));
        }
        Ok(
            json!({"locations":locations,"currentRegionCode":current_code,"currentRegionName":current_name}),
        )
    }

    pub fn reset(&self, params: &Value, auth: &AuthSession) -> Result<Value, ServiceError> {
        if params["confirmed"].as_bool() != Some(true) {
            return Err(ServiceError {
                code: "confirmation_required",
                message: "Persistent storage reset requires explicit confirmation".to_owned(),
            });
        }
        let region = params["storageRegion"]
            .as_str()
            .filter(|value| !value.trim().is_empty());
        let mut url = Url::parse(&format!("{PAYWALL_BASE}/reset/storage"))
            .expect("constant paywall URL is valid");
        url.query_pairs_mut()
            .append_pair("storageRegion", region.unwrap_or("null"));
        let response = self
            .client
            .post(url)
            .headers(paywall_headers(session_token(auth)?)?)
            .send()
            .map_err(|error| ServiceError {
                code: "network_error",
                message: format!("Persistent storage reset failed: {error}"),
            })?;
        let status = response.status();
        let payload = response.json::<Value>().unwrap_or(Value::Null);
        if !status.is_success() {
            let message = payload["message"]
                .as_str()
                .or_else(|| payload["errors"]["errorMessage"].as_str())
                .unwrap_or("Persistent storage reset failed");
            let message = if message.contains("idToken") {
                "NVIDIA requires a web-account session for this storage action. Open NVIDIA Storage Manager in your browser."
            } else {
                message
            };
            return Err(ServiceError {
                code: if matches!(status.as_u16(), 401 | 403) {
                    "authentication_required"
                } else {
                    "upstream_error"
                },
                message: message.to_owned(),
            });
        }
        Ok(json!({"ok":true,"storageRegion":region,"message":payload["message"]}))
    }
}

fn locations_from_products(payload: &Value, current_code: Option<&str>) -> Vec<Value> {
    let mut products = payload["products"].as_array().cloned().unwrap_or_default();
    let addons = products
        .iter()
        .flat_map(|product| product["add_on"].as_array().cloned().unwrap_or_default())
        .collect::<Vec<_>>();
    products.extend(addons);
    let regions = products
        .iter()
        .find_map(|product| {
            let kind = product["productType"].as_str().unwrap_or_default();
            (kind.eq_ignore_ascii_case("STORAGE")
                || kind.eq_ignore_ascii_case("PAID")
                || product["regions"].is_array())
            .then(|| product["regions"].as_array().cloned())
            .flatten()
            .filter(|values| !values.is_empty())
        })
        .unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    regions.into_iter().filter_map(|region| {
        let code = region["metroRegion"].as_str()?.trim();
        if code.is_empty() || !seen.insert(code.to_owned()) { return None; }
        Some(json!({
            "code":code,"name":region["metroRegionName"].as_str().unwrap_or(code),
            "isAvailable":region["isAvailable"].as_bool().unwrap_or(true),
            "isCurrent":current_code == Some(code),"isRecommended":region["isRecommendedRegion"].as_bool().unwrap_or(false)
        }))
    }).collect()
}

fn fallback_locations(current_code: Option<&str>, current_name: Option<&str>) -> Vec<Value> {
    const LOCATIONS: [(&str, &str); 20] = [
        ("NP-SJC6-04", "Northern California (USA)"),
        ("NP-LAX-03", "Southern California (USA)"),
        ("NP-PDX-01", "Oregon (USA)"),
        ("NP-PHX-02", "Arizona (USA)"),
        ("NP-DAL-04", "Texas (USA)"),
        ("NP-CHI-04", "Illinois (USA)"),
        ("NP-MIA-03", "Florida (USA)"),
        ("NP-ATL-03", "Georgia (USA)"),
        ("NP-ASH-04", "Virginia (USA)"),
        ("NP-NWK-03", "New Jersey (USA)"),
        ("NP-MON-02", "Quebec (Canada)"),
        ("NP-LON-07", "United Kingdom"),
        ("NP-STH-03", "Sweden"),
        ("NP-AMS-07", "Netherlands North"),
        ("NP-FRK-06", "Germany"),
        ("NP-PAR-05", "France"),
        ("NP-WAW-01", "Poland"),
        ("NP-SOF-02", "Bulgaria"),
        ("NP-BOM-01", "India"),
        ("NP-TYO-01", "Japan"),
    ];
    LOCATIONS.into_iter().map(|(code,name)| json!({
        "code":code,"name":if current_code == Some(code) { current_name.unwrap_or(name) } else { name },
        "isAvailable":true,"isCurrent":current_code == Some(code),"isRecommended":false
    })).collect()
}

fn paywall_headers(token: &str) -> Result<HeaderMap, ServiceError> {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        "idtoken",
        HeaderValue::from_str(token).map_err(|_| ServiceError {
            code: "invalid_params",
            message: "Invalid authentication token".to_owned(),
        })?,
    );
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn product_regions_are_deduplicated() {
        let payload = json!({"products":[{"productType":"STORAGE","regions":[
            {"metroRegion":"NP-AMS-07","metroRegionName":"Amsterdam","isAvailable":true},
            {"metroRegion":"NP-AMS-07","metroRegionName":"Duplicate"}
        ]}]});
        let result = locations_from_products(&payload, Some("NP-AMS-07"));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["isCurrent"], true);
    }
}
