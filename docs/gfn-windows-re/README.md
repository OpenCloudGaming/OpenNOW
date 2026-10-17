# GFN Windows native-client research

This directory records a static study of the GeForce NOW Windows client supplied on 2026-10-13, plus the user's own logs from that install. Product metadata reports **2.0.89.141**. `Geronimo.dll` reports `gs_04_92 38967594`. The study mirrors the macOS research in PR #1161 (`docs/gfn-macos-re/`), file for file where the evidence allows.

The research used Ghidra 12.1.4 headless decompilation, `pefile`/`capstone` byte inventory, and parsing of the supplied logs. No GFN executable was run. No NVIDIA or GFN server was contacted. No login was attempted.

The user subsequently supplied the matching `Bifrost2.dll`, `gs_04_92 38967594`. The network side is now independently traced on Windows, including CloudMatch conversion, ANNOUNCE, encrypted transport, packet layouts and recovery. Static findings do not reconstruct an unlogged packet capture. See [artifacts.md](artifacts.md).

## Read the findings

| Question | Document |
| --- | --- |
| Which files, hashes and versions were examined, and what is missing? | [Artifact inventory](artifacts.md) |
| How were claims established, and how can they be repeated? | [Method and reproduction](method.md) |
| Which process or DLL owns each stage, and how does a session flow from the app to the streamer? | [Architecture and IPC](architecture.md) |
| How does session signaling establish the streams? | [Handshake and connection setup](handshake.md) |
| How do media transport, loss feedback, control and termination work? | [Transport and feedback](transport-feedback.md) |
| What are the Windows packet and NACK byte layouts? | [Packet layouts](packet-layouts.md) |
| Which attributes does ANNOUNCE serialize, omit or convert? | [ANNOUNCE builder](announce-builder.md) |
| How are codecs, decoders, presentation and pacing selected? | [Video](video.md) |
| How do color, chroma, bit depth, SDR and HDR interact? | [Color and HDR](color.md) |
| How do audio playout and the microphone work? | [Audio](audio.md) |
| How do keyboard, mouse, cursor, gamepad, haptics and clipboard work? | [Input](input.md) |
| What is known about alliance partners, server types and endpoints? | [Providers](providers.md) |
| Which recovery and fallback decisions are established? | [Fallbacks and recovery](fallbacks.md) |
| Which configuration names and switches exist? | [Configuration fields](configuration-fields.md), [NvscClientConfig dump](nvsc-client-config.tsv) |
| What did the user's real sessions do, step by step? | [Log timeline](logs-timeline.md) |
| What does the CloudMatch request and response contain? | [CloudMatch fields](cloudmatch-fields.md) |
| How does native code build, convert and own those values? | [Native CloudMatch models and requests](cloudmatch-native.md) |
| Which RTSP headers and SDP attributes were exchanged? | [SDP attributes](sdp-attributes.md) |
| How does Windows differ from macOS? | [Windows vs macOS](windows-vs-macos.md) |
| What does OpenNOW do differently or lack? | [OpenNOW gap list](opennow-gaps.md) |
| What remains unverified? | [Unknowns](unknowns.md) |
| Which judgment calls did the study make? | [Decision log](decisions.tsv) |

## Evidence levels

- **S, static:** a string, import, export, RTTI name, GUID or data value is present in a supplied binary. Cited by virtual address (VA).
- **D, decompiled:** a branch, call, struct write or serializer was read in Ghidra output. Cited by function VA. This proves a code path, not that a session takes it.
- **L, log-confirmed:** a line in the supplied logs shows it happened in a real session. Cited by file and local time, with identifiers replaced by `<redacted>`.
- **I, inferred:** an interpretation the evidence does not fully prove.
- **U, unknown:** the needed artifact or observation is missing.

"macOS Bifrost (PR #1161)" marks facts taken from the macOS study. They are not Windows evidence. VAs use image base `0x140000000` for EXEs and `0x180000000` for DLLs. Log times are local wall-clock (UTC+2), which is the second timestamp on each `geronimo.log` line.

## Main findings

