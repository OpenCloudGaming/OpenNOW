# Android WebRTC low-latency audio

Issue [#793](https://github.com/OpenCloudGaming/OpenNOW/issues/793) reports delayed
audio with WebRTC using the normal Android mixer path. `StreamAudioPolicy.kt`
requests `JavaAudioDeviceModule.setUseLowLatency(true)` on Android 8 and newer,
including TVs. Stereo output and `USAGE_GAME` are preserved. Android 6/7 retain
the normal AudioTrack path. The OS still decides whether the output route can
actually deliver low latency.

Settings > Audio and keyboard offers **Low latency game audio**. It defaults on
for existing and new installs. Turning it off routes WebRTC and experimental NVST
playback through `USAGE_MEDIA` and disables their low-latency AudioTrack request
starting with the next stream. This is a route choice, not a volume boost; compare
the same game and output device before judging loudness.

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

## NVST audio isolation (NTE report, 2026-11-04)

The supplied build-179 report for NTE: Neverness to Everness uses native NVST,
an OPPO CPH2591 and a GamePadPlus controller. It records sustained HEVC decoder
stalls (128 ms average decode time, 16 decoded FPS against a 30 FPS target).
Haptics were advertised disabled. The export contains no audio capture or
AudioTrack underrun measurements, so it does not establish the sound's cause.

Previously, NVST audio and video shared an eight-frame channel and synchronous
JNI consumer. A video decoder stall could fill that channel and discard Opus
packets. Pending decoded audio was only drained when another packet arrived.
Audio now has its own bounded eight-packet channel and audio-priority worker;
video acknowledgement timing stays tied to decoder acceptance. Idle ticks every
5 ms drain MediaCodec output and partial AudioTrack writes. Decoder format-change
notifications no longer end a drain pass. The worker stops and joins before Java
releases its decoder and AudioTrack, including startup failure and reconnect.

`audio_worker` Rust tests cover a saturated video queue, bounded audio backlog,
idle draining, worker error propagation, and joined shutdown. The Android
`NvstAudioOutputInstrumentedTest` checks final-packet draining without another
network packet and repeated codec/track creation and release. Neither test proves
the audible NTE/gamepad symptom is resolved. Retest that game with the reported
controller, compare connected/disconnected playback, and check retained
`nvst.audio-worker`, `nvst.audio-input`, and `nvst.audio-failure` diagnostics.

Validation: 193 Rust tests passed (one existing test ignored), all 1,454 Android
unit tests passed, and the four-ABI debug APK built. The Android 17 arm64 emulator
passed the final-packet/restart instrumentation test. Physical NTE/controller
playback remains unverified.
