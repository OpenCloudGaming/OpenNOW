<h1 align="center">OpenNOW</h1>

<p align="center">
  <strong>Open source GeForce NOW client built from the ground up in Native Rust</strong>
</p>

<p align="center">
  <a href="https://github.com/zortos293/GFNClient/releases">
    <img src="https://img.shields.io/github/v/tag/zortos293/GFNClient?style=for-the-badge&label=Download" alt="Download">
  </a>
  <a href="https://github.com/zortos293/GFNClient/stargazers">
    <img src="https://img.shields.io/github/stars/zortos293/GFNClient?style=for-the-badge" alt="Stars">
  </a>
  <a href="https://discord.gg/8EJYaJcNfD">
    <img src="https://img.shields.io/badge/Discord-Join%20Us-7289da?style=for-the-badge&logo=discord" alt="Discord">
  </a>
</p>

---

## Disclaimer

This is an **independent project** not affiliated with NVIDIA Corporation. Created for educational purposes. GeForce NOW is a trademark of NVIDIA. Use at your own risk.

---

## About

OpenNOW is a custom GeForce NOW client rewritten entirely in **Native Rust** (moving away from the previous Tauri implementation) for maximum performance and lower resource usage. It uses `wgpu` and `egui` to provide a seamless, high-performance cloud gaming experience.

**Why OpenNOW?**
- **Native Performance**: Written in Rust with zero-overhead graphics bindings.
- **Uncapped Potential**: No artificial limits on FPS, resolution, or bitrate.
- **Privacy Focused**: No telemetry by default.
- **Cross-Platform**: Designed for Windows, macOS, and Linux (including Raspberry Pi).

---

## Platform Support

| Platform | Architecture | Status | Notes |
|----------|--------------|--------|-------|
| **Windows** | x64 | ✅ Working | D3D11VA zero-copy decoding. NVIDIA, AMD, and Intel GPUs supported. |
| **Windows** | ARM64 | ❓ Untested | Should work but not verified. |
| **macOS** | ARM64 / x64 | ✅ Working | VideoToolbox zero-copy hardware decoding. |
| **Linux** | x64 | ✅ Working | VAAPI zero-copy for AMD/Intel. NVDEC for NVIDIA. |
| **Linux** | ARM64 | ✅ Working | Full support including Raspberry Pi. |
| **Raspberry Pi 4** | ARM64 | ✅ Working | V4L2 H.264 hardware decoding via bcm2835-codec. |
| **Raspberry Pi 5** | ARM64 | ✅ Working | V4L2 H.264/HEVC hardware decoding via rpivid. |
| **Android** | ARM64 | 📅 Planned | No ETA. |
| **Apple TV** | ARM64 | 📅 Planned | No ETA. |

---

## Features & Implementation Status

| Component | Feature | Status | Notes |
|-----------|---------|:------:|-------|
| **Core** | Authentication | ✅ | Secure login flow. |
| **Core** | Game Library | ✅ | Search & browse via Cloudmatch integration. |
| **Streaming** | RTP/WebRTC | ✅ | Low-latency streaming implementation. |
| **Streaming** | Hardware Decoding | ✅ | Windows (D3D11VA), macOS (VideoToolbox), Linux (VAAPI/V4L2). |
| **Streaming** | Zero-Copy Rendering | ✅ | GPU textures passed directly to renderer (no CPU copy). |
| **Input** | Mouse/Keyboard | ✅ | Raw input capture (Windows Raw Input, macOS CGEventTap, Linux evdev). |
| **Input** | Gamepad | ✅ | Cross-platform support via `gilrs`. |
| **Input** | Clipboard Paste | 🚧 | Planned. |
| **Audio** | Playback | ✅ | Low-latency audio via `cpal`. |
| **Audio** | Microphone | 🚧 | Planned. |
| **UI** | Overlay | ✅ | In-stream stats & settings (egui). |
| **Media** | Instant Replay | 🚧 | Coming Soon (NVIDIA-like). |
| **Media** | Screenshots | 🚧 | Coming Soon. |

### Supported Codecs & Hardware Acceleration

| Codec | Windows | macOS | Linux (Desktop) | Linux (Raspberry Pi) |
|:---:|:---:|:---:|:---:|:---:|
| **H.264** | ✅ D3D11VA / NVDEC / QSV | ✅ VideoToolbox | ✅ VAAPI / NVDEC | ✅ V4L2 (Pi 4/5) |
| **HEVC (H.265)** | ✅ D3D11VA / NVDEC / QSV | ✅ VideoToolbox | ✅ VAAPI / NVDEC | ✅ V4L2 (Pi 5 only) |
| **AV1** | ✅ NVDEC / QSV | ✅ VideoToolbox (M3+) | ⚠️ VAAPI | ❌ No HW support |
| **Opus (Audio)** | ✅ Software | ✅ Software | ✅ Software | ✅ Software |

> **Note:** Zero-copy rendering eliminates GPU→CPU→GPU transfers for minimal latency.

### GPU Support Matrix

| GPU Vendor | Windows | macOS | Linux |
|------------|---------|-------|-------|
| **NVIDIA** | D3D11VA (zero-copy) / NVDEC | N/A | NVDEC / nvidia-vaapi-driver |
| **AMD** | D3D11VA (zero-copy) | N/A | VAAPI (mesa/radeonsi) |
| **Intel** | D3D11VA / QSV | N/A | VAAPI (iHD/i965) / QSV |
| **Apple Silicon** | N/A | VideoToolbox (zero-copy) | N/A |
| **Broadcom (Pi)** | N/A | N/A | V4L2 M2M (bcm2835/rpivid) |

