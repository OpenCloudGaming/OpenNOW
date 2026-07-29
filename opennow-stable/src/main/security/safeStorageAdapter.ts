import { safeStorage } from "electron";

import type { SafeStorageLike } from "./encryptedJsonFile";

/**
 * Adapts Electron's `safeStorage` to the injectable interface used by encrypted
 * stores, so provider modules stay free of Electron wiring and tests can pass a
 * fake without importing Electron at all.
 *
 * Every call is deferred to invocation time: `safeStorage` is only usable after
 * `app.whenReady()`.
 */
export function createSafeStorageAdapter(): SafeStorageLike {
  return {
    isEncryptionAvailable: () => safeStorage.isEncryptionAvailable(),
    encryptString: (plainText) => safeStorage.encryptString(plainText),
    decryptString: (encrypted) => safeStorage.decryptString(encrypted),
  };
}
