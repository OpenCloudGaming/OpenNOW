import type {
  AppLaunchMode,
  SessionCreateRequest,
  StreamSettings,
  VideoCodec,
} from "@shared/gfn";
import { DEFAULT_MINIMUM_FPS_FOR_REFLEX_WITHOUT_VRR } from "@shared/cloudGsync";

import type { CloudMatchRequest } from "./types";

// Wire values used by cloudmatch session requests. Matches the official
// client's mapping: Default -> 1, GamepadFriendly -> 2, TouchFriendly -> 3.
const APP_LAUNCH_MODE_WIRE_VALUES: Record<AppLaunchMode, number> = {
  default: 1,
  gamepadFriendly: 2,
  touchFriendly: 3,
};

export function appLaunchModeWireValue(mode: AppLaunchMode | undefined): number {
  return APP_LAUNCH_MODE_WIRE_VALUES[mode ?? "default"];
}

export function buildRequestedStreamingFeatures(
  settings: StreamSettings,
  bitDepth: number,
  chromaFormat: number,
  _hdrEnabled: boolean,
  supportedCodecs?: readonly VideoCodec[],
  transportMode: StreamSettings["transportMode"] = settings.transportMode,
): CloudMatchRequest["sessionRequestData"]["requestedStreamingFeatures"] {
  const cloudGsync = settings.enableCloudGsync;

  const commonFeatures = {
    reflex: shouldRequestReflex(settings),
    bitDepth,
    cloudGsync,
    enabledL4S: settings.enableL4S,
    supportedHidDevices: 0,
    profile: 0,
    fallbackToLogicalResolution: false,
    chromaFormat,
    prefilterMode: 0,
    prefilterSharpness: 0,
    prefilterNoiseReduction: 0,
    hudStreamingMode: 0,
  };

  if (transportMode === "nvst") {
    const sku = resolveNvstCreateStreamSku(settings);
    return {
      ...commonFeatures,
      reflex: sku.reflex,
      bitDepth: sku.bitDepth,
      chromaFormat: sku.chromaFormat,
      mouseMovementFlags: 0,
      trueHdr: false,
      hidDevices: null,
      qosPolicy: 0,
      touchSupport: false,
    };
  }

  return {
    ...commonFeatures,
    maxBitrateKbps: Math.round(settings.maxBitrateMbps * 1000),
    codec: resolveRequestedCodecWireValue(
      codecWireValue(settings.codec),
      (supportedCodecs ?? []).map(codecWireValue),
    ),
    vsync: false,
    dynamicStreamingMode: 3,
    audioChannelCount: 2,
  };
}

export function codecWireValue(codec: VideoCodec): number {
  switch (codec) {
    case "H264":
      return 1;
    case "H265":
      return 2;
    case "AV1":
      return 3;
    default:
      return 0;
  }
}

const OFFICIAL_CODEC_LADDERS: Readonly<Record<number, readonly number[]>> = {
  0: [0],
  1: [1],
  2: [2, 1],
  3: [3, 2, 1],
};

export function resolveRequestedCodecWireValue(
  preferenceWireValue: number,
  supportedCodecWireValues: readonly number[],
): number {
  const ladder = OFFICIAL_CODEC_LADDERS[preferenceWireValue] ?? [preferenceWireValue];
  if (supportedCodecWireValues.length === 0) {
    return ladder[0] ?? preferenceWireValue;
  }
  const supported = new Set(supportedCodecWireValues);
  return ladder.find((value) => supported.has(value)) ?? ladder[0] ?? preferenceWireValue;
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

/** Official Mac Bifrost NVST create advertises 10-bit + reflex even at 8-bit UI quality. */
export function resolveNvstCreateStreamSku(settings: StreamSettings): {
  bitDepth: number;
  chromaFormat: number;
  reflex: boolean;
} {
  void settings;
  return { bitDepth: 1, chromaFormat: 0, reflex: true };
}

export function shouldEnableInGameSettingsPersistence(
  input: Pick<SessionCreateRequest, "enablePersistingInGameSettings" | "supportsInGameSettingsPersistence">,
): boolean {
  return (
    input.enablePersistingInGameSettings === true &&
    input.supportsInGameSettingsPersistence === true
  );
}
