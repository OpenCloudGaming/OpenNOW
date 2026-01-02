# GeForce NOW Data Channel Protocol - Reverse Engineering Documentation

## 1. Data Channel Names & Configuration

### Input Channels

**input_channel_v1 (Reliable)**
```
Name: input_channel_v1
Ordered: true
Reliable: true
MaxRetransmits: 0
binaryType: arraybuffer
Used for: Keyboard, mouse buttons, wheel, handshake
```

**input_channel_partially_reliable (Low Latency)**
```
Name: input_channel_partially_reliable
Ordered: false
Reliable: false
MaxPacketLifeTime: 8ms
Used for: Mouse movement (can drop packets)
```

### Other Channels

**cursor_channel**
```
Ordered: true
Reliable: true
Used for: Cursor image updates, hotspot coordinates
```

**control_channel**
```
Ordered: true
Reliable: true
Used for: Server-to-client JSON messages (network test results)
```

**stats_channel**
```
Used for: Telemetry data (optional)
```

---

## 2. Input Message Types

| Type | Value | Size | Description |
|------|-------|------|-------------|
| HEARTBEAT | 0x02 | 4B | Keep-alive |
| KEY_DOWN | 0x03 | 18B | Keyboard pressed |
| KEY_UP | 0x04 | 18B | Keyboard released |
| MOUSE_ABS | 0x05 | - | Absolute mouse position |
| MOUSE_REL | 0x07 | 22B | Relative mouse movement |
| MOUSE_BUTTON_DOWN | 0x08 | 18B | Mouse button pressed |
| MOUSE_BUTTON_UP | 0x09 | 18B | Mouse button released |
| MOUSE_WHEEL | 0x0A | 22B | Mouse wheel scroll |

---

## 3. Binary Message Structures

### Heartbeat (4 bytes)
```
[0-3] Type: 0x02 (Little Endian u32)
```

### Key Down/Up (18 bytes)
```
[0-3]   Type: 0x03 or 0x04 (LE u32)
[4-5]   Keycode: u16 (BE) - Windows VK code
[6-7]   Modifiers: u16 (BE) - SHIFT|CTRL|ALT|META|CAPS|NUMLOCK
[8-9]   Scancode: u16 (BE) - USB HID scancode
[10-17] Timestamp: u64 (BE) - Microseconds
```

### Mouse Relative (22 bytes)
```
[0-3]   Type: 0x07 (LE u32)
[4-5]   dx: i16 (BE) - Relative X movement
[6-7]   dy: i16 (BE) - Relative Y movement
[8-9]   Reserved: u16 (0)
[10-13] Reserved: u32 (0)
[14-21] Timestamp: u64 (BE)
```

### Mouse Button Down/Up (18 bytes)
```
[0-3]   Type: 0x08 or 0x09 (LE u32)
[4]     Button: u8 (0=LEFT, 1=RIGHT, 2=MIDDLE, 3=BACK, 4=FORWARD)
[5]     Padding: u8 (0)
[6-9]   Reserved: u32 (0)
[10-17] Timestamp: u64 (BE)
```

### Mouse Wheel (22 bytes)
```
[0-3]   Type: 0x0A (LE u32)
[4-5]   Horizontal: i16 (BE) - Usually 0
[6-7]   Vertical: i16 (BE) - Positive=scroll up
[8-9]   Reserved: u16 (0)
[10-13] Reserved: u32 (0)
[14-21] Timestamp: u64 (BE)
```

---

## 4. Handshake Protocol

### Server Initiates
Sends handshake on input_channel_v1:
```
New Format: [0x0E, major_version, minor_version, flags]
Old Format: Direct version bytes
```

### Client Response
Echo the same bytes back to signal ready state.

### Version Detection
```rust
if data.len() >= 4 {
    // New format: version at bytes 2-4
    let version = u16::from_le_bytes([data[2], data[3]]);
} else {
    // Old format: version is first word
    let version = u16::from_le_bytes([data[0], data[1]]);
}
```

---

## 5. Protocol Versions

### Version 2 (Legacy)
- Direct event encoding
- No wrapper

### Version 3+ (Modern)
Each event wrapped with marker byte:
```
[0]     0x22 (wrapper marker = 34 decimal)
[1...N] Original message bytes
```

