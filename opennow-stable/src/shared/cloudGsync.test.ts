/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import {
  resolveCloudGsync,
  unsupportedNativeCloudGsyncCapabilities,
  type NativeCloudGsyncCapabilities,
} from "./cloudGsync";

const vrrCapabilities: NativeCloudGsyncCapabilities = {
  platformSupportsCloudGsync: true,
  isVrrCapableDisplay: true,
  isGsyncDisplay: true,
  minimumFpsForCloudGsync: 60,
  minimumFpsForReflexWithoutVrr: 120,
  detectionSource: "native",
};

test("user off always disables Cloud G-Sync", () => {
  const result = resolveCloudGsync({
    userRequested: false,
    fps: 240,
    clientMode: "native",
    nativeBackendAvailable: true,
    capabilities: vrrCapabilities,
  });

  assert.equal(result.enabled, false);
  assert.equal(result.reason, "user-disabled");
});

test("fps below Cloud G-Sync minimum disables it", () => {
  const result = resolveCloudGsync({
    userRequested: true,
    fps: 59,
    clientMode: "native",
    nativeBackendAvailable: true,
    capabilities: vrrCapabilities,
  });

  assert.equal(result.enabled, false);
  assert.equal(result.reason, "fps-too-low");
});

test("unsupported VRR display disables native Cloud G-Sync", () => {
  const result = resolveCloudGsync({
    userRequested: true,
    fps: 240,
    clientMode: "native",
    nativeBackendAvailable: true,
    capabilities: {
      ...vrrCapabilities,
      isVrrCapableDisplay: false,
      isGsyncDisplay: false,
      detectionSource: "unsupported",
    },
  });

  assert.equal(result.enabled, false);
  assert.equal(result.reason, "unsupported-display");
});

test("VRR supported plus user on plus 60 fps enables Cloud G-Sync", () => {
  const result = resolveCloudGsync({
    userRequested: true,
    fps: 60,
    clientMode: "native",
    nativeBackendAvailable: true,
    capabilities: vrrCapabilities,
  });

  assert.equal(result.enabled, true);
  assert.equal(result.reflexEnabled, true);
  assert.equal(result.reason, "enabled");
});

test("Reflex enables below 120 fps when Cloud G-Sync enables", () => {
  const result = resolveCloudGsync({
    userRequested: true,
    fps: 60,
    clientMode: "native",
    nativeBackendAvailable: true,
    capabilities: vrrCapabilities,
  });

  assert.equal(result.enabled, true);
  assert.equal(result.reflexEnabled, true);
});

test("Reflex still uses 120 fps threshold when Cloud G-Sync is disabled", () => {
  const belowThreshold = resolveCloudGsync({
    userRequested: true,
    fps: 119,
    clientMode: "native",
    nativeBackendAvailable: true,
    capabilities: unsupportedNativeCloudGsyncCapabilities("test"),
  });
  const atThreshold = resolveCloudGsync({
    userRequested: true,
    fps: 120,
    clientMode: "native",
    nativeBackendAvailable: true,
    capabilities: unsupportedNativeCloudGsyncCapabilities("test"),
  });

  assert.equal(belowThreshold.reflexEnabled, false);
  assert.equal(atThreshold.reflexEnabled, true);
});

test("web mode keeps Reflex on the fps threshold even when Cloud G-Sync is requested", () => {
  const result = resolveCloudGsync({
    userRequested: true,
    fps: 60,
    clientMode: "web",
    nativeBackendAvailable: false,
  });

  assert.equal(result.enabled, true);
  assert.equal(result.reflexEnabled, false);
  assert.equal(result.reason, "web-mode");
});

test("force-enable bypasses display detection but still requires user setting and fps threshold", () => {
  const forced = resolveCloudGsync({
    userRequested: true,
    fps: 60,
    clientMode: "native",
    nativeBackendAvailable: true,
    capabilities: unsupportedNativeCloudGsyncCapabilities("test"),
    override: "1",
  });
  const forcedLowFps = resolveCloudGsync({
    userRequested: true,
    fps: 59,
    clientMode: "native",
    nativeBackendAvailable: true,
    capabilities: unsupportedNativeCloudGsyncCapabilities("test"),
    override: "1",
  });
  const forcedUserOff = resolveCloudGsync({
    userRequested: false,
    fps: 240,
    clientMode: "native",
    nativeBackendAvailable: true,
    capabilities: unsupportedNativeCloudGsyncCapabilities("test"),
    override: "1",
  });

  assert.equal(forced.enabled, true);
  assert.equal(forced.reason, "force-enabled");
  assert.equal(forcedLowFps.enabled, false);
  assert.equal(forcedLowFps.reason, "fps-too-low");
  assert.equal(forcedUserOff.enabled, false);
  assert.equal(forcedUserOff.reason, "user-disabled");
});
