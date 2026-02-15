export const INPUT_HEARTBEAT = 2;
export const INPUT_KEY_DOWN = 3;
export const INPUT_KEY_UP = 4;
export const INPUT_MOUSE_REL = 7;
export const INPUT_MOUSE_BUTTON_DOWN = 8;
export const INPUT_MOUSE_BUTTON_UP = 9;
export const INPUT_MOUSE_WHEEL = 10;
export const INPUT_GAMEPAD = 12;

// Mouse button constants (1-based for GFN protocol)
// GFN uses: 1=Left, 2=Middle, 3=Right, 4=Back, 5=Forward
export const MOUSE_LEFT = 1;
export const MOUSE_MIDDLE = 2;
export const MOUSE_RIGHT = 3;
export const MOUSE_BACK = 4;
export const MOUSE_FORWARD = 5;

// XInput button flags (matching Windows XINPUT_GAMEPAD_* constants)
export const GAMEPAD_DPAD_UP = 0x0001;
export const GAMEPAD_DPAD_DOWN = 0x0002;
export const GAMEPAD_DPAD_LEFT = 0x0004;
export const GAMEPAD_DPAD_RIGHT = 0x0008;
export const GAMEPAD_START = 0x0010;
export const GAMEPAD_BACK = 0x0020;
export const GAMEPAD_LS = 0x0040; // Left stick click (L3)
export const GAMEPAD_RS = 0x0080; // Right stick click (R3)
export const GAMEPAD_LB = 0x0100; // Left bumper
export const GAMEPAD_RB = 0x0200; // Right bumper
export const GAMEPAD_GUIDE = 0x0400; // Xbox/Guide button
export const GAMEPAD_A = 0x1000;
export const GAMEPAD_B = 0x2000;
export const GAMEPAD_X = 0x4000;
export const GAMEPAD_Y = 0x8000;

// Axis indices for gamepad
export const GAMEPAD_AXIS_LX = 0; // Left stick X
export const GAMEPAD_AXIS_LY = 1; // Left stick Y
export const GAMEPAD_AXIS_RX = 2; // Right stick X
export const GAMEPAD_AXIS_RY = 3; // Right stick Y
export const GAMEPAD_AXIS_LT = 4; // Left trigger
export const GAMEPAD_AXIS_RT = 5; // Right trigger

// Gamepad constants
export const GAMEPAD_MAX_CONTROLLERS = 4;
export const GAMEPAD_PACKET_SIZE = 38;
export const GAMEPAD_DEADZONE = 0.15; // 15% radial deadzone

export interface KeyboardPayload {
  keycode: number;
  scancode: number;
  modifiers: number;
  timestampUs: bigint;
}

export interface MouseMovePayload {
  dx: number;
  dy: number;
  timestampUs: bigint;
}

export interface MouseButtonPayload {
  button: number;
  timestampUs: bigint;
}

export interface MouseWheelPayload {
  delta: number;
  timestampUs: bigint;
}

export interface GamepadInput {
  controllerId: number; // 0-3
  buttons: number; // 16-bit button flags
  leftTrigger: number; // 0-255
  rightTrigger: number; // 0-255
  leftStickX: number; // -32768 to 32767
  leftStickY: number; // -32768 to 32767 (inverted in XInput)
  rightStickX: number; // -32768 to 32767
  rightStickY: number; // -32768 to 32767 (inverted in XInput)
  connected: boolean; // true = connected, false = disconnected
  timestampUs: bigint;
}

