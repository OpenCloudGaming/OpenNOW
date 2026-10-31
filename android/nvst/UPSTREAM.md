# Android NVST source provenance

Transport, protocol, and RTSP negotiation imported from OpenCloudGaming/OpenNOW dev
commit 3002e5c67 (2026-09-07). Transport is independently authored MIT code.

1.6.4 also imports the transport and regression-test portions of dev commit 52791d1d4
(H.264/FEC reference recovery). Reconstructed data shards must not be discarded as parity;
all damaged-picture paths invalidate references and request a fresh keyframe.

Android changes: JNI lifecycle/media adapter, bundled TLS trust roots, and attachment-only
RTSP disconnect (never send TEARDOWN on local stop or reconnect), keep the cursor
composited by the server, and retain Android's selected FPS up to 360. Android owns MediaCodec,
audio playback, UI, and CloudMatch session termination.

Transport selection and decoder startup correction (2026-10-31), compared with dev
5b89338a3 (`opennow-core/src/cloudmatch.rs` and platform-linux `src/session.rs`):
- The RMX3890 reports reach H.264/H.265 MediaCodec configuration, then a caught
  IllegalStateException with zero queued video buffers; reconnects receive OPTIONS 400.
  Their CloudMatch payloads explicitly report WebRTC despite RTSPS-shaped endpoints.
  Android now honors reported allocation transport for NVIDIA as well as partners,
  retaining that evidence in a bounded per-session cache across sparse poll/resume replies.
  Explicit user NVST opt-in still selects NVST; missing allocation metadata keeps the
  existing endpoint fallback. Request profiles and provider identities are unchanged.
- The direct-Java texture decoder retries a rejected tuned instance once with a fresh,
  untuned hardware decoder inside the same attachment, following dev's local decoder
  recovery ownership. Codec, dimensions, FPS and persisted settings are preserved.
- The pinned WebRTC 144.7559.14 releaseInternal skips codec release before running=true,
  which is set only after configure/start succeeds. The codec wrapper now releases failed
  startup exactly once on both tuned and untuned paths; failed cleanup prevents a retry.
  Platform configure/start causes and bounded Java traces are retained before WebRTC
  converts the error into a status code. WebRTC keeps ownership of its own fallback.
- If local video initialization/decode still throws, Android reports a decoder failure
  and stops locally, retaining the cloud session instead of re-opening RTSP or claiming
  another attachment. Transport errors keep their existing bounded recovery policy.

Regression tests cover NVIDIA's reported WebRTC allocation with NVST off, explicit NVST
opt-in, sparse-response retention, startup fallback and codec cleanup. Live playback on
the affected phone still needs verification; these changes do not rewrite NVST negotiation.

Fortnite native-touch allocation correction (2026-10-31), compared with iOS b3894f2bb
(`NativeStreamNVSTConfiguration.swift`): a Pixel 9a on Android 17 reproduced the remaining
failure with NVST off. TOUCH_FRIENDLY sent NVIDIA-CLASSIC/NATIVE headers and an Android
platform, but requested GSStreamerType=WebRTC. CloudMatch echoed that metadata while
allocating only RTSPS descriptors; the WebSocket registered but never produced an offer.
NVIDIA touch launches/resumes now use a browser client identity when NVST is off, and the
native touch identity when NVST is on. Launch input mode 3, digitizer provisioning, requested
geometry/FPS, saved transport preference and alliance identities are preserved.

With the corrected APK installed, a fresh Fortnite native-touch/WebRTC session produced
an offer, connected ICE and both input channels, rendered the Epic launcher at 58-60 FPS,
and accepted touch input opening its remote settings panel. The free-tier host negotiated
1366x768 despite the unchanged 1920x1200@60 request. Gameplay beyond Epic authentication
was not tested; no account password was supplied during this check.

A second fresh Fortnite launch with NVST explicitly enabled completed native negotiation,
decoded H.264 at the requested 1920x1200, sustained 59-60 FPS, and accepted a native touch
tap opening the same remote settings panel. The original NVST-off preference was restored
after testing. These checks verify both transport choices and remote launcher touch input
on the Pixel 9a, not gameplay beyond Epic authentication or the RMX3890 hardware decoder.