---

## 6. Modifier Flags

```
SHIFT:     0x01
CTRL:      0x02
ALT:       0x04
META:      0x08
CAPS_LOCK: 0x10
NUM_LOCK:  0x20
```

---

## 7. USB HID Scancodes

```
A-Z:          0x04-0x1D
0-9:          0x1E-0x27
ENTER:        0x28
ESCAPE:       0x29
BACKSPACE:    0x2A
TAB:          0x2B
SPACE:        0x2C
F1-F12:       0x3A-0x45
LEFT_CTRL:    0xE0
LEFT_SHIFT:   0xE1
LEFT_ALT:     0xE2
LEFT_META:    0xE3
RIGHT_CTRL:   0xE4
RIGHT_SHIFT:  0xE5
RIGHT_ALT:    0xE6
RIGHT_META:   0xE7
```

---

## 8. Mouse Coalescing

### Configuration
- Interval: 4ms (250Hz effective rate)
- Constant: `MOUSE_COALESCE_INTERVAL_US = 4_000`

### Behavior
- Accumulates dx/dy deltas atomically
- Flushed when interval expires OR on button events
- Button events always flush pending movement first

---

## 9. Control Channel Messages

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

---

## 10. Timestamp Encoding

### Format
- Type: u64
- Unit: Microseconds since session start
- Encoding: Big-Endian in event messages

### Generation
```rust
pub fn get_timestamp_us() -> u64 {
    let elapsed = session_start.elapsed().as_micros() as u64;
    unix_start_us.wrapping_add(elapsed)
}
```

---

## 11. Reliability & Ordering

| Channel | Ordered | Reliable | MaxLifetime | Use Case |
|---------|---------|----------|-------------|----------|
| input_channel_v1 | YES | YES | ∞ | Keyboard, handshake |
| input_channel_partially_reliable | NO | NO | 8ms | Mouse movement |
| cursor_channel | YES | YES | ∞ | Cursor images |
| control_channel | YES | YES | ∞ | Bidirectional control |
| stats_channel | YES | YES | ∞ | Telemetry |

---

## 12. Byte-Level Examples

### Key Down (Shift+A)
```
Hex: 03 00 00 00 00 41 00 01 00 00 12 34 56 78 9A BC DE F0

[00-03] 03 00 00 00  = Type 3 (LE) = KEY_DOWN
[04-05] 00 41        = Keycode 0x0041 (VK_A) (BE)
[06-07] 00 01        = Modifiers 0x0001 (SHIFT) (BE)
[08-09] 00 00        = Scancode 0x0000 (BE)
[10-17]              = Timestamp (BE u64)
```

### Mouse Movement (dx=100, dy=-50)
```
Hex: 07 00 00 00 00 64 FF CE 00 00 00 00 00 00 12 34 56 78 9A BC DE F0

[00-03] 07 00 00 00  = Type 7 (LE) = MOUSE_REL
[04-05] 00 64        = dx=100 (BE i16)
[06-07] FF CE        = dy=-50 (BE i16, two's complement)
[08-13]              = Reserved
[14-21]              = Timestamp (BE u64)
```

### Protocol v3+ Wrapped Event
```
[0]    0x22          = Wrapper marker
[1...] Raw event bytes
```

---

## 13. Implementation Notes

1. **Channel Creation Order**: Input channels MUST be created BEFORE SDP negotiation
2. **Timestamp Synchronization**: Microseconds relative to session start
3. **Mouse Channel Fallback**: Use reliable channel if partially_reliable not ready
4. **Handshake Required**: No input processed until handshake response echoed
5. **Event Coalescing**: Mouse events coalesce every 4ms
6. **Data Types**: Mixed endianness - opcodes LE, fields BE

---

## 14. Comparison

| Feature | Web Client | Official Client | OpenNow |
|---------|-----------|-----------------|---------|
| Channel Creation | Explicit | Native C++ | webrtc-rs |
| Coalescing | 4-16ms | Hardware | 4ms |
| Protocol Version | v2/v3+ | Proprietary | v2/v3+ |
| Input Encoding | DataView | Native | bytes crate |
| Channel Types | 4+ channels | Similar | 2 input channels |
