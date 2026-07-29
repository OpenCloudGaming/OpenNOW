import { Search, LayoutGrid, ArrowUpDown, Filter, ChevronDown } from "lucide-react";
import { memo, useEffect, useMemo, useRef, useState } from "react";
import type { JSX } from "react";
import type { CatalogFilterGroup, CatalogSortOption, GameInfo, GamePanelResult } from "@shared/gfn";
import { GameCardListItem, useCatalogCardActionsRef } from "./GameCardListItem";
import { gameNeedsPurchase, getNextVariantId } from "../lib/controllerCatalogUi";
import { clampRowFocus, moveRowFocus, type RowFocusDirection } from "../lib/consoleRowFocus";
import { getConsoleStoreChoices } from "../lib/consoleStoreChoices";
import { useTranslation } from "../i18n";
import { controllerButton } from "../utils/controllerGamepad";
import { useControllerFocusScroll } from "../hooks/useControllerFocusScroll";
import { useControllerKeyDown, useControllerNavigation } from "../hooks/useControllerNavigation";
import { ConsoleStoreView } from "./console/ConsoleStoreView";
import { SelectDropdown } from "./ui/SelectDropdown";
import { MotionSpinner } from "./MotionSpinner";

/** Cap per shelf so a curated panel cannot produce an unbounded row. */
const CONTROLLER_STORE_ROW_LIMIT = 18;

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
  surfaceActive?: boolean;
  storePanels?: GamePanelResult[];
  activeSessionAppIds?: number[];
  onBuyGame?: (game: GameInfo, selectedVariantId?: string) => void;
  onMarkGameOwned?: (game: GameInfo, selectedVariantId?: string) => void;
  markOwnedInFlightByVariantId?: Record<string, boolean>;
  onPreviousControllerPage?: () => void;
  onNextControllerPage?: () => void;
}


