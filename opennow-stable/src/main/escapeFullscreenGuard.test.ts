import test from "node:test";
import assert from "node:assert/strict";

import {
  isEscapeKeyDownInput,
  shouldCaptureEscapeFullscreenInput,
} from "./escapeFullscreenGuard";

test("isEscapeKeyDownInput recognizes Electron Escape keydown variants", () => {
  assert.equal(isEscapeKeyDownInput({ type: "keyDown", key: "Escape" }), true);
  assert.equal(isEscapeKeyDownInput({ type: "keyDown", key: "Esc" }), true);
  assert.equal(isEscapeKeyDownInput({ type: "keyDown", code: "Escape" }), true);
  assert.equal(isEscapeKeyDownInput({ type: "keyDown", keyCode: 27 }), true);
  assert.equal(isEscapeKeyDownInput({ type: "keyUp", key: "Escape", keyCode: 27 }), false);
  assert.equal(isEscapeKeyDownInput({ type: "keyDown", key: "Enter" }), false);
});

test("shouldCaptureEscapeFullscreenInput captures Escape while pointer locked", () => {
  assert.equal(shouldCaptureEscapeFullscreenInput(
    { type: "keyDown", key: "Escape" },
    {
      allowEscapeToExitFullscreen: false,
      pointerLockActive: true,
      windowFullscreen: false,
      pointerLockEscapeCaptureUntilMs: 0,
      nowMs: 100,
    },
  ), true);
});

test("shouldCaptureEscapeFullscreenInput captures rapid Escape presses during fullscreen pointer-lock loss", () => {
  assert.equal(shouldCaptureEscapeFullscreenInput(
    { type: "keyDown", key: "Escape" },
    {
      allowEscapeToExitFullscreen: false,
      pointerLockActive: false,
      windowFullscreen: true,
      pointerLockEscapeCaptureUntilMs: 1500,
      nowMs: 1000,
    },
  ), true);
});

test("shouldCaptureEscapeFullscreenInput allows Escape outside protected stream states", () => {
  const input = { type: "keyDown", key: "Escape" };
  assert.equal(shouldCaptureEscapeFullscreenInput(input, {
    allowEscapeToExitFullscreen: true,
    pointerLockActive: true,
    windowFullscreen: true,
    pointerLockEscapeCaptureUntilMs: 1500,
    nowMs: 1000,
  }), false);
  assert.equal(shouldCaptureEscapeFullscreenInput(input, {
    allowEscapeToExitFullscreen: false,
    pointerLockActive: false,
    windowFullscreen: true,
    pointerLockEscapeCaptureUntilMs: 999,
    nowMs: 1000,
  }), false);
  assert.equal(shouldCaptureEscapeFullscreenInput(input, {
    allowEscapeToExitFullscreen: false,
    pointerLockActive: false,
    windowFullscreen: false,
    pointerLockEscapeCaptureUntilMs: 1500,
    nowMs: 1000,
  }), false);
});
