import type {
  IceCandidatePayload,
  IceServer,
  SessionInfo,
  VideoCodec,
} from "@shared/gfn";

import {
  InputEncoder,
  mapKeyboardEvent,
  modifierFlags,
  toMouseButton,
  mapGamepadButtons,
  readGamepadAxes,
  normalizeToInt16,
  normalizeToUint8,
  GAMEPAD_MAX_CONTROLLERS,
  type GamepadInput,
} from "./inputProtocol";
import {
  buildNvstSdp,
  extractIceCredentials,
  extractIceUfragFromOffer,
  extractPublicIp,
  fixServerIp,
  preferCodec,
} from "./sdp";

interface OfferSettings {
  codec: VideoCodec;
  resolution: string;
  fps: number;
  maxBitrateKbps: number;
}

export interface StreamDiagnostics {
  // Connection state
  connectionState: RTCPeerConnectionState | "closed";
  inputReady: boolean;
  connectedGamepads: number;

  // Video stats
  resolution: string;
  codec: string;
  isHdr: boolean;
  bitrateKbps: number;
  decodeFps: number;
  renderFps: number;

  // Network stats
  packetsLost: number;
  packetsReceived: number;
  packetLossPercent: number;
  jitterMs: number;
  rttMs: number;

  // Frame counters
  framesReceived: number;
  framesDecoded: number;
  framesDropped: number;

  // Timing
  decodeTimeMs: number;
  renderTimeMs: number;
  jitterBufferDelayMs: number;

  // System info
  gpuType: string;
  serverRegion: string;
}

interface ClientOptions {
  videoElement: HTMLVideoElement;
  audioElement: HTMLAudioElement;
  onLog: (line: string) => void;
  onStats?: (stats: StreamDiagnostics) => void;
}

function timestampUs(): bigint {
  return BigInt(Math.floor(performance.now() * 1000));
}

function parseResolution(resolution: string): { width: number; height: number } {
  const [rawWidth, rawHeight] = resolution.split("x");
  const width = Number.parseInt(rawWidth ?? "", 10);
  const height = Number.parseInt(rawHeight ?? "", 10);

  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) {
    return { width: 1920, height: 1080 };
  }

  return { width, height };
}

function toRtcIceServers(iceServers: IceServer[]): RTCIceServer[] {
  return iceServers.map((server) => ({
    urls: server.urls,
    username: server.username,
    credential: server.credential,
  }));
}

async function toBytes(data: string | Blob | ArrayBuffer): Promise<Uint8Array> {
  if (typeof data === "string") {
    return new TextEncoder().encode(data);
  }
  if (data instanceof ArrayBuffer) {
    return new Uint8Array(data);
  }
  const arrayBuffer = await data.arrayBuffer();
  return new Uint8Array(arrayBuffer);
}

/**
 * Detect GPU type using browser APIs
 * Uses WebGL renderer string to identify GPU vendor/model
 */
function detectGpuType(): string {
  try {
    const canvas = document.createElement("canvas");
    const gl = canvas.getContext("webgl2") || canvas.getContext("webgl");
    if (!gl) {
      return "Unknown";
    }

    const debugInfo = gl.getExtension("WEBGL_debug_renderer_info");
    if (debugInfo) {
      const vendor = gl.getParameter(debugInfo.UNMASKED_VENDOR_WEBGL);
      const renderer = gl.getParameter(debugInfo.UNMASKED_RENDERER_WEBGL);

      // Clean up renderer string - extract main GPU name
      let gpuName = renderer;

      // Remove common prefixes/suffixes for cleaner display
      gpuName = gpuName
        .replace(/\(R\)/g, "")
        .replace(/\(TM\)/g, "")
        .replace(/NVIDIA /i, "")
        .replace(/AMD /i, "")
        .replace(/Intel /i, "")
        .replace(/Microsoft Corporation - /i, "")
        .replace(/D3D12 /i, "")
        .replace(/Direct3D11 /i, "")
        .replace(/OpenGL Engine/i, "")
        .trim();

      // Limit length
      if (gpuName.length > 30) {
        gpuName = gpuName.substring(0, 27) + "...";
      }

      return gpuName || vendor || "Unknown";
    }
    return "Unknown";
  } catch {
    return "Unknown";
  }
}

/**
 * Extract codec name from codecId string (e.g., "VP09" -> "VP9", "AV1X" -> "AV1")
 */
function normalizeCodecName(codecId: string): string {
  const upper = codecId.toUpperCase();

  if (upper.startsWith("H264") || upper === "H264") {
    return "H264";
  }
  if (upper.startsWith("H265") || upper === "H265" || upper.startsWith("HEVC")) {
    return "H265";
  }
  if (upper.startsWith("AV1")) {
    return "AV1";
  }
  if (upper.startsWith("VP9") || upper.startsWith("VP09")) {
    return "VP9";
  }
  if (upper.startsWith("VP8")) {
    return "VP8";
  }

  return codecId;
}

export class GfnWebRtcClient {
  private readonly videoStream = new MediaStream();
  private readonly audioStream = new MediaStream();
  private readonly inputEncoder = new InputEncoder();

  private pc: RTCPeerConnection | null = null;
  private reliableInputChannel: RTCDataChannel | null = null;
  private mouseInputChannel: RTCDataChannel | null = null;

  private inputReady = false;
  private inputProtocolVersion = 2;
  private heartbeatTimer: number | null = null;
  private mouseFlushTimer: number | null = null;
  private statsTimer: number | null = null;
  private gamepadPollTimer: number | null = null;
  private pendingMouseDx = 0;
  private pendingMouseDy = 0;
  private inputCleanup: Array<() => void> = [];
  private queuedCandidates: RTCIceCandidateInit[] = [];

  // Input mode: auto-switches between mouse+keyboard and gamepad
  // When gamepad has activity, mouse/keyboard are suppressed (and vice versa)
  private activeInputMode: "mkb" | "gamepad" = "mkb";
  // Timestamp of last gamepad state change — used for mode-switch lockout
  private lastGamepadActivityMs = 0;
  // Timestamp of last gamepad packet sent — used for keepalive
  private lastGamepadSendMs = 0;
  // Gamepad keepalive interval: resend last state every 100ms to keep server controller alive
  private static readonly GAMEPAD_KEEPALIVE_MS = 100;
  // How long to wait after last gamepad activity before allowing switch to mkb (seconds)
  // Prevents accidental key/mouse events from disrupting controller gameplay
  private static readonly GAMEPAD_MODE_LOCKOUT_MS = 3000;

  // Gamepad bitmap: tracks which gamepads are connected, matching official client's this.nu field.
  // Bit i (0-3) = gamepad i is connected. Sent in every gamepad packet at offset 8.
  private gamepadBitmap = 0;

  // Stats tracking
  private lastStatsSample: {
    bytesReceived: number;
    framesReceived: number;
    framesDecoded: number;
    framesDropped: number;
    packetsReceived: number;
    packetsLost: number;
    atMs: number;
  } | null = null;
  private renderFpsCounter = { frames: 0, lastUpdate: 0, fps: 0 };
  private connectedGamepads: Set<number> = new Set();
  private previousGamepadStates: Map<number, GamepadInput> = new Map();

