/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import {
  appendPublicGameSearchMatches,
  inferPublicGameStore,
  mergePublicGameVariants,
  publicGameToGameInfo,
} from "./publicGames";

test("infers NCSoft as the public catalog store for Guild Wars 2", () => {
  assert.equal(
    inferPublicGameStore({
      id: 17940711,
      title: "Guild Wars 2",
      steamUrl: "",
      publisher: "NCsoft Corp.",
      store: "",
      status: "AVAILABLE",
    }),
    "NCSoft",
  );
});

test("uses explicit public catalog stores before publisher fallback", () => {
  assert.equal(
    inferPublicGameStore({
      title: "Steam Game",
      store: "Steam",
      publisher: "NCsoft Corp.",
      status: "AVAILABLE",
    }),
    "Steam",
  );
});

test("uses Unknown for blank public catalog stores without a known launcher publisher", () => {
  assert.equal(inferPublicGameStore({ title: "Unlabeled Game", status: "AVAILABLE" }), "Unknown");
  assert.equal(
    inferPublicGameStore({ title: "Publisher Only Game", publisher: "Some Publisher", status: "AVAILABLE" }),
    "Unknown",
  );
});

test("maps Guild Wars 2 public catalog data to an NCSoft default-icon variant", () => {
  const game = publicGameToGameInfo({
    id: 17940711,
    title: "Guild Wars 2",
    steamUrl: "",
    publisher: "NCsoft Corp.",
    store: "",
    status: "AVAILABLE",
  });

  assert.equal(game.launchAppId, "17940711");
  assert.deepEqual(game.variants, [{ id: "17940711", store: "NCSoft", supportedControls: [] }]);
  assert.deepEqual(game.availableStores, ["NCSoft"]);
  assert.equal(game.searchText, "guild wars 2 ncsoft corp.");
});

test("merges supplemental public launcher variants into catalog games by title", () => {
  const [game] = mergePublicGameVariants(
    [
      {
        id: "guild-wars-2",
        title: "Guild Wars 2",
        selectedVariantIndex: 0,
        variants: [{ id: "steam", store: "Steam", supportedControls: [] }],
        availableStores: ["Steam"],
        searchText: "guild wars 2 steam",
      },
    ],
    [
      publicGameToGameInfo({
        id: 17940711,
        title: "Guild Wars 2",
        steamUrl: "",
        publisher: "NCsoft Corp.",
        store: "",
        status: "AVAILABLE",
      }),
    ],
  );

  assert.deepEqual(
    game?.variants.map((variant) => ({ id: variant.id, store: variant.store })),
    [
      { id: "steam", store: "Steam" },
      { id: "17940711", store: "NCSoft" },
    ],
  );
  assert.deepEqual(game?.availableStores, ["Steam", "NCSoft"]);
});

test("preserves public Steam hero fallback when merging supplemental variants", () => {
  const [game] = mergePublicGameVariants(
    [
      {
        id: "third-party-steam-game",
        title: "Third Party Steam Game",
        selectedVariantIndex: 0,
        variants: [{ id: "third-party", store: "GOG", supportedControls: [] }],
        availableStores: ["GOG"],
      },
    ],
    [
      publicGameToGameInfo({
        id: 456,
        title: "Third Party Steam Game",
        steamUrl: "https://store.steampowered.com/app/456",
        store: "Steam",
        status: "AVAILABLE",
      }),
    ],
  );

  assert.equal(game?.heroImageUrl, "https://cdn.cloudflare.steamstatic.com/steam/apps/456/library_hero.jpg");
});

test("merges same-id public launcher variants when the store is missing from catalog data", () => {
  const [game] = mergePublicGameVariants(
    [
      {
        id: "guild-wars-2",
        title: "Guild Wars 2",
        selectedVariantIndex: 0,
        variants: [{ id: "17940711", store: "Steam", supportedControls: [] }],
        availableStores: ["Steam"],
      },
    ],
    [
      publicGameToGameInfo({
        id: 17940711,
        title: "Guild Wars 2",
        steamUrl: "",
        publisher: "NCsoft Corp.",
        store: "",
        status: "AVAILABLE",
      }),
    ],
  );

  assert.deepEqual(
    game?.variants.map((variant) => ({ id: variant.id, store: variant.store })),
    [
      { id: "17940711", store: "Steam" },
      { id: "17940711", store: "NCSoft" },
    ],
  );
});

test("does not duplicate primary catalog store variants from public data", () => {
  const [game] = mergePublicGameVariants(
    [
      {
        id: "steam-game",
        title: "Steam Game",
        selectedVariantIndex: 0,
        variants: [{ id: "steam", store: "Steam", supportedControls: [] }],
        availableStores: ["Steam"],
      },
    ],
    [
      publicGameToGameInfo({
        id: 123,
        title: "Steam Game",
        steamUrl: "https://store.steampowered.com/app/123",
        store: "Steam",
        status: "AVAILABLE",
      }),
    ],
  );

  assert.deepEqual(game?.variants.map((variant) => variant.store), ["Steam"]);
});

test("appends public-only games that match catalog search", () => {
  const games = appendPublicGameSearchMatches(
    [
      {
        id: "catalog-game",
        title: "Catalog Game",
        selectedVariantIndex: 0,
        variants: [{ id: "catalog-game", store: "Steam", supportedControls: [] }],
      },
    ],
    [
      publicGameToGameInfo({
        id: 17940711,
        title: "Guild Wars 2",
        steamUrl: "",
        publisher: "NCsoft Corp.",
        store: "",
        status: "AVAILABLE",
      }),
      publicGameToGameInfo({
        id: 123,
        title: "Unrelated Game",
        steamUrl: "",
        store: "Unknown",
        status: "AVAILABLE",
      }),
    ],
    "guild wars",
  );

  assert.deepEqual(games.map((game) => game.title), ["Catalog Game", "Guild Wars 2"]);
});

test("does not append public search matches already represented by catalog results", () => {
  const games = appendPublicGameSearchMatches(
    [
      {
        id: "guild-wars-2-catalog",
        title: "Guild Wars 2",
        selectedVariantIndex: 0,
        variants: [{ id: "steam", store: "Steam", supportedControls: [] }],
      },
    ],
    [
      publicGameToGameInfo({
        id: 17940711,
        title: "Guild Wars 2",
        steamUrl: "",
        publisher: "NCsoft Corp.",
        store: "",
        status: "AVAILABLE",
      }),
    ],
    "guild wars",
  );

  assert.deepEqual(games.map((game) => game.title), ["Guild Wars 2"]);
});
