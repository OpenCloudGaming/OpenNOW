import { app, BrowserWindow, ipcMain, dialog, shell, systemPreferences, session } from "electron";
import * as net from "node:net";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve, relative } from "node:path";
import { existsSync, readFileSync, createWriteStream } from "node:fs";
import { copyFile, mkdir, readdir, readFile, rename, stat, unlink, writeFile, realpath } from "node:fs/promises";
import { randomUUID, createHash } from "node:crypto";
import { spawn } from "node:child_process";

// Keyboard shortcuts reference (matching Rust implementation):
// Screenshot keybind - configurable, handled in renderer
// F3  - Toggle stats overlay (handled in renderer)
// Ctrl+Shift+Q - Stop streaming (handled in renderer)
// F8  - Toggle mouse/pointer lock (handled in main process via IPC)

import { IPC_CHANNELS } from "@shared/ipc";
import { initLogCapture, exportLogs } from "@shared/logger";
import { cacheManager } from "./services/cacheManager";
import { refreshScheduler } from "./services/refreshScheduler";
import { cacheEventBus } from "./services/cacheEventBus";
import {
  fetchMainGamesUncached,
  fetchLibraryGamesUncached,
  fetchPublicGamesUncached,
} from "./gfn/games";
import type {
  ActiveSessionInfo,
  ExistingSessionStrategy,
  MainToRendererSignalingEvent,
  AppUpdaterState,
  AuthLoginRequest,
  SessionInfo,
  AuthSessionRequest,
  GamesFetchRequest,
  CatalogBrowseRequest,
  ResolveLaunchIdRequest,
  RegionsFetchRequest,
  SessionAdReportRequest,
  SessionCreateRequest,
  SessionPollRequest,
  SessionStopRequest,
  SessionClaimRequest,
  SignalingConnectRequest,
  SendAnswerRequest,
  IceCandidatePayload,
  KeyframeRequest,
  Settings,
  SubscriptionFetchRequest,
  SessionConflictChoice,
  PingResult,
  StreamRegion,
  VideoAccelerationPreference,
  ScreenshotDeleteRequest,
  ScreenshotEntry,
  ScreenshotSaveAsRequest,
  ScreenshotSaveAsResult,
  ScreenshotSaveRequest,
  RecordingEntry,
  RecordingBeginRequest,
  RecordingBeginResult,
  RecordingChunkRequest,
  RecordingFinishRequest,
  RecordingAbortRequest,
  RecordingDeleteRequest,
  MicrophonePermissionResult,
  ThankYouContributor,
  ThankYouDataResult,
  ThankYouSupporter,
} from "@shared/gfn";
import { serializeSessionErrorTransport } from "@shared/sessionError";

import { getSettingsManager, type SettingsManager } from "./settings";

import { createSession, pollSession, reportSessionAd, stopSession, getActiveSessions, claimSession } from "./gfn/cloudmatch";
import { AuthService } from "./gfn/auth";
import {
  browseCatalog,
  fetchLibraryGames,
  fetchMainGames,
  fetchPublicGames,
  resolveLaunchAppId,
} from "./gfn/games";
import { fetchSubscription, fetchDynamicRegions } from "./gfn/subscription";
import { GfnSignalingClient } from "./gfn/signaling";
import { isSessionError, SessionError, GfnErrorCode } from "./gfn/errorCodes";
import { connectDiscordRpc, setActivity, clearActivity, destroyDiscordRpc, getCurrentActivity, isDiscordRpcConnected } from "./discordRpc";
import { createAppUpdaterController, type AppUpdaterController } from "./updater";
import type { MainIpcDeps, SignalingState, ActiveRecording } from "./ipc/types";
import { registerMainIpcHandlers } from "./ipc/registerAll";
import { fetchWithTimeout, withTimeout } from "./lib/httpFetch";
import {
  buildScreenshotDataUrl,
  dataUrlToBuffer,
  sanitizeTitleForFileName,
  assertSafeScreenshotId,
  assertSafeRecordingId,
} from "./lib/imageDataUrl";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

// Configure Chromium video and WebRTC behavior before app.whenReady().

interface BootstrapVideoPreferences {
  decoderPreference: VideoAccelerationPreference;
  encoderPreference: VideoAccelerationPreference;
}

function isAccelerationPreference(value: unknown): value is VideoAccelerationPreference {
  return value === "auto" || value === "hardware" || value === "software";
}

function loadBootstrapVideoPreferences(): BootstrapVideoPreferences {
  const defaults: BootstrapVideoPreferences = {
    decoderPreference: "auto",
    encoderPreference: "auto",
  };
  try {
    const settingsPath = join(app.getPath("userData"), "settings.json");
    if (!existsSync(settingsPath)) {
      return defaults;
    }
    const parsed = JSON.parse(readFileSync(settingsPath, "utf-8")) as Partial<BootstrapVideoPreferences>;
    return {
      decoderPreference: isAccelerationPreference(parsed.decoderPreference)
        ? parsed.decoderPreference
        : defaults.decoderPreference,
      encoderPreference: isAccelerationPreference(parsed.encoderPreference)
        ? parsed.encoderPreference
        : defaults.encoderPreference,
    };
  } catch {
    return defaults;
  }
}

