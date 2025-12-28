//! GFN Native Streaming Client
//!
//! A full-featured native client for GeForce NOW streaming.
//! Handles signaling, WebRTC, video decoding, audio playback, and input.

mod input;
mod signaling;
mod webrtc_client;

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use openh264::formats::YUVSource;
use clap::Parser;
use log::{info, warn, error, debug};
use parking_lot::Mutex;
use tokio::sync::mpsc;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{DeviceEvent, ElementState, WindowEvent, MouseButton};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

use input::{InputEncoder, InputEvent};
use signaling::{GfnSignaling, SignalingEvent, IceCandidate};
use webrtc_client::{WebRtcClient, WebRtcEvent};
use webrtc::ice_transport::ice_server::RTCIceServer;

/// GFN Native Streaming Client
#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// GFN streaming server IP address
    #[arg(short, long)]
    server: String,

    /// Session ID from GFN
    #[arg(short = 'i', long)]
    session_id: String,

    /// Window width
    #[arg(long, default_value = "1920")]
    width: u32,

    /// Window height
    #[arg(long, default_value = "1080")]
    height: u32,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,
}

/// Video frame for rendering
struct VideoFrame {
    width: u32,
    height: u32,
    data: Vec<u32>, // ARGB pixels
}

/// Shared state between async tasks and window
struct SharedState {
    video_frame: Option<VideoFrame>,
    connected: bool,
    signaling_connected: bool,
    webrtc_connected: bool,
    input_ready: bool,
    stats: StreamingStats,
    status_message: String,
}

impl Default for SharedState {
    fn default() -> Self {
        Self {
            video_frame: None,
            connected: false,
            signaling_connected: false,
            webrtc_connected: false,
            input_ready: false,
            stats: StreamingStats::default(),
            status_message: "Initializing...".to_string(),
        }
    }
}

#[derive(Default)]
struct StreamingStats {
    frames_received: u64,
    frames_decoded: u64,
    bytes_received: u64,
    audio_packets: u64,
}

/// Main application state
struct GfnApp {
    window: Option<Arc<Window>>,
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    shared_state: Arc<Mutex<SharedState>>,
    input_tx: mpsc::Sender<InputEvent>,
    mouse_captured: bool,
    start_time: Instant,
    args: Args,
}

impl GfnApp {
    fn new(args: Args, input_tx: mpsc::Sender<InputEvent>, shared_state: Arc<Mutex<SharedState>>) -> Self {
        Self {
            window: None,
            surface: None,
            shared_state,
            input_tx,
            mouse_captured: false,
            start_time: Instant::now(),
            args,
        }
    }

    fn get_timestamp_us(&self) -> u64 {
        self.start_time.elapsed().as_micros() as u64
    }

    fn capture_mouse(&mut self) {
        if let Some(window) = &self.window {
            // Try confined first, then locked
            if window.set_cursor_grab(CursorGrabMode::Confined).is_err() {
                let _ = window.set_cursor_grab(CursorGrabMode::Locked);
            }
            window.set_cursor_visible(false);
            self.mouse_captured = true;
            info!("Mouse captured");
        }
    }

    fn release_mouse(&mut self) {
        if let Some(window) = &self.window {
            let _ = window.set_cursor_grab(CursorGrabMode::None);
            window.set_cursor_visible(true);
            self.mouse_captured = false;
            info!("Mouse released");
        }
    }

    fn render_frame(&mut self) {
        let Some(surface) = &mut self.surface else { return };
        let Some(window) = &self.window else { return };

        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }

        // Resize surface if needed
        let _ = surface.resize(
            NonZeroU32::new(size.width).unwrap(),
            NonZeroU32::new(size.height).unwrap(),
        );

        let mut buffer = match surface.buffer_mut() {
            Ok(b) => b,
            Err(e) => {
                warn!("Failed to get buffer: {}", e);
                return;
            }
        };

        // Get video frame if available
        let state = self.shared_state.lock();

