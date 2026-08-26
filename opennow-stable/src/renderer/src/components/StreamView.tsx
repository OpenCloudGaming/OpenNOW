import { useState, useEffect, useCallback, useRef } from "react";
import { createPortal } from "react-dom";
import { AnimatePresence, m } from "motion/react";
import type { JSX } from "react";
import { Maximize, Minimize, LogOut, AlertTriangle } from "lucide-react";
import { SessionStartedSplash } from "./SessionStartedSplash";
import { StreamStatsHud } from "./StreamStatsHud";
import type { StreamDiagnosticsStore } from "../utils/streamDiagnosticsStore";
import { useStreamDiagnosticsSelector } from "../utils/streamDiagnosticsStore";
import { getStoreDisplayName, getStoreIconComponent } from "./GameCard";
import { SessionElapsedIndicator } from "./ElapsedSessionIndicators";
import {
  videoShaderHasVisibleEffect,
  type FrameInterpolationSettings,
  type MicrophoneMode,
  type RecordingFps,
  type RecordingResolution,
  type StatsOverlayPosition,
  type SubscriptionInfo,
  type VideoShaderSettings,
} from "@shared/gfn";
import { VideoShaderPipeline } from "../platforms/gfn/videoShaderPipeline";
import { FrameInterpolationPipeline } from "../platforms/gfn/frameInterpolationPipeline";
import { formatShortcutForDisplay } from "../shortcuts";
import { useScreenshotGallery } from "../hooks/useScreenshotGallery";
import { useStreamMenuNavigation } from "../hooks/useStreamMenuNavigation";
import { useStreamRecorder } from "../hooks/useStreamRecorder";
import { formatSessionTimeRemaining, formatWarningSeconds } from "./stream/streamFormatters";
import { AntiAfkIndicator, MicrophoneIndicator, RecordingIndicator } from "./stream/StreamIndicators";
import { StreamTitleBar } from "./stream/StreamTitleBar";
import {
  hasVisibleStreamVideo,
  StreamEmptyState,
  StreamWaitingForVideo,
  VideoFocusOnReady,
} from "./stream/StreamEmptyStates";
import { StreamQuickMenu } from "./stream/quick-menu/StreamQuickMenu";
import { MotionSpinner } from "./MotionSpinner";
import { isStreamPointerLocked } from "../lib/pointerLock";
import type { StatsOverlayMode } from "../utils/streamStatsHud";
import { RecurringReminderScheduler, shouldScheduleAntiAfkReminder } from "./stream/antiAfkReminder";
import { usesNativeInternalSurface } from "./stream/nativePresentation";

const ANTI_AFK_TOGGLE_ACK_MS = 5000;
const CONTROLLER_SIDEBAR_SHORTCUT_DISPLAY = "View + Menu";

interface StreamViewProps {
  videoRef: React.Ref<HTMLVideoElement>;
  audioRef: React.Ref<HTMLAudioElement>;
  diagnosticsStore: StreamDiagnosticsStore;
  statsMode: StatsOverlayMode;
  statsPosition: StatsOverlayPosition;
  showNativeStats?: boolean;
  nativeInputCaptureActive?: boolean;
  nativeStreamingEnabled: boolean;
  nativeExternalRenderer?: boolean;
  shortcuts: {
    toggleStats: string;
    togglePointerLock: string;
    toggleFullscreen: string;
    stopStream: string;
    toggleAntiAfk: string;
    toggleMicrophone?: string;
    screenshot: string;
    recording: string;
  };
  hideStreamButtons?: boolean;
  serverRegion?: string;
  antiAfkEnabled: boolean;
  antiAfkAckNonce: number;
  showAntiAfkIndicator: boolean;
  antiAfkReminderEveryMinutes: number;
  antiAfkReminderDurationSeconds: number;
  exitPrompt: {
    open: boolean;
    gameTitle: string;
  };
  sessionStartedAtMs: number | null;
  isStreaming: boolean;
  sessionCounterEnabled: boolean;
  showSessionTimeRemainingInStatsOverlay: boolean;
  sessionTimeRemainingSeconds: number | null;
  sessionClockShowEveryMinutes: number;
  sessionClockShowDurationSeconds: number;
  streamWarning: {
    code: 1 | 2 | 3;
    message: string;
    tone: "warn" | "critical";
    secondsLeft?: number;
  } | null;
  isFullscreen: boolean;
  isConnecting: boolean;
  streamRevealComplete: boolean;
  gameTitle: string;
  recordingBitrateMbps: number | null;
  recordingResolution: RecordingResolution;
  recordingFps: RecordingFps;
  platformStore?: string;
  onToggleFullscreen: () => void;
  onConfirmExit: () => void;
  onCancelExit: () => void;
  onEndSession: () => void;
  onReportBug: () => void;
  onToggleMicrophone?: () => void;
  mouseSensitivity: number;
  onMouseSensitivityChange: (value: number) => void;
  mouseAcceleration: number;
  onMouseAccelerationChange: (value: number) => void;
  onRequestPointerLock?: () => void;
  onReleasePointerLock?: () => void;
  onNativeInputPaused?: (paused: boolean) => void;
  microphoneMode: MicrophoneMode;
  onMicrophoneModeChange: (value: MicrophoneMode) => void;
  onScreenshotShortcutChange: (value: string) => void;
  onRecordingShortcutChange: (value: string) => void;
  onRecordingResolutionChange: (value: RecordingResolution) => void;
  onRecordingFpsChange: (value: RecordingFps) => void;
  onRecordingBitrateMbpsChange: (value: number | null) => void;
  onShowSessionTimeRemainingInStatsOverlayChange: (value: boolean) => void;
  subscriptionInfo: SubscriptionInfo | null;
  micTrack?: MediaStreamTrack | null;
  className?: string;
  allowEscapeToExitFullscreen?: boolean;
  videoShader: VideoShaderSettings;
  onVideoShaderChange: (value: VideoShaderSettings) => void;
  frameInterpolation: FrameInterpolationSettings;
  onFrameInterpolationChange: (value: FrameInterpolationSettings) => void;
}

