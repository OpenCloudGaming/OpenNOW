/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import { FULLSCREEN_KEYBOARD_LOCK_CODES } from "./keyboardLock";

test("matches the official fullscreen keyboard lock coverage for affected regional keys", () => {
  assert.equal(FULLSCREEN_KEYBOARD_LOCK_CODES[0], "Escape");
  assert.equal(FULLSCREEN_KEYBOARD_LOCK_CODES[1], "F11");
  assert.ok(FULLSCREEN_KEYBOARD_LOCK_CODES.includes("KeyT"));
  assert.ok(FULLSCREEN_KEYBOARD_LOCK_CODES.includes("KeyN"));
  assert.ok(FULLSCREEN_KEYBOARD_LOCK_CODES.includes("KeyZ"));
  assert.ok(FULLSCREEN_KEYBOARD_LOCK_CODES.includes("Slash"));
  assert.ok(FULLSCREEN_KEYBOARD_LOCK_CODES.includes("Digit1"));
  assert.ok(FULLSCREEN_KEYBOARD_LOCK_CODES.includes("PrintScreen"));
  assert.ok(FULLSCREEN_KEYBOARD_LOCK_CODES.includes("LaunchMail"));
  assert.equal(
    FULLSCREEN_KEYBOARD_LOCK_CODES[FULLSCREEN_KEYBOARD_LOCK_CODES.length - 1],
    "KeyG",
  );
});
