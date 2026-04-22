import { createServer } from "node:http";
import { createHash, randomBytes } from "node:crypto";
import { access, mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname } from "node:path";
import net from "node:net";
import os from "node:os";

import { shell } from "electron";

import type {
  AuthLoginRequest,
  AuthSession,
  AuthSessionResult,
  AuthTokens,
  AuthUser,
  LoginProvider,
  SavedAccount,
  StreamRegion,
  SubscriptionInfo,
} from "@shared/gfn";
import { fetchSubscription, fetchDynamicRegions } from "./subscription";

const SERVICE_URLS_ENDPOINT = "https://pcs.geforcenow.com/v1/serviceUrls";
const TOKEN_ENDPOINT = "https://login.nvidia.com/token";
const CLIENT_TOKEN_ENDPOINT = "https://login.nvidia.com/client_token";
const USERINFO_ENDPOINT = "https://login.nvidia.com/userinfo";
const AUTH_ENDPOINT = "https://login.nvidia.com/authorize";

const CLIENT_ID = "ZU7sPN-miLujMD95LfOQ453IB0AtjM8sMyvgJ9wCXEQ";
const SCOPES = "openid consent email tk_client age";
const DEFAULT_IDP_ID = "PDiAhv2kJTFeQ7WOPqiQ2tRZ7lGhR2X11dXvM4TZSxg";

const GFN_USER_AGENT =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36 NVIDIACEFClient/HEAD/debb5919f6 GFN-PC/2.0.80.173";

const REDIRECT_PORTS = [2259, 6460, 7119, 8870, 9096];
const TOKEN_REFRESH_WINDOW_MS = 10 * 60 * 1000;
const CLIENT_TOKEN_REFRESH_WINDOW_MS = 5 * 60 * 1000;

interface PersistedAuthState {
  sessions: AuthSession[];
  activeUserId: string | null;
  selectedProvider: LoginProvider | null;
}

interface ServiceUrlsResponse {
  requestStatus?: {
    statusCode?: number;
  };
  gfnServiceInfo?: {
    gfnServiceEndpoints?: Array<{
      idpId: string;
      loginProviderCode: string;
      loginProviderDisplayName: string;
      streamingServiceUrl: string;
      loginProviderPriority?: number;
    }>;
  };
}

interface TokenResponse {
  access_token: string;
  refresh_token?: string;
  id_token?: string;
  client_token?: string;
  expires_in?: number;
}

interface ClientTokenResponse {
  client_token: string;
  expires_in?: number;
}

interface ServerInfoResponse {
  requestStatus?: {
    serverId?: string;
  };
  metaData?: Array<{
    key: string;
    value: string;
  }>;
}

function defaultProvider(): LoginProvider {
  return {
    idpId: DEFAULT_IDP_ID,
    code: "NVIDIA",
    displayName: "NVIDIA",
    streamingServiceUrl: "https://prod.cloudmatchbeta.nvidiagrid.net/",
    priority: 0,
  };
}

function normalizeProvider(provider: LoginProvider): LoginProvider {
  return {
    ...provider,
    streamingServiceUrl: provider.streamingServiceUrl.endsWith("/")
      ? provider.streamingServiceUrl
      : `${provider.streamingServiceUrl}/`,
  };
}

function decodeBase64Url(value: string): string {
  const normalized = value.replace(/-/g, "+").replace(/_/g, "/");
  const padding = normalized.length % 4;
  const padded = padding === 0 ? normalized : `${normalized}${"=".repeat(4 - padding)}`;
  return Buffer.from(padded, "base64").toString("utf8");
}

function parseJwtPayload<T>(token: string): T | null {
  const parts = token.split(".");
  if (parts.length !== 3) {
    return null;
  }
  try {
    const payload = decodeBase64Url(parts[1]);
    return JSON.parse(payload) as T;
  } catch {
    return null;
  }
}

function toExpiresAt(expiresInSeconds: number | undefined, defaultSeconds = 86400): number {
  return Date.now() + (expiresInSeconds ?? defaultSeconds) * 1000;
}

function isExpired(expiresAt: number | undefined): boolean {
  if (!expiresAt) {
    return true;
  }
  return expiresAt <= Date.now();
}

function isNearExpiry(expiresAt: number | undefined, windowMs: number): boolean {
  if (!expiresAt) {
    return true;
  }
  return expiresAt - Date.now() < windowMs;
}

