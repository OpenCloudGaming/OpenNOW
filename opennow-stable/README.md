# OpenNOW Stable (Electron)

This is a new Electron + Chromium desktop baseline for OpenNOW, built to avoid Linux WebRTC/runtime variability from system WebView stacks.

It ports the key protocol shape from `../opennow-streamer` into TypeScript:

- CloudMatch session bootstrap (`/v2/session`)
- NVIDIA signaling over WebSocket (`/nvst/sign_in` + `x-nv-sessionid.*`)
- WebRTC offer/answer + trickle ICE
- `nvstSdp` answer payload generation
- Input data channels (`input_channel_v1`, `input_channel_partially_reliable`)
- Handshake echo + protocol v3 wrapper (`0x22`) support
- OAuth login with PKCE (`login.nvidia.com` + localhost callback)
- Service URL provider selection (NVIDIA + Alliance providers)
- Games catalog and library fetch from `games.geforce.com/graphql`

## Current UX Flow

- Sign in through OAuth in the app (opens browser, returns via localhost callback)
- Pick provider and load main catalog/library/public games
- Cached sessions auto-restore at startup and auto-load MAIN catalog
- Select a game card, pick its variant/store, and launch session
- Tune stream settings in-app: region, zone, resolution, fps, bitrate, codec (H264/H265/AV1)
- Stream in Chromium WebRTC and send keyboard/mouse input over GFN channels

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

- Windows: NSIS + portable
- macOS: dmg + zip
- Linux: AppImage + deb
- Linux ARM64 (Raspberry Pi 4/5 64-bit): supported through electron-builder `linux --arm64`

## CI

GitHub Actions workflow: `.github/workflows/auto-build.yml`

- Windows (NSIS + portable)
- macOS (dmg + zip, x64 + arm64)
- Linux x64 (AppImage + deb)
- Linux arm64 (AppImage + deb)

## Tagged Releases

Release publishing is handled in `.github/workflows/auto-build.yml` on tag pushes.

- Push a tag like `opennow-stable-v0.2.0`
- Workflow builds all platform artifacts
- Workflow creates/updates a GitHub Release and uploads installers automatically

## Notes

- `ws` runs in Electron main process so we can keep custom signaling behavior and relaxed TLS handling where needed.
