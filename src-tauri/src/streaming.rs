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
    /// Enable NVIDIA Reflex low-latency mode
    pub reflex: Option<bool>,
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
    app_id: String, // GFN internal app ID as STRING (browser format)
    internal_title: Option<String>,
    available_supported_controllers: Vec<i32>,
    network_test_session_id: Option<String>,
    parent_session_id: Option<String>,
    client_identification: String,
    device_hash_id: String,
    client_version: String,
    sdk_version: String,
    streamer_version: i32, // NUMBER, not string (browser format)
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
    user_age: i32,
    requested_streaming_features: Option<StreamingFeatures>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MonitorSettings {
    width_in_pixels: u32,
    height_in_pixels: u32,
    frames_per_second: u32,
    sdr_hdr_mode: i32,
    display_data: DisplayDataSimple,
    dpi: i32,
}

/// Simplified DisplayData for browser format
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DisplayDataSimple {
    desired_content_max_luminance: i32,
    desired_content_min_luminance: i32,
    desired_content_max_frame_average_luminance: i32,
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

    // Reflex: auto-enable for 120+ FPS if not explicitly set, or use user preference
    let reflex_enabled = request.reflex.unwrap_or(fps >= 120);

    log::info!("Using resolution {}x{} @ {} FPS, codec: {:?}, max bitrate: {} kbps, reflex: {}",
               width, height, fps, codec, max_bitrate_kbps, reflex_enabled);

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
            app_id: request.game_id.clone(), // STRING format like browser ("100013311")
            internal_title: None,
            available_supported_controllers: vec![], // Browser sends empty
            network_test_session_id: None, // Browser sends null
            parent_session_id: None,
            client_identification: "GFN-PC".to_string(),
            device_hash_id: device_id.clone(), // UUID format
            client_version: "30.0".to_string(),
            sdk_version: "1.0".to_string(),
            streamer_version: 1, // NUMBER, not string (browser format)
            client_platform_name: "windows".to_string(), // Native Windows client
            client_request_monitor_settings: vec![MonitorSettings {
                width_in_pixels: width,
                height_in_pixels: height,
                frames_per_second: fps,
                sdr_hdr_mode: 0,
                display_data: DisplayDataSimple {
                    desired_content_max_luminance: 0,
                    desired_content_min_luminance: 0,
                    desired_content_max_frame_average_luminance: 0,
                },
                dpi: 100, // Browser uses 100
            }],
            use_ops: true, // Browser uses true
            audio_mode: 2, // 0=UNKNOWN, 1=STEREO, 2=5.1_SURROUND, 3=7.1_SURROUND
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
            client_display_hdr_capabilities: None,
            surround_audio_info: 0,
            remote_controllers_bitmap: 0,
            client_timezone_offset: timezone_offset_ms,
            enhanced_stream_mode: 1,
            app_launch_mode: 1,
            secure_rtsp_supported: false,
            partner_custom_data: Some("".to_string()),
            account_linked: true, // Browser uses true
            enable_persisting_in_game_settings: false,
            user_age: 26, // Use a reasonable default age
            requested_streaming_features: Some(StreamingFeatures {
                reflex: reflex_enabled, // NVIDIA Reflex low-latency mode
                bit_depth: 0,
                cloud_gsync: false,
                enabled_l4s: false,
                mouse_movement_flags: 0,
                true_hdr: false,
                supported_hid_devices: 0,
                profile: 0,
                fallback_to_logical_resolution: false,
                hid_devices: None,
                chroma_format: 0,
                prefilter_mode: 0,
                prefilter_sharpness: 0,
                prefilter_noise_reduction: 0,
                hud_streaming_mode: 0,
            }),
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

/// Active session info returned from the server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveSession {
    pub session_id: String,
    pub app_id: i64,
    pub gpu_type: Option<String>,
    pub status: i32,
    pub server_ip: Option<String>,
    pub signaling_url: Option<String>,
    pub resolution: Option<String>,
    pub fps: Option<u32>,
}

