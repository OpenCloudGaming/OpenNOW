import type { GameInfo, GameVariant } from "@shared/gfn";
import { cacheManager } from "../services/cacheManager";

const GRAPHQL_URL = "https://games.geforce.com/graphql";
const PANELS_QUERY_HASH = "f8e26265a5db5c20e1334a6872cf04b6e3970507697f6ae55a6ddefa5420daf0";
const APP_METADATA_QUERY_HASH = "39187e85b6dcf60b7279a5f233288b0a8b69a8b1dbcfb5b25555afdcb988f0d7";
const DEFAULT_LOCALE = "en_US";
const LCARS_CLIENT_ID = "ec7e38d4-03af-4b58-b131-cfb0495903ab";
const GFN_CLIENT_VERSION = "2.0.80.173";

const GFN_USER_AGENT =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36 NVIDIACEFClient/HEAD/debb5919f6 GFN-PC/2.0.80.173";

interface GraphQlResponse {
  data?: {
    panels: Array<{
      name: string;
      sections: Array<{
        items: Array<{
          __typename: string;
          app?: AppData;
        }>;
      }>;
    }>;
  };
  errors?: Array<{ message: string }>;
}

interface AppMetaDataResponse {
  data?: {
    apps: {
      items: AppData[];
    };
  };
  errors?: Array<{ message: string }>;
}

interface AppData {
  id: string;
  title: string;
  description?: string;
  longDescription?: string;
  features?: unknown[];
  gameFeatures?: unknown[];
  appFeatures?: unknown[];
  genres?: unknown[];
  tags?: unknown[];
  images?: {
    GAME_BOX_ART?: string;
    TV_BANNER?: string;
    HERO_IMAGE?: string;
  };
  variants?: Array<{
    id: string;
    appStore: string;
    supportedControls?: string[];
    gfn?: {
      library?: {
        selected?: boolean;
      };
    };
  }>;
  gfn?: {
    playType?: string;
    minimumMembershipTierLabel?: string;
  };
}

interface ServerInfoResponse {
  requestStatus?: {
    serverId?: string;
  };
}

interface RawPublicGame {
  id?: string | number;
  title?: string;
  steamUrl?: string;
  status?: string;
}

function optimizeImage(url: string): string {
  if (url.includes("img.nvidiagrid.net")) {
    return `${url};f=webp;w=272`;
  }
  return url;
}

function isNumericId(value: string | undefined): value is string {
  if (!value) {
    return false;
  }
  return /^\d+$/.test(value);
}

function randomHuId(): string {
  return `${Date.now().toString(16)}${Math.random().toString(16).slice(2)}`;
}

async function getVpcId(token: string, providerStreamingBaseUrl?: string): Promise<string> {
  const base = providerStreamingBaseUrl?.trim() || "https://prod.cloudmatchbeta.nvidiagrid.net/";
  const normalizedBase = base.endsWith("/") ? base : `${base}/`;

  const response = await fetch(`${normalizedBase}v2/serverInfo`, {
    headers: {
      Accept: "application/json",
      Authorization: `GFNJWT ${token}`,
      "nv-client-id": LCARS_CLIENT_ID,
      "nv-client-type": "NATIVE",
      "nv-client-version": GFN_CLIENT_VERSION,
      "nv-client-streamer": "NVIDIA-CLASSIC",
      "nv-device-os": "WINDOWS",
      "nv-device-type": "DESKTOP",
      "User-Agent": GFN_USER_AGENT,
    },
  });

  if (!response.ok) {
    return "GFN-PC";
  }

  const payload = (await response.json()) as ServerInfoResponse;
  return payload.requestStatus?.serverId ?? "GFN-PC";
}

function appToGame(app: AppData): GameInfo {
  const variants: GameVariant[] =
    app.variants?.map((variant) => ({
      id: variant.id,
      store: variant.appStore,
      supportedControls: variant.supportedControls ?? [],
    })) ?? [];

  const selectedVariantIndex =
    app.variants?.findIndex((variant) => variant.gfn?.library?.selected === true) ?? 0;

  const safeIndex = Math.max(0, selectedVariantIndex);
  const selectedVariant = variants[safeIndex];
  const selectedVariantId = selectedVariant?.id;
  const fallbackNumericVariantId = variants.find((variant) => isNumericId(variant.id))?.id;
  const launchAppId = isNumericId(selectedVariantId)
    ? selectedVariantId
    : fallbackNumericVariantId ?? (isNumericId(app.id) ? app.id : undefined);

  const id = `${app.id}:${selectedVariantId ?? "default"}`;
  const imageUrl =
    app.images?.GAME_BOX_ART ?? app.images?.TV_BANNER ?? app.images?.HERO_IMAGE ?? undefined;

  return {
    id,
    uuid: app.id,
    launchAppId,
    title: app.title,
    description: app.description,
    longDescription: app.longDescription,
    featureLabels: extractFeatureLabels(app),
    genres: extractGenres(app),
    imageUrl: imageUrl ? optimizeImage(imageUrl) : undefined,
    playType: app.gfn?.playType,
    membershipTierLabel: app.gfn?.minimumMembershipTierLabel,
    selectedVariantIndex: Math.max(0, selectedVariantIndex),
    variants,
  };
}

