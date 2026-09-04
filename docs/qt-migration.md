# OpenNOW Qt migration

This document is the durable implementation checklist for replacing the Electron
application shell with Qt Quick/QML. Electron remains the reference and fallback
until every removal gate at the end of this document passes.

## Target architecture

```text
Qt Quick/QML shell
  -> thin Qt/C++ bridge -> linked Rust NVST runtime
  -> versioned OpenNOW core RPC -> Rust application services
```

The QML layer owns presentation, motion, spatial focus, responsive layout and
accessible interaction. It must not own GFN protocol details, credentials,
filesystem access, native process management or stream transport.

The Rust application core remains out of process and owns authentication, GFN
orchestration, settings, storage, updates, diagnostics and platform services.
The NVST media runtime is a shared library loaded into the Qt process. Qt owns
the D3D11, Metal or Vulkan graphics device and scene-graph presentation; Rust
decodes into native GPU frames and records conversion work on Qt's command
stream without creating another application window or presenter process.

## Phase checklist

### 0. Baseline and proof

- [x] Select Qt Quick/QML with a Rust application core.
- [x] Confirm Qt Quick, Quick Controls and SDL3 are available in development.
- [x] Inventory the current Paper `OpenNOW Socket / OpenNOW V3` page: 24 V3 app-state artboards, three teaser compositions and one site composition (28 total).
- [x] Inventory the Electron renderer, main-process and native-streamer boundaries.
- [x] Record a local Linux release checkpoint: five first-frame samples of 1,334/1,291/1,315/1,310/1,303 ms (1,310 ms median) and a 26 MiB application payload before Qt runtime deployment and bundled-FFmpeg expansion. The observed GPU-less-container RSS is retained as diagnostic data, not a representative performance result.
- [ ] Prove a real native stream with a composited QML guide overlay on every OS.
- [ ] Record 1080p and 4K focus/page/popup frame timing on the baseline iGPU.
  The native `--performance-report` workload and hardware-only guard are implemented; the two
  representative-hardware reports remain to be captured.

### 1. Native shell foundation

- [x] Add a Qt 6.8+ CMake project without Electron or Node dependencies.
- [x] Add a single-instance application entry point and structured Qt logging.
- [x] Add Paper-derived color, type, spacing, radius and motion tokens.
- [x] Add application routing and overlay state with deterministic back behavior.
- [x] Add SDL3 controller discovery, hot-plug, buttons, sticks and repeat behavior.
- [x] Add OS single-instance forwarding; direct-launch URL delivery remains in Phase 5.
- [x] Add bounded RPC breadcrumbs, credential redaction and diagnostic export.
- [x] Add automated warning-fatal offscreen QML startup and screenshot smoke tests.

### 2. Controller-first component system

- [x] Add reusable glass, focus, glyph, navigation and game-tile primitives.
- [x] Complete focus scopes, modal traps and per-route focus restoration.
- [x] Complete grid edge/wrap policy and scroll-to-focus for every collection.
- [x] Add last-input-modality behavior for controller, keyboard and pointer.
- [x] Add reduced-motion support and warning-fatal smoke coverage.
- [x] Add narration/accessibility names, roles, values and state announcements.
- [x] Validate held-input repeat, analog hysteresis and controller reconnect with an SDL3 virtual gamepad integration test.

### 3. Paper V3 screen parity

- [x] V3 · 00 Components.
- [x] V3 · 01 Home.
- [x] V3 · 02 Library.
- [x] V3 · 03 Game detail.
- [x] V3 · 04 Inserting / session preparation.
- [x] V3 · 05a Guide overlay · Session.
- [x] V3 · 05b Guide overlay · Controls.
- [x] V3 · 05c Guide overlay · Media.
- [x] V3 · 05d Guide overlay · Shortcuts.
- [x] V3 · 06a Settings · Account.
- [x] V3 · 06b Settings · Streaming.
- [x] V3 · 06c Settings · Video & display, including open dropdown.
- [x] V3 · 06d Settings · Input & controllers.
- [x] V3 · 06e Settings · Network.
- [x] V3 · 06f Settings · Themes.
- [x] V3 · 06g Settings · Advanced, including backend dropdown.
- [x] V3 · 07 Sign in and device-code refresh.
- [x] V3 · 08 Friends dropdown.
- [x] V3 · 09 Friend actions and co-op invitation.
- [x] V3 · 10 Joining session / controller 2.
- [x] V3 · 11 Quick settings.
- [x] V3 · 12 Store.
- [x] V3 · 13 Theme store, including filters, live preview and persisted built-in theme selection.
- [x] Teaser/demo compositions use the same production components.

