use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::command;
use tokio::sync::Mutex;

/// Streaming session state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingSession {
    pub session_id: String,
    pub game_id: String,
    pub server: SessionServer,
    pub status: SessionStatus,
    pub quality: StreamingQuality,
    pub stats: Option<StreamingStats>,
    pub webrtc_offer: Option<String>,
    pub signaling_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionServer {
    pub id: String,
    pub name: String,
    pub region: String,
    pub ip: Option<String>,
    pub zone: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionStatus {
    Queued { position: u32, estimated_wait: u32 },
    Connecting,
    Starting,
    Running,
    Paused,
    Resuming,
    Stopping,
    Stopped,
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingQuality {
    pub resolution: Resolution,
    pub fps: u32,
    pub bitrate_kbps: u32,
    pub codec: VideoCodec,
    pub hdr_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Resolution {
    R720p,
    R1080p,
    R1440p,
    R2160p, // 4K
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VideoCodec {
    H264,
    H265,
    AV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingStats {
    pub fps: f32,
    pub latency_ms: u32,
    pub packet_loss: f32,
    pub bitrate_kbps: u32,
    pub resolution: String,
    pub codec: String,
    pub jitter_ms: Option<f32>,
    pub round_trip_time_ms: Option<u32>,
}

/// Session start request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartSessionRequest {
    pub game_id: String,
    pub store_type: String,
    pub store_id: String,
    pub preferred_server: Option<String>,
    pub quality_preset: Option<String>,
    pub resolution: Option<String>,
    pub fps: Option<u32>,
    pub codec: Option<String>,
    pub max_bitrate_mbps: Option<u32>,
}

/// CloudMatch session request - based on GFN native client protocol (from geronimo.log)
/// POST to https://{zone}.cloudmatchbeta.nvidiagrid.net:443/v2/session?keyboardLayout=en-US&languageCode=en_US
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CloudMatchRequest {
    session_request_data: SessionRequestData,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionRequestData {
    app_id: i64, // GFN internal app ID (NOT store ID)
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
    client_display_hdr_capabilities: Option<HdrCapabilities>,
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
    requested_streaming_features: Option<StreamingFeatures>,
    transport: Option<String>,
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
    display_data: Option<DisplayData>,
    hdr10_plus_gaming_data: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DisplayData {
    display_primary_x0: f32,
    display_primary_y0: f32,
    display_primary_x1: f32,
    display_primary_y1: f32,
    display_primary_x2: f32,
    display_primary_y2: f32,
    display_white_point_x: f32,
    display_white_point_y: f32,
    desired_content_max_luminance: f32,
    desired_content_min_luminance: f32,
    desired_content_max_frame_average_luminance: f32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HdrCapabilities {
    version: i32,
    hdr_edr_supported_flags_in_uint32: i32,
    static_metadata_descriptor_id: i32,
    display_data: Option<DisplayData>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MetaDataEntry {
    key: String,
    value: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamingFeatures {
    reflex: bool,
    bit_depth: i32,
    cloud_gsync: bool,
    enabled_l4s: bool,
    mouse_movement_flags: i32,
    true_hdr: bool,
    supported_hid_devices: i32,
    profile: i32,
    fallback_to_logical_resolution: bool,
    hid_devices: Option<String>,
    chroma_format: i32,
    prefilter_mode: i32,
    prefilter_sharpness: i32,
    prefilter_noise_reduction: i32,
    hud_streaming_mode: i32,
}

/// Session start response from CloudMatch - matches actual API response
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CloudMatchApiResponse {
    session: SessionData,
    request_status: RequestStatus,
    #[serde(default)]
    other_user_sessions: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionData {
    session_id: String,
    #[serde(default)]
    session_request_data: Option<serde_json::Value>,
    #[serde(default)]
    seat_setup_info: Option<SeatSetupInfo>,
    #[serde(default)]
    session_control_info: Option<SessionControlInfo>,
    #[serde(default)]
    connection_info: Option<Vec<ConnectionInfo>>,
    #[serde(default)]
    gpu_type: Option<String>,
    #[serde(default)]
    status: i32,
    #[serde(default)]
    error_code: i32,
    #[serde(default)]
    client_ip: Option<String>,
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
    #[serde(default)]
    resource_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionInfo {
    #[serde(default)]
    ip: Option<String>,
    #[serde(default)]
    port: u16,
    #[serde(default)]
    resource_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RequestStatus {
    status_code: i32,
    #[serde(default)]
    status_description: Option<String>,
    #[serde(default)]
    unified_error_code: i32,
    #[serde(default)]
    request_id: Option<String>,
    #[serde(default)]
    server_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IceServer {
    pub urls: Vec<String>,
    pub username: Option<String>,
    pub credential: Option<String>,
}

/// WebRTC signaling messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SignalingMessage {
    #[serde(rename = "offer")]
    Offer { sdp: String },
    #[serde(rename = "answer")]
    Answer { sdp: String },
    #[serde(rename = "candidate")]
    IceCandidate { candidate: String, sdp_mid: Option<String>, sdp_m_line_index: Option<u32> },
    #[serde(rename = "ready")]
    Ready,
    #[serde(rename = "bye")]
    Bye,
}

/// WebRTC session info for frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebRTCSessionInfo {
    pub session_id: String,
    pub signaling_url: String,
    pub ice_servers: Vec<IceServer>,
    pub offer_sdp: Option<String>,
}

/// Global session storage
static CURRENT_SESSION: std::sync::OnceLock<Arc<Mutex<Option<StreamingSession>>>> = std::sync::OnceLock::new();

fn get_session_storage() -> Arc<Mutex<Option<StreamingSession>>> {
    CURRENT_SESSION
        .get_or_init(|| Arc::new(Mutex::new(None)))
        .clone()
}

// API endpoints - discovered from GFN native client analysis (geronimo.log)
// CloudMatch handles session allocation - regional endpoints
// Format: https://{zone}.cloudmatchbeta.nvidiagrid.net:443/v2/session
// Example zones: eu-netherlands-north, us-california-north, ap-japan, etc.
const CLOUDMATCH_DEFAULT_ZONE: &str = "eu-netherlands-north";
const CLOUDMATCH_PROD_URL: &str = "https://prod.cloudmatchbeta.nvidiagrid.net";

// Server info endpoint to get zone/server details
fn cloudmatch_zone_url(zone: &str) -> String {
    format!("https://{}.cloudmatchbeta.nvidiagrid.net", zone)
}

// STUN/TURN servers for WebRTC
// NVIDIA's official server (TURN servers also handle STUN requests)
const DEFAULT_ICE_SERVERS: &[&str] = &[
    "stun:turn.gamestream.nvidia.com:19302",
    "stun:stun.l.google.com:19302",
];

/// Parse resolution string to width/height
/// Supports formats: "1080p", "1440p", "4k", "2160p", or "WIDTHxHEIGHT" (e.g., "2560x1440")
fn parse_resolution(resolution: Option<&str>) -> (u32, u32) {
    match resolution {
        Some("720p") => (1280, 720),
        Some("1080p") => (1920, 1080),
        Some("1440p") => (2560, 1440),
        Some("4k") | Some("2160p") => (3840, 2160),
        Some(res) if res.contains('x') => {
            // Parse "WIDTHxHEIGHT" format
            let parts: Vec<&str> = res.split('x').collect();
            if parts.len() == 2 {
                let width = parts[0].parse::<u32>().unwrap_or(1920);
                let height = parts[1].parse::<u32>().unwrap_or(1080);
                log::info!("Parsed resolution {}x{} from '{}'", width, height, res);
                (width, height)
            } else {
                (1920, 1080)
            }
        }
        _ => (1920, 1080), // Default to 1080p
    }
}

fn parse_codec(codec: Option<&str>) -> VideoCodec {
    match codec {
        Some("h264") | Some("H264") => VideoCodec::H264,
        Some("h265") | Some("H265") | Some("hevc") | Some("HEVC") => VideoCodec::H265,
        Some("av1") | Some("AV1") => VideoCodec::AV1,
        _ => VideoCodec::H264, // Default to H264
    }
}

/// Generate a device ID (UUID format like browser client)
fn generate_device_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Get or create a persistent device ID stored in app data
fn get_device_id() -> String {
    // For now, generate a new one - in production this should be persisted
    generate_device_id()
}

/// Get client ID (also UUID format)
fn get_client_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Start a streaming session with CloudMatch and get WebRTC signaling info
/// Uses the browser client format which works with standard JWT authentication
#[command]
pub async fn start_session(
    request: StartSessionRequest,
    access_token: String,
) -> Result<StreamingSession, String> {
    log::info!("Starting streaming session for game: {}", request.game_id);
    log::info!("Requested resolution: {:?}, fps: {:?}", request.resolution, request.fps);

    // Use proxy-aware client if configured
    let client = crate::proxy::create_proxied_client().await?;

    let (width, height) = parse_resolution(request.resolution.as_deref());
    let fps = request.fps.unwrap_or(60);
    let codec = parse_codec(request.codec.as_deref());
    let max_bitrate_mbps = request.max_bitrate_mbps.unwrap_or(200);
    // Convert to kbps, 200+ means unlimited (use very high value)
    let max_bitrate_kbps = if max_bitrate_mbps >= 200 {
        500_000 // 500 Mbps = effectively unlimited
    } else {
        max_bitrate_mbps * 1000
    };

    log::info!("Using resolution {}x{} @ {} FPS, codec: {:?}, max bitrate: {} kbps",
               width, height, fps, codec, max_bitrate_kbps);

    // Determine zone to use (browser uses eu-netherlands-south)
    let zone = request.preferred_server.clone()
        .unwrap_or_else(|| "eu-netherlands-south".to_string());

    // Generate device and client IDs (UUID format like browser)
    let device_id = get_device_id();
    let client_id = get_client_id();
    let sub_session_id = uuid::Uuid::new_v4().to_string();

    // Get timezone offset in milliseconds
    let timezone_offset_ms = chrono::Local::now().offset().local_minus_utc() as i64 * 1000;

    // Build CloudMatch request matching the BROWSER client format exactly
    let cloudmatch_request = CloudMatchRequest {
        session_request_data: SessionRequestData {
            app_id: request.game_id.parse::<i64>().unwrap_or(0), // Must be GFN app ID (e.g., 102048611)
            internal_title: None,
            available_supported_controllers: vec![], // Browser sends empty
            preferred_controller: 0,
            network_test_session_id: Some("00000000-0000-0000-0000-000000000000".to_string()),
            parent_session_id: None,
            client_identification: "GFN-PC".to_string(),
            device_hash_id: device_id.clone(), // UUID format
            client_version: "30.0".to_string(),
            sdk_version: "1.0".to_string(), // Browser uses 1.0
            streamer_version: "1".to_string(), // Browser uses "1"
            client_platform_name: "windows".to_string(), // Native client
            client_request_monitor_settings: vec![MonitorSettings {
                monitor_id: 0,
                position_x: 0,
                position_y: 0,
                width_in_pixels: width,
                height_in_pixels: height,
                dpi: 0,
                frames_per_second: fps,
                sdr_hdr_mode: 0, // Browser uses 0
                display_data: None,
                hdr10_plus_gaming_data: None,
            }],
            use_ops: false, // Browser uses false
            audio_mode: 0, // Browser uses 0
            meta_data: vec![
                MetaDataEntry { key: "SubSessionId".to_string(), value: sub_session_id },
                MetaDataEntry { key: "wssignaling".to_string(), value: "1".to_string() },
                MetaDataEntry { key: "GSStreamerType".to_string(), value: "WebRTC".to_string() },
                MetaDataEntry { key: "networkType".to_string(), value: "Unknown".to_string() },
                MetaDataEntry { key: "ClientImeSupport".to_string(), value: "0".to_string() },
                MetaDataEntry { key: "clientPhysicalResolution".to_string(), value: format!("{{\"horizontalPixels\":{},\"verticalPixels\":{}}}", width, height) },
                MetaDataEntry { key: "surroundAudioInfo".to_string(), value: "2".to_string() },
            ],
            sdr_hdr_mode: 0,
            client_display_hdr_capabilities: None, // Browser sends null
            surround_audio_info: 0,
            remote_controllers_bitmap: 0,
            client_timezone_offset: timezone_offset_ms,
            enhanced_stream_mode: 1, // Browser uses 1
            app_launch_mode: 1,
            secure_rtsp_supported: false, // Browser uses false
            partner_custom_data: Some("".to_string()), // Browser sends empty string
            account_linked: false,
            enable_persisting_in_game_settings: false,
            requested_audio_format: 0,
            user_age: 0, // Browser uses 0
            requested_streaming_features: None, // Browser sends null
            transport: None,
        },
    };

    log::info!("Requesting session from CloudMatch zone: {}", zone);
    log::debug!("Device ID: {}, Client ID: {}", device_id, client_id);

    // Build the session URL with query params
    let session_url = format!(
        "{}/v2/session?keyboardLayout=en-US&languageCode=en_US",
        cloudmatch_zone_url(&zone)
    );

    // Request session from CloudMatch with browser-style headers
    let response = client
        .post(&session_url)
        .header("Authorization", format!("GFNJWT {}", access_token))
        .header("Content-Type", "application/json")
        // NV-* headers that browser sends
        .header("nv-browser-type", "CHROME")
        .header("nv-client-id", &client_id)
        .header("nv-client-streamer", "NVIDIA-CLASSIC")
        .header("nv-client-type", "NATIVE")
        .header("nv-client-version", "2.0.80.173")
        .header("nv-device-make", "UNKNOWN")
        .header("nv-device-model", "UNKNOWN")
        .header("nv-device-os", "WINDOWS")
        .header("nv-device-type", "DESKTOP")
        .header("x-device-id", &device_id)
        .json(&cloudmatch_request)
        .send()
        .await
        .map_err(|e| format!("Failed to request session: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        log::error!("CloudMatch request failed: {} - {}", status, body);

        return Err(format!("Session request failed: {} - {}", status, body));
    }

    // First get raw text to debug
    let response_text = response.text().await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    log::info!("CloudMatch response: {}", &response_text[..std::cmp::min(500, response_text.len())]);

    let api_response: CloudMatchApiResponse = serde_json::from_str(&response_text)
        .map_err(|e| format!("Failed to parse CloudMatch response: {} - Response: {}", e, &response_text[..std::cmp::min(500, response_text.len())]))?;

    // Check request status
    if api_response.request_status.status_code != 1 {
        let error_desc = api_response.request_status.status_description
            .unwrap_or_else(|| "Unknown error".to_string());
        return Err(format!("CloudMatch error: {} (code: {}, unified: {})",
            error_desc,
            api_response.request_status.status_code,
            api_response.request_status.unified_error_code));
    }

    let session_data = api_response.session;
    log::info!("Session allocated: {}", session_data.session_id);

    // Determine initial status from seat setup info
    let status = if let Some(ref seat_info) = session_data.seat_setup_info {
        if seat_info.queue_position > 0 {
            SessionStatus::Queued {
                position: seat_info.queue_position as u32,
                estimated_wait: (seat_info.seat_setup_eta / 1000) as u32,
            }
        } else {
            SessionStatus::Connecting
        }
    } else {
        SessionStatus::Connecting
    };

    let resolution = match (width, height) {
        (1280, 720) => Resolution::R720p,
        (1920, 1080) => Resolution::R1080p,
        (2560, 1440) => Resolution::R1440p,
        _ => Resolution::R2160p,
    };

    // Get server info from session control info
    let server_ip = session_data.session_control_info
        .as_ref()
        .and_then(|sci| sci.ip.clone())
        .unwrap_or_else(|| "unknown".to_string());

    // Get streaming connection info (WebRTC/RTSP endpoint)
    let signaling_url = session_data.connection_info
        .as_ref()
        .and_then(|conns| conns.first())
        .and_then(|conn| conn.resource_path.clone());

    let session = StreamingSession {
        session_id: session_data.session_id.clone(),
        game_id: request.game_id,
        server: SessionServer {
            id: api_response.request_status.server_id.unwrap_or_else(|| "unknown".to_string()),
            name: session_data.gpu_type.unwrap_or_else(|| "GFN Server".to_string()),
            region: zone.clone(),
            ip: Some(server_ip),
            zone: Some(zone),
        },
        status,
        quality: StreamingQuality {
            resolution,
            fps,
            bitrate_kbps: max_bitrate_kbps,
            codec,
            hdr_enabled: false,
        },
        stats: None,
        webrtc_offer: None,
        signaling_url,
    };

    // Store session
    {
        let storage = get_session_storage();
        let mut guard = storage.lock().await;
        *guard = Some(session.clone());
    }

    Ok(session)
}

/// Calculate appropriate bitrate based on resolution and FPS
fn calculate_bitrate(width: u32, height: u32, fps: u32) -> u32 {
    // Base bitrate calculation (in kbps) - max 100 Mbps
    let pixels = width * height;
    let base_bitrate = match pixels {
        p if p <= 921600 => 30000,   // 720p: 30 Mbps
        p if p <= 2073600 => 50000,  // 1080p: 50 Mbps
        p if p <= 3686400 => 70000,  // 1440p: 70 Mbps
        _ => 80000,                   // 4K: 80 Mbps
    };

    // Adjust for frame rate
    let adjusted = if fps >= 360 {
        base_bitrate * 20 / 10 // 360fps: +100%
    } else if fps >= 240 {
        base_bitrate * 17 / 10 // 240fps: +70%
    } else if fps >= 120 {
        base_bitrate * 14 / 10 // 120fps: +40%
    } else if fps > 30 {
        base_bitrate
    } else {
        base_bitrate * 7 / 10 // 30fps: -30%
    };

    // Cap at 100 Mbps
    std::cmp::min(adjusted, 100000)
}

/// Stop a streaming session
#[command]
pub async fn stop_session(
    session_id: String,
    access_token: String,
) -> Result<(), String> {
    log::info!("Stopping streaming session: {}", session_id);

    // Get the session to find the zone
    let zone = {
        let storage = get_session_storage();
        let guard = storage.lock().await;
        guard.as_ref()
            .and_then(|s| s.server.zone.clone())
            .unwrap_or_else(|| CLOUDMATCH_DEFAULT_ZONE.to_string())
    };

    let client = crate::proxy::create_proxied_client().await?;

    // DELETE to https://{zone}.cloudmatchbeta.nvidiagrid.net:443/v2/session/{session_id}
    let delete_url = format!("{}/v2/session/{}", cloudmatch_zone_url(&zone), session_id);

    let response = client
        .delete(&delete_url)
        .header("Authorization", format!("GFNJWT {}", access_token))
        .header("Content-Type", "application/json")
        .send()
        .await
        .map_err(|e| format!("Failed to stop session: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        log::warn!("Session stop returned: {} - {}", status, body);
    }

    // Clear stored session
    {
        let storage = get_session_storage();
        let mut guard = storage.lock().await;
        *guard = None;
    }

    log::info!("Session stopped: {}", session_id);
    Ok(())
}

/// Launch a game via the official GFN client using deep links
/// This is the recommended way to launch games since direct session creation requires Bifrost SDK
#[command]
pub async fn launch_via_official_client(game_id: String) -> Result<(), String> {
    log::info!("Launching game {} via official GFN client", game_id);

    // GFN deep link format: geforcenow://game/{app_id}
    // This will open the official GFN client if installed
    let deep_link = format!("geforcenow://game/{}", game_id);

    // Try to open the deep link
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &deep_link])
            .spawn()
            .map_err(|e| format!("Failed to launch GFN client: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&deep_link)
            .spawn()
            .map_err(|e| format!("Failed to launch GFN client: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&deep_link)
            .spawn()
            .map_err(|e| format!("Failed to launch GFN client: {}", e))?;
    }

    Ok(())
}

/// Open a game in the GFN web browser
/// Alternative to the native client
#[command]
pub async fn launch_via_web(game_id: String) -> Result<(), String> {
    log::info!("Opening game {} in GFN web", game_id);

    // GFN web URL format
    let web_url = format!("https://play.geforcenow.com/#/games?game-id={}", game_id);

    // Open in default browser
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &web_url])
            .spawn()
            .map_err(|e| format!("Failed to open browser: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&web_url)
            .spawn()
            .map_err(|e| format!("Failed to open browser: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&web_url)
            .spawn()
            .map_err(|e| format!("Failed to open browser: {}", e))?;
    }

    Ok(())
}

/// Session status response from server
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionStatusResponse {
    status: String,
    queue_position: Option<u32>,
    estimated_wait_seconds: Option<u32>,
    server_address: Option<String>,
    signaling_url: Option<String>,
}

/// Get current session status
#[command]
pub async fn get_session_status(
    session_id: String,
    access_token: String,
) -> Result<SessionStatus, String> {
    log::info!("Getting session status: {}", session_id);

    // Get the zone from stored session
    let zone = {
        let storage = get_session_storage();
        let guard = storage.lock().await;
        guard.as_ref()
            .and_then(|s| s.server.zone.clone())
            .unwrap_or_else(|| CLOUDMATCH_DEFAULT_ZONE.to_string())
    };

    let client = crate::proxy::create_proxied_client().await?;

    // GET https://{zone}.cloudmatchbeta.nvidiagrid.net:443/v2/session/{session_id}
    let status_url = format!("{}/v2/session/{}", cloudmatch_zone_url(&zone), session_id);

    let response = client
        .get(&status_url)
        .header("Authorization", format!("GFNJWT {}", access_token))
        .send()
        .await
        .map_err(|e| format!("Failed to get session status: {}", e))?;

    if !response.status().is_success() {
        let status_code = response.status();
        if status_code.as_u16() == 404 {
            return Ok(SessionStatus::Stopped);
        }
        return Err(format!("Status request failed with status: {}", status_code));
    }

    let status_response: SessionStatusResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse status response: {}", e))?;

    // Map server status to our enum
    let status = match status_response.status.to_lowercase().as_str() {
        "queued" | "queue" => SessionStatus::Queued {
            position: status_response.queue_position.unwrap_or(0),
            estimated_wait: status_response.estimated_wait_seconds.unwrap_or(0),
        },
        "connecting" => SessionStatus::Connecting,
        "starting" => SessionStatus::Starting,
        "running" | "active" => SessionStatus::Running,
        "paused" => SessionStatus::Paused,
        "resuming" => SessionStatus::Resuming,
        "stopping" => SessionStatus::Stopping,
        "stopped" | "ended" => SessionStatus::Stopped,
        "error" | "failed" => SessionStatus::Error {
            message: "Session encountered an error".to_string(),
        },
        _ => SessionStatus::Error {
            message: format!("Unknown status: {}", status_response.status),
        },
    };

    // Update stored session if we have new signaling info
    if status_response.signaling_url.is_some() {
        let storage = get_session_storage();
        let mut guard = storage.lock().await;
        if let Some(session) = guard.as_mut() {
            session.signaling_url = status_response.signaling_url;
            session.status = status.clone();
        }
    }

    Ok(status)
}

/// Get WebRTC connection info for current session
#[command]
pub async fn get_webrtc_info(session_id: String) -> Result<WebRTCSessionInfo, String> {
    let storage = get_session_storage();
    let guard = storage.lock().await;

    match &*guard {
        Some(session) if session.session_id == session_id => {
            let signaling_url = session
                .signaling_url
                .clone()
                .ok_or_else(|| "Signaling URL not available yet".to_string())?;

            // Provide default ICE servers if none were returned
            let ice_servers = vec![
                IceServer {
                    urls: DEFAULT_ICE_SERVERS.iter().map(|s| s.to_string()).collect(),
                    username: None,
                    credential: None,
                },
            ];

            Ok(WebRTCSessionInfo {
                session_id: session.session_id.clone(),
                signaling_url,
                ice_servers,
                offer_sdp: session.webrtc_offer.clone(),
            })
        }
        Some(_) => Err("Session ID mismatch".to_string()),
        None => Err("No active session".to_string()),
    }
}

/// Get current streaming session
#[command]
pub async fn get_current_session() -> Result<Option<StreamingSession>, String> {
    let storage = get_session_storage();
    let guard = storage.lock().await;
    Ok(guard.clone())
}

/// Update streaming stats from frontend
#[command]
pub async fn update_streaming_stats(stats: StreamingStats) -> Result<(), String> {
    let storage = get_session_storage();
    let mut guard = storage.lock().await;

    if let Some(session) = guard.as_mut() {
        session.stats = Some(stats);
        log::debug!("Streaming stats updated");
    }

    Ok(())
}

/// Session control actions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionAction {
    Pause,
    Resume,
    Restart,
}

/// Send control command to session
#[command]
pub async fn control_session(
    session_id: String,
    action: SessionAction,
    access_token: String,
) -> Result<(), String> {
    let action_str = match action {
        SessionAction::Pause => "pause",
        SessionAction::Resume => "resume",
        SessionAction::Restart => "restart",
    };

    log::info!("Sending {} command to session {}", action_str, session_id);

    // Get the zone from stored session
    let zone = {
        let storage = get_session_storage();
        let guard = storage.lock().await;
        guard.as_ref()
            .and_then(|s| s.server.zone.clone())
            .unwrap_or_else(|| CLOUDMATCH_DEFAULT_ZONE.to_string())
    };

    let client = crate::proxy::create_proxied_client().await?;

    let response = client
        .post(format!("{}/v2/session/{}/control", cloudmatch_zone_url(&zone), session_id))
        .header("Authorization", format!("GFNJWT {}", access_token))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({ "action": action_str }))
        .send()
        .await
        .map_err(|e| format!("Failed to send control command: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Control command failed: {} - {}", status, body));
    }

    // Update local session status
    {
        let storage = get_session_storage();
        let mut guard = storage.lock().await;
        if let Some(session) = guard.as_mut() {
            session.status = match action {
                SessionAction::Pause => SessionStatus::Paused,
                SessionAction::Resume => SessionStatus::Resuming,
                SessionAction::Restart => SessionStatus::Starting,
            };
        }
    }

    Ok(())
}

impl Default for StreamingQuality {
    fn default() -> Self {
        Self {
            resolution: Resolution::R1080p,
            fps: 60,
            bitrate_kbps: 25000,
            codec: VideoCodec::H264,
            hdr_enabled: false,
        }
    }
}

impl Resolution {
    pub fn width(&self) -> u32 {
        match self {
            Resolution::R720p => 1280,
            Resolution::R1080p => 1920,
            Resolution::R1440p => 2560,
            Resolution::R2160p => 3840,
        }
    }

    pub fn height(&self) -> u32 {
        match self {
            Resolution::R720p => 720,
            Resolution::R1080p => 1080,
            Resolution::R1440p => 1440,
            Resolution::R2160p => 2160,
        }
    }
}

// ============================================================================
// SESSION POLLING & STREAMING MANAGEMENT
// ============================================================================

use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;

/// Global flag to control polling loop
static POLLING_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Streaming connection state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingConnectionState {
    pub session_id: String,
    pub phase: StreamingPhase,
    pub server_ip: Option<String>,
    pub signaling_url: Option<String>,
    pub connection_info: Option<StreamConnectionInfo>,
    pub gpu_type: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StreamingPhase {
    Queued { position: i32, eta_ms: i32 },
    SeatSetup { step: i32, eta_ms: i32 },
    Connecting,
    Ready,
    Streaming,
    Error,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConnectionInfo {
    pub control_ip: String,
    pub control_port: u16,
    pub stream_ip: Option<String>,
    pub stream_port: u16,
    pub resource_path: String,
}

/// Extended session status response for polling
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PollSessionResponse {
    session: PollSessionData,
    request_status: RequestStatus,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PollSessionData {
    session_id: String,
    #[serde(default)]
    seat_setup_info: Option<SeatSetupInfo>,
    #[serde(default)]
    session_control_info: Option<SessionControlInfo>,
    #[serde(default)]
    connection_info: Option<Vec<ConnectionInfo>>,
    #[serde(default)]
    gpu_type: Option<String>,
    #[serde(default)]
    status: i32,
    #[serde(default)]
    error_code: i32,
    #[serde(default)]
    client_ip: Option<String>,
    #[serde(default)]
    monitor_settings: Option<serde_json::Value>,
    #[serde(default)]
    finalized_streaming_features: Option<serde_json::Value>,
}

/// Poll session status until ready or error
/// Returns the final streaming connection state
#[command]
pub async fn poll_session_until_ready(
    session_id: String,
    access_token: String,
    poll_interval_ms: Option<u64>,
) -> Result<StreamingConnectionState, String> {
    let interval = poll_interval_ms.unwrap_or(1500); // Default 1.5 seconds

    log::info!("Starting session polling for {}", session_id);
    POLLING_ACTIVE.store(true, Ordering::SeqCst);

    // Get session info for zone
    let (zone, control_server) = {
        let storage = get_session_storage();
        let guard = storage.lock().await;
        match guard.as_ref() {
            Some(session) => (
                session.server.zone.clone().unwrap_or_else(|| "eu-netherlands-south".to_string()),
                session.server.ip.clone(),
            ),
            None => return Err("No active session found".to_string()),
        }
    };

    let client = crate::proxy::create_proxied_client().await?;

    // Build polling URL using session control server
    let poll_base = control_server
        .map(|ip| format!("https://{}", ip))
        .unwrap_or_else(|| cloudmatch_zone_url(&zone));

    let poll_url = format!("{}/v2/session/{}", poll_base, session_id);
    log::info!("Polling URL: {}", poll_url);

    let device_id = get_device_id();
    let client_id = get_client_id();

    let mut last_status = -1;
    let mut last_step = -1;
    let max_polls = 120; // Max ~3 minutes of polling
    let mut poll_count = 0;

    loop {
        if !POLLING_ACTIVE.load(Ordering::SeqCst) {
            return Err("Polling cancelled".to_string());
        }

        poll_count += 1;
        if poll_count > max_polls {
            return Err("Session polling timeout - server not ready".to_string());
        }

        // Poll session status
        let response = client
            .get(&poll_url)
            .header("Authorization", format!("GFNJWT {}", access_token))
            .header("Content-Type", "application/json")
            .header("nv-client-id", &client_id)
            .header("nv-client-streamer", "NVIDIA-CLASSIC")
            .header("nv-client-type", "NATIVE")
            .header("nv-client-version", "2.0.80.173")
            .header("nv-device-os", "WINDOWS")
            .header("nv-device-type", "DESKTOP")
            .header("x-device-id", &device_id)
            .send()
            .await
            .map_err(|e| format!("Poll request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            log::error!("Poll failed: {} - {}", status, body);
            return Err(format!("Poll failed: {} - {}", status, body));
        }

        let response_text = response.text().await
            .map_err(|e| format!("Failed to read poll response: {}", e))?;

        let poll_response: PollSessionResponse = serde_json::from_str(&response_text)
            .map_err(|e| format!("Failed to parse poll response: {}", e))?;

        // Check for API errors
        if poll_response.request_status.status_code != 1 {
            let error = poll_response.request_status.status_description
                .unwrap_or_else(|| "Unknown error".to_string());
            return Err(format!("Session error: {}", error));
        }

        let session = poll_response.session;
        let status = session.status;
        let seat_info = session.seat_setup_info.as_ref();
        let step = seat_info.map(|s| s.seat_setup_step).unwrap_or(0);

        // Log status changes
        if status != last_status || step != last_step {
            log::info!(
                "Session status: {} (step: {}, queue: {}, eta: {}ms, gpu: {:?})",
                status,
                step,
                seat_info.map(|s| s.queue_position).unwrap_or(0),
                seat_info.map(|s| s.seat_setup_eta).unwrap_or(0),
                session.gpu_type
            );
            last_status = status;
            last_step = step;
        }

        // Status 2 = ready for streaming
        if status == 2 {
            log::info!("Session ready! GPU: {:?}", session.gpu_type);

            // Extract connection info for WebRTC streaming
            // Browser client uses port 443 with /nvst/ path for WebSocket signaling
            // Native client uses port 322 for RTSPS - but we're browser-based
            let connection = session.connection_info.as_ref()
                .and_then(|conns| conns.first())
                .map(|conn| {
                    // Get the stream IP from connection info or fall back to control IP
                    let stream_ip = conn.ip.clone().or_else(|| {
                        session.session_control_info.as_ref()
                            .and_then(|c| c.ip.clone())
                    });

                    // Get the resource path (typically /nvst/ for WebRTC signaling)
                    let resource_path = conn.resource_path.clone()
                        .unwrap_or_else(|| "/nvst/".to_string());

                    // Use port 443 for browser WebSocket signaling (not 322 which is RTSPS)
                    // The browser client always connects on 443 with wss://
                    let stream_port = if conn.port == 322 || conn.port == 48322 {
                        // These are RTSPS ports - use 443 for browser WebSocket instead
                        443
                    } else if conn.port == 0 {
                        443
                    } else {
                        conn.port
                    };

                    StreamConnectionInfo {
                        control_ip: session.session_control_info.as_ref()
                            .and_then(|c| c.ip.clone())
                            .unwrap_or_default(),
                        control_port: session.session_control_info.as_ref()
                            .map(|c| c.port)
                            .unwrap_or(443),
                        stream_ip,
                        stream_port,
                        resource_path,
                    }
                });

            // Update stored session status
            {
                let storage = get_session_storage();
                let mut guard = storage.lock().await;
                if let Some(s) = guard.as_mut() {
                    s.status = SessionStatus::Running;
                }
            }

            POLLING_ACTIVE.store(false, Ordering::SeqCst);

            return Ok(StreamingConnectionState {
                session_id: session.session_id,
                phase: StreamingPhase::Ready,
                server_ip: session.session_control_info.and_then(|c| c.ip),
                signaling_url: session.connection_info
                    .and_then(|c| c.first().and_then(|i| i.resource_path.clone())),
                connection_info: connection,
                gpu_type: session.gpu_type,
                error: None,
            });
        }

        // Status 1 = still setting up
        // Status 0 or negative = error
        if status <= 0 && session.error_code != 1 {
            POLLING_ACTIVE.store(false, Ordering::SeqCst);
            return Err(format!("Session failed with error code: {}", session.error_code));
        }

        // Wait before next poll
        tokio::time::sleep(tokio::time::Duration::from_millis(interval)).await;
    }
}

/// Cancel active polling
#[command]
pub fn cancel_polling() {
    log::info!("Cancelling session polling");
    POLLING_ACTIVE.store(false, Ordering::SeqCst);
}

/// Check if polling is active
#[command]
pub fn is_polling_active() -> bool {
    POLLING_ACTIVE.load(Ordering::SeqCst)
}

// ============================================================================
// RTSP SIGNALING CLIENT
// ============================================================================

/// RTSP signaling state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RtspSignalingState {
    pub connected: bool,
    pub sdp_offer: Option<String>,
    pub sdp_answer: Option<String>,
    pub server_endpoints: Option<ServerEndpoints>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerEndpoints {
    pub rtsp_handshake_port: u16,
    pub control_port: u16,
    pub udp_control_port: u16,
    pub audio_port: u16,
    pub input_port: u16,
    pub bundle_port: u16,
    pub video_ports: Vec<u16>,
}

/// Connect to signaling server and prepare for WebRTC
/// For browser clients, we use WebSocket on port 443 with /nvst/ path
/// The actual WebSocket connection is handled by the frontend
#[command]
pub async fn connect_rtsp_signaling(
    session_id: String,
    signaling_url: String,
    access_token: String,
) -> Result<RtspSignalingState, String> {
    log::info!("Preparing signaling connection: {}", signaling_url);

    // Get host from signaling URL or session
    let host = if signaling_url.starts_with("wss://") || signaling_url.starts_with("rtsps://") {
        // Extract host from URL
        signaling_url
            .replace("wss://", "")
            .replace("rtsps://", "")
            .split(':')
            .next()
            .unwrap_or("")
            .split('/')
            .next()
            .unwrap_or("")
            .to_string()
    } else {
        // Get from session
        let storage = get_session_storage();
        let guard = storage.lock().await;
        match guard.as_ref() {
            Some(session) => {
                session.server.ip.clone()
                    .ok_or("No server IP in session")?
            }
            None => return Err("No active session".to_string()),
        }
    };

    // Browser client uses port 443 for WebSocket signaling
    let ws_url = format!("wss://{}:443/nvst/", host);
    log::info!("WebSocket signaling URL: {}", ws_url);

    // The actual WebSocket connection is handled by the frontend (streaming.ts)
    // We just return the signaling info and server endpoints

    Ok(RtspSignalingState {
        connected: true,
        sdp_offer: None,
        sdp_answer: None,
        server_endpoints: Some(ServerEndpoints {
            rtsp_handshake_port: 443, // Browser uses 443, not 322
            control_port: 47995,
            udp_control_port: 47999,
            audio_port: 48000,
            input_port: 47995,
            bundle_port: 48001,
            video_ports: vec![47998, 48005, 48008, 48012],
        }),
    })
}

fn parse_rtsp_url(url: &str) -> Result<(String, u16), String> {
    let without_scheme = url.strip_prefix("rtsps://")
        .or_else(|| url.strip_prefix("rtsp://"))
        .ok_or("Invalid RTSP URL")?;

    let parts: Vec<&str> = without_scheme.split(':').collect();
    let host = parts.first().ok_or("No host in URL")?.to_string();
    let port = parts.get(1)
        .and_then(|p| p.split('/').next())
        .and_then(|p| p.parse().ok())
        .unwrap_or(322);

    Ok((host, port))
}

// ============================================================================
// WEBRTC STREAMING
// ============================================================================

/// WebRTC connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebRtcConfig {
    pub session_id: String,
    pub signaling_url: String,
    pub ice_servers: Vec<IceServerConfig>,
    pub video_codec: String,
    pub audio_codec: String,
    pub max_bitrate_kbps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IceServerConfig {
    pub urls: Vec<String>,
    pub username: Option<String>,
    pub credential: Option<String>,
}

// ============================================================================
// WEBSOCKET SIGNALING PROXY
// ============================================================================

use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures_util::{StreamExt, SinkExt};
use std::collections::HashMap;

/// Global WebSocket connections storage
static WS_CONNECTIONS: std::sync::OnceLock<Arc<Mutex<HashMap<String, WsConnection>>>> = std::sync::OnceLock::new();

struct WsConnection {
    // We'll store messages received from server
    messages: Vec<String>,
    connected: bool,
}

fn get_ws_storage() -> Arc<Mutex<HashMap<String, WsConnection>>> {
    WS_CONNECTIONS
        .get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
        .clone()
}

/// Connect to GFN signaling server via native WebSocket
/// This bypasses browser limitations (port 322, custom headers)
#[command]
pub async fn connect_signaling_native(
    session_id: String,
    server_ip: String,
    access_token: String,
) -> Result<String, String> {
    log::info!("Connecting to signaling server natively: {}", server_ip);

    // Build the WebSocket URL for port 322 (RTSPS)
    let ws_url = format!("wss://{}:322/", server_ip);
    log::info!("WebSocket URL: {}", ws_url);

    // Create custom request with headers
    let uri: http::Uri = ws_url.parse()
        .map_err(|e| format!("Invalid URL: {}", e))?;

    let host = uri.host().unwrap_or(&server_ip);

    // Build HTTP request with proper headers for GFN
    let request = http::Request::builder()
        .uri(&ws_url)
        .header("Host", host)
        .header("Authorization", format!("GFNJWT {}", access_token))
        .header("X-GS-Version", "14.2")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", generate_ws_key())
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .body(())
        .map_err(|e| format!("Failed to build request: {}", e))?;

    // Connect using tokio-tungstenite
    match connect_async(request).await {
        Ok((ws_stream, response)) => {
            log::info!("WebSocket connected! Response: {:?}", response.status());

            // Store connection
            let storage = get_ws_storage();
            let mut guard = storage.lock().await;
            guard.insert(session_id.clone(), WsConnection {
                messages: Vec::new(),
                connected: true,
            });

            // Start message handling in background
            let session_id_clone = session_id.clone();
            tokio::spawn(async move {
                handle_ws_messages(session_id_clone, ws_stream).await;
            });

            Ok("Connected".to_string())
        }
        Err(e) => {
            log::error!("WebSocket connection failed: {}", e);

            // Try fallback to port 48322
            let fallback_url = format!("wss://{}:48322/", server_ip);
            log::info!("Trying fallback URL: {}", fallback_url);

            let fallback_request = http::Request::builder()
                .uri(&fallback_url)
                .header("Host", host)
                .header("Authorization", format!("GFNJWT {}", access_token))
                .header("X-GS-Version", "14.2")
                .header("Sec-WebSocket-Version", "13")
                .header("Sec-WebSocket-Key", generate_ws_key())
                .header("Connection", "Upgrade")
                .header("Upgrade", "websocket")
                .body(())
                .map_err(|e| format!("Failed to build fallback request: {}", e))?;

            match connect_async(fallback_request).await {
                Ok((ws_stream, response)) => {
                    log::info!("Fallback WebSocket connected! Response: {:?}", response.status());

                    let storage = get_ws_storage();
                    let mut guard = storage.lock().await;
                    guard.insert(session_id.clone(), WsConnection {
                        messages: Vec::new(),
                        connected: true,
                    });

                    let session_id_clone = session_id.clone();
                    tokio::spawn(async move {
                        handle_ws_messages(session_id_clone, ws_stream).await;
                    });

                    Ok("Connected via fallback".to_string())
                }
                Err(e2) => {
                    Err(format!("All WebSocket connections failed. Primary: {}, Fallback: {}", e, e2))
                }
            }
        }
    }
}

/// Generate WebSocket key for handshake
fn generate_ws_key() -> String {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let random_bytes: [u8; 16] = rand::random();
    STANDARD.encode(random_bytes)
}

/// Handle WebSocket messages in background
async fn handle_ws_messages(
    session_id: String,
    ws_stream: tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
) {
    let (mut write, mut read) = ws_stream.split();

    // Send RTSP OPTIONS to initiate handshake
    let options_request = format!(
        "OPTIONS * RTSP/1.0\r\nCSeq: 1\r\nX-GS-Version: 14.2\r\n\r\n"
    );

    if let Err(e) = write.send(Message::Text(options_request)).await {
        log::error!("Failed to send OPTIONS: {}", e);
        return;
    }

    // Read messages
    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                log::info!("WS received text: {}", &text[..std::cmp::min(200, text.len())]);

                // Store message
                let storage = get_ws_storage();
                let mut guard = storage.lock().await;
                if let Some(conn) = guard.get_mut(&session_id) {
                    conn.messages.push(text);
                }
            }
            Ok(Message::Binary(data)) => {
                log::info!("WS received binary: {} bytes", data.len());
            }
            Ok(Message::Close(_)) => {
                log::info!("WebSocket closed by server");
                break;
            }
            Err(e) => {
                log::error!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }

    // Mark as disconnected
    let storage = get_ws_storage();
    let mut guard = storage.lock().await;
    if let Some(conn) = guard.get_mut(&session_id) {
        conn.connected = false;
    }
}

/// Send message to signaling server
#[command]
pub async fn send_signaling_message(
    session_id: String,
    message: String,
) -> Result<(), String> {
    // This would need to store the write half of the WebSocket
    // For now, just log
    log::info!("Would send to {}: {}", session_id, message);
    Ok(())
}

/// Get received signaling messages
#[command]
pub async fn get_signaling_messages(
    session_id: String,
) -> Result<Vec<String>, String> {
    let storage = get_ws_storage();
    let mut guard = storage.lock().await;

    if let Some(conn) = guard.get_mut(&session_id) {
        let messages = std::mem::take(&mut conn.messages);
        Ok(messages)
    } else {
        Err("No connection found".to_string())
    }
}

/// Check if signaling is connected
#[command]
pub async fn is_signaling_connected(session_id: String) -> bool {
    let storage = get_ws_storage();
    let guard = storage.lock().await;
    guard.get(&session_id).map(|c| c.connected).unwrap_or(false)
}

/// Get WebRTC configuration for streaming
/// Returns the signaling URL and ICE servers needed for browser WebRTC connection
#[command]
pub async fn get_webrtc_config(session_id: String) -> Result<WebRtcConfig, String> {
    let storage = get_session_storage();
    let guard = storage.lock().await;

    let session = guard.as_ref()
        .ok_or("No active session")?;

    // Build signaling URL from session info
    // The signaling_url field may contain:
    // - A full RTSP URL (from native client): rtsps://80-84-170-155.cloudmatchbeta.nvidiagrid.net:322
    // - A path (from browser client): /nvst/
    // We need to extract the hostname and build a WebSocket URL
    let signaling_url = if let Some(ref sig_url) = session.signaling_url {
        if sig_url.starts_with("rtsps://") || sig_url.starts_with("rtsp://") {
            // Native client format: extract hostname from RTSP URL
            // e.g., rtsps://80-84-170-155.cloudmatchbeta.nvidiagrid.net:322
            if let Some(host) = sig_url
                .strip_prefix("rtsps://")
                .or_else(|| sig_url.strip_prefix("rtsp://"))
                .and_then(|s| s.split(':').next())
                .or_else(|| sig_url.split("://").nth(1).and_then(|s| s.split('/').next()))
            {
                format!("wss://{}/nvst/", host)
            } else {
                // Fallback to server IP
                session.server.ip.as_ref()
                    .map(|ip| format!("wss://{}:443/nvst/", ip))
                    .ok_or("No signaling URL available")?
            }
        } else if sig_url.starts_with('/') {
            // Browser client format: path like /nvst/
            session.server.ip.as_ref()
                .map(|ip| format!("wss://{}:443{}", ip, sig_url))
                .ok_or("No signaling URL available")?
        } else {
            // Assume it's already a full WebSocket URL
            sig_url.clone()
        }
    } else {
        // No signaling URL, use server IP with default path
        session.server.ip.as_ref()
            .map(|ip| format!("wss://{}:443/nvst/", ip))
            .ok_or("No signaling URL available")?
    };

    log::info!("WebRTC signaling URL: {}", signaling_url);

    // Determine video codec from quality settings
    let video_codec = match session.quality.codec {
        VideoCodec::H264 => "H264",
        VideoCodec::H265 => "H265",
        VideoCodec::AV1 => "AV1",
    }.to_string();

    Ok(WebRtcConfig {
        session_id: session.session_id.clone(),
        signaling_url,
        ice_servers: vec![
            // NVIDIA's official TURN server (also handles STUN)
            IceServerConfig {
                urls: vec![
                    "stun:turn.gamestream.nvidia.com:19302".to_string(),
                ],
                username: None,
                credential: None,
            },
            // Google STUN servers as fallback
            IceServerConfig {
                urls: vec![
                    "stun:stun.l.google.com:19302".to_string(),
                    "stun:stun1.l.google.com:19302".to_string(),
                ],
                username: None,
                credential: None,
            },
        ],
        video_codec,
        audio_codec: "opus".to_string(),
        max_bitrate_kbps: session.quality.bitrate_kbps,
    })
}

/// Streaming event types for frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StreamingEvent {
    #[serde(rename = "status_update")]
    StatusUpdate {
        phase: String,
        message: String,
        progress: Option<f32>,
    },
    #[serde(rename = "session_ready")]
    SessionReady {
        session_id: String,
        gpu_type: String,
        server_ip: String,
    },
    #[serde(rename = "streaming_started")]
    StreamingStarted {
        resolution: String,
        fps: u32,
        codec: String,
    },
    #[serde(rename = "stats_update")]
    StatsUpdate {
        fps: f32,
        latency_ms: u32,
        bitrate_kbps: u32,
        packet_loss: f32,
    },
    #[serde(rename = "error")]
    Error {
        code: String,
        message: String
    },
    #[serde(rename = "disconnected")]
    Disconnected { reason: String },
}

// ============================================================================
// INPUT HANDLING
// ============================================================================

/// Input event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum InputEvent {
    #[serde(rename = "mouse_move")]
    MouseMove { dx: i32, dy: i32, absolute: bool, x: Option<i32>, y: Option<i32> },

    #[serde(rename = "mouse_button")]
    MouseButton { button: u8, pressed: bool },

    #[serde(rename = "mouse_wheel")]
    MouseWheel { delta_x: i32, delta_y: i32 },

    #[serde(rename = "key")]
    Key { key_code: u32, scan_code: u32, pressed: bool, modifiers: u32 },

    #[serde(rename = "gamepad_button")]
    GamepadButton { gamepad_id: u8, button: u8, pressed: bool, value: f32 },

    #[serde(rename = "gamepad_axis")]
    GamepadAxis { gamepad_id: u8, axis: u8, value: f32 },
}

/// Input state for tracking
#[derive(Debug, Default)]
pub struct InputState {
    pub mouse_captured: bool,
    pub keyboard_captured: bool,
    pub active_gamepads: Vec<u8>,
}

static INPUT_STATE: std::sync::OnceLock<Arc<Mutex<InputState>>> = std::sync::OnceLock::new();

fn get_input_state() -> Arc<Mutex<InputState>> {
    INPUT_STATE
        .get_or_init(|| Arc::new(Mutex::new(InputState::default())))
        .clone()
}

/// Capture input for streaming
#[command]
pub async fn capture_input(capture: bool) -> Result<(), String> {
    let state = get_input_state();
    let mut guard = state.lock().await;
    guard.mouse_captured = capture;
    guard.keyboard_captured = capture;
    log::info!("Input capture: {}", capture);
    Ok(())
}

/// Check if input is captured
#[command]
pub async fn is_input_captured() -> bool {
    let state = get_input_state();
    let guard = state.lock().await;
    guard.mouse_captured || guard.keyboard_captured
}

/// Send input event to stream
/// In a full implementation, this would encode and send via WebRTC data channel
#[command]
pub async fn send_input_event(event: InputEvent) -> Result<(), String> {
    // Check if we have an active session
    let storage = get_session_storage();
    let guard = storage.lock().await;

    if guard.is_none() {
        return Err("No active streaming session".to_string());
    }

    // In a full implementation, this would:
    // 1. Encode the input event into NVST protocol format
    // 2. Send via the input data channel (SCTP)
    // 3. Handle acknowledgment/retry

    log::trace!("Input event: {:?}", event);
    Ok(())
}

// ============================================================================
// FULL STREAMING FLOW
// ============================================================================

/// Start the full streaming flow: poll -> connect -> stream
#[command]
pub async fn start_streaming_flow(
    session_id: String,
    access_token: String,
) -> Result<StreamingConnectionState, String> {
    log::info!("Starting full streaming flow for session: {}", session_id);

    // Phase 1: Poll until session is ready
    log::info!("Phase 1: Polling session status...");
    let connection_state = poll_session_until_ready(
        session_id.clone(),
        access_token.clone(),
        Some(1500),
    ).await?;

    log::info!("Session ready! Phase: {:?}", connection_state.phase);

    // Phase 2: Connect to signaling server
    if let Some(ref signaling_url) = connection_state.signaling_url {
        log::info!("Phase 2: Connecting to signaling server...");
        let _signaling = connect_rtsp_signaling(
            session_id.clone(),
            signaling_url.clone(),
            access_token.clone(),
        ).await?;
        log::info!("Signaling connected");
    }

    // Phase 3: Get WebRTC config for frontend
    log::info!("Phase 3: Preparing WebRTC configuration...");
    let _webrtc_config = get_webrtc_config(session_id.clone()).await?;
    log::info!("WebRTC config ready");

    // Phase 4: Enable input capture
    log::info!("Phase 4: Enabling input capture...");
    capture_input(true).await?;
    log::info!("Input capture enabled");

    Ok(connection_state)
}

/// Stop streaming and cleanup
#[command]
pub async fn stop_streaming_flow(
    session_id: String,
    access_token: String,
) -> Result<(), String> {
    log::info!("Stopping streaming flow for session: {}", session_id);

    // Cancel any active polling
    cancel_polling();

    // Disable input capture
    capture_input(false).await?;

    // Stop the session on the server
    stop_session(session_id, access_token).await?;

    log::info!("Streaming flow stopped");
    Ok(())
}
