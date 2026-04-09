# Streamer Investigation: Current Implementation & External Streamer Direction

This document records findings from investigating the current OpenNOW streaming stack and provides a recommended direction for the dedicated external streamer (OPEN-23).

---

## 1. Current Implementation

### 1.1 High-Level Architecture

OpenNOW is an Electron app. Streaming is handled entirely inside the Chromium renderer process using the browser's built-in WebRTC stack. The main process (Node.js/Electron) owns session lifecycle, signaling transport, and local media I/O (screenshots, recordings). The renderer owns WebRTC peer connection negotiation, video/audio rendering, input capture, and microphone.

```
┌──────────────────────────────────────────────────────────────────────┐
│ Renderer (Chromium)                                                  │
│  GfnWebRtcClient                                                     │
│  ├── RTCPeerConnection (Chromium WebRTC)                             │
│  │    ├── video track → HTMLVideoElement (HW-decoded, low-latency)   │
│  │    ├── audio track → AudioContext → HTMLAudioElement              │
│  │    ├── mic track   ← getUserMedia (push-to-talk / VAD)            │
│  │    └── DataChannel ← InputEncoder (keyboard/mouse/gamepad)        │
│  ├── SDP manipulation (preferCodec, fixServerIp, mungeAnswerSdp)     │
│  └── NVST SDP builder (buildNvstSdp)                                 │
│                                                                      │
│  StreamView.tsx                                                      │
│  ├── <video> renders stream frames                                   │
│  ├── StatsOverlay (bitrate, RTT, codec, decode/render fps, jitter)   │
│  ├── Screenshot: canvas.drawImage(video) → dataURL → IPC             │
│  └── Recording: MediaRecorder(video.captureStream) → chunks → IPC    │
└────────────────────────┬─────────────────────────────────────────────┘
                         │ contextBridge (window.openNow)
┌────────────────────────▼─────────────────────────────────────────────┐
│ Main Process (Node.js/Electron)                                      │
│  ├── GfnSignalingClient  — WebSocket to NVST signaling server        │
│  │    └── forwards offer/ICE to renderer via IPC                     │
│  ├── CloudMatch API      — session create/poll/claim/stop            │
│  ├── AuthService         — OAuth tokens                              │
│  ├── Screenshots         — saved to ~/Pictures/OpenNOW/Screenshots/  │
│  ├── Recordings          — written to ~/Pictures/OpenNOW/Recordings/ │
│  │    └── ffmpeg (spawned) for thumbnail extraction                  │
│  └── Settings            — electron-store persistent settings        │
└──────────────────────────────────────────────────────────────────────┘
```

### 1.2 Session Lifecycle

1. **CloudMatch** — `POST /v2/session` creates a cloud gaming seat. Response includes `signalingServer`, `signalingUrl`, `iceServers[]`, `serverIp`, and `mediaConnectionInfo` (UDP endpoint).
2. **Signaling** — `GfnSignalingClient` opens a WebSocket with protocol header `x-nv-sessionid.{sessionId}`. Sends a `peer_info` init message. The server then sends an SDP offer.
3. **Offer handling** — `GfnWebRtcClient.handleOffer()`:
   - Fixes `0.0.0.0` placeholders in server SDP (`fixServerIp`).
   - Creates `RTCPeerConnection` with the GFN ICE servers.
   - Filters the SDP to the configured codec (`preferCodec`).
   - Sets remote description (server offer).
   - Creates local answer, waits for ICE gathering.
   - Injects `b=AS:` bitrate caps and `stereo=1` for Opus (`mungeAnswerSdp`).
   - Builds a parallel NVST custom SDP (`buildNvstSdp`) with stream quality params.
   - Sends both `sdp` + `nvstSdp` back to the server via signaling.
   - Injects a manual ICE candidate from `mediaConnectionInfo` (GFN servers use ICE-lite and do not trickle candidates).
4. **Tracks** — `ontrack` events attach video/audio tracks to separate `MediaStream` objects → bound to `<video>`/`<audio>` elements.
5. **Input** — A WebRTC `RTCDataChannel` (`application` m-line) carries binary-encoded keyboard, mouse, and gamepad packets via `InputEncoder`.
6. **Teardown** — `stopSession` → `disconnectSignaling` → `clientRef.dispose()`.

### 1.3 SDP / NVST Protocol Details

GFN uses two parallel SDP exchanges:

