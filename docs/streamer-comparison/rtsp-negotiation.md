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
