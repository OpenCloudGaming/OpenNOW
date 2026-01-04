# GeForce NOW Audio Handling - Reverse Engineering Documentation

## 1. Audio Codec Details

### Opus Configuration
- **Codec**: Opus (RFC 6716)
- **Sample Rate**: 48000 Hz
- **Channels**: 2 (stereo) or up to 8 (multiopus)
- **Payload Types**:
  - 101: opus/48000/2 (standard stereo)
  - 100: multiopus/48000/N (N = 2, 4, 6, or 8 channels)

### SDP Configuration
```
a=rtpmap:101 opus/48000/2
a=fmtp:101 minptime=10;useinbandfec=1

a=rtpmap:100 multiopus/48000/2
a=fmtp:100 minptime=10;useinbandfec=1;coupled_streams=2
```

### Multiopus Channel Mapping
```
4-channel: FL, FR, BL, BR
6-channel: FL, FR, C, LFE, BL, BR (5.1 surround)
8-channel: FL, FR, C, LFE, BL, BR, SL, SR (7.1 surround)
```

---

## 2. RTP Packet Structure

### RTP Header for Audio
```
0                   1                   2                   3
0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|V=2|P|X|  CC   |M|     PT      |       sequence number         |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                           timestamp                           |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|           synchronization source (SSRC) identifier            |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

**Fields:**
- V: 2 (RTP version)
- P: 0 (no padding)
- X: 0 (no extension typically)
- CC: 0 (no CSRC)
- M: 1 = last packet of frame
- PT: 100 (multiopus) or 101 (opus)
- Timestamp: 48 kHz audio clock

### Opus RTP Payload (RFC 7587)
```
[TOC Byte] [Opus Frame Data...]

TOC Byte: F(1) | C(1) | VBR(2) | MODE(4)
```

---

## 3. Frame Sizes

### Opus at 48 kHz
```
10 ms = 480 samples
20 ms = 960 samples (default)
40 ms = 1920 samples
60 ms = 2880 samples
```

### Typical Configuration
- Default frame size: 20 ms (960 samples)
- Minimum packet interval: 10 ms
- Bitrate: 64-96 kbps for stereo

---

## 4. Audio Buffer Management

### OpenNow AudioBuffer Structure
```rust
struct AudioBuffer {
    samples: Vec<i16>,
    read_pos: usize,
    write_pos: usize,
    capacity: usize,  // ~200ms at 48kHz
    total_written: u64,
    total_read: u64,
}
```

### Buffer Size Calculation
```rust
// At 48000 Hz, 2 channels:
// 48000 * 2 / 5 = 19200 samples = ~200ms buffering
let buffer_size = (sample_rate as usize) * (channels as usize) / 5;
```

### Jitter Handling
- Circular buffer with read/write pointers
- Underrun: Output silence (zeros)
- Overrun: Drop oldest samples
- RTP sequence numbers for packet ordering

---

## 5. Sample Format & Conversion

### PCM Sample Format
```
Output: 16-bit signed PCM (i16)
Range: -32768 to +32767
Channels: Interleaved stereo [L0, R0, L1, R1, ...]
```

### Format Conversion

**From F32 Planar:**
```rust
let sample = (plane[i] * 32767.0).clamp(-32768.0, 32767.0) as i16;
```

**From I16 Planar (Interleave):**
```rust
for i in 0..nb_samples {
    for ch in 0..channels {
        let plane = frame.plane::<i16>(ch);
        output.push(plane[i]);
    }
}
```

---

## 6. Audio/Video Synchronization

### RTP Timestamp Alignment
```
Video: 90 kHz clock
Audio: 48 kHz clock

Frame at 60 FPS:
  Video: 1500 RTP ticks (90000/60)
  Audio: 800 RTP ticks for 16.67ms (48000 * 0.01667)
```

### OpenNow Sync Method
- RTP timestamps provide absolute timing
- Both streams timestamped from server clock
- Audio buffer maintains timing through sample count

---

## 7. Decode Process Flow

```
RTP Packet Received (peer.rs)
       ↓
Extract RTP Payload
       ↓
Send to AudioDecoder (mpsc channel)
       ↓
FFmpeg Opus Decode
       ↓
Convert to i16 samples
       ↓
Write to AudioBuffer
       ↓
AudioPlayer reads from buffer
       ↓
Output to audio device (cpal)
```

### OpenNow Implementation
```rust
// webrtc/mod.rs line 265-276
let mut audio_decoder = AudioDecoder::new(48000, 2)?;
let (audio_tx, mut audio_rx) = mpsc::channel::<Vec<i16>>(32);

std::thread::spawn(move || {
    if let Ok(audio_player) = AudioPlayer::new(48000, 2) {
        while let Some(samples) = audio_rx.blocking_recv() {
            audio_player.push_samples(&samples);
        }
    }
});
```

---

## 8. Device Configuration

### Sample Rate Selection Priority
```rust
1. Use requested 48 kHz if supported
2. Fallback to 44.1 kHz if 48 kHz not supported
3. Use device maximum as last resort
```

### Output Format Selection
```rust
// Scoring system:
// F32 format: 100 points (preferred)
// I16 format: 50 points
// Matching channels: +50 points
// Matching sample rate: +100 points
```

---

## 9. Comparison

| Feature | Web Client | Official Client | OpenNow |
|---------|-----------|-----------------|---------|
| Codec | Opus/Multiopus | Opus | Opus (FFmpeg) |
| Sample Rate | 48 kHz | 48 kHz | 48 kHz |
| Channels | 2-8 (dynamic) | 2-8 | 2 (hardcoded) |
| Decoding | Browser | Native C++ | FFmpeg |
| Output | WebAudio API | WASAPI/etc | cpal |
| Jitter Buffer | Built-in | Yes | RTP seq-based |
| FEC Support | useinbandfec=1 | Yes | No |
| Surround | Yes (8ch) | Yes | Not yet |
| Latency | 20-50ms | 15-30ms | 20-40ms |
| Buffer Size | ~500ms | ~200ms | ~200ms |

---

## 10. OpenNow Limitations

**Current:**
- Hardcoded stereo (2 channels)
- No explicit jitter buffer
- No FEC recovery implementation
- Basic circular buffer

**Future Extensions:**
1. Add surround sound support (modify `AudioDecoder::new()`)
2. Implement jitter buffer with RTP reordering
3. Add FEC parsing for packet loss recovery
