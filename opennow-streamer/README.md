# opennow-streamer

`opennow-streamer` is the native external play-surface for OpenNOW.

Why Rust:
- long-term maintainability with memory safety around RTP, input, windowing, and control IPC
- cross-platform native binary distribution without tying the streamer to Chromium
- async/runtime ecosystem that fits Electron main ↔ native control and WebRTC session ownership
- clear path to future decode/backend specialization without rewriting the app shell

Why loopback socket IPC instead of stdio:
- framed full-duplex JSON works consistently across Windows, macOS, and Linux
- avoids stdio backpressure issues when the child is also producing logs
- gives Electron main a robust reconnect/health boundary for a long-lived streamer process
- leaves room for future stats/debug/control channels without redesigning the process contract

Current MVP responsibilities implemented here:
- native control bridge to Electron main
- native WebRTC peer connection, offer handling, answer generation, and ICE forwarding
- GFN-specific SDP handling for server IP fixing, codec filtering, answer munging, NVST SDP generation, native-bundle V2 fingerprint fields, and manual media endpoint ICE injection
- native SDL2 play surface for decoded video
- native SDL2 audio output for decoded Opus audio
- native keyboard / mouse / controller capture using the existing GFN input packet semantics, including protocol-version handshake and gamepad PR framing

Current media implementation:
- video RTP is depacketized in-process and decoded through an FFmpeg child pipeline into RGB frames rendered in the SDL window
- audio RTP is depacketized in-process and decoded with libopus, then queued to SDL audio output
- the MVP decode path currently targets the practical GFN desktop path first: H.264 and H.265 video plus Opus audio

Recent native-parity adjustments from the official GFN desktop client:
- dedicated `gamepad_channel_v1` plus `input_channel_v1` / `input_channel_partially_reliable`
- protocol-version detection from the input-channel handshake and native heartbeats on the reliable channel
- gamepad packet bitmap / controller-id / sequence framing aligned with the browser/native protocol observations
- NVST attributes for `general.dtlsFingerprintV2`, `general.icePasswordV2`, `general.iceUserNameFragmentV2`, `general.rtcDataChannelOnNativeBundle`, and related native-bundle flags

Still intentionally out of scope for this phase:
- recording / screenshots migration
- microphone uplink migration
- AV1 native decode path
- HDR / 10-bit output polishing and platform-specific hardware decode optimization

Project layout:
- `src/control.rs` — Electron/native control socket protocol
- `src/messages.rs` — typed control/state messages
- `src/sdp.rs` — GFN SDP and NVST helpers
- `src/session.rs` — peer connection + signaling/media orchestration
- `src/media.rs` — RTP depacketize + decode pipeline
- `src/input.rs` — GFN-compatible input packet encoding
- `src/window.rs` — SDL window, rendering, audio, and native input capture
- `src/main.rs` — process bootstrap

Packaging/runtime model:
- packaged OpenNOW builds copy `opennow-streamer` and a colocated `ffmpeg` sidecar into `resources/bin/` via `opennow-stable/scripts/bundle-native-runtime.mjs`
- `opennow-stable` build now runs `cargo build --release` for the native streamer and bundles both binaries into Electron extra resources
- at runtime the streamer resolves `ffmpeg` relative to its own executable first, then `resources/bin`, then `OPENNOW_FFMPEG_BIN` for development overrides