const codeMap: Record<string, { vk: number; scancode: number }> = {
  // Letters
  KeyA: { vk: 0x41, scancode: 0x04 },
  KeyB: { vk: 0x42, scancode: 0x05 },
  KeyC: { vk: 0x43, scancode: 0x06 },
  KeyD: { vk: 0x44, scancode: 0x07 },
  KeyE: { vk: 0x45, scancode: 0x08 },
  KeyF: { vk: 0x46, scancode: 0x09 },
  KeyG: { vk: 0x47, scancode: 0x0a },
  KeyH: { vk: 0x48, scancode: 0x0b },
  KeyI: { vk: 0x49, scancode: 0x0c },
  KeyJ: { vk: 0x4a, scancode: 0x0d },
  KeyK: { vk: 0x4b, scancode: 0x0e },
  KeyL: { vk: 0x4c, scancode: 0x0f },
  KeyM: { vk: 0x4d, scancode: 0x10 },
  KeyN: { vk: 0x4e, scancode: 0x11 },
  KeyO: { vk: 0x4f, scancode: 0x12 },
  KeyP: { vk: 0x50, scancode: 0x13 },
  KeyQ: { vk: 0x51, scancode: 0x14 },
  KeyR: { vk: 0x52, scancode: 0x15 },
  KeyS: { vk: 0x53, scancode: 0x16 },
  KeyT: { vk: 0x54, scancode: 0x17 },
  KeyU: { vk: 0x55, scancode: 0x18 },
  KeyV: { vk: 0x56, scancode: 0x19 },
  KeyW: { vk: 0x57, scancode: 0x1a },
  KeyX: { vk: 0x58, scancode: 0x1b },
  KeyY: { vk: 0x59, scancode: 0x1c },
  KeyZ: { vk: 0x5a, scancode: 0x1d },
  // Numbers
  Digit1: { vk: 0x31, scancode: 0x1e },
  Digit2: { vk: 0x32, scancode: 0x1f },
  Digit3: { vk: 0x33, scancode: 0x20 },
  Digit4: { vk: 0x34, scancode: 0x21 },
  Digit5: { vk: 0x35, scancode: 0x22 },
  Digit6: { vk: 0x36, scancode: 0x23 },
  Digit7: { vk: 0x37, scancode: 0x24 },
  Digit8: { vk: 0x38, scancode: 0x25 },
  Digit9: { vk: 0x39, scancode: 0x26 },
  Digit0: { vk: 0x30, scancode: 0x27 },
  // Special keys
  Enter: { vk: 0x0d, scancode: 0x28 },
  Escape: { vk: 0x1b, scancode: 0x29 },
  Backspace: { vk: 0x08, scancode: 0x2a },
  Tab: { vk: 0x09, scancode: 0x2b },
  Space: { vk: 0x20, scancode: 0x2c },
  // Punctuation
  Minus: { vk: 0xbd, scancode: 0x2d },
  Equal: { vk: 0xbb, scancode: 0x2e },
  BracketLeft: { vk: 0xdb, scancode: 0x2f },
  BracketRight: { vk: 0xdd, scancode: 0x30 },
  Backslash: { vk: 0xdc, scancode: 0x31 },
  Semicolon: { vk: 0xba, scancode: 0x33 },
  Quote: { vk: 0xde, scancode: 0x34 },
  Backquote: { vk: 0xc0, scancode: 0x35 },
  Comma: { vk: 0xbc, scancode: 0x36 },
  Period: { vk: 0xbe, scancode: 0x37 },
  Slash: { vk: 0xbf, scancode: 0x38 },
  // Function keys
  F1: { vk: 0x70, scancode: 0x3a },
  F2: { vk: 0x71, scancode: 0x3b },
  F3: { vk: 0x72, scancode: 0x3c },
  F4: { vk: 0x73, scancode: 0x3d },
  F5: { vk: 0x74, scancode: 0x3e },
  F6: { vk: 0x75, scancode: 0x3f },
  F7: { vk: 0x76, scancode: 0x40 },
  F8: { vk: 0x77, scancode: 0x41 },
  F9: { vk: 0x78, scancode: 0x42 },
  F10: { vk: 0x79, scancode: 0x43 },
  F11: { vk: 0x7a, scancode: 0x44 },
  F12: { vk: 0x7b, scancode: 0x45 },
  F13: { vk: 0x7c, scancode: 0x64 },
  // Navigation keys
  ArrowRight: { vk: 0x27, scancode: 0x4f },
  ArrowLeft: { vk: 0x25, scancode: 0x50 },
  ArrowDown: { vk: 0x28, scancode: 0x51 },
  ArrowUp: { vk: 0x26, scancode: 0x52 },
  // Modifier keys
  ControlLeft: { vk: 0xa2, scancode: 0xe0 },
  ShiftLeft: { vk: 0xa0, scancode: 0xe1 },
  AltLeft: { vk: 0xa4, scancode: 0xe2 },
  MetaLeft: { vk: 0x5b, scancode: 0xe3 },
  ControlRight: { vk: 0xa3, scancode: 0xe4 },
  ShiftRight: { vk: 0xa1, scancode: 0xe5 },
  AltRight: { vk: 0xa5, scancode: 0xe6 },
  MetaRight: { vk: 0x5c, scancode: 0xe7 },
  // Caps Lock and Num Lock
  CapsLock: { vk: 0x14, scancode: 0x39 },
  NumLock: { vk: 0x90, scancode: 0x53 },
  // Navigation cluster
  Insert: { vk: 0x2d, scancode: 0x49 },
  Delete: { vk: 0x2e, scancode: 0x4c },
  Home: { vk: 0x24, scancode: 0x4a },
  End: { vk: 0x23, scancode: 0x4d },
  PageUp: { vk: 0x21, scancode: 0x4b },
  PageDown: { vk: 0x22, scancode: 0x4e },
  // System keys
  PrintScreen: { vk: 0x2c, scancode: 0x46 },
  ScrollLock: { vk: 0x91, scancode: 0x47 },
  Pause: { vk: 0x13, scancode: 0x48 },
  // Context Menu key
  ContextMenu: { vk: 0x5d, scancode: 0x65 },
  // Numpad keys
  Numpad0: { vk: 0x60, scancode: 0x62 },
  Numpad1: { vk: 0x61, scancode: 0x59 },
  Numpad2: { vk: 0x62, scancode: 0x5a },
  Numpad3: { vk: 0x63, scancode: 0x5b },
  Numpad4: { vk: 0x64, scancode: 0x5c },
  Numpad5: { vk: 0x65, scancode: 0x5d },
  Numpad6: { vk: 0x66, scancode: 0x5e },
  Numpad7: { vk: 0x67, scancode: 0x5f },
  Numpad8: { vk: 0x68, scancode: 0x60 },
  Numpad9: { vk: 0x69, scancode: 0x61 },
  NumpadAdd: { vk: 0x6b, scancode: 0x57 },
  NumpadSubtract: { vk: 0x6d, scancode: 0x56 },
  NumpadMultiply: { vk: 0x6a, scancode: 0x55 },
  NumpadDivide: { vk: 0x6f, scancode: 0x54 },
  NumpadDecimal: { vk: 0x6e, scancode: 0x63 },
  NumpadEnter: { vk: 0x0d, scancode: 0x58 },
};

