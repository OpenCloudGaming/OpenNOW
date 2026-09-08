# OpenNOW Qt shell

The supported OpenNOW Qt Quick/QML desktop application. It targets Qt 6.8
or newer and uses SDL3 for controller input. A bundled Rust process owns settings
and is the start of the shell-neutral application core. See
`docs/qt-migration.md` for the migration history and remaining release checklist.

## Code organization

`src/main.cpp` only enters `app/ApplicationStartup.cpp`, which composes the application-lifetime
services and binds them to QML. C++ features live in `app/`, `core/`, `input/`, `localization/`,
`media/`, `diagnostics/`, and `streaming/`. Cross-feature includes are qualified relative to
`src/`; there are no forwarding headers in the former flat layout.

The streaming item has separate lifecycle, input, and scene-graph translation units, but remains
one `StreamVideoItem` with one capture boundary. `streaming/rendering/` owns the GPU callback,
texture renderer, and graphics integration. `input/platform/` owns native Wayland capture.
The Rust runtime wrapper remains the single owner of FFI shutdown and callback marshalling.

Startup smoke fixtures and workloads, motion acceptance, and performance profiling live in
`src/acceptance/`. `AcceptanceSession` bounds their callbacks to the engine's lifetime; the
development switches and their execution order are unchanged.

Desktop QML is grouped into `shell/`, `components/`, `home/`, `library/`, `store/`, `settings/`,
`stream/`, `auth/`, `friends/`, and `updates/`. Settings pages live in `settings/pages/`, with
explicit screen/width dependencies, while reusable controls live in `settings/controls/`.
These folders still share the existing `OpenNOW` QML module and type names. Console screens,
overlays, shared components, and themes retain their existing folders.

`qml/state/ShellStore.qml` preserves the public singleton API and session/recovery orchestration.
It composes dedicated catalog, artwork, settings, and account-service owners in `state/catalog/`,
`state/settings/`, and `state/account/`. Property aliases retain reactive updates without copying
feature state; facade methods keep existing UI and acceptance consumers compatible.

The top-level CMake file composes focused modules in `cmake/`: `Sources` for C++, `QmlModule`
for QML, `Resources` for locales/shaders, `PlatformInput` for native pointer support,
`NativeRuntime` and `WindowsRuntime` for deployment, `Tests` for acceptance, and `Packaging`
for installation. Source lists are explicit; register new files in the matching module.
See the repository's `AGENTS.md` for the full ownership map and invariants.

## Remote streaming diagnostics

Settings → About → Copy diagnostics exports a bounded report containing both
`native-streamer.log` and `qt-native.log` from the core's diagnostics directory
(`%APPDATA%/OpenNOW/diagnostics` on Windows, or `OPENNOW_DATA_DIR/diagnostics`).
Reproduce the problem, wait at least 10 seconds, then export before closing the app.

The `diagnostics-v2` Qt startup marker identifies the detailed trace build. The trace
records request IDs on both sides of the embedded ABI, queue acceptance, callback
delivery, timed RTSPS stages, ICE/DTLS progress, two-second receive/assembly counters
(including zero frames), decoder submission, adapter identity and first-frame events.
Queue acceptance is not a successful handshake; a frame notification is not a
presented frame. Compare the successive stages to locate a stall.

Raw session contexts, credentials, URLs, SDP and media payloads are excluded from
the new handshake trace. Logs rotate during use, and exports retain readable lines.
No per-packet or per-input file logging is added to the gameplay path.

## Video color precision

The color-depth setting selects the stream's encoded and decoded precision, not HDR.
Ten-bit video stays ten-bit through native YUV-to-RGB conversion and GPU texture import;
SDR frame-generation history and generated output retain that texture format. The normal Qt SDR
window uses an 8-bit swapchain. Its final video draw applies centered, static 8×8 spatial
dithering after scaling so lower bits contribute to the displayed gradient instead of being
discarded before composition. Black, white, neutral balance, and the SDR transfer function
are preserved. High-precision texture render targets do not receive the 8-bit dither.

`qt-native.log` reports `Video composition` when the imported format, source color space, or output format changes,
including `sourceColorSpace`, `textureBits`, `outputBits`, and the dither policy. A ten-bit SDR stream with
`textureBits=10 outputBits=8 dither=ordered-8x8` is the expected SDR path, not native ten-bit
scan-out. HDR10 swapchains are not used to display unconverted SDR pixels.

