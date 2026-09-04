# Video

## Overview

Both clients decode HEVC or H.264 on Windows with D3D11 and present with a tearing-allowed flip swapchain. Official GFN on this machine ran 10-bit HEVC at 2560×1440@120 for about 76 minutes. OpenNOW's native CloudMatch path forces 8-bit 4:2:0 before the seat is created, then presents 8-bit BGRA even if a 10-bit bitstream arrives.

Recovery is the other split. Official GFN NACKs, then flushes a stalled decode queue, then sends IDR, then invalidates references and can freeze the display on a bad ref. OpenNOW NACKs, then sends RTCP PLI plus control `0x302`. It has no reference-invalidation command.

If you stream on OpenNOW today, you get the official two-socket video path with a narrower color SKU and a simpler recovery ladder.

## Key concepts

**Mjolnir.** The dedicated UDP socket that carries video SRTP. Official and OpenNOW both prefer client port 49005. Video does not ride the ICE bundle. ANNOUNCE sets `rtcVideoOnNativeBundle:0`.

**GS extension `0x4753`.** Each video RTP packet carries a 16-byte GameStream header with SOF, EOF, frame index, and FEC coordinates. OpenNOW parses this in `NvstVideoReceiver` (`nvst.rs`).

**ANNOUNCE vs `MediaStreamConfig`.** CloudMatch and RTSP tell the server what to encode. `media_stream_config` tells the local decoder what to expect. Those two defaults disagree in OpenNOW. ANNOUNCE uses 75 Mbps when `maxBitrateMbps` is missing. Decode config uses 10 Mbps.

**Class-preserving fallback.** After a fatal D3D device loss, H.264 can leave Media Foundation for bundled OpenH264 plus SDL. HEVC and AV1 stay on Media Foundation and switch to a software MFT.

## How settings reach the stream

The user changes `resolution`, `fps`, `codec`, `maxBitrateMbps`, and `colorQuality` in Qt. Those values land in `%APPDATA%/OpenNOW/settings.json`. Launch does not resend the profile. `session.create` re-reads the store.

`cloudmatch.rs` `build_create_body` maps the store onto NVIDIA `sessionRequestData`.

On the native path it then overwrites color:

```
if native {
    bit_depth = 0;
    chroma = 0;
}
```

`trueHdr` is forced false. `maxBitrateKbps` is omitted from the feature bag. Bitrate still reaches the streamer later through ANNOUNCE `x-nv-vqos` and `MediaStreamConfig`.

`codec: "auto"` on native requests H.264 on purpose so CloudMatch cannot pick AV1 or HEVC before local decode is known. Official Auto is wire `0` and this machine still selected HEVC.

Desktop AUTO FPS writes `0`. Core clamps fps to 30–240 with fallback 60, so AUTO is 60.

`OPENNOW_NATIVE_VIDEO_BACKEND` comes from `nativeVideoBackend` and `decoderPreference`. Auto prefers D3D12 for H.264 and H.265 when the probe succeeds. AV1 auto stays on D3D11 because D3D11-on-12 would flush every frame.

Official persist on this machine (`sharedstorage.json` `customProfile`):

- 2560×1440@120
- `maxBitrate: 100` applied as `maxBitrate: 100000` kbps
- `codec: 0` then runtime HEVC
- `bitDepth: 10`, live chroma 3
- `hdrStreamingMode: "OFF"`
- `vSync: 0`, `reflex: true`, `cloudGsync: false`
- `dynamicStreamingMode: 0`

Official also has named profiles, AI prefilter, HUD sharpness, and upscaling. OpenNOW does not send those on native create.

## How OpenNOW plays video

1. Prefer NVST. Reserve Mjolnir then bundle+1. The child owns RTSPS.
2. ANNOUNCE viewport, fps, bitrate, NACK, FEC 20–35%, packet size 1280.
3. `NvstVideoReceiver` unprotects SRTP, reorders up to 1024 packets, waits up to 150 ms for a gap, NACKs, assembles access units from the GS header.
4. Keyframe rules. H.264 NAL type 5. H.265 IRAP 16–21. AV1 SOF header byte `[3]==2`.
5. `MediaSink` encoded video queue capacity 2. Overflow marks desync and requests a keyframe. Windows MF overflow uses `push_or_clear_on_overflow`, which dumps the whole reference chain.
6. Media Foundation decode. Hardware MFTs first, then the Microsoft D3D11-aware MFT. NVIDIA and AMD often only expose that Microsoft DXVA2 MFT. `MF_LOW_LATENCY=1`. Output NV12 or P010.
7. Present. Flip-discard, 2 buffers, waitable latency object, max latency 1, `DXGI_PRESENT_ALLOW_TEARING` when available. Not vsync-locked. Letterbox with `fit_rect`.
8. `PresentationClock` ignores GFN RTP timestamps. H.265 and AV1 group timestamps, so the clock learns arrival cadence instead.

Live GFN video is Mjolnir.

