<h1 align="center">OpenNOW</h1>

<p align="center">
  <img src="logo.png" alt="OpenNOW logo" width="180" />
</p>

<p align="center">
  <strong>An open-source GeForce NOW client — play your games, your way.</strong>
</p>

<p align="center">
  <img src="img.png" alt="OpenNOW" />
</p>



<p align="center">
  <a href="https://github.com/OpenCloudGaming/OpenNOW/releases">
    <img src="https://img.shields.io/github/v/tag/OpenCloudGaming/OpenNOW?style=for-the-badge&label=Download&color=brightgreen" alt="Download">
  </a>
  <a href="https://opennow.zortos.me">
    <img src="https://img.shields.io/badge/Docs-opennow.zortos.me-blue?style=for-the-badge" alt="Documentation">
  </a>
  <a href="https://github.com/OpenCloudGaming/OpenNOW/actions/workflows/auto-build.yml">
    <img src="https://img.shields.io/github/actions/workflow/status/OpenCloudGaming/OpenNOW/auto-build.yml?style=for-the-badge&label=Auto%20Build" alt="Auto Build">
  </a>
  <a href="https://discord.gg/8EJYaJcNfD">
    <img src="https://img.shields.io/badge/Discord-Join%20Us-7289da?style=for-the-badge&logo=discord&logoColor=white" alt="Discord">
  </a>
</p>

<p align="center">
  <a href="https://github.com/OpenCloudGaming/OpenNOW/stargazers">
    <img src="https://img.shields.io/github/stars/OpenCloudGaming/OpenNOW?style=flat-square" alt="Stars">
  </a>
  <a href="https://github.com/OpenCloudGaming/OpenNOW/releases">
    <img src="https://img.shields.io/github/downloads/OpenCloudGaming/OpenNOW/total?style=flat-square" alt="Downloads">
  </a>
  <a href="https://github.com/OpenCloudGaming/OpenNOW/blob/dev/LICENSE">
    <img src="https://img.shields.io/github/license/OpenCloudGaming/OpenNOW?style=flat-square" alt="License">
  </a>
</p>

---

> **Warning**  
> OpenNOW is under active development. Bugs and performance issues are expected while features are finalized.

> **Trademark & Affiliation Notice**  
> OpenNOW is an independent community project and is **not affiliated with, endorsed by, or sponsored by NVIDIA Corporation**.  
> **NVIDIA** and **GeForce NOW** are trademarks of NVIDIA Corporation. You must use your own GeForce NOW account.

---

## What is OpenNOW?

