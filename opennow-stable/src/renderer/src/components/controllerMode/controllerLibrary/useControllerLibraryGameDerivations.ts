import { useMemo } from "react";
import type { GameInfo } from "@shared/gfn";
import { PREVIEW_TILE_COUNT, SPOTLIGHT_RECENT_COUNT } from "./constants";
import { sanitizeGenreName } from "./helpers";
import type {
  GameSubcategory,
  LibrarySortId,
  SpotlightEntry,
  TopCategory,
} from "./types";
import type { PlaytimeStore } from "../../../utils/usePlaytime";

type UseControllerLibraryGameDerivationsArgs = {
  games: GameInfo[];
  favoriteGameIds: string[];
  playtimeData: PlaytimeStore;
  topCategory: TopCategory;
  currentStreamingGame?: GameInfo | null;
  /** Shown as the first row label on the Home shelf (stream or last-played title). */
  homeShelfGameTitle?: string | null;
  /** Resume / Home context game id — excluded from Featured pick. */
  resumeContextGameId?: string | null;
  gameSubcategory: GameSubcategory;
  selectedGameId: string;
  selectedVariantByGameId: Record<string, string>;
  cloudSessionResumable: boolean;
  cloudResumeTitle: string | null;
  cloudResumeCoverUrl: string | null;
  cloudResumeBusy: boolean;
  onResumeCloudSession?: () => void;
  inStreamMenu: boolean;
  streamMenuMicOn: boolean;
  streamMenuMicLevel: number;
  streamMenuVolume: number;
  streamMenuIsFullscreen: boolean;
  endSessionConfirm: boolean;
  librarySortId: LibrarySortId;
};

