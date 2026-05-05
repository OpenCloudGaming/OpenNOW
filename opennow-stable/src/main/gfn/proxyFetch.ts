import { session as electronSession } from "electron";

import { normalizeSessionProxyUrl } from "./proxyUrl";

const CLOUDMATCH_PROXY_PARTITION = "opennow:gfn-session-proxy";

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

  const proxySession = electronSession.fromPartition(CLOUDMATCH_PROXY_PARTITION, { cache: false }) as ElectronSessionWithFetch;
  await proxySession.setProxy({ proxyRules: normalizedProxyUrl });

  if (typeof proxySession.fetch === "function") {
    return proxySession.fetch(input, init);
  }

  throw new Error("Electron session fetch is unavailable for session proxy requests.");
}
