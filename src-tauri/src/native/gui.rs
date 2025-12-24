//! GFN Native GUI Client
//!
//! A complete GUI application for GeForce NOW streaming.
//! Handles login, game browsing, session launching, and streaming.

mod input;
mod signaling;
mod webrtc_client;

use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;
use log::{info, warn, error, debug};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use input::{InputEncoder, InputEvent};
use signaling::{GfnSignaling, SignalingEvent};
use webrtc_client::{WebRtcClient, WebRtcEvent};
use webrtc::ice_transport::ice_server::RTCIceServer;
use openh264::formats::YUVSource;

// ============================================================================
// Data Structures
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuthTokens {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GameInfo {
    id: String,
    title: String,
    publisher: Option<String>,
    image_url: Option<String>,
    store: String,
    #[serde(rename = "cmsId")]
    cms_id: Option<String>,
    #[serde(default)]
    app_id: Option<i64>,  // GFN internal app ID
}

// CloudMatch API request structure
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CloudMatchRequest {
    session_request_data: SessionRequestData,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionRequestData {
    app_id: i64,
    internal_title: Option<String>,
    available_supported_controllers: Vec<i32>,
    preferred_controller: i32,
    network_test_session_id: Option<String>,
    parent_session_id: Option<String>,
    client_identification: String,
    device_hash_id: String,
    client_version: String,
    sdk_version: String,
    streamer_version: String,
    client_platform_name: String,
    client_request_monitor_settings: Vec<MonitorSettings>,
    use_ops: bool,
    audio_mode: i32,
    meta_data: Vec<MetaDataEntry>,
    sdr_hdr_mode: i32,
    surround_audio_info: i32,
    remote_controllers_bitmap: i32,
    client_timezone_offset: i64,
    enhanced_stream_mode: i32,
    app_launch_mode: i32,
    secure_rtsp_supported: bool,
    partner_custom_data: Option<String>,
    account_linked: bool,
    enable_persisting_in_game_settings: bool,
    requested_audio_format: i32,
    user_age: i32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MonitorSettings {
    monitor_id: i32,
    position_x: i32,
    position_y: i32,
    width_in_pixels: u32,
    height_in_pixels: u32,
    dpi: i32,
    frames_per_second: u32,
    sdr_hdr_mode: i32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MetaDataEntry {
    key: String,
    value: String,
}

// CloudMatch API response structures
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CloudMatchResponse {
    session: CloudMatchSession,
    request_status: RequestStatus,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CloudMatchSession {
    session_id: String,
    #[serde(default)]
    seat_setup_info: Option<SeatSetupInfo>,
    #[serde(default)]
    session_control_info: Option<SessionControlInfo>,
    #[serde(default)]
    connection_info: Option<Vec<ConnectionInfoData>>,
    #[serde(default)]
    gpu_type: Option<String>,
    #[serde(default)]
    status: i32,
    #[serde(default)]
    error_code: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeatSetupInfo {
    #[serde(default)]
    queue_position: i32,
    #[serde(default)]
    seat_setup_eta: i32,
    #[serde(default)]
    seat_setup_step: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionControlInfo {
    #[serde(default)]
    ip: Option<String>,
    #[serde(default)]
    port: u16,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionInfoData {
    #[serde(default)]
    ip: Option<String>,
    #[serde(default)]
    port: u16,
    #[serde(default)]
    resource_path: Option<String>,
    #[serde(default)]
    usage: i32,  // 14 = streaming/signaling server
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RequestStatus {
    status_code: i32,
    #[serde(default)]
    status_description: Option<String>,
    #[serde(default)]
    unified_error_code: i32,
}

#[derive(Debug, Clone)]
struct SessionInfo {
    session_id: String,
    server_ip: String,
    zone: String,
    state: SessionState,
    gpu_type: Option<String>,
    signaling_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum SessionState {
    Requesting,  // Calling CloudMatch API
    Launching,   // Session created, seat being set up
    InQueue { position: u32, eta_secs: u32 },
    Ready,
    Streaming,
    Error(String),
}

// Shared session state for async updates
#[derive(Default)]
struct SessionUpdate {
    session: Option<SessionInfo>,
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum AppScreen {
    Login,
    Games,
    Session,
    Streaming,
}

// ============================================================================
// App State
// ============================================================================

struct GfnGuiApp {
    // Runtime
    runtime: tokio::runtime::Runtime,

    // Auth
    auth_tokens: Option<AuthTokens>,
    login_url: Option<String>,
    auth_code: String,

    // Games - shared state for async loading
    games: Vec<GameInfo>,
    games_loading: bool,
    games_shared: Arc<Mutex<Option<Vec<GameInfo>>>>,
    search_query: String,
    selected_game: Option<GameInfo>,

    // Session - shared state for async session updates
    current_session: Option<SessionInfo>,
    session_shared: Arc<Mutex<SessionUpdate>>,
    session_polling: bool,

    // Streaming
    streaming_state: Arc<Mutex<StreamingState>>,
    input_tx: Option<mpsc::Sender<InputEvent>>,

    // UI
    current_screen: AppScreen,
    status_message: String,
    error_message: Option<String>,

    // Texture cache for game images
    texture_cache: std::collections::HashMap<String, egui::TextureHandle>,
}

#[derive(Default)]
struct StreamingState {
    connected: bool,
    video_frame: Option<VideoFrame>,
    frames_received: u64,
    status: String,
}

struct VideoFrame {
    width: u32,
    height: u32,
    pixels: Vec<egui::Color32>,
}

impl Default for GfnGuiApp {
    fn default() -> Self {
        Self {
            runtime: tokio::runtime::Runtime::new().unwrap(),
            auth_tokens: None,
            login_url: None,
            auth_code: String::new(),
            games: Vec::new(),
            games_loading: false,
            games_shared: Arc::new(Mutex::new(None)),
            search_query: String::new(),
            selected_game: None,
            current_session: None,
            session_shared: Arc::new(Mutex::new(SessionUpdate::default())),
            session_polling: false,
            streaming_state: Arc::new(Mutex::new(StreamingState::default())),
            input_tx: None,
            current_screen: AppScreen::Login,
            status_message: "Welcome to GFN Native Client".to_string(),
            error_message: None,
            texture_cache: std::collections::HashMap::new(),
        }
    }
}

impl GfnGuiApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        // Load saved tokens if available
        let mut app = Self::default();

        if let Ok(tokens_json) = std::fs::read_to_string("gfn_tokens.json") {
            if let Ok(tokens) = serde_json::from_str::<AuthTokens>(&tokens_json) {
                // Check if token is still valid
                let now = chrono::Utc::now().timestamp();
                if tokens.expires_at > now {
                    app.auth_tokens = Some(tokens);
                    app.current_screen = AppScreen::Games;
                    app.status_message = "Logged in".to_string();
                    app.fetch_games();
                }
            }
        }

        app
    }

    fn save_tokens(&self) {
        if let Some(tokens) = &self.auth_tokens {
            if let Ok(json) = serde_json::to_string_pretty(tokens) {
                let _ = std::fs::write("gfn_tokens.json", json);
            }
        }
    }

    fn logout(&mut self) {
        self.auth_tokens = None;
        self.games.clear();
        self.current_session = None;
        self.current_screen = AppScreen::Login;
        let _ = std::fs::remove_file("gfn_tokens.json");
        self.status_message = "Logged out".to_string();
    }

    fn start_oauth_flow(&mut self) {
        // Generate PKCE code verifier and challenge
        let code_verifier: String = (0..64)
            .map(|_| {
                let idx = rand::random::<usize>() % 62;
                "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789"
                    .chars()
                    .nth(idx)
                    .unwrap()
            })
            .collect();

        use sha2::{Sha256, Digest};
        use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

        let mut hasher = Sha256::new();
        hasher.update(code_verifier.as_bytes());
        let code_challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());

        // Build OAuth URL
        let client_id = "ZU7sPN-miLujMD95LfOQ453IB0AtjM8sMyvgJ9wCXEQ";
        let redirect_uri = "http://127.0.0.1:8080/callback";
        let scopes = "openid consent email tk_client age";

        let auth_url = format!(
            "https://login.nvidia.com/oauth/authorize?\
            client_id={}&\
            redirect_uri={}&\
            response_type=code&\
            scope={}&\
            code_challenge={}&\
            code_challenge_method=S256",
            urlencoding::encode(client_id),
            urlencoding::encode(redirect_uri),
            urlencoding::encode(scopes),
            code_challenge
        );

        self.login_url = Some(auth_url.clone());

        // Open browser
        if let Err(e) = open::that(&auth_url) {
            self.error_message = Some(format!("Failed to open browser: {}", e));
        } else {
            self.status_message = "Opening browser for login...".to_string();
        }
    }

    fn exchange_code_for_tokens(&mut self, code: &str) {
        let code = code.to_string();
        let streaming_state = self.streaming_state.clone();

        self.runtime.spawn(async move {
            let client = reqwest::Client::new();

            let params = [
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("client_id", "ZU7sPN-miLujMD95LfOQ453IB0AtjM8sMyvgJ9wCXEQ"),
                ("redirect_uri", "http://127.0.0.1:8080/callback"),
            ];

            match client
                .post("https://login.nvidia.com/oauth/token")
                .form(&params)
                .send()
                .await
            {
                Ok(resp) => {
                    if let Ok(text) = resp.text().await {
                        info!("Token response: {}", text);
                    }
                }
                Err(e) => {
                    error!("Token exchange failed: {}", e);
                }
            }
        });

        self.status_message = "Exchanging code for tokens...".to_string();
    }

    fn set_access_token(&mut self, token: String) {
        let expires_at = chrono::Utc::now().timestamp() + 3600 * 24; // 24 hours
        self.auth_tokens = Some(AuthTokens {
            access_token: token,
            refresh_token: None,
            expires_at,
        });
        self.save_tokens();
        self.current_screen = AppScreen::Games;
        self.status_message = "Logged in".to_string();
        self.fetch_games();
    }

    fn fetch_games(&mut self) {
        if self.games_loading {
            return;
        }

        self.games_loading = true;
        self.status_message = "Loading games...".to_string();

        let games_shared = self.games_shared.clone();
        let runtime = self.runtime.handle().clone();

        // Fetch from static games list (no auth required)
        runtime.spawn(async move {
            let client = reqwest::Client::new();
            let url = "https://static.nvidiagrid.net/supported-public-game-list/locales/gfnpc-en-US.json";

            match client.get(url).send().await {
                Ok(resp) => {
                    if let Ok(text) = resp.text().await {
                        info!("Fetched {} bytes of games data", text.len());

                        // Parse the JSON array of games
                        if let Ok(games_json) = serde_json::from_str::<Vec<serde_json::Value>>(&text) {
                            let games: Vec<GameInfo> = games_json.iter()
                                .filter_map(|g| {
                                    let title = g.get("title")?.as_str()?.to_string();
                                    let id = g.get("id")
                                        .and_then(|v| v.as_str())
                                        .or_else(|| g.get("cmsId").and_then(|v| v.as_str()))
                                        .unwrap_or(&title)
                                        .to_string();
                                    let publisher = g.get("publisher").and_then(|v| v.as_str()).map(|s| s.to_string());
                                    let image_url = g.get("imageUrl").and_then(|v| v.as_str()).map(|s| s.to_string());
                                    let store = g.get("store").and_then(|v| v.as_str()).unwrap_or("Unknown").to_string();
                                    let cms_id = g.get("cmsId").and_then(|v| v.as_str()).map(|s| s.to_string());
                                    // Get appId (the GFN internal app ID used for session requests)
                                    let app_id = g.get("appId")
                                        .and_then(|v| v.as_i64())
                                        .or_else(|| g.get("id").and_then(|v| v.as_i64()));

                                    Some(GameInfo {
                                        id,
                                        title,
                                        publisher,
                                        image_url,
                                        store,
                                        cms_id,
                                        app_id,
                                    })
                                })
                                .collect();

                            info!("Parsed {} games", games.len());

                            // Store in shared state
                            let mut shared = games_shared.lock();
                            *shared = Some(games);
                        } else {
                            error!("Failed to parse games JSON");
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to fetch games: {}", e);
                }
            }
        });
    }

    fn launch_game(&mut self, game: &GameInfo) {
        let Some(tokens) = &self.auth_tokens else {
            self.error_message = Some("Not logged in".to_string());
            return;
        };

        info!("Launching game: {} (id: {})", game.title, game.id);

        self.selected_game = Some(game.clone());
        self.current_screen = AppScreen::Session;
        self.status_message = format!("Requesting session for {}...", game.title);
        self.error_message = None;

        // Set initial session state
        self.current_session = Some(SessionInfo {
            session_id: String::new(),
            server_ip: String::new(),
            zone: "eu-west".to_string(),
            state: SessionState::Requesting,
            gpu_type: None,
            signaling_url: None,
        });
        self.session_polling = true;

        // Start async session request
        let access_token = tokens.access_token.clone();
        let game_id = game.id.clone();
        let game_title = game.title.clone();
        let app_id = game.app_id.unwrap_or_else(|| {
            // Try to parse game_id as app_id
            game.id.parse::<i64>().unwrap_or(0)
        });
        let session_shared = self.session_shared.clone();

        self.runtime.spawn(async move {
            info!("Starting session request for app_id: {}", app_id);

            match request_gfn_session(&access_token, app_id, &game_title).await {
                Ok(session) => {
                    info!("Session created: {} on {}", session.session_id, session.server_ip);
                    let mut shared = session_shared.lock();
                    shared.session = Some(session);
                    shared.error = None;
                }
                Err(e) => {
                    error!("Session request failed: {}", e);
                    let mut shared = session_shared.lock();
                    shared.error = Some(e);
                }
            }
        });
    }

    fn start_streaming(&mut self, server_ip: String, session_id: String) {
        self.current_screen = AppScreen::Streaming;

        let streaming_state = self.streaming_state.clone();
        let (input_tx, input_rx) = mpsc::channel::<InputEvent>(256);
        self.input_tx = Some(input_tx);

        self.runtime.spawn(async move {
            if let Err(e) = run_streaming_session(server_ip, session_id, streaming_state, input_rx).await {
                error!("Streaming error: {}", e);
            }
        });
    }

    fn render_login_screen(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(50.0);

            ui.heading("GFN Native Client");
            ui.add_space(20.0);

            ui.label("Login with your NVIDIA account to access GeForce NOW");
            ui.add_space(30.0);

            if ui.button("🔐 Login with NVIDIA").clicked() {
                self.start_oauth_flow();
            }

            ui.add_space(20.0);
            ui.separator();
            ui.add_space(20.0);

            ui.label("Or paste your access token:");
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.auth_code);
                if ui.button("Set Token").clicked() && !self.auth_code.is_empty() {
                    let token = self.auth_code.clone();
                    self.auth_code.clear();
                    self.set_access_token(token);
                }
            });

            if let Some(url) = &self.login_url {
                ui.add_space(20.0);
                ui.label("If browser didn't open, visit:");
                if ui.link(url).clicked() {
                    let _ = open::that(url);
                }
            }
        });
    }

    fn render_games_screen(&mut self, ui: &mut egui::Ui) {
        // Top bar
        ui.horizontal(|ui| {
            ui.heading("🎮 Games Library");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("🚪 Logout").clicked() {
                    self.logout();
                }
                if ui.button("🔄 Refresh").clicked() {
                    self.fetch_games();
                }
            });
        });

        ui.separator();

        // Search bar
        ui.horizontal(|ui| {
            ui.label("🔍");
            ui.text_edit_singleline(&mut self.search_query);
        });

        ui.add_space(10.0);

        // Games grid
        if self.games_loading {
            ui.centered_and_justified(|ui| {
                ui.spinner();
                ui.label("Loading games...");
            });
        } else if self.games.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(50.0);
                ui.label("No games found");
                ui.add_space(10.0);

                // Demo games for testing
                ui.label("Demo games (for testing):");
                ui.add_space(10.0);

                let demo_games = vec![
                    ("Cyberpunk 2077", "CD Projekt Red"),
                    ("Fortnite", "Epic Games"),
                    ("Counter-Strike 2", "Valve"),
                    ("Destiny 2", "Bungie"),
                ];

                for (title, publisher) in demo_games {
                    if ui.button(format!("▶ {}", title)).clicked() {
                        let game = GameInfo {
                            id: uuid::Uuid::new_v4().to_string(),
                            title: title.to_string(),
                            publisher: Some(publisher.to_string()),
                            image_url: None,
                            store: "Steam".to_string(),
                            cms_id: None,
                            app_id: None,
                        };
                        self.launch_game(&game);
                    }
                }
            });
        } else {
            // Clone games to avoid borrow issues
            let search_lower = self.search_query.to_lowercase();
            let filtered_games: Vec<GameInfo> = self.games
                .iter()
                .filter(|g| {
                    search_lower.is_empty() ||
                    g.title.to_lowercase().contains(&search_lower)
                })
                .cloned()
                .collect();

            let mut clicked_game: Option<GameInfo> = None;

            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("games_grid")
                    .num_columns(4)
                    .spacing([10.0, 10.0])
                    .show(ui, |ui| {
                        for (i, game) in filtered_games.iter().enumerate() {
                            ui.vertical(|ui| {
                                // Game card
                                let response = ui.allocate_response(
                                    egui::vec2(150.0, 200.0),
                                    egui::Sense::click(),
                                );

                                if response.clicked() {
                                    clicked_game = Some(game.clone());
                                }

                                let rect = response.rect;
                                ui.painter().rect_filled(
                                    rect,
                                    5.0,
                                    egui::Color32::from_rgb(40, 40, 50),
                                );

                                // Title
                                ui.painter().text(
                                    rect.center_bottom() - egui::vec2(0.0, 20.0),
                                    egui::Align2::CENTER_BOTTOM,
                                    &game.title,
                                    egui::FontId::proportional(12.0),
                                    egui::Color32::WHITE,
                                );

                                if response.hovered() {
                                    ui.painter().rect_stroke(
                                        rect,
                                        5.0,
                                        egui::Stroke::new(2.0, egui::Color32::from_rgb(118, 185, 0)),
                                    );
                                }
                            });

                            if (i + 1) % 4 == 0 {
                                ui.end_row();
                            }
                        }
                    });
            });

            // Handle clicked game after the UI
            if let Some(game) = clicked_game {
                self.launch_game(&game);
            }
        }
    }

    fn render_session_screen(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(50.0);

            if let Some(game) = &self.selected_game {
                ui.heading(&game.title);
            }

            ui.add_space(30.0);

            if let Some(session) = &self.current_session {
                // Show session ID if we have one
                if !session.session_id.is_empty() {
                    ui.label(format!("Session: {}", &session.session_id[..std::cmp::min(8, session.session_id.len())]));
                    ui.add_space(10.0);
                }

                match &session.state {
                    SessionState::Requesting => {
                        ui.spinner();
                        ui.label("Requesting session from CloudMatch...");
                    }
                    SessionState::Launching => {
                        ui.spinner();
                        ui.label("Setting up session...");
                        if let Some(ref gpu) = session.gpu_type {
                            ui.label(format!("GPU: {}", gpu));
                        }
                    }
                    SessionState::InQueue { position, eta_secs } => {
                        ui.spinner();
                        ui.label(format!("In queue: position {}", position));
                        ui.label(format!("Estimated wait: {} seconds", eta_secs));
                    }
                    SessionState::Ready => {
                        ui.colored_label(egui::Color32::GREEN, "✅ Session ready!");
                        ui.add_space(10.0);

                        if let Some(ref gpu) = session.gpu_type {
                            ui.label(format!("GPU: {}", gpu));
                        }
                        if !session.server_ip.is_empty() {
                            ui.label(format!("Server: {}", session.server_ip));
                        }

                        ui.add_space(20.0);

                        if ui.button("▶ Start Streaming").clicked() {
                            let server = session.server_ip.clone();
                            let sid = session.session_id.clone();
                            self.start_streaming(server, sid);
                        }
                    }
                    SessionState::Streaming => {
                        ui.label("🎮 Streaming...");
                    }
                    SessionState::Error(e) => {
                        ui.colored_label(egui::Color32::RED, format!("Error: {}", e));
                        ui.add_space(10.0);
                        if ui.button("Retry").clicked() {
                            if let Some(game) = self.selected_game.clone() {
                                self.launch_game(&game);
                            }
                        }
                    }
                }
            } else {
                ui.label("No session");
            }

            ui.add_space(30.0);

            if ui.button("← Back to Games").clicked() {
                self.current_session = None;
                self.session_polling = false;
                self.error_message = None;
                self.current_screen = AppScreen::Games;
            }

            // Manual connect section
            ui.add_space(30.0);
            ui.separator();
            ui.add_space(10.0);
            ui.label("Manual Connect (for testing):");

            ui.horizontal(|ui| {
                ui.label("Server IP:");
                static mut DEMO_SERVER: String = String::new();
                static mut DEMO_SESSION: String = String::new();

                unsafe {
                    ui.text_edit_singleline(&mut DEMO_SERVER);
                    ui.label("Session ID:");
                    ui.text_edit_singleline(&mut DEMO_SESSION);

                    if ui.button("Connect").clicked() && !DEMO_SERVER.is_empty() && !DEMO_SESSION.is_empty() {
                        let server = DEMO_SERVER.clone();
                        let session = DEMO_SESSION.clone();
                        self.start_streaming(server, session);
                    }
                }
            });
        });
    }

    fn render_streaming_screen(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let state = self.streaming_state.lock();

        // Full screen video
        let available = ui.available_size();

        if let Some(frame) = &state.video_frame {
            // Create texture from video frame
            let pixels: Vec<egui::Color32> = frame.pixels.clone();
            let image = egui::ColorImage {
                size: [frame.width as usize, frame.height as usize],
                pixels,
            };

            let texture = ctx.load_texture(
                "video_frame",
                image,
                egui::TextureOptions::LINEAR,
            );

            ui.image(&texture);
        } else {
            // Show status overlay
            ui.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| {
                    if state.connected {
                        ui.spinner();
                        ui.label("Waiting for video...");
                    } else {
                        ui.label(&state.status);
                    }

                    ui.add_space(20.0);
                    ui.label(format!("Frames: {}", state.frames_received));
                });
            });
        }

        drop(state);

        // Overlay controls (ESC to exit)
        egui::Area::new(egui::Id::new("streaming_overlay"))
            .anchor(egui::Align2::LEFT_TOP, [10.0, 10.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("⬅ Exit").clicked() {
                        self.current_screen = AppScreen::Games;
                        self.input_tx = None;
                    }

                    let state = self.streaming_state.lock();
                    if state.connected {
                        ui.label("🟢 Connected");
                    } else {
                        ui.label("🔴 Connecting...");
                    }
                });
            });
    }
}

