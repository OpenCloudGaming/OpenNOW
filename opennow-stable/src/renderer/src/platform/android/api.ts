import { App as CapacitorApp } from "@capacitor/app";
import { registerPlugin } from "@capacitor/core";
import { Device } from "@capacitor/device";
import { Directory, Filesystem } from "@capacitor/filesystem";
import { StatusBar, Style } from "@capacitor/status-bar";

import type {
  ActiveSessionInfo,
  AuthLoginRequest,
  AuthSession,
  AuthSessionRequest,
  AuthSessionResult,
  AuthTokens,
  AppUpdaterState,
  CatalogBrowseRequest,
  CatalogBrowseResult,
  CatalogFilterGroup,
  CatalogSortOption,
  GameInfo,
  IceCandidatePayload,
  KeyframeRequest,
  LoginProvider,
  MainToRendererSignalingEvent,
  MediaListingResult,
  MicrophonePermissionResult,
  OpenNowApi,
  PingResult,
  RecordingAbortRequest,
  RecordingBeginRequest,
  RecordingBeginResult,
  RecordingChunkRequest,
  RecordingDeleteRequest,
  RecordingEntry,
  RecordingFinishRequest,
  RegionsFetchRequest,
  ResolveLaunchIdRequest,
  ScreenshotDeleteRequest,
  ScreenshotEntry,
  ScreenshotSaveAsRequest,
  ScreenshotSaveAsResult,
  ScreenshotSaveRequest,
  SendAnswerRequest,
  SessionAdReportRequest,
  SessionClaimRequest,
  SessionConflictChoice,
  SessionCreateRequest,
  SessionInfo,
  IceServer,
  SessionPollRequest,
  SessionStopRequest,
  Settings,
  SignalingConnectRequest,
  StreamSettings,
  StreamRegion,
  SubscriptionFetchRequest,
  SubscriptionInfo,
  ThankYouDataResult,
  PrintedWasteQueueData,
  PrintedWasteServerMapping,
} from "@shared/gfn";
import { DEFAULT_KEYBOARD_LAYOUT, colorQualityBitDepth, colorQualityChromaFormat, resolveGfnKeyboardLayout } from "@shared/gfn";
import {
  AUTH_ENDPOINT,
  CLIENT_ID,
  CLIENT_TOKEN_ENDPOINT,
  CLIENT_TOKEN_REFRESH_WINDOW_MS,
  DEFAULT_PROVIDER_STREAMING_URL,
  GFN_CLIENT_VERSION,
  GFN_GRAPHQL_URL,
  GFN_USER_AGENT,
  LCARS_CLIENT_ID,
  MES_URL,
  SCOPES,
  SERVICE_URLS_ENDPOINT,
  TOKEN_ENDPOINT,
  TOKEN_REFRESH_WINDOW_MS,
  USERINFO_ENDPOINT,
  defaultProvider,
  isExpired,
  isNearExpiry,
  normalizeProvider,
  toExpiresAt,
  userFromJwt,
} from "@shared/gfnRuntime";
import { DEFAULT_SETTINGS } from "@shared/settings";
import type { OpenNowPlatform } from "../types";
import { BrowserSignalingClient } from "./browserSignaling";
import { isNativeHttpError, nativeRequest } from "./http";
import { appendBase64File, clearDirectory, deleteFile, ensureDir, getPreferenceJson, readDir, readFileBase64, removePreference, setPreferenceJson, writeBase64File } from "./storage";

const AUTH_STATE_KEY = "opennow.android.auth-state.v1";
const SETTINGS_KEY = "opennow.android.settings.v1";
const RECORDINGS_KEY = "opennow.android.recordings.v1";
const SCREENSHOTS_KEY = "opennow.android.screenshots.v1";
const SCREENSHOT_DIR = "opennow-media/screenshots";
const RECORDING_DIR = "opennow-media/recordings";
const PUBLIC_GAMES_URL = "https://static.nvidiagrid.net/supported-public-game-list/locales/gfnpc-en-US.json";
const PANELS_QUERY_HASH = "f8e26265a5db5c20e1334a6872cf04b6e3970507697f6ae55a6ddefa5420daf0";
const APP_METADATA_QUERY_HASH = "39187e85b6dcf60b7279a5f233288b0a8b69a8b1dbcfb5b25555afdcb988f0d7";
const DEFAULT_LOCALE = "en_US";
const PRINTEDWASTE_QUEUE_URL = "https://api.printedwaste.com/gfn/queue/";
const PRINTEDWASTE_SERVER_MAPPING_URL = "https://remote.printedwaste.com/config/GFN_SERVERID_TO_REGION_MAPPING";
const SESSION_MODIFY_ACTION_AD_UPDATE = 6;
const AD_ACTION_CODES: Record<import("@shared/gfn").SessionAdAction, number> = { start: 1, pause: 2, resume: 3, finish: 4, cancel: 5 };
const GFN_AD_MEDIA_PROFILE_ORDER = new Map<string, number>([["mp4", 0], ["webm", 1], ["hls", 2]]);
const DEFAULT_ANDROID_UPDATER_STATE: AppUpdaterState = {
  status: "disabled",
  currentVersion: "android",
  updateSource: "github-releases",
  message: "App updates are not supported on Android in this pass.",
  canCheck: false,
  canDownload: false,
  canInstall: false,
  isPackaged: false,
};
const READY_SESSION_STATUSES = new Set([2, 3]);
const ACTIVE_CREATE_SESSION_STATUSES = new Set([1, 2, 3]);
const ANDROID_CATALOG_SORT_OPTIONS: CatalogSortOption[] = [
  { id: "relevance", label: "Recommended", orderBy: "RELEVANCE" },
  { id: "title_az", label: "Title (A-Z)", orderBy: "TITLE_ASC" },
  { id: "title_za", label: "Title (Z-A)", orderBy: "TITLE_DESC" },
  { id: "last_played", label: "Last played", orderBy: "LAST_PLAYED_DESC" },
];
const ANDROID_CATALOG_CACHE_TTL_MS = 5 * 60 * 1000;

interface LocalhostAuthPlugin {
  startLogin(options: { authUrl: string; port: number; timeoutMs?: number }): Promise<{ code: string; redirectUri?: string }>;
  tcpPing(options: { url: string; timeoutMs?: number; samples?: number; warmup?: boolean }): Promise<{ pingMs?: number; error?: string }>;
}

const LocalhostAuth = registerPlugin<LocalhostAuthPlugin>("LocalhostAuth");

interface PersistedAuthState { session: AuthSession | null; selectedProvider: LoginProvider | null; preferredGfnToken?: "id" | "access"; }
interface TokenResponse { access_token: string; refresh_token?: string; id_token?: string; client_token?: string; expires_in?: number; }
interface ClientTokenResponse { client_token: string; expires_in?: number; }
interface ServiceUrlsResponse { gfnServiceInfo?: { gfnServiceEndpoints?: Array<{ idpId: string; loginProviderCode: string; loginProviderDisplayName: string; streamingServiceUrl: string; loginProviderPriority?: number; }>; }; }
interface ServerInfoResponse { requestStatus?: { serverId?: string }; metaData?: Array<{ key: string; value: string }>; }
interface GraphQlResponse { data?: { panels?: Array<{ sections?: Array<{ items?: Array<{ __typename: string; app?: AppData }> }> }>; apps?: { items: AppData[] } }; errors?: Array<{ message: string }>; }
interface AppData { id: string; title: string; description?: string; longDescription?: string; features?: unknown[]; gameFeatures?: unknown[]; appFeatures?: unknown[]; genres?: unknown[]; tags?: unknown[]; images?: { KEY_ART?: string; GAME_BOX_ART?: string; TV_BANNER?: string; HERO_IMAGE?: string }; publisherName?: string; contentRatings?: unknown[]; variants?: Array<{ id: string; appStore: string; supportedControls?: string[]; gfn?: { status?: string; library?: { selected?: boolean; status?: string; lastPlayedDate?: string } } }>; gfn?: { playType?: string; playabilityState?: string; minimumMembershipTierLabel?: string; catalogSkuStrings?: { SKU_BASED_TAG?: string[] } }; }
interface SubscriptionResponse { firstEntitlementStartDateTime?: string; type?: string; membershipTier?: string; allottedTimeInMinutes?: number; purchasedTimeInMinutes?: number; rolledOverTimeInMinutes?: number; remainingTimeInMinutes?: number; totalTimeInMinutes?: number; notifications?: { notifyUserWhenTimeRemainingInMinutes?: number; notifyUserOnSessionWhenRemainingTimeInMinutes?: number }; currentSpanStartDateTime?: string; currentSpanEndDateTime?: string; currentSubscriptionState?: { state?: string; isGamePlayAllowed?: boolean }; subType?: string; addons?: Array<{ type?: string; subType?: string; status?: string; attributes?: Array<{ key?: string; textValue?: string }> }>; features?: { resolutions?: Array<{ heightInPixels: number; widthInPixels: number; framesPerSecond: number }> }; }
interface RawPublicGame { id?: string | number; title?: string; steamUrl?: string; status?: string; }
interface CloudMatchConnectionInfo { usage?: number; ip?: string | string[]; port?: number; resourcePath?: string; }
interface CloudMatchAdMediaFile { mediaFileUrl?: string; encodingProfile?: string; }
interface CloudMatchSessionAd { adId?: string; adState?: number; adUrl?: string; mediaUrl?: string; videoUrl?: string; url?: string; adMediaFiles?: CloudMatchAdMediaFile[]; clickThroughUrl?: string; adLengthInSeconds?: number; durationMs?: number; durationInMs?: number; title?: string; description?: string; }
interface CloudMatchOpportunity { state?: string; queuePaused?: boolean; gracePeriodSeconds?: number; message?: string; title?: string; description?: string; }
interface CloudMatchResponse { requestStatus: { statusCode: number; statusName?: string; statusDescription?: string }; session: { sessionId: string; status: number; queuePosition?: number; seatSetupInfo?: { queuePosition?: number; seatSetupStep?: number }; sessionProgress?: { queuePosition?: number; isAdsRequired?: boolean }; progressInfo?: { queuePosition?: number; isAdsRequired?: boolean }; sessionAdsRequired?: boolean; isAdsRequired?: boolean; sessionAds?: CloudMatchSessionAd[] | null; opportunity?: CloudMatchOpportunity; connectionInfo?: CloudMatchConnectionInfo[]; sessionControlInfo?: { ip?: string | string[] }; gpuType?: string; errorCode?: number; iceServerConfiguration?: { iceServers?: Array<{ urls: string[] | string; username?: string; credential?: string }> }; sessionRequestData?: { clientRequestMonitorSettings?: Array<{ widthInPixels: number; heightInPixels: number; framesPerSecond: number }>; requestedStreamingFeatures?: { bitDepth?: number; chromaFormat?: number; enabledL4S?: boolean } }; finalizedStreamingFeatures?: { bitDepth?: number; chromaFormat?: number; enabledL4S?: boolean } }; sessions?: Array<{ sessionId: string; appId?: number; gpuType?: string; status: number; sessionControlInfo?: { ip?: string | string[] }; connectionInfo?: CloudMatchConnectionInfo[]; resolution?: string; fps?: number; monitorSettings?: Array<{ widthInPixels?: number; heightInPixels?: number; framesPerSecond?: number }>; sessionRequestData?: { appId?: number | string } }>; }

