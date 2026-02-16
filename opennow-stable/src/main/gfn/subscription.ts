/**
 * MES (Membership/Subscription) API integration for GeForce NOW
 * Handles fetching subscription info from the MES API endpoint.
 */

import type {
  SubscriptionInfo,
  EntitledResolution,
  StorageAddon,
  StreamRegion,
} from "@shared/gfn";

/** MES API endpoint URL */
const MES_URL = "https://mes.geforcenow.com/v4/subscriptions";

/** LCARS Client ID */
const LCARS_CLIENT_ID = "ec7e38d4-03af-4b58-b131-cfb0495903ab";

/** GFN client version */
const GFN_CLIENT_VERSION = "2.0.80.173";

interface SubscriptionResponse {
  firstEntitlementStartDateTime?: string;
  type?: string;
  membershipTier?: string;
  allottedTimeInMinutes?: number;
  purchasedTimeInMinutes?: number;
  rolledOverTimeInMinutes?: number;
  remainingTimeInMinutes?: number;
  totalTimeInMinutes?: number;
  notifications?: {
    notifyUserWhenTimeRemainingInMinutes?: number;
    notifyUserOnSessionWhenRemainingTimeInMinutes?: number;
  };
  currentSpanStartDateTime?: string;
  currentSpanEndDateTime?: string;
  currentSubscriptionState?: {
    state?: string;
    isGamePlayAllowed?: boolean;
  };
  subType?: string;
  addons?: SubscriptionAddonResponse[];
  features?: SubscriptionFeatures;
}

interface SubscriptionFeatures {
  resolutions?: SubscriptionResolution[];
}

interface SubscriptionResolution {
  heightInPixels: number;
  widthInPixels: number;
  framesPerSecond: number;
  isEntitled: boolean;
}

interface SubscriptionAddonResponse {
  type?: string;
  subType?: string;
  status?: string;
  attributes?: AddonAttribute[];
}

interface AddonAttribute {
  key?: string;
  textValue?: string;
}

function parseMinutes(value: unknown): number | undefined {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }
  if (typeof value === "string" && value.trim().length > 0) {
    const parsed = Number(value);
    if (Number.isFinite(parsed)) {
      return parsed;
    }
  }
  return undefined;
}

function parseNumberText(value: unknown): number | undefined {
  if (typeof value !== "string" || value.trim().length === 0) {
    return undefined;
  }
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) {
    return undefined;
  }
  return parsed;
}

