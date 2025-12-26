use serde::{Deserialize, Serialize};
use tauri::command;
use reqwest::Client;

/// Game variant (different store versions)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameVariant {
    pub id: String,
    pub store_type: StoreType,
    pub supported_controls: Vec<String>,
}

/// Game information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Game {
    pub id: String,
    pub title: String,
    pub publisher: Option<String>,
    pub developer: Option<String>,
    pub genres: Vec<String>,
    pub images: GameImages,
    pub store: StoreInfo,
    pub status: GameStatus,
    pub supported_controls: Vec<String>,
    #[serde(default)]
    pub variants: Vec<GameVariant>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameImages {
    pub box_art: Option<String>,
    pub hero: Option<String>,
    pub thumbnail: Option<String>,
    pub screenshots: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreInfo {
    pub store_type: StoreType,
    pub store_id: String,
    pub store_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StoreType {
    Steam,
    Epic,
    Ubisoft,
    Origin,
    GoG,
    Xbox,
    EaApp,
    Other(String),
}

impl Serialize for StoreType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for StoreType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(StoreType::from(s.as_str()))
    }
}

impl std::fmt::Display for StoreType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreType::Steam => write!(f, "Steam"),
            StoreType::Epic => write!(f, "Epic"),
            StoreType::Ubisoft => write!(f, "Ubisoft"),
            StoreType::Origin => write!(f, "Origin"),
            StoreType::GoG => write!(f, "GOG"),
            StoreType::Xbox => write!(f, "Xbox"),
            StoreType::EaApp => write!(f, "EA"),
            StoreType::Other(s) => write!(f, "{}", s),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameStatus {
    Available,
    Maintenance,
    Unavailable,
}

/// Server information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    pub id: String,
    pub name: String,
    pub region: String,
    pub country: String,
    pub ping_ms: Option<u32>,
    pub queue_size: Option<u32>,
    pub status: ServerStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerStatus {
    Online,
    Busy,
    Maintenance,
    Offline,
}

/// Game library response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GamesResponse {
    pub games: Vec<Game>,
    pub total_count: u32,
    pub page: u32,
    pub page_size: u32,
}

/// API endpoints discovered from GFN client analysis
/// GraphQL endpoint (uses persisted queries with GET)
const GRAPHQL_URL: &str = "https://games.geforce.com/graphql";
/// Static JSON endpoint for game list (public, no auth required)
const STATIC_GAMES_URL: &str = "https://static.nvidiagrid.net/supported-public-game-list/locales/gfnpc-en-US.json";
/// Image CDN base URL
#[allow(dead_code)]
const IMAGE_CDN_BASE: &str = "https://img.nvidiagrid.net";
/// LCARS Client ID
const LCARS_CLIENT_ID: &str = "ec7e38d4-03af-4b58-b131-cfb0495903ab";
/// CloudMatch server for streaming sessions
#[allow(dead_code)]
const CLOUDMATCH_URL: &str = "https://prod.cloudmatchbeta.nvidiagrid.net/v2";

/// GFN client version
const GFN_CLIENT_VERSION: &str = "2.0.80.173";

/// MES (Membership/Subscription) API URL
const MES_URL: &str = "https://mes.geforcenow.com/v4/subscriptions";

/// GFN CEF User-Agent (native client)
const GFN_CEF_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36 NVIDIACEFClient/HEAD/debb5919f6 GFN-PC/2.0.80.173";

/// Persisted query hash for panels (MAIN, LIBRARY, etc.)
const PANELS_QUERY_HASH: &str = "f8e26265a5db5c20e1334a6872cf04b6e3970507697f6ae55a6ddefa5420daf0";
/// Persisted query hash for static app data
const STATIC_APP_DATA_HASH: &str = "fd555528201fe16f28011637244243e170368bc68e06b040a132a7c177c9ed2a";
/// Persisted query hash for search
const SEARCH_QUERY_HASH: &str = "7581d1b6e4d87013ac88e58bff8294b5a9fb4dee1aa0d98c1719dac9d8e9dcf7";

/// GraphQL query for fetching game sections (featured games, categories)
/// Based on GetGameSection query from GFN client
const GET_GAME_SECTION_QUERY: &str = r#"
query GetGameSection($vpcId: String!, $locale: String!, $panelNames: [String!]!) {
    panels(vpcId: $vpcId, locale: $locale, panelNames: $panelNames) {
        name
        sections {
            id
            title
            items {
                ... on GameItem {
                    app {
                        id
                        title
                        publisherName
                        images {
                            GAME_BOX_ART
                            TV_BANNER
                            HERO_IMAGE
                            GAME_LOGO
                        }
                        variants {
                            id
                            shortName
                            appStore
                            supportedControls
                            gfn {
                                status
                            }
                        }
                        gfn {
                            playabilityState
                            minimumMembershipTierLabel
                        }
                    }
                }
            }
        }
    }
}
"#;

/// GraphQL query for fetching user's library
const GET_LIBRARY_QUERY: &str = r#"
query GetLibrary($vpcId: String!, $locale: String!, $panelNames: [String!]!) {
    panels(vpcId: $vpcId, locale: $locale, panelNames: $panelNames) {
        id
        name
        sections {
            id
            title
            items {
                __typename
                ... on GameItem {
                    app {
                        id
                        title
                        images {
                            GAME_BOX_ART
                            TV_BANNER
                            HERO_IMAGE
                        }
                        library {
                            favorited
                        }
                        variants {
                            id
                            shortName
                            appStore
                            supportedControls
                            gfn {
                                library {
                                    status
                                    selected
                                }
                                status
                            }
                        }
                        gfn {
                            playabilityState
                            playType
                        }
                    }
                }
            }
        }
    }
}
"#;


