import type { IpcMain } from "electron";
import { IPC_CHANNELS } from "@shared/ipc";
import type {
  SessionAdReportRequest,
  SessionClaimRequest,
  SessionConflictChoice,
  SessionCreateRequest,
  SessionInfo,
  SessionPollRequest,
  SessionStopRequest,
} from "@shared/gfn";
import { formatErrorChainForLog } from "@shared/networkError";
import type { AuthService } from "../gfn/auth";
import {
  claimSession,
  createSession,
  getActiveSessions,
  pollSession,
  reportSessionAd,
  stopSession,
} from "../gfn/cloudmatch";
import { SessionError } from "../gfn/errorCodes";
import type { SettingsManager } from "../settings";
import {
  rethrowSerializedSessionError,
  showSessionConflictDialog,
  type SessionConflictDialogDeps,
} from "../session/sessionConflict";
import {
  resolveSessionCloudGsyncSettings,
  shouldForceNewSession,
} from "../session/cloudGsyncSettings";
import { stopActiveSessionsForCreate } from "../session/sessionLifecycle";
import {
  selectLaunchingSession,
  selectReadySessionToClaim,
} from "../session/sessionSelection";

export interface SessionIpcHandlerDeps extends SessionConflictDialogDeps {
  ipcMain: IpcMain;
  authService: AuthService;
  settingsManager: SettingsManager;
  resolveJwt(token?: string): Promise<string>;
  setActivity(gameName: string, startTimestamp: Date, appId?: string): Promise<void>;
  clearActivity(): Promise<void>;
}