function ensureTrailingSlash(value: string): string { return value.endsWith("/") ? value : `${value}/`; }
function normalizeBaseUrl(value: string): string { return ensureTrailingSlash(value.trim()); }
function isNumericId(value: string | undefined): value is string { return typeof value === "string" && /^\d+$/.test(value); }
function randomHuId(): string { return `${Date.now().toString(16)}${Math.random().toString(16).slice(2)}`; }
function toOptionalString(value: unknown): string | undefined { if (typeof value !== "string") return undefined; const trimmed = value.trim(); return trimmed || undefined; }
function toPositiveInt(value: unknown): number | undefined { if (typeof value === "number" && Number.isFinite(value)) { const normalized = Math.trunc(value); return normalized > 0 ? normalized : undefined; } if (typeof value === "string" && value.trim()) { const parsed = Number.parseInt(value, 10); return Number.isFinite(parsed) && parsed > 0 ? parsed : undefined; } return undefined; }
function toBoolean(value: unknown): boolean | undefined { if (typeof value === "boolean") return value; if (typeof value === "number") return value !== 0; if (typeof value === "string") { const normalized = value.trim().toLowerCase(); if (normalized === "true" || normalized === "1") return true; if (normalized === "false" || normalized === "0") return false; } return undefined; }
function parseFeatureLabel(value: unknown): string | null { if (typeof value === "string") return value.trim() || null; if (value && typeof value === "object") { const candidate = value as Record<string, unknown>; for (const key of ["name", "label", "title", "displayName"]) { const raw = candidate[key]; if (typeof raw === "string" && raw.trim()) return raw.trim(); } } return null; }
function extractFeatureLabels(app: AppData): string[] { const out: string[] = []; for (const bucket of [app.features, app.gameFeatures, app.appFeatures, app.genres, app.tags, app.gfn?.catalogSkuStrings?.SKU_BASED_TAG]) { if (!Array.isArray(bucket)) continue; for (const entry of bucket) { const label = parseFeatureLabel(entry); if (label) out.push(label); } } return [...new Set(out)]; }
function extractGenres(app: AppData): string[] { if (!Array.isArray(app.genres)) return []; return [...new Set(app.genres.map(parseFeatureLabel).filter((value): value is string => Boolean(value)))]; }
function extractContentRatings(app: AppData): string[] { if (!Array.isArray(app.contentRatings)) return []; return [...new Set(app.contentRatings.map(parseFeatureLabel).filter((value): value is string => Boolean(value)))]; }
function optimizeImage(url: string): string { return url.includes("img.nvidiagrid.net") ? `${url};f=webp;w=272` : url; }
function buildSearchText(title: string, stores: string[], genres: string[], featureLabels: string[], publisherName?: string): string { return [title, publisherName, ...stores, ...genres, ...featureLabels].filter((value): value is string => typeof value === "string" && value.trim().length > 0).join(" ").toLowerCase(); }
function appToGame(app: AppData): GameInfo { const variants = app.variants?.map((variant) => ({ id: variant.id, store: variant.appStore, supportedControls: variant.supportedControls ?? [], librarySelected: variant.gfn?.library?.selected, libraryStatus: variant.gfn?.library?.status, lastPlayedDate: variant.gfn?.library?.lastPlayedDate, gfnStatus: variant.gfn?.status })) ?? []; const selectedVariantIndex = app.variants?.findIndex((variant) => variant.gfn?.library?.selected === true) ?? -1; const safeIndex = selectedVariantIndex >= 0 ? selectedVariantIndex : 0; const selectedVariantId = variants[safeIndex]?.id; const fallbackNumericVariantId = variants.find((variant) => isNumericId(variant.id))?.id; const launchAppId = isNumericId(selectedVariantId) ? selectedVariantId : fallbackNumericVariantId ?? (isNumericId(app.id) ? app.id : undefined); const imageUrl = app.images?.KEY_ART ?? app.images?.GAME_BOX_ART ?? app.images?.TV_BANNER ?? app.images?.HERO_IMAGE ?? undefined; const genres = extractGenres(app); const featureLabels = extractFeatureLabels(app); const availableStores = [...new Set(variants.map((variant) => variant.store).filter((store) => typeof store === "string" && store.trim().length > 0))]; const lastPlayed = variants.map((variant) => variant.lastPlayedDate).find((value): value is string => typeof value === "string" && value.length > 0); return { id: app.id, uuid: app.id, launchAppId, title: app.title, description: app.description, longDescription: app.longDescription, featureLabels, genres, imageUrl: imageUrl ? optimizeImage(imageUrl) : undefined, playType: app.gfn?.playType, membershipTierLabel: app.gfn?.minimumMembershipTierLabel, publisherName: app.publisherName, contentRatings: extractContentRatings(app), playabilityState: app.gfn?.playabilityState, selectedVariantIndex: Math.max(0, selectedVariantIndex), variants, availableStores, searchText: buildSearchText(app.title, availableStores, genres, featureLabels, app.publisherName), isInLibrary: variants.some((variant) => variant.librarySelected || variant.libraryStatus === "IN_LIBRARY"), lastPlayed }; }
function mergeAppMetaIntoGame(game: GameInfo, app: AppData): GameInfo { const merged = appToGame(app); const selectedVariantId = game.variants[game.selectedVariantIndex]?.id; const selectedVariantIndex = selectedVariantId ? merged.variants.findIndex((variant) => variant.id === selectedVariantId) : -1; return { ...game, ...merged, id: game.id, selectedVariantIndex: selectedVariantIndex >= 0 ? selectedVariantIndex : merged.selectedVariantIndex }; }
function dedupeGames(games: GameInfo[]): GameInfo[] { const byId = new Map<string, GameInfo>(); for (const game of games) { const existing = byId.get(game.id); if (!existing) { byId.set(game.id, game); continue; } const mergedVariants = new Map<string, import("@shared/gfn").GameVariant>(); for (const variant of [...existing.variants, ...game.variants]) mergedVariants.set(variant.id, variant); byId.set(game.id, { ...existing, ...game, id: existing.id, uuid: existing.uuid ?? game.uuid, launchAppId: existing.launchAppId ?? game.launchAppId, title: existing.title || game.title, description: existing.description ?? game.description, longDescription: existing.longDescription ?? game.longDescription, imageUrl: existing.imageUrl ?? game.imageUrl, playType: existing.playType ?? game.playType, membershipTierLabel: existing.membershipTierLabel ?? game.membershipTierLabel, publisherName: existing.publisherName ?? game.publisherName, playabilityState: existing.playabilityState ?? game.playabilityState, lastPlayed: existing.lastPlayed ?? game.lastPlayed, isInLibrary: Boolean(existing.isInLibrary || game.isInLibrary), variants: [...mergedVariants.values()], genres: [...new Set([...(existing.genres ?? []), ...(game.genres ?? [])])], featureLabels: [...new Set([...(existing.featureLabels ?? []), ...(game.featureLabels ?? [])])], contentRatings: [...new Set([...(existing.contentRatings ?? []), ...(game.contentRatings ?? [])])], availableStores: [...new Set([...(existing.availableStores ?? []), ...(game.availableStores ?? [])])], searchText: [existing.searchText, game.searchText].filter(Boolean).join(" ").trim() || undefined, selectedVariantIndex: Math.max(0, existing.variants[existing.selectedVariantIndex] ? [...mergedVariants.values()].findIndex((variant) => variant.id === existing.variants[existing.selectedVariantIndex]?.id) : game.selectedVariantIndex) }); } return [...byId.values()]; }
async function readPreferenceJson<T>(key: string, fallback: T): Promise<T> { return getPreferenceJson(key, fallback); }
async function writePreferenceJson<T>(key: string, value: T): Promise<void> { await setPreferenceJson(key, value); }
async function httpRequest<T>(url: string, options: { method?: string; headers?: Record<string, string>; data?: unknown; responseType?: "json" | "text" } = {}): Promise<T> { return nativeRequest<T>({ url, method: options.method ?? "GET", headers: options.headers, data: options.data, readTimeout: 120000, connectTimeout: 30000 }, options.responseType ?? "json"); }
async function tcpPingRegion(region: StreamRegion): Promise<PingResult> {
  try {
    const result = await LocalhostAuth.tcpPing({ url: region.url, timeoutMs: 3000, samples: 3, warmup: true });
    if (typeof result.pingMs === "number" && Number.isFinite(result.pingMs)) {
      return { url: region.url, pingMs: Math.max(0, Math.round(result.pingMs)) };
    }
    return { url: region.url, pingMs: null, error: result.error ?? "All ping tests failed" };
  } catch (error) {
    return { url: region.url, pingMs: null, error: error instanceof Error ? error.message : String(error) };
  }
}
function authRedirectUri(port: number): string { return `http://localhost:${port}`; }
async function createPkce(): Promise<{ verifier: string; challenge: string }> { const bytes = new Uint8Array(64); crypto.getRandomValues(bytes); let binary = ""; for (const value of bytes) binary += String.fromCharCode(value); const verifier = btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "").slice(0, 86); const challengeBuffer = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier)); let challengeBinary = ""; for (const value of new Uint8Array(challengeBuffer)) challengeBinary += String.fromCharCode(value); const challenge = btoa(challengeBinary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, ""); return { verifier, challenge }; }
function buildAuthUrl(provider: LoginProvider, challenge: string, deviceId: string, port: number): string { const nonce = `${Math.random().toString(16).slice(2)}${Math.random().toString(16).slice(2)}`; const params = new URLSearchParams({ response_type: "code", device_id: deviceId, scope: SCOPES, client_id: CLIENT_ID, redirect_uri: authRedirectUri(port), ui_locales: "en_US", nonce, prompt: "select_account", code_challenge: challenge, code_challenge_method: "S256", idp_id: provider.idpId }); return `${AUTH_ENDPOINT}?${params.toString()}`; }
async function exchangeAuthorizationCode(code: string, verifier: string, port: number): Promise<AuthTokens> { const body = new URLSearchParams({ grant_type: "authorization_code", code, redirect_uri: authRedirectUri(port), code_verifier: verifier }); const payload = await httpRequest<TokenResponse>(TOKEN_ENDPOINT, { method: "POST", headers: { "Content-Type": "application/x-www-form-urlencoded; charset=UTF-8", Origin: "https://nvfile", Referer: "https://nvfile/", Accept: "application/json, text/plain, */*", "User-Agent": GFN_USER_AGENT }, data: body.toString() }); return { accessToken: payload.access_token, refreshToken: payload.refresh_token, idToken: payload.id_token, expiresAt: toExpiresAt(payload.expires_in) }; }
async function refreshAuthTokens(refreshToken: string): Promise<AuthTokens> { const body = new URLSearchParams({ grant_type: "refresh_token", refresh_token: refreshToken, client_id: CLIENT_ID }); const payload = await httpRequest<TokenResponse>(TOKEN_ENDPOINT, { method: "POST", headers: { "Content-Type": "application/x-www-form-urlencoded; charset=UTF-8", Origin: "https://nvfile", Accept: "application/json, text/plain, */*", "User-Agent": GFN_USER_AGENT }, data: body.toString() }); return { accessToken: payload.access_token, refreshToken: payload.refresh_token ?? refreshToken, idToken: payload.id_token, expiresAt: toExpiresAt(payload.expires_in) }; }
async function requestClientToken(accessToken: string): Promise<{ token: string; expiresAt: number; lifetimeMs: number }> { const payload = await httpRequest<ClientTokenResponse>(CLIENT_TOKEN_ENDPOINT, { headers: { Authorization: `Bearer ${accessToken}`, Origin: "https://nvfile", Accept: "application/json, text/plain, */*", "User-Agent": GFN_USER_AGENT } }); const expiresAt = toExpiresAt(payload.expires_in); return { token: payload.client_token, expiresAt, lifetimeMs: Math.max(0, expiresAt - Date.now()) }; }
async function refreshWithClientToken(clientToken: string, userId: string): Promise<TokenResponse> { const body = new URLSearchParams({ grant_type: "urn:ietf:params:oauth:grant-type:client_token", client_token: clientToken, client_id: CLIENT_ID, sub: userId }); return httpRequest<TokenResponse>(TOKEN_ENDPOINT, { method: "POST", headers: { "Content-Type": "application/x-www-form-urlencoded; charset=UTF-8", Origin: "https://nvfile", Accept: "application/json, text/plain, */*", "User-Agent": GFN_USER_AGENT }, data: body.toString() }); }
function mergeTokenSnapshot(base: AuthTokens, refreshed: TokenResponse): AuthTokens { return { accessToken: refreshed.access_token, refreshToken: refreshed.refresh_token ?? base.refreshToken, idToken: refreshed.id_token, expiresAt: toExpiresAt(refreshed.expires_in), clientToken: refreshed.client_token ?? base.clientToken, clientTokenExpiresAt: base.clientTokenExpiresAt, clientTokenLifetimeMs: base.clientTokenLifetimeMs }; }
async function fetchUserInfo(tokens: AuthTokens): Promise<AuthSession["user"]> { const jwtUser = userFromJwt(tokens); if (jwtUser?.email || jwtUser?.avatarUrl) return jwtUser; const payload = await httpRequest<{ sub: string; preferred_username?: string; email?: string; picture?: string }>(USERINFO_ENDPOINT, { headers: { Authorization: `Bearer ${tokens.accessToken}`, Origin: "https://nvfile", Accept: "application/json", "User-Agent": GFN_USER_AGENT } }); return { userId: payload.sub, displayName: payload.preferred_username ?? payload.email?.split("@")[0] ?? "User", email: payload.email, avatarUrl: payload.picture, membershipTier: jwtUser?.membershipTier ?? "FREE" }; }

const AUTH_REDIRECT_PORTS = [2259, 6460, 7119, 8870, 9096] as const;

class AndroidAuthService {
  private providers: LoginProvider[] = [];
  private session: AuthSession | null = null;
  private selectedProvider: LoginProvider = defaultProvider();
  private preferredGfnToken: "id" | "access" = "id";

  async initialize(): Promise<void> {
    const state = await readPreferenceJson<PersistedAuthState>(AUTH_STATE_KEY, { session: null, selectedProvider: null });
    if (state.selectedProvider) this.selectedProvider = normalizeProvider(state.selectedProvider);
    if (state.session) this.session = { ...state.session, provider: normalizeProvider(state.session.provider) };
    this.preferredGfnToken = state.preferredGfnToken === "access" ? "access" : "id";
  }

  private async persist(): Promise<void> {
    await writePreferenceJson(AUTH_STATE_KEY, { session: this.session, selectedProvider: this.selectedProvider, preferredGfnToken: this.preferredGfnToken });
  }

  private pickAuthRedirectPort(): number {
    return AUTH_REDIRECT_PORTS[0];
  }

  private async waitForAuthorizationCode(authUrl: string, port: number, timeoutMs = 180000): Promise<string> {
    const { code, redirectUri } = await LocalhostAuth.startLogin({ authUrl, port, timeoutMs });
    const expectedRedirectUri = authRedirectUri(port);
    if (redirectUri && redirectUri !== expectedRedirectUri) {
      throw new Error(`Unexpected OAuth redirect URI: ${redirectUri}`);
    }
    return code;
  }