OpenNOW is an independent, community-built desktop client for [NVIDIA GeForce NOW](https://www.nvidia.com/en-us/geforce-now/), built with Electron and TypeScript. It provides a fully open-source, cross-platform alternative to the official app with zero telemetry, full transparency, and power-user features.

- 🔓 **Fully open source** — audit every line, fork it, improve it
- 🚫 **No telemetry** — OpenNOW collects nothing
- 🖥️ **Cross-platform** — Windows, macOS, Linux, and ARM64
- ⚡ **Community-driven** — faster fixes, transparent development
- 🎮 **Anti-AFK, Stats Overlay, Adjustable Shortcuts** — power-user features built in

## Gameplay Video (Ball X Pit) (Controller Mode)
[![Watch the video](https://img.youtube.com/vi/N2X7VBKp4jg/maxresdefault.jpg)](https://www.youtube.com/watch?v=N2X7VBKp4jg)

## OpenNOW vs Official GeForce NOW

| Feature | OpenNOW | Official GFN | Notes |
|---------|:-------:|:------------:|-------|
| **Streaming** | | | |
| WebRTC Streaming | ✅ | ✅ | Chromium-based in OpenNOW |
| H.264 Codec | ✅ | ✅ | |
| H.265 / HEVC Codec | ✅ | ✅ | Full support |
| AV1 Codec | ✅ | ✅ | |
| Up to 1080p | ✅ | ✅ | |
| Up to 4K | ✅ | ✅ | Configurable in settings |
| 5K Resolution | ✅ | ✅ | Up to 5K@120fps |
| 120+ FPS | ✅ | ✅ | Configurable: 30/60/120/144/240 |
| HDR Streaming | 📋 | ✅ | 10-bit color supported, full HDR pipeline planned |
| AI-Enhanced Stream Mode | ❌ | ✅ | NVIDIA Cinematic Quality — not available |
| Adjustable Bitrate | ✅ | ✅ | Up to 200 Mbps in OpenNOW |
| Color Quality (8/10-bit, 4:2:0/4:4:4) | ✅ | ✅ | Full chroma/bit-depth control |
| **Input** | | | |
| Keyboard + Mouse | ✅ | ✅ | Full input over GFN data channels |
| Gamepad Support | ✅ | ✅ | Up to 4 controllers simultaneously |
| Flight Controls | ❌ | ✅ | Added in official client v2.0.81 |
| Mouse Sensitivity | ✅ | ❌ | OpenNOW-exclusive setting |
| Clipboard Paste | ✅ | ❌ | Paste text into cloud session |
| **Features** | | | |
| Authentication + Session Restore | ✅ | ✅ | OAuth PKCE, auto-restore on startup |
| Game Library + Catalog | ✅ | ✅ | Main catalog, library, and public games |
| Alliance Partners | ✅ | ✅ | NVIDIA + partner providers |
| Audio Playback | ✅ | ✅ | |
| Microphone Support | ✅ | ✅ | Voice chat with mute/unmute toggle |
| Instant Replay | 📋 | ✅ | Planned for future release |
| Screenshots | 📋 | ✅ | Planned for future release |
| Stats Overlay | ✅ | ✅ | Detailed: RTT, decode, render, jitter, loss, input queue |
| Anti-AFK | ✅ | ❌ | OpenNOW-exclusive — prevents idle disconnects |
| Adjustable Shortcuts | ✅ | 🚧 | Fully customizable in OpenNOW |
| Session Conflict Resolution | ✅ | ✅ | Resume / New / Cancel existing sessions |
| Subscription Info | ✅ | ✅ | Hours, tier, entitled resolutions |
| Region Selection | ✅ | ✅ | Dynamic region discovery |
| Install-to-Play | ✅ | ✅ | For games not in standard catalog |
| Discord Integration | ❌ | ✅ | |
| **Platform Support** | | | |
| Windows | ✅ | ✅ | NSIS installer + portable |
| macOS (x64 + ARM) | ✅ | ✅ | Universal builds |
| Linux | ✅ | 🚧 | Official client has beta native app |
| ARM64 / Raspberry Pi | ✅ | ❌ | OpenNOW builds for ARM64 Linux |
| Steam Deck | 📋 | ✅ | |
| Android / iOS / TV | ❌ | ✅ | Desktop-only for now |
| **Privacy & Openness** | | | |
| Open Source | ✅ | ❌ | MIT licensed |
| No Telemetry | ✅ | ❌ | Zero data collection |
| Auditable Code | ✅ | ❌ | |

> 💡 **Legend:** ✅ Working  ·  🚧 In Progress  ·  📋 Planned  ·  ❌ Not Available

## Roadmap

| Priority | Feature | Status | Description |
|:--------:|---------|:------:|-------------|
| 🔴 | ~~H.265 codec tuning~~ | ✅ Completed | Full HEVC support implemented |
| 🔴 | ~~Microphone support~~ | ✅ Completed | Voice chat with mute/unmute toggle |
| 🟡 | Instant replay | 📋 Planned | Clip and save gameplay moments |
| 🟡 | Screenshots | 📋 Planned | Capture in-stream screenshots |
| 🟡 | HDR streaming pipeline | 📋 Planned | Full HDR end-to-end support |
| 🟢 | Latency optimizations | 🚧 Ongoing | Input and render path improvements |
| 🟢 | Platform stability | 🚧 Ongoing | Cross-platform bug fixes |

> 🔴 High priority · 🟡 Medium priority · 🟢 Ongoing effort

## Features

**Streaming**
`H.264` `AV1` `H.265 (WIP)` · Up to 4K@240fps · Adjustable bitrate · 8/10-bit color · 4:2:0/4:4:4 chroma

**Input**
`Keyboard` `Mouse` `Gamepad ×4` · Mouse sensitivity · Clipboard paste

**Client**
`Stats Overlay` `Anti-AFK` `Adjustable Shortcuts` · OAuth + session restore · Region selection · Alliance partners

**Platforms**
`Windows` `macOS` `Linux` `ARM64` · Installer, portable, AppImage, deb, dmg

## Platform Support

| Platform | Status | Builds |
|----------|:------:|--------|
| Windows | ✅ Working | NSIS installer + portable |
| macOS | ✅ Working | dmg + zip (x64 and arm64) |
| Linux x64 | ✅ Working | AppImage + deb |
| Linux ARM64 | 🚧 Experimental | AppImage + deb (Raspberry Pi 4/5) |

## Quick Start

```bash
git clone https://github.com/OpenCloudGaming/OpenNOW.git
cd OpenNOW/opennow-stable
npm install
npm run dev
```

See [opennow-stable/README.md](./opennow-stable/README.md) for build and packaging details.

## Download

Grab the latest release for your platform:

👉 **[Download from GitHub Releases](https://github.com/OpenCloudGaming/OpenNOW/releases)**

| Platform | File |
|----------|------|
| Windows (installer) | `OpenNOW-v0.2.4-setup-x64.exe` |
| Windows (portable) | `OpenNOW-v0.2.4-portable-x64.exe` |
| macOS (x64) | `OpenNOW-v0.2.4-mac-x64.dmg` |
| macOS (ARM) | `OpenNOW-v0.2.4-mac-arm64.dmg` |
| Linux (x64) | `OpenNOW-v0.2.4-linux-x86_64.AppImage` |
| Linux (ARM64) | `OpenNOW-v0.2.4-linux-arm64.AppImage` |

## Architecture

OpenNOW is an Electron app with three processes:

| Layer | Technology | Role |
|-------|-----------|------|
| **Main** | Node.js + Electron | OAuth, CloudMatch API, WebSocket signaling, settings |
| **Renderer** | React 19 + TypeScript | UI, WebRTC streaming, input encoding, stats |
| **Preload** | Electron contextBridge | Secure IPC between main and renderer |

```
opennow-stable/src/
├── main/           # Electron main process
│   ├── gfn/        # Auth, CloudMatch, signaling, games, subscription
│   ├── index.ts    # Entry point, IPC handlers, window management
│   └── settings.ts # Persistent user settings
├── renderer/src/   # React UI
│   ├── components/ # Login, Home, Library, Settings, StreamView
│   ├── gfn/        # WebRTC client, SDP, input protocol
│   └── App.tsx     # Root component with routing and state
├── shared/         # Shared types and IPC channel definitions
│   ├── gfn.ts      # All TypeScript interfaces
│   └── ipc.ts      # IPC channel constants
└── preload/        # Context bridge (safe API exposure)
```

## FAQ

**Is this the official GeForce NOW client?**
No. OpenNOW is an independent third-party client and is not affiliated with, endorsed by, or sponsored by NVIDIA. NVIDIA and GeForce NOW are trademarks of NVIDIA Corporation.

**Was this project built in Rust before?**
Yes. OpenNOW originally used Rust/Tauri but switched to Electron for better cross-platform compatibility and faster development.

**Does OpenNOW collect any data?**
No. OpenNOW has zero telemetry. Your credentials are stored locally and only sent to NVIDIA's authentication servers.

## Contributing

Contributions are welcome! Open an issue or PR on [GitHub](https://github.com/OpenCloudGaming/OpenNOW).

## Support Me

<p align="center">
  <a href="https://github.com/sponsors/zortos293">
    <img src="https://img.shields.io/badge/Support%20Me-GitHub%20Sponsors-ea4aaa?style=for-the-badge&logo=githubsponsors&logoColor=white" alt="Support Me">
  </a>
</p>

## Star History

<a href="https://www.star-history.com/?repos=OpenCloudGaming%2FOpenNOW&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/image?repos=OpenCloudGaming/OpenNOW&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/image?repos=OpenCloudGaming/OpenNOW&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/image?repos=OpenCloudGaming/OpenNOW&type=date&legend=top-left" />
 </picture>
</a>

## License

[MIT](./LICENSE) © Zortos
