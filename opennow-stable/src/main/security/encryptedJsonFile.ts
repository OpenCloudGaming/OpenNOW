import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname } from "node:path";

/**
 * Minimal slice of Electron's `safeStorage`, injected so tests never import
 * Electron and callers can degrade cleanly when OS encryption is unavailable.
 */
export interface SafeStorageLike {
  isEncryptionAvailable(): boolean;
  encryptString(plainText: string): Buffer;
  decryptString(encrypted: Buffer): string;
}

export interface EncryptedEnvelope {
  version: 1;
  encrypted: boolean;
  /** base64 ciphertext when encrypted, raw JSON text otherwise. */
  payload: string;
}

function isEnvelope(value: unknown): value is EncryptedEnvelope {
  if (typeof value !== "object" || value === null) return false;
  const candidate = value as Partial<EncryptedEnvelope>;
  return candidate.version === 1
    && typeof candidate.encrypted === "boolean"
    && typeof candidate.payload === "string";
}

/**
 * Reads an encrypted JSON document, returning `fallback` for anything
 * unreadable.
 *
 * Undecryptable payloads fail OPEN. There is no server and no PIN reset, so
 * failing closed would permanently strand a user out of their own accounts
 * after a Windows profile migration or keychain reset. The hold-B recovery on
 * the PIN pad covers the forgotten-PIN case instead.
 */
export async function readEncryptedJson<T>(
  path: string,
  crypto: SafeStorageLike,
  fallback: T,
): Promise<T> {
  let raw: string;
  try {
    raw = await readFile(path, "utf8");
  } catch {
    return fallback;
  }

  try {
    const envelope: unknown = JSON.parse(raw);
    if (!isEnvelope(envelope)) return fallback;
    const json = envelope.encrypted
      ? crypto.decryptString(Buffer.from(envelope.payload, "base64"))
      : envelope.payload;
    return JSON.parse(json) as T;
  } catch {
    return fallback;
  }
}

export async function writeEncryptedJson<T>(
  path: string,
  crypto: SafeStorageLike,
  value: T,
): Promise<void> {
  const json = JSON.stringify(value);

  let envelope: EncryptedEnvelope;
  if (crypto.isEncryptionAvailable()) {
    envelope = { version: 1, encrypted: true, payload: crypto.encryptString(json).toString("base64") };
  } else {
    // Linux without a keyring, mainly. Storing plaintext is no worse than the
    // adjacent auth-state.json and keeps the feature usable.
    envelope = { version: 1, encrypted: false, payload: json };
  }

  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, JSON.stringify(envelope, null, 2), "utf8");
}
