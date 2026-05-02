import { IPC_CHANNELS } from "@shared/ipc";
import type {
  AuthLoginRequest,
  AuthSessionRequest,
  RegionsFetchRequest,
  SubscriptionFetchRequest,
  GamesFetchRequest,
  CatalogBrowseRequest,
  ResolveLaunchIdRequest,
} from "@shared/gfn";
import type { MainIpcDeps } from "./types";
import { fetchSubscription, fetchDynamicRegions } from "../gfn/subscription";
import {
  browseCatalog,
  fetchLibraryGames,
  fetchMainGames,
  fetchPublicGames,
  resolveLaunchAppId,
} from "../gfn/games";
import { refreshScheduler } from "../services/refreshScheduler";

export function registerAuthSubscriptionGamesIpc(deps: MainIpcDeps): void {
  const { ipcMain, authService } = deps;

  ipcMain.handle(IPC_CHANNELS.AUTH_GET_SESSION, async (_event, payload: AuthSessionRequest = {}) => {
    return authService.ensureValidSessionWithStatus(Boolean(payload.forceRefresh));
  });

  ipcMain.handle(IPC_CHANNELS.AUTH_GET_PROVIDERS, async () => {
    return authService.getProviders();
  });

  ipcMain.handle(IPC_CHANNELS.AUTH_GET_REGIONS, async (_event, payload: RegionsFetchRequest) => {
    return authService.getRegions(payload?.token);
  });

  ipcMain.handle(IPC_CHANNELS.AUTH_LOGIN, async (_event, payload: AuthLoginRequest) => {
    return authService.login(payload);
  });

  ipcMain.handle(IPC_CHANNELS.AUTH_LOGOUT, async () => {
    await authService.logout();
  });

  ipcMain.handle(IPC_CHANNELS.AUTH_LOGOUT_ALL, async () => {
    await authService.logoutAll();
  });

  ipcMain.handle(IPC_CHANNELS.AUTH_GET_SAVED_ACCOUNTS, async () => {
    return authService.getSavedAccounts();
  });

  ipcMain.handle(IPC_CHANNELS.AUTH_SWITCH_ACCOUNT, async (_event, userId: string) => {
    return authService.switchAccount(userId);
  });

  ipcMain.handle(IPC_CHANNELS.AUTH_REMOVE_ACCOUNT, async (_event, userId: string) => {
    await authService.removeAccount(userId);
  });

  ipcMain.handle(IPC_CHANNELS.SUBSCRIPTION_FETCH, async (_event, payload: SubscriptionFetchRequest) => {
    const token = await deps.resolveJwt(payload?.token);
    const streamingBaseUrl =
      payload?.providerStreamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl;
    const userId = payload.userId;

    const { vpcId } = await fetchDynamicRegions(token, streamingBaseUrl);

    return fetchSubscription(token, userId, vpcId ?? undefined);
  });

  ipcMain.handle(IPC_CHANNELS.GAMES_FETCH_MAIN, async (_event, payload: GamesFetchRequest) => {
    const token = await deps.resolveJwt(payload?.token);
    const streamingBaseUrl =
      payload?.providerStreamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl;
    refreshScheduler.updateAuthContext(token, streamingBaseUrl);
    return fetchMainGames(token, streamingBaseUrl);
  });

  ipcMain.handle(IPC_CHANNELS.GAMES_FETCH_LIBRARY, async (_event, payload: GamesFetchRequest) => {
    const token = await deps.resolveJwt(payload?.token);
    const streamingBaseUrl =
      payload?.providerStreamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl;
    refreshScheduler.updateAuthContext(token, streamingBaseUrl);
    return fetchLibraryGames(token, streamingBaseUrl);
  });

  ipcMain.handle(IPC_CHANNELS.GAMES_BROWSE_CATALOG, async (_event, payload: CatalogBrowseRequest) => {
    const token = await deps.resolveJwt(payload?.token);
    const streamingBaseUrl =
      payload?.providerStreamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl;
    refreshScheduler.updateAuthContext(token, streamingBaseUrl);
    return browseCatalog({ ...payload, token, providerStreamingBaseUrl: streamingBaseUrl });
  });

  ipcMain.handle(IPC_CHANNELS.GAMES_FETCH_PUBLIC, async () => {
    return fetchPublicGames();
  });

  ipcMain.handle(IPC_CHANNELS.GAMES_RESOLVE_LAUNCH_ID, async (_event, payload: ResolveLaunchIdRequest) => {
    const token = await deps.resolveJwt(payload?.token);
    const streamingBaseUrl =
      payload?.providerStreamingBaseUrl ?? authService.getSelectedProvider().streamingServiceUrl;
    return resolveLaunchAppId(token, payload.appIdOrUuid, streamingBaseUrl);
  });
}
