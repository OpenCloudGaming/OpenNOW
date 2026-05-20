import { Library, Search, Clock, Gamepad2, Loader2, ArrowUpDown, Info, MoreHorizontal, Menu } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { JSX } from "react";
import type { CatalogSortOption, GameInfo } from "@shared/gfn";
import { GameCard } from "./GameCard";
import { useTranslation } from "../i18n";
import { formatCatalogLastPlayed } from "../utils/lastPlayedFormat";

const CONTROLLER_HERO_ROTATION_MS = 8000;
const CONTROLLER_MOVE_REPEAT_MS = 220;

const CONTROLLER_HERO_BACKGROUND_KEYS = [
  "MARQUEE_HERO_IMAGE",
  "FEATURE_IMAGE",
  "HERO_IMAGE",
  "TV_BANNER",
  "KEY_ART",
  "KEY_IMAGE",
] as const;

export interface LibraryPageProps {
  games: GameInfo[];
  searchQuery: string;
  onSearchChange: (query: string) => void;
  onPlayGame: (game: GameInfo) => void;
  isLoading: boolean;
  selectedGameId: string;
  onSelectGame: (id: string) => void;
  selectedVariantByGameId: Record<string, string>;
  onSelectGameVariant: (gameId: string, variantId: string) => void;
  libraryCount: number;
  sortOptions: CatalogSortOption[];
  selectedSortId: string;
  onSortChange: (sortId: string) => void;
  controllerMode?: boolean;
  featuredGames?: GameInfo[];
}

function appendUnique(values: string[], candidate: string | undefined): void {
  if (!candidate || values.includes(candidate)) return;
  values.push(candidate);
}

function appendImageType(values: string[], game: GameInfo, type: string): void {
  for (const candidate of game.imageUrlsByType?.[type] ?? []) {
    appendUnique(values, candidate);
  }
}

function getControllerHeroBackgroundCandidates(game: GameInfo): string[] {
  const candidates: string[] = [];
  for (const type of CONTROLLER_HERO_BACKGROUND_KEYS) {
    appendImageType(candidates, game, type);
  }
  appendUnique(candidates, game.heroImageUrl);
  appendUnique(candidates, game.imageUrl);
  for (const candidate of game.screenshotUrls ?? []) appendUnique(candidates, candidate);
  appendUnique(candidates, game.screenshotUrl);
  return candidates;
}

function getControllerHeroLogoUrl(game: GameInfo): string | undefined {
  return game.imageUrlsByType?.GAME_LOGO?.find(Boolean);
}

function getControllerFeaturedGames(featuredGames: GameInfo[], fallbackGames: GameInfo[]): GameInfo[] {
  const source = featuredGames.length > 0 ? featuredGames : fallbackGames;
  return source.slice(0, 6);
}

function getGameStoreSummary(game: GameInfo, fallback: string): string {
  const stores = [...new Set((game.availableStores?.length ? game.availableStores : game.variants.map((variant) => variant.store)).filter(Boolean))];
  if (stores.length === 0) return fallback;
  const visible = stores.slice(0, 3).join(", ");
  return stores.length > 3 ? `${visible} +${stores.length - 3}` : visible;
}

