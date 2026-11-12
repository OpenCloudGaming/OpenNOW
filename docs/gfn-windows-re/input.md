# GFN Windows client: keyboard, mouse, gamepad, haptics, cursor, clipboard

Evidence tags as in architecture.md: **S** static, **D** decompiled, **L** log-confirmed, **I** inferred, **U** unknown. Geronimo sends input through `BifrostSDKExecutor::sendNvstInputEvent` (`0x1800d2eb0`). Bifrost is now supplied; its control framing and channel policies are traced in [handshake.md](handshake.md). The event-specific conversion below is not yet fully traced.

## Summary

- Keyboard and mouse come from a dedicated **Raw Input thread** (`RawInputController`) with a hidden message-only window. Keyboard raw input is converted to SDL scancodes and then to a GFN-specific NVST key-code space, with left/right modifier bits. Lock-key state (Caps/Num/Scroll) is read with `GetKeyState` and sent as its own event. **D/L**
- The client tells the server its mouse settings via `nvbFeatureControl` (feature type 10, "accel=0, speed=10") and disables session mouse acceleration at start. **D/L**
- The cursor is drawn locally. The server sends system cursor IDs and bitmap cursors. On every cursor change the client tells the server to stop compositing the cursor (`nvbFeatureControl`, disable feature type 0). **L**
- Gamepads use SDL2 GameController with an XInput-style bitmap, plus Sony DS4/DS5 HID synthesis with cross-synthesis flags, plus **raw HID passthrough ("GSHID") for wheels and flight sticks** (Thrustmaster, Logitech, Saitek, Mad Catz VID:PIDs). **D/L**
- Clipboard is a server-driven request/response over NVST custom messages: the server sends a `PASTE` message to `GFN_CLIENT_UI`, the JS app answers, and the shell sends a `PASTE` custom message to recipient `UIPlugin`. **D/L**
- `Sending Input event failed: NVST_R_GENERIC_ERROR (0x800b0000)` is a Bifrost-side error. It clusters on focus and window-state changes and does not end the session. **L/I**

## 1. Keyboard

### 1.1 Capture — **D** `RawInputController`

- The async thread `FUN_18025d280` registers the class `AsyncWindowClass` and creates `AsyncWindow` with parent `HWND_MESSAGE` (`-3`). It calls `RegisterRawInputDevices` with two entries: usage page 1 / usage 6 (keyboard) and usage page 1 / usage 2 (mouse), both with flags `0x100` (`RIDEV_INPUTSINK`). Then it runs a `GetMessageA` loop. **L** `[RawInputController] Async Thread start` / `Async Thread End` (18 each), `Enable async input` (18).
- At thread start it reads `GetKeyState(VK_CAPITAL 0x14)`, `GetKeyState(VK_NUMLOCK 0x90)`, `GetKeyState(VK_SCROLL 0x91)`. It maps toggled → 2 and untoggled → 1, and emits a lock-key `NvstInputEvent` immediately.
- `FUN_18025ee60` (initMouse) snapshots `SDL_GetMouseState` and the SDL keyboard state (up to 0x200 scancodes) plus Caps/Num modifier bits before starting the thread. **I** This seeds the key-state table so already-held keys are not re-sent as new presses. **L** `initMouse: mask 0` / `mask 1`.
- The window procedure `FUN_18025cc00` handles `WM_INPUT` (0xff) through `GetRawInputData(RID_INPUT)`. Type 1 (keyboard) goes to `FUN_18025d6e0` and type 0 (mouse) to `FUN_18025dd50`. It then forwards the same `WM_INPUT` to the SDL window with `SendNotifyMessageA`.
- `FUN_18025d6e0` (keyboard):
  - Some raw keys are filtered (`Ignored Raw Keyboard: MakeCode=%04x Flags=%04x ... VKey=%04x`).
  - Others are converted from (MakeCode, VKey, E0 flag) to an **SDL scancode** (`FUN_18025f7e0`).
  - It synthesizes `SDL_KEYDOWN` (0x300) or `SDL_KEYUP` (0x301) and maintains SDL `KMOD` bits: LSHIFT 1, RSHIFT 2, LCTRL 0x40, RCTRL 0x80, LALT 0x100, RALT 0x200, LGUI 0x400, RGUI 0x800, MODE 0x4000. Caps (scancode 0x39) toggles 0x2000 and NumLock (0x53) toggles 0x1000.
  - A repeat flag is set when the key is already down. **I** Auto-repeat is forwarded to the server as repeat events rather than swallowed.

