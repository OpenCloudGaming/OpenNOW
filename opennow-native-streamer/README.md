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

Raspberry Pi 4 decoder selection now prefers:
- `h264_v4l2m2m` for H.264 when the Pi 4 V4L2 M2M decoder is available
- `hevc_v4l2m2m` for H.265/HEVC when the Pi 4 V4L2 M2M decoder is available
- software decode fallback when the Pi hardware path is unavailable or fails to initialize

On Raspberry Pi 4, AV1 is not advertised as a hardware-decoded path. The native streamer logs that limitation and falls back to software AV1 decode when AV1 is negotiated. Diagnostics and the native overlay report the actual FFmpeg decoder/backend name, whether the path is hardware or software, and the active render/upload path.

## Raspberry Pi 4 / Linux ARM64

The native streamer now includes an explicit Linux ARM64 decoder-selection ladder aimed at Raspberry Pi 4:

- `H264` → prefer FFmpeg `h264_v4l2m2m`
- `H265/HEVC` → prefer FFmpeg `hevc_v4l2m2m` when the Pi runtime exposes it cleanly
- `AV1` → no fake Pi 4 hardware path; log the limitation and fall back to software decode
- if Pi-specific hardware decode cannot initialize, fall back to the next supported path and finally to software decode

Runtime diagnostics, logs, and the native overlay report:
- negotiated codec
- actual FFmpeg decoder/backend name in use
- `hardware` vs `software`
- active render/upload path

Typical Pi 4 paths look like:
- `video path: Raspberry Pi 4 V4L2 M2M H264 hardware decode + SDL YUV/RGBA upload`
- `video path: Raspberry Pi 4 V4L2 M2M HEVC hardware decode + SDL YUV/RGBA upload`
- `video path: software AV1 decode + SDL YUV/RGBA upload fallback`

The current Pi/Linux ARM64 implementation does **not** claim a zero-copy renderer. The Pi-optimized work in this PR is decoder selection/fallback and honest diagnostics; presentation still uses the existing SDL upload path.

### Local build on Raspberry Pi 4 / Linux ARM64

Install practical host/build dependencies first:

```bash
sudo apt-get update
sudo apt-get install -y \
  build-essential \
  cmake \
  ninja-build \
  pkg-config \
  nasm \
  libsdl3-dev \
  libavcodec-dev \
  libavformat-dev \
  libavutil-dev \
  libswresample-dev \
  libswscale-dev
```

`libdatachannel` is expected via `vcpkg` in CI and is the most reliable local path too:

```bash
git clone https://github.com/microsoft/vcpkg.git ~/vcpkg
~/vcpkg/bootstrap-vcpkg.sh -disableMetrics
```

Then configure/build the native streamer:

```bash
cmake \
  -S opennow-native-streamer \
  -B opennow-native-streamer/build \
  -G Ninja \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_TOOLCHAIN_FILE="$HOME/vcpkg/scripts/buildsystems/vcpkg.cmake" \
  -DVCPKG_TARGET_TRIPLET=arm64-linux

cmake --build opennow-native-streamer/build --config Release
```

Resulting binary:

```bash
./opennow-native-streamer/build/opennow-native-streamer
```

### Manual native binary smoke run on Pi / Linux ARM64

The binary is normally launched by Electron main over loopback IPC. For a manual smoke check you can start it directly:

```bash
./opennow-native-streamer/build/opennow-native-streamer \
  --ipc-host=127.0.0.1 \
  --ipc-port=9000 \
  --session-id=pi-smoke-test
```

That only verifies process startup and window/runtime initialization. A real stream still requires the Electron app to create the session, own signaling, and connect to the native helper.

For end-to-end testing with the Electron shell, point OpenNOW at the built binary:

```bash
export OPENNOW_NATIVE_STREAMER_BIN="$PWD/opennow-native-streamer/build/opennow-native-streamer"
```

Then launch the Electron app, enable the `OpenNOW Native Streamer` beta toggle in settings, and start a session.

### GitHub Actions Linux ARM64 artifact

The native streamer CI workflow now builds a Linux ARM64 artifact on a GitHub-hosted ARM runner:

- workflow artifact name: `opennow-native-streamer-linux-arm64`
- contained binary: `opennow-native-streamer`

Practical usage on a Pi 4:

1. Download the `linux-arm64` artifact from the PR or workflow run.
2. Extract it onto the Pi, for example into `~/OpenNOW-native/`.
3. Ensure runtime libraries are installed (`SDL3`, FFmpeg shared libraries, standard C++ runtime).
4. Point OpenNOW at the extracted binary:

```bash
export OPENNOW_NATIVE_STREAMER_BIN="$HOME/OpenNOW-native/opennow-native-streamer"
```

5. Launch OpenNOW and enable the native streamer toggle.

Still intentionally partial in this task:
- Raspberry Pi 4/Linux ARM64 prefers explicit FFmpeg backend ladders (`h264_v4l2m2m`, `hevc_v4l2m2m` when available) but still falls back to software when the runtime cannot sustain the hardware path
- microphone parity is not migrated yet
- recording/screenshot/overlay parity is not migrated yet
- H265/AV1 platform capability handling is structured but will need more real-device validation

If required native dependencies are unavailable, the Electron-side toggle surfaces a clear launch failure without affecting future Chromium fallback launches.
