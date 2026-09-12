# AGENTS.md

## Core Priorities

1. Performance first.
2. Reliability first.
3. Keep behavior predictable under load and during failures, including session restarts,
   reconnects, partial streams, device loss, and graphics-surface recreation.

If a tradeoff is required, choose correctness and robustness over short-term convenience.

## Active Product Scope

- The supported desktop product is the Qt 6 application in `opennow-qt/` backed by
  `native/opennow-core/` and `native/opennow-streamer/`.
- Interpret unqualified references to OpenNOW, the desktop app, the client, the stream view, or
  the streamer as referring to this Qt/native stack.
- Electron implementations, Node-based desktop shells, browser renderer processes, preload
  scripts, and Electron IPC are legacy or out of scope. Do not modify, restore, build, test, or
  copy architecture from them unless the user explicitly requests historical comparison or a
  migration task.
- Do not introduce Electron, an embedded web application, or a second desktop runtime to solve a
  Qt problem. JavaScript tooling may still be used for repository-only tasks such as localization
  validation; it must not become part of the desktop application's runtime architecture.

## Repository Layout

- `opennow-qt/` is the current OpenNOW desktop application. C++ shell/runtime integration lives
  in `src/`, Qt Quick UI lives in `qml/`, packaging assets live in `packaging/`, and Qt tests live
  in `tests/`.
- `native/opennow-core/` is the Rust application core. It owns settings, credentials, NVIDIA/GFN
  account and session orchestration, catalog services, CloudMatch, diagnostics, and updates.
- `native/opennow-streamer/` is the in-process native NVST streaming workspace. Its focused crates
  own protocol, transport, platform decode/audio/input, the engine, and the C FFI consumed by Qt.
- `locales/` contains localization sources and generated Crowdin output. See Localization before
  editing.
- `docs/` contains the protocol, migration, acceptance, and streamer-comparison references.
- `OpenNOW-Site/` is a separate repository when present; do not modify it from OpenNOW tasks unless
  explicitly requested.

### Qt C++ Module Map

All paths below are relative to `opennow-qt/`. Use `src/` as the include root and qualify
cross-module includes, for example `"streaming/NativeStreamRuntime.h"`. Keep existing owners
intact when splitting their implementation across translation units.

| Location | Ownership |
| --- | --- |
| `src/main.cpp` | Minimal entry point into `runApplication`. |
| `src/app/` | `ApplicationStartup` composes application-lifetime services and QML context bindings; `AppController` owns shell navigation and OS actions; `SingleInstance` forwards launches. |
| `src/core/` | `CoreClient` owns the versioned JSON process connection, request tracking, and restart handling. |
| `src/input/` | SDL controller discovery/routing and shell input-mode tracking. |
| `src/input/platform/` | Native Wayland pointer capture using Qt's display and surface. |
| `src/streaming/` | `NativeStreamRuntime` owns the bounded FFI wrapper. `StreamVideoItem.cpp` owns item lifecycle and properties; `StreamVideoItemInput.cpp` implements input/capture on that same item. |
| `src/streaming/rendering/` | The render callback contract, native GPU callback, scene-graph node, texture renderer, and Vulkan device integration. |
| `src/localization/` | Source-text/key translation and locale selection. |
| `src/media/` | Media thumbnail generation. |
| `src/diagnostics/` | Shared diagnostics path policy. |
| `src/acceptance/` | `AcceptanceSession` owns acceptance callbacks and warning state; smoke fixtures, smoke workloads, motion checks, and performance profiling live separately from production startup. |

`ApplicationStartup` must keep the QML engine and acceptance callbacks within the lifetime of
their controller, core, and native runtime dependencies. Acceptance-only setup belongs in
`src/acceptance/`, not back in the entry point. Rendering and input source splits do not create
new presenters, runtime instances, threads, or capture owners.

### Qt QML Module Map

The QML files remain one `OpenNOW` module with stable public type names. Folder boundaries
organize features; they do not create additional QML engines or desktop runtimes.

- `qml/Main.qml` composes desktop/console surfaces and application-level shortcuts.
- `qml/desktop/shell/` owns desktop composition, navigation, sidebar, and command palette.
- `qml/desktop/components/` owns shared desktop primitives and the `DesktopTokens` singleton.
- `qml/desktop/{home,library,store,auth,friends,updates}/` owns each desktop feature's UI.
- `qml/desktop/settings/` owns settings navigation and shared settings actions;
  `settings/pages/` contains the individual page components, and `settings/controls/` contains
  reusable settings controls. Pages declare their width, screen dependency, and nested page
  components explicitly rather than reaching into an enclosing component's IDs.
