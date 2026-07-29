import { Search, Gamepad2 } from "lucide-react";
import type { JSX, RefObject } from "react";
import type { GameInfo } from "@shared/gfn";
import {
  gameMatchesActiveSession,
  getSelectedVariantStoreLabel,
} from "../../lib/controllerCatalogUi";
import type { ConsoleLibraryRow } from "../../lib/consoleLibraryRows";
import type { ControllerStoreFilterItem } from "../../lib/libraryFilters";
import { useTranslation } from "../../i18n";
import { ConsoleBillboard } from "../console/ConsoleBillboard";
import { ConsoleGameDetails } from "../console/ConsoleGameDetails";
import { ConsoleHintBar } from "../console/ConsoleHintBar";
import { ConsoleOverlay } from "../console/ConsoleOverlay";
import { ConsoleRow } from "../console/ConsoleRow";
import { ConsoleStorePicker } from "../console/ConsoleStorePicker";
import { getConsoleStoreChoices } from "../../lib/consoleStoreChoices";

export interface LibraryControllerViewProps {
  isLoading: boolean;
  libraryCount: number;
  searchQuery: string;
  onSearchChange: (query: string) => void;
  onPlayGame: (game: GameInfo) => void;
  onBuyGame?: (game: GameInfo, selectedVariantId?: string) => void;
  selectedVariantByGameId: Record<string, string>;
  activeSessionAppIds: number[];
  rows: ConsoleLibraryRow[];
  rowRefs: RefObject<Array<HTMLDivElement | null>>;
  focusedRowIndex: number;
  focusedColumnIndex: number;
  onFocusCard: (rowIndex: number, columnIndex: number) => void;
  heroGame: GameInfo | undefined;
  controllerStoreFilterOpen: boolean;
  controllerStoreFilterItems: ControllerStoreFilterItem[];
  controllerStoreFilterId: string;
  focusedControllerStoreFilterIndex: number;
  onFocusControllerStoreFilter: (index: number) => void;
  onSelectControllerStoreFilter: (itemId: string) => void;
  controllerSearchOpen: boolean;
  controllerSearchInputRef: RefObject<HTMLInputElement | null>;
  detailsGame: GameInfo | null;
  detailsActionIndex: number;
  onFocusDetailsAction: (index: number) => void;
  onCloseDetails: () => void;
  storePickerOpen: boolean;
  storePickerIndex: number;
  onFocusStoreChoice: (index: number) => void;
  onSelectStoreChoice: (variantId: string) => void;
  onOpenStorePicker: () => void;
  onCloseStorePicker: () => void;
  onCycleGameVariant: (game: GameInfo | undefined) => void;
  onSelectHint?: () => void;
  onBackHint?: () => void;
  onFilterHint?: () => void;
  onSearchHint?: () => void;
  onMoreOptionsHint?: () => void;
  onCloseSearchHint?: () => void;
}

/**
 * Console library shell — a billboard over Netflix-style shelves.
 *
 * Like the rest of `components/console`, this tree carries no `motion` /
 * `AnimatePresence`: those crashed the Chromium renderer when combined with the
 * library hero and card strip. Every animation is CSS; see the performance
 * contract in styles/console.css.
 */
