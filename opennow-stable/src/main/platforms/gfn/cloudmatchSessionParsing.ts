import type {
  ColorQuality,
  NegotiatedStreamProfile,
  SessionAdInfo,
  SessionAdState,
  SessionInfo,
  StreamingFeatures,
} from "@shared/gfn";

import type { CloudMatchResponse, GetSessionsResponse } from "./types";
import { SessionError } from "./errorCodes";
import { resolveSessionControlBaseUrl } from "./cloudmatchTransport";
import {
  normalizeIceServers,
  resolveSignaling,
} from "./cloudmatchSignaling";

const GFN_AD_MEDIA_PROFILE_ORDER = new Map<string, number>([
  ["mp4deinterlaced720p", 0],
  ["webm", 1],
  ["hlsadaptive", 2],
]);

/** Wire appLaunchMode the server echoes back for an existing session, if present. */
export function echoedSessionAppLaunchMode(payload: CloudMatchResponse): number | undefined {
  const raw = payload.session?.sessionRequestData?.appLaunchMode;
  return typeof raw === "number" && Number.isFinite(raw) ? raw : undefined;
}

export function toPositiveInt(value: unknown): number | undefined {
  if (typeof value === "number" && Number.isFinite(value)) {
    const normalized = Math.trunc(value);
    return normalized > 0 ? normalized : undefined;
  }
  if (typeof value === "string" && value.trim().length > 0) {
    const parsed = Number.parseInt(value, 10);
    return Number.isFinite(parsed) && parsed > 0 ? parsed : undefined;
  }
  return undefined;
}

export function toBoolean(value: unknown): boolean | undefined {
  if (typeof value === "boolean") {
    return value;
  }
  if (typeof value === "number" && Number.isFinite(value)) {
    return value !== 0;
  }
  if (typeof value === "string") {
    const normalized = value.trim().toLowerCase();
    if (normalized === "true" || normalized === "1") {
      return true;
    }
    if (normalized === "false" || normalized === "0") {
      return false;
    }
  }
  return undefined;
}

