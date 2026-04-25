/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import {
  chooseAdaptiveMouseFlushInterval,
  quantizeMouseDeltaWithResidual,
} from "./webrtcClient";

test("quantizeMouseDeltaWithResidual preserves precision across sends", () => {
  const a = quantizeMouseDeltaWithResidual(0.4);
  assert.equal(a.send, 0);
  assert.equal(a.residual, 0.4);

  const b = quantizeMouseDeltaWithResidual(a.residual + 0.4);
  assert.equal(b.send, 1);
  assert.ok(Math.abs(b.residual - (-0.2)) < 1e-9);

  const c = quantizeMouseDeltaWithResidual(b.residual + 0.2);
  assert.equal(c.send, 0);
  assert.ok(Math.abs(c.residual) < 1e-9);
});

test("adaptive flush falls back to base when partially-reliable is unavailable", () => {
  const interval = chooseAdaptiveMouseFlushInterval({
    baseIntervalMs: 8,
    currentIntervalMs: 4,
    reliableBufferedAmount: 0,
    schedulingDelayMs: 0,
    canUsePartiallyReliableMouse: false,
    backpressureThresholdBytes: 64 * 1024,
    minIntervalMs: 2,
    maxIntervalMs: 20,
  });
  assert.equal(interval, 8);
});

test("adaptive flush tightens under low pressure and relaxes under pressure", () => {
  const lowPressure = chooseAdaptiveMouseFlushInterval({
    baseIntervalMs: 8,
    currentIntervalMs: 8,
    reliableBufferedAmount: 1024,
    schedulingDelayMs: 0.5,
    canUsePartiallyReliableMouse: true,
    backpressureThresholdBytes: 64 * 1024,
    minIntervalMs: 2,
    maxIntervalMs: 20,
  });
  assert.equal(lowPressure, 7);

  const highPressure = chooseAdaptiveMouseFlushInterval({
    baseIntervalMs: 8,
    currentIntervalMs: 7,
    reliableBufferedAmount: 48 * 1024,
    schedulingDelayMs: 5,
    canUsePartiallyReliableMouse: true,
    backpressureThresholdBytes: 64 * 1024,
    minIntervalMs: 2,
    maxIntervalMs: 20,
  });
  assert.equal(highPressure, 9);
});