### 1.2 NVST key code — **D** `FUN_1800ee440` (`keyCodeMapper::getNvstKeyCode`)

- It maps the SDL scancode (< 0x200) through a table built once by `FUN_1800ee070`. On a miss: `GERONIMO_ERROR 0xc0020001 GERONIMO_IOINTERFACE_UNSUPPORTED_KEY` and `Unsupported key. Scancode: %d`.
- The table is a GFN-specific code space, not Windows VKs. Letters A-Z (scancodes 4-29) map to ASCII `0x41`-`0x5a`, and digits 1-0 (30-39) to `0x31`-`0x30`. Space maps to `0x20`, `-` to `0x2d`. Return maps to `0x104`, Escape to `0x100`, Backspace to `0x103`, Tab to `0x101`. F1-F12 (scancodes 58-69) map to `0x400`-`0x40b`.
- NVST modifier bits: Shift `0x01`, Ctrl `0x02`, Alt `0x04`, Meta `0x08`. Separate right-side bits: RShift `0x10`, RCtrl `0x20`, RAlt `0x40`, RGui `0x80`. Action is 2 for down and 1 for up.
- **U** The complete Bifrost key-code conversion before serialization is not traced. Geronimo uses its own key-code domain, while OpenNOW forwards Windows virtual keys. This is an unresolved boundary, not proof that one representation is accepted universally.

### 1.3 Keyboard layout

- **D** `GFNQueryHandler` reads `keyboardLayout` and `allowKeyboardLayoutChange` for `QUERY_GFN_START` and calls `platformKeyboardLayoutToGenericFormat` (`0x18015c170`). That function reads the OS layout (`GetKeyboardLayoutNameA`; `Failed to get the system keyboard layout.`), looks up a mapping (`Cannot find KB layout mapping for %s`), and logs `PlatformKeyboardLayout: %s, GenericFormat: %s`. If no layout is found, the query handler logs `Failed to get keyboard layout using OS language as the keyboard layout`.
- **D** `QUERY_GFN_SET_KEYBOARD_LAYOUT` (fields `kbLayout`, `allowKeyboardLayoutChange`) calls `GridApp::setKeyboardLayout(bool, string)` at `0x1401aea38`. `QUERY_GFN_REGISTER_ON_KEYBOARD_LAYOUT_CHANGED` wires `GridApp::onKeyboardLayoutChanged` (**S** `System keyboard layout has changed %s.`, `KeyboardLayoutChanged %s to %s`).
- **L** `debug.log` 19:45:11.634 `QUERY_GFN_START values, ... Keyboard Layout: en-US`; `geronimo.log` 19:45:31 `KBlayout:: upstreamready, SessionKBLayout: en-US, SystemKBLayout: en-US, ClientRequestedKBLayout: en-US, autoMode: true`. **D** `FUN_1800e83f0` compares the session, system, and requested layouts at upstream-ready and re-sends if they differ when `autoMode` is false.

### 1.4 Lock keys and cached input

- **D** `FUN_1800e5e00` (GIOInterface) flushes input events that were cached before upstream was ready. Lock-key events update the local lock state (`Updated lock keys with cached values: %d %d %d`). Then it re-sends gamepad state for indices 0-3 through `RIDeviceMediator`.
- **L** `geronimo.log.bak` 19:45:31 `Updated lock keys with cached values: 1 2 1` (Caps off, Num on, Scroll off), and `Caching input event type N` (18 times).
- **L** Custom messages sent before upstream-ready are queued too: `sendCustomMessage before onUpstreamReady. Queueing it.`, then `Sending 2 cached custom messages`.