/// GraphQL query for detailed app/game data
/// Based on GetAppDataQueryForAppId from GFN client
const GET_APP_DATA_QUERY: &str = r#"
query GetAppDataQueryForAppId($vpcId: String!, $locale: String!, $appIds: [String!]!) {
    apps(vpcId: $vpcId, locale: $locale, appIds: $appIds) {
        id
        title
        shortDescription
        longDescription
        publisherName
        developerName
        genres
        contentRatings {
            categoryKey
            contentDescriptorKeys
        }
        images {
            GAME_BOX_ART
            HERO_IMAGE
            GAME_LOGO
            SCREENSHOTS
            KEY_ART
        }
        variants {
            id
            shortName
            appStore
            supportedControls
            gfn {
                status
                installTimeInMinutes
            }
            libraryStatus {
                installed
                status
                selected
            }
        }
        gfn {
            playabilityState
            minimumMembershipTierLabel
            catalogSkuStrings
        }
        maxLocalPlayers
        maxOnlinePlayers
        releaseDate
        itemMetadata {
            favorited
        }
    }
}
"#;

/// GraphQL mutation to add game to favorites
const ADD_FAVORITE_MUTATION: &str = r#"
mutation AddFavoriteApp($appId: String!, $locale: String!) {
    addFavoriteApp(appId: $appId, locale: $locale) {
        appId
    }
}
"#;

/// GraphQL mutation to remove game from favorites
const REMOVE_FAVORITE_MUTATION: &str = r#"
mutation RemoveFavoriteApp($appId: String!, $locale: String!) {
    removeFavoriteApp(appId: $appId, locale: $locale) {
        appId
    }
}
"#;

/// Default VPC ID for general access (from GFN config)
const DEFAULT_VPC_ID: &str = "GFN-PC";
/// Default locale
const DEFAULT_LOCALE: &str = "en_US";

/// GraphQL response wrapper
#[derive(Debug, Deserialize)]
struct GraphQLResponse<T> {
    data: Option<T>,
    errors: Option<Vec<GraphQLError>>,
}

#[derive(Debug, Deserialize)]
struct GraphQLError {
    message: String,
}

/// Static JSON game entry from nvidiagrid.net
/// Format: { "id": 100932911, "title": "41 Hours", "sortName": "41_hours", "isFullyOptimized": false,
///           "steamUrl": "https://...", "store": "Steam", "publisher": "...", "genres": [...], "status": "AVAILABLE" }
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StaticGameEntry {
    id: i64,
    title: String,
    #[serde(default)]
    sort_name: String,
    #[serde(default)]
    is_fully_optimized: bool,
    #[serde(default)]
    steam_url: Option<String>,
    #[serde(default)]
    store: Option<String>,
    #[serde(default)]
    publisher: Option<String>,
    #[serde(default)]
    genres: Vec<String>,
    #[serde(default)]
    status: Option<String>,
}

/// Convert StaticGameEntry to Game struct
fn static_game_to_game(entry: StaticGameEntry) -> Game {
    let store_type = entry.store.as_deref()
        .map(StoreType::from)
        .unwrap_or(StoreType::Other("Unknown".to_string()));

    // Extract store ID from Steam URL if available
    let store_id = entry.steam_url.as_ref()
        .and_then(|url| url.split('/').last())
        .map(|s| s.to_string())
        .unwrap_or_else(|| entry.id.to_string());

    let status = match entry.status.as_deref() {
        Some("AVAILABLE") => GameStatus::Available,
        Some("MAINTENANCE") => GameStatus::Maintenance,
        _ => GameStatus::Unavailable,
    };

    let game_id_str = entry.id.to_string();

    // NOTE: static.nvidiagrid.net returns 403 due to referrer policy
    // Don't generate URLs - let frontend use placeholders
    // Images should come from fetch_main_games or fetch_library instead

    Game {
        id: game_id_str,
        title: entry.title,
        publisher: entry.publisher,
        developer: None,
        genres: entry.genres,
        images: GameImages {
            box_art: None, // Let frontend use placeholder
            hero: None,
            thumbnail: None,
            screenshots: vec![],
        },
        store: StoreInfo {
            store_type,
            store_id,
            store_url: entry.steam_url,
        },
        status,
        supported_controls: vec!["keyboard".to_string(), "gamepad".to_string()],
        variants: vec![],
    }
}

/// Detailed app variant (for get_game_details)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppVariant {
    id: String,
    #[allow(dead_code)]
    short_name: Option<String>,
    app_store: String,
    supported_controls: Option<Vec<String>>,
}

/// GFN status info (for get_game_details)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GfnStatus {
    playability_state: Option<String>,
    #[allow(dead_code)]
    minimum_membership_tier_label: Option<String>,
}


/// Fetch games from the public static JSON API
/// This endpoint is publicly available and returns all GFN-supported games
#[command]
pub async fn fetch_games(
    limit: Option<u32>,
    offset: Option<u32>,
    _access_token: Option<String>,
) -> Result<GamesResponse, String> {
    let client = Client::new();

    let response = client
        .get(STATIC_GAMES_URL)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch games: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("API request failed with status {}: {}", status, body));
    }

    let game_entries: Vec<StaticGameEntry> = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let total_available = game_entries.len() as u32;

    // Apply offset and limit
    let offset_val = offset.unwrap_or(0) as usize;
    let limit_val = limit.unwrap_or(50) as usize;

    let games: Vec<Game> = game_entries
        .into_iter()
        .skip(offset_val)
        .take(limit_val)
        .map(static_game_to_game)
        .collect();

    let page = offset_val / limit_val;

    Ok(GamesResponse {
        total_count: total_available,
        games,
        page: page as u32,
        page_size: limit_val as u32,
    })
}

