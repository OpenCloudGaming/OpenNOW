import {
  CONTROLLER_THEME_STYLE_ORDER,
} from "../constants";
import {
  isEditableTarget,
  sanitizeControllerThemeStyle,
  spotlightEntryHasGame,
} from "../helpers";
import {
  routeCancel,
  routeCategoryActivate,
  routeOpenOptions,
  routeOptionsActivate,
  routeSecondaryActivate,
} from "../actions/actionRouter";
import type {
  ApplyDirection,
  ControllerLibraryEventContext,
  ControllerLibraryEventHandlers,
  CycleTopCategory,
} from "./types";

type ActionHandlers = Pick<
  ControllerLibraryEventHandlers,
  "onActivate" | "onSecondaryActivate" | "onTertiaryActivate" | "onCancel" | "onKeyboard"
>;

export function createActionHandlers(
  ctx: ControllerLibraryEventContext,
  applyDirection: ApplyDirection,
  cycleTopCategory: CycleTopCategory,
): ActionHandlers {
  const {
    selectedGame,
    selectedGameId,
    selectedVariantId,
    onPlayGame,
    onSelectGameVariant,
    onToggleFavoriteGame,
    playUiSound,
    throttledOnSelectGame,
    topCategory,
    selectedSettingIndex,
    selectedMediaIndex,
    selectedGameSubcategoryIndex,
    displayItems,
    mediaAssetItems,
    mediaSubcategory,
    gameSubcategory,
    settings,
    settingsSubcategory,
    lastRootSettingIndex,
    lastRootMediaIndex,
    lastRootGameIndex,
    lastSystemMenuIndex,
    lastThemeRootIndex,
    onSettingChange,
    resolutionOptions,
    fpsOptions,
    codecOptions,
    aspectRatioOptions,
    currentStreamingGame,
    currentTabGame,
    onResumeGame,
    onResumeCloudSession,
    onCloseGame,
    onExitControllerMode,
    onExitApp,
    editingBandwidth,
    editingThemeChannel,
    gamesShelfBrowseActive,
    mediaShelfBrowseActive,
    topLevelRowBehaviorActive,
    canEnterDetailRow,
    ps5Row,
    detailRailIndex,
    detailRailItems,
    optionsOpen,
    optionsFocusIndex,
    optionsEntries,
    gamesRootPlane,
    spotlightIndex,
    spotlightEntries,
    favoriteGameIdSet,
    microphoneDevices,
    gamesHubOpen,
    gamesHubDisplayGame,
    gamesHubFocusIndex,
    gamesHubTiles,
    inStreamMenu,
    endSessionConfirm,
    editingStreamVolume,
    editingStreamMicLevel,
    onStreamMenuMicLevelChange,
    onStreamMenuVolumeChange,
    onStreamMenuToggleMicrophone,
    onStreamMenuToggleFullscreen,
    setSelectedSettingIndex,
    setSettingsSubcategory,
    setMediaSubcategory,
    setSelectedMediaIndex,
    setGameSubcategory,
    setSelectedGameSubcategoryIndex,
    setEditingBandwidth,
    setEditingThemeChannel,
    setEditingStreamVolume,
    setEditingStreamMicLevel,
    setOptionsEntries,
    setOptionsOpen,
    setOptionsFocusIndex,
    setGamesHubFocusIndex,
    setPs5Row,
    gamesHubReturnSnapshotRef,
    setGamesHubOpen,
    setEndSessionConfirm,
    setLastRootGameIndex,
    setLastRootSettingIndex,
    setLastSystemMenuIndex,
    setLastThemeRootIndex,
    setLastRootMediaIndex,
    setLibrarySortId,
    setCategoryIndex,
    setGamesRootPlane,
    setHomeRootPlane,
    setSpotlightIndex,
    featuredHomeGame,
    homeDualShelf,
    gamesDualShelf,
    homeRootPlane,
    localVideoPlayerOpen,
    closeLocalVideoPlayer,
    openLocalVideoPlayer,
    localVideoFilePathForOptions,
    bumpMediaListRefresh,
  } = ctx;

  const openOptionsMenu = (): void => {
    routeOpenOptions({
      gamesShelfBrowseActive,
      mediaShelfBrowseActive,
      topCategory,
      gameSubcategory,
      gamesRootPlane,
      gamesDualShelf,
      spotlightEntries,
      spotlightIndex,
      selectedMediaIndex,
      mediaAssetItems,
      selectedGame,
      gamesHubDisplayGame,
      gamesHubOpen,
      currentStreamingGame,
      favoriteGameIdSet,
      setOptionsEntries,
      setOptionsFocusIndex,
      setOptionsOpen,
      playUiSound,
      spotlightEntryHasGame,
      localVideoFilePathForOptions,
      bumpMediaListRefresh,
      closeLocalVideoPlayer,
      setSelectedMediaIndex,
    });
  };

  const onSecondaryActivate = (): void => {
    if (localVideoPlayerOpen) return;
    if (optionsOpen) return;
    if (gamesHubOpen) return;
    if (routeSecondaryActivate({
      topCategory,
      all: topCategory === "all" ? { gamesShelfBrowseActive, gameSubcategory, setLibrarySortId, playUiSound } : undefined,
      settings: topCategory === "settings" ? {
        settingsSubcategory,
        displayItems,
        selectedSettingIndex,
        onSettingChange,
        settings,
        microphoneDevices,
        aspectRatioOptions,
        resolutionOptions,
        fpsOptions,
        codecOptions,
        setEditingThemeChannel,
        setEditingBandwidth,
        playUiSound,
      } : undefined,
    })) return;
    if (topCategory === "current" && inStreamMenu) {
      const item = displayItems[selectedSettingIndex];
      if (item?.id === "streamVolume" && onStreamMenuVolumeChange) {
        setEditingStreamVolume(true);
        setEditingStreamMicLevel(false);
        playUiSound("move");
      } else if (item?.id === "streamMicLevel" && onStreamMenuMicLevelChange) {
        setEditingStreamMicLevel(true);
        setEditingStreamVolume(false);
        playUiSound("move");
      }
      return;
    }
    if (topCategory === "current") return;
    if (topCategory === "settings") return;
  };

  const onActivate = (): void => {
    if (routeOptionsActivate({
      optionsOpen,
      optionsEntries,
      optionsFocusIndex,
      gamesShelfBrowseActive,
      mediaShelfBrowseActive,
      topCategory,
      gameSubcategory,
      gamesRootPlane,
      spotlightEntries,
      spotlightIndex,
      selectedMediaIndex,
      mediaAssetItems,
      selectedGame,
      gamesHubDisplayGame,
      gamesHubOpen,
      currentStreamingGame,
      favoriteGameIdSet,
      setOptionsEntries,
      setOptionsFocusIndex,
      setOptionsOpen,
      playUiSound,
      spotlightEntryHasGame,
      selectedVariantId,
      onPlayGame,
      onToggleFavoriteGame,
      onSelectGameVariant,
      selectedGameSubcategoryIndex,
      setLastRootGameIndex,
      setGameSubcategory,
      throttledOnSelectGame,
      setGamesHubOpen,
      setGamesHubFocusIndex,
      setPs5Row,
      gamesHubReturnSnapshotRef,
      localVideoFilePathForOptions,
      bumpMediaListRefresh,
      closeLocalVideoPlayer,
      setSelectedMediaIndex,
    })) return;
    if (localVideoPlayerOpen) return;

    if (
      gamesHubOpen &&
      gamesHubDisplayGame &&
      ((topCategory === "all" && gameSubcategory !== "root") || topCategory === "current")
    ) {
      const tile = gamesHubTiles[gamesHubFocusIndex];
      const hubGame = gamesHubDisplayGame;
      if (!tile || tile.disabled) {
        playUiSound("move");
        return;
      }
      if (tile.id === "play") {
        onPlayGame(hubGame);
        gamesHubReturnSnapshotRef.current = null;
        setGamesHubOpen(false);
        setGamesHubFocusIndex(0);
        playUiSound("confirm");
        return;
      }
      if (tile.id === "favorite") {
        onToggleFavoriteGame(hubGame.id);
        playUiSound("confirm");
        return;
      }
      if (tile.id === "version" && hubGame.variants.length > 1) {
        const idx = hubGame.variants.findIndex((v: { id: string }) => v.id === selectedVariantId);
        const next = hubGame.variants[(idx + 1) % hubGame.variants.length];
        onSelectGameVariant(hubGame.id, next.id);
        playUiSound("confirm");
        return;
      }
      playUiSound("move");
      return;
    }

    if (ps5Row === "top") {
      setPs5Row("main");
      playUiSound("confirm");
      return;
    }

    if (ps5Row === "detail") {
      if (!canEnterDetailRow || detailRailItems.length === 0) {
        setPs5Row("main");
        return;
      }
      const selectedDetail = detailRailItems[detailRailIndex];
      if (!selectedDetail) return;
      if (topCategory === "media") {
        if (selectedDetail.id === "m1") {
          const current = mediaAssetItems[selectedMediaIndex];
          if (current && typeof window.openNow?.showMediaInFolder === "function") {
            void window.openNow.showMediaInFolder({ filePath: current.filePath });
          }
          playUiSound("confirm");
          return;
        }
        if (selectedDetail.id === "m2") {
          setMediaSubcategory("root");
          setSelectedMediaIndex(lastRootMediaIndex);
          setPs5Row("main");
          playUiSound("confirm");
          return;
        }
        playUiSound("confirm");
        return;
      }
      setPs5Row("main");
      playUiSound("confirm");
      return;
    }

    if (topCategory === "settings" && settingsSubcategory !== "root" && editingBandwidth) {
      setEditingBandwidth(false);
      playUiSound("confirm");
      return;
    }
    if (topCategory === "settings" && settingsSubcategory === "ThemeColor" && editingThemeChannel) {
      setEditingThemeChannel(null);
      playUiSound("confirm");
      return;
    }
    if (topCategory === "current" && inStreamMenu && editingStreamVolume) {
      setEditingStreamVolume(false);
      playUiSound("confirm");
      return;
    }
    if (topCategory === "current" && inStreamMenu && editingStreamMicLevel) {
      setEditingStreamMicLevel(false);
      playUiSound("confirm");
      return;
    }

    if (topCategory === "current" && homeDualShelf && homeRootPlane === "spotlight") {
      const entry = spotlightEntries[spotlightIndex];
      if (entry?.kind === "cloudResume") {
        if (!entry.busy && onResumeCloudSession) {
          onResumeCloudSession();
          playUiSound("confirm");
        } else {
          playUiSound("move");
        }
        return;
      }
      if (spotlightEntryHasGame(entry)) {
        const game = entry.game;
        if (game) {
          gamesHubReturnSnapshotRef.current = {
            gameSubcategory,
            selectedGameSubcategoryIndex,
            gamesRootPlane,
            spotlightIndex,
            restoreSelectedGameId: game.id,
            restoreHomeRootPlane: homeRootPlane,
          };
          throttledOnSelectGame(game.id);
          setGamesHubOpen(true);
          setGamesHubFocusIndex(0);
          setPs5Row("main");
          playUiSound("confirm");
        } else {
          playUiSound("move");
        }
        return;
      }
      playUiSound("move");
      return;
    }

    if (topCategory === "current") {
      const item = displayItems[selectedSettingIndex];
      if (item?.id === "featured" && featuredHomeGame && onPlayGame) {
        onPlayGame(featuredHomeGame);
        playUiSound("confirm");
        return;
      }
      if (item?.id === "resume" && currentTabGame && onResumeGame) {
        onResumeGame(currentTabGame);
        playUiSound("confirm");
        return;
      }
      if (item?.id === "toggleMic" && onStreamMenuToggleMicrophone) {
        onStreamMenuToggleMicrophone();
        playUiSound("confirm");
        return;
      }
      if (item?.id === "openMedia") {
        window.dispatchEvent(new CustomEvent("opennow:controller-navigate", { detail: { target: "media" } }));
        playUiSound("confirm");
        return;
      }
      if (item?.id === "toggleFullscreen" && onStreamMenuToggleFullscreen) {
        onStreamMenuToggleFullscreen();
        playUiSound("confirm");
        return;
      }
      if (item?.id === "closeGame" && onCloseGame) {
        if (inStreamMenu) {
          if (endSessionConfirm) {
            setEndSessionConfirm(false);
            onCloseGame();
            playUiSound("confirm");
          } else {
            setEndSessionConfirm(true);
            playUiSound("move");
          }
          return;
        }
        onCloseGame();
        playUiSound("confirm");
        return;
      }
      return;
    }

    const routedCategoryActivate = routeCategoryActivate({
      topCategory,
      settings: topCategory === "settings" ? {
        settingsSubcategory,
        selectedSettingIndex,
        displayItems,
        currentStreamingGame,
        onExitApp,
        onExitControllerMode,
        onSettingChange,
        settings,
        setLastRootSettingIndex,
        setLastSystemMenuIndex,
        setLastThemeRootIndex,
        setSettingsSubcategory,
        setSelectedSettingIndex,
        setEditingThemeChannel,
        setCategoryIndex: (updater: (value: number) => number) => setCategoryIndex((prev: number) => updater(prev)),
        playUiSound,
        sanitizeControllerThemeStyle,
        themeStyleOrder: CONTROLLER_THEME_STYLE_ORDER,
      } : undefined,
      media: topCategory === "media" ? {
        mediaSubcategory,
        displayItems,
        selectedMediaIndex,
        setLastRootMediaIndex,
        setMediaSubcategory,
        setSelectedMediaIndex,
        mediaAssetItems,
        playUiSound,
        openLocalVideoPlayer,
      } : undefined,
      all: topCategory === "all" ? {
        gameSubcategory,
        gamesRootPlane,
        spotlightEntries,
        spotlightIndex,
        onResumeCloudSession,
        selectedGameSubcategoryIndex,
        displayItems,
        selectedGame,
        selectedGameId,
        setLastRootGameIndex,
        setGameSubcategory,
        setSelectedGameSubcategoryIndex,
        setGamesHubOpen,
        setGamesHubFocusIndex,
        setPs5Row,
        throttledOnSelectGame,
        gamesHubReturnSnapshotRef,
        playUiSound,
        spotlightEntryHasGame,
      } : undefined,
    });
    if (routedCategoryActivate) {
      if (topCategory === "settings" && settingsSubcategory !== "root") {
        const setting = displayItems[selectedSettingIndex];
        if (setting?.id !== "exitControllerMode") onSecondaryActivate();
      }
      return;
    }
    if (selectedGame) {
      onPlayGame(selectedGame);
      playUiSound("confirm");
    }
  };

  const onTertiaryActivate = (): void => {
    if (optionsOpen) return;
    openOptionsMenu();
  };

  const onCancel = (e: Event): void => {
    if (optionsOpen) {
      setOptionsOpen(false);
      playUiSound("move");
      e.preventDefault();
      return;
    }
    if (localVideoPlayerOpen) {
      closeLocalVideoPlayer();
      playUiSound("move");
      e.preventDefault();
      return;
    }
    if (inStreamMenu && endSessionConfirm) {
      setEndSessionConfirm(false);
      playUiSound("move");
      e.preventDefault();
      return;
    }
    if (inStreamMenu && editingStreamVolume) {
      setEditingStreamVolume(false);
      playUiSound("move");
      e.preventDefault();
      return;
    }
    if (inStreamMenu && editingStreamMicLevel) {
      setEditingStreamMicLevel(false);
      playUiSound("move");
      e.preventDefault();
      return;
    }
    if (topCategory === "settings" && settingsSubcategory !== "root") {
      routeCancel({
        topCategory,
        settings: {
          settingsSubcategory,
          editingBandwidth,
          editingThemeChannel,
          lastThemeRootIndex,
          lastSystemMenuIndex,
          lastRootSettingIndex,
          setEditingBandwidth,
          setEditingThemeChannel,
          setSettingsSubcategory,
          setSelectedSettingIndex,
          playUiSound,
        },
      });
      e.preventDefault();
      return;
    }
    if (topCategory === "media" && mediaSubcategory !== "root") {
      routeCancel({
        topCategory,
        media: {
          lastRootMediaIndex,
          setMediaSubcategory,
          setSelectedMediaIndex,
          playUiSound,
        },
      });
      e.preventDefault();
      return;
    }
    if (
      (topCategory === "all" && gameSubcategory !== "root") ||
      (topCategory === "current" && gamesHubOpen)
    ) {
      routeCancel({
        topCategory,
        all: {
          gamesHubOpen,
          gameSubcategory,
          lastRootGameIndex,
          gamesHubReturnSnapshotRef,
          setGamesHubFocusIndex,
          setGamesHubOpen,
          setGameSubcategory,
          setSelectedGameSubcategoryIndex,
          setGamesRootPlane,
          setSpotlightIndex,
          throttledOnSelectGame,
          playUiSound,
          setCategoryIndex: (idx: number) => {
            setCategoryIndex(idx);
          },
          setHomeRootPlane,
        },
      });
      e.preventDefault();
      return;
    }
    e.preventDefault();
  };

  const onKeyboard = (e: KeyboardEvent): void => {
    if (e.repeat || e.altKey || e.ctrlKey || e.metaKey || isEditableTarget(e.target)) return;
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      applyDirection("left");
      return;
    }
    if (e.key === "ArrowRight") {
      e.preventDefault();
      applyDirection("right");
      return;
    }
    if (e.key === "ArrowUp") {
      e.preventDefault();
      applyDirection("up");
      return;
    }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      applyDirection("down");
      return;
    }
    if (e.key === "Enter") {
      e.preventDefault();
      onActivate();
      return;
    }
    if (e.key.toLowerCase() === "x") {
      e.preventDefault();
      onSecondaryActivate();
      return;
    }
    if (e.key.toLowerCase() === "y") {
      e.preventDefault();
      onTertiaryActivate();
      return;
    }
    if (e.key.toLowerCase() === "f") {
      e.preventDefault();
      onSecondaryActivate();
      return;
    }
    if (e.key.toLowerCase() === "o") {
      e.preventDefault();
      onTertiaryActivate();
      return;
    }
    if (e.key.toLowerCase() === "q" && topLevelRowBehaviorActive && !gamesHubOpen && !localVideoPlayerOpen) {
      e.preventDefault();
      cycleTopCategory(-1);
      return;
    }
    if (e.key.toLowerCase() === "e" && topLevelRowBehaviorActive && !gamesHubOpen && !localVideoPlayerOpen) {
      e.preventDefault();
      cycleTopCategory(1);
      return;
    }
    if (e.key === "Backspace" || e.key === "Escape") {
      if (optionsOpen) {
        e.preventDefault();
        setOptionsOpen(false);
        playUiSound("move");
        return;
      }
      if (topCategory === "settings" && settingsSubcategory !== "root") {
        onCancel(e);
        return;
      }
      if (topCategory === "media" && mediaSubcategory !== "root") {
        onCancel(e);
        return;
      }
      if (topCategory === "all" && gameSubcategory !== "root") {
        onCancel(e);
        return;
      }
      if (inStreamMenu && endSessionConfirm) {
        e.preventDefault();
        setEndSessionConfirm(false);
        playUiSound("move");
        return;
      }
      if (inStreamMenu && editingStreamVolume) {
        e.preventDefault();
        setEditingStreamVolume(false);
        playUiSound("move");
        return;
      }
      if (inStreamMenu && editingStreamMicLevel) {
        e.preventDefault();
        setEditingStreamMicLevel(false);
        playUiSound("move");
        return;
      }
      e.preventDefault();
    }
  };

  return {
    onActivate,
    onSecondaryActivate,
    onTertiaryActivate,
    onCancel,
    onKeyboard,
  };
}
