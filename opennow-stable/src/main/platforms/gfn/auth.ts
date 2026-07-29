import { randomBytes } from "node:crypto";

import { shell } from "electron";

import type {
  AuthDeviceLoginAttemptRequest,
  AuthDeviceLoginChallenge,
  AuthDeviceLoginPollRequest,
  AuthDeviceLoginPollResult,
  AuthDeviceLoginStartRequest,
  AuthLoginRequest,
  AuthSession,
  AuthSessionResult,
  AuthTokens,
  LoginProvider,
  SavedAccount,
  StreamRegion,
  SubscriptionInfo,
} from "@shared/gfn";

import type { SafeStorageLike } from "../../security/encryptedJsonFile";
import { AccountManager } from "./auth/accountManager";
import { ConsoleProfileStore } from "./auth/consoleProfileStore";
import { exchangeDeviceCode, requestDeviceAuthorization } from "./auth/deviceLogin";
import { SubscriptionVpcEnrichmentCaches } from "./auth/enrichmentCaches";
import {
  buildAuthUrl,
  exchangeAuthorizationCode,
  findAvailablePort,
  generatePkce,
  waitForAuthorizationCode,
} from "./auth/oauthFlow";
import { PersistedAccountState } from "./auth/persistedAccountState";
import {
  normalizeProvider,
  ProviderDiscovery,
} from "./auth/providerDiscovery";
import { SessionValidityCoordinator } from "./auth/sessionValidity";
import { fetchUserInfo } from "./auth/userInfo";
import { buildGfnLcarsHeaders } from "./clientHeaders";

interface ServerInfoResponse {
  metaData?: Array<{
    key: string;
    value: string;
  }>;
}

interface DeviceLoginAttempt {
  provider: LoginProvider;
  deviceCode: string;
  expiresAt: number;
}

export class AuthService {
  private readonly state: PersistedAccountState;
  private readonly providerDiscovery: ProviderDiscovery;
  private readonly enrichmentCaches: SubscriptionVpcEnrichmentCaches;
  private readonly sessionValidity: SessionValidityCoordinator;
  private readonly accountManager: AccountManager;
  private readonly consoleProfiles: ConsoleProfileStore;
  private deviceLoginAttempts = new Map<string, DeviceLoginAttempt>();
  private pendingDeviceLoginSessions = new Map<string, AuthSession>();

  constructor(statePath: string, consoleProfilesPath: string, secretStorage: SafeStorageLike) {
    this.state = new PersistedAccountState(statePath);
    this.consoleProfiles = new ConsoleProfileStore(consoleProfilesPath, secretStorage);
    this.providerDiscovery = new ProviderDiscovery();
    this.enrichmentCaches = new SubscriptionVpcEnrichmentCaches({
      getSession: () => this.getSession(),
      getSelectedProvider: () => this.getSelectedProvider(),
      ensureValidSession: () => this.ensureValidSession(),
      updateSession: (session) => this.state.accounts.updateSession(session),
    });
    this.sessionValidity = new SessionValidityCoordinator({
      state: this.state,
      enrichmentCaches: this.enrichmentCaches,
      logout: () => this.accountManager.logout(),
    });
    this.accountManager = new AccountManager(
      this.state,
      () => this.enrichmentCaches.clearAll(),
      this.consoleProfiles,
    );
  }

  /** Console profile PIN locks. Hashes never leave the main process. */
  getConsoleProfiles(): ConsoleProfileStore {
    return this.consoleProfiles;
  }

  async initialize(): Promise<void> {
    await this.consoleProfiles.initialize();
    const restoredSession = await this.state.initialize();
    if (restoredSession) {
      this.state.accounts.setSelectedProvider(restoredSession.provider);
      await this.enrichmentCaches.enrichUserTier();
      await this.state.persist();
    }
  }

  async getProviders(): Promise<LoginProvider[]> {
    return this.providerDiscovery.getProviders();
  }

  setSession(session: AuthSession | null): void {
    this.accountManager.setSession(session);
  }

  getSession(): AuthSession | null {
    return this.state.accounts.getSession();
  }

  getSavedAccounts(): SavedAccount[] {
    return this.accountManager.getSavedAccounts();
  }

  async switchAccount(userId: string): Promise<AuthSession> {
    return this.accountManager.switchAccount(
      userId,
      (forceRefresh, expectedUserId) =>
        this.ensureValidSessionWithStatus(forceRefresh, expectedUserId),
    );
  }

