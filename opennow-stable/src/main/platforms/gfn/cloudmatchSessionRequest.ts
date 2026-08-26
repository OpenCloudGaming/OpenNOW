import crypto from "node:crypto";

import type { SessionCreateRequest, StreamSettings } from "@shared/gfn";
import {
  clampNativeStreamFps,
  colorQualityBitDepth,
  colorQualityChromaFormat,
} from "@shared/gfn";

import type { CloudMatchRequest } from "./types";
import { GFN_CLIENT_IDENTIFICATION } from "./clientHeaders";
import { resolveGfnDeviceIdentity } from "./deviceIdentity";
import { getCloudMatchDeviceHashId } from "./deviceId";
import {
  appLaunchModeWireValue,
  buildRequestedStreamingFeatures,
  resolveNvstCreateStreamSku,
  shouldEnableInGameSettingsPersistence,
} from "./cloudmatchFeatures";

/** Official native Mac Bifrost `availableSupportedControllers` / `preferredController`. */
const OFFICIAL_GAMEPAD_CONTROLLER = 2;

const EMPTY_DISPLAY_DATA = {
  displayPrimaryX0: 0,
  displayPrimaryY0: 0,
  displayPrimaryX1: 0,
  displayPrimaryY1: 0,
  displayPrimaryX2: 0,
  displayPrimaryY2: 0,
  displayWhitePointX: 0,
  displayWhitePointY: 0,
  desiredContentMaxLuminance: 0,
  desiredContentMinLuminance: 0,
  desiredContentMaxFrameAverageLuminance: 0,
} as const;

export function parseResolution(input: string): { width: number; height: number } {
  const [rawWidth, rawHeight] = input.split("x");
  const width = Number.parseInt(rawWidth ?? "", 10);
  const height = Number.parseInt(rawHeight ?? "", 10);

  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) {
    return { width: 1920, height: 1080 };
  }

  return { width, height };
}

export function timezoneOffsetMs(): number {
  return -new Date().getTimezoneOffset() * 60 * 1000;
}

function defaultMonitorDpi(): number {
  return process.platform === "darwin" ? 144 : 96;
}

function defaultNetworkType(): string {
  return process.platform === "darwin" ? "WiFi5.0" : "Unknown";
}

function readPrimaryDisplayMetrics(): {
  dpi: number;
  horizontalPixels: number;
  verticalPixels: number;
} | null {
  try {
    const electron = require("electron") as typeof import("electron");
    const display = electron.screen?.getPrimaryDisplay?.();
    if (!display) {
      return null;
    }
    const scale = display.scaleFactor > 0 ? display.scaleFactor : 1;
    const dpi = Math.round(scale * 72);
    return {
      dpi: dpi > 0 ? dpi : defaultMonitorDpi(),
      horizontalPixels: Math.round(display.size.width * scale),
      verticalPixels: Math.round(display.size.height * scale),
    };
  } catch {
    return null;
  }
}

function officialHdrCapabilities(): NonNullable<
  CloudMatchRequest["sessionRequestData"]["clientDisplayHdrCapabilities"]
> {
  return {
    version: 2,
    hdrEdrSupportedFlagsInUint32: 1,
    static_metadata_descriptor_id: 0,
    display_data: { ...EMPTY_DISPLAY_DATA },
  };
}

export function sessionMetadata(
  width: number,
  height: number,
  transportMode: StreamSettings["transportMode"],
): Array<{ key: string; value: string }> {
  const display = readPrimaryDisplayMetrics();
  const physical = {
    horizontalPixels: display?.horizontalPixels ?? width,
    verticalPixels: display?.verticalPixels ?? height,
  };
  return [
    { key: "ClientImeSupport", value: "0" },
    { key: "SubSessionId", value: crypto.randomUUID() },
    {
      key: "clientPhysicalResolution",
      value: JSON.stringify(physical),
    },
    { key: "networkType", value: defaultNetworkType() },
    { key: "wssignaling", value: "1" },
    ...(transportMode === "nvst" ? [] : [{ key: "GSStreamerType", value: "WebRTC" }]),
    { key: "surroundAudioInfo", value: "2" },
  ];
}

