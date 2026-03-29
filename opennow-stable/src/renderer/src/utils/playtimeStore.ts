export const PLAYTIME_STORAGE_KEY = "opennow:playtime";

export interface PlaytimeRecord {
  totalSeconds: number;
  lastPlayedAt: string | null;
  sessionCount: number;
}

export type PlaytimeStore = Record<string, PlaytimeRecord>;

export function loadPlaytimeStore(storage: Pick<Storage, "getItem">, key = PLAYTIME_STORAGE_KEY): PlaytimeStore {
  try {
    const raw = storage.getItem(key);
    if (raw) {
      const parsed = JSON.parse(raw);
      if (parsed && typeof parsed === "object") return parsed as PlaytimeStore;
    }
  } catch {
  }
  return {};
}

export function savePlaytimeStore(storage: Pick<Storage, "setItem">, store: PlaytimeStore, key = PLAYTIME_STORAGE_KEY): void {
  try {
    storage.setItem(key, JSON.stringify(store));
  } catch {
  }
}

export function emptyPlaytimeRecord(): PlaytimeRecord {
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

export function formatLastPlayed(isoString: string | null, now = new Date()): string {
  if (!isoString) return "Never";
  const then = new Date(isoString);

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

export function startPlaytimeSession(
  store: PlaytimeStore,
  gameId: string,
  nowMs: number,
): PlaytimeStore {
  const existing = store[gameId] ?? emptyPlaytimeRecord();
  return {
    ...store,
    [gameId]: {
      ...existing,
      lastPlayedAt: new Date(nowMs).toISOString(),
      sessionCount: existing.sessionCount + 1,
    },
  };
}

export function endPlaytimeSession(
  store: PlaytimeStore,
  gameId: string,
  startMs: number | undefined,
  nowMs: number,
): PlaytimeStore {
  if (startMs == null) {
    return store;
  }

  const elapsedSeconds = Math.max(0, Math.floor((nowMs - startMs) / 1000));
  if (elapsedSeconds === 0) {
    return store;
  }

  const existing = store[gameId] ?? emptyPlaytimeRecord();
  return {
    ...store,
    [gameId]: {
      ...existing,
      totalSeconds: existing.totalSeconds + elapsedSeconds,
    },
  };
}
