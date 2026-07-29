/// <reference types="node" />

import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import type { AuthSession } from "@shared/gfn";

import type { SafeStorageLike } from "../../../security/encryptedJsonFile";
import { AccountManager } from "./accountManager";
import { ConsoleProfileStore } from "./consoleProfileStore";
import { PersistedAccountState } from "./persistedAccountState";

const passthroughCrypto: SafeStorageLike = {
  isEncryptionAvailable: () => false,
  encryptString: (plainText) => Buffer.from(plainText, "utf8"),
  decryptString: (encrypted) => encrypted.toString("utf8"),
};

function session(userId: string): AuthSession {
  return {
    provider: {
      idpId: "nvidia",
      code: "NVIDIA",
      displayName: "NVIDIA",
      streamingServiceUrl: "https://provider.example/",
      priority: 0,
    },
    tokens: {
      accessToken: `${userId}-access`,
      expiresAt: Date.now() + 60_000,
    },
    user: {
      userId,
      displayName: userId,
      membershipTier: "FREE",
    },
  };
}

test("AccountManager removes an incomplete switched account and restores the prior account", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "opennow-auth-account-"));
  t.after(() => rm(directory, { recursive: true, force: true }));

  const statePath = join(directory, "auth.json");
  const state = new PersistedAccountState(statePath);
  const first = session("first");
  const second = session("second");
  state.accounts.setSession(first);
  state.accounts.setSession(second);
  state.accounts.setActiveAccount("first");

  let cacheClears = 0;
  const manager = new AccountManager(state, () => {
    cacheClears += 1;
  });

  await assert.rejects(
    () => manager.switchAccount("second", async () => ({
      session: second,
      refresh: {
        attempted: true,
        forced: true,
        outcome: "missing_refresh_token",
        message: "No refresh token available.",
      },
    })),
    /Saved login for this account is incomplete/,
  );

  assert.equal(state.accounts.getSession()?.user.userId, "first");
  assert.equal(state.accounts.hasAccount("second"), false);
  assert.equal(cacheClears, 3);

  const persisted = JSON.parse(await readFile(statePath, "utf8")) as {
    activeUserId: string | null;
    sessions: AuthSession[];
  };
  assert.equal(persisted.activeUserId, "first");
  assert.deepEqual(persisted.sessions.map((saved) => saved.user.userId), ["first"]);
});

test("AccountManager reports hasPin per account and never leaks the hash", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "opennow-auth-account-"));
  t.after(() => rm(directory, { recursive: true, force: true }));

  const state = new PersistedAccountState(join(directory, "auth.json"));
  state.accounts.setSession(session("locked"));
  state.accounts.setSession(session("open"));

  const profiles = new ConsoleProfileStore(join(directory, "console-profiles.json"), passthroughCrypto);
  await profiles.initialize();
  await profiles.setPin("locked", "1234");

  const manager = new AccountManager(state, () => {}, profiles);
  const accounts = manager.getSavedAccounts();

  assert.equal(accounts.find((account) => account.userId === "locked")?.hasPin, true);
  assert.equal(accounts.find((account) => account.userId === "open")?.hasPin, false);
  for (const account of accounts) {
    assert.deepEqual(
      Object.keys(account).filter((key) => /pin|hash|salt/i.test(key)),
      ["hasPin"],
      "only the boolean may cross the process boundary",
    );
  }
});

test("AccountManager defaults hasPin to false without a lock store", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "opennow-auth-account-"));
  t.after(() => rm(directory, { recursive: true, force: true }));

  const state = new PersistedAccountState(join(directory, "auth.json"));
  state.accounts.setSession(session("solo"));

  const manager = new AccountManager(state, () => {});
  assert.deepEqual(manager.getSavedAccounts().map((account) => account.hasPin), [false]);
});

test("AccountManager forgets PIN locks when accounts go away", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "opennow-auth-account-"));
  t.after(() => rm(directory, { recursive: true, force: true }));

  const state = new PersistedAccountState(join(directory, "auth.json"));
  state.accounts.setSession(session("first"));
  state.accounts.setSession(session("second"));

  const profilesPath = join(directory, "console-profiles.json");
  const profiles = new ConsoleProfileStore(profilesPath, passthroughCrypto);
  await profiles.initialize();
  await profiles.setPin("first", "1111");
  await profiles.setPin("second", "2222");

  const manager = new AccountManager(state, () => {}, profiles);

  await manager.removeAccount("first");
  assert.equal(profiles.hasPin("first"), false, "removeAccount must drop the lock");
  assert.equal(profiles.hasPin("second"), true);

  await manager.logoutAll();
  assert.equal(profiles.hasPin("second"), false, "logoutAll must drop every lock");

  // The removal is durable, not just in-memory.
  const reopened = new ConsoleProfileStore(profilesPath, passthroughCrypto);
  await reopened.initialize();
  assert.equal(reopened.hasPin("first"), false);
  assert.equal(reopened.hasPin("second"), false);
});

test("AccountManager forgets the active account's PIN on logout", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "opennow-auth-account-"));
  t.after(() => rm(directory, { recursive: true, force: true }));

  const state = new PersistedAccountState(join(directory, "auth.json"));
  state.accounts.setSession(session("first"));
  state.accounts.setSession(session("second"));
  state.accounts.setActiveAccount("second");

  const profiles = new ConsoleProfileStore(join(directory, "console-profiles.json"), passthroughCrypto);
  await profiles.initialize();
  await profiles.setPin("first", "1111");
  await profiles.setPin("second", "2222");

  await new AccountManager(state, () => {}, profiles).logout();

  assert.equal(profiles.hasPin("second"), false);
  assert.equal(profiles.hasPin("first"), true, "other profiles keep their locks");
});
