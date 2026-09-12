# OpenNOW Qt shell

The supported OpenNOW Qt Quick/QML desktop application. It targets Qt 6.8
or newer and uses SDL3 for controller input. A bundled Rust process owns settings
and is the start of the shell-neutral application core. See
`docs/qt-migration.md` for the migration history and remaining release checklist.

## CI checks and manual builds

Pull requests and pushes to `dev` or `main` run workflow lint, packaging-contract
tests and localization validation, followed by a native test matrix for Linux x64,
Windows x64, and macOS ARM64. Each platform must pass Rust format/lint/tests, QML
syntax checks, and the headless-compatible `ci-unit` Qt tests. The Qt check compiles only the
`opennow-ci-unit-tests` target, not the application or the release streamer with
bundled FFmpeg. Rust test binaries and the SDL3 test dependency still need compiling;
Rust, Qt, and SDL caches reduce repeated work.
General-purpose check and package jobs use Blacksmith runners. The isolated
release-signing job remains on `opennow-release-signer`.

The upstream Rust cache action automatically uses Blacksmith's colocated cache.
Platform-specific shared keys survive job renames, and dependency caches are saved
even if a later test fails. Check and release-build caches remain separate; each
still invalidates when the Rust toolchain or dependency configuration changes.

Full application builds, embedded-runtime/QML acceptance tests, Linux/Windows ARM64
builds, and package creation run only on manual dispatch. Automatic checks do not
prove those full-application or ARM64 cross-platform paths work; run a manual build
before shipping changes that touch them. All five package platforms share one
matrix, including macOS; platform-specific steps handle its app bundle and relocation.

Required status names are `linux-x64`, `windows-x64`, and `macos-arm64`. Keep all
three required in the repository's branch rules. Each also fails if shared checks
fail or are cancelled, and manual packaging waits for every platform to pass.

To run the test-only Qt suite locally after configuring a Debug build:

```sh
cmake --build build/opennow-qt --target opennow-ci-unit-tests --parallel 4
ctest --test-dir build/opennow-qt --output-on-failure --no-tests=error -L ci-unit --parallel 4
```

Windows CI runs 17 Qt targets; Linux and macOS run 18. The Windows HDR/native-window
test and macOS native cursor-capture test require an interactive desktop, so they
remain registered under `interactive-desktop` instead of `ci-unit`. Blacksmith
package jobs also exclude this label. No test assertions are disabled or relaxed.

Run the separate suite from an interactive Windows or macOS session after configuring
the Debug build. On Windows, first verify the desktop with the existing preflight:

```powershell
.github/scripts/ensure-windows-test-desktop.ps1
```

```sh
cmake --build build/opennow-qt --target opennow-interactive-tests --config Debug --parallel 4
ctest --test-dir build/opennow-qt -C Debug --output-on-failure --no-tests=error -L interactive-desktop
```

### Manual artifact-only builds

In GitHub **Actions → qt-ci → Run workflow**, select `dev` (or the branch or tag
to build) and leave **Publish a nightly with signed update manifests after all checks pass**
unchecked. Leave the `public_key` input empty for a no-key artifact-only build. The workflow
uploads build artifacts without creating a release, tag, or updater manifests, and requires
no signing environment. No separate supporter-build workflow is needed.

After a successful run, download
`opennow-qt-<version>-complete-unsigned` from the run's **Artifacts** section.
Versions use `<project-version>-nightly.<run-number>.<attempt>` even when release
publishing is disabled. The complete archive contains Windows x64/ARM64 MSI installers and
portable ZIPs, Linux x64/ARM64 AppImages and DEBs, the Apple Silicon macOS DMG, `SHA256SUMS`,
and source-commit metadata. The macOS ARM64 validation ZIP is a separate artifact.
Artifacts expire after 14 days.

These default no-key builds have no platform publisher signatures and require manual downloads
for updates. Public publishing instead requires a pinned public key and the protected isolated
signer described in the [signing setup guide](../docs/update-signing-setup.md); see the
[nightly release runbook](../docs/qt-nightly-release.md) for the publishing command. Users of
earlier no-key nightlies must manually install an update-enabled build once before verified
in-app updates can work. Windows may show a SmartScreen warning; macOS packages are not notarized.
Windows ARM64 is cross-built rather than runtime-tested. Release Linux DEBs bundle Qt and SDL3
for Ubuntu 24.04 / Linux Mint 22.x; AppImages are the portable option. Download the files and distribute them through
your supporter channel. **Actions artifacts in this public repository are not
private or supporter-access-controlled**, even though they do not appear in Releases.

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