Mid-session IPC `bitrate` is rejected. `WindowsBackend::reconfigure_video` has no in-tree caller. Live size changes come from `MF_E_TRANSFORM_STREAM_CHANGE` only.

## How official GFN plays video

From `geronimo.log` and `debug.log` on 2026-08-30:

1. Capability probe. HEVC HDR and YUV444 advertised up to 7680×4320. Rule logged: select HEVC for 240 FPS or higher.
2. `ConfigureStreamerVideoSettings` for this session. 2560×1440, 120 FPS, `maxBitrate: 100000`, HDR off, Reflex on, Cloud G-Sync off, 10-bit, colorSpace 2, chroma 3, codec 0 then HEVC.
3. `Creating DX11 decoder with Async for H265` on a dedicated decode D3D11 device. Present is a second device. Algemist continues with different decode and render devices.
4. `ulNumDecodeSurfaces = 10`. Frame pacing strategy `frl`.
5. First frame. Stream bit depth 10, chroma 3, colorspace 2, limited range, HDR 0. Swapchain 2560×1440, vsync 0, 10-bit, tearing allowed.
6. Two sockets. Video RTP thread on 49005, WebRTC audio bind on 49006, server 5004.
7. NACK wait 52 ms, 1024 packets, 2048 pending, 3 retries, 4 ms backoff. Dynamic FEC on. This session recovered 0 FEC packets and used 639 of 677 NACKed packets.

UI window starts with vsync on. Streaming immediately sets vsync 0 and `DXGI_PRESENT_ALLOW_TEARING`. The session logged 438096 missed vsyncs. That count matches vsync-off present, not 438k stalls.

## Recovery

### Official GFN, this session

1. NACK first. Same envelope OpenNOW advertises.
2. Decode queue overflow, cap 30 then 40. Complete flush, skip remaining frames, `Sent IDR request for stream 0`.
3. QoS drops a P-frame. `Sent invalidation request` for that frame range.
4. Next I-frame clears invalidations. `Stream[0]: recovered`.
5. Client asked the server to freeze display on invalidated reference frames.
6. Session totals. 5 complete flushes, 195 flushed frames, 117 decoder drops. No `PLI` or `FIR` strings. No device-lost path.

Intra-refresh is advertised supported and then `enableIntraRefresh: 0`. Recovery is IDR plus invalidation.

### OpenNOW

| Trigger | What happens |
|---|---|
| RTP gap inside the NACK window | RTCP generic NACK on SCTP `rtcp1`, every 10 ms poll. Max 64 packets, 3 attempts, 20 ms retry, 150 ms track. |
| Unrecoverable gap or bad GS header | Assembler reset. `contiguous=false`. RTCP PLI and NVST control `0x302` IDR. 250 ms cooldown. Inter-frames dropped until a keyframe. That keyframe rebuilds the MFT. |
| Encoded overflow | Clear the reference chain. Request keyframe. |
| Decoder submit or poll error | Rebuild MFT on the same device. If that fails, recreate Graphics plus Decoder for up to 5 s. |
| Device recreate timeout | Fatal. Hardware then falls back, class-preserving for HEVC and AV1. |
| Packet-gap keyframe | Does not spend `NVST_RECOVERY_ATTEMPT_LIMIT`. |
| Other NVST recovery | One `recover()`, then terminal. |

NVST does not send FIR. OpenNOW has no invalidation command and no display-freeze-on-bad-ref.

`reset_decoder` after loss is required. `MFSampleExtension_CleanPoint` alone does not forget damaged H.264 references. See the comment in `platform-windows/src/format.rs`.

## Where things live

- Session mapping. `native/opennow-streamer/crates/opennow-streamer-core/src/lib.rs` `media_stream_config`
- ANNOUNCE. `native/opennow-streamer/crates/opennow-streamer-core/src/nvst_rtsp.rs` `build_announce`
- Receive, NACK, FEC, AU assemble. `native/opennow-streamer/crates/opennow-streamer-transport/src/nvst.rs`
- IDR, ack, pacing, QoS. `native/opennow-streamer/crates/opennow-streamer-transport/src/nvst_control.rs`
- Windows decode. `native/opennow-streamer/crates/opennow-streamer-platform-windows/src/windows/decoder.rs`
- Present. `.../windows/graphics.rs`
- Clock and worker. `.../windows/mod.rs`
- CloudMatch color force. `native/opennow-core/src/cloudmatch.rs` around the native `bit_depth = 0` block
- Backend env. `native/opennow-core/src/streamer.rs`

## Gotchas

Two bitrate defaults. Missing `maxBitrateMbps` becomes 75 Mbps on the wire and 10 Mbps in the local media config.

Overlay `8-bit` is a label. A P010 decode still presents as BGRA and the HUD still prints 8-bit.

Hardware MFT enum is often empty. The Microsoft D3D11-aware MFT is the real GPU path.

Stats LATENCY, PING, and LOSS% are placeholders. NVST has no trustworthy RTT in this tree. LOSS shows dropped-frame count only.
