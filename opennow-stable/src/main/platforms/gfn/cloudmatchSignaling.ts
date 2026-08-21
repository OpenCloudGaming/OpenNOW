import type { IceServer, MediaConnectionInfo } from "@shared/gfn";

import type { CloudMatchResponse } from "./types";
import { collectRtspsEndpoints } from "./nvstRtspProbe";
import { isZoneHostname } from "./cloudmatchTransport";

const READY_SESSION_STATUSES = new Set([2, 3]);

export function isReadySessionStatus(status: number): boolean {
  return READY_SESSION_STATUSES.has(status);
}

export async function normalizeIceServers(response: CloudMatchResponse): Promise<IceServer[]> {
  const raw = response.session.iceServerConfiguration?.iceServers ?? [];
  const servers = raw
    .map((entry) => {
      const urls = Array.isArray(entry.urls) ? entry.urls : [entry.urls];
      return {
        urls,
        username: entry.username,
        credential: entry.credential,
      };
    })
    .filter((entry) => entry.urls.length > 0);

  if (servers.length > 0) return servers;

  return [
    { urls: ["stun:s1.stun.gamestream.nvidia.com:19308"] },
    { urls: ["stun:stun.l.google.com:19302"] },
    { urls: ["stun:stun1.l.google.com:19302"] },
  ];
}

/**
 * Extract the streaming server IP from the CloudMatch response, matching Rust's
 * `streaming_server_ip()` priority chain:
 *   1. connectionInfo[usage==14].ip (direct IP)
 *   2. Host extracted from connectionInfo[usage==14].resourcePath (for rtsps:// URLs)
 *   3. sessionControlInfo.ip (fallback)
 */
export function streamingServerIp(response: CloudMatchResponse): string | null {
  const connections = response.session.connectionInfo ?? [];
  const sigConn = connections.find((conn) => conn.usage === 14);

  if (sigConn) {
    // Priority 1: Direct IP field
    const rawIp = sigConn.ip;
    const directIp = Array.isArray(rawIp) ? rawIp[0] : rawIp;
    if (directIp && directIp.length > 0) {
      return directIp;
    }

    // Priority 2: Extract host from resourcePath (Alliance format: rtsps://host:port)
    if (sigConn.resourcePath) {
      const host = extractHostFromUrl(sigConn.resourcePath);
      if (host) return host;
    }
  }

  // Priority 3: sessionControlInfo.ip
  const controlIp = response.session.sessionControlInfo?.ip;
  if (controlIp && controlIp.length > 0) {
    return Array.isArray(controlIp) ? controlIp[0] : controlIp;
  }

  return null;
}

/**
 * Extract host from a URL string (handles rtsps://, rtsp://, wss://, https://).
 * Matches Rust's extract_host_from_url().
 */
export function extractHostFromUrl(url: string): string | null {
  const prefixes = ["rtsps://", "rtsp://", "wss://", "https://"];
  let afterProto: string | null = null;
  for (const prefix of prefixes) {
    if (url.startsWith(prefix)) {
      afterProto = url.slice(prefix.length);
      break;
    }
  }
  if (!afterProto) return null;

  // Get host (before port or path)
  const host = afterProto.split(":")[0]?.split("/")[0];
  if (!host || host.length === 0 || host.startsWith(".")) return null;
  return host;
}

