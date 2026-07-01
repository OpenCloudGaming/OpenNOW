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

// ---------------------------------------------------------------------------
// Mac mini / macOS optimized presets
// ---------------------------------------------------------------------------

/** Preset identifiers for macOS-specific stream quality configurations. */
export type MacStreamQualityPresetId =
  | 'mac-performance'
  | 'mac-balanced'
  | 'mac-quality'
  | 'mac-ultra';

/**
 * Extended stream preset for macOS, adding codec and latency hints that
 * the macOS-specific streaming path can act on.
 */
export interface MacStreamPreset extends StreamPresetPick {
  /** Preferred video codec for this preset, if the platform supports it. */
  codecHint?: 'H264' | 'AV1' | 'H265';
  /** Target latency mode passed to the streaming layer. */
  latencyMode?: 'low' | 'ultra-low' | 'normal';
}

/**
 * Returns hardcoded Mac mini–optimised presets indexed by {@link MacStreamQualityPresetId}.
 *
 * Preset summary:
 * - `mac-ultra`       — 2560×1440 · 60 fps · 150 Mbps · H264   · normal latency
 * - `mac-quality`     — 1920×1080 · 60 fps · 120 Mbps · AV1    · normal latency (Apple Silicon)
 * - `mac-balanced`    — 1920×1080 · 60 fps ·  75 Mbps · H264   · low latency
 * - `mac-performance` — 1280×720  · 60 fps ·  45 Mbps · H264   · ultra-low latency
 */
export function getMacStreamPresets(): Record<MacStreamQualityPresetId, MacStreamPreset> {
  return {
    'mac-ultra': {
      resolution: '2560x1440',
      fps: 60,
      maxBitrateMbps: 150,
      codecHint: 'H264',
      latencyMode: 'normal',
    },
    'mac-quality': {
      resolution: '1920x1080',
      fps: 60,
      maxBitrateMbps: 120,
      codecHint: 'AV1',
      latencyMode: 'normal',
    },
    'mac-balanced': {
      resolution: '1920x1080',
      fps: 60,
      maxBitrateMbps: 75,
      codecHint: 'H264',
      latencyMode: 'low',
    },
    'mac-performance': {
      resolution: '1280x720',
      fps: 60,
      maxBitrateMbps: 45,
      codecHint: 'H264',
      latencyMode: 'ultra-low',
    },
  };
}

/**
 * Returns the {@link MacStreamPreset} for the given `presetId`, applying
 * Apple Silicon–specific overrides when `isAppleSilicon` is `true`.
 *
 * Override rules:
 * - `mac-quality` and `mac-ultra` on Apple Silicon → `codecHint` is forced to
 *   `'AV1'` to leverage hardware AV1 decode available on M3+ chips.
 * - All other combinations are returned as-is from {@link getMacStreamPresets}.
 *
 * @param presetId       - The desired Mac stream quality preset.
 * @param isAppleSilicon - Whether the host machine runs on Apple Silicon (M-series).
 * @returns A {@link MacStreamPreset} ready to pass to the streaming layer.
 */
export function pickMacStreamPreset(
  presetId: MacStreamQualityPresetId,
  isAppleSilicon: boolean,
): MacStreamPreset {
  const presets = getMacStreamPresets();
  const preset = { ...presets[presetId] };

  if (isAppleSilicon && (presetId === 'mac-quality' || presetId === 'mac-ultra')) {
    preset.codecHint = 'AV1';
  }

  return preset;
}