function generateDeviceId(): string {
  const host = os.hostname();
  const username = os.userInfo().username;
  return createHash("sha256").update(`${host}:${username}:opennow-stable`).digest("hex");
}

function generatePkce(): { verifier: string; challenge: string } {
  const verifier = randomBytes(64)
    .toString("base64")
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/g, "")
    .slice(0, 86);

  const challenge = createHash("sha256")
    .update(verifier)
    .digest("base64")
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/g, "");

  return { verifier, challenge };
}

function buildAuthUrl(provider: LoginProvider, challenge: string, port: number): string {
  const redirectUri = `http://localhost:${port}`;
  const nonce = randomBytes(16).toString("hex");
  const params = new URLSearchParams({
    response_type: "code",
    device_id: generateDeviceId(),
    scope: SCOPES,
    client_id: CLIENT_ID,
    redirect_uri: redirectUri,
    ui_locales: "en_US",
    nonce,
    prompt: "select_account",
    code_challenge: challenge,
    code_challenge_method: "S256",
    idp_id: provider.idpId,
  });
  return `${AUTH_ENDPOINT}?${params.toString()}`;
}

async function isPortAvailable(port: number): Promise<boolean> {
  return new Promise((resolve) => {
    const server = net.createServer();
    server.once("error", () => resolve(false));
    server.once("listening", () => {
      server.close(() => resolve(true));
    });
    server.listen(port, "127.0.0.1");
  });
}

async function findAvailablePort(): Promise<number> {
  for (const port of REDIRECT_PORTS) {
    if (await isPortAvailable(port)) {
      return port;
    }
  }

  throw new Error("No available OAuth callback ports");
}

async function waitForAuthorizationCode(port: number, timeoutMs: number): Promise<string> {
  return new Promise((resolve, reject) => {
    const server = createServer((request, response) => {
      const url = new URL(request.url ?? "/", `http://localhost:${port}`);
      const code = url.searchParams.get("code");
      const error = url.searchParams.get("error");

      const html = `<!doctype html><html><head><meta charset="utf-8"><title>OpenNOW Login</title></head><body style="font-family:Segoe UI,Arial,sans-serif;background:#0b1220;color:#dbe7ff;display:flex;justify-content:center;align-items:center;height:100vh"><div style="background:#111a2c;padding:24px 28px;border:1px solid #30425f;border-radius:12px;max-width:460px"><h2 style="margin-top:0">OpenNOW Login</h2><p>${
        code
          ? "Login complete. You can close this window and return to OpenNOW Stable."
          : "Login failed or was cancelled. You can close this window and return to OpenNOW Stable."
      }</p></div></body></html>`;

      response.statusCode = 200;
      response.setHeader("Content-Type", "text/html; charset=utf-8");
      response.end(html);

      server.close(() => {
        if (code) {
          resolve(code);
          return;
        }
        reject(new Error(error ?? "Authorization failed"));
      });
    });

    server.listen(port, "127.0.0.1", () => {
      const timer = setTimeout(() => {
        server.close(() => reject(new Error("Timed out waiting for OAuth callback")));
      }, timeoutMs);

      server.once("close", () => clearTimeout(timer));
    });
  });
}

async function exchangeAuthorizationCode(code: string, verifier: string, port: number): Promise<AuthTokens> {
  const body = new URLSearchParams({
    grant_type: "authorization_code",
    code,
    redirect_uri: `http://localhost:${port}`,
    code_verifier: verifier,
  });

  const response = await fetch(TOKEN_ENDPOINT, {
    method: "POST",
    headers: {
      "Content-Type": "application/x-www-form-urlencoded; charset=UTF-8",
      Origin: "https://nvfile",
      Referer: "https://nvfile/",
      Accept: "application/json, text/plain, */*",
      "User-Agent": GFN_USER_AGENT,
    },
    body,
  });

  if (!response.ok) {
    const text = await response.text();
    throw new Error(`Token exchange failed (${response.status}): ${text.slice(0, 400)}`);
  }

  const payload = (await response.json()) as TokenResponse;
  return {
    accessToken: payload.access_token,
    refreshToken: payload.refresh_token,
    idToken: payload.id_token,
    expiresAt: toExpiresAt(payload.expires_in),
  };
}

