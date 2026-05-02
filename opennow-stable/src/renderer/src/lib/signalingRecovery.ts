/** Base backoff steps for full reclaim (Stage B). Jitter applied per attempt in App. */
export const SIGNALING_RECOVERY_BASE_DELAYS_MS = [0, 1500, 4000, 8000, 15_000] as const;

const JITTER_MIN = 0.85;
const JITTER_SPAN = 0.3;
const MAX_DELAY_MS = 60_000;

/** Apply ±15% jitter to a base delay (deterministic when `rand` is fixed — for tests). */
export function signalingRecoveryDelayMs(
  baseMs: number,
  rand: () => number = Math.random,
): number {
  if (baseMs <= 0) {
    return 0;
  }
  const jitter = JITTER_MIN + rand() * JITTER_SPAN;
  return Math.min(MAX_DELAY_MS, Math.floor(baseMs * jitter));
}
