# Manifest, binaries, and shell integration

## Flatpak identity

From `files/manifest.json`:

- **App id**: `com.nvidia.geforcenow`
- **Command**: `GeForceNOW`
- **Runtime**: `org.freedesktop.Platform` / `24.08`
- **Build roots** (Jenkins): `gfn-flatpack-release-job` → `/app/`

Notable **finish-args**: full device/DRI, network, PulseAudio, X11, Wayland, session/system bus, home filesystem, Gamescope HDR env (`ENABLE_GAMESCOPE=1`, `ENABLE_GAMESCOPE_HDR=1`, `GAMESCOPE_HDR=1`), broad portal/talk names.

## Binary map (under `files/cef/`)

| File | Size | SONAME / role |
| --- | ---: | --- |
| `libGeronimo.so` | 34,347,824 bytes | Stream shell: SDL window, Vulkan/LAVC/VDPAU decode, input, `GridApp`, imports **libBifrost2.so** |
| `libBifrost2.so` | 19,052,024 bytes | NVB / NVST SDK: CloudMatch HTTP, RTSP/WebSocket, Mjolnir, WebRTC bundle, `nvb*` C API |
| `libGsAudioWebRTC.so` | ~1.5 MB | WebRTC audio device module + AEC3; **not** Opus/NetEq |
| `libBifrost2.so` NEEDED | minimal | pthread, dl, atomic, stdc++, m — HTTP/TLS compiled in |
| `GeForceNOW` | 2.4 MB | CEF 128 embedder; loads `libcef.so`, Geronimo, Bifrost |
| `GeForceNOWContainer` | 861 KB | Helper PIE; does not link Geronimo/Bifrost |
| `libcef.so` | ~220 MB | Chromium shell (mall UI) |

**Build IDs** (this payload):

- Bifrost: `fa3685038bd71962fe30ad09482bcb0721a54f35`
- Geronimo: `15d0eebc08da503f1f37ea9cae2dbac1d760fea4`

Embedded toolchain strings: GCC 9/10, OpenSSL **3.5.6**, Mjolnir vcpkg snapshot **2025-04-09**, Chromium **128.4.13** (`128.0.6613.138.nv27`), product path `gfn_release/2f4a4c46`.

## Geronimo dynamic surface

- **Exports**: ~2925 demangled dynamic symbols (stripped, but `.dynsym` populated).
- **No exported `nvb*`** in Geronimo — all session/stream I/O goes through **imports** from Bifrost.
- **Key C++ owners** (from demangled exports): `GridApp`, `IOInterface`, `BifrostSDKExecutor`, `SDLGamepad`, `XInput2InputController`, `VulkanDecoder`, `AsyncFrameQueue`, `FrameTiming`, `GeronimoSettingsImpl`, `WebRTCAudioCapturer` / `SDLAudio`.

## Bifrost exported C API (complete `nm -D` list)

Session and stream:

`nvbCreateClient`, `nvbDestroyClient`, `nvbInitializeClient`, `nvbRegisterCallback`, `nvbSetAuthInfo`, `nvbSetHttpClient`, `nvbGetServerInfo`, `nvbStartSession`, `nvbJoinSession`, `nvbGetSession`, `nvbGetActiveSessions`, `nvbGetActiveSessionsTraced`, `nvbStartStreaming`, `nvbStopStreaming`, `nvbPauseSession`, `nvbPauseStream`, `nvbResumeSession`, `nvbStopSession`, `nvbCancelRequest`.

Runtime control:

`nvbSendInputEvent`, `nvbSendMicAudioFrame`, `nvbSendStreamStats`, `nvbSendMessage`, `nvbSendClientInfo`, `nvbFeatureControl`, `nvbConfigureAudioChannelCount`, `nvbUpdateVideoFrameState`, `nvbUpdateVideoDecoderState`, `nvbUpdateDJBState`, `nvbUpdateAdState`, `nvbCollectStatistics`, `nvbSetAvailableInputDevices`, `nvbTestNetworkLatency`, `nvbTestNetworkLatencyAsync`, `nvbTestNetworkCapability`, `nvbTestNetworkCapabilityAsync`, `nvbTestNetworkAsyncCancel`, `nvbFreeMemory`, `nvbEnumToString`, `registerTracingCallbacks`.

C++ helpers: `nvbutil::WebRequest`, `nvbutil::Network`.

## CEF shell switches (`Resources/GeForceNOW.json`)

Active switches include:

`nv-gfn-streamer=true`, `nv-ipc-type=mallclient`, `nv-url-relative=../mall/index.html`, `nv-streamer-url-relative=../mall/index.html`, `nv-plugin-folder-relative=plugins/BackgroundProcess;plugins/Base;plugins/GeForceNOW`, `nv-gfn-async-renderer=true`, `nv-use-angle-gl-egl`, `nv-sdl-vsync=true`, `nv-sdl-hidpi=true`, `nv-renderer-blocklist=llvmpipe,softpipe,software`, `nv-shared-storage-name=GeForceNOW`.

These bind the **mall** (Angular) to native streaming: the page issues **`QUERY_GFN_*`** CEF queries; the shell forwards into **`GridApp`** → **`SessionController`** → **`nvb*`**.


## Mall config entry points

Primary product config: `files/mall/shared/assets/config/config.json`

- **Build**: `2.0.84.127`
- **CloudMatch grid**: `https://prod.cloudmatchbeta.nvidiagrid.net/` (`grid.version`: `v2`)
- **Streamer block**: host `prod.cloudmatchbeta.nvidiagrid.net`, port `443`, `useSerenity: true`, frame-loss timeouts, reconnect 300s / refresh 5s
- **Client**: `userAgent: GFN-PC`, `clientStreamerClassic: true`, streaming profiles, dynamic resolution, HDR flags
- **Remote config**: `https://rconfig.nvidiagrid.net/v2` branch `ebeta`; GXT remote overrides can patch `NVbSessionParams` via `GeronimoSettingsImpl::overrideNVbSessionParams`

Shell-only logging config: `files/cef/Resources/config/product-config.json` (file logger, 50 MB rotation).

## Plugins and dependencies

Under `files/cef/plugins/` and `dependencies/`:

- `libNetworkTest.so` / `libNetworkTestSDK.so` — network test before seat
- `libnvmessagebus.so`, `libgfnspfbc.so`, `libbackgroundagent.so`
- Message bus router, share server, system info

## OpenNOW correspondence

| Official | OpenNOW |
| --- | --- |
| Mall + CEF queries | Qt QML shell + `CoreClient` JSON protocol |
| `nvbStartSession` / GridServer POST | `CloudMatchService` `session.create` |
| `nvbStartStreaming` + connection list | `streamer.prepare` / `streamer.start` + in-process NVST |
| `libGeronimo.so` + Bifrost | `opennow-streamer` + `opennow-core` (no NVIDIA `.so` loaded) |

See [session-creation-cloudmatch.md](session-creation-cloudmatch.md) for the full orchestration graph and parity gaps.
