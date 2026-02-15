export type VideoCodec = "H264" | "H265" | "AV1";
export type VideoAccelerationPreference = "auto" | "hardware" | "software";

/** Color quality (bit depth + chroma subsampling), matching Rust ColorQuality enum */
export type ColorQuality = "8bit_420" | "8bit_444" | "10bit_420" | "10bit_444";

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

export interface Settings {
  resolution: string;
  fps: number;
  maxBitrateMbps: number;
  codec: VideoCodec;
  decoderPreference: VideoAccelerationPreference;
  encoderPreference: VideoAccelerationPreference;
  colorQuality: ColorQuality;
  region: string;
  clipboardPaste: boolean;
  mouseSensitivity: number;
  shortcutToggleStats: string;
  shortcutTogglePointerLock: string;
  shortcutStopStream: string;
  shortcutToggleAntiAfk: string;
  windowWidth: number;
  windowHeight: number;
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
}

export interface SubscriptionInfo {
  membershipTier: string;
  remainingHours: number;
  totalHours: number;
  isUnlimited: boolean;
  storageAddon?: StorageAddon;
  entitledResolutions: EntitledResolution[];
}

export interface AuthSession {
  provider: LoginProvider;
  tokens: AuthTokens;
  user: AuthUser;
}

export interface AuthLoginRequest {
  providerIdpId?: string;
}

export interface RegionsFetchRequest {
  token?: string;
}

export interface StreamRegion {
  name: string;
  url: string;
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
  imageUrl?: string;
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
}

export interface SessionStopRequest {
  token?: string;
  streamingBaseUrl?: string;
  serverIp?: string;
  zone: string;
  sessionId: string;
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

export interface SessionInfo {
  sessionId: string;
  status: number;
  zone: string;
  streamingBaseUrl?: string;
  serverIp: string;
  signalingServer: string;
  signalingUrl: string;
  gpuType?: string;
  iceServers: IceServer[];
  mediaConnectionInfo?: MediaConnectionInfo;
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
  getAuthSession(): Promise<AuthSession | null>;
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
  onSignalingEvent(listener: (event: MainToRendererSignalingEvent) => void): () => void;
  /** Listen for F11 fullscreen toggle from main process */
  onToggleFullscreen(listener: () => void): () => void;
  toggleFullscreen(): Promise<void>;
  togglePointerLock(): Promise<void>;
  getSettings(): Promise<Settings>;
  setSetting<K extends keyof Settings>(key: K, value: Settings[K]): Promise<void>;
  resetSettings(): Promise<Settings>;
}
