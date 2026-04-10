export type VideoCodec = "H264" | "H265" | "AV1";
export type VideoAccelerationPreference = "auto" | "hardware" | "software";

/** Color quality (bit depth + chroma subsampling), matching Rust ColorQuality enum */
export type ColorQuality = "8bit_420" | "8bit_444" | "10bit_420" | "10bit_444";

/** Game language codes for in-game localization (sent to GFN servers) */
export type GameLanguage =
  | "en_US" | "en_GB" | "de_DE" | "fr_FR" | "es_ES" | "es_MX" | "it_IT"
  | "pt_PT" | "pt_BR" | "ru_RU" | "pl_PL" | "tr_TR" | "ar_SA" | "ja_JP"
  | "ko_KR" | "zh_CN" | "zh_TW" | "th_TH" | "vi_VN" | "id_ID" | "cs_CZ"
  | "el_GR" | "hu_HU" | "ro_RO" | "uk_UA" | "nl_NL" | "sv_SE" | "da_DK"
  | "fi_FI" | "no_NO";

/** Keyboard layout codes for physical key mapping in remote sessions */
export type KeyboardLayout =
  | "en-US" | "en-GB" | "tr-TR" | "de-DE" | "fr-FR" | "es-ES" | "es-MX" | "it-IT"
  | "pt-PT" | "pt-BR" | "pl-PL" | "ru-RU" | "ja-JP" | "ko-KR" | "zh-CN" | "zh-TW";

export interface KeyboardLayoutOption {
  value: KeyboardLayout;
  label: string;
  macValue?: string;
}

export const DEFAULT_KEYBOARD_LAYOUT: KeyboardLayout = "en-US";

export const keyboardLayoutOptions: readonly KeyboardLayoutOption[] = [
  { value: "en-US", label: "English (US)", macValue: "m-us" },
  { value: "en-GB", label: "English (UK)", macValue: "m-brit" },
  { value: "tr-TR", label: "Turkish Q", macValue: "m-tr-qty" },
  { value: "de-DE", label: "German" },
  { value: "fr-FR", label: "French" },
  { value: "es-ES", label: "Spanish" },
  { value: "es-MX", label: "Spanish (Latin America)" },
  { value: "it-IT", label: "Italian" },
  { value: "pt-PT", label: "Portuguese (Portugal)" },
  { value: "pt-BR", label: "Portuguese (Brazil)" },
  { value: "pl-PL", label: "Polish" },
  { value: "ru-RU", label: "Russian" },
  { value: "ja-JP", label: "Japanese" },
  { value: "ko-KR", label: "Korean" },
  { value: "zh-CN", label: "Chinese (Simplified)" },
  { value: "zh-TW", label: "Chinese (Traditional)" },
] as const;

export function resolveGfnKeyboardLayout(layout: KeyboardLayout, platform: string): string {
  const option = keyboardLayoutOptions.find((candidate) => candidate.value === layout);
  if (platform === "darwin" && option?.macValue) {
    return option.macValue;
  }
  return option?.value ?? DEFAULT_KEYBOARD_LAYOUT;
}

/** Helper: get CloudMatch bitDepth value (0 = 8-bit SDR, 10 = 10-bit HDR capable) */
export function colorQualityBitDepth(cq: ColorQuality): number {
  return cq.startsWith("10bit") ? 10 : 0;
}

/** Helper: get CloudMatch chromaFormat value (0 = 4:2:0, 2 = 4:4:4) */
export function colorQualityChromaFormat(cq: ColorQuality): number {
  return cq.endsWith("444") ? 2 : 0;
}

/** Helper: does this color quality mode require HEVC or AV1? */
export function colorQualityRequiresHevc(cq: ColorQuality): boolean {
  return cq !== "8bit_420";
}

/** Helper: is this a 10-bit (HDR-capable) mode? */
export function colorQualityIs10Bit(cq: ColorQuality): boolean {
  return cq.startsWith("10bit");
}

