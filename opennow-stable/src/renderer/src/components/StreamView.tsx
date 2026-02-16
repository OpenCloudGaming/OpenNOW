import { useState, useEffect, useCallback } from "react";
import type { JSX } from "react";
import { Maximize, Minimize, Gamepad2, Loader2, LogOut, Clock3, AlertTriangle } from "lucide-react";
import type { StreamDiagnostics } from "../gfn/webrtcClient";

interface StreamViewProps {
  videoRef: React.Ref<HTMLVideoElement>;
  audioRef: React.Ref<HTMLAudioElement>;
  stats: StreamDiagnostics;
  showStats: boolean;
  shortcuts: {
    toggleStats: string;
    togglePointerLock: string;
    stopStream: string;
  };
  serverRegion?: string;
  connectedControllers: number;
  antiAfkEnabled: boolean;
  escHoldReleaseIndicator: {
    visible: boolean;
    progress: number;
  };
  exitPrompt: {
    open: boolean;
    gameTitle: string;
  };
  sessionElapsedSeconds: number;
  streamWarning: {
    code: 1 | 2 | 3;
    message: string;
    tone: "warn" | "critical";
    secondsLeft?: number;
  } | null;
  isConnecting: boolean;
  gameTitle: string;
  onToggleFullscreen: () => void;
  onConfirmExit: () => void;
  onCancelExit: () => void;
  onEndSession: () => void;
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

export function StreamView({
  videoRef,
  audioRef,
  stats,
  showStats,
  shortcuts,
  serverRegion,
  connectedControllers,
  antiAfkEnabled,
  escHoldReleaseIndicator,
  exitPrompt,
  sessionElapsedSeconds,
  streamWarning,
  isConnecting,
  gameTitle,
  onToggleFullscreen,
  onConfirmExit,
  onCancelExit,
  onEndSession,
}: StreamViewProps): JSX.Element {
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [showHints, setShowHints] = useState(true);

  const handleFullscreenToggle = useCallback(() => {
    onToggleFullscreen();
  }, [onToggleFullscreen]);

  useEffect(() => {
    const timer = setTimeout(() => setShowHints(false), 5000);
    return () => clearTimeout(timer);
  }, []);

  useEffect(() => {
    const handleFullscreenChange = () => {
      setIsFullscreen(!!document.fullscreenElement);
    };
    document.addEventListener("fullscreenchange", handleFullscreenChange);
    return () => document.removeEventListener("fullscreenchange", handleFullscreenChange);
  }, []);

  const bitrateMbps = (stats.bitrateKbps / 1000).toFixed(1);
  const hasResolution = stats.resolution && stats.resolution !== "";
  const hasCodec = stats.codec && stats.codec !== "";
  const regionLabel = stats.serverRegion || serverRegion || "";
  const decodeColor = getTimingColor(stats.decodeTimeMs, 8, 16);
  const renderColor = getTimingColor(stats.renderTimeMs, 12, 22);
  const jitterBufferColor = getTimingColor(stats.jitterBufferDelayMs, 10, 24);
  const lossColor = getPacketLossColor(stats.packetLossPercent);
  const dText = stats.decodeTimeMs > 0 ? `${stats.decodeTimeMs.toFixed(1)}ms` : "--";
  const rText = stats.renderTimeMs > 0 ? `${stats.renderTimeMs.toFixed(1)}ms` : "--";
  const jbText = stats.jitterBufferDelayMs > 0 ? `${stats.jitterBufferDelayMs.toFixed(1)}ms` : "--";
  const inputLive = stats.inputReady && stats.connectionState === "connected";
  const escHoldProgress = Math.max(0, Math.min(1, escHoldReleaseIndicator.progress));
  const escHoldSecondsLeft = Math.max(0, 5 - Math.floor(escHoldProgress * 5));
  const inputQueueColor = getInputQueueColor(stats.inputQueueBufferedBytes, stats.inputQueueDropCount);
  const inputQueueText = `${(stats.inputQueueBufferedBytes / 1024).toFixed(1)}KB`;
  const warningSeconds = formatWarningSeconds(streamWarning?.secondsLeft);
  const sessionTimeText = formatElapsed(sessionElapsedSeconds);

  return (
    <div className="sv">
      {/* Video element */}
      <video ref={videoRef} autoPlay playsInline muted tabIndex={0} className="sv-video" />
      <audio ref={audioRef} autoPlay playsInline />

      {/* Gradient background when no video */}
      {!hasResolution && (
        <div className="sv-empty">
          <div className="sv-empty-grad" />
        </div>
      )}

      {/* Connecting overlay */}
      {isConnecting && (
        <div className="sv-connect">
          <div className="sv-connect-inner">
            <Loader2 className="sv-connect-spin" size={44} />
            <p className="sv-connect-title">Connecting to {gameTitle}</p>
            <p className="sv-connect-sub">Setting up stream...</p>
          </div>
        </div>
      )}

      {!isConnecting && (
        <div className="sv-session-clock" title="Current gaming session elapsed time">
          <Clock3 size={14} />
          <span>Session {sessionTimeText}</span>
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

      {/* Stats HUD (top-right) */}
      {showStats && !isConnecting && (
        <div className="sv-stats">
          <div className="sv-stats-head">
            {hasResolution ? (
              <span className="sv-stats-primary">{stats.resolution} · {stats.decodeFps}fps</span>
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
            <span className="sv-stats-sub-right">{bitrateMbps} Mbps</span>
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
            <span className="sv-stats-chip" title="Packet loss percentage">
              Loss <span className="sv-stats-chip-val" style={{ color: lossColor }}>{stats.packetLossPercent.toFixed(2)}%</span>
            </span>
            <span className="sv-stats-chip" title="Input queue pressure (buffered bytes and delayed flush)">
              IQ <span className="sv-stats-chip-val" style={{ color: inputQueueColor }}>{inputQueueText}</span>
            </span>
          </div>

          <div className="sv-stats-foot">
            Input queue peak {(stats.inputQueuePeakBufferedBytes / 1024).toFixed(1)}KB · drops {stats.inputQueueDropCount} · sched {stats.inputQueueMaxSchedulingDelayMs.toFixed(1)}ms
          </div>

          {(stats.gpuType || regionLabel) && (
            <div className="sv-stats-foot">
              {[stats.gpuType, regionLabel].filter(Boolean).join(" · ")}
            </div>
          )}
        </div>
      )}

      {/* Controller indicator (top-left) */}
      {connectedControllers > 0 && !isConnecting && (
        <div className="sv-ctrl" title={`${connectedControllers} controller(s) connected`}>
          <Gamepad2 size={18} />
          {connectedControllers > 1 && <span className="sv-ctrl-n">{connectedControllers}</span>}
        </div>
      )}

      {/* Anti-AFK indicator (top-left, below controller badge when present) */}
      {antiAfkEnabled && !isConnecting && (
        <div className={`sv-afk${connectedControllers > 0 ? " sv-afk--stacked" : ""}`} title="Anti-AFK is enabled">
          <span className="sv-afk-dot" />
          <span className="sv-afk-label">ANTI-AFK ON</span>
        </div>
      )}

      {/* Hold-Esc release indicator (appears after 1s hold) */}
      {escHoldReleaseIndicator.visible && !isConnecting && (
        <>
          <div className="sv-esc-hold-backdrop" />
          <div className="sv-esc-hold" title="Keep holding Escape to release mouse lock">
            <div className="sv-esc-hold-title">Hold Escape to Release Mouse</div>
            <div className="sv-esc-hold-head">
              <span>Keep holding…</span>
              <span>{escHoldSecondsLeft}s</span>
            </div>
            <div className="sv-esc-hold-track">
              <span className="sv-esc-hold-fill" style={{ transform: `scaleX(${escHoldProgress})` }} />
            </div>
          </div>
        </>
      )}

      {exitPrompt.open && !isConnecting && (
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
        </div>
      )}

      {/* Fullscreen toggle */}
      <button
        className="sv-fs"
        onClick={handleFullscreenToggle}
        title={isFullscreen ? "Exit fullscreen" : "Enter fullscreen"}
        aria-label={isFullscreen ? "Exit fullscreen" : "Enter fullscreen"}
      >
        {isFullscreen ? <Minimize size={18} /> : <Maximize size={18} />}
      </button>

      {/* End session button */}
      <button
        className="sv-end"
        onClick={onEndSession}
        title="End session"
        aria-label="End session"
      >
        <LogOut size={18} />
      </button>

      {/* Keyboard hints */}
      {showHints && !isConnecting && (
        <div className="sv-hints">
          <div className="sv-hint"><kbd>{shortcuts.toggleStats}</kbd><span>Stats</span></div>
          <div className="sv-hint"><kbd>{shortcuts.togglePointerLock}</kbd><span>Mouse lock</span></div>
          <div className="sv-hint"><kbd>{shortcuts.stopStream}</kbd><span>Stop</span></div>
        </div>
      )}

      {/* Game title (bottom-center, fades) */}
      {hasResolution && showHints && (
        <div className="sv-title-bar">
          <span>{gameTitle}</span>
        </div>
      )}
    </div>
  );
}
