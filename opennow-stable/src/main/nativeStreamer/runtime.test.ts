import assert from "node:assert/strict";
import test from "node:test";

import {
  createNativeStreamerRuntimeEnvironment,
  nativeStreamerExecutableName,
  nativeStreamerPlatformKey,
} from "./runtime";

test("v2 runtime is self-contained and removes inherited GStreamer variables", () => {
  const result = createNativeStreamerRuntimeEnvironment({
    executablePath: "/tmp/opennow-streamer",
    baseEnv: { GST_PLUGIN_PATH: "/old/plugins", DISPLAY: ":1" },
    platform: "linux",
    arch: "arm64",
    userDataPath: "/tmp/opennow-test",
    protocolVersion: 4,
    videoBackendPreference: "auto",
    externalRendererEnabled: false,
    cloudGsyncMode: "auto",
    d3dFullscreenMode: "auto",
  });

  assert.equal(result.env.GST_PLUGIN_PATH, undefined);
  assert.equal(result.env.OPENNOW_NATIVE_STREAMER_PROTOCOL, "4");
  assert.equal(result.runtimeStatus.selfContained, true);
  assert.match(result.runtimeStatus.message, /self-contained/);
});

test("runtime executable names and platform keys accept explicit platforms", () => {
  assert.equal(nativeStreamerExecutableName("win32"), "opennow-streamer.exe");
  assert.equal(nativeStreamerExecutableName("linux"), "opennow-streamer");
  assert.equal(nativeStreamerPlatformKey("darwin", "arm64"), "darwin-arm64");
});
