# OpenNOW Stable (Electron)

This directory contains the active Electron-based OpenNOW client.

For user-facing project information, downloads, and the high-level overview, start with the [main README](../README.md). For local setup and architecture notes, see the [development guide](../docs/development.md).

## Quick Commands

```bash
npm install
npm run dev
npm run typecheck
npm run build
npm run dist
```

## What Lives Here

- `src/main/`: Electron main process, auth, sessions, signaling, caching, media handling
- `src/preload/`: secure renderer bridge
- `src/renderer/src/`: React UI, stream playback, controls, diagnostics, settings
- `src/shared/`: shared types, IPC channels, and utilities

## Packaging Targets

| Platform | Formats |
| --- | --- |
| Windows | NSIS installer, portable executable |
| macOS | `dmg`, `zip` |
| Linux x64 | `AppImage`, `deb` |
| Linux ARM64 | `AppImage`, `deb` |

## Technical Notes

- WebRTC relies on Chromium's built-in stack
- `ws` is used in the main process for custom signaling behavior
- Authentication uses an OAuth PKCE flow with a localhost callback
- Settings are persisted locally through `electron-store`
- React Scan is available in renderer development builds for performance debugging
