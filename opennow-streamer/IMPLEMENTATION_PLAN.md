# OpenNow Streamer - Native Client Implementation Plan

## Executive Summary

A high-performance, cross-platform native streaming client for GeForce NOW that:
- Works on **Windows, macOS, and Linux**
- Supports **all video codecs**: H.264, H.265/HEVC, AV1
- Uses **native mouse capture** for minimal latency
- Runs efficiently on **low-end hardware**
- Features the same **stats panel** (bottom-left) as the web client

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                     OpenNow Streamer                            │
├─────────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │   GUI Layer  │  │  Stats Panel │  │  Settings/Config     │  │
│  │  (winit/wgpu)│  │  (bottom-left)│  │  (JSON persistent)   │  │
│  └──────┬───────┘  └──────┬───────┘  └──────────┬───────────┘  │
│         │                 │                     │               │
│  ┌──────▼─────────────────▼─────────────────────▼───────────┐  │
│  │                    Application Core                       │  │
│  │  - Session Management (CloudMatch API)                    │  │
│  │  - Authentication (OAuth/JWT)                             │  │
│  │  - WebRTC State Machine                                   │  │
│  └──────┬───────────────────────────────────────┬───────────┘  │
│         │                                       │               │
│  ┌──────▼───────┐  ┌───────────────┐  ┌────────▼────────────┐  │
│  │  WebRTC      │  │  Video Decode │  │  Input Handler      │  │
│  │  (webrtc-rs) │  │  (FFmpeg)     │  │  (Platform-native)  │  │
│  │  - Signaling │  │  - H.264      │  │  - Windows: RawInput│  │
│  │  - ICE/DTLS  │  │  - H.265      │  │  - macOS: CGEvent   │  │
│  │  - DataChan  │  │  - AV1        │  │  - Linux: evdev     │  │
│  └──────┬───────┘  └───────┬───────┘  └─────────┬───────────┘  │
│         │                  │                    │               │
│  ┌──────▼──────────────────▼────────────────────▼───────────┐  │
│  │                    Media Pipeline                         │  │
│  │  RTP → Depacketize → Decode → YUV→RGB → GPU Texture      │  │
│  │  Audio: Opus → PCM → CPAL (cross-platform audio)         │  │
│  └──────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

---

## Component Breakdown

### 1. Project Structure

```
opennow-streamer/
├── Cargo.toml                 # Workspace dependencies
├── src/
│   ├── main.rs               # Entry point, CLI args
│   ├── lib.rs                # Library exports
│   │
│   ├── app/
│   │   ├── mod.rs            # Application state machine
│   │   ├── config.rs         # Settings (JSON, persistent)
│   │   └── session.rs        # GFN session lifecycle
│   │
│   ├── auth/
│   │   ├── mod.rs            # OAuth flow, token management
│   │   └── jwt.rs            # JWT/GFN token handling
│   │
│   ├── api/
│   │   ├── mod.rs            # HTTP client wrapper
│   │   ├── cloudmatch.rs     # Session API (CloudMatch)
│   │   └── games.rs          # Game library fetching
│   │
│   ├── webrtc/
│   │   ├── mod.rs            # WebRTC state machine
│   │   ├── signaling.rs      # WebSocket signaling (GFN protocol)
│   │   ├── peer.rs           # RTCPeerConnection wrapper
│   │   ├── sdp.rs            # SDP parsing/manipulation
│   │   └── datachannel.rs    # Input/control channels
│   │
│   ├── media/
│   │   ├── mod.rs            # Media pipeline orchestration
│   │   ├── rtp.rs            # RTP depacketization
│   │   ├── video_decoder.rs  # FFmpeg video decode (H.264/H.265/AV1)
│   │   ├── audio_decoder.rs  # Opus decode
│   │   └── renderer.rs       # GPU texture upload, frame queue
│   │
│   ├── input/
│   │   ├── mod.rs            # Cross-platform input abstraction
│   │   ├── protocol.rs       # GFN binary input protocol encoder
│   │   ├── windows.rs        # Windows RawInput + cursor clip
│   │   ├── macos.rs          # macOS CGEvent + CGWarpMouseCursorPosition
│   │   └── linux.rs          # Linux evdev/libinput
│   │
│   ├── gui/
│   │   ├── mod.rs            # GUI framework setup
│   │   ├── window.rs         # winit window management
│   │   ├── renderer.rs       # wgpu rendering pipeline
│   │   ├── stats_panel.rs    # Stats overlay (bottom-left)
│   │   └── fullscreen.rs     # Fullscreen management
│   │
│   └── utils/
│       ├── mod.rs
│       ├── logging.rs        # File + console logging
│       └── time.rs           # High-precision timestamps
│
├── assets/
│   └── shaders/
│       ├── video.wgsl        # YUV→RGB shader
│       └── ui.wgsl           # Stats panel shader
│
└── build.rs                  # FFmpeg linking, platform setup
```