const keyFallbackMap: Record<string, { vk: number; scancode: number }> = {
  Escape: { vk: 0x1b, scancode: 0x29 },
  Esc: { vk: 0x1b, scancode: 0x29 },
};

/**
 * Write an 8-byte big-endian timestamp (performance.now() * 1000 = microseconds)
 * into a DataView at the given offset. Matches official GFN client's _r() function.
 */
function writeTimestamp(view: DataView, offset: number): void {
  const tsUs = performance.now() * 1000;
  const lo = Math.floor(tsUs) & 0xFFFFFFFF;
  const hi = Math.floor(tsUs / 4294967296);
  view.setUint32(offset, hi, false);     // high 32 bits, big-endian
  view.setUint32(offset + 4, lo, false); // low 32 bits, big-endian
}

/**
 * Protocol v3+ wrapper for SINGLE non-mouse events (keyboard, mouse button, wheel).
 * Format: [0x23][8B timestamp][0x22][payload]
 *
 * 0x23 = outer timestamp wrapper (added by yc() in official client)
 * 0x22 = single-event sub-message marker (added by Ec() allocator in official client)
 *
 * For protocol v1-v2, returns the raw payload unchanged.
 */
function wrapSingleEvent(payload: Uint8Array, protocolVersion: number): Uint8Array {
  if (protocolVersion <= 2) {
    return payload;
  }
  // [0x23][8B timestamp][0x22][payload]
  const wrapped = new Uint8Array(9 + 1 + payload.length);
  const view = new DataView(wrapped.buffer);
  wrapped[0] = 0x23;
  writeTimestamp(view, 1);
  wrapped[9] = 0x22;  // single-event sub-message marker
  wrapped.set(payload, 10);
  return wrapped;
}

