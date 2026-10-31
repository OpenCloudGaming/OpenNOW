# Streaming crash validation

The WebRTC dependency is pinned to `144.7559.14`. The buildSrc transforms apply to
both debug and release builds and reject unexpected dependency method shapes.
They do not change the negotiated transport or enable software decoding.

## Deterministic regressions

- `WebRtcTextureDeliveryTest` forces texture-helper creation to return null, as
  upstream does after EGL initialization fails. Decoder initialization must return
  `ERROR`, clear its callback, and remain safe to release or retry. Successful
  texture initialization and initialization without a shared EGL context must keep
  their existing behavior.
- `WebRtcPlaybackGuardTest` exercises playback stop while a worker is fetching or
  writing audio, including the bounded join timeout and immediate restart. A live
  old worker must retain its playback resources and prevent their replacement.
  Both low-latency settings must preserve normal playback and cleanup.

Run the transform tests before the Android unit suite and APK build:

```sh
cd android
./gradlew :buildSrc:test :app:testDebugUnitTest :app:assembleDebug
cargo test --locked --manifest-path nvst/Cargo.toml --workspace
```

The tests inject failure conditions; passing them does not establish that a
particular production crash report has the same cause. Java exception boundaries
also cannot catch a native driver abort, process kill, or out-of-memory killer.

## Device checks

Use the same game, server region, resolution, FPS, codec, video output, and
transport when comparing builds. Record the app build, Android version, device
model, selected decoder, and whether the app exited or only the stream stopped.
Keep NVST and WebRTC results separate; NVST texture output also uses the guarded
WebRTC decoder, but its audio does not use WebRTC playback.

On each device, verify initial playback, opening and closing stream overlays,
background/foreground transitions, local disconnect and resume, and repeated
start/stop. Exercise direct SurfaceView and texture output separately, with
microphone disabled and enabled when permission is available. Test a conservative
720p30 profile before increasing resolution, FPS, or enabling HDR.

A cloud Pixel test is a smoke check, not a hardware compatibility matrix. Vendor
MediaCodec/EGL behavior still needs affected physical devices, particularly
Qualcomm, Exynos, MediaTek, and low-memory/32-bit devices. Do not report gameplay
as validated if testing stopped at a queue, account prompt, or decoded login screen.

After an unexpected exit, reopen OpenNOW and export diagnostics before clearing
app data. Compare the incident time with `previousJavaCrash` and
`processExitHistory` as described in [diagnostics-format.md](diagnostics-format.md).
Keep native crash, Java crash, ANR, and low-memory exits distinct; an API error or
continued audio alone does not identify a video crash.
