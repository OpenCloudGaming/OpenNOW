# Android WebRTC low-latency audio

Issue [#793](https://github.com/OpenCloudGaming/OpenNOW/issues/793) reports delayed
audio with WebRTC using the normal Android mixer path. `StreamAudioPolicy.kt`
requests `JavaAudioDeviceModule.setUseLowLatency(true)` on Android 8 and newer,
including TVs. Stereo output and `USAGE_GAME` are preserved. Android 6/7 retain
the normal AudioTrack path. The OS still decides whether the output route can
actually deliver low latency.

## Pinned WebRTC teardown guard

In `io.github.webrtc-sdk:android:144.7559.14`, `stopPlayout()` stops the playback
thread and waits up to two seconds before stopping/releasing/nulling the track.
A delayed blocking write can finish after that timeout. The playback loop then
calls `LowLatencyAudioBufferManager.maybeAdjustBufferSize()` without checking
whether the thread was stopped or the track released.

`buildSrc` instruments that one buffer-tuning method for all app variants:

- Skip null or uninitialized tracks.
- Catch `IllegalStateException` if release races the state check.
- Otherwise execute the unchanged upstream tuner, including underrun recovery.

This protects buffer adjustment only; it does not replace WebRTC's audio lifecycle
or guarantee that every driver/teardown failure is handled. There is no runtime
reflection, custom audio thread, or per-buffer allocation in the guard. Review it
when updating WebRTC and remove it when upstream handles the race. A changed
method signature fails instrumentation; if upstream renames/removes the class,
also remove/update the class selector and verify the packaged output.

## Validation

From `android/`:

```sh
./gradlew :buildSrc:test :app:testDebugUnitTest :app:assembleDebug --console=plain
```

The buildSrc tests execute transformed JVM fixtures covering live, null, released,
and concurrently released tracks, unrelated exceptions, and a changed signature.
The app test covers the Android API boundary.

On a device, check for `audio playback lowLatencyRequested=true` in the exported
diagnostics and `createAudioTrackOnOreoOrHigher` in WebRTC logcat. The first message
records the request, not a latency measurement. Compare `dumpsys media.audio_flinger`
and audible attack/video timing with the previous APK on the reported RedMagic
tablet. Exercise stream stop/start, reconnect, background/resume, and audio-route
changes; check for underruns, audio loss, and teardown exceptions. A successful
build does not establish the reported audio delay is resolved.
