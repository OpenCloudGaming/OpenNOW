import { Gamepad2 } from "lucide-react";
import type { JSX, RefObject } from "react";
import type { GameInfo } from "@shared/gfn";
import { gameNeedsPurchase, getSelectedVariant } from "../../lib/controllerCatalogUi";
import { getConsoleStoreChoices } from "../../lib/consoleStoreChoices";
import { useTranslation } from "../../i18n";
import { ConsoleBillboard } from "./ConsoleBillboard";
import { ConsoleGameDetails, type ConsoleGameDetailsAction } from "./ConsoleGameDetails";
import { ConsoleHintBar } from "./ConsoleHintBar";
import { ConsoleOverlay } from "./ConsoleOverlay";
import { ConsoleRow } from "./ConsoleRow";
import { ConsoleStorePicker } from "./ConsoleStorePicker";

export interface ConsoleStoreSection {
  id: string;
  title: string;
  games: GameInfo[];
}

export interface ConsoleStoreViewProps {
  isLoading: boolean;
  sections: ConsoleStoreSection[];
  rowRefs: RefObject<Array<HTMLDivElement | null>>;
  focusedRowIndex: number;
  focusedColumnIndex: number;
  onFocusCard: (rowIndex: number, columnIndex: number) => void;
  onActivateCard: (game: GameInfo) => void;
  heroGame: GameInfo | undefined;
  selectedVariantByGameId: Record<string, string>;
  markOwnedInFlightByVariantId: Record<string, boolean>;
  onHeroPrimaryAction: (game: GameInfo) => void;
  onCycleVariant: () => void;
  onBack: () => void;
  detailsGame: GameInfo | null;
  detailsActionIndex: number;
  onFocusDetailsAction: (index: number) => void;
  onOpenDetails: (game: GameInfo) => void;
  onCloseDetails: () => void;
  storePickerOpen: boolean;
  storePickerIndex: number;
  onFocusStoreChoice: (index: number) => void;
  onSelectStoreChoice: (variantId: string) => void;
  onOpenStorePicker: () => void;
  onCloseStorePicker: () => void;
  searchQuery: string;
  onSearchChange: (query: string) => void;
  searchOpen: boolean;
  searchInputRef: RefObject<HTMLInputElement | null>;
  onOpenSearch: () => void;
  onCloseSearch: () => void;
}

/**
 * Presentational console store. All state lives in HomePage, mirroring the
 * existing LibraryPage → LibraryControllerView split.
 */
