# Windows transport, recovery and feedback

The separately supplied `Bifrost2.dll` now provides Windows-derived wire layouts and policy branches. Addresses use image base `0x180000000`. **D** proves a static path, **L** proves an observed log event, and **U** remains unresolved. `NT/` means `native/opennow-streamer/crates/opennow-streamer-transport/src/`; `SC/` means the core crate's `src/`. OpenNOW citations refer to frozen dev `7ce3cd54`.

See [handshake.md](handshake.md) for sockets, DTLS and SCTP, [packet-layouts.md](packet-layouts.md) for Mjolnir, GS and NACK bytes, and [announce-builder.md](announce-builder.md) for conditional attribute emission. Geronimo's 21 Mjolnir strings are dependency build paths; the real media protocol is now inspected in Bifrost.

## Session evidence remains separate

**L.** S6 used dedicated SRTP video on client UDP 49005, with audio, microphone and six SCTP channels on the DTLS bundle socket 49006. Both used server port 5004 from SETUP, not `serverBundlePort 47999`. Unified mode was off. The DLL supports other arrangements; this log does not exercise all of them.

**L/I.** S5 disabled NACK at 19:27:21 and enabled it at 19:28:55 during high RTT. The configuration enables RTT-driven toggling. S5 recorded 16 NACK requests, 46 requested and received packets, 14 utilized and 6 retried. S6 recorded 3 requests, 8 requested and received, 4 utilized and no retries. These counters do not prove recovery or presentation of every lost frame.

**L.** Neither log prints a wire 0x301/0x302 command. Static code proves those serializers and the policy below, not their occurrence in these sessions.

Control layouts below describe the plaintext command buffer before the selected backend's transport framing. Multiple serializers directly write `u16 code`, `u16 payload length`, then the payload, and pass `length+4` onward. On Windows x64 these raw stores are little-endian. This does not mean encrypted or aggregated outer packets have only this four-byte header.

## 1. Normal invalidation is 0x301; NvWac invalidation is 0x30c

**D, `0x180721fd0`.** Normal invalidation admission checks the per-stream configuration byte at the referenced settings object `+0xa1`. If disabled, it does nothing. If manager byte `+0x74` selects the IDR-clear policy, it invokes a virtual operation with arguments `0,0` instead of expanding and sending an RFI range. The target fits the IDR signature, but that particular indirect target is **I** until its callsite slot is resolved.

Under normal RFI policy, a request expands the stored interval only if its start is below the current start or its end is above the current end. A fully contained request is ignored. The manager merges by minimum start and maximum end, not by creating independent disjoint ranges. When the interval was previously empty, it starts the RFI timer. If the first request starts above the virtual last-frame value plus one, it lowers the start to that value plus one. This is a real admission correction, not a log-only observation.

An empty interval is exactly `start=UINT64_MAX, end=0`. `0x180721ec0` tests that sentinel under the manager mutex; `0x180721e50` restores it. Admission stores the expanded interval before sending. A failed send therefore leaves an active range for later maintenance. If the pending-IDR byte `+0x1ac` is set and the separate predicate `0x180255e10` is true, the immediate invalidation send is suppressed while the range remains stored. The full meaning of that predicate is **U** here.

**D, `0x180721fd0` and `0x180721240`.** Normal RFI has this payload:

| Payload offset | Width | Value |
| --- | --- | --- |
| 0 | u64 LE | first lost sender frame number |
| 8 | u64 LE | last lost sender frame number |
| 16 | u64 LE | zero-extended manager stream index from its u16 field `+0xd8` |

The command is `0x301`, length `24`, total plaintext record length `28`. For a lost range 41 through 43 on stream zero, the header is `01 03 18 00`; payload words are LE `41,43,0`. The producer `0x18071e160` uses `[last_frame+1, current_frame-1]`, which establishes an inclusive range of missing sender frames. It is not a presentation-frame counter.

