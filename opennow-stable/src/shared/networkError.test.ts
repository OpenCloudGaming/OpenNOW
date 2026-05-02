/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import { enrichErrorForIpc, formatErrorChainForLog } from "./networkError";

test("formatErrorChainForLog: simple Error", () => {
  assert.equal(formatErrorChainForLog(new Error("hello")), "Error: hello");
});

test("formatErrorChainForLog: non-Error", () => {
  assert.equal(formatErrorChainForLog("plain"), "plain");
  assert.equal(formatErrorChainForLog(42), "42");
});

test("formatErrorChainForLog: TypeError fetch failed with errno cause", () => {
  const inner = new Error("getaddrinfo ENOTFOUND example.invalid") as Error & { code?: string; syscall?: string; hostname?: string };
  inner.code = "ENOTFOUND";
  inner.syscall = "getaddrinfo";
  inner.hostname = "example.invalid";

  const outer = new TypeError("fetch failed");
  outer.cause = inner;

  const log = formatErrorChainForLog(outer);
  assert.match(log, /TypeError: fetch failed/);
  assert.match(log, /caused by:/);
  assert.match(log, /ENOTFOUND/);
  assert.match(log, /getaddrinfo/);
  assert.match(log, /example\.invalid/);
});

test("enrichErrorForIpc: returns same instance when there is no cause", () => {
  const err = new Error("something");
  assert.strictEqual(enrichErrorForIpc(err), err);
});

test("enrichErrorForIpc: folds cause into message for IPC", () => {
  const inner = new Error("deep") as Error & { code?: string };
  inner.code = "ECONNREFUSED";

  const outer = new TypeError("fetch failed");
  outer.cause = inner;

  const enriched = enrichErrorForIpc(outer);
  assert.notStrictEqual(enriched, outer);
  assert.match(enriched.message, /fetch failed/);
  assert.match(enriched.message, /ECONNREFUSED/);
  assert.match(enriched.message, /\|/);
});

test("enrichErrorForIpc: wraps non-Error", () => {
  const e = enrichErrorForIpc("oops");
  assert.equal(e.message, "oops");
});
