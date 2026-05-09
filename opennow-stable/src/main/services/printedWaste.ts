import type {
  PrintedWasteQueueData,
  PrintedWasteServerMapping,
} from "@shared/gfn";
import { fetchWithTimeout, withTimeout } from "./requestTimeout";

const PRINTEDWASTE_TIMEOUT_MS = 7000;
const PRINTEDWASTE_QUEUE_URL = "https://api.printedwaste.com/gfn/queue/";
const PRINTEDWASTE_SERVER_MAPPING_URL =
  "https://remote.printedwaste.com/config/GFN_SERVERID_TO_REGION_MAPPING";

export async function fetchPrintedWasteQueue(
  appVersion: string,
): Promise<PrintedWasteQueueData> {
  const response = await fetchWithTimeout(
    PRINTEDWASTE_QUEUE_URL,
    {
      headers: {
        "User-Agent": `opennow/${appVersion}`,
        Accept: "application/json",
      },
    },
    PRINTEDWASTE_TIMEOUT_MS,
    "PrintedWaste queue request",
  );
  if (!response.ok) {
    throw new Error(`PrintedWaste API returned HTTP ${response.status}`);
  }

  const body = await withTimeout(
    response.json() as Promise<unknown>,
    PRINTEDWASTE_TIMEOUT_MS,
    "PrintedWaste queue response parse",
  );
  if (!body || typeof body !== "object" || Array.isArray(body)) {
    throw new Error("PrintedWaste API response was not an object");
  }

  const apiBody = body as { status?: unknown; data?: unknown };
  if (typeof apiBody.status !== "boolean") {
    throw new Error("PrintedWaste API response missing boolean status");
  }
  if (!apiBody.status) {
    throw new Error("PrintedWaste API returned status:false");
  }
  if (
    !apiBody.data ||
    typeof apiBody.data !== "object" ||
    Array.isArray(apiBody.data)
  ) {
    throw new Error("PrintedWaste API response missing data object");
  }

  const normalizedData: PrintedWasteQueueData = {};
  for (const [zoneId, rawZone] of Object.entries(
    apiBody.data as Record<string, unknown>,
  )) {
    if (!rawZone || typeof rawZone !== "object" || Array.isArray(rawZone)) {
      continue;
    }
    const zone = rawZone as Record<string, unknown>;
    const queuePosition = zone.QueuePosition;
    const lastUpdated = zone["Last Updated"];
    const region = zone.Region;
    const eta = zone.eta;

    if (
      typeof queuePosition !== "number" ||
      !Number.isFinite(queuePosition)
    ) {
      continue;
    }
    if (typeof lastUpdated !== "number" || !Number.isFinite(lastUpdated)) {
      continue;
    }
    if (typeof region !== "string" || region.length === 0) {
      continue;
    }
    if (
      eta !== undefined &&
      (typeof eta !== "number" || !Number.isFinite(eta))
    ) {
      continue;
    }

    normalizedData[zoneId] = {
      QueuePosition: queuePosition,
      "Last Updated": lastUpdated,
      Region: region,
      ...(typeof eta === "number" ? { eta } : {}),
    };
  }

  if (Object.keys(normalizedData).length === 0) {
    throw new Error("PrintedWaste API returned no valid zones");
  }
  return normalizedData;
}

export async function fetchPrintedWasteServerMapping(
  appVersion: string,
): Promise<PrintedWasteServerMapping> {
  const response = await fetchWithTimeout(
    PRINTEDWASTE_SERVER_MAPPING_URL,
    {
      headers: {
        "User-Agent": `opennow/${appVersion}`,
        Accept: "application/json",
      },
    },
    PRINTEDWASTE_TIMEOUT_MS,
    "PrintedWaste server mapping request",
  );
  if (!response.ok) {
    throw new Error(
      `PrintedWaste server mapping returned HTTP ${response.status}`,
    );
  }

  const body = await withTimeout(
    response.json() as Promise<unknown>,
    PRINTEDWASTE_TIMEOUT_MS,
    "PrintedWaste server mapping response parse",
  );
  if (!body || typeof body !== "object" || Array.isArray(body)) {
    throw new Error("PrintedWaste server mapping response was not an object");
  }

  const apiBody = body as { status?: unknown; data?: unknown };
  if (typeof apiBody.status !== "boolean") {
    throw new Error(
      "PrintedWaste server mapping response missing boolean status",
    );
  }
  if (!apiBody.status) {
    throw new Error("PrintedWaste server mapping returned status:false");
  }
  if (
    !apiBody.data ||
    typeof apiBody.data !== "object" ||
    Array.isArray(apiBody.data)
  ) {
    throw new Error(
      "PrintedWaste server mapping response missing data object",
    );
  }

  const normalizedData: PrintedWasteServerMapping = {};

  for (const [zoneId, rawZone] of Object.entries(
    apiBody.data as Record<string, unknown>,
  )) {
    if (!rawZone || typeof rawZone !== "object" || Array.isArray(rawZone)) {
      continue;
    }
    const zone = rawZone as Record<string, unknown>;
    const title = zone.title;
    const region = zone.region;
    const is4080Server = zone.is4080Server;
    const is5080Server = zone.is5080Server;
    const nuked = zone.nuked;

    normalizedData[zoneId] = {
      ...(typeof title === "string" ? { title } : {}),
      ...(typeof region === "string" ? { region } : {}),
      ...(typeof is4080Server === "boolean" ? { is4080Server } : {}),
      ...(typeof is5080Server === "boolean" ? { is5080Server } : {}),
      ...(typeof nuked === "boolean" ? { nuked } : {}),
    };
  }

  return normalizedData;
}
