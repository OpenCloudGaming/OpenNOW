# OpenNOW macOS Host

This directory is the native macOS host for the C++ core. It intentionally starts
as lightweight scaffolding so the stream proof can be validated before the full
desktop UI is rebuilt.

## Host Strategy

- Keep durable protocol and service behavior in `native/core`.
- Use Objective-C++ as the bridge layer between C++ and AppKit/SwiftUI.
- Use native macOS APIs for windows, input capture, game controllers,
  microphone permissions, file dialogs, signing, and notarization.
- Keep the Electron app intact until this host reaches release-candidate quality.

## First Milestones

1. Link `opennow_core`.
2. Expose login/session/catalog operations through a small bridge API.
3. Prove native WebRTC rendering in a minimal stream window.
4. Rebuild the production UI around the proven bridge.

## Local App Bundle

Build and launch the current native host scaffold from the repository root:

```sh
native/macos/build-app.sh
open native/macos/build/OpenNOWMac.app
```

The bundle is unsigned and intended for local validation only.

## Native Auth Scaffold

The Account screen can now start the native login path. It generates the NVIDIA
authorization URL through the C++ service builders, opens the system browser, and
stores pending login metadata in Keychain. The OAuth callback listener and token
exchange are still intentionally separate follow-up work.

## Release Scaffolding

- `native/scripts/validate-macos-release.sh` is the CI and local preflight for
  native macOS release work. It builds/tests/packages `native/core`, builds the
  unsigned local `OpenNOWMac.app` bundle, and checks that signing assets are
  present.
- `release/OpenNOWMac.entitlements` captures the first hardened runtime
  entitlements expected by the host: outbound network access, microphone access,
  and user-selected read/write file access.
- `release/sign-and-notarize.sh` signs an `.app` bundle with Developer ID,
  submits it to Apple notarization, staples the ticket, and supports
  `OPENNOW_DRY_RUN=1` for CI scaffold checks.

Example dry run:

```sh
OPENNOW_DRY_RUN=1 \
DEVELOPER_ID_APPLICATION="Developer ID Application: Example" \
APPLE_ID="notarization@example.com" \
APPLE_TEAM_ID="TEAMID1234" \
APP_SPECIFIC_PASSWORD="app-specific-password" \
native/macos/release/sign-and-notarize.sh /path/to/OpenNOWMac.app
```

Real signing should only be enabled once live auth/session/streaming are wired
and CI secrets are available.