export const HomePage = memo(function HomePage({
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
  surfaceActive = true,
  storePanels = [],
  activeSessionAppIds: _activeSessionAppIds = [],
  onBuyGame,
  onMarkGameOwned,
  markOwnedInFlightByVariantId = {},
  onPreviousControllerPage,
  onNextControllerPage,
}: HomePageProps): JSX.Element {
  const { t } = useTranslation();
  const catalogActionsRef = useCatalogCardActionsRef({
    onPlayGame,
    onSelectGame,
    onSelectGameVariant,
  });
  const [focusedRowIndex, setFocusedRowIndex] = useState(0);
  const [focusedColumnIndex, setFocusedColumnIndex] = useState(0);
  const [controllerSearchOpen, setControllerSearchOpen] = useState(false);
  const [detailsGame, setDetailsGame] = useState<GameInfo | null>(null);
  const [detailsActionIndex, setDetailsActionIndex] = useState(0);
  const [storePickerOpen, setStorePickerOpen] = useState(false);
  const [storePickerIndex, setStorePickerIndex] = useState(0);
  const rowRefs = useRef<Array<HTMLDivElement | null>>([]);
  const controllerSearchInputRef = useRef<HTMLInputElement | null>(null);
  const controllerSurfaceActive = controllerMode && surfaceActive;
  const scrollFocusIntoView = useControllerFocusScroll(controllerSurfaceActive);

  const controllerSections = useMemo(
    () => storePanels.flatMap((panel) => panel.sections).filter((section) => section.games.length > 0),
    [storePanels],
  );
  const controllerRowLengths = useMemo(
    () => controllerSections.map((section) => Math.min(section.games.length, CONTROLLER_STORE_ROW_LIMIT)),
    [controllerSections],
  );

  const focusTile = (rowIndex: number, columnIndex: number): void => {
    if (!surfaceActive || controllerRowLengths.length === 0) return;
    const next = clampRowFocus(controllerRowLengths, { rowIndex, columnIndex });
    const nextGame = controllerSections[next.rowIndex]?.games[next.columnIndex];
    if (!nextGame) return;
    setFocusedRowIndex(next.rowIndex);
    setFocusedColumnIndex(next.columnIndex);
    onSelectGame(nextGame.id);
    // Scroll the card horizontally into its track first, then let the hook bring
    // the whole row into view — the row must win the vertical scroll.
    scrollFocusIntoView(() => {
      const card = rowRefs.current[next.rowIndex]?.querySelector<HTMLElement>(`[data-console-column="${next.columnIndex}"]`);
      card?.scrollIntoView({ inline: "nearest", block: "nearest", behavior: "auto" });
      return card?.closest<HTMLElement>(".console-row");
    });
  };

  const moveFocus = (direction: RowFocusDirection): void => {
    const next = moveRowFocus(controllerRowLengths, { rowIndex: focusedRowIndex, columnIndex: focusedColumnIndex }, direction);
    focusTile(next.rowIndex, next.columnIndex);
  };

  const launchGame = (game: GameInfo): void => {
    const selectedVariantId = selectedVariantByGameId[game.id];
    if (gameNeedsPurchase(game, selectedVariantId)) {
      (onMarkGameOwned ?? onBuyGame)?.(game, selectedVariantId);
      return;
    }
    onPlayGame(game);
  };

  const focusedStoreGame = (): GameInfo | undefined =>
    controllerSections[focusedRowIndex]?.games[focusedColumnIndex];

  const openDetails = (game: GameInfo): void => {
    setDetailsGame(game);
    setDetailsActionIndex(0);
    setStorePickerOpen(false);
  };

  const closeDetails = (): void => {
    setDetailsGame(null);
    setStorePickerOpen(false);
  };

  const storeChoicesFor = (game: GameInfo) => getConsoleStoreChoices(game, selectedVariantByGameId[game.id]);

  /** Number of buttons the detail sheet renders, mirrored from ConsoleStoreView. */
  const detailsActionCount = (game: GameInfo): number => (storeChoicesFor(game).length > 1 ? 3 : 2);

  const openStorePicker = (): void => {
    if (!detailsGame) return;
    const choices = storeChoicesFor(detailsGame);
    setStorePickerIndex(Math.max(0, choices.findIndex((choice) => choice.isActive)));
    setStorePickerOpen(true);
  };

  const selectStoreChoice = (variantId: string): void => {
    if (detailsGame) onSelectGameVariant(detailsGame.id, variantId);
    setStorePickerOpen(false);
  };

  const activateDetailsAction = (): void => {
    if (!detailsGame) return;
    if (detailsActionIndex === 0) {
      launchGame(detailsGame);
      return;
    }
    if (storeChoicesFor(detailsGame).length > 1 && detailsActionIndex === 1) {
      openStorePicker();
      return;
    }
    closeDetails();
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
    if (!controllerMode || !surfaceActive || !controllerSearchOpen) return;
    controllerSearchInputRef.current?.focus();
  }, [controllerMode, controllerSearchOpen, surfaceActive]);

  /**
   * Keeps the shared selection pinned to whatever card is focused on THIS
   * page. selectedGameId is app-level state shared with the library, so on
   * arriving here it still points at the other page's game; syncing focus ->
   * selection (rather than the reverse) is what makes the billboard, the
   * focus ring and the selection agree.
   */
  useEffect(() => {
    if (!controllerMode || !surfaceActive || controllerRowLengths.length === 0) return;
    const next = clampRowFocus(controllerRowLengths, { rowIndex: focusedRowIndex, columnIndex: focusedColumnIndex });
    const focusedGame = controllerSections[next.rowIndex]?.games[next.columnIndex];
    if (!focusedGame) return;
    if (next.rowIndex !== focusedRowIndex) setFocusedRowIndex(next.rowIndex);
    if (next.columnIndex !== focusedColumnIndex) setFocusedColumnIndex(next.columnIndex);
    if (focusedGame.id !== selectedGameId) onSelectGame(focusedGame.id);
  }, [controllerMode, controllerRowLengths, controllerSections, focusedColumnIndex, focusedRowIndex, onSelectGame, selectedGameId, surfaceActive]);

  useControllerKeyDown(controllerSurfaceActive, (event) => {
    if (detailsGame && storePickerOpen) {
      const choices = storeChoicesFor(detailsGame);
      if (event.key === "Escape" || event.key.toLowerCase() === "b") {
        event.preventDefault();
        setStorePickerOpen(false);
      } else if (event.key === "ArrowUp") {
        event.preventDefault();
        setStorePickerIndex((index) => Math.max(0, index - 1));
      } else if (event.key === "ArrowDown") {
        event.preventDefault();
        setStorePickerIndex((index) => Math.min(choices.length - 1, index + 1));
      } else if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        const choice = choices[storePickerIndex];
        if (choice) selectStoreChoice(choice.variantId);
      }
      return;
    }
    if (detailsGame) {
      if (event.key === "Escape" || event.key.toLowerCase() === "b") {
        event.preventDefault();
        closeDetails();
      } else if (event.key === "ArrowLeft") {
        event.preventDefault();
        setDetailsActionIndex((index) => Math.max(0, index - 1));
      } else if (event.key === "ArrowRight") {
        event.preventDefault();
        setDetailsActionIndex((index) => Math.min(detailsActionCount(detailsGame) - 1, index + 1));
      } else if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        activateDetailsAction();
      }
      return;
    }
    if (controllerSearchOpen) {
      if (event.key === "Escape") {
        event.preventDefault();
        setControllerSearchOpen(false);
      }
      return;
    }
    if (event.key === "ArrowLeft") {
      event.preventDefault();
      moveFocus("left");
    } else if (event.key === "ArrowRight") {
      event.preventDefault();
      moveFocus("right");
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      moveFocus("up");
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      moveFocus("down");
    } else if (event.key.toLowerCase() === "x") {
      event.preventDefault();
      setControllerSearchOpen(true);
    } else if (event.key === "Escape") {
      event.preventDefault();
      onPreviousControllerPage?.();
    } else if (event.key.toLowerCase() === "b") {
      event.preventDefault();
      onPreviousControllerPage?.();
    } else if (event.key === "[") {
      event.preventDefault();
      onPreviousControllerPage?.();
    } else if (event.key === "]") {
      event.preventDefault();
      onNextControllerPage?.();
    } else if (event.key.toLowerCase() === "m" || event.key.toLowerCase() === "y") {
      event.preventDefault();
      cycleFocusedVariant();
    } else if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      const game = focusedStoreGame();
      if (game) openDetails(game);
    }
  });

  useControllerNavigation({
    enabled: controllerSurfaceActive,
    onFrame: ({ pressed }) => {
      if (detailsGame && storePickerOpen) {
        const choices = storeChoicesFor(detailsGame);
        if (pressed & controllerButton.east) setStorePickerOpen(false);
        if (pressed & controllerButton.up) setStorePickerIndex((index) => Math.max(0, index - 1));
        if (pressed & controllerButton.down) setStorePickerIndex((index) => Math.min(choices.length - 1, index + 1));
        if (pressed & controllerButton.south) {
          const choice = choices[storePickerIndex];
          if (choice) selectStoreChoice(choice.variantId);
        }
        return;
      }

      if (detailsGame) {
        if (pressed & controllerButton.east) closeDetails();
        if (pressed & controllerButton.left) setDetailsActionIndex((index) => Math.max(0, index - 1));
        if (pressed & controllerButton.right) {
          setDetailsActionIndex((index) => Math.min(detailsActionCount(detailsGame) - 1, index + 1));
        }
        if (pressed & controllerButton.south) activateDetailsAction();
        return;
      }

      if (controllerSearchOpen) {
        if (pressed & controllerButton.east) setControllerSearchOpen(false);
        return;
      }

      // A opens the detail sheet; launching happens from there, so a stray
      // press can never start a session or mark a game owned by accident.
      if (pressed & controllerButton.south) {
        const game = focusedStoreGame();
        if (game) openDetails(game);
      }
      if (pressed & controllerButton.east) onPreviousControllerPage?.();
      if (pressed & controllerButton.west) setControllerSearchOpen(true);
      if (pressed & controllerButton.leftShoulder) onPreviousControllerPage?.();
      if (pressed & controllerButton.rightShoulder) onNextControllerPage?.();
      if (pressed & controllerButton.menu) cycleFocusedVariant();
      if (pressed & controllerButton.up) moveFocus("up");
      if (pressed & controllerButton.down) moveFocus("down");
      if (pressed & controllerButton.left) moveFocus("left");
      if (pressed & controllerButton.right) moveFocus("right");
    },
  });

  const gameGridItems = useMemo(
    () => games.map((game) => (
      <GameCardListItem
        key={game.id}
        game={game}
        isSelected={game.id === selectedGameId}
        selectedVariantId={selectedVariantByGameId[game.id]}
        surface="home"
        actionsRef={catalogActionsRef}
      />
    )),
    [catalogActionsRef, games, selectedGameId, selectedVariantByGameId],
  );

  if (controllerMode) {
    return (
      <ConsoleStoreView
        isLoading={isLoading}
        sections={controllerSections.map((section, rowIndex) => ({
          id: `${section.id}-${rowIndex}`,
          title: section.title,
          games: section.games.slice(0, CONTROLLER_STORE_ROW_LIMIT),
        }))}
        rowRefs={rowRefs}
        focusedRowIndex={focusedRowIndex}
        focusedColumnIndex={focusedColumnIndex}
        onFocusCard={focusTile}
        onActivateCard={launchGame}
        detailsGame={detailsGame}
        detailsActionIndex={detailsActionIndex}
        onFocusDetailsAction={setDetailsActionIndex}
        onOpenDetails={openDetails}
        onCloseDetails={closeDetails}
        storePickerOpen={storePickerOpen}
        storePickerIndex={storePickerIndex}
        onFocusStoreChoice={setStorePickerIndex}
        onSelectStoreChoice={selectStoreChoice}
        onOpenStorePicker={openStorePicker}
        onCloseStorePicker={() => setStorePickerOpen(false)}
        // Always the focused card. A separate featured carousel meant the
        // billboard and the focus ring disagreed, and it showed the same
        // NVIDIA featured list on both pages.
        heroGame={focusedStoreGame() ?? controllerSections[0]?.games[0]}
        selectedVariantByGameId={selectedVariantByGameId}
        markOwnedInFlightByVariantId={markOwnedInFlightByVariantId}
        onHeroPrimaryAction={launchGame}
        onCycleVariant={() => { cycleFocusedVariant(); }}
        onBack={() => onPreviousControllerPage?.()}
        searchQuery={searchQuery}
        onSearchChange={onSearchChange}
        searchOpen={controllerSearchOpen}
        searchInputRef={controllerSearchInputRef}
        onOpenSearch={() => setControllerSearchOpen(true)}
        onCloseSearch={() => setControllerSearchOpen(false)}
      />
    );
  }

  const hasGames = games.length > 0;
  const showInitialLoading = isLoading && !hasGames;
  const visibleFilterGroups = filterGroups.filter((group) => ["digital_store", "genre", "subscriptions"].includes(group.id));
  const activeFilterCount = selectedFilterIds.length;
  const countLabel = showInitialLoading
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

        {sortOptions.length > 0 && (
          <div className="home-sort">
            <ArrowUpDown size={14} />
            <SelectDropdown
              value={selectedSortId}
              options={sortOptions.map((option) => ({ value: option.id, label: option.label }))}
              onChange={onSortChange}
              disabled={showInitialLoading}
              ariaLabel={t("home.sortAriaLabel")}
            />
          </div>
        )}

        <span className="home-count">
          {countLabel}
        </span>
      </header>

      <div className="home-grid-area">
        {showInitialLoading ? (
          <div className="home-empty-state">
            <MotionSpinner className="home-spinner" size={36} label={t("common.loading")} />
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
            {gameGridItems}
          </div>
        )}
      </div>
    </div>
  );
});
