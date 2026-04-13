import type {
  CatalogBrowseRequest,
  CatalogBrowseResult,
  CatalogFilterGroup,
  CatalogSortOption,
  GameInfo,
  GameVariant,
} from "@shared/gfn";
import { cacheManager } from "../services/cacheManager";

const GRAPHQL_URL = "https://games.geforce.com/graphql";
const PANELS_QUERY_HASH = "f8e26265a5db5c20e1334a6872cf04b6e3970507697f6ae55a6ddefa5420daf0";
const APP_METADATA_QUERY_HASH = "39187e85b6dcf60b7279a5f233288b0a8b69a8b1dbcfb5b25555afdcb988f0d7";
const LIBRARY_WITH_TIME_QUERY_HASH = "039e8c0d553972975485fee56e59f2549d2fdb518e247a42ab5022056a74406f";
const DEFAULT_LOCALE = "en_US";
const LCARS_CLIENT_ID = "ec7e38d4-03af-4b58-b131-cfb0495903ab";
const GFN_CLIENT_VERSION = "2.0.80.173";
const DEFAULT_CATALOG_FETCH_COUNT = 120;
const MAX_CATALOG_PAGES = 3;
const DEFAULT_SORT_ID = "relevance";

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
    KEY_ART?: string;
  };
  publisherName?: string;
  contentRatings?: unknown[];
  variants?: Array<{
    id: string;
    appStore: string;
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
    catalogSkuStrings?: {
      SKU_BASED_TAG?: string[];
    };
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

interface RawPublicGame {
  id?: string | number;
  title?: string;
  steamUrl?: string;
  status?: string;
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

function buildHeaders(token?: string): HeadersInit {
  return {
    Accept: "application/json, text/plain, */*",
    "Content-Type": "application/json",
    Origin: "https://play.geforcenow.com",
    Referer: "https://play.geforcenow.com/",
    ...(token ? { Authorization: `GFNJWT ${token}` } : {}),
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
  };
}

async function postGraphQl<T>(query: string, variables: Record<string, unknown>, token?: string): Promise<T> {
  const response = await fetch(GRAPHQL_URL, {
    method: "POST",
    headers: buildHeaders(token),
    body: JSON.stringify({ query, variables }),
  });

  if (!response.ok) {
    const text = await response.text();
    throw new Error(`GFN GraphQL failed (${response.status}): ${text.slice(0, 400)}`);
  }

  return (await response.json()) as T;
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

function buildSearchText(title: string, variants: GameVariant[], genres: string[], featureLabels: string[], publisherName?: string): string {
  const stores = variants.map((variant) => variant.store);
  return [title, publisherName, ...stores, ...genres, ...featureLabels]
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
  const isInLibrary = variants.some((variant) => variant.gfn?.library?.status === "IN_LIBRARY" || variant.gfn?.library?.selected === true);

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
    supportedControls: variant.supportedControls ?? [],
    librarySelected: variant.gfn?.library?.selected,
    libraryStatus: variant.gfn?.library?.status,
    lastPlayedDate: variant.gfn?.library?.lastPlayedDate,
    gfnStatus: variant.gfn?.status,
  })) ?? [];
}

function appToGame(app: AppData): GameInfo {
  const variants = appToVariants(app);
  const resolution = resolveAppData(app);
  const imageUrl =
    app.images?.KEY_ART ?? app.images?.GAME_BOX_ART ?? app.images?.TV_BANNER ?? app.images?.HERO_IMAGE ?? undefined;
  const genres = extractGenres(app);
  const featureLabels = extractFeatureLabels(app);

  return {
    id: app.id,
    uuid: app.id,
    launchAppId: resolution.numericAppId,
    title: app.title,
    description: app.description,
    longDescription: app.longDescription,
    featureLabels,
    genres,
    imageUrl: imageUrl ? optimizeImage(imageUrl) : undefined,
    playType: app.gfn?.playType,
    membershipTierLabel: app.gfn?.minimumMembershipTierLabel,
    publisherName: app.publisherName,
    contentRatings: extractContentRatings(app),
    playabilityState: app.gfn?.playabilityState,
    availableStores: [...new Set(variants.map((variant) => variant.store).filter(Boolean))],
    searchText: buildSearchText(app.title, variants, genres, featureLabels, app.publisherName),
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
      description: existing.description ?? game.description,
      longDescription: existing.longDescription ?? game.longDescription,
      imageUrl: existing.imageUrl ?? game.imageUrl,
      playType: existing.playType ?? game.playType,
      membershipTierLabel: existing.membershipTierLabel ?? game.membershipTierLabel,
      publisherName: existing.publisherName ?? game.publisherName,
      playabilityState: existing.playabilityState ?? game.playabilityState,
      lastPlayed: existing.lastPlayed ?? game.lastPlayed,
      isInLibrary: existing.isInLibrary || game.isInLibrary,
      variants: [...mergedVariants.values()],
      genres: [...new Set([...(existing.genres ?? []), ...(game.genres ?? [])])],
      featureLabels: [...new Set([...(existing.featureLabels ?? []), ...(game.featureLabels ?? [])])],
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
      ...buildHeaders(token),
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
      sha256Hash: options?.withLibraryTime ? LIBRARY_WITH_TIME_QUERY_HASH : PANELS_QUERY_HASH,
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
      ...buildHeaders(token),
      "Content-Type": "application/graphql",
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

  return dedupeGames(games);
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
        images { TV_BANNER HERO_IMAGE }
        variants {
          id
          appStore
          supportedControls
          gfn {
            status
            library { status selected }
          }
        }
        gfn {
          playabilityState
          minimumMembershipTierLabel
          catalogSkuStrings { SKU_BASED_TAG }
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

  const games = dedupeGames(collectedApps.map(appToGame));

  return {
    games,
    numberReturned,
    numberSupported: Math.max(numberSupported, games.length),
    totalCount: Math.max(totalCount, games.length),
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
  return browseCatalogUncached(input);
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
  return enrichGamesWithMetadata(token, vpcId, games);
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
  let payload: GraphQlResponse;

  try {
    payload = await fetchPanels(token, ["LIBRARY"], vpcId, { withLibraryTime: true });
  } catch {
    payload = await fetchPanels(token, ["LIBRARY"], vpcId);
  }

  const games = flattenPanels(payload);
  return enrichGamesWithMetadata(token, vpcId, games);
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
  return payload
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
        searchText: (item.title ?? id).toLowerCase(),
        selectedVariantIndex: 0,
        variants: [{ id, store: "Unknown", supportedControls: [] }],
        imageUrl,
        availableStores: ["Unknown"],
        isInLibrary: false,
      } as GameInfo;
    });
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

export {
  browseCatalogUncached,
  fetchMainGamesUncached,
  fetchLibraryGamesUncached,
  fetchPublicGamesUncached,
};
