import { App as CapacitorApp } from "@capacitor/app";
import { Browser } from "@capacitor/browser";
import { CapacitorHttp, registerPlugin, type PluginListenerHandle } from "@capacitor/core";
import { Device } from "@capacitor/device";
import { Directory, Filesystem } from "@capacitor/filesystem";
import { Preferences } from "@capacitor/preferences";
import { StatusBar, Style } from "@capacitor/status-bar";

import type {
  ActiveSessionInfo,
  AuthLoginRequest,
  AuthSession,
  AuthSessionRequest,
  AuthSessionResult,
  AuthTokens,
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
  StreamRegion,
  SubscriptionFetchRequest,
  SubscriptionInfo,
  ThankYouDataResult,
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
  gravatarUrl,
  isExpired,
  isNearExpiry,
  normalizeProvider,
  toExpiresAt,
  userFromJwt,
} from "@shared/gfnRuntime";
import { DEFAULT_SETTINGS } from "@shared/settings";
import type { OpenNowPlatform } from "../types";
import { BrowserSignalingClient } from "./browserSignaling";

const AUTH_STATE_KEY = "opennow.android.auth-state.v1";
const SETTINGS_KEY = "opennow.android.settings.v1";
const RECORDINGS_KEY = "opennow.android.recordings.v1";
const SCREENSHOT_DIR = "opennow-media/screenshots";
const RECORDING_DIR = "opennow-media/recordings";
const PUBLIC_GAMES_URL = "https://static.nvidiagrid.net/supported-public-game-list/locales/gfnpc-en-US.json";
const PANELS_QUERY_HASH = "f8e26265a5db5c20e1334a6872cf04b6e3970507697f6ae55a6ddefa5420daf0";
const APP_METADATA_QUERY_HASH = "39187e85b6dcf60b7279a5f233288b0a8b69a8b1dbcfb5b25555afdcb988f0d7";
const DEFAULT_LOCALE = "en_US";

interface LocalhostAuthPlugin {
  startServer(): Promise<{ port: number }>;
  waitForCode(options: { timeoutMs?: number }): Promise<{ code: string }>;
  stopServer(): Promise<void>;
}

const LocalhostAuth = registerPlugin<LocalhostAuthPlugin>("LocalhostAuth");

interface PersistedAuthState { session: AuthSession | null; selectedProvider: LoginProvider | null; }
interface TokenResponse { access_token: string; refresh_token?: string; id_token?: string; client_token?: string; expires_in?: number; }
interface ClientTokenResponse { client_token: string; expires_in?: number; }
interface ServiceUrlsResponse { gfnServiceInfo?: { gfnServiceEndpoints?: Array<{ idpId: string; loginProviderCode: string; loginProviderDisplayName: string; streamingServiceUrl: string; loginProviderPriority?: number; }>; }; }
interface ServerInfoResponse { requestStatus?: { serverId?: string }; metaData?: Array<{ key: string; value: string }>; }
interface GraphQlResponse { data?: { panels?: Array<{ sections?: Array<{ items?: Array<{ __typename: string; app?: AppData }> }> }>; apps?: { items: AppData[] } }; errors?: Array<{ message: string }>; }
interface AppData { id: string; title: string; description?: string; longDescription?: string; features?: unknown[]; gameFeatures?: unknown[]; appFeatures?: unknown[]; genres?: unknown[]; tags?: unknown[]; images?: { GAME_BOX_ART?: string; TV_BANNER?: string; HERO_IMAGE?: string }; variants?: Array<{ id: string; appStore: string; supportedControls?: string[]; gfn?: { library?: { selected?: boolean } } }>; gfn?: { playType?: string; minimumMembershipTierLabel?: string }; }
interface SubscriptionResponse { firstEntitlementStartDateTime?: string; type?: string; membershipTier?: string; allottedTimeInMinutes?: number; purchasedTimeInMinutes?: number; rolledOverTimeInMinutes?: number; remainingTimeInMinutes?: number; totalTimeInMinutes?: number; notifications?: { notifyUserWhenTimeRemainingInMinutes?: number; notifyUserOnSessionWhenRemainingTimeInMinutes?: number }; currentSpanStartDateTime?: string; currentSpanEndDateTime?: string; currentSubscriptionState?: { state?: string; isGamePlayAllowed?: boolean }; subType?: string; addons?: Array<{ type?: string; subType?: string; status?: string; attributes?: Array<{ key?: string; textValue?: string }> }>; features?: { resolutions?: Array<{ heightInPixels: number; widthInPixels: number; framesPerSecond: number }> }; }
interface RawPublicGame { id?: string | number; title?: string; steamUrl?: string; status?: string; }
interface CloudMatchResponse { requestStatus: { statusCode: number; statusName?: string; statusDescription?: string }; session: { sessionId: string; status: number; queuePosition?: number; seatSetupInfo?: { queuePosition?: number; seatSetupStep?: number }; sessionProgress?: { queuePosition?: number; isAdsRequired?: boolean }; progressInfo?: { queuePosition?: number; isAdsRequired?: boolean }; sessionAdsRequired?: boolean; isAdsRequired?: boolean; connectionInfo?: Array<{ usage?: number; ip?: string | string[]; port?: number; resourcePath?: string }>; sessionControlInfo?: { ip?: string | string[] }; gpuType?: string; iceServerConfiguration?: { iceServers?: Array<{ urls: string[] | string; username?: string; credential?: string }> }; sessionRequestData?: { clientRequestMonitorSettings?: Array<{ widthInPixels: number; heightInPixels: number; framesPerSecond: number }>; requestedStreamingFeatures?: { bitDepth?: number; chromaFormat?: number; enabledL4S?: boolean } }; finalizedStreamingFeatures?: { bitDepth?: number; chromaFormat?: number; enabledL4S?: boolean } }; sessions?: Array<{ sessionId: string; appId: number; gpuType?: string; status: number; sessionControlInfo?: { ip?: string | string[] }; connectionInfo?: Array<{ usage?: number; ip?: string | string[]; resourcePath?: string }>; resolution?: string; fps?: number }>; }