impl eframe::App for GfnGuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for loaded games from async task
        {
            let mut games_shared = self.games_shared.lock();
            if let Some(games) = games_shared.take() {
                self.games = games;
                self.games_loading = false;
                self.status_message = format!("Logged in - {} games available", self.games.len());
            }
        }

        // Check for session updates from async task
        {
            let mut session_shared = self.session_shared.lock();

            // Check for errors
            if let Some(error) = session_shared.error.take() {
                self.error_message = Some(error.clone());
                self.status_message = format!("Session error: {}", error);
                if let Some(ref mut session) = self.current_session {
                    session.state = SessionState::Error(error);
                }
                self.session_polling = false;
            }

            // Check for session updates
            if let Some(new_session) = session_shared.session.take() {
                info!("Session update received: {:?}", new_session.state);

                // If session was just created (Launching state), start polling
                let should_poll = matches!(new_session.state, SessionState::Launching | SessionState::InQueue { .. });
                let session_id = new_session.session_id.clone();
                let zone = new_session.zone.clone();

                self.current_session = Some(new_session.clone());

                match &new_session.state {
                    SessionState::Ready => {
                        self.status_message = format!("Session ready! GPU: {}",
                            new_session.gpu_type.as_deref().unwrap_or("Unknown"));
                        self.session_polling = false;
                    }
                    SessionState::InQueue { position, eta_secs } => {
                        self.status_message = format!("In queue: position {} (ETA: {}s)", position, eta_secs);
                    }
                    SessionState::Launching => {
                        self.status_message = "Setting up session...".to_string();
                    }
                    _ => {}
                }

                // Start polling if session is not ready yet
                if should_poll && self.session_polling {
                    if let Some(tokens) = &self.auth_tokens {
                        let access_token = tokens.access_token.clone();
                        let session_shared = self.session_shared.clone();

                        self.runtime.spawn(async move {
                            poll_session_status(&access_token, &session_id, &zone, session_shared).await;
                        });
                    }
                }
            }
        }

        // Handle keyboard input for streaming
        if self.current_screen == AppScreen::Streaming {
            ctx.input(|i| {
                if i.key_pressed(egui::Key::Escape) {
                    self.current_screen = AppScreen::Games;
                    self.input_tx = None;
                }
            });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            // Status bar
            ui.horizontal(|ui| {
                ui.label(&self.status_message);

                if let Some(err) = &self.error_message {
                    ui.colored_label(egui::Color32::RED, err);
                }
            });
            ui.separator();

            // Main content
            match self.current_screen {
                AppScreen::Login => self.render_login_screen(ui),
                AppScreen::Games => self.render_games_screen(ui),
                AppScreen::Session => self.render_session_screen(ui),
                AppScreen::Streaming => self.render_streaming_screen(ctx, ui),
            }
        });

        // Request continuous repaint during streaming, loading games, or session polling
        if self.current_screen == AppScreen::Streaming || self.games_loading || self.session_polling {
            ctx.request_repaint();
        }
    }
}

