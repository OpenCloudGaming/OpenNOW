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
