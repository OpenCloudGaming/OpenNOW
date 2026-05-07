import {
  app,
  BrowserWindow,
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
import { Buffer } from "node:buffer";

// Keyboard shortcuts reference (matching Rust implementation):
// Screenshot keybind - configurable, handled in renderer
// F3  - Toggle stats overlay (handled in renderer)
// Ctrl+Shift+Q - Stop streaming (handled in renderer)
// F8  - Toggle mouse/pointer lock (handled in main process via IPC)

import { IPC_CHANNELS } from "@shared/ipc";
import { registerOpenNowMediaProtocol } from "./mediaPaths";
import { initLogCapture, exportLogs } from "@shared/logger";
import type { NativeStreamerInputPacket } from "@shared/nativeStreamer";
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
  StreamSettings,
  SignalingConnectRequest,
  SendAnswerRequest,
  IceCandidatePayload,
  NativeInputPacket,
  NativeRenderSurface,
  NativeRenderSurfaceUpdate,
  KeyframeRequest,
  Settings,
  NativeStreamerSessionContext,
  SubscriptionFetchRequest,
  SessionConflictChoice,
  PingResult,
  StreamRegion,
  VideoAccelerationPreference,
  MicrophonePermissionResult,
  NativeStreamerStatus,
  ThankYouContributor,
  ThankYouDataResult,
  ThankYouSupporter,
} from "@shared/gfn";
import { serializeSessionErrorTransport } from "@shared/sessionError";
import {
  normalizeCloudGsyncOverride,
  resolveCloudGsync,
} from "@shared/cloudGsync";
import {
  enrichErrorForIpc,
  formatErrorChainForLog,
} from "@shared/networkError";

import { getSettingsManager, type SettingsManager } from "./settings";

import {
  createSession,
  pollSession,
  reportSessionAd,
  stopSession,
  getActiveSessions,
  claimSession,
} from "./gfn/cloudmatch";
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
import { NativeStreamerManager } from "./nativeStreamer/manager";
import { getNativeCloudGsyncCapabilities } from "./nativeCloudGsync";
import { registerMediaIpcHandlers } from "./ipc/mediaHandlers";
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
let signalingClient: GfnSignalingClient | null = null;
let signalingClientKey: string | null = null;
let nativeStreamerManager: NativeStreamerManager | null = null;
let nativeStreamerContext: NativeStreamerSessionContext | null = null;
let nativeStreamerFallbackSessionId: string | null = null;
const MAX_NATIVE_INPUT_PACKET_BYTES = 4096;
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
  if (!shouldSkipExplicitSignalingDisconnect) {
    signalingClient?.disconnect();
  }
  signalingClient = null;
  signalingClientKey = null;
  nativeStreamerManager?.dispose(reason);
  nativeStreamerManager = null;
  nativeStreamerContext = null;
  nativeStreamerFallbackSessionId = null;
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

function emitToRenderer(event: MainToRendererSignalingEvent): void {
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send(IPC_CHANNELS.SIGNALING_EVENT, event);
  }
}

function getNativeStreamerManager(): NativeStreamerManager {
  nativeStreamerManager ??= new NativeStreamerManager({
    mainDir: __dirname,
    getBackendPreference: () => "gstreamer",
    getVideoBackendPreference: () =>
      settingsManager?.get("nativeVideoBackend") ?? "auto",
    getExecutablePathOverride: () =>
      settingsManager?.get("nativeStreamerExecutablePath") ?? "",
    getCloudGsyncMode: () =>
      settingsManager?.get("nativeCloudGsyncMode") ?? "auto",
    getD3dFullscreenMode: () =>
      settingsManager?.get("nativeD3dFullscreenMode") ?? "auto",
    getExternalRendererEnabled: () => true,
    emit: emitToRenderer,
    sendAnswer: async (payload) => {
      if (!signalingClient) {
        throw new Error("Signaling is not connected");
      }
      await signalingClient.sendAnswer(payload);
    },
    sendIceCandidate: async (candidate) => {
      if (!signalingClient) {
        throw new Error("Signaling is not connected");
      }
      await signalingClient.sendIceCandidate(candidate);
    },
    requestKeyframe: async (payload) => {
      if (!signalingClient) {
        throw new Error("Signaling is not connected");
      }
      await signalingClient.requestKeyframe(payload);
    },
  });
  return nativeStreamerManager;
}