/**
 * Protocol v3+ wrapper for MOUSE MOVE events.
 * Format: [0x23][8B timestamp][0x21][2B event-length][payload]
 *
 * 0x23 = outer timestamp wrapper
 * 0x21 = mouse/cursor event marker (used by Tc() coalescer in official client)
 * 2B   = payload length (BE uint16) — official client's Wa() with no endian param = BE
 *
 * For protocol v1-v2, returns the raw payload unchanged.
 */
function wrapMouseMoveEvent(payload: Uint8Array, protocolVersion: number): Uint8Array {
  if (protocolVersion <= 2) {
    return payload;
  }
  // [0x23][8B timestamp][0x21][2B length][payload]
  const wrapped = new Uint8Array(9 + 1 + 2 + payload.length);
  const view = new DataView(wrapped.buffer);
  wrapped[0] = 0x23;
  writeTimestamp(view, 1);
  wrapped[9] = 0x21;  // mouse/cursor event marker
  view.setUint16(10, payload.length, false);  // event length (BE, matches official setUint16)
  wrapped.set(payload, 12);
  return wrapped;
}

/**
 * Protocol v3+ wrapper for GAMEPAD events on the RELIABLE channel.
 * Format: [0x23][8B timestamp][0x21][2B size BE][payload]
 *
 * Official GFN client's ul() with m=false writes [0x21][2B size] then yc() prepends [0x23][8B ts].
 * Gamepad goes through the same batching system as other events.
 *
 * For protocol v1-v2, returns the raw payload unchanged.
 */
function wrapGamepadReliable(payload: Uint8Array, protocolVersion: number): Uint8Array {
  if (protocolVersion <= 2) {
    return payload;
  }
  // [0x23][8B timestamp][0x21][2B size][payload]
  const wrapped = new Uint8Array(9 + 1 + 2 + payload.length);
  const view = new DataView(wrapped.buffer);
  wrapped[0] = 0x23;
  writeTimestamp(view, 1);
  wrapped[9] = 0x21;  // batched event marker (m=false path in ul())
  view.setUint16(10, payload.length, false);  // size (BE, Wa() with no endian param)
  wrapped.set(payload, 12);
  return wrapped;
}

/**
 * Protocol v3+ wrapper for GAMEPAD events on the PARTIALLY RELIABLE channel.
 * Format: [0x23][8B timestamp][0x26][1B gamepadIdx][2B seqNum BE][0x21][2B size BE][payload]
 *
 * Official GFN client's ul() adds [0x26][idx][seq] header when gamepad index is specified
 * (partially reliable path), then [0x21][2B size], then yc() prepends [0x23][8B ts].
 *
 * 0x26 = 38 decimal, PR sequence header byte (written by Va(38) in ul())
 *
 * For protocol v1-v2, returns the raw payload unchanged.
 */
function wrapGamepadPartiallyReliable(
  payload: Uint8Array,
  protocolVersion: number,
  gamepadIndex: number,
  sequenceNumber: number,
): Uint8Array {
  if (protocolVersion <= 2) {
    return payload;
  }
  // [0x23][8B ts][0x26][1B idx][2B seq][0x21][2B size][payload]
  const wrapped = new Uint8Array(9 + 1 + 1 + 2 + 1 + 2 + payload.length);
  const view = new DataView(wrapped.buffer);
  wrapped[0] = 0x23;
  writeTimestamp(view, 1);
  wrapped[9] = 0x26;  // PR sequence header (decimal 38, written by Va(38))
  wrapped[10] = gamepadIndex & 0xFF;  // gamepad index byte
  view.setUint16(11, sequenceNumber, false);  // sequence number (BE, Wa() with no endian param)
  wrapped[13] = 0x21;  // batched event marker
  view.setUint16(14, payload.length, false);  // size (BE)
  wrapped.set(payload, 16);
  return wrapped;
}

