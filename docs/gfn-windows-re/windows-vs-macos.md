# Windows vs macOS

This table compares the Windows build studied here (2.0.89.141, Geronimo `gs_04_92`) with the macOS study in PR #1161 (`docs/gfn-macos-re/`, macOS 2.0.87.131). The builds are two releases apart, so some differences may be version drift rather than platform design (**I**).

Both Bifrost binaries are now available. Windows packet and signaling claims below have independent function-level evidence, not macOS provenance. The studies agree on several structures, but agreement must be checked byte by byte and enum domains must remain separate.

## Same on both platforms

| Area | Shared behavior | Windows evidence | macOS evidence |
| --- | --- | --- | --- |
| App-to-native boundary | `GFNQueryHandler` reads `deviceId`, `serverAddress`, `clientHeaders`, `keyboardLayout`, `metaData`, `supportedControls`, `spanData`, `streamingProfile`, `partnerCustomData` and calls `GridApp::prepare`, `setAuthInfo`, `start`, `resume` | **D** `GeForceNOW.exe` `0x1401a69c0` ([architecture.md](architecture.md)) | `GFNQueryHandler::OnQueryNative` `0x1000c3660` |
| Engine split | Geronimo is the client facade over Bifrost through the `nvb*` C API | **S/D** 31 imports and matching DLL exports | symbols in `libGeronimo.dylib` |
| Codec selection | Same selector, same branch order: 4:4:4 forces HEVC, explicit-codec failure has no fallback, Auto prefers AV1 > HEVC > H.264, H.264 + HDR is an error | **D** `FUN_180157f20` ([video.md](video.md)) | `chooseMaxSupportedVideoSettings` `0xb1de0` |
| Socket arrangements | RTC media on the bundle, optional unified Mjolnir video, legacy dedicated video socket | **L** legacy path taken on an NVIDIA seat ([handshake.md](handshake.md)) | **D** `setupSessionStreams` `0x1145c0` |
| NACK v2 | Version 2, at most 64 base/mask records under command 0x317 | **D** `0x18047a220`, `0x18047ec60`; Windows LE records, first 64 entries, no sort | **D** serializer `0x256310`; compare retry policy separately |
| Mjolnir receive deadline | Bounded receive queue with configured wait | **D** `0x18046fba0` capacity 0x400; `0x1801fdfc0` converts supplied ms to ns; **L** configured 8 ms | **D** `CNvQueue::Add` `0x25da10`; equal setting does not prove identical scheduling |
| Signaling header version | `14.2` | **L** `X-GS-Version: 14.2` | **D** `connectWithRtspHandshake` `0x1197a0` |
| Mic processing | WebRTC APM with a reverse (render) stream feeding the echo canceller | **D/L** `GsAudioWebRTC.dll`, `processReverseStream` `0x1802c2710` | **D** `libGsAudioWebRTC.dylib` |
| Server types | `NVB_SERVER_NIMBUS`, `GAMESTREAM`, `ROAMING_PROXY`, `CLOUD_STATION`, `QUADRO_STATION`, `PASSTHRU` | **D** numbers recovered at `0x1401a2c00` ([providers.md](providers.md)) | names only |

## Different on Windows

