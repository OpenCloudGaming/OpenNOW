import { Search, LayoutGrid, Loader2, ArrowUpDown, Filter, ChevronDown, Gamepad2, Menu } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { JSX } from "react";
import { isOwnedLibraryStatus } from "@shared/gfn";
import type { CatalogFilterGroup, CatalogSortOption, GameInfo, GamePanelResult, GameVariant } from "@shared/gfn";
import { GameCard, getStoreDisplayName, getStoreIconComponent } from "./GameCard";
import { useTranslation } from "../i18n";

const CONTROLLER_STORE_HERO_ROTATION_MS = 7000;
const CONTROLLER_MOVE_REPEAT_MS = 220;

const CONTROLLER_STORE_PROMINENT_IMAGE_KEYS = [
  "MARQUEE_HERO_IMAGE",
  "HERO_IMAGE",
  "TV_BANNER",
  "FEATURE_IMAGE",
  "KEY_ART",
  "KEY_IMAGE",
  "GAME_BOX_ART",
] as const;

const CONTROLLER_STORE_TILE_IMAGE_KEYS = [
  "TV_BANNER",
  "HERO_IMAGE",
  "KEY_IMAGE",
  "KEY_ART",
  "GAME_BOX_ART",
  "FEATURE_IMAGE",
] as const;

