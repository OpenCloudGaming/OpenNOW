# Fallbacks and recovery

The Windows client has many unrelated fallback mechanisms. A codec choice made before the session is not a decoder retry. Retrying an RTSP endpoint is not provider switching. Withholding frames after loss is not the same as an IDR request. This table separates the decisions that were traced (**D**) or seen in a real session (**L**) from those that are only configured or inferred.

Evidence levels are defined in the [README](README.md). Unless a row says Bifrost, addresses are in Geronimo. The separately supplied Bifrost DLL now confirms network setup and recovery; observed runtime paths remain separately marked **L**.

## Established decisions

| Decision | Behavior | Evidence |
| --- | --- | --- |
| Automatic codec choice | Auto (codec 0 → 7) picks AV1 when the AV1 decode bit and AV1 gate pass, else HEVC, else H.264. The order is the same when HDR is requested. When H.264 is chosen for an HDR or ≥240 fps request, HDR is cleared and fps is capped at 120 (`Select H264 codec and ignore %s %s request`). | **D** `FUN_180157f20`; same order as macOS `0xb1de0`; [video.md](video.md) §3 rows 15 |
| Explicit codec rejection | An explicit codec that cannot decode returns an error with no substitution: H.264 + HDR `0xc0f11042`, HEVC `0xc0f11043`, AV1 `0xc0f11050` (`... No fallback path used.`). Any other capability failure drops to 1280x720@60 H.264. | **D** `FUN_180157f20` row 12 |
| H.264 limits | When the codec is H.264, or H.264 is the only decodable codec, the pixel budget is capped below 4,953,600 and fps at 120. The closest-aspect mode under the cap is chosen. | **D** row 13; **L** capability sweep at 19:35:30 (`3440x1440@120, codec = H264` → `2560x1080@120`) |
| 4:4:4 | A 4:4:4 request forces HEVC, even over an explicit codec, when `platformSupports444Decode` passes; otherwise chroma drops to 4:2:0 and the bit depth is kept. | **D** row 7, `0x18015cd80`; **L** S6 (H.264 request → HEVC 4:4:4) |
| 10-bit SDR | 10-bit SDR needs 10-bit decode and the remote `enableSDR10` switch; otherwise the stream becomes 8-bit with SDR color space 2. | **D** [color.md](color.md) §2 |
| HDR decode | HDR needs `platformSupportsHDR10Decode(codec)`; failure clears HDR. The selector does not read the display's HDR state. | **D** `0x18015d970`; display gate location **U** |
| Explicit codec disabled by remote config | The client caps fps at 120 and retries the config check once; a second rejection returns the codec error. | **D** row 16; **L** `Requested codec AV1 is disabled for the resolution ...` (16 lines across SDR and HDR) |
| HEVC/AV1 without transcode | If the client cannot re-encode to H.264 (GRC), small SDR 8-bit <240 fps requests drop to H.264 only. Otherwise HEVC/AV1 are allowed without transcoding. | **D** row 11, `0x18015dcc0` |
| fps snapping | Requested fps snaps to 30, 60, 90, 120, 144, 165, 240 or 360 before selection. | **D** `platformChooseStreamingSettings` `0x18015b210` |
| Undecodable frames | Each decode failure reports frame state 4 and flushes the decode queue (complete or partial). Repeated failures past a threshold raise `0xc0f11002 GERONIMO_VIDEODECODER_UNRECOVERABLE_BITSTREAM` once; a successful decode resets the count. | **D** `0x1800e92a0`, `0x1800e1260` |
| Reference loss | The client asks the server to freeze display on invalidated references, and the config enables reference-picture invalidation, recovery mode (6000 ms trigger) and intra refresh in recovery. | **L** `Client requests display freeze on Invalidated Reference Frames`; config dump. Runtime 0x301/0x302 sends **U** (none in the logs) |
| NACK on and off | Bifrost turns NACK off and on again by frame number during a high-RTD stretch (`Deactivating Rtp Nack frameNumber:...`, `Activating ...`). `useRtdForRtpNackToggle 1` is configured. | **L** S5 19:27:21 and 19:28:55; RTD-driven policy **I** |
| Frame filters | NvScaler, NvSharp and DeepISP fall back to Lanczos when they fail. DeepISP needs NV12 8-bit input, CUDA 11.2+ and `NvPixels.dll`. | **S** `0x180226a3f`; **D** `0x18015d170` |
| Swap chain mode | Flip-sequential on Windows 8+; bitblt `SEQUENTIAL` when vsync is off and tearing is unsupported. A failed fullscreen-exclusive switch logs `Disabling FSE!!`. | **D** `0x18026cc80` |
| WASAPI rate adjust | `IAudioClient::Initialize` with `EVENTCALLBACK \| RATEADJUST`; on failure it retries with `EVENTCALLBACK` only. | **D** [audio.md](audio.md) §1 |
| Audio device change | Default-device or format changes rebuild the renderer and the mic capturer. | **D** `0x1802597e0`, `0x1802c2c60` |
| Mic AEC | Hardware AEC skips the software APM; otherwise WebRTC AEC runs with decoded game audio as the reverse stream. | **D** `GsAudioWebRTC.dll` `0x180001230`, `0x1802c2710` |
| Input send failure | `Sending Input event failed: NVST_R_GENERIC_ERROR (0x800b0000)` is logged and the session continues. | **L** 42 lines around focus and window-state changes |
| Server prefilter | The client requests a prefilter mode, the server answers, and the finalized mode wins (request 0, server 2, finalized 0 in S6). | **L** 19:45:29 |
| Requested features | `finalizedStreamingFeatures` overrides requests (Reflex requested true, finalized false in S6). | **L** CloudMatch response |
| Resumable drop | Reason 55 with a resumable extended code: the JS app calls `gfnResume` at once, retries once, and gives up after about 10 s (`Auto resume failed!`). Both attempts in the logs failed with `0xC2EC1025`. | **L** console.log 18:48:40-18:48:51; **D** resumable rule `0x180142a70` |
| Idle timeout | The server warns 60 s ahead (timer type 4), the client raises `NVB_SN_APPROACHING_IDLE_TIMEOUT`, then the server ends the session (timer type 5, `0x80030023`), which is not resumable. | **L** S5 and S6 |

