# macOS Native Behavior Inventory

This inventory freezes the current Electron behavior that the macOS C++ port must
preserve. It deliberately excludes the iOS prototype.

## Source Boundaries

### Main Process Services

Reference files:

- `opennow-stable/src/main/index.ts`
- `opennow-stable/src/main/settings.ts`
- `opennow-stable/src/main/gfn/auth.ts`
- `opennow-stable/src/main/gfn/cloudmatch.ts`
- `opennow-stable/src/main/gfn/games.ts`
- `opennow-stable/src/main/gfn/subscription.ts`
- `opennow-stable/src/main/gfn/signaling.ts`
- `opennow-stable/src/main/gfn/errorCodes.ts`

Native ownership:

- OAuth provider discovery, PKCE login, token refresh, account persistence.
- Catalog, library, public game, and launch-id resolution APIs.
- Subscription and dynamic-region API access.
- CloudMatch create, poll, ad report, stop, active-session, and claim flows.
- GFN signaling WebSocket, heartbeat, peer messages, ICE, answer, and keyframe
  messages.
- Settings storage, migration, logging, media file management, and macOS
  permission prompts.

### Renderer Streaming

Reference files:

- `opennow-stable/src/renderer/src/gfn/webrtcClient.ts`
- `opennow-stable/src/renderer/src/gfn/inputProtocol.ts`
- `opennow-stable/src/renderer/src/gfn/sdp.ts`
- `opennow-stable/src/renderer/src/gfn/microphoneManager.ts`
- `opennow-stable/src/renderer/src/utils/streamDiagnosticsStore.ts`

Native ownership:

- Native WebRTC peer connection, data channels, ICE candidate exchange, media
  tracks, and WebRTC stats.
- SDP generation and munging for codec, color quality, input capabilities, and
  GFN-specific attributes.
- Keyboard, mouse, wheel, and gamepad packet encoding.
- Pointer lock equivalent, input backpressure, diagnostics, recovery, and
  microphone capture.

### UI Orchestration

Reference files:

- `opennow-stable/src/renderer/src/App.tsx`
- `opennow-stable/src/renderer/src/components/LoginScreen.tsx`
- `opennow-stable/src/renderer/src/components/HomePage.tsx`
- `opennow-stable/src/renderer/src/components/LibraryPage.tsx`
- `opennow-stable/src/renderer/src/components/SettingsPage.tsx`
- `opennow-stable/src/renderer/src/components/StreamLoading.tsx`
- `opennow-stable/src/renderer/src/components/StreamView.tsx`
- `opennow-stable/src/renderer/src/components/StatsOverlay.tsx`

Native ownership:

- Login/account switcher.
- Home, catalog, library, and search views.
- Stream preferences and settings.
- Queue/ad/session loading states.
- Stream controls, diagnostics overlay, screenshot, recording, and exit flow.

## IPC Contract Groups

Reference: `opennow-stable/src/shared/ipc.ts`.

- Auth: `auth:*`
- Regions and ping: `auth:get-regions`, `gfn:ping-regions`
- Subscription: `subscription:fetch`
- Games: `games:*`
- Session lifecycle: `gfn:create-session`, `gfn:poll-session`,
  `gfn:report-session-ad`, `gfn:stop-session`, `gfn:get-active-sessions`,
  `gfn:claim-session`, `gfn:session-conflict-dialog`
- Signaling: `gfn:connect-signaling`, `gfn:disconnect-signaling`,
  `gfn:send-answer`, `gfn:send-ice-candidate`, `gfn:request-keyframe`,
  `gfn:signaling-event`
- Window and input: `window:*`, `app:quit`
- Settings/logs/media/update/cache/community/Discord: remaining non-stream
  desktop services.

In native macOS, these should become typed C++ service APIs exposed through a
small Objective-C++ bridge rather than stringly typed IPC.

## Existing Test Anchors

Reference command:

```sh
npm --prefix opennow-stable test
```

Existing tests:

- `opennow-stable/src/shared/gfn.test.ts`
- `opennow-stable/src/renderer/src/gfn/inputProtocol.test.ts`
- `opennow-stable/src/renderer/src/gfn/webrtcClient.test.ts`
- `opennow-stable/src/renderer/src/lib/launchOwnership.test.ts`
- `opennow-stable/src/renderer/src/components/GameCard.test.ts`

Native fixture priorities:

- Stream preference normalization and color-quality helpers.
- Keyboard layout mapping for `darwin`.
- Input packet encoding bytes for heartbeat, keyboard, mouse, wheel, and gamepad.
- SDP transform inputs/outputs.
- CloudMatch response normalization, including ICE server and streaming server
  extraction.
- Signaling peer-message parse and outbound message construction.
- Error-code serialization.

## Behavioral Risks

- Chromium currently provides WebRTC, media devices, codec selection, hardware
  acceleration flags, stats, screen capture, and MediaRecorder behavior.
- Native macOS needs explicit entitlements for microphone, network, local callback
  server login, file access, and signed/notarized distribution.
- Input latency and packet framing are correctness-sensitive and must be tested
  byte-for-byte before stream integration.
- The Electron app must stay buildable while the native app is developed in
  parallel.
