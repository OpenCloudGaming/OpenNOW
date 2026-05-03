import type { AllActivateContext, AllCancelContext, AllSecondaryContext } from "./contracts";

export function handleAllActivateAction(context: AllActivateContext): boolean {
  const {
    gameSubcategory,
    gamesRootPlane,
    spotlightEntries,
    spotlightIndex,
    onResumeCloudSession,
    selectedGameSubcategoryIndex,
    displayItems,
    setLastRootGameIndex,
    setGameSubcategory,
    setSelectedGameSubcategoryIndex,
    selectedGame,
    selectedGameId,
    gamesHubReturnSnapshotRef,
    throttledOnSelectGame,
    setGamesHubOpen,
    setGamesHubFocusIndex,
    setPs5Row,
    playUiSound,
    spotlightEntryHasGame,
  } = context;

  if (gameSubcategory === "root") {
    if (gamesRootPlane === "spotlight") {
      const entry = spotlightEntries[spotlightIndex];
      if (entry?.kind === "cloudResume") {
        if (!entry.busy && onResumeCloudSession) {
          onResumeCloudSession();
          playUiSound("confirm");
        } else {
          playUiSound("move");
        }
        return true;
      }
      if (spotlightEntryHasGame(entry)) {
        const game = entry.game;
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
        playUiSound("confirm");
        return true;
      }
    }
    const item = displayItems[selectedGameSubcategoryIndex];
    if (item) {
      setLastRootGameIndex(selectedGameSubcategoryIndex);
      setGameSubcategory(item.id as "all" | "favorites" | `genre:${string}`);
      setSelectedGameSubcategoryIndex(0);
      playUiSound("confirm");
    }
    return true;
  }

  if (selectedGame) {
    gamesHubReturnSnapshotRef.current = {
      gameSubcategory,
      selectedGameSubcategoryIndex,
      gamesRootPlane,
      spotlightIndex,
      restoreSelectedGameId: selectedGameId,
    };
    setGamesHubOpen(true);
    setGamesHubFocusIndex(0);
    setPs5Row("main");
    playUiSound("confirm");
  }
  return true;
}

export function handleAllSecondaryAction(context: AllSecondaryContext): boolean {
  const { gamesShelfBrowseActive, gameSubcategory, setLibrarySortId, playUiSound } = context;
  if (gamesShelfBrowseActive && gameSubcategory === "all") {
    setLibrarySortId((prev) => {
      const order = ["recent", "favoritesFirst", "az", "za"] as const;
      const i = order.indexOf(prev);
      return order[(i + 1) % order.length] ?? "recent";
    });
    playUiSound("move");
    return true;
  }
  return false;
}

export function handleAllCancelAction(context: AllCancelContext): boolean {
  const {
    gamesHubOpen,
    gamesHubReturnSnapshotRef,
    setGamesHubFocusIndex,
    setGamesHubOpen,
    setGameSubcategory,
    setSelectedGameSubcategoryIndex,
    setGamesRootPlane,
    setSpotlightIndex,
    throttledOnSelectGame,
    lastRootGameIndex,
    playUiSound,
    setCategoryIndex,
    setHomeRootPlane,
  } = context;

  if (gamesHubOpen) {
    playUiSound("move");
    const snap = gamesHubReturnSnapshotRef.current;
    gamesHubReturnSnapshotRef.current = null;
    setGamesHubFocusIndex(0);
    setGamesHubOpen(false);
    if (snap) {
      setGameSubcategory(snap.gameSubcategory);
      setSelectedGameSubcategoryIndex(snap.selectedGameSubcategoryIndex);
      setGamesRootPlane(snap.gamesRootPlane);
      setSpotlightIndex(snap.spotlightIndex);
      if (snap.restoreSelectedGameId) {
        throttledOnSelectGame(snap.restoreSelectedGameId);
      }
      if (snap.restoreCategoryIndex != null && setCategoryIndex) {
        setCategoryIndex(snap.restoreCategoryIndex);
      }
      if (snap.restoreHomeRootPlane != null && setHomeRootPlane) {
        setHomeRootPlane(snap.restoreHomeRootPlane);
      }
    }
    return true;
  }
  setGameSubcategory("root");
  setSelectedGameSubcategoryIndex(lastRootGameIndex);
  playUiSound("move");
  return true;
}