/// Search games by title - filters from the full games list
/// The static JSON endpoint doesn't support search, so we fetch all and filter client-side
#[command]
pub async fn search_games(
    query: String,
    limit: Option<u32>,
    _access_token: Option<String>,
) -> Result<GamesResponse, String> {
    let client = Client::new();

    let response = client
        .get(STATIC_GAMES_URL)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch games: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("API request failed with status {}: {}", status, body));
    }

    let game_entries: Vec<StaticGameEntry> = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let query_lower = query.to_lowercase();
    let limit_val = limit.unwrap_or(20) as usize;

    let filtered: Vec<Game> = game_entries
        .into_iter()
        .filter(|g| g.title.to_lowercase().contains(&query_lower))
        .take(limit_val)
        .map(static_game_to_game)
        .collect();

    let total_count = filtered.len() as u32;

    Ok(GamesResponse {
        total_count,
        games: filtered,
        page: 0,
        page_size: limit_val as u32,
    })
}

/// Optimize image URL with webp format and size
/// GFN CDN supports: ;f=webp;w=272 format
fn optimize_image_url(url: &str, width: u32) -> String {
    if url.contains("img.nvidiagrid.net") {
        format!("{};f=webp;w={}", url, width)
    } else {
        url.to_string()
    }
}

/// Response structure for library GraphQL query
#[derive(Debug, Deserialize)]
struct LibraryPanelsData {
    panels: Vec<LibraryPanel>,
}

#[derive(Debug, Deserialize)]
struct LibraryPanel {
    #[allow(dead_code)]
    id: Option<String>,
    name: String,
    #[serde(default)]
    sections: Vec<LibrarySection>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LibrarySection {
    #[allow(dead_code)]
    id: Option<String>,
    #[allow(dead_code)]
    title: Option<String>,
    #[serde(default)]
    render_directives: Option<String>,
    #[serde(default)]
    see_more_info: Option<serde_json::Value>,
    #[serde(default)]
    items: Vec<LibraryItem>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "__typename")]
