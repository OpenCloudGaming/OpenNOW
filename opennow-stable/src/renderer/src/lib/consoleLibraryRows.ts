import type { GameInfo } from "@shared/gfn";
import { getStoreDisplayName } from "./gameCardStores";
import type { PlaytimeData } from "./gameCatalog";
import {
  gameHasLibraryActivity,
  gameMatchesStoreFilter,
  getControllerStoreFilterItems,
  type LibraryTranslation,
} from "./libraryFilters";

/** Rows past this count are dropped so the shelf stays navigable on a TV. */
const MAX_STORE_ROWS = 6;
const DEFAULT_MAX_ROW_LENGTH = 40;
const CONTINUE_ROW_LENGTH = 20;

export interface ConsoleLibraryRow {
  id: string;
  title: string;
  games: GameInfo[];
}

export interface BuildConsoleLibraryRowsInput {
  games: GameInfo[];
  playtimeData: PlaytimeData;
  /** Active store filter id from `getControllerStoreFilterItems`; "library" means all. */
  storeFilterId: string;
  t: LibraryTranslation;
  maxRowLength?: number;
}

function getLastPlayedTime(game: GameInfo, playtimeData: PlaytimeData): number {
  const candidates = [game.lastPlayed, playtimeData[game.id]?.lastPlayedAt];
  let latest = 0;
  for (const candidate of candidates) {
    if (!candidate) continue;
    const parsed = Date.parse(candidate);
    if (Number.isFinite(parsed) && parsed > latest) latest = parsed;
  }
  return latest;
}

function pushRow(rows: ConsoleLibraryRow[], row: ConsoleLibraryRow): void {
  if (row.games.length > 0) rows.push(row);
}

/**
 * Groups the console library into Netflix-style shelves.
 *
 * Rows are omitted when empty, so a user with no play history never sees a
 * blank "Continue playing" shelf. Store membership reuses the same predicates
 * that drive the Y-hold store filter, so the two can never disagree.
 */
export function buildConsoleLibraryRows({
  games,
  playtimeData,
  storeFilterId,
  t,
  maxRowLength = DEFAULT_MAX_ROW_LENGTH,
}: BuildConsoleLibraryRowsInput): ConsoleLibraryRow[] {
  if (games.length === 0) return [];

  const filtered = storeFilterId === "library"
    ? games
    : games.filter((game) => gameMatchesStoreFilter(game, storeFilterId));
  if (filtered.length === 0) return [];

  const rows: ConsoleLibraryRow[] = [];

  const continueGames = filtered
    .filter((game) => gameHasLibraryActivity(game, playtimeData))
    .map((game) => ({ game, playedAt: getLastPlayedTime(game, playtimeData) }))
    .sort((left, right) => right.playedAt - left.playedAt)
    .slice(0, CONTINUE_ROW_LENGTH)
    .map((entry) => entry.game);
  pushRow(rows, { id: "continue", title: t("library.rows.continuePlaying"), games: continueGames });

  // A single store is already selected — one shelf of everything in it reads
  // better than re-splitting it by the same dimension.
  if (storeFilterId === "library") {
    // Only worth showing when there is no play history to lead with; otherwise
    // it would just repeat the top of the library under a second heading.
    if (continueGames.length === 0) {
      pushRow(rows, {
        id: "recent",
        title: t("library.rows.recentlyAdded"),
        games: filtered.slice(0, CONTINUE_ROW_LENGTH),
      });
    }

    const storeRows = getControllerStoreFilterItems(filtered, "")
      .filter((item) => item.id !== "library")
      .map((item) => ({
        id: item.id,
        // The filter list carries the raw store key (EA_APP, BATTLENET); shelf
        // headings need the human name.
        title: getStoreDisplayName(item.title),
        games: filtered.filter((game) => gameMatchesStoreFilter(game, item.id)).slice(0, maxRowLength),
      }))
      .filter((row) => row.games.length > 0)
      .sort((left, right) => right.games.length - left.games.length)
      .slice(0, MAX_STORE_ROWS);
    for (const row of storeRows) pushRow(rows, row);
  }

  // Never truncated — this is the shelf a user falls back to when hunting for
  // something the curated rows did not surface.
  pushRow(rows, { id: "all", title: t("library.rows.allGames"), games: filtered });

  return rows;
}