**D, `0x180721a80` and `0x180237ae0`.** These functions are a different path. They send command `0x30c`, length `6`, with `u16 stream`, `u16 start`, `u16 end` at offsets `0,2,4`. The logger calls it `NvWac invalidation`. The QoS method obtains the start/end from another object and clears its pending state only after a successful send. Do not describe `0x180721a80` as the normal 24-byte RFI sender.

**OpenNOW at `7ce3cd54`.** `NT/nvst_control.rs:166-182` implements the exact normal `0x301` payload, including the zero-extended stream index. `NT/nvst_invalidation.rs:230-248` merges sender ranges, and `NT/nvst.rs:2994-3010` restores a failed send. Thus T7's missing ANNOUNCE attributes are not evidence of missing RFI functionality. OpenNOW does not expose the IDR-clear policy or the separate NvWac command in these owners.

## 2. Reference withholding and recovery admission

**D, `0x18071e160`.** The manager processes the received sender frame number and frame-type value before deciding whether to withhold the frame. When RFI is enabled:

1. Types `2` and `4` are recovery candidates. Type `5` is also a candidate only when the `+0x74` IDR-clear policy is false. Under that policy, a complete type-5 reference picture does not clear the invalidation and the client keeps waiting for IDR.
2. A gap greater than one in sender-frame sequence expands `[previous+1,current-1]` only for a non-recovery candidate, then sets the invalidating byte `+0x68` and accumulates the lost-frame count.
3. A recovery candidate with either a pending invalidation interval or pending IDR clears the interval sentinel, resets invalidating/lost/withheld counters, and calls a session recovery operation. Type `2` additionally clears pending IDR and records that an I frame has been seen at `+0x1ad`.
4. With configured intra-refresh support, a non-I frame before the first I frame, excluding frame zero, requests IDR. This is an explicit startup branch.

The supplied type-name strings and OpenNOW's frame-type enum identify `2=I`, `3=Header`, `4=IntraRefresh`, and `5=RefPicI`. The numerical branch itself is the primary proof. The exact distinctions between an I descriptor and every codec's IDR bitstream are not established by these branches alone.

**D, `0x18071da50`.** An active invalidation or the separate withholding latch `+0x1ae` marks a frame status `0x20`, records the matching statistics flag, and increments the withheld-frame count. The latch also records `0x100`. Types `2` and `4` clear that latch. Type `3` overrides the withheld status with zero, so header frames are allowed through.

There is a configured QEC exception. It requires a previously seen I frame, no withholding latch, the QEC-enable byte `+0x808`, lost/withheld counts within two per-stream thresholds, and a successful `0x1807224e0` admission test against metadata and frame relationships. The manager then uses status `0x80000` instead of the drop status. `0x180714110` explicitly releases the assembled unit when its status equals `0x20`; it recognizes `0x80000` separately. The basic freeze is therefore proved at the media admission boundary. It is not merely the meaning inferred from a configuration string. The complete names and units of the QEC metadata fields are **U**; this research does not propose copying the exception without them.

`0x180714110` also has separate lossy-I and failed-I-parsing branches and sets a depacketizer latch. Its indirect recovery calls are not all resolved here, so these branches prove rejection and recovery intent, not every resulting control request.

**OpenNOW at `7ce3cd54`.** `NT/nvst_invalidation.rs:169-227` withholds dependent frames and allows headers, I, intra-refresh, and reference-picture recovery. This matches the basic non-QEC RFI policy, not the policy-selected type-5 exception or QEC exception. The baseline additionally treats a locally detected `frame.keyframe` as recovery. It has no equivalent pre-first-I admission rule in this module. Its unknown-type fallback admits the frame after requesting a keyframe and clearing the RFI episode. These are actual implementation differences, not a claim that the baseline has no recovery.

## 3. IDR suppression and active-request maintenance

**D, `0x180722720`.** An ordinary IDR request is suppressed while pending byte `+0x1ac` is set. A nonzero force argument bypasses that suppression. The third argument can latch the separate withholding byte `+0x1ae` if that latch is not already set.

