//! Application State Management
//!
//! Central state machine for the OpenNow Streamer.

pub mod config;
pub mod session;

pub use config::{Settings, VideoCodec, AudioCodec, StreamQuality, StatsPosition};
pub use session::{SessionInfo, SessionState};

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use parking_lot::{Mutex, RwLock};
use tokio::runtime::Handle;
use tokio::sync::mpsc;
use log::{info, error, warn};

use crate::auth::{self, LoginProvider, AuthTokens, UserInfo, PkceChallenge};
use crate::api::{self, GfnApiClient, DynamicServerRegion};

use crate::input::InputHandler;

use crate::media::{VideoFrame, StreamStats};
use crate::webrtc::StreamingSession;

/// Cache for dynamic regions fetched from serverInfo API
static DYNAMIC_REGIONS_CACHE: RwLock<Option<Vec<DynamicServerRegion>>> = RwLock::new(None);

/// Shared frame holder for zero-latency frame delivery
/// Decoder writes latest frame, renderer reads it - no buffering
pub struct SharedFrame {
    frame: Mutex<Option<VideoFrame>>,
    frame_count: AtomicU64,
    last_read_count: AtomicU64,
}

impl SharedFrame {
    pub fn new() -> Self {
        Self {
            frame: Mutex::new(None),
            frame_count: AtomicU64::new(0),
            last_read_count: AtomicU64::new(0),
        }
    }

    /// Write a new frame (called by decoder)
    pub fn write(&self, frame: VideoFrame) {
        *self.frame.lock() = Some(frame);
        self.frame_count.fetch_add(1, Ordering::Release);
    }

    /// Check if there's a new frame since last read
    pub fn has_new_frame(&self) -> bool {
        let current = self.frame_count.load(Ordering::Acquire);
        let last = self.last_read_count.load(Ordering::Acquire);
        current > last
    }

    /// Read the latest frame (called by renderer)
    /// Returns None if no frame available or no new frame since last read
    /// Uses take() instead of clone() to avoid copying ~3MB per frame
    pub fn read(&self) -> Option<VideoFrame> {
        let current = self.frame_count.load(Ordering::Acquire);
        let last = self.last_read_count.load(Ordering::Acquire);

        if current > last {
            self.last_read_count.store(current, Ordering::Release);
            self.frame.lock().take()  // Move instead of clone - zero copy
        } else {
            None
        }
    }

    /// Get frame count for stats
    pub fn frame_count(&self) -> u64 {
        self.frame_count.load(Ordering::Relaxed)
    }
}

impl Default for SharedFrame {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse resolution string (e.g., "1920x1080") into (width, height)
/// Returns (1920, 1080) as default if parsing fails
pub fn parse_resolution(res: &str) -> (u32, u32) {
    let parts: Vec<&str> = res.split('x').collect();
    if parts.len() == 2 {
        let width = parts[0].parse().unwrap_or(1920);
        let height = parts[1].parse().unwrap_or(1080);
        (width, height)
    } else {
        (1920, 1080) // Default to 1080p
    }
}

/// UI actions that can be triggered from the renderer
#[derive(Debug, Clone)]
pub enum UiAction {
    /// Start OAuth login flow
    StartLogin,
    /// Select a login provider
    SelectProvider(usize),
    /// Logout
    Logout,
    /// Launch a game by index
    LaunchGame(usize),
    /// Launch a specific game
    LaunchGameDirect(GameInfo),
    /// Stop streaming
    StopStreaming,
    /// Toggle stats overlay
    ToggleStats,
    /// Update search query
    UpdateSearch(String),
    /// Toggle settings panel
    ToggleSettings,
    /// Update a setting
    UpdateSetting(SettingChange),
    /// Refresh games list
    RefreshGames,
    /// Switch to a tab
    SwitchTab(GamesTab),
    /// Open game detail popup
    OpenGamePopup(GameInfo),
    /// Close game detail popup
    CloseGamePopup,
    /// Select a server/region
    SelectServer(usize),
    /// Enable auto server selection (best ping)
    SetAutoServerSelection(bool),
    /// Start ping test for all servers
    StartPingTest,
    /// Toggle settings modal
    ToggleSettingsModal,
}

/// Setting changes
#[derive(Debug, Clone)]
pub enum SettingChange {
    Resolution(String),
    Fps(u32),
    Codec(VideoCodec),
    MaxBitrate(u32),
    Fullscreen(bool),
    VSync(bool),
    LowLatency(bool),
}

/// Application state enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    /// Login screen
    Login,
    /// Browsing games library
    Games,
    /// Session being set up (queue, launching)
    Session,
    /// Active streaming
    Streaming,
}

/// Main application structure
pub struct App {
    /// Current application state
    pub state: AppState,

    /// Tokio runtime handle for async operations
    pub runtime: Handle,

    /// User settings
    pub settings: Settings,

    /// Authentication tokens
    pub auth_tokens: Option<AuthTokens>,

    /// User info
    pub user_info: Option<UserInfo>,

    /// Current session info
    pub session: Option<SessionInfo>,

    /// Streaming session (WebRTC)
    pub streaming_session: Option<Arc<Mutex<StreamingSession>>>,

    /// Input handler for the current platform
    pub input_handler: Option<Arc<InputHandler>>,

    /// Whether cursor is captured
    pub cursor_captured: bool,

    /// Current video frame (for rendering)
    pub current_frame: Option<VideoFrame>,

    /// Shared frame holder for zero-latency frame delivery
    pub shared_frame: Option<Arc<SharedFrame>>,

    /// Stream statistics
    pub stats: StreamStats,

    /// Whether to show stats overlay
    pub show_stats: bool,

    /// Status message for UI
    pub status_message: String,

    /// Error message (if any)
    pub error_message: Option<String>,

    /// Games list
    pub games: Vec<GameInfo>,

    /// Search query
    pub search_query: String,

    /// Selected game
    pub selected_game: Option<GameInfo>,

    /// Channel for receiving stats updates
    stats_rx: Option<mpsc::Receiver<StreamStats>>,

    // === Login State ===
    /// Available login providers
    pub login_providers: Vec<LoginProvider>,

    /// Selected provider index
    pub selected_provider_index: usize,

    /// Whether settings panel is visible
    pub show_settings: bool,

