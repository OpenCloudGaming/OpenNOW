//! WebRTC Module
//!
//! WebRTC peer connection, signaling, and data channels for GFN streaming.

mod signaling;
mod peer;
mod sdp;
mod datachannel;

pub use signaling::{GfnSignaling, SignalingEvent, IceCandidate};
pub use peer::{WebRtcPeer, WebRtcEvent, request_keyframe};
pub use sdp::*;
pub use datachannel::*;

use std::sync::Arc;
use parking_lot::Mutex;
use tokio::sync::mpsc;
use anyhow::{Result, Context};
use log::{info, warn, error, debug};
use webrtc::ice_transport::ice_server::RTCIceServer;

use crate::app::{SessionInfo, Settings, VideoCodec, SharedFrame};
use crate::media::{VideoFrame, StreamStats, VideoDecoder, AudioDecoder, AudioPlayer, RtpDepacketizer, DepacketizerCodec, DecodeStats};
use crate::input::InputHandler;

/// Active streaming session
pub struct StreamingSession {
    pub signaling: Option<GfnSignaling>,
    pub peer: Option<WebRtcPeer>,
    pub connected: bool,
    pub stats: StreamStats,
    pub input_ready: bool,
}

impl StreamingSession {
    pub fn new() -> Self {
        Self {
            signaling: None,
            peer: None,
            connected: false,
            stats: StreamStats::default(),
            input_ready: false,
        }
    }
}

impl Default for StreamingSession {
    fn default() -> Self {
        Self::new()
    }
}