Run `ctest --test-dir build/opennow-qt -R opennow-streamcolor-tests --output-on-failure`
for GPU readback checks of ten-bit gradients, endpoint and neutral colors, final quantization,
and high-precision render targets. Live acceptance still requires checking gradients and
black/white levels on the intended GPU and display in windowed and fullscreen modes.

## Build

Install the Qt ShaderTools development module with Qt Quick and Multimedia; CMake
bakes the portable video-composition shaders into the executable at build time.
Linux input also requires `pkg-config`, `libwayland-dev` (including `wayland-scanner`),
and `wayland-protocols`. These are mandatory build dependencies, including for builds
that will run on X11. The Wayland backend uses the display, surface and pointer owned
by Qt; it does not create a second connection or presenter window.

```sh
cmake -S opennow-qt -B build/opennow-qt -DCMAKE_BUILD_TYPE=Debug
cmake --build build/opennow-qt
ctest --test-dir build/opennow-qt --output-on-failure
```

### macOS

Use Xcode command-line tools, CMake 3.24+, Rust, Qt 6.8+ (including Quick,
Multimedia, ShaderTools, Svg, and the Gui private headers), and a shared SDL3 build.
Point `CMAKE_PREFIX_PATH` at Qt and SDL3 if they are not already discoverable.
Build one architecture at a time; universal Qt bundles are not supported.

```sh
arch=$(uname -m)
case "$arch" in
  arm64) rustup target add aarch64-apple-darwin ;;
  x86_64) rustup target add x86_64-apple-darwin ;;
esac
cmake -S opennow-qt -B build/opennow-qt -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_OSX_ARCHITECTURES="$arch"
cmake --build build/opennow-qt --parallel 4
ctest --test-dir build/opennow-qt --output-on-failure --parallel 2
./build/opennow-qt/OpenNOW.app/Contents/MacOS/OpenNOW
cpack --config build/opennow-qt/CPackConfig.cmake -G ZIP -B build/qt-packages
```

CMake selects the matching Rust target; an explicitly supplied
`OPENNOW_RUST_TARGET` must agree with the Qt architecture. The app bundle carries
the core, acceptance verifier, standalone capability probe, embedded streamer
dylib, and deployed Qt/SDL3 libraries. The FFI dylib uses an `@rpath` install name
so it can load outside the build tree.
The app explicitly links Qt Svg so macdeployqt includes the SVG image plugin used
by the QML icons; the relocated-bundle check requires that plugin to be present.

The separate `macos-validate` CI job builds and tests the native Apple Silicon
stack, then checks a relocated ZIP with the development dependencies hidden.
These are unsigned validation artifacts, not notarized releases, and are not
part of the Linux/Windows nightly inventory. Offscreen tests cover shell and FFI
contracts; real VideoToolbox/Metal presentation, audio, input capture, and login
still require macOS hardware and a GFN account.

Run with the offscreen Qt platform plugin for a startup smoke test:

```sh
QT_QPA_PLATFORM=offscreen ./build/opennow-qt/opennow-qt \
  --smoke-test --allow-multiple-instances --route home
```

Useful development switches are `--route <name>`, `--overlay <name>`,
`--reduced-motion`, `--core <path>` and `--screenshot <png-path>`. The test suite
opens every route and overlay with QML warnings treated as failures.

Run `ctest --test-dir build/opennow-qt --output-on-failure -R 'theme-tests|qml-theme-settings'`
to check all built-in packs in both appearances, accent contrast, preview restoration,
and the desktop Look controls at compact and desktop widths. The
`--smoke-theme-settings --route settings-themes --desktop` workload also accepts
`--smoke-light-theme` and `--screenshot <png-path>` for account-free visual checks.

Run `ctest --test-dir build/opennow-qt --output-on-failure -R '^qml-stream-exit-'`
to check the in-stream exit confirmation in desktop/console and windowed/fullscreen modes.
The keyboard fixtures cover Return, keypad Enter, Tab+Space, Escape, safe default focus,
auto-repeat suppression, and preserving the stream surface/input across cancellation.
They use a smoke session, not a live GFN connection.

Run `ctest --test-dir build/opennow-qt --output-on-failure -R '^qml-session-launch-'`
to check the desktop launch screen in windowed/fullscreen and normal/reduced-motion modes.
The fixture exercises queue updates, entry/first-frame fades, reconnects, failures, cancellation
confirmation, and interrupted transitions while checking that the same video item survives.
Receiver readiness alone must not dismiss the launch screen. These tests inject native status
and first-frame events; they do not establish live network or decoder behavior.
When Xvfb is available, the video variants also render a synthetic GPU texture through the
production video material and check that it stays covered until the first-frame handoff.

