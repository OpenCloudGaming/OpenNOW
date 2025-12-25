# GFN Client

A custom GeForce NOW client built with Tauri (Rust + TypeScript) that provides enhanced streaming features and a modern UI.

## Download

[![Download](https://img.shields.io/github/v/release/zortos293/GFNClient?include_prereleases&label=Download&style=for-the-badge)](https://github.com/zortos293/GFNClient/releases/latest)

**[Download Latest Release](https://github.com/zortos293/GFNClient/releases/latest)**

### Available Platforms
- **Windows**: `.msi` installer or `.exe` setup
- **macOS**: `.dmg` disk image
- **Linux**: `.deb` package or `.AppImage`

---

## Screenshots

![Home Screen](https://img.youtube.com/vi/bF84_CKopPQ/maxresdefault.jpg)

**[Watch Demo on YouTube](https://www.youtube.com/watch?v=bF84_CKopPQ)**

---

## Features

### Streaming
- **High FPS Modes** - 120fps, 240fps, and 360fps streaming
- **Codec Selection** - H.264 (best compatibility) or AV1 (best quality)
- **Opus Stereo Audio** - Enhanced audio for macOS
- **Unlimited Bitrate** - Configurable from 20 Mbps to unlimited
- **Stable Resolution** - No adaptive quality, consistent streaming
- **Multi-Region Support** - Auto-select lowest ping server with latency display

### Input
- **Raw Mouse Input** - Uses `getCoalescedEvents()` for precise 1:1 movement
- **Native macOS Cursor Capture** - Core Graphics integration for unlimited mouse movement
- **Fullscreen Control** - Hold ESC for 1 second to exit (ESC still works in-game)

### UI
- **Modern Dark Theme** - Clean, minimal interface
- **Custom Dropdowns** - Styled components replacing native OS elements
- **Lucide Icons** - Consistent icon pack throughout the app
- **Latency Color Coding** - Visual ping indicators for regions

### Account
- **NVIDIA OAuth Login** - Secure sign-in with OAuth 2.0 + PKCE
- **Subscription Display** - Shows tier (Free/Priority/Ultimate) and playtime
- **Discord Rich Presence** - Display current game on Discord
- **Persistent Settings** - All preferences saved locally

### Privacy
- **No Telemetry** - NVIDIA telemetry disabled by default
- **Custom Proxy Support** - Route traffic through your own proxy

---

## Tech Stack

| Component | Technology |
|-----------|------------|
| Frontend | TypeScript, Vite |
| Backend | Rust (Tauri 2.x) |
| Streaming | WebRTC + NVST Protocol |
| UI | Custom CSS, Lucide Icons |

---

## Building

### Prerequisites

- [Node.js](https://nodejs.org/) (v18+)
- [Rust](https://rustup.rs/) (latest stable)
- [Tauri CLI](https://tauri.app/)

### Development

```bash
# Clone the repository
git clone https://github.com/zortos293/GFNClient.git
cd GFNClient

# Install dependencies
npm install

# Run in development mode
npm run tauri dev

# Build for production
npm run tauri build
```

---

## Configuration

Settings are stored in the app data directory:
- **Windows**: `%APPDATA%/gfn-client/settings.json`
- **macOS**: `~/Library/Application Support/gfn-client/settings.json`
- **Linux**: `~/.config/gfn-client/settings.json`

| Setting | Options | Default |
|---------|---------|---------|
| Resolution | 720p, 1080p, 1440p, 4K | 1080p |
| Frame Rate | 30, 60, 120, 240, 360 FPS | 60 |
| Codec | H.264, AV1 | H.264 |
| Audio | Opus, Opus Stereo | Opus |
| Max Bitrate | 20-200 Mbps (200 = unlimited) | Unlimited |
| Region | Auto or specific region | Auto |

---

## Contributing

See [CONTRIBUTING.md](.github/CONTRIBUTING.md) for guidelines.

---

## Disclaimer

This is an **independent project** not affiliated with, authorized, or endorsed by NVIDIA Corporation.

- GeForce NOW and NVIDIA are trademarks of NVIDIA Corporation
- Developed for educational and enhancement purposes
- Users are responsible for compliance with applicable terms of service
- No warranty provided; use at your own risk

---

## License

This project is for educational purposes. See NVIDIA's Terms of Service regarding GeForce NOW usage.