WebRTC startup fallback extension (2026-10-31): the RMX3890 2.2.8 report now receives
WebRTC offers, opens input channels and receives about 60 FPS, but c2.qti.avc.decoder
repeatedly rejects MediaCodec.start with vendor low-latency tuning enabled. The untuned
startup retry previously wrapped only direct-Java NVST. It now covers WebRTC too, preserves
its untuned choice across WebRTC Release/InitDecode cycles, and remembers rejected tuning
per hardware decoder for the factory lifetime. A failed release prevents another decoder
from starting; initialization/cleanup exceptions become status codes at the JNI boundary.
CodecException diagnostics now retain the platform diagnostic string, code, recoverability
and transience. Lifecycle regression tests cover this correction; untuned playback on the
affected RMX3890 still requires an updated build and device verification.

To refresh, compare the two crates and src/nvst_rtsp.rs against native/opennow-streamer
at the recorded upstream revision, retaining the Android lifecycle differences.

Android now sends native touch records through the reliable RemoteInput envelope, forwards
bounded server input notifications to the Android haptics parser, and initially advertises haptics
disabled until Android supplies the actual user/device capability. The microphone sender uses the
native-bundle path already announced by dev: MID 3 (the Android GFN microphone contract), Opus
PT 111 inside RFC 2198 RED PT 63, stereo 48 kHz / 20 ms / 16 kbps, with three prior blocks and
in-band FEC disabled. Opus is built from the Cargo-locked audiopus_sys sources, statically per ABI.

Rumble ServerControl framing (0x010b, reserved/controller/low-frequency/high-frequency) follows
wire facts documented in Moonlight's ControlStream.c; no Moonlight implementation code is copied:
https://github.com/moonlight-stream/moonlight-common-c/blob/master/src/ControlStream.c
Touch records and OC report layout reuse Android's existing input protocol. Mapping those records
onto native RemoteInput and the native descriptor indices needs live GFN/device verification;
unit tests cover framing, bounds, motor ordering, and Opus/RED round trips, not host acceptance.

Alliance native startup correction (2026-10-30), compared with dev 5b89338a3:
- YES CloudMatch identities now request secure RTSP and omit GSStreamerType=WebRTC
  in both CREATE and RESUME. The YES logs reached WSS registration/heartbeats but never an
  offer; the user confirmed the official Android app streams the same game. Existing reported
  WebRTC allocations retain their WebRTC interpretation; this changes new YES native requests.
  Other alliance providers retain their existing allocation defaults.
- Partner negotiation uses dev's legacy bundle NATT username plus decimal bundle port, retains
  a valid server-offered runtime key, and authenticates DTLS before finishing PLAY startup.
- Partner ANNOUNCE emits the selected enableUnifiedSocket value once. The old Android template
  appended a second hard-coded zero, contradicting the unified receiver on partner seats.
- Kotlin supplies the partner flag from the named CloudMatch provider base. Missing, direct-IP,
  and NVIDIA contexts do not enable these rules. NVIDIA request and RTSP negotiation behavior
  stay on the existing path. Startup waits and retained pre-PLAY events have bounded budgets;
  cancellation/failure closes only the local attachment and retains the cloud session.

Rust negotiation/transport tests and Android request-profile tests cover these changes. Live YES
startup and video still require verification with the updated build; the supplied logs predate it.

Startup validation on Android (2026-09-07) found `invalid-media-peer` when Android's WebRTC
fallback classified an RTSPS usage-14 control endpoint as the UDP bundle peer. RTSP connections
are now excluded from that fallback, matching dev's usage-2/17-only media selection; absent an
explicit media endpoint, RTSP SETUP supplies the native peer. Android native stage logs also
reach logcat under OpenNOWNvst. The Android NVST retry budget stops locally after three retries
instead of reopening the same session through unbounded CloudMatch recovery.

