import { app, type BrowserWindow } from "electron";
import { createServer, type Server, type Socket } from "node:net";
import { createInterface } from "node:readline";
import { spawn, type ChildProcessByStdio } from "node:child_process";
import type { Readable } from "node:stream";
import { access, chmod, mkdir, rm } from "node:fs/promises";
import { constants } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { randomUUID } from "node:crypto";
import { fileURLToPath } from "node:url";

import type {
  IceCandidatePayload,
  KeyframeRequest,
  MainToRendererSignalingEvent,
  NativeStreamerEvent,
  NativeStreamerInputEnvelope,
  NativeStreamerStartRequest,
  SendAnswerRequest,
} from "@shared/gfn";
import { IPC_CHANNELS } from "@shared/ipc";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const REPO_ROOT = resolve(__dirname, "../../../../..");
const NATIVE_PROJECT_ROOT = join(REPO_ROOT, "opennow-native-streamer");
const IPC_VERSION = 1;
const STARTUP_TIMEOUT_MS = 15000;

type Envelope = {
  type: string;
  version?: number;
  payload?: unknown;
};

type NativeProcessDescriptor = { command: string; args: string[]; cwd?: string };

type NativeStatsPayload = Extract<NativeStreamerEvent, { type: "stats" }>['stats'];
const SOCKET_RECOVERABLE_ERRORS = new Set(["ECONNRESET", "EPIPE"]);

export interface NativeStreamerManagerOptions {
  onEvent: (event: NativeStreamerEvent) => void;
  onSendAnswer: (payload: SendAnswerRequest) => Promise<void>;
  onSendIceCandidate: (payload: IceCandidatePayload) => Promise<void>;
  onRequestKeyframe?: (payload: KeyframeRequest) => Promise<void>;
}

export class NativeStreamerManager {
  private readonly options: NativeStreamerManagerOptions;
  private server: Server | null = null;
  private socket: Socket | null = null;
  private socketPath: string | null = null;
  private child: ChildProcessByStdio<null, Readable, Readable> | null = null;
  private connected = false;
  private currentStartRequest: NativeStreamerStartRequest | null = null;
  private startupPromise: Promise<void> | null = null;
  private stopPromise: Promise<void> | null = null;
  private pendingSocketResolve: ((socket: Socket) => void) | null = null;
  private pendingHelloResolver: ((env: Envelope) => void) | null = null;
  private pendingStartupReject: ((error: Error) => void) | null = null;
  private startupTimer: NodeJS.Timeout | null = null;

  constructor(options: NativeStreamerManagerOptions) {
    this.options = options;
  }

  isActive(): boolean {
    return this.connected || this.child !== null;
  }

  async start(request: NativeStreamerStartRequest): Promise<void> {
    this.currentStartRequest = request;
    if (this.startupPromise) {
      return this.startupPromise;
    }
    this.startupPromise = this.doStart(request).finally(() => {
      this.startupPromise = null;
    });
    return this.startupPromise;
  }

  private async doStart(request: NativeStreamerStartRequest): Promise<void> {
    await this.stop();
    this.options.onEvent({ type: "starting", message: "Launching OpenNOW Native Streamer" });
    const endpoint = await this.prepareServer();
    const descriptor = await this.resolveProcessDescriptor();
    try {
      const env = await this.buildProcessEnv(descriptor, endpoint);
      this.child = spawn(descriptor.command, descriptor.args, {
        cwd: descriptor.cwd ?? REPO_ROOT,
        env,
        stdio: ["ignore", "pipe", "pipe"],
      });
      this.child.stdout.on("data", (chunk) => {
        console.log(`[NativeStreamer] ${String(chunk).trimEnd()}`);
      });
      this.child.stderr.on("data", (chunk) => {
        console.warn(`[NativeStreamer] ${String(chunk).trimEnd()}`);
      });

      const socket = await this.waitForSocketConnection();
      this.socket = socket;
      this.installSocket(socket);
      await this.waitForHello(socket);
      this.clearStartupWaiters();

      await this.send("hello-ack", {
        accepted: true,
        version: IPC_VERSION,
        message: "Electron main is ready",
      });
      await this.send("start-session", {
        session: request.session,
        settings: request.settings,
        window: {
          title: request.window?.title ?? "OpenNOW Native Streamer",
          width: request.window?.width ?? 1280,
          height: request.window?.height ?? 720,
        },
      });
      this.connected = true;
      this.options.onEvent({ type: "ready", message: "OpenNOW Native Streamer connected" });
    } catch (error) {
      const failure = error instanceof Error ? error : new Error(String(error));
      this.options.onEvent({ type: "error", code: "native-startup", message: failure.message, fatal: true });
      await this.abortStartup(failure);
      throw failure;
    }
  }