    /// Loading state for async operations
    pub is_loading: bool,

    /// VPC ID for current provider
    pub vpc_id: Option<String>,

    /// API client
    api_client: GfnApiClient,

    /// Subscription info (hours, storage, etc.)
    pub subscription: Option<SubscriptionInfo>,

    /// User's library games
    pub library_games: Vec<GameInfo>,

    /// Current tab in Games view
    pub current_tab: GamesTab,

    /// Selected game for detail popup (None = popup closed)
    pub selected_game_popup: Option<GameInfo>,

    /// Available servers/regions
    pub servers: Vec<ServerInfo>,

    /// Selected server index
    pub selected_server_index: usize,

    /// Auto server selection (picks best ping)
    pub auto_server_selection: bool,

    /// Whether ping test is running
    pub ping_testing: bool,

    /// Whether settings modal is visible
    pub show_settings_modal: bool,

    /// Last time we polled the session (for rate limiting)
    last_poll_time: std::time::Instant,

    /// Render FPS tracking
    render_frame_count: u64,
    last_render_fps_time: std::time::Instant,
    last_render_frame_count: u64,
}

/// Poll interval for session status (2 seconds)
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/// Game information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GameInfo {
    pub id: String,
    pub title: String,
    pub publisher: Option<String>,
    pub image_url: Option<String>,
    pub store: String,
    pub app_id: Option<i64>,
}

/// Subscription information
#[derive(Debug, Clone, Default)]
pub struct SubscriptionInfo {
    pub membership_tier: String,
    pub remaining_hours: f32,
    pub total_hours: f32,
    pub has_persistent_storage: bool,
    pub storage_size_gb: Option<u32>,
}

/// Current tab in Games view
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GamesTab {
    AllGames,
    MyLibrary,
}

impl Default for GamesTab {
    fn default() -> Self {
        GamesTab::AllGames
    }
}

/// Server/Region information
#[derive(Debug, Clone)]
pub struct ServerInfo {
    pub id: String,
    pub name: String,
    pub region: String,
    pub url: Option<String>,
    pub ping_ms: Option<u32>,
    pub status: ServerStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerStatus {
    Online,
    Testing,
    Offline,
    Unknown,
}

impl App {
    /// Create new application instance
    pub fn new(runtime: Handle) -> Self {
        // Load settings
        let settings = Settings::load().unwrap_or_default();
        let auto_server = settings.auto_server_selection; // Save before move

        // Try to load saved tokens
        let auth_tokens = Self::load_tokens();
        let has_token = auth_tokens.as_ref().map(|t| !t.is_expired()).unwrap_or(false);

        let initial_state = if has_token {
            AppState::Games
        } else {
            AppState::Login
        };

        // Start fetching login providers
        let rt = runtime.clone();
        rt.spawn(async {
            if let Err(e) = auth::fetch_login_providers().await {
                warn!("Failed to fetch login providers: {}", e);
            }
        });

        Self {
            state: initial_state,
            runtime,
            settings,
            auth_tokens,
            user_info: None,
            session: None,
            streaming_session: None,
            input_handler: None,
            cursor_captured: false,
            current_frame: None,
            shared_frame: None,
            stats: StreamStats::default(),
            show_stats: true,
            status_message: "Welcome to OpenNOW".to_string(),
            error_message: None,
            games: Vec::new(),
            search_query: String::new(),
            selected_game: None,
            stats_rx: None,
            login_providers: vec![LoginProvider::nvidia_default()],
            selected_provider_index: 0,
            show_settings: false,
            is_loading: false,
            vpc_id: None,
            api_client: GfnApiClient::new(),
            subscription: None,
            library_games: Vec::new(),
            current_tab: GamesTab::AllGames,
            selected_game_popup: None,
            servers: Vec::new(),
            selected_server_index: 0,
            auto_server_selection: auto_server, // Load from settings
            ping_testing: false,
            show_settings_modal: false,
            last_poll_time: std::time::Instant::now(),
            render_frame_count: 0,
            last_render_fps_time: std::time::Instant::now(),
            last_render_frame_count: 0,
        }
    }

    /// Handle a UI action
    pub fn handle_action(&mut self, action: UiAction) {
        match action {
            UiAction::StartLogin => {
                self.start_oauth_login();
            }
            UiAction::SelectProvider(index) => {
                self.select_provider(index);
            }
            UiAction::Logout => {
                self.logout();
            }
            UiAction::LaunchGame(index) => {
                // Get game from appropriate list based on current tab
                let game = match self.current_tab {
                    GamesTab::AllGames => self.games.get(index).cloned(),
                    GamesTab::MyLibrary => self.library_games.get(index).cloned(),
                };
                if let Some(game) = game {
                    self.launch_game(&game);
                }
            }
            UiAction::LaunchGameDirect(game) => {
                self.launch_game(&game);
            }
            UiAction::StopStreaming => {
                self.stop_streaming();
            }
            UiAction::ToggleStats => {
                self.toggle_stats();
            }
            UiAction::UpdateSearch(query) => {
                self.search_query = query;
            }
            UiAction::ToggleSettings => {
                self.show_settings = !self.show_settings;
            }
            UiAction::UpdateSetting(change) => {
                match change {
                    SettingChange::Resolution(res) => self.settings.resolution = res,
                    SettingChange::Fps(fps) => self.settings.fps = fps,
                    SettingChange::Codec(codec) => self.settings.codec = codec,
                    SettingChange::MaxBitrate(bitrate) => self.settings.max_bitrate_mbps = bitrate,
                    SettingChange::Fullscreen(fs) => self.settings.fullscreen = fs,
                    SettingChange::VSync(vsync) => self.settings.vsync = vsync,
                    SettingChange::LowLatency(ll) => self.settings.low_latency_mode = ll,
                }
                self.save_settings();
            }
            UiAction::RefreshGames => {
                self.fetch_games();
            }
            UiAction::SwitchTab(tab) => {
                self.current_tab = tab;
                // Fetch library if switching to My Library and it's empty
                if tab == GamesTab::MyLibrary && self.library_games.is_empty() {
                    self.fetch_library();
                }
            }
            UiAction::OpenGamePopup(game) => {
                self.selected_game_popup = Some(game);
            }
            UiAction::CloseGamePopup => {
                self.selected_game_popup = None;
            }
            UiAction::SelectServer(index) => {
                if index < self.servers.len() {
                    self.selected_server_index = index;
                    self.auto_server_selection = false; // Disable auto when manually selecting
                    // Save selected server and auto mode to settings
                    self.settings.selected_server = Some(self.servers[index].id.clone());
                    self.settings.auto_server_selection = false;
                    self.save_settings();
                    info!("Selected server: {}", self.servers[index].name);
                }
            }
            UiAction::SetAutoServerSelection(enabled) => {
                self.auto_server_selection = enabled;
                self.settings.auto_server_selection = enabled;
                self.save_settings();
                if enabled {
                    // Auto-select best server based on ping
                    self.select_best_server();
                }
            }
            UiAction::StartPingTest => {
                self.start_ping_test();
            }
            UiAction::ToggleSettingsModal => {
                self.show_settings_modal = !self.show_settings_modal;
                // Load servers when opening settings if not loaded
                if self.show_settings_modal && self.servers.is_empty() {
                    self.load_servers();
                }
            }
        }
    }

