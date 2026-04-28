# macOS UI Migration Notes

The native UI should rebuild the Electron renderer behavior in stages and should
not copy React component structure directly.

## Initial Shell

The current scaffold in `native/macos/OpenNOWMac` proves:

- SwiftUI can host the app shell.
- Objective-C++ can bridge into `opennow_core`.
- The UI can query core defaults and invoke the proof stream backend.

## UI Parity Order

1. Login and account switching.
2. Home, catalog browsing, search, and library.
3. Stream settings needed for launch.
4. Queue/ad/session loading states.
5. Stream view controls and diagnostics overlay.
6. Screenshots, recordings, logs, cache controls, updates, and Discord presence.

## Settings Migration

Current Electron settings live in the Electron user-data directory as
`settings.json`. The native macOS app should migrate these keys into its own
Application Support storage on first launch, preserving stream preferences,
shortcuts, region, bitrate, codec, color quality, controller mode, microphone,
and window size.

Token migration must be opt-in or protected by Keychain-backed storage before
shipping a release candidate.