### 1.5 Text and IME

- **S/D** `QUERY_GFN_SEND_UNICODE` (field `unicodeInput`; `unicodeInput field missing while Sending unicode input events.`) and `QUERY_GFN_REGISTER_ON_IME_WINDOW_OPENED/CLOSED`. Geronimo has `SDLWindow::setTextInputMode` and `setIMEPosition`, and imports `Imm*` (8 functions). **U** The NVST event type used for unicode text was not traced.

## 2. Mouse

### 2.1 Raw mouse — **D** `FUN_18025dd50`

- When the cursor mode field (`+0x60`) is 2 (relative/locked) and the raw report is not `MOUSE_MOVE_ABSOLUTE`, it sends a relative event with raw `lLastX`/`lLastY`. The NvstInputEvent type field is 2.
- Otherwise it calls `GetCursorPos` and `ScreenToClient` and sends an absolute position event.
- Buttons go through `FUN_18025da70`. `RI_MOUSE_WHEEL` (0x400) goes through `FUN_18025e190` with `usButtonData`. **I** `RI_MOUSE_HWHEEL` (0x800) is not tested in this function, so horizontal wheel probably comes from the SDL path or is unsupported.
- **S** `SDLEventProcessor::setMouseFrequencyCap(uint)` (`0x1801841a0`) stores a cap and pushes it to an `InputAggregator` (RTTI `InputAggregator`). **I** Relative mouse events are aggregated to a configurable rate. **U** The cap value was not found in logs.
- **L** `SDLProcessEvent: pollTime: 53284us` warnings (16 times) show the SDL event loop sometimes stalls for tens of ms.

### 2.2 Mouse settings sent to the server

- **D** `SDLEventProcessor::sendMouseSettings` (`0x180184040`):
  - Mode 1: logs the literal `Sending SDL mouse settings (accel=0, speed=10)`.
  - Mode 2: sends "alt" settings (`Sending alt mouse settings (accel=%d, speed=%d)`), or `Sending default alt mouse settings (accel=1, speed=10)` when none are stored.
  - It then calls `SDLWindow::sendMouseSettings(bool accel, int speed)`.
- **L** The result reaches Bifrost as `nvbFeatureControl` with `Update feature type: 10` (25 times). Example `geronimo.log` 19:59:33.213: `Sending SDL mouse settings (accel=0, speed=10)` → `nvbFeatureControl(...)` → `Update feature type: 10`. Alt settings were logged 12 times with `accel=0, speed=10`.
- **L** `debug.log` 19:45:11.634 `Disabling session mouse acceleration` inside `QUERY_GFN_START`. **D** `GridApp::setNVbSessionParams` (`0x1800cd100`) consults `GeronimoSettingsImpl::disableUnadjustedMovementForAppId(appId)` (remote-config list `disableUnadjustedMovementForAppIds`) and logs `useConfinedHiddenCursorInsteadOfLocked %d for appId: ...`.

### 2.3 Cursor capture and confinement — **D** `FUN_18017a9c0` (`SDLCursorManager::updateCursor`)

- It reads keyboard focus, fullscreen, and input mode. It toggles `SDL_SetRelativeMouseMode` when the lock state changes, and logs `Local cursor: %d. hasFocus: %d. fullscreen: %d, inputMode: %d, shouldLock: %d` (381 times).
- It warps the cursor to the scaled server position when the remote cursor becomes visible (`updateCursor: Warping cursor to window point {%d, %d}`, 31 times) and sets the mouse control mode (`Update mouse control mode to %u`: 0 ×37, 1 ×9, 2 ×31).
- **S** It also uses `SDLWindow::captureCursor`/`lockCursor`, `ClipCursor`, `SDL_WarpMouseInWindow`/`SDL_WarpMouseGlobal`. **L** `captureCursor( true ) attempted/succeeded`, `updateCursor: confining to window`.
- **I** Mode 0 is absolute with a visible local cursor, 1 is confined, and 2 is relative/locked. This matches the raw-input branch on mode 2.

