import { mkdir } from "node:fs/promises";
import { dirname, join } from "node:path";
import net from "node:net";

import type {
  ActiveSessionInfo,
  CatalogBrowseRequest,
  ExistingSessionStrategy,
  GamesFetchRequest,
  IceCandidatePayload,
  KeyframeRequest,
  MainToRendererSignalingEvent,
  PingResult,
  ResolveLaunchIdRequest,
  SessionAdReportRequest,
  SessionClaimRequest,
  SessionConflictChoice,
  SessionCreateRequest,
  SessionInfo,
  SessionPollRequest,
  SessionStopRequest,
  SignalingConnectRequest,
  StreamRegion,
  SubscriptionFetchRequest,
  ThankYouDataResult,
} from "@shared/gfn";
import { serializeSessionErrorTransport } from "@shared/sessionError";
import { AuthService } from "../../src/main/gfn/auth";
import { createSession, pollSession, reportSessionAd, stopSession, getActiveSessions, claimSession } from "../../src/main/gfn/cloudmatch";
import {
  browseCatalog,
  fetchLibraryGames,
  fetchMainGames,
  fetchPublicGames,
  resolveLaunchAppId,
} from "../../src/main/gfn/games";
import { fetchDynamicRegions, fetchSubscription } from "../../src/main/gfn/subscription";
import { GfnSignalingClient } from "../../src/main/gfn/signaling";
import { isSessionError, SessionError } from "../../src/main/gfn/errorCodes";

declare function require(name: string): any;

const Service = require("webos-service");
const service = new Service("com.zortos.opennow.stable.service");
const APP_ID = "com.zortos.opennow.stable";
const DATA_DIR = join(process.env.HOME || process.env.TMPDIR || "/tmp", ".opennow");
const authService = new AuthService(join(DATA_DIR, "auth-state.json"), {
  openExternal: launchBrowser,
});

let authReady: Promise<void> | null = null;
let signalingClient: GfnSignalingClient | null = null;
let signalingClientKey: string | null = null;
const signalingSubscribers = new Set<any>();

function ensureInitialized(): Promise<void> {
  if (!authReady) {
    authReady = mkdir(dirname(join(DATA_DIR, "auth-state.json")), { recursive: true })
      .then(() => authService.initialize());
  }
  return authReady;
}

function getPayload<T>(message: any): T {
  const payload = message?.payload ?? {};
  return (payload.input ?? payload) as T;
}

function ok(message: any, result?: unknown): void {
  message.respond({
    returnValue: true,
    result,
  });
}

function fail(message: any, error: unknown): void {
  const sessionPayload = error instanceof SessionError
    ? serializeSessionErrorTransport(error.toJSON())
    : null;
  message.respond({
    returnValue: false,
    errorCode: error instanceof SessionError ? error.errorType : "OPENNOW_ERROR",
    errorText: sessionPayload ?? (error instanceof Error ? error.message : String(error)),
  });
}

function register(method: string, handler: (message: any) => Promise<unknown> | unknown): void {
  service.register(method, async (message: any) => {
    try {
      await ensureInitialized();
      ok(message, await handler(message));
    } catch (error) {
      fail(message, error);
    }
  });
}

function callLuna<T>(uri: string, params: Record<string, unknown>): Promise<T> {
  return new Promise((resolve, reject) => {
    service.call(uri, params, (response: { payload?: T }) => {
      const payload = response.payload as { returnValue?: boolean; errorText?: string; errorCode?: string | number } & T;
      if (payload?.returnValue === false) {
        reject(new Error(payload.errorText ?? `Luna call failed: ${uri}`));
        return;
      }
      resolve(payload as T);
    });
  });
}

async function launchBrowser(url: string): Promise<void> {
  const params = { target: url };
  try {
    await callLuna("luna://com.webos.applicationManager/launch", {
      id: "com.webos.app.browser",
      params,
    });
    return;
  } catch (browserError) {
    console.warn("[webOS] Failed to launch com.webos.app.browser, trying enact browser:", browserError);
  }

  await callLuna("luna://com.webos.applicationManager/launch", {
    id: "com.webos.app.enactbrowser",
    params,
  });
}