export function ConsoleStoreView({
  isLoading,
  sections,
  rowRefs,
  focusedRowIndex,
  focusedColumnIndex,
  onFocusCard,
  onActivateCard,
  heroGame,
  selectedVariantByGameId,
  markOwnedInFlightByVariantId,
  onHeroPrimaryAction,
  onCycleVariant,
  onBack,
  detailsGame,
  detailsActionIndex,
  onFocusDetailsAction,
  onOpenDetails,
  onCloseDetails,
  storePickerOpen,
  storePickerIndex,
  onFocusStoreChoice,
  onSelectStoreChoice,
  onOpenStorePicker,
  onCloseStorePicker,
  searchQuery,
  onSearchChange,
  searchOpen,
  searchInputRef,
  onOpenSearch,
  onCloseSearch,
}: ConsoleStoreViewProps): JSX.Element {
  const { t } = useTranslation();

  if (isLoading && sections.length === 0) {
    return (
      <div className="console-page">
        <div className="console-empty">
          <div className="console-spinner" role="status" aria-label={t("common.loading")} />
          <p>{t("home.empty.loadingGames")}</p>
        </div>
      </div>
    );
  }

  if (sections.length === 0) {
    return (
      <div className="console-page">
        <div className="console-empty">
          <Gamepad2 className="console-empty-icon" size={64} />
          <h3>{t("home.controller.emptyTitle")}</h3>
          <p>{t("home.controller.emptyBody")}</p>
        </div>
      </div>
    );
  }

  const buildStoreDetailActions = (game: GameInfo): ConsoleGameDetailsAction[] => {
    const variantId = selectedVariantByGameId[game.id];
    const variant = getSelectedVariant(game, variantId);
    const needsOwnership = gameNeedsPurchase(game, variantId);
    const marking = Boolean(needsOwnership && variant?.id && markOwnedInFlightByVariantId[variant.id]);
    const actions: ConsoleGameDetailsAction[] = [{
      id: "primary",
      tone: "primary",
      disabled: marking,
      label: needsOwnership
        ? (marking ? t("app.status.markingOwned") : t("app.actions.markAsOwned"))
        : t("app.actions.play"),
      onSelect: () => onActivateCard(game),
    }];
    if (getConsoleStoreChoices(game, variantId).length > 1) {
      actions.push({ id: "variant", label: t("library.changeStore"), onSelect: onOpenStorePicker });
    }
    actions.push({ id: "back", label: t("app.actions.back"), onSelect: onCloseDetails });
    return actions;
  };

  const heroVariantId = heroGame ? selectedVariantByGameId[heroGame.id] : undefined;
  const heroVariant = heroGame ? getSelectedVariant(heroGame, heroVariantId) : undefined;
  const heroNeedsOwnership = heroGame ? gameNeedsPurchase(heroGame, heroVariantId) : false;
  const heroMarkingOwned = Boolean(heroNeedsOwnership && heroVariant?.id && markOwnedInFlightByVariantId[heroVariant.id]);

  return (
    <div className="console-page console-store">
      <div className="console-scroll">
        {heroGame && (
          <ConsoleBillboard
            game={heroGame}
            selectedVariantId={heroVariantId}
            fallbackGenreLabel={t("home.controller.cloudGame")}
            extraChips={[{
              label: heroNeedsOwnership ? t("home.controller.notOwned") : t("home.controller.owned"),
              accent: !heroNeedsOwnership,
            }]}
            actions={(
              <button
                type="button"
                className="console-action console-action--primary"
                onClick={() => onHeroPrimaryAction(heroGame)}
                disabled={heroMarkingOwned}
              >
                {heroNeedsOwnership
                  ? (heroMarkingOwned ? t("app.status.markingOwned") : t("app.actions.markAsOwned"))
                  : t("app.actions.play")}
              </button>
            )}
          />
        )}

        <div className="console-rows">
          {sections.map((section, rowIndex) => (
            <div key={section.id} ref={(element) => { rowRefs.current[rowIndex] = element; }}>
              <ConsoleRow
                title={section.title || t("home.controller.featured")}
                games={section.games}
                rowIndex={rowIndex}
                isActiveRow={rowIndex === focusedRowIndex}
                focusedColumnIndex={focusedColumnIndex}
                selectedVariantByGameId={selectedVariantByGameId}
                getCardPill={(game) => (gameNeedsPurchase(game, selectedVariantByGameId[game.id])
                  ? { label: t("home.controller.notOwned"), tone: "neutral" }
                  : { label: t("home.controller.owned"), tone: "owned" })}
                onFocusCard={onFocusCard}
                onActivateCard={onActivateCard}
                showCount
              />
            </div>
          ))}
        </div>
      </div>

      <ConsoleHintBar
        hints={[
          { glyph: "a", label: t("library.viewDetails"), onSelect: () => heroGame && onOpenDetails(heroGame) },
          { glyph: "b", label: t("app.actions.back"), onSelect: onBack },
          { glyph: "x", label: t("app.actions.search"), onSelect: onOpenSearch },
          { glyph: "menu", label: t("library.moreOptions"), onSelect: onCycleVariant },
        ]}
      />

      {detailsGame && (
        <ConsoleGameDetails
          game={detailsGame}
          focusedActionIndex={detailsActionIndex}
          onFocusAction={onFocusDetailsAction}
          onClose={onCloseDetails}
          actions={buildStoreDetailActions(detailsGame)}
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

      {searchOpen && (
        <ConsoleOverlay label={t("app.actions.search")} eyebrow={t("app.actions.search")}>
          <input
            ref={searchInputRef}
            type="text"
            value={searchQuery}
            onChange={(event) => onSearchChange(event.target.value)}
            placeholder={t("home.searchPlaceholder")}
            className="console-search-input"
          />
          <div className="console-overlay-actions">
            <button type="button" className="console-action console-action--secondary" onClick={onCloseSearch}>
              {t("app.actions.back")}
            </button>
          </div>
        </ConsoleOverlay>
      )}
    </div>
  );
}
