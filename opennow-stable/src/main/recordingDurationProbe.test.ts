import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

import {
  probeMp4DurationFromBuffer,
  probeWebmDurationFromBuffer,
} from "./recordingDurationProbe";

const __dirname = dirname(fileURLToPath(import.meta.url));

test("probeWebmDurationFromBuffer returns stable duration for VP8 fixture", async () => {
  const fixturePath = join(__dirname, "../../test/fixtures/probe-duration-sample.webm");
  const buf = await readFile(fixturePath);
  const ms = probeWebmDurationFromBuffer(buf, 999_999);
  assert.ok(ms >= 480 && ms <= 560, `expected ~520ms, got ${ms}`);
});

test("probeWebmDurationFromBuffer falls back on garbage", () => {
  const fb = 12_345;
  const ms = probeWebmDurationFromBuffer(Buffer.from("not a webm"), fb);
  assert.equal(ms, fb);
});

test("probeMp4DurationFromBuffer returns null for garbage", () => {
  assert.equal(probeMp4DurationFromBuffer(Buffer.from("xxxx")), null);
});

test("probeMp4DurationFromBuffer reads mvhd for H.264 fixture", async () => {
  const fixturePath = join(__dirname, "../../test/fixtures/probe-duration-sample.mp4");
  const buf = await readFile(fixturePath);
  const ms = probeMp4DurationFromBuffer(buf);
  assert.ok(ms !== null && ms >= 350 && ms <= 450, `expected ~400ms, got ${ms}`);
});
