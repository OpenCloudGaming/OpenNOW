import { useEffect, useLayoutEffect, useState } from "react";
import type { CSSProperties, RefObject } from "react";
import { computeShelfTranslateXClamped, sanitizeControllerThemeStyle } from "./helpers";
import type { GameSubcategory, MediaSubcategory, SettingsSubcategory, TopCategory } from "./types";

type UseControllerLibraryLayoutMotionArgs = {
  topCategory: TopCategory;
  gameSubcategory: GameSubcategory;
  mediaSubcategory: MediaSubcategory;
  settingsSubcategory: SettingsSubcategory;
  gamesShelfBrowseActive: boolean;
  mediaShelfBrowseActive: boolean;
  topLevelShelfActive: boolean;
  topLevelShelfIndex: number;
  selectedIndex: number;
  selectedMediaIndex: number;
  gamesDualShelf: boolean;
  homeDualShelf: boolean;
  spotlightIndex: number;
  spotlightEntriesLength: number;
  itemsContainerRef: RefObject<HTMLDivElement | null>;
  spotlightTrackRef: RefObject<HTMLDivElement | null>;
  focusMotionKey: string;
  settings: {
    controllerThemeStyle?: string;
    controllerThemeColor?: { r: number; g: number; b: number };
    controllerBackgroundAnimations?: boolean;
  };
  ps5Row: "top" | "main" | "detail";
};

type UseControllerLibraryLayoutMotionResult = {
  isEntering: boolean;
  listTranslateY: number;
  listTranslateX: number;
  spotlightShelfTranslateX: number;
  gamesRootMenuTranslateX: number;
  heroTransitionMs: number;
  wrapperThemeVars: CSSProperties;
  wrapperClassNameWithRow: string;
  menuShelfTranslateX: number;
};

