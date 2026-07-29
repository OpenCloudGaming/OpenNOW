import { ipcMain } from "electron";

import type {
  ConsolePinClearRequest,
  ConsolePinMutationResult,
  ConsolePinSetRequest,
  ConsolePinStatus,
  ConsolePinVerifyRequest,
  ConsolePinVerifyResult,
} from "@shared/gfn";
import { IPC_CHANNELS } from "@shared/ipc";

import type { ConsoleProfileStore } from "../platforms/gfn/auth/consoleProfileStore";

export interface ConsolePinIpcDependencies {
  getConsoleProfiles: () => ConsoleProfileStore;
}

/**
 * Verify-only PIN surface for the renderer.
 *
 * Nothing here ever returns a hash, salt, or KDF parameter — the renderer only
 * learns whether a PIN exists, whether a guess was right, and how long it is
 * locked out for.
 */
export function registerConsolePinIpcHandlers(deps: ConsolePinIpcDependencies): void {
  ipcMain.handle(
    IPC_CHANNELS.CONSOLE_PIN_GET_STATUS,
    (_event, userId: string): ConsolePinStatus =>
      deps.getConsoleProfiles().getStatus(userId, Date.now()),
  );

  ipcMain.handle(
    IPC_CHANNELS.CONSOLE_PIN_SET,
    (_event, input: ConsolePinSetRequest): Promise<ConsolePinMutationResult> =>
      deps.getConsoleProfiles().setPin(input.userId, input.pin, input.currentPin),
  );

  ipcMain.handle(
    IPC_CHANNELS.CONSOLE_PIN_CLEAR,
    (_event, input: ConsolePinClearRequest): Promise<ConsolePinMutationResult> =>
      deps.getConsoleProfiles().clearPin(input.userId, input.currentPin),
  );

  ipcMain.handle(
    IPC_CHANNELS.CONSOLE_PIN_VERIFY,
    (_event, input: ConsolePinVerifyRequest): Promise<ConsolePinVerifyResult> =>
      deps.getConsoleProfiles().verifyPin(input.userId, input.pin),
  );
}
