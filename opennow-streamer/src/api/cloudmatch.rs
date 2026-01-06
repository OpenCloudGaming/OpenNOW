//! CloudMatch Session API
//!
//! Create and manage GFN streaming sessions.

use anyhow::{Context, Result};
use log::{debug, error, info, warn};

use super::error_codes::SessionError;
use super::GfnApiClient;
use crate::app::session::*;
use crate::app::Settings;
use crate::auth;
use crate::utils::generate_uuid;

/// GFN client version
const GFN_CLIENT_VERSION: &str = "2.0.80.173";

/// User-Agent for native client
const GFN_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36 NVIDIACEFClient/HEAD/debb5919f6 GFN-PC/2.0.80.173";

/// Build CloudMatch zone URL
fn cloudmatch_zone_url(zone: &str) -> String {
    format!("https://{}.cloudmatchbeta.nvidiagrid.net", zone)
}

impl GfnApiClient {
    /// Request a new streaming session using browser-compatible format
    pub async fn create_session(
        &self,
        app_id: &str,
        game_title: &str,
        settings: &Settings,
        zone: &str,
        account_linked: bool,
    ) -> Result<SessionInfo> {
        let token = self.token().context("No access token")?;

        let device_id = generate_uuid();
        let client_id = generate_uuid();
        let sub_session_id = generate_uuid();

        let (width, height) = settings.resolution_tuple();

        // Get timezone offset in milliseconds
        let timezone_offset_ms = chrono::Local::now().offset().local_minus_utc() as i64 * 1000;

        // Build browser-compatible request
        let request = CloudMatchRequest {
            session_request_data: SessionRequestData {
                app_id: app_id.to_string(), // STRING format
                internal_title: Some(game_title.to_string()),
                available_supported_controllers: vec![],
                network_test_session_id: None,
                parent_session_id: None,
                client_identification: "GFN-PC".to_string(),
                device_hash_id: device_id.clone(),
                client_version: "30.0".to_string(),
                sdk_version: "1.0".to_string(),
                streamer_version: 1, // NUMBER format
                client_platform_name: "windows".to_string(),
                client_request_monitor_settings: vec![MonitorSettings {
                    width_in_pixels: width,
                    height_in_pixels: height,
                    frames_per_second: settings.fps,
                    sdr_hdr_mode: if settings.hdr_enabled { 1 } else { 0 },
                    display_data: DisplayData {
                        // HDR luminance values (typical HDR display capabilities)
                        desired_content_max_luminance: if settings.hdr_enabled { 1000 } else { 0 },
                        desired_content_min_luminance: 0,
                        desired_content_max_frame_average_luminance: if settings.hdr_enabled {
                            500
                        } else {
                            0
                        },
                    },
                    dpi: 100,
                }],
                use_ops: true,
                audio_mode: 2, // 5.1 surround
                meta_data: vec![
                    MetaDataEntry {
                        key: "SubSessionId".to_string(),
                        value: sub_session_id,
                    },
                    MetaDataEntry {
                        key: "wssignaling".to_string(),
                        value: "1".to_string(),
                    },
                    MetaDataEntry {
                        key: "GSStreamerType".to_string(),
                        value: "WebRTC".to_string(),
                    },
                    MetaDataEntry {
                        key: "networkType".to_string(),
                        value: "Unknown".to_string(),
                    },
                    MetaDataEntry {
                        key: "ClientImeSupport".to_string(),
                        value: "0".to_string(),
                    },
                    MetaDataEntry {
                        key: "clientPhysicalResolution".to_string(),
                        value: format!(
                            "{{\"horizontalPixels\":{},\"verticalPixels\":{}}}",
                            width, height
                        ),
                    },
                    MetaDataEntry {
                        key: "surroundAudioInfo".to_string(),
                        value: "2".to_string(),
                    },
                ],
                sdr_hdr_mode: if settings.hdr_enabled { 1 } else { 0 },
                client_display_hdr_capabilities: if settings.hdr_enabled {
                    Some(HdrCapabilities {
                        version: 1,
                        hdr_edr_supported_flags_in_uint32: 1, // HDR10 support flag
                        static_metadata_descriptor_id: 0,
                    })
                } else {
                    None
                },
                surround_audio_info: 0,
                remote_controllers_bitmap: 0,
                client_timezone_offset: timezone_offset_ms,
                enhanced_stream_mode: 1,
                app_launch_mode: 1,
                secure_rtsp_supported: false,
                partner_custom_data: Some("".to_string()),
                account_linked,
                enable_persisting_in_game_settings: true,
                user_age: 26,
                requested_streaming_features: Some(StreamingFeatures {
                    reflex: settings.fps >= 120, // Enable Reflex for high refresh rate
                    bit_depth: settings.color_quality.bit_depth(),
                    cloud_gsync: false,
                    enabled_l4s: false,
                    mouse_movement_flags: 0,
                    true_hdr: settings.hdr_enabled,
                    supported_hid_devices: 0,
                    profile: 0,
                    fallback_to_logical_resolution: false,
                    hid_devices: None,
                    chroma_format: settings.color_quality.chroma_format(),
                    prefilter_mode: 0,
                    prefilter_sharpness: 0,
                    prefilter_noise_reduction: 0,
                    hud_streaming_mode: 0,
                }),
            },
        };

        // Check if we're using an Alliance Partner
        let streaming_base_url = auth::get_streaming_base_url();
        let is_alliance_partner = !streaming_base_url.contains("cloudmatchbeta.nvidiagrid.net");

        // Build session URL
        let url = if is_alliance_partner {
            let base = streaming_base_url.trim_end_matches('/');
            format!(
                "{}/v2/session?keyboardLayout=en-US&languageCode=en_US",
                base
            )
        } else {
            format!(
                "{}/v2/session?keyboardLayout=en-US&languageCode=en_US",
                cloudmatch_zone_url(zone)
            )
        };

        info!("Creating session at: {}", url);
        debug!("App ID: {}, Title: {}", app_id, game_title);

        let response = self
            .client
            .post(&url)
            .header("User-Agent", GFN_USER_AGENT)
            .header("Authorization", format!("GFNJWT {}", token))
            .header("Content-Type", "application/json")
            .header("Origin", "https://play.geforcenow.com")
            .header("Referer", "https://play.geforcenow.com/")
            // NV-* headers
            .header("nv-browser-type", "CHROME")
            .header("nv-client-id", &client_id)
            .header("nv-client-streamer", "NVIDIA-CLASSIC")
            .header("nv-client-type", "NATIVE")
            .header("nv-client-version", GFN_CLIENT_VERSION)
            .header("nv-device-make", "UNKNOWN")
            .header("nv-device-model", "UNKNOWN")
            .header("nv-device-os", "WINDOWS")
            .header("nv-device-type", "DESKTOP")
            .header("x-device-id", &device_id)
            .json(&request)
            .send()
            .await
            .context("Session request failed")?;

        let status = response.status();
        let response_text = response.text().await.context("Failed to read response")?;

        debug!(
            "CloudMatch response ({} bytes): {}",
            response_text.len(),
            &response_text[..response_text.len().min(500)]
        );

        if !status.is_success() {
            // Parse error response for user-friendly message
            let session_error = SessionError::from_response(status.as_u16(), &response_text);
            error!(
                "CloudMatch session error: {} - {} (code: {}, unified: {:?})",
                session_error.title,
                session_error.description,
                session_error.gfn_error_code,
                session_error.unified_error_code
            );

            return Err(anyhow::anyhow!(
                "{}: {}",
                session_error.title,
                session_error.description
            ));
        }

        let api_response: CloudMatchResponse =
            serde_json::from_str(&response_text).context("Failed to parse CloudMatch response")?;

        if api_response.request_status.status_code != 1 {
            // Parse error for user-friendly message
            let session_error = SessionError::from_response(200, &response_text);
            error!(
                "CloudMatch API error: {} - {} (statusCode: {}, unified: {})",
                session_error.title,
                session_error.description,
                api_response.request_status.status_code,
                api_response.request_status.unified_error_code
            );

            return Err(anyhow::anyhow!(
                "{}: {}",
                session_error.title,
                session_error.description
            ));
        }

        let session_data = api_response.session;
        info!(
            "Session allocated: {} (status: {})",
            session_data.session_id, session_data.status
        );

        // Determine session state
        let state = Self::parse_session_state(&session_data);

        // Extract connection info
        let server_ip = session_data.streaming_server_ip().unwrap_or_default();
        let signaling_path = session_data.signaling_url();

        // Build full signaling URL
        let signaling_url = signaling_path
            .map(|path| {
                if path.starts_with("wss://") || path.starts_with("rtsps://") {
                    // Already a full URL
                    Self::build_signaling_url(&path, &server_ip)
                } else if path.starts_with('/') {
                    // Path like /nvst/
                    format!("wss://{}:443{}", server_ip, path)
                } else {
                    format!("wss://{}:443/nvst/", server_ip)
                }
            })
            .or_else(|| {
                if !server_ip.is_empty() {
                    Some(format!("wss://{}:443/nvst/", server_ip))
                } else {
                    None
                }
            });

        info!(
            "Stream server: {}, signaling: {:?}",
            server_ip, signaling_url
        );

        // Extract ICE servers and media info before moving other fields
        let ice_servers = session_data.ice_servers();
        let media_connection_info = session_data.media_connection_info();

        // Debug: log connection info
        if let Some(ref conns) = session_data.connection_info {
            for conn in conns {
                info!(
                    "ConnectionInfo: ip={:?} port={} usage={} protocol={}",
                    conn.ip, conn.port, conn.usage, conn.protocol
                );
            }
        } else {
            info!("No connection_info in session response");
        }
        info!("Media connection info: {:?}", media_connection_info);

        Ok(SessionInfo {
            session_id: session_data.session_id,
            server_ip,
            zone: zone.to_string(),
            state,
            gpu_type: session_data.gpu_type,
            signaling_url,
            ice_servers,
            media_connection_info,
        })
    }

