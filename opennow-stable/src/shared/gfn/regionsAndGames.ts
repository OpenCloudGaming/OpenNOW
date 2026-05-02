export interface RegionsFetchRequest {
  token?: string;
}

export interface StreamRegion {
  name: string;
  url: string;
  pingMs?: number;
}

export interface PingResult {
  url: string;
  pingMs: number | null;
  error?: string;
}

export interface GamesFetchRequest {
  token?: string;
  providerStreamingBaseUrl?: string;
}

export interface CatalogBrowseRequest extends GamesFetchRequest {
  searchQuery?: string;
  sortId?: string;
  filterIds?: string[];
  fetchCount?: number;
}

export interface ResolveLaunchIdRequest {
  token?: string;
  providerStreamingBaseUrl?: string;
  appIdOrUuid: string;
}

export interface SubscriptionFetchRequest {
  token?: string;
  providerStreamingBaseUrl?: string;
  userId: string;
}

export interface GameVariant {
  id: string;
  store: string;
  supportedControls: string[];
  librarySelected?: boolean;
  libraryStatus?: string;
  lastPlayedDate?: string;
  gfnStatus?: string;
}

export const OWNED_LIBRARY_STATUSES = ["MANUAL", "PLATFORM_SYNC", "IN_LIBRARY"] as const;

export function normalizeGameStore(store: string): string {
  return store.toUpperCase().replace(/[\s-]+/g, "_");
}

export function isOwnedLibraryStatus(status?: string): boolean {
  return typeof status === "string" && OWNED_LIBRARY_STATUSES.includes(status as (typeof OWNED_LIBRARY_STATUSES)[number]);
}

export function isOwnedVariant(variant: Pick<GameVariant, "libraryStatus">): boolean {
  return isOwnedLibraryStatus(variant.libraryStatus);
}

export interface GameInfo {

  id: string;
  uuid?: string;
  launchAppId?: string;
  title: string;
  description?: string;
  longDescription?: string;
  featureLabels?: string[];
  genres?: string[];
  imageUrl?: string;
  screenshotUrl?: string;
  playType?: string;
  membershipTierLabel?: string;
  publisherName?: string;
  contentRatings?: string[];
  playabilityState?: string;
  availableStores?: string[];
  searchText?: string;
  lastPlayed?: string;
  isInLibrary?: boolean;
  selectedVariantIndex: number;
  variants: GameVariant[];
}

export function isGameInLibrary(game: Pick<GameInfo, "variants">): boolean {
  return game.variants.some((variant) => isOwnedVariant(variant));
}

export function isEpicStore(store: string): boolean {
  const key = normalizeGameStore(store);
  return key === "EPIC_GAMES_STORE" || key === "EPIC" || key === "EGS";
}

export interface CatalogFilterOption {
  id: string;
  rawId: string;
  label: string;
  groupId: string;
  groupLabel: string;
}

export interface CatalogFilterGroup {
  id: string;
  label: string;
  options: CatalogFilterOption[];
}

export interface CatalogSortOption {
  id: string;
  label: string;
  orderBy: string;
}

export interface CatalogBrowseResult {
  games: GameInfo[];
  numberReturned: number;
  numberSupported: number;
  totalCount: number;
  hasNextPage: boolean;
  endCursor?: string;
  searchQuery: string;
  selectedSortId: string;
  selectedFilterIds: string[];
  filterGroups: CatalogFilterGroup[];
  sortOptions: CatalogSortOption[];
}