export function StreamView({
  videoRef,
  audioRef,
  diagnosticsStore,
  statsMode,
  statsPosition,
  showNativeStats = false,
  nativeInputCaptureActive = false,
  nativeStreamingEnabled,
  nativeExternalRenderer = false,
  shortcuts,
  serverRegion,
  antiAfkEnabled,
  antiAfkAckNonce,
  showAntiAfkIndicator,
  antiAfkReminderEveryMinutes,
  antiAfkReminderDurationSeconds,
  exitPrompt,
  sessionStartedAtMs,
  isStreaming,
  sessionCounterEnabled,
  showSessionTimeRemainingInStatsOverlay,
  sessionTimeRemainingSeconds,
  sessionClockShowEveryMinutes,
  sessionClockShowDurationSeconds,
  streamWarning,
  isFullscreen,
  isConnecting,
  streamRevealComplete,
  gameTitle,
  recordingBitrateMbps,
  recordingResolution,
  recordingFps,
  platformStore,
  onToggleFullscreen,
  onConfirmExit,
  onCancelExit,
  onEndSession,
  onReportBug,
  onToggleMicrophone,
  mouseSensitivity,
  onMouseSensitivityChange,
  mouseAcceleration,
  onMouseAccelerationChange,
  onRequestPointerLock,
  onReleasePointerLock,
  onNativeInputPaused,
  microphoneMode,
  onMicrophoneModeChange,
  onScreenshotShortcutChange,
  onRecordingShortcutChange,
  onRecordingResolutionChange,
  onRecordingFpsChange,
  onRecordingBitrateMbpsChange,
  onShowSessionTimeRemainingInStatsOverlayChange,
  subscriptionInfo,
  micTrack,
  hideStreamButtons = false,
  allowEscapeToExitFullscreen,
  className,
  videoShader,
  onVideoShaderChange,
  frameInterpolation,
  onFrameInterpolationChange,
}: StreamViewProps): JSX.Element {
  const [showHints, setShowHints] = useState(true);
  const [showSessionClock, setShowSessionClock] = useState(false);
  const [antiAfkToggleAck, setAntiAfkToggleAck] = useState<"on" | "off" | null>(null);
  const [antiAfkReminderVisible, setAntiAfkReminderVisible] = useState(false);
  const [isPointerLocked, setIsPointerLocked] = useState(false);
  const [pointerLockHintVisible, setPointerLockHintVisible] = useState(false);
  const pointerLockHintTimerRef = useRef<number | null>(null);
  const nativeRendererActive = useStreamDiagnosticsSelector(
    diagnosticsStore,
    (stats) => stats.nativeRendererActive,
  );
  const localVideoRef = useRef<HTMLVideoElement | null>(null);
  const localAudioRef = useRef<HTMLAudioElement | null>(null);
  const shaderPipelineRef = useRef<VideoShaderPipeline | null>(null);
  const frameInterpolationPipelineRef = useRef<FrameInterpolationPipeline | null>(null);
  const streamHasVideo = useStreamDiagnosticsSelector(
    diagnosticsStore,
    (stats) => hasVisibleStreamVideo(stats),
  );
  const [videoElementHasFrame, setVideoElementHasFrame] = useState(false);

  useEffect(() => {
    if (isConnecting) {
      setVideoElementHasFrame(false);
      return undefined;
    }

    const video = localVideoRef.current;
    if (!video) {
      return undefined;
    }

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
  }, [isConnecting]);

  const streamVideoReady = streamHasVideo || videoElementHasFrame;
  const [sessionReadySplashVisible, setSessionReadySplashVisible] = useState(false);
  const sessionReadySplashShownRef = useRef(false);
  const showStatsHud = statsMode !== "off" && !nativeRendererActive && !isConnecting;

  useEffect(() => {
    if (isConnecting) {
      sessionReadySplashShownRef.current = false;
      setSessionReadySplashVisible(false);
      return;
    }
    if (
      nativeRendererActive
      || !streamVideoReady
      || !streamRevealComplete
      || sessionReadySplashShownRef.current
    ) {
      return;
    }
    sessionReadySplashShownRef.current = true;
    setSessionReadySplashVisible(true);
  }, [isConnecting, nativeRendererActive, streamRevealComplete, streamVideoReady]);

  const handleSessionReadySplashFinished = useCallback(() => {
    setSessionReadySplashVisible(false);
  }, []);

  const handleFullscreenToggle = useCallback(() => {
    onToggleFullscreen();
  }, [onToggleFullscreen]);

  const handlePointerLockToggle = useCallback(() => {
    if (isPointerLocked) {
      if (onReleasePointerLock) {
        onReleasePointerLock();
        return;
      }
      document.exitPointerLock();
      return;
    }
    if (onRequestPointerLock) {
      onRequestPointerLock();
    }
  }, [isPointerLocked, onReleasePointerLock, onRequestPointerLock]);

  useEffect(() => {
    const timer = setTimeout(() => setShowHints(false), 5000);
    return () => clearTimeout(timer);
  }, []);

  useEffect(() => {
    if (!sessionCounterEnabled) {
      setShowSessionClock(false);
      return;
    }

    if (isConnecting) {
      setShowSessionClock(false);
      return;
    }

    const intervalMinutes = Math.max(0, Math.floor(sessionClockShowEveryMinutes || 0));
    const durationSeconds = Math.max(1, Math.floor(sessionClockShowDurationSeconds || 1));
    const intervalMs = intervalMinutes * 60 * 1000;
    const durationMs = durationSeconds * 1000;

    let hideTimer: number | undefined;
    let periodicTimer: number | undefined;

    const showFor = (durationMs: number): void => {
      setShowSessionClock(true);
      if (hideTimer !== undefined) {
        window.clearTimeout(hideTimer);
      }
      hideTimer = window.setTimeout(() => {
        setShowSessionClock(false);
      }, durationMs);
    };

    // Show session clock at stream start.
    showFor(durationMs);

    if (intervalMs > 0) {
      periodicTimer = window.setInterval(() => {
        showFor(durationMs);
      }, intervalMs);
    }

    return () => {
      if (hideTimer !== undefined) {
        window.clearTimeout(hideTimer);
      }
      if (periodicTimer !== undefined) {
        window.clearInterval(periodicTimer);
      }
    };
  }, [isConnecting, sessionClockShowDurationSeconds, sessionClockShowEveryMinutes, sessionCounterEnabled]);

  useEffect(() => {
    if (antiAfkAckNonce === 0 || isConnecting) {
      setAntiAfkToggleAck(null);
      return;
    }

    // Omit transient "on" message when persistent ANTI-AFK badge already shows it
    if (antiAfkEnabled && showAntiAfkIndicator) {
      setAntiAfkToggleAck(null);
      return;
    }

    setAntiAfkToggleAck(antiAfkEnabled ? "on" : "off");

    const hideTimer = window.setTimeout(() => {
      setAntiAfkToggleAck(null);
    }, ANTI_AFK_TOGGLE_ACK_MS);

    return (): void => {
      window.clearTimeout(hideTimer);
    };
  }, [antiAfkAckNonce, antiAfkEnabled, showAntiAfkIndicator, isConnecting]);

  useEffect(() => {
    const intervalMs = Math.max(0, Math.floor(antiAfkReminderEveryMinutes || 0)) * 60 * 1000;
    const durationMs = Math.max(1, Math.floor(antiAfkReminderDurationSeconds || 1)) * 1000;
    const scheduler = new RecurringReminderScheduler(window, setAntiAfkReminderVisible);
    scheduler.start(
      shouldScheduleAntiAfkReminder({
        antiAfkEnabled,
        isStreaming,
        isConnecting,
        showPersistentIndicator: showAntiAfkIndicator,
        intervalMs,
      }),
      intervalMs,
      durationMs,
    );

    return () => scheduler.stop();
  }, [
    antiAfkEnabled,
    antiAfkReminderDurationSeconds,
    antiAfkReminderEveryMinutes,
    isConnecting,
    isStreaming,
    showAntiAfkIndicator,
  ]);

  const warningSeconds = formatWarningSeconds(streamWarning?.secondsLeft);
  const sessionTimeRemainingText = formatSessionTimeRemaining(sessionTimeRemainingSeconds);
  const showSessionTimeRemainingInStats =
    sessionTimeRemainingText !== null && showSessionTimeRemainingInStatsOverlay;
  const platformName = platformStore ? getStoreDisplayName(platformStore) : "";
  const PlatformIcon = platformStore ? getStoreIconComponent(platformStore) : null;
  const isMacClient = navigator.platform?.toLowerCase().includes("mac") || navigator.userAgent.includes("Macintosh");
  const sidebarToggleRaw = isMacClient ? "Meta+G" : "Ctrl+G";
  const sidebarToggleShortcutDisplay = formatShortcutForDisplay(sidebarToggleRaw, isMacClient);

  const screenshotGallery = useScreenshotGallery({
    videoRef: localVideoRef,
    gameTitle,
  });
  const streamRecorder = useStreamRecorder({
    videoRef: localVideoRef,
    audioRef: localAudioRef,
    gameTitle,
    micTrack: micTrack ?? null,
    recordingBitrateMbps,
    recordingResolution,
    recordingFps,
  });
  const releasePointerLockForMenu = useCallback(() => {
    if (document.pointerLockElement) {
      if (onReleasePointerLock) {
        onReleasePointerLock();
      } else {
        document.exitPointerLock();
      }
    }
  }, [onReleasePointerLock]);
  const {
    showSideBar,
    setShowSideBar,
    activeSidebarTab,
    setActiveSidebarTab,
    sidebarRef,
  } = useStreamMenuNavigation({
    shortcuts,
    isMacClient,
    exitPromptOpen: exitPrompt.open,
    selectedScreenshotId: screenshotGallery.selectedScreenshotId,
    setSelectedScreenshotId: screenshotGallery.setSelectedScreenshotId,
    captureScreenshot: screenshotGallery.captureScreenshot,
    toggleRecording: streamRecorder.toggleRecording,
    onCancelExit,
    onConfirmExit,
    onBeforeOpen: releasePointerLockForMenu,
  });
  const suppressVideoFocusOnSidebarCloseRef = useRef(false);

  // Video shader post-processing pipeline (embedded WebRTC path only; the
  // native streamer renders outside Chromium so shaders cannot apply there).
  useEffect(() => {
    const video = localVideoRef.current;
    if (!video) return;
    const effective = nativeRendererActive || (nativeStreamingEnabled && isConnecting)
      ? { ...videoShader, enabled: false }
      : videoShader;
    if (!shaderPipelineRef.current) {
      if (!effective.enabled) return;
      shaderPipelineRef.current = new VideoShaderPipeline(video, effective);
    } else {
      shaderPipelineRef.current.updateSettings(effective);
    }
  }, [videoShader, nativeStreamingEnabled, isConnecting, nativeRendererActive]);

  useEffect(() => {
    const video = localVideoRef.current;
    if (!video) return;
    const effective = nativeStreamingEnabled || nativeRendererActive
      ? { ...frameInterpolation, enabled: false }
      : frameInterpolation;
    if (!frameInterpolationPipelineRef.current) {
      if (!effective.enabled) return;
      frameInterpolationPipelineRef.current = new FrameInterpolationPipeline(video, effective);
    } else {
      frameInterpolationPipelineRef.current.updateSettings(effective);
    }
  }, [frameInterpolation, nativeStreamingEnabled, nativeRendererActive]);

  useEffect(() => () => {
    shaderPipelineRef.current?.dispose();
    shaderPipelineRef.current = null;
    frameInterpolationPipelineRef.current?.dispose();
    frameInterpolationPipelineRef.current = null;
  }, []);

  const setVideoRef = useCallback((element: HTMLVideoElement | null) => {
    localVideoRef.current = element;
    if (typeof videoRef === "function") {
      videoRef(element);
    } else if (videoRef && "current" in videoRef) {
      (videoRef as React.MutableRefObject<HTMLVideoElement | null>).current = element;
    }
  }, [videoRef]);

  const setAudioRef = useCallback((element: HTMLAudioElement | null) => {
    localAudioRef.current = element;
    if (typeof audioRef === "function") {
      audioRef(element);
    } else if (audioRef && "current" in audioRef) {
      (audioRef as React.MutableRefObject<HTMLAudioElement | null>).current = element;
    }
  }, [audioRef]);

  useEffect(() => {
    const updateSurface = window.openNow?.updateNativeRenderSurface;
    if (typeof updateSurface !== "function") {
      return undefined;
    }

    let frame = 0;
    const publish = (): void => {
      const element = localVideoRef.current;
      const dpr = window.devicePixelRatio || 1;
      if (!element || document.visibilityState === "hidden") {
        updateSurface({ rect: null, visible: false, deviceScaleFactor: dpr });
        return;
      }

      const rect = element.getBoundingClientRect();
      const width = Math.round(rect.width * dpr);
      const height = Math.round(rect.height * dpr);
      const visible = width >= 2 && height >= 2 && !showSideBar && !exitPrompt.open;
      updateSurface({
        deviceScaleFactor: dpr,
        visible,
        showStats: statsMode !== "off" || showNativeStats,
        rect: visible
          ? {
              x: Math.round(rect.left * dpr),
              y: Math.round(rect.top * dpr),
              width,
              height,
            }
          : null,
      });
    };

    const schedule = (): void => {
      if (frame !== 0) {
        return;
      }
      frame = window.requestAnimationFrame(() => {
        frame = 0;
        publish();
      });
    };

    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(schedule);
    if (observer && localVideoRef.current) {
      observer.observe(localVideoRef.current);
    }

    window.addEventListener("resize", schedule);
    window.addEventListener("fullscreenchange", schedule);
    document.addEventListener("visibilitychange", schedule);
    window.visualViewport?.addEventListener("resize", schedule);
    window.visualViewport?.addEventListener("scroll", schedule);
    schedule();

    return () => {
      if (frame !== 0) {
        window.cancelAnimationFrame(frame);
      }
      observer?.disconnect();
      window.removeEventListener("resize", schedule);
      window.removeEventListener("fullscreenchange", schedule);
      document.removeEventListener("visibilitychange", schedule);
      window.visualViewport?.removeEventListener("resize", schedule);
      window.visualViewport?.removeEventListener("scroll", schedule);
      updateSurface({
        rect: null,
        visible: false,
        deviceScaleFactor: window.devicePixelRatio || 1,
        showStats: false,
      });
    };
  }, [exitPrompt.open, showNativeStats, showSideBar, statsMode]);

  useEffect(() => {
    const handlePointerLockChange = () => {
      setIsPointerLocked(
        (localVideoRef.current !== null && isStreamPointerLocked(localVideoRef.current))
          || nativeInputCaptureActive,
      );
    };
    handlePointerLockChange();
    document.addEventListener("pointerlockchange", handlePointerLockChange);
    return () => document.removeEventListener("pointerlockchange", handlePointerLockChange);
  }, [nativeInputCaptureActive]);

  useEffect(() => {
    // Show a transient HUD hint when pointer lock is acquired
    if (isPointerLocked) {
      setPointerLockHintVisible(true);
      if (pointerLockHintTimerRef.current) {
        window.clearTimeout(pointerLockHintTimerRef.current);
      }
      pointerLockHintTimerRef.current = window.setTimeout(() => {
        pointerLockHintTimerRef.current = null;
        setPointerLockHintVisible(false);
      }, 3000);
    } else {
      if (pointerLockHintTimerRef.current) {
        window.clearTimeout(pointerLockHintTimerRef.current);
        pointerLockHintTimerRef.current = null;
      }
      setPointerLockHintVisible(false);
    }
    return () => {
      if (pointerLockHintTimerRef.current) {
        window.clearTimeout(pointerLockHintTimerRef.current);
        pointerLockHintTimerRef.current = null;
      }
    };
  }, [isPointerLocked]);

  useEffect(() => {
    onNativeInputPaused?.(showSideBar);
    return () => {
      if (showSideBar) {
        onNativeInputPaused?.(false);
      }
    };
  }, [onNativeInputPaused, showSideBar]);

  useEffect(() => {
    if (showSideBar) {
      // Mark sidebar open so input auto-lock code can avoid re-requesting.
      try {
        document.body.dataset.sidebarOpen = "1";
      } catch {}

      if (onReleasePointerLock) {
        void onReleasePointerLock();
      } else {
        document.exitPointerLock();
      }
      void screenshotGallery.refreshScreenshots();
      void streamRecorder.refreshRecordings();
      return () => {
        try {
          delete (document.body.dataset as DOMStringMap).sidebarOpen;
        } catch {}
      };
    }
    if (suppressVideoFocusOnSidebarCloseRef.current) {
      suppressVideoFocusOnSidebarCloseRef.current = false;
      return undefined;
    }
    // Sidebar just closed — restore focus to the video so clicks register
    // immediately. Without this, focus stays on the last sidebar element and
    // mousedown's preventDefault() blocks the browser from re-focusing on click.
    const timer = window.setTimeout(() => {
      if (localVideoRef.current && document.activeElement !== localVideoRef.current) {
        localVideoRef.current.focus({ preventScroll: true });
      }
    }, 50);
    try {
      delete (document.body.dataset as DOMStringMap).sidebarOpen;
    } catch {}
    return () => clearTimeout(timer);
  }, [screenshotGallery.refreshScreenshots, showSideBar, streamRecorder.refreshRecordings]);

  const handleSidebarExitSession = useCallback(() => {
    suppressVideoFocusOnSidebarCloseRef.current = true;
    setShowSideBar(false);
    onEndSession();
  }, [onEndSession, setShowSideBar]);

  useEffect(() => {
    const blurStreamFocusTarget = (): void => {
      const active = document.activeElement;
      if (active instanceof HTMLElement && active.closest(".sv")) {
        active.blur();
      }
    };

    const hideFocusRingOnAccessKey = (event: KeyboardEvent): void => {
      if (event.key === "Alt" && !event.repeat) {
        blurStreamFocusTarget();
      }
    };

    const restoreStreamVideoFocus = (event: PointerEvent): void => {
      if (showSideBar || isConnecting || exitPrompt.open) {
        return;
      }
      const target = event.target as HTMLElement | null;
      if (target?.closest(".sv-sidebar, .sv-exit, .sv-shot-modal, button, a, input, textarea, select")) {
        return;
      }
      const video = localVideoRef.current;
      if (video && document.activeElement !== video) {
        video.focus({ preventScroll: true });
      }
    };

    window.addEventListener("blur", blurStreamFocusTarget);
    window.addEventListener("keydown", hideFocusRingOnAccessKey, true);
    window.addEventListener("pointerdown", restoreStreamVideoFocus, true);
    return () => {
      window.removeEventListener("blur", blurStreamFocusTarget);
      window.removeEventListener("keydown", hideFocusRingOnAccessKey, true);
      window.removeEventListener("pointerdown", restoreStreamVideoFocus, true);
    };
  }, [exitPrompt.open, isConnecting, showSideBar]);

  const nativeInternalHole = usesNativeInternalSurface({
    nativeRendererActive,
    nativeStreamingEnabled,
    connecting: isConnecting,
    externalRenderer: nativeExternalRenderer === true,
  });

  return (
    <div className={["sv", streamVideoReady ? "sv--video-ready" : "sv--video-pending", nativeInternalHole ? "sv--native-hole" : "", className].filter(Boolean).join(" ")}>
      {nativeInternalHole ? (
        <video
          ref={setVideoRef}
          autoPlay
          playsInline
          muted
          tabIndex={-1}
          className="sv-video sv-video--native-hole"
          onClick={() => {
            if (localVideoRef.current && document.activeElement !== localVideoRef.current) {
              localVideoRef.current.focus({ preventScroll: true });
            }
          }}
        />
      ) : (
        <m.video
          ref={setVideoRef}
          autoPlay
          playsInline
          muted
          tabIndex={-1}
          className="sv-video"
          initial={false}
          animate={streamVideoReady
            ? { opacity: 1, scale: 1 }
            : { opacity: 0, scale: 1.008 }}
          transition={{ duration: 0.9, ease: [0.22, 1, 0.36, 1] }}
          onClick={() => {
            if (localVideoRef.current && document.activeElement !== localVideoRef.current) {
              localVideoRef.current.focus({ preventScroll: true });
            }
          }}
        />
      )}
      <audio ref={setAudioRef} autoPlay playsInline />
      <VideoFocusOnReady
        diagnosticsStore={diagnosticsStore}
        isConnecting={isConnecting}
        videoRef={localVideoRef}
      />

      {pointerLockHintVisible && (
        <div className="sv-pointerlock-hint" role="status" aria-live="polite">
          <div>Press {shortcuts.toggleFullscreen} to exit fullscreen & release mouse</div>
          <div className="sv-pointerlock-hint-sub">
            {allowEscapeToExitFullscreen
              ? "Press Escape will also exit fullscreen per your settings."
              : "Escape goes to the game while pointer-locked; hold Escape ~1.5s to exit fullscreen."}
          </div>
        </div>
      )}

      <StreamQuickMenu
        open={showSideBar}
        onClose={() => setShowSideBar(false)}
        sidebarRef={sidebarRef}
        activeTab={activeSidebarTab}
        setActiveTab={setActiveSidebarTab}
        onEndSession={handleSidebarExitSession}
        onReportBug={onReportBug}
        gameTitle={gameTitle}
        platformName={platformName}
        PlatformIcon={PlatformIcon}
        subscriptionInfo={subscriptionInfo}
        sessionStartedAtMs={sessionStartedAtMs}
        isStreaming={isStreaming}
        sessionTimeRemainingText={sessionTimeRemainingText}
        isFullscreen={isFullscreen}
        isPointerLocked={isPointerLocked}
        onToggleFullscreen={handleFullscreenToggle}
        onTogglePointerLock={handlePointerLockToggle}
        onToggleMicrophone={onToggleMicrophone}
        showSessionTimeRemainingInStatsOverlay={showSessionTimeRemainingInStatsOverlay}
        onShowSessionTimeRemainingInStatsOverlayChange={onShowSessionTimeRemainingInStatsOverlayChange}
        sidebarToggleShortcutDisplay={sidebarToggleShortcutDisplay}
        controllerSidebarShortcutDisplay={CONTROLLER_SIDEBAR_SHORTCUT_DISPLAY}
        mouseSensitivity={mouseSensitivity}
        onMouseSensitivityChange={onMouseSensitivityChange}
        mouseAcceleration={mouseAcceleration}
        onMouseAccelerationChange={onMouseAccelerationChange}
        nativeStreamingEnabled={nativeStreamingEnabled}
        videoShader={videoShader}
        onVideoShaderChange={onVideoShaderChange}
        frameInterpolation={frameInterpolation}
        onFrameInterpolationChange={onFrameInterpolationChange}
        microphoneMode={microphoneMode}
        onMicrophoneModeChange={onMicrophoneModeChange}
        diagnosticsStore={diagnosticsStore}
        micTrack={micTrack ?? null}
        shortcuts={shortcuts}
        isMacClient={isMacClient}
        onScreenshotShortcutChange={onScreenshotShortcutChange}
        onRecordingShortcutChange={onRecordingShortcutChange}
        screenshotGallery={screenshotGallery}
        streamRecorder={streamRecorder}
        recordingBitrateMbps={recordingBitrateMbps}
        recordingResolution={recordingResolution}
        recordingFps={recordingFps}
        onRecordingResolutionChange={onRecordingResolutionChange}
        onRecordingFpsChange={onRecordingFpsChange}
        onRecordingBitrateMbpsChange={onRecordingBitrateMbpsChange}
      />

      {/* Gradient background when no video */}
      <StreamEmptyState diagnosticsStore={diagnosticsStore} />
      <StreamWaitingForVideo diagnosticsStore={diagnosticsStore} isConnecting={isConnecting} />

      {/* Connecting overlay */}
      {isConnecting && (
        <div className="sv-connect">
          <div className="sv-connect-inner">
            <MotionSpinner className="sv-connect-spin" size={44} label="Connecting to stream" />
            <p className="sv-connect-title">Connecting to {gameTitle}</p>
            {PlatformIcon && (
              <div className="sv-connect-platform" title={platformName}>
                <span className="sv-connect-platform-icon">
                  <PlatformIcon />
                </span>
                <span>{platformName}</span>
              </div>
            )}
            <p className="sv-connect-sub">Setting up stream...</p>
          </div>
        </div>
      )}

      {sessionCounterEnabled && !isConnecting && (
        <div
          className={`sv-session-clock${showSessionClock ? " is-visible" : ""}`}
          title="Current gaming session elapsed time"
          aria-hidden={!showSessionClock}
        >
          <SessionElapsedIndicator startedAtMs={sessionStartedAtMs} active={isStreaming} />
        </div>
      )}

      {streamWarning && !isConnecting && !exitPrompt.open && (
        <div
          className={`sv-time-warning sv-time-warning--${streamWarning.tone}`}
          title="Session time warning"
        >
          <AlertTriangle size={14} />
          <span>
            {streamWarning.message}
            {warningSeconds ? ` · ${warningSeconds} left` : ""}
          </span>
        </div>
      )}

      {(antiAfkToggleAck || antiAfkReminderVisible) && !isConnecting && (
        <div className={`sv-afk-ack sv-afk-ack--${antiAfkToggleAck ?? "on"}`} role="status" aria-live="polite">
          <span className="sv-afk-ack-dot" aria-hidden />
          <span>{antiAfkToggleAck === "off" ? "Anti-AFK off" : "Anti-AFK on"}</span>
        </div>
      )}

      <SessionStartedSplash
        visible={sessionReadySplashVisible && !isConnecting}
        gameTitle={gameTitle}
        onFinished={handleSessionReadySplashFinished}
      />

      <AnimatePresence>
        {showStatsHud && (
          <StreamStatsHud
            key="stream-stats-hud"
            diagnosticsStore={diagnosticsStore}
            mode={statsMode === "full" ? "full" : "compact"}
            position={statsPosition}
            nativeStreamingEnabled={nativeStreamingEnabled}
            serverRegion={serverRegion}
            sessionTimeRemainingText={showSessionTimeRemainingInStats ? sessionTimeRemainingText : null}
            hintsVisible={showHints}
            shaderActive={!nativeRendererActive && videoShaderHasVisibleEffect(videoShader)}
          />
        )}
      </AnimatePresence>

      {/* Microphone toggle button */}
      <MicrophoneIndicator
        diagnosticsStore={diagnosticsStore}
        showAntiAfkIndicator={antiAfkEnabled && showAntiAfkIndicator}
        hideStreamButtons={hideStreamButtons}
        isConnecting={isConnecting}
        onToggleMicrophone={onToggleMicrophone}
      />

      {/* Anti-AFK indicator */}
      <AntiAfkIndicator
        diagnosticsStore={diagnosticsStore}
        antiAfkEnabled={antiAfkEnabled}
        showAntiAfkIndicator={showAntiAfkIndicator}
        isConnecting={isConnecting}
      />

      {/* Recording indicator (top-left, stacked below other badges) */}
      <RecordingIndicator
        diagnosticsStore={diagnosticsStore}
        showAntiAfkIndicator={antiAfkEnabled && showAntiAfkIndicator}
        hideStreamButtons={hideStreamButtons}
        isConnecting={isConnecting}
        isRecording={streamRecorder.isRecording}
        onToggleMicrophone={onToggleMicrophone}
        recordingDurationMs={streamRecorder.recordingDurationMs}
      />

      {exitPrompt.open && !isConnecting && typeof document !== "undefined" && createPortal(
        <div className="sv-exit" role="dialog" aria-modal="true" aria-label="Exit stream confirmation">
          <button
            type="button"
            className="sv-exit-backdrop"
            onClick={onCancelExit}
            aria-label="Cancel exit"
          />
          <div className="sv-exit-card">
            <div className="sv-exit-kicker">Session Control</div>
            <h3 className="sv-exit-title">Exit Stream?</h3>
            <p className="sv-exit-text">
              Do you really want to exit <strong>{exitPrompt.gameTitle}</strong>?
            </p>
            <p className="sv-exit-subtext">Your current cloud gaming session will be closed.</p>
            <div className="sv-exit-actions">
              <button type="button" className="sv-exit-btn sv-exit-btn-cancel" onClick={onCancelExit}>
                Keep Playing
              </button>
              <button type="button" className="sv-exit-btn sv-exit-btn-confirm" onClick={onConfirmExit}>
                Exit Stream
              </button>
            </div>
            <div className="sv-exit-hint">
              <span><kbd>Enter</kbd> confirm · <kbd>Esc</kbd> cancel</span>
              <span><kbd>A</kbd> select · <kbd>B</kbd> cancel</span>
            </div>
          </div>
        </div>,
        document.body,
      )}

      {/* Fullscreen toggle */}
      {!hideStreamButtons && (
        <button
          className="sv-fs"
          onClick={handleFullscreenToggle}
          title={isFullscreen ? "Exit fullscreen" : "Enter fullscreen"}
          aria-label={isFullscreen ? "Exit fullscreen" : "Enter fullscreen"}
        >
          {isFullscreen ? <Minimize size={18} /> : <Maximize size={18} />}
        </button>
      )}

      {/* End session button */}
      {!hideStreamButtons && (
        <button
          className="sv-end"
          onClick={onEndSession}
          title="End session"
          aria-label="End session"
        >
          <LogOut size={18} />
        </button>
      )}

      {/* Keyboard hints */}
      {showHints && !isConnecting && (
        <div className="sv-hints">
          <div className="sv-hint"><kbd>{shortcuts.toggleStats}</kbd><span>Stats</span></div>
          <div className="sv-hint"><kbd>{shortcuts.togglePointerLock}</kbd><span>Mouse lock</span></div>
          <div className="sv-hint"><kbd>{shortcuts.toggleFullscreen}</kbd><span>Full screen</span></div>
          <div className="sv-hint"><kbd>{shortcuts.stopStream}</kbd><span>Stop</span></div>
          <div className="sv-hint"><kbd>{CONTROLLER_SIDEBAR_SHORTCUT_DISPLAY}</kbd><span>Controller menu</span></div>
          {shortcuts.toggleMicrophone && <div className="sv-hint"><kbd>{shortcuts.toggleMicrophone}</kbd><span>Mic</span></div>}
        </div>
      )}

      {/* Game title (bottom-center, fades) */}
      <StreamTitleBar
        diagnosticsStore={diagnosticsStore}
        gameTitle={gameTitle}
        platformName={platformName}
        PlatformIcon={PlatformIcon}
        showHints={showHints}
      />
    </div>
  );
}
