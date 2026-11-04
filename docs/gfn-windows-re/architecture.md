# GFN Windows client: process and IPC architecture

Client version 2.0.89.141, Geronimo and Bifrost `gs_04_92 38967594`. This is a static study of the supplied install subset, the later Bifrost upload, and the user's logs. No GFN binary was run and no NVIDIA server was contacted. Windows network functions are now traced directly; log evidence remains separately tagged.

Evidence tags: **S** static (string, import, export, data), **D** decompiled (Ghidra 12.1.4; image base `0x140000000` for EXEs, `0x180000000` for DLLs), **L** log-confirmed, **I** inferred, **U** unknown. Log times are local wall-clock (the second bracket in `geronimo.log`; the timestamp in `debug.log`).

## Summary

The streaming client is not a separate process. `GeForceNOW.exe` is the CEF shell and also hosts Geronimo, Bifrost, and SDL2 in-process. The JS app drives Geronimo through CEF query names (`QUERY_GFN_PREPARE`, `QUERY_GFN_START`, ...) that `GFNQueryHandler` maps to direct `GridApp` method calls. `GeForceNOWStreamer.exe` is a 500 KB relauncher for `GeForceNOW.exe`. `GeForceNOWContainer.exe` is an NvContainer host for four plugins that communicate with the shell over the NVIDIA MessageBus (named pipe). Those plugins observe streaming state; they do not carry media or input.

This matches the macOS shape (`GFNQueryHandler::OnQueryNative` making named `GridApp::prepare/setAuthInfo/start/resume` calls; tiny streamer stub; separate agent) with one difference worth noting: on macOS the stub was inferred to be a launcher. On Windows it is now decompiled and confirmed as a one-argument relauncher of the main executable.

## Process and module map

