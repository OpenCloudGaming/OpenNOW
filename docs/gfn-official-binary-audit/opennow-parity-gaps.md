# OpenNOW parity gaps (official Linux `gs_04_87` vs current tree)

Consolidated from the section audits. **Already aligned** items (RTSP shape, Mjolnir socket, NACK-v2 `0x0317`, eight-channel profile subset, activation chain through window state 19, RED parse/recover on audio, type 7/12 wire layouts) are omitted here; see [transport-rtsp-mjolnir.md](transport-rtsp-mjolnir.md) and [input-mouse-gamepad-features.md](input-mouse-gamepad-features.md).

## P0 — Session feel and server policy

| Gap | Official behavior | OpenNOW today | Primary owner |
| --- | --- | --- | --- |
| **Mouse host settings (NVB feature 10)** | `sendMouseSettings` → `nvbFeatureControl`; logs `accel=0, speed=10` on focus | Client-side `tune_relative_mouse` only; no type-10 blob on wire | `opennow-streamer` control + activation |
| **Mid-session feature control** | `nvbFeatureControl` types `0x0F` (DRC/DFC), `0x10` (max bitrate), `0x13` (L4S) | Values fixed at ANNOUNCE; core rejects mid-session bitrate IPC | `nvst_control` / core streamer RPC |
| **Server DJB (de-jitter buffer)** | `nvbUpdateDJBState`; min/max depth in µs; QOS/MODE + FIXED/VVSYNC reasons | `LinuxFramePacer` local depth ≤2; no `jbConfig` ANNOUNCE | `linux_frame_pacing.rs` |
| **Reference invalidation + display freeze** | After NACK failure: IDR + invalidation commands (session logs) | PLI + `0x0302` only; no invalidation | `nvst_control.rs` |
| **Decoder state → server** | `nvbUpdateVideoDecoderState` provokes official IDR/invalidation ladder | Local transform rebuild + PLI | streamer transport + control |

## P1 — Session orchestration

| Gap | Official | OpenNOW |
| --- | --- | --- |
| **Auth refresh during POST/poll** | `NVB_EVT_UPDATE_AUTH_TOKEN` blocks until mall refreshes JWT | Token captured once at create | `opennow-core` CloudMatch |
| **Setup progress vocabulary** | `NVB_SSS_*`, `seatSetupEta` ms | `phase`, `queuePosition`, `seatSetupStep` only | core session normalization + Qt UI |
| **Poll failure → DELETE** | Bifrost DELETE on poll network error | Poll continues; separate cancel path | CloudMatch poller policy |
| **Join / transfer / rating** | `nvbJoinSession`, FORWARD, TRANSFER, SESSION_RATING | RESUME claim only | core protocol |
| **Network test + LBR** | `nvbTestNetwork*`, `QUERY_GFN_LATENCY_BASED_ROUTING`, cached latency | Optional `networkTestSessionId`; separate zone selector | core + QML |
| **Remote config override of session params** | `GeronimoSettingsImpl::overrideNVbSessionParams` after mall fill | Settings resolved pre-POST; server `finalizedStreamingFeatures` overlay | core + streamer ANNOUNCE |
| **Stream session IDs in normalized seat** | Requires `networkSessionId`, `rtspSessionId`, `streamSessionId`, `streamSubSessionIds` | Raw `connectionInfo` + derived URLs | `session_info` normalization |

## P2 — Media and devices

| Gap | Official | OpenNOW |
| --- | --- | --- |
| **Audio TimestampAudioBuffer** | Adaptive threshold, stale drops, overbuffer flush | RED + PLC; no JB stats | platform audio / future Bifrost parity layer |
| **Microphone upstream** | GsAudioWebRTC AEC + `nvbSendMicAudioFrame` + Opus + mic RED 3 | ANNOUNCE mic bundle possible; no capture path | streamer + platform |
| **Gamepad aggregation** | Timer + destructive aggregation settings | Event-driven + 100 ms keepalive | input queue policy |
| **GSHID / DS4 synth** | `GamepadHIDSynthesizer`, cross-synth DS4/DS5 | DS4 report commands exist; no generic→Sony synth | `nvst_input` / HID |
| **HUD second decoder set** | `HudVideoDecoderSet` | Single queue | streamer (if product needs HUD video) |
| **Serenity H.264 local record** | CEF transcode path when live codec not recordable | Matroska of negotiated stream | out of scope unless product asks |

## P3 — Platform / packaging

| Gap | Official Linux | OpenNOW Linux |
| --- | --- | --- |
| **Pointer capture** | XInput2 raw, XWayland `XGrabPointer`, SDL fallback | XI2 + Qt/Wayland path in app | `opennow-qt` input |
| **Gamescope HDR env** | Flatpak sets `ENABLE_GAMESCOPE_HDR` | Compositor HDR from Qt output | packaging + settings |
| **Default audio device churn** | Explicitly disabled on Linux client | WASAPI-style replug on Windows only | platform audio |

## Version skew note

This audit binary is mall **2.0.84.127** / **gs_04_87**. OpenNOW CloudMatch headers often impersonate **2.0.87.131** / Windows **`gs_04_90`**. Parity tests should record which build is under test; command IDs and log strings can move between branches without renaming the English message.

## Suggested closure order

1. Capture **feature type 10** on wire (pcap or instrumented Bifrost) → implement in activation or post-focus path.
2. Implement **DJB receive/send** paired with **`nvbUpdateDJBState`-equivalent** control messages.
3. Add **invalidation** command alongside IDR after unrecoverable video gap.
4. Expose **mid-session** `0x0F` / `0x10` / `0x13` feature writes from settings changes.
5. **Auth refresh callback** during long poll/setup.
6. **Microphone** path end-to-end once product enables it.

Cross-links: [session-creation-cloudmatch.md](session-creation-cloudmatch.md), [video-streaming-decode-recovery.md](video-streaming-decode-recovery.md), [docs/streamer-comparison/README.md](../streamer-comparison/README.md).
