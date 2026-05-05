import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { CSSProperties, JSX } from "react";
import { createPortal } from "react-dom";

import type {
  ActiveSessionInfo,
  AuthSession,
  AuthUser,
  CatalogBrowseResult,
  CatalogFilterGroup,
  CatalogSortOption,
  ExistingSessionStrategy,
  GameInfo,
  GameVariant,
  LoginProvider,
  MainToRendererSignalingEvent,
  SessionAdAction,
  SessionAdInfo,
  SessionAdState,
  SessionInfo,
  SessionStopRequest,
  SavedAccount,
  Settings,
  SubscriptionInfo,
  SignalingConnectRequest,
  StreamSettings,
  StreamRegion,
  VideoCodec,
  NativeStreamStats,
  PrintedWasteQueueData,
  PrintedWasteServerMapping,
} from "@shared/gfn";
import {
  buildNativeStreamerSessionContext,
  DEFAULT_KEYBOARD_LAYOUT,
  getDefaultStreamPreferences,
  USER_FACING_VIDEO_CODEC_OPTIONS,
  getPreferredSessionAdMediaUrl,
  getSessionAdDurationMs,
  getSessionAdItems,
  getSessionAdOpportunity,
  isSessionAdsRequired,
  isSessionQueuePaused,
} from "@shared/gfn";

import {
  GfnWebRtcClient,
  type StreamDiagnostics,
  type StreamTimeWarning,
} from "./gfn/webrtcClient";
import { formatShortcutForDisplay, isShortcutMatch, normalizeShortcut } from "./shortcuts";
import { useControllerNavigation } from "./controllerNavigation";
import { useElapsedSeconds } from "./utils/useElapsedSeconds";
import { usePlaytime } from "./utils/usePlaytime";
import { createStreamDiagnosticsStore, useStreamDiagnosticsSelector } from "./utils/streamDiagnosticsStore";
import { playControllerUiSound } from "./utils/controllerUiSound";
import { loadStoredCodecResults, saveStoredCodecResults, testCodecSupport, type CodecTestResult } from "./lib/codecDiagnostics";
import { chooseAccountLinked, getEpicOwnershipLaunchError } from "./lib/launchOwnership";

// UI Components
import { LoginScreen } from "./components/LoginScreen";
import { Navbar } from "./components/Navbar";
import { HomePage } from "./components/HomePage";
import { LibraryPage } from "./components/LibraryPage";
import { ControllerLibraryPage } from "./components/controllerMode/ControllerLibraryPage";
import { ControllerInStreamShell } from "./components/controllerMode/controllerInStream/ControllerInStreamShell";
import { SettingsPage } from "./components/SettingsPage";
import { StreamLoading } from "./components/StreamLoading";
import { ControllerStreamLoading } from "./components/controllerMode/ControllerStreamLoading";
import type { QueueAdPlaybackEvent, QueueAdPreviewHandle } from "./components/QueueAdPreview";
import { StreamView } from "./components/StreamView";
import { QueueServerSelectModal } from "./components/QueueServerSelectModal";

const codecOptions: VideoCodec[] = [...USER_FACING_VIDEO_CODEC_OPTIONS];
const DEFAULT_STREAM_PREFERENCES = getDefaultStreamPreferences();
const allResolutionOptions = ["1280x720", "1280x800", "1440x900", "1680x1050", "1920x1080", "1920x1200", "2560x1080", "2560x1440", "2560x1600", "3440x1440", "3840x2160", "3840x2400"];
const fpsOptions = [30, 60, 120, 144, 165, 240];
const aspectRatioOptions = ["16:9", "16:10", "21:9", "32:9"] as const;

const RESOLUTION_TO_ASPECT_RATIO: Record<string, string> = {
  "1280x720": "16:9",
  "1280x800": "16:10",
  "1440x900": "16:10",
  "1680x1050": "16:10",
  "1920x1080": "16:9",
  "1920x1200": "16:10",
  "2560x1080": "21:9",
  "2560x1440": "16:9",
  "2560x1600": "16:10",
  "3440x1440": "21:9",
  "3840x2160": "16:9",
  "3840x2400": "16:10",
  "5120x1440": "32:9",
};

const getResolutionsByAspectRatio = (aspectRatio: string): string[] => {
  return allResolutionOptions.filter(res => RESOLUTION_TO_ASPECT_RATIO[res] === aspectRatio);
};
const resolutionOptions = getResolutionsByAspectRatio("16:9");

type AppStyle = CSSProperties & {
  "--game-poster-scale"?: string;
};

function getAppStyle(posterSizeScale: number): AppStyle {
  return {
    "--game-poster-scale": String(posterSizeScale),
  };
}
const SESSION_READY_POLL_INTERVAL_MS = 2000;
const SESSION_AD_POLL_INTERVAL_MS = 30000;
const SESSION_AD_PROGRESS_CHECK_INTERVAL_MS = 1000;
const SESSION_AD_START_TIMEOUT_MS = 30000;
const SESSION_AD_FORCE_PLAY_TIMEOUT_MS = 10000;
const SESSION_AD_STUCK_TIMEOUT_MS = 30000;
const VARIANT_SELECTION_LOCALSTORAGE_KEY = "opennow.variantByGameId";
const CATALOG_PREFERENCES_LOCALSTORAGE_KEY = "opennow.catalogPreferences.v1";
const RUNTIME_SNAPSHOT_LOCALSTORAGE_KEY = "opennow.runtimeSnapshot.v1";
const PLAYTIME_RESYNC_INTERVAL_MS = 5 * 60 * 1000;
const FREE_TIER_SESSION_LIMIT_SECONDS = 60 * 60;
const FREE_TIER_30_MIN_WARNING_SECONDS = 30 * 60;
const FREE_TIER_15_MIN_WARNING_SECONDS = 15 * 60;
const FREE_TIER_FINAL_MINUTE_WARNING_SECONDS = 60;
const STREAM_WARNING_VISIBILITY_MS = 15 * 1000;

interface CatalogPreferences {
  sortId: string;
  filterIds: string[];
}

interface RuntimeSnapshot {
  version: 1;
  updatedAt: number;
  streamStatus: StreamStatus;
  sessionId: string | null;
  sessionAppId: number | null;
  streamingGameId: string | null;
  streamingStore: string | null;
  recoveryAppId: number | null;
  resumeContext: {
    sessionId: string;
    serverIp: string;
    streamingBaseUrl?: string;
    signalingServer?: string;
    signalingUrl?: string;
    appId?: number;
    clientId?: string;
    deviceId?: string;
  } | null;
}

function loadCatalogPreferences(): CatalogPreferences {
  try {
    const raw = localStorage.getItem(CATALOG_PREFERENCES_LOCALSTORAGE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<CatalogPreferences>;
      return {
        sortId: typeof parsed.sortId === "string" ? parsed.sortId : "relevance",
        filterIds: Array.isArray(parsed.filterIds) ? parsed.filterIds.filter((id): id is string => typeof id === "string") : [],
      };
    }
  } catch {
    // ignore
  }
  return { sortId: "relevance", filterIds: [] };
}

function saveCatalogPreferences(prefs: CatalogPreferences): void {
  try {
    localStorage.setItem(CATALOG_PREFERENCES_LOCALSTORAGE_KEY, JSON.stringify(prefs));
  } catch {
    // ignore
  }
}