export type MicrophoneMode = "disabled" | "push-to-talk" | "voice-activity";
export type AspectRatio = "16:9" | "16:10" | "21:9" | "32:9";
export type RuntimePlatform =
  | "aix"
  | "android"
  | "cygwin"
  | "darwin"
  | "freebsd"
  | "haiku"
  | "linux"
  | "netbsd"
  | "openbsd"
  | "sunos"
  | "win32"
  | "unknown";

export type MacOsMicrophoneAccessStatus = "not-determined" | "granted" | "denied" | "restricted" | "unknown";

export interface MicrophonePermissionResult {
  platform: RuntimePlatform;
  isMacOs: boolean;
  status: MacOsMicrophoneAccessStatus | "not-applicable";
  granted: boolean;
  canRequest: boolean;
  shouldUseBrowserApi: boolean;
}

export interface Settings {
  resolution: string;
  aspectRatio: AspectRatio;
  fps: number;
  maxBitrateMbps: number;
  codec: VideoCodec;
  colorQuality: ColorQuality;
  region: string;
  clipboardPaste: boolean;
  mouseSensitivity: number;
  mouseAcceleration: number;
  shortcutToggleStats: string;
  shortcutTogglePointerLock: string;
  shortcutStopStream: string;
  shortcutToggleAntiAfk: string;
  shortcutToggleMicrophone: string;
  shortcutScreenshot: string;
  shortcutToggleRecording: string;
  microphoneMode: MicrophoneMode;
  microphoneDeviceId: string;
  hideStreamButtons: boolean;
  showStatsOnLaunch: boolean;
  controllerMode: boolean;
  controllerUiSounds: boolean;
  autoLoadControllerLibrary: boolean;
  /** When true, controller-mode overlays will show animated background orbs */
  controllerBackgroundAnimations: boolean;
  /** When true, the app will automatically enter fullscreen when controller mode triggers it */
  autoFullScreen: boolean;
  favoriteGameIds: string[];
  sessionCounterEnabled: boolean;
  sessionClockShowEveryMinutes: number;
  sessionClockShowDurationSeconds: number;
  windowWidth: number;
  windowHeight: number;
  /** Keyboard layout for mapping physical keys inside the remote session */
  keyboardLayout: KeyboardLayout;
  /** In-game language setting (sent to GFN servers via languageCode parameter) */
  gameLanguage: GameLanguage;
  /** Experimental request for Low Latency, Low Loss, Scalable throughput on new sessions */
  enableL4S: boolean;
}

export interface LoginProvider {
  idpId: string;
  code: string;
  displayName: string;
  streamingServiceUrl: string;
  priority: number;
}

export interface AuthTokens {
  accessToken: string;
  refreshToken?: string;
  idToken?: string;
  expiresAt: number;
  clientToken?: string;
  clientTokenExpiresAt?: number;
  clientTokenLifetimeMs?: number;
}

export interface AuthUser {
  userId: string;
  displayName: string;
  email?: string;
  avatarUrl?: string;
  membershipTier: string;
}

export interface EntitledResolution {
  width: number;
  height: number;
  fps: number;
}

export interface StorageAddon {
  type: "PERMANENT_STORAGE";
  sizeGb?: number;
  usedGb?: number;
  regionName?: string;
  regionCode?: string;
}

export interface SubscriptionInfo {
  membershipTier: string;
  subscriptionType?: string;
  subscriptionSubType?: string;
  allottedHours: number;
  purchasedHours: number;
  rolledOverHours: number;
  usedHours: number;
  remainingHours: number;
  totalHours: number;
  firstEntitlementStartDateTime?: string;
  serverRegionId?: string;
  currentSpanStartDateTime?: string;
  currentSpanEndDateTime?: string;
  notifyUserWhenTimeRemainingInMinutes?: number;
  notifyUserOnSessionWhenRemainingTimeInMinutes?: number;
  state?: string;
  isGamePlayAllowed?: boolean;
  isUnlimited: boolean;
  storageAddon?: StorageAddon;
  entitledResolutions: EntitledResolution[];
}