const bootstrapVideoPrefs = loadBootstrapVideoPreferences();
console.log(
  `[Main] Video acceleration preference: decode=${bootstrapVideoPrefs.decoderPreference}, encode=${bootstrapVideoPrefs.encoderPreference}`,
);

// --- Platform-specific HW video decode features ---
const platformFeatures: string[] = [];
const isLinuxArm = process.platform === "linux" && (process.arch === "arm64" || process.arch === "arm");

if (process.platform === "win32") {
  // Windows: D3D11 + Media Foundation path for HW decode/encode acceleration
  if (bootstrapVideoPrefs.decoderPreference !== "software") {
    platformFeatures.push("D3D11VideoDecoder");
  }
  if (
    bootstrapVideoPrefs.decoderPreference !== "software" ||
    bootstrapVideoPrefs.encoderPreference !== "software"
  ) {
    platformFeatures.push("MediaFoundationD3D11VideoCapture");
  }
} else if (process.platform === "linux") {
  if (isLinuxArm) {
    // Raspberry Pi/Linux ARM: allow Chromium's direct V4L2 decoder path.
    if (bootstrapVideoPrefs.decoderPreference !== "software") {
      platformFeatures.push("UseChromeOSDirectVideoDecoder");
    }
  } else {
    // Linux x64 desktop GPUs: VA-API path (Intel/AMD).
    if (bootstrapVideoPrefs.decoderPreference !== "software") {
      platformFeatures.push("VaapiVideoDecoder");
    }
    if (bootstrapVideoPrefs.encoderPreference !== "software") {
      platformFeatures.push("VaapiVideoEncoder");
    }
    if (
      bootstrapVideoPrefs.decoderPreference !== "software" ||
      bootstrapVideoPrefs.encoderPreference !== "software"
    ) {
      platformFeatures.push("VaapiIgnoreDriverChecks");
    }
  }
}
// macOS: VideoToolbox handles HW acceleration natively, no extra feature flags needed

app.commandLine.appendSwitch("enable-features",
  [
    // --- MP4 recording via MediaRecorder (Chromium 127+) ---
    "MediaRecorderEnableMp4Muxer",
    // --- AV1 support (cross-platform) ---
    "Dav1dVideoDecoder", // Fast AV1 software fallback via dav1d (if no HW decoder)
    // --- Additional (cross-platform) ---
    "HardwareMediaKeyHandling",
    // --- Platform-specific HW decode/encode ---
    ...platformFeatures,
  ].join(","),
);

const disableFeatures: string[] = [
  // Prevents mDNS candidate generation — faster ICE connectivity
  "WebRtcHideLocalIpsWithMdns",
];
if (process.platform === "linux" && !isLinuxArm) {
  // ChromeOS-only direct video decoder path interferes on regular Linux
  disableFeatures.push("UseChromeOSDirectVideoDecoder");
}
app.commandLine.appendSwitch("disable-features", disableFeatures.join(","));

app.commandLine.appendSwitch("force-fieldtrials",
  [
    // Disable send-side pacing — we are receive-only, pacing adds latency to RTCP feedback
    "WebRTC-Video-Pacing/Disabled/",
  ].join("/"),
);

if (bootstrapVideoPrefs.decoderPreference === "hardware") {
  app.commandLine.appendSwitch("enable-accelerated-video-decode");
} else if (bootstrapVideoPrefs.decoderPreference === "software") {
  app.commandLine.appendSwitch("disable-accelerated-video-decode");
}

if (bootstrapVideoPrefs.encoderPreference === "hardware") {
  app.commandLine.appendSwitch("enable-accelerated-video-encode");
} else if (bootstrapVideoPrefs.encoderPreference === "software") {
  app.commandLine.appendSwitch("disable-accelerated-video-encode");
}

// Ensure the GPU process doesn't blocklist our GPU for video decode
app.commandLine.appendSwitch("ignore-gpu-blocklist");

// --- Responsiveness flags ---
// Keep default compositor frame pacing (vsync + frame cap) to avoid runaway
// CPU usage from uncapped UI animations.
// Prevent renderer throttling when the window is backgrounded or occluded.
app.commandLine.appendSwitch("disable-renderer-backgrounding");
app.commandLine.appendSwitch("disable-backgrounding-occluded-windows");
// Remove getUserMedia FPS cap (not strictly needed for receive-only but avoids potential limits)
app.commandLine.appendSwitch("max-gum-fps", "999");

let mainWindow: BrowserWindow | null = null;
const signalingState: SignalingState = { client: null, key: null };
let authService: AuthService;
let settingsManager: SettingsManager;
let appUpdater: AppUpdaterController | null = null;
const SCREENSHOT_LIMIT = 60;
const EXPLICIT_SHUTDOWN_FORCE_EXIT_DELAY_MS = 2000;
let isShutdownRequested = false;
let isShutdownCleanupComplete = false;
let isUpdaterInstallQuitInProgress = false;
let explicitShutdownFallbackTimer: NodeJS.Timeout | null = null;

function clearExplicitShutdownFallback(): void {
  if (explicitShutdownFallbackTimer) {
    clearTimeout(explicitShutdownFallbackTimer);
    explicitShutdownFallbackTimer = null;
  }
}

