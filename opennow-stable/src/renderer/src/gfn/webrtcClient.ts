import type {
  IceCandidatePayload,
  ColorQuality,
  IceServer,
  SessionInfo,
  VideoCodec,
  MicrophoneMode,
} from "@shared/gfn";

import {
  InputEncoder,
  INPUT_MOUSE_REL,
  PARTIALLY_RELIABLE_GAMEPAD_MASK_ALL,
  PARTIALLY_RELIABLE_HID_DEVICE_MASK_ALL,
  partiallyReliableHidMaskForInputType,
  isPartiallyReliableHidTransferEligible,
  mapKeyboardEvent,
  modifierFlags,
  toMouseButton,
  mapGamepadButtons,
  readGamepadAxes,
  normalizeToInt16,
  normalizeToUint8,
  GAMEPAD_MAX_CONTROLLERS,
  type GamepadInput,
  codeMap,
  mapTextCharToKeySpec,
} from "./inputProtocol";
import {
  buildNvstSdp,
  extractIceCredentials,
  extractIceUfragFromOffer,
  extractPublicIp,
  fixServerIp,
  mungeAnswerSdp,
  preferCodec,
  rewriteH265LevelIdByProfile,
  rewriteH265TierFlag,
} from "./sdp";
import { MicrophoneManager, type MicState, type MicStateChange } from "./microphoneManager";

interface OfferSettings {
  codec: VideoCodec;
  colorQuality: ColorQuality;
  resolution: string;
  fps: number;
  maxBitrateKbps: number;
}

interface RiInputCapabilities {
  partialReliableThresholdMs: number | null;
  hidDeviceMask: number;
  enablePartiallyReliableTransferGamepad: number;
  enablePartiallyReliableTransferHid: number;
}

interface DualRumbleEffectOptions {
  startDelay: 0;
  duration: number;
  weakMagnitude: number;
  strongMagnitude: number;
}

interface GamepadHapticActuatorLike {
  readonly type?: string;
  playEffect(effectType: "dual-rumble", options: DualRumbleEffectOptions): Promise<unknown>;
}

interface LegacyGamepadHapticActuatorLike {
  pulse(value: number, duration: number): Promise<unknown>;
}

type GamepadWithOptionalHaptics = Gamepad & {
  readonly vibrationActuator?: GamepadHapticActuatorLike | null;
  readonly hapticActuators?: readonly (LegacyGamepadHapticActuatorLike | null | undefined)[] | null;
};

interface GamepadRumbleApi {
  playEffectActuator: GamepadHapticActuatorLike | null;
  pulseActuator: LegacyGamepadHapticActuatorLike | null;
}

interface ConnectedRumbleGamepad {
  index: number;
  gamepad: Gamepad;
  api: GamepadRumbleApi | null;
}

function hevcPreferredProfileId(colorQuality: ColorQuality): 1 | 2 {
  // 10-bit modes should prefer HEVC Main10 profile-id=2.
  return colorQuality.startsWith("10bit") ? 2 : 1;
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

  // Input channel pressure
  inputQueueBufferedBytes: number;
  inputQueuePeakBufferedBytes: number;
  partiallyReliableInputQueueBufferedBytes: number;
  partiallyReliableInputQueuePeakBufferedBytes: number;
  inputQueueDropCount: number;
  inputQueueMaxSchedulingDelayMs: number;
  partiallyReliableInputOpen: boolean;
  mouseMoveTransport: "reliable" | "partially_reliable";
  mouseFlushIntervalMs: number;
  mousePacketsPerSecond: number;
  mouseResidualMagnitude: number;
  mouseAdaptiveFlushActive: boolean;

  lagReason: StreamLagReason;
  lagReasonDetail: string;

  // System info
  gpuType: string;
  serverRegion: string;

  // Decoder recovery status
  decoderPressureActive: boolean;
  decoderRecoveryAttempts: number;
  decoderRecoveryAction: string;

  // Microphone state
  micState: MicState;
  micEnabled: boolean;
}

export type StreamLagReason =
  | "unknown"
  | "stable"
  | "network"
  | "decoder"
  | "input_backpressure"
  | "render";

export interface StreamTimeWarning {
  code: 1 | 2 | 3;
  secondsLeft?: number;
}

interface ClientOptions {
  videoElement: HTMLVideoElement;
  audioElement: HTMLAudioElement;
  /** Microphone mode preference */
  microphoneMode?: MicrophoneMode;
  /** When true, pointer-lock acquisition may also enter fullscreen */
  autoFullScreen?: boolean;
  /** Preferred microphone device ID */
  microphoneDeviceId?: string;
  /** Mouse sensitivity multiplier (1.0 = default) */
  mouseSensitivity?: number;
  /** Software acceleration strength percentage (1-150) */
  mouseAcceleration?: number;
  onLog: (line: string) => void;
  onStats?: (stats: StreamDiagnostics) => void;
  onTimeWarning?: (warning: StreamTimeWarning) => void;
  onMicStateChange?: (state: MicStateChange) => void;
  onIceConnectionStateChange?: (state: RTCIceConnectionState) => void;
  onPeerConnectionStateChange?: (state: RTCPeerConnectionState) => void;
  /** Optional host callback for Meta/Home button edge presses (button 16). */
  onControllerMetaPress?: (event: { controllerId: number; gamepad: Gamepad }) => void;
}

function timestampUs(sourceTimestampMs?: number): bigint {
  const base =
    typeof sourceTimestampMs === "number" && Number.isFinite(sourceTimestampMs) && sourceTimestampMs >= 0
      ? sourceTimestampMs
      : performance.now();
  return BigInt(Math.floor(base * 1000));
}

function parsePartialReliableThresholdMs(sdp: string): number | null {
  const match = sdp.match(/a=ri\.partialReliableThresholdMs:(\d+)/i);
  if (!match?.[1]) {
    return null;
  }
  const parsed = Number.parseInt(match[1], 10);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    return null;
  }
  return Math.max(1, Math.min(5000, parsed));
}

