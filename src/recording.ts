// Recording Manager for OpenNow GFN Client
// Optimized to minimize impact on WebRTC streaming performance

import { invoke } from "@tauri-apps/api/core";

export const RECORDING_QUALITY = {
  low: 1_500_000,    // 1.5 Mbps - minimal impact
  medium: 3_000_000, // 3 Mbps - balanced
  high: 6_000_000,   // 6 Mbps - high quality
} as const;

export type RecordingQualityType = keyof typeof RECORDING_QUALITY;

// Codec preference
// VP8 is default - software encoded, won't interfere with H.264 stream decoding
// H.264 causes stuttering because encoding/decoding compete for same GPU hardware
// AV1 works well on RTX 40 series (separate encoder chip)
export type RecordingCodecType = "vp8" | "h264" | "av1";

// Recording mode
// canvas = captures from video element (decoupled from WebRTC, no stutter)
// stream = direct MediaStream recording (may cause stutter)
export type RecordingMode = "canvas" | "stream";

export interface RecordingState {
  isRecording: boolean;
  isPaused: boolean;
  startTime: number | null;
  duration: number;
  filename: string | null;
}

export type RecordingSavedCallback = (filepath: string, isScreenshot: boolean) => void;

export class RecordingManager {
  private mediaRecorder: MediaRecorder | null = null;
  private recordedChunks: Blob[] = [];
  private stream: MediaStream | null = null;
  private clonedStream: MediaStream | null = null;
  private videoElement: HTMLVideoElement | null = null;
  private gameName = "Unknown";
  private customOutputDir: string | null = null;
  private quality: RecordingQualityType = "medium";
  private codecPreference: RecordingCodecType = "vp8"; // VP8 default - no GPU contention
  private recordingMode: RecordingMode = "canvas"; // Canvas mode by default - no stutter
  private recordingFps = 60; // Match stream FPS for smooth recording
  private _isRecording = false;
  private _isPaused = false;
  private recordingStartTime: number | null = null;
  private durationInterval: ReturnType<typeof setInterval> | null = null;
  private dvrChunks: Blob[] = [];
  private dvrEnabled = false;
  private dvrDuration = 60;
  private dvrCleanupInterval: ReturnType<typeof setInterval> | null = null;
  private onRecordingSaved: RecordingSavedCallback | null = null;
  private onStateChange: ((state: RecordingState) => void) | null = null;

  // Canvas-based recording (decoupled from WebRTC pipeline)
  private canvas: HTMLCanvasElement | OffscreenCanvas | null = null;
  private canvasCtx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D | null = null;
  private canvasStream: MediaStream | null = null;
  private frameRequestId: number | null = null;
  private lastFrameTime = 0;
  private frameInterval: number = 0; // ms between frames

  setStream(stream: MediaStream | null) {
    this.stream = stream;
    // Clean up old cloned stream
    if (this.clonedStream) {
      this.clonedStream.getTracks().forEach(t => t.stop());
      this.clonedStream = null;
    }
  }

  setVideoElement(el: HTMLVideoElement | null) {
    this.videoElement = el;
  }

