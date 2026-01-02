# GeForce NOW Keyboard Input Protocol - Reverse Engineering Documentation

## 1. Key Event Structure

### Binary Message Format

#### KeyDown Event (Type 3)
```
Byte Layout (18 bytes total):
[0-3]   Type: 0x03 (4 bytes, Little Endian) = INPUT_KEY_DOWN
[4-5]   Keycode: u16 (2 bytes, Big Endian) = Windows Virtual Key code
[6-7]   Modifiers: u16 (2 bytes, Big Endian) = Modifier flags bitmask
[8-9]   Scancode: u16 (2 bytes, Big Endian) = USB HID scancode (usually 0)
[10-17] Timestamp: u64 (8 bytes, Big Endian) = Microseconds since session start
```

#### KeyUp Event (Type 4)
```
Byte Layout (18 bytes total):
[0-3]   Type: 0x04 (4 bytes, Little Endian) = INPUT_KEY_UP
[4-5]   Keycode: u16 (2 bytes, Big Endian) = Windows Virtual Key code
[6-7]   Modifiers: u16 (2 bytes, Big Endian) = Modifier flags bitmask
[8-9]   Scancode: u16 (2 bytes, Big Endian) = USB HID scancode (usually 0)
[10-17] Timestamp: u64 (8 bytes, Big Endian) = Microseconds since session start
```

### Protocol v3+ Wrapper
For protocol version 3+, single events are wrapped:
```
[0]     Wrapper Marker: 0x22 (34 decimal)
[1-18]  Keyboard event payload
Total: 19 bytes for v3+
```

---

## 2. Virtual Key Codes

GFN uses **Windows Virtual Key codes** (VK codes), NOT scancodes.

### Alphabetic Keys
```
VK_A (0x41) through VK_Z (0x5A)
```

### Numeric Keys
```
VK_0 (0x30) through VK_9 (0x39)
```

### Function Keys
```
VK_F1  (0x70) through VK_F12 (0x7B)
VK_F13 (0x7C) through VK_F24 (0x87)
```

### Special Keys
```
VK_ESCAPE    (0x1B)
VK_TAB       (0x09)
VK_CAPITAL   (0x14) - CapsLock
VK_SPACE     (0x20)
VK_ENTER     (0x0D)
VK_BACKSPACE (0x08)
VK_DELETE    (0x2E)
VK_INSERT    (0x2D)
VK_HOME      (0x24)
VK_END       (0x23)
VK_PRIOR     (0x21) - Page Up
VK_NEXT      (0x22) - Page Down
```

### Arrow Keys
```
VK_UP    (0x26)
VK_DOWN  (0x28)
VK_LEFT  (0x25)
VK_RIGHT (0x27)
```

### Numpad Keys
```
VK_NUMPAD0 (0x60) through VK_NUMPAD9 (0x69)
VK_MULTIPLY (0x6A)
VK_ADD      (0x6B)
VK_SUBTRACT (0x6D)
VK_DECIMAL  (0x6E)
VK_DIVIDE   (0x6F)
VK_NUMLOCK  (0x90)
```

### Modifier Keys
```
VK_LSHIFT      (0xA0) - Left Shift
VK_RSHIFT      (0xA1) - Right Shift
VK_LCONTROL    (0xA2) - Left Control
VK_RCONTROL    (0xA3) - Right Control
VK_LMENU       (0xA4) - Left Alt
VK_RMENU       (0xA5) - Right Alt
VK_LWIN        (0x5B) - Left Windows/Meta
VK_RWIN        (0x5C) - Right Windows/Meta
```

### Punctuation Keys
```
VK_OEM_MINUS     (0xBD) - Minus/Underscore
VK_OEM_PLUS      (0xBB) - Plus/Equals
VK_OEM_LBRACKET  (0xDB) - Left Bracket
VK_OEM_RBRACKET  (0xDD) - Right Bracket
VK_OEM_BACKSLASH (0xDC) - Backslash
VK_OEM_SEMICOLON (0xBA) - Semicolon
VK_OEM_QUOTE     (0xDE) - Quote
VK_OEM_TILDE     (0xC0) - Backtick/Tilde
VK_OEM_COMMA     (0xBC) - Comma
VK_OEM_PERIOD    (0xBE) - Period
VK_OEM_SLASH     (0xBF) - Forward Slash
```