    /// Build signaling WebSocket URL from raw path/URL
    fn build_signaling_url(raw: &str, server_ip: &str) -> String {
        if raw.starts_with("rtsps://") || raw.starts_with("rtsp://") {
            // Extract hostname from RTSP URL
            let host = raw
                .strip_prefix("rtsps://")
                .or_else(|| raw.strip_prefix("rtsp://"))
                .and_then(|s| s.split(':').next())
                .filter(|h| !h.is_empty() && !h.starts_with('.'));

            if let Some(h) = host {
                format!("wss://{}/nvst/", h)
            } else {
                // Malformed URL, use server IP
                format!("wss://{}:443/nvst/", server_ip)
            }
        } else if raw.starts_with("wss://") {
            raw.to_string()
        } else if raw.starts_with('/') {
            format!("wss://{}:443{}", server_ip, raw)
        } else {
            format!("wss://{}:443/nvst/", server_ip)
        }
    }

    /// Poll session status until ready
    pub async fn poll_session(
        &self,
        session_id: &str,
        zone: &str,
        server_ip: Option<&str>,
    ) -> Result<SessionInfo> {
        let token = self.token().context("No access token")?;

        let device_id = generate_uuid();
        let client_id = generate_uuid();

        // Check if we're using an Alliance Partner
        let streaming_base_url = auth::get_streaming_base_url();
        let is_alliance_partner = !streaming_base_url.contains("cloudmatchbeta.nvidiagrid.net");

        // Build polling URL - prefer server IP if available
        let poll_base = if is_alliance_partner {
            streaming_base_url.trim_end_matches('/').to_string()
        } else if let Some(ip) = server_ip {
            format!("https://{}", ip)
        } else {
            cloudmatch_zone_url(zone)
        };

        let url = format!("{}/v2/session/{}", poll_base, session_id);

        debug!("Polling session at: {}", url);

        let response = self
            .client
            .get(&url)
            .header("User-Agent", GFN_USER_AGENT)
            .header("Authorization", format!("GFNJWT {}", token))
            .header("Content-Type", "application/json")
            // NV-* headers
            .header("nv-client-id", &client_id)
            .header("nv-client-streamer", "NVIDIA-CLASSIC")
            .header("nv-client-type", "NATIVE")
            .header("nv-client-version", GFN_CLIENT_VERSION)
            .header("nv-device-os", "WINDOWS")
            .header("nv-device-type", "DESKTOP")
            .header("x-device-id", &device_id)
            .send()
            .await
            .context("Poll request failed")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Poll failed: {} - {}", status, body));
        }

