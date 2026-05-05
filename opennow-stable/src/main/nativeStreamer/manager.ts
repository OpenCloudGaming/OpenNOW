import { app } from "electron";
import { randomUUID } from "node:crypto";
import { existsSync, statSync } from "node:fs";
import { dirname, resolve, join, delimiter } from "node:path";
import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";

import {
  nativeStreamerFeatureModeToEnvValue,
  type IceCandidatePayload,
  type KeyframeRequest,
  type MainToRendererSignalingEvent,
  type NativeStreamerBackendPreference,
  type NativeStreamerFeatureMode,
  type NativeStreamerStatus,
  type NativeGstreamerRuntimeStatus,
  type NativeGstreamerInstallInstruction,
  type NativeRenderSurface,
  type NativeStreamerSessionContext,
  type NativeVideoBackendCapability,
  type SendAnswerRequest,
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

type NativeStreamerCommandInput = NativeStreamerCommand extends infer T
  ? T extends NativeStreamerCommand
    ? Omit<T, "id">
    : never
  : never;

interface NativeStreamerCallbacks {
  sendAnswer(payload: SendAnswerRequest): Promise<void>;
  sendIceCandidate(candidate: IceCandidatePayload): Promise<void>;
  requestKeyframe(payload: KeyframeRequest): Promise<void>;
  emit(event: MainToRendererSignalingEvent): void;
}

interface NativeStreamerManagerOptions extends NativeStreamerCallbacks {
  mainDir: string;
  getBackendPreference(): NativeStreamerBackendPreference;
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
const CONTROL_TIMEOUT_MS = 8000;
const SESSION_START_TIMEOUT_MS = 45000;
const SURFACE_UPDATE_TIMEOUT_MS = 15000;
const OFFER_TIMEOUT_MS = 20000;
const STOP_TIMEOUT_MS = 1200;
const MAX_INPUT_STDIN_BUFFER_BYTES = 64 * 1024;
const MIN_NATIVE_BITRATE_KBPS = 5_000;
const MAX_NATIVE_BITRATE_KBPS = 150_000;

function nativeStreamerExecutableName(): string {
  return process.platform === "win32" ? "opennow-streamer.exe" : "opennow-streamer";
}

function nativeStreamerPlatformKey(): string {
  return `${process.platform}-${process.arch}`;
}

function isExistingFile(path: string): boolean {
  try {
    return existsSync(path) && statSync(path).isFile();
  } catch {
    return false;
  }
}

function isExistingDirectory(path: string): boolean {
  try {
    return existsSync(path) && statSync(path).isDirectory();
  } catch {
    return false;
  }
}

function prependEnvPath(env: NodeJS.ProcessEnv, key: string, directory: string): void {
  env[key] = env[key] ? `${directory}${delimiter}${env[key]}` : directory;
}

function prependProcessPath(env: NodeJS.ProcessEnv, directory: string): void {
  const pathKey = Object.keys(env).find((key) => key.toLowerCase() === "path") || "PATH";
  prependEnvPath(env, pathKey, directory);
}

const LINUX_GSTREAMER_INSTALL_INSTRUCTIONS: NativeGstreamerInstallInstruction[] = [
  {
    distro: "Debian / Ubuntu / Mint / Pop!_OS / KDE neon",
    command: "sudo apt update && sudo apt install libgstreamer1.0-0 libgstreamer-plugins-base1.0-0 gstreamer1.0-tools gstreamer1.0-libav gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly gstreamer1.0-gl gstreamer1.0-vaapi gstreamer1.0-x gstreamer1.0-alsa libva2 libva-drm2 libvulkan1 mesa-vulkan-drivers",
  },
  {
    distro: "Fedora / RHEL / Nobara / Bazzite",
    command: "sudo dnf install gstreamer1 gstreamer1-plugins-base gstreamer1-plugins-good gstreamer1-plugins-bad-free gstreamer1-plugins-bad-freeworld gstreamer1-plugins-ugly gstreamer1-libav gstreamer1-vaapi gstreamer1-plugin-openh264 mesa-vulkan-drivers libva",
    note: "RPM Fusion may be required for libav, ugly, or bad-freeworld packages.",
  },
  {
    distro: "Arch / Manjaro / EndeavourOS / SteamOS",
    command: "sudo pacman -S --needed gstreamer gst-plugins-base gst-plugins-good gst-plugins-bad gst-plugins-ugly gst-libav gst-plugin-va libva mesa vulkan-radeon",
    note: "NVIDIA users should use their distro NVIDIA/Vulkan driver packages instead of vulkan-radeon.",
  },
  {
    distro: "openSUSE Tumbleweed / Leap",
    command: "sudo zypper install gstreamer gstreamer-plugins-base gstreamer-plugins-good gstreamer-plugins-bad gstreamer-plugins-ugly gstreamer-plugins-libav gstreamer-plugins-vaapi libva2 Mesa-vulkan-device-select",
  },
];

function linuxInstallInstructions(): NativeGstreamerInstallInstruction[] | undefined {
  return process.platform === "linux" ? LINUX_GSTREAMER_INSTALL_INSTRUCTIONS : undefined;
}

function configureBundledGstreamerRuntime(
  env: NodeJS.ProcessEnv,
  executablePath: string,
): NativeGstreamerRuntimeStatus {
  const runtimeRoot = join(dirname(executablePath), "gstreamer");
  if (!isExistingDirectory(runtimeRoot)) {
    return {
      source: "system",
      bundled: false,
      message: process.platform === "linux"
        ? "No bundled GStreamer runtime was found. Linux uses distro GStreamer packages so VAAPI/V4L2/Vulkan plugins match the host driver stack."
        : "No bundled GStreamer runtime was found; using the system runtime if available.",
      installInstructions: linuxInstallInstructions(),
    };
  }

  const binDir = join(runtimeRoot, "bin");
  const libDir = join(runtimeRoot, "lib");
  const pluginDir = join(runtimeRoot, "lib", "gstreamer-1.0");
  const scanner = join(
    runtimeRoot,
    "libexec",
    "gstreamer-1.0",
    process.platform === "win32" ? "gst-plugin-scanner.exe" : "gst-plugin-scanner",
  );
  const gioModulesDir = join(runtimeRoot, "lib", "gio", "modules");

  if (process.platform === "win32") prependProcessPath(env, dirname(executablePath));
  if (isExistingDirectory(binDir)) prependProcessPath(env, binDir);
  if (isExistingDirectory(pluginDir)) {
    env.GST_PLUGIN_PATH = pluginDir;
    env.GST_PLUGIN_PATH_1_0 = pluginDir;
    env.GST_PLUGIN_SYSTEM_PATH = pluginDir;
    env.GST_PLUGIN_SYSTEM_PATH_1_0 = pluginDir;
  }
  if (isExistingFile(scanner)) {
    env.GST_PLUGIN_SCANNER = scanner;
    env.GST_PLUGIN_SCANNER_1_0 = scanner;
  }
  env.GST_REGISTRY_REUSE_PLUGIN_SCANNER = "no";
  if (isExistingDirectory(gioModulesDir)) {
    env.GIO_MODULE_DIR = gioModulesDir;
    env.GIO_EXTRA_MODULES = gioModulesDir;
  }
  if (process.platform === "linux") {
    if (isExistingDirectory(libDir)) prependEnvPath(env, "LD_LIBRARY_PATH", libDir);
    if (isExistingDirectory(binDir)) prependEnvPath(env, "LD_LIBRARY_PATH", binDir);
  }
  if (process.platform === "darwin") {
    if (isExistingDirectory(libDir)) {
      prependEnvPath(env, "DYLD_LIBRARY_PATH", libDir);
      prependEnvPath(env, "DYLD_FALLBACK_LIBRARY_PATH", libDir);
    }
    if (isExistingDirectory(binDir)) {
      prependEnvPath(env, "DYLD_LIBRARY_PATH", binDir);
      prependEnvPath(env, "DYLD_FALLBACK_LIBRARY_PATH", binDir);
    }
  }

  return {
    source: "bundled",
    bundled: true,
    path: runtimeRoot,
    message: "Using bundled GStreamer runtime next to the native streamer executable.",
  };
}

function isWindowsDllLoadFailure(error: unknown): boolean {
  const message = formatError(error);
  return process.platform === "win32" && (message.includes("3221225781") || message.toLowerCase().includes("0xc0000135"));
}

function formatNativeStreamerDetectionFailure(error: unknown, runtime: NativeGstreamerRuntimeStatus | null): string {
  if (isWindowsDllLoadFailure(error)) {
    return runtime?.bundled
      ? `Native streamer could not load a required DLL even though bundled GStreamer was detected at ${runtime.path}. The packaged runtime may be incomplete or blocked. ${formatError(error)}`
      : `Native streamer could not load a required DLL and no bundled GStreamer runtime was detected. ${formatError(error)}`;
  }
  return `Native streamer was not detected: ${formatError(error)}`;
}

function formatError(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
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

function formatVideoBackendName(backend: string | undefined): string {
  switch (backend) {
    case "d3d12":
      return "D3D12";
    case "d3d11":
      return "D3D11";
    case "videotoolbox":
      return "VideoToolbox";
    case "vaapi":
      return "VAAPI";
    case "v4l2":
      return "V4L2";
    case "vulkan":
      return "Vulkan";
    case "software":
      return "Software";
    default:
      return backend ?? "Unknown";
  }
}

function formatVideoCodec(codec: string): string {
  switch (codec.toLowerCase()) {
    case "h264":
      return "H.264";
    case "h265":
      return "H.265";
    case "av1":
      return "AV1";
    default:
      return codec.toUpperCase();
  }
}

function resolveActiveVideoBackend(
  videoBackends: NativeVideoBackendCapability[],
): NativeVideoBackendCapability | undefined {
  const currentPlatform = process.platform === "win32"
    ? "windows"
    : process.platform === "darwin"
      ? "macos"
      : process.platform === "linux"
        ? "linux"
        : "other";

  return videoBackends.find((candidate) => candidate.available && candidate.platform === currentPlatform)
    ?? videoBackends.find((candidate) => candidate.available && candidate.platform === "cross-platform")
    ?? videoBackends.find((candidate) => candidate.available);
}

function summarizeCodecs(backend: NativeVideoBackendCapability | undefined): string {
  const codecs = backend?.codecs
    .filter((codec) => codec.available)
    .map((codec) => formatVideoCodec(codec.codec)) ?? [];
  return codecs.length > 0 ? codecs.join(", ") : "No hardware codec path";
}

function summarizeZeroCopy(backend: NativeVideoBackendCapability | undefined): string {
  if (!backend) {
    return "Not available";
  }
  return backend.zeroCopyModes.length > 0
    ? `Hardware memory: ${backend.zeroCopyModes.join(", ")}`
    : "System memory";
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function isResponse(message: NativeStreamerMessage): message is NativeStreamerResponse {
  return isRecord(message) && typeof (message as Record<string, unknown>)["id"] === "string";
}

function isEvent(message: NativeStreamerMessage): message is NativeStreamerEvent {
  return isRecord(message) && typeof (message as Record<string, unknown>)["id"] !== "string";
}

export class NativeStreamerManager {
  private child: ChildProcessWithoutNullStreams | null = null;
  private stdoutBuffer = "";
  private stderrTail: string[] = [];
  private gstreamerRuntime: NativeGstreamerRuntimeStatus | null = null;
  private pending = new Map<string, PendingRequest>();
  private capabilities: NativeStreamerCapabilities | null = null;
  private activeSessionId: string | null = null;
  private inputBackpressureWarned = false;
  private answerInFlight = false;
  private queuedLocalIce: IceCandidatePayload[] = [];
  private queuedRemoteIceSessionId: string | null = null;
  private queuedRemoteIce: IceCandidatePayload[] = [];
  private lastSurface: NativeRenderSurface | null = null;
  private surfaceUpdateInFlight = false;
  private surfaceUpdateQueued = false;

  constructor(private readonly options: NativeStreamerManagerOptions) {}

  isRunning(): boolean {
    return this.child !== null;
  }

  async handleOffer(sdp: string, context: NativeStreamerSessionContext): Promise<void> {
    if (this.activeSessionId && this.activeSessionId !== context.session.sessionId) {
      await this.stop("new native streamer session");
    }
    this.prepareRemoteIceQueue(context.session.sessionId);

    const negotiatedProfile = context.session.negotiatedStreamProfile;
    console.log(
      "[NativeStreamer] Session context:",
      JSON.stringify({
        sessionId: context.session.sessionId,
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

    await this.ensureProcess();

    if (!this.capabilities?.supportsOfferAnswer) {
      console.warn(
        `[NativeStreamer] Backend "${this.capabilities?.backend ?? "unknown"}" reports offer/answer is not ready; forwarding offer for validation/fallback.`,
      );
    }

    if (this.activeSessionId !== context.session.sessionId) {
      if (context.settings.enableCloudGsync) {
        console.log(
          "[NativeStreamer] Cloud G-Sync/VRR mode resolved for this session; preserving unthrottled low-latency present behavior.",
        );
      }
      await this.request({
        type: "start",
        context,
      }, SESSION_START_TIMEOUT_MS);
      this.activeSessionId = context.session.sessionId;
      await this.flushQueuedRemoteIce(context.session.sessionId);
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
    try {
      await this.ensureProcess();
      const backend = this.capabilities?.backend;
      const gstreamerAvailable = backend === "gstreamer" && this.capabilities?.supportsOfferAnswer === true;
      const videoBackends = this.capabilities?.videoBackends ?? [];
      const activeVideoBackend = resolveActiveVideoBackend(videoBackends);
      const codecSummary = summarizeCodecs(activeVideoBackend);
      const zeroCopySummary = summarizeZeroCopy(activeVideoBackend);
      const runtime = this.gstreamerRuntime ?? {
        source: "unknown",
        bundled: false,
        message: "GStreamer runtime has not been checked yet.",
        installInstructions: linuxInstallInstructions(),
      } satisfies NativeGstreamerRuntimeStatus;
      const effectiveRuntime: NativeGstreamerRuntimeStatus = gstreamerAvailable
        ? runtime.bundled
          ? runtime
          : {
            ...runtime,
            source: "system",
            message: "Using system GStreamer runtime; packaged Windows/macOS builds should use the bundled runtime.",
          }
        : {
          ...runtime,
          source: runtime.bundled ? "bundled" : process.platform === "linux" ? "missing" : runtime.source,
          message: runtime.bundled
            ? "Bundled GStreamer runtime was found, but the GStreamer backend is not ready."
            : process.platform === "linux"
              ? "GStreamer is not ready. Install distro GStreamer packages so plugins match the host GPU/driver stack."
              : runtime.message,
          installInstructions: runtime.installInstructions ?? linuxInstallInstructions(),
        };
      return {
        detected: true,
        gstreamerAvailable,
        supportsOfferAnswer: this.capabilities?.supportsOfferAnswer === true,
        backend,
        fallbackReason: this.capabilities?.fallbackReason,
        videoBackends,
        activeVideoBackend,
        codecSummary,
        zeroCopySummary,
        gstreamerRuntime: effectiveRuntime,
        message: gstreamerAvailable
          ? `${effectiveRuntime.message} Video path: ${formatVideoBackendName(activeVideoBackend?.backend)}.`
          : this.capabilities?.fallbackReason ?? effectiveRuntime.message,
      };
    } catch (error) {
      const runtime = this.gstreamerRuntime ?? {
        source: process.platform === "linux" ? "missing" : "unknown",
        bundled: false,
        message: process.platform === "linux"
          ? "GStreamer is not ready. Linux uses distro packages because private AppImage GStreamer bundling is unreliable across glibc, libdrm/VAAPI/Vulkan, and GPU driver stacks."
          : "GStreamer runtime could not be checked because the native streamer did not start.",
        installInstructions: linuxInstallInstructions(),
      } satisfies NativeGstreamerRuntimeStatus;
      return {
        detected: false,
        gstreamerAvailable: false,
        supportsOfferAnswer: false,
        gstreamerRuntime: runtime,
        message: formatNativeStreamerDetectionFailure(error, runtime),
      };
    }
  }

  async addRemoteIce(candidate: IceCandidatePayload, context: NativeStreamerSessionContext): Promise<void> {
    const sessionId = context.session.sessionId;
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

    const flushed = child.stdin.write(`${JSON.stringify(payload)}\n`, "utf8", (error) => {
      if (error && !this.inputBackpressureWarned) {
        this.inputBackpressureWarned = true;
        console.warn("[NativeStreamer] Failed to write native input:", error);
      }
    });

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
    this.lastSurface = surface;
    void this.flushSurfaceUpdate();
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

  async stop(reason = "stopped"): Promise<void> {
    const child = this.child;
    this.activeSessionId = null;
    this.capabilities = null;
    this.clearQueuedRemoteIce();

    if (!child) {
      return;
    }

    try {
      await this.request({ type: "stop", reason }, STOP_TIMEOUT_MS);
    } catch (error) {
      console.warn("[NativeStreamer] Stop request failed:", error);
    } finally {
      this.terminateProcess();
    }
  }

  dispose(reason = "disposed"): void {
    this.activeSessionId = null;
    this.capabilities = null;
    this.clearQueuedRemoteIce();
    this.rejectPending(new Error(`Native streamer ${reason}.`));
    this.terminateProcess();
  }

  private async ensureProcess(): Promise<void> {
    if (this.child) {
      return;
    }

    const executablePath = this.resolveExecutablePath();
    const backendPreference = this.options.getBackendPreference();
    console.log("[NativeStreamer] Starting:", executablePath);
    console.log("[NativeStreamer] Backend preference:", backendPreference);

    const childEnv: NodeJS.ProcessEnv = {
      ...process.env,
      OPENNOW_NATIVE_STREAMER_PROTOCOL: String(NATIVE_STREAMER_PROTOCOL_VERSION),
    };
    if (process.platform === "win32") {
      childEnv.OPENNOW_NATIVE_EXTERNAL_RENDERER = this.options.getExternalRendererEnabled() ? "1" : "0";
    }
    childEnv.OPENNOW_NATIVE_CLOUD_GSYNC = nativeStreamerFeatureModeToEnvValue(this.options.getCloudGsyncMode());
    childEnv.OPENNOW_NATIVE_D3D_FULLSCREEN = nativeStreamerFeatureModeToEnvValue(this.options.getD3dFullscreenMode());
    if (backendPreference !== "auto") {
      childEnv.OPENNOW_NATIVE_STREAMER_BACKEND = backendPreference;
    }
    const runtimeStatus = configureBundledGstreamerRuntime(childEnv, executablePath);
    this.gstreamerRuntime = runtimeStatus;
    if (runtimeStatus.bundled) {
      console.log("[NativeStreamer] Using bundled GStreamer runtime:", runtimeStatus.path);
    } else {
      console.log("[NativeStreamer]", runtimeStatus.message);
    }

    const child = spawn(executablePath, [], {
      stdio: "pipe",
      // The default native path lets the GStreamer video sink create its own
      // render window. Hiding the child process also hides that sink window on
      // Windows, which leaves the Electron input placeholder black.
      windowsHide: false,
      env: childEnv,
    });

    this.child = child;
    this.stdoutBuffer = "";
    this.stderrTail = [];

    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk: string) => this.handleStdout(chunk));
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (chunk: string) => {
      for (const line of chunk.split(/\r?\n/)) {
        if (line.trim()) {
          this.appendStderr(line);
          console.warn(`[NativeStreamer] ${line}`);
        }
      }
    });

    child.once("error", (error) => {
      this.options.emit({ type: "error", message: `Native streamer failed to start: ${formatError(error)}` });
      this.handleProcessExit(`spawn error: ${formatError(error)}`);
    });

    child.once("exit", (code, signal) => {
      const reason = signal ? `signal ${signal}` : `exit code ${code ?? "unknown"}`;
      this.handleProcessExit(reason);
    });

    const response = await this.request({
      type: "hello",
      protocolVersion: NATIVE_STREAMER_PROTOCOL_VERSION,
    }, HELLO_TIMEOUT_MS);

    if (response.type !== "ready") {
      throw new Error(`Native streamer returned ${response.type} instead of ready.`);
    }

    this.capabilities = response.capabilities;
    console.log("[NativeStreamer] Capabilities:", response.capabilities);
    if (response.capabilities.protocolVersion !== NATIVE_STREAMER_PROTOCOL_VERSION) {
      throw new Error(
        `Native streamer reported protocolVersion=${response.capabilities.protocolVersion}, expected ${NATIVE_STREAMER_PROTOCOL_VERSION}.`,
      );
    }
    this.assertBackendPreference(response.capabilities, backendPreference);
    await this.flushSurfaceUpdate();
  }

  private assertBackendPreference(
    capabilities: NativeStreamerCapabilities,
    backendPreference: NativeStreamerBackendPreference,
  ): void {
    if (backendPreference === "auto" || capabilities.backend === backendPreference) {
      return;
    }

    const reason = capabilities.fallbackReason ? ` ${capabilities.fallbackReason}` : "";
    throw new Error(
      `Native streamer backend "${backendPreference}" is unavailable; process selected "${capabilities.backend}".${reason}`,
    );
  }

  private resolveExecutablePath(): string {
    const exeName = nativeStreamerExecutableName();
    const platformKey = nativeStreamerPlatformKey();
    const configuredPath = this.options.getExecutablePathOverride().trim();
    if (configuredPath) {
      if (isExistingFile(configuredPath)) {
        return configuredPath;
      }
      throw new Error(`Configured native streamer executable was not found: ${configuredPath}`);
    }

    const candidates = [
      process.env.OPENNOW_NATIVE_STREAMER,
      join(process.resourcesPath, "native", "opennow-streamer", platformKey, exeName),
      join(process.resourcesPath, "native", "opennow-streamer", exeName),
      resolve(this.options.mainDir, "../../../native/opennow-streamer/bin", platformKey, exeName),
      resolve(this.options.mainDir, "../../../native/opennow-streamer/bin", exeName),
      resolve(this.options.mainDir, "../../../native/opennow-streamer/dist", platformKey, exeName),
      resolve(this.options.mainDir, "../../../native/opennow-streamer/dist", exeName),
      resolve(this.options.mainDir, "../../../native/opennow-streamer/target/release", platformKey, exeName),
      resolve(this.options.mainDir, "../../../native/opennow-streamer/target/release", exeName),
      resolve(this.options.mainDir, "../../../native/opennow-streamer/target/debug", platformKey, exeName),
      resolve(this.options.mainDir, "../../../native/opennow-streamer/target/debug", exeName),
      resolve(app.getAppPath(), "../native/opennow-streamer/bin", platformKey, exeName),
      resolve(app.getAppPath(), "../native/opennow-streamer/bin", exeName),
      resolve(app.getAppPath(), "../native/opennow-streamer/dist", platformKey, exeName),
      resolve(app.getAppPath(), "../native/opennow-streamer/dist", exeName),
      resolve(app.getAppPath(), "../native/opennow-streamer/target/release", platformKey, exeName),
      resolve(app.getAppPath(), "../native/opennow-streamer/target/release", exeName),
      resolve(app.getAppPath(), "../native/opennow-streamer/target/debug", platformKey, exeName),
      resolve(app.getAppPath(), "../native/opennow-streamer/target/debug", exeName),
    ].filter((candidate): candidate is string => Boolean(candidate));

    const found = candidates.find((candidate) => isExistingFile(candidate));
    if (found) {
      return found;
    }

    throw new Error(`Native streamer binary not found. Checked: ${candidates.join(", ")}`);
  }

  private request(input: NativeStreamerCommandInput, timeoutMs: number): Promise<NativeStreamerResponse> {
    const child = this.child;
    if (!child || child.killed || !child.stdin.writable) {
      return Promise.reject(new Error("Native streamer process is not running."));
    }

    const id = randomUUID();
    const payload = { ...input, id } as NativeStreamerCommand;

    return new Promise<NativeStreamerResponse>((resolveRequest, rejectRequest) => {
      const timeout = setTimeout(() => {
        this.pending.delete(id);
        rejectRequest(new Error(`Native streamer request "${input.type}" timed out.${this.formatStderrTail()}`));
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

      child.stdin.write(`${JSON.stringify(payload)}\n`, "utf8", (error) => {
        if (!error) {
          return;
        }
        const pending = this.pending.get(id);
        if (pending) {
          this.pending.delete(id);
          pending.reject(error);
        }
      });
    });
  }

  private async flushSurfaceUpdate(): Promise<void> {
    if (this.surfaceUpdateInFlight) {
      this.surfaceUpdateQueued = true;
      return;
    }

    while (this.child && this.lastSurface) {
      this.surfaceUpdateInFlight = true;
      this.surfaceUpdateQueued = false;
      const surface = this.lastSurface;

      try {
        await this.request({ type: "surface", surface }, SURFACE_UPDATE_TIMEOUT_MS);
      } catch (error) {
        console.warn("[NativeStreamer] Failed to update native render surface:", error);
        break;
      } finally {
        this.surfaceUpdateInFlight = false;
      }

      if (!this.surfaceUpdateQueued || this.lastSurface === surface) {
        break;
      }
    }
  }

  private handleStdout(chunk: string): void {
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

    if (isResponse(message)) {
      this.handleResponse(message);
      return;
    }

    if (isEvent(message)) {
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
      this.options.emit({
        type: "native-stream-stats",
        stats: message.stats,
      });
      return;
    }

    if (message.type === "status") {
      console.log(`[NativeStreamer] Status: ${message.status}${message.message ? ` (${message.message})` : ""}`);
      if (message.status === "streaming") {
        this.options.emit({ type: "native-stream-started", message: message.message });
      } else if (message.status === "stopped") {
        this.options.emit({ type: "native-stream-stopped", reason: message.message });
      }
      return;
    }

    if (message.type === "error") {
      this.options.emit({ type: "error", message: `Native streamer error: ${message.message}` });
    }
  }

  private handleProcessExit(reason: string): void {
    if (!this.child) {
      return;
    }

    const tail = this.formatStderrTail();
    const hadActiveSession = this.activeSessionId !== null;
    const stoppedReason = `process ended (${reason})`;
    console.warn(`[NativeStreamer] Process ended (${reason})${tail}`);
    this.child = null;
    this.stdoutBuffer = "";
    this.stderrTail = [];
    this.activeSessionId = null;
    this.capabilities = null;
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
    for (const [id, pending] of this.pending.entries()) {
      clearTimeout(pending.timeout);
      pending.reject(error);
      this.pending.delete(id);
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
