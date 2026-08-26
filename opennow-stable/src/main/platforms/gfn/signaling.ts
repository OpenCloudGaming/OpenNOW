import { randomBytes } from "node:crypto";

import WebSocket from "ws";

import type {
  IceCandidatePayload,
  KeyframeRequest,
  MainToRendererSignalingEvent,
  SendAnswerRequest,
} from "@shared/gfn";
import {
  iceCandidateDiagnosticSummary,
  sdpDiagnosticSummary,
  signalingUrlForDiagnostics,
  streamDiagnosticId,
} from "@shared/gfn";
import { setLogContext } from "@shared/logger";
import { GFN_PLAY_ORIGIN, GFN_USER_AGENT } from "./clientHeaders";

interface SignalingMessage {
  ackid?: number;
  ack?: number;
  hb?: number;
  error?: string;
  peer_info?: {
    id: number;
    name?: string;
  };
  peer_msg?: {
    from: number;
    to: number;
    msg: string;
  };
}

export class GfnSignalingClient {
  private ws: WebSocket | null = null;
  private peerId = 0;
  private remotePeerId = 1;
  private peerName = `peer-${Math.floor(Math.random() * 10_000_000_000)}`;
  private ackCounter = 0;
  private heartbeatTimer: NodeJS.Timeout | null = null;
  private connectionGeneration = 0;
  private listeners = new Set<(event: MainToRendererSignalingEvent) => void>();

  constructor(
    private readonly signalingServer: string,
    private readonly sessionId: string,
    private readonly signalingUrl?: string,
  ) {}

  private buildSignInUrl(): string {
    const fallbackHost = this.signalingServer.includes(":")
      ? this.signalingServer
      : `${this.signalingServer}:443`;
    const baseUrl = this.signalingUrl?.trim() || `wss://${fallbackHost}/nvst/`;
    const signInUrl = new URL(baseUrl);

    signInUrl.protocol = "wss:";
    signInUrl.pathname = `${signInUrl.pathname.replace(/\/?$/, "/")}sign_in`;
    signInUrl.searchParams.set("peer_id", this.peerName);
    signInUrl.searchParams.set("version", "2");
    signInUrl.searchParams.set("peer_role", "1");
    signInUrl.searchParams.set("pairing_id", this.sessionId);

    return signInUrl.toString();
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

  private sendJson(payload: unknown): boolean {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      return false;
    }
    this.ws.send(JSON.stringify(payload));
    return true;
  }

  private retainState(state: string, details: Record<string, unknown> = {}): void {
    setLogContext("signaling.latest", {
      streamKey: streamDiagnosticId(this.sessionId),
      state,
      server: this.signalingServer,
      url: signalingUrlForDiagnostics(this.signalingUrl, this.sessionId),
      heartbeatIntervalMs: 5000,
      ...details,
    });
  }

