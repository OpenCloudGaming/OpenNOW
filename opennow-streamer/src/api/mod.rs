//! GFN API Client
//!
//! HTTP API interactions with GeForce NOW services.

mod cloudmatch;
mod games;
pub mod error_codes;

#[allow(unused_imports)]
pub use cloudmatch::*;
pub use games::*;
pub use error_codes::SessionError;

use reqwest::Client;
use parking_lot::RwLock;
use log::{info, debug, warn};
use serde::Deserialize;

/// Cached VPC ID from serverInfo
static CACHED_VPC_ID: RwLock<Option<String>> = RwLock::new(None);

/// Server info response from /v2/serverInfo endpoint
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServerInfoResponse {
    request_status: Option<ServerInfoRequestStatus>,
    #[serde(default)]
    meta_data: Vec<ServerMetaData>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServerInfoRequestStatus {
    server_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ServerMetaData {
    key: String,
    value: String,
}

/// Dynamic server region from serverInfo API
#[derive(Debug, Clone)]
pub struct DynamicServerRegion {
    pub name: String,
    pub url: String,
}

/// Get the cached VPC ID or fetch it from serverInfo
pub async fn get_vpc_id(client: &Client, token: Option<&str>) -> String {
    // Check cache first
    {
        let cached = CACHED_VPC_ID.read();
        if let Some(vpc_id) = cached.as_ref() {
            return vpc_id.clone();
        }
    }

    // Fetch from serverInfo endpoint
    if let Some(vpc_id) = fetch_vpc_id_from_server_info(client, token).await {
        // Cache it
        *CACHED_VPC_ID.write() = Some(vpc_id.clone());
        return vpc_id;
    }

    // Fallback to a common European VPC
    "NP-AMS-08".to_string()
}

/// Fetch VPC ID from the /v2/serverInfo endpoint
async fn fetch_vpc_id_from_server_info(client: &Client, token: Option<&str>) -> Option<String> {
    let url = "https://prod.cloudmatchbeta.nvidiagrid.net/v2/serverInfo";

    info!("Fetching VPC ID from serverInfo: {}", url);

    let mut request = client
        .get(url)
        .header("Accept", "application/json")
        .header("nv-client-id", "ec7e38d4-03af-4b58-b131-cfb0495903ab")
        .header("nv-client-type", "NATIVE")
        .header("nv-client-version", "2.0.80.173")
        .header("nv-client-streamer", "NVIDIA-CLASSIC")
        .header("nv-device-os", "WINDOWS")
        .header("nv-device-type", "DESKTOP");

    if let Some(t) = token {
        request = request.header("Authorization", format!("GFNJWT {}", t));
    }

    let response = match request.send().await {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to fetch serverInfo: {}", e);
            return None;
        }
    };

    if !response.status().is_success() {
        warn!("serverInfo returned status: {}", response.status());
        return None;
    }

    let body = match response.text().await {
        Ok(b) => b,
        Err(e) => {
            warn!("Failed to read serverInfo body: {}", e);
            return None;
        }
    };

    debug!("serverInfo response: {}", &body[..body.len().min(500)]);

    let info: ServerInfoResponse = match serde_json::from_str(&body) {
        Ok(i) => i,
        Err(e) => {
            warn!("Failed to parse serverInfo: {}", e);
            return None;
        }
    };

    let vpc_id = info.request_status
        .and_then(|s| s.server_id);

    info!("Discovered VPC ID: {:?}", vpc_id);
    vpc_id
}

/// Clear the cached VPC ID (call on logout)
pub fn clear_vpc_cache() {
    *CACHED_VPC_ID.write() = None;
}

/// Fetch dynamic server regions from the /v2/serverInfo endpoint
/// Uses the selected provider's streaming URL (supports Alliance partners)
/// Returns regions discovered from metaData with their streaming URLs
pub async fn fetch_dynamic_regions(client: &Client, token: Option<&str>) -> Vec<DynamicServerRegion> {
    use crate::auth;

    // Get the base URL from the selected provider (Alliance partners have different URLs)
    let base_url = auth::get_streaming_base_url();
    let url = format!("{}v2/serverInfo", base_url);

    info!("[serverInfo] Fetching dynamic regions from: {}", url);

    let mut request = client
        .get(&url)
        .header("Accept", "application/json")
        .header("nv-client-id", "ec7e38d4-03af-4b58-b131-cfb0495903ab")
        .header("nv-client-type", "BROWSER")
        .header("nv-client-version", "2.0.80.173")
        .header("nv-client-streamer", "WEBRTC")
        .header("nv-device-os", "WINDOWS")
        .header("nv-device-type", "DESKTOP");

    if let Some(t) = token {
        request = request.header("Authorization", format!("GFNJWT {}", t));
    }

    let response = match request.send().await {
        Ok(r) => r,
        Err(e) => {
            warn!("[serverInfo] Failed to fetch: {}", e);
            return Vec::new();
        }
    };

    if !response.status().is_success() {
        warn!("[serverInfo] Returned status: {}", response.status());
        return Vec::new();
    }

    let body = match response.text().await {
        Ok(b) => b,
        Err(e) => {
            warn!("[serverInfo] Failed to read body: {}", e);
            return Vec::new();
        }
    };

    let info: ServerInfoResponse = match serde_json::from_str(&body) {
        Ok(i) => i,
        Err(e) => {
            warn!("[serverInfo] Failed to parse: {}", e);
            return Vec::new();
        }
    };

    // Extract regions from metaData
    // Format: key="REGION NAME", value="https://region-url.domain.net"
    // For NVIDIA: URLs contain "nvidiagrid.net"
    // For Alliance partners: URLs may have different domains
    let mut regions: Vec<DynamicServerRegion> = Vec::new();

    for meta in &info.meta_data {
        // Skip special keys like "gfn-regions"
        if meta.key == "gfn-regions" || meta.key.starts_with("gfn-") {
            continue;
        }

        // Include entries where value is a streaming URL (https://)
        // Don't filter by domain - Alliance partners have different domains
        if meta.value.starts_with("https://") {
            regions.push(DynamicServerRegion {
                name: meta.key.clone(),
                url: meta.value.clone(),
            });
        }
    }

    info!("[serverInfo] Found {} zones from API", regions.len());

    // Also cache the VPC ID if available
    if let Some(vpc_id) = info.request_status.and_then(|s| s.server_id) {
        info!("[serverInfo] Discovered VPC ID: {}", vpc_id);
        *CACHED_VPC_ID.write() = Some(vpc_id);
    }

    regions
}

/// HTTP client wrapper for GFN APIs
pub struct GfnApiClient {
    client: Client,
    access_token: Option<String>,
}

impl GfnApiClient {
    /// Create a new API client
    pub fn new() -> Self {
        let client = Client::builder()
            .danger_accept_invalid_certs(true) // GFN servers may have self-signed certs
            .gzip(true)
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            access_token: None,
        }
    }

    /// Set the access token for authenticated requests
    pub fn set_access_token(&mut self, token: String) {
        self.access_token = Some(token);
    }

    /// Get the HTTP client
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Get the access token
    pub fn token(&self) -> Option<&String> {
        self.access_token.as_ref()
    }
}

