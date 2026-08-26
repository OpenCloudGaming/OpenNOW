import os from "node:os";

import { getCloudMatchDeviceHashId } from "./deviceId";

import { GFN_PLAY_ORIGIN as SHARED_GFN_PLAY_ORIGIN, GFN_PLAY_REFERER as SHARED_GFN_PLAY_REFERER } from "@shared/gfn/endpoints";

import {
  resolveGfnDeviceIdentity,
  type GfnDeviceIdentity,
  type GfnDeviceOs,
} from "./deviceIdentity";

/** Official mall `shared/assets/config/config.json` build.version / hash (2.0.87.131). */
export const GFN_CLIENT_VERSION = "2.0.87.131";
/** Official CEF host token: `HEAD/` + first 10 hex chars of mall `build.hash`. */
const GFN_CEF_PRODUCT = "NVIDIACEFClient/HEAD/7b92719716";

const GFN_WINDOWS_USER_AGENT =
  `Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36 ${GFN_CEF_PRODUCT} GFN-PC/${GFN_CLIENT_VERSION}`;
const GFN_MACOS_USER_AGENT =
  `Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36 ${GFN_CEF_PRODUCT} GFN-PC/${GFN_CLIENT_VERSION}`;
const GFN_LINUX_USER_AGENT =
  `Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36 ${GFN_CEF_PRODUCT} GFN-PC/${GFN_CLIENT_VERSION}`;

export function gfnUserAgentForPlatform(platform: NodeJS.Platform = process.platform): string {
  if (platform === "darwin") {
    return GFN_MACOS_USER_AGENT;
  }
  if (platform === "linux") {
    return GFN_LINUX_USER_AGENT;
  }
  return GFN_WINDOWS_USER_AGENT;
}

export const GFN_USER_AGENT = gfnUserAgentForPlatform();
/** Official mall JSON `clientVersion` token used in native Grid User-Agent. */
export const GFN_BIFROST_CLIENT_VERSION = "30.0";
/** Official Mac BifrostClientSDK token from Grid POST 2026-08-19. */
const GFN_BIFROST_SDK = "4.9";
const GFN_BIFROST_SDK_BUILD = "38495286";

export function gfnBifrostUserAgentForPlatform(platform: NodeJS.Platform = process.platform): string {
  if (platform === "darwin") {
    return `GFN-PC/${GFN_BIFROST_CLIENT_VERSION} (MacOSX ${os.release()}) BifrostClientSDK/${GFN_BIFROST_SDK} (${GFN_BIFROST_SDK_BUILD})`;
  }
  if (platform === "linux") {
    return `GFN-PC/${GFN_BIFROST_CLIENT_VERSION} (Linux ${os.release()}) BifrostClientSDK/${GFN_BIFROST_SDK} (${GFN_BIFROST_SDK_BUILD})`;
  }
  return `GFN-PC/${GFN_BIFROST_CLIENT_VERSION} (Windows NT 10.0) BifrostClientSDK/${GFN_BIFROST_SDK} (${GFN_BIFROST_SDK_BUILD})`;
}

export const LCARS_CLIENT_ID = "ec7e38d4-03af-4b58-b131-cfb0495903ab";
/** Official CloudMatch / Bifrost `clientIdentification` for the native PC client. */
export const GFN_CLIENT_IDENTIFICATION = "GFN-PC";

export const GFN_PLAY_ORIGIN = SHARED_GFN_PLAY_ORIGIN;
export const GFN_PLAY_REFERER = SHARED_GFN_PLAY_REFERER;
export const NVIDIA_FILE_ORIGIN = "https://nvfile";
export const NVIDIA_FILE_REFERER = "https://nvfile/";

export type GfnClientStreamer = "NVIDIA-CLASSIC" | "WEBRTC";
export type GfnClientType = "NATIVE" | "BROWSER";
export type { GfnDeviceOs };

export function gfnJwtAuthorization(token: string): string {
  return `GFNJWT ${token}`;
}

export function bearerAuthorization(token: string): string {
  return `Bearer ${token}`;
}

export function platformToGfnDeviceOs(platform: NodeJS.Platform = process.platform): GfnDeviceOs {
  return resolveGfnDeviceIdentity({ identifyAsSteamDeck: false, platform }).deviceOs;
}

function applyDeviceIdentityHeaders(
  headers: Record<string, string>,
  identity: GfnDeviceIdentity,
  options?: { includeMakeModel?: boolean },
): void {
  headers["nv-device-os"] = identity.deviceOs;
  headers["nv-device-type"] = identity.deviceType;
  headers["x-nv-client-identity"] = GFN_CLIENT_IDENTIFICATION;
  if (options?.includeMakeModel !== false) {
    headers["nv-device-make"] = identity.deviceMake;
    headers["nv-device-model"] = identity.deviceModel;
  }
}

export interface NvidiaAuthHeadersOptions {
  bearerToken?: string;
  accept?: string;
  contentType?: string;
  includeReferer?: boolean;
}

export function buildNvidiaAuthHeaders(options: NvidiaAuthHeadersOptions = {}): Record<string, string> {
  const headers: Record<string, string> = {};

  if (options.bearerToken !== undefined) {
    headers.Authorization = bearerAuthorization(options.bearerToken);
  }
  if (options.contentType) {
    headers["Content-Type"] = options.contentType;
  }

  headers.Origin = NVIDIA_FILE_ORIGIN;
  if (options.includeReferer) {
    headers.Referer = NVIDIA_FILE_REFERER;
  }
  headers.Accept = options.accept ?? "application/json, text/plain, */*";
  headers["User-Agent"] = GFN_USER_AGENT;

  return headers;
}

