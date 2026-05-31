import { app } from "electron";

declare const __OPENNOW_BUILD_NUMBER__: string | undefined;
declare const __OPENNOW_BUILD_COMMIT__: string | undefined;

export interface AppBuildInfo {
  version: string;
  displayVersion: string;
  buildNumber?: string;
  commit?: string;
}

function normalizeMetadataValue(value: string | undefined): string | undefined {
  const trimmed = value?.trim();
  return trimmed ? trimmed : undefined;
}

function getEmbeddedBuildNumber(): string | undefined {
  return normalizeMetadataValue(
    typeof __OPENNOW_BUILD_NUMBER__ === "string" ? __OPENNOW_BUILD_NUMBER__ : undefined,
  );
}

function getEmbeddedCommit(): string | undefined {
  return normalizeMetadataValue(
    typeof __OPENNOW_BUILD_COMMIT__ === "string" ? __OPENNOW_BUILD_COMMIT__ : undefined,
  );
}

export function getAppBuildInfo(): AppBuildInfo {
  const version = app.getVersion();
  const buildNumber = getEmbeddedBuildNumber();
  const commit = getEmbeddedCommit();

  return {
    version,
    displayVersion: buildNumber ? `${version} (build ${buildNumber})` : version,
    buildNumber,
    commit,
  };
}
