/**
 * Console profile PIN policy — pure, no I/O, no crypto, no Electron.
 *
 * A 4-digit PIN has 10 000 possibilities and is trivially brute-forced offline;
 * this throttle only defends the online path (someone tapping at a TV). See the
 * honesty note in pinHash.ts.
 */

export const PIN_LENGTH = 4;
export const PIN_MAX_ATTEMPTS = 5;

/** Escalating lockout durations; the final entry repeats once exhausted. */
export const PIN_LOCKOUT_STEPS_MS = [30_000, 60_000, 300_000, 900_000] as const;

const PIN_PATTERN = /^[0-9]{4}$/;

export interface PinAttemptState {
  failedAttempts: number;
  lockoutLevel: number;
  lockedUntilMs: number | null;
}

export interface PinGateResult {
  allowed: boolean;
  lockedUntilMs: number | null;
  remainingAttempts: number;
}

export function createPinAttemptState(): PinAttemptState {
  return { failedAttempts: 0, lockoutLevel: 0, lockedUntilMs: null };
}

/**
 * ASCII digits only. Unicode digit forms (full-width, Arabic-Indic) are
 * rejected rather than normalized — the keypad only ever emits ASCII, so
 * anything else is a malformed request.
 */
export function isPinFormatValid(pin: string): boolean {
  return PIN_PATTERN.test(pin);
}

export function evaluatePinGate(state: PinAttemptState, nowMs: number): PinGateResult {
  const lockedOut = state.lockedUntilMs !== null && nowMs < state.lockedUntilMs;
  return {
    allowed: !lockedOut,
    lockedUntilMs: lockedOut ? state.lockedUntilMs : null,
    remainingAttempts: lockedOut ? 0 : Math.max(0, PIN_MAX_ATTEMPTS - state.failedAttempts),
  };
}

export function registerPinFailure(state: PinAttemptState, nowMs: number): PinAttemptState {
  const failedAttempts = state.failedAttempts + 1;
  if (failedAttempts < PIN_MAX_ATTEMPTS) {
    return { ...state, failedAttempts, lockedUntilMs: null };
  }

  const durationMs = PIN_LOCKOUT_STEPS_MS[Math.min(state.lockoutLevel, PIN_LOCKOUT_STEPS_MS.length - 1)];
  return {
    failedAttempts: 0,
    lockoutLevel: Math.min(state.lockoutLevel + 1, PIN_LOCKOUT_STEPS_MS.length),
    lockedUntilMs: nowMs + durationMs,
  };
}

/** Clears both the attempt counter and the escalation level. */
export function registerPinSuccess(_state: PinAttemptState): PinAttemptState {
  return createPinAttemptState();
}
