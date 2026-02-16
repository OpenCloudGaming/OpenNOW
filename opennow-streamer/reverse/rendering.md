# GeForce NOW Video Rendering & Shaders - Reverse Engineering Documentation

## 1. GPU Architecture

### Framework Selection
- **Windows**: wgpu with DirectX 12 backend (exclusive fullscreen support)
- **macOS**: wgpu with Metal backend
- **Linux**: wgpu with Vulkan backend

### Backend Priority
```rust
#[cfg(target_os = "windows")]
let backends = wgpu::Backends::DX12;  // Forced for exclusive fullscreen

#[cfg(target_os = "macos")]
let backends = wgpu::Backends::METAL;

#[cfg(target_os = "linux")]
let backends = wgpu::Backends::VULKAN;
```

---

## 2. WGSL Shader Architecture

### Video Shader (YUV420P)
3 separate texture planes:
- Y plane: R8Unorm (full resolution)
- U plane: R8Unorm (half resolution)
- V plane: R8Unorm (half resolution)

```wgsl
@group(0) @binding(0) var y_texture: texture_2d<f32>;
@group(0) @binding(1) var u_texture: texture_2d<f32>;
@group(0) @binding(2) var v_texture: texture_2d<f32>;
@group(0) @binding(3) var tex_sampler: sampler;

@fragment
fn fs_main(@location(0) tex_coord: vec2<f32>) -> @location(0) vec4<f32> {
    let y_raw = textureSample(y_texture, tex_sampler, tex_coord).r;
    let u_raw = textureSample(u_texture, tex_sampler, tex_coord).r;
    let v_raw = textureSample(v_texture, tex_sampler, tex_coord).r;

    // BT.709 limited range to full range
    let y = (y_raw - 0.0625) * 1.1644;
    let u = (u_raw - 0.5) * 1.1384;
    let v = (v_raw - 0.5) * 1.1384;

    // BT.709 color matrix
    let r = y + 1.5748 * v;
    let g = y - 0.1873 * u - 0.4681 * v;
    let b = y + 1.8556 * u;

    return vec4<f32>(clamp(r, 0.0, 1.0), clamp(g, 0.0, 1.0), clamp(b, 0.0, 1.0), 1.0);
}
```

### NV12 Shader (Semi-planar)
2 planes with interleaved UV:
- Y plane: R8Unorm (full resolution)
- UV plane: Rg8Unorm (half resolution, interleaved)

```wgsl
@group(0) @binding(0) var y_texture: texture_2d<f32>;
@group(0) @binding(1) var uv_texture: texture_2d<f32>;
@group(0) @binding(2) var tex_sampler: sampler;

@fragment
fn fs_main(@location(0) tex_coord: vec2<f32>) -> @location(0) vec4<f32> {
    let y_raw = textureSample(y_texture, tex_sampler, tex_coord).r;
    let uv = textureSample(uv_texture, tex_sampler, tex_coord).rg;

    let y = (y_raw - 0.0625) * 1.1644;
    let u = (uv.r - 0.5) * 1.1384;
    let v = (uv.g - 0.5) * 1.1384;

    let r = y + 1.5748 * v;
    let g = y - 0.1873 * u - 0.4681 * v;
    let b = y + 1.8556 * u;

    return vec4<f32>(clamp(r, 0.0, 1.0), clamp(g, 0.0, 1.0), clamp(b, 0.0, 1.0), 1.0);
}
```

---

## 3. BT.709 Color Space Conversion

### Limited Range to Full Range
```
Y' = (Y - 16/255) × (255/219)  = (Y - 0.0625) × 1.1644
U' = (U - 128/255) × (255/224) = (U - 0.5) × 1.1384
V' = (V - 128/255) × (255/224) = (V - 0.5) × 1.1384
```

### BT.709 Matrix
```
R = Y' + 1.5748 × V'
G = Y' - 0.1873 × U' - 0.4681 × V'
B = Y' + 1.8556 × U'
```

### Matrix Form
```
[R]   [1.0000   0.0000   1.5748] [Y']
[G] = [1.0000  -0.1873  -0.4681] [U']
[B]   [1.0000   1.8556   0.0000] [V']
```

### CPU Fallback (Integer Math)
```rust
let r = (y + ((359 * v) >> 8)).clamp(0, 255) as u8;
let g = (y - ((88 * u + 183 * v) >> 8)).clamp(0, 255) as u8;
let b = (y + ((454 * u) >> 8)).clamp(0, 255) as u8;
```

---

## 4. Texture Formats

### YUV420P Memory Layout (1920×1080)
```
Y plane: 1920 × 1080 = 2,073,600 bytes
U plane:  960 ×  540 =   518,400 bytes
V plane:  960 ×  540 =   518,400 bytes
Total:                  3,110,400 bytes (2.97 MB/frame)
```

### NV12 Memory Layout (1920×1080)
```
Y plane:  1920 × 1080 = 2,073,600 bytes
UV plane: 1920 ×  540 = 1,036,800 bytes (interleaved)
Total:                  3,110,400 bytes (same size)
```

---

## 5. Present Mode Configuration

