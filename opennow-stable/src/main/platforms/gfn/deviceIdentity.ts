import { execFileSync } from "node:child_process";

/**
 * GFN device identity profiles.
 *
 * Official Steam Deck CEF still uses clientIdentification/MES serviceName `gfn_pc`,
 * but advertises itself via nv-device-* headers and clientPlatformName. OpenNOW only
 * used Steam Deck headers for QR device-login; this module owns the stream/MES profile.
 */

export type GfnDeviceOs = "WINDOWS" | "MACOS" | "LINUX" | "STEAMOS";
export type GfnDeviceType = "DESKTOP" | "CONSOLE";

export interface GfnDeviceIdentity {
  deviceOs: GfnDeviceOs;
  deviceType: GfnDeviceType;
  deviceMake: string;
  deviceModel: string;
  /** CloudMatch session / nettest `clientPlatformName` */
  clientPlatformName: string;
}

let cachedDarwinHwModel: string | null = null;

function readDarwinHwModel(): string {
  if (cachedDarwinHwModel) {
    return cachedDarwinHwModel;
  }
  try {
    const model = execFileSync("sysctl", ["-n", "hw.model"], {
      encoding: "utf8",
      timeout: 500,
    }).trim();
    cachedDarwinHwModel = model.length > 0 ? model : "UNKNOWN";
  } catch {
    cachedDarwinHwModel = "UNKNOWN";
  }
  return cachedDarwinHwModel;
}

const DESKTOP_IDENTITY_BY_PLATFORM: Record<"win32" | "darwin" | "linux", GfnDeviceIdentity> = {
  win32: {
    deviceOs: "WINDOWS",
    deviceType: "DESKTOP",
    deviceMake: "UNKNOWN",
    deviceModel: "UNKNOWN",
    clientPlatformName: "Windows",
  },
  darwin: {
    deviceOs: "MACOS",
    deviceType: "DESKTOP",
    deviceMake: "Apple",
    deviceModel: "UNKNOWN",
    // Native Bifrost / QUERY_GFN_START uses MacOSX. Mall JS Session Control
    // falls back to OSName "MacOS", but official Mac never POSTs that body.
    clientPlatformName: "MacOSX",
  },
  linux: {
    deviceOs: "LINUX",
    deviceType: "DESKTOP",
    deviceMake: "UNKNOWN",
    deviceModel: "UNKNOWN",
    clientPlatformName: "Linux",
  },
};

/** Matches official Steam Deck / SteamOS mall device headers (VALVE + STEAMDECK + CONSOLE). */
export const STEAM_DECK_DEVICE_IDENTITY: Readonly<GfnDeviceIdentity> = Object.freeze({
  deviceOs: "STEAMOS",
  deviceType: "CONSOLE",
  deviceMake: "VALVE",
  deviceModel: "STEAMDECK",
  clientPlatformName: "SteamOS",
});

let identifyAsSteamDeckReader: (() => boolean) | null = null;

/** Wire from main after settings load so header builders stay free of settings imports. */
export function configureIdentifyAsSteamDeck(reader: () => boolean): void {
  identifyAsSteamDeckReader = reader;
}

export function isIdentifyAsSteamDeckEnabled(): boolean {
  return identifyAsSteamDeckReader?.() ?? false;
}

export function resolveHostDesktopIdentity(
  platform: NodeJS.Platform = process.platform,
): GfnDeviceIdentity {
  if (platform === "darwin") {
    return {
      ...DESKTOP_IDENTITY_BY_PLATFORM.darwin,
      deviceMake: "Apple",
      deviceModel: readDarwinHwModel(),
    };
  }
  if (platform === "win32" || platform === "linux") {
    return DESKTOP_IDENTITY_BY_PLATFORM[platform];
  }
  return DESKTOP_IDENTITY_BY_PLATFORM.linux;
}

export function resolveGfnDeviceIdentity(options?: {
  identifyAsSteamDeck?: boolean;
  platform?: NodeJS.Platform;
}): GfnDeviceIdentity {
  const identifyAsSteamDeck = options?.identifyAsSteamDeck ?? isIdentifyAsSteamDeckEnabled();
  if (identifyAsSteamDeck) {
    return STEAM_DECK_DEVICE_IDENTITY;
  }
  return resolveHostDesktopIdentity(options?.platform);
}
