import type { NativeCloudGsyncCapabilities } from "../cloudGsync";
import type {
  AuthDeviceLoginAttemptRequest,
  AuthDeviceLoginChallenge,
  AuthDeviceLoginPollRequest,
  AuthDeviceLoginPollResult,
  AuthDeviceLoginStartRequest,
  AuthLoginRequest,
  AuthSession,
  AuthSessionRequest,
  AuthSessionResult,
  LoginProvider,
  SavedAccount,
  ConsolePinClearRequest,
  ConsolePinMutationResult,
  ConsolePinSetRequest,
  ConsolePinStatus,
  ConsolePinVerifyRequest,
  ConsolePinVerifyResult,
} from "./auth";
import type {
  CatalogBrowseRequest,
  CatalogBrowseResult,
  DirectLaunchRequest,
  GameAccountConnectionsResult,
  GameAccountOperationRequest,
  GameAccountOperationResult,
  GameInfo,
  GamePanelResult,
  GamesFetchRequest,
  MarkGameOwnedRequest,
  MarkGameOwnedResult,
  PersistentStorageLocationsFetchRequest,
  PersistentStorageLocationsResult,
  PersistentStorageResetRequest,
  PersistentStorageResetResult,
  PingResult,
  RegionsFetchRequest,
  ResolveLaunchIdRequest,
  ResolveStoreUrlRequest,
  StreamRegion,
  SubscriptionFetchRequest,
} from "./catalog";
import type { MicrophonePermissionResult, Settings } from "./settings";
import type {
  ActiveSessionInfo,
  SessionAdReportRequest,
  SessionClaimRequest,
  SessionConflictChoice,
  SessionCreateRequest,
  SessionInfo,
  SessionPollRequest,
  SessionStopRequest,
} from "./session";
import type {
  IceCandidatePayload,
  KeyframeRequest,
  MainToRendererSignalingEvent,
  NativeInputPacket,
  NativeRenderSurfaceUpdate,
  StreamShortcutInterceptionGate,
  NativeStreamerShortcutAction,
  NativeStreamerShortcutBindings,
  SendAnswerRequest,
  SignalingConnectRequest,
} from "./signaling";
import type { NativeStreamerStatus } from "./nativeStreamer";
import type { SubscriptionInfo } from "./subscription";
import type { ThankYouDataResult } from "./thankYou";
import type { AppUpdaterState, ReleaseHighlightsPayload } from "./updater";
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
import type { DesktopBugReportReceipt, DesktopBugReportRequest } from "../bugReport";

export interface GpuBackendInfo {
  gpuName: string | null;
  vendorName: string | null;
  driverVersion: string | null;
  decodeAccelerated: boolean | null;
  encodeAccelerated: boolean | null;
  hardwareDecodeCodecs: string[];
  hardwareEncodeCodecs: string[];
}

