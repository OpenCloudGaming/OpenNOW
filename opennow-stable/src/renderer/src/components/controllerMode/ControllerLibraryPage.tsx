import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { JSX } from "react";
import type { GameInfo } from "@shared/gfn";
import { Star, Clock, Calendar, Repeat2 } from "lucide-react";
import { ButtonA, ButtonX, ButtonY, ButtonPSCross, ButtonPSSquare, ButtonPSTriangle } from "./ControllerButtons";
import { getStoreDisplayName } from "../GameCard";
import { SessionElapsedIndicator } from "../ElapsedSessionIndicators";
import { formatPlaytime, formatLastPlayed } from "../../utils/usePlaytime";
import { playControllerUiSound } from "../../utils/controllerUiSound";
import {
  type ControllerOverlayNavSnapshot,
  readControllerOverlayNav,
  writeControllerOverlayNav,
} from "../../utils/controllerOverlayNavStorage";
import {
  CONTROLLER_THEME_STYLE_ORDER,
  CONTROLLER_THEME_STYLE_LABEL,
  LIBRARY_SORT_STORAGE_KEY,
} from "./controllerLibrary/constants";
import {
  clampRgbByte,
  getCategoryIcon,
  getCategoryLabel,
  isEditableTarget,
  isWithinContentWindow,
  isWithinImageWindow,
  readLibrarySortId,
  sanitizeControllerThemeStyle,
  spotlightEntryHasGame,
} from "./controllerLibrary/helpers";
import { ControllerLibraryLayout } from "./controllerLibrary/ControllerLibraryLayout";
import { loadScreenshotUrlsForGameTitle } from "./controllerLibrary/loadGameScreenshotUrls";
import { TopLevelMenuTrack } from "./controllerLibrary/TopLevelMenuTrack";
import { useControllerLibraryGameDerivations } from "./controllerLibrary/useControllerLibraryGameDerivations";
import { useControllerLibraryEvents } from "./controllerLibrary/useControllerLibraryEvents";
import { useControllerLibraryLayoutMotion } from "./controllerLibrary/useControllerLibraryLayoutMotion";
import { LocalVideoPlayerOverlay } from "./controllerLibrary/LocalVideoPlayerOverlay";
import { useControllerLibraryMedia } from "./controllerLibrary/useControllerLibraryMedia";
import { useLocalVideoPlayback } from "./controllerLibrary/useLocalVideoPlayback";
import { useControllerWindowBindings } from "./controllerLibrary/useControllerWindowBindings";
import {
  routeCancel,
  routeCategoryActivate,
  routeOpenOptions,
  routeOptionsActivate,
  routeSecondaryActivate,
} from "./controllerLibrary/actions/actionRouter";
import type {
  ControllerLibraryPageProps,
  Direction,
  GameSubcategory,
  GamesHubReturnSnapshot,
  HomeRootPlane,
  LibrarySortId,
  MediaSubcategory,
  SettingsSubcategory,
  SoundKind,
  TopCategory,
} from "./controllerLibrary/types";


