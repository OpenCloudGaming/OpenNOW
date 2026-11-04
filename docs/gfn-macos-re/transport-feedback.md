# Media transport, feedback, and packet metadata

The supplied Bifrost library implements more than one media/control arrangement. The inspected branches include RTC-compliant media, native Mjolnir video on a unified transport, and legacy per-stream sockets. These paths must not be inferred from the presence of a WebRTC helper or an OpenSSL symbol. All findings here refer to `archive/libBifrost2.dylib`, x86_64.

## Evidence anchors

| Alias | REA evidence ID | Coverage |
| --- | --- | --- |
| T1 | `ev_d95fac31d976eea57467f92a5dddf849b00c2a6bb6b93655d61a9f35f86f1e20` | Socket selection, NACK dispatch boundary, frame invalidation, IDR request |
| T2 | `ev_d1ab53c09326e0e81c00125c87528abf0c329f85fdad9a680d7a2cd8ae890255` | NACK v2 construction and send policy |
| T3 | `ev_792c051e5ffcc4bf87adfb785b5fd4963245b02f3a6a7189dd165cbb7e1713e0` | Unified receiver registration, input forwarding, FEC accessors |
| T4 | `ev_8ed5c66dce78c14cbfc5264bbec87da145ac66b6497ed00b6ba94318244014fd` | Receiver construction, receive handoff, packet classifier, RTP receiver fallback |
| T5 | `ev_761f6076e08acc843bc9793dcd243ef776c1f415542c8ddcfdec4afaf2dab740` | Bounded receive-queue insertion, Mjolnir handoff, microphone wrappers and RTP sender |
| T6 | `ev_465d152c2f71d13f3b966d35e011b74965e0b70174c6574e80f34130f41df10d` | NACK control command, frame-header read/write pair, metadata-command dispatch |

The query plan is [queries/transport.json](queries/transport.json). Function addresses below are virtual addresses in the exact thin image; [artifacts.md](artifacts.md) records its hash and original-container offset.

## Unified Mjolnir receive ownership

**D, T3, `SignalingHandler::setupWebRtcTransport`, `0x11fcd0`.** The unified branch constructs and registers `MjolnirVideoReceiver` early. **D, T4, `WebRtcTransport::isMjolnirPacket`, `0x1b82c0`.** The classifier rejects a null buffer, requires more than 15 remaining bytes, temporarily sets the mbuf cursor to offset 12, reads a 16-bit value with `mbuf_read_u16`, compares it with `0x5347`, and restores the original cursor. The helper's byte-order contract and every surrounding RTP-header validation were not traced; this is not a complete safe packet parser.

**D, T5, `handleMjolnirRtp`, `0x1b8320`.** A missing receiver takes a rate-limited diagnostic branch. An existing receiver receives the moved packet buffer through `receiveMjolnirPacket`; any buffer remaining after that call is returned to the mbuf pool. Ghidra warns about an inaccurate recovered load size in this function, so the ancillary timing/TOS argument types are not a recovered ABI.

**D, T4, `MjolnirVideoReceiver` constructor, `0x25d310`.** The receiver constructs `CNvQueue<MjolnirReceivedPacket,1024u>` with capacity `0x400`. **D, T4, receive function `0x25d510`, and T5, queue `Add` `0x25da10`.** The producer requests queue insertion with argument eight. `Add` converts that argument to nanoseconds with a factor of 1,000,000 and uses a steady-clock condition-variable deadline **8 ms after its initial clock sample**. Mutex acquisition is untimed, and thread scheduling can extend the operation beyond that deadline; this is not a maximum measured or guaranteed insertion duration.

If insertion fails, the receiver increments its drop counter, logs that the consumer may be too slow, and releases the unconsumed buffer. Successful insertion stores the moved buffer plus accompanying metadata, advances a wrapping queue index, increments occupancy, and notifies a consumer. A separate callback occurs when insertion changes an empty queue to nonempty. The receive function can also notify an interface-address change.