### 2. Core Dependencies (Cargo.toml)

```toml
[package]
name = "opennow-streamer"
version = "0.1.0"
edition = "2021"

[dependencies]
# Async runtime
tokio = { version = "1", features = ["full"] }

# WebRTC
webrtc = "0.12"

# HTTP client
reqwest = { version = "0.12", features = ["json", "rustls-tls"] }

# WebSocket (signaling)
tokio-tungstenite = { version = "0.24", features = ["rustls-tls-native-roots"] }

# Video decoding (FFmpeg bindings)
ffmpeg-next = "7"

# Audio decoding
opus = "0.3"
audiopus = "0.3"

# Audio playback (cross-platform)
cpal = "0.15"

# Window & Graphics
winit = "0.30"
wgpu = "23"
bytemuck = { version = "1", features = ["derive"] }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Utilities
anyhow = "1"
log = "0.4"
env_logger = "0.11"
parking_lot = "0.12"
bytes = "1"
base64 = "0.22"
sha2 = "0.10"
uuid = { version = "1", features = ["v4"] }
chrono = "0.4"

# Platform-specific
[target.'cfg(windows)'.dependencies]
windows = { version = "0.58", features = [
    "Win32_UI_WindowsAndMessaging",
    "Win32_UI_Input",
    "Win32_UI_Input_KeyboardAndMouse",
    "Win32_Graphics_Gdi",
    "Win32_Foundation",
] }

[target.'cfg(target_os = "macos")'.dependencies]
core-foundation = "0.10"
core-graphics = "0.24"

[target.'cfg(target_os = "linux")'.dependencies]
evdev = "0.12"
x11 = { version = "2.21", features = ["xlib"] }
```

---

## Implementation Phases

### Phase 1: Core Infrastructure (Week 1)

#### 1.1 Project Setup
- [ ] Create Cargo workspace
- [ ] Set up build.rs for FFmpeg linking
- [ ] Configure cross-compilation for Windows/macOS/Linux
- [ ] Set up logging infrastructure

#### 1.2 Configuration System
```rust
// src/app/config.rs
#[derive(Serialize, Deserialize, Default)]
pub struct Settings {
    // Video settings
    pub resolution: Resolution,
    pub fps: u32,              // 30, 60, 120, 240, 360
    pub codec: VideoCodec,     // H264, H265, AV1
    pub max_bitrate_mbps: u32, // 5-200, 200 = unlimited

    // Audio settings
    pub audio_codec: AudioCodec,
    pub surround: bool,

    // Performance
    pub vsync: bool,
    pub low_latency_mode: bool,
    pub nvidia_reflex: bool,

    // Input
    pub mouse_sensitivity: f32,
    pub raw_input: bool,       // Windows only

    // Display
    pub fullscreen: bool,
    pub borderless: bool,
    pub stats_panel: bool,
    pub stats_position: StatsPosition,

    // Network
    pub preferred_region: Option<String>,
    pub proxy: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub enum VideoCodec {
    H264,
    H265,
    AV1,
    Auto, // Let server decide
}
```

