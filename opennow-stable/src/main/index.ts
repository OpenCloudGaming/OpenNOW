import {
  app,
  BrowserWindow,
  clipboard,
  ipcMain,
  dialog,
  shell,
  systemPreferences,
  session,
  protocol,
} from "electron";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { existsSync, readFileSync } from "node:fs";

// Keyboard shortcuts reference (matching Rust implementation):
// Screenshot keybind - configurable, handled in renderer
// F3  - Toggle stats overlay (handled in renderer)
// Ctrl+Shift+Q - Stop streaming (handled in renderer)
// F8  - Toggle mouse/pointer lock (handled in main process via IPC)

import { IPC_CHANNELS } from "@shared/ipc";
import { registerOpenNowMediaProtocol } from "./mediaPaths";
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
  AppUpdaterState,
  SessionConflictChoice,
  Settings,
  PingResult,
  StreamRegion,
  VideoAccelerationPreference,
  MicrophonePermissionResult,
  ThankYouContributor,
  ThankYouDataResult,
  ThankYouSupporter,
} from "@shared/gfn";

import { getSettingsManager, type SettingsManager } from "./settings";

import { getActiveSessions } from "./gfn/cloudmatch";
import { AuthService } from "./gfn/auth";
import {
  connectDiscordRpc,
  setActivity,
  clearActivity,
  destroyDiscordRpc,
  getCurrentActivity,
  isDiscordRpcConnected,
} from "./discordRpc";
import {
  createAppUpdaterController,
  type AppUpdaterController,
} from "./updater";
import { getAppBuildInfo } from "./appBuildInfo";
import { registerAccountCatalogIpcHandlers } from "./ipc/accountCatalogHandlers";
import { registerMediaIpcHandlers } from "./ipc/mediaHandlers";
import { registerSessionIpcHandlers } from "./ipc/sessionHandlers";
import {
  registerSignalingIpcHandlers,
  type SignalingCoordinator,
} from "./signaling/signalingCoordinator";
import {
  isSessionConflictError,
  showSessionConflictDialog as showSessionConflictDialogWithDeps,
} from "./session/sessionConflict";
import { fetchWithTimeout, withTimeout } from "./services/requestTimeout";
import {
  fetchPrintedWasteQueue,
  fetchPrintedWasteServerMapping,
} from "./services/printedWaste";
import { pingRegions } from "./services/regionPing";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

// Configure Chromium video and WebRTC behavior before app.whenReady().

interface BootstrapVideoPreferences {
  decoderPreference: VideoAccelerationPreference;
  encoderPreference: VideoAccelerationPreference;
}

