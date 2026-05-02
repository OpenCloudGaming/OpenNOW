import type { BrowserWindow } from "electron";
import type { WriteStream } from "node:fs";
import type { AuthService } from "../gfn/auth";
import type { GfnSignalingClient } from "../gfn/signaling";
import type { AppUpdaterController } from "../updater";
import type { SettingsManager } from "../settings";
import type {
  ActiveSessionInfo,
  AppUpdaterState,
  AuthLoginRequest,
  AuthSessionRequest,
  CatalogBrowseRequest,
  ExistingSessionStrategy,
  GamesFetchRequest,
  IceCandidatePayload,
  KeyframeRequest,
  MainToRendererSignalingEvent,
  PingResult,
  RecordingAbortRequest,
  RecordingBeginRequest,
  RecordingBeginResult,
  RecordingChunkRequest,
  RecordingDeleteRequest,
  RecordingEntry,
  RecordingFinishRequest,
  RegionsFetchRequest,
  ResolveLaunchIdRequest,
  ScreenshotDeleteRequest,
  ScreenshotEntry,
  ScreenshotSaveAsRequest,
  ScreenshotSaveAsResult,
  ScreenshotSaveRequest,
  SendAnswerRequest,
  SessionAdReportRequest,
  SessionClaimRequest,
  SessionConflictChoice,
  SessionCreateRequest,
  SessionInfo,
  SessionPollRequest,
  SessionStopRequest,
  Settings,
  SignalingConnectRequest,
  StreamRegion,
  SubscriptionFetchRequest,
  ThankYouDataResult,
} from "@shared/gfn";
import type * as net from "node:net";

/** In-flight recording write state (IPC layer). */
export interface ActiveRecording {
  writeStream: WriteStream;
  tempPath: string;
  mimeType: string;
}

export interface SignalingState {
  client: GfnSignalingClient | null;
  key: string | null;
}

export interface MainIpcDeps {
  ipcMain: Electron.IpcMain;
  app: Electron.App;
  dialog: Electron.Dialog;
  shell: Electron.Shell;
  systemPreferences: Electron.SystemPreferences;
  getMainWindow: () => BrowserWindow | null;
  authService: AuthService;
  settingsManager: SettingsManager;
  appUpdater: AppUpdaterController | null;
  signaling: SignalingState;
  GfnSignalingClient: typeof GfnSignalingClient;
  emitToRenderer: (event: MainToRendererSignalingEvent) => void;
  emitUpdaterStateToRenderer: (state: AppUpdaterState) => void;
  requestAppShutdown: (options?: { reason?: string; forceExitFallback?: boolean; exitCode?: number }) => void;
  discordMonitor: { start: () => void; stop: () => void };
  refreshScheduler: { manualRefresh: () => Promise<void> };
  cacheManager: { deleteAll: () => Promise<void> };
  resolveJwt: (token?: string) => Promise<string>;
  rethrowSerializedSessionError: (error: unknown) => never;
  showSessionConflictDialog: () => Promise<SessionConflictChoice>;
  shouldForceNewSession: (strategy: ExistingSessionStrategy | undefined) => boolean;
  selectReadySessionToClaim: (activeSessions: ActiveSessionInfo[], numericAppId: number) => ActiveSessionInfo | null;
  selectLaunchingSession: (activeSessions: ActiveSessionInfo[], numericAppId: number) => ActiveSessionInfo | null;
  stopActiveSessionsForCreate: (params: {
    token: string;
    streamingBaseUrl: string;
    zone: string;
    appId: string;
  }) => Promise<void>;
  SCREENSHOT_LIMIT: number;
  RECORDING_LIMIT: number;
  activeRecordings: Map<string, ActiveRecording>;
  saveScreenshot: (input: ScreenshotSaveRequest) => Promise<ScreenshotEntry>;
  listScreenshots: () => Promise<ScreenshotEntry[]>;
  deleteScreenshot: (input: ScreenshotDeleteRequest) => Promise<void>;
  saveScreenshotAs: (input: ScreenshotSaveAsRequest) => Promise<ScreenshotSaveAsResult>;
  dataUrlToBuffer: (dataUrl: string) => { ext: "png" | "jpg" | "webp"; buffer: Buffer };
  sanitizeTitleForFileName: (value: string | undefined) => string;
  ensureRecordingsDirectory: () => Promise<string>;
  getRecordingsDirectory: () => string;
  listRecordings: () => Promise<RecordingEntry[]>;
  ensureThumbnailForMedia: (filePath: string) => Promise<string | null>;
  extFromMimeType: (mimeType: string) => ".mp4" | ".webm";
  fetchThanksData: () => Promise<ThankYouDataResult>;
  net: typeof net;
  exportLogs: (format?: "text" | "json") => Promise<string>;
}
