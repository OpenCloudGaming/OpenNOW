import electron from "electron";

import { IPC_CHANNELS } from "@shared/ipc";
import type {
  AuthLoginRequest,
  AuthSession,
  AuthSessionRequest,
  GamesFetchRequest,
  CatalogBrowseRequest,
  ResolveLaunchIdRequest,
  RegionsFetchRequest,
  MainToRendererSignalingEvent,
  OpenNowApi,
  SavedAccount,
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
  StreamRegion,
  ScreenshotSaveRequest,
  ScreenshotDeleteRequest,
  ScreenshotSaveAsRequest,
  RecordingBeginRequest,
  RecordingBeginResult,
  RecordingChunkRequest,
  RecordingFinishRequest,
  RecordingAbortRequest,
  RecordingEntry,
  RecordingDeleteRequest,
  MediaListingResult,
  PrintedWasteQueueData,
  PrintedWasteServerMapping,
  ThankYouDataResult,
  AppUpdaterState,
} from "@shared/gfn";
import { parseSerializedSessionErrorTransport } from "@shared/sessionError";

const { contextBridge, ipcRenderer } = electron;

function unwrapSessionInvokeError(error: unknown): never {
  if (error instanceof Error) {
    const sessionError = parseSerializedSessionErrorTransport(error.message);
    if (sessionError) {
      throw sessionError;
    }
  }

  throw error;
}

function invokeSessionChannel<T>(channel: string, ...args: unknown[]): Promise<T> {
  return ipcRenderer.invoke(channel, ...args).catch((error: unknown) => unwrapSessionInvokeError(error)) as Promise<T>;
}

