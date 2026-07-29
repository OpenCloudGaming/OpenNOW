import { Delete, X } from "lucide-react";
import type { JSX } from "react";
import { PIN_LENGTH } from "../../lib/consolePinInput";
import { useTranslation } from "../../i18n";

/** 3x4 grid: nine digits, delete, zero, cancel. */
export const PIN_KEYPAD_ROWS: ReadonlyArray<ReadonlyArray<string>> = [
  ["1", "2", "3"],
  ["4", "5", "6"],
  ["7", "8", "9"],
  ["delete", "0", "cancel"],
];

export interface ConsolePinPadProps {
  profileName: string;
  entry: string;
  isError: boolean;
  errorMessage?: string;
  lockedMessage?: string;
  isLocked: boolean;
  focusedRow: number;
  focusedColumn: number;
  onFocusKey: (row: number, column: number) => void;
  onPressKey: (key: string) => void;
  /** 0..1 progress of the hold-B recovery gesture. */
  recoveryProgress: number;
  recoveryHint: string;
}

const RING_RADIUS = 12;
const RING_CIRCUMFERENCE = 2 * Math.PI * RING_RADIUS;

/**
 * PIN entry for a locked console profile.
 *
 * Every gamepad action has a keyboard equivalent (digits, Backspace, Escape):
 * CDP cannot synthesize Gamepad API input, so keyboard parity is what makes
 * this screen verifiable, and it is the right accessibility answer anyway.
 */
export function ConsolePinPad({
  profileName,
  entry,
  isError,
  errorMessage,
  lockedMessage,
  isLocked,
  focusedRow,
  focusedColumn,
  onFocusKey,
  onPressKey,
  recoveryProgress,
  recoveryHint,
}: ConsolePinPadProps): JSX.Element {
  const { t } = useTranslation();

  return (
    <div className={`console-pin-panel${isError ? " is-error" : ""}${isLocked ? " console-pin-pad--locked" : ""}`}>
      <h1 className="console-gate-title">{t("console.pin.title", { name: profileName })}</h1>
      <p className="console-gate-subtitle">{t("console.pin.subtitle")}</p>

      <div className="console-pin-dots" role="status" aria-label={t("console.pin.enteredDigits", { count: entry.length })}>
        {Array.from({ length: PIN_LENGTH }).map((_, index) => (
          <span
            key={index}
            className={`console-pin-dot${index < entry.length ? " console-pin-dot--filled" : ""}`}
          />
        ))}
      </div>

      {isLocked && lockedMessage && <p className="console-gate-error">{lockedMessage}</p>}
      {!isLocked && isError && errorMessage && <p className="console-gate-error">{errorMessage}</p>}

      <div className="console-pin-keypad">
        {PIN_KEYPAD_ROWS.map((row, rowIndex) => row.map((key, columnIndex) => (
          <button
            key={key}
            type="button"
            className={`console-pin-key${rowIndex === focusedRow && columnIndex === focusedColumn ? " is-focused" : ""}`}
            data-console-pin-key={key}
            disabled={isLocked && key !== "cancel"}
            aria-label={key === "delete" ? t("console.pin.delete") : key === "cancel" ? t("app.actions.back") : key}
            onClick={() => {
              onFocusKey(rowIndex, columnIndex);
              onPressKey(key);
            }}
          >
            {key === "delete" ? <Delete /> : key === "cancel" ? <X /> : key}
          </button>
        )))}
      </div>

      <div className="console-pin-recovery">
        <svg className="console-pin-recovery-ring" viewBox="0 0 30 30" aria-hidden="true">
          <circle className="track" cx="15" cy="15" r={RING_RADIUS} />
          <circle
            className="progress"
            cx="15"
            cy="15"
            r={RING_RADIUS}
            strokeDasharray={RING_CIRCUMFERENCE}
            strokeDashoffset={RING_CIRCUMFERENCE * (1 - Math.min(Math.max(recoveryProgress, 0), 1))}
          />
        </svg>
        <span>{recoveryHint}</span>
      </div>
    </div>
  );
}
