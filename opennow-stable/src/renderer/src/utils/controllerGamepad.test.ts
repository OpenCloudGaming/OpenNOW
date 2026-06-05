/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import { controllerButton, readControllerGamepadButtons } from "./controllerGamepad";

function makePad(pressedButtonIndexes: number[], axes = [0, 0]): Gamepad {
  return {
    axes,
    buttons: Array.from({ length: 17 }, (_, index) => ({ pressed: pressedButtonIndexes.includes(index) })),
  } as unknown as Gamepad;
}

test("maps standard gamepad action buttons used by controller mode", () => {
  const buttons = readControllerGamepadButtons(makePad([0, 1, 2, 3, 4, 5, 9]));
  const guideAliasButtons = readControllerGamepadButtons(makePad([16]));

  assert.equal(buttons & controllerButton.south, controllerButton.south);
  assert.equal(buttons & controllerButton.east, controllerButton.east);
  assert.equal(buttons & controllerButton.west, controllerButton.west);
  assert.equal(buttons & controllerButton.north, controllerButton.north);
  assert.equal(buttons & controllerButton.leftShoulder, controllerButton.leftShoulder);
  assert.equal(buttons & controllerButton.rightShoulder, controllerButton.rightShoulder);
  assert.equal(buttons & controllerButton.menu, controllerButton.menu);
  assert.equal(guideAliasButtons & controllerButton.menu, controllerButton.menu);
});

test("maps standard d-pad buttons and left-stick fallback axes", () => {
  const dpadButtons = readControllerGamepadButtons(makePad([12, 13, 14, 15]));
  const axes = readControllerGamepadButtons(makePad([], [-0.8, 0.9]));

  assert.equal(dpadButtons & controllerButton.up, controllerButton.up);
  assert.equal(dpadButtons & controllerButton.down, controllerButton.down);
  assert.equal(dpadButtons & controllerButton.left, controllerButton.left);
  assert.equal(dpadButtons & controllerButton.right, controllerButton.right);
  assert.equal(axes & controllerButton.left, controllerButton.left);
  assert.equal(axes & controllerButton.down, controllerButton.down);
});
