import type { Direction } from "../types";
import { clampRgbByte } from "../helpers";
import type {
  ApplyDirection,
  ControllerLibraryEventContext,
  CycleTopCategory,
} from "./types";

type NavigationHandlers = {
  applyDirection: ApplyDirection;
  cycleTopCategory: CycleTopCategory;
  onDirection: (event: Event) => void;
  onShoulder: (event: Event) => void;
};

export function createNavigationHandlers(
  ctx: ControllerLibraryEventContext,
): NavigationHandlers {
  const {
    isLoading,
    TOP_CATEGORIES,
    categorizedGames,
    selectedIndex,
    selectedGame,
    selectedGameId,
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
    editingBandwidth,
    editingThemeChannel,
    playUiSound,
    inStreamMenu,
    editingStreamVolume,
    editingStreamMicLevel,
    streamMenuMicLevel,
    onStreamMenuMicLevelChange,
    streamMenuVolume,
    onStreamMenuVolumeChange,
    gamesShelfBrowseActive,
    mediaShelfBrowseActive,
    topLevelRowBehaviorActive,
    topLevelShelfIndex,
    canEnterDetailRow,
    canEnterTopRow,
    ps5Row,
    detailRailIndex,
    detailRailItems,
    optionsOpen,
    optionsFocusIndex,
    optionsEntries,
    gamesRootPlane,
    homeRootPlane,
    spotlightIndex,
    spotlightEntries,
    gamesDualShelf,
    homeDualShelf,
    gamesHubOpen,
    gamesHubDisplayGame,
    gamesHubFocusIndex,
    gamesHubTiles,
    onSettingChange,
    setCategoryIndex,
    setSelectedSettingIndex,
    setSettingsSubcategory,
    setSelectedMediaIndex,
    setMediaSubcategory,
    setSelectedGameSubcategoryIndex,
    setGameSubcategory,
    setEditingBandwidth,
    setEditingThemeChannel,
    setEditingStreamVolume,
    setEditingStreamMicLevel,
    setOptionsFocusIndex,
    setGamesHubFocusIndex,
    setPs5Row,
    setDetailRailIndex,
    setGamesRootPlane,
    setHomeRootPlane,
    setSpotlightIndex,
    gamesHubReturnSnapshotRef,
    setGamesHubOpen,
    localVideoPlayerOpen,
  } = ctx;

  const cycleTopCategory = (delta: number): void => {
    setCategoryIndex((prev: number) => (prev + delta + TOP_CATEGORIES.length) % TOP_CATEGORIES.length);
    setSelectedSettingIndex(0);
    setSettingsSubcategory("root");
    setSelectedMediaIndex(0);
    setMediaSubcategory("root");
    setSelectedGameSubcategoryIndex(0);
    setGameSubcategory("root");
    setHomeRootPlane("spotlight");
    setEditingBandwidth(false);
    setEditingThemeChannel(null);
    setEditingStreamVolume(false);
    setEditingStreamMicLevel(false);
    playUiSound("move");
  };

  const applyDirection = (direction: Direction): void => {
    if (topCategory === "settings" && settingsSubcategory === "ThemeColor" && editingThemeChannel && onSettingChange) {
      const step = 8;
      const tc = settings.controllerThemeColor ?? { r: 124, g: 241, b: 177 };
      const channel = editingThemeChannel;
      const cur = tc[channel];
      if (direction === "left") {
        const next = clampRgbByte(cur - step);
        onSettingChange("controllerThemeColor", { ...tc, [channel]: next });
        playUiSound("move");
        return;
      }
      if (direction === "right") {
        const next = clampRgbByte(cur + step);
        onSettingChange("controllerThemeColor", { ...tc, [channel]: next });
        playUiSound("move");
        return;
      }
    }

    if (topCategory === "settings" && settingsSubcategory !== "root" && editingBandwidth) {
      const step = 5;
      const current = settings.maxBitrateMbps ?? 75;
      if (direction === "left") {
        const next = Math.max(5, current - step);
        onSettingChange && onSettingChange("maxBitrateMbps" as any, next as any);
        playUiSound("move");
        return;
      }
      if (direction === "right") {
        const next = Math.min(150, current + step);
        onSettingChange && onSettingChange("maxBitrateMbps" as any, next as any);
        playUiSound("move");
        return;
      }
    }

    if (topCategory === "current" && inStreamMenu && editingStreamVolume && onStreamMenuVolumeChange) {
      const step = 0.05;
      const cur = streamMenuVolume ?? 1;
      if (direction === "left") {
        onStreamMenuVolumeChange(Math.max(0, cur - step));
        playUiSound("move");
        return;
      }
      if (direction === "right") {
        onStreamMenuVolumeChange(Math.min(1, cur + step));
        playUiSound("move");
        return;
      }
      return;
    }

    if (topCategory === "current" && inStreamMenu && editingStreamMicLevel && onStreamMenuMicLevelChange) {
      const step = 0.05;
      const cur = streamMenuMicLevel ?? 1;
      if (direction === "left") {
        onStreamMenuMicLevelChange(Math.max(0, cur - step));
        playUiSound("move");
        return;
      }
      if (direction === "right") {
        onStreamMenuMicLevelChange(Math.min(1, cur + step));
        playUiSound("move");
        return;
      }
      return;
    }

    if (isLoading && topCategory !== "settings" && topCategory !== "current") return;

    if (optionsOpen && optionsEntries.length > 0) {
      if (direction === "up") {
        const ni = Math.max(0, optionsFocusIndex - 1);
        if (ni !== optionsFocusIndex) {
          playUiSound("move");
          setOptionsFocusIndex(ni);
        }
        return;
      }
      if (direction === "down") {
        const ni = Math.min(optionsEntries.length - 1, optionsFocusIndex + 1);
        if (ni !== optionsFocusIndex) {
          playUiSound("move");
          setOptionsFocusIndex(ni);
        }
        return;
      }
      return;
    }

    if (localVideoPlayerOpen) {
      return;
    }

    if (
      gamesHubOpen &&
      gamesHubDisplayGame &&
      ((topCategory === "all" && gameSubcategory !== "root") || topCategory === "current")
    ) {
      const n = gamesHubTiles.length;
      if (n === 0) return;
      if (direction === "left") {
        setGamesHubFocusIndex((i: number) => Math.max(0, i - 1));
        playUiSound("move");
        return;
      }
      if (direction === "right") {
        setGamesHubFocusIndex((i: number) => Math.min(n - 1, i + 1));
        playUiSound("move");
        return;
      }
      return;
    }

    if (ps5Row === "top") {
      if (direction === "left") {
        cycleTopCategory(-1);
        return;
      }
      if (direction === "right") {
        cycleTopCategory(1);
        return;
      }
      if (direction === "down") {
        playUiSound("move");
        setPs5Row("main");
        if (topCategory === "all" && gameSubcategory === "root" && gamesDualShelf) {
          setGamesRootPlane("spotlight");
        }
        if (topCategory === "current" && homeDualShelf) {
          setHomeRootPlane("spotlight");
        }
        return;
      }
      return;
    }

    if (ps5Row === "detail") {
      if (!canEnterDetailRow || detailRailItems.length === 0) {
        setPs5Row("main");
        return;
      }
      if (direction === "up") {
        playUiSound("move");
        setPs5Row("main");
        return;
      }
      if (direction === "left") {
        const next = Math.max(0, detailRailIndex - 1);
        if (next !== detailRailIndex) {
          playUiSound("move");
          setDetailRailIndex(next);
        }
        return;
      }
      if (direction === "right") {
        const next = Math.min(detailRailItems.length - 1, detailRailIndex + 1);
        if (next !== detailRailIndex) {
          playUiSound("move");
          setDetailRailIndex(next);
        }
        return;
      }
      return;
    }

    const shelfHasGames = categorizedGames.length > 0;
    if (gamesShelfBrowseActive) {
      if (shelfHasGames) {
        if (direction === "down") {
          if (selectedGame) {
            playUiSound("move");
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
          }
          return;
        }
        if (direction === "left") {
          const ni = Math.max(0, selectedIndex - 1);
          if (ni !== selectedIndex) {
            playUiSound("move");
            throttledOnSelectGame(categorizedGames[ni].id);
          }
          return;
        }
        if (direction === "right") {
          const ni = Math.min(categorizedGames.length - 1, selectedIndex + 1);
          if (ni !== selectedIndex) {
            playUiSound("move");
            throttledOnSelectGame(categorizedGames[ni].id);
          }
          return;
        }
        if (direction === "up") {
          if (canEnterTopRow) {
            playUiSound("move");
            setPs5Row("top");
          }
          return;
        }
      } else if (direction === "up") {
        if (canEnterTopRow) {
          playUiSound("move");
          setPs5Row("top");
        }
        return;
      }
    }

    if (mediaShelfBrowseActive) {
      if (direction === "down") {
        if (canEnterDetailRow && detailRailItems.length > 0) {
          playUiSound("move");
          setPs5Row("detail");
        }
        return;
      }
      const itemCount = mediaAssetItems.length;
      if (itemCount > 0 && direction === "left") {
        const nextIndex = Math.max(0, selectedMediaIndex - 1);
        if (nextIndex !== selectedMediaIndex) {
          playUiSound("move");
          setSelectedMediaIndex(nextIndex);
        }
        return;
      }
      if (itemCount > 0 && direction === "right") {
        const nextIndex = Math.min(itemCount - 1, selectedMediaIndex + 1);
        if (nextIndex !== selectedMediaIndex) {
          playUiSound("move");
          setSelectedMediaIndex(nextIndex);
        }
        return;
      }
      if (direction === "up") {
        if (canEnterTopRow) {
          playUiSound("move");
          setPs5Row("top");
        }
        return;
      }
    }

    if (topLevelRowBehaviorActive) {
      const isGamesRoot = topCategory === "all" && gameSubcategory === "root";
      const isHomeDual = topCategory === "current" && homeDualShelf;
      const itemCount = displayItems.length;
      if (isHomeDual && homeRootPlane === "spotlight" && (direction === "left" || direction === "right")) {
        const delta = direction === "left" ? -1 : 1;
        const next = Math.max(0, Math.min(spotlightEntries.length - 1, spotlightIndex + delta));
        if (next !== spotlightIndex) {
          playUiSound("move");
          setSpotlightIndex(next);
        }
        return;
      }
      if (isGamesRoot && gamesDualShelf && gamesRootPlane === "spotlight" && (direction === "left" || direction === "right")) {
        const delta = direction === "left" ? -1 : 1;
        const next = Math.max(0, Math.min(spotlightEntries.length - 1, spotlightIndex + delta));
        if (next !== spotlightIndex) {
          playUiSound("move");
          setSpotlightIndex(next);
        }
        return;
      }

      if (itemCount > 0 && (direction === "left" || direction === "right")) {
        const delta = direction === "left" ? -1 : 1;
        const next = Math.max(0, Math.min(itemCount - 1, topLevelShelfIndex + delta));
        if (next !== topLevelShelfIndex) {
          playUiSound("move");
          if (topCategory === "media") setSelectedMediaIndex(next);
          else if (topCategory === "all") setSelectedGameSubcategoryIndex(next);
          else setSelectedSettingIndex(next);
        }
        return;
      }

      if (direction === "up" || direction === "down") {
        if (isHomeDual) {
          if (direction === "up") {
            if (homeRootPlane === "actions") {
              playUiSound("move");
              setHomeRootPlane("spotlight");
              return;
            }
            if (homeRootPlane === "spotlight" && canEnterTopRow) {
              playUiSound("move");
              setPs5Row("top");
              return;
            }
          }
          if (direction === "down" && homeRootPlane === "spotlight") {
            playUiSound("move");
            setHomeRootPlane("actions");
            return;
          }
        }
        if (isGamesRoot && gamesDualShelf) {
          if (direction === "up") {
            if (gamesRootPlane === "categories") {
              playUiSound("move");
              setGamesRootPlane("spotlight");
              return;
            }
            if (gamesRootPlane === "spotlight" && canEnterTopRow) {
              playUiSound("move");
              setPs5Row("top");
              return;
            }
          }
          if (direction === "down" && gamesRootPlane === "spotlight") {
            playUiSound("move");
            setGamesRootPlane("categories");
            return;
          }
        }
        if (direction === "up" && canEnterTopRow) {
          playUiSound("move");
          setPs5Row("top");
          return;
        }
        if (direction === "down" && canEnterDetailRow && detailRailItems.length > 0) {
          playUiSound("move");
          setPs5Row("detail");
          return;
        }
        return;
      }
    }

    if (topCategory === "settings" && settingsSubcategory !== "root" && (direction === "left" || direction === "right")) {
      const itemCount = displayItems.length;
      if (itemCount === 0) return;
      const delta = direction === "left" ? -1 : 1;
      const nextIndex = Math.max(0, Math.min(itemCount - 1, selectedSettingIndex + delta));
      if (nextIndex !== selectedSettingIndex) {
        playUiSound("move");
        setSelectedSettingIndex(nextIndex);
      }
      return;
    }

    if (direction === "left") {
      playUiSound("move");
      setCategoryIndex((prev: number) => (prev - 1 + TOP_CATEGORIES.length) % TOP_CATEGORIES.length);
      setSelectedSettingIndex(0);
      setSettingsSubcategory("root");
      setSelectedMediaIndex(0);
      setMediaSubcategory("root");
      setSelectedGameSubcategoryIndex(0);
      setGameSubcategory("root");
      setHomeRootPlane("spotlight");
      setEditingBandwidth(false);
      setEditingThemeChannel(null);
      return;
    }
    if (direction === "right") {
      playUiSound("move");
      setCategoryIndex((prev: number) => (prev + 1) % TOP_CATEGORIES.length);
      setSelectedSettingIndex(0);
      setSettingsSubcategory("root");
      setSelectedMediaIndex(0);
      setMediaSubcategory("root");
      setSelectedGameSubcategoryIndex(0);
      setGameSubcategory("root");
      setHomeRootPlane("spotlight");
      setEditingBandwidth(false);
      setEditingThemeChannel(null);
      return;
    }
    if (topCategory === "current" || topCategory === "settings") {
      if (direction === "up") {
        const nextIndex = Math.max(0, selectedSettingIndex - 1);
        if (nextIndex !== selectedSettingIndex) {
          playUiSound("move");
          setSelectedSettingIndex(nextIndex);
          if (topCategory === "current" && inStreamMenu) setEditingStreamVolume(false);
          if (topCategory === "current" && inStreamMenu) setEditingStreamMicLevel(false);
        }
        return;
      }
      if (direction === "down") {
        const nextIndex = Math.min(displayItems.length - 1, selectedSettingIndex + 1);
        if (nextIndex !== selectedSettingIndex) {
          playUiSound("move");
          setSelectedSettingIndex(nextIndex);
          if (topCategory === "current" && inStreamMenu) setEditingStreamVolume(false);
          if (topCategory === "current" && inStreamMenu) setEditingStreamMicLevel(false);
        }
        return;
      }
      return;
    }
    if (topCategory === "media" && mediaSubcategory === "root") {
      const itemCount = mediaSubcategory === "root" ? displayItems.length : mediaAssetItems.length;
      if (itemCount === 0) return;
      if (direction === "up") {
        const nextIndex = Math.max(0, selectedMediaIndex - 1);
        if (nextIndex !== selectedMediaIndex) {
          playUiSound("move");
          setSelectedMediaIndex(nextIndex);
        }
        return;
      }
      if (direction === "down") {
        const nextIndex = Math.min(itemCount - 1, selectedMediaIndex + 1);
        if (nextIndex !== selectedMediaIndex) {
          playUiSound("move");
          setSelectedMediaIndex(nextIndex);
        }
        return;
      }
      return;
    }
    if (topCategory === "all" && gameSubcategory === "root") {
      const itemCount = displayItems.length;
      if (itemCount === 0) return;
      if (direction === "up") {
        const nextIndex = Math.max(0, selectedGameSubcategoryIndex - 1);
        if (nextIndex !== selectedGameSubcategoryIndex) {
          playUiSound("move");
          setSelectedGameSubcategoryIndex(nextIndex);
        }
        return;
      }
      if (direction === "down") {
        const nextIndex = Math.min(itemCount - 1, selectedGameSubcategoryIndex + 1);
        if (nextIndex !== selectedGameSubcategoryIndex) {
          playUiSound("move");
          setSelectedGameSubcategoryIndex(nextIndex);
        }
        return;
      }
    }
  };

  const onDirection = (e: any): void => {
    if (e.detail?.direction) applyDirection(e.detail.direction);
  };

  const onShoulder = (e: any): void => {
    const direction = e?.detail?.direction as "prev" | "next" | undefined;
    if (!direction) return;
    if (optionsOpen) return;
    if (localVideoPlayerOpen) return;
    if (gamesHubOpen) return;
    if (topCategory === "settings" && settingsSubcategory !== "root") return;
    if (editingBandwidth || editingThemeChannel || editingStreamVolume || editingStreamMicLevel) return;
    cycleTopCategory(direction === "prev" ? -1 : 1);
  };

  return { applyDirection, cycleTopCategory, onDirection, onShoulder };
}