/// Build nvstSdp string with streaming parameters
/// Based on official GFN browser client format
fn build_nvst_sdp(
    ice_ufrag: &str,
    ice_pwd: &str,
    fingerprint: &str,
    width: u32,
    height: u32,
    fps: u32,
    max_bitrate_kbps: u32,
) -> String {
    let min_bitrate_kbps = std::cmp::min(10000, max_bitrate_kbps / 10);
    let initial_bitrate_kbps = max_bitrate_kbps / 2;

    let is_high_fps = fps >= 120;
    let is_120_fps = fps == 120;
    let is_240_fps = fps >= 240;

    let mut lines = vec![
        "v=0".to_string(),
        "o=SdpTest test_id_13 14 IN IPv4 127.0.0.1".to_string(),
        "s=-".to_string(),
        "t=0 0".to_string(),
        format!("a=general.icePassword:{}", ice_pwd),
        format!("a=general.iceUserNameFragment:{}", ice_ufrag),
        format!("a=general.dtlsFingerprint:{}", fingerprint),
        "m=video 0 RTP/AVP".to_string(),
        "a=msid:fbc-video-0".to_string(),
        // FEC settings
        "a=vqos.fec.rateDropWindow:10".to_string(),
        "a=vqos.fec.minRequiredFecPackets:2".to_string(),
        "a=vqos.fec.repairMinPercent:5".to_string(),
        "a=vqos.fec.repairPercent:5".to_string(),
        "a=vqos.fec.repairMaxPercent:35".to_string(),
    ];

    // DRC/DFC settings based on FPS
    if is_high_fps {
        lines.push("a=vqos.drc.enable:0".to_string());
        lines.push("a=vqos.dfc.enable:1".to_string());
        lines.push("a=vqos.dfc.decodeFpsAdjPercent:85".to_string());
        lines.push("a=vqos.dfc.targetDownCooldownMs:250".to_string());
        lines.push("a=vqos.dfc.dfcAlgoVersion:2".to_string());
        lines.push(format!("a=vqos.dfc.minTargetFps:{}", if is_120_fps { 100 } else { 60 }));
    } else {
        lines.push("a=vqos.drc.minRequiredBitrateCheckEnabled:1".to_string());
    }

    // Video encoder settings
    lines.extend(vec![
        "a=video.dx9EnableNv12:1".to_string(),
        "a=video.dx9EnableHdr:1".to_string(),
        "a=vqos.qpg.enable:1".to_string(),
        "a=vqos.resControl.qp.qpg.featureSetting:7".to_string(),
        "a=bwe.useOwdCongestionControl:1".to_string(),
        "a=video.enableRtpNack:1".to_string(),
        "a=vqos.bw.txRxLag.minFeedbackTxDeltaMs:200".to_string(),
        "a=vqos.drc.bitrateIirFilterFactor:18".to_string(),
        "a=video.packetSize:1140".to_string(),
        "a=packetPacing.minNumPacketsPerGroup:15".to_string(),
    ]);

    // High FPS optimizations
    if is_high_fps {
        lines.extend(vec![
            "a=bwe.iirFilterFactor:8".to_string(),
            "a=video.encoderFeatureSetting:47".to_string(),
            "a=video.encoderPreset:6".to_string(),
            "a=vqos.resControl.cpmRtc.badNwSkipFramesCount:600".to_string(),
            "a=vqos.resControl.cpmRtc.decodeTimeThresholdMs:9".to_string(),
            format!("a=video.fbcDynamicFpsGrabTimeoutMs:{}", if is_120_fps { 6 } else { 18 }),
            format!("a=vqos.resControl.cpmRtc.serverResolutionUpdateCoolDownCount:{}", if is_120_fps { 6000 } else { 12000 }),
        ]);
    }

    // 240+ FPS optimizations
    if is_240_fps {
        lines.extend(vec![
            "a=video.enableNextCaptureMode:1".to_string(),
            "a=vqos.maxStreamFpsEstimate:240".to_string(),
            "a=video.videoSplitEncodeStripsPerFrame:3".to_string(),
            "a=video.updateSplitEncodeStateDynamically:1".to_string(),
        ]);
    }

    // Out of focus and additional settings
    lines.extend(vec![
        "a=vqos.adjustStreamingFpsDuringOutOfFocus:1".to_string(),
        "a=vqos.resControl.cpmRtc.ignoreOutOfFocusWindowState:1".to_string(),
        "a=vqos.resControl.perfHistory.rtcIgnoreOutOfFocusWindowState:1".to_string(),
        "a=vqos.resControl.cpmRtc.featureMask:3".to_string(),
        format!("a=packetPacing.numGroups:{}", if is_120_fps { 3 } else { 5 }),
        "a=packetPacing.maxDelayUs:1000".to_string(),
        "a=packetPacing.minNumPacketsFrame:10".to_string(),
        // NACK settings
        "a=video.rtpNackQueueLength:1024".to_string(),
        "a=video.rtpNackQueueMaxPackets:512".to_string(),
        "a=video.rtpNackMaxPacketCount:25".to_string(),
        // Resolution/quality
        "a=vqos.drc.qpMaxResThresholdAdj:4".to_string(),
        "a=vqos.grc.qpMaxResThresholdAdj:4".to_string(),
        "a=vqos.drc.iirFilterFactor:100".to_string(),
        // Viewport and FPS
        format!("a=video.clientViewportWd:{}", width),
        format!("a=video.clientViewportHt:{}", height),
        format!("a=video.maxFPS:{}", fps),
        // Bitrate
        format!("a=video.initialBitrateKbps:{}", initial_bitrate_kbps),
        format!("a=video.initialPeakBitrateKbps:{}", initial_bitrate_kbps),
        format!("a=vqos.bw.maximumBitrateKbps:{}", max_bitrate_kbps),
        format!("a=vqos.bw.minimumBitrateKbps:{}", min_bitrate_kbps),
        // Encoder settings
        "a=video.maxNumReferenceFrames:4".to_string(),
        "a=video.mapRtpTimestampsToFrames:1".to_string(),
        "a=video.encoderCscMode:3".to_string(),
        "a=video.scalingFeature1:0".to_string(),
        "a=video.prefilterParams.prefilterModel:0".to_string(),
        // Audio track
        "m=audio 0 RTP/AVP".to_string(),
        "a=msid:audio".to_string(),
        // Mic track
        "m=mic 0 RTP/AVP".to_string(),
        "a=msid:mic".to_string(),
        // Input/application track
        "m=application 0 RTP/AVP".to_string(),
        "a=msid:input_1".to_string(),
        "a=ri.partialReliableThresholdMs:300".to_string(),
        "".to_string(),
    ]);

    lines.join("\n")
}

