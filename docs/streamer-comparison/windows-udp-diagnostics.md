# Windows UDP startup diagnostics

## Reservation versus reachability

The Qt process owns the embedded streamer's UDP sockets. Firewall rules for a
standalone `opennow-streamer.exe` do not describe permission for `OpenNOW.exe`.
The application never changes firewall policy or requests router port forwarding.

Reserve video 49005 and bundle 49006 before ANNOUNCE, retaining both sockets for
the receiver threads. If either bind fails, release the partial pair and try a
bounded number of dynamic adjacent pairs. On Windows use `SO_EXCLUSIVEADDRUSE`
before binding, not `SO_REUSEADDR`: the latter permits ambiguous delivery when
another socket is already using the address. The exclusive bind is the atomic
availability check; a separate probe/close/rebind would introduce a race.

A successful bind only proves local reservation, **not** firewall/NAT/server
reachability. Similarly, successful `send_to` only means the OS accepted the send.
Do not report either as "UDP open". Authenticated incoming video is the relevant
proof for the video leg; functioning bundle audio/DTLS does not prove video works.

## Evidence from the installed NVIDIA client

The local `GeForceNOW/geronimo.log.bak` trace dated 2026-09-02 shows:

- VIDEO SETUP and UDP RTP source bound on 49005 (lines 2176–2202).
- Early WebRTC/ICE bundle socket bound on 49006 (2211–2214).
- `useReserved=1`, `fallbackDynamic=1` (2396–2397).
- Version-6 NATT with standard STUN on both legs, with a three-send bundle burst.

These observations establish port preference and separate transport legs, not
that NVIDIA probes router/firewall policy before binding. A missing STUN reply
alone is not proof of failure: video receipt must be inspected independently.
Do not commit raw official traces; they contain ICE credentials and session data.

## Reading the exported native log

1. `nvst-udp bound`: local port, IPv4/IPv6, exclusive reservation, actual receive
   buffer size. `reachability=unverified` is intentional.
2. `bind-unavailable`: requested port, OS error code and error kind; followed by
   preferred/dynamic pair selection. Neither socket is released after success.
3. `rtsps`: method, response status and elapsed stage time. An OPTIONS rejection
   precedes media activation and is distinct from UDP/video timeout.
4. `nvst-handoff`: local and remote **ports**, whether peer hosts match, ping
   version/length/class, SRTP profile, and feedback routing. No credentials/IPs.
5. `receiver-start` / `first-inbound`: leg, local/remote ports, whether the sender
   matches the negotiated endpoint, bytes and elapsed time. First receipt is
   logged before authentication, explicitly labelled as such.
6. `nvst-video` / `nvst-bundle`: bounded two-second counters for received traffic,
   authentication, frames, hole punches, ICE/DTLS/SCTP and feedback.
7. `media-timeout`: final stage counters, with firewall status explicitly unknown.
8. `present`: retains a bounded platform error description before conversion to
   the generic FFI status, separating decoder readiness from actual render faults.

For a remote report, export diagnostics immediately after one failed attempt and
preserve all three logs (`current.log`, `native-streamer.log`, `qt-native.log`).
Record the exact executable path, selected backend/codec, region and whether
audio was received. Never request tokens, passwords, or unredacted SDP.

Reference: [Microsoft SO_EXCLUSIVEADDRUSE documentation](https://learn.microsoft.com/en-us/windows/win32/winsock/so-exclusiveaddruse).

## Local Qt retest, 2026-09-05

The rebuilt Qt executable retained exclusive 49005/49006 sockets (confirmed by
Windows UDP endpoint ownership). Both peers were negotiated on remote port 5004.
The first bundle datagram arrived after 33 ms; the first video datagram after
586 ms. At six seconds video had received/authenticated 3,386 datagrams and
assembled 302 frames. Bundle DTLS/SCTP were ready. These are local observations,
not proof of the remote user's network conditions.

Presentation still failed independently: the new error detail was
`GPU frame recording failed: the D3D11 decoder has not produced a frame yet`.
`EmbeddedD3d11State::record` returns this ordinary asynchronous startup condition
as a string error; `GraphicsRuntime::record` wraps it as `RecordFailed`, and the
FFI maps it to `RenderFailed`. Qt then attempts `start` while the engine remains
connected, which is rejected as `invalid-state`. This was a readiness contract
bug, not a local UDP failure. The fix adds typed `GraphicsFrameError::NotReady`,
maps it to the existing FFI `NoFrame` result, and lets Qt retain its surface until
the asynchronous decoder publishes its next frame-ready event. Other errors
remain `RecordFailed`/`RenderFailed`; no string matching or blanket error
suppression is used. Regression coverage exercises repeated not-ready output,
successful delivery on the same graphics context, and a real rendering failure.

An earlier attempt in this retest received OPTIONS 400 before media activation;
ending that cloud session and starting a new one reached PLAY 200. The supplied
remote capture instead reached PLAY/DTLS/SCTP but had zero video datagrams while
audio continued. Its cause is not established by the local rendering failure.

### Readiness fix live verification

The patched Windows Qt build rendered the remote Steam account-selection screen
in the Trove session at 2560x1440/H.264. The log recorded the expected pending
decoder result at `1788639916921`, followed by `first-frame status=streaming` at
`1788639916957` (36 ms later), without a restart. The image remained visible with
the local session menu, in fullscreen, with F3 statistics, and after returning to
windowed mode. Authentication was not automated, so this verifies streamed video
presentation and overlay/resize continuity, not signed-in gameplay.

### Startup keyframe recovery

The subsequent remote capture (`native-streamer (3).log`) authenticated 3,156
video packets by 54 seconds, but the media consumer received zero keyframes
through shutdown at 56 seconds. Qt initialized its D3D11 context without a
first-frame event. This does not establish whether the keyframe was lost on the
network or mishandled during assembly.

The feedback path had two independently verifiable gaps: both PLI and control
IDR waited for a video SSRC, and a successful enqueue consumed the request
without confirming keyframe receipt. Startup now arms recovery before channel
opening; control IDR can send without a video SSRC, while PLI still requires the
authenticated stream identifier. Successful and failed send attempts are limited
to the existing 250 ms cooldown. Requests remain pending until a complete
keyframe is assembled, and subsequent loss or decoder rejection can re-arm them.
The existing wire messages and sender identifiers are unchanged.

`nvst-keyframe` file diagnostics distinguish each enqueue attempt (PLI/IDR,
whether a stream is known) from actual keyframe assembly (sender frame ID and
size). Enqueue success is not reported as server acknowledgement or presentation.
Regression tests cover channel ordering, no video SSRC, cooldown/retry behavior,
non-keyframe arrival, completed keyframes and re-arming. Remote playback still
requires verification with the updated binary.
