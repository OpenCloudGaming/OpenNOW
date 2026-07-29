import { Library, Search, Clock, Gamepad2, ArrowUpDown, Filter, ChevronDown, X } from "lucide-react";
import { memo, useEffect, useMemo, useRef, useState } from "react";
import type { JSX } from "react";
import { AnimatePresence, m } from "motion/react";
import type { CatalogSortOption, GameInfo } from "@shared/gfn";
import { GameCardListItem, useCatalogCardActionsRef } from "./GameCardListItem";
import type { PlaytimeData } from "../lib/gameCatalog";
import { buildConsoleLibraryRows } from "../lib/consoleLibraryRows";
import { clampRowFocus, moveRowFocus, type RowFocusDirection } from "../lib/consoleRowFocus";
import { getConsoleStoreChoices } from "../lib/consoleStoreChoices";
import {
  gameMatchesLibraryFilters,
  getControllerStoreFilterItems,
  getLibraryFilterGroups,
  getLibraryFilterOptionById,
  type LibraryFilterOption,
} from "../lib/libraryFilters";
import { useTranslation } from "../i18n";
import { formatCatalogLastPlayed } from "../utils/lastPlayedFormat";
import { controllerButton } from "../utils/controllerGamepad";
import { wasReleasedAsTap } from "../lib/controllerInputState";
import { useControllerFocusScroll } from "../hooks/useControllerFocusScroll";
import { useControllerKeyDown, useControllerNavigation } from "../hooks/useControllerNavigation";
import { pageTransition } from "./MotionProvider";
import { SelectDropdown } from "./ui/SelectDropdown";
import { LibraryControllerView } from "./library/LibraryControllerView";
import { MotionSpinner } from "./MotionSpinner";


export interface LibraryPageProps {
  games: GameInfo[];
  allGames: GameInfo[];
  playtimeData: PlaytimeData;
  searchQuery: string;
  onSearchChange: (query: string) => void;
  onPlayGame: (game: GameInfo) => void;
  onBuyGame?: (game: GameInfo, selectedVariantId?: string) => void;
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
  surfaceActive?: boolean;
  activeSessionAppIds?: number[];
  onPreviousControllerPage?: () => void;
  onNextControllerPage?: () => void;
}