  async stop(): Promise<void> {
    if (this.stopPromise) {
      return this.stopPromise;
    }
    this.stopPromise = this.doStop().finally(() => {
      this.stopPromise = null;
    });
    return this.stopPromise;
  }

  private async doStop(): Promise<void> {
    this.connected = false;
    this.clearStartupWaiters();
    const socket = this.socket;
    if (socket && !socket.destroyed) {
      try {
        await this.send("stop", {});
      } catch {
      }
      this.socket = null;
      socket.destroy();
    } else {
      this.socket = null;
    }
    if (this.child) {
      this.child.kill();
      this.child = null;
    }
    if (this.server) {
      this.server.close();
      this.server = null;
    }
    if (this.socketPath && process.platform !== "win32") {
      await rm(this.socketPath, { force: true }).catch(() => {});
    }
    this.socketPath = null;
    this.currentStartRequest = null;
  }

  async sendInput(input: NativeStreamerInputEnvelope): Promise<void> {
    if (!this.connected) {
      throw new Error("Native streamer is not connected");
    }
    await this.send("input", input);
  }

  async requestKeyframe(payload: KeyframeRequest): Promise<void> {
    if (!this.connected) return;
    await this.send("request-keyframe", payload);
  }

  async handleSignalingEvent(event: MainToRendererSignalingEvent): Promise<boolean> {
    if (!this.currentStartRequest || !this.connected) {
      return false;
    }
    if (event.type === "offer") {
      await this.send("signaling-offer", { sdp: event.sdp });
      return true;
    }
    if (event.type === "remote-ice") {
      await this.send("remote-ice", event.candidate);
      return true;
    }
    if (event.type === "disconnected") {
      this.options.onEvent({ type: "state", status: "signaling-disconnected", message: event.reason });
      return true;
    }
    return false;
  }

  private async send(type: string, payload: unknown): Promise<void> {
    const socket = this.socket;
    if (!socket || socket.destroyed) {
      throw new Error("Native streamer IPC socket is not available");
    }
    const envelope = JSON.stringify({ type, version: IPC_VERSION, payload });
    await new Promise<void>((resolve, reject) => {
      socket.write(`${envelope}\n`, (error) => {
        if (error) reject(error);
        else resolve();
      });
    });
  }

  private async prepareServer(): Promise<string> {
    if (this.server) {
      this.server.close();
      this.server = null;
    }
    if (process.platform === "win32") {
      const port = await new Promise<number>((resolve, reject) => {
        const server = createServer((socket) => {
          this.pendingSocketResolve?.(socket);
        });
        server.once("error", reject);
        server.listen(0, "127.0.0.1", () => {
          const address = server.address();
          if (!address || typeof address === "string") {
            reject(new Error("Could not determine native streamer TCP port"));
            return;
          }
          this.server = server;
          resolve(address.port);
        });
      });
      return `tcp:127.0.0.1:${port}`;
    }

    const endpoint = join(app.getPath("temp"), `opennow-native-streamer-${randomUUID()}.sock`);
    this.socketPath = endpoint;
    await rm(endpoint, { force: true }).catch(() => {});
    this.server = createServer((socket) => {
      this.pendingSocketResolve?.(socket);
    });
    await new Promise<void>((resolve, reject) => {
      this.server?.once("error", reject);
      this.server?.listen(endpoint, () => resolve());
    });
    return `unix:${endpoint}`;
  }

