import type {
  AppUpdaterState,
  AuthSessionResult,
  LoginProvider,
  MainToRendererSignalingEvent,
  MediaListingResult,
  MicrophonePermissionResult,
  OpenNowApi,
  PingResult,
  PrintedWasteQueueData,
  PrintedWasteServerMapping,
  RecordingBeginResult,
  RecordingEntry,
  ScreenshotEntry,
  Settings,
  StreamRegion,
  ThankYouDataResult,
} from "@shared/gfn";
import { DEFAULT_SETTINGS } from "@shared/gfn";
import { parseSerializedSessionErrorTransport } from "@shared/sessionError";

const WEBOS_RUNTIME = import.meta.env.VITE_OPENNOW_RUNTIME === "webos";
const WEBOS_SERVICE_URI = "luna://com.zortos.opennow.stable.service";
const SETTINGS_STORAGE_KEY = "opennow.webos.settings.v1";
const SCREENSHOT_STORAGE_KEY = "opennow.webos.screenshots.v1";
const SERVICE_TIMEOUT_MS = 30_000;

const DEFAULT_PROVIDER: LoginProvider = {
  idpId: "PDiAhv2kJTFeQ7WOPqiQ2tRZ7lGhR2X11dXvM4TZSxg",
  code: "NVIDIA",
  displayName: "NVIDIA",
  streamingServiceUrl: "https://prod.cloudmatchbeta.nvidiagrid.net/",
  priority: 0,
};

const WEBOS_DEFAULT_SETTINGS: Settings = {
  ...DEFAULT_SETTINGS,
  resolution: "1920x1080",
  aspectRatio: "16:9",
  fps: 60,
  maxBitrateMbps: 50,
  codec: "H264",
  colorQuality: "8bit_420",
  controllerMode: true,
  controllerUiSounds: true,
  controllerBackgroundAnimations: true,
  autoLoadControllerLibrary: true,
  autoFullScreen: true,
  autoCheckForUpdates: false,
  windowWidth: 1920,
  windowHeight: 1080,
};

interface WebOsServiceResponse<T> {
  returnValue?: boolean;
  result?: T;
  event?: T;
  errorCode?: string | number;
  errorText?: string;
}

type WebOsServiceBridgeConstructor = new () => {
  onservicecallback?: (message: string) => void;
  call: (url: string, params: string) => void;
  cancel?: () => void;
};

function getWebOsServiceBridge(): WebOsServiceBridgeConstructor | null {
  const candidate = (window as unknown as { WebOSServiceBridge?: WebOsServiceBridgeConstructor }).WebOSServiceBridge;
  return typeof candidate === "function" ? candidate : null;
}

function serviceUnavailableError(method: string): Error {
  return new Error(`OpenNOW webOS service is unavailable for ${method}. Install the packaged webOS app with its bundled service.`);
}

function normalizeServicePayload<T>(raw: string, method: string): T {
  let payload: WebOsServiceResponse<T>;
  try {
    payload = JSON.parse(raw) as WebOsServiceResponse<T>;
  } catch (error) {
    throw new Error(`Invalid webOS service response for ${method}: ${String(error)}`);
  }

  if (payload.returnValue === false) {
    const message = payload.errorText ?? `webOS service ${method} failed`;
    const sessionError = parseSerializedSessionErrorTransport(message);
    if (sessionError) {
      throw sessionError;
    }
    const suffix = payload.errorCode ? ` (${payload.errorCode})` : "";
    throw new Error(`${message}${suffix}`);
  }

  if ("result" in payload) {
    return payload.result as T;
  }

  return payload as T;
}

function callService<T>(method: string, parameters: Record<string, unknown> = {}, timeoutMs = SERVICE_TIMEOUT_MS): Promise<T> {
  const Bridge = getWebOsServiceBridge();
  if (!Bridge) {
    return Promise.reject(serviceUnavailableError(method));
  }

  return new Promise<T>((resolve, reject) => {
    const bridge = new Bridge();
    const timer = window.setTimeout(() => {
      bridge.cancel?.();
      reject(new Error(`webOS service ${method} timed out`));
    }, timeoutMs);

    bridge.onservicecallback = (message: string) => {
      window.clearTimeout(timer);
      try {
        resolve(normalizeServicePayload<T>(message, method));
      } catch (error) {
        reject(error);
      }
    };

    try {
      bridge.call(`${WEBOS_SERVICE_URI}/${method}`, JSON.stringify(parameters));
    } catch (error) {
      window.clearTimeout(timer);
      reject(error);
    }
  });
}

