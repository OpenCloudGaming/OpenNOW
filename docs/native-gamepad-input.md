# Native gamepad input

Qt's `ControllerInput` discovers SDL3 gamepads and sends typed snapshots through
`NativeStreamRuntime` and the streamer FFI. The native core already serializes the
complete 38-byte gamepad event. The NVST transport must preserve that event rather
than reconstructing it from a captured idle packet.

## Wire contract

Command `0x020d` carries both controller registration and controller state. The
command header is a little-endian u16 command followed by a little-endian u16
payload length.

Gameplay state uses the input partially reliable channel with this 54-byte payload:

| Field | Encoding |
| --- | --- |
| Versioned envelope | `0x23` |
| Capture timestamp | u64 big-endian microseconds |
| Partially reliable marker | `0x26` |
| Controller slot | u8, 0 through 3 |
| Per-controller sequence | u16 big-endian, wrapping |
| Length-prefixed event marker | `0x21` |
| Event length | u16 big-endian, 38 |
| Gamepad event | The original 38 bytes |

The event contains the controller ID at bytes 6–7, the connected bitmap at bytes
8–9, and the full capture timestamp at bytes 30–37, all little-endian. Bitmap bit
`i` indicates a connected controller; bit `i + 8` identifies it as XInput-style.
One controller in slot zero is `0x0101`, not `3`. Buttons, triggers, sticks, and
the event's fixed fields remain unchanged in transport.

Registration uses the reliable control channel and the same versioned timestamp
prefix, followed by `0x22` and a neutral 38-byte event. Its bitmap must match the
state events. Activation announces an empty bitmap; the first actual snapshot
announces the connected controllers. A topology change sends a new registration,
including an empty bitmap when the last controller leaves.

Before removing controllers from the bitmap, transport sends their neutral states
with the old bitmap on the reliable control channel. These use the same `0x22`
single-event framing as registration, so releases and removal are ordered. Removed
slots reset their partially reliable sequence. A failed channel write stops the
batch and restores the codec checkpoint, allowing the next snapshot to retry the
registration instead of permanently treating it as delivered. A new transport
session starts with fresh registration and sequence state.

## Reference and regression checks

The framing, bitmap interpretation, and sequence width were compared against
OpenNOW-Mac commit `90627114383501dd18ef165baa005d9ea603fdf3`:

- [`NvstGamepadPacket.swift`](https://github.com/OpenCloudGaming/OpenNOW-Mac/blob/90627114383501dd18ef165baa005d9ea603fdf3/GFN/NVST/BifrostFree/NvstGamepadPacket.swift)
  documents the corrected length-prefixed encoding and captured event fields.
- [`NvstInputActivation.swift`](https://github.com/OpenCloudGaming/OpenNOW-Mac/blob/90627114383501dd18ef165baa005d9ea603fdf3/GFN/NVST/BifrostFree/NvstInputActivation.swift)
  documents matching registration and state bitmaps.
- [`NvstFeedbackReportTests.swift`](https://github.com/OpenCloudGaming/OpenNOW-Mac/blob/90627114383501dd18ef165baa005d9ea603fdf3/Tests/GFN/NVST/NvstFeedbackReportTests.swift)
  compares gamepad bytes with the previously working encoder.

Run the focused checks with:

```sh
cargo test --manifest-path native/opennow-streamer/Cargo.toml \
  -p opennow-streamer-transport nvst_input
```

Tests cover exact framing, all four slots, unchanged analog/button/timestamp fields,
sequence rollover beyond 255 and 65535, matching topology announcements, ordered
disconnect releases, reconnects, and registration replay after rollback or restart.

Live acceptance still requires an authenticated GFN session and physical controllers:
check a controller connected before launch, hotplug during gameplay, disconnect
while holding a button, reconnect, and a second controller. Repeat with windowed
and fullscreen presentation and with stream overlays open and closed; overlays
must consume local input without stopping media or leaving remote buttons held.
