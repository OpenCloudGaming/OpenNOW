//! Games Library API
//!
//! Fetch and search GFN game catalog using GraphQL.

use anyhow::{Result, Context};
use log::{info, debug, warn, error};
use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::app::{GameInfo, GameSection, GameVariant};
use crate::auth;
use super::GfnApiClient;

/// GraphQL endpoint
const GRAPHQL_URL: &str = "https://games.geforce.com/graphql";

/// Persisted query hash for panels (MAIN, LIBRARY, etc.)
const PANELS_QUERY_HASH: &str = "f8e26265a5db5c20e1334a6872cf04b6e3970507697f6ae55a6ddefa5420daf0";

/// Persisted query hash for app metadata
const APP_METADATA_QUERY_HASH: &str = "39187e85b6dcf60b7279a5f233288b0a8b69a8b1dbcfb5b25555afdcb988f0d7";

/// GFN CEF User-Agent
const GFN_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36 NVIDIACEFClient/HEAD/debb5919f6 GFN-PC/2.0.80.173";

/// Default VPC ID for general access (from GFN config)
const DEFAULT_VPC_ID: &str = "GFN-PC";

/// Default locale
const DEFAULT_LOCALE: &str = "en_US";

/// LCARS Client ID
const LCARS_CLIENT_ID: &str = "ec7e38d4-03af-4b58-b131-cfb0495903ab";

/// GFN client version
const GFN_CLIENT_VERSION: &str = "2.0.80.173";

// ============================================
// GraphQL Response Types (matching Tauri client)
// ============================================

#[derive(Debug, Deserialize)]
struct GraphQLResponse {
    data: Option<PanelsData>,
    errors: Option<Vec<GraphQLError>>,
}

#[derive(Debug, Deserialize)]
struct AppMetaDataResponse {
    data: Option<AppsData>,
    errors: Option<Vec<GraphQLError>>,
}

#[derive(Debug, Deserialize)]
struct AppsData {
    apps: ItemsData,
}

#[derive(Debug, Deserialize)]
struct ItemsData {
    items: Vec<AppData>,
}

#[derive(Debug, Deserialize)]
struct GraphQLError {
    message: String,
}

#[derive(Debug, Deserialize)]
struct PanelsData {
    panels: Vec<Panel>,
}