  async removeAccount(userId: string): Promise<void> {
    await this.accountManager.removeAccount(userId);
  }

  async logoutAll(): Promise<void> {
    await this.accountManager.logoutAll();
  }

  getSelectedProvider(): LoginProvider {
    return this.state.accounts.getSelectedProvider();
  }

  private async selectLoginProvider(providerIdpId?: string): Promise<LoginProvider> {
    const selected = await this.providerDiscovery.selectProvider(
      this.state.accounts.getPersistedSelectedProvider(),
      providerIdpId,
    );
    this.state.accounts.setSelectedProvider(selected);
    return this.state.accounts.getPersistedSelectedProvider();
  }

  private async buildLoginSession(
    initialTokens: AuthTokens,
    provider: LoginProvider,
  ): Promise<AuthSession> {
    const user = await fetchUserInfo(initialTokens);
    console.debug("auth: fetched user info during login", {
      userId: user.userId,
      email: user.email,
      avatarUrl: user.avatarUrl,
    });
    let tokens = initialTokens;
    try {
      tokens = await this.sessionValidity.ensureClientToken(initialTokens);
    } catch (error) {
      console.warn("Unable to fetch client token after login. Falling back to OAuth token only:", error);
    }

    return {
      provider: normalizeProvider(provider),
      tokens,
      user,
    };
  }

  private async saveLoginSession(session: AuthSession): Promise<AuthSession> {
    return this.accountManager.saveLoginSession(
      session,
      () => this.enrichmentCaches.enrichUserTier(),
    );
  }

  private pruneExpiredDeviceLogins(now = Date.now(), skipAttemptId?: string): void {
    for (const [attemptId, attempt] of this.deviceLoginAttempts) {
      if (attemptId === skipAttemptId) {
        continue;
      }
      if (attempt.expiresAt <= now) {
        this.deviceLoginAttempts.delete(attemptId);
        this.pendingDeviceLoginSessions.delete(attemptId);
      }
    }
  }

  async getRegions(explicitToken?: string): Promise<StreamRegion[]> {
    const provider = this.getSelectedProvider();
    const base = provider.streamingServiceUrl.endsWith("/")
      ? provider.streamingServiceUrl
      : `${provider.streamingServiceUrl}/`;

    let token = explicitToken;
    if (!token) {
      const session = await this.ensureValidSession();
      token = session ? session.tokens.idToken ?? session.tokens.accessToken : undefined;
    }

    const headers = buildGfnLcarsHeaders({
      token,
      clientType: "BROWSER",
      clientStreamer: "WEBRTC",
      includeUserAgent: true,
    });

    let response: Response;
    try {
      response = await fetch(`${base}v2/serverInfo`, { headers });
    } catch {
      return [];
    }
    if (!response.ok) {
      return [];
    }

    const payload = (await response.json()) as ServerInfoResponse;
    return (payload.metaData ?? [])
      .filter((entry) => entry.value.startsWith("https://"))
      .filter((entry) => entry.key !== "gfn-regions" && !entry.key.startsWith("gfn-"))
      .map<StreamRegion>((entry) => ({
        name: entry.key,
        url: entry.value.endsWith("/") ? entry.value : `${entry.value}/`,
      }))
      .sort((a, b) => a.name.localeCompare(b.name));
  }

  async login(input: AuthLoginRequest): Promise<AuthSession> {
    const provider = await this.selectLoginProvider(input.providerIdpId);
    const { verifier, challenge } = generatePkce();
    const port = await findAvailablePort();
    const authUrl = buildAuthUrl(provider, challenge, port);
    const codePromise = waitForAuthorizationCode(port, 120000);
    await shell.openExternal(authUrl);
    const code = await codePromise;
    const initialTokens = await exchangeAuthorizationCode(code, verifier, port);
    const session = await this.buildLoginSession(initialTokens, provider);
    return this.saveLoginSession(session);
  }

  async startDeviceLogin(input: AuthDeviceLoginStartRequest): Promise<AuthDeviceLoginChallenge> {
    this.pruneExpiredDeviceLogins();
    const provider = await this.selectLoginProvider(input.providerIdpId);
    const challenge = await requestDeviceAuthorization(provider);
    const attemptId = randomBytes(16).toString("hex");
    this.deviceLoginAttempts.set(attemptId, {
      provider,
      deviceCode: challenge.deviceCode,
      expiresAt: challenge.expiresAt,
    });
    return { ...challenge, attemptId };
  }