When admitted, it sends through `0x180237bf0` if a control object exists, then calls `0x180722960(manager,1)` even if the send failed or there was no control object. That helper sets pending IDR and records its timestamp. Successful transport admission is not the condition for declaring the request pending. The force argument is consequently not a time threshold.

`0x180237bf0` proves command `0x302`, payload length `2`, containing the u16 stream index. Header bytes are `02 03 02 00`. Stream zero gives a six-byte record ending `00 00`.

**D, `0x180721240`.** This maintenance routine does not itself clear the RFI interval despite the original task label. It initializes previously zero RFI/IDR timer values. It resends pending IDR only when elapsed time is **strictly greater than 100 ms**; its successful send updates the IDR timestamp. It independently resends an active RFI interval after **strictly greater than 100 ms** since the RFI timestamp, updating that timestamp only on success. Both can be emitted in one invocation. A successful resend does not clear the corresponding pending state. An I/recovery frame does that later.

This 100 ms value is a retry boundary in this method. It is not proof that Bifrost always switches from RFI to IDR at 100 ms. The longer recovery-mode trigger and its complete timeout escalation need their own callers. In particular, the effective log's 6,000 ms recovery-mode threshold must not be substituted for this resend interval or treated as a recovered universal timeout.

**OpenNOW at `7ce3cd54`.** The baseline uses `KEYFRAME_REQUEST_COOLDOWN=250 ms` at `NT/nvst.rs:92`, and keeps retrying its pending keyframe request until recovery. `NT/nvst_invalidation.rs:142-164` triggers its RFI backstop after that cooldown and can permanently mark RFI unsupported for the session when its first sent episode produces no non-I recovery. `NT/nvst.rs:4543-4547` polls this RFI backstop on authenticated packets, not an independent timer. After a successfully sent unchanged range, the baseline does not periodically resend it; another range expansion or failed send is needed to queue it again. These are narrower differences from Bifrost's active maintenance. Do not present Bifrost's pending-IDR suppression as a lack of retry, or OpenNOW's 250 ms backstop as the same rule.

## 4. Frame state is not the 0x204 wire version

**D, `0x180806fd0`.** The public wrapper's `updateVideoFrameState` bookkeeping finds a sender frame in a 64-entry ring and groups supplied state values as follows:

| Supplied state values | Internal progress group |
| --- | --- |
| 1, 5, 6, 13 | 1 |
| 2, 4 | 2 |
| 3, 7, 8 | 3 |
| 9, 10, 11 | 4 |

It rejects a backward group transition except transition to group 3. State 11 bypasses the normal bookkeeping block. When stages are skipped it fills intermediate duration fields with marker `1`. State 4 writes `UINT32_MAX` to a decode-stage duration, and other named stages store clock-derived durations. This is local state bookkeeping, not a byte-for-byte control message or a proof that every state reaches one particular acknowledgement immediately.

**D, setters `0x1802762b0`, `0x180276400`, `0x180276550`, `0x180276870`, `0x1802769d0`, `0x180276b10`, `0x180276ce0`.** The actual client statistics owner uses a separate 256-slot ring of 216-byte per-frame records, indexed by `frame_number & 255`. Each setter verifies that the stored full frame number matches, so late callbacks cannot mutate a new frame occupying the same slot. The owner must be enabled. A zero supplied timestamp selects the current local clock; a nonzero timestamp is converted relative to a stored origin.

The named decode-result setter `0x180276550` treats its result argument zero as success and stores render-present time. Result one clears that timestamp and adds status bit `0x4`, logging decode failure. Any other nonzero result clears the timestamp and adds bit `0x100`, logging a skipped decode. `0x180276b10` and `0x180276ce0` add bit `0x8` on their explicit missing-presentation branch. RFI adds `0x20` through `0x1802795a0`. These are separate outcome flags, not the wrapper's progress groups.

**D, chain `0x180277f50` -> `0x1802781f0` -> `0x1802799b0` -> `0x18027a010`.** The log/drain policy builds client blob records from the per-frame ring. The producer stores 102-byte records in another bounded queue. With neither original nor recovered packet count for the frame, it creates the compact record version `3`; otherwise it uses the negotiated blob version. The serializer copies version `9` unchanged at 102 bytes. Other versions have distinct lengths: `3=12`, `4=72`, `5=84`, `6=90`, `7=94`, `8=98`, `9=102`.