export interface GfnLcarsHeadersOptions {
  token?: string;
  clientId?: string;
  clientType: GfnClientType;
  clientStreamer: GfnClientStreamer;
  accept?: string;
  deviceOs?: GfnDeviceOs;
  identifyAsSteamDeck?: boolean;
  includeUserAgent?: boolean;
  includeEmptyTokenAuthorization?: boolean;
}

export function buildGfnLcarsHeaders(options: GfnLcarsHeadersOptions): Record<string, string> {
  const identity = resolveGfnDeviceIdentity({ identifyAsSteamDeck: options.identifyAsSteamDeck });
  const headers: Record<string, string> = {
    Accept: options.accept ?? "application/json",
  };

  if (options.token || (options.includeEmptyTokenAuthorization && options.token !== undefined)) {
    headers.Authorization = gfnJwtAuthorization(options.token);
  }

  headers["nv-client-id"] = options.clientId ?? LCARS_CLIENT_ID;
  headers["nv-client-type"] = options.clientType;
  headers["nv-client-version"] = GFN_CLIENT_VERSION;
  headers["nv-client-streamer"] = options.clientStreamer;
  applyDeviceIdentityHeaders(headers, {
    ...identity,
    deviceOs: options.deviceOs ?? identity.deviceOs,
  });

  if (options.includeUserAgent) {
    headers["User-Agent"] = GFN_USER_AGENT;
  }

  return headers;
}

export function buildGfnGraphQlHeaders(
  token?: string,
  options?: { identifyAsSteamDeck?: boolean },
): Record<string, string> {
  const identity = resolveGfnDeviceIdentity(options);
  const headers: Record<string, string> = {
    Accept: "application/json, text/plain, */*",
    "Content-Type": "application/json",
    Origin: GFN_PLAY_ORIGIN,
    Referer: GFN_PLAY_REFERER,
    ...(token ? { Authorization: gfnJwtAuthorization(token) } : {}),
    "nv-client-id": LCARS_CLIENT_ID,
    "nv-client-type": "NATIVE",
    "nv-client-version": GFN_CLIENT_VERSION,
    "nv-client-streamer": "NVIDIA-CLASSIC",
    "nv-browser-type": "CHROME",
    "User-Agent": GFN_USER_AGENT,
  };
  applyDeviceIdentityHeaders(headers, identity);
  return headers;
}

export interface GfnCloudMatchHeadersOptions {
  token: string;
  clientId?: string;
  deviceId?: string;
  includeOrigin?: boolean;
  identifyAsSteamDeck?: boolean;
}

function resolveCloudMatchIdentity(options: GfnCloudMatchHeadersOptions): { clientId: string; deviceId: string } {
  return {
    clientId: options.clientId ?? LCARS_CLIENT_ID,
    deviceId: options.deviceId ?? getCloudMatchDeviceHashId(),
  };
}

export function buildGfnCloudMatchHeaders(options: GfnCloudMatchHeadersOptions): Record<string, string> {
  const { clientId, deviceId } = resolveCloudMatchIdentity(options);
  const identity = resolveGfnDeviceIdentity({ identifyAsSteamDeck: options.identifyAsSteamDeck });
  const userAgent = gfnBifrostUserAgentForPlatform();
  void options.includeOrigin;
  return {
    "User-Agent": userAgent,
    Authorization: gfnJwtAuthorization(options.token),
    "Content-Type": "text/plain",
    "NV-Client-ID": clientId,
    "NV-Client-Streamer": "NVIDIA-CLASSIC",
    "NV-Client-Type": "NATIVE",
    "NV-Client-Version": GFN_CLIENT_VERSION,
    "NV-Device-OS": identity.deviceOs,
    "NV-Device-Type": identity.deviceType,
    "NV-Device-Make": identity.deviceMake,
    "NV-Device-Model": identity.deviceModel,
    "X-Device-Id": deviceId,
    "x-nv-client-identity": userAgent,
  };
}

export interface GfnNvstClientHeadersOptions {
  deviceId?: string;
  identifyAsSteamDeck?: boolean;
}

/**
 * Official Bifrost GridServer / CloudMatch HTTP identity. Not used on RTSP-over-WSS:
 * that upgrade is `GET /rtsp` + `x-nv-sessionid` only.
 */
export function buildGfnNvstClientHeaders(
  options: GfnNvstClientHeadersOptions = {},
): Record<string, string> {
  const identity = resolveGfnDeviceIdentity({ identifyAsSteamDeck: options.identifyAsSteamDeck });
  const userAgent = gfnBifrostUserAgentForPlatform();
  const headers: Record<string, string> = {
    "User-Agent": userAgent,
    "x-nv-client-identity": userAgent,
    "NV-Device-OS": identity.deviceOs,
    "NV-Client-Streamer": "NVIDIA-CLASSIC",
    "NV-Device-Type": identity.deviceType,
    "NV-Client-Type": "NATIVE",
    "NV-Device-Make": identity.deviceMake,
    "NV-Device-Model": identity.deviceModel,
    "NV-Client-Version": GFN_CLIENT_VERSION,
  };
  if (options.deviceId) {
    headers["X-Device-Id"] = options.deviceId;
  }
  return headers;
}

export function buildGfnCloudMatchClaimHeaders(options: GfnCloudMatchHeadersOptions): Record<string, string> {
  return buildGfnCloudMatchHeaders({ ...options, includeOrigin: false });
}
