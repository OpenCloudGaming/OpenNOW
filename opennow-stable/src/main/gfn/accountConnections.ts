import { createHash } from "node:crypto";
import { createServer } from "node:http";
import net from "node:net";

import { shell } from "electron";

import type {
  AuthSession,
  GameAccountConnection,
  GameAccountConnectionsResult,
  GameAccountOperationResult,
} from "@shared/gfn";
import { cacheManager } from "../services/cacheManager";
import { getAccountGamesCacheKeys } from "./games";
import { buildGfnGraphQlHeaders, GFN_PLAY_ORIGIN, GFN_PLAY_REFERER, GFN_USER_AGENT } from "./clientHeaders";

const LCARS_GRAPHQL_URL = "https://apps.gxn.nvidia.com/graphql";
const STATIC_APP_DATA_QUERY_HASH = "d4117df5319f644c984945715ded9574bb074107eb02e97be17605b5f14c33ba";
const USER_ACCOUNT_QUERY_HASH = "39fa5dbf8c14ac4c873857fd510f337cdc8710d5614038a0625487d41f98986b";

const ALS_BASE_URL = "https://als.geforcenow.com/v1";
const ALS_CLIENT_ID = "gfn-pc";
const ALS_REDIRECT_FINISHED_URL = "https://static-als.nvidia.com/result";
const REDIRECT_PORTS = [2259, 6460, 7119, 8870, 9096];
const ACCOUNT_LINK_TIMEOUT_MS = 5 * 60 * 1000;
const STATIC_DATA_CACHE_MS = 30 * 60 * 1000;
const SYNC_REFRESH_WAIT_MS = 10_000;
const DISCONNECT_REFRESH_WAIT_MS = 5_000;
const POST_LOGIN_REFRESH_WAIT_MS = 1_000;

interface AccountFeature {
  __typename?: string;
  supported?: boolean;
  displayProposition?: string;
}

interface AppStoreDefinition {
  store: string;
  label?: string;
  sortOrder?: number;
  smallImageUrl?: string;
  features?: AccountFeature[];
  accountLinkingMetadata?: {
    isSupported?: boolean;
    isRequired?: boolean;
    label?: string;
  };
}

interface StaticAppDataResponse {
  data?: {
    appStoreDefinitions?: AppStoreDefinition[];
  };
  errors?: Array<{ message?: string }>;
}

interface AccountSyncingData {
  totalNumberOfSyncedGfnGames?: number;
  syncState?: string;
  syncDate?: string;
}

interface AccountLinkingData {
  userDisplayName?: string;
  expiresIn?: string;
  userIdentifier?: string;
  accountSyncingData?: AccountSyncingData;
}

interface StoreAccountData {
  store?: string;
  accountLinkingData?: AccountLinkingData;
}

interface UserAccountData {
  subscriptions?: Array<{ id?: string }>;
  storesData?: StoreAccountData[];
}

interface UserAccountResponse {
  data?: {
    userAccount?: UserAccountData;
  };
  errors?: Array<{ message?: string }>;
}

interface AlsLoginUrlResponse {
  login_url?: string;
}

interface AccountLinkLoginResult {
  platform: string;
  displayName?: string;
  expiresIn?: string;
}

let staticDefinitionsCache: { definitions: AppStoreDefinition[]; fetchedAt: number } | null = null;

