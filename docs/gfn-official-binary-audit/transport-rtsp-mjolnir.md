# GFN transport in `libGeronimo.so` and `libBifrost2.so`

This note is a static reading of the Linux GeForce NOW client under `audit/gfn-official/`, set next to OpenNOW’s NVST implementation in `native/opennow-streamer` (`nvst_rtsp.rs`, `nvst.rs`, `nvst_input.rs`, `nvst_control.rs`) and the comparison notes in `docs/streamer-comparison/` and `docs/mjolnir-nack-v2.md`. The official libraries are the Flatpak payload `com.nvidia.geforcenow` (Freedesktop runtime 24.08). Both are stripped ELF64 x86-64 shared objects. `libBifrost2.so` is 19,052,024 bytes, SHA-256 `8400714f98b7db928ef4377515b1ed35be12523b7fa306566e776b76965537c9`, GNU build id `fa3685038bd71962fe30ad09482bcb0721a54f35`. `libGeronimo.so` is 34,347,824 bytes, SHA-256 `4863f02ed7d82b06b8152b23e8683dd9b77bcd63165890ce4c6c1cd883fc0cd1`, build id `15d0eebc08da503f1f37ea9cae2dbac1d760fea4`. Embedded Perforce paths are `gs_04_87` (`/dvs/p4/build/sw/gcomp/rel/gs_04_87/...`), not the Windows `gs_04_90` install described in `docs/streamer-comparison/README.md`. Bifrost’s rodata also names OpenSSL 3.5.6 (7 Apr 2026) and a Mjolnir vcpkg tree (Poco, usrsctp-era SCTP helpers, `StunConsent.cpp`). The Bifrost hash is the same artifact already cited for private NACK-v2, so addresses below are ELF virtual addresses in that file and can be checked with `objdump` or radare2 5.5.0.

The split of ownership is the whole architecture. Geronimo is the Grid shell: SDL windows, Vulkan and VDPAU decode, session UI, and the C++ types `GridApp`, `IOInterface`, and `BifrostSDKExecutor`. It does not implement RTSP, Mjolnir, ICE, DTLS, or SCTP. Its dynamic symbol table imports the Bifrost C API (`nvbCreateClient`, `nvbInitializeClient`, `nvbStartSession`, `nvbStartStreaming`, `nvbSendInputEvent`, `nvbFeatureControl`, `nvbSendMicAudioFrame`, `nvbUpdateVideoFrameState`, `nvbUpdateVideoDecoderState`, and the rest of that list). Bifrost is the stream SDK. Its rodata is full of `NVSTE_RTSP_*` stage names, `x-nv-*` SDP keys, Mjolnir receiver types, ServerControl channel labels, and `NVST_R_SERVER_CONTROL_*` failures. Geronimo decides when a session is allowed to send. Bifrost decides how the bytes are framed, which socket they leave on, and which ServerControl command code they carry.

## RTSP and RTSPS

Signaling is RTSP carried on a secure WebSocket, not a raw TCP RTSP socket. Bifrost strings include `NVB_PU_RTSP`, `NVB_PU_RTSPS`, `secureRTSPSupported`, `rtsps://`, `Using Secure WebSockets for Signaling`, `WebSocket Loop Starting.`, `WebSocket Loop Exiting.`, `Exception during WebSocket operation: %s`, and `RTSP/WebSocket upgrade forbidden (403): %s`. The SDP key `general.rtspWebSocketPerConnection` (also emitted as `x-nv-general.rtspWebSocketPerConnection`) is the switch for one WebSocket per RTSP connection. OpenNOW’s ANNOUNCE hard-codes `a=x-nv-general.rtspWebSocketPerConnection:1` and connects with `RtspClient` to the first CloudMatch endpoint that starts with `rtsps://` or `rtsp://`.

The method set, recovered from the `NVSTE_RTSP_*` / `NVST_RTSP_*` name table, is the classic sequence plus a GFN-specific fan-out inside SETUP:

| Stage | Success / failure names present in Bifrost |
| --- | --- |
| OPTIONS | `NVST_RTSP_OPTIONS_OK`, `NVST_RTSP_OPTIONS_BAD_REQUEST`, `NVSTE_RTSP_OPTIONS`, timeout |
| DESCRIBE | `NVST_RTSP_DESCRIBE_OK`, `NVSTE_RTSP_DESCRIBE`, connect failure |
| SETUP | Split into audio, video, control, mic, and extended. Each has its own `NVSTE_RTSP_SETUP_*` prefix |
| ANNOUNCE | `NVST_RTSP_ANNOUNCE_OK`, `NVST_RTSP_ANNOUNCE_NOT_FOUND`, connect failure |
| PLAY | `NVST_RTSP_PLAY_OK`, `NVST_RTSP_PLAY_BAD_REQUEST`, timeout |
| TEARDOWN | `NVST_RTSP_TEARDOWN_OK`, timeout |

