# AGENTS.md

## Project Scope

- Treat `opennow-stable/` as the active desktop client. It is an Electron, React, and TypeScript application.
- Treat `native/opennow-streamer/` as native Rust streaming infrastructure. Keep performance-sensitive streaming logic predictable and explicit.
- Keep repository-level scripts in the root `package.json` aligned with the real implementation in `opennow-stable/`.

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
