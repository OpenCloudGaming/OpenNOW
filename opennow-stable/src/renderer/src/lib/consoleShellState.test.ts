import test from "node:test";
import assert from "node:assert/strict";

import { resolveInitialConsoleStage, resolveProfileSelection } from "./consoleShellState";

const BASE = {
  controllerMode: true,
  directLaunchConsoleMode: false,
  pickerEnabled: true,
  hasAuthSession: true,
  savedAccountCount: 2,
};

test("boots into the splash, which hands over to the picker", () => {
  assert.equal(resolveInitialConsoleStage(BASE), "splash");
});

test("shows the picker even with a single account, so Manage profiles stays reachable", () => {
  assert.equal(resolveInitialConsoleStage({ ...BASE, savedAccountCount: 1 }), "splash");
});

test("never gates a direct launch, even when profiles are locked", () => {
  assert.equal(resolveInitialConsoleStage({ ...BASE, directLaunchConsoleMode: true }), "shell");
});

test("leaves the desktop shell untouched", () => {
  assert.equal(resolveInitialConsoleStage({ ...BASE, controllerMode: false }), "shell");
});

test("respects the launch-picker setting", () => {
  assert.equal(resolveInitialConsoleStage({ ...BASE, pickerEnabled: false }), "shell");
});

test("skips the picker when there is nothing to pick", () => {
  assert.equal(resolveInitialConsoleStage({ ...BASE, savedAccountCount: 0 }), "shell");
  assert.equal(resolveInitialConsoleStage({ ...BASE, hasAuthSession: false }), "shell");
});

test("a locked profile always verifies, including the active one", () => {
  assert.deepEqual(resolveProfileSelection({ userId: "a", hasPin: true }, "b"), { action: "verify" });
  assert.deepEqual(
    resolveProfileSelection({ userId: "a", hasPin: true }, "a"),
    { action: "verify" },
    "reopening the picker must not bypass the lock",
  );
});

test("an unlocked active profile enters directly instead of round-tripping a switch", () => {
  assert.deepEqual(resolveProfileSelection({ userId: "a", hasPin: false }, "a"), { action: "enter" });
});

test("an unlocked inactive profile switches", () => {
  assert.deepEqual(resolveProfileSelection({ userId: "a", hasPin: false }, "b"), { action: "switch" });
  assert.deepEqual(resolveProfileSelection({ userId: "a", hasPin: false }, null), { action: "switch" });
});
