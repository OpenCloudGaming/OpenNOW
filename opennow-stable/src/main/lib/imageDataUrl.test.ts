/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import {
  assertSafeRecordingId,
  assertSafeScreenshotId,
  buildScreenshotDataUrl,
  dataUrlToBuffer,
  sanitizeTitleForFileName,
} from "./imageDataUrl";

test("sanitizeTitleForFileName produces safe slug", () => {
  assert.equal(sanitizeTitleForFileName("  My Game: Special!  "), "my-game-special");
  assert.equal(sanitizeTitleForFileName(undefined), "stream");
});

test("dataUrlToBuffer parses png payload", () => {
  const input =
    "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
  const { ext, buffer } = dataUrlToBuffer(input);
  assert.equal(ext, "png");
  assert.ok(buffer.length > 0);
});

test("dataUrlToBuffer rejects invalid payload", () => {
  assert.throws(() => dataUrlToBuffer("not-a-data-url"), /Invalid screenshot payload/);
});

test("buildScreenshotDataUrl round-trips with png", () => {
  const buf = Buffer.from([0x89, 0x50]);
  const url = buildScreenshotDataUrl("png", buf);
  assert.ok(url.startsWith("data:image/png;base64,"));
});

test("assertSafeScreenshotId rejects path traversal", () => {
  assert.throws(() => assertSafeScreenshotId("../x"), /Invalid screenshot id/);
});

test("assertSafeRecordingId rejects path traversal", () => {
  assert.throws(() => assertSafeRecordingId("a/../b"), /Invalid recording id/);
});
