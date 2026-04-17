import type {
  IceCandidatePayload,
  KeyframeRequest,
  MainToRendererSignalingEvent,
  SendAnswerRequest,
  SignalingConnectRequest,
} from "@shared/gfn";

const USER_AGENT = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/131.0.0.0 Safari/537.36";

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

function randomKey(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  let binary = "";
  for (const value of bytes) binary += String.fromCharCode(value);
  return btoa(binary);
}

export class BrowserSignalingClient {
  private ws: WebSocket | null = null;
  private peerId = 2;
  private peerName = `peer-${Math.floor(Math.random() * 10_000_000_000)}`;
  private ackCounter = 0;
  private heartbeatTimer: number | null = null;
  private listeners = new Set<(event: MainToRendererSignalingEvent) => void>();
  private generation = 0;
  private pendingConnect: {
    ws: WebSocket;
    generation: number;
    settled: boolean;
    resolve: () => void;
    reject: (error: Error) => void;
  } | null = null;

  onEvent(listener: (event: MainToRendererSignalingEvent) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private emit(event: MainToRendererSignalingEvent): void {
    for (const listener of this.listeners) listener(event);
  }

  private nextAckId(): number {
    this.ackCounter += 1;
    return this.ackCounter;
  }

  private clearHeartbeat(): void {
    if (this.heartbeatTimer !== null) {
      window.clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
  }

  private settlePendingConnect(ws: WebSocket, generation: number, error?: Error): void {
    const pending = this.pendingConnect;
    if (!pending || pending.ws !== ws || pending.generation !== generation || pending.settled) {
      return;
    }
    pending.settled = true;
    this.pendingConnect = null;
    if (error) {
      pending.reject(error);
      return;
    }
    pending.resolve();
  }

  private buildSignInUrl(input: SignalingConnectRequest): string {
    const fallbackHost = input.signalingServer.includes(":")
      ? input.signalingServer
      : `${input.signalingServer}:443`;
    const baseUrl = input.signalingUrl?.trim() || `wss://${fallbackHost}/nvst/`;
    const signInUrl = new URL(baseUrl);
    signInUrl.protocol = "wss:";
    signInUrl.pathname = `${signInUrl.pathname.replace(/\/?$/, "/")}sign_in`;
    signInUrl.search = "";
    signInUrl.searchParams.set("peer_id", this.peerName);
    signInUrl.searchParams.set("version", "2");
    return signInUrl.toString();
  }

  async connect(input: SignalingConnectRequest): Promise<void> {
    if (this.ws?.readyState === WebSocket.OPEN) return;
    if (this.ws || this.pendingConnect) {
      this.disconnect("Signaling connect replaced");
    }
    const url = this.buildSignInUrl(input);
    const protocol = `x-nv-sessionid.${input.sessionId}`;
    const generation = ++this.generation;

    await new Promise<void>((resolve, reject) => {
      const ws = new WebSocket(url, protocol);
      this.ws = ws;
      this.pendingConnect = { ws, generation, settled: false, resolve, reject };

      const isCurrent = (): boolean => this.ws === ws && this.generation === generation;

      ws.onerror = () => {
        if (!isCurrent()) return;
        this.emit({ type: "error", message: "Signaling connect failed" });
        this.settlePendingConnect(ws, generation, new Error("Signaling connect failed"));
      };

      ws.onopen = () => {
        if (!isCurrent()) return;
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
            userAgent: USER_AGENT,
            secWebSocketKey: randomKey(),
          },
        });
        this.clearHeartbeat();
        this.heartbeatTimer = window.setInterval(() => {
          if (this.ws === ws && this.generation === generation) {
            this.sendJson({ hb: 1 });
          }
        }, 5000);
        this.emit({ type: "connected" });
        this.settlePendingConnect(ws, generation);
      };

      ws.onmessage = (event) => {
        if (!isCurrent()) return;
        this.handleMessage(typeof event.data === "string" ? event.data : "");
      };

      ws.onclose = (event) => {
        if (this.ws === ws) {
          this.clearHeartbeat();
          this.ws = null;
        }
        const reason = event.reason || "socket closed";
        if (isCurrent()) {
          this.settlePendingConnect(ws, generation, new Error(reason));
          this.emit({ type: "disconnected", reason });
          return;
        }
        this.settlePendingConnect(ws, generation, new Error(reason));
      };
    });
  }

  private sendJson(payload: unknown): void {
    if (this.ws?.readyState !== WebSocket.OPEN) return;
    this.ws.send(JSON.stringify(payload));
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
      if (shouldAck) this.sendJson({ ack: parsed.ackid });
    }

    if (parsed.hb) {
      this.sendJson({ hb: 1 });
      return;
    }

    if (!parsed.peer_msg?.msg) return;

    let payload: Record<string, unknown>;
    try {
      payload = JSON.parse(parsed.peer_msg.msg) as Record<string, unknown>;
    } catch {
      this.emit({ type: "log", message: "Received non-JSON peer payload" });
      return;
    }

    if (payload.type === "offer" && typeof payload.sdp === "string") {
      this.emit({ type: "offer", sdp: payload.sdp });
      return;
    }

    if (typeof payload.candidate === "string") {
      this.emit({
        type: "remote-ice",
        candidate: {
          candidate: payload.candidate,
          sdpMid: typeof payload.sdpMid === "string" || payload.sdpMid === null ? payload.sdpMid : undefined,
          sdpMLineIndex: typeof payload.sdpMLineIndex === "number" || payload.sdpMLineIndex === null ? payload.sdpMLineIndex : undefined,
          usernameFragment:
            typeof payload.usernameFragment === "string" || payload.usernameFragment === null
              ? payload.usernameFragment
              : undefined,
        },
      });
    }
  }

  async sendAnswer(payload: SendAnswerRequest): Promise<void> {
    this.sendJson({
      peer_msg: {
        from: this.peerId,
        to: 1,
        msg: JSON.stringify({
          type: "answer",
          sdp: payload.sdp,
          ...(payload.nvstSdp ? { nvstSdp: payload.nvstSdp } : {}),
        }),
      },
      ackid: this.nextAckId(),
    });
  }

  async sendIceCandidate(candidate: IceCandidatePayload): Promise<void> {
    this.sendJson({
      peer_msg: {
        from: this.peerId,
        to: 1,
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
  }

  disconnect(reason = "Signaling disconnected"): void {
    const ws = this.ws;
    this.generation += 1;
    this.clearHeartbeat();
    if (this.pendingConnect && !this.pendingConnect.settled) {
      const pending = this.pendingConnect;
      pending.settled = true;
      this.pendingConnect = null;
      pending.reject(new Error(reason));
    }
    this.ws = null;
    if (ws) {
      ws.close();
      this.emit({ type: "disconnected", reason });
    }
  }
}
