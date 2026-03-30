import crypto from "node:crypto";
import dns from "node:dns";

import type {
  ActiveSessionInfo,
  IceServer,
  SessionClaimRequest,
  SessionCreateRequest,
  SessionInfo,
  SessionPollRequest,
  SessionStopRequest,
  StreamSettings,
} from "@shared/gfn";

import {
  colorQualityBitDepth,
  colorQualityChromaFormat,
} from "@shared/gfn";
import {
  buildGfnHeaders,
  GFN_APP_LAUNCH_MODE,
  GFN_BROWSER_TYPE_CHROME,
  GFN_CLIENT_IDENTIFICATION,
  GFN_CLIENT_PLATFORM_NAME,
  GFN_CLIENT_STREAMER_CLASSIC,
  GFN_CLIENT_STREAMER_WEBRTC,
  GFN_CLIENT_TYPE_NATIVE,
  GFN_DEVICE_MAKE_UNKNOWN,
  GFN_DEVICE_MODEL_UNKNOWN,
  GFN_DEVICE_TYPE_DESKTOP,
  GFN_ENHANCED_STREAM_MODE,
  GFN_PLAY_ORIGIN,
  GFN_PLAY_REFERER,
  GFN_SDK_VERSION,
  GFN_SESSION_REQUEST_CLIENT_VERSION,
  GFN_STREAMER_VERSION,
  resolveGfnDeviceOs,
} from "@shared/gfnClient";

import type { CloudMatchRequest, CloudMatchResponse, GetSessionsResponse } from "./types";
import { SessionError } from "./errorCodes";

const CLAIM_POLL_TIMEOUT_MS = 60_000;
const CLAIM_INITIAL_DELAY_MS = 250;
const CLAIM_MAX_DELAY_MS = 1_500;

async function resolveHostnameWithFallback(hostname: string): Promise<string | null> {
  // Try system resolver first, then fall back to Cloudflare (1.1.1.1) and Google (8.8.8.8)
  try {
    const r = await dns.promises.lookup(hostname);
    if (r && (r as any).address) return (r as any).address;
  } catch {
    // ignore and try custom resolvers
  }

  const fallbackServers = ["1.1.1.1", "8.8.8.8"];
  for (const server of fallbackServers) {
    try {
      const resolver = new dns.Resolver();
      resolver.setServers([server]);
      const addrs: string[] = await new Promise((resolve, reject) => {
        resolver.resolve4(hostname, (err, addresses) => {
          if (err) reject(err);
          else resolve(addresses);
        });
      });
      if (addrs && addrs.length > 0) return addrs[0];
    } catch {
      // try next fallback
    }
  }

  return null;
}

async function normalizeIceServers(response: CloudMatchResponse): Promise<IceServer[]> {
  const raw = response.session.iceServerConfiguration?.iceServers ?? [];
  const servers = raw
    .map((entry) => {
      const urls = Array.isArray(entry.urls) ? entry.urls : [entry.urls];
      return {
        urls,
        username: entry.username,
        credential: entry.credential,
      };
    })
    .filter((entry) => entry.urls.length > 0);

  if (servers.length > 0) {
    // Attempt to resolve any hostnames in STUN/TURN URLs to IPs to avoid relying on the
    // renderer's DNS resolution. This makes it possible to try alternate DNS servers
    // when the system resolver fails.
    const resolvedServers: IceServer[] = [];
    for (const s of servers) {
      const resolvedUrls: string[] = [];
      for (const u of s.urls) {
          try {
          const m = u.match(/^([a-zA-Z0-9+.-]+):([^/]+)/);
          if (m) {
            const scheme = m[1];
            const hostPort = m[2];
            const host = hostPort.split(":")[0];
            const portPart = hostPort.includes(":") ? ":" + hostPort.split(":").slice(1).join(":") : "";

            // Helper to bracket IPv6 literals when necessary
            const bracketIfIpv6 = (h: string) => {
              if (h.startsWith("[") && h.endsWith("]")) return h;
              // Heuristic: contains ':' and is not an IPv4 dotted-quad
              if (h.includes(":") && !/^\d{1,3}(?:\.\d{1,3}){3}$/.test(h)) {
                return `[${h}]`;
              }
              return h;
            };

            // If host already looks like an IPv4 or bracketed IPv6, keep original URL
            if (/^\d{1,3}(?:\.\d{1,3}){3}$/.test(host) || /^\[[0-9a-fA-F:]+\]$/.test(host)) {
              resolvedUrls.push(u);
            } else {
              const ip = await resolveHostnameWithFallback(host);
              const finalHost = ip ?? host;
              const maybeBracketted = bracketIfIpv6(finalHost);
              resolvedUrls.push(`${scheme}:${maybeBracketted}${portPart}`);
            }
          } else {
            resolvedUrls.push(u);
          }
        } catch {
          resolvedUrls.push(u);
        }
      }
      resolvedServers.push({ urls: resolvedUrls, username: s.username, credential: s.credential });
    }

    return resolvedServers;
  }

  // Default fallbacks — try to resolve known STUN hostnames to IPs as well
  const defaults = ["s1.stun.gamestream.nvidia.com:19308", "stun.l.google.com:19302", "stun1.l.google.com:19302"];
  const out: IceServer[] = [];
  for (const d of defaults) {
    const parts = d.split(":");
    const host = parts[0];
    const port = parts.length > 1 ? `:${parts.slice(1).join(":")}` : "";
    const ip = await resolveHostnameWithFallback(host);
    const bracketIfIpv6 = (h: string) => (h.includes(":") && !h.startsWith("[") ? `[${h}]` : h);
    if (ip) out.push({ urls: [`stun:${bracketIfIpv6(ip)}${port}`] });
    else out.push({ urls: [`stun:${bracketIfIpv6(host)}${port}`] });
  }

  return out;
}