| Process / module | Owns | Evidence |
| --- | --- | --- |
| `GeForceNOW.exe` (CEF shell, NVIDIACEFClient 128.6613.138.34) | Browser process, JS app host, CEF plugin manager, `GFNQueryHandler`, `SimpleGridApp` (a `GridApp` subclass), `BackgroundAgentIpc`, `HeartbeatSender`, `GeronimoLogger` (writes `geronimo.log`), offscreen-rendered (OSR) overlay, self-update helper | **S** delay-imports 214 Geronimo symbols, 17 SDL2 symbols, and 2 Bifrost symbols (`nvbTestNetworkLatencyAsync`, `nvbTestNetworkAsyncCancel`). **S** strings `Creating GFNQueryHandler` `0x1402a14c8`, `Creating SimpleGridApp` `0x14029e840`, `Creating BackgroundAgentIpc` `0x14029d7d0`, `Creating HeartbeatSender` `0x14029da60`. **L** `debug.log` 18:31:47.993 `browser_app.cc(587)] Browser app is initializing Geronimo` in PID 13432 |
| `Geronimo.dll` (29 MB, 2781 exports), in-process in `GeForceNOW.exe` | `GridApp` session facade, `BifrostSDKExecutor` (the only caller of `nvb*`), `IOInterface`/`GIOInterface`, `SDLWindow`/`SDLWindowManager`/`SDLGraphicsContext` (D3D11/DXVA), `SDLEventProcessor`, `RawInputController`, `SDLGamepad`, `RIDeviceMediator`, `HIDDevicesController`, `SDLCursorManager`, `WASAPIAudioRenderer`, `WebRTCAudioCapturer`, recording, ImGui stats overlay | **S** exports and RTTI. **S** imports 31 `nvb*` from `Bifrost2.dll`, 124 SDL2 functions, and 10 `gsAudioWebRTC_*` |
| `Bifrost2.dll` | Session provisioning, RTSP/SDP, NVST transport, media queues and feedback | **D/L** See [handshake.md](handshake.md), [transport-feedback.md](transport-feedback.md), [packet-layouts.md](packet-layouts.md) |
| `SDL2.dll` (not supplied) | Window, events, game controllers, clipboard primitives, cursors | **S** imports from both `GeForceNOW.exe` and `Geronimo.dll` |
| `GsAudioWebRTC.dll` (15 exports) | Microphone capture through the WebRTC Windows Core Audio device module, plus WebRTC APM (AEC) | **S** exports. **D** wrappers at `0x1800011f0`-`0x1800013b0`. See audio.md |
| `NvPixels.dll` (exports `GetObj`, `DeleteObj`) | CUDA-based pixel processing (imports 29 `cu*` functions including external-memory/semaphore interop) | **S** imports. **L** `[NvPixels]` tag (4 lines). Role in the video path: DeepISP upscaler, see [video.md](video.md) |
| `GeForceNOWStreamer.exe` (500 KB) | Relauncher: starts `GeForceNOW.exe` with one forwarded argument | **D** see below |
| `GeForceNOWContainer.exe` (NvContainer v1.51) | Plugin host process | **L** `GeForceNOWContainer.log` `Container main v1.51`, "Dynamic loading of plugins is disabled" |
| `NvMessageBusBroadcast.dll` (container plugin) | MessageBus broadcaster: named-pipe server `\\.\pipe\NvMessageBusBroadcast` (+ `LocalConnectionSuffix` from `messagebus.conf`) | **S** pipe string; imports `ConnectNamedPipe`. **L** `MessageBusBroadcast.log` "PipeServer ... ConnectNamedPipe - connection pending" |
| `GeForceNOWReliabilityMonitor.dll` (container plugin "GfnBackgroundAgent") | Bus name `CrimsonNative:BackgroundAgent`: self-updater (OTA check), log sender, telemetry, user consent, single-instance moderator, streamer diagnostics that relay streaming events | **L** `gfn_reliability_monitor.log` `gfn_streamer_diagnostics.cpp ... Relaying streaming event 'IPC_STREAMING_STARTED_EVENT'`, `gfn_self_updater.cpp ... Query for available updates` |
| `GameStreamClientAgent.dll` (container plugin) | Bus name `GameStream:ClientAgent`: `StreamAgent` that runs traceroute to the zone, Wi-Fi diagnostics (`WifiOptimization`), network connectivity, and GsTelemetry. It statically links `GeronimoSettings`/`GeronimoSettingsImpl` (165 exports) so it can read the same remote config | **S** RTTI `MessageBusIPC@GameStream`, `WifiOptimization`, exports `?GetInstance@GeronimoSettingsImpl`. **L** `gameStreamClientAgent.log` "Traceroute: tracert /h ...", "WlanOpenHandle failed" |
| `NvTelemetry64.dll` (container plugin "NvTelemetryPlugin") | Telemetry SDK | **L** `GeForceNOWContainer.log` "Loaded mandatory plugin NvTelemetryPlugin" |

**L** The container loads all four plugins as mandatory (`GeForceNOWContainer.log`: `Loading ...\nvc\localuser/GFN/GameStreamClientAgent.dll`, `Loaded mandatory plugin GfnBackgroundAgent`, ...). PID mapping from the 2026-10-12 run: `GeForceNOW.exe` is PID 13432 (`debug.log` prefix `[13432:...]` and `MessageBus_13432.log`), and the container that hosts both GFN plugins is PID 29224 (`MessageBusBroadcast.log`: `Accepting join from: GameStream:ClientAgent ... pid: 29224` and `Accepting join from: CrimsonNative:BackgroundAgent ... pid: 29224`).

## GeForceNOWStreamer.exe is a relauncher

**D, `FUN_140009eb0` (main body).**
1. It opens `proxy_app_logs.txt` (string `0x14002e908`) and logs `Start time:` plus each argv element.
2. It calls `GetCommandLineW` and `CommandLineToArgvW`. If argc is not exactly 2, it logs `There is a problem in passing the arguments to this application (argument count shoud be 2).` (`0x14002e970`) and exits.
3. It takes the directory of `argv[0]` (`FUN_1400069e0` does a find-last-of path separator), appends `GeForceNOW.exe` (`0x14002e9d0`), and builds the command line `"<dir>\GeForceNOW.exe" <argv[1]>` using the quote literals at `0x14002e6d8` and `0x14002e9f0`.
4. It calls `CreateProcessW(NULL, cmdline, ...)` and logs `Create Process failed with <err>` on failure.

