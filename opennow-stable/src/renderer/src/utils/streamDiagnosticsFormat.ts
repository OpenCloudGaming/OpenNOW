export function getRttColor(rttMs: number): string {
  if (rttMs <= 0) return "var(--ink-muted)";
  if (rttMs < 30) return "var(--success)";
  if (rttMs < 60) return "var(--warning)";
  return "var(--error)";
}

export function getPacketLossColor(lossPercent: number): string {
  if (lossPercent <= 0.15) return "var(--success)";
  if (lossPercent < 1) return "var(--warning)";
  return "var(--error)";
}

export function getTimingColor(valueMs: number, goodMax: number, warningMax: number): string {
  if (valueMs <= 0) return "var(--ink-muted)";
  if (valueMs <= goodMax) return "var(--success)";
  if (valueMs <= warningMax) return "var(--warning)";
  return "var(--error)";
}

export function getInputQueueColor(bufferedBytes: number, dropCount: number): string {
  if (dropCount > 0 || bufferedBytes >= 65536) return "var(--error)";
  if (bufferedBytes >= 32768) return "var(--warning)";
  return "var(--success)";
}

export function getBitratePerformanceColor(percent: number): string {
  if (percent <= 0) return "var(--ink-muted)";
  if (percent >= 70 && percent <= 110) return "var(--success)";
  if (percent >= 45 && percent < 130) return "var(--warning)";
  return "var(--error)";
}

export function formatBitrate(kbps: number): string {
  if (kbps >= 1000) return `${(kbps / 1000).toFixed(1)} Mbps`;
  return `${kbps.toFixed(0)} kbps`;
}
