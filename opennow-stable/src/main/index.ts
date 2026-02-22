import { app, BrowserWindow, ipcMain, dialog, systemPreferences, session } from "electron";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { existsSync, readFileSync } from "node:fs";

// Keyboard shortcuts reference (matching Rust implementation):
// F11 - Toggle fullscreen (handled in main process)
// F3  - Toggle stats overlay (handled in renderer)
// Ctrl+Shift+Q - Stop streaming (handled in renderer)
// F8  - Toggle mouse/pointer lock (handled in main process via IPC)

import { IPC_CHANNELS } from "@shared/ipc";
import { initLogCapture, exportLogs } from "@shared/logger";
import type {
  MainToRendererSignalingEvent,
  AuthLoginRequest,
  AuthSessionRequest,
  GamesFetchRequest,
  ResolveLaunchIdRequest,
  RegionsFetchRequest,
  SessionCreateRequest,
  SessionPollRequest,
  SessionStopRequest,
  SessionClaimRequest,
  SignalingConnectRequest,
  SendAnswerRequest,
  IceCandidatePayload,
  Settings,
  VideoAccelerationPreference,
  VideoDecodeBackend,
  SubscriptionFetchRequest,
  SessionConflictChoice,
} from "@shared/gfn";

import { getSettingsManager, type SettingsManager } from "./settings";

import { createSession, pollSession, stopSession, getActiveSessions, claimSession } from "./gfn/cloudmatch";
import { AuthService } from "./gfn/auth";
import {
  fetchLibraryGames,
  fetchMainGames,
  fetchPublicGames,
  resolveLaunchAppId,
} from "./gfn/games";
import { fetchSubscription, fetchDynamicRegions } from "./gfn/subscription";
import { GfnSignalingClient } from "./gfn/signaling";
import { isSessionError, SessionError, GfnErrorCode } from "./gfn/errorCodes";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

// Configure Chromium video and WebRTC behavior before app.whenReady().

interface BootstrapVideoPreferences {
  decoderPreference: VideoAccelerationPreference;
  encoderPreference: VideoAccelerationPreference;
  videoDecodeBackend: VideoDecodeBackend;
}

function isAccelerationPreference(value: unknown): value is VideoAccelerationPreference {
  return value === "auto" || value === "hardware" || value === "software";
}

function isVideoDecodeBackend(value: unknown): value is VideoDecodeBackend {
  return value === "auto" || value === "vaapi" || value === "v4l2" || value === "software";
}

function loadBootstrapVideoPreferences(): BootstrapVideoPreferences {
  const defaults: BootstrapVideoPreferences = {
    decoderPreference: "auto",
    encoderPreference: "auto",
    videoDecodeBackend: "auto",
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
      videoDecodeBackend: isVideoDecodeBackend(parsed.videoDecodeBackend)
        ? parsed.videoDecodeBackend
        : defaults.videoDecodeBackend,
    };
  } catch {
    return defaults;
  }
}

const bootstrapVideoPrefs = loadBootstrapVideoPreferences();
console.log(
  `[Main] Video acceleration preference: decode=${bootstrapVideoPrefs.decoderPreference}, encode=${bootstrapVideoPrefs.encoderPreference}, videoDecodeBackend=${bootstrapVideoPrefs.videoDecodeBackend}`,
);

// --- Platform-specific HW video decode features ---
const platformFeatures: string[] = [];
const isLinuxArm = process.platform === "linux" && (process.arch === "arm64" || process.arch === "arm");
const isLinuxArm = process.platform === "linux" && (process.arch === "arm64" || process.arch === "arm");

const platformFeatures: string[] = [];
const disableFeatures: string[] = [
  "WebRtcHideLocalIpsWithMdns",
];
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
  const pref = bootstrapVideoPrefs.videoDecodeBackend;

  if (pref === "software") {
    disableFeatures.push("UseChromeOSDirectVideoDecoder");
  } else if (pref === "vaapi") {
    if (bootstrapVideoPrefs.decoderPreference !== "software") {
      platformFeatures.push("VaapiVideoDecoder");
      platformFeatures.push("VaapiIgnoreDriverChecks");
    }
    disableFeatures.push("UseChromeOSDirectVideoDecoder");
  } else if (pref === "v4l2") {
    platformFeatures.push("UseChromeOSDirectVideoDecoder");
  } else {
    // auto: select based on architecture
    if (isLinuxArm) {
      platformFeatures.push("UseChromeOSDirectVideoDecoder");
    } else {
      if (bootstrapVideoPrefs.decoderPreference !== "software") {
        platformFeatures.push("VaapiVideoDecoder");
        platformFeatures.push("VaapiIgnoreDriverChecks");
      }
      disableFeatures.push("UseChromeOSDirectVideoDecoder");
    }
  }

  if (bootstrapVideoPrefs.encoderPreference !== "software") {
    platformFeatures.push("VaapiVideoEncoder");  }
}
// macOS: VideoToolbox handles HW acceleration natively, no extra feature flags needed

