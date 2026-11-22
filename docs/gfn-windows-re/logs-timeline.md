# GFN Windows client 2.0.89.141: session timeline from the supplied logs

Scope: runtime evidence from the user's Windows logs only. Tools: `extract.py` (parses and redacts), `compare_opennow.py`, `render.py`. Generated, already-redacted intermediates live in `extracted/`. Rerun with `python3 extract.py && python3 compare_opennow.py && python3 render.py`.

Conventions:

- All times are local wall clock, UTC+2 (CEST). `geronimo.log*` lines carry a UTC bracket timestamp plus the local time between `=...=`. For example, `[2026-10-12 17:45:11,637]=19:45:11=` is 19:45:11 local. Other logs use local time.
- Evidence tags: **L** log-confirmed (file, local time, short redacted quote), **I** inferred, **U** unknown, **S** static. "macOS Bifrost (PR #1161)" marks facts from the macOS research. Those facts are not Windows evidence.
- The original log study preceded the separate Bifrost upload. Every `[NVST:*]` / `[BifrostClient: *]` line here is runtime output printed into `geronimo.log`. Static Windows findings are now recorded in the linked protocol documents; they do not turn this timeline into a packet capture.
- IPs, session/sub-session/request IDs, the device hash, HMAC/keys, ICE/DTLS material, the Windows user name, and the user's age are redacted.

## 1. What the logs cover

| log | time range (local) | session coverage |
|---|---|---|
| `logs/gameStreamClientAgent.log.bak` + `.log` | 2026-10-11 16:40 to 2026-10-12 22:39 | all 7 launches (IPC events from BackgroundAgent) |
| `console.log` + `.bak` (JS app) | 2026-10-12 18:31 onward | launches 3 to 7 (profile, queue, mic, resume) |
| `CxNative_GeForceNOW*.log` (CEF native) | both days | app start/stop per launch, NetworkTest |
| `debug.log` + `.bak` (Chromium + Geronimo glue in the CEF process) | both days | QUERY_GFN_* handlers, IPC sends |
| `geronimo.log.bak` | 19:22:59 to 19:59:30 (starts mid-session) | tail of S5, start of S6 |
| `geronimo.log` | 19:59:30 to 23:33:04 (after rotation) | rest of S6, app shutdown |
| `logs/gfn_reliability_monitor.log*`, `logs/NvContainer/*` | both days | relayed IPC events, plugin load |

**L**: geronimo.log begins with "Performing log rotation." and its first entry is 19:59:30, which is where geronimo.log.bak ends. Geronimo logs for S1 to S4 were rotated away. So the **only full CloudMatch, RTSP, and SDP handshake in the upload is S6** (2026-10-12 19:45). S5 has only its last 10 minutes, including the termination.

## 2. Sessions found: 7 launches, 6 streaming sessions

The labels S1 to S6 are local labels (session IDs are redacted). Sources: `extracted/sessions.tsv` and `extracted/launch_sequences.txt`.

| launch | session | launch start | setup -> started | streamed | end reason (IPC) | extendedCode | resumable / alive | after end | geronimo coverage |
|---|---|---|---|---|---|---|---|---|---|
| 1 | S1 | 10-11 16:40:25 | 19.2 s -> +7.7 s | 126.0 min | 55 | 0x80040010 | true / true | no resume logged (console not covering) | none |
| 2 | S2 | 10-11 18:47:16 | 28.5 s -> +0.4 s | 75.5 min | 56 | 0x80030007 | false / false | exit | none |
| 3 | S3 | 10-12 18:31:57 | 20.0 s -> +0.2 s | 16.4 min | 55 | 0x80040010 | true / true | auto-resume failed, `SESSION_RESUME_FAILURE` result 0xC2EC1025 | none |
| 4 | none | 10-12 18:49:55 | none | none | `SESSION_SETUP_FAILURE` 0xC0F52159 at +94 s | none | none | user relaunched | none |
| 5 | S4 | 10-12 18:53:30 | 63.5 s -> +0.2 s | 8.7 min | 55 | 0x80040010 | true / true | auto-resume failed (cached terminated event), 0xC2EC1025 | none |
| 6 | S5 | 10-12 19:07:15 | 25.2 s -> +0.2 s | 26.0 min | 56 | 0x80030007 | false / false | exit | last 10.7 min |
| 7 | S6 | 10-12 19:45:11 | 17.8 s -> +1.4 s | 29.9 min | 56 | 0x80030007 | false / false | exit (app stayed open until 22:39) | full |

