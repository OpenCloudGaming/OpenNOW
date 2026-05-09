import type { JSX } from "react";

interface OptionsSheetProps {
  optionsOpen: boolean;
  optionsEntries: Array<{ id: string; label: string }>;
  optionsFocusIndex: number;
}

export function OptionsSheet({
  optionsOpen,
  optionsEntries,
  optionsFocusIndex,
}: OptionsSheetProps): JSX.Element | null {
  if (!(optionsOpen && optionsEntries.length > 0)) {
    return null;
  }

  return (
    <div className="xmb-ps5-options-sheet" role="dialog" aria-modal="true" aria-label="Options">
      <div className="xmb-ps5-options-backdrop" aria-hidden />
      <div className="xmb-ps5-options-panel">
        <div className="xmb-ps5-options-title">Options</div>
        <ul className="xmb-ps5-options-list">
          {optionsEntries.map((opt, i) => (
            <li key={`${opt.id}-${i}`} className={`xmb-ps5-options-item ${i === optionsFocusIndex ? "active" : ""}`}>
              {opt.label}
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