const fallbackDefinitions: AppStoreDefinition[] = [
  {
    store: "UPLAY",
    label: "Ubisoft Connect",
    sortOrder: 100,
    features: [
      { __typename: "AccountLinkingSso", supported: true },
      { __typename: "AccountGamesSyncing", supported: true },
    ],
    accountLinkingMetadata: { isSupported: true, isRequired: true, label: "Ubisoft" },
  },
  {
    store: "BATTLENET",
    label: "Battle.net",
    sortOrder: 101,
    features: [
      { __typename: "AccountLinkingSso", supported: true },
      { __typename: "AccountGamesSyncing", supported: true },
    ],
    accountLinkingMetadata: { isSupported: true, isRequired: true, label: "Battle.net" },
  },
  {
    store: "EPIC",
    label: "Epic Games Store",
    sortOrder: 104,
    features: [
      { __typename: "AccountLinkingSso", supported: true },
      { __typename: "AccountGamesSyncing", supported: false },
    ],
    accountLinkingMetadata: { isSupported: true, isRequired: true, label: "Epic Games" },
  },
  {
    store: "GAIJIN",
    label: "Gaijin.net",
    sortOrder: 105,
    features: [
      { __typename: "AccountLinkingSso", supported: true },
      { __typename: "AccountGamesSyncing", supported: true },
    ],
    accountLinkingMetadata: { isSupported: true, isRequired: true, label: "Gaijin.net" },
  },
  {
    store: "STEAM",
    label: "Steam",
    sortOrder: 108,
    features: [
      { __typename: "AccountLinkingSso", supported: false },
      { __typename: "AccountGamesSyncing", supported: true },
    ],
    accountLinkingMetadata: { isSupported: false, isRequired: false, label: "Steam" },
  },
  {
    store: "XBOX",
    label: "Xbox",
    sortOrder: 120,
    features: [
      { __typename: "AccountLinkingSso", supported: true },
      { __typename: "AccountGamesSyncing", supported: true },
    ],
    accountLinkingMetadata: { isSupported: true, isRequired: true, label: "Xbox" },
  },
];

function normalizeProviderCode(provider: string): string {
  const normalized = provider.trim().toUpperCase().replace(/[\s-]+/g, "_");
  switch (normalized) {
    case "UBISOFT":
    case "UBISOFT_CONNECT":
      return "UPLAY";
    case "BATTLE_NET":
    case "BLIZZARD":
      return "BATTLENET";
    case "EPIC_GAMES":
    case "EPIC_GAMES_STORE":
      return "EPIC";
    default:
      return normalized;
  }
}

function stableHashedUserId(value: string): string {
  return createHash("sha256").update(value).digest("hex");
}

function getSessionIdToken(session: AuthSession): string {
  const token = session.tokens.idToken ?? session.tokens.accessToken;
  if (!token) {
    throw new Error("No authenticated token available");
  }
  return token;
}

function buildPersistedQueryParams(requestType: string, sha256Hash: string, variables?: Record<string, unknown>): URLSearchParams {
  const params = new URLSearchParams({
    requestType,
    extensions: JSON.stringify({
      persistedQuery: {
        sha256Hash,
      },
    }),
  });
  if (variables) {
    params.set("variables", JSON.stringify(variables));
  }
  return params;
}

function buildLcarsHeaders(token?: string): Record<string, string> {
  return {
    ...buildGfnGraphQlHeaders(token),
    "Content-Type": "application/graphql",
  };
}

function throwGraphQlErrors(errors: Array<{ message?: string }> | undefined, context: string): void {
  if (errors?.length) {
    throw new Error(`${context}: ${errors.map((error) => error.message ?? "Unknown error").join(", ")}`);
  }
}

async function fetchStaticAccountProviderDefinitions(): Promise<AppStoreDefinition[]> {
  const now = Date.now();
  if (staticDefinitionsCache && now - staticDefinitionsCache.fetchedAt < STATIC_DATA_CACHE_MS) {
    return staticDefinitionsCache.definitions;
  }

  const params = buildPersistedQueryParams("staticAppData", STATIC_APP_DATA_QUERY_HASH, {
    locale: "en_US",
    stringsKey: [""],
  });

  try {
    const response = await fetch(`${LCARS_GRAPHQL_URL}?${params.toString()}`, {
      headers: buildLcarsHeaders(),
    });
    if (!response.ok) {
      const text = await response.text();
      throw new Error(`Static account providers failed (${response.status}): ${text.slice(0, 400)}`);
    }

    const payload = (await response.json()) as StaticAppDataResponse;
    throwGraphQlErrors(payload.errors, "Static account providers failed");
    const definitions = (payload.data?.appStoreDefinitions ?? []).filter((definition) => {
      const support = getProviderFeatureSupport(definition);
      return !["UNKNOWN", "NONE"].includes(normalizeProviderCode(definition.store)) && (support.supportsLinking || support.supportsSync);
    });
    if (definitions.length > 0) {
      staticDefinitionsCache = { definitions, fetchedAt: now };
      return definitions;
    }
  } catch (error) {
    console.warn("[AccountConnections] Falling back to bundled provider definitions:", error);
  }

  return fallbackDefinitions;
}

