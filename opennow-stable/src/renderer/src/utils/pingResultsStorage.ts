const REGION_PING_RESULTS_STORAGE_KEY = "opennow.ping-results.v1";
const PRINTEDWASTE_PING_RESULTS_STORAGE_KEY = "opennow.printedwaste-pings.v1";

interface PingCacheEntry {
  url: string;
  pingMs: number | null;
}

function loadPingResults(storageKey: string, fallback: null): Map<string, number | null> | null;
function loadPingResults(storageKey: string, fallback: Map<string, number | null>): Map<string, number | null>;
function loadPingResults(
  storageKey: string,
  fallback: Map<string, number | null> | null,
): Map<string, number | null> | null {
  try {
    const raw = window.sessionStorage.getItem(storageKey);
    if (!raw) return fallback;
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return fallback;
    const results = new Map<string, number | null>();
    for (const entry of parsed as PingCacheEntry[]) {
      results.set(entry.url, entry.pingMs);
    }
    return results;
  } catch {
    return fallback;
  }
}

function savePingResults(storageKey: string, results: Map<string, number | null>): void {
  try {
    const entries: PingCacheEntry[] = [];
    results.forEach((pingMs, url) => {
      entries.push({ url, pingMs });
    });
    window.sessionStorage.setItem(storageKey, JSON.stringify(entries));
  } catch {
  }
}

export function loadStoredRegionPingResults(): Map<string, number | null> | null {
  return loadPingResults(REGION_PING_RESULTS_STORAGE_KEY, null);
}

export function saveStoredRegionPingResults(results: Map<string, number | null>): void {
  savePingResults(REGION_PING_RESULTS_STORAGE_KEY, results);
}

export function clearStoredRegionPingResults(): void {
  try {
    window.sessionStorage.removeItem(REGION_PING_RESULTS_STORAGE_KEY);
  } catch {
  }
}

export function loadStoredPrintedWastePingResults(): Map<string, number | null> {
  return loadPingResults(PRINTEDWASTE_PING_RESULTS_STORAGE_KEY, new Map());
}

export function saveStoredPrintedWastePingResults(results: Map<string, number | null>): void {
  savePingResults(PRINTEDWASTE_PING_RESULTS_STORAGE_KEY, results);
}
