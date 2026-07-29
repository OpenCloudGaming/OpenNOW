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
  authClientId?: string;
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

export interface AuthSession {
  provider: LoginProvider;
  tokens: AuthTokens;
  user: AuthUser;
}

/**
 * Account fields derivable from the persisted auth session alone. The PIN lock
 * store is a separate owner, so it decorates this into a `SavedAccount`.
 */
export interface SavedAccountIdentity {
  userId: string;
  displayName: string;
  email?: string;
  avatarUrl?: string;
  membershipTier: string;
  providerCode: string;
}

export interface SavedAccount extends SavedAccountIdentity {
  /** Whether a console profile PIN is set. The hash never leaves the main process. */
  hasPin: boolean;
}

export type ConsolePinFailureReason =
  | "invalid_format"
  | "invalid_pin"
  | "locked_out"
  | "no_pin_set"
  | "unknown_account"
  | "storage_unavailable";

export interface ConsolePinSetRequest {
  userId: string;
  pin: string;
  /** Required when replacing an existing PIN. */
  currentPin?: string;
}

export interface ConsolePinClearRequest {
  userId: string;
  currentPin: string;
}

export interface ConsolePinVerifyRequest {
  userId: string;
  pin: string;
}

export interface ConsolePinStatus {
  userId: string;
  hasPin: boolean;
  /** Epoch ms until which verification is refused, or null when unlocked. */
  lockedUntilMs: number | null;
  remainingAttempts: number;
}

export interface ConsolePinVerifyResult {
  ok: boolean;
  reason?: ConsolePinFailureReason;
  remainingAttempts: number;
  lockedUntilMs: number | null;
}

export interface ConsolePinMutationResult {
  ok: boolean;
  reason?: ConsolePinFailureReason;
  hasPin: boolean;
}

export interface AuthLoginRequest {
  providerIdpId?: string;
}

export interface AuthDeviceLoginStartRequest {
  providerIdpId?: string;
}

export interface AuthDeviceLoginChallenge {
  attemptId: string;
  deviceCode: string;
  userCode: string;
  verificationUri: string;
  verificationUriComplete: string;
  expiresAt: number;
  intervalSeconds: number;
}

export interface AuthDeviceLoginPollRequest {
  attemptId: string;
  deviceCode: string;
}

export interface AuthDeviceLoginAttemptRequest {
  attemptId: string;
}

export type AuthDeviceLoginPollStatus =
  | "pending"
  | "slow_down"
  | "expired"
  | "access_denied"
  | "authorized"
  | "error";

export interface AuthDeviceLoginPollResult {
  status: AuthDeviceLoginPollStatus;
  session?: AuthSession;
  error?: string;
  intervalSeconds?: number;
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
