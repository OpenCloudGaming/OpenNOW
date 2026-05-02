import type { GameInfo } from "@shared/gfn";

export const CATALOG_PREFERENCES_LOCALSTORAGE_KEY = "opennow.catalogPreferences.v1";

export interface CatalogPreferences {
  sortId: string;
  filterIds: string[];
}

export function loadCatalogPreferences(): CatalogPreferences {
  try {
    const raw = localStorage.getItem(CATALOG_PREFERENCES_LOCALSTORAGE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<CatalogPreferences>;
      return {
        sortId: typeof parsed.sortId === "string" ? parsed.sortId : "relevance",
        filterIds: Array.isArray(parsed.filterIds) ? parsed.filterIds.filter((id): id is string => typeof id === "string") : [],
      };
    }
  } catch {
    // ignore
  }
  return { sortId: "relevance", filterIds: [] };
}

export function saveCatalogPreferences(prefs: CatalogPreferences): void {
  try {
    localStorage.setItem(CATALOG_PREFERENCES_LOCALSTORAGE_KEY, JSON.stringify(prefs));
  } catch {
    // ignore
  }
}

/** Sort library rows by user-selected order id. */
export function sortLibraryGames(games: GameInfo[], sortId: string): GameInfo[] {
  const copy = [...games];
  const compareTitle = (left: GameInfo, right: GameInfo) => left.title.localeCompare(right.title);
  if (sortId === "z_to_a") {
    return copy.sort((left, right) => right.title.localeCompare(left.title));
  }
  if (sortId === "a_to_z") {
    return copy.sort(compareTitle);
  }
  if (sortId === "last_played") {
    return copy.sort((left, right) => {
      const leftTime = left.lastPlayed ? new Date(left.lastPlayed).getTime() : 0;
      const rightTime = right.lastPlayed ? new Date(right.lastPlayed).getTime() : 0;
      if (leftTime === rightTime) return compareTitle(left, right);
      return rightTime - leftTime;
    });
  }
  if (sortId === "last_added") {
    return copy.sort((left, right) => {
      const leftTime = left.isInLibrary ? new Date(left.lastPlayed ?? 0).getTime() : 0;
      const rightTime = right.isInLibrary ? new Date(right.lastPlayed ?? 0).getTime() : 0;
      if (leftTime === rightTime) return compareTitle(left, right);
      return rightTime - leftTime;
    });
  }
  if (sortId === "most_popular") {
    return copy.sort(
      (left, right) =>
        (right.membershipTierLabel ? 1 : 0) - (left.membershipTierLabel ? 1 : 0) || compareTitle(left, right),
    );
  }
  return copy.sort(compareTitle);
}