These observations establish bounded capacity and a condition-variable deadline. They do not establish a total producer-call time bound, the network-thread's full scheduling, queue-drain latency, every shutdown wakeup, or successful decoder recovery after a drop. **Recommendation:** preserve observable queue pressure and distinguish packet loss from decode/presentation failure; do not silently turn this into an unbounded queue.

## NACK v2 payload construction

**D, T1, `NvstRtpStreamReceiver::createNackRequest`, `0xcc390`.** The wrapper requires an existing source queue and a successful cast to `RtpSourceQueueExtV2`, then delegates. It returns failure otherwise.

**D, T2, `RtpSourceQueueExtV2::createNackRequest`, `0x256310`.** The inspected serializer rejects an empty missing-packet list or null output pointer. It accepts requested version value two; another version returns failure. It copies the supplied 16-bit sequence-number vector and processes at most its first **64 entries**. The visible body does not sort the vector.

The serializer writes a record count at output byte two. Each following record occupies **10 bytes**: a 16-bit base sequence number at `3 + 10*k` and a 64-bit additional-loss mask at `5 + 10*k`. The base itself is implicit. A subsequent distinct sequence contributes bit `(sequence - base - 1) & 63`, within the grouping conditions. Wraparound has a separate comparison branch; exact boundary behavior, duplicate handling, and the caller's input-order guarantee still need differential tests. These are x86_64 stores in the recovered serializer, not a complete outer transport wire specification.

**D, T2, `NvscClientPipeline::createAndSendNackRequest`, `0x13cc00`.** The pipeline initializes byte zero to two and byte one from pipeline offset `0x282`, zeroes the remainder, and requests version-two construction through an indirect virtual call at slot `0x50` of its stored receiver object. It permits no more than 64 resulting records, then sends **`3 + 10*record_count` bytes**. The concrete object bound to that call was not resolved, so the pipeline-to-wrapper-to-serializer chain is **I**, not a proved selected-session call chain. The meaning of the second pipeline byte is also unresolved; it must not be renamed a stream ID solely by analogy with another implementation. An over-count request is dropped and recorded in `InvalidNackRequestDroppedCount`; creation failure and send failure have distinct diagnostics.

**D, T6, `ServerControl::sendRtpNackRequest`, `0x133dc0`.** The control wrapper forwards that payload to its selected backend with command value **`0x317`**, the caller-supplied length, a configuration-derived byte, and another flag set to one. It holds a mutex around the dispatch. The selected backend's framing, reliability mode, encryption, and any byte transformation beyond this boundary remain **U**.

## Frame invalidation and reference recovery

**D, T1.** `ServerControl::sendFrameInvalidationRange`, `0x132c20`, passes two caller-supplied 64-bit values plus a trailing field to a **24-byte** payload under command **`0x301`**. `sendIdrRequest`, `0x132da0`, passes a caller-supplied 16-bit value in a **two-byte** payload under command **`0x302`**. Both dispatch through the selected control backend under a mutex.

These wrappers establish command values, argument widths, and dispatch lengths. They do not establish the callers' frame-ID domain, inclusive/exclusive range semantics, trailing padding contents, complete wire endianness, or the circumstances requiring IDR instead of invalidation. **Recommendation:** trace callers and preserve sender-authored identifiers before implementing either command. Do not substitute a local presentation counter.

## Mjolnir FEC accessors

**D, T3.** The following are in-memory accessors for `RtpExtensionHeaderMjolnir`. They are not a proof that incoming bytes can be read as a native C struct without conversion.

| Function | Address | Recovered operation |
| --- | --- | --- |
| `getFECId` | `0xcaa50` | 32-bit load at header offset four |
| `getIndexOfFecGroup` | `0xcaa60` | `(u32_at_8 >> 28) & 3` |
| `getNumOfFecGroup` | `0xcaa70` | `u32_at_8 >> 30` |
| `getFecPercent` | `0xcaa80` | `(u64_at_8 >> 36) & 255` |
| `getSrcPacketCount` | `0xcaaa0` | `u64_at_8 >> 54` |
| `getPacketIndex` | `0xcaab0` | `(u64_at_8 >> 44) & 1023` |

