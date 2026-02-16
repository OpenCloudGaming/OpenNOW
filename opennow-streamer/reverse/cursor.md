# GeForce NOW Mouse/Cursor Handling - Reverse Engineering Documentation

## 1. Data Channels

### Input Channels

**Primary Input Channel (Reliable)**
- Name: `input_channel_v1`
- Ordered: Yes
- Reliable: Yes
- Used for: Keyboard, mouse buttons, wheel, handshake

**Mouse Channel (Partially Reliable)**
- Name: `input_channel_partially_reliable`
- Ordered: No
- Reliable: No (8ms max lifetime)
- Used for: Low-latency mouse movement

**Cursor Channel**
- Name: `cursor_channel`
- Ordered: Yes
- Reliable: Yes
- Used for: Cursor image updates, hotspot coordinates

---

## 2. Mouse Movement (Type 7 - INPUT_MOUSE_REL)

### Binary Format (22 bytes)
```
[0-3]   Type: 0x07 (4 bytes, Little Endian)
[4-5]   Delta X: i16 (2 bytes, Big Endian, signed)
[6-7]   Delta Y: i16 (2 bytes, Big Endian, signed)
[8-9]   Reserved: u16 (0x00 0x00)
[10-13] Reserved: u32 (0x00 0x00 0x00 0x00)
[14-21] Timestamp: u64 (8 bytes, Big Endian, microseconds)
```

### Coalescing
- Interval: 4ms (250Hz effective rate)
- Accumulates dx/dy deltas
- Flushed on interval expiry OR before button events

---

## 3. Mouse Button Down (Type 8)

### Binary Format (18 bytes)
```
[0-3]   Type: 0x08 (4 bytes, Little Endian)
[4]     Button: u8 (0=Left, 1=Right, 2=Middle, 3=Back, 4=Forward)
[5]     Padding: u8 (0)
[6-9]   Reserved: u32 (0)
[10-17] Timestamp: u64 (Big Endian, microseconds)
```

---

## 4. Mouse Button Up (Type 9)

### Binary Format (18 bytes)
```
[0-3]   Type: 0x09 (4 bytes, Little Endian)
[4]     Button: u8 (0=Left, 1=Right, 2=Middle, 3=Back, 4=Forward)
[5]     Padding: u8 (0)
[6-9]   Reserved: u32 (0)
[10-17] Timestamp: u64 (Big Endian, microseconds)
```

---

## 5. Mouse Wheel (Type 10)

### Binary Format (22 bytes)
```
[0-3]   Type: 0x0A (4 bytes, Little Endian)
[4-5]   Horizontal Delta: i16 (Big Endian, usually 0)
[6-7]   Vertical Delta: i16 (Big Endian, positive=scroll up)
[8-9]   Reserved: u16 (0)
[10-13] Reserved: u32 (0)
[14-21] Timestamp: u64 (Big Endian, microseconds)
```

### Wheel Delta Values
- Standard: WHEEL_DELTA = 120 per notch
- Positive = scroll up
- Negative = scroll down

---

## 6. Cursor Capture Modes

### Windows Implementation
```rust
// Preferred: Confined to window
CursorGrabMode::Confined

// Fallback: Locked (hidden)
CursorGrabMode::Locked

// Released: Normal cursor
CursorGrabMode::None
```

### macOS Implementation
- Uses Core Graphics Event Taps
- Captures at HID level: `CGEventTapLocation::HIDEventTap`

---

## 7. Raw Input (Windows)

### HID Registration
```rust
let device = RAWINPUTDEVICE {
    usage_page: 0x01,  // HID_USAGE_PAGE_GENERIC
    usage: 0x02,       // HID_USAGE_GENERIC_MOUSE
    flags: 0,          // Only when window focused
    hwnd_target: hwnd,
};
```

### Benefits
- No OS acceleration applied
- Hardware-level relative deltas
- Lower latency than standard events

---

