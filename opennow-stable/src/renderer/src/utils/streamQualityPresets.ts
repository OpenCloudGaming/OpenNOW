export type StreamQualityPresetId = "performance" | "balanced" | "quality";

export interface StreamPresetPick {
  resolution: string;
  fps: number;
  maxBitrateMbps: number;
}

function parseResPixels(res: string): number {
  const m = /^(\d+)x(\d+)$/.exec(res.trim());
  if (!m) return 0;
  return Number(m[1]) * Number(m[2]);
}

function sortedByPixelsAsc(resolutions: string[]): string[] {
  return [...resolutions].sort((a, b) => parseResPixels(a) - parseResPixels(b));
}

function sortedFpsAsc(fpsOptions: number[]): number[] {
  return [...fpsOptions].sort((a, b) => a - b);
}

/**
 * Pick resolution/FPS/bitrate targets for each preset from allowed lists (caller applies via settings).
 */
export function pickStreamPreset(
  preset: StreamQualityPresetId,
  resolutions: string[],
  fpsOptions: number[],
  bitrateMin = 5,
  bitrateMax = 150,
): StreamPresetPick | null {
  if (resolutions.length === 0 || fpsOptions.length === 0) return null;

  const byRes = sortedByPixelsAsc(resolutions);
  const byFps = sortedFpsAsc(fpsOptions);

  const lowRes = byRes[0]!;
  const highRes = byRes[byRes.length - 1]!;
  const midRes = byRes[Math.floor((byRes.length - 1) / 2)]!;

  const lowFps = byFps[0]!;
  const highFps = byFps[byFps.length - 1]!;
  const midFps = byFps[Math.floor((byFps.length - 1) / 2)]!;

  const lowBitrate = Math.max(bitrateMin, Math.min(45, bitrateMax));
  const midBitrate = Math.max(bitrateMin, Math.min(75, bitrateMax));
  const highBitrate = Math.max(bitrateMin, Math.min(120, bitrateMax));

  if (preset === "performance") {
    return { resolution: lowRes, fps: highFps, maxBitrateMbps: lowBitrate };
  }
  if (preset === "quality") {
    return { resolution: highRes, fps: midFps, maxBitrateMbps: highBitrate };
  }
  return { resolution: midRes, fps: midFps, maxBitrateMbps: midBitrate };
}