**S** Its imports are only `KERNEL32` (including `CreateProcessW`) and `SHELL32!CommandLineToArgvW`. It does not load Geronimo, SDL, or Bifrost. **I** It exists so shortcuts, deep links, and the self-updater can start a stream through a stable executable name. `GeForceNOW.exe` contains the self-update relaunch arguments ` -skipMallLaunch:true -launchStreamer:true -streamerArgs:"--url-route=` (`0x140294050`), a ` --url-route="#?` builder (`0x14028f1b0`), and named single-instance events `Force Streamer shutdown event`, `URL Route event`, `Foreground event` near the string `GeForceNOWStreamer` (`0x140292e90`-`0x140292f88`). **U** The exact argument format a shortcut passes was not traced.

The stub does not load Geronimo. `nv-gfn-streamer` is a `GeForceNOW.exe` command-line switch (**S** `0x14028b920`, read by `FUN_1400319e0` and `FUN_140082c50`), together with `nv-gfn-accelerator`, `nv-gfn-stats`, `nv-gfn-encrypt-flags`, `nv-gfn-async-renderer`, and `nv-sdl-fullscreen-exclusive`, `nv-sdl-offscreen`, `nv-sdl-vsync-adaptive`, `nv-sdl-force-windowed`, `nv-sdl-resizable`.

## Geronimo is loaded in-process by the shell

- **S** `Geronimo.dll` is a delay-load import of `GeForceNOW.exe`. The imported classes are: `GridApp` (69 methods), `SDLWindow` (56), `SDLWindowManager` (8), `SDLGraphicsContext` (9), `SDLEventProcessor` (4), `SDLGamepad` (8), `Gamepad` (9), `AudioCapturer::registerMicStateHandler`, `FrameRenderer`, `ImageWriter`, `Nsk::*`, `TracingDispatcher`, `GeronimoSettings::GetInstance`, `AppLogger`, `GeronimoOverlayHandle`/`GeronimoOverlayLayer`, and free functions such as `platformStartup`, `platformCreateAudioRenderer`, `platformCreateAudioCapturer`, `platformCreateVideoDecoder`, `platformChooseStreamingSettings`, `platformKeyboardLayoutToGenericFormat`, `platformGetMaxBitRate`.
- **S** `GeForceNOW.exe` init function `FUN_14002ca70` holds the Geronimo bring-up log strings in order: `Geronimo Logger Init`, `Initializing Geronimo telemetry`, `Geronimo SDL Init`, `Geronimo platform startup failed!`, `Geronimo Tracing Init`, `Geronimo GRID Init`.
- **L** `debug.log` 18:31:47.993 `Browser app is initializing Geronimo`, then 18:31:48.206 `Initializing Geronimo telemetry`, both in PID 13432.
- **I** The in-stream UI is CEF offscreen rendering composited into Geronimo's SDL window. `GeForceNOW.exe` imports `SDLWindow::setOverlayLayers` and constructs `GeronimoOverlayLayer(SDL_Rect, SDL_Rect, shared_ptr<GeronimoOverlayHandle>)` from an `ID3D11Texture2D` plus `IDXGIKeyedMutex`. **L** `debug.log` 19:45:30.883 `osr_simple_handler_win.cpp ... SetWindowFullScreenNative 1` and `offscreen_window.cpp ... IGO overlay zoom level`.

## CEF query surface (JS app to native)

**S** `GeForceNOW.exe` registers 153 `QUERY_*` names. The streaming ones (`QUERY_GFN_*`) are:

