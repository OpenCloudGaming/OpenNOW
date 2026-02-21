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
  session: AuthSession | null;
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
    return {
      userId: parsed.sub,
      displayName: parsed.preferred_username ?? parsed.email?.split("@")[0] ?? "User",
      email: parsed.email,
      avatarUrl: parsed.picture,
      membershipTier: parsed.gfn_tier ?? "FREE",
    };
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

  return {
    userId: payload.sub,
    displayName: payload.preferred_username ?? payload.email?.split("@")[0] ?? "User",
    email: payload.email,
    avatarUrl: payload.picture,
    membershipTier: "FREE",
  };
}

export class AuthService {
  private providers: LoginProvider[] = [];
  private session: AuthSession | null = null;
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
      const parsed = JSON.parse(raw) as PersistedAuthState;
      if (parsed.selectedProvider) {
        this.selectedProvider = normalizeProvider(parsed.selectedProvider);
      }
      if (parsed.session) {
        this.session = {
          ...parsed.session,
          provider: normalizeProvider(parsed.session.provider),
        };

        // Refresh the real tier from MES API on session restore
        // (persisted tier may be stale or was "FREE" from JWT fallback)
        await this.enrichUserTier();
        await this.persist();
      }
    } catch {
      this.session = null;
      this.selectedProvider = defaultProvider();
      await this.persist();
    }
  }

  private async persist(): Promise<void> {
    const payload: PersistedAuthState = {
      session: this.session,
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

  getSession(): AuthSession | null {
    return this.session;
  }

  getSelectedProvider(): LoginProvider {
    return this.selectedProvider;
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
    let tokens = initialTokens;
    try {
      tokens = await this.ensureClientToken(initialTokens, user.userId);
    } catch (error) {
      console.warn("Unable to fetch client token after login. Falling back to OAuth token only:", error);
    }

    this.session = {
      provider: this.selectedProvider,
      tokens,
      user,
    };

    // Fetch real membership tier from MES subscription API
    // (JWT does not contain gfn_tier, so fetchUserInfo always falls back to "FREE")
    await this.enrichUserTier();

    await this.persist();
    return this.session;
  }

  async logout(): Promise<void> {
    this.session = null;
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
    const { vpcId } = await fetchDynamicRegions(token, this.selectedProvider.streamingServiceUrl);

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
    if (!this.session) return;

    try {
      const subscription = await this.getSubscription();
      if (subscription && subscription.membershipTier) {
        this.session = {
          ...this.session,
          user: {
            ...this.session.user,
            membershipTier: subscription.membershipTier,
          },
        };
        console.log(`Resolved membership tier: ${subscription.membershipTier}`);
      }
    } catch (error) {
      console.warn("Failed to fetch subscription tier, keeping fallback:", error);
    }
  }

  private shouldRefresh(tokens: AuthTokens): boolean {
    return isNearExpiry(tokens.expiresAt, TOKEN_REFRESH_WINDOW_MS);
  }

  async ensureValidSessionWithStatus(forceRefresh = false): Promise<AuthSessionResult> {
    if (!this.session) {
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

    const userId = this.session.user.userId;
    let tokens = this.session.tokens;

    // Official GFN client flow relies on client_token-based refresh. Bootstrap it
    // for older sessions that were saved before we persisted client tokens.
    if (!tokens.clientToken && !isExpired(tokens.expiresAt)) {
      try {
        const withClientToken = await this.ensureClientToken(tokens, userId);
        if (withClientToken.clientToken && withClientToken.clientToken !== tokens.clientToken) {
          this.session = {
            ...this.session,
            tokens: withClientToken,
          };
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
        session: this.session,
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
      let user = this.session?.user;
      try {
        user = await fetchUserInfo(refreshedTokens);
      } catch (error) {
        console.warn("Token refresh succeeded but user info refresh failed. Keeping cached user:", error);
      }

      this.session = {
        provider: this.session!.provider,
        tokens: refreshedTokens,
        user: user ?? this.session!.user,
      };

      // Re-fetch real tier after token refresh
      this.clearSubscriptionCache();
      await this.enrichUserTier();
      await this.persist();

      const sourceText = source === "client_token" ? "client token" : "refresh token";
      return {
        session: this.session,
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
          session: null,
          refresh: {
            attempted: true,
            forced: forceRefresh,
            outcome: "missing_refresh_token",
            message: "Saved session expired and has no refresh mechanism. Please log in again.",
          },
        };
      }

      return {
        session: this.session,
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
        session: null,
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
      session: this.session,
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
    if (this.session) {
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