Common facts for all launches. Every launch went to zone `eu-netherlands-north.cloudmatchbeta.nvidiagrid.net` and the title "Another Hardest Game Demo" (CMS id 106275040). **L**: gameStreamClientAgent `IPC_STREAMING_PREPARE_EVENT {"serverAddress": "eu-netherlands-north..."}`. The JS profile for S3 to S6 was the same: `2560x1440@120 bitDepth=10 chromaFormat=1 hdr=OFF codecPref=0 audioMode=2 l4s=False reflex=True`. **L**: console, for example 19:45:11.615.

Termination codes. All values are **L** from IPC TERMINATED events and the Bifrost lines for S5/S6.

- **Reason 56 (0x38), extendedCode 0x80030007, not resumable.** In S5 and S6 this is the server idle timeout. The sequence is a `timer notification of type 4 with 60 seconds left`, then `NVB_SN_APPROACHING_IDLE_TIMEOUT`, then a type 5 timer at 0 s, then `Server sent termination notification NVST_DISCONN_SERVER_TERMINATED_CLOSED (0x80030023)`. After that come `SessionTerminationReason: 0x104, NvbSessionTerminationType: 56, ExitCode: 0x80030007, SessionAlive: 0` and `Resumable: 0, SessionAlive: 0 ... terminationReason:0x00000038`. **L**: geronimo.log.bak 19:32:41 to 19:33:41 (S5); geronimo.log 20:14:24 to 20:15:24 (S6). S2 has the same code, so it was **I** probably also idle.
- **Reason 55 (0x37), extendedCode 0x80040010, resumable and session alive (S1, S3, S4).** No geronimo logs exist for these. In S3 and S4 the JS logged `frameloss: onFrameLossWarning` 8 to 9 s before termination (**L**: console 18:48:31.587, 19:03:09.573). **I**: this is a client-side media or network loss disconnect while the seat stays alive. The exact NVST_DISCONN name is **U** (it lives in Bifrost2.dll).
- **Resume.** On reason 55, `streamingService attemptAutoResume` called `gfnResume`. The resume then failed with `IPC_STREAMING_FAILURE_EVENT SESSION_RESUME_FAILURE result -1024716763` (0xC2EC1025), was retried once in S3, and ended in "Auto resume failed!" and "Terminating auto resume.". **L**: console 18:48:40.068 to 18:48:51.638. No successful resume exists in the logs.
- **Setup failure.** Launch 4 failed with `GeronimoLaunchError` `0xC0F52159` after 93.6 s of `streamerConnectionDuration` (**L**: console 18:51:28.309). The NetworkTest step before it took 9.0 s.

## 3. S6 end-to-end (representative session)

T0 = `nvbStartSession` at 19:45:11.637 (geronimo.log.bak). Relative times come from `extracted/geronimo_timeline.txt`.