#[derive(Debug, Deserialize)]
struct Panel {
    #[allow(dead_code)]
    id: Option<String>,
    name: String,
    #[serde(default)]
    sections: Vec<PanelSection>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PanelSection {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    items: Vec<PanelItem>,
}

/// Panel items are tagged by __typename
#[derive(Debug, Deserialize)]
#[serde(tag = "__typename")]
enum PanelItem {
    GameItem { app: AppData },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppData {
    id: String,
    title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    long_description: Option<String>,
    #[serde(default)]
    images: Option<AppImages>,
    #[serde(default)]
    variants: Option<Vec<AppVariant>>,
    #[serde(default)]
    gfn: Option<AppGfnStatus>,
}

/// Image URLs from GraphQL
#[derive(Debug, Deserialize)]
struct AppImages {
    #[serde(rename = "GAME_BOX_ART")]
    game_box_art: Option<String>,
    #[serde(rename = "TV_BANNER")]
    tv_banner: Option<String>,
    #[serde(rename = "HERO_IMAGE")]
    hero_image: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppVariant {
    id: String,
    app_store: String,
    #[serde(default)]
    supported_controls: Option<Vec<String>>,
    #[serde(default)]
    gfn: Option<VariantGfnStatus>,
}

#[derive(Debug, Deserialize)]
struct VariantGfnStatus {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    library: Option<VariantLibraryStatus>,
}

#[derive(Debug, Deserialize)]
struct VariantLibraryStatus {
    #[serde(default)]
    selected: Option<bool>,
    #[serde(default)]
    installed: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppGfnStatus {
    #[serde(default)]
    playability_state: Option<String>,
    #[serde(default)]
    play_type: Option<String>,
    #[serde(default)]
    minimum_membership_tier_label: Option<String>,
    #[serde(default)]
    catalog_sku_strings: Option<CatalogSkuStrings>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
struct CatalogSkuStrings {
    #[serde(default)]
    sku_based_playability_text: Option<String>,
    #[serde(default)]
    sku_based_tag: Option<String>,
}

// ============================================
// Raw game data from static JSON
// ============================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawGameInfo {
    /// Game ID (numeric in public list)
    #[serde(default)]
    id: Option<serde_json::Value>,
    /// Game title
    #[serde(default)]
    title: Option<String>,
    /// Publisher name
    #[serde(default)]
    publisher: Option<String>,
    /// Store type (Steam, Epic, etc.)
    #[serde(default)]
    store: Option<String>,
    /// Steam URL (contains app ID)
    #[serde(default)]
    steam_url: Option<String>,
    /// Epic URL
    #[serde(default)]
    epic_url: Option<String>,
    /// Status (AVAILABLE, etc.)
    #[serde(default)]
    status: Option<String>,
    /// Genres
    #[serde(default)]
    genres: Vec<String>,
}

/// Generate a random huId for GraphQL requests
fn generate_hu_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:x}", timestamp)
}

/// Optimize image URL with webp format and size
fn optimize_image_url(url: &str, width: u32) -> String {
    if url.contains("img.nvidiagrid.net") {
        format!("{};f=webp;w={}", url, width)
    } else {
        url.to_string()
    }
}

impl GfnApiClient {
    /// Fetch panels using persisted query (GET request)
    /// This is the correct way to fetch from GFN API
    async fn fetch_panels(&self, panel_names: &[&str], vpc_id: &str) -> Result<Vec<Panel>> {
        let token = self.token()
            .context("No access token for panel fetch")?;

        let variables = serde_json::json!({
            "vpcId": vpc_id,
            "locale": DEFAULT_LOCALE,
            "panelNames": panel_names,
        });

        let extensions = serde_json::json!({
            "persistedQuery": {
                "sha256Hash": PANELS_QUERY_HASH
            }
        });

        // Build request type based on panel names
        let request_type = if panel_names.contains(&"LIBRARY") {
            "panels/Library"
        } else {
            "panels/MainV2"
        };

        let variables_str = serde_json::to_string(&variables)
            .context("Failed to serialize variables")?;
        let extensions_str = serde_json::to_string(&extensions)
            .context("Failed to serialize extensions")?;

        let hu_id = generate_hu_id();

        // Build URL with all required parameters
        let url = format!(
            "{}?requestType={}&extensions={}&huId={}&variables={}",
            GRAPHQL_URL,
            urlencoding::encode(request_type),
            urlencoding::encode(&extensions_str),
            urlencoding::encode(&hu_id),
            urlencoding::encode(&variables_str)
        );

        debug!("Fetching panels from: {}", url);

        let response = self.client
            .get(&url)
            .header("User-Agent", GFN_USER_AGENT)
            .header("Accept", "application/json, text/plain, */*")
            .header("Content-Type", "application/graphql")
            .header("Origin", "https://play.geforcenow.com")
            .header("Referer", "https://play.geforcenow.com/")
            .header("Authorization", format!("GFNJWT {}", token))
            // GFN client headers (native client)
            .header("nv-client-id", LCARS_CLIENT_ID)
            .header("nv-client-type", "NATIVE")
            .header("nv-client-version", GFN_CLIENT_VERSION)
            .header("nv-client-streamer", "NVIDIA-CLASSIC")
            .header("nv-device-os", "WINDOWS")
            .header("nv-device-type", "DESKTOP")
            .header("nv-device-make", "UNKNOWN")
            .header("nv-device-model", "UNKNOWN")
            .header("nv-browser-type", "CHROME")
            .send()
            .await
            .context("Panel fetch request failed")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Panel fetch failed: {} - {}", status, body));
        }

        let body_text = response.text().await
            .context("Failed to read panel response")?;

        debug!("Panel response (first 500 chars): {}", &body_text[..body_text.len().min(500)]);

        let graphql_response: GraphQLResponse = serde_json::from_str(&body_text)
            .context(format!("Failed to parse panel response: {}", &body_text[..body_text.len().min(200)]))?;

        if let Some(errors) = graphql_response.errors {
            if !errors.is_empty() {
                let error_msg = errors.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join(", ");
                return Err(anyhow::anyhow!("GraphQL errors: {}", error_msg));
            }
        }

        Ok(graphql_response.data
            .map(|d| d.panels)
            .unwrap_or_default())
    }