async function resolveJwt(token?: string): Promise<string> {
  return authService.resolveJwtToken(token);
}

const AUTO_RESUME_SESSION_STATUSES = new Set([2, 3]);
const ACTIVE_CREATE_SESSION_STATUSES = new Set([1, 2, 3]);

function shouldForceNewSession(strategy: ExistingSessionStrategy | undefined): boolean {
  return strategy === "force-new";
}

function isAutoResumeReadySession(entry: ActiveSessionInfo): boolean {
  return entry.serverIp != null && AUTO_RESUME_SESSION_STATUSES.has(entry.status);
}

function isActiveCreateSessionConflict(entry: ActiveSessionInfo): boolean {
  return ACTIVE_CREATE_SESSION_STATUSES.has(entry.status);
}

function selectReadySessionToClaim(activeSessions: ActiveSessionInfo[], numericAppId: number): ActiveSessionInfo | null {
  return (
    activeSessions.find((session) => isAutoResumeReadySession(session) && session.appId === numericAppId) ??
    activeSessions.find((session) => isAutoResumeReadySession(session)) ??
    null
  );
}

function selectLaunchingSession(activeSessions: ActiveSessionInfo[], numericAppId: number): ActiveSessionInfo | null {
  return (
    activeSessions.find((session) => session.serverIp && session.appId === numericAppId && session.status === 1) ??
    activeSessions.find((session) => session.serverIp && session.status === 1) ??
    null
  );
}

async function stopActiveSessionsForCreate(params: {
  token: string;
  streamingBaseUrl: string;
  zone: string;
  appId: string;
}): Promise<void> {
  const { token, streamingBaseUrl, zone } = params;
  const activeSessions = await getActiveSessions(token, streamingBaseUrl);
  for (const activeSession of activeSessions.filter(isActiveCreateSessionConflict)) {
    if (!activeSession.serverIp) {
      continue;
    }
    await stopSession({
      token,
      streamingBaseUrl,
      serverIp: activeSession.serverIp,
      zone,
      sessionId: activeSession.sessionId,
    });
  }
}

async function createOrResumeSession(payload: SessionCreateRequest): Promise<SessionInfo> {
  const token = await resolveJwt(payload.token);
  const streamingBaseUrl = payload.streamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl;
  const forceNewSession = shouldForceNewSession(payload.existingSessionStrategy);

  const tryClaimExisting = async (): Promise<SessionInfo | null> => {
    const activeSessions = await getActiveSessions(token, streamingBaseUrl);
    if (activeSessions.length === 0) {
      return null;
    }

    const numericAppId = parseInt(payload.appId, 10);
    const readyCandidate = selectReadySessionToClaim(activeSessions, numericAppId);
    if (readyCandidate) {
      return claimSession({
        token,
        streamingBaseUrl,
        sessionId: readyCandidate.sessionId,
        serverIp: readyCandidate.serverIp!,
        appId: payload.appId,
        settings: payload.settings,
      });
    }

    const launchingCandidate = selectLaunchingSession(activeSessions, numericAppId);
    if (!launchingCandidate) {
      return null;
    }

    try {
      return await pollSession({
        token,
        streamingBaseUrl,
        serverIp: launchingCandidate.serverIp!,
        zone: payload.zone,
        sessionId: launchingCandidate.sessionId,
      });
    } catch {
      return {
        sessionId: launchingCandidate.sessionId,
        status: 1,
        zone: payload.zone,
        streamingBaseUrl,
        serverIp: launchingCandidate.serverIp!,
        signalingServer: launchingCandidate.serverIp!,
        signalingUrl: launchingCandidate.signalingUrl ?? `wss://${launchingCandidate.serverIp}:443/nvst/`,
        iceServers: [],
      };
    }
  };

  if (!forceNewSession) {
    const existing = await tryClaimExisting();
    if (existing) {
      return existing;
    }
  }

  try {
    if (forceNewSession) {
      await stopActiveSessionsForCreate({
        token,
        streamingBaseUrl,
        zone: payload.zone,
        appId: payload.appId,
      });
    }
    return await createSession({ ...payload, token, streamingBaseUrl });
  } catch (error) {
    if (!forceNewSession && error instanceof SessionError && error.statusCode === 11) {
      const existing = await tryClaimExisting();
      if (existing) {
        return existing;
      }
    }
    throw error;
  }
}

