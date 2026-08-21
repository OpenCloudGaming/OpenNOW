import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { CSSProperties, JSX } from "react";
import { AnimatePresence, m, useReducedMotion } from "motion/react";

import type {
  ActiveSessionInfo,
  AuthSession,
  DirectLaunchRequest,
  FrameInterpolationSettings,
  GameInfo,
  LoginProvider,
  NativeStreamerShortcutAction,
  ReleaseHighlightsPayload,
  SessionInfo,
  SessionStopRequest,
  Settings,
  SubscriptionInfo,
  SignalingConnectRequest,
  StreamSettings,
  StreamRegion,
  VideoShaderSettings,
} from "@shared/gfn";
import { discordGameImageUrl } from "@shared/discord";
import type { DesktopSessionReport } from "@shared/bugReport";
import type { FeedbackCategory } from "@shared/telemetry";
import {
  buildNativeStreamerSessionContext,
  createDefaultSettings,
  createPlatformShortcutDefaults,
  resolveEntitledStreamProfile,
  resolveRuntimePlatform,
  SAFE_FALLBACK_STREAM_PROFILE,
  streamDiagnosticId,
} from "@shared/gfn";
import { setLogContext } from "@shared/logger";
import { formatShortcutForDisplay, isShortcutMatch, normalizeShortcut } from "./shortcuts";
import { dispatchStreamShortcutAction } from "./streamShortcutActions";
import { useElapsedSeconds } from "./utils/useElapsedSeconds";
import { useAuthSession } from "./hooks/useAuthSession";
import { useCatalogData } from "./hooks/useCatalogData";
import { useActiveSessionActions } from "./hooks/streamSession/useActiveSessionActions";
import { useGameLaunch } from "./hooks/streamSession/useGameLaunch";
import { useSignalingEvents } from "./hooks/streamSession/useSignalingEvents";
import {
  RECOVERABLE_STREAM_STATUSES,
  SIGNALING_RECOVERY_WINDOW_MS,
  nextSignalingRecoveryPollDelayMs,
  remoteSessionEndCode,
  sendStreamClipboardPaste,
  sleep,
  useStreamSession,
} from "./hooks/useStreamSession";
import { useQueueAdRuntime } from "./hooks/useQueueAdRuntime";
import { usePlaytime } from "./utils/usePlaytime";
import { createStreamDiagnosticsStore, useStreamDiagnosticsSelector } from "./utils/streamDiagnosticsStore";
import { StreamSessionReportAccumulator } from "./utils/sessionReport";
import { nextStatsOverlayMode } from "./utils/streamStatsHud";
import { isShortcutCaptureTarget } from "./utils/shortcutCaptureFocus";
import type { StreamStatus } from "./lib/appTypes";
import {
  getCodecToMigrateToAuto,
  loadStoredCodecResults,
  resolveStreamProfileCodec,
  saveStoredCodecResults,
  testCodecSupport,
  type CodecTestResult,
} from "./lib/codecDiagnostics";
import {
  createSyntheticDirectLaunchGame,
  findDirectLaunchTarget,
} from "./lib/directLaunch";
import {
  defaultVariantId,
  findSessionContextForAppId,
  getSelectedVariant,
  isNumericId,
  matchesGameSearch,
  parseNumericId,
  sortLibraryGames,
} from "./lib/gameCatalog";
import { resolveInstallToPlayStorageRegionUrl } from "./lib/launchOwnership";
import { resolveAppLaunchMode } from "./lib/appLaunchMode";
import { hasAnyEligiblePrintedWasteZone, isAllianceStreamingBaseUrl } from "./lib/printedWaste";
import { getStreamPointerLockTarget, isStreamPointerLocked } from "./lib/pointerLock";
import { normalizeMembershipTier } from "./lib/queueAds";
import { clearRuntimeSnapshot, type RuntimeSnapshot } from "./lib/runtimeSnapshot";
import { getEnabledSessionProxyUrl } from "./lib/sessionProxy";
import {
  getSessionLimitSecondsForTier,
  getLocalSessionTimerWarning,
  hasCrossedWarningThreshold,
  shouldShowFreeTierSessionWarnings,
} from "./lib/sessionWarnings";
import {
  isStreamVideoReady,
  shouldSurfaceRemoteSessionEnd,
  toRemoteSessionEndedError,
  toLoadingStatus,
} from "./lib/sessionState";
import { defaultDiagnostics } from "./lib/streamDiagnostics";
import { selectRecoveryCandidate } from "./lib/streamRecoveryDecisions";
import { applyAccentColor, applyTheme, applyTranslucentUI } from "./lib/uiCustomization";
import { useTranslation } from "./i18n";

// UI Components
import { LoginScreen } from "./components/LoginScreen";
import { Navbar } from "./components/Navbar";
import { HomePage } from "./components/HomePage";
import { LibraryPage } from "./components/LibraryPage";
import { PageErrorBoundary } from "./components/PageErrorBoundary";
import { SettingsPage } from "./components/SettingsPage";
import { SettingsModalHost } from "./components/SettingsModalHost";
import { StreamLoading } from "./components/StreamLoading";
import { StreamView } from "./components/StreamView";
import { QueueServerSelectModal } from "./components/QueueServerSelectModal";
import { GameDetailModal } from "./components/GameDetailModal";
import { ReleaseHighlightsModal } from "./components/ReleaseHighlightsModal";
import { ErrorReportingConsentModal } from "./components/ErrorReportingConsentModal";
import { FeedbackModal } from "./components/FeedbackModal";
import { SessionReportModal } from "./components/SessionReportModal";
import { ControllerModePromptModal } from "./components/ControllerModePromptModal";
import { ModalSurface } from "./components/ui/ModalSurface";
import { overlayMotion, pageTransition, streamRevealTransition } from "./components/MotionProvider";
import { LazyShaderAtmosphere } from "./components/LazyShaderAtmosphere";
import { ConsoleProfileGate } from "./components/console/ConsoleProfileGate";
import { useConsoleShell } from "./hooks/useConsoleShell";
import {
  shouldOfferControllerModePrompt,
  useControllerModePrompt,
} from "./hooks/useControllerModePrompt";
import { syncRendererTelemetry } from "./telemetry/posthog";

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

const PLAYTIME_RESYNC_INTERVAL_MS = 5 * 60 * 1000;
const FREE_TIER_30_MIN_WARNING_SECONDS = 30 * 60;
const FREE_TIER_15_MIN_WARNING_SECONDS = 15 * 60;
const FREE_TIER_FINAL_MINUTE_WARNING_SECONDS = 60;
const STREAM_WARNING_VISIBILITY_MS = 15 * 1000;

type AppPage = "home" | "library" | "settings";
type ExitPromptState = { open: boolean; gameTitle: string };

const RUNTIME_PLATFORM = resolveRuntimePlatform(navigator.platform);
const isMac = RUNTIME_PLATFORM === "darwin";
const DEFAULT_SHORTCUTS = createPlatformShortcutDefaults(RUNTIME_PLATFORM).bindings;

