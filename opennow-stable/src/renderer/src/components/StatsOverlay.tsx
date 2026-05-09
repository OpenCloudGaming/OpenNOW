import { Monitor, Wifi, Activity, Gamepad2, AlertTriangle } from "lucide-react";
import type { StreamDiagnostics } from "../gfn/webrtcClient";
import type { JSX } from "react";
import { useTranslation } from "../i18n";
import { formatBitrate, getRttColor } from "../utils/streamDiagnosticsFormat";

interface StatsOverlayProps {
  stats: StreamDiagnostics;
  isVisible: boolean;
  serverRegion?: string;
  connectedControllers: number;
}

export function StatsOverlay({
  stats,
  isVisible,
  serverRegion,
  connectedControllers,
}: StatsOverlayProps): JSX.Element | null {
  const { t } = useTranslation();
  if (!isVisible) return null;

  const rttColor = getRttColor(stats.rttMs);
  const showPacketLoss = stats.packetLossPercent > 0;
  const hasData = stats.resolution !== "" || stats.bitrateKbps > 0;

  if (!hasData) {
    return (
      <div className="sovl">
          <div className="sovl-body">
          <span className="sovl-connecting">{t("stream.stats.connecting")}</span>
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
            <span className="sovl-val">{t("stream.stats.packetLossValue", { value: stats.packetLossPercent.toFixed(1) })}</span>
          </div>
        )}

        {stats.lagReason !== "stable" && stats.lagReason !== "unknown" && (
          <div className="sovl-pill sovl-pill--warn" title={stats.lagReasonDetail}>
            <AlertTriangle size={13} className="sovl-icon" />
            <span className="sovl-val">{stats.lagReason}</span>
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