SETUP is not one transaction. Bifrost names five sub-stages: `NVSTE_RTSP_SETUP_AUDIO`, `NVSTE_RTSP_SETUP_VIDEO`, `NVSTE_RTSP_SETUP_CONTROL`, `NVSTE_RTSP_SETUP_MIC`, and `NVSTE_RTSP_SETUP_EXTENDED`. Each sub-stage has a parallel response-code family: bad request, unauthorized, forbidden, not found, not allowed, authentication required, timeout, not enough bandwidth, session not found, not valid in this state, unsupported transport, destination unreachable, invalid CSeq, malformed response, internal server error, not implemented, version not supported, a stage-specific “not supported” code (`SETUP_AUDIO_NOT_SUPPORTED`, `SETUP_VIDEO_NOT_SUPPORTED`, and the same for control and mic), and `STREAMER_COMMUNICATION_FAILED`. Transport failures are named separately from HTTP-style codes: `RTSPE_OPERATION_TIMEDOUT`, `RTSPE_COULDNT_CONNECT`, `RTSPE_SSL_CONNECT_ERROR`, `RTSPE_SEND_ERROR`, `RTSPE_RECV_ERROR`, `RTSPE_SSL_INVALIDCERTSTATUS`, `RTSPE_NOT_AUTHENTICATED_EXCEPTION`, and three Poco SSL exceptions (`SSLCONTEXT`, `CERTVALIDATION`, `SSLCON_UNEXPECTEDLYCLOSED`). A session can therefore fail “at SETUP” in twenty different ways that a single status integer would collapse. OpenNOW’s client collapses this to `ensure_rtsp_ok` plus a video-SETUP retry. `nvst_rtsp.rs` treats a 200 that comes back without a usable video peer as retryable (three rounds, three seconds apart) because a rig whose streamer is still starting answers SETUP with 200 and an empty peer, and repeating that SETUP can poison later ANNOUNCE on some alliance seats. That retry policy is OpenNOW’s, not a loop visible as a string in this Bifrost build.

Two SETUP bypass strings change the media map. Bifrost logs `Bypassing 'Mjolnir' SETUP for the Audio stream. Deferring to WebRtcTransport setup` and the same sentence for the mic stream. Video stays on the Mjolnir SETUP. Audio and microphone are expected to ride the WebRTC bundle once `rtcAudioOnNativeBundle` and `rtcMicOnNativeBundle` are set. OpenNOW matches that shape: ANNOUNCE sets `rtcVideoOnNativeBundle:0`, `rtcAudioOnNativeBundle:1`, `rtcDataChannelOnNativeBundle:1`, and, when a microphone is available, `rtcMicOnNativeBundle:1` plus `a=x-nv-mic.micSsrcConfig.senderSsrc:1`. It also sets `enableUnifiedSocket:0`. Bifrost still contains the other mode: `general.enableUnifiedSocket`, `MjolnirVideoReceiver created early for unified socket mode`, `setupUnifiedSocketMode: MjolnirVideoReceiver not found in WebRtcTransport`, and `MjolnirVideoReceiver not set on WebRtcTransport for unified socket mode`. Unified socket is a real code path in `gs_04_87`. It is not the path OpenNOW advertises, and it is not what `docs/streamer-comparison/video.md` describes for the Windows log from 2026-08-30.

ANNOUNCE is where the client’s transport policy becomes SDP. Bifrost’s rodata holds the attribute names for at least six video indexes (`video[0]` through `video[5]`, collapsed here as `video[N]`). The keys that define the socket split and the recovery contract are:

- `x-nv-general.clientBundlePort` and `general.clientBundlePortUsage`
- `x-nv-general.clientPorts.video`, `.audio`, `.mic`, `.control`, `.bundle`, `.session`, `.localAddress`
- `x-nv-general.clientPorts.useReserved` and `.fallbackDynamic`
- `x-nv-general.nativeRtcOnBundlePort`
- `x-nv-general.rtcVideoOnNativeBundle`, `.rtcAudioOnNativeBundle`, `.rtcDataChannelOnNativeBundle`, `.rtcMicOnNativeBundle`
- `x-nv-general.rtcpOnSctp`
- `x-nv-general.iceUserNameFragmentV2`, `iceUsernameFragment`, `icePasswordV2`, `iceUsernamePwd`, `iceTransportPolicy`
- `x-nv-general.dtlsFingerprint` and `dtlsFingerprintV2`
- `x-nv-runtime.videoSrtp`, `.audioSrtp`, `.micSrtp`, `.encryptionKey`, `.encryptionKeyId`, `.srtpReplayWindowSize`
- `x-nv-video[N].packetSize`, `.enableRtpNack`, `.rtpNackVersion`, `.rtpNackBackoffTimeMs`, `.rtpNackMaxRetries`, `.rtpNackMaxPacketCount`, `.rtpNackQueueLength`, `.rtpNackQueueMaxPackets`, `.rtpNackInitialWaitTimeMs`, `.rtpNackDjbInteractionMode`
- `x-nv-video[N].packetSizeDetection.enable`, `.minNumFrames`, `.packetLossRate`, and `dynamicPacketSize.packetSizeL0` / `L1`
- `x-nv-vqos[N].fec.enable`, `.repairPercent`, `.repairMinPercent`, `.repairMaxPercent`, `.minRequiredFecPackets`, `.maxAllowedFecPackets`, `.numSrcPackets`, `.rateDropWindow`, `.type`
- `x-nv-ri.partialReliableThresholdMs`, `x-nv-ri.hidDeviceMask`
- `x-nv-general.pingIntervalBeforeConnectionMs`, `.pingIntervalAfterConnectionMs`
- a large `x-nv-general.enetControlChannel.*` block (`mtuSize`, `reliabilityType`, `maxRetransmit`, `rtoInitMs`, `rtoMinMs`, `rtoMaxMs`, `heartbeatIntervalMs`, `dcsctpA`, `dcsctpB`, `dcsctpC`, `maxTxPartiallyReliable`, `waitWindowPartiallyReliable`, and others)

