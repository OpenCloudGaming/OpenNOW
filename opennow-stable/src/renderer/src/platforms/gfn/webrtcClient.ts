import type {
  IceCandidatePayload,
  ColorQuality,
  FallbackCodecPreference,
  IceServer,
  SessionInfo,
  VideoCodec,
  MicrophoneMode,
  NativeTransitionDiagnostics,
  KeyboardLayout,
} from "@shared/gfn";
import {
  iceCandidateDiagnosticSummary,
  sdpDiagnosticSummary,
  signalingUrlForDiagnostics,
  streamDiagnosticId,
} from "@shared/gfn";

import {
  InputEncoder,
  INPUT_MOUSE_REL,
  PARTIALLY_RELIABLE_GAMEPAD_MASK_ALL,
  PARTIALLY_RELIABLE_HID_DEVICE_MASK_ALL,
  codeMap,
  startInputSessionClock,
  captureTimestampUs,
  sendTimestampUs,
  restampProtocolV3OuterTimestamp,
} from "./inputProtocol";
import {
  buildClipboardControlMessage,
  CLIPBOARD_CLIENT_ADDED_DATA,
  CLIPBOARD_CLIENT_DATA_RESPONSE,
  CLIPBOARD_CLIENT_REMOVED_DATA,
  isClipboardServerDataRequest,
  parseClipboardControlMessage,
  validateClipboardText,
  type ClipboardTracingData,
} from "./clipboardProtocol";
import {
  buildNvstSdp,
  extractIceCredentials,
  fixServerIp,
  mungeAnswerSdp,
  preferCodec,
  resolveNegotiationCandidates,
  extractNegotiatedVideoCodec,
  rewriteIceCandidateEndpoint,
  rewriteH265LevelIdByProfile,
  rewriteH265TierFlag,
  rewriteSdpIceCandidateEndpoints,
} from "./sdp";
import { MicrophoneManager, type MicState, type MicStateChange } from "./microphoneManager";
import type {
  StreamDiagnostics,
  StreamTimeWarning,
} from "./webrtc/streamDiagnosticsTypes";
import { classifyStreamLagReason } from "./webrtc/streamLag";
import { chooseAdaptiveMouseFlushInterval } from "./webrtc/mouseInput";
import {
  averageJitterBufferDelayMs,
  codecLabelFromMimeType,
  computeBitrateDiagnostics,
  computeIntervalFrameRates,
  detectGpuType,
} from "./webrtc/streamStatsHelpers";
import { extractActiveIceTransportStats } from "./webrtc/iceTransportStats";
import { parseStatsChannelGameFps } from "./webrtc/statsChannel";
import { deriveStreamSessionDiagnostics } from "./webrtc/sessionDiagnostics";
import {
  DecoderPressureController,
} from "./webrtc/decoderPressureController";
import {
  InputChannelPolicyController,
  type RiInputCapabilities,
} from "./webrtc/inputChannelPolicy";
import { GamepadController } from "./webrtc/gamepadController";
import { DomInputCaptureController } from "./webrtc/domInputCaptureController";
import { PeerMediaLifecycleController } from "./webrtc/peerMediaLifecycleController";
import { updateVideoSenderBitrate } from "./webrtc/senderBitrate";
import { CODEC_MIME_BY_NAME, buildCodecPreferenceList } from "./webrtc/codecPreferences";
import { negotiatePeerConnectionCodecAnswer } from "./webrtc/codecNegotiation";
import { OFFICIAL_MIN_BITRATE_KBPS } from "./sdp/nvstOffer";

export type {
  StreamDiagnostics,
  StreamLagReason,
  StreamTimeWarning,
} from "./webrtc/streamDiagnosticsTypes";
export {
  classifyStreamLagReason,
  type ClassifyStreamLagReasonParams,
} from "./webrtc/streamLag";
export {
  chooseAdaptiveMouseFlushInterval,
  quantizeMouseDeltaWithResidual,
  subsampleCoalescedPointerEvents,
  type AdaptiveMouseFlushDecisionParams,
} from "./webrtc/mouseInput";
export {
  evaluateControllerOverlayShortcutGate,
  type ControllerOverlayChordState,
  type ControllerOverlayShortcutGate,
} from "./webrtc/controllerOverlayGate";
export {
  classifyDecoderPressureSample,
  type DecoderPressureSample,
  type DecoderPressureSignal,
} from "./webrtc/decoderPressureController";
export { parseStatsChannelGameFps } from "./webrtc/statsChannel";
export {
  extractActiveIceTransportStats,
  type ActiveIceTransportStats,
  type IceTransportType,
} from "./webrtc/iceTransportStats";
export {
  canUsePartiallyReliableGamepad,
  canUsePartiallyReliableInput,
  type RiInputCapabilities,
} from "./webrtc/inputChannelPolicy";

interface OfferSettings {
  codec: VideoCodec;
  colorQuality: ColorQuality;
  resolution: string;
  fps: number;
  maxBitrateKbps: number;
  fallbackCodec?: FallbackCodecPreference;
  nativeTransitionDiagnostics?: NativeTransitionDiagnostics;
}

const DEFAULT_CLIPBOARD_MAX_BYTES = 1024 * 1024;

function hevcPreferredProfileId(colorQuality: ColorQuality): 1 | 2 {
  // 10-bit modes should prefer HEVC Main10 profile-id=2.
  return colorQuality.startsWith("10bit") ? 2 : 1;
}

function describeColorQuality(colorQuality: ColorQuality): string {
  switch (colorQuality) {
    case "8bit_420":
      return "8-bit 4:2:0";
    case "8bit_444":
      return "8-bit 4:4:4";
    case "10bit_420":
      return "10-bit 4:2:0";
    case "10bit_444":
      return "10-bit 4:4:4";
    default:
      return colorQuality;
  }
}

function describeNativeHardwareAcceleration(): string {
  const platform = navigator.platform.toLowerCase();
  if (platform.includes("win")) {
    return "Native D3D11/DXVA";
  }
  if (platform.includes("mac")) {
    return "Native VideoToolbox";
  }
  return "Native VA-API/V4L2/Vulkan";
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
  /** Use the WebRTC cursor_channel overlay; false leaves cursor rendering to the stream/server. */
  nativeCursorOverlay?: boolean;
  /** Mouse sensitivity multiplier (1.0 = default) */
  mouseSensitivity?: number;
  /** Software acceleration strength percentage (1-150) */
  mouseAcceleration?: number;
  /** Selected GFN keyboard layout for remote physical OEM key mapping. */
  keyboardLayout?: KeyboardLayout;
  /** Enable official GFN clipboard custom-message paste support. */
  clipboardPaste?: boolean;
  /** Host clipboard reader used for server paste requests. */
  readClipboardText?: () => Promise<string>;
  /** Maximum UTF-8 clipboard bytes to advertise/send. */
  clipboardMaxBytes?: number;
  onLog: (line: string) => void;
  onStats?: (stats: StreamDiagnostics) => void;
  onTimeWarning?: (warning: StreamTimeWarning) => void;
  onMicStateChange?: (state: MicStateChange) => void;
  onIceConnectionStateChange?: (state: RTCIceConnectionState) => void;
  onPeerConnectionStateChange?: (state: RTCPeerConnectionState) => void;
  /** Optional host callback for controller overlay shortcut edge presses. */
  onControllerMetaPress?: (event: { controllerId: number; gamepad: Gamepad }) => void;
}

