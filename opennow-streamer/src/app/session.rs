//! Session Management
//!
//! GFN session state and lifecycle.

use serde::{Deserialize, Serialize};

/// Session information
#[derive(Debug, Clone)]
pub struct SessionInfo {
    /// Session ID from CloudMatch
    pub session_id: String,

    /// Streaming server IP
    pub server_ip: String,

    /// Server region/zone
    pub zone: String,

    /// Current session state
    pub state: SessionState,

    /// GPU type allocated
    pub gpu_type: Option<String>,

    /// Signaling WebSocket URL (full URL like wss://server/nvst/)
    pub signaling_url: Option<String>,

    /// ICE servers from session API (for Alliance Partners with TURN servers)
    pub ice_servers: Vec<IceServerConfig>,

    /// Media connection info (real UDP port for streaming)
    pub media_connection_info: Option<MediaConnectionInfo>,
}

/// ICE server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IceServerConfig {
    pub urls: Vec<String>,
    pub username: Option<String>,
    pub credential: Option<String>,
}

/// Media connection info (real port for Alliance Partners)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaConnectionInfo {
    pub ip: String,
    pub port: u16,
}

/// Session state
#[derive(Debug, Clone, PartialEq)]
pub enum SessionState {
    /// Requesting session from CloudMatch
    Requesting,

    /// Connecting to server (seatSetupStep = 0)
    Connecting,

    /// Session created, seat being set up (configuring)
    Launching,

    /// In queue waiting for a seat (seatSetupStep = 1)
    InQueue {
        position: u32,
        eta_secs: u32,
    },

    /// Cleaning up previous session (seatSetupStep = 5)
    CleaningUp,

    /// Waiting for storage to be ready (seatSetupStep = 6)
    WaitingForStorage,

    /// Session ready for streaming
    Ready,

    /// Actively streaming
    Streaming,

    /// Session error
    Error(String),

    /// Session terminated
    Terminated,
}

impl SessionInfo {
    /// Create a new session in requesting state
    pub fn new_requesting(zone: &str) -> Self {
        Self {
            session_id: String::new(),
            server_ip: String::new(),
            zone: zone.to_string(),
            state: SessionState::Requesting,
            gpu_type: None,
            signaling_url: None,
            ice_servers: Vec::new(),
            media_connection_info: None,
        }
    }

    /// Check if session is ready to stream
    pub fn is_ready(&self) -> bool {
        matches!(self.state, SessionState::Ready)
    }

    /// Check if session is in queue
    pub fn is_queued(&self) -> bool {
        matches!(self.state, SessionState::InQueue { .. })
    }

    /// Get queue position if in queue
    pub fn queue_position(&self) -> Option<u32> {
        match self.state {
            SessionState::InQueue { position, .. } => Some(position),
            _ => None,
        }
    }
}

// ============================================
// CloudMatch API Request Types (Browser Format)
// ============================================

