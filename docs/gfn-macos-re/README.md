# GFN macOS native-client research

This directory records compatibility research on the Mac GFN artifacts supplied on 2026-10-13. The supplied streamer and helper plists report version **2.0.87.131**. That metadata does not independently authenticate a NVIDIA release.

The research uses **REA 6.4.0's MCP** and **Ghidra 12.1.4**, plus a reproducible byte inventory. It does not modify OpenNOW or the GFN binaries. The supported OpenNOW implementation remains the Qt/native stack.

| Uploaded archive | SHA-256 |
| --- | --- |
| `Archive.zip` | `054c3733c817093974a8d7c7ef31f01304fef9c4fbe5cd3fa2e0167fca1b1066` |
| `MacOS.zip` | `73e47930225f5ffc2eaa01da54ed143670e030c58315b528872412fd1ebc8a26` |

## Read the findings

| Question | Document |
| --- | --- |
| Which files, hashes, architectures, and versions were examined? | [Artifact inventory](artifacts.md) |
| How were claims established, and how can the queries be repeated? | [Method and reproduction](method.md) |
| Which native component owns each stage? | [Architecture and other native features](architecture.md) |
| How does session signaling establish the streams? | [Handshake and connection setup](handshake.md) |
| How do media transport, loss feedback, and control differ? | [Transport and feedback](transport-feedback.md) |
| How are codecs, decoding, and presentation selected? | [Video](video.md) |
| How do color, chroma, bit depth, SDR, and HDR interact? | [Color and HDR](color.md) |
| How do audio playout and microphone processing work? | [Audio](audio.md) |
| What is known about alliance routing and session orchestration? | [Providers and account boundaries](providers.md) |
| Which fallback and recovery decisions are actually established? | [Fallbacks and recovery](fallbacks.md) |
| Which NV-specific configuration names are present? | [Configuration field-name catalog](configuration-fields.md) |
| What remains unresolved, and which probes would resolve it? | [Unknowns and follow-up work](unknowns.md) |

## Evidence levels

- **D — decompiled or decoded behavior:** a branch, call, serializer, or decoded instruction was examined. This establishes a static path, not that a production session takes it.
- **S — artifact observation:** a literal, symbol, import, plist value, or byte sequence is present. A log message alone does not establish execution, ordering, default values, or server support.
- **I — inference:** an interpretation that the available evidence does not fully prove.
- **U — unknown:** the necessary artifact, caller, type information, capture, or runtime observation is missing.

Each subsystem document carries its own evidence anchors and limitations. Function addresses are virtual addresses in the selected **x86_64 thin image**, unless a different architecture is stated. They are not offsets in the original ZIP or universal Mach-O.

## Coverage limit

The uploads contain 52 regular non-AppleDouble files, including 28 universal Mach-O files and 56 extracted architecture slices. Static inventory covers both x86_64 and arm64. Focused decompilation uses x86_64; equivalent arm64 behavior has not been established.

The uploads are not a complete macOS application installation. They omit the full application resource tree and account UI. Linux cannot run the Mac client. There are no authenticated live-session captures, hardware/display measurements, or observed alliance-login exchanges in this study.

This is a set of evidence-backed findings and a field-name reference, not a complete source recovery or a drop-in GFN protocol specification. Findings are version-specific. Cryptographic secrets, proprietary executable files, and bulk decompiled source are intentionally excluded.