    /// Convert AppData to GameInfo
    fn app_to_game_info(app: AppData) -> GameInfo {
        // Build variants list from app variants
        let variants: Vec<GameVariant> = app.variants.as_ref()
            .map(|vars| vars.iter().map(|v| GameVariant {
                id: v.id.clone(),
                store: v.app_store.clone(),
                supported_controls: v.supported_controls.clone().unwrap_or_default(),
            }).collect())
            .unwrap_or_default();

        // Find selected variant index (the one marked as selected, or 0 for first)
        let selected_variant_index = app.variants.as_ref()
            .and_then(|vars| vars.iter().position(|v| {
                v.gfn.as_ref()
                    .and_then(|g| g.library.as_ref())
                    .and_then(|l| l.selected)
                    .unwrap_or(false)
            }))
            .unwrap_or(0);

        // Get the selected variant for current store/id
        let selected_variant = variants.get(selected_variant_index);

        let store = selected_variant
            .map(|v| v.store.clone())
            .unwrap_or_else(|| "Unknown".to_string());

        // Use variant ID for launching (e.g., "102217611")
        let variant_id = selected_variant
            .map(|v| v.id.clone())
            .unwrap_or_default();

        // Parse app_id from variant ID (may be numeric)
        let app_id = variant_id.parse::<i64>().ok();

        // Optimize image URLs (272px width for cards, webp format)
        // Prefer GAME_BOX_ART over TV_BANNER for better quality box art
        let image_url = app.images.as_ref()
            .and_then(|i| i.game_box_art.as_ref().or(i.tv_banner.as_ref()).or(i.hero_image.as_ref()))
            .map(|url| optimize_image_url(url, 272));

        // Check if playType is INSTALL_TO_PLAY
        let is_install_to_play = app.gfn.as_ref()
            .and_then(|g| g.play_type.as_deref())
            .map(|t| t == "INSTALL_TO_PLAY")
            .unwrap_or(false);

        GameInfo {
            id: if variant_id.is_empty() { app.id.clone() } else { variant_id },
            title: app.title,
            publisher: None,
            image_url,
            store,
            app_id,
            is_install_to_play,
            play_type: app.gfn.as_ref().and_then(|g| g.play_type.clone()),
            membership_tier_label: app.gfn.as_ref().and_then(|g| g.minimum_membership_tier_label.clone()),
            playability_text: app.gfn.as_ref().and_then(|g| g.catalog_sku_strings.as_ref()).and_then(|s| s.sku_based_playability_text.clone()),
            uuid: Some(app.id.clone()),
            description: app.description.or(app.long_description),
            variants,
            selected_variant_index,
        }
    }

    /// Fetch games from MAIN panel (GraphQL with images)
    pub async fn fetch_main_games(&self, vpc_id: Option<&str>) -> Result<Vec<GameInfo>> {
        // Use provided VPC ID or fetch dynamically from serverInfo
        let vpc = match vpc_id {
            Some(v) => v.to_string(),
            None => {
                let token = self.token().map(|s| s.as_str());
                super::get_vpc_id(&self.client, token).await
            }
        };

        info!("Fetching main games from GraphQL (VPC: {})", vpc);

        let panels = self.fetch_panels(&["MAIN"], &vpc).await?;

        let mut games: Vec<GameInfo> = Vec::new();

        for panel in panels {
            info!("Panel '{}' has {} sections", panel.name, panel.sections.len());
            for section in panel.sections {
                debug!("Section has {} items", section.items.len());
                for item in section.items {
                    if let PanelItem::GameItem { app } = item {
                        debug!("Found game: {} with images: {:?}", app.title, app.images.is_some());
                        games.push(Self::app_to_game_info(app));
                    }
                }
            }
        }

        info!("Fetched {} games from MAIN panel", games.len());
        Ok(games)
    }

    /// Fetch games organized by section (Home view)
    /// Returns sections with titles like "Trending", "Free to Play", etc.
    pub async fn fetch_sectioned_games(&self, vpc_id: Option<&str>) -> Result<Vec<GameSection>> {
        // Use provided VPC ID or fetch dynamically from serverInfo
        let vpc = match vpc_id {
            Some(v) => v.to_string(),
            None => {
                let token = self.token().map(|s| s.as_str());
                super::get_vpc_id(&self.client, token).await
            }
        };

        info!("Fetching sectioned games from GraphQL (VPC: {})", vpc);

        let panels = self.fetch_panels(&["MAIN"], &vpc).await?;

        let mut sections: Vec<GameSection> = Vec::new();

        for panel in panels {
            info!("Panel '{}' has {} sections", panel.name, panel.sections.len());
            for section in panel.sections {
                let section_title = section.title.clone().unwrap_or_else(|| "Games".to_string());
                debug!("Section '{}' has {} items", section_title, section.items.len());
                
                let games: Vec<GameInfo> = section.items
                    .into_iter()
                    .filter_map(|item| {
                        if let PanelItem::GameItem { app } = item {
                            Some(Self::app_to_game_info(app))
                        } else {
                            None
                        }
                    })
                    .collect();
                
                if !games.is_empty() {
                    info!("Section '{}': {} games", section_title, games.len());
                    sections.push(GameSection {
                        id: section.id,
                        title: section_title,
                        games,
                    });
                }
            }
        }

        info!("Fetched {} sections from MAIN panel", sections.len());
        Ok(sections)
    }

