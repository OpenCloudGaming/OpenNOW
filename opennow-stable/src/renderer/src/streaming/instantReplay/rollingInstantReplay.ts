import type { RecordingEntry } from "@shared/gfn";
import { buildComposedRecordingStream, type ComposedRecordingResources } from "../composedRecordingStream";

export const INSTANT_REPLAY_TIMESLICE_MS = 1000;

const IR_MIME_CANDIDATES = ["video/webm;codecs=vp8,opus", "video/webm;codecs=vp8", "video/webm"];

/** Leading `dataavailable` blobs can be tiny; merge until we have a plausible WebM prefix (EBML + segment start). */
const MIN_INIT_ACCUM_BYTES = 512;

function pickInstantReplayMimeType(): string {
  return IR_MIME_CANDIDATES.find((m) => MediaRecorder.isTypeSupported(m)) ?? "video/webm";
}

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
 * Rolling buffer: one `MediaRecorder` emits timesliced blobs. Mutually exclusive with manual recording.
 * Saves concatenate `initChunk` + every retained blob (single WebM stream). Buffer trimming rolls the recorder.
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
  /** When true, `onstop` must not call `startRecorderIfPossible` (rolling restart or similar). */
  private suppressAutoRestart = false;
  private restartScheduled = false;
  /** Wall time when the current recorder session started (`rec.start`); used for duration metadata. */
  private bufferSessionStartMs: number | null = null;
  private readonly maxBufferBytes = 256 * 1024 * 1024; // 256MB safety cap
  private readonly mimeType = pickInstantReplayMimeType();

  constructor(options: RollingInstantReplayOptions) {
    this.options = options;
  }

  private apiAvailable(): boolean {
    return (
      typeof window.openNow?.beginRecording === "function" &&
      typeof window.openNow?.sendRecordingChunk === "function" &&
      typeof window.openNow?.finishRecording === "function"
    );
  }

  async start(): Promise<void> {
    if (this.destroyed || !this.apiAvailable()) return;
    this.clearBuffer();
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

  /**
   * WebM chunks from a single MediaRecorder are one continuous bitstream: dropping blobs from the
   * middle/tail without a new init segment corrupts the file. When the buffer window or byte cap is
   * exceeded, restart capture so `initChunk` + `chunks` stay contiguous.
   */
  private pruneOrRollAfterAppend(nowMs: number): void {
    const cutoff = nowMs - Math.max(1_000, this.options.bufferWindowMs);
    const overTime = this.chunks.length > 0 && this.chunks[0]!.endedAtMs < cutoff;
    const overBytes = this.bytesInBuffer > this.maxBufferBytes;
    if (overTime || overBytes) {
      this.scheduleRollingRestart();
    }
  }

  private scheduleRollingRestart(): void {
    if (this.restartScheduled || this.destroyed || this.manualHold) return;
    this.restartScheduled = true;
    queueMicrotask(() => {
      this.restartScheduled = false;
      void this.restartRollingCapture();
    });
  }

  private async restartRollingCapture(): Promise<void> {
    if (this.destroyed || this.manualHold) return;
    this.suppressAutoRestart = true;
    try {
      await this.stopRecorder();
      this.clearBuffer();
    } finally {
      this.suppressAutoRestart = false;
    }
    this.startRecorderIfPossible();
  }

  private clearBuffer(): void {
    this.chunks = [];
    this.bytesInBuffer = 0;
    this.initChunk = null;
    this.pendingInitParts = [];
    this.pendingInitBytes = 0;
    this.bufferSessionStartMs = null;
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
      this.pruneOrRollAfterAppend(nowMs);
    };
    rec.onerror = () => {
      this.mediaRecorder = null;
    };
    rec.onstop = () => {
      this.mediaRecorder = null;
      if (!this.destroyed && !this.manualHold && !this.suppressAutoRestart) {
        this.startRecorderIfPossible();
      }
    };
    try {
      rec.start(INSTANT_REPLAY_TIMESLICE_MS);
      this.bufferSessionStartMs = Date.now();
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

  /**
   * Stop instant replay capture so manual recording can use the same video tracks.
   */
  async pauseForManualRecording(): Promise<void> {
    this.manualHold = true;
    await this.stopRecorder();
    this.disposeComposed();
    // Recorder restart can change container initialization; start fresh after resume.
    this.clearBuffer();
  }

  /** Resume after manual recording stops. */
  async resumeAfterManualRecording(): Promise<void> {
    this.manualHold = false;
    if (this.destroyed || !this.apiAvailable()) return;
    this.startRecorderIfPossible();
  }

  /**
   * Save clip from recent in-memory chunks.
   */
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
        // ignore - continue with available chunks
      }
      await new Promise((resolve) => window.setTimeout(resolve, 30));
    }

    const video = this.options.getVideo();
    const thumb = video ? makeThumbnailDataUrl(video) : null;
    if (!this.initChunk && this.pendingInitParts.length > 0) {
      this.initChunk = new Blob(this.pendingInitParts, { type: this.mimeType });
      this.pendingInitParts = [];
      this.pendingInitBytes = 0;
    }
    const nowMs = Date.now();
    const cutoff = nowMs - Math.max(1_000, clipDurationMs);
    const hasRecentCoverage = this.chunks.some((chunk) => chunk.endedAtMs >= cutoff);
    if (!this.initChunk || this.chunks.length === 0 || !hasRecentCoverage) {
      throw new Error("No replay data available yet.");
    }
    const mimeType = this.initChunk.type || this.chunks[0]?.blob.type || this.mimeType;
    // Must concatenate every in-memory part: WebM is one stream; omitting chunks corrupts the file.
    const clipBlob = new Blob(
      [this.initChunk, ...this.chunks.map((chunk) => chunk.blob)],
      { type: mimeType },
    );

    // Metadata for the library row: derive from timeslice count + session wall time (not chunk arrival deltas).
    const fromSliceCount = (this.chunks.length + 1) * INSTANT_REPLAY_TIMESLICE_MS;
    const fromSessionWall =
      this.bufferSessionStartMs != null ? nowMs - this.bufferSessionStartMs : fromSliceCount;
    const durationMs = Math.max(INSTANT_REPLAY_TIMESLICE_MS, fromSliceCount, fromSessionWall);

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

    return window.openNow.finishRecording({
      recordingId,
      durationMs,
      gameTitle,
      thumbnailDataUrl: thumb ?? undefined,
    });
  }

  async destroy(): Promise<void> {
    this.destroyed = true;
    this.manualHold = true;
    await this.stopRecorder();
    this.disposeComposed();
    this.clearBuffer();
  }
}
