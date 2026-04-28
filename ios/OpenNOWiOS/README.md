# OpenNOW iOS

Native SwiftUI app shell for OpenNOW on iPhone and iPad, using standard iOS navigation and controls.

Current app architecture:

- native `TabView` navigation (`Home`, `Browse`, `Library`, `Session`, `Settings`)
- centralized `OpenNOWStore` for auth, catalog/library loading, subscription, session lifecycle, and settings persistence
- direct NVIDIA API integration (same service family used by desktop):
  - providers: `https://pcs.geforcenow.com/v1/serviceUrls`
  - OAuth + user info: `https://login.nvidia.com/*`
  - games GraphQL: `https://games.geforce.com/graphql`
  - subscription: `https://mes.geforcenow.com/v4/subscriptions`
  - cloud session create: `https://<vpc>.cloudmatchbeta.nvidiagrid.net/v2/session`
- CloudMatch lifecycle support:
  - poll active session state (`GET /v2/session/{id}`)
  - stop active session (`DELETE /v2/session/{id}`)
  - enumerate active sessions (`GET /v2/session`)
  - claim/resume active sessions (`PUT /v2/session/{id}` with `action=2`)
- OAuth callback URL scheme registered: `opennowios://oauth-callback`

## Open In Xcode

1. Open `ios/OpenNOWiOS/OpenNOWiOS.xcodeproj`.
2. Choose an iOS or tvOS simulator/device.
3. Run the `OpenNOWiOS` target.

## Notes

- Deployment target: iOS 17.0
- Bundle ID (default): `com.opencloudgaming.opennow`
- App icon slots are scaffolded in `OpenNOWiOS/Assets.xcassets/AppIcon.appiconset`.
- The shared `OpenNOWiOS` target now builds for tvOS as well.
- The current Apple TV build is intentionally limited: sign-in uses the tvOS authentication session path, Live Activities are disabled, notifications are stubbed, and in-app streaming still needs a native tvOS player rather than the iOS web-view flow.
