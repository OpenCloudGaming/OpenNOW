export const GFN_CLIENT_VERSION = "2.0.80.173";
export const GFN_SESSION_REQUEST_CLIENT_VERSION = "30.0";
export const GFN_USER_AGENT =
  `Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 ` +
  `(KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36 ` +
  `NVIDIACEFClient/HEAD/debb5919f6 GFN-PC/${GFN_CLIENT_VERSION}`;

export const GFN_LCARS_CLIENT_ID = "ec7e38d4-03af-4b58-b131-cfb0495903ab";
export const GFN_CLIENT_IDENTIFICATION = "GFN-PC";
export const GFN_CLIENT_PLATFORM_NAME = "windows";
export const GFN_SDK_VERSION = "1.0";
export const GFN_STREAMER_VERSION = 1;
export const GFN_APP_LAUNCH_MODE = 1;
export const GFN_ENHANCED_STREAM_MODE = 1;

export const GFN_CLIENT_TYPE_NATIVE = "NATIVE";
export const GFN_CLIENT_TYPE_BROWSER = "BROWSER";
export const GFN_CLIENT_STREAMER_CLASSIC = "NVIDIA-CLASSIC";
export const GFN_CLIENT_STREAMER_WEBRTC = "WEBRTC";
export const GFN_DEVICE_OS_WINDOWS = "WINDOWS";
export const GFN_DEVICE_OS_MACOS = "MACOS";
export const GFN_DEVICE_OS_LINUX = "LINUX";
export const GFN_DEVICE_TYPE_DESKTOP = "DESKTOP";
export const GFN_DEVICE_MAKE_UNKNOWN = "UNKNOWN";
export const GFN_DEVICE_MODEL_UNKNOWN = "UNKNOWN";
export const GFN_BROWSER_TYPE_CHROME = "CHROME";
export const GFN_PLAY_ORIGIN = "https://play.geforcenow.com";
export const GFN_PLAY_REFERER = `${GFN_PLAY_ORIGIN}/`;
export const GFN_NVFILE_ORIGIN = "https://nvfile";
export const GFN_NVFILE_REFERER = `${GFN_NVFILE_ORIGIN}/`;

type GfnAuthScheme = "Bearer" | "GFNJWT";

export interface GfnHeaderOptions {
  accept?: string;
  contentType?: string;
  authorization?: {
    token: string;
    scheme?: GfnAuthScheme;
  };
  origin?: string;
  referer?: string;
  clientId?: string;
  clientType?: string;
  clientStreamer?: string;
  clientVersion?: string;
  deviceOs?: string;
  deviceType?: string;
  deviceMake?: string;
  deviceModel?: string;
  browserType?: string;
  deviceId?: string;
}

export function resolveGfnDeviceOs(platform?: string): string {
  switch (platform) {
    case "win32":
      return GFN_DEVICE_OS_WINDOWS;
    case "darwin":
      return GFN_DEVICE_OS_MACOS;
    default:
      return GFN_DEVICE_OS_LINUX;
  }
}

export function buildGfnHeaders(options: GfnHeaderOptions = {}): Record<string, string> {
  const headers: Record<string, string> = {
    "User-Agent": GFN_USER_AGENT,
  };

  if (options.accept) {
    headers.Accept = options.accept;
  }

  if (options.contentType) {
    headers["Content-Type"] = options.contentType;
  }

  if (options.authorization?.token) {
    headers.Authorization = `${options.authorization.scheme ?? "GFNJWT"} ${options.authorization.token}`;
  }

  if (options.origin) {
    headers.Origin = options.origin;
  }

  if (options.referer) {
    headers.Referer = options.referer;
  }

  if (
    options.clientId ||
    options.clientType ||
    options.clientStreamer ||
    options.clientVersion
  ) {
    if (options.clientId) {
      headers["nv-client-id"] = options.clientId;
    }
    if (options.clientType) {
      headers["nv-client-type"] = options.clientType;
    }
    if (options.clientStreamer) {
      headers["nv-client-streamer"] = options.clientStreamer;
    }
    headers["nv-client-version"] = options.clientVersion ?? GFN_CLIENT_VERSION;
  }

  if (options.deviceOs) {
    headers["nv-device-os"] = options.deviceOs;
  }
  if (options.deviceType) {
    headers["nv-device-type"] = options.deviceType;
  }
  if (options.deviceMake) {
    headers["nv-device-make"] = options.deviceMake;
  }
  if (options.deviceModel) {
    headers["nv-device-model"] = options.deviceModel;
  }
  if (options.browserType) {
    headers["nv-browser-type"] = options.browserType;
  }
  if (options.deviceId) {
    headers["x-device-id"] = options.deviceId;
  }

  return headers;
}

export function buildGfnSignalingSignInUrl(
  baseUrl: string,
  peerId: string,
  pairingId?: string,
): string {
  const url = new URL(baseUrl);
  const normalizedPath = url.pathname.endsWith("/") ? url.pathname : `${url.pathname}/`;
  url.pathname = `${normalizedPath}sign_in`;
  url.search = new URLSearchParams({
    peer_id: peerId,
    version: "2",
    peer_role: "1",
    ...(pairingId ? { pairing_id: pairingId } : {}),
  }).toString();
  return url.toString();
}

export function isGfnVerboseLoggingEnabled(): boolean {
  const globalValue = globalThis as {
    process?: { env?: Record<string, string | undefined> };
    localStorage?: Storage;
  };

  if (globalValue.process?.env?.OPENNOW_GFN_VERBOSE_LOGS === "1") {
    return true;
  }

  try {
    return globalValue.localStorage?.getItem("OPENNOW_GFN_VERBOSE_LOGS") === "1";
  } catch {
    return false;
  }
}
