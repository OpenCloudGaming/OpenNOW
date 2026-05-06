import type { NativeCloudGsyncCapabilities, CloudGsyncResolution } from "./cloudGsync";

export type VideoCodec = "H264" | "H265" | "AV1";
export type VideoAccelerationPreference = "auto" | "hardware" | "software";
export type StreamClientMode = "web" | "native";
export type NativeStreamerBackend = "stub" | "gstreamer";
export type NativeStreamerBackendPreference = "auto" | NativeStreamerBackend;
export type NativeStreamerFeatureMode = "auto" | "disabled" | "forced";
export type NativeVideoBackendPreference = "auto" | "d3d11" | "d3d12";
export type NativeQueueMode = "auto" | "fixed" | "adaptive" | "vrr";

export function nativeStreamerFeatureModeToEnvValue(mode: NativeStreamerFeatureMode): "auto" | "0" | "1" {
  switch (mode) {
    case "disabled":
      return "0";
    case "forced":
      return "1";
    default:
      return "auto";
  }
}

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

export const USER_FACING_VIDEO_CODEC_OPTIONS: readonly VideoCodec[] = ["H264", "H265", "AV1"];
export const USER_FACING_COLOR_QUALITY_OPTIONS: readonly ColorQuality[] = ["8bit_420", "8bit_444", "10bit_420", "10bit_444"];

export function isSupportedUserFacingCodec(codec: VideoCodec): boolean {
  return USER_FACING_VIDEO_CODEC_OPTIONS.includes(codec);
}

export function normalizeStreamPreferences(codec: VideoCodec, colorQuality: ColorQuality): {
  codec: VideoCodec;
  colorQuality: ColorQuality;
  migrated: boolean;
} {
  const normalizedCodec = isSupportedUserFacingCodec(codec)
    ? codec
    : USER_FACING_VIDEO_CODEC_OPTIONS[0];
  const normalizedColorQuality = USER_FACING_COLOR_QUALITY_OPTIONS.includes(colorQuality)
    ? colorQuality
    : USER_FACING_COLOR_QUALITY_OPTIONS[0];

  return {
    codec: normalizedCodec,
    colorQuality: normalizedColorQuality,
    migrated: normalizedCodec !== codec || normalizedColorQuality !== colorQuality,
  };
}

/** Helper: is this a 10-bit (HDR-capable) mode? */
export function colorQualityIs10Bit(cq: ColorQuality): boolean {
  return cq.startsWith("10bit");
}

/** Controller-mode XMB background visual preset */
export type ControllerThemeStyle = "aurora" | "nebula" | "grid" | "minimal" | "pulse";

