# Native macOS Release Readiness

The native macOS app must ship side-by-side with the Electron app until all gates
below are green.

## CI Gates

- `native/core` configures with CMake on macOS.
- `opennow_core_tests` passes under CTest.
- `native/core` installs and packages a release archive with CPack.
- Native macOS signing/notarization scripts pass a dry-run scaffold check.
- The native macOS host builds in Xcode once the project file is added.
- Electron macOS packaging remains unaffected.

## Signing And Notarization Gates

- Hardened runtime enabled.
- Network client entitlement enabled.
- Microphone entitlement enabled before microphone capture ships.
- User-selected read/write file access for screenshots, recordings, and log
  export.
- Developer ID signing configured for release artifacts.
- Notarization succeeds and stapling is verified.

Current scaffolding lives in `native/macos/release/`. CI validates the scripts
without secrets; release jobs must provide Developer ID and Apple notarization
credentials before signing can run for real artifacts.

## Side-By-Side Validation

Before replacing the Electron macOS artifact, validate the native app against the
Electron app for:

- Login, logout, account switching, and token refresh.
- Catalog, library, public games, launch-id resolution, and subscription state.
- Session create, queue, ad reporting, poll, stream start, stream stop, and claim.
- Signaling answer, ICE candidate, keyframe request, reconnect, and clean close.
- Input latency, mouse capture, keyboard layout mapping, gamepad packets, and
  partially reliable channel behavior.
- Stats overlay values for RTT, jitter, packet loss, decode FPS, render FPS, and
  bitrate.
- Settings migration from Electron user data.
- Crash logs, app logs, screenshot output, and recording output.

## Release Rule

Keep Electron as the macOS release artifact until native WebRTC streaming has
passed real-session validation on Apple Silicon and Intel macOS hardware.
