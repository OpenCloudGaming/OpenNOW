import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { JSX } from "react";
import type { GameInfo, MediaListingEntry, Settings, ControllerThemeStyle } from "@shared/gfn";
import { Star, Clock, Calendar, Repeat2, House, Settings as SettingsIcon, Library, Clapperboard } from "lucide-react";
import { ButtonA, ButtonB, ButtonX, ButtonY, ButtonPSCross, ButtonPSCircle, ButtonPSSquare, ButtonPSTriangle } from "./ControllerButtons";
import { getStoreDisplayName } from "./GameCard";
import { SessionElapsedIndicator, RemainingPlaytimeIndicator, CurrentClock } from "./ElapsedSessionIndicators";
import { ControllerGameHub } from "./ControllerGameHub";
import { Ps5LoadingScreen } from "./Ps5LoadingScreen";
import { type PlaytimeStore, formatPlaytime, formatLastPlayed } from "../utils/usePlaytime";
import { playControllerUiSound } from "../utils/controllerUiSound";
import {
  type ControllerOverlayNavSnapshot,
  readControllerOverlayNav,
  writeControllerOverlayNav,
} from "../utils/controllerOverlayNavStorage";

interface ControllerLibraryPageProps {
  games: GameInfo[];
  isLoading: boolean;
  selectedGameId: string;
  uiSoundsEnabled: boolean;
  selectedVariantByGameId: Record<string, string>;
  favoriteGameIds: string[];
  userName?: string;
  userAvatarUrl?: string;
  subscriptionInfo: import("@shared/gfn").SubscriptionInfo | null;
  playtimeData?: PlaytimeStore;
  onSelectGame: (id: string) => void;
  onSelectGameVariant: (gameId: string, variantId: string) => void;
  onToggleFavoriteGame: (gameId: string) => void;
  onPlayGame: (game: GameInfo) => void;
  onOpenSettings?: () => void;
  currentStreamingGame?: GameInfo | null;
  onResumeGame?: (game: GameInfo) => void;
  onCloseGame?: () => void;
  onExitApp?: () => void;
  pendingSwitchGameCover?: string | null;
  settings?: {
    resolution?: string;
    fps?: number;
    codec?: string;
    enableL4S?: boolean;
    enableCloudGsync?: boolean;
    microphoneDeviceId?: string;
    controllerUiSounds?: boolean;
    controllerBackgroundAnimations?: boolean;
    autoLoadControllerLibrary?: boolean;
    autoFullScreen?: boolean;
    aspectRatio?: string;
    posterSizeScale?: number;
    maxBitrateMbps?: number;
    controllerThemeStyle?: ControllerThemeStyle;
    controllerThemeColor?: { r: number; g: number; b: number };
  };
  resolutionOptions?: string[];
  fpsOptions?: number[];
  codecOptions?: string[];
  aspectRatioOptions?: string[];
  onSettingChange?: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
  onExitControllerMode?: () => void;
  sessionStartedAtMs?: number | null;
  isStreaming?: boolean;
  sessionCounterEnabled?: boolean;
  /** In-stream Meta/Home overlay: extra confirm for end session, nav restore, etc. */
  inStreamMenu?: boolean;
  /** In-stream: gamepad-friendly stream actions (see Current row). */
  streamMenuVolume?: number;
  onStreamMenuVolumeChange?: (volume01: number) => void;
  streamMenuMicLevel?: number;
  onStreamMenuMicLevelChange?: (level01: number) => void;
  onStreamMenuToggleMicrophone?: () => void;
  onStreamMenuToggleFullscreen?: () => void;
  streamMenuMicOn?: boolean;
  streamMenuIsFullscreen?: boolean;
  /** When a cloud session can be continued (server ready, app idle), show a PS5-style resume tile in the Games spotlight row. */
  cloudSessionResumable?: boolean;
  cloudResumeTitle?: string | null;
  cloudResumeCoverUrl?: string | null;
  onResumeCloudSession?: () => void;
  cloudResumeBusy?: boolean;
}

type Direction = "up" | "down" | "left" | "right";
type TopCategory = "current" | "all" | "settings" | "media";
type SoundKind = "move" | "confirm";
type SettingsSubcategory = "root" | "Network" | "Audio" | "Video" | "System" | "Theme" | "ThemeColor" | "ThemeStyle";
type MediaSubcategory = "root" | "Videos" | "Screenshots";
type GameSubcategory = "root" | "all" | "favorites" | `genre:${string}`;
type LibrarySortId = "recent" | "az" | "za" | "favoritesFirst";

/** Captured when opening game hub so Back restores navigation (shelf, spotlight, category). */
type GamesHubReturnSnapshot = {
  gameSubcategory: GameSubcategory;
  selectedGameSubcategoryIndex: number;
  gamesRootPlane: "spotlight" | "categories";
  spotlightIndex: number;
  /** Omitted when the hub was not opened from a game selection (e.g. cloud resume tile). */
  restoreSelectedGameId?: string;
};

type SpotlightEntry =
  | { kind: "cloudResume"; title: string; coverUrl: string | null; busy: boolean }
  | { kind: "recent"; game: GameInfo | null };

function spotlightEntryHasGame(entry: SpotlightEntry | undefined): entry is { kind: "recent"; game: GameInfo } {
  return entry?.kind === "recent" && entry.game != null;
}

const LIBRARY_SORT_STORAGE_KEY = "opennow:controllerLibrarySort.v1";

const LIBRARY_SORT_LABEL: Record<LibrarySortId, string> = {
  recent: "Recent",
  az: "A–Z",
  za: "Z–A",
  favoritesFirst: "Favorites first",
};

function readLibrarySortId(): LibrarySortId {
  try {
    const v = typeof sessionStorage !== "undefined" ? sessionStorage.getItem(LIBRARY_SORT_STORAGE_KEY) : null;
    if (v === "recent" || v === "az" || v === "za" || v === "favoritesFirst") return v;
  } catch {
  }
  return "recent";
}

const CATEGORY_STEP_PX = 160;
const CATEGORY_ACTIVE_HALF_WIDTH_PX = 60;
const GAME_ACTIVE_CENTER_OFFSET_X_PX = 320;
const PREVIEW_TILE_COUNT = 6;
const SPOTLIGHT_RECENT_COUNT = 5;
const MEDIA_HUB_MIN_TILES = 8;
const MEDIA_VIDEO_PLACEHOLDER_TEMPLATES: ReadonlyArray<{ title: string; subtitle: string }> = [
  { title: "Recent Clip slot", subtitle: "Record gameplay moments" },
  { title: "Highlight Reel slot", subtitle: "Mark your best plays" },
  { title: "Shared Clip slot", subtitle: "Publish to your social feed" },
  { title: "Squad Clip slot", subtitle: "Capture co-op highlights" },
];
const MEDIA_SCREENSHOT_PLACEHOLDER_TEMPLATES: ReadonlyArray<{ title: string; subtitle: string }> = [
  { title: "Recent Screenshot slot", subtitle: "Capture gameplay stills" },
  { title: "Wallpaper slot", subtitle: "Save scenic moments" },
  { title: "Trophy Moment slot", subtitle: "Archive major unlocks" },
  { title: "Shared Screenshot slot", subtitle: "Publish to your social feed" },
];

/** Decode off main thread; lazy-load shelf art so clock/timer rerenders don’t contend with image work */
const SHELF_IMAGE_PROPS = { decoding: "async" as const, loading: "lazy" as const };
const SHELF_IMAGE_WINDOW_RADIUS = 8;
const SHELF_CONTENT_WINDOW_RADIUS = 14;

function isWithinImageWindow(index: number, activeIndex: number, radius: number = SHELF_IMAGE_WINDOW_RADIUS): boolean {
  return Math.abs(index - activeIndex) <= radius;
}

function isWithinContentWindow(index: number, activeIndex: number, radius: number = SHELF_CONTENT_WINDOW_RADIUS): boolean {
  return Math.abs(index - activeIndex) <= radius;
}

/** XMB-style horizontal shelf: align active tile center with the shelf viewport center (track’s parent). */
function computeShelfTranslateXToCenter(track: HTMLElement | null, activeIndex: number): number {
  if (!track) return 0;
  const viewport = track.parentElement;
  if (!(viewport instanceof HTMLElement)) return 0;
  const children = Array.from(track.children) as HTMLElement[];
  if (children.length === 0 || activeIndex < 0 || activeIndex >= children.length) return 0;
  const activeEl = children[activeIndex];
  const centerInTrack = activeEl.offsetLeft + activeEl.offsetWidth / 2;
  const halfVp = viewport.clientWidth / 2;
  return halfVp - track.offsetLeft - centerInTrack;
}

function computeShelfTranslateXClamped(track: HTMLElement | null, activeIndex: number): number {
  if (!track) return 0;
  const viewport = track.parentElement;
  if (!(viewport instanceof HTMLElement)) return 0;
  const desired = computeShelfTranslateXToCenter(track, activeIndex);
  const maxTranslate = -track.offsetLeft;
  const minTranslate = viewport.clientWidth - (track.offsetLeft + track.scrollWidth);
  if (minTranslate > maxTranslate) return maxTranslate;
  return Math.max(minTranslate, Math.min(maxTranslate, desired));
}

const CONTROLLER_THEME_STYLE_ORDER: readonly ControllerThemeStyle[] = ["aurora", "nebula", "grid", "minimal", "pulse"];

const CONTROLLER_THEME_STYLE_LABEL: Record<ControllerThemeStyle, string> = {
  aurora: "Aurora",
  nebula: "Nebula",
  grid: "Grid",
  minimal: "Minimal",
  pulse: "Pulse",
};

function sanitizeControllerThemeStyle(raw: string | undefined): ControllerThemeStyle {
  return CONTROLLER_THEME_STYLE_ORDER.includes(raw as ControllerThemeStyle) ? (raw as ControllerThemeStyle) : "aurora";
}

function clampRgbByte(n: number): number {
  return Math.max(0, Math.min(255, Math.round(Number.isFinite(n) ? n : 0)));
}

function sanitizeGenreName(raw: string): string {
  return raw
    .replace(/_/g, " ")
    .replace(/\b\w/g, (ch) => ch.toUpperCase());
}

function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  return target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.tagName === "SELECT";
}

function getCategoryLabel(categoryId: string, currentGameTitle?: string): { label: string } {
  if (categoryId === "current") return { label: currentGameTitle || "Current" };
  if (categoryId === "all") return { label: "Games" };
  if (categoryId === "settings") return { label: "Settings" };
  if (categoryId === "media") return { label: "Media" };
  return { label: "Games" };
}