        let response_text = response
            .text()
            .await
            .context("Failed to read poll response")?;

        let poll_response: CloudMatchResponse =
            serde_json::from_str(&response_text).context("Failed to parse poll response")?;

        if poll_response.request_status.status_code != 1 {
            let error = poll_response
                .request_status
                .status_description
                .unwrap_or_else(|| "Unknown error".to_string());
            return Err(anyhow::anyhow!("Session poll error: {}", error));
        }

        let session_data = poll_response.session;
        let state = Self::parse_session_state(&session_data);

        let server_ip = session_data.streaming_server_ip().unwrap_or_default();
        let signaling_path = session_data.signaling_url();

        // Build full signaling URL
        let signaling_url = signaling_path
            .map(|path| Self::build_signaling_url(&path, &server_ip))
            .or_else(|| {
                if !server_ip.is_empty() {
                    Some(format!("wss://{}:443/nvst/", server_ip))
                } else {
                    None
                }
            });

        // Extract ICE servers and media info before moving other fields
        let ice_servers = session_data.ice_servers();
        let media_connection_info = session_data.media_connection_info();

        // Debug: log connection info in poll response
        if let Some(ref conns) = session_data.connection_info {
            for conn in conns {
                info!(
                    "Poll ConnectionInfo: ip={:?} port={} usage={} protocol={}",
                    conn.ip, conn.port, conn.usage, conn.protocol
                );
            }
        }
        if media_connection_info.is_some() {
            info!("Poll media connection info: {:?}", media_connection_info);
        }