/// Response from GET /v2/session endpoint
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetSessionsResponse {
    #[serde(default)]
    sessions: Vec<SessionFromApi>,
    request_status: RequestStatus,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionFromApi {
    session_id: String,
    #[serde(default)]
    session_request_data: Option<SessionRequestDataFromApi>,
    #[serde(default)]
    gpu_type: Option<String>,
    #[serde(default)]
    status: i32,
    #[serde(default)]
    session_control_info: Option<SessionControlInfo>,
    #[serde(default)]
    connection_info: Option<Vec<ConnectionInfo>>,
    #[serde(default)]
    monitor_settings: Option<Vec<MonitorSettingsFromApi>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionRequestDataFromApi {
    #[serde(default)]
    app_id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MonitorSettingsFromApi {
    #[serde(default)]
    width_in_pixels: u32,
    #[serde(default)]
    height_in_pixels: u32,
    #[serde(default)]
    frames_per_second: u32,
}

/// Get active sessions from the CloudMatch server
/// This checks if there are any running sessions that can be reconnected to
#[command]
pub async fn get_active_sessions(
    access_token: String,
) -> Result<Vec<ActiveSession>, String> {
    log::info!("Checking for active sessions...");

    let client = crate::proxy::create_proxied_client().await?;
    let device_id = get_device_id();
    let client_id = get_client_id();

    // Use the prod endpoint which returns all user sessions
    let session_url = format!("{}/v2/session", CLOUDMATCH_PROD_URL);

    log::info!("Using device_id: {} for session check", device_id);

    let response = client
        .get(&session_url)
        .header("Authorization", format!("GFNJWT {}", access_token))
        .header("Content-Type", "application/json")
        .header("Origin", "https://play.geforcenow.com")
        .header("nv-client-id", &client_id)
        .header("nv-client-streamer", "WEBRTC")
        .header("nv-client-type", "BROWSER")
        .header("nv-client-version", "2.0.80.173")
        .header("nv-browser-type", "CHROMIUM")
        .header("nv-device-make", "APPLE")
        .header("nv-device-model", "UNKNOWN")
        .header("nv-device-os", "MACOS")
        .header("nv-device-type", "DESKTOP")
        .header("x-device-id", &device_id)
        .send()
        .await
        .map_err(|e| format!("Failed to check sessions: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        log::warn!("Get sessions failed: {} - {}", status, body);
        return Ok(vec![]); // Return empty on error, don't fail
    }

    let response_text = response.text().await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    log::info!("Active sessions raw response length: {} bytes", response_text.len());

    // Try to parse as generic JSON first to see what we got
    if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&response_text) {
        if let Some(sessions_arr) = json_value.get("sessions").and_then(|s| s.as_array()) {
            log::info!("Raw sessions array contains {} items", sessions_arr.len());
            for (i, session) in sessions_arr.iter().enumerate() {
                let status = session.get("status").and_then(|s| s.as_i64()).unwrap_or(-1);
                let session_id = session.get("sessionId").and_then(|s| s.as_str()).unwrap_or("unknown");
                log::info!("  Session {}: id={}, status={}", i, session_id, status);
            }
        }
    }

    let sessions_response: GetSessionsResponse = serde_json::from_str(&response_text)
        .map_err(|e| {
            log::error!("Failed to parse sessions: {}", e);
            format!("Failed to parse sessions response: {}", e)
        })?;

    if sessions_response.request_status.status_code != 1 {
        log::warn!("Get sessions API error: {:?}", sessions_response.request_status.status_description);
        return Ok(vec![]);
    }

    log::info!("Parsed {} sessions from response", sessions_response.sessions.len());

    // Convert to our ActiveSession struct
    // Session status values:
    // 1 = Queued/Pending
    // 2 = Ready/Connecting
    // 3 = Running/Streaming (in-game)
    // 4+ = Stopping/Stopped/Error
    let active_sessions: Vec<ActiveSession> = sessions_response.sessions
        .into_iter()
        .filter(|s| {
            log::info!("Session {} has status {}", s.session_id, s.status);
            s.status == 2 || s.status == 3 // Include both ready and running states
        })
        .map(|s| {
            let app_id = s.session_request_data
                .as_ref()
                .map(|d| d.app_id)
                .unwrap_or(0);

            let server_ip = s.session_control_info
                .as_ref()
                .and_then(|c| c.ip.clone());

            // Try to get signaling URL from connection_info first, then fall back to server_ip
            let signaling_url = s.connection_info
                .as_ref()
                .and_then(|conns| conns.first())
                .and_then(|conn| {
                    conn.ip.as_ref().map(|ip| format!("wss://{}:443/nvst/", ip))
                })
                .or_else(|| {
                    // Fall back to server_ip if connection_info doesn't have the IP
                    server_ip.as_ref().map(|ip| format!("wss://{}:443/nvst/", ip))
                });

            log::info!("Session {} server_ip: {:?}, signaling_url: {:?}",
                       s.session_id, server_ip, signaling_url);

            let (resolution, fps) = s.monitor_settings
                .as_ref()
                .and_then(|ms| ms.first())
                .map(|m| (
                    Some(format!("{}x{}", m.width_in_pixels, m.height_in_pixels)),
                    Some(m.frames_per_second)
                ))
                .unwrap_or((None, None));

            ActiveSession {
                session_id: s.session_id,
                app_id,
                gpu_type: s.gpu_type,
                status: s.status,
                server_ip,
                signaling_url,
                resolution,
                fps,
            }
        })
        .collect();

    log::info!("Found {} active session(s) after filtering", active_sessions.len());
    Ok(active_sessions)
}

/// Claim/Resume an active session by sending a PUT request
/// This is required before connecting to an existing session
/// The browser client makes this request to "activate" the session for streaming
#[command]
pub async fn claim_session(
    session_id: String,
    server_ip: String,
    access_token: String,
    app_id: String,
    resolution: Option<String>,
    fps: Option<u32>,
) -> Result<ClaimSessionResponse, String> {
    log::info!("Claiming session: {} on server {} for app {}", session_id, server_ip, app_id);

    let client = crate::proxy::create_proxied_client().await?;
    let device_id = get_device_id();
    let client_id = get_client_id();
    let sub_session_id = uuid::Uuid::new_v4().to_string();

    // Parse resolution
    let (width, height) = parse_resolution(resolution.as_deref());
    let fps_val = fps.unwrap_or(60);

    // Get timezone offset in milliseconds
    let timezone_offset_ms = chrono::Local::now().offset().local_minus_utc() as i64 * 1000;

    // Build the PUT URL - use the server IP directly like the browser does
    // Format: PUT https://{server_ip}/v2/session/{sessionId}?keyboardLayout=m-us&languageCode=en_US
    let claim_url = format!(
        "https://{}/v2/session/{}?keyboardLayout=m-us&languageCode=en_US",
        server_ip, session_id
    );

    log::info!("Claim URL: {}", claim_url);

    // Build the RESUME payload matching browser client format exactly
    // Note: appId must be a STRING, not a number
    let resume_payload = serde_json::json!({
        "action": 2,
        "data": "RESUME",
        "sessionRequestData": {
            "audioMode": 2,
            "remoteControllersBitmap": 0,
            "sdrHdrMode": 0,
            "networkTestSessionId": null,
            "availableSupportedControllers": [],
            "clientVersion": "30.0",
            "deviceHashId": device_id,
            "internalTitle": null,
            "clientPlatformName": "browser",
            "metaData": [
                {"key": "SubSessionId", "value": sub_session_id},
                {"key": "wssignaling", "value": "1"},
                {"key": "GSStreamerType", "value": "WebRTC"},
                {"key": "networkType", "value": "Unknown"},
                {"key": "ClientImeSupport", "value": "0"},
                {"key": "clientPhysicalResolution", "value": format!("{{\"horizontalPixels\":{},\"verticalPixels\":{}}}", width, height)},
                {"key": "surroundAudioInfo", "value": "2"}
            ],
            "surroundAudioInfo": 0,
            "clientTimezoneOffset": timezone_offset_ms,
            "clientIdentification": "GFN-PC",
            "parentSessionId": null,
            "appId": app_id, // Must be string like "106466949"
            "streamerVersion": 1,
            "clientRequestMonitorSettings": [{
                "widthInPixels": width,
                "heightInPixels": height,
                "framesPerSecond": fps_val,
                "sdrHdrMode": 0,
                "displayData": {
                    "desiredContentMaxLuminance": 0,
                    "desiredContentMinLuminance": 0,
                    "desiredContentMaxFrameAverageLuminance": 0
                },
                "dpi": 100
            }],
            "appLaunchMode": 1,
            "sdkVersion": "1.0",
            "enhancedStreamMode": 1,
            "useOps": true,
            "clientDisplayHdrCapabilities": null,
            "accountLinked": true,
            "partnerCustomData": "",
            "enablePersistingInGameSettings": false,
            "secureRTSPSupported": false,
            "userAge": 26,
            "requestedStreamingFeatures": {
                "reflex": false,
                "bitDepth": 0,
                "cloudGsync": false,
                "enabledL4S": false,
                "profile": 1,
                "fallbackToLogicalResolution": false,
                "chromaFormat": 0,
                "prefilterMode": 0,
                "hudStreamingMode": 0
            }
        },
        "metaData": []
    });

    log::info!("Sending RESUME payload for session claim: appId={}", app_id);

    let response = client
        .put(&claim_url)
        .header("Authorization", format!("GFNJWT {}", access_token))
        .header("Content-Type", "application/json")
        .header("Origin", "https://play.geforcenow.com")
        .header("nv-client-id", &client_id)
        .header("nv-client-streamer", "WEBRTC")
        .header("nv-client-type", "BROWSER")
        .header("nv-client-version", "2.0.80.173")
        .header("nv-browser-type", "CHROMIUM")
        .header("nv-device-os", "MACOS")
        .header("nv-device-type", "DESKTOP")
        .header("x-device-id", &device_id)
        .json(&resume_payload)
        .send()
        .await
        .map_err(|e| format!("Failed to claim session: {}", e))?;

    let status_code = response.status();
    if !status_code.is_success() {
        let body = response.text().await.unwrap_or_default();
        log::error!("Claim session failed: {} - {}", status_code, body);
        return Err(format!("Claim session failed: {} - {}", status_code, body));
    }

    let response_text = response.text().await
        .map_err(|e| format!("Failed to read claim response: {}", e))?;

    log::info!("Claim session response: {}", &response_text[..std::cmp::min(500, response_text.len())]);

    // Parse the response to get updated session info
    let claim_response: ClaimSessionApiResponse = serde_json::from_str(&response_text)
        .map_err(|e| format!("Failed to parse claim response: {}", e))?;

    if claim_response.request_status.status_code != 1 {
        let error = claim_response.request_status.status_description
            .unwrap_or_else(|| "Unknown error".to_string());
        return Err(format!("Claim session API error: {}", error));
    }

    let session = claim_response.session;
    log::info!("Session claimed! Status: {}, GPU: {:?}", session.status, session.gpu_type);

    // After claiming, we need to poll GET the session info until status changes from 6 to 2
    // The official browser client does this - it waits for the session to be "ready" (status 2)
    // before connecting. Status 6 is a transitional state during claim.
    log::info!("Polling session info until ready (status 2)...");

    let get_url = format!(
        "https://{}/v2/session/{}",
        server_ip, session_id
    );

    let mut updated_session: Option<ClaimSessionData> = None;
    let max_attempts = 10;

    for attempt in 1..=max_attempts {
        log::info!("GET session attempt {}/{}", attempt, max_attempts);

        // Small delay between attempts (except first)
        if attempt > 1 {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }

        let get_response = client
            .get(&get_url)
            .header("Authorization", format!("GFNJWT {}", access_token))
            .header("Content-Type", "application/json")
            .header("Origin", "https://play.geforcenow.com")
            .header("nv-client-id", &client_id)
            .header("nv-client-streamer", "WEBRTC")
            .header("nv-client-type", "BROWSER")
            .header("nv-client-version", "2.0.80.173")
            .header("nv-browser-type", "CHROMIUM")
            .header("nv-device-os", "MACOS")
            .header("nv-device-type", "DESKTOP")
            .header("x-device-id", &device_id)
            .send()
            .await
            .map_err(|e| format!("Failed to get session info: {}", e))?;

        if !get_response.status().is_success() {
            let body = get_response.text().await.unwrap_or_default();
            log::warn!("GET session info failed on attempt {}: {}", attempt, body);
            continue;
        }

        let get_response_text = get_response.text().await
            .map_err(|e| format!("Failed to read GET session response: {}", e))?;

        log::info!("GET session response (attempt {}): {}", attempt, &get_response_text[..std::cmp::min(300, get_response_text.len())]);

        let get_session_response: ClaimSessionApiResponse = match serde_json::from_str(&get_response_text) {
            Ok(r) => r,
            Err(e) => {
                log::warn!("Failed to parse GET session response on attempt {}: {}", attempt, e);
                continue;
            }
        };

        let sess = get_session_response.session;
        log::info!("Session status on attempt {}: {}", attempt, sess.status);

        // Status 2 = Ready, Status 3 = Running - both are good to connect
        if sess.status == 2 || sess.status == 3 {
            log::info!("Session is ready! Status: {}", sess.status);
            updated_session = Some(sess);
            break;
        }

        // If still status 6, keep polling
        if sess.status == 6 {
            log::info!("Session still transitioning (status 6), continuing to poll...");
            // Store in case we run out of attempts
            updated_session = Some(sess);
        } else {
            // Some other status, use it
            log::info!("Session in status {}, using this", sess.status);
            updated_session = Some(sess);
            break;
        }
    }

    // Use the session data we got (either status 2/3 or the last status 6)
    let updated_session = updated_session.ok_or_else(|| {
        format!("Failed to get session info after {} attempts", max_attempts)
    })?;

    log::info!("Final session status: {}, GPU: {:?}", updated_session.status, updated_session.gpu_type);

    // Extract signaling URL from updated connection info
    let signaling_url = updated_session.connection_info
        .as_ref()
        .and_then(|conns| conns.first())
        .and_then(|conn| {
            log::info!("Connection info IP from GET response: {:?}", conn.ip);
            conn.ip.as_ref().map(|ip| format!("wss://{}:443/nvst/", ip))
        });

    log::info!("Final signaling URL being returned: {:?}", signaling_url);

    Ok(ClaimSessionResponse {
        session_id: updated_session.session_id,
        status: updated_session.status,
        gpu_type: updated_session.gpu_type,
        signaling_url,
        server_ip: updated_session.session_control_info
            .and_then(|c| c.ip),
    })
}

/// Response from claim session
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimSessionResponse {
    pub session_id: String,
    pub status: i32,
    pub gpu_type: Option<String>,
    pub signaling_url: Option<String>,
    pub server_ip: Option<String>,
}

/// API response for PUT /v2/session/{sessionId}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaimSessionApiResponse {
    session: ClaimSessionData,
    request_status: RequestStatus,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaimSessionData {
    session_id: String,
    #[serde(default)]
    status: i32,
    #[serde(default)]
    gpu_type: Option<String>,
    #[serde(default)]
    session_control_info: Option<SessionControlInfo>,
    #[serde(default)]
    connection_info: Option<Vec<ConnectionInfo>>,
}

/// Set up session storage for reconnecting to an existing session
/// This is used when we detect an active session and want to connect to it
#[command]
pub async fn setup_reconnect_session(
    session_id: String,
    server_ip: String,
    signaling_url: String,
    _gpu_type: Option<String>,
) -> Result<(), String> {
    log::info!("Setting up reconnect session: {} on {}", session_id, server_ip);

    let session = StreamingSession {
        session_id: session_id.clone(),
        game_id: String::new(), // Unknown for reconnect
        server: SessionServer {
            id: String::new(),
            name: String::from("Reconnected"),
            region: String::new(),
            ip: Some(server_ip.clone()),
            zone: None,
        },
        status: SessionStatus::Running,
        quality: StreamingQuality {
            resolution: Resolution::R1080p,
            fps: 60,
            bitrate_kbps: 50000,
            codec: VideoCodec::H264,
            hdr_enabled: false,
        },
        stats: None,
        webrtc_offer: None,
        signaling_url: Some(signaling_url),
    };

    let storage = get_session_storage();
    let mut guard = storage.lock().await;
    *guard = Some(session);

    log::info!("Reconnect session setup complete");
    Ok(())
}

/// Terminate an active session
#[command]
pub async fn terminate_session(
    session_id: String,
    access_token: String,
) -> Result<(), String> {
    log::info!("Terminating session: {}", session_id);

    let client = crate::proxy::create_proxied_client().await?;
    let device_id = get_device_id();

    // Try to delete from prod endpoint
    let delete_url = format!("{}/v2/session/{}", CLOUDMATCH_PROD_URL, session_id);

    let response = client
        .delete(&delete_url)
        .header("Authorization", format!("GFNJWT {}", access_token))
        .header("Content-Type", "application/json")
        .header("x-device-id", &device_id)
        .send()
        .await
        .map_err(|e| format!("Failed to terminate session: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        log::warn!("Session termination returned: {} - {}", status, body);
        // Don't fail even if termination reports error - session might already be gone
    }

    log::info!("Session terminated: {}", session_id);
    Ok(())
}

/// Start streaming flow - polls session until ready
/// This is the main entry point called by the frontend after start_session
#[command]
pub async fn start_streaming_flow(
    session_id: String,
    access_token: String,
) -> Result<StreamingConnectionState, String> {
    log::info!("Starting streaming flow for session: {}", session_id);

    // Poll the session until it's ready
    poll_session_until_ready(session_id, access_token, None).await
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

    // Stop the session on the server
    stop_session(session_id, access_token).await?;

    log::info!("Streaming flow stopped");
    Ok(())
}