function getCategoryIcon(categoryId: string): JSX.Element {
  if (categoryId === "current") return <House size={28} />;
  if (categoryId === "settings") return <SettingsIcon size={28} />;
  if (categoryId === "media") return <Clapperboard size={28} />;
  return <Library size={28} />;
}


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
  pendingSwitchGameCover,
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
  sessionCounterEnabled = false,
  inStreamMenu = false,
  streamMenuVolume = 1,
  onStreamMenuVolumeChange,
  streamMenuMicLevel = 1,
  onStreamMenuMicLevelChange,
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
  const [isEntering, setIsEntering] = useState(true);
  const initialCategoryIndex = (() => {
    if (currentStreamingGame) {
      // TOP_CATEGORIES: current (game title), settings, all, media
      return 0;
    }
    // TOP_CATEGORIES without `current`: settings, all, media
    return 1;
  })();
  const [categoryIndex, setCategoryIndex] = useState(initialCategoryIndex);
  const [endSessionConfirm, setEndSessionConfirm] = useState(false);
  const [editingStreamVolume, setEditingStreamVolume] = useState(false);
  const [editingStreamMicLevel, setEditingStreamMicLevel] = useState(false);
  const itemsContainerRef = useRef<HTMLDivElement>(null);
  const overlayNavWriteRef = useRef<ControllerOverlayNavSnapshot | null>(null);
  const overlayNavRestoredRef = useRef(false);
  const currentPosterImgRef = useRef<HTMLImageElement | null>(null);
  const [metaMaxWidth, setMetaMaxWidth] = useState<number | null>(null);
  const posterObserverRef = useRef<ResizeObserver | null>(null);
  const attachPosterRef = (el: HTMLImageElement | null) => {
    if (posterObserverRef.current) {
      try { posterObserverRef.current.disconnect(); } catch {}
      posterObserverRef.current = null;
    }
    currentPosterImgRef.current = el;
    const update = () => setMetaMaxWidth(currentPosterImgRef.current?.clientWidth ?? null);
    if (el) {
      if (typeof ResizeObserver !== "undefined") {
        const ro = new ResizeObserver(update);
        posterObserverRef.current = ro;
        try { ro.observe(el); } catch {}
      }
      update();
    } else {
      setMetaMaxWidth(null);
    }
  };
  const [listTranslateY, setListTranslateY] = useState(0);
  const [listTranslateX, setListTranslateX] = useState(0);
  /** Dual-shelf games root: separate pans so “Recently played” and “Library” rows both stay screen-centered */
  const [spotlightShelfTranslateX, setSpotlightShelfTranslateX] = useState(0);
  const [gamesRootMenuTranslateX, setGamesRootMenuTranslateX] = useState(0);
  const [viewportWidth, setViewportWidth] = useState(() =>
    typeof window === "undefined" ? 1200 : window.innerWidth,
  );
  const favoriteGameIdSet = useMemo(() => new Set(favoriteGameIds), [favoriteGameIds]);
  const favoriteGames = useMemo(
    () => games.filter((game) => favoriteGameIdSet.has(game.id)),
    [games, favoriteGameIdSet],
  );
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
  const [mediaLoading, setMediaLoading] = useState(false);
  const [mediaError, setMediaError] = useState<string | null>(null);
  const [mediaVideos, setMediaVideos] = useState<MediaListingEntry[]>([]);
  const [mediaScreenshots, setMediaScreenshots] = useState<MediaListingEntry[]>([]);
  const [mediaThumbById, setMediaThumbById] = useState<Record<string, string>>({});
  const [controllerType, setControllerType] = useState<"ps" | "xbox" | "nintendo" | "generic">("generic");
  const [editingBandwidth, setEditingBandwidth] = useState(false);
  const [lastSystemMenuIndex, setLastSystemMenuIndex] = useState(0);
  const [lastThemeRootIndex, setLastThemeRootIndex] = useState(0);
  const [editingThemeChannel, setEditingThemeChannel] = useState<null | "r" | "g" | "b">(null);
  const [ps5Row, setPs5Row] = useState<"top" | "main" | "detail">("main");
  const [detailRailIndex, setDetailRailIndex] = useState(0);
  const [librarySortId, setLibrarySortId] = useState<LibrarySortId>(() => readLibrarySortId());
  const [gamesRootPlane, setGamesRootPlane] = useState<"spotlight" | "categories">("spotlight");
  const [spotlightIndex, setSpotlightIndex] = useState(0);
  const [optionsOpen, setOptionsOpen] = useState(false);
  const [optionsEntries, setOptionsEntries] = useState<Array<{ id: string; label: string }>>([]);
  const [optionsFocusIndex, setOptionsFocusIndex] = useState(0);
  const [heroTransitionMs, setHeroTransitionMs] = useState(420);
  const [gamesHubOpen, setGamesHubOpen] = useState(false);
  const [gamesHubFocusIndex, setGamesHubFocusIndex] = useState(0);
  /** Local captures for the focused game; loaded when hub opens so Media tab need not be visited first */
  const [gameHubScreenshotUrls, setGameHubScreenshotUrls] = useState<string[]>([]);
  const gamesHubReturnSnapshotRef = useRef<GamesHubReturnSnapshot | null>(null);
  const spotlightTrackRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    try {
      sessionStorage.setItem(LIBRARY_SORT_STORAGE_KEY, librarySortId);
    } catch {
    }
  }, [librarySortId]);

  useEffect(() => {
    if (typeof window === "undefined") {
      setIsEntering(false);
      return;
    }

    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      setIsEntering(false);
      return;
    }

    const timeoutId = window.setTimeout(() => {
      setIsEntering(false);
    }, 760);

    return () => {
      window.clearTimeout(timeoutId);
    };
  }, []);


  // poster measurement handled by `attachPosterRef` callback ref

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

  const allGenres = useMemo(() => {
    const genreSet = new Set<string>();
    for (const game of games) {
      if (game.genres && Array.isArray(game.genres)) {
        for (const genre of game.genres) genreSet.add(genre);
      }
    }
    return Array.from(genreSet).sort();
  }, [games]);

  const TOP_CATEGORIES = useMemo(() => {
    const categories: Array<{ id: TopCategory; label: string }> = [];
    if (currentStreamingGame) {
      categories.push({ id: "current", label: currentStreamingGame.title || "Current Game" });
    }
    categories.push({ id: "settings", label: "Settings" });
    categories.push({ id: "all", label: "Games" });
    categories.push({ id: "media", label: "Media" });
    return categories;
  }, [currentStreamingGame]);

  const topCategory = (TOP_CATEGORIES[categoryIndex]?.id ?? "all") as unknown as string;

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
 
  const currentGameItems = useMemo(() => {
    const streamExtras = inStreamMenu
      ? [
          { id: "toggleMic", label: "Microphone", value: streamMenuMicOn ? "On" : "Off" },
          {
            id: "streamMicLevel",
            label: "Mic level",
            value: `${Math.round((streamMenuMicLevel ?? 1) * 100)}%`,
          },
          {
            id: "streamVolume",
            label: "Stream volume",
            value: `${Math.round((streamMenuVolume ?? 1) * 100)}%`,
          },
          { id: "openMedia", label: "Media & captures", value: "Open" },
          {
            id: "toggleFullscreen",
            label: "Fullscreen",
            value: streamMenuIsFullscreen ? "On" : "Off",
          },
        ]
      : [];
    return [
      { id: "resume", label: "Resume Game", value: "" },
      ...streamExtras,
      {
        id: "closeGame",
        label:
          inStreamMenu && endSessionConfirm ? "End session (confirm)" : "Close Game",
        value: "",
      },
    ];
  }, [
    inStreamMenu,
    endSessionConfirm,
    streamMenuMicOn,
    streamMenuMicLevel,
    streamMenuVolume,
    streamMenuIsFullscreen,
  ]);

  const mediaRootItems = useMemo(() => [
    { id: "videos", label: "Videos", value: "" },
    { id: "screenshots", label: "Screenshots", value: "" },
  ], []);

  const mediaAssetItems = useMemo(() => {
    if (mediaSubcategory === "Videos") return mediaVideos;
    if (mediaSubcategory === "Screenshots") return mediaScreenshots;
    return [];
  }, [mediaSubcategory, mediaVideos, mediaScreenshots]);

  const gameRootItems = useMemo(() => {
    const items: Array<{ id: GameSubcategory; label: string; value: string }> = [
      { id: "all", label: "All Games", value: `${games.length}` },
      { id: "favorites", label: "Favorites", value: `${favoriteGames.length}` },
    ];
    for (const genre of allGenres) {
      const count = games.filter((game) => game.genres?.includes(genre)).length;
      items.push({ id: `genre:${genre}`, label: sanitizeGenreName(genre), value: `${count}` });
    }
    return items;
  }, [allGenres, favoriteGames.length, games]);

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
    if (topCategory !== "media" || mediaSubcategory === "root") return;
    if (typeof window.openNow?.listMediaByGame !== "function") {
      setMediaVideos([]);
      setMediaScreenshots([]);
      setMediaThumbById({});
      setMediaError("Media API unavailable");
      setMediaLoading(false);
      return;
    }

    let cancelled = false;
    const loadMedia = async () => {
      try {
        setMediaLoading(true);
        setMediaError(null);
        const listing = await window.openNow.listMediaByGame({});
        if (cancelled) return;

        const videos = [...(listing.videos ?? [])].sort((a, b) => b.createdAtMs - a.createdAtMs);
        const screenshots = [...(listing.screenshots ?? [])].sort((a, b) => b.createdAtMs - a.createdAtMs);

        setMediaVideos(videos);
        setMediaScreenshots(screenshots);

        const allItems = [...videos, ...screenshots];
        const thumbEntries = await Promise.all(
          allItems.map(async (item): Promise<[string, string | null]> => {
            if (item.thumbnailDataUrl) return [item.id, item.thumbnailDataUrl];
            if (item.dataUrl) return [item.id, item.dataUrl];
            if (typeof window.openNow?.getMediaThumbnail === "function") {
              const generated = await window.openNow.getMediaThumbnail({ filePath: item.filePath });
              return [item.id, generated];
            }
            return [item.id, null];
          }),
        );

        if (cancelled) return;
        const thumbMap: Record<string, string> = {};
        for (const [id, url] of thumbEntries) {
          if (url) thumbMap[id] = url;
        }
        setMediaThumbById(thumbMap);
      } catch {
        if (cancelled) return;
        setMediaError("Failed to load media");
      } finally {
        if (!cancelled) setMediaLoading(false);
      }
    };

    void loadMedia();
    return () => {
      cancelled = true;
    };
  }, [topCategory, mediaSubcategory]);

  const categorizedGames = useMemo(() => {
    if (topCategory === "settings" || topCategory === "current" || topCategory === "media") return [];
    if (gameSubcategory === "root") return [];
    if (gameSubcategory === "favorites") return favoriteGames;
    if (gameSubcategory.startsWith("genre:")) {
      const genreName = gameSubcategory.slice(6);
      return games.filter((game) => game.genres?.includes(genreName));
    }
    const lastPlayedMs = (gameId: string) => {
      const raw = playtimeData[gameId]?.lastPlayedAt;
      if (!raw) return 0;
      const ms = Date.parse(raw);
      return Number.isFinite(ms) ? ms : 0;
    };
    const sortByRecent = (a: GameInfo, b: GameInfo) => {
      const aLastPlayed = lastPlayedMs(a.id);
      const bLastPlayed = lastPlayedMs(b.id);
      if (aLastPlayed !== bLastPlayed) return bLastPlayed - aLastPlayed;
      return a.title.localeCompare(b.title);
    };
    const base = [...games];
    if (librarySortId === "recent") {
      base.sort(sortByRecent);
    } else if (librarySortId === "az") {
      base.sort((a, b) => a.title.localeCompare(b.title));
    } else if (librarySortId === "za") {
      base.sort((a, b) => b.title.localeCompare(a.title));
    } else {
      base.sort((a, b) => {
        const fa = favoriteGameIdSet.has(a.id);
        const fb = favoriteGameIdSet.has(b.id);
        if (fa !== fb) return fa ? -1 : 1;
        return sortByRecent(a, b);
      });
    }
    return base;
  }, [games, favoriteGames, favoriteGameIdSet, gameSubcategory, topCategory, playtimeData, librarySortId]);

  const gamesSortedByRecent = useMemo(() => {
    return [...games].sort((a, b) => {
      const lastPlayedMs = (gameId: string) => {
        const raw = playtimeData[gameId]?.lastPlayedAt;
        if (!raw) return 0;
        const ms = Date.parse(raw);
        return Number.isFinite(ms) ? ms : 0;
      };
      const aLastPlayed = lastPlayedMs(a.id);
      const bLastPlayed = lastPlayedMs(b.id);
      if (aLastPlayed !== bLastPlayed) return bLastPlayed - aLastPlayed;
      return a.title.localeCompare(b.title);
    });
  }, [games, playtimeData]);

  const spotlightEntries = useMemo((): SpotlightEntry[] => {
    const showResume = Boolean(cloudSessionResumable && onResumeCloudSession);
    const recentCap = showResume ? Math.max(0, SPOTLIGHT_RECENT_COUNT - 1) : SPOTLIGHT_RECENT_COUNT;

    const lastPlayedMs = (gameId: string) => {
      const raw = playtimeData[gameId]?.lastPlayedAt;
      if (!raw) return 0;
      const ms = Date.parse(raw);
      return Number.isFinite(ms) ? ms : 0;
    };

    const played =
      games.length === 0
        ? []
        : games
            .filter((g) => lastPlayedMs(g.id) > 0)
            .sort((a, b) => {
              const d = lastPlayedMs(b.id) - lastPlayedMs(a.id);
              if (d !== 0) return d;
              return a.title.localeCompare(b.title);
            })
            .slice(0, recentCap);

    const recentSlots: SpotlightEntry[] = played.map((g) => ({ kind: "recent" as const, game: g }));
    while (recentSlots.length < recentCap) {
      recentSlots.push({ kind: "recent", game: null });
    }

    if (!showResume) {
      return recentSlots;
    }

    const resumeTitle = cloudResumeTitle?.trim() || "Cloud session";
    return [
      {
        kind: "cloudResume" as const,
        title: resumeTitle,
        coverUrl: cloudResumeCoverUrl ?? null,
        busy: Boolean(cloudResumeBusy),
      },
      ...recentSlots,
    ];
  }, [
    games,
    playtimeData,
    cloudSessionResumable,
    onResumeCloudSession,
    cloudResumeTitle,
    cloudResumeCoverUrl,
    cloudResumeBusy,
  ]);

  useEffect(() => {
    if (spotlightEntries.length === 0) {
      setSpotlightIndex(0);
      return;
    }
    setSpotlightIndex((i) => Math.min(i, spotlightEntries.length - 1));
  }, [spotlightEntries.length]);

  const hadCloudResumeSpotlightRef = useRef(false);
  useEffect(() => {
    const hasResume = spotlightEntries.some((e) => e.kind === "cloudResume");
    if (hasResume && !hadCloudResumeSpotlightRef.current && topCategory === "all" && gameSubcategory === "root") {
      setGamesRootPlane("spotlight");
      setSpotlightIndex(0);
    }
    hadCloudResumeSpotlightRef.current = hasResume;
  }, [spotlightEntries, topCategory, gameSubcategory]);

  const gameCategoryPreviewById = useMemo(() => {
    const isNonEmptyString = (value: string | undefined): value is string => typeof value === "string" && value.length > 0;
    const randomize = (arr: string[]): string[] => {
      const copy = [...arr];
      for (let i = copy.length - 1; i > 0; i--) {
        const j = Math.floor(Math.random() * (i + 1));
        [copy[i], copy[j]] = [copy[j], copy[i]];
      }
      return copy;
    };
    const toFilledPreview = (covers: string[]): string[] => {
      const unique = Array.from(new Set(covers.filter(isNonEmptyString)));
      if (unique.length === 0) return [];
      const randomized = randomize(unique);
      return randomized.slice(0, PREVIEW_TILE_COUNT);
    };

    const previews: Record<string, string[]> = {};
    previews.all = toFilledPreview(gamesSortedByRecent.map((g) => g.imageUrl).filter(isNonEmptyString));
    previews.favorites = toFilledPreview(favoriteGames.map((g) => g.imageUrl).filter(isNonEmptyString));
    for (const genre of allGenres) {
      const key = `genre:${genre}`;
      previews[key] = gamesSortedByRecent
        .filter((g) => g.genres?.includes(genre))
        .map((g) => g.imageUrl)
        .filter(isNonEmptyString);
      previews[key] = toFilledPreview(previews[key]);
    }
    return previews;
  }, [allGenres, favoriteGames, gamesSortedByRecent]);

  const parallaxBackdropTiles = useMemo(() => {
    const isNonEmptyString = (value: string | undefined): value is string => typeof value === "string" && value.length > 0;
    const unique = Array.from(new Set(games.map((g) => g.imageUrl).filter(isNonEmptyString)));
    if (unique.length === 0) return [] as Array<{
      src: string;
      lane: 0 | 1 | 2;
      left: number;
      delaySec: number;
      scale: number;
      xFrom: number;
      xTo: number;
      rotFrom: number;
      rotTo: number;
    }>;
    const shuffled = [...unique];
    for (let i = shuffled.length - 1; i > 0; i--) {
      const j = Math.floor(Math.random() * (i + 1));
      [shuffled[i], shuffled[j]] = [shuffled[j], shuffled[i]];
    }
    return shuffled.slice(0, 16).map((src, idx) => {
      const lane = (idx % 3) as 0 | 1 | 2;
      const drift = 6 + Math.random() * 12;
      const rotStart = -6 + Math.random() * 6;
      const rotEnd = 1 + Math.random() * 8;
      return {
        src,
        lane,
        left: 4 + Math.random() * 88,
        delaySec: -(Math.random() * 54),
        scale: 0.88 + Math.random() * 0.34,
        xFrom: -drift,
        xTo: drift,
        rotFrom: rotStart,
        rotTo: rotEnd,
      };
    });
  }, [games]);

  const selectedIndex = useMemo(() => {
    const index = categorizedGames.findIndex((game) => game.id === selectedGameId);
    return index >= 0 ? index : 0;
  }, [categorizedGames, selectedGameId]);

  const selectedGame = useMemo(() => categorizedGames[selectedIndex] ?? null, [categorizedGames, selectedIndex]);

  useEffect(() => {
    if (!gamesHubOpen || !selectedGame?.title?.trim()) {
      setGameHubScreenshotUrls([]);
      return;
    }
    if (typeof window.openNow?.listMediaByGame !== "function") {
      setGameHubScreenshotUrls([]);
      return;
    }

    let cancelled = false;
    const titleArg = selectedGame.title.trim();

    void (async () => {
      try {
        const listing = await window.openNow.listMediaByGame({ gameTitle: titleArg });
        if (cancelled) return;

        const rows = [...(listing.screenshots ?? [])].sort((a, b) => b.createdAtMs - a.createdAtMs);
        const urls: string[] = [];

        for (const s of rows) {
          let u = s.thumbnailDataUrl || s.dataUrl;
          if (!u && typeof window.openNow?.getMediaThumbnail === "function") {
            try {
              u = (await window.openNow.getMediaThumbnail({ filePath: s.filePath })) ?? undefined;
            } catch {
              u = undefined;
            }
          }
          if (u) urls.push(u);
        }

        if (!cancelled) setGameHubScreenshotUrls(urls);
      } catch {
        if (!cancelled) setGameHubScreenshotUrls([]);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [gamesHubOpen, selectedGame?.id, selectedGame?.title]);

  const selectedVariantId = useMemo(() => {
    if (!selectedGame) return "";
    const current = selectedVariantByGameId[selectedGame.id];
    return current ?? selectedGame.variants[0]?.id ?? "";
  }, [selectedGame, selectedVariantByGameId]);

  const showCurrentDetail = topCategory === "current" && Boolean(currentStreamingGame);
  const detailVisible = showCurrentDetail;

  const gamesShelfBrowseActive = topCategory === "all" && gameSubcategory !== "root";
  const mediaShelfBrowseActive = topCategory === "media" && mediaSubcategory !== "root";
  const topLevelShelfActive =
    !gamesShelfBrowseActive &&
    !mediaShelfBrowseActive &&
    (topCategory === "settings" ||
      topCategory === "current" ||
      (topCategory === "media" && mediaSubcategory === "root") ||
      (topCategory === "all" && gameSubcategory === "root"));
  const gamesDualShelf =
    topCategory === "all" &&
    gameSubcategory === "root" &&
    (games.length > 0 || Boolean(cloudSessionResumable && onResumeCloudSession));
  const topLevelRowBehaviorActive = topLevelShelfActive && !(topCategory === "settings" && settingsSubcategory !== "root");
  const canEnterDetailRow = mediaShelfBrowseActive;
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

  const selectedCategoryLabel = useMemo(() => getCategoryLabel(topCategory, currentStreamingGame?.title).label, [topCategory, currentStreamingGame?.title]);
  const selectedTopLevelItemLabel = useMemo(() => {
    if (!topLevelShelfActive) return selectedCategoryLabel;
    if (topCategory === "all" && gameSubcategory === "root" && gamesRootPlane === "spotlight") {
      const entry = spotlightEntries[spotlightIndex];
      if (entry?.kind === "cloudResume") return entry.title;
      if (spotlightEntryHasGame(entry)) return entry.game.title;
      return "Recently played";
    }
    const active = displayItems[topLevelShelfIndex];
    if (topCategory === "all" && gameSubcategory === "root" && active?.label) return active.label;
    return selectedCategoryLabel;
  }, [topLevelShelfActive, selectedCategoryLabel, displayItems, topLevelShelfIndex, topCategory, gameSubcategory, gamesRootPlane, spotlightEntries, spotlightIndex]);
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
    if (!selectedGame || topCategory !== "all" || gameSubcategory === "root") return [];
    const fav = favoriteGameIdSet.has(selectedGame.id);
    const tiles: Array<{ id: string; title: string; subtitle: string; disabled?: boolean }> = [
      {
        id: "play",
        title: currentStreamingGame && currentStreamingGame.id !== selectedGame.id ? "Switch" : "Play",
        subtitle:
          inStreamMenu && currentStreamingGame && currentStreamingGame.id !== selectedGame.id
            ? `Switch from ${currentStreamingGame.title}`
            : currentStreamingGame && currentStreamingGame.id !== selectedGame.id
              ? "Switch to this title"
              : "Launch now",
      },
      {
        id: "favorite",
        title: fav ? "Remove favorite" : "Add favorite",
        subtitle: "Library",
      },
    ];
    if (selectedGame.variants.length > 1) {
      tiles.push({ id: "version", title: "Version", subtitle: "Cycle stream variant" });
    }
    tiles.push({ id: "activities", title: "Activities", subtitle: "Coming soon", disabled: true });
    tiles.push({ id: "progress", title: "Progress", subtitle: "Coming soon", disabled: true });
    return tiles;
  }, [topCategory, gameSubcategory, selectedGame, favoriteGameIdSet, currentStreamingGame, inStreamMenu]);

  useEffect(() => {
    const n = gamesHubTiles.length;
    if (n === 0) return;
    setGamesHubFocusIndex((i) => Math.max(0, Math.min(n - 1, i)));
  }, [gamesHubTiles.length, selectedGame?.id]);
  const focusMotionKey = useMemo(() => {
    if (topCategory === "all" && gameSubcategory === "root" && gamesRootPlane === "spotlight") {
      const entry = spotlightEntries[spotlightIndex];
      if (entry?.kind === "cloudResume") return `spotlight-resume-${entry.busy ? "busy" : "idle"}`;
      if (spotlightEntryHasGame(entry)) return `spotlight-${entry.game.id}`;
      return `spotlight-empty-${spotlightIndex}`;
    }
    if (topCategory === "all" && gameSubcategory !== "root") return `game-${selectedGame?.id ?? "none"}`;
    if (topCategory === "media" && mediaSubcategory !== "root") return `media-${selectedMediaIndex}-${mediaAssetItems[selectedMediaIndex]?.id ?? "none"}`;
    return `menu-${topCategory}-${topLevelShelfIndex}`;
  }, [topCategory, gameSubcategory, gamesRootPlane, spotlightEntries, spotlightIndex, selectedGame?.id, topLevelShelfIndex, mediaSubcategory, selectedMediaIndex, mediaAssetItems]);
  const selectedGameDescription = useMemo(() => {
    if (!selectedGame) return "";
    const description = selectedGame.longDescription?.trim() || selectedGame.description?.trim();
    return description || `${selectedGame.title} is ready to launch from your XMB library.`;
  }, [selectedGame]);
  const selectedGameSessionState = useMemo(() => {
    if (!selectedGame) return null;
    if (!currentStreamingGame) return "Ready To Launch";
    if (currentStreamingGame.id === selectedGame.id) return "Active Session";
    return "Ready To Switch";
  }, [currentStreamingGame, selectedGame]);

  useEffect(() => {
    setHeroTransitionMs(200);
    const t = window.setTimeout(() => setHeroTransitionMs(420), 420);
    return () => window.clearTimeout(t);
  }, [focusMotionKey]);

  useEffect(() => {
    if (topCategory !== "all") {
      setGamesRootPlane("spotlight");
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
    if (!gamesShelfBrowseActive && !mediaShelfBrowseActive && !topLevelShelfActive) {
      setListTranslateX(0);
      setSpotlightShelfTranslateX(0);
      setGamesRootMenuTranslateX(0);
    }
  }, [gamesShelfBrowseActive, mediaShelfBrowseActive, topLevelShelfActive]);

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

    if (!gamesRoot || !gamesDualShelf) {
      setSpotlightShelfTranslateX(0);
      setGamesRootMenuTranslateX(0);
    }

    if (gamesRoot && gamesDualShelf) {
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

    // Use offsetTop/offsetHeight to avoid per-item style reads on every navigation move.
    const activeChild = children[selectedIndex];
    const offset = activeChild.offsetTop + (activeChild.offsetHeight / 2);
    setListTranslateY(-offset);
    setListTranslateX(0);
  }, [
    selectedIndex,
    categorizedGames,
    gamesShelfBrowseActive,
    mediaShelfBrowseActive,
    topLevelShelfActive,
    topLevelShelfIndex,
    selectedMediaIndex,
    viewportWidth,
    topCategory,
    gameSubcategory,
    gamesDualShelf,
    spotlightIndex,
    spotlightEntries,
  ]);

  const throttledOnSelectGame = useCallback((id: string) => onSelectGame(id), [onSelectGame]);

  const toggleFavoriteForSelected = useCallback(() => {
    if (selectedGame) {
      onToggleFavoriteGame(selectedGame.id);
      playUiSound("confirm");
    }
  }, [onToggleFavoriteGame, playUiSound, selectedGame]);

  const controllerEventHandlersRef = useRef<{
    onDirection: (event: Event) => void;
    onShoulder: (event: Event) => void;
    onActivate: () => void;
    onSecondaryActivate: () => void;
    onTertiaryActivate: () => void;
    onCancel: (event: Event) => void;
    onKeyboard: (event: KeyboardEvent) => void;
  }>({
    onDirection: () => {},
    onShoulder: () => {},
    onActivate: () => {},
    onSecondaryActivate: () => {},
    onTertiaryActivate: () => {},
    onCancel: () => {},
    onKeyboard: () => {},
  });

  useEffect(() => {
    const applyDirection = (direction: Direction): void => {
      // When editing Theme RGB channels, use left/right to adjust value
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
      // When editing the bandwidth slider, use left/right to adjust value
      if (topCategory === "settings" && settingsSubcategory !== "root" && editingBandwidth) {
        const step = 5; // Mbps per left/right press
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

      if (
        gamesHubOpen &&
        topCategory === "all" &&
        gameSubcategory !== "root"
      ) {
        const n = gamesHubTiles.length;
        if (n === 0) return;
        if (direction === "left") {
          setGamesHubFocusIndex((i) => Math.max(0, i - 1));
          playUiSound("move");
          return;
        }
        if (direction === "right") {
          setGamesHubFocusIndex((i) => Math.min(n - 1, i + 1));
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
        const itemCount = displayItems.length;

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
        // In settings submenus, left/right move along the submenu list (same as horizontal shelves).
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
        // Cycle main categories (settings always resets to root)
        setCategoryIndex((prev) => (prev - 1 + TOP_CATEGORIES.length) % TOP_CATEGORIES.length);
        setSelectedSettingIndex(0);
        setSettingsSubcategory("root");
        setSelectedMediaIndex(0);
        setMediaSubcategory("root");
        setSelectedGameSubcategoryIndex(0);
        setGameSubcategory("root");
        setEditingBandwidth(false);
        setEditingThemeChannel(null);
        return;
      }
      if (direction === "right") {
        playUiSound("move");
        // Cycle main categories (settings always resets to root)
        setCategoryIndex((prev) => (prev + 1) % TOP_CATEGORIES.length);
        setSelectedSettingIndex(0);
        setSettingsSubcategory("root");
        setSelectedMediaIndex(0);
        setMediaSubcategory("root");
        setSelectedGameSubcategoryIndex(0);
        setGameSubcategory("root");
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
        return;
      }
    };

    const cycleTopCategory = (delta: number) => {
      setCategoryIndex((prev) => (prev + delta + TOP_CATEGORIES.length) % TOP_CATEGORIES.length);
      setSelectedSettingIndex(0);
      setSettingsSubcategory("root");
      setSelectedMediaIndex(0);
      setMediaSubcategory("root");
      setSelectedGameSubcategoryIndex(0);
      setGameSubcategory("root");
      setEditingBandwidth(false);
      setEditingThemeChannel(null);
      setEditingStreamVolume(false);
      setEditingStreamMicLevel(false);
      playUiSound("move");
    };

    const handler = (e: any) => {
      if (e.detail?.direction) applyDirection(e.detail.direction);
    };
    const shoulderHandler = (e: any) => {
      const direction = e?.detail?.direction as "prev" | "next" | undefined;
      if (!direction) return;
      if (gamesHubOpen) return;
      if (topCategory === "settings" && settingsSubcategory !== "root") return;
      if (editingBandwidth || editingThemeChannel || editingStreamVolume || editingStreamMicLevel) return;
      cycleTopCategory(direction === "prev" ? -1 : 1);
    };

    const openOptionsMenu = (): void => {
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
      if (entries.length === 0) return;
      entries.push({ id: "close", label: "Back" });
      setOptionsEntries(entries);
      setOptionsFocusIndex(0);
      setOptionsOpen(true);
      playUiSound("move");
    };

    const activateHandler = () => {
      if (optionsOpen && optionsEntries.length > 0) {
        const opt = optionsEntries[optionsFocusIndex];
        if (!opt) return;
        if (opt.id === "close") {
          setOptionsOpen(false);
          playUiSound("move");
          return;
        }
        if (opt.id === "play" && selectedGame) {
          onPlayGame(selectedGame);
          gamesHubReturnSnapshotRef.current = null;
          setGamesHubOpen(false);
          setOptionsOpen(false);
          playUiSound("confirm");
          return;
        }
        if (opt.id === "favorite" && selectedGame) {
          onToggleFavoriteGame(selectedGame.id);
          setOptionsOpen(false);
          playUiSound("confirm");
          return;
        }
        if (opt.id === "variant" && selectedGame && selectedGame.variants.length > 1) {
          const idx = selectedGame.variants.findIndex((v) => v.id === selectedVariantId);
          const next = selectedGame.variants[(idx + 1) % selectedGame.variants.length];
          onSelectGameVariant(selectedGame.id, next.id);
          setOptionsOpen(false);
          playUiSound("confirm");
          return;
        }
        if (opt.id === "openFolder") {
          const cur = mediaAssetItems[selectedMediaIndex];
          if (cur && typeof window.openNow?.showMediaInFolder === "function") {
            void window.openNow.showMediaInFolder({ filePath: cur.filePath });
          }
          setOptionsOpen(false);
          playUiSound("confirm");
          return;
        }
        if (opt.id === "openLibrary") {
          const entry = spotlightEntries[spotlightIndex];
          const g = spotlightEntryHasGame(entry) ? entry.game : null;
          if (g) {
            gamesHubReturnSnapshotRef.current = {
              gameSubcategory: "root",
              selectedGameSubcategoryIndex,
              gamesRootPlane,
              spotlightIndex,
              restoreSelectedGameId: g.id,
            };
            setLastRootGameIndex(selectedGameSubcategoryIndex);
            setGameSubcategory("all");
            throttledOnSelectGame(g.id);
            setGamesHubOpen(true);
            setGamesHubFocusIndex(0);
            setPs5Row("main");
            setOptionsOpen(false);
            playUiSound("confirm");
          }
          return;
        }
        return;
      }

      if (
        gamesHubOpen &&
        topCategory === "all" &&
        gameSubcategory !== "root" &&
        selectedGame
      ) {
        const tile = gamesHubTiles[gamesHubFocusIndex];
        if (!tile || tile.disabled) {
          playUiSound("move");
          return;
        }
        if (tile.id === "play") {
          onPlayGame(selectedGame);
          gamesHubReturnSnapshotRef.current = null;
          setGamesHubOpen(false);
          setGamesHubFocusIndex(0);
          playUiSound("confirm");
          return;
        }
        if (tile.id === "favorite") {
          onToggleFavoriteGame(selectedGame.id);
          playUiSound("confirm");
          return;
        }
        if (tile.id === "version" && selectedGame.variants.length > 1) {
          const idx = selectedGame.variants.findIndex((v) => v.id === selectedVariantId);
          const next = selectedGame.variants[(idx + 1) % selectedGame.variants.length];
          onSelectGameVariant(selectedGame.id, next.id);
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

        // Placeholder detail cards: confirm and return to the main row.
        setPs5Row("main");
        playUiSound("confirm");
        return;
      }

      // If currently editing bandwidth, A confirms and exits edit mode
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
      if (topCategory === "current") {
        const item = displayItems[selectedSettingIndex];
        if (item?.id === "resume" && currentStreamingGame && onResumeGame) {
          onResumeGame(currentStreamingGame);
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
      if (topCategory === "settings") {
        const setting = displayItems[selectedSettingIndex];
        // Enter subcategory if at root and selecting network/audio/system
        if (settingsSubcategory === "root" && setting && (setting.id === "network" || setting.id === "audio" || setting.id === "video" || setting.id === "system")) {
          setLastRootSettingIndex(selectedSettingIndex);
          if (setting.id === "network") setSettingsSubcategory("Network");
          if (setting.id === "audio") setSettingsSubcategory("Audio");
          if (setting.id === "video") setSettingsSubcategory("Video");
          if (setting.id === "system") setSettingsSubcategory("System");
          setSelectedSettingIndex(0);
          playUiSound("confirm");
          return;
        }
        if (settingsSubcategory === "root" && setting?.id === "exitApp") {
          if (onExitApp) {
            onExitApp();
          } else if (window.openNow?.quitApp) {
            void window.openNow.quitApp();
          }
          playUiSound("confirm");
          return;
        }
        if (settingsSubcategory === "System" && setting?.id === "theme") {
          setLastSystemMenuIndex(selectedSettingIndex);
          setSettingsSubcategory("Theme");
          setSelectedSettingIndex(0);
          setEditingThemeChannel(null);
          playUiSound("confirm");
          return;
        }
        if (settingsSubcategory === "Theme") {
          const item = displayItems[selectedSettingIndex];
          if (item?.id === "themeColor") {
            setLastThemeRootIndex(selectedSettingIndex);
            setSettingsSubcategory("ThemeColor");
            setSelectedSettingIndex(0);
            setEditingThemeChannel(null);
            playUiSound("confirm");
            return;
          }
          if (item?.id === "themeStyle") {
            setLastThemeRootIndex(selectedSettingIndex);
            setSettingsSubcategory("ThemeStyle");
            const resolvedStyle = sanitizeControllerThemeStyle(settings.controllerThemeStyle);
            const idx = CONTROLLER_THEME_STYLE_ORDER.indexOf(resolvedStyle);
            setSelectedSettingIndex(idx >= 0 ? idx : 0);
            playUiSound("confirm");
            return;
          }
          return;
        }
        if (settingsSubcategory === "ThemeStyle") {
          const row = displayItems[selectedSettingIndex];
          if (row?.id && onSettingChange) {
            onSettingChange("controllerThemeStyle", row.id as ControllerThemeStyle);
            playUiSound("confirm");
          }
          return;
        }
        // In subcategory, A toggles values like X does
        if (settingsSubcategory !== "root") {
          if (setting?.id === "exitControllerMode") {
            if (onExitControllerMode) {
              onExitControllerMode();
            } else if (onSettingChange) {
              onSettingChange("controllerMode" as any, false as any);
            }
            playUiSound("confirm");
            const nextSettingsIndex = currentStreamingGame ? 0 : 1;
            setCategoryIndex(nextSettingsIndex);
            setSelectedSettingIndex(0);
            return;
          }
          secondaryActivateHandler();
          return;
        }
        playUiSound("confirm");
      } else if (topCategory === "media") {
        const item = displayItems[selectedMediaIndex];
        if (mediaSubcategory === "root" && item && (item.id === "videos" || item.id === "screenshots")) {
          setLastRootMediaIndex(selectedMediaIndex);
          setMediaSubcategory(item.id === "videos" ? "Videos" : "Screenshots");
          setSelectedMediaIndex(0);
          playUiSound("confirm");
          return;
        }

        if (mediaSubcategory !== "root") {
          const selectedMedia = mediaAssetItems[selectedMediaIndex];
          if (selectedMedia && typeof window.openNow?.showMediaInFolder === "function") {
            void window.openNow.showMediaInFolder({ filePath: selectedMedia.filePath });
            playUiSound("confirm");
            return;
          }
        }

        playUiSound("confirm");
      } else if (topCategory === "all") {
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
              return;
            }
            if (spotlightEntryHasGame(entry)) {
              const g = entry.game;
              gamesHubReturnSnapshotRef.current = {
                gameSubcategory: "root",
                selectedGameSubcategoryIndex,
                gamesRootPlane,
                spotlightIndex,
                restoreSelectedGameId: g.id,
              };
              setLastRootGameIndex(selectedGameSubcategoryIndex);
              setGameSubcategory("all");
              throttledOnSelectGame(g.id);
              setGamesHubOpen(true);
              setGamesHubFocusIndex(0);
              setPs5Row("main");
              playUiSound("confirm");
              return;
            }
          }
          const item = displayItems[selectedGameSubcategoryIndex];
          if (item) {
            setLastRootGameIndex(selectedGameSubcategoryIndex);
            setGameSubcategory(item.id as GameSubcategory);
            setSelectedGameSubcategoryIndex(0);
            playUiSound("confirm");
          }
          return;
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
      } else if (selectedGame) {
        onPlayGame(selectedGame);
        playUiSound("confirm");
      }
    };

    const secondaryActivateHandler = () => {
      if (optionsOpen) return;
      if (gamesHubOpen) return;
      if (gamesShelfBrowseActive && gameSubcategory === "all") {
        setLibrarySortId((prev) => {
          const order: LibrarySortId[] = ["recent", "favoritesFirst", "az", "za"];
          const i = order.indexOf(prev);
          return order[(i + 1) % order.length] ?? "recent";
        });
        playUiSound("move");
        return;
      }
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
        if (topCategory === "current") {
          return;
        }
        if (topCategory === "settings") {
          if (settingsSubcategory === "ThemeStyle" || settingsSubcategory === "Theme") return;
          // X button cycles through setting values (no-op for exit actions or subcategory items at root)
          const setting = displayItems[selectedSettingIndex];
          if (!setting || !onSettingChange) return;
          if (setting.id === "exitApp" || setting.id === "exitControllerMode") return;
          // Skip X cycling for subcategory items at root
          if (settingsSubcategory === "root" && (setting.id === "network" || setting.id === "audio" || setting.id === "video" || setting.id === "system")) return;

          if (
            settingsSubcategory === "ThemeColor" &&
            (setting.id === "themeR" || setting.id === "themeG" || setting.id === "themeB")
          ) {
            const ch = setting.id === "themeR" ? "r" : setting.id === "themeG" ? "g" : "b";
            setEditingThemeChannel(ch);
            playUiSound("move");
            return;
          }

          // Microphone device cycling
          if (setting.id === "microphone") {
            const current = (settings as any).microphoneDeviceId as string | undefined;
            const list = microphoneDevices.length > 0 ? microphoneDevices : [{ deviceId: "", label: "Default" }];
            const ids = list.map(d => d.deviceId);
            const curIdx = ids.indexOf(current ?? "");
            const nextIdx = (curIdx + 1) % ids.length;
            onSettingChange("microphoneDeviceId" as any, ids[nextIdx] as any);
            playUiSound("move");
            return;
          }
          
          if (setting.id === "aspectRatio" && aspectRatioOptions.length > 0) {
            const currentIdx = aspectRatioOptions.indexOf(settings.aspectRatio || "16:9");
            const nextIdx = (currentIdx + 1) % aspectRatioOptions.length;
            onSettingChange("aspectRatio", aspectRatioOptions[nextIdx] as any);
            playUiSound("move");
          } else if (setting.id === "resolution" && resolutionOptions.length > 0) {
            const currentIdx = resolutionOptions.indexOf(settings.resolution || "1920x1080");
            const nextIdx = (currentIdx + 1) % resolutionOptions.length;
            onSettingChange("resolution", resolutionOptions[nextIdx]);
            playUiSound("move");
          } else if (setting.id === "fps" && fpsOptions.length > 0) {
            const currentIdx = fpsOptions.indexOf(settings.fps || 60);
            const nextIdx = (currentIdx + 1) % fpsOptions.length;
            onSettingChange("fps", fpsOptions[nextIdx]);
            playUiSound("move");
          } else if (setting.id === "codec" && codecOptions.length > 0) {
            const currentIdx = codecOptions.indexOf(settings.codec || "H264");
            const nextIdx = (currentIdx + 1) % codecOptions.length;
            onSettingChange("codec", codecOptions[nextIdx] as any);
            playUiSound("move");
          } else if (setting.id === "sounds") {
            onSettingChange("controllerUiSounds", !(settings.controllerUiSounds || false));
            playUiSound("move");
          } else if (setting.id === "autoLoad") {
            onSettingChange("autoLoadControllerLibrary", !((settings as any).autoLoadControllerLibrary || false));
            playUiSound("move");
          } else if (setting.id === "autoFullScreen") {
            onSettingChange("autoFullScreen" as any, !((settings as any).autoFullScreen || false));
            playUiSound("move");
          } else if (setting.id === "backgroundAnimations") {
            onSettingChange("controllerBackgroundAnimations" as any, !((settings as any).controllerBackgroundAnimations || false));
            playUiSound("move");
          } else if (setting.id === "l4s") {
            onSettingChange("enableL4S" as any, !((settings as any).enableL4S || false));
            playUiSound("move");
          } else if (setting.id === "cloudGsync") {
            onSettingChange("enableCloudGsync" as any, !((settings as any).enableCloudGsync || false));
            playUiSound("move");
          }
          else if (setting.id === "bandwidth") {
            // Enter bandwidth edit mode so d-pad left/right adjust value
            setEditingBandwidth(true);
            playUiSound("move");
          }
          return;
        }
    };

    const tertiaryActivateHandler = () => {
      if (optionsOpen) return;
      openOptionsMenu();
    };

    const cancelHandler = (e: Event) => {
      if (optionsOpen) {
        setOptionsOpen(false);
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
      // Circle/B button goes back from subcategory to root.
      // Prevent default to signal the App-level back handler that we've handled it.
      if (topCategory === "settings" && settingsSubcategory !== "root") {
        if (editingBandwidth) {
          setEditingBandwidth(false);
          playUiSound("move");
          e.preventDefault();
          return;
        }
        if (editingThemeChannel) {
          setEditingThemeChannel(null);
          playUiSound("move");
          e.preventDefault();
          return;
        }
        if (settingsSubcategory === "ThemeColor") {
          setSettingsSubcategory("Theme");
          setSelectedSettingIndex(lastThemeRootIndex);
          playUiSound("move");
          e.preventDefault();
          return;
        }
        if (settingsSubcategory === "ThemeStyle") {
          setSettingsSubcategory("Theme");
          setSelectedSettingIndex(lastThemeRootIndex);
          playUiSound("move");
          e.preventDefault();
          return;
        }
        if (settingsSubcategory === "Theme") {
          setSettingsSubcategory("System");
          setSelectedSettingIndex(lastSystemMenuIndex);
          playUiSound("move");
          e.preventDefault();
          return;
        }
        setSettingsSubcategory("root");
        setSelectedSettingIndex(lastRootSettingIndex);
        playUiSound("move");
        e.preventDefault();
        return;
      }
      if (topCategory === "media" && mediaSubcategory !== "root") {
        setMediaSubcategory("root");
        setSelectedMediaIndex(lastRootMediaIndex);
        playUiSound("move");
        e.preventDefault();
        return;
      }
      if (topCategory === "all" && gameSubcategory !== "root") {
        if (gamesHubOpen) {
          playUiSound("move");
          e.preventDefault();
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
          }
          return;
        }
        setGameSubcategory("root");
        setSelectedGameSubcategoryIndex(lastRootGameIndex);
        playUiSound("move");
        e.preventDefault();
        return;
      }

      // At top-level views, Back/Cancel is intentionally a no-op.
      e.preventDefault();
    };

    const kbdHandler = (e: KeyboardEvent) => {
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
        activateHandler();
        return;
      }
      if (e.key.toLowerCase() === "x") {
        e.preventDefault();
        secondaryActivateHandler();
        return;
      }
      if (e.key.toLowerCase() === "y") {
        e.preventDefault();
        tertiaryActivateHandler();
        return;
      }
      if (e.key.toLowerCase() === "f") {
        e.preventDefault();
        secondaryActivateHandler();
        return;
      }
      if (e.key.toLowerCase() === "o") {
        e.preventDefault();
        tertiaryActivateHandler();
        return;
      }
      if (e.key.toLowerCase() === "q" && topLevelRowBehaviorActive && !gamesHubOpen) {
        e.preventDefault();
        cycleTopCategory(-1);
        return;
      }
      if (e.key.toLowerCase() === "e" && topLevelRowBehaviorActive && !gamesHubOpen) {
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
          cancelHandler(e);
          return;
        }
        if (topCategory === "media" && mediaSubcategory !== "root") {
          cancelHandler(e);
          return;
        }
        if (topCategory === "all" && gameSubcategory !== "root") {
          cancelHandler(e);
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

        // Top-level back is intentionally a no-op.
        e.preventDefault();
        return;
      }
    };

    controllerEventHandlersRef.current = {
      onDirection: handler as (event: Event) => void,
      onShoulder: shoulderHandler as (event: Event) => void,
      onActivate: activateHandler,
      onSecondaryActivate: secondaryActivateHandler,
      onTertiaryActivate: tertiaryActivateHandler,
      onCancel: cancelHandler,
      onKeyboard: kbdHandler,
    };
  }, [
    isLoading,
    TOP_CATEGORIES.length,
    categorizedGames,
    selectedIndex,
    selectedGame,
    selectedGameId,
    selectedVariantId,
    onPlayGame,
    onSelectGameVariant,
    onOpenSettings,
    onToggleFavoriteGame,
    playUiSound,
    throttledOnSelectGame,
    toggleFavoriteForSelected,
    topCategory,
    selectedSettingIndex,
    selectedMediaIndex,
    selectedGameSubcategoryIndex,
    displayItems,
    mediaAssetItems,
    mediaSubcategory,
    gameSubcategory,
    settings,
    settingsBySubcategory,
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
    onResumeGame,
    onResumeCloudSession,
    onCloseGame,
    onExitControllerMode,
    onExitApp,
    editingBandwidth,
    editingThemeChannel,
    gamesShelfBrowseActive,
    mediaShelfBrowseActive,
    topLevelShelfActive,
    topLevelRowBehaviorActive,
    topLevelShelfIndex,
    canEnterDetailRow,
    canEnterTopRow,
    ps5Row,
    detailRailIndex,
    detailRailItems,
    librarySortId,
    optionsOpen,
    optionsFocusIndex,
    optionsEntries.length,
    gamesRootPlane,
    spotlightIndex,
    spotlightEntries,
    gamesDualShelf,
    favoriteGameIdSet,
    microphoneDevices,
    gamesHubOpen,
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
    controllerType,
  ]);

  useEffect(() => {
    const directionListener = (event: Event) => controllerEventHandlersRef.current.onDirection(event);
    const shoulderListener = (event: Event) => controllerEventHandlersRef.current.onShoulder(event);
    const activateListener = () => controllerEventHandlersRef.current.onActivate();
    const secondaryActivateListener = () => controllerEventHandlersRef.current.onSecondaryActivate();
    const tertiaryActivateListener = () => controllerEventHandlersRef.current.onTertiaryActivate();
    const cancelListener = (event: Event) => controllerEventHandlersRef.current.onCancel(event);
    const keyboardListener = (event: KeyboardEvent) => controllerEventHandlersRef.current.onKeyboard(event);

    window.addEventListener("opennow:controller-direction", directionListener);
    window.addEventListener("opennow:controller-shoulder", shoulderListener);
    window.addEventListener("opennow:controller-activate", activateListener);
    window.addEventListener("opennow:controller-secondary-activate", secondaryActivateListener);
    window.addEventListener("opennow:controller-tertiary-activate", tertiaryActivateListener);
    window.addEventListener("opennow:controller-cancel", cancelListener);
    window.addEventListener("keydown", keyboardListener);
    return () => {
      window.removeEventListener("opennow:controller-direction", directionListener);
      window.removeEventListener("opennow:controller-shoulder", shoulderListener);
      window.removeEventListener("opennow:controller-activate", activateListener);
      window.removeEventListener("opennow:controller-secondary-activate", secondaryActivateListener);
      window.removeEventListener("opennow:controller-tertiary-activate", tertiaryActivateListener);
      window.removeEventListener("opennow:controller-cancel", cancelListener);
      window.removeEventListener("keydown", keyboardListener);
    };
  }, []);

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

  const themeStyleSafe = sanitizeControllerThemeStyle(settings.controllerThemeStyle);
  const selectedMediaItem = topCategory === "media" && mediaSubcategory !== "root"
    ? mediaAssetItems[selectedMediaIndex] ?? null
    : null;
  const mediaHubSlots = useMemo(() => {
    if (mediaLoading || mediaError || mediaSubcategory === "root") return [];
    const placeholdersNeeded = Math.max(0, MEDIA_HUB_MIN_TILES - mediaAssetItems.length);
    const filled = mediaAssetItems.map((item) => ({ kind: "asset" as const, item }));
    const templates = mediaSubcategory === "Videos"
      ? MEDIA_VIDEO_PLACEHOLDER_TEMPLATES
      : MEDIA_SCREENSHOT_PLACEHOLDER_TEMPLATES;
    const placeholders = Array.from({ length: placeholdersNeeded }, (_, idx) => ({
      kind: "placeholder" as const,
      id: `placeholder-${mediaSubcategory}-${idx}`,
      title: templates[idx % templates.length]?.title ?? "Capture slot available",
      subtitle: templates[idx % templates.length]?.subtitle ?? "Capture gameplay to populate",
    }));
    return [...filled, ...placeholders];
  }, [mediaAssetItems, mediaError, mediaLoading, mediaSubcategory]);
  const mediaHubPlaceholderCount = Math.max(0, mediaHubSlots.length - mediaAssetItems.length);
  const heroBackdropUrl = useMemo(() => {
    if (topCategory === "all" && gameSubcategory === "root" && gamesRootPlane === "spotlight" && spotlightEntries.length > 0) {
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
    if (topCategory === "current") return currentStreamingGame?.imageUrl ?? null;
    if (topCategory === "media") {
      if (selectedMediaItem?.thumbnailDataUrl) return selectedMediaItem.thumbnailDataUrl;
      if (selectedMediaItem?.dataUrl) return selectedMediaItem.dataUrl;
      return selectedMediaItem ? mediaThumbById[selectedMediaItem.id] ?? null : null;
    }
    if (currentStreamingGame?.imageUrl) return currentStreamingGame.imageUrl;
    return selectedGame?.imageUrl ?? null;
  }, [topCategory, gameSubcategory, gamesRootPlane, spotlightEntries, spotlightIndex, selectedGame, currentStreamingGame, selectedMediaItem, mediaThumbById]);
  const themeRgbResolved = settings.controllerThemeColor ?? { r: 124, g: 241, b: 177 };
  const wrapperThemeVars = {
    "--xmb-theme-r": String(themeRgbResolved.r),
    "--xmb-theme-g": String(themeRgbResolved.g),
    "--xmb-theme-b": String(themeRgbResolved.b),
    "--xmb-hero-crossfade-ms": `${heroTransitionMs}ms`,
  } as React.CSSProperties;

  const wrapperClassName = `xmb-wrapper xmb-theme-${themeStyleSafe} ${settings.controllerBackgroundAnimations ? "xmb-animate" : "xmb-static"} ${isEntering ? "xmb-entering" : "xmb-ready"} xmb-layout--ps5-home`;
  const wrapperClassNameWithRow = `${wrapperClassName} xmb-row-${ps5Row} ${topCategory === "settings" ? "xmb-ps5-section-settings" : ""} ${topCategory === "settings" && settingsSubcategory === "root" ? "xmb-ps5-settings-root" : ""} ${topCategory === "settings" && settingsSubcategory !== "root" ? "xmb-ps5-settings-sub" : ""}`;

  const themeRgbForTrack = settings.controllerThemeColor ?? { r: 124, g: 241, b: 177 };
  const maxBitrateMbpsForTrack = settings.maxBitrateMbps ?? 75;
  const menuShelfTranslateX = gamesDualShelf ? gamesRootMenuTranslateX : listTranslateX;

  const topLevelMenuTrack = useMemo(() => (
    <div
      ref={itemsContainerRef}
      className={`xmb-ps5-shelf-track xmb-ps5-shelf-track--menu ${topCategory === "all" && gameSubcategory === "root" ? "xmb-ps5-shelf-track--games-root" : ""} ${topCategory === "settings" ? "xmb-ps5-shelf-track--settings" : ""}`}
      role="listbox"
      aria-label={
        topCategory === "current" ? "Current game actions" : topCategory === "settings" ? "Controller settings" : topCategory === "all" ? "Game categories" : "Media categories"
      }
      style={{ transform: `translateX(${menuShelfTranslateX}px)` }}
    >
      {displayItems.map((item, idx) => {
        const isActive = idx === topLevelShelfIndex;
        const themeChannelForRow =
          item.id === "themeR" ? "r" : item.id === "themeG" ? "g" : item.id === "themeB" ? "b" : null;
        const themeRgbLive = themeRgbForTrack;
        const isGameRootTile = topCategory === "all" && gameSubcategory === "root";
        const isCurrentResumeTile = topCategory === "current" && item.id === "resume";
        const isSettingsTile = topCategory === "settings";
        const previewThumbs = isGameRootTile ? (gameCategoryPreviewById[item.id] ?? []) : [];
        return (
          <div
            key={item.id}
            className={`xmb-ps5-menu-tile ${isActive ? "active" : ""} ${isCurrentResumeTile ? "xmb-ps5-menu-tile--resume" : ""} ${isSettingsTile ? "xmb-ps5-menu-tile--settings" : ""} ${isSettingsTile && settingsSubcategory === "root" ? "xmb-ps5-menu-tile--settings-root" : ""}`}
            role="option"
            aria-selected={isActive}
          >
            {isCurrentResumeTile ? (
              <div className="xmb-ps5-menu-resume-preview" aria-hidden>
                {currentStreamingGame?.imageUrl ? (
                  <img src={currentStreamingGame.imageUrl} alt="" className="xmb-ps5-menu-resume-image" decoding="async" />
                ) : (
                  <div className="xmb-ps5-menu-resume-image xmb-ps5-menu-resume-image--placeholder" />
                )}
                <div className="xmb-ps5-menu-resume-overlay">
                  <span className="xmb-ps5-menu-resume-badge">Live Snapshot</span>
                </div>
              </div>
            ) : null}
            {isGameRootTile ? (
              <div className="xmb-ps5-menu-thumb-row" aria-hidden>
                {previewThumbs.map((src, i) => (
                  <div key={`${item.id}-${i}`} className="xmb-ps5-menu-thumb">
                    <img src={src} alt="" className="xmb-ps5-menu-thumb-img" {...SHELF_IMAGE_PROPS} />
                  </div>
                ))}
                {Array.from({ length: Math.max(0, PREVIEW_TILE_COUNT - previewThumbs.length) }).map((_, i) => (
                  <div key={`${item.id}-empty-${i}`} className="xmb-ps5-menu-thumb xmb-ps5-menu-thumb--empty" />
                ))}
              </div>
            ) : null}
            <div className="xmb-ps5-menu-title">{item.label}</div>
            {item.value ? (
              <div className="xmb-ps5-menu-meta">
                {item.id === "bandwidth" && settingsSubcategory !== "root" ? (
                  <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
                    <input
                      type="range"
                      min={1}
                      max={150}
                      step={1}
                      value={maxBitrateMbpsForTrack}
                      onChange={(e) => onSettingChange && onSettingChange("maxBitrateMbps" as any, Number(e.target.value) as any)}
                      aria-label="Bandwidth Limit (Mbps)"
                      style={editingBandwidth ? { outline: "2px solid rgba(255,255,255,0.2)" } : undefined}
                    />
                    <span className="xmb-game-meta-chip">{`${maxBitrateMbpsForTrack} Mbps`}{editingBandwidth ? " • Editing" : ""}</span>
                  </div>
                ) : themeChannelForRow && settingsSubcategory === "ThemeColor" ? (
                  <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
                    <input
                      type="range"
                      min={0}
                      max={255}
                      step={1}
                      value={themeRgbLive[themeChannelForRow]}
                      onChange={(e) =>
                        onSettingChange &&
                        onSettingChange("controllerThemeColor", {
                          ...themeRgbLive,
                          [themeChannelForRow]: clampRgbByte(Number(e.target.value)),
                        })
                      }
                      aria-label={`Theme ${item.label}`}
                      style={editingThemeChannel === themeChannelForRow ? { outline: "2px solid rgba(255,255,255,0.2)" } : undefined}
                    />
                    <span className="xmb-game-meta-chip">
                      {item.value}
                      {editingThemeChannel === themeChannelForRow ? " • Editing" : ""}
                    </span>
                  </div>
                ) : item.id === "streamMicLevel" && inStreamMenu ? (
                  <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
                    <input
                      type="range"
                      min={0}
                      max={100}
                      step={1}
                      value={Math.round((streamMenuMicLevel ?? 1) * 100)}
                      onChange={(e) =>
                        onStreamMenuMicLevelChange?.(Math.max(0, Math.min(1, Number(e.target.value) / 100)))
                      }
                      aria-label="Microphone level"
                      style={editingStreamMicLevel ? { outline: "2px solid rgba(255,255,255,0.2)" } : undefined}
                    />
                    <span className="xmb-game-meta-chip">
                      {`${Math.round((streamMenuMicLevel ?? 1) * 100)}%`}
                      {editingStreamMicLevel
                        ? " • Editing ←/→"
                        : controllerType === "ps"
                          ? " • □ to adjust"
                          : " • X to adjust"}
                    </span>
                  </div>
                ) : item.id === "streamVolume" && inStreamMenu ? (
                  <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
                    <input
                      type="range"
                      min={0}
                      max={100}
                      step={1}
                      value={Math.round((streamMenuVolume ?? 1) * 100)}
                      onChange={(e) =>
                        onStreamMenuVolumeChange?.(Math.max(0, Math.min(1, Number(e.target.value) / 100)))
                      }
                      aria-label="Stream volume"
                      style={editingStreamVolume ? { outline: "2px solid rgba(255,255,255,0.2)" } : undefined}
                    />
                    <span className="xmb-game-meta-chip">
                      {`${Math.round((streamMenuVolume ?? 1) * 100)}%`}
                      {editingStreamVolume
                        ? " • Editing ←/→"
                        : controllerType === "ps"
                          ? " • □ to adjust"
                          : " • X to adjust"}
                    </span>
                  </div>
                ) : (
                  <span className="xmb-game-meta-chip">{item.value}</span>
                )}
              </div>
            ) : null}
          </div>
        );
      })}
    </div>
  ), [
    topCategory,
    gameSubcategory,
    menuShelfTranslateX,
    displayItems,
    topLevelShelfIndex,
    gameCategoryPreviewById,
    currentStreamingGame?.imageUrl,
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
  ]);

  if (isLoading && topCategory !== "settings" && topCategory !== "current" && topCategory !== "media") {
    return (
      <div className={wrapperClassNameWithRow} style={wrapperThemeVars}>
        <div className="xmb-bg-layer">
          <div className="xmb-bg-gradient" />
        </div>
        <Ps5LoadingScreen
          title="Loading your library"
          subtitle="Please wait"
          backdropImageUrl={currentStreamingGame?.imageUrl}
        />
      </div>
    );
  }

  return (
    <div className={wrapperClassNameWithRow} style={wrapperThemeVars}>
      {inStreamMenu && endSessionConfirm ? (
        <div className="xmb-end-session-banner" role="status">
          End session? Press Enter again to confirm, or Back to cancel.
        </div>
      ) : null}
      <div className="xmb-bg-layer">
        {parallaxBackdropTiles.length > 0 ? (
          <div className="xmb-ps5-parallax-field" aria-hidden>
            {parallaxBackdropTiles.map((tile, idx) => {
              return (
                <div
                  key={`${tile.src}-${idx}`}
                  className={`xmb-ps5-parallax-tile xmb-ps5-parallax-row-${tile.lane}`}
                  style={
                    {
                      backgroundImage: `url(${tile.src})`,
                      left: `${tile.left}%`,
                      "--parallax-delay": `${tile.delaySec}s`,
                      "--parallax-scale": String(tile.scale),
                      "--parallax-x-from": `${tile.xFrom}vw`,
                      "--parallax-x-to": `${tile.xTo}vw`,
                      "--parallax-rot-from": `${tile.rotFrom}deg`,
                      "--parallax-rot-to": `${tile.rotTo}deg`,
                    } as React.CSSProperties
                  }
                />
              );
            })}
          </div>
        ) : null}
        {heroBackdropUrl ? (
          <div
            className={`xmb-ps5-hero-art ${settings.controllerBackgroundAnimations ? "xmb-ps5-hero-art--motion" : ""}`}
            style={{ backgroundImage: `url(${heroBackdropUrl})` }}
            aria-hidden
          />
        ) : null}
        <div className="xmb-bg-gradient" />
        <div className="xmb-bg-overlay" />
      </div>

      <div className="xmb-top-right">
        <div className="xmb-clock-wrap">
          <CurrentClock className="xmb-clock" />
          <div className="xmb-remaining-playtime"><RemainingPlaytimeIndicator subscriptionInfo={subscriptionInfo} startedAtMs={sessionStartedAtMs} active={isStreaming} className="xmb-remaining-playtime-text" /></div>
        </div>
        <div className="xmb-user-badge">
          {userAvatarUrl ? (
            <img
              src={userAvatarUrl}
              alt={userName}
              className="xmb-user-avatar"
            />
          ) : (
            <div className="xmb-user-avatar" />
          )}
          <div className="xmb-user-name">{userName}</div>
        </div>
      </div>

      <div className="xmb-top-left">
        <div className="xmb-logo" aria-hidden>
          {/* Use import.meta URL to avoid needing image module typings */}
          <img src={new URL('../assets/opennow-logo.png', import.meta.url).toString()} alt="OpenNow" />
        </div>
      </div>

      <div
        className="xmb-categories-container"
        style={{ transform: `translate(${-categoryIndex * CATEGORY_STEP_PX - CATEGORY_ACTIVE_HALF_WIDTH_PX}px, -50%)` }}
      >
            {TOP_CATEGORIES.map((cat, idx) => {
              const isActive = idx === categoryIndex;
              // Use the label already populated on TOP_CATEGORIES so "current"
              // shows the streaming game's title when available.
              const label = cat.label;
              return (
                <div key={cat.id} className={`xmb-category-item ${isActive ? 'active' : ''}`}>
                  <div className="xmb-category-icon-wrap">{getCategoryIcon(cat.id)}</div>
                  <div className="xmb-category-label">{label}</div>
                </div>
              );
            })}
      </div>

      {topCategory === "all" && gameSubcategory !== "root" && gamesHubOpen && selectedGame ? (
        <ControllerGameHub
          game={selectedGame}
          screenshotUrls={gameHubScreenshotUrls}
          playtimeData={playtimeData}
          selectedVariantId={selectedVariantId}
          currentStreamingGame={currentStreamingGame}
          librarySortLabel={gameSubcategory === "all" ? LIBRARY_SORT_LABEL[librarySortId] : null}
          tiles={gamesHubTiles}
          focusIndex={gamesHubFocusIndex}
          inStreamMenu={inStreamMenu}
        />
      ) : null}

      {topCategory === "all" && gameSubcategory !== "root" && !gamesHubOpen && (
        <div className="xmb-ps5-stack xmb-ps5-media-hub">
          {!isLoading && categorizedGames.length === 0 ? (
            <div className="xmb-ps5-focus-meta" aria-live="polite" key="games-empty">
              <h2 className="xmb-ps5-focus-title">No games here</h2>
              <p className="xmb-ps5-focus-subtitle">Try another category or refresh your library.</p>
            </div>
          ) : selectedGame ? (
            <div className="xmb-ps5-focus-meta" aria-live="polite" key={focusMotionKey}>
              <h2 className="xmb-ps5-focus-title">{selectedGame.title}</h2>
              <div className="xmb-ps5-actions">
                <span className="xmb-ps5-action xmb-ps5-action--primary">Game hub</span>
                <span className="xmb-ps5-action">Options</span>
              </div>
              <div className="xmb-ps5-focus-chips">
                {gameSubcategory === "all" ? (
                  <span className="xmb-game-meta-chip xmb-game-meta-chip--sort">Sort: {LIBRARY_SORT_LABEL[librarySortId]}</span>
                ) : null}
                {(() => {
                  const record = playtimeData[selectedGame.id];
                  const totalSecs = record?.totalSeconds ?? 0;
                  const lastPlayedAt = record?.lastPlayedAt ?? null;
                  const sessionCount = record?.sessionCount ?? 0;
                  const playtimeLabel = formatPlaytime(totalSecs);
                  const lastPlayedLabel = formatLastPlayed(lastPlayedAt);
                  const vId = selectedVariantByGameId[selectedGame.id] || selectedGame.variants[0]?.id;
                  const variant = selectedGame.variants.find((v) => v.id === vId) || selectedGame.variants[0];
                  const storeName = getStoreDisplayName(variant?.store || "");
                  const genres = selectedGame.genres?.slice(0, 3) ?? [];
                  const tierLabel = selectedGame.membershipTierLabel;
                  return (
                    <>
                      {storeName ? <span className="xmb-game-meta-chip xmb-game-meta-chip--store">{storeName}</span> : null}
                      <span className="xmb-game-meta-chip xmb-game-meta-chip--playtime">
                        <Clock size={10} className="xmb-meta-icon" />
                        {playtimeLabel}
                      </span>
                      <span className="xmb-game-meta-chip xmb-game-meta-chip--last-played">
                        <Calendar size={10} className="xmb-meta-icon" />
                        {lastPlayedLabel}
                      </span>
                      {sessionCount > 0 ? (
                        <span className="xmb-game-meta-chip xmb-game-meta-chip--sessions">
                          <Repeat2 size={10} className="xmb-meta-icon" />
                          {sessionCount === 1 ? "1 session" : `${sessionCount} sessions`}
                        </span>
                      ) : null}
                      {genres.map((g) => (
                        <span key={g} className="xmb-game-meta-chip xmb-game-meta-chip--genre">
                          {sanitizeGenreName(g)}
                        </span>
                      ))}
                      {tierLabel ? <span className="xmb-game-meta-chip xmb-game-meta-chip--tier">{tierLabel}</span> : null}
                    </>
                  );
                })()}
              </div>
            </div>
          ) : null}
          <div className="xmb-ps5-shelf-label-row xmb-ps5-shelf-label-row--active xmb-ps5-shelf-label-row--games-list">
            <span className="xmb-ps5-shelf-label">Games</span>
          </div>
          <div className="xmb-ps5-shelf-viewport">
            <div
              ref={itemsContainerRef}
              className="xmb-ps5-shelf-track"
              role="listbox"
              aria-label="Game library"
              style={{
                transform: `translateX(${listTranslateX}px)`,
              }}
            >
              {!isLoading && categorizedGames.length === 0
                ? Array.from({ length: 6 }).map((_, idx) => (
                    <div key={`game-empty-${idx}`} className={`xmb-ps5-tile ${idx === 0 ? "active" : ""}`} role="option" aria-selected={idx === 0} aria-label="Empty slot">
                      <div className="xmb-ps5-tile-frame xmb-ps5-tile-frame--placeholder" />
                    </div>
                  ))
                : categorizedGames.map((game, idx) => {
                    const isActive = idx === selectedIndex;
                    const shouldRenderContent = isWithinContentWindow(idx, selectedIndex);
                    const shouldRenderImage = isWithinImageWindow(idx, selectedIndex);
                    const eagerLoadImage = Math.abs(idx - selectedIndex) <= 2;
                    return (
                      <div
                        key={game.id}
                        className={`xmb-ps5-tile ${isActive ? "active" : ""}`}
                        role="option"
                        aria-selected={isActive}
                        aria-label={game.title}
                      >
                        {shouldRenderContent ? (
                          <>
                            {favoriteGameIdSet.has(game.id) ? <Star className="xmb-ps5-tile-fav" aria-hidden /> : null}
                            <div className="xmb-ps5-tile-frame">
                              {game.imageUrl && shouldRenderImage ? (
                                <img
                                  src={game.imageUrl}
                                  alt=""
                                  className="xmb-ps5-tile-cover"
                                  {...SHELF_IMAGE_PROPS}
                                  loading={eagerLoadImage ? "eager" : "lazy"}
                                />
                              ) : <div className="xmb-ps5-tile-cover xmb-ps5-tile-cover--placeholder" />}
                            </div>
                          </>
                        ) : <div className="xmb-ps5-tile-frame xmb-ps5-tile-frame--virtualized" aria-hidden />}
                      </div>
                    );
                  })}
            </div>
          </div>
        </div>
      )}

      {topLevelShelfActive && (
        <div className="xmb-ps5-stack">
          <div className="xmb-ps5-focus-meta" aria-live="polite" key={focusMotionKey}>
            <h2 className="xmb-ps5-focus-title">{selectedTopLevelItemLabel}</h2>
            {topCategory === "all" && gameSubcategory === "root" && gamesRootPlane === "spotlight" ? (
              <p className="xmb-ps5-focus-subtitle">
                {(() => {
                  const se = spotlightEntries[spotlightIndex];
                  if (se?.kind === "cloudResume") {
                    return se.busy
                      ? "Resuming your cloud session…"
                      : "Active cloud session · Enter continues from where you left off";
                  }
                  if (spotlightEntryHasGame(se)) {
                    return "Recently played · Enter opens this title in your library";
                  }
                  return "Recently played · Empty slot — play games to fill your shelf";
                })()}
              </p>
            ) : null}
            {topCategory === "current" && displayItems[topLevelShelfIndex]?.id === "resume" && currentStreamingGame ? (
              <div className="xmb-ps5-focus-chips">
                {(() => {
                  const record = playtimeData[currentStreamingGame.id];
                  const totalSecs = record?.totalSeconds ?? 0;
                  const lastPlayedAt = record?.lastPlayedAt ?? null;
                  const sessionCount = record?.sessionCount ?? 0;
                  return (
                    <>
                      <span className="xmb-game-meta-chip xmb-game-meta-chip--playtime">
                        <Clock size={10} className="xmb-meta-icon" />
                        {formatPlaytime(totalSecs)}
                      </span>
                      <span className="xmb-game-meta-chip xmb-game-meta-chip--last-played">
                        <Calendar size={10} className="xmb-meta-icon" />
                        {formatLastPlayed(lastPlayedAt)}
                      </span>
                      {sessionCount > 0 ? (
                        <span className="xmb-game-meta-chip xmb-game-meta-chip--sessions">
                          <Repeat2 size={10} className="xmb-meta-icon" />
                          {sessionCount === 1 ? "1 session" : `${sessionCount} sessions`}
                        </span>
                      ) : null}
                    </>
                  );
                })()}
              </div>
            ) : null}
          </div>
          {gamesDualShelf ? (
            <div className="xmb-ps5-shelf-anchored">
              <div className="xmb-ps5-shelf-band xmb-ps5-shelf-band--spotlight">
                <div
                  className={`xmb-ps5-shelf-label-row xmb-ps5-shelf-label-row--spotlight ${gamesRootPlane === "spotlight" ? "xmb-ps5-shelf-label-row--active" : ""}`}
                >
                  <span className="xmb-ps5-shelf-label">
                    {cloudSessionResumable && onResumeCloudSession ? "Resume & recently played" : "Recently played"}
                  </span>
                </div>
                <div className="xmb-ps5-shelf-viewport xmb-ps5-shelf-viewport--spotlight">
                  <div
                    ref={spotlightTrackRef}
                    className="xmb-ps5-shelf-track xmb-ps5-shelf-track--spotlight"
                    role="listbox"
                    aria-label="Recently played games"
                    style={{ transform: `translateX(${spotlightShelfTranslateX}px)` }}
                  >
                    {spotlightEntries.map((entry, idx) => {
                      const isActive = gamesRootPlane === "spotlight" && idx === spotlightIndex;
                      if (entry.kind === "cloudResume") {
                        return (
                          <div
                            key="spotlight-cloud-resume"
                            className={`xmb-ps5-tile xmb-ps5-tile--spotlight xmb-ps5-tile--spotlight-resume ${isActive ? "active" : ""} ${entry.busy ? "xmb-ps5-tile--spotlight-resume-busy" : ""}`.trim()}
                            role="option"
                            aria-selected={isActive}
                            aria-label={`Resume ${entry.title}`}
                          >
                            <div className="xmb-ps5-tile-frame">
                              {entry.coverUrl ? (
                                <img src={entry.coverUrl} alt="" className="xmb-ps5-tile-cover" {...SHELF_IMAGE_PROPS} />
                              ) : (
                                <div className="xmb-ps5-tile-cover xmb-ps5-tile-cover--placeholder" />
                              )}
                              <div className="xmb-ps5-spotlight-resume-badge" aria-hidden>
                                <span className="xmb-ps5-spotlight-resume-label">{entry.busy ? "Connecting…" : "Resume"}</span>
                              </div>
                            </div>
                          </div>
                        );
                      }
                      const game = entry.game;
                      const key = game ? game.id : `recent-empty-${idx}`;
                      return (
                        <div
                          key={key}
                          className={`xmb-ps5-tile xmb-ps5-tile--spotlight ${game ? "" : "xmb-ps5-tile--spotlight-empty"} ${isActive ? "active" : ""}`.trim()}
                          role="option"
                          aria-selected={isActive}
                          aria-label={game ? game.title : "Empty recent slot"}
                        >
                          <div className="xmb-ps5-tile-frame">
                            {game?.imageUrl ? <img src={game.imageUrl} alt="" className="xmb-ps5-tile-cover" {...SHELF_IMAGE_PROPS} /> : <div className="xmb-ps5-tile-cover xmb-ps5-tile-cover--placeholder" />}
                          </div>
                        </div>
                      );
                    })}
                  </div>
                </div>
              </div>
              <div className="xmb-ps5-shelf-band xmb-ps5-shelf-band--library">
                <div
                  className={`xmb-ps5-shelf-label-row xmb-ps5-shelf-label-row--library ${gamesRootPlane === "categories" ? "xmb-ps5-shelf-label-row--active" : ""}`}
                >
                  <span className="xmb-ps5-shelf-label">Library</span>
                </div>
                <div className="xmb-ps5-shelf-viewport xmb-ps5-shelf-viewport--games-root">{topLevelMenuTrack}</div>
              </div>
            </div>
          ) : (
            <>
              <div
                className={`xmb-ps5-shelf-label-row xmb-ps5-shelf-label-row--library ${!(topCategory === "all" && gameSubcategory === "root") || gamesRootPlane === "categories" ? "xmb-ps5-shelf-label-row--active" : ""}`}
              >
                <span className="xmb-ps5-shelf-label">
                  {topCategory === "current" ? "Current" : topCategory === "settings" ? "Settings" : topCategory === "all" ? "Library" : "Media"}
                </span>
              </div>
              <div className={`xmb-ps5-shelf-viewport ${topCategory === "all" && gameSubcategory === "root" ? "xmb-ps5-shelf-viewport--games-root" : ""}`}>{topLevelMenuTrack}</div>
            </>
          )}
        </div>
      )}

      {topCategory === "media" && mediaSubcategory !== "root" && (
        <div className="xmb-ps5-stack">
          <div className="xmb-ps5-focus-meta" aria-live="polite" key={focusMotionKey}>
            <h2 className="xmb-ps5-focus-title">
              {selectedMediaItem?.gameTitle || selectedMediaItem?.fileName || mediaSubcategory}
            </h2>
            <p className="xmb-ps5-media-hub-subtitle">
              {mediaAssetItems.length} ready · {mediaHubPlaceholderCount} reserved slots
            </p>
            <div className="xmb-ps5-actions">
              <span className="xmb-ps5-action xmb-ps5-action--primary">Open Folder</span>
              <span className="xmb-ps5-action">Options</span>
            </div>
          </div>
          <div className="xmb-ps5-shelf-label-row xmb-ps5-shelf-label-row--active xmb-ps5-shelf-label-row--media-list">
            <span className="xmb-ps5-shelf-label">Captures</span>
          </div>
          <div className="xmb-ps5-shelf-viewport">
            <div
              ref={itemsContainerRef}
              className="xmb-ps5-shelf-track xmb-ps5-shelf-track--media"
              role="listbox"
              aria-label={`${mediaSubcategory} media`}
              style={{ transform: `translateX(${listTranslateX}px)` }}
            >
              {mediaLoading && Array.from({ length: 8 }).map((_, idx) => (
                <div key={`media-loading-${idx}`} className={`xmb-ps5-media-tile ${idx === 0 ? "active" : ""}`} role="option" aria-selected={idx === 0}>
                  <div className="xmb-ps5-media-frame xmb-ps5-media-frame--placeholder" />
                  <div className="xmb-ps5-media-caption">Loading {mediaSubcategory}...</div>
                </div>
              ))}

              {!mediaLoading && mediaError && (
                <div className="xmb-ps5-media-tile active" role="option" aria-selected>
                  <div className="xmb-ps5-media-frame xmb-ps5-media-frame--placeholder" />
                  <div className="xmb-ps5-media-caption">{mediaError}</div>
                </div>
              )}

              {!mediaLoading && !mediaError && mediaAssetItems.length === 0 && Array.from({ length: 6 }).map((_, idx) => (
                <div key={`media-empty-${idx}`} className={`xmb-ps5-media-tile ${idx === 0 ? "active" : ""}`} role="option" aria-selected={idx === 0}>
                  <div className="xmb-ps5-media-frame xmb-ps5-media-frame--placeholder" />
                  <div className="xmb-ps5-media-caption">
                    {idx === 0 ? `No ${mediaSubcategory.toLowerCase()} found` : "Capture more to fill this shelf"}
                  </div>
                </div>
              ))}

              {!mediaLoading && !mediaError && mediaHubSlots.map((slot, idx) => {
                const isAsset = slot.kind === "asset";
                const isActive = isAsset && idx === selectedMediaIndex;
                if (!isAsset) {
                  return (
                    <div key={slot.id} className="xmb-ps5-media-tile xmb-ps5-media-tile--placeholder-slot" role="option" aria-selected={false}>
                      <div className="xmb-ps5-media-frame xmb-ps5-media-frame--placeholder">
                        <div className="xmb-ps5-media-slot-overlay">
                          <span className="xmb-ps5-media-slot-badge">Empty Slot</span>
                        </div>
                      </div>
                      <div className="xmb-ps5-media-caption">{slot.title}</div>
                      <div className="xmb-ps5-media-meta">
                        <span className="xmb-game-meta-chip">{slot.subtitle}</span>
                      </div>
                    </div>
                  );
                }

                const item = slot.item;
                const shouldRenderContent = isWithinContentWindow(idx, selectedMediaIndex);
                const shouldRenderImage = isWithinImageWindow(idx, selectedMediaIndex);
                const eagerLoadImage = Math.abs(idx - selectedMediaIndex) <= 1;
                const thumb = mediaThumbById[item.id];
                const dateLabel = new Date(item.createdAtMs).toLocaleDateString();
                const durationMs = item.durationMs ?? 0;
                const hasDuration = durationMs > 0;
                const durationLabel = hasDuration ? `${Math.max(1, Math.round(durationMs / 1000))}s` : "Screenshot";

                return (
                  <div key={item.id} className={`xmb-ps5-media-tile ${isActive ? "active" : ""}`} role="option" aria-selected={isActive}>
                    {shouldRenderContent ? (
                      <>
                        <div className="xmb-ps5-media-frame">
                          {thumb && shouldRenderImage ? (
                            <img
                              src={thumb}
                              alt=""
                              className="xmb-ps5-media-image"
                              {...SHELF_IMAGE_PROPS}
                              loading={eagerLoadImage ? "eager" : "lazy"}
                            />
                          ) : <div className="xmb-ps5-media-image xmb-ps5-media-image--placeholder" />}
                        </div>
                        <div className="xmb-ps5-media-caption">{item.gameTitle || item.fileName}</div>
                        <div className="xmb-ps5-media-meta">
                          <span className="xmb-game-meta-chip">{durationLabel}</span>
                          <span className="xmb-game-meta-chip">{dateLabel}</span>
                        </div>
                      </>
                    ) : (
                      <div className="xmb-ps5-media-frame xmb-ps5-media-frame--virtualized" aria-hidden />
                    )}
                  </div>
                );
              })}
            </div>
          </div>
        </div>
      )}

      <div className={`xmb-detail-layer ${detailVisible ? 'visible' : ''}`}>
          {topCategory === "current" && (
            <div className="xmb-current-detail">
              <div className="xmb-current-poster">
                <img ref={attachPosterRef} src={pendingSwitchGameCover ?? currentStreamingGame?.imageUrl} alt={currentStreamingGame?.title ?? "Current"} />
              </div>
              <div className="xmb-current-info">
                <div className="xmb-game-title">{currentStreamingGame?.title ?? "Current Game"}</div>
                <div
                  className="xmb-game-meta"
                  style={{
                    maxWidth: metaMaxWidth ? `${metaMaxWidth}px` : undefined,
                    justifyContent: 'flex-end',
                  }}
                >
                  {(() => {
                    const cs = currentStreamingGame;
                    if (!cs) return null;
                    const vId = selectedVariantByGameId[cs.id] || cs.variants[0]?.id;
                    const variant = cs.variants.find(v => v.id === vId) || cs.variants[0];
                    const storeName = getStoreDisplayName(variant?.store || "");
                    const record = (playtimeData ?? {})[cs.id];
                    const totalSecs = record?.totalSeconds ?? 0;
                    const lastPlayed = record?.lastPlayedAt ?? null;
                    const sessionCount = record?.sessionCount ?? 0;
                    const playtimeLabel = formatPlaytime(totalSecs);
                    const lastPlayedLabel = formatLastPlayed(lastPlayed);
                    const genres = cs.genres?.slice(0, 2) ?? [];
                    const tier = cs.membershipTierLabel;
                    return (
                      <>
                        {storeName && <span className="xmb-game-meta-chip xmb-game-meta-chip--store">{storeName}</span>}
                        {sessionCounterEnabled && (
                          <span className="xmb-game-meta-chip xmb-game-meta-chip--session">
                            <SessionElapsedIndicator startedAtMs={sessionStartedAtMs ?? null} active={isStreaming} />
                          </span>
                        )}
                        <span className="xmb-game-meta-chip xmb-game-meta-chip--playtime">
                          <Clock size={10} className="xmb-meta-icon" />
                          {playtimeLabel}
                        </span>
                        <span className="xmb-game-meta-chip xmb-game-meta-chip--last-played">
                          <Calendar size={10} className="xmb-meta-icon" />
                          {lastPlayedLabel}
                        </span>
                        {sessionCount > 0 && (
                          <span className="xmb-game-meta-chip xmb-game-meta-chip--sessions">
                            <Repeat2 size={10} className="xmb-meta-icon" />
                            {sessionCount === 1 ? "1 session" : `${sessionCount} sessions`}
                          </span>
                        )}
                        {genres.map(g => (
                          <span key={g} className="xmb-game-meta-chip xmb-game-meta-chip--genre">{sanitizeGenreName(g)}</span>
                        ))}
                        {tier && <span className="xmb-game-meta-chip xmb-game-meta-chip--tier">{tier}</span>}
                      </>
                    );
                  })()}
                </div>
              </div>
            </div>
          )}
          </div>

      {ps5Row === "detail" && canEnterDetailRow && detailRailItems.length > 0 && (
        <div className="xmb-ps5-detail-rail" role="listbox" aria-label="Detail row">
          {detailRailItems.map((item, idx) => (
            <div key={item.id} className={`xmb-ps5-detail-card ${idx === detailRailIndex ? "active" : ""}`} role="option" aria-selected={idx === detailRailIndex}>
              <div className="xmb-ps5-detail-card-image-wrap">
                {item.imageUrl ? (
                  <img src={item.imageUrl} alt="" className="xmb-ps5-detail-card-image" {...SHELF_IMAGE_PROPS} />
                ) : (
                  <div className="xmb-ps5-detail-card-image xmb-ps5-detail-card-image--placeholder" />
                )}
              </div>
              <div className="xmb-ps5-detail-card-title">{item.title}</div>
              <div className="xmb-ps5-detail-card-subtitle">{item.subtitle}</div>
            </div>
          ))}
        </div>
      )}

      {optionsOpen && optionsEntries.length > 0 ? (
        <div className="xmb-ps5-options-sheet" role="dialog" aria-modal="true" aria-label="Options">
          <div className="xmb-ps5-options-backdrop" aria-hidden />
          <div className="xmb-ps5-options-panel">
            <div className="xmb-ps5-options-title">Options</div>
            <ul className="xmb-ps5-options-list">
              {optionsEntries.map((opt, i) => (
                <li key={`${opt.id}-${i}`} className={`xmb-ps5-options-item ${i === optionsFocusIndex ? "active" : ""}`}>
                  {opt.label}
                </li>
              ))}
            </ul>
          </div>
        </div>
      ) : null}

      <div className="xmb-footer">
        {topLevelRowBehaviorActive ? (
          <>
            <div className="xmb-btn-hint">
              {controllerType === "ps" ? (
                <ButtonPSCross className="xmb-btn-icon" size={24} />
              ) : (
                <ButtonA className="xmb-btn-icon" size={24} />
              )}
              <span>Select</span>
            </div>
            <div className="xmb-btn-hint"><span className="xmb-btn-keycap">L1</span> <span>Prev Section</span></div>
            <div className="xmb-btn-hint"><span className="xmb-btn-keycap">R1</span> <span>Next Section</span></div>
          </>
        ) : topCategory === "current" ? (
          <>
            <div className="xmb-btn-hint" style={{margin: '0 auto'}}>
              {controllerType === "ps" ? (
                <ButtonPSCross className="xmb-btn-icon" size={24} />
              ) : (
                <ButtonA className="xmb-btn-icon" size={24} />
              )}
              <span>Select</span>
            </div>
          </>
        ) : topCategory === "settings" ? (
          <>
            {settingsSubcategory === "root" ? (
              <>
                <div className="xmb-btn-hint">
                  {controllerType === "ps" ? (
                    <ButtonPSCross className="xmb-btn-icon" size={24} />
                  ) : (
                    <ButtonA className="xmb-btn-icon" size={24} />
                  )}
                  <span>Enter</span>
                </div>
              </>
            ) : settingsSubcategory === "Theme" ? (
              <div className="xmb-btn-hint">
                {controllerType === "ps" ? (
                  <ButtonPSCross className="xmb-btn-icon" size={24} />
                ) : (
                  <ButtonA className="xmb-btn-icon" size={24} />
                )}
                <span>Enter</span>
              </div>
            ) : settingsSubcategory === "ThemeStyle" ? (
              <>
                <div className="xmb-btn-hint">
                  {controllerType === "ps" ? (
                    <ButtonPSCircle className="xmb-btn-icon" size={24} />
                  ) : (
                    <ButtonB className="xmb-btn-icon" size={24} />
                  )}
                  <span>Back</span>
                </div>
                <div className="xmb-btn-hint">
                  {controllerType === "ps" ? (
                    <ButtonPSCross className="xmb-btn-icon" size={24} />
                  ) : (
                    <ButtonA className="xmb-btn-icon" size={24} />
                  )}
                  <span>Select</span>
                </div>
              </>
            ) : settingsSubcategory === "ThemeColor" ? (
              <>
                <div className="xmb-btn-hint">
                  {controllerType === "ps" ? (
                    <ButtonPSCircle className="xmb-btn-icon" size={24} />
                  ) : (
                    <ButtonB className="xmb-btn-icon" size={24} />
                  )}
                  <span>Back</span>
                </div>
                <div className="xmb-btn-hint">
                  {controllerType === "ps" ? (
                    <ButtonPSCross className="xmb-btn-icon" size={24} />
                  ) : (
                    <ButtonA className="xmb-btn-icon" size={24} />
                  )}
                  <span>{editingThemeChannel ? "Confirm" : "Adjust"}</span>
                </div>
              </>
            ) : (
              <>
                <div className="xmb-btn-hint">
                  {controllerType === "ps" ? (
                    <ButtonPSCircle className="xmb-btn-icon" size={24} />
                  ) : (
                    <ButtonB className="xmb-btn-icon" size={24} />
                  )}
                  <span>Back</span>
                </div>
                <div className="xmb-btn-hint">
                  {controllerType === "ps" ? (
                    <ButtonPSCross className="xmb-btn-icon" size={24} />
                  ) : (
                    <ButtonA className="xmb-btn-icon" size={24} />
                  )}
                  <span>Toggle</span>
                </div>
              </>
            )}
          </>
        ) : topCategory === "media" ? (
          <>
            {mediaSubcategory === "root" ? (
              <div className="xmb-btn-hint">
                {controllerType === "ps" ? (
                  <ButtonPSCross className="xmb-btn-icon" size={24} />
                ) : (
                  <ButtonA className="xmb-btn-icon" size={24} />
                )}
                <span>Enter</span>
              </div>
            ) : (
              <>
                <div className="xmb-btn-hint">
                  <span>Browse · Left / Right</span>
                </div>
                <div className="xmb-btn-hint">
                  {controllerType === "ps" ? (
                    <ButtonPSCross className="xmb-btn-icon" size={24} />
                  ) : (
                    <ButtonA className="xmb-btn-icon" size={24} />
                  )}
                  <span>Open Folder</span>
                </div>
                <div className="xmb-btn-hint">{renderFaceButton("tertiary", "xmb-btn-icon", 24)} <span>Options</span></div>
                <div className="xmb-btn-hint">
                  {controllerType === "ps" ? (
                    <ButtonPSCircle className="xmb-btn-icon" size={24} />
                  ) : (
                    <ButtonB className="xmb-btn-icon" size={24} />
                  )}
                  <span>Back To Media</span>
                </div>
              </>
            )}
          </>
        ) : topCategory === "all" && gameSubcategory !== "root" ? (
          gamesHubOpen ? (
            <>
              <div className="xmb-btn-hint">
                <span>Actions · Left / Right</span>
              </div>
              <div className="xmb-btn-hint">
                {controllerType === "ps" ? (
                  <ButtonPSCross className="xmb-btn-icon" size={24} />
                ) : (
                  <ButtonA className="xmb-btn-icon" size={24} />
                )}
                <span>Confirm</span>
              </div>
              <div className="xmb-btn-hint">
                {controllerType === "ps" ? (
                  <ButtonPSCircle className="xmb-btn-icon" size={24} />
                ) : (
                  <ButtonB className="xmb-btn-icon" size={24} />
                )}
                <span>Back</span>
              </div>
              <div className="xmb-btn-hint">{renderFaceButton("tertiary", "xmb-btn-icon", 24)} <span>Options</span></div>
            </>
          ) : (
            <>
              <div className="xmb-btn-hint">
                <span>Browse · Left / Right</span>
              </div>
              <div className="xmb-btn-hint">
                <span>Library filters · Up</span>
              </div>
              <div className="xmb-btn-hint">{renderFaceButton("primary", "xmb-btn-icon", 24)} <span>Game hub</span></div>
              <div className="xmb-btn-hint">
                <span>Hub · Down</span>
              </div>
              {gameSubcategory === "all" ? (
                <div className="xmb-btn-hint">{renderFaceButton("secondary", "xmb-btn-icon", 24)} <span>Sort</span></div>
              ) : null}
              <div className="xmb-btn-hint">{renderFaceButton("tertiary", "xmb-btn-icon", 24)} <span>Options</span></div>
            </>
          )
        ) : topCategory === "all" && gameSubcategory === "root" ? (
          <>
            <div className="xmb-btn-hint">
              {controllerType === "ps" ? (
                <ButtonPSCross className="xmb-btn-icon" size={24} />
              ) : (
                <ButtonA className="xmb-btn-icon" size={24} />
              )}
              <span>
                {gamesRootPlane === "spotlight" && spotlightEntries[spotlightIndex]?.kind === "cloudResume"
                  ? spotlightEntries[spotlightIndex].busy
                    ? "Please wait"
                    : "Resume session"
                  : gamesRootPlane === "spotlight" && spotlightEntryHasGame(spotlightEntries[spotlightIndex])
                    ? "View in library"
                    : "Enter"}
              </span>
            </div>
            {gamesRootPlane === "spotlight" && spotlightEntryHasGame(spotlightEntries[spotlightIndex]) ? (
              <div className="xmb-btn-hint">{renderFaceButton("tertiary", "xmb-btn-icon", 24)} <span>Options</span></div>
            ) : null}
            <div className="xmb-btn-hint">
              <span className="xmb-btn-keycap">L1</span> <span>Prev Section</span>
            </div>
            <div className="xmb-btn-hint">
              <span className="xmb-btn-keycap">R1</span> <span>Next Section</span>
            </div>
          </>
        ) : (
          <>
            <div className="xmb-btn-hint">{renderFaceButton("primary", "xmb-btn-icon", 24)} <span>{currentStreamingGame && selectedGame && currentStreamingGame.id !== selectedGame.id ? "Switch" : "Play"}</span></div>
            <div className="xmb-btn-hint">{renderFaceButton("tertiary", "xmb-btn-icon", 24)} <span>Options</span></div>
          </>
        )}
      </div>
    </div>
  );
}
