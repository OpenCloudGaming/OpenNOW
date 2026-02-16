# GeForce NOW Statistics & QoS - Reverse Engineering Documentation

## 1. Statistics Structure

### StreamStats (OpenNow)
```rust
pub struct StreamStats {
    pub resolution: String,
    pub fps: f32,
    pub render_fps: f32,
    pub target_fps: u32,
    pub bitrate_mbps: f32,
    pub latency_ms: f32,
    pub decode_time_ms: f32,
    pub render_time_ms: f32,
    pub input_latency_ms: f32,
    pub codec: String,
    pub gpu_type: String,
    pub server_region: String,
    pub packet_loss: f32,
    pub jitter_ms: f32,
    pub frames_received: u64,
    pub frames_decoded: u64,
    pub frames_dropped: u64,
    pub frames_rendered: u64,
}
```

### DecodeStats
```rust
pub struct DecodeStats {
    pub decode_time_ms: f32,
    pub frame_produced: bool,
    pub needs_keyframe: bool,
}
```

---

## 2. QoS SDP Parameters

### FEC (Forward Error Correction)
```
a=vqos.fec.rateDropWindow:10
a=vqos.fec.minRequiredFecPackets:2
a=vqos.fec.repairMinPercent:5
a=vqos.fec.repairPercent:5
a=vqos.fec.repairMaxPercent:35
```

### DFC (Dynamic FPS Control)
```
a=vqos.dfc.enable:1
a=vqos.dfc.decodeFpsAdjPercent:85
a=vqos.dfc.targetDownCooldownMs:250
a=vqos.dfc.dfcAlgoVersion:2
a=vqos.dfc.minTargetFps:100  (or 60 for lower fps)
```

### DRC (Dynamic Resolution Control)
```
a=vqos.drc.minQpHeadroom:20
a=vqos.drc.lowerQpThreshold:100
a=vqos.drc.upperQpThreshold:200
a=vqos.drc.minAdaptiveQpThreshold:180
a=vqos.drc.iirFilterFactor:100
```

### Bitrate Control
```
a=vqos.bw.maximumBitrateKbps:{max_bitrate}
a=vqos.bw.minimumBitrateKbps:{max_bitrate / 10}
a=video.initialBitrateKbps:{max_bitrate / 2}
a=video.initialPeakBitrateKbps:{max_bitrate / 2}
```

### BWE (Bandwidth Estimation)
```
a=bwe.useOwdCongestionControl:1
a=bwe.iirFilterFactor:8
a=vqos.drc.bitrateIirFilterFactor:18
```

### NACK (Retransmission)
```
a=video.enableRtpNack:1
a=video.rtpNackQueueLength:1024
a=video.rtpNackQueueMaxPackets:512
a=video.rtpNackMaxPacketCount:25
```

### Packet Pacing
```
a=packetPacing.minNumPacketsPerGroup:15
a=packetPacing.numGroups:3  (or 5 for 60fps)
a=packetPacing.maxDelayUs:1000
a=packetPacing.minNumPacketsFrame:10
```

---

## 3. RTCP Statistics Collection

### Inbound RTP Video Stats
```
packetsReceived     - Total packets received
packetsLost         - Total packets lost
bytesReceived       - Total bytes received
framesReceived      - Total frames received
framesDecoded       - Total frames decoded
framesDropped       - Total frames dropped
pliCount            - Picture Loss Indication count
jitter              - Network jitter
jitterBufferDelay   - Jitter buffer delay (ms)
totalInterFrameDelay - Total inter-frame delay
totalDecodeTime     - Total decode time
frameHeight/Width   - Frame dimensions
```

### Connection Stats
```
currentRoundTripTime    - RTT (ms)
availableOutgoingBitrate
availableIncomingBitrate
```

### Audio Stats
```
audioLevel
concealedSamples
jitterBufferDelay
totalSamplesDuration
```

---

## 4. Frame Timing Metrics

### Decode Time Tracking
- Measured from packet receive to decode completion
- Tracked per-frame in DecodeStats
- Average calculated over 1-second intervals

### Latency Calculations
```
Pipeline Latency = Sum(decode_times) / frame_count
Input Latency    = Time from event creation to transmission
Network Latency  = RTT / 2 (approximation)
Total Latency    = Network + Decode + Render
```

---

## 5. Bitrate Adaptation

### Calculation
```rust
bitrate_mbps = (bytes_received * 8) / (elapsed_seconds * 1_000_000)
```

### Server-Side Adaptation Triggers
- Decode time exceeds threshold
- Frame drop rate increases
- Packet loss percentage increases
- QP (Quantization Parameter) feedback