  private installSocket(socket: Socket): void {
    socket.setEncoding("utf8");
    const reader = createInterface({ input: socket, crlfDelay: Infinity });
    reader.on("line", (line) => {
      void this.handleIncoming(line).catch((error) => {
        console.error("[NativeStreamer] IPC decode failed:", error);
      });
    });
    socket.on("error", (error) => {
      void this.handleSocketFailure(socket, error);
    });
    socket.once("close", () => {
      if (this.socket === socket) {
        this.connected = false;
        this.socket = null;
      }
      if (!this.connected && this.pendingStartupReject) {
        this.pendingStartupReject(new Error("Native streamer IPC socket closed during startup"));
      }
      reader.close();
    });
  }

  private async handleSocketFailure(socket: Socket, error: Error & { code?: string }): Promise<void> {
    if (this.socket !== socket && !this.pendingStartupReject) {
      return;
    }
    const message = SOCKET_RECOVERABLE_ERRORS.has(error.code ?? "")
      ? `Native streamer IPC disconnected: ${error.code}`
      : `Native streamer IPC error: ${error.message}`;
    console.warn("[NativeStreamer]", message);
    if (this.pendingStartupReject) {
      this.pendingStartupReject(new Error(message));
      return;
    }
    this.options.onEvent({ type: "error", code: "native-ipc", message, fatal: true });
    await this.abortStartup(new Error(message));
  }

  private async waitForSocketConnection(): Promise<Socket> {
    return await new Promise<Socket>((resolve, reject) => {
      this.pendingSocketResolve = resolve;
      this.pendingStartupReject = reject;
      this.startupTimer = setTimeout(() => {
        reject(new Error(`Native streamer did not connect within ${STARTUP_TIMEOUT_MS}ms`));
      }, STARTUP_TIMEOUT_MS);

      this.child?.once("error", (error) => {
        reject(new Error(`Failed to spawn native streamer: ${error.message}`));
      });
      this.child?.once("exit", (code, signal) => {
        reject(new Error(`Native streamer exited before IPC connect (${signal ?? code ?? "unknown"})`));
      });
    });
  }

  private async waitForHello(socket: Socket): Promise<void> {
    const envelope = await new Promise<Envelope>((resolve, reject) => {
      this.pendingHelloResolver = resolve;
      socket.once("error", reject);
      this.child?.once("exit", () => reject(new Error("Native streamer exited before hello")));
    });
    this.pendingHelloResolver = null;
    if (envelope.type !== "hello") {
      throw new Error(`Expected native hello, received ${envelope.type}`);
    }
  }

  private clearStartupWaiters(): void {
    this.pendingSocketResolve = null;
    this.pendingHelloResolver = null;
    this.pendingStartupReject = null;
    if (this.startupTimer) {
      clearTimeout(this.startupTimer);
      this.startupTimer = null;
    }
  }

  private async abortStartup(error: Error): Promise<void> {
    this.clearStartupWaiters();
    const socket = this.socket;
    this.socket = null;
    if (socket && !socket.destroyed) {
      socket.destroy();
    }
    if (this.child) {
      this.child.kill();
      this.child = null;
    }
    if (this.server) {
      this.server.close();
      this.server = null;
    }
    if (this.socketPath && process.platform !== "win32") {
      await rm(this.socketPath, { force: true }).catch(() => {});
    }
    this.socketPath = null;
    this.connected = false;
    this.currentStartRequest = null;
    this.options.onEvent({ type: "stopped", reason: error.message });
  }

  private async handleIncoming(line: string): Promise<void> {
    if (!line.trim()) return;
    const parsed = JSON.parse(line) as Envelope;
    if (this.pendingHelloResolver) {
      this.pendingHelloResolver(parsed);
      return;
    }
    switch (parsed.type) {
      case "local-answer":
        await this.options.onSendAnswer(parsed.payload as SendAnswerRequest);
        return;
      case "local-ice":
        await this.options.onSendIceCandidate(parsed.payload as IceCandidatePayload);
        return;
      case "state": {
        const payload = parsed.payload as { status: string; message?: string };
        this.options.onEvent({ type: "state", status: payload.status, message: payload.message });
        return;
      }
      case "stats":
        this.options.onEvent({ type: "stats", stats: parsed.payload as NativeStatsPayload });
        return;
      case "error": {
        const payload = parsed.payload as { code: string; message: string; fatal: boolean };
        this.options.onEvent({ type: "error", code: payload.code, message: payload.message, fatal: payload.fatal });
        return;
      }
      default:
        return;
    }
  }