async function tcpPing(hostname: string, port: number, timeoutMs = 3000): Promise<number | null> {
  return new Promise((resolve) => {
    const startTime = Date.now();
    const socket = new net.Socket();
    socket.setTimeout(timeoutMs);
    socket.once("connect", () => {
      const pingMs = Date.now() - startTime;
      socket.destroy();
      resolve(pingMs);
    });
    socket.once("timeout", () => {
      socket.destroy();
      resolve(null);
    });
    socket.once("error", () => {
      socket.destroy();
      resolve(null);
    });
    socket.connect(port, hostname);
  });
}

async function pingRegions(regions: StreamRegion[]): Promise<PingResult[]> {
  return Promise.all(regions.map(async (region) => {
    try {
      const url = new URL(region.url);
      const hostname = url.hostname;
      const port = url.protocol === "https:" ? 443 : 80;
      const validPings: number[] = [];
      await tcpPing(hostname, port, 3000);
      for (let i = 0; i < 3; i++) {
        if (i > 0) {
          await new Promise<void>((resolve) => setTimeout(resolve, 100));
        }
        const pingMs = await tcpPing(hostname, port, 3000);
        if (pingMs !== null) {
          validPings.push(pingMs);
        }
      }
      if (validPings.length === 0) {
        return { url: region.url, pingMs: null, error: "All ping tests failed" };
      }
      return {
        url: region.url,
        pingMs: Math.round(validPings.reduce((sum, value) => sum + value, 0) / validPings.length),
      };
    } catch {
      return { url: region.url, pingMs: null, error: "Invalid URL" };
    }
  }));
}

async function fetchPrintedWasteJson<T>(url: string): Promise<T> {
  const response = await fetch(url, {
    headers: {
      "User-Agent": "opennow-webos/0.1",
      Accept: "application/json",
    },
  });
  if (!response.ok) {
    throw new Error(`PrintedWaste request failed (${response.status})`);
  }
  const payload = await response.json() as { status?: boolean; data?: T };
  if (payload.status === false || payload.data == null) {
    throw new Error("PrintedWaste returned an invalid response");
  }
  return payload.data;
}

function emitSignalingEvent(event: MainToRendererSignalingEvent): void {
  for (const subscriber of signalingSubscribers) {
    subscriber.respond({
      returnValue: true,
      event,
      result: event,
      subscribed: true,
    });
  }
}