/**
 * Extract the streaming server IP from the CloudMatch response, matching Rust's
 * `streaming_server_ip()` priority chain:
 *   1. connectionInfo[usage==14].ip (direct IP)
 *   2. Host extracted from connectionInfo[usage==14].resourcePath (for rtsps:// URLs)
 *   3. sessionControlInfo.ip (fallback)
 */
function streamingServerIp(response: CloudMatchResponse): string | null {
  const connections = response.session.connectionInfo ?? [];
  const sigConn = connections.find((conn) => conn.usage === 14);

  if (sigConn) {
    // Priority 1: Direct IP field
    const rawIp = sigConn.ip;
    const directIp = Array.isArray(rawIp) ? rawIp[0] : rawIp;
    if (directIp && directIp.length > 0) {
      return directIp;
    }

    // Priority 2: Extract host from resourcePath (Alliance format: rtsps://host:port)
    if (sigConn.resourcePath) {
      const host = extractHostFromUrl(sigConn.resourcePath);
      if (host) return host;
    }
  }

  // Priority 3: sessionControlInfo.ip
  const controlIp = response.session.sessionControlInfo?.ip;
  if (controlIp && controlIp.length > 0) {
    return Array.isArray(controlIp) ? controlIp[0] : controlIp;
  }

  return null;
}

/**
 * Extract host from a URL string (handles rtsps://, rtsp://, wss://, https://).
 * Matches Rust's extract_host_from_url().
 */
function extractHostFromUrl(url: string): string | null {
  if (!/^(rtsps?|wss?|https?):\/\//i.test(url)) {
    return null;
  }

  try {
    const parsed = new URL(url.replace(/^rtsps:/i, "https:").replace(/^rtsp:/i, "http:"));
    const host = parsed.hostname;
    if (!host || host.length === 0 || host.startsWith(".")) {
      return null;
    }
    return host;
  } catch {
    return null;
  }
}

/**
 * Check if a given IP/hostname is a CloudMatch zone load balancer hostname
 * (not a real game server IP). Zone hostnames look like:
 *   np-ams-06.cloudmatchbeta.nvidiagrid.net
 */
function isZoneHostname(ip: string): boolean {
  const normalized = ip.replace(/^\[([^\]]+)\]:(\d+)$/, "$1").split(":")[0] ?? ip;
  return normalized.includes("cloudmatchbeta.nvidiagrid.net") || normalized.includes("cloudmatch.nvidiagrid.net");
}

type SessionRoutingInfo = Pick<SessionInfo, "serverIp" | "signalingServer" | "signalingUrl" | "mediaConnectionInfo">;

const sessionRoutingCache = new Map<string, SessionRoutingInfo>();

function isUsefulRoutingHost(value: string): boolean {
  return value.length > 0 && !isZoneHostname(value);
}

function isUsefulSignalingUrl(value: string): boolean {
  try {
    return !isZoneHostname(new URL(value).hostname);
  } catch {
    return !isZoneHostname(value);
  }
}

function isUsefulMediaConnectionInfo(info?: SessionRoutingInfo["mediaConnectionInfo"]): boolean {
  return Boolean(info?.ip && info.port > 0 && !isZoneHostname(info.ip));
}

function mergeSessionRoutingInfo(sessionId: string, next: SessionRoutingInfo): {
  routing: SessionRoutingInfo;
  retainedFields: string[];
} {
  const previous = sessionRoutingCache.get(sessionId);
  const routing: SessionRoutingInfo = {
    serverIp: next.serverIp,
    signalingServer: next.signalingServer,
    signalingUrl: next.signalingUrl,
    mediaConnectionInfo: next.mediaConnectionInfo,
  };
  const retainedFields: string[] = [];

  if (previous) {
    if (!isUsefulRoutingHost(routing.serverIp) && isUsefulRoutingHost(previous.serverIp)) {
      routing.serverIp = previous.serverIp;
      retainedFields.push("serverIp");
    }

    if (!isUsefulRoutingHost(routing.signalingServer) && isUsefulRoutingHost(previous.signalingServer)) {
      routing.signalingServer = previous.signalingServer;
      retainedFields.push("signalingServer");
    }

    if (!isUsefulSignalingUrl(routing.signalingUrl) && isUsefulSignalingUrl(previous.signalingUrl)) {
      routing.signalingUrl = previous.signalingUrl;
      retainedFields.push("signalingUrl");
    }

    if (!isUsefulMediaConnectionInfo(routing.mediaConnectionInfo) && isUsefulMediaConnectionInfo(previous.mediaConnectionInfo)) {
      routing.mediaConnectionInfo = previous.mediaConnectionInfo;
      retainedFields.push("mediaConnectionInfo");
    }
  }

  sessionRoutingCache.set(sessionId, routing);
  return { routing, retainedFields };
}

function resolveSignaling(response: CloudMatchResponse): {
  serverIp: string;
  signalingServer: string;
  signalingUrl: string;
  mediaConnectionInfo?: { ip: string; port: number };
} {
  const connections = response.session.connectionInfo ?? [];
  const signalingConnection =
    connections.find((conn) => conn.usage === 14) ?? connections.find((conn) => conn.ip);

  // Use the Rust-matching priority chain for server IP
  const serverIp = streamingServerIp(response);
  if (!serverIp) {
    throw new Error("CloudMatch response did not include a signaling host");
  }

  const { signalingUrl, signalingHost, signalingPort } = buildSignalingUrl(signalingConnection, serverIp);
  const effectiveHost = signalingHost ?? serverIp;
  const signalingServer = `${effectiveHost}:${signalingPort}`;

  return {
    serverIp,
    signalingServer,
    signalingUrl,
    mediaConnectionInfo: resolveMediaConnectionInfo(connections, serverIp),
  };
}