  setGameName(name: string) { this.gameName = name.replace(/[<>:"/|?*]/g, "_"); }
  setOutputDir(dir: string | null) { this.customOutputDir = dir; }
  setQuality(quality: RecordingQualityType) { this.quality = quality; }
  setCodecPreference(codec: RecordingCodecType) { this.codecPreference = codec; }
  getCodecPreference(): RecordingCodecType { return this.codecPreference; }
  setRecordingMode(mode: RecordingMode) { this.recordingMode = mode; }
  getRecordingMode(): RecordingMode { return this.recordingMode; }
  setRecordingFps(fps: number) { this.recordingFps = Math.max(15, Math.min(60, fps)); }
  getRecordingFps(): number { return this.recordingFps; }
  onSaved(cb: RecordingSavedCallback) { this.onRecordingSaved = cb; }
  onStateChanged(cb: (s: RecordingState) => void) { this.onStateChange = cb; }

  getState(): RecordingState {
    return {
      isRecording: this._isRecording,
      isPaused: this._isPaused,
      startTime: this.recordingStartTime,
      duration: this.recordingStartTime ? Math.floor((Date.now() - this.recordingStartTime) / 1000) : 0,
      filename: null,
    };
  }

  get isRecording() { return this._isRecording; }
  get duration() { return this.recordingStartTime ? Math.floor((Date.now() - this.recordingStartTime) / 1000) : 0; }
  formatDuration(s: number) { return Math.floor(s / 60).toString().padStart(2, "0") + ":" + (s % 60).toString().padStart(2, "0"); }

  private genFilename(pre: string, ext: string) {
    const ts = new Date().toISOString().replace(/[:.]/g, "-").replace("T", "_").slice(0, 19);
    return "OpenNow_" + this.gameName + "_" + pre + ts + "." + ext;
  }

  // Get MIME type based on user preference
  // VP8 is default - software encoded, won't interfere with stream playback
  // H.264 causes stuttering because it competes with stream decoding for GPU
  // AV1 works on RTX 40+ (separate encoder chip)
  private getMime(): string {
    const vp8Codecs = [
      "video/webm;codecs=vp8,opus",
      "video/webm;codecs=vp8",
      "video/webm",
    ];

    const h264Codecs = [
      "video/webm;codecs=h264,opus",
      "video/webm;codecs=h264",
      "video/mp4;codecs=h264,aac",
      "video/mp4;codecs=avc1.42E01E,mp4a.40.2",
      "video/mp4",
    ];

    const av1Codecs = [
      "video/webm;codecs=av1,opus",
      "video/mp4;codecs=av01.0.04M.08",
    ];

    // Build codec list based on user preference
    let codecs: string[];
    if (this.codecPreference === "av1") {
      // AV1 first (RTX 40+), then VP8 fallback
      codecs = [...av1Codecs, ...vp8Codecs];
    } else if (this.codecPreference === "h264") {
      // H.264 first (may cause stuttering!), then VP8 fallback
      codecs = [...h264Codecs, ...vp8Codecs];
    } else {
      // VP8 first (default) - software encoded, no GPU contention
      codecs = [...vp8Codecs, ...h264Codecs];
    }

    for (const codec of codecs) {
      if (MediaRecorder.isTypeSupported(codec)) {
        console.log("Recording codec:", codec);
        return codec;
      }
    }
    return "video/webm";
  }

  // Get file extension based on mime type
  private getFileExtension(): string {
    const mime = this.getMime();
    return mime.startsWith("video/mp4") ? "mp4" : "webm";
  }

  // Get stream for recording - canvas mode decouples from WebRTC pipeline
  private getRecordingStream(): MediaStream | null {
    // Canvas mode: capture from video element (decoupled, no stutter)
    if (this.recordingMode === "canvas" && this.videoElement) {
      return this.createCanvasStream();
    }

    // Stream mode: clone the MediaStream directly (may cause stutter)
    if (!this.stream) return null;

    if (!this.clonedStream) {
      try {
        this.clonedStream = this.stream.clone();
      } catch {
        this.clonedStream = new MediaStream();
        this.stream.getTracks().forEach(track => {
          this.clonedStream!.addTrack(track.clone());
        });
      }
    }
    return this.clonedStream;
  }

  // Create a canvas-based stream that captures from the video element
  // This completely decouples recording from the WebRTC decode pipeline
  private createCanvasStream(): MediaStream | null {
    if (!this.videoElement || !this.videoElement.videoWidth) {
      console.warn("[Recording] Video element not ready for canvas capture");
      return null;
    }

    const width = this.videoElement.videoWidth;
    const height = this.videoElement.videoHeight;

    // Use regular canvas for captureStream compatibility
    this.canvas = document.createElement("canvas");
    this.canvas.width = width;
    this.canvas.height = height;

    // Get context with performance optimizations
    this.canvasCtx = this.canvas.getContext("2d", {
      alpha: false,
      desynchronized: true, // Don't sync with compositor - reduces latency
      willReadFrequently: false, // We're writing, not reading
    });

    if (!this.canvasCtx) {
      console.error("[Recording] Failed to create canvas context");
      return null;
    }

    // Disable image smoothing for faster draws
    this.canvasCtx.imageSmoothingEnabled = false;

    // Create stream from canvas - let it capture at native rate
    // The FPS limiting happens in our frame loop
    this.canvasStream = this.canvas.captureStream(0); // 0 = manual frame capture

    // Add audio track from original stream if available
    if (this.stream) {
      const audioTracks = this.stream.getAudioTracks();
      audioTracks.forEach(track => {
        this.canvasStream!.addTrack(track.clone());
      });
    }

    // Calculate frame interval
    this.frameInterval = 1000 / this.recordingFps;
    this.lastFrameTime = 0;

    // Start frame capture loop using requestAnimationFrame (smoother than setInterval)
    this.startCanvasCapture();

    console.log(`[Recording] Canvas capture started at ${this.recordingFps}fps, ${width}x${height}`);
    return this.canvasStream;
  }

  // Capture frames using requestAnimationFrame for smooth, jank-free capture
  private startCanvasCapture() {
    const captureFrame = (timestamp: number) => {
      if (!this.canvasCtx || !this.videoElement || !this.canvas) {
        return; // Stop if resources cleaned up
      }

      // Throttle to target FPS
      const elapsed = timestamp - this.lastFrameTime;
      if (elapsed >= this.frameInterval) {
        this.lastFrameTime = timestamp - (elapsed % this.frameInterval);

        // Check if video dimensions changed
        if (this.canvas.width !== this.videoElement.videoWidth ||
            this.canvas.height !== this.videoElement.videoHeight) {
          this.canvas.width = this.videoElement.videoWidth;
          this.canvas.height = this.videoElement.videoHeight;
        }

        // Draw current video frame to canvas
        this.canvasCtx.drawImage(this.videoElement, 0, 0);

        // Request new frame from canvas stream
        const videoTrack = this.canvasStream?.getVideoTracks()[0];
        if (videoTrack && 'requestFrame' in videoTrack) {
          (videoTrack as any).requestFrame();
        }
      }

      // Continue loop
      this.frameRequestId = requestAnimationFrame(captureFrame);
    };

    // Start the loop
    this.frameRequestId = requestAnimationFrame(captureFrame);
  }

  private stopCanvasCapture() {
    if (this.frameRequestId !== null) {
      cancelAnimationFrame(this.frameRequestId);
      this.frameRequestId = null;
    }
    if (this.canvasStream) {
      this.canvasStream.getTracks().forEach(t => t.stop());
      this.canvasStream = null;
    }
    this.canvas = null;
    this.canvasCtx = null;
    this.lastFrameTime = 0;
  }

  async startRecording(): Promise<boolean> {
    if (this._isRecording) return false;

    const recordingStream = this.getRecordingStream();
    if (!recordingStream) return false;

    // Try codecs in order based on user preference
    const codecsToTry = this.getCodecPriorityList();
    let selectedMime: string | null = null;

    for (const mime of codecsToTry) {
      try {
        // Test if MediaRecorder can actually be created with this codec
        const testRecorder = new MediaRecorder(recordingStream, {
          mimeType: mime,
          videoBitsPerSecond: RECORDING_QUALITY[this.quality],
        });
        testRecorder.stop();
        selectedMime = mime;
        console.log("[Recording] Successfully initialized with codec:", mime);
        break;
      } catch (e) {
        console.warn(`[Recording] Codec ${mime} failed:`, e);
        // Continue to next codec
      }
    }

    if (!selectedMime) {
      console.error("[Recording] No working codec found");
      return false;
    }

    try {
      this.mediaRecorder = new MediaRecorder(recordingStream, {
        mimeType: selectedMime,
        videoBitsPerSecond: RECORDING_QUALITY[this.quality],
      });

      this.recordedChunks = [];

      this.mediaRecorder.ondataavailable = e => {
        if (e.data.size > 0) this.recordedChunks.push(e.data);
      };

      this.mediaRecorder.onstop = () => this.saveRecording();

      // Use 5 second timeslice to reduce encoder pressure
      // Shorter timeslices cause more frequent encoding flushes which stutter playback
      this.mediaRecorder.start(5000);

      this._isRecording = true;
      this.recordingStartTime = Date.now();
      this.durationInterval = setInterval(() => this.notifyStateChange(), 1000);
      this.notifyStateChange();
      return true;
    } catch (e) {
      console.error("Failed to start recording:", e);
      return false;
    }
  }

  // Get list of codecs to try in priority order based on user preference
  private getCodecPriorityList(): string[] {
    const vp8Codecs = [
      "video/webm;codecs=vp8,opus",
      "video/webm;codecs=vp8",
      "video/webm",
    ];

    const h264Codecs = [
      "video/webm;codecs=h264,opus",
      "video/webm;codecs=h264",
      "video/mp4;codecs=avc1.42E01E,mp4a.40.2",
    ];

    const av1Codecs = [
      "video/webm;codecs=av1,opus",
      "video/mp4;codecs=av01.0.04M.08",
    ];

    let codecs: string[];
    if (this.codecPreference === "av1") {
      codecs = [...av1Codecs, ...vp8Codecs];
    } else if (this.codecPreference === "h264") {
      codecs = [...h264Codecs, ...vp8Codecs];
    } else {
      // VP8 default
      codecs = [...vp8Codecs, ...h264Codecs];
    }

    return codecs.filter(mime => MediaRecorder.isTypeSupported(mime));
  }

  async stopRecording(): Promise<boolean> {
    if (!this._isRecording || !this.mediaRecorder) return false;

    this.mediaRecorder.stop();
    this._isRecording = false;

    // Stop canvas capture if active
    this.stopCanvasCapture();

    if (this.durationInterval) {
      clearInterval(this.durationInterval);
      this.durationInterval = null;
    }

    this.notifyStateChange();
    return true;
  }

  async toggleRecording() {
    return this._isRecording ? this.stopRecording() : this.startRecording();
  }

  private async saveRecording() {
    if (!this.recordedChunks.length) return;

    const mimeType = this.getMime();
    const ext = this.getFileExtension();
    const blob = new Blob(this.recordedChunks, { type: mimeType });

    // Use requestIdleCallback to defer heavy work, or setTimeout as fallback
    const deferredSave = async () => {
      const data = Array.from(new Uint8Array(await blob.arrayBuffer()));
      const fp = await invoke<string>("save_recording", {
        data,
        filename: this.genFilename("", ext),
        customDir: this.customOutputDir
      });
      this.recordedChunks = [];
      this.recordingStartTime = null;
      if (this.onRecordingSaved) this.onRecordingSaved(fp, false);
    };

    if ('requestIdleCallback' in window) {
      (window as any).requestIdleCallback(deferredSave, { timeout: 5000 });
    } else {
      setTimeout(deferredSave, 100);
    }
  }

  async takeScreenshot(vid: HTMLVideoElement): Promise<boolean> {
    if (!vid || !vid.videoWidth) return false;

    // Use OffscreenCanvas if available for better performance
    let canvas: HTMLCanvasElement | OffscreenCanvas;
    let ctx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D | null;

    if (typeof OffscreenCanvas !== 'undefined') {
      canvas = new OffscreenCanvas(vid.videoWidth, vid.videoHeight);
      ctx = canvas.getContext('2d');
    } else {
      canvas = document.createElement("canvas");
      canvas.width = vid.videoWidth;
      canvas.height = vid.videoHeight;
      ctx = canvas.getContext("2d");
    }

    if (!ctx) return false;
    ctx.drawImage(vid, 0, 0);

    let blob: Blob | null;
    if (canvas instanceof OffscreenCanvas) {
      blob = await canvas.convertToBlob({ type: "image/png" });
    } else {
      blob = await new Promise<Blob | null>(r => canvas.toBlob(r, "image/png"));
    }

    if (!blob) return false;

    // Defer to avoid blocking
    const data = Array.from(new Uint8Array(await blob.arrayBuffer()));
    const fp = await invoke<string>("save_screenshot", {
      data,
      filename: this.genFilename("", "png"),
      customDir: this.customOutputDir
    });

    if (this.onRecordingSaved) this.onRecordingSaved(fp, true);
    return true;
  }

  enableInstantReplay(dur = 60): boolean {
    if (this.dvrEnabled) return false;

    const recordingStream = this.getRecordingStream();
    if (!recordingStream) return false;

    this.dvrDuration = dur;
    this.dvrChunks = [];

    // If already recording, share the chunks instead of creating another recorder
    if (this._isRecording && this.mediaRecorder) {
      // Just enable DVR mode - we'll use the main recorder's chunks
      this.dvrEnabled = true;
      return true;
    }

    // Create a dedicated DVR recorder with lower quality for less impact
    try {
      const dvrRecorder = new MediaRecorder(recordingStream, {
        mimeType: this.getMime(),
        videoBitsPerSecond: RECORDING_QUALITY.low, // Use low quality for DVR to reduce impact
      });

      dvrRecorder.ondataavailable = e => {
        if (e.data.size > 0) this.dvrChunks.push(e.data);
      };

      // Use 5 second chunks for DVR too
      dvrRecorder.start(5000);

      // Cleanup old chunks periodically - check every 5 seconds
      this.dvrCleanupInterval = setInterval(() => {
        // Each chunk is ~5 seconds, so keep (duration/5) chunks
        const maxChunks = Math.ceil(this.dvrDuration / 5);
        while (this.dvrChunks.length > maxChunks) {
          this.dvrChunks.shift();
        }
      }, 5000);

      this.dvrEnabled = true;
      // Store recorder reference for cleanup
      (this as any)._dvrRecorder = dvrRecorder;
      return true;
    } catch (e) {
      console.error("Failed to enable instant replay:", e);
      return false;
    }
  }

  disableInstantReplay() {
    if (!this.dvrEnabled) return;

    const dvrRecorder = (this as any)._dvrRecorder as MediaRecorder | undefined;
    if (dvrRecorder && dvrRecorder.state !== "inactive") {
      dvrRecorder.stop();
    }
    (this as any)._dvrRecorder = null;

    if (this.dvrCleanupInterval) {
      clearInterval(this.dvrCleanupInterval);
      this.dvrCleanupInterval = null;
    }

    this.dvrChunks = [];
    this.dvrEnabled = false;
  }

  get isInstantReplayEnabled() { return this.dvrEnabled; }

  async saveInstantReplay(): Promise<boolean> {
    if (!this.dvrEnabled) return false;

    // If we're sharing with main recorder, use those chunks
    const chunks = this.dvrChunks.length > 0 ? this.dvrChunks : this.recordedChunks;
    if (!chunks.length) return false;

    const mimeType = this.getMime();
    const ext = this.getFileExtension();
    const blob = new Blob([...chunks], { type: mimeType });

    // Defer heavy work
    const deferredSave = async () => {
      const data = Array.from(new Uint8Array(await blob.arrayBuffer()));
      const fp = await invoke<string>("save_recording", {
        data,
        filename: this.genFilename("Replay_", ext),
        customDir: this.customOutputDir
      });
      if (this.onRecordingSaved) this.onRecordingSaved(fp, false);
    };

    if ('requestIdleCallback' in window) {
      (window as any).requestIdleCallback(deferredSave, { timeout: 5000 });
    } else {
      setTimeout(deferredSave, 100);
    }

    return true;
  }

  private notifyStateChange() {
    if (this.onStateChange) this.onStateChange(this.getState());
  }

  // Expose the current codec for UI display
  getCurrentCodec(): string {
    return this.getMime();
  }

  dispose() {
    this.stopRecording();
    this.disableInstantReplay();
    this.stopCanvasCapture();

    // Clean up cloned stream
    if (this.clonedStream) {
      this.clonedStream.getTracks().forEach(t => t.stop());
      this.clonedStream = null;
    }

    this.stream = null;
    this.videoElement = null;
  }
}

let inst: RecordingManager | null = null;
export function getRecordingManager() {
  if (!inst) inst = new RecordingManager();
  return inst;
}

export async function openRecordingsFolder(d?: string) {
  await invoke("open_recordings_folder", { customDir: d || null });
}

export async function getRecordingsDir(d?: string) {
  return invoke<string>("get_recordings_dir", { customDir: d || null });
}

// Test all codecs and return support status
export interface CodecSupport {
  codec: string;
  supported: boolean;
  description: string;
  hwAccelerated: boolean;
}

export function testCodecSupport(): CodecSupport[] {
  const codecs = [
    // AV1 - best compression, modern GPUs (RTX 40, Intel Arc, AMD RX 7000)
    { codec: "video/webm;codecs=av1,opus", description: "AV1 + Opus (WebM) - Best Quality", hwAccelerated: true },
    { codec: "video/mp4;codecs=av01.0.04M.08", description: "AV1 (MP4)", hwAccelerated: true },
    // H.264 - widely supported, hardware accelerated
    { codec: "video/webm;codecs=h264,opus", description: "H.264 + Opus (WebM) - Best Compatibility", hwAccelerated: true },
    { codec: "video/webm;codecs=h264", description: "H.264 (WebM)", hwAccelerated: true },
    { codec: "video/mp4;codecs=h264,aac", description: "H.264 + AAC (MP4)", hwAccelerated: true },
    { codec: "video/mp4;codecs=avc1.42E01E,mp4a.40.2", description: "H.264 Baseline (MP4)", hwAccelerated: true },
    { codec: "video/mp4", description: "MP4 (generic)", hwAccelerated: true },
    // VP9/VP8 - software encoded fallbacks
    { codec: "video/webm;codecs=vp9,opus", description: "VP9 + Opus (WebM)", hwAccelerated: false },
    { codec: "video/webm;codecs=vp9", description: "VP9 (WebM)", hwAccelerated: false },
    { codec: "video/webm;codecs=vp8,opus", description: "VP8 + Opus (WebM)", hwAccelerated: false },
    { codec: "video/webm;codecs=vp8", description: "VP8 (WebM)", hwAccelerated: false },
    { codec: "video/webm", description: "WebM (generic)", hwAccelerated: false },
  ];

  return codecs.map(c => ({
    ...c,
    supported: MediaRecorder.isTypeSupported(c.codec),
  }));
}

// Get the currently selected codec
export function getCurrentCodec(): string {
  return getRecordingManager().getCurrentCodec();
}