function subscribeService<T>(
  method: string,
  parameters: Record<string, unknown>,
  listener: (payload: T) => void,
): () => void {
  const Bridge = getWebOsServiceBridge();
  if (!Bridge) {
    return () => undefined;
  }

  const bridge = new Bridge();
  bridge.onservicecallback = (message: string) => {
    try {
      const payload = normalizeServicePayload<T>(message, method);
      listener(payload);
    } catch (error) {
      console.warn(`[webOS] ${method} subscription failed:`, error);
    }
  };

  try {
    bridge.call(`${WEBOS_SERVICE_URI}/${method}`, JSON.stringify({ ...parameters, subscribe: true }));
  } catch (error) {
    console.warn(`[webOS] failed to subscribe to ${method}:`, error);
  }

  return () => bridge.cancel?.();
}

function readJson<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) {
      return fallback;
    }
    return JSON.parse(raw) as T;
  } catch {
    return fallback;
  }
}

function writeJson(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch (error) {
    console.warn(`[webOS] failed to persist ${key}:`, error);
  }
}

function readSettings(): Settings {
  const stored = readJson<Partial<Settings>>(SETTINGS_STORAGE_KEY, {});
  return { ...WEBOS_DEFAULT_SETTINGS, ...stored };
}

function writeSettings(settings: Settings): void {
  writeJson(SETTINGS_STORAGE_KEY, settings);
}

function disabledUpdaterState(message = "Updates are handled by the webOS package installer."): AppUpdaterState {
  return {
    status: "disabled",
    currentVersion: import.meta.env.VITE_OPENNOW_VERSION || "0.0.0",
    message,
    updateSource: "github-releases",
    canCheck: false,
    canDownload: false,
    canInstall: false,
    isPackaged: true,
  };
}

function unsupported<T>(feature: string): Promise<T> {
  return Promise.reject(new Error(`${feature} is not available in the webOS runtime yet.`));
}

async function fetchJson<T>(url: string): Promise<T> {
  const response = await fetch(url, { headers: { Accept: "application/json" } });
  if (!response.ok) {
    throw new Error(`Request failed (${response.status})`);
  }
  const payload = await response.json() as { status?: boolean; data?: T } | T;
  if (
    payload &&
    typeof payload === "object" &&
    !Array.isArray(payload) &&
    "data" in payload &&
    (payload as { data?: T }).data != null
  ) {
    return (payload as { data: T }).data;
  }
  return payload as T;
}

function listScreenshots(): ScreenshotEntry[] {
  return readJson<ScreenshotEntry[]>(SCREENSHOT_STORAGE_KEY, []);
}

function persistScreenshots(items: ScreenshotEntry[]): void {
  writeJson(SCREENSHOT_STORAGE_KEY, items.slice(0, 30));
}

