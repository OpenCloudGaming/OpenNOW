# GeForce NOW Controller/Gamepad Input - Reverse Engineering Documentation

## 1. Controller Detection

### Web Client (Gamepad API)
```javascript
let gamepads = navigator.getGamepads();

// Event-driven detection
window.addEventListener("gamepadconnected", handler);
window.addEventListener("gamepaddisconnected", handler);
```

### Detection Priority
1. **PlayStation 4/5**: Vendor ID `054c`, 18+ buttons
2. **Xbox Controllers**: Device ID contains "Xbox" or "xinput"
3. **Nvidia Shield**: Shield ID check
4. **Standard Gamepad**: HID-compliant fallback
5. **Virtual Gamepad**: Software emulation

### Polling Rate
- Default: 4ms (250Hz)
- Configurable via URL: `?gamepadpoll=X`

---

## 2. Gamepad State Structure

### Standard Format (XInput-style)
```
[Offset] [Size] [Field]        [Type]
0x00     4B     Type            u32 LE (event type)
0x04     2B     Index           u16 BE (gamepad index 0-3)
0x06     2B     Bitmap          u16 BE (button state bitmask)
0x08     2B     Reserved        u16 BE
0x0A     2B     Buttons         u16 BE (button bitmask)
0x0C     2B     Trigger         u16 BE (combined analog triggers)
0x0E     4×2B   Axes[0-3]       4× i16 BE (left X/Y, right X/Y)
0x16     8B     CaptureTs       u64 BE (timestamp)
```

---

## 3. Button Mapping

### Standard Button IDs (Xbox Layout)
```
Index  Xbox Name   PlayStation   Physical Position
0      A           ○ (Circle)    Bottom/Right
1      B           ✕ (Cross)     Right
2      X           □ (Square)    Left
3      Y           △ (Triangle)  Top
4      LB          L1            Left Shoulder
5      RB          R1            Right Shoulder
6      LT          L2            Left Trigger (analog)
7      RT          R2            Right Trigger (analog)
8      Back        Select/Share  Left Center
9      Start       Options       Right Center
10     Left Stick  L3            Left Stick Click
11     Right Stick R3            Right Stick Click
12     Guide       PS Button     Center
13-15  [Reserved]  [Reserved]    [Reserved]
```

### Button Bitmap Encoding
- Bit 0: Y / △
- Bit 1: X / □
- Bit 2: A / ○
- Bit 3: B / ✕
- Bit 4-7: Shoulder buttons
- Bit 8-11: Start/Select/Sticks
- Bit 12: Guide button

---

## 4. Trigger Handling

### Packed Format (u16)
```
Low byte (0xFF):  Left Trigger (L2/LT) 0-255
High byte (0xFF): Right Trigger (R2/RT) 0-255

Example: 0xFF00 = LT fully pressed (255), RT released (0)
```

### Quantization
```javascript
let left_trigger = Math.round(255 * (axis_lt + 1) / 2);
let right_trigger = Math.round(255 * (axis_rt + 1) / 2);
let packed = (right_trigger << 8) | left_trigger;
```

---

## 5. Analog Stick Handling

### Axis Mapping
- Axis[0]: Left Stick X (-1.0 to 1.0)
- Axis[1]: Left Stick Y (-1.0 to 1.0, inverted)
- Axis[2]: Right Stick X (-1.0 to 1.0)
- Axis[3]: Right Stick Y (-1.0 to 1.0, inverted)

### Dead Zone
- Typical: 0.15 (15% of full range)
- Applied per-axis before quantization

### Quantization to i16
```javascript
if (Math.abs(axis_value) < 0.15) {
    axis_value = 0;  // Dead zone
}
let quantized = Math.round(axis_value * 32767);

// Special value for unchanged axes
if (axis_value === last_axis_value) {
    quantized = 2;  // "Unchanged" marker
}
```

---

## 6. Vibration/Rumble

### Dual-Rumble API
```javascript
if (gamepad.vibrationActuator?.type === "dual-rumble") {
    gamepad.vibrationActuator.playEffect("dual-rumble", {
        startDelay: 0,
        duration: milliseconds,
        strongMagnitude: 0.0-1.0,  // Left motor
        weakMagnitude: 0.0-1.0,    // Right motor
    });
}
```

### Stop Rumble
```javascript
gamepad.vibrationActuator.playEffect("dual-rumble", {
    duration: 0,
    strongMagnitude: 0,
    weakMagnitude: 0,
});
```

### Support Matrix
- Xbox Controllers: Full dual-rumble
- DualSense: Full dual-rumble
- DualShock 4: Limited (single motor emulated)
- Generic: Varies by device

---

## 7. Controller Type Identification

### Vendor IDs
```
Sony (DualShock/DualSense): 054c
Microsoft (Xbox):            045e
Nintendo (Switch):           057e
Nvidia (Shield):             Custom
Generic HID:                 Various
```

### Type Classification
| Type | Detected By | Button Mapper |
|------|-------------|---------------|
| Xbox Series | "Xbox" in ID | KA() |
| Xbox Wired | "Xbox" in ID | YA() |
| DualShock 4 | VID=054c, 18+ btns | NA() |
| DualSense | Device ID check | _A() |
| Shield | Shield ID check | GA() |
| Standard | Generic HID | NA() |

---

## 8. DualShock 4/DualSense Format

### Extended Format
```
[Offset] [Size] [Field]
...standard header...
0x0E     3B     ds4Btns[3]     Sony-specific buttons
0x11     2B     triggers[2]    L2/R2 analog (0-255)
0x13     4B     axes[4]        Analog (0-255, centered at 128)
```

---

## 9. Data Channel Configuration

### WebRTC Data Channel
- Name: `input_channel_v1` (reliable)
- Ordered: Yes
- Reliable: Yes
- Used for: Gamepad state updates

---

## 10. Handshake Protocol

### Server → Client
```
[0]: 0x0E (handshake marker)
[1]: Major version
[2]: Minor version
[3]: Flags
```

### Client → Server
Echo same bytes back to confirm ready state.

---

## 11. Protocol Versions

### Version 2 (Legacy)
- Direct event encoding
- No wrapper

### Version 3+ (Modern)
- Events wrapped with 0x22 prefix
```
[0]: 0x22 (wrapper marker)
[1...]: Event payload
```

---

## 12. Comparison

| Feature | Web Client | Official Client | OpenNow |
|---------|-----------|-----------------|---------|
| Input API | Gamepad API | HID (Raw Input) | HID + Gamepad |
| Controllers | 19 types | Static list | Mouse/KB only |
| Polling Rate | 4ms (250Hz) | Hardware interrupt | 4ms |
| Triggers | Packed u16 or u8 | Analog u8 | N/A |
| Vibration | Full dual-rumble | Full support | Not implemented |
| Dead Zone | 0.15 per-axis | Hardware-level | Mouse only |
| Multi-controller | Up to 4 | Up to 4 | Not supported |

---

## 13. Limitations

### Web Client
- Max 4 controllers (Gamepad API limit)
- Vibration not supported on all devices

### OpenNow
- Controller support not implemented
- Only mouse/keyboard input
- No rumble feedback

### Official Client
- VID-based detection may miss non-standard controllers
- 18+ button requirement excludes older gamepads

---

## 14. Implementation Notes

1. **Always initialize session timing** before first input
2. **Flush mouse events before button events**
3. **Use correct timestamp format** (microseconds)
4. **Implement 4ms coalescing** for consistency
5. **Handle protocol version negotiation**
6. **Test with multiple controller types**
7. **Reserve unused fields as zeros**
