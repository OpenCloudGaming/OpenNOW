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
  DiscordPresencePayload,
  FlightProfile,
  MicDeviceInfo,
  PlatformInfo,} from "@shared/gfn";

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
  exportLogs: (format?: "text" | "json") => ipcRenderer.invoke(IPC_CHANNELS.LOGS_EXPORT, format),
  updateDiscordPresence: (state: DiscordPresencePayload) =>
    ipcRenderer.invoke(IPC_CHANNELS.DISCORD_UPDATE_PRESENCE, state),
  clearDiscordPresence: () => ipcRenderer.invoke(IPC_CHANNELS.DISCORD_CLEAR_PRESENCE),
  flightGetProfile: (vidPid: string, gameId?: string) =>
    ipcRenderer.invoke(IPC_CHANNELS.FLIGHT_GET_PROFILE, vidPid, gameId),
  flightSetProfile: (profile: FlightProfile) =>
    ipcRenderer.invoke(IPC_CHANNELS.FLIGHT_SET_PROFILE, profile),
  flightDeleteProfile: (vidPid: string, gameId?: string) =>
    ipcRenderer.invoke(IPC_CHANNELS.FLIGHT_DELETE_PROFILE, vidPid, gameId),
  flightGetAllProfiles: () => ipcRenderer.invoke(IPC_CHANNELS.FLIGHT_GET_ALL_PROFILES),
  flightResetProfile: (vidPid: string) => ipcRenderer.invoke(IPC_CHANNELS.FLIGHT_RESET_PROFILE, vidPid),
  getOsHdrInfo: () => ipcRenderer.invoke(IPC_CHANNELS.HDR_GET_OS_INFO),
  relaunchApp: () => ipcRenderer.invoke(IPC_CHANNELS.APP_RELAUNCH),
  micEnumerateDevices: () => ipcRenderer.invoke(IPC_CHANNELS.MIC_ENUMERATE_DEVICES),
  onMicDevicesChanged: (listener: (devices: MicDeviceInfo[]) => void) => {
    const wrapped = (_event: Electron.IpcRendererEvent, devices: MicDeviceInfo[]) => {
      listener(devices);
    };
    ipcRenderer.on(IPC_CHANNELS.MIC_DEVICES_CHANGED, wrapped);
    return () => {
      ipcRenderer.off(IPC_CHANNELS.MIC_DEVICES_CHANGED, wrapped);
    };
  },
  onSessionExpired: (listener: (reason: string) => void) => {
    const wrapped = (_event: Electron.IpcRendererEvent, reason: string) => {
      listener(reason);
    };
    ipcRenderer.on(IPC_CHANNELS.AUTH_SESSION_EXPIRED, wrapped);
    return () => {
      ipcRenderer.off(IPC_CHANNELS.AUTH_SESSION_EXPIRED, wrapped);
    };
  },
  getPlatformInfo: (): Promise<PlatformInfo> => ipcRenderer.invoke(IPC_CHANNELS.GET_PLATFORM_INFO),};

contextBridge.exposeInMainWorld("openNow", api);