The getter names are supplied symbol evidence. Encoded count conventions, FEC algorithm selection, original-versus-repair packet identity, multi-group assembly, parser byte order, and the relationship between FEC ID and video frame ID remain **U**.

## Frame-header and command metadata

**D, T6, `MjolnirGsFrameHeader::ReadFrom` `0x1f1a80` and `WriteBody` `0x1f1b10`.** The writer emits a slice-count byte, a byte with frame type in the high nibble and priority in the low nibble, then a 32-bit `getFrameCaptureTime` value with an explicit byte swap around `mbuf_write_u32`. The reader consumes the corresponding byte fields and performs the corresponding 32-bit swap before constructing `GsFrameHeader`.

The observed body is six bytes; outer RTP-extension headers, alignment, and length validation are separate. The mbuf helper implementations and the timestamp's units, epoch, and wraparound policy remain **U**. A capture-time field is evidence to preserve, not permission to replace it with a local monotonic timestamp.

**D, T6, `GsFrameHeaderExtensionDecoder::decodeCommandPacket`, `0x136f00`.** Command values `0x100` and `0x101` select the `NvWacMetadataDecoder` family; `0x102` and `0x103` select `NvWacMetadataDeltaDecoder`. Other values delegate to the base decoder. The selected decoder is created lazily and reused. The body leaves its decoder call indirect. The full metadata schemas, codec-specific fields, and reset rules remain **U**.

## Microphone and RTP audio boundaries

**D, T5, `ClientSession::sendMicAudioFrame`, `0xffcf0`.** The wrapper holds a recursive mutex and delegates to an existing audio-player object. Without that object it reports `RtpAudioPlayer not started` and returns local result `0x800b0005`. The recovered parameter type misidentifies the object layout, so this is not a `NvstAudioFrame_t` ABI definition.

**D, T5, `NVB::Streamer::sendMicAudioFrame`, `0x8fad0`.** The outer wrapper checks a session/interface predicate and a microphone object. An enabled-state byte controls whether it invokes the lower call or returns success without sending. Lower-call failure maps to a local result value. These predicates do not establish which audio transport is selected.

**D, T5, `WebRtcAudioRtpSender::sendAudio`, `0x1d4b00`.** One timestamp branch uses `convertTimeUsTo48KHz` on the supplied 64-bit time. The other obtains the current timestamp and increments stored state by `param_3 * 48`; the units/source of `param_3` were not recovered from its caller. A configured byte selects plain versus redundant audio, and conditional sender/DLRR report calls follow. This proves a 48 kHz RTP-clock conversion path, not that all local capture devices or every session use one sample format. [audio.md](audio.md) describes the separately analyzed capture/AEC helper.

## Input, diagnostics, and remaining transport gaps

**D, T3, `NVB::Streamer::sendInputEvent`, `0x8f810`.** The input wrapper requires a valid session/interface predicate and input object, calls its lower input operation, and maps failures to a local result. On successful input, a one-shot idle-state branch queues `NVB_SN_CLEAR_IDLE_TIMEOUT` through the scheduler. The payload's wire schema, local-shortcut filtering, key-release synthesis, and disconnect neutralization are not recovered.

**S.** Symbol families identify H.264/H.265/AV1 depacketizers, SCTP transport, RTP receiver fallback, frame pacing, FEC, and statistics. Only the functions identified above were traced for their described behavior. Bundled QUIC/OpenSSL implementation names do not establish a QUIC media path. Application telemetry, local stage timing, and protocol feedback are different outputs.

**U.** Complete media packet formats, frame acknowledgment generation, pacing/QoS algorithms, SRTP replay behavior, loss-to-IDR thresholds, SCTP channel negotiation, reconnect queue flushing, and recording timestamp propagation remain unresolved. [unknowns.md](unknowns.md) lists the additional evidence needed.
