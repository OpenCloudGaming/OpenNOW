import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { CSSProperties, JSX } from "react";
import { createPortal } from "react-dom";

import type {
  ActiveSessionInfo,
  AuthSession,
  CatalogBrowseResult,
  CatalogFilterGroup,
  CatalogSortOption,
  ExistingSessionStrategy,
  GameInfo,
  GamePanelResult,
  LoginProvider,
  MainToRendererSignalingEvent,
  SessionInfo,
  SessionStopRequest,
  SavedAccount,
  Settings,
  SubscriptionInfo,
  SignalingConnectRequest,
  StreamSettings,
  StreamRegion,
  PrintedWasteQueueData,
} from "@shared/gfn";
import {
  buildNativeStreamerSessionContext,
  DEFAULT_KEYBOARD_LAYOUT,
  getDefaultStreamPreferences,
  isSessionAdsRequired,
} from "@shared/gfn";
import { GfnWebRtcClient } from "./gfn/webrtcClient";
import { formatShortcutForDisplay, isShortcutMatch, normalizeShortcut } from "./shortcuts";
import { useElapsedSeconds } from "./utils/useElapsedSeconds";
import { useQueueAdRuntime } from "./hooks/useQueueAdRuntime";
import { usePlaytime } from "./utils/usePlaytime";
import { createStreamDiagnosticsStore } from "./utils/streamDiagnosticsStore";
import type {
  LaunchErrorState,
  LocalSessionTimerWarningState,
  StreamLoadingStatus,
  StreamStatus,
  StreamWarningState,
} from "./lib/appTypes";
import { loadCatalogPreferences, saveCatalogPreferences, VARIANT_SELECTION_LOCALSTORAGE_KEY } from "./lib/catalogPreferences";
import { loadStoredCodecResults, saveStoredCodecResults, testCodecSupport, type CodecTestResult } from "./lib/codecDiagnostics";
import {
  areStringArraysEqual,
  defaultVariantId,
  findSessionContextForAppId,
  getSelectedVariant,
  isNumericId,
  matchesGameSearch,
  mergeVariantSelections,
  parseNumericId,
  sortLibraryGames,
} from "./lib/gameCatalog";
import { chooseAccountLinked, getEpicOwnershipLaunchError } from "./lib/launchOwnership";
import { hasAnyEligiblePrintedWasteZone, isAllianceStreamingBaseUrl } from "./lib/printedWaste";
import {
  mergePolledSessionState,
  normalizeMembershipTier,
  shouldUseQueueAdPolling,
} from "./lib/queueAds";
import { clearRuntimeSnapshot, loadRuntimeSnapshot, saveRuntimeSnapshot, type RuntimeSnapshot } from "./lib/runtimeSnapshot";
import {
  getSessionLimitSecondsForTier,
  getLocalSessionTimerWarning,
  hasCrossedWarningThreshold,
  shouldShowFreeTierSessionWarnings,
  warningMessage,
  warningTone,
} from "./lib/sessionWarnings";
import {
  isSessionInQueue,
  isSessionReadyForConnect,
  streamStatusToLoadingStage,
  toLaunchErrorState,
  toLoadingStatus,
} from "./lib/sessionState";
import { defaultDiagnostics, mergeNativeStreamStats } from "./lib/streamDiagnostics";
import { applyAccentColor } from "./lib/uiCustomization";
import { useTranslation } from "./i18n";

// UI Components
import { LoginScreen } from "./components/LoginScreen";
import { Navbar } from "./components/Navbar";
import { HomePage } from "./components/HomePage";
import { LibraryPage } from "./components/LibraryPage";
import { SettingsPage } from "./components/SettingsPage";
import { StreamLoading } from "./components/StreamLoading";
import { StreamView } from "./components/StreamView";
import { QueueServerSelectModal } from "./components/QueueServerSelectModal";

const DEFAULT_STREAM_PREFERENCES = getDefaultStreamPreferences();

type AppStyle = CSSProperties & {
  "--game-poster-scale"?: string;
};

function getAppStyle(posterSizeScale: number): AppStyle {
  return {
    "--game-poster-scale": String(posterSizeScale),
  };
}

function isNvidiaProvider(provider: LoginProvider | null | undefined): boolean {
  return (provider?.code ?? "").trim().toUpperCase() === "NVIDIA";
}

const SESSION_READY_POLL_INTERVAL_MS = 2000;
const SESSION_AD_POLL_INTERVAL_MS = 30000;
const PLAYTIME_RESYNC_INTERVAL_MS = 5 * 60 * 1000;
const FREE_TIER_30_MIN_WARNING_SECONDS = 30 * 60;
const FREE_TIER_15_MIN_WARNING_SECONDS = 15 * 60;
const FREE_TIER_FINAL_MINUTE_WARNING_SECONDS = 60;
const STREAM_WARNING_VISIBILITY_MS = 15 * 1000;

type AppPage = "home" | "library" | "settings";
type ExitPromptState = { open: boolean; gameTitle: string };
type SignalingRecoveryState = {
  attemptCount: number;
  inFlight: Promise<boolean> | null;
  explicitShutdown: boolean;
  appId: number | null;
  generation: number;
};

