/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import {
  GAMEPAD_A,
  GAMEPAD_B,
  GAMEPAD_BACK,
  GAMEPAD_DPAD_UP,
  GAMEPAD_GUIDE,
  GAMEPAD_LB,
  GAMEPAD_PACKET_SIZE,
  GAMEPAD_RB,
  GAMEPAD_RS,
  GAMEPAD_START,
  GAMEPAD_X,
  GAMEPAD_Y,
  INPUT_GAMEPAD,
  INPUT_KEY_DOWN,
  INPUT_KEY_UP,
  INPUT_MOUSE_BUTTON_DOWN,
  INPUT_MOUSE_REL,
  INPUT_MOUSE_WHEEL,
  InputEncoder,
  codeMap,
  isPartiallyReliableHidTransferEligible,
  mapGamepadButtons,
  mapKeyboardEvent,
  mapTextCharToKeySpec,
  modifierFlags,
  normalizeToInt16,
  normalizeToUint8,
  partiallyReliableHidMaskForInputType,
  readGamepadAxes,
  toMouseButton,
} from "./inputProtocol";

function keyboardEvent(init: Partial<KeyboardEvent> & Pick<KeyboardEvent, "code" | "key">): KeyboardEvent {
  return {
    code: init.code,
    key: init.key,
    location: init.location ?? 0,
    shiftKey: init.shiftKey ?? false,
    ctrlKey: init.ctrlKey ?? false,
    altKey: init.altKey ?? false,
    metaKey: init.metaKey ?? false,
    keyCode: init.keyCode ?? 0,
    getModifierState: init.getModifierState ?? (() => false),
  } as KeyboardEvent;
}

function gamepad(overrides: Partial<Gamepad> = {}): Gamepad {
  const buttons = Array.from({ length: 17 }, () => ({ pressed: false, touched: false, value: 0 }));
  return {
    axes: [0, 0, 0, 0],
    buttons,
    connected: true,
    hapticActuators: [],
    id: "test-gamepad",
    index: 0,
    mapping: "standard",
    timestamp: 0,
    vibrationActuator: null,
    ...overrides,
  } as Gamepad;
}

function view(bytes: Uint8Array): DataView {
  return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
}

function assertV3BatchWrapper(bytes: Uint8Array, marker: number, lengthOffset: number, payloadOffset: number, payloadSize: number): DataView {
  assert.equal(bytes[0], 0x23);
  assert.equal(bytes[9], marker);
  const data = view(bytes);
  assert.equal(data.getUint16(lengthOffset, false), payloadSize);
  return new DataView(bytes.buffer, bytes.byteOffset + payloadOffset, payloadSize);
}

test("maps representative physical keys to Windows set-1 scancodes", () => {
  assert.deepEqual(mapKeyboardEvent(keyboardEvent({ code: "KeyA", key: "a" })), codeMap.KeyA);
  assert.deepEqual(mapKeyboardEvent(keyboardEvent({ code: "KeyN", key: "n" })), codeMap.KeyN);
  assert.deepEqual(mapKeyboardEvent(keyboardEvent({ code: "KeyT", key: "t" })), codeMap.KeyT);
  assert.deepEqual(mapKeyboardEvent(keyboardEvent({ code: "KeyZ", key: "z" })), codeMap.KeyZ);
  assert.deepEqual(mapKeyboardEvent(keyboardEvent({ code: "Comma", key: "," })), codeMap.Comma);
  assert.deepEqual(mapKeyboardEvent(keyboardEvent({ code: "Slash", key: "/" })), codeMap.Slash);
});

test("maps escape and left/right modifiers with correct scancodes", () => {
  assert.deepEqual(mapKeyboardEvent(keyboardEvent({ code: "Escape", key: "Escape", keyCode: 27 })), codeMap.Escape);
  assert.deepEqual(mapKeyboardEvent(keyboardEvent({ code: "ShiftLeft", key: "Shift", keyCode: 16 })), codeMap.ShiftLeft);
  assert.deepEqual(mapKeyboardEvent(keyboardEvent({ code: "ShiftRight", key: "Shift", keyCode: 16 })), codeMap.ShiftRight);
});

test("maps non-US and numpad physical keys", () => {
  assert.deepEqual(
    mapKeyboardEvent(keyboardEvent({ code: "IntlBackslash", key: "<" })),
    codeMap.IntlBackslash,
  );
  assert.deepEqual(mapKeyboardEvent(keyboardEvent({ code: "NumLock", key: "NumLock", keyCode: 144 })), codeMap.NumLock);
  assert.deepEqual(
    mapKeyboardEvent(keyboardEvent({ code: "Numpad0", key: "0", keyCode: 96, location: 3 })),
    codeMap.Numpad0,
  );
  assert.deepEqual(
    mapKeyboardEvent(keyboardEvent({ code: "NumpadEnter", key: "Enter", keyCode: 13, location: 3 })),
    codeMap.NumpadEnter,
  );
});

