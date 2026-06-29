/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import {
  getSafeFallbackEntitledResolutions,
  resolveEntitledStreamProfile,
  SAFE_FALLBACK_STREAM_PROFILE,
} from "./gfn";

test("resolves requested stream settings to entitled resolution and fps profiles", () => {
  const entitlements = [
    { width: 1920, height: 1080, fps: 60 },
    { width: 1280, height: 720, fps: 60 },
  ];

  assert.deepEqual(
    resolveEntitledStreamProfile(entitlements, { resolution: "1920x1080", fps: 240 }),
    { resolution: "1920x1080", fps: 60 },
  );
  assert.deepEqual(
    resolveEntitledStreamProfile(entitlements, { resolution: "3840x2160", fps: 120 }),
    { resolution: "1920x1080", fps: 60 },
  );
  assert.equal(resolveEntitledStreamProfile([], { resolution: "1920x1080", fps: 60 }), null);
});

test("safe fallback entitlements resolve oversized requests to 1080p60", () => {
  assert.deepEqual(
    resolveEntitledStreamProfile(
      getSafeFallbackEntitledResolutions(),
      { resolution: "3840x2160", fps: 240 },
    ),
    SAFE_FALLBACK_STREAM_PROFILE,
  );
});