  private async resolveProcessDescriptor(): Promise<NativeProcessDescriptor> {
    const explicit = process.env.OPENNOW_NATIVE_STREAMER_BIN?.trim();
    if (explicit) {
      return { command: explicit, args: [] };
    }

    const name = process.platform === "win32" ? "opennow-native-streamer.exe" : "opennow-native-streamer";
    const candidates = [
      join(REPO_ROOT, "opennow-native-streamer", "bin", process.platform, process.arch, name),
      join(REPO_ROOT, "opennow-native-streamer", name),
      join(process.resourcesPath, "native-streamer", name),
    ];

    for (const candidate of candidates) {
      if (await this.isExecutable(candidate)) {
        return { command: candidate, args: [] };
      }
    }

    return {
      command: process.platform === "win32" ? "go.exe" : "go",
      args: ["run", "./cmd/opennow-native-streamer"],
      cwd: NATIVE_PROJECT_ROOT,
    };
  }

  private async isExecutable(filePath: string): Promise<boolean> {
    try {
      await access(filePath, constants.X_OK);
      if (process.platform !== "win32" && filePath.startsWith(REPO_ROOT)) {
        await chmod(filePath, 0o755).catch(() => {});
      }
      return true;
    } catch {
      return false;
    }
  }

  private async buildProcessEnv(descriptor: NativeProcessDescriptor, endpoint: string): Promise<NodeJS.ProcessEnv> {
    const env: NodeJS.ProcessEnv = {
      ...process.env,
      OPENNOW_NATIVE_STREAMER_ENDPOINT: endpoint,
    };

    if (descriptor.args.length > 0) {
      return env;
    }

    const runtimeDir = dirname(descriptor.command);
    if (process.platform === "win32") {
      const pluginDir = await this.firstExistingPath([
        join(runtimeDir, "gstreamer-plugins"),
        join(runtimeDir, "lib", "gstreamer-1.0"),
      ]);
      const pluginScanner = await this.firstExistingPath([
        join(runtimeDir, "libexec", "gstreamer-1.0", "gst-plugin-scanner.exe"),
        join(runtimeDir, "gst-plugin-scanner.exe"),
      ]);

      env.PATH = [runtimeDir, env.PATH].filter(Boolean).join(";");
      if (pluginDir) {
        env.GST_PLUGIN_PATH_1_0 = pluginDir;
        env.GST_PLUGIN_SYSTEM_PATH_1_0 = pluginDir;
        env.GST_PLUGIN_PATH = pluginDir;
        env.GST_PLUGIN_SYSTEM_PATH = pluginDir;
      }
      if (pluginScanner) {
        env.GST_PLUGIN_SCANNER = pluginScanner;
        env.GST_PLUGIN_SCANNER_1_0 = pluginScanner;
      }

      const registryDir = join(app.getPath("userData"), "native-streamer");
      await mkdir(registryDir, { recursive: true });
      env.GST_REGISTRY = join(registryDir, "registry.bin");
      env.GST_REGISTRY_1_0 = join(registryDir, "registry.bin");
      const pathEntries = [runtimeDir, pluginDir, pluginScanner ? dirname(pluginScanner) : null, env.PATH].filter(Boolean);
      env.PATH = pathEntries.join(";");
    }

    return env;
  }

  private async firstExistingPath(candidates: string[]): Promise<string | null> {
    for (const candidate of candidates) {
      try {
        await access(candidate, constants.F_OK);
        return candidate;
      } catch {
      }
    }
    return null;
  }
}

export function emitNativeStreamerEvent(mainWindow: BrowserWindow | null, event: NativeStreamerEvent): void {
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send(IPC_CHANNELS.NATIVE_STREAMER_EVENT, event);
  }
}
