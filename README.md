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

## Features

### ✅ Working
Based on the current v0.2.0 Native Rust codebase:
- **Authentication**: Secure login flow.
- **Game Library**: Search and browse your GFN library (Cloudmatch integration).
- **Streaming**:
    - Low-latency RTP/WebRTC streaming.
    - **Hardware Decoding**:
        - Windows (D3D11/DXGI).
        - macOS (VideoToolbox).
        - Linux (FFmpeg/VAAPI where supported).
- **Input**:
    - Raw Mouse & Keyboard input.
    - Gamepad support (via `gilrs`).
- **Audio**: Low-latency audio playback (`cpal`).
- **Overlay**: In-stream stats and settings overlay (`egui`).

### 🚧 To-Do / In Progress
- [ ] **Multi-account support**
- [ ] **Fix IGPU specific issues** (Intel/AMD integrated graphics quirks)
- [ ] **Clipboard Paste** support
- [ ] **Microphone** support

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