`0x1802799b0` wraps that serialized record in command `0x204`. It only enters this old fixed-record drain for negotiated versions 4 through 9. A send failure tries to restore the popped record at the queue's front. The normal callback `0x1804841f0` asks it to drain up to 15 records. `0x180481df0` separately drains remaining decoded/transcode records during teardown. Version zero is rejected. Therefore `0x204` is outcome/timing feedback, not a proof that a frame was rendered simply because its network packets arrived.

### Proved version-9 payload fields

Offsets below are payload-relative. `F` denotes the 216-byte source frame record in `0x1802781f0`. Named stages are backed by the setter diagnostic and source offset, not guessed from the separate `RM_BLOB_DEF` schema.

| Offset | Width | Proven source or meaning |
| --- | --- | --- |
| 0 | u16 | record family `1` |
| 2 | u16 | blob version, `9` in this layout |
| 4 | u32 | sender frame number, `F+0` |
| 8 | u16 | statistics owner's stream index |
| 10 | u16 | copied frame field `F+0x96`; its full loss-count producer is U here |
| 12 | f64 | first original-packet receive time `F+8`, with recovered-packet-time fallback `F+0x10` |
| 20 | f32 | decoder-present delta, source timestamp `F+0x20` |
| 24 | f32 | delta from timestamp `F+0x78`; stage name U |
| 28 | f32 | render-present delta, `F+0x28` |
| 32 | f32 | render-start delta, `F+0x80` |
| 36 | f32 | render-completed delta, `F+0x50` |
| 40 | f32 | present-completed delta, `F+0x60` |
| 44 | f32 | present-completed-confirmed delta, `F+0x68` |
| 48 | f32 | ancillary value supplied to the render-completed setter, stored at `F+0xc0`; exact units U |
| 52 | f32 | cast of packet-record value `F+0x40`; exact meaning U |
| 56 | f32 | cast of frame-record value `F+0x38`; exact meaning U |
| 60 | u32 | frame receive duration from `F+0x88`; original last receive minus first receive in `0x180275540` |
| 64 | 8 bytes | copied source fields `F+0xb4`; their names U |
| 72 | u32 | frame byte-count field `F+4`; complete producer U in this pass |
| 76 | u32 | copied stats-owner field `+0x4aac`; name U |
| 80 | u32 | copied stats-owner field `+0x4ab0`; name U |
| 84 | u32 | outcome flags assembled below |
| 88 | u16 | recovered-packet count, low u16 of `F+0x98`, incremented by the recovered-packet branch in `0x180275540` |
| 90 | f32 | copied `F+0xcc`, updated by packet producer argument 8; name U |
| 94 | f32 | delta from timestamp `F+0x70`; stage name U |
| 98 | f32 | copied `F+0xd0`, updated by packet producer argument 9; name U |

Stage deltas use first receive as the common origin and clamp a valid stage earlier than receive to zero. Missing stage values retain negative-one sentinels where initialized. The status word is `(F.u8[0x35]*2) | (F.u32[0x30] & 0x7e9fc) | F.u8[0x34]`, additionally ORing `0x200` for the separate out-of-order byte `F+0x36`. This proves that decode failure `0x4`, skip `0x100`, invalid reference `0x20`, and presentation-loss `0x8` can survive into the serialized status word. Do not use the large metadata schema's separate record family/version to rename the still-unresolved fields in this table.

**OpenNOW at `7ce3cd54`.** `NT/nvst_control.rs:72-92` sends the same 102-byte family-1/version-9 shape and sender number, first-packet time, and frame-size slot. It sets stage slots `20..48` and `94,98` to `-1`, and leaves status word `84`, recovered-count slot `88`, and other telemetry zero. That is a deliberate unavailable-data encoding, not measured stage parity. `SC/lib.rs:2541-2549` acknowledges `MediaFeedback::VideoFrameAccepted` through `SC/lib.rs:169-171`, which calls `NT/nvst.rs:990-1020` to queue the record. This path does not prove a decode/presentation outcome. This is the important functional gap; neither matching payload length nor a callback called an acknowledgement closes it.