## 3. Cursor channel

- **L** The server sends system cursor IDs with optional normalized positions: `[NVST:ServerControl] Server sent system cursor info with ID: 1 (0,0)`. Seen IDs: 0 (×30), 1 (×82), 2, 3, 5, 8, 9, 12. Position examples are `(32857,37617)` in the 0-65535 range.
- **L** Bitmap cursors are supported and rendered: `Server sent bitmap cursor info with ID: 1000, size: 4150` followed by `[SDLCursorManager] Receive new bitmap cursor, payload size: 4150`. **D** `FUN_18017afe0` caches bitmap cursors by ID (FNV-1a hash table), builds them with `SDL_CreateColorCursor`, and maps system IDs to SDL system cursors through `SDLMouseCursorDecoder::getCursor` (`Cannot find corresponding SDL cursor for the provided nvsc cursor id: %u.`). It scales the cursor by a `/65535.0` factor.
- **L** Remote cursor hiding: every cursor-info callback logs `[GridApp] Hiding remote on NVB_SN_CURSOR_INFO_CHANGED` followed by `nvbFeatureControl(...)` and `Disabled feature type: 0` (250 times). One `Enabled feature type: 0` was seen. **I** Feature type 0 is "server composites cursor into video", and the client re-asserts it off on each change.
- **S** `GridApp::onCursorInfoUpdate` (`0x1800c9d00`) and `SDLWindowManager::updateCursorInfo` are the entry points. `GeForceNOW.exe` imports `GridApp::onCursorInfoUpdate` and has `QUERY_OSC_SET_CURSOR_INFO` for the overlay.

## 4. Gamepads and HID

### 4.1 Standard path

- **S** SDL2 GameController (`SDL_GameControllerOpen`, `SDL_GameControllerGetTouchpadFinger`, `SDL_GameControllerRumble`, `SDL_GameControllerAddMappingsFromRW` with an embedded mapping DB). `Gamepad::buildGamepadBitmap` and `GridApp::sendGamepadBitmap` produce the connected-pad bitmap. `QUERY_GET_GAMEPAD_BITMAP`/`QUERY_GFN_GET_CONNECTED_GAMEPADS` expose it to JS.
- **L** `Streamer_InputDevice` telemetry: `"deviceName":"DualSense Wireless Controller","deviceType":"GAMEPAD","vendorId":"1356","productId":"3302"` (0x054C:0x0CE6).
- **L** `[NVST:ServerControl] Server sent Gamepad handling notification (RS = 0/1)` (90 times). **S** This relates to `GridApp::setGamepadRsdmm`, `RIDeviceMediator::enableGamepadRsdmmGesture`/`isRsdmmGestureSupported`, and `QUERY_GFN_TOGGLE_JOYSTICK_MOUSE` (`ToggleJoystickMouse enable:`). **U** The meaning of "RSDMM" is not expanded in any string. **I** It is a server-requested mode in which a gamepad stick drives the mouse.

### 4.2 Sony synthesis — **D/L**