// ============================================================================
// GFN Session API
// ============================================================================

/// Request a new GFN session via CloudMatch API
async fn request_gfn_session(
    access_token: &str,
    app_id: i64,
    game_title: &str,
) -> Result<SessionInfo, String> {
    info!("Requesting GFN session for app_id: {}, title: {}", app_id, game_title);

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)  // GFN servers may have self-signed certs
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let zone = "eu-west";  // TODO: Make configurable
    let device_id = uuid::Uuid::new_v4().to_string();
    let client_id = uuid::Uuid::new_v4().to_string();
    let sub_session_id = uuid::Uuid::new_v4().to_string();

    // Build CloudMatch request
    let request = CloudMatchRequest {
        session_request_data: SessionRequestData {
            app_id,
            internal_title: Some(game_title.to_string()),
            available_supported_controllers: vec![],
            preferred_controller: 0,
            network_test_session_id: Some("00000000-0000-0000-0000-000000000000".to_string()),
            parent_session_id: None,
            client_identification: "GFN-PC".to_string(),
            device_hash_id: device_id.clone(),
            client_version: "30.0".to_string(),
            sdk_version: "1.0".to_string(),
            streamer_version: "1".to_string(),
            client_platform_name: "windows".to_string(),
            client_request_monitor_settings: vec![MonitorSettings {
                monitor_id: 0,
                position_x: 0,
                position_y: 0,
                width_in_pixels: 1920,
                height_in_pixels: 1080,
                dpi: 96,
                frames_per_second: 60,
                sdr_hdr_mode: 0,
            }],
            use_ops: false,
            audio_mode: 0,
            meta_data: vec![
                MetaDataEntry { key: "SubSessionId".to_string(), value: sub_session_id },
                MetaDataEntry { key: "wssignaling".to_string(), value: "1".to_string() },
                MetaDataEntry { key: "GSStreamerType".to_string(), value: "WebRTC".to_string() },
                MetaDataEntry { key: "networkType".to_string(), value: "Unknown".to_string() },
            ],
            sdr_hdr_mode: 0,
            surround_audio_info: 0,
            remote_controllers_bitmap: 0,
            client_timezone_offset: 0,
            enhanced_stream_mode: 1,
            app_launch_mode: 1,
            secure_rtsp_supported: false,
            partner_custom_data: Some("".to_string()),
            account_linked: false,
            enable_persisting_in_game_settings: false,
            requested_audio_format: 0,
            user_age: 0,
        },
    };

    let session_url = format!(
        "https://{}.cloudmatchbeta.nvidiagrid.net/v2/session?keyboardLayout=en-US&languageCode=en_US",
        zone
    );

    info!("Sending session request to: {}", session_url);

    let response = client
        .post(&session_url)
        .header("Authorization", format!("GFNJWT {}", access_token))
        .header("Content-Type", "application/json")
        .header("nv-browser-type", "CHROME")
        .header("nv-client-id", &client_id)
        .header("nv-client-streamer", "NVIDIA-CLASSIC")
        .header("nv-client-type", "NATIVE")
        .header("nv-client-version", "2.0.80.173")
        .header("nv-device-os", "WINDOWS")
        .header("nv-device-type", "DESKTOP")
        .header("x-device-id", &device_id)
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    let status = response.status();
    let response_text = response.text().await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    info!("CloudMatch response status: {}", status);
    info!("CloudMatch response: {}", &response_text[..std::cmp::min(500, response_text.len())]);

    if !status.is_success() {
        return Err(format!("CloudMatch request failed: {} - {}", status, response_text));
    }

    let api_response: CloudMatchResponse = serde_json::from_str(&response_text)
        .map_err(|e| format!("Failed to parse response: {} - {}", e, &response_text[..std::cmp::min(200, response_text.len())]))?;

    if api_response.request_status.status_code != 1 {
        let error_desc = api_response.request_status.status_description
            .unwrap_or_else(|| "Unknown error".to_string());
        return Err(format!("CloudMatch error: {} (code: {})",
            error_desc, api_response.request_status.status_code));
    }

    let session_data = api_response.session;
    info!("Session allocated: {} (status: {})", session_data.session_id, session_data.status);

    // Determine session state
    let state = if session_data.status == 2 {
        SessionState::Ready
    } else if let Some(ref seat_info) = session_data.seat_setup_info {
        if seat_info.queue_position > 0 {
            SessionState::InQueue {
                position: seat_info.queue_position as u32,
                eta_secs: (seat_info.seat_setup_eta / 1000) as u32,
            }
        } else {
            SessionState::Launching
        }
    } else {
        SessionState::Launching
    };

    // Use connection_info with usage=14 for streaming (the WebRTC signaling server)
    let streaming_conn = session_data.connection_info
        .as_ref()
        .and_then(|conns| conns.iter().find(|c| c.usage == 14));

    let server_ip = streaming_conn
        .and_then(|conn| conn.ip.clone())
        .or_else(|| session_data.session_control_info.as_ref().and_then(|sci| sci.ip.clone()))
        .unwrap_or_default();

    let signaling_url = streaming_conn
        .and_then(|conn| conn.resource_path.clone());

    info!("Stream server IP: {}, signaling path: {:?}", server_ip, signaling_url);

    // Debug: log all connection_info entries
    if let Some(conns) = &session_data.connection_info {
        for (i, c) in conns.iter().enumerate() {
            info!("  connection_info[{}]: ip={:?}, port={}, usage={}, path={:?}",
                i, c.ip, c.port, c.usage, c.resource_path);
        }
    }

    Ok(SessionInfo {
        session_id: session_data.session_id,
        server_ip,
        zone: zone.to_string(),
        state,
        gpu_type: session_data.gpu_type,
        signaling_url,
    })
}

