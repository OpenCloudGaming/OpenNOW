/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import type { StreamSettings } from "@shared/gfn";
import { buildRequestedStreamingFeatures } from "./cloudmatch";

function makeSettings(overrides: Partial<StreamSettings> = {}): StreamSettings {
  return {
    resolution: "2560x1440",
    fps: 240,
    maxBitrateMbps: 75,
    codec: "H265",
    colorQuality: "8bit_420",
    keyboardLayout: "en-US",
    gameLanguage: "en_US",
    enableL4S: false,
    enableCloudGsync: false,
    clientMode: "native",
    ...overrides,
  };
}

test("CloudMatch requests resolved Cloud G-Sync value", () => {
  const off = buildRequestedStreamingFeatures(makeSettings({ enableCloudGsync: false }), 0, 0, false);
  const on = buildRequestedStreamingFeatures(makeSettings({ enableCloudGsync: true }), 0, 0, false);

  assert.equal(off.cloudGsync, false);
  assert.equal(on.cloudGsync, true);
});

test("CloudMatch reflex request follows official-style Cloud G-Sync gating", () => {
  const lowFpsNoVrr = buildRequestedStreamingFeatures(
    makeSettings({ fps: 60, enableCloudGsync: false }),
    0,
    0,
    false,
  );
  const lowFpsWithVrr = buildRequestedStreamingFeatures(
    makeSettings({ fps: 60, enableCloudGsync: true }),
    0,
    0,
    false,
  );
  const highFpsNoVrr = buildRequestedStreamingFeatures(
    makeSettings({ fps: 120, enableCloudGsync: false }),
    0,
    0,
    false,
  );

  assert.equal(lowFpsNoVrr.reflex, false);
  assert.equal(lowFpsWithVrr.reflex, true);
  assert.equal(highFpsNoVrr.reflex, true);
});

test("CloudMatch uses resolver Reflex decision when present", () => {
  const features = buildRequestedStreamingFeatures(
    makeSettings({
      fps: 60,
      enableCloudGsync: true,
      clientMode: "web",
      cloudGsyncResolution: {
        requested: true,
        enabled: true,
        reflexEnabled: false,
        reason: "web-mode",
        capabilities: {
          platformSupportsCloudGsync: false,
          isVrrCapableDisplay: false,
          isGsyncDisplay: false,
          minimumFpsForCloudGsync: 60,
          minimumFpsForReflexWithoutVrr: 120,
          detectionSource: "unsupported",
        },
      },
    }),
    0,
    0,
    false,
  );

  assert.equal(features.cloudGsync, true);
  assert.equal(features.reflex, false);
});
