import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ConsolePinVerifyResult, SavedAccount } from "@shared/gfn";

import {
  CONSOLE_SPLASH_MS,
  resolveInitialConsoleStage,
  resolveProfileSelection,
  type ConsoleShellStage,
} from "../lib/consoleShellState";
import type { ConsolePickerEntry } from "../components/console/ConsoleProfilePicker";

export interface UseConsoleShellInput {
  controllerMode: boolean;
  directLaunchConsoleMode: boolean;
  pickerEnabled: boolean;
  /** True while the saved auth session is still being restored. */
  isInitializing: boolean;
  hasAuthSession: boolean;
  savedAccounts: SavedAccount[];
  activeUserId: string | null;
  onSwitchAccount: (userId: string) => Promise<void> | void;
  onAddAccount: () => void;
  onRemoveAccount: (userId: string) => Promise<void> | void;
}

export interface ConsoleShell {
  stage: ConsoleShellStage;
  pickerEntries: ConsolePickerEntry[];
  pendingAccount: SavedAccount | undefined;
  verifyResult: ConsolePinVerifyResult | null;
  errorMessage: string | null;
  openPicker: () => void;
  skipSplash: () => void;
  closeToShell: () => void;
  openManage: () => void;
  selectPickerEntry: (index: number) => Promise<void>;
  verifyPin: (pin: string) => Promise<boolean>;
  cancelPin: () => void;
  removePendingProfile: () => Promise<void>;
}

/**
 * Owns the console profile gate: which stage is showing, which profile is
 * pending PIN entry, and the account calls each selection triggers.
 */
export function useConsoleShell({
  controllerMode,
  directLaunchConsoleMode,
  pickerEnabled,
  isInitializing,
  hasAuthSession,
  savedAccounts,
  activeUserId,
  onSwitchAccount,
  onAddAccount,
  onRemoveAccount,
}: UseConsoleShellInput): ConsoleShell {
  const [stage, setStage] = useState<ConsoleShellStage>("shell");
  const [pendingUserId, setPendingUserId] = useState<string | null>(null);
  const [verifyResult, setVerifyResult] = useState<ConsolePinVerifyResult | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const bootResolvedRef = useRef(false);

  // Resolve the launch stage exactly once, and only from a definitively loaded
  // state. The restore flag, the session, and the account list each land in a
  // separate state update; latching in between would decide "nothing to pick"
  // and, because it latches, never re-run. A signed-out user never reaches
  // here — App renders the login screen instead.
  useEffect(() => {
    if (bootResolvedRef.current) return;
    if (!controllerMode) return;
    if (isInitializing) return;
    if (!hasAuthSession || savedAccounts.length === 0) return;
    bootResolvedRef.current = true;
    setStage(resolveInitialConsoleStage({
      controllerMode,
      directLaunchConsoleMode,
      pickerEnabled,
      hasAuthSession,
      savedAccountCount: savedAccounts.length,
    }));
  }, [
    controllerMode,
    directLaunchConsoleMode,
    hasAuthSession,
    isInitializing,
    pickerEnabled,
    savedAccounts.length,
  ]);

  // Leaving console mode must never strand the gate over the desktop shell.
  useEffect(() => {
    if (controllerMode) return;
    bootResolvedRef.current = false;
    setStage("shell");
    setPendingUserId(null);
  }, [controllerMode]);

  const pickerEntries = useMemo<ConsolePickerEntry[]>(() => [
    ...savedAccounts.map((account) => ({ kind: "account" as const, account })),
    { kind: "action" as const, action: "add" as const },
    { kind: "action" as const, action: "manage" as const },
  ], [savedAccounts]);

  const pendingAccount = useMemo(
    () => savedAccounts.find((account) => account.userId === pendingUserId),
    [pendingUserId, savedAccounts],
  );

  const openPicker = useCallback(() => {
    setErrorMessage(null);
    setPendingUserId(null);
    setStage("picker");
  }, []);

  // The splash is a timed hand-off, never a place the user can get stuck.
  useEffect(() => {
    if (stage !== "splash") return undefined;
    const timer = window.setTimeout(() => setStage("picker"), CONSOLE_SPLASH_MS);
    return () => window.clearTimeout(timer);
  }, [stage]);

  /** Lets any button press skip the splash rather than waiting it out. */
  const skipSplash = useCallback(() => {
    setStage((current) => (current === "splash" ? "picker" : current));
  }, []);

  const closeToShell = useCallback(() => {
    setErrorMessage(null);
    setPendingUserId(null);
    setVerifyResult(null);
    setStage("shell");
  }, []);

  const openManage = useCallback(() => {
    setErrorMessage(null);
    setStage("manage");
  }, []);

  const enterAccount = useCallback(async (account: SavedAccount, alreadyActive: boolean) => {
    if (!alreadyActive) {
      try {
        await onSwitchAccount(account.userId);
      } catch (error) {
        setErrorMessage(error instanceof Error ? error.message : String(error));
        setStage("picker");
        return;
      }
    }
    closeToShell();
  }, [closeToShell, onSwitchAccount]);

  const selectPickerEntry = useCallback(async (index: number) => {
    const entry = pickerEntries[index];
    if (!entry) return;

    if (entry.kind === "action") {
      if (entry.action === "add") {
        onAddAccount();
        return;
      }
      openManage();
      return;
    }

    setErrorMessage(null);
    const { action } = resolveProfileSelection(entry.account, activeUserId);
    if (action === "verify") {
      setPendingUserId(entry.account.userId);
      setVerifyResult(null);
      const status = await window.openNow.getConsolePinStatus(entry.account.userId);
      setVerifyResult({
        ok: false,
        remainingAttempts: status.remainingAttempts,
        lockedUntilMs: status.lockedUntilMs,
      });
      setStage("pin");
      return;
    }

    await enterAccount(entry.account, action === "enter");
  }, [activeUserId, enterAccount, onAddAccount, openManage, pickerEntries]);

  const verifyPin = useCallback(async (pin: string): Promise<boolean> => {
    if (!pendingAccount) return false;
    const result = await window.openNow.verifyConsolePin({ userId: pendingAccount.userId, pin });
    setVerifyResult(result);
    if (!result.ok) return false;

    await enterAccount(pendingAccount, pendingAccount.userId === activeUserId);
    return true;
  }, [activeUserId, enterAccount, pendingAccount]);

  const cancelPin = useCallback(() => {
    setPendingUserId(null);
    setVerifyResult(null);
    setStage("picker");
  }, []);

  /**
   * The forgotten-PIN escape hatch: signing the account out entirely is the
   * only recovery that does not weaken the lock, since there is no server and
   * no reset flow. The user must log in again afterwards.
   */
  const removePendingProfile = useCallback(async () => {
    if (!pendingAccount) return;
    await onRemoveAccount(pendingAccount.userId);
    setPendingUserId(null);
    setVerifyResult(null);
    setStage("picker");
  }, [onRemoveAccount, pendingAccount]);

  // A profile that disappears underneath the PIN pad must not leave it stuck.
  useEffect(() => {
    if (stage !== "pin" || pendingAccount) return;
    setStage("picker");
    setPendingUserId(null);
  }, [pendingAccount, stage]);

  return {
    stage,
    pickerEntries,
    pendingAccount,
    verifyResult,
    errorMessage,
    openPicker,
    skipSplash,
    closeToShell,
    openManage,
    selectPickerEntry,
    verifyPin,
    cancelPin,
    removePendingProfile,
  };
}
