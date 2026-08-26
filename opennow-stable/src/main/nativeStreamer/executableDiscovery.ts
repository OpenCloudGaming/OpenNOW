import { join, resolve } from "node:path";

import {
  isExistingFile,
  nativeStreamerExecutableName,
  nativeStreamerPlatformKey,
} from "./runtime";

export interface NativeStreamerExecutableDiscoveryOptions {
  platform: NodeJS.Platform;
  arch: string;
  resourcesPath: string;
  appPath: string;
  mainDir: string;
  envExecutablePath: string | undefined;
  getConfiguredPath(): string;
}

export function resolveNativeStreamerExecutableCandidates(
  options: NativeStreamerExecutableDiscoveryOptions,
): string[] {
  const exeName = nativeStreamerExecutableName(options.platform);
  const platformKey = nativeStreamerPlatformKey(options.platform, options.arch);
  const configuredPath = options.getConfiguredPath().trim();
  if (configuredPath && !isExistingFile(configuredPath)) {
    throw new Error(`Configured native streamer executable was not found: ${configuredPath}`);
  }
  // On macOS the streamer must run from inside an .app bundle: a bundle-less process is
  // refused window compositing by the WindowServer, so the video overlay never appears.
  const macBundleRelativeExecutable = join(
    platformKey,
    "OpenNOWStreamer.app",
    "Contents",
    "MacOS",
    exeName,
  );
  const explicitCandidates = [
    configuredPath || undefined,
    options.envExecutablePath,
  ];
  const automaticCandidates = options.platform === "darwin"
    ? [
        join(options.resourcesPath, "native", "opennow-streamer", macBundleRelativeExecutable),
        resolve(
          options.mainDir,
          "../../../native/opennow-streamer/bin",
          macBundleRelativeExecutable,
        ),
        resolve(
          options.appPath,
          "../native/opennow-streamer/bin",
          macBundleRelativeExecutable,
        ),
      ]
    : [
        join(options.resourcesPath, "native", "opennow-streamer", platformKey, exeName),
        resolve(options.mainDir, "../../../native/opennow-streamer/bin", platformKey, exeName),
        resolve(options.mainDir, "../../../native/opennow-streamer/target/release", exeName),
        resolve(options.mainDir, "../../../native/opennow-streamer/target/debug", exeName),
        resolve(options.appPath, "../native/opennow-streamer/bin", platformKey, exeName),
      ];
  const checked = [...explicitCandidates, ...automaticCandidates]
    .filter((candidate): candidate is string => Boolean(candidate));
  const candidates = checked.filter((candidate, index) =>
    isExistingFile(candidate) && checked.indexOf(candidate) === index,
  );
  if (candidates.length > 0) return candidates;
  throw new Error(`Native streamer binary not found. Checked: ${checked.join(", ")}`);
}
