import { readFile } from "node:fs/promises";

import { Decoder, Reader, tools } from "ts-ebml";

const MIN_PLAUSIBLE_MS = 250;
const READER_FLOOR_MS = 50;

function copyBufferForEbmlDecode(buf: Buffer): ArrayBuffer {
  const copy = Buffer.allocUnsafe(buf.length);
  buf.copy(copy);
  return copy.buffer.slice(copy.byteOffset, copy.byteOffset + copy.byteLength);
}

type BlockSample = { readonly track: number; readonly absTick: number };

function collectClusterBlockSamples(elements: ReturnType<Decoder["decode"]>): BlockSample[] {
  const samples: BlockSample[] = [];
  let clusterDepth = 0;
  let clusterTimecode = 0;

  for (const elm of elements) {
    if (elm.type === "m" && elm.name === "Cluster") {
      if (!elm.isEnd) {
        clusterDepth += 1;
        clusterTimecode = 0;
      } else {
        clusterDepth = Math.max(0, clusterDepth - 1);
      }
      continue;
    }

    if (clusterDepth === 0) continue;

    if (elm.type === "u" && elm.name === "Timestamp") {
      clusterTimecode = elm.value as number;
      continue;
    }

    if (elm.type === "b" && (elm.name === "SimpleBlock" || elm.name === "Block")) {
      try {
        const blk = tools.ebmlBlock(elm.data);
        samples.push({ track: blk.trackNumber, absTick: clusterTimecode + blk.timecode });
      } catch {
        // ignore corrupt block
      }
    }
  }

  return samples;
}

/** Largest gap between consecutive blocks on the same track (tick units); estimates last frame duration. */
function maxSameTrackDeltaTicks(samples: BlockSample[]): number {
  if (samples.length === 0) return 0;
  const sorted = [...samples].sort((a, b) => a.absTick - b.absTick || a.track - b.track);
  let maxDelta = 0;
  for (let i = 1; i < sorted.length; i++) {
    const cur = sorted[i]!;
    const prev = sorted[i - 1]!;
    if (cur.track === prev.track) {
      maxDelta = Math.max(maxDelta, cur.absTick - prev.absTick);
    }
  }
  return maxDelta;
}

function readerDurationMs(reader: Reader): number {
  const ns = reader.duration * reader.timestampScale;
  return Math.round(ns / 1e6);
}

/**
 * Wall-clock span of samples in the file: (latest − earliest block time) + tail frame estimate.
 * Required when the first cluster still uses timestamps from a long MediaRecorder session after older
 * chunks were dropped — using only max(abs) would report duration from segment t=0, not clip length.
 */
function clusterScanSpanMs(
  elements: ReturnType<Decoder["decode"]>,
  timestampScale: number,
): number | null {
  if (timestampScale <= 0 || !Number.isFinite(timestampScale)) return null;
  const samples = collectClusterBlockSamples(elements);
  if (samples.length === 0) return null;
  let minAbs = Infinity;
  let maxAbs = 0;
  for (const s of samples) {
    minAbs = Math.min(minAbs, s.absTick);
    maxAbs = Math.max(maxAbs, s.absTick);
  }
  if (!Number.isFinite(minAbs)) return null;
  const tailTicks = maxSameTrackDeltaTicks(samples);
  const spanTicks = Math.max(0, maxAbs - minAbs) + tailTicks;
  if (spanTicks <= 0) return null;
  return Math.round((spanTicks * timestampScale) / 1e6);
}

function plausibleBoundsMs(fallbackDurationMs: number, fileSizeBytes: number): { min: number; max: number } {
  const slackMs = 120_000;
  const maxFromFallback = Math.max(fallbackDurationMs * 4 + slackMs, 10 * 60_000);
  const sixHours = 6 * 60 * 60 * 1000;
  const looseBitrateCapMs = Math.floor((fileSizeBytes * 8) / 3000) * 1000;
  const max = Math.min(Math.max(maxFromFallback, 60_000), sixHours, looseBitrateCapMs || sixHours);
  return { min: MIN_PLAUSIBLE_MS, max };
}

function passesSanity(ms: number, bounds: { min: number; max: number }): boolean {
  return Number.isFinite(ms) && ms >= bounds.min && ms <= bounds.max;
}

