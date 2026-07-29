export const PIN_LENGTH = 4;

/** How long B must be held on the PIN pad before the recovery option appears. */
export const PIN_RECOVERY_HOLD_MS = 5000;

/** Appends a digit, ignoring anything past the PIN length. */
export function appendPinDigit(entry: string, digit: string): string {
  if (!/^[0-9]$/.test(digit)) return entry;
  return entry.length >= PIN_LENGTH ? entry : entry + digit;
}

export function deletePinDigit(entry: string): string {
  return entry.slice(0, -1);
}

export function isPinComplete(entry: string): boolean {
  return entry.length === PIN_LENGTH;
}

/**
 * Human-readable remaining lockout, rounded up so "1 minute" never displays
 * while the lock still has 59 seconds to run.
 */
export function getLockoutSecondsRemaining(lockedUntilMs: number | null, nowMs: number): number {
  if (lockedUntilMs === null) return 0;
  return Math.max(0, Math.ceil((lockedUntilMs - nowMs) / 1000));
}