  async getProviders(): Promise<LoginProvider[]> {
    if (this.providers.length > 0) return this.providers;
    try {
      const payload = await httpRequest<ServiceUrlsResponse>(SERVICE_URLS_ENDPOINT, { headers: { Accept: "application/json", "User-Agent": GFN_USER_AGENT } });
      this.providers = (payload.gfnServiceInfo?.gfnServiceEndpoints ?? []).map<LoginProvider>((entry) => ({ idpId: entry.idpId, code: entry.loginProviderCode, displayName: entry.loginProviderCode === "BPC" ? "bro.game" : entry.loginProviderDisplayName, streamingServiceUrl: entry.streamingServiceUrl, priority: entry.loginProviderPriority ?? 0 })).sort((a, b) => a.priority - b.priority).map(normalizeProvider);
      if (this.providers.length === 0) this.providers = [defaultProvider()];
    } catch {
      this.providers = [defaultProvider()];
    }
    return this.providers;
  }

  getSession(): AuthSession | null { return this.session; }
  getSelectedProvider(): LoginProvider { return this.selectedProvider; }
  private getProviderStreamingBaseUrl(explicit?: string): string {
    if (explicit?.trim()) return normalizeBaseUrl(explicit);
    if (this.session?.provider?.streamingServiceUrl) return normalizeBaseUrl(this.session.provider.streamingServiceUrl);
    if (this.selectedProvider?.streamingServiceUrl) return normalizeBaseUrl(this.selectedProvider.streamingServiceUrl);
    return DEFAULT_PROVIDER_STREAMING_URL;
  }

  private candidateGfnTokens(session: AuthSession): Array<{ type: "id" | "access"; token: string }> {
    const out: Array<{ type: "id" | "access"; token: string }> = [];
    if (this.preferredGfnToken === "id") {
      if (session.tokens.idToken?.trim()) out.push({ type: "id", token: session.tokens.idToken.trim() });
      if (session.tokens.accessToken?.trim()) out.push({ type: "access", token: session.tokens.accessToken.trim() });
    } else {
      if (session.tokens.accessToken?.trim()) out.push({ type: "access", token: session.tokens.accessToken.trim() });
      if (session.tokens.idToken?.trim()) out.push({ type: "id", token: session.tokens.idToken.trim() });
    }
    return out.filter((entry, index, list) => list.findIndex((other) => other.token === entry.token) === index);
  }

  private async chooseGfnToken(session: AuthSession): Promise<string> {
    const candidates = this.candidateGfnTokens(session);
    if (candidates.length === 0) throw new Error("No authenticated session token available");
    if (candidates.length === 1) {
      this.preferredGfnToken = candidates[0].type;
      return candidates[0].token;
    }

    const streamingBaseUrl = this.getProviderStreamingBaseUrl(session.provider.streamingServiceUrl);
    for (const candidate of candidates) {
      try {
        await getVpcInfo(candidate.token, streamingBaseUrl);
        if (this.preferredGfnToken !== candidate.type) {
          this.preferredGfnToken = candidate.type;
          await this.persist();
        }
        return candidate.token;
      } catch {}
    }

    return candidates[0].token;
  }

  private async ensureClientToken(tokens: AuthTokens, userId: string): Promise<AuthTokens> {
    const hasUsable = Boolean(tokens.clientToken) && !isNearExpiry(tokens.clientTokenExpiresAt, CLIENT_TOKEN_REFRESH_WINDOW_MS);
    if (hasUsable || isExpired(tokens.expiresAt)) return tokens;
    const clientToken = await requestClientToken(tokens.accessToken);
    return { ...tokens, clientToken: clientToken.token, clientTokenExpiresAt: clientToken.expiresAt, clientTokenLifetimeMs: clientToken.lifetimeMs };
  }

  async login(input: AuthLoginRequest): Promise<AuthSession> {
    const providers = await this.getProviders();
    const selected = providers.find((provider) => provider.idpId === input.providerIdpId) ?? this.selectedProvider ?? providers[0] ?? defaultProvider();
    this.selectedProvider = normalizeProvider(selected);
    const { identifier } = await Device.getId();
    const deviceId = identifier || `android-${Math.random().toString(16).slice(2)}`;
    const { verifier, challenge } = await createPkce();
    const port = this.pickAuthRedirectPort();
    const authUrl = buildAuthUrl(this.selectedProvider, challenge, deviceId, port);
    const code = await this.waitForAuthorizationCode(authUrl, port);
    const initialTokens = await exchangeAuthorizationCode(code, verifier, port);
    const user = await fetchUserInfo(initialTokens);
    let tokens = initialTokens;
    try { tokens = await this.ensureClientToken(tokens, user.userId); } catch {}
    this.session = { provider: this.selectedProvider, tokens, user };
    await this.chooseGfnToken(this.session);
    try {
      const subscription = await fetchSubscriptionInfo({ userId: user.userId, providerStreamingBaseUrl: this.selectedProvider.streamingServiceUrl });
      this.session = { ...this.session, user: { ...this.session.user, membershipTier: subscription.membershipTier ?? this.session.user.membershipTier } };
    } catch {}
    await this.persist();
    return this.session;
  }

  async logout(): Promise<void> { this.session = null; this.preferredGfnToken = "id"; await this.persist(); }

  async ensureValidSessionWithStatus(forceRefresh = false): Promise<AuthSessionResult> {
    if (!this.session) return { session: null, refresh: { attempted: false, forced: forceRefresh, outcome: "not_attempted", message: "No saved session found." } };
    const userId = this.session.user.userId;
    let tokens = this.session.tokens;
    if (!tokens.clientToken && !isExpired(tokens.expiresAt)) {
      try {
        const withClientToken = await this.ensureClientToken(tokens, userId);
        if (withClientToken.clientToken && withClientToken.clientToken !== tokens.clientToken) {
          this.session = { ...this.session, tokens: withClientToken };
          tokens = withClientToken;
          await this.persist();
        }
      } catch {}
    }
    const shouldRefreshNow = forceRefresh || isNearExpiry(tokens.expiresAt, TOKEN_REFRESH_WINDOW_MS);
    if (!shouldRefreshNow) return { session: this.session, refresh: { attempted: false, forced: forceRefresh, outcome: "not_attempted", message: "Session token is still valid." } };
    const applyRefreshedTokens = async (refreshedTokens: AuthTokens, source: "client_token" | "refresh_token"): Promise<AuthSessionResult> => {
      let user = this.session?.user;
      try { user = await fetchUserInfo(refreshedTokens); } catch {}
      this.session = { provider: this.session!.provider, tokens: refreshedTokens, user: user ?? this.session!.user };
      await this.persist();
      return { session: this.session, refresh: { attempted: true, forced: forceRefresh, outcome: "refreshed", message: `Saved session token refreshed via ${source === "client_token" ? "client token" : "refresh token"}.` } };
    };
    const refreshErrors: string[] = [];
    if (tokens.clientToken) {
      try {
        const refreshed = await refreshWithClientToken(tokens.clientToken, userId);
        const merged = await this.ensureClientToken(mergeTokenSnapshot(tokens, refreshed), userId);
        return applyRefreshedTokens(merged, "client_token");
      } catch (error) { refreshErrors.push(error instanceof Error ? error.message : String(error)); }
    }
    if (tokens.refreshToken) {
      try {
        const refreshedOAuth = await refreshAuthTokens(tokens.refreshToken);
        const merged = await this.ensureClientToken({ ...tokens, ...refreshedOAuth, clientToken: tokens.clientToken, clientTokenExpiresAt: tokens.clientTokenExpiresAt, clientTokenLifetimeMs: tokens.clientTokenLifetimeMs }, userId);
        return applyRefreshedTokens(merged, "refresh_token");
      } catch (error) { refreshErrors.push(error instanceof Error ? error.message : String(error)); }
    }
    const expired = isExpired(tokens.expiresAt);
    if (!tokens.clientToken && !tokens.refreshToken) {
      if (expired) {
        await this.logout();
        return { session: null, refresh: { attempted: true, forced: forceRefresh, outcome: "missing_refresh_token", message: "Saved session expired and has no refresh mechanism. Please log in again." } };
      }
      return { session: this.session, refresh: { attempted: true, forced: forceRefresh, outcome: "missing_refresh_token", message: "No refresh token available. Using saved session token." } };
    }
    if (expired) {
      await this.logout();
      return { session: null, refresh: { attempted: true, forced: forceRefresh, outcome: "failed", message: "Token refresh failed and the saved session expired. Please log in again.", error: refreshErrors.join(" | ") } };
    }
    return { session: this.session, refresh: { attempted: true, forced: forceRefresh, outcome: "failed", message: "Token refresh failed. Using saved session token.", error: refreshErrors.join(" | ") } };
  }

  async resolveJwtToken(explicitToken?: string): Promise<string> {
    if (this.session) {
      const result = await this.ensureValidSessionWithStatus(false);
      if (!result.session) throw new Error("No authenticated session available");
      return result.session.tokens.idToken?.trim() || result.session.tokens.accessToken.trim();
    }
    if (explicitToken?.trim()) return explicitToken.trim();
    throw new Error("No authenticated session available");
  }
}

const authStore = new AndroidAuthService();
const initPromise = authStore.initialize();
async function ensureInitialized(): Promise<void> { await initPromise; }
async function getStoredSettings(): Promise<Settings> { return { ...DEFAULT_SETTINGS, ...(await readPreferenceJson<Partial<Settings>>(SETTINGS_KEY, {})) }; }
async function saveSettings(settings: Settings): Promise<void> { await writePreferenceJson(SETTINGS_KEY, settings); }

