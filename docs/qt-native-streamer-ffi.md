# Qt native streamer lifecycle events

ABI version 13 delivers these events through the generic JSON event callback
without a new C ABI layout.
The native streamer owns wire parsing and termination classification. Qt does not
infer resumability from a wire code or reason.

## Session timers

```json
{"type":"session-timer","startId":"start-42","timerType":4,"secondsLeft":60,"active":true}
```

`timerType` is 4 or 5, `secondsLeft` is an unsigned 32-bit integer, and `active`
is a boolean. `NativeStreamRuntime` validates these fields and emits
`sessionTimerReceived(quint8, quint32, bool)` on the Qt thread. Malformed timer
events do not reach QML. `startId` must match the current start-command generation.
Timer events from a replaced or stopped session, or a previous runtime instance,
are discarded.

`ShellStore.acceptSessionTimer` owns warning state for the current stream. Type 4
with `active:true` starts a display-only countdown from the server-provided seconds.
The countdown clamps at zero and never clears the warning or ends the session.
Type 4 with `active:false` clears the warning after the transport confirms successful input. Type 5 clears
the warning. Qt neither clears on local input submission nor ends a session when
a timer arrives. Session end, replacement, native restart, and terminal native
status also clear the warning.

Both desktop and console compose `StreamIdleWarning` above the same
`StreamVideoItem`. It has no input handlers, focus requests, or overlay route, so
it does not pause gameplay input or recreate the session. A refreshed server timer
resets the display-only countdown.

## Server termination

Terminal error and status events can include this object:

```json
{"source":"nvst-server","code":"server-terminated","startId":"start-42","wireCode":2147680291,"reason":56,"extendedCode":2147680263,"resumable":false}
```

`reason` and `extendedCode` can be null. `resumable` can be true, false, or null.
The native streamer retains the numeric values through the existing JSON callback.
Qt rejects server termination events whose `startId` does not match the current
start-command generation, before invalidating input or presentation. The matching
error and stopped pair can both arrive after presentation has been invalidated.
Qt treats `nvst-server` with `code:"server-terminated"` and `resumable:false` as an
authoritative session end, cancels pending recovery, and never auto-resumes that
session. It retains the server message in `lastError` and the accessibility
announcement, and the full termination object in the streamer snapshot.
True or unknown classification follows the existing bounded
CloudMatch reconciliation path. Existing authoritative CloudMatch status 7 and
HTTP 404 handling remains unchanged.

The evidence baseline is `reports/opennow-gfn-parity/audio-input-lifecycle.md`,
rows L3 through L5, and Windows `transport-feedback.md`, section 8. Offline Qt
callback, orchestration, and smoke tests cover delivery and passive composition;
they do not connect to GFN or execute GFN binaries.

## Text retry feedback

`text-input-error` carries the current `startId` and a fixed `reason`, with no
paste contents. C++ rejects stale generations before forwarding it through the
existing `eventReceived` signal. ShellStore announces localized retry feedback,
without changing streamer state, input ownership, error telemetry, or incidents.
Desktop and console display it for five seconds in the existing passive input
notice, including when the Guide is closed. The normal stream video item and
gameplay focus remain unchanged. Temporary SCTP deferral queues one bounded paste
behind older input; only rejected local paste admission needs this notice.

## Co-op sessions (ABI 16)

ABI version 16 adds co-op hosting and joining on the existing JSON command and
event channel. No C symbol or struct layout changed; Qt requires version 16 so
it never sends co-op commands to an older streamer. `docs/coop-sessions.md`
section 7 is the contract. This section records what the native streamer does
with it.

### Host commands

The host commands run while this client streams a native NVST session. Every
command gets an `ok` response or an error with one of these codes:
`coop-unavailable` (no active NVST session), `coop-not-running`,
`coop-busy` (the bounded co-op command queue is full), or `invalid-coop-command`.