async function fetchUserAccount(session: AuthSession): Promise<UserAccountData | undefined> {
  const token = getSessionIdToken(session);
  const params = buildPersistedQueryParams("userAccount", USER_ACCOUNT_QUERY_HASH);
  params.set("huId", stableHashedUserId(session.user.userId));

  const response = await fetch(`${LCARS_GRAPHQL_URL}?${params.toString()}`, {
    headers: buildLcarsHeaders(token),
  });
  if (!response.ok) {
    const text = await response.text();
    throw new Error(`User account stores failed (${response.status}): ${text.slice(0, 400)}`);
  }

  const payload = (await response.json()) as UserAccountResponse;
  throwGraphQlErrors(payload.errors, "User account stores failed");
  return payload.data?.userAccount;
}

function getProviderFeatureSupport(definition: AppStoreDefinition): { supportsLinking: boolean; supportsSync: boolean } {
  const features = definition.features ?? [];
  return {
    supportsLinking: features.some((feature) => feature.__typename === "AccountLinkingSso" && feature.supported === true),
    supportsSync: features.some((feature) => feature.__typename === "AccountGamesSyncing" && feature.supported === true),
  };
}

function parseExpiresIn(expiresIn: string | undefined): number | null {
  if (!expiresIn) {
    return null;
  }
  const seconds = Number.parseInt(expiresIn, 10);
  return Number.isFinite(seconds) ? seconds : null;
}

function buildAccountConnection(
  definition: AppStoreDefinition,
  storeData: StoreAccountData | undefined,
  fetchedAt: number,
): GameAccountConnection {
  const provider = normalizeProviderCode(definition.store);
  const support = getProviderFeatureSupport(definition);
  const linkingData = storeData?.accountLinkingData;
  const expiresInSeconds = parseExpiresIn(linkingData?.expiresIn);
  const expiresAt = expiresInSeconds !== null && expiresInSeconds >= 0 ? fetchedAt + expiresInSeconds * 1000 : undefined;
  const isConnected = Boolean(storeData?.store);
  const syncState = linkingData?.accountSyncingData?.syncState;
  const isExpired = isConnected && support.supportsLinking && expiresAt !== undefined && expiresAt <= fetchedAt;
  const hasSyncError = isConnected && support.supportsSync && Boolean(syncState) && syncState !== "SYNC_SUCCESS";

  return {
    provider,
    label: definition.accountLinkingMetadata?.label ?? definition.label ?? provider,
    sortOrder: definition.sortOrder ?? 999,
    iconUrl: definition.smallImageUrl,
    supportsLinking: support.supportsLinking,
    supportsSync: support.supportsSync,
    isRequired: definition.accountLinkingMetadata?.isRequired === true,
    isConnected,
    status: !isConnected ? "not_connected" : isExpired ? "expired" : hasSyncError ? "sync_error" : "connected",
    displayName: linkingData?.userDisplayName,
    userIdentifier: linkingData?.userIdentifier,
    expiresIn: linkingData?.expiresIn,
    expiresAt,
    syncState,
    syncDate: linkingData?.accountSyncingData?.syncDate,
    syncedGames: linkingData?.accountSyncingData?.totalNumberOfSyncedGfnGames ?? 0,
  };
}

function buildConnections(
  definitions: AppStoreDefinition[],
  storesData: StoreAccountData[] | undefined,
  fetchedAt: number,
): GameAccountConnection[] {
  const storesByProvider = new Map<string, StoreAccountData>();
  for (const storeData of storesData ?? []) {
    if (storeData.store) {
      storesByProvider.set(normalizeProviderCode(storeData.store), storeData);
    }
  }

  return definitions
    .map((definition) =>
      buildAccountConnection(definition, storesByProvider.get(normalizeProviderCode(definition.store)), fetchedAt),
    )
    .sort((left, right) => left.sortOrder - right.sortOrder || left.label.localeCompare(right.label));
}