## 8. Local Cursor Rendering

### Position Tracking
```rust
struct LocalCursor {
    x: AtomicI32,
    y: AtomicI32,
    visible: AtomicBool,
    stream_width: AtomicU32,
    stream_height: AtomicU32,
}
```

### Update Logic
- Updated on every raw input event
- Bounded to stream dimensions
- Provides instant visual feedback

---

## 9. Mouse Coalescing

### Implementation
```rust
pub struct MouseCoalescer {
    accumulated_dx: AtomicI32,
    accumulated_dy: AtomicI32,
    last_send_us: AtomicU64,
    coalesce_interval_us: u64,  // 4000 (4ms)
}

pub fn accumulate(&self, dx: i32, dy: i32) -> Option<(i16, i16, u64)> {
    self.accumulated_dx.fetch_add(dx, Ordering::Relaxed);
    self.accumulated_dy.fetch_add(dy, Ordering::Relaxed);

    let now_us = session_elapsed_us();
    if now_us - last_send >= coalesce_interval_us {
        // Flush accumulated movement
        Some((dx as i16, dy as i16, timestamp_us))
    } else {
        None
    }
}
```

### Event Ordering
Movement is **flushed BEFORE button events**:
```
MouseMove(100,200) → MouseButtonDown → MouseMove(50,50)
```

---

## 10. Cursor Image Updates

### Cursor Channel Messages
- Image data (PNG format)
- Hotspot coordinates (X, Y)
- Visibility state

### Cursor Type Values
```
CursorType = {
    None: 0,
    Mouse: 1,
    Keyboard: 2,
    Gamepad: 4,
    Touch: 8,
    All: 15
}
```

---

## 11. Timestamp Generation

```rust
pub fn get_timestamp_us() -> u64 {
    if let Some(ref t) = *SESSION_TIMING.read() {
        let elapsed_us = t.start.elapsed().as_micros() as u64;
        t.unix_us.wrapping_add(elapsed_us)
    } else {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0)
    }
}
```

---

## 12. Comparison

| Feature | Web Client | Official Client | OpenNow |
|---------|-----------|-----------------|---------|
| Input API | Pointer Lock API | Raw Input | Raw Input + winit |
| Coalescing | 4-16ms | Hardware optimized | 4ms |
| Cursor Capture | Pointer Lock | Native capture | CursorGrabMode |
| Local Cursor | Canvas rendering | Native rendering | GPU rendering |
| Latency | 40-80ms | 10-30ms | 20-40ms |

---

## 13. Error Codes

- `StreamCursorChannelError` (3237093897): Cursor channel failure
- `StreamerCursorChannelNotOpen` (3237093920): Channel not established
- `ServerDisconnectedInvalidMouseState` (3237094150): Invalid mouse state

---

## 14. Protocol v3+ Wrapper

For protocol version 3+:
```
[0]     0x22 (wrapper marker)
[1...N] Raw mouse event bytes
```

---

## 15. Byte-Level Example

### Mouse Movement (dx=100, dy=-50)
```
Hex: 07 00 00 00 00 64 FF CE 00 00 00 00 00 00 12 34 56 78 9A BC DE F0

[00-03] 07 00 00 00  = Type 7 (LE) = MOUSE_REL
[04-05] 00 64        = dx=100 (BE i16)
[06-07] FF CE        = dy=-50 (BE i16, two's complement)
[08-13] 00 00 00 00 00 00 = Reserved
[14-21] Timestamp (BE u64)
```

### Mouse Button Down (Left Click)
```
Hex: 08 00 00 00 00 00 00 00 00 00 12 34 56 78 9A BC DE F0

[00-03] 08 00 00 00  = Type 8 (LE) = MOUSE_BUTTON_DOWN
[04]    00           = Button 0 (Left)
[05]    00           = Padding
[06-09] 00 00 00 00  = Reserved
[10-17] Timestamp (BE u64)
```