## Observed but not established as policy

- **I.** CloudMatch returned two RTSPS endpoints (ports 322 and 48322). Signaling used 322, and Bifrost logged the 48322 URL as `Signaling Server Ip` after ANNOUNCE. Whether 48322 is a reconnect target or an alternate is **U**. The macOS candidate-loop rule (advance on `0x800b1004`) is macOS Bifrost (PR #1161) only.
- **I.** One extra `Retrying DTLS handshake after timeout, retry count 1` appears right after `DTLS handshake done.` in S6. It comes from a second DTLS thread and did not affect the session.
- **U.** No runtime decoder fallback (D3D11VA to MFT to FFmpeg) after a creation failure was found. `VideoDecoderSet::CreateAndInitializeDecoder` (`0x18019bfe0`) only logs `Failed to create platform decoder[%u]`.
- **U.** GPU device removal and hang have error codes (`GERONIMO_DX11_DEVICE_REMOVED`, `GERONIMO_DX11_DEVICE_HUNG`), but the recovery path was not traced.
- **U.** The failed resume (`0xC2EC1025`) and failed setup (`0xC0F52159`) codes have no readable meaning in the supplied artifacts.

## Recommendation for OpenNOW

Keep each fallback as its own typed decision with its own diagnostic, as GFN's log lines do. The codec rules worth matching exactly are the H.264 cap and the Auto order; see [opennow-gaps.md](opennow-gaps.md). Do not copy GFN's remote-config gates, which OpenNOW cannot fetch.