impl Default for GfnApiClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Common headers for GFN API requests
pub fn gfn_headers() -> Vec<(&'static str, &'static str)> {
    vec![
        ("nv-browser-type", "CHROME"),
        ("nv-client-streamer", "NVIDIA-CLASSIC"),
        ("nv-client-type", "NATIVE"),
        ("nv-client-version", "2.0.80.173"),
        ("nv-device-os", "WINDOWS"),
        ("nv-device-type", "DESKTOP"),
    ]
}

/// MES (Membership/Subscription) API URL
const MES_URL: &str = "https://mes.geforcenow.com/v4/subscriptions";

/// LCARS Client ID
const LCARS_CLIENT_ID: &str = "ec7e38d4-03af-4b58-b131-cfb0495903ab";

/// GFN client version
const GFN_CLIENT_VERSION: &str = "2.0.80.173";

/// Subscription response from MES API
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubscriptionResponse {
    #[serde(default = "default_tier")]
    membership_tier: String,
    remaining_time_in_minutes: Option<i32>,
    total_time_in_minutes: Option<i32>,
    #[serde(default)]
    sub_type: Option<String>,  // TIME_CAPPED or UNLIMITED
    #[serde(default)]
    addons: Vec<SubscriptionAddon>,
    features: Option<SubscriptionFeatures>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubscriptionFeatures {
    #[serde(default)]
    resolutions: Vec<SubscriptionResolution>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubscriptionResolution {
    height_in_pixels: u32,
    width_in_pixels: u32,
    frames_per_second: u32,
    is_entitled: bool,
}

fn default_tier() -> String {
    "FREE".to_string()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubscriptionAddon {
    #[serde(rename = "type")]
    addon_type: Option<String>,
    sub_type: Option<String>,
    #[serde(default)]
    attributes: Vec<AddonAttribute>,
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AddonAttribute {
    key: Option<String>,
    #[serde(rename = "textValue")]
    text_value: Option<String>,
}

/// Fetch subscription info from MES API
pub async fn fetch_subscription(token: &str, user_id: &str) -> Result<crate::app::SubscriptionInfo, String> {
    use crate::auth;
    
    let client = Client::builder()
        .gzip(true)
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    // For Alliance partners, we need to fetch VPC ID from their serverInfo first
    // because the cached VPC ID might be stale or from NVIDIA's serverInfo
    let provider = auth::get_selected_provider();
    let vpc_id = if provider.is_alliance_partner() {
        // Fetch VPC ID from Alliance partner's serverInfo
        info!("Fetching VPC ID for Alliance partner: {}", provider.login_provider_display_name);
        let regions = fetch_dynamic_regions(&client, Some(token)).await;
        
        // The VPC ID gets cached by fetch_dynamic_regions, so try to read it
        let cached = CACHED_VPC_ID.read();
        if let Some(vpc) = cached.as_ref() {
            info!("Using Alliance VPC ID: {}", vpc);
            vpc.clone()
        } else {
            // Fallback: try to extract from first region URL
            if let Some(first_region) = regions.first() {
                // Extract VPC-like ID from region name if possible
                info!("Using Alliance region as VPC hint: {}", first_region.name);
                first_region.name.clone()
            } else {
                return Err("Could not determine Alliance VPC ID".to_string());
            }
        }
    } else {
        // For NVIDIA, use cached VPC ID or fallback
        let cached = CACHED_VPC_ID.read();
        cached.as_ref().cloned().unwrap_or_else(|| "NP-AMS-08".to_string())
    };

    let url = format!(
        "{}?serviceName=gfn_pc&languageCode=en_US&vpcId={}&userId={}",
        MES_URL, vpc_id, user_id
    );

    info!("Fetching subscription from: {}", url);

    let response = client
        .get(&url)
        .header("Authorization", format!("GFNJWT {}", token))
        .header("Accept", "application/json")
        .header("nv-client-id", LCARS_CLIENT_ID)
        .header("nv-client-type", "NATIVE")
        .header("nv-client-version", GFN_CLIENT_VERSION)
        .header("nv-client-streamer", "NVIDIA-CLASSIC")
        .header("nv-device-os", "WINDOWS")
        .header("nv-device-type", "DESKTOP")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch subscription: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Subscription API failed with status {}: {}", status, body));
    }

    let body = response.text().await
        .map_err(|e| format!("Failed to read subscription response: {}", e))?;

    debug!("Subscription response: {}", &body[..body.len().min(500)]);

    let sub: SubscriptionResponse = serde_json::from_str(&body)
        .map_err(|e| format!("Failed to parse subscription: {}", e))?;

    // Convert minutes to hours
    let remaining_hours = sub.remaining_time_in_minutes
        .map(|m| m as f32 / 60.0)
        .unwrap_or(0.0);
    let total_hours = sub.total_time_in_minutes
        .map(|m| m as f32 / 60.0)
        .unwrap_or(0.0);

    // Check for persistent storage addon
    let mut has_persistent_storage = false;
    let mut storage_size_gb: Option<u32> = None;

    for addon in &sub.addons {
        // Check for storage addon - API returns type="STORAGE", subType="PERMANENT_STORAGE", status="OK"
        if addon.addon_type.as_deref() == Some("STORAGE")
            && addon.sub_type.as_deref() == Some("PERMANENT_STORAGE")
            && addon.status.as_deref() == Some("OK")
        {
            has_persistent_storage = true;
            // Try to find storage size from attributes (key is "TOTAL_STORAGE_SIZE_IN_GB")
            for attr in &addon.attributes {
                if attr.key.as_deref() == Some("TOTAL_STORAGE_SIZE_IN_GB") {
                    if let Some(val) = attr.text_value.as_ref() {
                        storage_size_gb = val.parse().ok();
                    }
                }
            }
        }
    }

    info!("Subscription: tier={}, hours={:.1}/{:.1}, storage={}, subType={:?}",
        sub.membership_tier, remaining_hours, total_hours, has_persistent_storage, sub.sub_type);

    // Check if this is an unlimited subscription (no hour cap)
    let is_unlimited = sub.sub_type.as_deref() == Some("UNLIMITED");

    // Extract entitled resolutions
    let mut entitled_resolutions = Vec::new();
    if let Some(features) = sub.features {
        for res in features.resolutions {
            // User requested to ignore entitlement check (include all resolutions)
            entitled_resolutions.push(crate::app::types::EntitledResolution {
                width: res.width_in_pixels,
                height: res.height_in_pixels,
                fps: res.frames_per_second,
            });
        }
    }
    
    // Sort to be nice (highest res/fps first)
    entitled_resolutions.sort_by(|a, b| {
        b.width.cmp(&a.width)
            .then(b.height.cmp(&a.height))
            .then(b.fps.cmp(&a.fps))
    });

    Ok(crate::app::SubscriptionInfo {
        membership_tier: sub.membership_tier,
        remaining_hours,
        total_hours,
        has_persistent_storage,
        storage_size_gb,
        is_unlimited,
        entitled_resolutions,
    })
}