export function ControllerLibraryPage({
  games,
  isLoading,
  selectedGameId,
  selectedVariantByGameId,
  uiSoundsEnabled,
  favoriteGameIds,
  onSelectGame,
  onSelectGameVariant,
  onToggleFavoriteGame,
  onPlayGame,
  onOpenSettings,
  currentStreamingGame,
  onResumeGame,
  onCloseGame,
  onExitApp,
  userName = "Player One",
  userAvatarUrl,
  subscriptionInfo,
  playtimeData = {},
  settings = {},
  resolutionOptions = [],
  fpsOptions = [],
  codecOptions = [],
  aspectRatioOptions = [],
  onSettingChange,
  onExitControllerMode,
  sessionStartedAtMs = null,
  isStreaming = false,
  inStreamMenu = false,
  streamMenuVolume = 1,
  onStreamMenuVolumeChange,
  streamMenuMicLevel = 1,
  onStreamMenuMicLevelChange,
  streamMicTrack = null,
  onStreamMenuToggleMicrophone,
  onStreamMenuToggleFullscreen,
  streamMenuMicOn = false,
  streamMenuIsFullscreen = false,
  cloudSessionResumable = false,
  cloudResumeTitle = null,
  cloudResumeCoverUrl = null,
  onResumeCloudSession,
  cloudResumeBusy = false,
}: ControllerLibraryPageProps): JSX.Element {
  /** Top strip: Home (current / last played) is always the default landing tab. */
  const initialCategoryIndex = 0;
  const [categoryIndex, setCategoryIndex] = useState(initialCategoryIndex);
  const [endSessionConfirm, setEndSessionConfirm] = useState(false);
  const [editingStreamVolume, setEditingStreamVolume] = useState(false);
  const [editingStreamMicLevel, setEditingStreamMicLevel] = useState(false);
  const itemsContainerRef = useRef<HTMLDivElement>(null);
  const overlayNavWriteRef = useRef<ControllerOverlayNavSnapshot | null>(null);
  const overlayNavRestoredRef = useRef(false);
  const [selectedSettingIndex, setSelectedSettingIndex] = useState(0);
  const [microphoneDevices, setMicrophoneDevices] = useState<{ deviceId: string; label: string }[]>([]);
  const [settingsSubcategory, setSettingsSubcategory] = useState<SettingsSubcategory>("root");
  const [lastRootSettingIndex, setLastRootSettingIndex] = useState(0);
  const [mediaSubcategory, setMediaSubcategory] = useState<MediaSubcategory>("root");
  const [lastRootMediaIndex, setLastRootMediaIndex] = useState(0);
  const [selectedMediaIndex, setSelectedMediaIndex] = useState(0);
  const [gameSubcategory, setGameSubcategory] = useState<GameSubcategory>("root");
  const [lastRootGameIndex, setLastRootGameIndex] = useState(0);
  const [selectedGameSubcategoryIndex, setSelectedGameSubcategoryIndex] = useState(0);
  const [controllerType, setControllerType] = useState<"ps" | "xbox" | "nintendo" | "generic">("generic");
  const [editingBandwidth, setEditingBandwidth] = useState(false);
  const [lastSystemMenuIndex, setLastSystemMenuIndex] = useState(0);
  const [lastThemeRootIndex, setLastThemeRootIndex] = useState(0);
  const [editingThemeChannel, setEditingThemeChannel] = useState<null | "r" | "g" | "b">(null);
  const [ps5Row, setPs5Row] = useState<"top" | "main" | "detail">("main");
  const [detailRailIndex, setDetailRailIndex] = useState(0);
  const [librarySortId, setLibrarySortId] = useState<LibrarySortId>(() => readLibrarySortId());
  const [gamesRootPlane, setGamesRootPlane] = useState<"spotlight" | "categories">("categories");
  const [homeRootPlane, setHomeRootPlane] = useState<HomeRootPlane>("spotlight");
  const [spotlightIndex, setSpotlightIndex] = useState(0);
  const [optionsOpen, setOptionsOpen] = useState(false);
  const [optionsEntries, setOptionsEntries] = useState<Array<{ id: string; label: string }>>([]);
  const [optionsFocusIndex, setOptionsFocusIndex] = useState(0);
  const [gamesHubOpen, setGamesHubOpen] = useState(false);
  const [gamesHubFocusIndex, setGamesHubFocusIndex] = useState(0);
  const [mediaListRefreshNonce, setMediaListRefreshNonce] = useState(0);
  /** Local captures for the focused game; loaded when hub opens so Media tab need not be visited first */
  const [gameHubScreenshotUrls, setGameHubScreenshotUrls] = useState<string[]>([]);
  /** Random capture for Home last-played resume tile; falls back to poster in TopLevelMenuTrack */
  const [homeResumeSnapshotUrl, setHomeResumeSnapshotUrl] = useState<string | null>(null);
  const gamesHubReturnSnapshotRef = useRef<GamesHubReturnSnapshot | null>(null);
  const spotlightTrackRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    try {
      sessionStorage.setItem(LIBRARY_SORT_STORAGE_KEY, librarySortId);
    } catch {
    }
  }, [librarySortId]);

  useEffect(() => {
    const detectTypeFromGamepad = (g: Gamepad | null): "ps" | "xbox" | "nintendo" | "generic" => {
      if (!g || !g.id) return "generic";
      const id = g.id.toLowerCase();
      if (id.includes("wireless controller") || id.includes("dualshock") || id.includes("dualsense") || id.includes("054c")) return "ps";
      if (id.includes("xbox") || id.includes("x-input") || id.includes("xinput") || id.includes("xusb")) return "xbox";
      if (id.includes("nintendo") || id.includes("pro controller") || id.includes("joy-con") || id.includes("joycon")) return "nintendo";
      return "generic";
    };

    const updateFromConnected = () => {
      try {
        const pads = navigator.getGamepads ? navigator.getGamepads() : [];
        for (const p of pads) {
          if (p && p.connected) {
            setControllerType(detectTypeFromGamepad(p));
            return;
          }
        }
        setControllerType("generic");
      } catch {
        setControllerType("generic");
      }
    };

    window.addEventListener("gamepadconnected", updateFromConnected);
    window.addEventListener("gamepaddisconnected", updateFromConnected);
    updateFromConnected();
    return () => {
      window.removeEventListener("gamepadconnected", updateFromConnected);
      window.removeEventListener("gamepaddisconnected", updateFromConnected);
    };
  }, []);

  const formatTime = (date: Date) => {
    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  };

  const formatElapsed = (totalSeconds: number) => {
    const safe = Math.max(0, Math.floor(totalSeconds));
    const hours = Math.floor(safe / 3600);
    const minutes = Math.floor((safe % 3600) / 60);
    const seconds = safe % 60;
    if (hours > 0) return `${hours}:${minutes.toString().padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`;
    return `${minutes}:${seconds.toString().padStart(2, '0')}`;
  };

  const playUiSound = useCallback((kind: SoundKind): void => {
    playControllerUiSound(kind, uiSoundsEnabled);
  }, [uiSoundsEnabled]);

  const {
    playback: localVideoPlayback,
    localVideoPlayerOpen,
    openFromEntry: openLocalVideoPlayer,
    close: closeLocalVideoPlayer,
  } = useLocalVideoPlayback(playUiSound);

  const bumpMediaListRefresh = useCallback((): void => {
    setMediaListRefreshNonce((n) => n + 1);
  }, []);

  const lastPlayedGame = useMemo((): GameInfo | null => {
    const lastPlayedMs = (gameId: string) => {
      const raw = playtimeData[gameId]?.lastPlayedAt;
      if (!raw) return 0;
      const ms = Date.parse(raw);
      return Number.isFinite(ms) ? ms : 0;
    };
    const played = games.filter((g) => lastPlayedMs(g.id) > 0);
    if (played.length === 0) return null;
    played.sort((a, b) => {
      const d = lastPlayedMs(b.id) - lastPlayedMs(a.id);
      if (d !== 0) return d;
      return a.title.localeCompare(b.title);
    });
    return played[0] ?? null;
  }, [games, playtimeData]);

  const currentTabGame = currentStreamingGame ?? lastPlayedGame;

  const TOP_CATEGORIES = useMemo((): Array<{ id: TopCategory; label: string }> => {
    return [
      { id: "current", label: "Home" },
      { id: "all", label: "Games" },
      { id: "media", label: "Media" },
      { id: "settings", label: "Settings" },
    ];
  }, []);

  const topCategory = (TOP_CATEGORIES[categoryIndex]?.id ?? "all") as TopCategory;
  const {
    mediaLoading,
    mediaError,
    mediaThumbById,
    mediaAssetItems,
    selectedMediaItem,
    mediaHubSlots,
    mediaHubPlaceholderCount,
  } = useControllerLibraryMedia({
    topCategory,
    mediaSubcategory,
    selectedMediaIndex,
    mediaListRefreshNonce,
  });

  useEffect(() => {
    if (topCategory !== "media" || mediaSubcategory === "root") return;
    const len = mediaAssetItems.length;
    if (len === 0) {
      if (selectedMediaIndex !== 0) setSelectedMediaIndex(0);
      return;
    }
    if (selectedMediaIndex >= len) setSelectedMediaIndex(len - 1);
  }, [topCategory, mediaSubcategory, mediaAssetItems.length, selectedMediaIndex]);

  const {
    favoriteGameIdSet,
    allGenres,
    currentGameItems,
    mediaRootItems,
    gameRootItems,
    categorizedGames,
    spotlightEntries,
    gameCategoryPreviewById,
    parallaxBackdropTiles,
    selectedIndex,
    selectedGame,
    selectedVariantId,
    featuredHomeGame,
  } = useControllerLibraryGameDerivations({
    games,
    favoriteGameIds,
    playtimeData,
    topCategory,
    currentStreamingGame,
    homeShelfGameTitle: currentTabGame?.title ?? null,
    resumeContextGameId: currentTabGame?.id ?? null,
    gameSubcategory,
    selectedGameId,
    selectedVariantByGameId,
    cloudSessionResumable,
    cloudResumeTitle,
    cloudResumeCoverUrl,
    cloudResumeBusy,
    onResumeCloudSession,
    inStreamMenu,
    streamMenuMicOn,
    streamMenuMicLevel,
    streamMenuVolume,
    streamMenuIsFullscreen,
    endSessionConfirm,
    librarySortId,
  });

  useEffect(() => {
    if (TOP_CATEGORIES.length === 0) return;
    setCategoryIndex((prev) => Math.max(0, Math.min(prev, TOP_CATEGORIES.length - 1)));
  }, [TOP_CATEGORIES.length]);

  useLayoutEffect(() => {
    if (!inStreamMenu) {
      overlayNavRestoredRef.current = false;
      return;
    }
    if (overlayNavRestoredRef.current) return;
    overlayNavRestoredRef.current = true;
    const snap = readControllerOverlayNav();
    if (!snap) return;
    const maxCat = Math.max(0, TOP_CATEGORIES.length - 1);
    setCategoryIndex(Math.max(0, Math.min(snap.categoryIndex, maxCat)));
    setGameSubcategory(snap.gameSubcategory as GameSubcategory);
    setMediaSubcategory(snap.mediaSubcategory as MediaSubcategory);
    setSettingsSubcategory(snap.settingsSubcategory as SettingsSubcategory);
    setGamesRootPlane(snap.gamesRootPlane);
    setSpotlightIndex(snap.spotlightIndex);
    setSelectedGameSubcategoryIndex(snap.selectedGameSubcategoryIndex);
    setSelectedSettingIndex(snap.selectedSettingIndex);
    setSelectedMediaIndex(snap.selectedMediaIndex);
    setPs5Row(snap.ps5Row);
  }, [inStreamMenu, TOP_CATEGORIES.length]);

  useEffect(() => {
    if (!inStreamMenu) {
      overlayNavWriteRef.current = null;
      return;
    }
    overlayNavWriteRef.current = {
      categoryIndex,
      gameSubcategory: gameSubcategory as string,
      mediaSubcategory: mediaSubcategory as string,
      settingsSubcategory: settingsSubcategory as string,
      gamesRootPlane,
      spotlightIndex,
      selectedGameSubcategoryIndex,
      selectedSettingIndex,
      selectedMediaIndex,
      ps5Row,
    };
  }, [
    inStreamMenu,
    categoryIndex,
    gameSubcategory,
    mediaSubcategory,
    settingsSubcategory,
    gamesRootPlane,
    spotlightIndex,
    selectedGameSubcategoryIndex,
    selectedSettingIndex,
    selectedMediaIndex,
    ps5Row,
  ]);

  useEffect(() => {
    if (!inStreamMenu) return;
    return () => {
      const snap = overlayNavWriteRef.current;
      if (snap) writeControllerOverlayNav(snap);
    };
  }, [inStreamMenu]);

  useEffect(() => {
    const onNav = (ev: Event): void => {
      const ce = ev as CustomEvent<{ target?: string }>;
      if (ce.detail?.target !== "media") return;
      const mediaIdx = TOP_CATEGORIES.findIndex((c) => c.id === "media");
      if (mediaIdx >= 0) setCategoryIndex(mediaIdx);
      setMediaSubcategory("root");
      setSelectedMediaIndex(0);
      setPs5Row("main");
      setGamesHubOpen(false);
      setGameSubcategory("root");
      setEndSessionConfirm(false);
      playUiSound("move");
    };
    window.addEventListener("opennow:controller-navigate", onNav as EventListener);
    return () => window.removeEventListener("opennow:controller-navigate", onNav as EventListener);
  }, [TOP_CATEGORIES, playUiSound]);

  const settingsBySubcategory = useMemo(() => {
    const micLabel = (() => {
      const id = (settings as any).microphoneDeviceId as string | undefined;
      if (!id) return "Default";
      const found = microphoneDevices.find(d => d.deviceId === id);
      return found?.label ?? id;
    })();

    const themeRgb = settings.controllerThemeColor ?? { r: 124, g: 241, b: 177 };
    const themeStyleResolved = sanitizeControllerThemeStyle(settings.controllerThemeStyle);

    return {
      root: [
        { id: "network", label: "Network", value: "" },
        { id: "audio", label: "Audio", value: "" },
        { id: "video", label: "Video", value: "" },
        { id: "system", label: "System", value: "" },
        { id: "exitApp", label: "Exit", value: "" },
      ],
      Network: [
        { id: "bandwidth", label: "Max Bitrate", value: `${(settings.maxBitrateMbps ?? 75)} Mbps` },
        { id: "l4s", label: "Experimental L4S", value: settings.enableL4S ? "On" : "Off" },
        { id: "cloudGsync", label: "Cloud G-Sync (VRR)", value: settings.enableCloudGsync ? "On" : "Off" },
      ],
      Video: [
        { id: "resolution", label: "Resolution", value: settings.resolution || "1920x1080" },
        { id: "aspectRatio", label: "Aspect Ratio", value: settings.aspectRatio || "16:9" },
        { id: "fps", label: "Frame Rate", value: `${settings.fps || 60} FPS` },
        { id: "codec", label: "Video Codec", value: settings.codec || "H264" },
      ],
      Audio: [
        { id: "microphone", label: "Microphone", value: micLabel },
        { id: "sounds", label: "UI Sounds", value: settings.controllerUiSounds ? "On" : "Off" },
      ],
      System: [
        { id: "autoFullScreen", label: "Auto Full Screen", value: (settings as any).autoFullScreen ? "On" : "Off" },
        { id: "autoLoad", label: "Auto-Load Library", value: (settings as any).autoLoadControllerLibrary ? "On" : "Off" },
        { id: "backgroundAnimations", label: "Background Animations", value: ((settings as any).controllerBackgroundAnimations ? "On" : "Off") },
        { id: "theme", label: "Theme", value: "" },
        { id: "exitControllerMode", label: "Exit Controller Mode", value: "" },
      ],
      Theme: [
        { id: "themeColor", label: "Color", value: `RGB ${themeRgb.r}, ${themeRgb.g}, ${themeRgb.b}` },
        { id: "themeStyle", label: "Style", value: CONTROLLER_THEME_STYLE_LABEL[themeStyleResolved] },
        {
          id: "libraryGameBackdrop",
          label: "Match background to game",
          value: (settings.controllerLibraryGameBackdrop !== false) ? "On" : "Off",
        },
      ],
      ThemeColor: [
        { id: "themeR", label: "Red", value: `${themeRgb.r}` },
        { id: "themeG", label: "Green", value: `${themeRgb.g}` },
        { id: "themeB", label: "Blue", value: `${themeRgb.b}` },
      ],
      ThemeStyle: CONTROLLER_THEME_STYLE_ORDER.map((id) => ({
        id,
        label: CONTROLLER_THEME_STYLE_LABEL[id],
        value: id === themeStyleResolved ? "Active" : "",
      })),
    } as Record<string, Array<{ id: string; label: string; value: string }>>;
  }, [settings, microphoneDevices]);
 

  const displayItems = useMemo(() => {
    if (topCategory === "current") return currentGameItems;
    if (topCategory === "settings") return settingsBySubcategory[settingsSubcategory] ?? [];
    if (topCategory === "all" && gameSubcategory === "root") return gameRootItems;
    if (topCategory === "media" && mediaSubcategory === "root") return mediaRootItems;
    return [];
  }, [topCategory, currentGameItems, settingsBySubcategory, settingsSubcategory, gameSubcategory, gameRootItems, mediaSubcategory, mediaRootItems]);

  useEffect(() => {
    let mounted = true;
    if (!navigator.mediaDevices || !navigator.mediaDevices.enumerateDevices) return;
    navigator.mediaDevices.enumerateDevices().then(devs => {
      if (!mounted) return;
      const mics = devs
        .filter(d => d.kind === "audioinput")
        .map(d => ({ deviceId: d.deviceId, label: d.label || "Microphone" }));
      // Ensure there's at least a default entry
      if (mics.length === 0) mics.push({ deviceId: "", label: "Default" });
      setMicrophoneDevices(mics);
    }).catch(() => {
      if (!mounted) return;
      setMicrophoneDevices([{ deviceId: "", label: "Default" }]);
    });
    return () => { mounted = false; };
  }, []);


  useEffect(() => {
    if (spotlightEntries.length === 0) {
      setSpotlightIndex(0);
      return;
    }
    setSpotlightIndex((i) => Math.min(i, spotlightEntries.length - 1));
  }, [spotlightEntries.length]);

  const gamesHubDisplayGame = useMemo((): GameInfo | null => {
    if (!gamesHubOpen) return null;
    if (topCategory === "current") {
      return games.find((g) => g.id === selectedGameId) ?? null;
    }
    if (topCategory === "all" && gameSubcategory !== "root") {
      return selectedGame;
    }
    return null;
  }, [gamesHubOpen, topCategory, gameSubcategory, games, selectedGameId, selectedGame]);

  useEffect(() => {
    if (!gamesHubOpen || !gamesHubDisplayGame?.title?.trim()) {
      setGameHubScreenshotUrls([]);
      return;
    }

    let cancelled = false;
    void loadScreenshotUrlsForGameTitle(gamesHubDisplayGame.title).then((urls) => {
      if (!cancelled) setGameHubScreenshotUrls(urls);
    });

    return () => {
      cancelled = true;
    };
  }, [gamesHubOpen, gamesHubDisplayGame?.id, gamesHubDisplayGame?.title]);

  useEffect(() => {
    const game = currentTabGame;
    if (!game?.title?.trim()) {
      setHomeResumeSnapshotUrl(null);
      return;
    }

    let cancelled = false;
    void loadScreenshotUrlsForGameTitle(game.title).then((urls) => {
      if (cancelled) return;
      if (urls.length === 0) {
        setHomeResumeSnapshotUrl(null);
        return;
      }
      const pick = urls[Math.floor(Math.random() * urls.length)] ?? null;
      setHomeResumeSnapshotUrl(pick);
    });

    return () => {
      cancelled = true;
    };
  }, [currentTabGame?.id, currentTabGame?.title]);


  const gamesShelfBrowseActive = topCategory === "all" && gameSubcategory !== "root";
  const mediaShelfBrowseActive = topCategory === "media" && mediaSubcategory !== "root";
  const topLevelShelfActive =
    !gamesShelfBrowseActive &&
    !mediaShelfBrowseActive &&
    !(topCategory === "current" && gamesHubOpen) &&
    (topCategory === "settings" ||
      topCategory === "current" ||
      (topCategory === "media" && mediaSubcategory === "root") ||
      (topCategory === "all" && gameSubcategory === "root"));
  /** Games root: category row only (no Recently played / spotlight strip; that lives on Home). */
  const gamesDualShelf = false;
  const homeDualShelf =
    topCategory === "current" && !inStreamMenu && spotlightEntries.length > 0;

  const featuredIsFavorite = Boolean(featuredHomeGame && favoriteGameIdSet.has(featuredHomeGame.id));

  useEffect(() => {
    if (topCategory !== "current") return;
    if (!inStreamMenu) setHomeRootPlane("spotlight");
  }, [topCategory, inStreamMenu]);
  const topLevelRowBehaviorActive = topLevelShelfActive && !(topCategory === "settings" && settingsSubcategory !== "root");
  /** Media browse: no secondary “detail” row (down used to open Open folder / Media hub cards). */
  const canEnterDetailRow = false;
  const canEnterTopRow = topLevelRowBehaviorActive || gamesShelfBrowseActive || mediaShelfBrowseActive;
  const topLevelShelfIndex =
    topCategory === "media"
      ? selectedMediaIndex
      : topCategory === "all"
        ? selectedGameSubcategoryIndex
        : selectedSettingIndex;

  useEffect(() => {
    if (!inStreamMenu || !endSessionConfirm) return;
    if (topCategory !== "current") {
      setEndSessionConfirm(false);
      return;
    }
    const item = displayItems[topLevelShelfIndex];
    if (item?.id !== "closeGame") setEndSessionConfirm(false);
  }, [inStreamMenu, endSessionConfirm, topCategory, displayItems, topLevelShelfIndex]);

  const selectedCategoryLabel = useMemo(() => getCategoryLabel(topCategory).label, [topCategory]);
  const selectedTopLevelItemLabel = useMemo(() => {
    if (!topLevelShelfActive) return selectedCategoryLabel;
    if (topCategory === "current" && homeDualShelf && homeRootPlane === "spotlight") {
      const entry = spotlightEntries[spotlightIndex];
      if (entry?.kind === "cloudResume") return entry.title;
      if (spotlightEntryHasGame(entry)) return entry.game.title;
      return "Recently played";
    }
    if (topCategory === "all" && gameSubcategory === "root" && gamesDualShelf && gamesRootPlane === "spotlight") {
      const entry = spotlightEntries[spotlightIndex];
      if (entry?.kind === "cloudResume") return entry.title;
      if (spotlightEntryHasGame(entry)) return entry.game.title;
      return "Recently played";
    }
    const active = displayItems[topLevelShelfIndex];
    if (topCategory === "all" && gameSubcategory === "root" && active?.label) return active.label;
    if (topCategory === "media" && mediaSubcategory === "root" && active?.label) return active.label;
    if (topCategory === "settings" && active?.label) return active.label;
    if (topCategory === "current" && active?.label) return active.label;
    return selectedCategoryLabel;
  }, [
    topLevelShelfActive,
    selectedCategoryLabel,
    displayItems,
    topLevelShelfIndex,
    topCategory,
    gameSubcategory,
    mediaSubcategory,
    gamesRootPlane,
    gamesDualShelf,
    homeDualShelf,
    homeRootPlane,
    spotlightEntries,
    spotlightIndex,
  ]);
  const detailRailItems = useMemo<Array<{ id: string; title: string; subtitle: string; imageUrl?: string }>>(() => {
    if (topCategory === "media" && mediaSubcategory !== "root") {
      const current = mediaAssetItems[selectedMediaIndex];
      const imageUrl = current?.thumbnailDataUrl || current?.dataUrl || (current ? mediaThumbById[current.id] : undefined);
      return [
        { id: "m1", title: "Open folder", subtitle: "Reveal in Explorer / Finder", imageUrl },
        { id: "m2", title: "Media hub", subtitle: "Back to Videos & Screenshots", imageUrl },
      ];
    }
    return [];
  }, [topCategory, mediaSubcategory, mediaAssetItems, selectedMediaIndex, mediaThumbById]);

  const gamesHubTiles = useMemo(() => {
    if (!gamesHubDisplayGame) return [];
    const fav = favoriteGameIdSet.has(gamesHubDisplayGame.id);
    const tiles: Array<{ id: string; title: string; subtitle: string; disabled?: boolean }> = [
      {
        id: "play",
        title: currentStreamingGame && currentStreamingGame.id !== gamesHubDisplayGame.id ? "Switch" : "Play",
        subtitle:
          inStreamMenu && currentStreamingGame && currentStreamingGame.id !== gamesHubDisplayGame.id
            ? `Switch from ${currentStreamingGame.title}`
            : currentStreamingGame && currentStreamingGame.id !== gamesHubDisplayGame.id
              ? "Switch to this title"
              : "Launch now",
      },
      {
        id: "favorite",
        title: fav ? "Remove favorite" : "Add favorite",
        subtitle: "Library",
      },
    ];
    if (gamesHubDisplayGame.variants.length > 1) {
      tiles.push({ id: "version", title: "Version", subtitle: "Cycle stream variant" });
    }
    tiles.push({ id: "activities", title: "Activities", subtitle: "Coming soon", disabled: true });
    tiles.push({ id: "progress", title: "Progress", subtitle: "Coming soon", disabled: true });
    return tiles;
  }, [gamesHubDisplayGame, favoriteGameIdSet, currentStreamingGame, inStreamMenu]);

  useEffect(() => {
    const n = gamesHubTiles.length;
    if (n === 0) return;
    setGamesHubFocusIndex((i) => Math.max(0, Math.min(n - 1, i)));
  }, [gamesHubTiles.length, gamesHubDisplayGame?.id]);
  const focusMotionKey = useMemo(() => {
    if (topCategory === "current" && gamesHubOpen) {
      return `game-${gamesHubDisplayGame?.id ?? "none"}`;
    }
    if (topCategory === "current" && homeDualShelf && homeRootPlane === "spotlight") {
      const entry = spotlightEntries[spotlightIndex];
      if (entry?.kind === "cloudResume") return `home-spotlight-resume-${entry.busy ? "busy" : "idle"}`;
      if (spotlightEntryHasGame(entry)) return `home-spotlight-${entry.game.id}`;
      return `home-spotlight-empty-${spotlightIndex}`;
    }
    if (topCategory === "all" && gameSubcategory === "root" && gamesDualShelf && gamesRootPlane === "spotlight") {
      const entry = spotlightEntries[spotlightIndex];
      if (entry?.kind === "cloudResume") return `spotlight-resume-${entry.busy ? "busy" : "idle"}`;
      if (spotlightEntryHasGame(entry)) return `spotlight-${entry.game.id}`;
      return `spotlight-empty-${spotlightIndex}`;
    }
    if (topCategory === "all" && gameSubcategory !== "root") return `game-${selectedGame?.id ?? "none"}`;
    if (topCategory === "media" && mediaSubcategory !== "root") return `media-${selectedMediaIndex}-${mediaAssetItems[selectedMediaIndex]?.id ?? "none"}`;
    return `menu-${topCategory}-${topLevelShelfIndex}`;
  }, [
    topCategory,
    gameSubcategory,
    gamesRootPlane,
    homeDualShelf,
    homeRootPlane,
    gamesHubOpen,
    gamesHubDisplayGame?.id,
    spotlightEntries,
    spotlightIndex,
    gamesDualShelf,
    selectedGame?.id,
    topLevelShelfIndex,
    mediaSubcategory,
    selectedMediaIndex,
    mediaAssetItems,
  ]);
  const {
    listTranslateX,
    spotlightShelfTranslateX,
    gamesRootMenuTranslateX,
    heroTransitionMs,
    wrapperThemeVars,
    wrapperClassNameWithRow,
    menuShelfTranslateX,
  } = useControllerLibraryLayoutMotion({
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
    spotlightEntriesLength: spotlightEntries.length,
    itemsContainerRef,
    spotlightTrackRef,
    focusMotionKey,
    settings,
    ps5Row,
  });
  useEffect(() => {
    if (topCategory !== "all") {
      setGamesRootPlane("categories");
      setSpotlightIndex(0);
    }
  }, [topCategory]);

  useEffect(() => {
    setOptionsOpen(false);
    setOptionsEntries([]);
    setOptionsFocusIndex(0);
  }, [topCategory, gameSubcategory, mediaSubcategory, settingsSubcategory]);

  useEffect(() => {
    gamesHubReturnSnapshotRef.current = null;
    setGamesHubOpen(false);
    setGamesHubFocusIndex(0);
  }, [topCategory]);

  useEffect(() => {
    if (gameSubcategory === "root") {
      gamesHubReturnSnapshotRef.current = null;
      setGamesHubOpen(false);
      setGamesHubFocusIndex(0);
    }
  }, [gameSubcategory]);

  useEffect(() => {
    if (!gamesShelfBrowseActive) {
      gamesHubReturnSnapshotRef.current = null;
      setGamesHubOpen(false);
      setGamesHubFocusIndex(0);
    }
  }, [gamesShelfBrowseActive]);

  useEffect(() => {
    if (!gamesShelfBrowseActive || categorizedGames.length === 0) return;
    const idxs = [selectedIndex - 2, selectedIndex - 1, selectedIndex + 1, selectedIndex + 2];
    for (const i of idxs) {
      const url = categorizedGames[i]?.imageUrl;
      if (typeof url === "string" && url.length > 0) {
        const im = new Image();
        im.src = url;
      }
    }
  }, [gamesShelfBrowseActive, selectedIndex, categorizedGames]);

  useEffect(() => {
    setPs5Row("main");
    setDetailRailIndex(0);
  }, [topCategory, gameSubcategory, mediaSubcategory, settingsSubcategory]);

  useEffect(() => {
    if (ps5Row === "detail" && !canEnterDetailRow) {
      setPs5Row("main");
      return;
    }
    if (ps5Row === "top" && !canEnterTopRow) {
      setPs5Row("main");
    }
  }, [ps5Row, canEnterDetailRow, canEnterTopRow]);

  const throttledOnSelectGame = useCallback((id: string) => onSelectGame(id), [onSelectGame]);

  const toggleFavoriteForSelected = useCallback(() => {
    if (selectedGame) {
      onToggleFavoriteGame(selectedGame.id);
      playUiSound("confirm");
    }
  }, [onToggleFavoriteGame, playUiSound, selectedGame]);


  const controllerEventHandlers = useControllerLibraryEvents({
    isLoading,
    TOP_CATEGORIES,
    categorizedGames,
    selectedIndex,
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
    categoryIndex,
    featuredHomeGame,
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
    streamMenuMicLevel,
    onStreamMenuMicLevelChange,
    streamMenuVolume,
    onStreamMenuVolumeChange,
    onStreamMenuToggleMicrophone,
    onStreamMenuToggleFullscreen,
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
    setOptionsEntries,
    setOptionsOpen,
    setOptionsFocusIndex,
    setGamesHubFocusIndex,
    setPs5Row,
    setDetailRailIndex,
    setGamesRootPlane,
    setHomeRootPlane,
    setSpotlightIndex,
    gamesHubReturnSnapshotRef,
    setGamesHubOpen,
    setEndSessionConfirm,
    setLastRootGameIndex,
    setLastRootSettingIndex,
    setLastSystemMenuIndex,
    setLastThemeRootIndex,
    setLastRootMediaIndex,
    setLibrarySortId,
    localVideoPlayerOpen,
    closeLocalVideoPlayer,
    openLocalVideoPlayer,
    localVideoFilePathForOptions: localVideoPlayback?.filePath ?? null,
    bumpMediaListRefresh,
  });


  const controllerEventHandlersRef = useRef(controllerEventHandlers);

  useEffect(() => {
    controllerEventHandlersRef.current = controllerEventHandlers;
  }, [controllerEventHandlers]);
  useControllerWindowBindings(controllerEventHandlersRef);

  const renderFaceButton = (kind: "primary" | "secondary" | "tertiary", className: string, size: number): JSX.Element => {
    if (kind === "primary") {
      return controllerType === "ps"
        ? <ButtonPSCross className={className} size={size} />
        : <ButtonA className={className} size={size} />;
    }

    if (kind === "secondary") {
      return controllerType === "ps"
        ? <ButtonPSSquare className={className} size={size} />
        : <ButtonX className={className} size={size} />;
    }

    return controllerType === "ps"
      ? <ButtonPSTriangle className={className} size={size} />
      : <ButtonY className={className} size={size} />;
  };

  const libraryGameBackdropOn = settings.controllerLibraryGameBackdrop !== false;

  const heroBackdropUrlRaw = useMemo(() => {
    if (topCategory === "current" && gamesHubOpen && gamesHubDisplayGame?.imageUrl) {
      return gamesHubDisplayGame.imageUrl;
    }
    if (
      topCategory === "all" &&
      gameSubcategory === "root" &&
      gamesDualShelf &&
      gamesRootPlane === "spotlight" &&
      spotlightEntries.length > 0
    ) {
      const cur = spotlightEntries[spotlightIndex];
      if (cur?.kind === "cloudResume" && cur.coverUrl) return cur.coverUrl;
      if (spotlightEntryHasGame(cur) && cur.game.imageUrl) return cur.game.imageUrl;
      for (const e of spotlightEntries) {
        if (e.kind === "cloudResume" && e.coverUrl) return e.coverUrl;
        if (e.kind === "recent" && e.game?.imageUrl) return e.game.imageUrl;
      }
      return null;
    }
    if (topCategory === "current" && homeDualShelf && homeRootPlane === "spotlight" && spotlightEntries.length > 0) {
      const cur = spotlightEntries[spotlightIndex];
      if (cur?.kind === "cloudResume" && cur.coverUrl) return cur.coverUrl;
      if (spotlightEntryHasGame(cur) && cur.game.imageUrl) return cur.game.imageUrl;
      for (const e of spotlightEntries) {
        if (e.kind === "cloudResume" && e.coverUrl) return e.coverUrl;
        if (e.kind === "recent" && e.game?.imageUrl) return e.game.imageUrl;
      }
      return null;
    }
    if (topCategory === "all") return selectedGame?.imageUrl ?? null;
    if (topCategory === "current") return currentTabGame?.imageUrl ?? null;
    if (topCategory === "media") {
      if (selectedMediaItem?.thumbnailDataUrl) return selectedMediaItem.thumbnailDataUrl;
      if (selectedMediaItem?.dataUrl) return selectedMediaItem.dataUrl;
      return selectedMediaItem ? mediaThumbById[selectedMediaItem.id] ?? null : null;
    }
    if (currentTabGame?.imageUrl) return currentTabGame.imageUrl;
    return selectedGame?.imageUrl ?? null;
  }, [
    topCategory,
    gameSubcategory,
    gamesRootPlane,
    homeDualShelf,
    homeRootPlane,
    gamesHubOpen,
    gamesHubDisplayGame,
    gamesDualShelf,
    spotlightEntries,
    spotlightIndex,
    selectedGame,
    currentTabGame,
    selectedMediaItem,
    mediaThumbById,
  ]);

  const heroBackdropUrl = useMemo(() => {
    if (libraryGameBackdropOn) return heroBackdropUrlRaw;
    if (topCategory === "media") return heroBackdropUrlRaw;
    return null;
  }, [libraryGameBackdropOn, topCategory, heroBackdropUrlRaw]);

  const themeRgbForTrack = settings.controllerThemeColor ?? { r: 124, g: 241, b: 177 };
  const maxBitrateMbpsForTrack = settings.maxBitrateMbps ?? 75;

  const topLevelMenuTrack = useMemo(() => (
    <TopLevelMenuTrack
      itemsContainerRef={itemsContainerRef}
      topCategory={topCategory}
      gameSubcategory={gameSubcategory}
      menuShelfTranslateX={menuShelfTranslateX}
      displayItems={displayItems}
      topLevelShelfIndex={topLevelShelfIndex}
      gameCategoryPreviewById={gameCategoryPreviewById}
      currentStreamingImageUrl={homeResumeSnapshotUrl ?? currentTabGame?.imageUrl}
      featuredPreviewImageUrl={featuredHomeGame?.imageUrl ?? null}
      settingsSubcategory={settingsSubcategory}
      editingBandwidth={editingBandwidth}
      maxBitrateMbpsForTrack={maxBitrateMbpsForTrack}
      onSettingChange={onSettingChange}
      themeRgbForTrack={themeRgbForTrack}
      editingThemeChannel={editingThemeChannel}
      inStreamMenu={inStreamMenu}
      streamMenuMicLevel={streamMenuMicLevel}
      onStreamMenuMicLevelChange={onStreamMenuMicLevelChange}
      editingStreamMicLevel={editingStreamMicLevel}
      streamMenuVolume={streamMenuVolume}
      onStreamMenuVolumeChange={onStreamMenuVolumeChange}
      editingStreamVolume={editingStreamVolume}
      controllerType={controllerType}
      streamMicTrack={streamMicTrack}
    />
  ), [
    topCategory,
    gameSubcategory,
    menuShelfTranslateX,
    displayItems,
    topLevelShelfIndex,
    gameCategoryPreviewById,
    homeResumeSnapshotUrl,
    currentTabGame?.imageUrl,
    featuredHomeGame?.imageUrl,
    editingBandwidth,
    editingThemeChannel,
    settingsSubcategory,
    onSettingChange,
    themeRgbForTrack.r,
    themeRgbForTrack.g,
    themeRgbForTrack.b,
    maxBitrateMbpsForTrack,
    inStreamMenu,
    streamMenuMicLevel,
    onStreamMenuMicLevelChange,
    streamMenuVolume,
    onStreamMenuVolumeChange,
    editingStreamVolume,
    editingStreamMicLevel,
    controllerType,
    streamMicTrack,
  ]);

  return (
    <>
    <ControllerLibraryLayout
      isLoading={isLoading}
      localVideoPlayerOpen={localVideoPlayerOpen}
      topCategory={topCategory}
      wrapperClassNameWithRow={wrapperClassNameWithRow}
      wrapperThemeVars={wrapperThemeVars}
      currentStreamingGame={currentStreamingGame}
      currentTabGame={currentTabGame}
      inStreamMenu={inStreamMenu}
      endSessionConfirm={endSessionConfirm}
      parallaxBackdropTiles={libraryGameBackdropOn ? parallaxBackdropTiles : []}
      heroBackdropUrl={heroBackdropUrl}
      loadingBackdropImageUrl={libraryGameBackdropOn ? currentTabGame?.imageUrl ?? null : null}
      gameHubShowHeroBackdrop={libraryGameBackdropOn}
      settings={settings}
      subscriptionInfo={subscriptionInfo}
      sessionStartedAtMs={sessionStartedAtMs}
      isStreaming={isStreaming}
      userAvatarUrl={userAvatarUrl}
      userName={userName}
      categoryIndex={categoryIndex}
      TOP_CATEGORIES={TOP_CATEGORIES}
      getCategoryIcon={getCategoryIcon}
      gameSubcategory={gameSubcategory}
      gamesHubOpen={gamesHubOpen}
      gamesHubDisplayGame={gamesHubDisplayGame}
      selectedGame={selectedGame}
      gameHubScreenshotUrls={gameHubScreenshotUrls}
      playtimeData={playtimeData}
      selectedVariantId={selectedVariantId}
      librarySortId={librarySortId}
      gamesHubTiles={gamesHubTiles}
      gamesHubFocusIndex={gamesHubFocusIndex}
      categorizedGames={categorizedGames}
      focusMotionKey={focusMotionKey}
      selectedVariantByGameId={selectedVariantByGameId}
      favoriteGameIdSet={favoriteGameIdSet}
      selectedIndex={selectedIndex}
      itemsContainerRef={itemsContainerRef}
      listTranslateX={listTranslateX}
      topLevelShelfActive={topLevelShelfActive}
      selectedTopLevelItemLabel={selectedTopLevelItemLabel}
      gamesRootPlane={gamesRootPlane}
      homeRootPlane={homeRootPlane}
      homeDualShelf={homeDualShelf}
      featuredHomeGame={featuredHomeGame}
      featuredIsFavorite={featuredIsFavorite}
      spotlightEntries={spotlightEntries}
      spotlightIndex={spotlightIndex}
      displayItems={displayItems}
      topLevelShelfIndex={topLevelShelfIndex}
      gamesDualShelf={gamesDualShelf}
      cloudSessionResumable={cloudSessionResumable}
      onResumeCloudSession={onResumeCloudSession}
      spotlightTrackRef={spotlightTrackRef}
      spotlightShelfTranslateX={spotlightShelfTranslateX}
      topLevelMenuTrack={topLevelMenuTrack}
      mediaSubcategory={mediaSubcategory}
      selectedMediaItem={selectedMediaItem}
      mediaAssetItems={mediaAssetItems}
      mediaHubPlaceholderCount={mediaHubPlaceholderCount}
      mediaLoading={mediaLoading}
      mediaError={mediaError}
      mediaHubSlots={mediaHubSlots}
      selectedMediaIndex={selectedMediaIndex}
      mediaThumbById={mediaThumbById}
      ps5Row={ps5Row}
      canEnterDetailRow={canEnterDetailRow}
      detailRailItems={detailRailItems}
      detailRailIndex={detailRailIndex}
      optionsOpen={optionsOpen}
      optionsEntries={optionsEntries}
      optionsFocusIndex={optionsFocusIndex}
      topLevelRowBehaviorActive={topLevelRowBehaviorActive}
      settingsSubcategory={settingsSubcategory}
      editingThemeChannel={editingThemeChannel}
      selectedGameForHints={gamesHubDisplayGame ?? selectedGame}
      controllerType={controllerType}
      renderFaceButton={renderFaceButton}
    />
    {localVideoPlayback ? (
      <LocalVideoPlayerOverlay src={localVideoPlayback.src} onClose={closeLocalVideoPlayer} />
    ) : null}
    </>
  );
}
