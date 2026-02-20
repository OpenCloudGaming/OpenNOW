import { app, BrowserWindow, ipcMain, dialog, session, systemPreferences } from "electron";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { existsSync, readFileSync } from "node:fs";

import { IPC_CHANNELS } from "@shared/ipc";
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
  DiscordPresencePayload,
  FlightProfile,
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
import { isSessionError, SessionError } from "./gfn/errorCodes";
import { DiscordPresenceService } from "./discord/DiscordPresenceService";
import { FlightProfileManager } from "./flight/FlightProfiles";
import { getOsHdrInfo } from "./hdr/hdrDetect";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

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

const isLinuxArm = process.platform === "linux" && (process.arch === "arm64" || process.arch === "arm");

const platformFeatures: string[] = [];
const disableFeatures: string[] = [
  "WebRtcHideLocalIpsWithMdns",
];

if (process.platform === "win32") {
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
    platformFeatures.push("VaapiVideoEncoder");
  }
}

app.commandLine.appendSwitch("enable-features",
  [
    "Dav1dVideoDecoder",
    "HardwareMediaKeyHandling",
    ...platformFeatures,
  ].join(","),
);

app.commandLine.appendSwitch("disable-features", disableFeatures.join(","));

app.commandLine.appendSwitch("force-fieldtrials",
  [
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

app.commandLine.appendSwitch("ignore-gpu-blocklist");

app.commandLine.appendSwitch("disable-renderer-backgrounding");
app.commandLine.appendSwitch("disable-backgrounding-occluded-windows");
app.commandLine.appendSwitch("max-gum-fps", "999");

let mainWindow: BrowserWindow | null = null;
let signalingClient: GfnSignalingClient | null = null;
let signalingClientKey: string | null = null;
let authService: AuthService;
let settingsManager: SettingsManager;
let discordService: DiscordPresenceService;
let flightProfileManager: FlightProfileManager;

const grantedHidDeviceIds = new Set<string>();

function emitToRenderer(event: MainToRendererSignalingEvent): void {
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send(IPC_CHANNELS.SIGNALING_EVENT, event);
  }
}

function emitSessionExpired(reason: string): void {
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send(IPC_CHANNELS.AUTH_SESSION_EXPIRED, reason);
  }
}

async function withRetryOn401<T>(
  fn: (token: string) => Promise<T>,
  explicitToken?: string,
): Promise<T> {
  const token = await resolveJwt(explicitToken);
  try {
    return await fn(token);
  } catch (error) {
    const { shouldRetry, token: newToken } = await authService.handleApiError(error);
    if (shouldRetry && newToken) {
      return fn(newToken);
    }
    throw error;
  }
}

