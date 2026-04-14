import { app, type BrowserWindow } from "electron";
import electronUpdater from "electron-updater";
import type { ProgressInfo, UpdateInfo } from "electron-updater";

import type {
  ReleaseNotesArtifact,
  UpdaterDownloadProgress,
  UpdaterReleaseNotesSource,
  UpdaterState,
  UpdaterStatus,
} from "@shared/gfn";
import { IPC_CHANNELS } from "@shared/ipc";

import type { SettingsManager } from "./settings";

const { autoUpdater } = electronUpdater;

const RELEASE_NOTES_TIMEOUT_MS = 8000;
const GITHUB_RELEASES_DOWNLOAD_PREFIX = "/releases/download/";
const RELEASE_NOTES_ASSET_PREFIX = "release-notes-v";

function createInitialState(): UpdaterState {
  return {
    currentVersion: app.getVersion(),
    status: "idle",
    availableVersion: null,
    releaseName: null,
    releaseDate: null,
    releaseNotes: null,
    releaseNotesSource: "none",
    releaseTag: null,
    downloadProgress: null,
    lastCheckedAt: null,
    lastError: null,
    downloaded: false,
    canInstall: false,
    skippedVersion: null,
    isSkipped: false,
  };
}

function withTimeout<T>(promise: Promise<T>, timeoutMs: number, label: string): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error(`${label} timed out after ${timeoutMs}ms`)), timeoutMs);
    promise.then(
      (value) => {
        clearTimeout(timeout);
        resolve(value);
      },
      (error: unknown) => {
        clearTimeout(timeout);
        reject(error);
      },
    );
  });
}

function toIsoTimestamp(input: unknown): string | null {
  if (typeof input !== "string" || input.trim().length === 0) {
    return null;
  }
  const parsed = new Date(input);
  if (Number.isNaN(parsed.getTime())) {
    return null;
  }
  return parsed.toISOString();
}

function normalizeFallbackReleaseNotes(releaseNotes: UpdateInfo["releaseNotes"], version: string): string | null {
  if (typeof releaseNotes === "string") {
    const normalized = releaseNotes.trim();
    return normalized.length > 0 ? normalized : null;
  }

  if (!Array.isArray(releaseNotes)) {
    return null;
  }

  const parts = releaseNotes
    .map((entry) => {
      const note = entry.note?.trim();
      if (!note) {
        return null;
      }
      if (!entry.version || entry.version === version) {
        return note;
      }
      return `## ${entry.version}\n\n${note}`;
    })
    .filter((value): value is string => value !== null);

  return parts.length > 0 ? parts.join("\n\n") : null;
}

function parseReleaseTagFromInfo(updateInfo: UpdateInfo): string | null {
  const candidates = [...updateInfo.files.map((file) => file.url), updateInfo.path];

  for (const candidate of candidates) {
    if (typeof candidate !== "string" || candidate.length === 0) {
      continue;
    }

    try {
      const url = new URL(candidate);
      const markerIndex = url.pathname.indexOf(GITHUB_RELEASES_DOWNLOAD_PREFIX);
      if (markerIndex < 0) {
        continue;
      }

      const suffix = url.pathname.slice(markerIndex + GITHUB_RELEASES_DOWNLOAD_PREFIX.length);
      const [tag] = suffix.split("/", 1);
      if (tag) {
        return decodeURIComponent(tag);
      }
    } catch {
      const match = candidate.match(/\/releases\/download\/([^/]+)\//);
      if (match?.[1]) {
        return decodeURIComponent(match[1]);
      }
    }
  }

  return null;
}

function buildReleaseNotesAssetUrl(tag: string, version: string): string {
  return `https://github.com/OpenCloudGaming/OpenNOW/releases/download/${encodeURIComponent(tag)}/${RELEASE_NOTES_ASSET_PREFIX}${encodeURIComponent(version)}.json`;
}

function isReleaseNotesArtifact(value: unknown): value is ReleaseNotesArtifact {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return false;
  }

  const candidate = value as Record<string, unknown>;
  return typeof candidate.version === "string"
    && typeof candidate.tag === "string"
    && typeof candidate.notes === "string";
}

function sanitizeDownloadProgress(progressInfo: ProgressInfo): UpdaterDownloadProgress {
  return {
    percent: Number.isFinite(progressInfo.percent) ? progressInfo.percent : 0,
    transferred: progressInfo.transferred,
    total: progressInfo.total,
    bytesPerSecond: progressInfo.bytesPerSecond,
  };
}