function isNativeStreamerSelected(): boolean {
  return settingsManager?.get("streamClientMode") === "native";
}

function normalizeMaxBitrateMbps(value: unknown): number | null {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return null;
  }

  return Math.min(150, Math.max(5, Math.round(value)));
}

function updateNativeStreamerBitrateSetting(value: unknown): void {
  const maxBitrateMbps = normalizeMaxBitrateMbps(value);
  if (maxBitrateMbps === null) {
    return;
  }

  if (nativeStreamerContext) {
    nativeStreamerContext = {
      ...nativeStreamerContext,
      settings: {
        ...nativeStreamerContext.settings,
        maxBitrateMbps,
      },
    };
  }

  nativeStreamerManager?.updateBitrateLimit(maxBitrateMbps * 1000);
}

function nativeWindowHandleToHex(window: BrowserWindow): string | null {
  const handle = window.getNativeWindowHandle();
  if (handle.byteLength >= 8) {
    return `0x${handle.readBigUInt64LE(0).toString(16)}`;
  }
  if (handle.byteLength >= 4) {
    return `0x${handle.readUInt32LE(0).toString(16)}`;
  }
  return null;
}

function normalizeNativeRenderSurface(
  window: BrowserWindow,
  input: NativeRenderSurfaceUpdate,
): NativeRenderSurface | null {
  if (!input || typeof input !== "object") {
    return null;
  }

  const windowHandle = nativeWindowHandleToHex(window);
  if (!windowHandle) {
    return null;
  }

  const deviceScaleFactor = Number.isFinite(input.deviceScaleFactor)
    ? Math.min(8, Math.max(0.25, input.deviceScaleFactor))
    : 1;
  const rect = input.rect;
  const visible =
    input.visible === true &&
    rect !== null &&
    Number.isFinite(rect.x) &&
    Number.isFinite(rect.y) &&
    Number.isFinite(rect.width) &&
    Number.isFinite(rect.height) &&
    rect.width >= 2 &&
    rect.height >= 2;

  return {
    windowHandle,
    deviceScaleFactor,
    visible,
    showStats: input.showStats === true,
    rect: visible
      ? {
          x: Math.round(rect.x),
          y: Math.round(rect.y),
          width: Math.max(2, Math.round(rect.width)),
          height: Math.max(2, Math.round(rect.height)),
        }
      : null,
  };
}

function normalizeNativeInputPacket(
  input: unknown,
): NativeStreamerInputPacket | null {
  if (!input || typeof input !== "object") {
    return null;
  }

  const packet = input as { payload?: unknown; partiallyReliable?: unknown };
  const rawPayload = packet.payload;
  let bytes: Uint8Array;

  if (rawPayload instanceof ArrayBuffer) {
    bytes = new Uint8Array(rawPayload);
  } else if (ArrayBuffer.isView(rawPayload)) {
    bytes = new Uint8Array(
      rawPayload.buffer,
      rawPayload.byteOffset,
      rawPayload.byteLength,
    );
  } else if (Array.isArray(rawPayload)) {
    if (
      rawPayload.length === 0 ||
      rawPayload.length > MAX_NATIVE_INPUT_PACKET_BYTES ||
      rawPayload.some(
        (byte) => !Number.isInteger(byte) || byte < 0 || byte > 255,
      )
    ) {
      return null;
    }
    bytes = Uint8Array.from(rawPayload);
  } else {
    return null;
  }

  if (
    bytes.byteLength === 0 ||
    bytes.byteLength > MAX_NATIVE_INPUT_PACKET_BYTES
  ) {
    return null;
  }

  return {
    payloadBase64: Buffer.from(
      bytes.buffer,
      bytes.byteOffset,
      bytes.byteLength,
    ).toString("base64"),
    partiallyReliable: packet.partiallyReliable === true,
  };
}