function runShutdownCleanup(reason = "app-quit"): void {
  if (isShutdownCleanupComplete) {
    return;
  }

  isShutdownCleanupComplete = true;
  console.log(`[Main] Running shutdown cleanup (${reason})`);

  refreshScheduler.stop();
  signalingState.client?.disconnect();
  signalingState.client = null;
  signalingState.key = null;
  void destroyDiscordRpc();
  appUpdater?.dispose();
  appUpdater = null;

  const windowToClose = mainWindow;
  if (windowToClose && !windowToClose.isDestroyed()) {
    mainWindow = null;
    try {
      windowToClose.close();
    } catch (error) {
      console.warn("[Main] Failed to close main window during shutdown:", error);
    }

    if (!windowToClose.isDestroyed()) {
      try {
        windowToClose.destroy();
      } catch (error) {
        console.warn("[Main] Failed to destroy main window during shutdown:", error);
      }
    }
  }
}

function scheduleExplicitShutdownFallback(reason: string, exitCode = 0): void {
  if (explicitShutdownFallbackTimer || isUpdaterInstallQuitInProgress) {
    return;
  }

  explicitShutdownFallbackTimer = setTimeout(() => {
    explicitShutdownFallbackTimer = null;
    console.warn(`[Main] Explicit shutdown fallback triggered (${reason}); forcing process exit.`);
    app.exit(exitCode);
  }, EXPLICIT_SHUTDOWN_FORCE_EXIT_DELAY_MS);
  explicitShutdownFallbackTimer.unref?.();
}

function requestAppShutdown(options: { reason?: string; forceExitFallback?: boolean; exitCode?: number } = {}): void {
  const { reason = "app-quit", forceExitFallback = false, exitCode = 0 } = options;

  if (!isShutdownRequested) {
    isShutdownRequested = true;
    discordMonitor.stop();
    runShutdownCleanup(reason);
  }

  if (forceExitFallback) {
    scheduleExplicitShutdownFallback(reason, exitCode);
  }

  app.quit();
}

/**
 * Periodically verifies that the Discord Rich Presence status accurately
 * reflects the user's actual game session state.
 */
class DiscordStatusMonitor {
  private timer: NodeJS.Timeout | null = null;
  private readonly intervalMs = 60 * 1000;
  private isSyncing = false;
  private hasPerformedInitialSync = false;

  start(): void {
    if (this.timer) return;
    this.timer = setInterval(() => void this.sync(), this.intervalMs);
    void this.sync();
  }

  stop(): void {
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = null;
    }
  }

  async sync(): Promise<void> {
    if (this.isSyncing) return;
    this.isSyncing = true;

    try {
      if (!settingsManager.get("discordRichPresence")) {
        this.stop();
        void clearActivity();
        return;
      }

      if (!isDiscordRpcConnected()) {
        await connectDiscordRpc().catch(() => {});
      }

      // On first run, always clear regardless of auth state — the app just started
      // and any stale status from the previous session must be wiped.
      if (!this.hasPerformedInitialSync) {
        console.log("[DiscordRPC] Startup: clearing any stale Discord status.");
        await clearActivity().catch(() => {});
        this.hasPerformedInitialSync = true;
      }

      const token = await resolveJwt().catch(() => null);
      if (!token) return;

      const provider = authService.getSelectedProvider();
      const streamingBaseUrl = provider.streamingServiceUrl;
      const activeSessions = await getActiveSessions(token, streamingBaseUrl).catch(() => []);

      const activeSession = activeSessions.find((s) => [1, 2, 3].includes(s.status));
      const currentActivity = getCurrentActivity();

      if (activeSession) {
        const sessionAppId = activeSession.appId.toString();

        if (!currentActivity || currentActivity.appId !== sessionAppId) {
          const title = sessionAppId;
          const startTime = new Date();
          void setActivity(title, startTime, sessionAppId);
        }
      } else if (currentActivity) {
        console.log("[DiscordRPC] Monitor clearing stale status.");
        void clearActivity();
      }
    } catch (err) {
      console.warn("[DiscordRPC] Monitor sync failed:", (err as Error).message);
    } finally {
      this.isSyncing = false;
    }
  }
}

const discordMonitor = new DiscordStatusMonitor();

function getScreenshotDirectory(): string {
  return join(app.getPath("pictures"), "OpenNOW", "Screenshots");
}

async function ensureScreenshotDirectory(): Promise<string> {
  const dir = getScreenshotDirectory();
  await mkdir(dir, { recursive: true });
  return dir;
}

async function listScreenshots(): Promise<ScreenshotEntry[]> {
  const dir = await ensureScreenshotDirectory();
  const entries = await readdir(dir, { withFileTypes: true });
  const screenshotFiles = entries
    .filter((entry) => entry.isFile())
    .map((entry) => entry.name)
    .filter((name) => /\.(png|jpg|jpeg|webp)$/i.test(name));

  const loaded = await Promise.all(
    screenshotFiles.map(async (fileName): Promise<ScreenshotEntry | null> => {
      const filePath = join(dir, fileName);
      try {
        const fileStats = await stat(filePath);
        const fileBuffer = await readFile(filePath);
        const extMatch = /\.([^.]+)$/.exec(fileName);
        const ext = (extMatch?.[1] ?? "png").toLowerCase();

        return {
          id: fileName,
          fileName,
          filePath,
          createdAtMs: fileStats.birthtimeMs || fileStats.mtimeMs,
          sizeBytes: fileStats.size,
          dataUrl: buildScreenshotDataUrl(ext, fileBuffer),
        };
      } catch {
        return null;
      }
    }),
  );

  return loaded
    .filter((item): item is ScreenshotEntry => item !== null)
    .sort((a, b) => b.createdAtMs - a.createdAtMs)
    .slice(0, SCREENSHOT_LIMIT);
}