const RECOVERABLE_STREAM_STATUSES: readonly StreamStatus[] = ["streaming"];
const SIGNALING_RECOVERY_ATTEMPT_DELAYS_MS = [0, 3000] as const;
const SIGNALING_RECOVERY_STABLE_RESET_DELAY_MS = 15000;
const SIGNALING_REMOTE_ICE_GRACE_MS = 5000;
const ICE_DISCONNECTED_RECOVERY_GRACE_MS = 7000;

const isMac = navigator.platform.toLowerCase().includes("mac");

function isExpectedNativeSessionClose(reason: string): boolean {
  const normalized = reason.trim().toLowerCase();
  return normalized === "socket closed" || normalized === "signaling disconnected: socket closed";
}

function gameIdentityMatches(left: GameInfo, right: GameInfo): boolean {
  if (left.uuid && right.uuid && left.uuid === right.uuid) return true;
  if (left.id && right.id && left.id === right.id) return true;
  if (left.launchAppId && right.launchAppId && left.launchAppId === right.launchAppId) return true;
  return left.title.trim().length > 0 && left.title.localeCompare(right.title, undefined, { sensitivity: "accent" }) === 0;
}

function getLibrarySelectedVariantId(storeGame: GameInfo, libraryGames: GameInfo[]): string | undefined {
  const libraryGame = libraryGames.find((candidate) => gameIdentityMatches(storeGame, candidate));
  const libraryVariant = libraryGame?.variants.find((variant) => variant.librarySelected)
    ?? libraryGame?.variants.find((variant) => variant.inLibrary)
    ?? libraryGame?.variants[0];
  if (!libraryVariant) return undefined;

  const sameIdVariant = storeGame.variants.find((variant) => variant.id === libraryVariant.id);
  if (sameIdVariant) return sameIdVariant.id;

  const sameStoreVariant = storeGame.variants.find((variant) => variant.store.localeCompare(libraryVariant.store, undefined, { sensitivity: "accent" }) === 0);
  return sameStoreVariant?.id;
}