/**
 * Resolve the media connection endpoint (IP + port) from the session's connectionInfo array.
 * Matches Rust's media_connection_info() priority chain:
 *   1. usage=2 (Primary media path, UDP)
 *   2. usage=17 (Alternative media path)
 *   3. usage=14 with highest port (Alliance fallback — distinguishes media port from signaling port)
 *   4. Fallback: use serverIp with the highest port from any usage=14 entry
 *
 * For each entry, IP is extracted from:
 *   a. The .ip field directly
 *   b. The hostname in .resourcePath (e.g. rtsps://80-250-97-40.server.net:48322)
 *   c. Fallback to serverIp (only for usage=14 Alliance fallback)
 */
function resolveMediaConnectionInfo(
  connections: Array<{ ip?: string; port: number; usage: number; protocol?: number; resourcePath?: string }>,
  serverIp: string,
): { ip: string; port: number } | undefined {
  // Helper: extract IP from a connection entry
  const extractIp = (conn: { ip?: string; resourcePath?: string }): string | null => {
    // Try direct IP field
    const rawIp = conn.ip;
    const directIp = Array.isArray(rawIp) ? rawIp[0] : rawIp;
    if (directIp && directIp.length > 0) return directIp;

    // Try hostname from resourcePath
    if (conn.resourcePath) {
      const host = extractHostFromUrl(conn.resourcePath);
      if (host) return host;
    }

    return null;
  };

  // Helper: extract port from a connection entry (fallback to resourcePath URL port)
  const extractPort = (conn: { port: number; resourcePath?: string }): number => {
    if (conn.port > 0) return conn.port;

    // Try extracting port from resourcePath URL
    if (conn.resourcePath) {
      try {
        const url = new URL(conn.resourcePath.replace("rtsps://", "https://").replace("rtsp://", "http://"));
        const portStr = url.port;
        if (portStr) return parseInt(portStr, 10);
      } catch {
        // Ignore
      }
    }

    return 0;
  };

  // Priority 1: usage=2 (Primary media path, UDP)
  const primary = connections.find((c) => c.usage === 2);
  if (primary) {
    const ip = extractIp(primary);
    const port = extractPort(primary);
    console.log(`[CloudMatch] resolveMediaConnectionInfo: usage=2 candidate: ip=${ip}, port=${port}`);
    if (ip && port > 0) return { ip, port };
  }

  // Priority 2: usage=17 (Alternative media path)
  const alt = connections.find((c) => c.usage === 17);
  if (alt) {
    const ip = extractIp(alt);
    const port = extractPort(alt);
    console.log(`[CloudMatch] resolveMediaConnectionInfo: usage=17 candidate: ip=${ip}, port=${port}`);
    if (ip && port > 0) return { ip, port };
  }

  // Priority 3: usage=14 with highest port (Alliance fallback)
  const alliance = connections
    .filter((c) => c.usage === 14)
    .sort((a, b) => b.port - a.port);

  for (const conn of alliance) {
    const ip = extractIp(conn) ?? serverIp;
    const port = extractPort(conn);
    console.log(`[CloudMatch] resolveMediaConnectionInfo: usage=14 candidate: ip=${ip}, port=${port} (serverIp fallback=${serverIp})`);
    if (ip && port > 0) return { ip, port };
  }

  console.log("[CloudMatch] resolveMediaConnectionInfo: NO valid media connection info found");
  return undefined;
}

/**
 * Build signaling websocket URL using CloudMatch usage=14 semantics.
 * Prefer the connection's explicit host/port/path instead of forcing :443 + /nvst/.
 */
function buildSignalingUrl(
  connection: { ip?: string; port: number; appLevelProtocol?: number; resourcePath?: string } | undefined,
  serverIp: string,
): { signalingUrl: string; signalingHost: string | null; signalingPort: number } {
  const raw = connection?.resourcePath?.trim() || "/nvst";
  const explicitIp = Array.isArray(connection?.ip) ? connection?.ip[0] : connection?.ip;
  const defaultSecure = connection?.appLevelProtocol === 5;
  const defaultPort = connection?.port && connection.port > 0 ? connection.port : 443;

  if (/^(wss?|https?|rtsps?):\/\//i.test(raw)) {
    try {
      const parsed = new URL(raw.replace(/^rtsps:/i, "https:").replace(/^rtsp:/i, "http:"));
      const protocol = raw.startsWith("wss://")
        || raw.startsWith("https://")
        || raw.startsWith("rtsp://")
        || raw.startsWith("rtsps://")
          ? "wss"
          : "ws";
      const host = parsed.hostname && !parsed.hostname.startsWith(".")
        ? parsed.hostname
        : explicitIp || serverIp;
      const port = parsed.port ? Number.parseInt(parsed.port, 10) : defaultPort;
      const path = parsed.pathname && parsed.pathname.length > 0 ? parsed.pathname : "/nvst";
      const search = parsed.search ?? "";
      return {
        signalingUrl: `${protocol}://${host}:${port}${path}${search}`,
        signalingHost: host,
        signalingPort: port,
      };
    } catch {
      // Fall through to path-based handling.
    }
  }

  const protocol = defaultSecure ? "wss" : "ws";
  const host = explicitIp || serverIp;
  const path = raw.startsWith("/") ? raw : `/${raw}`;

  return {
    signalingUrl: `${protocol}://${host}:${defaultPort}${path}`,
    signalingHost: host,
    signalingPort: defaultPort,
  };
}

interface RequestHeadersOptions {
  token: string;
  clientId?: string;
  deviceId?: string;
  includeOrigin?: boolean;
}

