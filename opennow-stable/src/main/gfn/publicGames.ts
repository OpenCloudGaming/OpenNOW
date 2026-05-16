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

function splitPublicStoreKeys(store: string): string[] {
  return store
    .split(",")
    .map((part) => normalizeGameStore(part.trim()))
    .filter((part) => part.length > 0);
}

function isPrimaryCatalogStoreValue(store: string): boolean {
  const storeKeys = splitPublicStoreKeys(store);
  return storeKeys.length > 0 && storeKeys.every((storeKey) => PRIMARY_CATALOG_STORE_KEYS.has(storeKey));
}

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
  const id = String(item.id ?? item.title ?? "unknown");
  const steamAppId = item.steamUrl?.split("/app/")[1]?.split("/")[0];
  const imageUrl = steamAppId
    ? `https://cdn.cloudflare.steamstatic.com/steam/apps/${steamAppId}/library_600x900.jpg`
    : undefined;
  const store = inferPublicGameStore(item);

  return {
    id,
    uuid: id,
    launchAppId: isNumericId(id) ? id : undefined,
    title: item.title ?? id,
    searchText: [item.title ?? id, item.store, item.publisher]
      .filter((value): value is string => typeof value === "string" && value.trim().length > 0)
      .join(" ")
      .toLowerCase(),
    selectedVariantIndex: 0,
    variants: [{ id, store, supportedControls: [] }],
    imageUrl,
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

export function hasSamePublicGameTitle(left: GameInfo, right: GameInfo): boolean {
  const leftKey = normalizeTitleKey(left.title);
  return leftKey.length > 0 && leftKey === normalizeTitleKey(right.title);
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
    return !isPrimaryCatalogStoreValue(variant.store) && !existingStores.has(storeKey);
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

    const supplementalVariants = getSupplementalPublicVariants(game, publicGame);
    if (supplementalVariants.length === 0) {
      return game;
    }

    return {
      ...game,
      uuid: game.uuid ?? publicGame.uuid,
      launchAppId: game.launchAppId ?? publicGame.launchAppId,
      imageUrl: game.imageUrl ?? publicGame.imageUrl,
      variants: [...game.variants, ...supplementalVariants],
      availableStores: [
        ...new Set([
          ...(game.availableStores ?? []),
          ...supplementalVariants.map((variant) => variant.store),
          ...(publicGame.availableStores ?? []),
        ].filter((value): value is string => typeof value === "string" && value.trim().length > 0)),
      ],
      searchText: mergeSearchText(game.searchText, publicGame.searchText),
    };
  });
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