  // Track currently pressed keys (VK codes) for synthetic Escape detection
  private pressedKeys: Set<number> = new Set();
  // Video element reference for pointer lock re-acquisition
  private videoElement: HTMLVideoElement | null = null;
  // Timer for synthetic Escape on pointer lock loss
  private pointerLockEscapeTimer: number | null = null;

  // Stream info
  private currentCodec = "";
  private currentResolution = "";
  private isHdr = false;
  private serverRegion = "";
  private gpuType = "";

  private diagnostics: StreamDiagnostics = {
    connectionState: "closed",
    inputReady: false,
    connectedGamepads: 0,
    resolution: "",
    codec: "",
    isHdr: false,
    bitrateKbps: 0,
    decodeFps: 0,
    renderFps: 0,
    packetsLost: 0,
    packetsReceived: 0,
    packetLossPercent: 0,
    jitterMs: 0,
    rttMs: 0,
    framesReceived: 0,
    framesDecoded: 0,
    framesDropped: 0,
    decodeTimeMs: 0,
    renderTimeMs: 0,
    jitterBufferDelayMs: 0,
    gpuType: "",
    serverRegion: "",
  };

  constructor(private readonly options: ClientOptions) {
    options.videoElement.srcObject = this.videoStream;
    options.audioElement.srcObject = this.audioStream;

    // Configure video element for lowest latency playback
    this.configureVideoElementForLowLatency(options.videoElement);

    // Detect GPU once on construction
    this.gpuType = detectGpuType();
    this.diagnostics.gpuType = this.gpuType;
  }

  /**
   * Configure the video element for minimum latency streaming.
   * Sets attributes that reduce internal buffering and prioritize
   * immediate frame display over smooth playback.
   */
  private configureVideoElementForLowLatency(video: HTMLVideoElement): void {
    // disableRemotePlayback prevents Chrome from offering cast/remote playback
    // which can add buffering layers
    video.disableRemotePlayback = true;

    // Ensure no preload buffering (we get frames via WebRTC, not a URL)
    video.preload = "none";

    // Set playback rate to 1.0 explicitly (some browsers may adjust)
    video.playbackRate = 1.0;
    video.defaultPlaybackRate = 1.0;

    this.log("Video element configured for low-latency playback");
  }

  /**
   * Configure an RTCRtpReceiver for minimum jitter buffer delay.
   * 
   * jitterBufferTarget controls how long Chrome holds decoded frames before
   * displaying them. For cloud gaming we want the smallest possible value.
   * 
   * WARNING: Setting to exactly 0 can cause stutters at high resolutions
   * (per selkies-project findings). We use a very small non-zero value
   * that allows the jitter buffer to absorb minimal network variance
   * while keeping latency extremely low.
   * 
   * The playoutDelayHint property is the older name for jitterBufferTarget
   * in some Chrome versions — we set both for compatibility.
   */
  private configureReceiverForLowLatency(receiver: RTCRtpReceiver, kind: string): void {
    try {
      // Video: 20ms — roughly 1-2 frames at 60fps. Small enough for gaming,
      // large enough to absorb a single packet retransmission.
      // Audio: 10ms — tighter than video since audio frames are small.
      const targetMs = kind === "video" ? 20 : 10;

      if ("jitterBufferTarget" in receiver) {
        (receiver as unknown as Record<string, unknown>).jitterBufferTarget = targetMs;
        this.log(`${kind} receiver: jitterBufferTarget set to ${targetMs}ms`);
      }

      // Legacy property name (Chrome <M114 approximately)
      if ("playoutDelayHint" in receiver) {
        (receiver as unknown as Record<string, unknown>).playoutDelayHint = targetMs / 1000;
        this.log(`${kind} receiver: playoutDelayHint set to ${targetMs / 1000}s`);
      }
    } catch (error) {
      this.log(`Warning: could not set ${kind} jitter buffer target: ${String(error)}`);
    }
  }

  private log(message: string): void {
    this.options.onLog(message);
  }

  private emitStats(): void {
    if (this.options.onStats) {
      this.options.onStats({ ...this.diagnostics });
    }
  }

  private resetDiagnostics(): void {
    this.lastStatsSample = null;
    this.currentCodec = "";
    this.currentResolution = "";
    this.isHdr = false;
    this.diagnostics = {
      connectionState: this.pc?.connectionState ?? "closed",
      inputReady: false,
      connectedGamepads: 0,
      resolution: "",
      codec: "",
      isHdr: false,
      bitrateKbps: 0,
      decodeFps: 0,
      renderFps: 0,
      packetsLost: 0,
      packetsReceived: 0,
      packetLossPercent: 0,
      jitterMs: 0,
      rttMs: 0,
      framesReceived: 0,
      framesDecoded: 0,
      framesDropped: 0,
      decodeTimeMs: 0,
      renderTimeMs: 0,
      jitterBufferDelayMs: 0,
      gpuType: this.gpuType,
      serverRegion: this.serverRegion,
    };
    this.emitStats();
  }

  private resetInputState(): void {
    this.inputReady = false;
    this.inputProtocolVersion = 2;
    this.inputEncoder.setProtocolVersion(2);
    this.diagnostics.inputReady = false;
    this.emitStats();
  }

  private closeDataChannels(): void {
    this.reliableInputChannel?.close();
    this.mouseInputChannel?.close();
    this.reliableInputChannel = null;
    this.mouseInputChannel = null;
  }

