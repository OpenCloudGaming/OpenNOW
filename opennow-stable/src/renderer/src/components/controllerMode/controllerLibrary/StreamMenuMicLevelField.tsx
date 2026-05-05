import type { JSX } from "react";
import { useRef } from "react";
import { useMicMeter } from "../../../hooks/useMicMeter";

export function StreamMenuMicLevelField({
  streamMenuMicLevel,
  onStreamMenuMicLevelChange,
  editingStreamMicLevel,
  isRowSelected,
  micTrack,
  controllerType,
}: {
  streamMenuMicLevel?: number;
  onStreamMenuMicLevelChange?: (value: number) => void;
  editingStreamMicLevel: boolean;
  isRowSelected: boolean;
  micTrack?: MediaStreamTrack | null;
  controllerType: "ps" | "xbox" | "nintendo" | "generic";
}): JSX.Element {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const trackLive = Boolean(micTrack && micTrack.readyState === "live");
  const meterActive = trackLive && (editingStreamMicLevel || isRowSelected);
  useMicMeter(canvasRef, micTrack ?? null, meterActive);

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 8, width: "100%", minWidth: 0 }}>
      <div style={{ display: "flex", alignItems: "center", gap: 12, flexWrap: "wrap" }}>
        <input
          type="range"
          min={0}
          max={100}
          step={1}
          value={Math.round((streamMenuMicLevel ?? 1) * 100)}
          onChange={(e) => onStreamMenuMicLevelChange?.(Math.max(0, Math.min(1, Number(e.target.value) / 100)))}
          aria-label="Microphone level"
          style={editingStreamMicLevel ? { outline: "2px solid rgba(255,255,255,0.2)" } : undefined}
        />
        <span className="xmb-game-meta-chip">
          {`${Math.round((streamMenuMicLevel ?? 1) * 100)}%`}
          {editingStreamMicLevel ? " • Editing ←/→" : controllerType === "ps" ? " • □ to adjust" : " • X to adjust"}
        </span>
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
        <span className="xmb-game-meta-chip" style={{ opacity: 0.75, fontSize: "0.85em" }}>
          Send level (what others hear)
        </span>
        <canvas
          ref={canvasRef}
          width={280}
          height={14}
          className="xmb-stream-mic-meter"
          style={{
            width: "100%",
            maxWidth: 320,
            height: 14,
            display: "block",
            opacity: trackLive ? 1 : 0.35,
          }}
          aria-hidden
        />
        {!trackLive ? (
          <span className="xmb-game-meta-chip" style={{ opacity: 0.55, fontSize: "0.8em" }}>
            No send audio — unmute mic or check permissions to test
          </span>
        ) : null}
      </div>
    </div>
  );
}