  async pollDeviceLogin(input: AuthDeviceLoginPollRequest): Promise<AuthDeviceLoginPollResult> {
    this.pruneExpiredDeviceLogins();
    if (!input.attemptId || !input.deviceCode) {
      return { status: "error", error: "Missing device code" };
    }

    const attempt = this.deviceLoginAttempts.get(input.attemptId);
    if (!attempt || attempt.deviceCode !== input.deviceCode) {
      return { status: "expired", error: "QR login was cancelled or expired" };
    }
    if (Date.now() >= attempt.expiresAt) {
      this.cancelDeviceLogin(input);
      return { status: "expired", error: "QR login expired" };
    }

    const result = await exchangeDeviceCode(input.deviceCode);
    if (!this.deviceLoginAttempts.has(input.attemptId)) {
      return { status: "expired", error: "QR login was cancelled" };
    }

    if ("accessToken" in result) {
      const session = await this.buildLoginSession(result, attempt.provider);
      if (!this.deviceLoginAttempts.has(input.attemptId)) {
        return { status: "expired", error: "QR login was cancelled" };
      }
      this.pendingDeviceLoginSessions.set(input.attemptId, session);
      return { status: "authorized" };
    }

    switch (result.error) {
      case "authorization_pending":
        return { status: "pending", error: result.error_description };
      case "slow_down":
        return { status: "slow_down", error: result.error_description };
      case "expired_token":
        this.cancelDeviceLogin(input);
        return { status: "expired", error: result.error_description ?? "QR login expired" };
      case "access_denied":
        this.cancelDeviceLogin(input);
        return { status: "access_denied", error: result.error_description ?? "QR login was denied" };
      default:
        this.cancelDeviceLogin(input);
        return { status: "error", error: result.error_description ?? result.error ?? "QR login failed" };
    }
  }

  async completeDeviceLogin(input: AuthDeviceLoginAttemptRequest): Promise<AuthSession> {
    this.pruneExpiredDeviceLogins(Date.now(), input.attemptId);
    const session = this.pendingDeviceLoginSessions.get(input.attemptId);
    if (!session || !this.deviceLoginAttempts.has(input.attemptId)) {
      throw new Error("QR login is no longer active");
    }

    this.cancelDeviceLogin(input);
    return this.saveLoginSession(session);
  }

  cancelDeviceLogin(input: AuthDeviceLoginAttemptRequest): void {
    this.deviceLoginAttempts.delete(input.attemptId);
    this.pendingDeviceLoginSessions.delete(input.attemptId);
  }

  async logout(): Promise<void> {
    await this.accountManager.logout();
  }

  async getSubscription(): Promise<SubscriptionInfo | null> {
    return this.enrichmentCaches.getSubscription();
  }

  clearSubscriptionCache(): void {
    this.enrichmentCaches.clearSubscription();
  }

  getCachedSubscription(): SubscriptionInfo | null {
    return this.enrichmentCaches.getCachedSubscription();
  }

  async getVpcId(explicitToken?: string): Promise<string | null> {
    return this.enrichmentCaches.getVpcId(explicitToken);
  }

  clearVpcCache(): void {
    this.enrichmentCaches.clearVpc();
  }

  getCachedVpcId(): string | null {
    return this.enrichmentCaches.getCachedVpcId();
  }

  async ensureValidSessionWithStatus(
    forceRefresh = false,
    expectedUserId?: string,
  ): Promise<AuthSessionResult> {
    return this.sessionValidity.ensureValidSessionWithStatus(forceRefresh, expectedUserId);
  }

  async ensureValidSession(): Promise<AuthSession | null> {
    return this.sessionValidity.ensureValidSession();
  }

  async resolveJwtToken(explicitToken?: string): Promise<string> {
    if (this.getSession()) {
      const session = await this.ensureValidSession();
      if (!session) {
        throw new Error("No authenticated session available");
      }
      return session.tokens.idToken ?? session.tokens.accessToken;
    }

    if (explicitToken && explicitToken.trim()) {
      return explicitToken.trim();
    }

    const session = await this.ensureValidSession();
    if (!session) {
      throw new Error("No authenticated session available");
    }
    return session.tokens.idToken ?? session.tokens.accessToken;
  }
}
