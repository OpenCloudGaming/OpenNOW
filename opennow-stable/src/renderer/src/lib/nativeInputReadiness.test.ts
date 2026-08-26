import assert from "node:assert/strict";
import test from "node:test";

import { nativeInputLifecycleAction } from "./nativeInputReadiness";

test("native stream start leaves input pending without activating capture", () => {
  assert.deepEqual(
    nativeInputLifecycleAction({ type: "native-stream-started" }),
    { state: "pending", activate: false },
  );
});

test("only native input ready activates capture", () => {
  assert.deepEqual(
    nativeInputLifecycleAction({ type: "native-input-ready", protocolVersion: 3 }),
    { state: "ready", activate: true, protocolVersion: 3 },
  );
});

test("native input unavailable records a truthful reason without activating capture", () => {
  assert.deepEqual(
    nativeInputLifecycleAction({ type: "native-input-unavailable", reason: "data channel failed" }),
    { state: "unavailable", activate: false, reason: "data channel failed" },
  );
});
