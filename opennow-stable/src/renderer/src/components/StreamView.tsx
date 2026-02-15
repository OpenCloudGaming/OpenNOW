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
  if (lossPercent <= 0.1) return "var(--ink-muted)";
  if (lossPercent < 1) return "var(--warning)";
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
  const showPacketLoss = stats.packetLossPercent > 0.1;
  const showFrameCounters = stats.framesReceived > 0;
  const showTiming = stats.decodeTimeMs > 0 || stats.renderTimeMs > 0;
  const regionLabel = stats.serverRegion || serverRegion || "";

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
          {/* Primary: Resolution + FPS */}
          <div className="sv-stats-head">
            {hasResolution ? (
              <span>{stats.resolution} @ {stats.decodeFps} fps</span>
            ) : (
              <span className="sv-stats-wait">Connecting...</span>
            )}
          </div>

          {/* Codec + Bitrate */}
          {hasCodec && (
            <div className="sv-stats-row sv-stats-codec">
              <span>{stats.codec} · {bitrateMbps} Mbps</span>
              {stats.isHdr && <span className="sv-stats-hdr">HDR</span>}
            </div>
          )}

          {/* RTT */}
          <div className="sv-stats-row">
            <span className="sv-stats-lbl">RTT</span>
            <span className="sv-stats-val" style={{ color: getRttColor(stats.rttMs) }}>
              {stats.rttMs > 0 ? `${stats.rttMs.toFixed(0)}ms` : "N/A"}
            </span>
          </div>

          {/* Packet Loss */}
          {showPacketLoss && (
            <div className="sv-stats-row">
              <span className="sv-stats-lbl">Loss</span>
              <span className="sv-stats-val" style={{ color: getPacketLossColor(stats.packetLossPercent) }}>
                {stats.packetLossPercent.toFixed(2)}%
              </span>
            </div>
          )}

          {/* Decode/Render timing */}
          {showTiming && (
            <div className="sv-stats-row sv-stats-dim">
              D: {stats.decodeTimeMs.toFixed(1)}ms · R: {stats.renderTimeMs.toFixed(1)}ms
              {stats.jitterBufferDelayMs > 0 && ` · JB: ${stats.jitterBufferDelayMs.toFixed(1)}ms`}
            </div>
          )}

          {/* Frame counters */}
          {showFrameCounters && (
            <div className="sv-stats-row sv-stats-dim">
              F: {stats.framesDecoded}/{stats.framesReceived} ({stats.framesDropped} drop)
            </div>
          )}

          {/* GPU + Region */}
          {(stats.gpuType || regionLabel) && (
            <div className="sv-stats-row sv-stats-sys">
              {[stats.gpuType, regionLabel].filter(Boolean).join(" · ")}
            </div>
          )}

          {/* Connection state */}
          <div className="sv-stats-row sv-stats-state">
            {stats.connectionState}{stats.inputReady ? " · Input ready" : " · Input pending"}
          </div>
        </div>
      )}

      {/* Controller indicator (top-left) */}
      {connectedControllers > 0 && !isConnecting && (
        <div className="sv-ctrl" title={`${connectedControllers} controller(s) connected`}>
          <Gamepad2 size={18} />
          {connectedControllers > 1 && <span className="sv-ctrl-n">{connectedControllers}</span>}
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
