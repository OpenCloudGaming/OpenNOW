# Copilot instructions for OpenNOW

## Project scope
- The active desktop client is in `opennow-stable/` (Electron + React + TypeScript).
- The repository root `package.json` is a workspace shim: root scripts proxy to `opennow-stable` via `npm --prefix opennow-stable`.

## Build, check, and packaging commands

### From repository root
```bash
npm run dev
npm run typecheck
npm run build
npm run dist
npm run dist:signed
```

### From `opennow-stable/`
```bash
npm install
npm run dev
npm run typecheck
npm run build
npm run dist
npm run dist:signed
```

### Tests and linting
- There is currently no test script or lint script in this repository.
- There is no single-test command configured yet.
- PR guidance in this repo expects `typecheck` and `build` to pass locally.

## High-level architecture
- Electron app with three boundaries:
  - **Main process** (`src/main/`): auth/session lifecycle, CloudMatch API calls, signaling setup, settings persistence, IPC handlers.
  - **Preload** (`src/preload/index.ts`): typed `contextBridge` surface that exposes the safe API as `window.openNow`.
  - **Renderer** (`src/renderer/src/`): React UI and WebRTC client; stream lifecycle is orchestrated in `App.tsx`.
- Shared cross-process contracts live in `src/shared/`:
  - `gfn.ts` defines request/response types and the `OpenNowApi` interface used by preload and renderer.
  - `ipc.ts` defines canonical IPC channel names used by both preload and main.
- Streaming flow (big picture):
  1. Renderer calls `window.openNow` API.
  2. Main IPC handlers in `src/main/index.ts` delegate to `src/main/gfn/*` services.
  3. CloudMatch/session APIs return signaling/session data.
  4. Main signaling client emits events back to renderer.
  5. Renderer WebRTC client establishes media/data channels and drives stream UI/state.

## Key conventions
- Keep the **shared contract first**: when adding/changing API shapes, update `src/shared/gfn.ts` and `src/shared/ipc.ts`, then wire both preload and main handlers.
- Keep alias usage consistent: `@shared/*` is defined in both TS configs and `electron.vite.config.ts`; new shared modules should follow this import pattern.
- Renderer should use `window.openNow` only (declared in `src/renderer/src/vite-env.d.ts`); avoid importing Electron APIs directly in renderer code.
- Session/auth token handling should stay centralized in main (`AuthService.resolveJwtToken` / `ensureValidSessionWithStatus`) so renderer-side cached tokens do not bypass refresh logic.
- CloudMatch errors are normalized with `SessionError` (`src/main/gfn/errorCodes.ts`) and rethrown via JSON in IPC handlers; renderer launch/error UX depends on `title`, `description`, and `gfnErrorCode`.
- Logging is intentionally capturable and exportable from both processes (`@shared/logger`, `logs:export` IPC); avoid bypassing this path for diagnostics features.
