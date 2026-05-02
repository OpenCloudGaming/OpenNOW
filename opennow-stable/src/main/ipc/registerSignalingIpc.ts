import { IPC_CHANNELS } from "@shared/ipc";
import type { IceCandidatePayload, KeyframeRequest, SendAnswerRequest, SignalingConnectRequest } from "@shared/gfn";
import type { MainIpcDeps } from "./types";

export function registerSignalingIpc(deps: MainIpcDeps): void {
  const { ipcMain, signaling, GfnSignalingClient } = deps;

  ipcMain.handle(IPC_CHANNELS.CONNECT_SIGNALING, async (_event, payload: SignalingConnectRequest): Promise<void> => {
    const nextKey = `${payload.sessionId}|${payload.signalingServer}|${payload.signalingUrl ?? ""}`;
    if (signaling.client && signaling.key === nextKey) {
      console.log("[Signaling] Reuse existing signaling connection (duplicate connect request ignored)");
      return;
    }

    if (signaling.client) {
      signaling.client.disconnect();
    }

    signaling.client = new GfnSignalingClient(payload.signalingServer, payload.sessionId, payload.signalingUrl);
    signaling.key = nextKey;
    signaling.client.onEvent(deps.emitToRenderer);
    await signaling.client.connect();
  });

  ipcMain.handle(IPC_CHANNELS.DISCONNECT_SIGNALING, async (): Promise<void> => {
    signaling.client?.disconnect();
    signaling.client = null;
    signaling.key = null;
  });

  ipcMain.handle(IPC_CHANNELS.SEND_ANSWER, async (_event, payload: SendAnswerRequest) => {
    if (!signaling.client) {
      throw new Error("Signaling is not connected");
    }
    return signaling.client.sendAnswer(payload);
  });

  ipcMain.handle(IPC_CHANNELS.SEND_ICE_CANDIDATE, async (_event, payload: IceCandidatePayload) => {
    if (!signaling.client) {
      throw new Error("Signaling is not connected");
    }
    return signaling.client.sendIceCandidate(payload);
  });

  ipcMain.handle(IPC_CHANNELS.REQUEST_KEYFRAME, async (_event, payload: KeyframeRequest) => {
    if (!signaling.client) {
      throw new Error("Signaling is not connected");
    }
    return signaling.client.requestKeyframe(payload);
  });
}