- **D** `RIDeviceMediator::shouldSynthesizeSonyController(vid, pid)` (`0x1800b9670`) checks VID `0x054C`: DS4 PIDs `0x09CC`, `0x05C4`, `0x0BA0` and DS5 PIDs `0x0CE6`, `0x0DF2`. Each class has its own allow flags. Other VIDs fall into the "generic" path, governed by `allowCrossSynthGenericToDS4/DS5`.
- **L** Config at session start (19:45:11): `supportSynthesizedDS4: 1`, `supportSynthesizedDS5: 1`, `allowCrossSynthDS5ToDS4: 1`, `allowCrossSynthDS4ToDS5: 1`, `allowCrossSynthGenericToDS4: 0`, `allowCrossSynthGenericToDS5: 0`, `allowSynthesizedDS4: 0`. At 19:45:30: `Configuring server supported Hid devices: Supports DS4: yes, DS5: no (raw:4)`.
- **S** Remote-config keys: `synthesizedDS4AllGamesEnabled`, `synthesizedDS5AllGamesEnabled`, `synthesizedDS4FromDS5Disabled`, `synthesizedDS5FromDS4Disabled`, `synthesizedDS4FromGenericEnabled`, `synthesizedDS5FromGenericEnabled`, `synthesizedDS4Disabled`, `synthesizedDS5Disabled`, `synthesizeGamepadHid`.
- **S** `GamepadHIDSynthesizer::buildSynthesizedDS4State`, `parseDS4Report`, `parseGamepadHidOutput` (server-to-client HID output, i.e. rumble/LED for the synthesized pad). `GridApp::sendSynthesizedHidDevices`, `setSupportedHidTypes` (`QUERY_SET_SUPPORTED_HID_TYPES`).

### 4.3 Raw HID passthrough (GSHID) — **D/L**

- **D** `HIDDevicesController::initialize` (`0x1801b02c0`) loads `genericHidVidPidPairs` from remote config, merges a local `RIDevices.json` (`GSHID: added %zu vidPids from local RIDevices.json`), and initializes hidapi (`Unable to initialize hidapi library`). It imports `HidD_GetAttributes`/`HidP_GetCaps`.
- **L** `[GIOInterface] Enabling GSHID`, then the supported list from remote config: `{044F:0402} {044F:0404} {044F:B67C} {044F:B687} {044F:B68D} {044F:B68F} {044F:B697} {046D:C24F} {046D:C262} {046D:C266} {046D:C268} {046D:C26E} {046D:C272} {046D:C276} {06A3:075C} {06A3:0762} {0738:2221} {0738:A221}`. These are Thrustmaster (044F), Logitech (046D), Saitek (06A3), and Mad Catz (0738) vendors. **I** They are racing wheels and flight sticks.
- **S** `handleHidEvent`, `handleHidOutputEvent`, `handleHidChangeEvent(NvstHidControl_t, ...)`, and `HID dynamic change event: %04X:%04X:%04X, control: %d` show bidirectional raw reports (input up, output/feature down).

## 5. Haptics and rumble

- **L** Bifrost enables haptics on the server: `[NVST:NvscClientConfig] runtime.hapticsState: 0`, then `[NVST:RiClientBackend] Sending haptics state 1 to server` (19:45:31). At teardown: `SendHapticsState failed during teardown` (19:33:41).
- **D** `SDLGamepad::updateHapticState(bool)` (`0x1801a4f40`) logs `enable haptic state` / `disable haptic state` per pad. `IOInterface::updateHapticState` forwards to Bifrost. **S** `BifrostSDKExecutor::getCachedEnableGamepadHapticsSupport`, remote keys `hapticsSupported`, `hapticsFeedbackCount`.
- **D** `SDLGamepad::playHaptic` (`0x1801a4470`) and `simpleRumble` (`0x1801a4e00`) call `SDL_GameControllerRumble`. Rumble is rate-limited to one call per 33 ms (`now + 0x21`), but only for Microsoft VID `0x045E` with PIDs `0x02E0`, `0x02FD`, `0x0B05`, `0x0B13` (Xbox One S / Elite 2 / Series Bluetooth). Other pads get every update. **I** This works around Bluetooth Xbox rumble flooding.
- **S** `SDL_JOYSTICK_HIDAPI_PS4_RUMBLE`/`PS5_RUMBLE` hints are referenced, so Sony pads use SDL's HIDAPI rumble.

## 6. Clipboard and custom messages