function parseRiIntegerAttribute(sdp: string, attribute: string, fallback: number): number {
  const escapedAttribute = attribute.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = sdp.match(new RegExp(`a=${escapedAttribute}:([^\\r\\n]+)`, "i"));
  const raw = match?.[1]?.trim();
  if (!raw) {
    return fallback;
  }
  const normalized = raw.toLowerCase();
  const parsed = normalized.startsWith("0x")
    ? Number.parseInt(normalized.slice(2), 16)
    : Number.parseInt(normalized, 10);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function parseRiInputCapabilities(sdp: string): RiInputCapabilities {
  return {
    partialReliableThresholdMs: parsePartialReliableThresholdMs(sdp),
    hidDeviceMask: parseRiIntegerAttribute(sdp, "ri.hidDeviceMask", PARTIALLY_RELIABLE_HID_DEVICE_MASK_ALL),
    enablePartiallyReliableTransferGamepad: parseRiIntegerAttribute(
      sdp,
      "ri.enablePartiallyReliableTransferGamepad",
      PARTIALLY_RELIABLE_GAMEPAD_MASK_ALL,
    ),
    enablePartiallyReliableTransferHid: parseRiIntegerAttribute(
      sdp,
      "ri.enablePartiallyReliableTransferHid",
      PARTIALLY_RELIABLE_HID_DEVICE_MASK_ALL,
    ),
  };
}

function clampRumbleMagnitude(value: number): number {
  if (!Number.isFinite(value)) {
    return 0;
  }
  return Math.max(0, Math.min(1, value));
}

function isXboxLikeGamepad(gamepad: Gamepad): boolean {
  return /xbox|xinput/i.test(gamepad.id);
}

function getGamepadRumbleApi(gamepad: Gamepad): GamepadRumbleApi | null {
  const hapticGamepad = gamepad as GamepadWithOptionalHaptics;
  const playEffectActuator = hapticGamepad.vibrationActuator;
  const pulseActuator = hapticGamepad.hapticActuators?.[0];
  const api: GamepadRumbleApi = {
    playEffectActuator: playEffectActuator && typeof playEffectActuator.playEffect === "function"
      ? playEffectActuator
      : null,
    pulseActuator: pulseActuator && typeof pulseActuator.pulse === "function"
      ? pulseActuator
      : null,
  };
  return api.playEffectActuator || api.pulseActuator ? api : null;
}

export interface AdaptiveMouseFlushDecisionParams {
  baseIntervalMs: number;
  currentIntervalMs: number;
  reliableBufferedAmount: number;
  schedulingDelayMs: number;
  canUsePartiallyReliableMouse: boolean;
  backpressureThresholdBytes: number;
  minIntervalMs: number;
  maxIntervalMs: number;
}

export function chooseAdaptiveMouseFlushInterval(params: AdaptiveMouseFlushDecisionParams): number {
  const boundedBase = Math.max(params.minIntervalMs, Math.min(params.maxIntervalMs, params.baseIntervalMs));
  const boundedCurrent = Math.max(params.minIntervalMs, Math.min(params.maxIntervalMs, params.currentIntervalMs));
  if (!params.canUsePartiallyReliableMouse) {
    return boundedBase;
  }

  const highPressure =
    params.reliableBufferedAmount >= params.backpressureThresholdBytes / 2
    || params.schedulingDelayMs >= 4;
  if (highPressure) {
    return Math.max(boundedBase, Math.min(params.maxIntervalMs, boundedCurrent + 2));
  }

  const lowPressure = params.reliableBufferedAmount <= 4096 && params.schedulingDelayMs <= 1;
  if (lowPressure) {
    return Math.max(params.minIntervalMs, boundedCurrent - 1);
  }

  if (boundedCurrent > boundedBase) {
    return Math.max(boundedBase, boundedCurrent - 1);
  }
  if (boundedCurrent < boundedBase) {
    return Math.min(boundedBase, boundedCurrent + 1);
  }
  return boundedCurrent;
}

export function quantizeMouseDeltaWithResidual(accumulatedDelta: number): { send: number; residual: number } {
  const send = Math.round(accumulatedDelta);
  return {
    send,
    residual: accumulatedDelta - send,
  };
}

class MouseDeltaFilter {
  private x = 0;
  private y = 0;
  private lastTsMs = 0;
  private velocityX = 0;
  private velocityY = 0;
  private rejectedX = 0;
  private rejectedY = 0;
  private pendingX = 0;
  private pendingY = 0;
  private sawZero = false;
  private relaxedForRawInput = false;

  public setRelaxedForRawInput(value: boolean): void {
    this.relaxedForRawInput = value;
  }

  public getX(): number {
    return this.x;
  }

  public getY(): number {
    return this.y;
  }

  public reset(): void {
    this.x = 0;
    this.y = 0;
    this.lastTsMs = 0;
    this.velocityX = 0;
    this.velocityY = 0;
    this.rejectedX = 0;
    this.rejectedY = 0;
    this.pendingX = 0;
    this.pendingY = 0;
    this.sawZero = false;
  }

  public update(dx: number, dy: number, tsMs: number): boolean {
    if (dx === 0 && dy === 0) {
      if (this.sawZero) {
        this.pendingX = 0;
        this.pendingY = 0;
      } else {
        this.sawZero = true;
      }
      return false;
    }

    this.sawZero = false;
    if (this.pendingX === 0 && this.pendingY === 0) {
      if (tsMs < this.lastTsMs) {
        this.pendingX = dx;
        this.pendingY = dy;
        return false;
      }
    } else {
      dx += this.pendingX;
      dy += this.pendingY;
      this.pendingX = 0;
      this.pendingY = 0;
    }

    const dot = dx * this.x + dy * this.y;
    const magIncoming = dx * dx + dy * dy;
    const magPrev = this.x * this.x + this.y * this.y;
    let accept = true;

    const dtMs = tsMs - this.lastTsMs;
    const directionReversalCosineThreshold = this.relaxedForRawInput ? 0.89 : 0.81;
    if (dtMs < 0.95 && dot < 0 && magPrev !== 0 && dot * dot > directionReversalCosineThreshold * magIncoming * magPrev) {
      const ratio = Math.sqrt(magIncoming) / Math.sqrt(magPrev);
      let distToInt = Math.abs(ratio - Math.trunc(ratio));
      if (distToInt > 0.5) {
        distToInt = 1 - distToInt;
      }
      const intRatioRejectThreshold = this.relaxedForRawInput ? 0.07 : 0.1;
      if (distToInt < intRatioRejectThreshold) {
        accept = false;
      }
    }

    const diffX = dx - this.x;
    const diffY = dy - this.y;
    const diffMag = diffX * diffX + diffY * diffY;

    if (accept) {
      const scale = 1 + 0.1 * Math.max(1, Math.min(16, dtMs));
      const vx2 = 2 * scale * Math.abs(this.velocityX);
      const vy2 = 2 * scale * Math.abs(this.velocityY);
      const threshold = Math.max(this.relaxedForRawInput ? 9800 : 8100, vx2 * vx2 + vy2 * vy2);
      accept = diffMag < threshold;
      if (!accept && (this.rejectedX !== 0 || this.rejectedY !== 0)) {
        const rx = dx - this.rejectedX;
        const ry = dy - this.rejectedY;
        accept = rx * rx + ry * ry < threshold;
      }
    }

    if (accept) {
      this.velocityX = 0.4 * this.velocityX + 0.6 * diffX;
      this.velocityY = 0.4 * this.velocityY + 0.6 * diffY;
      this.x = dx;
      this.y = dy;
      this.lastTsMs = tsMs;
      this.rejectedX = 0;
      this.rejectedY = 0;
      return true;
    }

    this.rejectedX = dx;
    this.rejectedY = dy;
    return false;
  }
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
  private partiallyReliableInputChannel: RTCDataChannel | null = null;
  private controlChannel: RTCDataChannel | null = null;
  private audioContext: AudioContext | null = null;
  private audioSourceNode: MediaStreamAudioSourceNode | null = null;
  private audioGainNode: GainNode | null = null;
  private outputVolume = 1;

  private inputReady = false;
  /** When true, the host (e.g. in-stream controller menu) blocks forwarding; not cleared by focus/visibility. */
  public inputPaused = false;
  /** When true, window blur or document hidden blocks forwarding until focus/visible again. */
  private windowStateInputPaused = false;
  private inputProtocolVersion = 2;
  private heartbeatTimer: number | null = null;
  private mouseFlushTimer: number | null = null;
  private statsTimer: number | null = null;
  private statsPollInFlight = false;
  private gamepadPollTimer: number | null = null;
  private pendingMouseDxFloat = 0;
  private pendingMouseDyFloat = 0;
  private inputCleanup: Array<() => void> = [];
  private queuedCandidates: RTCIceCandidateInit[] = [];

  // Input mode: all input types (mouse, keyboard, gamepad) work simultaneously
  // Removed exclusive mode switching to allow concurrent input
  // Timestamp of last gamepad packet sent — used for keepalive
  private lastGamepadSendMs = 0;
  // Gamepad keepalive interval: resend last state every 100ms to keep server controller alive
  private static readonly GAMEPAD_KEEPALIVE_MS = 100;
  private static readonly MOUSE_FLUSH_FAST_MS = 4;
  private static readonly MOUSE_FLUSH_NORMAL_MS = 8;
  private static readonly MOUSE_FLUSH_SAFE_MS = 16;
  private static readonly MOUSE_FLUSH_MIN_MS = 2;
  private static readonly MOUSE_FLUSH_MAX_MS = 20;
  private static readonly DEFAULT_PARTIAL_RELIABLE_THRESHOLD_MS = 300;
  private static readonly RELIABLE_MOUSE_BACKPRESSURE_BYTES = 64 * 1024;
  private static readonly BACKPRESSURE_LOG_INTERVAL_MS = 2000;
  private static readonly VIDEO_BASE_JITTER_TARGET_MS = 12;
  private static readonly AUDIO_BASE_JITTER_TARGET_MS = 20;
  private static readonly VIDEO_PRESSURE_JITTER_TARGET_MS = 30;
  private static readonly AUDIO_PRESSURE_JITTER_TARGET_MS = 32;
  private static readonly DECODER_PRESSURE_CONSECUTIVE_POLLS = 3;
  private static readonly DECODER_STABLE_CONSECUTIVE_POLLS = 6;
  private static readonly DECODER_RECOVERY_COOLDOWN_MS = 1500;
  private static readonly DECODER_KEYFRAME_COOLDOWN_MS = 1200;
  private static readonly DECODER_BITRATE_STEP_FACTOR = 0.85;
  private static readonly DECODER_MIN_RECOVERY_BITRATE_KBPS = 4000;
  private static readonly RUMBLE_EFFECT_MS = 500;
  private static readonly RUMBLE_THROTTLE_MS = 500;
  private static readonly HAPTICS_LOG_INTERVAL_MS = 5000;

  // Gamepad bitmap sent at packet offset 8, matching official client's this.nu field:
  // bit i (0-3) = connected, bit i+8 = Xbox/xinput style device.
  // Haptics availability is advertised separately with input event type 13.
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
  private gamepadMetaPressed: Map<number, boolean> = new Map();
  private lastEmittedDiagnostics: StreamDiagnostics | null = null;
  private previousGamepadStates: Map<number, GamepadInput> = new Map();
  private lastRumbleWeak: number[] = [0, 0, 0, 0];
  private lastRumbleStrong: number[] = [0, 0, 0, 0];
  private lastRumbleEffectAtMs: number[] = [0, 0, 0, 0];
  private hapticsSupportLogged: boolean[] = [false, false, false, false];
  private fallbackHapticsSupportLogged: boolean[] = [false, false, false, false];
  private lastHapticsWarningAtMs = 0;
  private hapticsAdvertised = false;

  // Track currently pressed keys (VK codes) for synthetic Escape detection
  private pressedKeys: Set<number> = new Set();
  // Pointer lock target reference for lock re-acquisition
  private pointerLockTarget: HTMLElement | null = null;
  // Auto-pointer-lock in progress flag
  private autoPointerLockInProgress = false;
  // Timer for synthetic Escape on pointer lock loss
  private pointerLockEscapeTimer: number | null = null;
  // Timer for restoring pointer lock after Escape releases it.
  private pointerLockRelockTimer: number | null = null;
  // Skip one synthetic Escape on pointer loss when lock was released intentionally (e.g. F8).
  private suppressNextSyntheticEscape = false;
  private keyboardLockState: "unknown" | "unsupported" | "locked" | "failed" = "unknown";
  private mouseBackpressureLoggedAtMs = 0;
  private mouseFlushBaseIntervalMs = GfnWebRtcClient.MOUSE_FLUSH_NORMAL_MS;
  private mouseAdaptiveFlushActive = false;
  private mousePacketsSentInWindow = 0;
  private mousePacketsPerSecond = 0;
  private mousePacketRateWindowStartedAtMs = 0;
  private mouseFlushIntervalMs = GfnWebRtcClient.MOUSE_FLUSH_NORMAL_MS;
  private mouseFlushLastTickMs = 0;
  private pendingMouseTimestampUs: bigint | null = null;
  private mouseDeltaFilter = new MouseDeltaFilter();
  private mouseSensitivity = 1;
  private mouseAccelerationPercent = 1;
  private autoFullScreenEnabled = true;

  private partialReliableThresholdMs = GfnWebRtcClient.DEFAULT_PARTIAL_RELIABLE_THRESHOLD_MS;
  private riInputCapabilities: RiInputCapabilities = {
    partialReliableThresholdMs: GfnWebRtcClient.DEFAULT_PARTIAL_RELIABLE_THRESHOLD_MS,
    hidDeviceMask: PARTIALLY_RELIABLE_HID_DEVICE_MASK_ALL,
    enablePartiallyReliableTransferGamepad: PARTIALLY_RELIABLE_GAMEPAD_MASK_ALL,
    enablePartiallyReliableTransferHid: PARTIALLY_RELIABLE_HID_DEVICE_MASK_ALL,
  };
  private inputQueuePeakBufferedBytesWindow = 0;
  private partiallyReliableInputQueuePeakBufferedBytesWindow = 0;
  private inputQueueMaxSchedulingDelayMsWindow = 0;
  private inputQueuePressureLoggedAtMs = 0;
  private inputQueueDropCount = 0;

  // Decoder pressure detection + recovery state.
  private decoderPressureActive = false;
  private decoderPressureConsecutivePolls = 0;
  private decoderStableConsecutivePolls = 0;
  private decoderRecoveryAttemptCount = 0;
  private lastDecoderRecoveryAtMs = 0;
  private lastDecoderKeyframeRequestAtMs = 0;
  private negotiatedMaxBitrateKbps = 0;
  private currentBitrateCeilingKbps = 0;
  private receiverLatencyTargets = {
    video: GfnWebRtcClient.VIDEO_BASE_JITTER_TARGET_MS,
    audio: GfnWebRtcClient.AUDIO_BASE_JITTER_TARGET_MS,
  };
  private activeReceivers: Array<{ receiver: RTCRtpReceiver; kind: "audio" | "video" }> = [];

  // Microphone
  private micManager: MicrophoneManager | null = null;
  private micState: MicState = "uninitialized";

  // Stream info
  private currentCodec = "";
  private currentResolution = "";
  private isHdr = false;
  private videoDecodeStallWarningSent = false;
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
    inputQueueBufferedBytes: 0,
    inputQueuePeakBufferedBytes: 0,
    partiallyReliableInputQueueBufferedBytes: 0,
    partiallyReliableInputQueuePeakBufferedBytes: 0,
    inputQueueDropCount: 0,
    inputQueueMaxSchedulingDelayMs: 0,
    partiallyReliableInputOpen: false,
    mouseMoveTransport: "reliable",
    mouseFlushIntervalMs: GfnWebRtcClient.MOUSE_FLUSH_NORMAL_MS,
    mousePacketsPerSecond: 0,
    mouseResidualMagnitude: 0,
    mouseAdaptiveFlushActive: false,
    lagReason: "unknown",
    lagReasonDetail: "Waiting for stream stats",
    gpuType: "",
    serverRegion: "",
    decoderPressureActive: false,
    decoderRecoveryAttempts: 0,
    decoderRecoveryAction: "none",
    micState: "uninitialized",
    micEnabled: false,
  };

  constructor(private readonly options: ClientOptions) {
    options.videoElement.srcObject = this.videoStream;
    options.audioElement.srcObject = this.audioStream;
    options.audioElement.muted = true;
    options.audioElement.volume = this.outputVolume;
    this.mouseSensitivity = options.mouseSensitivity ?? 1;
    this.mouseAccelerationPercent = Math.max(1, Math.min(150, Math.round(options.mouseAcceleration ?? 1)));
    this.autoFullScreenEnabled = options.autoFullScreen !== false;

    // Configure video element for lowest latency playback
    this.configureVideoElementForLowLatency(options.videoElement);

    // Detect GPU once on construction
    this.gpuType = detectGpuType();
    this.diagnostics.gpuType = this.gpuType;

    // Initialize microphone manager if mode is enabled
    const micMode = options.microphoneMode ?? "disabled";
    if (micMode !== "disabled" && MicrophoneManager.isSupported()) {
      this.micManager = new MicrophoneManager();
      this.micManager.setOnStateChange((state) => {
        this.micState = state.state;
        this.diagnostics.micState = state.state;
        this.diagnostics.micEnabled = this.micManager?.isEnabled() ?? false;
        this.emitStats();
        this.options.onMicStateChange?.(state);
      });
      if (options.microphoneDeviceId) {
        this.micManager.setDeviceId(options.microphoneDeviceId);
      }
    }
  }

  private shouldAutoFullscreen(): boolean {
    return this.autoFullScreenEnabled;
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

    // Disable picture-in-picture to prevent additional compositor layers
    video.disablePictureInPicture = true;

    // Ensure no preload buffering (we get frames via WebRTC, not a URL)
    video.preload = "none";

    // Set playback rate to 1.0 explicitly (some browsers may adjust)
    video.playbackRate = 1.0;
    video.defaultPlaybackRate = 1.0;

    this.log("Video element configured for low-latency playback");
  }

  /** Update mouse sensitivity multiplier at runtime. */
  public setMouseSensitivity(value: number): void {
    const v = Number.isFinite(value) ? value : 1;
    this.mouseSensitivity = Math.max(0.01, v);
    this.log(`Mouse sensitivity set to ${this.mouseSensitivity}`);
  }

  /** Update software mouse acceleration strength at runtime (1-150%). */
  public setMouseAccelerationPercent(value: number): void {
    const v = Number.isFinite(value) ? value : 1;
    this.mouseAccelerationPercent = Math.max(1, Math.min(150, Math.round(v)));
    this.log(`Mouse acceleration set to ${this.mouseAccelerationPercent}%`);
  }

  /** Update fullscreen preference used by auto pointer-lock flows at runtime. */
  public setAutoFullScreen(value: boolean): void {
    this.autoFullScreenEnabled = Boolean(value);
    this.log(`Auto fullscreen ${this.autoFullScreenEnabled ? "enabled" : "disabled"}`);
  }

  /**
   * Replace the b=AS bandwidth line in video sections of an SDP string.
   * Unlike mungeAnswerSdp, this is safe to call on an already-munged SDP
   * because it replaces the existing line rather than injecting a new one.
   */
  private replaceVideoBitrateInSdp(sdp: string, maxBitrateKbps: number): string {
    const lines = sdp.split("\r\n");
    let inVideoSection = false;
    let bitrateReplaced = false;
    const result: string[] = [];
    for (const line of lines) {
      if (line.startsWith("m=")) {
        inVideoSection = line.startsWith("m=video");
        bitrateReplaced = false;
      }
      if (inVideoSection && !bitrateReplaced && line.startsWith("b=AS:")) {
        result.push(`b=AS:${maxBitrateKbps}`);
        bitrateReplaced = true;
        continue;
      }
      result.push(line);
    }
    return result.join("\r\n");
  }

  /**
   * Update the maximum receive bitrate ceiling mid-stream by replacing b=AS
   * in the local SDP and re-applying it. Chrome/Electron honours this change
   * without requiring a full ICE renegotiation.
   */
  public async setMaxBitrateKbps(kbps: number): Promise<void> {
    if (!this.pc || !this.pc.localDescription) {
      return;
    }
    const updatedSdp = this.replaceVideoBitrateInSdp(
      this.pc.localDescription.sdp,
      kbps,
    );
    try {
      await this.pc.setLocalDescription(
        new RTCSessionDescription({ type: this.pc.localDescription.type, sdp: updatedSdp }),
      );
      this.log(`Bitrate ceiling updated to ${kbps} kbps via local SDP`);
    } catch (err) {
      this.log(`setMaxBitrateKbps failed (non-fatal): ${String(err)}`);
    }
  }

  /**
   * Configure an RTCRtpReceiver for minimum jitter buffer delay.
   *
   * jitterBufferTarget controls how long Chrome holds decoded frames before
   * displaying them. Setting to 0 tells the browser to use the absolute
   * minimum buffer — effectively "display as soon as decoded". This is
   * aggressive but correct for cloud gaming where we prioritize latency
   * over smoothness.
   *
   * The official GFN browser client doesn't set this at all (defaulting to
   * ~100-200ms). As an Electron app we can be more aggressive.
   *
   */
  private configureReceiverForLowLatency(receiver: RTCRtpReceiver, kind: string): void {
    if (kind !== "video" && kind !== "audio") {
      return;
    }

    this.registerReceiver(receiver, kind);

    try {
      const targetMs = this.receiverLatencyTargets[kind];
      const rawReceiver = receiver as unknown as Record<string, unknown>;

      if ("jitterBufferTarget" in receiver) {
        rawReceiver.jitterBufferTarget = targetMs;
        this.log(`${kind} receiver: jitterBufferTarget set to ${targetMs}ms`);
      }

      if ("playoutDelayHint" in receiver) {
        const playoutDelaySeconds = targetMs / 1000;
        rawReceiver.playoutDelayHint = playoutDelaySeconds;
        this.log(`${kind} receiver: playoutDelayHint set to ${playoutDelaySeconds}s`);
      }

      if (kind === "video" && "contentHint" in receiver.track) {
        receiver.track.contentHint = "motion";
      }
    } catch (error) {
      this.log(`Warning: could not apply ${kind} low-latency receiver tuning: ${String(error)}`);
    }
  }

  private registerReceiver(receiver: RTCRtpReceiver, kind: "audio" | "video"): void {
    const alreadyRegistered = this.activeReceivers.some((entry) => entry.receiver === receiver);
    if (!alreadyRegistered) {
      this.activeReceivers.push({ receiver, kind });
    }
  }

  private applyReceiverLatencyTargets(): void {
    for (const entry of this.activeReceivers) {
      this.configureReceiverForLowLatency(entry.receiver, entry.kind);
    }
  }

  private setDecoderPressureMode(active: boolean): void {
    if (this.decoderPressureActive === active) {
      return;
    }

    this.decoderPressureActive = active;
    this.diagnostics.decoderPressureActive = active;
    this.receiverLatencyTargets.video = active
      ? GfnWebRtcClient.VIDEO_PRESSURE_JITTER_TARGET_MS
      : GfnWebRtcClient.VIDEO_BASE_JITTER_TARGET_MS;
    this.receiverLatencyTargets.audio = active
      ? GfnWebRtcClient.AUDIO_PRESSURE_JITTER_TARGET_MS
      : GfnWebRtcClient.AUDIO_BASE_JITTER_TARGET_MS;
    this.log(
      `Decoder pressure mode ${active ? "enabled" : "cleared"}; receiver targets video=${this.receiverLatencyTargets.video}ms audio=${this.receiverLatencyTargets.audio}ms`,
    );
    this.applyReceiverLatencyTargets();
  }

  private log(message: string): void {
    this.options.onLog(message);
  }

  private diagnosticsChangedSinceLastEmit(): boolean {
    if (!this.lastEmittedDiagnostics) return true;
    const current = this.diagnostics as unknown as Record<string, unknown>;
    const previous = this.lastEmittedDiagnostics as unknown as Record<string, unknown>;
    const keys = Object.keys(current);
    for (const key of keys) {
      if (!Object.is(current[key], previous[key])) {
        return true;
      }
    }
    return false;
  }

  private emitStats(force = false): void {
    if (!this.options.onStats) return;
    if (!force && !this.diagnosticsChangedSinceLastEmit()) return;
    const snapshot = { ...this.diagnostics };
    this.lastEmittedDiagnostics = snapshot;
    this.options.onStats(snapshot);
  }

  private resetDecoderRecoveryState(): void {
    this.decoderPressureActive = false;
    this.decoderPressureConsecutivePolls = 0;
    this.decoderStableConsecutivePolls = 0;
    this.decoderRecoveryAttemptCount = 0;
    this.lastDecoderRecoveryAtMs = 0;
    this.lastDecoderKeyframeRequestAtMs = 0;
    this.negotiatedMaxBitrateKbps = 0;
    this.currentBitrateCeilingKbps = 0;
    this.receiverLatencyTargets.video = GfnWebRtcClient.VIDEO_BASE_JITTER_TARGET_MS;
    this.receiverLatencyTargets.audio = GfnWebRtcClient.AUDIO_BASE_JITTER_TARGET_MS;
    this.activeReceivers = [];
    this.diagnostics.decoderPressureActive = false;
    this.diagnostics.decoderRecoveryAttempts = 0;
    this.diagnostics.decoderRecoveryAction = "none";
  }

  private resetDiagnostics(): void {
    this.lastStatsSample = null;
    this.lastEmittedDiagnostics = null;
    this.currentCodec = "";
    this.currentResolution = "";
    this.isHdr = false;
    this.videoDecodeStallWarningSent = false;
    this.resetDecoderRecoveryState();
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
      inputQueueBufferedBytes: 0,
      inputQueuePeakBufferedBytes: 0,
      partiallyReliableInputQueueBufferedBytes: 0,
      partiallyReliableInputQueuePeakBufferedBytes: 0,
      inputQueueDropCount: 0,
      inputQueueMaxSchedulingDelayMs: 0,
      partiallyReliableInputOpen: false,
      mouseMoveTransport: "reliable",
      mouseFlushIntervalMs: this.mouseFlushIntervalMs,
      mousePacketsPerSecond: this.mousePacketsPerSecond,
      mouseResidualMagnitude: 0,
      mouseAdaptiveFlushActive: this.mouseAdaptiveFlushActive,
      lagReason: "unknown",
      lagReasonDetail: "Waiting for stream stats",
      gpuType: this.gpuType,
      serverRegion: this.serverRegion,
      decoderPressureActive: false,
      decoderRecoveryAttempts: 0,
      decoderRecoveryAction: "none",
      micState: this.micState,
      micEnabled: this.micManager?.isEnabled() ?? false,
    };
    this.emitStats();
  }

  private resetInputState(): void {
    this.inputReady = false;
    this.inputProtocolVersion = 2;
    this.hapticsAdvertised = false;
    this.inputEncoder.setProtocolVersion(2);
    this.diagnostics.inputReady = false;
    this.diagnostics.partiallyReliableInputOpen = false;
    this.diagnostics.mouseMoveTransport = "reliable";
    this.emitStats();
  }

  private closeDataChannels(): void {
    if (this.controlChannel) {
      this.controlChannel.onmessage = null;
      this.controlChannel.onclose = null;
      this.controlChannel.onerror = null;
    }
    this.reliableInputChannel?.close();
    this.partiallyReliableInputChannel?.close();
    this.controlChannel?.close();
    this.reliableInputChannel = null;
    this.partiallyReliableInputChannel = null;
    this.controlChannel = null;
  }

  private clearTimers(): void {
    if (this.heartbeatTimer !== null) {
      window.clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
    if (this.mouseFlushTimer !== null) {
      window.clearTimeout(this.mouseFlushTimer);
      this.mouseFlushTimer = null;
    }
    if (this.statsTimer !== null) {
      window.clearInterval(this.statsTimer);
      this.statsTimer = null;
    }
    if (this.gamepadPollTimer !== null) {
      window.clearTimeout(this.gamepadPollTimer);
      this.gamepadPollTimer = null;
    }
  }

  private setupStatsPolling(): void {
    if (this.statsTimer !== null) {
      window.clearInterval(this.statsTimer);
    }

    this.statsTimer = window.setInterval(() => {
      if (this.statsPollInFlight) {
        return;
      }
      this.statsPollInFlight = true;
      void this.collectStats().finally(() => {
        this.statsPollInFlight = false;
      });
    }, 500);
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

  private shouldTreatAsDecoderPressure(params: {
    framesReceived: number;
    framesDecoded: number;
    framesDropped: number;
    decodeTimeMs: number;
    decodeFps: number;
    prevSample: {
      framesReceived: number;
      framesDecoded: number;
      framesDropped: number;
    } | null;
  }): { active: boolean; reason: string; backlogFrames: number; dropRatePercent: number } {
    const backlogFrames = Math.max(0, params.framesReceived - params.framesDecoded);
    const dropRatePercent = params.framesReceived > 0
      ? (params.framesDropped / params.framesReceived) * 100
      : 0;
    const severeStall = params.framesReceived > 120 && params.framesDecoded === 0;
    const backlogHigh = backlogFrames >= 45;
    const dropRateHigh = dropRatePercent >= 6;

    let dropBurst = false;
    if (params.prevSample) {
      const decodedDelta = params.framesDecoded - params.prevSample.framesDecoded;
      const droppedDelta = params.framesDropped - params.prevSample.framesDropped;
      dropBurst = droppedDelta >= 8 && decodedDelta <= 4;
    }

    let decodeSaturated = false;
    if (params.decodeFps > 0 && params.decodeTimeMs > 0) {
      const frameBudgetMs = 1000 / params.decodeFps;
      decodeSaturated = params.decodeTimeMs >= frameBudgetMs * 0.82;
    }

    if (severeStall) {
      return {
        active: true,
        reason: "severe_stall",
        backlogFrames,
        dropRatePercent,
      };
    }

    const active = (backlogHigh && (dropRateHigh || dropBurst || decodeSaturated))
      || (dropBurst && decodeSaturated);
    const reason = active
      ? (backlogHigh
        ? "backlog_and_drop"
        : "decode_saturated")
      : "stable";

    return {
      active,
      reason,
      backlogFrames,
      dropRatePercent,
    };
  }

  private classifyLagReason(params: {
    framesReceived: number;
    framesDecoded: number;
    framesDropped: number;
    decodeTimeMs: number;
    decodeFps: number;
    renderFps: number;
    rttMs: number;
    packetLossPercent: number;
    jitterMs: number;
    jitterBufferDelayMs: number;
    inputQueueBufferedBytes: number;
    inputQueueDropCount: number;
    inputQueueMaxSchedulingDelayMs: number;
  }): { reason: StreamLagReason; detail: string } {
    const networkSignals: string[] = [];
    if (params.packetLossPercent >= 1) networkSignals.push(`${params.packetLossPercent.toFixed(1)}% loss`);
    if (params.rttMs >= 75) networkSignals.push(`RTT ${params.rttMs.toFixed(0)}ms`);
    if (params.jitterMs >= 12) networkSignals.push(`jitter ${params.jitterMs.toFixed(1)}ms`);
    if (params.jitterBufferDelayMs >= 20) networkSignals.push(`buffer ${params.jitterBufferDelayMs.toFixed(1)}ms`);
    if (networkSignals.length > 0) {
      return {
        reason: "network",
        detail: networkSignals.join(" · "),
      };
    }

    const frameBudgetMs = params.decodeFps > 0 ? 1000 / params.decodeFps : 0;
    const decodeSaturated =
      frameBudgetMs > 0 &&
      params.decodeTimeMs > 0 &&
      params.decodeTimeMs >= frameBudgetMs * 0.82;
    const severeDecoderStall = params.framesReceived > 100 && params.framesDecoded === 0;
    const decoderBacklog = Math.max(0, params.framesReceived - params.framesDecoded);
    if (severeDecoderStall || decodeSaturated || decoderBacklog >= 45 || params.framesDropped >= 8) {
      const detailParts: string[] = [];
      if (severeDecoderStall) detailParts.push("frames received but not decoded");
      if (decodeSaturated) detailParts.push(`decode ${params.decodeTimeMs.toFixed(1)}ms`);
      if (decoderBacklog >= 45) detailParts.push(`backlog ${decoderBacklog}`);
      if (params.framesDropped >= 8) detailParts.push(`drops ${params.framesDropped}`);
      return {
        reason: "decoder",
        detail: detailParts.join(" · ") || "decode saturation",
      };
    }

    if (
      params.inputQueueDropCount > 0 ||
      params.inputQueueBufferedBytes >= GfnWebRtcClient.RELIABLE_MOUSE_BACKPRESSURE_BYTES ||
      params.inputQueueMaxSchedulingDelayMs >= 4
    ) {
      const detailParts: string[] = [];
      if (params.inputQueueDropCount > 0) detailParts.push(`drops ${params.inputQueueDropCount}`);
      if (params.inputQueueBufferedBytes >= GfnWebRtcClient.RELIABLE_MOUSE_BACKPRESSURE_BYTES) {
        detailParts.push(`buffered ${(params.inputQueueBufferedBytes / 1024).toFixed(1)}KB`);
      }
      if (params.inputQueueMaxSchedulingDelayMs >= 4) {
        detailParts.push(`sched ${params.inputQueueMaxSchedulingDelayMs.toFixed(1)}ms`);
      }
      return {
        reason: "input_backpressure",
        detail: detailParts.join(" · "),
      };
    }

    if (params.renderFps > 0 && params.decodeFps > 0) {
      const renderGap = params.decodeFps - params.renderFps;
      if (renderGap >= 8 || params.renderFps < 24) {
        return {
          reason: "render",
          detail: `render ${params.renderFps}fps vs decode ${params.decodeFps}fps`,
        };
      }
    }

    return {
      reason: params.decodeFps > 0 || params.renderFps > 0 ? "stable" : "unknown",
      detail: params.decodeFps > 0 || params.renderFps > 0
        ? "No dominant lag source detected"
        : "Waiting for stream stats",
    };
  }

  private async requestDecoderKeyframe(backlogFrames: number, reason: string): Promise<boolean> {
    const now = performance.now();
    if (now - this.lastDecoderKeyframeRequestAtMs < GfnWebRtcClient.DECODER_KEYFRAME_COOLDOWN_MS) {
      return false;
    }

    let requestedViaSender = false;
    if (this.pc) {
      for (const sender of this.pc.getSenders()) {
        if (sender.track?.kind !== "video") {
          continue;
        }
        const senderWithKeyframe = sender as RTCRtpSender & {
          requestKeyFrame?: () => Promise<void>;
        };
        if (typeof senderWithKeyframe.requestKeyFrame !== "function") {
          continue;
        }
        try {
          await senderWithKeyframe.requestKeyFrame();
          requestedViaSender = true;
        } catch (error) {
          this.log(`requestKeyFrame failed on sender (non-fatal): ${String(error)}`);
        }
      }
    }

    if (!requestedViaSender && this.controlChannel?.readyState === "open") {
      try {
        this.controlChannel.send(JSON.stringify({
          type: "request_keyframe",
          reason,
          backlogFrames,
          attempt: this.decoderRecoveryAttemptCount + 1,
        }));
        requestedViaSender = true;
        this.diagnostics.decoderRecoveryAction = "control_channel_keyframe";
      } catch (error) {
        this.log(`control_channel keyframe request failed (non-fatal): ${String(error)}`);
      }
    }

    if (!requestedViaSender) {
      try {
        await window.openNow.requestKeyframe({
          reason,
          backlogFrames,
          attempt: this.decoderRecoveryAttemptCount + 1,
        });
        requestedViaSender = true;
        this.diagnostics.decoderRecoveryAction = "signaling_keyframe";
      } catch (error) {
        this.log(`signaling keyframe request failed (non-fatal): ${String(error)}`);
      }
    }

    if (requestedViaSender) {
      this.lastDecoderKeyframeRequestAtMs = now;
      if (this.diagnostics.decoderRecoveryAction === "none") {
        this.diagnostics.decoderRecoveryAction = "sender_keyframe";
      }
      this.log(
        `Decoder recovery: keyframe requested (reason=${reason}, backlog=${backlogFrames}, attempt=${this.decoderRecoveryAttemptCount + 1})`,
      );
      return true;
    }

    return false;
  }

  private async reduceBitrateForDecoderRecovery(): Promise<boolean> {
    if (!this.pc || !this.pc.localDescription) {
      return false;
    }

    const current = this.currentBitrateCeilingKbps > 0
      ? this.currentBitrateCeilingKbps
      : this.negotiatedMaxBitrateKbps;
    if (current <= GfnWebRtcClient.DECODER_MIN_RECOVERY_BITRATE_KBPS) {
      return false;
    }

    const next = Math.max(
      GfnWebRtcClient.DECODER_MIN_RECOVERY_BITRATE_KBPS,
      Math.floor(current * GfnWebRtcClient.DECODER_BITRATE_STEP_FACTOR),
    );
    if (next >= current) {
      return false;
    }

    await this.setMaxBitrateKbps(next);
    this.currentBitrateCeilingKbps = next;
    this.diagnostics.decoderRecoveryAction = "bitrate_step_down";
    this.log(`Decoder recovery: bitrate ceiling stepped down ${current} -> ${next} kbps`);
    return true;
  }

  private async maybeRecoverFromDecoderPressure(signal: {
    active: boolean;
    reason: string;
    backlogFrames: number;
    dropRatePercent: number;
  }): Promise<void> {
    if (!signal.active) {
      this.decoderPressureConsecutivePolls = 0;
      this.decoderStableConsecutivePolls++;
      if (this.decoderStableConsecutivePolls >= GfnWebRtcClient.DECODER_STABLE_CONSECUTIVE_POLLS) {
        this.decoderRecoveryAttemptCount = 0;
        this.diagnostics.decoderRecoveryAttempts = 0;
        this.diagnostics.decoderRecoveryAction = "none";
        this.setDecoderPressureMode(false);
      }
      return;
    }

    this.decoderStableConsecutivePolls = 0;
    this.decoderPressureConsecutivePolls++;

    if (this.decoderPressureConsecutivePolls < GfnWebRtcClient.DECODER_PRESSURE_CONSECUTIVE_POLLS) {
      return;
    }

    this.setDecoderPressureMode(true);

    const now = performance.now();
    if (now - this.lastDecoderRecoveryAtMs < GfnWebRtcClient.DECODER_RECOVERY_COOLDOWN_MS) {
      return;
    }

    const keyframeRequested = await this.requestDecoderKeyframe(signal.backlogFrames, signal.reason);

    let bitrateReduced = false;
    if (!keyframeRequested || this.decoderRecoveryAttemptCount >= 1) {
      bitrateReduced = await this.reduceBitrateForDecoderRecovery();
    }

    if (keyframeRequested || bitrateReduced) {
      this.decoderRecoveryAttemptCount++;
      this.diagnostics.decoderRecoveryAttempts = this.decoderRecoveryAttemptCount;
      this.lastDecoderRecoveryAtMs = now;
      this.log(
        `Decoder pressure detected: reason=${signal.reason}, backlog=${signal.backlogFrames}, dropRate=${signal.dropRatePercent.toFixed(1)}%, recoveryAttempt=${this.decoderRecoveryAttemptCount}`,
      );
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
    let framesReceived = 0;
    let framesDecoded = 0;
    let framesDropped = 0;

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
      framesReceived = Number(inboundVideo.framesReceived ?? 0);
      framesDecoded = Number(inboundVideo.framesDecoded ?? 0);
      framesDropped = Number(inboundVideo.framesDropped ?? 0);
      const packetsReceived = Number(inboundVideo.packetsReceived ?? 0);
      const packetsLost = Number(inboundVideo.packetsLost ?? 0);
      const prevSample = this.lastStatsSample;

      // Calculate bitrate
      if (prevSample) {
        const bytesDelta = bytes - prevSample.bytesReceived;
        const timeDeltaMs = now - prevSample.atMs;
        if (bytesDelta >= 0 && timeDeltaMs > 0) {
          const kbps = (bytesDelta * 8) / (timeDeltaMs / 1000) / 1000;
          this.diagnostics.bitrateKbps = Math.max(0, Math.round(kbps));
        }

        // Calculate packet loss percentage over the interval
        const packetsDelta = packetsReceived - prevSample.packetsReceived;
        const lostDelta = packetsLost - prevSample.packetsLost;
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

      if (
        !this.videoDecodeStallWarningSent &&
        framesReceived > 100 &&
        framesDecoded === 0
      ) {
        this.videoDecodeStallWarningSent = true;
        this.log("Warning: inbound video packets received but 0 frames decoded (decoder stall)");
      }

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

      const pressureSignal = this.shouldTreatAsDecoderPressure({
        framesReceived,
        framesDecoded,
        framesDropped,
        decodeTimeMs: this.diagnostics.decodeTimeMs,
        decodeFps: this.diagnostics.decodeFps,
        prevSample,
      });
      await this.maybeRecoverFromDecoderPressure(pressureSignal);
    }

    // RTT from active candidate pair
    if (activePair?.currentRoundTripTime !== undefined) {
      const rtt = Number(activePair.currentRoundTripTime);
      this.diagnostics.rttMs = Math.round(rtt * 1000 * 10) / 10;
    }

    const reliableBufferedAmount = this.reliableInputChannel?.bufferedAmount ?? 0;
    const partiallyReliableBufferedAmount = this.partiallyReliableInputChannel?.bufferedAmount ?? 0;
    this.inputQueuePeakBufferedBytesWindow = Math.max(
      this.inputQueuePeakBufferedBytesWindow,
      reliableBufferedAmount,
    );
    this.partiallyReliableInputQueuePeakBufferedBytesWindow = Math.max(
      this.partiallyReliableInputQueuePeakBufferedBytesWindow,
      partiallyReliableBufferedAmount,
    );
    this.diagnostics.inputQueueBufferedBytes = reliableBufferedAmount;
    this.diagnostics.inputQueuePeakBufferedBytes = this.inputQueuePeakBufferedBytesWindow;
    this.diagnostics.partiallyReliableInputQueueBufferedBytes = partiallyReliableBufferedAmount;
    this.diagnostics.partiallyReliableInputQueuePeakBufferedBytes = this.partiallyReliableInputQueuePeakBufferedBytesWindow;
    this.diagnostics.inputQueueDropCount = this.inputQueueDropCount;
    this.diagnostics.inputQueueMaxSchedulingDelayMs =
      Math.round(this.inputQueueMaxSchedulingDelayMsWindow * 10) / 10;
    this.diagnostics.partiallyReliableInputOpen = this.isPartiallyReliableChannelOpen();
    this.diagnostics.mouseMoveTransport = this.canSendInputTypePartiallyReliable(INPUT_MOUSE_REL)
      ? "partially_reliable"
      : "reliable";
    this.diagnostics.mouseFlushIntervalMs = this.mouseFlushIntervalMs;
    this.diagnostics.mousePacketsPerSecond = this.mousePacketsPerSecond;
    this.diagnostics.mouseResidualMagnitude = Math.hypot(this.pendingMouseDxFloat, this.pendingMouseDyFloat);
    this.diagnostics.mouseAdaptiveFlushActive = this.mouseAdaptiveFlushActive;

    const lagClassification = this.classifyLagReason({
      framesReceived,
      framesDecoded,
      framesDropped,
      decodeTimeMs: this.diagnostics.decodeTimeMs,
      decodeFps: this.diagnostics.decodeFps,
      renderFps: this.diagnostics.renderFps,
      rttMs: this.diagnostics.rttMs,
      packetLossPercent: this.diagnostics.packetLossPercent,
      jitterMs: this.diagnostics.jitterMs,
      jitterBufferDelayMs: this.diagnostics.jitterBufferDelayMs,
      inputQueueBufferedBytes: reliableBufferedAmount,
      inputQueueDropCount: this.inputQueueDropCount,
      inputQueueMaxSchedulingDelayMs: this.diagnostics.inputQueueMaxSchedulingDelayMs,
    });
    this.diagnostics.lagReason = lagClassification.reason;
    this.diagnostics.lagReasonDetail = lagClassification.detail;

    const shouldLogQueuePressure =
      reliableBufferedAmount > GfnWebRtcClient.RELIABLE_MOUSE_BACKPRESSURE_BYTES / 2
      || this.inputQueueMaxSchedulingDelayMsWindow >= 4
      || this.inputQueueDropCount > 0;

    if (shouldLogQueuePressure) {
      const nowMs = performance.now();
      if (nowMs - this.inputQueuePressureLoggedAtMs >= GfnWebRtcClient.BACKPRESSURE_LOG_INTERVAL_MS) {
        this.inputQueuePressureLoggedAtMs = nowMs;
        this.log(
          `Input queue pressure: reliable=${reliableBufferedAmount}B reliablePeak=${this.inputQueuePeakBufferedBytesWindow}B pr=${partiallyReliableBufferedAmount}B prPeak=${this.partiallyReliableInputQueuePeakBufferedBytesWindow}B drops=${this.inputQueueDropCount} mouseMoveTransport=${this.diagnostics.mouseMoveTransport} maxSchedDelay=${this.diagnostics.inputQueueMaxSchedulingDelayMs.toFixed(1)}ms`,
        );
      }
    }

    this.inputQueuePeakBufferedBytesWindow = reliableBufferedAmount;
    this.partiallyReliableInputQueuePeakBufferedBytesWindow = partiallyReliableBufferedAmount;
    this.inputQueueMaxSchedulingDelayMsWindow = 0;

    this.emitStats();
  }

  private detachInputCapture(): void {
    for (const cleanup of this.inputCleanup.splice(0)) {
      cleanup();
    }
    this.stopAllGamepadRumble();
    this.updateHapticsAdvertisement(false);
  }

  private replaceTrackInStream(stream: MediaStream, track: MediaStreamTrack): void {
    const existingTracks = track.kind === "video"
      ? stream.getVideoTracks()
      : stream.getAudioTracks();

    for (const existingTrack of existingTracks) {
      stream.removeTrack(existingTrack);
    }

    stream.addTrack(track);
  }

  private cleanupAudioRouting(): void {
    if (this.audioSourceNode) {
      try {
        this.audioSourceNode.disconnect();
      } catch {
        // Ignore cleanup errors from an already-disconnected node.
      }
      this.audioSourceNode = null;
    }

    if (this.audioGainNode) {
      try {
        this.audioGainNode.disconnect();
      } catch {
        // Ignore cleanup errors from an already-disconnected node.
      }
      this.audioGainNode = null;
    }

    if (this.audioContext) {
      void this.audioContext.close().catch(() => {});
      this.audioContext = null;
    }

    this.options.audioElement.pause();
    this.options.audioElement.muted = true;
  }

  private startDirectAudioPlayback(reason: string): void {
    this.log(reason);
    this.options.audioElement.muted = false;
    this.options.audioElement.volume = this.outputVolume;
    this.options.audioElement
      .play()
      .then(() => {
        this.log("Audio track attached (fallback)");
      })
      .catch((playError) => {
        this.log(`Audio autoplay blocked: ${String(playError)}`);
      });
  }

  private cleanupPeerConnection(): void {
    this.clearTimers();
    this.detachInputCapture();
    this.closeDataChannels();
    this.cleanupAudioRouting();
    if (this.pc) {
      this.pc.onicecandidate = null;
      this.pc.ontrack = null;
      this.pc.onconnectionstatechange = null;
      this.pc.ondatachannel = null;
      this.pc.close();
      this.pc = null;
    }

    // Remove old tracks so reconnects don't accumulate ended tracks in srcObject streams.
    for (const track of this.videoStream.getTracks()) {
      this.videoStream.removeTrack(track);
    }
    for (const track of this.audioStream.getTracks()) {
      this.audioStream.removeTrack(track);
    }

    this.resetInputState();
    this.resetDiagnostics();
    this.connectedGamepads.clear();
    this.previousGamepadStates.clear();
    this.gamepadSendCount = 0;
    this.lastGamepadSendMs = 0;
    this.reliableDropLogged = false;
    this.gamepadBitmap = 0;
    this.pendingMouseDxFloat = 0;
    this.pendingMouseDyFloat = 0;
    this.pendingMouseTimestampUs = null;
    this.mouseDeltaFilter.reset();
    this.mouseFlushLastTickMs = 0;
    this.mouseFlushBaseIntervalMs = GfnWebRtcClient.MOUSE_FLUSH_NORMAL_MS;
    this.mouseFlushIntervalMs = GfnWebRtcClient.MOUSE_FLUSH_NORMAL_MS;
    this.mouseAdaptiveFlushActive = false;
    this.mousePacketsSentInWindow = 0;
    this.mousePacketsPerSecond = 0;
    this.mousePacketRateWindowStartedAtMs = 0;
    this.inputQueuePeakBufferedBytesWindow = 0;
    this.partiallyReliableInputQueuePeakBufferedBytesWindow = 0;
    this.inputQueueMaxSchedulingDelayMsWindow = 0;
    this.inputQueueDropCount = 0;
    this.inputQueuePressureLoggedAtMs = 0;
    this.inputEncoder.resetGamepadSequences();
  }

  private attachTrack(track: MediaStreamTrack): void {
    if (track.kind === "video") {
      this.replaceTrackInStream(this.videoStream, track);

      // Set up render FPS tracking using video element
      const video = this.options.videoElement;
      const frameCallback = () => {
        this.updateRenderFps();
        if (this.videoStream.active) {
          video.requestVideoFrameCallback(frameCallback);
        }
      };
      video.requestVideoFrameCallback(frameCallback);

      this.log(
        `Video element before play: paused=${video.paused}, readyState=${video.readyState}, size=${video.videoWidth}x${video.videoHeight}`,
      );

      // Explicitly start video playback after track attachment.
      // Some Chromium/Electron builds keep the video element paused even with autoplay.
      video
        .play()
        .then(() => {
          this.log("Video element playback started");
        })
        .catch((playError) => {
          this.log(`Video play() failed: ${String(playError)}`);
        });

      window.setTimeout(() => {
        this.log(
          `Video element post-play: paused=${video.paused}, readyState=${video.readyState}, size=${video.videoWidth}x${video.videoHeight}`,
        );
      }, 1500);

      track.onunmute = () => {
        this.log("Video track unmuted");
      };
      track.onmute = () => {
        this.log("Warning: video track muted by sender");
      };
      track.onended = () => {
        this.log("Warning: video track ended");
      };

      this.log("Video track attached");
      return;
    }

    if (track.kind === "audio") {
      this.replaceTrackInStream(this.audioStream, track);
      this.cleanupAudioRouting();

      // Route audio through an AudioContext with interactive latency hint.
      // This tells the OS audio subsystem to use the smallest possible buffer,
      // matching what the official GFN browser client does for low-latency playback.
      let audioContext: AudioContext | null = null;
      let audioSourceNode: MediaStreamAudioSourceNode | null = null;
      let audioGainNode: GainNode | null = null;

      try {
        audioContext = new AudioContext({
          latencyHint: "interactive",
          sampleRate: 48000,
        });
        audioSourceNode = audioContext.createMediaStreamSource(this.audioStream);
        audioGainNode = audioContext.createGain();
        audioGainNode.gain.value = this.outputVolume;
        audioSourceNode.connect(audioGainNode);
        audioGainNode.connect(audioContext.destination);

        // Resume the context (browsers require user gesture, but Electron is more lenient)
        if (audioContext.state === "suspended") {
          void audioContext.resume();
        }

        this.audioContext = audioContext;
        this.audioSourceNode = audioSourceNode;
        this.audioGainNode = audioGainNode;
        this.log(
          `Audio routed through AudioContext (latency: ${(audioContext.baseLatency * 1000).toFixed(1)}ms, sampleRate: ${audioContext.sampleRate}Hz)`,
        );
      } catch (error) {
        if (audioSourceNode) {
          try {
            audioSourceNode.disconnect();
          } catch {
            // Ignore cleanup errors from a partially-created node.
          }
        }
        if (audioGainNode) {
          try {
            audioGainNode.disconnect();
          } catch {
            // Ignore cleanup errors from a partially-created node.
          }
        }
        if (audioContext) {
          void audioContext.close().catch(() => {});
        }
        this.startDirectAudioPlayback(`AudioContext creation failed, falling back to audio element: ${String(error)}`);
      }
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
      window.clearTimeout(this.gamepadPollTimer);
    }

    this.log("Gamepad polling started (adaptive)");
    this.scheduleGamepadPolling();
  }

  private scheduleGamepadPolling(): void {
    if (this.gamepadPollTimer !== null) {
      window.clearTimeout(this.gamepadPollTimer);
    }

    const nextDelay = this.getGamepadPollIntervalMs();
    this.gamepadPollTimer = window.setTimeout(() => {
      this.gamepadPollTimer = null;
      if (!this.inputReady) {
        this.scheduleGamepadPolling();
        return;
      }
      this.pollGamepads();
      this.scheduleGamepadPolling();
    }, nextDelay);
  }

  private isStreamInputBlocked(): boolean {
    return this.inputPaused || this.windowStateInputPaused;
  }

  private getGamepadPollIntervalMs(): number {
    if (!this.inputReady || this.isStreamInputBlocked() || document.visibilityState !== "visible") {
      return 100;
    }

    if (this.connectedGamepads.size === 0) {
      return 100;
    }

    return 4;
  }

  private gamepadSendCount = 0;

  private updateGamepadBitmap(controllerId: number, gamepad: Gamepad): void {
    const connectedBit = 1 << controllerId;
    const xboxBit = 1 << (controllerId + 8);
    this.gamepadBitmap |= connectedBit;
    if (isXboxLikeGamepad(gamepad)) {
      this.gamepadBitmap |= xboxBit;
    } else {
      this.gamepadBitmap &= ~xboxBit;
    }
  }

  private clearGamepadBitmap(controllerId: number): void {
    this.gamepadBitmap &= ~(1 << controllerId);
    this.gamepadBitmap &= ~(1 << (controllerId + 8));
  }

  private pollGamepads(): void {
    const streamInputBlocked = this.isStreamInputBlocked();
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
        this.updateGamepadBitmap(i, gamepad);
        const metaPressed = Boolean(gamepad.buttons[16]?.pressed);
        const prevMetaPressed = this.gamepadMetaPressed.get(i) ?? false;
        if (metaPressed && !prevMetaPressed) {
          try {
            this.options.onControllerMetaPress?.({ controllerId: i, gamepad });
          } catch {
            // Host callbacks must never break stream input polling.
          }
        }
        this.gamepadMetaPressed.set(i, metaPressed);

        // Track connected gamepads and update bitmap
        if (!this.connectedGamepads.has(i)) {
          this.connectedGamepads.add(i);
          this.log(`Gamepad ${i} connected: ${gamepad.id}`);
          this.log(`  Buttons: ${gamepad.buttons.length}, Axes: ${gamepad.axes.length}, Mapping: ${gamepad.mapping}`);
          this.log(`  Bitmap now: 0x${this.gamepadBitmap.toString(16)}`);
          this.diagnostics.connectedGamepads = this.connectedGamepads.size;
          this.emitStats();
        }

        // Read and encode gamepad state
        if (streamInputBlocked) {
          continue;
        }
        const gamepadInput = this.readGamepadState(gamepad, i);
        const stateChanged = this.hasGamepadStateChanged(i, gamepadInput);

        // Send if state changed OR as a keepalive to maintain server controller presence
        // Games detect active input device by receiving packets; if we stop sending,
        // the game falls back to showing keyboard/mouse prompts.
        const needsKeepalive = !stateChanged
          && (nowMs - this.lastGamepadSendMs) >= GfnWebRtcClient.GAMEPAD_KEEPALIVE_MS;

        if (stateChanged || needsKeepalive) {
          const usePR = this.canSendGamepadPartiallyReliable(i);
          const bytes = this.inputEncoder.encodeGamepadState(gamepadInput, this.gamepadBitmap, usePR);
          if (usePR) {
            this.sendGamepad(bytes);
          } else {
            this.sendReliable(bytes);
          }
          this.lastGamepadSendMs = nowMs;

          if (stateChanged) {
            this.previousGamepadStates.set(i, { ...gamepadInput });
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
        this.stopGamepadRumble(i, gamepad ?? undefined);
        this.connectedGamepads.delete(i);
        this.gamepadMetaPressed.delete(i);
        this.previousGamepadStates.delete(i);
        this.clearGamepadBitmap(i);
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
        const usePR = this.canSendGamepadPartiallyReliable(i);
        const bytes = this.inputEncoder.encodeGamepadState(disconnectState, this.gamepadBitmap, usePR);
        if (usePR) {
          this.sendGamepad(bytes);
        } else {
          this.sendReliable(bytes);
        }
      }
    }

    this.diagnostics.connectedGamepads = connectedCount;
    this.updateHapticsAdvertisement(this.hasConnectedHapticGamepad());
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
    this.stopGamepadRumble(event.gamepad.index, event.gamepad);
    // The polling loop will detect and handle the disconnection
  };

  private logHapticsWarning(message: string): void {
    const nowMs = performance.now();
    if (nowMs - this.lastHapticsWarningAtMs < GfnWebRtcClient.HAPTICS_LOG_INTERVAL_MS) {
      return;
    }
    this.lastHapticsWarningAtMs = nowMs;
    this.log(message);
  }

  private getConnectedRumbleGamepads(): ConnectedRumbleGamepad[] {
    const gamepads = navigator.getGamepads();
    if (!gamepads) {
      return [];
    }

    const connected: ConnectedRumbleGamepad[] = [];
    for (let i = 0; i < Math.min(gamepads.length, GAMEPAD_MAX_CONTROLLERS); i++) {
      const gamepad = gamepads[i];
      if (gamepad?.connected) {
        connected.push({ index: i, gamepad, api: getGamepadRumbleApi(gamepad) });
      }
    }
    return connected;
  }

  private hasConnectedHapticGamepad(): boolean {
    const gamepads = navigator.getGamepads();
    if (!gamepads) {
      return false;
    }

    for (let i = 0; i < Math.min(gamepads.length, GAMEPAD_MAX_CONTROLLERS); i++) {
      const gamepad = gamepads[i];
      if (gamepad?.connected && getGamepadRumbleApi(gamepad)) {
        return true;
      }
    }
    return false;
  }

  private updateHapticsAdvertisement(enabled: boolean): void {
    if (!this.inputReady || this.reliableInputChannel?.readyState !== "open" || this.hapticsAdvertised === enabled) {
      return;
    }

    this.sendReliable(this.inputEncoder.encodeHapticsEnabled(enabled));
    this.hapticsAdvertised = enabled;
    this.log(`Gamepad haptics advertised: ${enabled ? "enabled" : "disabled"}`);
  }

  private findConnectedGamepad(controllerId: number): ConnectedRumbleGamepad | null {
    const connected = this.getConnectedRumbleGamepads();
    if (connected.length === 0) {
      this.logHapticsWarning(`Input haptics: no haptic-capable gamepad for controller ${controllerId} (connected=0)`);
      return null;
    }

    const exact = controllerId >= 0 && controllerId < GAMEPAD_MAX_CONTROLLERS
      ? connected.find((candidate) => candidate.index === controllerId)
      : undefined;
    if (exact?.api) {
      return exact;
    }

    const hapticConnected = connected.filter((candidate) => candidate.api);
    const indexedFallback = controllerId >= 0 && controllerId < GAMEPAD_MAX_CONTROLLERS
      ? hapticConnected[controllerId]
      : undefined;
    if (indexedFallback) {
      return indexedFallback;
    }

    if (hapticConnected.length === 1) {
      return hapticConnected[0];
    }

    this.logHapticsWarning(
      `Input haptics: no haptic-capable gamepad for controller ${controllerId} (connected=${connected.length})`,
    );
    return null;
  }

  private applyRumbleApi(api: GamepadRumbleApi, index: number, weakMagnitude: number, strongMagnitude: number, isStop: boolean): void {
    const duration = isStop ? 0 : GfnWebRtcClient.RUMBLE_EFFECT_MS;
    let usedPlayEffect = false;
    if (api.playEffectActuator) {
      usedPlayEffect = true;
      void api.playEffectActuator.playEffect("dual-rumble", {
        startDelay: 0,
        duration,
        weakMagnitude: isStop ? 0 : weakMagnitude,
        strongMagnitude: isStop ? 0 : strongMagnitude,
      }).catch(() => {});
    }

    if (api.pulseActuator && (isStop || !usedPlayEffect)) {
      if (!isStop && !this.fallbackHapticsSupportLogged[index]) {
        this.fallbackHapticsSupportLogged[index] = true;
        this.log(`Gamepad ${index} fallback pulse haptics available`);
      }
      void api.pulseActuator.pulse(isStop ? 0 : Math.max(weakMagnitude, strongMagnitude), duration).catch(() => {});
    }
  }

  private applyGamepadRumble(controllerId: number, weakMagnitude16: number, strongMagnitude16: number): void {
    const target = this.findConnectedGamepad(controllerId);
    if (!target) {
      return;
    }
    if (!target.api) {
      return;
    }

    const index = target.index;
    if (target.api.playEffectActuator && !this.hapticsSupportLogged[index]) {
      this.hapticsSupportLogged[index] = true;
      this.log(`Gamepad ${index} dual-rumble haptics available`);
    }

    const weakMagnitude = clampRumbleMagnitude(weakMagnitude16 / 65535);
    const strongMagnitude = clampRumbleMagnitude(strongMagnitude16 / 65535);
    const isStop = weakMagnitude === 0 && strongMagnitude === 0;
    const nowMs = performance.now();
    this.lastRumbleWeak[index] = weakMagnitude;
    this.lastRumbleStrong[index] = strongMagnitude;

    if (
      !isStop
      && this.lastRumbleEffectAtMs[index] !== 0
      && nowMs - this.lastRumbleEffectAtMs[index] <= GfnWebRtcClient.RUMBLE_THROTTLE_MS
    ) {
      return;
    }

    this.lastRumbleEffectAtMs[index] = isStop ? 0 : nowMs;
    this.applyRumbleApi(target.api, index, weakMagnitude, strongMagnitude, isStop);
  }

  private stopGamepadRumble(controllerId: number, gamepad?: Gamepad): void {
    if (controllerId < 0 || controllerId >= GAMEPAD_MAX_CONTROLLERS) {
      return;
    }
    if (gamepad) {
      const api = getGamepadRumbleApi(gamepad);
      if (api) {
        this.applyRumbleApi(api, controllerId, 0, 0, true);
      }
    } else {
      this.applyGamepadRumble(controllerId, 0, 0);
    }
    this.lastRumbleWeak[controllerId] = 0;
    this.lastRumbleStrong[controllerId] = 0;
    this.lastRumbleEffectAtMs[controllerId] = 0;
    this.hapticsSupportLogged[controllerId] = false;
    this.fallbackHapticsSupportLogged[controllerId] = false;
  }

  private stopAllGamepadRumble(): void {
    for (const target of this.getConnectedRumbleGamepads()) {
      if (target.api) {
        this.applyRumbleApi(target.api, target.index, 0, 0, true);
      }
    }
    for (let i = 0; i < this.lastRumbleWeak.length; i++) {
      this.lastRumbleWeak[i] = 0;
      this.lastRumbleStrong[i] = 0;
      this.lastRumbleEffectAtMs[i] = 0;
      this.hapticsSupportLogged[i] = false;
      this.fallbackHapticsSupportLogged[i] = false;
    }
    this.lastHapticsWarningAtMs = 0;
  }

  private parseLegacyHapticPacket(view: DataView, offset: number): boolean {
    if (offset < 0 || offset + 10 > view.byteLength) {
      this.logHapticsWarning(`Input haptics: malformed legacy packet (${view.byteLength - offset} bytes)`);
      return false;
    }

    const kind = view.getUint16(offset, true);
    if (kind !== 1) {
      if (kind !== 0) {
        this.logHapticsWarning(`Input haptics: unknown legacy kind ${kind}`);
      }
      return false;
    }

    const length = view.getUint16(offset + 2, true);
    if (length < 6) {
      return false;
    }

    const controllerId = view.getUint16(offset + 4, true);
    const weakMagnitude = view.getUint16(offset + 6, true);
    const strongMagnitude = view.getUint16(offset + 8, true);
    this.applyGamepadRumble(controllerId, weakMagnitude, strongMagnitude);
    return true;
  }

  private parseOcHapticPacket(view: DataView, offset: number): boolean {
    if (offset < 0 || offset + 9 > view.byteLength) {
      this.logHapticsWarning(`Input haptics: malformed Oc packet (${view.byteLength - offset} bytes)`);
      return false;
    }

    const controllerByte = view.getUint8(offset);
    if (controllerByte < 6 || controllerByte >= 10) {
      this.logHapticsWarning(`Input haptics: unknown Oc controller byte ${controllerByte}`);
      return false;
    }

    const reportKind = view.getUint8(offset + 3);
    const flags = view.getUint8(offset + 4);
    if (reportKind !== 5 || (flags & ~1) !== 0) {
      this.logHapticsWarning(`Input haptics: unsupported Oc report kind=${reportKind} flags=0x${flags.toString(16)}`);
      return false;
    }

    const controllerId = controllerByte - 6;
    const weakMagnitude = view.getUint8(offset + 7) << 8;
    const strongMagnitude = view.getUint8(offset + 8) << 8;
    this.applyGamepadRumble(controllerId, weakMagnitude, strongMagnitude);
    return true;
  }

  private parseInputSubMessage(view: DataView, offset: number): boolean {
    if (offset < 0 || offset + 4 > view.byteLength) {
      this.logHapticsWarning(`Input haptics: malformed sub-message (${view.byteLength - offset} bytes)`);
      return false;
    }

    const type = view.getUint32(offset, true);
    if (type === 267) {
      return this.parseLegacyHapticPacket(view, offset + 4);
    }
    if (type === 17) {
      return this.parseOcHapticPacket(view, offset + 4);
    }

    this.logHapticsWarning(`Input haptics: unknown sub-message type ${type}`);
    return false;
  }

  private parseInputHapticsMessage(bytes: Uint8Array): void {
    if (bytes.length < 2) {
      return;
    }

    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const firstWord = view.getUint16(0, true);
    if (firstWord === 267) {
      this.parseLegacyHapticPacket(view, 2);
      return;
    }

    const wrapperType = firstWord & 0xff;
    switch (wrapperType) {
      case 34:
        this.parseInputSubMessage(view, 1);
        return;
      case 32:
      case 33:
      case 35:
      case 36:
      case 255:
        return;
      default:
        this.parseLegacyHapticPacket(view, 0);
    }
  }

  private isPartiallyReliableChannelOpen(): boolean {
    return this.partiallyReliableInputChannel?.readyState === "open";
  }

  private canSendGamepadPartiallyReliable(controllerId: number): boolean {
    const mask = 1 << (controllerId & 0x1f);
    return this.isPartiallyReliableChannelOpen()
      && (this.riInputCapabilities.enablePartiallyReliableTransferGamepad & mask) !== 0;
  }

  private canSendInputTypePartiallyReliable(inputType: number): boolean {
    if (!this.isPartiallyReliableChannelOpen() || !isPartiallyReliableHidTransferEligible(inputType)) {
      return false;
    }
    const hidMask = partiallyReliableHidMaskForInputType(inputType);
    if (hidMask === 0) {
      return false;
    }
    if ((this.riInputCapabilities.hidDeviceMask & hidMask) === 0) {
      return false;
    }
    return (this.riInputCapabilities.enablePartiallyReliableTransferHid & hidMask) !== 0;
  }

  private sendPartiallyReliable(payload: Uint8Array): void {
    if (this.partiallyReliableInputChannel?.readyState === "open") {
      const view = payload.byteOffset === 0 && payload.byteLength === payload.buffer.byteLength
        ? payload
        : payload.slice();
      this.partiallyReliableInputChannel.send(view as unknown as ArrayBufferView<ArrayBuffer>);
      return;
    }

    this.sendReliable(payload);
  }

  private sendInputPacket(payload: Uint8Array, inputType: number): void {
    if (this.canSendInputTypePartiallyReliable(inputType)) {
      this.sendPartiallyReliable(payload);
      return;
    }

    this.sendReliable(payload);
  }

  private onInputHandshakeMessage(bytes: Uint8Array): void {
    if (bytes.length < 2) {
      if (!this.inputReady) {
        this.log(`Input handshake: ignoring short message (${bytes.length} bytes)`);
      }
      return;
    }

    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const firstWord = view.getUint16(0, true);
    let version = 2;

    if (this.inputReady) {
      this.parseInputHapticsMessage(bytes);
      return;
    }

    const hex = Array.from(bytes.slice(0, Math.min(bytes.length, 16)))
      .map((b) => b.toString(16).padStart(2, "0"))
      .join(" ");
    this.log(`Input channel message: ${bytes.length} bytes [${hex}]`);

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
      this.updateHapticsAdvertisement(this.hasConnectedHapticGamepad());
      this.setupInputHeartbeat();
      this.setupGamepadPolling();
      // After input becomes ready, attempt to auto-enable pointer lock.
      void this.attemptAutoPointerLock(this.shouldAutoFullscreen()).catch(() => {});
    }
  }

  private createDataChannels(pc: RTCPeerConnection): void {
    this.reliableInputChannel = pc.createDataChannel("input_channel_v1", {
      ordered: true,
    });

    this.reliableInputChannel.onopen = () => {
      this.log("Reliable input channel open");
    };

    this.reliableInputChannel.onmessage = async (event) => {
      const bytes = await toBytes(event.data as string | Blob | ArrayBuffer);
      this.onInputHandshakeMessage(bytes);
    };

    this.partiallyReliableInputChannel = pc.createDataChannel("input_channel_partially_reliable", {
      ordered: false,
      maxPacketLifeTime: this.partialReliableThresholdMs,
    });

    this.partiallyReliableInputChannel.onopen = () => {
      this.diagnostics.partiallyReliableInputOpen = true;
      this.diagnostics.mouseMoveTransport = this.canSendInputTypePartiallyReliable(INPUT_MOUSE_REL)
        ? "partially_reliable"
        : "reliable";
      this.emitStats();
      this.log(
        `Partially reliable input channel open (maxPacketLifeTime=${this.partialReliableThresholdMs}ms, mouseMoveTransport=${this.diagnostics.mouseMoveTransport})`,
      );
    };

    this.partiallyReliableInputChannel.onclose = () => {
      this.diagnostics.partiallyReliableInputOpen = false;
      this.diagnostics.mouseMoveTransport = "reliable";
      this.emitStats();
      this.log("Partially reliable input channel closed");
    };
  }

  private mapTimerNotificationCode(rawCode: number): StreamTimeWarning["code"] | null {
    // Mirrors official client behavior from timerNotification -> StreamWarningType.
    if (rawCode === 1 || rawCode === 2) {
      return 1;
    }
    if (rawCode === 4) {
      return 2;
    }
    if (rawCode === 6) {
      return 3;
    }
    return null;
  }

  private async onControlChannelMessage(data: string | Blob | ArrayBuffer): Promise<void> {
    let payloadText: string;
    if (typeof data === "string") {
      payloadText = data;
    } else if (data instanceof Blob) {
      payloadText = await data.text();
    } else if (data instanceof ArrayBuffer) {
      payloadText = new TextDecoder().decode(data);
    } else {
      return;
    }

    let parsed: unknown;
    try {
      parsed = JSON.parse(payloadText);
    } catch {
      return;
    }

    if (!parsed || typeof parsed !== "object" || !("timerNotification" in parsed)) {
      return;
    }

    const timerNotification = (parsed as { timerNotification?: unknown }).timerNotification;
    if (!timerNotification || typeof timerNotification !== "object") {
      return;
    }

    const rawCode = Number((timerNotification as { code?: unknown }).code);
    const mappedCode = this.mapTimerNotificationCode(rawCode);
    if (mappedCode === null) {
      this.log(`Control timer notification ignored: code=${rawCode}`);
      return;
    }

    const rawSecondsLeft = Number((timerNotification as { secondsLeft?: unknown }).secondsLeft);
    const secondsLeft =
      Number.isFinite(rawSecondsLeft) && rawSecondsLeft >= 0
        ? Math.floor(rawSecondsLeft)
        : undefined;
    this.log(
      `Control timer warning: rawCode=${rawCode} mappedCode=${mappedCode} secondsLeft=${secondsLeft ?? "n/a"}`,
    );
    this.options.onTimeWarning?.({ code: mappedCode, secondsLeft });
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
      const view = payload.byteOffset === 0 && payload.byteLength === payload.buffer.byteLength
        ? payload
        : payload.slice();
      this.reliableInputChannel.send(view as unknown as ArrayBufferView<ArrayBuffer>);
    } else if (!this.reliableDropLogged) {
      this.reliableDropLogged = true;
      this.log(`Reliable channel not open (state=${this.reliableInputChannel?.readyState ?? "null"}), dropping event (${payload.length} bytes)`);
    }
  }

  private async requestPointerLockCompat(
    lockTarget: HTMLElement,
    options?: { unadjustedMovement?: boolean },
  ): Promise<void> {
    const maybePromise = lockTarget.requestPointerLock(options as any) as unknown;
    if (maybePromise && typeof (maybePromise as Promise<void>).then === "function") {
      await (maybePromise as Promise<void>);
    }
  }

  private requestEscapeKeyboardLock(): void {
    if (!document.fullscreenElement) {
      if (this.keyboardLockState === "locked") {
        this.keyboardLockState = "unknown";
      }
      return;
    }

    const nav = navigator as any;
    if (!nav.keyboard?.lock) {
      if (this.keyboardLockState !== "unsupported") {
        this.keyboardLockState = "unsupported";
        this.log("Keyboard Lock API unavailable; Escape may release pointer lock");
      }
      return;
    }

    void Promise.resolve(nav.keyboard.lock())
      .then(() => {
        if (this.keyboardLockState !== "locked") {
          this.keyboardLockState = "locked";
          this.log("Keyboard lock active for fullscreen stream");
        }
      })
      .catch((error: unknown) => {
        this.keyboardLockState = "failed";
        this.log(`Keyboard Escape lock failed: ${String(error)}`);
      });
  }

  private async requestPointerLockWithOptionalFullscreen(
    lockTarget: HTMLElement,
    ensureFullscreen: boolean,
  ): Promise<void> {
    if (ensureFullscreen && !document.fullscreenElement) {
      try {
        await document.documentElement.requestFullscreen();
      } catch (error) {
        this.log(`DOM fullscreen request failed: ${String(error)}`);
      }

      if (typeof window.openNow?.setFullscreen === "function") {
        try {
          await window.openNow.setFullscreen(true);
        } catch (error) {
          this.log(`Native fullscreen request failed: ${String(error)}`);
        }
      }
    }

    this.requestEscapeKeyboardLock();

    try {
      await this.requestPointerLockCompat(lockTarget, { unadjustedMovement: true });
      this.log("Pointer lock acquired with unadjustedMovement=true (raw/unaccelerated)");
    } catch (err) {
      const domErr = err as DOMException;
      if (domErr?.name === "NotSupportedError") {
        this.log("unadjustedMovement not supported, falling back to standard pointer lock (accelerated)");
        await this.requestPointerLockCompat(lockTarget);
      } else {
        throw err;
      }
    }
  }

  private async attemptAutoPointerLock(ensureFullscreen = true): Promise<void> {
    if (this.autoPointerLockInProgress) return;
    this.autoPointerLockInProgress = true;
    try {
      const target = this.pointerLockTarget ?? this.options.videoElement;
      if (!target) return;
      const lockElement = document.pointerLockElement;
      if (lockElement === target || lockElement === this.options.videoElement) {
        return;
      }

      try {
        await this.requestPointerLockWithOptionalFullscreen(target, ensureFullscreen);
        this.log("Auto pointer lock acquired");
        return;
      } catch (err) {
        // Fallback to a simpler request if the guarded method fails
        try {
          await this.requestPointerLockCompat(target, { unadjustedMovement: true });
          this.log("Auto pointer lock acquired (fallback)");
          return;
        } catch (err2) {
          this.log(`Auto pointer lock failed: ${String(err)}`);
        }
      }
    } finally {
      this.autoPointerLockInProgress = false;
    }
  }

  private shouldSendSyntheticEscapeOnPointerLockLoss(): boolean {
    if (document.visibilityState !== "visible") {
      return false;
    }
    if (typeof document.hasFocus === "function" && !document.hasFocus()) {
      return false;
    }
    return true;
  }

  private releasePressedKeys(reason: string): void {
    if (this.pressedKeys.size === 0 || !this.inputReady) {
      this.pressedKeys.clear();
      return;
    }

    this.log(`Releasing ${this.pressedKeys.size} key(s): ${reason}`);
    for (const vk of this.pressedKeys) {
      const payload = this.inputEncoder.encodeKeyUp({
        keycode: vk,
        scancode: 0,
        modifiers: 0,
        timestampUs: timestampUs(),
      });
      this.sendReliable(payload);
    }
    this.pressedKeys.clear();
  }

  private sendKeyPacket(vk: number, scancode: number, modifiers: number, isDown: boolean): void {
    const payload = isDown
      ? this.inputEncoder.encodeKeyDown({
        keycode: vk,
        scancode,
        modifiers,
        timestampUs: timestampUs(),
      })
      : this.inputEncoder.encodeKeyUp({
        keycode: vk,
        scancode,
        modifiers,
        timestampUs: timestampUs(),
      });
    this.sendReliable(payload);
  }

  public sendAntiAfkPulse(): boolean {
    if (!this.inputReady) {
      return false;
    }

    this.sendKeyPacket(codeMap.F13.vk, codeMap.F13.scancode, 0, true);
    window.setTimeout(() => this.sendKeyPacket(codeMap.F13.vk, codeMap.F13.scancode, 0, false), 50);
    return true;
  }

  public sendPasteShortcut(useMeta: boolean): boolean {
    if (!this.inputReady) {
      return false;
    }

    const modifier = useMeta
      ? { ...codeMap.MetaLeft, flag: 0x08 }
      : { ...codeMap.ControlLeft, flag: 0x02 };

    this.sendKeyPacket(modifier.vk, modifier.scancode, modifier.flag, true);
    this.sendKeyPacket(codeMap.KeyV.vk, codeMap.KeyV.scancode, modifier.flag, true);
    this.sendKeyPacket(codeMap.KeyV.vk, codeMap.KeyV.scancode, modifier.flag, false);
    this.sendKeyPacket(modifier.vk, modifier.scancode, 0, false);
    return true;
  }

  public sendText(text: string): number {
    if (!this.inputReady || !text) {
      return 0;
    }

    let sent = 0;
    const maxChars = 4096;
    for (const char of text.slice(0, maxChars)) {
      const key = mapTextCharToKeySpec(char);
      if (!key) {
        continue;
      }

      if (key.shift) {
        this.sendKeyPacket(codeMap.ShiftLeft.vk, codeMap.ShiftLeft.scancode, 0x01, true);
      }

      const mods = key.shift ? 0x01 : 0;
      this.sendKeyPacket(key.vk, key.scancode, mods, true);
      this.sendKeyPacket(key.vk, key.scancode, mods, false);

      if (key.shift) {
        this.sendKeyPacket(codeMap.ShiftLeft.vk, codeMap.ShiftLeft.scancode, 0, false);
      }

      sent++;
    }

    return sent;
  }

  private sendGamepad(payload: Uint8Array): void {
    this.sendPartiallyReliable(payload);
  }

  private installInputCapture(videoElement: HTMLVideoElement): void {
    this.detachInputCapture();

    const pointerLockTarget = (videoElement.parentElement as HTMLElement | null) ?? videoElement;
    const isPointerLockActive = (): boolean => {
      const lockElement = document.pointerLockElement;
      return lockElement === pointerLockTarget || lockElement === videoElement;
    };

    // Mirror mode: tracks whether the HW cursor is over the stream viewport.
    // Dual-source: coarse window focus/blur sets the initial state and handles
    // cases where the cursor was already inside when the stream started;
    // mouseenter/mouseleave on pointerLockTarget refines it for sub-window
    // boundaries (overlays, toolbars, multi-monitor cursor exit without blur).
    let mouseInStreamView = document.hasFocus();
    let lastAbsX: number | null = null;
    let lastAbsY: number | null = null;
    // Prevent repeated auto-lock attempts within the same focus session.
    let autoLockPending = false;

    // Track an approximate server-side absolute pointer position (in server
    // pixels — the remote stream's resolution) so we can align the server cursor
    // to the hardware cursor when transitioning from mirror -> pointer-lock.
    // `null` means unknown; when unknown we assume server cursor equals HW cursor on first entry.
    let simulatedAbsX: number | null = null;
    let simulatedAbsY: number | null = null;
    // When a document-level entry event triggers tryAutoLock, we store the
    // entry absolute coordinates here so tryAutoLock can align before locking.
    let pendingEntryAbsX: number | null = null;
    let pendingEntryAbsY: number | null = null;

    const onPointerLockTargetMouseEnter = (): void => {
      mouseInStreamView = true;
      lastAbsX = null;
      lastAbsY = null;
      tryAutoLock();
    };

    const onPointerLockTargetMouseLeave = (): void => {
      mouseInStreamView = false;
      lastAbsX = null;
      lastAbsY = null;
      autoLockPending = false;
    };

    const hasPointerRawUpdate = "onpointerrawupdate" in videoElement;
    const hasCoalescedEvents =
      typeof PointerEvent !== "undefined" && "getCoalescedEvents" in PointerEvent.prototype;
    const pointerMoveEventName: "pointerrawupdate" | "pointermove" | null = hasPointerRawUpdate
      ? "pointerrawupdate"
      : (typeof PointerEvent !== "undefined" ? "pointermove" : null);
    this.mouseFlushBaseIntervalMs = hasPointerRawUpdate
      ? GfnWebRtcClient.MOUSE_FLUSH_FAST_MS
      : hasCoalescedEvents
        ? GfnWebRtcClient.MOUSE_FLUSH_NORMAL_MS
        : GfnWebRtcClient.MOUSE_FLUSH_SAFE_MS;
    this.mouseFlushIntervalMs = this.mouseFlushBaseIntervalMs;
    this.mouseAdaptiveFlushActive = false;
    this.mouseFlushLastTickMs = performance.now();
    this.pendingMouseDxFloat = 0;
    this.pendingMouseDyFloat = 0;
    this.pendingMouseTimestampUs = null;
    this.mousePacketsPerSecond = 0;
    this.mousePacketsSentInWindow = 0;
    this.mousePacketRateWindowStartedAtMs = this.mouseFlushLastTickMs;
    this.mouseDeltaFilter.reset();
    this.mouseDeltaFilter.setRelaxedForRawInput(hasPointerRawUpdate);
    this.log(
      `Mouse input mode: ${pointerMoveEventName ?? "mousemove"}, coalesced=${hasCoalescedEvents ? "yes" : "no"}, flush=${this.mouseFlushIntervalMs}ms`,
    );

    const pointerScaleCache = {
      rectWidth: 0,
      rectHeight: 0,
      scaleX: 1,
      scaleY: 1,
      serverWidth: 0,
      serverHeight: 0,
      resolution: "",
    };
    const getPointerScale = (): typeof pointerScaleCache => {
      const rect = pointerLockTarget.getBoundingClientRect();
      const resolution = this.currentResolution ?? "";
      if (
        pointerScaleCache.rectWidth === rect.width
        && pointerScaleCache.rectHeight === rect.height
        && pointerScaleCache.resolution === resolution
      ) {
        return pointerScaleCache;
      }

      let serverWidth = rect.width;
      let serverHeight = rect.height;
      const resMatch = /^([0-9]+)x([0-9]+)$/.exec(resolution);
      if (resMatch) {
        serverWidth = parseInt(resMatch[1], 10) || serverWidth;
        serverHeight = parseInt(resMatch[2], 10) || serverHeight;
      }

      pointerScaleCache.rectWidth = rect.width;
      pointerScaleCache.rectHeight = rect.height;
      pointerScaleCache.serverWidth = serverWidth;
      pointerScaleCache.serverHeight = serverHeight;
      pointerScaleCache.scaleX = rect.width > 0 ? serverWidth / rect.width : 1;
      pointerScaleCache.scaleY = rect.height > 0 ? serverHeight / rect.height : 1;
      pointerScaleCache.resolution = resolution;
      return pointerScaleCache;
    };

    const flushMouse = () => {
      const tickNow = performance.now();
      if (this.mouseFlushLastTickMs > 0) {
        const expected = this.mouseFlushLastTickMs + this.mouseFlushIntervalMs;
        const schedulingDelay = Math.max(0, tickNow - expected);
        this.inputQueueMaxSchedulingDelayMsWindow = Math.max(
          this.inputQueueMaxSchedulingDelayMsWindow,
          schedulingDelay,
        );
      }
      this.mouseFlushLastTickMs = tickNow;

      if (!this.inputReady) {
        return;
      }

      const hasPendingMovement = Math.abs(this.pendingMouseDxFloat) >= 0.5 || Math.abs(this.pendingMouseDyFloat) >= 0.5;
      if (!hasPendingMovement) {
        return;
      }

      const reliable = this.reliableInputChannel;
      const mouseMoveUsesPartiallyReliable = this.canSendInputTypePartiallyReliable(INPUT_MOUSE_REL);
      if (
        !mouseMoveUsesPartiallyReliable
        && 
        reliable?.readyState === "open"
        && reliable.bufferedAmount > GfnWebRtcClient.RELIABLE_MOUSE_BACKPRESSURE_BYTES
      ) {
        const now = performance.now();
        this.inputQueueDropCount++;
        if (now - this.mouseBackpressureLoggedAtMs >= GfnWebRtcClient.BACKPRESSURE_LOG_INTERVAL_MS) {
          this.mouseBackpressureLoggedAtMs = now;
          this.log(`Dropping stale mouse movement (reliable bufferedAmount=${reliable.bufferedAmount})`);
        }
        this.pendingMouseDxFloat = 0;
        this.pendingMouseDyFloat = 0;
        this.pendingMouseTimestampUs = null;
        return;
      }
      const { scaleX, scaleY } = getPointerScale();
      const dxQuantized = quantizeMouseDeltaWithResidual(this.pendingMouseDxFloat);
      const dyQuantized = quantizeMouseDeltaWithResidual(this.pendingMouseDyFloat);
      const dxServer = Math.max(-32768, Math.min(32767, Math.round(dxQuantized.send * scaleX)));
      const dyServer = Math.max(-32768, Math.min(32767, Math.round(dyQuantized.send * scaleY)));
      if (dxServer === 0 && dyServer === 0) {
        // Keep pending movement intact until a non-zero packet is sent.
        // Otherwise quantized integer deltas can be dropped when server scaling rounds to zero.
        return;
      }
      this.pendingMouseDxFloat = dxQuantized.residual;
      this.pendingMouseDyFloat = dyQuantized.residual;

      const payload = this.inputEncoder.encodeMouseMove({
        dx: dxServer,
        dy: dyServer,
        timestampUs: this.pendingMouseTimestampUs ?? timestampUs(),
      });

      this.pendingMouseTimestampUs = null;
      this.sendInputPacket(payload, INPUT_MOUSE_REL);
      this.mousePacketsSentInWindow += 1;
      // Update simulated absolute pointer (stored in server pixels) if we have a baseline.
      if (simulatedAbsX !== null && simulatedAbsY !== null) {
        simulatedAbsX += dxServer;
        simulatedAbsY += dyServer;
      }
    };
    const scheduleNextFlush = (): void => {
      if (this.mouseFlushTimer !== null) {
        window.clearTimeout(this.mouseFlushTimer);
      }
      this.mouseFlushTimer = window.setTimeout(() => {
        try {
          flushMouse();
          const reliableBufferedAmount = this.reliableInputChannel?.bufferedAmount ?? 0;
          const schedulingDelay = this.inputQueueMaxSchedulingDelayMsWindow;
          const nextInterval = chooseAdaptiveMouseFlushInterval({
            baseIntervalMs: this.mouseFlushBaseIntervalMs,
            currentIntervalMs: this.mouseFlushIntervalMs,
            reliableBufferedAmount,
            schedulingDelayMs: schedulingDelay,
            canUsePartiallyReliableMouse: this.canSendInputTypePartiallyReliable(INPUT_MOUSE_REL),
            backpressureThresholdBytes: GfnWebRtcClient.RELIABLE_MOUSE_BACKPRESSURE_BYTES,
            minIntervalMs: GfnWebRtcClient.MOUSE_FLUSH_MIN_MS,
            maxIntervalMs: GfnWebRtcClient.MOUSE_FLUSH_MAX_MS,
          });
          this.mouseAdaptiveFlushActive = nextInterval !== this.mouseFlushBaseIntervalMs;
          this.mouseFlushIntervalMs = nextInterval;
          const now = performance.now();
          if (this.mousePacketRateWindowStartedAtMs <= 0) {
            this.mousePacketRateWindowStartedAtMs = now;
          }
          const elapsed = now - this.mousePacketRateWindowStartedAtMs;
          if (elapsed >= 1000) {
            this.mousePacketsPerSecond = Math.round((this.mousePacketsSentInWindow * 1000) / elapsed);
            this.mousePacketsSentInWindow = 0;
            this.mousePacketRateWindowStartedAtMs = now;
          }
        } catch (err) {
          this.log(`Mouse flush tick failed (non-fatal): ${String(err)}`);
        } finally {
          // clearTimers() nulls this timer during teardown; avoid re-arming a zombie loop.
          if (this.mouseFlushTimer !== null) {
            scheduleNextFlush();
          }
        }
      }, this.mouseFlushIntervalMs);
    };
    scheduleNextFlush();

    const tryAutoLock = (): void => {
      try {
        if (document?.body?.dataset?.sidebarOpen === "1") {
          return;
        }
      } catch {}

      if (autoLockPending || isPointerLockActive() || !mouseInStreamView || !this.inputReady) {
        return;
      }
      autoLockPending = true;

      // Align server cursor to current HW cursor (if we have an entry position)
      // before requesting pointer lock so the transition appears smooth.
      try {
        const targetAbsX = pendingEntryAbsX ?? lastAbsX;
        const targetAbsY = pendingEntryAbsY ?? lastAbsY;
        // Consume pending entry coords
        pendingEntryAbsX = null;
        pendingEntryAbsY = null;

        if (typeof targetAbsX === "number" && typeof targetAbsY === "number") {
          const { scaleX, scaleY, serverWidth, serverHeight } = getPointerScale();

          // Translate the element-local target into server pixels.
          const targetServerX = Math.round(targetAbsX * scaleX);
          const targetServerY = Math.round(targetAbsY * scaleY);

          if (simulatedAbsX === null || simulatedAbsY === null) {
            // No baseline known: assume server cursor is centered and move from
            // center -> target in server pixels so remote cursor matches HW cursor.
            const baselineXServer = Math.round(serverWidth / 2);
            const baselineYServer = Math.round(serverHeight / 2);
            const dx = Math.round(targetServerX - baselineXServer);
            const dy = Math.round(targetServerY - baselineYServer);
            if (dx !== 0 || dy !== 0) {
              const movePayload = this.inputEncoder.encodeMouseMove({
                dx: Math.max(-32768, Math.min(32767, dx)),
                dy: Math.max(-32768, Math.min(32767, dy)),
                timestampUs: timestampUs(),
              });
              this.sendReliable(movePayload);
            }
            // Record simulated baseline in server pixels.
            simulatedAbsX = targetServerX;
            simulatedAbsY = targetServerY;
          } else {
            // sim values are stored in server pixels now; compute server delta.
            const dx = Math.round(targetServerX - simulatedAbsX);
            const dy = Math.round(targetServerY - simulatedAbsY);
            if (dx !== 0 || dy !== 0) {
              const movePayload = this.inputEncoder.encodeMouseMove({
                dx: Math.max(-32768, Math.min(32767, dx)),
                dy: Math.max(-32768, Math.min(32767, dy)),
                timestampUs: timestampUs(),
              });
              this.sendReliable(movePayload);
              simulatedAbsX += dx;
              simulatedAbsY += dy;
            }
          }
        }
      } catch (err) {
        this.log(`Pointer lock alignment failed (non-fatal): ${String(err)}`);
      }

      void this.attemptAutoPointerLock(this.shouldAutoFullscreen())
        .catch(() => {})
        .finally(() => {
          autoLockPending = false;
        });
    };

    const queueMouseMovement = (dx: number, dy: number, eventTimestampMs: number): void => {
      if (!this.inputReady || !isPointerLockActive()) {
        return;
      }

      if (!this.mouseDeltaFilter.update(dx, dy, eventTimestampMs)) {
        return;
      }

      // Apply user-configured sensitivity, then optional software acceleration.
      let adjustedDx = this.mouseDeltaFilter.getX() * this.mouseSensitivity;
      let adjustedDy = this.mouseDeltaFilter.getY() * this.mouseSensitivity;

      if (this.mouseAccelerationPercent > 1) {
        const speed = Math.hypot(adjustedDx, adjustedDy);
        const strength = (this.mouseAccelerationPercent - 1) / 149;
        // Gentle curve: low-speed precision, high-speed turn boost (caps at +60% at 150%).
        const accelFactor = 1 + Math.min(0.6 * strength, (speed / 50) * strength);
        adjustedDx *= accelFactor;
        adjustedDy *= accelFactor;
      }

      this.pendingMouseDxFloat += adjustedDx;
      this.pendingMouseDyFloat += adjustedDy;
      this.pendingMouseTimestampUs = timestampUs(eventTimestampMs);
    };

    const onPointerMove = (event: PointerEvent) => {
      try {
        if (document?.body?.dataset?.sidebarOpen === "1") return;
      } catch {}
      if (this.isStreamInputBlocked()) return;
      if (event.pointerType && event.pointerType !== "mouse") {
        return;
      }

      if (isPointerLockActive()) {
        // Pointer lock active: use raw relative movement (movementX/Y).
        const samples = hasCoalescedEvents ? event.getCoalescedEvents() : [];
        if (samples.length > 0) {
          for (const sample of samples) {
            queueMouseMovement(sample.movementX, sample.movementY, sample.timeStamp);
          }
          return;
        }
        queueMouseMovement(event.movementX, event.movementY, event.timeStamp);
      } else if (mouseInStreamView) {
        // Pointer lock disabled: keep local cursor tracking up to date without
        // forwarding mouse movement into the stream.
        const rect = pointerLockTarget.getBoundingClientRect();
        const absX = event.clientX - rect.left;
        const absY = event.clientY - rect.top;
        lastAbsX = absX;
        lastAbsY = absY;
      }
    };

    const onMouseMove = (event: MouseEvent) => {
      try {
        if (document?.body?.dataset?.sidebarOpen === "1") return;
      } catch {}
      if (this.isStreamInputBlocked()) return;
      if (isPointerLockActive()) {
        queueMouseMovement(event.movementX, event.movementY, event.timeStamp);
      } else if (mouseInStreamView) {
        // Pointer lock disabled: keep local cursor tracking up to date without
        // forwarding mouse movement into the stream.
        const rect = pointerLockTarget.getBoundingClientRect();
        const absX = event.clientX - rect.left;
        const absY = event.clientY - rect.top;
        lastAbsX = absX;
        lastAbsY = absY;
      }
    };

    const onKeyDown = (event: KeyboardEvent) => {
      if (this.isStreamInputBlocked()) return;
      if (!this.inputReady) {
        return;
      }

      const isEscapeEvent =
        event.key === "Escape"
        || event.key === "Esc"
        || event.code === "Escape"
        || event.keyCode === 27;
      const mapped = mapKeyboardEvent(event) ?? (isEscapeEvent ? codeMap.Escape : null);

      // Keep browser from handling held keys (for example Tab focus traversal)
      // while streaming input is active.
      if (event.repeat) {
        if (isPointerLockActive() || mapped) {
          event.preventDefault();
        }
        return;
      }

      if (isPointerLockActive()) {
        event.preventDefault();
      }

      if (!mapped) {
        return;
      }

      event.preventDefault();
      this.pressedKeys.add(mapped.vk);

      const payload = this.inputEncoder.encodeKeyDown({
        keycode: mapped.vk,
        scancode: mapped.scancode,
        modifiers: modifierFlags(event),
        // Use a fresh monotonic timestamp for keyboard events. In some
        // fullscreen paths, event.timeStamp can be unstable.
        timestampUs: timestampUs(),
      });
      this.sendReliable(payload);
    };

    const onKeyUp = (event: KeyboardEvent) => {
      if (this.isStreamInputBlocked()) return;
      if (!this.inputReady) {
        return;
      }

      const isEscapeEvent =
        event.key === "Escape"
        || event.key === "Esc"
        || event.code === "Escape"
        || event.keyCode === 27;
      const mapped = mapKeyboardEvent(event) ?? (isEscapeEvent ? codeMap.Escape : null);
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

    const onMouseDown = (event: MouseEvent) => {
      if (this.isStreamInputBlocked()) return;
      if (!this.inputReady) {
        return;
      }
      if (!isPointerLockActive()) {
        return;
      }
      event.preventDefault();
      const payload = this.inputEncoder.encodeMouseButtonDown({
        button: toMouseButton(event.button),
        timestampUs: timestampUs(event.timeStamp),
      });
      // Official GFN client sends all mouse events on reliable channel (input_channel_v1)
      this.sendReliable(payload);
    };

    const onMouseUp = (event: MouseEvent) => {
      if (this.isStreamInputBlocked()) return;
      if (!this.inputReady) {
        return;
      }
      if (!isPointerLockActive()) {
        return;
      }
      event.preventDefault();
      const payload = this.inputEncoder.encodeMouseButtonUp({
        button: toMouseButton(event.button),
        timestampUs: timestampUs(event.timeStamp),
      });
      // Official GFN client sends all mouse events on reliable channel (input_channel_v1)
      this.sendReliable(payload);
    };

    const onWheel = (event: WheelEvent) => {
      if (this.isStreamInputBlocked()) return;
      if (!this.inputReady) {
        return;
      }
      if (!isPointerLockActive()) {
        return;
      }
      event.preventDefault();
      // Official GFN client sends negated raw deltaY as int16 (no quantization to ±120).
      // Clamp to int16 range since browser deltaY can exceed it with fast scrolling.
      const delta = Math.max(-32768, Math.min(32767, Math.round(-event.deltaY)));
      const payload = this.inputEncoder.encodeMouseWheel({
        delta,
        timestampUs: timestampUs(event.timeStamp),
      });
      this.sendReliable(payload);
    };

    const onClick = () => {
      void this.requestPointerLockWithOptionalFullscreen(pointerLockTarget, this.shouldAutoFullscreen()).catch(
        (err: DOMException) => {
          this.log(`Pointer lock request failed: ${err.name}: ${err.message}`);
        },
      );
      videoElement.focus();
    };

    const schedulePointerLockRetention = (reason: string): void => {
      if (this.pointerLockRelockTimer !== null) {
        return;
      }

      this.pointerLockRelockTimer = window.setTimeout(() => {
        this.pointerLockRelockTimer = null;

        if (!this.inputReady || !this.shouldSendSyntheticEscapeOnPointerLockLoss() || isPointerLockActive()) {
          return;
        }

        const target = this.pointerLockTarget;
        if (!target) {
          return;
        }

        void this.requestPointerLockWithOptionalFullscreen(target, false)
          .then(() => {
            this.log(`Pointer lock restored after ${reason}`);
          })
          .catch((error: unknown) => {
            this.log(`Pointer lock restore failed after ${reason}: ${String(error)}`);
          });
      }, 75);
    };

    // Store lock target for pointer lock re-acquisition
    this.pointerLockTarget = pointerLockTarget;

    // Handle pointer lock changes — send synthetic Escape when lock is lost by browser
    // (matches official GFN client's "pointerLockEscape" feature)
    const onPointerLockChange = () => {
      if (isPointerLockActive()) {
        // Pointer lock gained — cancel any pending synthetic Escape.
        // Reset absolute position tracking since we switch to relative movement.
        lastAbsX = null;
        lastAbsY = null;
        if (this.pointerLockEscapeTimer !== null) {
          window.clearTimeout(this.pointerLockEscapeTimer);
          this.pointerLockEscapeTimer = null;
        }
        if (this.pointerLockRelockTimer !== null) {
          window.clearTimeout(this.pointerLockRelockTimer);
          this.pointerLockRelockTimer = null;
        }
        this.suppressNextSyntheticEscape = false;
        // Try to acquire keyboard lock for low-level key capture (best-effort).
        try {
          this.requestEscapeKeyboardLock();
        } catch {}

        // Notify main process that pointer lock is active so native-level
        // interception (before-input-event) can act accordingly.
        try {
          (window as any).openNow?.notifyPointerLockChange?.(true);
        } catch {}
        return;
      }

      // Pointer lock was lost — reset mirror state so tracking resumes from the
      // current cursor position rather than from a stale last-known position.
      lastAbsX = null;
      lastAbsY = null;

      try {
        (window as any).openNow?.notifyPointerLockChange?.(false);
      } catch {}

      // Pointer lock was lost
      if (!this.inputReady) return;

      if (this.suppressNextSyntheticEscape) {
        this.suppressNextSyntheticEscape = false;
        this.releasePressedKeys("pointer lock intentionally released");
        return;
      }

      if (!this.shouldSendSyntheticEscapeOnPointerLockLoss()) {
        this.releasePressedKeys("pointer lock lost while unfocused");
        return;
      }

      // VK 0x1B = 27 = Escape
      const escapeWasPressed = this.pressedKeys.has(0x1B);

      if (escapeWasPressed) {
        // Escape was already tracked as pressed — the normal keyup handler will fire
        // and send Escape keyup to the server. No synthetic needed, but Chromium
        // still released pointer lock, so restore it after keyup has a chance to run.
        schedulePointerLockRetention("tracked Escape");
        return;
      }

      // Escape was NOT tracked as pressed — browser intercepted it before our keydown fired.
      // Send synthetic Escape keydown+keyup after 50ms (matches official GFN client).
      // Also re-acquire pointer lock so the user stays in the game.
      this.pointerLockEscapeTimer = window.setTimeout(() => {
        this.pointerLockEscapeTimer = null;

        if (!this.inputReady) return;

        if (!this.shouldSendSyntheticEscapeOnPointerLockLoss()) {
          this.releasePressedKeys("focus changed before synthetic Escape");
          return;
        }

        // Release all currently held keys first (matching official client's MS() function)
        this.releasePressedKeys("pointer lock lost before synthetic Escape");

        // Send synthetic Escape keydown + keyup
        this.log("Sending synthetic Escape (pointer lock lost by browser)");
        const escDown = this.inputEncoder.encodeKeyDown({
          keycode: 0x1B,
          scancode: codeMap.Escape.scancode,
          modifiers: 0,
          timestampUs: timestampUs(),
        });
        this.sendReliable(escDown);

        const escUp = this.inputEncoder.encodeKeyUp({
          keycode: 0x1B,
          scancode: codeMap.Escape.scancode,
          modifiers: 0,
          timestampUs: timestampUs(),
        });
        this.sendReliable(escUp);

        schedulePointerLockRetention("synthetic Escape");
      }, 50);
    };

    const onWindowBlur = () => {
      // Don't release keys during microphone permission request
      // as getUserMedia() may cause brief window focus loss
      if (this.micState === "permission_pending") {
        this.log("Window blur during mic permission - keeping keys pressed");
        return;
      }
      mouseInStreamView = false;
      lastAbsX = null;
      lastAbsY = null;
      this.releasePressedKeys("window blur");
      // Pause forwarding while window is not focused (host overlay pause is separate).
      this.windowStateInputPaused = true;
    };

    const onVisibilityChange = () => {
      if (document.visibilityState !== "visible") {
        this.releasePressedKeys(`visibility ${document.visibilityState}`);
        this.windowStateInputPaused = true;
        return;
      }

      this.windowStateInputPaused = false;
    };

    const onWindowFocus = () => {
      this.windowStateInputPaused = false;
      mouseInStreamView = true;
      lastAbsX = null;
      lastAbsY = null;
      // Auto-lock: acquire pointer lock when the user switches back to the app.
      tryAutoLock();
    };

    // Release any prior Keyboard API lock when leaving fullscreen (e.g. other UI may have locked keys).
    const onFullscreenChange = () => {
      if (document.fullscreenElement) {
        this.requestEscapeKeyboardLock();
        return;
      }
      const nav = navigator as any;
      if (nav.keyboard?.unlock) {
        try {
          nav.keyboard.unlock();
          this.keyboardLockState = "unknown";
        } catch {
          /* no-op */
        }
      }
    };

    // Add gamepad event listeners
    window.addEventListener("gamepadconnected", this.onGamepadConnected);
    window.addEventListener("gamepaddisconnected", this.onGamepadDisconnected);

    document.addEventListener("keydown", onKeyDown, true);
    document.addEventListener("keyup", onKeyUp, true);
    if (pointerMoveEventName) {
      document.addEventListener(pointerMoveEventName, onPointerMove as EventListener);
    } else {
      window.addEventListener("mousemove", onMouseMove);
    }
    pointerLockTarget.addEventListener("mousedown", onMouseDown);
    pointerLockTarget.addEventListener("mouseup", onMouseUp);
    pointerLockTarget.addEventListener("wheel", onWheel, { passive: false });
    pointerLockTarget.addEventListener("mouseenter", onPointerLockTargetMouseEnter);
    pointerLockTarget.addEventListener("mouseleave", onPointerLockTargetMouseLeave);
    // Detect when the mouse enters the application window (from outside the
    // browsing context) and trigger auto pointer lock. We listen to
    // `pointerover` when PointerEvents are available and fall back to
    // `mouseover` for older environments. If `relatedTarget` is null or not
    // part of this document, the pointer came from outside the window. Only
    // attempt auto-lock when the pointer is actually over the stream viewport
    // (pointerLockTarget) to avoid accidental locks when the cursor enters
    // over chrome/UI areas.
    const onDocumentPointerEnterWindow = (ev: PointerEvent | MouseEvent) => {
      // Only care about physical mouse pointers
      if (typeof PointerEvent !== "undefined" && ev instanceof PointerEvent) {
        if (ev.pointerType && ev.pointerType !== "mouse") return;
      }

      const related = (ev as any).relatedTarget as Node | null | undefined;
      if (related && document.contains(related)) {
        // relatedTarget is still within this document — this is an intra-document
        // move, not an entry from outside the window.
        return;
      }

      // Only trigger auto-lock if the pointer is actually over the stream
      // viewport (pointerLockTarget). This prevents accidental locks when the
      // cursor enters the window over chrome/UI areas.
      const rect = pointerLockTarget.getBoundingClientRect();
      const clientX = (ev as MouseEvent).clientX;
      const clientY = (ev as MouseEvent).clientY;
      if (!Number.isFinite(clientX) || !Number.isFinite(clientY)) {
        return;
      }

      if (clientX < rect.left || clientX > rect.right || clientY < rect.top || clientY > rect.bottom) {
        return;
      }

      // Treat this as entering the stream/window area for auto-lock purposes
      mouseInStreamView = true;
      // Save entry absolute coords so tryAutoLock can align the server cursor
      // before requesting pointer lock.
      pendingEntryAbsX = clientX - rect.left;
      pendingEntryAbsY = clientY - rect.top;
      lastAbsX = null;
      lastAbsY = null;
      tryAutoLock();
    };

    // Fallback: some environments may not produce pointerover relatedTarget=null
    // when entering the native window. Listen for the first mousemove while we
    // believe the pointer is outside the window and treat that as an entry.
    const onFirstMouseMoveIntoWindow = (ev: MouseEvent | PointerEvent) => {
      if (mouseInStreamView) return;
      if (typeof PointerEvent !== "undefined" && ev instanceof PointerEvent) {
        if (ev.pointerType && ev.pointerType !== "mouse") return;
      }

      // Only consider it an entry if the cursor is over the stream viewport
      const rect = pointerLockTarget.getBoundingClientRect();
      const clientX = (ev as MouseEvent).clientX;
      const clientY = (ev as MouseEvent).clientY;
      if (!Number.isFinite(clientX) || !Number.isFinite(clientY)) return;
      if (clientX < rect.left || clientX > rect.right || clientY < rect.top || clientY > rect.bottom) return;

      mouseInStreamView = true;
      lastAbsX = null;
      lastAbsY = null;
      tryAutoLock();
      // remove this listener after first use
      document.removeEventListener("mousemove", onFirstMouseMoveIntoWindow as EventListener, true);
      if (typeof PointerEvent !== "undefined") {
        document.removeEventListener("pointermove", onFirstMouseMoveIntoWindow as EventListener, true);
      }
    };
    videoElement.addEventListener("click", onClick);
    if (typeof PointerEvent !== "undefined") {
      document.addEventListener("pointerover", onDocumentPointerEnterWindow, true);
      document.addEventListener("pointermove", onFirstMouseMoveIntoWindow as EventListener, true);
    } else {
      document.addEventListener("mouseover", onDocumentPointerEnterWindow, true);
      document.addEventListener("mousemove", onFirstMouseMoveIntoWindow as EventListener, true);
    }
    document.addEventListener("pointerlockchange", onPointerLockChange);
    document.addEventListener("fullscreenchange", onFullscreenChange);
    window.addEventListener("blur", onWindowBlur);
    document.addEventListener("visibilitychange", onVisibilityChange);
    window.addEventListener("focus", onWindowFocus);

    // Listen for external Escape events forwarded from main process and
    // forward them to the remote session as synthetic Escape keypresses.
    try {
      (window as any).openNow?.onExternalEscape?.(() => {
        if (!this.inputReady) return;
        this.releasePressedKeys("external Escape forwarded from main");

        const escDown = this.inputEncoder.encodeKeyDown({
          keycode: 0x1B,
          scancode: codeMap.Escape.scancode,
          modifiers: 0,
          timestampUs: timestampUs(),
        });
        this.sendReliable(escDown);

        const escUp = this.inputEncoder.encodeKeyUp({
          keycode: 0x1B,
          scancode: codeMap.Escape.scancode,
          modifiers: 0,
          timestampUs: timestampUs(),
        });
        this.sendReliable(escUp);
      });
    } catch {}

    this.inputCleanup.push(() => window.removeEventListener("gamepadconnected", this.onGamepadConnected));
    this.inputCleanup.push(() => window.removeEventListener("gamepaddisconnected", this.onGamepadDisconnected));
    this.inputCleanup.push(() => document.removeEventListener("keydown", onKeyDown, true));
    this.inputCleanup.push(() => document.removeEventListener("keyup", onKeyUp, true));
    if (pointerMoveEventName) {
      this.inputCleanup.push(() => document.removeEventListener(pointerMoveEventName, onPointerMove as EventListener));
    } else {
      this.inputCleanup.push(() => window.removeEventListener("mousemove", onMouseMove));
    }
    this.inputCleanup.push(() => pointerLockTarget.removeEventListener("mousedown", onMouseDown));
    this.inputCleanup.push(() => pointerLockTarget.removeEventListener("mouseup", onMouseUp));
    this.inputCleanup.push(() => pointerLockTarget.removeEventListener("wheel", onWheel));
    this.inputCleanup.push(() => pointerLockTarget.removeEventListener("mouseenter", onPointerLockTargetMouseEnter));
    this.inputCleanup.push(() => pointerLockTarget.removeEventListener("mouseleave", onPointerLockTargetMouseLeave));
    if (typeof PointerEvent !== "undefined") {
      this.inputCleanup.push(() => document.removeEventListener("pointerover", onDocumentPointerEnterWindow, true));
      this.inputCleanup.push(() => document.removeEventListener("pointermove", onFirstMouseMoveIntoWindow as EventListener, true));
    } else {
      this.inputCleanup.push(() => document.removeEventListener("mouseover", onDocumentPointerEnterWindow, true));
      this.inputCleanup.push(() => document.removeEventListener("mousemove", onFirstMouseMoveIntoWindow as EventListener, true));
    }
    this.inputCleanup.push(() => videoElement.removeEventListener("click", onClick));
    this.inputCleanup.push(() => document.removeEventListener("pointerlockchange", onPointerLockChange));
    this.inputCleanup.push(() => document.removeEventListener("fullscreenchange", onFullscreenChange));
    this.inputCleanup.push(() => window.removeEventListener("blur", onWindowBlur));
    this.inputCleanup.push(() => document.removeEventListener("visibilitychange", onVisibilityChange));
    this.inputCleanup.push(() => window.removeEventListener("focus", onWindowFocus));
    this.inputCleanup.push(() => {
      if (this.pointerLockEscapeTimer !== null) {
        window.clearTimeout(this.pointerLockEscapeTimer);
        this.pointerLockEscapeTimer = null;
      }
      if (this.pointerLockRelockTimer !== null) {
        window.clearTimeout(this.pointerLockRelockTimer);
        this.pointerLockRelockTimer = null;
      }
      this.releasePressedKeys("input cleanup");
      this.pendingMouseDxFloat = 0;
      this.pendingMouseDyFloat = 0;
      this.pendingMouseTimestampUs = null;
      this.mouseDeltaFilter.reset();
      this.pointerLockTarget = null;
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

  /** Get supported HEVC profile-id values from RTCRtpReceiver capabilities (e.g. "1", "2"). */
  private getSupportedHevcProfiles(): Set<string> {
    const profiles = new Set<string>();
    try {
      const capabilities = RTCRtpReceiver.getCapabilities("video");
      if (!capabilities) return profiles;
      for (const codec of capabilities.codecs) {
        const mime = codec.mimeType.toUpperCase();
        if (!mime.includes("H265") && !mime.includes("HEVC")) {
          continue;
        }
        const fmtp = codec.sdpFmtpLine ?? "";
        const match = fmtp.match(/(?:^|;)\s*profile-id=(\d+)/i);
        if (match?.[1]) {
          profiles.add(match[1]);
        }
      }
    } catch {
      // Ignore capability failures
    }
    return profiles;
  }

  /** Maximum HEVC level-id by profile-id from receiver capabilities. */
  private getHevcMaxLevelsByProfile(): Partial<Record<1 | 2, number>> {
    const result: Partial<Record<1 | 2, number>> = {};
    try {
      const capabilities = RTCRtpReceiver.getCapabilities("video");
      if (!capabilities) return result;
      for (const codec of capabilities.codecs) {
        const mime = codec.mimeType.toUpperCase();
        if (!mime.includes("H265") && !mime.includes("HEVC")) {
          continue;
        }

        const fmtp = codec.sdpFmtpLine ?? "";
        const profileMatch = fmtp.match(/(?:^|;)\s*profile-id=(\d+)/i);
        const levelMatch = fmtp.match(/(?:^|;)\s*level-id=(\d+)/i);
        if (!profileMatch?.[1] || !levelMatch?.[1]) {
          continue;
        }

        const profile = Number.parseInt(profileMatch[1], 10) as 1 | 2;
        const level = Number.parseInt(levelMatch[1], 10);
        if (!Number.isFinite(level) || (profile !== 1 && profile !== 2)) {
          continue;
        }

        const current = result[profile];
        if (!current || level > current) {
          result[profile] = level;
        }
      }
    } catch {
      // Ignore capability failures
    }
    return result;
  }

  /** Whether receiver capabilities explicitly expose HEVC tier-flag=1 support. */
  private supportsHevcTierFlagOne(): boolean {
    try {
      const capabilities = RTCRtpReceiver.getCapabilities("video");
      if (!capabilities) return false;
      return capabilities.codecs.some((codec) => {
        const mime = codec.mimeType.toUpperCase();
        if (!mime.includes("H265") && !mime.includes("HEVC")) {
          return false;
        }
        return /(?:^|;)\s*tier-flag=1/i.test(codec.sdpFmtpLine ?? "");
      });
    } catch {
      return false;
    }
  }

  /**
   * Apply setCodecPreferences roughly matching GFN web client behavior:
   * preferred codec + RTX/FlexFEC only (receiver capabilities first).
   * On failure, retry with sender capabilities appended.
   */
  private applyCodecPreferences(
    pc: RTCPeerConnection,
    codec: VideoCodec,
    preferredHevcProfileId?: 1 | 2,
  ): void {
    try {
      const transceivers = pc.getTransceivers();
      const videoTransceiver = transceivers.find(
        (t) => t.receiver.track.kind === "video",
      );
      if (!videoTransceiver) {
        this.log("setCodecPreferences: no video transceiver found, skipping");
        return;
      }

      const receiverCaps = RTCRtpReceiver.getCapabilities("video")?.codecs;
      if (!receiverCaps) {
        this.log("setCodecPreferences: RTCRtpReceiver.getCapabilities returned null, skipping");
        return;
      }

      const senderCaps = RTCRtpSender.getCapabilities?.("video")?.codecs ?? [];

      // Map our codec name to the MIME type used in WebRTC capabilities
      const codecMimeMap: Record<string, string> = {
        H264: "video/H264",
        H265: "video/H265",
        AV1: "video/AV1",
        VP9: "video/VP9",
        VP8: "video/VP8",
      };
      const preferredMime = codecMimeMap[codec];
      if (!preferredMime) {
        this.log(`setCodecPreferences: unknown codec "${codec}", skipping`);
        return;
      }

      const preferred = receiverCaps.filter(
        (c) => c.mimeType.toLowerCase() === preferredMime.toLowerCase(),
      );

      const auxiliary = receiverCaps.filter((c) => {
        const mime = c.mimeType.toLowerCase();
        return mime.includes("rtx") || mime.includes("flexfec-03");
      });

      if (preferred.length === 0) {
        this.log(`setCodecPreferences: ${codec} (${preferredMime}) not in receiver capabilities, skipping`);
        return;
      }

      // H265 can be exposed with multiple profiles; prefer profile-id=1 first
      // for maximum decoder compatibility (reduces macroblocking on some GPUs).
      if (codec === "H265" && preferredHevcProfileId) {
        preferred.sort((a, b) => {
          const getScore = (c: RTCRtpCodec): number => {
            const fmtp = (c.sdpFmtpLine ?? "").toLowerCase();
            const match = fmtp.match(/(?:^|;)\s*profile-id=(\d+)/);
            const profile = match?.[1];
            if (profile === String(preferredHevcProfileId)) return 0;
            if (!profile) return 1;
            return 2;
          };
          return getScore(a) - getScore(b);
        });
      }

      let codecList = [...preferred, ...auxiliary];

      try {
        videoTransceiver.setCodecPreferences(codecList);
        this.log(
          `setCodecPreferences: set ${codec} (${preferred.length} preferred + ${auxiliary.length} auxiliary receiver codecs)`,
        );
      } catch (e) {
        this.log(`setCodecPreferences: receiver-only failed (${String(e)}), retrying with sender capabilities`);
        try {
          codecList = codecList.concat(senderCaps);
          videoTransceiver.setCodecPreferences(codecList);
          this.log(
            `setCodecPreferences: retry succeeded with sender capabilities (+${senderCaps.length})`,
          );
        } catch (retryErr) {
          this.log(`setCodecPreferences: retry failed (${String(retryErr)}), falling back to SDP-only approach`);
        }
      }
    } catch (e) {
      this.log(`setCodecPreferences: failed (${String(e)}), falling back to SDP-only approach`);
    }
  }

  async handleOffer(offerSdp: string, session: SessionInfo, settings: OfferSettings): Promise<void> {
    this.cleanupPeerConnection();
    this.log("=== handleOffer START ===");
    this.log(`Session: id=${session.sessionId}, status=${session.status}, serverIp=${session.serverIp}`);
    this.log(`Signaling: server=${session.signalingServer}, url=${session.signalingUrl}`);
    this.log(`MediaConnectionInfo: ${session.mediaConnectionInfo ? `ip=${session.mediaConnectionInfo.ip}, port=${session.mediaConnectionInfo.port}` : "NONE"}`);
    this.log(
      `Settings: codec=${settings.codec}, colorQuality=${settings.colorQuality}, resolution=${settings.resolution}, fps=${settings.fps}, maxBitrate=${settings.maxBitrateKbps}kbps`,
    );
    if (session.negotiatedStreamProfile) {
      this.log(`Negotiated stream profile override: ${JSON.stringify(session.negotiatedStreamProfile)}`);
    }
    this.log(`ICE servers: ${session.iceServers.length} (${session.iceServers.map(s => s.urls.join(",")).join(" | ")})`);
    this.log(`Offer SDP length: ${offerSdp.length} chars`);
    // Log full offer SDP for ICE debugging
    this.log(`=== FULL OFFER SDP START ===`);
    for (const line of offerSdp.split(/\r?\n/)) {
      this.log(`  SDP> ${line}`);
    }
    this.log(`=== FULL OFFER SDP END ===`);

    this.riInputCapabilities = parseRiInputCapabilities(offerSdp);
    const negotiatedPartialReliable = this.riInputCapabilities.partialReliableThresholdMs;
    this.partialReliableThresholdMs = negotiatedPartialReliable ?? GfnWebRtcClient.DEFAULT_PARTIAL_RELIABLE_THRESHOLD_MS;
    this.negotiatedMaxBitrateKbps = Math.max(
      GfnWebRtcClient.DECODER_MIN_RECOVERY_BITRATE_KBPS,
      Math.floor(settings.maxBitrateKbps),
    );
    this.currentBitrateCeilingKbps = this.negotiatedMaxBitrateKbps;
    this.log(
      `Input channel policy: partial reliable threshold=${this.partialReliableThresholdMs}ms${negotiatedPartialReliable === null ? " (fallback)" : ""}, hidMask=0x${this.riInputCapabilities.hidDeviceMask.toString(16)}, prGamepadMask=0x${this.riInputCapabilities.enablePartiallyReliableTransferGamepad.toString(16)}, prHidMask=0x${this.riInputCapabilities.enablePartiallyReliableTransferHid.toString(16)}`,
    );

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
      this.options.onPeerConnectionStateChange?.(pc.connectionState);
    };

    pc.ondatachannel = (event) => {
      const channel = event.channel;
      this.log(`Remote data channel received: label=${channel.label}, ordered=${channel.ordered}`);
      if (channel.label !== "control_channel") {
        return;
      }

      this.controlChannel = channel;
      this.controlChannel.binaryType = "arraybuffer";
      this.controlChannel.onmessage = (msgEvent) => {
        void this.onControlChannelMessage(msgEvent.data as string | Blob | ArrayBuffer);
      };
      this.controlChannel.onclose = () => {
        this.log("Control channel closed");
        if (this.controlChannel === channel) {
          this.controlChannel = null;
        }
      };
      this.controlChannel.onerror = () => {
        this.log("Control channel error");
      };
    };

    pc.onicecandidateerror = (event: Event) => {
      const e = event as RTCPeerConnectionIceErrorEvent;
      const hostCandidate = "hostCandidate" in e
        ? (e as RTCPeerConnectionIceErrorEvent & { hostCandidate?: string }).hostCandidate
        : undefined;
      this.log(`ICE candidate error: ${e.errorCode} ${e.errorText} (${e.url ?? "no url"}) hostCandidate=${hostCandidate ?? "?"}`);
    };

    pc.oniceconnectionstatechange = () => {
      this.log(`ICE connection state: ${pc.iceConnectionState}`);
      this.options.onIceConnectionStateChange?.(pc.iceConnectionState);
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

    const preferredHevcProfileId = hevcPreferredProfileId(settings.colorQuality);

    // 3. Filter to preferred codec — but only if the browser actually supports it
    let effectiveCodec = settings.codec;
    const supported = this.getSupportedVideoCodecs();
    this.log(`Browser supported video codecs: ${supported.join(", ") || "unknown"}`);

    if (settings.codec === "H265") {
      const hevcProfiles = this.getSupportedHevcProfiles();
      if (hevcProfiles.size > 0) {
        this.log(`Browser HEVC profile-id support: ${Array.from(hevcProfiles).join(", ")}`);
      }

      const hevcMaxLevels = this.getHevcMaxLevelsByProfile();
      if (hevcMaxLevels[1] || hevcMaxLevels[2]) {
        this.log(
          `Browser HEVC max level-id by profile: p1=${hevcMaxLevels[1] ?? "?"}, p2=${hevcMaxLevels[2] ?? "?"}`,
        );
        const rewrittenLevel = rewriteH265LevelIdByProfile(processedOffer, hevcMaxLevels);
        if (rewrittenLevel.replacements > 0) {
          this.log(
            `HEVC level compatibility: rewrote ${rewrittenLevel.replacements} fmtp lines to receiver max level-id`,
          );
          processedOffer = rewrittenLevel.sdp;
        }
      }

      const tierFlagOneSupported = this.supportsHevcTierFlagOne();
      this.log(`Browser HEVC tier-flag=1 support: ${tierFlagOneSupported ? "yes" : "no"}`);
      if (!tierFlagOneSupported) {
        const rewritten = rewriteH265TierFlag(processedOffer, 0);
        if (rewritten.replacements > 0) {
          this.log(
            `HEVC tier compatibility: rewrote ${rewritten.replacements} fmtp lines tier-flag=1 -> tier-flag=0`,
          );
          processedOffer = rewritten.sdp;
        }
      }
      if (hevcProfiles.size > 0 && !hevcProfiles.has(String(preferredHevcProfileId))) {
        this.log(
          `Warning: requested H265 profile-id=${preferredHevcProfileId} not reported in browser capabilities; forcing H265 anyway per user preference`,
        );
      }
    }

    if (supported.length > 0 && !supported.includes(settings.codec)) {
      this.log(`Warning: ${settings.codec} not reported in browser codec list; forcing requested codec anyway`);
    }
    this.log(`Effective codec: ${effectiveCodec} (preferred HEVC profile-id=${preferredHevcProfileId})`);
    const filteredOffer = preferCodec(processedOffer, effectiveCodec, {
      preferHevcProfileId: preferredHevcProfileId,
    });
    this.log(`Filtered offer SDP length: ${filteredOffer.length} chars`);
    this.log("Setting remote description (offer)...");
    await pc.setRemoteDescription({ type: "offer", sdp: filteredOffer });
    this.log("Remote description set successfully");
    await this.flushQueuedCandidates();

    // Attach microphone track to the correct transceiver after remote description is set
    if (this.micManager) {
      this.micManager.setPeerConnection(pc);
      await this.micManager.attachTrackToPeerConnection();
    }

    // 3b. Apply setCodecPreferences on the video transceiver to reinforce codec choice.
    //     This is the modern WebRTC API — more reliable than SDP munging alone.
    //     Must be called after setRemoteDescription (which creates the transceiver)
    //     but before createAnswer (which generates the answer SDP).
    this.applyCodecPreferences(pc, effectiveCodec, preferredHevcProfileId);

    // 4. Create answer, munge SDP, and set local description
    this.log("Creating answer...");
    const answer = await pc.createAnswer();
    this.log(`Answer created, SDP length: ${answer.sdp?.length ?? 0} chars`);

    // Munge answer SDP: inject b=AS: bitrate limits and stereo=1 for opus
    if (answer.sdp) {
      answer.sdp = mungeAnswerSdp(answer.sdp, settings.maxBitrateKbps);
      this.log(`Answer SDP munged (b=AS:${settings.maxBitrateKbps}, stereo=1)`);
    }

    await pc.setLocalDescription(answer);
    this.log("Local description set, waiting for ICE gathering...");

    const finalSdp = await this.waitForIceGathering(pc, 5000);
    this.log(`ICE gathering done, final SDP length: ${finalSdp.length} chars`);

    // Debug negotiated video codec/fmtp lines from local answer SDP
    {
      const lines = finalSdp.split(/\r?\n/);
      let inVideo = false;
      const negotiatedVideoLines: string[] = [];
      let hasNegotiatedH265 = false;
      for (const line of lines) {
        if (line.startsWith("m=video")) {
          inVideo = true;
          negotiatedVideoLines.push(line);
          continue;
        }
        if (line.startsWith("m=") && inVideo) {
          break;
        }
        if (inVideo && (line.startsWith("a=rtpmap:") || line.startsWith("a=fmtp:") || line.startsWith("a=rtcp-fb:"))) {
          negotiatedVideoLines.push(line);
          if (line.startsWith("a=rtpmap:") && /\sH(?:265|EVC)\//i.test(line)) {
            hasNegotiatedH265 = true;
          }
        }
      }
      if (negotiatedVideoLines.length > 0) {
        this.log("Negotiated local video SDP lines:");
        for (const l of negotiatedVideoLines) {
          this.log(`  SDP< ${l}`);
        }
      }

      if (effectiveCodec === "H265" && !hasNegotiatedH265) {
        throw new Error("H265 requested but not negotiated in local SDP (no H265 rtpmap in answer)");
      }
    }

    const credentials = extractIceCredentials(finalSdp);
    this.log(`Extracted ICE credentials: ufrag=${credentials.ufrag}, pwd=${credentials.pwd.slice(0, 8)}...`);
    const { width, height } = parseResolution(settings.resolution);

    const nvstSdp = buildNvstSdp({
      width,
      height,
      fps: settings.fps,
      maxBitrateKbps: settings.maxBitrateKbps,
      partialReliableThresholdMs: this.partialReliableThresholdMs,
      hidDeviceMask: this.riInputCapabilities.hidDeviceMask,
      enablePartiallyReliableTransferGamepad: this.riInputCapabilities.enablePartiallyReliableTransferGamepad,
      enablePartiallyReliableTransferHid: this.riInputCapabilities.enablePartiallyReliableTransferHid,
      codec: effectiveCodec,
      colorQuality: settings.colorQuality,
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

    // Cleanup microphone
    if (this.micManager) {
      this.micManager.dispose();
      this.micManager = null;
    }

    for (const track of this.videoStream.getTracks()) {
      this.videoStream.removeTrack(track);
    }
    for (const track of this.audioStream.getTracks()) {
      this.audioStream.removeTrack(track);
    }
  }

  /**
   * Initialize and start microphone capture
   */
  async startMicrophone(): Promise<boolean> {
    if (!this.micManager) {
      this.log("Microphone not available (mode disabled or not supported)");
      return false;
    }

    // Set peer connection for mic track
    if (this.pc) {
      this.micManager.setPeerConnection(this.pc);
    }

    const result = await this.micManager.initialize();
    if (result) {
      this.log("Microphone initialized successfully");
    } else {
      this.log("Microphone initialization failed");
    }
    return result;
  }

  /**
   * Stop microphone capture
   */
  stopMicrophone(): void {
    if (!this.micManager) return;

    this.micManager.stop();
    this.log("Microphone stopped");
  }

  /**
   * Toggle microphone mute/unmute
   */
  toggleMicrophone(): void {
    if (!this.micManager) return;

    const isEnabled = this.micManager.isEnabled();
    this.micManager.setEnabled(!isEnabled);
    this.log(`Microphone ${!isEnabled ? "unmuted" : "muted"}`);
  }

  /**
   * Set microphone enabled state
   */
  setMicrophoneEnabled(enabled: boolean): void {
    if (!this.micManager) return;

    this.micManager.setEnabled(enabled);
    this.log(`Microphone ${enabled ? "enabled" : "disabled"}`);
  }

  setMicrophoneLevel(level01: number): void {
    if (!this.micManager) return;
    this.micManager.setMicLevel(level01);
  }

  setOutputVolume(volume: number): void {
    const next = Math.max(0, Math.min(1, Number.isFinite(volume) ? volume : 1));
    this.outputVolume = next;
    this.options.audioElement.volume = next;
    if (this.audioGainNode) {
      this.audioGainNode.gain.value = next;
    }
  }

  getMicrophoneLevel(): number {
    return this.micManager?.getMicLevel() ?? 1;
  }

  /**
   * Check if microphone is currently enabled (unmuted)
   */
  isMicrophoneEnabled(): boolean {
    return this.micManager?.isEnabled() ?? false;
  }

  /**
   * Get current microphone state
   */
  getMicrophoneState(): MicState {
    return this.micState;
  }

  /**
   * Live audio track for UI metering / local recording mix: post-gain send path when available
   * (same levels the remote session hears), else raw capture.
   */
  getMicTrack(): MediaStreamTrack | null {
    return this.micManager?.getTrack() ?? null;
  }

  /**
   * Enumerate available microphone devices
   */
  async enumerateMicrophones(): Promise<MediaDeviceInfo[]> {
    if (!MicrophoneManager.isSupported()) {
      return [];
    }
    // Ensure permission first to get labels
    const manager = new MicrophoneManager();
    return await manager.enumerateDevices();
  }
}