| T+ (s) | local | step | evidence |
|---|---|---|---|
| -0.12 | 19:45:11.51 | JS launch steps: streamerJsInit 40 ms, nesAuthorization 20 ms, networkTest 42 ms (cached), streamerClientSetup 36 ms | **L** console "Game launch duration for ..." |
| -0.02 | 19:45:11.615 | Platform picks the profile: `Will request 10 bit SDR video for codec HEVC`. `Resolution selected ... 2560x1440@120, codec: HEVC ... bitDepth = 10, chroma = 3`. DXVA `Using YUV444 format for codec 2` | **L** geronimo.log.bak 19:45:11.615-.619 |
| -0.01 | 19:45:11.626 | `WASAPIAudioRenderer ... nChannels: 2, bitsPerSample: 16, samplesPerSec: 48000` | **L** |
| 0.000 | 19:45:11.637 | `nvbStartSession`, then the IPC_STREAMING_STARTING + 3x IPC_GERONIMO_PAYLOAD (CLIENT_INFO, RCONFIG, USER_INFO) sends | **L** geronimo.log.bak, debug.log 19:45:11.637 |
| 0.002 | 19:45:11.639 | `Using Secure WebSockets for Signaling`. `Serialized session request. Size: 1957`, POST `https://eu-netherlands-north.cloudmatchbeta.nvidiagrid.net:443/v2/session?keyboardLayout=en-US&languageCode=en_US` | **L** |
| 0.422 | 19:45:12.059 | First response: status 1, `seatSetupStep 0`, ETA 30000, two `connectionInfo` usage 14 entries with null `resourcePath`. `otherUserSessions[0]` echoes the same pending session | **L** |
| 0.47 to 17.78 | 19:45:12.1 to 29.4 | 18 polls about every 1.02 s: GET `https://np-ams-08.cloudmatchbeta.nvidiagrid.net:443/v2/session/<id>`. `seatSetupStep 3`, ETA 20000 from T+1.49. The rtsps `resourcePath` appears at step 3 | **L** `Polling for session <id>, request id: 11, QueuePosition: 0, ETA: 20000(msec)` |
| 17.778 | 19:45:29.415 | Ready: status 2, `gpuType "5080h / B40"`, `finalizedStreamingFeatures` present. `ConnectionInfo[0]: ... :322, usage: NVB_PU_RTSPS`, `[1]: ... :48322, usage: NVB_PU_RTSPS` | **L** |
| 17.779 | 19:45:29.416 | Prefilter: client requested mode 0, server responded 2, finalized 0. `Converted NVB profile 8 to NVSC profile 1` | **L** |
| 17.785 | 19:45:29.422 | `nvbStartStreaming`. `connectionInfoCount is 2 but maxNumOfPorts is 20`. `Setup for seat configuration only. Initiating teardown of the nvst client instance.` | **L** |
| 17.80 | 19:45:29.437 | IPC_STREAMING_SESSION_SETUP_EVENT. JS seatConfigDuration 16310 ms | **L** debug.log, console |
| 17.821 | 19:45:29.458 | `NVST:WebSocket Connecting to host <redacted-ip>.cloudmatchbeta.nvidiagrid.net, port 322` | **L** |
| 18.923 | 19:45:30.560 | WSS connected (1.1 s TCP+TLS+WS). `WSS client using 1-way SSL`, `Header version: 14.2` | **L** |
| 18.930 | 19:45:30.567 | OPTIONS -> `HTTP/1.0 200 OK`, `Public: OPTIONS,DESCRIBE,ANNOUNCE,SETUP,TEARDOWN,PLAY,PAUSE,X_NV_COMMAND,X_NV_EVENT` (7 ms) | **L** |
| 19.000 | 19:45:30.637 | DESCRIBE -> 200, `Content-Length: 219292` (70 ms). `Successfully read 4145 NvscClientConfig attributes from SDP` | **L** |
| 19.006 | 19:45:30.643 | Control protocol `udp_ag (2) - not encrypted`. `Selected HEVC codec from client preferences`. `Client requests display freeze on Invalidated Reference Frames`. `Client supports intra refresh frames`. `QEC feature is disabled since client requested for display freeze` | **L** |
| 19.017 | 19:45:30.654 | `Bypassing 'Mjolnir' SETUP for the Audio stream` / `Mic stream. Deferring to WebRtcTransport setup`. SETUP `streamid=video/0/0` from client port 49005 | **L** |
| 19.190 | 19:45:30.827 | SETUP -> 200, `Transport: unicast;X-GS-ServerPort=5004-5005;source=<redacted-ip>`, `X-Nv-Ping-Payload: <redacted>`, `x-nv-ping: 6` (173 ms) | **L** |
| 19.191 | 19:45:30.828 | `Set ping-version: 6`, `Old server only supports PING`. STUN RFC 5389 on 49005. Early ICE/bundle socket bound to 49006. `Encryption key in RTSPS`. Skipped ICE ufrag/pwd/DTLS fingerprint SDP fields. `Successfully appended 163 NvscClientConfig attributes to SDP` | **L** |
| 19.193 | 19:45:30.830 | `RtspConnectLatency took 1405 ms`. `onStreamerConnected: stream[0] resolution: 2560x1440`. IPC_STREAMING_STARTED_EVENT sent **before** ANNOUNCE/PLAY. `Configuring server supported Hid devices: Supports DS4: yes, DS5: no (raw:4)` | **L** geronimo.log.bak, debug.log 19:45:30.830 |
| 19.239 | 19:45:30.876 | ANNOUNCE (382 ms to 200) | **L** |
| 19.621 | 19:45:31.258 | Final config dump (975 keys, `extracted/nvsc_config.tsv`). `general.serverEndpoints: RTSP hanshake 322 control 47995 UDP control 47999 audio 48000 input 47995 bundle 48001 stream 0: video stream 47998 ...` | **L** |
| 19.630 | 19:45:31.267 | Bundle socket 49006: ping v6 to `<redacted-ip>:5004` | **L** `Send src ping version 6 on port 49006 to dest <redacted-ip>:5004` |
| 19.686 to 19.700 | 19:45:31.323-.337 | `Setup dtls`, peer `<redacted-ip>:5004`, `DTLS handshake done.` (14 ms). `Unified socket RiverMax: PIPELINE_RTC_TRANSPORT_READY published` | **L** |
| 19.717 to 19.725 | 19:45:31.354-.362 | SCTP data channels 0, 2, 4, 6, 8, 10 created (see section 4) | **L** |
| 19.725 | 19:45:31.362 | Audio RED level 2, mic RED level 3, Opus mic 16 kbps. `libsrtp2 2.7.0`, `SRTP for video is enabled.` NACK queue setup | **L** |
| 19.727 | 19:45:31.364 | `qosFeedbackVersion: [7]`, `qosTimingsVersion: [5]`, `Dynamic FEC is enabled`, `Unified socket mode disabled (enableUnifiedSocket=0, using legacy dedicated sockets)` | **L** |
| 19.729 to 19.843 | 19:45:31.366-.480 | PLAY -> 200 (114 ms). `Successfully started streaming.` | **L** |
| 19.844 | 19:45:31.481 | `Sending haptics state 1 to server` | **L** |
| 19.897 | 19:45:31.534 | `Server sent HdrMode update notification (hdrMode = 0) for stream 0` | **L** |
| 19.898 | 19:45:31.535 | `UDP RTP Source Connect: connected to: <redacted-ip>:5004`, `received first payload` | **L** |
| 19.914 | 19:45:31.551 | First decode: `Stream bit depth changed to: 10`, `chroma format changed to: 3`, `colorspace changed to: 2 hdr: 0 range: limited`. DJB config FIXED. `createDx11SwapChain ... 2560 x 1440, vsync = 0, pq = 0, bit depth = 10` | **L** |
| 20.050 | 19:45:31.687 | `Stream resolution changed to: 2560x1440` | **L** |
| 1752.5 | 20:14:24.163 | Idle warning: timer type 4, 60 s | **L** geronimo.log |
| 1812.6 | 20:15:24.271 | `NVST_DISCONN_SERVER_TERMINATED_CLOSED (0x80030023)`, then teardown and stats | **L** |

