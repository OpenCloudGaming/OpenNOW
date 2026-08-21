import type {
  ActiveSessionInfo,
  SessionAdAction,
  SessionAdReportRequest,
  SessionClaimRequest,
  SessionCreateRequest,
  SessionInfo,
  SessionPollRequest,
  SessionStopRequest,
} from "@shared/gfn";

import {
  DEFAULT_KEYBOARD_LAYOUT,
  resolveGfnKeyboardLayout,
} from "@shared/gfn";

import type { CloudMatchResponse, GetSessionsResponse } from "./types";
import { SessionError } from "./errorCodes";
import {
  buildGfnCloudMatchClaimHeaders,
  buildGfnCloudMatchHeaders,
  LCARS_CLIENT_ID,
} from "./clientHeaders";
import { getCloudMatchDeviceHashId } from "./deviceId";
import {
  readCloudMatchJson,
  throwIfCloudMatchResponseError,
} from "./request";
import { appLaunchModeWireValue } from "./cloudmatchFeatures";
import {
  extractServerInfoRegionBases,
  fetchCloudMatch,
  formatErrorForLog,
  isZoneHostname,
  normalizeTrustedCloudMatchBaseUrl,
  resolveCreateSessionBase,
  resolvePollStopBase,
  resolveStreamingBaseUrl,
  type CloudMatchServerInfoResponse,
} from "./cloudmatchTransport";
import {
  isReadySessionStatus,
  normalizeIceServers,
  resolveSignaling,
  streamingServerIp,
} from "./cloudmatchSignaling";
import {
  buildClaimRequestBody,
  buildSessionRequestBody,
} from "./cloudmatchSessionRequest";
import {
  echoedSessionAppLaunchMode,
  extractAdState,
  extractNegotiatedStreamProfile,
  extractQueuePosition,
  extractSessionQueuePosition,
  extractSessionSeatSetupStep,
  normalizeStreamingFeatures,
  toSessionInfo,
} from "./cloudmatchSessionParsing";

export {
  appLaunchModeWireValue,
  buildRequestedStreamingFeatures,
  resolveRequestedCodecWireValue,
  shouldEnableInGameSettingsPersistence,
  shouldRequestReflex,
} from "./cloudmatchFeatures";
export { extractServerInfoRegionBases } from "./cloudmatchTransport";

const SESSION_MODIFY_ACTION_AD_UPDATE = 6;
const SESSION_MODIFY_ACTION_RESUME = 2;

const AD_ACTION_CODES: Record<SessionAdAction, number> = {
  start: 1,
  pause: 2,
  resume: 3,
  finish: 4,
  cancel: 5,
};