  private setupHeartbeat(): void {
    this.clearHeartbeat();
    this.heartbeatTimer = setInterval(() => {
      this.sendJson({ hb: 1 });
    }, 5000);
    this.heartbeatTimer.unref?.();
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
        peerRole: 0,
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
    const generation = ++this.connectionGeneration;

    this.retainState("connecting");
    console.log(
      `[Signaling] Connecting session=${streamDiagnosticId(this.sessionId)} url=${signalingUrlForDiagnostics(url, this.sessionId)}`,
    );

    await new Promise<void>((resolve, reject) => {
      // Extract host:port for the Host header (matching Rust behavior)
      const urlHost = url.replace(/^wss?:\/\//, "").split("/")[0];

      const ws = new WebSocket(url, protocol, {
        headers: {
          Host: urlHost,
          Origin: GFN_PLAY_ORIGIN,
          "User-Agent": GFN_USER_AGENT,
          "Sec-WebSocket-Key": randomBytes(16).toString("base64"),
        },
      });

      this.ws = ws;

      const isCurrentSocket = (): boolean => this.ws === ws && this.connectionGeneration === generation;

      ws.once("error", (error) => {
        if (!isCurrentSocket()) {
          return;
        }
        this.retainState("connect-failed", {
          error: error instanceof Error ? error.message : String(error),
        });
        console.warn(
          `[Signaling] Connect failed session=${streamDiagnosticId(this.sessionId)}:`,
          error,
        );
        this.emit({ type: "error", message: `Signaling connect failed: ${String(error)}` });
        reject(error);
      });

      ws.once("open", () => {
        if (!isCurrentSocket()) {
          return;
        }
        this.sendPeerInfo();
        this.setupHeartbeat();
        this.retainState("connected", { protocol: `x-nv-sessionid.${streamDiagnosticId(this.sessionId)}` });
        console.log(
          `[Signaling] Connected session=${streamDiagnosticId(this.sessionId)} heartbeatMs=5000`,
        );
        this.emit({ type: "connected" });
        resolve();
      });

      ws.on("message", (raw) => {
        if (!isCurrentSocket()) {
          return;
        }
        const text = typeof raw === "string" ? raw : raw.toString("utf8");
        this.handleMessage(text);
      });

      ws.on("close", (code, reason) => {
        this.clearHeartbeat();

        if (!isCurrentSocket()) {
          return;
        }

        this.ws = null;

        const reasonText = typeof reason === "string" ? reason : reason.toString("utf8");
        this.retainState("closed", { closeCode: code, closeReason: reasonText || "socket closed" });
        console.warn(
          `[Signaling] Closed session=${streamDiagnosticId(this.sessionId)} code=${code} reason=${reasonText || "none"}`,
        );
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

    if (parsed.peer_info) {
      if (typeof parsed.peer_info.id === "number" && parsed.peer_info.name === this.peerName) {
        this.peerId = parsed.peer_info.id;
        console.log(`[Signaling] Local peer id assigned: ${this.peerId}`);
      }
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

    if (parsed.error === "peerRemoved") {
      console.log("[Signaling] Received peerRemoved signaling error");
      this.emit({ type: "disconnected", reason: "peerRemoved" });
      return;
    }

    if (!parsed.peer_msg?.msg) {
      return;
    }

    if (typeof parsed.peer_msg.from === "number") {
      this.remotePeerId = parsed.peer_msg.from;
      console.log(`[Signaling] Remote peer id: ${this.remotePeerId}`);
    }

    const peerMessage = parsed.peer_msg.msg.trim();
    if (peerMessage === "BYE") {
      console.log("[Signaling] Received BYE peer message");
      this.emit({ type: "disconnected", reason: "BYE" });
      return;
    }

    let peerPayload: Record<string, unknown>;
    try {
      peerPayload = JSON.parse(peerMessage) as Record<string, unknown>;
    } catch {
      this.emit({ type: "log", message: "Received non-JSON peer payload" });
      return;
    }

    if (peerPayload.type === "offer" && typeof peerPayload.sdp === "string") {
      console.log(`[Signaling] ${sdpDiagnosticSummary("Received offer", peerPayload.sdp)}`);
      this.emit({ type: "offer", sdp: peerPayload.sdp });
      return;
    }

    if (typeof peerPayload.candidate === "string") {
      const sdpMLineIndex =
        typeof peerPayload.sdpMLineIndex === "number" || peerPayload.sdpMLineIndex === null
          ? peerPayload.sdpMLineIndex
          : 0;
      const candidate = {
        candidate: peerPayload.candidate,
        sdpMid:
          typeof peerPayload.sdpMid === "string" || peerPayload.sdpMid === null
            ? peerPayload.sdpMid
            : undefined,
        sdpMLineIndex,
        usernameFragment:
          typeof peerPayload.usernameFragment === "string" || peerPayload.usernameFragment === null
            ? peerPayload.usernameFragment
            : undefined,
      } satisfies IceCandidatePayload;
      console.log(`[Signaling] Received remote ICE candidate ${iceCandidateDiagnosticSummary(candidate)}`);
      this.emit({
        type: "remote-ice",
        candidate,
      });
      return;
    }

    // Log any unhandled peer message types for debugging
    console.log("[Signaling] Unhandled peer message keys:", Object.keys(peerPayload));
  }

  async sendAnswer(payload: SendAnswerRequest): Promise<void> {
    console.log(`[Signaling] ${sdpDiagnosticSummary("Sending answer", payload.sdp)}`);
    if (payload.nvstSdp) {
      console.log(
        `[Signaling] Sending NVST SDP lines=${payload.nvstSdp.split(/\r?\n/).filter(Boolean).length} bytes=${payload.nvstSdp.length}`,
      );
    }
    const answer = {
      type: "answer",
      sdp: payload.sdp,
      ...(payload.nvstSdp ? { nvstSdp: payload.nvstSdp } : {}),
    };

    console.log(`[Signaling] Sending answer peer_msg from=${this.peerId} to=${this.remotePeerId}`);
    this.sendJson({
      peer_msg: {
        from: this.peerId,
        to: this.remotePeerId,
        msg: JSON.stringify(answer),
      },
      ackid: this.nextAckId(),
    });
  }

  async sendIceCandidate(candidate: IceCandidatePayload): Promise<void> {
    if (isTcpIceCandidate(candidate.candidate)) {
      console.log(`[Signaling] Dropping TCP local ICE candidate ${iceCandidateDiagnosticSummary(candidate)}`);
      return;
    }

    console.log(`[Signaling] Sending local ICE candidate ${iceCandidateDiagnosticSummary(candidate)}`);
    console.log(`[Signaling] Sending ICE peer_msg from=${this.peerId} to=${this.remotePeerId}`);
    this.sendJson({
      peer_msg: {
        from: this.peerId,
        to: this.remotePeerId,
        msg: JSON.stringify({
          candidate: candidate.candidate,
          sdpMid: candidate.sdpMid,
          sdpMLineIndex: candidate.sdpMLineIndex,
          usernameFragment: candidate.usernameFragment,
        }),
      },
      ackid: this.nextAckId(),
    });
  }

  async requestKeyframe(payload: KeyframeRequest): Promise<void> {
    this.sendJson({
      peer_msg: {
        from: this.peerId,
        to: this.remotePeerId,
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
    this.connectionGeneration += 1;
    this.clearHeartbeat();
    if (this.ws) {
      const socket = this.ws;
      this.ws = null;
      socket.close();
    }
    this.retainState("client-disconnect");
    console.log(`[Signaling] Client disconnect session=${streamDiagnosticId(this.sessionId)}`);
  }
}

function isTcpIceCandidate(candidate: string): boolean {
  const parts = candidate.trim().split(/\s+/);
  return parts[2]?.toLowerCase() === "tcp";
}
