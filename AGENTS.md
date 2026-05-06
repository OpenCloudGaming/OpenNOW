# AGENTS.md

## Project Scope

- Treat `opennow-stable/` as the active desktop client. It is an Electron, React, and TypeScript application.
- Treat `native/opennow-streamer/` as native Rust streaming infrastructure. Keep performance-sensitive streaming logic predictable and explicit.
- Keep repository-level scripts in the root `package.json` aligned with the real implementation in `opennow-stable/`.

## Repository Map

- `AGENTS.md`: repository-wide instructions for AI agents and contributors.
- `README.md`: user-facing project overview, setup, architecture summary, and contribution entry points.
- `package.json`: root workspace shim. Scripts proxy into `opennow-stable/`.
- `.github/`: GitHub workflows, issue templates, PR template, Copilot instructions, and contributing docs.
- `.agents/skills/`: Capy/project skills for specialized agent workflows.
- `docs/`: project documentation, including development notes, streamer investigation, and GStreamer bundling details.
- `opennow-stable/`: active Electron desktop client.
  - `package.json`: real app scripts, dependencies, Electron Builder config, lint/typecheck/test commands.
  - `electron.vite.config.ts`: Electron Vite build configuration and path aliases.
  - `tsconfig.json`: renderer/shared TypeScript configuration.
  - `tsconfig.node.json`: main/preload/shared TypeScript configuration.
  - `scripts/`: build, dev, release-version, native-streamer, and test runner scripts.
  - `src/main/`: Electron main process. Owns app lifecycle, windows, IPC handlers, auth/session orchestration, settings, updates, Discord RPC, Cloud G-Sync, and native streamer startup.
    - `src/main/index.ts`: central Electron bootstrap, IPC registration, window/session lifecycle, and app orchestration.
    - `src/main/gfn/`: GeForce NOW-facing services for auth, CloudMatch, games, proxy fetch/url handling, signaling, subscriptions, error codes, and shared main-process GFN types.
    - `src/main/services/`: cache manager, cache event bus, and refresh scheduler services.
    - `src/main/nativeStreamer/manager.ts`: native streamer process discovery, launch, lifecycle, and IPC integration.
  - `src/preload/index.ts`: Electron `contextBridge` layer. Exposes the safe `window.openNow` API to the renderer.
  - `src/shared/`: shared contracts and utilities used across main, preload, and renderer.
    - `gfn.ts`: core request/response types and the `OpenNowApi` interface.
    - `ipc.ts`: canonical IPC channel names.
    - `logger.ts`: shared logging contracts.
    - `sessionError.ts`, `networkError.ts`: structured error helpers.
    - `nativeStreamer.ts`, `mediaPlayback.ts`, `cloudGsync.ts`: shared domain contracts.
  - `src/renderer/`: React renderer application.
    - `src/renderer/index.html`: renderer HTML entry.
    - `src/renderer/src/main.tsx`: React bootstrap.
    - `src/renderer/src/App.tsx`: top-level UI, session state, launch flow, streaming orchestration, and major app-level handlers.
    - `src/renderer/src/styles.css`: global renderer styles.
    - `src/renderer/src/vite-env.d.ts`: renderer global declarations, including `window.openNow`.
    - `src/renderer/src/components/`: main UI screens and reusable components such as login, home, library, settings, stream view, queue UI, navigation, overlays, and cards.
    - `src/renderer/src/components/controllerMode/`: controller-first UI, controller library layout, in-stream shell, controller navigation, and media/controller interaction components.
    - `src/renderer/src/gfn/`: renderer streaming client code, including WebRTC, SDP helpers, input protocol, and microphone management.
    - `src/renderer/src/hooks/`: React hooks.
    - `src/renderer/src/lib/`: renderer-side domain helpers and stores.
    - `src/renderer/src/utils/`: renderer utilities for stream diagnostics, stream health, quality presets, controller UI sound/navigation persistence, elapsed time, and playtime.
- `native/opennow-streamer/`: Rust native streamer.
  - `Cargo.toml`, `Cargo.lock`: Rust package metadata and lockfile.
  - `README.md`: native streamer notes.
  - `src/main.rs`: native streamer entry point.
  - `src/backend.rs`, `src/gstreamer_backend.rs`: backend abstraction and GStreamer implementation.
  - `src/protocol.rs`, `src/input.rs`, `src/sdp.rs`: protocol, input, and SDP handling.
  - `bin/`: output location for built native streamer binaries.
- `ios/`: iOS-related project files and widgets. Keep separate from the active Electron desktop client unless the change explicitly targets iOS.
- `flake.nix`, `flake.lock`: Nix development environment configuration.
- `logo.png`, `img.png`, `OpenNOW_Settings.png`: repository image assets.

## Core Priorities

1. Performance first.
2. Stability first.
3. Reliability under load and during failures, including session restarts, reconnects, partial streams, network errors, and interrupted IPC flows.
4. Long-term maintainability so the codebase stays understandable as it grows.

If a tradeoff is required, choose correctness, robustness, and predictable behavior over short-term convenience.

## Maintainability Standards

- Keep changes professional, focused, and consistent with the existing architecture.
- Prefer shared, well-named modules over duplicating local logic across files.
- Before adding new functionality, check whether existing shared logic, types, utilities, services, or components can be extended cleanly.
- Do not take shortcuts that make future maintenance harder. Refactor surrounding code when that is the safer long-term path.
- Keep cross-process contracts explicit. When changing IPC or shared API shapes, update the shared types first and then wire main, preload, and renderer code to match.
- Keep behavior easy to reason about: avoid hidden global state, implicit side effects, and broad catch-all error handling.

## TypeScript Standards

- Do not use `any` in TypeScript unless there is no practical typed alternative.
- Prefer `unknown`, concrete interfaces, discriminated unions, generics, and narrow type guards over `any`.
- If `any` is unavoidable for an external API or browser/Electron edge case, isolate it to the smallest possible scope and convert it back to a typed value immediately.
- Do not add TypeScript complexity where it is not needed. Use the simplest type that accurately describes the data and keeps call sites safe.
- Keep shared TypeScript contracts in `opennow-stable/src/shared/` when values cross process boundaries.

## Performance and Stability Guidelines

- Avoid regressions in startup time, stream responsiveness, input latency, memory usage, and reconnection behavior.
- Keep hot paths allocation-conscious and avoid unnecessary React re-renders in streaming or controller navigation flows.
- Make failure states explicit and recoverable. Prefer typed errors and structured status values over stringly typed checks.
- Preserve useful diagnostics through the existing logging and export paths instead of adding one-off logging channels.