        if let Some(frame) = &state.video_frame {
            // Scale and blit video frame to window
            let scale_x = frame.width as f32 / size.width as f32;
            let scale_y = frame.height as f32 / size.height as f32;

            for y in 0..size.height {
                for x in 0..size.width {
                    let src_x = ((x as f32 * scale_x) as u32).min(frame.width - 1);
                    let src_y = ((y as f32 * scale_y) as u32).min(frame.height - 1);
                    let src_idx = (src_y * frame.width + src_x) as usize;
                    let dst_idx = (y * size.width + x) as usize;

                    if src_idx < frame.data.len() && dst_idx < buffer.len() {
                        buffer[dst_idx] = frame.data[src_idx];
                    }
                }
            }
        } else {
            // Show status screen
            let status = state.status_message.clone();
            let signaling = state.signaling_connected;
            let webrtc = state.webrtc_connected;
            let input_ready = state.input_ready;
            let stats = &state.stats;
            let frames = stats.frames_received;
            drop(state);

            // Draw status screen
            for (i, pixel) in buffer.iter_mut().enumerate() {
                let x = i as u32 % size.width;
                let y = i as u32 / size.width;

                // Background gradient
                let brightness = 0x1a + (y as u32 * 0x10 / size.height).min(0x10) as u8;
                *pixel = 0xFF000000 | ((brightness as u32) << 16) | ((brightness as u32) << 8) | (brightness as u32 + 0x10);

                // Status indicator box in center
                let cx = size.width / 2;
                let cy = size.height / 2;
                let box_w = 400;
                let box_h = 200;

                if x >= cx - box_w/2 && x <= cx + box_w/2 && y >= cy - box_h/2 && y <= cy + box_h/2 {
                    // Status box background
                    *pixel = 0xFF2a2a3a;

                    // Border
                    if x == cx - box_w/2 || x == cx + box_w/2 || y == cy - box_h/2 || y == cy + box_h/2 {
                        *pixel = if webrtc { 0xFF00AA00 } else if signaling { 0xFFAAAA00 } else { 0xFF666666 };
                    }
                }

                // Connection status dots
                let dot_y = cy - 50;
                let dot_radius = 8u32;

                // Signaling dot
                let dot1_x = cx - 60;
                let dist1 = ((x as i32 - dot1_x as i32).pow(2) + (y as i32 - dot_y as i32).pow(2)) as u32;
                if dist1 <= dot_radius * dot_radius {
                    *pixel = if signaling { 0xFF00FF00 } else { 0xFF444444 };
                }

                // WebRTC dot
                let dot2_x = cx;
                let dist2 = ((x as i32 - dot2_x as i32).pow(2) + (y as i32 - dot_y as i32).pow(2)) as u32;
                if dist2 <= dot_radius * dot_radius {
                    *pixel = if webrtc { 0xFF00FF00 } else { 0xFF444444 };
                }

                // Input dot
                let dot3_x = cx + 60;
                let dist3 = ((x as i32 - dot3_x as i32).pow(2) + (y as i32 - dot_y as i32).pow(2)) as u32;
                if dist3 <= dot_radius * dot_radius {
                    *pixel = if input_ready { 0xFF00FF00 } else { 0xFF444444 };
                }
            }
        }