function isAccelerationPreference(
  value: unknown,
): value is VideoAccelerationPreference {
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
    const parsed = JSON.parse(
      readFileSync(settingsPath, "utf-8"),
    ) as Partial<BootstrapVideoPreferences>;
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
const isLinuxArm =
  process.platform === "linux" &&
  (process.arch === "arm64" || process.arch === "arm");

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

app.commandLine.appendSwitch(
  "enable-features",
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

app.commandLine.appendSwitch(
  "force-fieldtrials",
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

// file:// in &lt;video&gt; is blocked by Chromium for renderer pages; use a privileged custom scheme.
protocol.registerSchemesAsPrivileged([
  {
    scheme: "opennow-media",
    privileges: {
      standard: true,
      secure: true,
      supportFetchAPI: true,
      stream: true,
      corsEnabled: true,
    },
  },
]);

let mainWindow: BrowserWindow | null = null;
let rendererControlledFullscreen = false;
let signalingCoordinator: SignalingCoordinator | null = null;
let authService: AuthService;
let settingsManager: SettingsManager;
let appUpdater: AppUpdaterController | null = null;
const EXPLICIT_SHUTDOWN_FORCE_EXIT_DELAY_MS = 2000;
let isShutdownRequested = false;
let isShutdownCleanupComplete = false;
let isUpdaterInstallQuitInProgress = false;
let explicitShutdownFallbackTimer: NodeJS.Timeout | null = null;

// Runtime pointer-lock state (updated by renderer)
let isPointerLockActiveRuntime = false;

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
  // Parity with soft-reset behavior: on full app quit, let process teardown close
  // signaling sockets naturally instead of emitting an explicit disconnect event
  // into the renderer during shutdown.
  const shouldSkipExplicitSignalingDisconnect =
    reason === "renderer-explicit-exit" ||
    reason === "app-quit" ||
    reason === "before-quit" ||
    reason === "window-all-closed";
  signalingCoordinator?.disconnectForShutdown({
    emitDisconnectEvent: !shouldSkipExplicitSignalingDisconnect,
    reason,
  });
  signalingCoordinator = null;
  void destroyDiscordRpc();
  appUpdater?.dispose();
  appUpdater = null;

  const windowToClose = mainWindow;
  if (windowToClose && !windowToClose.isDestroyed()) {
    mainWindow = null;
    try {
      windowToClose.close();
    } catch (error) {
      console.warn(
        "[Main] Failed to close main window during shutdown:",
        error,
      );
    }

    if (!windowToClose.isDestroyed()) {
      try {
        windowToClose.destroy();
      } catch (error) {
        console.warn(
          "[Main] Failed to destroy main window during shutdown:",
          error,
        );
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
    console.warn(
      `[Main] Explicit shutdown fallback triggered (${reason}); forcing process exit.`,
    );
    app.exit(exitCode);
  }, EXPLICIT_SHUTDOWN_FORCE_EXIT_DELAY_MS);
  explicitShutdownFallbackTimer.unref?.();
}

function requestAppShutdown(
  options: {
    reason?: string;
    forceExitFallback?: boolean;
    exitCode?: number;
  } = {},
): void {
  const {
    reason = "app-quit",
    forceExitFallback = false,
    exitCode = 0,
  } = options;

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
      const activeSessions = await getActiveSessions(
        token,
        streamingBaseUrl,
      ).catch(() => []);

      const activeSession = activeSessions.find((s) =>
        [1, 2, 3].includes(s.status),
      );
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

function emitUpdaterStateToRenderer(state: AppUpdaterState): void {
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send(IPC_CHANNELS.APP_UPDATER_STATE_CHANGED, state);
  }
}

async function createMainWindow(): Promise<void> {
  const preloadMjsPath = join(__dirname, "../preload/index.mjs");
  const preloadJsPath = join(__dirname, "../preload/index.js");
  const preloadPath = existsSync(preloadMjsPath)
    ? preloadMjsPath
    : preloadJsPath;

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
      if (
        mainWindow &&
        !mainWindow.isDestroyed() &&
        !mainWindow.isFullScreen()
      ) {
        mainWindow.setFullScreen(true);
      }
    });

    mainWindow.webContents.on("leave-html-full-screen", () => {
      if (rendererControlledFullscreen) {
        return;
      }
      if (
        mainWindow &&
        !mainWindow.isDestroyed() &&
        mainWindow.isFullScreen()
      ) {
        mainWindow.setFullScreen(false);
      }
    });
  }

  // Track pointer-lock state from renderer; used to decide whether to swallow
  // Escape at the native level (before Chromium handles it).
  ipcMain.on(IPC_CHANNELS.POINTER_LOCK_CHANGE, (_ev, active: boolean) => {
    isPointerLockActiveRuntime = Boolean(active);
  });

  // Intercept Escape early to avoid Chromium exiting fullscreen before the
  // renderer can forward the key to the remote session. This is a best-effort
  // interception and is gated by the user's `allowEscapeToExitFullscreen` setting.
  mainWindow.webContents.on("before-input-event", (event, input) => {
    try {
      if (
        input.type === "keyDown" &&
        input.key === "Escape" &&
        isPointerLockActiveRuntime &&
        settingsManager &&
        !settingsManager.get("allowEscapeToExitFullscreen")
      ) {
        event.preventDefault();
        if (mainWindow && mainWindow.webContents) {
          mainWindow.webContents.send(IPC_CHANNELS.EXTERNAL_ESCAPE);
        }
      }
    } catch {
      // ignore errors - interception is best-effort
    }
  });

  if (process.env.ELECTRON_RENDERER_URL) {
    await mainWindow.loadURL(process.env.ELECTRON_RENDERER_URL);
  } else {
    await mainWindow.loadFile(join(__dirname, "../../dist/index.html"));
  }

  mainWindow.on("closed", () => {
    mainWindow = null;
    rendererControlledFullscreen = false;
  });
}

async function resolveJwt(token?: string): Promise<string> {
  return authService.resolveJwtToken(token);
}

