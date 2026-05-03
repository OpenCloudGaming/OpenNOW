import type { GameInfo } from "@shared/gfn";
import type { OptionsActionContext } from "./contracts";

export function openOptionsMenuAction(context: OptionsActionContext): boolean {
  const {
    gamesShelfBrowseActive,
    selectedGame,
    gamesHubDisplayGame = null,
    gamesHubOpen = false,
    currentStreamingGame,
    favoriteGameIdSet,
    mediaShelfBrowseActive,
    mediaAssetItems,
    selectedMediaIndex,
    topCategory,
    gameSubcategory,
    gamesRootPlane,
    gamesDualShelf = true,
    spotlightEntries,
    spotlightIndex,
    spotlightEntryHasGame,
    setOptionsEntries,
    setOptionsFocusIndex,
    setOptionsOpen,
    playUiSound,
    localVideoFilePathForOptions,
  } = context;

  const entries: Array<{ id: string; label: string }> = [];
  const gameForHubOptions =
    gamesShelfBrowseActive && selectedGame
      ? selectedGame
      : topCategory === "current" && gamesHubOpen && gamesHubDisplayGame
        ? gamesHubDisplayGame
        : null;
  if (gameForHubOptions) {
    entries.push({
      id: "play",
      label: currentStreamingGame && currentStreamingGame.id !== gameForHubOptions.id ? "Switch" : "Play",
    });
    entries.push({
      id: "favorite",
      label: favoriteGameIdSet.has(gameForHubOptions.id) ? "Remove favorite" : "Add favorite",
    });
    if (gameForHubOptions.variants.length > 1) {
      entries.push({ id: "variant", label: "Change version" });
    }
  } else if (
    (mediaShelfBrowseActive && mediaAssetItems[selectedMediaIndex]) ||
    (typeof localVideoFilePathForOptions === "string" && localVideoFilePathForOptions.length > 0)
  ) {
    const mediaPath = localVideoFilePathForOptions ?? mediaAssetItems[selectedMediaIndex]?.filePath;
    if (!mediaPath) {
      /* noop */
    } else {
      entries.push({ id: "openFolder", label: "Open folder" });
      entries.push({ id: "mediaDelete", label: "Delete File" });
      entries.push({ id: "mediaRegenThumb", label: "Regen Thumbnail" });
    }
  } else if (
    topCategory === "all" &&
    gameSubcategory === "root" &&
    gamesDualShelf &&
    gamesRootPlane === "spotlight" &&
    spotlightEntryHasGame(spotlightEntries[spotlightIndex])
  ) {
    entries.push({ id: "openLibrary", label: "View in library" });
  }
  if (entries.length === 0) return false;
  entries.push({ id: "close", label: "Back" });
  setOptionsEntries(entries);
  setOptionsFocusIndex(0);
  setOptionsOpen(true);
  playUiSound("move");
  return true;
}

interface OptionsActivateContext extends OptionsActionContext {
  optionsEntries: Array<{ id: string; label: string }>;
  optionsFocusIndex: number;
  selectedVariantId: string;
  onPlayGame: (game: GameInfo) => void;
  onToggleFavoriteGame: (gameId: string) => void;
  onSelectGameVariant: (gameId: string, variantId: string) => void;
  selectedGameSubcategoryIndex: number;
  setLastRootGameIndex: (index: number) => void;
  setGameSubcategory: (subcategory: "root" | "all" | "favorites" | `genre:${string}`) => void;
  throttledOnSelectGame: (id: string) => void;
  setGamesHubOpen: (open: boolean) => void;
  setGamesHubFocusIndex: (index: number) => void;
  setPs5Row: (row: "top" | "main" | "detail") => void;
  gamesHubReturnSnapshotRef: React.MutableRefObject<{
    gameSubcategory: "root" | "all" | "favorites" | `genre:${string}`;
    selectedGameSubcategoryIndex: number;
    gamesRootPlane: "spotlight" | "categories";
    spotlightIndex: number;
    restoreSelectedGameId?: string;
  } | null>;
}

