import type { AuthTokens, AuthUser, LoginProvider } from "./gfn";

export const SERVICE_URLS_ENDPOINT = "https://pcs.geforcenow.com/v1/serviceUrls";
export const TOKEN_ENDPOINT = "https://login.nvidia.com/token";
export const CLIENT_TOKEN_ENDPOINT = "https://login.nvidia.com/client_token";
export const USERINFO_ENDPOINT = "https://login.nvidia.com/userinfo";
export const AUTH_ENDPOINT = "https://login.nvidia.com/authorize";
export const MES_URL = "https://mes.geforcenow.com/v4/subscriptions";
export const GFN_GRAPHQL_URL = "https://games.geforce.com/graphql";
export const LCARS_CLIENT_ID = "ec7e38d4-03af-4b58-b131-cfb0495903ab";
export const CLIENT_ID = "ZU7sPN-miLujMD95LfOQ453IB0AtjM8sMyvgJ9wCXEQ";
export const SCOPES = "openid consent email tk_client age";
export const DEFAULT_IDP_ID = "PDiAhv2kJTFeQ7WOPqiQ2tRZ7lGhR2X11dXvM4TZSxg";
export const GFN_CLIENT_VERSION = "2.0.80.173";
export const GFN_USER_AGENT =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36 NVIDIACEFClient/HEAD/debb5919f6 GFN-PC/2.0.80.173";
export const GFN_BROWSER_USER_AGENT =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/131.0.0.0 Safari/537.36";
export const DEFAULT_PROVIDER_STREAMING_URL = "https://prod.cloudmatchbeta.nvidiagrid.net/";
export const TOKEN_REFRESH_WINDOW_MS = 10 * 60 * 1000;
export const CLIENT_TOKEN_REFRESH_WINDOW_MS = 5 * 60 * 1000;

export function defaultProvider(): LoginProvider {
  return {
    idpId: DEFAULT_IDP_ID,
    code: "NVIDIA",
    displayName: "NVIDIA",
    streamingServiceUrl: DEFAULT_PROVIDER_STREAMING_URL,
    priority: 0,
  };
}

export function normalizeProvider(provider: LoginProvider): LoginProvider {
  return {
    ...provider,
    streamingServiceUrl: provider.streamingServiceUrl.endsWith("/")
      ? provider.streamingServiceUrl
      : `${provider.streamingServiceUrl}/`,
  };
}

export function decodeBase64Url(value: string): string {
  const normalized = value.replace(/-/g, "+").replace(/_/g, "/");
  const padding = normalized.length % 4;
  const padded = padding === 0 ? normalized : `${normalized}${"=".repeat(4 - padding)}`;
  return decodeURIComponent(Array.from(atob(padded), (char) => `%${char.charCodeAt(0).toString(16).padStart(2, "0")}`).join(""));
}

export function parseJwtPayload<T>(token: string): T | null {
  const parts = token.split(".");
  if (parts.length !== 3) return null;
  try {
    return JSON.parse(decodeBase64Url(parts[1])) as T;
  } catch {
    return null;
  }
}

export function toExpiresAt(expiresInSeconds: number | undefined, defaultSeconds = 86400): number {
  return Date.now() + (expiresInSeconds ?? defaultSeconds) * 1000;
}

export function isExpired(expiresAt: number | undefined): boolean {
  if (!expiresAt) return true;
  return expiresAt <= Date.now();
}

export function isNearExpiry(expiresAt: number | undefined, windowMs: number): boolean {
  if (!expiresAt) return true;
  return expiresAt - Date.now() < windowMs;
}

export function generatePkce(): { verifier: string; challenge: string } {
  throw new Error("Use a runtime-specific PKCE helper");
}

export function userFromJwt(tokens: AuthTokens): AuthUser | null {
  const jwtToken = tokens.idToken ?? tokens.accessToken;
  const parsed = parseJwtPayload<{
    sub?: string;
    email?: string;
    preferred_username?: string;
    gfn_tier?: string;
    picture?: string;
  }>(jwtToken);
  if (!parsed?.sub) return null;
  return {
    userId: parsed.sub,
    displayName: parsed.preferred_username ?? parsed.email?.split("@")[0] ?? "User",
    email: parsed.email,
    avatarUrl: parsed.picture,
    membershipTier: parsed.gfn_tier ?? "FREE",
  };
}
