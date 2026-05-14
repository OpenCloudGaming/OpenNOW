import electron from "electron";
const IPC_CHANNELS = {
  AUTH_GET_SESSION: "auth:get-session",
  AUTH_GET_PROVIDERS: "auth:get-providers",
  AUTH_GET_REGIONS: "auth:get-regions",
  AUTH_LOGIN: "auth:login",
  AUTH_LOGOUT: "auth:logout",
  AUTH_LOGOUT_ALL: "auth:logout-all",
  AUTH_GET_SAVED_ACCOUNTS: "auth:get-saved-accounts",
  AUTH_SWITCH_ACCOUNT: "auth:switch-account",
  AUTH_REMOVE_ACCOUNT: "auth:remove-account",
  PING_REGIONS: "gfn:ping-regions",
  SUBSCRIPTION_FETCH: "subscription:fetch",
  GAMES_FETCH_MAIN: "games:fetch-main",
  GAMES_FETCH_LIBRARY: "games:fetch-library",
  GAMES_BROWSE_CATALOG: "games:browse-catalog",
  GAMES_FETCH_PUBLIC: "games:fetch-public",
  GAMES_RESOLVE_LAUNCH_ID: "games:resolve-launch-id",
  CREATE_SESSION: "gfn:create-session",
  POLL_SESSION: "gfn:poll-session",
  REPORT_SESSION_AD: "gfn:report-session-ad",
  STOP_SESSION: "gfn:stop-session",
  GET_ACTIVE_SESSIONS: "gfn:get-active-sessions",
  CLAIM_SESSION: "gfn:claim-session",
  SESSION_CONFLICT_DIALOG: "gfn:session-conflict-dialog",
  CONNECT_SIGNALING: "gfn:connect-signaling",
  DISCONNECT_SIGNALING: "gfn:disconnect-signaling",
  SEND_ANSWER: "gfn:send-answer",
  SEND_ICE_CANDIDATE: "gfn:send-ice-candidate",
  NATIVE_INPUT: "gfn:native-input",
  NATIVE_RENDER_SURFACE: "gfn:native-render-surface",
  REQUEST_KEYFRAME: "gfn:request-keyframe",
  SIGNALING_EVENT: "gfn:signaling-event",
  TOGGLE_FULLSCREEN: "window:toggle-fullscreen",
  SET_FULLSCREEN: "window:set-fullscreen",
  TOGGLE_POINTER_LOCK: "window:toggle-pointer-lock",
  POINTER_LOCK_CHANGE: "window:pointer-lock-change",
  EXTERNAL_ESCAPE: "app:external-escape",
  QUIT_APP: "app:quit",
  APP_UPDATER_GET_STATE: "app-updater:get-state",
  APP_UPDATER_CHECK: "app-updater:check",
  APP_UPDATER_DOWNLOAD: "app-updater:download",
  APP_UPDATER_INSTALL: "app-updater:install",
  APP_UPDATER_STATE_CHANGED: "app-updater:state-changed",
  SETTINGS_GET: "settings:get",
  SETTINGS_SET: "settings:set",
  SETTINGS_RESET: "settings:reset",
  SETTINGS_SELECT_NATIVE_STREAMER_EXECUTABLE: "settings:select-native-streamer-executable",
  NATIVE_STREAMER_STATUS: "native:streamer-status",
  NATIVE_CLOUD_GSYNC_CAPABILITIES: "native:cloud-gsync-capabilities",
  MICROPHONE_PERMISSION_GET: "microphone:permission:get",
  LOGS_EXPORT: "logs:export",
  SCREENSHOT_SAVE: "screenshot:save",
  SCREENSHOT_LIST: "screenshot:list",
  SCREENSHOT_DELETE: "screenshot:delete",
  SCREENSHOT_SAVE_AS: "screenshot:save-as",
  RECORDING_BEGIN: "recording:begin",
  RECORDING_CHUNK: "recording:chunk",
  RECORDING_FINISH: "recording:finish",
  RECORDING_ABORT: "recording:abort",
  RECORDING_LIST: "recording:list",
  RECORDING_DELETE: "recording:delete",
  RECORDING_SHOW_IN_FOLDER: "recording:showInFolder",
  CACHE_DELETE_ALL: "cache:delete-all",
  COMMUNITY_GET_THANKS: "community:get-thanks",
  // Media browsing
  MEDIA_LIST_BY_GAME: "media:list-by-game",
  MEDIA_THUMBNAIL: "media:thumbnail",
  MEDIA_SHOW_IN_FOLDER: "media:show-in-folder",
  MEDIA_PLAYBACK_URL: "media:playback-url",
  MEDIA_DELETE_FILE: "media:delete-file",
  MEDIA_REGEN_THUMBNAIL: "media:regen-thumbnail",
  // PrintedWaste queue integration
  PRINTEDWASTE_QUEUE_FETCH: "printedwaste:queue-fetch",
  PRINTEDWASTE_SERVER_MAPPING_FETCH: "printedwaste:server-mapping-fetch",
  // Discord Rich Presence
  DISCORD_CLEAR_ACTIVITY: "discord:clear-activity"
};
const SESSION_ERROR_TRANSPORT_KIND = "opennow.session-error";
const SESSION_ERROR_TRANSPORT_PREFIX = "__OPENNOW_SESSION_ERROR__:";
function isSerializedSessionError(error) {
  return Boolean(
    error && typeof error === "object" && "kind" in error && error.kind === SESSION_ERROR_TRANSPORT_KIND && "name" in error && error.name === "SessionError" && "gfnErrorCode" in error && typeof error.gfnErrorCode === "number" && "title" in error && typeof error.title === "string" && "description" in error && typeof error.description === "string"
  );
}
function parseSerializedSessionErrorTransport(message) {
  const markerIndex = message.indexOf(SESSION_ERROR_TRANSPORT_PREFIX);
  if (markerIndex < 0) {
    return null;
  }
  const payload = message.slice(markerIndex + SESSION_ERROR_TRANSPORT_PREFIX.length);
  try {
    const parsed = JSON.parse(payload);
    return isSerializedSessionError(parsed) ? parsed : null;
  } catch {
    return null;
  }
}
const { contextBridge, ipcRenderer } = electron;
function unwrapSessionInvokeError(error) {
  if (error instanceof Error) {
    const sessionError = parseSerializedSessionErrorTransport(error.message);
    if (sessionError) {
      throw sessionError;
    }
  }
  throw error;
}
function invokeSessionChannel(channel, ...args) {
  return ipcRenderer.invoke(channel, ...args).catch((error) => unwrapSessionInvokeError(error));
}
const api = {
  getAuthSession: (input = {}) => ipcRenderer.invoke(IPC_CHANNELS.AUTH_GET_SESSION, input),
  getLoginProviders: () => ipcRenderer.invoke(IPC_CHANNELS.AUTH_GET_PROVIDERS),
  getRegions: (input = {}) => ipcRenderer.invoke(IPC_CHANNELS.AUTH_GET_REGIONS, input),
  login: (input) => ipcRenderer.invoke(IPC_CHANNELS.AUTH_LOGIN, input),
  logout: () => ipcRenderer.invoke(IPC_CHANNELS.AUTH_LOGOUT),
  logoutAll: () => ipcRenderer.invoke(IPC_CHANNELS.AUTH_LOGOUT_ALL),
  getSavedAccounts: () => ipcRenderer.invoke(IPC_CHANNELS.AUTH_GET_SAVED_ACCOUNTS),
  switchAccount: (userId) => ipcRenderer.invoke(IPC_CHANNELS.AUTH_SWITCH_ACCOUNT, userId),
  removeAccount: (userId) => ipcRenderer.invoke(IPC_CHANNELS.AUTH_REMOVE_ACCOUNT, userId),
  fetchSubscription: (input) => ipcRenderer.invoke(IPC_CHANNELS.SUBSCRIPTION_FETCH, input),
  fetchMainGames: (input) => ipcRenderer.invoke(IPC_CHANNELS.GAMES_FETCH_MAIN, input),
  fetchLibraryGames: (input) => ipcRenderer.invoke(IPC_CHANNELS.GAMES_FETCH_LIBRARY, input),
  browseCatalog: (input) => ipcRenderer.invoke(IPC_CHANNELS.GAMES_BROWSE_CATALOG, input),
  fetchPublicGames: () => ipcRenderer.invoke(IPC_CHANNELS.GAMES_FETCH_PUBLIC),
  resolveLaunchAppId: (input) => ipcRenderer.invoke(IPC_CHANNELS.GAMES_RESOLVE_LAUNCH_ID, input),
  createSession: (input) => invokeSessionChannel(IPC_CHANNELS.CREATE_SESSION, input),
  pollSession: (input) => invokeSessionChannel(IPC_CHANNELS.POLL_SESSION, input),
  reportSessionAd: (input) => invokeSessionChannel(IPC_CHANNELS.REPORT_SESSION_AD, input),
  stopSession: (input) => invokeSessionChannel(IPC_CHANNELS.STOP_SESSION, input),
  getActiveSessions: (token, streamingBaseUrl) => ipcRenderer.invoke(IPC_CHANNELS.GET_ACTIVE_SESSIONS, token, streamingBaseUrl),
  claimSession: (input) => invokeSessionChannel(IPC_CHANNELS.CLAIM_SESSION, input),
  showSessionConflictDialog: () => ipcRenderer.invoke(IPC_CHANNELS.SESSION_CONFLICT_DIALOG),
  connectSignaling: (input) => ipcRenderer.invoke(IPC_CHANNELS.CONNECT_SIGNALING, input),
  disconnectSignaling: () => ipcRenderer.invoke(IPC_CHANNELS.DISCONNECT_SIGNALING),
  sendAnswer: (input) => ipcRenderer.invoke(IPC_CHANNELS.SEND_ANSWER, input),
  sendIceCandidate: (input) => ipcRenderer.invoke(IPC_CHANNELS.SEND_ICE_CANDIDATE, input),
  sendNativeInput: (input) => {
    ipcRenderer.send(IPC_CHANNELS.NATIVE_INPUT, input);
  },
  updateNativeRenderSurface: (input) => {
    ipcRenderer.send(IPC_CHANNELS.NATIVE_RENDER_SURFACE, input);
  },
  requestKeyframe: (input) => ipcRenderer.invoke(IPC_CHANNELS.REQUEST_KEYFRAME, input),
  onSignalingEvent: (listener) => {
    const wrapped = (_event, payload) => {
      listener(payload);
    };
    ipcRenderer.on(IPC_CHANNELS.SIGNALING_EVENT, wrapped);
    return () => {
      ipcRenderer.off(IPC_CHANNELS.SIGNALING_EVENT, wrapped);
    };
  },
  onToggleFullscreen: (listener) => {
    const wrapped = () => listener();
    ipcRenderer.on("app:toggle-fullscreen", wrapped);
    return () => {
      ipcRenderer.off("app:toggle-fullscreen", wrapped);
    };
  },
  quitApp: () => ipcRenderer.invoke(IPC_CHANNELS.QUIT_APP),
  getUpdaterState: () => ipcRenderer.invoke(IPC_CHANNELS.APP_UPDATER_GET_STATE),
  checkForUpdates: () => ipcRenderer.invoke(IPC_CHANNELS.APP_UPDATER_CHECK),
  downloadUpdate: () => ipcRenderer.invoke(IPC_CHANNELS.APP_UPDATER_DOWNLOAD),
  installUpdateAndRestart: () => ipcRenderer.invoke(IPC_CHANNELS.APP_UPDATER_INSTALL),
  onUpdaterStateChanged: (listener) => {
    const wrapped = (_event, payload) => {
      listener(payload);
    };
    ipcRenderer.on(IPC_CHANNELS.APP_UPDATER_STATE_CHANGED, wrapped);
    return () => {
      ipcRenderer.off(IPC_CHANNELS.APP_UPDATER_STATE_CHANGED, wrapped);
    };
  },
  toggleFullscreen: () => ipcRenderer.invoke(IPC_CHANNELS.TOGGLE_FULLSCREEN),
  setFullscreen: (v) => ipcRenderer.invoke(IPC_CHANNELS.SET_FULLSCREEN, v),
  togglePointerLock: () => ipcRenderer.invoke(IPC_CHANNELS.TOGGLE_POINTER_LOCK),
  getSettings: () => ipcRenderer.invoke(IPC_CHANNELS.SETTINGS_GET),
  setSetting: (key, value) => ipcRenderer.invoke(IPC_CHANNELS.SETTINGS_SET, key, value),
  resetSettings: () => ipcRenderer.invoke(IPC_CHANNELS.SETTINGS_RESET),
  selectNativeStreamerExecutable: () => ipcRenderer.invoke(IPC_CHANNELS.SETTINGS_SELECT_NATIVE_STREAMER_EXECUTABLE),
  getNativeStreamerStatus: () => ipcRenderer.invoke(IPC_CHANNELS.NATIVE_STREAMER_STATUS),
  getNativeCloudGsyncCapabilities: () => ipcRenderer.invoke(IPC_CHANNELS.NATIVE_CLOUD_GSYNC_CAPABILITIES),
  notifyPointerLockChange: (active) => ipcRenderer.send(IPC_CHANNELS.POINTER_LOCK_CHANGE, active),
  onExternalEscape: (listener) => {
    const wrapped = () => listener();
    ipcRenderer.on(IPC_CHANNELS.EXTERNAL_ESCAPE, wrapped);
    return () => ipcRenderer.off(IPC_CHANNELS.EXTERNAL_ESCAPE, wrapped);
  },
  getMicrophonePermission: () => ipcRenderer.invoke(IPC_CHANNELS.MICROPHONE_PERMISSION_GET),
  exportLogs: (format) => ipcRenderer.invoke(IPC_CHANNELS.LOGS_EXPORT, format),
  pingRegions: (regions) => ipcRenderer.invoke(IPC_CHANNELS.PING_REGIONS, regions),
  saveScreenshot: (input) => ipcRenderer.invoke(IPC_CHANNELS.SCREENSHOT_SAVE, input),
  listScreenshots: () => ipcRenderer.invoke(IPC_CHANNELS.SCREENSHOT_LIST),
  deleteScreenshot: (input) => ipcRenderer.invoke(IPC_CHANNELS.SCREENSHOT_DELETE, input),
  saveScreenshotAs: (input) => ipcRenderer.invoke(IPC_CHANNELS.SCREENSHOT_SAVE_AS, input),
  onTriggerScreenshot: (listener) => {
    const wrapped = () => listener();
    ipcRenderer.on("app:trigger-screenshot", wrapped);
    return () => {
      ipcRenderer.off("app:trigger-screenshot", wrapped);
    };
  },
  beginRecording: (input) => ipcRenderer.invoke(IPC_CHANNELS.RECORDING_BEGIN, input),
  sendRecordingChunk: (input) => ipcRenderer.invoke(IPC_CHANNELS.RECORDING_CHUNK, input),
  finishRecording: (input) => ipcRenderer.invoke(IPC_CHANNELS.RECORDING_FINISH, input),
  abortRecording: (input) => ipcRenderer.invoke(IPC_CHANNELS.RECORDING_ABORT, input),
  listRecordings: () => ipcRenderer.invoke(IPC_CHANNELS.RECORDING_LIST),
  deleteRecording: (input) => ipcRenderer.invoke(IPC_CHANNELS.RECORDING_DELETE, input),
  showRecordingInFolder: (id) => ipcRenderer.invoke(IPC_CHANNELS.RECORDING_SHOW_IN_FOLDER, id),
  listMediaByGame: (input = {}) => ipcRenderer.invoke(IPC_CHANNELS.MEDIA_LIST_BY_GAME, input),
  getMediaThumbnail: (input) => ipcRenderer.invoke(IPC_CHANNELS.MEDIA_THUMBNAIL, input),
  showMediaInFolder: (input) => ipcRenderer.invoke(IPC_CHANNELS.MEDIA_SHOW_IN_FOLDER, input),
  getMediaPlaybackUrl: (input) => ipcRenderer.invoke(IPC_CHANNELS.MEDIA_PLAYBACK_URL, input),
  deleteMediaFile: (input) => ipcRenderer.invoke(IPC_CHANNELS.MEDIA_DELETE_FILE, input),
  regenMediaThumbnail: (input) => ipcRenderer.invoke(IPC_CHANNELS.MEDIA_REGEN_THUMBNAIL, input),
  deleteCache: () => ipcRenderer.invoke(IPC_CHANNELS.CACHE_DELETE_ALL),
  fetchPrintedWasteQueue: () => ipcRenderer.invoke(IPC_CHANNELS.PRINTEDWASTE_QUEUE_FETCH),
  fetchPrintedWasteServerMapping: () => ipcRenderer.invoke(IPC_CHANNELS.PRINTEDWASTE_SERVER_MAPPING_FETCH),
  getThanksData: () => ipcRenderer.invoke(IPC_CHANNELS.COMMUNITY_GET_THANKS),
  clearDiscordActivity: () => ipcRenderer.invoke(IPC_CHANNELS.DISCORD_CLEAR_ACTIVITY)
};
contextBridge.exposeInMainWorld("openNow", api);
//# sourceMappingURL=index.mjs.map