export async function createSession(input: SessionCreateRequest): Promise<SessionInfo> {
  if (!input.token) {
    throw new Error("Missing token for session creation");
  }

  if (!/^\d+$/.test(input.appId)) {
    throw new Error(`Invalid launch appId '${input.appId}' (must be numeric)`);
  }

  // Generate client/device IDs once for the entire session lifecycle
  const clientId = LCARS_CLIENT_ID;
  const deviceId = getCloudMatchDeviceHashId();

  const requestedBase = resolveStreamingBaseUrl(input.zone, input.streamingBaseUrl);
  const base = await resolveCreateSessionBase(
    requestedBase,
    input.token,
    clientId,
    deviceId,
    input.proxyUrl,
    { preferRegionalHost: input.settings.transportMode === "nvst" },
  );
  // Official Bifrost create sends networkTestSessionId: null and does not POST /v2/nettestsession.
  const body = buildSessionRequestBody(input, deviceId, null);
  const request = body.sessionRequestData;
  console.log(
    `[CloudMatch] createSession in-game settings persistence: user=${input.enablePersistingInGameSettings === true}, ` +
    `gameSupport=${input.supportsInGameSettingsPersistence === true}, ` +
    `sent=${request.enablePersistingInGameSettings}, ` +
    `networkTestSessionId=${request.networkTestSessionId ?? "null"}`,
  );
  console.log(
    `[CloudMatch] createSession identity: platform=${request.clientPlatformName}, ` +
    `sdk=${request.sdkVersion}, streamer=${request.streamerVersion}, ` +
    `enhanced=${request.enhancedStreamMode}, controllers=${JSON.stringify(request.availableSupportedControllers)}, ` +
    `audioFormat=${request.requestedAudioFormat ?? "absent"}, partnerCustomData=${request.partnerCustomData === null ? "null" : "set"}`,
  );

  const keyboardLayout = resolveGfnKeyboardLayout(input.settings.keyboardLayout ?? DEFAULT_KEYBOARD_LAYOUT, process.platform);
  const languageCode = input.settings.gameLanguage ?? "en_US";
  const url = `${base}/v2/session?${new URLSearchParams({ keyboardLayout, languageCode }).toString()}`;
  console.log(
    `[CloudMatch] createSession POST ${url} resolution=${input.settings.resolution} fps=${input.settings.fps} ` +
    `colorQuality=${input.settings.colorQuality} bitDepth=${request.requestedStreamingFeatures.bitDepth} ` +
    `reflex=${request.requestedStreamingFeatures.reflex} deviceHashId=${deviceId.slice(0, 12)}…`,
  );
  const response = await fetchCloudMatch(url, {
    method: "POST",
    headers: buildGfnCloudMatchHeaders({ token: input.token, clientId, deviceId, includeOrigin: false }),
    body: JSON.stringify(body),
  }, { proxyUrl: input.proxyUrl });

  const { payload } = await readCloudMatchJson<CloudMatchResponse>(response);

  // Official Bifrost follows every fresh create with an immediate
  // PUT action=2 RESUME carrying the same full sessionRequestData (fresh
  // SubSessionId). That explicit RESUME starts seat setup on the modern hex
  // ping-hash streamer pool; sessions left to auto-start from POST alone land
  // on the legacy literal-"PING" pool that never completes NVST hole-punch.
  if (input.settings.transportMode === "nvst" && payload.session?.sessionId) {
    await resumeFreshNvstSession({
      base,
      sessionId: payload.session.sessionId,
      input,
      clientId,
      deviceId,
      keyboardLayout,
      languageCode,
    });
  }

  return await toSessionInfo({
    zone: input.zone,
    streamingBaseUrl: base,
    payload,
    clientId,
    deviceId,
    fallbackAppId: input.appId,
    fallbackAppLaunchMode: appLaunchModeWireValue(input.settings.appLaunchMode),
  });
}

/**
 * Official fresh-create parity for the classic (NVST) streamer: immediately
 * RESUME the just-created session with the full sessionRequestData. Failures
 * are logged and ignored — the caller still polls to readiness as before.
 */
async function resumeFreshNvstSession(args: {
  base: string;
  sessionId: string;
  input: SessionCreateRequest;
  clientId: string;
  deviceId: string;
  keyboardLayout: string;
  languageCode: string;
}): Promise<void> {
  const { base, sessionId, input, clientId, deviceId, keyboardLayout, languageCode } = args;
  // Rebuild the body so SubSessionId is fresh, mirroring the official client.
  const resumeBody = buildSessionRequestBody(input, deviceId, null);
  const payload = {
    action: SESSION_MODIFY_ACTION_RESUME,
    data: "RESUME",
    sessionRequestData: resumeBody.sessionRequestData,
    metaData: null,
    adUpdates: null,
  };
  const url = `${base}/v2/session/${sessionId}?${new URLSearchParams({ keyboardLayout, languageCode }).toString()}`;
  console.log(`[CloudMatch] createSession RESUME PUT ${url} (official fresh-create parity)`);
  try {
    const response = await fetchCloudMatch(url, {
      method: "PUT",
      headers: buildGfnCloudMatchHeaders({ token: input.token as string, clientId, deviceId, includeOrigin: false }),
      body: JSON.stringify(payload),
    }, { proxyUrl: input.proxyUrl });
    const text = await response.text();
    let statusCode = -1;
    try {
      statusCode = (JSON.parse(text) as CloudMatchResponse).requestStatus?.statusCode ?? -1;
    } catch {
      // Keep -1 for unparsable bodies; the warning below carries the HTTP status.
    }
    console.log(`[CloudMatch] createSession RESUME response: HTTP ${response.status}, requestStatus=${statusCode}`);
    if (!response.ok || statusCode !== 1) {
      console.warn(
        `[CloudMatch] createSession RESUME not accepted (HTTP ${response.status}, status=${statusCode}); continuing with poll-based setup`,
      );
    }
  } catch (error) {
    console.warn(`[CloudMatch] createSession RESUME failed: ${formatErrorForLog(error)}; continuing with poll-based setup`);
  }
}

