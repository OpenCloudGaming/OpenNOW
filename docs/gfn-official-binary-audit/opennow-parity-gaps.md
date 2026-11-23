# OpenNOW parity candidates (Linux `gs_04_87` vs recorded source)

Consolidated from the section audits at the exact OpenNOW comparison commit in [README.md](README.md). Existing source contracts include the classic RTSP flow, Mjolnir transport, tested NACK-v2 encoder, route-specific SCTP profile, activation commands, and RED recovery. These are not proof of live vendor compatibility. The candidates below are grouped by owner, not ranked by measured quality or treated as ready implementation specifications.

## Input, presentation, and server policy

| Gap | Official behavior | OpenNOW today | Primary owner |
| --- | --- | --- | --- |
| **Mouse host settings (NVB feature 10)** | `sendMouseSettings` → `nvbFeatureControl`; logs `accel=0, speed=10` on focus | Client-side `tune_relative_mouse` only; no type-10 blob on wire | `opennow-streamer` control + activation |
| **Mid-session feature control** | `nvbFeatureControl` types `0x0F` (DRC/DFC), `0x10` (max bitrate), `0x13` (L4S) | Values fixed at ANNOUNCE; core rejects mid-session bitrate IPC | `nvst_control` / core streamer RPC |
| **Server DJB (de-jitter buffer)** | Forced µs queue-bound overrides, reporting API, and separate local-adaptation evidence | Embedded GPU publication into Qt; no equivalent vendor DJB API established | Embedded media + Qt scene graph; recover wire/limits before implementation |
| **Reference invalidation + display freeze** | Names in prior Windows logs; Linux state/wire contract untraced | Control IDR on Mjolnir, optional bundle-video PLI; no invalidation encoder | `nvst_control.rs`, after wire/state evidence |
| **Decoder state → server** | `nvbUpdateVideoDecoderState` API; causal IDR/invalidation sequence not established | Decoder-specific local recovery and route-specific keyframe requests | Streamer decoder/transport owners |

## Session orchestration

| Gap | Official | OpenNOW | Primary owner |
| --- | --- | --- | --- |
| **Auth refresh during POST/poll** | `NVB_EVT_UPDATE_AUTH_TOKEN` blocks until mall refreshes JWT | Token captured once at create | `opennow-core` CloudMatch |
| **Setup progress vocabulary** | `NVB_SSS_*`, `seatSetupEta` ms | `phase`, `queuePosition`, `seatSetupStep` only | core session normalization + Qt UI |
| **Poll failure → DELETE** | Bifrost DELETE on poll network error | Poll continues; separate cancel path | CloudMatch poller policy |
| **Join / transfer / rating** | `nvbJoinSession`, FORWARD, TRANSFER, SESSION_RATING | RESUME claim only | core protocol |
| **Network test + LBR** | `nvbTestNetwork*`, `QUERY_GFN_LATENCY_BASED_ROUTING`, cached latency | Optional `networkTestSessionId`; separate zone selector | core + QML |
| **Remote config override of session params** | `GeronimoSettingsImpl::overrideNVbSessionParams` after mall fill | Settings resolved pre-POST; server `finalizedStreamingFeatures` overlay | core + streamer ANNOUNCE |
| **Stream session IDs in normalized seat** | Requires `networkSessionId`, `rtspSessionId`, `streamSessionId`, `streamSubSessionIds` | Raw `connectionInfo` + derived URLs | `session_info` normalization |

## Media and devices

| Gap | Official | OpenNOW | Primary owner |
| --- | --- | --- | --- |
| **Audio TimestampAudioBuffer** | Adaptive threshold, stale drops, overbuffer flush | RED + PLC; no equivalent vendor adaptive policy established | Existing platform audio owner |
| **Microphone AEC / redundancy** | GsAudioWebRTC reverse-stream AEC; prior Windows mic RED level 3 | Negotiated SDL capture, mono 48 kHz Opus at 32 kbps, bounded uplink; no reverse-stream AEC or mic RED | Existing native microphone owners; target acceptance still required |
| **Gamepad aggregation** | Timer + destructive aggregation settings | Event-driven + 100 ms keepalive | input queue policy |
| **GSHID / DS4 synth** | `GamepadHIDSynthesizer`, cross-synth DS4/DS5 | DS4 report commands exist; no generic→Sony synth | `nvst_input` / HID |
| **HUD second decoder set** | `HudVideoDecoderSet` | One embedded stream; no vendor HUD decoder-set equivalent established | Streamer, only if product scope requires HUD video |
| **Serenity H.264 local record** | CEF transcode path when live codec not recordable | Matroska of negotiated stream | out of scope unless product asks |

## Platform and packaging

| Gap | Official Linux | OpenNOW Linux | Primary owner |
| --- | --- | --- | --- |
| **Pointer capture** | XInput2 raw, XWayland `XGrabPointer`, SDL fallback | XI2 + Qt/Wayland path in app | `opennow-qt` input |
| **Gamescope HDR env** | Flatpak sets `ENABLE_GAMESCOPE=1`, `ENABLE_GAMESCOPE_HDR=1`, and `GAMESCOPE_HDR=1` | Compositor HDR from Qt output | packaging + settings |
| **Default audio device churn** | Explicitly disabled on Linux client | WASAPI-style replug on Windows only | platform audio |

## Version skew note

The supplied payload identifies itself as mall **2.0.84.127** / **gs_04_87**; its publisher provenance was not authenticated. The compared OpenNOW source advertises newer client identity values, and the earlier Windows logs use **`gs_04_90`**. Record both source and vendor build when testing. Command IDs and log strings can change between branches without changing the English message.

## Evidence needed before follow-up implementation

1. Recover the **feature-type-10** wire payload and compare controlled sessions before assigning an aim-feel cause or adding a focus/activation write.
2. Trace **DJB** force/local-adaptation behavior and its wire representation. Assess bounded queue/deadline policy through the embedded GPU publisher and Qt, not the standalone frame pacer.
3. Trace the **decoder-state/invalidation** call path or paired wire exchange before choosing a new recovery command. Keep route-specific IDR and PLI distinctions.
4. Verify negotiated **live feature writes** for `0x0F`, `0x10`, and `0x13`, including failures, before exposing new mid-session settings actions.
5. Verify the **auth-refresh** retry contract during POST/poll before changing account/session orchestration.
6. Validate the existing **microphone uplink** on a target device and live seat. Assess reverse-stream AEC and mic redundancy as separate additions, not a missing capture path.

Cross-links: [session-creation-cloudmatch.md](session-creation-cloudmatch.md), [video-streaming-decode-recovery.md](video-streaming-decode-recovery.md), [docs/streamer-comparison/README.md](../streamer-comparison/README.md).