async function getVpcInfo(token: string | undefined, streamingBaseUrl: string): Promise<{ regions: StreamRegion[]; vpcId: string | null }> { const headers: Record<string, string> = { Accept: "application/json", "nv-client-id": LCARS_CLIENT_ID, "nv-client-type": "BROWSER", "nv-client-version": GFN_CLIENT_VERSION, "nv-client-streamer": "WEBRTC", "nv-device-os": "ANDROID", "nv-device-type": "PHONE", "User-Agent": GFN_USER_AGENT }; if (token) headers.Authorization = `GFNJWT ${token}`; const payload = await httpRequest<ServerInfoResponse>(`${normalizeBaseUrl(streamingBaseUrl)}v2/serverInfo`, { headers }); const regions = (payload.metaData ?? []).filter((entry) => entry.value.startsWith("https://") && entry.key !== "gfn-regions" && !entry.key.startsWith("gfn-")).map<StreamRegion>((entry) => ({ name: entry.key, url: normalizeBaseUrl(entry.value) })).sort((a, b) => a.name.localeCompare(b.name)); return { regions, vpcId: payload.requestStatus?.serverId ?? null }; }
async function getVpcId(token: string, providerStreamingBaseUrl?: string): Promise<string> { try { return (await getVpcInfo(token, providerStreamingBaseUrl ?? authStore.getSession()?.provider.streamingServiceUrl ?? authStore.getSelectedProvider().streamingServiceUrl ?? DEFAULT_PROVIDER_STREAMING_URL)).vpcId ?? "GFN-PC"; } catch { return "GFN-PC"; } }
function buildCatalogHeaders(token: string): Record<string, string> { return { Accept: "application/json, text/plain, */*", "Content-Type": "application/graphql", Origin: "https://play.geforcenow.com", Referer: "https://play.geforcenow.com/", Authorization: `GFNJWT ${token}`, "nv-client-id": LCARS_CLIENT_ID, "nv-client-type": "NATIVE", "nv-client-version": GFN_CLIENT_VERSION, "nv-client-streamer": "NVIDIA-CLASSIC", "nv-device-os": "WINDOWS", "nv-device-type": "DESKTOP", "nv-device-make": "UNKNOWN", "nv-device-model": "UNKNOWN", "nv-browser-type": "CHROME", "User-Agent": GFN_USER_AGENT }; }
async function fetchPanels(token: string, panelNames: string[], vpcId: string): Promise<GraphQlResponse> { const params = new URLSearchParams({ requestType: panelNames.includes("LIBRARY") ? "panels/Library" : "panels/MainV2", extensions: JSON.stringify({ persistedQuery: { sha256Hash: PANELS_QUERY_HASH } }), huId: randomHuId(), variables: JSON.stringify({ vpcId, locale: DEFAULT_LOCALE, panelNames }) }); return httpRequest<GraphQlResponse>(`${GFN_GRAPHQL_URL}?${params.toString()}`, { headers: buildCatalogHeaders(token) }); }
function flattenPanels(payload: GraphQlResponse): GameInfo[] { if (payload.errors?.length) throw new Error(payload.errors.map((error) => error.message).join(", ")); const games: GameInfo[] = []; for (const panel of payload.data?.panels ?? []) { for (const section of panel.sections ?? []) { for (const item of section.items ?? []) { if (item.__typename === "GameItem" && item.app) games.push(appToGame(item.app)); } } } return dedupeGames(games); }
async function fetchAppMetaData(token: string, appIds: string[], vpcId: string): Promise<GraphQlResponse> { const params = new URLSearchParams({ requestType: "appMetaData", extensions: JSON.stringify({ persistedQuery: { sha256Hash: APP_METADATA_QUERY_HASH } }), huId: randomHuId(), variables: JSON.stringify({ vpcId, locale: DEFAULT_LOCALE, appIds: [...new Set(appIds)] }) }); return httpRequest<GraphQlResponse>(`${GFN_GRAPHQL_URL}?${params.toString()}`, { headers: buildCatalogHeaders(token) }); }
async function enrichGamesWithMetadata(token: string, vpcId: string, games: GameInfo[]): Promise<GameInfo[]> { const uuids = [...new Set(games.map((game) => game.uuid).filter((uuid): uuid is string => Boolean(uuid)))]; if (uuids.length === 0) return games; const appById = new Map<string, AppData>(); for (let index = 0; index < uuids.length; index += 40) { const payload = await fetchAppMetaData(token, uuids.slice(index, index + 40), vpcId); if (payload.errors?.length) throw new Error(payload.errors.map((error) => error.message).join(", ")); for (const app of payload.data?.apps?.items ?? []) appById.set(app.id, app); } return games.map((game) => { if (!game.uuid) return game; const metadata = appById.get(game.uuid); return metadata ? mergeAppMetaIntoGame(game, metadata) : game; }); }
async function fetchCatalog(kind: "MAIN" | "LIBRARY", token: string, providerStreamingBaseUrl?: string): Promise<GameInfo[]> {
  const vpcId = await getVpcId(token, providerStreamingBaseUrl);
  const payload = await fetchPanels(token, [kind], vpcId);
  return flattenPanels(payload);
}
const androidCatalogCache = new Map<string, { games: GameInfo[]; loadedAtMs: number }>();
const androidCatalogInflight = new Map<string, Promise<GameInfo[]>>();
function androidCatalogCacheKey(kind: "MAIN" | "LIBRARY", token: string, providerStreamingBaseUrl?: string): string {
  return `${kind}:${providerStreamingBaseUrl ?? ""}:${token.slice(-32)}`;
}
async function fetchCatalogCached(kind: "MAIN" | "LIBRARY", token: string, providerStreamingBaseUrl?: string): Promise<GameInfo[]> {
  const key = androidCatalogCacheKey(kind, token, providerStreamingBaseUrl);
  const cached = androidCatalogCache.get(key);
  if (cached && Date.now() - cached.loadedAtMs < ANDROID_CATALOG_CACHE_TTL_MS) {
    return cached.games;
  }
  const existing = androidCatalogInflight.get(key);
  if (existing) {
    return existing;
  }
  const request = fetchCatalog(kind, token, providerStreamingBaseUrl)
    .then((games) => {
      androidCatalogCache.set(key, { games, loadedAtMs: Date.now() });
      return games;
    })
    .finally(() => {
      androidCatalogInflight.delete(key);
    });
  androidCatalogInflight.set(key, request);
  return request;
}
async function fetchPublicCatalog(): Promise<GameInfo[]> { const payload = await httpRequest<RawPublicGame[]>(PUBLIC_GAMES_URL, { headers: { "User-Agent": GFN_USER_AGENT } }); return payload.filter((item) => item.status === "AVAILABLE" && item.title).map((item) => { const id = String(item.id ?? item.title ?? "unknown"); const steamAppId = item.steamUrl?.split("/app/")[1]?.split("/")[0]; return { id, uuid: id, launchAppId: isNumericId(id) ? id : undefined, title: item.title ?? id, selectedVariantIndex: 0, variants: [{ id, store: "Unknown", supportedControls: [] }], imageUrl: steamAppId ? `https://cdn.cloudflare.steamstatic.com/steam/apps/${steamAppId}/library_600x900.jpg` : undefined } satisfies GameInfo; }); }
function catalogSearchText(game: GameInfo): string {
  return [
    game.title,
    game.description,
    game.longDescription,
    game.publisherName,
    ...(game.genres ?? []),
    ...(game.featureLabels ?? []),
    ...game.variants.map((variant) => variant.store),
  ]
    .filter((value): value is string => typeof value === "string" && value.trim().length > 0)
    .join(" ")
    .toLowerCase();
}
function toCatalogOptionId(groupId: string, rawValue: string): string {
  return `${groupId}:${rawValue.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-")}`;
}
function buildAndroidCatalogFilterGroups(games: GameInfo[]): CatalogFilterGroup[] {
  const stores = new Map<string, string>();
  const genres = new Map<string, string>();
  const subscriptions = new Map<string, string>();
  for (const game of games) {
    for (const variant of game.variants) {
      const store = variant.store?.trim();
      if (store) stores.set(toCatalogOptionId("digital_store", store), store);
    }
    for (const genre of game.genres ?? []) {
      const value = genre.trim();
      if (value) genres.set(toCatalogOptionId("genre", value), value);
    }
    const tier = game.membershipTierLabel?.trim();
    if (tier) subscriptions.set(toCatalogOptionId("subscriptions", tier), tier);
  }
  const groups: CatalogFilterGroup[] = [];
  if (stores.size > 0) {
    groups.push({
      id: "digital_store",
      label: "Stores",
      options: [...stores.entries()].map(([id, label]) => ({ id, rawId: label, label, groupId: "digital_store", groupLabel: "Stores" })),
    });
  }
  if (genres.size > 0) {
    groups.push({
      id: "genre",
      label: "Genres",
      options: [...genres.entries()].map(([id, label]) => ({ id, rawId: label, label, groupId: "genre", groupLabel: "Genres" })),
    });
  }
  if (subscriptions.size > 0) {
    groups.push({
      id: "subscriptions",
      label: "Membership",
      options: [...subscriptions.entries()].map(([id, label]) => ({ id, rawId: label, label, groupId: "subscriptions", groupLabel: "Membership" })),
    });
  }
  return groups;
}
function gameMatchesCatalogFilter(game: GameInfo, filterId: string): boolean {
  const [groupId, ...rest] = filterId.split(":");
  const normalizedValue = rest.join(":");
  if (!groupId || !normalizedValue) return true;
  switch (groupId) {
    case "digital_store":
      return game.variants.some((variant) => toCatalogOptionId("digital_store", variant.store) === filterId);
    case "genre":
      return (game.genres ?? []).some((genre) => toCatalogOptionId("genre", genre) === filterId);
    case "subscriptions":
      return Boolean(game.membershipTierLabel && toCatalogOptionId("subscriptions", game.membershipTierLabel) === filterId);
    default:
      return true;
  }
}
function sortAndroidCatalogGames(games: GameInfo[], sortId: string): GameInfo[] {
  const sorted = [...games];
  switch (sortId) {
    case "title_az":
      sorted.sort((a, b) => a.title.localeCompare(b.title));
      break;
    case "title_za":
      sorted.sort((a, b) => b.title.localeCompare(a.title));
      break;
    case "last_played":
      sorted.sort((a, b) => (Date.parse(b.lastPlayed ?? "") || 0) - (Date.parse(a.lastPlayed ?? "") || 0));
      break;
    default:
      break;
  }
  return sorted;
}
async function browseCatalogRequest(input: CatalogBrowseRequest): Promise<CatalogBrowseResult> {
  const token = await authStore.resolveJwtToken(input.token);
  const allGames = await fetchCatalogCached("MAIN", token, input.providerStreamingBaseUrl);
  const filterGroups = buildAndroidCatalogFilterGroups(allGames);
  const validFilterIds = new Set(filterGroups.flatMap((group) => group.options.map((option) => option.id)));
  const selectedFilterIds = (input.filterIds ?? []).filter((filterId) => validFilterIds.has(filterId));
  const selectedSortId = ANDROID_CATALOG_SORT_OPTIONS.some((option) => option.id === input.sortId)
    ? (input.sortId as string)
    : "relevance";
  const normalizedQuery = input.searchQuery?.trim().toLowerCase() ?? "";
  const searchedGames = normalizedQuery
    ? allGames.filter((game) => catalogSearchText(game).includes(normalizedQuery))
    : allGames;
  const filteredGames = selectedFilterIds.length > 0
    ? searchedGames.filter((game) => selectedFilterIds.every((filterId) => gameMatchesCatalogFilter(game, filterId)))
    : searchedGames;
  const sortedGames = sortAndroidCatalogGames(filteredGames, selectedSortId);
  return {
    games: sortedGames,
    numberReturned: sortedGames.length,
    numberSupported: sortedGames.length,
    totalCount: filteredGames.length,
    hasNextPage: false,
    searchQuery: input.searchQuery ?? "",
    selectedSortId,
    selectedFilterIds,
    filterGroups,
    sortOptions: ANDROID_CATALOG_SORT_OPTIONS,
  };
}
async function fetchPrintedWasteQueueRequest(): Promise<PrintedWasteQueueData> {
  const body = await httpRequest<unknown>(PRINTEDWASTE_QUEUE_URL, {
    headers: {
      Accept: "application/json",
      "User-Agent": GFN_USER_AGENT,
    },
  });
  if (!body || typeof body !== "object" || Array.isArray(body)) {
    throw new Error("PrintedWaste API response was not an object");
  }
  const payload = body as { status?: unknown; data?: unknown };
  if (payload.status !== true || !payload.data || typeof payload.data !== "object" || Array.isArray(payload.data)) {
    throw new Error("PrintedWaste API returned invalid queue data");
  }
  const normalized: PrintedWasteQueueData = {};
  for (const [zoneId, rawZone] of Object.entries(payload.data as Record<string, unknown>)) {
    if (!rawZone || typeof rawZone !== "object" || Array.isArray(rawZone)) continue;
    const zone = rawZone as Record<string, unknown>;
    const queuePosition = zone.QueuePosition;
    const lastUpdated = zone["Last Updated"];
    const region = zone.Region;
    const eta = zone.eta;
    if (typeof queuePosition !== "number" || !Number.isFinite(queuePosition)) continue;
    if (typeof lastUpdated !== "number" || !Number.isFinite(lastUpdated)) continue;
    if (typeof region !== "string" || region.length === 0) continue;
    if (eta !== undefined && (typeof eta !== "number" || !Number.isFinite(eta))) continue;
    normalized[zoneId] = {
      QueuePosition: queuePosition,
      "Last Updated": lastUpdated,
      Region: region,
      ...(typeof eta === "number" ? { eta } : {}),
    };
  }
  if (Object.keys(normalized).length === 0) {
    throw new Error("PrintedWaste API returned no valid zones");
  }
  return normalized;
}
async function fetchPrintedWasteServerMappingRequest(): Promise<PrintedWasteServerMapping> {
  const body = await httpRequest<unknown>(PRINTEDWASTE_SERVER_MAPPING_URL, {
    headers: {
      Accept: "application/json",
      "User-Agent": GFN_USER_AGENT,
    },
  });
  if (!body || typeof body !== "object" || Array.isArray(body)) {
    throw new Error("PrintedWaste server mapping response was not an object");
  }
  const payload = body as { status?: unknown; data?: unknown };
  if (payload.status !== true || !payload.data || typeof payload.data !== "object" || Array.isArray(payload.data)) {
    throw new Error("PrintedWaste server mapping returned invalid data");
  }
  const normalized: PrintedWasteServerMapping = {};
  for (const [zoneId, rawZone] of Object.entries(payload.data as Record<string, unknown>)) {
    if (!rawZone || typeof rawZone !== "object" || Array.isArray(rawZone)) continue;
    const zone = rawZone as Record<string, unknown>;
    normalized[zoneId] = {
      ...(typeof zone.title === "string" ? { title: zone.title } : {}),
      ...(typeof zone.region === "string" ? { region: zone.region } : {}),
      ...(typeof zone.is4080Server === "boolean" ? { is4080Server: zone.is4080Server } : {}),
      ...(typeof zone.is5080Server === "boolean" ? { is5080Server: zone.is5080Server } : {}),
      ...(typeof zone.nuked === "boolean" ? { nuked: zone.nuked } : {}),
    };
  }
  return normalized;
}
async function resolveLaunchId(token: string, appIdOrUuid: string, providerStreamingBaseUrl?: string): Promise<string | null> { if (isNumericId(appIdOrUuid)) return appIdOrUuid; const vpcId = await getVpcId(token, providerStreamingBaseUrl); const payload = await fetchAppMetaData(token, [appIdOrUuid], vpcId); if (payload.errors?.length) throw new Error(payload.errors.map((error) => error.message).join(", ")); const app = payload.data?.apps?.items?.[0]; if (!app) return null; const selected = app.variants?.find((variant) => variant.gfn?.library?.selected === true); if (isNumericId(selected?.id)) return selected.id; const firstNumeric = app.variants?.find((variant) => isNumericId(variant.id)); if (firstNumeric) return firstNumeric.id; return isNumericId(app.id) ? app.id : null; }

