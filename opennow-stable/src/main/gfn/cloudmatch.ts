import crypto from "node:crypto";
import dns from "node:dns";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { createRequire } from "node:module";

import type {
  ActiveSessionInfo,
  ColorQuality,
  NegotiatedStreamProfile,
  IceServer,
  SessionAdAction,
  SessionAdInfo,
  SessionAdReportRequest,
  SessionAdState,
  SessionClaimRequest,
  SessionCreateRequest,
  SessionInfo,
  SessionPollRequest,
  SessionStopRequest,
  StreamSettings,
} from "@shared/gfn";

import {
  DEFAULT_KEYBOARD_LAYOUT,
  colorQualityBitDepth,
  colorQualityChromaFormat,
  resolveGfnKeyboardLayout,
} from "@shared/gfn";
import { DEFAULT_MINIMUM_FPS_FOR_REFLEX_WITHOUT_VRR } from "@shared/cloudGsync";

import type { CloudMatchRequest, CloudMatchResponse, GetSessionsResponse } from "./types";
import { SessionError } from "./errorCodes";
import { fetchWithOptionalProxy } from "./proxyFetch";

const GFN_USER_AGENT =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36 NVIDIACEFClient/HEAD/debb5919f6 GFN-PC/2.0.80.173";
const GFN_CLIENT_VERSION = "2.0.80.173";
const SESSION_MODIFY_ACTION_AD_UPDATE = 6;
const READY_SESSION_STATUSES = new Set([2, 3]);
const GFN_DEVICE_ID_FILENAME = "gfn-device-id.json";

let cachedStableDeviceId: string | null = null;
const require = createRequire(import.meta.url);

function getElectronApp(): Electron.App | null {
  try {
    return require("electron").app ?? null;
  } catch {
    return null;
  }
}

const AD_ACTION_CODES: Record<SessionAdAction, number> = {
  start: 1,
  pause: 2,
  resume: 3,
  finish: 4,
  cancel: 5,
};

const GFN_AD_MEDIA_PROFILE_ORDER = new Map<string, number>([
  ["mp4deinterlaced720p", 0],
  ["webm", 1],
  ["hlsadaptive", 2],
]);

export function buildRequestedStreamingFeatures(
  settings: StreamSettings,
  bitDepth: number,
  chromaFormat: number,
  hdrEnabled: boolean,
): CloudMatchRequest["sessionRequestData"]["requestedStreamingFeatures"] {
  const cloudGsync = settings.enableCloudGsync;

  return {
    reflex: shouldRequestReflex(settings),
    bitDepth,
    cloudGsync,
    enabledL4S: settings.enableL4S,
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
  };
}

export function shouldRequestReflex(settings: StreamSettings): boolean {
  if (typeof settings.cloudGsyncResolution?.reflexEnabled === "boolean") {
    return settings.cloudGsyncResolution.reflexEnabled;
  }

  const reflexMinimum =
    settings.cloudGsyncResolution?.capabilities.minimumFpsForReflexWithoutVrr
    ?? DEFAULT_MINIMUM_FPS_FOR_REFLEX_WITHOUT_VRR;
  return settings.enableCloudGsync || settings.fps >= reflexMinimum;
}

function isReadySessionStatus(status: number): boolean {
  return READY_SESSION_STATUSES.has(status);
}

function getStableDeviceId(): string {
  if (cachedStableDeviceId) {
    return cachedStableDeviceId;
  }

  try {
    const electronApp = getElectronApp();
    if (!electronApp) {
      throw new Error("Electron app is unavailable outside the main process.");
    }
    const path = join(electronApp.getPath("userData"), GFN_DEVICE_ID_FILENAME);
    if (existsSync(path)) {
      const parsed = JSON.parse(readFileSync(path, "utf-8")) as { deviceId?: unknown };
      if (typeof parsed.deviceId === "string" && parsed.deviceId.length > 0) {
        cachedStableDeviceId = parsed.deviceId;
        return parsed.deviceId;
      }
    }

    const deviceId = crypto.randomUUID();
    writeFileSync(path, JSON.stringify({ deviceId }, null, 2), "utf-8");
    cachedStableDeviceId = deviceId;
    return deviceId;
  } catch (error) {
    // Fallback to in-memory UUID if disk read/write fails.
    const fallback = crypto.randomUUID();
    cachedStableDeviceId = fallback;
    console.warn("[CloudMatch] Failed to load persisted device ID, using in-memory fallback:", error);
    return fallback;
  }
}

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
  const prefixes = ["rtsps://", "rtsp://", "wss://", "https://"];
  let afterProto: string | null = null;
  for (const prefix of prefixes) {
    if (url.startsWith(prefix)) {
      afterProto = url.slice(prefix.length);
      break;
    }
  }
  if (!afterProto) return null;

  // Get host (before port or path)
  const host = afterProto.split(":")[0]?.split("/")[0];
  if (!host || host.length === 0 || host.startsWith(".")) return null;
  return host;
}

