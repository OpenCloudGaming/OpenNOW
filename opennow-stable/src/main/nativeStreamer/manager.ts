import electron from "electron";
import { randomUUID } from "node:crypto";
import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { basename } from "node:path";
import type { Readable, Writable } from "node:stream";

import {
  createUnsupportedNativeStreamerStatus,
  extractNegotiatedVideoCodec,
  isNativeStreamerSupportedPlatform,
  NATIVE_STREAMER_WINDOWS_ONLY_MESSAGE,
  type IceCandidatePayload,
  type KeyframeRequest,
  type MainToRendererSignalingEvent,
  type NativeStreamerFeatureMode,
  type NativeVideoBackendPreference,
  type NativeStreamerStatus,
  type NativeStreamerRuntimeStatus,
  type NativeRenderSurface,
  type NativeStreamerSessionContext,
  type SendAnswerRequest,
  streamDiagnosticId,
} from "@shared/gfn";
import {
  NATIVE_STREAMER_PROTOCOL_VERSION,
  type NativeStreamerCapabilities,
  type NativeStreamerActiveTransportCapabilities,
  type NativeStreamerCommand,
  type NativeStreamerEvent,
  type NativeStreamerInputPacket,
  type NativeStreamerMessage,
  type NativeStreamerResponse,
} from "@shared/nativeStreamer";
import { isTerminalBrokenWriteError, setLogContext } from "@shared/logger";
import {
  createNativeStreamerDetectionFailureStatus,
  createNativeStreamerStatus,
  formatError,
} from "./capabilities";
import { resolveNativeStreamerExecutableCandidates } from "./executableDiscovery";
import {
  isNativeStreamerEvent,
  isNativeStreamerResponse,
  type NativeStreamerCommandInput,
} from "./protocol";
import { createNativeStreamerRuntimeEnvironment } from "./runtime";
import { launchMacOSStreamerApp } from "./macosLaunchServices";
import { NativeSurfaceUpdateQueue } from "./surfaceUpdateQueue";

const { app } = electron;

interface NativeStreamerCallbacks {
  sendAnswer(payload: SendAnswerRequest): Promise<void>;
  sendIceCandidate(candidate: IceCandidatePayload): Promise<void>;
  requestKeyframe(payload: KeyframeRequest): Promise<void>;
  emit(event: MainToRendererSignalingEvent): void;
  retryWithSoftwareDecoder(message: string): void;
}

interface NativeStreamerManagerOptions extends NativeStreamerCallbacks {
  mainDir: string;
  getVideoBackendPreference(): NativeVideoBackendPreference;
  getExecutablePathOverride(): string;
  getCloudGsyncMode(): NativeStreamerFeatureMode;
  getD3dFullscreenMode(): NativeStreamerFeatureMode;
  getExternalRendererEnabled(): boolean;
}

interface PendingRequest {
  resolve(message: NativeStreamerResponse): void;
  reject(error: Error): void;
  timeout: NodeJS.Timeout;
}

interface NvstReservationLease {
  ownership: "negotiator" | "native";
  released: boolean;
}

interface PendingNvstReadiness {
  sessionId: string;
  resolve(): void;
  reject(error: Error): void;
  timeout: NodeJS.Timeout;
}

interface NativeStreamerProcessIo {
  child: ChildProcessWithoutNullStreams;
  stdin: Writable;
  stdout: Readable;
  stderr: Readable;
  launchMode: "direct" | "macos-launch-services";
  cleanup(): void;
}

const HELLO_TIMEOUT_MS = 10000;
const BUNDLED_NATIVE_HELLO_TIMEOUT_MS = process.platform === "win32" ? 120000 : 30000;
const CONTROL_TIMEOUT_MS = 8000;
const SESSION_START_TIMEOUT_MS = process.platform === "win32" ? 90000 : 45000;
const SURFACE_UPDATE_TIMEOUT_MS = 15000;
const OFFER_TIMEOUT_MS = 20000;
const STOP_TIMEOUT_MS = 1200;
const NVST_TRANSPORT_READY_TIMEOUT_MS = 7000;
const INPUT_RESPONSE_TIMEOUT_MS = 2000;
const MAX_INPUT_STDIN_BUFFER_BYTES = 64 * 1024;

function toError(error: unknown): Error {
  return error instanceof Error ? error : new Error(String(error));
}

export class NativeStreamerManager {
  private child: ChildProcessWithoutNullStreams | null = null;
  private processIo: NativeStreamerProcessIo | null = null;
  private nativeProcessPid: number | null = null;
  private startupPromise: Promise<void> | null = null;
  private stdoutBuffer = "";
  private stderrBuffer = "";
  private stderrTail: string[] = [];
  private runtimeStatus: NativeStreamerRuntimeStatus | null = null;
  private pending = new Map<string, PendingRequest>();
  private capabilities: NativeStreamerCapabilities | null = null;
  private activeSessionId: string | null = null;
  private activeTransport: "webrtc" | "nvst" | null = null;
  private activeTransportCapabilities: NativeStreamerActiveTransportCapabilities | null = null;
  private nvstReservation: NvstReservationLease | null = null;
  private pendingNvstSessionId: string | null = null;
  private nvstReadinessWaiters = new Set<PendingNvstReadiness>();
  private nvstTransportReady = false;
  private nvstReadinessError: Error | null = null;
  private inputReady = false;
  private nativeInputOwner: "electron" | "native" = "electron";
  private inputBackpressureWarned = false;
  private answerInFlight = false;
  private queuedLocalIce: IceCandidatePayload[] = [];
  private queuedRemoteIceSessionId: string | null = null;
  private queuedRemoteIce: IceCandidatePayload[] = [];
  private readonly surfaceUpdates: NativeSurfaceUpdateQueue;
  private videoBackendOverride: NativeVideoBackendPreference | null = null;
  private suppressNextStoppedEvent = false;
  private diagnosticState: Record<string, unknown> = {
    processState: "not-started",
    sessionState: "idle",
  };