export async function pollSession(input: SessionPollRequest): Promise<SessionInfo> {  if (!input.token) {
    throw new Error("Missing token for session polling");
  }

  // Use provided client/device IDs if available (should match session creation)
  const clientId = input.clientId ?? LCARS_CLIENT_ID;
  const deviceId = input.deviceId ?? getCloudMatchDeviceHashId();

  const base = resolvePollStopBase(input.zone, input.streamingBaseUrl, input.serverIp);
  const baseHost = new URL(base).hostname;
  const pollProxyUrl = isZoneHostname(baseHost) ? input.proxyUrl : undefined;
  const url = `${base}/v2/session/${input.sessionId}`;
  // Polling should NOT include Origin/Referer headers (matches claimSession polling pattern)
  const headers = buildGfnCloudMatchHeaders({ token: input.token, clientId, deviceId, includeOrigin: false });
  const response = await fetchCloudMatch(url, {
    method: "GET",
    headers,
  }, { proxyUrl: pollProxyUrl });

  const { payload } = await readCloudMatchJson<CloudMatchResponse>(response);

  // Match Rust behavior: if the poll was routed through the zone load balancer
  // and the response now contains a real server IP in connectionInfo, re-poll
  // directly via the real server IP. This ensures the signaling data and
  // connection info are correct (the zone LB may return different data than
  // a direct server poll).
  const realServerIp = streamingServerIp(payload);
  const polledViaZone = isZoneHostname(baseHost);
  const realIpDiffers =
    realServerIp &&
    realServerIp.length > 0 &&
    !isZoneHostname(realServerIp) &&
    realServerIp !== input.serverIp;

  if (polledViaZone && realIpDiffers && isReadySessionStatus(payload.session.status)) {
    // Session is ready and we now know the real server IP — re-poll directly
    console.log(
      `[CloudMatch] Session ready: re-polling via real server IP ${realServerIp} (was: ${baseHost})`,
    );
    const directBase = `https://${realServerIp}`;
    const directUrl = `${directBase}/v2/session/${input.sessionId}`;
    try {
      // The ready-session direct real-IP re-poll intentionally bypasses the session proxy.
      const directResponse = await fetchCloudMatch(directUrl, {
        method: "GET",
        headers,
      });
      if (directResponse.ok) {
        const directText = await directResponse.text();
        const directPayload = JSON.parse(directText) as CloudMatchResponse;
        if (directPayload.requestStatus.statusCode === 1) {
          console.log("[CloudMatch] Direct re-poll succeeded, using direct response for signaling info");
          return await toSessionInfo({ zone: input.zone, streamingBaseUrl: directBase, payload: directPayload, clientId, deviceId });
        }
      }
    } catch (e) {
      // Direct poll failed — fall through to use the original zone LB response
      console.warn("[CloudMatch] Direct re-poll failed, using zone LB response:", e);
    }
  }

  return await toSessionInfo({ zone: input.zone, streamingBaseUrl: base, payload, clientId, deviceId });
}

