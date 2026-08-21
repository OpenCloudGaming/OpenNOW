import type { SessionInfo } from "@shared/gfn";
import type { StreamStatus } from "./appTypes";
import type { GfnWebRtcClient } from "../platforms/gfn/webrtcClient";

export type SignalingRecoveryState = {
  attemptCount: number;
  deadlineAtMs: number | null;
  inFlight: Promise<boolean> | null;
  explicitShutdown: boolean;
  appId: number | null;
  generation: number;
};

export const RECOVERABLE_STREAM_STATUSES: readonly StreamStatus[] = ["streaming"];
export const SIGNALING_RECOVERY_WINDOW_MS = 300_000;
export const SIGNALING_RECOVERY_POLL_INTERVAL_MS = 5_000;
export const SIGNALING_RECOVERY_STABLE_RESET_DELAY_MS = 15000;
export const SIGNALING_REMOTE_ICE_GRACE_MS = 5000;
export const ICE_DISCONNECTED_RECOVERY_GRACE_MS = 7000;

export function nextSignalingRecoveryPollDelayMs(input: {
  attemptCount: number;
  online: boolean;
  nowMs: number;
  deadlineAtMs: number;
}): number | null {
  const remainingMs = input.deadlineAtMs - input.nowMs;
  if (remainingMs <= 0) return null;
  if (input.attemptCount === 0 && input.online) return 0;
  return Math.min(SIGNALING_RECOVERY_POLL_INTERVAL_MS, remainingMs);
}

export function isRemoteSessionEndReason(reason: string): boolean {
  const normalized = reason.trim().toLowerCase();
  return normalized === "bye" ||
    normalized === "peerremoved" ||
    normalized === "peer removed";
}

export function remoteSessionEndCode(reason: string): string | undefined {
  const normalized = reason.trim().toLowerCase();
  if (normalized === "bye") {
    return "RemoteSessionEnded (BYE)";
  }
  if (normalized === "peerremoved" || normalized === "peer removed") {
    return "RemoteSessionEnded (PeerRemoved)";
  }
  return undefined;
}

export function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

export async function disposeSessionCreatedAfterAbort(
  aborted: boolean,
  session: SessionInfo,
  stopSession: (session: SessionInfo) => Promise<boolean>,
): Promise<boolean> {
  if (!aborted) {
    return false;
  }
  try {
    await stopSession(session);
  } catch (error) {
    console.warn("Failed to stop session created after launch cancellation:", error);
  }
  return true;
}

export async function readStreamClipboardText(): Promise<string> {
  try {
    const browserClipboard = navigator.clipboard;
    if (browserClipboard?.readText) {
      const text = await browserClipboard.readText();
      if (text) {
        return text;
      }
    }
  } catch {
    // Electron main-process clipboard is the reliable fallback on Linux.
  }

  return window.openNow.readClipboardText();
}

export async function sendStreamClipboardPaste(
  client: GfnWebRtcClient | null,
): Promise<void> {
  if (!client) {
    return;
  }

  const sentOfficialPaste = await client.pasteClipboardText();
  if (sentOfficialPaste) {
    return;
  }

  try {
    const text = await readStreamClipboardText();
    if (text) {
      client.sendText(text);
    }
    return;
  } catch (error) {
    console.warn("Clipboard read failed, falling back to paste shortcut:", error);
  }

  client.sendPasteShortcut(false);
}