export class InputEncoder {
  private protocolVersion = 2;
  // Per-gamepad sequence numbers for partially reliable channel framing.
  // Official GFN client tracks this per-gamepad-index via this.tc Map.
  private gamepadSequence: Map<number, number> = new Map();

  setProtocolVersion(version: number): void {
    this.protocolVersion = version;
  }

  /** Get and increment the sequence number for a gamepad on the PR channel.
   *  Wraps at 65536 (uint16 range), matching official client's cl() function. */
  getNextGamepadSequence(gamepadIndex: number): number {
    const current = this.gamepadSequence.get(gamepadIndex) ?? 1;
    this.gamepadSequence.set(gamepadIndex, (current + 1) % 65536);
    return current;
  }

  resetGamepadSequences(): void {
    this.gamepadSequence.clear();
  }

  encodeHeartbeat(): Uint8Array {
    // Heartbeat is sent RAW — no v3 wrapper.
    // Official GFN client's Jc() sends [u32 LE = 2] directly, no 0x23/0x22 prefix.
    const payload = new Uint8Array(4);
    const view = new DataView(payload.buffer);
    view.setUint32(0, INPUT_HEARTBEAT, true);
    return payload;
  }

  encodeKeyDown(payload: KeyboardPayload): Uint8Array {
    return this.encodeKey(INPUT_KEY_DOWN, payload);
  }

  encodeKeyUp(payload: KeyboardPayload): Uint8Array {
    return this.encodeKey(INPUT_KEY_UP, payload);
  }

  encodeMouseMove(payload: MouseMovePayload): Uint8Array {
    const bytes = new Uint8Array(22);
    const view = new DataView(bytes.buffer);
    // [type 4B LE][dx 2B BE][dy 2B BE][reserved 6B BE][timestamp 8B BE]
    view.setUint32(0, INPUT_MOUSE_REL, true);        // type: LE
    view.setInt16(4, payload.dx, false);              // dx: BE
    view.setInt16(6, payload.dy, false);              // dy: BE
    view.setUint16(8, 0, false);                      // reserved: BE
    view.setUint32(10, 0, false);                     // reserved: BE
    view.setBigUint64(14, payload.timestampUs, false); // timestamp: BE
    return wrapMouseMoveEvent(bytes, this.protocolVersion);
  }

  encodeMouseButtonDown(payload: MouseButtonPayload): Uint8Array {
    return this.encodeMouseButton(INPUT_MOUSE_BUTTON_DOWN, payload);
  }

  encodeMouseButtonUp(payload: MouseButtonPayload): Uint8Array {
    return this.encodeMouseButton(INPUT_MOUSE_BUTTON_UP, payload);
  }

  encodeMouseWheel(payload: MouseWheelPayload): Uint8Array {
    const bytes = new Uint8Array(22);
    const view = new DataView(bytes.buffer);
    // [type 4B LE][horiz 2B BE][vert 2B BE][reserved 6B BE][timestamp 8B BE]
    view.setUint32(0, INPUT_MOUSE_WHEEL, true);        // type: LE
    view.setInt16(4, 0, false);                         // horizontal: BE
    view.setInt16(6, payload.delta, false);              // vertical: BE
    view.setUint16(8, 0, false);                         // reserved: BE
    view.setUint32(10, 0, false);                        // reserved: BE
    view.setBigUint64(14, payload.timestampUs, false);   // timestamp: BE
    return wrapSingleEvent(bytes, this.protocolVersion);
  }