function parseFeatureLabel(value: unknown): string | null {
  if (typeof value === "string") {
    const trimmed = value.trim();
    return trimmed.length > 0 ? trimmed : null;
  }
  if (value && typeof value === "object") {
    const candidate = value as Record<string, unknown>;
    const keys = ["name", "label", "title", "displayName"];
    for (const key of keys) {
      const raw = candidate[key];
      if (typeof raw === "string") {
        const trimmed = raw.trim();
        if (trimmed.length > 0) {
          return trimmed;
        }
      }
    }
  }
  return null;
}

function extractFeatureLabels(app: AppData): string[] {
  const buckets: unknown[] = [
    app.features,
    app.gameFeatures,
    app.appFeatures,
    app.genres,
    app.tags,
  ];

  const labels: string[] = [];
  for (const bucket of buckets) {
    if (!Array.isArray(bucket)) {
      continue;
    }
    for (const entry of bucket) {
      const label = parseFeatureLabel(entry);
      if (label) {
        labels.push(label);
      }
    }
  }

  return [...new Set(labels)];
}

function extractGenres(app: AppData): string[] {
  if (!Array.isArray(app.genres)) {
    return [];
  }

  const genres: string[] = [];
  for (const entry of app.genres) {
    const genre = parseFeatureLabel(entry);
    if (genre) {
      genres.push(genre);
    }
  }

  return [...new Set(genres)];
}

function appToVariants(app: AppData): GameVariant[] {
  return app.variants?.map((variant) => ({
    id: variant.id,
    store: variant.appStore,
    supportedControls: variant.supportedControls ?? [],
  })) ?? [];
}

function mergeAppMetaIntoGame(game: GameInfo, app: AppData): GameInfo {
  const metadataVariants = appToVariants(app);
  const variants = metadataVariants.length > 0 ? metadataVariants : game.variants;
  const selectedVariantId = game.id.split(":")[1];
  const selectedVariantIndex = Math.max(0, variants.findIndex((variant) => variant.id === selectedVariantId));
  const imageUrl =
    app.images?.GAME_BOX_ART ?? app.images?.TV_BANNER ?? app.images?.HERO_IMAGE ?? undefined;

  const description = app.description ?? game.description;
  const longDescription = app.longDescription ?? game.longDescription;
  const featureLabels = extractFeatureLabels(app);
  const genres = extractGenres(app);

  return {
    ...game,
    title: app.title || game.title,
    description,
    longDescription,
    featureLabels,
    genres,
    imageUrl: imageUrl ? optimizeImage(imageUrl) : game.imageUrl,
    playType: app.gfn?.playType ?? game.playType,
    membershipTierLabel: app.gfn?.minimumMembershipTierLabel ?? game.membershipTierLabel,
    selectedVariantIndex,
    variants,
  };
}

async function fetchAppMetaData(
  token: string,
  appIds: string[],
  vpcId: string,
): Promise<AppMetaDataResponse> {
  const normalizedIds = [...new Set(appIds.map((id) => id.trim()).filter((id) => id.length > 0))];
  if (normalizedIds.length === 0) {
    return { data: { apps: { items: [] } } };
  }

  const variables = JSON.stringify({
    vpcId,
    locale: DEFAULT_LOCALE,
    appIds: normalizedIds,
  });

  const extensions = JSON.stringify({
    persistedQuery: {
      sha256Hash: APP_METADATA_QUERY_HASH,
    },
  });

  const params = new URLSearchParams({
    requestType: "appMetaData",
    extensions,
    huId: randomHuId(),
    variables,
  });

  try {
    const response = await fetch(`${GRAPHQL_URL}?${params.toString()}`, {
      headers: {
        Accept: "application/json, text/plain, */*",
        "Content-Type": "application/graphql",
        Origin: "https://play.geforcenow.com",
        Referer: "https://play.geforcenow.com/",
        Authorization: `GFNJWT ${token}`,
        "nv-client-id": LCARS_CLIENT_ID,
        "nv-client-type": "NATIVE",
        "nv-client-version": GFN_CLIENT_VERSION,
        "nv-client-streamer": "NVIDIA-CLASSIC",
        "nv-device-os": "WINDOWS",
        "nv-device-type": "DESKTOP",
        "nv-device-make": "UNKNOWN",
        "nv-device-model": "UNKNOWN",
        "nv-browser-type": "CHROME",
        "User-Agent": GFN_USER_AGENT,
      },
    });

    if (!response.ok) {
      const text = await response.text();
      console.error(`[GFN Metadata] fetchAppMetaData failed (${response.status}):`, text.slice(0, 400));
      throw new Error(`App metadata failed (${response.status}): ${text.slice(0, 400)}`);
    }

    return (await response.json()) as AppMetaDataResponse;
  } catch (error) {
    console.error("[GFN Metadata] fetchAppMetaData error:", error);
    throw error;
  }
}

