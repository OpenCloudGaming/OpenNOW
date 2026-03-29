import { randomBytes } from "node:crypto";

import WebSocket from "ws";

import type {
  IceCandidatePayload,
  KeyframeRequest,
  MainToRendererSignalingEvent,
  SendAnswerRequest,
} from "@shared/gfn";
import {
  buildGfnHeaders,
  buildGfnSignalingSignInUrl,
  GFN_PLAY_ORIGIN,
  GFN_USER_AGENT,
  isGfnVerboseLoggingEnabled,
} from "@shared/gfnClient";

interface SignalingMessage {
  ackid?: number;
  ack?: number;
  hb?: number;
  peer_info?: {
    id: number;
  };
  peer_msg?: {
    from: number;
    to: number;
    msg: string;
  };
}

export class GfnSignalingClient {
  private ws: WebSocket | null = null;
  private peerId = 2;
  private peerName = `peer-${Math.floor(Math.random() * 10_000_000_000)}`;
  private ackCounter = 0;
  private heartbeatTimer: NodeJS.Timeout | null = null;
  private listeners = new Set<(event: MainToRendererSignalingEvent) => void>();
  private readonly verboseLogging = isGfnVerboseLoggingEnabled();

  constructor(
    private readonly signalingServer: string,
    private readonly sessionId: string,
    private readonly signalingUrl?: string,
    private readonly pairingId: string = sessionId,
  ) {}

  private buildSignInUrl(): string {
    const serverWithPort = this.signalingServer.includes(":")
      ? this.signalingServer
      : `${this.signalingServer}:443`;
    const baseUrl = this.signalingUrl?.trim() || `wss://${serverWithPort}/nvst/`;
    const url = buildGfnSignalingSignInUrl(baseUrl, this.peerName, this.pairingId);
    const parsedUrl = new URL(url);
    console.log(
      `[Signaling] Prepared sign-in URL host=${parsedUrl.host} path=${parsedUrl.pathname} pairing=${this.pairingId.slice(0, 8)}…`,
    );
    return url;
  }

  private summarizeSdp(label: string, sdp: string): string {
    const lineCount = sdp.split(/\r?\n/).filter(Boolean).length;
    const mediaSections = (sdp.match(/^m=/gm) ?? []).length;
    return `${label}: ${sdp.length} chars, ${lineCount} lines, ${mediaSections} media sections`;
  }

  private summarizeCandidate(candidate: string): string {
    const protocol = candidate.match(/candidate:\S+\s+\d+\s+(\w+)/i)?.[1] ?? "unknown";
    const type = candidate.match(/\styp\s+(\w+)/i)?.[1] ?? "unknown";
    return `${protocol.toLowerCase()}/${type} (${candidate.length} chars)`;
  }

  onEvent(listener: (event: MainToRendererSignalingEvent) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private emit(event: MainToRendererSignalingEvent): void {
    for (const listener of this.listeners) {
      listener(event);
    }
  }

  private nextAckId(): number {
    this.ackCounter += 1;
    return this.ackCounter;
  }

  private sendJson(payload: unknown): void {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      return;
    }
    this.ws.send(JSON.stringify(payload));
  }

  private setupHeartbeat(): void {
    this.clearHeartbeat();
    this.heartbeatTimer = setInterval(() => {
      this.sendJson({ hb: 1 });
    }, 5000);
  }

  private clearHeartbeat(): void {
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
  }

  private sendPeerInfo(): void {
    this.sendJson({
      ackid: this.nextAckId(),
      peer_info: {
        browser: "Chrome",
        browserVersion: "131",
        connected: true,
        id: this.peerId,
        name: this.peerName,
        peerRole: 1,
        resolution: "1920x1080",
        version: 2,
      },
    });
  }

  async connect(): Promise<void> {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      return;
    }

    const url = this.buildSignInUrl();
    const protocol = `x-nv-sessionid.${this.sessionId}`;
    const parsedUrl = new URL(url);

    console.log(
      `[Signaling] Connecting host=${parsedUrl.host} protocol=${protocol} pairing=${this.pairingId.slice(0, 8)}…`,
    );

