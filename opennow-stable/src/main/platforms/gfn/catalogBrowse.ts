import type {
  CatalogBrowseRequest,
  CatalogBrowseResult,
  CatalogFilterGroup,
  CatalogSortOption,
  GameInfo,
  GamePanelResult,
} from "@shared/gfn";
import { cacheManager } from "../../services/cacheManager";
import { appendPublicGameSearchMatches, mergePublicGameVariants } from "./publicGames";
import { fetchLcarsGraphQl } from "./lcarsGraphql";
import {
  accountScopedGamesCacheKey,
  catalogBrowseCacheKey,
  fetchPublicGames,
  loadAccountScopedFromCache,
  resolveAccountCacheId,
  shouldBypassGamesCache,
} from "./gamesCache";
import {
  appToGame,
  type AppData,
  DEFAULT_LOCALE,
  dedupeGames,
  enrichGamesWithMetadata,
  GFN_FEATURE_FIELDS,
  type GraphQlResponse,
  getVpcId,
} from "./gameAppMapper";

const DEFAULT_CATALOG_FETCH_COUNT = 120;
const MAX_CATALOG_PAGES = 3;
const DEFAULT_SORT_ID = "relevance";

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

interface CatalogDefinitions {
  filterGroups: CatalogFilterGroup[];
  sortOptions: CatalogSortOption[];
  filterPayloadById: Record<string, unknown>;
}

