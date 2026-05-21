import type { IpcMain } from "electron";
import { IPC_CHANNELS } from "@shared/ipc";
import type {
  AuthLoginRequest,
  AuthSessionRequest,
  CatalogBrowseRequest,
  GamesFetchRequest,
  RegionsFetchRequest,
  ResolveLaunchIdRequest,
  ResolveStoreUrlRequest,
  SubscriptionFetchRequest,
} from "@shared/gfn";
import type { AuthService } from "../gfn/auth";
import {
  browseCatalog,
  fetchFeaturedGames,
  fetchLibraryGames,
  fetchMainGames,
  fetchPublicGames,
  fetchStorePanels,
  resolveLaunchAppId,
  resolveStoreUrl,
} from "../gfn/games";
import { fetchSubscription, fetchDynamicRegions } from "../gfn/subscription";

interface RefreshSchedulerAuthContextUpdater {
  updateAuthContext(token: string, providerStreamingBaseUrl?: string): void;
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
    IPC_CHANNELS.GAMES_FETCH_MAIN,
    async (_event, payload: GamesFetchRequest) => {
      const token = await resolveJwt(payload?.token);
      const streamingBaseUrl =
        payload?.providerStreamingBaseUrl ??
        authService.getSelectedProvider().streamingServiceUrl;
      refreshScheduler.updateAuthContext(token, streamingBaseUrl);
      return fetchMainGames(token, streamingBaseUrl);
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.GAMES_FETCH_FEATURED,
    async (_event, payload: GamesFetchRequest) => {
      const token = await resolveJwt(payload?.token);
      const streamingBaseUrl =
        payload?.providerStreamingBaseUrl ??
        authService.getSelectedProvider().streamingServiceUrl;
      refreshScheduler.updateAuthContext(token, streamingBaseUrl);
      return fetchFeaturedGames(token, streamingBaseUrl);
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.GAMES_FETCH_STORE_PANELS,
    async (_event, payload: GamesFetchRequest) => {
      const token = await resolveJwt(payload?.token);
      const streamingBaseUrl =
        payload?.providerStreamingBaseUrl ??
        authService.getSelectedProvider().streamingServiceUrl;
      refreshScheduler.updateAuthContext(token, streamingBaseUrl);
      return fetchStorePanels(token, streamingBaseUrl);
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.GAMES_FETCH_LIBRARY,
    async (_event, payload: GamesFetchRequest) => {
      const token = await resolveJwt(payload?.token);
      const streamingBaseUrl =
        payload?.providerStreamingBaseUrl ??
        authService.getSelectedProvider().streamingServiceUrl;
      refreshScheduler.updateAuthContext(token, streamingBaseUrl);
      return fetchLibraryGames(token, streamingBaseUrl);
    },
  );

  ipcMain.handle(
    IPC_CHANNELS.GAMES_BROWSE_CATALOG,
    async (_event, payload: CatalogBrowseRequest) => {
      const token = await resolveJwt(payload?.token);
      const streamingBaseUrl =
        payload?.providerStreamingBaseUrl ??
        authService.getSelectedProvider().streamingServiceUrl;
      refreshScheduler.updateAuthContext(token, streamingBaseUrl);
      return browseCatalog({
        ...payload,
        token,
        providerStreamingBaseUrl: streamingBaseUrl,
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
      const token = await resolveJwt(payload?.token);
      const streamingBaseUrl =
        payload?.providerStreamingBaseUrl ??
        authService.getSelectedProvider().streamingServiceUrl;
      refreshScheduler.updateAuthContext(token, streamingBaseUrl);
      return resolveStoreUrl(token, payload.appIdOrUuid, streamingBaseUrl, {
        variantId: payload.variantId,
        store: payload.store,
      });
    },
  );
}