| Area | Windows | macOS | Evidence |
| --- | --- | --- | --- |
| Process model | Geronimo, Bifrost and SDL2 load in-process in the CEF shell. `GeForceNOWStreamer.exe` only relaunches `GeForceNOW.exe` | streamer stub inferred to be a launcher | **D** `GeForceNOWStreamer.exe` `0x140009eb0` |
| Background services | NvContainer host with `GameStreamClientAgent`, `GeForceNOWReliabilityMonitor` and `NvMessageBusBroadcast` over named pipes | `libGameStreamClientAgent.dylib` in an `nvc` folder | **L** NvContainer logs |
| Hardware decode | D3D11VA with Geronimo's own SPS/PPS parser and DXVA picture parameters; MFT, DXVA2/DX9 and FFmpeg (`avcodec-59`, software H.264) as alternates | VideoToolbox | **D** `platformCreateVideoDecoder` `0x18015bc10` |
| Capability probe | Creates real `ID3D11VideoDecoder` objects per candidate resolution; per-GPU disabled-config list; long/short slice | VideoToolbox support masks; `hevc120`; AV1-broken OS check | **D** `0x1801f9e90` |
| 4:4:4 decode | `HEVC_VLD_Main_444` / `HEVC_VLD_Main10_444` into AYUV/Y410; optional second D3D11 device on NVIDIA and Intel | `444v`/`x444` pixel buffers | **D** `0x180201e50` |
| Presentation | DXGI flip-sequential, 2 buffers, RGBA8 or RGB10A2, tearing, waitable object, `SetMaximumFrameLatency(1)`; render thread at time-critical priority | CAMetalLayer | **D** `0x18026cc80`, `0x18027b4b0` |
| HDR output | `SetColorSpace1(RGB_FULL_G2084_NONE_P2020)`; no `SetHDRMetaData` call | EDR layer with `CAEDRMetadata` from stream SEI | **D** ([color.md](color.md)) |
| Display HDR test | `DisplayConfigGetDeviceInfo` advanced-color supported/enabled bits plus BT.2020 or >400 nits; SDR white level from display config | EDR headroom | **D** ([color.md](color.md)) |
| Upscaling | Lanczos, NvScaler, NvSharp, Shepherd3/6 and DeepISP (CUDA in `NvPixels.dll`, NV12 8-bit only, falls back to Lanczos) | not compared (**U**) | **D/S** ([video.md](video.md)) |
| VRR | `NvAPI_D3D_SetVRRState`, cloud G-Sync, VVSync | display-sync flag in the Metal path | **D** |
| Audio playback | WASAPI shared, event-driven, `RATEADJUST` drift correction, ducking off, 2/6/8 channels | Audio Units renderer symbols | **D/L** ([audio.md](audio.md)) |
| Raw input | One Raw Input thread (`RIDEV_INPUTSINK`) for keyboard and mouse; GFN key-code space from SDL scancodes | not compared in detail (**U**) | **D** ([input.md](input.md)) |
| HID passthrough | `GSHID` raw HID forwarding for listed wheels and flight sticks, from remote config plus `RIDevices.json` | not studied (**U**) | **S/L** |
| Telemetry export | OpenTelemetry over gRPC, statically linked from a vcpkg tree named "Mjolnir" | not studied (**U**) | **S** |

## What the Windows logs add to the macOS findings

- The CloudMatch JSON values seen on an NVIDIA seat are 14 (SIGNALING) for stream endpoints and 10 for session control. They are not native `NVB_PORT_USAGE` values: native RTSPS is 10 and BUNDLE is 11. Windows conversion supports JSON 2 VIDEO, 14/16 signaling and 17 BUNDLE; only part of that table is exercised in these logs.
- The media peer for a non-unified session comes from the SETUP `Transport` header (`X-GS-ServerPort`), not from SDP ports.
- Six named SCTP data channels and the `udp_ag` control protocol choice.
- The NACK timing constants as configured in production, including the 1 ms initial delay.
- The idle-timeout timer notifications and the termination codes for idle and resumable drops.
- The frame-state values Geronimo reports to Bifrost (`nvbUpdateVideoFrameState`), which are the client half of frame acknowledgement.

## Corrections and independent Windows protocol evidence

- The GS marker is wire bytes `47 53`. Windows reads a network-order u16 and compares `0x4753` (`0x180390c30`). The macOS comparison literal `0x5347` was not paired with a proved helper byte order, so the literals do not establish a platform mismatch. The GS capture time is BE u32, but its unit remains unknown.
- Windows copies the 16-byte Mjolnir extension raw (`0x18046a3e0`) and parses the FEC bit fields in `0x180749930`. The packet layout is independently recovered in [packet-layouts.md](packet-layouts.md).
- Windows ANNOUNCE compares the desired config with DESCRIBE and emits changed values. Its depth/chroma byte writer emits configured bytes unchanged (`0x1803f7970`). S6 depth/chroma are 10/1, so depth agrees with OpenNOW's literal 10 but 4:4:4 chroma differs from its codec IDC 3. CloudMatch JSON flags 1/1 are separate. Final config alone still cannot reconstruct the S6 body. See [announce-builder.md](announce-builder.md).
- Windows distinguishes normal RFI 0x301, IDR 0x302 and NvWac invalidation 0x30c. Active RFI/IDR maintenance retries strictly after 100 ms. Neither an absent command log nor zero RFI counters proves that no request was sent.
- Windows frame-ack v9 is a 102-byte outcome/timing record. Progress notifications are not wire ack versions or evidence that a frame was presented. QoS v7 and timing v5 carry measured feedback; unresolved private fields retain offset names in [transport-feedback.md](transport-feedback.md).
- Windows verifies the SHA-256 DTLS peer fingerprint before SRTP export. WSS certificate-chain/hostname and conditional CA-pinning checks are a separate unknown, not resolved by this DTLS gate. Windows DCEP advertisements also need their own reliability/priority table rather than borrowing the macOS labels.

The earlier claim that all overlapping Windows and macOS evidence agrees was too broad. These corrections narrow interpretations where macOS helper semantics, domains, or Windows runtime observations did not establish equivalence. They do not demonstrate a server incompatibility.
