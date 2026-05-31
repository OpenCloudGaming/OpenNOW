import type { GameInfo, GameVariant } from "@shared/gfn";
import { normalizeGameStore } from "@shared/gfn";
import { GFN_USER_AGENT } from "./clientHeaders";

export interface RawPublicGame {
  id?: string | number;
  title?: string;
  steamUrl?: string;
  store?: string;
  publisher?: string;
  status?: string;
}

const PRIMARY_CATALOG_STORE_KEYS = new Set([
  "STEAM",
  "EPIC",
  "EPIC_GAMES_STORE",
  "EGS",
  "XBOX",
  "XBOX_GAME_PASS",
  "MICROSOFT",
  "MICROSOFT_STORE",
]);

export function inferPublicGameStore(item: RawPublicGame): string {
  const explicitStore = item.store?.trim();
  if (explicitStore) {
    return explicitStore;
  }

  const publisher = item.publisher?.trim();
  if (publisher) {
    const publisherName = publisher.toLowerCase();
    if (publisherName.includes("ncsoft")) {
      return "NCSoft";
    }
  }

  return "Unknown";
}

function isNumericId(value: string | undefined): value is string {
  if (!value) {
    return false;
  }
  return /^\d+$/.test(value);
}

export function publicGameToGameInfo(item: RawPublicGame): GameInfo {
  const sourceId = String(item.id ?? item.title ?? "unknown");
  const steamAppId = item.steamUrl?.split("/app/")[1]?.split("/")[0];
  const id = steamAppId || sourceId;
  const imageUrl = steamAppId
    ? `https://cdn.cloudflare.steamstatic.com/steam/apps/${steamAppId}/header.jpg`
    : undefined;
  const heroImageUrl = steamAppId
    ? `https://cdn.cloudflare.steamstatic.com/steam/apps/${steamAppId}/library_hero.jpg`
    : undefined;
  const store = inferPublicGameStore(item);

  return {
    id,
    uuid: sourceId,
    launchAppId: isNumericId(id) ? id : undefined,
    title: item.title ?? id,
    searchText: [item.title ?? id, item.store, item.publisher]
      .filter((value): value is string => typeof value === "string" && value.trim().length > 0)
      .join(" ")
      .toLowerCase(),
    selectedVariantIndex: 0,
    variants: [{ id, store, supportedControls: [] }],
    imageUrl,
    heroImageUrl,
    availableStores: [store],
    isInLibrary: false,
  };
}

function normalizeTitleKey(title: string): string {
  return title
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, " ")
    .trim();
}

function mergeSearchText(left?: string, right?: string): string | undefined {
  const merged = [left, right]
    .filter((value): value is string => typeof value === "string" && value.trim().length > 0)
    .join(" ")
    .trim();
  return merged || undefined;
}

function getSupplementalPublicVariants(game: GameInfo, publicGame: GameInfo): GameVariant[] {
  const existingStores = new Set(game.variants.map((variant) => normalizeGameStore(variant.store)));

  return publicGame.variants.filter((variant) => {
    const storeKey = normalizeGameStore(variant.store);
    return !PRIMARY_CATALOG_STORE_KEYS.has(storeKey) && !existingStores.has(storeKey);
  });
}

export function mergePublicGameVariants(games: GameInfo[], publicGames: GameInfo[]): GameInfo[] {
  const publicGameByTitle = new Map<string, GameInfo>();
  for (const publicGame of publicGames) {
    const titleKey = normalizeTitleKey(publicGame.title);
    if (titleKey && !publicGameByTitle.has(titleKey)) {
      publicGameByTitle.set(titleKey, publicGame);
    }
  }

  return games.map((game) => {
    const publicGame = publicGameByTitle.get(normalizeTitleKey(game.title));
    if (!publicGame) {
      return game;
    }

    const gameWithPublicFallbacks: GameInfo = {
      ...game,
      imageUrl: game.imageUrl ?? publicGame.imageUrl,
      heroImageUrl: game.heroImageUrl ?? publicGame.heroImageUrl,
      searchText: mergeSearchText(game.searchText, publicGame.searchText),
    };
    const supplementalVariants = getSupplementalPublicVariants(game, publicGame);
    if (supplementalVariants.length === 0) {
      return gameWithPublicFallbacks;
    }

    return {
      ...gameWithPublicFallbacks,
      uuid: gameWithPublicFallbacks.uuid ?? publicGame.uuid,
      launchAppId: gameWithPublicFallbacks.launchAppId ?? publicGame.launchAppId,
      variants: [...game.variants, ...supplementalVariants],
      availableStores: [
        ...new Set([
          ...(game.availableStores ?? []),
          ...supplementalVariants.map((variant) => variant.store),
          ...(publicGame.availableStores ?? []),
        ].filter((value): value is string => typeof value === "string" && value.trim().length > 0)),
      ],
    };
  });
}

function matchesPublicGameSearch(game: GameInfo, searchQuery: string): boolean {
  const normalizedQuery = searchQuery.trim().toLowerCase();
  if (!normalizedQuery) {
    return false;
  }

  if (game.searchText?.includes(normalizedQuery)) {
    return true;
  }

  return [game.title, ...(game.availableStores ?? [])]
    .filter((value): value is string => typeof value === "string" && value.trim().length > 0)
    .some((value) => value.toLowerCase().includes(normalizedQuery));
}

export function appendPublicGameSearchMatches(
  games: GameInfo[],
  publicGames: GameInfo[],
  searchQuery: string,
): GameInfo[] {
  const normalizedQuery = searchQuery.trim();
  if (!normalizedQuery) {
    return games;
  }

  const existingIds = new Set(games.map((game) => game.id));
  const existingTitles = new Set(games.map((game) => normalizeTitleKey(game.title)).filter(Boolean));
  const matches = publicGames.filter((game) => {
    const titleKey = normalizeTitleKey(game.title);
    return !existingIds.has(game.id)
      && (!titleKey || !existingTitles.has(titleKey))
      && matchesPublicGameSearch(game, normalizedQuery);
  });

  if (matches.length === 0) {
    return games;
  }

  return [...games, ...matches];
}

export async function fetchPublicGamesUncached(): Promise<GameInfo[]> {
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
    .map(publicGameToGameInfo);
}