OpenNOW’s `build_announce` writes a subset of those keys with fixed values: packet size from the route (baseline 1280, smaller only when a measured VPN path requires it), `enableRtpNack:1`, queue length 2048, queue max packets 1024, max packet count 64, FEC enable 1 with repair 20–35 percent, `partialReliableThresholdMs:300`, ping intervals 20 ms before connection and 100 ms after, `videoSrtp:1`, `audioSrtp:0`, `micSrtp:0`, `useReserved:1`, `fallbackDynamic:1`, `rtcpOnSctp` from the negotiated flag, and ICE ufrag, password, and SHA-256 fingerprint both as `x-nv-general.*V2` attributes and as ordinary `a=ice-ufrag`, `a=ice-pwd`, `a=fingerprint`, `a=setup:actpass`, plus one host candidate. The enet block in OpenNOW is a single line, `enetControlChannel.mtuSize:1191`, and `usePartiallyReliableUdpChannel:0`. Bifrost still knows the full enet reliability surface, including dcSCTP tuning knobs. Those knobs are how an older reliable-UDP control channel was configured. They are not evidence that a current session uses enet for input. The live data path advertised by both clients is native RTC on the bundle port.

Headers on the OpenNOW side of the same handshake are `X-GS-Version: 14.2`, `x-nv-sessionid`, `Accept: application/sdp` on DESCRIBE, and `x-nv-abtesting: 2`. SETUP then reads `x-nv-ping` and `x-nv-ping-payload`. Ping version 6 without a payload is a hard error in `nvst_rtsp.rs`. Bifrost names `NVST_R_ERROR_RTSP_STUN_FAILED` and a deprecated `NVST_R_SERVER_CONTROL_STUN_FAILURE_DEPRECATED`, which is the same idea from the server-control side: STUN reachability is part of SETUP, and an older control command for reporting STUN failure still has an error string.

## Ports

Neither library contains the immediates `49005` or `49006` as UDP port constants. A byte search for `6d bf 00 00` and `6e bf 00 00` in Bifrost returned no hits. The two Geronimo hits for `6e bf 00 00` sit inside pointer-sized data (`0xbf6ed0`-style addresses), not a `mov` of the port. Decimal `5004` appears once in Bifrost and seven times in Geronimo; that is the historical RTP port the Windows comparison log called “server 5004”, but this static pass does not show a `mov $5004` on the bind path, so it should not be treated as the Linux client’s listen port.

What the binary does contain is the reservation policy. Strings, in order of the failure they describe:

- `Enable dynamic fallback when using reserved ports`
- `Setup a stream clientAddress=%s, serverPort=%d`
- `clientPorts.useReserved: Failed to create a bound VIDEO stream on %s.`
- `clientPorts.useReserved [Fallback plan]: On. Attempting to use the next available port from the dynamic range.`
- the same pattern for AUDIO, MIC, and CONTROL, with CONTROL’s fallback text saying “the first available port” rather than “the next”
- `useReserved port [Fallback:On]. Use next available port from dynamic range.`
- `general.clientPorts.useReserved` and `general.clientPorts.fallbackDynamic`

So the official client tries a reserved bind per stream kind (video, audio, mic, control) and, if that bind fails and fallback is on, takes an ephemeral port. The reserved numbers themselves are not in this build’s immediates. They are either computed, read from configuration, or simply the ports the process already holds when ANNOUNCE fills `clientBundlePort` and `clientPorts.*`. The Windows comparison notes record 49005 for video RTP and 49006 for the WebRTC audio bind on one session. That is a log fact about that install, not a constant in `gs_04_87`.

OpenNOW makes the pair explicit because it does not link these libraries. `reserve_nvst_socket_pair_from(49_005)` in `nvst.rs` binds video on 49005 and the bundle on 49006, then falls back to a dynamic adjacent pair (ephemeral video port, bundle = video+1, up to 32 attempts). The comment there cites Bifrost’s `useReserved=1` behavior. ANNOUNCE then publishes the ports that were actually bound: `clientBundlePort` is the bundle socket, and the video `m=` line carries the Mjolnir port. `clientPorts.video/audio/mic/control/bundle/session` are written as `0` with `useReserved:1` and `fallbackDynamic:1`, which tells the server to use the reserved client ports rather than a list of fixed per-media ports in the SDP. A test fixture still speaks the older Transport form `unicast;X-GS-ClientPort=49005-49006`.

Packet size is a port-adjacent MTU problem, not a port number. Bifrost logs `Configured packetSize value is invalid and it is getting overridden to 512 bytes.` and `Updating streamer packetSize of stream %zu to use MTU value determined by NCT. New packetSize: %u bytes, NCT detected packet size: %u bytes`. SDP also has `packetSizeDetection` and two dynamic levels `packetSizeL0` and `packetSizeL1`. OpenNOW keeps 1280 unless the video route’s interface is a detected VPN and a measured datagram size is tighter. The 512-byte override is a Bifrost safety floor this tree does not copy.

## Mjolnir

Mjolnir is the video receive stack, not a synonym for “the UDP socket.” The socket is the dedicated video UDP flow. The types in Bifrost are `MjolnirVideoReceiver`, `MjolnirExtension`, `MjolnirGsFrameHeader` (an `RtpExtensionHeader`), and `VirtualAssignable` wrappers around those headers. Log lines:

- `Using mjolnir header.`
- `Invalid MjolnirExtension length %u, it should be %zu`
- `Can't read MjolnirExtension with ID %02X and length %zu, not enough bytes %zu`
- `Mjolnir packet received but MjolnirVideoReceiver not set`
- `MjolnirVideoReceiver has been set`
- `MjolnirVideoReceiver destroyed. Stats: received=%llu dropped=%llu queue_size=%zu`
- `MjolnirVideoReceiver queue full (%zu packets), dropping packet. This may indicate RtpSourceQueue is not consuming fast enough.`

