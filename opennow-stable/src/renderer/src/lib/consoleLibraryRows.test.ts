import test from "node:test";
import assert from "node:assert/strict";

import type { GameInfo } from "@shared/gfn";
import { buildConsoleLibraryRows } from "./consoleLibraryRows";
import type { PlaytimeData } from "./gameCatalog";

const t = (key: string): string => key;

function makeGame(id: string, store: string, overrides: Partial<GameInfo> = {}): GameInfo {
  return {
    id,
    title: id,
    variants: [{ id: `${id}-v`, store, inLibrary: true } as GameInfo["variants"][number]],
    availableStores: [store],
    ...overrides,
  } as GameInfo;
}

const STEAM_A = makeGame("steam-a", "STEAM", { lastPlayed: "2026-07-01T00:00:00Z" });
const STEAM_B = makeGame("steam-b", "STEAM");
const STEAM_C = makeGame("steam-c", "STEAM");
const EPIC_A = makeGame("epic-a", "EPIC", { lastPlayed: "2026-07-20T00:00:00Z" });
const GAMES = [STEAM_A, STEAM_B, STEAM_C, EPIC_A];

const NO_PLAYTIME: PlaytimeData = {};

test("continue playing holds only games with activity, most recent first", () => {
  const rows = buildConsoleLibraryRows({ games: GAMES, playtimeData: NO_PLAYTIME, storeFilterId: "library", t });
  const continueRow = rows.find((row) => row.id === "continue");
  assert.ok(continueRow);
  assert.deepEqual(continueRow.games.map((game) => game.id), ["epic-a", "steam-a"]);
});

test("playtime data counts as activity even without lastPlayed", () => {
  const playtimeData: PlaytimeData = { "steam-b": { lastPlayedAt: "2026-07-25T00:00:00Z" } };
  const rows = buildConsoleLibraryRows({ games: GAMES, playtimeData, storeFilterId: "library", t });
  const continueRow = rows.find((row) => row.id === "continue");
  assert.deepEqual(continueRow?.games.map((game) => game.id), ["steam-b", "epic-a", "steam-a"]);
});

test("omits continue playing and falls back to recently added when nothing has been played", () => {
  const games = [STEAM_B, STEAM_C];
  const rows = buildConsoleLibraryRows({ games, playtimeData: NO_PLAYTIME, storeFilterId: "library", t });
  assert.equal(rows.some((row) => row.id === "continue"), false);
  assert.equal(rows.some((row) => row.id === "recent"), true);
});

test("does not show recently added when continue playing already leads", () => {
  const rows = buildConsoleLibraryRows({ games: GAMES, playtimeData: NO_PLAYTIME, storeFilterId: "library", t });
  assert.equal(rows.some((row) => row.id === "recent"), false);
});

test("partitions games into one row per store", () => {
  const rows = buildConsoleLibraryRows({ games: GAMES, playtimeData: NO_PLAYTIME, storeFilterId: "library", t });
  const steamRow = rows.find((row) => row.id === "store:STEAM");
  const epicRow = rows.find((row) => row.id === "store:EPIC");
  assert.deepEqual(steamRow?.games.map((game) => game.id), ["steam-a", "steam-b", "steam-c"]);
  assert.deepEqual(epicRow?.games.map((game) => game.id), ["epic-a"]);
});

test("orders store rows by size, largest first", () => {
  const rows = buildConsoleLibraryRows({ games: GAMES, playtimeData: NO_PLAYTIME, storeFilterId: "library", t });
  const storeRowIds = rows.filter((row) => row.id.startsWith("store:")).map((row) => row.id);
  assert.deepEqual(storeRowIds, ["store:STEAM", "store:EPIC"]);
});

test("every game appears in the all-games row, untruncated", () => {
  const many = Array.from({ length: 120 }, (_, index) => makeGame(`game-${index}`, "STEAM"));
  const rows = buildConsoleLibraryRows({ games: many, playtimeData: NO_PLAYTIME, storeFilterId: "library", t });
  const allRow = rows.find((row) => row.id === "all");
  assert.equal(allRow?.games.length, 120);
});

test("a store filter collapses the shelf set to continue plus one row", () => {
  const rows = buildConsoleLibraryRows({ games: GAMES, playtimeData: NO_PLAYTIME, storeFilterId: "store:STEAM", t });
  assert.deepEqual(rows.map((row) => row.id), ["continue", "all"]);
  assert.deepEqual(rows[1].games.map((game) => game.id), ["steam-a", "steam-b", "steam-c"]);
  assert.deepEqual(rows[0].games.map((game) => game.id), ["steam-a"]);
});

test("never emits duplicate row ids", () => {
  const rows = buildConsoleLibraryRows({ games: GAMES, playtimeData: NO_PLAYTIME, storeFilterId: "library", t });
  assert.equal(new Set(rows.map((row) => row.id)).size, rows.length);
});

test("never emits an empty row", () => {
  const rows = buildConsoleLibraryRows({ games: GAMES, playtimeData: NO_PLAYTIME, storeFilterId: "library", t });
  assert.equal(rows.every((row) => row.games.length > 0), true);
});

test("returns no rows for an empty library or a filter that matches nothing", () => {
  assert.deepEqual(buildConsoleLibraryRows({ games: [], playtimeData: NO_PLAYTIME, storeFilterId: "library", t }), []);
  assert.deepEqual(
    buildConsoleLibraryRows({ games: GAMES, playtimeData: NO_PLAYTIME, storeFilterId: "store:GOG", t }),
    [],
  );
});