  encodeGamepadState(payload: GamepadInput, bitmap: number, usePartiallyReliable: boolean): Uint8Array {
    const bytes = new Uint8Array(GAMEPAD_PACKET_SIZE);
    const view = new DataView(bytes.buffer);

    // Match official GFN client's gl() function exactly (vendor_beautified.js line 13469-13470):
    // gl(i, u, m, w, P, L, $=0, ae=0) where:
    //   i=DataView, u=base offset (0), m=gamepad index, w=buttons,
    //   P=triggers, L=axes[4], $=timestamp, ae=bitmap
    
    // Offset 0x00: Type (u32 LE) - event type 12
    view.setUint32(0, INPUT_GAMEPAD, true);
    
    // Offset 0x04: Payload size (u16 LE) = 26
    view.setUint16(4, 26, true);
    
    // Offset 0x06: Gamepad index (u16 LE)
    view.setUint16(6, payload.controllerId & 0x03, true);
    
    // Offset 0x08: Bitmap (u16 LE) — NOT a simple connected flag!
    // Official client uses a bitmask: bit i = gamepad i connected, bit (i+8) = additional state.
    // Passed as the `ae` parameter in gl() from the gamepad manager's this.nu field.
    view.setUint16(8, bitmap, true);
    
    // Offset 0x0A: Inner payload size (u16 LE) = 20
    view.setUint16(10, 20, true);
    
    // Offset 0x0C: Button flags (u16 LE) - XInput format
    view.setUint16(12, payload.buttons, true);
    
    // Offset 0x0E: Packed triggers (u16 LE: low byte=LT, high byte=RT)
    const packedTriggers = (payload.leftTrigger & 0xFF) | ((payload.rightTrigger & 0xFF) << 8);
    view.setUint16(14, packedTriggers, true);
    
    // Offset 0x10: Left stick X (i16 LE)
    view.setInt16(16, payload.leftStickX, true);
    
    // Offset 0x12: Left stick Y (i16 LE)
    view.setInt16(18, payload.leftStickY, true);
    
    // Offset 0x14: Right stick X (i16 LE)
    view.setInt16(20, payload.rightStickX, true);
    
    // Offset 0x16: Right stick Y (i16 LE)
    view.setInt16(22, payload.rightStickY, true);
    
    // Offset 0x18: Reserved (u16 LE) = 0
    view.setUint16(24, 0, true);
    
    // Offset 0x1A: Magic constant (u16 LE) = 85 (0x55)
    view.setUint16(26, 85, true);
    
    // Offset 0x1C: Reserved (u16 LE) = 0
    view.setUint16(28, 0, true);
    
    // Offset 0x1E: Timestamp (u64 LE)
    view.setBigUint64(30, payload.timestampUs, true);

    // Gamepad packets ARE wrapped in protocol v3+ — the official client's yc() function
    // applies the 0x23 wrapper for ALL channels (the v2+ check does NOT exclude PR).
    // The batching system also adds 0x21 inner framing.
    if (usePartiallyReliable) {
      // PR channel: [0x23][8B ts][0x26][1B idx][2B seq][0x21][2B size][38B payload]
      const seq = this.getNextGamepadSequence(payload.controllerId);
      return wrapGamepadPartiallyReliable(bytes, this.protocolVersion, payload.controllerId, seq);
    }
    // Reliable channel: [0x23][8B ts][0x21][2B size][38B payload]
    return wrapGamepadReliable(bytes, this.protocolVersion);
  }

  private encodeKey(type: number, payload: KeyboardPayload): Uint8Array {
    const bytes = new Uint8Array(18);
    const view = new DataView(bytes.buffer);
    // [type 4B LE][keycode 2B BE][modifiers 2B BE][scancode 2B BE][timestamp 8B BE]
    view.setUint32(0, type, true);                       // type: LE
    view.setUint16(4, payload.keycode, false);            // keycode: BE
    view.setUint16(6, payload.modifiers, false);          // modifiers: BE
    view.setUint16(8, payload.scancode, false);           // scancode: BE
    view.setBigUint64(10, payload.timestampUs, false);    // timestamp: BE
    return wrapSingleEvent(bytes, this.protocolVersion);
  }

