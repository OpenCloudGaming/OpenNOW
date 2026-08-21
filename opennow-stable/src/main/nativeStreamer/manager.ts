import electron from "electron";
import { randomUUID } from "node:crypto";
import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { basename } from "node:path";

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
  type NativeStreamerCommand,
  type NativeStreamerEvent,
  type NativeStreamerInputPacket,
  type NativeStreamerMessage,
  type NativeStreamerResponse,
} from "@shared/nativeStreamer";
import { isTerminalBrokenWriteError, setLogContext } from "@shared/logger";
import type { NativeStreamerShortcutBindings } from "@shared/gfn";
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

const HELLO_TIMEOUT_MS = 10000;
const BUNDLED_NATIVE_HELLO_TIMEOUT_MS = process.platform === "win32" ? 120000 : 30000;
const CONTROL_TIMEOUT_MS = 8000;
const SESSION_START_TIMEOUT_MS = process.platform === "win32" ? 90000 : 45000;
const SURFACE_UPDATE_TIMEOUT_MS = 15000;
const OFFER_TIMEOUT_MS = 20000;
const STOP_TIMEOUT_MS = 1200;
const MAX_INPUT_STDIN_BUFFER_BYTES = 64 * 1024;
const MIN_NATIVE_BITRATE_KBPS = 5_000;
const MAX_NATIVE_BITRATE_KBPS = 150_000;

function toError(error: unknown): Error {
  return error instanceof Error ? error : new Error(String(error));
}

function normalizeBitrateKbps(value: number): number {
  if (!Number.isFinite(value)) {
    return MIN_NATIVE_BITRATE_KBPS;
  }

  return Math.min(
    MAX_NATIVE_BITRATE_KBPS,
    Math.max(MIN_NATIVE_BITRATE_KBPS, Math.round(value)),
  );
}

export class NativeStreamerManager {
  private child: ChildProcessWithoutNullStreams | null = null;
  private startupPromise: Promise<void> | null = null;
  private stdoutBuffer = "";
  private stderrTail: string[] = [];
  private runtimeStatus: NativeStreamerRuntimeStatus | null = null;
  private pending = new Map<string, PendingRequest>();
  private capabilities: NativeStreamerCapabilities | null = null;
  private activeSessionId: string | null = null;
  private activeTransport: "webrtc" | "nvst" | null = null;
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
      release: async () => undefined,
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

    const response = await this.request({
      type: "start",
      context,
    }, SESSION_START_TIMEOUT_MS);
    if (response.type !== "ok") {
      throw new Error(`Native streamer returned ${response.type} instead of ok.`);
    }
    this.activeSessionId = context.session.sessionId;
    this.activeTransport = response.transport === "nvst" ? "nvst" : "webrtc";
    this.retainDiagnosticState({ sessionState: "ready" });
    await this.flushQueuedRemoteIce(context.session.sessionId);
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