function ensureTrailingSlash(value: string): string { return value.endsWith("/") ? value : `${value}/`; }
function normalizeBaseUrl(value: string): string { return ensureTrailingSlash(value.trim()); }
function isNumericId(value: string | undefined): value is string { return typeof value === "string" && /^\d+$/.test(value); }
function randomHuId(): string { return `${Date.now().toString(16)}${Math.random().toString(16).slice(2)}`; }
function toOptionalString(value: unknown): string | undefined { if (typeof value !== "string") return undefined; const trimmed = value.trim(); return trimmed || undefined; }
function toPositiveInt(value: unknown): number | undefined { if (typeof value === "number" && Number.isFinite(value)) { const normalized = Math.trunc(value); return normalized > 0 ? normalized : undefined; } if (typeof value === "string" && value.trim()) { const parsed = Number.parseInt(value, 10); return Number.isFinite(parsed) && parsed > 0 ? parsed : undefined; } return undefined; }
function toBoolean(value: unknown): boolean | undefined { if (typeof value === "boolean") return value; if (typeof value === "number") return value !== 0; if (typeof value === "string") { const normalized = value.trim().toLowerCase(); if (normalized === "true" || normalized === "1") return true; if (normalized === "false" || normalized === "0") return false; } return undefined; }
function parseFeatureLabel(value: unknown): string | null { if (typeof value === "string") return value.trim() || null; if (value && typeof value === "object") { const candidate = value as Record<string, unknown>; for (const key of ["name", "label", "title", "displayName"]) { const raw = candidate[key]; if (typeof raw === "string" && raw.trim()) return raw.trim(); } } return null; }
function extractFeatureLabels(app: AppData): string[] { const out: string[] = []; for (const bucket of [app.features, app.gameFeatures, app.appFeatures, app.genres, app.tags]) { if (!Array.isArray(bucket)) continue; for (const entry of bucket) { const label = parseFeatureLabel(entry); if (label) out.push(label); } } return [...new Set(out)]; }
function extractGenres(app: AppData): string[] { if (!Array.isArray(app.genres)) return []; return [...new Set(app.genres.map(parseFeatureLabel).filter((value): value is string => Boolean(value)))]; }
function optimizeImage(url: string): string { return url.includes("img.nvidiagrid.net") ? `${url};f=webp;w=272` : url; }
function appToGame(app: AppData): GameInfo { const variants = app.variants?.map((variant) => ({ id: variant.id, store: variant.appStore, supportedControls: variant.supportedControls ?? [] })) ?? []; const selectedVariantIndex = app.variants?.findIndex((variant) => variant.gfn?.library?.selected === true) ?? 0; const safeIndex = Math.max(0, selectedVariantIndex); const selectedVariantId = variants[safeIndex]?.id; const fallbackNumericVariantId = variants.find((variant) => isNumericId(variant.id))?.id; const launchAppId = isNumericId(selectedVariantId) ? selectedVariantId : fallbackNumericVariantId ?? (isNumericId(app.id) ? app.id : undefined); const imageUrl = app.images?.GAME_BOX_ART ?? app.images?.TV_BANNER ?? app.images?.HERO_IMAGE ?? undefined; return { id: `${app.id}:${selectedVariantId ?? "default"}`, uuid: app.id, launchAppId, title: app.title, description: app.description, longDescription: app.longDescription, featureLabels: extractFeatureLabels(app), genres: extractGenres(app), imageUrl: imageUrl ? optimizeImage(imageUrl) : undefined, playType: app.gfn?.playType, membershipTierLabel: app.gfn?.minimumMembershipTierLabel, selectedVariantIndex: Math.max(0, selectedVariantIndex), variants }; }
function mergeAppMetaIntoGame(game: GameInfo, app: AppData): GameInfo { const variants = app.variants?.map((variant) => ({ id: variant.id, store: variant.appStore, supportedControls: variant.supportedControls ?? [] })) ?? game.variants; const selectedVariantId = game.id.split(":")[1]; const selectedVariantIndex = Math.max(0, variants.findIndex((variant) => variant.id === selectedVariantId)); const imageUrl = app.images?.GAME_BOX_ART ?? app.images?.TV_BANNER ?? app.images?.HERO_IMAGE ?? undefined; return { ...game, title: app.title || game.title, description: app.description ?? game.description, longDescription: app.longDescription ?? game.longDescription, featureLabels: extractFeatureLabels(app), genres: extractGenres(app), imageUrl: imageUrl ? optimizeImage(imageUrl) : game.imageUrl, playType: app.gfn?.playType ?? game.playType, membershipTierLabel: app.gfn?.minimumMembershipTierLabel ?? game.membershipTierLabel, selectedVariantIndex, variants }; }
async function readPreferenceJson<T>(key: string, fallback: T): Promise<T> { const { value } = await Preferences.get({ key }); if (!value) return fallback; try { return JSON.parse(value) as T; } catch { return fallback; } }
async function writePreferenceJson<T>(key: string, value: T): Promise<void> { await Preferences.set({ key, value: JSON.stringify(value) }); }
async function httpRequest<T>(url: string, options: { method?: string; headers?: Record<string, string>; data?: unknown; responseType?: "json" | "text" } = {}): Promise<T> { const response = await CapacitorHttp.request({ url, method: options.method ?? "GET", headers: options.headers, data: options.data, responseType: options.responseType ?? "json", readTimeout: 120000, connectTimeout: 30000 }); if (response.status < 200 || response.status >= 300) { const body = typeof response.data === "string" ? response.data : JSON.stringify(response.data); throw new Error(`HTTP ${response.status}: ${body.slice(0, 500)}`); } return response.data as T; }
function authRedirectUri(port: number): string { return `http://localhost:${port}`; }
async function createPkce(): Promise<{ verifier: string; challenge: string }> { const bytes = new Uint8Array(64); crypto.getRandomValues(bytes); let binary = ""; for (const value of bytes) binary += String.fromCharCode(value); const verifier = btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "").slice(0, 86); const challengeBuffer = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier)); let challengeBinary = ""; for (const value of new Uint8Array(challengeBuffer)) challengeBinary += String.fromCharCode(value); const challenge = btoa(challengeBinary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, ""); return { verifier, challenge }; }
function buildAuthUrl(provider: LoginProvider, challenge: string, deviceId: string, port: number): string { const nonce = `${Math.random().toString(16).slice(2)}${Math.random().toString(16).slice(2)}`; const params = new URLSearchParams({ response_type: "code", device_id: deviceId, scope: SCOPES, client_id: CLIENT_ID, redirect_uri: authRedirectUri(port), ui_locales: "en_US", nonce, prompt: "select_account", code_challenge: challenge, code_challenge_method: "S256", idp_id: provider.idpId }); return `${AUTH_ENDPOINT}?${params.toString()}`; }
async function exchangeAuthorizationCode(code: string, verifier: string, port: number): Promise<AuthTokens> { const body = new URLSearchParams({ grant_type: "authorization_code", code, redirect_uri: authRedirectUri(port), code_verifier: verifier }); const payload = await httpRequest<TokenResponse>(TOKEN_ENDPOINT, { method: "POST", headers: { "Content-Type": "application/x-www-form-urlencoded; charset=UTF-8", Origin: "https://nvfile", Referer: "https://nvfile/", Accept: "application/json, text/plain, */*", "User-Agent": GFN_USER_AGENT }, data: body.toString() }); return { accessToken: payload.access_token, refreshToken: payload.refresh_token, idToken: payload.id_token, expiresAt: toExpiresAt(payload.expires_in) }; }
async function refreshAuthTokens(refreshToken: string): Promise<AuthTokens> { const body = new URLSearchParams({ grant_type: "refresh_token", refresh_token: refreshToken, client_id: CLIENT_ID }); const payload = await httpRequest<TokenResponse>(TOKEN_ENDPOINT, { method: "POST", headers: { "Content-Type": "application/x-www-form-urlencoded; charset=UTF-8", Origin: "https://nvfile", Accept: "application/json, text/plain, */*", "User-Agent": GFN_USER_AGENT }, data: body.toString() }); return { accessToken: payload.access_token, refreshToken: payload.refresh_token ?? refreshToken, idToken: payload.id_token, expiresAt: toExpiresAt(payload.expires_in) }; }
async function requestClientToken(accessToken: string): Promise<{ token: string; expiresAt: number; lifetimeMs: number }> { const payload = await httpRequest<ClientTokenResponse>(CLIENT_TOKEN_ENDPOINT, { headers: { Authorization: `Bearer ${accessToken}`, Origin: "https://nvfile", Accept: "application/json, text/plain, */*", "User-Agent": GFN_USER_AGENT } }); const expiresAt = toExpiresAt(payload.expires_in); return { token: payload.client_token, expiresAt, lifetimeMs: Math.max(0, expiresAt - Date.now()) }; }
async function refreshWithClientToken(clientToken: string, userId: string): Promise<TokenResponse> { const body = new URLSearchParams({ grant_type: "urn:ietf:params:oauth:grant-type:client_token", client_token: clientToken, client_id: CLIENT_ID, sub: userId }); return httpRequest<TokenResponse>(TOKEN_ENDPOINT, { method: "POST", headers: { "Content-Type": "application/x-www-form-urlencoded; charset=UTF-8", Origin: "https://nvfile", Accept: "application/json, text/plain, */*", "User-Agent": GFN_USER_AGENT }, data: body.toString() }); }
function mergeTokenSnapshot(base: AuthTokens, refreshed: TokenResponse): AuthTokens { return { accessToken: refreshed.access_token, refreshToken: refreshed.refresh_token ?? base.refreshToken, idToken: refreshed.id_token, expiresAt: toExpiresAt(refreshed.expires_in), clientToken: refreshed.client_token ?? base.clientToken, clientTokenExpiresAt: base.clientTokenExpiresAt, clientTokenLifetimeMs: base.clientTokenLifetimeMs }; }
async function fetchUserInfo(tokens: AuthTokens): Promise<AuthSession["user"]> { const jwtUser = userFromJwt(tokens); if (jwtUser?.email || jwtUser?.avatarUrl) return jwtUser; const payload = await httpRequest<{ sub: string; preferred_username?: string; email?: string; picture?: string }>(USERINFO_ENDPOINT, { headers: { Authorization: `Bearer ${tokens.accessToken}`, Origin: "https://nvfile", Accept: "application/json", "User-Agent": GFN_USER_AGENT } }); return { userId: payload.sub, displayName: payload.preferred_username ?? payload.email?.split("@")[0] ?? "User", email: payload.email, avatarUrl: payload.picture ?? (payload.email ? gravatarUrl(payload.email) : undefined), membershipTier: jwtUser?.membershipTier ?? "FREE" }; }

