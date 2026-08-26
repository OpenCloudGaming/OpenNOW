/// <reference types="node" />

import assert from "node:assert/strict";
import test from "node:test";

import { parseInputProtocolVersion } from "./inputHandshake";

test("parses full and compact NVST input protocol announcements", () => {
  assert.equal(parseInputProtocolVersion(new Uint8Array([0x0e, 0x02, 0x02, 0x00, 0x03, 0x00])), 3);
  assert.equal(parseInputProtocolVersion(new Uint8Array([0x0e, 0x02, 0x03, 0x00])), 3);
  assert.equal(parseInputProtocolVersion(new Uint8Array([0x0e, 0x03])), 0x030e);
});

test("rejects truncated and unrelated input messages", () => {
  assert.equal(parseInputProtocolVersion(new Uint8Array()), null);
  assert.equal(parseInputProtocolVersion(new Uint8Array([0x0e])), null);
  assert.equal(parseInputProtocolVersion(new Uint8Array([0x01, 0x02, 0x03, 0x00])), null);
});
