export const VARIANT_SELECTION_LOCALSTORAGE_KEY = "opennow.variantByGameId";
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
