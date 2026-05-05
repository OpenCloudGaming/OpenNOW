import type { RecordingEntry } from "@shared/gfn";
import { pickSupportedRecordingMime } from "../../lib/recordingMime";
import { buildComposedRecordingStream, type ComposedRecordingResources } from "../composedRecordingStream";

export const INSTANT_REPLAY_TIMESLICE_MS = 1000;

/** Leading `dataavailable` blobs can be tiny; merge until we have a plausible WebM prefix (EBML + segment start). */
const MIN_INIT_ACCUM_BYTES = 512;

/** Inline IPC payload limit — larger single clips use chunked recording IPC instead of one ArrayBuffer. */
const SINGLE_CLIP_INLINE_MAX_BYTES = 32 * 1024 * 1024;

/**
 * When slicing to a wall-clock start time, include one extra timeslice **before** that boundary so the
 * first SimpleBlock is more likely to land on a decodable cluster (reduces frozen/garbled frames).
 */
const CHUNK_PREROLL_COUNT = 1;

function makeThumbnailDataUrl(video: HTMLVideoElement): string | null {
  if (video.videoWidth <= 0 || video.videoHeight <= 0) return null;
  const maxW = 320;
  const maxH = 180;
  let w = video.videoWidth;
  let h = video.videoHeight;
  if (w > maxW) {
    h = Math.round((maxW / w) * h);
    w = maxW;
  }
  if (h > maxH) {
    w = Math.round((maxH / h) * w);
    h = maxH;
  }
  const canvas = document.createElement("canvas");
  canvas.width = w;
  canvas.height = h;
  const ctx = canvas.getContext("2d");
  if (!ctx) return null;
  ctx.drawImage(video, 0, 0, w, h);
  return canvas.toDataURL("image/jpeg", 0.72);
}

export type RollingInstantReplayOptions = {
  bufferWindowMs: number;
  getVideo: () => HTMLVideoElement | null;
  getAudio: () => HTMLAudioElement | null;
  getMicTrack: () => MediaStreamTrack | null;
};

type ReplayChunk = {
  endedAtMs: number;
  blob: Blob;
};

/**
 * Rolling buffer: one continuous `MediaRecorder` session (`initChunk` + sequential `dataavailable` blobs).
 *
 * **Retention:** When the oldest chunk is outside `bufferWindowMs` (or the byte cap is exceeded), we
 * **drop** those blobs from RAM and **keep recording** — same recorder, same stream; only the in-memory
 * ring is trimmed. Saved clips still use `initChunk` plus the chunk tail (same pattern as slicing for
 * “save last N seconds”).
 *
 * **Destination capture / MIME:** see `buildComposedRecordingStream` and `recordingMime.ts`.
 */
export class RollingInstantReplayController {
  private readonly options: RollingInstantReplayOptions;
  private destroyed = false;
  private manualHold = false;
  private resources: ComposedRecordingResources | null = null;
  private mediaRecorder: MediaRecorder | null = null;
  private chunks: ReplayChunk[] = [];
  private bytesInBuffer = 0;
  private initChunk: Blob | null = null;
  private pendingInitParts: Blob[] = [];
  private pendingInitBytes = 0;
  private readonly maxBufferBytes = 256 * 1024 * 1024; // 256MB safety cap
  private readonly mimeType = pickSupportedRecordingMime();

  constructor(options: RollingInstantReplayOptions) {
    this.options = options;
  }

  private apiAvailable(): boolean {
    return (
      typeof window.openNow?.beginRecording === "function" &&
      typeof window.openNow?.sendRecordingChunk === "function" &&
      typeof window.openNow?.finishRecording === "function" &&
      typeof window.openNow?.instantReplaySave === "function"
    );
  }

  async start(): Promise<void> {
    if (this.destroyed || !this.apiAvailable()) return;
    this.clearBufferInternal();
    this.startRecorderIfPossible();
  }