function routeSignalingEvent(event: MainToRendererSignalingEvent): void {
  if (event.type === "disconnected") {
    void nativeStreamerManager?.stop(`signaling disconnected: ${event.reason}`);
    nativeStreamerContext = null;
    nativeStreamerFallbackSessionId = null;
    emitToRenderer(event);
    return;
  }

  const context = nativeStreamerContext;
  const nativeFallbackActive =
    context !== null &&
    nativeStreamerFallbackSessionId === context.session.sessionId;

  if (!isNativeStreamerSelected() || !context || nativeFallbackActive) {
    emitToRenderer(event);
    return;
  }

  if (event.type === "offer") {
    void handleNativeStreamerOffer(event.sdp, context);
    return;
  }

  if (event.type === "remote-ice") {
    void getNativeStreamerManager()
      .addRemoteIce(event.candidate, context)
      .catch((error) => {
        emitToRenderer({
          type: "error",
          message: `Native streamer ICE failed: ${String(error)}`,
        });
      });
    return;
  }

  emitToRenderer(event);
}

async function handleNativeStreamerOffer(
  sdp: string,
  context: NativeStreamerSessionContext,
): Promise<void> {
  try {
    await getNativeStreamerManager().handleOffer(sdp, context);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    console.warn("[NativeStreamer] Falling back to web streamer:", message);
    nativeStreamerFallbackSessionId = context.session.sessionId;
    const queuedRemoteIce =
      nativeStreamerManager?.drainQueuedRemoteIce(context.session.sessionId) ??
      [];
    await nativeStreamerManager
      ?.stop("native streamer fallback")
      .catch(() => undefined);
    emitToRenderer({
      type: "error",
      message: `Native streamer failed: ${message}. Falling back to web streamer.`,
    });
    emitToRenderer({ type: "offer", sdp });
    for (const candidate of queuedRemoteIce) {
      emitToRenderer({ type: "remote-ice", candidate });
    }
  }
}

async function resetNativeStreamerForSignalingReconnect(): Promise<void> {
  if (!nativeStreamerManager) {
    return;
  }

  if (
    !isNativeStreamerSelected() ||
    !nativeStreamerContext ||
    nativeStreamerManager.hasActiveSession()
  ) {
    await nativeStreamerManager.stop("signaling reconnect");
  }
}