### Latency Optimization
```rust
let present_mode = if caps.contains(&wgpu::PresentMode::Immediate) {
    wgpu::PresentMode::Immediate    // Best latency
} else if caps.contains(&wgpu::PresentMode::Mailbox) {
    wgpu::PresentMode::Mailbox      // Intermediate
} else {
    wgpu::PresentMode::Fifo         // VSync (fallback)
};

// Minimum frame latency
config.desired_maximum_frame_latency = 1;
```

### Present Mode Hierarchy
1. **Immediate**: No vsync, submit immediately (lowest latency)
2. **Mailbox**: Non-blocking buffer swap (intermediate)
3. **Fifo**: VSync blocking (highest latency)

---

## 6. Exclusive Fullscreen (Windows)

### DWM Bypass
- Bypasses Desktop Window Manager compositor
- Enables higher refresh rates (120Hz+)
- Lower input latency

### Implementation
```rust
// Find video mode with highest refresh rate
let modes = monitor.video_modes();
let best_mode = modes
    .filter(|m| m.size().width >= width && m.size().height >= height)
    .max_by_key(|m| m.refresh_rate_millihertz());

window.set_fullscreen(Some(Fullscreen::Exclusive(best_mode)));
```

---

## 7. macOS ProMotion Support

### Frame Rate Configuration
```rust
struct CAFrameRateRange {
    minimum: 120.0,
    maximum: 120.0,
    preferred: 120.0,  // Force 120Hz
}
```

### High-Performance Mode
- `NSActivityUserInitiated`: Prevents App Nap
- `NSActivityLatencyCritical`: Low-latency scheduling
- Disables auto-termination

---

## 8. Render Pipeline Stages

### Order of Operations
1. **Swapchain Error Recovery**: Handle Outdated/Lost surface
2. **Video Frame Update**: Upload YUV/NV12 planes to GPU
3. **Video Render Pass**: Execute shader on full-screen quad
4. **egui UI Render Pass**: Render overlay UI
5. **Present**: Display to screen

---

## 9. Hardware Decoder Integration

### Decoder Priority (Windows/Linux)
1. NVIDIA CUVID (H.264, H.265, AV1)
2. Intel QSV (H.264, H.265, AV1)
3. D3D11VA (Windows)
4. VAAPI (Linux)
5. Software decoder (fallback)

### Decoder Priority (macOS)
1. VideoToolbox (native)
2. Software decoder (fallback)

### Output Formats
- **NV12**: Direct from VideoToolbox, CUVID, QSV (preferred)
- **YUV420P**: Converted via FFmpeg if needed

---

## 10. Zero-Latency Frame Delivery

### SharedFrame Structure
```rust
pub struct SharedFrame {
    frame: Mutex<Option<VideoFrame>>,
    frame_count: AtomicU64,
    last_read_count: AtomicU64,
}
```

### Design Principles
- Decoder writes latest frame to SharedFrame
- Renderer reads via take() (zero copy)
- No frame buffering = always most recent
- Atomic frame counter detects new frames

---

## 11. Comparison

| Feature | Web Client | OpenNow | Official Client |
|---------|-----------|---------|-----------------|
| Rendering API | WebGL/WebGPU | wgpu (Rust) | DirectX 12/Vulkan |
| YUV Conversion | GPU shader | WGSL shader | HLSL shader |
| Color Space | BT.709 | BT.709 | BT.709 |
| Texture Format | YUV420P/NV12 | YUV420P/NV12 | NV12 |
| Present Mode | Vsync | Immediate | Exclusive fullscreen |
| Latency | 40-80ms | <20ms | 10-30ms |
| CPU Load | ~5-10% | ~5% | ~3-5% |

---

## 12. Performance Metrics

### GPU Memory (1440p)
```
Per-frame textures:
  Y plane:  2560 × 1440 = 3,686,400 bytes
  U plane:  1280 ×  720 =   921,600 bytes
  V plane:  1280 ×  720 =   921,600 bytes
  Total:                   5,529,600 bytes (~5.3 MB)

Triple buffering: ~15.9 MB total
```

### Frame Timing (1440p120)
```
Decode time:    8-12ms (hardware accelerated)
GPU shader:     <1ms
Render pass:    <2ms
Total:          <15ms per frame
```

---

## 13. Why BT.709?

- **HD Content Standard**: Streams are 720p+
- **Color Accuracy**: Better flesh tones
- **Industry Standard**: Used by all streaming services
- **Apple/NVIDIA Alignment**: Both prefer BT.709

---

## 14. NV12 Optimization Benefits

1. **No interleaving**: Single memory layout
2. **Fewer textures**: 2 instead of 3
3. **GPU efficiency**: Direct from hardware decoders
4. **Bandwidth**: Fewer texture fetch operations

---

## 15. HDR Considerations

### Current Status
- Not implemented in OpenNow
- Surface format: Linear (non-sRGB)
- No HDR10 texture formats
- No BT.2020 color space

### Future Requirements
- SCRGB (Extended Dynamic Range)
- BT.2020 color primaries
- SMPTE ST.2084 tone-mapping