export function handleOptionsActivateAction(context: OptionsActivateContext): boolean {
  const {
    optionsEntries,
    optionsFocusIndex,
    selectedGame,
    gamesHubDisplayGame = null,
    gamesHubOpen = false,
    topCategory,
    onPlayGame,
    gamesHubReturnSnapshotRef,
    setGamesHubOpen,
    setOptionsOpen,
    onToggleFavoriteGame,
    selectedVariantId,
    onSelectGameVariant,
    mediaAssetItems,
    selectedMediaIndex,
    spotlightEntries,
    spotlightIndex,
    spotlightEntryHasGame,
    selectedGameSubcategoryIndex,
    gamesRootPlane,
    setLastRootGameIndex,
    setGameSubcategory,
    throttledOnSelectGame,
    setGamesHubFocusIndex,
    setPs5Row,
    playUiSound,
    localVideoFilePathForOptions,
    bumpMediaListRefresh,
    closeLocalVideoPlayer,
    setSelectedMediaIndex,
  } = context;

  const mediaOptionsFilePath = (): string | null =>
    localVideoFilePathForOptions ?? mediaAssetItems[selectedMediaIndex]?.filePath ?? null;

  if (optionsEntries.length === 0) return false;
  const opt = optionsEntries[optionsFocusIndex];
  if (!opt) return true;
  if (opt.id === "close") {
    setOptionsOpen(false);
    playUiSound("move");
    return true;
  }
  const gameForOption =
    topCategory === "current" && gamesHubOpen && gamesHubDisplayGame ? gamesHubDisplayGame : selectedGame;
  if (opt.id === "play" && gameForOption) {
    onPlayGame(gameForOption);
    gamesHubReturnSnapshotRef.current = null;
    setGamesHubOpen(false);
    setOptionsOpen(false);
    playUiSound("confirm");
    return true;
  }
  if (opt.id === "favorite" && gameForOption) {
    onToggleFavoriteGame(gameForOption.id);
    setOptionsOpen(false);
    playUiSound("confirm");
    return true;
  }
  if (opt.id === "variant" && gameForOption && gameForOption.variants.length > 1) {
    const idx = gameForOption.variants.findIndex((v) => v.id === selectedVariantId);
    const next = gameForOption.variants[(idx + 1) % gameForOption.variants.length];
    onSelectGameVariant(gameForOption.id, next.id);
    setOptionsOpen(false);
    playUiSound("confirm");
    return true;
  }
  if (opt.id === "openFolder") {
    const fp = mediaOptionsFilePath();
    if (fp && typeof window.openNow?.showMediaInFolder === "function") {
      void window.openNow.showMediaInFolder({ filePath: fp });
    }
    setOptionsOpen(false);
    playUiSound("confirm");
    return true;
  }
  if (opt.id === "mediaDelete") {
    const fp = mediaOptionsFilePath();
    if (!fp || typeof window.openNow?.deleteMediaFile !== "function") return true;
    void window.openNow.deleteMediaFile({ filePath: fp }).then((r) => {
      if (r.ok) {
        setOptionsOpen(false);
        closeLocalVideoPlayer();
        bumpMediaListRefresh();
        setSelectedMediaIndex((i) => Math.max(0, i - 1));
        playUiSound("confirm");
      } else {
        playUiSound("move");
      }
    });
    return true;
  }
  if (opt.id === "mediaRegenThumb") {
    const fp = mediaOptionsFilePath();
    if (!fp || typeof window.openNow?.regenMediaThumbnail !== "function") return true;
    void window.openNow.regenMediaThumbnail({ filePath: fp }).then((r) => {
      if (r.ok) {
        setOptionsOpen(false);
        bumpMediaListRefresh();
        playUiSound("confirm");
      } else {
        playUiSound("move");
      }
    });
    return true;
  }
  if (opt.id === "openLibrary") {
    const entry = spotlightEntries[spotlightIndex];
    const game = spotlightEntryHasGame(entry) ? entry.game : null;
    if (game) {
      gamesHubReturnSnapshotRef.current = {
        gameSubcategory: "root",
        selectedGameSubcategoryIndex,
        gamesRootPlane,
        spotlightIndex,
        restoreSelectedGameId: game.id,
      };
      setLastRootGameIndex(selectedGameSubcategoryIndex);
      setGameSubcategory("all");
      throttledOnSelectGame(game.id);
      setGamesHubOpen(true);
      setGamesHubFocusIndex(0);
      setPs5Row("main");
      setOptionsOpen(false);
      playUiSound("confirm");
    }
    return true;
  }
  return true;
}
