/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import { FULLSCREEN_KEYBOARD_LOCK_CODES } from "./keyboardLock";

test("fullscreen keyboard lock includes keys intercepted by browsers", () => {
  assert.ok(FULLSCREEN_KEYBOARD_LOCK_CODES.includes("KeyT"));
  assert.ok(FULLSCREEN_KEYBOARD_LOCK_CODES.includes("KeyN"));
  assert.ok(FULLSCREEN_KEYBOARD_LOCK_CODES.includes("Escape"));
});