- **Standard WebRTC SDP** (RFC 8829) — used for actual RTP negotiation with Chromium's RTCPeerConnection. The app mutates it to prefer a single codec and inject bitrate hints.
- **NVST SDP** — a proprietary Nvidia extension sent alongside the WebRTC answer. Contains `a=vqos.*`, `a=video.*`, `a=bwe.*` attributes controlling encoder QoS, FEC rates, bitrate limits, resolution, framerate, and codec-specific tuning. This is the channel through which the client communicates stream quality preferences to the server-side encoder.

The NVST SDP defines four m-lines:

| m-line | Direction | Purpose |
|---|---|---|
| `m=video` | receive-only | Video RTP from cloud GPU |
| `m=audio` | receive-only | Opus stereo audio from game |
| `m=mic` | send-only | PCMU/8000 microphone to server |
| `m=application` | bidirectional | Input events DataChannel |

### 1.4 Codecs

| Codec | Bit depths | Chroma | Notes |
|---|---|---|---|
| H.264 | 8-bit | 4:2:0 | Widest HW support |
| H.265/HEVC | 8-bit, 10-bit | 4:2:0, 4:4:4 | Requires HEVC decoder; 10-bit = HDR |
| AV1 | 8-bit, 10-bit | 4:2:0 | dav1d software fallback if no HW |

### 1.5 Hardware Decode Paths (current, Chromium)

| Platform | Path | Flags |
|---|---|---|
| Windows | D3D11 / Media Foundation | `D3D11VideoDecoder`, `MediaFoundationD3D11VideoCapture` |
| Linux x64 | VA-API | `VaapiVideoDecoder`, `VaapiVideoEncoder`, `VaapiIgnoreDriverChecks` |
| Linux ARM | V4L2 | `UseChromeOSDirectVideoDecoder` |
| macOS | VideoToolbox | Native — no flags needed |
| All (AV1 fallback) | dav1d (SW) | `Dav1dVideoDecoder` |

### 1.6 Recording and Screenshots (current)

**Screenshots:**
- Renderer captures a frame by calling `canvas.drawImage(videoElement)` → `canvas.toDataURL()` → base64 string sent to main process via IPC.
- Main process decodes and writes PNG/JPEG/WebP to `~/Pictures/OpenNOW/Screenshots/`.
- Limit: 60 screenshots.

**Recordings:**
- Chromium feature flag `MediaRecorderEnableMp4Muxer` (Chromium 127+) enables MP4 output from `MediaRecorder`.
- Renderer calls `videoElement.captureStream()` to get a `MediaStream`, feeds it to `MediaRecorder`. Preferred MIME type is `video/mp4`; falls back to `video/webm`.
- `ondataavailable` chunks are sent to the main process via IPC (`recording:chunk`).
- Main process streams chunks to a `.tmp` file, renames to final on `recording:finish`.
- Thumbnail: `ffmpeg` is spawned (`child_process.spawn`) to extract a single frame at 1s.
- Limit: 20 recordings; `~/Pictures/OpenNOW/Recordings/`.

**Limitations of current recording:**
- `MediaRecorder` re-encodes the already-decoded video — decode → re-encode cycle wastes CPU and degrades quality.
- Output codec is whatever Chromium's MediaRecorder offers (typically H.264 baseline in MP4, or VP8/VP9 in WebM). The original HEVC/AV1 stream is transcoded.
- No control over output bitrate or format.
- Thumbnail generation requires `ffmpeg` to be on `PATH` (not bundled).

### 1.7 Key Source Files

| File | Role |
|---|---|
| `src/renderer/src/gfn/webrtcClient.ts` | Core WebRTC client, offer/answer, input, mic, stats |
| `src/renderer/src/gfn/sdp.ts` | SDP mutations and NVST SDP builder |
| `src/renderer/src/gfn/inputProtocol.ts` | Binary input encoding |
| `src/renderer/src/gfn/microphoneManager.ts` | Microphone capture |
| `src/renderer/src/components/StreamView.tsx` | `<video>` / `<audio>` rendering, recording UI, stats overlay |
| `src/renderer/src/App.tsx` | Session launch state machine, WebRTC client lifecycle |
| `src/main/gfn/signaling.ts` | NVST WebSocket signaling client |
| `src/main/gfn/cloudmatch.ts` | CloudMatch API: create/poll/claim/stop session |
| `src/main/index.ts` | IPC handlers, recording/screenshot file I/O, ffmpeg thumbnail |
| `src/shared/gfn.ts` | All shared types (SessionInfo, codecs, settings, recording API) |
| `src/shared/ipc.ts` | IPC channel name constants |
| `src/preload/index.ts` | `contextBridge` — `window.openNow` API surface |