type UseControllerLibraryGameDerivationsResult = {
  favoriteGameIdSet: Set<string>;
  favoriteGames: GameInfo[];
  allGenres: string[];
  currentGameItems: Array<{ id: string; label: string; value: string }>;
  mediaRootItems: Array<{ id: "videos" | "screenshots"; label: string; value: string }>;
  gameRootItems: Array<{ id: GameSubcategory; label: string; value: string }>;
  categorizedGames: GameInfo[];
  gamesSortedByRecent: GameInfo[];
  spotlightEntries: SpotlightEntry[];
  gameCategoryPreviewById: Record<string, string[]>;
  parallaxBackdropTiles: Array<{
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
  selectedIndex: number;
  selectedGame: GameInfo | null;
  selectedVariantId: string;
  selectedGameDescription: string;
  selectedGameSessionState: string | null;
  featuredHomeGame: GameInfo | null;
};

const isNonEmptyString = (value: string | undefined): value is string => typeof value === "string" && value.length > 0;

function pickFeaturedGameDeterministic(pool: GameInfo[]): GameInfo {
  const sorted = [...pool].sort((a, b) => a.id.localeCompare(b.id));
  const seed = sorted.map((g) => g.id).join("|");
  let h = 0;
  for (let i = 0; i < seed.length; i++) {
    h = (h * 31 + seed.charCodeAt(i)) | 0;
  }
  return sorted[Math.abs(h) % sorted.length]!;
}

export function useControllerLibraryGameDerivations({
  games,
  favoriteGameIds,
  playtimeData,
  topCategory,
  currentStreamingGame,
  homeShelfGameTitle,
  resumeContextGameId = null,
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
}: UseControllerLibraryGameDerivationsArgs): UseControllerLibraryGameDerivationsResult {
  const favoriteGameIdSet = useMemo(() => new Set(favoriteGameIds), [favoriteGameIds]);
  const favoriteGames = useMemo(() => games.filter((game) => favoriteGameIdSet.has(game.id)), [games, favoriteGameIdSet]);

  const featuredHomeGame = useMemo((): GameInfo | null => {
    const excludeId = resumeContextGameId ?? undefined;
    const candidates = excludeId ? games.filter((g) => g.id !== excludeId) : [...games];
    if (candidates.length === 0) return null;
    const playSecs = (id: string) => playtimeData[id]?.totalSeconds ?? 0;
    const favUnplayed = candidates.filter((g) => favoriteGameIdSet.has(g.id) && playSecs(g.id) === 0);
    if (favUnplayed.length > 0) return pickFeaturedGameDeterministic(favUnplayed);
    const anyUnplayed = candidates.filter((g) => playSecs(g.id) === 0);
    if (anyUnplayed.length > 0) return pickFeaturedGameDeterministic(anyUnplayed);
    const favs = candidates.filter((g) => favoriteGameIdSet.has(g.id));
    if (favs.length > 0) return pickFeaturedGameDeterministic(favs);
    return pickFeaturedGameDeterministic(candidates);
  }, [games, favoriteGameIdSet, playtimeData, resumeContextGameId]);

  const allGenres = useMemo(() => {
    const genreSet = new Set<string>();
    for (const game of games) {
      if (game.genres && Array.isArray(game.genres)) {
        for (const genre of game.genres) genreSet.add(genre);
      }
    }
    return Array.from(genreSet).sort();
  }, [games]);

  const currentGameItems = useMemo(() => {
    const streamExtras = inStreamMenu
      ? [
          { id: "toggleMic", label: "Microphone", value: streamMenuMicOn ? "On" : "Off" },
          { id: "streamMicLevel", label: "Mic level", value: `${Math.round((streamMenuMicLevel ?? 1) * 100)}%` },
          { id: "streamVolume", label: "Stream volume", value: `${Math.round((streamMenuVolume ?? 1) * 100)}%` },
          { id: "openMedia", label: "Media & captures", value: "Open" },
          { id: "toggleFullscreen", label: "Fullscreen", value: streamMenuIsFullscreen ? "On" : "Off" },
        ]
      : [];
    const resumeLabel = homeShelfGameTitle?.trim() || "Last played";
    const homeHead: Array<{ id: string; label: string; value: string }> = [{ id: "resume", label: resumeLabel, value: "" }];
    if (!inStreamMenu && featuredHomeGame) {
      homeHead.push({ id: "featured", label: featuredHomeGame.title, value: "" });
    }
    return [
      ...homeHead,
      ...streamExtras,
      ...(inStreamMenu
        ? [{ id: "closeGame", label: endSessionConfirm ? "End session (confirm)" : "Close Game", value: "" }]
        : []),
    ];
  }, [
    inStreamMenu,
    endSessionConfirm,
    streamMenuMicOn,
    streamMenuMicLevel,
    streamMenuVolume,
    streamMenuIsFullscreen,
    homeShelfGameTitle,
    featuredHomeGame,
  ]);

  const mediaRootItems = useMemo(
    () => [
      { id: "videos" as const, label: "Videos", value: "" },
      { id: "screenshots" as const, label: "Screenshots", value: "" },
    ],
    [],
  );

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
    const recentSlots: SpotlightEntry[] = played.map((g) => ({ kind: "recent", game: g }));
    while (recentSlots.length < recentCap) recentSlots.push({ kind: "recent", game: null });
    if (!showResume) return recentSlots;
    return [
      {
        kind: "cloudResume",
        title: cloudResumeTitle?.trim() || "Cloud session",
        coverUrl: cloudResumeCoverUrl ?? null,
        busy: Boolean(cloudResumeBusy),
      },
      ...recentSlots,
    ];
  }, [games, playtimeData, cloudSessionResumable, onResumeCloudSession, cloudResumeTitle, cloudResumeCoverUrl, cloudResumeBusy]);

  const gameCategoryPreviewById = useMemo(() => {
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
      return randomize(unique).slice(0, PREVIEW_TILE_COUNT);
    };
    const previews: Record<string, string[]> = {};
    previews.all = toFilledPreview(gamesSortedByRecent.map((g) => g.imageUrl).filter(isNonEmptyString));
    previews.favorites = toFilledPreview(favoriteGames.map((g) => g.imageUrl).filter(isNonEmptyString));
    for (const genre of allGenres) {
      const key = `genre:${genre}`;
      previews[key] = toFilledPreview(
        gamesSortedByRecent
          .filter((g) => g.genres?.includes(genre))
          .map((g) => g.imageUrl)
          .filter(isNonEmptyString),
      );
    }
    return previews;
  }, [allGenres, favoriteGames, gamesSortedByRecent]);

  const parallaxBackdropTiles = useMemo(() => {
    const unique = Array.from(new Set(games.map((g) => g.imageUrl).filter(isNonEmptyString)));
    if (unique.length === 0) return [];
    const shuffled = [...unique];
    for (let i = shuffled.length - 1; i > 0; i--) {
      const j = Math.floor(Math.random() * (i + 1));
      [shuffled[i], shuffled[j]] = [shuffled[j], shuffled[i]];
    }
    return shuffled.slice(0, 16).map((src, idx) => {
      const lane = (idx % 3) as 0 | 1 | 2;
      const drift = 6 + Math.random() * 12;
      return {
        src,
        lane,
        left: 4 + Math.random() * 88,
        delaySec: -(Math.random() * 54),
        scale: 0.88 + Math.random() * 0.34,
        xFrom: -drift,
        xTo: drift,
        rotFrom: -6 + Math.random() * 6,
        rotTo: 1 + Math.random() * 8,
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

  return {
    favoriteGameIdSet,
    favoriteGames,
    allGenres,
    currentGameItems,
    mediaRootItems,
    gameRootItems,
    categorizedGames,
    gamesSortedByRecent,
    spotlightEntries,
    gameCategoryPreviewById,
    parallaxBackdropTiles,
    selectedIndex,
    selectedGame,
    selectedVariantId,
    selectedGameDescription,
    selectedGameSessionState,
    featuredHomeGame,
  };
}