```json
{"id":"c1","type":"coop-host-start","iceServers":[{"urls":["stun:relay.example:3478"]}],"maxGuests":2,"relays":[{"id":"nl","name":"Netherlands","stunUrl":"stun:relay.example:3478","turnUrls":["turn:relay.example:3478?transport=udp"]}]}
{"id":"c2","type":"coop-peer-add","peerId":"usr_2","slot":1,"displayName":"Sam"}
{"id":"c3","type":"coop-signal-in","peerId":"usr_2","payload":{"kind":"answer","sdp":"v=0..."}}
{"id":"c4","type":"coop-peer-slot","peerId":"usr_2","slot":2}
{"id":"c5","type":"coop-peer-input-muted","peerId":"usr_2","muted":true}
{"id":"c6","type":"coop-peer-remove","peerId":"usr_2","reason":"kicked"}
{"id":"c7","type":"coop-host-stop"}
```

- `coop-host-start` is idempotent. A repeated start only applies `maxGuests`.
  It attaches a bounded tap (2048 packets) to the NVST receive path after SRTP
  decryption. When the co-op worker falls behind, the tap drops packets and
  counts them; it never blocks NVST receive. While no host runs, the tap costs
  one atomic load per packet.
- `coop-peer-add` makes the host the offerer. The offer and every trickled
  candidate arrive as `coop-signal-out`. A guest slot is registered with GFN only
  after that guest's link connects.
- `coop-peer-slot`, `coop-peer-input-muted`, `coop-peer-remove`, a guest `bye`,
  a link failure, and 500 ms without guest input all release the affected slot
  with a reliable neutral state before the registration bitmap changes.
- `coop-host-stop` and the normal `stop` remove every guest. Neither command
  changes the host's own stream.

### Guest commands

```json
{"id":"g1","type":"coop-guest-start","iceServers":[...],"relays":[...],"hostPeerId":"usr_1","slot":1}
{"id":"g2","type":"coop-signal-in","peerId":"usr_1","payload":{"kind":"offer","sdp":"v=0..."}}
{"id":"g3","type":"coop-guest-stop"}
```

`coop-guest-start` replaces `start`. It fails with `invalid-state` while this
client streams GFN, and with `coop-guest-active` while a guest session runs. A
`start` during a guest session fails with `coop-guest-active`. When the host
sends the stream description, the guest starts the normal embedded media
runtime. Surface, presentation, decode, and audio commands therefore work as
they do in a GFN session. Gamepad input uses `opennow_streamer_submit_gamepad`
with any controller id, and `opennow_streamer_submit_sony_snapshot` for Sony
controllers. While a guest session runs, a Sony snapshot is admitted as an
ordinary gamepad report with the same button layout and returns `Ok` (without a
guest session and without a GFN session it returns `Closed`). A guest never
sends rich Sony input. The guest forwards one controller: the first one to leave
neutral keeps the guest's slot until it returns to neutral or disconnects. Only
then can another controller take over, and the old controller's neutral state is
sent first. The guest sends a snapshot when the state changes and resends the
latest one every 50 ms. The Guide button emits `overlay-request` as usual.
Keyboard and mouse input are not forwarded.

### Events

- `coop-signal-out {peerId, payload}`. Payloads are opaque: offer, answer,
  candidate, end-of-candidates, and bye. They can hold SDP, so they are never
  logged.
- `coop-peer-state {peerId, state, path, relayId}`. The state is `connecting`,
  `connected`, `reconnecting`, `disconnected`, or `failed`. A peer that does not
  connect within 30 s fails. The event is sent again when the selected path
  changes.
