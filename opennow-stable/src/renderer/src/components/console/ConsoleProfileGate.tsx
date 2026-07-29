import { useCallback, useEffect, useRef, useState } from "react";
import type { JSX } from "react";
import type { SavedAccount } from "@shared/gfn";

import {
  appendPinDigit,
  deletePinDigit,
  getLockoutSecondsRemaining,
  isPinComplete,
  PIN_RECOVERY_HOLD_MS,
} from "../../lib/consolePinInput";
import { clampRowFocus, moveRowFocus, type RowFocusDirection } from "../../lib/consoleRowFocus";
import { useControllerKeyDown, useControllerNavigation } from "../../hooks/useControllerNavigation";
import type { ConsoleShell } from "../../hooks/useConsoleShell";
import { controllerButton } from "../../utils/controllerGamepad";
import { useTranslation } from "../../i18n";
import { ConsoleManageProfiles } from "./ConsoleManageProfiles";
import { ConsolePinPad, PIN_KEYPAD_ROWS } from "./ConsolePinPad";
import { ConsoleProfilePicker } from "./ConsoleProfilePicker";
import { ConsoleSplash } from "./ConsoleSplash";

/** Picker tiles wrap at this many per row, matching the CSS grid. */
const PICKER_COLUMNS = 5;

export interface ConsoleProfileGateProps {
  shell: ConsoleShell;
  savedAccounts: SavedAccount[];
  activeUserId: string | null;
  onAddAccount: () => void;
  onRemoveAccount: (userId: string) => Promise<void> | void;
  onLogoutAll: () => void;
}

function buildPickerRowLengths(total: number): number[] {
  const rows: number[] = [];
  for (let remaining = total; remaining > 0; remaining -= PICKER_COLUMNS) {
    rows.push(Math.min(remaining, PICKER_COLUMNS));
  }
  return rows.length > 0 ? rows : [0];
}

/**
 * Stage router for the console profile gate, and the single owner of gamepad
 * navigation while it is open. The app shell behind it is `inert` (see the
 * `shellBlocked` wiring in App.tsx), so exactly one poller is ever live.
 *
 * Every gamepad action has a keyboard equivalent — the screens are otherwise
 * unverifiable, since CDP cannot synthesize Gamepad API input.
 */
