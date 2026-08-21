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
import { cpus, totalmem } from "node:os";

// Keyboard shortcuts reference (matching Rust implementation):
// Screenshot keybind - configurable, handled in renderer
// Ctrl+N - Cycle stats overlay (handled in renderer)
// Ctrl+Shift+Q - Stop streaming (handled in renderer)
// F8  - Toggle mouse/pointer lock (handled in main process via IPC)

import { IPC_CHANNELS } from "@shared/ipc";
import { registerOpenNowMediaProtocol } from "./mediaPaths";
import { initLogCapture, setLogContext } from "@shared/logger";
import { cacheManager } from "./services/cacheManager";
import { refreshScheduler } from "./services/refreshScheduler";
import { cacheEventBus } from "./services/cacheEventBus";
import {
  fetchMainGamesUncached,
  fetchLibraryGamesUncached,
  fetchPublicGamesUncached,
} from "./platforms/gfn/games";
import type {
  AppUpdaterState,
  SessionConflictChoice,
  DirectLaunchRequest,
} from "@shared/gfn";

import { getSettingsManager, type SettingsManager } from "./settings";

import { getActiveSessions } from "./platforms/gfn/cloudmatch";
import { AuthService } from "./platforms/gfn/auth";
import { configureIdentifyAsSteamDeck } from "./platforms/gfn/deviceIdentity";
import { initSessionProxyAuth } from "./platforms/gfn/proxyFetch";
import {
  connectDiscordRpc,
  setActivity,
  clearActivity,
  destroyDiscordRpc,
  getCurrentActivity,
  isDiscordRpcConnected,
} from "./discordRpc";
import {
  discordMonitorActivityDecision,
} from "./discordPresence";
import {
  createAppUpdaterController,
  type AppUpdaterController,
} from "./updater";
import { registerAccountCatalogIpcHandlers } from "./ipc/accountCatalogHandlers";
import { registerConsolePinIpcHandlers } from "./ipc/consolePinHandlers";
import { createSafeStorageAdapter } from "./security/safeStorageAdapter";
import { registerCoreIpcHandlers } from "./ipc/coreHandlers";
import { registerSessionIpcHandlers } from "./ipc/sessionHandlers";
import {
  registerSignalingIpcHandlers,
  type SignalingCoordinator,
} from "./signaling/signalingCoordinator";
import {
  isSessionConflictError,
  showSessionConflictDialog as showSessionConflictDialogWithDeps,
} from "./session/sessionConflict";
import {
  buildChromiumCommandLine,
  normalizeBootstrapChromiumPreferences,
  type BootstrapChromiumPreferences,
} from "./chromiumCommandLine";
import { parseDirectLaunchArgs, type DirectLaunchArgs } from "@shared/directLaunch";
import { getReleaseHighlightsPayload, shouldShowReleaseHighlights } from "./releaseHighlights";
import { shutdownMainTelemetry, syncMainTelemetry } from "./telemetry/posthog";
import { createMainWindow } from "./window/mainWindow";
import { resolveAppInstanceProfile } from "./appInstance";
import {
  DiagnosticHistoryController,
  DiagnosticHistoryStore,
} from "./services/diagnosticHistory";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const appInstanceProfile = resolveAppInstanceProfile(
  process.argv,
  app.getPath("userData"),
);
if (appInstanceProfile.isSecondary) {
  app.setPath("userData", appInstanceProfile.userDataPath);
  app.setPath("sessionData", appInstanceProfile.userDataPath);
}

// Capture bootstrap decisions as well as app-ready/runtime output. Android's
// diagnostic export follows the same full-lifecycle model.
const mainLogCapture = initLogCapture("main");
const cpuInfo = cpus();
setLogContext("application.main", {
  version: app.getVersion(),
  packaged: app.isPackaged,
  platform: process.platform,
  architecture: process.arch,
  electron: process.versions.electron,
  chromium: process.versions.chrome,
  node: process.versions.node,
  cpuModel: cpuInfo[0]?.model ?? "unknown",
  processorCount: cpuInfo.length,
  memoryMiB: Math.round(totalmem() / (1024 * 1024)),
  secondaryInstance: appInstanceProfile.isSecondary,
});

