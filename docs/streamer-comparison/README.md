# OpenNOW native streamer vs official GeForce NOW

This set explains how the two Windows streamers move video, audio, mouse, and controller data. It is written for someone who already ships OpenNOW and wants to know why a session feels different from NVIDIA's client.

Official evidence is from the GeForce NOW install on this machine, build 2.0.87.131, Geronimo branch `gs_04_90`, dated 2026-08-30. OpenNOW evidence is the current `native/opennow-streamer` tree plus `native/opennow-core`.

## Who this is for

If you play on OpenNOW, the mouse section is the one that changes what you feel. If you maintain decode, recovery, or CloudMatch, start with video and the settings path.

The next engineer who owns this code inherits a map of what OpenNOW actually sends, what official GFN logs say they send, and which gaps are product choices versus missing wire.

## How to read

Each category file is one explanation. They do not mix how-to steps with reference tables beyond what you need to follow the flow.

- [Video](video.md). Decode, present, bitrate, color, and recovery.
- [Audio](audio.md). Opus, WASAPI, RED, jitter, microphone.
- [Mouse input](mouse-input.md). Why look and aim feel different.
- [Controller input](controller-input.md). SDL pads, the 38-byte type-12 packet, rumble.
- [Features and binaries](features.md). NVB types and byte-exact control frames. Type 10 is named in logs. The payload is not dumped.
- [HTML comparison](compare.html). Same facts, laid out for scanning.

## The short answer on mouse

Official GFN captures with SDL plus a focus-gated Raw Input async thread. On every focus change it tells the host `accel=0, speed=10` (NVB feature type 10). At session start it also logs `Disabling session mouse acceleration`. Cursor lock is server-driven. System cursor ID 0 hides and locks. Other IDs warp the local cursor from 16-bit normalized coordinates.

OpenNOW also uses a dedicated Raw Input thread plus SDL grab. Official logs `accel=0, speed=10` (NVB feature type 10) on every focus change. Those logs do not include the on-wire bytes. OpenNOW can still scale type-7 deltas on the client with `tune_relative_mouse`. Defaults are identity.

## What each client is

**Official GFN.** Geronimo plus Bifrost plus NVST stream SDK. Chrome 128 shell. Video on a dedicated Mjolnir UDP socket. Audio, input, and RTCP on an ICE/DTLS/SCTP bundle. Ports 49005 and 49006 on the session reconstructed from logs. Decode is DX11 DXVA on a dedicated D3D11 device. Present is a tearing flip swapchain. Audio is WASAPI 48 kHz stereo through `TimestampAudioBuffer`.

**OpenNOW native.** A Rust process launched by the Qt app. It speaks the same two-socket NVST shape and does not load NVIDIA client libraries. `transportMode` is `nvst`.

## Shared shape

Both clients run OPTIONS, DESCRIBE, SETUP, ANNOUNCE, then optional PLAY over WebSocket-carried RTSP. Video is raw SRTP on Mjolnir. Audio and input ride the bundle. NACK envelope numbers match. 1024-packet wait queue, 2048 pending NACKs, 3 retries, 1280-byte packets.

The differences start after that shared envelope. Color, recovery extras, audio jitter, host mouse settings, and what CloudMatch is allowed to request.

## Evidence

OpenNOW paths you can open now:

- `native/opennow-streamer/crates/opennow-streamer-core/src/lib.rs`
- `native/opennow-streamer/crates/opennow-streamer-core/src/nvst_rtsp.rs`
- `native/opennow-streamer/crates/opennow-streamer-transport/src/nvst.rs`
- `native/opennow-streamer/crates/opennow-streamer-transport/src/nvst_input.rs`
- `native/opennow-streamer/crates/opennow-streamer-platform/src/windows_raw_input.rs`
- `native/opennow-streamer/crates/opennow-streamer-platform/src/output.rs`
- `native/opennow-core/src/cloudmatch.rs`
- `native/opennow-core/src/streamer.rs`

Official GFN local files used:

- `C:\Users\Zortos\AppData\Local\NVIDIA Corporation\GeForceNOW\geronimo.log`
- `C:\Users\Zortos\AppData\Local\NVIDIA Corporation\GeForceNOW\debug.log`
- `C:\Users\Zortos\AppData\Local\NVIDIA Corporation\GeForceNOW\console.log`
- `C:\Users\Zortos\AppData\Local\NVIDIA Corporation\GeForceNOW\sharedstorage.json`
- `C:\Users\Zortos\AppData\Local\NVIDIA Corporation\GeForceNOW\Mall\shared\assets\config\config.json`

No official binary was disassembled. Official behavior is what those logs and JSON files print.

## Limits

A later GFN build can change command IDs and still log the same English strings. Treat log component names as the source of truth for this install, not as a frozen protocol spec.

OpenNOW Linux and macOS are noted only where they change the Windows story.