export function registerSessionIpcHandlers(deps: SessionIpcHandlerDeps): void {
  const {
    ipcMain,
    authService,
    settingsManager,
    resolveJwt,
    setActivity,
    clearActivity,
  } = deps;

  ipcMain.handle(
    IPC_CHANNELS.CREATE_SESSION,
    async (_event, payload: SessionCreateRequest) => {
      const token = await resolveJwt(payload.token);
      const streamingBaseUrl =
        payload.streamingBaseUrl ??
        authService.getSelectedProvider().streamingServiceUrl;
      const forceNewSession = shouldForceNewSession(
        payload.existingSessionStrategy,
      );
      const resolvedSettings = await resolveSessionCloudGsyncSettings(
        payload.settings,
      );
      const resolvedPayload: SessionCreateRequest = {
        ...payload,
        settings: resolvedSettings,
      };

      const tryClaimExisting = async (): Promise<SessionInfo | null> => {
        if (!token) return null;
        try {
          const activeSessions = await getActiveSessions(
            token,
            streamingBaseUrl,
          );
          if (activeSessions.length === 0) return null;
          const numericAppId = parseInt(resolvedPayload.appId, 10);

          const readyCandidate = selectReadySessionToClaim(
            activeSessions,
            numericAppId,
          );
          if (readyCandidate) {
            console.log(
              `[CreateSession] Resuming existing session (id=${readyCandidate.sessionId}, appId=${readyCandidate.appId}, status=${readyCandidate.status}) instead of creating new.`,
            );
            return claimSession({
              token,
              streamingBaseUrl,
              sessionId: readyCandidate.sessionId,
              serverIp: readyCandidate.serverIp!,
              appId: resolvedPayload.appId,
              settings: resolvedPayload.settings,
            });
          }

          const launchingCandidate = selectLaunchingSession(
            activeSessions,
            numericAppId,
          );
          if (launchingCandidate) {
            console.log(
              `[CreateSession] Found launching session (id=${launchingCandidate.sessionId}, appId=${launchingCandidate.appId}, status=1); returning for renderer queue/ad polling.`,
            );
            try {
              return await pollSession({
                token,
                streamingBaseUrl,
                serverIp: launchingCandidate.serverIp!,
                zone: resolvedPayload.zone,
                sessionId: launchingCandidate.sessionId,
                proxyUrl: payload.proxyUrl,
              });
            } catch (hydrateError) {
              console.warn(
                `[CreateSession] Failed to hydrate launching session ${launchingCandidate.sessionId}; falling back to minimal handoff: ${formatErrorChainForLog(hydrateError)}`,
              );
              return {
                sessionId: launchingCandidate.sessionId,
                status: 1,
                zone: resolvedPayload.zone,
                streamingBaseUrl,
                serverIp: launchingCandidate.serverIp!,
                signalingServer: launchingCandidate.serverIp!,
                signalingUrl:
                  launchingCandidate.signalingUrl ??
                  `wss://${launchingCandidate.serverIp}:443/nvst/`,
                iceServers: [],
              } satisfies SessionInfo;
            }
          }

          return null;
        } catch (claimError) {
          console.warn(
            `[CreateSession] Failed to claim existing session: ${formatErrorChainForLog(claimError)}`,
          );
          return null;
        }
      };

      if (!forceNewSession) {
        const preChecked = await tryClaimExisting();
        if (preChecked) {
          if (settingsManager.get("discordRichPresence")) {
            void setActivity(
              payload.internalTitle || payload.appId,
              new Date(),
              payload.appId,
            );
          }
          return preChecked;
        }
      }

      try {
        if (forceNewSession && token) {
          await stopActiveSessionsForCreate({
            token,
            streamingBaseUrl,
            zone: resolvedPayload.zone,
            appId: resolvedPayload.appId,
          });
        }
        const sessionResult = await createSession({
          ...resolvedPayload,
          token,
          streamingBaseUrl,
        });
        if (settingsManager.get("discordRichPresence")) {
          void setActivity(
            payload.internalTitle || payload.appId,
            new Date(),
            payload.appId,
          );
        }
        return sessionResult;
      } catch (error) {
        if (
          !forceNewSession &&
          error instanceof SessionError &&
          error.statusCode === 11
        ) {
          console.warn(
            "[CreateSession] SESSION_LIMIT_EXCEEDED — retrying as session claim.",
          );
          const fallback = await tryClaimExisting();
          if (fallback) {
            if (settingsManager.get("discordRichPresence")) {
              void setActivity(
                payload.internalTitle || payload.appId,
                new Date(),
                payload.appId,
              );
            }
            return fallback;
          }
        }
        rethrowSerializedSessionError(error);
      }
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.POLL_SESSION,
    async (_event, payload: SessionPollRequest) => {
      try {
        const token = await resolveJwt(payload.token);
        return pollSession({
          ...payload,
          token,
          streamingBaseUrl:
            payload.streamingBaseUrl ??
            authService.getSelectedProvider().streamingServiceUrl,
        });
      } catch (error) {
        rethrowSerializedSessionError(error);
      }
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.REPORT_SESSION_AD,
    async (_event, payload: SessionAdReportRequest) => {
      try {
        const token = await resolveJwt(payload.token);
        return reportSessionAd({
          ...payload,
          token,
          streamingBaseUrl:
            payload.streamingBaseUrl ??
            authService.getSelectedProvider().streamingServiceUrl,
        });
      } catch (error) {
        rethrowSerializedSessionError(error);
      }
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.STOP_SESSION,
    async (_event, payload: SessionStopRequest) => {
      try {
        const token = await resolveJwt(payload.token);
        const result = await stopSession({
          ...payload,
          token,
          streamingBaseUrl:
            payload.streamingBaseUrl ??
            authService.getSelectedProvider().streamingServiceUrl,
        });
        void clearActivity();
        return result;
      } catch (error) {
        rethrowSerializedSessionError(error);
      }
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.GET_ACTIVE_SESSIONS,
    async (_event, token?: string, streamingBaseUrl?: string) => {
      const jwt = await resolveJwt(token);
      const baseUrl =
        streamingBaseUrl ??
        authService.getSelectedProvider().streamingServiceUrl;
      return getActiveSessions(jwt, baseUrl);
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.CLAIM_SESSION,
    async (_event, payload: SessionClaimRequest) => {
      try {
        const token = await resolveJwt(payload.token);
        const streamingBaseUrl =
          payload.streamingBaseUrl ??
          authService.getSelectedProvider().streamingServiceUrl;
        const resolvedSettings = payload.settings
          ? await resolveSessionCloudGsyncSettings(payload.settings)
          : undefined;
        return claimSession({
          ...payload,
          token,
          streamingBaseUrl,
          settings: resolvedSettings,
        });
      } catch (error) {
        rethrowSerializedSessionError(error);
      }
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.SESSION_CONFLICT_DIALOG,
    async (): Promise<SessionConflictChoice> => {
      return showSessionConflictDialog(deps);
    },
  );
}