export function buildSessionRequestBody(
  input: SessionCreateRequest,
  deviceHashId: string,
  networkTestSessionId: string | null = null,
): CloudMatchRequest {
  const { width, height } = parseResolution(input.settings.resolution);
  const cq = input.settings.colorQuality;
  // IMPORTANT: hdrEnabled is a SEPARATE toggle from color quality.
  // The Rust reference (cloudmatch.rs) uses settings.hdr_enabled independently.
  // 10-bit color depth does NOT mean HDR — you can have 10-bit SDR.
  // Conflating them caused the server to set up an HDR pipeline, which
  // dynamically downscaled resolution to ~540p.
  const hdrEnabled = false; // No HDR toggle implemented yet; hardcode off like claim body
  const useClassicStreamer = input.settings.transportMode === "nvst";
  const streamSku = useClassicStreamer
    ? resolveNvstCreateStreamSku(input.settings)
    : {
        bitDepth: colorQualityBitDepth(cq),
        chromaFormat: colorQualityChromaFormat(cq),
      };
  const bitDepth = streamSku.bitDepth;
  const chromaFormat = streamSku.chromaFormat;
  const accountLinked = false;
  // Official Mac advertises HDR capability (sdrHdrMode 1 + caps v2) with zero luminance.
  // Do not send desiredContentMaxLuminance>0 — that previously downscaled to ~540p.
  const advertiseOfficialHdrCaps = useClassicStreamer && process.platform === "darwin";
  const sdrHdrMode = hdrEnabled || advertiseOfficialHdrCaps ? 1 : 0;
  const display = readPrimaryDisplayMetrics();
  const requestedFps = input.settings.clientMode === "native" || useClassicStreamer
    ? clampNativeStreamFps(input.settings.fps)
    : input.settings.fps;

  return {
    sessionRequestData: {
      appId: parseInt(input.appId, 10),
      externalAppId: null,
      internalTitle: null,
      availableSupportedControllers: [OFFICIAL_GAMEPAD_CONTROLLER],
      preferredController: OFFICIAL_GAMEPAD_CONTROLLER,
      networkTestSessionId,
      parentSessionId: null,
      clientIdentification: GFN_CLIENT_IDENTIFICATION,
      // Keep device identity stable across create -> reconnect/resume flows.
      // The official client preserves this identity, and resume reliability depends on it.
      deviceHashId,
      clientVersion: "30.0",
      sdkVersion: "2.0",
      streamerVersion: "14",
      clientPlatformName: resolveGfnDeviceIdentity().clientPlatformName,
      clientRequestMonitorSettings: [
        {
          monitorId: 0,
          positionX: 0,
          positionY: 0,
          widthInPixels: width,
          heightInPixels: height,
          framesPerSecond: requestedFps,
          sdrHdrMode,
          displayData: { ...EMPTY_DISPLAY_DATA },
          hdr10PlusGamingData: null,
          dpi: display?.dpi ?? defaultMonitorDpi(),
        },
      ],
      useOps: true,
      audioMode: 2,
      metaData: sessionMetadata(width, height, input.settings.transportMode),
      sdrHdrMode,
      clientDisplayHdrCapabilities: advertiseOfficialHdrCaps || hdrEnabled
        ? officialHdrCapabilities()
        : null,
      surroundAudioInfo: 0,
      remoteControllersBitmap: 0,
      clientTimezoneOffset: timezoneOffsetMs(),
      enhancedStreamMode: 0,
      appLaunchMode: appLaunchModeWireValue(input.settings.appLaunchMode),
      secureRTSPSupported: useClassicStreamer,
      partnerCustomData: null,
      accountLinked,
      enablePersistingInGameSettings: shouldEnableInGameSettingsPersistence(input),
      requestedAudioFormat: 0,
      userAge: 25,
      requestedStreamingFeatures: buildRequestedStreamingFeatures(
        input.settings,
        bitDepth,
        chromaFormat,
        hdrEnabled,
        input.supportedCodecs,
        input.settings.transportMode,
      ),
      transport: null,
    },
  };
}

/**
 * Build claim/resume request payload
 */
export function buildClaimRequestBody(
  sessionId: string,
  appId: string,
  settings: StreamSettings,
  sessionAppLaunchMode?: number,
  enablePersistingInGameSettings = false,
): unknown {
  // For RESUME claims, we must NOT attempt to renegotiate streaming parameters.
  // The session is already configured on the server side. Sending different fps, resolution,
  // codec, etc. causes HTTP 400 from the server because those parameters are immutable for
  // an already-streaming session. Only send the action and minimal required fields.
  const deviceId = getCloudMatchDeviceHashId();
  const subSessionId = crypto.randomUUID();
  const timezoneMs = timezoneOffsetMs();
  const useClassicStreamer = settings.transportMode === "nvst";

  return {
    action: 2,
    data: "RESUME",
    sessionRequestData: {
      // Minimal fields required for resume - NO streaming parameter renegotiation
      audioMode: 2,
      remoteControllersBitmap: 0,
      sdrHdrMode: useClassicStreamer && process.platform === "darwin" ? 1 : 0,
      networkTestSessionId: null,
      availableSupportedControllers: [OFFICIAL_GAMEPAD_CONTROLLER],
      preferredController: OFFICIAL_GAMEPAD_CONTROLLER,
      clientVersion: "30.0",
      deviceHashId: deviceId,
      internalTitle: null,
      clientPlatformName: resolveGfnDeviceIdentity().clientPlatformName,
      metaData: [
        { key: "ClientImeSupport", value: "0" },
        { key: "SubSessionId", value: subSessionId },
        { key: "networkType", value: defaultNetworkType() },
        { key: "wssignaling", value: "1" },
        ...(useClassicStreamer ? [] : [{ key: "GSStreamerType", value: "WebRTC" }]),
        { key: "surroundAudioInfo", value: "2" },
      ],
      surroundAudioInfo: 0,
      clientTimezoneOffset: timezoneMs,
      clientIdentification: GFN_CLIENT_IDENTIFICATION,
      parentSessionId: null,
      appId: parseInt(appId, 10),
      streamerVersion: "14",
      // Resume must not renegotiate session parameters: prefer the wire value the
      // session was created with over whatever the UI toggles currently say.
      appLaunchMode: sessionAppLaunchMode ?? appLaunchModeWireValue(settings.appLaunchMode),
      sdkVersion: "2.0",
      enhancedStreamMode: 0,
      useOps: true,
      clientDisplayHdrCapabilities: useClassicStreamer && process.platform === "darwin"
        ? officialHdrCapabilities()
        : null,
      accountLinked: false,
      partnerCustomData: null,
      enablePersistingInGameSettings,
      requestedAudioFormat: 0,
      secureRTSPSupported: useClassicStreamer,
      userAge: 25,
      transport: null,
    },
    metaData: [],
  };
}