        let _ = buffer.present();
    }

    fn handle_keyboard(&mut self, key: PhysicalKey, state: ElementState) {
        let PhysicalKey::Code(keycode) = key else { return };

        // ESC releases mouse or closes window
        if keycode == KeyCode::Escape && state == ElementState::Pressed {
            if self.mouse_captured {
                self.release_mouse();
            }
            return;
        }

        if !self.mouse_captured {
            return;
        }

        // Convert to Windows VK code and scancode
        let vk = keycode_to_vk(keycode);
        let scan = keycode_to_scan(keycode);

        let event = match state {
            ElementState::Pressed => InputEvent::KeyDown {
                keycode: vk,
                scancode: scan,
                modifiers: 0,
                timestamp_us: self.get_timestamp_us(),
            },
            ElementState::Released => InputEvent::KeyUp {
                keycode: vk,
                scancode: scan,
                modifiers: 0,
                timestamp_us: self.get_timestamp_us(),
            },
        };

        let _ = self.input_tx.try_send(event);
    }

    fn handle_mouse_button(&mut self, button: MouseButton, state: ElementState) {
        // Capture on click if not captured
        if !self.mouse_captured && state == ElementState::Pressed {
            self.capture_mouse();
            return;
        }

        if !self.mouse_captured {
            return;
        }

        let btn = match button {
            MouseButton::Left => 0,
            MouseButton::Right => 1,
            MouseButton::Middle => 2,
            MouseButton::Back => 3,
            MouseButton::Forward => 4,
            MouseButton::Other(n) => n as u8,
        };

        let event = match state {
            ElementState::Pressed => InputEvent::MouseButtonDown {
                button: btn,
                timestamp_us: self.get_timestamp_us(),
            },
            ElementState::Released => InputEvent::MouseButtonUp {
                button: btn,
                timestamp_us: self.get_timestamp_us(),
            },
        };

        let _ = self.input_tx.try_send(event);
    }

    fn handle_mouse_wheel(&mut self, delta: f32) {
        if !self.mouse_captured {
            return;
        }

        let event = InputEvent::MouseWheel {
            delta: (delta * 120.0) as i16,
            timestamp_us: self.get_timestamp_us(),
        };

        let _ = self.input_tx.try_send(event);
    }
}