/// Extract ICE credentials from SDP
fn extract_ice_credentials(sdp: &str) -> (String, String, String) {
    let ufrag = sdp.lines()
        .find(|l| l.starts_with("a=ice-ufrag:"))
        .map(|l| l.trim_start_matches("a=ice-ufrag:").to_string())
        .unwrap_or_default();

    let pwd = sdp.lines()
        .find(|l| l.starts_with("a=ice-pwd:"))
        .map(|l| l.trim_start_matches("a=ice-pwd:").to_string())
        .unwrap_or_default();

    let fingerprint = sdp.lines()
        .find(|l| l.starts_with("a=fingerprint:sha-256 "))
        .map(|l| l.trim_start_matches("a=fingerprint:sha-256 ").to_string())
        .unwrap_or_default();

    (ufrag, pwd, fingerprint)
}

/// Extract public IP from server hostname (e.g., "95-178-87-234.zai..." -> "95.178.87.234")
fn extract_public_ip(server_ip: &str) -> Option<String> {
    let re = regex::Regex::new(r"^(\d+-\d+-\d+-\d+)\.").ok()?;
    re.captures(server_ip)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().replace('-', "."))
}

/// Run the streaming session
pub async fn run_streaming(
    session_info: SessionInfo,
    settings: Settings,
    shared_frame: Arc<SharedFrame>,
    stats_tx: mpsc::Sender<StreamStats>,
    input_handler: Arc<InputHandler>,
) -> Result<()> {
    info!("Starting streaming to {} with session {}", session_info.server_ip, session_info.session_id);

    let (width, height) = settings.resolution_tuple();
    let fps = settings.fps;
    let max_bitrate = settings.max_bitrate_kbps();
    let codec = settings.codec;
    let codec_str = codec.as_str().to_string();

    // Create signaling client
    let (sig_event_tx, mut sig_event_rx) = mpsc::channel::<SignalingEvent>(64);
    let server_ip = session_info.signaling_url
        .as_ref()
        .and_then(|url| {
            url.split("://").nth(1).and_then(|s| s.split('/').next())
        })
        .unwrap_or(&session_info.server_ip)
        .to_string();

    let mut signaling = GfnSignaling::new(
        server_ip.clone(),
        session_info.session_id.clone(),
        sig_event_tx,
    );

    // Connect to signaling
    signaling.connect().await?;
    info!("Signaling connected");

    // Create WebRTC peer
    let (peer_event_tx, mut peer_event_rx) = mpsc::channel(64);
    let mut peer = WebRtcPeer::new(peer_event_tx);

    // Video decoder - use async mode for non-blocking decode
    // Decoded frames are written directly to SharedFrame by the decoder thread
    let (mut video_decoder, mut decode_stats_rx) = VideoDecoder::new_async(codec, shared_frame.clone())?;

    // Create RTP depacketizer with correct codec
    let depacketizer_codec = match codec {
        VideoCodec::H264 => DepacketizerCodec::H264,
        VideoCodec::H265 => DepacketizerCodec::H265,
        VideoCodec::AV1 => DepacketizerCodec::H264, // AV1 uses different packetization, fallback for now
    };
    let mut rtp_depacketizer = RtpDepacketizer::with_codec(depacketizer_codec);
    info!("RTP depacketizer using {:?} mode", depacketizer_codec);

    let mut audio_decoder = AudioDecoder::new(48000, 2)?;

    // Audio player is created in a separate thread due to cpal::Stream not being Send
    let (audio_tx, mut audio_rx) = mpsc::channel::<Vec<i16>>(32);
    std::thread::spawn(move || {
        if let Ok(audio_player) = AudioPlayer::new(48000, 2) {
            info!("Audio player thread started");
            while let Some(samples) = audio_rx.blocking_recv() {
                audio_player.push_samples(&samples);
            }
        } else {
            warn!("Failed to create audio player - audio disabled");
        }
    });

    // Stats tracking
    let mut stats = StreamStats::default();
    let mut last_stats_time = std::time::Instant::now();
    let mut frames_received: u64 = 0;
    let mut frames_decoded: u64 = 0;
    let mut frames_dropped: u64 = 0;
    let mut bytes_received: u64 = 0;
    let mut last_frames_decoded: u64 = 0; // For actual FPS calculation

    // Pipeline latency tracking (receive to decode complete)
    let mut pipeline_latency_sum: f64 = 0.0;
    let mut pipeline_latency_count: u64 = 0;

    // Input latency tracking (event creation to transmission)
    let mut input_latency_sum: f64 = 0.0;
    let mut input_latency_count: u64 = 0;

    // Input state - use atomic for cross-task communication
    // input_ready_flag and input_protocol_version_shared are created later with the input task

    // Input channel - connect InputHandler to the streaming loop
    // Large buffer (1024) to handle high-frequency mouse events without blocking
    let (input_event_tx, input_event_rx) = mpsc::channel::<InputEvent>(1024);
    input_handler.set_event_sender(input_event_tx.clone());

    // Also set raw input sender for direct mouse events (Windows/macOS)
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    crate::input::set_raw_input_sender(input_event_tx);

    info!("Input handler connected to streaming loop");

    // Channel for input task to send encoded packets to the WebRTC peer
    // This decouples input processing from video decoding completely
    // Tuple: (encoded_data, is_mouse, latency_us)
    let (input_packet_tx, mut input_packet_rx) = mpsc::channel::<(Vec<u8>, bool, u64)>(1024);

    // Stats interval timer (must be created OUTSIDE the loop to persist across iterations)
    let mut stats_interval = tokio::time::interval(std::time::Duration::from_secs(1));

    // Spawn dedicated input processing task - completely decoupled from video/signaling
    // This ensures mouse/keyboard events are processed immediately without being blocked
    // by video decoding or network operations
    let input_packet_tx_clone = input_packet_tx.clone();
    let input_ready_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let input_ready_flag_clone = input_ready_flag.clone();
    let input_protocol_version_shared = Arc::new(std::sync::atomic::AtomicU8::new(0));
    let input_protocol_version_clone = input_protocol_version_shared.clone();

    tokio::spawn(async move {
        let mut input_encoder = InputEncoder::new();
        let mut input_event_rx = input_event_rx;

        loop {
            match input_event_rx.recv().await {
                Some(event) => {
                    // Only process if input is ready (handshake complete)
                    if !input_ready_flag_clone.load(std::sync::atomic::Ordering::Acquire) {
                        continue;
                    }

                    // Update encoder protocol version if changed
                    let version = input_protocol_version_clone.load(std::sync::atomic::Ordering::Relaxed);
                    input_encoder.set_protocol_version(version);

                    // Extract event timestamp for latency calculation
                    let event_timestamp_us = match &event {
                        InputEvent::KeyDown { timestamp_us, .. } |
                        InputEvent::KeyUp { timestamp_us, .. } |
                        InputEvent::MouseMove { timestamp_us, .. } |
                        InputEvent::MouseButtonDown { timestamp_us, .. } |
                        InputEvent::MouseButtonUp { timestamp_us, .. } |
                        InputEvent::MouseWheel { timestamp_us, .. } => *timestamp_us,
                        InputEvent::Heartbeat => 0,
                    };

                    // Calculate input latency (time from event creation to now)
                    let now_us = crate::input::get_timestamp_us();
                    let latency_us = now_us.saturating_sub(event_timestamp_us);

                    // Encode the event
                    let encoded = input_encoder.encode(&event);

                    // Determine if this is a mouse event (for channel selection)
                    let is_mouse = matches!(
                        &event,
                        InputEvent::MouseMove { .. } |
                        InputEvent::MouseButtonDown { .. } |
                        InputEvent::MouseButtonUp { .. } |
                        InputEvent::MouseWheel { .. }
                    );

                    // Send to main loop for WebRTC transmission
                    // Use try_send to never block the input thread
                    if input_packet_tx_clone.try_send((encoded, is_mouse, latency_us)).is_err() {
                        // Channel full - this is fine, old packets can be dropped for mouse
                    }
                }
                None => {
                    // Channel closed, exit task
                    break;
                }
            }
        }
        debug!("Input processing task ended");
    });

    // Main event loop - no longer processes input directly
    loop {
        tokio::select! {
            // Process encoded input packets from the input task (high priority)
            biased;

            Some((encoded, is_mouse, latency_us)) = input_packet_rx.recv() => {
                // Track input latency for stats
                if latency_us > 0 {
                    input_latency_sum += latency_us as f64;
                    input_latency_count += 1;
                }

                if is_mouse {
                    // Mouse events - use partially reliable channel (8ms lifetime)
                    let _ = peer.send_mouse_input(&encoded).await;
                } else {
                    // Keyboard events - use reliable channel
                    let _ = peer.send_input(&encoded).await;
                }
            }
            Some(event) = sig_event_rx.recv() => {
                match event {
                    SignalingEvent::SdpOffer(sdp) => {
                        info!("Received SDP offer, length: {}", sdp.len());

                        // Extract public IP and modify SDP
                        let public_ip = extract_public_ip(&server_ip);
                        let modified_sdp = if let Some(ref ip) = public_ip {
                            fix_server_ip(&sdp, ip)
                        } else {
                            sdp.clone()
                        };

                        // Prefer codec
                        let modified_sdp = prefer_codec(&modified_sdp, &codec);

                        // CRITICAL: Create input channel BEFORE handling offer (per GFN protocol)
                        info!("Creating input channel BEFORE SDP negotiation...");

                        // Handle offer and create answer
                        match peer.handle_offer(&modified_sdp, vec![]).await {
                            Ok(answer_sdp) => {
                                // Create input channel
                                if let Err(e) = peer.create_input_channel().await {
                                    warn!("Failed to create input channel: {}", e);
                                }

                                // Extract ICE credentials from our answer
                                let (ufrag, pwd, fingerprint) = extract_ice_credentials(&answer_sdp);

                                // Build nvstSdp
                                let nvst_sdp = build_nvst_sdp(
                                    &ufrag,
                                    &pwd,
                                    &fingerprint,
                                    width,
                                    height,
                                    fps,
                                    max_bitrate,
                                );

                                info!("Sending SDP answer with nvstSdp...");
                                signaling.send_answer(&answer_sdp, Some(&nvst_sdp)).await?;

                                // Add manual ICE candidate ONLY if we have real port from session API
                                // Otherwise, rely on trickle ICE from server (has real port)
                                // SDP port 47998 is a DUMMY - never use it!
                                if let Some(ref mci) = session_info.media_connection_info {
                                    info!("Using media port {} from session API", mci.port);
                                    let candidate = format!(
                                        "candidate:1 1 udp 2130706431 {} {} typ host",
                                        mci.ip, mci.port
                                    );
                                    info!("Adding manual ICE candidate: {}", candidate);
                                    if let Err(e) = peer.add_ice_candidate(&candidate, Some("0"), Some(0)).await {
                                        warn!("Failed to add manual ICE candidate: {}", e);
                                        for mid in ["1", "2", "3"] {
                                            if peer.add_ice_candidate(&candidate, Some(mid), Some(mid.parse().unwrap_or(0))).await.is_ok() {
                                                info!("Added ICE candidate with sdpMid={}", mid);
                                                break;
                                            }
                                        }
                                    }
                                } else {
                                    info!("No media_connection_info - waiting for trickle ICE from server");
                                }

                                // Update stats with codec info
                                stats.codec = codec_str.clone();
                                stats.resolution = format!("{}x{}", width, height);
                                stats.target_fps = fps;
                            }
                            Err(e) => {
                                error!("Failed to handle offer: {}", e);
                            }
                        }
                    }
                    SignalingEvent::IceCandidate(candidate) => {
                        info!("Received trickle ICE candidate");
                        if let Err(e) = peer.add_ice_candidate(
                            &candidate.candidate,
                            candidate.sdp_mid.as_deref(),
                            candidate.sdp_mline_index.map(|i| i as u16),
                        ).await {
                            warn!("Failed to add ICE candidate: {}", e);
                        }
                    }
                    SignalingEvent::Connected => {
                        info!("Signaling connected event");
                    }
                    SignalingEvent::Disconnected(reason) => {
                        info!("Signaling disconnected: {}", reason);
                        break;
                    }
                    SignalingEvent::Error(e) => {
                        error!("Signaling error: {}", e);
                        break;
                    }
                }
            }
            Some(event) = peer_event_rx.recv() => {
                match event {
                    WebRtcEvent::Connected => {
                        info!("=== WebRTC CONNECTED ===");
                        stats.gpu_type = session_info.gpu_type.clone().unwrap_or_default();
                    }
                    WebRtcEvent::Disconnected => {
                        warn!("WebRTC disconnected");
                        break;
                    }
                    WebRtcEvent::VideoFrame { payload, rtp_timestamp: _ } => {
                        frames_received += 1;
                        bytes_received += payload.len() as u64;
                        let packet_receive_time = std::time::Instant::now();

                        // Only log first packet
                        if frames_received == 1 {
                            info!("First video RTP packet received: {} bytes", payload.len());
                        }

                        // Depacketize RTP - may return multiple NAL units (e.g., from STAP-A/AP)
                        let nal_units = rtp_depacketizer.process(&payload);
                        for nal_unit in nal_units {
                            // NON-BLOCKING decode - fire and forget!
                            // The decoder thread will write directly to SharedFrame
                            // This ensures the main loop never stalls waiting for decode
                            if let Err(e) = video_decoder.decode_async(&nal_unit, packet_receive_time) {
                                warn!("Decode async failed: {}", e);
                            }
                        }
                    }
                    WebRtcEvent::AudioFrame(rtp_data) => {
                        // Decode Opus (stubbed for now)
                        if let Ok(samples) = audio_decoder.decode(&rtp_data) {
                            let _ = audio_tx.try_send(samples);
                        }
                    }
                    WebRtcEvent::DataChannelOpen(label) => {
                        info!("Data channel opened: {}", label);
                        if label.contains("input") {
                            info!("Input channel ready, waiting for handshake...");
                        }
                    }
                    WebRtcEvent::DataChannelMessage(label, data) => {
                        debug!("Data channel '{}' message: {} bytes", label, data.len());

                        // Handle input handshake
                        if data.len() >= 2 {
                            let first_word = u16::from_le_bytes([data[0], data.get(1).copied().unwrap_or(0)]);
                            let mut protocol_version: u16 = 0;

                            if first_word == 526 {
                                // New format: 0x020E (526 LE)
                                protocol_version = data.get(2..4)
                                    .map(|b| u16::from_le_bytes([b[0], b[1]]))
                                    .unwrap_or(0);
                                info!("Input handshake (new format), version={}", protocol_version);
                            } else if data[0] == 0x0e {
                                // Old format
                                protocol_version = first_word;
                                info!("Input handshake (old format), version={}", protocol_version);
                            }

                            // Echo handshake response
                            let is_ready = input_ready_flag.load(std::sync::atomic::Ordering::Acquire);
                            if !is_ready && (first_word == 526 || data[0] == 0x0e) {
                                if let Err(e) = peer.send_input(&data).await {
                                    error!("Failed to send handshake response: {}", e);
                                } else {
                                    info!("Sent handshake response, input is ready! Protocol version: {}", protocol_version);

                                    // Update shared protocol version for input task
                                    input_protocol_version_shared.store(protocol_version as u8, std::sync::atomic::Ordering::Release);

                                    // Signal input task that handshake is complete
                                    input_ready_flag.store(true, std::sync::atomic::Ordering::Release);

                                    info!("Input encoder protocol version set to {}", protocol_version);
                                }
                            }
                        }
                    }
                    WebRtcEvent::IceCandidate(candidate, sdp_mid, sdp_mline_index) => {
                        // Send our ICE candidate to server
                        if let Err(e) = signaling.send_ice_candidate(
                            &candidate,
                            sdp_mid.as_deref(),
                            sdp_mline_index.map(|i| i as u32),
                        ).await {
                            warn!("Failed to send ICE candidate: {}", e);
                        }
                    }
                    WebRtcEvent::Error(e) => {
                        error!("WebRTC error: {}", e);
                    }
                }
            }
            // Receive decode stats from the decoder thread (non-blocking)
            Some(decode_stat) = decode_stats_rx.recv() => {
                if decode_stat.frame_produced {
                    frames_decoded += 1;

                    // Track decode latency
                    stats.decode_time_ms = decode_stat.decode_time_ms;
                    pipeline_latency_sum += decode_stat.decode_time_ms as f64;
                    pipeline_latency_count += 1;
                    stats.latency_ms = (pipeline_latency_sum / pipeline_latency_count as f64) as f32;

                    // Log first decoded frame
                    if frames_decoded == 1 {
                        info!("First frame decoded (async) in {:.1}ms", decode_stat.decode_time_ms);
                    }
                }

                // Request keyframe if decoder is failing
                if decode_stat.needs_keyframe {
                    request_keyframe().await;
                }
            }
            // Update stats periodically (interval persists across loop iterations)
            _ = stats_interval.tick() => {
                let now = std::time::Instant::now();
                let elapsed = now.duration_since(last_stats_time).as_secs_f64();

                // Calculate actual FPS from decoded frames
                let frames_this_period = frames_decoded - last_frames_decoded;
                stats.fps = (frames_this_period as f64 / elapsed) as f32;
                last_frames_decoded = frames_decoded;

                // Calculate bitrate
                stats.bitrate_mbps = ((bytes_received as f64 * 8.0) / (elapsed * 1_000_000.0)) as f32;
                stats.frames_received = frames_received;
                stats.frames_decoded = frames_decoded;
                stats.frames_dropped = frames_dropped;

                // Calculate average input latency (microseconds to milliseconds)
                if input_latency_count > 0 {
                    stats.input_latency_ms = (input_latency_sum / input_latency_count as f64 / 1000.0) as f32;
                    // Reset for next period
                    input_latency_sum = 0.0;
                    input_latency_count = 0;
                }

                // Log if FPS is significantly below target (more than 20% drop)
                if stats.fps > 0.0 && stats.fps < (fps as f32 * 0.8) {
                    debug!("FPS below target: {:.1} / {} (dropped: {})", stats.fps, fps, frames_dropped);
                }

                // Reset counters
                bytes_received = 0;
                last_stats_time = now;

                // Send stats update
                let _ = stats_tx.try_send(stats.clone());
            }
        }
    }

    // Clean up raw input sender
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    crate::input::clear_raw_input_sender();

    info!("Streaming session ended");
    Ok(())
}
