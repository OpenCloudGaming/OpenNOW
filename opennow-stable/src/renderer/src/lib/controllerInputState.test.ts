import test from "node:test";
import assert from "node:assert/strict";

import {
  CONTROLLER_HOLD_MS,
  CONTROLLER_MOVE_REPEAT_MS,
  createControllerEdgeState,
  stepControllerFrame,
  wasReleasedAsTap,
} from "./controllerInputState";
import { controllerButton } from "../utils/controllerGamepad";

const HOLD_OPTIONS = { holdMask: controllerButton.north } as const;

test("reports held buttons as pressed on the first frame only", () => {
  const first = stepControllerFrame(createControllerEdgeState(), controllerButton.south, 0);
  assert.equal(first.pressed & controllerButton.south, controllerButton.south);

  const second = stepControllerFrame(first.state, controllerButton.south, 16);
  assert.equal(second.pressed, 0);
});

test("re-fires a held direction after the repeat interval, not before", () => {
  const start = stepControllerFrame(createControllerEdgeState(), controllerButton.right, 1000);
  assert.ok(start.pressed & controllerButton.right);

  const tooSoon = stepControllerFrame(start.state, controllerButton.right, 1000 + CONTROLLER_MOVE_REPEAT_MS - 1);
  assert.equal(tooSoon.pressed, 0);

  const repeat = stepControllerFrame(tooSoon.state, controllerButton.right, 1000 + CONTROLLER_MOVE_REPEAT_MS + 1);
  assert.equal(repeat.pressed & controllerButton.right, controllerButton.right);
});

test("never auto-repeats non-directional buttons", () => {
  let frame = stepControllerFrame(createControllerEdgeState(), controllerButton.south, 0);
  for (let now = 100; now <= 2000; now += 100) {
    frame = stepControllerFrame(frame.state, controllerButton.south, now);
    assert.equal(frame.pressed, 0, `south repeated at ${now}ms`);
  }
});

test("repeatMs null disables auto-repeat entirely", () => {
  const start = stepControllerFrame(createControllerEdgeState(), controllerButton.down, 0, { repeatMs: null });
  assert.ok(start.pressed & controllerButton.down);

  const later = stepControllerFrame(start.state, controllerButton.down, 5000, { repeatMs: null });
  assert.equal(later.pressed, 0);
});

test("reports released buttons", () => {
  const pressed = stepControllerFrame(createControllerEdgeState(), controllerButton.east, 0);
  const released = stepControllerFrame(pressed.state, 0, 50);
  assert.equal(released.released & controllerButton.east, controllerButton.east);
  assert.equal(released.pressed, 0);
});

test("a short press does not fire the hold gesture and counts as a tap", () => {
  const down = stepControllerFrame(createControllerEdgeState(), controllerButton.north, 0, HOLD_OPTIONS);
  assert.equal(down.holdFired, false);

  const held = stepControllerFrame(down.state, controllerButton.north, 200, HOLD_OPTIONS);
  assert.equal(held.holdFired, false);

  const up = stepControllerFrame(held.state, 0, 210, HOLD_OPTIONS);
  assert.equal(up.holdFired, false);
  assert.equal(wasReleasedAsTap(up, controllerButton.north), true);
});

test("a long press latches the hold gesture exactly once and is not a tap", () => {
  const down = stepControllerFrame(createControllerEdgeState(), controllerButton.north, 0, HOLD_OPTIONS);
  const crossing = stepControllerFrame(down.state, controllerButton.north, CONTROLLER_HOLD_MS, HOLD_OPTIONS);
  assert.equal(crossing.holdFired, true);

  const stillHeld = stepControllerFrame(crossing.state, controllerButton.north, CONTROLLER_HOLD_MS + 500, HOLD_OPTIONS);
  assert.equal(stillHeld.holdFired, false, "hold must latch, not repeat");

  const up = stepControllerFrame(stillHeld.state, 0, CONTROLLER_HOLD_MS + 600, HOLD_OPTIONS);
  assert.equal(wasReleasedAsTap(up, controllerButton.north), false);
});

test("a tap after a hold re-arms the gesture", () => {
  const down = stepControllerFrame(createControllerEdgeState(), controllerButton.north, 0, HOLD_OPTIONS);
  const crossing = stepControllerFrame(down.state, controllerButton.north, CONTROLLER_HOLD_MS, HOLD_OPTIONS);
  const up = stepControllerFrame(crossing.state, 0, CONTROLLER_HOLD_MS + 10, HOLD_OPTIONS);

  const downAgain = stepControllerFrame(up.state, controllerButton.north, 2000, HOLD_OPTIONS);
  assert.equal(downAgain.state.holdConsumed, false);
  const upAgain = stepControllerFrame(downAgain.state, 0, 2100, HOLD_OPTIONS);
  assert.equal(wasReleasedAsTap(upAgain, controllerButton.north), true);
});

test("hold gesture is disabled by default", () => {
  const down = stepControllerFrame(createControllerEdgeState(), controllerButton.north, 0);
  const held = stepControllerFrame(down.state, controllerButton.north, 10_000);
  assert.equal(held.holdFired, false);
});

test("does not mutate the input state", () => {
  const state = createControllerEdgeState();
  const snapshot = { ...state };
  stepControllerFrame(state, controllerButton.left, 500, HOLD_OPTIONS);
  assert.deepEqual(state, snapshot);
});