    /// Get filtered games based on search query
    pub fn filtered_games(&self) -> Vec<(usize, &GameInfo)> {
        let query = self.search_query.to_lowercase();
        self.games
            .iter()
            .enumerate()
            .filter(|(_, game)| {
                query.is_empty() || game.title.to_lowercase().contains(&query)
            })
            .collect()
    }

    /// Select a login provider
    pub fn select_provider(&mut self, index: usize) {
        // Update cached providers from global state
        self.login_providers = auth::get_cached_providers();
        if self.login_providers.is_empty() {
            self.login_providers = vec![LoginProvider::nvidia_default()];
        }

        if index < self.login_providers.len() {
            self.selected_provider_index = index;
            let provider = self.login_providers[index].clone();
            auth::set_login_provider(provider.clone());
            info!("Selected provider: {}", provider.login_provider_display_name);
        }
    }

    /// Start OAuth login flow
    pub fn start_oauth_login(&mut self) {
        // Find available port
        let port = match auth::find_available_port() {
            Some(p) => p,
            None => {
                self.error_message = Some("No available ports for OAuth callback".to_string());
                return;
            }
        };

        self.is_loading = true;
        self.status_message = "Opening browser for login...".to_string();

        let pkce = PkceChallenge::new();
        let auth_url = auth::build_auth_url(&pkce, port);
        let verifier = pkce.verifier.clone();

        // Open browser
        if let Err(e) = open::that(&auth_url) {
            self.error_message = Some(format!("Failed to open browser: {}", e));
            self.is_loading = false;
            return;
        }

        info!("Opened browser for OAuth login");

        // Spawn task to wait for callback
        let runtime = self.runtime.clone();
        runtime.spawn(async move {
            match auth::start_callback_server(port).await {
                Ok(code) => {
                    info!("Received OAuth code");
                    match auth::exchange_code(&code, &verifier, port).await {
                        Ok(tokens) => {
                            info!("Token exchange successful!");
                            // Tokens will be picked up in update()
                            // For now, we store them in a temp file
                            Self::save_tokens(&tokens);
                        }
                        Err(e) => {
                            error!("Token exchange failed: {}", e);
                        }
                    }
                }
                Err(e) => {
                    error!("OAuth callback failed: {}", e);
                }
            }
        });
    }

    /// Update application state (called each frame)
    pub fn update(&mut self) {
        // Track render FPS
        self.render_frame_count += 1;
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(self.last_render_fps_time).as_secs_f64();
        if elapsed >= 1.0 {
            let frames_this_period = self.render_frame_count - self.last_render_frame_count;
            self.stats.render_fps = (frames_this_period as f64 / elapsed) as f32;
            self.stats.frames_rendered = self.render_frame_count;
            self.last_render_frame_count = self.render_frame_count;
            self.last_render_fps_time = now;
        }

        // Check for new video frames from shared frame holder
        if let Some(ref shared) = self.shared_frame {
            if let Some(frame) = shared.read() {
                // Only log the first frame (when current_frame is None)
                if self.current_frame.is_none() {
                    log::info!("First video frame received: {}x{}", frame.width, frame.height);
                }
                self.current_frame = Some(frame);
            }
        }

        // Check for stats updates
        if let Some(ref mut rx) = self.stats_rx {
            while let Ok(mut stats) = rx.try_recv() {
                // Preserve render_fps from our local tracking
                stats.render_fps = self.stats.render_fps;
                stats.frames_rendered = self.stats.frames_rendered;
                self.stats = stats;
            }
        }

        // Update cached providers
        let cached = auth::get_cached_providers();
        if !cached.is_empty() && cached.len() != self.login_providers.len() {
            self.login_providers = cached;
        }

        // Check if tokens were saved by OAuth callback
        if self.state == AppState::Login && self.is_loading {
            if let Some(tokens) = Self::load_tokens() {
                if !tokens.is_expired() {
                    info!("OAuth login successful!");
                    self.auth_tokens = Some(tokens.clone());
                    self.api_client.set_access_token(tokens.jwt().to_string());
                    self.is_loading = false;
                    self.state = AppState::Games;
                    self.status_message = "Login successful!".to_string();
                    self.fetch_games();
                    self.fetch_subscription(); // Also fetch subscription info
                    self.load_servers(); // Load servers (fetches dynamic regions)
                }
            }
        }

        // Check if games were fetched and saved to cache
        if self.state == AppState::Games && self.is_loading && self.games.is_empty() {
            if let Some(games) = Self::load_games_cache() {
                if !games.is_empty() {
                    // Check if cache has images - if not, it's old cache that needs refresh
                    let has_images = games.iter().any(|g| g.image_url.is_some());
                    if has_images {
                        info!("Loaded {} games from cache (with images)", games.len());
                        self.games = games;
                        self.is_loading = false;
                        self.status_message = format!("Loaded {} games", self.games.len());
                    } else {
                        info!("Cache has {} games but no images - forcing refresh", games.len());
                        Self::clear_games_cache();
                        self.fetch_games();
                    }
                }
            }
        }

        // Check if library was fetched and saved to cache
        if self.state == AppState::Games && self.current_tab == GamesTab::MyLibrary && self.library_games.is_empty() {
            if let Some(games) = Self::load_library_cache() {
                if !games.is_empty() {
                    info!("Loaded {} games from library cache", games.len());
                    self.library_games = games;
                    self.is_loading = false;
                    self.status_message = format!("Your Library: {} games", self.library_games.len());
                }
            }
        }

        // Check if subscription was fetched and saved to cache
        if self.state == AppState::Games && self.subscription.is_none() {
            if let Some(sub) = Self::load_subscription_cache() {
                info!("Loaded subscription from cache: {}", sub.membership_tier);
                self.subscription = Some(sub);
            }
        }

        // Check for dynamic regions from serverInfo API
        self.check_dynamic_regions();

        // Check for ping test results
        if self.ping_testing {
            self.load_ping_results();
        }

        // Poll session status when in session state
        if self.state == AppState::Session && self.is_loading {
            self.poll_session_status();
        }
    }