export function LibraryPage({
  games,
  searchQuery,
  onSearchChange,
  onPlayGame,
  isLoading,
  selectedGameId,
  onSelectGame,
  selectedVariantByGameId,
  onSelectGameVariant,
  libraryCount,
  sortOptions,
  selectedSortId,
  onSortChange,
  controllerMode = false,
  featuredGames = [],
}: LibraryPageProps): JSX.Element {
  const { t } = useTranslation();
  const [controllerHeroIndex, setControllerHeroIndex] = useState(0);
  const [detailsGame, setDetailsGame] = useState<GameInfo | null>(null);
  const gamepadPreviousButtonsRef = useRef(0);
  const gamepadLastMoveAtRef = useRef(0);
  const gamepadFrameRef = useRef<number | null>(null);
  const controllerGameRowRef = useRef<HTMLDivElement | null>(null);
  const controllerInputStateRef = useRef({
    detailsGame: null as GameInfo | null,
    selectedControllerGame: undefined as GameInfo | undefined,
    selectedControllerGameIndex: 0,
    selectedSortId: "",
    sortOptions: [] as CatalogSortOption[],
    focusControllerGame: (_index: number): void => {},
    cycleSelectedVariant: (): void => {},
    onPlayGame: (_game: GameInfo): void => {},
    onSortChange: (_sortId: string): void => {},
  });

  const controllerFeaturedGames = useMemo(
    () => getControllerFeaturedGames(featuredGames, games),
    [featuredGames, games],
  );

  useEffect(() => {
    if (!controllerMode) return;
    setControllerHeroIndex(0);
  }, [controllerMode, controllerFeaturedGames]);

  useEffect(() => {
    if (controllerMode) return;
    gamepadPreviousButtonsRef.current = 0;
    gamepadLastMoveAtRef.current = 0;
  }, [controllerMode]);

  useEffect(() => {
    if (!controllerMode || controllerFeaturedGames.length <= 1) return;
    const interval = window.setInterval(() => {
      setControllerHeroIndex((index) => (index + 1) % controllerFeaturedGames.length);
    }, CONTROLLER_HERO_ROTATION_MS);
    return () => window.clearInterval(interval);
  }, [controllerMode, controllerFeaturedGames.length]);

  useEffect(() => {
    if (!controllerMode || games.length === 0) return;
    if (games.some((game) => game.id === selectedGameId)) return;
    onSelectGame(games[0].id);
  }, [controllerMode, games, onSelectGame, selectedGameId]);

  const selectedControllerGameIndex = Math.max(0, games.findIndex((game) => game.id === selectedGameId));
  const selectedControllerGame = games[selectedControllerGameIndex] ?? games[0];

  const focusControllerGame = (index: number): void => {
    if (games.length === 0) return;
    const nextIndex = Math.max(0, Math.min(index, games.length - 1));
    const nextGame = games[nextIndex];
    onSelectGame(nextGame.id);
    window.requestAnimationFrame(() => {
      const row = controllerGameRowRef.current;
      const card = row?.querySelector<HTMLElement>(`[data-controller-game-id="${CSS.escape(nextGame.id)}"]`);
      card?.scrollIntoView({ inline: "nearest", block: "nearest", behavior: "auto" });
    });
  };

  const cycleSelectedVariant = (): void => {
    if (!selectedControllerGame || selectedControllerGame.variants.length <= 1) return;
    const activeVariantId = selectedVariantByGameId[selectedControllerGame.id];
    const activeIndex = Math.max(0, selectedControllerGame.variants.findIndex((variant) => variant.id === activeVariantId));
    const nextVariant = selectedControllerGame.variants[(activeIndex + 1) % selectedControllerGame.variants.length];
    if (nextVariant) onSelectGameVariant(selectedControllerGame.id, nextVariant.id);
  };

  useEffect(() => {
    controllerInputStateRef.current = {
      detailsGame,
      selectedControllerGame,
      selectedControllerGameIndex,
      selectedSortId,
      sortOptions,
      focusControllerGame,
      cycleSelectedVariant,
      onPlayGame,
      onSortChange,
    };
  }, [detailsGame, games, onPlayGame, onSelectGame, onSelectGameVariant, onSortChange, selectedControllerGame, selectedControllerGameIndex, selectedSortId, selectedVariantByGameId, sortOptions]);

  useEffect(() => {
    if (!controllerMode) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (detailsGame) {
        if (event.key === "Escape" || event.key.toLowerCase() === "b") {
          event.preventDefault();
          setDetailsGame(null);
        }
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onPlayGame(detailsGame);
        }
        return;
      }
      if (event.key === "ArrowLeft") {
        event.preventDefault();
        focusControllerGame(selectedControllerGameIndex - 1);
      } else if (event.key === "ArrowRight") {
        event.preventDefault();
        focusControllerGame(selectedControllerGameIndex + 1);
      } else if (event.key === "ArrowDown" || event.key.toLowerCase() === "x") {
        event.preventDefault();
        cycleSelectedVariant();
      } else if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        if (selectedControllerGame) onPlayGame(selectedControllerGame);
      } else if (event.key.toLowerCase() === "i") {
        event.preventDefault();
        if (selectedControllerGame) setDetailsGame(selectedControllerGame);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [controllerMode, detailsGame, games, onPlayGame, selectedControllerGame, selectedControllerGameIndex]);

  useEffect(() => {
    if (!controllerMode) return;
    const readButtons = (): number => {
      const pad = navigator.getGamepads?.().find((gamepad): gamepad is Gamepad => Boolean(gamepad));
      if (!pad) return 0;
      let buttons = 0;
      if (pad.buttons[0]?.pressed) buttons |= 1 << 0;
      if (pad.buttons[1]?.pressed) buttons |= 1 << 1;
      if (pad.buttons[3]?.pressed) buttons |= 1 << 2;
      if (pad.buttons[4]?.pressed) buttons |= 1 << 3;
      if (pad.buttons[5]?.pressed) buttons |= 1 << 4;
      if (pad.buttons[12]?.pressed || (pad.axes[1] ?? 0) < -0.65) buttons |= 1 << 5;
      if (pad.buttons[13]?.pressed || (pad.axes[1] ?? 0) > 0.65) buttons |= 1 << 6;
      if (pad.buttons[14]?.pressed || (pad.axes[0] ?? 0) < -0.65) buttons |= 1 << 7;
      if (pad.buttons[15]?.pressed || (pad.axes[0] ?? 0) > 0.65) buttons |= 1 << 8;
      if (pad.buttons[2]?.pressed) buttons |= 1 << 9;
      return buttons;
    };

    const handleGamepadFrame = () => {
      const buttons = readButtons();
      let pressed = buttons & ~gamepadPreviousButtonsRef.current;
      const moveMask = (1 << 5) | (1 << 6) | (1 << 7) | (1 << 8);
      const now = performance.now();
      const activeMoves = buttons & moveMask;
      const pressedMoves = pressed & moveMask;
      if (pressedMoves) {
        gamepadLastMoveAtRef.current = now;
      } else if (activeMoves && now - gamepadLastMoveAtRef.current > CONTROLLER_MOVE_REPEAT_MS) {
        pressed |= activeMoves;
        gamepadLastMoveAtRef.current = now;
      }

      const {
        detailsGame: currentDetailsGame,
        selectedControllerGame: currentSelectedGame,
        selectedControllerGameIndex: currentSelectedIndex,
        selectedSortId: currentSortId,
        sortOptions: currentSortOptions,
        focusControllerGame: focusGame,
        cycleSelectedVariant: cycleVariant,
        onPlayGame: playGame,
        onSortChange: changeSort,
      } = controllerInputStateRef.current;

      if (currentDetailsGame) {
        if (pressed & (1 << 0)) playGame(currentDetailsGame);
        if (pressed & (1 << 1)) setDetailsGame(null);
      } else {
        if (pressed & (1 << 0)) {
          if (currentSelectedGame) playGame(currentSelectedGame);
        }
        if (pressed & (1 << 7)) focusGame(currentSelectedIndex - 1);
        if (pressed & (1 << 8)) focusGame(currentSelectedIndex + 1);
        if ((pressed & (1 << 6)) || (pressed & (1 << 9))) cycleVariant();
        if (pressed & (1 << 2)) {
          const nextSort = currentSortOptions.find((option) => option.id !== currentSortId) ?? currentSortOptions[0];
          if (nextSort) changeSort(nextSort.id);
        }
      }
      gamepadPreviousButtonsRef.current = buttons;

      gamepadFrameRef.current = window.requestAnimationFrame(handleGamepadFrame);
    };

    const startGamepadNavigation = () => {
      if (gamepadFrameRef.current !== null) return;
      gamepadPreviousButtonsRef.current = readButtons();
      gamepadLastMoveAtRef.current = performance.now();
      gamepadFrameRef.current = window.requestAnimationFrame(handleGamepadFrame);
    };

    const stopGamepadNavigation = () => {
      if (gamepadFrameRef.current !== null) {
        window.cancelAnimationFrame(gamepadFrameRef.current);
        gamepadFrameRef.current = null;
      }
      gamepadPreviousButtonsRef.current = 0;
      gamepadLastMoveAtRef.current = 0;
    };

    const handleDisconnect = () => {
      const hasConnectedPad = navigator.getGamepads?.().some(Boolean) ?? false;
      if (!hasConnectedPad) stopGamepadNavigation();
    };

    window.addEventListener("gamepadconnected", startGamepadNavigation);
    window.addEventListener("gamepaddisconnected", handleDisconnect);
    startGamepadNavigation();

    return () => {
      window.removeEventListener("gamepadconnected", startGamepadNavigation);
      window.removeEventListener("gamepaddisconnected", handleDisconnect);
      stopGamepadNavigation();
    };
  }, [controllerMode]);

  if (controllerMode) {
    const featuredGame = controllerFeaturedGames[controllerHeroIndex] ?? selectedControllerGame;
    const heroImageUrl = featuredGame ? getControllerHeroBackgroundCandidates(featuredGame)[0] : undefined;
    const heroLogoUrl = featuredGame ? getControllerHeroLogoUrl(featuredGame) : undefined;
    const dotCount = Math.min(Math.max(controllerFeaturedGames.length, 1), 6);
    const activeDotIndex = dotCount > 0 && controllerFeaturedGames.length > 0 ? Math.min(controllerHeroIndex, dotCount - 1) : 0;

    return (
      <div className="library-page controller-library-page">
        {isLoading ? (
          <div className="library-empty-state controller-library-empty">
            <Loader2 className="library-spinner" size={54} />
            <p>{t("library.empty.loadingLibrary")}</p>
          </div>
        ) : libraryCount === 0 ? (
          <div className="library-empty-state controller-library-empty">
            <Gamepad2 className="library-empty-icon" size={64} />
            <h3>{t("library.empty.libraryEmpty")}</h3>
            <p>{t("library.empty.ownedGamesAppearHere")}</p>
          </div>
        ) : featuredGame ? (
          <>
            <section className="controller-hero" aria-label={featuredGame.title}>
              {heroImageUrl ? (
                <img src={heroImageUrl} alt="" className="controller-hero-image" />
              ) : (
                <div className="controller-hero-placeholder" />
              )}
              <div className="controller-hero-scrim" />
              <div className="controller-hero-content">
                {heroLogoUrl ? (
                  <img src={heroLogoUrl} alt={featuredGame.title} className="controller-hero-logo" />
                ) : (
                  <h1>{featuredGame.title}</h1>
                )}
                <div className="controller-hero-actions">
                  <button type="button" className="controller-primary-action" onClick={() => onPlayGame(featuredGame)}>
                    {featuredGame.isInLibrary ? t("app.actions.play") : t("app.actions.buy")}
                  </button>
                  <button type="button" className="controller-secondary-action" onClick={() => setDetailsGame(featuredGame)}>
                    <Info size={22} />
                    <span>{t("library.moreInfo")}</span>
                  </button>
                  <button type="button" className="controller-icon-action" aria-label={t("library.moreOptions")} onClick={cycleSelectedVariant}>
                    <MoreHorizontal size={30} />
                  </button>
                </div>
              </div>
            </section>

            <div className="controller-hero-dots" aria-hidden="true">
              {Array.from({ length: dotCount }).map((_, index) => (
                <span key={index} className={index === activeDotIndex ? "active" : ""} />
              ))}
            </div>

            <section className="controller-library-strip" aria-label={t("library.title")}> 
              <div className="controller-library-heading">
                <h2>{t("library.controllerTitle")}</h2>
                <span>{t("library.gameCount", { count: libraryCount })}</span>
              </div>
              {games.length === 0 ? (
                <div className="library-empty-state controller-library-empty controller-library-empty--compact">
                  <Search className="library-empty-icon" size={44} />
                  <h3>{t("library.empty.noGamesFound")}</h3>
                  <p>{t("library.empty.noGamesMatch", { query: searchQuery })}</p>
                </div>
              ) : (
                <div className="controller-game-row" ref={controllerGameRowRef}>
                  {games.map((game) => (
                    <div key={game.id} className="controller-library-card" data-controller-game-id={game.id}>
                      <GameCard
                        game={game}
                        isSelected={game.id === selectedGameId}
                        onSelect={() => onSelectGame(game.id)}
                        onPlay={() => onPlayGame(game)}
                        selectedVariantId={selectedVariantByGameId[game.id]}
                        onSelectStore={(variantId) => onSelectGameVariant(game.id, variantId)}
                      />
                    </div>
                  ))}
                </div>
              )}
            </section>

            <div className="controller-bottom-hints" aria-hidden="true">
              <div className="controller-hint"><span className="controller-button controller-button--a">A</span><span>{t("app.actions.select")}</span></div>
              <div className="controller-hint"><span className="controller-button controller-button--b">B</span><span>{t("app.actions.back")}</span></div>
              <div className="controller-hint"><span className="controller-button controller-button--y">Y</span><span>{t("library.filter")}</span></div>
              <div className="controller-hint"><span className="controller-button controller-button--x">X</span><span>{t("app.actions.search")}</span></div>
              <div className="controller-hint controller-hint--more"><span className="controller-menu-button"><Menu size={22} /></span><span>{t("library.moreOptions")}</span></div>
            </div>

            {detailsGame && (
              <div className="controller-details-overlay" role="dialog" aria-modal="true" aria-label={detailsGame.title}>
                <div className="controller-details-panel">
                  <h3>{detailsGame.title}</h3>
                  <p className="controller-details-store">{t("library.selectedStore", { store: getGameStoreSummary(detailsGame, t("library.storeNotListed")) })}</p>
                  <p className="controller-details-body">{detailsGame.description || detailsGame.longDescription || detailsGame.featureLabels?.join(" / ") || t("library.loadingGameDetails")}</p>
                  <div className="controller-details-meta">
                    {detailsGame.publisherName && <span>{t("library.publisher", { publisher: detailsGame.publisherName })}</span>}
                    {detailsGame.genres?.length ? <span>{t("library.genres", { genres: detailsGame.genres.slice(0, 4).join(", ") })}</span> : null}
                    {detailsGame.contentRatings?.length ? <span>{t("library.rating", { rating: detailsGame.contentRatings.slice(0, 2).join(", ") })}</span> : null}
                  </div>
                  <div className="controller-details-actions">
                    <button type="button" className="controller-primary-action" onClick={() => onPlayGame(detailsGame)}>{t("app.actions.play")}</button>
                    <button type="button" className="controller-secondary-action" onClick={() => setDetailsGame(null)}>{t("app.actions.back")}</button>
                  </div>
                </div>
              </div>
            )}
          </>
        ) : null}
      </div>
    );
  }

  return (
    <div className="library-page">
      <header className="library-toolbar">
        <div className="library-title">
          <Library className="library-title-icon" size={22} />
          <h1>{t("library.title")}</h1>
        </div>

        <div className="library-search">
          <Search className="library-search-icon" size={16} />
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => onSearchChange(e.target.value)}
            placeholder={t("library.searchPlaceholder")}
            className="library-search-input"
          />
        </div>

        <label className="library-sort">
          <ArrowUpDown size={14} />
          <select value={selectedSortId} onChange={(e) => onSortChange(e.target.value)}>
            {sortOptions.map((option) => (
              <option key={option.id} value={option.id}>
                {option.label}
              </option>
            ))}
          </select>
        </label>

        <span className="library-count">{t("library.gameCount", { count: libraryCount })}</span>
      </header>

      <div className="library-grid-area">
        {isLoading ? (
          <div className="library-empty-state">
            <Loader2 className="library-spinner" size={36} />
            <p>{t("library.empty.loadingLibrary")}</p>
          </div>
        ) : libraryCount === 0 ? (
          <div className="library-empty-state">
            <Gamepad2 className="library-empty-icon" size={44} />
            <h3>{t("library.empty.libraryEmpty")}</h3>
            <p>{t("library.empty.ownedGamesAppearHere")}</p>
          </div>
        ) : games.length === 0 ? (
          <div className="library-empty-state">
            <Search className="library-empty-icon" size={44} />
            <h3>{t("library.empty.noGamesFound")}</h3>
            <p>{t("library.empty.noGamesMatch", { query: searchQuery })}</p>
          </div>
        ) : (
          <div className="game-grid">
            {games.map((game) => (
              <div key={game.id} className="library-game-wrapper">
                <GameCard
                  game={game}
                  isSelected={game.id === selectedGameId}
                  onSelect={() => onSelectGame(game.id)}
                  onPlay={() => onPlayGame(game)}
                  selectedVariantId={selectedVariantByGameId[game.id]}
                  onSelectStore={(variantId) => onSelectGameVariant(game.id, variantId)}
                />
                {game.lastPlayed && (
                  <div className="library-last-played">
                    <Clock size={12} />
                    <span>{formatCatalogLastPlayed(t, game.lastPlayed)}</span>
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
