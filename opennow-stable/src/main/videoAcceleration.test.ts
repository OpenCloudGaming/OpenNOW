import assert from "node:assert/strict";
import test from "node:test";

import { buildVideoAccelerationCommandLine } from "./videoAcceleration";

test("enables NVIDIA VA-API Chromium flags for Linux desktop hardware decode", () => {
  const commandLine = buildVideoAccelerationCommandLine(
    { decoderPreference: "hardware", encoderPreference: "auto" },
    "linux",
    "x64",
  );

  assert.ok(commandLine.enableFeatures.includes("VaapiVideoDecoder"));
  assert.ok(commandLine.enableFeatures.includes("AcceleratedVideoDecodeLinuxGL"));
  assert.ok(commandLine.enableFeatures.includes("AcceleratedVideoDecodeLinuxZeroCopyGL"));
  assert.ok(commandLine.enableFeatures.includes("VaapiOnNvidiaGPUs"));
  assert.ok(commandLine.enableFeatures.includes("VaapiIgnoreDriverChecks"));
  assert.ok(commandLine.disableFeatures.includes("UseChromeOSDirectVideoDecoder"));
  assert.equal(commandLine.switches["enable-accelerated-video-decode"], true);
});

test("does not enable Linux VA-API decoder flags when software decode is forced", () => {
  const commandLine = buildVideoAccelerationCommandLine(
    { decoderPreference: "software", encoderPreference: "software" },
    "linux",
    "x64",
  );

  assert.equal(commandLine.enableFeatures.includes("VaapiVideoDecoder"), false);
  assert.equal(commandLine.enableFeatures.includes("VaapiOnNvidiaGPUs"), false);
  assert.equal(commandLine.switches["disable-accelerated-video-decode"], true);
  assert.equal(commandLine.switches["disable-accelerated-video-encode"], true);
});

test("enables VideoToolbox HW decode and encode on Apple Silicon (darwin arm64)", () => {
  const commandLine = buildVideoAccelerationCommandLine(
    { decoderPreference: "hardware", encoderPreference: "hardware" },
    "darwin",
    "arm64",
  );

  assert.ok(commandLine.enableFeatures.includes("VideoToolboxVideoDecoder"));
  assert.ok(commandLine.enableFeatures.includes("VideoToolboxVideoEncoder"));
  assert.equal(commandLine.switches["enable-accelerated-video-decode"], true);
  assert.equal(commandLine.switches["enable-accelerated-video-encode"], true);
  // Linux-only flags must not appear
  assert.equal(commandLine.enableFeatures.includes("AcceleratedVideoDecodeLinuxGL"), false);
  assert.equal(commandLine.enableFeatures.includes("VaapiVideoDecoder"), false);
});

test("enables VideoToolbox decode but not encode on Apple Silicon when encoder is auto", () => {
  const commandLine = buildVideoAccelerationCommandLine(
    { decoderPreference: "auto", encoderPreference: "auto" },
    "darwin",
    "arm64",
  );

  assert.ok(commandLine.enableFeatures.includes("VideoToolboxVideoDecoder"));
  assert.ok(commandLine.enableFeatures.includes("VideoToolboxVideoEncoder"));
  // auto preference => no explicit enable/disable switches
  assert.equal(commandLine.switches["enable-accelerated-video-decode"], undefined);
  assert.equal(commandLine.switches["disable-accelerated-video-decode"], undefined);
});

test("disables VideoToolbox features on darwin arm64 when software is forced", () => {
  const commandLine = buildVideoAccelerationCommandLine(
    { decoderPreference: "software", encoderPreference: "software" },
    "darwin",
    "arm64",
  );

  assert.equal(commandLine.enableFeatures.includes("VideoToolboxVideoDecoder"), false);
  assert.equal(commandLine.enableFeatures.includes("VideoToolboxVideoEncoder"), false);
  assert.equal(commandLine.switches["disable-accelerated-video-decode"], true);
  assert.equal(commandLine.switches["disable-accelerated-video-encode"], true);
});