The queue is bounded and drop-newest-or-drop-incoming when the RTP source queue falls behind. There is no string that says the receiver blocks the network thread until the decoder catches up. That matches the streamer rule that codec work must not run on the receive thread, and it matches OpenNOW’s bounded reorder queue (1024 packets, hard cap 2048) rather than an unbounded jitter buffer.

The header OpenNOW parses is the same object Bifrost names `MjolnirGsFrameHeader`. In `nvst.rs` the profile constant is `GS_VIDEO_EXTENSION_PROFILE = 0x4753` (“GS”), a fixed 16-byte RTP extension, not an in-payload prefix. Flags inside that header are SOF (`0x04`), EOF (`0x02`), and picture data (`0x01`). Frame assembly uses those bits plus the stream packet index. Bifrost’s “invalid extension length” and “not enough bytes” strings are the official parser rejecting a short or mistyped extension before decode. OpenNOW’s assembler resets, marks the access unit non-contiguous, and asks for an IDR when the GS header is bad. Both sides refuse to invent a frame from a truncated header.

Video confidentiality is SRTP on that socket. OpenNOW’s receiver comments record Bifrost’s `SecureRtp` mapping: `sec_serv_conf_and_auth` with a 256-bit key selects `srtp_crypto_policy_set_aes_gcm_256_8_auth`, an 8-byte tag, not the 16-byte tag of RFC 7714 `AEAD_AES_GCM`. RTP key derivation uses labels `0x00` and `0x02`. SRTCP, if used, uses `0x03` and `0x05` on a separate session derived from the same master key. ANNOUNCE carries `x-nv-runtime.videoSrtp:1` and the key id. Audio and mic SRTP are advertised off (`audioSrtp:0`, `micSrtp:0`) because those flows are DTLS-protected inside the bundle instead. A replay window attribute exists (`srtpReplayWindowSize`). OpenNOW sizes its replay bitmap to the reorder window (2048 packets). A 64-packet bitmap, which an earlier receiver used, rejected legitimate retransmissions after a few milliseconds at high bitrate and turned every NACK into a keyframe.

FEC is configured in the same ANNOUNCE block and applied before access-unit assembly. Bifrost exposes `fec.enable`, repair percents, `minRequiredFecPackets`, `maxAllowedFecPackets`, `numSrcPackets`, `rateDropWindow`, and `fec.type`, including an `rtcMultiStream` mode. OpenNOW enables systematic Reed-Solomon with repair 20 percent, floor 20, ceiling 35, minimum two FEC packets, and a rate-drop window of 10. The Windows log in `video.md` recorded dynamic FEC on and, for that session, zero recovered FEC packets against 639 used of 677 NACKed packets. FEC and NACK are both advertised. A session can run with NACK doing the work and FEC idle.

`ServerControlReliableUdp` and `ServerControlReliableUdpAggregated` are still linked. They observe enet connect, readable, peer-disconnected, error, and telemetry notifications. That is the pre-bundle control transport. It remains in the binary next to the native data-channel path. OpenNOW does not implement enet. It only advertises an MTU and disables the partially reliable UDP channel.

## ICE, DTLS, and SCTP

The bundle is a normal ICE/DTLS association that then carries SCTP data channels and Opus audio. SDP names both credential generations: `iceUsernameFragment` and `iceUserNameFragmentV2`, `icePasswordV2`, `iceUsernamePwd`, `dtlsFingerprint` and `dtlsFingerprintV2`, plus `iceTransportPolicy`. OpenNOW publishes V2 attributes and the standard `a=ice-*` / `a=fingerprint` lines from the same ufrag, password, and SHA-256 fingerprint, with `a=setup:actpass` and `a=ice-options:trickle`. One host candidate is written at priority `2122260223`. STUN in `nvst.rs` is RFC 5389: magic cookie `0x2112a442`, binding request `0x0001`, success `0x0101`, attributes username `0x0006`, message-integrity `0x0008`, XOR-mapped-address `0x0020`, and fingerprint `0x8028` XORed with `0x5354554e`. Integrity is checked against the remote ICE password. A response that does not match a sent transaction is ignored. Displayed RTT prefers the selected ICE pair, then video STUN, then bundle STUN, and samples expire after five seconds. That order is what NACK-v2 retry timing uses.

DTLS protects everything on the bundle socket: audio RTP, microphone RTP, and every data channel. Video does not join that DTLS session when `rtcVideoOnNativeBundle` is 0. Diagnostics have to keep those facts apart. Bundle audio running, or DTLS handshake complete, does not mean a Mjolnir datagram has arrived. `docs/streamer-comparison/windows-udp-diagnostics.md` records a capture that reached PLAY, DTLS, and SCTP with zero video datagrams.

SCTP channels are created by label. The label strings sit together in Bifrost rodata:

| Label in `libBifrost2.so` | In OpenNOW `NVST_CHANNEL_PROFILE` |
| --- | --- |
| `control_channel_reliable` | SID 0, ordered, reliable |
| `custom_message_on_sctp_private_reliable` | SID 2, ordered, reliable |
| `custom_message_on_sctp_private_partially_reliable` | SID 4, unordered, 300 ms lifetime |
| `control_channel_partially_reliable` | SID 6, unordered, 300 ms lifetime |
| `control_channel_unreliable` | SID 8, unordered, max retransmits 0 |
| `input_channel_partially_reliable` | SID 10, unordered, 300 ms lifetime |
| `cursor_channel` | SID 12, ordered, reliable |
| `rtcp_on_sctp_private` | SID 14, ordered, reliable |
| `control_channel` | not created |
| `input_channel_v1` | not created |
| `gamepad_channel_v1` | not created |
| `stats_channel` | not created |
| `remote_trace_channel` | not created |
| `wac_metadata_channel` | not created |

