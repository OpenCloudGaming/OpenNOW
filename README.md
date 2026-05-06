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
  <a href="https://testflight.apple.com/join/u1XPJKH2">
    <img src="https://img.shields.io/badge/TestFlight-Beta-blue.svg?style=for-the-badge&logo=apple" alt="TestFlight">
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

OpenNOW is a community-built Electron app for playing GeForce NOW from an open-source desktop client. The active implementation lives in [`opennow-stable/`](opennow-stable), and an iOS SwiftUI prototype lives under [`ios/OpenNOWiOS/`](ios/OpenNOWiOS/).

## Downloads

Grab the latest desktop build from [GitHub Releases](https://github.com/OpenCloudGaming/OpenNOW/releases). The iOS prototype is available through [TestFlight](https://testflight.apple.com/join/u1XPJKH2).

## Documentation

Canonical documentation lives at [opennow.zortos.me](https://opennow.zortos.me):

- [Getting Started](https://opennow.zortos.me/guides/getting-started/)
- [Development](https://opennow.zortos.me/development/)
- [Configuration](https://opennow.zortos.me/reference/configuration/)
- [WebRTC](https://opennow.zortos.me/reference/webrtc/)
- [Native Streamer](https://opennow.zortos.me/reference/native-streamer/)
- [Project Website](https://opennow.zortos.me/)

This repository intentionally does not carry duplicate long-form product, setup, development, native streamer, GStreamer packaging, or release workflow documentation.

## Repository Layout

```text
.
├── opennow-stable/          Active Electron desktop client
├── native/opennow-streamer/ Native Rust streaming infrastructure
├── ios/OpenNOWiOS/          Native iOS SwiftUI app prototype
├── .github/                 Workflows, templates, and contributor metadata
├── AGENTS.md                Repository instructions for AI agents and contributors
├── LICENSE                  Project license
├── logo.png                 Project logo
└── img.png                  App preview image
```

## Contributing

Contributions are welcome. Read the [contributing guide](.github/CONTRIBUTING.md), keep changes focused, and explain user-facing impact clearly.

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
