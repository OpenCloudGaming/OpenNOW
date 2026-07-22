import { ProxyAgent, fetch as undiciFetch } from "undici";

import {
  normalizeSessionProxyUrl,
  parseSessionProxyForElectron,
  sessionProxyEndpointKey,
  sessionProxyPartitionForUrl,
} from "./proxyUrl";

type ElectronSessionWithFetch = Electron.Session & {
  fetch?: typeof fetch;
};

const httpProxyAgents = new Map<string, ProxyAgent>();
const proxyCredentialsByEndpoint = new Map<string, { username: string; password: string }>();
let proxyLoginHandlerRegistered = false;

function getHttpProxyAgent(normalizedProxyUrl: string): ProxyAgent {
  const existing = httpProxyAgents.get(normalizedProxyUrl);
  if (existing) {
    return existing;
  }

  const agent = new ProxyAgent(normalizedProxyUrl);
  httpProxyAgents.set(normalizedProxyUrl, agent);
  return agent;
}

function registerSessionProxyCredentials(
  host: string,
  port: number | string,
  credentials: { username: string; password: string },
): void {
  proxyCredentialsByEndpoint.set(sessionProxyEndpointKey(host, port), credentials);
}

function ensureProxyLoginHandler(): void {
  if (proxyLoginHandlerRegistered) {
    return;
  }
  proxyLoginHandlerRegistered = true;

  const { app } = require("electron") as typeof import("electron");
  app.on("login", (event, _webContents, _details, authInfo, callback) => {
    if (!authInfo.isProxy) {
      return;
    }

    const credentials = proxyCredentialsByEndpoint.get(
      sessionProxyEndpointKey(authInfo.host, authInfo.port),
    );
    if (!credentials) {
      return;
    }

    event.preventDefault();
    callback(credentials.username, credentials.password);
  });
}

async function fetchWithElectronSessionProxy(
  input: string,
  init: RequestInit | undefined,
  proxyConfig: NonNullable<ReturnType<typeof parseSessionProxyForElectron>>,
): Promise<Response> {
  ensureProxyLoginHandler();

  const parsed = new URL(proxyConfig.normalizedUrl);
  if (proxyConfig.credentials) {
    registerSessionProxyCredentials(parsed.hostname, parsed.port, proxyConfig.credentials);
  }

  const { session: electronSession } = await import("electron");
  const proxySession = electronSession.fromPartition(
    sessionProxyPartitionForUrl(proxyConfig.normalizedUrl),
    { cache: false },
  ) as ElectronSessionWithFetch;
  await proxySession.setProxy({
    mode: "fixed_servers",
    proxyRules: proxyConfig.proxyRules,
  });

  if (typeof proxySession.fetch !== "function") {
    throw new Error("Electron session fetch is unavailable for session proxy requests.");
  }

  return proxySession.fetch(input, init);
}

export function initSessionProxyAuth(): void {
  ensureProxyLoginHandler();
}

export async function fetchWithOptionalProxy(
  input: string,
  init: RequestInit | undefined,
  proxyUrl?: string,
): Promise<Response> {
  const normalizedProxyUrl = normalizeSessionProxyUrl(proxyUrl);
  if (!normalizedProxyUrl) {
    return fetch(input, init);
  }

  const protocol = new URL(normalizedProxyUrl).protocol;
  if (protocol === "http:" || protocol === "https:") {
    const agent = getHttpProxyAgent(normalizedProxyUrl);
    return undiciFetch(input, {
      ...(init ?? {}),
      dispatcher: agent,
    } as Parameters<typeof undiciFetch>[1]) as unknown as Response;
  }

  const proxyConfig = parseSessionProxyForElectron(normalizedProxyUrl);
  if (!proxyConfig) {
    return fetch(input, init);
  }

  return fetchWithElectronSessionProxy(input, init, proxyConfig);
}
