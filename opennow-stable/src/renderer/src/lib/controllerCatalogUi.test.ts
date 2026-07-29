import test from "node:test";
import assert from "node:assert/strict";

import type { GameInfo } from "@shared/gfn";
import {
  countConsolePortraitPosterCandidates,
  getConsolePosterCandidates,
  getGameLogoUrl,
  getSteamLibraryPosterUrl,
} from "./controllerCatalogUi";

function makeGame(overrides: Partial<GameInfo> = {}): GameInfo {
  return {
    id: "game",
    title: "Game",
    variants: [],
    ...overrides,
  } as GameInfo;
}

const STEAM_VARIANT = { id: "440", store: "STEAM" } as GameInfo["variants"][number];

test("prefers GAME_BOX_ART above every other poster source", () => {
  const game = makeGame({
    variants: [STEAM_VARIANT],
    imageUrlsByType: { GAME_BOX_ART: ["box.jpg"], TV_BANNER: ["banner.jpg"] },
  });
  assert.equal(getConsolePosterCandidates(game)[0], "box.jpg");
});

test("falls back to Steam library poster art before any landscape key", () => {
  const game = makeGame({
    variants: [STEAM_VARIANT],
    imageUrlsByType: { TV_BANNER: ["banner.jpg"] },
  });
  const candidates = getConsolePosterCandidates(game);
  assert.equal(candidates[0], "https://cdn.cloudflare.steamstatic.com/steam/apps/440/library_600x900.jpg");
  assert.equal(candidates[1], "banner.jpg");
});

test("uses landscape art when no portrait source exists", () => {
  const game = makeGame({ imageUrlsByType: { TV_BANNER: ["banner.jpg"], KEY_ART: ["key.jpg"] } });
  assert.deepEqual(getConsolePosterCandidates(game), ["banner.jpg", "key.jpg"]);
  assert.equal(countConsolePortraitPosterCandidates(game), 0);
});

test("counts leading portrait candidates so cards know when they are cropping", () => {
  const withBoxArt = makeGame({ variants: [STEAM_VARIANT], imageUrlsByType: { GAME_BOX_ART: ["box.jpg"] } });
  assert.equal(countConsolePortraitPosterCandidates(withBoxArt), 2);

  const steamOnly = makeGame({ variants: [STEAM_VARIANT] });
  assert.equal(countConsolePortraitPosterCandidates(steamOnly), 1);
});

test("never repeats a candidate", () => {
  const game = makeGame({
    imageUrl: "shared.jpg",
    heroImageUrl: "shared.jpg",
    imageUrlsByType: { TV_BANNER: ["shared.jpg"], KEY_ART: ["shared.jpg"] },
  });
  assert.deepEqual(getConsolePosterCandidates(game), ["shared.jpg"]);
});

test("returns no candidates for a game with no artwork", () => {
  assert.deepEqual(getConsolePosterCandidates(makeGame()), []);
});

test("derives the Steam poster from launchAppId when no Steam variant is listed", () => {
  const game = makeGame({ launchAppId: "1091500" });
  assert.equal(
    getSteamLibraryPosterUrl(game),
    "https://cdn.cloudflare.steamstatic.com/steam/apps/1091500/library_600x900.jpg",
  );
});

test("ignores non-numeric ids when deriving Steam art", () => {
  assert.equal(getSteamLibraryPosterUrl(makeGame({ launchAppId: "not-an-id" })), undefined);
  assert.equal(getSteamLibraryPosterUrl(makeGame()), undefined);
});

test("resolves a game logo across the known logo keys", () => {
  assert.equal(getGameLogoUrl(makeGame({ imageUrlsByType: { GAME_LOGO: ["a.png"] } })), "a.png");
  assert.equal(getGameLogoUrl(makeGame({ imageUrlsByType: { TITLE_LOGO: ["c.png"] } })), "c.png");
  assert.equal(getGameLogoUrl(makeGame()), undefined);
});
