# Android controller haptics and HDR streaming

This change uses OpenNOW's existing application identity and update configuration.
It adds an opt-in USB haptics companion and improves the existing Android streaming
paths without replacing the session or authentication flow.

## Kishi USB haptics

Enable **Kishi USB haptics** in the controller settings, with vibration enabled
and output set to Auto or Controller. The default strength is 40 percent.
The manager requires exactly one supported Razer Kishi identity and the dedicated
Sensa interface before opening USB output. It leaves the gamepad input interface
to Android and associates rumble with the correct Android controller slot.
Devices without a USB service can still open the app; enabling the companion
reports its unavailability instead of crashing during manager construction.

Supported identities are vendor `0x1532`, products `0x0724` and `0x0727`;
the shared XInput product `0x0037` additionally requires the observed
`Razer Kishi V3 Pro` product name. Interface 4 must match the expected interrupt
endpoints and packet size. Firmware metadata is validated before output starts.
V3 Pro was physically tested; XL support remains based on the matching protocol
and descriptor checks rather than a separate XL hardware test.

Opening or returning to the app checks an already-connected controller's USB
permission. A missing grant opens Android's permission dialog. Repeated resumes
and scans do not reopen it after denial during the same foreground visit;
the next visit or manual **Authorize / Retry** can request again. Foreground
attachment also checks permission; background attachment does not open a dialog.
The system permission activity does not count as a new app visit.

Strong/weak game events and stop commands reach USB before Android one-shot
throttling. A dedicated worker owns the USB connection and bounded exchanges,
and lifecycle cleanup silences output and restores the prior Sensa mode.
Local left/right tests are time-limited. Permission callbacks, detach/replacement
and worker shutdown share the manager's existing scan/initialization path.

## NVST corrections

Gamepad snapshot sequences use the full big-endian 16-bit protocol field.
The earlier byte-sized counter wrapped after 255 updates, allowing neutral/release
snapshots to look older than the last held input. A regression exercises both
that boundary and the full 16-bit wrap.

Haptics availability uses reliable RemoteInput type 13 with its padded,
timestamped envelope. Startup advertises it disabled until Android determines
device and user availability. The parser validates both classic rumble framing
and the observed GFN kind/length framing and preserves their distinct motor order.
See [NVST provenance](nvst/UPSTREAM.md) for upstream wire references.

Native and Android interval counters distinguish packet gaps, discontinuities,
consumer backpressure, keyframe waiting, decoder rejection and dispatch time.
These diagnostics do not change the existing recovery requests or end the cloud
game when the local attachment stops.

## HEVC and AV1 HDR

HDR requires the existing subscription checks, an HDR10 display with usable
measured luminance, and a codec-specific Main10 hardware decoder supporting the
requested size/rate. Supported profiles are HEVC or AV1, up to 3840×2160 and
120 FPS. Constrained runtime profiles and unsupported devices fall back through
the existing SDR compatibility path.

CloudMatch HDR signaling is separate from its SDR bit-depth override. SDP and
native ANNOUNCE request HDR and 10-bit 4:2:0 consistently. The opaque MediaCodec
surface retains PQ/BT.2020 output; explicit SDR output metadata is rejected
instead of displaying it as HDR. No fabricated mastering luminance or 8-bit
readback is introduced. Direct HDR recording remains unavailable.

HDR codecs use asynchronous callbacks on Android 11 and newer, with compatibility
polling on older supported APIs. Decoder/generation/Surface checks discard stale
callbacks and buffers. Borrowed WebRTC encoded data is copied before returning
to its native pool. Metadata for AV1 inputs that update hidden reference pictures
is bounded separately from unsent input backpressure.

