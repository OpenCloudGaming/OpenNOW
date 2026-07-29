import type { CSSProperties, JSX } from "react";
import type { GameInfo } from "@shared/gfn";
import { ConsolePosterCard } from "./ConsolePosterCard";

/** Rows past this index defer rendering until they are scrolled toward. */
const DEFERRED_ROW_INDEX = 2;

export interface ConsoleRowProps {
  title: string;
  games: GameInfo[];
  rowIndex: number;
  isActiveRow: boolean;
  focusedColumnIndex: number;
  selectedVariantByGameId: Record<string, string>;
  getCardPill?: (game: GameInfo) => { label: string; tone?: "neutral" | "owned" | "session" } | undefined;
  onFocusCard: (rowIndex: number, columnIndex: number) => void;
  onActivateCard: (game: GameInfo) => void;
  showCount?: boolean;
}

/**
 * A single Netflix-style shelf.
 *
 * The console component tree is intentionally free of `motion` /
 * `AnimatePresence`: those crashed the Chromium renderer when combined with the
 * library hero and card strip, and were never root-caused. Every animation here
 * is CSS — see the performance contract at the top of styles/console.css.
 * For the same reason console overlays must not use `ui/ModalSurface`, which
 * wraps AnimatePresence.
 */
export function ConsoleRow({
  title,
  games,
  rowIndex,
  isActiveRow,
  focusedColumnIndex,
  selectedVariantByGameId,
  getCardPill,
  onFocusCard,
  onActivateCard,
  showCount = false,
}: ConsoleRowProps): JSX.Element {
  return (
    <section
      className={`console-row${isActiveRow ? " console-row--active" : ""}${rowIndex > DEFERRED_ROW_INDEX ? " console-row--deferred" : ""}`}
      style={{ "--i": rowIndex } as CSSProperties}
      data-console-row={rowIndex}
      role="group"
      aria-label={title}
    >
      <div className="console-row-heading">
        <h2 className="console-row-title">{title}</h2>
        {showCount && <span className="console-row-count">{games.length}</span>}
      </div>
      <div className="console-row-track">
        {games.map((game, columnIndex) => (
          <ConsolePosterCard
            key={game.id}
            game={game}
            index={columnIndex}
            isFocused={isActiveRow && columnIndex === focusedColumnIndex}
            selectedVariantId={selectedVariantByGameId[game.id]}
            pill={getCardPill?.(game)}
            onSelect={() => onFocusCard(rowIndex, columnIndex)}
            onActivate={() => onActivateCard(game)}
          />
        ))}
      </div>
    </section>
  );
}
