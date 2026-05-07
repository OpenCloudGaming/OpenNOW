import type { GameInfo, GameVariant } from "@shared/gfn";

export type PlaytimeData = Record<string, { lastPlayedAt?: string | null; totalSeconds?: number; sessionCount?: number }>;

export function isNumericId(value: string | undefined): value is string {
  if (!value) return false;
  return /^\d+$/.test(value);
}

export function parseNumericId(value: string | undefined): number | null {
  if (!isNumericId(value)) return null;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}

export function defaultVariantId(game: GameInfo): string {
  return game.variants[game.selectedVariantIndex]?.id ?? game.variants[0]?.id ?? game.id;
}

export function getSelectedVariant(game: GameInfo, variantId: string): GameVariant | undefined {
  return game.variants.find((variant) => variant.id === variantId) ?? game.variants[0];
}

export function findSessionContextForAppId(
  catalog: GameInfo[],
  variantByGameId: Record<string, string>,
  appId: number,
): { game: GameInfo; variant?: GameVariant } | null {
  for (const game of catalog) {
    const matchedVariant = game.variants.find((variant) => parseNumericId(variant.id) === appId);
    if (matchedVariant) {
      return { game, variant: matchedVariant };
    }

    if (parseNumericId(game.launchAppId) === appId) {
      const preferredVariantId = variantByGameId[game.id] ?? defaultVariantId(game);
      return {
        game,
        variant: getSelectedVariant(game, preferredVariantId),
      };
    }
  }

  return null;
}

export function matchesGameSearch(game: GameInfo, query: string): boolean {
  const normalizedQuery = query.trim().toLowerCase();
  if (!normalizedQuery) return true;
  if (game.searchText?.includes(normalizedQuery)) return true;
  return [
    game.title,
    game.description,
    game.publisherName,
    ...(game.genres ?? []),
    ...(game.featureLabels ?? []),
    ...(game.availableStores ?? []),
  ]
    .filter((value): value is string => typeof value === "string" && value.length > 0)
    .some((value) => value.toLowerCase().includes(normalizedQuery));
}

export function areStringArraysEqual(left: string[], right: string[]): boolean {
  if (left.length != right.length) {
    return false;
  }
  return left.every((value, index) => value === right[index]);
}

export function sortLibraryGames(
  games: GameInfo[],
  sortId: string,
  playtimeData: PlaytimeData,
): GameInfo[] {
  const copy = [...games];
  const compareTitle = (left: GameInfo, right: GameInfo) => left.title.localeCompare(right.title);
  const playtimeLastPlayedMs = (gameId: string): number => {
    const raw = playtimeData[gameId]?.lastPlayedAt;
    if (!raw) return 0;
    const ms = Date.parse(raw);
    return Number.isFinite(ms) ? ms : 0;
  };
  const legacyLastPlayedMs = (game: GameInfo): number => {
    if (!game.lastPlayed) return 0;
    const ms = Date.parse(game.lastPlayed);
    return Number.isFinite(ms) ? ms : 0;
  };
  if (sortId === "z_to_a") {
    return copy.sort((left, right) => right.title.localeCompare(left.title));
  }
  if (sortId === "a_to_z") {
    return copy.sort(compareTitle);
  }
  if (sortId === "last_played") {
    return copy.sort((left, right) => {
      const leftTime = playtimeLastPlayedMs(left.id) || legacyLastPlayedMs(left);
      const rightTime = playtimeLastPlayedMs(right.id) || legacyLastPlayedMs(right);
      if (leftTime === rightTime) return compareTitle(left, right);
      return rightTime - leftTime;
    });
  }
  if (sortId === "last_added") {
    // Preserve server-provided order. We do not currently have a trustworthy local "addedAt" field.
    return copy;
  }
  if (sortId === "most_popular") {
    return copy.sort((left, right) => {
      const leftSeconds = Math.max(0, playtimeData[left.id]?.totalSeconds ?? 0);
      const rightSeconds = Math.max(0, playtimeData[right.id]?.totalSeconds ?? 0);
      if (leftSeconds !== rightSeconds) return rightSeconds - leftSeconds;
      const leftSessions = Math.max(0, playtimeData[left.id]?.sessionCount ?? 0);
      const rightSessions = Math.max(0, playtimeData[right.id]?.sessionCount ?? 0);
      if (leftSessions !== rightSessions) return rightSessions - leftSessions;
      return compareTitle(left, right);
    });
  }
  return copy.sort(compareTitle);
}

export function mergeVariantSelections(
  current: Record<string, string>,
  catalog: GameInfo[],
): Record<string, string> {
  if (catalog.length === 0) {
    return current;
  }

  const next = { ...current };
  for (const game of catalog) {
    const selectedVariantId = next[game.id];
    const hasSelectedVariant = !!selectedVariantId && game.variants.some((variant) => variant.id === selectedVariantId);
    if (!hasSelectedVariant) {
      next[game.id] = defaultVariantId(game);
    }
  }
  return next;
}
