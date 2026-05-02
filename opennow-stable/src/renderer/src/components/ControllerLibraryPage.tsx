import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { JSX } from "react";
import type { GameInfo, MediaListingEntry, Settings, ControllerThemeStyle } from "@shared/gfn";
import { Star, Clock, Calendar, Repeat2, House, Settings as SettingsIcon, Library, Clapperboard } from "lucide-react";
import { ButtonA, ButtonB, ButtonX, ButtonY, ButtonPSCross, ButtonPSCircle, ButtonPSSquare, ButtonPSTriangle } from "./ControllerButtons";
import { getStoreDisplayName } from "./GameCard";
import { SessionElapsedIndicator, RemainingPlaytimeIndicator, CurrentClock } from "./ElapsedSessionIndicators";
import { type PlaytimeStore, formatPlaytime, formatLastPlayed } from "../utils/usePlaytime";

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
}

type Direction = "up" | "down" | "left" | "right";
type TopCategory = "current" | "all" | "settings" | "media";
type SoundKind = "move" | "confirm";
type SettingsSubcategory = "root" | "Network" | "Audio" | "Video" | "System" | "Theme" | "ThemeColor" | "ThemeStyle";
type MediaSubcategory = "root" | "Videos" | "Screenshots";
type GameSubcategory = "root" | "all" | "favorites" | `genre:${string}`;
const CATEGORY_STEP_PX = 160;
const CATEGORY_ACTIVE_HALF_WIDTH_PX = 60;
const GAME_ACTIVE_CENTER_OFFSET_X_PX = 320;
const PREVIEW_TILE_COUNT = 12;

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
  const audioContextRef = useRef<AudioContext | null>(null);
  const itemsContainerRef = useRef<HTMLDivElement>(null);
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
    if (!uiSoundsEnabled) return;
    const audioContext = audioContextRef.current ?? new AudioContext();
    audioContextRef.current = audioContext;
    if (audioContext.state === "suspended") void audioContext.resume();

    const now = audioContext.currentTime;
    const oscillator = audioContext.createOscillator();
    const gain = audioContext.createGain();

    const profile: Record<SoundKind, { start: number; end: number; duration: number; volume: number; type: OscillatorType }> = {
      move: { start: 720, end: 680, duration: 0.04, volume: 0.02, type: "triangle" },
      confirm: { start: 640, end: 860, duration: 0.1, volume: 0.04, type: "sine" },
    };

    const active = profile[kind];
    oscillator.type = active.type;
    oscillator.frequency.setValueAtTime(active.start, now);
    oscillator.frequency.exponentialRampToValueAtTime(active.end, now + active.duration);
    gain.gain.setValueAtTime(0.0001, now);
    gain.gain.exponentialRampToValueAtTime(active.volume, now + 0.012);
    gain.gain.exponentialRampToValueAtTime(0.0001, now + active.duration);

    oscillator.connect(gain);
    gain.connect(audioContext.destination);
    oscillator.start(now);
    oscillator.stop(now + active.duration + 0.01);
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
 
  const currentGameItems = useMemo(() => [
    { id: "resume", label: "Resume Game", value: "" },
    { id: "closeGame", label: "Close Game", value: "" },
  ], []);

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
  }, [games, favoriteGames, gameSubcategory, topCategory, playtimeData]);

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
  const canEnterDetailRow = gamesShelfBrowseActive || mediaShelfBrowseActive;
  const canEnterTopRow = topLevelShelfActive || gamesShelfBrowseActive || mediaShelfBrowseActive;
  const topLevelShelfIndex =
    topCategory === "media"
      ? selectedMediaIndex
      : topCategory === "all"
        ? selectedGameSubcategoryIndex
        : selectedSettingIndex;

  const selectedCategoryLabel = useMemo(() => getCategoryLabel(topCategory, currentStreamingGame?.title).label, [topCategory, currentStreamingGame?.title]);
  const selectedTopLevelItemLabel = useMemo(() => {
    if (!topLevelShelfActive) return selectedCategoryLabel;
    const active = displayItems[topLevelShelfIndex];
    if (topCategory === "all" && gameSubcategory === "root" && active?.label) return active.label;
    return selectedCategoryLabel;
  }, [topLevelShelfActive, selectedCategoryLabel, displayItems, topLevelShelfIndex, topCategory, gameSubcategory]);
  const detailRailItems = useMemo<Array<{ id: string; title: string; subtitle: string; imageUrl?: string }>>(() => {
    if (topCategory === "all" && gameSubcategory !== "root" && selectedGame) {
      const genre = selectedGame.genres?.[0] ? sanitizeGenreName(selectedGame.genres[0]) : "Cloud Action";
      return [
        { id: "d1", title: "Activities", subtitle: `${genre} challenges`, imageUrl: selectedGame.imageUrl },
        { id: "d2", title: "Community", subtitle: "Friends playing now", imageUrl: selectedGame.imageUrl },
        { id: "d3", title: "Store", subtitle: "DLC and add-ons", imageUrl: selectedGame.imageUrl },
      ];
    }
    if (topCategory === "media" && mediaSubcategory !== "root") {
      const current = mediaAssetItems[selectedMediaIndex];
      const imageUrl = current?.thumbnailDataUrl || current?.dataUrl || (current ? mediaThumbById[current.id] : undefined);
      return [
        { id: "m1", title: "Recent", subtitle: `Latest ${mediaSubcategory.toLowerCase()}`, imageUrl },
        { id: "m2", title: "By Game", subtitle: "Grouped captures", imageUrl },
        { id: "m3", title: "Storage", subtitle: "Manage media files", imageUrl },
      ];
    }
    return [];
  }, [topCategory, gameSubcategory, selectedGame, mediaSubcategory, mediaAssetItems, selectedMediaIndex, mediaThumbById]);
  const focusMotionKey = useMemo(() => {
    if (topCategory === "all" && gameSubcategory !== "root") return `game-${selectedGame?.id ?? "none"}`;
    if (topCategory === "media" && mediaSubcategory !== "root") return `media-${selectedMediaIndex}-${mediaAssetItems[selectedMediaIndex]?.id ?? "none"}`;
    return `menu-${topCategory}-${topLevelShelfIndex}`;
  }, [topCategory, gameSubcategory, selectedGame?.id, topLevelShelfIndex, mediaSubcategory, selectedMediaIndex, mediaAssetItems]);
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
    if (!gamesShelfBrowseActive && !mediaShelfBrowseActive && !topLevelShelfActive) setListTranslateX(0);
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
    const onResize = () => setViewportWidth(window.innerWidth);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  useLayoutEffect(() => {
    const container = itemsContainerRef.current;
    if (!container) return;
    const children = Array.from(container.children) as HTMLElement[];
    const activeIndex = gamesShelfBrowseActive ? selectedIndex : mediaShelfBrowseActive ? selectedMediaIndex : topLevelShelfIndex;
    if (children.length === 0 || activeIndex >= children.length) {
      if (gamesShelfBrowseActive || mediaShelfBrowseActive || topLevelShelfActive) setListTranslateX(0);
      return;
    }

    if (gamesShelfBrowseActive || mediaShelfBrowseActive || topLevelShelfActive) {
      let gap = 14;
      if (children.length >= 2) {
        gap = Math.max(8, children[1].offsetLeft - children[0].offsetLeft - children[0].offsetWidth);
      }
      let offsetCenter = 0;
      for (let i = 0; i < activeIndex; i++) {
        offsetCenter += children[i].offsetWidth + gap;
      }
      offsetCenter += children[activeIndex].offsetWidth / 2;
      setListTranslateX(viewportWidth / 2 - offsetCenter);
      setListTranslateY(0);
      return;
    }

    let offset = 0;
    for (let i = 0; i < selectedIndex; i++) {
      const childStyle = window.getComputedStyle(children[i]);
      offset += children[i].offsetHeight + parseFloat(childStyle.marginBottom);
    }
    offset += children[selectedIndex].offsetHeight / 2;
    setListTranslateY(-offset);
    setListTranslateX(0);
  }, [selectedIndex, categorizedGames, gamesShelfBrowseActive, mediaShelfBrowseActive, topLevelShelfActive, topLevelShelfIndex, selectedMediaIndex, viewportWidth]);

  const throttledOnSelectGame = useCallback((id: string) => onSelectGame(id), [onSelectGame]);

  const toggleFavoriteForSelected = useCallback(() => {
    if (selectedGame) {
      onToggleFavoriteGame(selectedGame.id);
      playUiSound("confirm");
    }
  }, [onToggleFavoriteGame, playUiSound, selectedGame]);

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
      if (isLoading && topCategory !== "settings" && topCategory !== "current") return;

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
            if (canEnterDetailRow && detailRailItems.length > 0) {
              playUiSound("move");
              setPs5Row("detail");
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

      if (topLevelShelfActive) {
        const itemCount = displayItems.length;
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
          if (direction === "up" && canEnterTopRow) {
            playUiSound("move");
            setPs5Row("top");
          } else if (direction === "down" && canEnterDetailRow && detailRailItems.length > 0) {
            playUiSound("move");
            setPs5Row("detail");
          }
          return;
        }
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
          }
          return;
        }
        if (direction === "down") {
          const nextIndex = Math.min(displayItems.length - 1, selectedSettingIndex + 1);
          if (nextIndex !== selectedSettingIndex) {
            playUiSound("move");
            setSelectedSettingIndex(nextIndex);
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
      playUiSound("move");
    };

    const handler = (e: any) => {
      if (e.detail?.direction) applyDirection(e.detail.direction);
    };
    const shoulderHandler = (e: any) => {
      const direction = e?.detail?.direction as "prev" | "next" | undefined;
      if (!direction) return;
      if (topCategory === "settings" && settingsSubcategory !== "root") return;
      if (editingBandwidth || editingThemeChannel) return;
      cycleTopCategory(direction === "prev" ? -1 : 1);
    };

    const activateHandler = () => {
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

        if (topCategory === "all" && gameSubcategory !== "root" && selectedGame) {
          if (selectedDetail.id === "d1") {
            onPlayGame(selectedGame);
            playUiSound("confirm");
            return;
          }
          if (selectedDetail.id === "d2") {
            toggleFavoriteForSelected();
            return;
          }
          if (selectedDetail.id === "d3" && selectedGame.variants.length > 1) {
            const idx = selectedGame.variants.findIndex((v) => v.id === selectedVariantId);
            const next = selectedGame.variants[(idx + 1) % selectedGame.variants.length];
            onSelectGameVariant(selectedGame.id, next.id);
            playUiSound("confirm");
            return;
          }
          playUiSound("confirm");
          return;
        }

        if (topCategory === "media" && mediaSubcategory !== "root") {
          if (selectedDetail.id === "m3" || selectedDetail.id === "m1") {
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
      if (topCategory === "current") {
        const item = displayItems[selectedSettingIndex];
        if (item?.id === "resume" && currentStreamingGame && onResumeGame) {
          onResumeGame(currentStreamingGame);
          playUiSound("confirm");
          return;
        }
        if (item?.id === "closeGame" && onCloseGame) {
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
          onPlayGame(selectedGame);
          playUiSound("confirm");
        }
      } else if (selectedGame) {
        onPlayGame(selectedGame);
        playUiSound("confirm");
      }
    };

    const secondaryActivateHandler = () => {
        if (topLevelShelfActive) {
          cycleTopCategory(-1);
          return;
        }
        if (topCategory === "current") {
          // X button does nothing on current game menu items
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
      if (selectedGame && selectedGame.variants.length > 1) {
        const idx = selectedGame.variants.findIndex(v => v.id === selectedVariantId);
        const next = selectedGame.variants[(idx + 1) % selectedGame.variants.length];
        onSelectGameVariant(selectedGame.id, next.id);
        playUiSound("move");
      }
    };

    const tertiaryActivateHandler = () => {
      if (topLevelShelfActive) {
        cycleTopCategory(1);
        return;
      }
      if (topCategory !== "settings" && topCategory !== "current" && !(topCategory === "all" && gameSubcategory === "root")) {
        toggleFavoriteForSelected();
      }
    };

    const cancelHandler = (e: Event) => {
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
      if (e.key.toLowerCase() === "q" && topLevelShelfActive) {
        e.preventDefault();
        cycleTopCategory(-1);
        return;
      }
      if (e.key.toLowerCase() === "e" && topLevelShelfActive) {
        e.preventDefault();
        cycleTopCategory(1);
        return;
      }
      if (e.key === "Backspace" || e.key === "Escape") {
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

        // Top-level back is intentionally a no-op.
        e.preventDefault();
        return;
      }
    };

    window.addEventListener("opennow:controller-direction", handler);
    window.addEventListener("opennow:controller-shoulder", shoulderHandler);
    window.addEventListener("opennow:controller-activate", activateHandler);
    window.addEventListener("opennow:controller-secondary-activate", secondaryActivateHandler);
    window.addEventListener("opennow:controller-tertiary-activate", tertiaryActivateHandler);
    window.addEventListener("opennow:controller-cancel", cancelHandler);
    window.addEventListener("keydown", kbdHandler);
    return () => {
      window.removeEventListener("opennow:controller-direction", handler);
      window.removeEventListener("opennow:controller-shoulder", shoulderHandler);
      window.removeEventListener("opennow:controller-activate", activateHandler);
      window.removeEventListener("opennow:controller-secondary-activate", secondaryActivateHandler);
      window.removeEventListener("opennow:controller-tertiary-activate", tertiaryActivateHandler);
      window.removeEventListener("opennow:controller-cancel", cancelHandler);
      window.removeEventListener("keydown", kbdHandler);
    };
  }, [isLoading, TOP_CATEGORIES.length, categorizedGames, selectedIndex, selectedGame, selectedVariantId, onPlayGame, onSelectGameVariant, onOpenSettings, playUiSound, throttledOnSelectGame, toggleFavoriteForSelected, topCategory, selectedSettingIndex, selectedMediaIndex, selectedGameSubcategoryIndex, displayItems, mediaAssetItems.length, mediaSubcategory, gameSubcategory, settings, settingsBySubcategory, settingsSubcategory, lastRootSettingIndex, lastRootMediaIndex, lastRootGameIndex, lastSystemMenuIndex, lastThemeRootIndex, onSettingChange, resolutionOptions, fpsOptions, codecOptions, aspectRatioOptions, currentStreamingGame, onResumeGame, onCloseGame, onExitControllerMode, onExitApp, editingBandwidth, editingThemeChannel, gamesShelfBrowseActive, mediaShelfBrowseActive, topLevelShelfActive, topLevelShelfIndex, canEnterDetailRow, canEnterTopRow, ps5Row, detailRailIndex, detailRailItems.length]);

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
  const heroBackdropUrl = useMemo(() => {
    if (topCategory === "all") return selectedGame?.imageUrl ?? null;
    if (topCategory === "current") return currentStreamingGame?.imageUrl ?? null;
    if (topCategory === "media") {
      if (selectedMediaItem?.thumbnailDataUrl) return selectedMediaItem.thumbnailDataUrl;
      if (selectedMediaItem?.dataUrl) return selectedMediaItem.dataUrl;
      return selectedMediaItem ? mediaThumbById[selectedMediaItem.id] ?? null : null;
    }
    if (currentStreamingGame?.imageUrl) return currentStreamingGame.imageUrl;
    return selectedGame?.imageUrl ?? null;
  }, [topCategory, selectedGame, currentStreamingGame, selectedMediaItem, mediaThumbById]);
  const themeRgbResolved = settings.controllerThemeColor ?? { r: 124, g: 241, b: 177 };
  const wrapperThemeVars = {
    "--xmb-theme-r": String(themeRgbResolved.r),
    "--xmb-theme-g": String(themeRgbResolved.g),
    "--xmb-theme-b": String(themeRgbResolved.b),
  } as React.CSSProperties;

  const wrapperClassName = `xmb-wrapper xmb-theme-${themeStyleSafe} ${settings.controllerBackgroundAnimations ? "xmb-animate" : "xmb-static"} ${isEntering ? "xmb-entering" : "xmb-ready"} xmb-layout--ps5-home`;
  const wrapperClassNameWithRow = `${wrapperClassName} xmb-row-${ps5Row}`;

  if (isLoading && topCategory !== "settings" && topCategory !== "current" && topCategory !== "media") return <div className={wrapperClassNameWithRow} style={wrapperThemeVars}><div className="xmb-bg-layer"><div className="xmb-bg-gradient" /></div><div style={{display:'flex',justifyContent:'center',alignItems:'center',height:'100vh'}}>Loading...</div></div>;

  return (
    <div className={wrapperClassNameWithRow} style={wrapperThemeVars}>
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
          <div className="xmb-ps5-hero-art" style={{ backgroundImage: `url(${heroBackdropUrl})` }} aria-hidden />
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

      {topCategory === "all" && gameSubcategory !== "root" && (
        <div className="xmb-ps5-stack">
          {selectedGame ? (
            <div className="xmb-ps5-focus-meta" aria-live="polite" key={focusMotionKey}>
              <h2 className="xmb-ps5-focus-title">{selectedGame.title}</h2>
              <div className="xmb-ps5-actions">
                <span className="xmb-ps5-action xmb-ps5-action--primary">
                  {currentStreamingGame && currentStreamingGame.id !== selectedGame.id ? "Switch" : "Play"}
                </span>
                <span className="xmb-ps5-action">
                  {favoriteGameIdSet.has(selectedGame.id) ? "Unfavorite" : "Favorite"}
                </span>
                {selectedGame.variants.length > 1 ? <span className="xmb-ps5-action">Variant</span> : null}
              </div>
              <div className="xmb-ps5-focus-chips">
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
              {categorizedGames.map((game, idx) => {
                const isActive = idx === selectedIndex;
                return (
                  <div
                    key={game.id}
                    className={`xmb-ps5-tile ${isActive ? "active" : ""}`}
                    role="option"
                    aria-selected={isActive}
                    aria-label={game.title}
                  >
                    {favoriteGameIdSet.has(game.id) ? <Star className="xmb-ps5-tile-fav" aria-hidden /> : null}
                    <div className="xmb-ps5-tile-frame">
                      <img src={game.imageUrl} alt="" className="xmb-ps5-tile-cover" />
                    </div>
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
            <div className="xmb-ps5-actions">
              <span className="xmb-ps5-action xmb-ps5-action--primary">Enter</span>
              <span className="xmb-ps5-action">Change Section</span>
            </div>
          </div>
          <div className={`xmb-ps5-shelf-viewport ${topCategory === "all" && gameSubcategory === "root" ? "xmb-ps5-shelf-viewport--games-root" : ""}`}>
            <div
              ref={itemsContainerRef}
              className={`xmb-ps5-shelf-track xmb-ps5-shelf-track--menu ${topCategory === "all" && gameSubcategory === "root" ? "xmb-ps5-shelf-track--games-root" : ""}`}
              role="listbox"
              aria-label={topCategory === "current" ? "Current game actions" : topCategory === "settings" ? "Controller settings" : topCategory === "all" ? "Game categories" : "Media categories"}
              style={{ transform: `translateX(${listTranslateX}px)` }}
            >
              {displayItems.map((item, idx) => {
                const isActive = idx === topLevelShelfIndex;
                const themeChannelForRow =
                  item.id === "themeR" ? "r" : item.id === "themeG" ? "g" : item.id === "themeB" ? "b" : null;
                const themeRgbLive = settings.controllerThemeColor ?? { r: 124, g: 241, b: 177 };
                const isGameRootTile = topCategory === "all" && gameSubcategory === "root";
                const previewThumbs = isGameRootTile ? (gameCategoryPreviewById[item.id] ?? []) : [];
                return (
                  <div key={item.id} className={`xmb-ps5-menu-tile ${isActive ? "active" : ""}`} role="option" aria-selected={isActive}>
                    {isGameRootTile ? (
                      <div className="xmb-ps5-menu-thumb-row" aria-hidden>
                        {previewThumbs.map((src, i) => (
                          <div key={`${item.id}-${i}`} className="xmb-ps5-menu-thumb">
                            <img src={src} alt="" className="xmb-ps5-menu-thumb-img" />
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
                              value={settings.maxBitrateMbps ?? 75}
                              onChange={(e) => onSettingChange && onSettingChange("maxBitrateMbps" as any, Number(e.target.value) as any)}
                              aria-label="Bandwidth Limit (Mbps)"
                              style={editingBandwidth ? { outline: "2px solid rgba(255,255,255,0.2)" } : undefined}
                            />
                            <span className="xmb-game-meta-chip">{`${settings.maxBitrateMbps ?? 75} Mbps`}{editingBandwidth ? " • Editing" : ""}</span>
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
                        ) : (
                          <span className="xmb-game-meta-chip">{item.value}</span>
                        )}
                      </div>
                    ) : null}
                  </div>
                );
              })}
            </div>
          </div>
        </div>
      )}

      {topCategory === "media" && mediaSubcategory !== "root" && (
        <div className="xmb-ps5-stack">
          <div className="xmb-ps5-focus-meta" aria-live="polite" key={focusMotionKey}>
            <h2 className="xmb-ps5-focus-title">
              {selectedMediaItem?.gameTitle || selectedMediaItem?.fileName || mediaSubcategory}
            </h2>
            <div className="xmb-ps5-actions">
              <span className="xmb-ps5-action xmb-ps5-action--primary">Open Folder</span>
              <span className="xmb-ps5-action">Back To Media</span>
            </div>
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

              {!mediaLoading && !mediaError && mediaAssetItems.map((item, idx) => {
                const isActive = idx === selectedMediaIndex;
                const thumb = mediaThumbById[item.id];
                const dateLabel = new Date(item.createdAtMs).toLocaleDateString();
                const durationMs = item.durationMs ?? 0;
                const hasDuration = durationMs > 0;
                const durationLabel = hasDuration ? `${Math.max(1, Math.round(durationMs / 1000))}s` : "Screenshot";

                return (
                  <div key={item.id} className={`xmb-ps5-media-tile ${isActive ? "active" : ""}`} role="option" aria-selected={isActive}>
                    <div className="xmb-ps5-media-frame">
                      {thumb ? <img src={thumb} alt="" className="xmb-ps5-media-image" /> : <div className="xmb-ps5-media-image xmb-ps5-media-image--placeholder" />}
                    </div>
                    <div className="xmb-ps5-media-caption">{item.gameTitle || item.fileName}</div>
                    <div className="xmb-ps5-media-meta">
                      <span className="xmb-game-meta-chip">{durationLabel}</span>
                      <span className="xmb-game-meta-chip">{dateLabel}</span>
                    </div>
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
                  <img src={item.imageUrl} alt="" className="xmb-ps5-detail-card-image" />
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

      <div className="xmb-footer">
        {topLevelShelfActive ? (
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
          <>
            <div className="xmb-btn-hint">
              <span>Browse · Left / Right</span>
            </div>
            <div className="xmb-btn-hint">
              <span>Library filters · Up</span>
            </div>
            <div className="xmb-btn-hint">{renderFaceButton("primary", "xmb-btn-icon", 24)} <span>{currentStreamingGame && selectedGame && currentStreamingGame.id !== selectedGame.id ? "Switch" : "Play"}</span></div>
            {selectedGame && selectedGame.variants.length > 1 ? (
              <div className="xmb-btn-hint">{renderFaceButton("secondary", "xmb-btn-icon", 24)} <span>Variant</span></div>
            ) : null}
            {selectedGame ? (
              <div className="xmb-btn-hint">{renderFaceButton("tertiary", "xmb-btn-icon", 24)} <span>{favoriteGameIdSet.has(selectedGame.id) ? "Unfavorite" : "Favorite"}</span></div>
            ) : null}
          </>
        ) : topCategory === "all" && gameSubcategory === "root" ? (
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
            <div className="xmb-btn-hint">{renderFaceButton("primary", "xmb-btn-icon", 24)} <span>{currentStreamingGame && selectedGame && currentStreamingGame.id !== selectedGame.id ? "Switch" : "Play"}</span></div>
            {selectedGame && selectedGame.variants.length > 1 ? (
              <div className="xmb-btn-hint">{renderFaceButton("secondary", "xmb-btn-icon", 24)} <span>Variant</span></div>
            ) : null}
            {selectedGame ? (
              <div className="xmb-btn-hint">{renderFaceButton("tertiary", "xmb-btn-icon", 24)} <span>{favoriteGameIdSet.has(selectedGame.id) ? "Unfavorite" : "Favorite"}</span></div>
            ) : null}
          </>
        )}
      </div>
    </div>
  );
}