function setupWebHidPermissions(): void {
  const ses = session.defaultSession;

  ses.setDevicePermissionHandler((details) => {
    if (details.deviceType === "hid") {
      return true;
    }
    return true;
  });

  ses.setPermissionCheckHandler((_webContents, permission) => {
    if (permission === "hid" || permission === "media" || permission === "keyboardLock") {
      return true;
    }
    return true;
  });

  ses.setPermissionRequestHandler((_webContents, permission, callback) => {
    if (permission === "media" || permission === "keyboardLock") {
      callback(true);
      return;
    }
    callback(true);
  });

  ses.on("select-hid-device", (event, details, callback) => {
    event.preventDefault();
    const ungranted = details.deviceList.find((d) => !grantedHidDeviceIds.has(d.deviceId));
    const selected = ungranted ?? details.deviceList[0];
    if (selected) {
      grantedHidDeviceIds.add(selected.deviceId);
      callback(selected.deviceId);
    } else {
      callback("");
    }
  });

  ses.on("hid-device-added", (_event, _details) => {
    // WebHID connect event handled in renderer via navigator.hid
  });

  ses.on("hid-device-removed", (_event, _details) => {
    // WebHID disconnect event handled in renderer via navigator.hid
  });
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

  mainWindow.webContents.on("before-input-event", (event, input) => {
    if (input.key === "F11" && input.type === "keyDown") {
      event.preventDefault();
      if (mainWindow && !mainWindow.isDestroyed()) {
        mainWindow.webContents.send("app:toggle-fullscreen");
      }
    }
  });

  if (process.platform === "win32") {
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
    return withRetryOn401(async (token) => {
      const streamingBaseUrl =
        payload?.providerStreamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl;
      const userId = payload.userId;
      const { vpcId } = await fetchDynamicRegions(token, streamingBaseUrl);
      return fetchSubscription(token, userId, vpcId ?? undefined);
    }, payload?.token);
  });

  ipcMain.handle(IPC_CHANNELS.GAMES_FETCH_MAIN, async (_event, payload: GamesFetchRequest) => {
    return withRetryOn401(async (token) => {
      const streamingBaseUrl =
        payload?.providerStreamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl;
      return fetchMainGames(token, streamingBaseUrl);
    }, payload?.token);
  });

  ipcMain.handle(IPC_CHANNELS.GAMES_FETCH_LIBRARY, async (_event, payload: GamesFetchRequest) => {
    return withRetryOn401(async (token) => {
      const streamingBaseUrl =
        payload?.providerStreamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl;
      return fetchLibraryGames(token, streamingBaseUrl);
    }, payload?.token);
  });

  ipcMain.handle(IPC_CHANNELS.GAMES_FETCH_PUBLIC, async () => {
    return fetchPublicGames();
  });

  ipcMain.handle(IPC_CHANNELS.GAMES_RESOLVE_LAUNCH_ID, async (_event, payload: ResolveLaunchIdRequest) => {
    return withRetryOn401(async (token) => {
      const streamingBaseUrl =
        payload?.providerStreamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl;
      return resolveLaunchAppId(token, payload.appIdOrUuid, streamingBaseUrl);
    }, payload?.token);
  });

  ipcMain.handle(IPC_CHANNELS.CREATE_SESSION, async (_event, payload: SessionCreateRequest) => {
    try {
      return await withRetryOn401(async (token) => {
        const streamingBaseUrl = payload.streamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl;
        return createSession({
          ...payload,
          token,
          streamingBaseUrl,
        });
      }, payload.token);
    } catch (error) {
      rethrowSerializedSessionError(error);
    }
  });

  ipcMain.handle(IPC_CHANNELS.POLL_SESSION, async (_event, payload: SessionPollRequest) => {
    try {
      return await withRetryOn401(async (token) => {
        return pollSession({
          ...payload,
          token,
          streamingBaseUrl: payload.streamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl,
        });
      }, payload.token);
    } catch (error) {
      rethrowSerializedSessionError(error);
    }
  });

  ipcMain.handle(IPC_CHANNELS.STOP_SESSION, async (_event, payload: SessionStopRequest) => {
    try {
      return await withRetryOn401(async (token) => {
        return stopSession({
          ...payload,
          token,
          streamingBaseUrl: payload.streamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl,
        });
      }, payload.token);
    } catch (error) {
      rethrowSerializedSessionError(error);
    }
  });

  ipcMain.handle(IPC_CHANNELS.GET_ACTIVE_SESSIONS, async (_event, token?: string, streamingBaseUrl?: string) => {
    return withRetryOn401(async (jwt) => {
      const baseUrl = streamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl;
      return getActiveSessions(jwt, baseUrl);
    }, token);
  });

  ipcMain.handle(IPC_CHANNELS.CLAIM_SESSION, async (_event, payload: SessionClaimRequest) => {
    try {
      return await withRetryOn401(async (token) => {
        const streamingBaseUrl = payload.streamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl;
        return claimSession({
          ...payload,
          token,
          streamingBaseUrl,
        });
      }, payload.token);
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

  ipcMain.handle(IPC_CHANNELS.TOGGLE_FULLSCREEN, async () => {
    if (mainWindow && !mainWindow.isDestroyed()) {
      const isFullScreen = mainWindow.isFullScreen();
      mainWindow.setFullScreen(!isFullScreen);
    }
  });

  ipcMain.handle(IPC_CHANNELS.TOGGLE_POINTER_LOCK, async () => {
    if (mainWindow && !mainWindow.isDestroyed()) {
      mainWindow.webContents.send("app:toggle-pointer-lock");
    }
  });

  ipcMain.handle(IPC_CHANNELS.SETTINGS_GET, async (): Promise<Settings> => {
    return settingsManager.getAll();
  });

  ipcMain.handle(IPC_CHANNELS.SETTINGS_SET, async <K extends keyof Settings>(_event: Electron.IpcMainInvokeEvent, key: K, value: Settings[K]) => {
    settingsManager.set(key, value);
    if (key === "discordPresenceEnabled" || key === "discordClientId") {
      const all = settingsManager.getAll();
      void discordService.updateConfig(all.discordPresenceEnabled, all.discordClientId);
    }
  });

  ipcMain.handle(IPC_CHANNELS.SETTINGS_RESET, async (): Promise<Settings> => {
    return settingsManager.reset();
  });

  ipcMain.handle(IPC_CHANNELS.DISCORD_UPDATE_PRESENCE, async (_event, payload: DiscordPresencePayload) => {
    await discordService.updatePresence(payload);
  });

  ipcMain.handle(IPC_CHANNELS.DISCORD_CLEAR_PRESENCE, async () => {
    await discordService.clearPresence();
  });

  ipcMain.handle(IPC_CHANNELS.FLIGHT_GET_PROFILE, (_event, vidPid: string, gameId?: string) => {
    return flightProfileManager.getProfile(vidPid, gameId);
  });

  ipcMain.handle(IPC_CHANNELS.FLIGHT_SET_PROFILE, (_event, profile: FlightProfile) => {
    flightProfileManager.setProfile(profile);
  });

  ipcMain.handle(IPC_CHANNELS.FLIGHT_DELETE_PROFILE, (_event, vidPid: string, gameId?: string) => {
    flightProfileManager.deleteProfile(vidPid, gameId);
  });

  ipcMain.handle(IPC_CHANNELS.FLIGHT_GET_ALL_PROFILES, () => {
    return flightProfileManager.getAllProfiles();
  });

  ipcMain.handle(IPC_CHANNELS.FLIGHT_RESET_PROFILE, (_event, vidPid: string) => {
    return flightProfileManager.resetProfile(vidPid);
  });

  ipcMain.handle(IPC_CHANNELS.HDR_GET_OS_INFO, () => {
    return getOsHdrInfo();
  });

  ipcMain.handle(IPC_CHANNELS.MIC_ENUMERATE_DEVICES, async () => {
    return [];
  });

  ipcMain.handle(IPC_CHANNELS.APP_RELAUNCH, () => {
    app.relaunch();
    app.exit(0);
  });

  ipcMain.handle(IPC_CHANNELS.GET_PLATFORM_INFO, () => {
    return { platform: process.platform, arch: process.arch };
  });

  mainWindow?.on("resize", () => {
    if (mainWindow && !mainWindow.isDestroyed()) {
      const [width, height] = mainWindow.getSize();
      settingsManager.set("windowWidth", width);
      settingsManager.set("windowHeight", height);
    }
  });
}

app.whenReady().then(async () => {
  authService = new AuthService(join(app.getPath("userData"), "auth-state.json"));
  await authService.initialize();

  authService.onSessionExpired(emitSessionExpired);

  settingsManager = getSettingsManager();

  const allSettings = settingsManager.getAll();
  discordService = new DiscordPresenceService(
    allSettings.discordPresenceEnabled,
    allSettings.discordClientId,
  );
  void discordService.initialize();

  flightProfileManager = new FlightProfileManager();

  if (process.platform === "darwin") {
    const micAccess = systemPreferences.getMediaAccessStatus("microphone");
    console.log(`[Main] macOS microphone access status: ${micAccess}`);
    if (micAccess !== "granted") {
      const granted = await systemPreferences.askForMediaAccess("microphone");
      console.log(`[Main] macOS microphone access prompt result: ${granted}`);
    }
  }

  setupWebHidPermissions();
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
  void discordService.dispose();
});

export { showSessionConflictDialog, isSessionConflictError };