async function showSessionConflictDialog(): Promise<SessionConflictChoice> {
  return showSessionConflictDialogWithDeps({
    dialog,
    getMainWindow: () => mainWindow,
  });
}

const THANKS_CONTRIBUTORS_URL =
  "https://api.github.com/repos/OpenCloudGaming/OpenNOW/contributors?per_page=100";
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
  return decodeHtmlEntities(value.replace(/<[^>]+>/g, " "))
    .replace(/\s+/g, " ")
    .trim();
}

function normalizeUrl(value: string | undefined): string | undefined {
  if (!value) return undefined;
  const decoded = decodeHtmlEntities(value.trim());
  if (!decoded) return undefined;
  if (decoded.startsWith("//")) return `https:${decoded}`;
  if (decoded.startsWith("/")) return `https://github.com${decoded}`;
  return decoded;
}

function shouldExcludeContributor(
  contributor: GitHubContributorResponse,
): boolean {
  const login = contributor.login?.trim() ?? "";
  const name = contributor.name?.trim() ?? "";
  if (!login || !contributor.avatar_url || !contributor.html_url) return true;
  if (contributor.type === "Bot") return true;
  if (/\[bot\]$/i.test(login)) return true;
  if (THANKS_EXCLUDED_PATTERN.test(login) || THANKS_EXCLUDED_PATTERN.test(name))
    return true;
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

  const payload = (await withTimeout(
    response.json() as Promise<GitHubContributorResponse[]>,
    THANKS_FETCH_TIMEOUT_MS,
    "GitHub contributors response",
  )) as GitHubContributorResponse[];
  if (!Array.isArray(payload)) {
    throw new Error("GitHub contributors response was not an array");
  }

  const contributors = payload
    .filter((contributor) => !shouldExcludeContributor(contributor))
    .map((contributor) => ({
      login: contributor.login!.trim(),
      avatarUrl: contributor.avatar_url!,
      profileUrl: contributor.html_url!,
      contributions:
        typeof contributor.contributions === "number"
          ? contributor.contributions
          : 0,
    }))
    .sort(
      (a, b) =>
        b.contributions - a.contributions || a.login.localeCompare(b.login),
    );
  return contributors;
}

function parseSupporterName(entryHtml: string): {
  name: string;
  isPrivate: boolean;
} {
  const privateHrefMatch = entryHtml.match(
    /href="https:\/\/docs\.github\.com\/sponsors\/sponsoring-open-source-contributors\/managing-your-sponsorship#managing-the-privacy-setting-for-your-sponsorship"/i,
  );
  const privateTooltipMatch = entryHtml.match(
    /<tool-tip[^>]*>\s*Private Sponsor\s*<\/tool-tip>/i,
  );
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
  const normalizedHref = hrefMatch
    ? decodeHtmlEntities(hrefMatch[1]).trim()
    : "";
  if (normalizedHref && !/sponsors/i.test(normalizedHref)) {
    return { name: normalizedHref.replace(/^@/, ""), isPrivate: false };
  }

  return { name: "Private", isPrivate: true };
}

function parseSupportersFromHtml(html: string): ThankYouSupporter[] {
  const sponsorsSectionMatch = html.match(
    /<div class="tmp-mt-3 tmp-pb-4" id="sponsors">([\s\S]*?)<\/remote-pagination>/i,
  );
  if (!sponsorsSectionMatch) {
    return [];
  }

  const listHtml = sponsorsSectionMatch[1];
  const entryMatches =
    listHtml.match(/<div class="d-flex mb-1 mr-1"[^>]*>[\s\S]*?<\/div>/gi) ??
    [];
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

  const html = await withTimeout(
    response.text(),
    THANKS_FETCH_TIMEOUT_MS,
    "GitHub sponsors response",
  );
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
    result.contributorsError =
      contributorsResult.reason instanceof Error
        ? contributorsResult.reason.message
        : "Unable to load contributors right now.";
  }

  if (supportersResult.status === "fulfilled") {
    result.supporters = supportersResult.value;
    if (result.supporters.length === 0) {
      result.supportersError =
        "No public supporters were found on GitHub Sponsors.";
    }
  } else {
    result.supportersError =
      supportersResult.reason instanceof Error
        ? supportersResult.reason.message
        : "Unable to load supporters right now.";
  }

  return result;
}