export function App(): JSX.Element {
  const { locale, t } = useTranslation();
  const reducedMotion = useReducedMotion();

  // Navigation / settings / stream state below; auth + catalog come from hooks after deps are ready.

  // Navigation
  const [currentPage, setCurrentPage] = useState<AppPage>("home");
  const [pageBeforeSettings, setPageBeforeSettings] = useState<AppPage>("home");
  const [sessionFullscreen, setSessionFullscreenState] = useState(false);

  // Settings State
  const [settings, setSettings] = useState<Settings>(() => createDefaultSettings(RUNTIME_PLATFORM));
  const [settingsLoaded, setSettingsLoaded] = useState(false);
  const [releaseHighlightsPayload, setReleaseHighlightsPayload] = useState<ReleaseHighlightsPayload | null>(null);
  const [releaseHighlightsIsAuto, setReleaseHighlightsIsAuto] = useState(false);
  const [feedbackOpen, setFeedbackOpen] = useState(false);
  const [feedbackSurfacePresent, setFeedbackSurfacePresent] = useState(false);
  const [feedbackInitialCategory, setFeedbackInitialCategory] = useState<FeedbackCategory>("bug");
  const [feedbackSessionReport, setFeedbackSessionReport] = useState<DesktopSessionReport | null>(null);
  const [latestSessionReport, setLatestSessionReport] = useState<DesktopSessionReport | null>(null);
  const [sessionReportOpen, setSessionReportOpen] = useState(false);
  const sessionReportAccumulatorRef = useRef<StreamSessionReportAccumulator | null>(null);
  const showSessionReportRef = useRef(settings.showSessionReport);
  const directLaunchConsoleModeRef = useRef(false);
  const [consentSurfacePresent, setConsentSurfacePresent] = useState(false);
  const activeSessionProxyUrl = useMemo(
    () => getEnabledSessionProxyUrl(settings),
    [settings.sessionProxyEnabled, settings.sessionProxyUrl],
  );
  const [codecResults, setCodecResults] = useState<CodecTestResult[] | null>(() => loadStoredCodecResults());
  const [codecTesting, setCodecTesting] = useState(false);
  const diagnosticsStoreRef = useRef<ReturnType<typeof createStreamDiagnosticsStore> | null>(null);
  const diagnosticsStore =
    diagnosticsStoreRef.current ?? (diagnosticsStoreRef.current = createStreamDiagnosticsStore(defaultDiagnostics()));
  const diagnosticsVideoReady = useStreamDiagnosticsSelector(
    diagnosticsStore,
    (stats) => stats.nativeRendererActive || stats.framesDecoded > 0,
  );

  const { runtime: streamRuntime, recovery, snapshot } = useStreamSession();
  const {
    session, setSession,
    streamStatus, setStreamStatus,
    statsMode, setStatsMode,
    antiAfkEnabled, setAntiAfkEnabled,
    antiAfkAckNonce, setAntiAfkAckNonce,
    nativeInputCaptureActive, setNativeInputCaptureActive,
    nativeInputBridgeReady, setNativeInputBridgeReady,
    streamingGame, setStreamingGame,
    streamingStore, setStreamingStore,
    queuePosition, setQueuePosition,
    navbarActiveSession, setNavbarActiveSession,
    isResumingNavbarSession, setIsResumingNavbarSession,
    isTerminatingNavbarSession,
    launchError, setLaunchError,
    pendingDirectLaunchRequest, setPendingDirectLaunchRequest,
    directLaunchConsoleMode, setDirectLaunchConsoleMode,
    queueModalGame, setQueueModalGame,
    queueModalData, setQueueModalData,
    sessionStartedAtMs, setSessionStartedAtMs,
    remoteStreamWarning, setRemoteStreamWarning,
    localSessionTimerWarning, setLocalSessionTimerWarning,
    streamVolume, setStreamVolume,
    videoElementHasFrame, setVideoElementHasFrame,
    streamRevealPhase, setStreamRevealPhase,
    videoRef, audioRef, clientRef,
    previousFreeTierRemainingSecondsRef,
    navbarSessionActionInFlightRef,
    nativeStreamingRef,
    handleStreamShortcutActionRef,
    streamingGameRef,
    isStreamingRef,
    sessionRef,
    launchInFlightRef,
    directLaunchAttemptIdRef,
    handledDirectLaunchIdsRef,
    runtimeSnapshotRef,
    claimResumePromisesRef,
    launchAbortRef,
    discordStreamingActivitySessionRef,
    streamStatusRef,
    nativeInputProtocolVersionRef,
    awaitingRecoveryRemoteIceRef,
    appUnloadingRef,
    signalingRecoveryRef,
    directLaunchSessionSeenRef,
  } = streamRuntime;
  const {
    disconnectSignalingControlled,
    isRecoveryGenerationCurrent,
    markExplicitSignalingShutdown,
    resetRecoveryConnectionState,
    resetSignalingRecoveryState,
    scheduleStableRecoveryReset,
  } = recovery;
  const { persistRuntimeSnapshotNow } = snapshot;

  useEffect(() => {
    setLogContext("application.renderer", {
      locale,
      platform: navigator.platform,
      userAgent: navigator.userAgent,
    });
  }, [locale]);

  useEffect(() => {
    setLogContext("application.state", {
      page: currentPage,
      streamPhase: streamStatus,
      settingsLoaded,
      queuePosition: queuePosition ?? 0,
      hasActiveSession: session !== null,
      launchError: launchError?.codeLabel ?? launchError?.title ?? "none",
    });
  }, [currentPage, launchError, queuePosition, session, settingsLoaded, streamStatus]);

  useEffect(() => {
    if (!settingsLoaded) return;
    setLogContext("stream.settings", {
      resolution: settings.resolution,
      fps: settings.fps,
      codec: settings.codec,
      colorQuality: settings.colorQuality,
      maxBitrateMbps: settings.maxBitrateMbps,
      clientMode: settings.streamClientMode,
      nativeVideoBackend: settings.nativeVideoBackend,
      transportMode: settings.transportMode,
      keyboardLayout: settings.keyboardLayout,
      microphoneMode: settings.microphoneMode,
      cloudGsync: settings.enableCloudGsync,
    });
  }, [settings, settingsLoaded]);

  useEffect(() => {
    const retainLatestStream = (): void => {
      const stats = diagnosticsStore.getSnapshot();
      const hasStreamEvidence = streamStatus !== "idle"
        || session !== null
        || stats.connectionState !== "closed"
        || stats.nativeRendererActive;
      if (!hasStreamEvidence) return;

      setLogContext("stream.latest", {
        streamKey: streamDiagnosticId(session?.sessionId),
        phase: streamStatus,
        appId: session?.appId ?? "unknown",
        sessionStatus: session?.status ?? "unknown",
        queuePosition: session?.queuePosition ?? queuePosition ?? 0,
        seatSetupStep: session?.seatSetupStep ?? 0,
        zone: session?.zone ?? "unknown",
        serverLocation: stats.serverLocation || session?.serverLocation || "unknown",
        serverRegion: stats.serverRegion || "unknown",
        serverGpuType: stats.serverGpuType || session?.gpuType || "unknown",
        streamer: stats.nativeRendererActive || settings.streamClientMode === "native" ? "native" : "web",
        requestedResolution: settings.resolution,
        requestedFps: settings.fps,
        requestedCodec: settings.codec,
        negotiatedResolution: session?.negotiatedStreamProfile?.resolution ?? "unknown",
        negotiatedFps: session?.negotiatedStreamProfile?.fps ?? "unknown",
        negotiatedCodec: (session?.negotiatedStreamProfile?.codec ?? stats.codec) || "unknown",
        activeResolution: stats.resolution || "unknown",
        activeCodec: stats.codec || "unknown",
        connectionState: stats.connectionState,
        transportType: stats.transportType,
        hardwareAcceleration: stats.hardwareAcceleration || "unknown",
        bitrateKbps: stats.bitrateKbps,
        targetBitrateKbps: stats.targetBitrateKbps,
        availableBitrateKbps: stats.availableBitrateKbps,
        receiveFps: stats.receiveFps,
        decodeFps: stats.decodeFps,
        renderFps: stats.renderFps,
        framesReceived: stats.framesReceived,
        framesDecoded: stats.framesDecoded,
        framesDropped: stats.framesDropped,
        packetLossPercent: stats.packetLossPercent,
        jitterMs: stats.jitterMs,
        jitterBufferDelayMs: stats.jitterBufferDelayMs,
        rttMs: stats.rttMs,
        inputReady: stats.inputReady,
        inputQueueDropCount: stats.inputQueueDropCount,
        decoderPressureActive: stats.decoderPressureActive,
        decoderRecoveryAttempts: stats.decoderRecoveryAttempts,
        lagReason: stats.lagReason,
        lagReasonDetail: stats.lagReasonDetail,
        nativeQueueMode: stats.nativeQueueMode ?? "unknown",
        nativePartialFlushCount: stats.nativePartialFlushCount ?? 0,
        nativeCompleteFlushCount: stats.nativeCompleteFlushCount ?? 0,
        nativeTransition: stats.nativeTransitionSummary ?? "none",
        capturedAt: new Date().toISOString(),
      });
    };
    retainLatestStream();
    return diagnosticsStore.subscribe(retainLatestStream);
  }, [diagnosticsStore, queuePosition, session, settings, streamStatus]);

  const [exitPrompt, setExitPrompt] = useState<ExitPromptState>({ open: false, gameTitle: t("app.labels.game") });
  const [settingsFocusSection, setSettingsFocusSection] = useState<"account" | undefined>();
  const {
    open: controllerModePromptOpen,
    dismiss: dismissControllerModePrompt,
  } = useControllerModePrompt(shouldOfferControllerModePrompt({
    settingsLoaded,
    controllerMode: settings.controllerMode,
    directLaunchConsoleMode,
    promptDismissed: settings.controllerModePromptDismissed,
  }));

  const { playtime, startSession: startPlaytimeSession, endSession: endPlaytimeSession } = usePlaytime();
  const sessionElapsedSeconds = useElapsedSeconds(sessionStartedAtMs, streamStatus === "streaming");
  const isStreaming = streamStatus === "streaming";
  const [shortcutCaptureActive, setShortcutCaptureActive] = useState(false);
  // freeTier/session-limit derived state is computed after auth/catalog hooks


  const codecTestPromiseRef = useRef<Promise<CodecTestResult[] | null> | null>(null);
  const codecStartupTestAttemptedRef = useRef(false);

  useEffect(() => {
    streamingGameRef.current = streamingGame;
  }, [streamingGame]);

  showSessionReportRef.current = settings.showSessionReport;
  directLaunchConsoleModeRef.current = directLaunchConsoleMode;

  useEffect(() => {
    if (sessionStartedAtMs === null) return undefined;

    const accumulator = new StreamSessionReportAccumulator({
      gameTitle: streamingGameRef.current?.title ?? t("app.labels.game"),
      requestedResolution: settings.resolution,
      requestedCodec: settings.codec,
      targetFps: settings.fps,
    }, sessionStartedAtMs);
    sessionReportAccumulatorRef.current = accumulator;
    const record = (): void => accumulator.record(diagnosticsStore.getSnapshot());
    record();
    const unsubscribe = diagnosticsStore.subscribe(record);

    return () => {
      unsubscribe();
      const report = accumulator.finish();
      if (sessionReportAccumulatorRef.current === accumulator) {
        sessionReportAccumulatorRef.current = null;
      }
      if (!report) return;
      setLatestSessionReport(report);
      if (showSessionReportRef.current && !directLaunchConsoleModeRef.current) {
        setSessionReportOpen(true);
      }
    };
    // A session keeps the profile captured when its first decoded frame arrives.
    // eslint-disable-next-line react-hooks/exhaustive-deps -- finalize only when this session anchor changes
  }, [diagnosticsStore, sessionStartedAtMs]);

  useEffect(() => {
    let active = true;
    const syncShortcutCaptureFocus = (): void => {
      if (active) {
        setShortcutCaptureActive(isShortcutCaptureTarget(document.activeElement));
      }
    };
    const scheduleShortcutCaptureFocusSync = (): void => {
      queueMicrotask(syncShortcutCaptureFocus);
    };

    document.addEventListener("focusin", scheduleShortcutCaptureFocusSync);
    document.addEventListener("focusout", scheduleShortcutCaptureFocusSync);
    syncShortcutCaptureFocus();
    return () => {
      active = false;
      document.removeEventListener("focusin", scheduleShortcutCaptureFocusSync);
      document.removeEventListener("focusout", scheduleShortcutCaptureFocusSync);
    };
  }, []);

  useEffect(() => {
    window.openNow.setStreamShortcutInterceptionGate({
      streamActive: isStreaming,
      shortcutCaptureActive,
    });
  }, [isStreaming, shortcutCaptureActive]);

  const resetStatsOverlayToPreference = useCallback((): void => {
    setStatsMode(settings.showStatsOnLaunch ? "compact" : "off");
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

  const accountConfirmRestoreFocusRef = useRef<HTMLElement | null>(null);
  const logoutConfirmCancelRef = useRef<HTMLButtonElement | null>(null);
  const removeAccountConfirmCancelRef = useRef<HTMLButtonElement | null>(null);
  const [streamSurfacePresent, setStreamSurfacePresent] = useState(false);
  const [launchSurfacePresent, setLaunchSurfacePresent] = useState(false);
  const [settingsSurfacePresent, setSettingsSurfacePresent] = useState(false);
  const [navbarOverlayBlocking, setNavbarOverlayBlocking] = useState(false);
  const [logoutConfirmSurfacePresent, setLogoutConfirmSurfacePresent] = useState(false);
  const [removeAccountConfirmSurfacePresent, setRemoveAccountConfirmSurfacePresent] = useState(false);
  const [releaseHighlightsSurfacePresent, setReleaseHighlightsSurfacePresent] = useState(false);
  const [controllerModePromptSurfacePresent, setControllerModePromptSurfacePresent] = useState(false);
  const streamRevealComplete = streamRevealPhase === "revealed";
  useEffect(() => {
    isStreamingRef.current = streamStatus === "streaming";
  }, [streamStatus]);

  useEffect(() => {
    if (streamStatus !== "streaming") {
      setVideoElementHasFrame(false);
      setStreamRevealPhase("covered");
    }

    if (streamStatus === "idle") return undefined;

    const video = videoRef.current;
    if (!video) return undefined;

    const syncVideoFrame = (): void => {
      setVideoElementHasFrame(video.videoWidth > 0 && video.videoHeight > 0);
    };

    syncVideoFrame();
    video.addEventListener("loadeddata", syncVideoFrame);
    video.addEventListener("playing", syncVideoFrame);
    video.addEventListener("resize", syncVideoFrame);
    return () => {
      video.removeEventListener("loadeddata", syncVideoFrame);
      video.removeEventListener("playing", syncVideoFrame);
      video.removeEventListener("resize", syncVideoFrame);
    };
  }, [streamStatus]);

  const streamVideoReady = isStreamVideoReady(streamStatus, diagnosticsVideoReady, videoElementHasFrame);

  useEffect(() => {
    if (streamStatus === "idle" || !streamVideoReady || streamRevealPhase !== "covered") return;
    setStreamRevealPhase(reducedMotion ? "revealed" : "revealing");
  }, [reducedMotion, streamRevealPhase, streamStatus, streamVideoReady]);

  useEffect(() => {
    if (streamStatus !== "idle") setStreamSurfacePresent(true);
  }, [streamStatus]);

  useEffect(() => {
    if (streamStatus !== "idle" || launchError) setLaunchSurfacePresent(true);
  }, [launchError, streamStatus]);

  useEffect(() => {
    if (currentPage === "settings") setSettingsSurfacePresent(true);
  }, [currentPage]);

  useEffect(() => {
    if (releaseHighlightsPayload) setReleaseHighlightsSurfacePresent(true);
  }, [releaseHighlightsPayload]);

  useEffect(() => {
    if (feedbackOpen) setFeedbackSurfacePresent(true);
  }, [feedbackOpen]);

  useEffect(() => {
    if (controllerModePromptOpen) setControllerModePromptSurfacePresent(true);
  }, [controllerModePromptOpen]);

  useEffect(() => {
    if (settingsLoaded && settings.errorReportingConsent === "unset") {
      setConsentSurfacePresent(true);
    }
  }, [settings.errorReportingConsent, settingsLoaded]);

  useEffect(() => {
    if (!settingsLoaded) {
      return;
    }
    void syncRendererTelemetry(settings).catch((error) => {
      console.warn("[Telemetry] Failed to sync renderer PostHog:", error);
    });
    // Only re-sync when consent or install identity changes — not on every settings keystroke.
    // eslint-disable-next-line react-hooks/exhaustive-deps -- intentional narrow deps
  }, [settings.errorReportingConsent, settings.telemetryInstallId, settingsLoaded]);

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
  const exitPromptResolverRef = useRef<((confirmed: boolean) => void) | null>(null);



  type CatalogOps = {
    hydrateCatalogSnapshot: (session: AuthSession, proxyUrl?: string) => string | null;
    loadSessionRuntimeData: (session: AuthSession, options?: { background?: boolean; proxyUrl?: string }) => Promise<void>;
    clearSessionCatalog: (mode: "logout" | "no-session", options?: { clearFeatured?: boolean }) => void;
    resetStorePanels: () => void;
    setVariantByGameId: (value: Record<string, string> | ((prev: Record<string, string>) => Record<string, string>)) => void;
  };
  const catalogOpsRef = useRef<CatalogOps>({
    hydrateCatalogSnapshot: () => null,
    loadSessionRuntimeData: async () => {},
    clearSessionCatalog: () => {},
    resetStorePanels: () => {},
    setVariantByGameId: (() => {}) as CatalogOps['setVariantByGameId'],
  });
  const resetLaunchRuntimeRef = useRef<(options?: { keepLaunchError?: boolean; keepStreamingContext?: boolean }) => void>(() => {});
  const refreshNavbarActiveSessionRef = useRef<(sessionOverride?: AuthSession) => Promise<void>>(async () => {});
  const navbarActiveSessionRefreshIdRef = useRef(0);

  const onBootstrapSettings = useCallback((loadedSettings: Settings, _sessionProxyUrl: string | undefined) => {
    setSettings(loadedSettings);
    setStatsMode(loadedSettings.showStatsOnLaunch ? "compact" : "off");
    setSettingsLoaded(true);
  }, []);

  const onBootstrapVariantSelections = useCallback((selections: Record<string, string>) => {
    catalogOpsRef.current.setVariantByGameId(selections);
  }, []);

  const onBootstrapRuntimeSnapshot = useCallback((snapshot: RuntimeSnapshot | null) => {
    runtimeSnapshotRef.current = snapshot;
    if (snapshot?.recoveryAppId !== null && snapshot?.recoveryAppId !== undefined) {
      signalingRecoveryRef.current.appId = snapshot.recoveryAppId;
    }
  }, []);

  const {
    authSession,
    savedAccounts,
    providers,
    providerIdpId,
    setProviderIdpId,
    isLoggingIn,
    activeLoginMode,
    loginError,
    qrLoginChallenge,
    isInitializing,
    startupStatusMessage,
    startupRefreshNotice,
    removeAccountConfirmOpen,
    setRemoveAccountConfirmOpen,
    logoutConfirmOpen,
    setLogoutConfirmOpen,
    selectedProvider,
    handleLogin,
    handleQrLogin,
    handleCancelQrLogin,
    handleSwitchAccount,
    handleRemoveAccount,
    removeAccountNow,
    confirmRemoveAccount,
    handleAddAccount,
    refreshSavedAccounts,
    confirmLogout,
    handleLogout,
    accountToRemoveDisplayName,
    setAccountToRemove,
  } = useAuthSession({
    t,
    loadSessionRuntimeData: (session, options) => catalogOpsRef.current.loadSessionRuntimeData(session, options),
    hydrateCatalogSnapshot: (session, proxyUrl) => catalogOpsRef.current.hydrateCatalogSnapshot(session, proxyUrl),
    clearSessionCatalog: (mode, options) => catalogOpsRef.current.clearSessionCatalog(mode, options),
    resetLaunchRuntime: (options) => resetLaunchRuntimeRef.current(options),
    refreshNavbarActiveSession: (sessionOverride) => refreshNavbarActiveSessionRef.current(sessionOverride),
    onBootstrapSettings,
    onBootstrapVariantSelections,
    onBootstrapRuntimeSnapshot,
    setCurrentPage,
    setNavbarActiveSession,
    setIsResumingNavbarSession,
  });

  useEffect(() => {
    if (logoutConfirmOpen) setLogoutConfirmSurfacePresent(true);
  }, [logoutConfirmOpen]);

  useEffect(() => {
    if (removeAccountConfirmOpen) setRemoveAccountConfirmSurfacePresent(true);
  }, [removeAccountConfirmOpen]);

  const effectiveControllerModeForCatalog = settings.controllerMode || directLaunchConsoleMode;
  const effectiveStreamingBaseUrlForCatalog = settings.region.trim()
    ? settings.region
    : (selectedProvider?.streamingServiceUrl ?? "");

  const {
    games,
    storePanels,
    libraryGames,
    searchQuery,
    setSearchQuery,
    selectedGameId,
    setSelectedGameId,
    variantByGameId,
    setVariantByGameId,
    isLoadingCatalog,
    isLoadingLibrary,
    isLoadingStorePanels,
    catalogFilterGroups,
    catalogSortOptions,
    catalogSelectedSortId,
    setCatalogSelectedSortId,
    catalogSelectedFilterIds,
    catalogTotalCount,
    catalogSupportedCount,
    markOwnedInFlightByVariantId,
    catalogActionNotice,
    regions,
    setRegions,
    subscriptionInfo,
    setSubscriptionInfo,
    allKnownGames,
    resetStorePanels,
    hydrateCatalogSnapshot,
    loadSessionRuntimeData,
    clearSessionCatalog,
    handleMarkGameOwned,
    handleSelectGameVariant,
    handleToggleCatalogFilter,
    loadSubscriptionInfo,
  } = useCatalogData({
    authSession,
    activeSessionProxyUrl,
    effectiveStreamingBaseUrl: effectiveStreamingBaseUrlForCatalog,
    currentPage,
    effectiveControllerMode: effectiveControllerModeForCatalog,
    isInitializing,
    t,
  });

  catalogOpsRef.current = {
    hydrateCatalogSnapshot,
    loadSessionRuntimeData,
    clearSessionCatalog,
    resetStorePanels,
    setVariantByGameId,
  };

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

  const queueDirectLaunchRequest = useCallback((request: DirectLaunchRequest | null): void => {
    if (!request || handledDirectLaunchIdsRef.current.has(request.id)) return;
    setDirectLaunchConsoleMode(true);
    setPendingDirectLaunchRequest((previous) => previous?.id === request.id ? previous : request);
  }, []);

  useEffect(() => {
    const unsubscribe = window.openNow.onDirectLaunchRequest(queueDirectLaunchRequest);
    void window.openNow.getPendingDirectLaunchRequest()
      .then(queueDirectLaunchRequest)
      .catch((error) => {
        console.warn("Failed to read pending direct launch request:", error);
      });
    return unsubscribe;
  }, [queueDirectLaunchRequest]);

  // Subscribe to automatic release-highlights events pushed from main process
  useEffect(() => {
    const unsubscribe = window.openNow.onReleaseHighlightsShow((payload) => {
      setReleaseHighlightsPayload(payload);
      setReleaseHighlightsIsAuto(true);
    });
    return unsubscribe;
  }, []);

  const resetLaunchRuntime = useCallback((options?: {
    keepLaunchError?: boolean;
    keepStreamingContext?: boolean;
  }): void => {
    resetRecoveryConnectionState();
    discordStreamingActivitySessionRef.current = null;
    signalingRecoveryRef.current.attemptCount = 0;
    signalingRecoveryRef.current.deadlineAtMs = null;
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
    window.openNow.notifyNativeInputModeChange(false, false);
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
  }, [diagnosticsStore, resetRecoveryConnectionState, resetStatsOverlayToPreference, settings.discordRichPresence]);

  resetLaunchRuntimeRef.current = resetLaunchRuntime;

  const markDiscordStreamStarted = useCallback((): void => {
    if (!settings.discordRichPresence) {
      return;
    }

    const activeSession = sessionRef.current;
    if (!activeSession || discordStreamingActivitySessionRef.current === activeSession.sessionId) {
      return;
    }

    const gameName = (streamingGameRef.current?.title || activeSession.appId || "Game").trim();
    const gameImageUrl = streamingGameRef.current
      ? discordGameImageUrl(streamingGameRef.current)
      : undefined;
    discordStreamingActivitySessionRef.current = activeSession.sessionId;
    void window.openNow.setDiscordActivity({
      gameName,
      gameImageUrl,
      kind: "streaming",
      appId: activeSession.appId,
      startTimestampMs: Date.now(),
    });
  }, [settings.discordRichPresence]);

  // Console shell is active when the user enabled Controller Mode or the app was
  // launched with a direct-launch argument (frontend / big picture usage).
  const effectiveControllerMode = settings.controllerMode || directLaunchConsoleMode;

  const consoleShell = useConsoleShell({
    controllerMode: effectiveControllerMode,
    directLaunchConsoleMode,
    pickerEnabled: settings.consoleProfilePickerOnLaunch,
    isInitializing,
    hasAuthSession: authSession !== null,
    savedAccounts,
    activeUserId: authSession?.user.userId ?? null,
    switchFailedMessage: t("console.profiles.switchFailed"),
    onSwitchAccount: handleSwitchAccount,
    onAddAccount: handleAddAccount,
    onRemoveAccount: removeAccountNow,
  });

  const buildCurrentStreamSettings = useCallback((subscriptionOverride?: SubscriptionInfo | null): StreamSettings => {
    const currentSubscription = subscriptionOverride === undefined ? subscriptionInfo : subscriptionOverride;
    const entitledProfile = resolveEntitledStreamProfile(currentSubscription?.entitledResolutions ?? [], {
      resolution: settings.resolution,
      fps: settings.fps,
    });
    const streamProfile = entitledProfile ?? SAFE_FALLBACK_STREAM_PROFILE;
    const codecProfile = resolveStreamProfileCodec(
      settings.codec,
      settings.colorQuality,
      codecResults,
    );

    return {
      resolution: streamProfile.resolution,
      fps: streamProfile.fps,
      maxBitrateMbps: settings.maxBitrateMbps,
      codec: codecProfile.codec,
      colorQuality: codecProfile.colorQuality,
      keyboardLayout: settings.keyboardLayout,
      gameLanguage: settings.gameLanguage,
      enableL4S: settings.enableL4S,
      enableCloudGsync: settings.enableCloudGsync,
      clientMode: settings.streamClientMode,
      transportMode: settings.transportMode,
      nativeCloudGsyncMode: settings.nativeCloudGsyncMode,
      nativeTransitionDiagnostics: settings.nativeTransitionDiagnostics,
      appLaunchMode: resolveAppLaunchMode({
        controllerMode: settings.controllerMode,
        requestGamepadFriendlySession: settings.launchInConsoleMode,
        directLaunchConsoleMode,
      }),
    };
  }, [
    settings.codec,
    settings.colorQuality,
    codecResults,
    settings.controllerMode,
    directLaunchConsoleMode,
    settings.enableCloudGsync,
    settings.enableL4S,
    settings.fps,
    settings.gameLanguage,
    settings.keyboardLayout,
    settings.launchInConsoleMode,
    settings.maxBitrateMbps,
    settings.nativeCloudGsyncMode,
    settings.nativeTransitionDiagnostics,
    settings.resolution,
    settings.streamClientMode,
    settings.transportMode,
    subscriptionInfo?.entitledResolutions,
  ]);

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

  // Derived state

  const effectiveStreamingBaseUrl = useMemo(() => {
    if (settings.region.trim()) {
      return settings.region;
    }
    return selectedProvider?.streamingServiceUrl ?? "";
  }, [selectedProvider, settings.region]);

  const resolveSubscriptionInfoForLaunch = useCallback(async (): Promise<SubscriptionInfo | null> => {
    if (subscriptionInfo) {
      return subscriptionInfo;
    }

    const token = authSession?.tokens.idToken ?? authSession?.tokens.accessToken;
    if (!authSession || !token) {
      return null;
    }

    try {
      const subscription = await window.openNow.fetchSubscription({
        token,
        providerStreamingBaseUrl: effectiveStreamingBaseUrl,
        userId: authSession.user.userId,
      });
      setSubscriptionInfo(subscription);
      return subscription;
    } catch (error) {
      console.warn("Failed to resolve subscription before launch; using safe stream profile fallback:", error);
      return null;
    }
  }, [authSession, effectiveStreamingBaseUrl, subscriptionInfo]);

  const resolveInstallToPlayStreamingBaseUrl = useCallback(async (
    game: GameInfo,
    subscription: SubscriptionInfo | null,
    token: string | undefined,
  ): Promise<string | undefined> => {
    let availableRegions: StreamRegion[] = regions;
    if (availableRegions.length === 0) {
      try {
        availableRegions = await window.openNow.getRegions({ token });
        if (availableRegions.length > 0) {
          setRegions(availableRegions);
        }
      } catch (error) {
        console.warn("[I2P] Failed to load regions for persistent storage routing:", error);
      }
    }

    const storageRegionUrl = resolveInstallToPlayStorageRegionUrl(game, subscription, availableRegions);
    if (storageRegionUrl) {
      console.log("[I2P] Routing install-to-play launch to persistent storage region", {
        title: game.title,
        storageRegion: subscription?.storageAddon?.regionName,
        storageRegionUrl,
      });
    } else if (game.playType === "INSTALL_TO_PLAY") {
      console.warn("[I2P] No matching persistent storage region found; using selected/default region", {
        title: game.title,
        storageRegion: subscription?.storageAddon?.regionName,
      });
    }
    return storageRegionUrl ?? undefined;
  }, [regions]);

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


  const refreshNavbarActiveSession = useCallback(async (
    sessionOverride?: AuthSession,
    streamingBaseUrlOverride?: string,
  ): Promise<void> => {
    const refreshId = ++navbarActiveSessionRefreshIdRef.current;
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
      if (navbarActiveSessionRefreshIdRef.current !== refreshId) return;
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
      if (navbarActiveSessionRefreshIdRef.current === refreshId) {
        console.warn("Failed to refresh active sessions:", error);
      }
    }
  }, [authSession, effectiveStreamingBaseUrl, settings.region]);

  refreshNavbarActiveSessionRef.current = refreshNavbarActiveSession;

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
    if (!authSession || streamStatus !== "idle") {
      return;
    }
    const refresh = async (): Promise<void> => {
      if (document.visibilityState === "hidden") return;
      await refreshNavbarActiveSession();
    };
    const handleVisibilityChange = (): void => {
      if (document.visibilityState !== "hidden") {
        void refresh();
      }
    };
    void refresh();
    const timer = window.setInterval(() => {
      void refresh();
    }, 10000);
    document.addEventListener("visibilitychange", handleVisibilityChange);
    return () => {
      navbarActiveSessionRefreshIdRef.current += 1;
      window.clearInterval(timer);
      document.removeEventListener("visibilitychange", handleVisibilityChange);
    };
  }, [authSession, refreshNavbarActiveSession, streamStatus]);

  useEffect(() => {
    saveStoredCodecResults(codecResults);
  }, [codecResults]);


  useEffect(() => {
    if (codecResults || codecTesting || codecStartupTestAttemptedRef.current) {
      return;
    }
    const runStartupCodecTest = (): void => {
      if (codecStartupTestAttemptedRef.current) return;
      codecStartupTestAttemptedRef.current = true;
      void runCodecTest();
    };
    if (typeof window.requestIdleCallback === "function") {
      const idleCallbackId = window.requestIdleCallback(runStartupCodecTest, { timeout: 4000 });
      return () => window.cancelIdleCallback(idleCallbackId);
    }
    const timer = window.setTimeout(runStartupCodecTest, 1500);
    return () => window.clearTimeout(timer);
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

  const nativeStreamerShortcuts = useMemo(() => ({
    toggleStats: shortcuts.toggleStats.canonical,
    togglePointerLock: shortcuts.togglePointerLock.canonical,
    toggleFullscreen: shortcuts.toggleFullscreen.canonical,
    stopStream: shortcuts.stopStream.canonical,
    toggleAntiAfk: shortcuts.toggleAntiAfk.canonical,
    toggleMicrophone: shortcuts.toggleMicrophone.canonical,
    screenshot: "",
    toggleRecording: "",
  }), [shortcuts]);

  const buildSignalingConnectRequest = useCallback((activeSession: SessionInfo): SignalingConnectRequest => {
    const streamSettings = buildCurrentStreamSettings();
    return {
      sessionId: activeSession.sessionId,
      signalingServer: activeSession.signalingServer,
      signalingUrl: activeSession.signalingUrl,
      nativeStreamer: buildNativeStreamerSessionContext(activeSession, streamSettings, nativeStreamerShortcuts),
    };
  }, [buildCurrentStreamSettings, nativeStreamerShortcuts]);

  // Propagate shortcut binding changes to native process during active session
  useEffect(() => {
    if (streamStatus !== "streaming" || !session || !nativeStreamingRef.current) {
      return;
    }
    window.openNow.updateNativeShortcuts(nativeStreamerShortcuts);
  }, [nativeStreamerShortcuts, session, streamStatus]);

  const setSessionFullscreen = useCallback(async (nextFullscreen: boolean) => {
    const canUseNativeFullscreen = typeof window.openNow?.setFullscreen === "function";
    if (document.pointerLockElement) {
      clientRef.current?.suppressNextSyntheticEscapeOnPointerLockLoss();
    }

    if (canUseNativeFullscreen) {
      try {
        // Electron owns desktop fullscreen. Stacking HTML fullscreen on top lets
        // Chromium force-exit the DOM layer on Escape before stream input runs.
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
    const lockTarget = getStreamPointerLockTarget(target);
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

  const setNativeInputPaused = useCallback((paused: boolean): void => {
    if (!nativeStreamingRef.current && settings.streamClientMode !== "native") {
      return;
    }
    window.openNow.setNativeInputPaused(paused);
  }, [settings.streamClientMode]);

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

  // Escape-hold (and other explicit exit requests) from main — never toggle back on.
  useEffect(() => {
    const unsubscribe = window.openNow.onExitFullscreen(() => {
      void setSessionFullscreen(false);
    });
    return () => unsubscribe();
  }, [setSessionFullscreen]);

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
    if (nativeStreamingRef.current && !nativeInputBridgeReady) return;

    const interval = window.setInterval(() => {
      clientRef.current?.sendAntiAfkPulse();
    }, 240000); // 4 minutes

    return () => clearInterval(interval);
  }, [antiAfkEnabled, nativeInputBridgeReady, streamStatus]);

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

  useEffect(() => {
    applyTheme(settings.appTheme);

    if (settings.appTheme === "auto") {
      const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
      const handler = () => applyTheme("auto");
      mediaQuery.addEventListener("change", handler);
      return () => mediaQuery.removeEventListener("change", handler);
    }
  }, [settings.appTheme]);

  useEffect(() => {
    applyTranslucentUI(settings.translucentUI);
  }, [settings.translucentUI]);


  const previewSetting = useCallback(<K extends keyof Settings>(key: K, value: Settings[K]): void => {
    setSettings((prev) => (Object.is(prev[key], value) ? prev : { ...prev, [key]: value }));

    // If a running client exists, push supported settings live while controls move.
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
    if (key === "clipboardPaste") {
      try {
        (clientRef.current as any)?.setClipboardPasteEnabled?.(value as boolean);
      } catch {
        // ignore
      }
    }
    if (key === "nativeCursorOverlay") {
      try {
        clientRef.current?.setNativeCursorOverlayEnabled(value as boolean);
      } catch {
        // ignore
      }
    }
    if (key === "maxBitrateMbps") {
      try {
        void clientRef.current?.setMaxBitrateKbps((value as number) * 1000);
      } catch {
        // ignore
      }
    }
  }, []);

  const updateSetting = useCallback(async <K extends keyof Settings>(key: K, value: Settings[K]): Promise<void> => {
    previewSetting(key, value);
    if (settingsLoaded) {
      await window.openNow.setSetting(key, value);
    }
    if (key === "identifyAsSteamDeck" && authSession) {
      try {
        await loadSubscriptionInfo(authSession);
      } catch (error) {
        console.warn("Failed to refresh subscription after Steam Deck identity change:", error);
      }
    }
  }, [authSession, loadSubscriptionInfo, previewSetting, settingsLoaded]);

  useEffect(() => {
    if (!settingsLoaded) {
      return;
    }
    const unsupportedCodec = getCodecToMigrateToAuto(settings.codec, codecResults);
    if (!unsupportedCodec) {
      return;
    }
    console.warn(
      `[Codec] Saved codec "${unsupportedCodec}" is unavailable for WebRTC decode; migrating to auto`,
    );
    void updateSetting("codec", "auto");
  }, [codecResults, settings.codec, settingsLoaded, updateSetting]);

  useEffect(() => {
    if (!settingsLoaded || !subscriptionInfo) {
      return;
    }

    const entitledProfile = resolveEntitledStreamProfile(subscriptionInfo.entitledResolutions, {
      resolution: settings.resolution,
      fps: settings.fps,
    }) ?? SAFE_FALLBACK_STREAM_PROFILE;

    if (entitledProfile.resolution !== settings.resolution) {
      void updateSetting("resolution", entitledProfile.resolution);
    }
    if (entitledProfile.fps !== settings.fps) {
      void updateSetting("fps", entitledProfile.fps);
    }
  }, [
    settings.fps,
    settings.resolution,
    settingsLoaded,
    subscriptionInfo,
    updateSetting,
  ]);

  const handleMouseSensitivityChange = useCallback((value: number) => {
    void updateSetting("mouseSensitivity", value);
  }, [updateSetting]);

  const handleMouseAccelerationChange = useCallback((value: number) => {
    void updateSetting("mouseAcceleration", value);
  }, [updateSetting]);

  const handleVideoShaderChange = useCallback((value: VideoShaderSettings) => {
    void updateSetting("videoShader", value);
  }, [updateSetting]);

  const handleFrameInterpolationChange = useCallback((value: FrameInterpolationSettings) => {
    void updateSetting("frameInterpolation", value);
  }, [updateSetting]);

  const handleExitApp = useCallback(() => {
    appUnloadingRef.current = true;
    persistRuntimeSnapshotNow();
    void window.openNow.quitApp().catch((error) => {
      console.warn("Failed to quit application:", error);
    });
  }, [persistRuntimeSnapshotNow]);

  // Argument-driven (direct) launches behave like a console frontend session:
  // when the streamed session ends cleanly, close OpenNOW and return to the caller.
  useEffect(() => {
    if (!directLaunchConsoleMode) return;
    if (streamStatus !== "idle") {
      directLaunchSessionSeenRef.current = true;
      return;
    }
    if (!directLaunchSessionSeenRef.current) return;
    if (launchError) return; // Keep the app open so the failure stays visible.
    console.log("[DirectLaunch] Session ended; quitting OpenNOW");
    handleExitApp();
  }, [directLaunchConsoleMode, streamStatus, launchError, handleExitApp]);

  const handleMicrophoneModeChange = useCallback((value: import("@shared/gfn").MicrophoneMode) => {
    // Keep UI responsive while still surfacing persistence failures.
    void updateSetting("microphoneMode", value).catch((error) => {
      console.warn("Failed to persist microphone mode setting:", error);
    });
  }, [updateSetting]);

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
    setNativeInputBridgeReady(false);
    setNativeInputCaptureActive(false);
    try {
      window.openNow.notifyPointerLockChange(false, true);
    } catch {
      /* best-effort */
    }
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

        const launchSubscription = await resolveSubscriptionInfoForLaunch();
        const streamSettings = buildCurrentStreamSettings(launchSubscription);
        const claimed = await window.openNow.claimSession({
          token,
          streamingBaseUrl: effectiveStreamingBaseUrl,
          serverIp: existingSession.serverIp,
          sessionId: existingSession.sessionId,
          ...resolveResumeIdentity(existingSession.sessionId),
          appId: resolveSessionClaimAppId(existingSession),
          appLaunchMode: existingSession.appLaunchMode,
          enablePersistingInGameSettings: existingSession.enablePersistingInGameSettings,
          settings: streamSettings,
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
  }, [applyClaimedSessionAndConnect, authSession, buildCurrentStreamSettings, effectiveStreamingBaseUrl, findGameContextForSession, resolveResumeIdentity, resolveSessionClaimAppId, resolveSubscriptionInfoForLaunch, warmNativeStreamerForLaunch]);

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

    const now = Date.now();
    recoveryState.deadlineAtMs ??= now + SIGNALING_RECOVERY_WINDOW_MS;
    if (now >= recoveryState.deadlineAtMs) {
      console.warn("[Recovery] Recovery window expired");
      return false;
    }

    const attemptPromise = (async (): Promise<boolean> => {
      clientRef.current?.dispose();
      clientRef.current = null;
      setStreamStatus("connecting");
      await disconnectSignalingControlled();

      let lastError: Error | null = null;
      while (Date.now() < (recoveryState.deadlineAtMs ?? 0)) {
        const pollDelayMs = nextSignalingRecoveryPollDelayMs({
          attemptCount: recoveryState.attemptCount,
          online: navigator.onLine !== false,
          nowMs: Date.now(),
          deadlineAtMs: recoveryState.deadlineAtMs ?? 0,
        });
        if (pollDelayMs === null) break;
        if (pollDelayMs > 0) await sleep(pollDelayMs);
        if (!isRecoveryGenerationCurrent(recoveryGeneration)) {
          console.log("[Recovery] Aborting attempt after explicit shutdown");
          return false;
        }
        if (navigator.onLine === false) {
          console.log("[Recovery] Network is offline; waiting before polling the session again");
          continue;
        }

        recoveryState.attemptCount += 1;
        const attemptNumber = recoveryState.attemptCount;
        console.warn(
          `[Recovery] Attempt ${attemptNumber} after signaling disconnect: ${reason}`,
        );

        try {
          const activeSessions = await window.openNow.getActiveSessions(token, effectiveStreamingBaseUrl);
          if (!isRecoveryGenerationCurrent(recoveryGeneration)) {
            console.log("[Recovery] Aborting attempt after active session lookup due to stale generation");
            return false;
          }
          const previousAppId = recoveryState.appId;
          const currentSessionId = currentSession.sessionId;
          const persisted = runtimeSnapshotRef.current?.resumeContext ?? null;
          const recoveryCandidate = selectRecoveryCandidate(
            activeSessions,
            currentSessionId,
            previousAppId,
            persisted,
          );
          const candidate = recoveryCandidate.candidate;
          if (recoveryCandidate.source === "persisted-resume-context" && persisted) {
            console.log("[Recovery] Falling back to persisted resume context", {
              sessionId: persisted.sessionId,
              serverIp: persisted.serverIp,
              appId: persisted.appId ?? previousAppId ?? null,
            });
          }

          if (!candidate) {
            if (recoveryCandidate.hasQueueOnlyMatch) {
              throw new Error("The session is still queued and cannot be reclaimed until the server marks it ready again.");
            }
            throw new Error("The running session could not be found anymore, so resume was not possible.");
          }

          if (!candidate.serverIp) {
            throw new Error("The running session is missing a server address, so resume was not possible.");
          }

          const recoverySubscription = await resolveSubscriptionInfoForLaunch();
          const recoveryStreamSettings = buildCurrentStreamSettings(recoverySubscription);
          const claimed = await window.openNow.claimSession({
            token,
            streamingBaseUrl: effectiveStreamingBaseUrl,
            serverIp: candidate.serverIp,
            sessionId: candidate.sessionId,
            ...resolveResumeIdentity(candidate.sessionId),
            recoveryMode: true,
            appId: resolveSessionClaimAppId(candidate),
            appLaunchMode: candidate.appLaunchMode,
            enablePersistingInGameSettings: candidate.enablePersistingInGameSettings,
            settings: recoveryStreamSettings,
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
    resolveSubscriptionInfoForLaunch,
  ]);

  const handleRemoteSessionEnd = useCallback((reason: string): void => {
    if (signalingRecoveryRef.current.explicitShutdown) {
      console.log("[Session] Ignoring duplicate remote end after shutdown");
      return;
    }
    const status = streamStatusRef.current;
    const activeSession = sessionRef.current;
    const reasonCode = remoteSessionEndCode(reason) ?? "RemoteSessionEnded";
    const endedAtMs = Date.now();
    const streamedForMs = sessionStartedAtMs === null
      ? null
      : Math.max(0, endedAtMs - sessionStartedAtMs);
    const premature = shouldSurfaceRemoteSessionEnd(sessionStartedAtMs, endedAtMs);
    console.warn("[Session] Remote stream ended without a local stop request", {
      reason: reasonCode,
      phase: status,
      sessionId: activeSession?.sessionId ?? null,
      appId: activeSession?.appId ?? null,
      receivedFirstFrame: sessionStartedAtMs !== null,
      streamedForMs,
      premature,
    });
    const activeGameId = streamingGameRef.current?.id;
    if (activeGameId) {
      endPlaytimeSession(activeGameId);
    }
    markExplicitSignalingShutdown();
    clientRef.current?.dispose();
    clientRef.current = null;
    launchInFlightRef.current = false;
    if (premature) {
      setLaunchError(toRemoteSessionEndedError(t, status, reason));
      resetLaunchRuntime({ keepLaunchError: true, keepStreamingContext: true });
    } else {
      resetLaunchRuntime();
    }
    void refreshNavbarActiveSession();
  }, [endPlaytimeSession, markExplicitSignalingShutdown, refreshNavbarActiveSession, resetLaunchRuntime, sessionStartedAtMs, t]);

  useSignalingEvents({
    runtime: streamRuntime,
    attemptSessionRecovery,
    diagnosticsStore,
    handleRemoteSessionEnd,
    markDiscordStreamStarted,
    refreshNavbarActiveSession,
    resetLaunchRuntime,
    scheduleStableRecoveryReset,
    settings,
    t,
  });

  const { handlePlayGame } = useGameLaunch({
    runtime: streamRuntime,
    activeSessionProxyUrl,
    allKnownGames,
    authSession,
    buildCurrentStreamSettings,
    buildSignalingConnectRequest,
    canLaunch: Boolean(selectedProvider),
    claimAndConnectSession,
    disconnectSignalingControlled,
    effectiveStreamingBaseUrl,
    queueAdPlaybackRef,
    refreshNavbarActiveSession,
    resetLaunchRuntime,
    resetSignalingRecoveryState,
    resetStatsOverlayToPreference,
    resolveInstallToPlayStreamingBaseUrl,
    resolveSubscriptionInfoForLaunch,
    settings,
    startPlaytimeSession,
    stopSessionByTarget,
    subscriptionInfo,
    t,
    variantByGameId,
    warmNativeStreamerForLaunch,
  });

  useEffect(() => {
    const request = pendingDirectLaunchRequest;
    if (!request || handledDirectLaunchIdsRef.current.has(request.id)) return;
    if (!authSession || isInitializing || isLoadingCatalog || isLoadingLibrary) return;
    if (
      launchInFlightRef.current ||
      streamStatus !== "idle" ||
      isResumingNavbarSession ||
      isTerminatingNavbarSession ||
      navbarSessionActionInFlightRef.current
    ) {
      return;
    }
    if (directLaunchAttemptIdRef.current === request.id) return;

    directLaunchAttemptIdRef.current = request.id;
    let cancelled = false;

    const launch = async (): Promise<void> => {
      let target = findDirectLaunchTarget(request, allKnownGames, variantByGameId);
      const token = authSession.tokens.idToken ?? authSession.tokens.accessToken;

      if (!target && request.title && token) {
        try {
          const searchResult = await window.openNow.browseCatalog({
            token,
            providerStreamingBaseUrl: effectiveStreamingBaseUrl,
            proxyUrl: activeSessionProxyUrl,
            searchQuery: request.title,
            sortId: "relevance",
            filterIds: [],
            fetchCount: 25,
          });
          target = findDirectLaunchTarget(request, searchResult.games, variantByGameId);
        } catch (error) {
          console.warn("Direct launch catalog search failed:", error);
        }
      }

      const requestedAppId = request.appId && isNumericId(request.appId) ? request.appId : undefined;
      if (!target && requestedAppId) {
        target = {
          game: createSyntheticDirectLaunchGame(request, requestedAppId),
          variantId: requestedAppId,
        };
      }

      if (cancelled) return;

      if (
        launchInFlightRef.current ||
        streamStatusRef.current !== "idle" ||
        navbarSessionActionInFlightRef.current
      ) {
        directLaunchAttemptIdRef.current = null;
        return;
      }

      handledDirectLaunchIdsRef.current.add(request.id);
      directLaunchAttemptIdRef.current = null;
      setPendingDirectLaunchRequest((previous) => previous?.id === request.id ? null : previous);

      if (!target) {
        const requestedName = request.title?.trim() || request.appId || t("app.labels.game");
        setLaunchError({
          stage: "queue",
          title: t("errors.directLaunchNotFoundTitle"),
          description: t("errors.directLaunchNotFoundDescription", { value: requestedName }),
        });
        return;
      }

      void handlePlayGame(target.game, { variantId: target.variantId });
    };

    void launch();

    return () => {
      cancelled = true;
      if (directLaunchAttemptIdRef.current === request.id) {
        directLaunchAttemptIdRef.current = null;
      }
    };
  }, [
    allKnownGames,
    activeSessionProxyUrl,
    authSession,
    effectiveStreamingBaseUrl,
    handlePlayGame,
    isInitializing,
    isLoadingCatalog,
    isLoadingLibrary,
    isResumingNavbarSession,
    isTerminatingNavbarSession,
    pendingDirectLaunchRequest,
    streamStatus,
    t,
    variantByGameId,
  ]);

  const [detailsGame, setDetailsGame] = useState<GameInfo | null>(null);
  const [detailsSurfacePresent, setDetailsSurfacePresent] = useState(false);
  const queueModalVariantIdRef = useRef<string | undefined>(undefined);
  const handleOpenDetails = useCallback((game: GameInfo): void => {
    setDetailsSurfacePresent(true);
    setDetailsGame(game);
  }, []);
  const handleCloseDetails = useCallback((): void => {
    setDetailsGame(null);
  }, []);

  // Gate handler: shows queue server modal for FREE-tier users before launching
  const handleInitiatePlay = useCallback(async (game: GameInfo, variantId?: string) => {
    const effectiveTier = normalizeMembershipTier(
      subscriptionInfo?.membershipTier ?? authSession?.user.membershipTier,
    );
    const isFreeUser = effectiveTier === "FREE";
    const activeProvider = authSession?.provider ?? selectedProvider;
    const isNvidiaAccount = isNvidiaProvider(activeProvider);
    const isAllianceServer = isAllianceStreamingBaseUrl(effectiveStreamingBaseUrl);
    if (!isNvidiaAccount || isAllianceServer) {
      setQueueModalData(null);
      queueModalVariantIdRef.current = undefined;
      void handlePlayGame(game, { variantId });
      return;
    }
    if (settings.hideServerSelector) {
      setQueueModalData(null);
      queueModalVariantIdRef.current = undefined;
      void handlePlayGame(game, { variantId });
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
          queueModalVariantIdRef.current = undefined;
          void handlePlayGame(game, { variantId });
          return;
        }

        const queueData = queueResult.value;
        if (!queueData || Object.keys(queueData).length === 0) {
          setQueueModalData(null);
          queueModalVariantIdRef.current = undefined;
          void handlePlayGame(game, { variantId });
          return;
        }

        if (!hasAnyEligiblePrintedWasteZone(queueData, mappingResult.value)) {
          console.warn(
            "[QueueServerSelect] No eligible non-nuked PrintedWaste zones available, skipping queue checks.",
          );
          setQueueModalData(null);
          queueModalVariantIdRef.current = undefined;
          void handlePlayGame(game, { variantId });
          return;
        }

        setQueueModalData(queueData);
        queueModalVariantIdRef.current = variantId;
        setQueueModalGame(game);
      } catch (error) {
        console.warn("[QueueServerSelect] PrintedWaste queue checks failed, launching without modal.", error);
        setQueueModalData(null);
        queueModalVariantIdRef.current = undefined;
        void handlePlayGame(game, { variantId });
      }
      return;
    }
    queueModalVariantIdRef.current = undefined;
    void handlePlayGame(game, { variantId });
  }, [subscriptionInfo, authSession, selectedProvider, settings.hideServerSelector, streamStatus, handlePlayGame, effectiveStreamingBaseUrl]);

  const handleQueueModalConfirm = useCallback((zoneUrl: string | null) => {
    const game = queueModalGame;
    const variantId = queueModalVariantIdRef.current;
    queueModalVariantIdRef.current = undefined;
    setQueueModalGame(null);
    setQueueModalData(null);
    if (game) {
      void handlePlayGame(game, { streamingBaseUrl: zoneUrl ?? undefined, variantId });
    }
  }, [queueModalGame, handlePlayGame]);

  const handleQueueModalCancel = useCallback(() => {
    queueModalVariantIdRef.current = undefined;
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
      proxyUrl: activeSessionProxyUrl,
      appIdOrUuid: game.uuid ?? game.id,
      variantId: selectedVariant?.id ?? selectedVariantId,
      store: selectedVariant?.store,
    }).then((storeUrl) => {
      if (storeUrl) handleOpenStoreUrl(storeUrl);
    }).catch((error) => {
      console.error("Failed to resolve Store URL:", error);
    });
  }, [activeSessionProxyUrl, authSession, effectiveStreamingBaseUrl, handleOpenStoreUrl]);

  const closeRemoveAccountConfirm = (): void => {
    setRemoveAccountConfirmOpen(false);
    setAccountToRemove(null);
  };

  const logoutConfirmModal = (
    <ModalSurface
      open={logoutConfirmOpen}
      onClose={() => setLogoutConfirmOpen(false)}
      onConfirm={() => {
        void confirmLogout();
      }}
      onExitComplete={() => setLogoutConfirmSurfacePresent(false)}
      motion="compact"
      overlayClassName="logout-confirm"
      backdropClassName="logout-confirm-backdrop"
      panelClassName="logout-confirm-card"
      ariaLabel={t("auth.accounts.logOutConfirmation")}
      backdropLabel={t("auth.accounts.cancelLogOut")}
      initialFocusRef={logoutConfirmCancelRef}
      restoreFocusRef={accountConfirmRestoreFocusRef}
    >
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
          ref={logoutConfirmCancelRef}
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
    </ModalSurface>
  );

  const removeAccountConfirmModal = (
    <ModalSurface
      open={removeAccountConfirmOpen}
      onClose={closeRemoveAccountConfirm}
      onConfirm={() => {
        void confirmRemoveAccount();
      }}
      onExitComplete={() => setRemoveAccountConfirmSurfacePresent(false)}
      motion="compact"
      overlayClassName="logout-confirm"
      backdropClassName="logout-confirm-backdrop"
      panelClassName="logout-confirm-card"
      ariaLabel={t("auth.accounts.removeAccountConfirmation")}
      backdropLabel={t("auth.accounts.cancelAccountRemoval")}
      initialFocusRef={removeAccountConfirmCancelRef}
      restoreFocusRef={accountConfirmRestoreFocusRef}
    >
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
          ref={removeAccountConfirmCancelRef}
          type="button"
          className="logout-confirm-btn logout-confirm-btn-cancel"
          onClick={closeRemoveAccountConfirm}
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
    </ModalSurface>
  );

  const { handleResumeFromNavbar, handleTerminateNavbarSession } = useActiveSessionActions({
    runtime: streamRuntime,
    canResume: Boolean(selectedProvider),
    claimAndConnectSession,
    disconnectSignalingControlled,
    effectiveStreamingBaseUrl,
    findGameContextForSession,
    gameTitleByAppId,
    refreshNavbarActiveSession,
    resetLaunchRuntime,
    resetSignalingRecoveryState,
    resetStatsOverlayToPreference,
    stopSessionByTarget,
    t,
  });

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

  const handleLaunchErrorAction = useCallback((): void => {
    if (launchError?.action !== "persistent-storage-settings") return;
    void (async () => {
      try {
        await handleDismissLaunchError();
      } finally {
        setSettingsFocusSection("account");
        if (currentPage !== "settings") {
          setPageBeforeSettings(currentPage);
        }
        setCurrentPage("settings");
      }
    })();
  }, [currentPage, handleDismissLaunchError, launchError?.action]);

  const releasePointerLockIfNeeded = useCallback(async () => {
    if (document.pointerLockElement) {
      clientRef.current?.suppressNextSyntheticEscapeOnPointerLockLoss();
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

  const handleStreamShortcutAction = useCallback((action: NativeStreamerShortcutAction): void => {
    switch (action) {
      case "toggleStats":
        setStatsMode(nextStatsOverlayMode);
        return;
      case "togglePointerLock":
        if (nativeStreamingRef.current) {
          // Native streamer toggles OS input capture locally in the renderer window.
          return;
        }
        {
          const targetVideo = videoRef.current;
          if (streamStatus === "streaming" && targetVideo) {
            if (isStreamPointerLocked(targetVideo)) {
              clientRef.current?.suppressNextSyntheticEscapeOnPointerLockLoss();
              document.exitPointerLock();
            } else {
              void requestPointerLockCapture(targetVideo);
            }
          }
        }
        return;
      case "toggleFullscreen":
        if (streamStatus === "connecting" || streamStatus === "streaming") {
          void toggleSessionFullscreen();
        }
        return;
      case "stopStream":
        void handlePromptedStopStream();
        return;
      case "toggleAntiAfk":
        if (streamStatus === "streaming") {
          setAntiAfkEnabled((prev) => !prev);
          setAntiAfkAckNonce((n) => n + 1);
        }
        return;
      case "toggleMicrophone":
        if (streamStatus === "streaming") {
          clientRef.current?.toggleMicrophone();
        }
        return;
      case "screenshot":
      case "toggleRecording":
        if (streamStatus === "streaming" && !nativeStreamingRef.current) {
          dispatchStreamShortcutAction(action);
        }
        return;
    }
  }, [handlePromptedStopStream, requestPointerLockCapture, streamStatus, toggleSessionFullscreen]);

  useEffect(() => {
    handleStreamShortcutActionRef.current = handleStreamShortcutAction;
  }, [handleStreamShortcutAction]);

  useEffect(() => {
    return window.openNow.onStreamShortcutAction(handleStreamShortcutAction);
  }, [handleStreamShortcutAction]);

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

      const isPasteShortcut = e.key.toLowerCase() === "v" && !e.altKey && !e.shiftKey && (e.ctrlKey || (isMac && e.metaKey));
      if (streamStatus === "streaming" && isPasteShortcut) {
        // Always stop local/browser paste behavior while streaming.
        // If clipboard paste is enabled, send clipboard text into the stream.
        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation();

        if (settings.clipboardPaste) {
          void (async () => {
            await sendStreamClipboardPaste(clientRef.current);
          })();
        }
        return;
      }

      if (isShortcutMatch(e, shortcuts.toggleStats)) {
        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation();
        handleStreamShortcutAction("toggleStats");
        return;
      }

      if (isShortcutMatch(e, shortcuts.togglePointerLock)) {
        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation();
        handleStreamShortcutAction("togglePointerLock");
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
  const librarySortOptions = useMemo(
    () => catalogSortOptions.filter((option) => option.id !== "relevance"),
    [catalogSortOptions],
  );

  const activeSessionAppIds = useMemo(
    () => (navbarActiveSession ? [navbarActiveSession.appId] : []),
    [navbarActiveSession?.appId],
  );

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
    const nextPage = pages[nextIndex];
    if (nextPage === "settings" && currentPage !== "settings") {
      setPageBeforeSettings(currentPage);
    }
    setCurrentPage(nextPage);
  }, [currentPage]);
  const navigateToPreviousControllerPage = useCallback((): void => {
    navigateControllerPage(-1);
  }, [navigateControllerPage]);
  const navigateToNextControllerPage = useCallback((): void => {
    navigateControllerPage(1);
  }, [navigateControllerPage]);

  const handleNavigate = useCallback((page: AppPage): void => {
    if (page === "settings" && currentPage !== "settings") {
      setPageBeforeSettings(currentPage);
    }
    setCurrentPage(page);
  }, [currentPage]);

  const handleCloseSettings = useCallback((): void => {
    setSettingsFocusSection(undefined);
    setCurrentPage(pageBeforeSettings);
  }, [pageBeforeSettings]);

  const handleOpenWhatsNew = useCallback((): void => {
    // Fetch current-version highlights and open modal in manual mode (no auto-ack)
    void window.openNow.getReleaseHighlights()
      .then((payload) => {
        setReleaseHighlightsPayload(payload);
        setReleaseHighlightsIsAuto(false);
      })
      .catch((error) => {
        console.warn("[App] Failed to fetch release highlights:", error);
      });
  }, []);

  const handleDismissReleaseHighlights = useCallback((): void => {
    if (releaseHighlightsIsAuto) {
      void window.openNow.ackReleaseHighlights().catch((err) => {
        console.warn("[App] Failed to ack release highlights:", err);
      });
    }
    setReleaseHighlightsPayload(null);
    setReleaseHighlightsIsAuto(false);
  }, [releaseHighlightsIsAuto]);

  const handleNavbarResumeSession = useCallback((): void => {
    void handleResumeFromNavbar();
  }, [handleResumeFromNavbar]);
  const handleNavbarTerminateSession = useCallback((): void => {
    void handleTerminateNavbarSession();
  }, [handleTerminateNavbarSession]);
  const handleNavbarRemoveAccount = useCallback((userId: string, restoreFocusTarget?: HTMLElement): void => {
    accountConfirmRestoreFocusRef.current = restoreFocusTarget ?? null;
    void handleRemoveAccount(userId);
  }, [handleRemoveAccount]);
  const handleNavbarLogoutAll = useCallback((restoreFocusTarget?: HTMLElement): void => {
    accountConfirmRestoreFocusRef.current = restoreFocusTarget ?? null;
    handleLogout();
  }, [handleLogout]);
  const openFeedback = useCallback((
    category: FeedbackCategory,
    report?: DesktopSessionReport | null,
  ): void => {
    const currentReport = report !== undefined
      ? report
      : sessionReportAccumulatorRef.current
        ? sessionReportAccumulatorRef.current.finish()
        : latestSessionReport;
    setFeedbackInitialCategory(category);
    setFeedbackSessionReport(currentReport);
    setFeedbackOpen(true);
  }, [latestSessionReport]);

  const handleOpenFeedback = useCallback((): void => {
    openFeedback("bug");
  }, [openFeedback]);

  const handleOpenStreamBugReport = useCallback((): void => {
    openFeedback("bug");
  }, [openFeedback]);

  const handleSessionReportBug = useCallback((report: DesktopSessionReport): void => {
    setSessionReportOpen(false);
    openFeedback("bug", report);
  }, [openFeedback]);

  const handleErrorReportingConsent = useCallback(async (granted: boolean): Promise<void> => {
    await updateSetting("errorReportingConsent", granted ? "granted" : "denied");
    try {
      const refreshed = await window.openNow.getSettings();
      setSettings((prev) => ({
        ...prev,
        errorReportingConsent: refreshed.errorReportingConsent,
        telemetryInstallId: refreshed.telemetryInstallId,
      }));
    } catch (error) {
      console.warn("[Telemetry] Failed to refresh settings after consent change:", error);
    }
  }, [updateSetting]);

  const showErrorReportingConsent = settingsLoaded && settings.errorReportingConsent === "unset";

  const handleAcceptControllerMode = useCallback((): void => {
    dismissControllerModePrompt();
    void updateSetting("controllerMode", true).catch((error) => {
      console.warn("[Controller Mode] Failed to save controller mode:", error);
    });
  }, [dismissControllerModePrompt, updateSetting]);

  const handleDeclineControllerMode = useCallback((): void => {
    dismissControllerModePrompt();
    void updateSetting("controllerModePromptDismissed", true).catch((error) => {
      console.warn("[Controller Mode] Failed to save prompt preference:", error);
    });
  }, [dismissControllerModePrompt, updateSetting]);

  const controllerModePromptModal = (
    <ControllerModePromptModal
      open={controllerModePromptOpen}
      onAccept={handleAcceptControllerMode}
      onDecline={handleDeclineControllerMode}
      onExitComplete={() => setControllerModePromptSurfacePresent(false)}
    />
  );

  const mainPage: AppPage = currentPage === "settings" ? pageBeforeSettings : currentPage;

  // Show login screen if not authenticated
  if (!authSession) {
    return (
      <>
        <LoginScreen
          providers={providers}
          selectedProviderId={providerIdpId}
          onProviderChange={setProviderIdpId}
          onLogin={handleLogin}
          onQrLogin={handleQrLogin}
          onCancelQrLogin={handleCancelQrLogin}
          isLoading={isLoggingIn}
          error={loginError}
          isInitializing={isInitializing}
          statusMessage={startupStatusMessage}
          qrLoginChallenge={qrLoginChallenge}
          isQrLoginPending={activeLoginMode === "qr" && !qrLoginChallenge}
        />
        <ReleaseHighlightsModal
          payload={releaseHighlightsPayload}
          onDismiss={handleDismissReleaseHighlights}
          onExitComplete={() => setReleaseHighlightsSurfacePresent(false)}
        />
        {controllerModePromptModal}
        <ErrorReportingConsentModal
          open={showErrorReportingConsent}
          onAccept={() => {
            void handleErrorReportingConsent(true);
          }}
          onDecline={() => {
            void handleErrorReportingConsent(false);
          }}
          onExitComplete={() => setConsentSurfacePresent(false)}
        />
        <FeedbackModal
          open={feedbackOpen}
          settings={settings}
          initialCategory={feedbackInitialCategory}
          sessionReport={feedbackSessionReport}
          onClose={() => setFeedbackOpen(false)}
          onExitComplete={() => setFeedbackSurfacePresent(false)}
        />
      </>
    );
  }

  const showLaunchOverlay = streamStatus !== "idle" || launchError !== null;
  const hasActiveStreamView = streamStatus !== "idle";
  const showLaunchErrorOverlay = launchError !== null;
  const showDesktopLaunchLoading = showLaunchErrorOverlay
    || (streamStatus !== "idle" && streamRevealPhase !== "revealed");
  const loadingStatus = launchError
    ? launchError.stage
    : streamStatus === "streaming"
      ? "connecting"
      : toLoadingStatus(streamStatus);
  const showCatalogAtmosphere = mainPage === "home" || mainPage === "library";
  const consoleGateOpen = consoleShell.stage !== "shell";
  const shellBlocked = consoleGateOpen
    || showLaunchOverlay
    || streamSurfacePresent
    || launchSurfacePresent
    || currentPage === "settings"
    || settingsSurfacePresent
    || navbarOverlayBlocking
    || detailsGame !== null
    || detailsSurfacePresent
    || queueModalGame !== null
    || releaseHighlightsPayload !== null
    || releaseHighlightsSurfacePresent
    || showErrorReportingConsent
    || consentSurfacePresent
    || feedbackOpen
    || feedbackSurfacePresent
    || sessionReportOpen
    || controllerModePromptOpen
    || controllerModePromptSurfacePresent
    || logoutConfirmOpen
    || logoutConfirmSurfacePresent
    || removeAccountConfirmOpen
    || removeAccountConfirmSurfacePresent;
  const catalogSurfaceActive = !shellBlocked;

  return (
    <div className={`app-container${effectiveControllerMode ? " app-container--controller" : ""}${showCatalogAtmosphere ? " app-container--atmosphere" : ""}`} style={getAppStyle(settings.posterSizeScale)}>
      {/* Sibling of the shell, not a child: `shellBlocked` already marks the
          shell inert and suspends its gamepad pollers, so exactly one poller
          is live while the gate is open. */}
      <ConsoleProfileGate
        shell={consoleShell}
        savedAccounts={savedAccounts}
        activeUserId={authSession?.user.userId ?? null}
        onAddAccount={handleAddAccount}
        onProfilesChanged={async () => { await refreshSavedAccounts(); }}
        onRemoveAccount={removeAccountNow}
        onLogoutAll={() => setLogoutConfirmOpen(true)}
      />
      <div
        className="app-shell"
        inert={shellBlocked ? true : undefined}
        aria-hidden={shellBlocked || undefined}
      >
      {showCatalogAtmosphere && (
        <LazyShaderAtmosphere
          variant="controller"
          className="catalog-atmosphere"
          active={catalogSurfaceActive}
        />
      )}
      <AnimatePresence>
        {startupRefreshNotice && (
          <m.div
            key="startup-refresh-notice"
            className={`auth-refresh-notice auth-refresh-notice--${startupRefreshNotice.tone}`}
            {...overlayMotion}
          >
            {startupRefreshNotice.text}
          </m.div>
        )}
      </AnimatePresence>
      <AnimatePresence>
        {catalogActionNotice && (
          <m.div
            key="catalog-action-notice"
            className={`auth-refresh-notice auth-refresh-notice--${catalogActionNotice.tone}`}
            {...overlayMotion}
          >
            {catalogActionNotice.text}
          </m.div>
        )}
      </AnimatePresence>
      <Navbar
        currentPage={currentPage}
        onNavigate={handleNavigate}
        user={authSession.user}
        subscription={subscriptionInfo}
        activeSession={navbarActiveSession}
        activeSessionGameTitle={activeSessionGameTitle}
        isResumingSession={isResumingNavbarSession}
        isTerminatingSession={isTerminatingNavbarSession}
        onResumeSession={handleNavbarResumeSession}
        onTerminateSession={handleNavbarTerminateSession}
        savedAccounts={savedAccounts}
        onOpenProfilePicker={consoleShell.openPicker}
        onSwitchAccount={handleSwitchAccount}
        onRemoveAccount={handleNavbarRemoveAccount}
        onAddAccount={handleAddAccount}
        onLogoutAll={handleNavbarLogoutAll}
        onExitApp={handleExitApp}
        onOpenFeedback={handleOpenFeedback}
        onBlockingOverlayChange={setNavbarOverlayBlocking}
        controllerMode={effectiveControllerMode}
      />

      <main className="main-content">
        <PageErrorBoundary label="main">
        <AnimatePresence mode="wait" initial={false}>
          <m.div
            key={mainPage}
            className="page-transition-surface"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={pageTransition}
          >
            {mainPage === "home" && (
              <HomePage
                games={filteredGames}
                searchQuery={searchQuery}
                onSearchChange={setSearchQuery}
                onPlayGame={handleInitiatePlay}
                isLoading={effectiveControllerMode ? isLoadingStorePanels : isLoadingCatalog}
                selectedGameId={selectedGameId}
                onSelectGame={setSelectedGameId}
                onOpenDetails={handleOpenDetails}
                selectedVariantByGameId={variantByGameId}
                onSelectGameVariant={handleSelectGameVariant}
                filterGroups={catalogFilterGroups}
                selectedFilterIds={catalogSelectedFilterIds}
                onToggleFilter={handleToggleCatalogFilter}
                sortOptions={catalogSortOptions}
                selectedSortId={catalogSelectedSortId}
                onSortChange={setCatalogSelectedSortId}
                totalCount={catalogTotalCount}
                supportedCount={catalogSupportedCount}
                controllerMode={effectiveControllerMode}
                surfaceActive={catalogSurfaceActive}
                storePanels={storePanels}
                activeSessionAppIds={activeSessionAppIds}
                onBuyGame={handleBuyGame}
                onMarkGameOwned={handleMarkGameOwned}
                markOwnedInFlightByVariantId={markOwnedInFlightByVariantId}
                onPreviousControllerPage={navigateToPreviousControllerPage}
                onNextControllerPage={navigateToNextControllerPage}
              />
            )}

            {mainPage === "library" && (
              <PageErrorBoundary label="library">
                <LibraryPage
                  games={filteredLibraryGames}
                  allGames={libraryGames}
                  playtimeData={playtime}
                  searchQuery={searchQuery}
                  onSearchChange={setSearchQuery}
                  onPlayGame={handleInitiatePlay}
                  isLoading={isLoadingLibrary}
                  selectedGameId={selectedGameId}
                  onSelectGame={setSelectedGameId}
                  onOpenDetails={handleOpenDetails}
                  selectedVariantByGameId={variantByGameId}
                  onSelectGameVariant={handleSelectGameVariant}
                  libraryCount={libraryGames.length}
                  sortOptions={librarySortOptions}
                  selectedSortId={catalogSelectedSortId === "relevance" ? "last_played" : catalogSelectedSortId}
                  onSortChange={setCatalogSelectedSortId}
                  controllerMode={effectiveControllerMode}
                  surfaceActive={catalogSurfaceActive}
                  activeSessionAppIds={activeSessionAppIds}
                  onBuyGame={handleBuyGame}
                  onPreviousControllerPage={navigateToPreviousControllerPage}
                  onNextControllerPage={navigateToNextControllerPage}
                />
              </PageErrorBoundary>
            )}
          </m.div>
        </AnimatePresence>
        </PageErrorBoundary>
      </main>
      </div>

      <AnimatePresence
        initial={false}
        onExitComplete={() => setStreamSurfacePresent(false)}
      >
        {hasActiveStreamView && (
          <m.div
            key="stream-view-layer"
            className="stream-view-layer"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={pageTransition}
          >
            <StreamView
              videoRef={videoRef}
              audioRef={audioRef}
              diagnosticsStore={diagnosticsStore}
              statsMode={statsMode}
              statsPosition={settings.statsOverlayPosition}
              showNativeStats={settings.showNativeStreamerStats}
              nativeInputCaptureActive={nativeInputCaptureActive}
              nativeStreamingEnabled={settings.streamClientMode === "native"}
              nativeExternalRenderer={settings.nativeExternalRenderer}
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
              antiAfkReminderEveryMinutes={settings.antiAfkReminderEveryMinutes}
              antiAfkReminderDurationSeconds={settings.antiAfkReminderDurationSeconds}
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
              streamRevealComplete={streamRevealComplete}
              isStreaming={isStreaming}
              recordingBitrateMbps={settings.recordingBitrateMbps}
              recordingResolution={settings.recordingResolution}
              recordingFps={settings.recordingFps}
              onRecordingResolutionChange={(value) => {
                void updateSetting("recordingResolution", value);
              }}
              onRecordingFpsChange={(value) => {
                void updateSetting("recordingFps", value);
              }}
              onRecordingBitrateMbpsChange={(value) => {
                void updateSetting("recordingBitrateMbps", value);
              }}
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
              onReportBug={handleOpenStreamBugReport}
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
              onNativeInputPaused={setNativeInputPaused}
              allowEscapeToExitFullscreen={settings.allowEscapeToExitFullscreen}
              videoShader={settings.videoShader}
              onVideoShaderChange={handleVideoShaderChange}
              frameInterpolation={settings.frameInterpolation}
              onFrameInterpolationChange={handleFrameInterpolationChange}
            />
          </m.div>
        )}
      </AnimatePresence>

      <AnimatePresence
        initial={false}
        onExitComplete={() => setLaunchSurfacePresent(false)}
      >
        {showDesktopLaunchLoading && (
          <m.div
            key="stream-loading-transition"
            className={`stream-loading-transition${streamRevealPhase === "revealing" && !launchError ? " stream-loading-transition--warping" : ""}`}
            initial={{ opacity: 1, scale: 1 }}
            animate={streamRevealPhase === "revealing" && !launchError
              ? { opacity: 0, scale: 1.025 }
              : { opacity: 1, scale: 1 }}
            exit={{ opacity: 0 }}
            transition={streamRevealPhase === "revealing" && !launchError
              ? streamRevealTransition
              : { duration: reducedMotion ? 0 : 0.16 }}
            onAnimationComplete={() => {
              if (streamRevealPhase === "revealing" && !launchError) {
                setStreamRevealPhase("revealed");
              }
            }}
          >
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
              error={launchError ? {
                title: launchError.title,
                description: launchError.description,
                code: launchError.codeLabel,
                actionLabel: launchError.actionLabel,
              } : undefined}
              onErrorAction={launchError?.action ? handleLaunchErrorAction : undefined}
              onCancel={() => {
                if (launchError) {
                  void handleDismissLaunchError();
                  return;
                }
                void handlePromptedStopStream();
              }}
            />
          </m.div>
        )}
      </AnimatePresence>

      <SettingsModalHost
        open={currentPage === "settings"}
        onClose={handleCloseSettings}
        onExitComplete={() => setSettingsSurfacePresent(false)}
      >
        <SettingsPage
          settings={settings}
          regions={regions}
          codecResults={codecResults}
          codecTesting={codecTesting}
          onRunCodecTest={runCodecTest}
          onSettingPreview={previewSetting}
          onSettingChange={updateSetting}
          onClose={handleCloseSettings}
          focusSection={settingsFocusSection}
          onOpenWhatsNew={handleOpenWhatsNew}
          onOpenFeedback={() => setFeedbackOpen(true)}
        />
      </SettingsModalHost>
      {logoutConfirmModal}
      {removeAccountConfirmModal}
      <GameDetailModal
        open={detailsGame !== null}
        game={detailsGame}
        selectedVariantId={detailsGame ? variantByGameId[detailsGame.id] : undefined}
        onSelectVariant={(variantId) => {
          if (detailsGame) handleSelectGameVariant(detailsGame.id, variantId);
        }}
        onPlay={(game, variantId) => {
          handleCloseDetails();
          void handleInitiatePlay(game, variantId);
        }}
        onClose={handleCloseDetails}
        onExitComplete={() => setDetailsSurfacePresent(false)}
      />
      {queueModalGame && streamStatus === "idle" && (
        <QueueServerSelectModal
          game={queueModalGame}
          initialQueueData={queueModalData}
          onConfirm={handleQueueModalConfirm}
          onCancel={handleQueueModalCancel}
        />
      )}
      <ReleaseHighlightsModal
        payload={releaseHighlightsPayload}
        onDismiss={handleDismissReleaseHighlights}
        onExitComplete={() => setReleaseHighlightsSurfacePresent(false)}
      />
      {controllerModePromptModal}
      <ErrorReportingConsentModal
        open={showErrorReportingConsent}
        onAccept={() => {
          void handleErrorReportingConsent(true);
        }}
        onDecline={() => {
          void handleErrorReportingConsent(false);
        }}
        onExitComplete={() => setConsentSurfacePresent(false)}
      />
      <FeedbackModal
        open={feedbackOpen}
        settings={settings}
        initialCategory={feedbackInitialCategory}
        sessionReport={feedbackSessionReport}
        onClose={() => setFeedbackOpen(false)}
        onExitComplete={() => setFeedbackSurfacePresent(false)}
      />
      <SessionReportModal
        open={sessionReportOpen}
        report={latestSessionReport}
        onClose={() => setSessionReportOpen(false)}
        onReportBug={handleSessionReportBug}
        onShowReportsChange={(show) => {
          void updateSetting("showSessionReport", show);
        }}
      />
    </div>
  );
}