async function prepareNativeStreamerBeforeSignaling(): Promise<void> {
  const context = nativeStreamerContext;
  if (!isNativeStreamerSelected() || !context) {
    return;
  }

  try {
    emitToRenderer({
      type: "log",
      message: "Preparing native streamer before signaling attach.",
    });
    await getNativeStreamerManager().prepareForSession(context);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    console.warn(
      "[NativeStreamer] Pre-attach startup failed; falling back to web streamer:",
      message,
    );
    nativeStreamerFallbackSessionId = context.session.sessionId;
    await nativeStreamerManager
      ?.stop("native streamer pre-attach fallback")
      .catch(() => undefined);
    emitToRenderer({
      type: "error",
      message: `Native streamer failed before signaling attach: ${message}. Falling back to web streamer.`,
    });
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
    } catch (err) {
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
  throw enrichErrorForIpc(error);
}

const AUTO_RESUME_SESSION_STATUSES = new Set([2, 3]);
const ACTIVE_CREATE_SESSION_STATUSES = new Set([1, 2, 3]);

function shouldForceNewSession(
  strategy: ExistingSessionStrategy | undefined,
): boolean {
  return strategy === "force-new";
}

function isAutoResumeReadySession(entry: ActiveSessionInfo): boolean {
  return (
    entry.serverIp != null && AUTO_RESUME_SESSION_STATUSES.has(entry.status)
  );
}

function isActiveCreateSessionConflict(entry: ActiveSessionInfo): boolean {
  return ACTIVE_CREATE_SESSION_STATUSES.has(entry.status);
}

async function resolveSessionCloudGsyncSettings(
  settings: StreamSettings,
): Promise<StreamSettings> {
  const userRequested = settings.enableCloudGsync;
  const clientMode = settings.clientMode ?? "web";
  const cloudGsyncMode = settings.nativeCloudGsyncMode ?? "auto";
  const capabilities =
    clientMode === "native"
      ? await getNativeCloudGsyncCapabilities(cloudGsyncMode)
      : undefined;
  const resolution = resolveCloudGsync({
    userRequested,
    fps: settings.fps,
    clientMode,
    nativeBackendAvailable: clientMode === "native",
    capabilities,
    override: normalizeCloudGsyncOverride(cloudGsyncMode),
  });

  console.log(
    `[CloudGsync] requested=${resolution.requested} resolved=${resolution.enabled} reflex=${resolution.reflexEnabled} reason=${resolution.reason} clientMode=${clientMode} fps=${settings.fps} capabilities=${JSON.stringify(resolution.capabilities)}`,
  );

  if (resolution.enabled) {
    console.log(
      "[CloudGsync] Native Cloud G-Sync/VRR mode is resolved on; keeping low-latency unthrottled presentation.",
    );
  }

  return {
    ...settings,
    requestedCloudGsync: userRequested,
    enableCloudGsync: resolution.enabled,
    cloudGsyncResolution: resolution,
  };
}

function selectReadySessionToClaim(
  activeSessions: ActiveSessionInfo[],
  numericAppId: number,
): ActiveSessionInfo | null {
  return (
    activeSessions.find(
      (session) =>
        isAutoResumeReadySession(session) && session.appId === numericAppId,
    ) ??
    activeSessions.find((session) => isAutoResumeReadySession(session)) ??
    null
  );
}

function selectLaunchingSession(
  activeSessions: ActiveSessionInfo[],
  numericAppId: number,
): ActiveSessionInfo | null {
  return (
    activeSessions.find(
      (session) =>
        session.serverIp &&
        session.appId === numericAppId &&
        session.status === 1,
    ) ??
    activeSessions.find(
      (session) => session.serverIp && session.status === 1,
    ) ??
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
  ipcMain.handle(
    IPC_CHANNELS.AUTH_GET_SESSION,
    async (_event, payload: AuthSessionRequest = {}) => {
      return authService.ensureValidSessionWithStatus(
        Boolean(payload.forceRefresh),
      );
    },
  );

  ipcMain.handle(IPC_CHANNELS.AUTH_GET_PROVIDERS, async () => {
    return authService.getProviders();
  });

  ipcMain.handle(
    IPC_CHANNELS.AUTH_GET_REGIONS,
    async (_event, payload: RegionsFetchRequest) => {
      return authService.getRegions(payload?.token);
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.AUTH_LOGIN,
    async (_event, payload: AuthLoginRequest) => {
      return authService.login(payload);
    },
  );

  ipcMain.handle(IPC_CHANNELS.AUTH_LOGOUT, async () => {
    await authService.logout();
  });

  ipcMain.handle(IPC_CHANNELS.AUTH_LOGOUT_ALL, async () => {
    await authService.logoutAll();
  });

  ipcMain.handle(IPC_CHANNELS.AUTH_GET_SAVED_ACCOUNTS, async () => {
    return authService.getSavedAccounts();
  });

  ipcMain.handle(
    IPC_CHANNELS.AUTH_SWITCH_ACCOUNT,
    async (_event, userId: string) => {
      return authService.switchAccount(userId);
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.AUTH_REMOVE_ACCOUNT,
    async (_event, userId: string) => {
      await authService.removeAccount(userId);
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.SUBSCRIPTION_FETCH,
    async (_event, payload: SubscriptionFetchRequest) => {
      const token = await resolveJwt(payload?.token);
      const streamingBaseUrl =
        payload?.providerStreamingBaseUrl ??
        authService.getSelectedProvider().streamingServiceUrl;
      const userId = payload.userId;

      // Fetch dynamic regions to get the VPC ID (handles Alliance partners correctly)
      const { vpcId } = await fetchDynamicRegions(token, streamingBaseUrl);

      return fetchSubscription(token, userId, vpcId ?? undefined);
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.GAMES_FETCH_MAIN,
    async (_event, payload: GamesFetchRequest) => {
      const token = await resolveJwt(payload?.token);
      const streamingBaseUrl =
        payload?.providerStreamingBaseUrl ??
        authService.getSelectedProvider().streamingServiceUrl;
      refreshScheduler.updateAuthContext(token, streamingBaseUrl);
      return fetchMainGames(token, streamingBaseUrl);
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.GAMES_FETCH_LIBRARY,
    async (_event, payload: GamesFetchRequest) => {
      const token = await resolveJwt(payload?.token);
      const streamingBaseUrl =
        payload?.providerStreamingBaseUrl ??
        authService.getSelectedProvider().streamingServiceUrl;
      refreshScheduler.updateAuthContext(token, streamingBaseUrl);
      return fetchLibraryGames(token, streamingBaseUrl);
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.GAMES_BROWSE_CATALOG,
    async (_event, payload: CatalogBrowseRequest) => {
      const token = await resolveJwt(payload?.token);
      const streamingBaseUrl =
        payload?.providerStreamingBaseUrl ??
        authService.getSelectedProvider().streamingServiceUrl;
      refreshScheduler.updateAuthContext(token, streamingBaseUrl);
      return browseCatalog({
        ...payload,
        token,
        providerStreamingBaseUrl: streamingBaseUrl,
      });
    },
  );

  ipcMain.handle(IPC_CHANNELS.GAMES_FETCH_PUBLIC, async () => {
    return fetchPublicGames();
  });

  ipcMain.handle(
    IPC_CHANNELS.GAMES_RESOLVE_LAUNCH_ID,
    async (_event, payload: ResolveLaunchIdRequest) => {
      const token = await resolveJwt(payload?.token);
      const streamingBaseUrl =
        payload?.providerStreamingBaseUrl ??
        authService.getSelectedProvider().streamingServiceUrl;
      return resolveLaunchAppId(token, payload.appIdOrUuid, streamingBaseUrl);
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.CREATE_SESSION,
    async (_event, payload: SessionCreateRequest) => {
      const token = await resolveJwt(payload.token);
      const streamingBaseUrl =
        payload.streamingBaseUrl ??
        authService.getSelectedProvider().streamingServiceUrl;
      const forceNewSession = shouldForceNewSession(
        payload.existingSessionStrategy,
      );
      const resolvedSettings = await resolveSessionCloudGsyncSettings(
        payload.settings,
      );
      const resolvedPayload: SessionCreateRequest = {
        ...payload,
        settings: resolvedSettings,
      };

      /**
       * Attempt to find and claim an existing active session.
       * Prefers a session whose appId matches the requested game; falls back to
       * any claimable session (serverIp present) if no exact match is found.
       * Returns null when no claimable session exists or the lookup fails.
       *
       * IMPORTANT: Only status=2/3 (ready/streaming) sessions are sent a RESUME claim PUT.
       * Status=1 sessions (still in queue/setup) must NOT receive a RESUME — the server
       * rejects it with SESSION_NOT_PAUSED, and even if we polled past it internally we
       * would bypass the renderer's queue/ad polling loop entirely. Instead, status=1
       * sessions are returned as a minimal SessionInfo so the renderer enters its own
       * polling loop which shows queue position and ads correctly.
       */
      const tryClaimExisting = async (): Promise<SessionInfo | null> => {
        if (!token) return null;
        try {
          const activeSessions = await getActiveSessions(
            token,
            streamingBaseUrl,
          );
          if (activeSessions.length === 0) return null;
          const numericAppId = parseInt(resolvedPayload.appId, 10);

          // First prefer a paused/ready session (status 2 or 3) that can be RESUME'd.
          const readyCandidate = selectReadySessionToClaim(
            activeSessions,
            numericAppId,
          );
          if (readyCandidate) {
            console.log(
              `[CreateSession] Resuming existing session (id=${readyCandidate.sessionId}, appId=${readyCandidate.appId}, status=${readyCandidate.status}) instead of creating new.`,
            );
            return claimSession({
              token,
              streamingBaseUrl,
              sessionId: readyCandidate.sessionId,
              serverIp: readyCandidate.serverIp!,
              appId: resolvedPayload.appId,
              settings: resolvedPayload.settings,
            });
          }

          // A status=1 session is still in queue/setup. Return it so the renderer's
          // polling loop handles queue position and ads — do NOT send a RESUME claim.
          const launchingCandidate = selectLaunchingSession(
            activeSessions,
            numericAppId,
          );
          if (launchingCandidate) {
            console.log(
              `[CreateSession] Found launching session (id=${launchingCandidate.sessionId}, appId=${launchingCandidate.appId}, status=1); returning for renderer queue/ad polling.`,
            );
            try {
              return await pollSession({
                token,
                streamingBaseUrl,
                serverIp: launchingCandidate.serverIp!,
                zone: resolvedPayload.zone,
                sessionId: launchingCandidate.sessionId,
                proxyUrl: payload.proxyUrl,
              });
            } catch (hydrateError) {
              console.warn(
                `[CreateSession] Failed to hydrate launching session ${launchingCandidate.sessionId}; falling back to minimal handoff: ${formatErrorChainForLog(hydrateError)}`,
              );
              return {
                sessionId: launchingCandidate.sessionId,
                status: 1,
                zone: resolvedPayload.zone,
                streamingBaseUrl,
                serverIp: launchingCandidate.serverIp!,
                signalingServer: launchingCandidate.serverIp!,
                signalingUrl:
                  launchingCandidate.signalingUrl ??
                  `wss://${launchingCandidate.serverIp}:443/nvst/`,
                iceServers: [],
              } satisfies SessionInfo;
            }
          }

          return null;
        } catch (claimError) {
          console.warn(
            `[CreateSession] Failed to claim existing session: ${formatErrorChainForLog(claimError)}`,
          );
          return null;
        }
      };

      // Pre-flight check: resume an active session before trying to create a new one.
      if (!forceNewSession) {
        const preChecked = await tryClaimExisting();
        if (preChecked) {
          if (settingsManager.get("discordRichPresence")) {
            void setActivity(
              payload.internalTitle || payload.appId,
              new Date(),
              payload.appId,
            );
          }
          return preChecked;
        }
      }

      try {
        if (forceNewSession && token) {
          await stopActiveSessionsForCreate({
            token,
            streamingBaseUrl,
            zone: resolvedPayload.zone,
            appId: resolvedPayload.appId,
          });
        }
        const sessionResult = await createSession({
          ...resolvedPayload,
          token,
          streamingBaseUrl,
        });
        if (settingsManager.get("discordRichPresence")) {
          void setActivity(
            payload.internalTitle || payload.appId,
            new Date(),
            payload.appId,
          );
        }
        return sessionResult;
      } catch (error) {
        // If the backend rejected the create because a session is already running,
        // attempt a claim now (the pre-flight may have missed a session whose appId
        // was not populated in the list response, or that had no serverIp at the
        // time of the pre-flight but is ready now).
        if (
          !forceNewSession &&
          error instanceof SessionError &&
          error.statusCode === 11
        ) {
          console.warn(
            "[CreateSession] SESSION_LIMIT_EXCEEDED — retrying as session claim.",
          );
          const fallback = await tryClaimExisting();
          if (fallback) {
            if (settingsManager.get("discordRichPresence")) {
              void setActivity(
                payload.internalTitle || payload.appId,
                new Date(),
                payload.appId,
              );
            }
            return fallback;
          }
        }
        rethrowSerializedSessionError(error);
      }
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.POLL_SESSION,
    async (_event, payload: SessionPollRequest) => {
      try {
        const token = await resolveJwt(payload.token);
        return pollSession({
          ...payload,
          token,
          streamingBaseUrl:
            payload.streamingBaseUrl ??
            authService.getSelectedProvider().streamingServiceUrl,
        });
      } catch (error) {
        rethrowSerializedSessionError(error);
      }
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.REPORT_SESSION_AD,
    async (_event, payload: SessionAdReportRequest) => {
      try {
        const token = await resolveJwt(payload.token);
        return reportSessionAd({
          ...payload,
          token,
          streamingBaseUrl:
            payload.streamingBaseUrl ??
            authService.getSelectedProvider().streamingServiceUrl,
        });
      } catch (error) {
        rethrowSerializedSessionError(error);
      }
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.STOP_SESSION,
    async (_event, payload: SessionStopRequest) => {
      try {
        const token = await resolveJwt(payload.token);
        const result = await stopSession({
          ...payload,
          token,
          streamingBaseUrl:
            payload.streamingBaseUrl ??
            authService.getSelectedProvider().streamingServiceUrl,
        });
        void clearActivity();
        return result;
      } catch (error) {
        rethrowSerializedSessionError(error);
      }
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.GET_ACTIVE_SESSIONS,
    async (_event, token?: string, streamingBaseUrl?: string) => {
      const jwt = await resolveJwt(token);
      const baseUrl =
        streamingBaseUrl ??
        authService.getSelectedProvider().streamingServiceUrl;
      return getActiveSessions(jwt, baseUrl);
    },
  );

  ipcMain.handle(IPC_CHANNELS.DISCORD_CLEAR_ACTIVITY, async () => {
    void clearActivity();
  });

  ipcMain.handle(
    IPC_CHANNELS.CLAIM_SESSION,
    async (_event, payload: SessionClaimRequest) => {
      try {
        const token = await resolveJwt(payload.token);
        const streamingBaseUrl =
          payload.streamingBaseUrl ??
          authService.getSelectedProvider().streamingServiceUrl;
        const resolvedSettings = payload.settings
          ? await resolveSessionCloudGsyncSettings(payload.settings)
          : undefined;
        return claimSession({
          ...payload,
          token,
          streamingBaseUrl,
          settings: resolvedSettings,
        });
      } catch (error) {
        rethrowSerializedSessionError(error);
      }
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.SESSION_CONFLICT_DIALOG,
    async (): Promise<SessionConflictChoice> => {
      return showSessionConflictDialog();
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.CONNECT_SIGNALING,
    async (_event, payload: SignalingConnectRequest): Promise<void> => {
      const nextKey = `${payload.sessionId}|${payload.signalingServer}|${payload.signalingUrl ?? ""}`;
      nativeStreamerContext = payload.nativeStreamer ?? null;
      nativeStreamerFallbackSessionId = null;
      if (nativeStreamerContext) {
        console.log(
          "[NativeStreamer] Signaling connect context:",
          JSON.stringify({
            sessionId: nativeStreamerContext.session.sessionId,
            resolution: nativeStreamerContext.settings.resolution,
            fps: nativeStreamerContext.settings.fps,
            codec: nativeStreamerContext.settings.codec,
            negotiatedStreamProfile:
              nativeStreamerContext.session.negotiatedStreamProfile,
            requestedStreamingFeatures:
              nativeStreamerContext.session.requestedStreamingFeatures,
            finalizedStreamingFeatures:
              nativeStreamerContext.session.finalizedStreamingFeatures,
          }),
        );
      }

      if (signalingClient && signalingClientKey === nextKey) {
        console.log(
          "[Signaling] Reuse existing signaling connection (duplicate connect request ignored)",
        );
        return;
      }

      if (signalingClient) {
        signalingClient.disconnect();
      }
      await resetNativeStreamerForSignalingReconnect();
      await prepareNativeStreamerBeforeSignaling();

      signalingClient = new GfnSignalingClient(
        payload.signalingServer,
        payload.sessionId,
        payload.signalingUrl,
      );
      signalingClientKey = nextKey;
      signalingClient.onEvent(routeSignalingEvent);
      try {
        await signalingClient.connect();
      } catch (error) {
        await nativeStreamerManager
          ?.stop("signaling connect failed")
          .catch(() => undefined);
        signalingClient = null;
        signalingClientKey = null;
        throw error;
      }
    },
  );

  ipcMain.handle(IPC_CHANNELS.DISCONNECT_SIGNALING, async (): Promise<void> => {
    await nativeStreamerManager?.stop("signaling disconnect");
    nativeStreamerContext = null;
    nativeStreamerFallbackSessionId = null;
    signalingClient?.disconnect();
    signalingClient = null;
    signalingClientKey = null;
  });

  ipcMain.handle(
    IPC_CHANNELS.SEND_ANSWER,
    async (_event, payload: SendAnswerRequest) => {
      if (!signalingClient) {
        throw new Error("Signaling is not connected");
      }
      return signalingClient.sendAnswer(payload);
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.SEND_ICE_CANDIDATE,
    async (_event, payload: IceCandidatePayload) => {
      if (!signalingClient) {
        throw new Error("Signaling is not connected");
      }
      return signalingClient.sendIceCandidate(payload);
    },
  );

  ipcMain.on(
    IPC_CHANNELS.NATIVE_INPUT,
    (_event, payload: NativeInputPacket) => {
      if (!isNativeStreamerSelected()) {
        return;
      }

      const context = nativeStreamerContext;
      if (
        !context ||
        nativeStreamerFallbackSessionId === context.session.sessionId
      ) {
        return;
      }

      const packet = normalizeNativeInputPacket(payload);
      if (!packet) {
        return;
      }

      nativeStreamerManager?.sendInput(packet);
    },
  );

  ipcMain.on(
    IPC_CHANNELS.NATIVE_RENDER_SURFACE,
    (event, payload: NativeRenderSurfaceUpdate) => {
      if (!isNativeStreamerSelected()) {
        return;
      }

      const window = BrowserWindow.fromWebContents(event.sender);
      if (!window || window.isDestroyed()) {
        return;
      }

      const surface = normalizeNativeRenderSurface(window, payload);
      if (!surface) {
        return;
      }

      getNativeStreamerManager().updateSurface(surface);
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.REQUEST_KEYFRAME,
    async (_event, payload: KeyframeRequest) => {
      if (!signalingClient) {
        throw new Error("Signaling is not connected");
      }
      return signalingClient.requestKeyframe(payload);
    },
  );

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

  ipcMain.handle(
    IPC_CHANNELS.APP_UPDATER_GET_STATE,
    async (): Promise<AppUpdaterState> => {
      return (
        appUpdater?.getState() ?? {
          status: "disabled",
          currentVersion: app.getVersion(),
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
      return (
        appUpdater?.checkForUpdates("manual") ?? {
          status: "disabled",
          currentVersion: app.getVersion(),
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
      return (
        appUpdater?.downloadUpdate() ?? {
          status: "disabled",
          currentVersion: app.getVersion(),
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
      return (
        appUpdater?.quitAndInstall() ?? {
          status: "disabled",
          currentVersion: app.getVersion(),
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

  ipcMain.handle(
    IPC_CHANNELS.SETTINGS_SET,
    async <K extends keyof Settings>(
      _event: Electron.IpcMainInvokeEvent,
      key: K,
      value: Settings[K],
    ) => {
      settingsManager.set(key, value);
      // React to certain setting changes immediately in main process
      try {
        if (key === "autoCheckForUpdates") {
          appUpdater?.setAutomaticChecksEnabled(value as boolean);
        }
        if (
          (key === "streamClientMode" && value !== "native") ||
          key === "nativeStreamerBackend" ||
          key === "nativeStreamerExecutablePath" ||
          key === "nativeCloudGsyncMode" ||
          key === "nativeD3dFullscreenMode" ||
          key === "nativeExternalRenderer"
        ) {
          void nativeStreamerManager?.stop(
            key === "nativeStreamerBackend"
              ? "native streamer backend changed"
              : key === "nativeStreamerExecutablePath"
                ? "native streamer executable changed"
                : key === "nativeCloudGsyncMode"
                  ? "native Cloud G-Sync mode changed"
                  : key === "nativeD3dFullscreenMode"
                    ? "native D3D fullscreen mode changed"
                    : key === "nativeExternalRenderer"
                      ? "native external renderer setting changed"
                      : "native streamer disabled",
          );
          nativeStreamerContext = null;
          nativeStreamerFallbackSessionId = null;
        }
        if (key === "nativeVideoBackend") {
          if (nativeStreamerManager?.hasActiveSession()) {
            console.log(
              "[NativeStreamer] Native video backend changed; active session will keep its current backend until the next native streamer restart.",
            );
          } else {
            void nativeStreamerManager?.stop("native video backend changed");
          }
        }
        if (key === "maxBitrateMbps") {
          updateNativeStreamerBitrateSetting(value);
        }
        if (key === "discordRichPresence") {
          if (value) {
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
    void nativeStreamerManager?.stop("settings reset");
    nativeStreamerContext = null;
    nativeStreamerFallbackSessionId = null;
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
    IPC_CHANNELS.NATIVE_STREAMER_STATUS,
    async (): Promise<NativeStreamerStatus> => {
      return getNativeStreamerManager().probeStatus();
    },
  );

  ipcMain.handle(IPC_CHANNELS.NATIVE_CLOUD_GSYNC_CAPABILITIES, async () => {
    const capabilities = await getNativeCloudGsyncCapabilities(
      settingsManager?.get("nativeCloudGsyncMode") ?? "auto",
    );
    console.log(
      `[CloudGsync] capability probe: ${JSON.stringify(capabilities)}`,
    );
    return capabilities;
  });

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
    (webContents, permission, requestingOrigin) => {
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