- `qml/desktop/stream/` owns the desktop stream screen, menus, statistics, and exit confirmation.
- `qml/screens/`, `qml/overlays/`, and `qml/components/` contain the console-oriented screens,
  shared overlays, and shared primitives. `qml/theme/` owns the shared `Theme` singleton.
- `qml/state/ShellStore.qml` is the public shell-state singleton and session orchestration owner.
  `state/catalog/CatalogState.qml` owns catalog/store paging and browse state;
  `state/catalog/ArtworkState.qml` owns artwork interests, resolution requests, and retry pacing;
  `state/settings/SettingsState.qml` owns preferences, capability choices, and console-mode
  persistence; `state/account/AccountServicesState.qml` owns subscription, region/ping, linked
  account, and storage-service state. These are singleton-owned instances with explicit
  dependencies, not new global singletons. Preserve the facade's property aliases and method
  contracts, and route feature responses to the owning instance without duplicating state.

Keep resource URLs stable when moving QML files. New QML types must be explicitly registered
in `cmake/QmlModule.cmake`, and singleton declarations must move with their source paths.
The translator resolves source text independently of the QML filename; preserve localized
copy and `qsTr` bindings when extracting components.

### Qt Build Ownership

`CMakeLists.txt` composes the build. `cmake/Sources.cmake` lists C++ sources and shares the
streaming source sets between the app and tests; `QmlModule.cmake` registers QML and its assets;
`Resources.cmake` bundles locales and shaders. `PlatformInput.cmake` owns generated Wayland
protocols, `NativeRuntime.cmake` owns Rust builds and runtime deployment, `WindowsRuntime.cmake`
owns Windows runtime dependencies, and `Packaging.cmake` owns installation and CPack.
`BuildMetadata.cmake` remains the version/architecture authority.

`cmake/Tests.cmake` registers native Qt tests and QML acceptance workloads. When moving an
implementation, update both build registrations and source-path fixtures in
`tests/tst_embeddedorchestration.cpp` without weakening their assertions. Keep test names and
CLI switches stable so the acceptance runbook and CI continue exercising the same contracts.

## Architecture and Ownership

- Qt/QML owns windows, focus, navigation, overlays, accessibility, fullscreen state, stream chrome,
  and composition of the native video item.
- `StreamVideoItem` owns the Qt-side stream presentation and keyboard/mouse capture boundary.
  Local shell shortcuts must be consumed before gameplay keys are forwarded.
- `ControllerInput` owns SDL3 controller discovery and decides whether the shell or active stream
  receives controller input. Transfer ownership with neutral snapshots so remote buttons cannot
  remain stuck.
- `NativeStreamRuntime` is the bounded C++ wrapper around the Rust FFI. Keep ownership, callback
  marshalling, shutdown, and graphics-thread requirements explicit.
- The native streamer owns NVST RTSPS setup, Mjolnir/ICE/DTLS/SCTP transport, decode, audio,
  recording, and typed gameplay input. It publishes opaque native frame handles to Qt; do not add
  a second presenter window or CPU-copy frame path to the normal embedded flow.
- `native/opennow-core` prepares one complete session context. Qt must not duplicate GFN request,
  authentication, endpoint, proxy, session-refresh, or error-parsing logic.

## Qt Desktop Rules

- Implement application UI in QML and keep operating-system, graphics, input, process, and FFI
  integration in narrowly owned C++ types. Do not reproduce native integration in QML.
- Keep the stream video item alive while local overlays are shown. F3 statistics, Ctrl+G menus,
  exit confirmations, and other stream chrome must compose above the same native video surface;
  they must not navigate away from the stream, recreate the session, or replace the presenter.
- Treat fullscreen transitions, window resizing, display scaling, and device-pixel-ratio changes as
  coordinate-space changes. Recompute pointer mapping and confinement from the current video
  viewport and native window geometry; never reuse stale pre-fullscreen dimensions.
- Local shortcuts are handled before gameplay forwarding. A consumed local shortcut must not leak
  to the remote session, while unconsumed gameplay keys such as Escape must reach the stream.
- Any action that ends a session or quits OpenNOW must use the Qt confirmation flow unless the
  shutdown is already unavoidable. Showing or dismissing a confirmation must not disturb media.
- Test behavior in windowed and fullscreen modes, with overlays both open and closed, whenever a
  change touches focus, pointer mapping, keyboard routing, video composition, or scene-graph state.

## Native Streamer Rules