The phase totals agree with Bifrost's own telemetry. The JS reported startSession at 19253 ms and streamingStartDuration at 1386 ms. Telemetry at disconnect reported `FirstFrameLatency` duration 2110 and `rtspHandshakeLatency` 2056 for S6, and 988/964 for S5. **L** geronimo.log 20:15:24.272 and geronimo.log.bak 19:33:41.366. The units are probably ms (**I**).

## 4. Per-session negotiated media and transport (S5 tail and S6)

Only S6 has the negotiation. S5 values come from its teardown lines.

| item | S6 value | evidence |
|---|---|---|
| codec | HEVC (`Selected HEVC codec from client preferences`; codecPref 0 = auto; JS `codec: H265`) | **L** 19:45:30.643 |
| resolution / fps | 2560x1440 @ 120 (`Nvst selected stream settings: Stream[0] 2560x1440 @ 120 fps, 0 kbps`) | **L** 19:45:29.424 |
| bit depth / chroma | 10-bit 4:4:4. CloudMatch JSON flags 1/1; Bifrost config depth/chroma 10/1. ANNOUNCE body not logged. Decoder chroma 3 and Y410 output (`format 0x65`, `is10Bit=1`) | **L** 19:45:31.260,.542-.551 |
| HDR / color | SDR. `hdrMode = 0`. `colorSpaceMode 0x2, hdrColorSpaceMode 0x4, dynamicRangeMode 0`. Decoded colorspace 2, limited range | **L** |
| decoder | DXVA D3D11 HEVC (`Video decoder is already initialized with type H265`, debug.log 19:45:11.633). `maxH264Level = 42` setting | **L** |
| swapchain | DX11, 2560x1440, vsync 0, PQ 0, bit depth 10 while streaming. Back to 8-bit and vsync 1 at the 1232x761 window after exit | **L** 19:45:31.551, 22:39:39 |
| audio | WASAPI render 2 ch / 16-bit / 48 kHz. `Channel count supported 2`. `Number of channels(2) is not valid for surround configuration` (ERROR, harmless). `audio.surround.enable 0`. Opus, `Audio redundancy enabled using RFC2198 (RED)` level 2. `FEC VBR mode = false` | **L** |
| mic | Permission allowed. Capturer started at 19:45:30.865 (`micindex 0, echoCancellationEnabled 1`). JS `mic: Unmuted`. Mic RED level 3, Opus 16000 bps. `mic.numChannels 1` in config. Encoder line says `channels 2` | **L** |
| audio/mic transport | On the WebRTC bundle socket (`Bypassing 'Mjolnir' SETUP ... Deferring to WebRtcTransport`; server SDP `rtcAudioOnNativeBundle:1`, `rtcMicOnNativeBundle:1`, `nativeRtcOnBundlePort:1`) | **L** |
| video transport | Legacy dedicated socket: client UDP 49005 -> server `<ip>:5004`. Encrypted with SRTP (`runtime.videoSrtp 1`; `percPacketsDroppedByAuthFail 0`) | **L** |
| unified socket | **Disabled**: `enableUnifiedSocket=0, using legacy dedicated sockets`. `rtcVideoOnNativeBundle 0` | **L** 19:45:31.364 |
| server media port | Both client sockets (video 49005 and bundle 49006) send to the **same server port 5004**, taken from `X-GS-ServerPort=5004-5005`. Telemetry: `Media Endpoint Details ... Selected endpoint: <redacted-ip>:5004 \| Configured endpoint: none \| Protocol: udp`. The SDP/config ports (`serverBundlePort 47999`, `bundle 48001`, video 47998) were **not** used as peers | **L** 19:45:31.267-.535 |
| NAT hole punch | `pingVersion 6` (SETUP header `x-nv-ping: 6`), ping string `PING`, STUN RFC 5389 on both client sockets. ICE `MaxStunResponseRetryCount: 255`. `No need to capture candidates for cloud server` | **L** |
| DTLS / SRTP | DTLS on the bundle (14 ms handshake, one `Retrying DTLS handshake after timeout, retry count 1` logged right after done). libsrtp2 2.7.0. `SRTP encrypt failures: total=0`. `runtime.audioSrtp 0`, `micSrtp 0` but server `audio encryption support = true` | **L** |
| data channels (SCTP over DTLS) | id 0 `control_channel_reliable`, 2 `custom_message_on_sctp_private_reliable`, 4 `custom_message_on_sctp_private_partially_reliable`, 6 `control_channel_partially_reliable`, 8 `control_channel_unreliable`, 10 `input_channel_partially_reliable` | **L** 19:45:31.360-.362 |
| control / input protocol | `udp_ag (2) - not encrypted`, `controlControlId: streamid=control/10`. `general.useReliableUdp 10`. Input is partially reliable (`ri.partialReliableThresholdMs 300`) | **L** |
| input / haptics | Haptics state 1 sent. Gamepad VID 1356 (0x054C) / PID 3302 (0x0CE6), type GAMEPAD. `Supports DS4: yes, DS5: no`. Mouse acceleration disabled. `initMouse: mask 0/1` toggles as focus changes | **L** |
| NACK | `NACK waiting queue limit time: 52 ms ... max allowed NACK retries: 3`. `Max allowed pending NACKS: 2048`. At end: `numNackRequest=3, numNackedPacketsRequested=8, numNackedPacketsReceived=8, numNackedPacketsUtilized=4, numNackedPacketsRetried=0` | **L** 19:45:31.363, 20:15:24.280 |
| RTP stats | `numTotalRxPkts=739602, numOutOfOrderRxPkts=40, numDropEvent=6, numRxDroppedPackets=9` (all `numRxDroppedFrameComplete`). Replay/auth drops 0 | **L** 20:15:24.280 |
| loss / drops | Packet loss 0.00% in all 5 real-time metric epochs. Min loss score 92. Frame drop 0.00%. `AsyncFrameQueue Total video frames dropped : 111` at teardown | **L** |
| IDR / invalidation | No runtime IDR, keyframe, or invalidation request logged. Config only: `refPicInvalidation 1`, `streamRecoveryType 2`, `enableRecoveryMode 1` (6000 ms), `enableIntraRefreshInRecoveryMode 1`, `encoderLtrFeatureSetting 2` | **L** config dump; **U** runtime |
| "Received old frame" | Once at teardown: `Received old frame - current 175834 received 0` (a teardown artifact, frame 0) | **L** 20:15:24.286 |
| QoS / bitrate | `qosFeedbackVersion 7`, `qosTimingsVersion 5`. Dynamic FEC on. `Enabling dynamic maximum bitrate feature`. Max bitrate 48543 kbps for 2560x1440@120 (`PlatformGetMaxBitRate`), `initialBitrateKbps 12135`, `minimumBitrateKbps 1000`. Observed `asr(kbps)` about 2.5 to 3.4 Mbps (static demo game). No bitrate-change line logged | **L** |
| quality scores | 162 samples. Q 97 to 100. Avg Game FPS 6.2 to 122.4 (mean 104). RTD avg about 5 to 8 ms | **L** `extracted/runtime_stats.txt` |
| L4S | Off (`L4S session parameter: 0`, `enabledL4S false`, `l4sHandling.enableEcnFeedback 0`) | **L** |
| GPU | `5080h / B40` | **L** |