async function refreshAuthTokens(refreshToken: string): Promise<AuthTokens> {
  const body = new URLSearchParams({
    grant_type: "refresh_token",
    refresh_token: refreshToken,
    client_id: CLIENT_ID,
  });

  const response = await fetch(TOKEN_ENDPOINT, {
    method: "POST",
    headers: {
      "Content-Type": "application/x-www-form-urlencoded; charset=UTF-8",
      Origin: "https://nvfile",
      Accept: "application/json, text/plain, */*",
      "User-Agent": GFN_USER_AGENT,
    },
    body,
  });

  if (!response.ok) {
    const text = await response.text();
    throw new Error(`Token refresh failed (${response.status}): ${text.slice(0, 400)}`);
  }

  const payload = (await response.json()) as TokenResponse;
  return {
    accessToken: payload.access_token,
    refreshToken: payload.refresh_token ?? refreshToken,
    idToken: payload.id_token,
    expiresAt: toExpiresAt(payload.expires_in),
  };
}

async function requestClientToken(accessToken: string): Promise<{
  token: string;
  expiresAt: number;
  lifetimeMs: number;
}> {
  const response = await fetch(CLIENT_TOKEN_ENDPOINT, {
    headers: {
      Authorization: `Bearer ${accessToken}`,
      Origin: "https://nvfile",
      Accept: "application/json, text/plain, */*",
      "User-Agent": GFN_USER_AGENT,
    },
  });

  if (!response.ok) {
    const text = await response.text();
    throw new Error(`Client token request failed (${response.status}): ${text.slice(0, 400)}`);
  }

  const payload = (await response.json()) as ClientTokenResponse;
  const expiresAt = toExpiresAt(payload.expires_in);
  return {
    token: payload.client_token,
    expiresAt,
    lifetimeMs: Math.max(0, expiresAt - Date.now()),
  };
}

async function refreshWithClientToken(clientToken: string, userId: string): Promise<TokenResponse> {
  const body = new URLSearchParams({
    grant_type: "urn:ietf:params:oauth:grant-type:client_token",
    client_token: clientToken,
    client_id: CLIENT_ID,
    sub: userId,
  });

  const response = await fetch(TOKEN_ENDPOINT, {
    method: "POST",
    headers: {
      "Content-Type": "application/x-www-form-urlencoded; charset=UTF-8",
      Origin: "https://nvfile",
      Accept: "application/json, text/plain, */*",
      "User-Agent": GFN_USER_AGENT,
    },
    body,
  });

  if (!response.ok) {
    const text = await response.text();
    throw new Error(`Client-token refresh failed (${response.status}): ${text.slice(0, 400)}`);
  }

  return (await response.json()) as TokenResponse;
}

function mergeTokenSnapshot(base: AuthTokens, refreshed: TokenResponse): AuthTokens {
  return {
    accessToken: refreshed.access_token,
    refreshToken: refreshed.refresh_token ?? base.refreshToken,
    idToken: refreshed.id_token,
    expiresAt: toExpiresAt(refreshed.expires_in),
    clientToken: refreshed.client_token ?? base.clientToken,
    clientTokenExpiresAt: base.clientTokenExpiresAt,
    clientTokenLifetimeMs: base.clientTokenLifetimeMs,
  };
}

function gravatarUrl(email: string, size = 80): string {
  const normalized = email.trim().toLowerCase();
  const hash = createHash("md5").update(normalized).digest("hex");
  return `https://www.gravatar.com/avatar/${hash}?s=${size}&d=identicon`;
}

async function fetchUserInfo(tokens: AuthTokens): Promise<AuthUser> {
  const jwtToken = tokens.idToken ?? tokens.accessToken;
  const parsed = parseJwtPayload<{
    sub?: string;
    email?: string;
    preferred_username?: string;
    gfn_tier?: string;
    picture?: string;
  }>(jwtToken);

  if (parsed?.sub) {
    const emailFromToken = parsed.email;
    const pictureFromToken = parsed.picture;
    if (emailFromToken || pictureFromToken) {
      const avatar = pictureFromToken ?? (emailFromToken ? gravatarUrl(emailFromToken) : undefined);
      return {
        userId: parsed.sub,
        displayName: parsed.preferred_username ?? emailFromToken?.split("@")[0] ?? "User",
        email: emailFromToken,
        avatarUrl: avatar,
        membershipTier: parsed.gfn_tier ?? "FREE",
      };
    }
  }

  const response = await fetch(USERINFO_ENDPOINT, {
    headers: {
      Authorization: `Bearer ${tokens.accessToken}`,
      Origin: "https://nvfile",
      Accept: "application/json",
      "User-Agent": GFN_USER_AGENT,
    },
  });

  if (!response.ok) {
    throw new Error(`User info failed (${response.status})`);
  }

  const payload = (await response.json()) as {
    sub: string;
    preferred_username?: string;
    email?: string;
    picture?: string;
  };

  const email = payload.email;
  const avatar = payload.picture ?? (email ? gravatarUrl(email) : undefined);

  return {
    userId: payload.sub,
    displayName: payload.preferred_username ?? email?.split("@")[0] ?? "User",
    email,
    avatarUrl: avatar,
    membershipTier: "FREE",
  };
}