async function saveScreenshot(input: ScreenshotSaveRequest): Promise<ScreenshotEntry> {
  const { ext, buffer } = dataUrlToBuffer(input.dataUrl);
  const dir = await ensureScreenshotDirectory();
  const stamp = new Date().toISOString().replace(/[:.]/g, "-");
  const title = sanitizeTitleForFileName(input.gameTitle);
  const fileName = `${stamp}-${title}-${Math.random().toString(16).slice(2, 8)}.${ext}`;
  const filePath = join(dir, fileName);

  await writeFile(filePath, buffer);

  return {
    id: fileName,
    fileName,
    filePath,
    createdAtMs: Date.now(),
    sizeBytes: buffer.byteLength,
    dataUrl: buildScreenshotDataUrl(ext, buffer),
  };
}

async function deleteScreenshot(input: ScreenshotDeleteRequest): Promise<void> {
  assertSafeScreenshotId(input.id);
  const dir = await ensureScreenshotDirectory();
  const filePath = join(dir, input.id);
  await unlink(filePath);
}

async function saveScreenshotAs(input: ScreenshotSaveAsRequest): Promise<ScreenshotSaveAsResult> {
  assertSafeScreenshotId(input.id);
  const dir = await ensureScreenshotDirectory();
  const sourcePath = join(dir, input.id);

  const saveDialogOptions = {
    title: "Save Screenshot",
    defaultPath: join(app.getPath("pictures"), input.id),
    filters: [
      { name: "PNG Image", extensions: ["png"] },
      { name: "JPEG Image", extensions: ["jpg", "jpeg"] },
      { name: "WebP Image", extensions: ["webp"] },
      { name: "All Files", extensions: ["*"] },
    ],
  };
  const target =
    mainWindow && !mainWindow.isDestroyed()
      ? await dialog.showSaveDialog(mainWindow, saveDialogOptions)
      : await dialog.showSaveDialog(saveDialogOptions);

  if (target.canceled || !target.filePath) {
    return { saved: false };
  }

  await copyFile(sourcePath, target.filePath);
  return { saved: true, filePath: target.filePath };
}

// ---------------------------------------------------------------------------
// Recording helpers
// ---------------------------------------------------------------------------

const RECORDING_LIMIT = 20;

const activeRecordings = new Map<string, ActiveRecording>();

function getRecordingsDirectory(): string {
  return join(app.getPath("pictures"), "OpenNOW", "Recordings");
}

function getThumbnailCacheDirectory(): string {
  return join(app.getPath("userData"), "media-thumbs");
}

async function ensureThumbnailCacheDirectory(): Promise<string> {
  const dir = getThumbnailCacheDirectory();
  await mkdir(dir, { recursive: true });
  return dir;
}

function md5(input: string): string {
  return createHash("md5").update(input).digest("hex");
}

async function generateVideoThumbnail(sourcePath: string, outPath: string): Promise<boolean> {
  return new Promise<boolean>((resolve) => {
    // Try to run ffmpeg to extract a frame at 1s.
    const args = ["-y", "-ss", "1", "-i", sourcePath, "-frames:v", "1", "-q:v", "2", outPath];
    const child = spawn("ffmpeg", args, { stdio: "ignore" });
    child.on("error", () => resolve(false));
    child.on("close", (code) => {
      resolve(code === 0);
    });
  });
}

async function ensureThumbnailForMedia(filePath: string): Promise<string | null> {
  try {
    const stats = await stat(filePath);
    const key = md5(`${filePath}|${stats.mtimeMs}`);
    const cacheDir = await ensureThumbnailCacheDirectory();
    const outPath = join(cacheDir, `${key}.jpg`);
    // If cached, return
    try {
      await stat(outPath);
      return outPath;
    } catch {
      // not exists
    }

    const lower = filePath.toLowerCase();
    if (lower.endsWith(".mp4") || lower.endsWith(".webm") || lower.endsWith(".mkv") || lower.endsWith(".mov")) {
      const ok = await generateVideoThumbnail(filePath, outPath);
      if (ok) return outPath;
      // generation failed
      return null;
    }

    // For images, copy into cache (no re-encoding)
    if (lower.endsWith(".png") || lower.endsWith(".jpg") || lower.endsWith(".jpeg") || lower.endsWith(".webp")) {
      try {
        const buf = await readFile(filePath);
        await writeFile(outPath, buf);
        return outPath;
      } catch {
        return null;
      }
    }

    return null;
  } catch (err) {
    console.warn("ensureThumbnailForMedia error:", err);
    return null;
  }
}

async function ensureRecordingsDirectory(): Promise<string> {
  const dir = getRecordingsDirectory();
  await mkdir(dir, { recursive: true });
  return dir;
}

function extFromMimeType(mimeType: string): ".mp4" | ".webm" {
  return mimeType.startsWith("video/mp4") ? ".mp4" : ".webm";
}