S5 tail (19:22:59 to 19:33:41). Its quality samples had Q min 0, latency score min 0, and Avg FPS down to 1. Two real-time metric epochs reported `Max RTD: 202 ms; Avg RTD: 168 ms` and `Max RTD: 671 ms`. Bifrost logged `Deactivating Rtp Nack frameNumber:125259` at 19:27:21 and `Activating Rtp Nack frameNumber:127903` at 19:28:55, during the high-RTD stretch. **I**: NACK is turned off when RTD makes retransmission useless. The final counters were `numNackRequest=16, ... Utilized=14, Retried=6` and `numDropEvent=35, numRxDroppedPackets=44`. Feedback retries: `0.1808% ... 56 out of 30978`. It ended by idle timeout, the same as S6.

## 5. App-side flow (who hands the session to whom)

Sources: `extracted/app_flow.txt`, `extracted/launch_sequences.txt`, debug.log, reliability monitor, and NvContainer logs.

1. **JS app (console.log).** `StreamerManagerService` walks the states UpdateCheck -> PreStreaming -> BrowserSupportCheck -> Authentication -> PlatformSelection -> AccountLinking -> Authorization -> NetworkTest -> Loading -> Streaming -> PostSessionConnection -> PostStreaming -> Done. **L** console 18:31:56.700 to 18:48:53.489. GraphQL catalog calls go to `apps.gxn.nvidia.com/graphql` with headers `NV-Device-Type, NV-Device-OS, NV-Client-Type (NATIVE), NV-Client-Streamer (NVIDIA-CLASSIC), NV-Device-Model, NV-Device-Make, NV-Client-Version (2.0.89.141), NV-Client-ID (<redacted>), Content-Type` (**L** console 18:31:52.074 `LcarsSdkService Headers for static call are: ...`).
2. **CEF native plugins (CxNative).** These are loaded in-process: BackgroundProcess, Discord, Gallery, GEAR, GfnUpdater, LoginId, MessageBusRouter (modules MessageBusRouter, ClientAgent, BackgroundAgent), NetworkTest, NvCamera, ShareServer, SystemInfo. MessageBus joins `GameStream:ClientAgent` and `CrimsonNative:BackgroundAgent`. **L**. The NetworkTest plugin (NCTSDK) posts `{"netTestRequestData":{"clientPlatformName":"gfnpc streamer_client","netTestProfile":{...5120x2880@120}}}` with 8 headers. It then gets a netTest session (UDP port 5002 + HMAC key, redacted) and reports zone `NP-AMS-08`. **L** CxNative 18:49:45.890 to 18:49:51.157.
3. **Geronimo in the CEF process (debug.log, pid 13432).** The JS calls `QUERY_GFN_PREPARE`, then `ConfigureStreamerVideoSettings` (profile 2560x1440@120, `maxPacketSize / customPacketSize: 1432`, reflex 1), then `QUERY_GFN_START` (server zone, `Pre-NVb-conversion ServerType : 1` -> `NVB_SERVER_NIMBUS`, AppID, keyboard en-US, locale en_US, `AudioMode ... 2`, `L4S session parameter: 0`, `Vsync settings value ... 0`). **L** debug.log 19:45:11.608 to .637. Geronimo then calls Bifrost `nvbStartSession`.
4. **Geronimo -> BackgroundAgent IPC.** `gfn_background_agent_ipc.cpp` sends `IPC_STREAMING_STARTING_EVENT` and `IPC_GERONIMO_PAYLOAD_EVENT` x3 (`AgentPluginHandler` CLIENT_INFO 2.0.89.141, RCONFIG `remoteConfigVersion 106.0`, USER_INFO). It then sends `IPC_STREAMING_SESSION_SETUP_EVENT` (sessionId, subSessionId, traceRouteAddress/Port 62767), `IPC_STREAMING_STARTED_EVENT`, `IPC_STREAMING_TERMINATED_EVENT` (reason, extendedCode, isResumable, isSessionAlive, udsData) and finally `IPC_STREAMING_MODE_EXIT_EVENT`. **L** debug.log. Routing: `CrimsonNative:BackgroundAgent -> GameStream:ClientAgent` (65 routed messages in gameStreamClientAgent.log).
5. **GeForceNOWContainer.exe (NvContainer 1.51).** It hosts the plugins `GfnBackgroundAgent`, `NvMessageBusBroadcast` (v3.21, named-pipe server, client pid = CEF process) and `GameStreamClientAgent`, plus `NvTelemetryPlugin`. **L** NvContainer logs. The reliability monitor (`gfn_streamer_diagnostics.cpp`) relays each streaming event and Geronimo payload, logs `streaming terminated (0x00000037/0x38)`, and uploads logs after each session (`Log files have been sent.`).
6. **UDS (user diagnostics).** At termination, `GeronimoUDSHandler` sends SA scores plus the keys `Av1/HevcDecodeMax*` (7680x4320), `MaxStreaming*` (2560x1440@120), `Rtd`, and Wi-Fi/router fields. **L** geronimo 19:33:41.380. Endpoint `uds.geforcenow.com/v1/uds/session/reports` (**L** URL list).
7. **GeForceNOWStreamer.exe** does not appear in any log line. The only mention of the name in the upload is inside `GeForceNOWReliabilityMonitor.dll` (**S**). **I**: on this install, streaming runs in-process in the CEF client via Geronimo.dll. The 500 KB Streamer.exe stub is not launched during these sessions.