export interface OpenNowApi {
  getAuthSession(input?: AuthSessionRequest): Promise<AuthSessionResult>;
  getLoginProviders(): Promise<LoginProvider[]>;
  getRegions(input?: RegionsFetchRequest): Promise<StreamRegion[]>;
  login(input: AuthLoginRequest): Promise<AuthSession>;
  startDeviceLogin(input: AuthDeviceLoginStartRequest): Promise<AuthDeviceLoginChallenge>;
  pollDeviceLogin(input: AuthDeviceLoginPollRequest): Promise<AuthDeviceLoginPollResult>;
  completeDeviceLogin(input: AuthDeviceLoginAttemptRequest): Promise<AuthSession>;
  cancelDeviceLogin(input: AuthDeviceLoginAttemptRequest): Promise<void>;
  logout(): Promise<void>;
  logoutAll(): Promise<void>;
  getSavedAccounts(): Promise<SavedAccount[]>;
  switchAccount(userId: string, pin?: string): Promise<AuthSession>;
  removeAccount(userId: string): Promise<void>;
  getConsolePinStatus(userId: string): Promise<ConsolePinStatus>;
  setConsolePin(input: ConsolePinSetRequest): Promise<ConsolePinMutationResult>;
  clearConsolePin(input: ConsolePinClearRequest): Promise<ConsolePinMutationResult>;
  verifyConsolePin(input: ConsolePinVerifyRequest): Promise<ConsolePinVerifyResult>;
  fetchSubscription(input: SubscriptionFetchRequest): Promise<SubscriptionInfo>;
  fetchPersistentStorageLocations(input?: PersistentStorageLocationsFetchRequest): Promise<PersistentStorageLocationsResult>;
  resetPersistentStorage(input?: PersistentStorageResetRequest): Promise<PersistentStorageResetResult>;
  fetchGameAccountConnections(): Promise<GameAccountConnectionsResult>;
  linkGameAccount(input: GameAccountOperationRequest): Promise<GameAccountOperationResult>;
  unlinkGameAccount(input: GameAccountOperationRequest): Promise<GameAccountOperationResult>;
  resyncGameAccount(input: GameAccountOperationRequest): Promise<GameAccountOperationResult>;
  fetchMainGames(input: GamesFetchRequest): Promise<GameInfo[]>;
  fetchStorePanels(input: GamesFetchRequest): Promise<GamePanelResult[]>;
  fetchFeaturedGames(input: GamesFetchRequest): Promise<GameInfo[]>;
  fetchLibraryGames(input: GamesFetchRequest): Promise<GameInfo[]>;
  browseCatalog(input: CatalogBrowseRequest): Promise<CatalogBrowseResult>;
  fetchPublicGames(): Promise<GameInfo[]>;
  resolveLaunchAppId(input: ResolveLaunchIdRequest): Promise<string | null>;
  resolveStoreUrl(input: ResolveStoreUrlRequest): Promise<string | null>;
  markGameOwned(input: MarkGameOwnedRequest): Promise<MarkGameOwnedResult>;
  getPendingDirectLaunchRequest(): Promise<DirectLaunchRequest | null>;
  onDirectLaunchRequest(listener: (request: DirectLaunchRequest) => void): () => void;
  createSession(input: SessionCreateRequest): Promise<SessionInfo>;
  pollSession(input: SessionPollRequest): Promise<SessionInfo>;
  reportSessionAd(input: SessionAdReportRequest): Promise<SessionInfo>;
  stopSession(input: SessionStopRequest): Promise<void>;
  /** Get list of active sessions (status 2 or 3) */
  getActiveSessions(token?: string, streamingBaseUrl?: string): Promise<ActiveSessionInfo[]>;
  /** Claim/resume an existing session */
  claimSession(input: SessionClaimRequest): Promise<SessionInfo>;
  getNativeStreamerStatus(): Promise<NativeStreamerStatus>;
  getNativeCloudGsyncCapabilities(): Promise<NativeCloudGsyncCapabilities>;
  /** Show dialog asking user how to handle session conflict */
  showSessionConflictDialog(): Promise<SessionConflictChoice>;
  connectSignaling(input: SignalingConnectRequest): Promise<void>;
  disconnectSignaling(): Promise<void>;
  sendAnswer(input: SendAnswerRequest): Promise<void>;
  sendIceCandidate(input: IceCandidatePayload): Promise<void>;
  sendNativeInput(input: NativeInputPacket): void;
  setNativeInputPaused(paused: boolean): void;
  updateNativeRenderSurface(input: NativeRenderSurfaceUpdate): void;
  updateNativeShortcuts(shortcuts: NativeStreamerShortcutBindings): void;
  requestKeyframe(input: KeyframeRequest): Promise<void>;
  onSignalingEvent(listener: (event: MainToRendererSignalingEvent) => void): () => void;
  /** Listen for the fullscreen toggle from the main process */
  onToggleFullscreen(listener: () => void): () => void;
  onExitFullscreen(listener: () => void): () => void;
  quitApp(): Promise<void>;
  getUpdaterState(): Promise<AppUpdaterState>;
  checkForUpdates(): Promise<AppUpdaterState>;
  downloadUpdate(): Promise<AppUpdaterState>;
  installUpdateAndRestart(): Promise<AppUpdaterState>;
  onUpdaterStateChanged(listener: (state: AppUpdaterState) => void): () => void;
  setFullscreen(v: boolean): Promise<void>;
  toggleFullscreen(): Promise<void>;
  togglePointerLock(): Promise<void>;
  /** Notify main process that pointer lock state changed (active = true/false). */
  notifyPointerLockChange(active: boolean, suppressEscapeFullscreenGrace?: boolean): void;
  /** Tell main whether an active native session owns keyboard input through RawInput. */
  notifyNativeInputModeChange(active: boolean, rawInputOwnsEscape: boolean): void;
  /** Read plain text from the OS clipboard through Electron main process */
  readClipboardText(): Promise<string>;
  getSettings(): Promise<Settings>;
  setSetting<K extends keyof Settings>(key: K, value: Settings[K]): Promise<void>;
  getGpuInfo(): Promise<GpuBackendInfo>;
  resetSettings(): Promise<Settings>;
  selectNativeStreamerExecutable(): Promise<string | null>;
  getMicrophonePermission(): Promise<MicrophonePermissionResult>;
  /** Export logs in redacted format */
  exportLogs(format?: "text" | "json"): Promise<string>;
  /** Submit an explicit, reviewed bug report through the main-process upload owner. */
  submitBugReport(input: DesktopBugReportRequest): Promise<DesktopBugReportReceipt>;
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

