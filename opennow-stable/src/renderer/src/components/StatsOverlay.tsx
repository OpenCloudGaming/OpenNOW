import { Monitor, Wifi, Activity, Gamepad2, AlertTriangle } from "lucide-react";
import type { StreamDiagnostics } from "../gfn/webrtcClient";
import type { JSX } from "react";

interface StatsOverlayProps {
  stats: StreamDiagnostics;
  isVisible: boolean;
  serverRegion?: string;
  connectedControllers: number;
}

function getRttColor(rttMs: number): string {
  if (rttMs <= 0) return "var(--ink-muted)";
  if (rttMs < 30) return "var(--success)";
  if (rttMs < 60) return "var(--warning)";
  return "var(--error)";
}

function formatBitrate(kbps: number): string {
  if (kbps >= 1000) return `${(kbps / 1000).toFixed(1)} Mbps`;
  return `${kbps.toFixed(0)} kbps`;
}

export function StatsOverlay({
  stats,
  isVisible,
  serverRegion,
  connectedControllers,
}: StatsOverlayProps): JSX.Element | null {
  if (!isVisible) return null;

  const rttColor = getRttColor(stats.rttMs);
  const showPacketLoss = stats.packetLossPercent > 0;
  const hasData = stats.resolution !== "" || stats.bitrateKbps > 0;

  if (!hasData) {
    return (
      <div className="sovl">
        <div className="sovl-body">
          <span className="sovl-connecting">Connecting...</span>
        </div>
      </div>
    );
  }

  return (
    <div className="sovl">
      <div className="sovl-body">
        {/* Resolution & FPS */}
        <div className="sovl-pill">
          <Monitor size={13} className="sovl-icon" />
          <span className="sovl-val">{stats.resolution} @ {stats.decodeFps} FPS</span>
        </div>

        {/* Bitrate */}
        <div className="sovl-pill">
          <Wifi size={13} className="sovl-icon" />
          <span className="sovl-val">{formatBitrate(stats.bitrateKbps)}</span>
        </div>

        {/* RTT / Latency */}
        <div className="sovl-pill">
          <Activity size={13} className="sovl-icon" style={{ color: rttColor }} />
          <span className="sovl-val" style={{ color: rttColor }}>
            {stats.rttMs > 0 ? `${stats.rttMs.toFixed(0)}ms` : "-- ms"}
          </span>
        </div>

        {/* Codec */}
        {stats.codec && (
          <div className="sovl-pill">
            <span className="sovl-badge">{stats.codec}</span>
            {stats.isHdr && <span className="sovl-badge sovl-badge--hdr">HDR</span>}
          </div>
        )}

        {/* Packet Loss */}
        {showPacketLoss && (
          <div className="sovl-pill sovl-pill--warn">
            <AlertTriangle size={13} className="sovl-icon" />
            <span className="sovl-val">{stats.packetLossPercent.toFixed(1)}% loss</span>
          </div>
        )}

        {/* Controller Status */}
        {connectedControllers > 0 && (
          <div className="sovl-pill">
            <Gamepad2 size={13} className="sovl-icon sovl-icon--ok" />
            <span className="sovl-val">{connectedControllers}</span>
          </div>
        )}

        {/* Server Region */}
        {serverRegion && (
          <div className="sovl-region">{serverRegion}</div>
        )}
      </div>
    </div>
  );
}
