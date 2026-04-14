/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import { codeMap, mapKeyboardEvent, mapTextCharToKeySpec } from "./inputProtocol";

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
