import {
  clampNativeStreamFps,
  type NativeStreamerSessionContext,
  type NvstVideoSession,
  type VideoCodec,
} from "@shared/gfn";

import {
  bindEphemeralUdp,
  createNvstNegotiationDependencies,
  negotiateNvstRtspSession,
  NvstRtspNegotiationError,
  type NvstRtspProbeInput,
  type NvstRtspSession,
  type NvstUdpPortReservation,
} from "./probe";

export type GfnNvstUnavailableCode =
  | "missing-rtsps-endpoints"
  | "negotiation-failed"
  | "preparation-superseded";

export class GfnNvstUnavailableError extends Error {
  constructor(
    readonly code: GfnNvstUnavailableCode,
    message: string,
    options?: ErrorOptions,
  ) {
    super(message, options);
    this.name = "GfnNvstUnavailableError";
  }
}

export interface GfnNvstRtspOwner {
  prepare(context: NativeStreamerSessionContext): Promise<NativeStreamerSessionContext>;
  /** Unix fd of the still-bound video UDP socket, if the probe kept it open. */
  videoUdpFd(): number | undefined;
  /** Releases the video UDP reservation after native has rebound the same port. */
  handoffVideoUdp(): Promise<void>;
  release(reason: string): Promise<void>;
}

export interface GfnNvstRtspSessionOwnerDependencies {
  negotiate?(input: NvstRtspProbeInput): Promise<NvstRtspSession>;
  /** Bind the video/bundle socket in native so ANNOUNCE never races a rebind. */
  reserveVideoUdp?(): Promise<NvstUdpPortReservation>;
  /** Start native receive as soon as video SETUP gives us a peer. */
  onVideoReady?(videoSession: NvstVideoSession): Promise<void>;
  /** Start ICE+DTLS after ANNOUNCE, before PLAY. */
  onAnnounceReady?(videoSession: NvstVideoSession): Promise<void>;
  onLog?(message: string): void;
}

interface OwnedSession {
  sessionId: string;
  rtsp: NvstRtspSession;
}

function withoutNvstVideo(
  context: NativeStreamerSessionContext,
): NativeStreamerSessionContext {
  const { nvstVideo: _nvstVideo, ...rest } = context;
  return rest;
}

function resolveNvstCodec(context: NativeStreamerSessionContext): VideoCodec {
  return context.session.negotiatedStreamProfile?.codec ?? context.settings.codec;
}

export class GfnNvstRtspSessionOwner implements GfnNvstRtspOwner {
  private active: OwnedSession | null = null;
  private revision = 0;
  private operation: Promise<void> = Promise.resolve();

  constructor(
    private readonly dependencies: GfnNvstRtspSessionOwnerDependencies = {},
  ) {}

  prepare(context: NativeStreamerSessionContext): Promise<NativeStreamerSessionContext> {
    const revision = ++this.revision;
    return this.enqueue(async () => {
      if (revision !== this.revision) {
        throw new GfnNvstUnavailableError(
          "preparation-superseded",
          "NVST preparation was superseded before it started",
        );
      }

      if (context.settings.transportMode !== "nvst") {
        await this.releaseActive("native transport is not NVST");
        return withoutNvstVideo(context);
      }

      const sessionId = context.session.sessionId;
      if (this.active?.sessionId === sessionId && this.active.rtsp.isHealthy()) {
        return {
          ...withoutNvstVideo(context),
          nvstVideo: this.active.rtsp.videoSession,
        };
      }

      if (this.active?.sessionId === sessionId) {
        this.log(`Replacing unhealthy NVST RTSPS control session for GFN session ${sessionId}`);
      }

      await this.releaseActive(`replaced by GFN session ${sessionId}`);

      const rtspsEndpoints = context.session.rtspsEndpoints ?? [];
      if (rtspsEndpoints.length === 0) {
        const message = `GFN session ${sessionId} did not provide RTSPS endpoints for explicit NVST mode`;
        this.log(message);
        throw new GfnNvstUnavailableError("missing-rtsps-endpoints", message);
      }

      let rtsp: NvstRtspSession;
      try {
        rtsp = await this.negotiateRtsp({
          sessionId,
          rtspsEndpoints,
          resolution: context.settings.resolution,
          fps: clampNativeStreamFps(
            context.session.negotiatedStreamProfile?.fps ?? context.settings.fps,
          ),
          maxBitrateKbps: Number.isFinite(context.settings.maxBitrateMbps)
            ? Math.round(context.settings.maxBitrateMbps * 1_000)
            : 100_000,
          codec: resolveNvstCodec(context),
          bundlePeer: context.session.mediaConnectionInfo,
          onLog: this.dependencies.onLog,
          onVideoReady: this.dependencies.onVideoReady,
          onAnnounceReady: this.dependencies.onAnnounceReady,
        });
      } catch (error) {
        const detail = error instanceof NvstRtspNegotiationError
          ? `${error.code}: ${error.message}`
          : error instanceof Error
            ? error.message
            : String(error);
        const message = `NVST is unavailable for GFN session ${sessionId}: ${detail}`;
        this.log(message);
        throw new GfnNvstUnavailableError("negotiation-failed", message, {
          cause: error,
        });
      }

      if (revision !== this.revision) {
        await rtsp.release("superseded NVST preparation");
        throw new GfnNvstUnavailableError(
          "preparation-superseded",
          "NVST preparation was superseded before native startup",
        );
      }

      this.active = { sessionId, rtsp };
      this.log(
        `Retaining NVST RTSPS control session for GFN session ${sessionId}${rtsp.videoUdpFd !== undefined ? ` (videoUdpFd=${rtsp.videoUdpFd})` : ""}`,
      );
      return {
        ...withoutNvstVideo(context),
        nvstVideo: rtsp.videoSession,
      };
    });
  }

  videoUdpFd(): number | undefined {
    return this.active?.rtsp.videoUdpFd;
  }

  handoffVideoUdp(): Promise<void> {
    return this.enqueue(async () => {
      const active = this.active;
      if (!active) {
        return;
      }
      await active.rtsp.handoffVideoUdp();
    });
  }

  release(reason: string): Promise<void> {
    ++this.revision;
    return this.enqueue(() => this.releaseActive(reason));
  }

  private negotiateRtsp(input: NvstRtspProbeInput): Promise<NvstRtspSession> {
    if (this.dependencies.negotiate) {
      return this.dependencies.negotiate(input);
    }
    return negotiateNvstRtspSession(
      input,
      createNvstNegotiationDependencies({
        reserveUdpPort: bindEphemeralUdp,
        reserveBundlePort: this.dependencies.reserveVideoUdp ?? bindEphemeralUdp,
      }),
    );
  }

  private enqueue<T>(operation: () => Promise<T>): Promise<T> {
    const result = this.operation.then(operation, operation);
    this.operation = result.then(
      () => undefined,
      () => undefined,
    );
    return result;
  }

  private async releaseActive(reason: string): Promise<void> {
    const active = this.active;
    this.active = null;
    if (!active) {
      return;
    }

    this.log(`Releasing NVST RTSPS control session for GFN session ${active.sessionId} (${reason})`);
    try {
      await active.rtsp.release(reason);
    } catch (error) {
      this.log(
        `Failed to release NVST RTSPS control session for GFN session ${active.sessionId}: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  }

  private log(message: string): void {
    console.log(`[GfnNvstRtspOwner] ${message}`);
    this.dependencies.onLog?.(message);
  }
}