#### 1.3 Authentication Module
- Port OAuth flow from web client
- JWT token management
- Token persistence and refresh

### Phase 2: WebRTC Implementation (Week 2)

#### 2.1 Signaling Protocol
```rust
// src/webrtc/signaling.rs
// Ported from native/signaling.rs with improvements

pub struct GfnSignaling {
    server_ip: String,
    session_id: String,
    ws: Option<WebSocketStream>,
    event_tx: mpsc::Sender<SignalingEvent>,
}

impl GfnSignaling {
    pub async fn connect(&mut self) -> Result<()> {
        // WebSocket URL: wss://{server}/nvst/sign_in?peer_id=peer-{random}&version=2
        // Subprotocol: x-nv-sessionid.{session_id}

        // Key implementation details from web client:
        // 1. Accept self-signed certs (GFN servers)
        // 2. Send peer_info immediately after connect
        // 3. Handle heartbeats (hb) every 5 seconds
        // 4. ACK all messages with ackid
    }

    pub async fn send_answer(&self, sdp: &str) -> Result<()>;
    pub async fn send_ice_candidate(&self, candidate: &IceCandidate) -> Result<()>;
}
```

#### 2.2 Peer Connection Management
```rust
// src/webrtc/peer.rs
// Enhanced from existing webrtc_client.rs

pub struct WebRtcPeer {
    connection: RTCPeerConnection,
    input_channel: Option<Arc<RTCDataChannel>>,
    video_track_rx: mpsc::Receiver<Vec<u8>>,
    audio_track_rx: mpsc::Receiver<Vec<u8>>,
}

impl WebRtcPeer {
    pub async fn handle_offer(&mut self, sdp: &str, ice_servers: Vec<IceServer>) -> Result<String> {
        // CRITICAL: Create input_channel_v1 BEFORE setRemoteDescription
        // This is required by GFN protocol (discovered from web client)

        let input_channel = self.connection.create_data_channel(
            "input_channel_v1",
            Some(RTCDataChannelInit {
                ordered: Some(false),      // Unordered for lowest latency
                max_retransmits: Some(0),  // No retransmits
                ..Default::default()
            }),
        ).await?;

        // Set remote description (server's offer)
        // Create and send answer
        // Wait for ICE gathering
    }
}
```

#### 2.3 SDP Manipulation
```rust
// src/webrtc/sdp.rs
// Codec forcing logic from streaming.ts preferCodec()

pub fn prefer_codec(sdp: &str, codec: VideoCodec) -> String {
    // Parse SDP lines
    // Find video section (m=video)
    // Identify payload types for each codec via a=rtpmap
    // Rewrite m=video line to only include preferred codec payloads
    // Remove a=rtpmap, a=fmtp, a=rtcp-fb lines for other codecs
}

pub fn fix_ice_candidates(sdp: &str, server_ip: &str) -> String {
    // Replace 0.0.0.0 with actual server IP
    // Add host candidates for ice-lite compatibility
}
```

### Phase 3: Media Pipeline (Week 3)

#### 3.1 RTP Depacketization
```rust
// src/media/rtp.rs

pub struct RtpDepacketizer {
    codec: VideoCodec,
    // H.264: NAL unit assembly from fragmented packets
    // H.265: Similar NAL unit handling
    // AV1: OBU (Open Bitstream Unit) assembly
}

impl RtpDepacketizer {
    pub fn process_packet(&mut self, rtp_data: &[u8]) -> Option<DecodableFrame> {
        // Extract payload from RTP
        // Handle fragmentation (FU-A for H.264)
        // Assemble complete NAL units
        // Return complete frames for decoding
    }
}
```