    /// Fetch user's library (GraphQL)
    pub async fn fetch_library(&self, vpc_id: Option<&str>) -> Result<Vec<GameInfo>> {
        // Use provided VPC ID or fetch dynamically from serverInfo
        let vpc = match vpc_id {
            Some(v) => v.to_string(),
            None => {
                let token = self.token().map(|s| s.as_str());
                super::get_vpc_id(&self.client, token).await
            }
        };

        info!("Fetching library from GraphQL (VPC: {})", vpc);

        let panels = match self.fetch_panels(&["LIBRARY"], &vpc).await {
            Ok(p) => p,
            Err(e) => {
                warn!("Library fetch failed: {}", e);
                return Ok(Vec::new());
            }
        };

        let mut games: Vec<GameInfo> = Vec::new();

        for panel in panels {
            if panel.name == "LIBRARY" {
                for section in panel.sections {
                    for item in section.items {
                        if let PanelItem::GameItem { app } = item {
                            games.push(Self::app_to_game_info(app));
                        }
                    }
                }
            }
        }

        info!("Fetched {} games from LIBRARY panel", games.len());
        Ok(games)
    }

    /// Fetch public games list (static JSON, no auth required)
    /// Uses Steam CDN for game images when available
    pub async fn fetch_public_games(&self) -> Result<Vec<GameInfo>> {
        let url = "https://static.nvidiagrid.net/supported-public-game-list/locales/gfnpc-en-US.json";

        info!("Fetching public games from: {}", url);

        let response = self.client.get(url)
            .header("User-Agent", GFN_USER_AGENT)
            .send()
            .await
            .context("Failed to fetch games list")?;

        let text = response.text().await
            .context("Failed to read games response")?;

        debug!("Fetched {} bytes of games data", text.len());

        let raw_games: Vec<RawGameInfo> = serde_json::from_str(&text)
            .context("Failed to parse games JSON")?;

        let games: Vec<GameInfo> = raw_games.into_iter()
            .filter_map(|g| {
                let title = g.title?;

                // Extract ID (can be number or string)
                let id = match g.id {
                    Some(serde_json::Value::Number(n)) => n.to_string(),
                    Some(serde_json::Value::String(s)) => s,
                    _ => title.clone(),
                };

                // Extract Steam app ID from steamUrl
                // Format: https://store.steampowered.com/app/123456
                let app_id = g.steam_url
                    .as_ref()
                    .and_then(|url| {
                        url.split("/app/")
                            .nth(1)
                            .and_then(|s| s.split('/').next())
                            .and_then(|s| s.parse::<i64>().ok())
                    });

                // Skip games that aren't available
                if g.status.as_deref() != Some("AVAILABLE") {
                    return None;
                }

                // Generate image URL from Steam CDN if we have a Steam app ID
                // Steam CDN provides public game art: header.jpg (460x215), library_600x900.jpg
                let image_url = app_id.map(|steam_id| {
                    format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{}/library_600x900.jpg", steam_id)
                });

                let store = g.store.unwrap_or_else(|| "Unknown".to_string());

                Some(GameInfo {
                    id,
                    title,
                    publisher: g.publisher,
                    image_url,
                    store,
                    app_id,
                    is_install_to_play: false,
                    play_type: None,
                    membership_tier_label: None,
                    playability_text: None,
                    uuid: None,
                    description: None,
                    variants: Vec::new(),
                    selected_variant_index: 0,
                })
            })
            .collect();

        info!("Parsed {} games from public list", games.len());
        Ok(games)
    }
        /// Search games by title
    pub fn search_games<'a>(games: &'a [GameInfo], query: &str) -> Vec<&'a GameInfo> {
        let query_lower = query.to_lowercase();

