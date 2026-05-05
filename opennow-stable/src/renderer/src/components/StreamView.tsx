import { useState, useEffect, useCallback, useRef, useMemo } from "react";
import { createPortal } from "react-dom";
import type { JSX } from "react";
import { Maximize, Minimize, Gamepad2, Loader2, LogOut, Clock3, AlertTriangle, Mic, MicOff, Camera, ChevronLeft, ChevronRight, Save, Trash2, X, Circle, Square, Video, FolderOpen } from "lucide-react";
import SideBar from "./SideBar";
import type { StreamDiagnosticsStore } from "../utils/streamDiagnosticsStore";
import { useStreamDiagnosticsSelector, useStreamDiagnosticsStore } from "../utils/streamDiagnosticsStore";
import type { StreamLagReason } from "../gfn/webrtcClient";
import type { MicState } from "../gfn/microphoneManager";
import { getStoreDisplayName, getStoreIconComponent } from "./GameCard";
import { RemainingPlaytimeIndicator, SessionElapsedIndicator } from "./ElapsedSessionIndicators";
import type { MicrophoneMode, ScreenshotEntry, RecordingEntry, SubscriptionInfo } from "@shared/gfn";
import { formatShortcutForDisplay, isShortcutMatch, normalizeShortcut, shortcutFromKeyboardEvent } from "../shortcuts";
import { useMicMeter } from "../hooks/useMicMeter";

const ANTI_AFK_TOGGLE_ACK_MS = 5000;

interface StreamViewProps {
  videoRef: React.Ref<HTMLVideoElement>;
  audioRef: React.Ref<HTMLAudioElement>;
  diagnosticsStore: StreamDiagnosticsStore;
  showStats: boolean;
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
  exitPrompt: {
    open: boolean;
    gameTitle: string;
  };
  sessionStartedAtMs: number | null;
  isStreaming: boolean;
  sessionCounterEnabled: boolean;
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
  gameTitle: string;
  platformStore?: string;
  onToggleFullscreen: () => void;
  onConfirmExit: () => void;
  onCancelExit: () => void;
  onEndSession: () => void;
  onToggleMicrophone?: () => void;
  mouseSensitivity: number;
  onMouseSensitivityChange: (value: number) => void;
  mouseAcceleration: number;
  onMouseAccelerationChange: (value: number) => void;
  onRequestPointerLock?: () => void;
  onReleasePointerLock?: () => void;
  microphoneMode: MicrophoneMode;
  onMicrophoneModeChange: (value: MicrophoneMode) => void;
  onScreenshotShortcutChange: (value: string) => void;
  onRecordingShortcutChange: (value: string) => void;
  subscriptionInfo: SubscriptionInfo | null;
  micTrack?: MediaStreamTrack | null;
  className?: string;
  allowEscapeToExitFullscreen?: boolean;
  /** When true, omit the in-player connecting overlay (controller mode uses ControllerStreamLoading instead). */
  hideConnectingOverlay?: boolean;
}

function getRttColor(rttMs: number): string {
  if (rttMs <= 0) return "var(--ink-muted)";
  if (rttMs < 30) return "var(--success)";
  if (rttMs < 60) return "var(--warning)";
  return "var(--error)";
}

function getPacketLossColor(lossPercent: number): string {
  if (lossPercent <= 0.15) return "var(--success)";
  if (lossPercent < 1) return "var(--warning)";
  return "var(--error)";
}

function getTimingColor(valueMs: number, goodMax: number, warningMax: number): string {
  if (valueMs <= 0) return "var(--ink-muted)";
  if (valueMs <= goodMax) return "var(--success)";
  if (valueMs <= warningMax) return "var(--warning)";
  return "var(--error)";
}

function getInputQueueColor(bufferedBytes: number, dropCount: number): string {
  if (dropCount > 0 || bufferedBytes >= 65536) return "var(--error)";
  if (bufferedBytes >= 32768) return "var(--warning)";
  return "var(--success)";
}

function getBitratePerformanceColor(percent: number): string {
  if (percent <= 0) return "var(--ink-muted)";
  if (percent >= 70 && percent <= 110) return "var(--success)";
  if (percent >= 45 && percent < 130) return "var(--warning)";
  return "var(--error)";
}

function getLagReasonLabel(reason: StreamLagReason): string {
  switch (reason) {
    case "network":
      return "Network";
    case "decoder":
      return "Decode";
    case "input_backpressure":
      return "Input";
    case "render":
      return "Render";
    case "stable":
      return "Stable";
    default:
      return "Unknown";
  }
}

function getLagReasonColor(reason: StreamLagReason): string {
  switch (reason) {
    case "network":
    case "decoder":
      return "var(--error)";
    case "input_backpressure":
    case "render":
      return "var(--warning)";
    case "stable":
      return "var(--success)";
    default:
      return "var(--ink-muted)";
  }
}

