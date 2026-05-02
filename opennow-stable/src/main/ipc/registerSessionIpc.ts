import { IPC_CHANNELS } from "@shared/ipc";
import type {
  SessionAdReportRequest,
  SessionClaimRequest,
  SessionCreateRequest,
  SessionPollRequest,
  SessionStopRequest,
  SessionInfo,
} from "@shared/gfn";
import type { MainIpcDeps } from "./types";
import { createSession, pollSession, reportSessionAd, stopSession, getActiveSessions, claimSession } from "../gfn/cloudmatch";
import { SessionError } from "../gfn/errorCodes";
import { setActivity, clearActivity } from "../discordRpc";

export function registerSessionIpc(deps: MainIpcDeps): void {
  const { ipcMain, authService, settingsManager } = deps;

  ipcMain.handle(IPC_CHANNELS.CREATE_SESSION, async (_event, payload: SessionCreateRequest) => {
    const token = await deps.resolveJwt(payload.token);
    const streamingBaseUrl = payload.streamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl;
    const forceNewSession = deps.shouldForceNewSession(payload.existingSessionStrategy);

    const tryClaimExisting = async (): Promise<SessionInfo | null> => {
      if (!token) return null;
      try {
        const activeSessions = await getActiveSessions(token, streamingBaseUrl);
        if (activeSessions.length === 0) return null;
        const numericAppId = parseInt(payload.appId, 10);

        const readyCandidate = deps.selectReadySessionToClaim(activeSessions, numericAppId);
        if (readyCandidate) {
          console.log(
            `[CreateSession] Resuming existing session (id=${readyCandidate.sessionId}, appId=${readyCandidate.appId}, status=${readyCandidate.status}) instead of creating new.`,
          );
          return claimSession({
            token,
            streamingBaseUrl,
            sessionId: readyCandidate.sessionId,
            serverIp: readyCandidate.serverIp!,
            appId: payload.appId,
            settings: payload.settings,
          });
        }

        const launchingCandidate = deps.selectLaunchingSession(activeSessions, numericAppId);
        if (launchingCandidate) {
          console.log(
            `[CreateSession] Found launching session (id=${launchingCandidate.sessionId}, appId=${launchingCandidate.appId}, status=1); returning for renderer queue/ad polling.`,
          );
          try {
            return await pollSession({
              token,
              streamingBaseUrl,
              serverIp: launchingCandidate.serverIp!,
              zone: payload.zone,
              sessionId: launchingCandidate.sessionId,
            });
          } catch (hydrateError) {
            console.warn(
              `[CreateSession] Failed to hydrate launching session ${launchingCandidate.sessionId}; falling back to minimal handoff:`,
              hydrateError,
            );
            return {
              sessionId: launchingCandidate.sessionId,
              status: 1,
              zone: payload.zone,
              streamingBaseUrl,
              serverIp: launchingCandidate.serverIp!,
              signalingServer: launchingCandidate.serverIp!,
              signalingUrl: launchingCandidate.signalingUrl ?? `wss://${launchingCandidate.serverIp}:443/nvst/`,
              iceServers: [],
            } satisfies SessionInfo;
          }
        }

        return null;
      } catch (claimError) {
        console.warn("[CreateSession] Failed to claim existing session:", claimError);
        return null;
      }
    };

    if (!forceNewSession) {
      const preChecked = await tryClaimExisting();
      if (preChecked) {
        if (settingsManager.get("discordRichPresence")) {
          void setActivity(payload.internalTitle || payload.appId, new Date(), payload.appId);
        }
        return preChecked;
      }
    }

    try {
      if (forceNewSession && token) {
        await deps.stopActiveSessionsForCreate({
          token,
          streamingBaseUrl,
          zone: payload.zone,
          appId: payload.appId,
        });
      }
      const sessionResult = await createSession({ ...payload, token, streamingBaseUrl });
      if (settingsManager.get("discordRichPresence")) {
        void setActivity(payload.internalTitle || payload.appId, new Date(), payload.appId);
      }
      return sessionResult;
    } catch (error) {
      if (!forceNewSession && error instanceof SessionError && error.statusCode === 11) {
        console.warn("[CreateSession] SESSION_LIMIT_EXCEEDED — retrying as session claim.");
        const fallback = await tryClaimExisting();
        if (fallback) {
          if (settingsManager.get("discordRichPresence")) {
            void setActivity(payload.internalTitle || payload.appId, new Date(), payload.appId);
          }
          return fallback;
        }
      }
      deps.rethrowSerializedSessionError(error);
    }
  });

  ipcMain.handle(IPC_CHANNELS.POLL_SESSION, async (_event, payload: SessionPollRequest) => {
    try {
      const token = await deps.resolveJwt(payload.token);
      return pollSession({
        ...payload,
        token,
        streamingBaseUrl: payload.streamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl,
      });
    } catch (error) {
      deps.rethrowSerializedSessionError(error);
    }
  });

  ipcMain.handle(IPC_CHANNELS.REPORT_SESSION_AD, async (_event, payload: SessionAdReportRequest) => {
    try {
      const token = await deps.resolveJwt(payload.token);
      return reportSessionAd({
        ...payload,
        token,
        streamingBaseUrl: payload.streamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl,
      });
    } catch (error) {
      deps.rethrowSerializedSessionError(error);
    }
  });

  ipcMain.handle(IPC_CHANNELS.STOP_SESSION, async (_event, payload: SessionStopRequest) => {
    try {
      const token = await deps.resolveJwt(payload.token);
      const result = await stopSession({
        ...payload,
        token,
        streamingBaseUrl: payload.streamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl,
      });
      void clearActivity();
      return result;
    } catch (error) {
      deps.rethrowSerializedSessionError(error);
    }
  });

  ipcMain.handle(IPC_CHANNELS.GET_ACTIVE_SESSIONS, async (_event, token?: string, streamingBaseUrl?: string) => {
    const jwt = await deps.resolveJwt(token);
    const baseUrl = streamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl;
    return getActiveSessions(jwt, baseUrl);
  });

  ipcMain.handle(IPC_CHANNELS.DISCORD_CLEAR_ACTIVITY, async () => {
    void clearActivity();
  });

  ipcMain.handle(IPC_CHANNELS.CLAIM_SESSION, async (_event, payload: SessionClaimRequest) => {
    try {
      const token = await deps.resolveJwt(payload.token);
      const streamingBaseUrl = payload.streamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl;
      return claimSession({
        ...payload,
        token,
        streamingBaseUrl,
      });
    } catch (error) {
      deps.rethrowSerializedSessionError(error);
    }
  });

  ipcMain.handle(IPC_CHANNELS.SESSION_CONFLICT_DIALOG, async (): Promise<import("@shared/gfn").SessionConflictChoice> => {
    return deps.showSessionConflictDialog();
  });
}
