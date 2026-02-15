<h1 align="center">OpenNOW</h1>

<p align="center">
  <strong>Open source GeForce NOW desktop client built with Electron + TypeScript</strong>
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

---

## About

OpenNOW is an Electron-based GeForce NOW desktop client focused on compatibility and fast iteration across Windows, macOS, and Linux.

- Main app lives in `/Users/zortos/Projects/OpenNOW/opennow-stable`
- Legacy Rust/Tauri stack has been removed from this repository
- CI/CD is unified in `/Users/zortos/Projects/OpenNOW/.github/workflows/auto-build.yml`

## Key Features

| Feature | Status |
|---------|:------:|
| OAuth Login + Session Restore | ✅ |
| Catalog + Library Browsing | ✅ |
| WebRTC Streaming (Chromium) | ✅ |
| Keyboard + Mouse Input Channels | ✅ |
| Adjustable Shortcuts in Settings | ✅ |
| H.264 Codec | ✅ |
| AV1 Codec | ✅ |
| H.265 Codec | 🚧 In progress |

## Platform Targets

| Platform | Output |
|----------|--------|
| Windows | NSIS installer + portable EXE |
| macOS | DMG + ZIP (x64 + arm64) |
| Linux | AppImage + DEB (x64 + arm64) |

## Quick Start

```bash
git clone https://github.com/OpenCloudGaming/OpenNOW.git
cd OpenNOW/opennow-stable
npm install
npm run dev
```

## Build

```bash
cd opennow-stable
npm run build
npm run dist
```

## Release Pipeline

- Push code to `dev`/`main` or open a PR to run build matrix
- Tag format for release publishing: `opennow-stable-vX.Y.Z`
- Workflow uploads packaged artifacts to GitHub Releases

## FAQ

**Was this project built in Rust before? Why switch to Electron?**  
Yes. OpenNOW originally had a Rust-based implementation, but it was replaced to improve compatibility and long-term maintainability.  
Electron gives us a more consistent cross-platform runtime across Windows, macOS, and Linux, which makes shipping and supporting releases on many platforms easier.

## Notes

- `H.265` support is still being worked on and is not considered complete yet.
- This repo no longer uses Rust/Tauri build paths.
