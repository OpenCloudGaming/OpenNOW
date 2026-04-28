export const INPUT_HEARTBEAT = 2;
export const INPUT_KEY_DOWN = 3;
export const INPUT_KEY_UP = 4;
export const INPUT_MOUSE_REL = 7;
export const INPUT_MOUSE_BUTTON_DOWN = 8;
export const INPUT_MOUSE_BUTTON_UP = 9;
export const INPUT_MOUSE_WHEEL = 10;
export const INPUT_GAMEPAD = 12;
export const INPUT_HAPTICS_ENABLED = 13;

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
export const PARTIALLY_RELIABLE_GAMEPAD_MASK_ALL = (1 << GAMEPAD_MAX_CONTROLLERS) - 1;
export const PARTIALLY_RELIABLE_HID_DEVICE_MASK_ALL = 0xFFFFFFFF;

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

export function partiallyReliableHidMaskForInputType(inputType: number): number {
  if (!Number.isInteger(inputType) || inputType < 0 || inputType > 31) {
    return 0;
  }
  return 1 << inputType;
}

export function isPartiallyReliableHidTransferEligible(inputType: number): boolean {
  return inputType === INPUT_MOUSE_REL;
}

export interface KeyMapping {
  vk: number;
  scancode: number;
}

export interface TextKeySpec extends KeyMapping {
  shift?: boolean;
}

type KeyLike = Pick<KeyboardEvent, "code" | "key" | "keyCode" | "location">;

const DOM_KEY_LOCATION_STANDARD = 0;
const DOM_KEY_LOCATION_LEFT = 1;
const DOM_KEY_LOCATION_RIGHT = 2;
const DOM_KEY_LOCATION_NUMPAD = 3;

const scancodeByCode: Record<string, number> = {
  KeyA: 0x001e,
  KeyB: 0x0030,
  KeyC: 0x002e,
  KeyD: 0x0020,
  KeyE: 0x0012,
  KeyF: 0x0021,
  KeyG: 0x0022,
  KeyH: 0x0023,
  KeyI: 0x0017,
  KeyJ: 0x0024,
  KeyK: 0x0025,
  KeyL: 0x0026,
  KeyM: 0x0032,
  KeyN: 0x0031,
  KeyO: 0x0018,
  KeyP: 0x0019,
  KeyQ: 0x0010,
  KeyR: 0x0013,
  KeyS: 0x001f,
  KeyT: 0x0014,
  KeyU: 0x0016,
  KeyV: 0x002f,
  KeyW: 0x0011,
  KeyX: 0x002d,
  KeyY: 0x0015,
  KeyZ: 0x002c,
  Digit1: 0x0002,
  Digit2: 0x0003,
  Digit3: 0x0004,
  Digit4: 0x0005,
  Digit5: 0x0006,
  Digit6: 0x0007,
  Digit7: 0x0008,
  Digit8: 0x0009,
  Digit9: 0x000a,
  Digit0: 0x000b,
  Enter: 0x001c,
  Escape: 0x0001,
  Backspace: 0x000e,
  Tab: 0x000f,
  Space: 0x0039,
  Minus: 0x000c,
  Equal: 0x000d,
  BracketLeft: 0x001a,
  BracketRight: 0x001b,
  Backslash: 0x002b,
  IntlBackslash: 0x0056,
  IntlRo: 0x0073,
  IntlYen: 0x007d,
  Semicolon: 0x0027,
  Quote: 0x0028,
  Backquote: 0x0029,
  Comma: 0x0033,
  Period: 0x0034,
  Slash: 0x0035,
  F1: 0x003b,
  F2: 0x003c,
  F3: 0x003d,
  F4: 0x003e,
  F5: 0x003f,
  F6: 0x0040,
  F7: 0x0041,
  F8: 0x0042,
  F9: 0x0043,
  F10: 0x0044,
  F11: 0x0057,
  F12: 0x0058,
  F13: 0x0064,
  ArrowRight: 0xe04d,
  ArrowLeft: 0xe04b,
  ArrowDown: 0xe050,
  ArrowUp: 0xe048,
  ControlLeft: 0x001d,
  ShiftLeft: 0x002a,
  AltLeft: 0x0038,
  MetaLeft: 0xe05b,
  ControlRight: 0xe01d,
  ShiftRight: 0x0036,
  AltRight: 0xe038,
  MetaRight: 0xe05c,
  CapsLock: 0x003a,
  NumLock: 0xe045,
  Insert: 0xe052,
  Delete: 0xe053,
  Home: 0xe047,
  End: 0xe04f,
  PageUp: 0xe049,
  PageDown: 0xe051,
  PrintScreen: 0xe037,
  ScrollLock: 0x0046,
  Pause: 0x0045,
  ContextMenu: 0xe05d,
  Numpad0: 0x0052,
  Numpad1: 0x004f,
  Numpad2: 0x0050,
  Numpad3: 0x0051,
  Numpad4: 0x004b,
  Numpad5: 0x004c,
  Numpad6: 0x004d,
  Numpad7: 0x0047,
  Numpad8: 0x0048,
  Numpad9: 0x0049,
  NumpadAdd: 0x004e,
  NumpadSubtract: 0x004a,
  NumpadMultiply: 0x0037,
  NumpadDivide: 0xe035,
  NumpadDecimal: 0x0053,
  NumpadEnter: 0xe01c,
  NumpadEqual: 0x0059,
  NumpadComma: 0x007e,
};