test("prefers physical code over layout-dependent key value", () => {
  const event = keyboardEvent({ code: "KeyZ", key: "y", keyCode: 89 });
  assert.deepEqual(mapKeyboardEvent(event), codeMap.KeyZ);
});

test("falls back to key-based escape detection when code is unavailable", () => {
  const event = keyboardEvent({ code: "", key: "Escape", keyCode: 27 });
  assert.deepEqual(mapKeyboardEvent(event), codeMap.Escape);
});

test("prefers platform keyCode for virtual-key derivation when available", () => {
  const event = keyboardEvent({ code: "Slash", key: "ö", keyCode: 191 });
  assert.deepEqual(mapKeyboardEvent(event), codeMap.Slash);
});

test("uses corrected scancodes for synthetic text injection", () => {
  assert.deepEqual(mapTextCharToKeySpec("a"), { ...codeMap.KeyA });
  assert.deepEqual(mapTextCharToKeySpec("N"), { ...codeMap.KeyN, shift: true });
  assert.deepEqual(mapTextCharToKeySpec("<"), { ...codeMap.Comma, shift: true });
  assert.deepEqual(mapTextCharToKeySpec("/"), { ...codeMap.Slash });
  assert.deepEqual(mapTextCharToKeySpec("?"), { ...codeMap.Slash, shift: true });
});

test("modifierFlags includes keyboard modifiers and lock bits", () => {
  const event = keyboardEvent({
    code: "KeyA",
    key: "a",
    shiftKey: true,
    ctrlKey: true,
    altKey: true,
    metaKey: true,
    getModifierState: (key) => key === "CapsLock" || key === "NumLock",
  });

  assert.equal(modifierFlags(event), 0x3f);
});

test("encodes raw v2 key down and key up payload layout", () => {
  const encoder = new InputEncoder();
  const payload = { keycode: 0x41, modifiers: 0x13, scancode: 0x1e, timestampUs: 0x0102030405060708n };

  for (const [bytes, type] of [[encoder.encodeKeyDown(payload), INPUT_KEY_DOWN], [encoder.encodeKeyUp(payload), INPUT_KEY_UP]] as const) {
    const data = view(bytes);
    assert.equal(bytes.byteLength, 18);
    assert.equal(data.getUint32(0, true), type);
    assert.equal(data.getUint16(4, false), 0x41);
    assert.equal(data.getUint16(6, false), 0x13);
    assert.equal(data.getUint16(8, false), 0x1e);
    assert.equal(data.getBigUint64(10, false), payload.timestampUs);
  }
});

test("maps browser mouse buttons to GFN buttons", () => {
  assert.deepEqual([0, 1, 2, 3, 4].map(toMouseButton), [1, 2, 3, 4, 5]);
});

test("encodes mouse move with v3 cursor wrapper and inner payload", () => {
  const encoder = new InputEncoder();
  encoder.setProtocolVersion(3);
  const bytes = encoder.encodeMouseMove({ dx: -12, dy: 34, timestampUs: 99n });
  const payload = assertV3BatchWrapper(bytes, 0x21, 10, 12, 22);

  assert.equal(payload.getUint32(0, true), INPUT_MOUSE_REL);
  assert.equal(payload.getInt16(4, false), -12);
  assert.equal(payload.getInt16(6, false), 34);
  assert.equal(payload.getBigUint64(14, false), 99n);
});

test("encodes mouse button and wheel with v3 single-event wrapper", () => {
  const encoder = new InputEncoder();
  encoder.setProtocolVersion(3);

  const buttonBytes = encoder.encodeMouseButtonDown({ button: 5, timestampUs: 123n });
  assert.equal(buttonBytes[0], 0x23);
  assert.equal(buttonBytes[9], 0x22);
  const buttonPayload = new DataView(buttonBytes.buffer, buttonBytes.byteOffset + 10, 18);
  assert.equal(buttonPayload.getUint32(0, true), INPUT_MOUSE_BUTTON_DOWN);
  assert.equal(buttonPayload.getUint8(4), 5);
  assert.equal(buttonPayload.getBigUint64(10, false), 123n);

  const wheelBytes = encoder.encodeMouseWheel({ delta: -120, timestampUs: 456n });
  assert.equal(wheelBytes[0], 0x23);
  assert.equal(wheelBytes[9], 0x22);
  const wheelPayload = new DataView(wheelBytes.buffer, wheelBytes.byteOffset + 10, 22);
  assert.equal(wheelPayload.getUint32(0, true), INPUT_MOUSE_WHEEL);
  assert.equal(wheelPayload.getInt16(4, false), 0);
  assert.equal(wheelPayload.getInt16(6, false), -120);
  assert.equal(wheelPayload.getBigUint64(14, false), 456n);
});