/** RGB tint for controller-mode background (0–255 each) */
export interface ControllerThemeRgb {
  r: number;
  g: number;
  b: number;
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

export type NativeGstreamerRuntimeSource = "bundled" | "system" | "missing" | "unknown";

export interface NativeGstreamerInstallInstruction {
  distro: string;
  command: string;
  note?: string;
}

export interface NativeGstreamerRuntimeStatus {
  source: NativeGstreamerRuntimeSource;
  bundled: boolean;
  path?: string;
  message: string;
  installInstructions?: NativeGstreamerInstallInstruction[];
}

export interface NativeStreamerStatus {
  detected: boolean;
  gstreamerAvailable: boolean;
  supportsOfferAnswer: boolean;
  backend?: NativeStreamerBackend;
  fallbackReason?: string;
  videoBackends?: NativeVideoBackendCapability[];
  activeVideoBackend?: NativeVideoBackendCapability;
  codecSummary?: string;
  zeroCopySummary?: string;
  gstreamerRuntime: NativeGstreamerRuntimeStatus;
  message: string;
}

export type NativeVideoBackendId =
  | "d3d12"
  | "d3d11"
  | "videotoolbox"
  | "vaapi"
  | "v4l2"
  | "vulkan"
  | "software"
  | string;

export interface NativeVideoCodecCapability {
  codec: "h264" | "h265" | "av1" | string;
  available: boolean;
  decoder?: string;
  parser?: string;
  depayloader?: string;
  reason?: string;
}

export interface NativeVideoBackendCapability {
  backend: NativeVideoBackendId;
  platform: "windows" | "macos" | "linux" | "cross-platform" | "other" | string;
  codecs: NativeVideoCodecCapability[];
  zeroCopyModes: string[];
  sink?: string;
  available: boolean;
  reason?: string;
}

export interface StreamingFeatures {
  reflex?: boolean;
  bitDepth?: number;
  cloudGsync?: boolean;
  chromaFormat?: number;
  enabledL4S?: boolean;
  trueHdr?: boolean;
}

export interface NativeTransitionDiagnostics {
  disableDynamicSplitEncodeUpdates?: boolean;
  forceQueueMode?: NativeQueueMode;
  disableTransitionFlushEscalation?: boolean;
}

export interface Settings {
  resolution: string;
  aspectRatio: AspectRatio;
  posterSizeScale: number;
  fps: number;
  maxBitrateMbps: number;
  streamClientMode: StreamClientMode;
  nativeStreamerBackend: NativeStreamerBackendPreference;
  nativeVideoBackend: NativeVideoBackendPreference;
  nativeStreamerExecutablePath: string;
  nativeCloudGsyncMode: NativeStreamerFeatureMode;
  nativeD3dFullscreenMode: NativeStreamerFeatureMode;
  nativeExternalRenderer: boolean;
  codec: VideoCodec;
  decoderPreference: VideoAccelerationPreference;
  encoderPreference: VideoAccelerationPreference;
  colorQuality: ColorQuality;
  region: string;
  sessionProxyEnabled: boolean;
  sessionProxyUrl: string;
  clipboardPaste: boolean;
  mouseSensitivity: number;
  mouseAcceleration: number;
  shortcutToggleStats: string;
  shortcutTogglePointerLock: string;
  shortcutToggleFullscreen: string;
  shortcutStopStream: string;
  shortcutToggleAntiAfk: string;
  shortcutToggleMicrophone: string;
  shortcutScreenshot: string;
  shortcutToggleRecording: string;
  microphoneMode: MicrophoneMode;
  microphoneDeviceId: string;
  hideStreamButtons: boolean;
  showAntiAfkIndicator: boolean;
  showStatsOnLaunch: boolean;
  /** Skip the free-tier queue server selection modal and launch with default routing */
  hideServerSelector: boolean;
  controllerMode: boolean;
  controllerUiSounds: boolean;
  autoLoadControllerLibrary: boolean;
  /** When true, controller-mode overlays will show animated background orbs */
  controllerBackgroundAnimations: boolean;
  /** Controller-mode library background visual preset */
  controllerThemeStyle: ControllerThemeStyle;
  /** Controller-mode library background tint (applied per style preset) */
  controllerThemeColor: ControllerThemeRgb;
  /**
   * When true, controller library/hub/loading layers may show art from the focused game or shelf.
   * Theme color/style presets still apply when false.
   */
  controllerLibraryGameBackdrop: boolean;
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
  /** Request Cloud G-Sync / Variable Refresh Rate on new sessions */
  enableCloudGsync: boolean;
  /** Hidden diagnostics for native transition recovery and 240 FPS server-side stream changes */
  nativeTransitionDiagnostics?: NativeTransitionDiagnostics;
  /** Show the currently streaming game as Discord Rich Presence activity */
  discordRichPresence: boolean;
  /** Automatically check GitHub Releases for app updates in the background */
  autoCheckForUpdates: boolean;
  /** When true, pressing Escape will exit fullscreen; when false Escape is sent to the game while pointer-locked */
  allowEscapeToExitFullscreen?: boolean;
}

export const DEFAULT_STREAM_PREFERENCES: Readonly<Pick<Settings, "codec" | "colorQuality">> = Object.freeze({
  codec: "H264",
  colorQuality: "10bit_420",
});

export function getDefaultStreamPreferences(): Pick<Settings, "codec" | "colorQuality"> {
  const normalized = normalizeStreamPreferences(
    DEFAULT_STREAM_PREFERENCES.codec,
    DEFAULT_STREAM_PREFERENCES.colorQuality,
  );
  return {
    codec: normalized.codec,
    colorQuality: normalized.colorQuality,
  };
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

export interface SavedAccount {
  userId: string;
  displayName: string;
  email?: string;
  avatarUrl?: string;
  membershipTier: string;
  providerCode: string;
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

export interface CatalogBrowseRequest extends GamesFetchRequest {
  searchQuery?: string;
  sortId?: string;
  filterIds?: string[];
  fetchCount?: number;
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
  librarySelected?: boolean;
  libraryStatus?: string;
  lastPlayedDate?: string;
  gfnStatus?: string;
}

export const OWNED_LIBRARY_STATUSES = ["MANUAL", "PLATFORM_SYNC", "IN_LIBRARY"] as const;

export function normalizeGameStore(store: string): string {
  return store.toUpperCase().replace(/[\s-]+/g, "_");
}

export function isOwnedLibraryStatus(status?: string): boolean {
  return typeof status === "string" && OWNED_LIBRARY_STATUSES.includes(status as (typeof OWNED_LIBRARY_STATUSES)[number]);
}

export function isOwnedVariant(variant: Pick<GameVariant, "libraryStatus">): boolean {
  return isOwnedLibraryStatus(variant.libraryStatus);
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
  publisherName?: string;
  contentRatings?: string[];
  playabilityState?: string;
  availableStores?: string[];
  searchText?: string;
  lastPlayed?: string;
  isInLibrary?: boolean;
  selectedVariantIndex: number;
  variants: GameVariant[];
}

export function isGameInLibrary(game: Pick<GameInfo, "variants">): boolean {
  return game.variants.some((variant) => isOwnedVariant(variant));
}

export function isEpicStore(store: string): boolean {
  const key = normalizeGameStore(store);
  return key === "EPIC_GAMES_STORE" || key === "EPIC" || key === "EGS";
}

export interface CatalogFilterOption {
  id: string;
  rawId: string;
  label: string;
  groupId: string;
  groupLabel: string;
}

export interface CatalogFilterGroup {
  id: string;
  label: string;
  options: CatalogFilterOption[];
}

export interface CatalogSortOption {
  id: string;
  label: string;
  orderBy: string;
}

export interface CatalogBrowseResult {
  games: GameInfo[];
  numberReturned: number;
  numberSupported: number;
  totalCount: number;
  hasNextPage: boolean;
  endCursor?: string;
  searchQuery: string;
  selectedSortId: string;
  selectedFilterIds: string[];
  filterGroups: CatalogFilterGroup[];
  sortOptions: CatalogSortOption[];
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
  /** Request Cloud G-Sync / Variable Refresh Rate on new sessions */
  enableCloudGsync: boolean;
  /** Renderer-selected client path; main uses this to apply native-only Cloud G-Sync gating. */
  clientMode?: StreamClientMode;
  /** Selected native streamer backend; stub cannot support Cloud G-Sync presentation. */
  nativeStreamerBackend?: NativeStreamerBackendPreference;
  /** Native-only override for Cloud G-Sync display detection. */
  nativeCloudGsyncMode?: NativeStreamerFeatureMode;
  /** User's raw Cloud G-Sync preference before main-process capability resolution. */
  requestedCloudGsync?: boolean;
  /** Diagnostics from the main-process Cloud G-Sync resolver. */
  cloudGsyncResolution?: CloudGsyncResolution;
  /** Hidden diagnostics for native transition recovery and 240 FPS server-side stream changes. */
  nativeTransitionDiagnostics?: NativeTransitionDiagnostics;
}

export interface SessionCreateRequest {
  token?: string;
  streamingBaseUrl?: string;
  appId: string;
  internalTitle: string;
  accountLinked?: boolean;
  existingSessionStrategy?: ExistingSessionStrategy;
  zone: string;
  settings: StreamSettings;
  proxyUrl?: string;
}

export interface SessionPollRequest {
  token?: string;
  streamingBaseUrl?: string;
  serverIp?: string;
  zone: string;
  sessionId: string;
  clientId?: string;
  deviceId?: string;
  proxyUrl?: string;
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
  codec?: VideoCodec;
  colorQuality?: ColorQuality;
  enableL4S?: boolean;
  enableCloudGsync?: boolean;
  enableReflex?: boolean;
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
  requestedStreamingFeatures?: StreamingFeatures;
  finalizedStreamingFeatures?: StreamingFeatures;
  clientId?: string;
  deviceId?: string;
}

/** Information about an active session from getActiveSessions */
export interface ActiveSessionInfo {
  sessionId: string;
  appId: number;
  gpuType?: string;
  status: number;
  streamingBaseUrl?: string;
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
  clientId?: string;
  deviceId?: string;
  appId?: string;
  settings?: StreamSettings;
  /** True when claim is triggered by automatic reconnect recovery logic */
  recoveryMode?: boolean;
}

export interface SignalingConnectRequest {
  sessionId: string;
  signalingServer: string;
  signalingUrl?: string;
  nativeStreamer?: NativeStreamerSessionContext;
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

export interface NativeStreamerSessionContext {
  session: SessionInfo;
  settings: StreamSettings;
}

export function buildNativeStreamerSessionContext(
  session: SessionInfo,
  settings: StreamSettings,
): NativeStreamerSessionContext {
  const negotiatedStreamProfile = session.negotiatedStreamProfile
    ? {
      ...session.negotiatedStreamProfile,
      codec: session.negotiatedStreamProfile.codec ?? settings.codec,
    }
    : { codec: settings.codec };

  return {
    session: {
      ...session,
      negotiatedStreamProfile,
    },
    settings: {
      ...settings,
      enableCloudGsync:
        session.negotiatedStreamProfile?.enableCloudGsync ?? settings.enableCloudGsync,
    },
  };
}

export interface NativeVideoTransition {
  transitionType: string;
  source: string;
  atMs: number;
  oldCaps?: string;
  newCaps?: string;
  oldFramerate?: string;
  newFramerate?: string;
  oldMemoryMode?: string;
  newMemoryMode?: string;
  renderGapMs?: number;
  requestedFps?: number;
  capsFramerate?: string;
  highFpsRisk?: boolean;
  queueMode?: NativeQueueMode;
  summary?: string;
}

export interface NativeInputPacket {
  payload: ArrayBuffer | Uint8Array | number[];
  partiallyReliable?: boolean;
}

export interface NativeRenderSurfaceRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface NativeRenderSurfaceUpdate {
  rect: NativeRenderSurfaceRect | null;
  visible: boolean;
  deviceScaleFactor: number;
  showStats?: boolean;
}

export interface NativeRenderSurface extends NativeRenderSurfaceUpdate {
  windowHandle?: string;
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
  | { type: "native-stream-started"; message?: string }
  | { type: "native-stream-stopped"; reason?: string }
  | { type: "native-stream-stats"; stats: NativeStreamStats }
  | { type: "native-stream-transition"; transition: NativeVideoTransition }
  | { type: "native-input-ready"; protocolVersion: number }
  | { type: "error"; message: string }
  | { type: "log"; message: string };

export interface NativeStreamStats {
  codec: string;
  resolution: string;
  hardwareAcceleration: string;
  memoryMode?: string;
  zeroCopy?: boolean;
  requestedFps?: number;
  capsFramerate?: string;
  bitrateKbps: number;
  targetBitrateKbps: number;
  bitratePerformancePercent: number;
  decodedFps: number;
  renderFps: number;
  framesDecoded: number;
  framesRendered: number;
  framesPendingToPresent?: number;
  sinkRendered?: number;
  sinkDropped?: number;
  zeroCopyD3D11: boolean;
  zeroCopyD3D12: boolean;
  queueMode?: NativeQueueMode;
  queueDepthChanges?: number;
  presentPacingChanges?: number;
  partialFlushCount?: number;
  completeFlushCount?: number;
  lastTransitionType?: string;
  lastTransitionAtMs?: number;
  lastTransitionSummary?: string;
  requestedStreamingFeaturesSummary?: string;
  finalizedStreamingFeaturesSummary?: string;
}

/** Dialog result for session conflict resolution */
export type SessionConflictChoice = "resume" | "new" | "cancel";

export type ExistingSessionStrategy = "auto-resume" | "force-new";

export type AppUpdaterStatus =
  | "disabled"
  | "idle"
  | "checking"
  | "available"
  | "not-available"
  | "downloading"
  | "downloaded"
  | "error";

export interface AppUpdaterProgress {
  percent: number;
  transferred: number;
  total: number;
  bytesPerSecond: number;
}

export interface AppUpdaterState {
  status: AppUpdaterStatus;
  currentVersion: string;
  availableVersion?: string;
  downloadedVersion?: string;
  progress?: AppUpdaterProgress;
  lastCheckedAt?: number;
  message?: string;
  errorCode?: string;
  updateSource: "github-releases";
  canCheck: boolean;
  canDownload: boolean;
  canInstall: boolean;
  isPackaged: boolean;
}

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
  getNativeStreamerStatus(): Promise<NativeStreamerStatus>;
  getNativeCloudGsyncCapabilities(): Promise<NativeCloudGsyncCapabilities>;
  /** Show dialog asking user how to handle session conflict */
  showSessionConflictDialog(): Promise<SessionConflictChoice>;
  connectSignaling(input: SignalingConnectRequest): Promise<void>;
  disconnectSignaling(): Promise<void>;
  sendAnswer(input: SendAnswerRequest): Promise<void>;
  sendIceCandidate(input: IceCandidatePayload): Promise<void>;
  sendNativeInput(input: NativeInputPacket): void;
  updateNativeRenderSurface(input: NativeRenderSurfaceUpdate): void;
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
  /** Notify main process that pointer lock state changed (active = true/false) */
  notifyPointerLockChange(active: boolean): void;
  getSettings(): Promise<Settings>;
  setSetting<K extends keyof Settings>(key: K, value: Settings[K]): Promise<void>;
  resetSettings(): Promise<Settings>;
  selectNativeStreamerExecutable(): Promise<string | null>;
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

  /** Listen for external Escape events forwarded by the main process */
  onExternalEscape(listener: () => void): () => void;

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
  /** Clear Discord rich presence activity */
  clearDiscordActivity(): Promise<void>;
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

/** A single zone entry from the PrintedWaste queue API */
export interface PrintedWasteZone {
  QueuePosition: number;
  /** Unix timestamp of last update */
  "Last Updated": number;
  /** Geographic region code: "US" | "EU" | "JP" | "KR" | "CA" | "THAI" | "MY" */
  Region: string;
  /** Estimated wait time in milliseconds */
  eta?: number;
}

/** Full data payload from https://api.printedwaste.com/gfn/queue/ */
export type PrintedWasteQueueData = Record<string, PrintedWasteZone>;

/** PrintedWaste server metadata entry from remote mapping config */
export interface PrintedWasteServerMappingEntry {
  title?: string;
  region?: string;
  is4080Server?: boolean;
  is5080Server?: boolean;
  nuked?: boolean;
}

/** Full data payload from PrintedWaste server-to-region mapping config */
export type PrintedWasteServerMapping = Record<string, PrintedWasteServerMappingEntry>;
