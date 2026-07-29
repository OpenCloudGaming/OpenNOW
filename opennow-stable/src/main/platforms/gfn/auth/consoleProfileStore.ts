import type {
  ConsolePinMutationResult,
  ConsolePinStatus,
  ConsolePinVerifyResult,
} from "@shared/gfn";

import { readEncryptedJson, writeEncryptedJson, type SafeStorageLike } from "../../../security/encryptedJsonFile";
import { hashPin, verifyPinHash, type PinHashRecord } from "./pinHash";
import {
  createPinAttemptState,
  evaluatePinGate,
  isPinFormatValid,
  PIN_MAX_ATTEMPTS,
  registerPinFailure,
  registerPinSuccess,
  type PinAttemptState,
} from "./pinPolicy";

export interface ConsoleProfileRecord {
  userId: string;
  pin: PinHashRecord | null;
  attempts: PinAttemptState;
  updatedAtMs: number;
}

interface ConsoleProfileDocument {
  version: 1;
  profiles: ConsoleProfileRecord[];
}

const EMPTY_DOCUMENT: ConsoleProfileDocument = { version: 1, profiles: [] };

/**
 * Per-account console PIN locks, persisted separately from auth-state.json.
 *
 * auth-state.json is rewritten wholesale from an AuthSession snapshot and
 * cleared on logout, so lock records stored there would be dropped; settings.json
 * is handed to the renderer in full by SETTINGS_GET, so a hash there would leak.
 * A missing file simply means no PINs, which makes adoption migration-free.
 *
 * Attempt state is persisted alongside the hash — keeping it in memory would let
 * a restart reset the lockout, making the throttle theatre.
 */
export class ConsoleProfileStore {
  private profiles = new Map<string, ConsoleProfileRecord>();
  private initialized = false;

  constructor(
    private readonly filePath: string,
    private readonly crypto: SafeStorageLike,
  ) {}

  async initialize(): Promise<void> {
    const document = await readEncryptedJson<ConsoleProfileDocument>(this.filePath, this.crypto, EMPTY_DOCUMENT);
    this.profiles.clear();
    for (const record of document.profiles ?? []) {
      if (!record?.userId) continue;
      this.profiles.set(record.userId, {
        userId: record.userId,
        pin: record.pin ?? null,
        attempts: record.attempts ?? createPinAttemptState(),
        updatedAtMs: record.updatedAtMs ?? 0,
      });
    }
    this.initialized = true;
  }

  private async persist(): Promise<void> {
    await writeEncryptedJson<ConsoleProfileDocument>(this.filePath, this.crypto, {
      version: 1,
      profiles: [...this.profiles.values()],
    });
  }

  private ensureRecord(userId: string): ConsoleProfileRecord {
    const existing = this.profiles.get(userId);
    if (existing) return existing;
    const created: ConsoleProfileRecord = {
      userId,
      pin: null,
      attempts: createPinAttemptState(),
      updatedAtMs: 0,
    };
    this.profiles.set(userId, created);
    return created;
  }

  hasPin(userId: string): boolean {
    return this.profiles.get(userId)?.pin !== null && this.profiles.get(userId)?.pin !== undefined;
  }

  getStatus(userId: string, nowMs: number): ConsolePinStatus {
    const record = this.profiles.get(userId);
    if (!record?.pin) {
      return { userId, hasPin: false, lockedUntilMs: null, remainingAttempts: PIN_MAX_ATTEMPTS };
    }
    const gate = evaluatePinGate(record.attempts, nowMs);
    return {
      userId,
      hasPin: true,
      lockedUntilMs: gate.lockedUntilMs,
      remainingAttempts: gate.remainingAttempts,
    };
  }