The extra labels are real. `input_channel_v1` and `gamepad_channel_v1` sit in the same string cluster as `cursor_channel` and `rtcp_on_sctp_private`, between `nvstCreateStream` diagnostics and frame-type names (`P-frame`, `I-frame`, `Intra Refresh`). They are the older per-media channels. OpenNOW folds gamepad updates onto `input_channel_partially_reliable` and never opens a v1 gamepad channel. `stats_channel`, `remote_trace_channel`, and `wac_metadata_channel` have no counterpart in the eight-channel profile. If a server still offers them, OpenNOW will not accept those streams as input or control.

SID values in OpenNOW are local choices of the str0m data-channel API (even numbers, negotiated in creation order). They are not constants in Bifrost. At `0x2c4769` the partially reliable control channel is created by loading the address of `control_channel_partially_reliable` and calling through the channel factory (`call *%r14`). The returned `ax` is stored at object offset `0x1f0` (`mov %ax, 0x1f0(%rbx)`). The next site, `0x2c488c`, does the same for `control_channel_unreliable` and stores `ax` at `0x1ee`. The SCTP stream id is whatever the factory returns. A comment in `docs/mjolnir-nack-v2.md` already separates “logical channel 1” from “SCTP SID 1”. This instruction sequence is why: the logical slot is the field in the ServerControl object; the SID is an output of channel creation. OpenNOW’s use of SID 6 for `control_channel_partially_reliable` matches the label and the unordered 300 ms lifetime. It does not match an official immediate `mov $6`.

Reliability policy is also not identical just because the label matches. OpenNOW’s partial channels are unordered with a 300 ms PR-SCTP lifetime, which is the value advertised as `x-nv-ri.partialReliableThresholdMs:300`. The NACK-v2 note records the inspected official default for that channel as ordered with two SCTP retransmissions. Ordered partial reliability is why mouse motion felt like delayed catch-up: one lost report holds every newer report until the lifetime expires. OpenNOW therefore forces `ordered: false` on `control_channel_partially_reliable` even though the official default observed previously was ordered. Reusing the label routes the command to the right server handler. It does not copy the official retransmission policy.

`rtcp_on_sctp_private` is the bundle-only feedback channel. OpenNOW names it `rtcp1` in log lines and sends receiver reports and, on the bundle-only route, RTCP generic NACK there. When a Mjolnir port is selected, loss feedback leaves on `control_channel_partially_reliable` as command `0x0317` instead, and does not fall back to RTCP if that channel is closed.

Data-channel ECN is advertised as attributes (`rtcDataChannelEnableEcnUplink`, `rtcDataChannelEcnCodepointUplink`) and is not set by OpenNOW’s ANNOUNCE. Ping cadence attributes match the constants in `nvst.rs`: 20 ms before the association is up, 100 ms after.

## NACK

Loss recovery on the dedicated video route is private NACK-v2, command `0x0317`, not RTCP generic NACK. The send site is still where the earlier note placed it. At `0x2b2c88` the code does `mov $0x317, %esi` and `call *%r14` with the ServerControl object in `rdi`. `esi` is the command code. The serializer just above the length calculation, at `0x2b74d9` through `0x2b7531`, is:

- `movb $0x2, 0x40(%rsp)` writes version 2 at payload byte 0.
- `movzwl 0x242(%rbx), %eax` then `mov %al, 0x41(%rsp)` writes the low byte of a `u16` stream ordinal at object offset `0x242` into payload byte 1. A single-stream client stores 0 there.
- The count byte lives at `0x42(%rsp)`. It is compared with `0x40` (64). A count above 64 is rejected.
- `lea 0x0(%rbp,%rbp,4), %rax` is `count * 5`, then `lea 0x3(%rax,%rax,1), %edx` is `count * 10 + 3`. That is the payload length: 3 bytes of header plus 10 bytes per record.
- The buffer is then passed to the framer that the `0x317` site sends.

Each record is a little-endian `u16` base RTP sequence plus a little-endian `u64` bitmap. The base itself is missing. Bit `j` requests `base+1+j` modulo 65536. A run of 64 consecutive losses is one record with 63 bits set, not 64. The maximum framed command is 4 bytes of ServerControl header plus 3 + 10×64 = 647 bytes of payload. Records do not carry frame ids, timestamps, or SSRCs. Empty and oversized batches are rejected rather than truncated. OpenNOW’s `nack_v2` in `nvst_control.rs` implements that layout and is covered by `nvst_nack_tests` on paired DTLS/SCTP peers.

Retry timing is separate from the payload. Bifrost SDP exposes `rtpNackInitialWaitTimeMs`, `rtpNackBackoffTimeMs`, `rtpNackMaxRetries`, `rtpNackDjbInteractionMode`, queue length, and queue max packets, per video index, and it logs the resolved integers (`video[%d].rtpNackMaxRetries: %d` and the rest). The constants OpenNOW uses, and that `docs/mjolnir-nack-v2.md` ties to this same binary, are a 4 ms extra wait, a 52 ms tracking budget, a starting RTT of 30 ms, and a send cap of `clamp(floor(52 ms / RTT), 1, 3)` including the first send. A zero RTT uses three sends and a 4 ms interval. With no fresh RTT sample the 30 ms default allows one send inside the 52 ms budget. The first request is eligible immediately. The 4 ms control tick can delay a retry; it does not fire one early. Sent requests stay resolvable until expiry even after they hit the send cap. Bundle-only generic NACK keeps a fixed 4 ms retry and a three-send cap. The official initial-delay and the mode-dependent 52/68 ms wait are named as outside OpenNOW’s implementation. `rtpNackDjbInteractionMode` is the attribute that would select that mode. OpenNOW does not emit it.

