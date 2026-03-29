import { createHash, randomBytes } from "node:crypto";
import net from "node:net";
import os from "node:os";

import type { LoginProvider } from "@shared/gfn";

export const AUTH_ENDPOINT = "https://login.nvidia.com/authorize";
export const CLIENT_ID = "ZU7sPN-miLujMD95LfOQ453IB0AtjM8sMyvgJ9wCXEQ";
export const SCOPES = "openid consent email tk_client age";
export const REDIRECT_PORTS = [2259, 6460, 7119, 8870, 9096];

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
  return Buffer.from(padded, "base64").toString("utf8");
}

export function parseJwtPayload<T>(token: string): T | null {
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

export function toExpiresAt(expiresInSeconds: number | undefined, defaultSeconds = 86400): number {
  return Date.now() + (expiresInSeconds ?? defaultSeconds) * 1000;
}

export function isExpired(expiresAt: number | undefined): boolean {
  if (!expiresAt) {
    return true;
  }
  return expiresAt <= Date.now();
}

export function isNearExpiry(expiresAt: number | undefined, windowMs: number): boolean {
  if (!expiresAt) {
    return true;
  }
  return expiresAt - Date.now() < windowMs;
}

export function generateDeviceId(host = os.hostname(), username = os.userInfo().username): string {
  return createHash("sha256").update(`${host}:${username}:opennow-stable`).digest("hex");
}

export function generatePkce(): { verifier: string; challenge: string } {
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

export interface BuildAuthUrlOptions {
  challenge: string;
  port: number;
  nonce?: string;
  deviceId?: string;
}

export function buildAuthUrl(provider: LoginProvider, options: BuildAuthUrlOptions): string {
  const redirectUri = `http://localhost:${options.port}`;
  const nonce = options.nonce ?? randomBytes(16).toString("hex");
  const params = new URLSearchParams({
    response_type: "code",
    device_id: options.deviceId ?? generateDeviceId(),
    scope: SCOPES,
    client_id: CLIENT_ID,
    redirect_uri: redirectUri,
    ui_locales: "en_US",
    nonce,
    prompt: "select_account",
    code_challenge: options.challenge,
    code_challenge_method: "S256",
    idp_id: provider.idpId,
  });
  return `${AUTH_ENDPOINT}?${params.toString()}`;
}

export type PortAvailabilityCheck = (port: number) => Promise<boolean>;

export async function isPortAvailable(port: number): Promise<boolean> {
  return new Promise((resolve) => {
    const server = net.createServer();
    server.once("error", () => resolve(false));
    server.once("listening", () => {
      server.close(() => resolve(true));
    });
    server.listen(port, "127.0.0.1");
  });
}

export async function findAvailablePort(
  ports: readonly number[] = REDIRECT_PORTS,
  checkPort: PortAvailabilityCheck = isPortAvailable,
): Promise<number> {
  for (const port of ports) {
    if (await checkPort(port)) {
      return port;
    }
  }

  throw new Error("No available OAuth callback ports");
}
