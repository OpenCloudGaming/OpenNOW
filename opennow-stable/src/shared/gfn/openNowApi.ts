import type {
  AuthLoginRequest,
  AuthSession,
  AuthSessionRequest,
  AuthSessionResult,
  LoginProvider,
  SavedAccount,
  SubscriptionInfo,
  ThankYouDataResult,
} from "./auth";
import type {
  CatalogBrowseRequest,
  CatalogBrowseResult,
  GameInfo,
  GamesFetchRequest,
  PingResult,
  RegionsFetchRequest,
  ResolveLaunchIdRequest,
  StreamRegion,
  SubscriptionFetchRequest,
} from "./regionsAndGames";
import type {
  ActiveSessionInfo,
  IceCandidatePayload,
  KeyframeRequest,
  MainToRendererSignalingEvent,
  SendAnswerRequest,
  SessionClaimRequest,
  SessionConflictChoice,
  SessionCreateRequest,
  SessionInfo,
  SessionPollRequest,
  SessionStopRequest,
  SessionAdReportRequest,
  SignalingConnectRequest,
} from "./session";
import type { AppUpdaterState } from "./updater";
import type { Settings, MicrophonePermissionResult } from "./microphoneSettings";
import type {
  MediaListingResult,
  RecordingAbortRequest,
  RecordingBeginRequest,
  RecordingBeginResult,
  RecordingChunkRequest,
  RecordingDeleteRequest,
  RecordingEntry,
  RecordingFinishRequest,
  ScreenshotDeleteRequest,
  ScreenshotEntry,
  ScreenshotSaveAsRequest,
  ScreenshotSaveAsResult,
  ScreenshotSaveRequest,
} from "./media";
import type { PrintedWasteQueueData, PrintedWasteServerMapping } from "./printedWaste";

export interface OpenNowApi {
  getAuthSession(input?: AuthSessionRequest): Promise<AuthSessionResult>;
  getLoginProviders(): Promise<LoginProvider[]>;
  getRegions(input?: RegionsFetchRequest): Promise<StreamRegion[]>;
  login(input: AuthLoginRequest): Promise<AuthSession>;
  logout(): Promise<void>;
  logoutAll(): Promise<void>;
  getSavedAccounts(): Promise<SavedAccount[]>;
  switchAccount(userId: string): Promise<AuthSession>;
  removeAccount(userId: string): Promise<void>;
  fetchSubscription(input: SubscriptionFetchRequest): Promise<SubscriptionInfo>;
  fetchMainGames(input: GamesFetchRequest): Promise<GameInfo[]>;
  fetchLibraryGames(input: GamesFetchRequest): Promise<GameInfo[]>;
  browseCatalog(input: CatalogBrowseRequest): Promise<CatalogBrowseResult>;
  fetchPublicGames(): Promise<GameInfo[]>;
  resolveLaunchAppId(input: ResolveLaunchIdRequest): Promise<string | null>;
  createSession(input: SessionCreateRequest): Promise<SessionInfo>;
  pollSession(input: SessionPollRequest): Promise<SessionInfo>;
  reportSessionAd(input: SessionAdReportRequest): Promise<SessionInfo>;
  stopSession(input: SessionStopRequest): Promise<void>;
  /** Get list of active sessions (status 2 or 3) */
  getActiveSessions(token?: string, streamingBaseUrl?: string): Promise<ActiveSessionInfo[]>;
  /** Claim/resume an existing session */
  claimSession(input: SessionClaimRequest): Promise<SessionInfo>;
  /** Show dialog asking user how to handle session conflict */
  showSessionConflictDialog(): Promise<SessionConflictChoice>;
  connectSignaling(input: SignalingConnectRequest): Promise<void>;
  disconnectSignaling(): Promise<void>;
  sendAnswer(input: SendAnswerRequest): Promise<void>;
  sendIceCandidate(input: IceCandidatePayload): Promise<void>;
  requestKeyframe(input: KeyframeRequest): Promise<void>;
  onSignalingEvent(listener: (event: MainToRendererSignalingEvent) => void): () => void;
  /** Listen for F11 fullscreen toggle from main process */
  onToggleFullscreen(listener: () => void): () => void;
  quitApp(): Promise<void>;
  getUpdaterState(): Promise<AppUpdaterState>;
  checkForUpdates(): Promise<AppUpdaterState>;
  downloadUpdate(): Promise<AppUpdaterState>;
  installUpdateAndRestart(): Promise<AppUpdaterState>;
  onUpdaterStateChanged(listener: (state: AppUpdaterState) => void): () => void;
  setFullscreen(v: boolean): Promise<void>;
  toggleFullscreen(): Promise<void>;
  togglePointerLock(): Promise<void>;
  getSettings(): Promise<Settings>;
  setSetting<K extends keyof Settings>(key: K, value: Settings[K]): Promise<void>;
  resetSettings(): Promise<Settings>;
  getMicrophonePermission(): Promise<MicrophonePermissionResult>;
  /** Export logs in redacted format */
  exportLogs(format?: "text" | "json"): Promise<string>;
  /** Ping all regions and return latency results */
  pingRegions(regions: StreamRegion[]): Promise<PingResult[]>;