enum LibraryItem {
    GameItem { app: LibraryApp },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LibraryApp {
    id: String,
    title: String,
    #[serde(default)]
    images: Option<LibraryImages>,
    #[serde(default)]
    library: Option<serde_json::Value>,
    #[serde(default)]
    item_metadata: Option<serde_json::Value>,
    #[serde(default)]
    variants: Option<Vec<LibraryVariant>>,
    #[serde(default)]
    gfn: Option<LibraryGfnStatus>,
}

/// Image URLs from GraphQL - uses literal field names
#[derive(Debug, Deserialize)]
struct LibraryImages {
    #[serde(rename = "GAME_BOX_ART")]
    game_box_art: Option<String>,
    #[serde(rename = "TV_BANNER")]
    tv_banner: Option<String>,
    #[serde(rename = "HERO_IMAGE")]
    hero_image: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LibraryVariant {
    id: String,
    #[allow(dead_code)]
    #[serde(default)]
    short_name: Option<String>,
    app_store: String,
    #[serde(default)]
    supported_controls: Option<Vec<String>>,
    #[serde(default)]
    minimum_size_in_bytes: Option<i64>,
    #[serde(default)]
    gfn: Option<LibraryVariantGfn>,
}

#[derive(Debug, Deserialize)]
struct LibraryVariantGfn {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    library: Option<LibraryVariantLibrary>,
}

#[derive(Debug, Deserialize)]
struct LibraryVariantLibrary {
    #[allow(dead_code)]
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    selected: Option<bool>,
    #[serde(default)]
    installed: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LibraryGfnStatus {
    #[serde(default)]
    playability_state: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    play_type: Option<String>,
    #[serde(default)]
    minimum_membership_tier_label: Option<String>,
    #[serde(default)]
    catalog_sku_strings: Option<serde_json::Value>,
}

/// Generate a random huId (hash ID) for requests
fn generate_hu_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:x}", timestamp)
}

/// Fetch panels using persisted query (GET request)
/// This is the correct way to fetch from GFN API - uses persisted queries
async fn fetch_panels_persisted(
    client: &Client,
    panel_names: Vec<&str>,
    vpc_id: &str,
    access_token: Option<&str>,
) -> Result<Vec<LibraryPanel>, String> {
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

    // URL encode the parameters
    let variables_str = serde_json::to_string(&variables)
        .map_err(|e| format!("Failed to serialize variables: {}", e))?;
    let extensions_str = serde_json::to_string(&extensions)
        .map_err(|e| format!("Failed to serialize extensions: {}", e))?;

    // Generate huId for this request
    let hu_id = generate_hu_id();

    // Use games.geforce.com endpoint with all required params
    let url = format!(
        "{}?requestType={}&extensions={}&huId={}&variables={}",
        GRAPHQL_URL,
        urlencoding::encode(request_type),
        urlencoding::encode(&extensions_str),
        urlencoding::encode(&hu_id),
        urlencoding::encode(&variables_str)
    );

    log::info!("Fetching panels from: {}", url);

    let mut request = client
        .get(&url)
        .header("Accept", "application/json, text/plain, */*")
        .header("Content-Type", "application/graphql")
        .header("Origin", "https://play.geforcenow.com")
        .header("Referer", "https://play.geforcenow.com/")
        // GFN client headers
        .header("nv-client-id", LCARS_CLIENT_ID)
        .header("nv-client-type", "NATIVE")
        .header("nv-client-version", GFN_CLIENT_VERSION)
        .header("nv-client-streamer", "NVIDIA-CLASSIC")
        .header("nv-device-os", "WINDOWS")
        .header("nv-device-type", "DESKTOP")
        .header("nv-device-make", "UNKNOWN")
        .header("nv-device-model", "UNKNOWN")
        .header("nv-browser-type", "CHROME");

    if let Some(token) = access_token {
        request = request.header("Authorization", format!("GFNJWT {}", token));
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("Failed to fetch panels: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        log::error!("Panels API failed: {} - {}", status, body);
        return Err(format!("API request failed with status {}: {}", status, body));
    }

    let body_text = response.text().await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    log::debug!("Panels response: {}", &body_text[..body_text.len().min(500)]);

    let graphql_response: GraphQLResponse<LibraryPanelsData> = serde_json::from_str(&body_text)
        .map_err(|e| format!("Failed to parse response: {} - body: {}", e, &body_text[..body_text.len().min(200)]))?;

    if let Some(errors) = graphql_response.errors {
        let error_msg = errors.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join(", ");
        return Err(format!("GraphQL errors: {}", error_msg));
    }

    Ok(graphql_response.data
        .map(|d| d.panels)
        .unwrap_or_default())
}

/// Convert LibraryApp to Game struct
fn library_app_to_game(app: LibraryApp) -> Game {
    // Find selected variant (the one marked as selected, or first available)
    let selected_variant = app.variants.as_ref()
        .and_then(|vars| vars.iter().find(|v| {
            v.gfn.as_ref()
                .and_then(|g| g.library.as_ref())
                .and_then(|l| l.selected)
                .unwrap_or(false)
        }))
        .or_else(|| app.variants.as_ref().and_then(|v| v.first()));

    let store_type = selected_variant
        .map(|v| StoreType::from(v.app_store.as_str()))
        .unwrap_or(StoreType::Other("Unknown".to_string()));

    // Use variant ID as the game ID for launching (e.g., "102217611")
    // The app.id is a UUID which is not used for launching
    let variant_id = selected_variant
        .map(|v| v.id.clone())
        .unwrap_or_default();

    let supported_controls = selected_variant
        .and_then(|v| v.supported_controls.clone())
        .unwrap_or_default();

    // Collect all variants for store selection
    let variants: Vec<GameVariant> = app.variants.as_ref()
        .map(|vars| vars.iter().map(|v| GameVariant {
            id: v.id.clone(),
            store_type: StoreType::from(v.app_store.as_str()),
            supported_controls: v.supported_controls.clone().unwrap_or_default(),
        }).collect())
        .unwrap_or_default();

    // Optimize image URLs (272px width for cards, webp format)
    // Prefer GAME_BOX_ART over TV_BANNER for better quality box art
    let box_art = app.images.as_ref()
        .and_then(|i| i.game_box_art.as_ref().or(i.tv_banner.as_ref()))
        .map(|url| optimize_image_url(url, 272));

    let hero = app.images.as_ref()
        .and_then(|i| i.hero_image.as_ref())
        .map(|url| optimize_image_url(url, 1920));

    let status = match app.gfn.as_ref()
        .and_then(|g| g.playability_state.as_deref()) {
        Some("PLAYABLE") => GameStatus::Available,
        Some("MAINTENANCE") => GameStatus::Maintenance,
        _ => GameStatus::Unavailable,
    };

    Game {
        id: variant_id.clone(), // Use variant ID for launching games
        title: app.title,
        publisher: None,
        developer: None,
        genres: vec![],
        images: GameImages {
            box_art,
            hero,
            thumbnail: None,
            screenshots: vec![],
        },
        store: StoreInfo {
            store_type,
            store_id: variant_id, // Same as game ID
            store_url: None,
        },
        status,
        supported_controls,
        variants,
    }
}

/// Fetch user's game library using persisted query API
#[command]
pub async fn fetch_library(
    access_token: String,
    vpc_id: Option<String>,
) -> Result<GamesResponse, String> {
    let client = Client::new();
    let vpc = vpc_id.as_deref().unwrap_or("NP-AMS-07"); // Default to Amsterdam

    let panels = fetch_panels_persisted(
        &client,
        vec!["LIBRARY"],
        vpc,
        Some(&access_token),
    ).await?;

    // Extract games from LIBRARY panel
    let mut games = Vec::new();

    for panel in panels {
        if panel.name == "LIBRARY" {
            for section in panel.sections {
                for item in section.items {
                    if let LibraryItem::GameItem { app } = item {
                        games.push(library_app_to_game(app));
                    }
                }
            }
        }
    }

    let total_count = games.len() as u32;

    Ok(GamesResponse {
        total_count,
        games,
        page: 0,
        page_size: total_count,
    })
}

/// Fetch main panel games (featured, popular, etc.) using persisted query API
#[command]
pub async fn fetch_main_games(
    access_token: Option<String>,
    vpc_id: Option<String>,
) -> Result<GamesResponse, String> {
    let client = Client::new();
    let vpc = vpc_id.as_deref().unwrap_or("NP-AMS-07"); // Default to Amsterdam

    log::info!("fetch_main_games: Starting with vpc={}", vpc);

    let panels = match fetch_panels_persisted(
        &client,
        vec!["MAIN"],
        vpc,
        access_token.as_deref(),
    ).await {
        Ok(p) => {
            log::info!("fetch_main_games: Got {} panels", p.len());
            p
        }
        Err(e) => {
            log::error!("fetch_main_games: Failed to fetch panels: {}", e);
            return Err(e);
        }
    };

    // Extract games from all sections
    let mut games = Vec::new();

    for panel in panels {
        log::info!("fetch_main_games: Panel {} has {} sections", panel.name, panel.sections.len());
        for section in panel.sections {
            log::debug!("fetch_main_games: Section has {} items", section.items.len());
            for item in section.items {
                if let LibraryItem::GameItem { app } = item {
                    log::debug!("fetch_main_games: Found game: {} with images: {:?}", app.title, app.images);
                    games.push(library_app_to_game(app));
                }
            }
        }
    }

    let total_count = games.len() as u32;

    Ok(GamesResponse {
        total_count,
        games,
        page: 0,
        page_size: total_count,
    })
}

/// Response for GetAppDataQueryForAppId
#[derive(Debug, Deserialize)]
struct AppsData {
    apps: Vec<DetailedAppData>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DetailedAppData {
    id: String,
    title: String,
    short_description: Option<String>,
    long_description: Option<String>,
    publisher_name: Option<String>,
    developer_name: Option<String>,
    genres: Option<Vec<String>>,
    images: Option<DetailedAppImages>,
    variants: Option<Vec<AppVariant>>,
    gfn: Option<GfnStatus>,
    max_local_players: Option<i32>,
    max_online_players: Option<i32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
struct DetailedAppImages {
    game_box_art: Option<String>,
    hero_image: Option<String>,
    game_logo: Option<String>,
    screenshots: Option<Vec<String>>,
    key_art: Option<String>,
}

/// Get detailed information about a specific game
#[command]
pub async fn get_game_details(
    game_id: String,
    access_token: Option<String>,
) -> Result<Game, String> {
    let client = Client::new();

    let variables = serde_json::json!({
        "vpcId": DEFAULT_VPC_ID,
        "locale": DEFAULT_LOCALE,
        "appIds": [game_id],
    });

    let body = serde_json::json!({
        "query": GET_APP_DATA_QUERY,
        "variables": variables,
    });

    let mut request = client
        .post(GRAPHQL_URL)
        .header("Content-Type", "application/json")
        .header("X-Client-Id", LCARS_CLIENT_ID);

    if let Some(token) = &access_token {
        request = request.header("Authorization", format!("GFNJWT {}", token));
    }

    let response = request
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to get game details: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("API request failed with status {}: {}", status, body));
    }

    let graphql_response: GraphQLResponse<AppsData> = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    if let Some(errors) = graphql_response.errors {
        let error_msg = errors.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join(", ");
        return Err(format!("GraphQL errors: {}", error_msg));
    }

    let app = graphql_response.data
        .and_then(|d| d.apps.into_iter().next())
        .ok_or("Game not found")?;

    // Convert detailed app to Game
    let variant = app.variants.as_ref().and_then(|v| v.first());

    let store_type = variant
        .map(|v| StoreType::from(v.app_store.as_str()))
        .unwrap_or(StoreType::Other("Unknown".to_string()));

    let store_id = variant.map(|v| v.id.clone()).unwrap_or_default();

    let images = app.images.map(|img| GameImages {
        box_art: img.game_box_art,
        hero: img.hero_image.or(img.key_art),
        thumbnail: img.game_logo,
        screenshots: img.screenshots.unwrap_or_default(),
    }).unwrap_or(GameImages {
        box_art: None,
        hero: None,
        thumbnail: None,
        screenshots: vec![],
    });

    let status = match app.gfn.as_ref().and_then(|g| g.playability_state.as_deref()) {
        Some("PLAYABLE") => GameStatus::Available,
        Some("MAINTENANCE") => GameStatus::Maintenance,
        _ => GameStatus::Unavailable,
    };

    let supported_controls = variant
        .and_then(|v| v.supported_controls.clone())
        .unwrap_or_default();

    // Collect all variants for store selection
    let variants: Vec<GameVariant> = app.variants.as_ref()
        .map(|vars| vars.iter().map(|v| GameVariant {
            id: v.id.clone(),
            store_type: StoreType::from(v.app_store.as_str()),
            supported_controls: v.supported_controls.clone().unwrap_or_default(),
        }).collect())
        .unwrap_or_default();

    Ok(Game {
        id: app.id,
        title: app.title,
        publisher: app.publisher_name,
        developer: app.developer_name,
        genres: app.genres.unwrap_or_default(),
        images,
        store: StoreInfo {
            store_type,
            store_id,
            store_url: None,
        },
        status,
        supported_controls,
        variants,
    })
}

/// Known GFN server zones discovered from network test results
const SERVER_ZONES: &[(&str, &str, &str)] = &[
    // Europe
    ("eu-netherlands-north", "Netherlands North", "Europe"),
    ("eu-netherlands-south", "Netherlands South", "Europe"),
    ("eu-united-kingdom-1", "United Kingdom 1", "Europe"),
    ("eu-united-kingdom-2", "United Kingdom 2", "Europe"),
    ("eu-france-1", "France 1", "Europe"),
    ("eu-france-2", "France 2", "Europe"),
    ("eu-germany", "Germany", "Europe"),
    ("eu-sweden", "Sweden", "Europe"),
    ("eu-poland", "Poland", "Europe"),
    ("eu-bulgaria", "Bulgaria", "Europe"),
    // North America
    ("us-california-north", "California North", "North America"),
    ("us-california-south", "California South", "North America"),
    ("us-oregon", "Oregon", "North America"),
    ("us-arizona", "Arizona", "North America"),
    ("us-texas", "Texas", "North America"),
    ("us-florida", "Florida", "North America"),
    ("us-georgia", "Georgia", "North America"),
    ("us-illinois", "Illinois", "North America"),
    ("us-virginia", "Virginia", "North America"),
    ("us-new-jersey", "New Jersey", "North America"),
    // Canada
    ("ca-quebec", "Quebec", "Canada"),
    // Asia-Pacific
    ("ap-japan", "Japan", "Asia-Pacific"),
];

/// Test a single server's latency using TCP connect time (measures TCP handshake RTT)
async fn test_server_latency(
    client: &Client,
    zone_id: &str,
    name: &str,
    region: &str,
) -> Server {
    let hostname = format!("{}.cloudmatchbeta.nvidiagrid.net", zone_id);
    let server_url = format!("https://{}/v2/serverInfo", hostname);

    // Measure TCP connect time to port 443 (HTTPS)
    // This gives accurate network latency by timing the TCP handshake
    let tcp_ping = async {
        use tokio::net::TcpStream;
        use std::net::ToSocketAddrs;

        // Resolve hostname to IP first
        let addr = format!("{}:443", hostname);
        let socket_addr = tokio::task::spawn_blocking(move || {
            addr.to_socket_addrs().ok().and_then(|mut addrs| addrs.next())
        }).await.ok().flatten();

        if let Some(socket_addr) = socket_addr {
            // Measure TCP connect time (SYN -> SYN-ACK)
            let start = std::time::Instant::now();
            match tokio::time::timeout(
                std::time::Duration::from_secs(5),
                TcpStream::connect(socket_addr)
            ).await {
                Ok(Ok(_stream)) => {
                    let elapsed = start.elapsed().as_millis() as u32;
                    // TCP handshake is 1 RTT, so this is accurate latency
                    Some(elapsed)
                }
                _ => None,
            }
        } else {
            None
        }
    };

    // Run TCP ping and HTTP status check in parallel
    let (ping_ms, http_result) = tokio::join!(
        tcp_ping,
        client.get(&server_url).timeout(std::time::Duration::from_secs(5)).send()
    );

    let status = match http_result {
        Ok(response) if response.status().is_success() => ServerStatus::Online,
        Ok(_) => ServerStatus::Maintenance,
        Err(_) => {
            if ping_ms.is_some() {
                ServerStatus::Online
            } else {
                ServerStatus::Offline
            }
        }
    };

    Server {
        id: zone_id.to_string(),
        name: name.to_string(),
        region: region.to_string(),
        country: zone_id.split('-').nth(1).unwrap_or("Unknown").to_string(),
        ping_ms,
        queue_size: None,
        status,
    }
}

/// Get available servers with ping information
/// Uses CloudMatch API to get server status - runs tests in parallel for speed
#[command]
pub async fn get_servers(_access_token: Option<String>) -> Result<Vec<Server>, String> {
    let client = Client::new();

    // Test all servers in parallel for fast results
    let futures: Vec<_> = SERVER_ZONES
        .iter()
        .map(|(zone_id, name, region)| {
            let client = client.clone();
            async move {
                test_server_latency(&client, zone_id, name, region).await
            }
        })
        .collect();

    let mut servers: Vec<Server> = futures_util::future::join_all(futures).await;

    // Sort by ping (online servers first, then by ping)
    servers.sort_by(|a, b| {
        match (&a.status, &b.status) {
            (ServerStatus::Online, ServerStatus::Online) => {
                a.ping_ms.cmp(&b.ping_ms)
            }
            (ServerStatus::Online, _) => std::cmp::Ordering::Less,
            (_, ServerStatus::Online) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        }
    });

    Ok(servers)
}

/// Helper function to build image URLs
pub fn build_image_url(path: &str, width: Option<u32>, height: Option<u32>) -> String {
    let mut url = format!("{}/{}", IMAGE_CDN_BASE, path);

    if let (Some(w), Some(h)) = (width, height) {
        url = format!("{}?w={}&h={}", url, w, h);
    }

    url
}

/// Parse store type from string
impl From<&str> for StoreType {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "steam" => StoreType::Steam,
            "epic" | "epicgames" => StoreType::Epic,
            "ubisoft" | "uplay" => StoreType::Ubisoft,
            "origin" => StoreType::Origin,
            "gog" => StoreType::GoG,
            "xbox" => StoreType::Xbox,
            "ea_app" | "ea" => StoreType::EaApp,
            other => StoreType::Other(other.to_string()),
        }
    }
}

