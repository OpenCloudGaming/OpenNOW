import assert from "node:assert/strict";
import test from "node:test";

import type { NativeVideoBackendCapability } from "@shared/gfn";
import {
  createNativeStreamerStatus,
  resolveActiveVideoBackend,
} from "./capabilities";

function backend(
  name: string,
  platform: string,
  available = true,
): NativeVideoBackendCapability {
  return {
    backend: name,
    platform,
    available,
    codecs: [{ codec: "h264", available: true }],
    zeroCopyModes: [],
  };
}

test("capability selection honors an available explicit preference", () => {
  const backends = [
    backend("d3d12", "windows"),
    backend("software", "cross-platform"),
  ];

  assert.equal(resolveActiveVideoBackend(backends, "software", "win32")?.backend, "software");
});

test("capability selection uses injected platform before cross-platform fallback", () => {
  const backends = [
    backend("d3d12", "windows"),
    backend("vaapi", "linux"),
    backend("software", "cross-platform"),
  ];

  assert.equal(resolveActiveVideoBackend(backends, "auto", "linux")?.backend, "vaapi");
  assert.equal(resolveActiveVideoBackend(backends, "auto", "darwin")?.backend, "software");
});

test("capability selection skips unavailable preferred and platform backends", () => {
  const backends = [
    backend("vaapi", "linux", false),
    backend("software", "cross-platform"),
  ];

  assert.equal(resolveActiveVideoBackend(backends, "vaapi", "linux")?.backend, "software");
});

test("status formatting reports selected video path and codec summary", () => {
  const status = createNativeStreamerStatus({
    protocolVersion: 4,
    backend: "native",
    supportsOfferAnswer: true,
    supportsRemoteIce: true,
    supportsLocalIce: true,
    supportsInput: true,
    supportsVideoDecode: true,
    supportsVideoPresent: true,
    videoBackends: [backend("vaapi", "linux")],
  }, {
    source: "self-contained",
    selfContained: true,
    message: "Self-contained runtime",
  }, "auto", "linux");

  assert.equal(status.activeVideoBackend?.backend, "vaapi");
  assert.equal(status.codecSummary, "H.264");
  assert.match(status.message, /Video path: VAAPI/);
});
