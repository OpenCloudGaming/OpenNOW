import type { JSX, ReactNode } from "react";
import { Activity } from "lucide-react";
import type { StreamDiagnosticsStore } from "../../../utils/streamDiagnosticsStore";
import { useStreamDiagnosticsSelector } from "../../../utils/streamDiagnosticsStore";
import { getStreamHealthSummary, type StreamHealthTier } from "../../../utils/streamHealthSummary";
import { SessionElapsedIndicator } from "../../ElapsedSessionIndicators";

export interface StreamWarningLine {
  message: string;
  tone: "warn" | "critical" | string;
}

export interface ControllerInStreamShellProps {
  children: ReactNode;
  diagnosticsStore: StreamDiagnosticsStore;
  sessionStartedAtMs: number | null;
  sessionCounterEnabled: boolean;
  isStreaming: boolean;
  streamWarning?: StreamWarningLine | null;
  queuePosition?: number;
}

function healthTierClass(tier: StreamHealthTier): string {
  switch (tier) {
    case "good":
      return "cis-health--good";
    case "fair":
      return "cis-health--fair";
    case "poor":
      return "cis-health--poor";
    default:
      return "cis-health--connecting";
  }
}

export function ControllerInStreamShell({
  children,
  diagnosticsStore,
  sessionStartedAtMs,
  sessionCounterEnabled,
  isStreaming,
  streamWarning,
  queuePosition,
}: ControllerInStreamShellProps): JSX.Element {
  const health = useStreamDiagnosticsSelector(
    diagnosticsStore,
    (d) => getStreamHealthSummary(d),
    (a, b) => a.label === b.label && a.tier === b.tier,
  );
  const connectedPads = useStreamDiagnosticsSelector(diagnosticsStore, (d) => d.connectedGamepads);

  const showQueue = typeof queuePosition === "number" && Number.isFinite(queuePosition) && queuePosition > 0;

  return (
    <div className="controller-overlay controller-overlay--in-stream" role="presentation">
      <div className="cis-root">
        {streamWarning ? (
          <div className={`cis-warning cis-warning--${streamWarning.tone}`} role="status">
            {streamWarning.message}
            {showQueue ? <span className="cis-warning-queue"> · Queue #{queuePosition}</span> : null}
          </div>
        ) : showQueue ? (
          <div className="cis-warning cis-warning--warn" role="status">
            Queue position #{queuePosition}
          </div>
        ) : null}

        <div className="cis-toolbar">
          <div className="cis-toolbar-left">
            {sessionCounterEnabled ? (
              <div className="cis-clock">
                <SessionElapsedIndicator startedAtMs={sessionStartedAtMs} active={isStreaming} className="cis-elapsed" />
              </div>
            ) : null}
            <div className={`cis-health ${healthTierClass(health.tier)}`} title="Stream quality">
              <Activity size={14} aria-hidden />
              <span>{health.label}</span>
            </div>
            {connectedPads > 0 ? (
              <span className="cis-pads" title="Connected controllers">
                {connectedPads} pad{connectedPads === 1 ? "" : "s"}
              </span>
            ) : null}
          </div>
        </div>

        <div className="cis-xmb-host">{children}</div>
      </div>
    </div>
  );
}