function requestHeaders(options: RequestHeadersOptions): Record<string, string> {
  const clientId = options.clientId ?? crypto.randomUUID();
  const deviceId = options.deviceId ?? crypto.randomUUID();

  return buildGfnHeaders({
    authorization: { token: options.token },
    contentType: "application/json",
    clientId,
    clientStreamer: GFN_CLIENT_STREAMER_CLASSIC,
    clientType: GFN_CLIENT_TYPE_NATIVE,
    deviceMake: GFN_DEVICE_MAKE_UNKNOWN,
    deviceModel: GFN_DEVICE_MODEL_UNKNOWN,
    deviceOs: resolveGfnDeviceOs(process.platform),
    deviceType: GFN_DEVICE_TYPE_DESKTOP,
    browserType: GFN_BROWSER_TYPE_CHROME,
    deviceId,
    ...(options.includeOrigin !== false
      ? {
          origin: GFN_PLAY_ORIGIN,
          referer: GFN_PLAY_REFERER,
        }
      : {}),
  });
}

function parseResolution(input: string): { width: number; height: number } {
  const [rawWidth, rawHeight] = input.split("x");
  const width = Number.parseInt(rawWidth ?? "", 10);
  const height = Number.parseInt(rawHeight ?? "", 10);

  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) {
    return { width: 1920, height: 1080 };
  }

  return { width, height };
}