test("maps Gamepad API button values to XInput flags without pressed", () => {
  const buttons = Array.from({ length: 17 }, () => ({ pressed: false, touched: false, value: 0 }));
  for (const index of [0, 1, 2, 3, 4, 5, 8, 9, 11, 12, 16]) buttons[index] = { pressed: false, touched: false, value: 0.25 };

  assert.equal(
    mapGamepadButtons(gamepad({ buttons })),
    GAMEPAD_A | GAMEPAD_B | GAMEPAD_X | GAMEPAD_Y | GAMEPAD_LB | GAMEPAD_RB | GAMEPAD_BACK | GAMEPAD_START | GAMEPAD_RS | GAMEPAD_DPAD_UP | GAMEPAD_GUIDE,
  );
});

test("reads gamepad axes with radial deadzone, inverted Y, and trigger fallbacks", () => {
  const fromButtons = readGamepadAxes(gamepad({
    axes: [0.3, 0.4, 0.1, 0.1],
    buttons: Array.from({ length: 8 }, (_, index) => ({ pressed: false, touched: false, value: index === 6 ? 0.5 : index === 7 ? 0.75 : 0 })),
  }));
  assert.ok(fromButtons.leftStickX > 0);
  assert.ok(fromButtons.leftStickY < 0);
  assert.equal(fromButtons.rightStickX, 0);
  assert.equal(Object.is(fromButtons.rightStickY, -0) ? 0 : fromButtons.rightStickY, 0);
  assert.equal(fromButtons.leftTrigger, 0.5);
  assert.equal(fromButtons.rightTrigger, 0.75);

  const fromAxes = readGamepadAxes(gamepad({ axes: [0, 0, 0, -1, 0.6, 0.7], buttons: [] }));
  assert.equal(fromAxes.leftTrigger, 0.6);
  assert.equal(fromAxes.rightTrigger, 0.7);
  assert.equal(fromAxes.rightStickY, 1);
});

test("normalizers clamp extremes", () => {
  assert.equal(normalizeToInt16(-2), -32768);
  assert.equal(normalizeToInt16(2), 32767);
  assert.equal(normalizeToUint8(-1), 0);
  assert.equal(normalizeToUint8(2), 255);
});

test("encodes gamepad state with reliable and partially reliable v3 wrappers", () => {
  const encoder = new InputEncoder();
  encoder.setProtocolVersion(3);
  const payload = {
    controllerId: 2,
    buttons: GAMEPAD_A,
    leftTrigger: 10,
    rightTrigger: 20,
    leftStickX: -100,
    leftStickY: 200,
    rightStickX: -300,
    rightStickY: 400,
    connected: true,
    timestampUs: 0x0102030405060708n,
  };

  const reliablePayload = assertV3BatchWrapper(encoder.encodeGamepadState(payload, 0x0104, false), 0x21, 10, 12, GAMEPAD_PACKET_SIZE);
  assert.equal(reliablePayload.getUint32(0, true), INPUT_GAMEPAD);
  assert.equal(reliablePayload.getUint16(4, true), 26);
  assert.equal(reliablePayload.getUint16(6, true), 2);
  assert.equal(reliablePayload.getUint16(8, true), 0x0104);
  assert.equal(reliablePayload.getUint16(12, true), GAMEPAD_A);
  assert.equal(reliablePayload.getUint16(14, true), 0x140a);
  assert.equal(reliablePayload.getInt16(16, true), -100);
  assert.equal(reliablePayload.getBigUint64(30, true), payload.timestampUs);

  const first = encoder.encodeGamepadState(payload, 0x0104, true);
  const second = encoder.encodeGamepadState(payload, 0x0104, true);
  for (const [bytes, sequence] of [[first, 1], [second, 2]] as const) {
    assert.equal(bytes[0], 0x23);
    assert.equal(bytes[9], 0x26);
    assert.equal(bytes[10], 2);
    const data = view(bytes);
    assert.equal(data.getUint16(11, false), sequence);
    assert.equal(bytes[13], 0x21);
    assert.equal(data.getUint16(14, false), GAMEPAD_PACKET_SIZE);
    assert.equal(new DataView(bytes.buffer, bytes.byteOffset + 16, GAMEPAD_PACKET_SIZE).getUint32(0, true), INPUT_GAMEPAD);
  }
});

test("partially reliable HID helpers only mark mouse-relative input eligible", () => {
  assert.equal(partiallyReliableHidMaskForInputType(INPUT_MOUSE_REL), 1 << INPUT_MOUSE_REL);
  assert.equal(partiallyReliableHidMaskForInputType(-1), 0);
  assert.equal(partiallyReliableHidMaskForInputType(32), 0);
  assert.equal(isPartiallyReliableHidTransferEligible(INPUT_MOUSE_REL), true);
  assert.equal(isPartiallyReliableHidTransferEligible(INPUT_MOUSE_BUTTON_DOWN), false);
  assert.equal(isPartiallyReliableHidTransferEligible(INPUT_GAMEPAD), false);
});