    /// Logout and return to login screen
    pub fn logout(&mut self) {
        self.auth_tokens = None;
        self.user_info = None;
        auth::clear_login_provider();
        Self::clear_tokens();
        self.state = AppState::Login;
        self.games.clear();
        self.status_message = "Logged out".to_string();
    }

    /// Fetch games library
    pub fn fetch_games(&mut self) {
        if self.auth_tokens.is_none() {
            return;
        }

        self.is_loading = true;
        self.status_message = "Loading games...".to_string();

        let token = self.auth_tokens.as_ref().unwrap().jwt().to_string();
        let mut api_client = GfnApiClient::new();
        api_client.set_access_token(token.clone());

        let runtime = self.runtime.clone();
        runtime.spawn(async move {
            // Fetch games from GraphQL MAIN panel (has images)
            // This is the same approach as the official GFN client
            match api_client.fetch_main_games(None).await {
                Ok(games) => {
                    info!("Fetched {} games from GraphQL MAIN panel (with images)", games.len());
                    Self::save_games_cache(&games);
                }
                Err(e) => {
                    error!("Failed to fetch main games from GraphQL: {}", e);

                    // Fallback: try public games list (no images, but has all games)
                    info!("Falling back to public games list...");
                    match api_client.fetch_public_games().await {
                        Ok(games) => {
                            info!("Fetched {} games from public list (fallback)", games.len());
                            Self::save_games_cache(&games);
                        }
                        Err(e2) => {
                            error!("Failed to fetch public games: {}", e2);
                        }
                    }
                }
            }
        });
    }

    /// Fetch user's library games
    pub fn fetch_library(&mut self) {
        if self.auth_tokens.is_none() {
            return;
        }

        self.is_loading = true;
        self.status_message = "Loading your library...".to_string();

        let token = self.auth_tokens.as_ref().unwrap().jwt().to_string();
        let mut api_client = GfnApiClient::new();
        api_client.set_access_token(token.clone());

        let runtime = self.runtime.clone();
        runtime.spawn(async move {
            match api_client.fetch_library(None).await {
                Ok(games) => {
                    info!("Fetched {} games from LIBRARY panel", games.len());
                    Self::save_library_cache(&games);
                }
                Err(e) => {
                    error!("Failed to fetch library: {}", e);
                }
            }
        });
    }

    /// Fetch subscription info (hours, addons, etc.)
    pub fn fetch_subscription(&mut self) {
        if self.auth_tokens.is_none() {
            return;
        }

        let token = self.auth_tokens.as_ref().unwrap().jwt().to_string();
        let user_id = self.auth_tokens.as_ref().unwrap().user_id().to_string();

        let runtime = self.runtime.clone();
        runtime.spawn(async move {
            match crate::api::fetch_subscription(&token, &user_id).await {
                Ok(sub) => {
                    info!("Fetched subscription: tier={}, hours={:.1}/{:.1}, storage={}",
                        sub.membership_tier,
                        sub.remaining_hours,
                        sub.total_hours,
                        sub.has_persistent_storage
                    );
                    Self::save_subscription_cache(&sub);
                }
                Err(e) => {
                    warn!("Failed to fetch subscription: {}", e);
                }
            }
        });
    }

    /// Load available servers/regions (tries dynamic fetch first, falls back to hardcoded)
    pub fn load_servers(&mut self) {
        info!("Loading servers...");

        let runtime = self.runtime.clone();
        let token = self.auth_tokens.as_ref().map(|t| t.jwt().to_string());

        // Spawn async task to fetch dynamic regions
        runtime.spawn(async move {
            let client = reqwest::Client::new();
            let regions = api::fetch_dynamic_regions(&client, token.as_deref()).await;

            // Store the results for the main thread to pick up
            DYNAMIC_REGIONS_CACHE.write().replace(regions);
        });

        // For now, start with hardcoded servers (will update when dynamic fetch completes)
        self.load_hardcoded_servers();
    }

    /// Load hardcoded servers as fallback
    fn load_hardcoded_servers(&mut self) {
        let server_zones: Vec<(&str, &str, &str)> = vec![
            // Europe
            ("eu-netherlands-north", "Netherlands North", "Europe"),
            ("eu-netherlands-south", "Netherlands South", "Europe"),
            ("eu-united-kingdom-1", "United Kingdom", "Europe"),
            ("eu-germany-frankfurt-1", "Frankfurt", "Europe"),
            ("eu-france-paris-1", "Paris", "Europe"),
            ("eu-finland-helsinki-1", "Helsinki", "Europe"),
            ("eu-norway-oslo-1", "Oslo", "Europe"),
            ("eu-sweden-stockholm-1", "Stockholm", "Europe"),
            ("eu-poland-warsaw-1", "Warsaw", "Europe"),
            ("eu-italy-rome-1", "Rome", "Europe"),
            ("eu-spain-madrid-1", "Madrid", "Europe"),
            // North America
            ("us-california-north", "California North", "North America"),
            ("us-california-south", "California South", "North America"),
            ("us-texas-dallas-1", "Dallas", "North America"),
            ("us-virginia-north", "Virginia North", "North America"),
            ("us-illinois-chicago-1", "Chicago", "North America"),
            ("us-washington-seattle-1", "Seattle", "North America"),
            ("us-arizona-phoenix-1", "Phoenix", "North America"),
            // Canada
            ("ca-quebec", "Quebec", "Canada"),
            // Asia-Pacific
            ("ap-japan-tokyo-1", "Tokyo", "Asia-Pacific"),
            ("ap-japan-osaka-1", "Osaka", "Asia-Pacific"),
            ("ap-south-korea-seoul-1", "Seoul", "Asia-Pacific"),
            ("ap-australia-sydney-1", "Sydney", "Asia-Pacific"),
            ("ap-singapore-1", "Singapore", "Asia-Pacific"),
        ];

        self.servers = server_zones
            .iter()
            .map(|(id, name, region)| ServerInfo {
                id: id.to_string(),
                name: name.to_string(),
                region: region.to_string(),
                url: None,
                ping_ms: None,
                status: ServerStatus::Unknown,
            })
            .collect();

        // Restore selected server from settings
        if let Some(ref selected_id) = self.settings.selected_server {
            if let Some(idx) = self.servers.iter().position(|s| s.id == *selected_id) {
                self.selected_server_index = idx;
            }
        }

        info!("Loaded {} hardcoded servers", self.servers.len());
    }

