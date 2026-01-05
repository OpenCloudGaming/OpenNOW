//! PrintedWaste Queue Times API
//!
//! Fetches queue time information from PrintedWaste API for GeForce NOW servers.

use log::{info, warn, debug};
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;

/// Server mapping data from PrintedWaste
#[derive(Debug, Clone)]
pub struct ServerMapping {
    pub title: String,
    pub region: String,
    pub is4080_server: bool,
    pub is5080_server: bool,
    pub nuked: bool,
}

// Custom deserialization for the weird field names
impl ServerMapping {
    fn from_raw(raw: RawServerMapping) -> Self {
        Self {
            title: raw.title,
            region: raw.region,
            is4080_server: raw.is_4080_server,
            is5080_server: raw.is_5080_server,
            nuked: raw.nuked,
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawServerMapping {
    title: String,
    region: String,
    #[serde(default, rename = "is4080Server")]
    is_4080_server: bool,
    #[serde(default, rename = "is5080Server")]
    is_5080_server: bool,
    #[serde(default)]
    nuked: bool,
}

/// Queue data for a server from PrintedWaste
#[derive(Debug, Clone, Deserialize)]
pub struct QueueData {
    #[serde(rename = "QueuePosition")]
    pub queue_position: i32,
    #[serde(rename = "Last Updated")]
    pub last_updated: i64,
    #[serde(rename = "Region")]
    pub region: String,
    /// ETA in milliseconds
    #[serde(default)]
    pub eta: Option<i64>,
}

/// Response from PrintedWaste queue API
#[derive(Debug, Deserialize)]
pub struct QueueResponse {
    #[serde(default)]
    pub status: bool,
    #[serde(default)]
    pub errors: Vec<String>,
    pub data: HashMap<String, QueueData>,
}

/// Response from PrintedWaste server mapping API
#[derive(Debug, Deserialize)]
struct RawMappingResponse {
    #[serde(default)]
    status: bool,
    #[serde(default)]
    errors: Vec<String>,
    data: HashMap<String, RawServerMapping>,
}

/// Combined server info for queue display
#[derive(Debug, Clone)]
pub struct QueueServerInfo {
    pub server_id: String,
    pub display_name: String,
    pub region: String,
    pub ping_ms: Option<u32>,
    pub queue_position: i32,
    pub eta_seconds: Option<i64>,
    pub is_4080_server: bool,
    pub is_5080_server: bool,
    pub last_updated: i64,
}

/// App version for User-Agent header
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// PrintedWaste API endpoints
const QUEUE_API_URL: &str = "https://api.printedwaste.com/gfn/queue/";
const MAPPING_API_URL: &str = "https://remote.printedwaste.com/config/GFN_SERVERID_TO_REGION_MAPPING";

/// Fetch server mapping from PrintedWaste
pub async fn fetch_server_mapping(client: &Client) -> Result<HashMap<String, ServerMapping>, String> {
    let user_agent = format!("OpenNOW/{}", APP_VERSION);

    debug!("Fetching server mapping from PrintedWaste...");

    let response = client
        .get(MAPPING_API_URL)
        .header("User-Agent", &user_agent)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch server mapping: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Server mapping API returned status: {}", response.status()));
    }

    let body = response.text().await
        .map_err(|e| format!("Failed to read server mapping response: {}", e))?;

    let raw: RawMappingResponse = serde_json::from_str(&body)
        .map_err(|e| format!("Failed to parse server mapping: {}", e))?;

    if !raw.errors.is_empty() {
        return Err(format!("Server mapping API returned errors: {:?}", raw.errors));
    }

    // Convert raw mappings to our struct
    let mappings: HashMap<String, ServerMapping> = raw.data
        .into_iter()
        .map(|(k, v)| (k, ServerMapping::from_raw(v)))
        .collect();

    info!("Fetched {} server mappings from PrintedWaste", mappings.len());
    Ok(mappings)
}

/// Fetch queue data from PrintedWaste
pub async fn fetch_queue_data(client: &Client) -> Result<QueueResponse, String> {
    let user_agent = format!("OpenNOW/{}", APP_VERSION);

    debug!("Fetching queue data from PrintedWaste...");

    let response = client
        .get(QUEUE_API_URL)
        .header("User-Agent", &user_agent)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch queue data: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Queue API returned status: {}", response.status()));
    }

    let body = response.text().await
        .map_err(|e| format!("Failed to read queue response: {}", e))?;

    let queue: QueueResponse = serde_json::from_str(&body)
        .map_err(|e| format!("Failed to parse queue data: {}", e))?;

    if !queue.errors.is_empty() {
        return Err(format!("Queue API returned errors: {:?}", queue.errors));
    }

    info!("Fetched queue data for {} servers from PrintedWaste", queue.data.len());
    Ok(queue)
}

/// Fetch combined queue server info
pub async fn fetch_queue_servers(client: &Client) -> Result<Vec<QueueServerInfo>, String> {
    // Fetch both mapping and queue data
    let (mapping_result, queue_result) = tokio::join!(
        fetch_server_mapping(client),
        fetch_queue_data(client)
    );

    let mapping = mapping_result?;
    let queue = queue_result?;

    let mut servers: Vec<QueueServerInfo> = Vec::new();

    for (server_id, server_mapping) in &mapping {
        // Skip nuked servers
        if server_mapping.nuked {
            continue;
        }

        // Only include RTX 4080 or 5080 servers
        if !server_mapping.is4080_server && !server_mapping.is5080_server {
            continue;
        }

        // Get queue data for this server
        if let Some(queue_data) = queue.data.get(server_id) {
            servers.push(QueueServerInfo {
                server_id: server_id.clone(),
                display_name: server_mapping.title.clone(),
                // Use the simple region from queue API (e.g., "US", "EU")
                // NOT the detailed region from mapping (e.g., "US Central")
                region: queue_data.region.clone(),
                ping_ms: None, // Will be filled in by caller if needed
                queue_position: queue_data.queue_position,
                eta_seconds: queue_data.eta.map(|ms| ms / 1000), // Convert ms to seconds
                is_4080_server: server_mapping.is4080_server,
                is_5080_server: server_mapping.is5080_server,
                last_updated: queue_data.last_updated,
            });
        }
    }

    // Sort by queue position (shortest first)
    servers.sort_by(|a, b| a.queue_position.cmp(&b.queue_position));

    Ok(servers)
}

/// Format ETA in a human-readable format
pub fn format_queue_eta(eta_seconds: Option<i64>) -> String {
    match eta_seconds {
        None | Some(0) => "No wait".to_string(),
        Some(secs) if secs < 0 => "No wait".to_string(),
        Some(secs) if secs < 60 => format!("{}s", secs),
        Some(secs) if secs < 3600 => {
            let minutes = secs / 60;
            format!("{}m", minutes)
        }
        Some(secs) if secs < 86400 => {
            let hours = secs / 3600;
            let minutes = (secs % 3600) / 60;
            if minutes > 0 {
                format!("{}h {}m", hours, minutes)
            } else {
                format!("{}h", hours)
            }
        }
        Some(secs) => {
            let days = secs / 86400;
            let hours = (secs % 86400) / 3600;
            if hours > 0 {
                format!("{}d {}h", days, hours)
            } else {
                format!("{}d", days)
            }
        }
    }
}

/// Calculate server score for "best value" sorting
/// Lower score = better server
/// Balances ping (important for gameplay) with queue time
pub fn calculate_server_score(server: &QueueServerInfo) -> f64 {
    // Ping weight: 1.0 per ms
    // ETA weight: 0.1 per minute (capped at 50 to prevent extremely long queues from dominating)
    let ping_score = server.ping_ms.unwrap_or(500) as f64; // High penalty for unknown ping
    let eta_minutes = (server.eta_seconds.unwrap_or(0) as f64) / 60.0;
    let eta_score = (eta_minutes * 0.5).min(100.0); // 0.5 per minute, capped at 100

    ping_score + eta_score
}

/// Get the best auto-selected server based on ping and queue time balance
pub fn get_auto_selected_server(servers: &[QueueServerInfo]) -> Option<&QueueServerInfo> {
    if servers.is_empty() {
        return None;
    }

    servers.iter().min_by(|a, b| {
        let score_a = calculate_server_score(a);
        let score_b = calculate_server_score(b);
        score_a.partial_cmp(&score_b).unwrap_or(std::cmp::Ordering::Equal)
    })
}

/// Get unique regions from server list
pub fn get_unique_regions(servers: &[QueueServerInfo]) -> Vec<String> {
    let mut regions: Vec<String> = servers
        .iter()
        .map(|s| s.region.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    regions.sort();
    regions
}

/// Sort servers by the specified mode
pub fn sort_servers(servers: &mut [QueueServerInfo], mode: crate::app::QueueSortMode) {
    use crate::app::QueueSortMode;

    match mode {
        QueueSortMode::BestValue => {
            servers.sort_by(|a, b| {
                let score_a = calculate_server_score(a);
                let score_b = calculate_server_score(b);
                score_a.partial_cmp(&score_b).unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        QueueSortMode::QueueTime => {
            servers.sort_by(|a, b| {
                let eta_a = a.eta_seconds.unwrap_or(i64::MAX);
                let eta_b = b.eta_seconds.unwrap_or(i64::MAX);
                eta_a.cmp(&eta_b)
            });
        }
        QueueSortMode::Ping => {
            servers.sort_by(|a, b| {
                let ping_a = a.ping_ms.unwrap_or(u32::MAX);
                let ping_b = b.ping_ms.unwrap_or(u32::MAX);
                ping_a.cmp(&ping_b)
            });
        }
        QueueSortMode::Alphabetical => {
            servers.sort_by(|a, b| a.display_name.cmp(&b.display_name));
        }
    }
}