- Treat sender-authored packet, frame, timestamp, and stream identifiers as protocol data. Preserve
  them end to end through transport, media queues, decode, feedback, diagnostics, and recording;
  do not replace them with locally synthesized counters.
- Keep network receive, depacketization, decode, audio, input, recording, and Qt presentation on
  explicit ownership/thread boundaries. Blocking codec work must not run on the Qt GUI or scene
  graph render thread.
- Queues must be bounded and observable. Define what happens on overflow, discontinuity, missing
  references, late frames, decoder drain, graphics-device loss, and surface recreation. Recovery
  must request a valid reference frame where required and must not silently present corrupt data.
- Match negotiated codec, bit depth, chroma, color range, color space, and HDR metadata across
  NVST, decoder output, texture import, and Qt presentation. Do not infer one of these values from
  resolution or codec name alone.
- Protocol feedback such as frame acknowledgements, loss reports, pacing, QoS, and keyframe
  requests must use negotiated or sender-provided values and documented wire formats. Validate
  changes against captured diagnostics and focused protocol tests rather than timing guesses.
- Audio continuing does not prove that video transport or decode is healthy. Diagnostics must make
  the receive, assembly, submission, decode, presentation, and feedback stages distinguishable.
- Prefer opaque GPU surfaces and zero-copy presentation. Any CPU-copy or fallback presenter must be
  explicit, bounded, measurable, and outside the normal embedded streaming path.

## Process and Contract Boundaries

- `CoreClient` communicates with the Rust core using the versioned JSON protocol documented in
  `docs/core-protocol.md`.
- `NativeStreamRuntime` communicates with `opennow-streamer-ffi` through the versioned C ABI in
  `native/opennow-streamer/crates/opennow-streamer-ffi/include/`.
- Keep all values crossing either boundary serializable, bounded, and explicitly typed. When a
  protocol or ABI changes, update every producer, consumer, fixture, and version check together.
- Do not expose platform handles or native graphics objects to QML. Keep them inside C++/Rust
  ownership boundaries.
- Preserve provider/alliance behavior, stable device IDs, persisted account/session compatibility,
  and recovery semantics when refactoring.

## Maintainability

Long-term maintainability is a core priority. Before adding functionality, look for shared logic
that belongs in a focused owner module. Duplicate logic across QML, C++, and Rust is a code smell.

- Refactors should reduce ownership ambiguity and keep behavior equivalent unless a behavior change
  is requested.
- Prefer small typed helpers over broad utility modules.
- Keep platform-specific decode, input, graphics, and audio code in the corresponding platform
  crate or a narrowly guarded C++ implementation.
- Never let an overlay, focus transition, or local shortcut stop or restart the media transport
  unless the user explicitly ends the session.
- Release pressed keys, buttons, controller state, pointer confinement, and native resources on
  every focus-loss, overlay, shutdown, and failure path.

## Localization

Crowdin owns generated translations. When changing localized copy, edit only `locales/en.json` as
the source language file. Do not manually edit other `locales/*.json` files; they are generated by
Crowdin and should change only through Crowdin synchronization pull requests.

## Build and Checks

- Configure: `cmake -S opennow-qt -B build/opennow-qt -DCMAKE_BUILD_TYPE=Debug`
- Build: `cmake --build build/opennow-qt`
- Test: `ctest --test-dir build/opennow-qt --output-on-failure`
- Run the narrowest relevant Qt test first, then build and run the complete Qt test suite when
  practical.
- For Rust core changes, run the affected crate tests from its Cargo workspace.
- For native streamer changes, run the affected crate test first, then
  `cargo test --manifest-path native/opennow-streamer/Cargo.toml --workspace` when practical.
- For localization changes, run `npm run locales:check`.
- Do not claim completion if a relevant acceptance check fails. Report the failing command and
  failure point.

## Development Runtime

- Qt 6.8+, CMake 3.24+, a C++20 toolchain, SDL3, Cargo, and the platform media dependencies are
  required.
- On Windows the development executable is normally `build/opennow-qt/OpenNOW.exe`; on Linux it is
  normally `build/opennow-qt/opennow-qt`.
- Build before restarting the development application. Stop only OpenNOW processes from this
  workspace, then launch the freshly built Qt executable from `build/opennow-qt` so bundled runtime
  libraries and the Rust core resolve predictably.
- Full login and gameplay validation requires a real NVIDIA/GFN account. Without credentials, use
  the Qt unit, smoke, screenshot, and performance harnesses documented in `opennow-qt/README.md`
  and `docs/qt-acceptance.md`.