export function LibraryControllerView({
  isLoading,
  libraryCount,
  searchQuery,
  onSearchChange,
  onPlayGame,
  onBuyGame,
  selectedVariantByGameId,
  activeSessionAppIds,
  rows,
  rowRefs,
  focusedRowIndex,
  focusedColumnIndex,
  onFocusCard,
  heroGame,
  controllerStoreFilterOpen,
  controllerStoreFilterItems,
  controllerStoreFilterId,
  focusedControllerStoreFilterIndex,
  onFocusControllerStoreFilter,
  onSelectControllerStoreFilter,
  controllerSearchOpen,
  controllerSearchInputRef,
  detailsGame,
  detailsActionIndex,
  onFocusDetailsAction,
  onCloseDetails,
  storePickerOpen,
  storePickerIndex,
  onFocusStoreChoice,
  onSelectStoreChoice,
  onOpenStorePicker,
  onCloseStorePicker,
  onCycleGameVariant,
  onSelectHint,
  onBackHint,
  onFilterHint,
  onSearchHint,
  onMoreOptionsHint,
  onCloseSearchHint,
}: LibraryControllerViewProps): JSX.Element {
  const { t } = useTranslation();

  const storeFilterOverlay = controllerStoreFilterOpen && (
    <ConsoleOverlay
      label={t("library.chooseStore")}
      eyebrow={t("library.storeFilter")}
      title={t("library.chooseStore")}
    >
      <p className="console-overlay-body">{t("library.storeFilterHint")}</p>
      <div className="console-overlay-options">
        {controllerStoreFilterItems.map((item, index) => (
          <button
            key={item.id}
            type="button"
            className={`console-overlay-option${index === focusedControllerStoreFilterIndex ? " is-focused" : ""}${item.id === controllerStoreFilterId ? " is-selected" : ""}`}
            onClick={() => {
              onFocusControllerStoreFilter(index);
              onSelectControllerStoreFilter(item.id);
            }}
          >
            {item.title}
          </button>
        ))}
      </div>
    </ConsoleOverlay>
  );

  const searchOverlay = controllerSearchOpen && (
    <ConsoleOverlay label={t("app.actions.search")} eyebrow={t("app.actions.search")}>
      <input
        ref={controllerSearchInputRef}
        type="text"
        value={searchQuery}
        onChange={(event) => onSearchChange(event.target.value)}
        placeholder={t("library.searchPlaceholder")}
        className="console-search-input"
      />
      <div className="console-overlay-actions">
        <button type="button" className="console-action console-action--secondary" onClick={() => onCloseSearchHint?.()}>
          {t("app.actions.back")}
        </button>
      </div>
    </ConsoleOverlay>
  );

  if (isLoading) {
    return (
      <div className="console-page console-library">
        <div className="console-empty">
          <div className="console-spinner" role="status" aria-label={t("common.loading")} />
          <p>{t("library.empty.loadingLibrary")}</p>
        </div>
      </div>
    );
  }

  if (libraryCount === 0) {
    return (
      <div className="console-page console-library">
        <div className="console-empty">
          <Gamepad2 className="console-empty-icon" size={64} />
          <h3>{t("library.empty.libraryEmpty")}</h3>
          <p>{t("library.empty.ownedGamesAppearHere")}</p>
        </div>
      </div>
    );
  }

  if (rows.length === 0 || !heroGame) {
    return (
      <div className="console-page console-library">
        <div className="console-empty">
          <Search className="console-empty-icon" size={64} />
          <h3>{t("library.empty.noGamesFound")}</h3>
          <p>{t("library.empty.noGamesMatch", { query: searchQuery })}</p>
        </div>
        <ConsoleHintBar
          hints={[
            { glyph: "b", label: t("app.actions.back"), onSelect: () => onBackHint?.() },
            { glyph: "y", label: t("library.filter"), onSelect: () => onFilterHint?.() },
            { glyph: "x", label: t("app.actions.search"), onSelect: () => onSearchHint?.() },
          ]}
        />
        {storeFilterOverlay}
        {searchOverlay}
      </div>
    );
  }

  const heroVariantId = selectedVariantByGameId[heroGame.id];
  const heroHasActiveSession = gameMatchesActiveSession(heroGame, activeSessionAppIds);
  const heroShouldBuy = !heroHasActiveSession && !heroGame.isInLibrary;
  const heroStoreLabel = getSelectedVariantStoreLabel(heroGame, heroVariantId, t("library.storeNotListed"));

  return (
    <div className="console-page console-library">
      <div className="console-scroll">
        <ConsoleBillboard
          game={heroGame}
          selectedVariantId={heroVariantId}
          fallbackGenreLabel={t("home.controller.cloudGame")}
          extraChips={heroHasActiveSession ? [{ label: t("app.status.sessionActive"), accent: true }] : []}
          actions={(
            <>
              <button
                type="button"
                className="console-action console-action--primary"
                onClick={() => {
                  if (heroShouldBuy) {
                    onBuyGame?.(heroGame, heroVariantId);
                    return;
                  }
                  onPlayGame(heroGame);
                }}
              >
                {heroHasActiveSession ? t("app.actions.resume") : heroShouldBuy ? t("app.actions.buy") : t("app.actions.play")}
              </button>
              <button
                type="button"
                className="console-action console-action--secondary"
                onClick={() => onCycleGameVariant(heroGame)}
              >
                {heroStoreLabel}
              </button>
            </>
          )}
        />

        <div className="console-rows">
          {rows.map((row, rowIndex) => (
            <div key={row.id} ref={(element) => { rowRefs.current[rowIndex] = element; }}>
              <ConsoleRow
                title={row.title}
                games={row.games}
                rowIndex={rowIndex}
                isActiveRow={rowIndex === focusedRowIndex}
                focusedColumnIndex={focusedColumnIndex}
                selectedVariantByGameId={selectedVariantByGameId}
                getCardPill={(game) => (gameMatchesActiveSession(game, activeSessionAppIds)
                  ? { label: t("app.actions.resume"), tone: "session" }
                  : undefined)}
                onFocusCard={onFocusCard}
                onActivateCard={onPlayGame}
                showCount
              />
            </div>
          ))}
        </div>
      </div>

      <ConsoleHintBar
        hints={[
          { glyph: "a", label: t("library.viewDetails"), onSelect: () => onSelectHint?.() },
          { glyph: "b", label: t("app.actions.back"), onSelect: () => onBackHint?.() },
          { glyph: "y", label: t("library.filter"), onSelect: () => onFilterHint?.() },
          { glyph: "x", label: t("app.actions.search"), onSelect: () => onSearchHint?.() },
          { glyph: "menu", label: t("library.moreOptions"), onSelect: () => onMoreOptionsHint?.() },
        ]}
      />

      {storeFilterOverlay}
      {searchOverlay}
      {detailsGame && (
        <ConsoleGameDetails
          game={detailsGame}
          focusedActionIndex={detailsActionIndex}
          onFocusAction={onFocusDetailsAction}
          onClose={onCloseDetails}
          actions={[
            {
              id: "play",
              tone: "primary",
              label: gameMatchesActiveSession(detailsGame, activeSessionAppIds)
                ? t("app.actions.resume")
                : t("app.actions.play"),
              onSelect: () => onPlayGame(detailsGame),
            },
            ...(getConsoleStoreChoices(detailsGame, selectedVariantByGameId[detailsGame.id]).length > 1
              ? [{ id: "variant", label: t("library.changeStore"), onSelect: onOpenStorePicker }]
              : []),
            { id: "back", label: t("app.actions.back"), onSelect: onCloseDetails },
          ]}
        />
      )}

      {detailsGame && storePickerOpen && (
        <ConsoleStorePicker
          game={detailsGame}
          selectedVariantId={selectedVariantByGameId[detailsGame.id]}
          focusedIndex={storePickerIndex}
          onFocus={onFocusStoreChoice}
          onSelect={onSelectStoreChoice}
          onClose={onCloseStorePicker}
        />
      )}
    </div>
  );
}
