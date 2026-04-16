import { app } from "electron";
import electronUpdater from "electron-updater";
import type { AppUpdater, ProgressInfo, UpdateDownloadedEvent, UpdateInfo } from "electron-updater";

import type { AppUpdaterState } from "@shared/gfn";

const { autoUpdater } = electronUpdater;

const STARTUP_CHECK_DELAY_MS = 12_000;
const PERIODIC_CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;
const UPDATER_TOKEN_ENV_KEYS = ["OPENNOW_GH_TOKEN", "GH_TOKEN"] as const;

export interface AppUpdaterController {
  initialize(): void;
  dispose(): void;
  getState(): AppUpdaterState;
  setAutomaticChecksEnabled(enabled: boolean): AppUpdaterState;
  checkForUpdates(source?: "auto" | "manual"): Promise<AppUpdaterState>;
  downloadUpdate(): Promise<AppUpdaterState>;
  quitAndInstall(): Promise<AppUpdaterState>;
}

interface AppUpdaterControllerOptions {
  onStateChanged: (state: AppUpdaterState) => void;
  automaticChecksEnabled: boolean;
  onBeforeQuitAndInstall?: () => void;
  onQuitAndInstallError?: () => void;
}

function isPrereleaseVersion(version: string): boolean {
  return version.includes("-");
}

function pickRuntimeToken(): string | null {
  for (const key of UPDATER_TOKEN_ENV_KEYS) {
    const value = process.env[key]?.trim();
    if (value) {
      return value;
    }
  }

  return null;
}

function normalizeErrorMessage(error: unknown): string {
  if (!(error instanceof Error)) {
    return "Update check failed.";
  }

  const message = error.message.trim();
  if (/sha(2|512)|checksum|signature/i.test(message)) {
    return "Downloaded update failed verification.";
  }
  if (/latest.*yml|app-update\.yml|Cannot find channel/i.test(message)) {
    return "Update metadata is unavailable for this release.";
  }
  if (/404/.test(message)) {
    return "No published update metadata was found on GitHub Releases.";
  }
  if (/403/.test(message)) {
    return "GitHub Releases rejected the update request.";
  }
  if (/net::|network|ENOTFOUND|ECONN|ETIMEDOUT|EAI_AGAIN|offline/i.test(message)) {
    return "Unable to reach GitHub Releases right now.";
  }

  return message || "Update check failed.";
}

function createDisabledState(currentVersion: string, message: string): AppUpdaterState {
  return {
    status: "disabled",
    currentVersion,
    updateSource: "github-releases",
    message,
    canCheck: false,
    canDownload: false,
    canInstall: false,
    isPackaged: app.isPackaged,
  };
}

