import type { CloudGsyncResolution } from "../cloudGsync";
import type {
  AppLaunchMode,
  ColorQuality,
  NativeTransitionDiagnostics,
  StreamClientMode,
  StreamingFeatures,
  VideoCodec,
} from "./stream";
import type {
  NativeStreamerFeatureMode,
  StreamTransportMode,
} from "./nativeStreamer";
import type { GameLanguage, KeyboardLayout } from "./keyboard";

export type SessionConflictChoice = "resume" | "new" | "cancel";
export type ExistingSessionStrategy = "auto-resume" | "force-new";

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
  /** Native media transport selected for the session. */
  transportMode?: StreamTransportMode;
  /** Native-only override for Cloud G-Sync display detection. */
  nativeCloudGsyncMode?: NativeStreamerFeatureMode;
  /** User's raw Cloud G-Sync preference before main-process capability resolution. */
  requestedCloudGsync?: boolean;
  /** Diagnostics from the main-process Cloud G-Sync resolver. */
  cloudGsyncResolution?: CloudGsyncResolution;
  /** Hidden diagnostics for native transition recovery and 240 FPS server-side stream changes. */
  nativeTransitionDiagnostics?: NativeTransitionDiagnostics;
  /** Requested session app launch mode; "gamepadFriendly" asks NVIDIA to launch games big-picture style. */
  appLaunchMode?: AppLaunchMode;
}

export interface SessionCreateRequest {
  token?: string;
  streamingBaseUrl?: string;
  appId: string;
  internalTitle: string;
  /** Public game artwork used only for the local Discord Rich Presence. */
  discordGameImageUrl?: string;
  accountLinked?: boolean;
  /**
   * Official clients only enable server-side in-game graphics/settings persistence
   * when the user is entitled and has opted in. Leave disabled by default.
   */
  enablePersistingInGameSettings?: boolean;
  /** Selected game variant must advertise IN_GAME_SETTINGS_PERSISTENCE_ENABLED. */
  supportsInGameSettingsPersistence?: boolean;
  existingSessionStrategy?: ExistingSessionStrategy;
  zone: string;
  settings: StreamSettings;
  supportedCodecs?: readonly VideoCodec[];
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
  usage?: number;
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
  subSessionId?: string;
  appId?: string;
  status: number;
  queuePosition?: number;
  seatSetupStep?: number;
  adState?: SessionAdState;
  zone: string;
  streamingBaseUrl?: string;
  serverIp: string;
  signalingServer: string;
  signalingUrl: string;
  /** CloudMatch-provided server location or zone hostname, when available. */
  serverLocation?: string;
  gpuType?: string;
  /** Wire appLaunchMode the session runs with, kept session-stable for resumes */
  appLaunchMode?: number;
  /** Wire in-game settings persistence value the session was created with, kept session-stable for resumes */
  enablePersistingInGameSettings?: boolean;
  /** Complete ordered CloudMatch transport list consumed by the native stream SDK. */
  connectionInfo?: SessionConnectionInfo[];
  /** Classic NVST RTSPS endpoints from CloudMatch usage=16 connections. */
  rtspsEndpoints?: string[];
  iceServers: IceServer[];
  mediaConnectionInfo?: MediaConnectionInfo;
  negotiatedStreamProfile?: NegotiatedStreamProfile;
  requestedStreamingFeatures?: StreamingFeatures;
  finalizedStreamingFeatures?: StreamingFeatures;
  clientId?: string;
  deviceId?: string;
}

export interface SessionConnectionInfo {
  ip?: string;
  port: number;
  usage: number;
  protocol?: number;
  appLevelProtocol?: number;
  resourcePath?: string;
  [key: string]: unknown;
}

/** Information about an active session from getActiveSessions */
export interface ActiveSessionInfo {
  sessionId: string;
  subSessionId?: string;
  appId: number;
  /** Wire appLaunchMode the session was created with, as echoed by the server */
  appLaunchMode?: number;
  /** Wire in-game settings persistence value the session was created with, when echoed by the server */
  enablePersistingInGameSettings?: boolean;
  gpuType?: string;
  status: number;
  queuePosition?: number;
  seatSetupStep?: number;
  streamingBaseUrl?: string;
  serverIp?: string;
  signalingUrl?: string;
  resolution?: string;
  fps?: number;
}

export interface GfnSessionQueueState {
  status: number;
  queuePosition?: number;
  seatSetupStep?: number;
}

export function isSessionReadyForConnectStatus(status: number): boolean {
  return status === 2 || status === 3;
}

export function isGfnSessionInQueue(session: GfnSessionQueueState): boolean {
  if (session.seatSetupStep === 1) {
    return true;
  }
  return (session.queuePosition ?? 0) > 1;
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
  /** Session-stable wire appLaunchMode captured when the session was created */
  appLaunchMode?: number;
  /** In-game settings persistence value to send with the resume claim. Defaults to false. */
  enablePersistingInGameSettings?: boolean;
  settings?: StreamSettings;
  /** True when claim is triggered by automatic reconnect recovery logic */
  recoveryMode?: boolean;
}