function formatElapsed(totalSeconds: number): string {
  const safe = Math.max(0, Math.floor(totalSeconds));
  const hours = Math.floor(safe / 3600);
  const minutes = Math.floor((safe % 3600) / 60);
  const seconds = safe % 60;
  if (hours > 0) {
    return `${hours}:${minutes.toString().padStart(2, "0")}:${seconds.toString().padStart(2, "0")}`;
  }
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function formatWarningSeconds(value: number | undefined): string | null {
  if (value === undefined || !Number.isFinite(value) || value < 0) {
    return null;
  }
  const total = Math.floor(value);
  const minutes = Math.floor(total / 60);
  const seconds = total % 60;
  if (minutes > 0) {
    return `${minutes}m ${seconds.toString().padStart(2, "0")}s`;
  }
  return `${seconds}s`;
}

type MicBadgeState = {
  connectedGamepads: number;
  micState: MicState;
  micEnabled: boolean;
};

function isMicBadgeStateEqual(prev: MicBadgeState, next: MicBadgeState): boolean {
  return (
    prev.connectedGamepads === next.connectedGamepads &&
    prev.micState === next.micState &&
    prev.micEnabled === next.micEnabled
  );
}

function StreamStatsHud({
  diagnosticsStore,
  serverRegion,
}: {
  diagnosticsStore: StreamDiagnosticsStore;
  serverRegion?: string;
}): JSX.Element {
  const stats = useStreamDiagnosticsStore(diagnosticsStore);
  const hasLiveBitrate = stats.bitrateKbps > 0;
  const bitrateKbps = hasLiveBitrate ? stats.bitrateKbps : stats.targetBitrateKbps;
  const bitrateMbps = bitrateKbps > 0 ? (bitrateKbps / 1000).toFixed(1) : "--";
  const bitrateLabel = hasLiveBitrate
    ? `${bitrateMbps} Mbps`
    : stats.targetBitrateKbps > 0
      ? `Target ${bitrateMbps} Mbps`
      : "-- Mbps";
  const bitratePerformancePercent = stats.targetBitrateKbps > 0 && stats.bitrateKbps > 0
    ? (stats.bitrateKbps / stats.targetBitrateKbps) * 100
    : 0;
  const bitratePerformanceText = bitratePerformancePercent > 0
    ? `${bitratePerformancePercent.toFixed(0)}%`
    : "--";
  const bitratePerformanceColor = getBitratePerformanceColor(bitratePerformancePercent);
  const hasResolution = stats.nativeRendererActive || stats.resolution !== "";
  const displayFps = Math.max(stats.decodeFps, stats.renderFps);
  const primaryText = hasResolution
    ? `${stats.resolution || "Native renderer"}${displayFps > 0 ? ` · ${displayFps}fps` : ""}`
    : "";
  const hasCodec = stats.codec && stats.codec !== "";
  const regionLabel = stats.serverRegion || serverRegion || "";
  const decodeColor = getTimingColor(stats.decodeTimeMs, 8, 16);
  const renderColor = getTimingColor(stats.renderTimeMs, 12, 22);
  const jitterBufferColor = getTimingColor(stats.jitterBufferDelayMs, 10, 24);
  const lossColor = getPacketLossColor(stats.packetLossPercent);
  const lossLabel = stats.nativeRendererActive ? "Drop" : "Loss";
  const lossTitle = stats.nativeRendererActive ? "Native renderer dropped frame percentage" : "Packet loss percentage";
  const dText = stats.decodeTimeMs > 0 ? `${stats.decodeTimeMs.toFixed(1)}ms` : "--";
  const rText = stats.renderTimeMs > 0 ? `${stats.renderTimeMs.toFixed(1)}ms` : "--";
  const jbText = stats.jitterBufferDelayMs > 0 ? `${stats.jitterBufferDelayMs.toFixed(1)}ms` : "--";
  const inputLive = stats.inputReady && stats.connectionState === "connected";
  const inputQueueColor = getInputQueueColor(stats.inputQueueBufferedBytes, stats.inputQueueDropCount);
  const inputQueueText = `${(stats.inputQueueBufferedBytes / 1024).toFixed(1)}KB`;
  const partiallyReliableQueueText = `${(stats.partiallyReliableInputQueueBufferedBytes / 1024).toFixed(1)}KB`;
  const mouseResidualText = `${stats.mouseResidualMagnitude.toFixed(2)}px`;

  return (
    <div className="sv-stats">
      <div className="sv-stats-head">
        {hasResolution ? (
          <span className="sv-stats-primary">{primaryText}</span>
        ) : (
          <span className="sv-stats-primary sv-stats-wait">Connecting...</span>
        )}
        <span className={`sv-stats-live ${inputLive ? "is-live" : "is-pending"}`}>
          {inputLive ? "Live" : "Sync"}
        </span>
      </div>

      <div className="sv-stats-sub">
        <span className="sv-stats-sub-left">
          {hasCodec ? stats.codec : "N/A"}
          {stats.isHdr && <span className="sv-stats-hdr">HDR</span>}
        </span>
        <span className="sv-stats-sub-right">{bitrateLabel}</span>
      </div>

      <div className="sv-stats-metrics">
        <span className="sv-stats-chip" title="Round-trip network latency">
          RTT <span className="sv-stats-chip-val" style={{ color: getRttColor(stats.rttMs) }}>{stats.rttMs > 0 ? `${stats.rttMs.toFixed(0)}ms` : "--"}</span>
        </span>
        <span className="sv-stats-chip" title="D = decode time">
          D <span className="sv-stats-chip-val" style={{ color: decodeColor }}>{dText}</span>
        </span>
        <span className="sv-stats-chip" title="R = render time">
          R <span className="sv-stats-chip-val" style={{ color: renderColor }}>{rText}</span>
        </span>
        <span className="sv-stats-chip" title="JB = jitter buffer delay">
          JB <span className="sv-stats-chip-val" style={{ color: jitterBufferColor }}>{jbText}</span>
        </span>
        <span className="sv-stats-chip" title={lossTitle}>
          {lossLabel} <span className="sv-stats-chip-val" style={{ color: lossColor }}>{stats.packetLossPercent.toFixed(2)}%</span>
        </span>
        <span className="sv-stats-chip" title="Actual receive bitrate compared with the negotiated target">
          Bit <span className="sv-stats-chip-val" style={{ color: bitratePerformanceColor }}>{bitratePerformanceText}</span>
        </span>
        <span className="sv-stats-chip" title="Input queue pressure (buffered bytes and delayed flush)">
          IQ <span className="sv-stats-chip-val" style={{ color: inputQueueColor }}>{inputQueueText}</span>
        </span>
        <span className="sv-stats-chip" title="Partially reliable input channel state and queued bytes">
          PR <span className="sv-stats-chip-val" style={{ color: stats.partiallyReliableInputOpen ? "var(--success)" : "var(--ink-muted)" }}>
            {stats.partiallyReliableInputOpen ? `${stats.mouseMoveTransport === "partially_reliable" ? "mouse" : "open"} · ${partiallyReliableQueueText}` : "off"}
          </span>
        </span>
        <span className="sv-stats-chip" title="Mouse flush cadence and packet rate">
          MF <span className="sv-stats-chip-val" style={{ color: stats.mouseAdaptiveFlushActive ? "var(--warning)" : "var(--success)" }}>
            {stats.mouseFlushIntervalMs.toFixed(0)}ms · {stats.mousePacketsPerSecond}/s
          </span>
        </span>
        {stats.lagReason !== "stable" && stats.lagReason !== "unknown" && (
          <span className="sv-stats-chip" title={stats.lagReasonDetail}>
            Lag <span className="sv-stats-chip-val" style={{ color: getLagReasonColor(stats.lagReason) }}>{getLagReasonLabel(stats.lagReason)}</span>
          </span>
        )}
      </div>

      <div className="sv-stats-foot">
        Input queue peak {(stats.inputQueuePeakBufferedBytes / 1024).toFixed(1)}KB · PR peak {(stats.partiallyReliableInputQueuePeakBufferedBytes / 1024).toFixed(1)}KB · drops {stats.inputQueueDropCount} · sched {stats.inputQueueMaxSchedulingDelayMs.toFixed(1)}ms · residual {mouseResidualText}
      </div>

      {(stats.hardwareAcceleration || stats.colorCodec) && (
        <div className="sv-stats-foot">
          {[stats.hardwareAcceleration, stats.colorCodec].filter(Boolean).join(" · ")}
        </div>
      )}

      {(stats.decoderPressureActive || stats.decoderRecoveryAttempts > 0) && (
        <div className="sv-stats-foot">
          Decoder recovery {stats.decoderPressureActive ? "active" : "idle"} · attempts {stats.decoderRecoveryAttempts} · action {stats.decoderRecoveryAction}
        </div>
      )}

      {(stats.nativeTransitionSummary || stats.nativeQueueMode || stats.nativeCapsFramerate) && (
        <div className="sv-stats-foot">
          Native transition {stats.nativeTransitionSummary ?? "none"} · queue {stats.nativeQueueMode ?? "unknown"} · caps {stats.nativeCapsFramerate ?? "unknown"}{typeof stats.nativeRequestedFps === "number" ? ` · requested ${stats.nativeRequestedFps}fps` : ""}{typeof stats.nativeFramesPendingToPresent === "number" ? ` · pending ${stats.nativeFramesPendingToPresent}` : ""}{typeof stats.nativePartialFlushCount === "number" || typeof stats.nativeCompleteFlushCount === "number" ? ` · flush ${stats.nativePartialFlushCount ?? 0}/${stats.nativeCompleteFlushCount ?? 0}` : ""}
        </div>
      )}

      {(stats.nativeRequestedStreamingFeaturesSummary || stats.nativeFinalizedStreamingFeaturesSummary) && (
        <div className="sv-stats-foot">
          Stream features requested {stats.nativeRequestedStreamingFeaturesSummary ?? "none"} · finalized {stats.nativeFinalizedStreamingFeaturesSummary ?? "none"}
        </div>
      )}

      {(stats.gpuType || regionLabel) && (
        <div className="sv-stats-foot">
          {[stats.gpuType, regionLabel].filter(Boolean).join(" · ")}
        </div>
      )}

      {stats.lagReason !== "stable" && stats.lagReason !== "unknown" && (
        <div className="sv-stats-foot">
          Lag source {getLagReasonLabel(stats.lagReason).toLowerCase()} · {stats.lagReasonDetail}
        </div>
      )}
    </div>
  );
}

function ControllerIndicator({
  diagnosticsStore,
  isConnecting,
}: {
  diagnosticsStore: StreamDiagnosticsStore;
  isConnecting: boolean;
}): JSX.Element | null {
  const connectedGamepads = useStreamDiagnosticsSelector(
    diagnosticsStore,
    (stats) => stats.connectedGamepads,
  );
  const [badgeVisible, setBadgeVisible] = useState(true);
  const hideTimerRef = useRef<number | null>(null);

  useEffect(() => {
    if (hideTimerRef.current !== null) {
      window.clearTimeout(hideTimerRef.current);
      hideTimerRef.current = null;
    }
    if (connectedGamepads > 0) {
      setBadgeVisible(true);
      hideTimerRef.current = window.setTimeout(() => {
        setBadgeVisible(false);
        hideTimerRef.current = null;
      }, 5000);
    } else {
      setBadgeVisible(true);
    }
    return () => {
      if (hideTimerRef.current !== null) {
        window.clearTimeout(hideTimerRef.current);
        hideTimerRef.current = null;
      }
    };
  }, [connectedGamepads]);

  if (isConnecting || connectedGamepads <= 0) {
    return null;
  }

  return (
    <div
      className={`sv-ctrl${badgeVisible ? "" : " sv-ctrl--hidden"}`}
      title={`${connectedGamepads} controller(s) connected`}
      aria-hidden={!badgeVisible}
    >
      <Gamepad2 size={18} />
      {connectedGamepads > 1 && <span className="sv-ctrl-n">{connectedGamepads}</span>}
    </div>
  );
}

function MicrophoneIndicator({
  diagnosticsStore,
  showAntiAfkIndicator,
  hideStreamButtons,
  isConnecting,
  onToggleMicrophone,
}: {
  diagnosticsStore: StreamDiagnosticsStore;
  showAntiAfkIndicator: boolean;
  hideStreamButtons: boolean;
  isConnecting: boolean;
  onToggleMicrophone?: () => void;
}): JSX.Element | null {
  const { connectedGamepads, micState, micEnabled } = useStreamDiagnosticsSelector(
    diagnosticsStore,
    (stats): MicBadgeState => ({
      connectedGamepads: stats.connectedGamepads,
      micState: stats.micState ?? "uninitialized",
      micEnabled: stats.micEnabled ?? false,
    }),
    isMicBadgeStateEqual,
  );
  const hasMicrophone = micState === "started" || micState === "stopped";
  const showMicIndicator = hasMicrophone && !isConnecting && !hideStreamButtons;

  if (!showMicIndicator || !onToggleMicrophone) {
    return null;
  }

  return (
    <button
      type="button"
      className={`sv-mic${connectedGamepads > 0 || showAntiAfkIndicator ? " sv-mic--stacked" : ""}`}
      onClick={onToggleMicrophone}
      data-enabled={micEnabled}
      title={micEnabled ? "Mute microphone" : "Unmute microphone"}
      aria-label={micEnabled ? "Mute microphone" : "Unmute microphone"}
      aria-pressed={micEnabled}
    >
      {micEnabled ? <Mic size={18} /> : <MicOff size={18} />}
    </button>
  );
}

function AntiAfkIndicator({
  diagnosticsStore,
  antiAfkEnabled,
  showAntiAfkIndicator,
  isConnecting,
}: {
  diagnosticsStore: StreamDiagnosticsStore;
  antiAfkEnabled: boolean;
  showAntiAfkIndicator: boolean;
  isConnecting: boolean;
}): JSX.Element | null {
  const hasController = useStreamDiagnosticsSelector(
    diagnosticsStore,
    (stats) => stats.connectedGamepads > 0,
  );

  if (!antiAfkEnabled || !showAntiAfkIndicator || isConnecting) {
    return null;
  }

  return (
    <div className={`sv-afk${hasController ? " sv-afk--stacked" : ""}`} title="Anti-AFK is enabled">
      <span className="sv-afk-dot" />
      <span className="sv-afk-label">ANTI-AFK ON</span>
    </div>
  );
}

function RecordingIndicator({
  diagnosticsStore,
  showAntiAfkIndicator,
  hideStreamButtons,
  isConnecting,
  isRecording,
  onToggleMicrophone,
  recordingDurationMs,
}: {
  diagnosticsStore: StreamDiagnosticsStore;
  showAntiAfkIndicator: boolean;
  hideStreamButtons: boolean;
  isConnecting: boolean;
  isRecording: boolean;
  onToggleMicrophone?: () => void;
  recordingDurationMs: number;
}): JSX.Element | null {
  const { connectedGamepads, micState } = useStreamDiagnosticsSelector(
    diagnosticsStore,
    (stats) => ({
      connectedGamepads: stats.connectedGamepads,
      micState: stats.micState ?? "uninitialized",
    }),
    (prev, next) => prev.connectedGamepads === next.connectedGamepads && prev.micState === next.micState,
  );
  const hasMicrophone = micState === "started" || micState === "stopped";
  const showMicIndicator = hasMicrophone && !isConnecting && !hideStreamButtons && Boolean(onToggleMicrophone);
  const stackedBadges = [connectedGamepads > 0, showAntiAfkIndicator, showMicIndicator].filter(Boolean).length;

  if (!isRecording || isConnecting) {
    return null;
  }

  return (
    <div
      className="sv-rec"
      style={{ top: 14 + 42 * stackedBadges }}
      title={`Recording · ${formatElapsed(Math.round(recordingDurationMs / 1000))}`}
    >
      <span className="sv-rec-dot" />
      <span className="sv-rec-label">REC {formatElapsed(Math.round(recordingDurationMs / 1000))}</span>
    </div>
  );
}

function StreamTitleBar({
  diagnosticsStore,
  gameTitle,
  platformName,
  PlatformIcon,
  showHints,
}: {
  diagnosticsStore: StreamDiagnosticsStore;
  gameTitle: string;
  platformName: string;
  PlatformIcon: (() => JSX.Element) | null;
  showHints: boolean;
}): JSX.Element | null {
  const hasResolution = useStreamDiagnosticsSelector(
    diagnosticsStore,
    (stats) => stats.nativeRendererActive || stats.resolution !== "",
  );

  if (!hasResolution || !showHints) {
    return null;
  }

  return (
    <div className="sv-title-bar">
      <span className="sv-title-game">{gameTitle}</span>
      {PlatformIcon && (
        <span className="sv-title-platform" title={platformName}>
          <span className="sv-title-platform-icon">
            <PlatformIcon />
          </span>
          <span>{platformName}</span>
        </span>
      )}
    </div>
  );
}

function StreamEmptyState({
  diagnosticsStore,
}: {
  diagnosticsStore: StreamDiagnosticsStore;
}): JSX.Element | null {
  const hasResolution = useStreamDiagnosticsSelector(
    diagnosticsStore,
    (stats) => stats.nativeRendererActive || stats.resolution !== "",
  );

  if (hasResolution) {
    return null;
  }

  return (
    <div className="sv-empty">
      <div className="sv-empty-grad" />
    </div>
  );
}

function SidebarMicMutedBadge({
  diagnosticsStore,
  micTrack,
}: {
  diagnosticsStore: StreamDiagnosticsStore;
  micTrack?: MediaStreamTrack | null;
}): JSX.Element | null {
  const micEnabled = useStreamDiagnosticsSelector(
    diagnosticsStore,
    (stats) => stats.micEnabled ?? false,
  );

  if (!micTrack || micEnabled) {
    return null;
  }

  return <span className="settings-value-badge">Muted</span>;
}

function VideoFocusOnReady({
  diagnosticsStore,
  isConnecting,
  videoRef,
}: {
  diagnosticsStore: StreamDiagnosticsStore;
  isConnecting: boolean;
  videoRef: React.RefObject<HTMLVideoElement | null>;
}): null {
  const shouldFocusVideo = useStreamDiagnosticsSelector(
    diagnosticsStore,
    (stats) => stats.resolution !== "" && !stats.nativeRendererActive,
  );

  useEffect(() => {
    if (!isConnecting && videoRef.current && shouldFocusVideo) {
      const timer = window.setTimeout(() => {
        if (videoRef.current && document.activeElement !== videoRef.current) {
          videoRef.current.focus();
          console.log("[StreamView] Focused video element");
        }
      }, 100);
      return () => clearTimeout(timer);
    }
  }, [isConnecting, shouldFocusVideo, videoRef]);

  return null;
}

export function StreamView({
  videoRef,
  audioRef,
  diagnosticsStore,
  showStats,
  shortcuts,
  serverRegion,
  antiAfkEnabled,
  antiAfkAckNonce,
  showAntiAfkIndicator,
  exitPrompt,
  sessionStartedAtMs,
  isStreaming,
  sessionCounterEnabled,
  sessionClockShowEveryMinutes,
  sessionClockShowDurationSeconds,
  streamWarning,
  isFullscreen,
  isConnecting,
  gameTitle,
  platformStore,
  onToggleFullscreen,
  onConfirmExit,
  onCancelExit,
  onEndSession,
  onToggleMicrophone,
  mouseSensitivity,
  onMouseSensitivityChange,
  mouseAcceleration,
  onMouseAccelerationChange,
  onRequestPointerLock,
  onReleasePointerLock,
  microphoneMode,
  onMicrophoneModeChange,
  onScreenshotShortcutChange,
  onRecordingShortcutChange,
  subscriptionInfo,
  micTrack,
  hideStreamButtons = false,
  allowEscapeToExitFullscreen,
  hideConnectingOverlay = false,
  className,
}: StreamViewProps): JSX.Element {
  const [showHints, setShowHints] = useState(true);
  const [showSessionClock, setShowSessionClock] = useState(false);
  const [antiAfkToggleAck, setAntiAfkToggleAck] = useState<"on" | "off" | null>(null);
  const [showSideBar, setShowSideBar] = useState(false);
  const [isPointerLocked, setIsPointerLocked] = useState(false);
  const [pointerLockHintVisible, setPointerLockHintVisible] = useState(false);
  const pointerLockHintTimerRef = useRef<number | null>(null);
  const [screenshots, setScreenshots] = useState<ScreenshotEntry[]>([]);
  const [isSavingScreenshot, setIsSavingScreenshot] = useState(false);
  const [galleryError, setGalleryError] = useState<string | null>(null);
  const [selectedScreenshotId, setSelectedScreenshotId] = useState<string | null>(null);
  const [screenshotShortcutInput, setScreenshotShortcutInput] = useState(shortcuts.screenshot);
  const [screenshotShortcutError, setScreenshotShortcutError] = useState<string | null>(null);
  const [activeSidebarTab, setActiveSidebarTab] = useState<"preferences" | "shortcuts">("preferences");
  const screenshotApiAvailable =
    typeof window.openNow?.saveScreenshot === "function" &&
    typeof window.openNow?.listScreenshots === "function" &&
    typeof window.openNow?.deleteScreenshot === "function" &&
    typeof window.openNow?.saveScreenshotAs === "function";
  const nativeRendererActive = useStreamDiagnosticsSelector(
    diagnosticsStore,
    (stats) => stats.nativeRendererActive,
  );
  const showStatsHud = showStats && !nativeRendererActive && !isConnecting;

  // Recording state
  const [isRecording, setIsRecording] = useState(false);
  const [recordings, setRecordings] = useState<RecordingEntry[]>([]);
  const [recordingDurationMs, setRecordingDurationMs] = useState(0);
  const [recordingError, setRecordingError] = useState<string | null>(null);
  const [usedMimeType, setUsedMimeType] = useState<string | null>(null);
  const [recordingShortcutInput, setRecordingShortcutInput] = useState(shortcuts.recording);
  const [recordingShortcutError, setRecordingShortcutError] = useState<string | null>(null);
  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const recordingIdRef = useRef<string | null>(null);
  const recordingStartTimeRef = useRef<number>(0);
  const recordingTimerRef = useRef<number | undefined>(undefined);
  const thumbnailDataUrlRef = useRef<string | null>(null);
  const recCarouselRef = useRef<HTMLDivElement | null>(null);
  const recordingApiAvailable =
    typeof window.openNow?.beginRecording === "function" &&
    typeof window.openNow?.sendRecordingChunk === "function" &&
    typeof window.openNow?.finishRecording === "function" &&
    typeof window.openNow?.abortRecording === "function" &&
    typeof window.openNow?.listRecordings === "function" &&
    typeof window.openNow?.deleteRecording === "function";

  const microphoneModes = useMemo(
    () => [
      { value: "disabled" as MicrophoneMode, label: "Disabled", description: "No microphone input" },
      { value: "push-to-talk" as MicrophoneMode, label: "Push-to-Talk", description: "Hold a key to talk" },
      { value: "voice-activity" as MicrophoneMode, label: "Voice Activity", description: "Always listen" },
    ],
    []
  );

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

  const warningSeconds = formatWarningSeconds(streamWarning?.secondsLeft);
  const platformName = platformStore ? getStoreDisplayName(platformStore) : "";
  const PlatformIcon = platformStore ? getStoreIconComponent(platformStore) : null;
  const isMacClient = navigator.platform?.toLowerCase().includes("mac") || navigator.userAgent.includes("Macintosh");

  // Local ref for video element to manage focus
  const localVideoRef = useRef<HTMLVideoElement | null>(null);
  // Local ref for audio element (game audio stream)
  const localAudioRef = useRef<HTMLAudioElement | null>(null);
  // AudioContext used during an active recording (torn down on stop/error)
  const audioCtxRef = useRef<AudioContext | null>(null);

  // Mic level meter canvas
  const micMeterRef = useRef<HTMLCanvasElement | null>(null);
  const galleryStripRef = useRef<HTMLDivElement | null>(null);
  useMicMeter(micMeterRef, micTrack ?? null, showSideBar && microphoneMode !== "disabled");

  const selectedScreenshot = useMemo(() => {
    if (!selectedScreenshotId) return null;
    return screenshots.find((item) => item.id === selectedScreenshotId) ?? null;
  }, [screenshots, selectedScreenshotId]);

  useEffect(() => {
    setScreenshotShortcutInput(shortcuts.screenshot);
    setScreenshotShortcutError(null);
  }, [shortcuts.screenshot]);

  useEffect(() => {
    setRecordingShortcutInput(shortcuts.recording);
    setRecordingShortcutError(null);
  }, [shortcuts.recording]);

  const getScreenshotShortcutError = useCallback((rawValue: string): string | null => {
    const trimmed = rawValue.trim();
    if (!trimmed) {
      return "Shortcut cannot be empty.";
    }

    const normalized = normalizeShortcut(trimmed);
    if (!normalized.valid) {
      return "Invalid shortcut format.";
    }

    const reserved = [
      shortcuts.toggleStats,
      shortcuts.togglePointerLock,
      shortcuts.stopStream,
      shortcuts.toggleAntiAfk,
      shortcuts.toggleMicrophone,
      shortcuts.recording,
      isMacClient ? "Meta+G" : "Ctrl+Shift+G",
    ]
      .filter((value): value is string => typeof value === "string" && value.trim().length > 0)
      .map((value) => normalizeShortcut(value))
      .filter((parsed) => parsed.valid)
      .map((parsed) => parsed.canonical);

    if (reserved.includes(normalized.canonical)) {
      return "Shortcut conflicts with an existing binding.";
    }

    return null;
  }, [isMacClient, shortcuts.recording, shortcuts.stopStream, shortcuts.toggleAntiAfk, shortcuts.toggleMicrophone, shortcuts.togglePointerLock, shortcuts.toggleStats]);

  const getRecordingShortcutError = useCallback((rawValue: string): string | null => {
    const trimmed = rawValue.trim();
    if (!trimmed) {
      return "Shortcut cannot be empty.";
    }

    const normalized = normalizeShortcut(trimmed);
    if (!normalized.valid) {
      return "Invalid shortcut format.";
    }

    const reserved = [
      shortcuts.toggleStats,
      shortcuts.togglePointerLock,
      shortcuts.stopStream,
      shortcuts.toggleAntiAfk,
      shortcuts.toggleMicrophone,
      shortcuts.screenshot,
      isMacClient ? "Meta+G" : "Ctrl+Shift+G",
    ]
      .filter((value): value is string => typeof value === "string" && value.trim().length > 0)
      .map((value) => normalizeShortcut(value))
      .filter((parsed) => parsed.valid)
      .map((parsed) => parsed.canonical);

    if (reserved.includes(normalized.canonical)) {
      return "Shortcut conflicts with an existing binding.";
    }

    return null;
  }, [isMacClient, shortcuts.screenshot, shortcuts.stopStream, shortcuts.toggleAntiAfk, shortcuts.toggleMicrophone, shortcuts.togglePointerLock, shortcuts.toggleStats]);

  const SIDEBAR_TOGGLE_RAW = isMacClient ? "Meta+G" : "Ctrl+Shift+G";
  const sidebarToggleShortcutDisplay = formatShortcutForDisplay(SIDEBAR_TOGGLE_RAW, isMacClient);

  const applyScreenshotShortcutFromCapture = useCallback(
    (canonical: string) => {
      const error = getScreenshotShortcutError(canonical);
      if (error) {
        setScreenshotShortcutError(error);
        return;
      }
      const normalized = normalizeShortcut(canonical.trim());
      if (!normalized.valid) {
        setScreenshotShortcutError("Invalid shortcut format.");
        return;
      }
      setScreenshotShortcutError(null);
      setScreenshotShortcutInput(normalized.canonical);
      if (normalized.canonical !== shortcuts.screenshot) {
        onScreenshotShortcutChange(normalized.canonical);
      }
    },
    [getScreenshotShortcutError, onScreenshotShortcutChange, shortcuts.screenshot],
  );

  const applyRecordingShortcutFromCapture = useCallback(
    (canonical: string) => {
      const error = getRecordingShortcutError(canonical);
      if (error) {
        setRecordingShortcutError(error);
        return;
      }
      const normalized = normalizeShortcut(canonical.trim());
      if (!normalized.valid) {
        setRecordingShortcutError("Invalid shortcut format.");
        return;
      }
      setRecordingShortcutError(null);
      setRecordingShortcutInput(normalized.canonical);
      if (normalized.canonical !== shortcuts.recording) {
        onRecordingShortcutChange(normalized.canonical);
      }
    },
    [getRecordingShortcutError, onRecordingShortcutChange, shortcuts.recording],
  );

  const handleStreamScreenshotShortcutKeyDown = (e: React.KeyboardEvent<HTMLInputElement>): void => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.currentTarget.blur();
      return;
    }
    if (e.key === "Enter" || e.key === "Tab") {
      return;
    }
    const captured = shortcutFromKeyboardEvent(e.nativeEvent);
    if (!captured) {
      return;
    }
    e.preventDefault();
    e.stopPropagation();
    applyScreenshotShortcutFromCapture(captured);
  };

  const handleStreamRecordingShortcutKeyDown = (e: React.KeyboardEvent<HTMLInputElement>): void => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.currentTarget.blur();
      return;
    }
    if (e.key === "Enter" || e.key === "Tab") {
      return;
    }
    const captured = shortcutFromKeyboardEvent(e.nativeEvent);
    if (!captured) {
      return;
    }
    e.preventDefault();
    e.stopPropagation();
    applyRecordingShortcutFromCapture(captured);
  };

  const handleStreamScreenshotShortcutPaste = (e: React.ClipboardEvent<HTMLInputElement>): void => {
    const text = e.clipboardData.getData("text/plain").trim();
    if (!text) {
      return;
    }
    e.preventDefault();
    applyScreenshotShortcutFromCapture(text);
  };

  const handleStreamRecordingShortcutPaste = (e: React.ClipboardEvent<HTMLInputElement>): void => {
    const text = e.clipboardData.getData("text/plain").trim();
    if (!text) {
      return;
    }
    e.preventDefault();
    applyRecordingShortcutFromCapture(text);
  };

  const refreshScreenshots = useCallback(async () => {
    setGalleryError(null);
    if (!screenshotApiAvailable) {
      setGalleryError("Screenshot API unavailable. Restart OpenNOW to enable gallery.");
      return;
    }
    try {
      const items = await window.openNow.listScreenshots();
      setScreenshots(items);
    } catch (error) {
      console.error("[StreamView] Failed to load screenshots:", error);
      setGalleryError("Unable to load screenshot gallery.");
    }
  }, [screenshotApiAvailable]);

  const captureScreenshot = useCallback(async () => {
    setGalleryError(null);
    if (!screenshotApiAvailable) {
      setGalleryError("Screenshot API unavailable. Restart OpenNOW to enable capture.");
      return;
    }
    if (isSavingScreenshot) {
      return;
    }

    const video = localVideoRef.current;
    if (!video || video.videoWidth <= 0 || video.videoHeight <= 0) {
      setGalleryError("Stream is not ready for screenshots yet.");
      return;
    }

    setIsSavingScreenshot(true);
    try {
      const canvas = document.createElement("canvas");
      canvas.width = video.videoWidth;
      canvas.height = video.videoHeight;
      const context = canvas.getContext("2d");
      if (!context) {
        throw new Error("Could not acquire 2D context");
      }

      context.drawImage(video, 0, 0, canvas.width, canvas.height);
      const dataUrl = canvas.toDataURL("image/png");
      const saved = await window.openNow.saveScreenshot({ dataUrl, gameTitle });
      setScreenshots((prev) => [saved, ...prev.filter((item) => item.id !== saved.id)].slice(0, 60));
    } catch (error) {
      console.error("[StreamView] Failed to capture screenshot:", error);
      setGalleryError("Screenshot failed. Try again.");
    } finally {
      setIsSavingScreenshot(false);
    }
  }, [gameTitle, isSavingScreenshot, screenshotApiAvailable]);

  const scrollGallery = useCallback((direction: "left" | "right") => {
    const strip = galleryStripRef.current;
    if (!strip) return;
    const delta = Math.max(180, Math.round(strip.clientWidth * 0.7));
    strip.scrollBy({ left: direction === "left" ? -delta : delta, behavior: "smooth" });
  }, []);

  const handleDeleteScreenshot = useCallback(async () => {
    setGalleryError(null);
    if (!screenshotApiAvailable) {
      setGalleryError("Screenshot API unavailable. Restart OpenNOW to enable gallery.");
      return;
    }
    if (!selectedScreenshot) return;

    try {
      await window.openNow.deleteScreenshot({ id: selectedScreenshot.id });
      setScreenshots((prev) => prev.filter((item) => item.id !== selectedScreenshot.id));
      setSelectedScreenshotId(null);
    } catch (error) {
      console.error("[StreamView] Failed to delete screenshot:", error);
      setGalleryError("Unable to delete screenshot.");
    }
  }, [screenshotApiAvailable, selectedScreenshot]);

  const handleSaveScreenshotAs = useCallback(async () => {
    setGalleryError(null);
    if (!screenshotApiAvailable) {
      setGalleryError("Screenshot API unavailable. Restart OpenNOW to enable gallery.");
      return;
    }
    if (!selectedScreenshot) return;

    try {
      await window.openNow.saveScreenshotAs({ id: selectedScreenshot.id });
    } catch (error) {
      console.error("[StreamView] Failed to save screenshot as:", error);
      setGalleryError("Unable to save screenshot.");
    }
  }, [screenshotApiAvailable, selectedScreenshot]);

  const refreshRecordings = useCallback(async () => {
    setRecordingError(null);
    if (!recordingApiAvailable) return;
    try {
      const items = await window.openNow.listRecordings();
      setRecordings(items);
    } catch (error) {
      console.error("[StreamView] Failed to load recordings:", error);
      setRecordingError("Unable to load recordings.");
    }
  }, [recordingApiAvailable]);

  const handleDeleteRecording = useCallback(async (id: string) => {
    setRecordingError(null);
    if (!recordingApiAvailable) return;
    try {
      await window.openNow.deleteRecording({ id });
      setRecordings((prev) => prev.filter((r) => r.id !== id));
    } catch (error) {
      console.error("[StreamView] Failed to delete recording:", error);
      setRecordingError("Unable to delete recording.");
    }
  }, [recordingApiAvailable]);

  const scrollRecCarousel = useCallback((direction: "left" | "right") => {
    const strip = recCarouselRef.current;
    if (!strip) return;
    strip.scrollBy({ left: direction === "left" ? -200 : 200, behavior: "smooth" });
  }, []);

  const toggleRecording = useCallback(async () => {
    setRecordingError(null);

    if (isRecording) {
      mediaRecorderRef.current?.stop();
      return;
    }

    if (!recordingApiAvailable) {
      setRecordingError("Recording API unavailable. Restart OpenNOW to enable recording.");
      return;
    }

    const video = localVideoRef.current;
    if (!video || !video.srcObject) {
      setRecordingError("Stream is not ready for recording yet.");
      return;
    }

    const stream = video.srcObject as MediaStream;
    const mimeTypes = [
      "video/mp4;codecs=avc1.42E01E,mp4a.40.2",
      "video/mp4;codecs=avc1",
      "video/mp4",
      "video/webm;codecs=h264",
      "video/webm;codecs=vp8",
      "video/webm",
    ];
    const mimeType = mimeTypes.find((m) => MediaRecorder.isTypeSupported(m)) ?? "video/webm";
    setUsedMimeType(mimeType);

    // Build a composed MediaStream: video tracks + mixed audio (game + mic)
    const audioCtx = new AudioContext();
    audioCtxRef.current = audioCtx;
    const audioDest = audioCtx.createMediaStreamDestination();

    // Wire game audio (from the <audio> element's srcObject)
    const audioEl = localAudioRef.current;
    const gameAudioStream = audioEl?.srcObject instanceof MediaStream ? audioEl.srcObject : null;
    if (gameAudioStream && gameAudioStream.getAudioTracks().length > 0) {
      audioCtx.createMediaStreamSource(gameAudioStream).connect(audioDest);
    }

    // Wire microphone (if active)
    if (micTrack && micTrack.readyState === "live") {
      const micStream = new MediaStream([micTrack]);
      audioCtx.createMediaStreamSource(micStream).connect(audioDest);
    }

    // Compose: video tracks from the video element + mixed audio destination track
    const composed = new MediaStream([
      ...stream.getVideoTracks(),
      ...audioDest.stream.getAudioTracks(),
    ]);

    let recordingId: string;
    try {
      const result = await window.openNow.beginRecording({ mimeType });
      recordingId = result.recordingId;
    } catch (error) {
      console.error("[StreamView] Failed to begin recording:", error);
      audioCtx.close().catch(() => undefined);
      audioCtxRef.current = null;
      setRecordingError("Could not start recording.");
      return;
    }

    recordingIdRef.current = recordingId;
    thumbnailDataUrlRef.current = null;
    recordingStartTimeRef.current = Date.now();
    setRecordingDurationMs(0);
    setIsRecording(true);

    recordingTimerRef.current = window.setInterval(() => {
      setRecordingDurationMs(Date.now() - recordingStartTimeRef.current);
    }, 500);

    let isFirstChunk = true;
    const recorder = new MediaRecorder(composed, { mimeType });

    recorder.ondataavailable = (e: BlobEvent) => {
      if (!e.data || e.data.size === 0) return;

      // Capture thumbnail from the first chunk (frame ~2 s in)
      if (isFirstChunk) {
        isFirstChunk = false;
        const vid = localVideoRef.current;
        if (vid && vid.videoWidth > 0 && vid.videoHeight > 0) {
          // create a canvas sized to preserve the video's aspect ratio, but
          // capped at roughly 320×180 so we don't generate unnecessarily large
          // thumbnails when the stream resolution is higher than 16:9.
          const maxW = 320;
          const maxH = 180;
          let w = vid.videoWidth;
          let h = vid.videoHeight;

          // shrink to fit within bounds while keeping aspect ratio
          if (w > maxW) {
            h = Math.round((maxW / w) * h);
            w = maxW;
          }
          if (h > maxH) {
            w = Math.round((maxH / h) * w);
            h = maxH;
          }

          const canvas = document.createElement("canvas");
          canvas.width = w;
          canvas.height = h;
          const ctx2d = canvas.getContext("2d");
          if (ctx2d) {
            ctx2d.drawImage(vid, 0, 0, w, h);
            thumbnailDataUrlRef.current = canvas.toDataURL("image/jpeg", 0.72);
          }
        }
      }

      void e.data.arrayBuffer().then((buf) => {
        const id = recordingIdRef.current;
        if (!id) return;
        window.openNow.sendRecordingChunk({ recordingId: id, chunk: buf }).catch((err: unknown) => {
          console.error("[StreamView] Failed to send recording chunk:", err);
        });
      });
    };

    recorder.onstop = () => {
      window.clearInterval(recordingTimerRef.current);
      recordingTimerRef.current = undefined;
      audioCtxRef.current?.close().catch(() => undefined);
      audioCtxRef.current = null;
      const id = recordingIdRef.current;
      recordingIdRef.current = null;
      setIsRecording(false);

      if (!id) return;

      const durationMs = Date.now() - recordingStartTimeRef.current;
      void window.openNow
        .finishRecording({
          recordingId: id,
          durationMs,
          gameTitle,
          thumbnailDataUrl: thumbnailDataUrlRef.current ?? undefined,
        })
        .then((entry) => {
          setRecordings((prev) => [entry, ...prev].slice(0, 20));
          thumbnailDataUrlRef.current = null;
        })
        .catch((err: unknown) => {
          console.error("[StreamView] Failed to finish recording:", err);
          setRecordingError("Recording could not be saved.");
        });
    };

    recorder.onerror = () => {
      window.clearInterval(recordingTimerRef.current);
      recordingTimerRef.current = undefined;
      audioCtxRef.current?.close().catch(() => undefined);
      audioCtxRef.current = null;
      const id = recordingIdRef.current;
      recordingIdRef.current = null;
      setIsRecording(false);
      thumbnailDataUrlRef.current = null;
      if (id) {
        window.openNow.abortRecording({ recordingId: id }).catch(() => undefined);
      }
      setRecordingError("Recording encountered an error.");
    };

    mediaRecorderRef.current = recorder;
    recorder.start(2000);
  }, [gameTitle, isRecording, micTrack, recordingApiAvailable]);

  // Cleanup: abort any active recording on unmount
  useEffect(() => {
    return () => {
      window.clearInterval(recordingTimerRef.current);
      const recorder = mediaRecorderRef.current;
      const id = recordingIdRef.current;
      if (recorder && recorder.state !== "inactive") {
        recorder.stop();
      }
      if (id) {
        window.openNow.abortRecording({ recordingId: id }).catch(() => undefined);
        recordingIdRef.current = null;
      }
      audioCtxRef.current?.close().catch(() => undefined);
      audioCtxRef.current = null;
    };
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
      const visible = width >= 2 && height >= 2;
      updateSurface({
        deviceScaleFactor: dpr,
        visible,
        showStats,
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
  }, [showStats]);

  useEffect(() => {
    const handlePointerLockChange = () => {
      setIsPointerLocked(document.pointerLockElement === localVideoRef.current);
    };
    document.addEventListener("pointerlockchange", handlePointerLockChange);
    return () => document.removeEventListener("pointerlockchange", handlePointerLockChange);
  }, []);

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
      void refreshScreenshots();
      void refreshRecordings();
      return;
    }
    // Sidebar just closed — restore focus to the video so clicks register
    // immediately. Without this, focus stays on the last sidebar element and
    // mousedown's preventDefault() blocks the browser from re-focusing on click.
    const timer = window.setTimeout(() => {
      if (localVideoRef.current && document.activeElement !== localVideoRef.current) {
        localVideoRef.current.focus();
      }
    }, 50);
    try {
      delete (document.body.dataset as DOMStringMap).sidebarOpen;
    } catch {}
    return () => clearTimeout(timer);
  }, [refreshRecordings, refreshScreenshots, showSideBar]);

  useEffect(() => {
    if (!selectedScreenshotId) return;
    if (!screenshots.some((item) => item.id === selectedScreenshotId)) {
      setSelectedScreenshotId(null);
    }
  }, [screenshots, selectedScreenshotId]);

  const handleToggleSideBar = useCallback(() => {
    setShowSideBar((s) => {
      if (!s && document.pointerLockElement) {
        if (onReleasePointerLock) {
          onReleasePointerLock();
        } else {
          document.exitPointerLock();
        }
      }
      return !s;
    });
  }, [onReleasePointerLock]);

  useEffect(() => {
    const screenshotShortcut = normalizeShortcut(shortcuts.screenshot);
    const recordingShortcut = normalizeShortcut(shortcuts.recording);
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const isTyping = !!target && (
        target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.isContentEditable
      );
      if (isTyping) {
        return;
      }

      if (isShortcutMatch(event, screenshotShortcut)) {
        event.preventDefault();
        event.stopPropagation();
        void captureScreenshot();
        return;
      }

      if (isShortcutMatch(event, recordingShortcut)) {
        event.preventDefault();
        event.stopPropagation();
        void toggleRecording();
        return;
      }

      const key = event.key.toLowerCase();
      if (isMacClient) {
        if (event.metaKey && !event.ctrlKey && !event.shiftKey && key === "g") {
          event.preventDefault();
          handleToggleSideBar();
        }
      } else if (event.ctrlKey && event.shiftKey && !event.metaKey && key === "g") {
        event.preventDefault();
        handleToggleSideBar();
      }
    };

    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [captureScreenshot, handleToggleSideBar, isMacClient, shortcuts.screenshot, shortcuts.recording, toggleRecording]);

  return (
    <div className={["sv", className].filter(Boolean).join(" ")}>
      <video
        ref={setVideoRef}
        autoPlay
        playsInline
        muted
        tabIndex={0}
        className="sv-video"
        onClick={() => {
          if (localVideoRef.current && document.activeElement !== localVideoRef.current) {
            localVideoRef.current.focus();
          }
        }}
      />
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
              : "Escape is forwarded to the game while pointer-locked (see Settings)."}
          </div>
        </div>
      )}

      {showSideBar && (
        <>
          <div
            className="sv-sidebar-backdrop"
            onMouseDown={(event) => event.stopPropagation()}
            onClick={() => setShowSideBar(false)}
          />
          <SideBar title="Settings" className="sv-sidebar" onClose={() => setShowSideBar(false)}>
            <div className="sidebar-stat-line" title="Total remaining playtime from subscription">
              <span className="sidebar-stat-label">Remaining Playtime</span>
              <RemainingPlaytimeIndicator subscriptionInfo={subscriptionInfo} startedAtMs={sessionStartedAtMs} active={isStreaming} className="settings-value-badge" />
            </div>
            <div className="sidebar-tabs" role="tablist" aria-label="Sidebar sections">
              <button
                type="button"
                role="tab"
                aria-selected={activeSidebarTab === "preferences"}
                className={`sidebar-tab${activeSidebarTab === "preferences" ? " sidebar-tab--active" : ""}`}
                onClick={() => setActiveSidebarTab("preferences")}
              >
                Preferences
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={activeSidebarTab === "shortcuts"}
                className={`sidebar-tab${activeSidebarTab === "shortcuts" ? " sidebar-tab--active" : ""}`}
                onClick={() => setActiveSidebarTab("shortcuts")}
              >
                Shortcuts
              </button>
            </div>

            {activeSidebarTab === "preferences" && (
              <>
                <div className="sidebar-separator" aria-hidden="true" />
                <section className="sidebar-section">
                  <div className="sidebar-section-header">
                    <span>Mouse Preferences</span>
                    <span className="sidebar-section-sub">Fine-tune cursor movement</span>
                  </div>
                  <div className="sidebar-row sidebar-row--column">
                    <div className="sidebar-row-top">
                      <span className="sidebar-label">Mouse Sensitivity</span>
                      <span className="settings-value-badge">{mouseSensitivity.toFixed(2)}x</span>
                    </div>
                    <input
                      type="range"
                      className="settings-slider"
                      min={0.1}
                      max={4}
                      step={0.01}
                      value={mouseSensitivity}
                      onChange={(event) => {
                        const next = Number(event.target.value);
                        if (Number.isFinite(next)) {
                          onMouseSensitivityChange(Math.max(0.1, Math.min(4, next)));
                        }
                      }}
                    />
                    <span className="sidebar-hint">Multiplier applied to mouse movement (1.00 = default).</span>
                  </div>
                  <div className="sidebar-row sidebar-row--column">
                    <div className="sidebar-row-top">
                      <span className="sidebar-label">Mouse Accelerator</span>
                      <span className="settings-value-badge">{Math.round(mouseAcceleration)}%</span>
                    </div>
                    <input
                      type="range"
                      className="settings-slider"
                      min={1}
                      max={150}
                      step={1}
                      value={Math.round(mouseAcceleration)}
                      onChange={(event) => {
                        const next = Number(event.target.value);
                        if (Number.isFinite(next)) {
                          onMouseAccelerationChange(Math.max(1, Math.min(150, Math.round(next))));
                        }
                      }}
                    />
                    <span className="sidebar-hint">Dynamic turn boost strength (1% = off-like, 150% = strongest).</span>
                  </div>
                </section>
                <div className="sidebar-separator" aria-hidden="true" />
                <section className="sidebar-section">
                  <div className="sidebar-section-header">
                    <span>Audio</span>
                    <span className="sidebar-section-sub">Microphone handling</span>
                  </div>
                  <div className="sidebar-row sidebar-row--column">
                    <div className="sidebar-row-top">
                      <span className="sidebar-label">Microphone Mode</span>
                      <span className="settings-value-badge">
                        {microphoneModes.find((option) => option.value === microphoneMode)?.label ?? microphoneMode}
                      </span>
                    </div>
                    <div className="sidebar-chip-row">
                      {microphoneModes.map((option) => (
                        <button
                          key={option.value}
                          type="button"
                          className={`sidebar-chip${microphoneMode === option.value ? " sidebar-chip--active" : ""}`}
                          onClick={() => onMicrophoneModeChange(option.value)}
                        >
                          <span>{option.label}</span>
                        </button>
                      ))}
                    </div>
                    <span className="sidebar-hint">
                      {microphoneModes.find((option) => option.value === microphoneMode)?.description ?? ""}
                    </span>
                  </div>
                  {microphoneMode !== "disabled" && (
                    <div className="sidebar-row sidebar-row--column">
                      <div className="sidebar-row-top">
                        <span className="sidebar-label">Send level</span>
                        <SidebarMicMutedBadge diagnosticsStore={diagnosticsStore} micTrack={micTrack} />
                      </div>
                      <canvas
                        ref={micMeterRef}
                        className="mic-meter-canvas"
                        aria-label="Microphone send level (what others hear)"
                      />
                      {!micTrack && <span className="sidebar-hint">Mic not active — check mode and permissions.</span>}
                    </div>
                  )}
                </section>
                <div className="sidebar-separator" aria-hidden="true" />
                <section className="sidebar-section">
                  <div className="sidebar-section-header">
                    <span>Gallery</span>
                    <span className="sidebar-section-sub">ScreensShot key: {shortcuts.screenshot}</span>
                  </div>
                  <div className="sidebar-row sidebar-row--aligned">
                    <span className="sidebar-label">ScreensShot</span>
                    <button
                      type="button"
                      className="sidebar-button sidebar-screenshot-button"
                      onClick={() => {
                        void captureScreenshot();
                      }}
                      disabled={isSavingScreenshot || !screenshotApiAvailable}
                    >
                      <Camera size={14} />
                      <span>{isSavingScreenshot ? "Capturing..." : "Capture"}</span>
                    </button>
                  </div>
                  <div className="sidebar-gallery-row">
                    <button
                      type="button"
                      className="sidebar-gallery-arrow"
                      onClick={() => scrollGallery("left")}
                      aria-label="Scroll gallery left"
                    >
                      <ChevronLeft size={16} />
                    </button>
                    <div className="sidebar-gallery-strip" ref={galleryStripRef}>
                      {screenshots.map((shot) => (
                        <button
                          key={shot.id}
                          type="button"
                          className="sidebar-gallery-item"
                          onClick={() => setSelectedScreenshotId(shot.id)}
                          title={new Date(shot.createdAtMs).toLocaleString()}
                        >
                          <img src={shot.dataUrl} alt={`Screenshot ${shot.fileName}`} />
                        </button>
                      ))}
                    </div>
                    <button
                      type="button"
                      className="sidebar-gallery-arrow"
                      onClick={() => scrollGallery("right")}
                      aria-label="Scroll gallery right"
                    >
                      <ChevronRight size={16} />
                    </button>
                  </div>
                  {screenshots.length === 0 && (
                    <span className="sidebar-hint">No screenshots yet. Press {shortcuts.screenshot} to capture one.</span>
                  )}
                  {galleryError && <span className="sidebar-hint sidebar-hint--error">{galleryError}</span>}
                </section>
                <div className="sidebar-separator" aria-hidden="true" />
                <section className="sidebar-section">
                  <div className="sidebar-section-header">
                    <span>Recordings</span>
                    <span className="sidebar-section-sub">Record key: {shortcuts.recording}</span>
                  </div>
                  {usedMimeType && (
                    <span className="sidebar-hint sidebar-hint--codec">Codec: {usedMimeType}</span>
                  )}
                  <div className="sidebar-row sidebar-row--aligned">
                    <span className="sidebar-label">
                      {isRecording ? `Recording ${formatElapsed(Math.round(recordingDurationMs / 1000))}` : "Record"}
                    </span>
                    <button
                      type="button"
                      className="sidebar-button sidebar-screenshot-button"
                      onClick={() => { void toggleRecording(); }}
                      disabled={!recordingApiAvailable}
                    >
                      {isRecording ? <Square size={14} /> : <Circle size={14} />}
                      <span>{isRecording ? "Stop" : "Start"}</span>
                    </button>
                  </div>
                  {recordingError && (
                    <span className="sidebar-hint sidebar-hint--error">{recordingError}</span>
                  )}
                  {recordings.length === 0 ? (
                    <span className="sidebar-hint">No recordings yet. Press {shortcuts.recording} to record.</span>
                  ) : (
                    <div className="sidebar-gallery-row">
                      <button
                        type="button"
                        className="sidebar-gallery-arrow"
                        onClick={() => scrollRecCarousel("left")}
                        aria-label="Scroll recordings left"
                      >
                        <ChevronLeft size={16} />
                      </button>
                      <div className="sidebar-rec-strip" ref={recCarouselRef}>
                        {recordings.map((rec) => (
                          <div key={rec.id} className="sidebar-rec-card">
                            {rec.thumbnailDataUrl ? (
                              <img
                                className="sidebar-rec-card-thumb"
                                src={rec.thumbnailDataUrl}
                                alt=""
                              />
                            ) : (
                              <div className="sidebar-rec-card-thumb sidebar-rec-card-thumb--placeholder">
                                <Video size={20} />
                              </div>
                            )}
                            <div className="sidebar-rec-card-meta">
                              <span className="sidebar-rec-card-title">{rec.gameTitle ?? "Untitled"}</span>
                              <span className="sidebar-rec-card-detail">
                                {formatElapsed(Math.round(rec.durationMs / 1000))} · {formatFileSize(rec.sizeBytes)}
                              </span>
                            </div>
                            <div className="sidebar-rec-card-actions">
                              <button
                                type="button"
                                className="sidebar-rec-card-action"
                                aria-label="Show in folder"
                                title="Show in folder"
                                onClick={() => { void window.openNow.showRecordingInFolder(rec.id); }}
                                disabled={typeof window.openNow?.showRecordingInFolder !== "function"}
                              >
                                <FolderOpen size={11} />
                              </button>
                              <button
                                type="button"
                                className="sidebar-rec-card-action sidebar-rec-card-action--danger"
                                aria-label="Delete recording"
                                title="Delete"
                                onClick={() => { void handleDeleteRecording(rec.id); }}
                              >
                                <Trash2 size={11} />
                              </button>
                            </div>
                          </div>
                        ))}
                      </div>
                      <button
                        type="button"
                        className="sidebar-gallery-arrow"
                        onClick={() => scrollRecCarousel("right")}
                        aria-label="Scroll recordings right"
                      >
                        <ChevronRight size={16} />
                      </button>
                    </div>
                  )}
                </section>
              </>
            )}

            {activeSidebarTab === "shortcuts" && (
              <>
                <div className="sidebar-separator" aria-hidden="true" />
                <section className="sidebar-section">
                  <div className="sidebar-section-header">
                    <span>Shortcut Bindings</span>
                    <span className="sidebar-section-sub">Edit screenshot keybind here</span>
                  </div>
                  <div className="sidebar-row sidebar-row--column">
                    <div className="sidebar-row-top">
                      <span className="sidebar-label">Screenshot Shortcut</span>
                    </div>
                    <input
                      type="text"
                      className={`settings-text-input settings-shortcut-input sidebar-shortcut-input ${screenshotShortcutError ? "error" : ""}`}
                      value={screenshotShortcutInput}
                      readOnly
                      onFocus={(event) => event.target.select()}
                      onPaste={handleStreamScreenshotShortcutPaste}
                      onBlur={() => {
                        const error = getScreenshotShortcutError(screenshotShortcutInput);
                        if (error) {
                          setScreenshotShortcutError(error);
                          return;
                        }
                        const normalized = normalizeShortcut(screenshotShortcutInput.trim());
                        if (!normalized.valid) {
                          setScreenshotShortcutError("Invalid shortcut format.");
                          return;
                        }
                        setScreenshotShortcutError(null);
                        setScreenshotShortcutInput(normalized.canonical);
                        if (normalized.canonical !== shortcuts.screenshot) {
                          onScreenshotShortcutChange(normalized.canonical);
                        }
                      }}
                      onKeyDown={handleStreamScreenshotShortcutKeyDown}
                      placeholder="Click, then press a key"
                      title="Focus and press the key combination to bind"
                      spellCheck={false}
                    />
                  </div>
                  {screenshotShortcutError && <span className="sidebar-hint sidebar-hint--error">{screenshotShortcutError}</span>}
                  <div className="sidebar-row sidebar-row--column">
                    <div className="sidebar-row-top">
                      <span className="sidebar-label">Recording Shortcut</span>
                    </div>
                    <input
                      type="text"
                      className={`settings-text-input settings-shortcut-input sidebar-shortcut-input ${recordingShortcutError ? "error" : ""}`}
                      value={recordingShortcutInput}
                      readOnly
                      onFocus={(event) => event.target.select()}
                      onPaste={handleStreamRecordingShortcutPaste}
                      onBlur={() => {
                        const error = getRecordingShortcutError(recordingShortcutInput);
                        if (error) {
                          setRecordingShortcutError(error);
                          return;
                        }
                        const normalized = normalizeShortcut(recordingShortcutInput.trim());
                        if (!normalized.valid) {
                          setRecordingShortcutError("Invalid shortcut format.");
                          return;
                        }
                        setRecordingShortcutError(null);
                        setRecordingShortcutInput(normalized.canonical);
                        if (normalized.canonical !== shortcuts.recording) {
                          onRecordingShortcutChange(normalized.canonical);
                        }
                      }}
                      onKeyDown={handleStreamRecordingShortcutKeyDown}
                      placeholder="Click, then press a key"
                      title="Focus and press the key combination to bind"
                      spellCheck={false}
                    />
                  </div>
                  {recordingShortcutError && <span className="sidebar-hint sidebar-hint--error">{recordingShortcutError}</span>}
                  <div className="sidebar-row sidebar-row--aligned">
                    <span className="sidebar-label">Toggle Stats</span>
                    <span className="settings-value-badge">{shortcuts.toggleStats}</span>
                  </div>
                  <div className="sidebar-row sidebar-row--aligned">
                    <span className="sidebar-label">Mouse Lock</span>
                    <span className="settings-value-badge">{shortcuts.togglePointerLock}</span>
                  </div>
                  <div className="sidebar-row sidebar-row--aligned">
                    <span className="sidebar-label">Stop Stream</span>
                    <span className="settings-value-badge">{shortcuts.stopStream}</span>
                  </div>
                  {shortcuts.toggleMicrophone && (
                    <div className="sidebar-row sidebar-row--aligned">
                      <span className="sidebar-label">Toggle Microphone</span>
                      <span className="settings-value-badge">{shortcuts.toggleMicrophone}</span>
                    </div>
                  )}
                  <div className="sidebar-row sidebar-row--aligned">
                    <span className="sidebar-label">Toggle Sidebar</span>
                    <span className="settings-value-badge">{sidebarToggleShortcutDisplay}</span>
                  </div>
                </section>
              </>
            )}
          </SideBar>
        </>
      )}

      {selectedScreenshot && (
        <div className="sv-shot-modal" role="dialog" aria-modal="true" aria-label="Screenshot preview">
          <button
            type="button"
            className="sv-shot-modal-backdrop"
            onClick={() => setSelectedScreenshotId(null)}
            aria-label="Close screenshot preview"
          />
          <div className="sv-shot-modal-card">
            <div className="sv-shot-modal-head">
              <h4>Screenshot</h4>
              <button
                type="button"
                className="sv-shot-modal-close"
                onClick={() => setSelectedScreenshotId(null)}
                aria-label="Close screenshot preview"
              >
                <X size={16} />
              </button>
            </div>
            <img
              className="sv-shot-modal-image"
              src={selectedScreenshot.dataUrl}
              alt={`Screenshot ${selectedScreenshot.fileName}`}
            />
            <div className="sv-shot-modal-actions">
              <button
                type="button"
                className="sv-shot-modal-btn"
                onClick={() => {
                  void handleSaveScreenshotAs();
                }}
              >
                <Save size={14} />
                <span>Save</span>
              </button>
              <button
                type="button"
                className="sv-shot-modal-btn sv-shot-modal-btn--danger"
                onClick={() => {
                  void handleDeleteScreenshot();
                }}
              >
                <Trash2 size={14} />
                <span>Delete</span>
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Gradient background when no video */}
      <StreamEmptyState diagnosticsStore={diagnosticsStore} />

      {/* Connecting overlay (desktop / non-controller; controller uses ControllerStreamLoading) */}
      {isConnecting && !hideConnectingOverlay && (
        <div className="sv-connect">
          <div className="sv-connect-inner">
            <Loader2 className="sv-connect-spin" size={44} />
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

      {antiAfkToggleAck && !isConnecting && (
        <div className={`sv-afk-ack sv-afk-ack--${antiAfkToggleAck}`} role="status" aria-live="polite">
          <span className="sv-afk-ack-dot" aria-hidden />
          <span>{antiAfkToggleAck === "on" ? "Anti-AFK on" : "Anti-AFK off"}</span>
        </div>
      )}

      {/* Stats HUD (top-right) */}
      {(showStatsHud || showStats) && !isConnecting && (
        <StreamStatsHud diagnosticsStore={diagnosticsStore} serverRegion={serverRegion} />
      )}

      {/* Controller indicator (top-left) */}
      <ControllerIndicator diagnosticsStore={diagnosticsStore} isConnecting={isConnecting} />

      {/* Microphone toggle button (top-left, below controller badge when present) */}
      <MicrophoneIndicator
        diagnosticsStore={diagnosticsStore}
        showAntiAfkIndicator={antiAfkEnabled && showAntiAfkIndicator}
        hideStreamButtons={hideStreamButtons}
        isConnecting={isConnecting}
        onToggleMicrophone={onToggleMicrophone}
      />

      {/* Anti-AFK indicator (top-left, below controller badge when present) */}
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
        isRecording={isRecording}
        onToggleMicrophone={onToggleMicrophone}
        recordingDurationMs={recordingDurationMs}
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
              <kbd>Enter</kbd> confirm · <kbd>Esc</kbd> cancel
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