app.commandLine.appendSwitch("enable-features",
  [
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
}app.commandLine.appendSwitch("disable-features", disableFeatures.join(","));

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
let signalingClient: GfnSignalingClient | null = null;
let signalingClientKey: string | null = null;
let authService: AuthService;
let settingsManager: SettingsManager;

function emitToRenderer(event: MainToRendererSignalingEvent): void {
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send(IPC_CHANNELS.SIGNALING_EVENT, event);
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

  // Handle F11 fullscreen toggle — send to renderer so it uses W3C Fullscreen API
  // (which enables navigator.keyboard.lock for Escape key capture)
  mainWindow.webContents.on("before-input-event", (event, input) => {
    if (input.key === "F11" && input.type === "keyDown") {
      event.preventDefault();
      if (mainWindow && !mainWindow.isDestroyed()) {
        mainWindow.webContents.send("app:toggle-fullscreen");
      }
    }
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
    throw error.toJSON();
  }
  throw error;
}

function registerIpcHandlers(): void {
  ipcMain.handle(IPC_CHANNELS.AUTH_GET_SESSION, async (_event, payload: AuthSessionRequest = {}) => {
    return authService.ensureValidSessionWithStatus(Boolean(payload.forceRefresh));
  });

  ipcMain.handle(IPC_CHANNELS.AUTH_GET_PROVIDERS, async () => {
    return authService.getProviders();
  });

  ipcMain.handle(IPC_CHANNELS.AUTH_GET_REGIONS, async (_event, payload: RegionsFetchRequest) => {
    return authService.getRegions(payload?.token);
  });

  ipcMain.handle(IPC_CHANNELS.AUTH_LOGIN, async (_event, payload: AuthLoginRequest) => {
    return authService.login(payload);
  });

  ipcMain.handle(IPC_CHANNELS.AUTH_LOGOUT, async () => {
    await authService.logout();
  });

  ipcMain.handle(IPC_CHANNELS.SUBSCRIPTION_FETCH, async (_event, payload: SubscriptionFetchRequest) => {
    const token = await resolveJwt(payload?.token);
    const streamingBaseUrl =
      payload?.providerStreamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl;
    const userId = payload.userId;

    // Fetch dynamic regions to get the VPC ID (handles Alliance partners correctly)
    const { vpcId } = await fetchDynamicRegions(token, streamingBaseUrl);

    return fetchSubscription(token, userId, vpcId ?? undefined);
  });

  ipcMain.handle(IPC_CHANNELS.GAMES_FETCH_MAIN, async (_event, payload: GamesFetchRequest) => {
    const token = await resolveJwt(payload?.token);
    const streamingBaseUrl =
      payload?.providerStreamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl;
    return fetchMainGames(token, streamingBaseUrl);
  });

  ipcMain.handle(IPC_CHANNELS.GAMES_FETCH_LIBRARY, async (_event, payload: GamesFetchRequest) => {
    const token = await resolveJwt(payload?.token);
    const streamingBaseUrl =
      payload?.providerStreamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl;
    return fetchLibraryGames(token, streamingBaseUrl);
  });

  ipcMain.handle(IPC_CHANNELS.GAMES_FETCH_PUBLIC, async () => {
    return fetchPublicGames();
  });

  ipcMain.handle(IPC_CHANNELS.GAMES_RESOLVE_LAUNCH_ID, async (_event, payload: ResolveLaunchIdRequest) => {
    const token = await resolveJwt(payload?.token);
    const streamingBaseUrl =
      payload?.providerStreamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl;
    return resolveLaunchAppId(token, payload.appIdOrUuid, streamingBaseUrl);
  });

  ipcMain.handle(IPC_CHANNELS.CREATE_SESSION, async (_event, payload: SessionCreateRequest) => {
    try {
      const token = await resolveJwt(payload.token);
      const streamingBaseUrl = payload.streamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl;
      return createSession({
        ...payload,
        token,
        streamingBaseUrl,
      });
    } catch (error) {
      rethrowSerializedSessionError(error);
    }
  });

  ipcMain.handle(IPC_CHANNELS.POLL_SESSION, async (_event, payload: SessionPollRequest) => {
    try {
      const token = await resolveJwt(payload.token);
      return pollSession({
        ...payload,
        token,
        streamingBaseUrl: payload.streamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl,
      });
    } catch (error) {
      rethrowSerializedSessionError(error);
    }
  });

  ipcMain.handle(IPC_CHANNELS.STOP_SESSION, async (_event, payload: SessionStopRequest) => {
    try {
      const token = await resolveJwt(payload.token);
      return stopSession({
        ...payload,
        token,
        streamingBaseUrl: payload.streamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl,
      });
    } catch (error) {
      rethrowSerializedSessionError(error);
    }
  });

  ipcMain.handle(IPC_CHANNELS.GET_ACTIVE_SESSIONS, async (_event, token?: string, streamingBaseUrl?: string) => {
    const jwt = await resolveJwt(token);
    const baseUrl = streamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl;
    return getActiveSessions(jwt, baseUrl);
  });

  ipcMain.handle(IPC_CHANNELS.CLAIM_SESSION, async (_event, payload: SessionClaimRequest) => {
    try {
      const token = await resolveJwt(payload.token);
      const streamingBaseUrl = payload.streamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl;
      return claimSession({
        ...payload,
        token,
        streamingBaseUrl,
      });
    } catch (error) {
      rethrowSerializedSessionError(error);
    }
  });

  ipcMain.handle(IPC_CHANNELS.SESSION_CONFLICT_DIALOG, async (): Promise<SessionConflictChoice> => {
    return showSessionConflictDialog();
  });

  ipcMain.handle(
    IPC_CHANNELS.CONNECT_SIGNALING,
    async (_event, payload: SignalingConnectRequest): Promise<void> => {
      const nextKey = `${payload.sessionId}|${payload.signalingServer}|${payload.signalingUrl ?? ""}`;
      if (signalingClient && signalingClientKey === nextKey) {
        console.log("[Signaling] Reuse existing signaling connection (duplicate connect request ignored)");
        return;
      }

      if (signalingClient) {
        signalingClient.disconnect();
      }

      signalingClient = new GfnSignalingClient(
        payload.signalingServer,
        payload.sessionId,
        payload.signalingUrl,
      );
      signalingClientKey = nextKey;
      signalingClient.onEvent(emitToRenderer);
      await signalingClient.connect();
    },
  );

  ipcMain.handle(IPC_CHANNELS.DISCONNECT_SIGNALING, async (): Promise<void> => {
    signalingClient?.disconnect();
    signalingClient = null;
    signalingClientKey = null;
  });

  ipcMain.handle(IPC_CHANNELS.SEND_ANSWER, async (_event, payload: SendAnswerRequest) => {
    if (!signalingClient) {
      throw new Error("Signaling is not connected");
    }
    return signalingClient.sendAnswer(payload);
  });

  ipcMain.handle(IPC_CHANNELS.SEND_ICE_CANDIDATE, async (_event, payload: IceCandidatePayload) => {
    if (!signalingClient) {
      throw new Error("Signaling is not connected");
    }
    return signalingClient.sendIceCandidate(payload);
  });

  // Toggle fullscreen via IPC (for completeness)
  ipcMain.handle(IPC_CHANNELS.TOGGLE_FULLSCREEN, async () => {
    if (mainWindow && !mainWindow.isDestroyed()) {
      const isFullScreen = mainWindow.isFullScreen();
      mainWindow.setFullScreen(!isFullScreen);
    }
  });

  // Toggle pointer lock via IPC (F8 shortcut)
  ipcMain.handle(IPC_CHANNELS.TOGGLE_POINTER_LOCK, async () => {
    if (mainWindow && !mainWindow.isDestroyed()) {
      mainWindow.webContents.send("app:toggle-pointer-lock");
    }
  });

  // Settings IPC handlers
  ipcMain.handle(IPC_CHANNELS.SETTINGS_GET, async (): Promise<Settings> => {
    return settingsManager.getAll();
  });

  ipcMain.handle(IPC_CHANNELS.SETTINGS_SET, async <K extends keyof Settings>(_event: Electron.IpcMainInvokeEvent, key: K, value: Settings[K]) => {
    settingsManager.set(key, value);
  });

  ipcMain.handle(IPC_CHANNELS.SETTINGS_RESET, async (): Promise<Settings> => {
    return settingsManager.reset();
  });

  // Logs export IPC handler
  ipcMain.handle(IPC_CHANNELS.LOGS_EXPORT, async (_event, format: "text" | "json" = "text"): Promise<string> => {
    return exportLogs(format);
  });

  // Save window size when it changes
  ipcMain.handle(IPC_CHANNELS.GET_PLATFORM_INFO, () => {
    return { platform: process.platform, arch: process.arch };
  });  mainWindow?.on("resize", () => {
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

  authService = new AuthService(join(app.getPath("userData"), "auth-state.json"));
  await authService.initialize();

  settingsManager = getSettingsManager();

  // Request microphone permission on macOS at startup
  if (process.platform === "darwin") {
    const micStatus = systemPreferences.getMediaAccessStatus("microphone");
    console.log("[Main] macOS microphone permission status:", micStatus);
    if (micStatus !== "granted") {
      const granted = await systemPreferences.askForMediaAccess("microphone");
      console.log("[Main] Requested microphone permission:", granted);
    }
  }

  // Set up permission handlers for getUserMedia, fullscreen, pointer lock
  session.defaultSession.setPermissionRequestHandler((webContents, permission, callback) => {
    const url = webContents.getURL();
    console.log(`[Main] Permission request: ${permission} from ${url}`);

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
      console.log(`[Main] Granting permission: ${permission}`);
      callback(true);
      return;
    }

    callback(false);
  });

  session.defaultSession.setPermissionCheckHandler((webContents, permission, requestingOrigin) => {
    console.log(`[Main] Permission check: ${permission} from ${requestingOrigin}`);

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

  registerIpcHandlers();
  await createMainWindow();

  app.on("activate", async () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      await createMainWindow();
    }
  });
});

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") {
    app.quit();
  }
});

app.on("before-quit", () => {
  signalingClient?.disconnect();
  signalingClient = null;
  signalingClientKey = null;
});

// Export for use by other modules
export { showSessionConflictDialog, isSessionConflictError };