  async setPin(userId: string, pin: string, currentPin?: string, nowMs = Date.now()): Promise<ConsolePinMutationResult> {
    if (!this.initialized) return { ok: false, reason: "storage_unavailable", hasPin: false };
    if (!isPinFormatValid(pin)) return { ok: false, reason: "invalid_format", hasPin: this.hasPin(userId) };

    const record = this.ensureRecord(userId);
    if (record.pin) {
      // Replacing a PIN requires proving you know the current one, otherwise the
      // lock could be reset from the manage screen without ever unlocking it.
      if (currentPin === undefined || !isPinFormatValid(currentPin)) {
        return { ok: false, reason: "invalid_format", hasPin: true };
      }
      const gate = evaluatePinGate(record.attempts, nowMs);
      if (!gate.allowed) return { ok: false, reason: "locked_out", hasPin: true };
      if (!(await verifyPinHash(currentPin, record.pin))) {
        record.attempts = registerPinFailure(record.attempts, nowMs);
        await this.persist();
        return { ok: false, reason: "invalid_pin", hasPin: true };
      }
    }

    record.pin = await hashPin(pin);
    record.attempts = createPinAttemptState();
    record.updatedAtMs = nowMs;
    await this.persist();
    return { ok: true, hasPin: true };
  }

  async clearPin(userId: string, currentPin: string, nowMs = Date.now()): Promise<ConsolePinMutationResult> {
    if (!this.initialized) return { ok: false, reason: "storage_unavailable", hasPin: false };

    const record = this.profiles.get(userId);
    if (!record?.pin) return { ok: false, reason: "no_pin_set", hasPin: false };

    const gate = evaluatePinGate(record.attempts, nowMs);
    if (!gate.allowed) return { ok: false, reason: "locked_out", hasPin: true };
    if (!isPinFormatValid(currentPin)) return { ok: false, reason: "invalid_format", hasPin: true };

    if (!(await verifyPinHash(currentPin, record.pin))) {
      record.attempts = registerPinFailure(record.attempts, nowMs);
      await this.persist();
      return { ok: false, reason: "invalid_pin", hasPin: true };
    }

    record.pin = null;
    record.attempts = createPinAttemptState();
    record.updatedAtMs = nowMs;
    await this.persist();
    return { ok: true, hasPin: false };
  }

  async verifyPin(userId: string, pin: string, nowMs = Date.now()): Promise<ConsolePinVerifyResult> {
    if (!this.initialized) {
      return { ok: false, reason: "storage_unavailable", remainingAttempts: 0, lockedUntilMs: null };
    }

    const record = this.profiles.get(userId);
    if (!record?.pin) {
      return { ok: true, reason: "no_pin_set", remainingAttempts: PIN_MAX_ATTEMPTS, lockedUntilMs: null };
    }

    const gate = evaluatePinGate(record.attempts, nowMs);
    if (!gate.allowed) {
      return { ok: false, reason: "locked_out", remainingAttempts: 0, lockedUntilMs: gate.lockedUntilMs };
    }

    if (!isPinFormatValid(pin)) {
      return { ok: false, reason: "invalid_format", remainingAttempts: gate.remainingAttempts, lockedUntilMs: null };
    }

    if (await verifyPinHash(pin, record.pin)) {
      record.attempts = registerPinSuccess(record.attempts);
      await this.persist();
      return { ok: true, remainingAttempts: PIN_MAX_ATTEMPTS, lockedUntilMs: null };
    }

    record.attempts = registerPinFailure(record.attempts, nowMs);
    await this.persist();
    const nextGate = evaluatePinGate(record.attempts, nowMs);
    return {
      ok: false,
      reason: nextGate.allowed ? "invalid_pin" : "locked_out",
      remainingAttempts: nextGate.remainingAttempts,
      lockedUntilMs: nextGate.lockedUntilMs,
    };
  }

  /** Drops a profile's lock when the underlying account is removed. */
  async forgetUser(userId: string): Promise<void> {
    if (!this.profiles.delete(userId)) return;
    await this.persist();
  }

  async forgetAll(): Promise<void> {
    if (this.profiles.size === 0) return;
    this.profiles.clear();
    await this.persist();
  }
}
