//! Iced UI Controls
//!
//! Main UI state and message handling for iced integration.

use iced_wgpu::Renderer;
use iced_widget::{
    button, column, container, image, row, scrollable, text, text_input, slider, Space,
    checkbox, pick_list,
};
use iced_winit::core::{Alignment, Color, Element, Length, Padding, Theme};

/// Safely truncate a string to at most `max_chars` characters.
/// Returns a slice that respects UTF-8 char boundaries.
fn truncate_string(s: &str, max_chars: usize) -> &str {
    if s.chars().count() <= max_chars {
        s
    } else {
        // Find the byte index of the nth character
        s.char_indices()
            .nth(max_chars)
            .map(|(idx, _)| &s[..idx])
            .unwrap_or(s)
    }
}

use crate::app::{
    AppState, GameInfo, GameSection, GamesTab, ServerInfo, Settings, SettingChange,
    SubscriptionInfo, UiAction,
};
use crate::app::config::{
    FPS_OPTIONS, RESOLUTIONS, VideoCodec, VideoDecoderBackend, ColorQuality, GameLanguage,
};

/// Get platform-specific decoder options
fn get_platform_decoder_options() -> Vec<VideoDecoderBackend> {
    #[cfg(target_os = "windows")]
    {
        vec![
            VideoDecoderBackend::Auto,
            VideoDecoderBackend::Dxva,         // D3D11 via GStreamer
            VideoDecoderBackend::NativeDxva,   // Native D3D11 (HEVC only)
            VideoDecoderBackend::Cuvid,        // NVIDIA NVDEC
            VideoDecoderBackend::Qsv,          // Intel QuickSync
            VideoDecoderBackend::Software,
        ]
    }
    #[cfg(target_os = "macos")]
    {
        vec![
            VideoDecoderBackend::Auto,
            VideoDecoderBackend::VideoToolbox,
            VideoDecoderBackend::Software,
        ]
    }
    #[cfg(target_os = "linux")]
    {
        vec![
            VideoDecoderBackend::Auto,
            VideoDecoderBackend::VulkanVideo,  // GStreamer HW
            VideoDecoderBackend::Vaapi,        // VA-API
            VideoDecoderBackend::Software,
        ]
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        vec![
            VideoDecoderBackend::Auto,
            VideoDecoderBackend::Software,
        ]
    }
}

/// Messages for the iced UI
#[derive(Debug, Clone)]
pub enum Message {
    // Login screen
    LoginWithNvidia,
    LoginWithGoogle,
    SelectProvider(usize),
    
    // Games screen
    SearchChanged(String),
    SearchSubmit,
    TabSelected(GamesTab),
    GameClicked(GameInfo),
    GameLaunch(GameInfo),
    GamePopupClose,
    VariantSelected(usize),
    
    // Settings
    OpenSettings,
    CloseSettings,
    BitrateChanged(f32),
    ResolutionChanged(String),
    FpsChanged(u32),
    CodecChanged(String),
    DecoderChanged(String),
    ColorQualityChanged(String),
    HdrChanged(bool),
    LowLatencyChanged(bool),
    BorderlessChanged(bool),
    FullscreenChanged(bool),
    VsyncChanged(bool),
    SurroundChanged(bool),
    ClipboardPasteChanged(bool),
    GameLanguageChanged(String),
    ServerSelected(usize),
    AutoServerChanged(bool),
    StartPingTest,
    ResetSettings,
    
    // Session
    CancelSession,
    
    // Server selection (free tier)
    OpenServerSelection,
    CloseServerSelection,
    QueueServerSelected(Option<String>),
    LaunchWithServer,
    
    // Dialogs
    CloseSessionConflict,
    ResumeSession,
    TerminateAndLaunch,
    CloseAV1Warning,
    CloseAllianceWarning,
    CloseWelcome,
    
    // General
    RefreshGames,
    RefreshQueueTimes,
    Logout,
    ToggleStats,
    
    // Internal
    Tick,
    ImageLoaded(String, Vec<u8>),
}

/// Controls state for the iced UI
pub struct Controls {
    // Login state
    pub login_error: Option<String>,
    pub selected_provider: usize,
    
    // Games state
    pub search_query: String,
    pub current_tab: GamesTab,
    pub selected_game_popup: Option<GameInfo>,
    pub show_settings: bool,
    
    // Server selection (free tier)
    pub show_server_selection: bool,
    pub pending_game: Option<GameInfo>,
    pub selected_queue_server: Option<String>,
    
    // Dialogs
    pub show_session_conflict: bool,
    pub show_av1_warning: bool,
    pub show_alliance_warning: bool,
    pub show_welcome: bool,
    
    // Image cache for game thumbnails
    pub loaded_images: std::collections::HashMap<String, iced_widget::image::Handle>,
    
    // Settings cache
    pub settings: Settings,
    pub bitrate_value: f32,
}

impl Default for Controls {
    fn default() -> Self {
        Self::new()
    }
}

impl Controls {
    pub fn new() -> Self {
        Self {
            login_error: None,
            selected_provider: 0,
            search_query: String::new(),
            current_tab: GamesTab::Home,
            selected_game_popup: None,
            show_settings: false,
            show_server_selection: false,
            pending_game: None,
            selected_queue_server: None,
            show_session_conflict: false,
            show_av1_warning: false,
            show_alliance_warning: false,
            show_welcome: false,
            loaded_images: std::collections::HashMap::new(),
            settings: Settings::default(),
            bitrate_value: 50.0,
        }
    }
    
    /// Sync state from app
    pub fn sync_from_app(
        &mut self,
        settings: &Settings,
        show_settings: bool,
        selected_game_popup: Option<&GameInfo>,
        show_session_conflict: bool,
        show_av1_warning: bool,
        show_alliance_warning: bool,
        show_welcome: bool,
    ) {
        self.settings = settings.clone();
        self.bitrate_value = settings.max_bitrate_mbps as f32;
        self.show_settings = show_settings;
        self.selected_game_popup = selected_game_popup.cloned();
        self.show_session_conflict = show_session_conflict;
        self.show_av1_warning = show_av1_warning;
        self.show_alliance_warning = show_alliance_warning;
        self.show_welcome = show_welcome;
    }
    