export interface AuthSession {
  provider: LoginProvider;
  tokens: AuthTokens;
  user: AuthUser;
}

export interface ThankYouContributor {
  login: string;
  avatarUrl: string;
  profileUrl: string;
  contributions: number;
}

export interface ThankYouSupporter {
  name: string;
  avatarUrl?: string;
  profileUrl?: string;
  isPrivate: boolean;
}

export interface ThankYouDataResult {
  contributors: ThankYouContributor[];
  supporters: ThankYouSupporter[];
  contributorsError?: string;
  supportersError?: string;
}

export interface AuthLoginRequest {
  providerIdpId?: string;
}

export interface AuthSessionRequest {
  forceRefresh?: boolean;
}

export type AuthRefreshOutcome = "not_attempted" | "refreshed" | "failed" | "missing_refresh_token";

export interface AuthRefreshStatus {
  attempted: boolean;
  forced: boolean;
  outcome: AuthRefreshOutcome;
  message: string;
  error?: string;
}

export interface AuthSessionResult {
  session: AuthSession | null;
  refresh: AuthRefreshStatus;
}

export interface RegionsFetchRequest {
  token?: string;
}

export interface StreamRegion {
  name: string;
  url: string;
  pingMs?: number;
}

export interface PingResult {
  url: string;
  pingMs: number | null;
  error?: string;
}

export interface GamesFetchRequest {
  token?: string;
  providerStreamingBaseUrl?: string;
}

export interface ResolveLaunchIdRequest {
  token?: string;
  providerStreamingBaseUrl?: string;
  appIdOrUuid: string;
}

export interface SubscriptionFetchRequest {
  token?: string;
  providerStreamingBaseUrl?: string;
  userId: string;
}

export interface GameVariant {
  id: string;
  store: string;
  supportedControls: string[];
}

export interface GameInfo {
  id: string;
  uuid?: string;
  launchAppId?: string;
  title: string;
  description?: string;
  longDescription?: string;
  featureLabels?: string[];
  genres?: string[];
  imageUrl?: string;
  screenshotUrl?: string;
  playType?: string;
  membershipTierLabel?: string;
  selectedVariantIndex: number;
  variants: GameVariant[];
}

export interface StreamSettings {
  resolution: string;
  fps: number;
  maxBitrateMbps: number;
  codec: VideoCodec;
  colorQuality: ColorQuality;
  /** Keyboard layout for mapping physical keys inside the remote session */
  keyboardLayout: KeyboardLayout;
  /** In-game language setting (sent to GFN servers via languageCode parameter) */
  gameLanguage: GameLanguage;
  /** Experimental request for Low Latency, Low Loss, Scalable throughput on new sessions */
  enableL4S: boolean;
}

export interface SessionCreateRequest {
  token?: string;
  streamingBaseUrl?: string;
  appId: string;
  internalTitle: string;
  accountLinked?: boolean;
  zone: string;
  settings: StreamSettings;
}

export interface SessionPollRequest {
  token?: string;
  streamingBaseUrl?: string;
  serverIp?: string;
  zone: string;
  sessionId: string;
  clientId?: string;
  deviceId?: string;
}

export interface SessionStopRequest {
  token?: string;
  streamingBaseUrl?: string;
  serverIp?: string;
  zone: string;
  sessionId: string;
  clientId?: string;
  deviceId?: string;
}

export type SessionAdAction = "start" | "pause" | "resume" | "finish" | "cancel";

export interface SessionAdReportRequest {
  token?: string;
  streamingBaseUrl?: string;
  serverIp?: string;
  zone: string;
  sessionId: string;
  clientId?: string;
  deviceId?: string;
  adId: string;
  action: SessionAdAction;
  clientTimestamp?: number;
  watchedTimeInMs?: number;
  pausedTimeInMs?: number;
  cancelReason?: string;
  errorInfo?: string;
}

