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

export function hasAnyEligiblePrintedWasteZone(
  queueData: PrintedWasteQueueData,
  mapping: PrintedWasteServerMapping,
): boolean {
  return Object.keys(queueData).some(
    (zoneId) => isStandardPrintedWasteZone(zoneId) && mapping[zoneId]?.nuked !== true,
  );
}
