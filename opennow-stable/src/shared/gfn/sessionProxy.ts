/**
 * Session-proxy URL normalization shared by main and renderer.
 * Keep protocol/port rules here so settings UI and fetch paths cannot drift.
 */

export const INVALID_SESSION_PROXY_URL_MESSAGE =
  "Invalid session proxy URL. Use http://host:port, https://host:port, socks4://host:port, or socks5://host:port.";

const SUPPORTED_PROXY_PROTOCOLS = new Set(["http:", "https:", "socks4:", "socks5:"]);

function safeDecodeURIComponent(value: string): string {
  try {
    return decodeURIComponent(value);
  } catch {
    throw new Error(INVALID_SESSION_PROXY_URL_MESSAGE);
  }
}

function resolveExplicitProxyPort(protocol: string, parsedPort: string): string | null {
  if (parsedPort) {
    return parsedPort;
  }

  // WHATWG URL strips default ports for http/https (`:80` / `:443` → ""),
  // so accept those schemes with an implicit default port.
  if (protocol === "http:") {
    return "80";
  }
  if (protocol === "https:") {
    return "443";
  }

  // socks4/socks5 are non-special schemes and never strip ports; require an explicit one.
  return null;
}

/**
 * Normalize a session proxy URL to `scheme://[user[:pass]@]host:port`.
 * Returns null for empty/whitespace input. Throws for invalid values.
 */
export function normalizeSessionProxyUrl(raw?: string): string | null {
  const trimmed = raw?.trim() ?? "";
  if (!trimmed) return null;

  const candidate = /^[a-z][a-z0-9+.-]*:\/\//i.test(trimmed) ? trimmed : `http://${trimmed}`;
  let parsed: URL;
  try {
    parsed = new URL(candidate);
  } catch {
    throw new Error(INVALID_SESSION_PROXY_URL_MESSAGE);
  }

  if (!SUPPORTED_PROXY_PROTOCOLS.has(parsed.protocol) || !parsed.hostname) {
    throw new Error(INVALID_SESSION_PROXY_URL_MESSAGE);
  }

  const port = resolveExplicitProxyPort(parsed.protocol, parsed.port);
  if (!port) {
    throw new Error(INVALID_SESSION_PROXY_URL_MESSAGE);
  }

  const username = parsed.username ? encodeURIComponent(safeDecodeURIComponent(parsed.username)) : "";
  const password = parsed.password ? encodeURIComponent(safeDecodeURIComponent(parsed.password)) : "";
  const credentials = username ? `${username}${password ? `:${password}` : ""}@` : "";
  return `${parsed.protocol}//${credentials}${parsed.hostname}:${port}`;
}

/** True when the value is empty or a normalizeable session proxy URL. */
export function isValidSessionProxyUrl(raw?: string): boolean {
  try {
    normalizeSessionProxyUrl(raw);
    return true;
  } catch {
    return false;
  }
}

/** True when `error` (or its message) is the invalid session-proxy validation failure. */
export function isInvalidSessionProxyUrlError(error: unknown): boolean {
  if (error instanceof Error) {
    return error.message.includes(INVALID_SESSION_PROXY_URL_MESSAGE)
      || error.message.includes("Invalid session proxy URL");
  }
  if (typeof error === "string") {
    return error.includes(INVALID_SESSION_PROXY_URL_MESSAGE)
      || error.includes("Invalid session proxy URL");
  }
  return false;
}