  private encodeMouseButton(type: number, payload: MouseButtonPayload): Uint8Array {
    const bytes = new Uint8Array(18);
    const view = new DataView(bytes.buffer);
    // [type 4B LE][button 1B][pad 1B][reserved 4B BE][timestamp 8B BE]
    view.setUint32(0, type, true);                       // type: LE
    view.setUint8(4, payload.button);
    view.setUint8(5, 0);
    view.setUint32(6, 0, false);                          // reserved: BE
    view.setBigUint64(10, payload.timestampUs, false);    // timestamp: BE
    return wrapSingleEvent(bytes, this.protocolVersion);
  }
}

export function modifierFlags(event: KeyboardEvent): number {
  let flags = 0;
  // Basic modifiers (match Rust implementation)
  if (event.shiftKey) flags |= 0x01; // SHIFT
  if (event.ctrlKey) flags |= 0x02;  // CTRL
  if (event.altKey) flags |= 0x04;   // ALT
  if (event.metaKey) flags |= 0x08;  // META
  // Lock keys (match Rust modifier flags)
  if (event.getModifierState("CapsLock")) flags |= 0x10; // CAPS_LOCK
  if (event.getModifierState("NumLock")) flags |= 0x20;  // NUM_LOCK
  return flags;
}

export function mapKeyboardEvent(event: KeyboardEvent): { vk: number; scancode: number } | null {
  const mapped = codeMap[event.code];
  if (mapped) {
    return mapped;
  }

  const fallbackMapped = keyFallbackMap[event.key];
  if (fallbackMapped) {
    return fallbackMapped;
  }

  const key = event.key;
  if (key.length === 1) {
    const upper = key.toUpperCase();
    if (upper >= "A" && upper <= "Z") {
      return { vk: upper.charCodeAt(0), scancode: 0 };
    }
    if (key >= "0" && key <= "9") {
      return { vk: key.charCodeAt(0), scancode: 0 };
    }
  }

  return null;
}

/**
 * Convert browser mouse button (0-based) to GFN protocol (1-based).
 * Browser: 0=Left, 1=Middle, 2=Right, 3=Back, 4=Forward
 * GFN:     1=Left, 2=Middle, 3=Right, 4=Back, 5=Forward
 */
export function toMouseButton(button: number): number {
  // Convert 0-based browser button to 1-based GFN button
  return button + 1;
}

/**
 * Apply radial deadzone to analog stick values.
 * Uses a circular deadzone where values inside the threshold are zeroed.
 * @param x X-axis value (-1.0 to 1.0)
 * @param y Y-axis value (-1.0 to 1.0)
 * @param deadzone Deadzone threshold (0.0 to 1.0), default 15%
 * @returns Adjusted {x, y} values
 */
export function applyDeadzone(
  x: number,
  y: number,
  deadzone: number = GAMEPAD_DEADZONE
): { x: number; y: number } {
  // Calculate magnitude (distance from center)
  const magnitude = Math.sqrt(x * x + y * y);

  // If inside deadzone, return zero
  if (magnitude < deadzone) {
    return { x: 0, y: 0 };
  }

  // Normalize and rescale to full range
  const normalizedX = x / magnitude;
  const normalizedY = y / magnitude;

  // Scale from deadzone edge to 1.0
  const scaledMagnitude = (magnitude - deadzone) / (1.0 - deadzone);
  const clampedMagnitude = Math.min(1.0, scaledMagnitude);

  return {
    x: normalizedX * clampedMagnitude,
    y: normalizedY * clampedMagnitude,
  };
}

/**
 * Convert a normalized axis value (-1.0 to 1.0) to signed 16-bit integer.
 * @param value Normalized value (-1.0 to 1.0)
 * @returns Signed 16-bit integer (-32768 to 32767)
 */
export function normalizeToInt16(value: number): number {
  return Math.max(-32768, Math.min(32767, Math.round(value * 32767)));
}

/**
 * Convert a normalized trigger value (0.0 to 1.0) to unsigned 8-bit integer.
 * @param value Normalized value (0.0 to 1.0)
 * @returns Unsigned 8-bit integer (0 to 255)
 */
export function normalizeToUint8(value: number): number {
  return Math.max(0, Math.min(255, Math.round(value * 255)));
}

