/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import {
  routeCancel,
  routeCategoryActivate,
  routeOpenOptions,
  routeOptionsActivate,
  routeSecondaryActivate,
} from "./actionRouter";

const game = {
  id: "game-1",
  title: "Test Game",
  selectedVariantIndex: 0,
  variants: [
    { id: "steam", store: "Steam", supportedControls: [] },
    { id: "epic", store: "Epic", supportedControls: [] },
  ],
};

function calls() {
  const values: string[] = [];
  return {
    values,
    fn: (name: string) => (...args: unknown[]) => values.push(`${name}:${args.map(String).join(",")}`),
  };
}

function baseOptionsContext(overrides: Record<string, unknown> = {}) {
  return {
    gamesShelfBrowseActive: true,
    mediaShelfBrowseActive: false,
    topCategory: "all",
    gameSubcategory: "all",
    gamesRootPlane: "categories",
    spotlightEntries: [],
    spotlightIndex: 0,
    selectedMediaIndex: 0,
    mediaAssetItems: [],
    selectedGame: game,
    gamesHubDisplayGame: null,
    gamesHubOpen: false,
    currentStreamingGame: null,
    favoriteGameIdSet: new Set<string>(),
    localVideoFilePathForOptions: null,
    spotlightEntryHasGame: (entry: unknown): entry is { kind: "recent"; game: typeof game } => Boolean(entry),
    bumpMediaListRefresh: () => {},
    closeLocalVideoPlayer: () => {},
    setSelectedMediaIndex: () => {},
    ...overrides,
  } as any;
}

test("opening options for a selected game creates play/favorite/variant entries", () => {
  const log = calls();
  let entries: Array<{ id: string; label: string }> = [];
  let focus = -1;
  let open = false;

  const handled = routeOpenOptions(baseOptionsContext({
    setOptionsEntries: (next: typeof entries) => { entries = next; },
    setOptionsFocusIndex: (index: number) => { focus = index; },
    setOptionsOpen: (value: boolean) => { open = value; },
    playUiSound: log.fn("sound"),
  }));

  assert.equal(handled, true);
  assert.deepEqual(entries.map((entry) => entry.id), ["play", "favorite", "variant", "close"]);
  assert.equal(entries[0]?.label, "Play");
  assert.equal(focus, 0);
  assert.equal(open, true);
  assert.deepEqual(log.values, ["sound:move"]);
});

test("activating play option plays selected game and closes hub/options", () => {
  const log = calls();
  const snapshot = { current: { restoreSelectedGameId: "game-1" } };

  const handled = routeOptionsActivate(baseOptionsContext({
    optionsOpen: true,
    optionsEntries: [{ id: "play", label: "Play" }],
    optionsFocusIndex: 0,
    selectedVariantId: "steam",
    gamesHubReturnSnapshotRef: snapshot,
    onPlayGame: (played: typeof game) => log.values.push(`play:${played.id}`),
    onToggleFavoriteGame: log.fn("favorite"),
    onSelectGameVariant: log.fn("variant"),
    setGamesHubOpen: log.fn("hub"),
    setOptionsOpen: log.fn("options"),
    setLastRootGameIndex: log.fn("lastRoot"),
    setGameSubcategory: log.fn("subcategory"),
    throttledOnSelectGame: log.fn("select"),
    setGamesHubFocusIndex: log.fn("hubFocus"),
    setPs5Row: log.fn("row"),
    playUiSound: log.fn("sound"),
  }));

  assert.equal(handled, true);
  assert.equal(snapshot.current, null);
  assert.deepEqual(log.values, ["play:game-1", "hub:false", "options:false", "sound:confirm"]);
});