export async function reportSessionAd(input: SessionAdReportRequest): Promise<SessionInfo> {
  if (!input.token) {
    throw new Error("Missing token for ad update");
  }

  const clientId = input.clientId ?? LCARS_CLIENT_ID;
  const deviceId = input.deviceId ?? getCloudMatchDeviceHashId();
  const base = resolvePollStopBase(input.zone, input.streamingBaseUrl, input.serverIp);
  const url = `${base}/v2/session/${input.sessionId}`;
  const clientTimestamp = input.clientTimestamp ?? Math.floor(Date.now() / 1000);
  const adUpdate = {
    adId: input.adId,
    adAction: AD_ACTION_CODES[input.action],
    clientTimestamp,
    ...(typeof input.watchedTimeInMs === "number"
      ? { watchedTimeInMs: Math.max(0, Math.round(input.watchedTimeInMs)) }
      : {}),
    ...(typeof input.pausedTimeInMs === "number"
      ? { pausedTimeInMs: Math.max(0, Math.round(input.pausedTimeInMs)) }
      : {}),
    ...(input.cancelReason ? { cancelReason: input.cancelReason } : {}),
  };
  const requestBody = {
    action: SESSION_MODIFY_ACTION_AD_UPDATE,
    adUpdates: [adUpdate],
  };

  console.log(
    `[CloudMatch] reportSessionAd: sending action=${input.action}(${requestBody.adUpdates[0].adAction}), adId=${input.adId}, ` +
      `sessionId=${input.sessionId}, zone=${input.zone}, url=${url}, ` +
      `cancelReason=${input.cancelReason ?? "n/a"}, errorInfo=${input.errorInfo ?? "n/a"}`,
  );

  const response = await fetchCloudMatch(url, {
    method: "PUT",
    // Official browser requests include Origin/Referer on cross-origin ad updates.
    headers: buildGfnCloudMatchHeaders({ token: input.token, clientId, deviceId, includeOrigin: true }),
    body: JSON.stringify(requestBody),
  });

  const { text, payload } = await readCloudMatchJson<CloudMatchResponse>(response, {
    onErrorText: (text) => {
      console.warn(
        `[CloudMatch] reportSessionAd: backend error status=${response.status}, sessionId=${input.sessionId}, ` +
          `adId=${input.adId}, action=${input.action}, body=${text.slice(0, 500)}`,
      );
    },
  });
  if (payload.requestStatus.statusCode !== 1) {
    console.warn(
      `[CloudMatch] reportSessionAd: API error requestStatus=${payload.requestStatus.statusCode}, ` +
        `description=${payload.requestStatus.statusDescription ?? "unknown"}, sessionId=${input.sessionId}, ` +
        `adId=${input.adId}, action=${input.action}`,
    );
    throw SessionError.fromResponse(200, text);
  }

  console.log(
    `[CloudMatch] reportSessionAd: success sessionId=${input.sessionId}, adId=${input.adId}, action=${input.action}, ` +
      `status=${payload.session.status}, queuePosition=${extractQueuePosition(payload) ?? "n/a"}, ` +
      `adsRequired=${extractAdState(payload)?.isAdsRequired ?? false}`,
  );

  return await toSessionInfo({ zone: input.zone, streamingBaseUrl: base, payload, clientId, deviceId });
}

export async function stopSession(input: SessionStopRequest): Promise<void> {
  if (!input.token) {
    throw new Error("Missing token for session stop");
  }

  // Use provided client/device IDs if available (should match session creation)
  const clientId = input.clientId ?? LCARS_CLIENT_ID;
  const deviceId = input.deviceId ?? getCloudMatchDeviceHashId();

  const base = resolvePollStopBase(input.zone, input.streamingBaseUrl, input.serverIp);
  const url = `${base}/v2/session/${input.sessionId}`;
  const response = await fetchCloudMatch(url, {
    method: "DELETE",
    headers: buildGfnCloudMatchHeaders({ token: input.token, clientId, deviceId, includeOrigin: false }),
  });

  await throwIfCloudMatchResponseError(response);
}

/**
 * Get list of active sessions (status 2 or 3)
 * Returns sessions that are Ready or Streaming
 */
