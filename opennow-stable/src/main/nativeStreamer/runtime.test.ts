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
    baseEnv: {
      GST_PLUGIN_PATH: "/old/plugins",
      DISPLAY: ":1",
      OPENNOW_NATIVE_VIDEO_BACKEND: "software",
    },
    platform: "linux",
    arch: "arm64",
    userDataPath: "/tmp/opennow-test",
    protocolVersion: 4,
    videoBackendPreference: "auto",
    externalRendererEnabled: false,
    cloudGsyncMode: "disabled",
    d3dFullscreenMode: "auto",
  });

  assert.equal(result.env.GST_PLUGIN_PATH, undefined);
  assert.equal(result.env.OPENNOW_NATIVE_STREAMER_PROTOCOL, "4");
  assert.equal(result.env.OPENNOW_NATIVE_INPUT_OWNER, "electron");
  assert.equal(result.env.OPENNOW_NATIVE_VIDEO_BACKEND, "auto");
  assert.equal(result.env.SDL_VIDEODRIVER, "x11");
  assert.equal(result.env.OPENNOW_NATIVE_WINDOW_SYSTEM, "x11");
  assert.equal(result.runtimeStatus.selfContained, true);
  assert.match(result.runtimeStatus.message, /self-contained/);
});

test("external renderer makes the native SDL window the input owner", () => {
  const result = createNativeStreamerRuntimeEnvironment({
    executablePath: "C:\\OpenNOW\\opennow-streamer.exe",
    baseEnv: {},
    platform: "win32",
    arch: "x64",
    userDataPath: "C:\\OpenNOW",
    protocolVersion: 4,
    videoBackendPreference: "auto",
    externalRendererEnabled: true,
    cloudGsyncMode: "auto",
    d3dFullscreenMode: "auto",
  });

  assert.equal(result.env.OPENNOW_NATIVE_EXTERNAL_RENDERER, "1");
  assert.equal(result.env.OPENNOW_NATIVE_INPUT_OWNER, "native");
});

test("explicit Linux video backend preference is passed to the child", () => {
  const result = createNativeStreamerRuntimeEnvironment({
    executablePath: "/tmp/opennow-streamer",
    baseEnv: { DISPLAY: ":1" },
    platform: "linux",
    arch: "x64",
    userDataPath: "/tmp/opennow-test",
    protocolVersion: 4,
    videoBackendPreference: "v4l2",
    externalRendererEnabled: false,
    cloudGsyncMode: "auto",
    d3dFullscreenMode: "auto",
  });

  assert.equal(result.env.OPENNOW_NATIVE_VIDEO_BACKEND, "v4l2");
});

test("Cloud G-SYNC uses a top-level native output on X11", () => {
  const result = createNativeStreamerRuntimeEnvironment({
    executablePath: "/tmp/opennow-streamer",
    baseEnv: { DISPLAY: ":1", XDG_SESSION_TYPE: "x11" },
    platform: "linux",
    arch: "x64",
    userDataPath: "/tmp/opennow-test",
    protocolVersion: 4,
    videoBackendPreference: "auto",
    externalRendererEnabled: false,
    cloudGsyncMode: "auto",
    d3dFullscreenMode: "auto",
  });

  assert.equal(result.env.OPENNOW_NATIVE_EXTERNAL_RENDERER, "1");
  assert.equal(result.env.OPENNOW_NATIVE_INPUT_OWNER, "native");
  assert.match(result.runtimeStatus.message, /Cloud G-SYNC\/VRR/);
});

test("Wayland uses a native top-level renderer and owns its input", () => {
  const result = createNativeStreamerRuntimeEnvironment({
    executablePath: "/tmp/opennow-streamer",
    baseEnv: { WAYLAND_DISPLAY: "wayland-0", XDG_SESSION_TYPE: "wayland" },
    platform: "linux",
    arch: "arm64",
    userDataPath: "/tmp/opennow-test",
    protocolVersion: 4,
    videoBackendPreference: "auto",
    externalRendererEnabled: false,
    linuxOzonePlatform: "wayland",
    cloudGsyncMode: "auto",
    d3dFullscreenMode: "auto",
  });

  assert.equal(result.env.SDL_VIDEODRIVER, "wayland");
  assert.equal(result.env.OPENNOW_NATIVE_WINDOW_SYSTEM, "wayland");
  assert.equal(result.env.OPENNOW_NATIVE_EXTERNAL_RENDERER, "1");
  assert.equal(result.env.OPENNOW_NATIVE_INPUT_OWNER, "native");
  assert.match(result.runtimeStatus.message, /Wayland output window/);
});

test("runtime executable names and platform keys accept explicit platforms", () => {
  assert.equal(nativeStreamerExecutableName("win32"), "opennow-streamer.exe");
  assert.equal(nativeStreamerExecutableName("linux"), "opennow-streamer");
  assert.equal(nativeStreamerPlatformKey("darwin", "arm64"), "darwin-arm64");
});