// Configure Chromium video, WebRTC, and input behavior before app.whenReady().

function loadBootstrapChromiumPreferences(): BootstrapChromiumPreferences {
  try {
    const settingsPath = join(app.getPath("userData"), "settings.json");
    if (!existsSync(settingsPath)) {
      return normalizeBootstrapChromiumPreferences(null);
    }
    return normalizeBootstrapChromiumPreferences(
      JSON.parse(readFileSync(settingsPath, "utf-8")),
    );
  } catch {
    return normalizeBootstrapChromiumPreferences(null);
  }
}

const bootstrapChromiumPrefs = loadBootstrapChromiumPreferences();
console.log(
  `[Main] Video acceleration preference: decode=${bootstrapChromiumPrefs.decoderPreference}, encode=${bootstrapChromiumPrefs.encoderPreference}`,
);

const chromiumCommandLine = buildChromiumCommandLine(
  bootstrapChromiumPrefs,
  process.platform,
  process.arch,
);

app.commandLine.appendSwitch(
  "enable-features",
  chromiumCommandLine.enableFeatures.join(","),
);

app.commandLine.appendSwitch(
  "disable-features",
  chromiumCommandLine.disableFeatures.join(","),
);

for (const [name, value] of Object.entries(chromiumCommandLine.switches)) {
  if (value === true) {
    app.commandLine.appendSwitch(name);
  } else {
    app.commandLine.appendSwitch(name, value);
  }
}

// --- Responsiveness flags ---
// Keep default compositor frame pacing (vsync + frame cap) to avoid runaway
// CPU usage from uncapped UI animations.
// Prevent renderer throttling when the window is backgrounded or occluded.
app.commandLine.appendSwitch("disable-renderer-backgrounding");
app.commandLine.appendSwitch("disable-backgrounding-occluded-windows");
// Remove getUserMedia FPS cap (not strictly needed for receive-only but avoids potential limits)
app.commandLine.appendSwitch("max-gum-fps", "999");
/*
 * Catalog artwork is served with `Cache-Control: max-age=604800`, but a browsed
 * store plus library is a few thousand images. Chromium's default disk cache is
 * small enough that the catalog evicts itself, so every launch re-downloaded the
 * same art. 512 MB comfortably holds a fully browsed catalog at the sizes the
 * shell actually requests (see lib/consoleImageSizing.ts).
 */
app.commandLine.appendSwitch("disk-cache-size", String(512 * 1024 * 1024));
if (!app.isPackaged && process.env.OPENNOW_REMOTE_DEBUG === "1") {
  app.commandLine.appendSwitch(
    "remote-debugging-port",
    appInstanceProfile.isSecondary ? "9223" : "9222",
  );
}

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
let diagnosticHistoryController: DiagnosticHistoryController | null = null;
const EXPLICIT_SHUTDOWN_FORCE_EXIT_DELAY_MS = 2000;
let isShutdownRequested = false;
let isShutdownCleanupComplete = false;
let isUpdaterInstallQuitInProgress = false;
let explicitShutdownFallbackTimer: NodeJS.Timeout | null = null;
let directLaunchRequestSequence = 0;
let pendingDirectLaunchRequest: DirectLaunchRequest | null = createDirectLaunchRequestFromArgv(process.argv);

// Runtime pointer-lock state (updated by renderer)
let isPointerLockActiveRuntime = false;
let pointerLockEscapeCaptureUntilMs = 0;
let isStreamInputActiveRuntime = false;
let nativeRawInputOwnsEscapeRuntime = false;