---

## 2. Integration Points for the External Streamer

The external streamer (OPEN-23) needs to plug into the same session/signaling lifecycle but receive and decode the RTP stream natively instead of through Chromium.

### 2.1 What the Streamer Must Own

| Responsibility | Where today | Streamer must |
|---|---|---|
| CloudMatch session create/poll/claim | Main process (`cloudmatch.ts`) | Reuse existing; main process hands session info to streamer |
| NVST signaling | Main process (`signaling.ts`) | Reuse; main process proxies events OR streamer opens its own WebSocket |
| SDP offer/answer + NVST SDP | Renderer (`webrtcClient.ts`, `sdp.ts`) | Implement in streamer — no Chromium available |
| ICE + UDP RTP receive | Chromium WebRTC | Implement in streamer (native ICE, or manual UDP) |
| Video decode | Chromium (HW/SW) | Implement using FFmpeg or GStreamer HW decoders |
| Audio decode | Chromium AudioContext | FFmpeg or GStreamer Opus decode + audio output |
| Input encode + send | Chromium DataChannel + `InputEncoder` | Port `InputEncoder` binary protocol; use DataChannel |
| Microphone send | Chromium `getUserMedia` | Implement mic capture + PCMU/8000 RTP send |
| Window/display output | `HTMLVideoElement` | Platform window (SDL, DXGI, Metal, Wayland, etc.) |

### 2.2 Data the Main Process Must Provide to the Streamer

After `createSession`/`pollSession`/`claimSession` completes, the main process has a `SessionInfo` object. This must be passed to the streamer process:

```typescript
interface SessionInfo {
  sessionId: string;
  serverIp: string;         // server address (may be dash-hostname → must run extractPublicIp())
  signalingServer: string;  // host:port
  signalingUrl: string;     // WSS URL (may be RTSP/relative → run buildSignalingUrl())
  iceServers: IceServer[];  // STUN/TURN servers
  mediaConnectionInfo?: {   // direct UDP endpoint for manual ICE injection
    ip: string;
    port: number;
  };
}
```

### 2.3 SDP Logic the Streamer Must Replicate

The following logic from `sdp.ts` and `webrtcClient.ts` must be ported:

- `fixServerIp()` — replace `0.0.0.0` / dash-hostname in server SDP with real IP.
- `extractPublicIp()` — parse dash-separated hostnames like `80-250-97-40.cloudmatchbeta.nvidiagrid.net`.
- `preferCodec()` — strip all but the chosen codec from the video m-line, keeping RTX; reorder payload types.
- `rewriteH265TierFlag()` / `rewriteH265LevelIdByProfile()` — H.265 profile/level normalization.
- `mungeAnswerSdp()` — inject `b=AS:` bitrate hints and Opus `stereo=1`.
- `buildNvstSdp()` — construct the NVST custom SDP. All `a=vqos.*`, `a=video.*`, and `a=bwe.*` parameters must be sent correctly or server-side quality control will not work.
- Manual ICE candidate injection from `mediaConnectionInfo` (GFN servers are ICE-lite).
- `parsePartialReliableThresholdMs()` — parse `a=ri.partialReliableThresholdMs` from server SDP; controls DataChannel reliability settings.

### 2.4 Signaling Protocol

`GfnSignalingClient` (`src/main/gfn/signaling.ts`):
- WebSocket with `protocols: ["x-nv-sessionid." + sessionId]`
- Sends `peer_info` init message
- Receives: `offer` (SDP), `remote-ice` (ICE candidates)
- Sends: `answer` (SDP + nvstSdp), `ice` (local candidates)
- Heartbeat every 5 seconds

The streamer can either:
- **Re-use main process signaling** — main process opens the WebSocket, forwards the SDP offer to the streamer, collects the answer back, and sends it.
- **Own the WebSocket** — streamer opens its own connection to the signaling server with the same session credentials.

Option 1 (re-use main process) is lower risk and avoids duplicating protocol knowledge.

---

## 3. Constraints

1. **Proprietary NVST protocol.** Both the signaling WebSocket messages and the NVST SDP extension attributes are reverse-engineered from the official GFN client. Parameter names and values must match what the codebase already uses. Changes to NVST SDP parameters can cause degraded video quality or connection failures.

