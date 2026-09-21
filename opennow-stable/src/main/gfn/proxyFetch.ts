import {
  isCloudflareWebSocketProxyUrl,
  normalizeSessionProxyUrl,
  sessionProxyPartitionForUrl,
} from "./proxyUrl";
import { localProxyForCloudflareTunnel } from "./websocketProxyBridge";

type ElectronSessionWithFetch = Electron.Session & {
  fetch?: typeof fetch;
};

export async function fetchWithOptionalProxy(
  input: string,
  init: RequestInit | undefined,
  proxyUrl?: string,
): Promise<Response> {
  const normalizedProxyUrl = normalizeSessionProxyUrl(proxyUrl);
  if (!normalizedProxyUrl) {
    return fetch(input, init);
  }

  const { session: electronSession } = await import("electron");
  const proxySession = electronSession.fromPartition(sessionProxyPartitionForUrl(normalizedProxyUrl), { cache: false }) as ElectronSessionWithFetch;
  const proxyRules = isCloudflareWebSocketProxyUrl(normalizedProxyUrl)
    ? await localProxyForCloudflareTunnel(normalizedProxyUrl)
    : normalizedProxyUrl;
  await proxySession.setProxy({ proxyRules });

  if (typeof proxySession.fetch === "function") {
    return proxySession.fetch(input, init);
  }

  throw new Error("Electron session fetch is unavailable for session proxy requests.");
}