    await new Promise<void>((resolve, reject) => {
      const urlHost = parsedUrl.host;

      const ws = new WebSocket(url, protocol, {
        rejectUnauthorized: false,
        headers: {
          ...buildGfnHeaders({
            origin: GFN_PLAY_ORIGIN,
          }),
          Host: urlHost,
          "User-Agent": GFN_USER_AGENT,
          "Sec-WebSocket-Key": randomBytes(16).toString("base64"),
        },
      });

      this.ws = ws;

      ws.once("error", (error) => {
        this.emit({ type: "error", message: `Signaling connect failed: ${String(error)}` });
        reject(error);
      });

      ws.once("open", () => {
        this.sendPeerInfo();
        this.setupHeartbeat();
        this.emit({ type: "connected" });
        resolve();
      });

      ws.on("message", (raw) => {
        const text = typeof raw === "string" ? raw : raw.toString("utf8");
        this.handleMessage(text);
      });

      ws.on("close", (_code, reason) => {
        this.clearHeartbeat();
        const reasonText = typeof reason === "string" ? reason : reason.toString("utf8");
        this.emit({ type: "disconnected", reason: reasonText || "socket closed" });
      });
    });
  }

  private handleMessage(text: string): void {
    let parsed: SignalingMessage;
    try {
      parsed = JSON.parse(text) as SignalingMessage;
    } catch {
      this.emit({ type: "log", message: `Ignoring non-JSON signaling packet: ${text.slice(0, 120)}` });
      return;
    }

    if (typeof parsed.ackid === "number") {
      const shouldAck = parsed.peer_info?.id !== this.peerId;
      if (shouldAck) {
        this.sendJson({ ack: parsed.ackid });
      }
    }

    if (parsed.hb) {
      this.sendJson({ hb: 1 });
      return;
    }

    if (!parsed.peer_msg?.msg) {
      return;
    }

    let peerPayload: Record<string, unknown>;
    try {
      peerPayload = JSON.parse(parsed.peer_msg.msg) as Record<string, unknown>;
    } catch {
      this.emit({ type: "log", message: "Received non-JSON peer payload" });
      return;
    }

    if (peerPayload.type === "offer" && typeof peerPayload.sdp === "string") {
      console.log(`[Signaling] ${this.summarizeSdp("Received offer SDP", peerPayload.sdp)}`);
      if (this.verboseLogging) {
        console.debug("[Signaling] Offer SDP preview:", peerPayload.sdp.slice(0, 1000));
      }
      this.emit({ type: "offer", sdp: peerPayload.sdp });
      return;
    }

    if (typeof peerPayload.candidate === "string") {
      console.log(
        `[Signaling] Received remote ICE candidate ${this.summarizeCandidate(peerPayload.candidate)}`,
      );
      this.emit({
        type: "remote-ice",
        candidate: {
          candidate: peerPayload.candidate,
          sdpMid:
            typeof peerPayload.sdpMid === "string" || peerPayload.sdpMid === null
              ? peerPayload.sdpMid
              : undefined,
          sdpMLineIndex:
            typeof peerPayload.sdpMLineIndex === "number" || peerPayload.sdpMLineIndex === null
              ? peerPayload.sdpMLineIndex
              : undefined,
        },
      });
      return;
    }

    console.log("[Signaling] Unhandled peer message keys:", Object.keys(peerPayload));
  }

  async sendAnswer(payload: SendAnswerRequest): Promise<void> {
    console.log(`[Signaling] ${this.summarizeSdp("Sending answer SDP", payload.sdp)}`);
    if (payload.nvstSdp) {
      console.log(`[Signaling] ${this.summarizeSdp("Sending nvstSdp", payload.nvstSdp)}`);
    }
    if (this.verboseLogging) {
      console.debug("[Signaling] Answer SDP preview:", payload.sdp.slice(0, 1000));
      if (payload.nvstSdp) {
        console.debug("[Signaling] nvstSdp preview:", payload.nvstSdp.slice(0, 1000));
      }
    }

    const answer = {
      type: "answer",
      sdp: payload.sdp,
      ...(payload.nvstSdp ? { nvstSdp: payload.nvstSdp } : {}),
    };

    this.sendJson({
      peer_msg: {
        from: this.peerId,
        to: 1,
        msg: JSON.stringify(answer),
      },
      ackid: this.nextAckId(),
    });
  }

  async sendIceCandidate(candidate: IceCandidatePayload): Promise<void> {
    console.log(
      `[Signaling] Sending local ICE candidate ${this.summarizeCandidate(candidate.candidate)} sdpMid=${candidate.sdpMid ?? "?"}`,
    );
    this.sendJson({
      peer_msg: {
        from: this.peerId,
        to: 1,
        msg: JSON.stringify({
          candidate: candidate.candidate,
          sdpMid: candidate.sdpMid,
          sdpMLineIndex: candidate.sdpMLineIndex,
        }),
      },
      ackid: this.nextAckId(),
    });
  }

  async requestKeyframe(payload: KeyframeRequest): Promise<void> {
    this.sendJson({
      peer_msg: {
        from: this.peerId,
        to: 1,
        msg: JSON.stringify({
          type: "request_keyframe",
          reason: payload.reason,
          backlogFrames: payload.backlogFrames,
          attempt: payload.attempt,
        }),
      },
      ackid: this.nextAckId(),
    });
    console.log(
      `[Signaling] Sent keyframe request (reason=${payload.reason}, backlog=${payload.backlogFrames}, attempt=${payload.attempt})`,
    );
  }

  disconnect(): void {
    this.clearHeartbeat();
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
  }
}