impl ApplicationHandler for GfnApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window_attrs = Window::default_attributes()
            .with_title(format!("GFN Native Client - {}", self.args.server))
            .with_inner_size(LogicalSize::new(self.args.width, self.args.height));

        let window = Arc::new(event_loop.create_window(window_attrs).unwrap());

        // Create software rendering surface
        let context = softbuffer::Context::new(window.clone()).unwrap();
        let surface = softbuffer::Surface::new(&context, window.clone()).unwrap();

        info!("Window created: {}x{}", self.args.width, self.args.height);
        info!("Server: {}", self.args.server);
        info!("Session: {}", self.args.session_id);
        info!("Press ESC to release mouse, click to capture");

        self.surface = Some(surface);
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                info!("Window closed");
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                self.render_frame();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                self.handle_keyboard(event.physical_key, event.state);
            }
            WindowEvent::MouseInput { button, state, .. } => {
                self.handle_mouse_button(button, state);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let y = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => y,
                    winit::event::MouseScrollDelta::PixelDelta(pos) => pos.y as f32 / 120.0,
                };
                self.handle_mouse_wheel(y);
            }
            WindowEvent::Resized(size) => {
                if let Some(surface) = &mut self.surface {
                    if size.width > 0 && size.height > 0 {
                        let _ = surface.resize(
                            NonZeroU32::new(size.width).unwrap(),
                            NonZeroU32::new(size.height).unwrap(),
                        );
                    }
                }
            }
            WindowEvent::Focused(focused) => {
                if focused {
                    // Window regained focus - re-capture mouse if it was captured before
                    if self.mouse_captured {
                        info!("Window regained focus - re-capturing mouse");
                        // Need to re-apply the cursor grab since Windows releases it on focus loss
                        if let Some(window) = &self.window {
                            // Try confined first, then locked
                            if window.set_cursor_grab(CursorGrabMode::Confined).is_err() {
                                let _ = window.set_cursor_grab(CursorGrabMode::Locked);
                            }
                            window.set_cursor_visible(false);
                        }
                    }
                } else {
                    // Window lost focus - cursor grab is automatically released by Windows
                    if self.mouse_captured {
                        info!("Window lost focus - cursor grab suspended");
                    }
                }
            }
            _ => {}
        }
    }

    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _device_id: winit::event::DeviceId, event: DeviceEvent) {
        if !self.mouse_captured {
            return;
        }

        if let DeviceEvent::MouseMotion { delta } = event {
            let (dx, dy) = delta;
            let event = InputEvent::MouseMove {
                dx: dx as i16,
                dy: dy as i16,
                timestamp_us: self.get_timestamp_us(),
            };
            let _ = self.input_tx.try_send(event);
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

/// Run the streaming connection
async fn run_streaming(
    server: String,
    session_id: String,
    shared_state: Arc<Mutex<SharedState>>,
    mut input_rx: mpsc::Receiver<InputEvent>,
) -> Result<()> {
    info!("Starting streaming connection to {}", server);

    {
        let mut state = shared_state.lock();
        state.status_message = "Connecting to signaling...".to_string();
    }

    // Create channels for signaling events
    let (sig_tx, mut sig_rx) = mpsc::channel::<SignalingEvent>(64);

    // Create signaling client
    let mut signaling = GfnSignaling::new(server.clone(), session_id.clone(), sig_tx);

    // Connect to signaling server
    match signaling.connect().await {
        Ok(_) => {
            info!("Connected to signaling server");
            let mut state = shared_state.lock();
            state.signaling_connected = true;
            state.status_message = "Signaling connected, waiting for offer...".to_string();
        }
        Err(e) => {
            error!("Failed to connect to signaling: {}", e);
            let mut state = shared_state.lock();
            state.status_message = format!("Signaling failed: {}", e);
            return Err(e);
        }
    }

    // Create WebRTC event channel
    let (webrtc_tx, mut webrtc_rx) = mpsc::channel::<WebRtcEvent>(64);
    let mut webrtc_client = WebRtcClient::new(webrtc_tx);

    // Input encoder
    let mut input_encoder = InputEncoder::new();

    // H.264 decoder
    let mut decoder = match openh264::decoder::Decoder::new() {
        Ok(d) => {
            info!("H.264 decoder initialized");
            Some(d)
        }
        Err(e) => {
            warn!("Failed to create H.264 decoder: {}. Video will not be displayed.", e);
            None
        }
    };

    // RTP packet assembler for H.264
    let mut h264_buffer: Vec<u8> = Vec::with_capacity(1024 * 1024); // 1MB buffer

    // Main event loop
    loop {
        tokio::select! {
            // Handle signaling events
            Some(event) = sig_rx.recv() => {
                match event {
                    SignalingEvent::Connected => {
                        info!("Signaling connected");
                    }
                    SignalingEvent::SdpOffer(sdp) => {
                        info!("Received SDP offer ({} bytes)", sdp.len());

                        {
                            let mut state = shared_state.lock();
                            state.status_message = "Received offer, creating answer...".to_string();
                        }

                        // Parse ICE servers - use STUN server
                        let ice_servers = vec![
                            RTCIceServer {
                                urls: vec!["stun:stun.l.google.com:19302".to_string()],
                                ..Default::default()
                            },
                        ];

                        match webrtc_client.handle_offer(&sdp, ice_servers).await {
                            Ok(answer) => {
                                info!("Created SDP answer, sending to server");

                                {
                                    let mut state = shared_state.lock();
                                    state.status_message = "Sending answer...".to_string();
                                }

                                if let Err(e) = signaling.send_answer(&answer, None).await {
                                    error!("Failed to send SDP answer: {}", e);
                                }

                                // Create input channel
                                if let Err(e) = webrtc_client.create_input_channel().await {
                                    warn!("Failed to create input channel: {}", e);
                                }
                            }
                            Err(e) => {
                                error!("Failed to handle SDP offer: {}", e);
                                let mut state = shared_state.lock();
                                state.status_message = format!("WebRTC error: {}", e);
                            }
                        }
                    }
                    SignalingEvent::IceCandidate(candidate) => {
                        debug!("Received ICE candidate: {}", &candidate.candidate[..candidate.candidate.len().min(50)]);
                        if let Err(e) = webrtc_client.add_ice_candidate(
                            &candidate.candidate,
                            candidate.sdp_mid.as_deref(),
                            candidate.sdp_mline_index.map(|i| i as u16),
                        ).await {
                            warn!("Failed to add ICE candidate: {}", e);
                        }
                    }
                    SignalingEvent::Disconnected(reason) => {
                        warn!("Signaling disconnected: {}", reason);
                        let mut state = shared_state.lock();
                        state.signaling_connected = false;
                        state.status_message = format!("Disconnected: {}", reason);
                        break;
                    }
                    SignalingEvent::Error(e) => {
                        error!("Signaling error: {}", e);
                    }
                }
            }

            // Handle WebRTC events
            Some(event) = webrtc_rx.recv() => {
                match event {
                    WebRtcEvent::Connected => {
                        info!("WebRTC connected!");
                        let mut state = shared_state.lock();
                        state.webrtc_connected = true;
                        state.connected = true;
                        state.status_message = "WebRTC connected, waiting for video...".to_string();
                    }
                    WebRtcEvent::Disconnected => {
                        warn!("WebRTC disconnected");
                        let mut state = shared_state.lock();
                        state.webrtc_connected = false;
                        state.connected = false;
                        state.status_message = "WebRTC disconnected".to_string();
                    }
                    WebRtcEvent::VideoFrame(rtp_payload) => {
                        // Update stats
                        {
                            let mut state = shared_state.lock();
                            state.stats.frames_received += 1;
                            state.stats.bytes_received += rtp_payload.len() as u64;
                        }

                        // Accumulate RTP payload (simplified - real impl needs NAL unit handling)
                        if !rtp_payload.is_empty() {
                            // Check for NAL unit start code or marker
                            let nal_type = rtp_payload[0] & 0x1F;

                            // Simple approach: try to decode each packet
                            // Real implementation would reassemble fragmented NALUs
                            if let Some(ref mut dec) = decoder {
                                // Add start code if not present
                                let data = if rtp_payload.len() >= 3 &&
                                    rtp_payload[0] == 0 && rtp_payload[1] == 0 &&
                                    (rtp_payload[2] == 1 || (rtp_payload[2] == 0 && rtp_payload.len() >= 4 && rtp_payload[3] == 1)) {
                                    rtp_payload.clone()
                                } else {
                                    let mut with_start = vec![0, 0, 0, 1];
                                    with_start.extend_from_slice(&rtp_payload);
                                    with_start
                                };

                                match dec.decode(&data) {
                                    Ok(Some(yuv)) => {
                                        let (width, height) = yuv.dimensions();
                                        let width = width as usize;
                                        let height = height as usize;

                                        // Get YUV data using trait methods
                                        let y_data = yuv.y();
                                        let u_data = yuv.u();
                                        let v_data = yuv.v();
                                        let (y_stride, u_stride, v_stride) = yuv.strides();

                                        let mut argb_data = Vec::with_capacity(width * height);

                                        for row in 0..height {
                                            for col in 0..width {
                                                let y_idx = row * y_stride + col;
                                                let uv_row = row / 2;
                                                let uv_col = col / 2;
                                                let u_idx = uv_row * u_stride + uv_col;
                                                let v_idx = uv_row * v_stride + uv_col;

                                                let y = y_data.get(y_idx).copied().unwrap_or(0) as f32;
                                                let u = u_data.get(u_idx).copied().unwrap_or(128) as f32 - 128.0;
                                                let v = v_data.get(v_idx).copied().unwrap_or(128) as f32 - 128.0;

                                                // YUV to RGB conversion
                                                let r = (y + 1.402 * v).clamp(0.0, 255.0) as u32;
                                                let g = (y - 0.344 * u - 0.714 * v).clamp(0.0, 255.0) as u32;
                                                let b = (y + 1.772 * u).clamp(0.0, 255.0) as u32;

                                                argb_data.push(0xFF000000 | (r << 16) | (g << 8) | b);
                                            }
                                        }

                                        let mut state = shared_state.lock();
                                        state.video_frame = Some(VideoFrame {
                                            width: width as u32,
                                            height: height as u32,
                                            data: argb_data,
                                        });
                                        state.stats.frames_decoded += 1;
                                        state.status_message = format!("Streaming - {}x{}", width, height);
                                    }
                                    Ok(None) => {
                                        // Decoder needs more data
                                    }
                                    Err(_) => {
                                        // Decode error - skip
                                    }
                                }
                            }
                        }
                    }
                    WebRtcEvent::AudioFrame(_data) => {
                        let mut state = shared_state.lock();
                        state.stats.audio_packets += 1;
                        // TODO: Play audio with cpal
                    }
                    WebRtcEvent::DataChannelOpen(name) => {
                        info!("Data channel opened: {}", name);
                        if name.contains("input") {
                            let mut state = shared_state.lock();
                            state.input_ready = true;
                            state.status_message = "Input channel ready".to_string();
                        }
                    }
                    WebRtcEvent::DataChannelMessage(name, data) => {
                        debug!("Data channel '{}' message: {} bytes", name, data.len());

                        // Handle input handshake
                        if data.len() == 4 && data[0] == 0x0e {
                            info!("Received input handshake: v{}.{} flags={}", data[1], data[2], data[3]);
                            if let Err(e) = webrtc_client.send_handshake_response(data[1], data[2], data[3]).await {
                                warn!("Failed to send handshake response: {}", e);
                            } else {
                                info!("Handshake complete - input ready!");
                            }
                        }
                    }
                    WebRtcEvent::IceCandidate(candidate, sdp_mid, sdp_mline_index) => {
                        debug!("Local ICE candidate generated");
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

            // Handle input events
            Some(event) = input_rx.recv() => {
                let ready = {
                    let state = shared_state.lock();
                    state.input_ready && webrtc_client.is_handshake_complete()
                };

                if ready {
                    let encoded = input_encoder.encode(&event);
                    if let Err(e) = webrtc_client.send_input(&encoded).await {
                        debug!("Failed to send input: {}", e);
                    }
                }
            }

            else => break,
        }
    }

    info!("Streaming ended");
    Ok(())
}

/// Convert winit keycode to Windows virtual key code
fn keycode_to_vk(keycode: KeyCode) -> u16 {
    match keycode {
        KeyCode::Backquote => 0xC0,
        KeyCode::Backslash => 0xDC,
        KeyCode::BracketLeft => 0xDB,
        KeyCode::BracketRight => 0xDD,
        KeyCode::Comma => 0xBC,
        KeyCode::Digit0 => 0x30,
        KeyCode::Digit1 => 0x31,
        KeyCode::Digit2 => 0x32,
        KeyCode::Digit3 => 0x33,
        KeyCode::Digit4 => 0x34,
        KeyCode::Digit5 => 0x35,
        KeyCode::Digit6 => 0x36,
        KeyCode::Digit7 => 0x37,
        KeyCode::Digit8 => 0x38,
        KeyCode::Digit9 => 0x39,
        KeyCode::Equal => 0xBB,
        KeyCode::KeyA => 0x41,
        KeyCode::KeyB => 0x42,
        KeyCode::KeyC => 0x43,
        KeyCode::KeyD => 0x44,
        KeyCode::KeyE => 0x45,
        KeyCode::KeyF => 0x46,
        KeyCode::KeyG => 0x47,
        KeyCode::KeyH => 0x48,
        KeyCode::KeyI => 0x49,
        KeyCode::KeyJ => 0x4A,
        KeyCode::KeyK => 0x4B,
        KeyCode::KeyL => 0x4C,
        KeyCode::KeyM => 0x4D,
        KeyCode::KeyN => 0x4E,
        KeyCode::KeyO => 0x4F,
        KeyCode::KeyP => 0x50,
        KeyCode::KeyQ => 0x51,
        KeyCode::KeyR => 0x52,
        KeyCode::KeyS => 0x53,
        KeyCode::KeyT => 0x54,
        KeyCode::KeyU => 0x55,
        KeyCode::KeyV => 0x56,
        KeyCode::KeyW => 0x57,
        KeyCode::KeyX => 0x58,
        KeyCode::KeyY => 0x59,
        KeyCode::KeyZ => 0x5A,
        KeyCode::Minus => 0xBD,
        KeyCode::Period => 0xBE,
        KeyCode::Quote => 0xDE,
        KeyCode::Semicolon => 0xBA,
        KeyCode::Slash => 0xBF,
        KeyCode::Backspace => 0x08,
        KeyCode::CapsLock => 0x14,
        KeyCode::Enter => 0x0D,
        KeyCode::Space => 0x20,
        KeyCode::Tab => 0x09,
        KeyCode::Delete => 0x2E,
        KeyCode::End => 0x23,
        KeyCode::Home => 0x24,
        KeyCode::Insert => 0x2D,
        KeyCode::PageDown => 0x22,
        KeyCode::PageUp => 0x21,
        KeyCode::ArrowDown => 0x28,
        KeyCode::ArrowLeft => 0x25,
        KeyCode::ArrowRight => 0x27,
        KeyCode::ArrowUp => 0x26,
        KeyCode::Escape => 0x1B,
        KeyCode::F1 => 0x70,
        KeyCode::F2 => 0x71,
        KeyCode::F3 => 0x72,
        KeyCode::F4 => 0x73,
        KeyCode::F5 => 0x74,
        KeyCode::F6 => 0x75,
        KeyCode::F7 => 0x76,
        KeyCode::F8 => 0x77,
        KeyCode::F9 => 0x78,
        KeyCode::F10 => 0x79,
        KeyCode::F11 => 0x7A,
        KeyCode::F12 => 0x7B,
        KeyCode::Numpad0 => 0x60,
        KeyCode::Numpad1 => 0x61,
        KeyCode::Numpad2 => 0x62,
        KeyCode::Numpad3 => 0x63,
        KeyCode::Numpad4 => 0x64,
        KeyCode::Numpad5 => 0x65,
        KeyCode::Numpad6 => 0x66,
        KeyCode::Numpad7 => 0x67,
        KeyCode::Numpad8 => 0x68,
        KeyCode::Numpad9 => 0x69,
        KeyCode::NumpadAdd => 0x6B,
        KeyCode::NumpadDecimal => 0x6E,
        KeyCode::NumpadDivide => 0x6F,
        KeyCode::NumpadEnter => 0x0D,
        KeyCode::NumpadMultiply => 0x6A,
        KeyCode::NumpadSubtract => 0x6D,
        KeyCode::ShiftLeft | KeyCode::ShiftRight => 0x10,
        KeyCode::ControlLeft | KeyCode::ControlRight => 0x11,
        KeyCode::AltLeft | KeyCode::AltRight => 0x12,
        KeyCode::SuperLeft => 0x5B,
        KeyCode::SuperRight => 0x5C,
        _ => 0,
    }
}

/// Convert winit keycode to scan code
fn keycode_to_scan(keycode: KeyCode) -> u16 {
    match keycode {
        KeyCode::Escape => 0x01,
        KeyCode::Digit1 => 0x02,
        KeyCode::Digit2 => 0x03,
        KeyCode::Digit3 => 0x04,
        KeyCode::Digit4 => 0x05,
        KeyCode::Digit5 => 0x06,
        KeyCode::Digit6 => 0x07,
        KeyCode::Digit7 => 0x08,
        KeyCode::Digit8 => 0x09,
        KeyCode::Digit9 => 0x0A,
        KeyCode::Digit0 => 0x0B,
        KeyCode::Minus => 0x0C,
        KeyCode::Equal => 0x0D,
        KeyCode::Backspace => 0x0E,
        KeyCode::Tab => 0x0F,
        KeyCode::KeyQ => 0x10,
        KeyCode::KeyW => 0x11,
        KeyCode::KeyE => 0x12,
        KeyCode::KeyR => 0x13,
        KeyCode::KeyT => 0x14,
        KeyCode::KeyY => 0x15,
        KeyCode::KeyU => 0x16,
        KeyCode::KeyI => 0x17,
        KeyCode::KeyO => 0x18,
        KeyCode::KeyP => 0x19,
        KeyCode::BracketLeft => 0x1A,
        KeyCode::BracketRight => 0x1B,
        KeyCode::Enter => 0x1C,
        KeyCode::ControlLeft => 0x1D,
        KeyCode::KeyA => 0x1E,
        KeyCode::KeyS => 0x1F,
        KeyCode::KeyD => 0x20,
        KeyCode::KeyF => 0x21,
        KeyCode::KeyG => 0x22,
        KeyCode::KeyH => 0x23,
        KeyCode::KeyJ => 0x24,
        KeyCode::KeyK => 0x25,
        KeyCode::KeyL => 0x26,
        KeyCode::Semicolon => 0x27,
        KeyCode::Quote => 0x28,
        KeyCode::Backquote => 0x29,
        KeyCode::ShiftLeft => 0x2A,
        KeyCode::Backslash => 0x2B,
        KeyCode::KeyZ => 0x2C,
        KeyCode::KeyX => 0x2D,
        KeyCode::KeyC => 0x2E,
        KeyCode::KeyV => 0x2F,
        KeyCode::KeyB => 0x30,
        KeyCode::KeyN => 0x31,
        KeyCode::KeyM => 0x32,
        KeyCode::Comma => 0x33,
        KeyCode::Period => 0x34,
        KeyCode::Slash => 0x35,
        KeyCode::ShiftRight => 0x36,
        KeyCode::Space => 0x39,
        KeyCode::CapsLock => 0x3A,
        KeyCode::F1 => 0x3B,
        KeyCode::F2 => 0x3C,
        KeyCode::F3 => 0x3D,
        KeyCode::F4 => 0x3E,
        KeyCode::F5 => 0x3F,
        KeyCode::F6 => 0x40,
        KeyCode::F7 => 0x41,
        KeyCode::F8 => 0x42,
        KeyCode::F9 => 0x43,
        KeyCode::F10 => 0x44,
        KeyCode::F11 => 0x57,
        KeyCode::F12 => 0x58,
        KeyCode::ArrowUp => 0x48,
        KeyCode::ArrowLeft => 0x4B,
        KeyCode::ArrowRight => 0x4D,
        KeyCode::ArrowDown => 0x50,
        _ => 0,
    }
}

fn main() -> Result<()> {
    // Parse arguments
    let args = Args::parse();

    // Initialize logging
    if args.debug {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
    } else {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    }

    info!("GFN Native Client v{}", env!("CARGO_PKG_VERSION"));
    info!("Server: {}", args.server);
    info!("Session: {}", args.session_id);

    // Create input channel
    let (input_tx, input_rx) = mpsc::channel::<InputEvent>(256);

    // Create shared state
    let shared_state = Arc::new(Mutex::new(SharedState::default()));

    // Start tokio runtime for async tasks
    let runtime = tokio::runtime::Runtime::new()?;

    // Spawn streaming task
    let server = args.server.clone();
    let session_id = args.session_id.clone();
    let state_clone = shared_state.clone();

    runtime.spawn(async move {
        if let Err(e) = run_streaming(server, session_id, state_clone, input_rx).await {
            error!("Streaming error: {}", e);
        }
    });

    // Create event loop
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    // Create application
    let mut app = GfnApp::new(args, input_tx, shared_state);

    // Run event loop (blocking)
    event_loop.run_app(&mut app)?;

    // Cleanup
    info!("Shutting down...");
    drop(runtime);

    Ok(())
}