async function listRecordings(): Promise<RecordingEntry[]> {
  const dir = await ensureRecordingsDirectory();
  const entries = await readdir(dir, { withFileTypes: true });
  const webmFiles = entries
    .filter((e) => e.isFile())
    .map((e) => e.name)
    .filter((name) => /\.(mp4|webm)$/i.test(name));

  const loaded = await Promise.all(
    webmFiles.map(async (fileName): Promise<RecordingEntry | null> => {
      const filePath = join(dir, fileName);
      try {
        const fileStats = await stat(filePath);
        const stem = fileName.replace(/\.webm$/i, "");
        const thumbName = `${stem}-thumb.jpg`;
        const thumbPath = join(dir, thumbName);

        let thumbnailDataUrl: string | undefined;
        try {
          const thumbBuf = await readFile(thumbPath);
          thumbnailDataUrl = `data:image/jpeg;base64,${thumbBuf.toString("base64")}`;
        } catch {
          // No thumbnail for this recording — that's fine
        }

        // Parse durationMs encoded in filename as last numeric segment before extension
        const durMatch = /-dur(\d+)\.(mp4|webm)$/i.exec(fileName);
        const durationMs = durMatch ? Number(durMatch[1]) : 0;

        // Parse game title from filename: {stamp}-{title}-{rand}[-dur{ms}].{ext}
        const titleMatch = /^[^-]+-[^-]+-([^-]+(?:-[^-]+)*?)-[a-f0-9]{6}(?:-dur\d+)?\.(mp4|webm)$/i.exec(fileName);
        const gameTitle = titleMatch ? titleMatch[1].replace(/-/g, " ") : undefined;

        return {
          id: fileName,
          fileName,
          filePath,
          createdAtMs: fileStats.birthtimeMs || fileStats.mtimeMs,
          sizeBytes: fileStats.size,
          durationMs,
          gameTitle,
          thumbnailDataUrl,
        };
      } catch {
        return null;
      }
    }),
  );

  return loaded
    .filter((item): item is RecordingEntry => item !== null)
    .sort((a, b) => b.createdAtMs - a.createdAtMs)
    .slice(0, RECORDING_LIMIT);
}

function emitToRenderer(event: MainToRendererSignalingEvent): void {
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send(IPC_CHANNELS.SIGNALING_EVENT, event);
  }
}

function emitUpdaterStateToRenderer(state: AppUpdaterState): void {
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send(IPC_CHANNELS.APP_UPDATER_STATE_CHANGED, state);
  }
}

async function createMainWindow(): Promise<void> {
  const preloadMjsPath = join(__dirname, "../preload/index.mjs");
  const preloadJsPath = join(__dirname, "../preload/index.js");
  const preloadPath = existsSync(preloadMjsPath) ? preloadMjsPath : preloadJsPath;

  const settings = settingsManager.getAll();

  mainWindow = new BrowserWindow({
    width: settings.windowWidth || 1400,
    height: settings.windowHeight || 900,
    minWidth: 1024,
    minHeight: 680,
    autoHideMenuBar: true,
    backgroundColor: "#0f172a",
    webPreferences: {
      preload: preloadPath,
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false,
    },
  });

  if (process.platform === "win32") {
    // Keep native window fullscreen in sync with HTML fullscreen so Windows treats
    // stream playback like a real fullscreen window instead of only DOM fullscreen.
    mainWindow.webContents.on("enter-html-full-screen", () => {
      if (mainWindow && !mainWindow.isDestroyed() && !mainWindow.isFullScreen()) {
        mainWindow.setFullScreen(true);
      }
    });

    mainWindow.webContents.on("leave-html-full-screen", () => {
      if (mainWindow && !mainWindow.isDestroyed() && mainWindow.isFullScreen()) {
        mainWindow.setFullScreen(false);
      }
    });
  }

  if (process.env.ELECTRON_RENDERER_URL) {
    await mainWindow.loadURL(process.env.ELECTRON_RENDERER_URL);
  } else {
    await mainWindow.loadFile(join(__dirname, "../../dist/index.html"));
  }

  mainWindow.on("resize", () => {
    if (mainWindow && !mainWindow.isDestroyed()) {
      const [width, height] = mainWindow.getSize();
      settingsManager.set("windowWidth", width);
      settingsManager.set("windowHeight", height);
    }
  });

  mainWindow.on("closed", () => {
    mainWindow = null;
  });
}

async function resolveJwt(token?: string): Promise<string> {
  return authService.resolveJwtToken(token);
}

/**
 * Show a dialog asking the user how to handle a session conflict
 * Returns the user's choice: "resume", "new", or "cancel"
 */
async function showSessionConflictDialog(): Promise<SessionConflictChoice> {
  if (!mainWindow || mainWindow.isDestroyed()) {
    return "cancel";
  }

  const result = await dialog.showMessageBox(mainWindow, {
    type: "question",
    buttons: ["Resume", "Start New", "Cancel"],
    defaultId: 0,
    cancelId: 2,
    title: "Active Session Detected",
    message: "You have an active session running.",
    detail: "Resume it or start a new one?",
  });

  switch (result.response) {
    case 0:
      return "resume";
    case 1:
      return "new";
    default:
      return "cancel";
  }
}

/**
 * Check if an error indicates a session conflict
 */