/**
 * Map Standard Gamepad API buttons to XInput button flags.
 * Standard Gamepad: https://w3c.github.io/gamepad/#remapping
 * 
 * Uses button.value (not button.pressed) to match the official GFN client's NA() function.
 * button.value is a float 0.0-1.0; any non-zero value counts as pressed.
 * This catches partial analog button presses that button.pressed might miss.
 */
export function mapGamepadButtons(gamepad: Gamepad): number {
  let buttons = 0;
  const b = gamepad.buttons;

  // Standard Gamepad mapping to XInput (matches official client's NA() exactly)
  // Face buttons
  if (b[0]?.value) buttons |= GAMEPAD_A;          // Bottom (A/Cross)
  if (b[1]?.value) buttons |= GAMEPAD_B;          // Right (B/Circle)
  if (b[2]?.value) buttons |= GAMEPAD_X;          // Left (X/Square)
  if (b[3]?.value) buttons |= GAMEPAD_Y;          // Top (Y/Triangle)
  
  // Bumpers
  if (b[4]?.value) buttons |= GAMEPAD_LB;         // Left Bumper
  if (b[5]?.value) buttons |= GAMEPAD_RB;         // Right Bumper
  
  // buttons[6] and [7] are LT/RT as buttons — we use analog trigger values instead
  
  // Center buttons
  if (b[8]?.value) buttons |= GAMEPAD_BACK;       // Back/Select
  if (b[9]?.value) buttons |= GAMEPAD_START;      // Start
  
  // Stick clicks (L3/R3)
  if (b[10]?.value) buttons |= GAMEPAD_LS;        // L3 (Left Stick click)
  if (b[11]?.value) buttons |= GAMEPAD_RS;        // R3 (Right Stick click)
  
  // D-Pad
  if (b[12]?.value) buttons |= GAMEPAD_DPAD_UP;
  if (b[13]?.value) buttons |= GAMEPAD_DPAD_DOWN;
  if (b[14]?.value) buttons |= GAMEPAD_DPAD_LEFT;
  if (b[15]?.value) buttons |= GAMEPAD_DPAD_RIGHT;
  
  // Guide button
  if (b[16]?.value) buttons |= GAMEPAD_GUIDE;     // Guide (Center/Xbox)

  return buttons;
}

/**
 * Read analog axes from Standard Gamepad API and apply deadzone.
 * @param gamepad The Gamepad object from navigator.getGamepads()
 * @returns Object with left/right stick and trigger values
 */
export function readGamepadAxes(gamepad: Gamepad): {
  leftStickX: number;
  leftStickY: number;
  rightStickX: number;
  rightStickY: number;
  leftTrigger: number;
  rightTrigger: number;
} {
  // Left stick (axes 0, 1)
  const lx = gamepad.axes[0] ?? 0;
  const ly = gamepad.axes[1] ?? 0;
  const leftStick = applyDeadzone(lx, ly);

  // Right stick (axes 2, 3)
  const rx = gamepad.axes[2] ?? 0;
  const ry = gamepad.axes[3] ?? 0;
  const rightStick = applyDeadzone(rx, ry);

  // Triggers - can be buttons (6, 7) or axes (4, 5) depending on browser
  let leftTrigger = 0;
  let rightTrigger = 0;

  if (gamepad.buttons[6]) {
    leftTrigger = gamepad.buttons[6].value;
  } else if (gamepad.axes[4] !== undefined && gamepad.axes[4] > 0) {
    leftTrigger = gamepad.axes[4];
  }

  if (gamepad.buttons[7]) {
    rightTrigger = gamepad.buttons[7].value;
  } else if (gamepad.axes[5] !== undefined && gamepad.axes[5] > 0) {
    rightTrigger = gamepad.axes[5];
  }

  return {
    leftStickX: leftStick.x,
    leftStickY: -leftStick.y, // Invert Y to match XInput convention
    rightStickX: rightStick.x,
    rightStickY: -rightStick.y, // Invert Y to match XInput convention
    leftTrigger,
    rightTrigger,
  };
}