class AndroidAuthService {
  private providers: LoginProvider[] = [];
  private session: AuthSession | null = null;
  private selectedProvider: LoginProvider = defaultProvider();

  async initialize(): Promise<void> {
    const state = await readPreferenceJson<PersistedAuthState>(AUTH_STATE_KEY, { session: null, selectedProvider: null });
    if (state.selectedProvider) this.selectedProvider = normalizeProvider(state.selectedProvider);
    if (state.session) this.session = { ...state.session, provider: normalizeProvider(state.session.provider) };
  }

  private async persist(): Promise<void> {
    await writePreferenceJson(AUTH_STATE_KEY, { session: this.session, selectedProvider: this.selectedProvider });
  }

  private async waitForAuthorizationCode(authUrl: string, port: number, timeoutMs = 180000): Promise<string> {
    let browserFinishedListener: PluginListenerHandle | null = null;
    let settled = false;

    const cleanup = async (): Promise<void> => {
      await Promise.allSettled([
        browserFinishedListener?.remove() ?? Promise.resolve(),
        LocalhostAuth.stopServer().catch(() => undefined),
        Browser.close().catch(() => undefined),
      ]);
      browserFinishedListener = null;
    };

    try {
      return await new Promise<string>(async (resolve, reject) => {
        const settle = (fn: () => void): void => {
          if (settled) return;
          settled = true;
          fn();
        };

        browserFinishedListener = await Browser.addListener("browserFinished", () => {
          settle(() => reject(new Error("Login was cancelled before the OAuth callback completed")));
        });

        void LocalhostAuth.waitForCode({ timeoutMs })
          .then(({ code }) => {
            settle(() => resolve(code));
          })
          .catch((error) => {
            const message = error instanceof Error ? error.message : String(error);
            settle(() => reject(new Error(message)));
          });

        try {
          await Browser.open({ url: authUrl, presentationStyle: "fullscreen" });
        } catch (error) {
          settle(() => reject(error instanceof Error ? error : new Error(String(error))));
        }
      });
    } finally {
      await cleanup();
    }
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
    const { port } = await LocalhostAuth.startServer();
    const authUrl = buildAuthUrl(this.selectedProvider, challenge, deviceId, port);
    const code = await this.waitForAuthorizationCode(authUrl, port);
    const initialTokens = await exchangeAuthorizationCode(code, verifier, port);
    const user = await fetchUserInfo(initialTokens);
    let tokens = initialTokens;
    try { tokens = await this.ensureClientToken(tokens, user.userId); } catch {}
    this.session = { provider: this.selectedProvider, tokens, user };
    await this.persist();
    return this.session;
  }