/// Resolution option from subscription features
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolutionOption {
    #[serde(rename = "heightInPixels", default)]
    pub height: u32,
    #[serde(rename = "widthInPixels", default)]
    pub width: u32,
    #[serde(rename = "framesPerSecond", default)]
    pub fps: u32,
    #[serde(rename = "isEntitled", default)]
    pub is_entitled: bool,
}

/// Feature key-value from subscription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureOption {
    #[serde(default)]
    pub key: Option<String>,
    #[serde(rename = "textValue", default)]
    pub text_value: Option<String>,
    #[serde(rename = "setValue", default)]
    pub set_value: Option<Vec<String>>,
    #[serde(rename = "booleanValue", default)]
    pub boolean_value: Option<bool>,
}

/// Subscription features containing resolutions and feature flags
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionFeatures {
    #[serde(default)]
    pub resolutions: Vec<ResolutionOption>,
    #[serde(default)]
    pub features: Vec<FeatureOption>,
}

/// Streaming quality profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingQualityProfile {
    #[serde(rename = "clientStreamingQualityMode", default)]
    pub mode: Option<String>,
    #[serde(rename = "maxBitRate", default)]
    pub max_bitrate: Option<BitrateConfig>,
    #[serde(default)]
    pub resolution: Option<ResolutionConfig>,
    #[serde(default)]
    pub features: Vec<FeatureOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitrateConfig {
    #[serde(rename = "bitrateOption", default)]
    pub bitrate_option: bool,
    #[serde(rename = "bitrateValue", default)]
    pub bitrate_value: u32,
    #[serde(rename = "minBitrateValue", default)]
    pub min_bitrate_value: u32,
    #[serde(rename = "maxBitrateValue", default)]
    pub max_bitrate_value: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolutionConfig {
    #[serde(rename = "heightInPixels", default)]
    pub height: u32,
    #[serde(rename = "widthInPixels", default)]
    pub width: u32,
    #[serde(rename = "framesPerSecond", default)]
    pub fps: u32,
}

/// Storage addon attribute
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddonAttribute {
    #[serde(default)]
    pub key: Option<String>,
    #[serde(rename = "textValue", default)]
    pub text_value: Option<String>,
}

/// Subscription addon (e.g., permanent storage)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionAddon {
    #[serde(default)]
    pub uri: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(rename = "type", default)]
    pub addon_type: Option<String>,
    #[serde(rename = "subType", default)]
    pub sub_type: Option<String>,
    #[serde(rename = "autoPayEnabled", default)]
    pub auto_pay_enabled: Option<bool>,
    #[serde(default)]
    pub attributes: Vec<AddonAttribute>,
    #[serde(default)]
    pub status: Option<String>,
}

