import type {
  CatalogBrowseRequest,
  CatalogBrowseResult,
  CatalogFilterGroup,
  CatalogSortOption,
  GameCatalogSkuStrings,
  GameInfo,
  GamePanelResult,
  GameVariant,
} from "@shared/gfn";
import { createHash } from "node:crypto";
import { isOwnedLibraryStatus, normalizeGameStore } from "@shared/gfn";
import { cacheManager } from "../services/cacheManager";
import { appendPublicGameSearchMatches, fetchPublicGamesUncached, mergePublicGameVariants } from "./publicGames";
import {
  buildGfnGraphQlHeaders,
  buildGfnLcarsHeaders,
} from "./clientHeaders";

const GRAPHQL_URL = "https://games.geforce.com/graphql";
const PANELS_QUERY_HASH = "f8e26265a5db5c20e1334a6872cf04b6e3970507697f6ae55a6ddefa5420daf0";
const MARQUEE_QUERY_HASH = "dd4bddfdef4707dfe340cc2040d6bb9c4c45f706976fca15b2ef33221c385d7f";
const APP_METADATA_QUERY_HASH = "cf8b620dfd03617017ba7c858cee65197e1ace5180e41be194b39227227ced63";
const LIBRARY_WITH_TIME_QUERY_HASH = "039e8c0d553972975485fee56e59f2549d2fdb518e247a42ab5022056a74406f";
const DEFAULT_LOCALE = "en_US";
const DEFAULT_CATALOG_FETCH_COUNT = 120;
const MAX_CATALOG_PAGES = 3;
const DEFAULT_SORT_ID = "relevance";
const PUBLIC_GAMES_CACHE_KEY = "games:public:v2";

function accountScopedGamesCacheKey(scope: string, accountId: string, providerStreamingBaseUrl?: string): string {
  const digest = createHash("sha256")
    .update(accountId)
    .update("\0")
    .update(providerStreamingBaseUrl ?? "")
    .digest("hex")
    .slice(0, 16);
  return `games:${scope}:${digest}`;
}

function legacyTokenScopedGamesCacheKey(scope: string, token: string, providerStreamingBaseUrl?: string): string {
  const digest = createHash("sha256")
    .update(token)
    .update("\0")
    .update(providerStreamingBaseUrl ?? "")
    .digest("hex")
    .slice(0, 16);
  return `games:${scope}:${digest}`;
}

function resolveAccountCacheId(accountId: string | undefined, token: string): string {
  return accountId?.trim() || token;
}

async function loadAccountScopedFromCache<T>(
  scope: string,
  accountId: string | undefined,
  token: string,
  providerStreamingBaseUrl?: string,
): Promise<Awaited<ReturnType<typeof cacheManager.loadFromCache<T>>>> {
  const resolvedAccountId = resolveAccountCacheId(accountId, token);
  const primaryKey = accountScopedGamesCacheKey(scope, resolvedAccountId, providerStreamingBaseUrl);
  const cached = await cacheManager.loadFromCache<T>(primaryKey);
  if (cached) {
    return cached;
  }

  if (resolvedAccountId !== token) {
    const legacyKey = legacyTokenScopedGamesCacheKey(scope, token, providerStreamingBaseUrl);
    if (legacyKey !== primaryKey) {
      const legacy = await cacheManager.loadFromCache<T>(legacyKey);
      if (legacy) {
        void cacheManager.saveToCache(primaryKey, legacy.data);
        void cacheManager.invalidateCache(legacyKey);
        return legacy;
      }
    }
  }

  return null;
}

function catalogBrowseCacheKey(input: CatalogBrowseRequest, accountId: string): string {
  const queryDigest = createHash("sha256")
    .update(input.searchQuery?.trim() ?? "")
    .update("\0")
    .update(input.sortId ?? "")
    .update("\0")
    .update((input.filterIds ?? []).join(","))
    .update("\0")
    .update(String(input.fetchCount ?? ""))
    .digest("hex")
    .slice(0, 12);
  return `${accountScopedGamesCacheKey("catalog", accountId, input.providerStreamingBaseUrl)}:${queryDigest}`;
}

export function getAccountGamesCacheKeys(accountId: string, providerStreamingBaseUrl?: string): {
  main: string;
  library: string;
  public: string;
} {
  return {
    main: accountScopedGamesCacheKey("main", accountId, providerStreamingBaseUrl),
    library: accountScopedGamesCacheKey("library", accountId, providerStreamingBaseUrl),
    public: PUBLIC_GAMES_CACHE_KEY,
  };
}