  private ensureComposed(): ComposedRecordingResources | null {
    if (this.resources) return this.resources;
    const video = this.options.getVideo();
    const audio = this.options.getAudio();
    if (!video?.srcObject) return null;
    const built = buildComposedRecordingStream({
      videoElement: video,
      gameAudioElement: audio,
      micTrack: this.options.getMicTrack(),
    });
    if (!built) return null;
    this.resources = built;
    return built;
  }

  private disposeComposed(): void {
    this.resources?.dispose();
    this.resources = null;
  }

  /** Drop oldest chunks that fall outside the replay window or byte budget; recorder keeps running. */
  private trimBufferAfterAppend(nowMs: number): void {
    const cutoff = nowMs - Math.max(INSTANT_REPLAY_TIMESLICE_MS, this.options.bufferWindowMs);
    while (this.chunks.length > 0 && this.chunks[0]!.endedAtMs < cutoff) {
      const removed = this.chunks.shift()!;
      this.bytesInBuffer -= removed.blob.size;
    }
    while (this.bytesInBuffer > this.maxBufferBytes && this.chunks.length > 0) {
      const removed = this.chunks.shift()!;
      this.bytesInBuffer -= removed.blob.size;
    }
  }

  private clearBufferInternal(): void {
    this.chunks = [];
    this.bytesInBuffer = 0;
    this.initChunk = null;
    this.pendingInitParts = [];
    this.pendingInitBytes = 0;
  }

  private startRecorderIfPossible(): void {
    if (this.destroyed || this.manualHold || this.mediaRecorder) return;
    const composed = this.ensureComposed();
    if (!composed) return;
    const rec = new MediaRecorder(composed.composed, { mimeType: this.mimeType });
    this.mediaRecorder = rec;
    rec.ondataavailable = (e) => {
      if (this.destroyed || this.manualHold || !e.data || e.data.size === 0) return;
      if (!this.initChunk) {
        this.pendingInitParts.push(e.data);
        this.pendingInitBytes += e.data.size;
        if (this.pendingInitBytes < MIN_INIT_ACCUM_BYTES) {
          return;
        }
        this.initChunk = new Blob(this.pendingInitParts, { type: this.mimeType });
        this.pendingInitParts = [];
        this.pendingInitBytes = 0;
        return;
      }
      const nowMs = Date.now();
      this.chunks.push({ endedAtMs: nowMs, blob: e.data });
      this.bytesInBuffer += e.data.size;
      this.trimBufferAfterAppend(nowMs);
    };
    rec.onerror = () => {
      this.mediaRecorder = null;
    };
    rec.onstop = () => {
      this.mediaRecorder = null;
      if (!this.destroyed && !this.manualHold) {
        this.startRecorderIfPossible();
      }
    };
    try {
      rec.start(INSTANT_REPLAY_TIMESLICE_MS);
    } catch (err) {
      console.error("[InstantReplay] recorder.start failed:", err);
      this.mediaRecorder = null;
    }
  }

  private async stopRecorder(): Promise<void> {
    const rec = this.mediaRecorder;
    if (!rec) return;
    await new Promise<void>((resolve) => {
      const done = (): void => resolve();
      const timer = window.setTimeout(done, 1_500);
      const onStop = (): void => {
        window.clearTimeout(timer);
        rec.removeEventListener("stop", onStop);
        done();
      };
      rec.addEventListener("stop", onStop);
      try {
        rec.stop();
      } catch {
        window.clearTimeout(timer);
        rec.removeEventListener("stop", onStop);
        done();
      }
    });
  }

  async pauseForManualRecording(): Promise<void> {
    this.manualHold = true;
    await this.stopRecorder();
    this.disposeComposed();
    this.clearBufferInternal();
  }

  async resumeAfterManualRecording(): Promise<void> {
    this.manualHold = false;
    if (this.destroyed || !this.apiAvailable()) return;
    this.startRecorderIfPossible();
  }

