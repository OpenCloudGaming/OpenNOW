import { BrowserWindow, type IpcMain } from "electron";
import { IPC_CHANNELS } from "@shared/ipc";
import type {
  IceCandidatePayload,
  KeyframeRequest,
  MainToRendererSignalingEvent,
  NativeInputPacket,
  NativeRenderSurfaceUpdate,
  NativeStreamerSessionContext,
  NativeStreamerShortcutBindings,
  NativeStreamerStatus,
  SendAnswerRequest,
  Settings,
  SignalingConnectRequest,
} from "@shared/gfn";
import { GfnSignalingClient } from "../gfn/signaling";
import { NativeStreamerManager } from "../nativeStreamer/manager";
import { normalizeNativeInputPacket } from "../nativeStreamer/input";
import { normalizeNativeRenderSurface } from "../nativeStreamer/surface";
import { getNativeCloudGsyncCapabilities } from "../nativeCloudGsync";
import type { SettingsManager } from "../settings";

export interface SignalingCoordinatorDeps {
  ipcMain: IpcMain;
  mainDir: string;
  settingsManager: SettingsManager;
  getMainWindow(): BrowserWindow | null;
}

export class SignalingCoordinator {
  private signalingClient: GfnSignalingClient | null = null;
  private signalingClientKey: string | null = null;
  private nativeStreamerManager: NativeStreamerManager | null = null;
  private nativeStreamerContext: NativeStreamerSessionContext | null = null;
  private nativeStreamerFallbackSessionId: string | null = null;

  constructor(private readonly deps: SignalingCoordinatorDeps) {}

  registerIpcHandlers(): void {
    const { ipcMain } = this.deps;

    ipcMain.handle(
      IPC_CHANNELS.CONNECT_SIGNALING,
      async (_event, payload: SignalingConnectRequest): Promise<void> => {
        await this.connectSignaling(payload);
      },
    );

    ipcMain.handle(IPC_CHANNELS.DISCONNECT_SIGNALING, async (): Promise<void> => {
      await this.disconnectSignaling();
    });

    ipcMain.handle(
      IPC_CHANNELS.SEND_ANSWER,
      async (_event, payload: SendAnswerRequest) => {
        if (!this.signalingClient) {
          throw new Error("Signaling is not connected");
        }
        return this.signalingClient.sendAnswer(payload);
      },
    );

    ipcMain.handle(
      IPC_CHANNELS.SEND_ICE_CANDIDATE,
      async (_event, payload: IceCandidatePayload) => {
        if (!this.signalingClient) {
          throw new Error("Signaling is not connected");
        }
        return this.signalingClient.sendIceCandidate(payload);
      },
    );

    ipcMain.on(
      IPC_CHANNELS.NATIVE_INPUT,
      (_event, payload: NativeInputPacket) => {
        if (!this.isNativeStreamerSelected()) {
          return;
        }

        const context = this.nativeStreamerContext;
        if (
          !context ||
          this.nativeStreamerFallbackSessionId === context.session.sessionId
        ) {
          return;
        }

        const packet = normalizeNativeInputPacket(payload);
        if (!packet) {
          return;
        }

        this.nativeStreamerManager?.sendInput(packet);
      },
    );

    ipcMain.on(
      IPC_CHANNELS.NATIVE_RENDER_SURFACE,
      (event, payload: NativeRenderSurfaceUpdate) => {
        if (!this.isNativeStreamerSelected()) {
          return;
        }

        const window = BrowserWindow.fromWebContents(event.sender);
        if (!window || window.isDestroyed()) {
          return;
        }

        const surface = normalizeNativeRenderSurface(window, payload);
        if (!surface) {
          return;
        }

        this.getNativeStreamerManager().updateSurface(surface);
      },
    );

    ipcMain.on(
      IPC_CHANNELS.NATIVE_UPDATE_SHORTCUTS,
      (_event, shortcuts: NativeStreamerShortcutBindings) => {
        if (!this.isNativeStreamerSelected()) {
          return;
        }
        this.getNativeStreamerManager().updateShortcuts(shortcuts);
      },
    );

    ipcMain.handle(
      IPC_CHANNELS.REQUEST_KEYFRAME,
      async (_event, payload: KeyframeRequest) => {
        if (!this.signalingClient) {
          throw new Error("Signaling is not connected");
        }
        return this.signalingClient.requestKeyframe(payload);
      },
    );

    ipcMain.handle(
      IPC_CHANNELS.NATIVE_STREAMER_STATUS,
      async (): Promise<NativeStreamerStatus> => {
        return this.getNativeStreamerManager().probeStatus();
      },
    );

    ipcMain.handle(IPC_CHANNELS.NATIVE_CLOUD_GSYNC_CAPABILITIES, async () => {
      const capabilities = await getNativeCloudGsyncCapabilities(
        this.deps.settingsManager?.get("nativeCloudGsyncMode") ?? "auto",
      );
      console.log(
        `[CloudGsync] capability probe: ${JSON.stringify(capabilities)}`,
      );
      return capabilities;
    });
  }