async function resolveProviderDefinition(provider: string): Promise<AppStoreDefinition> {
  const normalized = normalizeProviderCode(provider);
  const definitions = await fetchStaticAccountProviderDefinitions();
  const definition = definitions.find((candidate) => normalizeProviderCode(candidate.store) === normalized);
  if (!definition) {
    throw new Error(`Unsupported game account provider: ${provider}`);
  }
  return definition;
}

async function fetchConnectionsForSession(session: AuthSession): Promise<GameAccountConnectionsResult> {
  const [definitions, userAccount] = await Promise.all([
    fetchStaticAccountProviderDefinitions(),
    fetchUserAccount(session),
  ]);
  const fetchedAt = Date.now();
  return {
    accounts: buildConnections(definitions, userAccount?.storesData, fetchedAt),
    fetchedAt,
  };
}

function wait(durationMs: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, durationMs);
  });
}

function findAvailablePort(ports: readonly number[]): Promise<number> {
  return new Promise((resolve, reject) => {
    const remaining = [...ports];

    const tryNext = (): void => {
      const port = remaining.shift();
      if (!port) {
        reject(new Error("No available account-linking callback ports"));
        return;
      }

      const tester = net.createServer();
      tester.once("error", () => {
        tryNext();
      });
      tester.once("listening", () => {
        tester.close(() => resolve(port));
      });
      tester.listen(port, "127.0.0.1");
    };

    tryNext();
  });
}

function finalRedirectUrl(result: Partial<AccountLinkLoginResult> & { error?: string }, provider: string): string {
  const url = new URL(ALS_REDIRECT_FINISHED_URL);
  url.searchParams.set("platform", result.platform ? normalizeProviderCode(result.platform) : provider);
  url.searchParams.set("ui_locales", "en_US");
  if (result.displayName) {
    url.searchParams.set("display_name", result.displayName);
  }
  if (result.error) {
    url.searchParams.set("error", result.error);
  }
  return url.toString();
}

function parseLoginCallback(callbackUrl: URL, expectedProvider: string): AccountLinkLoginResult {
  const error = callbackUrl.searchParams.get("error");
  if (error) {
    throw new Error(callbackUrl.searchParams.get("error_description") ?? error);
  }

  const platform = callbackUrl.searchParams.get("platform");
  if (!platform) {
    throw new Error("Account-linking callback did not include a provider");
  }
  const normalizedPlatform = normalizeProviderCode(platform);
  if (normalizedPlatform !== expectedProvider) {
    throw new Error(`Account-linking callback provider mismatch: expected ${expectedProvider}, received ${normalizedPlatform}`);
  }

  return {
    platform: normalizedPlatform,
    displayName: callbackUrl.searchParams.get("display_name") ?? undefined,
    expiresIn: callbackUrl.searchParams.get("expires_in") ?? undefined,
  };
}

function startAccountLinkCallbackServer(
  port: number,
  provider: string,
): { close: () => void; ready: Promise<void>; result: Promise<AccountLinkLoginResult> } {
  let resolveReady!: () => void;
  let rejectReady!: (error: Error) => void;
  let completed = false;
  let server: ReturnType<typeof createServer> | null = null;
  const ready = new Promise<void>((resolve, reject) => {
    resolveReady = resolve;
    rejectReady = reject;
  });

  const result = new Promise<AccountLinkLoginResult>((resolve, reject) => {
    const callbackServer = createServer((request, response) => {
      const callbackUrl = new URL(request.url ?? "/", `http://localhost:${port}`);
      const hasAccountLinkingPayload =
        callbackUrl.searchParams.has("platform") ||
        callbackUrl.searchParams.has("error") ||
        callbackUrl.searchParams.has("display_name") ||
        callbackUrl.searchParams.has("expires_in");

      if (!hasAccountLinkingPayload) {
        response.statusCode = 204;
        response.end();
        return;
      }

      let result: AccountLinkLoginResult;
      try {
        result = parseLoginCallback(callbackUrl, provider);
        response.statusCode = 302;
        response.setHeader("Location", finalRedirectUrl(result, provider));
        response.end();
      } catch (error) {
        response.statusCode = 302;
        response.setHeader("Location", finalRedirectUrl({ error: "accountlink_fail" }, provider));
        response.end();
        callbackServer.close(() => {
          if (!completed) {
            completed = true;
            reject(error);
          }
        });
        return;
      }

      callbackServer.close(() => {
        if (!completed) {
          completed = true;
          resolve(result);
        }
      });
    });

    const timer = setTimeout(() => {
      callbackServer.close(() => {
        if (!completed) {
          completed = true;
          reject(new Error("Timed out waiting for account-linking callback"));
        }
      });
    }, ACCOUNT_LINK_TIMEOUT_MS);

    server = callbackServer;

    callbackServer.once("error", (error) => {
      clearTimeout(timer);
      rejectReady(error);
      if (!completed) {
        completed = true;
        reject(error);
      }
    });

    callbackServer.once("close", () => {
      clearTimeout(timer);
    });

    callbackServer.listen(port, "127.0.0.1", () => {
      resolveReady();
    });
  });

  return {
    close: () => {
      if (server?.listening && !completed) {
        completed = true;
        server.close();
      }
    },
    ready,
    result,
  };
}