export async function getActiveSessions(
  token: string,
  streamingBaseUrl: string,
): Promise<ActiveSessionInfo[]> {
  if (!token) {
    throw new Error("Missing token for getting active sessions");
  }

  const base = normalizeTrustedCloudMatchBaseUrl(streamingBaseUrl);
  const headers = buildGfnCloudMatchHeaders({
    token,
    deviceId: getCloudMatchDeviceHashId(),
    includeOrigin: false,
  });
  const primary = await fetchActiveSessionsFromBase(base, headers);
  if (primary) {
    return primary;
  }

  for (const discoveredBase of await discoverActiveSessionFallbackBases(base, headers)) {
    let fallbackBase: string;
    try {
      fallbackBase = normalizeTrustedCloudMatchBaseUrl(discoveredBase);
    } catch {
      console.warn("[CloudMatch] Ignoring untrusted active-session fallback endpoint");
      continue;
    }
    if (fallbackBase === base) {
      continue;
    }
    const fallback = await fetchActiveSessionsFromBase(fallbackBase, headers);
    if (fallback) {
      return fallback;
    }
  }

  return [];
}

async function discoverActiveSessionFallbackBases(
  base: string,
  headers: Record<string, string>,
): Promise<string[]> {
  try {
    const response = await fetchCloudMatch(`${base}/v2/serverInfo`, {
      method: "GET",
      headers,
    });
    if (!response.ok) {
      return [];
    }
    return extractServerInfoRegionBases((await response.json()) as CloudMatchServerInfoResponse);
  } catch (error) {
    console.warn(`[CloudMatch] getActiveSessions fallback discovery failed: ${formatErrorForLog(error)}`);
    return [];
  }
}

async function fetchActiveSessionsFromBase(
  base: string,
  headers: Record<string, string>,
): Promise<ActiveSessionInfo[] | null> {
  const url = `${base}/v2/session`;

  let response: Response;
  try {
    response = await fetchCloudMatch(url, {
      method: "GET",
      headers,
    }, { retries: 0 });
  } catch (error) {
    console.warn(`[CloudMatch] getActiveSessions fetch failed for ${base}: ${formatErrorForLog(error)}`);
    return null;
  }

  const text = await response.text();

  if (!response.ok) {
    console.warn(`Get sessions failed: ${response.status} - ${text.slice(0, 200)}`);
    return null;
  }

  let sessionsResponse: GetSessionsResponse;
  try {
    sessionsResponse = JSON.parse(text) as GetSessionsResponse;
  } catch {
    return [];
  }

  if (sessionsResponse.requestStatus.statusCode !== 1) {
    console.warn(`Get sessions API error: ${sessionsResponse.requestStatus.statusDescription}`);
    return [];
  }

  // Filter active sessions:
  //   1 = Setup/Queuing (counts against SESSION_LIMIT — must be included for resume logic)
  //   2 = Ready
  //   3 = Streaming
  const activeSessions: ActiveSessionInfo[] = sessionsResponse.sessions
    .filter((s) => s.status === 1 || s.status === 2 || s.status === 3)
    .map((s) => {
      // Extract appId from sessionRequestData
      const appId = s.sessionRequestData?.appId ? Number(s.sessionRequestData.appId) : 0;

      // The server echoes the appLaunchMode the session was created with; keep it
      // so claim/resume requests can stay session-stable.
      const rawAppLaunchMode = s.sessionRequestData?.appLaunchMode;
      const appLaunchMode =
        typeof rawAppLaunchMode === "number" && Number.isFinite(rawAppLaunchMode)
          ? rawAppLaunchMode
          : undefined;
      const enablePersistingInGameSettings =
        typeof s.sessionRequestData?.enablePersistingInGameSettings === "boolean"
          ? s.sessionRequestData.enablePersistingInGameSettings
          : undefined;

      // Prefer the real server IP from connectionInfo[usage=14] — this is the actual game server,
      // not the zone load balancer. sessionControlInfo.ip is the zone LB hostname and cannot
      // accept claim (PUT) requests, which causes HTTP 400.
      const connInfo = s.connectionInfo?.find((conn) => conn.usage === 14 && conn.ip);
      const rawConnIp = connInfo?.ip as string | string[] | undefined;
      const connIp = Array.isArray(rawConnIp) ? rawConnIp[0] : rawConnIp;

      const rawControlIp = s.sessionControlInfo?.ip as string | string[] | undefined;
      const controlIp = Array.isArray(rawControlIp) ? rawControlIp[0] : rawControlIp;

      const serverIp = connIp ?? controlIp;

      const signalingUrl = connIp
        ? `wss://${connIp}:443/nvst/`
        : controlIp
          ? `wss://${controlIp}:443/nvst/`
          : undefined;

      // Extract resolution and fps from monitor settings
      const monitorSettings = s.monitorSettings?.[0];
      const resolution = monitorSettings
        ? `${monitorSettings.widthInPixels ?? 0}x${monitorSettings.heightInPixels ?? 0}`
        : undefined;
      const fps = monitorSettings?.framesPerSecond ?? undefined;

      return {
        sessionId: s.sessionId,
        subSessionId: s.subSessionId,
        appId,
        appLaunchMode,
        enablePersistingInGameSettings,
        gpuType: s.gpuType,
        status: s.status,
        queuePosition: extractSessionQueuePosition(s),
        seatSetupStep: extractSessionSeatSetupStep(s),
        streamingBaseUrl: base,
        serverIp,
        signalingUrl,
        resolution,
        fps,
      };
    });

  return activeSessions;
}