    /// Handle a message and return any UI actions needed
    pub fn update(&mut self, message: Message) -> Option<UiAction> {
        match message {
            // Login
            Message::LoginWithNvidia | Message::LoginWithGoogle => {
                Some(UiAction::StartLogin)
            }
            Message::SelectProvider(idx) => {
                self.selected_provider = idx;
                Some(UiAction::SelectProvider(idx))
            }
            
            // Games
            Message::SearchChanged(query) => {
                self.search_query = query.clone();
                Some(UiAction::UpdateSearch(query))
            }
            Message::SearchSubmit => {
                Some(UiAction::UpdateSearch(self.search_query.clone()))
            }
            Message::TabSelected(tab) => {
                self.current_tab = tab;
                Some(UiAction::SwitchTab(tab))
            }
            Message::GameClicked(game) => {
                self.selected_game_popup = Some(game.clone());
                Some(UiAction::OpenGamePopup(game))
            }
            Message::GameLaunch(game) => {
                self.selected_game_popup = None;
                Some(UiAction::LaunchGameDirect(game))
            }
            Message::GamePopupClose => {
                self.selected_game_popup = None;
                Some(UiAction::CloseGamePopup)
            }
            Message::VariantSelected(idx) => {
                Some(UiAction::SelectVariant(idx))
            }
            
            // Settings
            Message::OpenSettings => {
                self.show_settings = true;
                Some(UiAction::ToggleSettingsModal)
            }
            Message::CloseSettings => {
                self.show_settings = false;
                Some(UiAction::ToggleSettingsModal)
            }
            Message::BitrateChanged(value) => {
                self.bitrate_value = value;
                self.settings.max_bitrate_mbps = value as u32;
                Some(UiAction::UpdateSetting(SettingChange::MaxBitrate(value as u32)))
            }
            Message::ResolutionChanged(res) => {
                self.settings.resolution = res.clone();
                Some(UiAction::UpdateSetting(SettingChange::Resolution(res)))
            }
            Message::FpsChanged(fps) => {
                self.settings.fps = fps;
                Some(UiAction::UpdateSetting(SettingChange::Fps(fps)))
            }
            Message::CodecChanged(codec_str) => {
                let codec = match codec_str.as_str() {
                    "H265" => VideoCodec::H265,
                    "AV1" => VideoCodec::AV1,
                    _ => VideoCodec::H264,
                };
                self.settings.codec = codec;
                Some(UiAction::UpdateSetting(SettingChange::Codec(codec)))
            }
            Message::DecoderChanged(decoder_str) => {
                let decoder = VideoDecoderBackend::all()
                    .iter()
                    .find(|d| d.as_str() == decoder_str)
                    .copied()
                    .unwrap_or(VideoDecoderBackend::Auto);
                self.settings.decoder_backend = decoder;
                Some(UiAction::UpdateSetting(SettingChange::DecoderBackend(decoder)))
            }
            Message::ColorQualityChanged(color_str) => {
                let color = ColorQuality::all()
                    .iter()
                    .find(|c| c.display_name() == color_str)
                    .copied()
                    .unwrap_or(ColorQuality::Bit10Yuv420);
                self.settings.color_quality = color;
                Some(UiAction::UpdateSetting(SettingChange::ColorQuality(color)))
            }
            Message::HdrChanged(hdr) => {
                self.settings.hdr_enabled = hdr;
                Some(UiAction::UpdateSetting(SettingChange::Hdr(hdr)))
            }
            Message::LowLatencyChanged(low_latency) => {
                self.settings.low_latency_mode = low_latency;
                Some(UiAction::UpdateSetting(SettingChange::LowLatency(low_latency)))
            }
            Message::BorderlessChanged(borderless) => {
                self.settings.borderless = borderless;
                Some(UiAction::UpdateSetting(SettingChange::Borderless(borderless)))
            }
            Message::FullscreenChanged(fs) => {
                self.settings.fullscreen = fs;
                Some(UiAction::UpdateSetting(SettingChange::Fullscreen(fs)))
            }
            Message::VsyncChanged(vs) => {
                self.settings.vsync = vs;
                Some(UiAction::UpdateSetting(SettingChange::VSync(vs)))
            }
            Message::SurroundChanged(surround) => {
                self.settings.surround = surround;
                // No specific UiAction for surround yet, just update local settings
                None
            }
            Message::ClipboardPasteChanged(enabled) => {
                self.settings.clipboard_paste_enabled = enabled;
                Some(UiAction::UpdateSetting(SettingChange::ClipboardPasteEnabled(enabled)))
            }
            Message::GameLanguageChanged(lang_str) => {
                let lang = GameLanguage::all()
                    .iter()
                    .find(|l| l.display_name() == lang_str)
                    .copied()
                    .unwrap_or(GameLanguage::EnglishUS);
                self.settings.game_language = lang;
                Some(UiAction::UpdateSetting(SettingChange::GameLanguage(lang)))
            }
            Message::ServerSelected(idx) => {
                Some(UiAction::SelectServer(idx))
            }
            Message::AutoServerChanged(auto) => {
                Some(UiAction::SetAutoServerSelection(auto))
            }
            Message::StartPingTest => {
                Some(UiAction::StartPingTest)
            }
            Message::ResetSettings => {
                Some(UiAction::ResetSettings)
            }
            
            // Session
            Message::CancelSession => Some(UiAction::StopStreaming),
            
            // Server selection
            Message::OpenServerSelection => {
                self.show_server_selection = true;
                None
            }
            Message::CloseServerSelection => {
                self.show_server_selection = false;
                self.pending_game = None;
                Some(UiAction::CloseServerSelection)
            }
            Message::QueueServerSelected(server) => {
                self.selected_queue_server = server.clone();
                Some(UiAction::SelectQueueServer(server))
            }
            Message::LaunchWithServer => {
                if let Some(game) = self.pending_game.take() {
                    self.show_server_selection = false;
                    Some(UiAction::LaunchWithServer(game, self.selected_queue_server.clone()))
                } else {
                    None
                }
            }
            
            // Dialogs
            Message::CloseSessionConflict => {
                self.show_session_conflict = false;
                Some(UiAction::CloseSessionConflict)
            }
            Message::ResumeSession => {
                self.show_session_conflict = false;
                // TODO: Need session info here
                None
            }
            Message::TerminateAndLaunch => {
                self.show_session_conflict = false;
                // TODO: Need session info here
                None
            }
            Message::CloseAV1Warning => {
                self.show_av1_warning = false;
                Some(UiAction::CloseAV1Warning)
            }
            Message::CloseAllianceWarning => {
                self.show_alliance_warning = false;
                Some(UiAction::CloseAllianceWarning)
            }
            Message::CloseWelcome => {
                self.show_welcome = false;
                Some(UiAction::CloseWelcomePopup)
            }
            
            // General
            Message::RefreshGames => Some(UiAction::RefreshGames),
            Message::RefreshQueueTimes => Some(UiAction::RefreshQueueTimes),
            Message::Logout => Some(UiAction::Logout),
            Message::ToggleStats => Some(UiAction::ToggleStats),
            
            // Internal
            Message::Tick => None,
            Message::ImageLoaded(url, data) => {
                let handle = iced_widget::image::Handle::from_bytes(data);
                self.loaded_images.insert(url, handle);
                None
            }
        }
    }
    