async function enrichGamesWithMetadata(token: string, vpcId: string, games: GameInfo[]): Promise<GameInfo[]> {
  const uuids = [...new Set(games.map((game) => game.uuid).filter((uuid): uuid is string => !!uuid))];
  
  if (uuids.length === 0) {
    return games;
  }

  const chunkSize = 40;
  const appById = new Map<string, AppData>();
  const startTime = Date.now();

  try {
    for (let index = 0; index < uuids.length; index += chunkSize) {
      const chunk = uuids.slice(index, index + chunkSize);
      const payload = await fetchAppMetaData(token, chunk, vpcId);
      if (payload.errors?.length) {
        console.error("[GFN Metadata] GraphQL errors:", payload.errors);
        throw new Error(payload.errors.map((error) => error.message).join(", "));
      }
      
      const items = payload.data?.apps.items ?? [];
      for (const app of items) {
        appById.set(app.id, app);
      }
    }

    let enrichedCount = 0;
    const enrichedGames = games.map((game) => {
      if (!game.uuid) {
        return game;
      }
      const metadata = appById.get(game.uuid);
      if (!metadata) {
        return game;
      }
      enrichedCount += 1;
      return mergeAppMetaIntoGame(game, metadata);
    });

    const elapsed = Date.now() - startTime;
    console.log(`[GFN Metadata] Enriched ${enrichedCount}/${games.length} games in ${elapsed}ms`);
    return enrichedGames;
  } catch (error) {
    console.error("[GFN Metadata] Enrichment error:", error);
    throw error;
  }
}

async function fetchPanels(
  token: string,
  panelNames: string[],
  vpcId: string,
): Promise<GraphQlResponse> {
  const variables = JSON.stringify({
    vpcId,
    locale: DEFAULT_LOCALE,
    panelNames,
  });

  const extensions = JSON.stringify({
    persistedQuery: {
      sha256Hash: PANELS_QUERY_HASH,
    },
  });

  const requestType = panelNames.includes("LIBRARY") ? "panels/Library" : "panels/MainV2";
  const params = new URLSearchParams({
    requestType,
    extensions,
    huId: randomHuId(),
    variables,
  });

  const response = await fetch(`${GRAPHQL_URL}?${params.toString()}`, {
    headers: {
      Accept: "application/json, text/plain, */*",
      "Content-Type": "application/graphql",
      Origin: "https://play.geforcenow.com",
      Referer: "https://play.geforcenow.com/",
      Authorization: `GFNJWT ${token}`,
      "nv-client-id": LCARS_CLIENT_ID,
      "nv-client-type": "NATIVE",
      "nv-client-version": GFN_CLIENT_VERSION,
      "nv-client-streamer": "NVIDIA-CLASSIC",
      "nv-device-os": "WINDOWS",
      "nv-device-type": "DESKTOP",
      "nv-device-make": "UNKNOWN",
      "nv-device-model": "UNKNOWN",
      "nv-browser-type": "CHROME",
      "User-Agent": GFN_USER_AGENT,
    },
  });

  if (!response.ok) {
    const text = await response.text();
    throw new Error(`Games GraphQL failed (${response.status}): ${text.slice(0, 400)}`);
  }

  return (await response.json()) as GraphQlResponse;
}

function flattenPanels(payload: GraphQlResponse): GameInfo[] {
  if (payload.errors?.length) {
    throw new Error(payload.errors.map((error) => error.message).join(", "));
  }

  const games: GameInfo[] = [];

  for (const panel of payload.data?.panels ?? []) {
    for (const section of panel.sections ?? []) {
      for (const item of section.items ?? []) {
        if (item.__typename === "GameItem" && item.app) {
          games.push(appToGame(item.app));
        }
      }
    }
  }

  console.log(`[GFN Metadata] flattenPanels: Extracted ${games.length} games from panels`);
  return games;
}