  async logout(): Promise<void> { this.session = null; await this.persist(); }

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
      return result.session.tokens.idToken ?? result.session.tokens.accessToken;
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
async function getVpcId(token: string, providerStreamingBaseUrl?: string): Promise<string> { try { return (await getVpcInfo(token, providerStreamingBaseUrl ?? DEFAULT_PROVIDER_STREAMING_URL)).vpcId ?? "GFN-PC"; } catch { return "GFN-PC"; } }
async function fetchPanels(token: string, panelNames: string[], vpcId: string): Promise<GraphQlResponse> { const params = new URLSearchParams({ requestType: panelNames.includes("LIBRARY") ? "panels/Library" : "panels/MainV2", extensions: JSON.stringify({ persistedQuery: { sha256Hash: PANELS_QUERY_HASH } }), huId: randomHuId(), variables: JSON.stringify({ vpcId, locale: DEFAULT_LOCALE, panelNames }) }); return httpRequest<GraphQlResponse>(`${GFN_GRAPHQL_URL}?${params.toString()}`, { headers: { Accept: "application/json, text/plain, */*", "Content-Type": "application/graphql", Origin: "https://play.geforcenow.com", Referer: "https://play.geforcenow.com/", Authorization: `GFNJWT ${token}`, "nv-client-id": LCARS_CLIENT_ID, "nv-client-type": "NATIVE", "nv-client-version": GFN_CLIENT_VERSION, "nv-client-streamer": "NVIDIA-CLASSIC", "nv-device-os": "ANDROID", "nv-device-type": "PHONE", "nv-browser-type": "CHROME", "User-Agent": GFN_USER_AGENT } }); }
function flattenPanels(payload: GraphQlResponse): GameInfo[] { if (payload.errors?.length) throw new Error(payload.errors.map((error) => error.message).join(", ")); const games: GameInfo[] = []; for (const panel of payload.data?.panels ?? []) { for (const section of panel.sections ?? []) { for (const item of section.items ?? []) { if (item.__typename === "GameItem" && item.app) games.push(appToGame(item.app)); } } } return games; }
async function fetchAppMetaData(token: string, appIds: string[], vpcId: string): Promise<GraphQlResponse> { const params = new URLSearchParams({ requestType: "appMetaData", extensions: JSON.stringify({ persistedQuery: { sha256Hash: APP_METADATA_QUERY_HASH } }), huId: randomHuId(), variables: JSON.stringify({ vpcId, locale: DEFAULT_LOCALE, appIds: [...new Set(appIds)] }) }); return httpRequest<GraphQlResponse>(`${GFN_GRAPHQL_URL}?${params.toString()}`, { headers: { Accept: "application/json, text/plain, */*", "Content-Type": "application/graphql", Origin: "https://play.geforcenow.com", Referer: "https://play.geforcenow.com/", Authorization: `GFNJWT ${token}`, "nv-client-id": LCARS_CLIENT_ID, "nv-client-type": "NATIVE", "nv-client-version": GFN_CLIENT_VERSION, "nv-client-streamer": "NVIDIA-CLASSIC", "nv-device-os": "ANDROID", "nv-device-type": "PHONE", "nv-browser-type": "CHROME", "User-Agent": GFN_USER_AGENT } }); }
async function enrichGamesWithMetadata(token: string, vpcId: string, games: GameInfo[]): Promise<GameInfo[]> { const uuids = [...new Set(games.map((game) => game.uuid).filter((uuid): uuid is string => Boolean(uuid)))]; if (uuids.length === 0) return games; const appById = new Map<string, AppData>(); for (let index = 0; index < uuids.length; index += 40) { const payload = await fetchAppMetaData(token, uuids.slice(index, index + 40), vpcId); if (payload.errors?.length) throw new Error(payload.errors.map((error) => error.message).join(", ")); for (const app of payload.data?.apps?.items ?? []) appById.set(app.id, app); } return games.map((game) => { if (!game.uuid) return game; const metadata = appById.get(game.uuid); return metadata ? mergeAppMetaIntoGame(game, metadata) : game; }); }
async function fetchCatalog(kind: "MAIN" | "LIBRARY", token: string, providerStreamingBaseUrl?: string): Promise<GameInfo[]> { const vpcId = await getVpcId(token, providerStreamingBaseUrl); const payload = await fetchPanels(token, [kind], vpcId); return enrichGamesWithMetadata(token, vpcId, flattenPanels(payload)); }
async function fetchPublicCatalog(): Promise<GameInfo[]> { const payload = await httpRequest<RawPublicGame[]>(PUBLIC_GAMES_URL, { headers: { "User-Agent": GFN_USER_AGENT } }); return payload.filter((item) => item.status === "AVAILABLE" && item.title).map((item) => { const id = String(item.id ?? item.title ?? "unknown"); const steamAppId = item.steamUrl?.split("/app/")[1]?.split("/")[0]; return { id, uuid: id, launchAppId: isNumericId(id) ? id : undefined, title: item.title ?? id, selectedVariantIndex: 0, variants: [{ id, store: "Unknown", supportedControls: [] }], imageUrl: steamAppId ? `https://cdn.cloudflare.steamstatic.com/steam/apps/${steamAppId}/library_600x900.jpg` : undefined } satisfies GameInfo; }); }
async function resolveLaunchId(token: string, appIdOrUuid: string, providerStreamingBaseUrl?: string): Promise<string | null> { if (isNumericId(appIdOrUuid)) return appIdOrUuid; const vpcId = await getVpcId(token, providerStreamingBaseUrl); const payload = await fetchAppMetaData(token, [appIdOrUuid], vpcId); if (payload.errors?.length) throw new Error(payload.errors.map((error) => error.message).join(", ")); const app = payload.data?.apps?.items?.[0]; if (!app) return null; const selected = app.variants?.find((variant) => variant.gfn?.library?.selected === true); if (isNumericId(selected?.id)) return selected.id; const firstNumeric = app.variants?.find((variant) => isNumericId(variant.id)); if (firstNumeric) return firstNumeric.id; return isNumericId(app.id) ? app.id : null; }

function requestHeaders(options: { token: string; clientId: string; deviceId: string; includeOrigin?: boolean }): Record<string, string> { const headers: Record<string, string> = { Authorization: `GFNJWT ${options.token}`, Accept: "application/json, text/plain, */*", "Content-Type": "application/json", "User-Agent": GFN_USER_AGENT, "nv-client-id": options.clientId, "nv-client-streamer": "NVIDIA-CLASSIC", "nv-client-type": "NATIVE", "nv-client-version": GFN_CLIENT_VERSION, "nv-device-os": "ANDROID", "nv-device-type": "PHONE", "x-device-id": options.deviceId }; if (options.includeOrigin !== false) { headers.Origin = "https://play.geforcenow.com"; headers.Referer = "https://play.geforcenow.com/"; } return headers; }
function cloudmatchUrl(zone: string): string { return `https://${zone}.cloudmatchbeta.nvidiagrid.net`; }
function resolveStreamingBaseUrl(zone: string, provided?: string): string { if (provided?.trim()) { const trimmed = provided.trim(); return trimmed.endsWith("/") ? trimmed.slice(0, -1) : trimmed; } return cloudmatchUrl(zone); }
function extractHostFromUrl(url: string): string | null { for (const prefix of ["rtsps://", "rtsp://", "wss://", "https://"]) { if (url.startsWith(prefix)) { const host = url.slice(prefix.length).split(":")[0]?.split("/")[0]; return host || null; } } return null; }
function streamingServerIp(response: CloudMatchResponse): string | null { const connection = response.session.connectionInfo?.find((conn) => conn.usage === 14); const directIp = Array.isArray(connection?.ip) ? connection?.ip[0] : connection?.ip; if (directIp) return directIp; if (connection?.resourcePath) { const host = extractHostFromUrl(connection.resourcePath); if (host) return host; } const controlIp = response.session.sessionControlInfo?.ip; return Array.isArray(controlIp) ? controlIp[0] ?? null : controlIp ?? null; }
function resolveSignaling(response: CloudMatchResponse): { serverIp: string; signalingServer: string; signalingUrl: string; mediaConnectionInfo?: { ip: string; port: number } } { const connection = response.session.connectionInfo?.find((conn) => conn.usage === 14 && conn.ip) ?? response.session.connectionInfo?.find((conn) => conn.ip); const serverIp = streamingServerIp(response); if (!serverIp) throw new Error("CloudMatch response did not include a signaling host"); const resourcePath = connection?.resourcePath ?? "/nvst/"; let signalingUrl = `wss://${serverIp}/nvst/`; if (resourcePath.startsWith("wss://")) signalingUrl = resourcePath; else if (resourcePath.startsWith("rtsps://") || resourcePath.startsWith("rtsp://")) signalingUrl = `wss://${extractHostFromUrl(resourcePath) ?? serverIp}/nvst/`; else if (resourcePath.startsWith("/")) signalingUrl = `wss://${serverIp}${resourcePath}`; const connectionIp = Array.isArray(connection?.ip) ? connection?.ip[0] : connection?.ip; return { serverIp, signalingServer: connectionIp ?? extractHostFromUrl(signalingUrl) ?? serverIp, signalingUrl, mediaConnectionInfo: connection?.port && connectionIp ? { ip: connectionIp, port: connection.port } : undefined }; }
function extractQueuePosition(payload: CloudMatchResponse): number | undefined { return toPositiveInt(payload.session.queuePosition) ?? toPositiveInt(payload.session.seatSetupInfo?.queuePosition) ?? toPositiveInt(payload.session.sessionProgress?.queuePosition) ?? toPositiveInt(payload.session.progressInfo?.queuePosition); }
function toColorQuality(bitDepth?: number, chromaFormat?: number): import("@shared/gfn").ColorQuality | undefined { if (bitDepth !== 0 && bitDepth !== 10) return undefined; if (chromaFormat !== 0 && chromaFormat !== 2) return undefined; if (bitDepth === 10) return chromaFormat === 2 ? "10bit_444" : "10bit_420"; return chromaFormat === 2 ? "8bit_444" : "8bit_420"; }
function normalizeIceServers(response: CloudMatchResponse): IceServer[] { const raw = response.session.iceServerConfiguration?.iceServers ?? []; const servers = raw.map((entry) => ({ urls: Array.isArray(entry.urls) ? entry.urls : [entry.urls], username: entry.username, credential: entry.credential })).filter((entry) => entry.urls.length > 0); if (servers.length > 0) return servers; return [{ urls: ["stun:s1.stun.gamestream.nvidia.com:19308"] }, { urls: ["stun:stun.l.google.com:19302"] }]; }
function extractAdState(payload: CloudMatchResponse): SessionInfo["adState"] {
  const sessionAdsRequired =
    toBoolean(payload.session.sessionAdsRequired) ??
    toBoolean(payload.session.isAdsRequired) ??
    toBoolean(payload.session.sessionProgress?.isAdsRequired) ??
    toBoolean(payload.session.progressInfo?.isAdsRequired);
  if (!sessionAdsRequired) return undefined;
  return {
    isAdsRequired: true,
    sessionAdsRequired: true,
    sessionAds: [],
    ads: [],
    serverSentEmptyAds: true,
  };
}
function extractNegotiatedStreamProfile(payload: CloudMatchResponse): SessionInfo["negotiatedStreamProfile"] { const monitor = payload.session.sessionRequestData?.clientRequestMonitorSettings?.[0]; const finalized = payload.session.finalizedStreamingFeatures; const requested = payload.session.sessionRequestData?.requestedStreamingFeatures; const resolution = monitor?.widthInPixels && monitor?.heightInPixels ? `${Math.trunc(monitor.widthInPixels)}x${Math.trunc(monitor.heightInPixels)}` : undefined; const colorQuality = toColorQuality(finalized?.bitDepth ?? requested?.bitDepth, finalized?.chromaFormat ?? requested?.chromaFormat); const enableL4S = finalized?.enabledL4S ?? requested?.enabledL4S; if (!resolution && !monitor?.framesPerSecond && !colorQuality && enableL4S === undefined) return undefined; return { resolution, fps: monitor?.framesPerSecond, colorQuality, enableL4S: enableL4S === undefined ? undefined : Boolean(enableL4S) }; }
async function toSessionInfo(zone: string, streamingBaseUrl: string, payload: CloudMatchResponse, clientId?: string, deviceId?: string): Promise<SessionInfo> { if (payload.requestStatus.statusCode !== 1) throw new Error(payload.requestStatus.statusDescription ?? payload.requestStatus.statusName ?? "Session request failed"); const signaling = resolveSignaling(payload); return { sessionId: payload.session.sessionId, status: payload.session.status, queuePosition: extractQueuePosition(payload), seatSetupStep: payload.session.seatSetupInfo?.seatSetupStep, adState: extractAdState(payload), zone, streamingBaseUrl, serverIp: signaling.serverIp, signalingServer: signaling.signalingServer, signalingUrl: signaling.signalingUrl, gpuType: payload.session.gpuType, iceServers: normalizeIceServers(payload), mediaConnectionInfo: signaling.mediaConnectionInfo, negotiatedStreamProfile: extractNegotiatedStreamProfile(payload), clientId, deviceId }; }
async function buildDeviceIdentifiers(): Promise<{ clientId: string; deviceId: string }> { const { identifier } = await Device.getId(); return { clientId: crypto.randomUUID(), deviceId: identifier || crypto.randomUUID() }; }

async function createSessionRequest(input: SessionCreateRequest): Promise<SessionInfo> { const token = await authStore.resolveJwtToken(input.token); const { clientId, deviceId } = await buildDeviceIdentifiers(); const streamingBaseUrl = resolveStreamingBaseUrl(input.zone, input.streamingBaseUrl); const [width, height] = input.settings.resolution.split("x").map((value) => Number.parseInt(value, 10)); const response = await httpRequest<CloudMatchResponse>(`${streamingBaseUrl}/v2/session`, { method: "POST", headers: requestHeaders({ token, clientId, deviceId }), data: { appId: input.appId, sessionRequestData: { clientRequestMonitorSettings: [{ widthInPixels: width, heightInPixels: height, framesPerSecond: input.settings.fps, requestedDisplayRefreshRate: input.settings.fps }], requestedStreamingFeatures: { reflex: input.settings.fps >= 120, bitDepth: colorQualityBitDepth(input.settings.colorQuality), enabledL4S: input.settings.enableL4S, chromaFormat: colorQualityChromaFormat(input.settings.colorQuality) }, appLaunchMode: 1, secureRTSPSupported: false, accountLinked: input.accountLinked ?? true, enablePersistingInGameSettings: true, userAge: 26 } } }); return toSessionInfo(input.zone, streamingBaseUrl, response, clientId, deviceId); }
async function pollSessionRequest(input: SessionPollRequest): Promise<SessionInfo> { const token = await authStore.resolveJwtToken(input.token); const clientId = input.clientId ?? crypto.randomUUID(); const deviceId = input.deviceId ?? (await Device.getId()).identifier ?? crypto.randomUUID(); const base = input.serverIp ? `https://${input.serverIp}` : resolveStreamingBaseUrl(input.zone, input.streamingBaseUrl); const response = await httpRequest<CloudMatchResponse>(`${base}/v2/session/${input.sessionId}`, { headers: requestHeaders({ token, clientId, deviceId, includeOrigin: false }) }); return toSessionInfo(input.zone, base, response, clientId, deviceId); }
async function stopSessionRequest(input: SessionStopRequest): Promise<void> { const token = await authStore.resolveJwtToken(input.token); const clientId = input.clientId ?? crypto.randomUUID(); const deviceId = input.deviceId ?? (await Device.getId()).identifier ?? crypto.randomUUID(); const base = input.serverIp ? `https://${input.serverIp}` : resolveStreamingBaseUrl(input.zone, input.streamingBaseUrl); await httpRequest<string>(`${base}/v2/session/${input.sessionId}`, { method: "DELETE", headers: requestHeaders({ token, clientId, deviceId }), responseType: "text" }); }
async function reportSessionAdRequest(input: SessionAdReportRequest): Promise<SessionInfo> { const token = await authStore.resolveJwtToken(input.token); const clientId = input.clientId ?? crypto.randomUUID(); const deviceId = input.deviceId ?? (await Device.getId()).identifier ?? crypto.randomUUID(); const base = input.serverIp ? `https://${input.serverIp}` : resolveStreamingBaseUrl(input.zone, input.streamingBaseUrl); const response = await httpRequest<CloudMatchResponse>(`${base}/v2/session/${input.sessionId}/ad`, { method: "PUT", headers: requestHeaders({ token, clientId, deviceId }), data: { adId: input.adId, action: ({ start: 1, pause: 2, resume: 3, finish: 4, cancel: 5 } as const)[input.action], clientTimestamp: input.clientTimestamp, watchedTimeInMs: input.watchedTimeInMs, pausedTimeInMs: input.pausedTimeInMs, cancelReason: input.cancelReason, errorInfo: input.errorInfo } }); return toSessionInfo(input.zone, base, response, clientId, deviceId); }
async function getActiveSessionsRequest(token: string, streamingBaseUrl?: string): Promise<ActiveSessionInfo[]> { const base = resolveStreamingBaseUrl("", streamingBaseUrl || authStore.getSelectedProvider().streamingServiceUrl); const response = await httpRequest<CloudMatchResponse>(`${base}/v2/sessions`, { headers: { Authorization: `GFNJWT ${token}`, Accept: "application/json, text/plain, */*", "User-Agent": GFN_USER_AGENT, "nv-client-id": LCARS_CLIENT_ID, "nv-client-streamer": "NVIDIA-CLASSIC", "nv-client-type": "NATIVE", "nv-client-version": GFN_CLIENT_VERSION, "nv-device-os": "ANDROID", "nv-device-type": "PHONE" } }); return (response.sessions ?? []).filter((session) => session.status === 2 || session.status === 3).map((session) => ({ sessionId: session.sessionId, appId: session.appId, gpuType: session.gpuType, status: session.status, serverIp: Array.isArray(session.sessionControlInfo?.ip) ? session.sessionControlInfo?.ip[0] : session.sessionControlInfo?.ip, signalingUrl: session.connectionInfo?.find((entry) => entry.usage === 14)?.resourcePath, resolution: session.resolution, fps: session.fps })); }
async function claimSessionRequest(input: SessionClaimRequest): Promise<SessionInfo> { const token = await authStore.resolveJwtToken(input.token); const clientId = crypto.randomUUID(); const deviceId = (await Device.getId()).identifier || crypto.randomUUID(); const base = `https://${input.serverIp}`; const claimUrl = `${base}/v2/session/${input.sessionId}?${new URLSearchParams({ keyboardLayout: resolveGfnKeyboardLayout(input.settings?.keyboardLayout ?? DEFAULT_KEYBOARD_LAYOUT, "linux"), languageCode: input.settings?.gameLanguage ?? "en_US" }).toString()}`; await httpRequest<unknown>(claimUrl, { method: "PUT", headers: requestHeaders({ token, clientId, deviceId }), data: { sessionRequestData: { appId: input.appId ?? "0", requestedStreamingFeatures: { reflex: (input.settings?.fps ?? 60) >= 120, bitDepth: colorQualityBitDepth(input.settings?.colorQuality ?? "8bit_420"), enabledL4S: input.settings?.enableL4S ?? false, chromaFormat: colorQualityChromaFormat(input.settings?.colorQuality ?? "8bit_420") } } } }).catch(() => undefined); for (let attempt = 0; attempt < 60; attempt += 1) { if (attempt > 0) await new Promise((resolve) => window.setTimeout(resolve, 1000)); try { const polled = await httpRequest<CloudMatchResponse>(`${base}/v2/session/${input.sessionId}`, { headers: requestHeaders({ token, clientId, deviceId, includeOrigin: false }) }); if (polled.session.status === 2 || polled.session.status === 3) return toSessionInfo("", base, polled, clientId, deviceId); } catch {} } throw new Error("Session did not become ready after claiming"); }
async function fetchSubscriptionInfo(input: SubscriptionFetchRequest): Promise<SubscriptionInfo> { const token = await authStore.resolveJwtToken(input.token); const vpcId = await getVpcId(token, input.providerStreamingBaseUrl); const url = new URL(MES_URL); url.searchParams.append("serviceName", "gfn_pc"); url.searchParams.append("languageCode", "en_US"); url.searchParams.append("vpcId", vpcId); url.searchParams.append("userId", input.userId); const data = await httpRequest<SubscriptionResponse>(url.toString(), { headers: { Authorization: `GFNJWT ${token}`, Accept: "application/json", "nv-client-id": LCARS_CLIENT_ID, "nv-client-type": "NATIVE", "nv-client-version": GFN_CLIENT_VERSION, "nv-client-streamer": "NVIDIA-CLASSIC", "nv-device-os": "ANDROID", "nv-device-type": "PHONE" } }); const allottedMinutes = data.allottedTimeInMinutes ?? 0; const purchasedMinutes = data.purchasedTimeInMinutes ?? 0; const rolledOverMinutes = data.rolledOverTimeInMinutes ?? 0; const totalMinutes = data.totalTimeInMinutes ?? allottedMinutes + purchasedMinutes + rolledOverMinutes; const remainingMinutes = data.remainingTimeInMinutes ?? 0; const usedMinutes = Math.max(totalMinutes - remainingMinutes, 0); const storageAddon = data.addons?.find((addon) => addon.type === "STORAGE" && addon.subType === "PERMANENT_STORAGE" && addon.status === "OK"); const attr = (key: string) => storageAddon?.attributes?.find((entry) => entry.key === key)?.textValue; return { membershipTier: data.membershipTier ?? "FREE", subscriptionType: data.type, subscriptionSubType: data.subType, allottedHours: allottedMinutes / 60, purchasedHours: purchasedMinutes / 60, rolledOverHours: rolledOverMinutes / 60, usedHours: usedMinutes / 60, remainingHours: remainingMinutes / 60, totalHours: totalMinutes / 60, firstEntitlementStartDateTime: data.firstEntitlementStartDateTime, serverRegionId: vpcId, currentSpanStartDateTime: data.currentSpanStartDateTime, currentSpanEndDateTime: data.currentSpanEndDateTime, notifyUserWhenTimeRemainingInMinutes: data.notifications?.notifyUserWhenTimeRemainingInMinutes, notifyUserOnSessionWhenRemainingTimeInMinutes: data.notifications?.notifyUserOnSessionWhenRemainingTimeInMinutes, state: data.currentSubscriptionState?.state, isGamePlayAllowed: data.currentSubscriptionState?.isGamePlayAllowed, isUnlimited: data.subType === "UNLIMITED", storageAddon: storageAddon ? { type: "PERMANENT_STORAGE", sizeGb: attr("TOTAL_STORAGE_SIZE_IN_GB") ? Number(attr("TOTAL_STORAGE_SIZE_IN_GB")) : undefined, usedGb: attr("USED_STORAGE_SIZE_IN_GB") ? Number(attr("USED_STORAGE_SIZE_IN_GB")) : undefined, regionName: attr("STORAGE_METRO_REGION_NAME"), regionCode: attr("STORAGE_METRO_REGION") } : undefined, entitledResolutions: (data.features?.resolutions ?? []).map((res) => ({ width: res.widthInPixels, height: res.heightInPixels, fps: res.framesPerSecond })) }; }
function unsupported(message: string): Promise<never> { return Promise.reject(new Error(message)); }
function dataUrlExtension(dataUrl: string): string { if (dataUrl.startsWith("data:image/jpeg")) return "jpg"; if (dataUrl.startsWith("data:image/webp")) return "webp"; return "png"; }
function decodeDataUrl(dataUrl: string): string { const match = /^data:[^;]+;base64,(.+)$/i.exec(dataUrl); if (!match || !match[1]) throw new Error("Invalid data URL"); return match[1]; }
function encodeBase64(bytes: Uint8Array): string { let binary = ""; const chunkSize = 0x8000; for (let index = 0; index < bytes.length; index += chunkSize) { binary += String.fromCharCode(...bytes.subarray(index, index + chunkSize)); } return btoa(binary); }
function concatBytes(chunks: Uint8Array[]): Uint8Array { const totalLength = chunks.reduce((sum, chunk) => sum + chunk.byteLength, 0); const merged = new Uint8Array(totalLength); let offset = 0; for (const chunk of chunks) { merged.set(chunk, offset); offset += chunk.byteLength; } return merged; }
async function ensureDirectory(path: string): Promise<void> { await Filesystem.mkdir({ path, directory: Directory.Data, recursive: true }).catch(() => undefined); }
async function listDirectory(path: string): Promise<Array<{ name: string }>> { try { const result = await Filesystem.readdir({ path, directory: Directory.Data }); return result.files.map((file) => ({ name: file.name })); } catch { return []; } }
type RecordingMeta = RecordingEntry;
const RECORDING_META_KEY = RECORDINGS_KEY;
const THANKS_CACHE_KEY = "opennow.android.thanks.v1";
async function readRecordingMeta(): Promise<RecordingMeta[]> { return readPreferenceJson<RecordingMeta[]>(RECORDING_META_KEY, []); }
async function writeRecordingMeta(entries: RecordingMeta[]): Promise<void> { await writePreferenceJson(RECORDING_META_KEY, entries); }
async function readDataUrl(path: string, mimeType: string): Promise<string> { const file = await Filesystem.readFile({ path, directory: Directory.Data }); return `data:${mimeType};base64,${typeof file.data === "string" ? file.data : ""}`; }

const signalingListeners = new Set<(event: MainToRendererSignalingEvent) => void>();
let signalingClient: BrowserSignalingClient | null = null;
const recordingBuffers = new Map<string, Uint8Array[]>();
const recordingMime = new Map<string, string>();

const api: OpenNowApi = {
  getAuthSession: async (input: AuthSessionRequest = {}) => { await ensureInitialized(); return authStore.ensureValidSessionWithStatus(Boolean(input.forceRefresh)); },
  getLoginProviders: async () => { await ensureInitialized(); return authStore.getProviders(); },
  getRegions: async (input: RegionsFetchRequest = {}) => { await ensureInitialized(); try { const token = input.token || authStore.getSession()?.tokens.idToken || authStore.getSession()?.tokens.accessToken; return (await getVpcInfo(token, authStore.getSelectedProvider().streamingServiceUrl)).regions; } catch { return []; } },
  login: async (input: AuthLoginRequest) => { await ensureInitialized(); return authStore.login(input); },
  logout: async () => { await ensureInitialized(); await authStore.logout(); },
  fetchSubscription: async (input) => fetchSubscriptionInfo(input),
  fetchMainGames: async (input) => fetchCatalog("MAIN", await authStore.resolveJwtToken(input.token), input.providerStreamingBaseUrl),
  fetchLibraryGames: async (input) => fetchCatalog("LIBRARY", await authStore.resolveJwtToken(input.token), input.providerStreamingBaseUrl),
  fetchPublicGames: async () => fetchPublicCatalog(),
  resolveLaunchAppId: async (input) => resolveLaunchId(await authStore.resolveJwtToken(input.token), input.appIdOrUuid, input.providerStreamingBaseUrl),
  createSession: async (input) => createSessionRequest(input),
  pollSession: async (input) => pollSessionRequest(input),
  reportSessionAd: async (input) => reportSessionAdRequest(input),
  stopSession: async (input) => stopSessionRequest(input),
  getActiveSessions: async (token, streamingBaseUrl) => getActiveSessionsRequest(await authStore.resolveJwtToken(token), streamingBaseUrl),
  claimSession: async (input) => claimSessionRequest(input),
  showSessionConflictDialog: async (): Promise<SessionConflictChoice> => window.confirm("An active GeForce NOW session was found. Tap OK to resume it or Cancel to start a new session.") ? "resume" : "new",
  connectSignaling: async (input: SignalingConnectRequest) => { signalingClient?.disconnect(); signalingClient = new BrowserSignalingClient(); signalingClient.onEvent((event) => { for (const listener of signalingListeners) listener(event); }); await signalingClient.connect(input); },
  disconnectSignaling: async () => { signalingClient?.disconnect(); signalingClient = null; },
  sendAnswer: async (input: SendAnswerRequest) => { await signalingClient?.sendAnswer(input); },
  sendIceCandidate: async (input: IceCandidatePayload) => { await signalingClient?.sendIceCandidate(input); },
  requestKeyframe: async (input: KeyframeRequest) => { await signalingClient?.requestKeyframe(input); },
  onSignalingEvent: (listener) => { signalingListeners.add(listener); return () => signalingListeners.delete(listener); },
  onToggleFullscreen: () => () => undefined,
  quitApp: async () => unsupported("Quit app is not supported on Android."),
  toggleFullscreen: async () => { const next = document.body.dataset.androidFullscreen !== "true"; document.body.dataset.androidFullscreen = next ? "true" : "false"; if (next) { await StatusBar.hide().catch(() => undefined); } else { await StatusBar.show().catch(() => undefined); await StatusBar.setStyle({ style: Style.Dark }).catch(() => undefined); } },
  setFullscreen: async (value: boolean) => { document.body.dataset.androidFullscreen = value ? "true" : "false"; if (value) { await StatusBar.hide().catch(() => undefined); } else { await StatusBar.show().catch(() => undefined); await StatusBar.setStyle({ style: Style.Dark }).catch(() => undefined); } },
  togglePointerLock: async () => unsupported("Pointer lock is not supported on Android."),
  getSettings: async () => getStoredSettings(),
  setSetting: async (key, value) => { const current = await getStoredSettings(); await saveSettings({ ...current, [key]: value }); },
  resetSettings: async () => { await saveSettings(DEFAULT_SETTINGS); return { ...DEFAULT_SETTINGS }; },
  getMicrophonePermission: async (): Promise<MicrophonePermissionResult> => ({ platform: "android", isMacOs: false, status: "not-applicable", granted: true, canRequest: true, shouldUseBrowserApi: true }),
  exportLogs: async () => unsupported("Log export is not supported on Android in this pass."),
  pingRegions: async (regions: StreamRegion[]): Promise<PingResult[]> => Promise.all(regions.map(async (region) => { const startedAt = performance.now(); try { await httpRequest<string>(region.url, { responseType: "text" }); return { url: region.url, pingMs: Math.round(performance.now() - startedAt) }; } catch (error) { return { url: region.url, pingMs: null, error: error instanceof Error ? error.message : String(error) }; } })),
  saveScreenshot: async (input: ScreenshotSaveRequest): Promise<ScreenshotEntry> => { await ensureDirectory(SCREENSHOT_DIR); const fileName = `${Date.now()}-${Math.random().toString(16).slice(2)}.${dataUrlExtension(input.dataUrl)}`; const filePath = `${SCREENSHOT_DIR}/${fileName}`; await Filesystem.writeFile({ path: filePath, directory: Directory.Data, data: decodeDataUrl(input.dataUrl), recursive: true }); const stat = await Filesystem.stat({ path: filePath, directory: Directory.Data }); return { id: fileName, fileName, filePath, createdAtMs: Number(stat.ctime ?? Date.now()), sizeBytes: stat.size, dataUrl: input.dataUrl }; },
  listScreenshots: async (): Promise<ScreenshotEntry[]> => { const files = await listDirectory(SCREENSHOT_DIR); const entries = await Promise.all(files.map(async (file) => { const filePath = `${SCREENSHOT_DIR}/${file.name}`; const stat = await Filesystem.stat({ path: filePath, directory: Directory.Data }); const mime = file.name.endsWith(".jpg") ? "image/jpeg" : file.name.endsWith(".webp") ? "image/webp" : "image/png"; return { id: file.name, fileName: file.name, filePath, createdAtMs: Number(stat.ctime ?? Date.now()), sizeBytes: stat.size, dataUrl: await readDataUrl(filePath, mime) } satisfies ScreenshotEntry; })); return entries.sort((a, b) => b.createdAtMs - a.createdAtMs); },
  deleteScreenshot: async (input: ScreenshotDeleteRequest) => { await Filesystem.deleteFile({ path: `${SCREENSHOT_DIR}/${input.id}`, directory: Directory.Data }).catch(() => undefined); },
  saveScreenshotAs: async (_input: ScreenshotSaveAsRequest): Promise<ScreenshotSaveAsResult> => unsupported("Screenshot export is not supported on Android.") as Promise<ScreenshotSaveAsResult>,
  onTriggerScreenshot: () => () => undefined,
  beginRecording: async (input: RecordingBeginRequest): Promise<RecordingBeginResult> => { const recordingId = crypto.randomUUID(); recordingBuffers.set(recordingId, []); recordingMime.set(recordingId, input.mimeType); return { recordingId }; },
  sendRecordingChunk: async (input: RecordingChunkRequest) => { const list = recordingBuffers.get(input.recordingId); if (!list) return; list.push(new Uint8Array(input.chunk.slice(0))); },
  finishRecording: async (input: RecordingFinishRequest): Promise<RecordingEntry> => { await ensureDirectory(RECORDING_DIR); const ext = (recordingMime.get(input.recordingId) ?? "video/webm").includes("mp4") ? "mp4" : "webm"; const fileName = `${Date.now()}-${input.recordingId}.${ext}`; const filePath = `${RECORDING_DIR}/${fileName}`; const mergedBytes = concatBytes(recordingBuffers.get(input.recordingId) ?? []); await Filesystem.writeFile({ path: filePath, directory: Directory.Data, data: encodeBase64(mergedBytes), recursive: true }); const stat = await Filesystem.stat({ path: filePath, directory: Directory.Data }); const entry: RecordingMeta = { id: input.recordingId, fileName, filePath, createdAtMs: Number(stat.ctime ?? Date.now()), sizeBytes: stat.size, durationMs: input.durationMs, gameTitle: input.gameTitle, thumbnailDataUrl: input.thumbnailDataUrl }; const entries = await readRecordingMeta(); await writeRecordingMeta([entry, ...entries.filter((item) => item.id !== entry.id)]); recordingBuffers.delete(input.recordingId); recordingMime.delete(input.recordingId); return entry; },
  abortRecording: async (input: RecordingAbortRequest) => { recordingBuffers.delete(input.recordingId); recordingMime.delete(input.recordingId); },
  listRecordings: async (): Promise<RecordingEntry[]> => { const entries = await readRecordingMeta(); return entries.sort((a, b) => b.createdAtMs - a.createdAtMs); },
  deleteRecording: async (input: RecordingDeleteRequest) => { const entries = await readRecordingMeta(); const match = entries.find((entry) => entry.id === input.id); if (match) await Filesystem.deleteFile({ path: match.filePath, directory: Directory.Data }).catch(() => undefined); await writeRecordingMeta(entries.filter((entry) => entry.id !== input.id)); },
  showRecordingInFolder: async () => unsupported("Folder access is not supported on Android."),
  listMediaByGame: async (input = {}): Promise<MediaListingResult> => { const screenshots = await api.listScreenshots(); const recordings = await api.listRecordings(); const title = input.gameTitle?.trim().toLowerCase(); return { screenshots: title ? [] : screenshots.map((entry) => ({ ...entry })), videos: recordings.filter((entry) => !title || entry.gameTitle?.trim().toLowerCase() === title).map((entry) => ({ ...entry })) }; },
  getMediaThumbnail: async (input: { filePath: string }) => { if (input.filePath.startsWith(SCREENSHOT_DIR)) return readDataUrl(input.filePath, "image/png"); const recordings = await readRecordingMeta(); return recordings.find((entry) => entry.filePath === input.filePath)?.thumbnailDataUrl ?? null; },
  showMediaInFolder: async () => unsupported("Folder access is not supported on Android."),
  deleteCache: async () => unsupported("Cache deletion is not supported on Android in this pass."),
  getThanksData: async (): Promise<ThankYouDataResult> => { const cached = await readPreferenceJson<ThankYouDataResult | null>(THANKS_CACHE_KEY, null); if (cached) return cached; const placeholder: ThankYouDataResult = { contributors: [], supporters: [], contributorsError: "Community data is unavailable on Android in this pass." }; await writePreferenceJson(THANKS_CACHE_KEY, placeholder); return placeholder; },
};

void CapacitorApp.addListener("backButton", () => { if (document.fullscreenElement) void document.exitFullscreen().catch(() => undefined); });

export const capacitorPlatform: OpenNowPlatform = { info: { kind: "capacitor-web", capabilities: { isAndroid: true, isElectron: false, supportsQuitApp: false, supportsPointerLockToggle: false, supportsDesktopFullscreen: false, supportsLogExport: false, supportsCacheDeletion: false, supportsMediaFolderAccess: false, supportsScreenshotExport: false, supportsPersistentMedia: true, supportsKeyboardShortcuts: false, supportsControllerExitApp: false } }, api };