- `coop-guest-state {state, path, relayId, reason}`. The guest moves through
  `negotiating`, `trying_direct`, then `trying_relay` when no direct pair
  connects within 4 s and TURN allocations start. `direct_blocked` and `relay` follow
  only when ICE nominates a relayed pair, so a direct pair that wins late goes straight to
  `connected`. Then `connected`, then `streaming` once the
  first decoded frame is presented. After that it can move to `reconnecting`
  (back to `connected` or `streaming`) or `ended`. `connected` is sent again when
  the path is first known. These are the `ended` reasons:

  | Reason | Meaning |
  |---|---|
  | `left` | The guest sent `coop-guest-stop`. |
  | `host-left` | The host sent `bye`. If the host gave its own reason, the guest reports that reason instead. |
  | `host-disconnected` | The link closed. |
  | `connect-timeout` | No connection within 30 s, or no reconnect within 15 s. |
  | `relay-unavailable` | No relay allocation succeeded. |
  | `relay-credentials-rejected` | A TURN server answered the allocation with 401 or 437. Fetch fresh `iceServers` once and join again. |
  | `media-unavailable: ...` | The guest cannot present the host's stream: an unsupported codec, or the local media runtime could not start a decoder for the described codec, size, depth and chroma. The text after the prefix is the platform's reason. The guest also sends the host a `bye` with this reason. |
  | `media-closed: ...` | The local media pipeline closed. The guest also sends the host a `bye` with this reason. |

- `path` is `direct`, `relay`, or null, taken from the ICE pair media is sent on:
  `relay` when either end of that pair is a TURN relay address. `relayId` is the
  configured relay site of the TURN server that carries the selected pair, whichever side
  allocated it. The host's `coop-peer-state` and `coop-host-stats` use the same rule.
- `coop-host-stats` and `coop-guest-stats` are sent at 1 Hz with the contract
  fields, including the cumulative input counters and the guest's `connectMs`,
  `firstFrameMs`, `codec` and `decoder` described in `docs/coop-sessions.md`. A
  guest logs `guest first frame` under `coop` with the times to connect, media
  start, first assembled keyframe and first presented frame, plus the keyframe
  requests it sent. The native file log includes them at most every 10 s,
  and each co-op worker also logs its input counters under the `coop` area. The host
  logs each guest's add and removal, and every 5 s per guest its state, slot, forwarded
  video packets, video frames (distinct RTP timestamps), audio packets, NACK repairs, the
  guest's keyframe requests and RTCP PLIs, send rate, and the GFN keyframe requests made
  for guests. The guest logs its input counters and the keyframe requests it sent (while
  waiting for the first frame, for its decoder, and for loss recovery) every 10 s.
  The file sink reopens `native-streamer.log` within 1 s after the Qt shell rotates it by
  renaming, so these lines keep landing in the current file. Every `coop-guest-state` and `coop-peer-state` transition is
  logged under `coop` with its state, path, relay id and (bounded) reason but no peer
  ids, as are the stream description a guest receives and the result of starting its
  local media. A host logs the reason of a guest's `bye`. `capacityKbps`
  is null because the peer link runs without bandwidth estimation. `lossPct` on
  the host comes from the guest's RTCP receiver reports. On the guest, it is the
  share of forwarded video packets still missing after NACK repair.
- A guest receives rumble for its own slot from the host and emits the existing
  `controller-rumble` event. The event has `controllerId:0` and the
  `coop-guest-start` command id as `startId`. On the host, rumble for a slot
  that a guest owns goes to that guest and not to a local controller.

### Media path

The host forwards each decrypted NVST video and audio packet byte for byte,
including the RTP header and the NV extension. Each one becomes the payload of
a P2P RTP packet whose sequence number, timestamp, and marker match the
original. FEC repair shards are not forwarded. The guest feeds the inner packets
through the same reorder, frame assembly, and bounded media consumer as an NVST
session. Guest NACKs are RTCP application feedback named `ONAK`, which carries
(PID, BLP) pairs. The host answers them from a 256-packet ring per guest. Guest
keyframe requests (`{"type":"keyframe-request"}` on `coop-control`, or RTCP PLI)
reach GFN at most once per second across all guests.

The stream description on `coop-control` has the contract fields plus
`audio.payloadType`, the negotiated GFN audio payload type. The guest uses it to
admit forwarded Opus and RED packets. `colorRange` is null: NVST does not
negotiate it, so the decoder reads it from the bitstream.
`colorSpace` is `bt2020` for HDR sessions and `bt709` otherwise.
`docs/coop-sessions.md` has not been changed for these additions.
