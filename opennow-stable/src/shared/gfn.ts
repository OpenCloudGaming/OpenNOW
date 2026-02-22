export type VideoCodec = "H264" | "H265" | "AV1";
export type VideoAccelerationPreference = "auto" | "hardware" | "software";

export type HdrStreamingMode = "off" | "auto" | "on";

export type MicMode = "off" | "on" | "push-to-talk";

export type HevcCompatMode = "auto" | "force_h264" | "force_hevc" | "hevc_software";

export type VideoDecodeBackend = "auto" | "vaapi" | "v4l2" | "software";

export interface PlatformInfo {
  platform: string;
  arch: string;
}

export interface MicSettings {
  micMode: MicMode;
  micDeviceId: string;
  micGain: number;
  micNoiseSuppression: boolean;
  micAutoGainControl: boolean;
  micEchoCancellation: boolean;
  shortcutToggleMic: string;
}

export interface MicDeviceInfo {
  deviceId: string;
  label: string;
  isDefault: boolean;
}

export type MicStatus = "off" | "active" | "muted" | "no-device" | "permission-denied" | "error";

export type HdrPlatformSupport = "supported" | "best_effort" | "unsupported" | "unknown";

export type HdrActiveStatus = "active" | "inactive" | "unsupported" | "fallback_sdr";

export interface HdrCapability {
  platform: "windows" | "macos" | "linux" | "unknown";
  platformSupport: HdrPlatformSupport;
  osHdrEnabled: boolean;
  displayHdrCapable: boolean;
  decoder10BitCapable: boolean;
  hdrColorSpaceSupported: boolean;
  notes: string[];
}

export interface HdrStreamState {
  status: HdrActiveStatus;
  bitDepth: 8 | 10;
  colorPrimaries: "BT.709" | "BT.2020" | "unknown";
  transferFunction: "SDR" | "PQ" | "HLG" | "unknown";
  matrixCoefficients: "BT.709" | "BT.2020" | "unknown";
  codecProfile: string;
  overlayForcesSdr: boolean;
  fallbackReason: string | null;
}/** Color quality (bit depth + chroma subsampling), matching Rust ColorQuality enum */
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

export type MicrophoneMode = "disabled" | "push-to-talk" | "voice-activity";

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
  shortcutToggleMicrophone: string;
  microphoneMode: MicrophoneMode;
  microphoneDeviceId: string;
  hideStreamButtons: boolean;
  windowWidth: number;
  windowHeight: number;
  discordPresenceEnabled: boolean;
  discordClientId: string;
  flightControlsEnabled: boolean;
  flightControlsSlot: number;
  flightSlots: FlightSlotConfig[];
  hdrStreaming: HdrStreamingMode;
  micMode: MicMode;
  micDeviceId: string;
  micGain: number;
  micNoiseSuppression: boolean;
  micAutoGainControl: boolean;
  micEchoCancellation: boolean;
  shortcutToggleMic: string;
  hevcCompatMode: HevcCompatMode;
  videoDecodeBackend: VideoDecodeBackend;  sessionClockShowEveryMinutes: number;
  sessionClockShowDurationSeconds: number;
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
  queuePosition?: number;
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
  /** Export logs in redacted format */
  exportLogs(format?: "text" | "json"): Promise<string>;
  updateDiscordPresence(state: DiscordPresencePayload): Promise<void>;
  clearDiscordPresence(): Promise<void>;
  flightGetProfile(vidPid: string, gameId?: string): Promise<FlightProfile | null>;
  flightSetProfile(profile: FlightProfile): Promise<void>;
  flightDeleteProfile(vidPid: string, gameId?: string): Promise<void>;
  flightGetAllProfiles(): Promise<FlightProfile[]>;
  flightResetProfile(vidPid: string): Promise<FlightProfile | null>;
  getOsHdrInfo(): Promise<{ osHdrEnabled: boolean; platform: string }>;
  relaunchApp(): Promise<void>;
  micEnumerateDevices(): Promise<MicDeviceInfo[]>;
  onMicDevicesChanged(listener: (devices: MicDeviceInfo[]) => void): () => void;
  onSessionExpired(listener: (reason: string) => void): () => void;
  getPlatformInfo(): Promise<PlatformInfo>;
}

export type FlightAxisTarget =
  | "leftStickX"
  | "leftStickY"
  | "rightStickX"
  | "rightStickY"
  | "leftTrigger"
  | "rightTrigger";

export type FlightSensitivityCurve = "linear" | "expo";

export interface FlightHidAxisSource {
  byteOffset: number;
  byteCount: 1 | 2;
  littleEndian: boolean;
  unsigned: boolean;
  rangeMin: number;
  rangeMax: number;
}

export interface FlightHidButtonSource {
  byteOffset: number;
  bitIndex: number;
}

export interface FlightHidHatSource {
  byteOffset: number;
  bitOffset: number;
  bitCount: 4 | 8;
  centerValue: number;
}

export interface FlightHidReportLayout {
  skipReportId: boolean;
  reportLength: number;
  axes: FlightHidAxisSource[];
  buttons: FlightHidButtonSource[];
  hat?: FlightHidHatSource;
}

export interface FlightAxisMapping {
  sourceIndex: number;
  target: FlightAxisTarget;
  inverted: boolean;
  deadzone: number;
  sensitivity: number;
  curve: FlightSensitivityCurve;
}

export interface FlightButtonMapping {
  sourceIndex: number;
  targetButton: number;
}

export interface FlightProfile {
  name: string;
  vidPid: string;
  deviceName: string;
  axisMappings: FlightAxisMapping[];
  buttonMappings: FlightButtonMapping[];
  reportLayout?: FlightHidReportLayout;
  gameId?: string;
}

export interface FlightSlotConfig {
  enabled: boolean;
  deviceKey: string | null;
  vidPid: string | null;
  deviceName: string | null;
}

export function makeDeviceKey(vendorId: number, productId: number, name: string): string {
  const vid = vendorId.toString(16).toUpperCase().padStart(4, "0");
  const pid = productId.toString(16).toUpperCase().padStart(4, "0");
  return `${vid}:${pid}:${name}`;
}

export function defaultFlightSlots(): FlightSlotConfig[] {
  return [0, 1, 2, 3].map(() => ({ enabled: false, deviceKey: null, vidPid: null, deviceName: null }));
}

export interface FlightControlsState {
  connected: boolean;
  deviceName: string;
  axes: number[];
  buttons: boolean[];
  hatSwitch: number;
  rawBytes: number[];
}

export interface FlightGamepadState {
  controllerId: number;
  buttons: number;
  leftTrigger: number;
  rightTrigger: number;
  leftStickX: number;
  leftStickY: number;
  rightStickX: number;
  rightStickY: number;
  connected: boolean;
}

export interface DiscordPresencePayload {
  type: "idle" | "queue" | "streaming";
  gameName?: string;
  resolution?: string;
  fps?: number;
  bitrateMbps?: number;
  region?: string;
  startTimestamp?: number;
  queuePosition?: number;}
