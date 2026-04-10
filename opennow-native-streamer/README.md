# OpenNOW Native Streamer

`OpenNOW Native Streamer` is a separate Go process for the OpenNOW Electron app. It owns the native streaming window, the WebRTC endpoint, input encoding, and the media pipeline abstraction while Electron main continues to own CloudMatch session lifecycle and the NVST signaling WebSocket.

## Why Go + GStreamer

- Go keeps the control plane, IPC, and process lifecycle readable.
- GStreamer is the long-term media/rendering backbone for cross-platform decode, audio output, and native-window presentation.
- The Electron app keeps the Chromium/WebRTC path intact as the fallback when the native toggle is disabled.
- Linux ARM and Raspberry Pi remain explicit targets through platform probing and isolated media/platform modules instead of desktop-only assumptions.

## Ownership split

- Electron main
  - session create/poll/claim/stop
  - signaling WebSocket
  - spawning and supervising the native process
  - forwarding offer/ICE and relaying answer/ICE back to signaling
- Native process
  - native window lifecycle
  - Pion `PeerConnection` and `DataChannel` endpoint
  - protocol-sensitive SDP munging and NVST SDP generation
  - GFN input packet encoding
  - media sink abstraction for GStreamer-backed playback

## IPC protocol

Electron main and the native process communicate over a versioned local socket using JSON lines.

Message classes:

- `hello` / `hello-ack`
- `start-session`
- `signaling-offer`
- `remote-ice`
- `local-answer`
- `local-ice`
- `input`
- `request-keyframe`
- `stats`
- `state`
- `error`
- `stop`

## Build

Prerequisites for the real media backend:

- Go 1.25+
- GStreamer 1.22+
- SDL2 development libraries

Development / CI can build the control-plane foundation without native media dependencies:

```bash
go build ./...
```

To build the native window + GStreamer backend:

```bash
go build -tags gstreamer ./cmd/opennow-native-streamer
```


## GitHub Actions artifacts

A dedicated GitHub Actions workflow builds downloadable native-streamer binaries with the `gstreamer` build tag:

- workflow: `.github/workflows/native-streamer-build.yml`
- triggers: `workflow_dispatch`, matching `pull_request`, and matching pushes to `dev` / `main`
- artifacts: Windows x64, macOS x64, macOS arm64, Linux x64

From the Actions tab, open the `native-streamer-build` workflow run and download the artifact for your target platform.

- macOS/Linux artifacts contain the built `opennow-native-streamer` binary for that runner/architecture.
- Windows artifacts contain a runnable folder layout, not just the EXE:
  - `opennow-native-streamer.exe`
  - bundled MinGW/UCRT runtime DLLs
  - bundled SDL2 and GStreamer runtime DLLs
  - `gstreamer-plugins/` with the matching plugin set from CI
  - `libexec/gstreamer-1.0/gst-plugin-scanner.exe`

For Windows, keep the extracted folder contents together. OpenNOW should launch the EXE from inside that folder so the bundled DLLs and GStreamer plugins remain discoverable.

Current CI caveats:

- Linux arm64 / Raspberry Pi is intentionally not greenwashed in CI yet. The codebase treats it as a real target, but the workflow leaves it disabled until runner and dependency provisioning are reproducible.
- The workflow installs native GStreamer / SDL2 development dependencies per platform before building.
- Windows builds use MSYS2 UCRT64 packages for the CGO compiler, `pkg-config`, GStreamer, SDL2, and the GStreamer plugin sets required for the currently validated shipped RTP media path, and the workflow bundles the matching runtime DLLs and plugin tree into the artifact so end users do not need a separate MSYS2 installation.
- A separately installed official GStreamer MSVC runtime does not satisfy the MSYS2/UCRT build produced by CI. Use the bundled Windows artifact contents as-is.
- The current packaged Windows runtime is validated for the H.264 + Opus path. AV1 RTP depayload support is not asserted in CI on Windows and should be treated as deferred until the MSYS2 runtime ships a reproducible `rtpav1depay` path.

## Platform notes

- Windows: intended decoder path is Media Foundation / D3D11-backed GStreamer plugins when available. The CI artifact is packaged as a self-contained folder with the matching MSYS2/UCRT runtime and GStreamer plugin set. Windows packaged artifacts currently validate the H.264 + Opus native path; AV1 remains an explicit follow-up.
- macOS: intended decoder path is VideoToolbox-backed GStreamer plugins.
- Linux x64: intended decoder path is VA-API/NVDEC depending on host.
- Linux ARM / Raspberry Pi: keep decoder selection capability-based. V4L2, VA-API, and software fallback must remain valid options.

## Current scope

Implemented foundation in this change:

- versioned main↔native IPC
- native process lifecycle and separate window abstraction
- protocol-sensitive SDP and NVST SDP helpers ported to Go
- Pion-based WebRTC endpoint with input/control channels
- GFN-compatible mouse, keyboard, and controller packet encoding foundation
- Electron integration behind a settings toggle

Deferred for follow-up:

- microphone parity beyond scaffolding
- screenshot / recording migration
- HDR / 10-bit output polish
- overlay parity with the browser path
- full production GStreamer decode path validation on each target platform