  constructor(private readonly options: NativeStreamerManagerOptions) {
    this.surfaceUpdates = new NativeSurfaceUpdateQueue(
      (surface) => this.request({ type: "surface", surface }, SURFACE_UPDATE_TIMEOUT_MS).then(() => undefined),
      (error) => console.warn("[NativeStreamer] Failed to update native render surface:", error),
    );
  }

  isRunning(): boolean {
    return this.child !== null;
  }

  hasActiveSession(): boolean {
    return this.activeSessionId !== null;
  }

  isNvstSessionActive(sessionId: string): boolean {
    return this.activeSessionId === sessionId && this.activeTransport === "nvst";
  }

  private retainDiagnosticState(values: Record<string, unknown>): void {
    this.diagnosticState = {
      ...this.diagnosticState,
      ...values,
    };
    setLogContext("nativeStreamer.latest", this.diagnosticState);
  }

  setVideoBackendOverride(value: NativeVideoBackendPreference | null): void {
    this.videoBackendOverride = value;
  }

  async reserveNvstUdp(): Promise<{
    port: number;
    mjolnirPort?: number;
    localAddress?: string;
    iceUsernameFragment?: string;
    icePassword?: string;
    dtlsFingerprint?: string;
    send(payload: Buffer, host: string, port: number): Promise<void>;
    release(): Promise<void>;
  }> {
    await this.ensureProcess();
    const response = await this.request({ type: "nvst-bind" }, CONTROL_TIMEOUT_MS);
    if (response.type !== "nvst-bound" || !Number.isInteger(response.port) || response.port <= 0) {
      throw new Error("Native streamer did not reserve an NVST UDP socket.");
    }
    const port = response.port;
    const mjolnirPort = Number.isInteger(response.mjolnirPort) && (response.mjolnirPort ?? 0) > 0
      ? response.mjolnirPort
      : undefined;
    const localAddress = typeof response.localAddress === "string" && response.localAddress.length > 0
      ? response.localAddress
      : undefined;
    const lease: NvstReservationLease = {
      ownership: "negotiator",
      released: false,
    };
    this.nvstReservation = lease;
    console.log(
      `[NativeStreamer] Reserved NVST video UDP on port ${port} before RTSP ANNOUNCE`
      + `${mjolnirPort ? ` mjolnirPort=${mjolnirPort}` : ""}`
      + `${localAddress ? ` localAddress=${localAddress}` : ""}`
      + `${response.dtlsFingerprint ? ` (dtlsFingerprintBytes=${response.dtlsFingerprint.length})` : ""}`,
    );
    return {
      port,
      mjolnirPort,
      localAddress,
      iceUsernameFragment: response.iceUsernameFragment,
      icePassword: response.icePassword,
      dtlsFingerprint: response.dtlsFingerprint,
      send: async (payload, host, peerPort) => {
        const sent = await this.request({
          type: "nvst-send",
          host,
          port: peerPort,
          payloadBase64: payload.toString("base64"),
        }, CONTROL_TIMEOUT_MS);
        if (sent.type !== "ok") {
          throw new Error(`Native streamer returned ${sent.type} instead of ok for nvst-send.`);
        }
      },
      release: async () => {
        if (lease.released) {
          return;
        }
        lease.released = true;
        if (lease.ownership === "native" || this.nvstReservation !== lease) {
          return;
        }
        this.nvstReservation = null;
        const released = await this.request({ type: "nvst-unbind" }, CONTROL_TIMEOUT_MS);
        if (released.type !== "ok") {
          throw new Error(`Native streamer returned ${released.type} instead of ok for nvst-unbind.`);
        }
      },
    };
  }

  async prepareForSession(context: NativeStreamerSessionContext): Promise<void> {
    if (this.activeSessionId && this.activeSessionId !== context.session.sessionId) {
      await this.stop("new native streamer session");
    }
    this.prepareRemoteIceQueue(context.session.sessionId);
    this.retainDiagnosticState({
      streamKey: streamDiagnosticId(context.session.sessionId),
      sessionState: "preparing",
      requestedResolution: context.settings.resolution,
      requestedFps: context.settings.fps,
      requestedCodec: context.settings.codec,
      transportMode: context.settings.transportMode ?? "webrtc",
    });

    await this.ensureProcess();

    if (this.activeSessionId === context.session.sessionId) {
      return;
    }

    if (context.settings.enableCloudGsync) {
      console.log(
        "[NativeStreamer] Cloud G-Sync/VRR mode resolved for this session; preserving unthrottled low-latency present behavior.",
      );
    }

    const expectsNvst = context.settings.transportMode === "nvst" || context.nvstVideo !== undefined;
    this.inputReady = false;
    this.nvstTransportReady = false;
    this.nvstReadinessError = null;
    this.pendingNvstSessionId = expectsNvst ? context.session.sessionId : null;
    let response: NativeStreamerResponse;
    try {
      response = await this.request({
        type: "start",
        context,
      }, SESSION_START_TIMEOUT_MS);
    } catch (error) {
      this.pendingNvstSessionId = null;
      throw error;
    }
    if (response.type !== "ok") {
      this.pendingNvstSessionId = null;
      throw new Error(`Native streamer returned ${response.type} instead of ok.`);
    }
    this.activeSessionId = context.session.sessionId;
    this.activeTransport = response.transport === "nvst" ? "nvst" : "webrtc";
    this.activeTransportCapabilities = response.capabilities ?? {
      supportsOfferAnswer: this.activeTransport === "webrtc"
        && this.capabilities?.supportsOfferAnswer === true,
      supportsRemoteIce: this.activeTransport === "webrtc"
        && this.capabilities?.supportsRemoteIce === true,
      supportsLocalIce: this.activeTransport === "webrtc"
        && this.capabilities?.supportsLocalIce === true,
      supportsInput: this.capabilities?.supportsInput === true,
      supportsAudioDecode: this.capabilities?.supportsAudioDecode === true,
      supportsAudioOutput: this.capabilities?.supportsAudioOutput === true,
    };
    if (this.activeTransport === "nvst" && this.nvstReservation?.ownership === "negotiator") {
      this.nvstReservation.ownership = "native";
    }
    if (this.activeTransport !== "nvst") {
      this.inputReady = false;
      this.nvstTransportReady = false;
      this.nvstReadinessError = null;
    }
    this.pendingNvstSessionId = null;
    this.retainDiagnosticState({ sessionState: "ready" });
    await this.flushQueuedRemoteIce(context.session.sessionId);
  }