export function probeWebmDurationFromBuffer(buf: Buffer, fallbackDurationMs: number): number {
  const bounds = plausibleBoundsMs(fallbackDurationMs, buf.byteLength);

  try {
    const ab = copyBufferForEbmlDecode(buf);
    const decoder = new Decoder();
    const elements = decoder.decode(ab);

    const reader = new Reader();
    reader.drop_default_duration = false;
    for (const elm of elements) {
      reader.read(elm);
    }
    reader.stop();

    const readerMs = readerDurationMs(reader);
    const clusterSpanMs = clusterScanSpanMs(elements, reader.timestampScale);

    if (clusterSpanMs !== null && passesSanity(clusterSpanMs, bounds)) {
      return clusterSpanMs;
    }

    if (Number.isFinite(readerMs) && readerMs >= READER_FLOOR_MS && passesSanity(readerMs, bounds)) {
      return readerMs;
    }

    if (clusterSpanMs !== null && Number.isFinite(clusterSpanMs) && clusterSpanMs >= READER_FLOOR_MS) {
      return clusterSpanMs;
    }

    return fallbackDurationMs;
  } catch {
    return fallbackDurationMs;
  }
}

/** Best-effort MP4 movie duration from `mvhd` (full file buffer; `moov` may be at end). */
export function probeMp4DurationFromBuffer(buf: Buffer): number | null {
  return readMvhdDurationMs(buf);
}

function readUInt32BE(buf: Buffer, o: number): number {
  return buf.readUInt32BE(o);
}

function readUInt64BE(buf: Buffer, o: number): bigint {
  return buf.readBigUInt64BE(o);
}

function readMvhdDurationMs(buf: Buffer): number | null {
  let best: number | null = null;
  walkBoxes(buf, 0, buf.length, (type, payload) => {
    if (type !== "mvhd" || payload.length < 20) return;
    const version = payload[0];
    let timescale: number;
    let durationTicks: number;
    if (version === 1) {
      if (payload.length < 32) return;
      timescale = readUInt32BE(payload, 20);
      const d = readUInt64BE(payload, 24);
      durationTicks = Number(d > BigInt(Number.MAX_SAFE_INTEGER) ? BigInt(0) : d);
    } else {
      timescale = readUInt32BE(payload, 12);
      durationTicks = readUInt32BE(payload, 16);
    }
    if (timescale <= 0 || durationTicks < 0 || !Number.isFinite(durationTicks)) return;
    const ms = Math.round((durationTicks / timescale) * 1000);
    if (Number.isFinite(ms) && ms > 0) {
      best = ms;
    }
  });
  return best;
}

function walkBoxes(buf: Buffer, start: number, end: number, visit: (type: string, payload: Buffer) => void): void {
  let offset = start;
  while (offset + 8 <= end) {
    const sizeHi = readUInt32BE(buf, offset);
    const type = buf.toString("ascii", offset + 4, offset + 8);
    let header = 8;
    let boxLen = sizeHi;
    if (sizeHi === 1) {
      if (offset + 16 > end) return;
      const big = readUInt64BE(buf, offset + 8);
      if (big > BigInt(Number.MAX_SAFE_INTEGER)) return;
      boxLen = Number(big);
      header = 16;
    }
    if (boxLen < header || offset + boxLen > end) return;
    const contentStart = offset + header;
    const contentEnd = offset + boxLen;
    const inner = buf.subarray(contentStart, contentEnd);
    if (type === "moov" || type === "trak" || type === "mdia") {
      walkBoxes(inner, 0, inner.length, visit);
    } else {
      visit(type, inner);
    }
    offset += boxLen;
  }
}

function isWebmPathOrMime(filePath: string, mimeType: string): boolean {
  const m = mimeType.toLowerCase();
  return m.includes("webm") || filePath.toLowerCase().endsWith(".webm");
}

function isMp4PathOrMime(filePath: string, mimeType: string): boolean {
  const m = mimeType.toLowerCase();
  return m.includes("mp4") || filePath.toLowerCase().endsWith(".mp4");
}

/**
 * Probes container duration after save. WebM uses dual EBML estimate; MP4 reads `mvhd`.
 * Falls back to renderer-supplied duration when the probe is implausible.
 */
export async function probeRecordingDurationOnDisk(
  filePath: string,
  mimeType: string,
  fallbackDurationMs: number,
): Promise<number> {
  try {
    const buf = await readFile(filePath);

    if (isWebmPathOrMime(filePath, mimeType)) {
      return probeWebmDurationFromBuffer(buf, fallbackDurationMs);
    }

    if (isMp4PathOrMime(filePath, mimeType)) {
      const mp4 = probeMp4DurationFromBuffer(buf);
      const bounds = plausibleBoundsMs(fallbackDurationMs, buf.byteLength);
      if (mp4 !== null && passesSanity(mp4, bounds)) return mp4;
      return fallbackDurationMs;
    }
  } catch {
    // fall through
  }

  return fallbackDurationMs;
}

/** @deprecated Use {@link probeRecordingDurationOnDisk} */
export async function probeWebmDurationOnDisk(
  filePath: string,
  mimeType: string,
  fallbackDurationMs: number,
): Promise<number> {
  return probeRecordingDurationOnDisk(filePath, mimeType, fallbackDurationMs);
}