The Windows session log summarized in `video.md` is the runtime picture for an older branch: NACK wait 52 ms, 1024-packet hold, 2048 pending, 3 retries, 4 ms backoff. Those numbers are the same envelope OpenNOW advertises (`rtpNackQueueMaxPackets:1024`, `rtpNackQueueLength:2048`, `rtpNackMaxPacketCount:64`) and the same retry constants above. What that log also showed, and what this Linux binary names but OpenNOW does not send, is the next rung: `NVST_R_SERVER_CONTROL_PICTURE_REQUEST_FAILED` (IDR), `NVST_R_SERVER_CONTROL_FRAME_INVALIDATION_FAILED`, and a client request to freeze the display on an invalidated reference. OpenNOW’s unrecoverable-gap path sends RTCP PLI plus control `0x0302` (`idr_request` bytes `02 03 02 00 00 00`) and drops inter frames until a keyframe. It has no reference-invalidation command. Intra-refresh is a frame-type string in Bifrost (`Intra Refresh`, `Ref PIC I`, `Non Ref P`). The Windows log advertised it and then ran with `enableIntraRefresh: 0`.

NACK version is an SDP integer (`video.rtpNackVersion`, logged as `video[%d].rtpNackVersion: %d`). The serializer’s hard-coded `2` is version 2 of that field. A peer that negotiates a different version would not be speaking this payload. OpenNOW always writes version 2 and does not branch on a negotiated version byte.

## ServerControl command IDs

Every outbound control frame OpenNOW sends is a little-endian `u16` command, a little-endian `u16` payload length, then the payload. `0x0308` is the bytes `08 03`. Bifrost’s command path passes the code in `esi` as a 32-bit immediate (`mov $0x317, %esi` is `be 17 03 00 00`), which is the same numeric value, not a byte-swapped one.

Commands confirmed by an immediate in this Bifrost image, or by an OpenNOW encoder whose bytes are tested and whose name lines up with a Bifrost error string:

| Code | Role | Evidence |
| --- | --- | --- |
| `0x0317` | NACK-v2 | `mov $0x317, %esi` at `0x2b2c88`; version byte `2` at `0x2b74d9` |
| `0x0302` | IDR / picture request | OpenNOW `IDR_REQUEST_CODE`; Bifrost error `NVST_R_SERVER_CONTROL_PICTURE_REQUEST_FAILED` |
| `0x0203` | Frame pacing | OpenNOW `FRAME_PACING_CODE`, 28-byte payload |
| `0x0204` | Frame ack | OpenNOW `FRAME_ACK_CODE`, 102-byte payload. Bifrost error `LAST_RECEIVED_F_NUM_FAILED` is the same family |
| `0x0207` | QoS report | OpenNOW `QOS_REPORT_CODE`, 52-byte payload. Error `QOS_STATS_SEND_FAILED` |
| `0x0200` | Keepalive | OpenNOW, 4-byte stream value, every 3 seconds on the reliable channel |
| `0x0206` | Remote input wrapper | OpenNOW. Inner types below |
| `0x020b` | Enable input | Three LE `u32`s: stream, counter, enabled |
| `0x020d` | Gamepad | Descriptor and state. Bifrost also has `gamepad_channel_v1` |
| `0x020e` | Input protocol version | Inbound. Ready when version is 3: `0e 02 02 00 03 00` |
| `0x0308` | Mouse cursor capture | NVB feature type 0. Error `MOUSE_CAPTURE_FAILED` |
| `0x030d` | Track remote cursor image | NVB feature type 8 |
| `0x0320` | Window state | Error `WINDOW_STATE_CHANGE_FAILED`. State 19 is required |
| `0x0321` | System state | Error `SYSTEM_STATE_CHANGE_FAILED` |
| `0x010f` | System cursor | Inbound cursor |
| `0x0110` | Bitmap cursor | Inbound cursor |
| `0x010b` | Haptics output | OpenNOW `HAPTIC_COMMAND_CODE`. Error `HAPTICS_STATE_CHANGE_FAILED` |

Inner remote-input types under `0x0206`, from `nvst_input.rs`, are heartbeat 2, key down 3, key up 4, absolute mouse 5, relative mouse 7, button down 8, button up 9, wheel 10, gamepad 12, haptics-enabled 13, lock-key sync 19, and text 23. Relative motion is one timestamp on the partial channel. Keys and absolute mouse carry two timestamps on the reliable channel. Feature type 6 (haptics enable) is remote-input type 13, not a separate command. An earlier mapping of that feature to `0x0322` was wrong; `0x0322` is the cursor mimic strategy, and Bifrost names `NVST_R_SERVER_CONTROL_MIMIC_CURSOR_STRATEGY_FAILED` and `NVST_R_SERVER_CONTROL_MIMIC_REMOTE_CURSOR_FAILED` as distinct failures. `x-nv-runtime.mimicRemoteCursor` and `mouseCursorCapture` are the SDP counterparts. OpenNOW announces `mouseCursorCapture:3` and `mimicRemoteCursor:0`.

Feature type 10 (mouse acceleration and speed) is the gap the comparison notes already flag. Geronimo exports `IOInterface::sendMouseSettings(bool, int)`, `BifrostSDKExecutor::sendMouseSettings(bool, unsigned int)`, and `SDLEventProcessor::sendMouseSettings`. Bifrost’s format string is `Failed to %s feature mouse acceleration. Nvst Error: %s (0x%x)` and the error name is `NVST_R_SERVER_CONTROL_MOUSE_SETTING_FAILED`. This pass did not recover a byte-exact type-10 payload from either library. The Windows log prints `accel=0, speed=10` and a heap pointer from `nvbFeatureControl`, not the frame. The current `nvst_input.rs` has no `0x0323` encoder, and `activation_chain` does not send a mouse-settings command. `docs/streamer-comparison/features.md` still says this tree emits a reconstructed `0x0323`; that command is not in `nvst_input.rs`.