export class UpdaterService {
  private initialized = false;
  private mainWindow: BrowserWindow | null = null;
  private settingsManager: SettingsManager | null = null;
  private state: UpdaterState = createInitialState();
  private startupCheckTimer: NodeJS.Timeout | null = null;

  initialize(settingsManager: SettingsManager): void {
    if (this.initialized) {
      this.settingsManager = settingsManager;
      this.state = {
        ...this.state,
        currentVersion: app.getVersion(),
        skippedVersion: settingsManager.get("skippedUpdateVersion") || null,
        isSkipped: this.state.availableVersion !== null && settingsManager.get("skippedUpdateVersion") === this.state.availableVersion,
      };
      this.broadcastState();
      return;
    }

    this.initialized = true;
    this.settingsManager = settingsManager;
    this.state = {
      ...this.state,
      currentVersion: app.getVersion(),
      skippedVersion: settingsManager.get("skippedUpdateVersion") || null,
    };

    autoUpdater.autoDownload = false;
    autoUpdater.autoInstallOnAppQuit = false;
    autoUpdater.fullChangelog = false;

    autoUpdater.on("checking-for-update", () => {
      this.setState({
        status: "checking",
        lastCheckedAt: new Date().toISOString(),
        lastError: null,
        downloadProgress: null,
        downloaded: false,
        canInstall: false,
      });
    });

    autoUpdater.on("update-available", (updateInfo) => {
      this.applyUpdateInfo(updateInfo, "available");
      void this.loadPreferredReleaseNotes(updateInfo);
    });

    autoUpdater.on("update-not-available", () => {
      this.setState({
        status: "not-available",
        availableVersion: null,
        releaseName: null,
        releaseDate: null,
        releaseNotes: null,
        releaseNotesSource: "none",
        releaseTag: null,
        downloadProgress: null,
        lastError: null,
        downloaded: false,
        canInstall: false,
        isSkipped: false,
      });
    });

    autoUpdater.on("download-progress", (progressInfo) => {
      this.setState({
        status: "downloading",
        downloadProgress: sanitizeDownloadProgress(progressInfo),
        lastError: null,
      });
    });

    autoUpdater.on("update-downloaded", (updateInfo) => {
      this.applyUpdateInfo(updateInfo, "downloaded");
      this.setState({
        downloaded: true,
        canInstall: true,
      });
    });

    autoUpdater.on("error", (error) => {
      this.setState({
        status: "error",
        lastError: error instanceof Error ? error.message : String(error),
      });
    });
  }

  setMainWindow(mainWindow: BrowserWindow | null): void {
    this.mainWindow = mainWindow && !mainWindow.isDestroyed() ? mainWindow : null;
    this.broadcastState();
  }

  getState(): UpdaterState {
    return { ...this.state };
  }

  async checkForUpdates(manual = false): Promise<UpdaterState> {
    if (!app.isPackaged) {
      if (manual) {
        this.setState({
          status: "error",
          lastError: "Auto-update is only available in packaged production builds.",
        });
      }
      return this.getState();
    }

    if (!manual && this.settingsManager && !this.settingsManager.get("autoCheckForUpdates")) {
      return this.getState();
    }

    const result = await autoUpdater.checkForUpdates();
    if (result?.isUpdateAvailable) {
      await this.loadPreferredReleaseNotes(result.updateInfo);
    }
    return this.getState();
  }

  async downloadUpdate(): Promise<UpdaterState> {
    if (!app.isPackaged) {
      this.setState({
        status: "error",
        lastError: "Auto-update is only available in packaged production builds.",
      });
      return this.getState();
    }

    await autoUpdater.downloadUpdate();
    return this.getState();
  }

  quitAndInstall(): void {
    if (!this.state.canInstall) {
      throw new Error("No downloaded update is ready to install.");
    }
    autoUpdater.quitAndInstall(false, true);
  }

  skipVersion(version: string): UpdaterState {
    const normalizedVersion = version.trim();
    if (!normalizedVersion) {
      throw new Error("A valid update version is required to skip an update.");
    }
    this.settingsManager?.set("skippedUpdateVersion", normalizedVersion);
    this.setState({
      skippedVersion: normalizedVersion,
      isSkipped: this.state.availableVersion === normalizedVersion,
    });
    return this.getState();
  }