test("root spotlight cloud resume only resumes when not busy", () => {
  const idle = calls();
  assert.equal(routeCategoryActivate({
    topCategory: "all",
    all: {
      gameSubcategory: "root",
      gamesRootPlane: "spotlight",
      spotlightEntries: [{ kind: "cloudResume", title: "Resume", coverUrl: null, busy: false }],
      spotlightIndex: 0,
      onResumeCloudSession: idle.fn("resume"),
      selectedGameSubcategoryIndex: 0,
      displayItems: [],
      selectedGame: null,
      selectedGameId: "",
      gamesHubReturnSnapshotRef: { current: null },
      spotlightEntryHasGame: () => false,
      setLastRootGameIndex: idle.fn("lastRoot"),
      setGameSubcategory: idle.fn("subcategory"),
      setSelectedGameSubcategoryIndex: idle.fn("selectedIndex"),
      throttledOnSelectGame: idle.fn("select"),
      setGamesHubOpen: idle.fn("hub"),
      setGamesHubFocusIndex: idle.fn("hubFocus"),
      setPs5Row: idle.fn("row"),
      playUiSound: idle.fn("sound"),
    } as any,
  }), true);
  assert.deepEqual(idle.values, ["resume:", "sound:confirm"]);

  const busy = calls();
  routeCategoryActivate({
    topCategory: "all",
    all: {
      gameSubcategory: "root",
      gamesRootPlane: "spotlight",
      spotlightEntries: [{ kind: "cloudResume", title: "Resume", coverUrl: null, busy: true }],
      spotlightIndex: 0,
      onResumeCloudSession: busy.fn("resume"),
      selectedGameSubcategoryIndex: 0,
      displayItems: [],
      selectedGame: null,
      selectedGameId: "",
      gamesHubReturnSnapshotRef: { current: null },
      spotlightEntryHasGame: () => false,
      setLastRootGameIndex: busy.fn("lastRoot"),
      setGameSubcategory: busy.fn("subcategory"),
      setSelectedGameSubcategoryIndex: busy.fn("selectedIndex"),
      throttledOnSelectGame: busy.fn("select"),
      setGamesHubOpen: busy.fn("hub"),
      setGamesHubFocusIndex: busy.fn("hubFocus"),
      setPs5Row: busy.fn("row"),
      playUiSound: busy.fn("sound"),
    } as any,
  });
  assert.deepEqual(busy.values, ["sound:move"]);
});

test("root spotlight game opens hub and stores return snapshot", () => {
  const log = calls();
  const snapshot = { current: null as unknown };

  routeCategoryActivate({
    topCategory: "all",
    all: {
      gameSubcategory: "root",
      gamesRootPlane: "spotlight",
      spotlightEntries: [{ kind: "recent", game }],
      spotlightIndex: 0,
      selectedGameSubcategoryIndex: 3,
      displayItems: [],
      selectedGame: null,
      selectedGameId: "old-game",
      gamesHubReturnSnapshotRef: snapshot,
      spotlightEntryHasGame: (entry: unknown): entry is { kind: "recent"; game: typeof game } => (entry as any)?.kind === "recent",
      setLastRootGameIndex: log.fn("lastRoot"),
      setGameSubcategory: log.fn("subcategory"),
      setSelectedGameSubcategoryIndex: log.fn("selectedIndex"),
      throttledOnSelectGame: log.fn("select"),
      setGamesHubOpen: log.fn("hub"),
      setGamesHubFocusIndex: log.fn("hubFocus"),
      setPs5Row: log.fn("row"),
      playUiSound: log.fn("sound"),
    } as any,
  });

  assert.deepEqual(snapshot.current, {
    gameSubcategory: "root",
    selectedGameSubcategoryIndex: 3,
    gamesRootPlane: "spotlight",
    spotlightIndex: 0,
    restoreSelectedGameId: "game-1",
  });
  assert.deepEqual(log.values, ["lastRoot:3", "subcategory:all", "select:game-1", "hub:true", "hubFocus:0", "row:main", "sound:confirm"]);
});