### Additional Features (Exclusive)
These features are not found in the official client:

| Feature | Status | Description |
|---------|:------:|-------------|
| **Plugin Support** | 🚧 | Add custom scripts to interact with stream controls/input. |
| **Theming** | 🚧 | Full UI customization and community themes. |
| **Multi-account** | 🚧 | Switch between GFN accounts seamlessly. |
| **Anti-AFK** | ✅ | Prevent session timeout (Ctrl+Shift+F10). |
| **Queue Monitor** | 🚧 | printedwaste integration by [@Kief5555](https://github.com/Kief5555) (View server queues). |

### Controls & Shortcuts

| Shortcut | Action | Description |
|----------|--------|-------------|
| **F11** | Keybind | Toggle Fullscreen |
| **F3** | Keybind | Toggle Stats Overlay |
| **Ctrl+Shift+Q** | Keybind | Force Quit Session |
| **Ctrl+Shift+F10**| Keybind | **Toggle Anti-AFK** (Status shows in console) |

---

## Building

### Requirements

**All Platforms:**
- Rust toolchain (1.75+)
- FFmpeg development libraries (v6.1+ recommended)

**Windows:**
- Visual Studio Build Tools with C++ workload
- FFmpeg (via vcpkg or manual install)

**macOS:**
- Xcode Command Line Tools
- FFmpeg (`brew install ffmpeg`)

**Linux:**
- Build essentials (`build-essential` / `base-devel`)
- FFmpeg dev libraries (`libavcodec-dev`, `libavformat-dev`, etc.)
- X11 dev libraries (`libx11-dev`, `libxext-dev`, `libxi-dev`)
- For VAAPI: `libva-dev`
- For evdev input: user must be in `input` group

### Build Commands

```bash
git clone https://github.com/zortos293/GFNClient.git
cd GFNClient/opennow-streamer
cargo build --release
```

To run in development mode:

```bash
cd opennow-streamer
cargo run
```

---

## Linux Setup

### Desktop Linux (AMD/Intel/NVIDIA)

1. **Install dependencies:**

   ```bash
   # Ubuntu/Debian
   sudo apt install build-essential pkg-config \
     libavcodec-dev libavformat-dev libavutil-dev libswscale-dev \
     libx11-dev libxext-dev libxi-dev libva-dev

   # Fedora
   sudo dnf install @development-tools pkg-config \
     ffmpeg-devel libX11-devel libXext-devel libXi-devel libva-devel

   # Arch
   sudo pacman -S base-devel pkg-config ffmpeg libx11 libxext libxi libva
   ```

2. **Install GPU-specific drivers:**

   ```bash
   # AMD (mesa VAAPI)
   sudo apt install mesa-va-drivers

   # Intel
   sudo apt install intel-media-va-driver  # or i965-va-driver for older GPUs

   # NVIDIA (for VAAPI via nvidia-vaapi-driver)
   # See: https://github.com/elFarto/nvidia-vaapi-driver
   ```

3. **Add user to input group (for raw mouse input):**

   ```bash
   sudo usermod -aG input $USER
   # Log out and back in
   ```

### Raspberry Pi

1. **Update system:**

   ```bash
   sudo apt update && sudo apt upgrade
   sudo rpi-update  # Optional: latest firmware
   ```

2. **Install dependencies:**

   ```bash
   sudo apt install build-essential pkg-config \
     libavcodec-dev libavformat-dev libavutil-dev libswscale-dev \
     libx11-dev libxext-dev libxi-dev
   ```

3. **Add user to input group:**

   ```bash
   sudo usermod -aG input $USER
   # Log out and back in
   ```

4. **GPU Memory (Pi 4):**

   For best performance, allocate at least 256MB to the GPU:
   ```bash
   sudo raspi-config
   # Performance Options → GPU Memory → 256
   ```

5. **Recommended codec:**
   - **Pi 4**: Use H.264 (only hardware decoder available)
   - **Pi 5**: H.264 or HEVC both supported

---

## Troubleshooting

### macOS: "App is damaged"
If macOS blocks the app, run:
```bash
xattr -d com.apple.quarantine /Applications/OpenNOW.app
```

### Linux: Mouse not working
Ensure you're in the `input` group:
```bash
groups  # Should show 'input'
# If not:
sudo usermod -aG input $USER
# Then log out and back in
```

### Linux: No hardware decoding
Check VAAPI is working:
```bash
vainfo
# Should show supported profiles
```

### Raspberry Pi: Decoder issues
Verify V4L2 devices exist:
```bash
ls -la /dev/video*
# Should show video10, video11, etc.
```

Check codec support:
```bash
v4l2-ctl --list-formats-out -d /dev/video10
```

---

## Support the Project

OpenNOW is a passion project developed entirely in my free time. I truly believe in open software and giving users control over their experience.

If you enjoy using the client and want to support its continued development (and keep me caffeinated), please consider becoming a sponsor. Your support helps me dedicate more time to fixing bugs, adding new features, and maintaining the project.

<p align="center">
  <a href="https://github.com/sponsors/zortos293">
    <img src="https://img.shields.io/badge/Sponsor_on_GitHub-EA4AAA?style=for-the-badge&logo=github-sponsors&logoColor=white" alt="Sponsor on GitHub">
  </a>
</p>

---

<p align="center">
  Made by <a href="https://github.com/zortos293">zortos293</a>
</p>
