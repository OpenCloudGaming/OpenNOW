import test from "node:test";
import assert from "node:assert/strict";

import { hashPin, verifyPinHash } from "./pinHash";

test("round-trips a correct PIN", async () => {
  const record = await hashPin("4821");
  assert.equal(await verifyPinHash("4821", record), true);
});

test("rejects a wrong PIN", async () => {
  const record = await hashPin("4821");
  assert.equal(await verifyPinHash("4822", record), false);
});

test("salts every hash, so the same PIN never produces the same record", async () => {
  const first = await hashPin("0000");
  const second = await hashPin("0000");
  assert.notEqual(first.salt, second.salt);
  assert.notEqual(first.hash, second.hash);
  assert.equal(await verifyPinHash("0000", first), true);
  assert.equal(await verifyPinHash("0000", second), true);
});

test("returns false rather than throwing on a truncated hash", async () => {
  const record = await hashPin("1234");
  const truncated = { ...record, hash: record.hash.slice(0, 10) };
  assert.equal(await verifyPinHash("1234", truncated), false);
});

test("returns false rather than throwing on a tampered or empty record", async () => {
  const record = await hashPin("1234");
  assert.equal(await verifyPinHash("1234", { ...record, salt: "" }), false);
  assert.equal(await verifyPinHash("1234", { ...record, hash: "" }), false);
  assert.equal(await verifyPinHash("1234", { ...record, algorithm: "bcrypt" as never }), false);
  assert.equal(await verifyPinHash("1234", { ...record, version: 2 as never }), false);
  assert.equal(await verifyPinHash("1234", { ...record, keylen: -1 }), false);
});

test("records the parameters used so future changes stay verifiable", async () => {
  const record = await hashPin("1234");
  assert.equal(record.algorithm, "scrypt");
  assert.equal(record.version, 1);
  assert.equal(record.N, 2 ** 15);
  assert.equal(record.keylen, 32);
  assert.equal(Buffer.from(record.salt, "base64").length, 16);
  assert.equal(Buffer.from(record.hash, "base64").length, 32);
});