HTTP endpoints seen in the logs (no secrets):

| endpoint | method | seen in |
|---|---|---|
| `https://eu-netherlands-north.cloudmatchbeta.nvidiagrid.net:443/v2/session?keyboardLayout=..&languageCode=..` | POST (create, **I** from "Serialized session request") | geronimo 19:45:11.639 |
| `https://np-ams-08.cloudmatchbeta.nvidiagrid.net:443/v2/session/<sessionId>` | GET poll (**I**) | geronimo 19:45:12.060 |
| `https://eu-netherlands-north.cloudmatchbeta.nvidiagrid.net/v2/serverInfo` | GET (**I**) | console/CxNative (19 hits) |
| `https://prod.cloudmatchbeta.nvidiagrid.net/` | (**U**) | 6 hits |
| `https://apps.gxn.nvidia.com/graphql` | POST GraphQL | console (104 hits) |
| `https://mes.geforcenow.com/v4/subscriptions` | GET (**I**) | 43 hits |
| `https://pcs.geforcenow.com/v1/serviceUrls`, `https://pns.geforcenow.com/v1/registrations`, `https://uds.geforcenow.com/v1/uds/session/reports` | (**U**) | logs |
| `https://login.nvidia.com/token`, `/userinfo` | OAuth | logs |
| `https://rconfig.nvidiagrid.net/v2`, `https://gx-target-rconfig-frontend-api.gx.nvidia.com/rconfig/v2` | remote config | logs |
| `https://ota.nvidia.com/release/available` | updater | reliability monitor |

CloudMatch HTTP request header names are not logged by Bifrost (**U**). Only the GraphQL header set above is visible.
