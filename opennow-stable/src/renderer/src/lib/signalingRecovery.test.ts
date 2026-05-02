/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import { SIGNALING_RECOVERY_BASE_DELAYS_MS, signalingRecoveryDelayMs } from "./signalingRecovery";

test("signalingRecoveryDelayMs returns 0 for non-positive base", () => {
  assert.equal(signalingRecoveryDelayMs(0), 0);
  assert.equal(signalingRecoveryDelayMs(-100), 0);
});

test("signalingRecoveryDelayMs applies deterministic jitter from rand", () => {
  const alwaysLow = () => 0;
  const alwaysHigh = () => 0.999999;
  assert.equal(signalingRecoveryDelayMs(1000, alwaysLow), Math.floor(1000 * 0.85));
  assert.equal(signalingRecoveryDelayMs(1000, alwaysHigh), Math.floor(1000 * 1.1499997));
});

test("SIGNALING_RECOVERY_BASE_DELAYS_MS has expected length and first step zero", () => {
  assert.ok(SIGNALING_RECOVERY_BASE_DELAYS_MS.length >= 3);
  assert.equal(SIGNALING_RECOVERY_BASE_DELAYS_MS[0], 0);
});
