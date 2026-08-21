import { existsSync, realpathSync, statSync } from "node:fs";
import { resolve, sep } from "node:path";

import {
  nativeStreamerFeatureModeToEnvValue,
  type NativeStreamerFeatureMode,
  type NativeStreamerRuntimeStatus,
  type NativeVideoBackendPreference,
} from "@shared/gfn";

export interface NativeStreamerRuntimeEnvironmentOptions {
  executablePath: string;
  baseEnv: NodeJS.ProcessEnv;
  platform: NodeJS.Platform;
  arch: string;
  userDataPath: string;
  protocolVersion: number;
  videoBackendPreference: NativeVideoBackendPreference;
  externalRendererEnabled: boolean;
  linuxOzonePlatform?: string;
  cloudGsyncMode: NativeStreamerFeatureMode;
  d3dFullscreenMode: NativeStreamerFeatureMode;
}

export interface NativeStreamerRuntimeEnvironment {
  env: NodeJS.ProcessEnv;
  runtimeStatus: NativeStreamerRuntimeStatus;
}

export function nativeStreamerExecutableName(platform = process.platform): string {
  return platform === "win32" ? "opennow-streamer.exe" : "opennow-streamer";
}

export function nativeStreamerPlatformKey(
  platform: NodeJS.Platform = process.platform,
  arch: string = process.arch,
): string {
  return `${platform}-${arch}`;
}

export function isExistingFile(path: string): boolean {
  try {
    return existsSync(path) && statSync(path).isFile();
  } catch {
    return false;
  }
}

export function normalizePathForComparison(
  path: string,
  platform = process.platform,
): string {
  let resolvedPath = resolve(path);
  try {
    resolvedPath = realpathSync.native(resolvedPath);
  } catch {
    // Paths selected in settings may not exist yet.
  }
  return platform === "win32" ? resolvedPath.toLowerCase() : resolvedPath;
}

export function isPathInside(
  parent: string,
  child: string,
  platform = process.platform,
): boolean {
  const normalizedParent = normalizePathForComparison(parent, platform);
  const normalizedChild = normalizePathForComparison(child, platform);
  return normalizedChild === normalizedParent || normalizedChild.startsWith(`${normalizedParent}${sep}`);
}

export function createNativeStreamerRuntimeEnvironment(
  options: NativeStreamerRuntimeEnvironmentOptions,
): NativeStreamerRuntimeEnvironment {
  const env: NodeJS.ProcessEnv = {
    ...options.baseEnv,
    OPENNOW_NATIVE_STREAMER_PROTOCOL: String(options.protocolVersion),
    OPENNOW_NATIVE_CLOUD_GSYNC: nativeStreamerFeatureModeToEnvValue(options.cloudGsyncMode),
    OPENNOW_NATIVE_D3D_FULLSCREEN: nativeStreamerFeatureModeToEnvValue(options.d3dFullscreenMode),
    OPENNOW_NATIVE_EXTERNAL_RENDERER: options.externalRendererEnabled ? "1" : "0",
  };
  delete env.GST_PLUGIN_PATH;
  delete env.GST_PLUGIN_PATH_1_0;
  delete env.GST_PLUGIN_SYSTEM_PATH;
  delete env.GST_PLUGIN_SYSTEM_PATH_1_0;
  delete env.GST_PLUGIN_SCANNER;
  delete env.GST_PLUGIN_SCANNER_1_0;
  delete env.GST_REGISTRY;

  if (options.videoBackendPreference !== "auto") {
    env.OPENNOW_NATIVE_VIDEO_BACKEND = options.videoBackendPreference;
  }

  return {
    env,
    runtimeStatus: {
      source: "self-contained",
      selfContained: true,
      path: options.executablePath,
      message: "Native streamer v2 is self-contained; no external media runtime is required.",
    },
  };
}