  /** Listen for screenshot hotkey events from the main process */
  onTriggerScreenshot(listener: () => void): () => void;

  /** Listen for external Escape events forwarded by the main process */
  onExternalEscape(listener: () => void): () => void;

  /** Listen for stream shortcuts captured before Chromium handles reserved combinations. */
  onStreamShortcutAction(listener: (action: NativeStreamerShortcutAction) => void): () => void;
  setStreamShortcutInterceptionGate(gate: StreamShortcutInterceptionGate): void;

  /** Open a trusted external URL in the OS default browser */
  openExternalUrl(url: string): Promise<void>;

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

  /** Trusted file:// URL for in-app playback of a video under OpenNOW media root, or null */
  getMediaPlaybackUrl(input: { filePath: string }): Promise<string | null>;

  /** Delete a media file under the OpenNOW pictures root (recordings, screenshots, etc.) */
  deleteMediaFile(input: { filePath: string }): Promise<{ ok: boolean }>;

  /** Invalidate cached / sidecar thumbnails and regenerate (returns data URL when possible) */
  regenMediaThumbnail(input: { filePath: string }): Promise<{ ok: boolean; thumbnailDataUrl: string | null }>;

  deleteCache(): Promise<void>;

  /** Fetch current GFN queue wait times from the PrintedWaste API */
  fetchPrintedWasteQueue(): Promise<PrintedWasteQueueData>;
  /** Fetch PrintedWaste server mapping metadata (includes nuked status) */
  fetchPrintedWasteServerMapping(): Promise<PrintedWasteServerMapping>;
  getThanksData(): Promise<ThankYouDataResult>;
  provisionZortosCommunityProxy(): Promise<import("@shared/communityProxy").CommunityProxyProvisionResult>;
  /** Set Discord rich presence activity */
  setDiscordActivity(input: import("../discord").DiscordActivityUpdate): Promise<void>;
  /** Clear Discord rich presence activity */
  clearDiscordActivity(): Promise<void>;

  /** Fetch release highlights payload for a given version (defaults to current) */
  getReleaseHighlights(version?: string): Promise<ReleaseHighlightsPayload>;

  /** Mark the current version's highlights as acknowledged */
  ackReleaseHighlights(): Promise<void>;

  /** Subscribe to automatic release-highlights show events from main process */
  onReleaseHighlightsShow(listener: (payload: ReleaseHighlightsPayload) => void): () => void;
}