    if (!this.capabilities?.supportsOfferAnswer) {
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
    if (this.capabilities && !this.capabilities.supportsRemoteIce) {
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
    if (
      !child
      || child.killed
      || !child.stdin.writable
      || child.stdin.destroyed
      || child.stdin.writableEnded
      || !this.activeSessionId
      || !this.capabilities?.supportsInput
    ) {
      return;
    }

    if (child.stdin.writableLength > MAX_INPUT_STDIN_BUFFER_BYTES) {
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

    let writeFailed = false;
    let flushed: boolean;
    try {
      flushed = child.stdin.write(`${JSON.stringify(payload)}\n`, "utf8", (error) => {
        if (!error) {
          return;
        }
        writeFailed = true;
        this.handleStdinFailure(child, error);
      });
    } catch (error) {
      const writeError = toError(error);
      if (!isTerminalBrokenWriteError(writeError, child.stdin)) {
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
      child.stdin.once("drain", () => {
        this.inputBackpressureWarned = false;
      });
    } else if (flushed) {
      this.inputBackpressureWarned = false;
    }
  }

  updateSurface(surface: NativeRenderSurface): void {
    this.surfaceUpdates.update(surface);
  }

  updateBitrateLimit(maxBitrateKbps: number): void {
    if (!this.child || !this.activeSessionId) {
      return;
    }

    void this.request({
      type: "bitrate",
      maxBitrateKbps: normalizeBitrateKbps(maxBitrateKbps),
    }, CONTROL_TIMEOUT_MS).catch((error) => {
      console.warn("[NativeStreamer] Failed to update native bitrate limit:", error);
    });
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

  updateShortcuts(shortcuts: NativeStreamerShortcutBindings): void {
    if (!this.child || !this.activeSessionId) {
      return;
    }

    void this.request({
      type: "update-shortcuts",
      shortcuts,
    }, CONTROL_TIMEOUT_MS).catch((error) => {
      console.warn("[NativeStreamer] Failed to update native shortcut bindings:", error);
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
      externalRendererEnabled: process.platform === "win32"
        ? this.options.getExternalRendererEnabled()
        : false,
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

    const child = spawn(executablePath, [], {
      stdio: "pipe",
      // The native presenter may own a top-level window on Windows.
      windowsHide: false,
      env: childEnv,
    });

    this.child = child;
    this.stdoutBuffer = "";
    this.stderrTail = [];
    this.inputBackpressureWarned = false;

    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk: string) => this.handleStdout(child, chunk));
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (chunk: string) => {
      if (this.child !== child) return;
      for (const line of chunk.split(/\r?\n/)) {
        if (line.trim()) {
          this.appendStderr(line);
          console.warn(`[NativeStreamer] ${line}`);
        }
      }
    });
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
    this.retainDiagnosticState({
      processState: "ready",
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
    if (
      !child
      || child.killed
      || !child.stdin.writable
      || child.stdin.destroyed
      || child.stdin.writableEnded
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
        child.stdin.write(`${JSON.stringify(payload)}\n`, "utf8", (error) => {
          if (error) {
            this.handleStdinFailure(child, error);
          }
        });
      } catch (error) {
        const writeError = toError(error);
        if (isTerminalBrokenWriteError(writeError, child.stdin)) {
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
      console.log(`[NativeStreamer] Input protocol ready: v${message.protocolVersion}`);
      this.options.emit({ type: "native-input-ready", protocolVersion: message.protocolVersion });
      return;
    }

    if (message.type === "shortcut") {
      this.options.emit({ type: "native-shortcut", action: message.action });
      return;
    }

    if (message.type === "clipboard-paste") {
      this.options.emit({ type: "native-clipboard-paste" });
      return;
    }

    if (message.type === "input-capture-changed") {
      this.options.emit({ type: "native-input-capture-changed", captured: message.captured });
      return;
    }

    if (message.type === "video-stall") {
      const formatAge = (value: number | undefined): string => value === undefined ? "n/a" : `${value}ms`;
      const stats = [
        `stall=${message.stallMs}ms`,
        `stage=${message.likelyStage ?? "unknown"}`,
        `encoded=${(message.encodedKbps ?? 0).toFixed(0)}kbps`,
        `decoded=${message.decodedFps.toFixed(1)}fps`,
        `sink=${message.sinkFps.toFixed(1)}fps`,
        `requestedFps=${message.requestedFps ?? "n/a"}`,
        `capsFramerate=${message.capsFramerate ?? "n/a"}`,
        `queueMode=${message.queueMode ?? "unknown"}`,
        `partialFlushes=${message.partialFlushCount ?? 0}`,
        `completeFlushes=${message.completeFlushCount ?? 0}`,
        `lastTransition=${message.lastTransitionType ?? "none"}`,
        `ages=encoded:${formatAge(message.encodedAgeMs)} decoded:${formatAge(message.decodedAgeMs)} sink:${formatAge(message.sinkAgeMs)}`,
        `rendered=${message.sinkRendered ?? "n/a"}`,
        `dropped=${message.sinkDropped ?? "n/a"}`,
        `memory=${message.memoryMode ?? "unknown"}`,
        `zeroCopy=${message.zeroCopy ?? "unknown"}`,
        `zeroCopyD3D11=${message.zeroCopyD3D11}`,
        `zeroCopyD3D12=${message.zeroCopyD3D12}`,
      ].join(" ");
      console.warn(`[NativeStreamer] Video stall recovery attempt ${message.recoveryAttempt}: ${stats}`);
      this.options.emit({
        type: "log",
        message: `[NativeStreamer] Video stall recovery attempt ${message.recoveryAttempt}: ${stats}`,
      });
      void this.options.requestKeyframe({
        reason: "native-video-stall",
        backlogFrames: 0,
        attempt: message.recoveryAttempt,
      }).catch((error) => {
        console.warn("[NativeStreamer] Failed to request video keyframe after stall:", error);
      });
      return;
    }

    if (message.type === "video-transition") {
      const transition = message.transition;
      const summary = transition.summary ?? `${transition.transitionType} @ ${transition.atMs}ms`;
      console.warn(`[NativeStreamer] Video transition: ${summary}`);
      this.options.emit({
        type: "native-stream-transition",
        transition,
      });
      this.options.emit({
        type: "log",
        message: `[NativeStreamer] Video transition: ${summary}`,
      });
      return;
    }

    if (message.type === "stats") {
      this.retainDiagnosticState({
        sessionState: "streaming",
        statsCapturedAt: new Date().toISOString(),
        codec: message.stats.codec,
        resolution: message.stats.resolution,
        hardwareAcceleration: message.stats.hardwareAcceleration,
        memoryMode: message.stats.memoryMode ?? "unknown",
        zeroCopy: message.stats.zeroCopy ?? false,
        bitrateKbps: message.stats.bitrateKbps,
        targetBitrateKbps: message.stats.targetBitrateKbps,
        decodedFps: message.stats.decodedFps,
        renderFps: message.stats.renderFps,
        framesDecoded: message.stats.framesDecoded,
        framesRendered: message.stats.framesRendered,
        sinkDropped: message.stats.sinkDropped ?? 0,
        queueMode: message.stats.queueMode ?? "unknown",
        partialFlushCount: message.stats.partialFlushCount ?? 0,
        completeFlushCount: message.stats.completeFlushCount ?? 0,
        lastTransition: message.stats.lastTransitionSummary ?? "none",
        serverGpuType: message.stats.serverGpuType ?? "unknown",
        serverLocation: message.stats.serverLocation ?? "unknown",
      });
      this.options.emit({
        type: "native-stream-stats",
        stats: message.stats,
      });
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

  private handleStdinFailure(
    child: ChildProcessWithoutNullStreams,
    error: Error,
  ): void {
    if (this.child !== child) {
      return;
    }

    if (!isTerminalBrokenWriteError(error, child.stdin)) {
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
    child.stdin.on("error", (error) => {
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
    this.stdoutBuffer = "";
    this.stderrTail = [];
    this.activeSessionId = null;
    this.activeTransport = null;
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

    this.child = null;
    try {
      child.kill();
    } catch (error) {
      console.warn("[NativeStreamer] Failed to terminate process:", error);
    }
  }
}