/// Subscription info response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionInfo {
    #[serde(rename = "membershipTier", default = "default_membership_tier")]
    pub membership_tier: String,
    #[serde(rename = "remainingTimeInMinutes")]
    pub remaining_time_minutes: Option<i32>,
    #[serde(rename = "totalTimeInMinutes")]
    pub total_time_minutes: Option<i32>,
    #[serde(rename = "renewalDateTime")]
    pub renewal_date: Option<String>,
    #[serde(rename = "type")]
    pub subscription_type: Option<String>,
    #[serde(rename = "subType")]
    pub sub_type: Option<String>,
    /// Subscription features including resolutions and feature flags
    #[serde(default)]
    pub features: Option<SubscriptionFeatures>,
    /// Streaming quality profiles (BALANCED, DATA_SAVER, COMPETITIVE, CINEMATIC)
    #[serde(rename = "streamingQualities", default)]
    pub streaming_qualities: Vec<StreamingQualityProfile>,
    /// Subscription addons (e.g., permanent storage)
    #[serde(default)]
    pub addons: Vec<SubscriptionAddon>,
}

fn default_membership_tier() -> String {
    "FREE".to_string()
}

/// Fetch subscription/membership info from MES API
#[command]
pub async fn fetch_subscription(
    access_token: String,
    user_id: String,
    vpc_id: Option<String>,
) -> Result<SubscriptionInfo, String> {
    let client = Client::builder()
        .user_agent(GFN_CEF_USER_AGENT)
        .gzip(true)
        .deflate(true)
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let vpc = vpc_id.as_deref().unwrap_or("NP-AMS-05");

    let url = format!(
        "{}?serviceName=gfn_pc&languageCode=en_US&vpcId={}&userId={}",
        MES_URL, vpc, user_id
    );

    log::info!("Fetching subscription from: {}", url);

    let response = client
        .get(&url)
        .header("Authorization", format!("GFNJWT {}", access_token))
        .header("Accept", "application/json, text/plain, */*")
        .header("Accept-Encoding", "gzip, deflate")
        .header("nv-client-id", LCARS_CLIENT_ID)
        .header("nv-client-type", "NATIVE")
        .header("nv-client-version", GFN_CLIENT_VERSION)
        .header("nv-client-streamer", "NVIDIA-CLASSIC")
        .header("nv-device-os", "WINDOWS")
        .header("nv-device-type", "DESKTOP")
        .header("nv-device-make", "UNKNOWN")
        .header("nv-device-model", "UNKNOWN")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch subscription: {}", e))?;

    let status = response.status();
    let content_type = response.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    log::debug!("Subscription response status: {}, content-type: {}", status, content_type);

    let body = response.text().await
        .map_err(|e| format!("Failed to read subscription response body: {}", e))?;

    if !status.is_success() {
        log::error!("Subscription API failed with status {}: {}", status, body);
        return Err(format!("Subscription API failed with status {}: {}", status, body));
    }

    // Log raw response for debugging
    log::debug!("Subscription raw response: {}", body);

    let subscription: SubscriptionInfo = serde_json::from_str(&body)
        .map_err(|e| {
            log::error!("Failed to parse subscription response: {}. Raw response: {}", e, body);
            format!("Failed to parse subscription response: {}. Check logs for raw response.", e)
        })?;

    log::info!("Subscription tier: {}, type: {:?}, subType: {:?}",
        subscription.membership_tier,
        subscription.subscription_type,
        subscription.sub_type
    );

    Ok(subscription)
}