function requestHeaders(options: { token: string; clientId: string; deviceId: string; includeOrigin?: boolean; deviceMake?: string; deviceModel?: string }): Record<string, string> { const headers: Record<string, string> = { Authorization: `GFNJWT ${options.token}`, Accept: "application/json, text/plain, */*", "Content-Type": "application/json", "User-Agent": GFN_USER_AGENT, "nv-browser-type": "CHROME", "nv-client-id": options.clientId, "nv-client-streamer": "NVIDIA-CLASSIC", "nv-client-type": "NATIVE", "nv-client-version": GFN_CLIENT_VERSION, "nv-device-make": options.deviceMake ?? "UNKNOWN", "nv-device-model": options.deviceModel ?? "UNKNOWN", "nv-device-os": "ANDROID", "nv-device-type": "PHONE", "x-device-id": options.deviceId }; if (options.includeOrigin !== false) { headers.Origin = "https://play.geforcenow.com"; headers.Referer = "https://play.geforcenow.com/"; } return headers; }
function cloudmatchUrl(zone: string): string { return `https://${zone}.cloudmatchbeta.nvidiagrid.net`; }
function resolveStreamingBaseUrl(zone: string, provided?: string): string { if (provided?.trim()) { const trimmed = provided.trim(); return trimmed.endsWith("/") ? trimmed.slice(0, -1) : trimmed; } return cloudmatchUrl(zone); }
function shouldUseServerIp(baseUrl: string): boolean { return baseUrl.includes("cloudmatchbeta.nvidiagrid.net"); }
function resolvePollStopBase(zone: string, provided?: string, serverIp?: string): string { const base = resolveStreamingBaseUrl(zone, provided); if (serverIp && shouldUseServerIp(base) && !isZoneHostname(serverIp)) return `https://${serverIp}`; return base; }
function parseResolution(input: string): { width: number; height: number } { const [rawWidth, rawHeight] = input.split("x"); const width = Number.parseInt(rawWidth ?? "", 10); const height = Number.parseInt(rawHeight ?? "", 10); if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) return { width: 1920, height: 1080 }; return { width, height }; }
function timezoneOffsetMs(): number { return -new Date().getTimezoneOffset() * 60 * 1000; }
function extractHostFromUrl(url: string): string | null { for (const prefix of ["rtsps://", "rtsp://", "wss://", "https://"]) { if (url.startsWith(prefix)) { const host = url.slice(prefix.length).split(":")[0]?.split("/")[0]; return host || null; } } return null; }
function isZoneHostname(ip: string): boolean { return ip.includes("cloudmatchbeta.nvidiagrid.net") || ip.includes("cloudmatch.nvidiagrid.net"); }
function isReadySessionStatus(status: number): boolean { return READY_SESSION_STATUSES.has(status); }
function buildSignalingUrl(resourcePath: string, serverIp: string): { signalingUrl: string; signalingHost: string | null } {
  if (resourcePath.startsWith("rtsps://") || resourcePath.startsWith("rtsp://")) {
    const host = extractHostFromUrl(resourcePath);
    if (host) {
      return { signalingUrl: `wss://${host}/nvst/`, signalingHost: host };
    }
    return { signalingUrl: `wss://${serverIp}:443/nvst/`, signalingHost: null };
  }

  if (resourcePath.startsWith("wss://")) {
    const authority = resourcePath.slice("wss://".length).split("/")[0] ?? null;
    return { signalingUrl: resourcePath, signalingHost: authority };
  }

  if (resourcePath.startsWith("/")) {
    return { signalingUrl: `wss://${serverIp}:443${resourcePath}`, signalingHost: null };
  }

  return { signalingUrl: `wss://${serverIp}:443/nvst/`, signalingHost: null };
}

function firstConnectionIp(connection: { ip?: string | string[] } | undefined): string | undefined {
  const rawIp = connection?.ip;
  return Array.isArray(rawIp) ? rawIp[0] : rawIp;
}

function resolveActiveSessionServerIp(connection: { ip?: string | string[]; resourcePath?: string } | undefined, controlIp?: string): string | undefined {
  return firstConnectionIp(connection) ?? (connection?.resourcePath ? extractHostFromUrl(connection.resourcePath) : undefined) ?? controlIp;
}

function resolveActiveSessionSignaling(connection: { ip?: string | string[]; resourcePath?: string } | undefined, serverIp?: string): { signalingServer?: string; signalingUrl?: string } {
  if (!serverIp) {
    return {};
  }
  const { signalingUrl, signalingHost } = buildSignalingUrl(connection?.resourcePath ?? "/nvst/", serverIp);
  const effectiveHost = signalingHost ?? serverIp;
  return {
    signalingServer: effectiveHost.includes(":") ? effectiveHost : `${effectiveHost}:443`,
    signalingUrl,
  };
}
function selectReadySessionToClaim(activeSessions: ActiveSessionInfo[], numericAppId: number): ActiveSessionInfo | null {
  return (
    activeSessions.find((session) => session.serverIp && session.appId === numericAppId && READY_SESSION_STATUSES.has(session.status)) ??
    activeSessions.find((session) => session.serverIp && READY_SESSION_STATUSES.has(session.status)) ??
    null
  );
}
function selectLaunchingSession(activeSessions: ActiveSessionInfo[], numericAppId: number): ActiveSessionInfo | null {
  return (
    activeSessions.find((session) => session.serverIp && session.appId === numericAppId && session.status === 1) ??
    activeSessions.find((session) => session.serverIp && session.status === 1) ??
    null
  );
}
async function stopActiveSessionsForCreate(token: string, streamingBaseUrl: string, zone: string): Promise<void> {
  const activeSessions = await getActiveSessionsRequest(token, streamingBaseUrl);
  const sessionsToStop = activeSessions.filter((session) => session.serverIp && ACTIVE_CREATE_SESSION_STATUSES.has(session.status));
  for (const activeSession of sessionsToStop) {
    await stopSessionRequest({
      token,
      streamingBaseUrl,
      serverIp: activeSession.serverIp,
      zone,
      sessionId: activeSession.sessionId,
    });
  }
}
function streamingServerIp(response: CloudMatchResponse): string | null { const connection = response.session.connectionInfo?.find((conn) => conn.usage === 14); const directIp = Array.isArray(connection?.ip) ? connection?.ip[0] : connection?.ip; if (directIp) return directIp; if (connection?.resourcePath) { const host = extractHostFromUrl(connection.resourcePath); if (host) return host; } const controlIp = response.session.sessionControlInfo?.ip; return Array.isArray(controlIp) ? controlIp[0] ?? null : controlIp ?? null; }
function resolveSignaling(response: CloudMatchResponse): { serverIp: string; signalingServer: string; signalingUrl: string; mediaConnectionInfo?: { ip: string; port: number } } {
  const connection =
    response.session.connectionInfo?.find((conn) => conn.usage === 14 && (conn.ip || conn.resourcePath))
    ?? response.session.connectionInfo?.find((conn) => conn.ip || conn.resourcePath);
  const serverIp = streamingServerIp(response);
  if (!serverIp) throw new Error("CloudMatch response did not include a signaling host");
  const { signalingUrl, signalingHost } = buildSignalingUrl(connection?.resourcePath ?? "/nvst/", serverIp);
  const effectiveHost = signalingHost ?? serverIp;
  const connectionIp = Array.isArray(connection?.ip) ? connection?.ip[0] : connection?.ip;
  return {
    serverIp,
    signalingServer: effectiveHost.includes(":") ? effectiveHost : `${effectiveHost}:443`,
    signalingUrl,
    mediaConnectionInfo: connection?.port && connectionIp ? { ip: connectionIp, port: connection.port } : undefined,
  };
}
function extractQueuePosition(payload: CloudMatchResponse): number | undefined { return toPositiveInt(payload.session.queuePosition) ?? toPositiveInt(payload.session.seatSetupInfo?.queuePosition) ?? toPositiveInt(payload.session.sessionProgress?.queuePosition) ?? toPositiveInt(payload.session.progressInfo?.queuePosition); }
function toColorQuality(bitDepth?: number, chromaFormat?: number): import("@shared/gfn").ColorQuality | undefined { if (bitDepth !== 0 && bitDepth !== 10) return undefined; if (chromaFormat !== 0 && chromaFormat !== 2) return undefined; if (bitDepth === 10) return chromaFormat === 2 ? "10bit_444" : "10bit_420"; return chromaFormat === 2 ? "8bit_444" : "8bit_420"; }
function normalizeIceServers(response: CloudMatchResponse): IceServer[] { const raw = response.session.iceServerConfiguration?.iceServers ?? []; const servers = raw.map((entry) => ({ urls: Array.isArray(entry.urls) ? entry.urls : [entry.urls], username: entry.username, credential: entry.credential })).filter((entry) => entry.urls.length > 0); if (servers.length > 0) return servers; return [{ urls: ["stun:s1.stun.gamestream.nvidia.com:19308"] }, { urls: ["stun:stun.l.google.com:19302"] }]; }
function normalizeSessionAdInfo(ad: CloudMatchSessionAd, index: number): import("@shared/gfn").SessionAdInfo | null { const adId = toOptionalString(ad.adId); const adMediaFiles = (ad.adMediaFiles ?? []).map((file) => ({ mediaFileUrl: toOptionalString(file.mediaFileUrl), encodingProfile: toOptionalString(file.encodingProfile) })).filter((file) => file.mediaFileUrl || file.encodingProfile).sort((left, right) => { const leftRank = left.encodingProfile ? GFN_AD_MEDIA_PROFILE_ORDER.get(left.encodingProfile) ?? Number.MAX_SAFE_INTEGER : Number.MAX_SAFE_INTEGER; const rightRank = right.encodingProfile ? GFN_AD_MEDIA_PROFILE_ORDER.get(right.encodingProfile) ?? Number.MAX_SAFE_INTEGER : Number.MAX_SAFE_INTEGER; return leftRank - rightRank; }); const preferredMediaFile = adMediaFiles.find((file) => file.mediaFileUrl); const mediaUrl = preferredMediaFile?.mediaFileUrl ?? toOptionalString(ad.adUrl) ?? toOptionalString(ad.mediaUrl) ?? toOptionalString(ad.videoUrl) ?? toOptionalString(ad.url); const adUrl = toOptionalString(ad.adUrl); const clickThroughUrl = toOptionalString(ad.clickThroughUrl); const title = toOptionalString(ad.title); const description = toOptionalString(ad.description); const adLengthInSeconds = typeof ad.adLengthInSeconds === "number" && Number.isFinite(ad.adLengthInSeconds) && ad.adLengthInSeconds > 0 ? ad.adLengthInSeconds : undefined; const durationMs = (adLengthInSeconds !== undefined ? Math.round(adLengthInSeconds * 1000) : undefined) ?? toPositiveInt(ad.durationMs) ?? toPositiveInt(ad.durationInMs); const adState = typeof ad.adState === "number" && Number.isFinite(ad.adState) ? Math.trunc(ad.adState) : undefined; if (!adId && !mediaUrl && !adUrl && adMediaFiles.length === 0 && !title && !description) return null; return { adId: adId ?? `ad-${index + 1}`, state: adState, adState, adUrl, mediaUrl, adMediaFiles, clickThroughUrl, adLengthInSeconds, durationMs, title, description }; }
function extractAdState(payload: CloudMatchResponse): SessionInfo["adState"] {
  const sessionAdsRequired =
    toBoolean(payload.session.sessionAdsRequired) ??
    toBoolean(payload.session.isAdsRequired) ??
    toBoolean(payload.session.sessionProgress?.isAdsRequired) ??
    toBoolean(payload.session.progressInfo?.isAdsRequired);
  const ads = (payload.session.sessionAds ?? []).map((ad, index) => normalizeSessionAdInfo(ad, index)).filter((ad): ad is import("@shared/gfn").SessionAdInfo => ad !== null);
  const opportunity = payload.session.opportunity;
  const normalizedOpportunity = opportunity ? { state: toOptionalString(opportunity.state), queuePaused: toBoolean(opportunity.queuePaused), gracePeriodSeconds: toPositiveInt(opportunity.gracePeriodSeconds), message: toOptionalString(opportunity.message), title: toOptionalString(opportunity.title), description: toOptionalString(opportunity.description) } : undefined;
  const queuePaused = normalizedOpportunity?.queuePaused ?? (typeof normalizedOpportunity?.state === "string" ? normalizedOpportunity.state.toLowerCase() === "graceperiodstart" : undefined);
  const gracePeriodSeconds = normalizedOpportunity?.gracePeriodSeconds;
  const effectiveIsAdsRequired = sessionAdsRequired ?? ads.length > 0;
  const message = normalizedOpportunity?.message ?? normalizedOpportunity?.description ?? (queuePaused ? "Resume ads to stay in queue." : effectiveIsAdsRequired ? "Finish ads to stay in queue." : undefined);
  if (!effectiveIsAdsRequired && ads.length === 0 && !queuePaused && !message) return undefined;
  return { isAdsRequired: effectiveIsAdsRequired, sessionAdsRequired, isQueuePaused: queuePaused, gracePeriodSeconds, message, sessionAds: ads, ads, opportunity: normalizedOpportunity, serverSentEmptyAds: payload.session.sessionAds == null };
}
function extractNegotiatedStreamProfile(payload: CloudMatchResponse): SessionInfo["negotiatedStreamProfile"] { const monitor = payload.session.sessionRequestData?.clientRequestMonitorSettings?.[0]; const finalized = payload.session.finalizedStreamingFeatures; const requested = payload.session.sessionRequestData?.requestedStreamingFeatures; const resolution = monitor?.widthInPixels && monitor?.heightInPixels ? `${Math.trunc(monitor.widthInPixels)}x${Math.trunc(monitor.heightInPixels)}` : undefined; const colorQuality = toColorQuality(finalized?.bitDepth ?? requested?.bitDepth, finalized?.chromaFormat ?? requested?.chromaFormat); const enableL4S = finalized?.enabledL4S ?? requested?.enabledL4S; if (!resolution && !monitor?.framesPerSecond && !colorQuality && enableL4S === undefined) return undefined; return { resolution, fps: monitor?.framesPerSecond, colorQuality, enableL4S: enableL4S === undefined ? undefined : Boolean(enableL4S) }; }
async function toSessionInfo(zone: string, streamingBaseUrl: string, payload: CloudMatchResponse, clientId?: string, deviceId?: string): Promise<SessionInfo> { if (payload.requestStatus.statusCode !== 1) throw new Error(payload.requestStatus.statusDescription ?? payload.requestStatus.statusName ?? "Session request failed"); const signaling = resolveSignaling(payload); return { sessionId: payload.session.sessionId, status: payload.session.status, queuePosition: extractQueuePosition(payload), seatSetupStep: payload.session.seatSetupInfo?.seatSetupStep, adState: extractAdState(payload), zone, streamingBaseUrl, serverIp: signaling.serverIp, signalingServer: signaling.signalingServer, signalingUrl: signaling.signalingUrl, gpuType: payload.session.gpuType, iceServers: normalizeIceServers(payload), mediaConnectionInfo: signaling.mediaConnectionInfo, negotiatedStreamProfile: extractNegotiatedStreamProfile(payload), clientId, deviceId }; }
async function buildDeviceIdentifiers(): Promise<{ clientId: string; deviceId: string; deviceMake: string; deviceModel: string }> { const [{ identifier }, info] = await Promise.all([Device.getId(), Device.getInfo()]); return { clientId: crypto.randomUUID(), deviceId: identifier || crypto.randomUUID(), deviceMake: info.manufacturer || "UNKNOWN", deviceModel: info.model || "UNKNOWN" }; }
function buildSessionRequestBody(input: SessionCreateRequest) { const { width, height } = parseResolution(input.settings.resolution); const hdrEnabled = false; const accountLinked = input.accountLinked ?? true; return { sessionRequestData: { appId: input.appId, internalTitle: input.internalTitle || null, availableSupportedControllers: [], networkTestSessionId: null, parentSessionId: null, clientIdentification: "GFN-PC", deviceHashId: crypto.randomUUID(), clientVersion: "30.0", sdkVersion: "1.0", streamerVersion: 1, clientPlatformName: "windows", clientRequestMonitorSettings: [{ widthInPixels: width, heightInPixels: height, framesPerSecond: input.settings.fps, sdrHdrMode: hdrEnabled ? 1 : 0, displayData: { desiredContentMaxLuminance: hdrEnabled ? 1000 : 0, desiredContentMinLuminance: 0, desiredContentMaxFrameAverageLuminance: hdrEnabled ? 500 : 0 }, dpi: 100 }], useOps: true, audioMode: 2, metaData: [{ key: "SubSessionId", value: crypto.randomUUID() }, { key: "wssignaling", value: "1" }, { key: "GSStreamerType", value: "WebRTC" }, { key: "networkType", value: "Unknown" }, { key: "ClientImeSupport", value: "0" }, { key: "clientPhysicalResolution", value: JSON.stringify({ horizontalPixels: width, verticalPixels: height }) }, { key: "surroundAudioInfo", value: "2" }], sdrHdrMode: hdrEnabled ? 1 : 0, clientDisplayHdrCapabilities: hdrEnabled ? { version: 1, hdrEdrSupportedFlagsInUint32: 1, staticMetadataDescriptorId: 0 } : null, surroundAudioInfo: 0, remoteControllersBitmap: 0, clientTimezoneOffset: timezoneOffsetMs(), enhancedStreamMode: 1, appLaunchMode: 1, secureRTSPSupported: false, partnerCustomData: "", accountLinked, enablePersistingInGameSettings: true, userAge: 26, requestedStreamingFeatures: { reflex: input.settings.fps >= 120, bitDepth: colorQualityBitDepth(input.settings.colorQuality), cloudGsync: false, enabledL4S: input.settings.enableL4S, mouseMovementFlags: 0, trueHdr: hdrEnabled, supportedHidDevices: 0, profile: 0, fallbackToLogicalResolution: false, hidDevices: null, chromaFormat: colorQualityChromaFormat(input.settings.colorQuality), prefilterMode: 0, prefilterSharpness: 0, prefilterNoiseReduction: 0, hudStreamingMode: 0, sdrColorSpace: 2, hdrColorSpace: hdrEnabled ? 4 : 0 } } }; }
function buildClaimRequestBody(sessionId: string, appId: string, _settings: StreamSettings): unknown { const deviceId = crypto.randomUUID(); const subSessionId = crypto.randomUUID(); return { action: 2, data: "RESUME", sessionRequestData: { audioMode: 2, remoteControllersBitmap: 0, sdrHdrMode: 0, networkTestSessionId: null, availableSupportedControllers: [], clientVersion: "30.0", deviceHashId: deviceId, internalTitle: null, clientPlatformName: "windows", metaData: [{ key: "SubSessionId", value: subSessionId }, { key: "wssignaling", value: "1" }, { key: "GSStreamerType", value: "WebRTC" }, { key: "networkType", value: "Unknown" }, { key: "ClientImeSupport", value: "0" }], surroundAudioInfo: 0, clientTimezoneOffset: timezoneOffsetMs(), clientIdentification: "GFN-PC", parentSessionId: null, appId: Number.parseInt(appId, 10), streamerVersion: 1, appLaunchMode: 1, sdkVersion: "1.0", enhancedStreamMode: 1, useOps: true, clientDisplayHdrCapabilities: null, accountLinked: true, partnerCustomData: "", enablePersistingInGameSettings: true, secureRTSPSupported: false, userAge: 26, requestedStreamingFeatures: { reflex: false, bitDepth: 0, cloudGsync: false, profile: 0, fallbackToLogicalResolution: false, chromaFormat: 0, prefilterMode: 0, hudStreamingMode: 0 } }, metaData: [] }; }