2. **ICE-lite servers.** GFN's media servers use ICE-lite. They do not trickle ICE candidates via signaling. The `mediaConnectionInfo` UDP endpoint must be injected as a manual host candidate. The current code tries `sdpMid` values `"0"` through `"3"` until one succeeds.

3. **HW decoder availability is platform-dependent.** H.265 HW decode is not universally available. On Linux without VA-API support (e.g., Nvidia proprietary driver), HW HEVC decode may not be available. On Windows, Media Foundation covers H.264/H.265/AV1. On macOS, VideoToolbox covers all three. The streamer should have a software fallback path.

4. **10-bit HDR.** The NVST SDP `a=video.bitDepth:10` path requires the decoder output format to match (P010 on Windows/Linux, `kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange` on macOS) and the display chain to support HDR output. This is advanced and can be deferred.

5. **AV1.** AV1 HW decode is not available everywhere. The current Chromium path uses `dav1d` software fallback. FFmpeg bundles `libaom`/`dav1d` on most platforms. GStreamer has `av1dec`.

6. **No bundled `ffmpeg` currently.** The current codebase spawns the system `ffmpeg` for thumbnails only. It is not bundled. The external streamer will need to decide whether to bundle ffmpeg/gstreamer or depend on system installations.

7. **Cross-process communication.** The streamer runs as a separate process. The current Electron IPC (contextBridge) is renderer-to-main only. A new IPC channel (Electron `ipcMain`/`ipcRenderer` or an OS pipe/socket) will be needed for the main process to control the streamer and for the streamer to report state back to the UI.

---

## 4. FFmpeg vs GStreamer Recommendation

### 4.1 Comparison

| Dimension | FFmpeg | GStreamer |
|---|---|---|
| **Cross-platform** | Excellent (Windows, macOS, Linux, ARM) | Excellent (same platforms) |
| **HW decode: H.264/H.265** | `h264_cuvid`, `hevc_cuvid` (Nvidia); `h264_qsv`/`hevc_qsv` (Intel); `h264_amf`/`hevc_amf` (AMD); `h264_videotoolbox`/`hevc_videotoolbox` (macOS); `h264_vaapi`/`hevc_vaapi` (Linux VA-API) | `nvh264dec`/`nvh265dec` (Nvidia); `qsvh264dec`/`qsvh265dec` (Intel); `vtdec` (macOS VideoToolbox); `vaapidecode` (Linux VA-API) |
| **AV1 HW decode** | `av1_cuvid` (Nvidia RTX30+); `av1_qsv` (Intel Arc+); `av1_d3d11va` / `av1_dxva2` (Windows); `av1_vaapi` (Linux) | `nvav1dec`; `vah264dec` (VA-API) |
| **WebRTC / RTP receive** | `avformat` has SRTP/RTP input; no built-in ICE — must handle ICE/DTLS externally | `webrtcbin` is a full WebRTC element including ICE/DTLS/SRTP. Closer to native WebRTC |
| **DTLS-SRTP** | `libsrtp2` integration for decryption; needs external DTLS | GStreamer `dtlssrtpdec` element handles full DTLS handshake |
| **Pipeline flexibility** | Command-line and `libavcodec`/`libavformat` API | Plugin pipeline — modular, composable, runtime re-routing |
| **Bundling / distribution** | FFmpeg static binaries are straightforward. `ffmpeg-static` npm package. | GStreamer has system packages but static bundling is complex |
| **Existing use in codebase** | Already used for thumbnail extraction (`spawn("ffmpeg", ...)`) | Not used |
| **Latency** | Configurable; low-latency decode with `-flags low_delay` | `webrtcbin` + `queuedepth=0` tuning; comparable |
| **Complexity** | Simpler C API for decode-only path; complex for full WebRTC pipeline | `webrtcbin` handles WebRTC natively but requires GLib main loop |

### 4.2 Recommendation

**Use FFmpeg for the initial streamer.**

Rationale:
- FFmpeg is already invoked from the main process (thumbnail extraction). Familiarity is established.
- The signaling and ICE negotiation can be owned by the main process (reusing existing TypeScript code). The streamer only needs to receive and decode the negotiated SRTP media streams and present output; this keeps the first version smaller than a full WebRTC reimplementation.
- FFmpeg static binaries are available for all target platforms and can be bundled with the Electron app, eliminating a system dependency.
- The HW decode coverage across Windows (D3D11/DXVA2/Media Foundation), macOS (VideoToolbox), and Linux (VA-API/NVDEC) via FFmpeg is comprehensive and well-maintained.
- GStreamer's `webrtcbin` is powerful but introduces significant complexity (GLib event loop, plugin discovery, GObject-style APIs). The NVST-specific ICE quirks (ICE-lite, manual candidate injection) would make the first integration steeper.