#### 3.2 Video Decoder (FFmpeg)
```rust
// src/media/video_decoder.rs

pub struct VideoDecoder {
    decoder: ffmpeg::decoder::Video,
    scaler: Option<ffmpeg::software::scaling::Context>,
    hw_accel: bool,
}

impl VideoDecoder {
    pub fn new(codec: VideoCodec, hw_accel: bool) -> Result<Self> {
        let codec_id = match codec {
            VideoCodec::H264 => ffmpeg::codec::Id::H264,
            VideoCodec::H265 => ffmpeg::codec::Id::HEVC,
            VideoCodec::AV1 => ffmpeg::codec::Id::AV1,
        };

        let mut decoder = ffmpeg::decoder::find(codec_id)
            .ok_or(anyhow!("Codec not found"))?
            .video()?;

        // Try hardware acceleration
        if hw_accel {
            #[cfg(target_os = "windows")]
            Self::try_dxva2(&mut decoder);

            #[cfg(target_os = "macos")]
            Self::try_videotoolbox(&mut decoder);

            #[cfg(target_os = "linux")]
            Self::try_vaapi(&mut decoder);
        }

        Ok(Self { decoder, scaler: None, hw_accel })
    }

    pub fn decode(&mut self, data: &[u8]) -> Result<Option<DecodedFrame>> {
        // Send packet to decoder
        // Receive frame (YUV420P typically)
        // Return decoded frame for rendering
    }
}
```

#### 3.3 GPU Rendering (wgpu)
```rust
// src/gui/renderer.rs

pub struct VideoRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    texture: wgpu::Texture,
    pipeline: wgpu::RenderPipeline,
    // YUV textures for efficient upload
    y_texture: wgpu::Texture,
    u_texture: wgpu::Texture,
    v_texture: wgpu::Texture,
}

impl VideoRenderer {
    pub fn upload_frame(&mut self, frame: &DecodedFrame) {
        // Upload Y, U, V planes separately
        // This is more efficient than CPU RGB conversion

        self.queue.write_texture(
            self.y_texture.as_image_copy(),
            &frame.y_plane,
            // ...
        );
        // Same for U and V
    }

    pub fn render(&self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
        // Run YUV→RGB shader
        // Draw fullscreen quad with video texture
    }
}
```

#### 3.4 YUV to RGB Shader
```wgsl
// assets/shaders/video.wgsl

@group(0) @binding(0) var y_texture: texture_2d<f32>;
@group(0) @binding(1) var u_texture: texture_2d<f32>;
@group(0) @binding(2) var v_texture: texture_2d<f32>;
@group(0) @binding(3) var tex_sampler: sampler;

@fragment
fn fs_main(@location(0) tex_coords: vec2<f32>) -> @location(0) vec4<f32> {
    let y = textureSample(y_texture, tex_sampler, tex_coords).r;
    let u = textureSample(u_texture, tex_sampler, tex_coords).r - 0.5;
    let v = textureSample(v_texture, tex_sampler, tex_coords).r - 0.5;

    // BT.709 YUV to RGB conversion (for HD content)
    let r = y + 1.5748 * v;
    let g = y - 0.1873 * u - 0.4681 * v;
    let b = y + 1.8556 * u;

    return vec4<f32>(r, g, b, 1.0);
}
```

### Phase 4: Audio Pipeline (Week 4)

#### 4.1 Opus Decoder
```rust
// src/media/audio_decoder.rs

pub struct AudioDecoder {
    decoder: opus::Decoder,
    sample_rate: u32,
    channels: opus::Channels,
}

impl AudioDecoder {
    pub fn new(sample_rate: u32, channels: u32) -> Result<Self> {
        let channels = match channels {
            1 => opus::Channels::Mono,
            2 => opus::Channels::Stereo,
            _ => return Err(anyhow!("Unsupported channel count")),
        };

        let decoder = opus::Decoder::new(sample_rate, channels)?;
        Ok(Self { decoder, sample_rate, channels })
    }

    pub fn decode(&mut self, data: &[u8]) -> Result<Vec<i16>> {
        let mut output = vec![0i16; 5760]; // Max frame size
        let samples = self.decoder.decode(data, &mut output, false)?;
        output.truncate(samples * self.channels.count());
        Ok(output)
    }
}
```