The patched APK was installed over ADB on a Xiaomi 22101320G. A fresh Satisfactory session
completed OPTIONS/DESCRIBE/SETUP/ANNOUNCE/PLAY with status 200 and reported Streaming at
2026-09-07 10:48:57 device time. Subsequent runtime samples continued decoding H.264 at
1920x1080 with no packet loss reported. This verifies live startup/video, not microphone or
physical touch/controller rumble behavior.

Input smoothing follows dev's nonblocking `queue_input` capture path instead of waiting for
an acknowledgement per packet. Android bounds outstanding native input to 128 packets and
drains at most 32 commands before servicing network I/O; permits are released on consumption,
failed enqueue, and shutdown. The bundle caches its socket timeout to avoid a setsockopt per
packet and logs peak input queue delay through the asynchronous Android diagnostic sink.
Android requests display-priority input/media workers and a Wi-Fi performance lock scoped to
the NVST attachment (low-latency mode on API 29+, high-performance mode on older devices).
The cursor remains server-composited. No stream quality settings are reduced.

Periodic str0m peer stats now forward the selected ICE pair's measured STUN RTT through JNI
to Android's ping field. Missing RTT stays unknown; region/discovery ping is not substituted.
The smoothness APK passes Android and Rust tests, but its latency and RTT display still need
live verification on the affected device.

1.6.4 defaults NVST off for all distributions and persists a one-time reset of previous APK
opt-ins. Later explicit opt-ins are retained. Reports from 1.6.3 showed 1366x768 H.264 output
while Android stamped the requested 1680x720 on every encoded frame, repeatedly triggering
AndroidVideoDecoder resolution reinitialization. Decoder initialization now uses negotiated
dimensions when available; encoded frames leave unparsed dimensions unspecified so the
bitstream determines them. The requested stream profile is preserved. Physical-device
verification of this correction and the reported large control/STUN RTT values is pending.

The NVST text adapter now starts UTF-8 after the four-byte input type: the single-event
marker was already removed by native_events. The previous five-byte offset discarded
single-character IME input and the first character of each pasted chunk. A failing-before,
passing-after regression reproduces Android's actual 0x22/type-23/UTF-8 framing.

Android's gamepad encoder now selectively incorporates upstream commit
086b505a8f21 (PR #837, native gamepad packets and registration). NVST 0x020d carries the
full 38-byte Android report in a length-prefixed v3 envelope, with the real slot, connected/XInput
bitmap, controls, timestamp, and independent 16-bit sequence. Reliable topology announcements
replace the old hard-coded descriptor values; removal neutralizes departing slots, and refused
channel writes restore encoder state so the next snapshot retries registration. A shared report
fixture checks the Kotlin encoder and Rust translation. This does not change WebRTC framing or
CloudMatch input provisioning. Live on-screen controller response remains to be verified.

Alliance streaming backport (2026-10-18), reviewed against dev 987ab3da4:
- bfc0cac50 (#882): enable TLS 1.2 for partner RTSPS endpoints.
- d041cd133 / e518df2c6 / 43c304919 (#998, #1022): bounded video SETUP URI/Transport
  compatibility, stop at the first 200, retain SETUP metadata and use CloudMatch's
  media peer when Transport is omitted. Native Mjolnir loss recovery uses private
  NACK-v2 on partial control and control IDR rather than RTCP Generic NACK/PLI.
- 7ce3cd549 (#1162): prefer usage-17 BUNDLE over usage-2 VIDEO for native ICE/DTLS,
  demux unified-socket Mjolnir video with its negotiated SRTP keys, and leave the
  separate video receiver inactive in unified mode. ANNOUNCE reflects that mode.
- WebRTC endpoint selection follows dev cloudmatch.rs: prefer explicit WebSocket/HTTPS
  and non-RTSP usage-14 descriptors, then native usage-14/16 converted to WSS 443.
  Preserve explicit WebSocket ports, IPv6 authorities and separate media mappings.

Android already defaults to WebRTC, so no desktop compatibility-mode preference or
provider identity policy was copied. Existing Android JNI/media/input ownership and
attachment-only disconnect remain. Scripted SETUP and encrypted UDP loopback tests
cover negotiation and media demux; ABYA/provider acceptance still needs a live session.
