<h1 align="center">OpenNOW</h1>

<p align="center">
  <img src="logo.png" alt="OpenNOW logo" width="180" />
</p>

<p align="center">
  <strong>An open-source desktop client for GeForce NOW.</strong>
</p>

<p align="center">
  Browse the catalog, tune your stream, and launch sessions from a community-built app.
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
  <a href="LICENSE">
    <img src="https://img.shields.io/github/license/OpenCloudGaming/OpenNOW?style=flat-square" alt="License">
  </a>
</p>

<p align="center">
  <img src="img.png" alt="OpenNOW application preview" />
</p>

> [!WARNING]
> OpenNOW is under active development. Expect occasional bugs, rough edges, and platform-specific issues while the client matures.

> [!IMPORTANT]
> OpenNOW is an independent community project and is not affiliated with, endorsed by, or sponsored by NVIDIA. NVIDIA and GeForce NOW are trademarks of NVIDIA Corporation. You must use your own GeForce NOW account.

## Overview

OpenNOW is a community-built GeForce NOW client. The active implementation lives in [`opennow-stable/`](opennow-stable) and now supports two runtime targets from the same React renderer: Electron for desktop and Capacitor for Android.

The project aims to give players a transparent, customizable alternative to the official client without hiding the technical parts from contributors.

## Highlights

- Open-source desktop client for Windows, macOS, and Linux
- Experimental Android target via Capacitor using the shared React renderer in a WebView
- Catalog and public game browsing with search and library-aware session handling
- Stream controls for codec, resolution, FPS, aspect ratio, region, and quality preferences
- In-stream diagnostics overlay with latency, packet loss, decode, and render stats
- Built-in screenshots, recording, microphone controls, and controller-friendly navigation
- Local settings storage and zero-telemetry project direction

## Quick Start

### Download

Grab the latest build from [GitHub Releases](https://github.com/OpenCloudGaming/OpenNOW/releases).

Current packaging targets:

| Platform | Formats |
| --- | --- |
| Windows | NSIS installer, portable executable |
| macOS | `dmg`, `zip` |
| Linux x64 | `AppImage`, `deb` |
| Linux ARM64 | `AppImage`, `deb` |

For Android testing, GitHub Actions now builds a debug APK artifact for manual workflow runs, plus pull requests and pushes to `main`/`dev` when the `auto-build` path filters match (`opennow-stable/**` or `.github/workflows/auto-build.yml`). Download it from the workflow artifacts; it is intended for testing, not release distribution.

### Develop Locally

From the repository root:

```bash
cd opennow-stable
npm install
cd ..
npm run dev
```

Useful root scripts:

```bash
npm run dev
npm run build
npm run typecheck
npm run dist
```

For a fuller setup guide, see [docs/development.md](docs/development.md).

## Repository Layout

```text
.
├── opennow-stable/   Electron app workspace
├── docs/             Local project documentation
├── .github/          Workflows, templates, contributing docs
├── logo.png          Project logo
└── img.png           App preview image
```

## Documentation

- [Development Guide](docs/development.md)
- [Contributing Guide](.github/CONTRIBUTING.md)
- [Project Website](https://opennow.zortos.me)

## Architecture At A Glance

OpenNOW uses a shared renderer with thin platform adapters:

| Layer | Tech | Responsibility |
| --- | --- | --- |
| Main | Electron + Node.js | Desktop-only OAuth, IPC, local filesystem/media, cache, window management |
| Preload | Electron `contextBridge` | Desktop bridge exposing the existing OpenNOW API surface |
| Renderer | React + TypeScript | Shared login flow, browsing, settings, WebRTC playback, diagnostics, controls |
| Capacitor Android | Capacitor + WebView | Android shell, localhost WebView auth interception, Preferences/filesystem storage, browser-based signaling |

The code lives under [`opennow-stable/src/`](opennow-stable/src), with shared TypeScript types and platform-neutral helpers in [`opennow-stable/src/shared/`](opennow-stable/src/shared). The renderer now consumes [`src/renderer/src/platform/`](opennow-stable/src/renderer/src/platform/) instead of hard-coding `window.openNow`.

## Android Status

The Android target is an initial pass intended to run the core OpenNOW flow inside a Capacitor WebView. Current Android support includes:

- auth session restore
- login via native Android WebView interception while preserving the exact desktop redirect URI contract (`http://localhost:<port>`)
- provider and region loading
- main/library/public game catalog fetches
- session create, poll, claim, and stop
- direct signaling from the WebView
- settings persistence
- screenshots and recordings stored in the app data directory

Known Android limitations in this pass:

- no desktop-style quit action
- no pointer-lock toggle semantics
- no Electron log export or cache deletion flow
- no show-in-folder integration for media
- screenshot export/save-as remains desktop-only
- some desktop shortcut UX is hidden or non-applicable on touch devices

Android auth now mirrors the desktop localhost redirect contract by opening NVIDIA login in a native Android WebView and intercepting the navigation to `http://localhost:<port>` inside the Android shell instead of using deep links or running a real loopback callback server.

CI also produces a testable Android APK artifact from the Capacitor target so reviewers can install and validate the current branch without making a release build.


## Contributing

Contributions are welcome. If you want to help:

1. Read the [contributing guide](.github/CONTRIBUTING.md).
2. Run `npm run typecheck` and `npm run build` before opening a PR.
3. Keep changes focused and explain user-facing impact clearly.

## FAQ

### Is this the official GeForce NOW client?

No. OpenNOW is a third-party, community-maintained client.

### Does OpenNOW collect telemetry?

The current project direction is zero telemetry. Settings and media stay on the local machine, and authentication is performed against NVIDIA services.

### What happened to the earlier Tauri/Rust version?

The active client in this repository is the Electron-based app in [`opennow-stable/`](opennow-stable).

## Star History

<a href="https://www.star-history.com/?repos=OpenCloudGaming%2FOpenNOW&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/image?repos=OpenCloudGaming/OpenNOW&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/image?repos=OpenCloudGaming/OpenNOW&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/image?repos=OpenCloudGaming/OpenNOW&type=date&legend=top-left" />
 </picture>
</a>

## License

OpenNOW is licensed under the [MIT License](LICENSE).
