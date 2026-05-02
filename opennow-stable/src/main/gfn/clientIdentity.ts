export const GFN_CLIENT_ID = "ec7e38d4-03af-4b58-b131-cfb0495903ab";
export const GFN_CLIENT_VERSION = "2.0.80.173";
export const GFN_CLOUDMATCH_CLIENT_VERSION = "30.0";
export const GFN_CLIENT_IDENTIFICATION = "GFN-PC";
export const GFN_CLIENT_PLATFORM_NAME = "windows";
export const GFN_CLIENT_TYPE = "NATIVE";
export const GFN_CLIENT_STREAMER = "NVIDIA-CLASSIC";
export const GFN_DEVICE_OS = "WINDOWS";
export const GFN_DEVICE_TYPE = "DESKTOP";
export const GFN_BROWSER_TYPE = "CHROME";
export const GFN_DEVICE_MAKE = "UNKNOWN";
export const GFN_DEVICE_MODEL = "UNKNOWN";
export const GFN_ORIGIN = "https://play.geforcenow.com";
export const GFN_REFERER = `${GFN_ORIGIN}/`;
export const GFN_USER_AGENT =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36 NVIDIACEFClient/HEAD/debb5919f6 GFN-PC/2.0.80.173";
// CloudMatch still expects WebRTC transport metadata because the session path negotiates a WebRTC stream.
export const GFN_STREAM_TRANSPORT_METADATA = "WebRTC";

interface DesktopGfnHeadersOptions {
  token?: string;
  clientId?: string;
  deviceId?: string;
  accept?: string;
  contentType?: string;
  includeOrigin?: boolean;
  includeBrowserType?: boolean;
  includeDeviceDetails?: boolean;
}

export function buildDesktopGfnHeaders(
  options: DesktopGfnHeadersOptions = {},
): Record<string, string> {
  const headers: Record<string, string> = {
    "User-Agent": GFN_USER_AGENT,
    "nv-client-id": options.clientId ?? GFN_CLIENT_ID,
    "nv-client-type": GFN_CLIENT_TYPE,
    "nv-client-version": GFN_CLIENT_VERSION,
    "nv-client-streamer": GFN_CLIENT_STREAMER,
    "nv-device-os": GFN_DEVICE_OS,
    "nv-device-type": GFN_DEVICE_TYPE,
  };

  if (options.accept) {
    headers.Accept = options.accept;
  }

  if (options.contentType) {
    headers["Content-Type"] = options.contentType;
  }

  if (options.includeOrigin) {
    headers.Origin = GFN_ORIGIN;
    headers.Referer = GFN_REFERER;
  }

  if (options.includeBrowserType) {
    headers["nv-browser-type"] = GFN_BROWSER_TYPE;
  }

  if (options.includeDeviceDetails) {
    headers["nv-device-make"] = GFN_DEVICE_MAKE;
    headers["nv-device-model"] = GFN_DEVICE_MODEL;
  }

  if (options.deviceId) {
    headers["x-device-id"] = options.deviceId;
  }

  if (options.token) {
    headers.Authorization = `GFNJWT ${options.token}`;
  }

  return headers;
}

export function buildDesktopCloudMatchIdentity(): {
  clientIdentification: string;
  clientVersion: string;
  clientPlatformName: string;
} {
  return {
    clientIdentification: GFN_CLIENT_IDENTIFICATION,
    clientVersion: GFN_CLOUDMATCH_CLIENT_VERSION,
    clientPlatformName: GFN_CLIENT_PLATFORM_NAME,
  };
}
