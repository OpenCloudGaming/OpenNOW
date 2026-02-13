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
  membershipTier?: string;
  remainingTimeInMinutes?: number;
  totalTimeInMinutes?: number;
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

  // Convert minutes to hours
  const remainingHours =
    data.remainingTimeInMinutes !== undefined
      ? data.remainingTimeInMinutes / 60
      : 0;
  const totalHours =
    data.totalTimeInMinutes !== undefined ? data.totalTimeInMinutes / 60 : 0;

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
    const sizeGb = sizeAttr?.textValue
      ? parseInt(sizeAttr.textValue, 10)
      : undefined;

    storageAddon = {
      type: "PERMANENT_STORAGE",
      sizeGb,
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
    remainingHours,
    totalHours,
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