export interface IceServer {
  urls: string[];
  username?: string;
  credential?: string;
}

export interface MediaConnectionInfo {
  ip: string;
  port: number;
}

/** Server-negotiated stream profile received from CloudMatch after session ready */
export interface NegotiatedStreamProfile {
  resolution?: string;
  fps?: number;
  colorQuality?: ColorQuality;
  enableL4S?: boolean;
}

export interface SessionAdMediaFile {
  mediaFileUrl?: string;
  encodingProfile?: string;
}

export interface SessionOpportunityInfo {
  state?: string;
  queuePaused?: boolean;
  gracePeriodSeconds?: number;
  message?: string;
  title?: string;
  description?: string;
}

export interface SessionAdInfo {
  adId: string;
  state?: number;
  adState?: number;
  adUrl?: string;
  mediaUrl?: string;
  adMediaFiles?: SessionAdMediaFile[];
  clickThroughUrl?: string;
  adLengthInSeconds?: number;
  durationMs?: number;
  title?: string;
  description?: string;
}

export interface SessionAdState {
  isAdsRequired: boolean;
  sessionAdsRequired?: boolean;
  isQueuePaused?: boolean;
  gracePeriodSeconds?: number;
  message?: string;
  sessionAds: SessionAdInfo[];
  ads: SessionAdInfo[];
  opportunity?: SessionOpportunityInfo;
  /**
   * True when the server explicitly returned sessionAds=null (transient gap
   * between polls). False/absent when ads were populated by the server or
   * when the list was explicitly cleared client-side after a failed ad action.
   * Used by mergeAdState to decide whether to restore the previous ad list.
   */
  serverSentEmptyAds?: boolean;
  enableL4S?: boolean;
}

export function getSessionAdItems(adState: SessionAdState | undefined): SessionAdInfo[] {
  return adState?.sessionAds ?? adState?.ads ?? [];
}

export function isSessionAdsRequired(adState: SessionAdState | undefined): boolean {
  return adState?.sessionAdsRequired ?? adState?.isAdsRequired ?? false;
}

export function getSessionAdOpportunity(adState: SessionAdState | undefined): SessionOpportunityInfo | undefined {
  return adState?.opportunity;
}

export function isSessionQueuePaused(adState: SessionAdState | undefined): boolean {
  return getSessionAdOpportunity(adState)?.queuePaused ?? adState?.isQueuePaused ?? false;
}

export function getSessionAdGracePeriodSeconds(adState: SessionAdState | undefined): number | undefined {
  return getSessionAdOpportunity(adState)?.gracePeriodSeconds ?? adState?.gracePeriodSeconds;
}

export function getSessionAdMessage(adState: SessionAdState | undefined): string | undefined {
  const opportunity = getSessionAdOpportunity(adState);
  return opportunity?.message ?? opportunity?.description ?? adState?.message;
}

export function getPreferredSessionAdMediaUrl(ad: SessionAdInfo | undefined): string | undefined {
  return ad?.adMediaFiles?.find((mediaFile) => mediaFile.mediaFileUrl)?.mediaFileUrl ?? ad?.adUrl ?? ad?.mediaUrl;
}

export function getSessionAdDurationMs(ad: SessionAdInfo | undefined): number | undefined {
  if (typeof ad?.adLengthInSeconds === "number" && Number.isFinite(ad.adLengthInSeconds) && ad.adLengthInSeconds > 0) {
    return Math.round(ad.adLengthInSeconds * 1000);
  }
  return ad?.durationMs;
}

export interface SessionInfo {
  sessionId: string;
  status: number;
  queuePosition?: number;
  seatSetupStep?: number;
  adState?: SessionAdState;
  zone: string;
  streamingBaseUrl?: string;
  serverIp: string;
  signalingServer: string;
  signalingUrl: string;
  gpuType?: string;
  iceServers: IceServer[];
  mediaConnectionInfo?: MediaConnectionInfo;
  negotiatedStreamProfile?: NegotiatedStreamProfile;
  clientId?: string;
  deviceId?: string;
}