export class AuthService {
  private providers: LoginProvider[] = [];
  private sessions = new Map<string, AuthSession>();
  private activeUserId: string | null = null;
  private selectedProvider: LoginProvider = defaultProvider();
  private cachedSubscription: SubscriptionInfo | null = null;
  private cachedVpcId: string | null = null;

  constructor(private readonly statePath: string) {}

  async initialize(): Promise<void> {
    try {
      await access(this.statePath);
    } catch {
      await mkdir(dirname(this.statePath), { recursive: true });
      await this.persist();
      return;
    }

    try {
      const raw = await readFile(this.statePath, "utf8");
      const parsed = JSON.parse(raw) as Partial<PersistedAuthState> & {
        session?: AuthSession | null;
      };
      if (parsed.selectedProvider) {
        this.selectedProvider = normalizeProvider(parsed.selectedProvider);
      }

      this.sessions.clear();
      if (Array.isArray(parsed.sessions)) {
        for (const persistedSession of parsed.sessions) {
          if (!persistedSession?.user?.userId) {
            continue;
          }
          this.sessions.set(persistedSession.user.userId, {
            ...persistedSession,
            provider: normalizeProvider(persistedSession.provider),
          });
        }
      } else if (parsed.session?.user?.userId) {
        this.sessions.set(parsed.session.user.userId, {
          ...parsed.session,
          provider: normalizeProvider(parsed.session.provider),
        });
      }

      if (typeof parsed.activeUserId === "string" && this.sessions.has(parsed.activeUserId)) {
        this.activeUserId = parsed.activeUserId;
      } else {
        this.activeUserId = this.sessions.keys().next().value ?? null;
      }

      const restoredSession = this.getSession();
      if (restoredSession) {
        this.selectedProvider = restoredSession.provider;
        await this.enrichUserTier();
        await this.persist();
      }
    } catch {
      this.sessions.clear();
      this.activeUserId = null;
      this.selectedProvider = defaultProvider();
      await this.persist();
    }
  }

  private async persist(): Promise<void> {
    const payload: PersistedAuthState = {
      sessions: Array.from(this.sessions.values()),
      activeUserId: this.activeUserId,
      selectedProvider: this.selectedProvider,
    };

    await mkdir(dirname(this.statePath), { recursive: true });
    await writeFile(this.statePath, JSON.stringify(payload, null, 2), "utf8");
  }

  private async ensureClientToken(tokens: AuthTokens, userId: string): Promise<AuthTokens> {
    const hasUsableClientToken =
      Boolean(tokens.clientToken) &&
      !isNearExpiry(tokens.clientTokenExpiresAt, CLIENT_TOKEN_REFRESH_WINDOW_MS);
    if (hasUsableClientToken) {
      return tokens;
    }

    if (isExpired(tokens.expiresAt)) {
      return tokens;
    }

    const clientToken = await requestClientToken(tokens.accessToken);
    return {
      ...tokens,
      clientToken: clientToken.token,
      clientTokenExpiresAt: clientToken.expiresAt,
      clientTokenLifetimeMs: clientToken.lifetimeMs,
    };
  }

  async getProviders(): Promise<LoginProvider[]> {
    if (this.providers.length > 0) {
      return this.providers;
    }

    let response: Response;
    try {
      response = await fetch(SERVICE_URLS_ENDPOINT, {
        headers: {
          Accept: "application/json",
          "User-Agent": GFN_USER_AGENT,
        },
      });
    } catch (error) {
      console.warn("Failed to fetch providers, using default:", error);
      this.providers = [defaultProvider()];
      return this.providers;
    }

    if (!response.ok) {
      console.warn(`Providers fetch failed with status ${response.status}, using default`);
      this.providers = [defaultProvider()];
      return this.providers;
    }

    try {
      const payload = (await response.json()) as ServiceUrlsResponse;
      const endpoints = payload.gfnServiceInfo?.gfnServiceEndpoints ?? [];

      const providers = endpoints
        .map<LoginProvider>((entry) => ({
          idpId: entry.idpId,
          code: entry.loginProviderCode,
          displayName:
            entry.loginProviderCode === "BPC" ? "bro.game" : entry.loginProviderDisplayName,
          streamingServiceUrl: entry.streamingServiceUrl,
          priority: entry.loginProviderPriority ?? 0,
        }))
        .sort((a, b) => a.priority - b.priority)
        .map(normalizeProvider);

      this.providers = providers.length > 0 ? providers : [defaultProvider()];
      console.log(`Loaded ${this.providers.length} providers`);
      return this.providers;
    } catch (error) {
      console.warn("Failed to parse providers response, using default:", error);
      this.providers = [defaultProvider()];
      return this.providers;
    }
  }