function buildAlsHeaders(token: string, contentType?: string): Record<string, string> {
  return {
    Accept: "application/json, text/plain, */*",
    ...(contentType ? { "Content-Type": contentType } : {}),
    Authorization: `Bearer ${token}`,
    Origin: GFN_PLAY_ORIGIN,
    Referer: GFN_PLAY_REFERER,
    "User-Agent": GFN_USER_AGENT,
  };
}

async function getLoginUrl(provider: string, port: number, token: string): Promise<string> {
  const redirectUri = `http://localhost:${port}/`;
  const params = new URLSearchParams({
    platform: provider,
    redirect_uri: redirectUri,
    client_id: ALS_CLIENT_ID,
  });

  const response = await fetch(`${ALS_BASE_URL}/login_url?${params.toString()}`, {
    headers: buildAlsHeaders(token),
  });
  if (!response.ok) {
    const text = await response.text();
    throw new Error(`Account-linking URL failed (${response.status}): ${text.slice(0, 400)}`);
  }

  const payload = (await response.json()) as AlsLoginUrlResponse;
  if (!payload.login_url) {
    throw new Error("Account-linking URL response did not include login_url");
  }
  return payload.login_url;
}

async function requestProviderSync(provider: string, token: string): Promise<void> {
  const response = await fetch(`${ALS_BASE_URL}/sync/${provider}`, {
    method: "POST",
    headers: buildAlsHeaders(token, "application/json"),
    body: JSON.stringify({}),
  });
  if (response.status !== 202) {
    const text = await response.text();
    throw new Error(`Account sync failed (${response.status}): ${text.slice(0, 400)}`);
  }
}

async function deleteProviderLink(provider: string, token: string): Promise<void> {
  const response = await fetch(`${ALS_BASE_URL}/linking/${provider}`, {
    method: "DELETE",
    headers: buildAlsHeaders(token),
  });
  if (!response.ok && response.status !== 404) {
    const text = await response.text();
    throw new Error(`Account unlink failed (${response.status}): ${text.slice(0, 400)}`);
  }
}

async function invalidateAccountGameCaches(session: AuthSession, proxyUrl?: string): Promise<void> {
  const cacheKeySets = [getAccountGamesCacheKeys(session.user.userId, session.provider.streamingServiceUrl)];
  if (proxyUrl?.trim()) {
    try {
      cacheKeySets.push(getAccountGamesCacheKeys(session.user.userId, session.provider.streamingServiceUrl, proxyUrl));
    } catch (error) {
      console.warn("[AccountConnections] Skipping proxy-scoped game cache invalidation:", error);
    }
  }

  const invalidations = new Map<string, Promise<void>>();
  for (const keys of cacheKeySets) {
    invalidations.set(keys.main, cacheManager.invalidateCache(keys.main));
    invalidations.set(keys.library, cacheManager.invalidateCache(keys.library));
    invalidations.set(keys.catalogPrefix, cacheManager.invalidateCachesByPrefix(keys.catalogPrefix));
  }
  await Promise.allSettled(invalidations.values());
}