function loadRuntimeSnapshot(): RuntimeSnapshot | null {
  try {
    const raw = localStorage.getItem(RUNTIME_SNAPSHOT_LOCALSTORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<RuntimeSnapshot>;
    if (parsed.version !== 1) return null;
    return {
      version: 1,
      updatedAt: typeof parsed.updatedAt === "number" ? parsed.updatedAt : Date.now(),
      streamStatus: (typeof parsed.streamStatus === "string" ? parsed.streamStatus : "idle") as StreamStatus,
      sessionId: typeof parsed.sessionId === "string" ? parsed.sessionId : null,
      sessionAppId: typeof parsed.sessionAppId === "number" ? parsed.sessionAppId : null,
      streamingGameId: typeof parsed.streamingGameId === "string" ? parsed.streamingGameId : null,
      streamingStore: typeof parsed.streamingStore === "string" ? parsed.streamingStore : null,
      recoveryAppId: typeof parsed.recoveryAppId === "number" ? parsed.recoveryAppId : null,
      resumeContext:
        parsed.resumeContext &&
        typeof parsed.resumeContext === "object" &&
        typeof parsed.resumeContext.sessionId === "string" &&
        typeof parsed.resumeContext.serverIp === "string"
          ? {
            sessionId: parsed.resumeContext.sessionId,
            serverIp: parsed.resumeContext.serverIp,
            streamingBaseUrl:
              typeof parsed.resumeContext.streamingBaseUrl === "string"
                ? parsed.resumeContext.streamingBaseUrl
                : undefined,
            signalingServer:
              typeof parsed.resumeContext.signalingServer === "string"
                ? parsed.resumeContext.signalingServer
                : undefined,
            signalingUrl:
              typeof parsed.resumeContext.signalingUrl === "string"
                ? parsed.resumeContext.signalingUrl
                : undefined,
            appId:
              typeof parsed.resumeContext.appId === "number" && Number.isFinite(parsed.resumeContext.appId)
                ? parsed.resumeContext.appId
                : undefined,
            clientId:
              typeof parsed.resumeContext.clientId === "string"
                ? parsed.resumeContext.clientId
                : undefined,
            deviceId:
              typeof parsed.resumeContext.deviceId === "string"
                ? parsed.resumeContext.deviceId
                : undefined,
          }
          : null,
    };
  } catch {
    return null;
  }
}

function saveRuntimeSnapshot(snapshot: RuntimeSnapshot): void {
  try {
    localStorage.setItem(RUNTIME_SNAPSHOT_LOCALSTORAGE_KEY, JSON.stringify(snapshot));
  } catch {
    // ignore
  }
}

function clearRuntimeSnapshot(): void {
  try {
    localStorage.removeItem(RUNTIME_SNAPSHOT_LOCALSTORAGE_KEY);
  } catch {
    // ignore
  }
}

type AppPage = "home" | "library" | "settings";
type StreamStatus = "idle" | "queue" | "setup" | "starting" | "connecting" | "streaming";
type StreamLoadingStatus = "queue" | "setup" | "starting" | "connecting";
type ExitPromptState = { open: boolean; gameTitle: string };
type StreamWarningState = {
  code: StreamTimeWarning["code"];
  message: string;
  tone: "warn" | "critical";
  secondsLeft?: number;
};
type LocalSessionTimerWarningState = {
  stage: "free-tier-30m" | "free-tier-15m" | "free-tier-final-minute";
  shownAtMs: number;
};
type LaunchErrorState = {
  stage: StreamLoadingStatus;
  title: string;
  description: string;
  codeLabel?: string;
};
type QueueAdCancelReason = "error" | "other";
type QueueAdErrorInfo = "Ad play timeout" | "Ad video is stuck" | "Error loading url";
type QueueAdMetrics = {
  startedAtMs: number | null;
  wasPausedAtLeastOnce: boolean;
};
type QueueAdReportOptions = {
  cancelReason?: QueueAdCancelReason;
  errorInfo?: QueueAdErrorInfo;
};

type SignalingRecoveryState = {
  attemptCount: number;
  inFlight: Promise<boolean> | null;
  explicitShutdown: boolean;
  appId: number | null;
  generation: number;
};

const APP_PAGE_ORDER: AppPage[] = ["home", "library", "settings"];
const RECOVERABLE_STREAM_STATUSES: readonly StreamStatus[] = ["streaming"];
const SIGNALING_RECOVERY_ATTEMPT_DELAYS_MS = [0, 3000] as const;
const SIGNALING_RECOVERY_STABLE_RESET_DELAY_MS = 15000;
const SIGNALING_REMOTE_ICE_GRACE_MS = 5000;
const ICE_DISCONNECTED_RECOVERY_GRACE_MS = 7000;

const isMac = navigator.platform.toLowerCase().includes("mac");

function isStandardPrintedWasteZone(zoneId: string): boolean {
  return zoneId.startsWith("NP-") && !zoneId.startsWith("NPA-");
}

function isAllianceStreamingBaseUrl(streamingBaseUrl: string): boolean {
  if (!streamingBaseUrl.trim()) return false;
  try {
    const { hostname } = new URL(streamingBaseUrl);
    return !hostname.endsWith(".nvidiagrid.net");
  } catch {
    return false;
  }
}

function hasAnyEligiblePrintedWasteZone(
  queueData: PrintedWasteQueueData,
  mapping: PrintedWasteServerMapping,
): boolean {
  return Object.keys(queueData).some((zoneId) => (
    isStandardPrintedWasteZone(zoneId) && mapping[zoneId]?.nuked !== true
  ));
}

const DEFAULT_SHORTCUTS = {
  shortcutToggleStats: "F3",
  shortcutTogglePointerLock: "F8",
  shortcutToggleFullscreen: "F10",
  shortcutStopStream: "Ctrl+Shift+Q",
  shortcutToggleAntiAfk: "Ctrl+Shift+K",
  shortcutToggleMicrophone: "Ctrl+Shift+M",
  shortcutScreenshot: "F11",
  shortcutToggleRecording: "F12",
} as const;


function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

async function waitFor(
  predicate: () => boolean,
  { timeout = 1000, interval = 50 }: { timeout?: number; interval?: number } = {},
): Promise<boolean> {
  const start = Date.now();
  while (true) {
    try {
      if (predicate()) return true;
    } catch {
      // ignore predicate errors
    }
    if (Date.now() - start >= timeout) return false;
    // eslint-disable-next-line no-await-in-loop
    await sleep(interval);
  }
}

function isSessionReadyForConnect(status: number): boolean {
  return status === 2 || status === 3;
}

function isSessionInQueue(session: SessionInfo): boolean {
  // Official client treats seat setup step 1 as queue state even when queuePosition reaches 1.
  // Fallback to queuePosition-based inference for payloads that do not expose seatSetupStep.
  if (session.seatSetupStep === 1) {
    return true;
  }
  return (session.queuePosition ?? 0) > 1;
}

function isNumericId(value: string | undefined): value is string {
  if (!value) return false;
  return /^\d+$/.test(value);
}

function parseNumericId(value: string | undefined): number | null {
  if (!isNumericId(value)) return null;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}

function defaultVariantId(game: GameInfo): string {
  return game.variants[game.selectedVariantIndex]?.id ?? game.variants[0]?.id ?? game.id;
}

function getSelectedVariant(game: GameInfo, variantId: string): GameVariant | undefined {
  return game.variants.find((variant) => variant.id === variantId) ?? game.variants[0];
}

function findSessionContextForAppId(
  catalog: GameInfo[],
  variantByGameId: Record<string, string>,
  appId: number,
): { game: GameInfo; variant?: GameVariant } | null {
  for (const game of catalog) {
    const matchedVariant = game.variants.find((variant) => parseNumericId(variant.id) === appId);
    if (matchedVariant) {
      return { game, variant: matchedVariant };
    }

    if (parseNumericId(game.launchAppId) === appId) {
      const preferredVariantId = variantByGameId[game.id] ?? defaultVariantId(game);
      return {
        game,
        variant: getSelectedVariant(game, preferredVariantId),
      };
    }
  }

  return null;
}

function matchesGameSearch(game: GameInfo, query: string): boolean {
  const normalizedQuery = query.trim().toLowerCase();
  if (!normalizedQuery) return true;
  if (game.searchText?.includes(normalizedQuery)) return true;
  return [
    game.title,
    game.description,
    game.publisherName,
    ...(game.genres ?? []),
    ...(game.featureLabels ?? []),
    ...(game.availableStores ?? []),
  ]
    .filter((value): value is string => typeof value === "string" && value.length > 0)
    .some((value) => value.toLowerCase().includes(normalizedQuery));
}

function areStringArraysEqual(left: string[], right: string[]): boolean {
  if (left.length != right.length) {
    return false;
  }
  return left.every((value, index) => value === right[index]);
}

function sortLibraryGames(
  games: GameInfo[],
  sortId: string,
  playtimeData: Record<string, { lastPlayedAt?: string | null; totalSeconds?: number; sessionCount?: number }>,
): GameInfo[] {
  const copy = [...games];
  const compareTitle = (left: GameInfo, right: GameInfo) => left.title.localeCompare(right.title);
  const playtimeLastPlayedMs = (gameId: string): number => {
    const raw = playtimeData[gameId]?.lastPlayedAt;
    if (!raw) return 0;
    const ms = Date.parse(raw);
    return Number.isFinite(ms) ? ms : 0;
  };
  const legacyLastPlayedMs = (game: GameInfo): number => {
    if (!game.lastPlayed) return 0;
    const ms = Date.parse(game.lastPlayed);
    return Number.isFinite(ms) ? ms : 0;
  };
  if (sortId === "z_to_a") {
    return copy.sort((left, right) => right.title.localeCompare(left.title));
  }
  if (sortId === "a_to_z") {
    return copy.sort(compareTitle);
  }
  if (sortId === "last_played") {
    return copy.sort((left, right) => {
      const leftTime = playtimeLastPlayedMs(left.id) || legacyLastPlayedMs(left);
      const rightTime = playtimeLastPlayedMs(right.id) || legacyLastPlayedMs(right);
      if (leftTime === rightTime) return compareTitle(left, right);
      return rightTime - leftTime;
    });
  }
  if (sortId === "last_added") {
    // Preserve server-provided order. We do not currently have a trustworthy local "addedAt" field.
    return copy;
  }
  if (sortId === "most_popular") {
    return copy.sort((left, right) => {
      const leftSeconds = Math.max(0, playtimeData[left.id]?.totalSeconds ?? 0);
      const rightSeconds = Math.max(0, playtimeData[right.id]?.totalSeconds ?? 0);
      if (leftSeconds !== rightSeconds) return rightSeconds - leftSeconds;
      const leftSessions = Math.max(0, playtimeData[left.id]?.sessionCount ?? 0);
      const rightSessions = Math.max(0, playtimeData[right.id]?.sessionCount ?? 0);
      if (leftSessions !== rightSessions) return rightSessions - leftSessions;
      return compareTitle(left, right);
    });
  }
  return copy.sort(compareTitle);
}

function mergeVariantSelections(
  current: Record<string, string>,
  catalog: GameInfo[],
): Record<string, string> {
  if (catalog.length === 0) {
    return current;
  }

  const next = { ...current };
  for (const game of catalog) {
    const selectedVariantId = next[game.id];
    const hasSelectedVariant = !!selectedVariantId && game.variants.some((variant) => variant.id === selectedVariantId);
    if (!hasSelectedVariant) {
      next[game.id] = defaultVariantId(game);
    }
  }
  return next;
}

function defaultDiagnostics(): StreamDiagnostics {
  return {
    connectionState: "closed",
    inputReady: false,
    nativeRendererActive: false,
    connectedGamepads: 0,
    resolution: "",
    codec: "",
    hardwareAcceleration: "",
    colorCodec: "",
    isHdr: false,
    bitrateKbps: 0,
    targetBitrateKbps: 0,
    decodeFps: 0,
    renderFps: 0,
    packetsLost: 0,
    packetsReceived: 0,
    packetLossPercent: 0,
    jitterMs: 0,
    rttMs: 0,
    framesReceived: 0,
    framesDecoded: 0,
    framesDropped: 0,
    decodeTimeMs: 0,
    renderTimeMs: 0,
    jitterBufferDelayMs: 0,
    inputQueueBufferedBytes: 0,
    inputQueuePeakBufferedBytes: 0,
    partiallyReliableInputQueueBufferedBytes: 0,
    partiallyReliableInputQueuePeakBufferedBytes: 0,
    inputQueueDropCount: 0,
    inputQueueMaxSchedulingDelayMs: 0,
    partiallyReliableInputOpen: false,
    mouseMoveTransport: "reliable",
    mouseFlushIntervalMs: 8,
    mousePacketsPerSecond: 0,
    mouseResidualMagnitude: 0,
    mouseAdaptiveFlushActive: false,
    lagReason: "unknown",
    lagReasonDetail: "Waiting for stream stats",
    gpuType: "",
    serverRegion: "",
    decoderPressureActive: false,
    decoderRecoveryAttempts: 0,
    decoderRecoveryAction: "none",
    nativeRequestedFps: undefined,
    nativeCapsFramerate: undefined,
    nativeQueueMode: undefined,
    nativeFramesPendingToPresent: undefined,
    nativePartialFlushCount: undefined,
    nativeCompleteFlushCount: undefined,
    nativeTransitionSummary: undefined,
    nativeRequestedStreamingFeaturesSummary: undefined,
    nativeFinalizedStreamingFeaturesSummary: undefined,
    micState: "uninitialized",
    micEnabled: false,
  };
}

function mergeNativeStreamStats(
  current: StreamDiagnostics,
  stats: NativeStreamStats,
): StreamDiagnostics {
  const sinkDropped = stats.sinkDropped ?? 0;
  const sinkRendered = stats.sinkRendered ?? stats.framesRendered;
  const totalSinkFrames = sinkRendered + sinkDropped;
  const dropPercent = totalSinkFrames > 0 ? (sinkDropped / totalSinkFrames) * 100 : 0;
  const hardwareAcceleration = [
    stats.hardwareAcceleration || "GStreamer native decode",
    stats.zeroCopy && stats.memoryMode ? `${stats.memoryMode} zero-copy` : "",
    !stats.zeroCopy && stats.memoryMode ? stats.memoryMode : "",
    !stats.memoryMode && stats.zeroCopyD3D12 ? "D3D12 zero-copy" : "",
    !stats.memoryMode && stats.zeroCopyD3D11 ? "D3D11 zero-copy" : "",
  ].filter(Boolean).join(" · ");

  return {
    ...current,
    connectionState: "connected",
    inputReady: current.inputReady,
    nativeRendererActive: true,
    resolution: stats.resolution || current.resolution,
    codec: stats.codec || current.codec,
    hardwareAcceleration,
    bitrateKbps: stats.bitrateKbps,
    targetBitrateKbps: stats.targetBitrateKbps,
    decodeFps: Math.round(stats.decodedFps),
    renderFps: Math.round(stats.renderFps),
    framesReceived: stats.framesDecoded,
    framesDecoded: stats.framesDecoded,
    framesDropped: sinkDropped,
    packetLossPercent: dropPercent,
    lagReason: dropPercent > 1 ? "render" : "stable",
    lagReasonDetail: stats.lastTransitionSummary
      ? `Native bitrate ${stats.bitratePerformancePercent.toFixed(0)}% of target · ${stats.lastTransitionSummary}`
      : `Native bitrate ${stats.bitratePerformancePercent.toFixed(0)}% of target`,
    decoderPressureActive: false,
    nativeRequestedFps: stats.requestedFps,
    nativeCapsFramerate: stats.capsFramerate,
    nativeQueueMode: stats.queueMode,
    nativeFramesPendingToPresent: stats.framesPendingToPresent,
    nativePartialFlushCount: stats.partialFlushCount,
    nativeCompleteFlushCount: stats.completeFlushCount,
    nativeTransitionSummary: stats.lastTransitionSummary,
    nativeRequestedStreamingFeaturesSummary: stats.requestedStreamingFeaturesSummary,
    nativeFinalizedStreamingFeaturesSummary: stats.finalizedStreamingFeaturesSummary,
  };
}

function isSessionLimitError(error: unknown): boolean {
  if (error && typeof error === "object" && "gfnErrorCode" in error) {
    const candidate = error.gfnErrorCode;
    if (typeof candidate === "number") {
      return candidate === 3237093643 || candidate === 3237093718;
    }
  }
  if (error instanceof Error) {
    const msg = error.message.toUpperCase();
    return msg.includes("SESSION LIMIT") || msg.includes("INSUFFICIENT_PLAYABILITY") || msg.includes("DUPLICATE SESSION");
  }
  return false;
}

function warningTone(code: StreamTimeWarning["code"]): "warn" | "critical" {
  if (code === 3) {
    return "critical";
  }
  return "warn";
}

function warningMessage(code: StreamTimeWarning["code"]): string {
  if (code === 1) return "Session time limit approaching";
  if (code === 2) return "Idle timeout approaching";
  return "Maximum session time approaching";
}

function normalizeMembershipTier(tier: string | null | undefined): string | null {
  if (!tier) {
    return null;
  }
  const normalized = tier.trim().toUpperCase();
  return normalized.length > 0 ? normalized : null;
}

function shouldShowQueueAdsForMembership(
  subscription: SubscriptionInfo | null,
  authSession: AuthSession | null,
): boolean {
  const effectiveTier = normalizeMembershipTier(subscription?.membershipTier ?? authSession?.user.membershipTier);
  return effectiveTier === null || effectiveTier === "FREE";
}

function shouldShowFreeTierSessionWarnings(subscription: SubscriptionInfo | null): boolean {
  return normalizeMembershipTier(subscription?.membershipTier) === "FREE";
}

function hasCrossedWarningThreshold(
  previousSeconds: number | null,
  currentSeconds: number,
  thresholdSeconds: number,
): boolean {
  if (previousSeconds === null) {
    return currentSeconds === thresholdSeconds;
  }
  return previousSeconds > thresholdSeconds && currentSeconds <= thresholdSeconds;
}

function getLocalSessionTimerWarning(
  stage: LocalSessionTimerWarningState["stage"],
  secondsLeft: number,
): StreamWarningState {
  if (stage === "free-tier-30m") {
    return {
      code: 1,
      message: "30 minutes remaining in this free-tier session",
      tone: "warn",
    };
  }

  if (stage === "free-tier-15m") {
    return {
      code: 1,
      message: "15 minutes remaining in this free-tier session",
      tone: "warn",
    };
  }

  return {
    code: 1,
    message: "This free-tier session ends soon",
    tone: "critical",
    secondsLeft: Math.max(0, secondsLeft),
  };
}

function shouldUseQueueAdPolling(session: SessionInfo, subscription: SubscriptionInfo | null, authSession: AuthSession | null): boolean {
  return (
    shouldShowQueueAdsForMembership(subscription, authSession) &&
    isSessionInQueue(session) &&
    isSessionAdsRequired(session.adState)
  );
}

function getEffectiveAdState(
  session: SessionInfo | null,
  subscription: SubscriptionInfo | null,
  authSession: AuthSession | null,
): SessionAdState | undefined {
  if (!session) {
    return undefined;
  }

  if (session.adState) {
    return session.adState;
  }

  if ((session.status === 1 || session.status === 2 || session.status === 3) && session.queuePosition !== undefined) {
    return {
      isAdsRequired: true,
      sessionAdsRequired: true,
      message: "Free-tier queue ads begin as soon as you enter queue.",
      opportunity: {
        message: "Free-tier queue ads begin as soon as you enter queue.",
      },
      sessionAds: [],
      ads: [],
      serverSentEmptyAds: true,
    };
  }

  if (!shouldShowQueueAdsForMembership(subscription, authSession)) {
    return undefined;
  }

  if (!isSessionInQueue(session)) {
    return undefined;
  }

  return {
    isAdsRequired: true,
    sessionAdsRequired: true,
    message: "Free-tier queue ads begin as soon as you enter queue.",
    opportunity: {
      message: "Free-tier queue ads begin as soon as you enter queue.",
    },
    sessionAds: [
      {
        adId: "queue-ad-placeholder",
        title: "Advertisement in progress",
        description: "Ad media will appear here as soon as it is available.",
      },
    ],
    ads: [
      {
        adId: "queue-ad-placeholder",
        title: "Advertisement in progress",
        description: "Ad media will appear here as soon as it is available.",
      },
    ],
  };
}

function mergeAdState(
  previous: SessionAdState | undefined,
  next: SessionAdState | undefined,
): SessionAdState | undefined {
  if (!next) {
    return previous;
  }
  // Server only populates sessionAds in the first poll after session creation.
  // Later polls return sessionAdsRequired=true but sessionAds=null (serverSentEmptyAds=true),
  // which produces an empty ads array. Preserve the ad list from the most recent poll that
  // had URLs so the ad player can continue.
  // Do NOT restore when serverSentEmptyAds is false — that signals an explicit client-side
  // clear after a rejected finish action, and we must NOT bring the stale ad back.
  if (
    isSessionAdsRequired(next) &&
    next.serverSentEmptyAds === true &&
    getSessionAdItems(next).length === 0 &&
    previous?.sessionAds &&
    previous.sessionAds.length > 0
  ) {
    return { ...next, sessionAds: previous.sessionAds, ads: previous.ads };
  }
  return next;
}

function mergePolledSessionState(previous: SessionInfo, next: SessionInfo): SessionInfo {
  if (isSessionReadyForConnect(next.status)) {
    return next;
  }

  return {
    ...next,
    adState: mergeAdState(previous.adState, next.adState),
    mediaConnectionInfo: next.mediaConnectionInfo ?? previous.mediaConnectionInfo,
  };
}

function getNextAdReportAction(
  lastAction: SessionAdAction | undefined,
  playbackEvent: "playing" | "paused" | "ended",
): SessionAdAction | null {
  switch (playbackEvent) {
    case "playing":
      if (!lastAction) {
        return "start";
      }
      return lastAction === "pause" ? "resume" : null;
    case "paused":
      return lastAction === "start" || lastAction === "resume" ? "pause" : null;
    case "ended":
      return lastAction === "finish" || lastAction === "cancel" ? null : "finish";
    default:
      return null;
  }
}

function getActiveQueueAd(
  adState: SessionAdState | undefined,
  activeAdId: string | null,
): SessionAdInfo | undefined {
  const ads = getSessionAdItems(adState);
  if (!ads.length) {
    return undefined;
  }

  if (activeAdId) {
    const matched = ads.find((ad) => ad.adId === activeAdId);
    if (matched) {
      return matched;
    }
  }

  return ads[0];
}

function getNextQueueAd(
  adState: SessionAdState | undefined,
  activeAdId: string,
): SessionAdInfo | undefined {
  const ads = getSessionAdItems(adState);
  if (!ads.length) {
    return undefined;
  }

  const currentIndex = ads.findIndex((ad) => ad.adId === activeAdId);
  if (currentIndex < 0) {
    return ads[0];
  }

  return ads[currentIndex + 1];
}

function toLoadingStatus(status: StreamStatus): StreamLoadingStatus {
  switch (status) {
    case "queue":
    case "setup":
    case "starting":
    case "connecting":
      return status;
    default:
      return "queue";
  }
}

function toCodeLabel(code: number | undefined): string | undefined {
  if (code === undefined) return undefined;
  if (code === 3237093643) return `SessionLimitExceeded (${code})`;
  if (code === 3237093718) return `SessionInsufficientPlayabilityLevel (${code})`;
  return `GFN Error ${code}`;
}

function extractLaunchErrorCode(error: unknown): number | undefined {
  if (error && typeof error === "object") {
    if ("gfnErrorCode" in error) {
      const directCode = error.gfnErrorCode;
      if (typeof directCode === "number") return directCode;
    }
    if ("statusCode" in error) {
      const statusCode = error.statusCode;
      if (typeof statusCode === "number" && statusCode > 0 && statusCode < 255) {
        return 3237093632 + statusCode;
      }
    }
  }
  if (error instanceof Error) {
    const match = error.message.match(/\b(3237\d{6,})\b/);
    if (match) {
      const code = Number(match[1]);
      if (Number.isFinite(code)) return code;
    }
  }
  return undefined;
}

function toLaunchErrorState(error: unknown, stage: StreamLoadingStatus): LaunchErrorState {
  const unknownMessage = "The game could not start. Please try again.";

  const titleFromError =
    error && typeof error === "object" && "title" in error && typeof error.title === "string"
      ? error.title.trim()
      : "";
  const descriptionFromError =
    error && typeof error === "object" && "description" in error && typeof error.description === "string"
      ? error.description.trim()
      : "";
  const statusDescription =
    error && typeof error === "object" && "statusDescription" in error && typeof error.statusDescription === "string"
      ? error.statusDescription.trim()
      : "";
  const messageFromError = error instanceof Error ? error.message.trim() : "";
  const combined = `${statusDescription} ${messageFromError}`.toUpperCase();
  const code = extractLaunchErrorCode(error);

  if (
    isSessionLimitError(error) ||
    combined.includes("INSUFFICIENT_PLAYABILITY") ||
    combined.includes("SESSION_LIMIT") ||
    combined.includes("DUPLICATE SESSION")
  ) {
    return {
      stage,
      title: "Duplicate Session Detected",
      description: "Another session is already running on your account. Close it first or wait for it to timeout, then launch again.",
      codeLabel: toCodeLabel(code),
    };
  }

  return {
    stage,
    title: titleFromError || "Launch Failed",
    description: descriptionFromError || messageFromError || statusDescription || unknownMessage,
    codeLabel: toCodeLabel(code),
  };
}

function streamStatusToLoadingStage(status: StreamStatus): StreamLoadingStatus {
  if (status === "queue" || status === "setup" || status === "starting" || status === "connecting") {
    return status;
  }
  return "connecting";
}

export function App(): JSX.Element {

  // Auth State
  const [authSession, setAuthSession] = useState<AuthSession | null>(null);
  const [savedAccounts, setSavedAccounts] = useState<SavedAccount[]>([]);
  const [providers, setProviders] = useState<LoginProvider[]>([]);
  const [providerIdpId, setProviderIdpId] = useState("");
  const [isLoggingIn, setIsLoggingIn] = useState(false);
  const [loginError, setLoginError] = useState<string | null>(null);
  const [isInitializing, setIsInitializing] = useState(true);
  const [startupStatusMessage, setStartupStatusMessage] = useState("Restoring saved session...");
  const [startupRefreshNotice, setStartupRefreshNotice] = useState<{
    tone: "success" | "warn";
    text: string;
  } | null>(null);

  // Navigation
  const [currentPage, setCurrentPage] = useState<AppPage>("home");
  const [sessionFullscreen, setSessionFullscreenState] = useState(false);

  // Games State
  const [games, setGames] = useState<GameInfo[]>([]);
  const [libraryGames, setLibraryGames] = useState<GameInfo[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedGameId, setSelectedGameId] = useState("");
  const [variantByGameId, setVariantByGameId] = useState<Record<string, string>>({});
  const [isLoadingGames, setIsLoadingGames] = useState(false);
  const [catalogFilterGroups, setCatalogFilterGroups] = useState<CatalogFilterGroup[]>([]);
  const [catalogSortOptions, setCatalogSortOptions] = useState<CatalogSortOption[]>([]);
  const [catalogSelectedSortId, setCatalogSelectedSortId] = useState(() => loadCatalogPreferences().sortId);
  const [catalogSelectedFilterIds, setCatalogSelectedFilterIds] = useState<string[]>(() => loadCatalogPreferences().filterIds);
  const [catalogTotalCount, setCatalogTotalCount] = useState(0);
  const [catalogSupportedCount, setCatalogSupportedCount] = useState(0);
  const catalogFilterKey = useMemo(() => catalogSelectedFilterIds.join("|"), [catalogSelectedFilterIds]);

  // Settings State
  const [settings, setSettings] = useState<Settings>({
    resolution: "1920x1080",
    aspectRatio: "16:9",
    posterSizeScale: 1,
    fps: 60,
    maxBitrateMbps: 75,
    streamClientMode: "web",
    nativeStreamerBackend: "gstreamer",
    nativeStreamerExecutablePath: "",
    nativeCloudGsyncMode: "auto",
    nativeD3dFullscreenMode: "auto",
    nativeExternalRenderer: true,
    codec: DEFAULT_STREAM_PREFERENCES.codec,
    decoderPreference: "auto",
    encoderPreference: "auto",
    colorQuality: DEFAULT_STREAM_PREFERENCES.colorQuality,
    region: "",
    sessionProxyEnabled: false,
    sessionProxyUrl: "",
    clipboardPaste: false,
    mouseSensitivity: 1,
    mouseAcceleration: 1,
    shortcutToggleStats: DEFAULT_SHORTCUTS.shortcutToggleStats,
    shortcutTogglePointerLock: DEFAULT_SHORTCUTS.shortcutTogglePointerLock,
    shortcutToggleFullscreen: DEFAULT_SHORTCUTS.shortcutToggleFullscreen,
    shortcutStopStream: DEFAULT_SHORTCUTS.shortcutStopStream,
    shortcutToggleAntiAfk: DEFAULT_SHORTCUTS.shortcutToggleAntiAfk,
    shortcutToggleMicrophone: DEFAULT_SHORTCUTS.shortcutToggleMicrophone,
    shortcutScreenshot: DEFAULT_SHORTCUTS.shortcutScreenshot,
    shortcutToggleRecording: DEFAULT_SHORTCUTS.shortcutToggleRecording,
    microphoneMode: "disabled",
    microphoneDeviceId: "",
    hideStreamButtons: false,
    showAntiAfkIndicator: true,
    showStatsOnLaunch: false,
    hideServerSelector: false,
    controllerMode: false,
    controllerUiSounds: false,
    controllerBackgroundAnimations: false,
    controllerThemeStyle: "aurora",
    controllerThemeColor: { r: 124, g: 241, b: 177 },
    controllerLibraryGameBackdrop: true,
    autoLoadControllerLibrary: false,
    autoFullScreen: false,
    favoriteGameIds: [],
    sessionCounterEnabled: false,
    sessionClockShowEveryMinutes: 60,
    sessionClockShowDurationSeconds: 30,
    windowWidth: 1400,
    windowHeight: 900,
    keyboardLayout: DEFAULT_KEYBOARD_LAYOUT,
    gameLanguage: "en_US",
    enableL4S: false,
    enableCloudGsync: false,
    discordRichPresence: false,
    autoCheckForUpdates: true,
  });
  const [settingsLoaded, setSettingsLoaded] = useState(false);
  const [codecResults, setCodecResults] = useState<CodecTestResult[] | null>(() => loadStoredCodecResults());
  const [codecTesting, setCodecTesting] = useState(false);
  const [regions, setRegions] = useState<StreamRegion[]>([]);
  const [subscriptionInfo, setSubscriptionInfo] = useState<SubscriptionInfo | null>(null);
  const diagnosticsStoreRef = useRef<ReturnType<typeof createStreamDiagnosticsStore> | null>(null);
  const diagnosticsStore =
    diagnosticsStoreRef.current ?? (diagnosticsStoreRef.current = createStreamDiagnosticsStore(defaultDiagnostics()));
  const streamMenuMicOn = useStreamDiagnosticsSelector(diagnosticsStore, (d) => d.micEnabled);

  // Stream State
  const [session, setSession] = useState<SessionInfo | null>(null);
  const [streamStatus, setStreamStatus] = useState<StreamStatus>("idle");
  const [showStatsOverlay, setShowStatsOverlay] = useState(false);
  const [antiAfkEnabled, setAntiAfkEnabled] = useState(false);
  const [antiAfkAckNonce, setAntiAfkAckNonce] = useState(0);
  const [exitPrompt, setExitPrompt] = useState<ExitPromptState>({ open: false, gameTitle: "Game" });
  const [streamingGame, setStreamingGame] = useState<GameInfo | null>(null);
  const [streamingStore, setStreamingStore] = useState<string | null>(null);
  const [queuePosition, setQueuePosition] = useState<number | undefined>();
  const [navbarActiveSession, setNavbarActiveSession] = useState<ActiveSessionInfo | null>(null);
  const [isResumingNavbarSession, setIsResumingNavbarSession] = useState(false);
  const [isTerminatingNavbarSession, setIsTerminatingNavbarSession] = useState(false);
  const [accountToRemove, setAccountToRemove] = useState<string | null>(null);
  const [removeAccountConfirmOpen, setRemoveAccountConfirmOpen] = useState(false);
  const [logoutConfirmOpen, setLogoutConfirmOpen] = useState(false);
  const [launchError, setLaunchError] = useState<LaunchErrorState | null>(null);
  const [queueModalGame, setQueueModalGame] = useState<GameInfo | null>(null);
  const [queueModalData, setQueueModalData] = useState<PrintedWasteQueueData | null>(null);
  const [sessionStartedAtMs, setSessionStartedAtMs] = useState<number | null>(null);
  const [remoteStreamWarning, setRemoteStreamWarning] = useState<StreamWarningState | null>(null);
  const [localSessionTimerWarning, setLocalSessionTimerWarning] = useState<LocalSessionTimerWarningState | null>(null);
  const [activeQueueAdId, setActiveQueueAdId] = useState<string | null>(null);
  const previousFreeTierRemainingSecondsRef = useRef<number | null>(null);

  const { playtime, startSession: startPlaytimeSession, endSession: endPlaytimeSession } = usePlaytime();
  const sessionElapsedSeconds = useElapsedSeconds(sessionStartedAtMs, streamStatus === "streaming");
  const isStreaming = streamStatus === "streaming";
  const freeTierSessionWarningsActive =
    isStreaming && sessionStartedAtMs !== null && shouldShowFreeTierSessionWarnings(subscriptionInfo);
  const freeTierSessionRemainingSeconds = freeTierSessionWarningsActive
    ? Math.max(0, FREE_TIER_SESSION_LIMIT_SECONDS - sessionElapsedSeconds)
    : null;
  const visibleLocalSessionTimerWarning = useMemo(() => {
    if (localSessionTimerWarning === null || freeTierSessionRemainingSeconds === null) {
      return null;
    }

    return getLocalSessionTimerWarning(localSessionTimerWarning.stage, freeTierSessionRemainingSeconds);
  }, [freeTierSessionRemainingSeconds, localSessionTimerWarning]);
  const streamWarning = useMemo(() => {
    if (visibleLocalSessionTimerWarning?.tone === "critical") {
      return visibleLocalSessionTimerWarning;
    }
    return remoteStreamWarning ?? visibleLocalSessionTimerWarning;
  }, [remoteStreamWarning, visibleLocalSessionTimerWarning]);

  const controllerOverlayOpenRef = useRef(false);
  const codecTestPromiseRef = useRef<Promise<CodecTestResult[] | null> | null>(null);
  const codecStartupTestAttemptedRef = useRef(false);
  const navbarSessionActionInFlightRef = useRef<"resume" | "terminate" | null>(null);
  const nativeStreamingRef = useRef(false);

  const resetStatsOverlayToPreference = useCallback((): void => {
    setShowStatsOverlay(settings.showStatsOnLaunch);
  }, [settings.showStatsOnLaunch]);

  const runCodecTest = useCallback(async (): Promise<void> => {
    if (codecTestPromiseRef.current) {
      await codecTestPromiseRef.current;
      return;
    }

    const testPromise = (async (): Promise<CodecTestResult[] | null> => {
      setCodecTesting(true);
      try {
        const results = await testCodecSupport();
        setCodecResults(results);
        saveStoredCodecResults(results);
        return results;
      } catch (error) {
        console.error("Codec test failed:", error);
        return null;
      } finally {
        setCodecTesting(false);
        codecTestPromiseRef.current = null;
      }
    })();

    codecTestPromiseRef.current = testPromise;
    await testPromise;
  }, []);

  const handleControllerPageNavigate = useCallback((direction: "prev" | "next"): void => {
    if (controllerOverlayOpenRef.current) {
      window.dispatchEvent(new CustomEvent("opennow:controller-shoulder", { detail: { direction } }));
      return;
    }
    if (!authSession || streamStatus !== "idle") {
      return;
    }

    if (settings.controllerMode && currentPage === "library") {
      window.dispatchEvent(new CustomEvent("opennow:controller-shoulder", { detail: { direction } }));
      return;
    }

    const currentIndex = APP_PAGE_ORDER.indexOf(currentPage);
    const step = direction === "next" ? 1 : -1;
    const nextIndex = (currentIndex + step + APP_PAGE_ORDER.length) % APP_PAGE_ORDER.length;
    setCurrentPage(APP_PAGE_ORDER[nextIndex]);
  }, [authSession, currentPage, settings.controllerMode, streamStatus]);

  const handleControllerBackAction = useCallback((): boolean => {
    // Prefer to let the controller library handle Back (e.g. closing submenus
    // inside the XMB) before falling back to global navigation.
    const cancelEvent = new CustomEvent("opennow:controller-cancel", { cancelable: true });
    window.dispatchEvent(cancelEvent);
    if (cancelEvent.defaultPrevented) {
      return true;
    }

    if (controllerOverlayOpenRef.current) {
      setControllerOverlayOpen(false);
      return true;
    }

    if (!authSession || streamStatus !== "idle") {
      return false;
    }

    if (settings.controllerMode && currentPage === "settings") {
      setCurrentPage("library");
      return true;
    }

    if (currentPage !== "home") {
      setCurrentPage("home");
      return true;
    }
    return false;
  }, [authSession, currentPage, settings.controllerMode, streamStatus]);

  const handleControllerDirectionInput = useCallback((direction: "up" | "down" | "left" | "right"): boolean => {
    if (controllerOverlayOpenRef.current) {
      window.dispatchEvent(new CustomEvent("opennow:controller-direction", { detail: { direction } }));
      return true;
    }
    if (!authSession || streamStatus !== "idle") {
      return false;
    }
    if (settings.controllerMode && currentPage === "library") {
      window.dispatchEvent(new CustomEvent("opennow:controller-direction", { detail: { direction } }));
      return true;
    }
    if (settings.controllerMode && currentPage === "settings") {
      window.dispatchEvent(new CustomEvent("opennow:controller-direction", { detail: { direction } }));
      return true;
    }
    return false;
  }, [authSession, currentPage, settings.controllerMode, streamStatus]);

  const handleControllerActivateInput = useCallback((): boolean => {
    if (controllerOverlayOpenRef.current) {
      window.dispatchEvent(new CustomEvent("opennow:controller-activate"));
      return true;
    }
    if (!authSession || streamStatus !== "idle") {
      return false;
    }
    if (settings.controllerMode && currentPage === "library") {
      window.dispatchEvent(new CustomEvent("opennow:controller-activate"));
      return true;
    }
    if (settings.controllerMode && currentPage === "settings") {
      window.dispatchEvent(new CustomEvent("opennow:controller-activate"));
      return true;
    }
    return false;
  }, [authSession, currentPage, settings.controllerMode, streamStatus]);

  const handleControllerSecondaryActivateInput = useCallback((): boolean => {
    if (controllerOverlayOpenRef.current) {
      window.dispatchEvent(new CustomEvent("opennow:controller-secondary-activate"));
      return true;
    }
    if (!authSession || streamStatus !== "idle") {
      return false;
    }
    if (settings.controllerMode && currentPage === "library") {
      window.dispatchEvent(new CustomEvent("opennow:controller-secondary-activate"));
      return true;
    }
    if (settings.controllerMode && currentPage === "settings") {
      window.dispatchEvent(new CustomEvent("opennow:controller-secondary-activate"));
      return true;
    }
    return false;
  }, [authSession, currentPage, settings.controllerMode, streamStatus]);

  const handleControllerTertiaryActivateInput = useCallback((): boolean => {
    if (controllerOverlayOpenRef.current) {
      window.dispatchEvent(new CustomEvent("opennow:controller-tertiary-activate"));
      return true;
    }
    if (!authSession || streamStatus !== "idle") {
      return false;
    }
    if (settings.controllerMode && currentPage === "library") {
      window.dispatchEvent(new CustomEvent("opennow:controller-tertiary-activate"));
      return true;
    }
    if (settings.controllerMode && currentPage === "settings") {
      window.dispatchEvent(new CustomEvent("opennow:controller-tertiary-activate"));
      return true;
    }
    return false;
  }, [authSession, currentPage, settings.controllerMode, streamStatus]);

  const [controllerOverlayOpen, setControllerOverlayOpen] = useState(false);
  const [streamVolume, setStreamVolume] = useState(1);
  const [streamMicLevel, setStreamMicLevel] = useState(1);
  const [isSwitchingGame, setIsSwitchingGame] = useState(false);
  const [switchingPhase, setSwitchingPhase] = useState<null | "cleaning" | "creating">(null);
  const [pendingSwitchGameTitle, setPendingSwitchGameTitle] = useState<string | null>(null);
  const [pendingSwitchGameCover, setPendingSwitchGameCover] = useState<string | null>(null);
  const [pendingSwitchGameDescription, setPendingSwitchGameDescription] = useState<string | null>(null);
  const [pendingSwitchGameId, setPendingSwitchGameId] = useState<string | null>(null);
  const controllerDesktopModeActive = Boolean(authSession)
    && streamStatus === "idle"
    && settings.controllerMode
    && (currentPage === "library" || currentPage === "settings");
  const controllerUiActive = controllerDesktopModeActive || controllerOverlayOpen;

  const controllerConnected = useControllerNavigation({
    enabled: controllerUiActive,
    onNavigatePage: handleControllerPageNavigate,
    onBackAction: handleControllerBackAction,
    onDirectionInput: handleControllerDirectionInput,
    onActivateInput: handleControllerActivateInput,
    onSecondaryActivateInput: handleControllerSecondaryActivateInput,
    onTertiaryActivateInput: handleControllerTertiaryActivateInput,
  });
  const showControllerHint = controllerUiActive
    && controllerConnected
    && !(settings.controllerMode && currentPage === "library");

  // Refs
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const clientRef = useRef<GfnWebRtcClient | null>(null);
  const controllerUiSoundsRef = useRef(settings.controllerUiSounds);
  const isStreamingRef = useRef(streamStatus === "streaming");

  useEffect(() => {
    controllerUiSoundsRef.current = settings.controllerUiSounds;
  }, [settings.controllerUiSounds]);
  useEffect(() => {
    isStreamingRef.current = streamStatus === "streaming";
  }, [streamStatus]);

  const handleControllerMetaToggle = useCallback(() => {
    if (!isStreamingRef.current) return;
    setControllerOverlayOpen((currentOpen) => {
      const opening = !currentOpen;
      if (controllerUiSoundsRef.current) {
        playControllerUiSound(opening ? "confirm" : "move", true);
      }
      return opening;
    });
  }, []);

  useEffect(() => {
    if (streamStatus === "streaming" && audioRef.current) {
      setStreamVolume(audioRef.current.volume);
    }
  }, [streamStatus]);
  useEffect(() => {
    if (audioRef.current) {
      audioRef.current.volume = streamVolume;
    }
    clientRef.current?.setOutputVolume(streamVolume);
  }, [streamVolume]);
  const sessionRef = useRef<SessionInfo | null>(null);
  const hasInitializedRef = useRef(false);
  const regionsRequestRef = useRef(0);
  const launchInFlightRef = useRef(false);
  const runtimeSnapshotRef = useRef<RuntimeSnapshot | null>(loadRuntimeSnapshot());
  /** Joins concurrent claim/resume calls for the same Cloud session id (single CloudMatch RESUME + signaling). */
  const claimResumePromisesRef = useRef<Map<string, Promise<void>>>(new Map());
  const launchAbortRef = useRef(false);
  const streamStatusRef = useRef<StreamStatus>(streamStatus);
  const nativeInputProtocolVersionRef = useRef<number | null>(null);
  const stableRecoveryResetTimerRef = useRef<number | null>(null);
  const remoteIceGraceTimerRef = useRef<number | null>(null);
  const remoteIceSeenForSessionRef = useRef<string | null>(null);
  const remoteIceRecoveryGenerationRef = useRef<number | null>(null);
  const awaitingRecoveryRemoteIceRef = useRef(false);
  const appUnloadingRef = useRef(false);
  const hasConfirmedRemoteIceRef = useRef(false);
  const latestIceConnectionStateRef = useRef<RTCIceConnectionState>("new");
  const iceDisconnectedRecoveryTimerRef = useRef<number | null>(null);
  const pendingControlledDisconnectsRef = useRef(0);
  const signalingRecoveryRef = useRef<SignalingRecoveryState>({
    attemptCount: 0,
    inFlight: null,
    explicitShutdown: false,
    appId: null,
    generation: 0,
  });
  const exitPromptResolverRef = useRef<((confirmed: boolean) => void) | null>(null);
  const adReportQueueRef = useRef<Promise<void>>(Promise.resolve());
  const adReportStateRef = useRef<Record<string, SessionAdAction>>({});
  const adMetricsRef = useRef<Record<string, QueueAdMetrics>>({});
  const adMediaUrlCacheRef = useRef<Record<string, string>>({});
  const queueAdPlaybackRef = useRef<{ adId: string; phase: "playing" | "finishing" } | null>(null);
  const queueAdPreviewRef = useRef<QueueAdPreviewHandle | null>(null);
  const activeQueueAdIdRef = useRef<string | null>(null);
  const adForcePlayTimeoutRef = useRef<number | null>(null);
  const adStartTimeoutRef = useRef<number | null>(null);
  const adProgressIntervalRef = useRef<number | null>(null);
  const adLastProgressTsRef = useRef<number | null>(null);
  // Stable ref so timer callbacks can call the latest reportQueueAdAction without
  // capturing a stale closure (auth state can change between scheduling and firing).
  const reportQueueAdActionRef = useRef<
    (adId: string, action: SessionAdAction, options?: QueueAdReportOptions) => void
  >(() => {});

  useEffect(() => {
    controllerOverlayOpenRef.current = controllerOverlayOpen;
    // Host-only pause: WebRTC client merges this with window blur/hidden (must not be cleared on focus).
    if (clientRef.current) {
      clientRef.current.inputPaused = controllerOverlayOpen;
    }
  }, [controllerOverlayOpen]);

  useEffect(() => {
    if (!controllerOverlayOpen) return;
    const overlay = document.querySelector(".controller-overlay");
    if (!overlay) return;
    const selector = [
      "button",
      "a[href]",
      "input:not([type='hidden'])",
      "select",
      "textarea",
      "[role='button']",
      "[tabindex]:not([tabindex='-1'])",
    ].join(",");
    const candidates = Array.from(overlay.querySelectorAll(selector)) as HTMLElement[];
    const first = candidates.find((el) => {
      const style = window.getComputedStyle(el);
      if (style.visibility === "hidden" || style.display === "none") return false;
      if ((el as HTMLButtonElement | HTMLInputElement | any).disabled) return false;
      return el.tabIndex >= 0;
    });
    if (first) {
      document.querySelectorAll<HTMLElement>(".controller-focus").forEach((n) => n.classList.remove("controller-focus"));
      first.classList.add("controller-focus");
      try {
        first.focus({ preventScroll: true });
      } catch {
        /* ignore */
      }
      first.scrollIntoView({ block: "nearest", inline: "nearest" });
    }
  }, [controllerOverlayOpen]);

  const applyVariantSelections = useCallback((catalog: GameInfo[]): void => {
    setVariantByGameId((prev) => mergeVariantSelections(prev, catalog));
  }, []);

  const clearQueueAdStartWatchdogs = useCallback((): void => {
    if (adForcePlayTimeoutRef.current) {
      clearTimeout(adForcePlayTimeoutRef.current);
      adForcePlayTimeoutRef.current = null;
    }
    if (adStartTimeoutRef.current) {
      clearTimeout(adStartTimeoutRef.current);
      adStartTimeoutRef.current = null;
    }
  }, []);

  const clearQueueAdProgressWatchdog = useCallback((): void => {
    if (adProgressIntervalRef.current) {
      clearInterval(adProgressIntervalRef.current);
      adProgressIntervalRef.current = null;
    }
    adLastProgressTsRef.current = null;
  }, []);

  const clearQueueAdWatchdogs = useCallback((): void => {
    clearQueueAdStartWatchdogs();
    clearQueueAdProgressWatchdog();
  }, [clearQueueAdProgressWatchdog, clearQueueAdStartWatchdogs]);

  const resetQueueAdMetrics = useCallback((adId: string): void => {
    adMetricsRef.current[adId] = {
      startedAtMs: null,
      wasPausedAtLeastOnce: false,
    };
  }, []);

  const markQueueAdStarted = useCallback((adId: string): void => {
    const current = adMetricsRef.current[adId];
    adMetricsRef.current[adId] = {
      startedAtMs: current?.startedAtMs ?? Date.now(),
      wasPausedAtLeastOnce: current?.wasPausedAtLeastOnce ?? false,
    };
  }, []);

  const markQueueAdPaused = useCallback((adId: string): void => {
    const current = adMetricsRef.current[adId];
    adMetricsRef.current[adId] = {
      startedAtMs: current?.startedAtMs ?? null,
      wasPausedAtLeastOnce: true,
    };
  }, []);

  const clearQueueAdMetrics = useCallback((adId: string): void => {
    delete adMetricsRef.current[adId];
  }, []);

  const getQueueAdReportPayload = useCallback((adId: string, action: SessionAdAction, options?: QueueAdReportOptions) => {
    const ad = getSessionAdItems(sessionRef.current?.adState).find((candidate) => candidate.adId === adId);
    const adLengthMs = getSessionAdDurationMs(ad);
    const snapshot = queueAdPreviewRef.current?.getSnapshot();
    const watchedTimeInMs =
      action === "finish" || action === "cancel"
        ? Math.max(0, Math.round((snapshot?.currentTime ?? 0) * 1000))
        : undefined;

    const metrics = adMetricsRef.current[adId];
    let pausedTimeInMs = 0;
    if (
      metrics?.startedAtMs &&
      metrics.wasPausedAtLeastOnce &&
      typeof adLengthMs === "number" &&
      Number.isFinite(adLengthMs) &&
      adLengthMs > 0
    ) {
      const elapsedMs = Date.now() - metrics.startedAtMs;
      if (elapsedMs > adLengthMs) {
        pausedTimeInMs = Math.round(elapsedMs - adLengthMs);
      }
    }

    return {
      watchedTimeInMs,
      pausedTimeInMs,
      cancelReason: action === "cancel" ? options?.cancelReason : undefined,
      errorInfo: action === "cancel" ? options?.errorInfo : undefined,
    };
  }, []);

  const armQueueAdStartWatchdogs = useCallback((adId: string): void => {
    clearQueueAdStartWatchdogs();
    adLastProgressTsRef.current = Date.now();

    adForcePlayTimeoutRef.current = window.setTimeout(() => {
      if (activeQueueAdIdRef.current !== adId) {
        return;
      }
      const lastAction = adReportStateRef.current[adId];
      if (lastAction === "start" || lastAction === "resume" || lastAction === "finish" || lastAction === "cancel") {
        return;
      }
      void queueAdPreviewRef.current?.attemptPlayback();
    }, SESSION_AD_FORCE_PLAY_TIMEOUT_MS);

    adStartTimeoutRef.current = window.setTimeout(() => {
      if (activeQueueAdIdRef.current !== adId) {
        return;
      }
      const lastAction = adReportStateRef.current[adId];
      if (lastAction === "start" || lastAction === "resume" || lastAction === "finish" || lastAction === "cancel") {
        return;
      }

      clearQueueAdWatchdogs();
      queueAdPlaybackRef.current = null;
      adReportStateRef.current[adId] = "cancel";
      reportQueueAdActionRef.current(adId, "cancel", {
        cancelReason: "error",
        errorInfo: "Ad play timeout",
      });
    }, SESSION_AD_START_TIMEOUT_MS);
  }, [clearQueueAdStartWatchdogs, clearQueueAdWatchdogs]);

  const ensureQueueAdProgressWatchdog = useCallback((adId: string): void => {
    adLastProgressTsRef.current = Date.now();
    if (adProgressIntervalRef.current) {
      return;
    }

    adProgressIntervalRef.current = window.setInterval(() => {
      if (activeQueueAdIdRef.current !== adId) {
        return;
      }

      const lastAction = adReportStateRef.current[adId];
      if (lastAction !== "start" && lastAction !== "resume") {
        return;
      }

      const snapshot = queueAdPreviewRef.current?.getSnapshot();
      if (!snapshot || snapshot.paused || snapshot.ended || isSessionQueuePaused(sessionRef.current?.adState)) {
        return;
      }

      const lastProgressTs = adLastProgressTsRef.current;
      if (!lastProgressTs || Date.now() - lastProgressTs < SESSION_AD_STUCK_TIMEOUT_MS) {
        return;
      }

      clearQueueAdWatchdogs();
      queueAdPlaybackRef.current = null;
      adReportStateRef.current[adId] = "cancel";
      reportQueueAdActionRef.current(adId, "cancel", {
        cancelReason: "error",
        errorInfo: "Ad video is stuck",
      });
    }, SESSION_AD_PROGRESS_CHECK_INTERVAL_MS);
  }, [clearQueueAdWatchdogs]);

  const resetLaunchRuntime = useCallback((options?: {
    keepLaunchError?: boolean;
    keepStreamingContext?: boolean;
  }): void => {
    if (stableRecoveryResetTimerRef.current !== null) {
      window.clearTimeout(stableRecoveryResetTimerRef.current);
      stableRecoveryResetTimerRef.current = null;
    }
    if (remoteIceGraceTimerRef.current !== null) {
      window.clearTimeout(remoteIceGraceTimerRef.current);
      remoteIceGraceTimerRef.current = null;
    }
    if (iceDisconnectedRecoveryTimerRef.current !== null) {
      window.clearTimeout(iceDisconnectedRecoveryTimerRef.current);
      iceDisconnectedRecoveryTimerRef.current = null;
    }
    remoteIceSeenForSessionRef.current = null;
    remoteIceRecoveryGenerationRef.current = null;
    awaitingRecoveryRemoteIceRef.current = false;
    hasConfirmedRemoteIceRef.current = false;
    latestIceConnectionStateRef.current = "new";
    pendingControlledDisconnectsRef.current = 0;
    signalingRecoveryRef.current.attemptCount = 0;
    signalingRecoveryRef.current.inFlight = null;
    signalingRecoveryRef.current.appId = null;
    setSession(null);
    setStreamStatus("idle");
    setQueuePosition(undefined);
    setSessionStartedAtMs(null);
    setRemoteStreamWarning(null);
    setLocalSessionTimerWarning(null);
    resetStatsOverlayToPreference();
    nativeStreamingRef.current = false;
    diagnosticsStore.set(defaultDiagnostics());

    if (!options?.keepStreamingContext) {
      setStreamingGame(null);
      setStreamingStore(null);
    }

    if (!options?.keepLaunchError) {
      setLaunchError(null);
    }

    // Clear Discord activity when returning to idle state
    if (settings.discordRichPresence) {
      void window.openNow.clearDiscordActivity();
    }
    runtimeSnapshotRef.current = null;
    clearRuntimeSnapshot();
  }, [diagnosticsStore, resetStatsOverlayToPreference, settings.discordRichPresence]);

  const buildCurrentStreamSettings = useCallback((): StreamSettings => ({
    resolution: settings.resolution,
    fps: settings.fps,
    maxBitrateMbps: settings.maxBitrateMbps,
    codec: settings.codec,
    colorQuality: settings.colorQuality,
    keyboardLayout: settings.keyboardLayout,
    gameLanguage: settings.gameLanguage,
    enableL4S: settings.enableL4S,
    enableCloudGsync: settings.enableCloudGsync,
    clientMode: settings.streamClientMode,
    nativeStreamerBackend: "gstreamer",
    nativeCloudGsyncMode: settings.nativeCloudGsyncMode,
    nativeTransitionDiagnostics: settings.nativeTransitionDiagnostics,
  }), [
    settings.codec,
    settings.colorQuality,
    settings.enableCloudGsync,
    settings.enableL4S,
    settings.fps,
    settings.gameLanguage,
    settings.keyboardLayout,
    settings.maxBitrateMbps,
    settings.nativeCloudGsyncMode,
    settings.nativeTransitionDiagnostics,
    settings.resolution,
    settings.streamClientMode,
  ]);

  const buildSignalingConnectRequest = useCallback((activeSession: SessionInfo): SignalingConnectRequest => {
    const streamSettings = buildCurrentStreamSettings();
    return {
      sessionId: activeSession.sessionId,
      signalingServer: activeSession.signalingServer,
      signalingUrl: activeSession.signalingUrl,
      nativeStreamer: buildNativeStreamerSessionContext(activeSession, streamSettings),
    };
  }, [buildCurrentStreamSettings]);
  const resetSignalingRecoveryState = useCallback((options?: {
    keepExplicitShutdown?: boolean;
  }): void => {
    if (stableRecoveryResetTimerRef.current !== null) {
      window.clearTimeout(stableRecoveryResetTimerRef.current);
      stableRecoveryResetTimerRef.current = null;
    }
    if (remoteIceGraceTimerRef.current !== null) {
      window.clearTimeout(remoteIceGraceTimerRef.current);
      remoteIceGraceTimerRef.current = null;
    }
    if (iceDisconnectedRecoveryTimerRef.current !== null) {
      window.clearTimeout(iceDisconnectedRecoveryTimerRef.current);
      iceDisconnectedRecoveryTimerRef.current = null;
    }
    remoteIceSeenForSessionRef.current = null;
    remoteIceRecoveryGenerationRef.current = null;
    awaitingRecoveryRemoteIceRef.current = false;
    hasConfirmedRemoteIceRef.current = false;
    latestIceConnectionStateRef.current = "new";
    pendingControlledDisconnectsRef.current = 0;
    signalingRecoveryRef.current.generation += 1;
    signalingRecoveryRef.current.attemptCount = 0;
    signalingRecoveryRef.current.inFlight = null;
    signalingRecoveryRef.current.appId = null;
    if (!options?.keepExplicitShutdown) {
      signalingRecoveryRef.current.explicitShutdown = false;
    }
  }, []);

  const markExplicitSignalingShutdown = useCallback((): void => {
    if (stableRecoveryResetTimerRef.current !== null) {
      window.clearTimeout(stableRecoveryResetTimerRef.current);
      stableRecoveryResetTimerRef.current = null;
    }
    if (remoteIceGraceTimerRef.current !== null) {
      window.clearTimeout(remoteIceGraceTimerRef.current);
      remoteIceGraceTimerRef.current = null;
    }
    if (iceDisconnectedRecoveryTimerRef.current !== null) {
      window.clearTimeout(iceDisconnectedRecoveryTimerRef.current);
      iceDisconnectedRecoveryTimerRef.current = null;
    }
    remoteIceSeenForSessionRef.current = null;
    remoteIceRecoveryGenerationRef.current = null;
    awaitingRecoveryRemoteIceRef.current = false;
    hasConfirmedRemoteIceRef.current = false;
    latestIceConnectionStateRef.current = "new";
    pendingControlledDisconnectsRef.current = 0;
    signalingRecoveryRef.current.generation += 1;
    signalingRecoveryRef.current.explicitShutdown = true;
    signalingRecoveryRef.current.inFlight = null;
  }, []);

  const isRecoveryGenerationCurrent = useCallback((generation: number): boolean => {
    const state = signalingRecoveryRef.current;
    return state.generation === generation && !state.explicitShutdown;
  }, []);

  const scheduleStableRecoveryReset = useCallback((sessionId: string): void => {
    if (stableRecoveryResetTimerRef.current !== null) {
      window.clearTimeout(stableRecoveryResetTimerRef.current);
      stableRecoveryResetTimerRef.current = null;
    }

    stableRecoveryResetTimerRef.current = window.setTimeout(() => {
      stableRecoveryResetTimerRef.current = null;
      const activeSessionId = sessionRef.current?.sessionId;
      if (
        streamStatusRef.current !== "streaming"
        || !activeSessionId
        || activeSessionId !== sessionId
      ) {
        return;
      }
      console.log(
        `[Recovery] Stream remained stable for ${SIGNALING_RECOVERY_STABLE_RESET_DELAY_MS}ms; resetting recovery budget`,
      );
      resetSignalingRecoveryState({ keepExplicitShutdown: true });
    }, SIGNALING_RECOVERY_STABLE_RESET_DELAY_MS);
  }, [resetSignalingRecoveryState]);

  const disconnectSignalingControlled = useCallback(async (): Promise<void> => {
    pendingControlledDisconnectsRef.current += 1;
    await window.openNow.disconnectSignaling().catch(() => {});
  }, []);

  // Session ref sync
  useEffect(() => {
    sessionRef.current = session;
  }, [session]);

  useEffect(() => {
    const streamIsActive = streamStatus !== "idle" || session !== null || navbarActiveSession !== null;
    if (!streamIsActive) {
      runtimeSnapshotRef.current = null;
      clearRuntimeSnapshot();
      return;
    }

    const snapshot: RuntimeSnapshot = {
      version: 1,
      updatedAt: Date.now(),
      streamStatus,
      sessionId: session?.sessionId ?? navbarActiveSession?.sessionId ?? null,
      sessionAppId:
        (Number.isFinite(signalingRecoveryRef.current.appId ?? NaN) ? signalingRecoveryRef.current.appId : null) ??
        (navbarActiveSession ? navbarActiveSession.appId : null),
      streamingGameId: streamingGame?.id ?? null,
      streamingStore: streamingStore ?? null,
      recoveryAppId: signalingRecoveryRef.current.appId,
      resumeContext: session
        ? {
          sessionId: session.sessionId,
          serverIp: session.serverIp,
          streamingBaseUrl: session.streamingBaseUrl,
          signalingServer: session.signalingServer,
          signalingUrl: session.signalingUrl,
          appId: Number.isFinite(signalingRecoveryRef.current.appId ?? NaN) ? signalingRecoveryRef.current.appId ?? undefined : undefined,
          clientId: session.clientId,
          deviceId: session.deviceId,
        }
        : (navbarActiveSession?.sessionId && navbarActiveSession.serverIp)
          ? {
            sessionId: navbarActiveSession.sessionId,
            serverIp: navbarActiveSession.serverIp,
            streamingBaseUrl: navbarActiveSession.streamingBaseUrl,
            signalingUrl: navbarActiveSession.signalingUrl,
            appId: Number.isFinite(navbarActiveSession.appId) ? navbarActiveSession.appId : undefined,
          }
          : null,
    };

    runtimeSnapshotRef.current = snapshot;
    saveRuntimeSnapshot(snapshot);
  }, [navbarActiveSession, session, streamStatus, streamingGame?.id, streamingStore]);

  const persistRuntimeSnapshotNow = useCallback((): void => {
    const latestSession = sessionRef.current;
    const latestNavbarSession = navbarActiveSession;
    const hasActiveContext =
      streamStatusRef.current !== "idle" || latestSession !== null || latestNavbarSession !== null;
    if (!hasActiveContext) {
      runtimeSnapshotRef.current = null;
      clearRuntimeSnapshot();
      return;
    }

    const snapshot: RuntimeSnapshot = {
      version: 1,
      updatedAt: Date.now(),
      streamStatus: streamStatusRef.current,
      sessionId: latestSession?.sessionId ?? latestNavbarSession?.sessionId ?? null,
      sessionAppId:
        (Number.isFinite(signalingRecoveryRef.current.appId ?? NaN) ? signalingRecoveryRef.current.appId : null) ??
        (latestNavbarSession ? latestNavbarSession.appId : null),
      streamingGameId: streamingGame?.id ?? null,
      streamingStore: streamingStore ?? null,
      recoveryAppId: signalingRecoveryRef.current.appId,
      resumeContext: latestSession
        ? {
          sessionId: latestSession.sessionId,
          serverIp: latestSession.serverIp,
          streamingBaseUrl: latestSession.streamingBaseUrl,
          signalingServer: latestSession.signalingServer,
          signalingUrl: latestSession.signalingUrl,
          appId: Number.isFinite(signalingRecoveryRef.current.appId ?? NaN) ? signalingRecoveryRef.current.appId ?? undefined : undefined,
          clientId: latestSession.clientId,
          deviceId: latestSession.deviceId,
        }
        : (latestNavbarSession?.sessionId && latestNavbarSession.serverIp)
          ? {
            sessionId: latestNavbarSession.sessionId,
            serverIp: latestNavbarSession.serverIp,
            streamingBaseUrl: latestNavbarSession.streamingBaseUrl,
            signalingUrl: latestNavbarSession.signalingUrl,
            appId: Number.isFinite(latestNavbarSession.appId) ? latestNavbarSession.appId : undefined,
          }
          : null,
    };

    runtimeSnapshotRef.current = snapshot;
    saveRuntimeSnapshot(snapshot);
  }, [navbarActiveSession, streamingGame?.id, streamingStore]);

  useEffect(() => {
    const onBeforeUnload = (): void => {
      appUnloadingRef.current = true;
      persistRuntimeSnapshotNow();
    };
    window.addEventListener("beforeunload", onBeforeUnload);
    return () => window.removeEventListener("beforeunload", onBeforeUnload);
  }, [persistRuntimeSnapshotNow]);

  useEffect(() => {
    adReportStateRef.current = {};
    adMetricsRef.current = {};
    adReportQueueRef.current = Promise.resolve();
    adMediaUrlCacheRef.current = {};
    queueAdPlaybackRef.current = null;
    activeQueueAdIdRef.current = null;
    setActiveQueueAdId(null);
    clearQueueAdWatchdogs();
  }, [clearQueueAdWatchdogs, session?.sessionId]);

  // Keep a ref copy of `streamStatus` so async callbacks can observe latest value
  useEffect(() => {
    streamStatusRef.current = streamStatus;
  }, [streamStatus]);

  // Broadcast minimal session/loading state for UI overlays (controller + other listeners)
  useEffect(() => {
    const detail = {
      status: streamStatus,
      queuePosition,
      launchError: launchError ? { title: launchError.title, description: launchError.description, stage: launchError.stage, codeLabel: launchError.codeLabel } : null,
      gameTitle: streamingGame?.title ?? null,
      gameCover: streamingGame?.imageUrl ?? null,
      platformStore: streamingStore ?? null,
    };
    try {
      window.dispatchEvent(new CustomEvent("opennow:session-update", { detail }));
    } catch {
      // ignore
    }
  }, [streamStatus, queuePosition, launchError, streamingGame, streamingStore]);

  useEffect(() => {
    document.body.classList.toggle("controller-mode", controllerUiActive);
    return () => {
      document.body.classList.remove("controller-mode");
    };
  }, [controllerUiActive]);

  useEffect(() => {
    const lowFx = streamStatus === "streaming" && controllerOverlayOpen;
    document.body.classList.toggle("controller-overlay-lowfx", lowFx);
    return () => {
      document.body.classList.remove("controller-overlay-lowfx");
    };
  }, [controllerOverlayOpen, streamStatus]);

  useEffect(() => {
    if (!controllerUiActive || !controllerConnected) {
      document.body.classList.remove("controller-hide-cursor");
      return;
    }

    const IDLE_MS = 1300;
    let timeoutId: number | null = null;

    const hideCursor = () => {
      document.body.classList.add("controller-hide-cursor");
    };

    const showCursor = () => {
      document.body.classList.remove("controller-hide-cursor");
      if (timeoutId != null) {
        window.clearTimeout(timeoutId);
      }
      timeoutId = window.setTimeout(hideCursor, IDLE_MS) as unknown as number;
    };

    const onMouseMove = (): void => showCursor();

    // Start visible then hide after timeout
    showCursor();
    document.addEventListener("mousemove", onMouseMove, { passive: true });

    return () => {
      if (timeoutId != null) {
        window.clearTimeout(timeoutId);
      }
      document.removeEventListener("mousemove", onMouseMove);
      document.body.classList.remove("controller-hide-cursor");
    };
  }, [controllerConnected, controllerUiActive]);

  // Derived state
  const selectedProvider = useMemo(() => {
    return providers.find((p) => p.idpId === providerIdpId) ?? authSession?.provider ?? null;
  }, [providers, providerIdpId, authSession]);

  const effectiveStreamingBaseUrl = useMemo(() => {
    if (settings.region.trim()) {
      return settings.region;
    }
    return selectedProvider?.streamingServiceUrl ?? "";
  }, [selectedProvider, settings.region]);

  const reportQueueAdAction = useCallback((adId: string, action: SessionAdAction, options?: QueueAdReportOptions): void => {
    const currentSession = sessionRef.current;
    if (!currentSession) {
      return;
    }

    const reportPayload = getQueueAdReportPayload(adId, action, options);

    const request = {
      token: authSession?.tokens.idToken ?? authSession?.tokens.accessToken ?? undefined,
      streamingBaseUrl: currentSession.streamingBaseUrl ?? effectiveStreamingBaseUrl,
      serverIp: currentSession.serverIp,
      zone: currentSession.zone,
      sessionId: currentSession.sessionId,
      clientId: currentSession.clientId,
      deviceId: currentSession.deviceId,
      adId,
      action,
      ...reportPayload,
    };

    adReportQueueRef.current = adReportQueueRef.current
      .catch(() => undefined)
      .then(async () => {
        if (sessionRef.current?.sessionId !== request.sessionId) {
          return;
        }

        try {
          console.log(
            `[QueueAds] Sending ad update: action=${action}, adId=${adId}, sessionId=${request.sessionId}, zone=${request.zone}, ` +
              `serverIp=${request.serverIp}, queuePosition=${sessionRef.current?.queuePosition ?? "n/a"}, ` +
              `watchedTimeInMs=${request.watchedTimeInMs ?? "n/a"}, pausedTimeInMs=${request.pausedTimeInMs ?? 0}, ` +
                `cancelReason=${request.cancelReason ?? "n/a"}, errorInfo=${request.errorInfo ?? "n/a"}`,
          );
          const updated = await window.openNow.reportSessionAd(request);
          if (sessionRef.current?.sessionId !== updated.sessionId) {
            return;
          }

          console.log(
            `[QueueAds] Ad update succeeded: action=${action}, adId=${adId}, sessionId=${updated.sessionId}, ` +
              `status=${updated.status}, queuePosition=${updated.queuePosition ?? "n/a"}`,
          );
          console.log(
            `[QueueAds] Returned ad state: sessionId=${updated.sessionId}, adsRequired=${isSessionAdsRequired(updated.adState)}, ` +
              `queuePaused=${getSessionAdOpportunity(updated.adState)?.queuePaused ?? false}, gracePeriodSeconds=${getSessionAdOpportunity(updated.adState)?.gracePeriodSeconds ?? "n/a"}, ` +
              `adCount=${getSessionAdItems(updated.adState).length}`,
          );

          setSession((previous) => {
            if (!previous || previous.sessionId !== updated.sessionId) {
              return previous;
            }
            return mergePolledSessionState(previous, updated);
          });
          setQueuePosition(updated.queuePosition);
          const updatedAd = getSessionAdItems(updated.adState).find((candidate) => candidate.adId === adId);
          const updatedAdMediaUrl = getPreferredSessionAdMediaUrl(updatedAd);
          if (updatedAdMediaUrl) {
            adMediaUrlCacheRef.current[adId] = updatedAdMediaUrl;
          }

          if (action === "finish" || action === "cancel") {
            clearQueueAdMetrics(adId);
            if (queueAdPlaybackRef.current?.adId === adId) {
              queueAdPlaybackRef.current = null;
            }
          }
        } catch (error) {
          console.warn(`[QueueAds] Failed to report ${action} for ${adId}:`, error);

          if (action === "finish") {
            // The server rejected the finish (e.g. session was re-queued mid-ad,
            // invalidating the ad token). Clear the local ad list so the player
            // falls back to the placeholder card while the server sends fresh
            // creatives in the next poll. Setting serverSentEmptyAds=false prevents
            // mergeAdState from restoring the now-stale ad URLs from the previous
            // state captured in the polling loop's latestSession snapshot.
            console.log(`[QueueAds] finish rejected — clearing local ad list for adId=${adId}`);
            delete adReportStateRef.current[adId];
            clearQueueAdMetrics(adId);
            if (queueAdPlaybackRef.current?.adId === adId) {
              queueAdPlaybackRef.current = null;
            }
            setSession((previous) => {
              if (!previous || !previous.adState) {
                return previous;
              }
              return {
                ...previous,
                adState: { ...previous.adState, sessionAds: [], ads: [], serverSentEmptyAds: false },
              };
            });
          }
        }
      });
  }, [authSession, clearQueueAdMetrics, effectiveStreamingBaseUrl, getQueueAdReportPayload]);

  const handleQueueAdPlaybackEvent = useCallback((event: QueueAdPlaybackEvent, adId: string): void => {
    const currentSession = sessionRef.current;
    const currentAd = getSessionAdItems(currentSession?.adState).find((ad) => ad.adId === adId);
    if (!currentAd) {
      return;
    }

    const lastAction = adReportStateRef.current[adId];

    if (event === "loadstart") {
      activeQueueAdIdRef.current = adId;
      setActiveQueueAdId((previous) => (previous === adId ? previous : adId));
      resetQueueAdMetrics(adId);
      queueAdPlaybackRef.current = null;
      clearQueueAdProgressWatchdog();
      armQueueAdStartWatchdogs(adId);
      return;
    }

    if (event === "timeupdate") {
      adLastProgressTsRef.current = Date.now();
      return;
    }

    if (event === "playing") {
      activeQueueAdIdRef.current = adId;
      setActiveQueueAdId((previous) => (previous === adId ? previous : adId));
      clearQueueAdStartWatchdogs();
      ensureQueueAdProgressWatchdog(adId);
      queueAdPlaybackRef.current = { adId, phase: "playing" };

      const nextAction = getNextAdReportAction(lastAction, "playing");
      if (!nextAction) {
        return;
      }
      if (nextAction === "start") {
        markQueueAdStarted(adId);
      }
      adReportStateRef.current[adId] = nextAction;
      reportQueueAdAction(adId, nextAction);
      return;
    }

    if (event === "paused") {
      if (queueAdPlaybackRef.current?.adId === adId) {
        queueAdPlaybackRef.current = null;
      }
      const nextAction = getNextAdReportAction(lastAction, "paused");
      if (nextAction) {
        markQueueAdPaused(adId);
        adReportStateRef.current[adId] = nextAction;
        reportQueueAdAction(adId, nextAction);
      }
      return;
    }

    if (event === "ended") {
      clearQueueAdWatchdogs();
      if (lastAction === "finish" || lastAction === "cancel") {
        return;
      }

      const nextAd = getNextQueueAd(currentSession?.adState, adId);
      if (nextAd) {
        activeQueueAdIdRef.current = nextAd.adId;
        setActiveQueueAdId((previous) => (previous === nextAd.adId ? previous : nextAd.adId));
      }

      queueAdPlaybackRef.current = { adId, phase: "finishing" };
      const nextAction = getNextAdReportAction(lastAction, "ended");
      if (!nextAction) {
        return;
      }
      adReportStateRef.current[adId] = nextAction;
      reportQueueAdAction(adId, nextAction);
      return;
    }

    if (event === "error") {
      clearQueueAdWatchdogs();
      if (lastAction === "finish" || lastAction === "cancel") {
        return;
      }
      queueAdPlaybackRef.current = null;
      adReportStateRef.current[adId] = "cancel";
      reportQueueAdAction(adId, "cancel", {
        cancelReason: "error",
        errorInfo: "Error loading url",
      });
    }
  }, [armQueueAdStartWatchdogs, clearQueueAdProgressWatchdog, clearQueueAdStartWatchdogs, clearQueueAdWatchdogs, ensureQueueAdProgressWatchdog, markQueueAdPaused, markQueueAdStarted, reportQueueAdAction, resetQueueAdMetrics]);

  // Keep the ref in sync so timer callbacks always use the latest auth state.
  useEffect(() => {
    reportQueueAdActionRef.current = reportQueueAdAction;
  }, [reportQueueAdAction]);

  const loadSubscriptionInfo = useCallback(
    async (session: AuthSession): Promise<void> => {
      const token = session.tokens.idToken ?? session.tokens.accessToken;
      const subscription = await window.openNow.fetchSubscription({
        token,
        providerStreamingBaseUrl: session.provider.streamingServiceUrl,
        userId: session.user.userId,
      });
      setSubscriptionInfo(subscription);
    },
    [],
  );

  const refreshSavedAccounts = useCallback(async (): Promise<SavedAccount[]> => {
    const accounts = await window.openNow.getSavedAccounts();
    setSavedAccounts(accounts);
    return accounts;
  }, []);

  const refreshNavbarActiveSession = useCallback(async (
    sessionOverride?: AuthSession,
    streamingBaseUrlOverride?: string,
  ): Promise<void> => {
    const session = sessionOverride ?? authSession;
    if (!session) {
      setNavbarActiveSession(null);
      return;
    }
    const token = session.tokens.idToken ?? session.tokens.accessToken;
    const streamingBaseUrl = streamingBaseUrlOverride
      ?? (settings.region.trim() ? effectiveStreamingBaseUrl : session.provider.streamingServiceUrl);
    if (!token || !streamingBaseUrl) {
      setNavbarActiveSession(null);
      return;
    }
    try {
      const activeSessions = await window.openNow.getActiveSessions(token, streamingBaseUrl);
      const snapshot = runtimeSnapshotRef.current;
      const resumableSessions = activeSessions.filter((entry) => entry.status === 3 || entry.status === 2);
      const candidate =
        (snapshot?.sessionId
          ? resumableSessions.find((entry) => entry.sessionId === snapshot.sessionId)
          : undefined) ??
        (snapshot?.sessionAppId !== null && snapshot?.sessionAppId !== undefined
          ? resumableSessions.find((entry) => entry.appId === snapshot.sessionAppId)
          : undefined) ??
        resumableSessions[0] ??
        null;
      setNavbarActiveSession(candidate);
    } catch (error) {
      console.warn("Failed to refresh active sessions:", error);
    }
  }, [authSession, effectiveStreamingBaseUrl, settings.region]);

  const allKnownGames = useMemo(() => [...games, ...libraryGames], [games, libraryGames]);

  const gameTitleByAppId = useMemo(() => {
    const titles = new Map<number, string>();

    for (const game of allKnownGames) {
      const idsForGame = new Set<number>();
      const launchId = parseNumericId(game.launchAppId);
      if (launchId !== null) {
        idsForGame.add(launchId);
      }
      for (const variant of game.variants) {
        const variantId = parseNumericId(variant.id);
        if (variantId !== null) {
          idsForGame.add(variantId);
        }
      }
      for (const appId of idsForGame) {
        if (!titles.has(appId)) {
          titles.set(appId, game.title);
        }
      }
    }

    return titles;
  }, [allKnownGames]);

  const findGameContextForSession = useCallback((activeSession: ActiveSessionInfo) => {
    return findSessionContextForAppId(allKnownGames, variantByGameId, activeSession.appId);
  }, [allKnownGames, variantByGameId]);

  const stopSessionByTarget = useCallback(async (
    target: Pick<SessionStopRequest, "sessionId" | "zone" | "streamingBaseUrl" | "serverIp" | "clientId" | "deviceId"> | null | undefined,
  ): Promise<boolean> => {
    if (!target) {
      return false;
    }
    const token = authSession?.tokens.idToken ?? authSession?.tokens.accessToken;
    if (!token) {
      console.warn("Skipping session stop: missing auth token");
      return false;
    }
    await window.openNow.stopSession({
      token,
      streamingBaseUrl: target.streamingBaseUrl,
      serverIp: target.serverIp,
      zone: target.zone,
      sessionId: target.sessionId,
      clientId: target.clientId,
      deviceId: target.deviceId,
    });
    return true;
  }, [authSession]);

  useEffect(() => {
    if (!startupRefreshNotice) return;
    const timer = window.setTimeout(() => setStartupRefreshNotice(null), 7000);
    return () => window.clearTimeout(timer);
  }, [startupRefreshNotice]);

  useEffect(() => {
    if (!authSession || streamStatus !== "idle") {
      return;
    }
    void refreshNavbarActiveSession();
    const timer = window.setInterval(() => {
      void refreshNavbarActiveSession();
    }, 10000);
    return () => window.clearInterval(timer);
  }, [authSession, refreshNavbarActiveSession, streamStatus]);

  useEffect(() => {
    saveStoredCodecResults(codecResults);
  }, [codecResults]);

  useEffect(() => {
    saveCatalogPreferences({ sortId: catalogSelectedSortId, filterIds: catalogSelectedFilterIds });
  }, [catalogSelectedSortId, catalogSelectedFilterIds]);

  useEffect(() => {
    if (codecResults || codecTesting || codecStartupTestAttemptedRef.current) {
      return;
    }
    codecStartupTestAttemptedRef.current = true;
    void runCodecTest();
  }, [codecResults, codecTesting, runCodecTest]);

  // Auto-load controller library at startup if enabled
  useEffect(() => {
    if (isInitializing || !authSession || !settings.controllerMode || !settings.autoLoadControllerLibrary || currentPage !== "home") {
      return;
    }
    setCurrentPage("library");
  }, [isInitializing, authSession, settings.controllerMode, settings.autoLoadControllerLibrary, currentPage]);

  const shortcuts = useMemo(() => {
    const parseWithFallback = (value: string, fallback: string) => {
      const parsed = normalizeShortcut(value);
      return parsed.valid ? parsed : normalizeShortcut(fallback);
    };
    const toggleStats = parseWithFallback(settings.shortcutToggleStats, DEFAULT_SHORTCUTS.shortcutToggleStats);
    const togglePointerLock = parseWithFallback(settings.shortcutTogglePointerLock, DEFAULT_SHORTCUTS.shortcutTogglePointerLock);
    const toggleFullscreen = parseWithFallback(settings.shortcutToggleFullscreen, DEFAULT_SHORTCUTS.shortcutToggleFullscreen);
    const stopStream = parseWithFallback(settings.shortcutStopStream, DEFAULT_SHORTCUTS.shortcutStopStream);
    const toggleAntiAfk = parseWithFallback(settings.shortcutToggleAntiAfk, DEFAULT_SHORTCUTS.shortcutToggleAntiAfk);
    const toggleMicrophone = parseWithFallback(settings.shortcutToggleMicrophone, DEFAULT_SHORTCUTS.shortcutToggleMicrophone);
    const screenshot = parseWithFallback(settings.shortcutScreenshot, DEFAULT_SHORTCUTS.shortcutScreenshot);
    const recording = parseWithFallback(settings.shortcutToggleRecording, DEFAULT_SHORTCUTS.shortcutToggleRecording);
    return { toggleStats, togglePointerLock, toggleFullscreen, stopStream, toggleAntiAfk, toggleMicrophone, screenshot, recording };
  }, [
    settings.shortcutToggleStats,
    settings.shortcutTogglePointerLock,
    settings.shortcutToggleFullscreen,
    settings.shortcutStopStream,
    settings.shortcutToggleAntiAfk,
    settings.shortcutToggleMicrophone,
    settings.shortcutScreenshot,
    settings.shortcutToggleRecording,
  ]);

  const setSessionFullscreen = useCallback(async (nextFullscreen: boolean) => {
    const canUseNativeFullscreen = typeof window.openNow?.setFullscreen === "function";

    if (canUseNativeFullscreen) {
      try {
        if (nextFullscreen) {
          if (!document.fullscreenElement) {
            await document.documentElement.requestFullscreen();
          }
        } else if (document.fullscreenElement) {
          await document.exitFullscreen();
        }
      } catch (error) {
        console.warn(`Failed to set DOM fullscreen state (${nextFullscreen ? "enter" : "exit"}):`, error);
      }

      try {
        await window.openNow.setFullscreen(nextFullscreen);
        setSessionFullscreenState(nextFullscreen);
      } catch (error) {
        console.warn(`Failed to set native fullscreen state (${nextFullscreen ? "enter" : "exit"}):`, error);
      }
      return;
    }

    try {
      if (nextFullscreen) {
        if (!document.fullscreenElement) {
          await document.documentElement.requestFullscreen();
        }
      } else if (document.fullscreenElement) {
        await document.exitFullscreen();
      }
    } catch {}

    setSessionFullscreenState(!!document.fullscreenElement);
  }, []);

  const toggleSessionFullscreen = useCallback(async () => {
    await setSessionFullscreen(!(sessionFullscreen || document.fullscreenElement));
  }, [sessionFullscreen, setSessionFullscreen]);

  useEffect(() => {
    const handleFullscreenChange = () => {
      if (typeof window.openNow?.setFullscreen === "function") {
        return;
      }
      setSessionFullscreenState(!!document.fullscreenElement);
    };

    document.addEventListener("fullscreenchange", handleFullscreenChange);
    return () => document.removeEventListener("fullscreenchange", handleFullscreenChange);
  }, []);

  const requestPointerLockCapture = useCallback(async (target: HTMLVideoElement) => {
    const lockTarget = (target.parentElement as HTMLElement | null) ?? target;
    const requestPointerLockCompat = async (
      options?: { unadjustedMovement?: boolean },
    ): Promise<void> => {
      const maybePromise = lockTarget.requestPointerLock(options as any) as unknown;
      if (maybePromise && typeof (maybePromise as Promise<void>).then === "function") {
        await (maybePromise as Promise<void>);
      }
    };

    if (settings.autoFullScreen && !(sessionFullscreen || document.fullscreenElement)) {
      await setSessionFullscreen(true);
    }

    await requestPointerLockCompat({ unadjustedMovement: true })
      .catch((err: DOMException) => {
        if (err.name === "NotSupportedError") {
          return requestPointerLockCompat();
        }
        throw err;
      })
      .catch(() => {});
  }, [sessionFullscreen, setSessionFullscreen, settings.autoFullScreen]);

  const handleRequestPointerLock = useCallback(() => {
    if (videoRef.current) {
      void requestPointerLockCapture(videoRef.current);
    }
  }, [requestPointerLockCapture]);

  const resolveExitPrompt = useCallback((confirmed: boolean) => {
    const resolver = exitPromptResolverRef.current;
    exitPromptResolverRef.current = null;
    setExitPrompt((prev) => (prev.open ? { ...prev, open: false } : prev));
    resolver?.(confirmed);
  }, []);

  const requestExitPrompt = useCallback((gameTitle: string): Promise<boolean> => {
    return new Promise((resolve) => {
      if (exitPromptResolverRef.current) {
        // Close any previous pending prompt to avoid dangling promises.
        exitPromptResolverRef.current(false);
      }
      exitPromptResolverRef.current = resolve;
      setExitPrompt({
        open: true,
        gameTitle: gameTitle || "this game",
      });
    });
  }, []);

  const handleExitPromptConfirm = useCallback(() => {
    resolveExitPrompt(true);
  }, [resolveExitPrompt]);

  const handleExitPromptCancel = useCallback(() => {
    resolveExitPrompt(false);
  }, [resolveExitPrompt]);

  useEffect(() => {
    return () => {
      if (exitPromptResolverRef.current) {
        exitPromptResolverRef.current(false);
        exitPromptResolverRef.current = null;
      }
    };
  }, []);

  // Listen for fullscreen toggle from main process.
  useEffect(() => {
    const unsubscribe = window.openNow.onToggleFullscreen(() => {
      void toggleSessionFullscreen();
    });
    return () => unsubscribe();
  }, [toggleSessionFullscreen]);

  const autoFullscreenRequestedRef = useRef(false);

  useEffect(() => {
    const isSessionConnecting = streamStatus === "connecting" || streamStatus === "streaming";
    const isNativeStreamerSession = settings.streamClientMode === "native" || nativeStreamingRef.current;
    if (!settings.autoFullScreen || !isSessionConnecting || isNativeStreamerSession) {
      autoFullscreenRequestedRef.current = false;
      return;
    }

    if (autoFullscreenRequestedRef.current || sessionFullscreen || document.fullscreenElement) {
      return;
    }

    autoFullscreenRequestedRef.current = true;
    void setSessionFullscreen(true);
  }, [sessionFullscreen, setSessionFullscreen, settings.autoFullScreen, settings.streamClientMode, streamStatus]);

  // Anti-AFK interval
  useEffect(() => {
    if (!isStreaming) {
      setAntiAfkAckNonce(0);
    }
  }, [isStreaming]);

  useEffect(() => {
    if (!antiAfkEnabled || streamStatus !== "streaming") return;

    const interval = window.setInterval(() => {
      clientRef.current?.sendAntiAfkPulse();
    }, 240000); // 4 minutes

    return () => clearInterval(interval);
  }, [antiAfkEnabled, streamStatus]);

  // Periodically re-sync subscription playtime from backend while streaming.
  useEffect(() => {
    if (streamStatus !== "streaming" || !authSession) {
      return;
    }

    let cancelled = false;

    const syncPlaytime = async (): Promise<void> => {
      try {
        await loadSubscriptionInfo(authSession);
      } catch (error) {
        if (!cancelled) {
          console.warn("Failed to re-sync subscription playtime:", error);
        }
      }
    };

    void syncPlaytime();
    const timer = window.setInterval(() => {
      void syncPlaytime();
    }, PLAYTIME_RESYNC_INTERVAL_MS);

    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [authSession, loadSubscriptionInfo, streamStatus]);

  // Restore focus to video element when navigating away from Settings during streaming
  useEffect(() => {
    if (streamStatus === "streaming" && currentPage !== "settings" && videoRef.current) {
      // Small delay to let React finish rendering the new page
      const timer = window.setTimeout(() => {
        if (videoRef.current && document.activeElement !== videoRef.current) {
          videoRef.current.focus();
          console.log("[App] Restored focus to video element after leaving Settings");
        }
      }, 50);
      return () => clearTimeout(timer);
    }
  }, [currentPage, streamStatus]);


  useEffect(() => {
    if (streamStatus !== "streaming" || sessionStartedAtMs !== null) {
      return;
    }

    const evaluate = () => {
      const snapshot = diagnosticsStore.getSnapshot();
      const hasLiveFrames =
        snapshot.framesDecoded > 0 || snapshot.framesReceived > 0 || snapshot.renderFps > 0;
      if (hasLiveFrames) {
        setSessionStartedAtMs(Date.now());
      }
    };

    evaluate();
    const unsubscribe = diagnosticsStore.subscribe(evaluate);
    return unsubscribe;
  }, [sessionStartedAtMs, streamStatus]);

  useEffect(() => {
    if (freeTierSessionRemainingSeconds === null) {
      previousFreeTierRemainingSecondsRef.current = null;
      setLocalSessionTimerWarning(null);
      return;
    }

    const previousSeconds = previousFreeTierRemainingSecondsRef.current;

    if (hasCrossedWarningThreshold(previousSeconds, freeTierSessionRemainingSeconds, FREE_TIER_FINAL_MINUTE_WARNING_SECONDS)) {
      setLocalSessionTimerWarning({ stage: "free-tier-final-minute", shownAtMs: Date.now() });
    } else if (hasCrossedWarningThreshold(previousSeconds, freeTierSessionRemainingSeconds, FREE_TIER_15_MIN_WARNING_SECONDS)) {
      setLocalSessionTimerWarning({ stage: "free-tier-15m", shownAtMs: Date.now() });
    } else if (hasCrossedWarningThreshold(previousSeconds, freeTierSessionRemainingSeconds, FREE_TIER_30_MIN_WARNING_SECONDS)) {
      setLocalSessionTimerWarning({ stage: "free-tier-30m", shownAtMs: Date.now() });
    }

    previousFreeTierRemainingSecondsRef.current = freeTierSessionRemainingSeconds;
  }, [freeTierSessionRemainingSeconds]);

  useEffect(() => {
    if (!localSessionTimerWarning) return;

    const warning = localSessionTimerWarning;
    const remainingMs = Math.max(0, warning.shownAtMs + STREAM_WARNING_VISIBILITY_MS - Date.now());
    const timer = window.setTimeout(() => {
      setLocalSessionTimerWarning((current) => (current === warning ? null : current));
    }, remainingMs);
    return () => window.clearTimeout(timer);
  }, [localSessionTimerWarning]);

  useEffect(() => {
    if (!remoteStreamWarning) return;

    const warning = remoteStreamWarning;
    const timer = window.setTimeout(() => {
      setRemoteStreamWarning((current) => (current === warning ? null : current));
    }, STREAM_WARNING_VISIBILITY_MS);
    return () => window.clearTimeout(timer);
  }, [remoteStreamWarning]);

  // Save settings when changed
  const updateSetting = useCallback(async <K extends keyof Settings>(key: K, value: Settings[K]) => {
    setSettings((prev) => ({ ...prev, [key]: value }));
    if (settingsLoaded) {
      await window.openNow.setSetting(key, value);
    }
    // If a running client exists, push certain settings live
    if (key === "mouseSensitivity") {
      try {
        (clientRef.current as any)?.setMouseSensitivity?.(value as number);
      } catch {
        // ignore
      }
    }
    if (key === "mouseAcceleration") {
      try {
        (clientRef.current as any)?.setMouseAccelerationPercent?.(value as number);
      } catch {
        // ignore
      }
    }
    if (key === "autoFullScreen") {
      try {
        (clientRef.current as any)?.setAutoFullScreen?.(value as boolean);
      } catch {
        // ignore
      }
    }
    if (key === "maxBitrateMbps") {
      try {
        void (clientRef.current as any)?.setMaxBitrateKbps?.((value as number) * 1000);
      } catch {
        // ignore
      }
    }
  }, [settingsLoaded]);

  const handleStreamVolumeChange = useCallback((v: number) => {
    const n = Math.max(0, Math.min(1, v));
    setStreamVolume(n);
  }, []);

  const handleToggleStreamMicrophone = useCallback(() => {
    clientRef.current?.toggleMicrophone();
  }, []);

  const handleStreamMicLevelChange = useCallback((level: number) => {
    const next = Math.max(0, Math.min(1, Number.isFinite(level) ? level : 1));
    setStreamMicLevel(next);
    clientRef.current?.setMicrophoneLevel(next);
  }, []);

  const handleMouseSensitivityChange = useCallback((value: number) => {
    void updateSetting("mouseSensitivity", value);
  }, [updateSetting]);

  const handleToggleFavoriteGame = useCallback((gameId: string): void => {
    const favorites = settings.favoriteGameIds;
    const exists = favorites.includes(gameId);
    const next = exists ? favorites.filter((id) => id !== gameId) : [...favorites, gameId];
    void updateSetting("favoriteGameIds", next);
  }, [settings.favoriteGameIds, updateSetting]);

  const handleMouseAccelerationChange = useCallback((value: number) => {
    void updateSetting("mouseAcceleration", value);
  }, [updateSetting]);

  const handleExitControllerMode = useCallback(() => {
    setSettings((prev) => ({
      ...prev,
      controllerMode: false,
      autoLoadControllerLibrary: false,
    }));

    if (settingsLoaded) {
      void Promise.all([
        window.openNow.setSetting("controllerMode", false),
        window.openNow.setSetting("autoLoadControllerLibrary", false),
      ]).catch((error) => {
        console.warn("Failed to persist controller mode exit settings:", error);
      });
    }
  }, [settingsLoaded]);

  const handleExitApp = useCallback(() => {
    appUnloadingRef.current = true;
    persistRuntimeSnapshotNow();
    void window.openNow.quitApp().catch((error) => {
      console.warn("Failed to quit application:", error);
    });
  }, [persistRuntimeSnapshotNow]);

  const handleMicrophoneModeChange = useCallback((value: import("@shared/gfn").MicrophoneMode) => {
    // Keep UI responsive while still surfacing persistence failures.
    void updateSetting("microphoneMode", value).catch((error) => {
      console.warn("Failed to persist microphone mode setting:", error);
    });
  }, [updateSetting]);

  const applyCatalogBrowseResult = useCallback((catalogResult: CatalogBrowseResult): void => {
    setGames(catalogResult.games);
    setCatalogFilterGroups(catalogResult.filterGroups);
    setCatalogSortOptions(catalogResult.sortOptions);
    setCatalogSelectedSortId((previous) => previous === catalogResult.selectedSortId ? previous : catalogResult.selectedSortId);
    setCatalogSelectedFilterIds((previous) => areStringArraysEqual(previous, catalogResult.selectedFilterIds) ? previous : catalogResult.selectedFilterIds);
    setCatalogTotalCount(catalogResult.totalCount);
    setCatalogSupportedCount(catalogResult.numberSupported);
    setSelectedGameId((previous) => catalogResult.games.some((game) => game.id === previous) ? previous : (catalogResult.games[0]?.id ?? ""));
    applyVariantSelections(catalogResult.games);
  }, [applyVariantSelections]);

  const loadSessionRuntimeData = useCallback(async (session: AuthSession): Promise<void> => {
    const token = session.tokens.idToken ?? session.tokens.accessToken;
    const discovered = await window.openNow.getRegions({ token });
    setRegions(discovered);

    try {
      await loadSubscriptionInfo(session);
    } catch (error) {
      console.warn("Failed to load subscription info:", error);
      setSubscriptionInfo(null);
    }

    try {
      const [catalogResult, libGames] = await Promise.all([
        window.openNow.browseCatalog({
          token,
          providerStreamingBaseUrl: session.provider.streamingServiceUrl,
          searchQuery: "",
          sortId: catalogSelectedSortId,
          filterIds: catalogSelectedFilterIds,
        }),
        window.openNow.fetchLibraryGames({
          token,
          providerStreamingBaseUrl: session.provider.streamingServiceUrl,
        }),
      ]);
      applyCatalogBrowseResult(catalogResult);
      setLibraryGames(libGames);
      applyVariantSelections(libGames);
    } catch (catalogError) {
      console.error("Initialization games load failed:", catalogError);
      setGames([]);
      setLibraryGames([]);
      setCatalogFilterGroups([]);
      setCatalogSortOptions([]);
      setCatalogTotalCount(0);
      setCatalogSupportedCount(0);
    }
  }, [
    applyCatalogBrowseResult,
    applyVariantSelections,
    catalogSelectedFilterIds,
    catalogSelectedSortId,
    loadSubscriptionInfo,
  ]);

  // Initialize app
  useEffect(() => {
    if (hasInitializedRef.current) return;
    hasInitializedRef.current = true;

    const initialize = async () => {
      try {
        // Load settings first
        const loadedSettings = await window.openNow.getSettings();
        setSettings(loadedSettings);
        setShowStatsOverlay(loadedSettings.showStatsOnLaunch);
        setSettingsLoaded(true);

        // Load providers and session (refresh only if token is near expiry)
        setStartupStatusMessage("Restoring saved session...");
        const [providerList, sessionResult] = await Promise.all([
          window.openNow.getLoginProviders(),
          window.openNow.getAuthSession(),
        ]);
        const accounts = await window.openNow.getSavedAccounts();
        const persistedSession = sessionResult.session;

        if (sessionResult.refresh.outcome === "refreshed") {
          setStartupRefreshNotice({
            tone: "success",
            text: "Session restored. Token refreshed.",
          });
          setStartupStatusMessage("Token refreshed. Loading your account...");
        } else if (sessionResult.refresh.outcome === "failed") {
          setStartupRefreshNotice({
            tone: "warn",
            text: "Token refresh failed. Using saved session token.",
          });
          setStartupStatusMessage("Token refresh failed. Continuing with saved session...");
        } else if (sessionResult.refresh.outcome === "missing_refresh_token") {
          setStartupStatusMessage("Saved session has no refresh token. Continuing...");
        } else if (persistedSession) {
          setStartupStatusMessage("Session restored.");
        } else {
          setStartupStatusMessage("No saved session found.");
        }

        // Load persisted variant selections from localStorage before applying defaults
        try {
          const raw = localStorage.getItem(VARIANT_SELECTION_LOCALSTORAGE_KEY);
          if (raw) {
            const parsed = JSON.parse(raw);
            if (parsed && typeof parsed === "object") {
              setVariantByGameId(parsed as Record<string, string>);
            }
          }
        } catch (e) {
          // ignore parse/storage errors
        }

        const persistedRuntimeSnapshot = loadRuntimeSnapshot();
        runtimeSnapshotRef.current = persistedRuntimeSnapshot;
        if (persistedRuntimeSnapshot?.recoveryAppId !== null && persistedRuntimeSnapshot?.recoveryAppId !== undefined) {
          signalingRecoveryRef.current.appId = persistedRuntimeSnapshot.recoveryAppId;
        }

        setProviders(providerList);
        setAuthSession(persistedSession);
        setSavedAccounts(accounts);

        const activeProviderId = persistedSession?.provider?.idpId ?? providerList[0]?.idpId ?? "";
        setProviderIdpId(activeProviderId);

        if (persistedSession) {
          await loadSessionRuntimeData(persistedSession);
        } else {
          setRegions([]);
          setGames([]);
          setLibraryGames([]);
          setSubscriptionInfo(null);
          setCatalogFilterGroups([]);
          setCatalogSortOptions([]);
          setCatalogTotalCount(0);
          setCatalogSupportedCount(0);
        }

        setIsInitializing(false);
      } catch (error) {
        console.error("Initialization failed:", error);
        setStartupStatusMessage("Session restore failed. Please sign in again.");
        // Always set isInitializing to false even on error
        setIsInitializing(false);
      }
    };

    void initialize();
  }, [loadSessionRuntimeData]);

  // Login handler
  const handleLogin = useCallback(async () => {
    setIsLoggingIn(true);
    setLoginError(null);
    try {
      const session = await window.openNow.login({ providerIdpId: providerIdpId || undefined });
      setAuthSession(session);
      setProviderIdpId(session.provider.idpId);
      await refreshSavedAccounts();
      await loadSessionRuntimeData(session);
    } catch (error) {
      setLoginError(error instanceof Error ? error.message : "Login failed");
    } finally {
      setIsLoggingIn(false);
    }
  }, [loadSessionRuntimeData, providerIdpId, refreshSavedAccounts]);

  const handleSwitchAccount = useCallback(async (userId: string) => {
    try {
      const session = await window.openNow.switchAccount(userId);
      setAuthSession(session);
      setProviderIdpId(session.provider.idpId);
      await refreshSavedAccounts();
      await loadSessionRuntimeData(session);
      await refreshNavbarActiveSession(session);
    } catch (error) {
      console.warn("Failed to switch account:", error);
      setLoginError(error instanceof Error ? error.message : "Failed to switch account");
      try {
        await refreshSavedAccounts();
        const sessionResult = await window.openNow.getAuthSession();
        setAuthSession(sessionResult.session);
        if (sessionResult.session) {
          setProviderIdpId(sessionResult.session.provider.idpId);
          await loadSessionRuntimeData(sessionResult.session);
          await refreshNavbarActiveSession(sessionResult.session);
        } else {
          setRegions([]);
          setGames([]);
          setLibraryGames([]);
          setSubscriptionInfo(null);
          setNavbarActiveSession(null);
          setCatalogFilterGroups([]);
          setCatalogSortOptions([]);
          setCatalogTotalCount(0);
          setCatalogSupportedCount(0);
        }
      } catch (recoveryError) {
        console.warn("Failed to recover account state after switch failure:", recoveryError);
      }
    }
  }, [loadSessionRuntimeData, refreshNavbarActiveSession, refreshSavedAccounts]);

  const handleRemoveAccount = useCallback((userId: string) => {
    setAccountToRemove(userId);
    setRemoveAccountConfirmOpen(true);
  }, []);

  const confirmRemoveAccount = useCallback(async () => {
    if (!accountToRemove) return;
    const targetUserId = accountToRemove;
    setRemoveAccountConfirmOpen(false);
    setAccountToRemove(null);

    await window.openNow.removeAccount(targetUserId);
    const [accounts, sessionResult] = await Promise.all([
      window.openNow.getSavedAccounts(),
      window.openNow.getAuthSession(),
    ]);
    setSavedAccounts(accounts);
    setAuthSession(sessionResult.session);
    if (sessionResult.session) {
      setProviderIdpId(sessionResult.session.provider.idpId);
      await loadSessionRuntimeData(sessionResult.session);
      await refreshNavbarActiveSession(sessionResult.session);
      return;
    }
    setRegions([]);
    setGames([]);
    setLibraryGames([]);
    setSubscriptionInfo(null);
    setNavbarActiveSession(null);
    setCatalogFilterGroups([]);
    setCatalogSortOptions([]);
    setCatalogTotalCount(0);
    setCatalogSupportedCount(0);
  }, [accountToRemove, loadSessionRuntimeData, refreshNavbarActiveSession]);

  const handleAddAccount = useCallback(() => {
    setAuthSession(null);
    setLoginError(null);
  }, []);

  const confirmLogout = useCallback(async () => {
    setLogoutConfirmOpen(false);
    await window.openNow.logoutAll();
    setAuthSession(null);
    setSavedAccounts([]);
    setGames([]);
    setLibraryGames([]);
    setVariantByGameId({});
    resetLaunchRuntime();
    setNavbarActiveSession(null);
    setIsResumingNavbarSession(false);
    setSubscriptionInfo(null);
    setCurrentPage("home");
    setCatalogFilterGroups([]);
    setCatalogSortOptions([]);
    setCatalogSelectedSortId("relevance");
    setCatalogSelectedFilterIds([]);
    setCatalogTotalCount(0);
    setCatalogSupportedCount(0);
    setSelectedGameId("");
  }, [resetLaunchRuntime]);

  // Logout handler
  const handleLogout = useCallback(() => {
    setLogoutConfirmOpen(true);
  }, []);

  // Load games handler
  const loadGames = useCallback(async (targetSource: "main" | "library") => {
    setIsLoadingGames(true);
    try {
      const token = authSession?.tokens.idToken ?? authSession?.tokens.accessToken;
      const baseUrl = effectiveStreamingBaseUrl;
      if (!token) {
        return;
      }

      if (targetSource === "main") {
        const catalogResult = await window.openNow.browseCatalog({
          token,
          providerStreamingBaseUrl: baseUrl,
          searchQuery,
          sortId: catalogSelectedSortId,
          filterIds: catalogSelectedFilterIds,
        });
        applyCatalogBrowseResult(catalogResult);
        return;
      }

      const result = await window.openNow.fetchLibraryGames({ token, providerStreamingBaseUrl: baseUrl });
      setLibraryGames(result);
      setSelectedGameId((previous) => result.some((game) => game.id === previous) ? previous : (result[0]?.id ?? ""));
      applyVariantSelections(result);
    } catch (error) {
      console.error("Failed to load games:", error);
    } finally {
      setIsLoadingGames(false);
    }
  }, [applyCatalogBrowseResult, applyVariantSelections, authSession, effectiveStreamingBaseUrl, searchQuery, catalogFilterKey, catalogSelectedSortId]);

  useEffect(() => {
    if (!authSession || currentPage !== "home") {
      return;
    }
    const handle = window.setTimeout(() => {
      void loadGames("main");
    }, searchQuery.trim() ? 220 : 0);
    return () => window.clearTimeout(handle);
  }, [authSession, currentPage, loadGames, searchQuery, catalogFilterKey, catalogSelectedSortId]);

  const handleSelectGameVariant = useCallback((gameId: string, variantId: string): void => {
    setVariantByGameId((prev) => {
      if (prev[gameId] === variantId) {
        return prev;
      }
      const next = { ...prev, [gameId]: variantId };
      try {
        localStorage.setItem(VARIANT_SELECTION_LOCALSTORAGE_KEY, JSON.stringify(next));
      } catch (e) {
        // ignore storage errors
      }
      return next;
    });
  }, []);

  const resolveSessionClaimAppId = useCallback((existingSession: ActiveSessionInfo): string => {
    const trackedAppId = signalingRecoveryRef.current.appId;
    const persistedAppId = runtimeSnapshotRef.current?.sessionAppId ?? runtimeSnapshotRef.current?.recoveryAppId;
    if (Number.isFinite(existingSession.appId) && existingSession.appId > 0) {
      return String(existingSession.appId);
    }
    if (trackedAppId && Number.isFinite(trackedAppId)) {
      return String(trackedAppId);
    }
    if (persistedAppId && Number.isFinite(persistedAppId)) {
      return String(persistedAppId);
    }
    throw new Error("Active session is missing app metadata required for resume.");
  }, []);

  const resolveResumeIdentity = useCallback((sessionId: string): { clientId?: string; deviceId?: string } => {
    const liveSession = sessionRef.current;
    if (liveSession?.sessionId === sessionId) {
      return {
        clientId: liveSession.clientId,
        deviceId: liveSession.deviceId,
      };
    }
    const persisted = runtimeSnapshotRef.current?.resumeContext;
    if (persisted?.sessionId === sessionId) {
      return {
        clientId: persisted.clientId,
        deviceId: persisted.deviceId,
      };
    }
    return {};
  }, []);

  const applyClaimedSessionAndConnect = useCallback(async (
    claimed: SessionInfo,
    expectedRecoveryGeneration?: number,
  ): Promise<void> => {
    const canProceedWithClaimedReconnect = (): boolean => {
      if (
        expectedRecoveryGeneration !== undefined
        && !isRecoveryGenerationCurrent(expectedRecoveryGeneration)
      ) {
        return false;
      }
      if (signalingRecoveryRef.current.explicitShutdown) {
        return false;
      }
      return true;
    };

    if (
      expectedRecoveryGeneration !== undefined
      && !isRecoveryGenerationCurrent(expectedRecoveryGeneration)
    ) {
      console.log("[Recovery] Skipping claimed session apply due to stale recovery generation");
      return;
    }

    console.log("Claimed session:", {
      sessionId: claimed.sessionId,
      signalingServer: claimed.signalingServer,
      signalingUrl: claimed.signalingUrl,
      status: claimed.status,
    });

    await sleep(1000);
    if (
      expectedRecoveryGeneration !== undefined
      && !isRecoveryGenerationCurrent(expectedRecoveryGeneration)
    ) {
      console.log("[Recovery] Skipping reconnect due to stale recovery generation after delay");
      return;
    }
    if (!canProceedWithClaimedReconnect()) {
      console.log("[Recovery] Skipping claimed session apply due to explicit shutdown");
      return;
    }

    // Mirror attemptSessionRecovery: tear down WebRTC + signaling before connecting to a new edge.
    // Avoids stale PeerConnection/video vs migrated CloudMatch connectionInfo (intermittent black screen on resume).
    const reconnectSource = expectedRecoveryGeneration !== undefined ? "recovery" : "resume";
    console.log(`[Stream] ${reconnectSource}: teardown WebRTC + signaling before reconnect`, {
      sessionId: claimed.sessionId,
      signalingServer: claimed.signalingServer,
      signalingUrl: claimed.signalingUrl,
      mediaConnectionInfo: claimed.mediaConnectionInfo,
    });
    clientRef.current?.dispose();
    clientRef.current = null;
    await disconnectSignalingControlled();
    awaitingRecoveryRemoteIceRef.current = expectedRecoveryGeneration !== undefined;

    setSession(claimed);
    sessionRef.current = claimed;
    nativeInputProtocolVersionRef.current = null;
    setQueuePosition(undefined);
    setLaunchError(null);
    setStreamStatus("connecting");
    await window.openNow.connectSignaling(buildSignalingConnectRequest(claimed));
  }, [buildSignalingConnectRequest, disconnectSignalingControlled, isRecoveryGenerationCurrent]);

  const claimAndConnectSession = useCallback(async (existingSession: ActiveSessionInfo): Promise<void> => {
    const sid = existingSession.sessionId;
    const inflight = claimResumePromisesRef.current.get(sid);
    if (inflight) {
      console.log("[Resume] claimAndConnectSession: deduped — joining in-flight claim for session", sid);
      await inflight;
      return;
    }

    const resumePromiseHolder: { promise?: Promise<void> } = {};
    resumePromiseHolder.promise = (async (): Promise<void> => {
      try {
        const token = authSession?.tokens.idToken ?? authSession?.tokens.accessToken;
        if (!token) {
          throw new Error("Missing token for session resume");
        }
        if (!existingSession.serverIp) {
          throw new Error("Active session is missing server address. Start the game again to create a new session.");
        }

        console.log("[Resume] claimAndConnectSession: invoking claimSession", {
          sessionId: existingSession.sessionId,
          serverIp: existingSession.serverIp,
          status: existingSession.status,
          appId: existingSession.appId,
        });

        const matchedContext = findGameContextForSession(existingSession);
        if (matchedContext) {
          setStreamingGame(matchedContext.game);
          setStreamingStore(matchedContext.variant?.store ?? null);
        } else {
          setStreamingStore(null);
        }

        const claimed = await window.openNow.claimSession({
          token,
          streamingBaseUrl: effectiveStreamingBaseUrl,
          serverIp: existingSession.serverIp,
          sessionId: existingSession.sessionId,
          ...resolveResumeIdentity(existingSession.sessionId),
          appId: resolveSessionClaimAppId(existingSession),
          settings: buildCurrentStreamSettings(),
        });

        await applyClaimedSessionAndConnect(claimed);
      } finally {
        const map = claimResumePromisesRef.current;
        const p = resumePromiseHolder.promise;
        if (p && map.get(sid) === p) {
          map.delete(sid);
        }
      }
    })();

    claimResumePromisesRef.current.set(sid, resumePromiseHolder.promise);
    await resumePromiseHolder.promise;
  }, [applyClaimedSessionAndConnect, authSession, buildCurrentStreamSettings, effectiveStreamingBaseUrl, findGameContextForSession, resolveResumeIdentity, resolveSessionClaimAppId]);

  const attemptSessionRecovery = useCallback(async (reason: string): Promise<boolean> => {
    const recoveryState = signalingRecoveryRef.current;
    const recoveryGeneration = recoveryState.generation;
    const currentStatus = streamStatusRef.current;
    const currentSession = sessionRef.current;

    if (!isRecoveryGenerationCurrent(recoveryGeneration)) {
      console.log("[Recovery] Skipping signaling recovery after explicit shutdown");
      return false;
    }
    if (!RECOVERABLE_STREAM_STATUSES.includes(currentStatus)) {
      console.log("[Recovery] Stream status is not recoverable:", currentStatus);
      return false;
    }
    if (!currentSession) {
      console.warn("[Recovery] No active session available for signaling recovery");
      return false;
    }
    if (recoveryState.inFlight) {
      console.log("[Recovery] Reusing in-flight signaling recovery attempt");
      return recoveryState.inFlight;
    }

    const token = authSession?.tokens.idToken ?? authSession?.tokens.accessToken;
    if (!token) {
      throw new Error("Connection to the running session was lost and your login token is no longer available for resume.");
    }

    if (recoveryState.attemptCount >= SIGNALING_RECOVERY_ATTEMPT_DELAYS_MS.length) {
      console.warn("[Recovery] Recovery budget exhausted");
      return false;
    }

    const attemptPromise = (async (): Promise<boolean> => {
      clientRef.current?.dispose();
      clientRef.current = null;
      setStreamStatus("connecting");
      await disconnectSignalingControlled();

      let lastError: Error | null = null;
      while (recoveryState.attemptCount < SIGNALING_RECOVERY_ATTEMPT_DELAYS_MS.length) {
        const attemptIndex = recoveryState.attemptCount;
        recoveryState.attemptCount += 1;
        const attemptNumber = recoveryState.attemptCount;
        const attemptDelayMs = SIGNALING_RECOVERY_ATTEMPT_DELAYS_MS[attemptIndex] ?? 0;

        console.warn(
          `[Recovery] Attempt ${attemptNumber}/${SIGNALING_RECOVERY_ATTEMPT_DELAYS_MS.length} after signaling disconnect: ${reason}`,
        );

        if (attemptDelayMs > 0) {
          await sleep(attemptDelayMs);
        }
        if (!isRecoveryGenerationCurrent(recoveryGeneration)) {
          console.log("[Recovery] Aborting attempt after explicit shutdown");
          return false;
        }

        try {
          const activeSessions = await window.openNow.getActiveSessions(token, effectiveStreamingBaseUrl);
          if (!isRecoveryGenerationCurrent(recoveryGeneration)) {
            console.log("[Recovery] Aborting attempt after active session lookup due to stale generation");
            return false;
          }
          const previousAppId = recoveryState.appId;
          const currentSessionId = currentSession.sessionId;
          const sameSessionCandidate =
            activeSessions.find((entry) => entry.sessionId === currentSessionId && entry.serverIp && isSessionReadyForConnect(entry.status)) ??
            null;

          let candidate = sameSessionCandidate;
          if (!candidate && previousAppId !== null) {
            candidate =
              activeSessions.find((entry) => (
                entry.appId === previousAppId &&
                entry.serverIp &&
                isSessionReadyForConnect(entry.status) &&
                entry.sessionId === currentSessionId
              )) ??
              activeSessions.find((entry) => (
                entry.appId === previousAppId &&
                entry.serverIp &&
                isSessionReadyForConnect(entry.status)
              )) ??
              null;
          }

          if (!candidate) {
            const persisted = runtimeSnapshotRef.current?.resumeContext;
            if (
              persisted &&
              persisted.sessionId === currentSessionId &&
              persisted.serverIp
            ) {
              candidate = {
                sessionId: persisted.sessionId,
                appId:
                  Number.isFinite(persisted.appId ?? NaN)
                    ? (persisted.appId as number)
                    : (previousAppId ?? 0),
                status: 2,
                serverIp: persisted.serverIp,
                streamingBaseUrl: persisted.streamingBaseUrl,
                signalingUrl: persisted.signalingUrl,
              };
              console.log("[Recovery] Falling back to persisted resume context", {
                sessionId: persisted.sessionId,
                serverIp: persisted.serverIp,
                appId: persisted.appId ?? previousAppId ?? null,
              });
            }
          }

          if (!candidate) {
            const hasQueueOnlyMatch = activeSessions.some((entry) => entry.sessionId === currentSessionId && entry.status === 1);
            if (hasQueueOnlyMatch) {
              throw new Error("The session is still queued and cannot be reclaimed until the server marks it ready again.");
            }
            throw new Error("The running session could not be found anymore, so resume was not possible.");
          }

          if (!candidate.serverIp) {
            throw new Error("The running session is missing a server address, so resume was not possible.");
          }

          const claimed = await window.openNow.claimSession({
            token,
            streamingBaseUrl: effectiveStreamingBaseUrl,
            serverIp: candidate.serverIp,
            sessionId: candidate.sessionId,
            ...resolveResumeIdentity(candidate.sessionId),
            recoveryMode: true,
            appId: resolveSessionClaimAppId(candidate),
            settings: buildCurrentStreamSettings(),
          });
          if (!isRecoveryGenerationCurrent(recoveryGeneration)) {
            console.log("[Recovery] Discarding claimed session due to stale recovery generation");
            return false;
          }

          const matchedContext = findGameContextForSession(candidate);
          if (matchedContext) {
            setStreamingGame(matchedContext.game);
            setStreamingStore(matchedContext.variant?.store ?? null);
          } else {
            setStreamingStore(null);
          }

          await applyClaimedSessionAndConnect(claimed, recoveryGeneration);
          if (!isRecoveryGenerationCurrent(recoveryGeneration)) {
            console.log("[Recovery] Recovery generation changed before connect completed");
            return false;
          }
          return true;
        } catch (error) {
          lastError = error instanceof Error ? error : new Error(String(error));
          console.warn(`[Recovery] Attempt ${attemptNumber} failed:`, lastError.message);
        }
      }

      if (lastError) {
        throw lastError;
      }
      return false;
    })();

    recoveryState.inFlight = attemptPromise;
    try {
      return await attemptPromise;
    } finally {
      if (signalingRecoveryRef.current.inFlight === attemptPromise) {
        signalingRecoveryRef.current.inFlight = null;
      }
    }
  }, [
    applyClaimedSessionAndConnect,
    authSession,
    disconnectSignalingControlled,
    effectiveStreamingBaseUrl,
    findGameContextForSession,
    isRecoveryGenerationCurrent,
    resolveResumeIdentity,
    resolveSessionClaimAppId,
    buildCurrentStreamSettings,
  ]);

  // Signaling events
  useEffect(() => {
    const ensureWebRtcClient = (): GfnWebRtcClient | null => {
      if (clientRef.current) {
        return clientRef.current;
      }
      if (!videoRef.current || !audioRef.current) {
        return null;
      }

      clientRef.current = new GfnWebRtcClient({
        videoElement: videoRef.current,
        audioElement: audioRef.current,
        autoFullScreen: settings.autoFullScreen,
        microphoneMode: settings.microphoneMode,
        microphoneDeviceId: settings.microphoneDeviceId || undefined,
        mouseSensitivity: settings.mouseSensitivity,
        mouseAcceleration: settings.mouseAcceleration,
        onLog: (line: string) => console.log(`[WebRTC] ${line}`),
        onStats: (stats) => diagnosticsStore.set(stats),
        onTimeWarning: (warning) => {
          setRemoteStreamWarning({
            code: warning.code,
            message: warningMessage(warning.code),
            tone: warningTone(warning.code),
            secondsLeft: warning.secondsLeft,
          });
        },
        onMicStateChange: (state) => {
          console.log(`[App] Mic state: ${state.state}${state.deviceLabel ? ` (${state.deviceLabel})` : ""}`);
        },
        onIceConnectionStateChange: (iceState) => {
          latestIceConnectionStateRef.current = iceState;
          if (iceDisconnectedRecoveryTimerRef.current !== null) {
            window.clearTimeout(iceDisconnectedRecoveryTimerRef.current);
            iceDisconnectedRecoveryTimerRef.current = null;
          }
          if (appUnloadingRef.current) {
            return;
          }
          if (streamStatusRef.current !== "streaming") {
            return;
          }
          if (iceState === "failed") {
            console.warn("[Recovery] ICE failed; attempting targeted recovery");
            void attemptSessionRecovery("ICE failed").catch((error) => {
              console.error("[Recovery] ICE-failed recovery failed:", error);
            });
            return;
          }
          if (iceState === "disconnected") {
            iceDisconnectedRecoveryTimerRef.current = window.setTimeout(() => {
              iceDisconnectedRecoveryTimerRef.current = null;
              if (appUnloadingRef.current || streamStatusRef.current !== "streaming") {
                return;
              }
              if (latestIceConnectionStateRef.current !== "disconnected") {
                return;
              }
              console.warn("[Recovery] ICE remained disconnected; attempting targeted recovery");
              void attemptSessionRecovery("ICE disconnected timeout").catch((error) => {
                console.error("[Recovery] ICE-disconnected recovery failed:", error);
              });
            }, ICE_DISCONNECTED_RECOVERY_GRACE_MS);
          }
        },
        onControllerMetaPress: () => {
          handleControllerMetaToggle();
        },
      });
      clientRef.current.inputPaused = controllerOverlayOpenRef.current;
      clientRef.current.setOutputVolume(streamVolume);
      clientRef.current.setMicrophoneLevel(streamMicLevel);
      if (settings.microphoneMode !== "disabled") {
        void clientRef.current.startMicrophone();
      }
      return clientRef.current;
    };

    const activateNativeInputForCurrentSession = (protocolVersion?: number): void => {
      const activeSession = sessionRef.current;
      if (!activeSession) {
        console.warn("[App] Received native stream event but no active session in sessionRef!");
        return;
      }
      const client = ensureWebRtcClient();
      if (!client) {
        console.warn("[App] Native stream event received before media elements were ready");
        return;
      }

      nativeStreamingRef.current = true;
      pendingControlledDisconnectsRef.current = 0;
      client.activateNativeInput(protocolVersion, {
        codec: settings.codec,
        colorQuality: settings.colorQuality,
        resolution: settings.resolution,
        fps: settings.fps,
        maxBitrateKbps: settings.maxBitrateMbps * 1000,
      });
      setLaunchError(null);
      setStreamStatus("streaming");
      scheduleStableRecoveryReset(activeSession.sessionId);
    };

    const unsubscribe = window.openNow.onSignalingEvent(async (event: MainToRendererSignalingEvent) => {
      console.log(`[App] Signaling event: ${event.type}`, event.type === "offer" ? `(SDP ${event.sdp.length} chars)` : "", event.type === "remote-ice" ? event.candidate : "");
      try {
        if (event.type === "offer") {
          pendingControlledDisconnectsRef.current = 0;
          const activeSession = sessionRef.current;
          if (!activeSession) {
            console.warn("[App] Received offer but no active session in sessionRef!");
            return;
          }
          const shouldEnforceRemoteIceGrace = awaitingRecoveryRemoteIceRef.current;
          remoteIceSeenForSessionRef.current = null;
          hasConfirmedRemoteIceRef.current = false;
          if (remoteIceGraceTimerRef.current !== null) {
            window.clearTimeout(remoteIceGraceTimerRef.current);
            remoteIceGraceTimerRef.current = null;
          }
          const expectedSessionId = activeSession.sessionId;
          const recoveryGenerationAtOffer = signalingRecoveryRef.current.generation;
          if (shouldEnforceRemoteIceGrace) {
            remoteIceGraceTimerRef.current = window.setTimeout(() => {
              remoteIceGraceTimerRef.current = null;
              if (sessionRef.current?.sessionId !== expectedSessionId) {
                return;
              }
              if (remoteIceSeenForSessionRef.current === expectedSessionId) {
                return;
              }
              if (remoteIceRecoveryGenerationRef.current === recoveryGenerationAtOffer) {
                return;
              }
              if (!RECOVERABLE_STREAM_STATUSES.includes(streamStatusRef.current)) {
                return;
              }
              awaitingRecoveryRemoteIceRef.current = false;
              remoteIceRecoveryGenerationRef.current = recoveryGenerationAtOffer;
              console.warn(
                `[Recovery] No remote ICE received within ${SIGNALING_REMOTE_ICE_GRACE_MS}ms after offer; forcing targeted recovery`,
              );
              void attemptSessionRecovery("No remote ICE received after offer").catch((error) => {
                console.error("[Recovery] ICE-timeout recovery failed:", error);
              });
            }, SIGNALING_REMOTE_ICE_GRACE_MS);
          }
          console.log("[App] Active session for offer:", JSON.stringify({
            sessionId: activeSession.sessionId,
            serverIp: activeSession.serverIp,
            signalingServer: activeSession.signalingServer,
            mediaConnectionInfo: activeSession.mediaConnectionInfo,
            iceServersCount: activeSession.iceServers?.length,
          }));

          const client = ensureWebRtcClient();

          if (client) {
            await client.handleOffer(event.sdp, activeSession, {
              codec: settings.codec,
              colorQuality: settings.colorQuality,
              resolution: settings.resolution,
              fps: settings.fps,
              maxBitrateKbps: settings.maxBitrateMbps * 1000,
              nativeTransitionDiagnostics: settings.nativeTransitionDiagnostics,
            });
            setLaunchError(null);
            setStreamStatus("streaming");
            scheduleStableRecoveryReset(activeSession.sessionId);
            console.log(
              "[Stream] Offer applied; use [WebRTC] logs for ICE/video dimensions. signalingServer=%s media=%s",
              activeSession.signalingServer,
              activeSession.mediaConnectionInfo
                ? `${activeSession.mediaConnectionInfo.ip}:${activeSession.mediaConnectionInfo.port}`
                : "n/a",
            );
          }
        } else if (event.type === "native-stream-started") {
          console.log("[App] Native streamer started:", event.message ?? "");
          activateNativeInputForCurrentSession(nativeInputProtocolVersionRef.current ?? undefined);
        } else if (event.type === "native-input-ready") {
          console.log("[App] Native input protocol ready:", event.protocolVersion);
          nativeInputProtocolVersionRef.current = event.protocolVersion;
          clientRef.current?.setNativeInputProtocolVersion(event.protocolVersion);
          if (nativeStreamingRef.current || sessionRef.current) {
            activateNativeInputForCurrentSession(event.protocolVersion);
          }
        } else if (event.type === "native-stream-stats") {
          diagnosticsStore.set(mergeNativeStreamStats(
            diagnosticsStore.getSnapshot(),
            event.stats,
          ));
        } else if (event.type === "native-stream-transition") {
          diagnosticsStore.set({
            ...diagnosticsStore.getSnapshot(),
            nativeRendererActive: true,
            nativeTransitionSummary: event.transition.summary,
            nativeRequestedFps: event.transition.requestedFps,
            nativeCapsFramerate: event.transition.capsFramerate,
            nativeQueueMode: event.transition.queueMode,
            lagReasonDetail: event.transition.summary ?? "Native video transition detected",
          });
        } else if (event.type === "native-stream-stopped") {
          const reason = event.reason ?? "Native streamer stopped";
          console.warn("[App] Native streamer stopped:", reason);
          nativeStreamingRef.current = false;
          nativeInputProtocolVersionRef.current = null;
          clientRef.current?.dispose();
          clientRef.current = null;
          launchInFlightRef.current = false;

          if (appUnloadingRef.current) {
            console.log("[Recovery] Ignoring native streamer stop during app shutdown");
            return;
          }
          if (
            signalingRecoveryRef.current.explicitShutdown
            || !RECOVERABLE_STREAM_STATUSES.includes(streamStatusRef.current)
          ) {
            console.log("[Recovery] Ignoring native streamer stop after explicit shutdown or non-recoverable status");
            return;
          }

          const recovered = await attemptSessionRecovery(reason).catch((error) => {
            console.error("[Recovery] Native streamer recovery failed:", error);
            return false;
          });
          if (!recovered) {
            if (
              signalingRecoveryRef.current.explicitShutdown
              || !RECOVERABLE_STREAM_STATUSES.includes(streamStatusRef.current)
            ) {
              console.log("[Recovery] Ignoring native streamer stop after explicit shutdown or non-recoverable status");
              return;
            }
            setLaunchError({
              stage: streamStatusToLoadingStage(streamStatusRef.current),
              title: "Native Streamer Stopped",
              description: "The native streaming process stopped and could not be restored automatically. Try resuming the session again from the app.",
            });
            resetLaunchRuntime({ keepLaunchError: true, keepStreamingContext: true });
            void refreshNavbarActiveSession();
            launchInFlightRef.current = false;
          }
        } else if (event.type === "remote-ice") {
          remoteIceSeenForSessionRef.current = sessionRef.current?.sessionId ?? null;
          hasConfirmedRemoteIceRef.current = true;
          awaitingRecoveryRemoteIceRef.current = false;
          if (remoteIceGraceTimerRef.current !== null) {
            window.clearTimeout(remoteIceGraceTimerRef.current);
            remoteIceGraceTimerRef.current = null;
          }
          await clientRef.current?.addRemoteCandidate(event.candidate);
        } else if (event.type === "disconnected") {
          if (appUnloadingRef.current) {
            console.log("[Recovery] Ignoring signaling disconnect during app shutdown");
            return;
          }
          const iceState = latestIceConnectionStateRef.current;
          if (
            iceState === "connected" ||
            iceState === "completed" ||
            iceState === "checking"
          ) {
            console.log(`[Recovery] Ignoring signaling disconnect while ICE state is ${iceState}`);
            return;
          }
          // Official-style behavior: if the attach never reached a confirmed remote ICE
          // handshake, do not auto-recover. Fail this attempt and require explicit resume.
          if (!hasConfirmedRemoteIceRef.current) {
            console.warn("[Recovery] Skipping auto-recovery: disconnected before remote ICE handshake");
            clientRef.current?.dispose();
            clientRef.current = null;
            setLaunchError({
              stage: streamStatusToLoadingStage(streamStatusRef.current),
              title: "Session Connection Lost",
              description: "Resume attach failed before media handshake. Try resuming once again.",
            });
            resetLaunchRuntime({ keepLaunchError: true, keepStreamingContext: true });
            void refreshNavbarActiveSession();
            launchInFlightRef.current = false;
            return;
          }
          if (remoteIceGraceTimerRef.current !== null) {
            window.clearTimeout(remoteIceGraceTimerRef.current);
            remoteIceGraceTimerRef.current = null;
          }
          remoteIceSeenForSessionRef.current = null;
          awaitingRecoveryRemoteIceRef.current = false;
          if (pendingControlledDisconnectsRef.current > 0) {
            pendingControlledDisconnectsRef.current -= 1;
            console.log("[Recovery] Ignoring controlled signaling disconnect");
            return;
          }
          console.warn("Signaling disconnected:", event.reason);
          const recovered = await attemptSessionRecovery(event.reason).catch((error) => {
            console.error("[Recovery] Signaling recovery failed:", error);
            throw error;
          });
          if (!recovered) {
            if (
              signalingRecoveryRef.current.explicitShutdown
              || !RECOVERABLE_STREAM_STATUSES.includes(streamStatusRef.current)
            ) {
              console.log("[Recovery] Ignoring disconnect after explicit shutdown or non-recoverable status");
              return;
            }
            clientRef.current?.dispose();
            clientRef.current = null;
            setLaunchError({
              stage: streamStatusToLoadingStage(streamStatusRef.current),
              title: "Session Connection Lost",
              description: "The connection to your running session was lost and could not be restored automatically. Try resuming it again from the app.",
            });
            resetLaunchRuntime({ keepLaunchError: true, keepStreamingContext: true });
            void refreshNavbarActiveSession();
            launchInFlightRef.current = false;
          }
        } else if (event.type === "error") {
          console.error("Signaling error:", event.message);
        }
      } catch (error) {
        if (appUnloadingRef.current) {
          console.log("[Recovery] Suppressing signaling handler errors during app shutdown");
          return;
        }
        if (
          signalingRecoveryRef.current.explicitShutdown
          || !RECOVERABLE_STREAM_STATUSES.includes(streamStatusRef.current)
        ) {
          console.log("[Recovery] Suppressing signaling error after explicit shutdown or non-recoverable status");
          return;
        }
        console.error("Signaling event error:", error);
        clientRef.current?.dispose();
        clientRef.current = null;
        const message = error instanceof Error ? error.message : "The connection to the running session was lost and resume failed.";
        setLaunchError({
          stage: streamStatusToLoadingStage(streamStatusRef.current),
          title: "Session Connection Lost",
          description: message,
        });
        resetLaunchRuntime({ keepLaunchError: true, keepStreamingContext: true });
        void refreshNavbarActiveSession();
        launchInFlightRef.current = false;
      }
    });

    return () => unsubscribe();
  }, [attemptSessionRecovery, diagnosticsStore, handleControllerMetaToggle, refreshNavbarActiveSession, resetLaunchRuntime, scheduleStableRecoveryReset, settings, streamMicLevel, streamVolume]);

  // Play game handler
  const handlePlayGame = useCallback(async (game: GameInfo, options?: { bypassGuards?: boolean; streamingBaseUrl?: string }) => {
    if (!selectedProvider) return;

    console.log("handlePlayGame entry", {
      title: game.title,
      launchInFlight: launchInFlightRef.current,
      streamStatus,
      bypass: options?.bypassGuards ?? false,
    });

    if (!options?.bypassGuards && (launchInFlightRef.current || streamStatus !== "idle" || navbarSessionActionInFlightRef.current)) {
      console.warn("Ignoring play request: launch already in progress or stream not idle", {
        inFlight: launchInFlightRef.current,
        streamStatus,
        navbarSessionAction: navbarSessionActionInFlightRef.current,
      });
      return;
    }

    const selectedVariantId = variantByGameId[game.id] ?? defaultVariantId(game);
    const selectedVariant = getSelectedVariant(game, selectedVariantId);
    const epicOwnershipError = getEpicOwnershipLaunchError(selectedVariant);
    if (epicOwnershipError) {
      setStreamingGame(game);
      setStreamingStore(selectedVariant?.store ?? null);
      setLaunchError({
        stage: "queue",
        title: epicOwnershipError.title,
        description: epicOwnershipError.description,
      });
      return;
    }

    launchInFlightRef.current = true;
    launchAbortRef.current = false;
    resetSignalingRecoveryState();
    let loadingStep: StreamLoadingStatus = "queue";
    const updateLoadingStep = (next: StreamLoadingStatus): void => {
      loadingStep = next;
      setStreamStatus(next);
    };

    setSessionStartedAtMs(null);
    setRemoteStreamWarning(null);
    setLocalSessionTimerWarning(null);
    setLaunchError(null);
    resetStatsOverlayToPreference();
    startPlaytimeSession(game.id);
    updateLoadingStep("queue");
    setQueuePosition(undefined);

    try {
      const token = authSession?.tokens.idToken ?? authSession?.tokens.accessToken;

      // Resolve appId
      let appId: string | null = null;
      if (isNumericId(selectedVariantId)) {
        appId = selectedVariantId;
      } else if (isNumericId(game.launchAppId)) {
        appId = game.launchAppId;
      }

      if (!appId && token) {
        try {
          const resolved = await window.openNow.resolveLaunchAppId({
            token,
            providerStreamingBaseUrl: effectiveStreamingBaseUrl,
            appIdOrUuid: game.uuid ?? selectedVariantId,
          });
          if (resolved && isNumericId(resolved)) {
            appId = resolved;
          }
        } catch {
          // Ignore resolution errors
        }
      }

      if (!appId) {
        throw new Error("Could not resolve numeric appId for this game");
      }

      const numericAppId = Number(appId);
      signalingRecoveryRef.current.appId = numericAppId;
      const matchedGameContext = findSessionContextForAppId(allKnownGames, variantByGameId, numericAppId) ?? {
        game,
        variant: selectedVariant,
      };
      setStreamingGame(matchedGameContext.game);
      setStreamingStore(matchedGameContext.variant?.store ?? null);

      let existingSessionStrategy: ExistingSessionStrategy | undefined;

      // Check for active sessions first
      if (token) {
        try {
          const activeSessions = await window.openNow.getActiveSessions(token, effectiveStreamingBaseUrl);
          if (activeSessions.length > 0) {
            // Only claim sessions that are already paused/ready (status 2 or 3).
            // Status=1 sessions are still in queue/setup; sending a RESUME claim
            // skips the queue/ad phase entirely. Let them fall through to
            // createSession so the polling loop handles queue position and ads.
            const matchingSession = activeSessions.find((entry) => entry.appId === numericAppId && (entry.status === 2 || entry.status === 3)) ?? null;
            const otherSession = activeSessions.find((s) => s.status === 2 || s.status === 3) ?? null;

            if (matchingSession) {
              await claimAndConnectSession(matchingSession);
              setNavbarActiveSession(null);
              return;
            }

            if (otherSession) {
              const choice = await window.openNow.showSessionConflictDialog();
              if (choice === "cancel") {
                resetLaunchRuntime();
                return;
              }
              if (choice === "resume") {
                await claimAndConnectSession(otherSession);
                setNavbarActiveSession(null);
                return;
              }
              if (choice === "new") {
                existingSessionStrategy = "force-new";
              }
            }
          }
        } catch (error) {
          console.error("Failed to claim/resume session:", error);
          // Continue to create new session
        }
      }

      const sessionProxyUrl = settings.sessionProxyEnabled ? settings.sessionProxyUrl.trim() : "";

      // Create new session
      const newSession = await window.openNow.createSession({
        token: token || undefined,
        streamingBaseUrl: options?.streamingBaseUrl || effectiveStreamingBaseUrl,
        appId,
        internalTitle: game.title,
        accountLinked: chooseAccountLinked(game, selectedVariant),
        existingSessionStrategy,
        proxyUrl: sessionProxyUrl || undefined,
        zone: "prod",
        settings: buildCurrentStreamSettings(),
      });

      setSession(newSession);
      setQueuePosition(newSession.queuePosition);

      // Poll for readiness.
      // Queue and setup/starting modes wait indefinitely until the session becomes ready
      // or the launch is explicitly aborted. Some rigs take much longer than 180s.
      let finalSession: SessionInfo | null = null;
      let latestSession = newSession;
      let isInQueueMode = isSessionInQueue(newSession);
      let attempt = 0;

      while (true) {
        attempt++;

        const pollIntervalMs = shouldUseQueueAdPolling(latestSession, subscriptionInfo, authSession)
          ? SESSION_AD_POLL_INTERVAL_MS
          : SESSION_READY_POLL_INTERVAL_MS;

        // Sleep in small ticks during ad-polling intervals so the loop can react
        // quickly when reportSessionAd clears isAdsRequired (which only updates
        // sessionRef, not the local latestSession variable).  Standard 2 s intervals
        // are kept as a single sleep since they're already short.
        if (pollIntervalMs > SESSION_READY_POLL_INTERVAL_MS) {
          const tickMs = 500;
          let elapsed = 0;
          while (elapsed < pollIntervalMs) {
            await sleep(tickMs);
            elapsed += tickMs;
            if (launchAbortRef.current) return;
            // Sync ad-action responses from sessionRef into the local tracking variable
            // so shouldUseQueueAdPolling sees the updated adState immediately.
            const refSession = sessionRef.current;
            if (refSession && refSession.sessionId === latestSession.sessionId) {
              latestSession = mergePolledSessionState(latestSession, refSession);
            }
            // Break out of the sleep early when ads are no longer required.
            if (!shouldUseQueueAdPolling(latestSession, subscriptionInfo, authSession)) {
              break;
            }
          }
        } else {
          await sleep(pollIntervalMs);
        }

        if (shouldUseQueueAdPolling(latestSession, subscriptionInfo, authSession) && queueAdPlaybackRef.current) {
          const graceDeadline = Date.now() + 5000;
          while (queueAdPlaybackRef.current && Date.now() < graceDeadline) {
            await sleep(200);
            if (launchAbortRef.current) {
              return;
            }
          }
        }

        if (launchAbortRef.current) {
          return;
        }

        if (launchAbortRef.current) {
          return;
        }

        const polled = await window.openNow.pollSession({
          token: token || undefined,
          streamingBaseUrl: newSession.streamingBaseUrl ?? effectiveStreamingBaseUrl,
          serverIp: newSession.serverIp,
          zone: newSession.zone,
          sessionId: newSession.sessionId,
          clientId: newSession.clientId,
          deviceId: newSession.deviceId,
          proxyUrl: sessionProxyUrl || undefined,
        });

        if (launchAbortRef.current) {
          return;
        }

        const mergedSession = mergePolledSessionState(latestSession, polled);
        latestSession = mergedSession;

        setSession(mergedSession);
        setQueuePosition(mergedSession.queuePosition);

        // Check if queue just cleared so the loading UI can transition to setup mode.
        isInQueueMode = isSessionInQueue(mergedSession);

        console.log(
          `Poll attempt ${attempt}: status=${mergedSession.status}, seatSetupStep=${mergedSession.seatSetupStep ?? "n/a"}, queuePosition=${mergedSession.queuePosition ?? "n/a"}, serverIp=${mergedSession.serverIp}, queueMode=${isInQueueMode}, adsRequired=${isSessionAdsRequired(mergedSession.adState)}`,
        );

        if (isSessionReadyForConnect(mergedSession.status)) {
          finalSession = mergedSession;
          break;
        }

        // Update status based on session state
        if (isInQueueMode) {
          updateLoadingStep("queue");
        } else if (mergedSession.status === 1) {
          updateLoadingStep("setup");
        }

      }

      // finalSession is guaranteed to be set here (we only exit the loop via break when session is ready)

      setQueuePosition(undefined);
      updateLoadingStep("connecting");

      // Use finalSession (the status=2 poll result) as the authoritative source for
      // signaling coordinates — it carries the real server IP resolved at the moment
      // the rig became ready. sessionRef.current may still hold stale zone-LB data
      // from a prior React render cycle.
      const sessionToConnect = finalSession ?? sessionRef.current ?? newSession;
      console.log("Connecting signaling with:", {
        sessionId: sessionToConnect.sessionId,
        signalingServer: sessionToConnect.signalingServer,
        signalingUrl: sessionToConnect.signalingUrl,
        status: sessionToConnect.status,
      });

      await window.openNow.connectSignaling(buildSignalingConnectRequest(sessionToConnect));
    } catch (error) {
      if (launchAbortRef.current) {
        return;
      }
      console.error("Launch failed:", error);
      setLaunchError(toLaunchErrorState(error, loadingStep));
      await disconnectSignalingControlled();
      clientRef.current?.dispose();
      clientRef.current = null;
      resetLaunchRuntime({ keepLaunchError: true, keepStreamingContext: true });
      void refreshNavbarActiveSession();
    } finally {
      launchInFlightRef.current = false;
    }
  }, [
    authSession,
    allKnownGames,
    buildSignalingConnectRequest,
    claimAndConnectSession,
    effectiveStreamingBaseUrl,
    refreshNavbarActiveSession,
    resetSignalingRecoveryState,
    resetLaunchRuntime,
    resetStatsOverlayToPreference,
    selectedProvider,
    settings,
    streamStatus,
    variantByGameId,
  ]);

  // Gate handler: shows queue server modal for FREE-tier users before launching
  const handleInitiatePlay = useCallback(async (game: GameInfo) => {
    const effectiveTier = normalizeMembershipTier(
      subscriptionInfo?.membershipTier ?? authSession?.user.membershipTier,
    );
    const isFreeUser = effectiveTier === "FREE";
    const isAllianceServer = isAllianceStreamingBaseUrl(effectiveStreamingBaseUrl);
    if (isAllianceServer) {
      setQueueModalData(null);
      void handlePlayGame(game);
      return;
    }
    if (settings.hideServerSelector) {
      setQueueModalData(null);
      void handlePlayGame(game);
      return;
    }
    if (isFreeUser && streamStatus === "idle" && !launchInFlightRef.current) {
      try {
        const [queueResult, mappingResult] = await Promise.allSettled([
          window.openNow.fetchPrintedWasteQueue(),
          window.openNow.fetchPrintedWasteServerMapping(),
        ]);

        if (queueResult.status !== "fulfilled" || mappingResult.status !== "fulfilled") {
          console.warn(
            "[QueueServerSelect] PrintedWaste unavailable, skipping queue checks and launching with default routing.",
            {
              queueStatus: queueResult.status,
              mappingStatus: mappingResult.status,
            },
          );
          setQueueModalData(null);
          void handlePlayGame(game);
          return;
        }

        const queueData = queueResult.value;
        if (!queueData || Object.keys(queueData).length === 0) {
          setQueueModalData(null);
          void handlePlayGame(game);
          return;
        }

        if (!hasAnyEligiblePrintedWasteZone(queueData, mappingResult.value)) {
          console.warn(
            "[QueueServerSelect] No eligible non-nuked PrintedWaste zones available, skipping queue checks.",
          );
          setQueueModalData(null);
          void handlePlayGame(game);
          return;
        }

        setQueueModalData(queueData);
        setQueueModalGame(game);
      } catch (error) {
        console.warn("[QueueServerSelect] PrintedWaste queue checks failed, launching without modal.", error);
        setQueueModalData(null);
        void handlePlayGame(game);
      }
      return;
    }
    void handlePlayGame(game);
  }, [subscriptionInfo, authSession, streamStatus, handlePlayGame, effectiveStreamingBaseUrl]);

  const handleQueueModalConfirm = useCallback((zoneUrl: string | null) => {
    const game = queueModalGame;
    setQueueModalGame(null);
    setQueueModalData(null);
    if (game) {
      void handlePlayGame(game, { streamingBaseUrl: zoneUrl ?? undefined });
    }
  }, [queueModalGame, handlePlayGame]);

  const handleQueueModalCancel = useCallback(() => {
    setQueueModalGame(null);
    setQueueModalData(null);
  }, []);

  useEffect(() => {
    if (!logoutConfirmOpen && !removeAccountConfirmOpen) return;

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        if (removeAccountConfirmOpen) {
          setRemoveAccountConfirmOpen(false);
          setAccountToRemove(null);
        } else if (logoutConfirmOpen) {
          setLogoutConfirmOpen(false);
        }
      }
      if (event.key === "Enter") {
        event.preventDefault();
        if (removeAccountConfirmOpen) {
          void confirmRemoveAccount();
        } else if (logoutConfirmOpen) {
          void confirmLogout();
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";

    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      document.body.style.overflow = previousOverflow;
    };
  }, [confirmLogout, confirmRemoveAccount, logoutConfirmOpen, removeAccountConfirmOpen]);

  const accountToRemoveDisplayName = useMemo(() => (
    savedAccounts.find((account) => account.userId === accountToRemove)?.displayName ?? "this account"
  ), [accountToRemove, savedAccounts]);

  const logoutConfirmModal = logoutConfirmOpen && typeof document !== "undefined"
    ? createPortal(
        <div className="logout-confirm" role="dialog" aria-modal="true" aria-label="Log out confirmation">
          <button
            type="button"
            className="logout-confirm-backdrop"
            onClick={() => setLogoutConfirmOpen(false)}
            aria-label="Cancel log out"
          />
          <div className="logout-confirm-card">
            <div className="logout-confirm-kicker">Accounts</div>
            <h3 className="logout-confirm-title">Sign out all accounts?</h3>
            <p className="logout-confirm-text">
              You&apos;re about to remove every saved account from this device and return to guest mode.
            </p>
            <p className="logout-confirm-subtext">
              Your cloud session data stays on the service. This just clears local OpenNOW account sessions.
            </p>
            <div className="logout-confirm-actions">
              <button
                type="button"
                className="logout-confirm-btn logout-confirm-btn-cancel"
                onClick={() => setLogoutConfirmOpen(false)}
              >
                Stay signed in
              </button>
              <button
                type="button"
                className="logout-confirm-btn logout-confirm-btn-confirm"
                onClick={() => {
                  void confirmLogout();
                }}
              >
                Sign out all
              </button>
            </div>
            <div className="logout-confirm-hint">
              <kbd>Enter</kbd> confirm · <kbd>Esc</kbd> cancel
            </div>
          </div>
        </div>,
        document.body,
      )
    : null;

  const removeAccountConfirmModal = removeAccountConfirmOpen && typeof document !== "undefined"
    ? createPortal(
        <div className="logout-confirm" role="dialog" aria-modal="true" aria-label="Remove account confirmation">
          <button
            type="button"
            className="logout-confirm-backdrop"
            onClick={() => {
              setRemoveAccountConfirmOpen(false);
              setAccountToRemove(null);
            }}
            aria-label="Cancel account removal"
          />
          <div className="logout-confirm-card">
            <div className="logout-confirm-kicker">Accounts</div>
            <h3 className="logout-confirm-title">Remove account?</h3>
            <p className="logout-confirm-text">
              Are you sure you want to remove {accountToRemoveDisplayName} from saved accounts?
            </p>
            <p className="logout-confirm-subtext">
              You can add this account again by signing in.
            </p>
            <div className="logout-confirm-actions">
              <button
                type="button"
                className="logout-confirm-btn logout-confirm-btn-cancel"
                onClick={() => {
                  setRemoveAccountConfirmOpen(false);
                  setAccountToRemove(null);
                }}
              >
                Cancel
              </button>
              <button
                type="button"
                className="logout-confirm-btn logout-confirm-btn-confirm"
                onClick={() => {
                  void confirmRemoveAccount();
                }}
              >
                Remove
              </button>
            </div>
            <div className="logout-confirm-hint">
              <kbd>Enter</kbd> confirm · <kbd>Esc</kbd> cancel
            </div>
          </div>
        </div>,
        document.body,
      )
    : null;

  const handleResumeFromNavbar = useCallback(async () => {
    if (
      !selectedProvider
      || !navbarActiveSession
      || isResumingNavbarSession
      || isTerminatingNavbarSession
      || navbarSessionActionInFlightRef.current
    ) {
      return;
    }
    if (launchInFlightRef.current || streamStatus !== "idle") {
      return;
    }

    navbarSessionActionInFlightRef.current = "resume";
    launchInFlightRef.current = true;
    resetSignalingRecoveryState();
    setIsResumingNavbarSession(true);
    let loadingStep: StreamLoadingStatus = "setup";
    const updateLoadingStep = (next: StreamLoadingStatus): void => {
      loadingStep = next;
      setStreamStatus(next);
    };

    setLaunchError(null);
    setQueuePosition(undefined);
    setSessionStartedAtMs(null);
    setRemoteStreamWarning(null);
    setLocalSessionTimerWarning(null);
    resetStatsOverlayToPreference();
    const matchedContext = findGameContextForSession(navbarActiveSession);
    if (matchedContext) {
      setStreamingGame(matchedContext.game);
      setStreamingStore(matchedContext.variant?.store ?? null);
    } else {
      setStreamingStore(null);
    }
    updateLoadingStep("setup");

    try {
      signalingRecoveryRef.current.appId = navbarActiveSession.appId;
      await claimAndConnectSession(navbarActiveSession);
      setNavbarActiveSession(null);
    } catch (error) {
      console.error("Navbar resume failed:", error);
      setLaunchError(toLaunchErrorState(error, loadingStep));
      await disconnectSignalingControlled();
      clientRef.current?.dispose();
      clientRef.current = null;
      resetLaunchRuntime({ keepLaunchError: true });
      void refreshNavbarActiveSession();
    } finally {
      navbarSessionActionInFlightRef.current = null;
      launchInFlightRef.current = false;
      setIsResumingNavbarSession(false);
    }
  }, [
    claimAndConnectSession,
    isTerminatingNavbarSession,
    isResumingNavbarSession,
    navbarActiveSession,
    findGameContextForSession,
    refreshNavbarActiveSession,
    resetSignalingRecoveryState,
    resetLaunchRuntime,
    resetStatsOverlayToPreference,
    selectedProvider,
    streamStatus,
  ]);

  const handleTerminateNavbarSession = useCallback(async () => {
    if (
      !navbarActiveSession
      || isResumingNavbarSession
      || isTerminatingNavbarSession
      || navbarSessionActionInFlightRef.current
    ) {
      return;
    }
    if (launchInFlightRef.current || streamStatus !== "idle") {
      return;
    }

    const activeSessionTitle = gameTitleByAppId.get(navbarActiveSession.appId)?.trim() || "this session";
    if (!window.confirm(`Terminate ${activeSessionTitle}? This will end the active cloud session immediately.`)) {
      return;
    }

    navbarSessionActionInFlightRef.current = "terminate";
    setIsTerminatingNavbarSession(true);
    try {
      await stopSessionByTarget({
        sessionId: navbarActiveSession.sessionId,
        zone: "",
        streamingBaseUrl: navbarActiveSession.streamingBaseUrl ?? (effectiveStreamingBaseUrl || undefined),
        serverIp: navbarActiveSession.serverIp,
      });
      setNavbarActiveSession(null);
    } catch (error) {
      console.error("Navbar terminate failed:", error);
    } finally {
      navbarSessionActionInFlightRef.current = null;
      setIsTerminatingNavbarSession(false);
      void refreshNavbarActiveSession();
    }
  }, [
    effectiveStreamingBaseUrl,
    gameTitleByAppId,
    isResumingNavbarSession,
    isTerminatingNavbarSession,
    navbarActiveSession,
    refreshNavbarActiveSession,
    stopSessionByTarget,
    streamStatus,
  ]);

  // Stop stream handler
  const handleStopStream = useCallback(async () => {
    try {
      resolveExitPrompt(false);
      const status = streamStatusRef.current;
      if (status !== "idle" && status !== "streaming") {
        launchAbortRef.current = true;
      }
      markExplicitSignalingShutdown();
      await disconnectSignalingControlled();

      const current = sessionRef.current;
      if (current) {
        await stopSessionByTarget({
          streamingBaseUrl: current.streamingBaseUrl,
          serverIp: current.serverIp,
          zone: current.zone,
          sessionId: current.sessionId,
          clientId: current.clientId,
          deviceId: current.deviceId,
        });
      }

      clientRef.current?.dispose();
      clientRef.current = null;
      setNavbarActiveSession(null);
      if (streamingGame) endPlaytimeSession(streamingGame.id);
      resetLaunchRuntime();
      void refreshNavbarActiveSession();
    } catch (error) {
      console.error("Stop failed:", error);
    }
  }, [endPlaytimeSession, markExplicitSignalingShutdown, refreshNavbarActiveSession, resetLaunchRuntime, resolveExitPrompt, stopSessionByTarget, streamingGame]);

  const handleSwitchGame = useCallback(async (game: GameInfo) => {
    setControllerOverlayOpen(false);
    setPendingSwitchGameTitle(game.title ?? null);
    setPendingSwitchGameCover(game.imageUrl ?? null);
    setPendingSwitchGameDescription(game.description ?? null);
    setPendingSwitchGameId(game.id ?? null);
    setIsSwitchingGame(true);
    setSwitchingPhase("cleaning");
    // allow overlay to paint
    await new Promise<void>((resolve) => window.setTimeout(resolve, 140));
    try {
      await handleStopStream();
    } catch (e) {
      console.error("Error while cleaning up stream during switch:", e);
    }
    setSwitchingPhase("creating");
    // ensure runtime flags/state reflect the stopped session before launching again
    launchInFlightRef.current = false;
    setStreamStatus("idle");
    // give the render loop a frame so React state updates propagate
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
    // small pause so user sees transition
    await new Promise<void>((resolve) => window.setTimeout(resolve, 120));

    // Wait until the play guard conditions are satisfied (defensive against races)
    const ready = await waitFor(() => !launchInFlightRef.current && streamStatusRef.current === "idle", { timeout: 1000, interval: 50 });
    if (!ready) {
      console.warn("Switch flow: runtime not ready for new launch after cleanup", {
        launchInFlight: launchInFlightRef.current,
        streamStatus: streamStatusRef.current,
      });
      // Do a small additional delay before aborting the automatic start to avoid nav-to-dashboard
      await sleep(250);
    }

    try {
      await handlePlayGame(game, { bypassGuards: true });
    } catch (e) {
      console.error("Error while starting new stream during switch:", e);
    }
    setIsSwitchingGame(false);
    setSwitchingPhase(null);
    setPendingSwitchGameTitle(null);
    setPendingSwitchGameCover(null);
    setPendingSwitchGameDescription(null);
    setPendingSwitchGameId(null);
  }, [handleStopStream, handlePlayGame]);

  const handleDismissLaunchError = useCallback(async () => {
    markExplicitSignalingShutdown();
    await disconnectSignalingControlled();
    clientRef.current?.dispose();
    clientRef.current = null;
    resetLaunchRuntime();
    void refreshNavbarActiveSession();
  }, [markExplicitSignalingShutdown, refreshNavbarActiveSession, resetLaunchRuntime]);

  const releasePointerLockIfNeeded = useCallback(async () => {
    if (document.pointerLockElement) {
      // Tell the client to suppress synthetic Escape/reactive re-acquisition
      try {
        // clientRef is a mutable ref to the GfnWebRtcClient instance; access runtime property
        (clientRef.current as any).suppressNextSyntheticEscape = true;
      } catch (e) {
        // ignore
      }
      document.exitPointerLock();
      await sleep(75);
    }
  }, []);

  const handlePromptedStopStream = useCallback(async () => {
    if (streamStatus === "idle") {
      return;
    }

    await releasePointerLockIfNeeded();

    const loadingPhases: StreamStatus[] = ["queue", "setup", "starting", "connecting"];
    if (loadingPhases.includes(streamStatus)) {
      launchAbortRef.current = true;
      await handleStopStream();
      return;
    }

    const gameName = (streamingGame?.title || "this game").trim();
    const shouldExit = await requestExitPrompt(gameName);
    if (!shouldExit) {
      return;
    }

    await handleStopStream();
  }, [handleStopStream, releasePointerLockIfNeeded, requestExitPrompt, streamStatus, streamingGame?.title]);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      const isTyping = !!target && (
        target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.isContentEditable
      );
      if (isTyping) {
        return;
      }

      if (exitPrompt.open) {
        if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          e.stopImmediatePropagation();
          handleExitPromptCancel();
        } else if (e.key === "Enter") {
          e.preventDefault();
          e.stopPropagation();
          e.stopImmediatePropagation();
          handleExitPromptConfirm();
        }
        return;
      }

      const isPasteShortcut = e.key.toLowerCase() === "v" && !e.altKey && (isMac ? e.metaKey : e.ctrlKey);
      if (streamStatus === "streaming" && isPasteShortcut) {
        // Always stop local/browser paste behavior while streaming.
        // If clipboard paste is enabled, send clipboard text into the stream.
        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation();

        if (settings.clipboardPaste) {
          void (async () => {
            const client = clientRef.current;
            if (!client) return;

            try {
              const text = await navigator.clipboard.readText();
              if (text && client.sendText(text) > 0) {
                return;
              }
            } catch (error) {
              console.warn("Clipboard read failed, falling back to paste shortcut:", error);
            }

            client.sendPasteShortcut(isMac);
          })();
        }
        return;
      }

      if (isShortcutMatch(e, shortcuts.toggleStats)) {
        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation();
        setShowStatsOverlay((prev) => !prev);
        return;
      }

      if (isShortcutMatch(e, shortcuts.togglePointerLock)) {
        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation();
        if (streamStatus === "streaming" && videoRef.current) {
          if (document.pointerLockElement === videoRef.current) {
            try {
              (clientRef.current as any).suppressNextSyntheticEscape = true;
            } catch {
              // best-effort — client may not be initialised
            }
            document.exitPointerLock();
          } else {
            void requestPointerLockCapture(videoRef.current);
          }
        }
        return;
      }

      if (isShortcutMatch(e, shortcuts.toggleFullscreen)) {
        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation();
        if (streamStatus === "connecting" || streamStatus === "streaming") {
          void toggleSessionFullscreen();
        }
        return;
      }

      if (isShortcutMatch(e, shortcuts.stopStream)) {
        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation();
        void handlePromptedStopStream();
        return;
      }

      if (isShortcutMatch(e, shortcuts.toggleAntiAfk)) {
        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation();
        if (streamStatus === "streaming") {
          setAntiAfkEnabled((prev) => !prev);
          setAntiAfkAckNonce((n) => n + 1);
        }
        return;
      }

      if (isShortcutMatch(e, shortcuts.toggleMicrophone)) {
        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation();
        if (streamStatus === "streaming") {
          clientRef.current?.toggleMicrophone();
        }
      }
    };

    // Use capture phase so app shortcuts run before stream input capture listeners.
    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [
    exitPrompt.open,
    handleExitPromptCancel,
    handleExitPromptConfirm,
    handlePromptedStopStream,
    requestPointerLockCapture,
    settings.clipboardPaste,
    shortcuts,
    streamStatus,
    toggleSessionFullscreen,
  ]);

  const filteredGames = games;

  const filteredLibraryGames = useMemo(() => {
    const query = searchQuery.trim();
    const searched = query ? libraryGames.filter((game) => matchesGameSearch(game, query)) : libraryGames;
    return sortLibraryGames(
      searched,
      catalogSelectedSortId === "relevance" ? "last_played" : catalogSelectedSortId,
      playtime,
    );
  }, [libraryGames, searchQuery, catalogSelectedSortId, playtime]);

  const activeSessionGameTitle = useMemo(() => {
    if (!navbarActiveSession) return null;
    const mappedTitle = gameTitleByAppId.get(navbarActiveSession.appId);
    if (mappedTitle) {
      return mappedTitle;
    }
    if (session?.sessionId === navbarActiveSession.sessionId && streamingGame?.title) {
      return streamingGame.title;
    }
    return null;
  }, [gameTitleByAppId, navbarActiveSession, session?.sessionId, streamingGame?.title]);

  const controllerCloudResumeCoverUrl = useMemo(() => {
    if (!navbarActiveSession) return null;
    const ctx = findGameContextForSession(navbarActiveSession);
    return ctx?.game?.imageUrl ?? null;
  }, [findGameContextForSession, navbarActiveSession]);

  const controllerCloudSessionResumable = useMemo(
    () =>
      Boolean(
        streamStatus === "idle"
        && selectedProvider
        && navbarActiveSession?.serverIp
        && !isResumingNavbarSession
        && !isTerminatingNavbarSession
      ),
    [isResumingNavbarSession, isTerminatingNavbarSession, navbarActiveSession, selectedProvider, streamStatus],
  );

  const effectiveAdState = getEffectiveAdState(session, subscriptionInfo, authSession);
  const activeQueueAd = useMemo(
    () => getActiveQueueAd(effectiveAdState, activeQueueAdId),
    [activeQueueAdId, effectiveAdState],
  );
  const activeQueueAdMediaUrl = getPreferredSessionAdMediaUrl(activeQueueAd) ?? (activeQueueAd ? adMediaUrlCacheRef.current[activeQueueAd.adId] : undefined);

  useEffect(() => {
    const nextActiveAd = getActiveQueueAd(effectiveAdState, activeQueueAdIdRef.current);
    const nextActiveAdId = nextActiveAd?.adId ?? null;
    activeQueueAdIdRef.current = nextActiveAdId;
    setActiveQueueAdId((previous) => (previous === nextActiveAdId ? previous : nextActiveAdId));

    if (!nextActiveAdId) {
      clearQueueAdWatchdogs();
    }
  }, [clearQueueAdWatchdogs, effectiveAdState]);

  useEffect(() => {
    if (!activeQueueAd) {
      return;
    }

    const syncQueueAdVisibility = (): void => {
      const preview = queueAdPreviewRef.current;
      if (!preview) {
        return;
      }

      if (document.hidden || isSessionQueuePaused(effectiveAdState)) {
        preview.pause();
        return;
      }

      void preview.resume();
    };

    syncQueueAdVisibility();
    document.addEventListener("visibilitychange", syncQueueAdVisibility);
    return () => {
      document.removeEventListener("visibilitychange", syncQueueAdVisibility);
    };
  }, [activeQueueAd, effectiveAdState]);

  // Show login screen if not authenticated
  if (!authSession) {
    return (
      <>
        <LoginScreen
          providers={providers}
          selectedProviderId={providerIdpId}
          onProviderChange={setProviderIdpId}
          onLogin={handleLogin}
          isLoading={isLoggingIn}
          error={loginError}
          isInitializing={isInitializing}
          statusMessage={startupStatusMessage}
        />
      </>
    );
  }

  const showLaunchOverlay = streamStatus !== "idle" || launchError !== null || isSwitchingGame;
  const hasActiveStreamView = streamStatus !== "idle";
  const showLaunchErrorOverlay = launchError !== null;
  const showControllerLaunchLoading =
    !isSwitchingGame
    && settings.controllerMode
    && (showLaunchErrorOverlay || (streamStatus !== "idle" && streamStatus !== "streaming"));
  const showDesktopLaunchLoading =
    !isSwitchingGame
    && !settings.controllerMode
    && (showLaunchErrorOverlay || (streamStatus !== "idle" && streamStatus !== "streaming"));
  // Show stream lifecycle (waiting/connecting/streaming/failure)
  if (showLaunchOverlay) {
    const loadingStatus = launchError ? launchError.stage : toLoadingStatus(streamStatus);
    return (
      <>
        {hasActiveStreamView && (
          <StreamView
            className={isSwitchingGame ? "sv--switching" : undefined}
            videoRef={videoRef}
            audioRef={audioRef}
            diagnosticsStore={diagnosticsStore}
            showStats={showStatsOverlay}
            shortcuts={{
              toggleStats: formatShortcutForDisplay(settings.shortcutToggleStats, isMac),
              togglePointerLock: formatShortcutForDisplay(settings.shortcutTogglePointerLock, isMac),
              toggleFullscreen: formatShortcutForDisplay(settings.shortcutToggleFullscreen, isMac),
              stopStream: formatShortcutForDisplay(settings.shortcutStopStream, isMac),
              toggleAntiAfk: shortcuts.toggleAntiAfk.canonical,
              toggleMicrophone: formatShortcutForDisplay(settings.shortcutToggleMicrophone, isMac),
              screenshot: shortcuts.screenshot.canonical,
              recording: shortcuts.recording.canonical,
            }}
            hideStreamButtons={settings.hideStreamButtons}
            serverRegion={session?.serverIp}
            antiAfkEnabled={antiAfkEnabled}
            antiAfkAckNonce={antiAfkAckNonce}
            showAntiAfkIndicator={settings.showAntiAfkIndicator}
            exitPrompt={exitPrompt}
            sessionStartedAtMs={sessionStartedAtMs}
            sessionCounterEnabled={settings.sessionCounterEnabled}
            sessionClockShowEveryMinutes={settings.sessionClockShowEveryMinutes}
            sessionClockShowDurationSeconds={settings.sessionClockShowDurationSeconds}
            streamWarning={streamWarning}
            isFullscreen={sessionFullscreen || !!document.fullscreenElement}
            isConnecting={streamStatus === "connecting"}
            isStreaming={isStreaming}
            gameTitle={streamingGame?.title ?? "Game"}
            platformStore={streamingStore ?? undefined}
            onToggleFullscreen={() => {
              void toggleSessionFullscreen();
            }}
            onConfirmExit={handleExitPromptConfirm}
            onCancelExit={handleExitPromptCancel}
            onEndSession={() => {
              void handlePromptedStopStream();
            }}
            onToggleMicrophone={() => {
              clientRef.current?.toggleMicrophone();
            }}
            mouseSensitivity={settings.mouseSensitivity}
            onMouseSensitivityChange={handleMouseSensitivityChange}
            mouseAcceleration={settings.mouseAcceleration}
            onMouseAccelerationChange={handleMouseAccelerationChange}
            microphoneMode={settings.microphoneMode}
            onMicrophoneModeChange={handleMicrophoneModeChange}
            onScreenshotShortcutChange={(value) => {
              void updateSetting("shortcutScreenshot", value);
            }}
            onRecordingShortcutChange={(value) => {
              void updateSetting("shortcutToggleRecording", value);
            }}
            subscriptionInfo={subscriptionInfo}
            micTrack={clientRef.current?.getMicTrack() ?? null}
            onRequestPointerLock={handleRequestPointerLock}
            onReleasePointerLock={() => {
              void releasePointerLockIfNeeded();
            }}
            allowEscapeToExitFullscreen={settings.allowEscapeToExitFullscreen}
            hideConnectingOverlay={settings.controllerMode}
          />
        )}
        {isSwitchingGame && settings.controllerMode && (
          <ControllerStreamLoading
            gameTitle={pendingSwitchGameTitle ?? streamingGame?.title ?? "Game"}
            status={loadingStatus}
            queuePosition={queuePosition}
            adState={effectiveAdState}
            activeAd={activeQueueAd}
            activeAdMediaUrl={activeQueueAdMediaUrl}
            error={
              launchError
                ? {
                    title: launchError.title,
                    description: launchError.description,
                    code: launchError.codeLabel,
                  }
                : undefined
            }
            onAdPlaybackEvent={handleQueueAdPlaybackEvent}
            adPreviewRef={queueAdPreviewRef}
          />
        )}
        {isSwitchingGame && !settings.controllerMode && (
          <StreamLoading
            gameTitle={pendingSwitchGameTitle ?? streamingGame?.title ?? "Game"}
            gameCover={pendingSwitchGameCover ?? streamingGame?.imageUrl}
            platformStore={streamingStore ?? undefined}
            status={switchingPhase === "cleaning" ? "setup" : "starting"}
            queuePosition={queuePosition}
            adState={effectiveAdState}
            activeAd={activeQueueAd}
            activeAdMediaUrl={activeQueueAdMediaUrl}
            onAdPlaybackEvent={handleQueueAdPlaybackEvent}
            adPreviewRef={queueAdPreviewRef}
            error={
              launchError
                ? {
                    title: launchError.title,
                    description: launchError.description,
                    code: launchError.codeLabel,
                  }
                : undefined
            }
            onCancel={() => {
              if (launchError) {
                void handleDismissLaunchError();
                return;
              }
              void handlePromptedStopStream();
            }}
          />
        )}
        {streamStatus === "streaming" && controllerOverlayOpen && (
          <ControllerInStreamShell
            diagnosticsStore={diagnosticsStore}
            sessionStartedAtMs={sessionStartedAtMs}
            sessionCounterEnabled={settings.sessionCounterEnabled}
            isStreaming={isStreaming}
            streamWarning={streamWarning ?? undefined}
            queuePosition={queuePosition}
          >
            <ControllerLibraryPage
              games={filteredLibraryGames}
              onPlayGame={handleSwitchGame}
              isLoading={isLoadingGames}
              selectedGameId={selectedGameId}
              onSelectGame={setSelectedGameId}
              uiSoundsEnabled={settings.controllerUiSounds}
              selectedVariantByGameId={variantByGameId}
              onSelectGameVariant={handleSelectGameVariant}
              favoriteGameIds={settings.favoriteGameIds}
              onToggleFavoriteGame={handleToggleFavoriteGame}
              onOpenSettings={() => setCurrentPage("settings")}
              currentStreamingGame={streamingGame}
              onResumeGame={() => setControllerOverlayOpen(false)}
              inStreamMenu
              streamMenuVolume={streamVolume}
              onStreamMenuVolumeChange={handleStreamVolumeChange}
              streamMenuMicLevel={streamMicLevel}
              onStreamMenuMicLevelChange={handleStreamMicLevelChange}
              streamMicTrack={clientRef.current?.getMicTrack() ?? null}
              onStreamMenuToggleMicrophone={handleToggleStreamMicrophone}
              onStreamMenuToggleFullscreen={() => {
                void toggleSessionFullscreen();
              }}
              streamMenuMicOn={streamMenuMicOn}
              streamMenuIsFullscreen={sessionFullscreen || !!document.fullscreenElement}
              cloudSessionResumable={controllerCloudSessionResumable}
              cloudResumeTitle={activeSessionGameTitle}
              cloudResumeCoverUrl={controllerCloudResumeCoverUrl}
              onResumeCloudSession={() => {
                void handleResumeFromNavbar();
              }}
              cloudResumeBusy={isResumingNavbarSession}
              onCloseGame={async () => {
                setControllerOverlayOpen(false);
                await sleep(300);
                await releasePointerLockIfNeeded();
                await handleStopStream();
              }}
              userName={authSession?.user.displayName}
              userAvatarUrl={authSession?.user.avatarUrl}
              subscriptionInfo={subscriptionInfo}
              playtimeData={playtime}
              sessionStartedAtMs={sessionStartedAtMs}
              isStreaming={isStreaming}
              settings={{
                resolution: settings.resolution,
                fps: settings.fps,
                codec: settings.codec,
                enableL4S: settings.enableL4S,
                enableCloudGsync: settings.enableCloudGsync,
                microphoneDeviceId: settings.microphoneDeviceId,
                controllerUiSounds: settings.controllerUiSounds,
                controllerBackgroundAnimations: settings.controllerBackgroundAnimations,
                controllerThemeStyle: settings.controllerThemeStyle,
                controllerThemeColor: settings.controllerThemeColor,
                controllerLibraryGameBackdrop: settings.controllerLibraryGameBackdrop,
                autoLoadControllerLibrary: settings.autoLoadControllerLibrary,
                autoFullScreen: settings.autoFullScreen,
                aspectRatio: settings.aspectRatio,
                posterSizeScale: settings.posterSizeScale,
                maxBitrateMbps: settings.maxBitrateMbps,
              }}
              resolutionOptions={getResolutionsByAspectRatio(settings.aspectRatio)}
              fpsOptions={fpsOptions}
              codecOptions={codecOptions}
              aspectRatioOptions={aspectRatioOptions as unknown as string[]}
              onSettingChange={updateSetting}
            />
          </ControllerInStreamShell>
        )}
        {showControllerLaunchLoading && (
          <ControllerStreamLoading
            gameTitle={streamingGame?.title ?? "Game"}
            status={loadingStatus}
            queuePosition={queuePosition}
            adState={effectiveAdState}
            activeAd={activeQueueAd}
            activeAdMediaUrl={activeQueueAdMediaUrl}
            error={
              launchError
                ? {
                    title: launchError.title,
                    description: launchError.description,
                    code: launchError.codeLabel,
                  }
                : undefined
            }
            onAdPlaybackEvent={handleQueueAdPlaybackEvent}
            adPreviewRef={queueAdPreviewRef}
          />
        )}
        {showDesktopLaunchLoading && (
          <StreamLoading
            gameTitle={streamingGame?.title ?? "Game"}
            gameCover={streamingGame?.imageUrl}
            platformStore={streamingStore ?? undefined}
            status={loadingStatus}
            queuePosition={queuePosition}
            adState={effectiveAdState}
            activeAd={activeQueueAd}
            activeAdMediaUrl={activeQueueAdMediaUrl}
            onAdPlaybackEvent={handleQueueAdPlaybackEvent}
            adPreviewRef={queueAdPreviewRef}
            error={
              launchError
                ? {
                    title: launchError.title,
                    description: launchError.description,
                    code: launchError.codeLabel,
                  }
                : undefined
            }
            onCancel={() => {
              if (launchError) {
                void handleDismissLaunchError();
                return;
              }
              void handlePromptedStopStream();
            }}
          />
        )}
        {showControllerHint && streamStatus !== "streaming" && (
          <div className="controller-hint controller-hint--overlay">
            <span>D-pad Navigate</span>
            <span>A Select</span>
            <span>B Back</span>
          </div>
        )}
      </>
    );
  }

  // Main app layout
  return (
    <div className="app-container" style={getAppStyle(settings.posterSizeScale)}>
      {startupRefreshNotice && (
        <div className={`auth-refresh-notice auth-refresh-notice--${startupRefreshNotice.tone}`}>
          {startupRefreshNotice.text}
        </div>
      )}
      {!(settings.controllerMode && currentPage === "library") && (
        <Navbar
          currentPage={currentPage}
          onNavigate={setCurrentPage}
          user={authSession.user}
          subscription={subscriptionInfo}
          activeSession={navbarActiveSession}
          activeSessionGameTitle={activeSessionGameTitle}
          isResumingSession={isResumingNavbarSession}
          isTerminatingSession={isTerminatingNavbarSession}
          onResumeSession={() => {
            void handleResumeFromNavbar();
          }}
          onTerminateSession={() => {
            void handleTerminateNavbarSession();
          }}
          savedAccounts={savedAccounts}
          onSwitchAccount={handleSwitchAccount}
          onRemoveAccount={(userId) => {
            void handleRemoveAccount(userId);
          }}
          onAddAccount={handleAddAccount}
          onLogoutAll={handleLogout}
        />
      )}

      <main className="main-content">
        {currentPage === "home" && (
          <HomePage
            games={filteredGames}
            searchQuery={searchQuery}
            onSearchChange={setSearchQuery}
            onPlayGame={handleInitiatePlay}
            isLoading={isLoadingGames}
            selectedGameId={selectedGameId}
            onSelectGame={setSelectedGameId}
            selectedVariantByGameId={variantByGameId}
            onSelectGameVariant={handleSelectGameVariant}
            filterGroups={catalogFilterGroups}
            selectedFilterIds={catalogSelectedFilterIds}
            onToggleFilter={(filterId) => {
              setCatalogSelectedFilterIds((previous) => previous.includes(filterId) ? previous.filter((value) => value !== filterId) : [...previous, filterId]);
            }}
            sortOptions={catalogSortOptions}
            selectedSortId={catalogSelectedSortId}
            onSortChange={setCatalogSelectedSortId}
            totalCount={catalogTotalCount}
            supportedCount={catalogSupportedCount}
          />
        )}

        {currentPage === "library" && (
          settings.controllerMode ? (
            <ControllerLibraryPage
              games={filteredLibraryGames}
              onPlayGame={handleInitiatePlay}
              isLoading={isLoadingGames}
              selectedGameId={selectedGameId}
              onSelectGame={setSelectedGameId}
              uiSoundsEnabled={settings.controllerUiSounds}
              selectedVariantByGameId={variantByGameId}
              onSelectGameVariant={handleSelectGameVariant}
              favoriteGameIds={settings.favoriteGameIds}
              onToggleFavoriteGame={handleToggleFavoriteGame}
              onOpenSettings={() => setCurrentPage("settings")}
              currentStreamingGame={streamingGame}
              onResumeGame={handlePlayGame}
              cloudSessionResumable={controllerCloudSessionResumable}
              cloudResumeTitle={activeSessionGameTitle}
              cloudResumeCoverUrl={controllerCloudResumeCoverUrl}
              onResumeCloudSession={() => {
                void handleResumeFromNavbar();
              }}
              cloudResumeBusy={isResumingNavbarSession}
              onCloseGame={handlePromptedStopStream}
              userName={authSession?.user.displayName}
              userAvatarUrl={authSession?.user.avatarUrl}
              subscriptionInfo={subscriptionInfo}
              playtimeData={playtime}
              sessionStartedAtMs={sessionStartedAtMs}
              isStreaming={isStreaming}
              settings={{
                resolution: settings.resolution,
                fps: settings.fps,
                codec: settings.codec,
                enableL4S: settings.enableL4S,
                enableCloudGsync: settings.enableCloudGsync,
                microphoneDeviceId: settings.microphoneDeviceId,
                controllerUiSounds: settings.controllerUiSounds,
                controllerBackgroundAnimations: settings.controllerBackgroundAnimations,
                controllerThemeStyle: settings.controllerThemeStyle,
                controllerThemeColor: settings.controllerThemeColor,
                controllerLibraryGameBackdrop: settings.controllerLibraryGameBackdrop,
                autoLoadControllerLibrary: settings.autoLoadControllerLibrary,
                autoFullScreen: settings.autoFullScreen,
                aspectRatio: settings.aspectRatio,
                posterSizeScale: settings.posterSizeScale,
                maxBitrateMbps: settings.maxBitrateMbps,
              }}
              resolutionOptions={getResolutionsByAspectRatio(settings.aspectRatio)}
              fpsOptions={fpsOptions}
              codecOptions={codecOptions}
              aspectRatioOptions={aspectRatioOptions as unknown as string[]}
              onSettingChange={updateSetting}
              onExitControllerMode={handleExitControllerMode}
              onExitApp={handleExitApp}
            />
          ) : (
            <LibraryPage
              games={filteredLibraryGames}
              searchQuery={searchQuery}
              onSearchChange={setSearchQuery}
              onPlayGame={handleInitiatePlay}
              isLoading={isLoadingGames}
              selectedGameId={selectedGameId}
              onSelectGame={setSelectedGameId}
              selectedVariantByGameId={variantByGameId}
              onSelectGameVariant={handleSelectGameVariant}
              libraryCount={libraryGames.length}
              sortOptions={catalogSortOptions.filter((option) => option.id !== "relevance")}
              selectedSortId={catalogSelectedSortId === "relevance" ? "last_played" : catalogSelectedSortId}
              onSortChange={setCatalogSelectedSortId}
            />
          )
        )}

        {currentPage === "settings" && (
          <SettingsPage
            settings={settings}
            regions={regions}
            codecResults={codecResults}
            codecTesting={codecTesting}
            onRunCodecTest={runCodecTest}
            onSettingChange={updateSetting}
          />
        )}
      </main>
      {showControllerHint && (
        <div className="controller-hint">
          <span>D-pad Navigate</span>
          <span>A Select</span>
          <span>B Back</span>
          <span>LB/RB Tabs</span>
        </div>
      )}

      {logoutConfirmModal}
      {removeAccountConfirmModal}
      {queueModalGame && streamStatus === "idle" && (
        <QueueServerSelectModal
          game={queueModalGame}
          initialQueueData={queueModalData}
          onConfirm={handleQueueModalConfirm}
          onCancel={handleQueueModalCancel}
        />
      )}
    </div>
  );
}