/// Poll session status until ready
async fn poll_session_status(
    access_token: &str,
    session_id: &str,
    zone: &str,
    session_shared: Arc<Mutex<SessionUpdate>>,
) {
    info!("Starting session polling for: {}", session_id);

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();

    let poll_url = format!(
        "https://{}.cloudmatchbeta.nvidiagrid.net/v2/session/{}",
        zone, session_id
    );

    let device_id = uuid::Uuid::new_v4().to_string();
    let client_id = uuid::Uuid::new_v4().to_string();

    for poll_count in 0..120 {  // Max 3 minutes
        tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;

        let response = match client
            .get(&poll_url)
            .header("Authorization", format!("GFNJWT {}", access_token))
            .header("Content-Type", "application/json")
            .header("nv-client-id", &client_id)
            .header("nv-client-streamer", "NVIDIA-CLASSIC")
            .header("nv-client-type", "NATIVE")
            .header("x-device-id", &device_id)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                error!("Poll request failed: {}", e);
                continue;
            }
        };

        let response_text = match response.text().await {
            Ok(t) => t,
            Err(e) => {
                error!("Failed to read poll response: {}", e);
                continue;
            }
        };

        let poll_response: CloudMatchResponse = match serde_json::from_str(&response_text) {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to parse poll response: {}", e);
                continue;
            }
        };

        if poll_response.request_status.status_code != 1 {
            let error = poll_response.request_status.status_description
                .unwrap_or_else(|| "Unknown error".to_string());
            error!("Session poll error: {}", error);
            let mut shared = session_shared.lock();
            shared.error = Some(error);
            return;
        }

        let session = poll_response.session;
        let status = session.status;
        let seat_info = session.seat_setup_info.as_ref();
        let step = seat_info.map(|s| s.seat_setup_step).unwrap_or(0);
        let queue_pos = seat_info.map(|s| s.queue_position).unwrap_or(0);
        let eta = seat_info.map(|s| s.seat_setup_eta).unwrap_or(0);

        info!("Poll {}: status={}, step={}, queue={}, eta={}ms, gpu={:?}",
            poll_count, status, step, queue_pos, eta, session.gpu_type);

        // Status 2 = ready
        if status == 2 {
            info!("Session ready! GPU: {:?}", session.gpu_type);

            // Debug: log all connection_info entries
            if let Some(conns) = &session.connection_info {
                for (i, c) in conns.iter().enumerate() {
                    info!("  connection_info[{}]: ip={:?}, port={}, usage={}, path={:?}",
                        i, c.ip, c.port, c.usage, c.resource_path);
                }
            }

            // Use connection_info with usage=14 for streaming (the WebRTC signaling server)
            let streaming_conn = session.connection_info
                .as_ref()
                .and_then(|conns| conns.iter().find(|c| c.usage == 14));

            let server_ip = streaming_conn
                .and_then(|conn| conn.ip.clone())
                .or_else(|| session.session_control_info.as_ref().and_then(|c| c.ip.clone()))
                .unwrap_or_default();

            let signaling_url = streaming_conn
                .and_then(|conn| conn.resource_path.clone());

            info!("Stream server: {}, signaling: {:?}", server_ip, signaling_url);

            let mut shared = session_shared.lock();
            shared.session = Some(SessionInfo {
                session_id: session.session_id,
                server_ip,
                zone: zone.to_string(),
                state: SessionState::Ready,
                gpu_type: session.gpu_type,
                signaling_url,
            });
            return;
        }

        // Update queue position
        if queue_pos > 0 {
            let mut shared = session_shared.lock();
            if let Some(ref mut s) = shared.session {
                s.state = SessionState::InQueue {
                    position: queue_pos as u32,
                    eta_secs: (eta / 1000) as u32,
                };
            }
        }

        // Status <= 0 with error_code != 1 = failed
        if status <= 0 && session.error_code != 1 {
            let mut shared = session_shared.lock();
            shared.error = Some(format!("Session failed with error code: {}", session.error_code));
            return;
        }
    }

    let mut shared = session_shared.lock();
    shared.error = Some("Session polling timeout".to_string());
}

