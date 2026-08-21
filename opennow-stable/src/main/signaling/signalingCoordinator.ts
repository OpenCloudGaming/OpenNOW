import electron, { type BrowserWindow, type IpcMain } from "electron";
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
import { streamDiagnosticId } from "@shared/gfn";
import { setLogContext } from "@shared/logger";
import {
  GfnNvstRtspSessionOwner,
  type GfnNvstRtspOwner,
} from "../platforms/gfn/nvstRtsp/owner";
import { GfnSignalingClient } from "../platforms/gfn/signaling";
import { NativeStreamerManager } from "../nativeStreamer/manager";
import { normalizeNativeInputPacket } from "../nativeStreamer/input";
import { normalizeNativeRenderSurface } from "../nativeStreamer/surface";
import { getNativeCloudGsyncCapabilities } from "../nativeCloudGsync";
import type { SettingsManager } from "../settings";

const { BrowserWindow: ElectronBrowserWindow } = electron;

export interface SignalingCoordinatorDeps {
  ipcMain: IpcMain;
  mainDir: string;
  settingsManager: SettingsManager;
  getMainWindow(): BrowserWindow | null;
  gfnNvstRtspOwner?: GfnNvstRtspOwner;
}

export class SignalingCoordinator {
  private signalingClient: GfnSignalingClient | null = null;
  private signalingClientKey: string | null = null;
  private nativeStreamerManager: NativeStreamerManager | null = null;
  private nativeStreamerContext: NativeStreamerSessionContext | null = null;
  private nativeStreamerFallbackSessionId: string | null = null;
  private nativeSoftwareRetrySessionId: string | null = null;
  private lastSignalingPayload: SignalingConnectRequest | null = null;
  private readonly gfnNvstRtspOwner: GfnNvstRtspOwner;
  private sessionDiagnosticState: Record<string, unknown> = {
    phase: "idle",
  };

  constructor(private readonly deps: SignalingCoordinatorDeps) {
    this.gfnNvstRtspOwner = deps.gfnNvstRtspOwner ?? new GfnNvstRtspSessionOwner({
      reserveVideoUdp: () => this.getNativeStreamerManager().reserveNvstUdp(),
      onVideoReady: async (videoSession) => {
        const current = this.nativeStreamerContext;
        if (!current) {
          throw new Error("Native streamer context missing while arming NVST receive");
        }
        this.nativeStreamerContext = {
          ...current,
          nvstVideo: videoSession,
        };
      },
      onAnnounceReady: async (videoSession) => {
        const current = this.nativeStreamerContext;
        if (!current) {
          throw new Error("Native streamer context missing after NVST ANNOUNCE");
        }
        const armed = {
          ...current,
          nvstVideo: videoSession,
        };
        this.nativeStreamerContext = armed;
        // Official doAnnounce: ANNOUNCE → setupWebRtcTransport → wait DTLS → PLAY.
        await this.getNativeStreamerManager().prepareForSession(armed);
      },
    });
  }