  setSession(session: AuthSession | null): void {
    if (!session) {
      this.sessions.clear();
      this.activeUserId = null;
      this.selectedProvider = defaultProvider();
      this.clearSubscriptionCache();
      this.clearVpcCache();
      void this.persist();
      return;
    }

    const normalized: AuthSession = {
      ...session,
      provider: normalizeProvider(session.provider),
    };
    this.sessions.set(normalized.user.userId, normalized);
    this.activeUserId = normalized.user.userId;
    this.selectedProvider = normalized.provider;
    this.clearSubscriptionCache();
    this.clearVpcCache();
    void this.persist();
  }

  getSession(): AuthSession | null {
    if (!this.activeUserId) {
      return null;
    }
    return this.sessions.get(this.activeUserId) ?? null;
  }

  getSavedAccounts(): SavedAccount[] {
    return Array.from(this.sessions.values()).map((session) => ({
      userId: session.user.userId,
      displayName: session.user.displayName,
      email: session.user.email,
      avatarUrl: session.user.avatarUrl,
      membershipTier: session.user.membershipTier,
      providerCode: session.provider.code,
    }));
  }

  async switchAccount(userId: string): Promise<AuthSession> {
    const target = this.sessions.get(userId);
    if (!target) {
      throw new Error("Saved account not found");
    }
    this.activeUserId = userId;
    this.selectedProvider = target.provider;
    this.clearSubscriptionCache();
    this.clearVpcCache();

    const result = await this.ensureValidSessionWithStatus(true, userId);
    const refreshFailed =
      result.refresh.outcome === "failed" || result.refresh.outcome === "missing_refresh_token";
    const switchedUserMismatch = result.session?.user.userId !== userId;
    if (!result.session || refreshFailed || switchedUserMismatch) {
      await this.removeAccount(userId);
      const fallbackMessage = "Failed to switch account due to an invalid or expired session.";
      if (switchedUserMismatch) {
        throw new Error("Switched session did not match the selected account.");
      }
      if (result.refresh.outcome === "missing_refresh_token") {
        throw new Error("Saved login for this account is incomplete. Please log in to this account again.");
      }
      throw new Error(result.refresh.message || fallbackMessage);
    }
    return result.session;
  }

  async removeAccount(userId: string): Promise<void> {
    const removed = this.sessions.delete(userId);
    if (!removed) {
      return;
    }
    if (this.activeUserId === userId) {
      this.activeUserId = this.sessions.keys().next().value ?? null;
    }
    this.selectedProvider = this.getSession()?.provider ?? defaultProvider();
    this.clearSubscriptionCache();
    this.clearVpcCache();
    await this.persist();
  }

  async logoutAll(): Promise<void> {
    this.sessions.clear();
    this.activeUserId = null;
    this.selectedProvider = defaultProvider();
    this.cachedSubscription = null;
    this.clearVpcCache();
    await this.persist();
  }

  getSelectedProvider(): LoginProvider {
    return this.getSession()?.provider ?? this.selectedProvider;
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

    const headers: Record<string, string> = {
      Accept: "application/json",
      "nv-client-id": "ec7e38d4-03af-4b58-b131-cfb0495903ab",
      "nv-client-type": "BROWSER",
      "nv-client-version": "2.0.80.173",
      "nv-client-streamer": "WEBRTC",
      "nv-device-os": "WINDOWS",
      "nv-device-type": "DESKTOP",
      "User-Agent": GFN_USER_AGENT,
    };

    if (token) {
      headers.Authorization = `GFNJWT ${token}`;
    }

    let response: Response;
    try {
      response = await fetch(`${base}v2/serverInfo`, {
        headers,
      });
    } catch {
      return [];
    }

    if (!response.ok) {
      return [];
    }

    const payload = (await response.json()) as ServerInfoResponse;
    const regions = (payload.metaData ?? [])
      .filter((entry) => entry.value.startsWith("https://"))
      .filter((entry) => entry.key !== "gfn-regions" && !entry.key.startsWith("gfn-"))
      .map<StreamRegion>((entry) => ({
        name: entry.key,
        url: entry.value.endsWith("/") ? entry.value : `${entry.value}/`,
      }))
      .sort((a, b) => a.name.localeCompare(b.name));

    return regions;
  }

