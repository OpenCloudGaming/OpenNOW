import { randomBytes, scrypt, timingSafeEqual } from "node:crypto";

/**
 * scrypt hashing for console profile PINs.
 *
 * Honesty note: a 4-digit PIN is 10 000 possibilities. Anyone with the hash can
 * exhaust that space in seconds no matter how expensive the KDF is. The cost
 * factor and the attempt lockout in pinPolicy.ts defend the *online* path only.
 * This is a shared-TV "who's playing" lock, not a credential — and the OAuth
 * tokens in auth-state.json are still plaintext, so file access bypasses it.
 */

const SCRYPT_N = 2 ** 15;
const SCRYPT_R = 8;
const SCRYPT_P = 1;
const KEY_LENGTH = 32;
const SALT_LENGTH = 16;

/**
 * scrypt needs roughly 128 * N * r bytes, which is exactly Node's 32 MiB
 * default cap for these parameters — it throws without a raised ceiling.
 */
const SCRYPT_MAXMEM = 64 * 1024 * 1024;

export interface PinHashRecord {
  version: 1;
  algorithm: "scrypt";
  N: number;
  r: number;
  p: number;
  keylen: number;
  /** base64 */
  salt: string;
  /** base64 */
  hash: string;
}

function derive(pin: string, salt: Buffer, params: { N: number; r: number; p: number; keylen: number }): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    // Async scrypt only: a synchronous ~100ms KDF would stall the main process
    // mid-stream.
    scrypt(
      pin,
      salt,
      params.keylen,
      { N: params.N, r: params.r, p: params.p, maxmem: SCRYPT_MAXMEM },
      (error, derivedKey) => (error ? reject(error) : resolve(derivedKey)),
    );
  });
}

export async function hashPin(pin: string, salt: Buffer = randomBytes(SALT_LENGTH)): Promise<PinHashRecord> {
  const params = { N: SCRYPT_N, r: SCRYPT_R, p: SCRYPT_P, keylen: KEY_LENGTH };
  const hash = await derive(pin, salt, params);
  return {
    version: 1,
    algorithm: "scrypt",
    ...params,
    salt: salt.toString("base64"),
    hash: hash.toString("base64"),
  };
}

export async function verifyPinHash(pin: string, record: PinHashRecord): Promise<boolean> {
  if (record?.algorithm !== "scrypt" || record.version !== 1) return false;

  let expected: Buffer;
  let salt: Buffer;
  try {
    expected = Buffer.from(record.hash, "base64");
    salt = Buffer.from(record.salt, "base64");
  } catch {
    return false;
  }
  if (expected.length === 0 || salt.length === 0) return false;

  let actual: Buffer;
  try {
    actual = await derive(pin, salt, { N: record.N, r: record.r, p: record.p, keylen: record.keylen });
  } catch {
    return false;
  }

  // timingSafeEqual throws on a length mismatch, so a truncated or tampered
  // record must be rejected before the comparison rather than crashing.
  if (actual.length !== expected.length) return false;
  return timingSafeEqual(actual, expected);
}
