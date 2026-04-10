import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { JSX } from "react";

import type {
  ActiveSessionInfo,
  AuthSession,
  AuthUser,
  GameInfo,
  GameVariant,
  LoginProvider,
  MainToRendererSignalingEvent,
  SessionAdAction,
  SessionAdInfo,
  SessionAdState,
  SessionInfo,
  Settings,
  SubscriptionInfo,
  StreamRegion,
  VideoCodec,
} from "@shared/gfn";
import {
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
import { createStreamDiagnosticsStore } from "./utils/streamDiagnosticsStore";

// UI Components
import { LoginScreen } from "./components/LoginScreen";
import { Navbar } from "./components/Navbar";
import { HomePage } from "./components/HomePage";
import { LibraryPage } from "./components/LibraryPage";
import { ControllerLibraryPage } from "./components/ControllerLibraryPage";
import { SettingsPage } from "./components/SettingsPage";
import { StreamLoading } from "./components/StreamLoading";
import { ControllerStreamLoading } from "./components/ControllerStreamLoading";
import type { QueueAdPlaybackEvent, QueueAdPreviewHandle } from "./components/QueueAdPreview";
import { StreamView } from "./components/StreamView";

const codecOptions: VideoCodec[] = [...USER_FACING_VIDEO_CODEC_OPTIONS];
const DEFAULT_STREAM_PREFERENCES = getDefaultStreamPreferences();
const allResolutionOptions = ["1280x720", "1280x800", "1440x900", "1680x1050", "1920x1080", "1920x1200", "2560x1080", "2560x1440", "2560x1600", "3440x1440", "3840x2160", "3840x2400"];
const fpsOptions = [30, 60, 120, 144, 240];
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
const SESSION_READY_POLL_INTERVAL_MS = 2000;
const SESSION_AD_POLL_INTERVAL_MS = 30000;
const SESSION_AD_PROGRESS_CHECK_INTERVAL_MS = 1000;
const SESSION_AD_START_TIMEOUT_MS = 30000;
const SESSION_AD_FORCE_PLAY_TIMEOUT_MS = 10000;
const SESSION_AD_STUCK_TIMEOUT_MS = 30000;
const SESSION_READY_TIMEOUT_MS = 180000;
const VARIANT_SELECTION_LOCALSTORAGE_KEY = "opennow.variantByGameId";
const PLAYTIME_RESYNC_INTERVAL_MS = 5 * 60 * 1000;

type GameSource = "main" | "library" | "public";
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

const APP_PAGE_ORDER: AppPage[] = ["home", "library", "settings"];

const isMac = navigator.platform.toLowerCase().includes("mac");

const DEFAULT_SHORTCUTS = {
  shortcutToggleStats: "F3",
  shortcutTogglePointerLock: "F8",
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
  return game.variants[0]?.id ?? game.id;
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
    connectedGamepads: 0,
    resolution: "",
    codec: "",
    isHdr: false,
    bitrateKbps: 0,
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
    inputQueueDropCount: 0,
    inputQueueMaxSchedulingDelayMs: 0,
    lagReason: "unknown",
    lagReasonDetail: "Waiting for stream stats",
    gpuType: "",
    serverRegion: "",
    decoderPressureActive: false,
    decoderRecoveryAttempts: 0,
    decoderRecoveryAction: "none",
    micState: "uninitialized",
    micEnabled: false,
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

function formatRemainingPlaytimeFromSubscription(
  subscription: SubscriptionInfo | null,
  consumedHours = 0,
): string {
  if (!subscription) {
    return "--";
  }
  if (subscription.isUnlimited) {
    return "Unlimited";
  }

  const baseHours = Number.isFinite(subscription.remainingHours) ? subscription.remainingHours : 0;
  const safeHours = Math.max(0, baseHours - Math.max(0, consumedHours));
  const totalMinutes = Math.round(safeHours * 60);
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;

  if (hours > 0) {
    return `${hours}h ${minutes.toString().padStart(2, "0")}m`;
  }
  return `${minutes}m`;
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

export function App(): JSX.Element {

  // Auth State
  const [authSession, setAuthSession] = useState<AuthSession | null>(null);
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

  // Games State
  const [games, setGames] = useState<GameInfo[]>([]);
  const [libraryGames, setLibraryGames] = useState<GameInfo[]>([]);
  const [source, setSource] = useState<GameSource>("main");
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedGameId, setSelectedGameId] = useState("");
  const [variantByGameId, setVariantByGameId] = useState<Record<string, string>>({});
  const [isLoadingGames, setIsLoadingGames] = useState(false);

  // Settings State
  const [settings, setSettings] = useState<Settings>({
    resolution: "1920x1080",
    aspectRatio: "16:9",
    fps: 60,
    maxBitrateMbps: 75,
    codec: DEFAULT_STREAM_PREFERENCES.codec,
    decoderPreference: "auto",
    encoderPreference: "auto",
    colorQuality: DEFAULT_STREAM_PREFERENCES.colorQuality,
    region: "",
    clipboardPaste: false,
    mouseSensitivity: 1,
    mouseAcceleration: 1,
    shortcutToggleStats: DEFAULT_SHORTCUTS.shortcutToggleStats,
    shortcutTogglePointerLock: DEFAULT_SHORTCUTS.shortcutTogglePointerLock,
    shortcutStopStream: DEFAULT_SHORTCUTS.shortcutStopStream,
    shortcutToggleAntiAfk: DEFAULT_SHORTCUTS.shortcutToggleAntiAfk,
    shortcutToggleMicrophone: DEFAULT_SHORTCUTS.shortcutToggleMicrophone,
    shortcutScreenshot: DEFAULT_SHORTCUTS.shortcutScreenshot,
    shortcutToggleRecording: DEFAULT_SHORTCUTS.shortcutToggleRecording,
    microphoneMode: "disabled",
    microphoneDeviceId: "",
    hideStreamButtons: false,
    showStatsOnLaunch: false,
    controllerMode: false,
    controllerUiSounds: false,
    controllerBackgroundAnimations: false,
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
  });
  const [settingsLoaded, setSettingsLoaded] = useState(false);
  const [regions, setRegions] = useState<StreamRegion[]>([]);
  const [subscriptionInfo, setSubscriptionInfo] = useState<SubscriptionInfo | null>(null);
  const diagnosticsStoreRef = useRef<ReturnType<typeof createStreamDiagnosticsStore> | null>(null);
  const diagnosticsStore =
    diagnosticsStoreRef.current ?? (diagnosticsStoreRef.current = createStreamDiagnosticsStore(defaultDiagnostics()));

  // Stream State
  const [session, setSession] = useState<SessionInfo | null>(null);
  const [streamStatus, setStreamStatus] = useState<StreamStatus>("idle");
  const [showStatsOverlay, setShowStatsOverlay] = useState(false);
  const [antiAfkEnabled, setAntiAfkEnabled] = useState(false);
  const [escHoldReleaseIndicator, setEscHoldReleaseIndicator] = useState<{ visible: boolean; progress: number }>({
    visible: false,
    progress: 0,
  });
  const [exitPrompt, setExitPrompt] = useState<ExitPromptState>({ open: false, gameTitle: "Game" });
  const [streamingGame, setStreamingGame] = useState<GameInfo | null>(null);
  const [streamingStore, setStreamingStore] = useState<string | null>(null);
  const [queuePosition, setQueuePosition] = useState<number | undefined>();
  const [navbarActiveSession, setNavbarActiveSession] = useState<ActiveSessionInfo | null>(null);
  const [isResumingNavbarSession, setIsResumingNavbarSession] = useState(false);
  const [launchError, setLaunchError] = useState<LaunchErrorState | null>(null);
  const [sessionStartedAtMs, setSessionStartedAtMs] = useState<number | null>(null);
  const [streamWarning, setStreamWarning] = useState<StreamWarningState | null>(null);
  const [activeQueueAdId, setActiveQueueAdId] = useState<string | null>(null);

  const { playtime, startSession: startPlaytimeSession, endSession: endPlaytimeSession } = usePlaytime();
  const sessionElapsedSeconds = useElapsedSeconds(sessionStartedAtMs, streamStatus === "streaming");
  const isStreaming = streamStatus === "streaming";

  const controllerOverlayOpenRef = useRef(false);

  const resetStatsOverlayToPreference = useCallback((): void => {
    setShowStatsOverlay(settings.showStatsOnLaunch);
  }, [settings.showStatsOnLaunch]);

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

  useEffect(() => {
    let raf = 0;
    const prev = { pressed: false };
    const tick = () => {
      try {
        if (streamStatus !== "streaming") {
          prev.pressed = false;
          raf = window.requestAnimationFrame(tick);
          return;
        }
        const pads = navigator.getGamepads ? navigator.getGamepads() : [];
        const pad = Array.from(pads).find((p) => p && p.connected) ?? null;
        if (!pad) {
          prev.pressed = false;
          raf = window.requestAnimationFrame(tick);
          return;
        }
        // Meta/Home button only: button 16 (standard)
        const metaPressed = Boolean(pad.buttons[16]?.pressed);
        if (metaPressed && !prev.pressed) {
          setControllerOverlayOpen((v) => !v);
        }
        prev.pressed = metaPressed;
      } catch {
        // ignore
      }
      raf = window.requestAnimationFrame(tick);
    };
    raf = window.requestAnimationFrame(tick);
    return () => { if (raf) window.cancelAnimationFrame(raf); };
  }, [streamStatus]);

  // Refs
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const clientRef = useRef<GfnWebRtcClient | null>(null);
  const sessionRef = useRef<SessionInfo | null>(null);
  const hasInitializedRef = useRef(false);
  const regionsRequestRef = useRef(0);
  const launchInFlightRef = useRef(false);
  const launchAbortRef = useRef(false);
  const streamStatusRef = useRef<StreamStatus>(streamStatus);
  const exitPromptResolverRef = useRef<((confirmed: boolean) => void) | null>(null);
  const adReportQueueRef = useRef<Promise<void>>(Promise.resolve());
  const adReportStateRef = useRef<Record<string, SessionAdAction>>({});
  const adMetricsRef = useRef<Record<string, QueueAdMetrics>>({});
  const adMediaUrlCacheRef = useRef<Record<string, string>>({});
  const queueAdPlaybackRef = useRef<{ adId: string; phase: "playing" | "finishing" } | null>(null);
  const queueAdPreviewRef = useRef<QueueAdPreviewHandle | null>(null);
  const activeQueueAdIdRef = useRef<string | null>(null);
  const adForcePlayTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const adStartTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const adProgressIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const adLastProgressTsRef = useRef<number | null>(null);
  // Stable ref so timer callbacks can call the latest reportQueueAdAction without
  // capturing a stale closure (auth state can change between scheduling and firing).
  const reportQueueAdActionRef = useRef<
    (adId: string, action: SessionAdAction, options?: QueueAdReportOptions) => void
  >(() => {});

  useEffect(() => {
    controllerOverlayOpenRef.current = controllerOverlayOpen;
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
    setSession(null);
    setStreamStatus("idle");
    setQueuePosition(undefined);
    setSessionStartedAtMs(null);
    setStreamWarning(null);
    setEscHoldReleaseIndicator({ visible: false, progress: 0 });
    resetStatsOverlayToPreference();
    diagnosticsStore.set(defaultDiagnostics());

    if (!options?.keepStreamingContext) {
      setStreamingGame(null);
      setStreamingStore(null);
    }

    if (!options?.keepLaunchError) {
      setLaunchError(null);
    }
  }, [diagnosticsStore, resetStatsOverlayToPreference]);

  // Session ref sync
  useEffect(() => {
    sessionRef.current = session;
  }, [session]);

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
    return selectedProvider?.streamingServiceUrl ?? "";
  }, [selectedProvider]);

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

  const refreshNavbarActiveSession = useCallback(async (): Promise<void> => {
    if (!authSession) {
      setNavbarActiveSession(null);
      return;
    }
    const token = authSession.tokens.idToken ?? authSession.tokens.accessToken;
    if (!token || !effectiveStreamingBaseUrl) {
      setNavbarActiveSession(null);
      return;
    }
    try {
      const activeSessions = await window.openNow.getActiveSessions(token, effectiveStreamingBaseUrl);
      const candidate = activeSessions.find((entry) => entry.status === 3 || entry.status === 2) ?? null;
      setNavbarActiveSession(candidate);
    } catch (error) {
      console.warn("Failed to refresh active sessions:", error);
    }
  }, [authSession, effectiveStreamingBaseUrl]);

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

        // Update isInitializing FIRST so UI knows we're done loading
        setIsInitializing(false);
        setProviders(providerList);
        setAuthSession(persistedSession);

        const activeProviderId = persistedSession?.provider?.idpId ?? providerList[0]?.idpId ?? "";
        setProviderIdpId(activeProviderId);

        if (persistedSession) {
          // Load regions
          const token = persistedSession.tokens.idToken ?? persistedSession.tokens.accessToken;
          const discovered = await window.openNow.getRegions({ token });
          setRegions(discovered);

          try {
            await loadSubscriptionInfo(persistedSession);
          } catch (error) {
            console.warn("Failed to load subscription info:", error);
            setSubscriptionInfo(null);
          }

          // Load games
          try {
            const mainGames = await window.openNow.fetchMainGames({
              token,
              providerStreamingBaseUrl: persistedSession.provider.streamingServiceUrl,
            });
            setGames(mainGames);
            setSource("main");
            setSelectedGameId(mainGames[0]?.id ?? "");
            applyVariantSelections(mainGames);

            // Also load library
            const libGames = await window.openNow.fetchLibraryGames({
              token,
              providerStreamingBaseUrl: persistedSession.provider.streamingServiceUrl,
            });
            setLibraryGames(libGames);
            applyVariantSelections(libGames);
          } catch {
            // Fallback to public games
            const publicGames = await window.openNow.fetchPublicGames();
            setGames(publicGames);
            setSource("public");
            applyVariantSelections(publicGames);
          }
        } else {
          // Load public games for non-logged in users
          const publicGames = await window.openNow.fetchPublicGames();
          setGames(publicGames);
          setSource("public");
          applyVariantSelections(publicGames);
          setSubscriptionInfo(null);
        }
      } catch (error) {
        console.error("Initialization failed:", error);
        setStartupStatusMessage("Session restore failed. Please sign in again.");
        // Always set isInitializing to false even on error
        setIsInitializing(false);
      }
    };

    void initialize();
  }, []);

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
    const stopStream = parseWithFallback(settings.shortcutStopStream, DEFAULT_SHORTCUTS.shortcutStopStream);
    const toggleAntiAfk = parseWithFallback(settings.shortcutToggleAntiAfk, DEFAULT_SHORTCUTS.shortcutToggleAntiAfk);
    const toggleMicrophone = parseWithFallback(settings.shortcutToggleMicrophone, DEFAULT_SHORTCUTS.shortcutToggleMicrophone);
    const screenshot = parseWithFallback(settings.shortcutScreenshot, DEFAULT_SHORTCUTS.shortcutScreenshot);
    const recording = parseWithFallback(settings.shortcutToggleRecording, DEFAULT_SHORTCUTS.shortcutToggleRecording);
    return { toggleStats, togglePointerLock, stopStream, toggleAntiAfk, toggleMicrophone, screenshot, recording };
  }, [
    settings.shortcutToggleStats,
    settings.shortcutTogglePointerLock,
    settings.shortcutStopStream,
    settings.shortcutToggleAntiAfk,
    settings.shortcutToggleMicrophone,
    settings.shortcutScreenshot,
    settings.shortcutToggleRecording,
  ]);

  const requestEscLockedPointerCapture = useCallback(async (target: HTMLVideoElement) => {
    const lockTarget = (target.parentElement as HTMLElement | null) ?? target;
    const requestPointerLockCompat = async (
      options?: { unadjustedMovement?: boolean },
    ): Promise<void> => {
      const maybePromise = lockTarget.requestPointerLock(options as any) as unknown;
      if (maybePromise && typeof (maybePromise as Promise<void>).then === "function") {
        await (maybePromise as Promise<void>);
      }
    };

    if (!document.fullscreenElement) {
      await document.documentElement.requestFullscreen().catch(() => {});
    }

    const nav = navigator as any;
    if (document.fullscreenElement && nav.keyboard?.lock) {
      await nav.keyboard.lock([
        "Escape", "F11", "BrowserBack", "BrowserForward", "BrowserRefresh",
      ]).catch(() => {});
    }

    await requestPointerLockCompat({ unadjustedMovement: true })
      .catch((err: DOMException) => {
        if (err.name === "NotSupportedError") {
          return requestPointerLockCompat();
        }
        throw err;
      })
      .catch(() => {});
  }, []);

  const handleRequestPointerLock = useCallback(() => {
    if (videoRef.current) {
      void requestEscLockedPointerCapture(videoRef.current);
    }
  }, [requestEscLockedPointerCapture]);

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

  // Listen for F11 fullscreen toggle from main process (uses W3C Fullscreen API
  // so navigator.keyboard.lock() can capture Escape in fullscreen)
  useEffect(() => {
    const unsubscribe = window.openNow.onToggleFullscreen(() => {
      if (document.fullscreenElement) {
        document.exitFullscreen().catch(() => {});
      } else {
        document.documentElement.requestFullscreen().catch(() => {});
      }
    });
    return () => unsubscribe();
  }, []);

  // Anti-AFK interval
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
    if (!streamWarning) return;
    const warning = streamWarning;
    const timer = window.setTimeout(() => {
      setStreamWarning((current) => (current === warning ? null : current));
    }, 12000);
    return () => window.clearTimeout(timer);
  }, [streamWarning]);

  // Signaling events
  useEffect(() => {
    const unsubscribe = window.openNow.onSignalingEvent(async (event: MainToRendererSignalingEvent) => {
      console.log(`[App] Signaling event: ${event.type}`, event.type === "offer" ? `(SDP ${event.sdp.length} chars)` : "", event.type === "remote-ice" ? event.candidate : "");
      try {
        if (event.type === "offer") {
          const activeSession = sessionRef.current;
          if (!activeSession) {
            console.warn("[App] Received offer but no active session in sessionRef!");
            return;
          }
          console.log("[App] Active session for offer:", JSON.stringify({
            sessionId: activeSession.sessionId,
            serverIp: activeSession.serverIp,
            signalingServer: activeSession.signalingServer,
            mediaConnectionInfo: activeSession.mediaConnectionInfo,
            iceServersCount: activeSession.iceServers?.length,
          }));

          if (!clientRef.current && videoRef.current && audioRef.current) {
            clientRef.current = new GfnWebRtcClient({
              videoElement: videoRef.current,
              audioElement: audioRef.current,
              microphoneMode: settings.microphoneMode,
              microphoneDeviceId: settings.microphoneDeviceId || undefined,
              mouseSensitivity: settings.mouseSensitivity,
              mouseAcceleration: settings.mouseAcceleration,
              onLog: (line: string) => console.log(`[WebRTC] ${line}`),
              onStats: (stats) => diagnosticsStore.set(stats),
              onEscHoldProgress: (visible, progress) => {
                setEscHoldReleaseIndicator({ visible, progress });
              },
              onTimeWarning: (warning) => {
                setStreamWarning({
                  code: warning.code,
                  message: warningMessage(warning.code),
                  tone: warningTone(warning.code),
                  secondsLeft: warning.secondsLeft,
                });
              },
              onMicStateChange: (state) => {
                console.log(`[App] Mic state: ${state.state}${state.deviceLabel ? ` (${state.deviceLabel})` : ""}`);
              },
            });
            // Auto-start microphone if mode is enabled
            if (settings.microphoneMode !== "disabled") {
              void clientRef.current.startMicrophone();
            }
          }

          if (clientRef.current) {
            await clientRef.current.handleOffer(event.sdp, activeSession, {
              codec: settings.codec,
              colorQuality: settings.colorQuality,
              resolution: settings.resolution,
              fps: settings.fps,
              maxBitrateKbps: settings.maxBitrateMbps * 1000,
            });
            setLaunchError(null);
            setStreamStatus("streaming");
            // Auto-enter fullscreen on stream start if user enabled it
            try {
              if ((settings as any).autoFullScreen) {
                void (window as any).openNow?.setFullscreen?.(true);
              }
            } catch (err) {
              console.warn("Failed to auto-fullscreen on stream start:", err);
            }
          }
        } else if (event.type === "remote-ice") {
          await clientRef.current?.addRemoteCandidate(event.candidate);
        } else if (event.type === "disconnected") {
          console.warn("Signaling disconnected:", event.reason);
          clientRef.current?.dispose();
          clientRef.current = null;
          resetLaunchRuntime();
          launchInFlightRef.current = false;
        } else if (event.type === "error") {
          console.error("Signaling error:", event.message);
        }
      } catch (error) {
        console.error("Signaling event error:", error);
      }
    });

    return () => unsubscribe();
  }, [resetLaunchRuntime, settings]);

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
    if (key === "maxBitrateMbps") {
      try {
        void (clientRef.current as any)?.setMaxBitrateKbps?.((value as number) * 1000);
      } catch {
        // ignore
      }
    }
  }, [settingsLoaded]);

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
    void window.openNow.quitApp().catch((error) => {
      console.warn("Failed to quit application:", error);
    });
  }, []);

  const handleMicrophoneModeChange = useCallback((value: import("@shared/gfn").MicrophoneMode) => {
    // Keep UI responsive while still surfacing persistence failures.
    void updateSetting("microphoneMode", value).catch((error) => {
      console.warn("Failed to persist microphone mode setting:", error);
    });
  }, [updateSetting]);

  // Login handler
  const handleLogin = useCallback(async () => {
    setIsLoggingIn(true);
    setLoginError(null);
    try {
      const session = await window.openNow.login({ providerIdpId: providerIdpId || undefined });
      setAuthSession(session);
      setProviderIdpId(session.provider.idpId);

      // Load regions
      const token = session.tokens.idToken ?? session.tokens.accessToken;
      const discovered = await window.openNow.getRegions({ token });
      setRegions(discovered);

      try {
        await loadSubscriptionInfo(session);
      } catch (error) {
        console.warn("Failed to load subscription info:", error);
        setSubscriptionInfo(null);
      }

      // Load games
      const mainGames = await window.openNow.fetchMainGames({
        token,
        providerStreamingBaseUrl: session.provider.streamingServiceUrl,
      });
      setGames(mainGames);
      setSource("main");
      setSelectedGameId(mainGames[0]?.id ?? "");
      applyVariantSelections(mainGames);

      // Load library
      const libGames = await window.openNow.fetchLibraryGames({
        token,
        providerStreamingBaseUrl: session.provider.streamingServiceUrl,
      });
      setLibraryGames(libGames);
      applyVariantSelections(libGames);
    } catch (error) {
      setLoginError(error instanceof Error ? error.message : "Login failed");
    } finally {
      setIsLoggingIn(false);
    }
  }, [applyVariantSelections, loadSubscriptionInfo, providerIdpId]);

  // Logout handler
  const handleLogout = useCallback(async () => {
    await window.openNow.logout();
    setAuthSession(null);
    setGames([]);
    setLibraryGames([]);
    setVariantByGameId({});
    resetLaunchRuntime();
    setNavbarActiveSession(null);
    setIsResumingNavbarSession(false);
    setSubscriptionInfo(null);
    setCurrentPage("home");
    const publicGames = await window.openNow.fetchPublicGames();
    setGames(publicGames);
    setSource("public");
    applyVariantSelections(publicGames);
  }, [applyVariantSelections, resetLaunchRuntime]);

  // Load games handler
  const loadGames = useCallback(async (targetSource: GameSource) => {
    setIsLoadingGames(true);
    try {
      const token = authSession?.tokens.idToken ?? authSession?.tokens.accessToken;
      const baseUrl = effectiveStreamingBaseUrl;

      let result: GameInfo[] = [];
      if (targetSource === "main" && token) {
        result = await window.openNow.fetchMainGames({ token, providerStreamingBaseUrl: baseUrl });
      } else if (targetSource === "library" && token) {
        result = await window.openNow.fetchLibraryGames({ token, providerStreamingBaseUrl: baseUrl });
        setLibraryGames(result);
        applyVariantSelections(result);
      } else if (targetSource === "public") {
        result = await window.openNow.fetchPublicGames();
      }

      if (targetSource !== "library") {
        setGames(result);
        setSource(targetSource);
        setSelectedGameId(result[0]?.id ?? "");
        applyVariantSelections(result);
      }
    } catch (error) {
      console.error("Failed to load games:", error);
    } finally {
      setIsLoadingGames(false);
    }
  }, [applyVariantSelections, authSession, effectiveStreamingBaseUrl]);

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

  const claimAndConnectSession = useCallback(async (existingSession: ActiveSessionInfo): Promise<void> => {
    const token = authSession?.tokens.idToken ?? authSession?.tokens.accessToken;
    if (!token) {
      throw new Error("Missing token for session resume");
    }
    if (!existingSession.serverIp) {
      throw new Error("Active session is missing server address. Start the game again to create a new session.");
    }

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
      appId: String(existingSession.appId),
      settings: {
        resolution: settings.resolution,
        fps: settings.fps,
        maxBitrateMbps: settings.maxBitrateMbps,
        codec: settings.codec,
        colorQuality: settings.colorQuality,
        keyboardLayout: settings.keyboardLayout,
        gameLanguage: settings.gameLanguage,
        enableL4S: settings.enableL4S,
      },
    });

    console.log("Claimed session:", {
      sessionId: claimed.sessionId,
      signalingServer: claimed.signalingServer,
      signalingUrl: claimed.signalingUrl,
      status: claimed.status,
    });

    await sleep(1000);

    setSession(claimed);
    sessionRef.current = claimed;
    setQueuePosition(undefined);
    setStreamStatus("connecting");
    await window.openNow.connectSignaling({
      sessionId: claimed.sessionId,
      signalingServer: claimed.signalingServer,
      signalingUrl: claimed.signalingUrl,
    });
  }, [authSession, effectiveStreamingBaseUrl, findGameContextForSession, resetStatsOverlayToPreference, settings]);

  // Play game handler
  const handlePlayGame = useCallback(async (game: GameInfo, options?: { bypassGuards?: boolean }) => {
    if (!selectedProvider) return;

    console.log("handlePlayGame entry", {
      title: game.title,
      launchInFlight: launchInFlightRef.current,
      streamStatus,
      bypass: options?.bypassGuards ?? false,
    });

    if (!options?.bypassGuards && (launchInFlightRef.current || streamStatus !== "idle")) {
      console.warn("Ignoring play request: launch already in progress or stream not idle", {
        inFlight: launchInFlightRef.current,
        streamStatus,
      });
      return;
    }

    launchInFlightRef.current = true;
    launchAbortRef.current = false;
    let loadingStep: StreamLoadingStatus = "queue";
    const updateLoadingStep = (next: StreamLoadingStatus): void => {
      loadingStep = next;
      setStreamStatus(next);
    };

    setSessionStartedAtMs(null);
    setStreamWarning(null);
    setLaunchError(null);
    resetStatsOverlayToPreference();
    const selectedVariantId = variantByGameId[game.id] ?? defaultVariantId(game);
    const selectedVariant = getSelectedVariant(game, selectedVariantId);
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
      const matchedGameContext = findSessionContextForAppId(allKnownGames, variantByGameId, numericAppId) ?? {
        game,
        variant: selectedVariant,
      };
      setStreamingGame(matchedGameContext.game);
      setStreamingStore(matchedGameContext.variant?.store ?? null);

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
            }
          }
        } catch (error) {
          console.error("Failed to claim/resume session:", error);
          // Continue to create new session
        }
      }

      // Create new session
      const newSession = await window.openNow.createSession({
        token: token || undefined,
        streamingBaseUrl: effectiveStreamingBaseUrl,
        appId,
        internalTitle: game.title,
        accountLinked: game.playType !== "INSTALL_TO_PLAY",
        zone: "prod",
        settings: {
          resolution: settings.resolution,
          fps: settings.fps,
          maxBitrateMbps: settings.maxBitrateMbps,
          codec: settings.codec,
          colorQuality: settings.colorQuality,
          keyboardLayout: settings.keyboardLayout,
          gameLanguage: settings.gameLanguage,
          enableL4S: settings.enableL4S,
        },
      });

      setSession(newSession);
      setQueuePosition(newSession.queuePosition);

      // Poll for readiness.
      // Queue mode: no timeout - users wait indefinitely and see position updates.
      // Setup/Starting mode: 180s timeout applies while machine is being allocated.
      let finalSession: SessionInfo | null = null;
      let latestSession = newSession;
      let isInQueueMode = isSessionInQueue(newSession);
      let setupTimeoutStartAt: number | null = isInQueueMode ? null : Date.now();
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
        });

        if (launchAbortRef.current) {
          return;
        }

        const mergedSession = mergePolledSessionState(latestSession, polled);
        latestSession = mergedSession;

        setSession(mergedSession);
        setQueuePosition(mergedSession.queuePosition);

        // Check if queue just cleared - transition from queue mode to setup mode
        const wasInQueueMode = isInQueueMode;
        isInQueueMode = isSessionInQueue(mergedSession);
        if (wasInQueueMode && !isInQueueMode) {
          // Queue just cleared, start timeout counting from now.
          setupTimeoutStartAt = Date.now();
        }

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

        // Only check timeout when NOT in queue mode (i.e., during setup/starting)
        if (!isInQueueMode && setupTimeoutStartAt !== null && Date.now() - setupTimeoutStartAt >= SESSION_READY_TIMEOUT_MS) {
          throw new Error(`Session did not become ready in time (${Math.round(SESSION_READY_TIMEOUT_MS / 1000)}s)`);
        }
      }

      // finalSession is guaranteed to be set here (we only exit the loop via break when session is ready)
      // Timeout only applies during setup/starting phase, not during queue wait

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

      await window.openNow.connectSignaling({
        sessionId: sessionToConnect.sessionId,
        signalingServer: sessionToConnect.signalingServer,
        signalingUrl: sessionToConnect.signalingUrl,
      });
    } catch (error) {
      if (launchAbortRef.current) {
        return;
      }
      console.error("Launch failed:", error);
      setLaunchError(toLaunchErrorState(error, loadingStep));
      await window.openNow.disconnectSignaling().catch(() => {});
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
    claimAndConnectSession,
    effectiveStreamingBaseUrl,
    refreshNavbarActiveSession,
    resetLaunchRuntime,
    resetStatsOverlayToPreference,
    selectedProvider,
    settings,
    streamStatus,
    variantByGameId,
  ]);

  const handleResumeFromNavbar = useCallback(async () => {
    if (!selectedProvider || !navbarActiveSession || isResumingNavbarSession) {
      return;
    }
    if (launchInFlightRef.current || streamStatus !== "idle") {
      return;
    }

    launchInFlightRef.current = true;
    setIsResumingNavbarSession(true);
    let loadingStep: StreamLoadingStatus = "setup";
    const updateLoadingStep = (next: StreamLoadingStatus): void => {
      loadingStep = next;
      setStreamStatus(next);
    };

    setLaunchError(null);
    setQueuePosition(undefined);
    setSessionStartedAtMs(null);
    setStreamWarning(null);
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
      await claimAndConnectSession(navbarActiveSession);
      setNavbarActiveSession(null);
    } catch (error) {
      console.error("Navbar resume failed:", error);
      setLaunchError(toLaunchErrorState(error, loadingStep));
      await window.openNow.disconnectSignaling().catch(() => {});
      clientRef.current?.dispose();
      clientRef.current = null;
      resetLaunchRuntime({ keepLaunchError: true });
      void refreshNavbarActiveSession();
    } finally {
      launchInFlightRef.current = false;
      setIsResumingNavbarSession(false);
    }
  }, [
    claimAndConnectSession,
    isResumingNavbarSession,
    navbarActiveSession,
    findGameContextForSession,
    refreshNavbarActiveSession,
    resetLaunchRuntime,
    resetStatsOverlayToPreference,
    selectedProvider,
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
      await window.openNow.disconnectSignaling();

      const current = sessionRef.current;
      if (current) {
        const token = authSession?.tokens.idToken ?? authSession?.tokens.accessToken;
        await window.openNow.stopSession({
          token: token || undefined,
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
  }, [authSession, endPlaytimeSession, refreshNavbarActiveSession, resetLaunchRuntime, resolveExitPrompt, streamingGame]);

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
    await window.openNow.disconnectSignaling().catch(() => {});
    clientRef.current?.dispose();
    clientRef.current = null;
    resetLaunchRuntime();
    void refreshNavbarActiveSession();
  }, [refreshNavbarActiveSession, resetLaunchRuntime]);

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
      setEscHoldReleaseIndicator({ visible: false, progress: 0 });
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
            setEscHoldReleaseIndicator({ visible: false, progress: 0 });
          } else {
            void requestEscLockedPointerCapture(videoRef.current);
          }
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
    requestEscLockedPointerCapture,
    settings.clipboardPaste,
    shortcuts,
    streamStatus,
  ]);

  // Filter games by search
  const filteredGames = useMemo(() => {
    const query = searchQuery.trim().toLowerCase();
    if (!query) return games;
    return games.filter((g) => g.title.toLowerCase().includes(query));
  }, [games, searchQuery]);

  const filteredLibraryGames = useMemo(() => {
    const query = searchQuery.trim().toLowerCase();
    if (!query) return libraryGames;
    return libraryGames.filter((g) => g.title.toLowerCase().includes(query));
  }, [libraryGames, searchQuery]);

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
    && (showLaunchErrorOverlay || (streamStatus !== "idle" && streamStatus !== "streaming" && streamStatus !== "connecting"));
  const showDesktopLaunchLoading =
    !isSwitchingGame
    && !settings.controllerMode
    && (showLaunchErrorOverlay || (streamStatus !== "idle" && streamStatus !== "streaming"));
  const consumedHours =
    streamStatus === "streaming"
      ? Math.floor(sessionElapsedSeconds / 60) / 60
      : 0;
  const remainingPlaytimeText = formatRemainingPlaytimeFromSubscription(subscriptionInfo, consumedHours);

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
              stopStream: formatShortcutForDisplay(settings.shortcutStopStream, isMac),
              toggleMicrophone: formatShortcutForDisplay(settings.shortcutToggleMicrophone, isMac),
              screenshot: shortcuts.screenshot.canonical,
              recording: shortcuts.recording.canonical,
            }}
            hideStreamButtons={settings.hideStreamButtons}
            serverRegion={session?.serverIp}
            antiAfkEnabled={antiAfkEnabled}
            escHoldReleaseIndicator={escHoldReleaseIndicator}
            exitPrompt={exitPrompt}
            sessionStartedAtMs={sessionStartedAtMs}
            sessionCounterEnabled={settings.sessionCounterEnabled}
            sessionClockShowEveryMinutes={settings.sessionClockShowEveryMinutes}
            sessionClockShowDurationSeconds={settings.sessionClockShowDurationSeconds}
            streamWarning={streamWarning}
            isConnecting={streamStatus === "connecting"}
            isStreaming={isStreaming}
            gameTitle={streamingGame?.title ?? "Game"}
            platformStore={streamingStore ?? undefined}
            onToggleFullscreen={() => {
              if (document.fullscreenElement) {
                document.exitFullscreen().catch(() => {});
              } else {
                document.documentElement.requestFullscreen().catch(() => {});
              }
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
          />
        )}
        {isSwitchingGame && settings.controllerMode && streamStatus !== "connecting" && (
          <ControllerStreamLoading
            gameTitle={pendingSwitchGameTitle ?? streamingGame?.title ?? "Game"}
            gamePoster={pendingSwitchGameCover ?? streamingGame?.imageUrl}
            gameDescription={pendingSwitchGameDescription ?? streamingGame?.description}
            status={switchingPhase === "cleaning" ? "setup" : "starting"}
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
            playtimeData={playtime}
            gameId={pendingSwitchGameId ?? streamingGame?.id}
            enableBackgroundAnimations={settings.controllerBackgroundAnimations}
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
          <div className="controller-overlay">
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
              onCloseGame={async () => {
                setControllerOverlayOpen(false);
                // allow overlay close animation to play
                await sleep(300);
                await releasePointerLockIfNeeded();
                await handleStopStream();
              }}
              pendingSwitchGameCover={pendingSwitchGameCover}
              userName={authSession?.user.displayName}
              userAvatarUrl={authSession?.user.avatarUrl}
              subscriptionInfo={subscriptionInfo}
              playtimeData={playtime}
              sessionStartedAtMs={sessionStartedAtMs}
              isStreaming={isStreaming}
              sessionCounterEnabled={settings.sessionCounterEnabled}
              settings={{
                resolution: settings.resolution,
                fps: settings.fps,
                codec: settings.codec,
                enableL4S: settings.enableL4S,
                microphoneDeviceId: settings.microphoneDeviceId,
                controllerUiSounds: settings.controllerUiSounds,
                controllerBackgroundAnimations: settings.controllerBackgroundAnimations,
                autoLoadControllerLibrary: settings.autoLoadControllerLibrary,
                autoFullScreen: settings.autoFullScreen,
                aspectRatio: settings.aspectRatio,
                maxBitrateMbps: settings.maxBitrateMbps,
              }}
              resolutionOptions={getResolutionsByAspectRatio(settings.aspectRatio)}
              fpsOptions={fpsOptions}
              codecOptions={codecOptions}
              aspectRatioOptions={aspectRatioOptions as unknown as string[]}
              onSettingChange={updateSetting}
            />
          </div>
        )}
        {showControllerLaunchLoading && (
          <ControllerStreamLoading
            gameTitle={streamingGame?.title ?? "Game"}
            gamePoster={streamingGame?.imageUrl}
            gameDescription={streamingGame?.description}
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
            playtimeData={playtime}
            gameId={streamingGame?.id}
            enableBackgroundAnimations={settings.controllerBackgroundAnimations}
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
    <div className="app-container">
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
          onResumeSession={() => {
            void handleResumeFromNavbar();
          }}
          onLogout={handleLogout}
        />
      )}

      <main className="main-content">
        {currentPage === "home" && (
          <HomePage
            games={filteredGames}
            source={source}
            onSourceChange={loadGames}
            searchQuery={searchQuery}
            onSearchChange={setSearchQuery}
            onPlayGame={handlePlayGame}
            isLoading={isLoadingGames}
            selectedGameId={selectedGameId}
            onSelectGame={setSelectedGameId}
            selectedVariantByGameId={variantByGameId}
            onSelectGameVariant={handleSelectGameVariant}
          />
        )}

        {currentPage === "library" && (
          settings.controllerMode ? (
            <ControllerLibraryPage
              games={filteredLibraryGames}
              onPlayGame={handlePlayGame}
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
              onCloseGame={handlePromptedStopStream}
              pendingSwitchGameCover={pendingSwitchGameCover}
              userName={authSession?.user.displayName}
              userAvatarUrl={authSession?.user.avatarUrl}
              subscriptionInfo={subscriptionInfo}
              playtimeData={playtime}
              sessionStartedAtMs={sessionStartedAtMs}
              isStreaming={isStreaming}
              sessionCounterEnabled={settings.sessionCounterEnabled}
              settings={{
                resolution: settings.resolution,
                fps: settings.fps,
                codec: settings.codec,
                enableL4S: settings.enableL4S,
                microphoneDeviceId: settings.microphoneDeviceId,
                controllerUiSounds: settings.controllerUiSounds,
                controllerBackgroundAnimations: settings.controllerBackgroundAnimations,
                autoLoadControllerLibrary: settings.autoLoadControllerLibrary,
                autoFullScreen: settings.autoFullScreen,
                aspectRatio: settings.aspectRatio,
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
              onPlayGame={handlePlayGame}
              isLoading={isLoadingGames}
              selectedGameId={selectedGameId}
              onSelectGame={setSelectedGameId}
              selectedVariantByGameId={variantByGameId}
              onSelectGameVariant={handleSelectGameVariant}
            />
          )
        )}

        {currentPage === "settings" && (
          <SettingsPage
            settings={settings}
            regions={regions}
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
    </div>
  );
}