For Raspberry Pi 4 and Pi 5, see the [Raspberry Pi OS requirements and hardware
acceptance checklist](../docs/raspberry-pi.md).

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

## macOS upscaling

Settings → Stream → Upscaling offers **Off** (default) and **MetalFX** on macOS.
The **Clarity** slider runs from 0–15 (default 10), and **Noise Reduction** from
0–20 (default 0), matching OpenNOW-Mac. Both are enabled only with MetalFX selected;
zero disables that enhancement. Values persist when upscaling is turned off and
update both desktop and console stream surfaces without restarting the session.
The console-oriented Stream settings expose the same saved preference. Linux and
Windows show no upscaling control and never enable MetalFX, even if a settings file
was copied from a Mac.

MetalFX spatial scaling runs on the decoded image before Qt draws stream chrome.
It is used only when enlarging the video, leaves the requested stream resolution
and frame rate unchanged, and adds GPU work rather than improving server rendering
performance. Quality depends on the game and compression. Unsupported devices or
scaler configurations retain normal scaling, including ten-bit color precision.
This does not add temporal reconstruction. HDR output is negotiated separately from upscaling.

Run `ctest --test-dir build/opennow-qt -R 'qml-upscaling|streamvideo-tests|nativestreamruntime' --output-on-failure`
for settings visibility, live preference binding, source geometry, and render
viewport checks. On a MetalFX-capable Mac, also run the ignored native spatial
tests documented in the macOS platform crate, then compare Off/MetalFX with a
lower-resolution live stream in windowed/fullscreen modes and with overlays open
and closed. Check resize/display-scale transitions, frame time, and color before
recommending the option for a particular GPU.

For an account-free settings preview, add `--smoke-upscaling --screenshot <absolute-png-path>`
to a `--smoke-test --desktop --route settings-streaming` launch. This acceptance-only
preview reveals the macOS control on any host after verifying its platform gate;
it does not enable MetalFX on unsupported platforms.

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
cpack --config build/opennow-qt/CPackConfig.cmake -G 'DragNDrop;ZIP' -B build/qt-packages
```

CMake selects the matching Rust target; an explicitly supplied
`OPENNOW_RUST_TARGET` must agree with the Qt architecture. The app bundle carries
the core, acceptance verifier, standalone capability probe, embedded streamer
dylib, and deployed Qt/SDL3 libraries. The FFI dylib uses an `@rpath` install name
so it can load outside the build tree.
The app explicitly links Qt Svg so macdeployqt includes the SVG image plugin used
by the QML icons; the relocated-bundle check requires that plugin to be present.

The manual `macos-arm64` package matrix entry builds and tests the native Apple Silicon
stack, then checks both a relocated ZIP and an app copied from the mounted DMG with development
dependencies hidden. The Apple Silicon DMG is part of the public nightly inventory; the ZIP
remains a separate CI validation artifact. These applications have no Developer ID signature or
notarization and require manual updates. See
[`qt-nightly-release.md`](../docs/qt-nightly-release.md) for installation warnings and the
Windows MSI/portable ZIP contract. Offscreen tests cover shell and FFI
contracts; real VideoToolbox/Metal presentation, audio, input capture, and login
still require macOS hardware and a GFN account.

Relative mouse mode uses native CoreGraphics pointer capture on macOS, not repeated
cursor warps. F8 (or the configured pointer-lock shortcut) explicitly locks or
unlocks the pointer; server cursor notifications do not override that choice until
the session resets. Unlocked absolute mode deliberately allows the pointer to leave
the window. Opening input-blocking overlays, switching applications, hiding the
window, or ending the session releases native capture and restores the Mac cursor.
While the seat still composites its cursor at startup, OpenNOW hides the local
cursor over the active video to avoid displaying both.

Run `ctest --test-dir build/opennow-qt --output-on-failure -R 'macpointer|streamvideo-tests'`
for the capture lifecycle and input-routing regressions. On macOS, the serial
`opennow-macpointer-native-tests` case uses Cocoa/CoreGraphics to acquire, hide,
release, and restore the actual system cursor. A live session with two displays is
still required to verify physical mouse containment, mixed-DPI transitions, and
the server/client cursor handoff.

### First-run onboarding

New profiles enter setup after provider sign-in. The flow covers the 1.0.0 beta
notice, desktop or console mode, streaming preferences, optional frame generation,
macOS-only MetalFX upscaling, and optional GitHub Sponsors support. Existing
profiles migrate with onboarding completed and retain their preferences.

Choices stay in a local draft until **Finish setup** or **Skip setup**. Both actions
save the current choices, then persist `onboardingCompleted` last. If a write fails,
setup keeps the draft and presents the error for retry. Selecting console mode does
not replace the wizard while its settings are being saved. No payment or diagnostic
upload happens during onboarding.

To repeat setup, open **Settings → About → Replay onboarding** at the bottom of
the page, then confirm **Restart and replay**. OpenNOW saves only
`onboardingCompleted=false` before restarting; other preferences and saved accounts
are kept. End an active or starting stream first. A failed save leaves the app open
and allows retrying. The replacement process starts after the old window, core,
native runtime and single-instance listener have been released.

The Picture step uses the same aspect-ratio groups, monitor filter and resolution
stepper as Settings. Selection still edits the onboarding draft rather than saving
immediately. Short pages center within the available content area; longer pages
remain scrollable, including keyboard focus inside the expanded resolution picker.

On macOS, setup requires the `awdl0` interface to be down. Boost offers an explicit,
confirmed disable action using the macOS administrator prompt. Finish and Skip
both check the current interface state, with a fresh check before sending the
completion-marker write after the other settings. Enabled, unreadable and busy
states block completion and return to Boost without discarding the draft. A
confirmed absent interface needs no change; other platforms have no AWDL requirement.
Re-enabling AWDL from the card undoes the network change and blocks completion again.
OpenNOW never disables AWDL automatically, stores administrator credentials, or
installs a service. Disabling AWDL can interrupt AirDrop, AirPlay, Sidecar and other
Continuity features for all users. macOS may re-enable it after the check; OpenNOW
does not enforce its state after setup. An interface-down reading is not proof of
zero AWDL radio traffic or a guaranteed fix for streaming stutter.

Run persistence and whole-app acceptance without an account:

```sh
ctest --test-dir build/opennow-qt -R 'onboarding' --output-on-failure
QT_QPA_PLATFORM=offscreen build/opennow-qt/opennow-qt \
  --smoke-test --allow-multiple-instances --desktop --route home \
  --smoke-onboarding --onboarding-step 0 --smoke-width 1440 \
  --reduced-motion --screenshot /absolute/path/onboarding.png