Run `ctest --test-dir build/opennow-qt --output-on-failure -R '^qml-session-fullscreen-'`
to verify that F11 can leave and re-enter fullscreen after confirming session exit,
in desktop/console mode, restoring either the normal or maximized window state.

### Local frame generation (experimental)

The desktop Streaming settings and console Video settings offer **Off** (default) or **2×**.
This is local video interpolation, not a higher GeForce NOW tier or a change to the negotiated
stream FPS. A 60 FPS stream targets 120 displayed FPS only with a sufficiently fast local GPU
and a display running at approximately 120 Hz or higher. Input and game simulation remain at
the source rate. Interpolation adds presentation latency and can artifact around fast motion,
thin geometry, transparency, repeated textures, and the game's HUD.
Generation is limited to a nominal 120 FPS target: source cadences faster than 60 FPS (with a
small arrival-jitter margin) bypass interpolation and report `source-rate-limit`. In particular,
a 120 FPS source is never doubled to 240, even on a 240 Hz or faster display. Normal source
presentation is not capped or renegotiated by this guard.

The Qt render-thread helper samples native GPU textures into two retained histories, estimates
bidirectional motion through three reduced-resolution pyramids, and synthesizes one midpoint.
Unreliable regions and detected scene cuts use the actual current image. OpenNOW's overlays
are composed afterward on the existing video surface. There is no CPU image readback, separate
presenter, neural model, or additional runtime dependency. The Off path allocates no interpolation
resources. Source frame identifiers, protocol feedback, audio, and source-stream recording are
unchanged.

The helper accepts single-sample RGBA8 and RGB10A2 textures with renderable RGBA16F flow targets,
up to 4096 pixels per axis and 4096×2160 pixels total. Owned texture storage is approximately
25 MiB at 1080p and 104 MiB at the maximum area. The motion grid is capped at 320×180. The pacer
rejects missing, reordered, stalled, or substantially discontinuous sources; insufficient display
refresh bypasses interpolation. Missing or grouped decoder timestamps alone are not missing
frames: consecutive frame IDs can use the median of the last eight local source-arrival intervals
when per-frame timestamps are unusable. Zero-duration arrivals remain in that bounded window;
bursts without a usable cadence fall back rather than inventing a rate. Raw source timestamps and
IDs are never rewritten. An undrained original frame triggers a two-second cooldown
instead of growing a queue. Device/surface changes and toggling the mode clear interpolation
history. Unsupported GPU resources leave normal streaming active.

The statistics overlay keeps **STREAM FPS** as the source measurement and adds **LOCAL OUTPUT
FPS** when 2× is selected. The latter counts newly selected video outputs at Qt's `frameSwapped`
boundary, not twice the stream FPS and not unrelated overlay redraws. It is a presentation-submit
measurement, not a hardware scanout measurement; generated slots rejected by the scene-cut or
confidence checks can contain the actual source image. The frame-generation status reports
warmup, insufficient refresh, overload, discontinuities, and unavailable resources.
Both desktop and console routes pass the active video item's snapshot to the top-level statistics
overlay and clipboard report. Sampled state changes are logged on the GUI thread to
`diagnostics/native-streamer.log` as `shell-mode frame-generation state=... outputFps=...`;
The entries include the timing source, rejection reason, raw timestamp/arrival deltas, sequence
delta, inferred interval, and Qt's display refresh rate. Changes to the timing source, rejection
reason, or display refresh also produce an entry; FPS-only updates do not. `scope=acceptance`
distinguishes smoke fixtures from `scope=stream`. No file I/O is added to the render thread.

Run the focused checks with:

```sh
ctest --test-dir build/opennow-qt --output-on-failure \
  -R 'framepacer|frameinterpolator|nativeframegeneration|frame-generation|streamvideo'
```

