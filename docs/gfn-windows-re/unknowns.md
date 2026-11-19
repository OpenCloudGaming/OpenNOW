# Unknowns after the Windows Bifrost pass

**U** means unresolved, not unsupported. The matching `Bifrost2.dll` is now supplied and analyzed. Missing-DLL disclaimers no longer apply. Static branches still do not supply an unlogged packet capture or prove behavior across every account, provider, GPU and failure mode.

## Questions closed by the DLL

| Earlier unknown | Windows evidence now available |
| --- | --- |
| CloudMatch methods, field ownership and usage conversion | POST/GET builders, typed models, finalized-feature conversion and JSON-to-native endpoint tables in [cloudmatch-native.md](cloudmatch-native.md). JSON usage 14 is SIGNALING, native RTSPS is 10. |
| Windows ANNOUNCE field policy and RFI support | Comparison against DESCRIBE, typed writers, V1-null skipping, V2 string fields and conditional recovery attributes in [announce-builder.md](announce-builder.md). |
| Mjolnir, GS and NACK layouts | Windows marker, raw 16-byte extension, six-byte GS header and 0x317 serializer in [packet-layouts.md](packet-layouts.md). |
| Normal invalidation, IDR and frame-ack bytes | 0x301/0x302, separate 0x30c NvWac invalidation, outcome-based v9 0x204 records and v7/v5 feedback in [transport-feedback.md](transport-feedback.md). |
| DTLS peer verification and SRTP export | SHA-256 certificate-fingerprint gate before key export, profile lengths and directional key/salt split in [handshake.md](handshake.md). |
| SCTP channel advertisements and command framing | DCEP reliability/priority/protocol fields, PPIDs and aggregated control framing in [handshake.md](handshake.md). Logged SIDs remain runtime allocation results. |
| No-video termination and setup-failure names | `0x80040010` is `NVST_NETERR_CLIENT_DISCONNECT_UNINTENDED_NO_VIDEO_PACKETS_RECENTLY`; `0xC0F52159` is `NVB_R_USER_STORAGE_NOT_AVAILABLE`, from service status 89. |

## Capture and session coverage

| Question | Current boundary | Evidence needed |
| --- | --- | --- |
| Exactly which 163 keys did S6 send in ANNOUNCE? | The conditional serializer is traced. The body is not printed and the complete DESCRIBE offer is not preserved. Final effective config is not the body. | A redacted signaling capture or trace at the serializer output. |
| Did S5/S6 send 0x301 or 0x302? | Both Windows paths exist; no log line prints their wire commands. Absence of a line is not proof of absence. | A control trace or a controlled loss session. |
| Does the server accept OpenNOW's alternate depth/chroma and DCEP values? | Static official-client differences are proved. No server-tolerance test was run. | An authorized, controlled comparison with captured negotiation. |
| What differs on alliance seats? | Every supplied session is NIMBUS. Native partner auth and passthrough branches exist, but no partner session exercises them. | A partner-seat trace with identifiers removed. |
| What does HDR do end to end? | All logged sessions have `pq=0`. The display/PQ code is static evidence only. | An HDR session with display-state and output-format diagnostics. |
| What happened in S1-S4 inside Bifrost? | Rotation removed those engine logs. IPC and JS events remain. | Logs copied before rotation. |
| Why did resume fail with `0xC2EC1025`? | No readable enum name is recovered. Unified-error propagation is possible, but the original response and source are not captured. | The response covering the failure or an authoritative service error table. |
| Why was user storage unavailable in launch 4? | The client error category and status conversion are now named. They do not identify local disk, remote service, account state or transient cause. | Storage/session response diagnostics from that launch. |
| How do 6/8-channel playback and mic state behave? | Both detailed sessions were stereo. Mic state values 2 and 3 were observed. | A multichannel session and complete state transitions. |

## Static work not completed

- Signaling TLS certificate-chain and hostname checks, and conditional Snoopy CA pinning inside POCO. The DTLS fingerprint gate is independently proved; it does not settle these signaling checks.
- Full ICE gathering, nomination, relay selection and optional-channel SID allocation. The SCTP OPEN advertisements are traced, but some per-message option-block contents are not.
- Every consumer of the GS capture-time value, its epoch and wrap rules, and all codec-specific extension/FEC metadata semantics.
- The complete QEC reference-safety metadata and recovery-mode timeout escalation. The strict-greater-than-100-ms active RFI/IDR retry is proved; it is not a universal RFI-to-IDR deadline.
- Several private frame-ack and QoS fields. The reference tables deliberately keep their producer offsets and mark the names unknown rather than borrow names from a different schema.
- SRTP repeat-transmit behavior in the library policy consumer. The initializer's configured replay window and adjacent flag are traced; native video and bundle SRTP remain separate owners.
- Geronimo's full state names for values 9 and 13, its undecodable-frame threshold, decoder drain/EOS and graphics-device-loss recovery.
- The full input-key conversion before serialization, Unicode/IME event schema, horizontal-wheel/touch forwarding, and custom-message clipboard JSON schema.
- Mic multistream decoder topology, the complete message-bus schemas, and the purpose of every Serenity recording message.

These are untraced code or missing observations, not gaps caused by an unavailable DLL. The recovered ABI/model offsets are specific to this build and must not be treated as public contracts.

## Safe follow-up

For runtime evidence, use an authorized account and change one condition per session. Copy `geronimo.log` immediately after each session, before rotation. Redact auth headers, cookies, endpoint addresses, account/device/session identifiers, certificates, keys and signed URLs before sharing. No such probe was performed for this study.