  waitForNvstTransportReady(
    sessionId: string,
    timeoutMs = NVST_TRANSPORT_READY_TIMEOUT_MS,
  ): Promise<void> {
    if (this.activeSessionId !== sessionId || this.activeTransport !== "nvst") {
      return Promise.reject(new Error(`Native NVST session ${sessionId} is not active.`));
    }
    if (!this.activeTransportCapabilities?.supportsInput) {
      return Promise.reject(new Error("Native NVST transport does not support the DTLS/SCTP readiness handshake."));
    }
    if (this.nvstReadinessError) {
      return Promise.reject(this.nvstReadinessError);
    }
    if (this.nvstTransportReady) {
      return Promise.resolve();
    }

    return new Promise<void>((resolve, reject) => {
      const waiter: PendingNvstReadiness = {
        sessionId,
        resolve: () => {
          clearTimeout(waiter.timeout);
          this.nvstReadinessWaiters.delete(waiter);
          resolve();
        },
        reject: (error) => {
          clearTimeout(waiter.timeout);
          this.nvstReadinessWaiters.delete(waiter);
          reject(error);
        },
        timeout: setTimeout(() => {
          waiter.reject(new Error(
            `Native NVST DTLS/SCTP readiness timed out after ${timeoutMs}ms.`,
          ));
        }, timeoutMs),
      };
      this.nvstReadinessWaiters.add(waiter);
    });
  }

  async handleOffer(sdp: string, context: NativeStreamerSessionContext): Promise<void> {
    const negotiatedProfile = context.session.negotiatedStreamProfile;
    console.log(
      "[NativeStreamer] Session context:",
      JSON.stringify({
        streamKey: streamDiagnosticId(context.session.sessionId),
        requestedResolution: context.settings.resolution,
        requestedFps: context.settings.fps,
        requestedCodec: context.settings.codec,
        negotiatedResolution: negotiatedProfile?.resolution,
        negotiatedFps: negotiatedProfile?.fps,
        negotiatedCodec: negotiatedProfile?.codec ?? context.settings.codec,
        requestedStreamingFeatures: context.session.requestedStreamingFeatures,
        finalizedStreamingFeatures: context.session.finalizedStreamingFeatures,
      }),
    );

    await this.prepareForSession(context);
    this.retainDiagnosticState({
      sessionState: "negotiating-offer",
      negotiatedResolution: negotiatedProfile?.resolution ?? "unknown",
      negotiatedFps: negotiatedProfile?.fps ?? "unknown",
      negotiatedCodec: negotiatedProfile?.codec ?? context.settings.codec,
    });

    if (!(this.activeTransportCapabilities?.supportsOfferAnswer ?? this.capabilities?.supportsOfferAnswer)) {
      throw new Error(
        `Native streamer backend "${this.capabilities?.backend ?? "unknown"}" does not support offer/answer.`,
      );
    }

    this.answerInFlight = true;
    this.queuedLocalIce = [];

    try {
      const response = await this.request({
        type: "offer",
        sdp,
        context,
      }, OFFER_TIMEOUT_MS);

      if (response.type !== "answer") {
        throw new Error(`Native streamer returned ${response.type} instead of answer.`);
      }

      const negotiatedCodec = extractNegotiatedVideoCodec(response.answer.sdp);
      if (!negotiatedCodec) {
        throw new Error("Native streamer answer rejected the video m-line.");
      }
      console.log(`[NativeStreamer] Answer negotiated video codec: ${negotiatedCodec}`);
      this.retainDiagnosticState({
        sessionState: "answer-sent",
        negotiatedCodec,
      });
      await this.options.sendAnswer(response.answer);
      this.answerInFlight = false;
      await this.flushQueuedLocalIce();
    } catch (error) {
      this.answerInFlight = false;
      this.queuedLocalIce = [];
      throw error;
    }

    this.options.emit({
      type: "log",
      message: "Native streamer accepted the WebRTC offer; waiting for decoded media.",
    });
  }

  async probeStatus(): Promise<NativeStreamerStatus> {
    if (!isNativeStreamerSupportedPlatform(process.platform)) {
      return createUnsupportedNativeStreamerStatus();
    }

    try {
      await this.ensureProcess();
      return createNativeStreamerStatus(
        this.capabilities,
        this.runtimeStatus,
        this.options.getVideoBackendPreference(),
        process.platform,
      );
    } catch (error) {
      return createNativeStreamerDetectionFailureStatus(
        error,
        this.runtimeStatus,
        process.platform,
      );
    }
  }

  async addRemoteIce(candidate: IceCandidatePayload, context: NativeStreamerSessionContext): Promise<void> {
    const sessionId = context.session.sessionId;
    if (
      this.activeTransportCapabilities
        ? !this.activeTransportCapabilities.supportsRemoteIce
        : this.capabilities && !this.capabilities.supportsRemoteIce
    ) {
      return;
    }
    if (!this.child || this.activeSessionId !== sessionId) {
      this.queueRemoteIce(sessionId, candidate);
      return;
    }

    await this.sendRemoteIce(candidate);
  }