// ============================================================================
// Streaming Logic
// ============================================================================

async fn run_streaming_session(
    server: String,
    session_id: String,
    state: Arc<Mutex<StreamingState>>,
    mut input_rx: mpsc::Receiver<InputEvent>,
) -> anyhow::Result<()> {
    info!("Starting streaming to {} with session {}", server, session_id);

    {
        let mut s = state.lock();
        s.status = "Connecting to signaling...".to_string();
    }

    // Create signaling client
    let (sig_tx, mut sig_rx) = mpsc::channel::<SignalingEvent>(64);
    let mut signaling = GfnSignaling::new(server.clone(), session_id.clone(), sig_tx);

    // Connect
    signaling.connect().await?;
    info!("Signaling connected");

    {
        let mut s = state.lock();
        s.status = "Waiting for offer...".to_string();
    }

    // WebRTC client
    let (webrtc_tx, mut webrtc_rx) = mpsc::channel::<WebRtcEvent>(64);
    let mut webrtc_client = WebRtcClient::new(webrtc_tx);

    // Input encoder
    let mut input_encoder = InputEncoder::new();

    // H.264 decoder
    let mut decoder = openh264::decoder::Decoder::new().ok();

    loop {
        tokio::select! {
            Some(event) = sig_rx.recv() => {
                match event {
                    SignalingEvent::SdpOffer(sdp) => {
                        info!("Received SDP offer, length: {}", sdp.len());

                        // Resolve server hostname to IP
                        let mut server_ip_str = String::new();
                        let server_clone = server.clone();
                        if let Ok(addrs) = tokio::net::lookup_host(format!("{}:443", server_clone)).await {
                            for addr in addrs {
                                if addr.is_ipv4() {
                                    server_ip_str = addr.ip().to_string();
                                    break;
                                }
                            }
                        }
                        info!("Resolved server IP: {}", server_ip_str);

                        // Modify SDP to replace 0.0.0.0 with actual server IP
                        // and add ICE candidates directly to the SDP
                        let modified_sdp = if !server_ip_str.is_empty() {
                            // Extract port from first media line
                            let server_port: u16 = sdp.lines()
                                .find(|l| l.starts_with("m="))
                                .and_then(|l| l.split_whitespace().nth(1))
                                .and_then(|p| p.parse().ok())
                                .unwrap_or(47998);

                            // Replace 0.0.0.0 with actual server IP
                            let sdp_with_ip = sdp.replace("c=IN IP4 0.0.0.0", &format!("c=IN IP4 {}", server_ip_str));

                            // Add ICE candidate to each media section
                            let candidate_line = format!(
                                "a=candidate:1 1 udp 2130706431 {} {} typ host\r\n",
                                server_ip_str, server_port
                            );

                            // Insert candidate after each c= line
                            let mut result = String::new();
                            for line in sdp_with_ip.lines() {
                                result.push_str(line);
                                result.push_str("\r\n");
                                if line.starts_with("c=IN IP4") {
                                    result.push_str(&candidate_line);
                                }
                            }

                            info!("Modified SDP with server IP and candidates");
                            result
                        } else {
                            warn!("Could not resolve server IP, using original SDP");
                            sdp.clone()
                        };

                        // Log key SDP lines for debugging
                        info!("=== Modified SDP Offer ===");
                        for line in modified_sdp.lines() {
                            if line.starts_with("m=") || line.starts_with("c=") ||
                               line.starts_with("a=ice") || line.starts_with("a=candidate") {
                                info!("  {}", line);
                            }
                        }
                        info!("=== End SDP ===");

                        // For GFN ice-lite, don't use external STUN servers
                        let ice_servers = vec![];

                        match webrtc_client.handle_offer(&modified_sdp, ice_servers).await {
                            Ok(answer) => {
                                info!("Generated answer, length: {}", answer.len());

                                // Log our candidates
                                let our_candidates: Vec<&str> = answer.lines()
                                    .filter(|l| l.starts_with("a=candidate:"))
                                    .collect();
                                info!("Our ICE candidates in answer: {}", our_candidates.len());

                                let _ = signaling.send_answer(&answer, None).await;
                                let _ = webrtc_client.create_input_channel().await;
                            }
                            Err(e) => {
                                error!("Failed to handle offer: {}", e);
                            }
                        }
                    }
                    SignalingEvent::IceCandidate(c) => {
                        let _ = webrtc_client.add_ice_candidate(
                            &c.candidate,
                            c.sdp_mid.as_deref(),
                            c.sdp_mline_index.map(|i| i as u16),
                        ).await;
                    }
                    SignalingEvent::Disconnected(_) => {
                        // Signaling WebSocket closed - this is expected for ice-lite
                        // Continue running to handle WebRTC events
                        info!("Signaling disconnected (expected for ice-lite mode)");
                    }
                    _ => {}
                }
            }

            Some(event) = webrtc_rx.recv() => {
                match event {
                    WebRtcEvent::Connected => {
                        let mut s = state.lock();
                        s.connected = true;
                        s.status = "Connected".to_string();
                    }
                    WebRtcEvent::VideoFrame(data) => {
                        let mut s = state.lock();
                        s.frames_received += 1;

                        // Decode H.264
                        if let Some(ref mut dec) = decoder {
                            let data = if data.len() >= 4 && data[0..4] == [0, 0, 0, 1] {
                                data
                            } else {
                                let mut with_start = vec![0, 0, 0, 1];
                                with_start.extend_from_slice(&data);
                                with_start
                            };

                            if let Ok(Some(yuv)) = dec.decode(&data) {
                                let (w, h) = yuv.dimensions();
                                let y = yuv.y();
                                let u = yuv.u();
                                let v = yuv.v();
                                let (ys, us, vs) = yuv.strides();

                                let mut pixels = Vec::with_capacity(w * h);
                                for row in 0..h {
                                    for col in 0..w {
                                        let yi = row * ys + col;
                                        let ui = (row/2) * us + col/2;
                                        let vi = (row/2) * vs + col/2;

                                        let yv = y.get(yi).copied().unwrap_or(0) as f32;
                                        let uv = u.get(ui).copied().unwrap_or(128) as f32 - 128.0;
                                        let vv = v.get(vi).copied().unwrap_or(128) as f32 - 128.0;

                                        let r = (yv + 1.402 * vv).clamp(0.0, 255.0) as u8;
                                        let g = (yv - 0.344 * uv - 0.714 * vv).clamp(0.0, 255.0) as u8;
                                        let b = (yv + 1.772 * uv).clamp(0.0, 255.0) as u8;

                                        pixels.push(egui::Color32::from_rgb(r, g, b));
                                    }
                                }

                                s.video_frame = Some(VideoFrame {
                                    width: w as u32,
                                    height: h as u32,
                                    pixels,
                                });
                            }
                        }
                    }
                    WebRtcEvent::DataChannelMessage(_, data) => {
                        if data.len() == 4 && data[0] == 0x0e {
                            let _ = webrtc_client.send_handshake_response(data[1], data[2], data[3]).await;
                        }
                    }
                    WebRtcEvent::IceCandidate(c, mid, idx) => {
                        let _ = signaling.send_ice_candidate(&c, mid.as_deref(), idx.map(|i| i as u32)).await;
                    }
                    _ => {}
                }
            }

            Some(event) = input_rx.recv() => {
                if webrtc_client.is_handshake_complete() {
                    let encoded = input_encoder.encode(&event);
                    let _ = webrtc_client.send_input(&encoded).await;
                }
            }

            else => break,
        }
    }

    Ok(())
}

// ============================================================================
// Main
// ============================================================================

fn main() -> eframe::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 720.0])
            .with_min_inner_size([800.0, 600.0])
            .with_title("GFN Native Client"),
        ..Default::default()
    };

    eframe::run_native(
        "GFN Native Client",
        options,
        Box::new(|cc| Ok(Box::new(GfnGuiApp::new(cc)))),
    )
}