  clearSkippedVersion(): UpdaterState {
    this.settingsManager?.set("skippedUpdateVersion", "");
    this.setState({
      skippedVersion: null,
      isSkipped: false,
    });
    return this.getState();
  }

  scheduleStartupCheck(delayMs = 12000): void {
    if (!app.isPackaged || !this.settingsManager?.get("autoCheckForUpdates")) {
      return;
    }

    if (this.startupCheckTimer) {
      clearTimeout(this.startupCheckTimer);
    }

    this.startupCheckTimer = setTimeout(() => {
      this.startupCheckTimer = null;
      void this.checkForUpdates(false).catch((error: unknown) => {
        console.warn("[Updater] Startup update check failed:", error);
      });
    }, delayMs);
  }

  private applyUpdateInfo(updateInfo: UpdateInfo, status: UpdaterStatus): void {
    const fallbackNotes = normalizeFallbackReleaseNotes(updateInfo.releaseNotes, updateInfo.version);
    const skippedVersion = this.settingsManager?.get("skippedUpdateVersion") || null;
    this.setState({
      status,
      availableVersion: updateInfo.version,
      releaseName: updateInfo.releaseName ?? null,
      releaseDate: toIsoTimestamp(updateInfo.releaseDate),
      releaseNotes: fallbackNotes,
      releaseNotesSource: fallbackNotes ? "feed" : "none",
      releaseTag: parseReleaseTagFromInfo(updateInfo),
      downloadProgress: status === "downloaded" ? this.state.downloadProgress : null,
      lastError: null,
      downloaded: status === "downloaded",
      canInstall: status === "downloaded",
      skippedVersion,
      isSkipped: skippedVersion === updateInfo.version,
    });
  }

  private setState(patch: Partial<UpdaterState>): void {
    this.state = {
      ...this.state,
      ...patch,
      currentVersion: app.getVersion(),
    };
    this.broadcastState();
  }

  private broadcastState(): void {
    if (!this.mainWindow || this.mainWindow.isDestroyed()) {
      return;
    }
    this.mainWindow.webContents.send(IPC_CHANNELS.UPDATES_STATE_CHANGED, this.getState());
  }

  private async loadPreferredReleaseNotes(updateInfo: UpdateInfo): Promise<void> {
    const tag = parseReleaseTagFromInfo(updateInfo);
    const fallbackNotes = normalizeFallbackReleaseNotes(updateInfo.releaseNotes, updateInfo.version);
    const fallbackState: Pick<UpdaterState, "releaseNotes" | "releaseNotesSource" | "releaseTag"> = {
      releaseNotes: fallbackNotes,
      releaseNotesSource: fallbackNotes ? "feed" : "none",
      releaseTag: tag,
    };

    if (!tag) {
      this.setState(fallbackState);
      return;
    }

    try {
      const response = await withTimeout(
        fetch(buildReleaseNotesAssetUrl(tag, updateInfo.version), {
          headers: {
            Accept: "application/json",
            "User-Agent": `OpenNOW/${app.getVersion()}`,
          },
        }),
        RELEASE_NOTES_TIMEOUT_MS,
        "Release notes request",
      );

      if (!response.ok) {
        this.setState(fallbackState);
        return;
      }

      const payload = await withTimeout(response.json() as Promise<unknown>, RELEASE_NOTES_TIMEOUT_MS, "Release notes parse");
      if (!isReleaseNotesArtifact(payload)) {
        this.setState(fallbackState);
        return;
      }

      if (payload.version !== updateInfo.version || payload.tag !== tag) {
        this.setState(fallbackState);
        return;
      }

      const artifactNotes = payload.notes.trim();
      this.setState({
        releaseName: payload.releaseName?.trim() || updateInfo.releaseName || null,
        releaseDate: toIsoTimestamp(payload.releaseDate) ?? toIsoTimestamp(updateInfo.releaseDate),
        releaseNotes: artifactNotes || fallbackNotes,
        releaseNotesSource: artifactNotes ? "artifact" : (fallbackNotes ? "feed" : "none") as UpdaterReleaseNotesSource,
        releaseTag: payload.tag,
      });
    } catch (error) {
      console.warn("[Updater] Failed to load immutable release notes artifact:", error);
      this.setState(fallbackState);
    }
  }
}

let updaterService: UpdaterService | null = null;

export function getUpdaterService(): UpdaterService {
  if (!updaterService) {
    updaterService = new UpdaterService();
  }
  return updaterService;
}
