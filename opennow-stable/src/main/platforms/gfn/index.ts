/**
 * GeForce NOW main-process platform surface.
 *
 * Prefer importing focused modules (`./auth`, `./cloudmatch`, …) from feature
 * code. This barrel is for platform-registry and cross-cutting consumers.
 */

export { AuthService } from "./auth";
export {
  claimSession,
  createSession,
  getActiveSessions,
  pollSession,
  reportSessionAd,
  stopSession,
} from "./cloudmatch";
export { SessionError, isSessionError } from "./errorCodes";
export {
  browseCatalog,
  fetchFeaturedGames,
  fetchLibraryGames,
  fetchMainGames,
  fetchStorePanels,
  getAccountGamesCacheKeys,
  markGameOwned,
  resolveLaunchAppId,
  resolveStoreUrl,
} from "./games";
export { initSessionProxyAuth } from "./proxyFetch";
export { normalizeSessionProxyUrl, sessionProxyHasCredentials } from "./proxyUrl";
export { getCloudMatchDeviceHashId, getStableDeviceId, toCloudMatchDeviceHashId } from "./deviceId";
export {
  STEAM_DECK_DEVICE_IDENTITY,
  configureIdentifyAsSteamDeck,
  resolveGfnDeviceIdentity,
} from "./deviceIdentity";
export { fetchSubscription, fetchDynamicRegions } from "./subscription";
export {
  fetchPersistentStorageLocations,
  resetPersistentStorage,
} from "./persistentStorage";
export {
  fetchGameAccountConnections,
  linkGameAccount,
  resyncGameAccount,
  unlinkGameAccount,
} from "./accountConnections";
export { GfnSignalingClient } from "./signaling";
export {
  GFN_BIFROST_CLIENT_VERSION,
  GFN_CLIENT_IDENTIFICATION,
  GFN_CLIENT_VERSION,
  gfnBifrostUserAgentForPlatform,
  GFN_PLAY_ORIGIN,
  GFN_PLAY_REFERER,
  GFN_USER_AGENT,
  LCARS_CLIENT_ID,
  buildGfnCloudMatchClaimHeaders,
  buildGfnCloudMatchHeaders,
  buildGfnGraphQlHeaders,
  buildGfnLcarsHeaders,
  buildGfnNvstClientHeaders,
  buildNvidiaAuthHeaders,
  gfnJwtAuthorization,
} from "./clientHeaders";