#### 4.2 Audio Playback (cpal)
```rust
// src/media/audio_player.rs

pub struct AudioPlayer {
    stream: cpal::Stream,
    buffer_tx: mpsc::Sender<Vec<i16>>,
}

impl AudioPlayer {
    pub fn new() -> Result<Self> {
        let host = cpal::default_host();
        let device = host.default_output_device()
            .ok_or(anyhow!("No audio output device"))?;

        let config = device.default_output_config()?;

        let (buffer_tx, mut buffer_rx) = mpsc::channel::<Vec<i16>>(64);

        let stream = device.build_output_stream(
            &config.into(),
            move |data: &mut [i16], _| {
                // Fill from buffer_rx
                if let Ok(samples) = buffer_rx.try_recv() {
                    for (i, sample) in samples.iter().enumerate() {
                        if i < data.len() {
                            data[i] = *sample;
                        }
                    }
                }
            },
            |err| eprintln!("Audio error: {}", err),
            None,
        )?;

        stream.play()?;
        Ok(Self { stream, buffer_tx })
    }

    pub fn push_samples(&self, samples: Vec<i16>) {
        let _ = self.buffer_tx.try_send(samples);
    }
}
```

### Phase 5: Input System (Week 5)

#### 5.1 GFN Binary Input Protocol
```rust
// src/input/protocol.rs
// Ported from native/input.rs with full protocol support

pub const INPUT_HEARTBEAT: u32 = 2;
pub const INPUT_KEY_UP: u32 = 3;
pub const INPUT_KEY_DOWN: u32 = 4;
pub const INPUT_MOUSE_ABS: u32 = 5;
pub const INPUT_MOUSE_REL: u32 = 7;
pub const INPUT_MOUSE_BUTTON_DOWN: u32 = 8;
pub const INPUT_MOUSE_BUTTON_UP: u32 = 9;
pub const INPUT_MOUSE_WHEEL: u32 = 10;

pub struct InputEncoder {
    protocol_version: u8,
    stream_start_time: Instant,
}

impl InputEncoder {
    pub fn encode(&self, event: &InputEvent) -> Vec<u8> {
        let mut buf = BytesMut::with_capacity(32);

        match event {
            InputEvent::MouseMove { dx, dy } => {
                // Type 7 (Mouse Relative): 22 bytes
                // [type 4B LE][dx 2B BE][dy 2B BE][reserved 6B][timestamp 8B BE]
                buf.put_u32_le(INPUT_MOUSE_REL);
                buf.put_i16(*dx);  // BE
                buf.put_i16(*dy);  // BE
                buf.put_u16(0);    // Reserved
                buf.put_u32(0);    // Reserved
                buf.put_u64(self.timestamp_us());
            }
            InputEvent::KeyDown { scancode, modifiers } => {
                // Type 4 (Key Down): 18 bytes
                // [type 4B LE][keycode 2B BE][modifiers 2B BE][scancode 2B BE][timestamp 8B BE]
                buf.put_u32_le(INPUT_KEY_DOWN);
                buf.put_u16(0);             // Keycode (unused)
                buf.put_u16(*modifiers);
                buf.put_u16(*scancode);
                buf.put_u64(self.timestamp_us());
            }
            // ... other event types
        }

        // Protocol v3+ requires header wrapper
        if self.protocol_version > 2 {
            let mut final_buf = BytesMut::with_capacity(10 + buf.len());
            final_buf.put_u8(0x23);  // Header marker
            final_buf.put_u64(self.timestamp_us());
            final_buf.put_u8(0x22);  // Single event wrapper
            final_buf.extend_from_slice(&buf);
            final_buf.to_vec()
        } else {
            buf.to_vec()
        }
    }

    fn timestamp_us(&self) -> u64 {
        self.stream_start_time.elapsed().as_micros() as u64
    }
}
```

