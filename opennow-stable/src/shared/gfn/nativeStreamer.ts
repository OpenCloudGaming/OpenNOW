import type { StreamClientMode } from "./stream";

export type NativeStreamerBackend = "native";
export type NativeStreamerFeatureMode = "auto" | "disabled" | "forced";
export type NativeVideoBackendPreference =
  | "auto"
  | "d3d11"
  | "d3d12"
  | "nvdec"
  | "vaapi"
  | "v4l2"
  | "vulkan"
  | "software";
export type StreamTransportMode = "webrtc" | "nvst";

export const NATIVE_STREAMER_UNSUPPORTED_PLATFORM_MESSAGE =
  "Native streamer requires a supported desktop OS (Windows, macOS, or Linux).";
/** Highest frame rate the native receive, decode, and presentation pipeline accepts. */
export const MAX_NATIVE_STREAM_FPS = 240;
/** @deprecated Use NATIVE_STREAMER_UNSUPPORTED_PLATFORM_MESSAGE. */
export const NATIVE_STREAMER_WINDOWS_ONLY_MESSAGE = NATIVE_STREAMER_UNSUPPORTED_PLATFORM_MESSAGE;

export function clampNativeStreamFps(fps: number): number {
  const normalized = Number.isFinite(fps) ? Math.trunc(fps) : 60;
  return Math.max(1, Math.min(MAX_NATIVE_STREAM_FPS, normalized));
}

export function isNativeStreamerSupportedPlatform(platform: string): boolean {
  const normalized = platform.toLowerCase();
  return (
    normalized === "win32" || normalized.startsWith("win") || normalized.includes("windows") ||
    normalized === "darwin" || normalized.includes("mac") ||
    normalized === "linux" || normalized.includes("linux")
  );
}

export function isNativeExternalRendererSupported(platform: string): boolean {
  return isNativeDirectXBackendSupported(platform) || isMacOsPlatform(platform);
}

/** macOS cannot embed an AppKit view across the Electron/helper process boundary. */
export function isNativeExternalRendererRequired(platform: string): boolean {
  return isMacOsPlatform(platform);
}

export function isNativeDirectXBackendSupported(platform: string): boolean {
  const normalized = platform.toLowerCase();
  return normalized === "win32" || normalized.startsWith("win") || normalized.includes("windows");
}

function isMacOsPlatform(platform: string): boolean {
  const normalized = platform.toLowerCase();
  return normalized === "darwin" || normalized.includes("mac");
}
export function isNvstTransportSupported(platform: string): boolean {
  return isNativeStreamerSupportedPlatform(platform);
}

export function normalizeStreamClientModeForPlatform(mode: StreamClientMode, platform: string): StreamClientMode {
  return mode === "native" && !isNativeStreamerSupportedPlatform(platform) ? "web" : mode;
}

export function normalizeNativeExternalRendererForPlatform(enabled: boolean, platform: string): boolean {
  return isNativeExternalRendererRequired(platform)
    || (enabled && isNativeExternalRendererSupported(platform));
}

export function normalizeTransportModeForPlatform(
  mode: StreamTransportMode,
  platform: string,
  streamClientMode: StreamClientMode = "native",
): StreamTransportMode {
  return mode === "nvst"
    && streamClientMode === "native"
    && isNvstTransportSupported(platform)
    ? "nvst"
    : "webrtc";
}

export function nativeStreamerFeatureModeToEnvValue(mode: NativeStreamerFeatureMode): "auto" | "0" | "1" {
  switch (mode) {
    case "disabled":
      return "0";
    case "forced":
      return "1";
    default:
      return "auto";
  }
}

export type NativeStreamerRuntimeSource = "self-contained" | "missing" | "unknown";

export interface NativeStreamerRuntimeStatus {
  source: NativeStreamerRuntimeSource;
  selfContained: boolean;
  path?: string;
  message: string;
}

export interface NativeStreamerStatus {
  detected: boolean;
  available: boolean;
  supportsOfferAnswer: boolean;
  backend?: NativeStreamerBackend;
  fallbackReason?: string;
  videoBackends?: NativeVideoBackendCapability[];
  activeVideoBackend?: NativeVideoBackendCapability;
  codecSummary?: string;
  zeroCopySummary?: string;
  runtime: NativeStreamerRuntimeStatus;
  message: string;
}

export function createUnsupportedNativeStreamerStatus(): NativeStreamerStatus {
  return {
    detected: false,
    available: false,
    supportsOfferAnswer: false,
    runtime: {
      source: "unknown",
      selfContained: false,
      message: NATIVE_STREAMER_UNSUPPORTED_PLATFORM_MESSAGE,
    },
    message: NATIVE_STREAMER_UNSUPPORTED_PLATFORM_MESSAGE,
  };
}

export type NativeVideoBackendId =
  | "d3d12"
  | "d3d11"
  | "videotoolbox"
  | "vaapi"
  | "v4l2"
  | "vulkan"
  | "software"
  | string;

export interface NativeVideoCodecCapability {
  codec: "h264" | "h265" | "av1" | string;
  available: boolean;
  decoder?: string;
  parser?: string;
  depayloader?: string;
  reason?: string;
}

export interface NativeVideoBackendCapability {
  backend: NativeVideoBackendId;
  platform: "windows" | "macos" | "linux" | "cross-platform" | "other" | string;
  codecs: NativeVideoCodecCapability[];
  zeroCopyModes: string[];
  sink?: string;
  available: boolean;
  reason?: string;
}