## 5. QoS v7 layout and measurement semantics

**D, `0x18071cf00`.** Feedback version becomes `min(server, configured, 7)`, with a server offer above seven clamped. `0x18071e910` supports layouts five, six, and seven and rejects other selected values. It emits v5/v6 payloads of 80 bytes and v7 of 52 bytes. Timing version offers below five disable client processing feedback; offers above five clamp to five, followed by the configured-version minimum.

**D, `0x18071e910`.** QoS admission requires a non-null output buffer and elapsed time since the previous report at least `configured_interval-5 ms`. It wraps command `0x207`. On successful buffer insertion it increments the sequence counter. After an attempted supported report, even failed insertion, it resets interval loss count, advances the packet baseline, and records the new time. Do not assume failed feedback retransmits the same measurement interval.

### Version-7 payload

`Q` denotes the QoS manager. All integers here are raw LE stores.

| Offset | Width | Proven source or computation |
| --- | --- | --- |
| 0 | u32 | version `7` |
| 4 | u8 | low byte of stream index `Q+0xd8` |
| 5..7 | 3 bytes | initialized zero |
| 8 | u32 | report sequence `Q+0x2c4`, incremented after insertion succeeds |
| 12 | u32 | last statistics frame number `Q+0x38` |
| 16 | u32 | virtual accessor result from object `Q+0x80`; accessor meaning U here |
| 20 | u32 | truncation of `Q.f64[0x60]*1000` |
| 24 | u16 | `Q+0xec` |
| 26 | u16 | `lost*10000/(latest_source_packet-previous_stats_packet)` when denominator positive |
| 28 | u8 | `Q+0xe0` |
| 29 | u8 | initialized zero |
| 30 | u16 | `min(65535,1000/Q.f64[0x198])` when positive; otherwise capped fallback |
| 32 | u16 | `min(65535,1000/Q.f64[0x1a0])` when positive; otherwise capped fallback |
| 34..35 | 2 bytes | initialized zero |
| 36 | u32 | client-clock conversion from `0x180723270`, 90 kHz |
| 40 | u32 | copied per-stream statistics-owner field `+0x4abc` when available |
| 44 | u32 | manager field `Q+0xe4` |
| 48 | u32 | second virtual accessor result from object `Q+0x80`; accessor meaning U here |

The named decode/render update functions `0x180722a00` and `0x180723080` maintain the `+0x198` and `+0x1a0` averages with configured smoothing, conditional good-network qualification, and caps. Thus offsets 30 and 32 are derived maximum decode/render rates, not fixed FPS or capability advertisements. The exact private fields behind offsets 16,20,24,28,40,44,48 should only receive domain names after the bandwidth-owner producers are traced. Existing OpenNOW names are not proof of the official producer's identity.

`0x180723270` converts a nanosecond clock relative to a stored origin to a 90 kHz count using 90000 and 1000 scalars, then the caller stores its low 32 bits. It does not substitute sender capture time. Its epoch differs from an arbitrary process-local counter unless both owners select the same origin.

**D, `0x1807223d0`.** The local bitrate/statistics report exposes manager `+0xe8`, the maximum of `+0xdc` and `+0xe8`, and a utilization value `100*e8/max` if that maximum is nonzero. The end of `0x18071e910` updates `+0xe8` using received-byte delta, elapsed time, and a 19/20 retention term. This is local statistics, separate from what `0x207` writes at offset 44. Do not equate every bitrate-named diagnostic with a command requesting a server target bitrate.

