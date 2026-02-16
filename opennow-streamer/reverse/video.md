# GeForce NOW Video Handling - Reverse Engineering Documentation

## 1. Video Codec Support

### Supported Codecs
- **H.264/AVC** (primary, widest compatibility)
- **H.265/HEVC** (better compression, dynamic payload type negotiation)
- **AV1** (newest codec, RTX 40+ only, requires CUVID or QSV)

### Codec Registration (WebRTC)
From `src/webrtc/peer.rs`:
- H.264: Standard registered via `register_default_codecs()`
- H.265: Custom registration with MIME type `"video/H265"`, clock rate 90kHz, payload type 0 (dynamic)
- AV1: Custom registration with MIME type `"video/AV1"`, clock rate 90kHz, payload type 0 (dynamic)

---

## 2. RTP Packet Structure & Depacketization

### RTP Header Format (RFC 3550)
```
0                   1                   2                   3
0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|V=2|P|X|  CC   |M|     PT      |       sequence number         |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                           timestamp                           |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|           synchronization source (SSRC) identifier            |
+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
```

Key fields:
- **V (Version)**: 2
- **M (Marker)**: 1 on last packet of frame (critical for frame boundaries)
- **PT (Payload Type)**: 96 (H.264), 127 (H.265), 98 (AV1)
- **Timestamp**: 90 kHz clock for video

### H.264 RTP Payload Types (RFC 6184)

**Single NAL Unit (PT 1-23):**
- Payload is raw NAL unit without start code
- Decoder adds start code: `0x00 0x00 0x00 0x01`

**STAP-A (Single-Time Aggregation Packet, PT 24):**
```
[0x18] + [Size1:2B BE] + [NAL1] + [Size2:2B BE] + [NAL2] + ...
```

**FU-A (Fragmentation Unit, PT 28):**
```
[0x7C] + [FU Header: S|E|R|Type] + [Fragment Payload]
FU Header: S=1 (start), E=1 (end), R=0 (reserved)
```

### H.265/HEVC RTP Payload (RFC 7798)

**NAL Unit Header: 2 bytes**
```
Byte 0: F(1) | Type(6) | LayerId_hi(1)
Byte 1: LayerId_lo(5) | TId_plus1(3)
```

**AP (Aggregation Packet, Type 48):**
```
[Header:2B] + [Size1:2B BE] + [NAL1] + [Size2:2B BE] + [NAL2] + ...
```

**FU (Fragmentation Unit, Type 49):**
```
[Header:2B] + [FU Header: S|E|Reserved|Type] + [Fragment Payload]
```

### AV1 RTP Payload (RFC 9000)

**Aggregation Header (1 byte):**
```
Z(1) | Y(1) | W(2) | N(1) | Reserved(3)
Z: Continuation of previous OBU fragment
Y: Last OBU fragment or complete OBU
W: Number of OBU elements
N: First packet of coded video sequence
```

**OBU Types:**
- 1: SEQUENCE_HEADER (critical, must precede picture data)
- 4: TILE_GROUP (contains picture data)
- 6: FRAME (complete frame)

---

## 3. NAL Unit Types

### H.264 NAL Unit Types
```
1:  Slice (Non-IDR) - P-frame/B-frame
5:  IDR Slice - Keyframe
6:  SEI (Supplemental Enhancement Information)
7:  SPS (Sequence Parameter Set)
8:  PPS (Picture Parameter Set)
24: STAP-A (aggregation)
28: FU-A (fragmentation)
```

### H.265/HEVC NAL Unit Types
```
19: IDR_W_RADL - Keyframe
20: IDR_N_LP - Keyframe
32: VPS (Video Parameter Set)
33: SPS (Sequence Parameter Set)
34: PPS (Picture Parameter Set)
48: AP (Aggregation Packet)
49: FU (Fragmentation Unit)
```

### SPS/PPS Caching Strategy
- H.264: Type 7 (SPS) and Type 8 (PPS) cached, prepended to IDR frames (type 5)
- H.265: Types 32/33/34 (VPS/SPS/PPS) cached, prepended to IDR frames (types 19-20)
- AV1: SEQUENCE_HEADER (type 1) cached, prepended to frames missing it

---

## 4. Color Space & Pixel Formats