On Linux, installing `xvfb` and Mesa Vulkan drivers lets CTest run the actual shader tests through
Xvfb. Without Xvfb, the offscreen platform can skip Vulkan coverage. Set
`OPENNOW_FRAMEGEN_VALIDATION=1` with the Khronos validation layer installed to check Vulkan
resource use. The tests cover translated images versus a crossfade, cut fallback, 10-bit values,
resource recreation, pacing, and the settings-to-surface bindings. Software Vulkan correctness
does not establish a physical GPU's 8.33 ms presentation budget.
The Linux/Windows `opennow-nativeframegeneration-tests` target drives the production native render
callback with injected FFI frames and real adopted-device Vulkan/D3D11 textures. It checks absent,
repeated, and grouped PTS, generated/original scheduling across `frameSwapped`, pixel readback,
Off, discontinuity diagnostics, metadata preservation, and resource release. Windows uses a
D3D11 software device for deterministic CI coverage. It is not a network session, hardware
swapchain test, or sustained output-FPS benchmark.
Moving-image rows also require the presented midpoint to differ from the current source and
approximate halfway motion, followed by an exact original. The shader suite separately checks
pixel-scale detail at small motion offsets. These checks do not replace validation of a live
Windows swapchain or establish that the submitted-output counter counts distinct motion frames.
The `qml-frame-generation-stats-desktop` and `qml-frame-generation-stats-console` acceptance tests
feed controlled snapshots through the render-callback boundary and the real item's timer, route
facade, top-level overlay, and clipboard report. They check FPS-only changes, fallback/recovery,
compact/expanded/hidden overlays, fullscreen transitions, and clearing stats after leaving a stream;
they do not inject values into the statistics component.

Before treating a GPU/backend as performance-validated, test a real 1080p60 stream on a 120 Hz+
display with Off and 2× in both windowed and fullscreen modes. Check output cadence, GPU time,
input latency, scene cuts, reconnects, resize/display changes, and overlays open/closed. STREAM
FPS must remain near 60, LOCAL OUTPUT FPS should approach 120 only when the hardware sustains
it, and fallback must not accumulate delay. Repeat on D3D11, Vulkan, and Metal hardware.

### HDR presentation

`HdrOutput` owns render-thread swapchain format changes and fail-closed display capability.
It prefers scRGB, probes the current surface again after display changes, and uses SDR if HDR
output or the required resources are unavailable. `CoreClient` injects the transient
`runtimeCapabilities.nativeHdrSupported` flag into session creation and stream preparation;
the core owns the final decoder/backend gate and persisted `enableHdr` preference.

ABI 6 imported frames carry their PQ/HLG/SDR color space explicitly. The video shader converts
PQ and HLG BT.2020 to linear scRGB, or performs luminance-based SDR tone mapping after HDR
output is lost. SDR chrome uses bounded QML layers and `HdrChromeEffect` to decode sRGB before
composition; the normal SDR path has no additional layers. HDR10-only surfaces require one
window-sized RGBA16F scene target (bounded to 8K) and a final PQ-encoding GPU pass so translucent
chrome blends in linear light. scRGB keeps the direct video path. Neither path creates another
window, copies frames to the CPU, or restarts media for overlays. Experimental frame generation
is explicitly unavailable for HDR sources.

Run `opennow-hdrcolor-tests`, `opennow-linuxvulkangraphics-tests`, and the backend-availability
QML workload. The GPU tests check shader luminance/gamut math, tone-map monotonicity, actual
linear alpha blending through the HDR10 output pass, and chrome-layer resizing/fullscreen.
On Windows, the HDR test uses D3D11 WARP for both QRhi devices and disables desktop-settings
discovery before constructing `QGuiApplication`. This keeps the native Windows window and GPU
shader checks independent of the WinRT theme services missing from Windows Server CI runners.
The Windows x64 CI lane runs on an interactive GitHub-hosted desktop and checks that desktop
before building; session-0 service runners cannot validate native window exposure or fullscreen.
These tests do not establish that a physical HDR monitor received HDR. The live Windows/Linux
display and reconnect matrix is in [HDR validation](../docs/hdr.md).

### Native input acceptance

Run the controller, stream video, native runtime and Wayland pointer unit tests first:

```sh
ctest --test-dir build/opennow-qt --output-on-failure \
  -R 'opennow-(controllerinput|streamvideo|nativestreamruntime|waylandpointer)-tests'
```

With a native Wayland compositor and an interactive pointer seat, run these opt-in
tests and move/click the pointer inside the test window when requested by capture:

```sh
QT_QPA_PLATFORM=wayland OPENNOW_TEST_WAYLAND_CAPTURE=1 \
  ./build/opennow-qt/opennow-waylandpointer-tests compositorCaptureLifecycle
QT_QPA_PLATFORM=wayland OPENNOW_TEST_WAYLAND_CAPTURE=1 \
  ./build/opennow-qt/opennow-streamvideo-tests \
  waylandAbsoluteToPendingRelativeLockSurvivesUntilCompositorAcknowledges
```