**OpenNOW at `7ce3cd54`.** `NT/nvst_control.rs:133-153` already emits version seven and this 52-byte offset shape. `NT/nvst.rs:1066-1104` computes interval loss and the local 90 kHz clock rather than filling both with constants. It uses the baseline bandwidth estimator for the other network fields and hardcodes 1000 in the decode/render-rate slots. T9 is not a v7-version mismatch. Measured decode/render capacity, per-frame outcomes, source-interval commit semantics, and the remaining bandwidth producer equivalence are separate parity questions. No evidence here establishes that OpenNOW's bandwidth estimator exactly implements Bifrost's estimator.

## 6. Timing v5 is a measured 28-byte report

**D, `0x18071fb10`, `0x180720550`, `0x180720280`, `0x180723610`, `0x1807232e0`.** With timing version five, the manager builds the following payload under command `0x203`. `0x180720550` only appends it when the server-selected feedback mode equals interval mode `1`. `0x180720280` provides a separate direct-send path through `0x180238dc0`, so do not conclude this is the only possible sender. The diagnostic still calls this fallback branch v4, but the body writes version five.

| Offset | Width | Proven value |
| --- | --- | --- |
| 0 | u32 | `5` |
| 4 | u32 | zero-extended u16 stream index |
| 8 | u32 | `current_frame-last_feedback_frame` |
| 12 | u32 | `last_feedback_frame+1` |
| 16 | u32 | computed total client-processing estimate |
| 20 | u32 | computed pacing/interval estimate from manager `+0x658` |
| 24 | u32 | copied manager field `+0x5c0`; exact label U |

The report covers actual progress since the prior committed feedback, not a constant two-frame window. Successful command insertion advances the feedback-frame baseline. The estimator records bounded per-frame samples from `0x18027ae90`, applies configuration-selected mean or variance/square-root estimation, clamps jitter/render estimates to configured bounds, adds the estimates, and converts using the one-million scalar. Units and exact public names of all configuration fields require their setters; the arithmetic is explicit in `0x180723610` and `0x1807232e0`.

**D, `0x180239040`, case `0x112`, and `0x180239e00`.** The inbound frame-pacing command must have payload length eight. Its `u32 stream index` selects the stream; `u32 mode` accepts zero or one and coerces another value to zero. A nonexistent stream is rejected. This is an active server control, not only an initial negotiation attribute.

**OpenNOW at `7ce3cd54`.** `NT/nvst_control.rs:95-112` has the correct 28-byte command/version shape but writes count `2`, the current supplied frame number, a fixed/default target, neutral pacing error zero, and constant `0x341a`. `NT/nvst.rs:7060-7073` deliberately avoids treating bursty packet completion as a display error. `NT/nvst.rs:1637-1646` uses the negotiated version presence to choose the default feedback enablement. It has no `0x112` parser in the inbound path examined at `7356-7454`. Calling this full timing-v5 parity is false even though the serializer version is correct. Do not replace the neutral value with network jitter; supply actual display/decode-owned measurements if implementing this later.

## 7. Keepalive, timer, QoS notification, and termination controls

**D, `0x180238a70`.** `0x200` carries a four-byte caller value and sends only when control byte `+0xe0` is enabled. The diagnostic calls this PingBackACK. It does not prove that this value is always a stream index.

**D, `0x180247620`.** The session's periodic PingBackACK routine uses the configured per-stream interval at settings `+0xa4`, obtains its payload through a stream accessor, and sends `0x200`. Success advances the successful-send timestamp and clears the failure count. Each failure increments the count; the **third failure** invokes one-shot termination with result `0x80040009` and returns `0x800c0011`. This is a control-send-admission failure policy, not evidence that the client failed to receive a response to a particular ping.

**D, `0x180248b00`.** The communication thread loops over configured video streams, performs PingBackACK, picture-refresh and video-timeout checks, then polls control with argument 1000. The visible control-receive result `2` leads to `0x180239040`; result `3` signals disconnect `0x80030023`. `0x180247e20` separately selects video-timeout results based on state, upstream/control predicates, and per-stream timeout. Do not reduce all of these failures to "keepalive timeout".

**D, `0x180239040`.** This parser establishes these controls. Integer timer fields are raw LE reads; the termination-v2 reason is explicitly converted through imported `Ordinal_14`, the WS2_32 ntohl ordinal, so it is BE on the wire.

