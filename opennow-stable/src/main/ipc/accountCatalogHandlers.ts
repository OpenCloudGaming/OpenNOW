import type { IpcMain } from "electron";
import { IPC_CHANNELS } from "@shared/ipc";
import type {
  AuthLoginRequest,
  AuthDeviceLoginAttemptRequest,
  AuthDeviceLoginPollRequest,
  AuthDeviceLoginStartRequest,
  AuthSessionRequest,
  CatalogBrowseRequest,
  GamesFetchRequest,
  RegionsFetchRequest,
  ResolveLaunchIdRequest,
  ResolveStoreUrlRequest,
  SubscriptionFetchRequest,
  PersistentStorageLocationsFetchRequest,
  PersistentStorageResetRequest,
} from "@shared/gfn";
import type { AuthService } from "../gfn/auth";
import {
  browseCatalog,
  fetchFeaturedGames,
  fetchLibraryGames,
  fetchMainGames,
  fetchPublicGames,
  fetchStorePanels,
  peekCachedBrowseCatalog,
  fetchLibraryGamesFromCache,
  resolveLaunchAppId,
  resolveStoreUrl,
} from "../gfn/games";
import { fetchSubscription, fetchDynamicRegions } from "../gfn/subscription";
import { fetchPersistentStorageLocations, resetPersistentStorage } from "../gfn/persistentStorage";

interface RefreshSchedulerAuthContextUpdater {
  updateAuthContext(token: string, userId: string, providerStreamingBaseUrl?: string): void;
}

async function resolveGamesFetchContext(
  deps: Pick<AccountCatalogIpcHandlerDeps, "authService" | "refreshScheduler" | "resolveJwt">,
  payload: GamesFetchRequest | CatalogBrowseRequest = {},
  options: { networkRequired?: boolean } = {},
): Promise<{
  token: string;
  streamingBaseUrl: string;
  userId: string;
}> {
  let session = deps.authService.getSession();
  if (!session || options.networkRequired) {
    session = await deps.authService.ensureValidSession();
  }
  if (!session) {
    throw new Error("No authenticated session available");
  }

  const token = await deps.resolveJwt(payload?.token);
  const streamingBaseUrl =
    payload?.providerStreamingBaseUrl ??
    deps.authService.getSelectedProvider().streamingServiceUrl;
  const userId = payload.userId ?? session.user.userId;
  deps.refreshScheduler.updateAuthContext(token, userId, streamingBaseUrl);
  return { token, streamingBaseUrl, userId };
}

function savedSessionTokens(
  session: NonNullable<ReturnType<AuthService["getSession"]>>,
): { token: string; userId: string } | null {
  const token = session.tokens.idToken ?? session.tokens.accessToken;
  if (!token) {
    return null;
  }
  return { token, userId: session.user.userId };
}

function sessionTokenCandidates(
  session: NonNullable<Awaited<ReturnType<AuthService["ensureValidSession"]>>>,
): [string, ...string[]] {
  const candidates = [
    session.tokens.idToken,
    session.tokens.accessToken,
  ].filter((token): token is string => Boolean(token));
  if (!candidates[0]) {
    throw new Error("No authenticated token available");
  }
  return candidates as [string, ...string[]];
}

export interface AccountCatalogIpcHandlerDeps {
  ipcMain: IpcMain;
  authService: AuthService;
  resolveJwt(token?: string): Promise<string>;
  refreshScheduler: RefreshSchedulerAuthContextUpdater;
}

