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
- **Cross-Platform**: Designed for Windows, macOS, and Linux.

---

## Platform Support

| Platform | Architecture | Status | Notes |
|----------|--------------|--------|-------|
| **macOS** | ARM64 / x64 | ✅ Working | Fully functional foundation. VideoToolbox hardware decoding supported. |
| **Windows** | x64 | ✅ Working | **Nvidia GPUs**: Tested & Working. <br> **AMD/Intel**: Untested (likely works via D3D11). |
| **Windows** | ARM64 | ❓ Untested | Should work but not verified. |
| **Linux** | x64 | ⚠️ Kinda Works | **Warning:** Persistent encoding/decoding issues may occur depending on distro/drivers. |
| **Linux** | ARM64 | ⚠️ Kinda Works | **Raspberry Pi 4**: Working (H.264). <br> **Raspberry Pi 5**: Untested. <br> **Asahi Linux**: ❌ Decode issues (No HW decoder yet). |
| **Android** | ARM64 | 📅 Planned | No ETA. |
| **Apple TV** | ARM64 | 📅 Planned | No ETA. |

---

## Features & Implementation Status

| Component | Feature | Status | Notes |
|-----------|---------|:------:|-------|
| **Core** | Authentication | ✅ | Secure login flow. |
| **Core** | Game Library | ✅ | Search & browse via Cloudmatch integration. |
| **Streaming** | RTP/WebRTC | ✅ | Low-latency streaming implementation. |
| **Streaming** | Hardware Decoding | ✅ | Windows (D3D11), macOS (VideoToolbox), Linux (VAAPI). |
| **Input** | Mouse/Keyboard | ✅ | Raw input capture. |
| **Input** | Gamepad | ✅ | Cross-platform support via `gilrs`. |
| **Input** | Clipboard Paste | 🚧 | Planned. |
| **Audio** | Playback | ✅ | Low-latency audio via `cpal`. |
| **Audio** | Microphone | 🚧 | Planned. |
| **UI** | Overlay | ✅ | In-stream stats & settings (egui). |
| **Media** | Instant Replay | 🚧 | Coming Soon (NVIDIA-like). |
| **Media** | Screenshots | 🚧 | Coming Soon. |
| **Fixes** | iGPU Support | 🚧 | Fixes for Intel/AMD quirks in progress. |

### 🎞️ Supported Codecs & Hardware Acceleration

| Codec | Windows | macOS | Linux | Notes |
|:---:|:---:|:---:|:---:|---|
| **H.264** | ✅ DXVA / NVDEC / QSV | ✅ VideoToolbox | ✅ VAAPI | Standard for most streams. |
| **HEVC (H.265)** | ✅ DXVA / NVDEC / QSV | ✅ VideoToolbox | ✅ VAAPI | High efficiency, lower bandwidth. |
| **AV1** | ✅ NVDEC / QSV | ✅ VideoToolbox (M3+) | ⚠️ VAAPI | Requires RTX 30/40 series or Intel Arc. |
| **Opus (Audio)** | ✅ Software | ✅ Software | ✅ Software | High-quality low-latency audio. |

> **Note:** The client utilizes zero-copy rendering where supported to minimize latency.

### 🚀 Additional Features (Exclusive)
These features are not found in the official client:

| Feature | Status | Description |
|---------|:------:|-------------|
| **Plugin Support** | 🚧 | Add custom scripts to interact with stream controls/input. |
| **Theming** | 🚧 | Full UI customization and community themes. |
| **Multi-account** | 🚧 | Switch between GFN accounts seamlessly. |
| **Anti-AFK** | ✅ | Prevent session timeout (Ctrl+Shift+F10). |

### ⌨️ Controls & Shortcuts

| Shortcut | Action | Description |
|----------|--------|-------------|
| **F11** | Keybind | Toggle Fullscreen |
| **F3** | Keybind | Toggle Stats Overlay |
| **Ctrl+Shift+Q** | Keybind | Force Quit Session |
| **Ctrl+Shift+F10**| Keybind | **Toggle Anti-AFK** (Status shows in console) |

---

## Building

**Requirements:**
- Rust toolchain (1.75+)
- FFmpeg development libraries (v6.1+ recommended)
- `pkg-config`

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

## Troubleshooting

### macOS: "App is damaged"
If macOS blocks the app, run:
```bash
xattr -d com.apple.quarantine /Applications/OpenNOW.app
```

---

## Support the Project

OpenNOW is a passion project developed entirely in my free time. I truly believe in open software and giving users control over their experience.

If you enjoy using the client and want to support its continued development (and keep me caffeinated ☕), please consider becoming a sponsor. Your support helps me dedicate more time to fixing bugs, adding new features, and maintaining the project.

<p align="center">
  <a href="https://github.com/sponsors/zortos293">
    <img src="https://img.shields.io/badge/Sponsor_on_GitHub-EA4AAA?style=for-the-badge&logo=github-sponsors&logoColor=white" alt="Sponsor on GitHub">
  </a>
</p>

---

<p align="center">
  Made by <a href="https://github.com/zortos293">zortos293</a>
</p>