---

## 3. Modifier Flags

Modifiers are encoded as a 16-bit bitmask:

```
SHIFT:     0x01
CTRL:      0x02
ALT:       0x04
META:      0x08
CAPS_LOCK: 0x10
NUM_LOCK:  0x20
```

### Important Modifier Behavior

When a modifier key itself is pressed, the modifiers field should be **0x0000**:
```
Shift key down: keycode=0xA0, modifiers=0x00 (not 0x01)
A key down (with Shift held): keycode=0x41, modifiers=0x01
```

---

## 4. USB HID Scancodes

The scancode field is typically set to **0x0000** (unused) in GFN.

Reference scancodes (if needed):
```
0x04 = A through 0x1D = Z
0x1E = 1 through 0x27 = 0
0x28 = Enter
0x29 = Escape
0x2A = Backspace
0x2B = Tab
0x2C = Space
0x3A - 0x45 = F1 through F12
0xE0-0xE7 = Modifier keys
```

---

## 5. Key Repeat Handling

Key repeat events are **filtered out**:

```rust
if event.repeat {
    return;  // Skip key repeat events
}
```

### Key State Tracking

Both clients track currently pressed keys:
```rust
pub struct InputHandler {
    pressed_keys: Mutex<HashSet<u16>>,
}
```

### Focus Loss Handling

When window loses focus, all keys are released:
```rust
pub fn release_all_keys(&self) {
    let keys_to_release: Vec<u16> = pressed_keys.drain().collect();
    for keycode in keys_to_release {
        send_key_up(keycode, 0, 0, timestamp_us);
    }
}
```

---

## 6. IME (Input Method Editor) Support

### Two Independent Channels

1. **Raw Keyboard Events**: KeyDown/KeyUp via WebRTC data channel
2. **Text Composition Events**: UTF-8 text via event emitter

### Text Composition Event
```javascript
this.emit("TextComposition", {
    compositionText: "input_text",
    imeRecommendation: true
});
```

---

## 7. Data Channel Usage

- **Channel Name**: `input_channel_v1`
- **Ordered**: Yes
- **Reliable**: Yes

Keyboard events are always sent via reliable channel (no tolerance for dropped events).

---

## 8. Timestamp Format

Each keyboard event carries a microsecond-precision timestamp:

```rust
fn get_timestamp_us() -> u64 {
    let elapsed_us = session_start.elapsed().as_micros() as u64;
    unix_start_us.wrapping_add(elapsed_us)
}
```

---

## 9. Byte-Level Example

### KeyDown for Shift+A
```
Hex: 03 00 00 00 41 00 01 00 00 00 12 34 56 78 9A BC DE F0

[00-03] 03 00 00 00  = Type 3 (LE) = KeyDown
[04-05] 00 41        = Keycode 0x0041 (VK_A) (BE)
[06-07] 00 01        = Modifiers 0x0001 (SHIFT) (BE)
[08-09] 00 00        = Scancode 0x0000 (BE)
[10-17] 12 34 56 78 9A BC DE F0 = Timestamp (BE)
```

---

## 10. Comparison

| Feature | Web Client | Official Client | OpenNow |
|---------|-----------|-----------------|---------|
| Key Mapping | so.get()/Fo.get() maps | Platform-native | Rust match |
| Timestamp | Browser timeStamp | System clock | Session-relative |
| IME Support | Full composition | Basic | Not implemented |
| Key Repeat | DOM event.repeat | Manual filtering | Manual filtering |
| Scancode | Always 0x0000 | Always 0x0000 | Always 0x0000 |

---

## 11. Implementation Checklist

- [ ] Map event.code to Windows VK codes
- [ ] Extract modifier state (ctrl/alt/shift/meta)
- [ ] Skip events with event.repeat === true
- [ ] Create 18-byte binary message
- [ ] Include microsecond timestamp
- [ ] Set scancode to 0x0000
- [ ] Send via `input_channel_v1`
- [ ] Track pressed keys to avoid duplicates
- [ ] Release all keys on window focus loss