### Packet Loss Calculation
```
PacketLoss% = (packetsLost * 100) / (packetsLost + packetsReceived)
```

---

## 6. OSD (On-Screen Display)

### Display Locations
- BottomLeft (default)
- BottomRight
- TopLeft
- TopRight

### Display Information
```
Resolution & FPS:   "1920x1080 @ 60 fps"
Codec & Bitrate:    "H.264 • 25.5 Mbps"
Latency:            Color-coded (Green <30ms, Yellow 30-60ms, Red >60ms)
Packet Loss:        Only shown if >0% (Yellow <1%, Red >=1%)
Decode & Render:    "Decode: 5.2 ms • Render: 1.8 ms"
Frame Stats:        "Frames: 1204 rx, 1198 dec, 6 drop"
GPU & Region:       "RTX 4090 • us-east-1"
```

---

## 7. Telemetry Binary Format

### Audio Stats (Type 4)
```
Float64: audioLevel
Uint32:  concealedSamples
Uint32:  concealmentEvents
Uint32:  insertedSamplesForDeceleration
Float64: jitterBufferDelay
Uint32:  jitterBufferEmittedCount
Uint32:  removedSamplesForAcceleration
Uint32:  silentConcealedSamples
Float64: totalSamplesReceived
Float64: totalSamplesDuration
Float64: timestamp
```

### Video Stats (Type 3)
```
Uint32:  framesDecoded
Uint32:  framesDropped
Uint32:  frameHeight
Uint32:  frameWidth
Uint32:  framesReceived
Float64: jitterBufferDelay
Uint32:  jitterBufferEmittedCount
Float64: timestamp
```

### Inbound RTP Stats (Type 2)
```
Uint32:  packetsReceived
Uint32:  bytesReceived
Uint32:  packetsLost
Float64: lastPacketReceivedTimestamp
Float64: jitter
Float64: timestamp
```

---

## 8. Quality Adjustment Parameters

### QP (Quantization Parameter) Thresholds
```
a=vqos.drc.minQpHeadroom:20
a=vqos.drc.lowerQpThreshold:100
a=vqos.drc.upperQpThreshold:200
a=vqos.drc.minAdaptiveQpThreshold:180
a=vqos.drc.qpMaxResThresholdAdj:4
a=vqos.grc.qpMaxResThresholdAdj:4
```

### Decode Time Thresholds
```
a=vqos.resControl.cpmRtc.decodeTimeThresholdMs:9
a=vqos.resControl.cpmRtc.badNwSkipFramesCount:600
```

---

## 9. PLI (Picture Loss Indication)

### Trigger Conditions
- 10 consecutive packets without decoded frame
- After 5+ failures, sent every 20 packets

### Implementation
```rust
let pli = PictureLossIndication {
    sender_ssrc: 0,
    media_ssrc: video_ssrc,
};
peer_connection.write_rtcp(&[Box::new(pli)]).await?
```

---

## 10. High FPS Optimizations (120+)

### SDP Parameters
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
a=video.fbcDynamicFpsGrabTimeoutMs:18
```

---

## 11. Comparison

| Feature | Web Client | Official Client | OpenNow |
|---------|-----------|-----------------|---------|
| Stats Collection | Full WebRTC getStats() | Native C++ | Basic StreamStats |
| Adaptive Bitrate | Server-side | Server-side | Not implemented |
| Adaptive Resolution | DRC algorithm | DRC/DFC hybrid | Not implemented |
| Adaptive FPS | DFC for high-FPS | DFC | Not implemented |
| Telemetry | Binary format | GEAR events | Basic logging |
| RTCP Stats | Full RFC 3550 | Native RTCP | Not implemented |
| BWE Algorithm | OWD congestion | Advanced | Not implemented |
| FEC | SDP configured | Dynamic | SDP configured |
| NACK | Full support | Full support | SDP configured |

---

## 12. Telemetry Events

### Web Client Events
```
TelemetryHandlerChanged
WorkerProblem
WebWorkerProblem
VideoPaused
MissingInboundRtpVideo
InboundVideoStats
TURN Server Details
Worker Thread Creation Failed
```

---

## 13. Control Channel Messages

### finAck (Network Test Result)
```json
{
  "finAck": {
    "downlinkBandwidth": <number MHz>,
    "packetLoss": <number percent>,
    "latency": <number ms>
  }
}
```

### fin (Graceful Shutdown)
```json
{
  "fin": {
    "sessionId": "<session_id>",
    "packetsLost": <number>,
    "packetsReceived": <number>
  }
}
```