async function createSessionRequest(input: SessionCreateRequest): Promise<SessionInfo> {
  const token = await authStore.resolveJwtToken(input.token);
  if (!isNumericId(input.appId)) {
    throw new Error(`Invalid launch appId '${input.appId}' (must be numeric)`);
  }

  const streamingBaseUrl = resolveStreamingBaseUrl(input.zone, input.streamingBaseUrl);
  const forceNewSession = input.existingSessionStrategy === "force-new";

  if (forceNewSession) {
    await stopActiveSessionsForCreate(token, streamingBaseUrl, input.zone);
  } else {
    try {
      const activeSessions = await getActiveSessionsRequest(token, streamingBaseUrl);
      const numericAppId = Number.parseInt(input.appId, 10);
      const readyCandidate = selectReadySessionToClaim(activeSessions, numericAppId);
      if (readyCandidate?.serverIp) {
        return claimSessionRequest({
          token,
          streamingBaseUrl,
          sessionId: readyCandidate.sessionId,
          serverIp: readyCandidate.serverIp,
          appId: input.appId,
          settings: input.settings,
        });
      }

      const launchingCandidate = selectLaunchingSession(activeSessions, numericAppId);
      if (launchingCandidate?.serverIp) {
        return pollSessionRequest({
          token,
          streamingBaseUrl,
          serverIp: launchingCandidate.serverIp,
          zone: input.zone,
          sessionId: launchingCandidate.sessionId,
        });
      }
    } catch (error) {
      console.warn("[Android] Failed to inspect existing sessions before create:", error);
    }
  }

  const { clientId, deviceId, deviceMake, deviceModel } = await buildDeviceIdentifiers();
  const keyboardLayout = resolveGfnKeyboardLayout(input.settings.keyboardLayout ?? DEFAULT_KEYBOARD_LAYOUT, "linux");
  const languageCode = input.settings.gameLanguage ?? "en_US";
  const response = await httpRequest<CloudMatchResponse>(`${streamingBaseUrl}/v2/session?${new URLSearchParams({ keyboardLayout, languageCode }).toString()}`, { method: "POST", headers: requestHeaders({ token, clientId, deviceId, deviceMake, deviceModel }), data: buildSessionRequestBody(input) });
  return toSessionInfo(input.zone, streamingBaseUrl, response, clientId, deviceId);
}
async function pollSessionRequest(input: SessionPollRequest): Promise<SessionInfo> {
  const token = await authStore.resolveJwtToken(input.token);
  const clientId = input.clientId ?? crypto.randomUUID();
  const deviceId = input.deviceId ?? (await Device.getId()).identifier ?? crypto.randomUUID();
  const zoneBase = resolveStreamingBaseUrl(input.zone, input.streamingBaseUrl);
  let base = resolvePollStopBase(input.zone, input.streamingBaseUrl, input.serverIp);
  const headers = requestHeaders({ token, clientId, deviceId, includeOrigin: false });
  const readSession = (targetBase: string) => httpRequest<CloudMatchResponse>(`${targetBase}/v2/session/${input.sessionId}`, { headers });

  let response: CloudMatchResponse;
  try {
    response = await readSession(base);
  } catch (error) {
    const shouldRetryViaZone =
      base !== zoneBase &&
      isNativeHttpError(error) &&
      (error.status === 403 || error.status === 404 || error.status === 409);

    if (!shouldRetryViaZone) {
      throw error;
    }

    console.warn(`[Android] Direct session poll failed with HTTP ${error.status}; retrying via zone endpoint.`);
    base = zoneBase;
    response = await readSession(zoneBase);
  }

  const baseHost = new URL(base).hostname;
  const realServerIp = streamingServerIp(response);
  const polledViaZone = isZoneHostname(baseHost);
  const realIpDiffers = Boolean(realServerIp && realServerIp.length > 0 && !isZoneHostname(realServerIp) && realServerIp !== input.serverIp);
  if (polledViaZone && realIpDiffers && isReadySessionStatus(response.session.status)) {
    const directBase = `https://${realServerIp}`;
    try {
      const directResponse = await readSession(directBase);
      if (directResponse.requestStatus.statusCode === 1) return toSessionInfo(input.zone, directBase, directResponse, clientId, deviceId);
    } catch {}
  }
  return toSessionInfo(input.zone, base, response, clientId, deviceId);
}
async function stopSessionRequest(input: SessionStopRequest): Promise<void> { const token = await authStore.resolveJwtToken(input.token); const clientId = input.clientId ?? crypto.randomUUID(); const deviceId = input.deviceId ?? (await Device.getId()).identifier ?? crypto.randomUUID(); const base = resolvePollStopBase(input.zone, input.streamingBaseUrl, input.serverIp); await httpRequest<string>(`${base}/v2/session/${input.sessionId}`, { method: "DELETE", headers: requestHeaders({ token, clientId, deviceId }), responseType: "text" }); }
async function reportSessionAdRequest(input: SessionAdReportRequest): Promise<SessionInfo> { const token = await authStore.resolveJwtToken(input.token); const clientId = input.clientId ?? crypto.randomUUID(); const deviceId = input.deviceId ?? (await Device.getId()).identifier ?? crypto.randomUUID(); const base = resolvePollStopBase(input.zone, input.streamingBaseUrl, input.serverIp); const clientTimestamp = input.clientTimestamp ?? Math.floor(Date.now() / 1000); const response = await httpRequest<CloudMatchResponse>(`${base}/v2/session/${input.sessionId}`, { method: "PUT", headers: requestHeaders({ token, clientId, deviceId }), data: { action: SESSION_MODIFY_ACTION_AD_UPDATE, adUpdates: [{ adId: input.adId, adAction: AD_ACTION_CODES[input.action], clientTimestamp, ...(typeof input.watchedTimeInMs === "number" ? { watchedTimeInMs: input.watchedTimeInMs } : {}), ...(typeof input.pausedTimeInMs === "number" ? { pausedTimeInMs: input.pausedTimeInMs } : {}), ...(input.cancelReason ? { cancelReason: input.cancelReason } : {}), ...(input.errorInfo ? { errorInfo: input.errorInfo } : {}) }] } }); return toSessionInfo(input.zone, base, response, clientId, deviceId); }
async function getActiveSessionsRequest(token: string, streamingBaseUrl?: string): Promise<ActiveSessionInfo[]> { const base = resolveStreamingBaseUrl("", streamingBaseUrl || authStore.getSelectedProvider().streamingServiceUrl); try { const response = await httpRequest<CloudMatchResponse>(`${base}/v2/session`, { headers: requestHeaders({ token, clientId: LCARS_CLIENT_ID, deviceId: crypto.randomUUID(), includeOrigin: false }) }); if (response.requestStatus.statusCode !== 1) return []; return (response.sessions ?? []).filter((session) => session.status === 1 || session.status === 2 || session.status === 3).map((session) => { const connection = session.connectionInfo?.find((entry) => entry.usage === 14 && (entry.ip || entry.resourcePath)) ?? session.connectionInfo?.find((entry) => entry.ip || entry.resourcePath); const controlIpRaw = session.sessionControlInfo?.ip; const controlIp = Array.isArray(controlIpRaw) ? controlIpRaw[0] : controlIpRaw; const serverIp = resolveActiveSessionServerIp(connection, controlIp); const { signalingServer, signalingUrl } = resolveActiveSessionSignaling(connection, serverIp); const monitor = session.monitorSettings?.[0]; const rawAppId = session.sessionRequestData?.appId ?? session.appId; const appId = typeof rawAppId === "string" || typeof rawAppId === "number" ? Number(rawAppId) : 0; return { sessionId: session.sessionId, appId: Number.isFinite(appId) ? appId : 0, gpuType: session.gpuType, status: session.status, streamingBaseUrl: base, serverIp, signalingServer, signalingUrl, resolution: session.resolution ?? (monitor?.widthInPixels && monitor?.heightInPixels ? `${monitor.widthInPixels}x${monitor.heightInPixels}` : undefined), fps: session.fps ?? monitor?.framesPerSecond }; }); } catch { return []; } }
async function claimSessionRequest(input: SessionClaimRequest): Promise<SessionInfo> { const token = await authStore.resolveJwtToken(input.token); const clientId = crypto.randomUUID(); const deviceId = (await Device.getId()).identifier || crypto.randomUUID(); const settings = input.settings ?? { resolution: "1920x1080", fps: 60, maxBitrateMbps: 75, codec: "H264", colorQuality: "8bit_420", keyboardLayout: DEFAULT_KEYBOARD_LAYOUT, gameLanguage: "en_US", enableL4S: false, enableCloudGsync: false }; const appId = input.appId ?? "0"; const keyboardLayout = resolveGfnKeyboardLayout(settings.keyboardLayout ?? DEFAULT_KEYBOARD_LAYOUT, "linux"); const languageCode = settings.gameLanguage ?? "en_US"; let effectiveServerIp = input.serverIp; if (isZoneHostname(effectiveServerIp)) { try { const zoneBase = `https://${effectiveServerIp}`; const prefetchPayload = await httpRequest<CloudMatchResponse>(`${zoneBase}/v2/session/${input.sessionId}`, { headers: requestHeaders({ token, clientId, deviceId, includeOrigin: false }) }); const realIp = streamingServerIp(prefetchPayload); if (realIp) effectiveServerIp = realIp; } catch {} } const effectiveBase = `https://${effectiveServerIp}`; const sessionUrl = `${effectiveBase}/v2/session/${input.sessionId}`; let preClaimStatus: number | null = null; try { const preClaimPayload = await httpRequest<CloudMatchResponse>(sessionUrl, { headers: requestHeaders({ token, clientId, deviceId, includeOrigin: false }) }); preClaimStatus = preClaimPayload.session?.status ?? null; } catch {} if (preClaimStatus !== 1) { const claimUrl = `${sessionUrl}?${new URLSearchParams({ keyboardLayout, languageCode }).toString()}`; await httpRequest<unknown>(claimUrl, { method: "PUT", headers: requestHeaders({ token, clientId, deviceId }), data: buildClaimRequestBody(input.sessionId, appId, settings) }); } for (let attempt = 0; attempt < 60; attempt += 1) { if (attempt > 0) await new Promise((resolve) => window.setTimeout(resolve, 1000)); try { const polled = await httpRequest<CloudMatchResponse>(sessionUrl, { headers: requestHeaders({ token, clientId, deviceId, includeOrigin: false }) }); if (polled.session.status === 2 || polled.session.status === 3) return toSessionInfo("", effectiveBase, polled, clientId, deviceId); if (polled.session.status > 3 && polled.session.status !== 6) break; } catch {} } throw new Error("Session did not become ready after claiming"); }
async function fetchSubscriptionInfo(input: SubscriptionFetchRequest): Promise<SubscriptionInfo> { const token = await authStore.resolveJwtToken(input.token); const vpcId = await getVpcId(token, input.providerStreamingBaseUrl); const userId = input.userId || authStore.getSession()?.user.userId; if (!userId) throw new Error("No authenticated user available for subscription lookup"); const url = new URL(MES_URL); url.searchParams.append("serviceName", "gfn_pc"); url.searchParams.append("languageCode", "en_US"); url.searchParams.append("vpcId", vpcId); url.searchParams.append("userId", userId); const data = await httpRequest<SubscriptionResponse>(url.toString(), { headers: { Authorization: `GFNJWT ${token}`, Accept: "application/json", "nv-client-id": LCARS_CLIENT_ID, "nv-client-type": "NATIVE", "nv-client-version": GFN_CLIENT_VERSION, "nv-client-streamer": "NVIDIA-CLASSIC", "nv-device-os": "ANDROID", "nv-device-type": "PHONE" } }); const allottedMinutes = data.allottedTimeInMinutes ?? 0; const purchasedMinutes = data.purchasedTimeInMinutes ?? 0; const rolledOverMinutes = data.rolledOverTimeInMinutes ?? 0; const totalMinutes = data.totalTimeInMinutes ?? allottedMinutes + purchasedMinutes + rolledOverMinutes; const remainingMinutes = data.remainingTimeInMinutes ?? 0; const usedMinutes = Math.max(totalMinutes - remainingMinutes, 0); const storageAddon = data.addons?.find((addon) => addon.type === "STORAGE" && addon.subType === "PERMANENT_STORAGE" && addon.status === "OK"); const attr = (key: string) => storageAddon?.attributes?.find((entry) => entry.key === key)?.textValue; return { membershipTier: data.membershipTier ?? "FREE", subscriptionType: data.type, subscriptionSubType: data.subType, allottedHours: allottedMinutes / 60, purchasedHours: purchasedMinutes / 60, rolledOverHours: rolledOverMinutes / 60, usedHours: usedMinutes / 60, remainingHours: remainingMinutes / 60, totalHours: totalMinutes / 60, firstEntitlementStartDateTime: data.firstEntitlementStartDateTime, serverRegionId: vpcId, currentSpanStartDateTime: data.currentSpanStartDateTime, currentSpanEndDateTime: data.currentSpanEndDateTime, notifyUserWhenTimeRemainingInMinutes: data.notifications?.notifyUserWhenTimeRemainingInMinutes, notifyUserOnSessionWhenRemainingTimeInMinutes: data.notifications?.notifyUserOnSessionWhenRemainingTimeInMinutes, state: data.currentSubscriptionState?.state, isGamePlayAllowed: data.currentSubscriptionState?.isGamePlayAllowed, isUnlimited: data.subType === "UNLIMITED", storageAddon: storageAddon ? { type: "PERMANENT_STORAGE", sizeGb: attr("TOTAL_STORAGE_SIZE_IN_GB") ? Number(attr("TOTAL_STORAGE_SIZE_IN_GB")) : undefined, usedGb: attr("USED_STORAGE_SIZE_IN_GB") ? Number(attr("USED_STORAGE_SIZE_IN_GB")) : undefined, regionName: attr("STORAGE_METRO_REGION_NAME"), regionCode: attr("STORAGE_METRO_REGION") } : undefined, entitledResolutions: (data.features?.resolutions ?? []).map((res) => ({ width: res.widthInPixels, height: res.heightInPixels, fps: res.framesPerSecond })) }; }
function unsupported(message: string): Promise<never> { return Promise.reject(new Error(message)); }
function dataUrlExtension(dataUrl: string): string { if (dataUrl.startsWith("data:image/jpeg")) return "jpg"; if (dataUrl.startsWith("data:image/webp")) return "webp"; return "png"; }
function decodeDataUrl(dataUrl: string): string { const match = /^data:[^;]+;base64,(.+)$/i.exec(dataUrl); if (!match || !match[1]) throw new Error("Invalid data URL"); return match[1]; }
function encodeBase64(bytes: Uint8Array): string { let binary = ""; const chunkSize = 0x8000; for (let index = 0; index < bytes.length; index += chunkSize) { binary += String.fromCharCode(...bytes.subarray(index, index + chunkSize)); } return btoa(binary); }
async function ensureDirectory(path: string): Promise<void> { await ensureDir(path, { relativeToBaseDir: false }); }
async function listDirectory(path: string): Promise<Array<{ name: string }>> { return (await readDir(path, { relativeToBaseDir: false })).map((name) => ({ name })); }
function showSessionConflictPrompt(): Promise<SessionConflictChoice> {
  if (typeof document === "undefined" || !document.body) {
    if (window.confirm("An active GeForce NOW session was found. Resume it?")) {
      return Promise.resolve("resume");
    }
    if (window.confirm("Start a new session instead?")) {
      return Promise.resolve("new");
    }
    return Promise.resolve("cancel");
  }

  return new Promise<SessionConflictChoice>((resolve) => {
    const previousActive = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const overlay = document.createElement("div");
    overlay.setAttribute("role", "presentation");
    overlay.style.cssText = "position:fixed;inset:0;z-index:2147483647;display:flex;align-items:center;justify-content:center;background:rgba(2,6,23,0.78);padding:24px;";

    const dialog = document.createElement("div");
    dialog.setAttribute("role", "dialog");
    dialog.setAttribute("aria-modal", "true");
    dialog.setAttribute("aria-labelledby", "android-session-conflict-title");
    dialog.style.cssText = "width:min(100%,420px);border-radius:20px;padding:24px;background:#0f172a;color:#e2e8f0;box-shadow:0 24px 60px rgba(0,0,0,0.45);display:flex;flex-direction:column;gap:20px;";

    const title = document.createElement("div");
    title.id = "android-session-conflict-title";
    title.textContent = "Active session found";
    title.style.cssText = "font:600 20px/1.3 system-ui,sans-serif;";

    const body = document.createElement("div");
    body.textContent = "Choose whether to resume the current GeForce NOW session, start a new one, or cancel.";
    body.style.cssText = "font:400 14px/1.5 system-ui,sans-serif;color:#cbd5e1;";

    const actions = document.createElement("div");
    actions.style.cssText = "display:flex;flex-direction:column;gap:12px;";

    const cleanup = () => {
      document.removeEventListener("keydown", onKeyDown, true);
      overlay.remove();
      previousActive?.focus();
    };

    const settle = (choice: SessionConflictChoice) => {
      cleanup();
      resolve(choice);
    };

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        settle("cancel");
      }
    };

    const createAction = (label: string, choice: SessionConflictChoice, accent: string): HTMLButtonElement => {
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = label;
      button.style.cssText = `border:none;border-radius:14px;padding:14px 16px;background:${accent};color:#f8fafc;font:600 15px/1 system-ui,sans-serif;`;
      button.addEventListener("click", () => settle(choice));
      return button;
    };

    const resumeButton = createAction("Resume", "resume", "#2563eb");
    const newButton = createAction("Start New", "new", "#475569");
    const cancelButton = createAction("Cancel", "cancel", "#1e293b");

    overlay.addEventListener("click", (event) => {
      if (event.target === overlay) {
        settle("cancel");
      }
    });

    actions.append(resumeButton, newButton, cancelButton);
    dialog.append(title, body, actions);
    overlay.append(dialog);
    document.body.append(overlay);
    document.addEventListener("keydown", onKeyDown, true);
    window.setTimeout(() => resumeButton.focus(), 0);
  });
}
type RecordingMeta = RecordingEntry;
type ScreenshotMeta = Omit<ScreenshotEntry, "dataUrl">;
interface RecordingDraft { id: string; fileName: string; filePath: string; pendingWrite: Promise<void>; }
const RECORDING_META_KEY = RECORDINGS_KEY;
const THANKS_CACHE_KEY = "opennow.android.thanks.v1";
async function readRecordingMeta(): Promise<RecordingMeta[]> { return readPreferenceJson<RecordingMeta[]>(RECORDING_META_KEY, []); }
async function writeRecordingMeta(entries: RecordingMeta[]): Promise<void> { await writePreferenceJson(RECORDING_META_KEY, entries); }
async function readScreenshotMeta(): Promise<ScreenshotMeta[]> { return readPreferenceJson<ScreenshotMeta[]>(SCREENSHOTS_KEY, []); }
async function writeScreenshotMeta(entries: ScreenshotMeta[]): Promise<void> { await writePreferenceJson(SCREENSHOTS_KEY, entries); }
async function readDataUrl(path: string, mimeType: string): Promise<string> { return `data:${mimeType};base64,${await readFileBase64(path, { relativeToBaseDir: false })}`; }
function recordingExtension(mimeType: string): "mp4" | "webm" { return mimeType.includes("mp4") ? "mp4" : "webm"; }
function screenshotMimeType(fileName: string): string { if (fileName.endsWith(".jpg")) return "image/jpeg"; if (fileName.endsWith(".webp")) return "image/webp"; return "image/png"; }
async function createRecordingDraft(recordingId: string, mimeType: string): Promise<RecordingDraft> { await ensureDirectory(RECORDING_DIR); const fileName = `${Date.now()}-${recordingId}.${recordingExtension(mimeType)}`; const filePath = `${RECORDING_DIR}/${fileName}`; await writeBase64File(filePath, "", { relativeToBaseDir: false }); return { id: recordingId, fileName, filePath, pendingWrite: Promise.resolve() }; }
function enqueueRecordingWrite(state: RecordingDraft, chunk: ArrayBuffer): Promise<void> { state.pendingWrite = state.pendingWrite.then(async () => { await appendBase64File(state.filePath, encodeBase64(new Uint8Array(chunk.slice(0))), { relativeToBaseDir: false }); }); return state.pendingWrite; }
async function cleanupRecordingDraft(state: RecordingDraft): Promise<void> { await state.pendingWrite.catch(() => undefined); await deleteFile(state.filePath, { relativeToBaseDir: false }); }

