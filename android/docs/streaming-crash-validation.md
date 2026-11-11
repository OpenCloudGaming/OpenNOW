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

## Play Console reports from build 179 (2.2.3)

The Android 12 `ForegroundServiceDidNotStartInTimeException` sample omits the
service name. Both queue and stream services now use `AndroidForegroundService`
to promote in `onCreate`, before locale preference reads or wake-lock acquisition.
Every command promotes again, including STOP/unknown/null commands, because a
new foreground start can arrive after a preceding STOP demotes the same instance.
Notifier command revisions discard obsolete queued starts/stops and prevent an
obsolete failed queue start from restoring a canceled notification.

The Android 16 Motorola `portov` native abort (`RefBase used with stack pointer
argument`, `MediaCodec::onMessageReceived`) identifies a platform codec-handler
failure, but does not identify the codec, transport, or operation. The diagnostic
NDK probe previously allocated and immediately deleted real decoders for each
MIME type, driving that handler even outside playback. API 36+ now uses the
[NDK codec metadata query](https://developer.android.com/ndk/reference/group/media#amediacodecstore_findnextdecoderforformat)
instead. A query failure remains unknown, without falling back to allocation.
Older Android versions retain the existing NDK probe. This removes an unnecessary
native lifecycle path; it does **not** establish that the reported abort happened
during probing or that Motorola playback is fixed.

Targeted device/emulator regressions:

```sh
./gradlew :app:connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=com.opencloudgaming.opennow.ForegroundServiceLifecycleInstrumentedTest,com.opencloudgaming.opennow.NativeCodecProbeInstrumentedTest
```

The service regression covers cold STOP-only starts, repeated start/stop/start,
foreground state and final cleanup, then waits past the asynchronous startup
timeout. The native regression repeats supported and unsupported MIME queries on
API 36+. Repeat service checks on Android 12 and playback/reconnect checks on the
reported Motorola Android 16 device before claiming those reports resolved.

## Additional build-179 ANR and artwork reports

The November 7 issue list and expanded traces expose several distinct paths:

- `AndroidUriMapper` runs inside Coil's interceptor chain while Compose measures
  the catalog grid. Fetch/decode dispatchers did not move mapping or cache-key
  creation off Main. The application loader now also assigns a bounded worker
  dispatcher to the interceptor chain, including memory-cache requests. See
  [Coil's dispatcher contract](https://coil-kt.github.io/coil/api/coil-core/coil3/-image-loader/-builder/index.html).
- `FT_Stream_ReadULong` enters font-variation creation through Compose's blocking
  resource-font loader during navigation layout. All five Inter weights now use
  a shared `AndroidFont` loader with `FontLoadingStrategy.Async` and an explicit
  bounded IO dispatcher around resource reads and variation creation. An async
  resource-font declaration alone still starts local loading inline because
  Compose launches it undispatched. The system fallback remains until Inter loads.
  See [Compose font loading](https://developer.android.com/reference/kotlin/androidx/compose/ui/text/font/FontLoadingStrategy).
- The external-mouse ANR trace samples the input router without showing a blocked
  native send. Inspection nevertheless found a remaining failure path that moved
  WebRTC sends back onto the input caller after a rejected/throwing worker send.
  That fallback is removed: subsequent packets retain the bounded worker queue.
  This closes a concrete ANR risk without establishing the sampled ANR's cause.
- Diagnostic sharing moved the final full-log redaction and summary preparation
  back to Main after awaiting the worker export. Preparation and upload now stay
  on IO, with preparation failures handled by the existing share-error state.
  Long escaped quoted values also reproduced `StackOverflowError` in the JVM
  redactors. Disjoint, possessive quoted-value matching removes recursive matching
  and handles incomplete previews. Android uses a different regex engine, so the
  JVM regression alone does not identify the production ANR's cause.
- `EmitFancyRGB` is inside the legacy system WebP decoder. Shared NVIDIA artwork
  URL construction now requests JPEG on API 24–27 and retains WebP on API 28+.
  The different URL avoids reusing old WebP cache entries. NVIDIA's CDN returned
  HTTP 200 with JPEG bytes and `image/jpeg` for a sample poster's `;f=jpg;w=272`
  request. This is a targeted workaround, not a reproduction of the native crash;
  arbitrary third-party WebP URLs still use the system decoder.

`AnrWorkIsolationInstrumentedTest` loads every Inter weight from Main through the
loader's checked worker boundary, checks local Android-URI decoding and memory
cache hits from Main, and injects rejected/throwing input sends followed by a
stalled send. It verifies that JNI stays off Main and excess input is rejected at
the existing 256-packet queue limit. Unit regressions cover large escaped/truncated
redaction values, font loading strategy, and legacy/current artwork URLs.

```sh
./gradlew :app:testDebugUnitTest :app:assembleDebug :app:assembleDebugAndroidTest
ANDROID_SERIAL=<test-device> ./gradlew :app:connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=com.opencloudgaming.opennow.AnrWorkIsolationInstrumentedTest
```

The remaining traces do not establish an actionable app trigger: the generic
`RemoteServiceException` omits its message/service; `BaseRenderNodeAnimator` and
`SurfaceComposerClient` crash in platform render/codec threads; one no-focus ANR
waits in `addToDisplayAsUser` Binder IPC; others sample process initialization, a
trivial UI-state constructor, or ordinary text drawing. The GPU ANR waits in
`nSyncAndDrawFrame`. The codec-probe mitigation above is already present for API
36+. Keep these reports open pending device/OS details, exception messages,
reproduction, or new-version evidence. Do not infer that a sampled trivial method
or unrelated background worker is the root cause, or disable hardware rendering
globally based on these traces.

Validation on November 7: all 1,461 Android unit tests, 32 build-transform tests,
debug APK build, and Android test APK build passed. All three
`AnrWorkIsolationInstrumentedTest` cases, the foreground-service lifecycle case,
and the codec metadata case passed together on the Android 17 ARM64 emulator
(five tests). The quoted-value regressions failed with `StackOverflowError`
before the redactor change. No affected physical-device or gameplay validation
was performed, and the legacy JPEG workaround was verified against a CDN sample
and URL-policy tests rather than the crashing device/image.

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