interface GraphQlResponse {
  data?: {
    panels: Array<{
      id?: string;
      name: string;
      sections: Array<{
        id?: string;
        title?: string;
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

interface FilterSortDefinitionsResponse {
  data?: {
    filterGroupDefinitions?: GraphQlFilterGroup[];
    sortOrderDefinitions?: Array<{
      id: string;
      label: string;
      orderBy: string;
    }>;
  };
  errors?: Array<{ message: string }>;
}

interface AppsSearchResponse {
  data?: {
    apps?: {
      numberReturned?: number;
      numberSupported?: number;
      pageInfo?: {
        hasNextPage?: boolean;
        endCursor?: string;
        totalCount?: number;
      };
      items?: AppData[];
    };
  };
  errors?: Array<{ message: string }>;
}

interface GraphQlFilterGroup {
  id: string;
  label: string;
  filters?: Array<{
    id: string;
    label: string;
    filters?: string[];
  }>;
}

interface AppData {
  id: string;
  title: string;
  shortName?: string;
  description?: string;
  longDescription?: string;
  developerName?: string;
  features?: unknown[];
  gameFeatures?: unknown[];
  appFeatures?: unknown[];
  genres?: unknown[];
  tags?: unknown[];
  supportedControls?: unknown[];
  nvidiaTech?: unknown[];
  maxLocalPlayers?: number;
  maxOnlinePlayers?: number;
  images?: Record<string, string | string[] | undefined>;
  publisherName?: string;
  contentRatings?: unknown[];
  variants?: Array<{
    id: string;
    appStore: string;
    storeUrl?: string;
    supportedControls?: string[];
    gfn?: {
      status?: string;
      library?: {
        status?: string;
        selected?: boolean;
        lastPlayedDate?: string;
      };
    };
  }>;
  gfn?: {
    playType?: string;
    playabilityState?: string;
    minimumMembershipTierLabel?: string;
    catalogSkuStrings?: GameCatalogSkuStrings;
  };
  itemMetadata?: {
    campaignIds?: string[];
  };
}

interface ServerInfoResponse {
  requestStatus?: {
    serverId?: string;
  };
}

interface AppResolution {
  numericAppId?: string;
  preferredVariantId?: string;
  selectedVariantIndex: number;
  lastPlayed?: string;
  isInLibrary: boolean;
}

interface CatalogDefinitions {
  filterGroups: CatalogFilterGroup[];
  sortOptions: CatalogSortOption[];
  filterPayloadById: Record<string, unknown>;
}

const LANDSCAPE_IMAGE_KEYS = ["MARQUEE_HERO_IMAGE", "HERO_IMAGE", "TV_BANNER", "FEATURE_IMAGE", "KEY_IMAGE", "KEY_ART"] as const;
const POSTER_IMAGE_KEYS = ["GAME_BOX_ART", "KEY_IMAGE", "KEY_ART"] as const;

function optimizeImage(url: string, width = 272): string {
  if (url.includes("img.nvidiagrid.net")) {
    return `${url};f=webp;w=${width}`;
  }
  return url;
}

function normalizeImageValues(value: string | string[] | undefined, width: number): string[] {
  const values = Array.isArray(value) ? value : value ? [value] : [];
  return [...new Set(values.map((url) => url.trim()).filter(Boolean).map((url) => optimizeImage(url, width)))];
}

function getFirstImage(images: AppData["images"], keys: readonly string[], width: number): string | undefined {
  if (!images) return undefined;
  for (const key of keys) {
    const value = normalizeImageValues(images[key], width)[0];
    if (value) return value;
  }
  return undefined;
}

function getImageUrlsByType(images: AppData["images"]): Record<string, string[]> | undefined {
  if (!images) return undefined;
  const entries = Object.entries(images)
    .map(([key, value]) => [key, normalizeImageValues(value, 1200)] as const)
    .filter(([, urls]) => urls.length > 0);
  return entries.length > 0 ? Object.fromEntries(entries) : undefined;
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

async function postGraphQl<T>(query: string, variables: Record<string, unknown>, token?: string): Promise<T> {
  const response = await fetch(GRAPHQL_URL, {
    method: "POST",
    headers: buildGfnGraphQlHeaders(token),
    body: JSON.stringify({ query, variables }),
  });

  if (!response.ok) {
    const text = await response.text();
    throw new Error(`GFN GraphQL failed (${response.status}): ${text.slice(0, 400)}`);
  }

  return (await response.json()) as T;
}

async function getVpcId(token: string, providerStreamingBaseUrl?: string): Promise<string> {
  const defaultBase = "https://prod.cloudmatchbeta.nvidiagrid.net/";
  const allowedHosts = new Set([
    "prod.cloudmatchbeta.nvidiagrid.net",
    "img.nvidiagrid.net",
  ]);

  let validatedBaseUrl: URL;
  try {
    const candidate = new URL(providerStreamingBaseUrl?.trim() || defaultBase);
    if (candidate.protocol !== "https:" || !allowedHosts.has(candidate.hostname)) {
      validatedBaseUrl = new URL(defaultBase);
    } else {
      validatedBaseUrl = candidate;
    }
  } catch {
    validatedBaseUrl = new URL(defaultBase);
  }

  const serverInfoUrl = new URL("v2/serverInfo", validatedBaseUrl);

  const response = await fetch(serverInfoUrl.toString(), {
    headers: buildGfnLcarsHeaders({
      token,
      clientType: "NATIVE",
      clientStreamer: "NVIDIA-CLASSIC",
      includeUserAgent: true,
      includeEmptyTokenAuthorization: true,
    }),
  });

  if (!response.ok) {
    return "GFN-PC";
  }

  const payload = (await response.json()) as ServerInfoResponse;
  return payload.requestStatus?.serverId ?? "GFN-PC";
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
    app.gfn?.catalogSkuStrings?.SKU_BASED_TAG,
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

function extractContentRatings(app: AppData): string[] {
  if (!Array.isArray(app.contentRatings)) {
    return [];
  }

  const labels: string[] = [];
  for (const entry of app.contentRatings) {
    const label = parseFeatureLabel(entry);
    if (label) {
      labels.push(label);
    }
  }

  return [...new Set(labels)];
}

function extractStringValues(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return [...new Set(value
    .map((entry) => typeof entry === "string" ? entry.trim() : parseFeatureLabel(entry))
    .filter((entry): entry is string => typeof entry === "string" && entry.length > 0))];
}

function buildSearchText(title: string, variants: GameVariant[], genres: string[], featureLabels: string[], publisherName?: string, developerName?: string): string {
  const stores = variants.map((variant) => variant.store);
  return [title, publisherName, developerName, ...stores, ...genres, ...featureLabels]
    .filter((value): value is string => typeof value === "string" && value.trim().length > 0)
    .join(" ")
    .toLowerCase();
}

function resolveAppData(app: AppData): AppResolution {
  const variants = app.variants ?? [];
  const selectedVariantIndex = variants.findIndex((variant) => variant.gfn?.library?.selected === true);
  const preferredVariant = selectedVariantIndex >= 0 ? variants[selectedVariantIndex] : undefined;
  const numericVariants = variants.filter((variant) => isNumericId(variant.id));
  const preferredNumericVariant = preferredVariant && isNumericId(preferredVariant.id) ? preferredVariant.id : undefined;
  const fallbackNumericVariant = numericVariants[0]?.id;
  const numericAppId = preferredNumericVariant ?? fallbackNumericVariant ?? (isNumericId(app.id) ? app.id : undefined);
  const preferredVariantId = preferredVariant?.id ?? numericAppId ?? variants[0]?.id ?? app.id;
  const lastPlayed = variants
    .map((variant) => variant.gfn?.library?.lastPlayedDate)
    .find((value): value is string => typeof value === "string" && value.length > 0);
  const isInLibrary = variants.some((variant) => isOwnedLibraryStatus(variant.gfn?.library?.status));

  return {
    numericAppId,
    preferredVariantId,
    selectedVariantIndex: selectedVariantIndex >= 0 ? selectedVariantIndex : Math.max(0, variants.findIndex((variant) => variant.id === preferredVariantId)),
    lastPlayed,
    isInLibrary,
  };
}

function appToVariants(app: AppData): GameVariant[] {
  return app.variants?.map((variant) => ({
    id: variant.id,
    store: variant.appStore,
    storeUrl: variant.storeUrl,
    supportedControls: variant.supportedControls ?? [],
    librarySelected: variant.gfn?.library?.selected,
    inLibrary: variant.gfn?.library?.selected === true,
    libraryStatus: variant.gfn?.library?.status,
    lastPlayedDate: variant.gfn?.library?.lastPlayedDate,
    gfnStatus: variant.gfn?.status,
  })) ?? [];
}

function appToGame(app: AppData): GameInfo {
  const variants = appToVariants(app);
  const resolution = resolveAppData(app);
  const heroImageUrl = getFirstImage(app.images, LANDSCAPE_IMAGE_KEYS, 1200);
  const posterImageUrl = getFirstImage(app.images, POSTER_IMAGE_KEYS, 900);
  const imageUrl = heroImageUrl ?? posterImageUrl;
  const screenshotUrls = normalizeImageValues(app.images?.SCREENSHOTS, 720);
  const genres = extractGenres(app);
  const featureLabels = extractFeatureLabels(app);
  const supportedControls = extractStringValues(app.supportedControls);
  const nvidiaTech = extractStringValues(app.nvidiaTech);

  return {
    id: app.id,
    uuid: app.id,
    launchAppId: resolution.numericAppId,
    title: app.title,
    shortName: app.shortName,
    description: app.description,
    longDescription: app.longDescription,
    developerName: app.developerName,
    maxLocalPlayers: app.maxLocalPlayers,
    maxOnlinePlayers: app.maxOnlinePlayers,
    featureLabels,
    genres,
    supportedControls: supportedControls.length > 0 ? supportedControls : undefined,
    nvidiaTech: nvidiaTech.length > 0 ? nvidiaTech : undefined,
    imageUrl,
    heroImageUrl,
    screenshotUrl: screenshotUrls[0],
    screenshotUrls: screenshotUrls.length > 0 ? screenshotUrls : undefined,
    imageUrlsByType: getImageUrlsByType(app.images),
    playType: app.gfn?.playType,
    membershipTierLabel: app.gfn?.minimumMembershipTierLabel,
    catalogSkuStrings: app.gfn?.catalogSkuStrings,
    publisherName: app.publisherName,
    contentRatings: extractContentRatings(app),
    playabilityState: app.gfn?.playabilityState,
    availableStores: [...new Set(variants.map((variant) => variant.store).filter(Boolean))],
    searchText: buildSearchText(app.title, variants, genres, featureLabels, app.publisherName, app.developerName),
    lastPlayed: resolution.lastPlayed,
    isInLibrary: resolution.isInLibrary,
    selectedVariantIndex: Math.max(0, Math.min(resolution.selectedVariantIndex, Math.max(variants.length - 1, 0))),
    variants,
  };
}

function mergeAppMetaIntoGame(game: GameInfo, app: AppData): GameInfo {
  const merged = appToGame(app);
  const selectedVariantId = game.variants[game.selectedVariantIndex]?.id;
  const selectedVariantIndex = selectedVariantId
    ? merged.variants.findIndex((variant) => variant.id === selectedVariantId)
    : -1;

  return {
    ...game,
    ...merged,
    id: game.id,
    selectedVariantIndex: selectedVariantIndex >= 0 ? selectedVariantIndex : merged.selectedVariantIndex,
  };
}

function dedupeGames(games: GameInfo[]): GameInfo[] {
  const byId = new Map<string, GameInfo>();

  for (const game of games) {
    const existing = byId.get(game.id);
    if (!existing) {
      byId.set(game.id, game);
      continue;
    }

    const mergedVariants = new Map<string, GameVariant>();
    for (const variant of [...existing.variants, ...game.variants]) {
      mergedVariants.set(variant.id, variant);
    }

    const merged: GameInfo = {
      ...existing,
      ...game,
      id: existing.id,
      uuid: existing.uuid ?? game.uuid,
      launchAppId: existing.launchAppId ?? game.launchAppId,
      title: existing.title || game.title,
      shortName: existing.shortName ?? game.shortName,
      description: existing.description ?? game.description,
      longDescription: existing.longDescription ?? game.longDescription,
      developerName: existing.developerName ?? game.developerName,
      maxLocalPlayers: existing.maxLocalPlayers ?? game.maxLocalPlayers,
      maxOnlinePlayers: existing.maxOnlinePlayers ?? game.maxOnlinePlayers,
      imageUrl: existing.imageUrl ?? game.imageUrl,
      heroImageUrl: existing.heroImageUrl ?? game.heroImageUrl,
      screenshotUrl: existing.screenshotUrl ?? game.screenshotUrl,
      screenshotUrls: [...new Set([...(existing.screenshotUrls ?? []), ...(game.screenshotUrls ?? [])])],
      imageUrlsByType: {
        ...(game.imageUrlsByType ?? {}),
        ...(existing.imageUrlsByType ?? {}),
      },
      playType: existing.playType ?? game.playType,
      membershipTierLabel: existing.membershipTierLabel ?? game.membershipTierLabel,
      catalogSkuStrings: existing.catalogSkuStrings ?? game.catalogSkuStrings,
      publisherName: existing.publisherName ?? game.publisherName,
      playabilityState: existing.playabilityState ?? game.playabilityState,
      lastPlayed: existing.lastPlayed ?? game.lastPlayed,
      isInLibrary: existing.isInLibrary || game.isInLibrary,
      variants: [...mergedVariants.values()],
      genres: [...new Set([...(existing.genres ?? []), ...(game.genres ?? [])])],
      featureLabels: [...new Set([...(existing.featureLabels ?? []), ...(game.featureLabels ?? [])])],
      supportedControls: [...new Set([...(existing.supportedControls ?? []), ...(game.supportedControls ?? [])])],
      nvidiaTech: [...new Set([...(existing.nvidiaTech ?? []), ...(game.nvidiaTech ?? [])])],
      contentRatings: [...new Set([...(existing.contentRatings ?? []), ...(game.contentRatings ?? [])])],
      availableStores: [...new Set([...(existing.availableStores ?? []), ...(game.availableStores ?? [])])],
      searchText: [existing.searchText, game.searchText].filter(Boolean).join(" ").trim() || undefined,
      selectedVariantIndex: Math.max(0, existing.variants[existing.selectedVariantIndex]
        ? [...mergedVariants.values()].findIndex((variant) => variant.id === existing.variants[existing.selectedVariantIndex]?.id)
        : game.selectedVariantIndex),
    };

    byId.set(game.id, merged);
  }

  return [...byId.values()];
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

  const response = await fetch(`${GRAPHQL_URL}?${params.toString()}`, {
    headers: {
      ...buildGfnGraphQlHeaders(token),
      "Content-Type": "application/graphql",
    },
  });

  if (!response.ok) {
    const text = await response.text();
    throw new Error(`App metadata failed (${response.status}): ${text.slice(0, 400)}`);
  }

  return (await response.json()) as AppMetaDataResponse;
}

async function enrichGamesWithMetadata(token: string, vpcId: string, games: GameInfo[]): Promise<GameInfo[]> {
  const uuids = [...new Set(games.map((game) => game.uuid).filter((uuid): uuid is string => !!uuid))];

  if (uuids.length === 0) {
    return games;
  }

  const chunkSize = 40;
  const appById = new Map<string, AppData>();

  for (let index = 0; index < uuids.length; index += chunkSize) {
    const chunk = uuids.slice(index, index + chunkSize);
    const payload = await fetchAppMetaData(token, chunk, vpcId);
    if (payload.errors?.length) {
      throw new Error(payload.errors.map((error) => error.message).join(", "));
    }

    for (const app of payload.data?.apps.items ?? []) {
      appById.set(app.id, app);
    }
  }

  return dedupeGames(
    games.map((game) => {
      const metadata = game.uuid ? appById.get(game.uuid) : undefined;
      return metadata ? mergeAppMetaIntoGame(game, metadata) : game;
    }),
  );
}

async function fetchPanels(
  token: string,
  panelNames: string[],
  vpcId: string,
  options?: { withLibraryTime?: boolean },
): Promise<GraphQlResponse> {
  const variables = JSON.stringify({
    vpcId,
    locale: DEFAULT_LOCALE,
    panelNames,
  });

  const extensions = JSON.stringify({
    persistedQuery: {
      sha256Hash: panelNames.includes("MARQUEE")
        ? MARQUEE_QUERY_HASH
        : options?.withLibraryTime
          ? LIBRARY_WITH_TIME_QUERY_HASH
          : PANELS_QUERY_HASH,
    },
  });

  const requestType = panelNames.includes("MARQUEE")
    ? "panels/Marquee"
    : panelNames.includes("LIBRARY")
      ? "panels/Library"
      : "panels/MainV2";
  const params = new URLSearchParams({
    requestType,
    extensions,
    huId: randomHuId(),
    variables,
  });

  const response = await fetch(`${GRAPHQL_URL}?${params.toString()}`, {
    headers: {
      ...buildGfnGraphQlHeaders(token),
      "Content-Type": "application/graphql",
    },
  });

  if (!response.ok) {
    const text = await response.text();
    throw new Error(`Games GraphQL failed (${response.status}): ${text.slice(0, 400)}`);
  }

  return (await response.json()) as GraphQlResponse;
}

function panelTextMatchesFeatured(value: string | undefined): boolean {
  return value?.toLowerCase().includes("featured") ?? false;
}

function getFeaturedGameIdentity(game: GameInfo): string {
  return game.id || game.uuid || game.launchAppId || game.title;
}

function featuredGamesFromPanels(payload: GraphQlResponse): GameInfo[] {
  if (payload.errors?.length) {
    throw new Error(payload.errors.map((error) => error.message).join(", "));
  }

  const explicitGames: GameInfo[] = [];
  const explicitIds = new Set<string>();
  const curatedGames: GameInfo[] = [];
  const curatedIds = new Set<string>();

  const appendUnique = (target: GameInfo[], seen: Set<string>, game: GameInfo): void => {
    const identity = getFeaturedGameIdentity(game);
    if (!identity || seen.has(identity)) return;
    seen.add(identity);
    target.push(game);
  };

  for (const panel of payload.data?.panels ?? []) {
    const panelFeatured = panelTextMatchesFeatured(panel.name) || panelTextMatchesFeatured(panel.id);
    for (const section of panel.sections ?? []) {
      const sectionFeatured = panelFeatured || panelTextMatchesFeatured(section.title) || panelTextMatchesFeatured(section.id);
      for (const item of section.items ?? []) {
        if (item.__typename !== "GameItem" || !item.app) continue;
        const game = appToGame(item.app);
        if (!game.id || !game.title || game.variants.length === 0) continue;
        appendUnique(curatedGames, curatedIds, game);
        if (sectionFeatured) appendUnique(explicitGames, explicitIds, game);
      }
    }
  }

  return explicitGames.length > 0 ? explicitGames : curatedGames;
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

  return dedupeGames(games);
}

function parsePanelResults(payload: GraphQlResponse): GamePanelResult[] {
  if (payload.errors?.length) {
    throw new Error(payload.errors.map((error) => error.message).join(", "));
  }

  const panels: GamePanelResult[] = [];
  for (const panel of payload.data?.panels ?? []) {
    const sections = (panel.sections ?? [])
      .map((section) => ({
        id: section.id ?? section.title ?? "",
        title: section.title ?? "",
        games: (section.items ?? [])
          .filter((item) => item.__typename === "GameItem" && item.app)
          .map((item) => appToGame(item.app as AppData))
          .filter((game) => game.id && game.title && game.variants.length > 0),
      }))
      .filter((section) => section.games.length > 0);

    if (sections.length > 0) {
      panels.push({
        id: panel.id ?? panel.name,
        title: panel.name,
        sections,
      });
    }
  }
  return panels;
}

async function fetchFilterAndSortDefinitions(token?: string): Promise<CatalogDefinitions> {
  const query = `query GetFilterGroupAndSortOrderDefinitions($locale: String!) {
    filterGroupDefinitions(language: $locale) {
      id
      label
      filters {
        id
        label
        filters
      }
    }
    sortOrderDefinitions(language: $locale) {
      id
      label
      orderBy
    }
  }`;

  const payload = await postGraphQl<FilterSortDefinitionsResponse>(query, { locale: DEFAULT_LOCALE }, token);
  if (payload.errors?.length) {
    throw new Error(payload.errors.map((error) => error.message).join(", "));
  }

  const filterPayloadById: Record<string, unknown> = {};
  const filterGroups: CatalogFilterGroup[] = [];

  for (const group of payload.data?.filterGroupDefinitions ?? []) {
    const options = (group.filters ?? []).flatMap((entry) => {
      const filterJson = entry.filters?.[0];
      if (!filterJson) {
        return [];
      }
      try {
        filterPayloadById[entry.id] = JSON.parse(filterJson);
        return [{
          id: entry.id,
          rawId: entry.id,
          label: entry.label,
          groupId: group.id,
          groupLabel: group.label,
        }];
      } catch {
        return [];
      }
    });

    if (options.length > 0) {
      filterGroups.push({ id: group.id, label: group.label, options });
    }
  }

  const sortOptions = (payload.data?.sortOrderDefinitions ?? []).map((sort) => ({
    id: sort.id,
    label: sort.label,
    orderBy: sort.orderBy,
  }));

  return {
    filterGroups,
    sortOptions,
    filterPayloadById,
  };
}

function mergeFilterPayloads(filterIds: string[], filterPayloadById: Record<string, unknown>): Record<string, unknown> {
  const merged: Record<string, unknown> = {};

  for (const filterId of filterIds) {
    const payload = filterPayloadById[filterId];
    if (!payload || typeof payload !== "object") {
      continue;
    }
    Object.assign(merged, payload as Record<string, unknown>);
  }

  return merged;
}

async function browseCatalogUncached(input: CatalogBrowseRequest): Promise<CatalogBrowseResult> {
  const token = input.token;
  if (!token) {
    throw new Error("Catalog browsing requires an authenticated token");
  }

  const vpcId = await getVpcId(token, input.providerStreamingBaseUrl);
  const definitions = await fetchFilterAndSortDefinitions(token);
  const normalizedFilterIds = (input.filterIds ?? []).filter((id) => id in definitions.filterPayloadById);
  const selectedSort = definitions.sortOptions.find((option) => option.id === input.sortId)
    ?? definitions.sortOptions.find((option) => option.id === DEFAULT_SORT_ID)
    ?? definitions.sortOptions[0]
    ?? { id: DEFAULT_SORT_ID, label: "Relevance", orderBy: "itemMetadata.relevance:DESC,sortName:ASC" };
  const searchQuery = input.searchQuery?.trim() ?? "";
  const fetchCount = Math.max(24, Math.min(input.fetchCount ?? DEFAULT_CATALOG_FETCH_COUNT, 200));
  const filters = mergeFilterPayloads(normalizedFilterIds, definitions.filterPayloadById);

  const appFields = `
      numberReturned
      numberSupported
      pageInfo { hasNextPage endCursor totalCount }
      items {
        id
        title
        images { KEY_ART KEY_IMAGE GAME_BOX_ART TV_BANNER HERO_IMAGE MARQUEE_HERO_IMAGE FEATURE_IMAGE GAME_LOGO SCREENSHOTS }
        variants {
          id
          appStore
          storeUrl
          supportedControls
          gfn {
            status
            library { status selected }
          }
        }
        gfn {
          playabilityState
          minimumMembershipTierLabel
          catalogSkuStrings {
            SKU_BASED_TAG
            SKU_BASED_PLAYABILITY_TEXT
            SKU_BASED_UNPLAYABLE_DIALOG_HEADER
            SKU_BASED_UNPLAYABLE_DIALOG_BODY_UPGRADE
            SKU_BASED_UNPLAYABLE_DIALOG_BODY_UPGRADE_ECOMM_RESTRICTED
          }
        }
        itemMetadata { campaignIds }
      }
  `;

  const query = searchQuery.length > 0
    ? `query GetSearchFilterResults(
      $vpcId: String!,
      $locale: String!,
      $sortString: String!,
      $fetchCount: Int!,
      $cursor: String!,
      $searchString: String!,
      $filters: AppFilterFields!
    ) {
      apps(
        vpcId: $vpcId,
        language: $locale,
        orderBy: $sortString,
        first: $fetchCount,
        after: $cursor,
        searchQuery: $searchString,
        filters: $filters
      ) {
${appFields}
      }
    }`
    : `query GetFilterBrowseResults(
      $vpcId: String!,
      $locale: String!,
      $sortString: String!,
      $fetchCount: Int!,
      $cursor: String!,
      $filters: AppFilterFields!
    ) {
      apps(
        vpcId: $vpcId,
        language: $locale,
        orderBy: $sortString,
        first: $fetchCount,
        after: $cursor,
        filters: $filters
      ) {
${appFields}
      }
    }`;

  const collectedApps: AppData[] = [];
  let numberReturned = 0;
  let numberSupported = 0;
  let totalCount = 0;
  let hasNextPage = false;
  let endCursor = "";
  let cursor = "";

  for (let page = 0; page < MAX_CATALOG_PAGES; page += 1) {
    const payload = await postGraphQl<AppsSearchResponse>(
      query,
      searchQuery.length > 0
        ? {
            vpcId,
            locale: DEFAULT_LOCALE,
            sortString: selectedSort.orderBy,
            fetchCount,
            cursor,
            searchString: searchQuery,
            filters,
          }
        : {
            vpcId,
            locale: DEFAULT_LOCALE,
            sortString: selectedSort.orderBy,
            fetchCount,
            cursor,
            filters,
          },
      token,
    );

    if (payload.errors?.length) {
      throw new Error(payload.errors.map((error) => error.message).join(", "));
    }

    const apps = payload.data?.apps;
    const items = apps?.items ?? [];
    collectedApps.push(...items);
    numberReturned += apps?.numberReturned ?? items.length;
    numberSupported = apps?.numberSupported ?? numberSupported;
    hasNextPage = apps?.pageInfo?.hasNextPage ?? false;
    endCursor = apps?.pageInfo?.endCursor ?? "";
    totalCount = apps?.pageInfo?.totalCount ?? totalCount;

    if (!hasNextPage || !endCursor) {
      break;
    }

    cursor = endCursor;
  }

  let games = dedupeGames(await enrichGamesWithMetadata(token, vpcId, collectedApps.map(appToGame)));
  const publicGames = await fetchPublicGames();
  const gamesWithPublicVariants = appendPublicGameSearchMatches(
    mergePublicGameVariants(games, publicGames),
    publicGames,
    searchQuery,
  );

  return {
    games: gamesWithPublicVariants,
    numberReturned,
    numberSupported: Math.max(numberSupported, gamesWithPublicVariants.length),
    totalCount: Math.max(totalCount, gamesWithPublicVariants.length),
    hasNextPage,
    endCursor: endCursor || undefined,
    searchQuery,
    selectedSortId: selectedSort.id,
    selectedFilterIds: normalizedFilterIds,
    filterGroups: definitions.filterGroups,
    sortOptions: definitions.sortOptions,
  };
}

export async function browseCatalog(input: CatalogBrowseRequest): Promise<CatalogBrowseResult> {
  const token = input.token;
  if (!token) {
    throw new Error("Catalog browsing requires an authenticated token");
  }

  const cached = await peekCachedBrowseCatalog(input);
  if (cached) {
    return cached;
  }

  const result = await browseCatalogUncached(input);
  const accountId = resolveAccountCacheId(input.userId, token);
  const cacheKey = catalogBrowseCacheKey(input, accountId);
  await cacheManager.saveToCache(cacheKey, result);
  return result;
}

export async function peekCachedBrowseCatalog(input: CatalogBrowseRequest): Promise<CatalogBrowseResult | null> {
  const token = input.token;
  if (!token) {
    return null;
  }

  const accountId = resolveAccountCacheId(input.userId, token);
  const cacheKey = catalogBrowseCacheKey(input, accountId);
  const cached = await cacheManager.loadFromCache<CatalogBrowseResult>(cacheKey);
  return cached?.data ?? null;
}

export async function peekCachedLibraryGames(
  token: string,
  providerStreamingBaseUrl?: string,
  accountId?: string,
): Promise<GameInfo[] | null> {
  const cached = await loadAccountScopedFromCache<GameInfo[]>("library", accountId, token, providerStreamingBaseUrl);
  return cached?.data ?? null;
}

export async function fetchLibraryGamesFromCache(
  token: string,
  providerStreamingBaseUrl?: string,
  accountId?: string,
): Promise<GameInfo[] | null> {
  const cached = await peekCachedLibraryGames(token, providerStreamingBaseUrl, accountId);
  if (!cached) {
    return null;
  }
  return mergePublicGameVariants(cached, await fetchPublicGames());
}

export async function fetchMainGames(
  token: string,
  providerStreamingBaseUrl?: string,
  accountId?: string,
): Promise<GameInfo[]> {
  const cached = await loadAccountScopedFromCache<GameInfo[]>("main", accountId, token, providerStreamingBaseUrl);
  if (cached) {
    return mergePublicGameVariants(cached.data, await fetchPublicGames());
  }

  const games = await fetchMainGamesUncached(token, providerStreamingBaseUrl);
  const cacheKey = accountScopedGamesCacheKey("main", resolveAccountCacheId(accountId, token), providerStreamingBaseUrl);
  await cacheManager.saveToCache(cacheKey, games);
  return games;
}

export async function fetchFeaturedGames(
  token: string,
  providerStreamingBaseUrl?: string,
  accountId?: string,
): Promise<GameInfo[]> {
  const cached = await loadAccountScopedFromCache<GameInfo[]>("featured", accountId, token, providerStreamingBaseUrl);
  if (cached) return cached.data;

  const vpcId = await getVpcId(token, providerStreamingBaseUrl);
  const games = featuredGamesFromPanels(await fetchPanels(token, ["MARQUEE"], vpcId)).slice(0, 6);

  const cacheKey = accountScopedGamesCacheKey("featured", resolveAccountCacheId(accountId, token), providerStreamingBaseUrl);
  await cacheManager.saveToCache(cacheKey, games);
  return games;
}

export async function fetchStorePanels(
  token: string,
  providerStreamingBaseUrl?: string,
  accountId?: string,
): Promise<GamePanelResult[]> {
  const cached = await loadAccountScopedFromCache<GamePanelResult[]>("store-panels", accountId, token, providerStreamingBaseUrl);
  if (cached) return cached.data;

  const vpcId = await getVpcId(token, providerStreamingBaseUrl);
  const panels = parsePanelResults(await fetchPanels(token, ["MAIN"], vpcId));
  const cacheKey = accountScopedGamesCacheKey("store-panels", resolveAccountCacheId(accountId, token), providerStreamingBaseUrl);
  await cacheManager.saveToCache(cacheKey, panels);
  return panels;
}

async function fetchMainGamesUncached(token: string, providerStreamingBaseUrl?: string): Promise<GameInfo[]> {
  const vpcId = await getVpcId(token, providerStreamingBaseUrl);
  const payload = await fetchPanels(token, ["MAIN"], vpcId);
  const games = flattenPanels(payload);
  return mergePublicGameVariants(await enrichGamesWithMetadata(token, vpcId, games), await fetchPublicGames());
}

export async function fetchLibraryGames(
  token: string,
  providerStreamingBaseUrl?: string,
  accountId?: string,
): Promise<GameInfo[]> {
  const cached = await loadAccountScopedFromCache<GameInfo[]>("library", accountId, token, providerStreamingBaseUrl);
  if (cached) {
    return mergePublicGameVariants(cached.data, await fetchPublicGames());
  }

  const games = await fetchLibraryGamesUncached(token, providerStreamingBaseUrl);
  const cacheKey = accountScopedGamesCacheKey("library", resolveAccountCacheId(accountId, token), providerStreamingBaseUrl);
  await cacheManager.saveToCache(cacheKey, games);
  return games;
}

async function fetchLibraryGamesUncached(
  token: string,
  providerStreamingBaseUrl?: string,
): Promise<GameInfo[]> {
  const vpcId = await getVpcId(token, providerStreamingBaseUrl);
  let payload: GraphQlResponse;

  try {
    payload = await fetchPanels(token, ["LIBRARY"], vpcId, { withLibraryTime: true });
  } catch {
    payload = await fetchPanels(token, ["LIBRARY"], vpcId);
  }

  const games = flattenPanels(payload);
  return mergePublicGameVariants(await enrichGamesWithMetadata(token, vpcId, games), await fetchPublicGames());
}

export async function fetchPublicGames(): Promise<GameInfo[]> {
  const cached = await cacheManager.loadFromCache<GameInfo[]>(PUBLIC_GAMES_CACHE_KEY);
  if (cached) {
    return cached.data;
  }

  const games = await fetchPublicGamesUncached();
  await cacheManager.saveToCache(PUBLIC_GAMES_CACHE_KEY, games);
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

  return resolveAppData(app).numericAppId ?? null;
}

export async function resolveStoreUrl(
  token: string,
  appIdOrUuid: string,
  providerStreamingBaseUrl?: string,
  options: { variantId?: string; store?: string } = {},
): Promise<string | null> {
  const vpcId = await getVpcId(token, providerStreamingBaseUrl);
  const payload = await fetchAppMetaData(token, [appIdOrUuid], vpcId);

  if (payload.errors?.length) {
    throw new Error(payload.errors.map((error) => error.message).join(", "));
  }

  const app = payload.data?.apps.items?.[0];
  const variants = app?.variants ?? [];
  const selectedVariant = options.variantId
    ? variants.find((variant) => variant.id === options.variantId)
    : undefined;
  if (selectedVariant?.storeUrl) return selectedVariant.storeUrl;

  const storeKey = options.store ? normalizeGameStore(options.store) : undefined;
  const matchingStoreVariant = storeKey
    ? variants.find((variant) => normalizeGameStore(variant.appStore) === storeKey && variant.storeUrl)
    : undefined;
  if (matchingStoreVariant?.storeUrl) return matchingStoreVariant.storeUrl;

  return variants.find((variant) => variant.storeUrl)?.storeUrl ?? null;
}

export {
  browseCatalogUncached,
  fetchMainGamesUncached,
  fetchLibraryGamesUncached,
  fetchPublicGamesUncached,
};
