# Native Qt: audio works, video is blank

The Windows capture from 2026-09-04 completed RTSPS OPTIONS, DESCRIBE, SETUP,
ANNOUNCE and PLAY, as well as ICE/DTLS/SCTP. The separate Mjolnir video socket
reported `inbound=0`, `auth=0`, `frames=0` through both eight-second receive
timeouts. This is a video **delivery** failure before authentication or decoding,
not evidence of an unsupported GPU or codec. Subsequent OPTIONS 400 responses
and an HTTP 503 are control-channel failures during recovery.

## Client corrections

- Unicast UDP reservations do not enable address/port sharing. Windows explicitly
  uses `SO_EXCLUSIVEADDRUSE`; an occupied video or bundle port triggers the bounded
  adjacent-port fallback, rather than an ambiguous shared bind.
- ANNOUNCE's local address follows the negotiated bundle peer's route, not the
  route to a public DNS server. This matters on split-tunnel/multi-NIC systems.
- Seat readiness, PLAY success and audio/control traffic do not reset the recovery
  budget. A failure's error/stopped notifications count once; automatic recovery
  is bounded to eight attempts through active-session discovery, claim and ready
  polling before media restarts. Only the first converted
  video frame, a new session, or explicit user retry resets the episode.
- Both Qt stream screens keep the active video item visible while the recovery
  budget waits for video progress. The embedded D3D11 path emits its first-frame
  notification once after a successful GPU-frame conversion, not for bootstrap
  tokens that have no decoded frame.
- Receive counters, zero-datagram timeouts, decoder progress/failures and RTSPS
  failures reach `diagnostics/native-streamer.log` in packaged builds. No SRTP
  keys, ICE credentials or packet payloads are added to these diagnostics.

These corrections do not establish which network condition affected the remote
PC. A firewall, VPN, router or server can still prevent video UDP delivery. Do
not disable the firewall or change the user's VPN automatically.

## WARP on/off capture, 2026-09-09

The `1.0.0-nightly.332.1` capture has the same zero-video-datagram failure on both
routes. The tunnel route reports MTU 1300 and packet size 1232; after the tunnel
is disabled, the route reports MTU 1500 and packet size 1280. Bundle ICE/DTLS/SCTP
and audio succeed in both cases. These logs do not establish a regression in
packet sizing or identify which network device is blocking video.

Comparison with [OpenNOW-Mac's port-range handling](https://github.com/OpenCloudGaming/OpenNOW-Mac/blob/666bd4a3391e13b074b37eecfd61d568e9231d34/GFN/NVST/Rtsp/NvstRtspWireFormat.swift#L334-L348)
found a separate negotiation gap: the native client
discarded the upper bound of SETUP's `X-GS-ServerPort` range. It sent NATT probes
only to the first port and rejected media from any other port. An endpoint-dependent
NAT can consequently filter a reply from the second advertised port before it
reaches the receiver. OpenNOW now preserves the advertised range, sends the existing
authenticated probes to each port, and admits STUN/SRTP from that range on the
negotiated host. Authentication and replay checks still apply. Bundle routing,
packet sizing, socket ownership, and recovery limits are unchanged.

The internal `nvstVideo` handoff adds optional `videoPeerPortEnd`, inclusive of the
upper bound. Omission retains the single `videoPeerPort` behavior. At most 16
ports are allowed, matching the Mac reference's bound; an invalid SETUP range
retains only its valid first port, while an invalid handoff range is rejected.
Diagnostics include the negotiated upper port, without credentials or payloads.

Loopback regression tests require an authenticated NATT probe on the selected
server port before releasing an authenticated video frame. They cover the
original single-port behavior and a server answering only on the second port,
with both packet sizes from the capture. This verifies the port-range correction,
not live WARP interoperability. The affected PC still needs fresh-session retests
with WARP enabled and disabled; success requires authenticated video, assembled
frames, and visible playback in both cases.

## Verification

Run the native streamer workspace tests and `opennow-embedded-orchestration-tests`.
The latter executes ShellStore recovery functions and each screen's status
calculation, including duplicate terminal events, repeated ready-seat replies,
retry exhaustion, manual retry and video-progress reset. Transport tests exercise
exclusive ownership, occupied-port fallback, negotiated-peer routing and NATT
wire bytes. The Windows media test checks one-shot first-frame reporting.

On the affected Windows 11 / GTX 1650 PC, retest H.264 1920x1080 at 120 FPS using
the complete rebuilt package in a fresh folder. Keep the same settings initially.
Check windowed/fullscreen playback and F3/Ctrl+G overlays without replacing the
video surface. Success requires rising authenticated/assembled video counters,
decoder `produced` progress, visible video and working audio/input.

If `inbound=0` persists, inspect application-specific firewall permissions, VPN
routes and competing UDP listeners; preserve the new log before another attempt.
The private local IP alone is not proof that a VPN caused the failure. An HTTP 503
does not justify changing decoder settings or unbounded session retries.

For deep Windows worktree paths, CMake accepts `OPENNOW_STREAMER_TARGET_DIR` as a
cache path override. A shorter artifact directory avoids MSBuild MAX_PATH errors
in bundled media dependency builds without changing the application's runtime.
