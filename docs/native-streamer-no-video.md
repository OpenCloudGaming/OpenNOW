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
  budget. A failure's error/stopped notifications count once; two media retries
  and two session claims exhaust automatic recovery. Only the first converted
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
