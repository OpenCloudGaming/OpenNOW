import type { BrowserWindow } from "electron";
import { serializeSessionErrorTransport } from "@shared/sessionError";
import type { SessionConflictChoice } from "@shared/gfn";
import { enrichErrorForIpc } from "@shared/networkError";
import { isSessionError, SessionError } from "../gfn/errorCodes";

export interface SessionConflictDialogDeps {
  dialog: Electron.Dialog;
  getMainWindow(): BrowserWindow | null;
}

export async function showSessionConflictDialog(
  deps: SessionConflictDialogDeps,
): Promise<SessionConflictChoice> {
  const mainWindow = deps.getMainWindow();
  if (!mainWindow || mainWindow.isDestroyed()) {
    return "cancel";
  }

  const result = await deps.dialog.showMessageBox(mainWindow, {
    type: "question",
    buttons: ["Resume", "Start New", "Cancel"],
    defaultId: 0,
    cancelId: 2,
    title: "Active Session Detected",
    message: "You have an active session running.",
    detail: "Resume it or start a new one?",
  });

  switch (result.response) {
    case 0:
      return "resume";
    case 1:
      return "new";
    default:
      return "cancel";
  }
}

export function isSessionConflictError(error: unknown): boolean {
  if (isSessionError(error)) {
    return error.isSessionConflict();
  }
  return false;
}

export function rethrowSerializedSessionError(error: unknown): never {
  if (error instanceof SessionError) {
    throw new Error(serializeSessionErrorTransport(error.toJSON()));
  }
  throw enrichErrorForIpc(error);
}