test("cancel from open games hub restores snapshot fields and selected game", () => {
  const log = calls();
  const snapshot = { current: {
    gameSubcategory: "favorites",
    selectedGameSubcategoryIndex: 4,
    gamesRootPlane: "categories",
    spotlightIndex: 1,
    restoreSelectedGameId: "restore-game",
    restoreCategoryIndex: 2,
    restoreHomeRootPlane: "actions",
  } };

  const handled = routeCancel({
    topCategory: "all",
    all: {
      gamesHubOpen: true,
      gameSubcategory: "all",
      lastRootGameIndex: 0,
      gamesHubReturnSnapshotRef: snapshot,
      setGamesHubFocusIndex: log.fn("hubFocus"),
      setGamesHubOpen: log.fn("hub"),
      setGameSubcategory: log.fn("subcategory"),
      setSelectedGameSubcategoryIndex: log.fn("selectedIndex"),
      setGamesRootPlane: log.fn("plane"),
      setSpotlightIndex: log.fn("spotlight"),
      throttledOnSelectGame: log.fn("select"),
      setCategoryIndex: log.fn("category"),
      setHomeRootPlane: log.fn("homePlane"),
      playUiSound: log.fn("sound"),
    } as any,
  });

  assert.equal(handled, true);
  assert.equal(snapshot.current, null);
  assert.deepEqual(log.values, [
    "sound:move",
    "hubFocus:0",
    "hub:false",
    "subcategory:favorites",
    "selectedIndex:4",
    "plane:categories",
    "spotlight:1",
    "select:restore-game",
    "category:2",
    "homePlane:actions",
  ]);
});

test("all-games secondary cycles library sort only while browsing all games", () => {
  let sort = "recent" as any;
  const log = calls();
  const setLibrarySortId = (updater: (prev: typeof sort) => typeof sort) => { sort = updater(sort); };

  assert.equal(routeSecondaryActivate({
    topCategory: "all",
    all: { gamesShelfBrowseActive: true, gameSubcategory: "all", setLibrarySortId, playUiSound: log.fn("sound") } as any,
  }), true);
  assert.equal(sort, "favoritesFirst");
  assert.deepEqual(log.values, ["sound:move"]);

  assert.equal(routeSecondaryActivate({
    topCategory: "all",
    all: { gamesShelfBrowseActive: true, gameSubcategory: "favorites", setLibrarySortId, playUiSound: log.fn("sound") } as any,
  }), false);
});

test("settings secondary cycles stream and controller settings used by streaming", () => {
  const settings: Record<string, unknown> = {
    resolution: "1280x720",
    fps: 60,
    codec: "H264",
    enableL4S: false,
    enableCloudGsync: true,
    controllerUiSounds: true,
    autoFullScreen: false,
  };
  const changes: Array<[string, unknown]> = [];
  const log = calls();
  const base = {
    settingsSubcategory: "Video",
    settings,
    microphoneDevices: [],
    aspectRatioOptions: [],
    resolutionOptions: ["1280x720", "1920x1080"],
    fpsOptions: [60, 120],
    codecOptions: ["H264", "H265"],
    setEditingThemeChannel: () => {},
    setEditingBandwidth: log.fn("bandwidth"),
    playUiSound: log.fn("sound"),
    onSettingChange: (key: string, value: unknown) => {
      settings[key] = value;
      changes.push([key, value]);
    },
  };

  for (const id of ["resolution", "fps", "codec", "l4s", "cloudGsync", "sounds", "autoFullScreen"] as const) {
    routeSecondaryActivate({
      topCategory: "settings",
      settings: { ...base, displayItems: [{ id, label: id }], selectedSettingIndex: 0 } as any,
    });
  }

  assert.deepEqual(changes, [
    ["resolution", "1920x1080"],
    ["fps", 120],
    ["codec", "H265"],
    ["enableL4S", true],
    ["enableCloudGsync", false],
    ["controllerUiSounds", false],
    ["autoFullScreen", true],
  ]);
});