function mergeLoginResult(
  result: GameAccountConnectionsResult,
  loginResult: AccountLinkLoginResult,
): GameAccountConnectionsResult {
  const provider = normalizeProviderCode(loginResult.platform);
  let mergedAccount: GameAccountConnection | null = null;
  const accounts = result.accounts.map((account) => {
    if (account.provider !== provider || (account.isConnected && account.status !== "expired")) {
      return account;
    }
    const expiresInSeconds = parseExpiresIn(loginResult.expiresIn);
    mergedAccount = {
      ...account,
      isConnected: true,
      status: "connected",
      displayName: loginResult.displayName ?? account.displayName,
      expiresIn: loginResult.expiresIn ?? account.expiresIn,
      expiresAt: expiresInSeconds !== null && expiresInSeconds >= 0 ? result.fetchedAt + expiresInSeconds * 1000 : account.expiresAt,
    };
    return mergedAccount;
  });

  return mergedAccount ? { ...result, accounts } : result;
}

function accountForProvider(result: GameAccountConnectionsResult, provider: string): GameAccountConnection | undefined {
  const normalized = normalizeProviderCode(provider);
  return result.accounts.find((account) => account.provider === normalized);
}

export async function fetchGameAccountConnections(session: AuthSession): Promise<GameAccountConnectionsResult> {
  return fetchConnectionsForSession(session);
}

export async function linkGameAccount(session: AuthSession, provider: string, proxyUrl?: string): Promise<GameAccountOperationResult> {
  const definition = await resolveProviderDefinition(provider);
  const normalizedProvider = normalizeProviderCode(definition.store);
  const support = getProviderFeatureSupport(definition);
  if (!support.supportsLinking) {
    throw new Error(`${definition.label ?? normalizedProvider} does not support account linking`);
  }

  const token = getSessionIdToken(session);
  const port = await findAvailablePort(REDIRECT_PORTS);
  const callback = startAccountLinkCallbackServer(port, normalizedProvider);
  let loginResult: AccountLinkLoginResult;
  try {
    await callback.ready;
    const loginUrl = await getLoginUrl(normalizedProvider, port, token);
    await shell.openExternal(loginUrl);
    loginResult = await callback.result;
  } catch (error) {
    callback.close();
    throw error;
  }

  let message: string | undefined;
  if (support.supportsSync) {
    try {
      await requestProviderSync(normalizedProvider, token);
      await wait(SYNC_REFRESH_WAIT_MS);
    } catch (error) {
      message = error instanceof Error ? error.message : "Account connected, but library sync did not start";
      await wait(POST_LOGIN_REFRESH_WAIT_MS);
    }
  } else {
    await wait(POST_LOGIN_REFRESH_WAIT_MS);
  }
  await invalidateAccountGameCaches(session, proxyUrl);

  const refreshed = mergeLoginResult(await fetchConnectionsForSession(session), loginResult);
  return {
    ...refreshed,
    ok: true,
    account: accountForProvider(refreshed, normalizedProvider),
    message,
  };
}

export async function unlinkGameAccount(session: AuthSession, provider: string, proxyUrl?: string): Promise<GameAccountOperationResult> {
  const definition = await resolveProviderDefinition(provider);
  const normalizedProvider = normalizeProviderCode(definition.store);
  const token = getSessionIdToken(session);

  await deleteProviderLink(normalizedProvider, token);
  await wait(DISCONNECT_REFRESH_WAIT_MS);
  await invalidateAccountGameCaches(session, proxyUrl);

  const refreshed = await fetchConnectionsForSession(session);
  return {
    ...refreshed,
    ok: true,
    account: accountForProvider(refreshed, normalizedProvider),
  };
}

export async function resyncGameAccount(session: AuthSession, provider: string, proxyUrl?: string): Promise<GameAccountOperationResult> {
  const definition = await resolveProviderDefinition(provider);
  const normalizedProvider = normalizeProviderCode(definition.store);
  const support = getProviderFeatureSupport(definition);
  if (!support.supportsSync) {
    throw new Error(`${definition.label ?? normalizedProvider} does not support library sync`);
  }

  const token = getSessionIdToken(session);
  await requestProviderSync(normalizedProvider, token);
  await wait(SYNC_REFRESH_WAIT_MS);
  await invalidateAccountGameCaches(session, proxyUrl);

  const refreshed = await fetchConnectionsForSession(session);
  return {
    ...refreshed,
    ok: true,
    account: accountForProvider(refreshed, normalizedProvider),
  };
}
