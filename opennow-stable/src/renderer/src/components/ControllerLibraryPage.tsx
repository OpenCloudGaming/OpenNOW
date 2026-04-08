import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { JSX } from "react";
import type { GameInfo, MediaListingEntry, Settings } from "@shared/gfn";
import { Star, Clock, Calendar, Repeat2 } from "lucide-react";
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
    controllerUiSounds?: boolean;
    controllerBackgroundAnimations?: boolean;
    autoLoadControllerLibrary?: boolean;
    autoFullScreen?: boolean;
    aspectRatio?: string;
    maxBitrateMbps?: number;
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
type TopCategory = "current" | "all" | "settings" | "media" | "favorites" | `genre:${string}`;
type SoundKind = "move" | "confirm";
type SettingsSubcategory = "root" | "Network" | "Audio" | "Video" | "System";
type MediaSubcategory = "root" | "Videos" | "Screenshots";

const CATEGORY_STEP_PX = 160;
const CATEGORY_ACTIVE_HALF_WIDTH_PX = 60;
const GAME_ACTIVE_CENTER_OFFSET_X_PX = 320;

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
  if (categoryId === "all") return { label: "All" };
  if (categoryId === "settings") return { label: "Settings" };
  if (categoryId === "media") return { label: "Media" };
  if (categoryId === "favorites") return { label: "Favorites" };
  const genreName = sanitizeGenreName(categoryId.slice(6));
  const shorthand: Record<string, string> = {
    "massively multiplayer online battle arena": "MOBA",
    "massively multiplayer online": "MMO",
    "multiplayer online battle arena": "MOBA",
    "first person shooter": "FPS",
    "role playing game": "RPG",
    "real time strategy": "RTS",
    "simulation": "Sim",
    "virtual reality": "VR",
    "third person shooter": "TPS",
  };
  const normalized = genreName.toLowerCase();
  const display = shorthand[normalized] ?? genreName;
  return { label: display };
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
    const hasFavorites = Array.isArray(favoriteGameIds) && favoriteGameIds.length > 0;
    if (currentStreamingGame) {
      // TOP_CATEGORIES: current, settings, all, favorites, ...genres
      return hasFavorites ? 3 : 0;
    }
    // TOP_CATEGORIES without `current`: settings, all, favorites, ...genres
    return hasFavorites ? 2 : 1;
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
  const favoriteGameIdSet = useMemo(() => new Set(favoriteGameIds), [favoriteGameIds]);
  const [selectedSettingIndex, setSelectedSettingIndex] = useState(0);
  const [microphoneDevices, setMicrophoneDevices] = useState<{ deviceId: string; label: string }[]>([]);
  const [settingsSubcategory, setSettingsSubcategory] = useState<SettingsSubcategory>("root");
  const [lastRootSettingIndex, setLastRootSettingIndex] = useState(0);
  const [mediaSubcategory, setMediaSubcategory] = useState<MediaSubcategory>("root");
  const [lastRootMediaIndex, setLastRootMediaIndex] = useState(0);
  const [selectedMediaIndex, setSelectedMediaIndex] = useState(0);
  const [mediaLoading, setMediaLoading] = useState(false);
  const [mediaError, setMediaError] = useState<string | null>(null);
  const [mediaVideos, setMediaVideos] = useState<MediaListingEntry[]>([]);
  const [mediaScreenshots, setMediaScreenshots] = useState<MediaListingEntry[]>([]);
  const [mediaThumbById, setMediaThumbById] = useState<Record<string, string>>({});
  const [controllerType, setControllerType] = useState<"ps" | "xbox" | "nintendo" | "generic">("generic");
  const [editingBandwidth, setEditingBandwidth] = useState(false);

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
    categories.push({ id: "all", label: "All" });
    categories.push({ id: "favorites", label: "Favorites" });
    for (const genre of allGenres) categories.push({ id: `genre:${genre}`, label: sanitizeGenreName(genre) });
    return categories;
  }, [allGenres, currentStreamingGame]);

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
        { id: "exitControllerMode", label: "Exit Controller Mode", value: "" },
      ],
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

  const displayItems = useMemo(() => {
    if (topCategory === "current") return currentGameItems;
    if (topCategory === "settings") return settingsBySubcategory[settingsSubcategory] ?? [];
    if (topCategory === "media" && mediaSubcategory === "root") return mediaRootItems;
    return [];
  }, [topCategory, currentGameItems, settingsBySubcategory, settingsSubcategory, mediaSubcategory, mediaRootItems]);

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
    if (topCategory === "settings") return [];
    if (topCategory === "favorites") return games.filter((game) => favoriteGameIdSet.has(game.id));
    if (topCategory.startsWith("genre:")) {
      const genreName = topCategory.slice(6);
      return games.filter((game) => game.genres?.includes(genreName));
    }
    return games;
  }, [games, favoriteGameIdSet, topCategory]);

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



  useEffect(() => {
    const container = itemsContainerRef.current;
    if (!container) return;
    const children = Array.from(container.children) as HTMLElement[];
    if (children.length === 0 || selectedIndex >= children.length) return;
    let offset = 0;
    for (let i = 0; i < selectedIndex; i++) {
      const childStyle = window.getComputedStyle(children[i]);
      offset += children[i].offsetHeight + parseFloat(childStyle.marginBottom);
    }
    offset += children[selectedIndex].offsetHeight / 2;
    setListTranslateY(-offset);
  }, [selectedIndex, categorizedGames]);

  const throttledOnSelectGame = useCallback((id: string) => onSelectGame(id), [onSelectGame]);

  const toggleFavoriteForSelected = useCallback(() => {
    if (selectedGame) {
      onToggleFavoriteGame(selectedGame.id);
      playUiSound("confirm");
    }
  }, [onToggleFavoriteGame, playUiSound, selectedGame]);

  useEffect(() => {
    const applyDirection = (direction: Direction): void => {
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
      if (direction === "left") {
        playUiSound("move");
        // Cycle main categories (settings always resets to root)
        setCategoryIndex((prev) => (prev - 1 + TOP_CATEGORIES.length) % TOP_CATEGORIES.length);
        setSelectedSettingIndex(0);
        setSettingsSubcategory("root");
        setSelectedMediaIndex(0);
        setMediaSubcategory("root");
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
      if (topCategory === "media") {
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
      if (categorizedGames.length === 0) return;
      if (direction === "up") {
        const nextIndex = Math.max(0, selectedIndex - 1);
        if (nextIndex !== selectedIndex) {
          playUiSound("move");
          throttledOnSelectGame(categorizedGames[nextIndex].id);
        }
        return;
      }
      if (direction === "down") {
        const nextIndex = Math.min(categorizedGames.length - 1, selectedIndex + 1);
        if (nextIndex !== selectedIndex) {
          playUiSound("move");
          throttledOnSelectGame(categorizedGames[nextIndex].id);
        }
        return;
      }
    };

    const handler = (e: any) => {
      if (e.detail?.direction) applyDirection(e.detail.direction);
    };

    const activateHandler = () => {
      // If currently editing bandwidth, A confirms and exits edit mode
      if (topCategory === "settings" && settingsSubcategory !== "root" && editingBandwidth) {
        setEditingBandwidth(false);
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
      } else if (selectedGame) {
        onPlayGame(selectedGame);
        playUiSound("confirm");
      }
    };

    const secondaryActivateHandler = () => {
        if (topCategory === "current") {
          // X button does nothing on current game menu items
          return;
        }
        if (topCategory === "settings") {
          // X button cycles through setting values (no-op for exit actions or subcategory items at root)
          const setting = displayItems[selectedSettingIndex];
          if (!setting || !onSettingChange) return;
          if (setting.id === "exitApp" || setting.id === "exitControllerMode") return;
          // Skip X cycling for subcategory items at root
          if (settingsSubcategory === "root" && (setting.id === "network" || setting.id === "audio" || setting.id === "video" || setting.id === "system")) return;

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
      if (topCategory !== "settings" && topCategory !== "current") {
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
      }
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
      if (e.key === "Backspace" || e.key === "Escape") {
        if (topCategory === "settings" && settingsSubcategory !== "root") {
          cancelHandler(e);
          return;
        }
        if (topCategory === "media" && mediaSubcategory !== "root") {
          cancelHandler(e);
          return;
        }
        e.preventDefault();
        if (topCategory === "current" || topCategory === "settings") {
          setCategoryIndex((prev) => (prev - 1 + TOP_CATEGORIES.length) % TOP_CATEGORIES.length);
          setSelectedSettingIndex(0);
          setSettingsSubcategory("root");
          setSelectedMediaIndex(0);
          setMediaSubcategory("root");
        } else {
          onOpenSettings?.();
        }
      }
    };

    window.addEventListener("opennow:controller-direction", handler);
    window.addEventListener("opennow:controller-activate", activateHandler);
    window.addEventListener("opennow:controller-secondary-activate", secondaryActivateHandler);
    window.addEventListener("opennow:controller-tertiary-activate", tertiaryActivateHandler);
    window.addEventListener("opennow:controller-cancel", cancelHandler);
    window.addEventListener("keydown", kbdHandler);
    return () => {
      window.removeEventListener("opennow:controller-direction", handler);
      window.removeEventListener("opennow:controller-activate", activateHandler);
      window.removeEventListener("opennow:controller-secondary-activate", secondaryActivateHandler);
      window.removeEventListener("opennow:controller-tertiary-activate", tertiaryActivateHandler);
      window.removeEventListener("opennow:controller-cancel", cancelHandler);
      window.removeEventListener("keydown", kbdHandler);
    };
  }, [isLoading, TOP_CATEGORIES.length, categorizedGames, selectedIndex, selectedGame, selectedVariantId, onPlayGame, onSelectGameVariant, onOpenSettings, playUiSound, throttledOnSelectGame, toggleFavoriteForSelected, topCategory, selectedSettingIndex, selectedMediaIndex, displayItems, mediaAssetItems.length, mediaSubcategory, settings, settingsBySubcategory, settingsSubcategory, lastRootSettingIndex, lastRootMediaIndex, onSettingChange, resolutionOptions, fpsOptions, codecOptions, aspectRatioOptions, currentStreamingGame, onResumeGame, onCloseGame, onExitControllerMode, onExitApp, editingBandwidth]);

  const wrapperClassName = `xmb-wrapper ${settings.controllerBackgroundAnimations ? "xmb-animate" : "xmb-static"} ${isEntering ? "xmb-entering" : "xmb-ready"}`;

  if (isLoading && topCategory !== "settings" && topCategory !== "current" && topCategory !== "media") return <div className={wrapperClassName}><div className="xmb-bg-layer"><div className="xmb-bg-gradient" /></div><div style={{display:'flex',justifyContent:'center',alignItems:'center',height:'100vh'}}>Loading...</div></div>;

  return (
    <div className={wrapperClassName}>
      <div className="xmb-bg-layer">
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

      <div className="xmb-selection-focus" />

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
                  <div className="xmb-category-label">{label}</div>
                </div>
              );
            })}
      </div>

      {topCategory !== "settings" && topCategory !== "current" && topCategory !== "media" && (
      <div
        ref={itemsContainerRef}
        className="xmb-items-container"
        role="listbox"
        aria-label="Game library"
        style={{
          transform: `translate(${-GAME_ACTIVE_CENTER_OFFSET_X_PX}px, ${listTranslateY}px)`,
        }}
      >
        {categorizedGames.map((game, idx) => {
          const isActive = idx === selectedIndex;
          const record = playtimeData[game.id];
          const totalSecs = record?.totalSeconds ?? 0;
          const lastPlayedAt = record?.lastPlayedAt ?? null;
          const sessionCount = record?.sessionCount ?? 0;
          const playtimeLabel = formatPlaytime(totalSecs);
          const lastPlayedLabel = formatLastPlayed(lastPlayedAt);
          const genres = game.genres?.slice(0, 2) ?? [];
          const tierLabel = game.membershipTierLabel;

          return (
            <div key={game.id} className={`xmb-game-item ${isActive ? 'active' : ''}`} role="option" aria-selected={isActive}>
              {favoriteGameIdSet.has(game.id) && (
              <Star className="xmb-game-favorite-icon" />
            )}
            <div className="xmb-game-poster-container">
                <img src={game.imageUrl} alt={game.title} className="xmb-game-poster" />
            </div>
              <div className="xmb-game-info">
                <div className="xmb-game-title">{game.title}</div>

                <div className="xmb-game-meta">
                  {(() => {
                    const vId = selectedVariantByGameId[game.id] || game.variants[0]?.id;
                    const variant = game.variants.find(v => v.id === vId) || game.variants[0];
                    const storeName = getStoreDisplayName(variant?.store || "");
                    return storeName ? (
                      <span className="xmb-game-meta-chip xmb-game-meta-chip--store">{storeName}</span>
                    ) : null;
                  })()}

                  <span className="xmb-game-meta-chip xmb-game-meta-chip--playtime">
                    <Clock size={10} className="xmb-meta-icon" />
                    {playtimeLabel}
                  </span>

                  <span className="xmb-game-meta-chip xmb-game-meta-chip--last-played">
                    <Calendar size={10} className="xmb-meta-icon" />
                    {lastPlayedLabel}
                  </span>
                </div>

                {isActive && (
                  <div className="xmb-game-meta xmb-game-meta--expanded">
                    {sessionCount > 0 && (
                      <span className="xmb-game-meta-chip xmb-game-meta-chip--sessions">
                        <Repeat2 size={10} className="xmb-meta-icon" />
                        {sessionCount === 1 ? "1 session" : `${sessionCount} sessions`}
                      </span>
                    )}
                    {genres.map((g) => (
                      <span key={g} className="xmb-game-meta-chip xmb-game-meta-chip--genre">{sanitizeGenreName(g)}</span>
                    ))}
                    {tierLabel && (
                      <span className="xmb-game-meta-chip xmb-game-meta-chip--tier">{tierLabel}</span>
                    )}
                  </div>
                )}
              </div>
            </div>
          );
        })}
      </div>
      )}

      {(topCategory === "settings" || topCategory === "current" || (topCategory === "media" && mediaSubcategory === "root")) && (
      <div
        ref={itemsContainerRef}
        className="xmb-items-container"
        role="listbox"
        aria-label={topCategory === "current" ? "Current game actions" : topCategory === "settings" ? "Controller settings" : "Media categories"}
        style={{
          transform: `translate(${-GAME_ACTIVE_CENTER_OFFSET_X_PX}px, ${-(topCategory === "media" ? selectedMediaIndex : selectedSettingIndex) * 120}px)`,
        }}
      >
        {displayItems.map((item, idx) => {
          const isActive = idx === (topCategory === "media" ? selectedMediaIndex : selectedSettingIndex);
          const isSubcategoryItem = settingsSubcategory === "root" && (item.id === "network" || item.id === "audio" || item.id === "video" || item.id === "system");
          const isMediaSubcategoryItem = topCategory === "media" && mediaSubcategory === "root" && (item.id === "videos" || item.id === "screenshots");
          return (
            <div 
              key={item.id} 
              className={`xmb-game-item ${isActive ? 'active' : ''}`}
              role="option"
              aria-selected={isActive}
              data-subcategory-id={isSubcategoryItem || isMediaSubcategoryItem ? item.id : undefined}
            >
              <div className="xmb-game-info">
                <div className="xmb-game-title">{item.label}</div>
                {item.value && (
                  <div className="xmb-game-meta">
                    {item.id === 'bandwidth' && settingsSubcategory !== 'root' ? (
                      <div style={{display:'flex',alignItems:'center',gap:12}}>
                        <input
                          type="range"
                          min={1}
                          max={150}
                          step={1}
                          value={(settings.maxBitrateMbps ?? 75)}
                          onChange={(e) => onSettingChange && onSettingChange("maxBitrateMbps" as any, Number(e.target.value) as any)}
                          aria-label="Bandwidth Limit (Mbps)"
                          style={editingBandwidth ? {outline: '2px solid rgba(255,255,255,0.2)'} : undefined}
                        />
                        <span className="xmb-game-meta-chip">{`${settings.maxBitrateMbps ?? 75} Mbps`}{editingBandwidth ? ' • Editing' : ''}</span>
                      </div>
                    ) : (
                      <span className="xmb-game-meta-chip">{item.value}</span>
                    )}
                  </div>
                )}
              </div>
            </div>
          );
        })}
      </div>
      )}

      {topCategory === "media" && mediaSubcategory !== "root" && (
      <div
        ref={itemsContainerRef}
        className="xmb-items-container"
        role="listbox"
        aria-label={`${mediaSubcategory} media`}
        style={{
          transform: `translate(${-GAME_ACTIVE_CENTER_OFFSET_X_PX}px, ${-selectedMediaIndex * 120}px)`,
        }}
      >
        {mediaLoading && (
          <div className="xmb-game-item active">
            <div className="xmb-game-info">
              <div className="xmb-game-title">Loading {mediaSubcategory}...</div>
            </div>
          </div>
        )}

        {!mediaLoading && mediaError && (
          <div className="xmb-game-item active">
            <div className="xmb-game-info">
              <div className="xmb-game-title">{mediaError}</div>
            </div>
          </div>
        )}

        {!mediaLoading && !mediaError && mediaAssetItems.length === 0 && (
          <div className="xmb-game-item active">
            <div className="xmb-game-info">
              <div className="xmb-game-title">No {mediaSubcategory.toLowerCase()} found</div>
            </div>
          </div>
        )}

        {!mediaLoading && !mediaError && mediaAssetItems.map((item, idx) => {
          const isActive = idx === selectedMediaIndex;
          const thumb = mediaThumbById[item.id];
          const dateLabel = new Date(item.createdAtMs).toLocaleDateString();
          const durationMs = item.durationMs ?? 0;
          const hasDuration = durationMs > 0;
          const durationLabel = hasDuration ? `${Math.max(1, Math.round(durationMs / 1000))}s` : "Screenshot";

          return (
            <div key={item.id} className={`xmb-game-item ${isActive ? "active" : ""}`} role="option" aria-selected={isActive}>
              <div className="xmb-game-poster-container">
                {thumb ? <img src={thumb} alt={item.gameTitle || item.fileName} className="xmb-game-poster" /> : <div className="xmb-game-poster" />}
              </div>
              <div className="xmb-game-info">
                <div className="xmb-game-title">{item.gameTitle || item.fileName}</div>
                <div className="xmb-game-meta">
                  <span className="xmb-game-meta-chip">{durationLabel}</span>
                  <span className="xmb-game-meta-chip">{dateLabel}</span>
                </div>
              </div>
            </div>
          );
        })}
      </div>
      )}

      <div className={`xmb-detail-layer ${selectedGame ? 'visible' : ''}`}>
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

      <div className="xmb-footer">
        {topCategory === "current" ? (
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
                  <span>Back</span>
                </div>
              </>
            )}
          </>
        ) : (
          <>
            {
              (() => {
                const Primary = controllerType === "ps" ? ButtonPSCross : ButtonA;
                const Left = controllerType === "ps" ? ButtonPSSquare : ButtonX;
                const Top = controllerType === "ps" ? ButtonPSTriangle : ButtonY;
                return (
                  <>
                    <div className="xmb-btn-hint"><Primary className="xmb-btn-icon" size={24} /> <span>Play</span></div>
                    <div className="xmb-btn-hint"><Left className="xmb-btn-icon" size={24} /> <span>Variant</span></div>
                    <div className="xmb-btn-hint"><Top className="xmb-btn-icon" size={24} /> <span>Favorite</span></div>
                  </>
                );
              })()
            }
          </>
        )}
      </div>
    </div>
  );
}
