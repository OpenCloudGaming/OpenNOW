/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import type { GameInfo, GameVariant } from "@shared/gfn";
import { getStoreOptions } from "../lib/gameCardStores";

function makeVariant(overrides: Partial<GameVariant> = {}): GameVariant {
  return {
    id: overrides.id ?? "variant-1",
    store: overrides.store ?? "Steam",
    supportedControls: overrides.supportedControls ?? [],
    librarySelected: overrides.librarySelected,
    libraryStatus: overrides.libraryStatus,
    lastPlayedDate: overrides.lastPlayedDate,
    gfnStatus: overrides.gfnStatus,
  };
}

function makeGame(variants: GameVariant[]): GameInfo {
  return {
    id: "game-1",
    title: "Test Game",
    selectedVariantIndex: 0,
    variants,
  };
}

test("marks owned and unowned store chips independently", () => {
  const options = getStoreOptions(
    makeGame([
      makeVariant({ id: "steam", store: "Steam", libraryStatus: "MANUAL" }),
      makeVariant({ id: "epic", store: "Epic Games Store", libraryStatus: "NOT_OWNED" }),
    ]),
  );

  assert.deepEqual(
    options.map((option) => ({
      storeKey: option.storeKey,
      isOwned: option.isOwned,
      isActive: option.isActive,
    })),
    [
      { storeKey: "STEAM", isOwned: true, isActive: true },
      { storeKey: "EPIC_GAMES_STORE", isOwned: false, isActive: false },
    ],
  );
});

test("collapses multiple variants from the same store into a single chip and prefers an owned variant", () => {
  const options = getStoreOptions(
    makeGame([
      makeVariant({ id: "steam-unowned", store: "Steam", libraryStatus: "NOT_OWNED" }),
      makeVariant({ id: "steam-owned", store: "Steam", libraryStatus: "PLATFORM_SYNC" }),
      makeVariant({ id: "epic", store: "Epic", libraryStatus: "NOT_OWNED" }),
    ]),
    "epic",
  );

  assert.equal(options.length, 2);

  const steamOption = options.find((option) => option.storeKey === "STEAM");
  assert.ok(steamOption);
  assert.equal(steamOption.variantId, "steam-owned");
  assert.equal(steamOption.isOwned, true);
  assert.equal(steamOption.isActive, false);
});

test("keeps active and owned states separate for the selected store", () => {
  const options = getStoreOptions(
    makeGame([
      makeVariant({ id: "epic", store: "EGS", libraryStatus: "NOT_OWNED" }),
      makeVariant({ id: "steam", store: "Steam", libraryStatus: "IN_LIBRARY" }),
    ]),
    "epic",
  );

  const epicOption = options.find((option) => option.storeKey === "EGS");
  const steamOption = options.find((option) => option.storeKey === "STEAM");

  assert.ok(epicOption);
  assert.ok(steamOption);
  assert.equal(epicOption.isActive, true);
  assert.equal(epicOption.isOwned, false);
  assert.equal(steamOption.isActive, false);
  assert.equal(steamOption.isOwned, true);
});