`PREPARE, START, RESUME, STOP, CANCEL, CLEANUP, PAUSE_STREAMING, SET_AUTH_INFO, SET_AUTH_TOKEN, GET_ACTIVE_SESSIONS, GET_SESSION_INFO, REGISTER_CALLBACK, GET_STREAM_STATS, TOGGLE_INTERNAL_STATS, SET_HUD_STATE, SET_STREAMING_MAX_BITRATE, SET_STREAMING_L4S_STATE, SET_DYNAMIC_STREAMING_MODE_STATE, SET_VSYNC_ENABLED, SET_ADAPTIVE_FRAME_PACING_ENABLED, SET_MAG_FILTER, SET_MAG_FILTER_SHARPNESS, SET_PRE_FILTER_STATE, GET_MAG_FILTER_SUPPORT(_LIST), GET_DISPLAY_MODE, PREFETCH_DEVICE_CAPABILITIES, GET_SERIALIZED_DEVICE_CAPABILITIES, GET_PLATFORM_CLOUD_GSYNC_CAPABILITIES, IS_PLATFORM_SUPPORTS_HDR_STREAMING, IS_PLATFORM_SUPPORT_FPS, PLATFORM_GET_MAXBITRATE_ARRAY, CALCULATE_DATA_USAGE, LATENCY_BASED_ROUTING, SET_KEYBOARD_LAYOUT, GET_KEYBOARD_LAYOUT, REGISTER_ON_KEYBOARD_LAYOUT_CHANGED, REGISTER_ON_IME_WINDOW_OPENED/CLOSED, SEND_UNICODE, SEND_CLIPBOARD_UPDATE, GET_CONNECTED_GAMEPADS, REGISTER_GAMEPAD_CHANGE_CALLBACK, REGISTER_GAMEPAD_EVENT_CALLBACK, TOGGLE_JOYSTICK_MOUSE, AUTH_AV_DEVICES, GET_MIC_STATE, SET_MIC_STATE, REGISTER_MIC_CALLBACK, GET_STREAM_VOLUME, SET_STREAM_VOLUME, REGISTER_HDR_STATUS, UPDATE_AD_STATE, UPDATE_EVENT_INFO_GUIDS, UPDATE_REMOTE_CONFIG, UPDATE_STREAMING_SETTINGS, EULA_ACCEPTED, LOGIN_STATUS, GET_UNIFIED_API_VERSION`.

Non-GFN queries that matter for streaming: `QUERY_SET_SUPPORTED_HID_TYPES`, `QUERY_GET_GAMEPAD_BITMAP`, `QUERY_SHOULD_DEFAULT_ENABLE_MIC`, `QUERY_WIN_COPY_TO_CLIPBOARD`, `QUERY_WIN_KB_MESSAGE`, `QUERY_SYNTHESIZE_CLICK_EVENT`, `QUERY_OSR_*`/`QUERY_OSC_*` (overlay windows, `QUERY_OSC_SET_CURSOR_INFO`), `QUERY_IPC_*` (plugin message push/pop), `QUERY_STREAMER_INSTALL`.

**D, `GFNQueryHandler` dispatcher `FUN_1401a69c0`.** The function is too large for Ghidra's default decompiler payload, so the call map was produced with a call/string listing script. The dispatcher makes direct imported calls to `GridApp::prepare` (`0x1401a7fb5`), `GridApp::setAuthInfo` (`0x1401a8624`), `platformKeyboardLayoutToGenericFormat` (`0x1401a8f35`), `GridApp::resume` (`0x1401aaee1`, `0x1401ab4a4`), `GridApp::start` (`0x1401aaf59`), `GridApp::stop` (`0x1401ab71f`), `GridApp::cancel` (`0x1401ab95e`), `GridApp::getSessionInfo` (`0x1401ac9a6`), `GridApp::pauseStreaming` (`0x1401ae406`), `GridApp::setKeyboardLayout` (`0x1401aea38`), `GridApp::buildGamepadBitmap` (`0x1401b3406`), `GridApp::setGamepadRsdmm` (`0x1401b3905`), and also `setVsyncEnabled`, `setAdaptiveFramePacingEnabled`, `setStreamingMaxBitrate`, `setL4sState`, `setDynamicStreamingMode`, `setHudStreamingParams`, `controlStats`, `updateAdState`, `getActiveSessions`. The JSON fields it reads around the prepare/start calls are `deviceId`, `clientHeaders`, `serverAddress`, `keyboardLayout`, `allowKeyboardLayoutChange`, `metaData`, `supportedControls`, `spanData`, `streamingProfile`, `partnerCustomData`, `streamingProfileGuid`, `clipboardInfo`, `kbLayout`, `unicodeInput`. This is the same field set the macOS analysis found at `GFNQueryHandler::OnQueryNative`.