SDR and HDR share a Qualcomm latency profile policy. Configuration tries full
vendor hints, then core hints, then real-time scheduling without vendor hints.
Each rejected HDR attempt releases its codec; SDR retries reset the codec and
construct a fresh format. Maximum operating-rate forcing remains disabled.
The vendor hints and asynchronous strategy were compared against
[Moonlight V+ MediaCodecHelper at the inspected revision](https://github.com/qiin2333/moonlight-vplus/blob/78b3493a48a47f68518ce3826f1add63c2d2871b/app/src/main/java/com/limelight/binding/video/MediaCodecHelper.kt)
and the [Android MediaCodec callback API](https://developer.android.com/reference/android/media/MediaCodec#setCallback(android.media.MediaCodec.Callback,%20android.os.Handler)).

## Presentation and packet-recovery diagnostics

WebRTC HDR output uses a Choreographer clock and bounded future presentation
times, without sleeping on the delivery path or holding extra Java codec slots.
Normal headroom is capped at three refresh periods. A delivery burst cannot
extend that horizon. Once a network gap exhausts the old schedule, the first
recovered picture targets one refresh period earlier; subsequent output restores
normal headroom. Missing or stale display clocks fall back to immediate output.
NVST and SDR presentation retain their existing paths.

Signed RTP loss corrections are retained when late/duplicate packets reduce
`packetsLost`; only invalid/reset counters reset the window. Rolling display
values remain bounded and recovery requests retain their existing sustained-loss
threshold and cooldown. Allowlisted WebRTC reports record NACK, RTX, FEC,
dropped frames, freezes, assembly and jitter-buffer measurements, with unavailable
fields represented as unknown. SDP summaries retain capability flags only.

Debug-only HDR frame-flow traces distinguish input arrival, queue submission,
hardware output callback, renderer delivery and release. Stage events are bounded
and reset with the decoder epoch. All stage times use a local monotonic clock;
source timestamps are represented as deltas. Codec turnaround includes callback
delivery, and renderer delivery is distinct from final screen presentation.

These changes cannot eliminate upstream packet loss or guarantee a constant
120 FPS. Short HEVC/HDR tests on an Android 16 Snapdragon tablet measured hardware
turnaround below 2ms and display intervals predominantly near 8.33ms; occasional
long gaps still occurred before decoder input. Network conditions changed between
runs, so those observations are not an isolated benchmark of this patch.

## Validation

From `android/`, with the documented SDK/NDK, CMake and Rust targets available:

```sh
./gradlew :buildSrc:test :app:testDebugUnitTest :app:assembleDebug :app:assembleDebugAndroidTest
cargo test --locked --workspace --manifest-path nvst/Cargo.toml
```

The APK build cross-compiles the native library for all four packaged ABIs.
Host Rust tests also need CMake on PATH; the SDK CMake installation can supply it.

The PR branch was validated against upstream Android commit `7935744`:
1,213 app unit tests, 6 build-logic tests and 147 Rust workspace tests passed
with no failures or skips. APK and instrumentation APK assembly succeeded,
including native release libraries for all four ABIs. The APK manifest and
generated configuration retain `com.opencloudgaming.opennow`, its update-provider
authority and enabled APK update support. Full Android lint was not run;
existing deprecation warnings remain.

New tests cover USB framing/metadata, motor order, controller ownership, bounded
permission requests and lifecycle transitions; HDR profile/signaling, borrowed
payload ownership, reference metadata, stale buffers, presentation timing and
long-running/burst clocks; signed loss corrections and recovery-report epochs.

Four added instrumentation classes cover five device tests: real HEVC/AV1
callback decoding with stale Surface/codec rejection, shared SDR decoder
configuration, a real Choreographer thread's stop/restart lifecycle, and startup
with a context that has no USB service. The codec
fixtures are locally generated synthetic frames; see
[fixture generation](app/src/androidTest/assets/HDR_FIXTURES.md).
These tests validate codec lifecycle, not cloud AV1 end-to-end performance.
All five ran successfully on the Android 16 Snapdragon tablet with this branch
installed under the original application ID, with no skipped tests.

Earlier device acceptance verified V3 Pro game rumble on WebRTC and NVST and
the already-authorized USB startup path. A real ungranted automatic permission
dialog, separate XL hardware, additional firmware/devices and sustained
full-bitrate gameplay remain further acceptance work. The policy's denial and
reopening cases are covered by unit tests.