  /**
   * First chunk index whose wall-clock end is at/after `cutoff`, minus optional preroll for decode stability.
   */
  private clipStartChunkIndex(cutoff: number): number {
    const firstKeep = this.chunks.findIndex((c) => c.endedAtMs >= cutoff);
    if (firstKeep <= 0) return 0;
    return Math.max(0, firstKeep - CHUNK_PREROLL_COUNT);
  }

  async saveClip(gameTitle: string, clipDurationMs: number): Promise<RecordingEntry> {
    if (!this.apiAvailable()) throw new Error("Instant replay API unavailable.");
    if (this.manualHold) {
      throw new Error("Replay buffer paused while manual recording is active.");
    }
    const rec = this.mediaRecorder;
    if (rec && rec.state === "recording") {
      try {
        rec.requestData();
      } catch {
        // ignore
      }
      await new Promise((resolve) => window.setTimeout(resolve, 50));
    }

    const video = this.options.getVideo();
    const thumb = video ? makeThumbnailDataUrl(video) : null;
    if (!this.initChunk && this.pendingInitParts.length > 0) {
      this.initChunk = new Blob(this.pendingInitParts, { type: this.mimeType });
      this.pendingInitParts = [];
      this.pendingInitBytes = 0;
    }

    const nowMs = Date.now();
    const windowMs = Math.max(INSTANT_REPLAY_TIMESLICE_MS, clipDurationMs);
    const cutoff = nowMs - windowMs;
    const hasRecentCoverage = this.chunks.some((c) => c.endedAtMs >= cutoff);
    if (!this.initChunk || this.chunks.length === 0 || !hasRecentCoverage) {
      throw new Error("No replay data available yet.");
    }

    const startIdx = this.clipStartChunkIndex(cutoff);
    const includedChunks = this.chunks.slice(startIdx);

    const wallSpanMs =
      includedChunks.length >= 2
        ? includedChunks[includedChunks.length - 1]!.endedAtMs - includedChunks[0]!.endedAtMs
        : 0;
    const minimumWallSpanMs = Math.max(
      INSTANT_REPLAY_TIMESLICE_MS,
      windowMs - INSTANT_REPLAY_TIMESLICE_MS * 4,
    );
    if (wallSpanMs < minimumWallSpanMs) {
      throw new Error(
        `Replay buffer only has ~${Math.round(wallSpanMs / 1000)}s of continuous footage in memory — wait a few seconds while streaming, or increase replay buffer length in Settings.`,
      );
    }

    const mimeType = this.initChunk.type || includedChunks[0]?.blob.type || this.mimeType;
    const clipBlob = new Blob(
      [this.initChunk, ...includedChunks.map((c) => c.blob)],
      { type: mimeType },
    );

    /** Matches requested clip length for probe fallback (main still probes the file). */
    const durationHint = windowMs;

    const saveMeta = {
      mimeType,
      clipDurationMs: windowMs,
      gameTitle,
      thumbnailDataUrl: thumb ?? undefined,
    };

    if (clipBlob.size <= SINGLE_CLIP_INLINE_MAX_BYTES) {
      const entry = await window.openNow.instantReplaySave({
        clip: await clipBlob.arrayBuffer(),
        ...saveMeta,
      });
      return entry;
    }

    const { recordingId } = await window.openNow.beginRecording({ mimeType });
    const bytes = await clipBlob.arrayBuffer();
    const chunkSize = 512 * 1024;
    for (let offset = 0; offset < bytes.byteLength; offset += chunkSize) {
      const end = Math.min(offset + chunkSize, bytes.byteLength);
      await window.openNow.sendRecordingChunk({
        recordingId,
        chunk: bytes.slice(offset, end),
      });
    }

    const entry = await window.openNow.finishRecording({
      recordingId,
      durationMs: durationHint,
      gameTitle,
      thumbnailDataUrl: thumb ?? undefined,
    });
    return entry;
  }

  async destroy(): Promise<void> {
    this.destroyed = true;
    this.manualHold = true;
    await this.stopRecorder();
    this.disposeComposed();
    this.clearBufferInternal();
  }
}