  private clearTimers(): void {
    if (this.heartbeatTimer !== null) {
      window.clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
    if (this.mouseFlushTimer !== null) {
      window.clearInterval(this.mouseFlushTimer);
      this.mouseFlushTimer = null;
    }
    if (this.statsTimer !== null) {
      window.clearInterval(this.statsTimer);
      this.statsTimer = null;
    }
    if (this.gamepadPollTimer !== null) {
      window.clearInterval(this.gamepadPollTimer);
      this.gamepadPollTimer = null;
    }
  }

  private setupStatsPolling(): void {
    if (this.statsTimer !== null) {
      window.clearInterval(this.statsTimer);
    }

    this.statsTimer = window.setInterval(() => {
      void this.collectStats();
    }, 1000);
  }

  private updateRenderFps(): void {
    const now = performance.now();
    this.renderFpsCounter.frames++;

    // Update FPS every 500ms
    if (now - this.renderFpsCounter.lastUpdate >= 500) {
      const elapsed = (now - this.renderFpsCounter.lastUpdate) / 1000;
      this.renderFpsCounter.fps = Math.round(this.renderFpsCounter.frames / elapsed);
      this.renderFpsCounter.frames = 0;
      this.renderFpsCounter.lastUpdate = now;
      this.diagnostics.renderFps = this.renderFpsCounter.fps;
    }
  }

  private async collectStats(): Promise<void> {
    if (!this.pc) {
      return;
    }

    const report = await this.pc.getStats();
    const now = performance.now();
    let inboundVideo: Record<string, unknown> | null = null;
    let activePair: Record<string, unknown> | null = null;
    const codecs = new Map<string, Record<string, unknown>>();

    for (const entry of report.values()) {
      const stats = entry as unknown as Record<string, unknown>;

      if (entry.type === "inbound-rtp" && stats.kind === "video") {
        inboundVideo = stats;
      }

      if (entry.type === "candidate-pair") {
        if (stats.state === "succeeded" && stats.nominated === true) {
          activePair = stats;
        }
      }

      // Collect codec information
      if (entry.type === "codec") {
        const codecId = stats.id as string;
        codecs.set(codecId, stats);
      }
    }

    // Process video track stats
    if (inboundVideo) {
      const bytes = Number(inboundVideo.bytesReceived ?? 0);
      const framesReceived = Number(inboundVideo.framesReceived ?? 0);
      const framesDecoded = Number(inboundVideo.framesDecoded ?? 0);
      const framesDropped = Number(inboundVideo.framesDropped ?? 0);
      const packetsReceived = Number(inboundVideo.packetsReceived ?? 0);
      const packetsLost = Number(inboundVideo.packetsLost ?? 0);

      // Calculate bitrate
      if (this.lastStatsSample) {
        const bytesDelta = bytes - this.lastStatsSample.bytesReceived;
        const timeDeltaMs = now - this.lastStatsSample.atMs;
        if (bytesDelta >= 0 && timeDeltaMs > 0) {
          const kbps = (bytesDelta * 8) / (timeDeltaMs / 1000) / 1000;
          this.diagnostics.bitrateKbps = Math.max(0, Math.round(kbps));
        }

        // Calculate packet loss percentage over the interval
        const packetsDelta = packetsReceived - this.lastStatsSample.packetsReceived;
        const lostDelta = packetsLost - this.lastStatsSample.packetsLost;
        if (packetsDelta > 0) {
          const totalPackets = packetsDelta + lostDelta;
          this.diagnostics.packetLossPercent = totalPackets > 0
            ? (lostDelta / totalPackets) * 100
            : 0;
        }
      }

      // Store current values for next delta calculation
      this.lastStatsSample = {
        bytesReceived: bytes,
        framesReceived,
        framesDecoded,
        framesDropped,
        packetsReceived,
        packetsLost,
        atMs: now,
      };

      // Frame counters
      this.diagnostics.framesReceived = framesReceived;
      this.diagnostics.framesDecoded = framesDecoded;
      this.diagnostics.framesDropped = framesDropped;

      // Decode FPS
      this.diagnostics.decodeFps = Math.round(Number(inboundVideo.framesPerSecond ?? 0));

      // Cumulative packet stats
      this.diagnostics.packetsLost = packetsLost;
      this.diagnostics.packetsReceived = packetsReceived;

      // Jitter (converted to milliseconds)
      this.diagnostics.jitterMs = Math.round(Number(inboundVideo.jitter ?? 0) * 1000 * 10) / 10;

      // Jitter buffer delay — the actual buffering latency added by the jitter buffer.
      // jitterBufferDelay is cumulative seconds, jitterBufferEmittedCount is cumulative frames.
      // Average = (delay / emittedCount) * 1000 for milliseconds.
      const jbDelay = Number(inboundVideo.jitterBufferDelay ?? 0);
      const jbEmitted = Number(inboundVideo.jitterBufferEmittedCount ?? 0);
      if (jbEmitted > 0) {
        this.diagnostics.jitterBufferDelayMs = Math.round((jbDelay / jbEmitted) * 1000 * 10) / 10;
      }

      // Get codec information
      const codecId = inboundVideo.codecId as string;
      if (codecId && codecs.has(codecId)) {
        const codecStats = codecs.get(codecId)!;
        const mimeType = (codecStats.mimeType as string) || "";
        const sdpFmtpLine = (codecStats.sdpFmtpLine as string) || "";

        // Extract codec name from MIME type
        if (mimeType.includes("H264")) {
          this.currentCodec = "H264";
        } else if (mimeType.includes("H265") || mimeType.includes("HEVC")) {
          this.currentCodec = "H265";
        } else if (mimeType.includes("AV1")) {
          this.currentCodec = "AV1";
        } else if (mimeType.includes("VP9")) {
          this.currentCodec = "VP9";
        } else if (mimeType.includes("VP8")) {
          this.currentCodec = "VP8";
        } else {
          // Try to extract from codecId itself
          this.currentCodec = normalizeCodecName(codecId);
        }

        // Check for HDR in SDP fmtp line
        this.isHdr = sdpFmtpLine.includes("transfer-characteristics=16") ||
          sdpFmtpLine.includes("hdr") ||
          sdpFmtpLine.includes("HDR");

        this.diagnostics.codec = this.currentCodec;
        this.diagnostics.isHdr = this.isHdr;
      }

      // Get video dimensions from track settings if available
      const videoTrack = this.videoStream.getVideoTracks()[0];
      if (videoTrack) {
        const settings = videoTrack.getSettings();
        if (settings.width && settings.height) {
          this.currentResolution = `${settings.width}x${settings.height}`;
          this.diagnostics.resolution = this.currentResolution;
        }
      }

      // Get decode timing if available
      const totalDecodeTime = Number(inboundVideo.totalDecodeTime ?? 0);
      const totalInterFrameDelay = Number(inboundVideo.totalInterFrameDelay ?? 0);
      const framesDecodedForTiming = Number(inboundVideo.framesDecoded ?? 1);

      if (framesDecodedForTiming > 0) {
        this.diagnostics.decodeTimeMs = Math.round((totalDecodeTime / framesDecodedForTiming) * 1000 * 10) / 10;
      }

      // Estimate render time from inter-frame delay
      if (totalInterFrameDelay > 0 && framesDecodedForTiming > 1) {
        const avgFrameDelay = totalInterFrameDelay / (framesDecodedForTiming - 1);
        this.diagnostics.renderTimeMs = Math.round(avgFrameDelay * 1000 * 10) / 10;
      }
    }

    // RTT from active candidate pair
    if (activePair?.currentRoundTripTime !== undefined) {
      const rtt = Number(activePair.currentRoundTripTime);
      this.diagnostics.rttMs = Math.round(rtt * 1000 * 10) / 10;
    }

    this.emitStats();
  }

  private detachInputCapture(): void {
    for (const cleanup of this.inputCleanup.splice(0)) {
      cleanup();
    }
  }

  private cleanupPeerConnection(): void {
    this.clearTimers();
    this.detachInputCapture();
    this.closeDataChannels();
    if (this.pc) {
      this.pc.onicecandidate = null;
      this.pc.ontrack = null;
      this.pc.onconnectionstatechange = null;
      this.pc.close();
      this.pc = null;
    }
    this.resetInputState();
    this.resetDiagnostics();
    this.connectedGamepads.clear();
    this.previousGamepadStates.clear();
    this.gamepadSendCount = 0;
    this.lastGamepadSendMs = 0;
    this.lastGamepadActivityMs = 0;
    this.reliableDropLogged = false;
    this.activeInputMode = "mkb";
    this.gamepadBitmap = 0;
    this.inputEncoder.resetGamepadSequences();
  }

  private attachTrack(track: MediaStreamTrack): void {
    if (track.kind === "video") {
      this.videoStream.addTrack(track);

      // Set up render FPS tracking using video element
      const video = this.options.videoElement;
      const frameCallback = () => {
        this.updateRenderFps();
        if (this.videoStream.active) {
          video.requestVideoFrameCallback(frameCallback);
        }
      };
      video.requestVideoFrameCallback(frameCallback);

      this.log("Video track attached");
      return;
    }

    if (track.kind === "audio") {
      this.audioStream.addTrack(track);
      this.options.audioElement
        .play()
        .then(() => {
          this.log("Audio track attached");
        })
        .catch((error) => {
          this.log(`Audio autoplay blocked: ${String(error)}`);
        });
    }
  }

  private async waitForIceGathering(pc: RTCPeerConnection, timeoutMs: number): Promise<string> {
    if (pc.iceGatheringState === "complete" && pc.localDescription?.sdp) {
      return pc.localDescription.sdp;
    }

    await new Promise<void>((resolve) => {
      let settled = false;
      const done = () => {
        if (!settled) {
          settled = true;
          pc.removeEventListener("icegatheringstatechange", onStateChange);
          resolve();
        }
      };

      const onStateChange = () => {
        if (pc.iceGatheringState === "complete") {
          done();
        }
      };

      pc.addEventListener("icegatheringstatechange", onStateChange);
      window.setTimeout(done, timeoutMs);
    });

    const sdp = pc.localDescription?.sdp;
    if (!sdp) {
      throw new Error("Missing local SDP after ICE gathering");
    }
    return sdp;
  }

  private setupInputHeartbeat(): void {
    if (this.heartbeatTimer !== null) {
      window.clearInterval(this.heartbeatTimer);
    }

    this.heartbeatTimer = window.setInterval(() => {
      if (!this.inputReady) {
        return;
      }
      const bytes = this.inputEncoder.encodeHeartbeat();
      this.sendReliable(bytes);
    }, 2000);
  }

  private setupGamepadPolling(): void {
    if (this.gamepadPollTimer !== null) {
      window.clearInterval(this.gamepadPollTimer);
    }

    this.log("Gamepad polling started (250Hz)");

    // Poll at 250Hz (4ms interval) — the practical minimum for setInterval in browsers.
    // The Rust reference polls at 1000Hz; browser timers can't go below ~4ms reliably.
    // Previous 60Hz (16.6ms) added up to 1-2 frames of input lag at 120fps.
    this.gamepadPollTimer = window.setInterval(() => {
      if (!this.inputReady) {
        return;
      }
      this.pollGamepads();
    }, 4);
  }

  private gamepadSendCount = 0;

  private pollGamepads(): void {
    const gamepads = navigator.getGamepads();
    if (!gamepads) {
      return;
    }

    let connectedCount = 0;
    const nowMs = performance.now();

    for (let i = 0; i < Math.min(gamepads.length, GAMEPAD_MAX_CONTROLLERS); i++) {
      const gamepad = gamepads[i];

      if (gamepad && gamepad.connected) {
        connectedCount++;

        // Track connected gamepads and update bitmap
        if (!this.connectedGamepads.has(i)) {
          this.connectedGamepads.add(i);
          // Set bit i in bitmap (matching official client's AA(i) = 1 << i)
          this.gamepadBitmap |= (1 << i);
          this.log(`Gamepad ${i} connected: ${gamepad.id}`);
          this.log(`  Buttons: ${gamepad.buttons.length}, Axes: ${gamepad.axes.length}, Mapping: ${gamepad.mapping}`);
          this.log(`  Bitmap now: 0x${this.gamepadBitmap.toString(16)}`);
          this.diagnostics.connectedGamepads = this.connectedGamepads.size;
          this.emitStats();
        }

        // Read and encode gamepad state
        const gamepadInput = this.readGamepadState(gamepad, i);
        const stateChanged = this.hasGamepadStateChanged(i, gamepadInput);

        // Send if state changed OR as a keepalive to maintain server controller presence
        // Games detect active input device by receiving packets; if we stop sending,
        // the game falls back to showing keyboard/mouse prompts.
        const needsKeepalive = this.activeInputMode === "gamepad"
          && !stateChanged
          && (nowMs - this.lastGamepadSendMs) >= GfnWebRtcClient.GAMEPAD_KEEPALIVE_MS;

        if (stateChanged || needsKeepalive) {
          // Determine if we should use the partially reliable channel
          const usePR = this.mouseInputChannel?.readyState === "open";
          const bytes = this.inputEncoder.encodeGamepadState(gamepadInput, this.gamepadBitmap, usePR);
          this.sendGamepad(bytes);
          this.lastGamepadSendMs = nowMs;

          if (stateChanged) {
            this.previousGamepadStates.set(i, { ...gamepadInput });
            this.lastGamepadActivityMs = nowMs;
          }

          // Switch to gamepad input mode — suppresses mouse/keyboard
          if (this.activeInputMode !== "gamepad") {
            this.activeInputMode = "gamepad";
            // Discard any pending mouse deltas to avoid a stale burst
            this.pendingMouseDx = 0;
            this.pendingMouseDy = 0;
            this.log("Input mode → gamepad");
          }

          // Log first N gamepad sends for debugging
          if (stateChanged) {
            this.gamepadSendCount++;
            if (this.gamepadSendCount <= 20) {
              this.log(`Gamepad send #${this.gamepadSendCount}: pad=${i} btns=0x${gamepadInput.buttons.toString(16)} lt=${gamepadInput.leftTrigger} rt=${gamepadInput.rightTrigger} lx=${gamepadInput.leftStickX} ly=${gamepadInput.leftStickY} rx=${gamepadInput.rightStickX} ry=${gamepadInput.rightStickY} bytes=${bytes.length}`);
            }
          }
        }
      } else if (this.connectedGamepads.has(i)) {
        // Gamepad disconnected — clear bit from bitmap
        this.connectedGamepads.delete(i);
        this.previousGamepadStates.delete(i);
        this.gamepadBitmap &= ~(1 << i);
        this.log(`Gamepad ${i} disconnected, bitmap now: 0x${this.gamepadBitmap.toString(16)}`);
        this.diagnostics.connectedGamepads = this.connectedGamepads.size;
        this.emitStats();

        // Send state with updated bitmap (gamepad bit cleared = disconnected)
        const disconnectState: GamepadInput = {
          controllerId: i,
          buttons: 0,
          leftTrigger: 0,
          rightTrigger: 0,
          leftStickX: 0,
          leftStickY: 0,
          rightStickX: 0,
          rightStickY: 0,
          connected: false,
          timestampUs: timestampUs(),
        };
        const usePR = this.mouseInputChannel?.readyState === "open";
        const bytes = this.inputEncoder.encodeGamepadState(disconnectState, this.gamepadBitmap, usePR);
        this.sendGamepad(bytes);
      }
    }

    this.diagnostics.connectedGamepads = connectedCount;
  }

  private readGamepadState(gamepad: Gamepad, controllerId: number): GamepadInput {
    const buttons = mapGamepadButtons(gamepad);
    const axes = readGamepadAxes(gamepad);

    return {
      controllerId,
      buttons,
      leftTrigger: normalizeToUint8(axes.leftTrigger),
      rightTrigger: normalizeToUint8(axes.rightTrigger),
      leftStickX: normalizeToInt16(axes.leftStickX),
      leftStickY: normalizeToInt16(axes.leftStickY),
      rightStickX: normalizeToInt16(axes.rightStickX),
      rightStickY: normalizeToInt16(axes.rightStickY),
      connected: true,
      timestampUs: timestampUs(),
    };
  }

  private hasGamepadStateChanged(controllerId: number, newState: GamepadInput): boolean {
    const prevState = this.previousGamepadStates.get(controllerId);
    if (!prevState) {
      return true;
    }

    return (
      prevState.buttons !== newState.buttons ||
      prevState.leftTrigger !== newState.leftTrigger ||
      prevState.rightTrigger !== newState.rightTrigger ||
      prevState.leftStickX !== newState.leftStickX ||
      prevState.leftStickY !== newState.leftStickY ||
      prevState.rightStickX !== newState.rightStickX ||
      prevState.rightStickY !== newState.rightStickY
    );
  }

  private onGamepadConnected = (event: GamepadEvent): void => {
    this.log(`Gamepad connected event: ${event.gamepad.id}`);
    // The polling loop will detect and handle the new gamepad
  };

  private onGamepadDisconnected = (event: GamepadEvent): void => {
    this.log(`Gamepad disconnected event: ${event.gamepad.id}`);
    // The polling loop will detect and handle the disconnection
  };

  private onInputHandshakeMessage(bytes: Uint8Array): void {
    if (bytes.length < 2) {
      this.log(`Input handshake: ignoring short message (${bytes.length} bytes)`);
      return;
    }

    const hex = Array.from(bytes.slice(0, Math.min(bytes.length, 16)))
      .map((b) => b.toString(16).padStart(2, "0"))
      .join(" ");
    this.log(`Input channel message: ${bytes.length} bytes [${hex}]`);

    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const firstWord = view.getUint16(0, true);
    let version = 2;

    if (firstWord === 526) {
      version = bytes.length >= 4 ? view.getUint16(2, true) : 2;
      this.log(`Handshake detected: firstWord=526 (0x020e), version=${version}`);
    } else if (bytes[0] === 0x0e) {
      version = firstWord;
      this.log(`Handshake detected: byte[0]=0x0e, version=${version}`);
    } else {
      this.log(`Input channel message not a handshake: firstWord=${firstWord} (0x${firstWord.toString(16)})`);
      return;
    }

    if (!this.inputReady) {
      // Official GFN browser client does NOT echo the handshake back.
      // It just reads the protocol version and starts sending input.
      // (The Rust reference implementation does echo, but that's for its own server.)
      this.inputReady = true;
      this.inputProtocolVersion = version;
      this.inputEncoder.setProtocolVersion(version);
      this.diagnostics.inputReady = true;
      this.emitStats();
      this.log(`Input handshake complete (protocol v${version}) — starting heartbeat + gamepad polling`);
      this.setupInputHeartbeat();
      this.setupGamepadPolling();
    }
  }

  private createDataChannels(pc: RTCPeerConnection): void {
    this.reliableInputChannel = pc.createDataChannel("input_channel_v1", {
      ordered: true,
      maxRetransmits: 0,
    });

    this.reliableInputChannel.onopen = () => {
      this.log("Reliable input channel open");
    };

    this.reliableInputChannel.onmessage = async (event) => {
      const bytes = await toBytes(event.data as string | Blob | ArrayBuffer);
      this.onInputHandshakeMessage(bytes);
    };

    this.mouseInputChannel = pc.createDataChannel("input_channel_partially_reliable", {
      ordered: false,
      maxPacketLifeTime: 8,
    });

    this.mouseInputChannel.onopen = () => {
      this.log("Mouse channel open (partially reliable)");
    };
  }

  private async flushQueuedCandidates(): Promise<void> {
    if (!this.pc || !this.pc.remoteDescription) {
      return;
    }

    while (this.queuedCandidates.length > 0) {
      const candidate = this.queuedCandidates.shift();
      if (!candidate) {
        continue;
      }
      await this.pc.addIceCandidate(candidate);
    }
  }

  private reliableDropLogged = false;

  public sendReliable(payload: Uint8Array): void {
    if (this.reliableInputChannel?.readyState === "open") {
      const safePayload = Uint8Array.from(payload);
      this.reliableInputChannel.send(safePayload.buffer);
    } else if (!this.reliableDropLogged) {
      this.reliableDropLogged = true;
      this.log(`Reliable channel not open (state=${this.reliableInputChannel?.readyState ?? "null"}), dropping event (${payload.length} bytes)`);
    }
  }

  /** Send gamepad data on the partially reliable channel (unordered, maxPacketLifeTime).
   *  Falls back to reliable channel if partially reliable isn't available.
   *  Official GFN client uses partially reliable ONLY for gamepad, not mouse. */
  private sendGamepad(payload: Uint8Array): void {
    if (this.mouseInputChannel?.readyState === "open") {
      const safePayload = Uint8Array.from(payload);
      this.mouseInputChannel.send(safePayload.buffer);
      return;
    }
    // Fallback to reliable channel if partially reliable not ready
    this.sendReliable(payload);
  }

  private installInputCapture(videoElement: HTMLVideoElement): void {
    this.detachInputCapture();

    const flushMouse = () => {
      if (!this.inputReady) {
        return;
      }
      if (this.activeInputMode === "gamepad") {
        // Discard accumulated mouse movement while gamepad is active
        this.pendingMouseDx = 0;
        this.pendingMouseDy = 0;
        return;
      }
      if (this.pendingMouseDx === 0 && this.pendingMouseDy === 0) {
        return;
      }

      const payload = this.inputEncoder.encodeMouseMove({
        dx: Math.max(-32768, Math.min(32767, this.pendingMouseDx)),
        dy: Math.max(-32768, Math.min(32767, this.pendingMouseDy)),
        timestampUs: timestampUs(),
      });

      this.pendingMouseDx = 0;
      this.pendingMouseDy = 0;
      // Official GFN client sends all mouse events on reliable channel (input_channel_v1)
      this.sendReliable(payload);
    };

    this.mouseFlushTimer = window.setInterval(flushMouse, 4);

    const onKeyDown = (event: KeyboardEvent) => {
      if (!this.inputReady || event.repeat) {
        return;
      }

      const mapped = mapKeyboardEvent(event);
      if (!mapped) {
        return;
      }

      // Don't send keyboard input while gamepad was recently active.
      // This prevents accidental key presses from making the game switch
      // to showing keyboard/mouse prompts. The user must put down the
      // controller for a few seconds before keyboard input takes over.
      if (this.activeInputMode === "gamepad") {
        const idleMs = performance.now() - this.lastGamepadActivityMs;
        if (idleMs < GfnWebRtcClient.GAMEPAD_MODE_LOCKOUT_MS) {
          return;
        }
        // Gamepad idle long enough — allow switch to mkb
        this.activeInputMode = "mkb";
        this.log("Input mode → mouse+keyboard (gamepad idle)");
      }

      event.preventDefault();
      this.pressedKeys.add(mapped.vk);
      const payload = this.inputEncoder.encodeKeyDown({
        keycode: mapped.vk,
        scancode: mapped.scancode,
        modifiers: modifierFlags(event),
        timestampUs: timestampUs(),
      });
      this.sendReliable(payload);
    };

    const onKeyUp = (event: KeyboardEvent) => {
      if (!this.inputReady || this.activeInputMode === "gamepad") {
        return;
      }

      const mapped = mapKeyboardEvent(event);
      if (!mapped) {
        return;
      }

      event.preventDefault();
      this.pressedKeys.delete(mapped.vk);
      const payload = this.inputEncoder.encodeKeyUp({
        keycode: mapped.vk,
        scancode: mapped.scancode,
        modifiers: modifierFlags(event),
        timestampUs: timestampUs(),
      });
      this.sendReliable(payload);
    };

    const onMouseMove = (event: MouseEvent) => {
      if (!this.inputReady || document.pointerLockElement !== videoElement) {
        return;
      }
      // Don't accumulate mouse movement in gamepad mode
      if (this.activeInputMode === "gamepad") {
        return;
      }
      this.pendingMouseDx += event.movementX;
      this.pendingMouseDy += event.movementY;
    };

    const onMouseDown = (event: MouseEvent) => {
      if (!this.inputReady) {
        return;
      }
      // Don't send mouse clicks while gamepad was recently active.
      // This prevents accidental clicks from making the game switch
      // to showing keyboard/mouse prompts.
      if (this.activeInputMode === "gamepad") {
        const idleMs = performance.now() - this.lastGamepadActivityMs;
        if (idleMs < GfnWebRtcClient.GAMEPAD_MODE_LOCKOUT_MS) {
          return;
        }
        // Gamepad idle long enough — allow switch to mkb
        this.activeInputMode = "mkb";
        this.log("Input mode → mouse+keyboard (gamepad idle)");
      }
      event.preventDefault();
      const payload = this.inputEncoder.encodeMouseButtonDown({
        button: toMouseButton(event.button),
        timestampUs: timestampUs(),
      });
      // Official GFN client sends all mouse events on reliable channel (input_channel_v1)
      this.sendReliable(payload);
    };

    const onMouseUp = (event: MouseEvent) => {
      if (!this.inputReady || this.activeInputMode === "gamepad") {
        return;
      }
      event.preventDefault();
      const payload = this.inputEncoder.encodeMouseButtonUp({
        button: toMouseButton(event.button),
        timestampUs: timestampUs(),
      });
      // Official GFN client sends all mouse events on reliable channel (input_channel_v1)
      this.sendReliable(payload);
    };

    const onWheel = (event: WheelEvent) => {
      if (!this.inputReady || this.activeInputMode === "gamepad") {
        return;
      }
      event.preventDefault();
      // Official GFN client sends negated raw deltaY as int16 (no quantization to ±120).
      // Clamp to int16 range since browser deltaY can exceed it with fast scrolling.
      const delta = Math.max(-32768, Math.min(32767, Math.round(-event.deltaY)));
      const payload = this.inputEncoder.encodeMouseWheel({
        delta,
        timestampUs: timestampUs(),
      });
      this.sendReliable(payload);
    };

    const onClick = () => {
      // Request pointer lock with unadjustedMovement for raw, unaccelerated input.
      // This bypasses the OS mouse acceleration curve, matching the official GFN client.
      // Falls back to standard pointer lock if unadjustedMovement is not supported.
      this.log("Requesting pointer lock with unadjustedMovement=true");
      const result = videoElement.requestPointerLock({ unadjustedMovement: true } as any);
      // Chrome 88+ returns a Promise when options are passed
      if (result && typeof (result as any).then === "function") {
        (result as unknown as Promise<void>)
          .then(() => {
            this.log("Pointer lock acquired with unadjustedMovement=true (raw/unaccelerated)");
          })
          .catch((err: DOMException) => {
            if (err.name === "NotSupportedError") {
              this.log("unadjustedMovement not supported, falling back to standard pointer lock (accelerated)");
              return videoElement.requestPointerLock();
            }
            this.log(`Pointer lock request failed: ${err.name}: ${err.message}`);
          });
      } else {
        // Older API returned void — pointer lock is acquired but unadjustedMovement may be ignored
        this.log("Warning: requestPointerLock returned void (not a Promise) — unadjustedMovement may not be applied");
      }
      videoElement.focus();
    };

    // Store video element for pointer lock re-acquisition
    this.videoElement = videoElement;

    // Handle pointer lock changes — send synthetic Escape when lock is lost by browser
    // (matches official GFN client's "pointerLockEscape" feature)
    const onPointerLockChange = () => {
      if (document.pointerLockElement) {
        // Pointer lock gained — cancel any pending synthetic Escape
        if (this.pointerLockEscapeTimer !== null) {
          window.clearTimeout(this.pointerLockEscapeTimer);
          this.pointerLockEscapeTimer = null;
        }
        return;
      }

      // Pointer lock was lost
      if (!this.inputReady) return;

      // VK 0x1B = 27 = Escape
      const escapeWasPressed = this.pressedKeys.has(0x1B);

      if (escapeWasPressed) {
        // Escape was already tracked as pressed — the normal keyup handler will fire
        // and send Escape keyup to the server. No synthetic needed.
        return;
      }

      // Escape was NOT tracked as pressed — browser intercepted it before our keydown fired.
      // Send synthetic Escape keydown+keyup after 50ms (matches official GFN client).
      // Also re-acquire pointer lock so the user stays in the game.
      this.pointerLockEscapeTimer = window.setTimeout(() => {
        this.pointerLockEscapeTimer = null;

        if (!this.inputReady) return;

        // Release all currently held keys first (matching official client's MS() function)
        for (const vk of this.pressedKeys) {
          const payload = this.inputEncoder.encodeKeyUp({
            keycode: vk,
            scancode: 0, // scancode not critical for release
            modifiers: 0,
            timestampUs: timestampUs(),
          });
          this.sendReliable(payload);
        }
        this.pressedKeys.clear();

        // Send synthetic Escape keydown + keyup
        this.log("Sending synthetic Escape (pointer lock lost by browser)");
        const escDown = this.inputEncoder.encodeKeyDown({
          keycode: 0x1B,
          scancode: 0x29, // Escape scancode
          modifiers: 0,
          timestampUs: timestampUs(),
        });
        this.sendReliable(escDown);

        const escUp = this.inputEncoder.encodeKeyUp({
          keycode: 0x1B,
          scancode: 0x29,
          modifiers: 0,
          timestampUs: timestampUs(),
        });
        this.sendReliable(escUp);

        // Re-acquire pointer lock so the user stays in the game
        if (this.videoElement && this.activeInputMode !== "gamepad") {
          this.videoElement.requestPointerLock({ unadjustedMovement: true } as any)
            .catch((err: DOMException) => {
              if (err.name === "NotSupportedError") {
                return this.videoElement?.requestPointerLock();
              }
              // Pointer lock re-acquire may fail if user intentionally exited — that's ok
            })
            .catch(() => {});
        }
      }, 50);
    };

    // Try to lock keyboard (Escape, F11, etc.) when in fullscreen.
    // This prevents the browser from processing Escape as pointer lock exit.
    // Only works in fullscreen + secure context + Chromium.
    const onFullscreenChange = () => {
      const nav = navigator as any;
      if (document.fullscreenElement) {
        if (nav.keyboard?.lock) {
          nav.keyboard.lock([
            "Escape", "F11", "BrowserBack", "BrowserForward", "BrowserRefresh",
          ]).then(() => {
            this.log("Keyboard lock acquired (Escape captured in fullscreen)");
          }).catch((err: Error) => {
            this.log(`Keyboard lock failed: ${err.message}`);
          });
        }
      } else {
        if (nav.keyboard?.unlock) {
          nav.keyboard.unlock();
        }
      }
    };

    // Add gamepad event listeners
    window.addEventListener("gamepadconnected", this.onGamepadConnected);
    window.addEventListener("gamepaddisconnected", this.onGamepadDisconnected);

    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    window.addEventListener("mousemove", onMouseMove);
    videoElement.addEventListener("mousedown", onMouseDown);
    videoElement.addEventListener("mouseup", onMouseUp);
    videoElement.addEventListener("wheel", onWheel, { passive: false });
    videoElement.addEventListener("click", onClick);
    document.addEventListener("pointerlockchange", onPointerLockChange);
    document.addEventListener("fullscreenchange", onFullscreenChange);

    // If already in fullscreen, try to lock keyboard immediately
    if (document.fullscreenElement) {
      onFullscreenChange();
    }

    this.inputCleanup.push(() => window.removeEventListener("gamepadconnected", this.onGamepadConnected));
    this.inputCleanup.push(() => window.removeEventListener("gamepaddisconnected", this.onGamepadDisconnected));
    this.inputCleanup.push(() => window.removeEventListener("keydown", onKeyDown));
    this.inputCleanup.push(() => window.removeEventListener("keyup", onKeyUp));
    this.inputCleanup.push(() => window.removeEventListener("mousemove", onMouseMove));
    this.inputCleanup.push(() => videoElement.removeEventListener("mousedown", onMouseDown));
    this.inputCleanup.push(() => videoElement.removeEventListener("mouseup", onMouseUp));
    this.inputCleanup.push(() => videoElement.removeEventListener("wheel", onWheel));
    this.inputCleanup.push(() => videoElement.removeEventListener("click", onClick));
    this.inputCleanup.push(() => document.removeEventListener("pointerlockchange", onPointerLockChange));
    this.inputCleanup.push(() => document.removeEventListener("fullscreenchange", onFullscreenChange));
    this.inputCleanup.push(() => {
      if (this.pointerLockEscapeTimer !== null) {
        window.clearTimeout(this.pointerLockEscapeTimer);
        this.pointerLockEscapeTimer = null;
      }
      this.pressedKeys.clear();
      this.videoElement = null;
      // Unlock keyboard on cleanup
      const nav = navigator as any;
      if (nav.keyboard?.unlock) {
        nav.keyboard.unlock();
      }
    });
  }

  /**
   * Query browser for supported video codecs via RTCRtpReceiver.getCapabilities.
   * Returns normalized names like "H264", "H265", "AV1", "VP9", "VP8".
   */
  private getSupportedVideoCodecs(): string[] {
    try {
      const capabilities = RTCRtpReceiver.getCapabilities("video");
      if (!capabilities) return [];
      const codecs = new Set<string>();
      for (const codec of capabilities.codecs) {
        const mime = codec.mimeType.toUpperCase();
        if (mime.includes("H264")) codecs.add("H264");
        else if (mime.includes("H265") || mime.includes("HEVC")) codecs.add("H265");
        else if (mime.includes("AV1")) codecs.add("AV1");
        else if (mime.includes("VP9")) codecs.add("VP9");
        else if (mime.includes("VP8")) codecs.add("VP8");
      }
      return Array.from(codecs);
    } catch {
      return [];
    }
  }

  async handleOffer(offerSdp: string, session: SessionInfo, settings: OfferSettings): Promise<void> {
    this.cleanupPeerConnection();

    this.log("=== handleOffer START ===");
    this.log(`Session: id=${session.sessionId}, status=${session.status}, serverIp=${session.serverIp}`);
    this.log(`Signaling: server=${session.signalingServer}, url=${session.signalingUrl}`);
    this.log(`MediaConnectionInfo: ${session.mediaConnectionInfo ? `ip=${session.mediaConnectionInfo.ip}, port=${session.mediaConnectionInfo.port}` : "NONE"}`);
    this.log(`Settings: codec=${settings.codec}, resolution=${settings.resolution}, fps=${settings.fps}, maxBitrate=${settings.maxBitrateKbps}kbps`);
    this.log(`ICE servers: ${session.iceServers.length} (${session.iceServers.map(s => s.urls.join(",")).join(" | ")})`);
    this.log(`Offer SDP length: ${offerSdp.length} chars`);
    // Log full offer SDP for ICE debugging
    this.log(`=== FULL OFFER SDP START ===`);
    for (const line of offerSdp.split(/\r?\n/)) {
      this.log(`  SDP> ${line}`);
    }
    this.log(`=== FULL OFFER SDP END ===`);

    // Extract server region from session
    this.serverRegion = session.signalingServer || session.streamingBaseUrl || "";
    // Clean up the region string (extract hostname or region name)
    if (this.serverRegion) {
      try {
        const url = new URL(this.serverRegion);
        this.serverRegion = url.hostname;
      } catch {
        // Keep as-is if not a valid URL
      }
    }

    const rtcConfig: RTCConfiguration = {
      iceServers: toRtcIceServers(session.iceServers),
      bundlePolicy: "max-bundle",
      rtcpMuxPolicy: "require",
    };

    const pc = new RTCPeerConnection(rtcConfig);
    this.pc = pc;
    this.diagnostics.connectionState = pc.connectionState;
    this.diagnostics.serverRegion = this.serverRegion;
    this.diagnostics.gpuType = this.gpuType;
    this.emitStats();

    this.resetInputState();
    this.resetDiagnostics();
    this.createDataChannels(pc);
    this.installInputCapture(this.options.videoElement);
    this.setupStatsPolling();

    pc.onicecandidate = (event) => {
      if (!event.candidate) {
        this.log("ICE gathering complete (null candidate)");
        return;
      }
      const payload = event.candidate.toJSON();
      if (!payload.candidate) {
        return;
      }
      this.log(`Local ICE candidate: ${payload.candidate}`);
      const candidate: IceCandidatePayload = {
        candidate: payload.candidate,
        sdpMid: payload.sdpMid,
        sdpMLineIndex: payload.sdpMLineIndex,
        usernameFragment: payload.usernameFragment,
      };
      window.openNow.sendIceCandidate(candidate).catch((error) => {
        this.log(`Failed to send local ICE candidate: ${String(error)}`);
      });
    };

    pc.onconnectionstatechange = () => {
      this.diagnostics.connectionState = pc.connectionState;
      this.emitStats();
      this.log(`Peer connection state: ${pc.connectionState}`);
    };

    pc.onicecandidateerror = (event: Event) => {
      const e = event as RTCPeerConnectionIceErrorEvent;
      this.log(`ICE candidate error: ${e.errorCode} ${e.errorText} (${e.url ?? "no url"}) hostCandidate=${e.hostCandidate ?? "?"}`);
    };

    pc.oniceconnectionstatechange = () => {
      this.log(`ICE connection state: ${pc.iceConnectionState}`);
    };

    pc.onicegatheringstatechange = () => {
      this.log(`ICE gathering state: ${pc.iceGatheringState}`);
    };

    pc.onsignalingstatechange = () => {
      this.log(`Signaling state: ${pc.signalingState}`);
    };

    pc.ontrack = (event) => {
      this.log(`Track received: kind=${event.track.kind}, id=${event.track.id}, readyState=${event.track.readyState}`);
      this.attachTrack(event.track);

      // Configure low-latency jitter buffer for video and audio receivers
      this.configureReceiverForLowLatency(event.receiver, event.track.kind);
    };

    // --- SDP Processing (matching Rust reference) ---

    // 1. Fix 0.0.0.0 in server's SDP offer with real server IP
    //    The GFN server sends c=IN IP4 0.0.0.0; replace with actual IP
    const serverIpForSdp = session.mediaConnectionInfo?.ip || session.serverIp || "";
    let processedOffer = offerSdp;
    if (serverIpForSdp) {
      processedOffer = fixServerIp(processedOffer, serverIpForSdp);
      this.log(`Fixed server IP in SDP offer: ${serverIpForSdp}`);
      // Log any remaining 0.0.0.0 references after fix
      const remaining = (processedOffer.match(/0\.0\.0\.0/g) ?? []).length;
      if (remaining > 0) {
        this.log(`Warning: ${remaining} occurrences of 0.0.0.0 still remain in SDP after fix`);
      }
    }

    // 2. Extract server's ice-ufrag BEFORE any modifications (needed for manual candidate injection)
    const serverIceUfrag = extractIceUfragFromOffer(processedOffer);
    this.log(`Server ICE ufrag: "${serverIceUfrag}"`);

    // 3. Filter to preferred codec — but only if the browser actually supports it
    let effectiveCodec = settings.codec;
    const supported = this.getSupportedVideoCodecs();
    this.log(`Browser supported video codecs: ${supported.join(", ") || "unknown"}`);
    if (supported.length > 0 && !supported.includes(settings.codec)) {
      // Requested codec not supported — fall back to H264 (universal) or first supported
      const fallback = supported.includes("H264") ? "H264" : supported[0];
      this.log(`Warning: ${settings.codec} not supported by browser, falling back to ${fallback}`);
      effectiveCodec = fallback as VideoCodec;
    }
    this.log(`Effective codec: ${effectiveCodec}`);
    const filteredOffer = preferCodec(processedOffer, effectiveCodec);
    this.log(`Filtered offer SDP length: ${filteredOffer.length} chars`);
    this.log("Setting remote description (offer)...");
    await pc.setRemoteDescription({ type: "offer", sdp: filteredOffer });
    this.log("Remote description set successfully");
    await this.flushQueuedCandidates();

    // 4. Create answer and set local description
    this.log("Creating answer...");
    const answer = await pc.createAnswer();
    this.log(`Answer created, SDP length: ${answer.sdp?.length ?? 0} chars`);
    await pc.setLocalDescription(answer);
    this.log("Local description set, waiting for ICE gathering...");

    const finalSdp = await this.waitForIceGathering(pc, 5000);
    this.log(`ICE gathering done, final SDP length: ${finalSdp.length} chars`);
    const credentials = extractIceCredentials(finalSdp);
    this.log(`Extracted ICE credentials: ufrag=${credentials.ufrag}, pwd=${credentials.pwd.slice(0, 8)}...`);
    const { width, height } = parseResolution(settings.resolution);

    const nvstSdp = buildNvstSdp({
      width,
      height,
      fps: settings.fps,
      maxBitrateKbps: settings.maxBitrateKbps,
      credentials,
    });

    await window.openNow.sendAnswer({
      sdp: finalSdp,
      nvstSdp,
    });
    this.log("Sent SDP answer and nvstSdp");

    // 5. Inject manual ICE candidate from mediaConnectionInfo AFTER answer is sent
    //    (matches Rust reference ordering — full SDP exchange completes first)
    //    GFN servers use ice-lite and may not trickle candidates via signaling.
    //    The actual media endpoint comes from the session's connectionInfo array.
    if (session.mediaConnectionInfo) {
      const mci = session.mediaConnectionInfo;
      const rawIp = extractPublicIp(mci.ip);
      if (rawIp && mci.port > 0) {
        const candidateStr = `candidate:1 1 udp 2130706431 ${rawIp} ${mci.port} typ host`;
        this.log(`Injecting manual ICE candidate: ${rawIp}:${mci.port}`);

        // Try sdpMid "0" first, then "1", "2", "3" (matching Rust fallback)
        const mids = ["0", "1", "2", "3"];
        let injected = false;
        for (const mid of mids) {
          try {
            await pc.addIceCandidate({
              candidate: candidateStr,
              sdpMid: mid,
              sdpMLineIndex: parseInt(mid, 10),
              usernameFragment: serverIceUfrag || undefined,
            });
            this.log(`Manual ICE candidate injected (sdpMid=${mid})`);
            injected = true;
            break;
          } catch (error) {
            this.log(`Manual ICE candidate failed for sdpMid=${mid}: ${String(error)}`);
          }
        }
        if (!injected) {
          this.log("Warning: Could not inject manual ICE candidate on any sdpMid");
        }
      } else {
        this.log(`Warning: mediaConnectionInfo present but no valid IP (ip=${mci.ip}, port=${mci.port})`);
      }
    } else {
      this.log("No mediaConnectionInfo available — relying on trickle ICE only");
    }

    this.log("=== handleOffer COMPLETE — waiting for ICE connectivity and tracks ===");
  }

  async addRemoteCandidate(candidate: IceCandidatePayload): Promise<void> {
    this.log(`Remote ICE candidate received: ${candidate.candidate} (sdpMid=${candidate.sdpMid})`);
    const init: RTCIceCandidateInit = {
      candidate: candidate.candidate,
      sdpMid: candidate.sdpMid ?? undefined,
      sdpMLineIndex: candidate.sdpMLineIndex ?? undefined,
      usernameFragment: candidate.usernameFragment ?? undefined,
    };

    if (!this.pc || !this.pc.remoteDescription) {
      this.queuedCandidates.push(init);
      return;
    }

    await this.pc.addIceCandidate(init);
  }

  dispose(): void {
    this.cleanupPeerConnection();

    for (const track of this.videoStream.getTracks()) {
      this.videoStream.removeTrack(track);
    }
    for (const track of this.audioStream.getTracks()) {
      this.audioStream.removeTrack(track);
    }
  }
}
