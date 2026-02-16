import { contextBridge, ipcRenderer } from "electron";

import { IPC_CHANNELS } from "@shared/ipc";
import type {
  AuthLoginRequest,
  AuthSessionRequest,
  GamesFetchRequest,
  ResolveLaunchIdRequest,
  RegionsFetchRequest,
  MainToRendererSignalingEvent,
  OpenNowApi,
  SessionCreateRequest,
  SessionPollRequest,
  SessionStopRequest,
  SessionClaimRequest,
  SignalingConnectRequest,
  SendAnswerRequest,
  IceCandidatePayload,
  Settings,
  SubscriptionFetchRequest,
} from "@shared/gfn";

// Extend the OpenNowApi interface for internal preload use
type PreloadApi = OpenNowApi;

const api: PreloadApi = {
  getAuthSession: (input: AuthSessionRequest = {}) => ipcRenderer.invoke(IPC_CHANNELS.AUTH_GET_SESSION, input),
  getLoginProviders: () => ipcRenderer.invoke(IPC_CHANNELS.AUTH_GET_PROVIDERS),
  getRegions: (input: RegionsFetchRequest = {}) => ipcRenderer.invoke(IPC_CHANNELS.AUTH_GET_REGIONS, input),
  login: (input: AuthLoginRequest) => ipcRenderer.invoke(IPC_CHANNELS.AUTH_LOGIN, input),
  logout: () => ipcRenderer.invoke(IPC_CHANNELS.AUTH_LOGOUT),
  fetchSubscription: (input: SubscriptionFetchRequest) =>
    ipcRenderer.invoke(IPC_CHANNELS.SUBSCRIPTION_FETCH, input),
  fetchMainGames: (input: GamesFetchRequest) => ipcRenderer.invoke(IPC_CHANNELS.GAMES_FETCH_MAIN, input),
  fetchLibraryGames: (input: GamesFetchRequest) =>
    ipcRenderer.invoke(IPC_CHANNELS.GAMES_FETCH_LIBRARY, input),
  fetchPublicGames: () => ipcRenderer.invoke(IPC_CHANNELS.GAMES_FETCH_PUBLIC),
  resolveLaunchAppId: (input: ResolveLaunchIdRequest) =>
    ipcRenderer.invoke(IPC_CHANNELS.GAMES_RESOLVE_LAUNCH_ID, input),
  createSession: (input: SessionCreateRequest) => ipcRenderer.invoke(IPC_CHANNELS.CREATE_SESSION, input),
  pollSession: (input: SessionPollRequest) => ipcRenderer.invoke(IPC_CHANNELS.POLL_SESSION, input),
  stopSession: (input: SessionStopRequest) => ipcRenderer.invoke(IPC_CHANNELS.STOP_SESSION, input),
  getActiveSessions: (token?: string, streamingBaseUrl?: string) =>
    ipcRenderer.invoke(IPC_CHANNELS.GET_ACTIVE_SESSIONS, token, streamingBaseUrl),
  claimSession: (input: SessionClaimRequest) => ipcRenderer.invoke(IPC_CHANNELS.CLAIM_SESSION, input),
  showSessionConflictDialog: () => ipcRenderer.invoke(IPC_CHANNELS.SESSION_CONFLICT_DIALOG),
  connectSignaling: (input: SignalingConnectRequest) =>
    ipcRenderer.invoke(IPC_CHANNELS.CONNECT_SIGNALING, input),
  disconnectSignaling: () => ipcRenderer.invoke(IPC_CHANNELS.DISCONNECT_SIGNALING),
  sendAnswer: (input: SendAnswerRequest) => ipcRenderer.invoke(IPC_CHANNELS.SEND_ANSWER, input),
  sendIceCandidate: (input: IceCandidatePayload) =>
    ipcRenderer.invoke(IPC_CHANNELS.SEND_ICE_CANDIDATE, input),
  onSignalingEvent: (listener: (event: MainToRendererSignalingEvent) => void) => {
    const wrapped = (_event: Electron.IpcRendererEvent, payload: MainToRendererSignalingEvent) => {
      listener(payload);
    };

    ipcRenderer.on(IPC_CHANNELS.SIGNALING_EVENT, wrapped);
    return () => {
      ipcRenderer.off(IPC_CHANNELS.SIGNALING_EVENT, wrapped);
    };
  },
  onToggleFullscreen: (listener: () => void) => {
    const wrapped = () => listener();
    ipcRenderer.on("app:toggle-fullscreen", wrapped);
    return () => {
      ipcRenderer.off("app:toggle-fullscreen", wrapped);
    };
  },
  toggleFullscreen: () => ipcRenderer.invoke(IPC_CHANNELS.TOGGLE_FULLSCREEN),
  togglePointerLock: () => ipcRenderer.invoke(IPC_CHANNELS.TOGGLE_POINTER_LOCK),
  getSettings: () => ipcRenderer.invoke(IPC_CHANNELS.SETTINGS_GET),
  setSetting: <K extends keyof Settings>(key: K, value: Settings[K]) =>
    ipcRenderer.invoke(IPC_CHANNELS.SETTINGS_SET, key, value),
  resetSettings: () => ipcRenderer.invoke(IPC_CHANNELS.SETTINGS_RESET),
};

contextBridge.exposeInMainWorld("openNow", api);
