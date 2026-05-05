import type { StreamDiagnostics } from "../gfn/webrtcClient";

export type StreamHealthTier = "good" | "fair" | "poor" | "connecting";

export interface StreamHealthSummary {
  /** Short label for UI chips */
  label: string;
  tier: StreamHealthTier;
}

function tierFromRttLoss(rttMs: number, packetLossPercent: number): StreamHealthTier {
  if (rttMs <= 0 && packetLossPercent <= 0) return "connecting";
  if (packetLossPercent >= 3 || rttMs >= 90) return "poor";
  if (packetLossPercent >= 1 || rttMs >= 60) return "fair";
  if (rttMs >= 30 || packetLossPercent > 0.15) return "fair";
  return "good";
}

/**
 * Compact stream quality summary for in-stream menu chip (aligned loosely with StatsOverlay RTT bands).
 */
export function getStreamHealthSummary(d: StreamDiagnostics): StreamHealthSummary {
  const lag = d.lagReason;
  if (lag === "decoder" || lag === "input_backpressure") {
    return { label: "Local strain", tier: "fair" };
  }
  if (lag === "network") {
    return { label: "Network", tier: "poor" };
  }

  const hasVideo = Boolean(d.resolution && d.resolution.length > 0);
  if (!hasVideo && d.rttMs <= 0) {
    return { label: "Connecting…", tier: "connecting" };
  }

  const tier = tierFromRttLoss(d.rttMs, d.packetLossPercent);
  const labels: Record<StreamHealthTier, string> = {
    good: "Good",
    fair: "Fair",
    poor: "Poor",
    connecting: "Connecting…",
  };
  return { label: labels[tier], tier };
}
