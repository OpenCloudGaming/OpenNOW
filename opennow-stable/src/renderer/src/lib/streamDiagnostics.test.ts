import assert from "node:assert/strict";
import test from "node:test";

import { defaultDiagnostics } from "./streamDiagnostics";

test("default diagnostics do not claim unavailable native measurements", () => {
  const diagnostics = defaultDiagnostics();

  assert.equal(diagnostics.nativeRendererActive, false);
  assert.equal(diagnostics.inputReady, false);
  assert.equal(diagnostics.bitrateKbps, 0);
  assert.equal(diagnostics.targetBitrateKbps, 0);
  assert.equal(diagnostics.decodeFps, 0);
  assert.equal(diagnostics.renderFps, 0);
});