export function toOptionalString(value: unknown): string | undefined {
  if (typeof value !== "string") {
    return undefined;
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : undefined;
}

export function extractSessionQueuePosition(session: CloudMatchResponse["session"] | GetSessionsResponse["sessions"][number]): number | undefined {
  const direct = toPositiveInt(session.queuePosition);
  if (direct !== undefined) {
    return direct;
  }

  const seatSetup = session.seatSetupInfo;
  if (seatSetup) {
    const nested = toPositiveInt(seatSetup.queuePosition);
    if (nested !== undefined) {
      return nested;
    }
  }

  const nestedSessionProgress = session.sessionProgress;
  if (nestedSessionProgress) {
    const nested = toPositiveInt(nestedSessionProgress.queuePosition);
    if (nested !== undefined) {
      return nested;
    }
  }

  const nestedProgressInfo = session.progressInfo;
  if (nestedProgressInfo) {
    const nested = toPositiveInt(nestedProgressInfo.queuePosition);
    if (nested !== undefined) {
      return nested;
    }
  }

  return undefined;
}

export function extractQueuePosition(payload: CloudMatchResponse): number | undefined {
  return extractSessionQueuePosition(payload.session);
}

export function extractSessionSeatSetupStep(session: CloudMatchResponse["session"] | GetSessionsResponse["sessions"][number]): number | undefined {
  const raw = session.seatSetupInfo?.seatSetupStep;
  if (typeof raw === "number" && Number.isFinite(raw)) {
    return Math.trunc(raw);
  }
  return undefined;
}

export function extractSeatSetupStep(payload: CloudMatchResponse): number | undefined {
  return extractSessionSeatSetupStep(payload.session);
}

export function normalizeSessionAdInfo(ad: NonNullable<CloudMatchResponse["session"]["sessionAds"]>[number], index: number): SessionAdInfo | null {
  const adId = toOptionalString(ad.adId);
  const adMediaFiles = (ad.adMediaFiles ?? [])
    .map((file) => ({
      mediaFileUrl: toOptionalString(file.mediaFileUrl),
      encodingProfile: toOptionalString(file.encodingProfile),
    }))
    .filter((file) => file.mediaFileUrl || file.encodingProfile)
    .sort((left, right) => {
      const leftRank = left.encodingProfile ? GFN_AD_MEDIA_PROFILE_ORDER.get(left.encodingProfile) ?? Number.MAX_SAFE_INTEGER : Number.MAX_SAFE_INTEGER;
      const rightRank = right.encodingProfile ? GFN_AD_MEDIA_PROFILE_ORDER.get(right.encodingProfile) ?? Number.MAX_SAFE_INTEGER : Number.MAX_SAFE_INTEGER;
      return leftRank - rightRank;
    });

  // Match the official browser config preference order: MP4, WebM, then HLS.
  const preferredMediaFile = adMediaFiles.find((file) => file.mediaFileUrl);
  const mediaUrl =
    preferredMediaFile?.mediaFileUrl ??
    toOptionalString(ad.adUrl) ??
    toOptionalString(ad.mediaUrl) ??
    toOptionalString(ad.videoUrl) ??
    toOptionalString(ad.url);

  const adUrl = toOptionalString(ad.adUrl);
  const clickThroughUrl = toOptionalString(ad.clickThroughUrl);
  const title = toOptionalString(ad.title);
  const description = toOptionalString(ad.description);
  const adLengthInSeconds =
    typeof ad.adLengthInSeconds === "number" && Number.isFinite(ad.adLengthInSeconds) && ad.adLengthInSeconds > 0
      ? ad.adLengthInSeconds
      : undefined;

  // adLengthInSeconds is the confirmed live field (value is in seconds, convert to ms).
  // Fall back to legacy durationMs / durationInMs which are already in ms.
  const durationMs =
    (adLengthInSeconds !== undefined
      ? Math.round(adLengthInSeconds * 1000)
      : undefined) ??
    toPositiveInt(ad.durationMs) ??
    toPositiveInt(ad.durationInMs);

  const adState = typeof ad.adState === "number" && Number.isFinite(ad.adState) ? Math.trunc(ad.adState) : undefined;

  if (!adId && !mediaUrl && !adUrl && adMediaFiles.length === 0 && !title && !description) {
    return null;
  }

  return {
    adId: adId ?? `ad-${index + 1}`,
    state: adState,
    adState,
    adUrl,
    mediaUrl,
    adMediaFiles,
    clickThroughUrl,
    adLengthInSeconds,
    durationMs,
    title,
    description,
  };
}

export function extractAdState(payload: CloudMatchResponse): SessionAdState | undefined {
  const sessionAdsRequired =
    toBoolean(payload.session.sessionAdsRequired) ??
    toBoolean(payload.session.isAdsRequired) ??
    toBoolean(payload.session.sessionProgress?.isAdsRequired) ??
    toBoolean(payload.session.progressInfo?.isAdsRequired);

  // Log raw sessionAds whenever the server signals ads are required so field names
  // can be verified when creative URLs are expected but the ads[] array stays empty.
  if (sessionAdsRequired) {
    console.log(
      `[CloudMatch] extractAdState: sessionAdsRequired=${payload.session.sessionAdsRequired}, ` +
      `isAdsRequired=${payload.session.isAdsRequired}, ` +
      `sessionAds=${JSON.stringify(payload.session.sessionAds ?? null)}, ` +
      `opportunity=${JSON.stringify(payload.session.opportunity ?? null)}`,
    );
  }

  const ads = (payload.session.sessionAds ?? [])
    .map((ad, index) => normalizeSessionAdInfo(ad, index))
    .filter((ad): ad is SessionAdInfo => ad !== null);

  const opportunity = payload.session.opportunity;
  const normalizedOpportunity = opportunity
    ? {
        state: toOptionalString(opportunity.state),
        queuePaused: toBoolean(opportunity.queuePaused),
        gracePeriodSeconds: toPositiveInt(opportunity.gracePeriodSeconds),
        message: toOptionalString(opportunity.message),
        title: toOptionalString(opportunity.title),
        description: toOptionalString(opportunity.description),
      }
    : undefined;
  const queuePaused =
    normalizedOpportunity?.queuePaused ??
    (typeof normalizedOpportunity?.state === "string" ? normalizedOpportunity.state.toLowerCase() === "graceperiodstart" : undefined);
  const gracePeriodSeconds = normalizedOpportunity?.gracePeriodSeconds;
  const effectiveIsAdsRequired = sessionAdsRequired ?? ads.length > 0;
  const message =
    normalizedOpportunity?.message ??
    normalizedOpportunity?.description ??
    (queuePaused
      ? "Resume ads to stay in queue."
      : effectiveIsAdsRequired
        ? "Finish ads to stay in queue."
        : undefined);

  if (!effectiveIsAdsRequired && ads.length === 0 && !queuePaused && !message) {
    return undefined;
  }

  return {
    isAdsRequired: effectiveIsAdsRequired,
    sessionAdsRequired,
    isQueuePaused: queuePaused,
    gracePeriodSeconds,
    message,
    sessionAds: ads,
    ads,
    opportunity: normalizedOpportunity,
    // Mark whether the server sent sessionAds=null (transient gap) so the
    // renderer's mergeAdState can safely restore the previous ad list for the
    // ad player, while NOT restoring it after an explicit client-side clear
    // that follows a rejected finish action.
    serverSentEmptyAds: payload.session.sessionAds == null,
  };
}

export function toColorQuality(bitDepth?: number, chromaFormat?: number): ColorQuality | undefined {
  const normalizedBitDepth = bitDepth === 10 ? 1 : bitDepth;
  const normalizedChromaFormat = chromaFormat === 2 ? 1 : chromaFormat;

  if (normalizedBitDepth !== 0 && normalizedBitDepth !== 1) {
    return undefined;
  }
  if (normalizedChromaFormat !== 0 && normalizedChromaFormat !== 1) {
    return undefined;
  }

  if (normalizedBitDepth === 1) {
    return normalizedChromaFormat === 1 ? "10bit_444" : "10bit_420";
  }

  return normalizedChromaFormat === 1 ? "8bit_444" : "8bit_420";
}

export function normalizeStreamingFeatures(
  features:
    | NonNullable<CloudMatchResponse["session"]["sessionRequestData"]>["requestedStreamingFeatures"]
    | CloudMatchResponse["session"]["finalizedStreamingFeatures"]
    | undefined,
): StreamingFeatures | undefined {
  if (!features) {
    return undefined;
  }

  const normalized: StreamingFeatures = {};

  if (typeof features.reflex === "boolean") {
    normalized.reflex = features.reflex;
  }
  if (typeof features.bitDepth === "number" && Number.isFinite(features.bitDepth)) {
    normalized.bitDepth = Math.trunc(features.bitDepth);
  }
  if (typeof features.cloudGsync === "boolean") {
    normalized.cloudGsync = features.cloudGsync;
  }
  if (typeof features.chromaFormat === "number" && Number.isFinite(features.chromaFormat)) {
    normalized.chromaFormat = Math.trunc(features.chromaFormat);
  }
  if (typeof features.enabledL4S === "boolean") {
    normalized.enabledL4S = features.enabledL4S;
  }
  if ("trueHdr" in features && typeof features.trueHdr === "boolean") {
    normalized.trueHdr = features.trueHdr;
  }

  return Object.keys(normalized).length > 0 ? normalized : undefined;
}

export function extractNegotiatedStreamProfile(payload: CloudMatchResponse): NegotiatedStreamProfile | undefined {
  const monitor = payload.session.sessionRequestData?.clientRequestMonitorSettings?.[0];
  const finalizedFeatures = payload.session.finalizedStreamingFeatures;
  const requestedFeatures = payload.session.sessionRequestData?.requestedStreamingFeatures;

  const width = monitor?.widthInPixels;
  const height = monitor?.heightInPixels;
  const fps = monitor?.framesPerSecond;
  const colorQuality = toColorQuality(
    finalizedFeatures?.bitDepth ?? requestedFeatures?.bitDepth,
    finalizedFeatures?.chromaFormat ?? requestedFeatures?.chromaFormat,
  );
  const enabledL4S = finalizedFeatures?.enabledL4S ?? requestedFeatures?.enabledL4S;
  const enabledCloudGsync = finalizedFeatures?.cloudGsync ?? requestedFeatures?.cloudGsync;
  const enabledReflex = finalizedFeatures?.reflex ?? requestedFeatures?.reflex;

  const profile: NegotiatedStreamProfile = {};

  if (
    typeof width === "number" &&
    Number.isFinite(width) &&
    width > 0 &&
    typeof height === "number" &&
    Number.isFinite(height) &&
    height > 0
  ) {
    profile.resolution = `${Math.trunc(width)}x${Math.trunc(height)}`;
  }

  if (typeof fps === "number" && Number.isFinite(fps) && fps > 0) {
    profile.fps = Math.trunc(fps);
  }

  if (colorQuality) {
    profile.colorQuality = colorQuality;
  }

  if (typeof enabledL4S === "boolean") {
    profile.enableL4S = enabledL4S;
  }

  if (typeof enabledCloudGsync === "boolean") {
    profile.enableCloudGsync = enabledCloudGsync;
  }

  if (typeof enabledReflex === "boolean") {
    profile.enableReflex = enabledReflex;
  }

  return Object.keys(profile).length > 0 ? profile : undefined;
}

export function firstZoneHostname(...candidates: unknown[]): string | undefined {
  for (const candidate of candidates) {
    const values = Array.isArray(candidate) ? candidate : [candidate];
    for (const value of values) {
      if (typeof value !== "string") {
        continue;
      }
      const hostname = value.trim();
      if (hostname && /[a-z]/i.test(hostname.split(".")[0] ?? "")) {
        return hostname;
      }
    }
  }
  return undefined;
}

export interface ToSessionInfoOptions {
  zone: string;
  streamingBaseUrl: string;
  payload: CloudMatchResponse;
  clientId?: string;
  deviceId?: string;
  fallbackAppId?: string;
  /** Wire appLaunchMode sent with the request, used when the server does not echo it */
  fallbackAppLaunchMode?: number;
}

export async function toSessionInfo(options: ToSessionInfoOptions): Promise<SessionInfo> {
  const { zone, streamingBaseUrl, payload, clientId, deviceId } = options;
  if (payload.requestStatus.statusCode !== 1) {
    // Use SessionError for parsing error responses
    const errorJson = JSON.stringify(payload);
    throw SessionError.fromResponse(200, errorJson);
  }

  const signaling = resolveSignaling(payload);
  const queuePosition = extractQueuePosition(payload);
  const seatSetupStep = extractSeatSetupStep(payload);
  const adState = extractAdState(payload);
  const negotiatedStreamProfile = extractNegotiatedStreamProfile(payload);
  const requestedStreamingFeatures = normalizeStreamingFeatures(
    payload.session.sessionRequestData?.requestedStreamingFeatures,
  );
  const finalizedStreamingFeatures = normalizeStreamingFeatures(
    payload.session.finalizedStreamingFeatures,
  );
  const serverLocation = firstZoneHostname(
    payload.session.serverLocation,
    payload.session.sessionControlInfo?.ip,
    payload.session.connectionInfo?.find((connection) => connection.usage === 14)?.ip,
  );
  const enablePersistingInGameSettings =
    typeof payload.session.sessionRequestData?.enablePersistingInGameSettings === "boolean"
      ? payload.session.sessionRequestData.enablePersistingInGameSettings
      : undefined;

  // Debug logging to trace signaling resolution
  const connections = payload.session.connectionInfo ?? [];
  const connectionSummary = connections
    .map((conn) => {
      const rawIp = Array.isArray(conn.ip) ? conn.ip[0] : conn.ip;
      return `{usage=${conn.usage},ip=${rawIp ?? "null"},port=${conn.port},resourcePath=${conn.resourcePath ?? "null"}}`;
    })
    .join(", ");
  console.log(
    `[CloudMatch] toSessionInfo: status=${payload.session.status}, ` +
    `seatSetupStep=${seatSetupStep ?? "n/a"}, ` +
    `queuePosition=${queuePosition ?? "n/a"}, ` +
    `gpuType=${payload.session.gpuType ?? "<empty>"}, ` +
    `connectionInfo=${connections.length} entries, ` +
    `serverIp=${signaling.serverIp}, ` +
    `signalingServer=${signaling.signalingServer}, ` +
    `signalingUrl=${signaling.signalingUrl}, ` +
    `rtspsEndpoints=${JSON.stringify(signaling.rtspsEndpoints)}, ` +
    `connections=[${connectionSummary}]`,
  );
  console.log(
    `[CloudMatch] negotiated streaming features: requested=${JSON.stringify(requestedStreamingFeatures ?? {})} finalized=${JSON.stringify(finalizedStreamingFeatures ?? {})} cloudGsync=${negotiatedStreamProfile?.enableCloudGsync ?? "n/a"}, reflex=${negotiatedStreamProfile?.enableReflex ?? "n/a"}, l4s=${negotiatedStreamProfile?.enableL4S ?? "n/a"}`,
  );

  return {
    sessionId: payload.session.sessionId,
    subSessionId: payload.session.subSessionId,
    appId: String(payload.session.sessionRequestData?.appId ?? options.fallbackAppId),
    status: payload.session.status,
    seatSetupStep,
    queuePosition,
    adState,
    zone,
    streamingBaseUrl: resolveSessionControlBaseUrl(payload.session.sessionControlInfo?.ip, streamingBaseUrl),
    serverIp: signaling.serverIp,
    signalingServer: signaling.signalingServer,
    signalingUrl: signaling.signalingUrl,
    serverLocation,
    gpuType: payload.session.gpuType,
    appLaunchMode: echoedSessionAppLaunchMode(payload) ?? options.fallbackAppLaunchMode,
    enablePersistingInGameSettings,
    connectionInfo: connections.length > 0 ? connections.map((connection) => ({ ...connection })) : undefined,
    rtspsEndpoints: signaling.rtspsEndpoints.length > 0 ? signaling.rtspsEndpoints : undefined,
    iceServers: await normalizeIceServers(payload),
    mediaConnectionInfo: signaling.mediaConnectionInfo,
    negotiatedStreamProfile,
    requestedStreamingFeatures,
    finalizedStreamingFeatures,
    clientId,
    deviceId,
  };
}
