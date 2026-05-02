import type { VideoCodec, ColorQuality, KeyboardLayout, GameLanguage } from "./streamPreferences";


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

export type ExistingSessionStrategy = "auto-resume" | "force-new";
