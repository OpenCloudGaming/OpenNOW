# OpenNOW iOS

Native iPhone/iPad GeForce NOW client with hardware decoding, HDR and 4:4:4 streaming, MetalFX spatial upscaling, controller/touch input, pointer capture, picture in picture and per-game stream profiles. Frame generation is removed.

## Metal rendering

Metal 4 is opt-in through **Settings → Stream → Metal 4 Rendering** or the live stream **Picture** panel. It defaults off, including for existing settings without the new key. When enabled on supported iOS 26+ devices, Metal 4 handles direct HDR rendering, native color conversion, sharpening and MetalFX. Turning it off uses compatible rendering while preserving HDR, color and MetalFX choices. The display layer's residency set is registered with each Metal 4 queue, and frames present directly to its drawables. The block repair was verified on an iPhone in build 149 with HEVC 10-bit 4:4:4 HDR and active MetalFX. See [renderer architecture and validation](METAL4.md).

Build 150 removes the diagnostic presentation-copy experiment, launch overrides, developer HUD and disk/per-frame trace machinery. Normal stream stats and error reporting remain. Existing settings load without the removed developer-HUD key; video and input choices are preserved. Historical experiment notes and pre-cleanup sources are archived locally under `Build/cleanup-before-150`.

## Build on an Apple Silicon Mac

Install full Xcode providing the iOS 27 SDK and its iOS platform. Run these commands from the repository root:

```sh
python3 ios/OpenNOWiOS/BuildScripts/prepare-native-libraries.py
xcodebuild build \
  -project ios/OpenNOWiOS/OpenNOWiOS.xcodeproj \
  -scheme OpenNOWiOS \
  -configuration Debug \
  -destination 'generic/platform=iOS' \
  -derivedDataPath DerivedData \
  CODE_SIGNING_ALLOWED=NO
```

The preparation script verifies the pinned SHA-256 of `Frameworks/OpenNOW-iOS-128-native-libraries.zip`, which is checked into this repository, and extracts only the matching static OpenSSL/usrsctp XCFrameworks. Preparation requires no network access or third-party release download. The native C libraries are unchanged from build 128. Extracted frameworks and build outputs are ignored by Git. WebRTC remains the vendored framework already in the upstream repository. For a source build of the native dependencies, install CMake and run:

```sh
python3 ios/OpenNOWiOS/BuildScripts/build-native-protocol-libs.py --cmake /path/to/cmake
```

The unsigned device app is in `DerivedData/Build/Products/Debug-iphoneos/OpenNOWiOS.app`. To run directly from Xcode, open `ios/OpenNOWiOS/OpenNOWiOS.xcodeproj` and select your signing team. The native dependency archives contain arm64 device and arm64 simulator slices.

## TestFlight automation

Xcode Cloud can archive and upload every push to `kief5555/ios`, automatically incrementing the build number. The post-clone script prepares the native libraries on a clean runner. See the [Xcode Cloud workflow configuration](XCODE_CLOUD.md).

## Native receiver

The native NVST receiver requires an advertised RTSPS endpoint and compatible hardware decoding. HEVC 10-bit 4:4:4 is validated against the actual bitstream and received surface, preserving chroma/depth or reporting an unsupported format. Ordinary WebRTC and compatible rendering remain available on their supported paths.

Native microphone carriage requires a host bundle offering it and the microphone option enabled. Protocol provenance, pinned dependencies and licenses are in [NVST](OpenNOWiOS/NVST/README.md).

## Checks

Run the simulator test scheme `OpenNOWiOS`, plus the GPU validation commands in [METAL4.md](METAL4.md). Simulator and Mac checks cannot establish iPhone/iPad performance. Build 149 verified the visible iPhone block repair; consistent 120 FPS, the separate decoder recovery event and physical iPad behavior remain separate validation targets.

## Controller rumble diagnostics

Build 152 adds a direct output test under Settings → Input → Touch & Controller. Connect the G8+ MFi, check the reported controller name/category and iOS haptics availability, then use Test Controller Rumble. Left/right tests appear only for exposed handle outputs. A successful API call reports acceptance, not proof that the physical motors moved. The pulse lasts 0.4 seconds; Stop Test, leaving the page, backgrounding and connection changes clean up the test engine. No stream or phone vibration fallback is used.

If controller haptics are unavailable, the normal GameController API cannot drive this controller's motors in its current connection/firmware state. If the direct test works but game rumble does not, investigate host haptic events and controller-slot routing next. No unsupported vendor protocol or firmware change is attempted by this branch.

## Android streaming parity (October 2026)