register("getAuthSession", (message) => {
  const payload = getPayload<{ forceRefresh?: boolean }>(message);
  return authService.ensureValidSessionWithStatus(Boolean(payload.forceRefresh));
});
register("getLoginProviders", () => authService.getProviders());
register("getRegions", (message) => authService.getRegions(getPayload<{ token?: string }>(message).token));
register("login", (message) => authService.login(getPayload(message)));
register("logout", () => authService.logout());
register("fetchSubscription", async (message) => {
  const payload = getPayload<SubscriptionFetchRequest>(message);
  const token = await resolveJwt(payload.token);
  const streamingBaseUrl = payload.providerStreamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl;
  const { vpcId } = await fetchDynamicRegions(token, streamingBaseUrl);
  return fetchSubscription(token, payload.userId, vpcId ?? undefined);
});
register("fetchMainGames", async (message) => {
  const payload = getPayload<GamesFetchRequest>(message);
  return fetchMainGames(await resolveJwt(payload.token), payload.providerStreamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl);
});
register("fetchLibraryGames", async (message) => {
  const payload = getPayload<GamesFetchRequest>(message);
  return fetchLibraryGames(await resolveJwt(payload.token), payload.providerStreamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl);
});
register("browseCatalog", async (message) => {
  const payload = getPayload<CatalogBrowseRequest>(message);
  const token = await resolveJwt(payload.token);
  const streamingBaseUrl = payload.providerStreamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl;
  return browseCatalog({ ...payload, token, providerStreamingBaseUrl: streamingBaseUrl });
});
register("fetchPublicGames", () => fetchPublicGames());
register("resolveLaunchAppId", async (message) => {
  const payload = getPayload<ResolveLaunchIdRequest>(message);
  return resolveLaunchAppId(
    await resolveJwt(payload.token),
    payload.appIdOrUuid,
    payload.providerStreamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl,
  );
});
register("createSession", (message) => createOrResumeSession(getPayload<SessionCreateRequest>(message)));
register("pollSession", async (message) => {
  const payload = getPayload<SessionPollRequest>(message);
  return pollSession({
    ...payload,
    token: await resolveJwt(payload.token),
    streamingBaseUrl: payload.streamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl,
  });
});
register("reportSessionAd", async (message) => {
  const payload = getPayload<SessionAdReportRequest>(message);
  return reportSessionAd({
    ...payload,
    token: await resolveJwt(payload.token),
    streamingBaseUrl: payload.streamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl,
  });
});
register("stopSession", async (message) => {
  const payload = getPayload<SessionStopRequest>(message);
  return stopSession({
    ...payload,
    token: await resolveJwt(payload.token),
    streamingBaseUrl: payload.streamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl,
  });
});
register("getActiveSessions", async (message) => {
  const payload = getPayload<{ token?: string; streamingBaseUrl?: string }>(message);
  return getActiveSessions(
    await resolveJwt(payload.token),
    payload.streamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl,
  );
});
register("claimSession", async (message) => {
  const payload = getPayload<SessionClaimRequest>(message);
  return claimSession({
    ...payload,
    token: await resolveJwt(payload.token),
    streamingBaseUrl: payload.streamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl,
  });
});
register("connectSignaling", async (message) => {
  const payload = getPayload<SignalingConnectRequest>(message);
  const nextKey = `${payload.sessionId}|${payload.signalingServer}|${payload.signalingUrl ?? ""}`;
  if (signalingClient && signalingClientKey === nextKey) {
    return;
  }
  signalingClient?.disconnect();
  signalingClient = new GfnSignalingClient(payload.signalingServer, payload.sessionId, payload.signalingUrl);
  signalingClientKey = nextKey;
  signalingClient.onEvent(emitSignalingEvent);
  await signalingClient.connect();
});
register("disconnectSignaling", () => {
  signalingClient?.disconnect();
  signalingClient = null;
  signalingClientKey = null;
});
register("sendAnswer", (message) => {
  if (!signalingClient) {
    throw new Error("Signaling is not connected");
  }
  return signalingClient.sendAnswer(getPayload(message));
});
register("sendIceCandidate", (message) => {
  if (!signalingClient) {
    throw new Error("Signaling is not connected");
  }
  return signalingClient.sendIceCandidate(getPayload<IceCandidatePayload>(message));
});
register("requestKeyframe", (message) => {
  if (!signalingClient) {
    throw new Error("Signaling is not connected");
  }
  return signalingClient.requestKeyframe(getPayload<KeyframeRequest>(message));
});
register("pingRegions", (message) => pingRegions(getPayload<StreamRegion[]>(message)));
register("fetchPrintedWasteQueue", () => fetchPrintedWasteJson("https://api.printedwaste.com/gfn/queue/"));
register("fetchPrintedWasteServerMapping", () => fetchPrintedWasteJson("https://remote.printedwaste.com/config/GFN_SERVERID_TO_REGION_MAPPING"));
register("getThanksData", (): ThankYouDataResult => ({
  contributors: [],
  supporters: [],
  contributorsError: "Community data is not loaded on webOS.",
}));
register("showSessionConflictDialog", (): SessionConflictChoice => "resume");

service.register("signalingEvents", (message: any) => {
  signalingSubscribers.add(message);
  message.respond({
    returnValue: true,
    subscribed: true,
  });
});

process.on("uncaughtException", (error) => {
  console.error("[OpenNOW webOS service] uncaught exception", error);
});
process.on("unhandledRejection", (error) => {
  console.error("[OpenNOW webOS service] unhandled rejection", error);
});