#### 5.2 Windows Input (Raw Input + Cursor Clip)
```rust
// src/input/windows.rs

use windows::Win32::UI::Input::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub struct WindowsInputHandler {
    hwnd: HWND,
    cursor_captured: bool,
    accumulated_dx: AtomicI32,
    accumulated_dy: AtomicI32,
}

impl WindowsInputHandler {
    pub fn capture_cursor(&mut self) -> Result<()> {
        unsafe {
            // Register for raw input (high-frequency mouse)
            let rid = RAWINPUTDEVICE {
                usUsagePage: 0x01,  // Generic Desktop
                usUsage: 0x02,      // Mouse
                dwFlags: RIDEV_INPUTSINK,
                hwndTarget: self.hwnd,
            };
            RegisterRawInputDevices(&[rid], std::mem::size_of::<RAWINPUTDEVICE>() as u32)?;

            // Clip cursor to window
            let mut rect = RECT::default();
            GetClientRect(self.hwnd, &mut rect)?;
            ClientToScreen(self.hwnd, &mut rect as *mut _ as *mut POINT)?;
            ClipCursor(Some(&rect))?;

            // Hide cursor
            ShowCursor(false);

            self.cursor_captured = true;
        }
        Ok(())
    }

    pub fn process_raw_input(&self, raw: &RAWINPUT) -> Option<InputEvent> {
        if raw.header.dwType == RIM_TYPEMOUSE as u32 {
            let mouse = unsafe { raw.data.mouse };

            // Accumulate deltas (for high-frequency polling)
            self.accumulated_dx.fetch_add(mouse.lLastX, Ordering::Relaxed);
            self.accumulated_dy.fetch_add(mouse.lLastY, Ordering::Relaxed);

            Some(InputEvent::MouseMove {
                dx: mouse.lLastX as i16,
                dy: mouse.lLastY as i16,
            })
        } else {
            None
        }
    }

    pub fn release_cursor(&mut self) {
        unsafe {
            ClipCursor(None);
            ShowCursor(true);
            self.cursor_captured = false;
        }
    }
}
```

#### 5.3 macOS Input (CGEvent + Quartz)
```rust
// src/input/macos.rs

use core_graphics::event::*;
use core_graphics::display::*;

pub struct MacOSInputHandler {
    event_tap: CFMachPortRef,
    run_loop_source: CFRunLoopSourceRef,
    cursor_captured: bool,
    center_x: f64,
    center_y: f64,
}

impl MacOSInputHandler {
    pub fn capture_cursor(&mut self, window_bounds: CGRect) -> Result<()> {
        // Calculate window center
        self.center_x = window_bounds.origin.x + window_bounds.size.width / 2.0;
        self.center_y = window_bounds.origin.y + window_bounds.size.height / 2.0;

        // Hide cursor
        CGDisplayHideCursor(CGMainDisplayID());

        // Warp cursor to center
        CGWarpMouseCursorPosition(CGPoint { x: self.center_x, y: self.center_y });

        // Disassociate mouse and cursor (for FPS games)
        CGAssociateMouseAndMouseCursorPosition(0);

        self.cursor_captured = true;
        Ok(())
    }

    pub fn handle_mouse_moved(&self, event: CGEvent) -> InputEvent {
        let dx = event.get_integer_value_field(CGEventField::MouseEventDeltaX);
        let dy = event.get_integer_value_field(CGEventField::MouseEventDeltaY);

        InputEvent::MouseMove {
            dx: dx as i16,
            dy: dy as i16,
        }
    }

    pub fn release_cursor(&mut self) {
        CGDisplayShowCursor(CGMainDisplayID());
        CGAssociateMouseAndMouseCursorPosition(1);
        self.cursor_captured = false;
    }
}
```