Other `NVST_R_SERVER_CONTROL_*` names that identify commands without a confirmed immediate in this pass: frame-rate change, audio stats, stats-record notification, network-capture notification, ETW notification, game record, relay of remote input, ping ack, audio surround info, UDP unreachable, max-bitrate change, DRC state, DFC state, QoS preference, data-channel creation, audio config, and L4S state. Geronimo’s `IOInterface::setL4sState` and `sendSystemStatesToServer` / `sendWindowStatesToServer` / `sendVsyncSettingToServer` are the shell-side callers. L4S is also a CloudMatch feature flag. The control failure string means the server-control send can fail after the socket is up; it does not by itself give the command number.

`nvbFeatureControl` is a different API from ServerControl command codes. Geronimo imports it. `GridApp::controlFeatures` loads the object at `this+0x10` and tail-calls vtable slot `0x10` with the feature type in `esi` and a constant `ecx=1`. `GridApp::controlRgbaSupport` uses the same slot with `esi=0xb`. Feature type numbers in the Windows log (0, 6, 8, 10) are this API’s type argument. Several of them are implemented by sending the command codes in the table above. The log line and the wire code are not the same integer.

## `nvbSendInputEvent` under radare2

`nvbSendInputEvent` is a global text symbol at `0x1a2090` in Bifrost (`nm -D`). `pdf` in radare2 shows a 642-byte function. The System V / Itanium return slot is `rdi` (preserved in `r12`). The real arguments are `rsi` (client object), `rdx` (session-id C string), and a 72-byte `NvstInputEvent` passed on the stack. The function returns the slot pointer in `rax`.

Control flow:

1. If `rsi` is null, it writes result code `0x66` into the return object and returns. No session lookup, no log.
2. Otherwise it locks (`call 0x2144c0`) and walks a tree rooted at the global `0x122bdb0` / `0x122bd60`, comparing the client pointer with the `u64` at node+`0x20`. This is a `std::map`/`std::set` lookup of registered clients, not a hash. Misses fall through to the `0x66`-style early return path that actually stores `1` after a second check.
3. On a hit it calls `0x1664a0` on the client, unlocks, then calls `0x166980`. If that predicate returns zero, the result dword is `1`.
4. If the predicate passes and the session id (`r13`) is null or has length zero, it logs `nvbSendInputEvent(). SessionIdentifier is '%s'` through `BifrostClient: Interface` (`0xd2f451` / `0xd30058`), substituting the literal `NULL` (`0xe38507`), and stores result `0x65`.
5. Otherwise it calls `0x167660` with the result slot, the client, the session id, and the event pointer.

`0x167660` is the session gate. It clears the result to `0xffffffff`, locks the client at offset `0x358`, and reads the byte at client+`0x354`. If that flag is clear, the result is `1` and it returns. If the session id is null, the result is `0x65`. It then calls `0x167070` to resolve the session. Failure logs `Session %s is not active.` (`0xd29b9a`) under `BifrostClient: BifrostClient` and stores `0x134`. Success calls `0x1aafd0(session, event)` and stores `eax` as the result.

`0x1aafd0` is a one-instruction trampoline: load the pointer at session+`0x88` and jump to `0x1f8f80`. That function is the stream send. It requires three pointers (object+`0x70`, a vfunc at vtable+`0x90` returning non-null, and object+`0x218`). If any is missing it returns `0x155`. Otherwise it pushes the nine qwords at event offsets `0x00` through `0x40` and calls vtable+`0x38`. A non-zero return becomes result `0x97` after logging `Sending Input event failed: Nvsc Error: %s (0x%x)` (`0xd35de0`) under `BifrostClient: Streamer`. A zero return checks a one-shot byte at object+`0x198`. If set, it clears the flag, zeroes `0x78` bytes, allocates `0x550` bytes, and calls `0x1b7040` with `esi=0xe`, then logs `Sending NVB_SN_CLEAR_IDLE_TIMEOUT with event type: %d` using the first dword of the event as the type. `0x0e` here is an internal notification id on that queue, not ServerControl `0x0206` and not remote-input type 14. The event’s first dword is the NVST input type the log prints.

Geronimo’s call site matches this ABI exactly. `BifrostSDKExecutor::sendNvstInputEvent` is at `0x354a50`. The byte at `this+9` must be non-zero or the function returns without calling Bifrost (streaming not armed). Otherwise:

- `r9 = this+0x70` (the `NVbClient`)
- `rdx = this+0x10` (session id)
- `rdi` = a stack return slot
- nine `push`es of `0x40(%rsi)` down through `(%rsi)`, which is the 72-byte event by value
- `call 0x1d5c00`, the PLT stub whose `jmpq *got` at `0x1d5c05` is the `R_X86_64_JUMP_SLOT` for `nvbSendInputEvent` (GOT `0x20693d0`)