The compositor must provide `zwp_relative_pointer_manager_v1` and
`zwp_pointer_constraints_v1`. Missing support visibly disables relative gameplay
capture without stopping audio/video. Native Wayland never falls back to XWayland
XInput2 or cursor warping. X11 retains XInput2 and Windows retains Raw Input.
The ordinary input queue remains bounded to 256 events, with four coalesced controller
neutral slots reserved for capture closure. Focus loss never depends on QML callback
ordering to release a held controller.

The public-safe `--smoke-test --route stream --smoke-input-capture-error` fixture
renders the shared error banner over the existing stream item. Add `--desktop` for
the desktop shell and `--screenshot /absolute/path.png` to save visual evidence.
Offscreen and nested compositor tests do not replace live Windows/X11/Wayland
controller and mouse acceptance on real streaming sessions.

Microphone fixtures never open capture hardware: `--smoke-test --desktop --route
settings-audio --smoke-microphone-supported` renders the default-disabled opt-in,
and `--smoke-test --desktop --route stream --overlay desktop-stream-menu
--smoke-microphone-muted` renders a supported, muted session. Add
`--screenshot /absolute/path.png` for visual evidence. For console fixtures use
`--console`, `settings-input`, and `guide-session` respectively. The
`--smoke-microphone` acceptance workload checks state, commands and reconnect mute
preservation against a mock runtime.

If one controller appears as both a physical device and a remapper's virtual device,
open Settings → Controls → Controller input source (Controllers in the console
settings) and select one device before starting the stream. The selected device is
Player 1; other sources cannot send gamepad, menu-navigation, or Guide-button input.
All controllers (multiplayer) restores normal independent player slots. Selection
is local to the running app and is not saved as a device blacklist. After the selected
device disconnects, input stays disabled until a source is selected again; reconnecting
does not silently activate another device. This does not suppress keyboard or mouse
events generated by an external remapper. Disable those mappings in the remapper if
they also duplicate input.