const specialVirtualKeyByCode: Record<string, number> = {
  Enter: 0x0d,
  Escape: 0x1b,
  Backspace: 0x08,
  Tab: 0x09,
  Space: 0x20,
  Minus: 0xbd,
  Equal: 0xbb,
  BracketLeft: 0xdb,
  BracketRight: 0xdd,
  Backslash: 0xdc,
  IntlBackslash: 0xe2,
  IntlRo: 0xc1,
  IntlYen: 0xdc,
  Semicolon: 0xba,
  Quote: 0xde,
  Backquote: 0xc0,
  Comma: 0xbc,
  Period: 0xbe,
  Slash: 0xbf,
  ArrowRight: 0x27,
  ArrowLeft: 0x25,
  ArrowDown: 0x28,
  ArrowUp: 0x26,
  ControlLeft: 0xa2,
  ShiftLeft: 0xa0,
  AltLeft: 0xa4,
  MetaLeft: 0x5b,
  ControlRight: 0xa3,
  ShiftRight: 0xa1,
  AltRight: 0xa5,
  MetaRight: 0x5c,
  CapsLock: 0x14,
  NumLock: 0x90,
  Insert: 0x2d,
  Delete: 0x2e,
  Home: 0x24,
  End: 0x23,
  PageUp: 0x21,
  PageDown: 0x22,
  PrintScreen: 0x2c,
  ScrollLock: 0x91,
  Pause: 0x13,
  ContextMenu: 0x5d,
  NumpadAdd: 0x6b,
  NumpadSubtract: 0x6d,
  NumpadMultiply: 0x6a,
  NumpadDivide: 0x6f,
  NumpadDecimal: 0x6e,
  NumpadEnter: 0x0d,
  NumpadEqual: 0xbb,
  NumpadComma: 0xbc,
};

const keyFallbackMap: Record<string, KeyMapping> = {
  Escape: { vk: 0x1b, scancode: 0x0001 },
  Esc: { vk: 0x1b, scancode: 0x0001 },
};

const baseCharCodeMap: Record<string, string> = {
  " ": "Space",
  "\n": "Enter",
  "\r": "Enter",
  "\t": "Tab",
  "0": "Digit0",
  "1": "Digit1",
  "2": "Digit2",
  "3": "Digit3",
  "4": "Digit4",
  "5": "Digit5",
  "6": "Digit6",
  "7": "Digit7",
  "8": "Digit8",
  "9": "Digit9",
  "-": "Minus",
  "=": "Equal",
  "[": "BracketLeft",
  "]": "BracketRight",
  "\\": "Backslash",
  ";": "Semicolon",
  "'": "Quote",
  "`": "Backquote",
  ",": "Comma",
  ".": "Period",
  "/": "Slash",
};

const shiftedCharCodeMap: Record<string, string> = {
  "!": "Digit1",
  "@": "Digit2",
  "#": "Digit3",
  "$": "Digit4",
  "%": "Digit5",
  "^": "Digit6",
  "&": "Digit7",
  "*": "Digit8",
  "(": "Digit9",
  ")": "Digit0",
  "_": "Minus",
  "+": "Equal",
  "{": "BracketLeft",
  "}": "BracketRight",
  "|": "Backslash",
  ":": "Semicolon",
  '"': "Quote",
  "~": "Backquote",
  "<": "Comma",
  ">": "Period",
  "?": "Slash",
};

function defaultVirtualKeyFromCode(code: string): number | null {
  if (code.startsWith("Key") && code.length === 4) {
    return code.charCodeAt(3);
  }

  if (code.startsWith("Digit") && code.length === 6) {
    return code.charCodeAt(5);
  }

  if (code.startsWith("F")) {
    const index = Number.parseInt(code.slice(1), 10);
    if (index >= 1 && index <= 24) {
      return 0x70 + index - 1;
    }
  }

  if (code.startsWith("Numpad") && code.length === 7) {
    const digit = Number.parseInt(code.slice(6), 10);
    if (digit >= 0 && digit <= 9) {
      return 0x60 + digit;
    }
  }

  return specialVirtualKeyByCode[code] ?? null;
}

function keyMappingFromCode(code: string): KeyMapping | null {
  const scancode = scancodeByCode[code];
  if (scancode === undefined) {
    return null;
  }

  const vk = defaultVirtualKeyFromCode(code);
  if (vk === null) {
    return null;
  }

  return { vk, scancode };
}

export const codeMap: Record<string, KeyMapping> = Object.freeze(
  Object.fromEntries(Object.keys(scancodeByCode).map((code) => [code, keyMappingFromCode(code)!])),
) as Record<string, KeyMapping>;

