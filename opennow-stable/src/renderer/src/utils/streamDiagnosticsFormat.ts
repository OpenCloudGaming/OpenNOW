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

/**
 * Format a round-trip time value with a latency grade label.
 *
 * Grade thresholds (aligned with getLatencyGrade in streamHealthSummary):
 *   Excellent: <20 ms
 *   Good:      <50 ms
 *   Fair:      <80 ms
 *   Poor:      ≥80 ms
 *
 * @param rttMs Round-trip time in milliseconds
 * @returns     Formatted string like "18ms · Excellent" or "-- · --"
 */
export function formatLatencyGrade(rttMs: number): string {
  if (rttMs <= 0) return "-- · --";
  let grade: string;
  if (rttMs < 20) grade = "Excellent";
  else if (rttMs < 50) grade = "Good";
  else if (rttMs < 80) grade = "Fair";
  else grade = "Poor";
  return `${rttMs.toFixed(0)}ms · ${grade}`;
}

/**
 * Return a CSS color token for a jitter value.
 * Low jitter (<5 ms) is imperceptible; moderate (5–15 ms) is tolerable;
 * high (>15 ms) causes perceptible video judder.
 *
 * @param jitterMs Jitter in milliseconds
 * @returns        CSS custom-property color string
 */
export function getJitterColor(jitterMs: number): string {
  if (jitterMs <= 0) return "var(--ink-muted)";
  if (jitterMs < 5) return "var(--success)";
  if (jitterMs < 15) return "var(--warning)";
  return "var(--error)";
}