1. **Geronimo runs inside `GeForceNOW.exe`.** The CEF shell delay-loads `Geronimo.dll`, and its `GFNQueryHandler` turns `QUERY_GFN_PREPARE`/`START`/`RESUME` into direct `GridApp` calls, the same boundary the macOS study found. `GeForceNOWStreamer.exe` is a 500 KB relauncher that starts `GeForceNOW.exe` with one forwarded argument. NvContainer plugins carry diagnostics over a named-pipe message bus, never media (**D/L**, [architecture.md](architecture.md)).
2. **On the logged NVIDIA seat the media peer comes from SETUP.** CloudMatch returned two JSON usage-14 SIGNALING entries, ports 322 and 48322, and a usage-10 session-control resource. Bifrost converts eligible JSON usage 14 to native `NVB_PU_RTSPS = 10`; these are different enum domains. JSON usage 17 becomes native BUNDLE 11. No media usage 2 or 17 appeared in S6. Both UDP sockets used SETUP's port 5004, not the SDP bundle port (**D/L**, [handshake.md](handshake.md), [CloudMatch fields](cloudmatch-fields.md)).
3. **No unified socket on that seat.** Bifrost logged `Unified socket mode disabled (enableUnifiedSocket=0, using legacy dedicated sockets)`. Video used its own SRTP socket. Audio, microphone and six SCTP data channels used the DTLS bundle, and audio and microphone skipped RTSP SETUP. This matches the macOS "legacy" arrangement and PR #1162's non-alliance path. The alliance branch of #1162 cannot be tested with these logs (**L**).
4. **Signaling is RTSP over secure WebSocket** (`Using Secure WebSockets for Signaling`, `X-GS-Version: 14.2`, `HTTP/1.0 200 OK` responses). The control channel is `udp_ag` over SCTP. The client appends 163 config attributes to ANNOUNCE and skips the V1 ICE/DTLS fields (`general.iceUsernameFragment`, `iceUsernamePwd`, `dtlsFingerprint`) (**L**). OpenNOW does not send those V1 keys either: `build_announce` writes only the `*V2` keys plus the standard `a=ice-ufrag`, `a=ice-pwd` and `a=fingerprint` lines (`nvst_rtsp.rs:1580-1586` on dev `7ce3cd54`).
5. **The codec selector is the macOS `chooseMaxSupportedVideoSettings` code.** On Windows it is `FUN_180157f20`. Auto prefers **AV1, then HEVC, then H.264, also when HDR is requested**. Merged PR #1165 tries HEVC before AV1 for HDR, so it contradicts this build (**D**, [video.md](video.md)).
6. **H.264 is capped at 120 fps and below 4,953,600 pixels** whenever H.264 is the requested codec or the only decodable one. Log probes show `1920x1080@240 H264` becoming `@120` and `3440x1440@120 H264` becoming `2560x1080@120`. PR #1165 caps only Auto+HDR landing on H.264 (**D/L**).
7. **4:4:4 forces HEVC at runtime.** The user's session requested H.264 10-bit 4:4:4 and streamed HEVC 10-bit 4:4:4 SDR, decoded as Y410 through D3D11VA (`HEVC_VLD_Main10_444`) into a 10-bit flip-model swap chain. This supports PR #1165's 4:4:4 rule (**D/L**).
8. **Recovery now has Windows code evidence.** The DLL has normal 0x301 invalidation, 0x302 IDR, and a separate 0x30c NvWac invalidation path. The supplied logs do not print these wire commands, so absence of a line does not prove a command never fired. OpenNOW already has 0x301 feedback and dependent-frame withholding; exact admission and clearing rules need separate comparison (**D/L**, [transport-feedback.md](transport-feedback.md)).
9. **NACK v2's Windows payload is recovered.** `0x18047a220` writes little-endian bases and masks; `0x18047ec60` sends command 0x317 and proves byte 1 is the stream index. The Windows tracker has a 2,048-entry ledger. OpenNOW retains 16 missing ranges and has no explicit 1 ms initial-delay guard, so payload similarity does not establish identical tracking or retry policy (**D/L**, [packet-layouts.md](packet-layouts.md)).
10. **Session end is classified, and the client warns before an idle kick.** The server sends a type 4 timer notification (60 s left) and a type 5 (0 s) before `NVST_DISCONN_SERVER_TERMINATED_CLOSED`. Geronimo derives `Resumable`/`SessionAlive` from the reason and extended code (`0x180142a70`), and the JS app auto-resumes reason 55. OpenNOW has neither the idle warning nor the resumable rule (**D/L**).
11. **The logged generic input errors did not stop streaming.** All 42 occur around focus or window-state changes. OpenNOW tolerates asynchronous motion-send failures but terminates on captured enqueue or discrete-send failures to avoid stuck input. The Windows log does not identify the event type, so blanket error-dropping is not a justified parity fix (**L**, [input.md](input.md)).
12. **The microphone runs WebRTC echo cancellation with decoded game audio as the reference**, and sends Opus with RFC 2198 RED level 3. OpenNOW sends unprocessed mono with no RED (**D/L**, [audio.md](audio.md)).
13. **HDR output is HDR10 without DXGI metadata.** The swap chain uses `SetColorSpace1(RGB_FULL_G2084_NONE_P2020)`. No code reaches `IDXGISwapChain4::SetHDRMetaData`. Display HDR detection uses the advanced-color bits from `DisplayConfigGetDeviceInfo`, not only `IDXGIOutput6` (**D**, [color.md](color.md)).
14. **ANNOUNCE is a difference against DESCRIBE, not a dump of the final config.** Its typed writers can omit unchanged values and support `refPicInvalidation` and recovery keys. The color writer emits its config byte unchanged. S6's GFN depth/chroma are **10/1** for 10-bit 4:4:4: depth agrees with OpenNOW's 10, but chroma differs from its 3. CloudMatch JSON flags 1/1 are separate. Server acceptance of alternate chroma values is unverified (**D/L**, [ANNOUNCE builder](announce-builder.md)).
15. **SCTP labels do not determine reliability.** Windows partial control is ordered and retransmission-count-limited, unlike OpenNOW's unordered lifetime-limited configuration. Its nominally unreliable control channel advertises one retransmission, protocol `control`, and a nonzero priority. The logged IDs match the first six channels but are allocator results, not a fixed universal table (**D/L**, [handshake.md](handshake.md)).
16. **Two unnamed errors are now named.** `0x80040010` is `NVST_NETERR_CLIENT_DISCONNECT_UNINTENDED_NO_VIDEO_PACKETS_RECENTLY`. Launch 4's `0xC0F52159` is `NVB_R_USER_STORAGE_NOT_AVAILABLE`, produced from service status 89. The resume error `0xC2EC1025` remains unnamed (**D**, [CloudMatch fields](cloudmatch-fields.md)).

[opennow-gaps.md](opennow-gaps.md) turns these into concrete OpenNOW file pointers.

## Coverage limit

The logs hold seven launches and six streaming sessions on 2026-10-11 and 2026-10-12. All went to the `eu-netherlands-north` zone on NVIDIA (NIMBUS) seats. Geronimo log rotation left a full handshake for only S6. There is no HDR, alliance-partner, successful resume or deliberate loss-injection session. S5 does include high RTT and packet loss. Findings are specific to this build and this hardware. Proprietary binaries, raw logs, bulk decompiled code and account, session, network and device identifiers are excluded.