test("enables VideoToolbox decode only on Intel Mac (darwin x64)", () => {
  const commandLine = buildVideoAccelerationCommandLine(
    { decoderPreference: "hardware", encoderPreference: "hardware" },
    "darwin",
    "x64",
  );

  assert.ok(commandLine.enableFeatures.includes("VideoToolboxVideoDecoder"));
  // Encode via VideoToolbox not enabled on Intel Mac
  assert.equal(commandLine.enableFeatures.includes("VideoToolboxVideoEncoder"), false);
  assert.equal(commandLine.switches["enable-accelerated-video-decode"], true);
  assert.equal(commandLine.switches["enable-accelerated-video-encode"], true);
  // Linux-only flags must not appear
  assert.equal(commandLine.enableFeatures.includes("AcceleratedVideoDecodeLinuxGL"), false);
  assert.equal(commandLine.enableFeatures.includes("VaapiVideoDecoder"), false);
});

test("does not enable VideoToolbox decoder on Intel Mac when software decode is forced", () => {
  const commandLine = buildVideoAccelerationCommandLine(
    { decoderPreference: "software", encoderPreference: "auto" },
    "darwin",
    "x64",
  );

  assert.equal(commandLine.enableFeatures.includes("VideoToolboxVideoDecoder"), false);
  assert.equal(commandLine.enableFeatures.includes("VideoToolboxVideoEncoder"), false);
  assert.equal(commandLine.switches["disable-accelerated-video-decode"], true);
});

test("enables MetalANGLE and UseEGLImageForMacVideoToolbox on Apple Silicon hardware decode", () => {
  const commandLine = buildVideoAccelerationCommandLine(
    { decoderPreference: "hardware", encoderPreference: "auto" },
    "darwin",
    "arm64",
  );

  assert.ok(commandLine.enableFeatures.includes("MetalANGLE"));
  assert.ok(commandLine.enableFeatures.includes("UseEGLImageForMacVideoToolbox"));
});

test("does not enable MetalANGLE or UseEGLImageForMacVideoToolbox on Apple Silicon when software decode is forced", () => {
  const commandLine = buildVideoAccelerationCommandLine(
    { decoderPreference: "software", encoderPreference: "software" },
    "darwin",
    "arm64",
  );

  assert.equal(commandLine.enableFeatures.includes("MetalANGLE"), false);
  assert.equal(commandLine.enableFeatures.includes("UseEGLImageForMacVideoToolbox"), false);
});

test("does not enable MetalANGLE or UseEGLImageForMacVideoToolbox on Intel Mac", () => {
  const commandLine = buildVideoAccelerationCommandLine(
    { decoderPreference: "hardware", encoderPreference: "hardware" },
    "darwin",
    "x64",
  );

  assert.equal(commandLine.enableFeatures.includes("MetalANGLE"), false);
  assert.equal(commandLine.enableFeatures.includes("UseEGLImageForMacVideoToolbox"), false);
});

test("always enables CanvasOopRasterization, Metal, and enable-gpu-rasterization on macOS (arm64)", () => {
  const commandLine = buildVideoAccelerationCommandLine(
    { decoderPreference: "auto", encoderPreference: "auto" },
    "darwin",
    "arm64",
  );

  assert.ok(commandLine.enableFeatures.includes("CanvasOopRasterization"));
  assert.ok(commandLine.enableFeatures.includes("Metal"));
  assert.equal(commandLine.switches["enable-gpu-rasterization"], true);
});

test("always enables CanvasOopRasterization, Metal, and enable-gpu-rasterization on macOS (x64)", () => {
  const commandLine = buildVideoAccelerationCommandLine(
    { decoderPreference: "software", encoderPreference: "software" },
    "darwin",
    "x64",
  );

  assert.ok(commandLine.enableFeatures.includes("CanvasOopRasterization"));
  assert.ok(commandLine.enableFeatures.includes("Metal"));
  assert.equal(commandLine.switches["enable-gpu-rasterization"], true);
});

test("does not enable macOS Metal flags on Linux or Windows", () => {
  const linuxCmd = buildVideoAccelerationCommandLine(
    { decoderPreference: "hardware", encoderPreference: "hardware" },
    "linux",
    "x64",
  );
  const win32Cmd = buildVideoAccelerationCommandLine(
    { decoderPreference: "hardware", encoderPreference: "hardware" },
    "win32",
    "x64",
  );

  for (const commandLine of [linuxCmd, win32Cmd]) {
    assert.equal(commandLine.enableFeatures.includes("CanvasOopRasterization"), false);
    assert.equal(commandLine.enableFeatures.includes("Metal"), false);
    assert.equal(commandLine.enableFeatures.includes("MetalANGLE"), false);
    assert.equal(commandLine.switches["enable-gpu-rasterization"], undefined);
  }
});