    /// Build the main view
    pub fn view<'a>(
        &'a self,
        app_state: AppState,
        games: &'a [GameInfo],
        library_games: &'a [GameInfo],
        game_sections: &'a [GameSection],
        status_message: &'a str,
        user_name: Option<&'a str>,
        servers: &'a [ServerInfo],
        selected_server_index: usize,
        subscription: Option<&'a SubscriptionInfo>,
    ) -> Element<'a, Message, Theme, Renderer> {
        let main_content: Element<'a, Message, Theme, Renderer> = match app_state {
            AppState::Login => self.view_login(status_message),
            AppState::Games => self.view_games(games, library_games, game_sections, user_name, subscription, servers, selected_server_index),
            AppState::Session => self.view_session(status_message),
            AppState::Streaming => {
                // Minimal UI during streaming - just stats overlay if enabled
                container(Space::new().width(Length::Fill).height(Length::Fill))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
            }
        };
        
        // Layer modals on top
        if self.show_settings {
            self.view_with_settings_modal(main_content, servers, selected_server_index, subscription)
        } else if let Some(ref game) = self.selected_game_popup {
            self.view_with_game_popup(main_content, game)
        } else {
            main_content
        }
    }
    
    fn view_login<'a>(&'a self, status_message: &'a str) -> Element<'a, Message, Theme, Renderer> {
        let title = text("OpenNOW")
            .size(48)
            .color(Color::from_rgb(0.467, 0.784, 0.196)); // GFN green
        
        let subtitle = text("Open Source GeForce NOW Client")
            .size(16)
            .color(Color::from_rgb(0.7, 0.7, 0.7));
        
        let nvidia_btn = button(
            container(text("Sign in with NVIDIA").size(16))
                .padding(Padding::from([12, 32]))
        )
        .style(|theme: &Theme, status| {
            let palette = theme.palette();
            button::Style {
                background: Some(Color::from_rgb(0.467, 0.784, 0.196).into()),
                text_color: Color::BLACK,
                border: iced_core::Border::default().rounded(8),
                ..button::Style::default()
            }
        })
        .on_press(Message::LoginWithNvidia);
        
        let google_btn = button(
            container(text("Sign in with Google").size(16))
                .padding(Padding::from([12, 32]))
        )
        .style(|theme: &Theme, status| {
            button::Style {
                background: Some(Color::from_rgb(0.2, 0.2, 0.25).into()),
                text_color: Color::WHITE,
                border: iced_core::Border::default().rounded(8),
                ..button::Style::default()
            }
        })
        .on_press(Message::LoginWithGoogle);
        
        let status: Element<'a, Message, Theme, Renderer> = if !status_message.is_empty() {
            text(status_message)
                .size(14)
                .color(Color::from_rgb(0.7, 0.7, 0.7))
                .into()
        } else {
            Space::new().into()
        };
        
        let error: Element<'a, Message, Theme, Renderer> = if let Some(ref err) = self.login_error {
            text(err)
                .size(14)
                .color(Color::from_rgb(0.9, 0.3, 0.3))
                .into()
        } else {
            Space::new().into()
        };
        
        let content = column![
            Space::new().height(Length::FillPortion(1)),
            title,
            Space::new().height(8),
            subtitle,
            Space::new().height(48),
            nvidia_btn,
            Space::new().height(16),
            google_btn,
            Space::new().height(32),
            status,
            error,
            Space::new().height(Length::FillPortion(1)),
        ]
        .align_x(Alignment::Center);
        
        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(|_| container::Style {
                background: Some(Color::from_rgb(0.078, 0.078, 0.118).into()),
                ..container::Style::default()
            })
            .into()
    }
    
    fn view_games<'a>(
        &'a self,
        games: &'a [GameInfo],
        library_games: &'a [GameInfo],
        game_sections: &'a [GameSection],
        user_name: Option<&'a str>,
        subscription: Option<&'a SubscriptionInfo>,
        servers: &'a [ServerInfo],
        selected_server_index: usize,
    ) -> Element<'a, Message, Theme, Renderer> {
        // Combined header with tabs in single navbar
        let header = self.view_header_with_tabs(user_name);
        
        // Content based on tab - use correct games list for each tab
        let content: Element<'a, Message, Theme, Renderer> = match self.current_tab {
            GamesTab::Home => self.view_home_sections(game_sections),
            GamesTab::AllGames => self.view_games_grid(games),
            GamesTab::MyLibrary => self.view_games_grid(library_games),
            GamesTab::QueueTimes => self.view_queue_times(),
        };
        
        // Bottom bar with subscription info
        let bottom_bar = self.view_bottom_bar(subscription, servers, selected_server_index);
        
        let main = column![
            header,
            scrollable(content).height(Length::Fill),
            bottom_bar,
        ];
        
        container(main)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(Color::from_rgb(0.078, 0.078, 0.118).into()),
                ..container::Style::default()
            })
            .into()
    }
    
    fn view_header_with_tabs<'a>(
        &'a self,
        user_name: Option<&'a str>,
    ) -> Element<'a, Message, Theme, Renderer> {
        let logo = text("OpenNOW")
            .size(24)
            .color(Color::from_rgb(0.463, 0.725, 0.0)); // GFN green
        
        // Tab buttons (in header)
        let active_bg = Color::from_rgb(0.463, 0.725, 0.0); // GFN green
        let inactive_bg = Color::from_rgb(0.196, 0.196, 0.255); // rgb(50, 50, 65)
        
        let home_selected = matches!(self.current_tab, GamesTab::Home);
        let all_selected = matches!(self.current_tab, GamesTab::AllGames);
        let library_selected = matches!(self.current_tab, GamesTab::MyLibrary);
        let queue_selected = matches!(self.current_tab, GamesTab::QueueTimes);
        
        let home_btn = button(text("Home").size(12))
            .padding(Padding::from([6, 12]))
            .on_press(Message::TabSelected(GamesTab::Home))
            .style(move |_, _| button::Style {
                background: Some(if home_selected { active_bg } else { inactive_bg }.into()),
                text_color: Color::WHITE,
                border: iced_core::Border::default().rounded(6),
                ..button::Style::default()
            });
        
        let all_btn = button(text("All Games").size(12))
            .padding(Padding::from([6, 12]))
            .on_press(Message::TabSelected(GamesTab::AllGames))
            .style(move |_, _| button::Style {
                background: Some(if all_selected { active_bg } else { inactive_bg }.into()),
                text_color: Color::WHITE,
                border: iced_core::Border::default().rounded(6),
                ..button::Style::default()
            });
        
        let library_btn = button(text("My Library").size(12))
            .padding(Padding::from([6, 12]))
            .on_press(Message::TabSelected(GamesTab::MyLibrary))
            .style(move |_, _| button::Style {
                background: Some(if library_selected { active_bg } else { inactive_bg }.into()),
                text_color: Color::WHITE,
                border: iced_core::Border::default().rounded(6),
                ..button::Style::default()
            });
        
        let queue_btn = button(text("Queue").size(12))
            .padding(Padding::from([6, 12]))
            .on_press(Message::TabSelected(GamesTab::QueueTimes))
            .style(move |_, _| button::Style {
                background: Some(if queue_selected { active_bg } else { inactive_bg }.into()),
                text_color: Color::WHITE,
                border: iced_core::Border::default().rounded(6),
                ..button::Style::default()
            });
        
        let tabs = row![home_btn, all_btn, library_btn, queue_btn].spacing(6);
        
        // Search box with icon-style background
        let search = text_input("🔍 Search...", &self.search_query)
            .on_input(Message::SearchChanged)
            .on_submit(Message::SearchSubmit)
            .padding(8)
            .width(180)
            .style(|_theme: &Theme, _status| {
                text_input::Style {
                    background: Color::from_rgb(0.137, 0.137, 0.176).into(), // rgb(35, 35, 45)
                    border: iced_core::Border::default()
                        .rounded(6)
                        .color(Color::from_rgb(0.235, 0.235, 0.294)) // rgb(60, 60, 75)
                        .width(1.0),
                    icon: Color::from_rgb(0.47, 0.47, 0.55),
                    placeholder: Color::from_rgb(0.47, 0.47, 0.55),
                    value: Color::WHITE,
                    selection: Color::from_rgb(0.463, 0.725, 0.0),
                }
            });
        
        let user_text = text(user_name.unwrap_or("User"))
            .size(13)
            .color(Color::WHITE);
        
        let settings_btn = button(text("⚙").size(14))
            .padding(Padding::from([6, 8]))
            .on_press(Message::OpenSettings)
            .style(|_, _| button::Style {
                background: Some(Color::from_rgb(0.196, 0.196, 0.255).into()),
                text_color: Color::WHITE,
                border: iced_core::Border::default().rounded(6),
                ..button::Style::default()
            });
        
        let logout_btn = button(text("Logout").size(12))
            .padding(Padding::from([6, 12]))
            .on_press(Message::Logout)
            .style(|_, _| button::Style {
                background: Some(Color::from_rgb(0.196, 0.196, 0.255).into()),
                text_color: Color::WHITE,
                border: iced_core::Border::default().rounded(6),
                ..button::Style::default()
            });
        
        // Combined header row: Logo | Tabs | Search | User | Settings | Logout
        let header_row = row![
            Space::new().width(12),
            logo,
            Space::new().width(16),
            tabs,
            Space::new().width(Length::Fill),
            search,
            Space::new().width(12),
            user_text,
            Space::new().width(10),
            settings_btn,
            Space::new().width(8),
            logout_btn,
            Space::new().width(12),
        ]
        .align_y(Alignment::Center);
        
        container(header_row)
            .width(Length::Fill)
            .padding(Padding::from([10, 0]))
            .style(|_| container::Style {
                background: Some(Color::from_rgb(0.086, 0.086, 0.118).into()), // rgb(22, 22, 30)
                ..container::Style::default()
            })
            .into()
    }
    
    fn view_bottom_bar<'a>(
        &'a self,
        subscription: Option<&'a SubscriptionInfo>,
        servers: &'a [ServerInfo],
        selected_server_index: usize,
    ) -> Element<'a, Message, Theme, Renderer> {
        // Left side: subscription info
        let left_content: Element<'a, Message, Theme, Renderer> = if let Some(sub) = subscription {
            // Tier badge colors
            let (tier_bg, tier_fg) = match sub.membership_tier.as_str() {
                "ULTIMATE" => (
                    Color::from_rgb(0.314, 0.235, 0.039), // rgb(80, 60, 10)
                    Color::from_rgb(1.0, 0.843, 0.0),     // rgb(255, 215, 0) gold
                ),
                "PERFORMANCE" | "PRIORITY" => (
                    Color::from_rgb(0.275, 0.157, 0.078), // rgb(70, 40, 20)
                    Color::from_rgb(0.804, 0.686, 0.584), // rgb(205, 175, 149)
                ),
                _ => (
                    Color::from_rgb(0.176, 0.176, 0.176), // rgb(45, 45, 45)
                    Color::from_rgb(0.706, 0.706, 0.706), // rgb(180, 180, 180)
                ),
            };
            
            // Tier badge
            let tier_badge = container(
                text(&sub.membership_tier).size(11).color(tier_fg)
            )
            .padding(Padding::from([4, 8]))
            .style(move |_| container::Style {
                background: Some(tier_bg.into()),
                border: iced_core::Border::default().rounded(4),
                ..container::Style::default()
            });
            
            // Hours display
            let hours_display: Element<'a, Message, Theme, Renderer> = if sub.is_unlimited {
                row![
                    text("⏱").size(14).color(Color::from_rgb(0.5, 0.5, 0.5)),
                    Space::new().width(5),
                    text("∞").size(15).color(Color::from_rgb(0.463, 0.725, 0.0)),
                ].align_y(Alignment::Center).into()
            } else {
                let hours_color = if sub.remaining_hours > 5.0 {
                    Color::from_rgb(0.463, 0.725, 0.0) // green
                } else if sub.remaining_hours > 1.0 {
                    Color::from_rgb(1.0, 0.784, 0.196) // yellow
                } else {
                    Color::from_rgb(1.0, 0.314, 0.314) // red
                };
                
                row![
                    text("⏱").size(14).color(Color::from_rgb(0.5, 0.5, 0.5)),
                    Space::new().width(5),
                    text(format!("{:.1}h", sub.remaining_hours)).size(13).color(hours_color),
                    text(format!(" / {:.0}h", sub.total_hours)).size(12).color(Color::from_rgb(0.5, 0.5, 0.5)),
                ].align_y(Alignment::Center).into()
            };
            
            // Storage display (if available)
            let storage_display: Element<'a, Message, Theme, Renderer> = if sub.has_persistent_storage {
                if let Some(storage_gb) = sub.storage_size_gb {
                    row![
                        text("💾").size(14).color(Color::from_rgb(0.5, 0.5, 0.5)),
                        Space::new().width(5),
                        text(format!("{} GB", storage_gb)).size(13).color(Color::from_rgb(0.392, 0.706, 1.0)),
                    ].align_y(Alignment::Center).into()
                } else {
                    Space::new().into()
                }
            } else {
                Space::new().into()
            };
            
            row![
                tier_badge,
                Space::new().width(20),
                hours_display,
                Space::new().width(20),
                storage_display,
            ]
            .align_y(Alignment::Center)
            .into()
        } else {
            text("Loading subscription info...")
                .size(12)
                .color(Color::from_rgb(0.5, 0.5, 0.5))
                .into()
        };
        
        // Right side: server info
        let server_display: Element<'a, Message, Theme, Renderer> = if let Some(server) = servers.get(selected_server_index) {
            let ping_text = server.ping_ms
                .map(|p| format!(" ({}ms)", p))
                .unwrap_or_default();
            
            text(format!("🌐 {}{}", server.name, ping_text))
                .size(12)
                .color(Color::from_rgb(0.392, 0.706, 1.0)) // rgb(100, 180, 255)
                .into()
        } else {
            text("🌐 Auto (waiting for ping)")
                .size(12)
                .color(Color::from_rgb(0.5, 0.5, 0.5))
                .into()
        };
        
        let bar_row = row![
            Space::new().width(15),
            left_content,
            Space::new().width(Length::Fill),
            server_display,
            Space::new().width(15),
        ]
        .align_y(Alignment::Center);
        
        container(bar_row)
            .width(Length::Fill)
            .padding(Padding::from([8, 0]))
            .style(|_| container::Style {
                background: Some(Color::from_rgb(0.086, 0.086, 0.118).into()), // rgb(22, 22, 30)
                ..container::Style::default()
            })
            .into()
    }
    
    
    fn view_home_sections<'a>(&'a self, sections: &'a [GameSection]) -> Element<'a, Message, Theme, Renderer> {
        // Show loading message if no sections yet
        if sections.is_empty() {
            return container(
                text("Loading game sections...")
                    .size(16)
                    .color(Color::from_rgb(0.5, 0.5, 0.5))
            )
            .width(Length::Fill)
            .height(300)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into();
        }
        
        let mut content = column![].spacing(24).padding(16);
        
        for section in sections {
            if section.games.is_empty() {
                continue;
            }
            
            let title = text(&section.title)
                .size(20)
                .color(Color::WHITE);
            
            let games_row = self.view_games_row(&section.games);
            
            content = content.push(column![
                title,
                Space::new().height(12),
                scrollable(games_row).direction(scrollable::Direction::Horizontal(
                    scrollable::Scrollbar::default()
                )),
            ]);
        }
        
        content.into()
    }
    
    fn view_games_row<'a>(&'a self, games: &'a [GameInfo]) -> Element<'a, Message, Theme, Renderer> {
        let mut row_content = row![].spacing(16);
        
        for game in games.iter().take(10) {
            row_content = row_content.push(self.view_game_card(game));
        }
        
        row_content.into()
    }
    
    fn view_games_grid<'a>(&'a self, games: &'a [GameInfo]) -> Element<'a, Message, Theme, Renderer> {
        // Show loading message if no games yet
        if games.is_empty() {
            return container(
                text("Loading games...")
                    .size(16)
                    .color(Color::from_rgb(0.5, 0.5, 0.5))
            )
            .width(Length::Fill)
            .height(300)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into();
        }
        
        // Filter games based on search
        let filtered: Vec<&GameInfo> = if self.search_query.is_empty() {
            games.iter().collect()
        } else {
            let query = self.search_query.to_lowercase();
            games.iter()
                .filter(|g| g.title.to_lowercase().contains(&query))
                .collect()
        };
        
        // Show no results message if search found nothing
        if filtered.is_empty() {
            return container(
                text(format!("No games found for '{}'", self.search_query))
                    .size(16)
                    .color(Color::from_rgb(0.5, 0.5, 0.5))
            )
            .width(Length::Fill)
            .height(300)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into();
        }
        
        let mut rows: Vec<Element<'a, Message, Theme, Renderer>> = Vec::new();
        
        for chunk in filtered.chunks(6) {
            let mut row_items: Vec<Element<'a, Message, Theme, Renderer>> = Vec::new();
            
            for game in chunk {
                row_items.push(self.view_game_card(game));
            }
            
            // Fill remaining slots
            while row_items.len() < 6 {
                row_items.push(Space::new().width(180).into());
            }
            
            rows.push(
                iced_widget::Row::with_children(row_items)
                    .spacing(16)
                    .into()
            );
        }
        
        iced_widget::Column::with_children(rows)
            .spacing(16)
            .padding(16)
            .into()
    }
    
    fn view_game_card<'a>(&'a self, game: &'a GameInfo) -> Element<'a, Message, Theme, Renderer> {
        let game_clone = game.clone();
        
        // Card background color - more visible against dark background
        let card_bg = Color::from_rgb(0.157, 0.157, 0.196); // rgb(40, 40, 50) - lighter than bg
        let card_border = Color::from_rgb(0.235, 0.235, 0.275); // rgb(60, 60, 70)
        
        // Image or placeholder
        let img: Element<'a, Message, Theme, Renderer> = if let Some(ref url) = game.image_url {
            if let Some(handle) = self.loaded_images.get(url) {
                container(
                    image(handle.clone())
                        .width(180)
                        .height(240)
                        .content_fit(iced_core::ContentFit::Cover)
                )
                .width(180)
                .height(240)
                .clip(true)
                .style(move |_| container::Style {
                    background: Some(card_bg.into()),
                    border: iced_core::Border::default().rounded(8).color(card_border).width(1.0),
                    ..container::Style::default()
                })
                .into()
            } else {
                // Placeholder while loading - show title text
                container(
                    column![
                        Space::new().height(Length::FillPortion(1)),
                        text(truncate_string(&game.title, 25))
                            .size(11)
                            .color(Color::from_rgb(0.6, 0.6, 0.6))
                            .width(Length::Fill)
                            .align_x(Alignment::Center),
                        Space::new().height(Length::FillPortion(1)),
                    ]
                    .width(Length::Fill)
                    .align_x(Alignment::Center)
                )
                .width(180)
                .height(240)
                .padding(8)
                .style(move |_| container::Style {
                    background: Some(card_bg.into()),
                    border: iced_core::Border::default().rounded(8).color(card_border).width(1.0),
                    ..container::Style::default()
                })
                .into()
            }
        } else {
            // No image URL - show placeholder with title
            container(
                column![
                    Space::new().height(Length::FillPortion(1)),
                    text(truncate_string(&game.title, 25))
                        .size(11)
                        .color(Color::from_rgb(0.6, 0.6, 0.6))
                        .width(Length::Fill)
                        .align_x(Alignment::Center),
                    Space::new().height(Length::FillPortion(1)),
                ]
                .width(Length::Fill)
                .align_x(Alignment::Center)
            )
            .width(180)
            .height(240)
            .padding(8)
            .style(move |_| container::Style {
                background: Some(card_bg.into()),
                border: iced_core::Border::default().rounded(8).color(card_border).width(1.0),
                ..container::Style::default()
            })
            .into()
        };
        
        let title = text(&game.title)
            .size(12)
            .color(Color::WHITE)
            .width(180);
        
        let store = text(&game.store)
            .size(10)
            .color(Color::from_rgb(0.5, 0.5, 0.5));
        
        let card = column![
            img,
            Space::new().height(8),
            title,
            store,
        ]
        .width(180);
        
        button(card)
            .padding(0)
            .on_press(Message::GameClicked(game_clone))
            .style(|_, _| button::Style {
                background: None,
                text_color: Color::WHITE,
                border: iced_core::Border::default(),
                ..button::Style::default()
            })
            .into()
    }
    
    fn view_queue_times<'a>(&'a self) -> Element<'a, Message, Theme, Renderer> {
        // Placeholder for queue times view
        container(text("Queue Times - Coming Soon").color(Color::WHITE))
            .width(Length::Fill)
            .height(300)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
    }
    
    fn view_session<'a>(&'a self, status_message: &'a str) -> Element<'a, Message, Theme, Renderer> {
        let spinner = text("⟳")
            .size(64)
            .color(Color::from_rgb(0.467, 0.784, 0.196));
        
        let status = text(status_message)
            .size(18)
            .color(Color::WHITE);
        
        let cancel_btn = button(
            container(text("Cancel").size(16))
                .padding(Padding::from([12, 32]))
        )
        .on_press(Message::CancelSession)
        .style(|_, _| button::Style {
            background: Some(Color::from_rgb(0.3, 0.15, 0.15).into()),
            text_color: Color::WHITE,
            border: iced_core::Border::default().rounded(8),
            ..button::Style::default()
        });
        
        let content = column![
            Space::new().height(Length::FillPortion(1)),
            spinner,
            Space::new().height(24),
            status,
            Space::new().height(32),
            cancel_btn,
            Space::new().height(Length::FillPortion(1)),
        ]
        .align_x(Alignment::Center);
        
        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(|_| container::Style {
                background: Some(Color::from_rgb(0.078, 0.078, 0.118).into()),
                ..container::Style::default()
            })
            .into()
    }
    
    fn view_with_settings_modal<'a>(
        &'a self,
        background: Element<'a, Message, Theme, Renderer>,
        servers: &'a [ServerInfo],
        selected_server_index: usize,
        subscription: Option<&'a SubscriptionInfo>,
    ) -> Element<'a, Message, Theme, Renderer> {
        let modal = self.view_settings_modal(servers, selected_server_index, subscription);
        
        // Overlay modal on background
        iced_widget::Stack::with_children(vec![
            background,
            container(modal)
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .style(|_| container::Style {
                    background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.7).into()),
                    ..container::Style::default()
                })
                .into(),
        ])
        .into()
    }
    
    fn view_settings_modal<'a>(
        &'a self,
        servers: &'a [ServerInfo],
        selected_server_index: usize,
        _subscription: Option<&'a SubscriptionInfo>,
    ) -> Element<'a, Message, Theme, Renderer> {
        let section_title_color = Color::from_rgb(0.467, 0.784, 0.196);
        let label_width = 130;
        
        let title = row![
            text("Settings").size(24).color(Color::WHITE),
            Space::new().width(Length::Fill),
            button(text("✕").size(18))
                .on_press(Message::CloseSettings)
                .padding(8)
                .style(|_, _| button::Style {
                    background: None,
                    text_color: Color::WHITE,
                    ..button::Style::default()
                }),
        ];
        
        // === VIDEO SETTINGS ===
        let codec_options: Vec<String> = VideoCodec::all().iter().map(|c| c.as_str().to_string()).collect();
        let current_codec = self.settings.codec.as_str().to_string();
        
        // Get platform-specific decoder options
        let decoder_options: Vec<String> = get_platform_decoder_options()
            .iter()
            .map(|d| d.as_str().to_string())
            .collect();
        let current_decoder = self.settings.decoder_backend.as_str().to_string();
        
        let color_options: Vec<String> = ColorQuality::all().iter().map(|c| c.display_name().to_string()).collect();
        let current_color = self.settings.color_quality.display_name().to_string();
        
        let video_section = column![
            text("Video").size(16).color(section_title_color),
            Space::new().height(10),
            
            // Bitrate
            row![
                text("Max Bitrate").size(13).color(Color::WHITE).width(label_width),
                slider(10.0..=200.0, self.bitrate_value, Message::BitrateChanged)
                    .step(5.0)
                    .width(180),
                Space::new().width(8),
                text(format!("{} Mbps", self.settings.max_bitrate_mbps)).size(12).color(Color::WHITE),
            ].align_y(Alignment::Center),
            Space::new().height(8),
            
            // Resolution  
            row![
                text("Resolution").size(13).color(Color::WHITE).width(label_width),
                pick_list(
                    RESOLUTIONS.iter().map(|(r, _)| r.to_string()).collect::<Vec<_>>(),
                    Some(self.settings.resolution.clone()),
                    Message::ResolutionChanged,
                ).width(180).text_size(12),
            ].align_y(Alignment::Center),
            Space::new().height(8),
            
            // FPS
            row![
                text("Frame Rate").size(13).color(Color::WHITE).width(label_width),
                pick_list(FPS_OPTIONS.to_vec(), Some(self.settings.fps), Message::FpsChanged)
                    .width(180).text_size(12),
            ].align_y(Alignment::Center),
            Space::new().height(8),
            
            // Codec
            row![
                text("Codec").size(13).color(Color::WHITE).width(label_width),
                pick_list(codec_options, Some(current_codec), Message::CodecChanged)
                    .width(180).text_size(12),
            ].align_y(Alignment::Center),
            Space::new().height(8),
            
            // Decoder
            row![
                text("Decoder").size(13).color(Color::WHITE).width(label_width),
                pick_list(decoder_options, Some(current_decoder), Message::DecoderChanged)
                    .width(180).text_size(12),
            ].align_y(Alignment::Center),
            Space::new().height(8),
            
            // Color Quality
            row![
                text("Color Quality").size(13).color(Color::WHITE).width(label_width),
                pick_list(color_options, Some(current_color), Message::ColorQualityChanged)
                    .width(180).text_size(12),
            ].align_y(Alignment::Center),
            Space::new().height(8),
            
            // HDR
            checkbox(self.settings.hdr_enabled)
                .label("HDR (requires 10-bit)")
                .on_toggle(Message::HdrChanged)
                .text_size(13)
                .style(|theme, status| {
                    let mut style = checkbox::primary(theme, status);
                    style.text_color = Some(Color::WHITE);
                    style
                }),
        ];
        
        // === DISPLAY SETTINGS ===
        let display_section = column![
            Space::new().height(14),
            text("Display").size(16).color(section_title_color),
            Space::new().height(10),
            
            checkbox(self.settings.fullscreen)
                .label("Fullscreen")
                .on_toggle(Message::FullscreenChanged)
                .text_size(13)
                .style(|theme, status| {
                    let mut style = checkbox::primary(theme, status);
                    style.text_color = Some(Color::WHITE);
                    style
                }),
            Space::new().height(6),
            
            checkbox(self.settings.borderless)
                .label("Borderless Fullscreen")
                .on_toggle(Message::BorderlessChanged)
                .text_size(13)
                .style(|theme, status| {
                    let mut style = checkbox::primary(theme, status);
                    style.text_color = Some(Color::WHITE);
                    style
                }),
            Space::new().height(6),
            
            checkbox(self.settings.vsync)
                .label("VSync")
                .on_toggle(Message::VsyncChanged)
                .text_size(13)
                .style(|theme, status| {
                    let mut style = checkbox::primary(theme, status);
                    style.text_color = Some(Color::WHITE);
                    style
                }),
        ];
        
        // === PERFORMANCE SETTINGS ===
        let performance_section = column![
            Space::new().height(14),
            text("Performance").size(16).color(section_title_color),
            Space::new().height(10),
            
            checkbox(self.settings.low_latency_mode)
                .label("Low Latency Mode")
                .on_toggle(Message::LowLatencyChanged)
                .text_size(13)
                .style(|theme, status| {
                    let mut style = checkbox::primary(theme, status);
                    style.text_color = Some(Color::WHITE);
                    style
                }),
        ];
        
        // === AUDIO SETTINGS ===
        let audio_section = column![
            Space::new().height(14),
            text("Audio").size(16).color(section_title_color),
            Space::new().height(10),
            
            checkbox(self.settings.surround)
                .label("Surround Sound (5.1)")
                .on_toggle(Message::SurroundChanged)
                .text_size(13)
                .style(|theme, status| {
                    let mut style = checkbox::primary(theme, status);
                    style.text_color = Some(Color::WHITE);
                    style
                }),
        ];
        
        // === INPUT SETTINGS ===
        let input_section = column![
            Space::new().height(14),
            text("Input").size(16).color(section_title_color),
            Space::new().height(10),
            
            checkbox(self.settings.clipboard_paste_enabled)
                .label("Enable Clipboard Paste (Ctrl+V)")
                .on_toggle(Message::ClipboardPasteChanged)
                .text_size(13)
                .style(|theme, status| {
                    let mut style = checkbox::primary(theme, status);
                    style.text_color = Some(Color::WHITE);
                    style
                }),
        ];
        
        // === GAME SETTINGS ===
        let language_options: Vec<String> = GameLanguage::all().iter().map(|l| l.display_name().to_string()).collect();
        let current_language = self.settings.game_language.display_name().to_string();
        
        let game_section = column![
            Space::new().height(14),
            text("Game").size(16).color(section_title_color),
            Space::new().height(10),
            
            row![
                text("Language").size(13).color(Color::WHITE).width(label_width),
                pick_list(language_options, Some(current_language), Message::GameLanguageChanged)
                    .width(180).text_size(12),
            ].align_y(Alignment::Center),
        ];
        
        // === SERVER SETTINGS ===
        let server_names: Vec<String> = servers.iter()
            .map(|s| format!("{} ({})", s.name, s.ping_ms.map(|p| format!("{}ms", p)).unwrap_or("?".into())))
            .collect();
        let selected_server = server_names.get(selected_server_index).cloned();
        
        let server_section = column![
            Space::new().height(14),
            text("Server").size(16).color(section_title_color),
            Space::new().height(10),
            
            row![
                text("Region").size(13).color(Color::WHITE).width(label_width),
                pick_list(server_names, selected_server, |_s| Message::ServerSelected(0))
                    .width(220).text_size(12),
            ].align_y(Alignment::Center),
            Space::new().height(8),
            
            button(text("Test Ping").size(12))
                .on_press(Message::StartPingTest)
                .padding(Padding::from([6, 14]))
                .style(|_, _| button::Style {
                    background: Some(Color::from_rgb(0.2, 0.2, 0.25).into()),
                    text_color: Color::WHITE,
                    border: iced_core::Border::default().rounded(6),
                    ..button::Style::default()
                }),
        ];
        
        // Reset button
        let reset_section = column![
            Space::new().height(20),
            button(text("Reset to Defaults").size(13))
                .on_press(Message::ResetSettings)
                .padding(Padding::from([8, 18]))
                .style(|_, _| button::Style {
                    background: Some(Color::from_rgb(0.3, 0.15, 0.15).into()),
                    text_color: Color::WHITE,
                    border: iced_core::Border::default().rounded(6),
                    ..button::Style::default()
                }),
        ];
        
        let content = column![
            title,
            Space::new().height(12),
            scrollable(
                column![
                    video_section,
                    display_section,
                    performance_section,
                    audio_section,
                    input_section,
                    game_section,
                    server_section,
                    reset_section,
                    Space::new().height(20),
                ]
                .padding(Padding::from([0, 12]))
            )
            .height(520),
        ]
        .padding(20);
        
        container(content)
            .width(420)
            .max_height(620)
            .style(|_| container::Style {
                background: Some(Color::from_rgb(0.1, 0.1, 0.14).into()),
                border: iced_core::Border::default().rounded(12),
                ..container::Style::default()
            })
            .into()
    }
    
    fn view_with_game_popup<'a>(
        &'a self,
        background: Element<'a, Message, Theme, Renderer>,
        game: &'a GameInfo,
    ) -> Element<'a, Message, Theme, Renderer> {
        let popup = self.view_game_popup(game);
        
        iced_widget::Stack::with_children(vec![
            background,
            container(popup)
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .style(|_| container::Style {
                    background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.7).into()),
                    ..container::Style::default()
                })
                .into(),
        ])
        .into()
    }
    
    fn view_game_popup<'a>(&'a self, game: &'a GameInfo) -> Element<'a, Message, Theme, Renderer> {
        let game_clone = game.clone();
        
        let title_row = row![
            text(&game.title).size(24).color(Color::WHITE),
            Space::new().width(Length::Fill),
            button(text("✕").size(18))
                .on_press(Message::GamePopupClose)
                .padding(8)
                .style(|_, _| button::Style {
                    background: None,
                    text_color: Color::WHITE,
                    ..button::Style::default()
                }),
        ];
        
        let store_text = text(format!("Platform: {}", game.store))
            .size(14)
            .color(Color::from_rgb(0.6, 0.6, 0.6));
        
        let publisher_text = if let Some(ref pub_name) = game.publisher {
            text(format!("Publisher: {}", pub_name))
                .size(14)
                .color(Color::from_rgb(0.6, 0.6, 0.6))
        } else {
            text("").size(14)
        };
        
        let desc_text = if let Some(ref desc) = game.description {
            text(desc)
                .size(14)
                .color(Color::from_rgb(0.8, 0.8, 0.8))
        } else {
            text("No description available.")
                .size(14)
                .color(Color::from_rgb(0.5, 0.5, 0.5))
        };
        
        // Variants selection
        let variants_section: Element<'a, Message, Theme, Renderer> = if game.variants.len() > 1 {
            let mut variant_btns: Vec<Element<'a, Message, Theme, Renderer>> = Vec::new();
            for (i, variant) in game.variants.iter().enumerate() {
                let is_selected = i == game.selected_variant_index;
                variant_btns.push(
                    button(text(&variant.store).size(12))
                        .padding(Padding::from([6, 12]))
                        .on_press(Message::VariantSelected(i))
                        .style(move |_, _| {
                            if is_selected {
                                button::Style {
                                    background: Some(Color::from_rgb(0.467, 0.784, 0.196).into()),
                                    text_color: Color::BLACK,
                                    border: iced_core::Border::default().rounded(6),
                                    ..button::Style::default()
                                }
                            } else {
                                button::Style {
                                    background: Some(Color::from_rgb(0.2, 0.2, 0.25).into()),
                                    text_color: Color::WHITE,
                                    border: iced_core::Border::default().rounded(6),
                                    ..button::Style::default()
                                }
                            }
                        })
                        .into()
                );
            }
            column![
                text("Platform:").size(14).color(Color::WHITE),
                Space::new().height(8),
                iced_widget::Row::with_children(variant_btns).spacing(8),
            ]
            .into()
        } else {
            Space::new().into()
        };
        
        let play_btn = button(
            container(text("Play").size(18))
                .padding(Padding::from([14, 48]))
        )
        .on_press(Message::GameLaunch(game_clone))
        .style(|_, _| button::Style {
            background: Some(Color::from_rgb(0.467, 0.784, 0.196).into()),
            text_color: Color::BLACK,
            border: iced_core::Border::default().rounded(8),
            ..button::Style::default()
        });
        
        let content = column![
            title_row,
            Space::new().height(16),
            store_text,
            publisher_text,
            Space::new().height(16),
            desc_text,
            Space::new().height(16),
            variants_section,
            Space::new().height(24),
            play_btn,
        ]
        .padding(24);
        
        container(content)
            .width(500)
            .style(|_| container::Style {
                background: Some(Color::from_rgb(0.1, 0.1, 0.14).into()),
                border: iced_core::Border::default().rounded(12),
                ..container::Style::default()
            })
            .into()
    }
    
    /// Stats overlay view for streaming (F3 to toggle)
    pub fn view_stats_overlay<'a>(
        &'a self,
        stats: &'a crate::media::StreamStats,
        decoder_backend: &'a str,
    ) -> Element<'a, Message, Theme, Renderer> {
        // Stats panel in bottom-left corner
        let green = Color::from_rgb(0.463, 0.725, 0.0);
        let white = Color::WHITE;
        let gray = Color::from_rgb(0.7, 0.7, 0.7);
        
        // FPS row
        let fps_row = row![
            text("FPS:").size(12).color(gray),
            Space::new().width(8),
            text(format!("{:.0}", stats.render_fps)).size(12).color(green),
            text(format!(" / {}", stats.target_fps)).size(12).color(gray),
        ];
        
        // Resolution row
        let res_row = row![
            text("Res:").size(12).color(gray),
            Space::new().width(8),
            text(&stats.resolution).size(12).color(white),
        ];
        
        // Codec row
        let codec_row = row![
            text("Codec:").size(12).color(gray),
            Space::new().width(8),
            text(&stats.codec).size(12).color(white),
        ];
        
        // Decoder row
        let decoder_display = if decoder_backend.is_empty() { "Auto" } else { decoder_backend };
        let decoder_row = row![
            text("Decoder:").size(12).color(gray),
            Space::new().width(8),
            text(decoder_display).size(12).color(white),
        ];
        
        // Bitrate row
        let bitrate_row = row![
            text("Bitrate:").size(12).color(gray),
            Space::new().width(8),
            text(format!("{:.1} Mbps", stats.bitrate_mbps)).size(12).color(white),
        ];
        
        // Latency row
        let latency_color = if stats.latency_ms < 30.0 {
            green
        } else if stats.latency_ms < 60.0 {
            Color::from_rgb(1.0, 0.784, 0.196) // yellow
        } else {
            Color::from_rgb(1.0, 0.314, 0.314) // red
        };
        let latency_row = row![
            text("Latency:").size(12).color(gray),
            Space::new().width(8),
            text(format!("{:.0} ms", stats.latency_ms)).size(12).color(latency_color),
        ];
        
        // Decode time row
        let decode_row = row![
            text("Decode:").size(12).color(gray),
            Space::new().width(8),
            text(format!("{:.1} ms", stats.decode_time_ms)).size(12).color(white),
        ];
        
        let stats_content = column![
            fps_row,
            res_row,
            codec_row,
            decoder_row,
            bitrate_row,
            latency_row,
            decode_row,
        ]
        .spacing(4)
        .padding(10);
        
        let stats_panel = container(stats_content)
            .style(|_| container::Style {
                background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.7).into()),
                border: iced_core::Border::default().rounded(8),
                ..container::Style::default()
            });
        
        // Position in bottom-left with margin
        container(
            column![
                Space::new().height(Length::Fill),
                row![
                    Space::new().width(10),
                    stats_panel,
                ],
                Space::new().height(10),
            ]
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}