/** Information about an active session from getActiveSessions */
export interface ActiveSessionInfo {
  sessionId: string;
  appId: number;
  gpuType?: string;
  status: number;
  serverIp?: string;
  signalingUrl?: string;
  resolution?: string;
  fps?: number;
}

/** Request to claim/resume an existing session */
export interface SessionClaimRequest {
  token?: string;
  streamingBaseUrl?: string;
  sessionId: string;
  serverIp: string;
  appId?: string;
  settings?: StreamSettings;
}

export interface SignalingConnectRequest {
  sessionId: string;
  signalingServer: string;
  signalingUrl?: string;
}

export interface IceCandidatePayload {
  candidate: string;
  sdpMid?: string | null;
  sdpMLineIndex?: number | null;
  usernameFragment?: string | null;
}

export interface SendAnswerRequest {
  sdp: string;
  nvstSdp?: string;
}

export interface KeyframeRequest {
  reason: string;
  backlogFrames: number;
  attempt: number;
}

export type MainToRendererSignalingEvent =
  | { type: "connected" }
  | { type: "disconnected"; reason: string }
  | { type: "offer"; sdp: string }
  | { type: "remote-ice"; candidate: IceCandidatePayload }
  | { type: "error"; message: string }
  | { type: "log"; message: string };

/** Dialog result for session conflict resolution */
export type SessionConflictChoice = "resume" | "new" | "cancel";

export interface OpenNowApi {
  getAuthSession(input?: AuthSessionRequest): Promise<AuthSessionResult>;
  getLoginProviders(): Promise<LoginProvider[]>;
  getRegions(input?: RegionsFetchRequest): Promise<StreamRegion[]>;
  login(input: AuthLoginRequest): Promise<AuthSession>;
  logout(): Promise<void>;
  fetchSubscription(input: SubscriptionFetchRequest): Promise<SubscriptionInfo>;
  fetchMainGames(input: GamesFetchRequest): Promise<GameInfo[]>;
  fetchLibraryGames(input: GamesFetchRequest): Promise<GameInfo[]>;
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

  /** Listen for screenshot hotkey events from the main process (F11) */
  onTriggerScreenshot(listener: () => void): () => void;

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
  getThanksData(): Promise<ThankYouDataResult>;
}

export interface ScreenshotSaveRequest {
  dataUrl: string;
  gameTitle?: string;
}

export interface ScreenshotDeleteRequest {
  id: string;
}

export interface ScreenshotSaveAsRequest {
  id: string;
}

export interface ScreenshotSaveAsResult {
  saved: boolean;
  filePath?: string;
}

export interface ScreenshotEntry {
  id: string;
  fileName: string;
  filePath: string;
  createdAtMs: number;
  sizeBytes: number;
  dataUrl: string;
}

export interface RecordingEntry {
  id: string;
  fileName: string;
  filePath: string;
  createdAtMs: number;
  sizeBytes: number;
  durationMs: number;
  gameTitle?: string;
  thumbnailDataUrl?: string;
}

export interface RecordingBeginRequest {
  mimeType: string;
}

export interface RecordingBeginResult {
  recordingId: string;
}

export interface RecordingChunkRequest {
  recordingId: string;
  chunk: ArrayBuffer;
}

export interface RecordingFinishRequest {
  recordingId: string;
  durationMs: number;
  gameTitle?: string;
  thumbnailDataUrl?: string;
}

export interface RecordingAbortRequest {
  recordingId: string;
}

export interface RecordingDeleteRequest {
  id: string;
}

export interface MediaListingEntry {
  id: string;
  fileName: string;
  filePath: string;
  createdAtMs: number;
  sizeBytes: number;
  gameTitle?: string;
  durationMs?: number;
  thumbnailDataUrl?: string;
  dataUrl?: string;
}

export interface MediaListingResult {
  screenshots: MediaListingEntry[];
  videos: MediaListingEntry[];
}
