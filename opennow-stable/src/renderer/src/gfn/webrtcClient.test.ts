/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import {
  chooseAdaptiveMouseFlushInterval,
  classifyStreamLagReason,
  quantizeMouseDeltaWithResidual,
  subsampleCoalescedPointerEvents,
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

test("adaptive flush on reliable mouse tightens toward min under low pressure", () => {
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
  assert.equal(interval, 3);
});

test("adaptive flush keeps base interval when partially-reliable mouse is active", () => {
  const underPressure = chooseAdaptiveMouseFlushInterval({
    baseIntervalMs: 8,
    currentIntervalMs: 20,
    reliableBufferedAmount: 48 * 1024,
    schedulingDelayMs: 8,
    canUsePartiallyReliableMouse: true,
    backpressureThresholdBytes: 64 * 1024,
    minIntervalMs: 2,
    maxIntervalMs: 20,
  });
  assert.equal(underPressure, 8);
});

test("adaptive flush tightens under low pressure and relaxes under pressure on reliable mouse", () => {
  const lowPressure = chooseAdaptiveMouseFlushInterval({
    baseIntervalMs: 8,
    currentIntervalMs: 8,
    reliableBufferedAmount: 1024,
    schedulingDelayMs: 0.5,
    canUsePartiallyReliableMouse: false,
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
    canUsePartiallyReliableMouse: false,
    backpressureThresholdBytes: 64 * 1024,
    minIntervalMs: 2,
    maxIntervalMs: 20,
  });
  assert.equal(highPressure, 9);
});

test("subsampleCoalescedPointerEvents thins large coalesced bursts", () => {
  const samples = Array.from({ length: 12 }, (_, index) => ({
    movementX: index,
    movementY: index,
  }));
  const { events, stride } = subsampleCoalescedPointerEvents(samples, 0, 4);
  assert.ok(stride > 1);
  assert.ok(events.length < samples.length);
  assert.equal(events[0]?.movementX, 0);
});

const stableLagParams = {
  nativeInputActive: false,
  nativeRendererActive: false,
  framesReceived: 5000,
  framesDecoded: 4980,
  decodeTimeMs: 9.2,
  decodeFps: 120,
  renderFps: 118,
  rttMs: 6,
  packetLossPercent: 0,
  jitterMs: 0.5,
  jitterBufferDelayMs: 0.5,
  inputQueueBufferedBytes: 0,
  inputQueueDropCount: 0,
  decoderPressureActive: false,
  decoderPressureReason: "stable",
  decoderBacklogFrames: 20,
  dropRatePercent: 0.1,
  backpressureThresholdBytes: 64 * 1024,
};

test("classifyStreamLagReason ignores mouse coalesce timer jitter for input lag", () => {
  const result = classifyStreamLagReason({
    ...stableLagParams,
    inputQueueBufferedBytes: 2048,
  });
  assert.equal(result.reason, "stable");
});

test("classifyStreamLagReason ignores normal AV1 decode time without sustained decoder pressure", () => {
  const result = classifyStreamLagReason({
    ...stableLagParams,
    decodeTimeMs: 8.4,
    decodeFps: 120,
  });
  assert.equal(result.reason, "stable");
});

test("classifyStreamLagReason reports input lag only for drops or full channel buffer", () => {
  assert.equal(
    classifyStreamLagReason({
      ...stableLagParams,
      inputQueueDropCount: 2,
    }).reason,
    "input_backpressure",
  );
  assert.equal(
    classifyStreamLagReason({
      ...stableLagParams,
      inputQueueBufferedBytes: 64 * 1024,
    }).reason,
    "input_backpressure",
  );
});

test("classifyStreamLagReason reports decoder lag only for sustained pressure", () => {
  const result = classifyStreamLagReason({
    ...stableLagParams,
    decoderPressureActive: true,
    decoderPressureReason: "backlog_and_drop",
    decoderBacklogFrames: 52,
    dropRatePercent: 7.5,
  });
  assert.equal(result.reason, "decoder");
  assert.match(result.detail, /backlog 52/);
});

test("classifyStreamLagReason ignores small render gap at high stream fps", () => {
  const result = classifyStreamLagReason({
    ...stableLagParams,
    decodeFps: 240,
    renderFps: 228,
  });
  assert.equal(result.reason, "stable");
});

test("classifyStreamLagReason reports render lag for large relative render drop", () => {
  const result = classifyStreamLagReason({
    ...stableLagParams,
    decodeFps: 120,
    renderFps: 90,
  });
  assert.equal(result.reason, "render");
});
