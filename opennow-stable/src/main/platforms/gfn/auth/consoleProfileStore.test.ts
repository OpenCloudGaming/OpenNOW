import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import type { SafeStorageLike } from "../../../security/encryptedJsonFile";
import { ConsoleProfileStore } from "./consoleProfileStore";
import { PIN_LOCKOUT_STEPS_MS, PIN_MAX_ATTEMPTS } from "./pinPolicy";

const USER = "user-1";

function createFakeCrypto(available = true): SafeStorageLike {
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

async function tempPath(): Promise<string> {
  const dir = await mkdtemp(join(tmpdir(), "opennow-console-profiles-"));
  return join(dir, "console-profiles.json");
}

async function openStore(path: string, available = true): Promise<ConsoleProfileStore> {
  const store = new ConsoleProfileStore(path, createFakeCrypto(available));
  await store.initialize();
  return store;
}

test("treats a missing file as no PINs without throwing", async () => {
  const store = await openStore(await tempPath());
  assert.equal(store.hasPin(USER), false);
  assert.deepEqual(store.getStatus(USER, 0), {
    userId: USER,
    hasPin: false,
    lockedUntilMs: null,
    remainingAttempts: PIN_MAX_ATTEMPTS,
  });
});

test("verifying an account with no PIN succeeds", async () => {
  const store = await openStore(await tempPath());
  const result = await store.verifyPin(USER, "0000", 0);
  assert.equal(result.ok, true);
  assert.equal(result.reason, "no_pin_set");
});

test("persists a PIN across store instances", async () => {
  const path = await tempPath();
  const first = await openStore(path);
  assert.equal((await first.setPin(USER, "4821")).ok, true);

  const second = await openStore(path);
  assert.equal(second.hasPin(USER), true);
  assert.equal((await second.verifyPin(USER, "4821", 0)).ok, true);
  assert.equal((await second.verifyPin(USER, "0000", 0)).ok, false);
});

test("rejects a malformed PIN", async () => {
  const store = await openStore(await tempPath());
  const result = await store.setPin(USER, "12");
  assert.equal(result.ok, false);
  assert.equal(result.reason, "invalid_format");
  assert.equal(store.hasPin(USER), false);
});

test("replacing a PIN requires the current one", async () => {
  const store = await openStore(await tempPath());
  await store.setPin(USER, "1111");

  const missing = await store.setPin(USER, "2222");
  assert.equal(missing.ok, false);
  assert.equal(missing.reason, "invalid_format");

  const wrong = await store.setPin(USER, "2222", "9999");
  assert.equal(wrong.ok, false);
  assert.equal(wrong.reason, "invalid_pin");
  assert.equal((await store.verifyPin(USER, "1111", 0)).ok, true, "the old PIN must still work");

  const correct = await store.setPin(USER, "2222", "1111");
  assert.equal(correct.ok, true);
  assert.equal((await store.verifyPin(USER, "2222", 0)).ok, true);
});

test("clearing a PIN requires the current one", async () => {
  const store = await openStore(await tempPath());
  await store.setPin(USER, "1111");

  const wrong = await store.clearPin(USER, "2222");
  assert.equal(wrong.ok, false);
  assert.equal(wrong.reason, "invalid_pin");
  assert.equal(store.hasPin(USER), true);

  const correct = await store.clearPin(USER, "1111");
  assert.equal(correct.ok, true);
  assert.equal(correct.hasPin, false);
  assert.equal(store.hasPin(USER), false);
});

test("clearing when no PIN is set reports no_pin_set", async () => {
  const store = await openStore(await tempPath());
  const result = await store.clearPin(USER, "1111");
  assert.equal(result.ok, false);
  assert.equal(result.reason, "no_pin_set");
});

test("locks out after repeated failures and reports the expiry", async () => {
  const store = await openStore(await tempPath());
  await store.setPin(USER, "1111");

  for (let attempt = 0; attempt < PIN_MAX_ATTEMPTS - 1; attempt += 1) {
    const result = await store.verifyPin(USER, "0000", 1_000);
    assert.equal(result.reason, "invalid_pin");
  }

  const locked = await store.verifyPin(USER, "0000", 1_000);
  assert.equal(locked.ok, false);
  assert.equal(locked.reason, "locked_out");
  assert.equal(locked.lockedUntilMs, 1_000 + PIN_LOCKOUT_STEPS_MS[0]);

  const stillLocked = await store.verifyPin(USER, "1111", 1_000);
  assert.equal(stillLocked.ok, false, "the correct PIN must be refused while locked out");
});

test("lockout survives a store restart", async () => {
  const path = await tempPath();
  const first = await openStore(path);
  await first.setPin(USER, "1111");
  for (let attempt = 0; attempt < PIN_MAX_ATTEMPTS; attempt += 1) {
    await first.verifyPin(USER, "0000", 1_000);
  }

  const second = await openStore(path);
  const status = second.getStatus(USER, 1_000);
  assert.equal(status.lockedUntilMs, 1_000 + PIN_LOCKOUT_STEPS_MS[0]);
  assert.equal((await second.verifyPin(USER, "1111", 1_000)).reason, "locked_out");
});

test("a successful verify clears the failure counter", async () => {
  const store = await openStore(await tempPath());
  await store.setPin(USER, "1111");
  await store.verifyPin(USER, "0000", 0);
  await store.verifyPin(USER, "0000", 0);

  assert.equal((await store.verifyPin(USER, "1111", 0)).ok, true);
  assert.equal(store.getStatus(USER, 0).remainingAttempts, PIN_MAX_ATTEMPTS);
});

test("forgetUser drops the lock and persists the removal", async () => {
  const path = await tempPath();
  const first = await openStore(path);
  await first.setPin(USER, "1111");
  await first.forgetUser(USER);
  assert.equal(first.hasPin(USER), false);

  const second = await openStore(path);
  assert.equal(second.hasPin(USER), false);
});

test("round-trips when OS encryption is unavailable", async () => {
  const path = await tempPath();
  const first = await openStore(path, false);
  await first.setPin(USER, "1111");

  assert.equal(JSON.parse(await readFile(path, "utf8")).encrypted, false);
  const second = await openStore(path, false);
  assert.equal((await second.verifyPin(USER, "1111", 0)).ok, true);
});

test("fails open on a corrupt payload and rewrites cleanly", async () => {
  const path = await tempPath();
  const first = await openStore(path);
  await first.setPin(USER, "1111");

  await writeFile(path, JSON.stringify({ version: 1, encrypted: true, payload: "Y29ycnVwdA==" }), "utf8");

  const second = await openStore(path);
  assert.equal(second.hasPin(USER), false, "an undecryptable record must not strand the user");
  assert.equal((await second.setPin(USER, "2222")).ok, true);
  assert.equal((await second.verifyPin(USER, "2222", 0)).ok, true);
});

test("keeps profiles independent", async () => {
  const store = await openStore(await tempPath());
  await store.setPin("a", "1111");
  await store.setPin("b", "2222");

  assert.equal((await store.verifyPin("a", "1111", 0)).ok, true);
  assert.equal((await store.verifyPin("b", "1111", 0)).ok, false);
  await store.forgetUser("a");
  assert.equal(store.hasPin("b"), true);
});