function isSessionConflictError(error: unknown): boolean {
  if (isSessionError(error)) {
    return error.isSessionConflict();
  }
  return false;
}

function rethrowSerializedSessionError(error: unknown): never {
  if (error instanceof SessionError) {
    throw new Error(serializeSessionErrorTransport(error.toJSON()));
  }
  throw error;
}

const AUTO_RESUME_SESSION_STATUSES = new Set([2, 3]);
const ACTIVE_CREATE_SESSION_STATUSES = new Set([1, 2, 3]);

function shouldForceNewSession(strategy: ExistingSessionStrategy | undefined): boolean {
  return strategy === "force-new";
}

function isAutoResumeReadySession(entry: ActiveSessionInfo): boolean {
  return entry.serverIp != null && AUTO_RESUME_SESSION_STATUSES.has(entry.status);
}

function isActiveCreateSessionConflict(entry: ActiveSessionInfo): boolean {
  return ACTIVE_CREATE_SESSION_STATUSES.has(entry.status);
}

function selectReadySessionToClaim(activeSessions: ActiveSessionInfo[], numericAppId: number): ActiveSessionInfo | null {
  return (
    activeSessions.find((session) => isAutoResumeReadySession(session) && session.appId === numericAppId) ??
    activeSessions.find((session) => isAutoResumeReadySession(session)) ??
    null
  );
}

function selectLaunchingSession(activeSessions: ActiveSessionInfo[], numericAppId: number): ActiveSessionInfo | null {
  return (
    activeSessions.find((session) => session.serverIp && session.appId === numericAppId && session.status === 1) ??
    activeSessions.find((session) => session.serverIp && session.status === 1) ??
    null
  );
}

async function stopActiveSessionsForCreate(params: {
  token: string;
  streamingBaseUrl: string;
  zone: string;
  appId: string;
}): Promise<void> {
  const { token, streamingBaseUrl, zone, appId } = params;
  const numericAppId = Number.parseInt(appId, 10);
  const activeSessions = await getActiveSessions(token, streamingBaseUrl);
  const sessionsToStop = activeSessions.filter(isActiveCreateSessionConflict);
  if (sessionsToStop.length === 0) {
    return;
  }

  console.log(
    `[CreateSession] Force-new requested; stopping ${sessionsToStop.length} existing active session(s) before create.`,
  );

  for (const activeSession of sessionsToStop) {
    if (!activeSession.serverIp) {
      console.warn(
        `[CreateSession] Cannot stop existing session ${activeSession.sessionId} (appId=${activeSession.appId}, status=${activeSession.status}) because serverIp is missing.`,
      );
      continue;
    }
    console.log(
      `[CreateSession] Stopping existing session id=${activeSession.sessionId}, appId=${activeSession.appId}, status=${activeSession.status}` +
        `${activeSession.appId === numericAppId ? " (same app)" : ""}.`,
    );
    await stopSession({
      token,
      streamingBaseUrl,
      serverIp: activeSession.serverIp,
      zone,
      sessionId: activeSession.sessionId,
    });
  }
}

const THANKS_CONTRIBUTORS_URL = "https://api.github.com/repos/OpenCloudGaming/OpenNOW/contributors?per_page=100";
const THANKS_SUPPORTERS_URL = "https://github.com/sponsors/zortos293";
const THANKS_REQUEST_HEADERS = {
  Accept: "application/vnd.github+json",
  "User-Agent": "OpenNOW-DesktopClient",
} as const;
const THANKS_EXCLUDED_PATTERN = /(copilot|claude|cappy)/i;
const THANKS_FETCH_TIMEOUT_MS = 8000;

interface GitHubContributorResponse {
  login?: string;
  avatar_url?: string;
  html_url?: string;
  contributions?: number;
  type?: string;
  name?: string | null;
}

function decodeHtmlEntities(value: string): string {
  return value
    .replace(/&amp;/g, "&")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">");
}

function stripHtml(value: string): string {
  return decodeHtmlEntities(value.replace(/<[^>]+>/g, " ")).replace(/\s+/g, " ").trim();
}

function normalizeUrl(value: string | undefined): string | undefined {
  if (!value) return undefined;
  const decoded = decodeHtmlEntities(value.trim());
  if (!decoded) return undefined;
  if (decoded.startsWith("//")) return `https:${decoded}`;
  if (decoded.startsWith("/")) return `https://github.com${decoded}`;
  return decoded;
}

function shouldExcludeContributor(contributor: GitHubContributorResponse): boolean {
  const login = contributor.login?.trim() ?? "";
  const name = contributor.name?.trim() ?? "";
  if (!login || !contributor.avatar_url || !contributor.html_url) return true;
  if (contributor.type === "Bot") return true;
  if (/\[bot\]$/i.test(login)) return true;
  if (THANKS_EXCLUDED_PATTERN.test(login) || THANKS_EXCLUDED_PATTERN.test(name)) return true;
  return false;
}

