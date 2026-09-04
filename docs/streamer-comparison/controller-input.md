# Controller input

## Overview

Both clients treat a pad as an XInput-shaped 38-byte type-12 packet and use SDL to open devices. Official GFN on this machine opened an Xbox One Controller through `SDLGamepad` (VID 045E, PID 02FF) and sent haptics as NVB feature type 6. OpenNOW native uses SDL2 GameController events, a 15% radial deadzone, four slots, and a 100 ms keepalive. It never applies rumble.

The wire is close. The extras are not. Gyro, DualSense touchpad, Guide-to-game, and NVST player index are missing or hardcoded on the native path.

## Key concepts

**Type 12.** OpenNOW native and official GFN share this layout. 38 bytes, little-endian. Type 12, payload 26, slot, bitmap, buttons, triggers, sticks, magic 85, timestamp.

**15% radial deadzone.** OpenNOW `radial_deadzone` in `output.rs`. Sticks inside the circle become zero. Outside is scaled back to full range. Y is inverted to XInput.

**Keepalive.** Stop sending and GFN games revert to keyboard and mouse prompts. OpenNOW resends the last state every 100 ms.

**`SDLGamepad` vs XInput API.** Official logs have no `XInput` string. Path is SDL plus `RIDeviceMediator` plus `GSHID`. OpenNOW also has no `XInputGetState`. Linux `linux_xinput.rs` is X11 XInput2 for mouse, not Xbox.

## How OpenNOW captures

`SdlInputCapture` is the only native pad owner. `sdl.game_controller()`, then `is_game_controller(index)` and `open()`. Unmapped joysticks are ignored. First-come slots 0–3. A fifth pad is dropped.

Events. `ControllerDeviceAdded/Removed`, `ControllerButtonDown/Up`, `ControllerAxisMotion`. Not a fixed-Hz XInput poll.

Guide becomes `CapturedInput::Guide` and stays local. Overlay, not the game.

SDL `Misc1`, `Touchpad`, and `Paddle1–4` are dropped.

Bitmap. Native always sets both connected and Xbox bits for every slot.

`enableGyroscopeControls` is stored in settings and never read by the streamer.

## How the packet is sent

NVST. First gamepad also sends `device_descriptor(..., 3)` on control-reliable. Updates go `InputPartial` as `COMMAND_GAMEPAD` `0x020d`. The type-12 payload is sent as raw 38 bytes.

`gamepad_command()` copies buttons, triggers, and sticks from the type-12 packet. It does not copy `controller_id` or the connected bitmap. The NVST body hardcodes inner index `0x03`. Multi-pad identity is not on that wire even though RTSP advertises `useMultipleGamepads:1`.

`INPUT_HAPTICS_ENABLED` is dropped. There is no `set_rumble` and no inbound rumble decode.

## How official GFN captures

From this machine's logs:

- Startup. `Creating Gamepad`. `SDLGamepad: Loaded 858 gamepad mappings`. `Enabling GSHID`. DualShock synth allowed. Generic-to-DS synth off.
- Session request. `availableSupportedControllers: [2]`, `preferredController: 2`. Same values OpenNOW sends.
- Mid-session connect. `Opened controller ... 45e / 2ff`. `handleNewGamepad: standard gamepad at effective ID: 0`.
- Haptics. `enable haptic state` → `Sending haptics state 1 to server` → feature type 6. Telemetry `hapticsSupported: TRUE`, `hapticsFeedbackCount: 0` this session.
- Server `Gamepad handling notification (RS = 0|1)`. 122 times 0, 38 times 1. `RS` is not named in the log.
- Mall `GamepadNavigationService` synthesizes keyboard events. It stops when the stream starts.
- `handleGamepadInputInNative: true` appears under steamOSConfig, not the GFN-PC client block.

`synthesizeGamepadHid: true` is in remote config. OpenNOW has no GSHID synthesizer.

## Settings

| Setting | OpenNOW effect | Official |
|---|---|---|
| `controllerMode` / `launchInConsoleMode` | Launch `appLaunchMode = gamepadFriendly` | `preferGamepadFriendlyAppLaunchMode` |
| `enableGyroscopeControls` | Stored, unused | Unknown from these logs |
| `steamControllerCompatibilityMode` | Stored, unused on this path | Not in this persist |
| Mouse sensitivity | Mouse only | No pad analog |

Qt `ControllerInput` is a separate SDL3 poller for shell navigation. It is gated off on the stream route. Stick-as-dpad thresholds 18000/12000 are not the 15% radial deadzone.

Two SDL runtimes can hold the same physical pad. Qt 3 for the shell, streamer 2 for HID.

## Who owns pads

Qt launches with `OPENNOW_NATIVE_INPUT_OWNER=native`. SDL events become type 12.

## Where things live

- Capture and deadzone. `native/opennow-streamer/crates/opennow-streamer-platform/src/output.rs`
- Type-12 build. `native/opennow-streamer/crates/opennow-streamer-core/src/lib.rs` `captured_input_packet`
- NVST encode. `native/opennow-streamer/crates/opennow-streamer-transport/src/nvst_input.rs` `gamepad_command`
- Qt shell only. `opennow-qt/src/ControllerInput.cpp`

## Gotchas

Keepalive is load-bearing. A quiet pad is not "nothing to send".

Native Guide is local-only. Games that expect the Xbox Guide chord on the wire will not see it.

NVST multi-pad SDP flags exist. The encoder still hardcodes player identity.

`enableGyroscopeControls` is a dead setting in both UIs.