function flattenStorePanelGames(panels: GamePanelResult[]): GameInfo[] {
  return panels.flatMap((panel) => panel.sections.flatMap((section) => section.games));
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

export function App(): JSX.Element {
  const { locale, t } = useTranslation();

  // Auth State
  const [authSession, setAuthSession] = useState<AuthSession | null>(null);
  const [savedAccounts, setSavedAccounts] = useState<SavedAccount[]>([]);
  const [providers, setProviders] = useState<LoginProvider[]>([]);
  const [providerIdpId, setProviderIdpId] = useState("");
  const [isLoggingIn, setIsLoggingIn] = useState(false);
  const [loginError, setLoginError] = useState<string | null>(null);
  const [isInitializing, setIsInitializing] = useState(true);
  const [startupStatusMessage, setStartupStatusMessage] = useState(() => t("auth.status.restoringSavedSession"));
  const [startupRefreshNotice, setStartupRefreshNotice] = useState<{
    tone: "success" | "warn";
    text: string;
  } | null>(null);

  // Navigation
  const [currentPage, setCurrentPage] = useState<AppPage>("home");
  const [sessionFullscreen, setSessionFullscreenState] = useState(false);

  // Games State
  const [games, setGames] = useState<GameInfo[]>([]);
  const [featuredGames, setFeaturedGames] = useState<GameInfo[]>([]);
  const [storePanels, setStorePanels] = useState<GamePanelResult[]>([]);
  const [libraryGames, setLibraryGames] = useState<GameInfo[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedGameId, setSelectedGameId] = useState("");
  const [variantByGameId, setVariantByGameId] = useState<Record<string, string>>({});
  const [isLoadingCatalog, setIsLoadingCatalog] = useState(false);
  const [isLoadingLibrary, setIsLoadingLibrary] = useState(false);
  const [isLoadingStorePanels, setIsLoadingStorePanels] = useState(false);
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
    nativeVideoBackend: "auto",
    nativeStreamerExecutablePath: "",
    nativeCloudGsyncMode: "auto",
    nativeD3dFullscreenMode: "auto",
    nativeExternalRenderer: true,
    showNativeStreamerStats: false,
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
    appAccentColor: "green",
    controllerMode: false,
    autoFullScreen: false,
    favoriteGameIds: [],
    sessionCounterEnabled: false,
    showSessionTimeRemainingInStatsOverlay: false,
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

  // Stream State
  const [session, setSession] = useState<SessionInfo | null>(null);
  const [streamStatus, setStreamStatus] = useState<StreamStatus>("idle");
  const [showStatsOverlay, setShowStatsOverlay] = useState(false);
  const [antiAfkEnabled, setAntiAfkEnabled] = useState(false);
  const [antiAfkAckNonce, setAntiAfkAckNonce] = useState(0);
  const [exitPrompt, setExitPrompt] = useState<ExitPromptState>({ open: false, gameTitle: t("app.labels.game") });
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
  const previousFreeTierRemainingSecondsRef = useRef<number | null>(null);

  const { playtime, startSession: startPlaytimeSession, endSession: endPlaytimeSession } = usePlaytime();
  const sessionElapsedSeconds = useElapsedSeconds(sessionStartedAtMs, streamStatus === "streaming");
  const isStreaming = streamStatus === "streaming";
  const freeTierSessionWarningsActive =
    isStreaming && sessionStartedAtMs !== null && shouldShowFreeTierSessionWarnings(subscriptionInfo);
  const sessionLimitTier = useMemo(() => {
    const subscriptionTier = normalizeMembershipTier(subscriptionInfo?.membershipTier);
    const authTier = normalizeMembershipTier(authSession?.user.membershipTier);
    return subscriptionTier ?? authTier;
  }, [authSession?.user.membershipTier, subscriptionInfo?.membershipTier]);
  const sessionLimitSeconds = getSessionLimitSecondsForTier(sessionLimitTier);
  const sessionTimeRemainingSeconds = isStreaming && sessionStartedAtMs !== null && sessionLimitSeconds !== null
    ? Math.max(0, sessionLimitSeconds - sessionElapsedSeconds)
    : null;
  const freeTierSessionRemainingSeconds = freeTierSessionWarningsActive
    ? sessionTimeRemainingSeconds
    : null;
  const visibleLocalSessionTimerWarning = useMemo(() => {
    if (localSessionTimerWarning === null || freeTierSessionRemainingSeconds === null) {
      return null;
    }

    return getLocalSessionTimerWarning(t, localSessionTimerWarning.stage, freeTierSessionRemainingSeconds);
  }, [freeTierSessionRemainingSeconds, localSessionTimerWarning, locale, t]);
  const streamWarning = useMemo(() => {
    if (visibleLocalSessionTimerWarning?.tone === "critical") {
      return visibleLocalSessionTimerWarning;
    }
    return remoteStreamWarning ?? visibleLocalSessionTimerWarning;
  }, [remoteStreamWarning, visibleLocalSessionTimerWarning]);

  const codecTestPromiseRef = useRef<Promise<CodecTestResult[] | null> | null>(null);
  const codecStartupTestAttemptedRef = useRef(false);
  const navbarSessionActionInFlightRef = useRef<"resume" | "terminate" | null>(null);
  const nativeStreamingRef = useRef(false);
  const streamingGameRef = useRef<GameInfo | null>(null);

  useEffect(() => {
    streamingGameRef.current = streamingGame;
  }, [streamingGame]);

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

  const [streamVolume, setStreamVolume] = useState(1);
  const [streamMicLevel, setStreamMicLevel] = useState(1);
  // Refs
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const clientRef = useRef<GfnWebRtcClient | null>(null);
  const isStreamingRef = useRef(streamStatus === "streaming");

  useEffect(() => {
    isStreamingRef.current = streamStatus === "streaming";
  }, [streamStatus]);

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
  const storePanelsLoadedContextRef = useRef("");
  const storePanelsLoadIdRef = useRef(0);
  const runtimeDataLoadIdRef = useRef(0);
  const signalingRecoveryRef = useRef<SignalingRecoveryState>({
    attemptCount: 0,
    inFlight: null,
    explicitShutdown: false,
    appId: null,
    generation: 0,
  });
  const exitPromptResolverRef = useRef<((confirmed: boolean) => void) | null>(null);


  const resetStorePanels = useCallback((): void => {
    storePanelsLoadIdRef.current += 1;
    storePanelsLoadedContextRef.current = "";
    setStorePanels([]);
    setIsLoadingStorePanels(false);
  }, []);


  const applyVariantSelections = useCallback((catalog: GameInfo[]): void => {
    setVariantByGameId((prev) => mergeVariantSelections(prev, catalog));
  }, []);

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

  const warmNativeStreamerForLaunch = useCallback((): void => {
    if (settings.streamClientMode !== "native") {
      return;
    }

    void window.openNow.getNativeStreamerStatus()
      .then((status) => {
        if (status.detected) {
          console.log("[NativeStreamer] Launch warm-up ready:", status.message);
        } else {
          console.warn("[NativeStreamer] Launch warm-up did not detect native streamer:", status.message);
        }
      })
      .catch((error) => {
        console.warn("[NativeStreamer] Launch warm-up failed:", error);
      });
  }, [settings.streamClientMode]);

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

  // Keep a ref copy of `streamStatus` so async callbacks can observe latest value
  useEffect(() => {
    streamStatusRef.current = streamStatus;
  }, [streamStatus]);

  // Broadcast minimal session/loading state for UI listeners.
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

  const {
    activeQueueAd,
    activeQueueAdMediaUrl,
    effectiveAdState,
    handleQueueAdPlaybackEvent,
    queueAdPlaybackRef,
    queueAdPreviewRef,
  } = useQueueAdRuntime({
    authSession,
    effectiveStreamingBaseUrl,
    session,
    sessionRef,
    setQueuePosition,
    setSession,
    subscriptionInfo,
    t,
  });

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

  const storePanelGames = useMemo(() => flattenStorePanelGames(storePanels), [storePanels]);
  const allKnownGames = useMemo(() => [...games, ...libraryGames, ...storePanelGames], [games, libraryGames, storePanelGames]);

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
        gameTitle: gameTitle || t("session.thisGame"),
      });
    });
  }, [t]);

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

  useEffect(() => {
    applyAccentColor(settings.appAccentColor);
  }, [settings.appAccentColor]);

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
    const streamingBaseUrl = session.provider.streamingServiceUrl;
    const loadId = ++runtimeDataLoadIdRef.current;
    const isCurrentLoad = (): boolean => runtimeDataLoadIdRef.current === loadId;

    setIsLoadingCatalog(true);
    setIsLoadingLibrary(true);

    void window.openNow.getRegions({ token }).then((discovered) => {
      if (isCurrentLoad()) setRegions(discovered);
    }).catch((error) => {
      console.warn("Failed to load regions:", error);
      if (isCurrentLoad()) setRegions([]);
    });

    void window.openNow.fetchSubscription({
      token,
      providerStreamingBaseUrl: streamingBaseUrl,
      userId: session.user.userId,
    }).then((subscription) => {
      if (isCurrentLoad()) setSubscriptionInfo(subscription);
    }).catch((error) => {
      console.warn("Failed to load subscription info:", error);
      if (isCurrentLoad()) setSubscriptionInfo(null);
    });

    void window.openNow.browseCatalog({
      token,
      providerStreamingBaseUrl: streamingBaseUrl,
      searchQuery: "",
      sortId: catalogSelectedSortId,
      filterIds: catalogSelectedFilterIds,
    }).then((catalogResult) => {
      if (!isCurrentLoad()) return;
      applyCatalogBrowseResult(catalogResult);
    }).catch((error) => {
      console.error("Catalog load failed:", error);
      if (!isCurrentLoad()) return;
      setGames([]);
      setCatalogFilterGroups([]);
      setCatalogSortOptions([]);
      setCatalogTotalCount(0);
      setCatalogSupportedCount(0);
    }).finally(() => {
      if (isCurrentLoad()) setIsLoadingCatalog(false);
    });

    void window.openNow.fetchLibraryGames({
      token,
      providerStreamingBaseUrl: streamingBaseUrl,
    }).then((libGames) => {
      if (!isCurrentLoad()) return;
      setLibraryGames(libGames);
      applyVariantSelections(libGames);
    }).catch((error) => {
      console.error("Library load failed:", error);
      if (!isCurrentLoad()) return;
      setLibraryGames([]);
    }).finally(() => {
      if (isCurrentLoad()) setIsLoadingLibrary(false);
    });

    void window.openNow.fetchFeaturedGames({
      token,
      providerStreamingBaseUrl: streamingBaseUrl,
    }).then((featured) => {
      if (isCurrentLoad()) setFeaturedGames(featured);
    }).catch((error) => {
      console.warn("Featured games load failed:", error);
      if (isCurrentLoad()) setFeaturedGames([]);
    });
  }, [
    applyCatalogBrowseResult,
    applyVariantSelections,
    catalogSelectedFilterIds,
    catalogSelectedSortId,
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
        setStartupStatusMessage(t("auth.status.restoringSavedSession"));
        const [providerList, sessionResult] = await Promise.all([
          window.openNow.getLoginProviders(),
          window.openNow.getAuthSession(),
        ]);
        const accounts = await window.openNow.getSavedAccounts();
        const persistedSession = sessionResult.session;

        if (sessionResult.refresh.outcome === "refreshed") {
          setStartupRefreshNotice({
            tone: "success",
            text: t("auth.status.sessionRestoredTokenRefreshed"),
          });
          setStartupStatusMessage(t("auth.status.tokenRefreshedLoadingAccount"));
        } else if (sessionResult.refresh.outcome === "failed") {
          setStartupRefreshNotice({
            tone: "warn",
            text: t("auth.status.tokenRefreshFailedUsingSaved"),
          });
          setStartupStatusMessage(t("auth.status.tokenRefreshFailedContinuing"));
        } else if (sessionResult.refresh.outcome === "missing_refresh_token") {
          setStartupStatusMessage(t("auth.status.missingRefreshTokenContinuing"));
        } else if (persistedSession) {
          setStartupStatusMessage(t("auth.status.sessionRestored"));
        } else {
          setStartupStatusMessage(t("auth.status.noSavedSessionFound"));
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
          runtimeDataLoadIdRef.current += 1;
          resetStorePanels();
          setRegions([]);
          setGames([]);
          setLibraryGames([]);
          setSubscriptionInfo(null);
          setCatalogFilterGroups([]);
          setCatalogSortOptions([]);
          setCatalogTotalCount(0);
          setCatalogSupportedCount(0);
          setIsLoadingCatalog(false);
          setIsLoadingLibrary(false);
        }

        setIsInitializing(false);
      } catch (error) {
        console.error("Initialization failed:", error);
        setStartupStatusMessage(t("auth.status.sessionRestoreFailed"));
        // Always set isInitializing to false even on error
        setIsInitializing(false);
      }
    };

    void initialize();
  }, [loadSessionRuntimeData, resetStorePanels, t]);

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
      setLoginError(error instanceof Error ? error.message : t("errors.loginFailed"));
    } finally {
      setIsLoggingIn(false);
    }
  }, [loadSessionRuntimeData, providerIdpId, refreshSavedAccounts, t]);

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
      setLoginError(error instanceof Error ? error.message : t("errors.switchAccountFailed"));
      try {
        await refreshSavedAccounts();
        const sessionResult = await window.openNow.getAuthSession();
        setAuthSession(sessionResult.session);
        if (sessionResult.session) {
          setProviderIdpId(sessionResult.session.provider.idpId);
          await loadSessionRuntimeData(sessionResult.session);
          await refreshNavbarActiveSession(sessionResult.session);
        } else {
          runtimeDataLoadIdRef.current += 1;
          resetStorePanels();
          setRegions([]);
          setGames([]);
          setLibraryGames([]);
          setSubscriptionInfo(null);
          setNavbarActiveSession(null);
          setCatalogFilterGroups([]);
          setCatalogSortOptions([]);
          setCatalogTotalCount(0);
          setCatalogSupportedCount(0);
          setIsLoadingCatalog(false);
          setIsLoadingLibrary(false);
        }
      } catch (recoveryError) {
        console.warn("Failed to recover account state after switch failure:", recoveryError);
      }
    }
  }, [loadSessionRuntimeData, refreshNavbarActiveSession, refreshSavedAccounts, resetStorePanels, t]);

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
    runtimeDataLoadIdRef.current += 1;
    resetStorePanels();
    setRegions([]);
    setGames([]);
    setFeaturedGames([]);
    setLibraryGames([]);
    setSubscriptionInfo(null);
    setNavbarActiveSession(null);
    setCatalogFilterGroups([]);
    setCatalogSortOptions([]);
    setCatalogTotalCount(0);
    setCatalogSupportedCount(0);
    setIsLoadingCatalog(false);
    setIsLoadingLibrary(false);
  }, [accountToRemove, loadSessionRuntimeData, refreshNavbarActiveSession, resetStorePanels]);

  const handleAddAccount = useCallback(() => {
    setAuthSession(null);
    setLoginError(null);
  }, []);

  const confirmLogout = useCallback(async () => {
    setLogoutConfirmOpen(false);
    runtimeDataLoadIdRef.current += 1;
    resetStorePanels();
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
    setIsLoadingCatalog(false);
    setIsLoadingLibrary(false);
  }, [resetLaunchRuntime, resetStorePanels]);

  // Logout handler
  const handleLogout = useCallback(() => {
    setLogoutConfirmOpen(true);
  }, []);

  // Load games handler
  const loadGames = useCallback(async (targetSource: "main" | "library") => {
    const setLoading = targetSource === "main" ? setIsLoadingCatalog : setIsLoadingLibrary;
    setLoading(true);
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
        if (featuredGames.length === 0) {
          void window.openNow.fetchFeaturedGames({ token, providerStreamingBaseUrl: baseUrl }).then((featured) => {
            if (featured.length > 0) setFeaturedGames(featured);
          }).catch((error) => {
            console.warn("Featured games refresh failed:", error);
          });
        }
        return;
      }

      const result = await window.openNow.fetchLibraryGames({ token, providerStreamingBaseUrl: baseUrl });
      setLibraryGames(result);
      setSelectedGameId((previous) => result.some((game) => game.id === previous) ? previous : (result[0]?.id ?? ""));
      applyVariantSelections(result);
    } catch (error) {
      console.error("Failed to load games:", error);
    } finally {
      setLoading(false);
    }
  }, [applyCatalogBrowseResult, applyVariantSelections, authSession, effectiveStreamingBaseUrl, featuredGames, searchQuery, catalogFilterKey, catalogSelectedSortId]);

  const loadStorePanels = useCallback(async () => {
    const session = authSession;
    if (!session) return;

    const token = session.tokens.idToken ?? session.tokens.accessToken;
    if (!token) return;

    const contextKey = `${session.user.userId}\0${effectiveStreamingBaseUrl}`;
    if (storePanelsLoadedContextRef.current === contextKey) return;

    const loadId = ++storePanelsLoadIdRef.current;
    const isCurrentLoad = (): boolean => storePanelsLoadIdRef.current === loadId;
    setIsLoadingStorePanels(true);
    try {
      const panels = await window.openNow.fetchStorePanels({
        token,
        providerStreamingBaseUrl: effectiveStreamingBaseUrl,
      });
      if (!isCurrentLoad()) return;
      const panelGames = flattenStorePanelGames(panels);
      storePanelsLoadedContextRef.current = contextKey;
      setStorePanels(panels);
      setSelectedGameId((previous) => panelGames.some((game) => game.id === previous) ? previous : (panelGames[0]?.id ?? ""));
      setVariantByGameId((previous) => {
        const next = { ...previous };
        for (const game of panelGames) {
          next[game.id] = defaultVariantId(game);
        }
        return next;
      });
    } catch (error) {
      if (!isCurrentLoad()) return;
      console.error("Failed to load Store panels:", error);
      storePanelsLoadedContextRef.current = "";
      setStorePanels([]);
    } finally {
      if (isCurrentLoad()) setIsLoadingStorePanels(false);
    }
  }, [authSession, effectiveStreamingBaseUrl]);

  useEffect(() => {
    if (storePanelGames.length === 0 || libraryGames.length === 0) return;
    setVariantByGameId((previous) => {
      let changed = false;
      const next = { ...previous };
      for (const game of storePanelGames) {
        const libraryVariantId = getLibrarySelectedVariantId(game, libraryGames);
        if (libraryVariantId && next[game.id] !== libraryVariantId) {
          next[game.id] = libraryVariantId;
          changed = true;
        }
      }
      return changed ? next : previous;
    });
  }, [libraryGames, storePanelGames]);

  useEffect(() => {
    if (!authSession || currentPage !== "home" || settings.controllerMode) {
      return;
    }
    const handle = window.setTimeout(() => {
      void loadGames("main");
    }, searchQuery.trim() ? 220 : 0);
    return () => window.clearTimeout(handle);
  }, [authSession, currentPage, loadGames, searchQuery, catalogFilterKey, catalogSelectedSortId, settings.controllerMode]);

  useEffect(() => {
    if (!authSession || currentPage !== "home" || !settings.controllerMode) {
      return;
    }
    void loadStorePanels();
  }, [authSession, currentPage, loadStorePanels, settings.controllerMode]);

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
        warmNativeStreamerForLaunch();

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
  }, [applyClaimedSessionAndConnect, authSession, buildCurrentStreamSettings, effectiveStreamingBaseUrl, findGameContextForSession, resolveResumeIdentity, resolveSessionClaimAppId, warmNativeStreamerForLaunch]);

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

  const handleExpectedNativeSessionClose = useCallback((reason: string): void => {
    console.log("[Recovery] Treating native signaling close as ended session:", reason);
    const activeGameId = streamingGameRef.current?.id;
    if (activeGameId) {
      endPlaytimeSession(activeGameId);
    }
    markExplicitSignalingShutdown();
    clientRef.current?.dispose();
    clientRef.current = null;
    launchInFlightRef.current = false;
    resetLaunchRuntime();
    void refreshNavbarActiveSession();
  }, [endPlaytimeSession, markExplicitSignalingShutdown, refreshNavbarActiveSession, resetLaunchRuntime]);

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
            message: warningMessage(t, warning.code),
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
      });
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
          if (streamStatusRef.current === "streaming" && isExpectedNativeSessionClose(reason)) {
            handleExpectedNativeSessionClose(reason);
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
              title: t("errors.nativeStreamerStoppedTitle"),
              description: t("errors.nativeStreamerStoppedDescription"),
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
          if (
            nativeStreamingRef.current
            && streamStatusRef.current === "streaming"
            && isExpectedNativeSessionClose(event.reason)
          ) {
            handleExpectedNativeSessionClose(event.reason);
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
              title: t("errors.sessionConnectionLostTitle"),
              description: t("errors.resumeAttachFailedDescription"),
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
              title: t("errors.sessionConnectionLostTitle"),
              description: t("errors.sessionConnectionLostDescription"),
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
        const message = error instanceof Error ? error.message : t("errors.sessionResumeFailedDescription");
        setLaunchError({
          stage: streamStatusToLoadingStage(streamStatusRef.current),
          title: t("errors.sessionConnectionLostTitle"),
          description: message,
        });
        resetLaunchRuntime({ keepLaunchError: true, keepStreamingContext: true });
        void refreshNavbarActiveSession();
        launchInFlightRef.current = false;
      }
    });

    return () => unsubscribe();
  }, [attemptSessionRecovery, diagnosticsStore, handleExpectedNativeSessionClose, refreshNavbarActiveSession, resetLaunchRuntime, scheduleStableRecoveryReset, settings, streamMicLevel, streamVolume, t]);

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
    warmNativeStreamerForLaunch();
    let launchGameContext: GameInfo = game;

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
      launchGameContext = matchedGameContext.game;
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
      setLaunchError(toLaunchErrorState(t, error, loadingStep, launchGameContext));
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
    t,
    variantByGameId,
    warmNativeStreamerForLaunch,
  ]);

  // Gate handler: shows queue server modal for FREE-tier users before launching
  const handleInitiatePlay = useCallback(async (game: GameInfo) => {
    const effectiveTier = normalizeMembershipTier(
      subscriptionInfo?.membershipTier ?? authSession?.user.membershipTier,
    );
    const isFreeUser = effectiveTier === "FREE";
    const activeProvider = authSession?.provider ?? selectedProvider;
    const isNvidiaAccount = isNvidiaProvider(activeProvider);
    const isAllianceServer = isAllianceStreamingBaseUrl(effectiveStreamingBaseUrl);
    if (!isNvidiaAccount || isAllianceServer) {
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
  }, [subscriptionInfo, authSession, selectedProvider, settings.hideServerSelector, streamStatus, handlePlayGame, effectiveStreamingBaseUrl]);

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

  const handleOpenStoreUrl = useCallback((url: string): void => {
    void window.openNow.openExternalUrl(url).catch((error) => {
      console.error("Failed to open Store URL:", error);
    });
  }, []);

  const handleBuyGame = useCallback((game: GameInfo, selectedVariantId?: string): void => {
    const selectedVariant = getSelectedVariant(game, selectedVariantId ?? defaultVariantId(game));
    const localStoreUrl = selectedVariant?.storeUrl
      ?? game.variants.find((variant) => variant.storeUrl)?.storeUrl;
    if (localStoreUrl) {
      handleOpenStoreUrl(localStoreUrl);
      return;
    }

    const token = authSession?.tokens.idToken ?? authSession?.tokens.accessToken;
    if (!token) return;

    void window.openNow.resolveStoreUrl({
      token,
      providerStreamingBaseUrl: effectiveStreamingBaseUrl,
      appIdOrUuid: game.uuid ?? game.id,
      variantId: selectedVariant?.id ?? selectedVariantId,
      store: selectedVariant?.store,
    }).then((storeUrl) => {
      if (storeUrl) handleOpenStoreUrl(storeUrl);
    }).catch((error) => {
      console.error("Failed to resolve Store URL:", error);
    });
  }, [authSession, effectiveStreamingBaseUrl, handleOpenStoreUrl]);

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
    savedAccounts.find((account) => account.userId === accountToRemove)?.displayName ?? t("auth.accounts.thisAccount")
  ), [accountToRemove, savedAccounts, locale, t]);

  const logoutConfirmModal = logoutConfirmOpen && typeof document !== "undefined"
    ? createPortal(
        <div className="logout-confirm" role="dialog" aria-modal="true" aria-label={t("auth.accounts.logOutConfirmation")}>
          <button
            type="button"
            className="logout-confirm-backdrop"
            onClick={() => setLogoutConfirmOpen(false)}
            aria-label={t("auth.accounts.cancelLogOut")}
          />
          <div className="logout-confirm-card">
            <div className="logout-confirm-kicker">{t("auth.accounts.kicker")}</div>
            <h3 className="logout-confirm-title">{t("auth.accounts.signOutAllTitle")}</h3>
            <p className="logout-confirm-text">
              {t("auth.accounts.signOutAllDescription")}
            </p>
            <p className="logout-confirm-subtext">
              {t("auth.accounts.signOutAllSubtext")}
            </p>
            <div className="logout-confirm-actions">
              <button
                type="button"
                className="logout-confirm-btn logout-confirm-btn-cancel"
                onClick={() => setLogoutConfirmOpen(false)}
              >
                {t("auth.accounts.staySignedIn")}
              </button>
              <button
                type="button"
                className="logout-confirm-btn logout-confirm-btn-confirm"
                onClick={() => {
                  void confirmLogout();
                }}
              >
                {t("auth.accounts.signOutAll")}
              </button>
            </div>
            <div className="logout-confirm-hint">
              <kbd>Enter</kbd> {t("app.actions.confirm")} · <kbd>Esc</kbd> {t("app.actions.cancel")}
            </div>
          </div>
        </div>,
        document.body,
      )
    : null;

  const removeAccountConfirmModal = removeAccountConfirmOpen && typeof document !== "undefined"
    ? createPortal(
        <div className="logout-confirm" role="dialog" aria-modal="true" aria-label={t("auth.accounts.removeAccountConfirmation")}>
          <button
            type="button"
            className="logout-confirm-backdrop"
            onClick={() => {
              setRemoveAccountConfirmOpen(false);
              setAccountToRemove(null);
            }}
            aria-label={t("auth.accounts.cancelAccountRemoval")}
          />
          <div className="logout-confirm-card">
            <div className="logout-confirm-kicker">{t("auth.accounts.kicker")}</div>
            <h3 className="logout-confirm-title">{t("auth.accounts.removeAccountTitle")}</h3>
            <p className="logout-confirm-text">
              {t("auth.accounts.removeAccountDescription", { name: accountToRemoveDisplayName })}
            </p>
            <p className="logout-confirm-subtext">
              {t("auth.accounts.removeAccountSubtext")}
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
                {t("app.actions.cancel")}
              </button>
              <button
                type="button"
                className="logout-confirm-btn logout-confirm-btn-confirm"
                onClick={() => {
                  void confirmRemoveAccount();
              }}
            >
                {t("app.actions.remove")}
              </button>
            </div>
            <div className="logout-confirm-hint">
              <kbd>Enter</kbd> {t("app.actions.confirm")} · <kbd>Esc</kbd> {t("app.actions.cancel")}
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
    let resumeGameContext: GameInfo | null = null;
    if (matchedContext) {
      resumeGameContext = matchedContext.game;
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
      setLaunchError(toLaunchErrorState(t, error, loadingStep, resumeGameContext));
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
    t,
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

    const activeSessionTitle = gameTitleByAppId.get(navbarActiveSession.appId)?.trim() || t("session.thisSession");
    if (!window.confirm(t("session.terminateConfirmation", { title: activeSessionTitle }))) {
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
    t,
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

    const gameName = (streamingGame?.title || t("session.thisGame")).trim();
    const shouldExit = await requestExitPrompt(gameName);
    if (!shouldExit) {
      return;
    }

    await handleStopStream();
  }, [handleStopStream, releasePointerLockIfNeeded, requestExitPrompt, streamStatus, streamingGame?.title, t]);

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

  const navigateControllerPage = useCallback((direction: -1 | 1): void => {
    const pages: AppPage[] = ["library", "home", "settings"];
    const currentIndex = Math.max(0, pages.indexOf(currentPage));
    const nextIndex = (currentIndex + direction + pages.length) % pages.length;
    setCurrentPage(pages[nextIndex]);
  }, [currentPage]);

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

  const showLaunchOverlay = streamStatus !== "idle" || launchError !== null;
  const hasActiveStreamView = streamStatus !== "idle";
  const showLaunchErrorOverlay = launchError !== null;
  const showDesktopLaunchLoading = showLaunchErrorOverlay || (streamStatus !== "idle" && streamStatus !== "streaming");
  // Show stream lifecycle (waiting/connecting/streaming/failure)
  if (showLaunchOverlay) {
    const loadingStatus = launchError ? launchError.stage : toLoadingStatus(streamStatus);
    return (
      <>
        {hasActiveStreamView && (
          <StreamView
            videoRef={videoRef}
            audioRef={audioRef}
            diagnosticsStore={diagnosticsStore}
            showStats={showStatsOverlay}
            showNativeStats={settings.showNativeStreamerStats}
            gstreamerEnabled={settings.streamClientMode === "native"}
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
            showSessionTimeRemainingInStatsOverlay={settings.showSessionTimeRemainingInStatsOverlay}
            sessionTimeRemainingSeconds={sessionTimeRemainingSeconds}
            sessionClockShowEveryMinutes={settings.sessionClockShowEveryMinutes}
            sessionClockShowDurationSeconds={settings.sessionClockShowDurationSeconds}
            streamWarning={streamWarning}
            isFullscreen={sessionFullscreen || !!document.fullscreenElement}
            isConnecting={streamStatus === "connecting"}
            isStreaming={isStreaming}
            gameTitle={streamingGame?.title ?? t("app.labels.game")}
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
            onShowSessionTimeRemainingInStatsOverlayChange={(value) => {
              void updateSetting("showSessionTimeRemainingInStatsOverlay", value);
            }}
            subscriptionInfo={subscriptionInfo}
            micTrack={clientRef.current?.getMicTrack() ?? null}
            onRequestPointerLock={handleRequestPointerLock}
            onReleasePointerLock={() => {
              void releasePointerLockIfNeeded();
            }}
            allowEscapeToExitFullscreen={settings.allowEscapeToExitFullscreen}
          />
        )}
        {showDesktopLaunchLoading && (
          <StreamLoading
            gameTitle={streamingGame?.title ?? t("app.labels.game")}
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
      </>
    );
  }

  // Main app layout
  return (
    <div className={`app-container${settings.controllerMode ? " app-container--controller" : ""}`} style={getAppStyle(settings.posterSizeScale)}>
      {startupRefreshNotice && (
        <div className={`auth-refresh-notice auth-refresh-notice--${startupRefreshNotice.tone}`}>
          {startupRefreshNotice.text}
        </div>
      )}
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
        controllerMode={settings.controllerMode}
      />

      <main className="main-content">
        {currentPage === "home" && (
          <HomePage
            games={filteredGames}
            searchQuery={searchQuery}
            onSearchChange={setSearchQuery}
            onPlayGame={handleInitiatePlay}
            isLoading={settings.controllerMode ? isLoadingStorePanels : isLoadingCatalog}
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
            controllerMode={settings.controllerMode}
            storePanels={storePanels}
            storeHeroGames={featuredGames}
            activeSessionAppIds={navbarActiveSession ? [navbarActiveSession.appId] : []}
            onBuyGame={handleBuyGame}
            onPreviousControllerPage={() => navigateControllerPage(-1)}
            onNextControllerPage={() => navigateControllerPage(1)}
          />
        )}

        {currentPage === "library" && (
          <LibraryPage
            games={filteredLibraryGames}
            searchQuery={searchQuery}
            onSearchChange={setSearchQuery}
            onPlayGame={handleInitiatePlay}
            isLoading={isLoadingLibrary}
            selectedGameId={selectedGameId}
            onSelectGame={setSelectedGameId}
            selectedVariantByGameId={variantByGameId}
            onSelectGameVariant={handleSelectGameVariant}
            libraryCount={libraryGames.length}
            sortOptions={catalogSortOptions.filter((option) => option.id !== "relevance")}
            selectedSortId={catalogSelectedSortId === "relevance" ? "last_played" : catalogSelectedSortId}
            onSortChange={setCatalogSelectedSortId}
            controllerMode={settings.controllerMode}
            featuredGames={featuredGames.length > 0 ? featuredGames : games}
            activeSessionAppIds={navbarActiveSession ? [navbarActiveSession.appId] : []}
            onBuyGame={handleBuyGame}
            onPreviousControllerPage={() => navigateControllerPage(-1)}
            onNextControllerPage={() => navigateControllerPage(1)}
          />
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
