/**
 * Product terminology for instant replay (single source of truth for UI copy).
 */
export const INSTANT_REPLAY_DEFINITIONS = {
  rollingBuffer:
    "Rolling buffer: temporary storage of the last N minutes.",
  clientSideInterception:
    "Client-side interception: capturing the stream at the destination (what you see and hear here), not at the remote source.",
  hardwareAcceleration:
    "Hardware acceleration: using your local GPU for encode when the browser selects an accelerated codec.",
} as const;

/** Upper cap for “save last N seconds” (UI slider / product limit). */
export const INSTANT_REPLAY_MAX_SAVE_SECONDS = 120;

/**
 * Longest allowed clip for a given replay buffer length (minutes). Capped by {@link INSTANT_REPLAY_MAX_SAVE_SECONDS}.
 */
export function maxInstantReplaySaveSeconds(bufferMinutes: number): number {
  const wallSec = Math.max(60, Math.round(bufferMinutes) * 60);
  return Math.max(5, Math.min(INSTANT_REPLAY_MAX_SAVE_SECONDS, wallSec));
}
