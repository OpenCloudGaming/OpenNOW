import { createHash } from "node:crypto";

const GFN_ETS_API_BASE_URL = "https://api-prod.nvidia.com/services/ets/v1";
const STARFLEET_TOKEN_REFRESH_SKEW_MS = 60_000;
const STARFLEET_TOKEN_FALLBACK_TTL_MS = 5 * 60_000;

export const GFN_PAYWALL_API_BASE_URL = "https://api-prod.nvidia.com/gfn-paywall-api/api/v2";

interface PaywallResponseWithMessage {
  message?: unknown;
  errors?: {
    errorMessage?: unknown;
  };
}

interface StarfleetTokenResponse extends PaywallResponseWithMessage {
  token?: unknown;
}

interface CachedStarfleetToken {
  token: string;
  expiresAt: number;
}

const starfleetTokenCache = new Map<string, CachedStarfleetToken>();

export function buildPaywallHeaders(idToken: string): Record<string, string> {
  return {
    Accept: "application/json",
    "Content-Type": "application/json",
    idToken,
  };
}

export async function readPaywallJson(response: Response): Promise<unknown> {
  const text = await response.text();
  if (!text.trim()) {
    return null;
  }

  try {
    return JSON.parse(text) as unknown;
  } catch {
    return text;
  }
}

export function parsePaywallMessage(payload: unknown): string | undefined {
  if (!payload || typeof payload !== "object") {
    return undefined;
  }

  const response = payload as PaywallResponseWithMessage;
  const statuspageMessage = (payload as { error?: { message?: unknown } }).error?.message;
  const message = response.message ?? response.errors?.errorMessage ?? statuspageMessage;
  return typeof message === "string" && message.trim().length > 0 ? message : undefined;
}

export function tokenCandidates(idToken: string, alternates: string[] | undefined): string[] {
  return [...new Set([idToken, ...(alternates ?? [])].map((token) => token.trim()).filter(Boolean))];
}

function asString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : undefined;
}

function cacheKey(token: string): string {
  return createHash("sha256").update(token).digest("hex");
}

function decodeBase64Url(value: string): string {
  const normalized = value.replace(/-/g, "+").replace(/_/g, "/");
  const padding = normalized.length % 4;
  const padded = padding === 0 ? normalized : `${normalized}${"=".repeat(4 - padding)}`;
  return Buffer.from(padded, "base64").toString("utf8");
}

function parseJwtExpiration(token: string): number | undefined {
  const [, payload] = token.split(".");
  if (!payload) {
    return undefined;
  }

  try {
    const parsed = JSON.parse(decodeBase64Url(payload)) as { exp?: unknown };
    return typeof parsed.exp === "number" && Number.isFinite(parsed.exp) ? parsed.exp * 1000 : undefined;
  } catch {
    return undefined;
  }
}

function cacheExpiresAt(token: string): number {
  const jwtExpiration = parseJwtExpiration(token);
  const fallbackExpiration = Date.now() + STARFLEET_TOKEN_FALLBACK_TTL_MS;
  const expiration = jwtExpiration ?? fallbackExpiration;
  return Math.max(Date.now(), expiration - STARFLEET_TOKEN_REFRESH_SKEW_MS);
}

async function fetchStarfleetToken(sourceIdToken: string): Promise<string> {
  const key = cacheKey(sourceIdToken);
  const cached = starfleetTokenCache.get(key);
  if (cached && cached.expiresAt > Date.now()) {
    return cached.token;
  }

  const response = await fetch(`${GFN_ETS_API_BASE_URL}/generate/starfleet/token`, {
    headers: {
      Accept: "application/json",
      idToken: sourceIdToken,
    },
  });

  const payload = await readPaywallJson(response);
  if (!response.ok) {
    const message = parsePaywallMessage(payload) ?? `Starfleet token request failed with status ${response.status}`;
    throw new Error(message);
  }

  const token = asString((payload as StarfleetTokenResponse | null)?.token);
  if (!token) {
    throw new Error("Starfleet token response did not include a token");
  }

  starfleetTokenCache.set(key, {
    token,
    expiresAt: cacheExpiresAt(token),
  });
  return token;
}

export async function resolvePaywallTokenCandidates(
  idToken: string,
  alternates: string[] | undefined,
): Promise<string[]> {
  const resolved: string[] = [];

  for (const candidate of tokenCandidates(idToken, alternates)) {
    try {
      resolved.push(await fetchStarfleetToken(candidate));
    } catch (error) {
      const message = error instanceof Error ? error.message : "Unknown Starfleet token error";
      console.warn("Unable to resolve NVIDIA Starfleet token for paywall API:", message);
    }
    resolved.push(candidate);
  }

  return [...new Set(resolved)];
}