export async function fetchMainGames(token: string, providerStreamingBaseUrl?: string): Promise<GameInfo[]> {
  const cached = await cacheManager.loadFromCache<GameInfo[]>("games:main");
  if (cached) {
    return cached.data;
  }

  const games = await fetchMainGamesUncached(token, providerStreamingBaseUrl);
  await cacheManager.saveToCache("games:main", games);
  return games;
}

async function fetchMainGamesUncached(token: string, providerStreamingBaseUrl?: string): Promise<GameInfo[]> {
  const vpcId = await getVpcId(token, providerStreamingBaseUrl);
  const payload = await fetchPanels(token, ["MAIN"], vpcId);
  const games = flattenPanels(payload);
  const gfnEnriched = await enrichGamesWithMetadata(token, vpcId, games);
  return gfnEnriched;
}

export async function fetchLibraryGames(
  token: string,
  providerStreamingBaseUrl?: string,
): Promise<GameInfo[]> {
  const cached = await cacheManager.loadFromCache<GameInfo[]>("games:library");
  if (cached) {
    return cached.data;
  }

  const games = await fetchLibraryGamesUncached(token, providerStreamingBaseUrl);
  await cacheManager.saveToCache("games:library", games);
  return games;
}

async function fetchLibraryGamesUncached(
  token: string,
  providerStreamingBaseUrl?: string,
): Promise<GameInfo[]> {
  const vpcId = await getVpcId(token, providerStreamingBaseUrl);
  const payload = await fetchPanels(token, ["LIBRARY"], vpcId);
  const games = flattenPanels(payload);
  const gfnEnriched = await enrichGamesWithMetadata(token, vpcId, games);
  return gfnEnriched;
}

export async function fetchPublicGames(): Promise<GameInfo[]> {
  const cached = await cacheManager.loadFromCache<GameInfo[]>("games:public");
  if (cached) {
    return cached.data;
  }

  const games = await fetchPublicGamesUncached();
  await cacheManager.saveToCache("games:public", games);
  return games;
}

async function fetchPublicGamesUncached(): Promise<GameInfo[]> {
  const response = await fetch(
    "https://static.nvidiagrid.net/supported-public-game-list/locales/gfnpc-en-US.json",
    {
      headers: {
        "User-Agent": GFN_USER_AGENT,
      },
    },
  );

  if (!response.ok) {
    throw new Error(`Public games fetch failed (${response.status})`);
  }

  const payload = (await response.json()) as RawPublicGame[];
  const games = payload
    .filter((item) => item.status === "AVAILABLE" && item.title)
    .map((item) => {
      const id = String(item.id ?? item.title ?? "unknown");
      const steamAppId = item.steamUrl?.split("/app/")[1]?.split("/")[0];
      const imageUrl = steamAppId
        ? `https://cdn.cloudflare.steamstatic.com/steam/apps/${steamAppId}/library_600x900.jpg`
        : undefined;

      return {
        id,
        uuid: id,
        launchAppId: isNumericId(id) ? id : undefined,
        title: item.title ?? id,
        selectedVariantIndex: 0,
        variants: [{ id, store: "Unknown", supportedControls: [] }],
        imageUrl,
      } as GameInfo;
    });

  return games;

}

export async function resolveLaunchAppId(
  token: string,
  appIdOrUuid: string,
  providerStreamingBaseUrl?: string,
): Promise<string | null> {
  if (isNumericId(appIdOrUuid)) {
    return appIdOrUuid;
  }

  const vpcId = await getVpcId(token, providerStreamingBaseUrl);
  const payload = await fetchAppMetaData(token, [appIdOrUuid], vpcId);

  if (payload.errors?.length) {
    throw new Error(payload.errors.map((error) => error.message).join(", "));
  }

  const app = payload.data?.apps.items?.[0];
  if (!app) {
    return null;
  }

  const variants = app.variants ?? [];
  const selected = variants.find((variant) => variant.gfn?.library?.selected === true);

  if (isNumericId(selected?.id)) {
    return selected.id;
  }

  const firstNumeric = variants.find((variant) => isNumericId(variant.id));
  if (firstNumeric) {
    return firstNumeric.id;
  }

  return isNumericId(app.id) ? app.id : null;
}

export {
  fetchMainGamesUncached,
  fetchLibraryGamesUncached,
  fetchPublicGamesUncached,
};