  drainQueuedRemoteIce(sessionId: string): IceCandidatePayload[] {
    if (this.queuedRemoteIceSessionId !== sessionId) {
      return [];
    }

    const queued = this.queuedRemoteIce;
    this.queuedRemoteIceSessionId = null;
    this.queuedRemoteIce = [];
    return queued;
  }

  sendInput(input: NativeStreamerInputPacket): void {
    const child = this.child;
    const stdin = child ? this.processStdin(child) : null;
    if (
      !child
      || !stdin
      || child.killed
      || !stdin.writable
      || stdin.destroyed
      || stdin.writableEnded
      || !this.activeSessionId
      || !this.capabilities?.supportsInput
      || !this.activeTransportCapabilities?.supportsInput
      || !this.inputReady
    ) {
      return;
    }

    if (stdin.writableLength > MAX_INPUT_STDIN_BUFFER_BYTES) {
      if (!this.inputBackpressureWarned) {
        this.inputBackpressureWarned = true;
        console.warn("[NativeStreamer] Dropping native input while streamer stdin is backpressured.");
      }
      return;
    }

    const payload = {
      id: randomUUID(),
      type: "input",
      input,
    } satisfies NativeStreamerCommand;

    const reportInputFailure = (error: Error): void => {
      if (!this.inputReady) {
        return;
      }
      const reason = `Native input write failed: ${formatError(error)}`;
      this.inputReady = false;
      this.retainDiagnosticState({ inputReady: false, inputUnavailableReason: reason });
      this.options.emit({ type: "native-input-unavailable", reason });
    };
    const timeout = setTimeout(() => {
      if (!this.pending.delete(payload.id)) {
        return;
      }
      reportInputFailure(new Error(`Native input acknowledgement timed out after ${INPUT_RESPONSE_TIMEOUT_MS}ms.`));
    }, INPUT_RESPONSE_TIMEOUT_MS);
    timeout.unref?.();
    this.pending.set(payload.id, {
      resolve: () => {
        clearTimeout(timeout);
      },
      reject: (error) => {
        clearTimeout(timeout);
        reportInputFailure(error);
      },
      timeout,
    });

    let writeFailed = false;
    let flushed: boolean;
    try {
      flushed = stdin.write(`${JSON.stringify(payload)}\n`, "utf8", (error) => {
        if (!error) {
          return;
        }
        writeFailed = true;
        this.handleStdinFailure(child, error);
      });
    } catch (error) {
      const writeError = toError(error);
      if (!isTerminalBrokenWriteError(writeError, stdin)) {
        this.rejectPendingRequest(payload.id, writeError);
        throw error;
      }
      this.handleStdinFailure(child, writeError);
      return;
    }

    if (writeFailed) {
      return;
    }

    if (!flushed && !this.inputBackpressureWarned) {
      this.inputBackpressureWarned = true;
      console.warn("[NativeStreamer] Native input writer reported backpressure; input will be dropped until it drains.");
      stdin.once("drain", () => {
        this.inputBackpressureWarned = false;
      });
    } else if (flushed) {
      this.inputBackpressureWarned = false;
    }
  }

  updateSurface(surface: NativeRenderSurface): void {
    this.surfaceUpdates.update(surface);
  }

  setInputPaused(paused: boolean): void {
    if (!this.child || !this.activeSessionId) {
      return;
    }

    void this.request({
      type: "input-paused",
      paused,
    }, CONTROL_TIMEOUT_MS).catch((error) => {
      console.warn("[NativeStreamer] Failed to update native input pause state:", error);
    });
  }

  async stop(reason = "stopped"): Promise<void> {
    if (reason === "retrying native video with software decoding") {
      this.suppressNextStoppedEvent = true;
    }
    const child = this.child;
    const streamKey = streamDiagnosticId(this.activeSessionId);
    this.retainDiagnosticState({
      streamKey,
      sessionState: "stopping",
      stopReason: reason,
    });
    this.activeSessionId = null;
    this.activeTransport = null;
    this.activeTransportCapabilities = null;
    this.pendingNvstSessionId = null;
    this.nvstReservation = null;
    this.inputReady = false;
    this.nvstTransportReady = false;
    this.nvstReadinessError = null;
    this.rejectNvstReadiness(new Error(`Native streamer stopped before NVST transport readiness (${reason}).`));
    this.capabilities = null;
    this.surfaceUpdates.markNotReady();
    this.clearQueuedRemoteIce();

    if (!child) {
      this.retainDiagnosticState({
        processState: "stopped",
        sessionState: "stopped",
        stopReason: reason,
      });
      return;
    }

    try {
      await this.request({ type: "stop", reason }, STOP_TIMEOUT_MS);
    } catch (error) {
      console.warn("[NativeStreamer] Stop request failed:", error);
    } finally {
      this.terminateProcess();
      this.suppressNextStoppedEvent = false;
      this.retainDiagnosticState({
        processState: "stopped",
        sessionState: "stopped",
        stopReason: reason,
      });
    }
  }

  dispose(reason = "disposed"): void {
    this.retainDiagnosticState({
      streamKey: streamDiagnosticId(this.activeSessionId),
      processState: "disposed",
      sessionState: "disposed",
      stopReason: reason,
    });
    this.activeSessionId = null;
    this.activeTransport = null;
    this.activeTransportCapabilities = null;
    this.pendingNvstSessionId = null;
    this.nvstReservation = null;
    this.inputReady = false;
    this.nvstTransportReady = false;
    this.nvstReadinessError = null;
    this.rejectNvstReadiness(new Error(`Native streamer ${reason} before NVST transport readiness.`));
    this.capabilities = null;
    this.surfaceUpdates.markNotReady();
    this.clearQueuedRemoteIce();
    this.rejectPending(new Error(`Native streamer ${reason}.`));
    this.terminateProcess();
  }