#### 5.4 Linux Input (evdev/libinput)
```rust
// src/input/linux.rs

use evdev::{Device, InputEventKind, RelativeAxisType};

pub struct LinuxInputHandler {
    mouse_device: Option<Device>,
    cursor_captured: bool,
}

impl LinuxInputHandler {
    pub fn new() -> Result<Self> {
        // Find mouse device
        let mouse = evdev::enumerate()
            .filter_map(|(_, device)| {
                if device.supported_relative_axes().map_or(false, |axes| {
                    axes.contains(RelativeAxisType::REL_X) && axes.contains(RelativeAxisType::REL_Y)
                }) {
                    Some(device)
                } else {
                    None
                }
            })
            .next();

        Ok(Self {
            mouse_device: mouse,
            cursor_captured: false,
        })
    }

    pub fn capture_cursor(&mut self, window: &Window) -> Result<()> {
        // Grab mouse device exclusively
        if let Some(ref mut device) = self.mouse_device {
            device.grab()?;
        }

        // Use XGrabPointer for X11 or zwp_pointer_constraints for Wayland
        // Hide cursor

        self.cursor_captured = true;
        Ok(())
    }

    pub fn poll_events(&mut self) -> Vec<InputEvent> {
        let mut events = Vec::new();

        if let Some(ref mut device) = self.mouse_device {
            for ev in device.fetch_events().ok().into_iter().flatten() {
                match ev.kind() {
                    InputEventKind::RelAxis(axis) => {
                        match axis {
                            RelativeAxisType::REL_X => {
                                events.push(InputEvent::MouseMove {
                                    dx: ev.value() as i16,
                                    dy: 0,
                                });
                            }
                            RelativeAxisType::REL_Y => {
                                events.push(InputEvent::MouseMove {
                                    dx: 0,
                                    dy: ev.value() as i16,
                                });
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }

        events
    }
}
```

### Phase 6: GUI & Stats Panel (Week 6)

#### 6.1 Stats Panel (Bottom-Left)
```rust
// src/gui/stats_panel.rs

pub struct StatsPanel {
    visible: bool,
    position: StatsPosition, // TopLeft, TopRight, BottomLeft, BottomRight
    stats: StreamStats,
}

#[derive(Default)]
pub struct StreamStats {
    pub resolution: String,      // "1920x1080"
    pub fps: f32,               // Current FPS
    pub target_fps: u32,        // Target FPS
    pub bitrate_mbps: f32,      // Video bitrate
    pub latency_ms: f32,        // Network latency
    pub decode_time_ms: f32,    // Frame decode time
    pub render_time_ms: f32,    // Frame render time
    pub codec: String,          // "H.264" / "H.265" / "AV1"
    pub gpu_type: String,       // "RTX 4080" etc
    pub server_region: String,  // "EU West"
    pub packet_loss: f32,       // %
    pub jitter_ms: f32,
}

impl StatsPanel {
    pub fn render(&self, ctx: &egui::Context) {
        if !self.visible {
            return;
        }

        let anchor = match self.position {
            StatsPosition::BottomLeft => egui::Align2::LEFT_BOTTOM,
            StatsPosition::BottomRight => egui::Align2::RIGHT_BOTTOM,
            StatsPosition::TopLeft => egui::Align2::LEFT_TOP,
            StatsPosition::TopRight => egui::Align2::RIGHT_TOP,
        };

        egui::Area::new("stats_panel")
            .anchor(anchor, [10.0, -10.0])
            .show(ctx, |ui| {
                ui.style_mut().override_font_id = Some(egui::FontId::monospace(12.0));

                egui::Frame::none()
                    .fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 180))
                    .rounding(4.0)
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.colored_label(egui::Color32::WHITE, format!(
                            "{} @ {} fps", self.stats.resolution, self.stats.fps as u32
                        ));
                        ui.colored_label(egui::Color32::LIGHT_GRAY, format!(
                            "{} • {:.1} Mbps", self.stats.codec, self.stats.bitrate_mbps
                        ));
                        ui.colored_label(egui::Color32::LIGHT_GRAY, format!(
                            "Latency: {:.0} ms • Loss: {:.1}%",
                            self.stats.latency_ms, self.stats.packet_loss
                        ));
                        ui.colored_label(egui::Color32::LIGHT_GRAY, format!(
                            "Decode: {:.1} ms • Render: {:.1} ms",
                            self.stats.decode_time_ms, self.stats.render_time_ms
                        ));
                        ui.colored_label(egui::Color32::DARK_GRAY, format!(
                            "{} • {}", self.stats.gpu_type, self.stats.server_region
                        ));
                    });
            });
    }
}
```