## MessageBus IPC (shell to container plugins)

**S/L** The participants and their roles:

| Bus identity | Process | Role |
| --- | --- | --- |
| `CrimsonNative:MessageBusRouter` | `GeForceNOW.exe` (CEF plugin `MessageBusRouter`) | Router for the shell. **L** `CxNative_GeForceNOW.log` `Using default config because MessageBusRouter.conf doesn't exist`; `MessageBus_13432.log` `CrimsonNative:MessageBusRouter joined the Broadcaster` |
| `CrimsonNative:BackgroundAgent` | container (`GeForceNOWReliabilityMonitor.dll`) | Receives shell events, relays streaming events onward, runs updater and log sender |
| `GameStream:ClientAgent` | container (`GameStreamClientAgent.dll`) | Network diagnostics during streaming |

**L** Transport: named pipes through the broadcaster (`MessageBusBroadcast.log` `New client connected on <conn>, processId: 13432`; protocol version 3; non-elevated; same session). `messagebus.conf` sets `MessageBusPort: 1`, `PerSessionInstance: false`, `LocalConnectionSuffix: GeForceNOW_<number>`.

**S** Event names sent by `GeForceNOW.exe` (`FUN_14016b350`): `IPC_STREAMING_PREPARE_EVENT`, `IPC_STREAMING_STARTING_EVENT`, `IPC_STREAMING_SESSION_SETUP_EVENT`, `IPC_STREAMING_STARTED_EVENT`, `IPC_STREAMING_FAILURE_EVENT`, `IPC_STREAMING_TERMINATED_EVENT`, `IPC_STREAMING_MODE_EXIT_EVENT`, `IPC_GERONIMO_PAYLOAD_EVENT`, `IPC_GFN_REMOTE_CONFIG_UPDATED_EVENT`, `IPC_LOGIN_STATUS_CHANGED_EVENT`, `IPC_EULA_ACCEPTED_EVENT`, `IPC_CLIENT_HEARTBEAT`, `IPC_CLIENT_EXIT`.

**L** `IPC_GERONIMO_PAYLOAD_EVENT` carries `IPC_GERONIMO_PAYLOAD_TYPE` = `CLIENT_INFO_PAYLOAD` (client version), `RCONFIG_PAYLOAD` (`remoteConfigType`, `remoteConfigVersion`), or `USER_INFO_PAYLOAD` (`externalUserId`, provider id; values redacted). `IPC_STREAMING_TERMINATED_EVENT` carries `extendedCode`, `isResumable`, `isSessionAlive`. `IPC_STREAMING_FAILURE_EVENT` carries `SESSION_SETUP_FAILURE`/`SESSION_RESUME_FAILURE` with `requestId` and `result`. Example: `gameStreamClientAgent.log` 18:31:56 `IPCMessage: CrimsonNative:BackgroundAgent GameStream:ClientAgent IPC_STREAMING_PREPARE_EVENT {"serverAddress":"<zone>.cloudmatchbeta.nvidiagrid.net"}`.

**L** The shell sends `IPC_CLIENT_HEARTBEAT` to BackgroundAgent every 10 s (`debug.log` `gfn_background_agent_ipc.cpp(29)] Sending 'IPC_CLIENT_HEARTBEAT' to BackgroundAgent` at 19:44:05.008, 19:44:15.045, 19:44:25.076, ...). The reliability monitor answers `IPC_GFN_REMOTE_CONFIG_UPDATED_EVENT` with `Method ... not registered with any listener. Returning NotImplemented.`

**I** The bus is for observability (traceroute, Wi-Fi telemetry, crash/reliability, updater). No media, input, or session control goes through it. OpenNOW does not need an equivalent for streaming correctness.

## Session flow, log-confirmed (2026-10-12 session)