    /// Update servers from dynamic region cache (call this periodically from update loop)
    pub fn check_dynamic_regions(&mut self) {
        let dynamic_regions = DYNAMIC_REGIONS_CACHE.write().take();

        if let Some(regions) = dynamic_regions {
            if !regions.is_empty() {
                info!("[serverInfo] Applying {} dynamic regions", regions.len());

                // Convert dynamic regions to ServerInfo
                // Group by region based on URL pattern
                self.servers = regions
                    .iter()
                    .map(|r| {
                        // Extract server ID from URL hostname
                        // e.g., "https://eu-netherlands-south.cloudmatchbeta.nvidiagrid.net" -> "eu-netherlands-south"
                        let hostname = r.url
                            .trim_start_matches("https://")
                            .trim_start_matches("http://")
                            .split('.')
                            .next()
                            .unwrap_or(&r.name);

                        // Determine region from name or hostname
                        let region = if hostname.starts_with("eu-") || r.name.contains("Europe") || r.name.contains("UK") || r.name.contains("France") || r.name.contains("Germany") {
                            "Europe"
                        } else if hostname.starts_with("us-") || r.name.contains("US") || r.name.contains("California") || r.name.contains("Texas") {
                            "North America"
                        } else if hostname.starts_with("ca-") || r.name.contains("Canada") || r.name.contains("Quebec") {
                            "Canada"
                        } else if hostname.starts_with("ap-") || r.name.contains("Japan") || r.name.contains("Korea") || r.name.contains("Singapore") {
                            "Asia-Pacific"
                        } else {
                            "Other"
                        };

                        ServerInfo {
                            id: hostname.to_string(),
                            name: r.name.clone(),
                            region: region.to_string(),
                            url: Some(r.url.clone()),
                            ping_ms: None,
                            status: ServerStatus::Unknown,
                        }
                    })
                    .collect();

                // Restore selected server
                if let Some(ref selected_id) = self.settings.selected_server {
                    if let Some(idx) = self.servers.iter().position(|s| s.id == *selected_id) {
                        self.selected_server_index = idx;
                    }
                }

                info!("[serverInfo] Now have {} servers", self.servers.len());

                // Auto-start ping test after loading dynamic servers
                self.start_ping_test();
            }
        }
    }

    /// Start ping test for all servers
    pub fn start_ping_test(&mut self) {
        if self.ping_testing {
            return; // Already running
        }

        self.ping_testing = true;
        info!("Starting ping test for {} servers", self.servers.len());

        // Mark all servers as testing
        for server in &mut self.servers {
            server.status = ServerStatus::Testing;
            server.ping_ms = None;
        }

        // Collect server info with URLs for pinging
        let server_data: Vec<(String, Option<String>)> = self.servers
            .iter()
            .map(|s| (s.id.clone(), s.url.clone()))
            .collect();
        let runtime = self.runtime.clone();

        runtime.spawn(async move {
            let mut results: Vec<(String, Option<u32>, ServerStatus)> = Vec::new();

            for (server_id, url_opt) in server_data {
                // Extract hostname from URL or construct from server_id
                let hostname = if let Some(url) = url_opt {
                    url.trim_start_matches("https://")
                        .trim_start_matches("http://")
                        .split('/')
                        .next()
                        .unwrap_or(&format!("{}.cloudmatchbeta.nvidiagrid.net", server_id))
                        .to_string()
                } else {
                    format!("{}.cloudmatchbeta.nvidiagrid.net", server_id)
                };

                // TCP ping to port 443
                let ping_result = Self::tcp_ping(&hostname, 443).await;

                let (ping_ms, status) = match ping_result {
                    Some(ms) => (Some(ms), ServerStatus::Online),
                    None => (None, ServerStatus::Offline),
                };

                results.push((server_id, ping_ms, status));
            }

            // Save results to cache
            Self::save_ping_results(&results);
        });
    }

    /// TCP ping to measure latency
    async fn tcp_ping(hostname: &str, port: u16) -> Option<u32> {
        use std::time::Instant;
        use tokio::net::TcpStream;
        use tokio::time::{timeout, Duration};

        // Resolve hostname first
        let addr = format!("{}:{}", hostname, port);

        let start = Instant::now();
        let result = timeout(Duration::from_secs(3), TcpStream::connect(&addr)).await;

        match result {
            Ok(Ok(_stream)) => {
                let elapsed = start.elapsed().as_millis() as u32;
                Some(elapsed)
            }
            _ => None,
        }
    }

    /// Save ping results to cache (for async loading)
    fn save_ping_results(results: &[(String, Option<u32>, ServerStatus)]) {
        if let Some(path) = Self::get_app_data_dir().map(|p| p.join("ping_results.json")) {
            let cache: Vec<serde_json::Value> = results
                .iter()
                .map(|(id, ping, status)| {
                    serde_json::json!({
                        "id": id,
                        "ping_ms": ping,
                        "status": format!("{:?}", status),
                    })
                })
                .collect();

            if let Ok(json) = serde_json::to_string(&cache) {
                let _ = std::fs::write(path, json);
            }
        }
    }

