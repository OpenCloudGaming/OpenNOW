# Native RTSP negotiation

The native-owned NVST path prepares sockets, sends ANNOUNCE, starts the transport
workers, waits for authenticated transport readiness, and then sends PLAY. The
wait is bounded at 20 seconds on the native engine worker, not a Qt thread.
DTLS completion or authenticated media received from the negotiated peer can
establish readiness. ICE, a socket send, and an unauthenticated packet cannot.
PLAY must return 200; 455 is a failed startup. The existing reverse-direction
PLAY gate still prevents upstream application data before PLAY succeeds.
If the session terminates during the readiness wait, startup returns the failure
without sending PLAY.
On the DTLS bundle path, the shared video startup clock remains parked during
authentication and starts when PLAY succeeds. Both unified video and the
dedicated Mjolnir receiver share that clock.

These changes follow the macOS reverse-engineering report `handshake.md`,
H2 `0x11ea20` and H1 `0x1bcda0` for readiness, and H3 `0x121310` for PLAY.
The Windows S6 log also completes DTLS before PLAY.

ANNOUNCE suppresses a supported baseline attribute only when the emitted value
matches DESCRIBE. Main and `;;` feature sections use last-value precedence;
the `||` upstream-video offer is excluded. An offered unified-socket value of 0
does not erase a client value of 1 selected by bundle fallback. The official
serializer evidence is macOS `0x4599c0` and `0x3798c0`, and Windows
`0x1803f7970` and `0x1803f77b0` in the parity report `session-signaling.md`.
The transport's frame-pacing feedback mode follows the effective ANNOUNCE value,
including a client value that differs from the server baseline.

Server FEC and packet-pacing parameters are retained when supported. Malformed
values, unsupported pacing versions or modes, invalid booleans, percentages
outside 0..100, and inconsistent FEC ranges fail negotiation. If the server
omits the repair percentage, the existing percentage is constrained to its
negotiated minimum and maximum. Transport logs publish the effective encoding
parameters, including initial bitrate, initial peak bitrate, and client ceiling.
No SDP, keys, fingerprints, or credentials are logged by this diagnostic.

Explicit server initial and initial-peak rates are accepted within the client's
ceiling. Absent rates retain the existing ceiling-based defaults. Windows S6
has distinct values of 12135 and 48543 kbps in its merged config, but that one
sample does not establish a general startup ratio. No ratio is inferred, and
no Windows merged config is copied into ANNOUNCE. The source evidence is the
Windows research report `sdp-attributes.md` and `logs-timeline.md`.

## AV1 QP scale

The host's dynamic quality controls (DRC, DFC and GRC under `vqos[0]`) compare
the encoder's QP against thresholds that DESCRIBE offers on the H.264/H.265
scale (0..=51, for example `drc.upperQpThreshold` 39). AV1 QP runs 0..=255, so
against those thresholds every AV1 frame looks badly over-quantized and the
host keeps stepping quality down. With the `saveBandwidth` policy
(`dynamicStreamingMode` 1) at 3840×2160 @ 120, 8-bit AV1 received 70–99 fps
instead of 120, and 10-bit AV1 sessions ended with `0x800e8605` 7–20 s after
PLAY; neither happened with the values below. PR #1269 decodes that code as
`NVST_DISCONN_SERVER_VIDEO_PIPELINE_SCALE_FRAME_FAILED`. Those runs sent
`chromaFormat:1` for 4:2:0, which `docs/core-protocol.md` also ties to
`0x800e8605`; there the session ends before the first video frame, whereas
these ended mid-stream and only with the policy on.

When ANNOUNCE carries `vqos[0].bitStreamFormat:2` (AV1) and DESCRIBE's
`drc.upperQpThreshold` is at most 49 or absent, `av1_qp_settings` in
`native/opennow-streamer/crates/opennow-streamer-core/src/nvst_rtsp_negotiation.rs`
writes the official client's AV1 values as session attributes and logs
`av1-qp-settings applied=[...]`. A higher offer is already on the AV1 scale,
so ANNOUNCE is left unchanged and the log says `av1-qp-settings skipped`.

| Field | `drc` | `dfc` | `grc` |
|---|---|---|---|
| `minQpHeadroom` | 20 | 20 | 20 |
| `lowerQpThreshold` / `qpLowerLimit` | 100 | 100 | 100 |
| `upperQpThreshold` / `qpMaxUpperLimit` | 200 | 200 | 200 |
| `minAdaptiveQpThreshold` / `qpMinUpperLimit` | 200 if width × height ≥ 3440 × 1440, else 180 | same | same |
| `qpMaxResThresholdAdj` | offered value × 5, as a wrapping byte | same | same |
| `qpCodecThresholdAdj` | 0 | 0 | 0 |