  disconnectForShutdown(options: {
    emitDisconnectEvent: boolean;
    reason: string;
  }): void {
    if (options.emitDisconnectEvent) {
      this.signalingClient?.disconnect();
    }
    this.signalingClient = null;
    this.signalingClientKey = null;
    this.nativeStreamerManager?.dispose(options.reason);
    this.nativeStreamerManager = null;
    this.nativeStreamerContext = null;
    this.nativeStreamerFallbackSessionId = null;
  }

  stopNativeStreamer(reason: string): void {
    void this.nativeStreamerManager?.stop(reason);
  }

  resetNativeStreamerContext(): void {
    this.nativeStreamerContext = null;
    this.nativeStreamerFallbackSessionId = null;
  }

  nativeStreamerHasActiveSession(): boolean {
    return this.nativeStreamerManager?.hasActiveSession() ?? false;
  }

  updateNativeStreamerBitrateSetting(value: unknown): void {
    const maxBitrateMbps = normalizeMaxBitrateMbps(value);
    if (maxBitrateMbps === null) {
      return;
    }

    if (this.nativeStreamerContext) {
      this.nativeStreamerContext = {
        ...this.nativeStreamerContext,
        settings: {
          ...this.nativeStreamerContext.settings,
          maxBitrateMbps,
        },
      };
    }

    this.nativeStreamerManager?.updateBitrateLimit(maxBitrateMbps * 1000);
  }

  applySettingsChange<K extends keyof Settings>(
    key: K,
    value: Settings[K],
  ): void {
    if (
      (key === "streamClientMode" && value !== "native") ||
      key === "nativeStreamerBackend" ||
      key === "nativeStreamerExecutablePath" ||
      key === "nativeCloudGsyncMode" ||
      key === "nativeD3dFullscreenMode" ||
      key === "nativeExternalRenderer"
    ) {
      this.stopNativeStreamer(
        key === "nativeStreamerBackend"
          ? "native streamer backend changed"
          : key === "nativeStreamerExecutablePath"
            ? "native streamer executable changed"
            : key === "nativeCloudGsyncMode"
              ? "native Cloud G-Sync mode changed"
              : key === "nativeD3dFullscreenMode"
                ? "native D3D fullscreen mode changed"
                : key === "nativeExternalRenderer"
                  ? "native external renderer setting changed"
                  : "native streamer disabled",
      );
      this.resetNativeStreamerContext();
    }
    if (key === "nativeVideoBackend") {
      if (this.nativeStreamerHasActiveSession()) {
        console.log(
          "[NativeStreamer] Native video backend changed; active session will keep its current backend until the next native streamer restart.",
        );
      } else {
        this.stopNativeStreamer("native video backend changed");
      }
    }
    if (key === "maxBitrateMbps") {
      this.updateNativeStreamerBitrateSetting(value);
    }
  }