    /// Load ping results from cache
    fn load_ping_results(&mut self) {
        if let Some(path) = Self::get_app_data_dir().map(|p| p.join("ping_results.json")) {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(results) = serde_json::from_str::<Vec<serde_json::Value>>(&content) {
                    for result in results {
                        if let Some(id) = result.get("id").and_then(|v| v.as_str()) {
                            if let Some(server) = self.servers.iter_mut().find(|s| s.id == id) {
                                server.ping_ms = result.get("ping_ms").and_then(|v| v.as_u64()).map(|v| v as u32);
                                server.status = match result.get("status").and_then(|v| v.as_str()) {
                                    Some("Online") => ServerStatus::Online,
                                    Some("Offline") => ServerStatus::Offline,
                                    _ => ServerStatus::Unknown,
                                };
                            }
                        }
                    }

                    // Clear the ping file after loading
                    let _ = std::fs::remove_file(&path);
                    self.ping_testing = false;

                    // Sort servers by ping (online first, then by ping)
                    self.servers.sort_by(|a, b| {
                        match (&a.status, &b.status) {
                            (ServerStatus::Online, ServerStatus::Online) => {
                                a.ping_ms.unwrap_or(9999).cmp(&b.ping_ms.unwrap_or(9999))
                            }
                            (ServerStatus::Online, _) => std::cmp::Ordering::Less,
                            (_, ServerStatus::Online) => std::cmp::Ordering::Greater,
                            _ => std::cmp::Ordering::Equal,
                        }
                    });

                    // Update selected index after sort
                    if self.auto_server_selection {
                        // Auto-select best server
                        self.select_best_server();
                    } else if let Some(ref selected_id) = self.settings.selected_server {
                        if let Some(idx) = self.servers.iter().position(|s| s.id == *selected_id) {
                            self.selected_server_index = idx;
                        }
                    }
                }
            }
        }
    }

    /// Select the best server based on ping (lowest ping online server)
    fn select_best_server(&mut self) {
        // Find the server with the lowest ping that is online
        let best_server = self.servers
            .iter()
            .enumerate()
            .filter(|(_, s)| s.status == ServerStatus::Online && s.ping_ms.is_some())
            .min_by_key(|(_, s)| s.ping_ms.unwrap_or(9999));

        if let Some((idx, server)) = best_server {
            self.selected_server_index = idx;
            info!("Auto-selected best server: {} ({}ms)", server.name, server.ping_ms.unwrap_or(0));
        }
    }

    /// Launch a game session
    pub fn launch_game(&mut self, game: &GameInfo) {
        info!("Launching game: {} (ID: {})", game.title, game.id);

        // Clear any old session data first
        Self::clear_session_cache();
        Self::clear_session_error();

        self.selected_game = Some(game.clone());
        self.state = AppState::Session;
        self.status_message = format!("Starting {}...", game.title);
        self.error_message = None;
        self.is_loading = true;
        self.last_poll_time = std::time::Instant::now() - POLL_INTERVAL; // Allow immediate first poll

        // Get token and settings for session creation
        let token = match &self.auth_tokens {
            Some(t) => t.jwt().to_string(),
            None => {
                self.error_message = Some("Not logged in".to_string());
                self.is_loading = false;
                return;
            }
        };

        let app_id = game.id.clone();
        let game_title = game.title.clone();
        let settings = self.settings.clone();

        // Use selected server or default
        let zone = self.servers.get(self.selected_server_index)
            .map(|s| s.id.clone())
            .unwrap_or_else(|| "eu-netherlands-south".to_string());

        // Create API client with token
        let mut api_client = GfnApiClient::new();
        api_client.set_access_token(token);

        // Spawn async task to create and poll session
        let runtime = self.runtime.clone();
        runtime.spawn(async move {
            // Create session
            match api_client.create_session(&app_id, &game_title, &settings, &zone).await {
                Ok(session) => {
                    info!("Session created: {} (state: {:?})", session.session_id, session.state);
                    // Save session info for polling
                    Self::save_session_cache(&session);
                }
                Err(e) => {
                    error!("Failed to create session: {}", e);
                    Self::save_session_error(&format!("Failed to create session: {}", e));
                }
            }
        });
    }

    /// Poll session state and update UI
    fn poll_session_status(&mut self) {
        // First check cache for state updates (from in-flight or completed requests)
        if let Some(session) = Self::load_session_cache() {
            if session.state == SessionState::Ready {
                info!("Session ready! GPU: {:?}, Server: {}", session.gpu_type, session.server_ip);
                self.status_message = format!(
                    "Connecting to GPU: {}",
                    session.gpu_type.as_deref().unwrap_or("Unknown")
                );
                Self::clear_session_cache();
                self.start_streaming(session);
                return;
            } else if let SessionState::InQueue { position, eta_secs } = session.state {
                self.status_message = format!("Queue position: {} (ETA: {}s)", position, eta_secs);
            } else if let SessionState::Error(ref msg) = session.state {
                self.error_message = Some(msg.clone());
                self.is_loading = false;
                Self::clear_session_cache();
                return;
            } else {
                self.status_message = "Setting up session...".to_string();
            }
        }

        // Rate limit polling - only poll every POLL_INTERVAL (2 seconds)
        let now = std::time::Instant::now();
        if now.duration_since(self.last_poll_time) < POLL_INTERVAL {
            return;
        }

        if let Some(session) = Self::load_session_cache() {
            let should_poll = matches!(
                session.state,
                SessionState::Requesting | SessionState::Launching | SessionState::InQueue { .. }
            );

            if should_poll {
                // Update timestamp to rate limit next poll
                self.last_poll_time = now;

                let token = match &self.auth_tokens {
                    Some(t) => t.jwt().to_string(),
                    None => return,
                };

                let session_id = session.session_id.clone();
                let zone = session.zone.clone();
                let server_ip = if session.server_ip.is_empty() {
                    None
                } else {
                    Some(session.server_ip.clone())
                };

                let mut api_client = GfnApiClient::new();
                api_client.set_access_token(token);

                let runtime = self.runtime.clone();
                runtime.spawn(async move {
                    match api_client.poll_session(&session_id, &zone, server_ip.as_deref()).await {
                        Ok(updated_session) => {
                            info!("Session poll: state={:?}", updated_session.state);
                            Self::save_session_cache(&updated_session);
                        }
                        Err(e) => {
                            error!("Session poll failed: {}", e);
                        }
                    }
                });
            }
        }

        // Check for session errors
        if let Some(error) = Self::load_session_error() {
            self.error_message = Some(error);
            self.is_loading = false;
            Self::clear_session_error();
        }
    }