export function createAppUpdaterController(options: AppUpdaterControllerOptions): AppUpdaterController {
  const currentVersion = app.getVersion();
  if (!app.isPackaged) {
    const disabledState = createDisabledState(currentVersion, "Auto-updates are only available in packaged builds.");
    return {
      initialize() {
        options.onStateChanged(disabledState);
      },
      dispose() {},
      getState() {
        return disabledState;
      },
      setAutomaticChecksEnabled() {
        return disabledState;
      },
      async checkForUpdates() {
        return disabledState;
      },
      async downloadUpdate() {
        return disabledState;
      },
      async quitAndInstall() {
        return disabledState;
      },
    };
  }

  const updater: AppUpdater = autoUpdater;
  const token = pickRuntimeToken();
  if (token) {
    updater.requestHeaders = {
      ...updater.requestHeaders,
      Authorization: `token ${token}`,
    };
  }

  updater.autoDownload = false;
  updater.autoInstallOnAppQuit = false;
  updater.autoRunAppAfterInstall = true;
  updater.allowPrerelease = isPrereleaseVersion(currentVersion);
  updater.allowDowngrade = false;
  updater.fullChangelog = false;

  let disposed = false;
  let startupTimer: NodeJS.Timeout | null = null;
  let intervalTimer: NodeJS.Timeout | null = null;
  let checkInFlight = false;
  let downloadInFlight = false;
  let automaticChecksEnabled = options.automaticChecksEnabled;
  let availableUpdateInfo: UpdateInfo | null = null;
  let downloadedUpdateInfo: UpdateDownloadedEvent | null = null;

  const baseState: Pick<AppUpdaterState, "currentVersion" | "updateSource" | "isPackaged"> = {
    currentVersion,
    updateSource: "github-releases",
    isPackaged: true,
  };

  let state: AppUpdaterState = {
    ...baseState,
    status: "idle",
    canCheck: true,
    canDownload: false,
    canInstall: false,
  };

  const emitState = (): void => {
    if (!disposed) {
      options.onStateChanged(state);
    }
  };

  const recomputeActionFlags = (nextState: AppUpdaterState): AppUpdaterState => ({
    ...nextState,
    canCheck: !checkInFlight && !downloadInFlight,
    canDownload: !checkInFlight && !downloadInFlight && nextState.status === "available" && Boolean(availableUpdateInfo),
    canInstall: nextState.status === "downloaded",
  });

  const updateState = (patch: Partial<AppUpdaterState>): void => {
    state = recomputeActionFlags({
      ...state,
      ...patch,
      ...baseState,
    });
    emitState();
  };

  const clearAutomaticCheckTimers = (): void => {
    if (startupTimer) {
      clearTimeout(startupTimer);
      startupTimer = null;
    }
    if (intervalTimer) {
      clearInterval(intervalTimer);
      intervalTimer = null;
    }
  };

  const scheduleAutomaticChecks = (): void => {
    clearAutomaticCheckTimers();
    if (disposed || !automaticChecksEnabled) {
      return;
    }
    startupTimer = setTimeout(() => {
      void controller.checkForUpdates("auto");
    }, STARTUP_CHECK_DELAY_MS);
    startupTimer.unref?.();
    intervalTimer = setInterval(() => {
      void controller.checkForUpdates("auto");
    }, PERIODIC_CHECK_INTERVAL_MS);
    intervalTimer.unref?.();
  };

  updater.on("checking-for-update", () => {
    updateState({
      status: "checking",
      message: "Checking GitHub Releases for updates…",
      errorCode: undefined,
    });
  });

  updater.on("update-available", (info: UpdateInfo) => {
    availableUpdateInfo = info;
    downloadedUpdateInfo = null;
    updateState({
      status: "available",
      availableVersion: info.version,
      downloadedVersion: undefined,
      progress: undefined,
      lastCheckedAt: Date.now(),
      message: `OpenNOW ${info.version} is available. Download when ready.`,
    });
  });

  updater.on("update-not-available", () => {
    availableUpdateInfo = null;
    downloadedUpdateInfo = null;
    updateState({
      status: "not-available",
      availableVersion: undefined,
      downloadedVersion: undefined,
      progress: undefined,
      lastCheckedAt: Date.now(),
      message: "OpenNOW is up to date.",
    });
  });

  updater.on("download-progress", (progress: ProgressInfo) => {
    updateState({
      status: "downloading",
      availableVersion: availableUpdateInfo?.version,
      progress: {
        percent: progress.percent,
        transferred: progress.transferred,
        total: progress.total,
        bytesPerSecond: progress.bytesPerSecond,
      },
      message: `Downloading OpenNOW ${availableUpdateInfo?.version ?? "update"}…`,
    });
  });

  updater.on("update-downloaded", (info: UpdateDownloadedEvent) => {
    downloadedUpdateInfo = info;
    updateState({
      status: "downloaded",
      availableVersion: info.version,
      downloadedVersion: info.version,
      progress: undefined,
      message: `OpenNOW ${info.version} is ready to install. Restart when convenient.`,
    });
  });

  updater.on("error", (error: Error) => {
    checkInFlight = false;
    downloadInFlight = false;
    updateState({
      status: "error",
      availableVersion: availableUpdateInfo?.version,
      progress: undefined,
      message: normalizeErrorMessage(error),
      errorCode: (error as Error & { code?: string }).code,
    });
  });

  const controller: AppUpdaterController = {
    initialize() {
      emitState();
      scheduleAutomaticChecks();
    },
    dispose() {
      disposed = true;
      clearAutomaticCheckTimers();
      updater.removeAllListeners("checking-for-update");
      updater.removeAllListeners("update-available");
      updater.removeAllListeners("update-not-available");
      updater.removeAllListeners("download-progress");
      updater.removeAllListeners("update-downloaded");
      updater.removeAllListeners("error");
    },
    getState() {
      return state;
    },
    setAutomaticChecksEnabled(enabled: boolean) {
      automaticChecksEnabled = enabled;
      scheduleAutomaticChecks();
      return state;
    },
    async checkForUpdates(source: "auto" | "manual" = "manual") {
      if (disposed || checkInFlight || downloadInFlight) {
        return state;
      }

      if (source === "auto" && !automaticChecksEnabled) {
        return state;
      }

      checkInFlight = true;
      updateState({
        status: "checking",
        message: source === "auto" ? "Checking for updates in the background…" : "Checking GitHub Releases for updates…",
        errorCode: undefined,
      });

      try {
        await updater.checkForUpdates();
      } catch (error) {
        updateState({
          status: "error",
          availableVersion: availableUpdateInfo?.version,
          progress: undefined,
          lastCheckedAt: Date.now(),
          message: normalizeErrorMessage(error),
          errorCode: error instanceof Error ? (error as Error & { code?: string }).code : undefined,
        });
      } finally {
        checkInFlight = false;
        updateState({});
      }

      return state;
    },
    async downloadUpdate() {
      if (disposed || checkInFlight || downloadInFlight || !availableUpdateInfo) {
        return state;
      }

      downloadInFlight = true;
      updateState({
        status: "downloading",
        availableVersion: availableUpdateInfo.version,
        progress: {
          percent: 0,
          transferred: 0,
          total: 0,
          bytesPerSecond: 0,
        },
        message: `Downloading OpenNOW ${availableUpdateInfo.version}…`,
      });

      try {
        await updater.downloadUpdate();
      } catch (error) {
        updateState({
          status: "error",
          availableVersion: availableUpdateInfo.version,
          progress: undefined,
          message: normalizeErrorMessage(error),
          errorCode: error instanceof Error ? (error as Error & { code?: string }).code : undefined,
        });
      } finally {
        downloadInFlight = false;
        updateState({});
      }

      return state;
    },
    async quitAndInstall() {
      if (disposed || !downloadedUpdateInfo) {
        return state;
      }

      updateState({
        status: "downloaded",
        message: `Restarting to install OpenNOW ${downloadedUpdateInfo.version}…`,
      });

      setImmediate(() => {
        try {
          options.onBeforeQuitAndInstall?.();
          updater.quitAndInstall(false, true);
        } catch (error) {
          options.onQuitAndInstallError?.();
          updateState({
            status: "error",
            message: normalizeErrorMessage(error),
          });
        }
      });

      return state;
    },
  };

  return controller;
}