function timezoneOffsetMs(): number {
  return -new Date().getTimezoneOffset() * 60 * 1000;
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function jitterDelayMs(baseMs: number): number {
  const jitter = 0.2 + Math.random() * 0.2;
  return Math.round(baseMs * (1 + jitter));
}

function nextClaimPollDelayMs(previousDelayMs: number): number {
  return Math.min(CLAIM_MAX_DELAY_MS, Math.max(CLAIM_INITIAL_DELAY_MS, Math.round(previousDelayMs * 1.5)));
}

function summarizeJsonText(body: string): string {
  const compact = body.replace(/\s+/g, " ").trim();
  if (!compact) {
    return "empty";
  }
  return compact.length > 240 ? `${compact.slice(0, 240)}…` : compact;
}

function summarizeCloudMatchPayload(payload: CloudMatchResponse): string {
  const queuePosition = extractQueuePosition(payload);
  const session = payload.session;
  return [
    `statusCode=${payload.requestStatus.statusCode}`,
    `status=${session.status}`,
    `queue=${queuePosition ?? "n/a"}`,
    `seatStep=${extractSeatSetupStep(payload) ?? "n/a"}`,
    `errorCode=${session.errorCode ?? "n/a"}`,
    `connections=${session.connectionInfo?.length ?? 0}`,
  ].join(" ");
}

function isReadySessionStatus(status: number): boolean {
  return status === 2 || status === 3;
}

function isTerminalSessionStatus(status: number): boolean {
  return status > 3 && status !== 6;
}

function isRetryablePollError(error: unknown): boolean {
  return error instanceof SessionError ? error.isRetryable() : false;
}

function buildSessionInfoFromPayload(
  sessionData: CloudMatchResponse["session"],
  payload: CloudMatchResponse,
  effectiveServerIp: string,
  pairingId: string,
  iceServers: IceServer[],
  clientId?: string,
  deviceId?: string,
): SessionInfo {
  const signaling = resolveSignaling(payload);
  const mergedRouting = mergeSessionRoutingInfo(sessionData.sessionId, signaling);
  const queuePosition = extractQueuePosition(payload);

  return {
    sessionId: sessionData.sessionId,
    status: sessionData.status,
    queuePosition,
    zone: "",
    streamingBaseUrl: `https://${effectiveServerIp}`,
    serverIp: mergedRouting.routing.serverIp,
    signalingServer: mergedRouting.routing.signalingServer,
    signalingUrl: mergedRouting.routing.signalingUrl,
    pairingId,
    gpuType: sessionData.gpuType,
    iceServers,
    mediaConnectionInfo: mergedRouting.routing.mediaConnectionInfo,
    clientId,
    deviceId,
  };
}

function buildSessionRequestBody(input: SessionCreateRequest): CloudMatchRequest {
  const { width, height } = parseResolution(input.settings.resolution);
  const cq = input.settings.colorQuality;
  // IMPORTANT: hdrEnabled is a SEPARATE toggle from color quality.
  // The Rust reference (cloudmatch.rs) uses settings.hdr_enabled independently.
  // 10-bit color depth does NOT mean HDR — you can have 10-bit SDR.
  // Conflating them caused the server to set up an HDR pipeline, which
  // dynamically downscaled resolution to ~540p.
  const hdrEnabled = false; // No HDR toggle implemented yet; hardcode off like claim body
  const bitDepth = colorQualityBitDepth(cq);
  const chromaFormat = colorQualityChromaFormat(cq);
  const accountLinked = input.accountLinked ?? true;

  return {
    sessionRequestData: {
      appId: input.appId,
      internalTitle: input.internalTitle || null,
      availableSupportedControllers: [],
      networkTestSessionId: null,
      parentSessionId: null,
      clientIdentification: GFN_CLIENT_IDENTIFICATION,
      deviceHashId: crypto.randomUUID(),
      clientVersion: GFN_SESSION_REQUEST_CLIENT_VERSION,
      sdkVersion: GFN_SDK_VERSION,
      streamerVersion: GFN_STREAMER_VERSION,
      clientPlatformName: GFN_CLIENT_PLATFORM_NAME,
      clientRequestMonitorSettings: [
        {
          widthInPixels: width,
          heightInPixels: height,
          framesPerSecond: input.settings.fps,
          sdrHdrMode: hdrEnabled ? 1 : 0,
          displayData: {
            desiredContentMaxLuminance: hdrEnabled ? 1000 : 0,
            desiredContentMinLuminance: 0,
            desiredContentMaxFrameAverageLuminance: hdrEnabled ? 500 : 0,
          },
          dpi: 100,
        },
      ],
      useOps: true,
      audioMode: 2,
      metaData: [
        { key: "SubSessionId", value: crypto.randomUUID() },
        { key: "wssignaling", value: "1" },
        { key: "GSStreamerType", value: GFN_CLIENT_STREAMER_WEBRTC },
        { key: "networkType", value: "Unknown" },
        { key: "ClientImeSupport", value: "0" },
        {
          key: "clientPhysicalResolution",
          value: JSON.stringify({ horizontalPixels: width, verticalPixels: height }),
        },
        { key: "surroundAudioInfo", value: "2" },
      ],
      sdrHdrMode: hdrEnabled ? 1 : 0,
      clientDisplayHdrCapabilities: hdrEnabled
        ? {
            version: 1,
            hdrEdrSupportedFlagsInUint32: 1,
            staticMetadataDescriptorId: 0,
          }
        : null,
      surroundAudioInfo: 0,
      remoteControllersBitmap: 0,
      clientTimezoneOffset: timezoneOffsetMs(),
      enhancedStreamMode: GFN_ENHANCED_STREAM_MODE,
      appLaunchMode: GFN_APP_LAUNCH_MODE,
      secureRTSPSupported: false,
      partnerCustomData: "",
      accountLinked,
      enablePersistingInGameSettings: true,
      userAge: 26,
      requestedStreamingFeatures: {
        reflex: input.settings.fps >= 120,
        bitDepth,
        cloudGsync: false,
        enabledL4S: input.settings.enableL4S,
        mouseMovementFlags: 0,
        trueHdr: hdrEnabled,
        supportedHidDevices: 0,
        profile: 0,
        fallbackToLogicalResolution: false,
        hidDevices: null,
        chromaFormat,
        prefilterMode: 0,
        prefilterSharpness: 0,
        prefilterNoiseReduction: 0,
        hudStreamingMode: 0,
        sdrColorSpace: 2,
        hdrColorSpace: hdrEnabled ? 4 : 0,
      },
    },
  };
}

function cloudmatchUrl(zone: string): string {
  return `https://${zone}.cloudmatchbeta.nvidiagrid.net`;
}

function resolveStreamingBaseUrl(zone: string, provided?: string): string {
  if (provided && provided.trim()) {
    const trimmed = provided.trim();
    return trimmed.endsWith("/") ? trimmed.slice(0, -1) : trimmed;
  }
  return cloudmatchUrl(zone);
}

function shouldUseServerIp(baseUrl: string): boolean {
  return baseUrl.includes("cloudmatchbeta.nvidiagrid.net");
}

function resolvePollStopBase(zone: string, provided?: string, serverIp?: string): string {
  const base = resolveStreamingBaseUrl(zone, provided);
  // Only use serverIp if it's a real server IP (not a zone hostname).
  // The Rust version checks: if we're NOT an alliance partner AND we have a server_ip, use it.
  // But if the "serverIp" is actually the zone hostname (from an early poll when connectionInfo
  // was empty), using it is circular and doesn't help.
  if (serverIp && shouldUseServerIp(base) && !isZoneHostname(serverIp)) {
    return `https://${serverIp}`;
  }
  return base;
}

function toPositiveInt(value: unknown): number | undefined {
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

function extractQueuePosition(payload: CloudMatchResponse): number | undefined {
  const direct = toPositiveInt(payload.session.queuePosition);
  if (direct !== undefined) {
    return direct;
  }

  const seatSetup = payload.session.seatSetupInfo;
  if (seatSetup) {
    const nested = toPositiveInt(seatSetup.queuePosition);
    if (nested !== undefined) {
      return nested;
    }
  }

  const nestedSessionProgress = payload.session.sessionProgress;
  if (nestedSessionProgress) {
    const nested = toPositiveInt(nestedSessionProgress.queuePosition);
    if (nested !== undefined) {
      return nested;
    }
  }

  const nestedProgressInfo = payload.session.progressInfo;
  if (nestedProgressInfo) {
    const nested = toPositiveInt(nestedProgressInfo.queuePosition);
    if (nested !== undefined) {
      return nested;
    }
  }

  return undefined;
}

function extractSeatSetupStep(payload: CloudMatchResponse): number | undefined {
  const raw = payload.session.seatSetupInfo?.seatSetupStep;
  if (typeof raw === "number" && Number.isFinite(raw)) {
    return Math.trunc(raw);
  }
  return undefined;
}

interface ToSessionInfoOptions {
  zone: string;
  streamingBaseUrl: string;
  payload: CloudMatchResponse;
  clientId?: string;
  deviceId?: string;
}

async function toSessionInfo(options: ToSessionInfoOptions): Promise<SessionInfo> {
  const { zone, streamingBaseUrl, payload, clientId, deviceId } = options;
  if (payload.requestStatus.statusCode !== 1) {
    // Use SessionError for parsing error responses
    const errorJson = JSON.stringify(payload);
    throw SessionError.fromResponse(200, errorJson);
  }

  const signaling = resolveSignaling(payload);
  const mergedRouting = mergeSessionRoutingInfo(payload.session.sessionId, signaling);
  const queuePosition = extractQueuePosition(payload);
  const seatSetupStep = extractSeatSetupStep(payload);
  if (mergedRouting.retainedFields.length > 0) {
    console.log(
      `[CloudMatch] Session routing retained session=${payload.session.sessionId} fields=${mergedRouting.retainedFields.join(",")}`,
    );
  }
  console.log(
    `[CloudMatch] Session info ready ${summarizeCloudMatchPayload(payload)} ` +
    `serverIp=${mergedRouting.routing.serverIp} signalingHost=${mergedRouting.routing.signalingServer}`,
  );

  return {
    sessionId: payload.session.sessionId,
    status: payload.session.status,
    seatSetupStep,
    queuePosition,
    zone,
    streamingBaseUrl,
    serverIp: mergedRouting.routing.serverIp,
    signalingServer: mergedRouting.routing.signalingServer,
    signalingUrl: mergedRouting.routing.signalingUrl,
    pairingId: payload.session.sessionId,
    gpuType: payload.session.gpuType,
    iceServers: await normalizeIceServers(payload),
    mediaConnectionInfo: mergedRouting.routing.mediaConnectionInfo,
    clientId,
    deviceId,
  };
}

export async function createSession(input: SessionCreateRequest): Promise<SessionInfo> {
  if (!input.token) {
    throw new Error("Missing token for session creation");
  }

  if (!/^\d+$/.test(input.appId)) {
    throw new Error(`Invalid launch appId '${input.appId}' (must be numeric)`);
  }

  // Generate client/device IDs once for the entire session lifecycle
  const clientId = crypto.randomUUID();
  const deviceId = crypto.randomUUID();

  const body = buildSessionRequestBody(input);

  const base = resolveStreamingBaseUrl(input.zone, input.streamingBaseUrl);
  const languageCode = input.settings.gameLanguage ?? "en_US";
  const url = `${base}/v2/session?keyboardLayout=en-US&languageCode=${languageCode}`;
  const response = await fetch(url, {
    method: "POST",
    headers: requestHeaders({ token: input.token, clientId, deviceId, includeOrigin: true }),
    body: JSON.stringify(body),
  });

  const text = await response.text();
  if (!response.ok) {
    // Use SessionError to parse and throw detailed error
    throw SessionError.fromResponse(response.status, text);
  }

  const payload = JSON.parse(text) as CloudMatchResponse;
  return await toSessionInfo({ zone: input.zone, streamingBaseUrl: base, payload, clientId, deviceId });
}

export async function pollSession(input: SessionPollRequest): Promise<SessionInfo> {
  if (!input.token) {
    throw new Error("Missing token for session polling");
  }

  // Use provided client/device IDs if available (should match session creation)
  const clientId = input.clientId ?? crypto.randomUUID();
  const deviceId = input.deviceId ?? crypto.randomUUID();

  const base = resolvePollStopBase(input.zone, input.streamingBaseUrl, input.serverIp);
  const url = `${base}/v2/session/${input.sessionId}`;
  // Polling should NOT include Origin/Referer headers (matches claimSession polling pattern)
  const headers = requestHeaders({ token: input.token, clientId, deviceId, includeOrigin: false });
  const response = await fetch(url, {
    method: "GET",
    headers,
  });

  const text = await response.text();
  if (!response.ok) {
    throw SessionError.fromResponse(response.status, text);
  }

  const payload = JSON.parse(text) as CloudMatchResponse;

  // Match Rust behavior: if the poll was routed through the zone load balancer
  // and the response now contains a real server IP in connectionInfo, re-poll
  // directly via the real server IP. This ensures the signaling data and
  // connection info are correct (the zone LB may return different data than
  // a direct server poll).
  const realServerIp = streamingServerIp(payload);
  const polledViaZone = isZoneHostname(new URL(base).hostname);
  const realIpDiffers =
    realServerIp &&
    realServerIp.length > 0 &&
    !isZoneHostname(realServerIp) &&
    realServerIp !== input.serverIp;

  if (polledViaZone && realIpDiffers) {
    // We now know a real server IP from the zone LB response; re-poll directly so
    // queue/setup flows can recover better routing metadata earlier than ready state.
    console.log(
      `[CloudMatch] Session routing upgrade: re-polling via real server IP ${realServerIp} (was: ${new URL(base).hostname}, status=${payload.session.status})`,
    );
    const directBase = `https://${realServerIp}`;
    const directUrl = `${directBase}/v2/session/${input.sessionId}`;
    try {
      const directResponse = await fetch(directUrl, {
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

export async function stopSession(input: SessionStopRequest): Promise<void> {
  if (!input.token) {
    throw new Error("Missing token for session stop");
  }

  // Use provided client/device IDs if available (should match session creation)
  const clientId = input.clientId ?? crypto.randomUUID();
  const deviceId = input.deviceId ?? crypto.randomUUID();

  const base = resolvePollStopBase(input.zone, input.streamingBaseUrl, input.serverIp);
  const url = `${base}/v2/session/${input.sessionId}`;
  const response = await fetch(url, {
    method: "DELETE",
    headers: requestHeaders({ token: input.token, clientId, deviceId, includeOrigin: false }),
  });

  if (!response.ok) {
    const text = await response.text();
    // Use SessionError to parse and throw detailed error
    throw SessionError.fromResponse(response.status, text);
  }

  sessionRoutingCache.delete(input.sessionId);
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

  const base = streamingBaseUrl.trim().endsWith("/")
    ? streamingBaseUrl.trim().slice(0, -1)
    : streamingBaseUrl.trim();
  const url = `${base}/v2/session`;

  const response = await fetch(url, {
    method: "GET",
    headers: requestHeaders({ token, includeOrigin: false }),
  });

  const text = await response.text();

  if (!response.ok) {
    // Return empty list on failure (matching Rust behavior)
    console.warn(`Get sessions failed: ${response.status} - ${text.slice(0, 200)}`);
    return [];
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

  // Filter active sessions (status 2 = Ready, status 3 = Streaming)
  const activeSessions: ActiveSessionInfo[] = sessionsResponse.sessions
    .filter((s) => s.status === 2 || s.status === 3)
    .map((s) => {
      // Extract appId from sessionRequestData
      const appId = s.sessionRequestData?.appId ? Number(s.sessionRequestData.appId) : 0;

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
        appId,
        gpuType: s.gpuType,
        status: s.status,
        serverIp,
        signalingUrl,
        resolution,
        fps,
      };
    });

  return activeSessions;
}

/**
 * Build claim/resume request payload
 */
function buildClaimRequestBody(sessionId: string, appId: string, settings: StreamSettings): unknown {
  // For RESUME claims, we must NOT attempt to renegotiate streaming parameters.
  // The session is already configured on the server side. Sending different fps, resolution,
  // codec, etc. causes HTTP 400 from the server because those parameters are immutable for
  // an already-streaming session. Only send the action and minimal required fields.
  const deviceId = crypto.randomUUID();
  const subSessionId = crypto.randomUUID();
  const timezoneMs = timezoneOffsetMs();

  return {
    action: 2,
    data: "RESUME",
    sessionRequestData: {
      // Minimal fields required for resume - NO streaming parameter renegotiation
      audioMode: 2,
      remoteControllersBitmap: 0,
      sdrHdrMode: 0,
      networkTestSessionId: null,
      availableSupportedControllers: [],
      clientVersion: GFN_SESSION_REQUEST_CLIENT_VERSION,
      deviceHashId: deviceId,
      internalTitle: null,
      clientPlatformName: GFN_CLIENT_PLATFORM_NAME,
      metaData: [
        { key: "SubSessionId", value: subSessionId },
        { key: "wssignaling", value: "1" },
        { key: "GSStreamerType", value: GFN_CLIENT_STREAMER_WEBRTC },
        { key: "networkType", value: "Unknown" },
        { key: "ClientImeSupport", value: "0" },
      ],
      surroundAudioInfo: 0,
      clientTimezoneOffset: timezoneMs,
      clientIdentification: GFN_CLIENT_IDENTIFICATION,
      parentSessionId: null,
      appId: parseInt(appId, 10),
      streamerVersion: GFN_STREAMER_VERSION,
      appLaunchMode: GFN_APP_LAUNCH_MODE,
      sdkVersion: GFN_SDK_VERSION,
      enhancedStreamMode: GFN_ENHANCED_STREAM_MODE,
      useOps: true,
      clientDisplayHdrCapabilities: null,
      accountLinked: true,
      partnerCustomData: "",
      enablePersistingInGameSettings: true,
      secureRTSPSupported: false,
      userAge: 26,
      requestedStreamingFeatures: {
        reflex: false,
        bitDepth: 0,
        cloudGsync: false,
        enabledL4S: false,
        profile: 0,
        fallbackToLogicalResolution: false,
        chromaFormat: 0,
        prefilterMode: 0,
        hudStreamingMode: 0,
      },
    },
    metaData: [],
  };
}

/**
 * Claim/Resume an existing session
 * Required before connecting to an existing session
 */
export async function claimSession(input: SessionClaimRequest): Promise<SessionInfo> {
  if (!input.token) {
    throw new Error("Missing token for session claim");
  }

  const deviceId = crypto.randomUUID();
  const clientId = crypto.randomUUID();
  const pairingId = input.sessionId;

  // Provide default values for optional parameters
  const appId = input.appId ?? "0";
  const settings = input.settings ?? {
    resolution: "1920x1080",
    fps: 60,
    maxBitrateMbps: 75,
    codec: "H264",
    colorQuality: "8bit_420",
    gameLanguage: "en_US",
    enableL4S: false,
  };

  const languageCode = settings.gameLanguage ?? "en_US";

  // The session list endpoint returns the zone LB hostname in sessionControlInfo.ip.
  // A claim PUT sent to the zone LB returns HTTP 400 because it does not handle
  // session-level mutations. The real game server IP is only reliably available from
  // the individual session endpoint (GET /v2/session/{id}). Resolve it here before
  // building the claim URL.
  // IMPORTANT: We must query the SAME zone LB where the session is hosted (use serverIp),
  // not the provider's generic streamingBaseUrl (which may route to a different zone LB).
  let effectiveServerIp = input.serverIp;
  console.log(
    `[CloudMatch] claimSession session=${input.sessionId} server=${input.serverIp} zoneHost=${isZoneHostname(input.serverIp)}`,
  );
  if (isZoneHostname(effectiveServerIp)) {
    const zoneBase = `https://${effectiveServerIp}`;
    const prefetchUrl = `${zoneBase}/v2/session/${input.sessionId}`;
    const prefetchHeaders = requestHeaders({ token: input.token, clientId, deviceId, includeOrigin: false });
    try {
      const prefetchResp = await fetch(prefetchUrl, { method: "GET", headers: prefetchHeaders });
      if (prefetchResp.ok) {
        const prefetchText = await prefetchResp.text();
        const prefetchPayload = JSON.parse(prefetchText) as CloudMatchResponse;
        const realIp = streamingServerIp(prefetchPayload);
        console.log(
          `[CloudMatch] claimSession preflight ${summarizeCloudMatchPayload(prefetchPayload)} realServer=${realIp ?? "n/a"}`,
        );
        if (realIp) {
          effectiveServerIp = realIp;
          console.log(
            `[CloudMatch] claimSession using preflight server ${realIp} direct=${!isZoneHostname(realIp)}`,
          );
        }
      } else {
        console.warn(
          `[CloudMatch] claimSession preflight HTTP ${prefetchResp.status}: ${summarizeJsonText(await prefetchResp.text())}`,
        );
      }
    } catch (error) {
      console.warn("[CloudMatch] claimSession preflight failed; proceeding with original server", error);
    }
  }

  const claimUrl = `https://${effectiveServerIp}/v2/session/${input.sessionId}?keyboardLayout=en-US&languageCode=${languageCode}`;

  // Pre-claim validation: verify the session is still alive and in ready state before attempting claim
  // This prevents sending a claim to an expired/dead session
  try {
    const validationUrl = `https://${effectiveServerIp}/v2/session/${input.sessionId}`;
    const validationHeaders = requestHeaders({ token: input.token, clientId, deviceId, includeOrigin: false });
    const validationResp = await fetch(validationUrl, { method: "GET", headers: validationHeaders });
    if (validationResp.ok) {
      const validationText = await validationResp.text();
      const validationPayload = JSON.parse(validationText) as CloudMatchResponse;
      const sessionStatus = validationPayload.session?.status ?? 0;
      const errorCode = validationPayload.session?.errorCode ?? 0;
      console.log(
        `[CloudMatch] claimSession validation ${summarizeCloudMatchPayload(validationPayload)} errorCode=${errorCode}`,
      );
      if (!isReadySessionStatus(sessionStatus)) {
        console.warn(
          `[CloudMatch] claimSession validation not ready yet (status=${sessionStatus}); continuing resume request`,
        );
      }
    } else {
      console.warn(`[CloudMatch] claimSession validation HTTP ${validationResp.status}`);
    }
  } catch (error) {
    console.warn("[CloudMatch] claimSession validation failed", error);
  }

  const payload = buildClaimRequestBody(input.sessionId, appId, settings);
  const headers = requestHeaders({ token: input.token, clientId, deviceId, includeOrigin: true });

  // Send claim request
  console.log(
    `[CloudMatch] claimSession PUT host=${new URL(claimUrl).host} session=${input.sessionId} language=${languageCode}`,
  );
  const response = await fetch(claimUrl, {
    method: "PUT",
    headers,
    body: JSON.stringify(payload),
  });

  const text = await response.text();
  console.log(`[CloudMatch] claimSession response HTTP ${response.status}: ${summarizeJsonText(text)}`);

  if (!response.ok) {
    throw SessionError.fromResponse(response.status, text);
  }

  const apiResponse = JSON.parse(text) as CloudMatchResponse;

  if (apiResponse.requestStatus.statusCode !== 1) {
    throw SessionError.fromResponse(200, text);
  }

  const getUrl = `https://${effectiveServerIp}/v2/session/${input.sessionId}`;
  const pollHeaders = requestHeaders({ token: input.token, clientId, deviceId, includeOrigin: false });
  const deadlineAt = Date.now() + CLAIM_POLL_TIMEOUT_MS;
  let attempt = 0;
  let nextDelayMs = CLAIM_INITIAL_DELAY_MS;
  let lastStatus: number | undefined;
  let lastQueuePosition: number | undefined;
  let lastRetryableError: SessionError | null = null;

  while (Date.now() < deadlineAt) {
    attempt += 1;
    if (attempt > 1) {
      await sleep(jitterDelayMs(nextDelayMs));
      nextDelayMs = nextClaimPollDelayMs(nextDelayMs);
    }

    let pollText = "";
    try {
      const pollResponse = await fetch(getUrl, {
        method: "GET",
        headers: pollHeaders,
      });
      pollText = await pollResponse.text();

      if (!pollResponse.ok) {
        const pollError = SessionError.fromResponse(pollResponse.status, pollText);
        if (pollError.isRetryable()) {
          lastRetryableError = pollError;
          console.warn(
            `[CloudMatch] claim poll retryable HTTP ${pollResponse.status} attempt=${attempt} type=${pollError.errorType}`,
          );
          continue;
        }
        throw pollError;
      }
    } catch (error) {
      if (isRetryablePollError(error)) {
        lastRetryableError = error as SessionError;
        console.warn(
          `[CloudMatch] claim poll retryable error attempt=${attempt} type=${lastRetryableError.errorType}`,
        );
        continue;
      }
      throw error;
    }

    let pollApiResponse: CloudMatchResponse;
    try {
      pollApiResponse = JSON.parse(pollText) as CloudMatchResponse;
    } catch {
      console.warn(`[CloudMatch] claim poll parse failure attempt=${attempt}`);
      continue;
    }

    if (pollApiResponse.requestStatus.statusCode !== 1) {
      const pollError = SessionError.fromResponse(200, pollText);
      if (pollError.isRetryable()) {
        lastRetryableError = pollError;
        console.warn(
          `[CloudMatch] claim poll retryable API error attempt=${attempt} type=${pollError.errorType}`,
        );
        continue;
      }
      throw pollError;
    }

    const sessionData = pollApiResponse.session;
    const queuePosition = extractQueuePosition(pollApiResponse);

    if (sessionData.status !== lastStatus || queuePosition !== lastQueuePosition || attempt === 1) {
      console.log(
        `[CloudMatch] claim poll attempt=${attempt} ${summarizeCloudMatchPayload(pollApiResponse)}`,
      );
      lastStatus = sessionData.status;
      lastQueuePosition = queuePosition;
    }

    if (isReadySessionStatus(sessionData.status)) {
      const iceServers = await normalizeIceServers(pollApiResponse);
      return buildSessionInfoFromPayload(
        sessionData,
        pollApiResponse,
        effectiveServerIp,
        pairingId,
        iceServers,
        clientId,
        deviceId,
      );
    }

    if (isTerminalSessionStatus(sessionData.status)) {
      throw SessionError.fromResponse(200, pollText);
    }
  }

  const retryDetail = lastRetryableError
    ? ` lastRetryable=${lastRetryableError.errorType}`
    : "";
  throw new Error(
    `Session did not become ready after claim within ${CLAIM_POLL_TIMEOUT_MS}ms ` +
    `(attempts=${attempt}, lastStatus=${lastStatus ?? "unknown"}, queue=${lastQueuePosition ?? "n/a"})${retryDetail}`,
  );
}
