import { Check } from "lucide-react";
import type { CSSProperties, JSX } from "react";
import type { GameInfo } from "@shared/gfn";
import { getConsoleStoreChoices } from "../../lib/consoleStoreChoices";
import { getStoreIconComponent } from "../GameCard";
import { useTranslation } from "../../i18n";

export interface ConsoleStorePickerProps {
  game: GameInfo;
  selectedVariantId?: string;
  focusedIndex: number;
  onFocus: (index: number) => void;
  onSelect: (variantId: string) => void;
  onClose: () => void;
}

/**
 * Store chooser for a game's detail sheet — one row per store variant, with the
 * store's logo and an owned marker. The row count is whatever the game actually
 * offers, so a single-store title never opens this.
 */
export function ConsoleStorePicker({
  game,
  selectedVariantId,
  focusedIndex,
  onFocus,
  onSelect,
  onClose,
}: ConsoleStorePickerProps): JSX.Element {
  const { t } = useTranslation();
  const choices = getConsoleStoreChoices(game, selectedVariantId);

  return (
    <div className="console-store-picker" role="dialog" aria-modal="true" aria-label={t("library.chooseStore")}>
      <div className="console-store-picker-panel">
        {/* The game, not "store filter" — that label belongs to the library's
            Y-hold filter, which is a different thing entirely. */}
        <span className="console-overlay-eyebrow">{game.title}</span>
        <h3 className="console-overlay-title">{t("library.chooseStore")}</h3>

        <div className="console-store-picker-list">
          {choices.map((choice, index) => {
            const StoreIcon = getStoreIconComponent(choice.store);
            return (
              <button
                key={choice.variantId}
                type="button"
                className={`console-store-choice${index === focusedIndex ? " is-focused" : ""}${choice.isActive ? " is-active" : ""}`}
                style={{ "--i": index } as CSSProperties}
                data-console-store-choice={choice.variantId}
                onClick={() => {
                  onFocus(index);
                  onSelect(choice.variantId);
                }}
              >
                <span className="console-store-choice-icon" aria-hidden="true"><StoreIcon /></span>
                <span className="console-store-choice-name">{choice.label}</span>
                {choice.isOwned && (
                  <span className="console-store-choice-owned">
                    <Check aria-hidden="true" />
                    {t("home.controller.owned")}
                  </span>
                )}
                {choice.isActive && <span className="console-store-choice-active" aria-hidden="true" />}
              </button>
            );
          })}
        </div>

        <div className="console-overlay-actions">
          <button type="button" className="console-action console-action--secondary" onClick={onClose}>
            {t("app.actions.back")}
          </button>
        </div>
      </div>
    </div>
  );
}