export const LibraryPage = memo(function LibraryPage({
  games,
  allGames,
  playtimeData,
  searchQuery,
  onSearchChange,
  onPlayGame,
  onBuyGame,
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
  surfaceActive = true,
  activeSessionAppIds = [],
  onPreviousControllerPage,
  onNextControllerPage,
}: LibraryPageProps): JSX.Element {
  const { t } = useTranslation();
  const catalogActionsRef = useCatalogCardActionsRef({
    onPlayGame,
    onSelectGame,
    onSelectGameVariant,
  });
  const [detailsGame, setDetailsGame] = useState<GameInfo | null>(null);
  const [detailsActionIndex, setDetailsActionIndex] = useState(0);
  const [storePickerOpen, setStorePickerOpen] = useState(false);
  const [storePickerIndex, setStorePickerIndex] = useState(0);
  const [controllerStoreFilterId, setControllerStoreFilterId] = useState("library");
  const [controllerStoreFilterOpen, setControllerStoreFilterOpen] = useState(false);
  const [controllerSearchOpen, setControllerSearchOpen] = useState(false);
  const [focusedControllerStoreFilterIndex, setFocusedControllerStoreFilterIndex] = useState(0);
  const [selectedLibraryFilterIds, setSelectedLibraryFilterIds] = useState<string[]>([]);
  const [focusedRowIndex, setFocusedRowIndex] = useState(0);
  const [focusedColumnIndex, setFocusedColumnIndex] = useState(0);
  const controllerSearchInputRef = useRef<HTMLInputElement | null>(null);
  const controllerRowRefs = useRef<Array<HTMLDivElement | null>>([]);
  const controllerSurfaceActive = controllerMode && surfaceActive;
  const scrollFocusIntoView = useControllerFocusScroll(controllerSurfaceActive);

  useEffect(() => {
    if (!controllerMode || !surfaceActive || !controllerSearchOpen) return;
    controllerSearchInputRef.current?.focus();
  }, [controllerMode, controllerSearchOpen, surfaceActive]);

  const librarySearchHasQuery = searchQuery.trim().length > 0;
  const libraryFilterGroups = useMemo(
    () => getLibraryFilterGroups(allGames, playtimeData, t),
    [allGames, playtimeData, t],
  );
  const visibleLibraryGames = useMemo(
    () => games.filter((game) => gameMatchesLibraryFilters(game, selectedLibraryFilterIds, playtimeData, t)),
    [games, playtimeData, selectedLibraryFilterIds, t],
  );
  const activeLibraryFilterOptions = useMemo(
    () => selectedLibraryFilterIds
      .map((filterId) => getLibraryFilterOptionById(libraryFilterGroups, filterId))
      .filter((option): option is LibraryFilterOption => Boolean(option)),
    [libraryFilterGroups, selectedLibraryFilterIds],
  );
  const hasActiveLibraryFilters = activeLibraryFilterOptions.length > 0;
  const libraryCountLabel = hasActiveLibraryFilters || librarySearchHasQuery
    ? t("library.filteredGameCount", { shown: visibleLibraryGames.length, total: libraryCount, count: libraryCount })
    : t("library.gameCount", { count: libraryCount });

  useEffect(() => {
    const availableFilterIds = new Set(libraryFilterGroups.flatMap((group) => group.options.map((option) => option.id)));
    setSelectedLibraryFilterIds((previous) => {
      const next = previous.filter((filterId) => availableFilterIds.has(filterId));
      return next.length === previous.length ? previous : next;
    });
  }, [libraryFilterGroups]);

  useEffect(() => {
    if (!surfaceActive || controllerMode || visibleLibraryGames.length === 0) return;
    if (visibleLibraryGames.some((game) => game.id === selectedGameId)) return;
    onSelectGame(visibleLibraryGames[0].id);
  }, [controllerMode, onSelectGame, selectedGameId, surfaceActive, visibleLibraryGames]);

  const toggleLibraryFilter = (filterId: string): void => {
    setSelectedLibraryFilterIds((previous) => (
      previous.includes(filterId)
        ? previous.filter((selectedFilterId) => selectedFilterId !== filterId)
        : [...previous, filterId]
    ));
  };

  const clearLibraryFilters = (): void => {
    setSelectedLibraryFilterIds([]);
  };

  const controllerStoreFilterItems = useMemo(
    () => getControllerStoreFilterItems(games, t("library.allStores")),
    [games, t],
  );

  useEffect(() => {
    if (controllerStoreFilterItems.some((item) => item.id === controllerStoreFilterId)) return;
    setControllerStoreFilterId("library");
    setFocusedControllerStoreFilterIndex(0);
  }, [controllerStoreFilterId, controllerStoreFilterItems]);

  const controllerRows = useMemo(
    () => buildConsoleLibraryRows({ games, playtimeData, storeFilterId: controllerStoreFilterId, t }),
    [controllerStoreFilterId, games, playtimeData, t],
  );
  const controllerRowLengths = useMemo(() => controllerRows.map((row) => row.games.length), [controllerRows]);

  /**
   * Keeps the shared selection pinned to whatever card is focused on THIS
   * page. selectedGameId is app-level state shared with the store, so on
   * arriving here it still points at the other page's game; syncing focus ->
   * selection (rather than the reverse) is what makes the billboard, the
   * focus ring and the selection agree.
   */
  useEffect(() => {
    if (!controllerMode || !surfaceActive || controllerRowLengths.length === 0) return;
    const next = clampRowFocus(controllerRowLengths, { rowIndex: focusedRowIndex, columnIndex: focusedColumnIndex });
    const focusedGame = controllerRows[next.rowIndex]?.games[next.columnIndex];
    if (!focusedGame) return;
    if (next.rowIndex !== focusedRowIndex) setFocusedRowIndex(next.rowIndex);
    if (next.columnIndex !== focusedColumnIndex) setFocusedColumnIndex(next.columnIndex);
    if (focusedGame.id !== selectedGameId) onSelectGame(focusedGame.id);
  }, [controllerMode, controllerRowLengths, controllerRows, focusedColumnIndex, focusedRowIndex, onSelectGame, selectedGameId, surfaceActive]);
  const selectedControllerGame = controllerRows[focusedRowIndex]?.games[focusedColumnIndex]
    ?? controllerRows[0]?.games[0];

  const focusControllerCard = (rowIndex: number, columnIndex: number): void => {
    if (!surfaceActive || controllerRowLengths.length === 0) return;
    const next = clampRowFocus(controllerRowLengths, { rowIndex, columnIndex });
    const nextGame = controllerRows[next.rowIndex]?.games[next.columnIndex];
    if (!nextGame) return;
    setFocusedRowIndex(next.rowIndex);
    setFocusedColumnIndex(next.columnIndex);
    onSelectGame(nextGame.id);
    // Scroll the card horizontally into its track first, then let the hook bring
    // the whole row into view — the row must win the vertical scroll.
    scrollFocusIntoView(() => {
      const card = controllerRowRefs.current[next.rowIndex]?.querySelector<HTMLElement>(
        `[data-console-column="${next.columnIndex}"]`,
      );
      card?.scrollIntoView({ inline: "nearest", block: "nearest", behavior: "auto" });
      return card?.closest<HTMLElement>(".console-row");
    });
  };

  const moveControllerFocus = (direction: RowFocusDirection): void => {
    const next = moveRowFocus(controllerRowLengths, { rowIndex: focusedRowIndex, columnIndex: focusedColumnIndex }, direction);
    focusControllerCard(next.rowIndex, next.columnIndex);
  };

  const cycleGameVariant = (game: GameInfo | undefined): void => {
    if (!game || game.variants.length <= 1) return;
    const activeVariantId = selectedVariantByGameId[game.id];
    const activeIndex = Math.max(0, game.variants.findIndex((variant) => variant.id === activeVariantId));
    const nextVariant = game.variants[(activeIndex + 1) % game.variants.length];
    if (nextVariant) onSelectGameVariant(game.id, nextVariant.id);
  };

  const cycleSelectedVariant = (): void => {
    cycleGameVariant(selectedControllerGame);
  };

  const cycleControllerStoreFilter = (): void => {
    if (controllerStoreFilterItems.length <= 1) return;
    const activeIndex = Math.max(0, controllerStoreFilterItems.findIndex((item) => item.id === controllerStoreFilterId));
    const nextItem = controllerStoreFilterItems[(activeIndex + 1) % controllerStoreFilterItems.length];
    setControllerStoreFilterId(nextItem.id);
    setFocusedControllerStoreFilterIndex((activeIndex + 1) % controllerStoreFilterItems.length);
  };

  const showControllerStoreFilterOverlay = (): void => {
    const activeIndex = Math.max(0, controllerStoreFilterItems.findIndex((item) => item.id === controllerStoreFilterId));
    setFocusedControllerStoreFilterIndex(activeIndex);
    setControllerStoreFilterOpen(true);
  };

  const moveControllerStoreFilterFocusBy = (delta: number): void => {
    if (controllerStoreFilterItems.length === 0) return;
    setFocusedControllerStoreFilterIndex((index) => Math.max(0, Math.min(index + delta, controllerStoreFilterItems.length - 1)));
  };

  const hideControllerStoreFilterOverlay = (applySelection: boolean): void => {
    if (applySelection) {
      const item = controllerStoreFilterItems[focusedControllerStoreFilterIndex] ?? controllerStoreFilterItems[0];
      if (item) {
        setControllerStoreFilterId(item.id);
          }
    }
    setControllerStoreFilterOpen(false);
  };

  const storeChoicesFor = (game: GameInfo) => getConsoleStoreChoices(game, selectedVariantByGameId[game.id]);
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
      onPlayGame(detailsGame);
      return;
    }
    if (storeChoicesFor(detailsGame).length > 1 && detailsActionIndex === 1) {
      openStorePicker();
      return;
    }
    setDetailsGame(null);
  };

  const openDetails = (game: GameInfo | undefined): void => {
    if (!game) return;
    setDetailsGame(game);
    setDetailsActionIndex(0);
    setStorePickerOpen(false);
  };

  const closeDetails = (): void => {
    setDetailsGame(null);
    setStorePickerOpen(false);
  };

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
      moveControllerFocus("left");
    } else if (event.key === "ArrowRight") {
      event.preventDefault();
      moveControllerFocus("right");
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      moveControllerFocus("up");
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      moveControllerFocus("down");
    } else if (event.key.toLowerCase() === "v") {
      event.preventDefault();
      cycleSelectedVariant();
    } else if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      openDetails(selectedControllerGame);
    } else if (event.key.toLowerCase() === "x") {
      event.preventDefault();
      setControllerSearchOpen(true);
    } else if (event.key.toLowerCase() === "b" || event.key === "Escape") {
      event.preventDefault();
      onPreviousControllerPage?.();
    } else if (event.key === "[") {
      event.preventDefault();
      onPreviousControllerPage?.();
    } else if (event.key === "]") {
      event.preventDefault();
      onNextControllerPage?.();
    } else if (event.key.toLowerCase() === "i" || event.key.toLowerCase() === "m") {
      event.preventDefault();
      if (selectedControllerGame) setDetailsGame(selectedControllerGame);
    }
  });

  useControllerNavigation({
    enabled: controllerSurfaceActive,
    holdMask: controllerButton.north,
    onFrame: (frame) => {
      const { pressed, released } = frame;
      const yButton = controllerButton.north;

      // Hold Y opens the store filter overlay; a tap cycles it. The hold fires
      // before the overlay branches below so it can open from any state.
      if (frame.holdFired) showControllerStoreFilterOverlay();

      if (controllerSearchOpen) {
        if (pressed & controllerButton.east) setControllerSearchOpen(false);
        return;
      }

      if (controllerStoreFilterOpen) {
        if (pressed & controllerButton.up) moveControllerStoreFilterFocusBy(-1);
        if (pressed & controllerButton.down) moveControllerStoreFilterFocusBy(1);
        if (pressed & controllerButton.east) hideControllerStoreFilterOverlay(false);
        if (released & yButton) hideControllerStoreFilterOverlay(true);
        return;
      }

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
        if (pressed & controllerButton.south) activateDetailsAction();
        if (pressed & controllerButton.east) closeDetails();
        if (pressed & controllerButton.left) setDetailsActionIndex((index) => Math.max(0, index - 1));
        if (pressed & controllerButton.right) {
          setDetailsActionIndex((index) => Math.min(detailsActionCount(detailsGame) - 1, index + 1));
        }
        return;
      }

      if (wasReleasedAsTap(frame, yButton)) cycleControllerStoreFilter();
      // A opens the detail sheet; playing happens from there.
      if (pressed & controllerButton.south) openDetails(selectedControllerGame);
      if (pressed & controllerButton.east) onPreviousControllerPage?.();
      if (pressed & controllerButton.west) setControllerSearchOpen(true);
      if (pressed & controllerButton.leftShoulder) onPreviousControllerPage?.();
      if (pressed & controllerButton.rightShoulder) onNextControllerPage?.();
      if ((pressed & controllerButton.menu) && selectedControllerGame) setDetailsGame(selectedControllerGame);
      if (pressed & controllerButton.up) moveControllerFocus("up");
      if (pressed & controllerButton.down) moveControllerFocus("down");
      if (pressed & controllerButton.left) moveControllerFocus("left");
      if (pressed & controllerButton.right) moveControllerFocus("right");
    },
  });

  const libraryGridItems = useMemo(
    () => visibleLibraryGames.map((game) => (
      <div key={game.id} className="library-game-wrapper">
        <GameCardListItem
          game={game}
          isSelected={game.id === selectedGameId}
          selectedVariantId={selectedVariantByGameId[game.id]}
          surface="library"
          actionsRef={catalogActionsRef}
        />
        <div
          className={`library-last-played${game.lastPlayed ? "" : " library-last-played--empty"}`}
          aria-hidden={game.lastPlayed ? undefined : true}
        >
          <Clock size={12} />
          <span>{game.lastPlayed ? formatCatalogLastPlayed(t, game.lastPlayed) : "—"}</span>
        </div>
      </div>
    )),
    [catalogActionsRef, selectedGameId, selectedVariantByGameId, t, visibleLibraryGames],
  );

  if (controllerMode) {
    // Always the focused card. A separate featured carousel meant the billboard
    // and the focus ring disagreed, and it headlined a store game that was not
    // even in this library.
    const heroGame = selectedControllerGame;
    return (
      <LibraryControllerView
        isLoading={isLoading}
        libraryCount={libraryCount}
        searchQuery={searchQuery}
        onSearchChange={onSearchChange}
        onPlayGame={onPlayGame}
        onBuyGame={onBuyGame}
        selectedVariantByGameId={selectedVariantByGameId}
        activeSessionAppIds={activeSessionAppIds}
        rows={controllerRows}
        rowRefs={controllerRowRefs}
        focusedRowIndex={focusedRowIndex}
        focusedColumnIndex={focusedColumnIndex}
        onFocusCard={focusControllerCard}
        heroGame={heroGame}
        controllerStoreFilterOpen={controllerStoreFilterOpen}
        controllerStoreFilterItems={controllerStoreFilterItems}
        controllerStoreFilterId={controllerStoreFilterId}
        focusedControllerStoreFilterIndex={focusedControllerStoreFilterIndex}
        onFocusControllerStoreFilter={setFocusedControllerStoreFilterIndex}
        onSelectControllerStoreFilter={(itemId) => {
          setControllerStoreFilterId(itemId);
          setControllerStoreFilterOpen(false);
        }}
        controllerSearchOpen={controllerSearchOpen}
        controllerSearchInputRef={controllerSearchInputRef}
        detailsGame={detailsGame}
        detailsActionIndex={detailsActionIndex}
        onFocusDetailsAction={setDetailsActionIndex}
        onCloseDetails={closeDetails}
        storePickerOpen={storePickerOpen}
        storePickerIndex={storePickerIndex}
        onFocusStoreChoice={setStorePickerIndex}
        onSelectStoreChoice={selectStoreChoice}
        onOpenStorePicker={openStorePicker}
        onCloseStorePicker={() => setStorePickerOpen(false)}
        onCycleGameVariant={cycleGameVariant}
        onSelectHint={() => openDetails(selectedControllerGame)}
        onBackHint={() => onPreviousControllerPage?.()}
        onFilterHint={showControllerStoreFilterOverlay}
        onSearchHint={() => setControllerSearchOpen(true)}
        onMoreOptionsHint={() => {
          if (selectedControllerGame) setDetailsGame(selectedControllerGame);
        }}
        onCloseSearchHint={() => setControllerSearchOpen(false)}
      />
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

        {libraryFilterGroups.length > 0 && (
          <details className="library-filter-dropdown">
            <summary className="library-filter-dropdown-trigger">
              <span className="library-filter-dropdown-label">
                <Filter size={14} />
                {t("library.filters")}
              </span>
              {selectedLibraryFilterIds.length > 0 && <span className="library-filter-dropdown-count">{selectedLibraryFilterIds.length}</span>}
              <ChevronDown size={14} className="library-filter-dropdown-chevron" />
            </summary>
            <div className="library-filter-dropdown-menu">
              {libraryFilterGroups.map((group) => (
                <div key={group.id} className="library-filter-dropdown-group">
                  <div className="library-filter-group-label">{group.label}</div>
                  <div className="library-filter-chips">
                    {group.options.map((option) => {
                      const active = selectedLibraryFilterIds.includes(option.id);
                      return (
                        <button
                          key={option.id}
                          type="button"
                          className={`library-filter-chip ${active ? "active" : ""}`}
                          onClick={() => toggleLibraryFilter(option.id)}
                          aria-pressed={active}
                        >
                          <span>{option.label}</span>
                          <span className="library-filter-chip-count">{option.count}</span>
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
          <div className="library-sort">
            <ArrowUpDown size={14} />
            <SelectDropdown
              value={selectedSortId}
              options={sortOptions.map((option) => ({ value: option.id, label: option.label }))}
              onChange={onSortChange}
              ariaLabel={t("library.sortAriaLabel")}
            />
          </div>
        )}

        <span className="library-count">{libraryCountLabel}</span>
      </header>

      <AnimatePresence initial={false}>
        {activeLibraryFilterOptions.length > 0 && (
          <m.div
            className="library-active-filters"
            initial={{ opacity: 0, y: -6 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -6 }}
            transition={pageTransition}
          >
            <span className="library-active-filter-label">{t("library.activeFilters")}</span>
            {activeLibraryFilterOptions.map((option) => (
              <button
                key={option.id}
                type="button"
                className="library-active-filter-chip"
                onClick={() => toggleLibraryFilter(option.id)}
                aria-label={t("library.removeFilter", { filter: option.label })}
              >
                <span>{option.label}</span>
                <X size={12} />
              </button>
            ))}
            <button type="button" className="library-clear-filters" onClick={clearLibraryFilters}>
              {t("library.clearFilters")}
            </button>
          </m.div>
        )}
      </AnimatePresence>

      <div className="library-grid-area">
        {isLoading ? (
          <div className="library-empty-state">
            <MotionSpinner className="library-spinner" size={36} label={t("common.loading")} />
            <p>{t("library.empty.loadingLibrary")}</p>
          </div>
        ) : libraryCount === 0 ? (
          <div className="library-empty-state">
            <Gamepad2 className="library-empty-icon" size={44} />
            <h3>{t("library.empty.libraryEmpty")}</h3>
            <p>{t("library.empty.ownedGamesAppearHere")}</p>
          </div>
        ) : visibleLibraryGames.length === 0 ? (
          <div className="library-empty-state">
            <Search className="library-empty-icon" size={44} />
            <h3>{hasActiveLibraryFilters && !librarySearchHasQuery ? t("library.empty.noFilteredGames") : t("library.empty.noGamesFound")}</h3>
            <p>
              {librarySearchHasQuery
                ? t("library.empty.noGamesMatch", { query: searchQuery })
                : hasActiveLibraryFilters
                  ? t("library.empty.tryAdjustingFilters")
                : t("library.empty.noGamesMatch", { query: searchQuery })}
            </p>
          </div>
        ) : (
          <div className="game-grid">
            {libraryGridItems}
          </div>
        )}
      </div>
    </div>
  );
});