```

`--onboarding-step` accepts 0 through 5. Add `--smoke-light-theme` or
`--onboarding-ui-scale 1.25` to inspect alternate appearances. For the existing
provider sign-in screen, replace `--onboarding-step 0` with `--onboarding-login`.
Add `--onboarding-ui-check` to verify centering and draft resolution selection, and
`--onboarding-resolution-expanded --onboarding-step 2` to exercise the expanded
picker. Use `--onboarding-replay-check` to verify the Settings confirmation,
cancellation and failed-save retry; add `--onboarding-replay-dialog` for a dialog
screenshot. These replay UI fixtures do not restart the test process.
Add `--onboarding-awdl-check --onboarding-step 3` to exercise the macOS card with an
injected controller on any platform. This checks confirmation, cancellation,
restore, busy, error and unsupported states without changing a network interface.
These switches run only with the smoke fixture; they do not start provider login,
open donation links, or write account settings. Real provider approval and native
MetalFX output still require the corresponding account and macOS device.

Capture every setup step in desktop, compact 1.25×, and light appearances, plus
desktop and compact login, for visual comparison with the Paper design:

```sh
bash scripts/capture-qt-onboarding.sh build/opennow-qt/opennow-qt /absolute/path/onboarding-review
```

The compact scroll checks focus every eligible control, verify that the focused
control fits inside the viewport, and capture the final scrolled position. The
capture script uses OpenGL and starts Xvfb on headless Linux, so shader-backed
controls render rather than disappearing under the offscreen software backend.
It requires a built app and `xvfb-run` on headless Linux. Each PNG has a matching
acceptance log. The mode cards use the original Paper shell previews; these are
illustrations, not the signed-in user's library.

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
to verify that session exit restores the pre-session windowed, maximized, or fullscreen
mode in both desktop and console shells, and F11 still toggles correctly afterward.
The fixture also checks exit cancellation, launch/reconnect route transitions, and a
subsequent aborted launch with a different initial window mode, and automatic fullscreen
on direct launch.

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

Windows uses Qt's active DXGI output color space, not the monitor's advertised capability.
Metal uses Qt's display-referred extended-linear sRGB output: 1.0 is SDR white, unlike
Windows scRGB's fixed 80-nit reference. PQ/HLG content is normalized to a 203-nit content
white on Metal, and SDR chrome remains at 1.0. If current EDR headroom disappears while
the format remains supported, HDR video is tone-mapped to SDR in the linear surface. If
the display no longer supports that format, or its creation fails, the swapchain falls
back to SDR. Recovery explicitly restores the existing CAMetalLayer's sRGB color space
and disables extended-range content, because Qt 6.8 does not reset these on its SDR path.
The same reset is applied after scene-graph recreation; the stream and its video item stay alive.

Linux requires Vulkan HDR surface-format support and a complete, current Wayland
`color-management-v1` output description with PQ encoding and target luminance above SDR
reference white. The protocol XML is pinned in `protocols/` so older distribution build
hosts still compile the observer; only Wayland client headers and `wayland-scanner` are
needed to generate its bindings. Vulkan formats, EDID capability, and preferred-image hints alone do not
enable HDR. The observer does not assign the surface color space; Vulkan WSI owns that.
X11, missing/older color-management support, incomplete or ICC-only descriptions, and
configurations with more than one Qt screen remain SDR. This is a conservative configured-output
check, not a measurement of optical display output.

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
These tests do not establish that a physical HDR monitor received HDR. Repeat live validation
on macOS with current EDR headroom available and exhausted, and on Linux with compositor HDR
enabled and disabled, including a forced HDR preferred-image hint on an SDR output.
The live Windows/Linux
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

Settings → Audio contains two independent, default-off background options.
**Mute when out of focus** silences local playback while another app is active and
restores it on return. It does not pause video, microphone capture, or recording.
**Background stream reminder** requests taskbar or dock attention every five minutes
while a session is streaming in the background. Returning to OpenNOW, ending the
stream, or disabling the option stops the timer. Desktop support determines how
the attention request appears; this is not an AFK-timeout warning or anti-AFK control.

Run `ctest --test-dir build/opennow-qt -R qml-background-stream --output-on-failure`
to check toggles, focus transitions, reminder cancellation, and runtime restart state.
For live acceptance, enable both options and switch apps during a stream in windowed
and fullscreen modes, with the stream menu open and closed. Check silence on leaving,
audio restoration on return, uninterrupted video and recording, and attention after
five minutes away. Repeat with each option disabled independently.

Settings → Controls → **Clipboard paste** enables local-to-stream plain-text paste
with Ctrl+V (Command+V on macOS). The console Controls page exposes the same persisted
`clipboardPaste` preference. It is disabled by default and only reads the clipboard
on an explicit paste shortcut while the stream has input capture. Blocking overlays
and focus loss prevent paste; custom stream shortcuts take priority.

Paste uses native NVST Unicode text packets, preserving UTF-8 characters and line
breaks without depending on the remote keyboard layout. Each paste is limited to
64 KiB of UTF-8; empty, invalid, NUL-containing, oversized, or locally rejected input
shows a brief nonblocking notice instead of forwarding the paste shortcut or silently
truncating text. There is no automatic synchronization, image/file transfer, or
remote-to-local clipboard copying. Network acceptance is not a remote application
delivery acknowledgement; live GFN paste still needs account-backed validation.
Run `opennow-streamvideo-tests clipboardPasteRouting` for windowed/fullscreen
shortcut, Unicode, size-limit, overlay, and focus regression coverage.

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

**Settings → Input & controllers** includes independent left/right stick dead zones
(0–50%) and controller vibration intensity (0–100%) in both desktop and console mode.
These global preferences are saved and apply without restarting the session. Defaults
are 5% for both sticks. Existing saved preferences are preserved.
The radial filter suppresses resting drift and rescales the remaining travel, preserving
full axis and diagonal output. Set either stick to 0% to leave its dead zone to the game;
shell navigation retains its separate press/release thresholds.

Vibration follows the server's player slot through the same source selection as input,
including a selected physical device exposed as Player 1. It requires SDL rumble support
for the device and connection. Setting intensity to 0% disables it. Rumble stops when
input moves to the shell, focus is lost, the source changes, or the session ends; timed
effects also expire locally. Validate actual motor behavior with a real game and pad
over both USB and Bluetooth, including menus, focus loss, disconnects, and reconnects.

Run `ctest --test-dir build/opennow-qt -R '^opennow-controllertuning-tests$'`
for virtual-device dead-zone endpoints, independent live settings, motor routing,
intensity scaling, effect refresh/expiry, and ownership cleanup.

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
feedback and update discovery. The protocol-v7 native streamer is linked into
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
