# GeForce NOW Reverse Engineering Documentation

**Last Updated:** 2026-01-01
**Sources Analyzed:**
- Official Web Client: `C:\Users\Zortos\CustomGFNClient\research`
- Official GFN Client: `C:\Users\Zortos\AppData\Local\NVIDIA Corporation\GeForceNOW`
- OpenNow Implementation: `C:\Users\Zortos\CustomGFNClient\gfn-client\opennow-streamer`

---

## Overview

This documentation provides comprehensive reverse engineering analysis of NVIDIA GeForce NOW's streaming protocol, comparing three implementations: the official web client, official native client, and the OpenNow open-source implementation.

---

## Documentation Index

### Core Protocol

| Document | Description |
|----------|-------------|
| [protocol.md](protocol.md) | WebRTC/RTP protocol details, SDP, ICE, signaling |
| [session.md](session.md) | CloudMatch API, authentication, session management |
| [datachannel.md](datachannel.md) | Data channel message formats and binary protocols |

### Media

| Document | Description |
|----------|-------------|
| [video.md](video.md) | Video decoding, RTP packetization, codecs (H.264/H.265/AV1) |
| [audio.md](audio.md) | Audio handling, Opus codec, RTP, synchronization |
| [rendering.md](rendering.md) | GPU rendering, shaders, YUV-RGB conversion |

### Input

| Document | Description |
|----------|-------------|
| [keyboard.md](keyboard.md) | Keyboard input protocol, keycodes, modifiers |
| [cursor.md](cursor.md) | Mouse/cursor handling, capture, rendering |
| [controller.md](controller.md) | Gamepad/controller input, button mapping, rumble |

### Telemetry

| Document | Description |
|----------|-------------|
| [statistics.md](statistics.md) | QoS metrics, bitrate adaptation, RTCP stats |

---

## Quick Reference

### Key Endpoints

```
Authentication:  https://login.nvidia.com/authorize
Token:           https://login.nvidia.com/token
CloudMatch:      https://{zone}.cloudmatchbeta.nvidiagrid.net/v2/session
Games:           https://games.geforce.com/graphql
Service URLs:    https://pcs.geforcenow.com/v1/serviceUrls
```

### Key Headers

```
Authorization: GFNJWT {token}
nv-client-id: {uuid}
nv-client-type: NATIVE
nv-client-version: 2.0.80.173
nv-client-streamer: NVIDIA-CLASSIC
```

### Data Channel Names

| Channel | Purpose | Reliability |
|---------|---------|-------------|
| `input_channel_v1` | Keyboard, handshake | Reliable, ordered |
| `input_channel_partially_reliable` | Mouse movement | Unreliable, 8ms lifetime |
| `cursor_channel` | Cursor updates | Reliable, ordered |
| `control_channel` | Control messages | Reliable, ordered |

### Input Message Types

| Type | Value | Size | Description |
|------|-------|------|-------------|
| HEARTBEAT | 0x02 | 4B | Keep-alive |
| KEY_DOWN | 0x03 | 18B | Keyboard press |
| KEY_UP | 0x04 | 18B | Keyboard release |
| MOUSE_REL | 0x07 | 22B | Relative mouse movement |
| MOUSE_BUTTON_DOWN | 0x08 | 18B | Mouse button press |
| MOUSE_BUTTON_UP | 0x09 | 18B | Mouse button release |
| MOUSE_WHEEL | 0x0A | 22B | Mouse scroll |

### Video Codecs

| Codec | Payload Type | Clock Rate |
|-------|--------------|------------|
| H.264 | 96 | 90000 Hz |
| H.265/HEVC | 127 | 90000 Hz |
| AV1 | 98 | 90000 Hz |

### Audio Codec

| Codec | Payload Type | Sample Rate | Channels |
|-------|--------------|-------------|----------|
| Opus | 111 | 48000 Hz | 2 (stereo) |
| Multiopus | 100 | 48000 Hz | 2-8 |

### Color Space (BT.709)

```
Y' = (Y - 16/255) * 1.1644
U' = (U - 128/255) * 1.1384
V' = (V - 128/255) * 1.1384

R = Y' + 1.5748 * V'
G = Y' - 0.1873 * U' - 0.4681 * V'
B = Y' + 1.8556 * U'
```

---

## Implementation Comparison

| Feature | Web Client | Official Client | OpenNow |
|---------|-----------|-----------------|---------|
| **Language** | JavaScript | C++ (CEF) | Rust |
| **WebRTC** | Browser native | libwebrtc | webrtc-rs |
| **Video Decode** | Browser | NVDEC | FFmpeg + CUVID |
| **Rendering** | WebGL/WebGPU | DirectX 12 | wgpu |
| **Input** | DOM Events | Raw Input | winit + Raw Input |
| **Audio** | WebAudio | Native | cpal + Opus |

---

## Protocol Version

- **Client Version:** 2.0.80.173
- **Input Protocol:** v2/v3+
- **WebRTC SDP:** Custom nvstSdp extensions
- **OAuth:** PKCE with code_challenge_method=S256

---

## License

This documentation is for educational and reverse engineering purposes only.
