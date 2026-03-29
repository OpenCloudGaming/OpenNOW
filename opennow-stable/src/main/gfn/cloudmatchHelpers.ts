import crypto from "node:crypto";

import type { SessionCreateRequest } from "@shared/gfn";
import {
  colorQualityBitDepth,
  colorQualityChromaFormat,
} from "@shared/gfn";

import type { CloudMatchRequest, CloudMatchResponse } from "./types";

export const GFN_USER_AGENT =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36 NVIDIACEFClient/HEAD/debb5919f6 GFN-PC/2.0.80.173";
export const GFN_CLIENT_VERSION = "2.0.80.173";

export interface RequestHeadersOptions {
  token: string;
  clientId?: string;
  deviceId?: string;
  includeOrigin?: boolean;
  platform?: NodeJS.Platform;
}

export function streamingServerIp(response: CloudMatchResponse): string | null {
  const connections = response.session.connectionInfo ?? [];
  const sigConn = connections.find((conn) => conn.usage === 14);

  if (sigConn) {
    const rawIp = sigConn.ip;
    const directIp = Array.isArray(rawIp) ? rawIp[0] : rawIp;
    if (directIp && directIp.length > 0) {
      return directIp;
    }

    if (sigConn.resourcePath) {
      const host = extractHostFromUrl(sigConn.resourcePath);
      if (host) return host;
    }
  }

  const controlIp = response.session.sessionControlInfo?.ip;
  if (controlIp && controlIp.length > 0) {
    return Array.isArray(controlIp) ? controlIp[0] : controlIp;
  }

  return null;
}

export function extractHostFromUrl(url: string): string | null {
  const prefixes = ["rtsps://", "rtsp://", "wss://", "https://"];
  let afterProto: string | null = null;
  for (const prefix of prefixes) {
    if (url.startsWith(prefix)) {
      afterProto = url.slice(prefix.length);
      break;
    }
  }
  if (!afterProto) return null;

  const host = afterProto.split(":")[0]?.split("/")[0];
  if (!host || host.length === 0 || host.startsWith(".")) return null;
  return host;
}

export function isZoneHostname(ip: string): boolean {
  return ip.includes("cloudmatchbeta.nvidiagrid.net") || ip.includes("cloudmatch.nvidiagrid.net");
}

export function buildSignalingUrl(
  raw: string,
  serverIp: string,
): { signalingUrl: string; signalingHost: string | null } {
  if (raw.startsWith("rtsps://") || raw.startsWith("rtsp://")) {
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
    const withoutScheme = raw.slice("wss://".length);
    const host = withoutScheme.split("/")[0] ?? null;
    return { signalingUrl: raw, signalingHost: host };
  }

  if (raw.startsWith("/")) {
    return {
      signalingUrl: `wss://${serverIp}:443${raw}`,
      signalingHost: null,
    };
  }

  return {
    signalingUrl: `wss://${serverIp}:443/nvst/`,
    signalingHost: null,
  };
}

export function requestHeaders(options: RequestHeadersOptions): Record<string, string> {
  const clientId = options.clientId ?? crypto.randomUUID();
  const deviceId = options.deviceId ?? crypto.randomUUID();
  const platform = options.platform ?? process.platform;

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
    "nv-device-os": platform === "win32" ? "WINDOWS" : platform === "darwin" ? "MACOS" : "LINUX",
    "nv-device-type": "DESKTOP",
    "x-device-id": deviceId,
  };

  if (options.includeOrigin !== false) {
    headers["Origin"] = "https://play.geforcenow.com";
    headers["Referer"] = "https://play.geforcenow.com/";
  }

  return headers;
}

export function parseResolution(input: string): { width: number; height: number } {
  const [rawWidth, rawHeight] = input.split("x");
  const width = Number.parseInt(rawWidth ?? "", 10);
  const height = Number.parseInt(rawHeight ?? "", 10);

  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) {
    return { width: 1920, height: 1080 };
  }

  return { width, height };
}

interface BuildSessionRequestBodyOptions {
  deviceHashId?: string;
  subSessionId?: string;
  now?: Date;
}

export function buildSessionRequestBody(
  input: SessionCreateRequest,
  options: BuildSessionRequestBodyOptions = {},
): CloudMatchRequest {
  const { width, height } = parseResolution(input.settings.resolution);
  const cq = input.settings.colorQuality;
  const hdrEnabled = false;
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
      deviceHashId: options.deviceHashId ?? crypto.randomUUID(),
      clientVersion: "30.0",
      sdkVersion: "1.0",
      streamerVersion: 1,
      clientPlatformName: "windows",
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
        { key: "SubSessionId", value: options.subSessionId ?? crypto.randomUUID() },
        { key: "wssignaling", value: "1" },
        { key: "GSStreamerType", value: "WebRTC" },
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
      clientTimezoneOffset: -(options.now ?? new Date()).getTimezoneOffset() * 60 * 1000,
      enhancedStreamMode: 1,
      appLaunchMode: 1,
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

export function cloudmatchUrl(zone: string): string {
  return `https://${zone}.cloudmatchbeta.nvidiagrid.net`;
}

export function resolveStreamingBaseUrl(zone: string, provided?: string): string {
  if (provided && provided.trim()) {
    const trimmed = provided.trim();
    return trimmed.endsWith("/") ? trimmed.slice(0, -1) : trimmed;
  }
  return cloudmatchUrl(zone);
}

export function shouldUseServerIp(baseUrl: string): boolean {
  return baseUrl.includes("cloudmatchbeta.nvidiagrid.net");
}

export function resolvePollStopBase(zone: string, provided?: string, serverIp?: string): string {
  const base = resolveStreamingBaseUrl(zone, provided);
  if (serverIp && shouldUseServerIp(base) && !isZoneHostname(serverIp)) {
    return `https://${serverIp}`;
  }
  return base;
}