#### 6.2 Window Management
```rust
// src/gui/window.rs

pub struct MainWindow {
    window: winit::window::Window,
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    fullscreen: bool,
}

impl MainWindow {
    pub fn new(event_loop: &EventLoop<()>) -> Result<Self> {
        let window = WindowBuilder::new()
            .with_title("OpenNow Streamer")
            .with_inner_size(LogicalSize::new(1920.0, 1080.0))
            .with_resizable(true)
            .build(event_loop)?;

        // Set up wgpu surface
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(&window)?;

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })).ok_or(anyhow!("No suitable GPU adapter"))?;

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor::default(),
            None,
        ))?;

        Ok(Self { window, surface, device, queue, fullscreen: false })
    }

    pub fn toggle_fullscreen(&mut self) {
        self.fullscreen = !self.fullscreen;
        self.window.set_fullscreen(if self.fullscreen {
            Some(winit::window::Fullscreen::Borderless(None))
        } else {
            None
        });
    }
}
```

---

## Performance Optimizations

### 1. Low-Latency Video Pipeline
```
RTP Packet → Zero-Copy Depacketize → HW Decode → Direct GPU Upload
                     ↓
             Ring buffer (3 frames)
                     ↓
          Present with minimal vsync delay
```

### 2. Input Optimizations
- **Windows**: Raw Input API at 1000Hz polling
- **macOS**: CGEvent tap with disassociated cursor
- **Linux**: evdev with exclusive grab

### 3. Memory Optimizations
- Pre-allocated frame buffers
- Ring buffer for decoded frames
- Zero-copy where possible

### 4. Thread Architecture
```
Main Thread:       Window events, rendering
Decode Thread:     Video decoding (FFmpeg)
Audio Thread:      Audio decode + playback
Network Thread:    WebRTC, signaling
Input Thread:      High-frequency input polling (Windows)
```

---

## Build & Distribution

### Cross-Compilation

```bash
# Windows (MSVC)
cargo build --release --target x86_64-pc-windows-msvc

# macOS (Universal)
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin
lipo -create -output target/opennow-streamer \
    target/aarch64-apple-darwin/release/opennow-streamer \
    target/x86_64-apple-darwin/release/opennow-streamer

# Linux
cargo build --release --target x86_64-unknown-linux-gnu
```

### FFmpeg Bundling
- Windows: Bundle ffmpeg.dll (or statically link)
- macOS: Bundle dylibs or use VideoToolbox
- Linux: Require libffmpeg as system dependency

---

## Testing Strategy

1. **Unit Tests**: Input encoding, SDP parsing, RTP depacketization
2. **Integration Tests**: WebRTC connection with mock server
3. **Manual Tests**: Real GFN server connection
4. **Performance Tests**: Frame latency, input latency, CPU/GPU usage

---

## Timeline Summary

| Phase | Duration | Deliverables |
|-------|----------|--------------|
| 1. Core | Week 1 | Project setup, config, auth |
| 2. WebRTC | Week 2 | Signaling, peer connection, SDP |
| 3. Video | Week 3 | RTP, FFmpeg decode, GPU render |
| 4. Audio | Week 4 | Opus decode, cpal playback |
| 5. Input | Week 5 | Platform-native capture |
| 6. GUI | Week 6 | Stats panel, fullscreen, polish |

---

## Next Steps

1. **Approve this plan** or suggest modifications
2. **Start with Phase 1**: Create project structure
3. **Iterate**: Build incrementally, test each component