async function applyAndroidFullscreen(value: boolean): Promise<void> {
  document.body.dataset.androidFullscreen = value ? "true" : "false";
  if (value) {
    await StatusBar.hide().catch(() => undefined);
    return;
  }
  await StatusBar.show().catch(() => undefined);
  await StatusBar.setStyle({ style: Style.Dark }).catch(() => undefined);
}

async function exitAndroidFullscreenState(): Promise<void> {
  if (document.fullscreenElement) {
    await document.exitFullscreen().catch(() => undefined);
  }
  if (document.body.dataset.androidFullscreen === "true") {
    await applyAndroidFullscreen(false);
  }
}

const signalingListeners = new Set<(event: MainToRendererSignalingEvent) => void>();
let signalingClient: BrowserSignalingClient | null = null;
const recordingStates = new Map<string, RecordingDraft>();

const api: OpenNowApi = {
  getAuthSession: async (input: AuthSessionRequest = {}) => { await ensureInitialized(); return authStore.ensureValidSessionWithStatus(Boolean(input.forceRefresh)); },
  getLoginProviders: async () => { await ensureInitialized(); return authStore.getProviders(); },
  getRegions: async (input: RegionsFetchRequest = {}) => { await ensureInitialized(); try { const token = await authStore.resolveJwtToken(input.token); return (await getVpcInfo(token, authStore.getSession()?.provider.streamingServiceUrl ?? authStore.getSelectedProvider().streamingServiceUrl)).regions; } catch { return []; } },
  login: async (input: AuthLoginRequest) => { await ensureInitialized(); return authStore.login(input); },
  logout: async () => { await ensureInitialized(); await authStore.logout(); },
  fetchSubscription: async (input) => fetchSubscriptionInfo(input),
  fetchMainGames: async (input) => fetchCatalogCached("MAIN", await authStore.resolveJwtToken(input.token), input.providerStreamingBaseUrl),
  fetchLibraryGames: async (input) => fetchCatalogCached("LIBRARY", await authStore.resolveJwtToken(input.token), input.providerStreamingBaseUrl),
  browseCatalog: async (input) => browseCatalogRequest(input),
  fetchPublicGames: async () => fetchPublicCatalog(),
  resolveLaunchAppId: async (input) => resolveLaunchId(await authStore.resolveJwtToken(input.token), input.appIdOrUuid, input.providerStreamingBaseUrl),
  createSession: async (input) => createSessionRequest(input),
  pollSession: async (input) => pollSessionRequest(input),
  reportSessionAd: async (input) => reportSessionAdRequest(input),
  stopSession: async (input) => stopSessionRequest(input),
  getActiveSessions: async (token, streamingBaseUrl) => getActiveSessionsRequest(await authStore.resolveJwtToken(token), streamingBaseUrl),
  claimSession: async (input) => claimSessionRequest(input),
  showSessionConflictDialog: async (): Promise<SessionConflictChoice> => showSessionConflictPrompt(),
  connectSignaling: async (input: SignalingConnectRequest) => { signalingClient?.disconnect(); signalingClient = new BrowserSignalingClient(); signalingClient.onEvent((event) => { for (const listener of signalingListeners) listener(event); }); await signalingClient.connect(input); },
  disconnectSignaling: async () => { signalingClient?.disconnect(); signalingClient = null; },
  sendAnswer: async (input: SendAnswerRequest) => { await signalingClient?.sendAnswer(input); },
  sendIceCandidate: async (input: IceCandidatePayload) => { await signalingClient?.sendIceCandidate(input); },
  requestKeyframe: async (input: KeyframeRequest) => { await signalingClient?.requestKeyframe(input); },
  onSignalingEvent: (listener) => { signalingListeners.add(listener); return () => signalingListeners.delete(listener); },
  onToggleFullscreen: () => () => undefined,
  quitApp: async () => unsupported("Quit app is not supported on Android."),
  getUpdaterState: async (): Promise<AppUpdaterState> => DEFAULT_ANDROID_UPDATER_STATE,
  checkForUpdates: async (): Promise<AppUpdaterState> => DEFAULT_ANDROID_UPDATER_STATE,
  downloadUpdate: async (): Promise<AppUpdaterState> => DEFAULT_ANDROID_UPDATER_STATE,
  installUpdateAndRestart: async (): Promise<AppUpdaterState> => DEFAULT_ANDROID_UPDATER_STATE,
  onUpdaterStateChanged: () => () => undefined,
  toggleFullscreen: async () => { const next = document.body.dataset.androidFullscreen !== "true"; await applyAndroidFullscreen(next); },
  setFullscreen: async (value: boolean) => { await applyAndroidFullscreen(value); },
  togglePointerLock: async () => unsupported("Pointer lock is not supported on Android."),
  getSettings: async () => getStoredSettings(),
  setSetting: async (key, value) => { const current = await getStoredSettings(); await saveSettings({ ...current, [key]: value }); },
  resetSettings: async () => { await saveSettings(DEFAULT_SETTINGS); return { ...DEFAULT_SETTINGS }; },
  getMicrophonePermission: async (): Promise<MicrophonePermissionResult> => ({ platform: "android", isMacOs: false, status: "not-applicable", granted: true, canRequest: true, shouldUseBrowserApi: true }),
  exportLogs: async () => unsupported("Log export is not supported on Android in this pass."),
  pingRegions: async (regions: StreamRegion[]): Promise<PingResult[]> => Promise.all(regions.map(tcpPingRegion)),
  saveScreenshot: async (input: ScreenshotSaveRequest): Promise<ScreenshotEntry> => { await ensureDirectory(SCREENSHOT_DIR); const fileName = `${Date.now()}-${Math.random().toString(16).slice(2)}.${dataUrlExtension(input.dataUrl)}`; const filePath = `${SCREENSHOT_DIR}/${fileName}`; await writeBase64File(filePath, decodeDataUrl(input.dataUrl), { relativeToBaseDir: false }); const stat = await Filesystem.stat({ path: filePath, directory: Directory.Data }); const entry: ScreenshotEntry = { id: fileName, fileName, filePath, createdAtMs: Number(stat.ctime ?? Date.now()), sizeBytes: stat.size, dataUrl: input.dataUrl, gameTitle: input.gameTitle }; const entries = await readScreenshotMeta(); await writeScreenshotMeta([{ id: entry.id, fileName: entry.fileName, filePath: entry.filePath, createdAtMs: entry.createdAtMs, sizeBytes: entry.sizeBytes, gameTitle: entry.gameTitle }, ...entries.filter((item) => item.id !== entry.id)]); return entry; },
  listScreenshots: async (): Promise<ScreenshotEntry[]> => { const files = await listDirectory(SCREENSHOT_DIR); const metadata = await readScreenshotMeta(); const metaById = new Map(metadata.map((entry) => [entry.id, entry])); const entries = await Promise.all(files.map(async (file) => { const filePath = `${SCREENSHOT_DIR}/${file.name}`; const stat = await Filesystem.stat({ path: filePath, directory: Directory.Data }); const meta = metaById.get(file.name); return { id: file.name, fileName: file.name, filePath, createdAtMs: meta?.createdAtMs ?? Number(stat.ctime ?? Date.now()), sizeBytes: meta?.sizeBytes ?? stat.size, gameTitle: meta?.gameTitle, dataUrl: await readDataUrl(filePath, screenshotMimeType(file.name)) } satisfies ScreenshotEntry; })); return entries.sort((a, b) => b.createdAtMs - a.createdAtMs); },
  deleteScreenshot: async (input: ScreenshotDeleteRequest) => { await deleteFile(`${SCREENSHOT_DIR}/${input.id}`, { relativeToBaseDir: false }); const entries = await readScreenshotMeta(); await writeScreenshotMeta(entries.filter((entry) => entry.id !== input.id)); },
  saveScreenshotAs: async (_input: ScreenshotSaveAsRequest): Promise<ScreenshotSaveAsResult> => unsupported("Screenshot export is not supported on Android.") as Promise<ScreenshotSaveAsResult>,
  onTriggerScreenshot: () => () => undefined,
  beginRecording: async (input: RecordingBeginRequest): Promise<RecordingBeginResult> => { const recordingId = crypto.randomUUID(); recordingStates.set(recordingId, await createRecordingDraft(recordingId, input.mimeType)); return { recordingId }; },
  sendRecordingChunk: async (input: RecordingChunkRequest) => { const state = recordingStates.get(input.recordingId); if (!state) return; await enqueueRecordingWrite(state, input.chunk); },
  finishRecording: async (input: RecordingFinishRequest): Promise<RecordingEntry> => { const state = recordingStates.get(input.recordingId); if (!state) throw new Error("Recording session not found."); try { await state.pendingWrite; const stat = await Filesystem.stat({ path: state.filePath, directory: Directory.Data }); const entry: RecordingMeta = { id: input.recordingId, fileName: state.fileName, filePath: state.filePath, createdAtMs: Number(stat.ctime ?? Date.now()), sizeBytes: stat.size, durationMs: input.durationMs, gameTitle: input.gameTitle, thumbnailDataUrl: input.thumbnailDataUrl }; const entries = await readRecordingMeta(); await writeRecordingMeta([entry, ...entries.filter((item) => item.id !== entry.id)]); return entry; } catch (error) { await cleanupRecordingDraft(state); throw error; } finally { recordingStates.delete(input.recordingId); } },
  abortRecording: async (input: RecordingAbortRequest) => { const state = recordingStates.get(input.recordingId); recordingStates.delete(input.recordingId); if (!state) return; await cleanupRecordingDraft(state); },
  listRecordings: async (): Promise<RecordingEntry[]> => { const entries = await readRecordingMeta(); return entries.sort((a, b) => b.createdAtMs - a.createdAtMs); },
  deleteRecording: async (input: RecordingDeleteRequest) => { const entries = await readRecordingMeta(); const match = entries.find((entry) => entry.id === input.id); if (match) await deleteFile(match.filePath, { relativeToBaseDir: false }); await writeRecordingMeta(entries.filter((entry) => entry.id !== input.id)); },
  showRecordingInFolder: async () => unsupported("Folder access is not supported on Android."),
  listMediaByGame: async (input = {}): Promise<MediaListingResult> => { const screenshots = await api.listScreenshots(); const recordings = await api.listRecordings(); const title = input.gameTitle?.trim().toLowerCase(); return { screenshots: screenshots.filter((entry) => !title || entry.gameTitle?.trim().toLowerCase() === title).map((entry) => ({ ...entry })), videos: recordings.filter((entry) => !title || entry.gameTitle?.trim().toLowerCase() === title).map((entry) => ({ ...entry })) }; },
  getMediaThumbnail: async (input: { filePath: string }) => { if (input.filePath.startsWith(SCREENSHOT_DIR)) return readDataUrl(input.filePath, "image/png"); const recordings = await readRecordingMeta(); return recordings.find((entry) => entry.filePath === input.filePath)?.thumbnailDataUrl ?? null; },
  showMediaInFolder: async () => unsupported("Folder access is not supported on Android."),
  deleteCache: async () => { await Promise.all([clearDirectory(SCREENSHOT_DIR, { relativeToBaseDir: false }), clearDirectory(RECORDING_DIR, { relativeToBaseDir: false }), writeRecordingMeta([]), writeScreenshotMeta([]), removePreference(THANKS_CACHE_KEY)]); },
  fetchPrintedWasteQueue: async (): Promise<PrintedWasteQueueData> => fetchPrintedWasteQueueRequest(),
  fetchPrintedWasteServerMapping: async (): Promise<PrintedWasteServerMapping> => fetchPrintedWasteServerMappingRequest(),
  getThanksData: async (): Promise<ThankYouDataResult> => { const cached = await readPreferenceJson<ThankYouDataResult | null>(THANKS_CACHE_KEY, null); if (cached) return cached; const placeholder: ThankYouDataResult = { contributors: [], supporters: [], contributorsError: "Community data is unavailable on Android in this pass." }; await writePreferenceJson(THANKS_CACHE_KEY, placeholder); return placeholder; },
};

void CapacitorApp.addListener("backButton", () => {
  if (document.fullscreenElement || document.body.dataset.androidFullscreen === "true") {
    void exitAndroidFullscreenState();
  }
});

export const capacitorPlatform: OpenNowPlatform = { info: { kind: "android", capabilities: { isAndroid: true, isElectron: false, supportsQuitApp: false, supportsPointerLockToggle: false, supportsDesktopFullscreen: false, supportsLogExport: false, supportsCacheDeletion: false, supportsMediaFolderAccess: false, supportsScreenshotExport: false, supportsPersistentMedia: true, supportsKeyboardShortcuts: false, supportsControllerExitApp: false } }, api };