- **D** `GFNQueryHandler` `QUERY_GFN_SEND_CLIPBOARD_UPDATE` (strings at `0x1401ad968`..`0x1401adb2f`: `Clipboard: Got clipboard update`, field `clipboardInfo`) calls `FUN_14018d470` (`simple_grid_app.cc`: `Sending custom message for clipboard update`). That function builds an `Nsk::CustomMessage` with messageType `"PASTE"`, recipient `"UIPlugin"` (inline constant `0x6e6967756c504955`), dataType `"application/json"`, and the JSON payload. It calls `GridApp::sendCustomMessage`, then `BifrostSDKExecutor::sendCustomMessage` (`0x1800d25d0`), then `nvbSendMessage`. On failure: `GERONIMO_GRIDAPP_SEND_CUSTOM_MESSAGE nvbSendMessage failed.`
- **L** Server-driven paste round trip at 19:57:01:
  1. `geronimo.log.bak` 19:57:01.595 `[BifrostClient: Streamer] NVST_MS_CUSTOM_MESSAGE: sizeInBytes: 327 messageType: PASTE dataType: application/json recipient: GFN_CLIENT_UI` (receive thread `0000973C`, the same thread that logs server cursor messages).
  2. `console.log` 19:57:01.609 `ClipboardService Sending clipboard data type CLIENT_DATA_RESPONSE`, then `Clipboard data sent true`, then `Sending event ClipboardPasteEvent`.
  3. Repeated at 19:57:11.
- **L** The JS app also pushes clipboard availability: `ClipboardService Sending clipboard data available type CLIENT_ADDED_DATA` (23) / `CLIENT_REMOVED_DATA` (27). Each produces `Clipboard: Got clipboard update` + `Sending custom message for clipboard update` in `debug.log` (235 total, including one at stream start, 19:45:31.013). Config: `clipboardConfig: {isClipboardEnabled: true, maxPasteBuffer: 65536, clipboardHintStringType: "keyboard"}`.
- **I** Protocol: (a) the client advertises "clipboard has data / no data" with PASTE custom messages to `UIPlugin`. (b) When the user pastes inside the cloud session, the server sends PASTE to `GFN_CLIENT_UI`. (c) The client replies with the data (`CLIENT_DATA_RESPONSE`). **U** The JSON schema of `clipboardInfo` was not captured (payload bodies are not logged).
- **S/D** Local clipboard access is not in the streaming path. `GeForceNOW.exe` `FUN_1400c7180` (`QUERY_WIN_COPY_TO_CLIPBOARD`, field `clipBoardData`) writes text with Win32 `OpenClipboard`/`SetClipboardData` (`Error in opening Clipboard.`). Geronimo imports `SDL_GetClipboardText`/`SDL_SetClipboardText`, but their only call sites are two small callbacks at `0x1801ee4f0`/`0x1801ee620`. These fetch the ImGui context first and free the SDL string into an ImGui buffer. **I** That is the ImGui SDL2 backend's clipboard hook for the debug overlay. Geronimo's own Win32 clipboard calls (`0x180405460`, `0x180418700`) match ImGui's default Win32 clipboard functions. **U** These two were not decompiled.
- **L** The dominant inbound custom message is `messageType: NvExtendedCommandMessage dataType: application/json recipient: Serenity`, about 9,700 lines across `geronimo.log` (4,202) and `.bak` (5,466), at 123-363 bytes. **S** `Serenity` is a client-side component in `GeForceNOW.exe` (`serenity.cc`, `serenity_win.cc`; `Serenity: onGfnPrepare called but Serenity not initialized`). Its other strings are about recording: `configuring serenity for record`, `serenity selected. target video codec H264`, `useSerenity`. **I** Serenity is the local capture/highlights recorder, and these messages are server-side game events driving it. **U** Payload meaning is not confirmed (bodies are not logged).

## 7. "Sending Input event failed"