  private async connectSignaling(payload: SignalingConnectRequest): Promise<void> {
    const nextKey = `${payload.sessionId}|${payload.signalingServer}|${payload.signalingUrl ?? ""}`;
    this.nativeStreamerContext = payload.nativeStreamer ?? null;
    this.nativeStreamerFallbackSessionId = null;
    if (this.nativeStreamerContext) {
      console.log(
        "[NativeStreamer] Signaling connect context:",
        JSON.stringify({
          sessionId: this.nativeStreamerContext.session.sessionId,
          resolution: this.nativeStreamerContext.settings.resolution,
          fps: this.nativeStreamerContext.settings.fps,
          codec: this.nativeStreamerContext.settings.codec,
          negotiatedStreamProfile:
            this.nativeStreamerContext.session.negotiatedStreamProfile,
          requestedStreamingFeatures:
            this.nativeStreamerContext.session.requestedStreamingFeatures,
          finalizedStreamingFeatures:
            this.nativeStreamerContext.session.finalizedStreamingFeatures,
        }),
      );
    }

    if (this.signalingClient && this.signalingClientKey === nextKey) {
      console.log(
        "[Signaling] Reuse existing signaling connection (duplicate connect request ignored)",
      );
      return;
    }

    if (this.signalingClient) {
      this.signalingClient.disconnect();
    }
    await this.resetNativeStreamerForSignalingReconnect();
    await this.prepareNativeStreamerBeforeSignaling();

    this.signalingClient = new GfnSignalingClient(
      payload.signalingServer,
      payload.sessionId,
      payload.signalingUrl,
    );
    this.signalingClientKey = nextKey;
    this.signalingClient.onEvent((event) => this.routeSignalingEvent(event));
    try {
      await this.signalingClient.connect();
    } catch (error) {
      await this.nativeStreamerManager
        ?.stop("signaling connect failed")
        .catch(() => undefined);
      this.signalingClient = null;
      this.signalingClientKey = null;
      throw error;
    }
  }

  private async disconnectSignaling(): Promise<void> {
    await this.nativeStreamerManager?.stop("signaling disconnect");
    this.nativeStreamerContext = null;
    this.nativeStreamerFallbackSessionId = null;
    this.signalingClient?.disconnect();
    this.signalingClient = null;
    this.signalingClientKey = null;
  }

  private emitToRenderer(event: MainToRendererSignalingEvent): void {
    const mainWindow = this.deps.getMainWindow();
    if (mainWindow && !mainWindow.isDestroyed()) {
      mainWindow.webContents.send(IPC_CHANNELS.SIGNALING_EVENT, event);
    }
  }

  private getNativeStreamerManager(): NativeStreamerManager {
    this.nativeStreamerManager ??= new NativeStreamerManager({
      mainDir: this.deps.mainDir,
      getBackendPreference: () => "gstreamer",
      getVideoBackendPreference: () =>
        this.deps.settingsManager?.get("nativeVideoBackend") ?? "auto",
      getExecutablePathOverride: () =>
        this.deps.settingsManager?.get("nativeStreamerExecutablePath") ?? "",
      getCloudGsyncMode: () =>
        this.deps.settingsManager?.get("nativeCloudGsyncMode") ?? "auto",
      getD3dFullscreenMode: () =>
        this.deps.settingsManager?.get("nativeD3dFullscreenMode") ?? "auto",
      getExternalRendererEnabled: () => true,
      emit: (event) => this.emitToRenderer(event),
      sendAnswer: async (payload) => {
        if (!this.signalingClient) {
          throw new Error("Signaling is not connected");
        }
        await this.signalingClient.sendAnswer(payload);
      },
      sendIceCandidate: async (candidate) => {
        if (!this.signalingClient) {
          throw new Error("Signaling is not connected");
        }
        await this.signalingClient.sendIceCandidate(candidate);
      },
      requestKeyframe: async (payload) => {
        if (!this.signalingClient) {
          throw new Error("Signaling is not connected");
        }
        await this.signalingClient.requestKeyframe(payload);
      },
    });
    return this.nativeStreamerManager;
  }

