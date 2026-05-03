import type { GameInfo } from "@shared/gfn";
import type { OptionsActionContext } from "./contracts";

export function openOptionsMenuAction(context: OptionsActionContext): boolean {
  const {
    gamesShelfBrowseActive,
    selectedGame,
    currentStreamingGame,
    favoriteGameIdSet,
    mediaShelfBrowseActive,
    mediaAssetItems,
    selectedMediaIndex,
    topCategory,
    gameSubcategory,
    gamesRootPlane,
    spotlightEntries,
    spotlightIndex,
    spotlightEntryHasGame,
    setOptionsEntries,
    setOptionsFocusIndex,
    setOptionsOpen,
    playUiSound,
  } = context;

  const entries: Array<{ id: string; label: string }> = [];
  if (gamesShelfBrowseActive && selectedGame) {
    entries.push({
      id: "play",
      label: currentStreamingGame && currentStreamingGame.id !== selectedGame.id ? "Switch" : "Play",
    });
    entries.push({
      id: "favorite",
      label: favoriteGameIdSet.has(selectedGame.id) ? "Remove favorite" : "Add favorite",
    });
    if (selectedGame.variants.length > 1) {
      entries.push({ id: "variant", label: "Change version" });
    }
  } else if (mediaShelfBrowseActive && mediaAssetItems[selectedMediaIndex]) {
    entries.push({ id: "openFolder", label: "Open folder" });
  } else if (topCategory === "all" && gameSubcategory === "root" && gamesRootPlane === "spotlight" && spotlightEntryHasGame(spotlightEntries[spotlightIndex])) {
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
  } = context;

  if (optionsEntries.length === 0) return false;
  const opt = optionsEntries[optionsFocusIndex];
  if (!opt) return true;
  if (opt.id === "close") {
    setOptionsOpen(false);
    playUiSound("move");
    return true;
  }
  if (opt.id === "play" && selectedGame) {
    onPlayGame(selectedGame);
    gamesHubReturnSnapshotRef.current = null;
    setGamesHubOpen(false);
    setOptionsOpen(false);
    playUiSound("confirm");
    return true;
  }
  if (opt.id === "favorite" && selectedGame) {
    onToggleFavoriteGame(selectedGame.id);
    setOptionsOpen(false);
    playUiSound("confirm");
    return true;
  }
  if (opt.id === "variant" && selectedGame && selectedGame.variants.length > 1) {
    const idx = selectedGame.variants.findIndex((v) => v.id === selectedVariantId);
    const next = selectedGame.variants[(idx + 1) % selectedGame.variants.length];
    onSelectGameVariant(selectedGame.id, next.id);
    setOptionsOpen(false);
    playUiSound("confirm");
    return true;
  }
  if (opt.id === "openFolder") {
    const cur = mediaAssetItems[selectedMediaIndex];
    if (cur && typeof window.openNow?.showMediaInFolder === "function") {
      void window.openNow.showMediaInFolder({ filePath: cur.filePath });
    }
    setOptionsOpen(false);
    playUiSound("confirm");
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