- **S** The string is not in `Geronimo.dll`. A string search in the Ghidra project found no reference. It comes from Bifrost (`[BifrostClient: Streamer]` tag).
- **L** 42 occurrences (15 in `geronimo.log`, 27 in `.bak`), all `Nvsc Error: NVST_R_GENERIC_ERROR (0x800b0000)` on the main thread `00007488`. Every instance sits next to `nvbSendClientInfo` + `[NVST:ClientSession] Window State Change: 16/17/19 requested`, or right after `Sending SDL mouse settings` + `nvbFeatureControl` + `Update feature type: 10`, or after `initMouse`/`Async Thread End`. Example: 19:59:33.213 mouse settings → 19:59:33.248 `Sending Input event failed` → `Window State Change: 16`.
- **I** The failures may involve focus-transition events because they occur beside focus/window changes. The affected event type and rejection cause are **U**; co-occurrence does not prove either. No termination follows the 42 observed errors. OpenNOW's captured enqueue errors are terminal (`lib.rs:1907-1918`), but its asynchronous motion-write failures are nonterminal while discrete-write failures are terminal (`nvst.rs:6412-6456`). These observations do not justify dropping all input errors.

## 8. Touch and pen

- **S** `SDLWindow::touchEvent(SDL_TouchFingerEvent)`, `TOUCH_INPUT`, `touchSupport`, `Pen Pressure: %.1f` (ImGui demo), and `SDL_GameControllerGetTouchpadFinger` (DS4/DS5 touchpad) exist. **U** No touch events were logged. Whether Windows touchscreens are forwarded was not traced.

## 9. Comparison with OpenNOW (frozen dev `7ce3cd54`)

| Area | GFN Windows | OpenNOW |
| --- | --- | --- |
| Keyboard source | Raw Input keyboard + mouse on one message-only window thread (`RIDEV_INPUTSINK`) | Qt key events; set-1 scancode → US-position VK (`opennow-qt/src/streaming/StreamVideoItemInput.cpp:152-224`); Raw Input for mouse only (`windows_raw_input.rs:65-92`) |
| Key code | SDL scancode → GFN NVST key code; right-side modifier bits 0x10-0x80 | Windows VK + Shift/Ctrl/Alt/Meta bits; AltGr = 0x06 (`StreamVideoItemInput.cpp:442-451`) |
| Auto-repeat | Forwarded with a repeat flag | Swallowed (`StreamVideoItemInput.cpp:557-560`) |
| Lock keys | Sent at capture start and after upstream-ready | Encoder exists (type 19), no producer (`nvst_input.rs:803-808`) |
| Layout | OS layout → generic format; re-checked at upstream-ready | `keyboardLayout` from settings, default `en-US` (`native/opennow-core/src/language.rs`) |
| Mouse settings | `nvbFeatureControl` type 10 (accel=0, speed=10); session accel disabled; per-app unadjusted-movement list | `mouseMovementFlags:0` in CloudMatch; env-var sensitivity |
| Mouse rate | `InputAggregator` with frequency cap | No coalescing; drain every 250 µs |
| Cursor | System IDs + bitmap cursors rendered locally; server compositing disabled on every change | System IDs + `cursor_channel` PNGs; bitmap cursor on control 0x0110 not rendered (`nvst_input.rs:501-515`) |
| Gamepad HID | DS4/DS5 synthesis with DS4↔DS5 cross-synth flags; GSHID raw passthrough for wheels/HOTAS | DS4 synthesis only (DS5→DS4); no generic HID passthrough |
| Rumble | SDL rumble, 33 ms throttle for BT Xbox pads | Newest-per-controller coalescing; vibration-intensity scaling |
| Clipboard | Custom-message PASTE protocol (server-driven) | Ctrl+V types text as type-23 Unicode; custom-message channels never written |
| Input send failure | 42 generic errors while streaming continues; affected event types and failure semantics are unknown | Captured enqueue and asynchronous discrete-write errors are terminal; asynchronous motion-write errors are not. Synchronous errors return to the caller (`nvst.rs:6412-6456`, `lib.rs:1907-1918`) |