/**
 * Check if a given IP/hostname is a CloudMatch zone load balancer hostname
 * (not a real game server IP). Zone hostnames look like:
 *   np-ams-06.cloudmatchbeta.nvidiagrid.net
 */
function isZoneHostname(ip: string): boolean {
  return ip.includes("cloudmatchbeta.nvidiagrid.net") || ip.includes("cloudmatch.nvidiagrid.net");
}

function resolveSignaling(response: CloudMatchResponse): {
  serverIp: string;
  signalingServer: string;
  signalingUrl: string;
  mediaConnectionInfo?: { ip: string; port: number };
} {
  const connections = response.session.connectionInfo ?? [];
  const signalingConnection =
    connections.find((conn) => conn.usage === 14 && conn.ip) ?? connections.find((conn) => conn.ip);

  // Use the Rust-matching priority chain for server IP
  const serverIp = streamingServerIp(response);
  if (!serverIp) {
    throw new Error("CloudMatch response did not include a signaling host");
  }

  const resourcePath = signalingConnection?.resourcePath ?? "/nvst/";

  // Build signaling URL matching Rust's build_signaling_url() behavior:
  // - rtsps://host:port -> extract host, convert to wss://host/nvst/
  // - wss://... -> use as-is
  // - /path -> wss://serverIp:443/path
  // - fallback -> wss://serverIp:443/nvst/
  const { signalingUrl, signalingHost } = buildSignalingUrl(resourcePath, serverIp);

  // Use the resolved signaling host (which may differ from serverIp if extracted from rtsps:// URL)
  const effectiveHost = signalingHost ?? serverIp;
  const signalingServer = effectiveHost.includes(":")
    ? effectiveHost
    : `${effectiveHost}:443`;

  return {
    serverIp,
    signalingServer,
    signalingUrl,
    mediaConnectionInfo: resolveMediaConnectionInfo(connections, serverIp, {
      logMissing: isReadySessionStatus(response.session.status),
    }),
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
  options?: { logMissing?: boolean },
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

  if (options?.logMissing ?? true) {
    console.log("[CloudMatch] resolveMediaConnectionInfo: NO valid media connection info found");
  }
  return undefined;
}

/**
 * Build signaling WSS URL from the resourcePath, matching Rust implementation.
 * Returns the URL and optionally the extracted host (if different from serverIp).
 */
function buildSignalingUrl(
  raw: string,
  serverIp: string,
): { signalingUrl: string; signalingHost: string | null } {
  if (raw.startsWith("rtsps://") || raw.startsWith("rtsp://")) {
    // Extract hostname from RTSP URL, convert to wss://
    const withoutScheme = raw.startsWith("rtsps://")
      ? raw.slice("rtsps://".length)
      : raw.slice("rtsp://".length);
    const host = withoutScheme.split(":")[0]?.split("/")[0];
    if (host && host.length > 0 && !host.startsWith(".")) {
      return {
        signalingUrl: `wss://${host}/nvst/`,
        signalingHost: host,
      };
    }
    return {
      signalingUrl: `wss://${serverIp}:443/nvst/`,
      signalingHost: null,
    };
  }

  if (raw.startsWith("wss://")) {
    // Already a full WSS URL, use as-is; extract host
    const withoutScheme = raw.slice("wss://".length);
    const host = withoutScheme.split("/")[0] ?? null;
    return { signalingUrl: raw, signalingHost: host };
  }

  if (raw.startsWith("/")) {
    // Relative path
    return {
      signalingUrl: `wss://${serverIp}:443${raw}`,
      signalingHost: null,
    };
  }

  // Fallback
  return {
    signalingUrl: `wss://${serverIp}:443/nvst/`,
    signalingHost: null,
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

  const headers: Record<string, string> = {
    "User-Agent": GFN_USER_AGENT,
    Authorization: `GFNJWT ${options.token}`,
    "Content-Type": "application/json",
    "nv-browser-type": "CHROME",
    "nv-client-id": clientId,
    "nv-client-streamer": "NVIDIA-CLASSIC",
    "nv-client-type": "NATIVE",
    "nv-client-version": GFN_CLIENT_VERSION,
    "nv-device-make": "UNKNOWN",
    "nv-device-model": "UNKNOWN",
    "nv-device-os": process.platform === "win32" ? "WINDOWS" : process.platform === "darwin" ? "MACOS" : "LINUX",
    "nv-device-type": "DESKTOP",
    "x-device-id": deviceId,
  };

  if (options.includeOrigin !== false) {
    headers["Origin"] = "https://play.geforcenow.com";
    headers["Referer"] = "https://play.geforcenow.com/";
  }

  return headers;
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

function webRtcSessionMetadata(width: number, height: number): Array<{ key: string; value: string }> {
  return [
    { key: "SubSessionId", value: crypto.randomUUID() },
    { key: "wssignaling", value: "1" },
    { key: "GSStreamerType", value: "WebRTC" },
    { key: "networkType", value: "Unknown" },
    { key: "ClientImeSupport", value: "0" },
    {
      key: "clientPhysicalResolution",
      value: JSON.stringify({ horizontalPixels: width, verticalPixels: height }),
    },
    { key: "surroundAudioInfo", value: "2" },
  ];
}

function buildSessionRequestBody(input: SessionCreateRequest, deviceHashId: string): CloudMatchRequest {
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
      clientIdentification: "GFN-PC",
      // Keep device identity stable across create -> reconnect/resume flows.
      // The official client preserves this identity, and resume reliability depends on it.
      deviceHashId,
      clientVersion: "30.0",
      sdkVersion: "1.0",
      streamerVersion: 1,
      clientPlatformName: "windows",
      clientRequestMonitorSettings: [
        {
          monitorId: 0,
          positionX: 0,
          positionY: 0,
          widthInPixels: width,
          heightInPixels: height,
          framesPerSecond: input.settings.fps,
          sdrHdrMode: hdrEnabled ? 1 : 0,
          displayData: hdrEnabled
            ? {
                desiredContentMaxLuminance: 1000,
                desiredContentMinLuminance: 0,
                desiredContentMaxFrameAverageLuminance: 500,
              }
            : null,
          hdr10PlusGamingData: null,
          dpi: 100,
        },
      ],
      useOps: true,
      audioMode: 2,
      metaData: webRtcSessionMetadata(width, height),
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
      enhancedStreamMode: 1,
      appLaunchMode: 1,
      secureRTSPSupported: false,
      partnerCustomData: "",
      accountLinked,
      enablePersistingInGameSettings: true,
      userAge: 26,
      requestedStreamingFeatures: buildRequestedStreamingFeatures(
        input.settings,
        bitDepth,
        chromaFormat,
        hdrEnabled,
      ),
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

function toBoolean(value: unknown): boolean | undefined {
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

function toOptionalString(value: unknown): string | undefined {
  if (typeof value !== "string") {
    return undefined;
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : undefined;
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

function normalizeSessionAdInfo(ad: NonNullable<CloudMatchResponse["session"]["sessionAds"]>[number], index: number): SessionAdInfo | null {
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

function extractAdState(payload: CloudMatchResponse): SessionAdState | undefined {
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

function toColorQuality(bitDepth?: number, chromaFormat?: number): ColorQuality | undefined {
  if (bitDepth !== 0 && bitDepth !== 10) {
    return undefined;
  }
  if (chromaFormat !== 0 && chromaFormat !== 2) {
    return undefined;
  }

  if (bitDepth === 10) {
    return chromaFormat === 2 ? "10bit_444" : "10bit_420";
  }

  return chromaFormat === 2 ? "8bit_444" : "8bit_420";
}

function extractNegotiatedStreamProfile(payload: CloudMatchResponse): NegotiatedStreamProfile | undefined {
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
  const queuePosition = extractQueuePosition(payload);
  const seatSetupStep = extractSeatSetupStep(payload);
  const adState = extractAdState(payload);
  const negotiatedStreamProfile = extractNegotiatedStreamProfile(payload);

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
    `connectionInfo=${connections.length} entries, ` +
    `serverIp=${signaling.serverIp}, ` +
    `signalingServer=${signaling.signalingServer}, ` +
    `signalingUrl=${signaling.signalingUrl}, ` +
    `connections=[${connectionSummary}]`,
  );
  console.log(
    `[CloudMatch] negotiated streaming features: cloudGsync=${negotiatedStreamProfile?.enableCloudGsync ?? "n/a"}, reflex=${negotiatedStreamProfile?.enableReflex ?? "n/a"}, l4s=${negotiatedStreamProfile?.enableL4S ?? "n/a"}`,
  );

  return {
    sessionId: payload.session.sessionId,
    status: payload.session.status,
    seatSetupStep,
    queuePosition,
    adState,
    zone,
    streamingBaseUrl,
    serverIp: signaling.serverIp,
    signalingServer: signaling.signalingServer,
    signalingUrl: signaling.signalingUrl,
    gpuType: payload.session.gpuType,
    iceServers: await normalizeIceServers(payload),
    mediaConnectionInfo: signaling.mediaConnectionInfo,
    negotiatedStreamProfile,
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
  const deviceId = getStableDeviceId();

  const body = buildSessionRequestBody(input, deviceId);

  const base = resolveStreamingBaseUrl(input.zone, input.streamingBaseUrl);
  const keyboardLayout = resolveGfnKeyboardLayout(input.settings.keyboardLayout ?? DEFAULT_KEYBOARD_LAYOUT, process.platform);
  const languageCode = input.settings.gameLanguage ?? "en_US";
  const url = `${base}/v2/session?${new URLSearchParams({ keyboardLayout, languageCode }).toString()}`;
  const response = await fetchWithOptionalProxy(url, {
    method: "POST",
    headers: requestHeaders({ token: input.token, clientId, deviceId, includeOrigin: true }),
    body: JSON.stringify(body),
  }, input.proxyUrl);

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
  const baseHost = new URL(base).hostname;
  const pollProxyUrl = isZoneHostname(baseHost) ? input.proxyUrl : undefined;
  const url = `${base}/v2/session/${input.sessionId}`;
  // Polling should NOT include Origin/Referer headers (matches claimSession polling pattern)
  const headers = requestHeaders({ token: input.token, clientId, deviceId, includeOrigin: false });
  const response = await fetchWithOptionalProxy(url, {
    method: "GET",
    headers,
  }, pollProxyUrl);

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

export async function reportSessionAd(input: SessionAdReportRequest): Promise<SessionInfo> {
  if (!input.token) {
    throw new Error("Missing token for ad update");
  }

  const clientId = input.clientId ?? crypto.randomUUID();
  const deviceId = input.deviceId ?? crypto.randomUUID();
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

  const response = await fetch(url, {
    method: "PUT",
    // Official browser requests include Origin/Referer on cross-origin ad updates.
    headers: requestHeaders({ token: input.token, clientId, deviceId, includeOrigin: true }),
    body: JSON.stringify(requestBody),
  });

  const text = await response.text();
  if (!response.ok) {
    console.warn(
      `[CloudMatch] reportSessionAd: backend error status=${response.status}, sessionId=${input.sessionId}, ` +
        `adId=${input.adId}, action=${input.action}, body=${text.slice(0, 500)}`,
    );
    throw SessionError.fromResponse(response.status, text);
  }

  const payload = JSON.parse(text) as CloudMatchResponse;
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

  // Filter active sessions:
  //   1 = Setup/Queuing (counts against SESSION_LIMIT — must be included for resume logic)
  //   2 = Ready
  //   3 = Streaming
  const activeSessions: ActiveSessionInfo[] = sessionsResponse.sessions
    .filter((s) => s.status === 1 || s.status === 2 || s.status === 3)
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
 * Build claim/resume request payload
 */
function buildClaimRequestBody(sessionId: string, appId: string, settings: StreamSettings): unknown {
  // For RESUME claims, we must NOT attempt to renegotiate streaming parameters.
  // The session is already configured on the server side. Sending different fps, resolution,
  // codec, etc. causes HTTP 400 from the server because those parameters are immutable for
  // an already-streaming session. Only send the action and minimal required fields.
  const deviceId = getStableDeviceId();
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
      clientVersion: "30.0",
      deviceHashId: deviceId,
      internalTitle: null,
      clientPlatformName: "windows",
      metaData: [
        { key: "SubSessionId", value: subSessionId },
        { key: "wssignaling", value: "1" },
        { key: "GSStreamerType", value: "WebRTC" },
        { key: "networkType", value: "Unknown" },
        { key: "ClientImeSupport", value: "0" },
        { key: "surroundAudioInfo", value: "2" },
      ],
      surroundAudioInfo: 0,
      clientTimezoneOffset: timezoneMs,
      clientIdentification: "GFN-PC",
      parentSessionId: null,
      appId: parseInt(appId, 10),
      streamerVersion: 1,
      appLaunchMode: 1,
      sdkVersion: "1.0",
      enhancedStreamMode: 1,
      useOps: true,
      clientDisplayHdrCapabilities: null,
      accountLinked: true,
      partnerCustomData: "",
      enablePersistingInGameSettings: true,
      secureRTSPSupported: false,
      userAge: 26,
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

  const deviceId = input.deviceId ?? getStableDeviceId();
  const clientId = input.clientId ?? crypto.randomUUID();

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
    const prefetchHeaders = requestHeaders({ token: input.token, clientId, deviceId, includeOrigin: false });
    try {
      const prefetchResp = await fetch(prefetchUrl, { method: "GET", headers: prefetchHeaders });
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
    const validationHeaders = requestHeaders({ token: input.token, clientId, deviceId, includeOrigin: false });
    const validationResp = await fetch(validationUrl, { method: "GET", headers: validationHeaders });
    if (validationResp.ok) {
      const validationText = await validationResp.text();
      const validationPayload = JSON.parse(validationText) as CloudMatchResponse;
      preClaimStatus = validationPayload.session?.status ?? 0;
      const errorCode = validationPayload.session?.errorCode ?? 0;
      console.log(`[CloudMatch] claimSession: pre-claim validation status=${preClaimStatus}, errorCode=${errorCode}`);
      console.log(`[CloudMatch] claimSession: validation response (first 1000 chars): ${validationText.slice(0, 1000)}`);
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
    const payload = buildClaimRequestBody(input.sessionId, appId, settings);

    const headers: Record<string, string> = {
      "User-Agent": GFN_USER_AGENT,
      Authorization: `GFNJWT ${input.token}`,
      "Content-Type": "application/json",
      Origin: "https://play.geforcenow.com",
      Referer: "https://play.geforcenow.com/",
      "nv-client-id": clientId,
      "nv-client-streamer": "NVIDIA-CLASSIC",
      "nv-client-type": "NATIVE",
      "nv-client-version": GFN_CLIENT_VERSION,
      "nv-device-os": process.platform === "win32" ? "WINDOWS" : process.platform === "darwin" ? "MACOS" : "LINUX",
      "nv-device-type": "DESKTOP",
      "x-device-id": deviceId,
    };

    console.log(`[CloudMatch] claimSession PUT ${claimUrl}`);
    console.log(`[CloudMatch] claimSession body: ${JSON.stringify(payload)}`);
    const response = await fetch(claimUrl, {
      method: "PUT",
      headers,
      body: JSON.stringify(payload),
    });

    const text = await response.text();

    console.log(`[CloudMatch] claimSession response: HTTP ${response.status}`);
    console.log(`[CloudMatch] claimSession response body FULL: ${text}`);

    if (!response.ok) {
      throw SessionError.fromResponse(response.status, text);
    }

    const apiResponse = JSON.parse(text) as CloudMatchResponse;

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

    const pollHeaders = requestHeaders({ token: input.token, clientId, deviceId, includeOrigin: false });

    const pollResponse = await fetch(getUrl, {
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
      console.log(
        `[CloudMatch] claimed negotiated streaming features: cloudGsync=${negotiatedStreamProfile?.enableCloudGsync ?? "n/a"}, reflex=${negotiatedStreamProfile?.enableReflex ?? "n/a"}, l4s=${negotiatedStreamProfile?.enableL4S ?? "n/a"}`,
      );

      return {
        sessionId: sessionData.sessionId,
        status: sessionData.status,
        queuePosition,
        zone: "", // Zone not applicable for claimed sessions
        streamingBaseUrl: `https://${effectiveServerIp}`,
        serverIp: signaling.serverIp,
        signalingServer: signaling.signalingServer,
        signalingUrl: signaling.signalingUrl,
        gpuType: sessionData.gpuType,
        iceServers: await normalizeIceServers(pollApiResponse),
        mediaConnectionInfo: signaling.mediaConnectionInfo,
        negotiatedStreamProfile: negotiatedStreamProfile ?? extractNegotiatedStreamProfile(pollApiResponse),
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