  async login(input: AuthLoginRequest): Promise<AuthSession> {
    const providers = await this.getProviders();
    const selected =
      providers.find((provider) => provider.idpId === input.providerIdpId) ??
      this.selectedProvider ??
      providers[0] ??
      defaultProvider();

    this.selectedProvider = normalizeProvider(selected);

    const { verifier, challenge } = generatePkce();
    const port = await findAvailablePort();
    const authUrl = buildAuthUrl(this.selectedProvider, challenge, port);

    const codePromise = waitForAuthorizationCode(port, 120000);
    await shell.openExternal(authUrl);
    const code = await codePromise;

    const initialTokens = await exchangeAuthorizationCode(code, verifier, port);
    const user = await fetchUserInfo(initialTokens);
    console.debug("auth: fetched user info during login", { userId: user.userId, email: user.email, avatarUrl: user.avatarUrl });
    let tokens = initialTokens;
    try {
      tokens = await this.ensureClientToken(initialTokens, user.userId);
    } catch (error) {
      console.warn("Unable to fetch client token after login. Falling back to OAuth token only:", error);
    }

    const nextSession: AuthSession = {
      provider: this.selectedProvider,
      tokens,
      user,
    };
    this.sessions.set(user.userId, nextSession);
    this.activeUserId = user.userId;
    this.selectedProvider = nextSession.provider;
    this.clearSubscriptionCache();
    this.clearVpcCache();

    // Fetch real membership tier from MES subscription API
    // (JWT does not contain gfn_tier, so fetchUserInfo always falls back to "FREE")
    await this.enrichUserTier();

    await this.persist();
    return this.getSession() as AuthSession;
  }

  async logout(): Promise<void> {
    if (!this.activeUserId) {
      return;
    }
    this.sessions.delete(this.activeUserId);
    this.activeUserId = this.sessions.keys().next().value ?? null;
    this.selectedProvider = this.getSession()?.provider ?? defaultProvider();
    this.cachedSubscription = null;
    this.clearVpcCache();
    await this.persist();
  }

  /**
   * Fetch subscription info for the current user.
   * Uses caching - call clearSubscriptionCache() to force refresh.
   */
  async getSubscription(): Promise<SubscriptionInfo | null> {
    // Return cached subscription if available
    if (this.cachedSubscription) {
      return this.cachedSubscription;
    }

    const session = await this.ensureValidSession();
    if (!session) {
      return null;
    }

    const token = session.tokens.idToken ?? session.tokens.accessToken;
    const userId = session.user.userId;

    // Fetch dynamic regions to get the VPC ID (handles Alliance partners correctly)
    const { vpcId } = await fetchDynamicRegions(token, session.provider.streamingServiceUrl);

    const subscription = await fetchSubscription(token, userId, vpcId ?? undefined);
    this.cachedSubscription = subscription;
    return subscription;
  }

  /**
   * Clear the cached subscription info.
   * Called automatically on logout.
   */
  clearSubscriptionCache(): void {
    this.cachedSubscription = null;
  }

  /**
   * Get the cached subscription without fetching.
   * Returns null if not cached.
   */
  getCachedSubscription(): SubscriptionInfo | null {
    return this.cachedSubscription;
  }

