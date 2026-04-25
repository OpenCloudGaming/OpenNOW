/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import type { GameInfo, GameVariant } from "@shared/gfn";
import { chooseAccountLinked, getEpicOwnershipLaunchError } from "./launchOwnership";

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

function makeGame(variants: GameVariant[], overrides: Partial<GameInfo> = {}): GameInfo {
  return {
    id: overrides.id ?? "game-1",
    title: overrides.title ?? "Test Game",
    selectedVariantIndex: overrides.selectedVariantIndex ?? 0,
    variants,
    playType: overrides.playType,
    isInLibrary: overrides.isInLibrary,
  };
}

test("returns an Epic ownership launch error for unowned Epic variants only", () => {
  assert.deepEqual(
    getEpicOwnershipLaunchError(makeVariant({ store: "EPIC_GAMES_STORE", libraryStatus: "NOT_OWNED" })),
    {
      title: "Epic Games Library Required",
      description: "This Epic Games version is not in your GeForce NOW library. Add it to Epic, sync GeForce NOW, or choose another owned store.",
    },
  );

  assert.equal(
    getEpicOwnershipLaunchError(makeVariant({ store: "Epic", libraryStatus: "MANUAL" })),
    null,
  );
  assert.equal(
    getEpicOwnershipLaunchError(makeVariant({ store: "Steam", libraryStatus: "NOT_OWNED" })),
    null,
  );
});

test("uses owned status instead of librarySelected when deciding account-linked launches", () => {
  assert.equal(
    chooseAccountLinked(
      makeGame([makeVariant({ store: "Steam", librarySelected: true })], { isInLibrary: false }),
      makeVariant({ store: "Steam", librarySelected: true }),
    ),
    false,
  );

  assert.equal(
    chooseAccountLinked(
      makeGame([makeVariant({ store: "Steam", libraryStatus: "PLATFORM_SYNC" })], { isInLibrary: true }),
      makeVariant({ store: "Steam", libraryStatus: "PLATFORM_SYNC" }),
    ),
    true,
  );
});

test("never marks install-to-play titles as account linked", () => {
  assert.equal(
    chooseAccountLinked(
      makeGame(
        [makeVariant({ store: "Steam", libraryStatus: "IN_LIBRARY" })],
        { playType: "INSTALL_TO_PLAY", isInLibrary: true },
      ),
      makeVariant({ store: "Steam", libraryStatus: "IN_LIBRARY" }),
    ),
    false,
  );
});
