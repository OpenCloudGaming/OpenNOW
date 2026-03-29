import { useCallback, useRef, useState } from "react";

import {
  endPlaytimeSession,
  formatLastPlayed,
  formatPlaytime,
  loadPlaytimeStore,
  savePlaytimeStore,
  startPlaytimeSession,
  type PlaytimeStore,
} from "./playtimeStore";

export { formatLastPlayed, formatPlaytime };
export type { PlaytimeRecord, PlaytimeStore } from "./playtimeStore";

export interface UsePlaytimeReturn {
  playtime: PlaytimeStore;
  startSession: (gameId: string) => void;
  endSession: (gameId: string) => void;
}

export function usePlaytime(): UsePlaytimeReturn {
  const [playtime, setPlaytime] = useState<PlaytimeStore>(() => loadPlaytimeStore(localStorage));
  const sessionStartRef = useRef<Record<string, number>>({});

  const startSession = useCallback((gameId: string): void => {
    const nowMs = Date.now();
    sessionStartRef.current[gameId] = nowMs;
    setPlaytime((prev) => {
      const next = startPlaytimeSession(prev, gameId, nowMs);
      savePlaytimeStore(localStorage, next);
      return next;
    });
  }, []);

  const endSession = useCallback((gameId: string): void => {
    const startMs = sessionStartRef.current[gameId];
    if (startMs == null) return;
    delete sessionStartRef.current[gameId];

    const nowMs = Date.now();
    setPlaytime((prev) => {
      const next = endPlaytimeSession(prev, gameId, startMs, nowMs);
      if (Object.is(next, prev)) {
        return prev;
      }
      savePlaytimeStore(localStorage, next);
      return next;
    });
  }, []);

  return { playtime, startSession, endSession };
}