  /**
   * Get the VPC ID for the current provider.
   * Returns cached value if available, otherwise fetches from serverInfo endpoint.
   * The VPC ID is used for Alliance partner support and routing to correct data center.
   */
  async getVpcId(explicitToken?: string): Promise<string | null> {
    // Return cached VPC ID if available
    if (this.cachedVpcId) {
      return this.cachedVpcId;
    }

    const provider = this.getSelectedProvider();
    const base = provider.streamingServiceUrl.endsWith("/")
      ? provider.streamingServiceUrl
      : `${provider.streamingServiceUrl}/`;

    let token = explicitToken;
    if (!token) {
      const session = await this.ensureValidSession();
      token = session ? session.tokens.idToken ?? session.tokens.accessToken : undefined;
    }

    const headers: Record<string, string> = {
      Accept: "application/json",
      "nv-client-id": "ec7e38d4-03af-4b58-b131-cfb0495903ab",
      "nv-client-type": "BROWSER",
      "nv-client-version": "2.0.80.173",
      "nv-client-streamer": "WEBRTC",
      "nv-device-os": "WINDOWS",
      "nv-device-type": "DESKTOP",
      "User-Agent": GFN_USER_AGENT,
    };

    if (token) {
      headers.Authorization = `GFNJWT ${token}`;
    }

    try {
      const response = await fetch(`${base}v2/serverInfo`, {
        headers,
      });

      if (!response.ok) {
        return null;
      }

      const payload = (await response.json()) as ServerInfoResponse;
      const vpcId = payload.requestStatus?.serverId ?? null;

      // Cache the VPC ID
      if (vpcId) {
        this.cachedVpcId = vpcId;
      }

      return vpcId;
    } catch {
      return null;
    }
  }

  /**
   * Clear the cached VPC ID.
   * Called automatically on logout.
   */
  clearVpcCache(): void {
    this.cachedVpcId = null;
  }

  /**
   * Get the cached VPC ID without fetching.
   * Returns null if not cached.
   */
  getCachedVpcId(): string | null {
    return this.cachedVpcId;
  }

  /**
   * Enrich the current session's user with the real membership tier from MES API.
   * Falls back silently to the existing tier if the fetch fails.
   */
  private async enrichUserTier(): Promise<void> {
    const session = this.getSession();
    if (!session) return;

    try {
      const subscription = await this.getSubscription();
      if (subscription && subscription.membershipTier) {
        this.sessions.set(session.user.userId, {
          ...session,
          user: {
            ...session.user,
            membershipTier: subscription.membershipTier,
          },
        });
        console.log(`Resolved membership tier: ${subscription.membershipTier}`);
      }
    } catch (error) {
      console.warn("Failed to fetch subscription tier, keeping fallback:", error);
    }
  }

  private shouldRefresh(tokens: AuthTokens): boolean {
    return isNearExpiry(tokens.expiresAt, TOKEN_REFRESH_WINDOW_MS);
  }