        games.iter()
            .filter(|g| g.title.to_lowercase().contains(&query_lower))
            .collect()
    }

    /// Fetch full details for a specific app (including playType)
    pub async fn fetch_app_details(&self, app_id: &str) -> Result<Option<GameInfo>> {
        let token = self.token()
            .context("No access token for app details")?;

        // Get VPC ID
        let vpc_id = super::get_vpc_id(&self.client, Some(token)).await;

        let variables = serde_json::json!({
            "vpcId": vpc_id,
            "locale": DEFAULT_LOCALE,
            "appIds": [app_id],
        });

        let extensions = serde_json::json!({
            "persistedQuery": {
                "sha256Hash": APP_METADATA_QUERY_HASH
            }
        });

        let variables_str = serde_json::to_string(&variables)?;
        let extensions_str = serde_json::to_string(&extensions)?;
        let hu_id = generate_hu_id();

        let url = format!(
            "{}?requestType=appMetaData&extensions={}&huId={}&variables={}",
            GRAPHQL_URL,
            urlencoding::encode(&extensions_str),
            urlencoding::encode(&hu_id),
            urlencoding::encode(&variables_str)
        );

        debug!("Fetching app details from: {}", url);
        info!("Fetching app details for ID: {} (Variables: {})", app_id, variables_str);

        let response = self.client
            .get(&url)
            .header("User-Agent", GFN_USER_AGENT)
            .header("Accept", "application/json")
            .header("Content-Type", "application/graphql") 
            .header("Authorization", format!("GFNJWT {}", token))
            .header("nv-client-id", LCARS_CLIENT_ID)
            .send()
            .await
            .context("App details request failed")?;

        if !response.status().is_success() {
             let status = response.status();
             let error_body = response.text().await.unwrap_or_else(|_| "Could not read error body".to_string());
             error!("App details failed for {}: {} - Body: {}", app_id, status, error_body);
             return Err(anyhow::anyhow!("App details failed: {} - {}", status, error_body));
        }

        let body = response.text().await?;
        let response_data: AppMetaDataResponse = serde_json::from_str(&body)
            .context("Failed to parse app details")?;

        if let Some(data) = response_data.data {
             if let Some(app) = data.apps.items.into_iter().next() {
                 return Ok(Some(Self::app_to_game_info(app)));
             }
        }

        Ok(None)
    }
}



/// Fetch server info to get VPC ID for current provider
pub async fn fetch_server_info(access_token: Option<&str>) -> Result<ServerInfo> {
    let base_url = auth::get_streaming_base_url();
    let url = format!("{}v2/serverInfo", base_url);

    info!("Fetching server info from: {}", url);

    let client = reqwest::Client::builder()
        .user_agent(GFN_USER_AGENT)
        .build()?;

    let mut request = client
        .get(&url)
        .header("Accept", "application/json")
        .header("nv-client-id", LCARS_CLIENT_ID)
        .header("nv-client-type", "BROWSER")
        .header("nv-client-version", GFN_CLIENT_VERSION)
        .header("nv-client-streamer", "WEBRTC")
        .header("nv-device-os", "WINDOWS")
        .header("nv-device-type", "DESKTOP");

    if let Some(token) = access_token {
        request = request.header("Authorization", format!("GFNJWT {}", token));
    }

    let response = request.send().await
        .context("Server info request failed")?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!("Server info failed: {}", response.status()));
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ServerInfoResponse {
        request_status: Option<RequestStatus>,
        #[serde(default)]
        meta_data: Vec<MetaDataEntry>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct RequestStatus {
        server_id: Option<String>,
    }

    #[derive(Deserialize)]
    struct MetaDataEntry {
        key: String,
        value: String,
    }

    let server_response: ServerInfoResponse = response.json().await
        .context("Failed to parse server info")?;

    let vpc_id = server_response.request_status
        .and_then(|s| s.server_id)
        .unwrap_or_else(|| DEFAULT_VPC_ID.to_string());

    // Extract regions from metaData
    let mut regions: Vec<(String, String)> = Vec::new();
    for meta in server_response.meta_data {
        if meta.value.starts_with("https://") {
            regions.push((meta.key, meta.value));
        }
    }

    info!("Server info: VPC={}, {} regions", vpc_id, regions.len());

    Ok(ServerInfo { vpc_id, regions })
}

/// Server info result
#[derive(Debug, Clone)]
pub struct ServerInfo {
    pub vpc_id: String,
    pub regions: Vec<(String, String)>,
}