async function fetchThanksContributors(): Promise<ThankYouContributor[]> {
  const response = await fetchWithTimeout(
    THANKS_CONTRIBUTORS_URL,
    { headers: THANKS_REQUEST_HEADERS },
    THANKS_FETCH_TIMEOUT_MS,
    "GitHub contributors request",
  );
  if (!response.ok) {
    throw new Error(`GitHub contributors request failed (${response.status})`);
  }

  const payload = (await withTimeout(response.json() as Promise<GitHubContributorResponse[]>, THANKS_FETCH_TIMEOUT_MS, "GitHub contributors response")) as GitHubContributorResponse[];
  if (!Array.isArray(payload)) {
    throw new Error("GitHub contributors response was not an array");
  }

  const contributors = payload
    .filter((contributor) => !shouldExcludeContributor(contributor))
    .map((contributor) => ({
      login: contributor.login!.trim(),
      avatarUrl: contributor.avatar_url!,
      profileUrl: contributor.html_url!,
      contributions: typeof contributor.contributions === "number" ? contributor.contributions : 0,
    }))
    .sort((a, b) => b.contributions - a.contributions || a.login.localeCompare(b.login));
  return contributors;
}

function parseSupporterName(entryHtml: string): { name: string; isPrivate: boolean } {
  const privateHrefMatch = entryHtml.match(/href="https:\/\/docs\.github\.com\/sponsors\/sponsoring-open-source-contributors\/managing-your-sponsorship#managing-the-privacy-setting-for-your-sponsorship"/i);
  const privateTooltipMatch = entryHtml.match(/<tool-tip[^>]*>\s*Private Sponsor\s*<\/tool-tip>/i);
  const privateAriaMatch = entryHtml.match(/aria-label="Private Sponsor"/i);
  if (privateHrefMatch || privateTooltipMatch || privateAriaMatch) {
    return { name: "Private", isPrivate: true };
  }

  const altMatch = entryHtml.match(/<img[^>]+alt="([^"]+)"/i);
  const altText = altMatch ? stripHtml(altMatch[1]) : "";
  const normalizedAlt = altText.replace(/^@/, "").trim();
  if (normalizedAlt) {
    return { name: normalizedAlt, isPrivate: false };
  }

  const ariaMatch = entryHtml.match(/aria-label="([^"]+)"/i);
  const ariaText = ariaMatch ? stripHtml(ariaMatch[1]) : "";
  const normalizedAria = ariaText.replace(/^@/, "").trim();
  if (normalizedAria && !/private sponsor/i.test(normalizedAria)) {
    return { name: normalizedAria, isPrivate: false };
  }

  const hrefMatch = entryHtml.match(/<a[^>]+href="\/([^"/?#]+)"/i);
  const normalizedHref = hrefMatch ? decodeHtmlEntities(hrefMatch[1]).trim() : "";
  if (normalizedHref && !/sponsors/i.test(normalizedHref)) {
    return { name: normalizedHref.replace(/^@/, ""), isPrivate: false };
  }

  return { name: "Private", isPrivate: true };
}

function parseSupportersFromHtml(html: string): ThankYouSupporter[] {
  const sponsorsSectionMatch = html.match(/<div class="tmp-mt-3 tmp-pb-4" id="sponsors">([\s\S]*?)<\/remote-pagination>/i);
  if (!sponsorsSectionMatch) {
    return [];
  }

  const listHtml = sponsorsSectionMatch[1];
  const entryMatches = listHtml.match(/<div class="d-flex mb-1 mr-1"[^>]*>[\s\S]*?<\/div>/gi) ?? [];
  const supporters: ThankYouSupporter[] = [];
  const seenKeys = new Set<string>();

  for (const entryHtml of entryMatches) {
    const { name, isPrivate } = parseSupporterName(entryHtml);
    const hrefMatch = entryHtml.match(/<a[^>]+href="([^"]+)"/i);
    const profileUrl = isPrivate ? undefined : normalizeUrl(hrefMatch?.[1]);
    const avatarMatch = entryHtml.match(/<img[^>]+src="([^"]+)"/i);
    const avatarUrl = normalizeUrl(avatarMatch?.[1]);
    const dedupeKey = `${name}|${profileUrl ?? ""}|${avatarUrl ?? ""}`;
    if (seenKeys.has(dedupeKey)) continue;
    seenKeys.add(dedupeKey);
    supporters.push({
      name: name || "Private",
      avatarUrl,
      profileUrl,
      isPrivate: isPrivate || !name,
    });
  }

  return supporters;
}

async function fetchThanksSupporters(): Promise<ThankYouSupporter[]> {
  const response = await fetchWithTimeout(
    THANKS_SUPPORTERS_URL,
    {
      headers: {
        ...THANKS_REQUEST_HEADERS,
        Accept: "text/html,application/xhtml+xml",
      },
    },
    THANKS_FETCH_TIMEOUT_MS,
    "GitHub sponsors request",
  );
  if (!response.ok) {
    throw new Error(`GitHub sponsors page request failed (${response.status})`);
  }

  const html = await withTimeout(response.text(), THANKS_FETCH_TIMEOUT_MS, "GitHub sponsors response");
  const supporters = parseSupportersFromHtml(html);
  return supporters;
}

async function fetchThanksData(): Promise<ThankYouDataResult> {
  const result: ThankYouDataResult = {
    contributors: [],
    supporters: [],
  };

  const [contributorsResult, supportersResult] = await Promise.allSettled([
    fetchThanksContributors(),
    fetchThanksSupporters(),
  ]);

  if (contributorsResult.status === "fulfilled") {
    result.contributors = contributorsResult.value;
  } else {
    result.contributorsError = contributorsResult.reason instanceof Error
      ? contributorsResult.reason.message
      : "Unable to load contributors right now.";
  }

  if (supportersResult.status === "fulfilled") {
    result.supporters = supportersResult.value;
    if (result.supporters.length === 0) {
      result.supportersError = "No public supporters were found on GitHub Sponsors.";
    }
  } else {
    result.supportersError = supportersResult.reason instanceof Error
      ? supportersResult.reason.message
      : "Unable to load supporters right now.";
  }

  return result;
}