async function fetchPanels(
  token: string,
  panelNames: string[],
  vpcId: string,
  options?: { withLibraryTime?: boolean },
  proxyUrl?: string,
): Promise<GraphQlResponse> {
  const queryName = panelNames.includes("MARQUEE")
    ? "Marquee"
    : panelNames.includes("LIBRARY")
      ? options?.withLibraryTime === true ? "LibrarySectionWithTime" : "LibrarySection"
      : "Main";

  return await fetchLcarsGraphQl<GraphQlResponse>(
    queryName,
    {
      vpcId,
      locale: DEFAULT_LOCALE,
      panelNames,
    },
    token,
    proxyUrl,
    { context: "Games GraphQL failed" },
  );
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

export function flattenPanels(payload: GraphQlResponse): GameInfo[] {
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

async function fetchFilterAndSortDefinitions(token?: string, proxyUrl?: string): Promise<CatalogDefinitions> {
  const payload = await fetchLcarsGraphQl<FilterSortDefinitionsResponse>(
    "FilterGroupAndSortOrderDefinitions",
    { locale: DEFAULT_LOCALE },
    token,
    proxyUrl,
  );
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

export async function browseCatalogUncached(input: CatalogBrowseRequest): Promise<CatalogBrowseResult> {
  const token = input.token;
  if (!token) {
    throw new Error("Catalog browsing requires an authenticated token");
  }

  const vpcId = await getVpcId(token, input.providerStreamingBaseUrl, input.proxyUrl);
  const definitions = await fetchFilterAndSortDefinitions(token, input.proxyUrl);
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
            features {
${GFN_FEATURE_FIELDS}
            }
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
    const variables = searchQuery.length > 0
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
        };
    const payload = await fetchLcarsGraphQl<AppsSearchResponse>(
      searchQuery.length > 0 ? "AppsWithSearch" : "AppsWithoutSearch",
      variables,
      token,
      input.proxyUrl,
      {
        context: "GFN catalog query failed",
        fallbackQuery: query,
      },
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

  const games = dedupeGames(await enrichGamesWithMetadata(token, vpcId, collectedApps.map(appToGame), input.proxyUrl));
  const publicGames = await fetchPublicGames(input.proxyUrl);
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
  if (!shouldBypassGamesCache(input.proxyUrl)) {
    const cacheKey = catalogBrowseCacheKey(input, accountId);
    await cacheManager.saveToCache(cacheKey, result);
  }
  return result;
}

export async function peekCachedBrowseCatalog(input: CatalogBrowseRequest): Promise<CatalogBrowseResult | null> {
  const token = input.token;
  if (!token) {
    return null;
  }
  if (shouldBypassGamesCache(input.proxyUrl)) {
    return null;
  }

  const accountId = resolveAccountCacheId(input.userId, token);
  const cacheKey = catalogBrowseCacheKey(input, accountId);
  const cached = await cacheManager.loadFromCache<CatalogBrowseResult>(cacheKey);
  return cached?.data ?? null;
}

export async function fetchMainGames(
  token: string,
  providerStreamingBaseUrl?: string,
  accountId?: string,
  proxyUrl?: string,
): Promise<GameInfo[]> {
  const cached = await loadAccountScopedFromCache<GameInfo[]>("main", accountId, token, providerStreamingBaseUrl, proxyUrl);
  if (cached) {
    return mergePublicGameVariants(cached.data, await fetchPublicGames(proxyUrl));
  }

  const games = await fetchMainGamesUncached(token, providerStreamingBaseUrl, proxyUrl);
  if (!shouldBypassGamesCache(proxyUrl)) {
    const cacheKey = accountScopedGamesCacheKey("main", resolveAccountCacheId(accountId, token), providerStreamingBaseUrl, proxyUrl);
    await cacheManager.saveToCache(cacheKey, games);
  }
  return games;
}

export async function fetchFeaturedGames(
  token: string,
  providerStreamingBaseUrl?: string,
  accountId?: string,
  proxyUrl?: string,
): Promise<GameInfo[]> {
  const cached = await loadAccountScopedFromCache<GameInfo[]>("featured", accountId, token, providerStreamingBaseUrl, proxyUrl);
  if (cached) return cached.data;

  const vpcId = await getVpcId(token, providerStreamingBaseUrl, proxyUrl);
  const games = featuredGamesFromPanels(await fetchPanels(token, ["MARQUEE"], vpcId, undefined, proxyUrl)).slice(0, 6);

  if (!shouldBypassGamesCache(proxyUrl)) {
    const cacheKey = accountScopedGamesCacheKey("featured", resolveAccountCacheId(accountId, token), providerStreamingBaseUrl, proxyUrl);
    await cacheManager.saveToCache(cacheKey, games);
  }
  return games;
}

export async function fetchStorePanels(
  token: string,
  providerStreamingBaseUrl?: string,
  accountId?: string,
  proxyUrl?: string,
): Promise<GamePanelResult[]> {
  const cached = await loadAccountScopedFromCache<GamePanelResult[]>("store-panels", accountId, token, providerStreamingBaseUrl, proxyUrl);
  if (cached) return cached.data;

  const vpcId = await getVpcId(token, providerStreamingBaseUrl, proxyUrl);
  const panels = await enrichPanelArtwork(
    token,
    vpcId,
    parsePanelResults(await fetchPanels(token, ["MAIN"], vpcId, undefined, proxyUrl)),
    proxyUrl,
  );
  if (!shouldBypassGamesCache(proxyUrl)) {
    const cacheKey = accountScopedGamesCacheKey("store-panels", resolveAccountCacheId(accountId, token), providerStreamingBaseUrl, proxyUrl);
    await cacheManager.saveToCache(cacheKey, panels);
  }
  return panels;
}

/**
 * The panels query is a server-persisted document that only returns TV_BANNER
 * and HERO_IMAGE, so store rows arrive without box art. The library path solves
 * the same problem with `enrichGamesWithMetadata`, whose app-metadata query does
 * return the full image set; reuse it here rather than duplicating a fetch.
 *
 * Games are re-attached by identity because the enrichment dedupes, and the same
 * title legitimately appears in several sections.
 */
async function enrichPanelArtwork(
  token: string,
  vpcId: string,
  panels: GamePanelResult[],
  proxyUrl?: string,
): Promise<GamePanelResult[]> {
  const allGames = panels.flatMap((panel) => panel.sections).flatMap((section) => section.games);
  if (allGames.length === 0) return panels;

  let enrichedById: Map<string, GameInfo>;
  try {
    const enriched = await enrichGamesWithMetadata(token, vpcId, allGames, proxyUrl);
    enrichedById = new Map(enriched.map((game) => [game.uuid || game.id, game]));
  } catch (error) {
    // Artwork is a nicety; a metadata outage must not empty the storefront.
    console.warn("Store panel artwork enrichment failed, using panel artwork:", error);
    return panels;
  }

  return panels.map((panel) => ({
    ...panel,
    sections: panel.sections.map((section) => ({
      ...section,
      games: section.games.map((game) => enrichedById.get(game.uuid || game.id) ?? game),
    })),
  }));
}

export async function fetchMainGamesUncached(token: string, providerStreamingBaseUrl?: string, proxyUrl?: string): Promise<GameInfo[]> {
  const vpcId = await getVpcId(token, providerStreamingBaseUrl, proxyUrl);
  const payload = await fetchPanels(token, ["MAIN"], vpcId, undefined, proxyUrl);
  const games = flattenPanels(payload);
  return mergePublicGameVariants(await enrichGamesWithMetadata(token, vpcId, games, proxyUrl), await fetchPublicGames(proxyUrl));
}

/** Shared by library fallback path. */
export { fetchPanels };