  private async ensureProcess(): Promise<void> {
    if (!isNativeStreamerSupportedPlatform(process.platform)) {
      throw new Error(NATIVE_STREAMER_WINDOWS_ONLY_MESSAGE);
    }

    if (this.child && this.capabilities) {
      return;
    }

    if (this.startupPromise) {
      await this.startupPromise;
      return;
    }

    if (this.child && !this.capabilities) {
      console.warn("[NativeStreamer] Restarting native streamer after an incomplete startup handshake.");
      this.rejectPending(new Error("Native streamer startup handshake did not complete."));
      this.terminateProcess();
      this.stdoutBuffer = "";
      this.stderrBuffer = "";
      this.stderrTail = [];
    }

    const startupPromise = (async () => {
      let lastError: Error | null = null;

      for (const executablePath of resolveNativeStreamerExecutableCandidates({
        platform: process.platform,
        arch: process.arch,
        resourcesPath: process.resourcesPath,
        appPath: app.getAppPath(),
        mainDir: this.options.mainDir,
        envExecutablePath: process.env.OPENNOW_NATIVE_STREAMER,
        getConfiguredPath: () => this.options.getExecutablePathOverride(),
      })) {
        try {
          await this.startProcess(executablePath);
          return;
        } catch (error) {
          lastError = error instanceof Error ? error : new Error(String(error));
          this.retainDiagnosticState({
            processState: "initialization-failed",
            sessionState: "failed",
            lastError: formatError(lastError),
          });
          console.warn(
            `[NativeStreamer] Failed to initialize ${executablePath}: ${formatError(lastError)}`,
          );
          this.rejectPending(lastError);
          this.terminateProcess();
          this.stdoutBuffer = "";
          this.stderrBuffer = "";
          this.stderrTail = [];
          this.capabilities = null;
        }
      }

      throw lastError ?? new Error("Native streamer could not be initialized from any candidate path.");
    })();

    this.startupPromise = startupPromise;
    try {
      await startupPromise;
    } finally {
      if (this.startupPromise === startupPromise) {
        this.startupPromise = null;
      }
    }
  }

  private async startProcess(executablePath: string): Promise<void> {
    this.retainDiagnosticState({
      processState: "starting",
      executable: basename(executablePath),
    });
    console.log("[NativeStreamer] Starting:", executablePath);
    const videoBackendPreference = this.videoBackendOverride
      ?? this.options.getVideoBackendPreference();
    this.retainDiagnosticState({ videoBackendPreference });
    console.log("[NativeStreamer] Video backend preference:", videoBackendPreference);

    const { env: childEnv, runtimeStatus } = createNativeStreamerRuntimeEnvironment({
      executablePath,
      baseEnv: process.env,
      platform: process.platform,
      arch: process.arch,
      userDataPath: app.getPath("userData"),
      protocolVersion: NATIVE_STREAMER_PROTOCOL_VERSION,
      videoBackendPreference,
      externalRendererEnabled: process.platform === "darwin"
        || (process.platform === "win32" && this.options.getExternalRendererEnabled()),
      linuxOzonePlatform: app.commandLine.getSwitchValue("ozone-platform")
        || app.commandLine.getSwitchValue("ozone-platform-hint"),
      cloudGsyncMode: this.options.getCloudGsyncMode(),
      d3dFullscreenMode: this.options.getD3dFullscreenMode(),
    });
    this.runtimeStatus = runtimeStatus;
    this.retainDiagnosticState({
      runtimeSelfContained: runtimeStatus.selfContained,
      runtimeState: runtimeStatus.message,
    });
    console.log("[NativeStreamer]", runtimeStatus.message, runtimeStatus.path);

    const launched = process.platform === "darwin"
      ? launchMacOSStreamerApp(executablePath, childEnv)
      : null;
    const child = launched?.child ?? spawn(executablePath, [], {
      stdio: "pipe",
      // The native presenter may own a top-level window on Windows.
      windowsHide: false,
      env: childEnv,
    });
    const processIo: NativeStreamerProcessIo = launched
      ? {
          child,
          stdin: launched.stdin,
          stdout: launched.stdout,
          stderr: launched.stderr,
          launchMode: "macos-launch-services",
          cleanup: launched.cleanup,
        }
      : {
          child,
          stdin: child.stdin,
          stdout: child.stdout,
          stderr: child.stderr,
          launchMode: "direct",
          cleanup: () => undefined,
        };

    this.child = child;
    this.processIo = processIo;
    this.nativeProcessPid = null;
    this.nativeInputOwner = childEnv.OPENNOW_NATIVE_INPUT_OWNER === "native"
      ? "native"
      : "electron";
    this.stdoutBuffer = "";
    this.stderrBuffer = "";
    this.stderrTail = [];
    this.inputBackpressureWarned = false;

    processIo.stdout.setEncoding("utf8");
    processIo.stdout.on("data", (chunk: string) => this.handleStdout(child, chunk));
    processIo.stderr.setEncoding("utf8");
    processIo.stderr.on("data", (chunk: string) => this.handleStderr(child, chunk));
    this.installStdinErrorHandler(child);

    child.once("error", (error) => {
      this.options.emit({ type: "error", message: `Native streamer failed to start: ${formatError(error)}` });
      this.handleProcessExit(child, `spawn error: ${formatError(error)}`);
    });

    child.once("exit", (code, signal) => {
      const reason = signal ? `signal ${signal}` : `exit code ${code ?? "unknown"}`;
      this.handleProcessExit(child, reason);
    });

    const helloTimeoutMs = runtimeStatus.selfContained ? BUNDLED_NATIVE_HELLO_TIMEOUT_MS : HELLO_TIMEOUT_MS;
    const response = await this.request({
      type: "hello",
      protocolVersion: NATIVE_STREAMER_PROTOCOL_VERSION,
    }, helloTimeoutMs);

    if (response.type !== "ready") {
      throw new Error(`Native streamer returned ${response.type} instead of ready.`);
    }

    this.capabilities = response.capabilities;
    this.nativeProcessPid = response.processId ?? child.pid ?? null;
    if (processIo.launchMode === "macos-launch-services") {
      console.log(
        `[NativeStreamer] Independent macOS app ready: nativePid=${this.nativeProcessPid ?? "unknown"}`
        + ` launchServicesMonitorPid=${child.pid ?? "unknown"}`,
      );
    }
    this.retainDiagnosticState({
      processState: "ready",
      launchMode: processIo.launchMode,
      nativeProcessPid: this.nativeProcessPid ?? "unknown",
      backend: response.capabilities.backend,
      protocolVersion: response.capabilities.protocolVersion,
      supportsOfferAnswer: response.capabilities.supportsOfferAnswer,
      supportsInput: response.capabilities.supportsInput,
    });
    console.log("[NativeStreamer] Capabilities:", response.capabilities);
    if (response.capabilities.protocolVersion !== NATIVE_STREAMER_PROTOCOL_VERSION) {
      throw new Error(
        `Native streamer reported protocolVersion=${response.capabilities.protocolVersion}, expected ${NATIVE_STREAMER_PROTOCOL_VERSION}.`,
      );
    }
    await this.surfaceUpdates.markReady();
  }