export function ConsoleProfileGate({
  shell,
  savedAccounts,
  activeUserId,
  onAddAccount,
  onRemoveAccount,
  onLogoutAll,
}: ConsoleProfileGateProps): JSX.Element | null {
  const { t } = useTranslation();
  const { stage, pickerEntries, pendingAccount, verifyResult } = shell;

  const [pickerIndex, setPickerIndex] = useState(0);
  const [pinEntry, setPinEntry] = useState("");
  const [pinFocus, setPinFocus] = useState({ rowIndex: 0, columnIndex: 0 });
  const [pinError, setPinError] = useState(false);
  const [recoveryProgress, setRecoveryProgress] = useState(0);
  const [recoveryArmed, setRecoveryArmed] = useState(false);
  const [nowMs, setNowMs] = useState(() => Date.now());
  const recoveryStartedAtRef = useRef<number | null>(null);
  const submittingRef = useRef(false);

  const lockedUntilMs = verifyResult?.lockedUntilMs ?? null;
  const lockoutSeconds = getLockoutSecondsRemaining(lockedUntilMs, nowMs);
  const isLocked = lockoutSeconds > 0;

  // Only tick while a lockout is actually counting down.
  useEffect(() => {
    if (stage !== "pin" || lockedUntilMs === null) return undefined;
    const interval = window.setInterval(() => setNowMs(Date.now()), 500);
    return () => window.clearInterval(interval);
  }, [lockedUntilMs, stage]);

  useEffect(() => {
    setPinEntry("");
    setPinError(false);
    setPinFocus({ rowIndex: 0, columnIndex: 0 });
    setRecoveryProgress(0);
    setRecoveryArmed(false);
    recoveryStartedAtRef.current = null;
  }, [pendingAccount?.userId, stage]);

  useEffect(() => {
    setPickerIndex((index) => Math.max(0, Math.min(index, pickerEntries.length - 1)));
  }, [pickerEntries.length]);

  const submitPin = useCallback(async (entry: string) => {
    if (submittingRef.current) return;
    submittingRef.current = true;
    try {
      const ok = await shell.verifyPin(entry);
      if (!ok) {
        setPinError(true);
        setPinEntry("");
        setNowMs(Date.now());
      }
    } finally {
      submittingRef.current = false;
    }
  }, [shell]);

  const pressPinKey = useCallback((key: string) => {
    if (key === "cancel") {
      shell.cancelPin();
      return;
    }
    if (isLocked) return;
    if (key === "delete") {
      setPinEntry((entry) => deletePinDigit(entry));
      setPinError(false);
      return;
    }

    setPinError(false);
    setPinEntry((entry) => {
      const next = appendPinDigit(entry, key);
      if (isPinComplete(next)) void submitPin(next);
      return next;
    });
  }, [isLocked, shell, submitPin]);

  const movePinFocus = useCallback((direction: RowFocusDirection) => {
    setPinFocus((focus) => moveRowFocus(PIN_KEYPAD_ROWS.map((row) => row.length), focus, direction));
  }, []);

  const pressFocusedPinKey = useCallback(() => {
    const key = PIN_KEYPAD_ROWS[pinFocus.rowIndex]?.[pinFocus.columnIndex];
    if (key) pressPinKey(key);
  }, [pinFocus, pressPinKey]);

  const movePickerFocus = useCallback((direction: RowFocusDirection) => {
    const rowLengths = buildPickerRowLengths(pickerEntries.length);
    const current = clampRowFocus(rowLengths, {
      rowIndex: Math.floor(pickerIndex / PICKER_COLUMNS),
      columnIndex: pickerIndex % PICKER_COLUMNS,
    });
    const next = moveRowFocus(rowLengths, current, direction);
    setPickerIndex(Math.min(next.rowIndex * PICKER_COLUMNS + next.columnIndex, pickerEntries.length - 1));
  }, [pickerEntries.length, pickerIndex]);

  const beginRecoveryHold = useCallback(() => {
    if (recoveryStartedAtRef.current === null) recoveryStartedAtRef.current = performance.now();
  }, []);

  const endRecoveryHold = useCallback(() => {
    recoveryStartedAtRef.current = null;
    setRecoveryProgress(0);
  }, []);

  useControllerKeyDown(stage !== "shell", (event) => {
    if (stage === "splash") {
      event.preventDefault();
      shell.skipSplash();
      return;
    }

    if (stage === "picker") {
      if (event.key === "ArrowLeft") { event.preventDefault(); movePickerFocus("left"); }
      else if (event.key === "ArrowRight") { event.preventDefault(); movePickerFocus("right"); }
      else if (event.key === "ArrowUp") { event.preventDefault(); movePickerFocus("up"); }
      else if (event.key === "ArrowDown") { event.preventDefault(); movePickerFocus("down"); }
      else if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        void shell.selectPickerEntry(pickerIndex);
      }
      return;
    }

    if (stage === "pin") {
      if (/^[0-9]$/.test(event.key)) { event.preventDefault(); pressPinKey(event.key); }
      else if (event.key === "Backspace") { event.preventDefault(); pressPinKey("delete"); }
      else if (event.key === "Escape") { event.preventDefault(); shell.cancelPin(); }
      else if (event.key === "ArrowLeft") { event.preventDefault(); movePinFocus("left"); }
      else if (event.key === "ArrowRight") { event.preventDefault(); movePinFocus("right"); }
      else if (event.key === "ArrowUp") { event.preventDefault(); movePinFocus("up"); }
      else if (event.key === "ArrowDown") { event.preventDefault(); movePinFocus("down"); }
      else if (event.key === "Enter") { event.preventDefault(); pressFocusedPinKey(); }
      return;
    }

    if (stage === "manage" && (event.key === "Escape" || event.key.toLowerCase() === "b")) {
      event.preventDefault();
      shell.openPicker();
    }
  });

  useControllerNavigation({
    enabled: stage !== "shell",
    onFrame: ({ buttons, pressed }) => {
      if (stage === "splash") {
        if (pressed) shell.skipSplash();
        return;
      }

      if (stage === "picker") {
        if (pressed & controllerButton.left) movePickerFocus("left");
        if (pressed & controllerButton.right) movePickerFocus("right");
        if (pressed & controllerButton.up) movePickerFocus("up");
        if (pressed & controllerButton.down) movePickerFocus("down");
        if (pressed & controllerButton.south) void shell.selectPickerEntry(pickerIndex);
        return;
      }

      if (stage === "pin") {
        // Hold B for the recovery option; a tap cancels. Progress is derived
        // from the live hold rather than a timer so releasing resets it.
        if (buttons & controllerButton.east) {
          beginRecoveryHold();
          const startedAt = recoveryStartedAtRef.current;
          if (startedAt !== null) {
            const progress = Math.min(1, (performance.now() - startedAt) / PIN_RECOVERY_HOLD_MS);
            setRecoveryProgress(progress);
            if (progress >= 1 && !recoveryArmed) setRecoveryArmed(true);
          }
        } else if (recoveryStartedAtRef.current !== null) {
          const wasArmed = recoveryArmed;
          endRecoveryHold();
          if (!wasArmed) shell.cancelPin();
        }

        if (pressed & controllerButton.left) movePinFocus("left");
        if (pressed & controllerButton.right) movePinFocus("right");
        if (pressed & controllerButton.up) movePinFocus("up");
        if (pressed & controllerButton.down) movePinFocus("down");
        if (pressed & controllerButton.south) pressFocusedPinKey();
        return;
      }

      if (stage === "manage" && (pressed & controllerButton.east)) shell.openPicker();
    },
  });

  if (stage === "shell") return null;

  return (
    <div className="console-gate" role="dialog" aria-modal="true" aria-label={t("console.profiles.whosPlaying")}>
      {stage === "splash" && <ConsoleSplash />}

      {stage === "picker" && (
        <ConsoleProfilePicker
          entries={pickerEntries}
          focusedIndex={pickerIndex}
          activeUserId={activeUserId}
          errorMessage={shell.errorMessage ?? undefined}
          onFocus={setPickerIndex}
          onSelect={(index) => void shell.selectPickerEntry(index)}
          onBack={activeUserId ? shell.closeToShell : undefined}
        />
      )}

      {stage === "pin" && pendingAccount && (
        <>
          <ConsolePinPad
            profileName={pendingAccount.displayName}
            entry={pinEntry}
            isError={pinError}
            errorMessage={t("console.pin.incorrect", { count: verifyResult?.remainingAttempts ?? 0 })}
            lockedMessage={t("console.pin.lockedOut", { seconds: lockoutSeconds })}
            isLocked={isLocked}
            focusedRow={pinFocus.rowIndex}
            focusedColumn={pinFocus.columnIndex}
            onFocusKey={(rowIndex, columnIndex) => setPinFocus({ rowIndex, columnIndex })}
            onPressKey={pressPinKey}
            recoveryProgress={recoveryProgress}
            recoveryHint={recoveryArmed ? t("console.pin.recoveryReady") : t("console.pin.recoveryHint")}
          />
          {recoveryArmed && (
            <div className="console-overlay-actions">
              <button
                type="button"
                className="console-action console-action--danger"
                onClick={() => void shell.removePendingProfile()}
              >
                {t("console.pin.removeProfile")}
              </button>
              <button
                type="button"
                className="console-action console-action--secondary"
                onClick={() => { setRecoveryArmed(false); endRecoveryHold(); }}
              >
                {t("app.actions.back")}
              </button>
            </div>
          )}
        </>
      )}

      {stage === "manage" && (
        <ConsoleManageProfiles
          savedAccounts={savedAccounts}
          activeUserId={activeUserId}
          onAddAccount={onAddAccount}
          onRemoveAccount={onRemoveAccount}
          onLogoutAll={onLogoutAll}
          onBack={shell.openPicker}
        />
      )}
    </div>
  );
}