| Direction | Code | Payload and branch proof |
| --- | --- | --- |
| client to server | `0x200` | 4 bytes, caller/stream-accessor u32; `0x180238a70`, `0x180247620` |
| client to server | `0x203` | timing feedback; v5 is 28 bytes as above |
| client to server | `0x204` | client frame blob; v9 is 102 bytes, v3 is 12 |
| client to server | `0x207` | QoS; v7 is 52 bytes |
| client to server | `0x209` | RL feedback; version 3, u32 stream, u32 count, then 16-byte records; `0x18071f5d0` |
| client to server | `0x20c` | 16 bytes: u32 stream at 0, byte zero at 4, unresolved padding 5..7, caller u32 values at 8 and 12; FEC state update `0x18071dff0` |
| client to server | `0x210` | 12-byte header containing u32 stream, zero, count; each record is u32 frame + byte ECN value, max 16 frames examined; `0x180720da0` |
| client to server | `0x301` | 24 bytes: inclusive u64 start/end and zero-extended stream index |
| client to server | `0x302` | 2 bytes: u16 stream index |
| client to server | `0x30c` | 6 bytes: u16 stream, u16 NvWac start, u16 NvWac end |
| client to server | `0x315` | 16 bytes: u32 stream, sender frame, detected packet count clamped to at least 3, ancillary value; throttling feedback `0x18071dcb0` |
| client to server | `0x317` | NACK v2: version byte, stream byte, record-count byte, then LE u16 base + LE u64 mask per 10-byte record; `0x18047a220`, `0x18047ec60` |
| client to server | `0x31f` | u16 caller field followed by the metadata schema string; `0x180238130`. This is not a frame ack |
| server to client | `0x101` | server QoS notification handled by `0x180256d90`, not termination; it updates per-stream QoS state |
| server to client | `0x102` | u16 original-command value, u16 outcome |
| server to client | `0x103` | u32 timer type at 0, u32 seconds left at 4 |
| server to client | `0x104` | u32 seconds left; emits timer type 4 |
| server to client | `0x105` | u32 seconds left; emits timer type 5 |
| server to client | `0x106` | u16 gamepad handling field |
| server to client | `0x109` | minimum 4 bytes; BE u32 termination reason, then remaining byte string passed onward |
| server to client | `0x112` | exactly 8 bytes; u32 stream and u32 frame-pacing mode |


The timer parser creates a notification and forwards it through `0x180255860`; it does not itself show a Qt-style dialog or decide resumability. `0x180239f50` ignores termination if its supplied session/config object is null, otherwise forwards it. `0x1802528d0` guards termination with `InitOnceBeginInitialize`, so concurrent/repeated notifications run the termination action once. `0x180252960` performs cleanup and schedules the asynchronous action. The UI's exact resumability rule for reasons `0x37/0x3b` and an extended code is **not** proved by these transport functions. Preserve the raw reason/message if implementing the missing parser rather than inventing that policy here.

**OpenNOW at `7ce3cd54`.** `NT/nvst_input.rs:25,234-235,393-395,1177-1179` already builds `0x200` with a four-byte LE value, and `NT/nvst.rs:6914-6920` sends value zero on a fixed three-second cadence. Failure only prints a message there. The baseline has other network liveness and send-error policies; "no timeout handling" is too broad. It lacks this per-stream cadence/payload accessor and third-control-send-failure policy.

The baseline inbound channel handling at `NT/nvst.rs:7356-7454` processes input protocol, haptics, Sony output, and cursors, with generic diagnostics afterward. It does not parse these timer, termination-v2, QoS-update, or frame-pacing controls. T2 is now supported by Windows-derived command IDs and field layouts. T3 remains a missing native reason-preservation/classification feature, but its exact claimed resumability rule still needs the higher-level consumer. `SC/lib.rs:2341-2366` emits `resumable:null` on native shutdown; separate CloudMatch resume handling must not be erased from the comparison.