The result dword is read back from the slot. The executor returns true only when that dword is zero (`sete %al`). So in this build, success is numeric 0, regardless of the order of the `NVB_R_*` name list in rodata. That list begins `NVB_R_CANCELLED`, `NVB_R_SUCCESS`, `NVB_R_UNINITIALIZED`, and continues through `NVB_R_INVALID_PARAM`, `NVB_R_INVALID_CLIENT_OBJECT`, `NVB_R_INVALID_PORT_NUMBER`, `NVB_R_INVALID_INPUT_DEVICE`, `NVB_R_SESSION_NOT_ACTIVE`, and stops being a pure `NVB_R_` run after 74 names at `Unknown NVbEvent`. The immediates `0x66`, `0x65`, `0x134`, `0x155`, and `0x97` are outside that short prefix, so the name table is not a dense index of those failure codes. `nvbEnumToString` is how Geronimo turns a non-zero mic-audio result into a name; the input path in the executor does not log, it only returns the boolean.

The wrappers above the executor do not touch bytes. `IOInterface::sendNvstInputEvent` at `0x351ec0` loads `this+0x68` and calls vtable+`0x40`, then returns 0 even if the executor failed. `GridApp::sendNvstInputEvent` (`0x33edf0`) and `GridApp::doSendNvstInputEvent` (`0x3415a0`) are the same tail call: object at `this+8`, vtable slot `0x208`. There is no second encoder in Geronimo. SDL events become an `NvstInputEvent` before this chain. The 72-byte image is what crosses the ABI. Bifrost’s stream vfunc is what later wraps that image in ServerControl `0x0206` or gamepad `0x020d` and writes it to a labeled data channel. This static pass stops at the vfunc call. It does not recover the per-type serializer that turns type 7 into the relative-mouse body OpenNOW builds in `remote_input_packet`.

A nearby function at `0x1ab120` is a different mapper, not the send. It walks a stride-`0x40c` table, switches on a field at offset `0x404` (compared against 11), and writes small integers 1 through 8 into an output descriptor, with a second field at `0x400` selecting among 0, 2, and 3. That is device-capability translation (the same region mentions NVB profile to NVSC profile: `Converted NVB profile %d to NVSC profile %d.`). It is easy to misread those `movl $0x7` stores as remote-input type 7. They are not on the `nvbSendInputEvent` path.

## What OpenNOW already matches

OpenNOW does not load either library. The overlap is behavioral and was checked against this image rather than assumed from the Windows log.

Signaling shape matches: OPTIONS, DESCRIBE, SETUP, ANNOUNCE, optional PLAY, WebSocket RTSPS, session id header, SDP application body. Video is a separate UDP socket with SRTP and a GS RTP extension. Audio, microphone, input, and RTCP are on the ICE/DTLS/SCTP bundle. ANNOUNCE says video is not on the bundle and data/audio are. Reserved ports are preferred and dynamic fallback exists. NACK-v2 is command `0x0317`, version 2, stream ordinal, count, then 10-byte records, length `3+10N`, cap 64, sent on `control_channel_partially_reliable`. The eight channel labels OpenNOW creates are a subset of the labels in this Bifrost. Activation order on the reliable channel (input off, device descriptor, cursor capture, cursor track, window state 19, system state, input on) is an OpenNOW test vector that lines up with the command names and the feature-type errors, not a trace lifted from this `.so`.

The differences that matter for feel and recovery:

- Official channel creation stores factory-returned SIDs. OpenNOW uses fixed even SIDs. Routing is by label.
- Official partial control was observed ordered with two retransmissions. OpenNOW’s partial channels are unordered with a 300 ms lifetime, on purpose, so mouse motion does not head-of-line block.
- Official recovery can invalidate reference frames and freeze the display. OpenNOW NACKs, then PLI plus `0x0302`, and has no invalidation command.
- Official audio and mic SETUP are explicitly bypassed onto WebRtcTransport. OpenNOW never issues a Mjolnir SETUP for those streams.
- Unified-socket mode is compiled into this Bifrost and compiled out of OpenNOW’s ANNOUNCE (`enableUnifiedSocket:0`).
- `input_channel_v1`, `gamepad_channel_v1`, `stats_channel`, `remote_trace_channel`, and `wac_metadata_channel` are absent from OpenNOW’s profile.
- Port numbers 49005 and 49006 are OpenNOW’s preferred pair plus a Windows log. They are not immediates in `gs_04_87`.
- Mouse-settings feature type 10 still has no captured payload in either library.
- The enet/`ServerControlReliableUdp` stack and the dcSCTP tuning attributes are present in Bifrost and reduced to one MTU line in OpenNOW.
- NACK DJB interaction mode, initial wait, and the 52-versus-68 ms policy are attributes and prior analysis, not a path OpenNOW selects dynamically.
- Geronimo drops the Bifrost result on the `IOInterface` path (always returns 0). A failed `nvbSendInputEvent` (`0x134` session not active, `0x97` NVSC send failure, `0x155` missing stream) does not surface as a distinct shell error at that wrapper.

## Limits

This is static analysis of one Linux Flatpak build. Command immediates other than `0x0317` were not re-derived by scanning every `mov $imm` in `.text`; a naive scan of `mov esi, imm32` in the `0x100–0x3ff` range hits hundreds of unrelated constants (buffer sizes, HTTP codes, enum sentinels). The command table above is the intersection of a confirmed site, OpenNOW’s tested encoders, and Bifrost’s `NVST_R_SERVER_CONTROL_*` names. A later `gs_04_*` branch can renumber commands and keep the English strings. The Windows comparison set was explicit that no official binary had been disassembled there. This note is the disassembly those notes did not have, for `gs_04_87` rather than for build 2.0.87.131.

No live session was captured. ICE credentials, SRTP keys, and session ids are not in this write-up and should not be pasted out of logs. Reachability of 49005, whether a given seat honors `useReserved`, and whether the server answers `0x0317` with a retransmitted SRTP packet are runtime questions. The stderr line `NVST NACK sent format=private-v2` records local send admission only.