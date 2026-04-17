# Development Guide

This guide covers the active OpenNOW app in [`opennow-stable/`](../opennow-stable), including the existing Electron desktop target and the new Capacitor Android target.

## Prerequisites

- Node.js 22 or newer
- npm
- Java 21 JDK
- Android SDK command-line tools or Android Studio
- `ANDROID_SDK_ROOT` set to your local Android SDK path when building the APK outside Android Studio
- A GeForce NOW account for end-to-end testing

## Getting Started

Install app dependencies inside the Electron workspace:

```bash
cd opennow-stable
npm install
```

You can then work either from the app directory or from the repository root.

From the repository root:

```bash
npm run dev
npm run typecheck
npm run build
npm run dist
```

Directly inside `opennow-stable/`:

```bash
npm run dev
npm run preview
npm run typecheck
npm run build
npm run build:web
npm run cap:sync:android
npm run cap:open:android
npm run dist
npm run dist:signed
```

GitHub Actions also builds a testable unsigned Android debug APK artifact in the `auto-build` workflow for pull requests and manual dispatch runs.

## Workspace Layout

The Android shell lives in [`opennow-stable/android/`](../opennow-stable/android), Capacitor config lives in [`opennow-stable/capacitor.config.ts`](../opennow-stable/capacitor.config.ts), and the renderer platform abstraction lives in [`opennow-stable/src/renderer/src/platform/`](../opennow-stable/src/renderer/src/platform/).


```text
opennow-stable/
├── src/
│   ├── main/           Electron main process
│   │   ├── gfn/        Auth, game catalogs, subscriptions, CloudMatch, signaling
│   │   └── services/   Cache and refresh helpers
│   ├── preload/        Safe API exposed to the renderer
│   ├── renderer/src/   React application
│   │   ├── components/ Screens, stream UI, settings, library, navigation
│   │   ├── gfn/        WebRTC client and input protocol
│   │   └── utils/      Diagnostics and UI helpers
│   └── shared/         Shared types, IPC channels, logging helpers
├── electron.vite.config.ts
├── package.json
└── tsconfig*.json
```

## Architecture

### Main process

The main process handles platform and system responsibilities:

- OAuth and session bootstrap
- Game catalog fetches and cache refresh
- CloudMatch session creation, polling, claiming, and stopping
- Signaling and low-level Electron integration
- Local media management for screenshots and recordings
- Persistent settings storage

Key entry point:

- [`opennow-stable/src/main/index.ts`](../opennow-stable/src/main/index.ts)

### Preload

The preload layer exposes a narrow IPC surface to the renderer with `contextBridge`.

Key entry point:

- [`opennow-stable/src/preload/index.ts`](../opennow-stable/src/preload/index.ts)

### Renderer

The renderer is a React app responsible for:

- Login and provider selection
- Browsing the catalog and public listings
- Managing stream launch state and session recovery
- Rendering the WebRTC stream
- Handling controller input, stats overlay, screenshots, recordings, and settings UI
- Choosing the active runtime implementation through `src/renderer/src/platform/`

Key entry points:

- [`opennow-stable/src/renderer/src/App.tsx`](../opennow-stable/src/renderer/src/App.tsx)
- [`opennow-stable/src/renderer/src/components/StreamView.tsx`](../opennow-stable/src/renderer/src/components/StreamView.tsx)
- [`opennow-stable/src/renderer/src/components/SettingsPage.tsx`](../opennow-stable/src/renderer/src/components/SettingsPage.tsx)

## Common Tasks

### Start the app in development

```bash
cd opennow-stable
npm run dev
```

### Run type checks

```bash
cd opennow-stable
npm run typecheck
```

### Build production bundles

```bash
cd opennow-stable
npm run build
```

### Package release artifacts locally

Unsigned packages:

```bash
cd opennow-stable
npm run dist
```

Signed packages, if your environment is configured for signing:

```bash
cd opennow-stable
npm run dist:signed
```

## CI And Releases

The repository includes two main GitHub Actions workflows:

- [`auto-build.yml`](../.github/workflows/auto-build.yml) builds pull requests and pushes to `main` and `dev`
- [`release.yml`](../.github/workflows/release.yml) packages and publishes tagged or manually-triggered releases

Current build matrix:

| Target | Output |
| --- | --- |
| Windows | NSIS installer, portable executable |
| macOS x64 | `dmg`, `zip` |
| macOS arm64 | `dmg`, `zip` |
| Linux x64 | `AppImage`, `deb` |
| Linux ARM64 | `AppImage`, `deb` |

Additional CI output:

| Target | Output |
| --- | --- |
| Android testing | Unsigned debug APK artifact uploaded from `auto-build` |

## Notes For Contributors

- The active app is the Electron client. If you see older references to previous implementations, prefer `opennow-stable/`.
- Root-level npm scripts are convenience wrappers around the `opennow-stable` workspace.
- Before opening a PR, run `npm run typecheck` and `npm run build`.

For contribution workflow details, see [`.github/CONTRIBUTING.md`](../.github/CONTRIBUTING.md).


## Android Workflow

Local Android toolchain prerequisites:

- Android platform `android-36`
- Android build-tools `36.0.0`
- Android platform-tools
- Accepted Android SDK licenses

If you are using the command-line SDK tools, install the same Android components that CI installs:

```bash
sdkmanager --install \
  "platform-tools" \
  "platforms;android-36" \
  "build-tools;36.0.0"
yes | sdkmanager --licenses
```

Build and sync web assets into the Android project:

```bash
cd opennow-stable
npm run cap:sync:android
```

Build a local test APK:

```bash
cd opennow-stable
npm run cap:sync:android
cd android
./gradlew assembleDebug
```

Open the Android project in Android Studio:

```bash
cd opennow-stable
npm run cap:open:android
```

Current Android support is limited to the core cloud-gaming path. Android login now follows the same localhost redirect contract as desktop (`http://localhost:<port>` for both authorize and token exchange), but the Android shell intercepts that navigation inside a native WebView instead of hosting a real localhost callback server. Desktop-specific features such as quit app, pointer-lock toggles, log export, cache deletion, show-in-folder actions, and screenshot save-as are intentionally gated or unavailable on Android in this pass.

For CI-based testing, use the APK artifact uploaded by the `auto-build` workflow. It is a debug/testing package and is not release-signed.
