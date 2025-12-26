use serde::{Deserialize, Serialize};
use tauri::command;
use std::path::PathBuf;
use std::fs;

/// Application settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Preferred streaming quality (legacy, kept for backward compatibility)
    pub quality: StreamQuality,
    /// Custom resolution (e.g., "1920x1080")
    pub resolution: Option<String>,
    /// Custom FPS
    pub fps: Option<u32>,
    /// Preferred video codec
    pub codec: VideoCodecSetting,
    /// Preferred audio codec
    pub audio_codec: AudioCodecSetting,
    /// Max bitrate in Mbps (200 = unlimited)
    pub max_bitrate_mbps: u32,
    /// Preferred server region
    pub region: Option<String>,
    /// Enable Discord Rich Presence
    pub discord_rpc: bool,
    /// Show stats (resolution, fps, ms) in Discord presence
    pub discord_show_stats: Option<bool>,
    /// Custom proxy URL
    pub proxy: Option<String>,
    /// Disable telemetry
    pub disable_telemetry: bool,
    /// Window behavior
    pub start_minimized: bool,
    /// Auto-update games library
    pub auto_refresh_library: bool,
    /// Enable NVIDIA Reflex low-latency mode (auto-enabled for 120+ FPS)
    pub reflex: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StreamQuality {
    #[default]
    Auto,
    Custom,       // Use explicit resolution/fps values
    Low,          // 720p 30fps
    Medium,       // 1080p 60fps
    High,         // 1440p 60fps
    Ultra,        // 4K 60fps
    High120,      // 1080p 120fps
    Ultra120,     // 1440p 120fps
    Competitive,  // 1080p 240fps
    Extreme,      // 1080p 360fps
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum VideoCodecSetting {
    #[default]
    H264,
    H265,
    Av1,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum AudioCodecSetting {
    #[default]
    Opus,
    #[serde(rename = "opus-stereo")]
    OpusStereo,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            quality: StreamQuality::Auto,
            resolution: Some("1920x1080".to_string()),
            fps: Some(60),
            codec: VideoCodecSetting::H264,
            audio_codec: AudioCodecSetting::Opus,
            max_bitrate_mbps: 200, // 200 = unlimited
            region: None,
            discord_rpc: false,
            discord_show_stats: Some(false),
            proxy: None,
            disable_telemetry: true,
            start_minimized: false,
            auto_refresh_library: true,
            reflex: true, // Enabled by default for low-latency gaming
        }
    }
}

/// GFN API Configuration endpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GfnConfig {
    /// Starfleet OAuth base URL
    pub starfleet_url: String,
    /// Jarvis session API URL
    pub jarvis_url: String,
    /// NES/LCARS API URL (game library)
    pub nes_url: String,
    /// GraphQL endpoint
    pub graphql_url: String,
    /// Image CDN base URL
    pub image_cdn: String,
    /// Session control API
    pub session_url: String,
}

impl Default for GfnConfig {
    fn default() -> Self {
        Self {
            starfleet_url: "https://accounts.nvgs.nvidia.com".to_string(),
            jarvis_url: "https://jarvis.nvidia.com".to_string(),
            nes_url: "https://nes.nvidia.com".to_string(),
            graphql_url: "https://api.gdn.nvidia.com".to_string(),
            image_cdn: "https://img.nvidiagrid.net".to_string(),
            session_url: "https://pcs.geforcenow.com".to_string(),
        }
    }
}

/// Get the path to the settings file
fn get_settings_file_path() -> PathBuf {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."));
    let app_dir = config_dir.join("gfn-client");
    fs::create_dir_all(&app_dir).ok();
    app_dir.join("settings.json")
}

#[command]
pub async fn get_settings() -> Result<Settings, String> {
    let path = get_settings_file_path();
    if path.exists() {
        match fs::read_to_string(&path) {
            Ok(json) => {
                match serde_json::from_str::<Settings>(&json) {
                    Ok(settings) => {
                        log::info!("Loaded settings from {:?}", path);
                        return Ok(settings);
                    }
                    Err(e) => log::warn!("Failed to parse settings file: {}", e),
                }
            }
            Err(e) => log::warn!("Failed to read settings file: {}", e),
        }
    }
    Ok(Settings::default())
}

#[command]
pub async fn save_settings(settings: Settings) -> Result<(), String> {
    let path = get_settings_file_path();
    log::info!("Saving settings: {:?}", settings);

    let json = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;

    fs::write(&path, json)
        .map_err(|e| format!("Failed to write settings file: {}", e))?;

    log::info!("Settings saved to {:?}", path);
    Ok(())
}