  private isNativeStreamerSelected(): boolean {
    return this.deps.settingsManager?.get("streamClientMode") === "native";
  }

  private routeSignalingEvent(event: MainToRendererSignalingEvent): void {
    if (event.type === "disconnected") {
      void this.nativeStreamerManager?.stop(
        `signaling disconnected: ${event.reason}`,
      );
      this.nativeStreamerContext = null;
      this.nativeStreamerFallbackSessionId = null;
      this.emitToRenderer(event);
      return;
    }

    const context = this.nativeStreamerContext;
    const nativeFallbackActive =
      context !== null &&
      this.nativeStreamerFallbackSessionId === context.session.sessionId;

    if (!this.isNativeStreamerSelected() || !context || nativeFallbackActive) {
      this.emitToRenderer(event);
      return;
    }

    if (event.type === "offer") {
      void this.handleNativeStreamerOffer(event.sdp, context);
      return;
    }

    if (event.type === "remote-ice") {
      void this.getNativeStreamerManager()
        .addRemoteIce(event.candidate, context)
        .catch((error) => {
          this.emitToRenderer({
            type: "error",
            message: `Native streamer ICE failed: ${String(error)}`,
          });
        });
      return;
    }

    this.emitToRenderer(event);
  }

  private async handleNativeStreamerOffer(
    sdp: string,
    context: NativeStreamerSessionContext,
  ): Promise<void> {
    try {
      await this.getNativeStreamerManager().handleOffer(sdp, context);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      console.warn("[NativeStreamer] Falling back to web streamer:", message);
      this.nativeStreamerFallbackSessionId = context.session.sessionId;
      const queuedRemoteIce =
        this.nativeStreamerManager?.drainQueuedRemoteIce(
          context.session.sessionId,
        ) ?? [];
      await this.nativeStreamerManager
        ?.stop("native streamer fallback")
        .catch(() => undefined);
      this.emitToRenderer({
        type: "error",
        message: `Native streamer failed: ${message}. Falling back to web streamer.`,
      });
      this.emitToRenderer({ type: "offer", sdp });
      for (const candidate of queuedRemoteIce) {
        this.emitToRenderer({ type: "remote-ice", candidate });
      }
    }
  }

  private async resetNativeStreamerForSignalingReconnect(): Promise<void> {
    if (!this.nativeStreamerManager) {
      return;
    }

    if (
      !this.isNativeStreamerSelected() ||
      !this.nativeStreamerContext ||
      this.nativeStreamerManager.hasActiveSession()
    ) {
      await this.nativeStreamerManager.stop("signaling reconnect");
    }
  }

  private async prepareNativeStreamerBeforeSignaling(): Promise<void> {
    const context = this.nativeStreamerContext;
    if (!this.isNativeStreamerSelected() || !context) {
      return;
    }

    try {
      this.emitToRenderer({
        type: "log",
        message: "Preparing native streamer before signaling attach.",
      });
      await this.getNativeStreamerManager().prepareForSession(context);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      console.warn(
        "[NativeStreamer] Pre-attach startup failed; falling back to web streamer:",
        message,
      );
      this.nativeStreamerFallbackSessionId = context.session.sessionId;
      await this.nativeStreamerManager
        ?.stop("native streamer pre-attach fallback")
        .catch(() => undefined);
      this.emitToRenderer({
        type: "error",
        message: `Native streamer failed before signaling attach: ${message}. Falling back to web streamer.`,
      });
    }
  }
}

export function registerSignalingIpcHandlers(
  deps: SignalingCoordinatorDeps,
): SignalingCoordinator {
  const coordinator = new SignalingCoordinator(deps);
  coordinator.registerIpcHandlers();
  return coordinator;
}

export function normalizeMaxBitrateMbps(value: unknown): number | null {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return null;
  }

  return Math.min(150, Math.max(5, Math.round(value)));
}