/// CloudMatch API request structure (browser format)
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudMatchRequest {
    pub session_request_data: SessionRequestData,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRequestData {
    /// App ID as STRING (browser format)
    pub app_id: String,
    pub internal_title: Option<String>,
    pub available_supported_controllers: Vec<i32>,
    pub network_test_session_id: Option<String>,
    pub parent_session_id: Option<String>,
    pub client_identification: String,
    pub device_hash_id: String,
    pub client_version: String,
    pub sdk_version: String,
    /// Streamer version as NUMBER (browser format)
    pub streamer_version: i32,
    pub client_platform_name: String,
    pub client_request_monitor_settings: Vec<MonitorSettings>,
    pub use_ops: bool,
    pub audio_mode: i32,
    pub meta_data: Vec<MetaDataEntry>,
    pub sdr_hdr_mode: i32,
    pub client_display_hdr_capabilities: Option<HdrCapabilities>,
    pub surround_audio_info: i32,
    pub remote_controllers_bitmap: i32,
    pub client_timezone_offset: i64,
    pub enhanced_stream_mode: i32,
    pub app_launch_mode: i32,
    pub secure_rtsp_supported: bool,
    pub partner_custom_data: Option<String>,
    pub account_linked: bool,
    pub enable_persisting_in_game_settings: bool,
    pub user_age: i32,
    pub requested_streaming_features: Option<StreamingFeatures>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorSettings {
    pub width_in_pixels: u32,
    pub height_in_pixels: u32,
    pub frames_per_second: u32,
    pub sdr_hdr_mode: i32,
    pub display_data: DisplayData,
    pub dpi: i32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayData {
    pub desired_content_max_luminance: i32,
    pub desired_content_min_luminance: i32,
    pub desired_content_max_frame_average_luminance: i32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HdrCapabilities {
    pub version: i32,
    pub hdr_edr_supported_flags_in_uint32: i32,
    pub static_metadata_descriptor_id: i32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaDataEntry {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamingFeatures {
    pub reflex: bool,
    pub bit_depth: i32,
    pub cloud_gsync: bool,
    pub enabled_l4s: bool,
    pub mouse_movement_flags: i32,
    pub true_hdr: bool,
    pub supported_hid_devices: i32,
    pub profile: i32,
    pub fallback_to_logical_resolution: bool,
    pub hid_devices: Option<String>,
    pub chroma_format: i32,
    pub prefilter_mode: i32,
    pub prefilter_sharpness: i32,
    pub prefilter_noise_reduction: i32,
    pub hud_streaming_mode: i32,
}

// ============================================
// CloudMatch API Response Types
// ============================================

/// CloudMatch API response
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudMatchResponse {
    pub session: CloudMatchSession,
    pub request_status: RequestStatus,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudMatchSession {
    pub session_id: String,
    #[serde(default)]
    pub seat_setup_info: Option<SeatSetupInfo>,
    #[serde(default)]
    pub session_control_info: Option<SessionControlInfo>,
    #[serde(default)]
    pub connection_info: Option<Vec<ConnectionInfoData>>,
    #[serde(default)]
    pub gpu_type: Option<String>,
    #[serde(default)]
    pub status: i32,
    #[serde(default)]
    pub error_code: i32,
    #[serde(default)]
    pub ice_server_configuration: Option<IceServerConfiguration>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeatSetupInfo {
    #[serde(default)]
    pub queue_position: i32,
    #[serde(default)]
    pub seat_setup_eta: i32,
    #[serde(default)]
    pub seat_setup_step: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionControlInfo {
    #[serde(default)]
    pub ip: Option<String>,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub resource_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInfoData {
    #[serde(default)]
    pub ip: Option<String>,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub resource_path: Option<String>,
    /// Usage type:
    ///   - 2:  Primary media path (UDP)
    ///   - 14: Signaling (WSS)
    ///   - 17: Alternative media path
    #[serde(default)]
    pub usage: i32,
    /// Protocol: 1 = TCP/WSS, 2 = UDP
    #[serde(default)]
    pub protocol: i32,
}

/// ICE server configuration from session API
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct IceServerConfiguration {
    #[serde(default)]
    pub ice_servers: Vec<SessionIceServer>,
}

/// Individual ICE server from session API
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SessionIceServer {
    pub urls: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub credential: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestStatus {
    pub status_code: i32,
    #[serde(default)]
    pub status_description: Option<String>,
    #[serde(default)]
    pub unified_error_code: i32,
    #[serde(default)]
    pub server_id: Option<String>,
}

impl CloudMatchSession {
    /// Extract streaming server IP from connection info
    pub fn streaming_server_ip(&self) -> Option<String> {
        // Look for connection with usage=14 (signaling) first
        self.connection_info
            .as_ref()
            .and_then(|conns| conns.iter().find(|c| c.usage == 14))
            .and_then(|conn| {
                // Try direct IP first
                conn.ip.clone().or_else(|| {
                    // If IP is null, extract from resourcePath (Alliance format)
                    // e.g., "rtsps://161-248-11-132.bpc.geforcenow.nvidiagrid.net:48322"
                    conn.resource_path.as_ref().and_then(|path| {
                        Self::extract_host_from_url(path)
                    })
                })
            })
            .or_else(|| {
                self.session_control_info
                    .as_ref()
                    .and_then(|sci| sci.ip.clone())
            })
    }

    /// Extract host from URL (handles rtsps://, wss://, etc.)
    fn extract_host_from_url(url: &str) -> Option<String> {
        // Remove protocol prefix
        let after_proto = url
            .strip_prefix("rtsps://")
            .or_else(|| url.strip_prefix("rtsp://"))
            .or_else(|| url.strip_prefix("wss://"))
            .or_else(|| url.strip_prefix("https://"))?;
        
        // Get host (before port or path)
        let host = after_proto
            .split(':')
            .next()
            .or_else(|| after_proto.split('/').next())?;
        
        if host.is_empty() || host.starts_with('.') {
            None
        } else {
            Some(host.to_string())
        }
    }

    /// Extract signaling URL from connection info
    pub fn signaling_url(&self) -> Option<String> {
        self.connection_info
            .as_ref()
            .and_then(|conns| conns.iter().find(|c| c.usage == 14))
            .and_then(|conn| conn.resource_path.clone())
    }

    /// Extract media connection info (usage=2, usage=17, or fallback to usage=14 for Alliance)
    pub fn media_connection_info(&self) -> Option<MediaConnectionInfo> {
        self.connection_info.as_ref().and_then(|conns| {
            // Try standard media paths first (usage=2 or usage=17)
            let media_conn = conns.iter()
                .find(|c| c.usage == 2)
                .or_else(|| conns.iter().find(|c| c.usage == 17));

            // If found, try to get IP/port
            if let Some(conn) = media_conn {
                let ip = conn.ip.clone().or_else(|| {
                    conn.resource_path.as_ref().and_then(|p| Self::extract_host_from_url(p))
                });
                let port = if conn.port > 0 { 
                    conn.port 
                } else {
                    conn.resource_path.as_ref().and_then(|p| Self::extract_port_from_url(p)).unwrap_or(0)
                };
                
                if let Some(ip) = ip {
                    if port > 0 {
                        return Some(MediaConnectionInfo { ip, port });
                    }
                }
            }

            // For Alliance: fall back to usage=14 with highest port (usually the UDP streaming port)
            // Alliance sessions have usage=14 for both signaling and media
            let alliance_conn = conns.iter()
                .filter(|c| c.usage == 14)
                .max_by_key(|c| c.port);

            alliance_conn.and_then(|conn| {
                let ip = conn.ip.clone().or_else(|| {
                    conn.resource_path.as_ref().and_then(|p| Self::extract_host_from_url(p))
                });
                let port = if conn.port > 0 { 
                    conn.port 
                } else {
                    conn.resource_path.as_ref().and_then(|p| Self::extract_port_from_url(p)).unwrap_or(0)
                };
                
                ip.filter(|_| port > 0).map(|ip| MediaConnectionInfo { ip, port })
            })
        })
    }

    /// Extract port from URL
    fn extract_port_from_url(url: &str) -> Option<u16> {
        // Find host:port pattern after ://
        let after_proto = url
            .strip_prefix("rtsps://")
            .or_else(|| url.strip_prefix("rtsp://"))
            .or_else(|| url.strip_prefix("wss://"))
            .or_else(|| url.strip_prefix("https://"))?;
        
        // Extract port after colon
        let parts: Vec<&str> = after_proto.split(':').collect();
        if parts.len() >= 2 {
            // Port is after the colon, before any path
            let port_str = parts[1].split('/').next()?;
            port_str.parse().ok()
        } else {
            None
        }
    }

    /// Convert ICE server configuration
    pub fn ice_servers(&self) -> Vec<IceServerConfig> {
        self.ice_server_configuration
            .as_ref()
            .map(|config| {
                config.ice_servers.iter().map(|server| {
                    IceServerConfig {
                        urls: vec![server.urls.clone()],
                        username: server.username.clone(),
                        credential: server.credential.clone(),
                    }
                }).collect()
            })
            .unwrap_or_default()
    }
}

// ============================================
// Session Management Types (GET /v2/session)
// ============================================

/// Response from GET /v2/session endpoint (list active sessions)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSessionsResponse {
    #[serde(default)]
    pub sessions: Vec<SessionFromApi>,
    pub request_status: RequestStatus,
}

/// Session data from GET /v2/session API
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionFromApi {
    pub session_id: String,
    #[serde(default)]
    pub session_request_data: Option<SessionRequestDataFromApi>,
    #[serde(default)]
    pub gpu_type: Option<String>,
    #[serde(default)]
    pub status: i32,
    #[serde(default)]
    pub session_control_info: Option<SessionControlInfo>,
    #[serde(default)]
    pub connection_info: Option<Vec<ConnectionInfoData>>,
    #[serde(default)]
    pub monitor_settings: Option<Vec<MonitorSettingsFromApi>>,
}

/// Lenient MonitorSettings for API response
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorSettingsFromApi {
    #[serde(default)]
    pub width_in_pixels: Option<u32>,
    #[serde(default)]
    pub height_in_pixels: Option<u32>,
    #[serde(default)]
    pub frames_per_second: Option<u32>,
}

/// Session request data from API (contains app_id)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRequestDataFromApi {
    /// App ID can be string or number
    #[serde(default)]
    pub app_id: Option<serde_json::Value>,
}

impl SessionRequestDataFromApi {
    pub fn get_app_id(&self) -> i64 {
        match &self.app_id {
            Some(serde_json::Value::Number(n)) => n.as_i64().unwrap_or(0),
            Some(serde_json::Value::String(s)) => s.parse::<i64>().unwrap_or(0),
            _ => 0,
        }
    }
}

/// Simplified active session info for UI
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveSessionInfo {
    pub session_id: String,
    pub app_id: i64,
    pub gpu_type: Option<String>,
    pub status: i32,
    pub server_ip: Option<String>,
    pub signaling_url: Option<String>,
    pub resolution: Option<String>,
    pub fps: Option<u32>,
}