function createWebOsApi(): OpenNowApi {
  return {
    getAuthSession: async (): Promise<AuthSessionResult> => {
      try {
        return await callService<AuthSessionResult>("getAuthSession", {});
      } catch {
        return {
          session: null,
          refresh: {
            attempted: false,
            forced: false,
            outcome: "not_attempted",
            message: "No saved webOS session.",
          },
        };
      }
    },
    getLoginProviders: async () => {
      try {
        return await callService<LoginProvider[]>("getLoginProviders", {});
      } catch {
        return [DEFAULT_PROVIDER];
      }
    },
    getRegions: (input = {}) => callService("getRegions", { input }),
    login: (input) => callService("login", { input }, 120_000),
    logout: () => callService<void>("logout", {}).catch(() => undefined),
    fetchSubscription: (input) => callService("fetchSubscription", { input }),
    fetchMainGames: (input) => callService("fetchMainGames", { input }),
    fetchLibraryGames: (input) => callService("fetchLibraryGames", { input }),
    browseCatalog: (input) => callService("browseCatalog", { input }),
    fetchPublicGames: () => callService("fetchPublicGames", {}),
    resolveLaunchAppId: (input) => callService("resolveLaunchAppId", { input }),
    createSession: (input) => callService("createSession", { input }),
    pollSession: (input) => callService("pollSession", { input }),
    reportSessionAd: (input) => callService("reportSessionAd", { input }),
    stopSession: (input) => callService("stopSession", { input }),
    getActiveSessions: (token, streamingBaseUrl) => callService("getActiveSessions", { token, streamingBaseUrl }),
    claimSession: (input) => callService("claimSession", { input }),
    showSessionConflictDialog: async () => "resume",
    connectSignaling: (input) => callService("connectSignaling", { input }),
    disconnectSignaling: () => callService("disconnectSignaling", {}),
    sendAnswer: (input) => callService("sendAnswer", { input }),
    sendIceCandidate: (input) => callService("sendIceCandidate", { input }),
    requestKeyframe: (input) => callService("requestKeyframe", { input }),
    onSignalingEvent: (listener: (event: MainToRendererSignalingEvent) => void) =>
      subscribeService<MainToRendererSignalingEvent>("signalingEvents", {}, listener),
    onToggleFullscreen: () => () => undefined,
    quitApp: async () => {
      window.close();
    },
    getUpdaterState: async () => disabledUpdaterState(),
    checkForUpdates: async () => disabledUpdaterState(),
    downloadUpdate: async () => disabledUpdaterState(),
    installUpdateAndRestart: async () => disabledUpdaterState(),
    onUpdaterStateChanged: () => () => undefined,
    setFullscreen: async (value) => {
      if (value && !document.fullscreenElement) {
        await document.documentElement.requestFullscreen?.();
      } else if (!value && document.fullscreenElement) {
        await document.exitFullscreen?.();
      }
    },
    toggleFullscreen: async () => {
      if (document.fullscreenElement) {
        await document.exitFullscreen?.();
      } else {
        await document.documentElement.requestFullscreen?.();
      }
    },
    togglePointerLock: async () => {
      if (document.pointerLockElement) {
        document.exitPointerLock?.();
      } else {
        document.body.requestPointerLock?.();
      }
    },
    getSettings: async () => readSettings(),
    setSetting: async (key, value) => {
      const settings = readSettings();
      writeSettings({ ...settings, [key]: value } as Settings);
    },
    resetSettings: async () => {
      writeSettings(WEBOS_DEFAULT_SETTINGS);
      return readSettings();
    },
    getMicrophonePermission: async (): Promise<MicrophonePermissionResult> => ({
      platform: "unknown",
      isMacOs: false,
      status: "not-applicable",
      granted: true,
      canRequest: true,
      shouldUseBrowserApi: true,
    }),
    exportLogs: async () => "webOS renderer logs are available through the webOS inspector.",
    pingRegions: async (regions: StreamRegion[]): Promise<PingResult[]> =>
      callService<PingResult[]>("pingRegions", { input: regions }),
    saveScreenshot: async ({ dataUrl }) => {
      const now = Date.now();
      const entry: ScreenshotEntry = {
        id: `webos-${now}`,
        fileName: `OpenNOW-${now}.png`,
        filePath: `localStorage://${now}`,
        createdAtMs: now,
        sizeBytes: dataUrl.length,
        dataUrl,
      };
      persistScreenshots([entry, ...listScreenshots()]);
      return entry;
    },
    listScreenshots: async () => listScreenshots(),
    deleteScreenshot: async ({ id }) => {
      persistScreenshots(listScreenshots().filter((entry) => entry.id !== id));
    },
    saveScreenshotAs: async () => ({ saved: false }),
    onTriggerScreenshot: () => () => undefined,
    beginRecording: () => unsupported<RecordingBeginResult>("Stream recording"),
    sendRecordingChunk: () => unsupported<void>("Stream recording"),
    finishRecording: () => unsupported<RecordingEntry>("Stream recording"),
    abortRecording: () => unsupported<void>("Stream recording"),
    listRecordings: async () => [],
    deleteRecording: async () => undefined,
    showRecordingInFolder: () => unsupported<void>("Opening recordings in a folder"),
    listMediaByGame: async (input = {}): Promise<MediaListingResult> => {
      const gameTitle = input.gameTitle?.trim().toLowerCase();
      const screenshots = listScreenshots()
        .filter(() => !gameTitle)
        .map((entry) => ({ ...entry }));
      return { screenshots, videos: [] };
    },
    getMediaThumbnail: async () => null,
    showMediaInFolder: () => unsupported<void>("Opening media in a folder"),
    deleteCache: async () => {
      localStorage.removeItem(SCREENSHOT_STORAGE_KEY);
    },
    fetchPrintedWasteQueue: (): Promise<PrintedWasteQueueData> =>
      callService<PrintedWasteQueueData>("fetchPrintedWasteQueue", {}).catch(() => fetchJson<PrintedWasteQueueData>("https://api.printedwaste.com/gfn/queue/")),
    fetchPrintedWasteServerMapping: (): Promise<PrintedWasteServerMapping> =>
      callService<PrintedWasteServerMapping>("fetchPrintedWasteServerMapping", {}).catch(() => fetchJson<PrintedWasteServerMapping>("https://remote.printedwaste.com/config/GFN_SERVERID_TO_REGION_MAPPING")),
    getThanksData: () => callService<ThankYouDataResult>("getThanksData", {}),
  };
}

export function installWebOsRuntime(): void {
  if (!WEBOS_RUNTIME || window.openNow) {
    return;
  }

  window.openNow = createWebOsApi();
}