export function useControllerLibraryLayoutMotion({
  topCategory,
  gameSubcategory,
  mediaSubcategory,
  settingsSubcategory,
  gamesShelfBrowseActive,
  mediaShelfBrowseActive,
  topLevelShelfActive,
  topLevelShelfIndex,
  selectedIndex,
  selectedMediaIndex,
  gamesDualShelf,
  homeDualShelf,
  spotlightIndex,
  spotlightEntriesLength,
  itemsContainerRef,
  spotlightTrackRef,
  focusMotionKey,
  settings,
  ps5Row,
}: UseControllerLibraryLayoutMotionArgs): UseControllerLibraryLayoutMotionResult {
  const [isEntering, setIsEntering] = useState(true);
  const [listTranslateY, setListTranslateY] = useState(0);
  const [listTranslateX, setListTranslateX] = useState(0);
  const [spotlightShelfTranslateX, setSpotlightShelfTranslateX] = useState(0);
  const [gamesRootMenuTranslateX, setGamesRootMenuTranslateX] = useState(0);
  const [viewportWidth, setViewportWidth] = useState(() => (typeof window === "undefined" ? 1200 : window.innerWidth));
  const [heroTransitionMs, setHeroTransitionMs] = useState(420);

  useEffect(() => {
    if (typeof window === "undefined") {
      setIsEntering(false);
      return;
    }
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      setIsEntering(false);
      return;
    }
    const timeoutId = window.setTimeout(() => setIsEntering(false), 760);
    return () => window.clearTimeout(timeoutId);
  }, []);

  useEffect(() => {
    if (!gamesShelfBrowseActive && !mediaShelfBrowseActive && !topLevelShelfActive) {
      setListTranslateX(0);
      setSpotlightShelfTranslateX(0);
      setGamesRootMenuTranslateX(0);
    }
  }, [gamesShelfBrowseActive, mediaShelfBrowseActive, topLevelShelfActive]);

  useEffect(() => {
    if (typeof window === "undefined") return;
    let raf = 0;
    const onResize = () => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => setViewportWidth(window.innerWidth));
    };
    window.addEventListener("resize", onResize);
    return () => {
      cancelAnimationFrame(raf);
      window.removeEventListener("resize", onResize);
    };
  }, []);

  useLayoutEffect(() => {
    const gamesRoot = topCategory === "all" && gameSubcategory === "root";
    const homeDualRoot = topCategory === "current" && homeDualShelf;
    const dualShelfActive = (gamesRoot && gamesDualShelf) || homeDualRoot;
    if (!dualShelfActive) {
      setSpotlightShelfTranslateX(0);
      setGamesRootMenuTranslateX(0);
    }
    if (dualShelfActive) {
      setSpotlightShelfTranslateX(computeShelfTranslateXClamped(spotlightTrackRef.current, spotlightIndex));
      setGamesRootMenuTranslateX(computeShelfTranslateXClamped(itemsContainerRef.current, topLevelShelfIndex));
      setListTranslateY(0);
      return;
    }

    const container = itemsContainerRef.current;
    if (!container) return;
    const children = Array.from(container.children) as HTMLElement[];
    const activeIndex = gamesShelfBrowseActive ? selectedIndex : mediaShelfBrowseActive ? selectedMediaIndex : topLevelShelfIndex;
    if (children.length === 0 || activeIndex >= children.length) {
      if (gamesShelfBrowseActive || mediaShelfBrowseActive || topLevelShelfActive) setListTranslateX(0);
      return;
    }
    if (gamesShelfBrowseActive || mediaShelfBrowseActive || topLevelShelfActive) {
      setListTranslateX(computeShelfTranslateXClamped(container, activeIndex));
      setListTranslateY(0);
      return;
    }
    const activeChild = children[selectedIndex];
    const offset = activeChild.offsetTop + activeChild.offsetHeight / 2;
    setListTranslateY(-offset);
    setListTranslateX(0);
  }, [
    selectedIndex,
    gamesShelfBrowseActive,
    mediaShelfBrowseActive,
    topLevelShelfActive,
    topLevelShelfIndex,
    selectedMediaIndex,
    viewportWidth,
    topCategory,
    gameSubcategory,
    gamesDualShelf,
    homeDualShelf,
    spotlightIndex,
    spotlightEntriesLength,
    itemsContainerRef,
    spotlightTrackRef,
  ]);

  useEffect(() => {
    setHeroTransitionMs(200);
    const t = window.setTimeout(() => setHeroTransitionMs(420), 420);
    return () => window.clearTimeout(t);
  }, [focusMotionKey]);

  const themeStyleSafe = sanitizeControllerThemeStyle(settings.controllerThemeStyle);
  const themeRgbResolved = settings.controllerThemeColor ?? { r: 124, g: 241, b: 177 };
  const wrapperThemeVars = {
    "--xmb-theme-r": String(themeRgbResolved.r),
    "--xmb-theme-g": String(themeRgbResolved.g),
    "--xmb-theme-b": String(themeRgbResolved.b),
    "--xmb-hero-crossfade-ms": `${heroTransitionMs}ms`,
  } as CSSProperties;
  const wrapperClassName = `xmb-wrapper xmb-theme-${themeStyleSafe} ${settings.controllerBackgroundAnimations ? "xmb-animate" : "xmb-static"} ${isEntering ? "xmb-entering" : "xmb-ready"} xmb-layout--ps5-home`;
  const wrapperClassNameWithRow = `${wrapperClassName} xmb-row-${ps5Row} ${topCategory === "settings" ? "xmb-ps5-section-settings" : ""} ${topCategory === "settings" && settingsSubcategory === "root" ? "xmb-ps5-settings-root" : ""} ${topCategory === "settings" && settingsSubcategory !== "root" ? "xmb-ps5-settings-sub" : ""}`;
  const menuShelfTranslateX = gamesDualShelf || homeDualShelf ? gamesRootMenuTranslateX : listTranslateX;

  return {
    isEntering,
    listTranslateY,
    listTranslateX,
    spotlightShelfTranslateX,
    gamesRootMenuTranslateX,
    heroTransitionMs,
    wrapperThemeVars,
    wrapperClassNameWithRow,
    menuShelfTranslateX,
  };
}
