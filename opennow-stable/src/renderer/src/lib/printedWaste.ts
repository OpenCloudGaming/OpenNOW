import type { PrintedWasteQueueData, PrintedWasteServerMapping } from "@shared/gfn";

export function isStandardPrintedWasteZone(zoneId: string): boolean {
  return zoneId.startsWith("NP-") && !zoneId.startsWith("NPA-");
}

export function isAllianceStreamingBaseUrl(streamingBaseUrl: string): boolean {
  if (!streamingBaseUrl.trim()) return false;
  try {
    const { hostname } = new URL(streamingBaseUrl);
    return !hostname.endsWith(".nvidiagrid.net");
  } catch {
    return false;
  }
}

export function constructPrintedWasteZoneUrl(zoneId: string): string {
  return `https://${zoneId.toLowerCase()}.cloudmatchbeta.nvidiagrid.net/`;
}

export interface EnrichedZoneInfo {
  zoneId: string;
  queuePosition: number;
  routingUrl: string;
  pingMs: number | null;
}

export function pickBestPrintedWasteZone(
  queueData: PrintedWasteQueueData,
  serverMapping: PrintedWasteServerMapping | null,
  pingMap: Map<string, number | null>,
): EnrichedZoneInfo | null {
  const nukedIds = new Set<string>();
  if (serverMapping) {
    for (const [zoneId, meta] of Object.entries(serverMapping)) {
      if (meta.nuked) nukedIds.add(zoneId);
    }
  }

  const candidates = Object.entries(queueData)
    .filter(([zoneId]) => isStandardPrintedWasteZone(zoneId) && !nukedIds.has(zoneId))
    .map(([zoneId, zone]) => ({
      zoneId,
      queuePosition: zone.QueuePosition,
      routingUrl: constructPrintedWasteZoneUrl(zoneId),
    }));

  if (candidates.length === 0) return null;

  const withPing = candidates.map((c) => ({
    ...c,
    pingMs: pingMap.get(c.routingUrl) ?? null,
  }));

  const pool = withPing.filter((z) => z.pingMs !== null).length > 0
    ? withPing.filter((z) => z.pingMs !== null)
    : withPing;

  const maxPing = Math.max(...pool.map((z) => z.pingMs ?? 999), 1);
  const maxQueue = Math.max(...pool.map((z) => z.queuePosition), 1);
  const AUTO_PING_WEIGHT = 0.75;
  const AUTO_QUEUE_WEIGHT = 0.25;

  return pool.reduce((prev, curr) => {
    const prevScore = ((prev.pingMs ?? maxPing) / maxPing) * AUTO_PING_WEIGHT + (prev.queuePosition / maxQueue) * AUTO_QUEUE_WEIGHT;
    const currScore = ((curr.pingMs ?? maxPing) / maxPing) * AUTO_PING_WEIGHT + (curr.queuePosition / maxQueue) * AUTO_QUEUE_WEIGHT;
    return currScore < prevScore ? curr : prev;
  });
}

export function hasAnyEligiblePrintedWasteZone(
  queueData: PrintedWasteQueueData,
  mapping: PrintedWasteServerMapping,
): boolean {
  return Object.keys(queueData).some((zoneId) => (
    isStandardPrintedWasteZone(zoneId) && mapping[zoneId]?.nuked !== true
  ));
}

