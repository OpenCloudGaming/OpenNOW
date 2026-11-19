# Unknowns and follow-up work

The following gaps prevent these documents from serving as a complete protocol specification. **U** means unresolved, not unsupported. Proposed probes have not been performed in this study.

## Missing artifacts and runtime observations

| Question | Current boundary | Evidence needed |
| --- | --- | --- |
| What is the complete NVIDIA or alliance login exchange? | The full account/resource tree and authenticated exchanges were not supplied. Native entry points alone cannot reconstruct account policy. | A complete application resource package, then an authorized, redacted login observation for each provider. |
| Which alliance endpoint, region, entitlement, and proxy is selected for an actual account? | Native settings and identifiers do not prove service-side selection. | The provider configuration and redacted session-preparation exchange for that account. |
| Does the server accept each advertised codec, bit depth, chroma, and HDR combination? | Client capability branches and field names are local evidence. Subscription policy and server negotiation remain external. | Redacted offer/answer and accepted session data for each exact mode. |
| Are the analyzed x86_64 branches equivalent on Apple Silicon? | Both slices were inventoried; focused decompilation uses x86_64. | Replay the relevant functions against the arm64 slice and compare calls, conditions, enums, and serializers. |
| Does playback survive fullscreen, display changes, overlays, or graphics-device loss? | Static ownership and error paths cannot establish observed recovery. | A controlled macOS session with staged window/display changes and correlated receive/decode/present diagnostics. |
| What appears optically on an HDR display? | Pixel formats, color metadata, and shader paths do not measure display output. | A known test signal, the exact display/OS configuration, and optical or calibrated output measurements. |
| What are latency, queue occupancy, clock drift, and loss-recovery performance under load? | No live measurements or injected-loss experiment was performed. | Controlled recordings with sender-authored IDs, synchronized stage timestamps, queue diagnostics, and a stated loss model. |

## Protocol details requiring further tracing

The handshake and transport documents describe the functions that were inspected. They do not establish every optional SDP attribute, negotiated command, packet type, or policy flag.

Further work needs to trace serializer/parser pairs for unexamined messages and prove their caller conditions. The `configuration-fields.md` catalog contains names, not a wire schema. For each field used by an implementation, establish its type, units, permitted values, default source, negotiation direction, and behavior when absent or rejected. Do not infer those properties from the field's spelling.

For media identifiers, establish where packet sequence numbers, frame IDs, stream IDs, timestamps, and discontinuities enter and leave each queue. Cross-check wraparound, FEC reconstruction, loss reports, acknowledgments, decoder submission, presentation feedback, and recording. A similarly named local counter is not interchangeable with a sender-authored identifier.

For cryptographic transport, establish peer identity binding and every failure path from the actual session description to certificate/fingerprint validation. Bundled TLS, SRTP, SCTP, or QUIC code does not prove that a particular GFN session uses it. No authentication bypass, recovered credential, or weakened certificate validation is required for compatibility research.

For microphone and audio playback, connect the processing helper's local sample format to the selected network codec, packet timing, clock domain, and device-reopen behavior. Library names and helper exports do not establish transport selection or a universal sample rate.

## Safe runtime evidence collection

Use a real, authorized account on a compatible Mac if runtime research is needed. Capture only the chosen scenario. Redact authorization headers, cookies, account identifiers, session secrets, signed URLs, and private keys before sharing or committing evidence. Preserve non-secret protocol structure, sizes, ordering, timestamps, and sender-authored media IDs.

First establish a successful baseline. Then vary one condition: provider, codec, HDR mode, display, microphone device, packet loss, or session reconnect. Keep a record of the exact application hash and negotiated session profile for each observation. A successful audio path is not evidence that video assembly, decode, or presentation is healthy.

No live GFN request or runtime probe listed here was performed by this study.