/// Search result item from GraphQL
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchResultItem {
    id: String,
    title: String,
    #[serde(default)]
    images: Option<SearchImages>,
    #[serde(default)]
    variants: Option<Vec<SearchVariant>>,
    #[serde(default)]
    gfn: Option<SearchGfnStatus>,
}

#[derive(Debug, Deserialize)]
struct SearchImages {
    #[serde(rename = "GAME_BOX_ART")]
    game_box_art: Option<String>,
    #[serde(rename = "TV_BANNER")]
    tv_banner: Option<String>,
    #[serde(rename = "HERO_IMAGE")]
    hero_image: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchVariant {
    id: String,
    app_store: String,
    #[serde(default)]
    supported_controls: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchGfnStatus {
    #[serde(default)]
    playability_state: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SearchData {
    apps: AppsSearchResults,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppsSearchResults {
    #[serde(default)]
    items: Vec<SearchResultItem>,
    #[serde(default)]
    number_returned: i32,
    #[serde(default)]
    number_supported: i32,
    page_info: Option<PageInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
    total_count: i32,
}

/// Search games using GraphQL persisted query
#[command]
pub async fn search_games_graphql(
    query: String,
    limit: Option<u32>,
    access_token: Option<String>,
    vpc_id: Option<String>,
) -> Result<GamesResponse, String> {
    let client = Client::new();
    let vpc = vpc_id.as_deref().unwrap_or("NP-AMS-07");
    let fetch_count = limit.unwrap_or(20) as i32;

    let variables = serde_json::json!({
        "searchString": query,
        "vpcId": vpc,
        "locale": DEFAULT_LOCALE,
        "fetchCount": fetch_count,
        "sortString": "itemMetadata.relevance:DESC,sortName:ASC",
        "cursor": "",
        "filters": {}
    });

    let extensions = serde_json::json!({
        "persistedQuery": {
            "sha256Hash": SEARCH_QUERY_HASH
        }
    });

    let variables_str = serde_json::to_string(&variables)
        .map_err(|e| format!("Failed to serialize variables: {}", e))?;
    let extensions_str = serde_json::to_string(&extensions)
        .map_err(|e| format!("Failed to serialize extensions: {}", e))?;

    let hu_id = generate_hu_id();

    let url = format!(
        "{}?requestType=apps&extensions={}&huId={}&variables={}",
        GRAPHQL_URL,
        urlencoding::encode(&extensions_str),
        urlencoding::encode(&hu_id),
        urlencoding::encode(&variables_str)
    );

    log::info!("Searching games: {}", query);

    let mut request = client
        .get(&url)
        .header("Accept", "application/json, text/plain, */*")
        .header("Content-Type", "application/graphql")
        .header("Origin", "https://play.geforcenow.com")
        .header("Referer", "https://play.geforcenow.com/")
        .header("nv-client-id", LCARS_CLIENT_ID)
        .header("nv-client-type", "NATIVE")
        .header("nv-client-version", GFN_CLIENT_VERSION)
        .header("nv-client-streamer", "NVIDIA-CLASSIC")
        .header("nv-device-os", "WINDOWS")
        .header("nv-device-type", "DESKTOP")
        .header("nv-device-make", "UNKNOWN")
        .header("nv-device-model", "UNKNOWN")
        .header("nv-browser-type", "CHROME");

    if let Some(token) = access_token {
        request = request.header("Authorization", format!("GFNJWT {}", token));
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("Failed to search games: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        log::error!("Search API failed: {} - {}", status, body);
        return Err(format!("API request failed with status {}: {}", status, body));
    }

    let body_text = response.text().await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    let graphql_response: GraphQLResponse<SearchData> = serde_json::from_str(&body_text)
        .map_err(|e| format!("Failed to parse search response: {}", e))?;

    if let Some(errors) = graphql_response.errors {
        let error_msg = errors.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join(", ");
        return Err(format!("GraphQL errors: {}", error_msg));
    }

    let search_results = graphql_response.data
        .map(|d| d.apps)
        .ok_or("No search results")?;

    // Get total count before consuming items
    let total_count = search_results.page_info
        .as_ref()
        .map(|p| p.total_count as u32)
        .unwrap_or(search_results.number_supported as u32);

    // Convert search results to Game structs
    let games: Vec<Game> = search_results.items.into_iter().map(|item| {
        let variant = item.variants.as_ref().and_then(|v| v.first());

        let store_type = variant
            .map(|v| StoreType::from(v.app_store.as_str()))
            .unwrap_or(StoreType::Other("Unknown".to_string()));

        let variant_id = variant.map(|v| v.id.clone()).unwrap_or_default();

        let supported_controls = variant
            .and_then(|v| v.supported_controls.clone())
            .unwrap_or_default();

        // Collect all variants for store selection
        let variants: Vec<GameVariant> = item.variants.as_ref()
            .map(|vars| vars.iter().map(|v| GameVariant {
                id: v.id.clone(),
                store_type: StoreType::from(v.app_store.as_str()),
                supported_controls: v.supported_controls.clone().unwrap_or_default(),
            }).collect())
            .unwrap_or_default();

        // Prefer GAME_BOX_ART over TV_BANNER for better quality box art
        let box_art = item.images.as_ref()
            .and_then(|i| i.game_box_art.as_ref().or(i.tv_banner.as_ref()))
            .map(|url| optimize_image_url(url, 272));

        let hero = item.images.as_ref()
            .and_then(|i| i.hero_image.as_ref())
            .map(|url| optimize_image_url(url, 1920));

        let status = match item.gfn.as_ref()
            .and_then(|g| g.playability_state.as_deref()) {
            Some("PLAYABLE") => GameStatus::Available,
            Some("MAINTENANCE") => GameStatus::Maintenance,
            _ => GameStatus::Unavailable,
        };

        Game {
            id: variant_id.clone(),
            title: item.title,
            publisher: None,
            developer: None,
            genres: vec![],
            images: GameImages {
                box_art,
                hero,
                thumbnail: None,
                screenshots: vec![],
            },
            store: StoreInfo {
                store_type,
                store_id: variant_id,
                store_url: None,
            },
            status,
            supported_controls,
            variants,
        }
    }).collect();

    Ok(GamesResponse {
        total_count,
        games,
        page: 0,
        page_size: fetch_count as u32,
    })
}