    // Session cache helpers
    fn session_cache_path() -> Option<std::path::PathBuf> {
        Self::get_app_data_dir().map(|p| p.join("session_cache.json"))
    }

    fn session_error_path() -> Option<std::path::PathBuf> {
        Self::get_app_data_dir().map(|p| p.join("session_error.txt"))
    }

    fn save_session_cache(session: &SessionInfo) {
        if let Some(path) = Self::session_cache_path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            // Serialize session info (we need to make it serializable)
            let cache = serde_json::json!({
                "session_id": session.session_id,
                "server_ip": session.server_ip,
                "zone": session.zone,
                "state": format!("{:?}", session.state),
                "gpu_type": session.gpu_type,
                "signaling_url": session.signaling_url,
                "is_ready": session.is_ready(),
                "is_queued": session.is_queued(),
                "queue_position": session.queue_position(),
                "media_connection_info": session.media_connection_info.as_ref().map(|mci| {
                    serde_json::json!({
                        "ip": mci.ip,
                        "port": mci.port,
                    })
                }),
            });
            if let Ok(json) = serde_json::to_string(&cache) {
                let _ = std::fs::write(path, json);
            }
        }
    }

    fn load_session_cache() -> Option<SessionInfo> {
        let path = Self::session_cache_path()?;
        let content = std::fs::read_to_string(path).ok()?;
        let cache: serde_json::Value = serde_json::from_str(&content).ok()?;

        let state_str = cache.get("state")?.as_str()?;
        let state = if state_str.contains("Ready") {
            SessionState::Ready
        } else if state_str.contains("Streaming") {
            SessionState::Streaming
        } else if state_str.contains("InQueue") {
            // Parse queue position and eta from state string
            let position = cache.get("queue_position")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            SessionState::InQueue { position, eta_secs: 0 }
        } else if state_str.contains("Error") {
            SessionState::Error(state_str.to_string())
        } else if state_str.contains("Launching") {
            SessionState::Launching
        } else {
            SessionState::Requesting
        };

        // Parse media_connection_info if present
        let media_connection_info = cache.get("media_connection_info")
            .and_then(|v| v.as_object())
            .and_then(|obj| {
                let ip = obj.get("ip")?.as_str()?.to_string();
                let port = obj.get("port")?.as_u64()? as u16;
                Some(crate::app::session::MediaConnectionInfo { ip, port })
            });

        Some(SessionInfo {
            session_id: cache.get("session_id")?.as_str()?.to_string(),
            server_ip: cache.get("server_ip")?.as_str()?.to_string(),
            zone: cache.get("zone")?.as_str()?.to_string(),
            state,
            gpu_type: cache.get("gpu_type").and_then(|v| v.as_str()).map(|s| s.to_string()),
            signaling_url: cache.get("signaling_url").and_then(|v| v.as_str()).map(|s| s.to_string()),
            ice_servers: Vec::new(),
            media_connection_info,
        })
    }

    fn clear_session_cache() {
        if let Some(path) = Self::session_cache_path() {
            let _ = std::fs::remove_file(path);
        }
    }

    fn save_session_error(error: &str) {
        if let Some(path) = Self::session_error_path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(path, error);
        }
    }

    fn load_session_error() -> Option<String> {
        let path = Self::session_error_path()?;
        std::fs::read_to_string(path).ok()
    }

    fn clear_session_error() {
        if let Some(path) = Self::session_error_path() {
            let _ = std::fs::remove_file(path);
        }
    }

    /// Start streaming once session is ready
    pub fn start_streaming(&mut self, session: SessionInfo) {
        info!("Starting streaming to {}", session.server_ip);

        self.session = Some(session.clone());
        self.state = AppState::Streaming;
        self.cursor_captured = true;
        self.is_loading = false;

        // Initialize session timing for proper input timestamps
        // This must be called BEFORE any input events are sent
        crate::input::init_session_timing();

        // Set local cursor dimensions for instant visual feedback
        // Parse resolution from settings (e.g., "1920x1080" -> width, height)
        let (width, height) = parse_resolution(&self.settings.resolution);
        #[cfg(target_os = "windows")]
        crate::input::set_local_cursor_dimensions(width, height);

        info!("Input system initialized: session timing + local cursor {}x{}", width, height);

        // Create shared frame holder for zero-latency frame delivery
        // No buffering - decoder writes latest frame, renderer reads it immediately
        let shared_frame = Arc::new(SharedFrame::new());
        self.shared_frame = Some(shared_frame.clone());

        // Stats channel (small buffer is fine for stats)
        let (stats_tx, stats_rx) = mpsc::channel(8);
        info!("Using zero-latency shared frame delivery for {}fps", self.settings.fps);

        self.stats_rx = Some(stats_rx);

        // Create input handler
        let input_handler = Arc::new(InputHandler::new());
        self.input_handler = Some(input_handler.clone());

        self.status_message = "Connecting...".to_string();

        // Clone settings for the async task
        let settings = self.settings.clone();

        // Spawn the streaming task
        let runtime = self.runtime.clone();
        runtime.spawn(async move {
            match crate::webrtc::run_streaming(
                session,
                settings,
                shared_frame,
                stats_tx,
                input_handler,
            ).await {
                Ok(()) => {
                    info!("Streaming ended normally");
                }
                Err(e) => {
                    error!("Streaming error: {}", e);
                }
            }
        });
    }

    /// Stop streaming and return to games
    pub fn stop_streaming(&mut self) {
        info!("Stopping streaming");

        // Clear session cache first to prevent stale session data
        Self::clear_session_cache();

        // Reset input timing for next session
        crate::input::reset_session_timing();

        // Reset input coalescing and local cursor state
        #[cfg(target_os = "windows")]
        crate::input::reset_coalescing();

        self.cursor_captured = false;
        self.state = AppState::Games;
        self.streaming_session = None;
        self.session = None;  // Clear session info
        self.input_handler = None;
        self.current_frame = None;
        self.shared_frame = None;
        self.stats_rx = None;
        self.selected_game = None;
        self.is_loading = false;
        self.error_message = None;

        self.status_message = "Stream ended".to_string();
    }

    /// Toggle stats overlay
    pub fn toggle_stats(&mut self) {
        self.show_stats = !self.show_stats;
    }

    /// Save settings
    pub fn save_settings(&self) {
        if let Err(e) = self.settings.save() {
            error!("Failed to save settings: {}", e);
        }
    }

    /// Get current user display name
    pub fn user_display_name(&self) -> &str {
        self.user_info.as_ref()
            .map(|u| u.display_name.as_str())
            .unwrap_or("User")
    }

    /// Get current membership tier
    pub fn membership_tier(&self) -> &str {
        self.user_info.as_ref()
            .map(|u| u.membership_tier.as_str())
            .unwrap_or("FREE")
    }

    // Token persistence helpers - cross-platform using data_dir/opennow
    // Cached app data directory (initialized once)
    fn get_app_data_dir() -> Option<std::path::PathBuf> {
        use std::sync::OnceLock;
        static APP_DATA_DIR: OnceLock<Option<std::path::PathBuf>> = OnceLock::new();

        APP_DATA_DIR.get_or_init(|| {
            let data_dir = dirs::data_dir()?;
            let app_dir = data_dir.join("opennow");

            // Ensure directory exists
            if let Err(e) = std::fs::create_dir_all(&app_dir) {
                error!("Failed to create app data directory: {}", e);
            }

            // Migration: copy auth.json from legacy locations if it doesn't exist in new location
            let new_auth = app_dir.join("auth.json");
            if !new_auth.exists() {
                // Try legacy opennow-streamer location (config_dir)
                if let Some(config_dir) = dirs::config_dir() {
                    let legacy_path = config_dir.join("opennow-streamer").join("auth.json");
                    if legacy_path.exists() {
                        if let Err(e) = std::fs::copy(&legacy_path, &new_auth) {
                            warn!("Failed to migrate auth.json from legacy location: {}", e);
                        } else {
                            info!("Migrated auth.json from {:?} to {:?}", legacy_path, new_auth);
                        }
                    }
                }

                // Try gfn-client location (config_dir)
                if !new_auth.exists() {
                    if let Some(config_dir) = dirs::config_dir() {
                        let legacy_path = config_dir.join("gfn-client").join("auth.json");
                        if legacy_path.exists() {
                            if let Err(e) = std::fs::copy(&legacy_path, &new_auth) {
                                warn!("Failed to migrate auth.json from gfn-client: {}", e);
                            } else {
                                info!("Migrated auth.json from {:?} to {:?}", legacy_path, new_auth);
                            }
                        }
                    }
                }
            }

            Some(app_dir)
        }).clone()
    }

    fn tokens_path() -> Option<std::path::PathBuf> {
        Self::get_app_data_dir().map(|p| p.join("auth.json"))
    }

    fn load_tokens() -> Option<AuthTokens> {
        let path = Self::tokens_path()?;
        let content = std::fs::read_to_string(&path).ok()?;
        let tokens: AuthTokens = serde_json::from_str(&content).ok()?;

        // Validate token is not expired
        if tokens.is_expired() {
            info!("Saved token expired, clearing auth file");
            let _ = std::fs::remove_file(&path);
            return None;
        }

        Some(tokens)
    }

    fn save_tokens(tokens: &AuthTokens) {
        if let Some(path) = Self::tokens_path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(json) = serde_json::to_string_pretty(tokens) {
                if let Err(e) = std::fs::write(&path, &json) {
                    error!("Failed to save tokens: {}", e);
                } else {
                    info!("Saved tokens to {:?}", path);
                }
            }
        }
    }

    fn clear_tokens() {
        if let Some(path) = Self::tokens_path() {
            let _ = std::fs::remove_file(path);
            info!("Cleared auth tokens");
        }
    }

    // Games cache for async fetch
    fn games_cache_path() -> Option<std::path::PathBuf> {
        Self::get_app_data_dir().map(|p| p.join("games_cache.json"))
    }

    fn save_games_cache(games: &[GameInfo]) {
        if let Some(path) = Self::games_cache_path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(json) = serde_json::to_string(games) {
                let _ = std::fs::write(path, json);
            }
        }
    }

    fn load_games_cache() -> Option<Vec<GameInfo>> {
        let path = Self::games_cache_path()?;
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    fn clear_games_cache() {
        if let Some(path) = Self::games_cache_path() {
            let _ = std::fs::remove_file(path);
        }
    }

    // Library cache for async fetch
    fn library_cache_path() -> Option<std::path::PathBuf> {
        Self::get_app_data_dir().map(|p| p.join("library_cache.json"))
    }

    fn save_library_cache(games: &[GameInfo]) {
        if let Some(path) = Self::library_cache_path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(json) = serde_json::to_string(games) {
                let _ = std::fs::write(path, json);
            }
        }
    }

    fn load_library_cache() -> Option<Vec<GameInfo>> {
        let path = Self::library_cache_path()?;
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    // Subscription cache for async fetch
    fn subscription_cache_path() -> Option<std::path::PathBuf> {
        Self::get_app_data_dir().map(|p| p.join("subscription_cache.json"))
    }

    fn save_subscription_cache(sub: &SubscriptionInfo) {
        if let Some(path) = Self::subscription_cache_path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let cache = serde_json::json!({
                "membership_tier": sub.membership_tier,
                "remaining_hours": sub.remaining_hours,
                "total_hours": sub.total_hours,
                "has_persistent_storage": sub.has_persistent_storage,
                "storage_size_gb": sub.storage_size_gb,
            });
            if let Ok(json) = serde_json::to_string(&cache) {
                let _ = std::fs::write(path, json);
            }
        }
    }

    fn load_subscription_cache() -> Option<SubscriptionInfo> {
        let path = Self::subscription_cache_path()?;
        let content = std::fs::read_to_string(path).ok()?;
        let cache: serde_json::Value = serde_json::from_str(&content).ok()?;

        Some(SubscriptionInfo {
            membership_tier: cache.get("membership_tier")?.as_str()?.to_string(),
            remaining_hours: cache.get("remaining_hours")?.as_f64()? as f32,
            total_hours: cache.get("total_hours")?.as_f64()? as f32,
            has_persistent_storage: cache.get("has_persistent_storage")?.as_bool()?,
            storage_size_gb: cache.get("storage_size_gb").and_then(|v| v.as_u64()).map(|v| v as u32),
        })
    }
}