function virtualKeyFromKeyCode(event: KeyLike): number | null {
  const keyCode = event.keyCode;
  if (!Number.isInteger(keyCode) || keyCode <= 0 || keyCode === 229) {
    return null;
  }

  switch (event.code) {
    case "ShiftLeft":
      return 0xa0;
    case "ShiftRight":
      return 0xa1;
    case "ControlLeft":
      return 0xa2;
    case "ControlRight":
      return 0xa3;
    case "AltLeft":
      return 0xa4;
    case "AltRight":
      return 0xa5;
    case "MetaLeft":
      return 0x5b;
    case "MetaRight":
      return 0x5c;
  }

  if (event.location === DOM_KEY_LOCATION_NUMPAD) {
    if (keyCode >= 0x60 && keyCode <= 0x69) {
      return keyCode;
    }
    if (keyCode === 0x0d && event.code === "NumpadEnter") {
      return keyCode;
    }
  }

  return keyCode;
}

function virtualKeyFromKeyValue(key: string): number | null {
  if (key.length === 1) {
    const codePoint = key.toUpperCase().charCodeAt(0);
    if ((codePoint >= 0x30 && codePoint <= 0x39) || (codePoint >= 0x41 && codePoint <= 0x5a)) {
      return codePoint;
    }
  }

  switch (key) {
    case "Escape":
    case "Esc":
      return 0x1b;
    case "Enter":
      return 0x0d;
    case "Tab":
      return 0x09;
    case "Backspace":
      return 0x08;
    case " ":
    case "Spacebar":
      return 0x20;
    case "ArrowLeft":
      return 0x25;
    case "ArrowUp":
      return 0x26;
    case "ArrowRight":
      return 0x27;
    case "ArrowDown":
      return 0x28;
    case "Delete":
      return 0x2e;
    case "Insert":
      return 0x2d;
    case "Home":
      return 0x24;
    case "End":
      return 0x23;
    case "PageUp":
      return 0x21;
    case "PageDown":
      return 0x22;
  }

  return null;
}

function virtualKeyFromEvent(event: KeyLike): number | null {
  if (event.code.startsWith("Key") || event.code.startsWith("Digit")) {
    return defaultVirtualKeyFromCode(event.code);
  }

  return virtualKeyFromKeyCode(event) ?? virtualKeyFromKeyValue(event.key) ?? defaultVirtualKeyFromCode(event.code);
}

function textKeySpecFromCode(code: string, shift: boolean = false): TextKeySpec | null {
  const mapped = keyMappingFromCode(code);
  if (!mapped) {
    return null;
  }
  return shift ? { ...mapped, shift: true } : mapped;
}

export function mapTextCharToKeySpec(char: string): TextKeySpec | null {
  const baseCode = baseCharCodeMap[char];
  if (baseCode) {
    return textKeySpecFromCode(baseCode);
  }

  const shiftedCode = shiftedCharCodeMap[char];
  if (shiftedCode) {
    return textKeySpecFromCode(shiftedCode, true);
  }

  if (char >= "a" && char <= "z") {
    return textKeySpecFromCode(`Key${char.toUpperCase()}`);
  }

  if (char >= "A" && char <= "Z") {
    return textKeySpecFromCode(`Key${char}`, true);
  }

  return null;
}

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

  encodeHapticsEnabled(enabled: boolean): Uint8Array {
    const bytes = new Uint8Array(6);
    const view = new DataView(bytes.buffer);
    view.setUint32(0, INPUT_HAPTICS_ENABLED, true);
    view.setUint16(4, enabled ? 1 : 0, false);
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
    
    // Offset 0x08: Bitmap (u16 LE) — official this.nu bitmask.
    // Bit i = gamepad i connected; bit (i+8) = Xbox/xinput style device.
    // The high bit likely advertises the XInput/haptics-capable variant.
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

export function mapKeyboardEvent(event: KeyboardEvent): KeyMapping | null {
  // The official GFN web client appears to forward the raw DOM key event into a lower-level
  // virtual input controller instead of keeping a large JS `{ code -> { vk, scancode } }` table.
  // Electron does not expose that downstream native translation layer to this renderer, so the
  // closest behavior we can reproduce here is:
  // 1. derive the Windows virtual-key from the DOM event itself (`keyCode`, `key`, `location`)
  // 2. keep only a DOM `code -> scancode` lookup for the protocol field that Chromium does not expose
  // This preserves physical-key behavior across layouts while minimizing JS-side policy.
  const scancode =
    (event.code ? scancodeByCode[event.code] : undefined)
    ?? keyFallbackMap[event.key]?.scancode
    ?? (event.key.length === 1 ? mapTextCharToKeySpec(event.key)?.scancode : undefined);

  if (scancode === undefined) {
    return null;
  }

  const vk = virtualKeyFromEvent(event);
  if (vk === null) {
    return null;
  }

  return { vk, scancode };
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
