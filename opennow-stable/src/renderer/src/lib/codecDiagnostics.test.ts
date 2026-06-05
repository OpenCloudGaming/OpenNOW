import assert from "node:assert/strict";
import test from "node:test";

import { shouldShowLinuxHardwareCodecHint, type CodecTestResult } from "./codecDiagnostics";

function withNavigator(platform: string, userAgent: string, run: () => void): void {
  const original = Object.getOwnPropertyDescriptor(globalThis, "navigator");
  Object.defineProperty(globalThis, "navigator", {
    configurable: true,
    value: { platform, userAgent },
  });
  try {
    run();
  } finally {
    if (original) {
      Object.defineProperty(globalThis, "navigator", original);
    } else {
      delete (globalThis as { navigator?: Navigator }).navigator;
    }
  }
}

function codecResult(overrides: Partial<CodecTestResult> = {}): CodecTestResult {
  return {
    codec: "H264",
    webrtcSupported: true,
    decodeSupported: true,
    hwAccelerated: false,
    encodeSupported: false,
    encodeHwAccelerated: false,
    decodeVia: "Software (CPU)",
    encodeVia: "Unsupported",
    profiles: [],
    ...overrides,
  };
}

test("shows Linux hardware hint for software-only codec diagnostics", () => {
  withNavigator("Linux x86_64", "OpenNOW Linux", () => {
    assert.equal(shouldShowLinuxHardwareCodecHint([codecResult()]), true);
  });
});

test("does not show Linux hardware hint for GPU-backed diagnostics", () => {
  withNavigator("Linux x86_64", "OpenNOW Linux", () => {
    assert.equal(shouldShowLinuxHardwareCodecHint([codecResult({ hwAccelerated: true })]), false);
  });
});

test("does not show Linux hardware hint on non-Linux clients", () => {
  withNavigator("Win32", "OpenNOW Windows", () => {
    assert.equal(shouldShowLinuxHardwareCodecHint([codecResult()]), false);
  });
});