| Step | Owner | Evidence |
| --- | --- | --- |
| 1. Geronimo init at app start | `GeForceNOW.exe` | **L** `debug.log` 18:31:47.993 `Browser app is initializing Geronimo` |
| 2. Prepare (early, at idle, before the user picks a game) | JS → `QUERY_GFN_PREPARE` → `GridApp::prepare` | **L** `debug.log` 18:31:56.810 `QUERY_GFN_PREPARE values, Server: <zone>..., DeviceID: <redacted>, Locale: en_US`, then 18:31:56.820 `Sending 'IPC_STREAMING_PREPARE_EVENT' to BackgroundAgent` |
| 3. Second prepare at game launch short-circuits | `GFNQueryHandler` | **L** 19:45:11.608 `QUERY_GFN_PREPARE already executed. Returning early forcing success` |
| 4. Streaming settings and renderer creation | `GFNQueryHandler`, `SimpleGridApp` | **L** 19:45:11.614 `ConfigureStreamerVideoSettings ... width: 2560, height: 1440, FPS: 120`; 19:45:11.620 `Creating audio renderer.` |
| 5. Start | `QUERY_GFN_START` → `GridApp::setAuthInfo` + `GridApp::start` → `nvbSetAuthInfo`, `nvbStartSession` | **L** 19:45:11.634 `QUERY_GFN_START values, Server: <zone>, ... Keyboard Layout: en-US`; 19:45:11.634 `Disabling session mouse acceleration`; 19:45:11.637 `AudioMode used for Start stream session: 2`; `geronimo.log` 19:45:11 `nvbSetAuthInfo(...)`, `nvbStartSession(...)` |
| 6. IPC fan-out | `BackgroundAgentIpc` | **L** 19:45:11.637 `Sending 'IPC_STREAMING_STARTING_EVENT'` and three `IPC_GERONIMO_PAYLOAD_EVENT` |
| 7. Queue/setup progress | Bifrost callbacks → `SimpleGridApp::onSessionSetupProgress` | **L** 19:45:12.072 `onSessionSetupProgress(state: 0, queue: 0, eta: 30000)`, then `state: 2` once per second until 19:45:28.398 |
| 8. Setup success | `SimpleGridApp::onSessionSetupSuccess` | **L** 19:45:29.436 `onSessionSetupSuccess()`; 19:45:29.437 `Sending 'IPC_STREAMING_SESSION_SETUP_EVENT'`; `geronimo.log` 19:45:29 `nvbStartStreaming(...)` |
| 9. Streaming begins | `SimpleGridApp::onStreamingBegin` | **L** 19:45:30.830 `onStreamingBegin()`, `Sending 'IPC_STREAMING_STARTED_EVENT'`, `AudioRenderer resumed`, `VideoDecoder resumed` |
| 10. Mic bring-up after streaming starts | `GFNQueryHandler` → `SimpleGridApp` | **L** 19:45:30.834 `Register mic callback`, `Initialize audio capturer as permission is allowed.`, `Creating audio capturer.`; 19:45:30.882 `Sending Mic State Event: 2` |
| 11. Upstream ready, cached input flushed | `GIOInterface` | **L** `geronimo.log` 19:45:31 `Sending N cached custom messages`, `Updated lock keys with cached values: 1 2 1`, `KBlayout:: upstreamready, SessionKBLayout: en-US, ... autoMode: true` |

The macOS research reached the same `prepare → setAuthInfo → start` boundary statically. The Windows logs add the timing: prepare is fired once at app start (with the zone server address), and the later prepare at game launch is a no-op.

## Comparison with OpenNOW

- OpenNOW keeps session orchestration in `native/opennow-core` and media in `native/opennow-streamer`, called from Qt through `NativeStreamRuntime`. That matches GFN's split in purpose (`GridApp` as a single facade over Bifrost) but OpenNOW has no counterpart to the early "prepare at idle" step. See [opennow-gaps.md](opennow-gaps.md).
- GFN's MessageBus plugins (traceroute, Wi-Fi diagnostics, reliability) have no OpenNOW equivalent. This is a diagnostics gap, not a streaming gap.
- GFN routes clipboard and some UI messages as NVST custom messages (`nvbSendMessage`) to named server-side recipients. OpenNOW opens the custom-message SCTP channels but never writes to them. See input.md.