  private request(input: NativeStreamerCommandInput, timeoutMs: number): Promise<NativeStreamerResponse> {
    const child = this.child;
    const stdin = child ? this.processStdin(child) : null;
    if (
      !child
      || !stdin
      || child.killed
      || !stdin.writable
      || stdin.destroyed
      || stdin.writableEnded
    ) {
      return Promise.reject(new Error("Native streamer process is not running."));
    }

    const id = randomUUID();
    const payload = { ...input, id } as NativeStreamerCommand;

    return new Promise<NativeStreamerResponse>((resolveRequest, rejectRequest) => {
      const timeout = setTimeout(() => {
        this.pending.delete(id);
        rejectRequest(new Error(`Native streamer request "${input.type}" timed out after ${timeoutMs}ms.${this.formatStderrTail()}`));
      }, timeoutMs);
      timeout.unref?.();

      this.pending.set(id, {
        resolve: (message) => {
          clearTimeout(timeout);
          resolveRequest(message);
        },
        reject: (error) => {
          clearTimeout(timeout);
          rejectRequest(error);
        },
        timeout,
      });

      try {
        stdin.write(`${JSON.stringify(payload)}\n`, "utf8", (error) => {
          if (error) {
            this.handleStdinFailure(child, error);
          }
        });
      } catch (error) {
        const writeError = toError(error);
        if (isTerminalBrokenWriteError(writeError, stdin)) {
          this.handleStdinFailure(child, writeError);
          return;
        }
        this.rejectPendingRequest(id, writeError);
      }
    });
  }

  private handleStdout(child: ChildProcessWithoutNullStreams, chunk: string): void {
    if (this.child !== child) return;
    this.stdoutBuffer += chunk;
    const lines = this.stdoutBuffer.split(/\r?\n/);
    this.stdoutBuffer = lines.pop() ?? "";

    for (const line of lines) {
      const trimmed = line.trim();
      if (!trimmed) {
        continue;
      }
      this.handleLine(trimmed);
    }
  }

  private handleStderr(child: ChildProcessWithoutNullStreams, chunk: string): void {
    if (this.child !== child) return;
    this.stderrBuffer += chunk;
    const lines = this.stderrBuffer.split(/\r?\n/);
    this.stderrBuffer = lines.pop() ?? "";
    for (const line of lines) {
      if (!line.trim()) continue;
      this.appendStderr(line);
      console.warn(`[NativeStreamer] ${line}`);
    }
  }

  private handleLine(line: string): void {
    let message: NativeStreamerMessage;
    try {
      message = JSON.parse(line) as NativeStreamerMessage;
    } catch {
      console.log(`[NativeStreamer] ${line}`);
      return;
    }

    if (isNativeStreamerResponse(message)) {
      this.handleResponse(message);
      return;
    }

    if (isNativeStreamerEvent(message)) {
      this.handleEvent(message);
    }
  }

  private handleResponse(message: NativeStreamerResponse): void {
    const pending = this.pending.get(message.id);
    if (!pending) {
      console.warn("[NativeStreamer] Ignoring response for unknown request:", message.id);
      return;
    }

    this.pending.delete(message.id);
    if (message.type === "error") {
      pending.reject(new Error(message.code ? `${message.code}: ${message.message}` : message.message));
      return;
    }

    pending.resolve(message);
  }

