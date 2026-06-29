import crypto from "node:crypto";

const INVALID_PROXY_MESSAGE =
  "Invalid session proxy URL. Use http://host:port, https://host:port, socks4://host:port, or socks5://host:port.";
const SUPPORTED_PROXY_PROTOCOLS = new Set(["http:", "https:", "socks4:", "socks5:"]);
const CLOUDMATCH_PROXY_PARTITION_PREFIX = "opennow:gfn-session-proxy";
const proxyPartitions = new Map<string, string>();

export function sessionProxyPartitionForUrl(normalizedProxyUrl: string): string {
  const existing = proxyPartitions.get(normalizedProxyUrl);
  if (existing) return existing;

  const partition = `${CLOUDMATCH_PROXY_PARTITION_PREFIX}:${crypto.randomUUID()}`;
  proxyPartitions.set(normalizedProxyUrl, partition);
  return partition;
}

export function normalizeSessionProxyUrl(raw?: string): string | null {
  const trimmed = raw?.trim() ?? "";
  if (!trimmed) return null;

  const candidate = /^[a-z][a-z0-9+.-]*:\/\//i.test(trimmed) ? trimmed : `http://${trimmed}`;
  let parsed: URL;
  try {
    parsed = new URL(candidate);
  } catch {
    throw new Error(INVALID_PROXY_MESSAGE);
  }

  if (!SUPPORTED_PROXY_PROTOCOLS.has(parsed.protocol) || !parsed.hostname || !parsed.port) {
    throw new Error(INVALID_PROXY_MESSAGE);
  }

  const username = parsed.username ? encodeURIComponent(decodeURIComponent(parsed.username)) : "";
  const password = parsed.password ? encodeURIComponent(decodeURIComponent(parsed.password)) : "";
  const credentials = username ? `${username}${password ? `:${password}` : ""}@` : "";
  return `${parsed.protocol}//${credentials}${parsed.host}`;
}