export function resolveSignaling(response: CloudMatchResponse): {
  serverIp: string;
  signalingServer: string;
  signalingUrl: string;
  mediaConnectionInfo?: MediaConnectionInfo;
  rtspsEndpoints: string[];
} {
  const connections = response.session.connectionInfo ?? [];
  const signalingConnection =
    connections.find((conn) => conn.usage === 14 && conn.ip) ?? connections.find((conn) => conn.ip);

  // Use the Rust-matching priority chain for server IP
  const serverIp = streamingServerIp(response);
  if (!serverIp) {
    throw new Error("CloudMatch response did not include a signaling host");
  }

  const resourcePath = signalingConnection?.resourcePath ?? "/nvst/";

  // Build signaling URL matching Rust's build_signaling_url() behavior:
  // - rtsps://host:port -> extract host, convert to wss://host/nvst/
  // - wss://... -> use as-is
  // - /path -> wss://serverIp:443/path
  // - fallback -> wss://serverIp:443/nvst/
  const { signalingUrl, signalingHost } = buildSignalingUrl(resourcePath, serverIp);

  // Use the resolved signaling host (which may differ from serverIp if extracted from rtsps:// URL)
  const effectiveHost = signalingHost ?? serverIp;
  const signalingServer = effectiveHost.includes(":")
    ? effectiveHost
    : `${effectiveHost}:443`;

  const rtspsHost =
    connections
      .filter((connection) =>
        connection.usage === 16
        || connection.appLevelProtocol === 6
        || (typeof connection.resourcePath === "string" && /^rtsps?:\/\//i.test(connection.resourcePath)),
      )
      .map((connection) => connection.ip
        ?? (typeof connection.resourcePath === "string"
          ? extractHostFromUrl(connection.resourcePath)
          : null))
      .find((host): host is string => Boolean(host)) ??
    signalingHost ??
    (isZoneHostname(serverIp) ? null : serverIp);

  return {
    serverIp,
    signalingServer,
    signalingUrl,
    mediaConnectionInfo: resolveMediaConnectionInfo(connections, serverIp, {
      logMissing: isReadySessionStatus(response.session.status),
    }),
    rtspsEndpoints: collectRtspsEndpoints(connections, rtspsHost),
  };
}

/**
 * Resolve the media connection endpoint (IP + port) from the session's connectionInfo array.
 * This is the compatibility projection used by the WebRTC path:
 *   1. usage=2 (legacy VIDEO)
 *   2. usage=17 (BUNDLE)
 *
 * The native path receives the complete ordered connectionInfo array and must
 * select current native transports such as usage=15 (MEDIA) itself. Signaling
 * (14), MEDIA (15), and RTSPS (16) must not be repurposed as WebRTC ICE endpoints.
 *
 * For each entry, IP is extracted from:
 *   a. The .ip field directly
 *   b. The hostname in .resourcePath (e.g. rtsps://80-250-97-40.server.net:48322)
 * CloudMatch usage=14 is signaling and must never be repurposed as a media endpoint.
 */
export function resolveMediaConnectionInfo(
  connections: Array<{ ip?: string; port: number; usage: number; protocol?: number; resourcePath?: string }>,
  serverIp: string,
  options?: { logMissing?: boolean },
): { ip: string; port: number; usage: number } | undefined {
  // Helper: extract IP from a connection entry
  const extractIp = (conn: { ip?: string; resourcePath?: string }): string | null => {
    // Try direct IP field
    const rawIp = conn.ip;
    const directIp = Array.isArray(rawIp) ? rawIp[0] : rawIp;
    if (directIp && directIp.length > 0) return directIp;

    // Try hostname from resourcePath
    if (conn.resourcePath) {
      const host = extractHostFromUrl(conn.resourcePath);
      if (host) return host;
    }

    return null;
  };

  // Helper: extract port from a connection entry (fallback to resourcePath URL port)
  const extractPort = (conn: { port: number; resourcePath?: string }): number => {
    if (conn.port > 0) return conn.port;

    // Try extracting port from resourcePath URL
    if (conn.resourcePath) {
      try {
        const url = new URL(conn.resourcePath.replace("rtsps://", "https://").replace("rtsp://", "http://"));
        const portStr = url.port;
        if (portStr) return parseInt(portStr, 10);
      } catch {
        // Ignore
      }
    }

    return 0;
  };

  // Priority 1: usage=2 (legacy VIDEO)
  const primary = connections.find((c) => c.usage === 2);
  if (primary) {
    const ip = extractIp(primary);
    const port = extractPort(primary);
    console.log(`[CloudMatch] resolveMediaConnectionInfo: usage=2 candidate: ip=${ip}, port=${port}`);
    if (ip && port > 0) return { ip, port, usage: primary.usage };
  }

  // Priority 2: usage=17 (BUNDLE)
  const alt = connections.find((c) => c.usage === 17);
  if (alt) {
    const ip = extractIp(alt);
    const port = extractPort(alt);
    console.log(`[CloudMatch] resolveMediaConnectionInfo: usage=17 candidate: ip=${ip}, port=${port}`);
    if (ip && port > 0) return { ip, port, usage: alt.usage };
  }

  if (options?.logMissing ?? true) {
    console.log("[CloudMatch] resolveMediaConnectionInfo: NO valid media connection info found");
  }
  return undefined;
}

/**
 * Build signaling WSS URL from the resourcePath, matching Rust implementation.
 * Returns the URL and optionally the extracted host (if different from serverIp).
 */
export function buildSignalingUrl(
  raw: string,
  serverIp: string,
): { signalingUrl: string; signalingHost: string | null } {
  if (raw.startsWith("rtsps://") || raw.startsWith("rtsp://")) {
    const signalingUrl = `wss://${raw.slice(raw.indexOf("://") + 3)}`;
    try {
      const parsed = new URL(signalingUrl);
      if (!parsed.hostname || parsed.hostname.startsWith(".")) throw new Error("invalid host");
      return {
        signalingUrl,
        signalingHost: parsed.host,
      };
    } catch {
      return {
        signalingUrl: `wss://${serverIp}:443/nvst/`,
        signalingHost: null,
      };
    }
  }

  if (raw.startsWith("wss://")) {
    // Already a full WSS URL, use as-is; extract host
    try {
      return { signalingUrl: raw, signalingHost: new URL(raw).host };
    } catch {
      return { signalingUrl: raw, signalingHost: null };
    }
  }

  if (raw.startsWith("/")) {
    // Relative path
    return {
      signalingUrl: `wss://${serverIp}:443${raw}`,
      signalingHost: null,
    };
  }

  // Fallback
  return {
    signalingUrl: `wss://${serverIp}:443/nvst/`,
    signalingHost: null,
  };
}