function timestampUs(sourceTimestampMs?: number): bigint {
  return captureTimestampUs(sourceTimestampMs);
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

type CodecAttemptMicrophoneManager = Pick<
  MicrophoneManager,
  "setPeerConnection" | "attachTrackToPeerConnection"
>;

export async function attachMicrophoneForCodecAttempt(
  micManager: CodecAttemptMicrophoneManager | null,
  pc: RTCPeerConnection,
): Promise<void> {
  if (!micManager) return;
  micManager.setPeerConnection(pc);
  await micManager.attachTrackToPeerConnection();
}

export class GfnWebRtcClient {
  private readonly inputEncoder = new InputEncoder();

  private pc: RTCPeerConnection | null = null;
  private reliableInputChannel: RTCDataChannel | null = null;
  private partiallyReliableInputChannel: RTCDataChannel | null = null;
  private cursorChannel: RTCDataChannel | null = null;
  private controlChannel: RTCDataChannel | null = null;
  private nativeInputActive = false;
  /**
   * When true, Electron captures keyboard/mouse/gamepad and forwards packets to
   * the native streamer over IPC (internal child-surface renderer).
   * When false, the external native presenter owns OS-level input.
   */
  private nativeElectronInputBridge = false;
  private remoteIceEndpoint: SessionInfo["mediaConnectionInfo"] | null = null;

  private inputReady = false;
  /** When true, the host (e.g. in-stream controller menu) blocks forwarding; not cleared by focus/visibility. */
  public inputPaused = false;
  /** When true, window blur or document hidden blocks forwarding until focus/visible again. */
  private windowStateInputPaused = false;
  private inputProtocolVersion = 2;
  private heartbeatTimer: number | null = null;
  private statsTimer: number | null = null;
  private statsPollInFlight = false;
  private externalEscapeCleanup: (() => void) | null = null;
  private queuedCandidates: RTCIceCandidateInit[] = [];

  private static readonly NATIVE_INPUT_PROTOCOL_FALLBACK = 3;
  private static readonly MOUSE_FLUSH_NORMAL_MS = 8;
  private static readonly MOUSE_FLUSH_MIN_MS = 2;
  private static readonly MOUSE_FLUSH_MAX_MS = 20;
  private static readonly DEFAULT_PARTIAL_RELIABLE_THRESHOLD_MS = 300;
  private static readonly RELIABLE_MOUSE_BACKPRESSURE_BYTES = 64 * 1024;
  private static readonly BACKPRESSURE_LOG_INTERVAL_MS = 2000;

  private static normalizeInputProtocolVersion(protocolVersion: number): number {
    if (!Number.isFinite(protocolVersion)) {
      return 2;
    }
    return Math.min(255, Math.max(1, Math.trunc(protocolVersion)));
  }

  // Stats tracking
  private lastStatsSample: {
    bytesReceived: number;
    framesReceived: number;
    framesDecoded: number;
    framesDropped: number;
    packetsReceived: number;
    packetsLost: number;
    totalDecodeTime: number;
    atMs: number;
  } | null = null;
  private renderFpsCounter = { frames: 0, lastUpdate: 0, fps: 0 };
  private lastEmittedDiagnostics: StreamDiagnostics | null = null;
  private statsChannelVersionLogged = false;

  private keyboardLayout?: KeyboardLayout;
  private autoFullScreenEnabled = true;
  private clipboardPasteEnabled = false;
  private clipboardMaxBytes = DEFAULT_CLIPBOARD_MAX_BYTES;
  private lastAdvertisedClipboardAvailable: boolean | null = null;

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

  private readonly decoderPressureController: DecoderPressureController;
  private readonly inputChannelPolicyController: InputChannelPolicyController;
  private readonly gamepadController: GamepadController;
  private readonly domInputController: DomInputCaptureController;
  private readonly peerMediaController: PeerMediaLifecycleController;

  // Microphone
  private micManager: MicrophoneManager | null = null;
  private micState: MicState = "uninitialized";

  // Stream info
  private currentCodec = "";
  private currentResolution = "";
  private isHdr = false;
  private videoDecodeStallWarningSent = false;
  private sessionId = "";
  private serverRegion = "";
  private serverZone = "";
  private serverLocation = "";
  private serverGpuType = "";
  private gpuType = "";

  private diagnostics: StreamDiagnostics = {
    connectionState: "closed",
    inputReady: false,
    nativeRendererActive: false,
    connectedGamepads: 0,
    resolution: "",
    codec: "",
    requestedCodec: "",
    hardwareAcceleration: "Chromium GPU decode",
    colorCodec: "",
    isHdr: false,
    bitrateKbps: 0,
    targetBitrateKbps: 0,
    availableBitrateKbps: 0,
    decodeFps: 0,
    receiveFps: 0,
    renderFps: 0,
    gameFps: undefined,
    packetsLost: 0,
    packetsReceived: 0,
    packetLossPercent: 0,
    jitterMs: 0,
    rttMs: 0,
    transportType: "unknown",
    localCandidateType: "",
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
    serverGpuType: "",
    sessionId: "",
    serverRegion: "",
    serverZone: "",
    serverLocation: "",
    decoderPressureActive: false,
    decoderRecoveryAttempts: 0,
    decoderRecoveryAction: "none",
    nativeRequestedFps: undefined,
    nativeCapsFramerate: undefined,
    nativeQueueMode: undefined,
    nativeFramesPendingToPresent: undefined,
    nativePartialFlushCount: undefined,
    nativeCompleteFlushCount: undefined,
    nativeTransitionSummary: undefined,
    nativeRequestedStreamingFeaturesSummary: undefined,
    nativeFinalizedStreamingFeaturesSummary: undefined,
    micState: "uninitialized",
    micEnabled: false,
  };

  constructor(private readonly options: ClientOptions) {
    this.decoderPressureController = new DecoderPressureController({
      log: (message) => this.log(message),
      getPeerConnection: () => this.pc,
      getControlChannel: () => this.controlChannel,
      requestSignalingKeyframe: (request) => window.openNow.requestKeyframe(request),
      setMaxBitrateKbps: (kbps) => this.setMaxBitrateKbps(kbps),
      onStateChange: (state) => {
        this.diagnostics.decoderPressureActive = state.active;
        this.diagnostics.decoderRecoveryAttempts = state.recoveryAttempts;
        this.diagnostics.decoderRecoveryAction = state.recoveryAction;
      },
    });
    this.inputChannelPolicyController = new InputChannelPolicyController(
      this.riInputCapabilities,
      {
        isNativeInputActive: () => this.nativeInputActive,
        getPartiallyReliableChannel: () => this.partiallyReliableInputChannel,
        sendNativeInput: (payload, partiallyReliable) => {
          this.sendNativeInput(payload, partiallyReliable);
        },
        sendReliable: (payload) => this.sendReliable(payload),
      },
    );
    this.gamepadController = new GamepadController({
      inputEncoder: this.inputEncoder,
      isInputReady: () => this.inputReady,
      isInputPaused: () => this.inputPaused || this.windowStateInputPaused,
      isNativeInputActive: () => this.nativeInputActive,
      isNativeElectronInputBridge: () => this.nativeElectronInputBridge,
      isReliableChannelOpen: () => this.reliableInputChannel?.readyState === "open",
      canSendPartiallyReliableGamepad: (controllerId) => (
        this.inputChannelPolicyController.canSendGamepad(controllerId)
      ),
      sendPartiallyReliable: (payload) => {
        this.inputChannelPolicyController.sendPartiallyReliable(payload);
      },
      sendReliable: (payload) => this.sendReliable(payload),
      onControllerMetaPress: options.onControllerMetaPress,
      onConnectedGamepadsChanged: (count, emit) => {
        this.diagnostics.connectedGamepads = count;
        if (emit) {
          this.emitStats();
        }
      },
      log: (message) => this.log(message),
    });
    this.domInputController = new DomInputCaptureController(
      {
        videoElement: options.videoElement,
        inputEncoder: this.inputEncoder,
        isInputReady: () => this.inputReady,
        isInputBlocked: () => this.isStreamInputBlocked(),
        isNativeInputActive: () => this.nativeInputActive,
        isNativeElectronInputBridge: () => this.nativeElectronInputBridge,
        shouldAutoFullscreen: () => this.shouldAutoFullscreen(),
        getCurrentResolution: () => this.currentResolution,
        getKeyboardLayout: () => this.keyboardLayout,
        getMicState: () => this.micState,
        setWindowInputPaused: (paused) => {
          this.windowStateInputPaused = paused;
        },
        recordSchedulingDelay: (delayMs) => {
          this.inputQueueMaxSchedulingDelayMsWindow = Math.max(
            this.inputQueueMaxSchedulingDelayMsWindow,
            delayMs,
          );
        },
        refreshClipboardAvailability: () => this.refreshClipboardAvailability(),
        sendReliableSingleInput: (payload) => this.sendReliableSingleInput(payload),
        sendReliable: (payload) => this.sendReliable(payload),
        sendInputPacket: (payload, inputType) => this.sendInputPacket(payload, inputType),
        onGamepadConnected: this.gamepadController.onGamepadConnected,
        onGamepadDisconnected: this.gamepadController.onGamepadDisconnected,
        log: (message) => this.log(message),
      },
      {
        mouseSensitivity: options.mouseSensitivity ?? 1,
        mouseAccelerationPercent: Math.max(
          1,
          Math.min(150, Math.round(options.mouseAcceleration ?? 1)),
        ),
        nativeCursorOverlay: options.nativeCursorOverlay !== false,
      },
    );
    this.peerMediaController = new PeerMediaLifecycleController({
      videoElement: options.videoElement,
      audioElement: options.audioElement,
      onRenderFrame: () => this.updateRenderFps(),
      log: (message) => this.log(message),
    });
    this.keyboardLayout = options.keyboardLayout;
    this.autoFullScreenEnabled = options.autoFullScreen !== false;
    this.clipboardPasteEnabled = Boolean(options.clipboardPaste);
    this.clipboardMaxBytes = Math.max(0, Math.trunc(options.clipboardMaxBytes ?? DEFAULT_CLIPBOARD_MAX_BYTES));

    // Escape is intercepted by Electron before Chromium can leave fullscreen.
    // Keep this subscription alive for the whole stream-client lifetime: Windows
    // internal native mode intentionally detaches DOM input capture while RawInput
    // owns the rest of the keyboard and mouse path.
    try {
      this.externalEscapeCleanup = window.openNow.onExternalEscape(() => {
        if (!this.inputReady) return;

        this.log("Forwarding main-process Escape tap to the remote session");
        this.domInputController.releasePressedKeys("external Escape forwarded from main");

        const escDown = this.inputEncoder.encodeKeyDown({
          keycode: 0x1B,
          scancode: codeMap.Escape.scancode,
          modifiers: 0,
          timestampUs: timestampUs(),
        });
        this.sendReliableSingleInput(escDown);

        const escUp = this.inputEncoder.encodeKeyUp({
          keycode: 0x1B,
          scancode: codeMap.Escape.scancode,
          modifiers: 0,
          timestampUs: timestampUs(),
        });
        this.sendReliableSingleInput(escUp);
      });
    } catch {
      this.externalEscapeCleanup = null;
    }

    // Configure video element for lowest latency playback
    this.configureVideoElementForLowLatency(options.videoElement);

    // Detect GPU once on construction
    this.gpuType = detectGpuType();
    this.diagnostics.gpuType = this.gpuType;
    this.diagnostics.hardwareAcceleration = "Chromium GPU decode";

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

  private isNativeCursorOverlayEnabled(): boolean {
    return this.domInputController.isNativeCursorOverlayEnabled();
  }

  public setNativeCursorOverlayEnabled(value: boolean): void {
    const enabled = Boolean(value);
    if (this.isNativeCursorOverlayEnabled() === enabled) {
      return;
    }

    this.options.nativeCursorOverlay = enabled;
    this.domInputController.setNativeCursorOverlayEnabled(enabled);
    if (!enabled) {
      this.closeCursorChannel();
      this.log("Native cursor overlay disabled");
      return;
    }

    if (this.pc && !this.cursorChannel) {
      try {
        this.createCursorChannel(this.pc);
      } catch (error) {
        this.log(`Failed to open cursor channel: ${error instanceof Error ? error.message : String(error)}`);
      }
    }
    this.log("Native cursor overlay enabled");
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
    const sensitivity = Math.max(0.01, v);
    this.domInputController.setMouseSensitivity(sensitivity);
    this.log(`Mouse sensitivity set to ${sensitivity}`);
  }

  /** Update software mouse acceleration strength at runtime (1-150%). */
  public setMouseAccelerationPercent(value: number): void {
    const v = Number.isFinite(value) ? value : 1;
    const accelerationPercent = Math.max(1, Math.min(150, Math.round(v)));
    this.domInputController.setMouseAccelerationPercent(accelerationPercent);
    this.log(`Mouse acceleration set to ${accelerationPercent}%`);
  }

  /** Update fullscreen preference used by auto pointer-lock flows at runtime. */
  public setAutoFullScreen(value: boolean): void {
    this.autoFullScreenEnabled = Boolean(value);
    this.log(`Auto fullscreen ${this.autoFullScreenEnabled ? "enabled" : "disabled"}`);
  }

  public setClipboardPasteEnabled(value: boolean): void {
    const enabled = Boolean(value);
    if (this.clipboardPasteEnabled === enabled) {
      return;
    }
    this.clipboardPasteEnabled = enabled;
    this.lastAdvertisedClipboardAvailable = null;
    void this.refreshClipboardAvailability();
  }

  public async refreshClipboardAvailability(): Promise<boolean> {
    if (this.controlChannel?.readyState !== "open") {
      return false;
    }

    const text = this.clipboardPasteEnabled ? await this.readClipboardTextForPaste() : null;
    const available = Boolean(text);
    if (this.lastAdvertisedClipboardAvailable === available) {
      return available;
    }

    this.sendClipboardControlMessage(available ? CLIPBOARD_CLIENT_ADDED_DATA : CLIPBOARD_CLIENT_REMOVED_DATA);
    this.lastAdvertisedClipboardAvailable = available;
    return available;
  }

  public async pasteClipboardText(): Promise<boolean> {
    if (!this.inputReady || this.controlChannel?.readyState !== "open") {
      return false;
    }

    const available = await this.refreshClipboardAvailability();
    if (!available) {
      // Official GFN treats empty/oversized/unreadable clipboard data as a handled no-op.
      // Do not synthesize Ctrl+V here, or the server can paste stale remote clipboard data.
      return true;
    }
    return this.sendPasteShortcut(false);
  }

  private async readClipboardTextForPaste(): Promise<string | null> {
    if (!this.clipboardPasteEnabled || !this.options.readClipboardText) {
      return null;
    }

    try {
      const text = await this.options.readClipboardText();
      return validateClipboardText(text, this.clipboardMaxBytes);
    } catch (error) {
      this.log(`Clipboard read failed: ${error instanceof Error ? error.message : String(error)}`);
      return null;
    }
  }

  private sendClipboardControlMessage(
    pasteType: typeof CLIPBOARD_CLIENT_ADDED_DATA | typeof CLIPBOARD_CLIENT_REMOVED_DATA | typeof CLIPBOARD_CLIENT_DATA_RESPONSE,
    text?: string | null,
    tracingData?: ClipboardTracingData,
  ): boolean {
    if (this.controlChannel?.readyState !== "open") {
      return false;
    }

    this.controlChannel.send(JSON.stringify(buildClipboardControlMessage(pasteType, { text, tracingData })));
    return true;
  }

  private async handleClipboardServerRequest(tracingData?: ClipboardTracingData): Promise<void> {
    const text = await this.readClipboardTextForPaste();
    this.sendClipboardControlMessage(
      text ? CLIPBOARD_CLIENT_DATA_RESPONSE : CLIPBOARD_CLIENT_REMOVED_DATA,
      text,
      tracingData,
    );
    this.lastAdvertisedClipboardAvailable = Boolean(text);
  }

  public suppressNextSyntheticEscapeOnPointerLockLoss(durationMs = 1000): void {
    this.domInputController.suppressNextSyntheticEscapeOnPointerLockLoss(durationMs);
  }

  public async setMaxBitrateKbps(kbps: number): Promise<boolean> {
    const normalizedKbps = Math.max(OFFICIAL_MIN_BITRATE_KBPS, Math.floor(kbps));
    if (!this.pc) {
      return false;
    }

    const result = await updateVideoSenderBitrate(this.pc, normalizedKbps);
    if (result.status === "updated") {
      this.log(`Bitrate ceiling updated to ${normalizedKbps} kbps via sender parameters`);
      return true;
    } else if (result.status === "unavailable") {
      this.log(
        `No video sender supports a live bitrate update; ${normalizedKbps} kbps applies on the next session`,
      );
    } else {
      this.log(
        `Video sender rejected the live bitrate update (non-fatal): ${String(result.error)}`,
      );
    }
    return false;
  }

  /**
   * Keep the receiver on libwebrtc's adaptive jitter target during normal
   * playback, matching the smooth Android-native path. A small explicit target
   * is used only while recovering from decoder pressure.
   */
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

  private resetDiagnostics(): void {
    this.lastStatsSample = null;
    this.lastEmittedDiagnostics = null;
    this.currentCodec = "";
    this.currentResolution = "";
    this.isHdr = false;
    this.videoDecodeStallWarningSent = false;
    this.statsChannelVersionLogged = false;
    this.decoderPressureController.reset();
    const mouseDiagnostics = this.domInputController.getMouseDiagnostics();
    this.diagnostics = {
      connectionState: this.pc?.connectionState ?? "closed",
      inputReady: false,
      nativeRendererActive: false,
      connectedGamepads: 0,
      resolution: "",
      codec: "",
      requestedCodec: "",
      hardwareAcceleration: "Chromium GPU decode",
      colorCodec: "",
      isHdr: false,
      bitrateKbps: 0,
      targetBitrateKbps: 0,
      availableBitrateKbps: 0,
      decodeFps: 0,
      receiveFps: 0,
      renderFps: 0,
      gameFps: undefined,
      packetsLost: 0,
      packetsReceived: 0,
      packetLossPercent: 0,
      jitterMs: 0,
      rttMs: 0,
      transportType: "unknown",
      localCandidateType: "",
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
      mouseFlushIntervalMs: mouseDiagnostics.flushIntervalMs,
      mousePacketsPerSecond: mouseDiagnostics.packetsPerSecond,
      mouseResidualMagnitude: 0,
      mouseAdaptiveFlushActive: mouseDiagnostics.adaptiveFlushActive,
      lagReason: "unknown",
      lagReasonDetail: "Waiting for stream stats",
      gpuType: this.gpuType,
      serverGpuType: this.serverGpuType,
      sessionId: this.sessionId,
      serverRegion: this.serverRegion,
      serverZone: this.serverZone,
      serverLocation: this.serverLocation,
      decoderPressureActive: false,
      decoderRecoveryAttempts: 0,
      decoderRecoveryAction: "none",
      nativeRequestedFps: undefined,
      nativeCapsFramerate: undefined,
      nativeQueueMode: undefined,
      nativeFramesPendingToPresent: undefined,
      nativePartialFlushCount: undefined,
      nativeCompleteFlushCount: undefined,
      nativeTransitionSummary: undefined,
      nativeRequestedStreamingFeaturesSummary: undefined,
      nativeFinalizedStreamingFeaturesSummary: undefined,
      micState: this.micState,
      micEnabled: this.micManager?.isEnabled() ?? false,
    };
    this.emitStats();
  }

  private resetInputState(): void {
    this.inputReady = false;
    this.nativeInputActive = false;
    this.nativeElectronInputBridge = false;
    this.inputProtocolVersion = 2;
    this.inputEncoder.setProtocolVersion(2);
    this.diagnostics.inputReady = false;
    this.diagnostics.nativeRendererActive = false;
    this.diagnostics.partiallyReliableInputOpen = false;
    this.diagnostics.mouseMoveTransport = "reliable";
    this.emitStats();
  }

  private applyStreamSettingsDiagnostics(
    settings: OfferSettings,
    codec: VideoCodec,
    nativeRendererActive: boolean,
  ): void {
    this.currentCodec = codec;
    this.currentResolution = settings.resolution;
    this.isHdr = settings.colorQuality.startsWith("10bit");
    this.decoderPressureController.initializeBitrate(settings.maxBitrateKbps);

    this.diagnostics.resolution = settings.resolution;
    this.diagnostics.codec = codec;
    this.diagnostics.requestedCodec = codec;
    this.diagnostics.hardwareAcceleration = nativeRendererActive
      ? describeNativeHardwareAcceleration()
      : "Chromium GPU decode";
    this.diagnostics.colorCodec = describeColorQuality(settings.colorQuality);
    this.diagnostics.isHdr = this.isHdr;
    this.diagnostics.targetBitrateKbps = this.decoderPressureController.targetBitrateKbps;
    this.diagnostics.decodeFps = nativeRendererActive ? settings.fps : 0;
    this.diagnostics.receiveFps = 0;
    this.diagnostics.renderFps = nativeRendererActive ? settings.fps : 0;
    this.domInputController.setFallbackResolution(settings.resolution);
  }

  private closeDataChannels(): void {
    if (this.controlChannel) {
      this.controlChannel.onmessage = null;
      this.controlChannel.onclose = null;
      this.controlChannel.onerror = null;
    }
    this.reliableInputChannel?.close();
    this.partiallyReliableInputChannel?.close();
    this.closeCursorChannel();
    this.controlChannel?.close();
    this.reliableInputChannel = null;
    this.partiallyReliableInputChannel = null;
    this.controlChannel = null;
  }

  private closeCursorChannel(): void {
    if (!this.cursorChannel) {
      return;
    }
    this.cursorChannel.onmessage = null;
    this.cursorChannel.onclose = null;
    this.cursorChannel.onerror = null;
    this.cursorChannel.close();
    this.cursorChannel = null;
  }

  private clearTimers(): void {
    if (this.heartbeatTimer !== null) {
      window.clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
    if (this.statsTimer !== null) {
      window.clearInterval(this.statsTimer);
      this.statsTimer = null;
    }
    this.gamepadController.stop();
    this.domInputController.clearSyntheticEscapeSuppression();
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
    const statsEntries: Record<string, unknown>[] = [];
    let framesReceived = 0;
    let framesDecoded = 0;
    let framesDropped = 0;
    let pressureSignal = {
      active: false,
      reason: "stable",
      backlogFrames: 0,
      dropRatePercent: 0,
    };

    for (const entry of report.values()) {
      const stats = entry as unknown as Record<string, unknown>;
      statsEntries.push(stats);

      if (entry.type === "inbound-rtp" && stats.kind === "video") {
        inboundVideo = stats;
      }

      // Collect codec information
      if (entry.type === "codec") {
        const codecId = stats.id as string;
        codecs.set(codecId, stats);
      }
    }

    const iceTransport = extractActiveIceTransportStats(statsEntries);
    activePair = iceTransport.activePair;
    this.diagnostics.transportType = iceTransport.transportType;
    this.diagnostics.localCandidateType = iceTransport.localCandidateType;

    // Process video track stats
    if (inboundVideo) {
      const bytes = Number(inboundVideo.bytesReceived ?? 0);
      framesReceived = Number(inboundVideo.framesReceived ?? 0);
      framesDecoded = Number(inboundVideo.framesDecoded ?? 0);
      framesDropped = Number(inboundVideo.framesDropped ?? 0);
      const packetsReceived = Number(inboundVideo.packetsReceived ?? 0);
      const packetsLost = Number(inboundVideo.packetsLost ?? 0);
      const totalDecodeTime = Number(inboundVideo.totalDecodeTime ?? 0);
      const prevSample = this.lastStatsSample;

      // Calculate bitrate
      if (prevSample) {
        const bytesDelta = bytes - prevSample.bytesReceived;
        const timeDeltaMs = now - prevSample.atMs;
        if (bytesDelta >= 0 && timeDeltaMs > 0) {
          const kbps = (bytesDelta * 8) / (timeDeltaMs / 1000) / 1000;
          this.diagnostics.bitrateKbps = Math.max(0, Math.round(kbps));
        }

        const frameRates = computeIntervalFrameRates({
          framesReceived,
          framesDecoded,
          totalDecodeTime,
          prevFramesReceived: prevSample.framesReceived,
          prevFramesDecoded: prevSample.framesDecoded,
          prevTotalDecodeTime: prevSample.totalDecodeTime,
          timeDeltaMs,
          prevReceiveFps: this.diagnostics.receiveFps,
          prevDecodeFps: this.diagnostics.decodeFps,
          prevDecodeTimeMs: this.diagnostics.decodeTimeMs,
        });
        this.diagnostics.receiveFps = frameRates.receiveFps;
        this.diagnostics.decodeFps = frameRates.decodeFps;
        this.diagnostics.decodeTimeMs = frameRates.decodeTimeMs;

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
        totalDecodeTime,
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
      const avgJitterBufferDelayMs = averageJitterBufferDelayMs(jbDelay, jbEmitted);
      if (avgJitterBufferDelayMs !== null) {
        this.diagnostics.jitterBufferDelayMs = avgJitterBufferDelayMs;
      }

      // Get codec information
      const codecId = inboundVideo.codecId as string;
      if (codecId && codecs.has(codecId)) {
        const codecStats = codecs.get(codecId)!;
        const mimeType = (codecStats.mimeType as string) || "";
        const sdpFmtpLine = (codecStats.sdpFmtpLine as string) || "";

        this.currentCodec = codecLabelFromMimeType(mimeType, codecId);

        // Check for HDR in SDP fmtp line
        this.isHdr = sdpFmtpLine.includes("transfer-characteristics=16") ||
          sdpFmtpLine.includes("hdr") ||
          sdpFmtpLine.includes("HDR");

        this.diagnostics.codec = this.currentCodec;
        this.diagnostics.isHdr = this.isHdr;
      }

      // Get video dimensions from track settings if available
      const videoTrack = this.peerMediaController.getVideoTrack();
      if (videoTrack) {
        const settings = videoTrack.getSettings();
        if (settings.width && settings.height) {
          this.currentResolution = `${settings.width}x${settings.height}`;
          this.diagnostics.resolution = this.currentResolution;
        }
      }

      // Get decode timing if available
      const totalInterFrameDelay = Number(inboundVideo.totalInterFrameDelay ?? 0);
      const framesDecodedForTiming = Number(inboundVideo.framesDecoded ?? 1);

      if (
        !prevSample &&
        framesDecodedForTiming > 0 &&
        Number.isFinite(totalDecodeTime) &&
        totalDecodeTime >= 0
      ) {
        this.diagnostics.decodeTimeMs = Math.round((totalDecodeTime / framesDecodedForTiming) * 1000 * 10) / 10;
      }

      // Estimate render time from inter-frame delay
      if (totalInterFrameDelay > 0 && framesDecodedForTiming > 1) {
        const avgFrameDelay = totalInterFrameDelay / (framesDecodedForTiming - 1);
        this.diagnostics.renderTimeMs = Math.round(avgFrameDelay * 1000 * 10) / 10;
      }

      pressureSignal = this.decoderPressureController.classifySample({
        framesReceived,
        framesDecoded,
        framesDropped,
        decodeTimeMs: this.diagnostics.decodeTimeMs,
        decodeFps: this.diagnostics.decodeFps,
        prevSample,
      });
      await this.decoderPressureController.recover(pressureSignal);
    }

    const bitrateDiagnostics = computeBitrateDiagnostics(
      this.decoderPressureController.targetBitrateKbps,
      activePair,
    );
    this.diagnostics.targetBitrateKbps = bitrateDiagnostics.targetBitrateKbps;
    this.diagnostics.availableBitrateKbps = bitrateDiagnostics.availableBitrateKbps;

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
    const mouseDiagnostics = this.domInputController.getMouseDiagnostics();
    this.diagnostics.mouseFlushIntervalMs = mouseDiagnostics.flushIntervalMs;
    this.diagnostics.mousePacketsPerSecond = mouseDiagnostics.packetsPerSecond;
    this.diagnostics.mouseResidualMagnitude = mouseDiagnostics.residualMagnitude;

    // Intentional adaptive coalesce: only when mouse moves ride the reliable
    // channel (PR mouse keeps the fixed 4/8/16 ms official interval). Skip while
    // pointerrawupdate forced immediate flush (interval 0).
    if (mouseDiagnostics.flushIntervalMs <= 0 || mouseDiagnostics.flushBaseIntervalMs <= 0) {
      this.domInputController.setAdaptiveFlushInterval(mouseDiagnostics.flushIntervalMs, false);
    } else if (this.canSendInputTypePartiallyReliable(INPUT_MOUSE_REL)) {
      // Official GFN keeps a fixed coalesce interval for PR mouse.
      this.domInputController.setAdaptiveFlushInterval(
        mouseDiagnostics.flushBaseIntervalMs,
        false,
      );
      this.diagnostics.mouseFlushIntervalMs = mouseDiagnostics.flushBaseIntervalMs;
    } else {
      const nextInterval = chooseAdaptiveMouseFlushInterval({
        baseIntervalMs: mouseDiagnostics.flushBaseIntervalMs,
        currentIntervalMs: mouseDiagnostics.flushIntervalMs,
        reliableBufferedAmount,
        schedulingDelayMs: this.inputQueueMaxSchedulingDelayMsWindow,
        canUsePartiallyReliableMouse: false,
        backpressureThresholdBytes: GfnWebRtcClient.RELIABLE_MOUSE_BACKPRESSURE_BYTES,
        minIntervalMs: GfnWebRtcClient.MOUSE_FLUSH_MIN_MS,
        maxIntervalMs: GfnWebRtcClient.MOUSE_FLUSH_MAX_MS,
      });
      const adaptive = nextInterval !== mouseDiagnostics.flushBaseIntervalMs;
      this.domInputController.setAdaptiveFlushInterval(nextInterval, adaptive);
      this.diagnostics.mouseFlushIntervalMs = nextInterval;
    }
    this.diagnostics.mouseAdaptiveFlushActive =
      this.domInputController.getMouseDiagnostics().adaptiveFlushActive;

    const lagClassification = classifyStreamLagReason({
      nativeInputActive: this.nativeInputActive,
      nativeRendererActive: this.diagnostics.nativeRendererActive,
      framesReceived,
      framesDecoded,
      decodeTimeMs: this.diagnostics.decodeTimeMs,
      decodeFps: this.diagnostics.decodeFps,
      renderFps: this.diagnostics.renderFps,
      rttMs: this.diagnostics.rttMs,
      packetLossPercent: this.diagnostics.packetLossPercent,
      jitterMs: this.diagnostics.jitterMs,
      jitterBufferDelayMs: this.diagnostics.jitterBufferDelayMs,
      inputQueueBufferedBytes: reliableBufferedAmount,
      inputQueueDropCount: this.inputQueueDropCount,
      decoderPressureActive: pressureSignal.active,
      decoderPressureReason: pressureSignal.reason,
      decoderBacklogFrames: pressureSignal.backlogFrames,
      dropRatePercent: pressureSignal.dropRatePercent,
      backpressureThresholdBytes: GfnWebRtcClient.RELIABLE_MOUSE_BACKPRESSURE_BYTES,
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
    this.domInputController.detach();
    this.gamepadController.stopHaptics();
  }

  private cleanupPeerConnection(): void {
    this.clearTimers();
    this.detachInputCapture();
    this.closeDataChannels();
    this.peerMediaController.cleanupAudio();
    this.remoteIceEndpoint = null;
    if (this.pc) {
      this.pc.onicecandidate = null;
      this.pc.ontrack = null;
      this.pc.onconnectionstatechange = null;
      this.pc.ondatachannel = null;
      this.pc.close();
      this.pc = null;
    }
    this.peerMediaController.clearTracks();

    this.resetInputState();
    this.resetDiagnostics();
    this.gamepadController.reset();
    this.reliableDropLogged = false;
    this.domInputController.reset();
    this.inputQueuePeakBufferedBytesWindow = 0;
    this.partiallyReliableInputQueuePeakBufferedBytesWindow = 0;
    this.inputQueueMaxSchedulingDelayMsWindow = 0;
    this.inputQueueDropCount = 0;
    this.inputQueuePressureLoggedAtMs = 0;
  }

  public activateNativeInput(
    protocolVersion?: number,
    settings?: OfferSettings,
    options?: { electronInputBridge?: boolean },
  ): void {
    this.cleanupPeerConnection();
    this.nativeInputActive = true;
    // Internal (one-window) mode: Electron owns capture and IPC-forwards packets.
    // External floating window: OS-level capture stays in the native streamer.
    // Linux is Internal-only: always use the Electron IPC bridge regardless of stale options.
    const isLinuxHost = typeof navigator !== "undefined"
      && /linux/i.test(`${navigator.platform} ${navigator.userAgent}`);
    this.nativeElectronInputBridge = isLinuxHost || options?.electronInputBridge !== false;
    this.inputReady = true;
    const nativeProtocolVersion = GfnWebRtcClient.normalizeInputProtocolVersion(
      protocolVersion
        ?? (this.inputProtocolVersion > 2
          ? this.inputProtocolVersion
          : GfnWebRtcClient.NATIVE_INPUT_PROTOCOL_FALLBACK),
    );
    this.inputProtocolVersion = nativeProtocolVersion;
    this.inputEncoder.setProtocolVersion(nativeProtocolVersion);
    this.diagnostics.connectionState = "connected";
    this.diagnostics.inputReady = true;
    this.diagnostics.nativeRendererActive = true;
    if (settings) {
      this.applyStreamSettingsDiagnostics(settings, settings.codec, true);
    } else {
      this.diagnostics.hardwareAcceleration = describeNativeHardwareAcceleration();
      this.diagnostics.codec = this.currentCodec || "Native";
    }
    this.diagnostics.lagReason = "stable";
    this.diagnostics.lagReasonDetail = this.nativeElectronInputBridge
      ? "Native streamer Electron input bridge active"
      : "Native streamer external-window input active";
    this.diagnostics.inputQueueBufferedBytes = 0;
    this.diagnostics.inputQueuePeakBufferedBytes = 0;
    this.diagnostics.partiallyReliableInputQueueBufferedBytes = 0;
    this.diagnostics.partiallyReliableInputQueuePeakBufferedBytes = 0;
    this.diagnostics.inputQueueDropCount = 0;
    this.diagnostics.inputQueueMaxSchedulingDelayMs = 0;
    this.diagnostics.mouseAdaptiveFlushActive = false;
    this.diagnostics.mousePacketsPerSecond = 0;
    this.diagnostics.mouseResidualMagnitude = 0;
    this.diagnostics.partiallyReliableInputOpen = true;
    this.diagnostics.mouseMoveTransport = this.canSendInputTypePartiallyReliable(INPUT_MOUSE_REL)
      ? "partially_reliable"
      : "reliable";
    this.emitStats();
    this.inputPaused = false;
    this.windowStateInputPaused = false;

    if (this.nativeElectronInputBridge) {
      // Native mode never runs handleOffer() in the renderer, so input listeners
      // were never installed. Re-attach capture and forward via sendNativeInput.
      // Defer one frame so the StreamView native-hole DOM is painted and focusable.
      this.domInputController.install(this.options.videoElement);
      this.gamepadController.start();
      const video = this.options.videoElement;
      const focusTarget = (video.parentElement as HTMLElement | null) ?? video;
      requestAnimationFrame(() => {
        try {
          focusTarget.focus({ preventScroll: true });
        } catch {
          focusTarget.focus();
        }
        // Kick pointer lock so relative mouse works immediately in internal mode.
        void this.domInputController.requestPointerLockCompat(focusTarget, { unadjustedMovement: true }).catch(() => {
          void this.domInputController.requestPointerLockCompat(focusTarget).catch(() => {});
        });
      });
      this.log(
        `Native internal input bridge active (protocol v${nativeProtocolVersion}); Electron keyboard/mouse/gamepad → IPC → streamer.`,
      );
    } else {
      this.detachInputCapture();
      // Overlay Meta/Home detection only; gamepad state is owned by the floating window.
      this.gamepadController.start();
      this.log(
        `Native external-window input active (protocol v${nativeProtocolVersion}); OS capture handled by streamer, Electron overlay shortcuts only.`,
      );
    }
  }

  public setNativeInputProtocolVersion(protocolVersion: number): void {
    const version = GfnWebRtcClient.normalizeInputProtocolVersion(protocolVersion);
    if (this.inputProtocolVersion === version) {
      return;
    }

    this.inputProtocolVersion = version;
    this.inputEncoder.setProtocolVersion(version);
    this.gamepadController.resetProtocolState();
    this.log(`Native input protocol updated to v${version}`);

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

  private isPartiallyReliableChannelOpen(): boolean {
    return this.inputChannelPolicyController.isPartiallyReliableOpen();
  }

  private canSendGamepadPartiallyReliable(controllerId: number): boolean {
    return this.inputChannelPolicyController.canSendGamepad(controllerId);
  }

  private canSendInputTypePartiallyReliable(inputType: number): boolean {
    return this.inputChannelPolicyController.canSendInput(inputType);
  }

  private sendPartiallyReliable(payload: Uint8Array): void {
    this.inputChannelPolicyController.sendPartiallyReliable(payload);
  }

  private sendInputPacket(payload: Uint8Array, inputType: number): void {
    this.inputChannelPolicyController.sendInput(payload, inputType);
  }

  private isStreamInputBlocked(): boolean {
    const sidebarOpen = typeof document !== "undefined"
      && document.body?.dataset?.sidebarOpen === "1";
    return this.inputPaused || this.windowStateInputPaused || sidebarOpen;
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
      this.gamepadController.handleHapticsMessage(bytes);
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
      startInputSessionClock();
      this.inputReady = true;
      this.inputProtocolVersion = version;
      this.inputEncoder.setProtocolVersion(version);
      this.diagnostics.inputReady = true;
      this.emitStats();
      this.log(`Input handshake complete (protocol v${version}) — starting heartbeat + gamepad polling`);
      this.gamepadController.refreshHapticsAdvertisement();
      this.setupInputHeartbeat();
      this.gamepadController.start();
      // After input becomes ready, attempt to auto-enable pointer lock.
      void this.domInputController.attemptAutoPointerLock(this.shouldAutoFullscreen()).catch(() => {});
    }
  }

  private handleStatsChannelMessage(buffer: ArrayBuffer): void {
    const parsed = parseStatsChannelGameFps(buffer);
    if (!parsed) {
      return;
    }

    if (!this.statsChannelVersionLogged) {
      this.statsChannelVersionLogged = true;
      this.log(`Stats channel: protocol version ${parsed.version}, gameFps=${parsed.fps}`);
    }
    this.diagnostics.gameFps = parsed.fps;
    this.emitStats();
  }

  private createDataChannels(pc: RTCPeerConnection): void {
    const statsChannel = pc.createDataChannel("stats_channel", {
      ordered: false,
      maxRetransmits: 0,
    });
    statsChannel.binaryType = "arraybuffer";
    statsChannel.onmessage = (event) => {
      if (event.data instanceof ArrayBuffer) {
        this.handleStatsChannelMessage(event.data);
      }
    };

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

    if (!this.isNativeCursorOverlayEnabled()) {
      this.log("Cursor channel disabled; using server-side cursor rendering");
      return;
    }

    this.createCursorChannel(pc);
  }

  private createCursorChannel(pc: RTCPeerConnection): void {
    if (this.cursorChannel) {
      return;
    }

    this.cursorChannel = pc.createDataChannel("cursor_channel", {
      ordered: true,
    });
    this.cursorChannel.binaryType = "arraybuffer";
    this.cursorChannel.onopen = () => {
      this.log("Cursor channel open");
    };
    this.cursorChannel.onmessage = async (event) => {
      const bytes = await toBytes(event.data as string | Blob | ArrayBuffer);
      if (!this.domInputController.handleCursorMessage(bytes)) {
        this.log(`Cursor channel message ignored (${bytes.length} bytes)`);
      }
    };
    this.cursorChannel.onclose = () => {
      this.log("Cursor channel closed");
    };
    this.cursorChannel.onerror = () => {
      this.log("Cursor channel error");
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

    const clipboardPayload = parseClipboardControlMessage(parsed);
    if (isClipboardServerDataRequest(clipboardPayload)) {
      void this.handleClipboardServerRequest(clipboardPayload?.tracingData);
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
      await this.pc.addIceCandidate(this.rewriteRemoteIceCandidateInit(candidate));
    }
  }

  private rewriteRemoteIceCandidateInit(candidate: RTCIceCandidateInit): RTCIceCandidateInit {
    if (!candidate.candidate) {
      return candidate;
    }

    const rewritten = rewriteIceCandidateEndpoint(candidate.candidate, this.remoteIceEndpoint);
    if (!rewritten.rewritten) {
      return candidate;
    }

    if (this.remoteIceEndpoint) {
      this.log(
        `Rewrote remote ICE candidate endpoint to mediaConnectionInfo ${this.remoteIceEndpoint.ip}:${this.remoteIceEndpoint.port}`,
      );
    }

    return {
      ...candidate,
      candidate: rewritten.candidate,
    };
  }

  private reliableDropLogged = false;

  /**
   * Send a reliable single-input packet immediately (official GFN Jc()->Tc()).
   * When a mouse batch is pending, flush it first (official kc(): cl() then send key).
   */
  private sendReliableSingleInput(payload: Uint8Array): void {
    this.domInputController.flushPendingMovement();

    let packet = payload;
    if (this.inputProtocolVersion > 2) {
      packet = payload.slice();
      restampProtocolV3OuterTimestamp(packet, sendTimestampUs());
    } else if (payload.byteOffset !== 0 || payload.byteLength !== payload.buffer.byteLength) {
      packet = payload.slice();
    }

    this.sendReliable(packet);
  }

  private sendNativeInput(payload: Uint8Array, partiallyReliable: boolean): void {
    const safePayload = payload.byteOffset === 0 && payload.byteLength === payload.buffer.byteLength
      ? payload
      : payload.slice();
    window.openNow.sendNativeInput({
      payload: safePayload,
      partiallyReliable,
    });
  }

  public sendReliable(payload: Uint8Array): void {
    if (this.nativeInputActive) {
      this.sendNativeInput(payload, false);
      return;
    }

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

  public sendAntiAfkPulse(): boolean {
    return this.domInputController.sendAntiAfkPulse();
  }

  public sendPasteShortcut(useMeta: boolean): boolean {
    return this.domInputController.sendPasteShortcut(useMeta);
  }

  public sendText(text: string): number {
    return this.domInputController.sendText(text);
  }

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
    keepFallbacks = false,
    fallbackCodec?: VideoCodec,
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

      const codecList = buildCodecPreferenceList(receiverCaps, codec, {
        preferredHevcProfileId,
        keepFallbacks,
        fallbackCodec,
      });
      const preferredCount = receiverCaps.filter(
        (entry) => entry.mimeType.toLowerCase() === CODEC_MIME_BY_NAME[codec].toLowerCase(),
      ).length;

      if (codecList.length === 0) {
        this.log(`setCodecPreferences: no usable video codecs for ${codec}, skipping`);
        return;
      }
      if (preferredCount === 0) {
        this.log(`setCodecPreferences: ${codec} unavailable; using fallback primaries`);
      }

      try {
        videoTransceiver.setCodecPreferences(codecList);
        this.log(
          `setCodecPreferences: set ${codec} (${codecList.length} codecs${keepFallbacks ? " with fallbacks" : ""})`,
        );
      } catch (e) {
        this.log(`setCodecPreferences: receiver-only failed (${String(e)}), retrying with sender capabilities`);
        try {
          videoTransceiver.setCodecPreferences(codecList.concat(senderCaps));
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
    this.remoteIceEndpoint = session.mediaConnectionInfo ?? null;
    this.log("=== handleOffer START ===");
    this.log(`Session: key=${streamDiagnosticId(session.sessionId)}, status=${session.status}, serverIp=${session.serverIp}`);
    this.log(`Signaling: server=${session.signalingServer}, url=${signalingUrlForDiagnostics(session.signalingUrl, session.sessionId)}`);
    this.log(`MediaConnectionInfo: ${session.mediaConnectionInfo ? `ip=${session.mediaConnectionInfo.ip}, port=${session.mediaConnectionInfo.port}` : "NONE"}`);
    this.log(
      `Settings: codec=${settings.codec}, colorQuality=${settings.colorQuality}, resolution=${settings.resolution}, fps=${settings.fps}, maxBitrate=${settings.maxBitrateKbps}kbps`,
    );
    if (session.negotiatedStreamProfile) {
      this.log(`Negotiated stream profile override: ${JSON.stringify(session.negotiatedStreamProfile)}`);
    }
    this.log(`ICE servers: ${session.iceServers.length} (${session.iceServers.map(s => s.urls.join(",")).join(" | ")})`);
    this.log(sdpDiagnosticSummary("Received offer", offerSdp));

    this.riInputCapabilities = parseRiInputCapabilities(offerSdp);
    this.inputChannelPolicyController.updateCapabilities(this.riInputCapabilities);
    const negotiatedPartialReliable = this.riInputCapabilities.partialReliableThresholdMs;
    this.partialReliableThresholdMs = negotiatedPartialReliable ?? GfnWebRtcClient.DEFAULT_PARTIAL_RELIABLE_THRESHOLD_MS;
    this.decoderPressureController.initializeBitrate(settings.maxBitrateKbps);
    this.log(
      `Input channel policy: partial reliable threshold=${this.partialReliableThresholdMs}ms${negotiatedPartialReliable === null ? " (fallback)" : ""}, hidMask=0x${this.riInputCapabilities.hidDeviceMask.toString(16)}, prGamepadMask=0x${this.riInputCapabilities.enablePartiallyReliableTransferGamepad.toString(16)}, prHidMask=0x${this.riInputCapabilities.enablePartiallyReliableTransferHid.toString(16)}`,
    );

    const sessionDiagnostics = deriveStreamSessionDiagnostics(session);
    this.sessionId = sessionDiagnostics.sessionId;
    this.serverZone = sessionDiagnostics.serverZone;
    this.serverLocation = sessionDiagnostics.serverLocation;
    this.serverGpuType = sessionDiagnostics.serverGpuType;
    this.serverRegion = sessionDiagnostics.serverRegion;

    const rtcConfig: RTCConfiguration = {
      iceServers: toRtcIceServers(session.iceServers),
      bundlePolicy: "max-bundle",
      rtcpMuxPolicy: "require",
    };

    const pc = new RTCPeerConnection(rtcConfig);
    this.pc = pc;
    this.resetInputState();
    this.resetDiagnostics();
    this.diagnostics.connectionState = pc.connectionState;
    this.emitStats();
    this.createDataChannels(pc);
    this.domInputController.install(this.options.videoElement);
    this.setupStatsPolling();

    let answerSent = false;
    const queuedLocalIce: IceCandidatePayload[] = [];
    const sendLocalIce = (candidate: IceCandidatePayload): void => {
      window.openNow.sendIceCandidate(candidate).catch((error) => {
        this.log(`Failed to send local ICE candidate: ${String(error)}`);
      });
    };

    pc.onicecandidate = (event) => {
      if (!event.candidate) {
        this.log("ICE gathering complete (null candidate)");
        return;
      }
      const payload = event.candidate.toJSON();
      if (!payload.candidate) {
        return;
      }
      const candidate: IceCandidatePayload = {
        candidate: payload.candidate,
        sdpMid: payload.sdpMid,
        sdpMLineIndex: payload.sdpMLineIndex,
        usernameFragment: payload.usernameFragment,
      };
      this.log(`Local ICE candidate ${iceCandidateDiagnosticSummary(candidate)}`);
      if (!answerSent) {
        queuedLocalIce.push(candidate);
        this.log("Queued local ICE candidate until answer is sent");
        return;
      }
      sendLocalIce(candidate);
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

      if (channel.label === "stats" || channel.label === "stats_channel") {
        channel.binaryType = "arraybuffer";
        channel.onmessage = (msgEvent) => {
          if (msgEvent.data instanceof ArrayBuffer) {
            this.handleStatsChannelMessage(msgEvent.data);
          }
        };
        return;
      }

      if (channel.label !== "control_channel") {
        return;
      }

      this.controlChannel = channel;
      this.controlChannel.binaryType = "arraybuffer";
      this.controlChannel.onopen = () => {
        this.log("Control channel open");
        this.lastAdvertisedClipboardAvailable = null;
        void this.refreshClipboardAvailability();
      };
      this.controlChannel.onmessage = (msgEvent) => {
        void this.onControlChannelMessage(msgEvent.data as string | Blob | ArrayBuffer);
      };
      this.controlChannel.onclose = () => {
        this.log("Control channel closed");
        if (this.controlChannel === channel) {
          this.controlChannel = null;
          this.lastAdvertisedClipboardAvailable = null;
        }
      };
      this.controlChannel.onerror = () => {
        this.log("Control channel error");
      };
      if (channel.readyState === "open") {
        this.controlChannel.onopen?.call(channel, new Event("open"));
      }
    };

    pc.onicecandidateerror = (event: Event) => {
      const e = event as RTCPeerConnectionIceErrorEvent;
      const hostCandidate = "hostCandidate" in e
        ? (e as RTCPeerConnectionIceErrorEvent & { hostCandidate?: string }).hostCandidate
        : undefined;
      this.log(
        `ICE candidate error: ${e.errorCode} ${e.errorText} url=${signalingUrlForDiagnostics(e.url, session.sessionId)} hostCandidate=${hostCandidate ?? "?"}`,
      );
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
      this.peerMediaController.attachTrack(event.track);

      // Configure low-latency jitter buffer for video and audio receivers
      this.decoderPressureController.configureReceiver(event.receiver, event.track.kind);
    };

    // --- SDP Processing (matching Rust reference) ---

    // 1. Match the official client by pointing server ICE candidates at the
    //    WebRTC media endpoint from CloudMatch when one is present.
    const webRtcMediaConnection =
      session.mediaConnectionInfo?.usage === 2 || session.mediaConnectionInfo?.usage === 17
        ? session.mediaConnectionInfo
        : undefined;
    let processedOffer = offerSdp;
    if (webRtcMediaConnection?.ip) {
      const serverIpForSdp = webRtcMediaConnection.ip;
      processedOffer = fixServerIp(processedOffer, serverIpForSdp);
      this.log(`Fixed server IP in SDP offer: ${serverIpForSdp}`);
      // Log any remaining 0.0.0.0 references after fix
      const remaining = (processedOffer.match(/0\.0\.0\.0/g) ?? []).length;
      if (remaining > 0) {
        this.log(`Warning: ${remaining} occurrences of 0.0.0.0 still remain in SDP after fix`);
      }
      const rewritten = rewriteSdpIceCandidateEndpoints(processedOffer, webRtcMediaConnection);
      if (rewritten.replacements > 0) {
        processedOffer = rewritten.sdp;
        this.log(
          `Rewrote ${rewritten.replacements} server ICE candidate endpoint(s) to mediaConnectionInfo ${webRtcMediaConnection.ip}:${webRtcMediaConnection.port}`,
        );
      }
    } else if (session.mediaConnectionInfo) {
      this.log(
        `Skipping SDP ICE rewrite for mediaConnectionInfo usage=${session.mediaConnectionInfo.usage ?? "unknown"} (${session.mediaConnectionInfo.ip}:${session.mediaConnectionInfo.port})`,
      );
    }

    const preferredHevcProfileId = hevcPreferredProfileId(settings.colorQuality);
    let effectiveCodec = settings.codec;
    const fallbackVideoCodec = settings.fallbackCodec && settings.fallbackCodec !== "auto"
      ? settings.fallbackCodec
      : undefined;
    const supported = this.getSupportedVideoCodecs();
    this.log(`Browser supported video codecs: ${supported.join(", ") || "unknown"}`);
    const negotiationCandidates = resolveNegotiationCandidates(
      effectiveCodec,
      fallbackVideoCodec,
      supported,
    );
    if (negotiationCandidates.length === 0) {
      throw new Error("Browser receiver capabilities expose no GFN video codec");
    }
    this.log(`Negotiation codec candidates: ${negotiationCandidates.join(" -> ")}`);

    if (negotiationCandidates.includes("H265")) {
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
          `Warning: H265 profile-id=${preferredHevcProfileId} not reported in browser capabilities`,
        );
      }
    }

    this.applyStreamSettingsDiagnostics(settings, effectiveCodec, false);
    this.emitStats();
    const negotiation = await negotiatePeerConnectionCodecAnswer(
      pc,
      negotiationCandidates,
      (candidate) => preferCodec(processedOffer, candidate, {
          preferHevcProfileId: preferredHevcProfileId,
          keepFallbacks: true,
          fallbackCodec: fallbackVideoCodec,
      }),
      async (candidate, attemptIndex) => {
        if (attemptIndex > 0) {
          this.log(`Remote offer rolled back; retrying video negotiation with ${candidate}`);
        }
        this.log(`Remote description set for ${candidate}`);

        await attachMicrophoneForCodecAttempt(this.micManager, pc);

        this.applyCodecPreferences(
          pc,
          candidate,
          preferredHevcProfileId,
          true,
          fallbackVideoCodec,
        );
      },
    );

    const answer = negotiation.answer;
    effectiveCodec = negotiation.negotiatedCodec;
    if (effectiveCodec !== settings.codec) {
      this.log(`Codec fallback: requested ${settings.codec}, negotiated ${effectiveCodec}`);
    }
    this.currentCodec = effectiveCodec;
    this.diagnostics.codec = effectiveCodec;
    this.emitStats();

    if (answer.sdp) {
      answer.sdp = mungeAnswerSdp(answer.sdp, settings.maxBitrateKbps);
      this.log(`Answer SDP munged (b=AS:${settings.maxBitrateKbps}, stereo=1)`);
    }

    await pc.setLocalDescription(answer);
    this.log("Local description set; sending answer before ICE gathering completes");

    const finalSdp = pc.localDescription?.sdp ?? answer.sdp;
    if (!finalSdp) {
      throw new Error("Missing local SDP after setLocalDescription");
    }
    const finalNegotiatedCodec = extractNegotiatedVideoCodec(finalSdp);
    if (!finalNegotiatedCodec) {
      throw new Error("Local description rejected the video m-line after codec negotiation");
    }
    if (finalNegotiatedCodec !== effectiveCodec) {
      effectiveCodec = finalNegotiatedCodec;
      this.currentCodec = effectiveCodec;
      this.diagnostics.codec = effectiveCodec;
      this.emitStats();
    }
    this.log(sdpDiagnosticSummary("Created answer", finalSdp));
    await this.flushQueuedCandidates();

    const credentials = extractIceCredentials(finalSdp);
    this.log(
      `Extracted ICE credentials: ufragBytes=${credentials.ufrag.length}, pwdBytes=${credentials.pwd.length}`,
    );
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
      dynamicSplitEncodeUpdatesEnabled:
        settings.nativeTransitionDiagnostics?.disableDynamicSplitEncodeUpdates !== true,
    });

    await window.openNow.sendAnswer({
      sdp: finalSdp,
      nvstSdp,
    });
    this.log("Sent SDP answer and nvstSdp");
    answerSent = true;
    if (queuedLocalIce.length > 0) {
      this.log(`Flushing ${queuedLocalIce.length} queued local ICE candidates after answer`);
      for (const candidate of queuedLocalIce.splice(0)) {
        sendLocalIce(candidate);
      }
    }

    // Keep using server-provided trickled ICE; when CloudMatch gives a WebRTC
    // media endpoint, remote candidate IP/port rewriting happens in addRemoteCandidate.
    this.log("Waiting for server-provided ICE candidates");

    this.log("=== handleOffer COMPLETE — waiting for ICE connectivity and tracks ===");
  }

  async addRemoteCandidate(candidate: IceCandidatePayload): Promise<void> {
    const sdpMLineIndex = candidate.sdpMLineIndex ?? (candidate.sdpMid == null ? 0 : undefined);
    this.log(
      `Remote ICE candidate received ${iceCandidateDiagnosticSummary({ ...candidate, sdpMLineIndex })}`,
    );
    const init: RTCIceCandidateInit = {
      candidate: candidate.candidate,
      sdpMid: candidate.sdpMid ?? undefined,
      sdpMLineIndex,
      usernameFragment: candidate.usernameFragment ?? undefined,
    };

    if (!this.pc || !this.pc.remoteDescription) {
      this.queuedCandidates.push(init);
      return;
    }

    await this.pc.addIceCandidate(this.rewriteRemoteIceCandidateInit(init));
  }

  dispose(): void {
    this.cleanupPeerConnection();
    this.externalEscapeCleanup?.();
    this.externalEscapeCleanup = null;

    // Cleanup microphone
    if (this.micManager) {
      this.micManager.dispose();
      this.micManager = null;
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
    this.peerMediaController.setOutputVolume(volume);
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
