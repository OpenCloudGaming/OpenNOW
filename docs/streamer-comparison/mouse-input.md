# Mouse input

## Overview

This is why aim feels different.

Official GFN and OpenNOW both use SDL plus a dedicated Windows Raw Input thread. Official then tells the host how to interpret those deltas. OpenNOW's capture-backed activation chain does not include that frame.

Every official focus change in `geronimo.log` sends `accel=0, speed=10`. Session start also logs `Disabling session mouse acceleration`. OpenNOW's proven activation chain sends enable-input, cursor capture `0x0308`, cursor tracking `0x030d`, and window state 19. The type 10 payload itself is not in any log or capture on this machine. See [features.md](features.md).

If you play FPS games with default sensitivity, official is sending raw HID counts into a host that was told accel off and speed 10. The on-wire bytes for that tell are not dumped anywhere here.

## Key concepts

**Type 7 vs type 5.** NVST remote input. Type 7 is relative `i16` dx/dy. Type 5 is absolute x/y plus viewport size. Official and OpenNOW use the same type numbers.

**Cursor ID 0.** Official system cursor ID 0 with coords (0,0) means lock, hide, relative. OpenNOW treats predefined message type 0 and cursor id 0 as `RemoteCursorState::Hidden` and enters SDL relative mode.

**Feature type 10.** Official NVB mouse settings. Logged as `Update feature type: 10` next to `accel=0, speed=10`. No hex in the logs. See [features.md](features.md).

**Feature type 8.** Track remote cursor image. OpenNOW `COMMAND_TRACK_REMOTE_CURSOR_IMAGE` `0x030d`. Official logs enable type 8 once the session is ready.

**`tune_relative_mouse`.** OpenNOW client-side scale and optional accel, applied only to type 7, at packet build time. Sensitivity default 1.0. Acceleration default 1.0, which is a no-op. The 150% setting on `dx=100` becomes 160. `OnceLock` means the first env read wins until process restart.

## How OpenNOW captures

Qt always launches with `OPENNOW_NATIVE_INPUT_OWNER=native` and `OPENNOW_NATIVE_EXTERNAL_RENDERER=1`.

Windows starts `WindowsRawInputController` on thread `opennow-raw-input`, priority above normal, `HWND_MESSAGE` class `OpenNOWRawInput`.

Register:

```
RAWINPUTDEVICE {
    usUsagePage: 0x01,
    usUsage: 0x02,
    dwFlags: RIDEV_INPUTSINK,
    hwndTarget: message_hwnd
}
```

`RIDEV_INPUTSINK` delivers `WM_INPUT` even when the SDL window is not focused. The handler then drops the event unless `GetForegroundWindow()` is the SDL HWND and capture is enabled.

Relative motion is used only when `relative_motion` is on and `MOUSE_MOVE_ABSOLUTE` is clear. `lLastX` / `lLastY` are i32. Values outside i16 are split into several `MouseMove` samples. Buttons 1–5 and vertical wheel come from Raw Input in both cursor modes. There is no `RI_MOUSE_HWHEEL`.

SDL still owns grab, hide, and absolute coordinates. Hints:

- `SDL_MOUSE_RELATIVE_MODE_WARP=0`
- `SDL_MOUSE_RELATIVE_SCALING=0`

After SDL toggles relative mode, OpenNOW posts `WM_APP+1` and re-registers the mouse onto the message window so SDL does not keep the deltas.

Hidden cursor, type 0 id 0, plus focus. Grab on, relative mode on, cursor hidden. Raw Input sends type 7.

Visible cursor. Grab off. SDL sends type 5, letterboxed into the stream viewport. Consecutive absolute samples replace the previous one. Relative samples are not merged.

Queue capacity 256. Host pump and NVST drain both use 250 µs. Drain takes up to 32 samples per tick. Motion uses unordered PR-SCTP, 300 ms lifetime, SID 6. Buttons and wheel use reliable control.

There is no `ClipCursor`, `SetCursorPos`, `SPI_GETMOUSE`, `RIDEV_NOLEGACY`, or `RIDEV_CAPTUREMOUSE` in this crate.

## How official GFN captures

From `geronimo.log` and `debug.log`:

1. Mall. `Using native input controller`. 858 SDL gamepad mappings. `captureCursor` toggles on UI clicks. No Raw Input thread yet.
2. Session start. `Updating SDL Event Processor to use raw events: 1`. `Disabling session mouse acceleration`.
3. Focus plus fullscreen. `RawInputController initMouse` mask 0 or 1. Async thread start. `Enable async input`. `Sending alt mouse settings (accel=0, speed=10)`.
4. First host apply can fail on timing. `Failed to update mouse sensitivity: 10 NVST_R_SUCCESS` once, before NVST was ready.
5. Server system cursor ID 0 → mode 2, local cursor 0, `shouldLock: 1`.
6. Other system cursor IDs → mode 1, visible, warp to `coord / 65535 * windowSize`. Example. 32524, 33017 on 2560×1440 → 1270, 725.
7. Unfocus. Async thread end. `Sending SDL mouse settings (accel=0, speed=10)`. Input send often fails.
8. `inputMode` stayed 0 for this dataset. Modes 0, 1, 2 are a separate `SDLCursorManager` enum.

`sharedstorage.json` has no mouse sensitivity or accel keys. Official i18n has no mouse feel slider. Feel is hardcoded plus server cursor mode.

## Settings

| Key | OpenNOW | Official |
|---|---|---|
| Host accel / speed | No captured frame | Logged as `0` / `10` on focus and blur. Bytes not dumped |
| Session accel disable | Never logged | `Disabling session mouse acceleration` at start |
| `mouseSensitivity` | Qt env, 0.1–3, default 1. Console UI only | Not persisted |
| `mouseAcceleration` | Env 1–150, default 1. No Qt slider | Not persisted |
| `mouseMovementFlags` | Hardcoded 0 | Not in `customProfile` |
| `nativeCursorOverlay` | Local show/hide | Official hides remote and draws local SDL cursor |
| `relativeMouseInput` | Acceptance attestation only | Not a setting |

Desktop Qt copy says "Acceleration off · raw input". The streamer can still apply software accel if `mouseAcceleration > 1`.

## Packet

Native HID, little-endian type:

| Type | Size | Body |
|---|---|---|
| 5 absolute | 26 | x, y BE u16, reserved, width, height, reserved, timestamp |
| 7 relative | 22 | dx, dy BE i16, 6 zeros, timestamp |
| 8 / 9 button | 18 | button 1–5, timestamp |
| 10 wheel | 22 | horiz 0, vert i16, timestamp |

NVST wrap is `COMMAND_REMOTE_INPUT` `0x0206`. Absolute encode injects flags `0x08 0x00`. Relative does not. Wheel has no horizontal component.

## Why it feels different

1. **Host curve.** Official disables session accel and pins speed 10 on every focus change. The type 10 frame itself has not been captured. This is still the first thing to match if you want official look.
2. **Client curve.** OpenNOW can scale type 7 before send. Official logs imply raw counts.
3. **i16 split.** A large HID burst becomes several type-7 packets. Official may keep a wider delta.
4. **Foreground filter.** The embedded Windows capture binds Raw Input ownership to the Qt window and pauses capture while an overlay owns input. Official's async thread is also focus-scoped, but it restarts on window state 19 and resends alternate settings.
5. **Binary cursor mode.** Official has visible-unlocked, visible-warped, and hidden-locked. OpenNOW has hidden versus visible. Custom bitmap id 0 stays visible.
6. **Qt owns pointer state** while the embedded Raw Input controller owns relative motion. Official may keep lock inside `RawInputController`.
7. **Drain stalls.** Encode and SCTP share the NVST event thread. A hitch batches then bursts. The 300 ms unordered lifetime exists because ordered PR-SCTP already felt like delayed catch-up.

## Where things live

- Raw Input. `native/opennow-streamer/crates/opennow-streamer-platform/src/windows_raw_input.rs`
- Embedded capture ownership. `native/opennow-streamer/crates/opennow-streamer-platform/src/embedded_input.rs`
- Tuning and packet. `native/opennow-streamer/crates/opennow-streamer-core/src/lib.rs` `tune_relative_mouse`, `captured_input_packet`
- NVST encode and channels. `native/opennow-streamer/crates/opennow-streamer-transport/src/nvst_input.rs`
- Qt input bridge. `opennow-qt/src/NativeStreamRuntime.cpp`

## Gotchas

The Qt path has no SDL presenter HWND. The shell passes its own native window handle to the embedded capture controller and submits Qt/controller events directly through the FFI.

Absolute `MOUSE_MOVE_ABSOLUTE` reports never become type 5. Tablets and some virtual mice produce no relative motion.

Linux XI2 can emit buttons twice because SDL still owns buttons. macOS has no dedicated raw-mouse thread.