  async ensureValidSessionWithStatus(
    forceRefresh = false,
    expectedUserId?: string,
  ): Promise<AuthSessionResult> {
    const currentSession = this.getSession();
    if (!currentSession) {
      return {
        session: null,
        refresh: {
          attempted: false,
          forced: forceRefresh,
          outcome: "not_attempted",
          message: "No saved session found.",
        },
      };
    }

    const userId = currentSession.user.userId;
    let tokens = currentSession.tokens;

    // Official GFN client flow relies on client_token-based refresh. Bootstrap it
    // for older sessions that were saved before we persisted client tokens.
    if (!tokens.clientToken && !isExpired(tokens.expiresAt)) {
      try {
        const withClientToken = await this.ensureClientToken(tokens, userId);
        if (withClientToken.clientToken && withClientToken.clientToken !== tokens.clientToken) {
          this.sessions.set(userId, {
            ...currentSession,
            tokens: withClientToken,
          });
          tokens = withClientToken;
          await this.persist();
        }
      } catch (error) {
        console.warn("Unable to bootstrap client token from saved session:", error);
      }
    }

    const shouldRefreshNow = forceRefresh || this.shouldRefresh(tokens);
    if (!shouldRefreshNow) {
      return {
        session: this.getSession(),
        refresh: {
          attempted: false,
          forced: forceRefresh,
          outcome: "not_attempted",
          message: "Session token is still valid.",
        },
      };
    }

    const applyRefreshedTokens = async (
      refreshedTokens: AuthTokens,
      source: "client_token" | "refresh_token",
    ): Promise<AuthSessionResult> => {
      const latestSession = this.getSession() ?? currentSession;
      let refreshedUser: AuthUser | null = null;
      let userInfoError: string | undefined;
      try {
        refreshedUser = await fetchUserInfo(refreshedTokens);
        console.debug("auth: fetched user info on token refresh", {
          userId: refreshedUser.userId,
          email: refreshedUser.email,
          avatarUrl: refreshedUser.avatarUrl,
        });
      } catch (error) {
        console.warn("Token refresh succeeded but user info refresh failed. Keeping cached user:", error);
        userInfoError = error instanceof Error ? error.message : "Unknown error while fetching user info";
      }

      if (expectedUserId && !refreshedUser) {
        return {
          session: latestSession,
          refresh: {
            attempted: true,
            forced: forceRefresh,
            outcome: "failed",
            message: "Token refresh could not verify the expected account identity.",
            error: userInfoError ?? `expected_user_id:${expectedUserId} user_info_unavailable`,
          },
        };
      }

      if (expectedUserId && refreshedUser && refreshedUser.userId !== expectedUserId) {
        return {
          session: latestSession,
          refresh: {
            attempted: true,
            forced: forceRefresh,
            outcome: "failed",
            message: "Token refresh returned a different account than expected.",
            error: `expected_user_id:${expectedUserId} actual_user_id:${refreshedUser.userId}`,
          },
        };
      }

      const resolvedUser = refreshedUser ?? latestSession.user;
      const updatedSession: AuthSession = {
        provider: latestSession.provider,
        tokens: refreshedTokens,
        user: resolvedUser,
      };
      this.sessions.set(updatedSession.user.userId, updatedSession);

      // Re-fetch real tier after token refresh
      this.clearSubscriptionCache();
      await this.enrichUserTier();
      await this.persist();

      const sourceText = source === "client_token" ? "client token" : "refresh token";
      return {
        session: this.getSession(),
        refresh: {
          attempted: true,
          forced: forceRefresh,
          outcome: "refreshed",
          message: forceRefresh
            ? `Saved session token refreshed via ${sourceText}.`
            : `Session token refreshed via ${sourceText} because it was near expiry.`,
        },
      };
    };

    const refreshErrors: string[] = [];

    if (tokens.clientToken) {
      try {
        const refreshedFromClientToken = await refreshWithClientToken(tokens.clientToken, userId);
        let refreshedTokens = mergeTokenSnapshot(tokens, refreshedFromClientToken);
        refreshedTokens = await this.ensureClientToken(refreshedTokens, userId);
        return applyRefreshedTokens(refreshedTokens, "client_token");
      } catch (error) {
        const message =
          error instanceof Error ? error.message : "Unknown error while refreshing with client token";
        refreshErrors.push(`client_token: ${message}`);
      }
    }

    if (tokens.refreshToken) {
      try {
        const refreshedOAuth = await refreshAuthTokens(tokens.refreshToken);
        let refreshedTokens: AuthTokens = {
          ...tokens,
          ...refreshedOAuth,
          // OAuth refresh does not always return a new client token.
          clientToken: tokens.clientToken,
          clientTokenExpiresAt: tokens.clientTokenExpiresAt,
          clientTokenLifetimeMs: tokens.clientTokenLifetimeMs,
        };
        refreshedTokens = await this.ensureClientToken(refreshedTokens, userId);
        return applyRefreshedTokens(refreshedTokens, "refresh_token");
      } catch (error) {
        const message =
          error instanceof Error ? error.message : "Unknown error while refreshing token";
        refreshErrors.push(`refresh_token: ${message}`);
      }
    }

    const errorText = refreshErrors.length > 0 ? refreshErrors.join(" | ") : undefined;
    const expired = isExpired(tokens.expiresAt);

    if (!tokens.clientToken && !tokens.refreshToken) {
      if (expired) {
        await this.logout();
        return {
          session: this.getSession(),
          refresh: {
            attempted: true,
            forced: forceRefresh,
            outcome: "missing_refresh_token",
            message: "Saved session expired and has no refresh mechanism. Please log in again.",
          },
        };
      }

      return {
        session: this.getSession(),
        refresh: {
          attempted: true,
          forced: forceRefresh,
          outcome: "missing_refresh_token",
          message: "No refresh token available. Using saved session token.",
        },
      };
    }

    if (expired) {
      await this.logout();
      return {
        session: this.getSession(),
        refresh: {
          attempted: true,
          forced: forceRefresh,
          outcome: "failed",
          message: "Token refresh failed and the saved session expired. Please log in again.",
          error: errorText,
        },
      };
    }

    return {
      session: this.getSession(),
      refresh: {
        attempted: true,
        forced: forceRefresh,
        outcome: "failed",
        message: "Token refresh failed. Using saved session token.",
        error: errorText,
      },
    };
  }

  async ensureValidSession(): Promise<AuthSession | null> {
    const result = await this.ensureValidSessionWithStatus(false);
    return result.session;
  }

  async resolveJwtToken(explicitToken?: string): Promise<string> {
    // Prefer the managed auth session whenever it exists so renderer-side cached
    // tokens cannot bypass refresh logic.
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