  private handleEvent(message: NativeStreamerEvent): void {
    if (message.type === "log") {
      const text = `[NativeStreamer] ${message.message}`;
      if (message.level === "error") {
        console.error(text);
      } else if (message.level === "warn") {
        console.warn(text);
      } else {
        console.log(text);
      }
      this.options.emit({ type: "log", message: text });
      return;
    }

    if (message.type === "local-ice") {
      if (!this.capabilities?.supportsLocalIce) {
        console.warn("[NativeStreamer] Ignoring local ICE from a backend that did not advertise it.");
        return;
      }
      if (this.answerInFlight) {
        this.queuedLocalIce.push(message.candidate);
        return;
      }

      this.forwardLocalIce(message.candidate);
      return;
    }

    if (message.type === "input-ready") {
      const readySessionId = this.activeSessionId ?? this.pendingNvstSessionId;
      if (
        !readySessionId
        || (this.activeTransportCapabilities && !this.activeTransportCapabilities.supportsInput)
      ) {
        console.warn("[NativeStreamer] Ignoring input readiness from an active transport that does not advertise input.");
        return;
      }
      this.inputReady = true;
      console.log(`[NativeStreamer] Input protocol ready: v${message.protocolVersion}`);
      if (this.activeTransport === "nvst" || this.pendingNvstSessionId === readySessionId) {
        this.nvstTransportReady = true;
        this.nvstReadinessError = null;
        this.resolveNvstReadiness(readySessionId);
      }
      this.options.emit({
        type: "native-input-ready",
        protocolVersion: message.protocolVersion,
        inputOwner: this.nativeInputOwner,
      });
      return;
    }

    if (message.type === "nvst-transport-ready") {
      const readySessionId = this.activeSessionId ?? this.pendingNvstSessionId;
      if (
        !readySessionId
        || (this.activeTransport !== null && this.activeTransport !== "nvst")
      ) {
        console.warn("[NativeStreamer] Ignoring NVST readiness without an active NVST session.");
        return;
      }
      console.log(`[NativeStreamer] NVST transport readiness: ${message.phase}`);
      if (message.phase === "sctp") {
        this.nvstTransportReady = true;
        this.resolveNvstReadiness(readySessionId);
      }
      return;
    }

    if (message.type === "input-unavailable") {
      this.inputReady = false;
      if (this.activeTransport === "nvst" || this.pendingNvstSessionId !== null) {
        this.nvstReadinessError = new Error(
          `Native NVST DTLS/SCTP readiness failed: ${message.reason}`,
        );
        this.rejectNvstReadiness(this.nvstReadinessError);
      }
      console.warn(`[NativeStreamer] Input unavailable: ${message.reason}`);
      this.options.emit({ type: "native-input-unavailable", reason: message.reason });
      return;
    }

    if (message.type === "status") {
      console.log(`[NativeStreamer] Status: ${message.status}${message.message ? ` (${message.message})` : ""}`);
      this.retainDiagnosticState({
        sessionState: message.status,
        statusMessage: message.message ?? "",
      });
      if (message.status === "streaming") {
        this.options.emit({ type: "native-stream-started", message: message.message });
      } else if (message.status === "stopped") {
        this.clearActiveSessionOwnership();
        if (this.suppressNextStoppedEvent) {
          this.suppressNextStoppedEvent = false;
          return;
        }
        this.options.emit({ type: "native-stream-stopped", reason: message.message });
      }
      return;
    }

    if (message.type === "error") {
      if (
        process.platform === "linux"
        && message.code === "native-video-decoder-startup-timeout"
      ) {
        console.warn(
          `[NativeStreamer] ${message.message} Restarting the native session with software decoding.`,
        );
        this.options.emit({
          type: "log",
          message: `[NativeStreamer] ${message.message} Retrying the native session with software decoding.`,
        });
        this.options.retryWithSoftwareDecoder(message.message);
        return;
      }
      this.options.emit({ type: "error", message: `Native streamer error: ${message.message}` });
    }
  }

  private clearActiveSessionOwnership(): void {
    this.rejectNvstReadiness(new Error("Native NVST session stopped before transport readiness."));
    this.activeSessionId = null;
    this.activeTransport = null;
    this.activeTransportCapabilities = null;
    this.pendingNvstSessionId = null;
    this.nvstReservation = null;
    this.inputReady = false;
    this.nativeInputOwner = "electron";
    this.nvstTransportReady = false;
    this.nvstReadinessError = null;
    this.surfaceUpdates.markNotReady();
    this.clearQueuedRemoteIce();
  }

  private handleStdinFailure(
    child: ChildProcessWithoutNullStreams,
    error: Error,
  ): void {
    if (this.child !== child) {
      return;
    }

    const stdin = this.processStdin(child);
    if (!isTerminalBrokenWriteError(error, stdin ?? child.stdin)) {
      console.error("[NativeStreamer] Streamer stdin failed:", error);
    }

    this.rejectPending(error);
    this.handleProcessExit(child, `stdin error: ${formatError(error)}`);
    if (!child.killed) {
      try {
        child.kill();
      } catch (killError) {
        console.warn("[NativeStreamer] Failed to terminate process after stdin failure:", killError);
      }
    }
  }

  private installStdinErrorHandler(child: ChildProcessWithoutNullStreams): void {
    const stdin = this.processStdin(child) ?? child.stdin;
    stdin.on("error", (error) => {
      this.handleStdinFailure(child, error);
    });
  }

  private handleProcessExit(
    child: ChildProcessWithoutNullStreams,
    reason: string,
  ): void {
    if (this.child !== child) {
      return;
    }

    if (this.stderrBuffer.trim()) {
      this.appendStderr(this.stderrBuffer);
    }
    const tail = this.formatStderrTail();
    const hadActiveSession = this.activeSessionId !== null;
    const stoppedReason = `process ended (${reason})`;
    console.warn(`[NativeStreamer] Process ended (${reason})${tail}`);
    this.retainDiagnosticState({
      streamKey: streamDiagnosticId(this.activeSessionId),
      processState: "ended",
      sessionState: hadActiveSession ? "interrupted" : "idle",
      processEndReason: reason,
      recentStderr: tail || "none",
    });
    this.child = null;
    this.releaseProcessIo(child);
    this.nativeProcessPid = null;
    this.stdoutBuffer = "";
    this.stderrBuffer = "";
    this.stderrTail = [];
    this.activeSessionId = null;
    this.activeTransport = null;
    this.activeTransportCapabilities = null;
    this.pendingNvstSessionId = null;
    this.nvstReservation = null;
    this.inputReady = false;
    this.nvstTransportReady = false;
    this.nvstReadinessError = null;
    this.rejectNvstReadiness(new Error(`Native streamer process ended before NVST transport readiness (${reason}).`));
    this.capabilities = null;
    this.inputBackpressureWarned = false;
    this.surfaceUpdates.markNotReady();
    this.clearQueuedRemoteIce();
    this.rejectPending(new Error(`Native streamer process ended (${reason}).${tail}`));

    if (hadActiveSession) {
      this.options.emit({ type: "native-stream-stopped", reason: stoppedReason });
      this.options.emit({ type: "error", message: `Native streamer ${stoppedReason}.${tail}` });
    }
  }

