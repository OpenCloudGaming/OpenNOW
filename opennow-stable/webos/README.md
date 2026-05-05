# OpenNOW webOS target

This folder contains the webOS packaging metadata and bundled JavaScript service for the TV build.

## Build

```sh
npm run webos:build
```

The staged app is written to `dist-webos/`. It includes:

- the React renderer built with Vite and relative assets
- `appinfo.json` with the package version injected from `package.json`
- app icons copied from the repo logo
- `services/com.zortos.opennow.stable.service/`

## Package

Install the LG webOS CLI, then run:

```sh
npm run webos:package
```

The `.ipk` is written under `dist-release/webos/`.

## Runtime shape

Desktop remains Electron-based. The webOS build sets `VITE_OPENNOW_RUNTIME=webos`, which installs `src/renderer/src/platform/webos/runtime.ts` as the `window.openNow` provider instead of Electron preload.

The webOS service is bundled from `webos/service-src/service.ts`. It reuses the desktop GFN modules for auth, catalog/library loading, subscription lookup, CloudMatch session create/poll/claim/stop/ad reporting, region pings, and signaling. TV defaults enable controller mode, auto-load the controller library, use H.264/8-bit 4:2:0 by default, and keep desktop Electron behavior unchanged.

NVIDIA sign-in launches the TV browser through Application Manager and waits for the same localhost OAuth callback used by desktop. If a TV suspends OpenNOW while the browser is foregrounded, relaunch OpenNOW after login; the service persists the restored auth state.
