# OpenNOW Native Android

This folder is a standalone Android Studio project for the native Kotlin / Jetpack Compose OpenNOW target.

Open it from Android Studio with **File > Open > `OpenNOW/android`**. Android Studio will install/use the required Gradle, Android SDK, CMake, and NDK components from the project configuration.

## Build Targets

- `:app:assembleDebug` builds a debug APK.
- `:app:assembleRelease` builds a release APK after signing config is added locally.

## Runtime Notes

- UI is native Compose.
- GFN auth, catalog, subscription, CloudMatch session creation/polling/claim/stop, signaling, and input packet behavior are implemented in Kotlin from the Electron project contracts.
- Streaming uses Android WebRTC plus hardware MediaCodec probing. The bundled `opennow_native` JNI library exposes native runtime diagnostics and keeps the NDK/CMake path wired for media-sensitive code.
- Queue ad metadata is preserved in `SessionInfo`; ad playback can use the included Media3 dependency when the server returns an ad media URL.