### 4. Versioned shell/core boundary

- [x] Capture all `OpenNowApi` requests, responses and events in schemas.
- [x] Add protocol version negotiation and incompatible-version diagnostics.
- [x] Add request IDs, cancellation, deadlines and bounded event queues.
- [x] Add typed structured errors without leaking credentials or raw responses.
- [x] Add golden contract fixtures for all 109 current API operations/events.
- [x] Replace the required Electron services directly with Rust owners; no Node adapter is used.

### 5. Product flows

- [x] Device login, saved accounts, persisted PIN lockout, switching, logout and secure storage.
- [x] Live provider discovery and NVIDIA device-code challenge/poll/complete/cancel.
- [x] Restart-safe active-session storage through the OS credential store, with explicit memory-only degradation.
- [x] Access-token, refresh-token and client-token refresh ordering with identity checks.
- [x] Subscription and dynamic streaming-region discovery.
- [x] Account connections and persistent-storage management.
- [x] Home, library, live public/account catalogs, search, platform/genre filters and sorting.
- [x] Live public catalog with bounded in-process caching and real artwork.
- [x] Authenticated paginated account-library query and Electron-compatible game mapping.
- [x] Live game detail, selected platform/store launch identity and direct CloudMatch launch.
- [x] Session create, queue polling and stop with stable client/device identity.
- [x] Ads, session conflict handling, remote active-session claim and bounded reconnect.
- [x] Honest provider social capability surface and live second-controller joining; NVIDIA third-party friends/invites are not exposed by the provider API.
- [x] Every settings section, persistence, normalization and reset.
- [x] Entitlement-driven aspect ratio, resolution and frame-rate choices, persisted favorites,
  game language/keyboard layout, gamepad-friendly launch mode and supported in-game settings persistence.
- [x] Implement signed update discovery, channels, highlights, automatic checks, trusted external URLs, Ed25519 verification, atomic install and rollback. Production signing credentials and release assets remain a Phase 8 release gate.
- [x] Screenshots, media listing, generated recording thumbnails and reveal-in-folder.
- [x] Record the negotiated H.264, H.265 or AV1 source stream and Opus audio into an atomic Matroska file without re-encoding; malformed configuration, discontinuity and bounded-queue overflow fail closed.
- [x] Discord presence, telemetry consent, feedback and bug reports.

### 6. Native streaming

- [x] Replace Electron-specific surface ownership with an in-process GPU frame contract. The Qt
  render thread lends QRhi native objects to the in-process Rust library, acquires opaque frame tokens,
  records conversion/synchronization into Qt's command buffer and samples the imported texture in
  `StreamVideoItem`.
- [x] Present native decoder frames inside the Qt scene graph on Windows, macOS, X11 and Wayland.
  QML menus, stats, reconnect and error states compose above live video without child HWNDs, paired
  windows or presenter hide/show ordering. Cross-OS live GPU interop remains an acceptance gate.
- [x] Replace the streamer subprocess with a bounded C FFI linked into `opennow-qt`; preserve the
  protocol-v5 engine contract internally without packaging a helper executable.
- [x] Preserve hardware decode selection and safe fallback behavior for every negotiable codec. NVST
  H.264/H.265/AV1 profiles, strict automatic/hardware/software selection, prelaunch capability
  probing, Windows class-separated Media Foundation hardware and system-software probing/fallback,
  Linux bundled-FFmpeg fallback, and independently probed macOS VideoToolbox H.264/HEVC/AV1 are
  implemented. Windows HEVC/AV1 software availability follows registered D3D11-aware software MFTs
  instead of being mixed into the hardware result. macOS guarantees H.264 through OpenH264 fallback;
  HEVC/AV1 are offered only when the corresponding VideoToolbox hardware probe succeeds, and
  failures disable that codec instead of silently negotiating an unavailable software path.
- [x] Route keyboard, mouse and up to four gamepads directly to the streamer while shell overlays are closed.
- [x] Transfer input ownership atomically to the shell for overlays and send neutral controller state on pause.
- [x] Integrate stream statistics, next-session bitrate, recording and Cloud G-Sync quick controls.
  The configurable stats shortcut is forwarded as `shortcut-action: toggle-stats`; Qt owns the
  overlay and renders core telemetry instead of asking the native presenter to draw it.
