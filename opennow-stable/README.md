# OpenNOW Stable (Electron)

> For features, comparison, and downloads, see the [main README](../README.md).

Developer reference for the Electron-based OpenNOW client.

## Development

```bash
npm install
npm run dev
```

## Build

```bash
npm run build
npm run dist
```

## Packaging Targets

| Platform | Formats |
|----------|---------|
| Windows | NSIS installer + portable |
| macOS | dmg + zip (x64 and arm64 universal) |
| Linux x64 | AppImage + deb |
| Linux ARM64 | AppImage + deb (Raspberry Pi 4/5) |

## CI/CD

Workflow: `.github/workflows/auto-build.yml`

- Triggers on pushes to `dev`/`main` and PRs
- Builds: Windows, macOS (x64/arm64), Linux x64, Linux arm64
- Artifacts uploaded to GitHub Releases

## Tagged Releases

Format: `opennow-stable-vX.Y.Z-B` (e.g., `opennow-stable-v0.2.4-4`)

```bash
git tag opennow-stable-v0.2.4-4
git push origin opennow-stable-v0.2.4-4
```

The workflow automatically builds all platforms and creates/updates the GitHub Release.

## Technical Notes

- `ws` runs in the Electron main process for custom signaling behavior and relaxed TLS handling
- WebRTC uses Chromium's built-in stack (no external dependencies)
- OAuth PKCE flow with localhost callback
- Persistent settings stored via `electron-store`