The iOS client shares Android's preference for a usable WebRTC route over an earlier RTSP control entry. Native-only allocations are retained as such, but cannot start the native receiver while Native NVST is disabled; the stream explains that a new WebRTC allocation or an explicit opt-in is required. Reopening uses the current saved transport preference while preserving the active game's other stream settings. NVIDIA WebRTC launch and claim requests use the browser client identity; alliance identities remain unchanged. Native requests use Android's streamer version 14, SDK 2.0 and native transport envelope on both launch and claim. Session reply numbers and ICE line indexes are checked before conversion. Stop immediately dismisses local media and invalidates pending launch, claim, poll and delayed reopen work before waiting for the provider.

Rig setup uses an indeterminate indicator until the provider reports a new phase; elapsed time is not presented as a completion percentage. Queue position is hidden once rig setup has started. PrintedWaste routes require source data no older than 30 minutes and are rechecked when selected. Provider estimates are labeled separately from measured movement in the active GeForce NOW queue.

Diagnostics exports include up to eight in-memory signaling attempts, retained after Stop/retry, with packet/offer/send counters, close/error codes and available URLSession DNS/TCP/TLS timings. Payloads, SDP and URL queries are excluded. Send completion does not establish server acknowledgment. Android JVM crash handlers and MediaCodec tuning are platform-specific; iOS retains its existing VideoToolbox/Metal decoder and presentation paths.

Native NVST requests a keyframe when the control channel becomes ready and retries once per second while decoded video is stalled, within the existing 20-second failure budget. Asynchronous decoder failures request recovery immediately rather than waiting for another video packet. Audio reception does not reset the video watchdog.

Native connection progress distinguishes TCP/TLS, WebSocket upgrade, OPTIONS, DESCRIBE, SETUP, local receiver/bundle preparation, ANNOUNCE and PLAY. The connection deadline includes the WebSocket upgrade, and request deadlines include both writes and responses. Stop cancels pending socket operations without waiting for their deadline or trying another endpoint. Bounded in-memory connection diagnostics retain the last stage on failure without logging handshake headers, SDP or credentials. These checks establish connection timeout/cancellation behavior, not successful physical-iPhone gameplay.

Catalog projections are reused until their inputs change, and catalog snapshot JSON encoding/decoding runs off the main actor. Account and catalog generations prevent stale cache reads or writes from crossing account changes. The library exposes All Games/Favorites, search, and labeled filtering/sorting controls; server selection uses native grouped sections and adapts to accessibility text sizes.

Continue Playing supports Android-style local dismissal: long-press a recently played card and choose **Remove from Continue Playing**. **Show Hidden Games** in the section's options menu restores those cards. Playing a game again also restores it. This does not alter provider history or hide active/resumable sessions.

## Native shell parity

Settings uses searchable native pages for quality, display, connection, input, appearance, catalog, statistics, sessions, and diagnostics. Existing audio mute, controller passthrough, HUD clock, text-clear confirmation, and controller-prompt preferences are exposed where they have working consumers. Resetting the default touch layout restores its geometry, sizes, opacity, padding, and offsets without deleting custom profiles or changing unrelated input preferences. Proxy activation requires a trust confirmation. The unused analytics switch and consent prompt are removed; persisted preference decoding remains compatible.

Library has native search, All Games/Favorites, launcher filters, consolidated filter/sort options, and poster/list layouts. Accessibility text sizes use the list layout. Metadata indexing and filtering run off the main actor, retain only the current index/results, cancel superseded work, and invalidate across account/catalog changes. Shared artwork requests keep subscriber ownership, so leaving a card cancels its abandoned download without canceling another visible card's request.

Retained sessions are checked with the provider after returning to the shell or foreground. Unavailable status is shown as **Check Session**, not a promise that an old allocation is ready. Confirmed terminal responses clear resume surfaces; transient failures retain the allocation for retry. Resume validates the exact allocation rather than silently selecting a different session for the same game. Session-ending actions capture identity and phase and explain queue loss or unsaved progress. Stop failures remain visible with account-session recovery.

The queue keeps status/actions visible alongside landscape ads, distinguishes PrintedWaste estimates from live queue movement, and uses indeterminate rig setup rather than elapsed-time percentages. Lock-screen Live Activities use phase-colored surfaces; Dynamic Island retains its system-controlled black background with accented content. Stale activities hide readiness, queue numbers, progress, and resume actions.

Android-only APK management, MediaCodec/output modes, dynamic bitrate, gyro/remapping, recording, TV pairing, and system-launcher integration are not represented by nonfunctional iOS controls. This shell work does not change streaming transport, decoding, rendering, or input implementations.

`OpenNOWiOSParityTests` covers settings search/reset preservation, session confirmation identity and phase, terminal versus transient checks, stale activities, queue estimates, artwork cancellation, and Library projection/filter/date ordering. `testQueueSettingsAndLiveActivityPresentationsRender` retains screenshots for queue, large text, Settings, Library, and queued/setup/ready/stale lock-screen cards. Simulator projection measurements are not a claim about physical-device scrolling or gameplay performance; rotation with ads, VoiceOver, actual ActivityKit transitions, and physical-device responsiveness still require device checks.
