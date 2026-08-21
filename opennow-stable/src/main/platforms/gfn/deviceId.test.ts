/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";

import { toCloudMatchDeviceHashId } from "./deviceId";

test("CloudMatch device ids are SHA-256 hex of the stable UUID", () => {
  const uuid = "11111111-1111-4111-8111-111111111111";
  const expected = createHash("sha256").update(uuid, "utf8").digest("hex");
  assert.equal(toCloudMatchDeviceHashId(uuid), expected);
  assert.match(toCloudMatchDeviceHashId(uuid), /^[0-9a-f]{64}$/);
});

test("CloudMatch device ids pass through existing 64-char hashes", () => {
  const hash = "eb3d00f59d2e42dafddbb00648b14be24a0f9e262bc5ba50d853a019301b03fc";
  assert.equal(toCloudMatchDeviceHashId(hash), hash);
  assert.equal(toCloudMatchDeviceHashId(hash.toUpperCase()), hash);
});