const api: OpenNowApi = {
  getAuthSession: (input: AuthSessionRequest = {}) => ipcRenderer.invoke(IPC_CHANNELS.AUTH_GET_SESSION, input),
  getLoginProviders: () => ipcRenderer.invoke(IPC_CHANNELS.AUTH_GET_PROVIDERS),
  getRegions: (input: RegionsFetchRequest = {}) => ipcRenderer.invoke(IPC_CHANNELS.AUTH_GET_REGIONS, input),
  login: (input: AuthLoginRequest) => ipcRenderer.invoke(IPC_CHANNELS.AUTH_LOGIN, input),
  logout: () => ipcRenderer.invoke(IPC_CHANNELS.AUTH_LOGOUT),
  logoutAll: () => ipcRenderer.invoke(IPC_CHANNELS.AUTH_LOGOUT_ALL),
  getSavedAccounts: (): Promise<SavedAccount[]> => ipcRenderer.invoke(IPC_CHANNELS.AUTH_GET_SAVED_ACCOUNTS),
  switchAccount: (userId: string): Promise<AuthSession> =>
    ipcRenderer.invoke(IPC_CHANNELS.AUTH_SWITCH_ACCOUNT, userId),
  removeAccount: (userId: string): Promise<void> => ipcRenderer.invoke(IPC_CHANNELS.AUTH_REMOVE_ACCOUNT, userId),
  fetchSubscription: (input: SubscriptionFetchRequest) =>
    ipcRenderer.invoke(IPC_CHANNELS.SUBSCRIPTION_FETCH, input),
  fetchMainGames: (input: GamesFetchRequest) => ipcRenderer.invoke(IPC_CHANNELS.GAMES_FETCH_MAIN, input),
  fetchLibraryGames: (input: GamesFetchRequest) =>
    ipcRenderer.invoke(IPC_CHANNELS.GAMES_FETCH_LIBRARY, input),
  browseCatalog: (input: CatalogBrowseRequest) => ipcRenderer.invoke(IPC_CHANNELS.GAMES_BROWSE_CATALOG, input),
  fetchPublicGames: () => ipcRenderer.invoke(IPC_CHANNELS.GAMES_FETCH_PUBLIC),
  resolveLaunchAppId: (input: ResolveLaunchIdRequest) =>
    ipcRenderer.invoke(IPC_CHANNELS.GAMES_RESOLVE_LAUNCH_ID, input),
  createSession: (input: SessionCreateRequest) => invokeSessionChannel(IPC_CHANNELS.CREATE_SESSION, input),
  pollSession: (input: SessionPollRequest) => invokeSessionChannel(IPC_CHANNELS.POLL_SESSION, input),
  reportSessionAd: (input: SessionAdReportRequest) => invokeSessionChannel(IPC_CHANNELS.REPORT_SESSION_AD, input),
  stopSession: (input: SessionStopRequest) => invokeSessionChannel(IPC_CHANNELS.STOP_SESSION, input),
  getActiveSessions: (token?: string, streamingBaseUrl?: string) =>
    ipcRenderer.invoke(IPC_CHANNELS.GET_ACTIVE_SESSIONS, token, streamingBaseUrl),
  claimSession: (input: SessionClaimRequest) => invokeSessionChannel(IPC_CHANNELS.CLAIM_SESSION, input),
  showSessionConflictDialog: () => ipcRenderer.invoke(IPC_CHANNELS.SESSION_CONFLICT_DIALOG),
  connectSignaling: (input: SignalingConnectRequest) =>
    ipcRenderer.invoke(IPC_CHANNELS.CONNECT_SIGNALING, input),
  disconnectSignaling: () => ipcRenderer.invoke(IPC_CHANNELS.DISCONNECT_SIGNALING),
  sendAnswer: (input: SendAnswerRequest) => ipcRenderer.invoke(IPC_CHANNELS.SEND_ANSWER, input),
  sendIceCandidate: (input: IceCandidatePayload) =>
    ipcRenderer.invoke(IPC_CHANNELS.SEND_ICE_CANDIDATE, input),
  requestKeyframe: (input: KeyframeRequest) =>
    ipcRenderer.invoke(IPC_CHANNELS.REQUEST_KEYFRAME, input),
  onSignalingEvent: (listener: (event: MainToRendererSignalingEvent) => void) => {
    const wrapped = (_event: Electron.IpcRendererEvent, payload: MainToRendererSignalingEvent) => {
      listener(payload);
    };

    ipcRenderer.on(IPC_CHANNELS.SIGNALING_EVENT, wrapped);
    return () => {
      ipcRenderer.off(IPC_CHANNELS.SIGNALING_EVENT, wrapped);
    };
  },
  onToggleFullscreen: (listener: () => void) => {
    const wrapped = () => listener();
    ipcRenderer.on("app:toggle-fullscreen", wrapped);
    return () => {
      ipcRenderer.off("app:toggle-fullscreen", wrapped);
    };
  },
  quitApp: () => ipcRenderer.invoke(IPC_CHANNELS.QUIT_APP),
  getUpdaterState: (): Promise<AppUpdaterState> => ipcRenderer.invoke(IPC_CHANNELS.APP_UPDATER_GET_STATE),
  checkForUpdates: (): Promise<AppUpdaterState> => ipcRenderer.invoke(IPC_CHANNELS.APP_UPDATER_CHECK),
  downloadUpdate: (): Promise<AppUpdaterState> => ipcRenderer.invoke(IPC_CHANNELS.APP_UPDATER_DOWNLOAD),
  installUpdateAndRestart: (): Promise<AppUpdaterState> => ipcRenderer.invoke(IPC_CHANNELS.APP_UPDATER_INSTALL),
  onUpdaterStateChanged: (listener: (state: AppUpdaterState) => void) => {
    const wrapped = (_event: Electron.IpcRendererEvent, payload: AppUpdaterState) => {
      listener(payload);
    };

    ipcRenderer.on(IPC_CHANNELS.APP_UPDATER_STATE_CHANGED, wrapped);
    return () => {
      ipcRenderer.off(IPC_CHANNELS.APP_UPDATER_STATE_CHANGED, wrapped);
    };
  },
  toggleFullscreen: () => ipcRenderer.invoke(IPC_CHANNELS.TOGGLE_FULLSCREEN),
  setFullscreen: (v: boolean) => ipcRenderer.invoke(IPC_CHANNELS.SET_FULLSCREEN, v),
  togglePointerLock: () => ipcRenderer.invoke(IPC_CHANNELS.TOGGLE_POINTER_LOCK),
  getSettings: () => ipcRenderer.invoke(IPC_CHANNELS.SETTINGS_GET),
  setSetting: <K extends keyof Settings>(key: K, value: Settings[K]) =>
    ipcRenderer.invoke(IPC_CHANNELS.SETTINGS_SET, key, value),
  resetSettings: () => ipcRenderer.invoke(IPC_CHANNELS.SETTINGS_RESET),
  notifyPointerLockChange: (active: boolean) => ipcRenderer.send(IPC_CHANNELS.POINTER_LOCK_CHANGE, active),
  onExternalEscape: (listener: () => void) => {
    const wrapped = () => listener();
    ipcRenderer.on(IPC_CHANNELS.EXTERNAL_ESCAPE, wrapped);
    return () => ipcRenderer.off(IPC_CHANNELS.EXTERNAL_ESCAPE, wrapped);
  },
  getMicrophonePermission: () => ipcRenderer.invoke(IPC_CHANNELS.MICROPHONE_PERMISSION_GET),
  exportLogs: (format?: "text" | "json") => ipcRenderer.invoke(IPC_CHANNELS.LOGS_EXPORT, format),
  pingRegions: (regions: StreamRegion[]) => ipcRenderer.invoke(IPC_CHANNELS.PING_REGIONS, regions),
  saveScreenshot: (input: ScreenshotSaveRequest) => ipcRenderer.invoke(IPC_CHANNELS.SCREENSHOT_SAVE, input),
  listScreenshots: () => ipcRenderer.invoke(IPC_CHANNELS.SCREENSHOT_LIST),
  deleteScreenshot: (input: ScreenshotDeleteRequest) => ipcRenderer.invoke(IPC_CHANNELS.SCREENSHOT_DELETE, input),
  saveScreenshotAs: (input: ScreenshotSaveAsRequest) => ipcRenderer.invoke(IPC_CHANNELS.SCREENSHOT_SAVE_AS, input),
  onTriggerScreenshot: (listener: () => void) => {
    const wrapped = () => listener();
    ipcRenderer.on("app:trigger-screenshot", wrapped);
    return () => {
      ipcRenderer.off("app:trigger-screenshot", wrapped);
    };
  },
  beginRecording: (input: RecordingBeginRequest): Promise<RecordingBeginResult> =>
    ipcRenderer.invoke(IPC_CHANNELS.RECORDING_BEGIN, input),
  sendRecordingChunk: (input: RecordingChunkRequest): Promise<void> =>
    ipcRenderer.invoke(IPC_CHANNELS.RECORDING_CHUNK, input),
  finishRecording: (input: RecordingFinishRequest): Promise<RecordingEntry> =>
    ipcRenderer.invoke(IPC_CHANNELS.RECORDING_FINISH, input),
  abortRecording: (input: RecordingAbortRequest): Promise<void> =>
    ipcRenderer.invoke(IPC_CHANNELS.RECORDING_ABORT, input),
  listRecordings: (): Promise<RecordingEntry[]> =>
    ipcRenderer.invoke(IPC_CHANNELS.RECORDING_LIST),
  deleteRecording: (input: RecordingDeleteRequest): Promise<void> =>
    ipcRenderer.invoke(IPC_CHANNELS.RECORDING_DELETE, input),
  showRecordingInFolder: (id: string): Promise<void> =>
    ipcRenderer.invoke(IPC_CHANNELS.RECORDING_SHOW_IN_FOLDER, id),
  listMediaByGame: (input: { gameTitle?: string } = {}): Promise<MediaListingResult> =>
    ipcRenderer.invoke(IPC_CHANNELS.MEDIA_LIST_BY_GAME, input),
  getMediaThumbnail: (input: { filePath: string }) => ipcRenderer.invoke(IPC_CHANNELS.MEDIA_THUMBNAIL, input),
  showMediaInFolder: (input: { filePath: string }): Promise<void> =>
    ipcRenderer.invoke(IPC_CHANNELS.MEDIA_SHOW_IN_FOLDER, input),
  getMediaPlaybackUrl: (input: { filePath: string }): Promise<string | null> =>
    ipcRenderer.invoke(IPC_CHANNELS.MEDIA_PLAYBACK_URL, input),
  deleteMediaFile: (input: { filePath: string }): Promise<{ ok: boolean }> =>
    ipcRenderer.invoke(IPC_CHANNELS.MEDIA_DELETE_FILE, input),
  regenMediaThumbnail: (input: { filePath: string }): Promise<{ ok: boolean; thumbnailDataUrl: string | null }> =>
    ipcRenderer.invoke(IPC_CHANNELS.MEDIA_REGEN_THUMBNAIL, input),
  deleteCache: (): Promise<void> =>
    ipcRenderer.invoke(IPC_CHANNELS.CACHE_DELETE_ALL),
  fetchPrintedWasteQueue: (): Promise<PrintedWasteQueueData> =>
    ipcRenderer.invoke(IPC_CHANNELS.PRINTEDWASTE_QUEUE_FETCH),
  fetchPrintedWasteServerMapping: (): Promise<PrintedWasteServerMapping> =>
    ipcRenderer.invoke(IPC_CHANNELS.PRINTEDWASTE_SERVER_MAPPING_FETCH),
  getThanksData: (): Promise<ThankYouDataResult> => ipcRenderer.invoke(IPC_CHANNELS.COMMUNITY_GET_THANKS),
  clearDiscordActivity: (): Promise<void> => ipcRenderer.invoke(IPC_CHANNELS.DISCORD_CLEAR_ACTIVITY),
};

contextBridge.exposeInMainWorld("openNow", api);