- [x] Migrate persisted microphone modes to disabled. Microphone capture and upstream audio are not
  part of the NVST runtime, and legacy values cannot select another transport.
- [x] Apply the seven active native shortcuts, including pointer lock, recording, screenshot, stop
  and real four-minute anti-AFK F13 pulses. Stats and fullscreen are forwarded to Qt; pointer lock
  remains native. The removed microphone shortcut key remains accepted only as persisted settings
  data so upgrades do not fail to load.
- [ ] Validate HDR, high-refresh, VRR, resize, fullscreen and display migration.
- [ ] Validate screenshots and source-stream recording with an authorized live session on each supported OS.

### 7. Rust application core

- [x] Port settings and persistence with compatibility fixtures.
- [x] Port active-session credentials using DPAPI, Keychain and Secret Service/libsecret.
- [x] Port proxy-aware HTTP, client identity, endpoint and error handling.
- [x] Port authentication, catalog, subscriptions and region discovery.
- [x] Port account connections.
- [x] Port fresh-session lifecycle and CloudMatch coordination for NVST-only media sessions.
- [x] Port active-session claim, recovery and native-owned NVST RTSP negotiation.
- [x] Port updater discovery/channels, media library, diagnostics, Discord and opt-in telemetry.
- [x] No temporary Node service was introduced.

### 8. Packaging and migration

- [ ] Windows x64 and ARM64 signed installers and portable packages. Unsigned x64 and cross-compiled ARM64 Qt ZIP validation jobs are implemented; certificate-backed installer signing remains a release gate.
- [ ] macOS Intel and Apple Silicon signed/notarized DMG and ZIP packages. Separate Apple-Silicon-native ARM64 and Rosetta-validated Intel Qt package jobs are implemented; Developer ID signing and notarization remain release gates.
- [ ] Linux x64/ARM64 AppImage release jobs. Native x64 and ARM64 Qt AppImage/DEB artifact jobs use architecture-matched Qt/SDL/Rust toolchains and checksum-pinned linuxdeploy binaries; the ARM64 job still needs to pass on the release runner before this gate is closed.
- [x] Add a reviewer-protected Qt production-candidate workflow that propagates one release version,
  Authenticode-signs Windows binaries/installers, Developer-ID signs and notarizes macOS apps and
  disks, builds native Linux packages, generates matching Ed25519 manifests, and inventories the
  complete immutable artifact set. Supplying credentials and executing it remain release gates.
- [x] Test signed update metadata parsing, stable/nightly channel compatibility, signature rejection, atomic replacement and executable rollback with fixtures.
- [ ] Publish production-signed update metadata and packages using an offline-protected Ed25519 signing key.
- [x] Preserve settings, accounts, media, cache and direct-launch associations. Electron settings are migrated without destroying unknown rollback fields, legacy account documents import into the OS credential store without deleting their source, media paths are shared, old cache data is left intact, and platform URL registration is packaged.
- [x] Package required dynamic Qt libraries and all third-party notices through Qt deployment scripts, generated exact Rust license notices and the Linux AppImage deployment path.
- [x] Run localization extraction and validate every supported locale. The runtime loads every catalog, reacts to language changes, and falls back to English for untranslated new copy; translated-copy review remains a Crowdin/release QA task.

### 9. Acceptance and Electron removal gates

- [x] Every Paper V3 state is reachable through the production router and has controller focus; warning-fatal route/overlay smoke coverage renders all 28 Paper compositions and the additional functional routes.
- [x] No focus loss across route, modal, reconnect, async refresh or error states in the automated shell suite; every smoke state must retain a visible, enabled active focus item.
- [ ] Performance budgets pass on representative low-end hardware.
- [x] Ship a fail-closed acceptance verifier for live evidence, 1080p/4K reports, manual hardware
  attestations, required platform package types, package hashes, signing and update verification.
- [x] Offline, partial-stream, core-restart and native-runtime failure tests pass with bounded queues, typed failures, graceful shutdown and bounded recovery.
- [ ] Authorized live sessions pass on every supported OS/window-system pair.
- [x] Upgrade, downgrade and rollback fixtures preserve user data: unknown Electron fields survive Qt saves, imported credentials leave the Electron source untouched, and failed AppImage replacement restores the previous executable.
- [ ] Qt release has completed a staged rollout with diagnostics monitored.
- [ ] Delete Electron main, preload and renderer code.
- [ ] Delete Electron dependencies, builder configuration and CI jobs.
- [ ] Make the Qt shell the only supported desktop entry point.

