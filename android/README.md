# OpenNOW Native Android

This folder is a standalone Android Studio project for the native Kotlin / Jetpack Compose OpenNOW target.

Open it from Android Studio with **File > Open > `OpenNOW/android`**. Android Studio will install/use the required Gradle, Android SDK, CMake, and NDK components from the project configuration.

## Build Targets

- `:app:assembleDebug` builds a debug APK.
- `:app:assembleRelease` builds the direct-distribution release APK with APK update support.
- `:app:bundleRelease` builds the Google Play Android App Bundle. This task removes `REQUEST_INSTALL_PACKAGES` and disables APK self-updates so Play installs use Google Play's update mechanism.

Release and debug builds include `arm64-v8a`, `armeabi-v7a`, and `x86_64`. The `armeabi-v7a` slice supports 32-bit ARM phones and 32-bit Android TV firmware. OpenNOW recommends 720p/30 FPS/12 Mbps for 32-bit processes and memory-constrained TVs, and warns when a custom profile exceeds that recommendation without overriding the user's selection.

## APK Update Manifest

APK and debug builds check `https://api.printedwaste.com/releases/opennow/latest` and can download the returned APK. App Bundle builds installed from Google Play detect `com.android.vending` as the install source and do not check, download, or install APK updates. The manifest should look like this:

```json
{
  "versionCode": 7,
  "versionName": "0.5.2",
  "apkUrl": "https://api.printedwaste.com/release-files/opennow/app-release.apk",
  "artifactUrl": "https://api.printedwaste.com/release-files/opennow/app-release.apk",
  "sha256": "optional lowercase apk checksum",
  "releaseNotes": "Short notes shown in Settings\nSecond line"
}
```

`apkUrl`, `artifactUrl`, or `url` may point at the APK. `releaseNotes` may use real newlines or literal `\n` separators. The app compares `versionCode` against its installed build and asks Android's package installer to confirm the downloaded APK.

## Runtime Notes

- UI is native Compose.
- GFN auth, catalog, subscription, CloudMatch session creation/polling/claim/stop, signaling, and input packet behavior are implemented in Kotlin from the Electron project contracts.
- Streaming uses Android WebRTC plus hardware MediaCodec probing. The bundled `opennow_native` JNI library exposes native runtime diagnostics and keeps the NDK/CMake path wired for media-sensitive code.
- Queue ad metadata is preserved in `SessionInfo`; ad playback can use the included Media3 dependency when the server returns an ad media URL.

## Experimental NVST

Settings > Advanced > Experimental streaming > NVST transport selects the native transport
ported from dev commit `3002e5c67`. NVST defaults off for all distributions. On the first 1.6.4
launch, a persisted migration also switches off the previous automatic APK opt-in. Users can
enable the experiment again afterward; that choice survives subsequent launches and upgrades.
The choice applies when attaching a stream; changing settings does not replace a running transport.

The NVST runtime performs RTSP-over-WebSocket negotiation, authenticated SRTP UDP video with
FEC/NACK recovery, Opus audio, and native SCTP keyboard/mouse/gamepad input. Android keeps its
hardware decoder and renderer. Input and encoded-media queues are bounded. Reconnects retain the
cloud session; only the existing explicit End Session action owns cloud-session termination.

Microphone upstream uses 48 kHz Opus with 20 ms frames and RED redundancy on the encrypted
bundle. Capture follows microphone permission, foreground-service, mute, and attachment lifecycle;
muting releases the recorder and clears redundant speech on resume. Native game touch uses reliable
control messages, and received rumble uses the existing vibration switch and controller/device routing.
On-screen gamepad and touch mouse continue to use ordinary gamepad/mouse packets. A previously allocated WebRTC session might not provide an NVST endpoint;
the app reports this and retains that session rather than terminating or replacing it.

Building now requires Python 3, CMake, Cargo/Rust (via rustup recommended), and Android Rust targets:

```sh
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android i686-linux-android
./gradlew :app:assembleDebug
```

Gradle builds the pinned Rust sources for all four packaged ABIs using its NDK, including statically linked Opus, with 16 KB ELF
page alignment. No precompiled NVST binaries are downloaded. The initial build downloads crates;
later builds reuse Cargo and Gradle outputs. `CARGO` can select an alternative Cargo executable.

Protocol validation: `cargo test --manifest-path nvst/Cargo.toml --workspace`.
Live GFN playback, touch/controller hardware, HDR output, and network-loss recovery still need
physical-device verification. Source provenance and Android-specific differences: `nvst/UPSTREAM.md`.