/**
 * Claim/Resume an existing session
 * Required before connecting to an existing session
 */
export async function claimSession(input: SessionClaimRequest): Promise<SessionInfo> {
  if (!input.token) {
    throw new Error("Missing token for session claim");
  }

  const deviceId = input.deviceId ?? getCloudMatchDeviceHashId();
  const clientId = input.clientId ?? LCARS_CLIENT_ID;

  // Provide default values for optional parameters
  const appId = input.appId ?? "0";
  const settings = input.settings ?? {
    resolution: "1920x1080",
    fps: 60,
    maxBitrateMbps: 75,
    codec: "H264",
    colorQuality: "8bit_420",
    keyboardLayout: DEFAULT_KEYBOARD_LAYOUT,
    gameLanguage: "en_US",
    enableL4S: false,
    enableCloudGsync: false,
  };
  const keyboardLayout = resolveGfnKeyboardLayout(settings.keyboardLayout ?? DEFAULT_KEYBOARD_LAYOUT, process.platform);
  const languageCode = settings.gameLanguage ?? "en_US";

  // The session list endpoint returns the zone LB hostname in sessionControlInfo.ip.
  // A claim PUT sent to the zone LB returns HTTP 400 because it does not handle
  // session-level mutations. The real game server IP is only reliably available from
  // the individual session endpoint (GET /v2/session/{id}). Resolve it here before
  // building the claim URL.
  // IMPORTANT: We must query the SAME zone LB where the session is hosted (use serverIp),
  // not the provider's generic streamingBaseUrl (which may route to a different zone LB).
  let effectiveServerIp = input.serverIp;
  console.log(`[CloudMatch] claimSession: input serverIp=${input.serverIp}, isZone=${isZoneHostname(input.serverIp)}`);
  if (isZoneHostname(effectiveServerIp)) {
    const zoneBase = `https://${effectiveServerIp}`;
    const prefetchUrl = `${zoneBase}/v2/session/${input.sessionId}`;
    console.log(`[CloudMatch] claimSession: pre-flight query ${prefetchUrl}`);
    const prefetchHeaders = buildGfnCloudMatchHeaders({ token: input.token, clientId, deviceId, includeOrigin: false });
    try {
      const prefetchResp = await fetchCloudMatch(prefetchUrl, { method: "GET", headers: prefetchHeaders });
      console.log(`[CloudMatch] claimSession: pre-flight response status=${prefetchResp.status}`);
      if (prefetchResp.ok) {
        const prefetchPayload = JSON.parse(await prefetchResp.text()) as CloudMatchResponse;
        const realIp = streamingServerIp(prefetchPayload);
        console.log(`[CloudMatch] claimSession: extracted realIp=${realIp}, isZone=${realIp ? isZoneHostname(realIp) : 'N/A'}`);
        if (realIp) {
          effectiveServerIp = realIp;
          const ipType = isZoneHostname(realIp) ? 'zone LB' : 'direct IP';
          console.log(`[CloudMatch] claimSession: using extracted ${ipType}: ${realIp}`);
        }
      } else {
        console.warn(`[CloudMatch] claimSession: pre-flight returned HTTP ${prefetchResp.status}, text=${await prefetchResp.text()}`);
      }
    } catch (e) {
      console.warn("[CloudMatch] claimSession: pre-flight poll failed, proceeding with zone hostname:", e);
    }
  }

  const claimUrl = `https://${effectiveServerIp}/v2/session/${input.sessionId}?${new URLSearchParams({ keyboardLayout, languageCode }).toString()}`;

  // Pre-claim validation: check session status before deciding whether to send a RESUME claim.
  // Status 1 (setup/launching/queuing) sessions cannot be RESUME'd — the server will reject
  // with SESSION_NOT_PAUSED. For these sessions we skip the claim PUT and poll directly.
  // Status 2/3 (ready/streaming) sessions are paused and can be RESUME'd normally.
  let preClaimStatus: number | null = null;
  let shouldSendResumeClaim = true;
  try {
    const validationUrl = `https://${effectiveServerIp}/v2/session/${input.sessionId}`;
    const validationHeaders = buildGfnCloudMatchHeaders({ token: input.token, clientId, deviceId, includeOrigin: false });
    const validationResp = await fetchCloudMatch(validationUrl, { method: "GET", headers: validationHeaders });
    if (validationResp.ok) {
      const validationText = await validationResp.text();
      const validationPayload = JSON.parse(validationText) as CloudMatchResponse;
      preClaimStatus = validationPayload.session?.status ?? 0;
      const errorCode = validationPayload.session?.errorCode ?? 0;
      console.log(`[CloudMatch] claimSession: pre-claim validation status=${preClaimStatus}, errorCode=${errorCode}`);
      if (preClaimStatus === 1) {
        console.log(`[CloudMatch] claimSession: session is still launching (status=1), skipping RESUME claim — polling directly to ready state`);
      } else if (
        input.recoveryMode === true &&
        (preClaimStatus === 2 || preClaimStatus === 3)
      ) {
        // Recovery parity: if the session is already ready/streaming, avoid sending
        // another RESUME mutation. Repeated RESUME PUTs can rotate signaling hosts
        // and push the session back into transient setup/cleanup states.
        shouldSendResumeClaim = false;
        console.log(
          `[CloudMatch] claimSession: recoveryMode and session already ready (status=${preClaimStatus}); skipping redundant RESUME claim`,
        );
      } else if (preClaimStatus !== 2 && preClaimStatus !== 3) {
        console.warn(`[CloudMatch] claimSession: session not in ready state (status=${preClaimStatus}), claim may fail`);
      }
    } else {
      console.warn(`[CloudMatch] claimSession: pre-claim validation returned HTTP ${validationResp.status}`);
    }
  } catch (e) {
    console.warn("[CloudMatch] claimSession: pre-claim validation failed:", e);
  }

  // Only send the RESUME claim PUT if the session is in a paused state (status 2 or 3).
  // For status=1 (still launching) we bypass the claim and fall through to the polling loop.
  if (preClaimStatus !== 1 && shouldSendResumeClaim) {
    const payload = buildClaimRequestBody(
      input.sessionId,
      appId,
      settings,
      input.appLaunchMode,
      input.enablePersistingInGameSettings === true,
    );

    const headers = buildGfnCloudMatchClaimHeaders({ token: input.token, clientId, deviceId });

    console.log(`[CloudMatch] claimSession PUT ${claimUrl}`);
    console.log(`[CloudMatch] claimSession body: ${JSON.stringify(payload)}`);
    const response = await fetchCloudMatch(claimUrl, {
      method: "PUT",
      headers,
      body: JSON.stringify(payload),
    });

    const { text, payload: apiResponse } = await readCloudMatchJson<CloudMatchResponse>(response);
    console.log(
      `[CloudMatch] claimSession response: HTTP ${response.status}, requestStatus=${apiResponse.requestStatus.statusCode}, sessionStatus=${apiResponse.session?.status ?? "n/a"}, connectionInfo=${apiResponse.session?.connectionInfo?.length ?? 0}`,
    );

    if (apiResponse.requestStatus.statusCode !== 1) {
      throw SessionError.fromResponse(200, text);
    }
  }

  // Poll until session is ready (status 2 or 3)
  const getUrl = `https://${effectiveServerIp}/v2/session/${input.sessionId}`;
  const maxAttempts = 60;

  for (let attempt = 1; attempt <= maxAttempts; attempt++) {
    if (attempt > 1) {
      await new Promise((resolve) => setTimeout(resolve, 1000));
    }

    const pollHeaders = buildGfnCloudMatchHeaders({ token: input.token, clientId, deviceId, includeOrigin: false });

    const pollResponse = await fetchCloudMatch(getUrl, {
      method: "GET",
      headers: pollHeaders,
    });

    if (!pollResponse.ok) {
      continue;
    }

    const pollText = await pollResponse.text();
    let pollApiResponse: CloudMatchResponse;

    try {
      pollApiResponse = JSON.parse(pollText) as CloudMatchResponse;
    } catch {
      continue;
    }

    const sessionData = pollApiResponse.session;

    if (sessionData.status === 2 || sessionData.status === 3) {
      // Session is ready
      const signaling = resolveSignaling(pollApiResponse);
      const queuePosition = extractQueuePosition(pollApiResponse);
      const negotiatedStreamProfile = extractNegotiatedStreamProfile(pollApiResponse);
      const requestedStreamingFeatures = normalizeStreamingFeatures(
        pollApiResponse.session.sessionRequestData?.requestedStreamingFeatures,
      );
      const finalizedStreamingFeatures = normalizeStreamingFeatures(
        pollApiResponse.session.finalizedStreamingFeatures,
      );
      const enablePersistingInGameSettings =
        typeof pollApiResponse.session.sessionRequestData?.enablePersistingInGameSettings === "boolean"
          ? pollApiResponse.session.sessionRequestData.enablePersistingInGameSettings
          : undefined;
      console.log(
        `[CloudMatch] claimed negotiated streaming features: requested=${JSON.stringify(requestedStreamingFeatures ?? {})} finalized=${JSON.stringify(finalizedStreamingFeatures ?? {})} cloudGsync=${negotiatedStreamProfile?.enableCloudGsync ?? "n/a"}, reflex=${negotiatedStreamProfile?.enableReflex ?? "n/a"}, l4s=${negotiatedStreamProfile?.enableL4S ?? "n/a"}`,
      );

      return {
        sessionId: sessionData.sessionId,
        subSessionId: sessionData.subSessionId,
        appId: input.appId,
        status: sessionData.status,
        queuePosition,
        zone: "", // Zone not applicable for claimed sessions
        streamingBaseUrl: `https://${effectiveServerIp}`,
        serverIp: signaling.serverIp,
        signalingServer: signaling.signalingServer,
        signalingUrl: signaling.signalingUrl,
        gpuType: sessionData.gpuType,
        appLaunchMode: echoedSessionAppLaunchMode(pollApiResponse) ?? input.appLaunchMode,
        enablePersistingInGameSettings,
        connectionInfo: sessionData.connectionInfo?.map((connection) => ({ ...connection })),
        rtspsEndpoints: signaling.rtspsEndpoints.length > 0 ? signaling.rtspsEndpoints : undefined,
        iceServers: await normalizeIceServers(pollApiResponse),
        mediaConnectionInfo: signaling.mediaConnectionInfo,
        negotiatedStreamProfile: negotiatedStreamProfile ?? extractNegotiatedStreamProfile(pollApiResponse),
        requestedStreamingFeatures,
        finalizedStreamingFeatures,
        clientId,
        deviceId,
      };
    }

    // Status 1 (setup/launching), 6 (cleaning up), etc. — continue polling for ready state (2 or 3)
    // Only break if we encounter a terminal error state (status 4, 5, etc.)
    if (sessionData.status > 3 && sessionData.status !== 6) {
      break;
    }
  }

  throw new Error("Session did not become ready after claiming");
}
