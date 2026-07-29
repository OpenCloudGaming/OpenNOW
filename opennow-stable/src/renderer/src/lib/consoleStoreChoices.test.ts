import test from "node:test";
import assert from "node:assert/strict";

import type { GameInfo } from "@shared/gfn";
import { getConsoleStoreChoices } from "./consoleStoreChoices";

function makeGame(variants: Array<Partial<GameInfo["variants"][number]>>, selectedVariantIndex = 0): GameInfo {
  return {
    id: "g",
    title: "Game",
    selectedVariantIndex,
    variants: variants.map((v, i) => ({ id: `v${i}`, store: "STEAM", ...v })),
  } as GameInfo;
}

test("returns one row per store with display names", () => {
  const game = makeGame([{ store: "STEAM" }, { store: "EPIC_GAMES_STORE" }, { store: "XBOX" }]);
  assert.deepEqual(getConsoleStoreChoices(game).map((c) => c.label), ["Epic", "Steam", "Xbox"]);
});

test("marks owned variants and sorts them first", () => {
  const game = makeGame([
    { id: "a", store: "EPIC_GAMES_STORE" },
    { id: "b", store: "XBOX", inLibrary: true },
  ]);
  const choices = getConsoleStoreChoices(game);
  assert.deepEqual(choices.map((c) => [c.label, c.isOwned]), [["Xbox", true], ["Epic", false]]);
});

test("marks exactly one active variant, following the selection", () => {
  const game = makeGame([{ id: "a", store: "STEAM" }, { id: "b", store: "XBOX" }]);
  const choices = getConsoleStoreChoices(game, "b");
  assert.deepEqual(choices.filter((c) => c.isActive).map((c) => c.variantId), ["b"]);
});

test("falls back to the game's default variant when none is selected", () => {
  const game = makeGame([{ id: "a", store: "STEAM" }, { id: "b", store: "XBOX" }], 1);
  assert.deepEqual(getConsoleStoreChoices(game).filter((c) => c.isActive).map((c) => c.variantId), ["b"]);
});

test("collapses duplicate stores, preferring the owned entry", () => {
  // The active variant is on a different store, so neither Steam entry is
  // active and ownership is the only tie-breaker.
  const game = makeGame([
    { id: "active", store: "XBOX" },
    { id: "a", store: "STEAM" },
    { id: "b", store: "STEAM", inLibrary: true },
  ]);
  const steam = getConsoleStoreChoices(game, "active").filter((c) => c.label === "Steam");
  assert.equal(steam.length, 1);
  assert.equal(steam[0].variantId, "b");
  assert.equal(steam[0].isOwned, true);
});

test("keeps the active entry even when a duplicate store is owned", () => {
  const game = makeGame([
    { id: "a", store: "STEAM" },
    { id: "b", store: "STEAM", inLibrary: true },
  ]);
  assert.equal(getConsoleStoreChoices(game, "a")[0].variantId, "a");
});

test("drops variants with no store", () => {
  const game = makeGame([{ id: "a", store: "" }, { id: "b", store: "  " }, { id: "c", store: "GOG" }]);
  assert.deepEqual(getConsoleStoreChoices(game).map((c) => c.variantId), ["c"]);
});

test("returns nothing for a game with no variants", () => {
  assert.deepEqual(getConsoleStoreChoices(makeGame([])), []);
});