**GStreamer should be reconsidered if:**
- A full WebRTC re-implementation (ICE, DTLS, SRTP all in GStreamer) is required.
- Live streaming to services like Twitch/YouTube from the streamer is a goal (GStreamer's `rtmpsink`/`rtspclientsink` are better supported).
- The team has existing GStreamer expertise.

### 4.3 Platform-Specific FFmpeg Decoder Strategy

| Platform | H.264 | H.265 | AV1 |
|---|---|---|---|
| Windows | `h264_d3d11va` (D3D11) | `hevc_d3d11va` | `av1_d3d11va`; fallback `libaom-av1` |
| macOS | `h264_videotoolbox` | `hevc_videotoolbox` | SW only (`libaom-av1` / `dav1d`) |
| Linux (Nvidia) | `h264_cuvid` or `h264_vaapi` | `hevc_cuvid` or `hevc_vaapi` | `av1_cuvid` (RTX30+) |
| Linux (Intel/AMD) | `h264_vaapi` | `hevc_vaapi` | `av1_vaapi` (Intel Arc / AMD RDNA3) |
| Linux (SW fallback) | `h264` | `hevc` | `dav1d` |

Each platform path should be probed at startup and fall back gracefully. The decoder should be configurable or auto-detected.

---

## 5. Recommended Architecture for the External Streamer

```
OpenNOW Main Process (Electron)
│
│  1. createSession / pollSession / claimSession → SessionInfo
│  2. connectSignaling(sessionInfo) → receives SDP offer
│  3. Forward offer to streamer over pipe/socket
│  4. Receive NVST SDP + WebRTC answer from streamer
│  5. sendAnswer() to signaling server
│  6. Forward ICE candidates bidirectionally
│
│ ◄──── stdin/stdout pipe or Unix socket / named pipe ────►
│
OpenNOW Streamer (external process, bundled)
│
├── SDP handling (port sdp.ts logic: fixServerIp, preferCodec, buildNvstSdp, etc.)
├── ICE / transport integration needed for the negotiated media path
├── DTLS-SRTP handling for negotiated media streams
├── RTP demux → video RTP stream, audio RTP stream
├── FFmpeg decode pipeline:
│    ├── video: RTP → H264/H265/AV1 → HW decoder → frame output
│    └── audio: RTP → Opus → decode → PCM → audio output
├── Input: DataChannel → receive keyboard/mouse/gamepad events from Electron renderer
│         OR: renderer sends input via main-process pipe to streamer DataChannel
├── Display output: SDL2 / platform-native (Metal, DXGI, Wayland/X11)
└── State reporting → IPC back to main process (connection state, stats, errors)
```

### 5.1 Phased Approach

**Phase 1 (MVP):**
- Main process handles signaling and session lifecycle (reuse existing TypeScript code).
- Streamer receives negotiated session details and is responsible for the media path it consumes.
- FFmpeg decodes H.264 first; HEVC/AV1 optional.
- SDL2 for display output (cross-platform, simple).

**Phase 2:**
- Streamer owns the DataChannel for input events.
- Microphone capture + PCMU encoding + RTP send.
- HEVC and AV1 HW decode paths per platform.

**Phase 3:**
- More protocol/media ownership in the streamer as needed.
- HDR/10-bit output.
- Native window integration (Metal on macOS, DXGI on Windows, Wayland/DRM on Linux).

---

## 6. Open Questions

1. **What language/runtime for the streamer?** C/C++ gives the lowest-latency FFmpeg integration. Rust has `rust-ffmpeg` bindings and better safety. Go is an option if simplicity is prioritized over raw performance.
2. **Bundling strategy.** Will FFmpeg be bundled as a static binary alongside the Electron app, or required as a system install? Bundling adds size per platform but eliminates a user dependency.
3. **IPC protocol between Electron and the streamer.** JSON over stdin/stdout is simple; a local TCP/Unix socket is more robust for bidirectional streaming of input events and stats.
4. **Screencasting / recording from the streamer.** If the external streamer decodes to a framebuffer, recording can be done without the current `MediaRecorder` re-encode penalty.
5. **Controller-mode / overlay rendering.** Currently done in the Electron renderer overlaid on the `<video>` element. With an external streamer window, the overlay must be composited differently (transparent overlay window or embedded OSD).
