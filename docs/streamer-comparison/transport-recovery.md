# NVST transport recovery

These changes follow the static macOS/Windows parity audit at dev `7ce3cd54`.
Evidence references name functions in the research reports, not live captures.
No official client was run and no NVIDIA/GFN connection was made.

## Recovery policy

Pending reference-picture invalidation ranges retry every 100 ms, measured from
the later of queueing the range and successful local control admission. Sender
frame-number gaps start invalidation directly, as packet-level loss already did.
The first loss fixes the episode's 250 ms IDR-backstop deadline; extending the
range and retrying it do not move that deadline. An assembled recovery frame
discards pending invalidations. IDR requests have their own 100 ms retry timer.

RFI remains available for every typed-frame episode. A late response is not proof
that the server lacks RFI support. No unsupported-episode threshold is introduced:
the audit verifies repeated pending requests, not a threshold that permanently
disables the feature. The 250 ms backstop and unconditional RefPic-I clearing are
retained OpenNOW policies, not claims of full GFN policy parity.

The audit's TR15/TR16/TR18 cite **D** macOS `0xf4960`, `0xf3c80` and Windows
`0x18071e160`, `0x180721240`, `0x180721fd0`, `0x180722720`. The collector retry
constants at macOS `0xd6ae88` and Windows `0x1812f1d18` are 100.0 ms.

## Packet recovery and liveness

NACK tracking is bounded to 2048 individual packet indices. Overlapping requests
do not reset an existing packet's original deadline, attempt count, or retry
timer. Resolving holes removes entries rather than splitting range-owned state.
Overflow requests IDR and preserves already tracked losses. Each report still
contains at most 64 contiguous missing packets. The encoder already places
`stream_index` in byte 1 of the v2 payload; no wire-format change is needed.
Receive diagnostics expose pending-packet count and tracker overflow count.

TR09/TR10/TR11 cite **D** macOS `0x256310`, `0x25eae0`, `0x25eb30`, `0xf6d730`
and Windows `0x18047ec60`, `0x1806c9120`; Windows startup logs establish 2048
pending requests, a 52 ms wait, three attempts, and 4 ms extra retry backoff.

Both plain reorder and FEC queues are polled from the socket loops. A gap that
already has a 52 ms deadline expires without requiring another packet, allowing
buffered successors to proceed through the existing discontinuity/RFI path.
TR13 cites **D** macOS timed source-queue operations and **L** Windows
`NACK waiting queue limit time: 52 ms`.

FEC blocks wait for retransmission side by side. A block's 52 ms wait starts
when a packet from a later block arrives, and a hole's when the block behind it
arrives, not when the gap reaches the head of the queue. Each gap is requested
as soon as it is evident, with up to eight NACK reports per 4 ms tick, and a
stalled block asks for every missing data run it needs to reconstruct, because
its wait is not extended for a second round. Giving up on one gap forgets only
that gap's requests. The queue reports one discontinuity at a time, so a
complete frame between two abandoned gaps is still delivered.

Before this, a damaged block started a fresh 52 ms wait, and its first NACK,
only after the block ahead of it had been resolved. A loss that damaged N
blocks held the queue for N × 52 ms, together with the invalidation request
(queued when the next frame assembles) and the recovery frame the server sends
in reply. In five 2-minute 3840×2160 @ 120 AV1 sessions over a link that drops
packets every 15 s, 21 of 48 loss episodes gave up on two or more gaps in
series before the first invalidation left, and the six longest episodes
(0.65–1.5 s from the first gap to a usable frame) gave up on 15 to 66 gaps
each. The plain reorder queue, used only when a stream carries no FEC layout,
still waits per gap in sequence.

Unified-socket Mjolnir calls the same produced-frame watchdog and publishes the
same typed stall and recovery events as the dedicated receiver. Authenticated
partial video can no longer keep that route frozen indefinitely while resetting
packet liveness. TR28 is verified by reading both OpenNOW receive loops; the
existing watchdog deadlines are unchanged.

Prepared-session video startup waits for the existing PLAY-completion signal.
The bundle records that signal in the feedback state shared with dedicated
Mjolnir, so the first-video budget begins at PLAY rather than worker creation.
Early authenticated partial video does not start a frame-progress deadline before
PLAY or consume the first-video allowance. Only the first authenticated video
packet after PLAY starts the idle and frame-progress budgets. Sessions without
the PLAY gate retain their immediate startup deadline.
The core's bounded PLAY gate still owns failure when PLAY never completes and
stops both workers on startup failure.

## Header parsing and deferred layouts

The canonical header parser requires signature bytes `0x47 0x53` and decodes
frame type from the low nibble of byte 3. Nonzero priority cannot hide a recovery
frame. TR06 cites **D** macOS `GsFrameHeader` serialization at `0x23c500` and
`0x23c540`. Short `0x01` and cloud `0x81` headers retain their existing byte-3
interpretation because the audit does not verify the canonical conversion for
those representations.

The following remain deferred:

- Sender capture time: the reports establish a u32 field but do not provide its
  canonical offset/endianness. The compact Mjolnir representation is different.
  No offset, unit, or local replacement timestamp is invented.
- Recovery-mode `0x312`: TR17 establishes a 16-byte payload and enter/reset
  branches, but the mounted reports do not give the complete field layout.
- RefPic-I configuration-dependent clearing and recovery-mode trigger thresholds
  require the negotiation owner's typed policy; this transport-only change does
  not add DESCRIBE or ANNOUNCE fields.
- NACK feasibility toggles and the initial 1 ms send delay are unchanged. The
  shipped change fixes tracking lifetime, capacity, overflow, and timer drain,
  rather than changing the rest of the feedback policy.

Focused tests cover prioritized canonical recovery frames, sender-only gaps,
RFI retransmission/deadline preservation, later RFI episodes, per-packet NACK
aging/attempt limits/overflow, idle reorder/FEC expiry, unified-loop watchdog
events, and delayed-PLAY startup budgets using only local authenticated packet
fixtures. The PLAY-clock change follows the existing OpenNOW coordinator's
ownership contract; it is not a new GFN timeout value.