### YUV420P (Planar)
```
Layout:
Y Plane: height * stride (full resolution)
U Plane: (height/2) * (stride/2)
V Plane: (height/2) * (stride/2)
```

### NV12 (Semi-planar)
```
Layout:
Y Plane: height * stride_y (full resolution)
UV Plane: (height/2) * stride_uv (interleaved U,V pairs)
```

### YUV to RGB Conversion (BT.709)

**Limited Range to Full Range:**
```
y = (y_raw - 0.0625) * 1.1644   // (Y - 16/255) * (255/219)
u = (u_raw - 0.5) * 1.1384      // (U - 128/255) * (255/224)
v = (v_raw - 0.5) * 1.1384      // (V - 128/255) * (255/224)
```

**BT.709 Matrix:**
```
R = Y + 1.5748 * V
G = Y - 0.1873 * U - 0.4681 * V
B = Y + 1.8556 * U
```

**Integer Math (Fast CPU Fallback):**
```
R = (y + ((359 * v) >> 8)).clamp(0, 255)
G = (y - ((88 * u + 183 * v) >> 8)).clamp(0, 255)
B = (y + ((454 * u) >> 8)).clamp(0, 255)
```

---

## 5. Hardware Acceleration

### Decoder Priority Order

**Windows:**
1. h264_cuvid / hevc_cuvid / av1_cuvid (NVIDIA CUDA)
2. h264_qsv / hevc_qsv / av1_qsv (Intel QuickSync)
3. h264_d3d11va / hevc_d3d11va (DirectX 11)
4. Software fallback

**macOS:**
- VideoToolbox (native macOS, NV12 output)

**Linux:**
1. CUVID (NVIDIA)
2. VAAPI (AMD/Intel)
3. Software fallback

---

## 6. Frame Timing & Synchronization

### RTP Timestamp Calculation
```
90 kHz clock rate:
- 60 FPS = 1500 RTP ticks per frame (90000/60)
- 120 FPS = 750 RTP ticks per frame (90000/120)
- 240 FPS = 375 RTP ticks per frame (90000/240)
```

### Picture Loss Indication (PLI)
From `src/webrtc/peer.rs`:
```rust
let pli = PictureLossIndication {
    sender_ssrc: 0,
    media_ssrc: video_ssrc,
};
peer_connection.write_rtcp(&[Box::new(pli)]).await?
```

**Trigger Conditions:**
- 10 consecutive packets without decoded frame
- Additional requests every 20 packets if still failing

---

## 7. Streaming Parameters (SDP)

### Video Quality Settings
```
a=video.packetSize:1140
a=video.maxFPS:120
a=video.initialBitrateKbps:25000
a=vqos.bw.maximumBitrateKbps:50000
a=vqos.bw.minimumBitrateKbps:5000
```

### NACK Configuration
```
a=video.enableRtpNack:1
a=video.rtpNackQueueLength:1024
a=video.rtpNackQueueMaxPackets:512
a=video.rtpNackMaxPacketCount:25
```

### High FPS Optimizations (120+)
```
a=video.encoderFeatureSetting:47
a=video.encoderPreset:6
a=video.fbcDynamicFpsGrabTimeoutMs:6
a=bwe.iirFilterFactor:8
```

### 240+ FPS
```
a=video.enableNextCaptureMode:1
a=vqos.maxStreamFpsEstimate:240
a=video.videoSplitEncodeStripsPerFrame:3
```

---

## 8. Decode Process Flow

```
RTP Packet Received
       ↓
Depacketize (H.264/H.265/AV1)
       ↓
Decode Async (FFmpeg + Hardware)
       ↓
Extract Planes (YUV420P or NV12)
       ↓
Upload to GPU Textures
       ↓
GPU Shader (YUV→RGB)
       ↓
Present to Screen
```

---

## 9. Comparison

| Feature | Web Client | OpenNow | Official Client |
|---------|-----------|---------|-----------------|
| RTP Parsing | libwebrtc | Custom Rust | libwebrtc |
| H.265 Support | Yes | Yes | Yes |
| AV1 Support | Yes | Yes (RTX 40+) | Yes |
| Hardware Decode | Browser | CUVID/QSV/VT | NVDEC |
| Color Space | BT.709 | BT.709 | BT.709 |
| Frame Format | Varies | YUV420P/NV12 | NV12 |