export interface HomePageProps {
  games: GameInfo[];
  searchQuery: string;
  onSearchChange: (query: string) => void;
  onPlayGame: (game: GameInfo) => void;
  isLoading: boolean;
  selectedGameId: string;
  onSelectGame: (id: string) => void;
  selectedVariantByGameId: Record<string, string>;
  onSelectGameVariant: (gameId: string, variantId: string) => void;
  filterGroups: CatalogFilterGroup[];
  selectedFilterIds: string[];
  onToggleFilter: (filterId: string) => void;
  sortOptions: CatalogSortOption[];
  selectedSortId: string;
  onSortChange: (sortId: string) => void;
  totalCount: number;
  supportedCount: number;
  controllerMode?: boolean;
  storePanels?: GamePanelResult[];
  storeHeroGames?: GameInfo[];
  activeSessionAppIds?: number[];
  onBuyGame?: (game: GameInfo, selectedVariantId?: string) => void;
  onPreviousControllerPage?: () => void;
  onNextControllerPage?: () => void;
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

function getSteamHeaderUrl(game: GameInfo): string | undefined {
  const steamVariant = game.variants.find((variant) => /^\d+$/.test(variant.id) && variant.store.toUpperCase().includes("STEAM"));
  const appId = steamVariant?.id ?? (/^\d+$/.test(game.launchAppId ?? "") ? game.launchAppId : undefined);
  return appId ? `https://cdn.cloudflare.steamstatic.com/steam/apps/${appId}/header.jpg` : undefined;
}

function getControllerStoreImageCandidates(game: GameInfo, prominent: boolean): string[] {
  const candidates: string[] = [];
  const keys = prominent ? CONTROLLER_STORE_PROMINENT_IMAGE_KEYS : CONTROLLER_STORE_TILE_IMAGE_KEYS;
  for (const type of keys) appendImageType(candidates, game, type);
  appendUnique(candidates, game.heroImageUrl);
  appendUnique(candidates, game.imageUrl);
  for (const screenshot of game.screenshotUrls ?? []) {
    appendUnique(candidates, screenshot);
    if (!prominent) break;
  }
  appendUnique(candidates, game.screenshotUrl);
  appendUnique(candidates, getSteamHeaderUrl(game));
  return candidates;
}

function getControllerStoreLogoUrl(game: GameInfo): string | undefined {
  return game.imageUrlsByType?.GAME_LOGO?.[0]
    ?? game.imageUrlsByType?.LOGO?.[0]
    ?? game.imageUrlsByType?.TITLE_LOGO?.[0];
}

function getSelectedVariant(game: GameInfo, selectedVariantId?: string): GameVariant | undefined {
  return game.variants.find((variant) => variant.id === selectedVariantId)
    ?? game.variants[game.selectedVariantIndex]
    ?? game.variants[0];
}

function storeVariantIsOwned(variant: GameVariant | undefined): boolean {
  return Boolean(variant?.inLibrary || variant?.librarySelected || isOwnedLibraryStatus(variant?.libraryStatus));
}

function getVariantDisplayName(variant: GameVariant | undefined, fallback: string): string {
  return variant?.store ? getStoreDisplayName(variant.store) : fallback;
}

function getPurchaseUrl(game: GameInfo, selectedVariantId?: string): string | undefined {
  const selectedVariant = getSelectedVariant(game, selectedVariantId);
  if (selectedVariant?.storeUrl) return selectedVariant.storeUrl;
  return game.variants.find((variant) => !storeVariantIsOwned(variant) && variant.storeUrl)?.storeUrl
    ?? game.variants.find((variant) => variant.storeUrl)?.storeUrl;
}

function gameNeedsPurchase(game: GameInfo, selectedVariantId?: string): boolean {
  const selectedVariant = getSelectedVariant(game, selectedVariantId);
  return !storeVariantIsOwned(selectedVariant);
}

function getNextVariantId(game: GameInfo, selectedVariantId?: string): string | undefined {
  if (game.variants.length === 0) return undefined;
  const activeIndex = Math.max(0, game.variants.findIndex((variant) => variant.id === selectedVariantId));
  return game.variants[(activeIndex + 1) % game.variants.length]?.id;
}

function gameMatchesActiveSession(game: GameInfo, activeSessionAppIds: number[]): boolean {
  if (activeSessionAppIds.length === 0) return false;
  const appIds = new Set(activeSessionAppIds.map(String));
  if (game.launchAppId && appIds.has(game.launchAppId)) return true;
  if (appIds.has(game.id)) return true;
  return game.variants.some((variant) => appIds.has(variant.id));
}

function getPrimaryGenre(game: GameInfo): string {
  return game.genres?.[0] ?? game.playType ?? "Cloud Game";
}

function getPrimaryStoreName(game: GameInfo, selectedVariantId?: string): string {
  const store = getSelectedVariant(game, selectedVariantId)?.store ?? game.availableStores?.[0] ?? "Cloud";
  const upper = store.toUpperCase();
  if (upper.includes("STEAM")) return "Steam";
  if (upper.includes("BATTLE")) return "Battle.net";
  if (upper.includes("UBISOFT") || upper.includes("UPLAY")) return "Ubisoft";
  if (upper.includes("XBOX")) return "Xbox";
  if (upper.includes("EPIC")) return "Epic";
  if (upper.includes("EA")) return "EA";
  return getStoreDisplayName(store);
}

function ControllerStoreTile({
  game,
  selectedVariantId,
  focused,
  onFocus,
  onBuy,
  onPlay,
}: {
  game: GameInfo;
  selectedVariantId?: string;
  focused: boolean;
  onFocus: () => void;
  onBuy: () => void;
  onPlay: () => void;
}): JSX.Element {
  const { t } = useTranslation();
  const imageUrl = getControllerStoreImageCandidates(game, false)[0];
  const selectedVariant = getSelectedVariant(game, selectedVariantId);
  const storeName = getPrimaryStoreName(game, selectedVariantId);
  const StoreIcon = getStoreIconComponent(selectedVariant?.store ?? storeName);
  const needsPurchase = gameNeedsPurchase(game, selectedVariantId);

  return (
    <div
      role="button"
      tabIndex={0}
      className={`controller-store-tile${focused ? " focused" : ""}`}
      onClick={onFocus}
      onDoubleClick={() => {
        if (needsPurchase) {
          onBuy();
          return;
        }
        onPlay();
      }}
      aria-label={game.title}
    >
      <span className="controller-store-tile-art">
        {imageUrl ? <img src={imageUrl} alt="" loading="lazy" /> : <span className="controller-store-tile-placeholder">{game.title.slice(0, 1)}</span>}
      </span>
      <span className="controller-store-tile-gradient" />
      <span className="controller-store-tile-shine" />
      <span className="controller-store-tile-accent" />
      <span className="controller-store-tile-badge">
        <StoreIcon />
        <span>{storeName}</span>
      </span>
      <span className={`controller-store-tile-ownership${needsPurchase ? " is-not-owned" : " is-owned"}`}>
        {needsPurchase ? t("home.controller.notOwned") : t("home.controller.owned")}
      </span>
      {!needsPurchase && (
        <span className="controller-store-tile-variant">
          {t("home.controller.variant", { variant: getVariantDisplayName(selectedVariant, storeName) })}
        </span>
      )}
      <button
        type="button"
        className="controller-store-tile-action"
        onClick={(event) => {
          event.stopPropagation();
          onFocus();
          if (needsPurchase) {
            onBuy();
            return;
          }
          onPlay();
        }}
      >
        {needsPurchase ? t("app.actions.buy") : t("app.actions.play")}
      </button>
    </div>
  );
}

export function HomePage({
  games,
  searchQuery,
  onSearchChange,
  onPlayGame,
  isLoading,
  selectedGameId,
  onSelectGame,
  selectedVariantByGameId,
  onSelectGameVariant,
  filterGroups,
  selectedFilterIds,
  onToggleFilter,
  sortOptions,
  selectedSortId,
  onSortChange,
  totalCount,
  supportedCount,
  controllerMode = false,
  storePanels = [],
  storeHeroGames = [],
  activeSessionAppIds = [],
  onBuyGame,
  onPreviousControllerPage,
  onNextControllerPage,
}: HomePageProps): JSX.Element {
  const { t } = useTranslation();
  const [controllerHeroIndex, setControllerHeroIndex] = useState(0);
  const [focusedRowIndex, setFocusedRowIndex] = useState(0);
  const [focusedColumnIndex, setFocusedColumnIndex] = useState(0);
  const rowRefs = useRef<Array<HTMLDivElement | null>>([]);
  const gamepadPreviousButtonsRef = useRef(0);
  const gamepadLastMoveAtRef = useRef(0);
  const gamepadFrameRef = useRef<number | null>(null);
  const controllerInputStateRef = useRef({
    focusTile: (_row: number, _column: number): void => {},
    launchFocusedTile: (): void => {},
    cycleFocusedVariant: (): boolean => false,
    focusedRowIndex: 0,
    focusedColumnIndex: 0,
    onPreviousControllerPage: undefined as (() => void) | undefined,
    onNextControllerPage: undefined as (() => void) | undefined,
  });

  const controllerSections = useMemo(
    () => storePanels.flatMap((panel) => panel.sections).filter((section) => section.games.length > 0),
    [storePanels],
  );
  const controllerHeroGames = useMemo(
    () => storeHeroGames.slice(0, 6),
    [storeHeroGames],
  );

  const focusTile = (rowIndex: number, columnIndex: number): void => {
    if (controllerSections.length === 0) return;
    const nextRowIndex = Math.max(0, Math.min(rowIndex, controllerSections.length - 1));
    const row = controllerSections[nextRowIndex];
    if (!row || row.games.length === 0) return;
    const nextColumnIndex = Math.max(0, Math.min(columnIndex, row.games.length - 1));
    const nextGame = row.games[nextColumnIndex];
    setFocusedRowIndex(nextRowIndex);
    setFocusedColumnIndex(nextColumnIndex);
    onSelectGame(nextGame.id);
    window.requestAnimationFrame(() => {
      const tile = rowRefs.current[nextRowIndex]?.querySelector<HTMLElement>(`[data-controller-store-column="${nextColumnIndex}"]`);
      tile?.scrollIntoView({ inline: "nearest", block: "nearest", behavior: "auto" });
      tile?.closest(".controller-store-section")?.scrollIntoView({ block: "nearest", inline: "nearest", behavior: "auto" });
    });
  };

  const launchGame = (game: GameInfo): void => {
    const selectedVariantId = selectedVariantByGameId[game.id];
    if (gameNeedsPurchase(game, selectedVariantId)) {
      onBuyGame?.(game, selectedVariantId);
      return;
    }
    onPlayGame(game);
  };

  const launchFocusedTile = (): void => {
    const game = controllerSections[focusedRowIndex]?.games[focusedColumnIndex];
    if (game) launchGame(game);
  };

  const cycleFocusedVariant = (): boolean => {
    const game = controllerSections[focusedRowIndex]?.games[focusedColumnIndex];
    if (!game || game.variants.length <= 1) return false;
    const nextVariantId = getNextVariantId(game, selectedVariantByGameId[game.id]);
    if (!nextVariantId) return false;
    onSelectGameVariant(game.id, nextVariantId);
    return true;
  };

  useEffect(() => {
    controllerInputStateRef.current = {
      focusTile,
      launchFocusedTile,
      cycleFocusedVariant,
      focusedRowIndex,
      focusedColumnIndex,
      onPreviousControllerPage,
      onNextControllerPage,
    };
  }, [cycleFocusedVariant, focusedColumnIndex, focusedRowIndex, focusTile, launchFocusedTile, onNextControllerPage, onPreviousControllerPage]);

  useEffect(() => {
    if (!controllerMode) return;
    setControllerHeroIndex(0);
  }, [controllerHeroGames, controllerMode]);

  useEffect(() => {
    if (!controllerMode || controllerHeroGames.length <= 1) return;
    const interval = window.setInterval(() => {
      setControllerHeroIndex((index) => (index + 1) % controllerHeroGames.length);
    }, CONTROLLER_STORE_HERO_ROTATION_MS);
    return () => window.clearInterval(interval);
  }, [controllerHeroGames.length, controllerMode]);

  useEffect(() => {
    if (!controllerMode || controllerSections.length === 0) return;
    const currentRow = controllerSections[focusedRowIndex];
    if (currentRow?.games.some((game) => game.id === selectedGameId)) return;
    focusTile(0, 0);
  }, [controllerMode, controllerSections, focusedRowIndex, selectedGameId]);

  useEffect(() => {
    if (!controllerMode) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "ArrowLeft") {
        event.preventDefault();
        focusTile(focusedRowIndex, focusedColumnIndex - 1);
      } else if (event.key === "ArrowRight") {
        event.preventDefault();
        focusTile(focusedRowIndex, focusedColumnIndex + 1);
      } else if (event.key === "ArrowUp") {
        event.preventDefault();
        focusTile(focusedRowIndex - 1, focusedColumnIndex);
      } else if (event.key === "ArrowDown") {
        event.preventDefault();
        if (!cycleFocusedVariant()) focusTile(focusedRowIndex + 1, focusedColumnIndex);
      } else if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        launchFocusedTile();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [controllerMode, cycleFocusedVariant, focusedColumnIndex, focusedRowIndex, focusTile, launchFocusedTile]);

  useEffect(() => {
    if (!controllerMode) return;
    const readButtons = (): number => {
      const pad = navigator.getGamepads?.().find((gamepad): gamepad is Gamepad => Boolean(gamepad));
      if (!pad) return 0;
      let buttons = 0;
      if (pad.buttons[0]?.pressed) buttons |= 1 << 0;
      if (pad.buttons[12]?.pressed || (pad.axes[1] ?? 0) < -0.65) buttons |= 1 << 2;
      if (pad.buttons[13]?.pressed || (pad.axes[1] ?? 0) > 0.65) buttons |= 1 << 3;
      if (pad.buttons[14]?.pressed || (pad.axes[0] ?? 0) < -0.65) buttons |= 1 << 4;
      if (pad.buttons[15]?.pressed || (pad.axes[0] ?? 0) > 0.65) buttons |= 1 << 5;
      if (pad.buttons[4]?.pressed) buttons |= 1 << 6;
      if (pad.buttons[5]?.pressed) buttons |= 1 << 7;
      return buttons;
    };

    const handleGamepadFrame = () => {
      const buttons = readButtons();
      let pressed = buttons & ~gamepadPreviousButtonsRef.current;
      const moveMask = (1 << 2) | (1 << 3) | (1 << 4) | (1 << 5);
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
        focusTile: focusControllerTile,
        launchFocusedTile: launchControllerTile,
        cycleFocusedVariant: cycleControllerVariant,
        focusedRowIndex: rowIndex,
        focusedColumnIndex: columnIndex,
        onPreviousControllerPage: previousControllerPage,
        onNextControllerPage: nextControllerPage,
      } = controllerInputStateRef.current;

      if (pressed & (1 << 0)) launchControllerTile();
      if (pressed & (1 << 6)) {
        previousControllerPage?.();
        gamepadPreviousButtonsRef.current = buttons;
        gamepadFrameRef.current = window.requestAnimationFrame(handleGamepadFrame);
        return;
      }
      if (pressed & (1 << 7)) {
        nextControllerPage?.();
        gamepadPreviousButtonsRef.current = buttons;
        gamepadFrameRef.current = window.requestAnimationFrame(handleGamepadFrame);
        return;
      }
      if (pressed & (1 << 2)) focusControllerTile(rowIndex - 1, columnIndex);
      if ((pressed & (1 << 3)) && !cycleControllerVariant()) focusControllerTile(rowIndex + 1, columnIndex);
      if (pressed & (1 << 4)) focusControllerTile(rowIndex, columnIndex - 1);
      if (pressed & (1 << 5)) focusControllerTile(rowIndex, columnIndex + 1);
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
    const heroGame = controllerHeroGames[controllerHeroIndex];
    const heroImageUrl = heroGame ? getControllerStoreImageCandidates(heroGame, true)[0] : undefined;
    const heroLogoUrl = heroGame ? getControllerStoreLogoUrl(heroGame) : undefined;
    const heroSelectedVariantId = heroGame ? selectedVariantByGameId[heroGame.id] : undefined;
    const heroDotCount = Math.min(Math.max(controllerHeroGames.length, 1), 6);
    const activeHeroDotIndex = controllerHeroGames.length > 0 ? Math.min(controllerHeroIndex % heroDotCount, heroDotCount - 1) : 0;

    return (
      <div className="home-page controller-store-page">
        {isLoading ? (
          <div className="home-empty-state controller-store-empty">
            <Loader2 className="home-spinner" size={54} />
            <p>{t("home.empty.loadingGames")}</p>
          </div>
        ) : controllerSections.length === 0 ? (
          <div className="home-empty-state controller-store-empty">
            <Gamepad2 className="home-empty-icon" size={64} />
            <h3>{t("home.controller.emptyTitle")}</h3>
            <p>{t("home.controller.emptyBody")}</p>
          </div>
        ) : (
          <>
            {heroGame && (
              <section className="controller-hero controller-store-hero" aria-label={heroGame.title}>
                {heroImageUrl ? <img src={heroImageUrl} alt="" className="controller-hero-image" /> : <div className="controller-hero-placeholder" />}
                <div className="controller-hero-scrim" />
                <div className="controller-hero-content">
                  {heroLogoUrl ? <img src={heroLogoUrl} alt={heroGame.title} className="controller-hero-logo" /> : <h1>{heroGame.title}</h1>}
                  <p className="controller-store-hero-meta">{getPrimaryStoreName(heroGame, heroSelectedVariantId)} / {getPrimaryGenre(heroGame)}</p>
                  <div className="controller-hero-actions">
                    <button type="button" className="controller-primary-action" onClick={() => onBuyGame?.(heroGame, heroSelectedVariantId)}>
                      {t("app.actions.buy")}
                    </button>
                    <span className="controller-store-hero-pill">{getPrimaryStoreName(heroGame, heroSelectedVariantId)}</span>
                  </div>
                </div>
              </section>
            )}

            {heroGame && (
              <div className="controller-hero-dots" aria-hidden="true">
                {Array.from({ length: heroDotCount }).map((_, index) => (
                  <span key={index} className={index === activeHeroDotIndex ? "active" : ""} />
                ))}
              </div>
            )}

            <div className="controller-store-sections">
              {controllerSections.map((section, rowIndex) => (
                <section key={`${section.id}-${rowIndex}`} className="controller-store-section">
                  <div className="controller-store-section-heading">
                    <span>{String(rowIndex + 1).padStart(2, "0")}</span>
                    <h2>{section.title || t("home.controller.featured")}</h2>
                    <p>{t("library.gameCount", { count: section.games.length })}</p>
                  </div>
                  <div
                    className="controller-store-row"
                    ref={(element) => { rowRefs.current[rowIndex] = element; }}
                    data-controller-store-row={rowIndex}
                  >
                    {section.games.slice(0, 18).map((game, columnIndex) => {
                      const focused = rowIndex === focusedRowIndex && columnIndex === focusedColumnIndex;
                      return (
                        <div key={game.id} className="controller-store-card" data-controller-store-column={columnIndex}>
                          <ControllerStoreTile
                            game={game}
                            selectedVariantId={selectedVariantByGameId[game.id]}
                            focused={focused}
                            onFocus={() => {
                              focusTile(rowIndex, columnIndex);
                              if (game.variants.length > 0) onSelectGameVariant(game.id, selectedVariantByGameId[game.id] ?? game.variants[game.selectedVariantIndex]?.id ?? game.variants[0].id);
                            }}
                            onBuy={() => onBuyGame?.(game, selectedVariantByGameId[game.id])}
                            onPlay={() => onPlayGame(game)}
                          />
                        </div>
                      );
                    })}
                  </div>
                </section>
              ))}
            </div>

            <div className="controller-bottom-hints" aria-hidden="true">
              <div className="controller-hint"><span className="controller-button controller-button--a">A</span><span>{t("app.actions.select")}</span></div>
              <div className="controller-hint"><span className="controller-button controller-button--b">B</span><span>{t("app.actions.back")}</span></div>
              <div className="controller-hint"><span className="controller-button controller-button--x">X</span><span>{t("app.actions.search")}</span></div>
              <div className="controller-hint controller-hint--more"><span className="controller-menu-button"><Menu size={22} /></span><span>{t("library.moreOptions")}</span></div>
            </div>
          </>
        )}
      </div>
    );
  }

  const hasGames = games.length > 0;
  const visibleFilterGroups = filterGroups.filter((group) => ["digital_store", "genre", "subscriptions"].includes(group.id));
  const activeFilterCount = selectedFilterIds.length;
  const countLabel = isLoading
    ? t("home.count.loading")
    : totalCount > games.length && supportedCount > 0
      ? t("home.count.shownTotalSupported", { shown: games.length, total: totalCount, supported: supportedCount })
      : totalCount > games.length
        ? t("home.count.shownTotal", { shown: games.length, total: totalCount })
        : supportedCount > 0
          ? t("home.count.shownSupported", { shown: games.length, supported: supportedCount })
          : t("home.count.shown", { shown: games.length });

  return (
    <div className="home-page">
      <header className="home-toolbar">
        <div className="home-search">
          <Search className="home-search-icon" size={16} />
          <input
            type="text"
            className="home-search-input"
            placeholder={t("home.searchPlaceholder")}
            value={searchQuery}
            onChange={(e) => onSearchChange(e.target.value)}
          />
        </div>

        {visibleFilterGroups.length > 0 && (
          <details className="home-filter-dropdown">
            <summary className="home-filter-dropdown-trigger">
              <span className="home-filter-dropdown-label">
                <Filter size={14} />
                {t("home.filters")}
              </span>
              {activeFilterCount > 0 && <span className="home-filter-dropdown-count">{activeFilterCount}</span>}
              <ChevronDown size={14} className="home-filter-dropdown-chevron" />
            </summary>
            <div className="home-filter-dropdown-menu">
              {visibleFilterGroups.map((group) => (
                <div key={group.id} className="home-filter-dropdown-group">
                  <div className="home-filter-group-label">{group.label}</div>
                  <div className="home-filter-chips">
                    {group.options.slice(0, group.id === "genre" ? 8 : group.options.length).map((option) => {
                      const active = selectedFilterIds.includes(option.id);
                      return (
                        <button
                          key={option.id}
                          type="button"
                          className={`home-filter-chip ${active ? "active" : ""}`}
                          onClick={() => onToggleFilter(option.id)}
                        >
                          {option.label}
                        </button>
                      );
                    })}
                  </div>
                </div>
              ))}
            </div>
          </details>
        )}

        <label className="home-sort">
          <ArrowUpDown size={14} />
          <select value={selectedSortId} onChange={(e) => onSortChange(e.target.value)} disabled={isLoading}>
            {sortOptions.map((option) => (
              <option key={option.id} value={option.id}>
                {option.label}
              </option>
            ))}
          </select>
        </label>

        <span className="home-count">
          {countLabel}
        </span>
      </header>

      <div className="home-grid-area">
        {isLoading ? (
          <div className="home-empty-state">
            <Loader2 className="home-spinner" size={36} />
            <p>{t("home.empty.loadingGames")}</p>
          </div>
        ) : !hasGames ? (
          <div className="home-empty-state">
            <LayoutGrid size={44} className="home-empty-icon" />
            <h3>{t("home.empty.noGamesFound")}</h3>
            <p>
              {searchQuery || selectedFilterIds.length > 0
                ? t("home.empty.tryAdjustingSearch")
                : t("home.empty.checkBackLater")}
            </p>
          </div>
        ) : (
          <div className="game-grid">
            {games.map((game) => (
              <GameCard
                key={game.id}
                game={game}
                isSelected={game.id === selectedGameId}
                onSelect={() => onSelectGame(game.id)}
                onPlay={() => onPlayGame(game)}
                selectedVariantId={selectedVariantByGameId[game.id]}
                onSelectStore={(variantId) => onSelectGameVariant(game.id, variantId)}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