function createDirectLaunchRequest(args: DirectLaunchArgs): DirectLaunchRequest {
  return {
    ...args,
    id: `cli-${process.pid}-${Date.now()}-${++directLaunchRequestSequence}`,
    source: "cli",
    receivedAt: Date.now(),
  };
}

function createDirectLaunchRequestFromArgv(argv: readonly string[]): DirectLaunchRequest | null {
  const args = parseDirectLaunchArgs(argv);
  return args ? createDirectLaunchRequest(args) : null;
}

function focusMainWindow(): void {
  if (!mainWindow || mainWindow.isDestroyed()) return;
  if (mainWindow.isMinimized()) {
    mainWindow.restore();
  }
  mainWindow.show();
  mainWindow.focus();
}

function emitDirectLaunchRequest(request: DirectLaunchRequest): void {
  if (!mainWindow || mainWindow.isDestroyed()) return;
  mainWindow.webContents.send(IPC_CHANNELS.DIRECT_LAUNCH_REQUEST, request);
}

function enqueueDirectLaunchRequest(request: DirectLaunchRequest): void {
  pendingDirectLaunchRequest = request;
  focusMainWindow();
  // Argument launches always run as a fullscreen console session.
  if (mainWindow && !mainWindow.isDestroyed() && !mainWindow.isFullScreen()) {
    mainWindow.setFullScreen(true);
  }
  emitDirectLaunchRequest(request);
}

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
  void shutdownMainTelemetry();
  appUpdater?.dispose();
  appUpdater = null;
  diagnosticHistoryController?.stop();
  void diagnosticHistoryController?.flush().catch((error) => {
    console.warn("[Diagnostics] Failed to flush shutdown diagnostic snapshot:", error);
  });

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
      const decision = discordMonitorActivityDecision(currentActivity, activeSession ?? null);

      if (decision.action === "set") {
        void setActivity({
          ...decision.activity,
          startTimestamp: decision.startTimestamp,
        });
      } else if (decision.action === "clear") {
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

function createMainWindowDeps() {
  return {
    mainDir: __dirname,
    windowTitle: appInstanceProfile.windowTitle,
    settingsManager,
    getMainWindow: () => mainWindow,
    setMainWindow: (window: BrowserWindow | null) => {
      mainWindow = window;
    },
    getRendererControlledFullscreen: () => rendererControlledFullscreen,
    setRendererControlledFullscreen: (value: boolean) => {
      rendererControlledFullscreen = value;
    },
    getPendingDirectLaunchRequest: () => pendingDirectLaunchRequest,
    emitDirectLaunchRequest,
    getPointerLockActive: () => isPointerLockActiveRuntime,
    setPointerLockActive: (active: boolean) => {
      isPointerLockActiveRuntime = active;
    },
    getPointerLockEscapeCaptureUntilMs: () => pointerLockEscapeCaptureUntilMs,
    setPointerLockEscapeCaptureUntilMs: (value: number) => {
      pointerLockEscapeCaptureUntilMs = value;
    },
    getStreamInputActive: () => isStreamInputActiveRuntime,
    setStreamInputActive: (active: boolean) => {
      isStreamInputActiveRuntime = active;
    },
    getNativeRawInputOwnsEscape: () => nativeRawInputOwnsEscapeRuntime,
    setNativeRawInputOwnsEscape: (ownsEscape: boolean) => {
      nativeRawInputOwnsEscapeRuntime = ownsEscape;
    },
    isAppShutdownRequested: () => isShutdownRequested,
  };
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

function registerIpcHandlers(): void {
  registerAccountCatalogIpcHandlers({
    ipcMain,
    authService,
    resolveJwt,
    refreshScheduler,
  });

  registerConsolePinIpcHandlers({
    getConsoleProfiles: () => authService.getConsoleProfiles(),
    isSavedAccount: (userId) => authService.getSavedAccounts().some((account) => account.userId === userId),
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

  registerCoreIpcHandlers({
    ipcMain,
    app,
    dialog,
    shell,
    clipboard,
    systemPreferences,
    settingsManager,
    refreshScheduler,
    getMainWindow: () => mainWindow,
    setRendererControlledFullscreen: (value) => {
      rendererControlledFullscreen = value;
    },
    getPendingDirectLaunchRequest: () => pendingDirectLaunchRequest,
    setPendingDirectLaunchRequest: (request) => {
      pendingDirectLaunchRequest = request;
    },
    getAppUpdater: () => appUpdater,
    getSignalingCoordinator: () => signalingCoordinator,
    discordMonitor,
    requestAppShutdown,
  });
}

const gotSingleInstanceLock = app.requestSingleInstanceLock();

if (!gotSingleInstanceLock) {
  app.quit();
} else {
  app.on("second-instance", (_event, argv) => {
    const request = createDirectLaunchRequestFromArgv(argv);
    if (request) {
      enqueueDirectLaunchRequest(request);
      return;
    }
    focusMainWindow();
  });
}

if (gotSingleInstanceLock) {
app.whenReady().then(async () => {
  diagnosticHistoryController = new DiagnosticHistoryController(
    new DiagnosticHistoryStore(app.getPath("userData")),
    mainLogCapture,
  );
  try {
    await diagnosticHistoryController.start();
  } catch (error) {
    console.warn("[Diagnostics] Failed to initialize previous-run history:", error);
    diagnosticHistoryController = null;
  }
  if (appInstanceProfile.isSecondary) {
    console.log(`[Main] Secondary instance profile: ${appInstanceProfile.userDataPath}`);
  }
  initSessionProxyAuth();

  await cacheManager.initialize();

  authService = new AuthService(
    join(app.getPath("userData"), "auth-state.json"),
    join(app.getPath("userData"), "console-profiles.json"),
    createSafeStorageAdapter(),
  );
  await authService.initialize();

  settingsManager = getSettingsManager();
  configureIdentifyAsSteamDeck(() => settingsManager.get("identifyAsSteamDeck"));
  appUpdater = createAppUpdaterController({
    onStateChanged: emitUpdaterStateToRenderer,
    automaticChecksEnabled: settingsManager.get("autoCheckForUpdates"),
    updateChannel: settingsManager.get("updateChannel"),
    disabledReason: appInstanceProfile.isSecondary
      ? "Updates are managed by the primary OpenNOW instance."
      : undefined,
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

  // Start anonymous error reporting only when the user has granted consent
  syncMainTelemetry(settingsManager);

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

  await createMainWindow(createMainWindowDeps());
  appUpdater.initialize();

  // Fire-and-forget: check if we should show release highlights after the window loads
  void (async () => {
    try {
      if (!app.isPackaged) return;
      const current = app.getVersion().replace(/^v/, "");
      const lastSeen = settingsManager.get("lastSeenReleaseHighlightsVersion") ?? "";
      if (!shouldShowReleaseHighlights(current, lastSeen)) return;

      // Fetch payload (may take up to 8s for GitHub, graceful fallback if offline)
      const payload = await getReleaseHighlightsPayload(current);

      // Wait for the renderer to finish loading before sending
      const win = mainWindow;
      if (!win || win.isDestroyed()) return;

      if (win.webContents.isLoading()) {
        await new Promise<void>((resolve) => win.webContents.once("did-finish-load", resolve));
      }

      if (!win.isDestroyed()) {
        win.webContents.send(IPC_CHANNELS.RELEASE_HIGHLIGHTS_SHOW, payload);
      }
    } catch (error) {
      console.warn("[ReleaseHighlights] Startup check failed:", error);
    }
  })();

  app.on("activate", async () => {
    if (isShutdownRequested) {
      return;
    }
    if (BrowserWindow.getAllWindows().length === 0) {
      await createMainWindow(createMainWindowDeps());
    }
  });
});
}

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
