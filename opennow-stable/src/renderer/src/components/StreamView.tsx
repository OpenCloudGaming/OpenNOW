import { useState, useEffect, useCallback } from "react";
import type { JSX } from "react";
import { Maximize, Minimize, Gamepad2, Loader2, LogOut } from "lucide-react";
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
  isConnecting: boolean;
  gameTitle: string;
  onToggleFullscreen: () => void;
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

export function StreamView({
  videoRef,
  audioRef,
  stats,
  showStats,
  shortcuts,
  serverRegion,
  connectedControllers,
  antiAfkEnabled,
  isConnecting,
  gameTitle,
  onToggleFullscreen,
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
