# OpenNOW Native Streamer

`OpenNOW Native Streamer` is the first-class native streaming backend for OpenNOW. It is designed as a separate process and native window that the Electron shell can launch behind a settings toggle.

This project uses:
- `C++20`
- `CMake`
- `SDL3` for windowing, audio device abstraction, and raw mouse/keyboard/controller input
- `libdatachannel` for WebRTC/DTLS/SRTP/DataChannel ownership
- `FFmpeg` for decode/audio pipeline scaffolding and hardware-acceleration probing

## Why this stack

The Electron renderer path remains the fastest way to keep the current app working, but it couples playback to Chromium. The native streamer gives OpenNOW a path toward:
- lower-overhead decode/render on Windows, macOS, Linux, and Linux ARM
- a dedicated native window instead of an embedded Chromium view
- direct SDL input capture for mouse, keyboard, and controller packets
- maintainable separation between CloudMatch/signaling orchestration and the native playback/runtime stack

## Electron/native split

Electron main process responsibilities stay in `opennow-stable/`:
- auth/session lifecycle
- CloudMatch create/poll/claim/stop
- signaling transport (`GfnSignalingClient`)
- settings persistence and UI
- launcher shell and fallback Chromium streamer

Native process responsibilities in this project:
- separate SDL window titled `OpenNOW Native Streamer`
- local IPC handshake with Electron main
- SDP/NVST helper ownership (`fixServerIp`, `extractPublicIp`, `preferCodec`, `mungeAnswerSdp`, `buildNvstSdp`, partial-reliability parsing)
- libdatachannel peer connection ownership for offer/answer, local ICE, remote ICE, and data channels
- FFmpeg decode ownership for received video/audio frames
- SDL render/audio output plus direct mouse/keyboard/controller capture

## IPC contract

Electron main and the native process communicate over a versioned length-prefixed JSON protocol on local loopback TCP.

Message framing:
- `u32` big-endian payload length
- UTF-8 JSON body

Protocol version:
- `1`

Core message types already reserved and wired:
- `hello`
- `session-config`
- `session-config-ack`
- `signaling-connected`
- `signaling-offer`
- `signaling-remote-ice`
- `answer`
- `local-ice`
- `disconnect`
- `state`
- `log`

This is intentionally socket-based instead of stdio so the transport can evolve without coupling lifecycle control to process pipes.

## Build

Dependencies are intentionally discovered through CMake instead of being hidden behind Electron.

Typical local build flow:

```bash
cmake -S opennow-native-streamer -B opennow-native-streamer/build
cmake --build opennow-native-streamer/build
```

If the required development libraries are missing, CMake will configure the project but print warnings for the unavailable subsystems.

## Current MVP status

This project now owns a minimum viable native stream path when `SDL3`, `FFmpeg`, and `libdatachannel` are present:
- Electron main launches a separate native process/window and proxies GFN signaling over versioned local IPC
- libdatachannel owns the native peer connection, answer generation, local ICE trickle, remote ICE ingestion, and negotiated data channels
- manual ICE candidate injection from `mediaConnectionInfo` is applied for GFN ICE-lite sessions
- FFmpeg decodes received video/audio frames and SDL3 renders video plus plays decoded PCM audio
- SDL3 mouse, keyboard, wheel, and controller input is encoded with the same packet framing semantics as the Chromium `inputProtocol.ts` path
- the Chromium/WebRTC renderer backend remains the default fallback when the setting is off

macOS presentation paths now prefer:
- `VideoToolbox + Metal/CVPixelBuffer direct presentation`: native macOS path, where VideoToolbox-decoded `CVPixelBuffer` surfaces are retained, exposed as `CVMetalTexture`, and presented through `CAMetalLayer`
- `VideoToolbox hardware decode + SDL YUV GPU upload`: fallback hardware path, where decoded frames are transferred to CPU-visible planes and uploaded through SDL YUV textures
- `software decode + SDL YUV/RGBA upload fallback`: fallback software path, where FFmpeg decodes to CPU-visible frames and the renderer uploads YUV or RGBA textures through SDL

The macOS direct path removes the previous mandatory `av_hwframe_transfer_data(...) -> CPU plane staging -> SDL_UpdateNVTexture/SDL_UpdateYUVTexture(...)` hot path for VideoToolbox-backed frames. Remaining copies in the preferred path are limited to the native surface/Metal presentation plumbing needed to bind `CVPixelBuffer` planes as Metal textures for final draw. If native-surface presentation fails at runtime, the fallback to the SDL upload path is sticky and immediately reflected in diagnostics and overlay path reporting.

Still intentionally partial in this task:
- decoder/hwaccel selection is conservative and software-first
- microphone parity is not migrated yet
- recording/screenshot/overlay parity is not migrated yet
- H265/AV1 platform capability handling is structured but will need more real-device validation

If required native dependencies are unavailable, the Electron-side toggle surfaces a clear launch failure without affecting future Chromium fallback launches.