## Non-negotiable removal rule

Electron is removed only after every acceptance item above passes. A visually
complete shell is not parity: auth, networking, streaming, recovery, updates,
storage migration and controller-only operation are all removal gates.

## Current measured checkpoint

The 2026-08-28 Linux checkpoint has a warning-fatal suite covering 48 shell,
controller, localization, native recording, route, overlay, performance-harness and lifecycle
tests; the complete four-way run takes about 19 seconds after making unused Qt Multimedia thumbnail
decoding lazy. The Rust application core has 74 unit tests, the acceptance verifier has four, the
update-manifest generator has two, and the legacy boundary has three machine-readable contract tests
(83 total).
The native streamer workspace's platform/core/protocol/transport tests and doc tests pass on the
development host. The system-FFmpeg Linux configuration retains one intentionally ignored
live-hardware probe, including linked H.264/H.265/AV1 software-decoder verification. Both Rust
workspaces pass clippy with warnings denied.

The macOS VideoToolbox crate passes `x86_64-apple-darwin` and `aarch64-apple-darwin` Rust type
checks, including the CoreMedia H.264/HEVC/AV1 format descriptions. The wrapper also contains
codec-specific tracking, main-thread configuration commands, reconfiguration and failure
handling. Full wrapper linking and execution require the Apple SDK and therefore still run only
on the macOS CI/hardware gate.

The Qt target links `opennow-streamer-ffi` as an in-process Rust shared library and packages no
`opennow-streamer` executable. `opennow-core` remains a separate account/session service, but it
only prepares the normalized NVST launch context; Qt sends media lifecycle and input commands to
the linked runtime. The native launch context carries exactly the CloudMatch codec configured for
the active decoder, and explicit HEVC/AV1 sessions remain available only where the linked backend
reports them.

These are development-host checkpoints, not live-account or representative-GPU
acceptance results. Production update keys/assets, signed/notarized multi-arch
packages, authorized multi-OS streams, representative hardware frame timing and
a monitored staged rollout remain external removal gates. Electron therefore
remains intact.

The Diagnostics screen now exports both a human-readable redacted report and a direct
machine-readable live-acceptance manifest. The latter hashes the screenshot, recording and
thumbnail, records ten-minute NVST streaming, first-frame, guide/input ownership, surface,
recording, recovery and error checks, and excludes session/process identifiers, endpoints,
executable paths and local media paths. The packaged `opennow-acceptance-verify` tool combines that
manifest with both hardware performance reports, the explicit manual-attestation template and all
required platform packages; it fails closed on any false/mismatched gate and emits a path-free
verification result.

The NVST-only core no longer contains its former browser offer/answer, trickle-ICE, RTP media,
data-channel input or microphone fallback. Persisted `transportMode` remains part of the settings contract
for rollback compatibility, but every legacy value normalizes to `nvst` before session creation or
runtime launch. NVIDIA still requires some protocol labels whose names contain `WEBRTC`: device
authorization and browser-style region discovery retain the `nv-client-streamer: WEBRTC` identity.
The runtime also retains Tungstenite/rustls for NVIDIA's RTSPS-over-WebSocket negotiation and
str0m-backed ICE/DTLS/SCTP bundle, input and control structures because NVST audio, input and RTCP
use that encrypted bundle. Those dependencies are NVST wire compatibility, not a second media
transport.

Qt owns the graphics API and scene graph: D3D11 on Windows, Metal on macOS and Vulkan on Linux.
Decoded frames stay in-process and are converted into frame-slot RGBA textures on Qt's device and
command stream before `StreamVideoItem` samples them. QML overlays therefore remain in the same
scene graph, and embedded mode creates no presenter window, child window, swapchain or platform
surface. Linux retains a synchronized CPU-plane-to-GPU upload fallback for unsupported direct
imports; this is not described as zero-copy. Live device, high-refresh, display-transition and
device-loss acceptance remains required separately on each target operating system.

The exact live-account, hardware, signing and rollout procedure is maintained in
[`docs/qt-acceptance.md`](qt-acceptance.md). It defines which artifacts constitute proof, so an
offscreen pass or an operator-only checklist cannot accidentally close a release gate.
The protected production-candidate workflow and required secret boundaries are documented in
[`docs/qt-release-candidate.md`](qt-release-candidate.md).
