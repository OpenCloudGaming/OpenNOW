import { Menu } from "lucide-react";
import type { JSX } from "react";

export type ConsoleHintGlyph = "a" | "b" | "x" | "y" | "menu";

export interface ConsoleHint {
  glyph: ConsoleHintGlyph;
  label: string;
  onSelect?: () => void;
}

export interface ConsoleHintBarProps {
  hints: ConsoleHint[];
}

/** Shared button-prompt strip along the bottom of every console surface. */
export function ConsoleHintBar({ hints }: ConsoleHintBarProps): JSX.Element {
  return (
    <div className="console-hint-bar" role="toolbar">
      {hints.map((hint) => (
        <button key={`${hint.glyph}-${hint.label}`} type="button" className="console-hint" onClick={() => hint.onSelect?.()}>
          <span className={`console-hint-glyph console-hint-glyph--${hint.glyph}`} aria-hidden="true">
            {hint.glyph === "menu" ? <Menu /> : hint.glyph.toUpperCase()}
          </span>
          <span>{hint.label}</span>
        </button>
      ))}
    </div>
  );
}