export function registerAccountCatalogIpcHandlers(
  deps: AccountCatalogIpcHandlerDeps,
): void {
  const { ipcMain, authService, refreshScheduler, resolveJwt } = deps;

  const resolveGamesContext = (payload: GamesFetchRequest | CatalogBrowseRequest = {}) =>
    resolveGamesFetchContext(deps, payload);

  ipcMain.handle(
    IPC_CHANNELS.AUTH_GET_SESSION,
    async (_event, payload: AuthSessionRequest = {}) => {
      return authService.ensureValidSessionWithStatus(
        Boolean(payload.forceRefresh),
      );
    },
  );

  ipcMain.handle(IPC_CHANNELS.AUTH_GET_PROVIDERS, async () => {
    return authService.getProviders();
  });

  ipcMain.handle(
    IPC_CHANNELS.AUTH_GET_REGIONS,
    async (_event, payload: RegionsFetchRequest) => {
      return authService.getRegions(payload?.token);
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.AUTH_LOGIN,
    async (_event, payload: AuthLoginRequest) => {
      return authService.login(payload);
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.AUTH_DEVICE_LOGIN_START,
    async (_event, payload: AuthDeviceLoginStartRequest) => {
      return authService.startDeviceLogin(payload);
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.AUTH_DEVICE_LOGIN_POLL,
    async (_event, payload: AuthDeviceLoginPollRequest) => {
      return authService.pollDeviceLogin(payload);
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.AUTH_DEVICE_LOGIN_COMPLETE,
    async (_event, payload: AuthDeviceLoginAttemptRequest) => {
      return authService.completeDeviceLogin(payload);
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.AUTH_DEVICE_LOGIN_CANCEL,
    async (_event, payload: AuthDeviceLoginAttemptRequest) => {
      authService.cancelDeviceLogin(payload);
    },
  );

  ipcMain.handle(IPC_CHANNELS.AUTH_LOGOUT, async () => {
    await authService.logout();
  });

  ipcMain.handle(IPC_CHANNELS.AUTH_LOGOUT_ALL, async () => {
    await authService.logoutAll();
  });

  ipcMain.handle(IPC_CHANNELS.AUTH_GET_SAVED_ACCOUNTS, async () => {
    return authService.getSavedAccounts();
  });

  ipcMain.handle(
    IPC_CHANNELS.AUTH_SWITCH_ACCOUNT,
    async (_event, userId: string) => {
      return authService.switchAccount(userId);
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.AUTH_REMOVE_ACCOUNT,
    async (_event, userId: string) => {
      await authService.removeAccount(userId);
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.SUBSCRIPTION_FETCH,
    async (_event, payload: SubscriptionFetchRequest) => {
      const token = await resolveJwt(payload?.token);
      const streamingBaseUrl =
        payload?.providerStreamingBaseUrl ??
        authService.getSelectedProvider().streamingServiceUrl;
      const userId = payload.userId;

      const { vpcId } = await fetchDynamicRegions(token, streamingBaseUrl);

      return fetchSubscription(token, userId, vpcId ?? undefined);
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.PERSISTENT_STORAGE_LOCATIONS_FETCH,
    async (_event, payload: PersistentStorageLocationsFetchRequest = {}) => {
      const session = await authService.ensureValidSession();
      if (!session) {
        throw new Error("No authenticated session available");
      }

      let vpcId = payload.serverRegionId ?? undefined;
      if (!vpcId) {
        const streamingBaseUrl = authService.getSelectedProvider().streamingServiceUrl;
        const dynamicRegions = await fetchDynamicRegions(session.tokens.accessToken, streamingBaseUrl);
        vpcId = dynamicRegions.vpcId ?? undefined;
      }

      const [idToken, ...idTokenAlternates] = sessionTokenCandidates(session);
      return fetchPersistentStorageLocations({
        idToken,
        idTokenAlternates,
        vpcId,
        locale: payload.locale,
        currentRegionCode: payload.currentRegionCode,
        currentRegionName: payload.currentRegionName,
      });
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.PERSISTENT_STORAGE_RESET,
    async (_event, payload: PersistentStorageResetRequest = {}) => {
      const session = await authService.ensureValidSession();
      if (!session) {
        throw new Error("No authenticated session available");
      }

      const [idToken, ...idTokenAlternates] = sessionTokenCandidates(session);
      const result = await resetPersistentStorage({
        idToken,
        idTokenAlternates,
        storageRegion: payload.storageRegion ?? null,
      });
      authService.clearSubscriptionCache();
      return result;
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.GAMES_FETCH_MAIN,
    async (_event, payload: GamesFetchRequest) => {
      const { token, streamingBaseUrl, userId } = await resolveGamesContext(payload);
      return fetchMainGames(token, streamingBaseUrl, userId);
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.GAMES_FETCH_FEATURED,
    async (_event, payload: GamesFetchRequest) => {
      const { token, streamingBaseUrl, userId } = await resolveGamesContext(payload);
      return fetchFeaturedGames(token, streamingBaseUrl, userId);
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.GAMES_FETCH_STORE_PANELS,
    async (_event, payload: GamesFetchRequest) => {
      const { token, streamingBaseUrl, userId } = await resolveGamesContext(payload);
      return fetchStorePanels(token, streamingBaseUrl, userId);
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.GAMES_FETCH_LIBRARY,
    async (_event, payload: GamesFetchRequest) => {
      const savedSession = authService.getSession();
      const streamingBaseUrl =
        payload?.providerStreamingBaseUrl ??
        authService.getSelectedProvider().streamingServiceUrl;
      if (savedSession) {
        const tokens = savedSessionTokens(savedSession);
        if (tokens) {
          const userId = payload.userId ?? tokens.userId;
          const cachedLibrary = await fetchLibraryGamesFromCache(tokens.token, streamingBaseUrl, userId);
          if (cachedLibrary) {
            refreshScheduler.updateAuthContext(tokens.token, userId, streamingBaseUrl);
            return cachedLibrary;
          }
        }
      }

      const { token, streamingBaseUrl: resolvedBaseUrl, userId } = await resolveGamesFetchContext(
        deps,
        payload,
        { networkRequired: true },
      );
      return fetchLibraryGames(token, resolvedBaseUrl, userId);
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.GAMES_BROWSE_CATALOG,
    async (_event, payload: CatalogBrowseRequest) => {
      const savedSession = authService.getSession();
      const streamingBaseUrl =
        payload?.providerStreamingBaseUrl ??
        authService.getSelectedProvider().streamingServiceUrl;
      if (savedSession) {
        const tokens = savedSessionTokens(savedSession);
        if (tokens) {
          const userId = payload.userId ?? tokens.userId;
          const cached = await peekCachedBrowseCatalog({
            ...payload,
            token: tokens.token,
            userId,
            providerStreamingBaseUrl: streamingBaseUrl,
          });
          if (cached) {
            refreshScheduler.updateAuthContext(tokens.token, userId, streamingBaseUrl);
            return cached;
          }
        }
      }

      const { token, streamingBaseUrl: resolvedBaseUrl, userId } = await resolveGamesFetchContext(
        deps,
        payload,
        { networkRequired: true },
      );
      return browseCatalog({
        ...payload,
        token,
        userId,
        providerStreamingBaseUrl: resolvedBaseUrl,
      });
    },
  );

  ipcMain.handle(IPC_CHANNELS.GAMES_FETCH_PUBLIC, async () => {
    return fetchPublicGames();
  });

  ipcMain.handle(
    IPC_CHANNELS.GAMES_RESOLVE_LAUNCH_ID,
    async (_event, payload: ResolveLaunchIdRequest) => {
      const token = await resolveJwt(payload?.token);
      const streamingBaseUrl =
        payload?.providerStreamingBaseUrl ??
        authService.getSelectedProvider().streamingServiceUrl;
      return resolveLaunchAppId(token, payload.appIdOrUuid, streamingBaseUrl);
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.GAMES_RESOLVE_STORE_URL,
    async (_event, payload: ResolveStoreUrlRequest) => {
      const { token, streamingBaseUrl } = await resolveGamesContext(payload);
      return resolveStoreUrl(token, payload.appIdOrUuid, streamingBaseUrl, {
        variantId: payload.variantId,
        store: payload.store,
      });
    },
  );
}
