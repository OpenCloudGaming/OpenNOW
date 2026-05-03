import type { StreamClientMode } from "./gfn";

export const DEFAULT_MINIMUM_FPS_FOR_CLOUD_GSYNC = 60;
export const DEFAULT_MINIMUM_FPS_FOR_REFLEX_WITHOUT_VRR = 120;

export type NativeCloudGsyncDetectionSource = "native" | "assumed" | "unsupported";
export type NativeCloudGsyncOverride = "auto" | "0" | "1";

export type CloudGsyncDisabledReason =
  | "user-disabled"
  | "fps-too-low"
  | "unsupported-backend"
  | "unsupported-display"
  | "force-disabled"
  | "detection-failed";

export type CloudGsyncEnabledReason = "enabled" | "force-enabled" | "web-mode";
export type CloudGsyncResolutionReason = CloudGsyncDisabledReason | CloudGsyncEnabledReason;

export interface NativeCloudGsyncCapabilities {
  platformSupportsCloudGsync: boolean;
  isVrrCapableDisplay: boolean;
  isGsyncDisplay: boolean;
  minimumFpsForCloudGsync: number;
  minimumFpsForReflexWithoutVrr: number;
  detectionSource: NativeCloudGsyncDetectionSource;
  reason?: string;
}

export interface CloudGsyncResolutionInput {
  userRequested: boolean;
  fps: number;
  clientMode: StreamClientMode;
  nativeBackendAvailable: boolean;
  capabilities?: Partial<NativeCloudGsyncCapabilities> | null;
  override?: NativeCloudGsyncOverride | string | null;
}

export interface CloudGsyncResolution {
  requested: boolean;
  enabled: boolean;
  reflexEnabled: boolean;
  reason: CloudGsyncResolutionReason;
  capabilities: NativeCloudGsyncCapabilities;
}

export function unsupportedNativeCloudGsyncCapabilities(reason = "unsupported"): NativeCloudGsyncCapabilities {
  return {
    platformSupportsCloudGsync: false,
    isVrrCapableDisplay: false,
    isGsyncDisplay: false,
    minimumFpsForCloudGsync: DEFAULT_MINIMUM_FPS_FOR_CLOUD_GSYNC,
    minimumFpsForReflexWithoutVrr: DEFAULT_MINIMUM_FPS_FOR_REFLEX_WITHOUT_VRR,
    detectionSource: "unsupported",
    reason,
  };
}

export function normalizeCloudGsyncOverride(value: string | null | undefined): NativeCloudGsyncOverride {
  if (value === "0" || value === "1") {
    return value;
  }
  if (value === "disabled") {
    return "0";
  }
  if (value === "forced") {
    return "1";
  }
  return "auto";
}

export function normalizeNativeCloudGsyncCapabilities(
  capabilities?: Partial<NativeCloudGsyncCapabilities> | null,
): NativeCloudGsyncCapabilities {
  return {
    platformSupportsCloudGsync: capabilities?.platformSupportsCloudGsync ?? false,
    isVrrCapableDisplay: capabilities?.isVrrCapableDisplay ?? false,
    isGsyncDisplay: capabilities?.isGsyncDisplay ?? false,
    minimumFpsForCloudGsync:
      capabilities?.minimumFpsForCloudGsync ?? DEFAULT_MINIMUM_FPS_FOR_CLOUD_GSYNC,
    minimumFpsForReflexWithoutVrr:
      capabilities?.minimumFpsForReflexWithoutVrr ?? DEFAULT_MINIMUM_FPS_FOR_REFLEX_WITHOUT_VRR,
    detectionSource: capabilities?.detectionSource ?? "unsupported",
    reason: capabilities?.reason,
  };
}

export function resolveCloudGsync(input: CloudGsyncResolutionInput): CloudGsyncResolution {
  const override = normalizeCloudGsyncOverride(input.override);
  const capabilities = normalizeNativeCloudGsyncCapabilities(input.capabilities);
  const minimumFpsForCloudGsync = Math.max(
    0,
    capabilities.minimumFpsForCloudGsync || DEFAULT_MINIMUM_FPS_FOR_CLOUD_GSYNC,
  );
  const minimumFpsForReflexWithoutVrr = Math.max(
    0,
    capabilities.minimumFpsForReflexWithoutVrr || DEFAULT_MINIMUM_FPS_FOR_REFLEX_WITHOUT_VRR,
  );
  const reflexEnabledWithoutVrr = input.fps >= minimumFpsForReflexWithoutVrr;

  if (!input.userRequested) {
    return {
      requested: false,
      enabled: false,
      reflexEnabled: reflexEnabledWithoutVrr,
      reason: "user-disabled",
      capabilities,
    };
  }

  if (input.clientMode === "web") {
    return {
      requested: true,
      enabled: true,
      reflexEnabled: reflexEnabledWithoutVrr,
      reason: "web-mode",
      capabilities,
    };
  }

  if (!input.nativeBackendAvailable) {
    return {
      requested: true,
      enabled: false,
      reflexEnabled: reflexEnabledWithoutVrr,
      reason: "unsupported-backend",
      capabilities,
    };
  }

  if (override === "0") {
    return {
      requested: true,
      enabled: false,
      reflexEnabled: reflexEnabledWithoutVrr,
      reason: "force-disabled",
      capabilities,
    };
  }

  if (input.fps < minimumFpsForCloudGsync) {
    return {
      requested: true,
      enabled: false,
      reflexEnabled: reflexEnabledWithoutVrr,
      reason: "fps-too-low",
      capabilities,
    };
  }

  if (override === "1") {
    return {
      requested: true,
      enabled: true,
      reflexEnabled: true,
      reason: "force-enabled",
      capabilities,
    };
  }

  if (!capabilities.platformSupportsCloudGsync) {
    return {
      requested: true,
      enabled: false,
      reflexEnabled: reflexEnabledWithoutVrr,
      reason: capabilities.detectionSource === "unsupported" ? "unsupported-backend" : "detection-failed",
      capabilities,
    };
  }

  if (!capabilities.isVrrCapableDisplay) {
    return {
      requested: true,
      enabled: false,
      reflexEnabled: reflexEnabledWithoutVrr,
      reason: "unsupported-display",
      capabilities,
    };
  }

  return {
    requested: true,
    enabled: true,
    reflexEnabled: true,
    reason: "enabled",
    capabilities,
  };
}