The desktop controller rows and source picker use SDL's reported device name and
[`SDL_GetRealGamepadType`](https://wiki.libsdl.org/SDL3/SDL_GetRealGamepadType)
for PlayStation (PS3/PS4/PS5) and Xbox (360/One-family) logos. Unknown devices keep
the generic gamepad icon. This ignores button-layout mapping overrides, but a
remapper that exposes only a virtual Xbox device can still hide the physical pad's
identity; OpenNOW does not guess its brand from the name.

Battery status uses [`SDL_GetGamepadPowerInfo`](https://wiki.libsdl.org/SDL3/SDL_GetGamepadPowerInfo)
on the existing SDL handle, refreshed every two seconds without a second HID reader.
It reports remaining charge and charging/full/wired states, not battery wear,
design capacity, or cycle count. Missing data is shown as **Battery unavailable**,
never as an empty battery. Availability depends on the controller, connection,
driver, remapper, and HID permissions. SDL's PS4/PS5 HIDAPI reports can be coarse;
its Xbox One HIDAPI and Windows XInput paths map battery bands to approximate
percentages. Enhanced-report settings are left unchanged because switching modes
can affect other applications. No additional raw-HID permissions are requested.

Run `ctest --test-dir build/opennow-qt -R 'opennow-controllermetadata-tests|qml-controller-metadata'`
for virtual-device identity and synthetic battery-display coverage. The QML fixture
also supports `--smoke-test --desktop --route settings-input --smoke-controller-metadata
--reduced-motion --screenshot /absolute/path.png`. Its device names and charge values
are test data, not hardware measurements. Verify real USB/Bluetooth pads separately.

Run `ctest --test-dir build/opennow-qt -R '^opennow-controller(input|sources)-tests$'`
for virtual-device coverage of source selection, neutral handoffs, disconnect/reselect,
independent stick repeats, and held-button release across input ownership changes.

The representative-hardware performance workload drives production route and popup motion,
checks focus after every transition, and records refresh-relative frame budgets atomically:

```sh
./build/opennow-qt/opennow-qt --allow-multiple-instances \
  --performance-report "$PWD/opennow-1080p.json" \
  --performance-width 1920 --performance-height 1080 \
  --performance-cycles 3 --performance-label linux-intel-uhd \
  --performance-require-hardware
```

Requested dimensions are physical pixels, so the workload remains comparable on HiDPI screens.
Offscreen/software runs validate the harness only and are rejected when
`--performance-require-hardware` is present. Hardware acceptance also forbids the test-only
`--performance-refresh-hz` override and records both the effective and display-reported rates.
See `docs/qt-acceptance.md` for the release matrix.

The `theme-store` route implements the current Paper V3 collection with
controller filters, temporary preview, persistent install/apply and access to
the local plain-file theme directory.

The versioned Rust core owns settings, NVIDIA device login and token refresh,
OS-protected accounts, PINs, catalogs, subscriptions, regions and latency tests,
account connections, persistent storage, CloudMatch lifecycle/recovery/ads,
NVST session orchestration, diagnostics, media listing, Discord, telemetry,
feedback and update discovery. The protocol-v6 native streamer is linked into
the Qt executable as an in-process Rust library. It owns NVST RTSPS negotiation,
Mjolnir video, the ICE/DTLS/SCTP control bundle, decode, audio and native input.
Qt/QML owns stream status, stats, menus, recovery, failure and fullscreen
chrome. The shell sends one complete CloudMatch session context and never
proxies RTSPS, ICE, SRTP or encoded media. Decoders publish native GPU frames
through the C FFI; `StreamVideoItem` imports and samples them on Qt's QRhi
render command stream without a child streamer process or native presenter
window. CPack installs the Qt executable, `opennow-core`, the runtime library
and `opennow-streamer` for the core's capability probe. Qt streaming still runs
in process through the runtime library, not in the probe executable. CI produces
the platform packages from that layout.
The screenshot shortcut captures the exact stream region, and F12 records the
negotiated H.264/H.265/AV1 source stream plus Opus audio atomically into Matroska
before generating a media thumbnail.

Settings → Recording exposes source capture information, the recordings folder,
editable recording/clipping shortcuts, and an opt-in replay buffer. Replay is off
by default. Enable it before starting a session, choose a 15/30/60/120-second target
and a 64/128/256/512 MiB memory cap, then press Ctrl+F12 to save recent gameplay.
F12 still starts/stops a manual recording. Both bindings also appear under Controls
→ Shortcuts. Disabling replay immediately clears its history and cancels an export;
enabling it or changing its limits takes effect next session.
Both stream interfaces show a recording timer and transient clip results above the
same video item without taking focus or opening a blocking overlay.

Recording and clips use the incoming resolution, frame rate, codec and bitrate.
There is no independent scaling or encoding pass; change Stream settings to change
the source profile. Clips can be shorter than their target when constrained by
memory, available keyframes, a recent save, or stream recovery. Source passthrough
avoids video encoding and GPU readback, but packet management and writing files
still use some CPU and disk bandwidth. Live playback/performance checks on each
supported OS remain required.
Clip publication requires hard-link support in the destination filesystem to avoid
overwriting an existing file; unsupported filesystems report an error without
publishing a partial clip. A/V timing uses source timestamp deltas with initial
receive-time anchors because the stream does not expose sender clock correlation.

Microphone capture defaults to disabled;
Audio settings offers an explicit Open microphone opt-in using the system default
input. The setting applies to the next session, and supported sessions expose live
mute/unmute through the stream menu or Ctrl+Shift+M without restarting media.
Build support and negotiated session support are checked independently. Live multi-OS streaming, GPU interop and
hardware validation plus production signing/notarization remain release gates.
The legacy Electron application has been removed; it is not a fallback in this tree.

The current presenter is a Qt scene-graph item. Platform decoders retain native
textures, record any required conversion and synchronization into the active
QRhi command buffer, and expose only opaque frame tokens across the FFI. QML
overlays therefore compose normally above the stream. No CPU frame callback,
standalone SDL presenter, child HWND or paired top-level video window participates
in the Qt path.

`StreamVideoItem` uses `QSGRenderNode`: native YUV conversion runs before the Qt
scene pass, then the converted texture is drawn directly into that pass. There is
no additional item-sized RGBA render target. The shared video material preserves
transforms, inherited opacity, scissor/stencil clips and letterboxing, and caches
imported texture bindings per QRhi frame slot.

For GPU pixel acceptance (including overlays, clipping and fullscreen), run
`opennow-streamvideo-tests` with the native platform plugin, not `offscreen`.
The normal offscreen CTest run deliberately skips those hardware-only checks.