  private retainSessionState(values: Record<string, unknown>): void {
    this.sessionDiagnosticState = {
      ...this.sessionDiagnosticState,
      ...values,
    };
    setLogContext("session.latest", this.sessionDiagnosticState);
  }

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
      IPC_CHANNELS.NATIVE_INPUT_PAUSED,
      (_event, paused: boolean) => {
        if (!this.isNativeStreamerSelected() || !this.nativeStreamerContext) {
          return;
        }

        this.nativeStreamerManager?.setInputPaused(paused === true);
      },
    );

    ipcMain.on(
      IPC_CHANNELS.NATIVE_RENDER_SURFACE,
      (event, payload: NativeRenderSurfaceUpdate) => {
        if (!this.isNativeStreamerSelected()) {
          return;
        }

        const window = ElectronBrowserWindow.fromWebContents(event.sender);
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
        if (this.nativeStreamerContext) {
          this.nativeStreamerContext = {
            ...this.nativeStreamerContext,
            shortcuts,
          };
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
    this.retainSessionState({
      phase: "shutdown",
      stopReason: options.reason,
    });
    if (options.emitDisconnectEvent) {
      this.signalingClient?.disconnect();
    }
    this.signalingClient = null;
    this.signalingClientKey = null;
    this.nativeStreamerManager?.setVideoBackendOverride(null);
    this.nativeStreamerManager?.dispose(options.reason);
    void this.gfnNvstRtspOwner.release(options.reason);
    this.nativeStreamerManager = null;
    this.nativeStreamerContext = null;
    this.nativeStreamerFallbackSessionId = null;
    this.nativeSoftwareRetrySessionId = null;
    this.lastSignalingPayload = null;
  }

  async stopNativeStreamer(reason: string): Promise<void> {
    await Promise.all([
      this.nativeStreamerManager?.stop(reason),
      this.gfnNvstRtspOwner.release(reason),
    ]);
  }

  resetNativeStreamerContext(): void {
    void this.gfnNvstRtspOwner.release("native streamer context reset");
    this.nativeStreamerContext = null;
    this.nativeStreamerFallbackSessionId = null;
    this.nativeSoftwareRetrySessionId = null;
    this.lastSignalingPayload = null;
    this.nativeStreamerManager?.setVideoBackendOverride(null);
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
      key === "nativeStreamerExecutablePath" ||
      key === "nativeCloudGsyncMode" ||
      key === "nativeD3dFullscreenMode" ||
      key === "nativeExternalRenderer" ||
      key === "transportMode"
    ) {
      void this.stopNativeStreamer(
        key === "nativeStreamerExecutablePath"
          ? "native streamer executable changed"
          : key === "nativeCloudGsyncMode"
            ? "native Cloud G-Sync mode changed"
            : key === "nativeD3dFullscreenMode"
              ? "native D3D fullscreen mode changed"
              : key === "nativeExternalRenderer"
                ? "native external renderer setting changed"
                : key === "transportMode"
                  ? "native transport mode changed"
                  : "native streamer disabled",
      );
      this.resetNativeStreamerContext();
    }
    if (key === "nativeVideoBackend") {
      this.nativeSoftwareRetrySessionId = null;
      this.nativeStreamerManager?.setVideoBackendOverride(null);
      if (this.nativeStreamerHasActiveSession()) {
        console.log(
          "[NativeStreamer] Native video backend changed; active session will keep its current backend until the next native streamer restart.",
        );
      } else {
        void this.stopNativeStreamer("native video backend changed");
      }
    }
    if (key === "maxBitrateMbps") {
      this.updateNativeStreamerBitrateSetting(value);
    }
  }

  private async connectSignaling(payload: SignalingConnectRequest): Promise<void> {
    const previousNativeStreamerContext = this.nativeStreamerContext;
    if (
      this.lastSignalingPayload
      && this.lastSignalingPayload.sessionId !== payload.sessionId
    ) {
      this.nativeSoftwareRetrySessionId = null;
      this.nativeStreamerManager?.setVideoBackendOverride(null);
    }
    this.lastSignalingPayload = payload;
    const nextKey = `${payload.sessionId}|${payload.signalingServer}|${payload.signalingUrl ?? ""}`;
    this.nativeStreamerContext = payload.nativeStreamer ?? null;
    this.nativeStreamerFallbackSessionId = null;
    const nativeContext = this.nativeStreamerContext;
    this.retainSessionState({
      streamKey: streamDiagnosticId(payload.sessionId),
      phase: "signaling-connect",
      appId: nativeContext?.session.appId ?? "unknown",
      sessionStatus: nativeContext?.session.status ?? "unknown",
      queuePosition: nativeContext?.session.queuePosition ?? 0,
      seatSetupStep: nativeContext?.session.seatSetupStep ?? 0,
      zone: nativeContext?.session.zone ?? "unknown",
      serverLocation: nativeContext?.session.serverLocation ?? "unknown",
      serverGpuType: nativeContext?.session.gpuType ?? "unknown",
      streamer: nativeContext ? "native" : "web",
      requestedResolution: nativeContext?.settings.resolution ?? "renderer-owned",
      requestedFps: nativeContext?.settings.fps ?? "renderer-owned",
      requestedCodec: nativeContext?.settings.codec ?? "renderer-owned",
      negotiatedResolution: nativeContext?.session.negotiatedStreamProfile?.resolution ?? "unknown",
      negotiatedFps: nativeContext?.session.negotiatedStreamProfile?.fps ?? "unknown",
      negotiatedCodec: nativeContext?.session.negotiatedStreamProfile?.codec ?? "unknown",
      transportMode: nativeContext?.settings.transportMode ?? "webrtc",
      connectedAt: new Date().toISOString(),
    });
    if (this.nativeStreamerContext) {
      console.log(
        "[NativeStreamer] Signaling connect context:",
        JSON.stringify({
          streamKey: streamDiagnosticId(this.nativeStreamerContext.session.sessionId),
          resolution: this.nativeStreamerContext.settings.resolution,
          fps: this.nativeStreamerContext.settings.fps,
          codec: this.nativeStreamerContext.settings.codec,
          transportMode: this.nativeStreamerContext.settings.transportMode ?? "webrtc",
          rtspsEndpoints: this.nativeStreamerContext.session.rtspsEndpoints ?? [],
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
      if (
        previousNativeStreamerContext?.session.sessionId === payload.sessionId
        && previousNativeStreamerContext.nvstVideo
        && this.nativeStreamerContext?.settings.transportMode === "nvst"
      ) {
        this.nativeStreamerContext = {
          ...this.nativeStreamerContext,
          nvstVideo: previousNativeStreamerContext.nvstVideo,
        };
      }
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
      this.retainSessionState({ phase: "signaling-connected" });
    } catch (error) {
      this.retainSessionState({
        phase: "signaling-connect-failed",
        lastError: error instanceof Error ? error.message : String(error),
      });
      await Promise.all([
        this.nativeStreamerManager
          ?.stop("signaling connect failed")
          .catch(() => undefined),
        this.gfnNvstRtspOwner.release("signaling connect failed"),
      ]);
      this.signalingClient = null;
      this.signalingClientKey = null;
      throw error;
    }
  }

  private async disconnectSignaling(): Promise<void> {
    this.retainSessionState({
      phase: "disconnecting",
      stopReason: "renderer signaling disconnect",
    });
    await Promise.all([
      this.nativeStreamerManager?.stop("signaling disconnect"),
      this.gfnNvstRtspOwner.release("signaling disconnect"),
    ]);
    this.nativeStreamerManager?.setVideoBackendOverride(null);
    this.nativeStreamerContext = null;
    this.nativeStreamerFallbackSessionId = null;
    this.nativeSoftwareRetrySessionId = null;
    this.lastSignalingPayload = null;
    this.signalingClient?.disconnect();
    this.signalingClient = null;
    this.signalingClientKey = null;
    this.retainSessionState({ phase: "disconnected" });
  }

  private emitToRenderer(event: MainToRendererSignalingEvent): void {
    const mainWindow = this.deps.getMainWindow();
    if (mainWindow && !mainWindow.isDestroyed()) {
      if (event.type === "native-stream-stats") {
        const serverGpuType = event.stats.serverGpuType?.trim()
          || this.nativeStreamerContext?.session.gpuType?.trim()
          || undefined;
        const serverLocation = event.stats.serverLocation?.trim()
          || this.nativeStreamerContext?.session.serverLocation?.trim()
          || undefined;
        event = {
          ...event,
          stats: {
            ...event.stats,
            serverGpuType,
            serverLocation,
          },
        };
      }
      mainWindow.webContents.send(IPC_CHANNELS.SIGNALING_EVENT, event);
    }
  }

  private getNativeStreamerManager(): NativeStreamerManager {
    this.nativeStreamerManager ??= new NativeStreamerManager({
      mainDir: this.deps.mainDir,
      getVideoBackendPreference: () =>
        this.deps.settingsManager?.get("nativeVideoBackend") ?? "auto",
      getExecutablePathOverride: () =>
        this.deps.settingsManager?.get("nativeStreamerExecutablePath") ?? "",
      getCloudGsyncMode: () =>
        this.deps.settingsManager?.get("nativeCloudGsyncMode") ?? "auto",
      getD3dFullscreenMode: () =>
        this.deps.settingsManager?.get("nativeD3dFullscreenMode") ?? "auto",
      getExternalRendererEnabled: () =>
        this.deps.settingsManager?.get("nativeExternalRenderer") ?? false,
      emit: (event) => {
        if (event.type === "native-stream-stopped") {
          void this.gfnNvstRtspOwner.release(
            event.reason
              ? `native streamer stopped: ${event.reason}`
              : "native streamer stopped",
          );
        }
        this.emitToRenderer(event);
      },
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
      retryWithSoftwareDecoder: (message) => {
        void this.retryNativeWithSoftwareDecoder(message);
      },
    });
    return this.nativeStreamerManager;
  }

  private async retryNativeWithSoftwareDecoder(message: string): Promise<void> {
    const context = this.nativeStreamerContext;
    const payload = this.lastSignalingPayload;
    const manager = this.nativeStreamerManager;
    if (!context || !payload || !manager) {
      this.emitToRenderer({
        type: "error",
        message: `Native software decoder recovery could not start: ${message}`,
      });
      return;
    }

    const sessionId = context.session.sessionId;
    if (
      this.nativeSoftwareRetrySessionId === sessionId
      || this.deps.settingsManager.get("nativeVideoBackend") === "software"
    ) {
      this.emitToRenderer({
        type: "error",
        message: `Native software decoder failed to produce video: ${message}`,
      });
      return;
    }

    this.nativeSoftwareRetrySessionId = sessionId;
    manager.setVideoBackendOverride("software");
    this.emitToRenderer({
      type: "log",
      message: "Native hardware decoding produced no frames; reconnecting once with the native software decoder.",
    });

    this.signalingClient?.disconnect();
    this.signalingClient = null;
    this.signalingClientKey = null;
    await manager.stop("retrying native video with software decoding").catch(() => undefined);

    try {
      await this.connectSignaling(payload);
    } catch (error) {
      const recoveryMessage = error instanceof Error ? error.message : String(error);
      this.emitToRenderer({
        type: "error",
        message: `Native software decoder recovery failed: ${recoveryMessage}`,
      });
    }
  }

  private isNativeStreamerSelected(): boolean {
    return this.deps.settingsManager?.get("streamClientMode") === "native";
  }

  private routeSignalingEvent(event: MainToRendererSignalingEvent): void {
    if (event.type === "disconnected") {
      this.retainSessionState({
        phase: "remote-disconnected",
        stopReason: event.reason,
      });
      void this.nativeStreamerManager?.stop(
        `signaling disconnected: ${event.reason}`,
      );
      void this.gfnNvstRtspOwner.release(
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

    const nativeNvstActive = this.nativeStreamerManager
      ?.isNvstSessionActive(context.session.sessionId) ?? false;

    if (
      nativeNvstActive
      && (event.type === "offer" || event.type === "remote-ice")
    ) {
      console.log(
        `[NativeStreamer] Explicit NVST is active; ignoring WebRTC ${event.type} (${
          event.type === "offer" ? `sdpBytes=${event.sdp.length}` : "candidate"
        })`,
      );
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
      if (context.settings.transportMode === "nvst") {
        console.warn("[NativeStreamer] Explicit NVST startup failed:", message);
        this.retainSessionState({
          streamer: "native",
          phase: "native-nvst-failed",
          lastError: message,
        });
        await Promise.all([
          this.nativeStreamerManager
            ?.stop("explicit NVST startup failed")
            .catch(() => undefined),
          this.gfnNvstRtspOwner.release("explicit NVST startup failed"),
        ]);
        this.emitToRenderer({
          type: "error",
          message: `Native NVST failed: ${message}. WebRTC media fallback is disabled for explicit NVST mode.`,
        });
        return;
      }
      console.warn("[NativeStreamer] Falling back to web streamer:", message);
      this.retainSessionState({
        streamer: "web-fallback",
        phase: "native-fallback",
        lastError: message,
      });
      this.nativeStreamerFallbackSessionId = context.session.sessionId;
      const queuedRemoteIce =
        this.nativeStreamerManager?.drainQueuedRemoteIce(
          context.session.sessionId,
        ) ?? [];
      await Promise.all([
        this.nativeStreamerManager
          ?.stop("native streamer fallback")
          .catch(() => undefined),
        this.gfnNvstRtspOwner.release("native streamer fallback"),
      ]);
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
      const preparedContext = await this.gfnNvstRtspOwner.prepare(context);
      if (
        this.nativeStreamerContext?.session.sessionId
        !== preparedContext.session.sessionId
      ) {
        await this.gfnNvstRtspOwner.release(
          "native streamer context changed during NVST preparation",
        );
        throw new Error("Native streamer context changed during NVST preparation");
      }
      this.nativeStreamerContext = preparedContext;
      // Official Bifrost binds the ICE/bundle socket in-process before ANNOUNCE
      // and never rebinds. Native reserved that socket during prepare(); start
      // must reuse it on the same process.
      await this.getNativeStreamerManager().prepareForSession(preparedContext);
      await this.gfnNvstRtspOwner.handoffVideoUdp();
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (context.settings.transportMode === "nvst") {
        console.warn("[NativeStreamer] Explicit NVST pre-attach startup failed:", message);
        this.retainSessionState({
          streamer: "native",
          phase: "native-nvst-pre-attach-failed",
          lastError: message,
        });
        await Promise.all([
          this.nativeStreamerManager
            ?.stop("explicit NVST pre-attach startup failed")
            .catch(() => undefined),
          this.gfnNvstRtspOwner.release("explicit NVST pre-attach startup failed"),
        ]);
        this.emitToRenderer({
          type: "error",
          message: `Native NVST failed before signaling attach: ${message}. WebRTC media fallback is disabled for explicit NVST mode.`,
        });
        throw error;
      }
      console.warn(
        "[NativeStreamer] Pre-attach startup failed; falling back to web streamer:",
        message,
      );
      this.retainSessionState({
        streamer: "web-fallback",
        phase: "native-pre-attach-fallback",
        lastError: message,
      });
      this.nativeStreamerFallbackSessionId = context.session.sessionId;
      await Promise.all([
        this.nativeStreamerManager
          ?.stop("native streamer pre-attach fallback")
          .catch(() => undefined),
        this.gfnNvstRtspOwner.release("native streamer pre-attach fallback"),
      ]);
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