- The resolution test compares pixel counts, as the official client does:
  2560×1600 gets 180, while 3840×1600 and 3200×1800 get 200.
- The official client scales its own merged `qpMaxResThresholdAdj`. OpenNOW
  scales only a value DESCRIBE offers and otherwise sends none, so the server's
  default stays. The byte wrap is kept: an offered 60 is sent as 44.
- All 18 values are sent even when one equals the DESCRIBE offer. None of them
  is in the server-negotiated list that ANNOUNCE omits when it matches DESCRIBE.

The values and the gate come from read-only static analysis of `libBifrost2.so`
in the official Linux client 2.0.89.141 (Flatpak `com.nvidia.geforcenow`),
BuildID `1e4ee18f55be056167757d72e407a7155362d84c`, SHA-256
`29754306f013488456ac688fd4e5cc58b798cd22778b054069d7bc74480511c7`. Addresses
are ELF virtual addresses in that file. `applyDefaultVideoFormatSpecificSettings`
at `0x344921` tests `vqos[0].bitStreamFormat` for 2 and the
`drc.upperQpThreshold` byte for at most `0x31`, then up to `0x344b4b` logs
"Applying default AV1 QP settings" (string at `0xdbecf0`) and writes the 18
bytes. The 200/180 choice uses the resolution-level table built at `0x348b7f`,
whose level 3 has a 3440×1440 minimum. The config-name setters store to the
same offsets, for example `vqos[0].drc.minQpHeadroom` (reference at
`0x36959e`) to vqos `+0x1f8`, `dfc.qpMaxUpperLimit` (`0x36d498`) to `+0x288`,
and `grc.minQpHeadroom` (`0x36ae34`) to `+0x230`. The official macOS client's
AV1 config dump also shows `drc` and `dfc` thresholds on this scale, for
example `drc.upperQpThreshold` and `dfc.qpMaxUpperLimit` 200; its `grc` values
were not collected. The same routine also sets `video[0].minQp` to 25 for AV1
and, by resolution level, changes `video[0].encoderPreset` and split-encode
settings; OpenNOW does not send those. No binary, raw log, or pseudocode is
committed.

## Additive internal handoff

`nvstVideo.recoveryPolicy` carries optional typed DESCRIBE values:

- `refPicInvalidation`, `enableRecoveryMode`, and
  `enableIntraRefreshInRecoveryMode` are booleans.
- `streamRecoveryType`, `framesWithInvalidRefThreshold`,
  `recoveryModeTriggerThresholdMs`, and `encoderLtrFeatureSetting` are unsigned
  32-bit integers.

Missing values remain absent semantically; no Windows or macOS defaults are
inserted. The transport exposes this policy without changing recovery behavior
in this negotiation change. Numeric recovery and LTR settings are preserved,
not interpreted as newly implemented capabilities. ANNOUNCE advertises existing
reference invalidation and intra-refresh support, but does not advertise a new
LTR mode or recovery-mode protocol. The evidence is the macOS configuration
field registrations and defaults, plus Windows `transport-feedback.md` config
values tagged L.

The handoff carries no microphone payload types. The `m=mic` section's rtpmap
describes the legacy RTSP media plane; the native bundle uses the fixed RTC
mapping (Opus 111, RED 63) that Bifrost hardcodes, so the transport sends the mic
as plain Opus on PT 111. `aqos.enableRedundancyForMic` and
`aqos.redundancyLevelForMic` are not applied to the uplink until a live seat
confirms it decodes OpenNOW's RED. Microphone negotiation
depends on the server's bundle offer, not whether local capture starts muted.
Negotiation does not enable capture. Windows `audio.md`, `sdp-attributes.md`,
and the mic-stream strings establish separate mic handling and RED depth 3;
the exact official mic m-line remains unknown.
Qt retains the mute control and its shortcut for a supported session that starts
muted. An acknowledged explicit unmute survives a reconnect of that same session;
a new session still uses its saved, default-off startup preference. The Windows
deferred-capture evidence is `0x14017f5f0`, recorded in parity row M6.

Endpoint fallback continues only for network failures, RTSP timeouts, or an
explicit temporary-service response. Authentication, malformed endpoints,
incompatible offers, and other semantic failures return the original error
without another endpoint attempt. This follows the macOS candidate loop in
`handshake.md` H4 `0x122780`; it does not change providers or TLS transport.

The public C ABI and versioned Qt/core JSON protocol are unchanged. This handoff
is an additive internal contract between the RTSP owner and native transport.
ANNOUNCE chroma and bit-depth encodings are unchanged pending live capture.