  /** Persist a PNG screenshot from a renderer-generated data URL */
  saveScreenshot(input: ScreenshotSaveRequest): Promise<ScreenshotEntry>;

  /** List recent screenshots from the persistent screenshot directory */
  listScreenshots(): Promise<ScreenshotEntry[]>;

  /** Delete a screenshot from the persistent screenshot directory */
  deleteScreenshot(input: ScreenshotDeleteRequest): Promise<void>;

  /** Export a screenshot to a user-selected path */
  saveScreenshotAs(input: ScreenshotSaveAsRequest): Promise<ScreenshotSaveAsResult>;

  /** Begin a new recording session; returns a recordingId to use for subsequent calls */
  beginRecording(input: RecordingBeginRequest): Promise<RecordingBeginResult>;

  /** Stream a chunk of recorded video data to the main process */
  sendRecordingChunk(input: RecordingChunkRequest): Promise<void>;

  /** Finalise a recording; saves the video and optional thumbnail to disk */
  finishRecording(input: RecordingFinishRequest): Promise<RecordingEntry>;

  /** Abort an in-progress recording and remove the temporary file */
  abortRecording(input: RecordingAbortRequest): Promise<void>;

  /** List all saved recordings from the recordings directory */
  listRecordings(): Promise<RecordingEntry[]>;

  /** Delete a saved recording (and its thumbnail if present) */
  deleteRecording(input: RecordingDeleteRequest): Promise<void>;

  /** Reveal a saved recording in the system file manager */
  showRecordingInFolder(id: string): Promise<void>;

  /** List screenshot and recording media, optionally filtered by game title */
  listMediaByGame(input?: { gameTitle?: string }): Promise<MediaListingResult>;

  /** Resolve a thumbnail data URL for a media file path */
  getMediaThumbnail(input: { filePath: string }): Promise<string | null>;

  /** Reveal a media file path in the system file manager */
  showMediaInFolder(input: { filePath: string }): Promise<void>;

  deleteCache(): Promise<void>;

  /** Trigger a background cache refresh (metadata/images) without deleting cache */
  refreshCache(): Promise<void>;

  /** Fetch current GFN queue wait times from the PrintedWaste API */
  fetchPrintedWasteQueue(): Promise<PrintedWasteQueueData>;
  /** Fetch PrintedWaste server mapping metadata (includes nuked status) */
  fetchPrintedWasteServerMapping(): Promise<PrintedWasteServerMapping>;
  getThanksData(): Promise<ThankYouDataResult>;
  /** Clear Discord rich presence activity */
  clearDiscordActivity(): Promise<void>;
}