  private appendStderr(line: string): void {
    this.stderrTail.push(line);
    if (this.stderrTail.length > 12) this.stderrTail.shift();
  }

  private formatStderrTail(): string {
    return this.stderrTail.length > 0 ? ` Recent stderr: ${this.stderrTail.join(" | ")}` : "";
  }

  private rejectPending(error: Error): void {
    const pendingRequests = [...this.pending.values()];
    this.pending.clear();
    for (const pending of pendingRequests) {
      clearTimeout(pending.timeout);
      pending.reject(error);
    }
  }

  private rejectPendingRequest(id: string, error: Error): void {
    const pending = this.pending.get(id);
    if (!pending) {
      return;
    }
    this.pending.delete(id);
    pending.reject(error);
  }

  private resolveNvstReadiness(sessionId: string): void {
    for (const waiter of this.nvstReadinessWaiters) {
      if (waiter.sessionId === sessionId) {
        waiter.resolve();
      }
    }
  }

  private rejectNvstReadiness(error: Error): void {
    for (const waiter of this.nvstReadinessWaiters) {
      waiter.reject(error);
    }
  }

  private async flushQueuedLocalIce(): Promise<void> {
    const queued = this.queuedLocalIce;
    this.queuedLocalIce = [];

    for (const candidate of queued) {
      await this.forwardLocalIce(candidate);
    }
  }

  private prepareRemoteIceQueue(sessionId: string): void {
    if (this.queuedRemoteIceSessionId !== null && this.queuedRemoteIceSessionId !== sessionId) {
      this.clearQueuedRemoteIce();
    }
    this.queuedRemoteIceSessionId = sessionId;
  }

  private queueRemoteIce(sessionId: string, candidate: IceCandidatePayload): void {
    this.prepareRemoteIceQueue(sessionId);
    this.queuedRemoteIce.push(candidate);
  }

  private clearQueuedRemoteIce(): void {
    this.queuedRemoteIceSessionId = null;
    this.queuedRemoteIce = [];
  }

  private async flushQueuedRemoteIce(sessionId: string): Promise<void> {
    const queued = this.drainQueuedRemoteIce(sessionId);
    if (!this.capabilities?.supportsRemoteIce) {
      return;
    }
    for (const candidate of queued) {
      await this.sendRemoteIce(candidate);
    }
  }

  private async sendRemoteIce(candidate: IceCandidatePayload): Promise<void> {
    await this.request({
      type: "remote-ice",
      candidate,
    }, CONTROL_TIMEOUT_MS);
  }

  private async forwardLocalIce(candidate: IceCandidatePayload): Promise<void> {
    try {
      await this.options.sendIceCandidate(candidate);
    } catch (error) {
      console.warn("[NativeStreamer] Failed to forward local ICE candidate:", error);
    }
  }

  private terminateProcess(): void {
    this.surfaceUpdates.markNotReady();
    const child = this.child;
    if (!child) {
      return;
    }

    const processIo = this.takeProcessIo(child);
    const nativeProcessPid = this.nativeProcessPid;
    this.child = null;
    this.nativeProcessPid = null;

    if (processIo?.launchMode === "macos-launch-services") {
      const shutdown = {
        id: randomUUID(),
        type: "shutdown",
        reason: "Electron host stopped native streamer",
      } satisfies NativeStreamerCommand;
      try {
        processIo.stdin.end(`${JSON.stringify(shutdown)}\n`, "utf8");
      } catch (error) {
        if (!isTerminalBrokenWriteError(error, processIo.stdin)) {
          console.warn("[NativeStreamer] Failed to request native app shutdown:", error);
        }
        processIo.stdin.destroy();
      }

      let finalized = false;
      const finalize = (): void => {
        if (finalized) return;
        finalized = true;
        processIo.cleanup();
      };
      child.once("exit", finalize);
      const forceTimer = setTimeout(() => {
        if (nativeProcessPid) {
          try {
            process.kill(nativeProcessPid, "SIGTERM");
          } catch (error) {
            const code = error && typeof error === "object" && "code" in error
              ? error.code
              : undefined;
            if (code !== "ESRCH") {
              console.warn("[NativeStreamer] Failed to terminate native macOS app:", error);
            }
          }
        }
        if (!child.killed) {
          try {
            child.kill();
          } catch (error) {
            console.warn("[NativeStreamer] Failed to terminate LaunchServices monitor:", error);
          }
        }
        finalize();
      }, STOP_TIMEOUT_MS);
      forceTimer.unref?.();
      child.once("exit", () => clearTimeout(forceTimer));
      return;
    }

    processIo?.cleanup();
    try {
      child.kill();
    } catch (error) {
      console.warn("[NativeStreamer] Failed to terminate process:", error);
    }
  }

  private processStdin(child: ChildProcessWithoutNullStreams): Writable | null {
    return this.processIo?.child === child ? this.processIo.stdin : child.stdin;
  }

  private takeProcessIo(child: ChildProcessWithoutNullStreams): NativeStreamerProcessIo | null {
    if (this.processIo?.child !== child) return null;
    const processIo = this.processIo;
    this.processIo = null;
    return processIo;
  }

  private releaseProcessIo(child: ChildProcessWithoutNullStreams): void {
    this.takeProcessIo(child)?.cleanup();
  }
}