function registerIpcHandlers(): void {
  registerAccountCatalogIpcHandlers({
    ipcMain,
    authService,
    resolveJwt,
    refreshScheduler,
  });

  registerSessionIpcHandlers({
    ipcMain,
    dialog,
    authService,
    settingsManager,
    resolveJwt,
    setActivity,
    clearActivity,
    getMainWindow: () => mainWindow,
  });

  signalingCoordinator = registerSignalingIpcHandlers({
    ipcMain,
    mainDir: __dirname,
    settingsManager,
    getMainWindow: () => mainWindow,
  });

  ipcMain.handle(IPC_CHANNELS.DISCORD_CLEAR_ACTIVITY, async () => {
    void clearActivity();
  });

  // Toggle fullscreen via IPC (for completeness)
  ipcMain.handle(IPC_CHANNELS.TOGGLE_FULLSCREEN, async () => {
    if (mainWindow && !mainWindow.isDestroyed()) {
      const isFullScreen = mainWindow.isFullScreen();
      const nextFullscreen = !isFullScreen;
      mainWindow.setFullScreen(nextFullscreen);
      rendererControlledFullscreen = nextFullscreen;
    }
  });

  ipcMain.handle(
    IPC_CHANNELS.SET_FULLSCREEN,
    async (_event, value: boolean) => {
      if (mainWindow && !mainWindow.isDestroyed()) {
        try {
          const nextFullscreen = Boolean(value);
          mainWindow.setFullScreen(nextFullscreen);
          rendererControlledFullscreen = nextFullscreen;
        } catch (err) {
          console.warn("Failed to set fullscreen:", err);
        }
      }
    },
  );

  // Toggle pointer lock via IPC (F8 shortcut)
  ipcMain.handle(IPC_CHANNELS.TOGGLE_POINTER_LOCK, async () => {
    if (mainWindow && !mainWindow.isDestroyed()) {
      mainWindow.webContents.send("app:toggle-pointer-lock");
    }
  });

  ipcMain.handle(IPC_CHANNELS.QUIT_APP, async () => {
    requestAppShutdown({
      reason: "renderer-explicit-exit",
      forceExitFallback: true,
    });
  });

  ipcMain.handle(IPC_CHANNELS.OPEN_EXTERNAL_URL, async (_event, url: string): Promise<void> => {
    const parsed = new URL(url);
    if (parsed.protocol !== "https:" && parsed.protocol !== "http:") {
      throw new Error("Only HTTP(S) external URLs can be opened.");
    }
    await shell.openExternal(parsed.toString());
  });

  ipcMain.handle(
    IPC_CHANNELS.APP_UPDATER_GET_STATE,
    async (): Promise<AppUpdaterState> => {
      const buildInfo = getAppBuildInfo();
      return (
        appUpdater?.getState() ?? {
          status: "disabled",
          currentVersion: buildInfo.version,
          currentDisplayVersion: buildInfo.displayVersion,
          currentBuildNumber: buildInfo.buildNumber,
          updateSource: "github-releases",
          canCheck: false,
          canDownload: false,
          canInstall: false,
          isPackaged: app.isPackaged,
          message: "Updater is unavailable.",
        }
      );
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.APP_UPDATER_CHECK,
    async (): Promise<AppUpdaterState> => {
      const buildInfo = getAppBuildInfo();
      return (
        appUpdater?.checkForUpdates("manual") ?? {
          status: "disabled",
          currentVersion: buildInfo.version,
          currentDisplayVersion: buildInfo.displayVersion,
          currentBuildNumber: buildInfo.buildNumber,
          updateSource: "github-releases",
          canCheck: false,
          canDownload: false,
          canInstall: false,
          isPackaged: app.isPackaged,
          message: "Updater is unavailable.",
        }
      );
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.APP_UPDATER_DOWNLOAD,
    async (): Promise<AppUpdaterState> => {
      const buildInfo = getAppBuildInfo();
      return (
        appUpdater?.downloadUpdate() ?? {
          status: "disabled",
          currentVersion: buildInfo.version,
          currentDisplayVersion: buildInfo.displayVersion,
          currentBuildNumber: buildInfo.buildNumber,
          updateSource: "github-releases",
          canCheck: false,
          canDownload: false,
          canInstall: false,
          isPackaged: app.isPackaged,
          message: "Updater is unavailable.",
        }
      );
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.APP_UPDATER_INSTALL,
    async (): Promise<AppUpdaterState> => {
      const buildInfo = getAppBuildInfo();
      return (
        appUpdater?.quitAndInstall() ?? {
          status: "disabled",
          currentVersion: buildInfo.version,
          currentDisplayVersion: buildInfo.displayVersion,
          currentBuildNumber: buildInfo.buildNumber,
          updateSource: "github-releases",
          canCheck: false,
          canDownload: false,
          canInstall: false,
          isPackaged: app.isPackaged,
          message: "Updater is unavailable.",
        }
      );
    },
  );

  // Settings IPC handlers
  ipcMain.handle(IPC_CHANNELS.SETTINGS_GET, async (): Promise<Settings> => {
    return settingsManager.getAll();
  });

  ipcMain.handle(IPC_CHANNELS.CLIPBOARD_READ_TEXT, async (): Promise<string> => {
    return clipboard.readText();
  });

  ipcMain.handle(
    IPC_CHANNELS.SETTINGS_SET,
    async <K extends keyof Settings>(
      _event: Electron.IpcMainInvokeEvent,
      key: K,
      value: Settings[K],
    ) => {
      settingsManager.set(key, value);
      const appliedValue = settingsManager.get(key);
      // React to certain setting changes immediately in main process
      try {
        if (key === "autoCheckForUpdates") {
          appUpdater?.setAutomaticChecksEnabled(appliedValue as boolean);
        }
        signalingCoordinator?.applySettingsChange(key, appliedValue);
        if (key === "discordRichPresence") {
          if (appliedValue) {
            void connectDiscordRpc().then(() => discordMonitor.start());
          } else {
            discordMonitor.stop();
            void destroyDiscordRpc();
          }
        }
      } catch (err) {
        console.warn("Failed to apply setting change in main process:", err);
      }
    },
  );

  ipcMain.handle(IPC_CHANNELS.SETTINGS_RESET, async (): Promise<Settings> => {
    const resetSettings = settingsManager.reset();
    appUpdater?.setAutomaticChecksEnabled(resetSettings.autoCheckForUpdates);
    signalingCoordinator?.stopNativeStreamer("settings reset");
    signalingCoordinator?.resetNativeStreamerContext();
    return resetSettings;
  });

  ipcMain.handle(
    IPC_CHANNELS.SETTINGS_SELECT_NATIVE_STREAMER_EXECUTABLE,
    async (): Promise<string | null> => {
      const filters =
        process.platform === "win32"
          ? [
              { name: "Executable", extensions: ["exe"] },
              { name: "All Files", extensions: ["*"] },
            ]
          : [{ name: "All Files", extensions: ["*"] }];

      const options: Electron.OpenDialogOptions = {
        title: "Select OpenNOW streamer executable",
        properties: ["openFile"],
        filters,
      };
      const result =
        mainWindow && !mainWindow.isDestroyed()
          ? await dialog.showOpenDialog(mainWindow, options)
          : await dialog.showOpenDialog(options);

      if (result.canceled || result.filePaths.length === 0) {
        return null;
      }
      return result.filePaths[0] ?? null;
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.MICROPHONE_PERMISSION_GET,
    async (): Promise<MicrophonePermissionResult> => {
      if (process.platform !== "darwin") {
        return {
          platform: process.platform,
          isMacOs: false,
          status: "not-applicable",
          granted: false,
          canRequest: false,
          shouldUseBrowserApi: true,
        };
      }

      const currentStatus =
        systemPreferences.getMediaAccessStatus("microphone");
      console.log("[Main] macOS microphone permission status:", currentStatus);

      if (currentStatus === "granted") {
        return {
          platform: process.platform,
          isMacOs: true,
          status: "granted",
          granted: true,
          canRequest: false,
          shouldUseBrowserApi: true,
        };
      }

      if (currentStatus === "not-determined") {
        const granted = await systemPreferences.askForMediaAccess("microphone");
        const nextStatus = systemPreferences.getMediaAccessStatus("microphone");
        console.log(
          "[Main] Requested macOS microphone permission:",
          granted,
          nextStatus,
        );
        return {
          platform: process.platform,
          isMacOs: true,
          status: nextStatus,
          granted,
          canRequest: nextStatus === "not-determined",
          shouldUseBrowserApi: granted,
        };
      }

      return {
        platform: process.platform,
        isMacOs: true,
        status: currentStatus,
        granted: false,
        canRequest: false,
        shouldUseBrowserApi: false,
      };
    },
  );

  // Logs export IPC handler
  ipcMain.handle(
    IPC_CHANNELS.LOGS_EXPORT,
    async (_event, format: "text" | "json" = "text"): Promise<string> => {
      return exportLogs(format);
    },
  );

  registerMediaIpcHandlers({
    ipcMain,
    dialog,
    shell,
    getMainWindow: () => mainWindow,
  });

  ipcMain.handle(IPC_CHANNELS.CACHE_REFRESH_MANUAL, async (): Promise<void> => {
    await refreshScheduler.manualRefresh();
  });

  ipcMain.handle(IPC_CHANNELS.CACHE_DELETE_ALL, async (): Promise<void> => {
    await cacheManager.deleteAll();
    console.log("[IPC] Cache deletion completed successfully");
  });

  ipcMain.handle(
    IPC_CHANNELS.COMMUNITY_GET_THANKS,
    async (): Promise<ThankYouDataResult> => {
      return fetchThanksData();
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.PING_REGIONS,
    async (_event, regions: StreamRegion[]): Promise<PingResult[]> => {
      return pingRegions(regions);
    },
  );

  // PrintedWaste queue API — fetched from main process so User-Agent can be set
  ipcMain.handle(IPC_CHANNELS.PRINTEDWASTE_QUEUE_FETCH, async () => {
    return fetchPrintedWasteQueue(app.getVersion());
  });

  ipcMain.handle(IPC_CHANNELS.PRINTEDWASTE_SERVER_MAPPING_FETCH, async () => {
    return fetchPrintedWasteServerMapping(app.getVersion());
  });

  // Save window size when it changes
  mainWindow?.on("resize", () => {
    if (mainWindow && !mainWindow.isDestroyed()) {
      const [width, height] = mainWindow.getSize();
      settingsManager.set("windowWidth", width);
      settingsManager.set("windowHeight", height);
    }
  });
}

app.whenReady().then(async () => {
  // Initialize log capture first to capture all console output
  initLogCapture("main");

  await cacheManager.initialize();

  authService = new AuthService(
    join(app.getPath("userData"), "auth-state.json"),
  );
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
  session.defaultSession.setPermissionRequestHandler(
    (webContents, permission, callback) => {
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
    },
  );

  session.defaultSession.setPermissionCheckHandler(
    (_webContents, permission, _requestingOrigin) => {
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
    },
  );

  registerOpenNowMediaProtocol();
  registerIpcHandlers();

  refreshScheduler.initialize(
    fetchMainGamesUncached,
    fetchLibraryGamesUncached,
    fetchPublicGamesUncached,
  );

  cacheEventBus.on("cache:refresh-start", () => {
    if (mainWindow && !mainWindow.isDestroyed()) {
      mainWindow.webContents.send(IPC_CHANNELS.CACHE_STATUS_UPDATE, {
        event: "refresh-start",
      });
    }
  });

  cacheEventBus.on("cache:refresh-success", () => {
    if (mainWindow && !mainWindow.isDestroyed()) {
      mainWindow.webContents.send(IPC_CHANNELS.CACHE_STATUS_UPDATE, {
        event: "refresh-success",
      });
    }
  });

  cacheEventBus.on(
    "cache:refresh-error",
    (details: { key: string; error: string }) => {
      if (mainWindow && !mainWindow.isDestroyed()) {
        mainWindow.webContents.send(IPC_CHANNELS.CACHE_STATUS_UPDATE, {
          event: "refresh-error",
          ...details,
        });
      }
    },
  );

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
  requestAppShutdown({ reason: "window-all-closed" });
});

app.on("before-quit", () => {
  isShutdownRequested = true;
  runShutdownCleanup(
    isUpdaterInstallQuitInProgress
      ? "before-quit-updater-install"
      : "before-quit",
  );
});

app.on("will-quit", () => {
  clearExplicitShutdownFallback();
});

app.on("quit", () => {
  clearExplicitShutdownFallback();
});

// Export for use by other modules
export { showSessionConflictDialog, isSessionConflictError };