app.whenReady().then(async () => {
  // Initialize log capture first to capture all console output
  initLogCapture("main");

  await cacheManager.initialize();

  authService = new AuthService(join(app.getPath("userData"), "auth-state.json"));
  await authService.initialize();

  settingsManager = getSettingsManager();
  appUpdater = createAppUpdaterController({
    onStateChanged: emitUpdaterStateToRenderer,
    automaticChecksEnabled: settingsManager.get("autoCheckForUpdates"),
    onBeforeQuitAndInstall: () => {
      isUpdaterInstallQuitInProgress = true;
      clearExplicitShutdownFallback();
    },
    onQuitAndInstallError: () => {
      isUpdaterInstallQuitInProgress = false;
    },
  });

  // Connect and start Discord Rich Presence monitor if the user has opted in
  if (settingsManager.get("discordRichPresence")) {
    void connectDiscordRpc().then(() => discordMonitor.start());
  }

  // Set up permission handlers for getUserMedia, fullscreen, pointer lock
  session.defaultSession.setPermissionRequestHandler((webContents, permission, callback) => {
    const allowedPermissions = new Set([
      "media",
      "microphone",
      "fullscreen",
      "automatic-fullscreen",
      "pointerLock",
      "keyboardLock",
      "speaker-selection",
    ]);

    if (allowedPermissions.has(permission)) {
      callback(true);
      return;
    }

    callback(false);
  });

  session.defaultSession.setPermissionCheckHandler((webContents, permission, requestingOrigin) => {
    const allowedPermissions = new Set([
      "media",
      "microphone",
      "fullscreen",
      "automatic-fullscreen",
      "pointerLock",
      "keyboardLock",
      "speaker-selection",
    ]);

    return allowedPermissions.has(permission);
  });

  const mainIpcDeps: MainIpcDeps = {
    ipcMain,
    app,
    dialog,
    shell,
    systemPreferences,
    getMainWindow: () => mainWindow,
    authService,
    settingsManager,
    appUpdater,
    signaling: signalingState,
    GfnSignalingClient,
    emitToRenderer,
    emitUpdaterStateToRenderer,
    requestAppShutdown,
    discordMonitor,
    refreshScheduler,
    cacheManager,
    resolveJwt,
    rethrowSerializedSessionError,
    showSessionConflictDialog,
    shouldForceNewSession,
    selectReadySessionToClaim,
    selectLaunchingSession,
    stopActiveSessionsForCreate,
    SCREENSHOT_LIMIT,
    RECORDING_LIMIT,
    activeRecordings,
    saveScreenshot,
    listScreenshots,
    deleteScreenshot,
    saveScreenshotAs,
    dataUrlToBuffer,
    sanitizeTitleForFileName,
    ensureRecordingsDirectory,
    getRecordingsDirectory,
    listRecordings,
    ensureThumbnailForMedia,
    extFromMimeType,
    fetchThanksData,
    net,
    exportLogs: (format?: "text" | "json") => Promise.resolve(exportLogs(format)),
  };
  registerMainIpcHandlers(mainIpcDeps);

  refreshScheduler.initialize(
    fetchMainGamesUncached,
    fetchLibraryGamesUncached,
    fetchPublicGamesUncached,
  );

  cacheEventBus.on("cache:refresh-start", () => {
    if (mainWindow && !mainWindow.isDestroyed()) {
      mainWindow.webContents.send(IPC_CHANNELS.CACHE_STATUS_UPDATE, { event: "refresh-start" });
    }
  });

  cacheEventBus.on("cache:refresh-success", () => {
    if (mainWindow && !mainWindow.isDestroyed()) {
      mainWindow.webContents.send(IPC_CHANNELS.CACHE_STATUS_UPDATE, { event: "refresh-success" });
    }
  });

  cacheEventBus.on("cache:refresh-error", (details: { key: string; error: string }) => {
    if (mainWindow && !mainWindow.isDestroyed()) {
      mainWindow.webContents.send(IPC_CHANNELS.CACHE_STATUS_UPDATE, { event: "refresh-error", ...details });
    }
  });

  refreshScheduler.start();

  await createMainWindow();
  appUpdater.initialize();

  app.on("activate", async () => {
    if (isShutdownRequested) {
      return;
    }
    if (BrowserWindow.getAllWindows().length === 0) {
      await createMainWindow();
    }
  });
});

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") {
    requestAppShutdown({ reason: "window-all-closed" });
  }
});

app.on("before-quit", () => {
  isShutdownRequested = true;
  runShutdownCleanup(isUpdaterInstallQuitInProgress ? "before-quit-updater-install" : "before-quit");
});

app.on("will-quit", () => {
  clearExplicitShutdownFallback();
});

app.on("quit", () => {
  clearExplicitShutdownFallback();
});

// Export for use by other modules
export { showSessionConflictDialog, isSessionConflictError };
