# Windows Mjolnir packet and NACK layouts

All addresses below are in the separately supplied Windows `Bifrost2.dll`, image base `0x180000000`. These are Windows static findings, not layouts borrowed from macOS. Little-endian and big-endian labels are traced through the actual load/store helpers. The supplied logs provide runtime confirmation of the selected transport, not a packet capture.

## Mjolnir classifier and extension

**D.** `WebRtcTransport` classifier `0x180390c30` requires a non-null mbuf with at least 16 remaining bytes. It saves the cursor, sets the cursor to absolute offset 12, reads a raw u16 through `0x18085bc50`, applies `ntohs`, compares with `0x4753`, and restores the cursor. The tested wire bytes are therefore `47 53`, ASCII `GS`. This is a classifier, not proof that the full RTP header, CSRC count, extension length, or payload is valid.

The macOS classifier compares `0x5347` after its helper read. The earlier macOS study left that helper's byte order unresolved. The Windows conversion resolves its own on-wire marker; the different numeric comparison is not evidence of different wire bytes.

**D.** `MjolnirExtension::ReadFrom` at `0x18046a3e0` expects 16 bytes and copies the two raw 64-bit halves into the extension object at `+0x10` and `+0x18`. The writer at `0x18046a210` copies those same 16 bytes out. There is no whole-word byte swap, so the Windows packed words are little-endian.

For extension-payload offsets, FEC processing at `0x180749930` proves these fields:

| Payload offset | Encoding | Meaning supported by the FEC consumer |
| --- | --- | --- |
| 4 | little-endian u32 | FEC ID used to select the current group |
| 8 | bits 28-29 of little-endian u32 | Index of FEC group |
| 8 | bits 30-31 of little-endian u32 | Number-of-groups field |
| 12 | bits 4-11 of little-endian u32 | FEC percentage |
| 12 | bits 12-21 of little-endian u32 | Packet index in the FEC group |
| 12 | bits 22-31 of little-endian u32 | Number of original source packets |

The last three are equivalently bits 36-43, 44-53 and 54-63 of the u64 at payload offset 8. This matches the macOS getter results. `0x180749930` rejects zero or over-limit source counts and packet indexes beyond configured capacity. Do not infer group-count conventions, a complete recovery algorithm, or a frame-ID relationship merely from the bit widths.

## GS frame header

**D.** `MjolnirGsFrameHeader::ReadFrom` at `0x18046a500` and `WriteBody` at `0x18046a580` consume and produce six bytes:

| Offset | Bytes | Encoding |
| --- | --- | --- |
| 0 | 1 | Slice count |
| 1 | 1 | Frame type in the high nibble, priority in the low nibble |
| 2 | 4 | Sender-authored capture-time u32, big-endian |

The raw u32 mbuf reader `0x18085bcc0` and writer `0x18085c510` perform host-order loads/stores. The frame-header functions explicitly apply `ntohl`/`htonl`. The capture-time unit, epoch, and wrap handling at every consumer are not established by these functions. Preserve the sender value; do not replace it with a local timestamp.

The six-byte layout agrees with macOS. Windows closes the earlier byte-order uncertainty. Header-extension commands, their separate lengths, and codec depacketization must not be folded into this six-byte header.

## NACK v2, control command 0x317

**D.** `RtpSourceQueueExtV2::createNackRequest` at `0x18047a220` rejects an empty sequence vector or null output. Only requested version 2 has a successful path. It copies the vector, processes at most its first 64 entries, and does not sort it. A record contains a base sequence number and a 64-bit mask. Bit `d-1` represents a missing sequence `d` after the base. Serial-number comparisons and the 16-bit wrap branch determine whether to reuse a record.

`NvscClientPipeline::createAndSendNackRequest` at `0x18047ec60` initializes the header, invokes its receiver's serializer, limits the resulting record count to 64, and dispatches command `0x317` under the control-owner mutex. Length is `3 + 10 * record_count`, at most 643 bytes.

| Offset | Bytes | Encoding |
| --- | --- | --- |
| 0 | 1 | Version 2 |
| 1 | 1 | Low byte of the pipeline's stream index |
| 2 | 1 | Record count |
| `3 + 10k` | 2 | Base sequence number, little-endian |
| `5 + 10k` | 8 | Loss mask, little-endian |

Byte 1 comes from pipeline `+0xd2`. `0x180480260` names that same u16 field `streamIndex` in its stop diagnostic. This closes the corresponding Windows unknown. The macOS study only located its own byte source at pipeline `+0x282` and did not recover its semantic name.

The base and mask are direct integer stores with no network-order conversion. The control framing carries the serialized payload unchanged. This is NVST command `0x317`, not RTCP Generic NACK.

**D/L.** NACK tracking uses `NackTracker`, constructed at `0x1806c84c0`; S6 configures 2,048 pending missing-packet entries, 1 ms initial delay, 52 ms lifetime, 4 ms extra retry wait and retry value 3. Overflow handling at `0x1806c9120` clears the pending ledger and marks IDR pending rather than growing it indefinitely. The exact meaning of the retry value must be compared with the send counter, not just the word "retries" in a log.

OpenNOW's baseline retains 16 missing ranges, a separate bounded representation, and counts three total accepted sends. Its initial-send guard also differs. Those policies are not proved identical by matching the payload limit or the logged value 3.

## Limits

These layouts do not prove every selected-session packet, the exact server-side FEC algorithm, malformed-packet handling at all layers, or successful loss recovery under load. Runtime 0x301/0x302 sends were not logged in the supplied sessions. Their Windows serializers and recovery policy are described in [transport-feedback.md](transport-feedback.md), alongside frame acknowledgements and QoS.
