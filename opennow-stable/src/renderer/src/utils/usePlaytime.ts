import { useCallback, useRef, useState } from "react";

const STORAGE_KEY = "opennow:playtime";

export interface PlaytimeRecord {
  totalSeconds: number;
  lastPlayedAt: string | null;
  sessionCount: number;
}

export type PlaytimeStore = Record<string, PlaytimeRecord>;

function loadStore(): PlaytimeStore {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw);
      if (parsed && typeof parsed === "object") return parsed as PlaytimeStore;
    }
  } catch {
  }
  return {};
}

function saveStore(store: PlaytimeStore): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(store));
  } catch {
  }
}

function emptyRecord(): PlaytimeRecord {
  return { totalSeconds: 0, lastPlayedAt: null, sessionCount: 0 };
}

export function formatPlaytime(totalSeconds: number): string {
  if (totalSeconds < 60) {
    return totalSeconds <= 0 ? "Never played" : "< 1 min";
  }
  const h = Math.floor(totalSeconds / 3600);
  const m = Math.floor((totalSeconds % 3600) / 60);
  if (h === 0) return `${m} m`;
  if (m === 0) return `${h} h`;
  return `${h} h ${m} m`;
}

export function formatLastPlayed(isoString: string | null): string {
  if (!isoString) return "Never";
  const then = new Date(isoString);
  const now = new Date();

  const thenDay = new Date(then.getFullYear(), then.getMonth(), then.getDate()).getTime();
  const todayDay = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();

  const diffDays = Math.round((todayDay - thenDay) / 86_400_000);

  if (diffDays === 0) return "Today";
  if (diffDays === 1) return "Yesterday";
  if (diffDays < 7) return `${diffDays} days ago`;
  if (diffDays < 30) return `${Math.floor(diffDays / 7)} wk ago`;
  if (diffDays < 365) return `${Math.floor(diffDays / 30)} mo ago`;
  return `${Math.floor(diffDays / 365)} yr ago`;
}

export function formatRemainingPlaytimeFromSubscription(
  subscription: { isUnlimited: boolean; remainingHours: number } | null,
  consumedHours = 0,
): string {
  if (!subscription) {
    return "--";
  }
  if (subscription.isUnlimited) {
    return "Unlimited";
  }

  const baseHours = Number.isFinite(subscription.remainingHours) ? subscription.remainingHours : 0;
  const safeHours = Math.max(0, baseHours - Math.max(0, consumedHours));
  const totalMinutes = Math.round(safeHours * 60);
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;

  if (hours > 0) {
    return `${hours}h ${minutes.toString().padStart(2, "0")}m`;
  }
  return `${minutes}m`;
}

export interface UsePlaytimeReturn {
  playtime: PlaytimeStore;
  startSession: (gameId: string) => void;
  endSession: (gameId: string) => void;
}

export function usePlaytime(): UsePlaytimeReturn {
  const [playtime, setPlaytime] = useState<PlaytimeStore>(loadStore);
  const sessionStartRef = useRef<Record<string, number>>({});

  const startSession = useCallback((gameId: string): void => {
    sessionStartRef.current[gameId] = Date.now();
    setPlaytime((prev) => {
      const existing = prev[gameId] ?? emptyRecord();
      const next: PlaytimeStore = {
        ...prev,
        [gameId]: {
          ...existing,
          lastPlayedAt: new Date().toISOString(),
          sessionCount: existing.sessionCount + 1,
        },
      };
      saveStore(next);
      return next;
    });
  }, []);

  const endSession = useCallback((gameId: string): void => {
    const startMs = sessionStartRef.current[gameId];
    if (startMs == null) return;
    delete sessionStartRef.current[gameId];

    const elapsedSeconds = Math.max(0, Math.floor((Date.now() - startMs) / 1000));
    if (elapsedSeconds === 0) return;

    setPlaytime((prev) => {
      const existing = prev[gameId] ?? emptyRecord();
      const next: PlaytimeStore = {
        ...prev,
        [gameId]: {
          ...existing,
          totalSeconds: existing.totalSeconds + elapsedSeconds,
        },
      };
      saveStore(next);
      return next;
    });
  }, []);

  return { playtime, startSession, endSession };
}
