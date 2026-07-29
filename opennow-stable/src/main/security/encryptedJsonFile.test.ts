import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { readEncryptedJson, writeEncryptedJson, type SafeStorageLike } from "./encryptedJsonFile";

/** Reversible stand-in for safeStorage so tests never load Electron. */
function createFakeCrypto(available: boolean): SafeStorageLike {
  return {
    isEncryptionAvailable: () => available,
    encryptString: (plainText) => Buffer.from(`enc:${plainText}`, "utf8"),
    decryptString: (encrypted) => {
      const text = encrypted.toString("utf8");
      if (!text.startsWith("enc:")) throw new Error("not encrypted by this fake");
      return text.slice(4);
    },
  };
}

async function tempFile(name: string): Promise<string> {
  const dir = await mkdtemp(join(tmpdir(), "opennow-encjson-"));
  return join(dir, name);
}

test("round-trips through the encrypted envelope", async () => {
  const path = await tempFile("state.json");
  const crypto = createFakeCrypto(true);
  await writeEncryptedJson(path, crypto, { hello: "world", count: 2 });

  const envelope = JSON.parse(await readFile(path, "utf8"));
  assert.equal(envelope.version, 1);
  assert.equal(envelope.encrypted, true);
  assert.ok(!envelope.payload.includes("world"), "ciphertext must not contain the plaintext");

  assert.deepEqual(await readEncryptedJson(path, crypto, null), { hello: "world", count: 2 });
});

test("falls back to a plaintext envelope when OS encryption is unavailable", async () => {
  const path = await tempFile("state.json");
  const crypto = createFakeCrypto(false);
  await writeEncryptedJson(path, crypto, { hello: "world" });

  const envelope = JSON.parse(await readFile(path, "utf8"));
  assert.equal(envelope.encrypted, false);
  assert.deepEqual(await readEncryptedJson(path, crypto, null), { hello: "world" });
});

test("returns the fallback for a missing file", async () => {
  const path = await tempFile("missing.json");
  assert.deepEqual(await readEncryptedJson(path, createFakeCrypto(true), { empty: true }), { empty: true });
});

test("fails open when decryption throws", async () => {
  const path = await tempFile("state.json");
  await writeFile(path, JSON.stringify({ version: 1, encrypted: true, payload: "bm90LWVuY3J5cHRlZA==" }), "utf8");
  assert.deepEqual(await readEncryptedJson(path, createFakeCrypto(true), { fallback: true }), { fallback: true });
});

test("fails open for malformed JSON and unknown envelope shapes", async () => {
  const badJson = await tempFile("bad.json");
  await writeFile(badJson, "{not json", "utf8");
  assert.deepEqual(await readEncryptedJson(badJson, createFakeCrypto(true), { fallback: true }), { fallback: true });

  const wrongShape = await tempFile("shape.json");
  await writeFile(wrongShape, JSON.stringify({ version: 99, payload: "x" }), "utf8");
  assert.deepEqual(await readEncryptedJson(wrongShape, createFakeCrypto(true), { fallback: true }), { fallback: true });
});

test("creates missing parent directories", async () => {
  const dir = await mkdtemp(join(tmpdir(), "opennow-encjson-"));
  const path = join(dir, "nested", "deeper", "state.json");
  await writeEncryptedJson(path, createFakeCrypto(true), { ok: true });
  assert.deepEqual(await readEncryptedJson(path, createFakeCrypto(true), null), { ok: true });
});