function parseIsoDate(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

/**
 * Fetch subscription info from MES API
 * @param token - The authentication token
 * @param userId - The user ID
 * @param vpcId - The VPC ID (defaults to a common European VPC if not provided)
 * @returns The subscription info
 */
export async function fetchSubscription(
  token: string,
  userId: string,
  vpcId = "NP-AMS-08",
): Promise<SubscriptionInfo> {
  const url = new URL(MES_URL);
  url.searchParams.append("serviceName", "gfn_pc");
  url.searchParams.append("languageCode", "en_US");
  url.searchParams.append("vpcId", vpcId);
  url.searchParams.append("userId", userId);

  const response = await fetch(url.toString(), {
    headers: {
      Authorization: `GFNJWT ${token}`,
      Accept: "application/json",
      "nv-client-id": LCARS_CLIENT_ID,
      "nv-client-type": "NATIVE",
      "nv-client-version": GFN_CLIENT_VERSION,
      "nv-client-streamer": "NVIDIA-CLASSIC",
      "nv-device-os": "WINDOWS",
      "nv-device-type": "DESKTOP",
    },
  });

  if (!response.ok) {
    const body = await response.text();
    throw new Error(`Subscription API failed with status ${response.status}: ${body}`);
  }

  const data = (await response.json()) as SubscriptionResponse;

  // Parse membership tier (defaults to FREE)
  const membershipTier = data.membershipTier ?? "FREE";

  // Convert minutes to hours. Use the additive fields as fallback if total is absent.
  const allottedMinutes = parseMinutes(data.allottedTimeInMinutes) ?? 0;
  const purchasedMinutes = parseMinutes(data.purchasedTimeInMinutes) ?? 0;
  const rolledOverMinutes = parseMinutes(data.rolledOverTimeInMinutes) ?? 0;
  const fallbackTotalMinutes = allottedMinutes + purchasedMinutes + rolledOverMinutes;
  const totalMinutes = parseMinutes(data.totalTimeInMinutes) ?? fallbackTotalMinutes;
  const remainingMinutes = parseMinutes(data.remainingTimeInMinutes) ?? 0;
  const usedMinutes = Math.max(totalMinutes - remainingMinutes, 0);

  const allottedHours = allottedMinutes / 60;
  const purchasedHours = purchasedMinutes / 60;
  const rolledOverHours = rolledOverMinutes / 60;
  const usedHours = usedMinutes / 60;
  const remainingHours = remainingMinutes / 60;
  const totalHours = totalMinutes / 60;

  // Check if unlimited subscription
  const isUnlimited = data.subType === "UNLIMITED";

  // Parse storage addon
  let storageAddon: StorageAddon | undefined;
  const storageAddonResponse = data.addons?.find(
    (addon) =>
      addon.type === "STORAGE" &&
      addon.subType === "PERMANENT_STORAGE" &&
      addon.status === "OK",
  );

  if (storageAddonResponse) {
    const sizeAttr = storageAddonResponse.attributes?.find(
      (attr) => attr.key === "TOTAL_STORAGE_SIZE_IN_GB",
    );
    const usedAttr = storageAddonResponse.attributes?.find(
      (attr) => attr.key === "USED_STORAGE_SIZE_IN_GB",
    );
    const regionNameAttr = storageAddonResponse.attributes?.find(
      (attr) => attr.key === "STORAGE_METRO_REGION_NAME",
    );
    const regionCodeAttr = storageAddonResponse.attributes?.find(
      (attr) => attr.key === "STORAGE_METRO_REGION",
    );
    const sizeGb = parseNumberText(sizeAttr?.textValue);
    const usedGb = parseNumberText(usedAttr?.textValue);
    const regionName = regionNameAttr?.textValue;
    const regionCode = regionCodeAttr?.textValue;

    storageAddon = {
      type: "PERMANENT_STORAGE",
      sizeGb,
      usedGb,
      regionName,
      regionCode,
    };
  }

  // Parse entitled resolutions
  const entitledResolutions: EntitledResolution[] = [];
  if (data.features?.resolutions) {
    for (const res of data.features.resolutions) {
      // Include all resolutions (matching Rust implementation behavior)
      entitledResolutions.push({
        width: res.widthInPixels,
        height: res.heightInPixels,
        fps: res.framesPerSecond,
      });
    }

    // Sort by highest resolution/fps first
    entitledResolutions.sort((a, b) => {
      if (b.width !== a.width) return b.width - a.width;
      if (b.height !== a.height) return b.height - a.height;
      return b.fps - a.fps;
    });
  }

  return {
    membershipTier,
    subscriptionType: data.type,
    subscriptionSubType: data.subType,
    allottedHours,
    purchasedHours,
    rolledOverHours,
    usedHours,
    remainingHours,
    totalHours,
    firstEntitlementStartDateTime: parseIsoDate(data.firstEntitlementStartDateTime),
    serverRegionId: vpcId,
    currentSpanStartDateTime: parseIsoDate(data.currentSpanStartDateTime),
    currentSpanEndDateTime: parseIsoDate(data.currentSpanEndDateTime),
    notifyUserWhenTimeRemainingInMinutes: parseMinutes(
      data.notifications?.notifyUserWhenTimeRemainingInMinutes,
    ),
    notifyUserOnSessionWhenRemainingTimeInMinutes: parseMinutes(
      data.notifications?.notifyUserOnSessionWhenRemainingTimeInMinutes,
    ),
    state: data.currentSubscriptionState?.state,
    isGamePlayAllowed: data.currentSubscriptionState?.isGamePlayAllowed,
    isUnlimited,
    storageAddon,
    entitledResolutions,
  };
}

/**
 * Fetch dynamic regions from serverInfo endpoint to get VPC ID
 * @param token - Optional authentication token
 * @param streamingBaseUrl - Base URL for the streaming service
 * @returns Array of stream regions and the discovered VPC ID
 */
export async function fetchDynamicRegions(
  token: string | undefined,
  streamingBaseUrl: string,
): Promise<{ regions: StreamRegion[]; vpcId: string | null }> {
  const base = streamingBaseUrl.endsWith("/")
    ? streamingBaseUrl
    : `${streamingBaseUrl}/`;
  const url = `${base}v2/serverInfo`;

  const headers: Record<string, string> = {
    Accept: "application/json",
    "nv-client-id": LCARS_CLIENT_ID,
    "nv-client-type": "BROWSER",
    "nv-client-version": GFN_CLIENT_VERSION,
    "nv-client-streamer": "WEBRTC",
    "nv-device-os": "WINDOWS",
    "nv-device-type": "DESKTOP",
  };

  if (token) {
    headers.Authorization = `GFNJWT ${token}`;
  }

  let response: Response;
  try {
    response = await fetch(url, { headers });
  } catch {
    return { regions: [], vpcId: null };
  }

  if (!response.ok) {
    return { regions: [], vpcId: null };
  }

  const data = (await response.json()) as {
    requestStatus?: { serverId?: string };
    metaData?: Array<{ key: string; value: string }>;
  };

  // Extract VPC ID
  const vpcId = data.requestStatus?.serverId ?? null;

  // Extract regions
  const regions = (data.metaData ?? [])
    .filter(
      (entry) =>
        entry.value.startsWith("https://") &&
        entry.key !== "gfn-regions" &&
        !entry.key.startsWith("gfn-"),
    )
    .map<StreamRegion>((entry) => ({
      name: entry.key,
      url: entry.value.endsWith("/") ? entry.value : `${entry.value}/`,
    }))
    .sort((a, b) => a.name.localeCompare(b.name));

  return { regions, vpcId };
}
