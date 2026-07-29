import type { SavedAccount } from "@shared/gfn";

export type ConsoleShellStage = "shell" | "splash" | "picker" | "pin" | "manage";

/** How long the console splash holds before handing over to the picker. */
export const CONSOLE_SPLASH_MS = 1600;

export interface ResolveInitialConsoleStageInput {
  controllerMode: boolean;
  /** A CLI/Playnite direct launch — an explicit app id the user already chose. */
  directLaunchConsoleMode: boolean;
  /** `settings.consoleProfilePickerOnLaunch`. */
  pickerEnabled: boolean;
  hasAuthSession: boolean;
  savedAccountCount: number;
}

export type ProfileSelectionAction = "enter" | "verify" | "switch";

/**
 * Which console stage the app should boot into.
 *
 * Direct launches are deliberately never gated: the shortcut already carries an
 * explicit app id the user chose in their frontend, and the app quits when that
 * session ends. Blocking on a picker would break the one-click contract and can
 * strand a TV user with no keyboard. The PIN gates *switching* profiles, not
 * resuming the one already active.
 */
export function resolveInitialConsoleStage({
  controllerMode,
  directLaunchConsoleMode,
  pickerEnabled,
  hasAuthSession,
  savedAccountCount,
}: ResolveInitialConsoleStageInput): ConsoleShellStage {
  if (!controllerMode) return "shell";
  if (directLaunchConsoleMode) return "shell";
  if (!pickerEnabled) return "shell";
  if (!hasAuthSession) return "shell";
  // Nothing to pick between, and no profile management to reach.
  if (savedAccountCount === 0) return "shell";
  // Branded splash first, then the picker — dropping straight into a profile
  // grid gives console mode no sense of having started.
  return "splash";
}

/**
 * What selecting a profile tile should do. A locked profile always verifies,
 * even when it is already active — otherwise the lock would be bypassed by
 * simply reopening the picker.
 */
export function resolveProfileSelection(
  account: Pick<SavedAccount, "userId" | "hasPin">,
  activeUserId: string | null,
): { action: ProfileSelectionAction } {
  if (account.hasPin) return { action: "verify" };
  if (account.userId === activeUserId) return { action: "enter" };
  return { action: "switch" };
}