        Ok(SessionInfo {
            session_id: session_data.session_id,
            server_ip,
            zone: zone.to_string(),
            state,
            gpu_type: session_data.gpu_type,
            signaling_url,
            ice_servers,
            media_connection_info,
        })
    }

    /// Stop a streaming session
    pub async fn stop_session(
        &self,
        session_id: &str,
        zone: &str,
        server_ip: Option<&str>,
    ) -> Result<()> {
        let token = self.token().context("No access token")?;

        let device_id = generate_uuid();

        // Check if we're using an Alliance Partner
        let streaming_base_url = auth::get_streaming_base_url();
        let is_alliance_partner = !streaming_base_url.contains("cloudmatchbeta.nvidiagrid.net");

        // Build delete URL
        let delete_base = if is_alliance_partner {
            streaming_base_url.trim_end_matches('/').to_string()
        } else if let Some(ip) = server_ip {
            format!("https://{}", ip)
        } else {
            cloudmatch_zone_url(zone)
        };

        let url = format!("{}/v2/session/{}", delete_base, session_id);

        info!("Stopping session at: {}", url);

        let response = self
            .client
            .delete(&url)
            .header("User-Agent", GFN_USER_AGENT)
            .header("Authorization", format!("GFNJWT {}", token))
            .header("Content-Type", "application/json")
            .header("x-device-id", &device_id)
            .send()
            .await
            .context("Stop session request failed")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            warn!("Session stop returned: {} - {}", status, body);
        }

        info!("Session stopped: {}", session_id);
        Ok(())
    }

    /// Parse session state from CloudMatch response
    fn parse_session_state(session_data: &CloudMatchSession) -> SessionState {
        // Status 2 = ready for streaming
        if session_data.status == 2 {
            return SessionState::Ready;
        }

        // Status 3 = already streaming
        if session_data.status == 3 {
            return SessionState::Streaming;
        }

        // Check seat setup info for detailed states
        if let Some(ref seat_info) = session_data.seat_setup_info {
            match seat_info.seat_setup_step {
                0 => return SessionState::Connecting,
                1 => {
                    // In queue - show position
                    return SessionState::InQueue {
                        position: seat_info.queue_position.max(0) as u32,
                        eta_secs: (seat_info.seat_setup_eta / 1000).max(0) as u32,
                    };
                }
                5 => return SessionState::CleaningUp,
                6 => return SessionState::WaitingForStorage,
                _ => {
                    // Other steps = general launching/configuring
                    if seat_info.seat_setup_step > 0 {
                        return SessionState::Launching;
                    }
                }
            }
        }

        // Status 1 = setting up
        if session_data.status == 1 {
            return SessionState::Launching;
        }

        // Error states
        if session_data.status <= 0 || session_data.error_code != 0 {
            return SessionState::Error(format!(
                "Error code: {} (status: {})",
                session_data.error_code, session_data.status
            ));
        }

        SessionState::Launching
    }

    /// Get active sessions
    /// Returns list of sessions with status 2 (Ready) or 3 (Streaming)
    pub async fn get_active_sessions(&self) -> Result<Vec<ActiveSessionInfo>> {
        let token = self.token().context("No access token")?;

        let device_id = generate_uuid();
        let client_id = generate_uuid();

        // Get streaming base URL
        let streaming_base_url = auth::get_streaming_base_url();
        let session_url = format!("{}/v2/session", streaming_base_url.trim_end_matches('/'));

        info!("Checking for active sessions at: {}", session_url);

        let response = self
            .client
            .get(&session_url)
            .header("User-Agent", GFN_USER_AGENT)
            .header("Authorization", format!("GFNJWT {}", token))
            .header("Content-Type", "application/json")
            .header("nv-client-id", &client_id)
            .header("nv-client-streamer", "NVIDIA-CLASSIC")
            .header("nv-client-type", "NATIVE")
            .header("nv-client-version", GFN_CLIENT_VERSION)
            .header("nv-device-os", "WINDOWS")
            .header("nv-device-type", "DESKTOP")
            .header("x-device-id", &device_id)
            .send()
            .await
            .context("Failed to get sessions")?;

        let status = response.status();
        let response_text = response.text().await.context("Failed to read response")?;

        if !status.is_success() {
            warn!(
                "Get sessions failed: {} - {}",
                status,
                &response_text[..response_text.len().min(200)]
            );
            return Ok(vec![]);
        }

        debug!("Active sessions response: {} bytes", response_text.len());

        let sessions_response: GetSessionsResponse =
            serde_json::from_str(&response_text).context("Failed to parse sessions response")?;

        if sessions_response.request_status.status_code != 1 {
            warn!(
                "Get sessions API error: {:?}",
                sessions_response.request_status.status_description
            );
            return Ok(vec![]);
        }

        info!(
            "Found {} session(s) from API",
            sessions_response.sessions.len()
        );

        let active_sessions: Vec<ActiveSessionInfo> = sessions_response
            .sessions
            .into_iter()
            .filter(|s| {
                debug!("Session {} has status {}", s.session_id, s.status);
                s.status == 2 || s.status == 3
            })
            .map(|s| {
                let app_id = s
                    .session_request_data
                    .as_ref()
                    .map(|d| d.get_app_id())
                    .unwrap_or(0);

                let server_ip = s.session_control_info.as_ref().and_then(|c| c.ip.clone());

                let signaling_url = s
                    .connection_info
                    .as_ref()
                    .and_then(|conns| conns.iter().find(|c| c.usage == 14))
                    .and_then(|conn| conn.ip.as_ref().map(|ip| format!("wss://{}:443/nvst/", ip)))
                    .or_else(|| {
                        server_ip
                            .as_ref()
                            .map(|ip| format!("wss://{}:443/nvst/", ip))
                    });

                let (resolution, fps) = s
                    .monitor_settings
                    .as_ref()
                    .and_then(|ms| ms.first())
                    .map(|m| {
                        (
                            Some(format!(
                                "{}x{}",
                                m.width_in_pixels.unwrap_or(0),
                                m.height_in_pixels.unwrap_or(0)
                            )),
                            m.frames_per_second,
                        )
                    })
                    .unwrap_or((None, None));

                ActiveSessionInfo {
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

        info!("Found {} active session(s)", active_sessions.len());
        Ok(active_sessions)
    }

    /// Claim/Resume an existing session
    /// Required before connecting to an existing session
    pub async fn claim_session(
        &self,
        session_id: &str,
        server_ip: &str,
        app_id: &str,
        settings: &Settings,
    ) -> Result<SessionInfo> {
        let token = self.token().context("No access token")?;

        let device_id = generate_uuid();
        let client_id = generate_uuid();
        let sub_session_id = generate_uuid();

        let (width, height) = settings.resolution_tuple();

        let timezone_offset_ms = chrono::Local::now().offset().local_minus_utc() as i64 * 1000;

        let claim_url = format!(
            "https://{}/v2/session/{}?keyboardLayout=en-US&languageCode=en_US",
            server_ip, session_id
        );

        info!("Claiming session: {} at {}", session_id, claim_url);

        // Build HDR capabilities JSON if enabled
        // Based on NVIDIA GFN client analysis:
        // - hdrEdrSupportedFlagsInUint32: 1 = HDR10 supported
        // - staticMetadataDescriptorId: 0 = Type 1 (Static Metadata)
        let hdr_capabilities = if settings.hdr_enabled {
            serde_json::json!({
                "version": 1,
                "hdrEdrSupportedFlagsInUint32": 3,  // 1=HDR10, 2=EDR, 3=both
                "staticMetadataDescriptorId": 0,
                "displayData": {
                    "maxLuminance": 1000,
                    "minLuminance": 0.01,
                    "maxFrameAverageLuminance": 500
                }
            })
        } else {
            serde_json::Value::Null
        };

        // HDR colorspace values from NVIDIA GFN client:
        // SDR colorSpace = 2 (BT.709)
        // HDR colorSpace = 4 (BT.2020)
        let sdr_color_space = 2; // BT.709 / YCBCR_LIMITED_BT709
        let hdr_color_space = 4; // BT.2020 / YCBCR_LIMITED_BT2020

        let resume_payload = serde_json::json!({
            "action": 2,
            "data": "RESUME",
            "sessionRequestData": {
                "audioMode": 2,
                "remoteControllersBitmap": 0,
                "sdrHdrMode": if settings.hdr_enabled { 1 } else { 0 },
                "networkTestSessionId": null,
                "availableSupportedControllers": [],
                "clientVersion": "30.0",
                "deviceHashId": device_id,
                "internalTitle": null,
                "clientPlatformName": "windows",
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
                "appId": app_id,
                "streamerVersion": 1,
                "clientRequestMonitorSettings": [{
                    "widthInPixels": width,
                    "heightInPixels": height,
                    "framesPerSecond": settings.fps,
                    "sdrHdrMode": if settings.hdr_enabled { 1 } else { 0 },
                    "displayData": {
                        "desiredContentMaxLuminance": if settings.hdr_enabled { 1000 } else { 0 },
                        "desiredContentMinLuminance": 0,
                        "desiredContentMaxFrameAverageLuminance": if settings.hdr_enabled { 500 } else { 0 }
                    },
                    "dpi": 100
                }],
                "appLaunchMode": 1,
                "sdkVersion": "1.0",
                "enhancedStreamMode": 1,
                "useOps": true,
                "clientDisplayHdrCapabilities": hdr_capabilities,
                "accountLinked": true,
                "partnerCustomData": "",
                "enablePersistingInGameSettings": true,
                "secureRTSPSupported": false,
                "userAge": 26,
                "requestedStreamingFeatures": {
                    "reflex": settings.fps >= 120,
                    "bitDepth": if settings.hdr_enabled { 10 } else { settings.color_quality.bit_depth() },
                    "cloudGsync": false,
                    "enabledL4S": false,
                    "mouseMovementFlags": 0,
                    "trueHdr": settings.hdr_enabled,
                    "supportedHidDevices": 0,
                    "profile": 0,
                    "fallbackToLogicalResolution": false,
                    "hidDevices": null,
                    "chromaFormat": settings.color_quality.chroma_format(),
                    "prefilterMode": 0,
                    "prefilterSharpness": 0,
                    "prefilterNoiseReduction": 0,
                    "hudStreamingMode": 0,
                    "sdrColorSpace": sdr_color_space,
                    "hdrColorSpace": if settings.hdr_enabled { hdr_color_space } else { 0 }
                }
            },
            "metaData": []
        });

        let response = self
            .client
            .put(&claim_url)
            .header("User-Agent", GFN_USER_AGENT)
            .header("Authorization", format!("GFNJWT {}", token))
            .header("Content-Type", "application/json")
            .header("Origin", "https://play.geforcenow.com")
            .header("Referer", "https://play.geforcenow.com/")
            .header("nv-client-id", &client_id)
            .header("nv-client-streamer", "NVIDIA-CLASSIC")
            .header("nv-client-type", "NATIVE")
            .header("nv-client-version", GFN_CLIENT_VERSION)
            .header("nv-device-os", "WINDOWS")
            .header("nv-device-type", "DESKTOP")
            .header("x-device-id", &device_id)
            .json(&resume_payload)
            .send()
            .await
            .context("Claim session request failed")?;

        let status = response.status();
        let response_text = response
            .text()
            .await
            .context("Failed to read claim response")?;

        if !status.is_success() {
            return Err(anyhow::anyhow!(
                "Claim session failed: {} - {}",
                status,
                &response_text[..response_text.len().min(200)]
            ));
        }

        let api_response: CloudMatchResponse =
            serde_json::from_str(&response_text).context("Failed to parse claim response")?;

        if api_response.request_status.status_code != 1 {
            let error = api_response
                .request_status
                .status_description
                .unwrap_or_else(|| "Unknown error".to_string());
            return Err(anyhow::anyhow!("Claim session error: {}", error));
        }

        info!("Session claimed! Polling until ready...");

        let get_url = format!("https://{}/v2/session/{}", server_ip, session_id);

        for attempt in 1..=60 {
            if attempt > 1 {
                tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
            }

            let poll_response = self
                .client
                .get(&get_url)
                .header("User-Agent", GFN_USER_AGENT)
                .header("Authorization", format!("GFNJWT {}", token))
                .header("Content-Type", "application/json")
                .header("nv-client-id", &client_id)
                .header("nv-client-streamer", "NVIDIA-CLASSIC")
                .header("nv-client-type", "NATIVE")
                .header("nv-client-version", GFN_CLIENT_VERSION)
                .header("nv-device-os", "WINDOWS")
                .header("nv-device-type", "DESKTOP")
                .header("x-device-id", &device_id)
                .send()
                .await
                .context("Poll claim request failed")?;

            if !poll_response.status().is_success() {
                continue;
            }

            let poll_text = poll_response
                .text()
                .await
                .context("Failed to read poll response")?;

            let poll_api_response: CloudMatchResponse = match serde_json::from_str(&poll_text) {
                Ok(r) => r,
                Err(_) => continue,
            };

            let session_data = poll_api_response.session;
            debug!(
                "Claim poll attempt {}: status {}",
                attempt, session_data.status
            );

            if session_data.status == 2 || session_data.status == 3 {
                info!("Session ready after claim! Status: {}", session_data.status);

                let state = Self::parse_session_state(&session_data);
                let server_ip_final = session_data
                    .streaming_server_ip()
                    .unwrap_or_else(|| server_ip.to_string());
                let signaling_path = session_data.signaling_url();

                let signaling_url = signaling_path
                    .map(|path| Self::build_signaling_url(&path, &server_ip_final))
                    .or_else(|| Some(format!("wss://{}:443/nvst/", server_ip_final)));

                let ice_servers = session_data.ice_servers();
                let media_connection_info = session_data.media_connection_info();

                return Ok(SessionInfo {
                    session_id: session_data.session_id,
                    server_ip: server_ip_final,
                    zone: String::new(),
                    state,
                    gpu_type: session_data.gpu_type,
                    signaling_url,
                    ice_servers,
                    media_connection_info,
                });
            }

            if session_data.status != 6 {
                break;
            }
        }

        Err(anyhow::anyhow!(
            "Session did not become ready after claiming"
        ))
    }
}
