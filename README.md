# OpenNOW

OpenNOW is a native macOS client for cloud gaming: a fast, polished desktop app for signing in, browsing your library, tuning stream settings, and launching games into a full-screen stream without fighting the browser.

It is built directly on AppKit with Objective-C++ and a native libwebrtc-based streaming stack, so the experience feels like a Mac app first and a workaround second.

## Why OpenNOW

- Native macOS UI instead of a wrapped web view.
- Persistent sessions and account switching for multi-account users.
- Fast access to your catalog and library from a desktop app.
- Stream controls, video/audio/input preferences, and interface settings in one place.
- Native WebRTC rendering and input delivery for a real streaming client experience.

## Features

### Sign in once, keep playing

- Browser-based OAuth sign-in using the system browser.
- Optional local session persistence when stay-logged-in is enabled.
- Saved account switching from within the app.
- Automatic session refresh when a token can be renewed.

### Browse and launch your library

- Loads catalog and library data from the backend service.
- Supports browsing, filtering, and sorting games.
- Shows account status and membership context in the interface.
- Launches the selected game directly into a cloud session.

### Native streaming

- Stream playback is rendered in a native macOS view.
- WebRTC handles negotiation, media rendering, and recovery.
- Input is delivered through a native input protocol.
- The app tracks the active display and chooses a sensible stream resolution.
- Stream lifecycle handling covers teardown, relaunch, and recovery paths.

### Practical streaming controls

- Stream preferences for region, codec, bitrate, and related toggles.
- Dedicated settings sections for stream, video, audio, input, interface, about, and thanks.
- Recovery-aware launch flow when a previous session is still winding down.

## What It Feels Like

OpenNOW is designed for users who want the convenience of cloud gaming without the friction of a browser tab. You sign in, pick a game, and stay inside a focused native app that remembers your accounts, respects your stream preferences, and keeps the transition from catalog to gameplay as smooth as possible.

## Requirements

- macOS with AppKit/Cocoa support.
- `clang++` with C++20 and Objective-C ARC support.
- Apple Command Line Tools or Xcode command line tooling.
- A macOS `WebRTC.framework` or `WebRTC.xcframework` available at `third_party/webrtc-official`, or a custom path supplied with `WEBRTC_FRAMEWORK_DIR`.
- Optional Qt migration track: Qt 6.5 or newer with Widgets, Network, and WebSockets modules plus CMake 3.21 or newer.

## Build

```sh
make
```

The build output is written to `build/OpenNOW`.

## Run

```sh
make run
```

This builds the app if needed and launches the binary from the local build directory.

## Clean

```sh
make clean
```

Removes the `build/` directory and all compiled artifacts.

## Qt 6 Migration Track

The Qt port lives side-by-side under `qt/` while the AppKit app remains the shipping build.

Configure and build the Qt app:

```sh
make qt-build
```

Run the Qt app:

```sh
make qt-run
```

The Qt target currently provides the cross-platform application shell, persistent window state, and the primary navigation surfaces that match the existing authentication, library, store, settings, and streaming flow. Service and WebRTC parity can be ported into this target incrementally without disrupting the native macOS build.

## Core Flow

1. The app starts in a native `NSApplication` entry point and installs `AppDelegate`.
2. The delegate restores window state, including fullscreen preference.
3. If a valid saved session exists, the app opens directly into the catalog.
4. If a saved session is present but expired, the app attempts a refresh before falling back to sign-in.
5. If no usable session exists, the app shows the email entry screen and starts browser-based OAuth.
6. After sign-in, the catalog loads the library and public game data.
7. Selecting a game creates a stream session and swaps the main window into the streaming controller.
8. The stream view can be closed safely, returning the user to the catalog.

## Major Components

- `src/OPNAppDelegate.mm` orchestrates window state, screen transitions, account switching, and catalog loading.
- `src/auth/OPNAuthService.*` handles OAuth, session refresh, persistence, and logout.
- `src/games/OPNGameService.*` fetches catalog, library, panel, and subscription data, and launches games.
- `src/streaming/OPNStreamViewController.*` manages the live stream window, overlays, recovery, and termination flow.
- `src/streaming/OPNStreamSession.h` defines the stream session interface used by the libwebrtc implementation.
- `src/streaming/OPNLibWebRTCStreamSession.*` owns WebRTC negotiation, media rendering, stats, and input data channels.
- `src/streaming/OPNStreamPreferences.*` stores and resolves streaming profiles, region selection, codec, bitrate, and related toggles.
- `src/views/*` contains the native views for auth, loading, error, catalog, settings, backdrop, and game cards.
- `src/common/*` defines the shared auth, game, UI, and color types used across the app.

## Authentication

OpenNOW uses a browser-based OAuth flow with native session persistence.

- The email entry view routes users into the browser sign-in flow.
- Saved sessions are loaded on startup when stay-logged-in is enabled.
- Access tokens are refreshed when possible before the app drops back to manual sign-in.
- Multiple saved accounts are surfaced in the account menu.

## Streaming

The streaming stack is native and does not depend on a web UI.

- Game selection resolves the app ID, selected store, and whether the account is linked.
- The stream controller replaces the catalog view while the game is running.
- The stream session supports input delivery, stats collection, and recovery handling.
- The app tracks the display size and tries to select a reasonable streaming resolution for the active screen.

## Settings

The settings UI is organized into these sections:

- Stream
- Video
- Audio
- Input
- Interface
- About
- Thanks

The Stream section is backed by `OPNStreamPreferences` and reacts to stream-region updates posted by the app delegate.

## Repository Layout

- `src/main.mm` contains the `NSApplication` entry point.
- `src/OPNAppDelegate.h` and `src/OPNAppDelegate.mm` define the app coordinator.
- `src/auth/` contains sign-in and session management.
- `src/games/` contains catalog, panel, and launch orchestration.
- `src/streaming/` contains the stream session, signaling, view controller, and preference storage.
- `src/views/` contains the native AppKit views.
- `src/common/` contains shared data structures and UI helpers.
- `assets/` contains app artwork and store icons.

## Notes

- The app is built directly against Cocoa/AppKit, not SwiftUI.
- Logging is left in place for stream and catalog transitions to help with local debugging.
- The repo does not currently ship a test harness or packaged release workflow.
