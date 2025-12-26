

<h1 align="center">OpenNOW</h1>

<p align="center">
  <strong>Open source GeForce NOW client built from the ground up</strong>
</p>

<p align="center">
  <a href="https://github.com/zortos293/GFNClient/releases">
    <img src="https://img.shields.io/github/v/tag/zortos293/GFNClient?style=for-the-badge&label=Download" alt="Download">
  </a>
  <a href="https://github.com/zortos293/GFNClient/stargazers">
    <img src="https://img.shields.io/github/stars/zortos293/GFNClient?style=for-the-badge" alt="Stars">
  </a>
  <a href="https://github.com/sponsors/zortos293">
    <img src="https://img.shields.io/badge/Sponsor-EA4AAA?style=for-the-badge&logo=github-sponsors&logoColor=white" alt="Sponsor">
  </a>
</p>

---

## About

OpenNOW is a custom GeForce NOW client created by reverse engineering the official NVIDIA client. Built with Tauri (Rust + TypeScript), it removes artificial limitations and gives you full control over your cloud gaming experience.

**Why OpenNOW?**
- No artificial limitations on FPS, resolution, or bitrate
- Privacy focused - telemetry disabled by default
- Open source and community-driven
- Works on Windows, macOS, and Linux

---

## Screenshot

<p align="center">
  <img src="img.png" alt="OpenNOW Screenshot" width="800">
</p>

---

## Download

<p align="center">
  <a href="https://github.com/zortos293/GFNClient/releases/latest">
    <img src="https://img.shields.io/badge/Windows-0078D6?style=for-the-badge&logo=windows&logoColor=white" alt="Windows">
  </a>
  <a href="https://github.com/zortos293/GFNClient/releases/latest">
    <img src="https://img.shields.io/badge/macOS-000000?style=for-the-badge&logo=apple&logoColor=white" alt="macOS">
  </a>
  <a href="https://github.com/zortos293/GFNClient/releases/latest">
    <img src="https://img.shields.io/badge/Linux-FCC624?style=for-the-badge&logo=linux&logoColor=black" alt="Linux">
  </a>
</p>

---

## Features

### Streaming
| Feature | Description |
|---------|-------------|
| **High FPS Modes** | 60, 120, 240, and 360 FPS streaming |
| **4K & 5K Resolutions** | Up to 5120x2880, ultrawide support (21:9, 32:9) |
| **Video Codecs** | H.264, H.265 (HEVC), and AV1 |
| **Audio Codecs** | Opus mono and stereo |
| **Unlimited Bitrate** | Up to 200 Mbps (no artificial caps) |
| **NVIDIA Reflex** | Low-latency mode for competitive gaming |

### Input & Controls
| Feature | Description |
|---------|-------------|
| **Raw Mouse Input** | 1:1 movement with `pointerrawupdate` events |
| **Unadjusted Movement** | Bypasses OS mouse acceleration |
| **Clipboard Paste** | Paste text directly into games (Ctrl+V) |
| **Full Keyboard Capture** | All keys captured in fullscreen |

### Experience
| Feature | Description |
|---------|-------------|
| **Discord Rich Presence** | Shows current game with optional stats |
| **Multi-Region Support** | Connect to any GFN server region |
| **Privacy Focused** | Telemetry disabled by default |
| **GPU Accelerated** | Hardware video decoding (Windows) |
| **Dark UI** | Modern, clean interface |

---

## Building

```bash
git clone https://github.com/zortos293/GFNClient.git
cd GFNClient
npm install
npm run tauri dev
```

**Requirements:** Node.js 18+, Rust, Tauri CLI

---

## Support the Project

If OpenNOW is useful to you, consider sponsoring to support development:

<p align="center">
  <a href="https://github.com/sponsors/zortos293">
    <img src="https://img.shields.io/badge/Sponsor_on_GitHub-EA4AAA?style=for-the-badge&logo=github-sponsors&logoColor=white" alt="Sponsor on GitHub">
  </a>
</p>


---

## Disclaimer

This is an **independent project** not affiliated with NVIDIA Corporation. Created through reverse engineering for educational purposes. GeForce NOW is a trademark of NVIDIA. Use at your own risk.

---

<p align="center">
  Made by <a href="https://github.com/zortos293">zortos293</a>
</p>
