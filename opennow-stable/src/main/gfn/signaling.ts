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
  GFN_CLIENT_STREAMER_WEBRTC,
  GFN_CLIENT_TYPE_NATIVE,
  GFN_CLIENT_VERSION,
  GFN_PLAY_ORIGIN,
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
  private remotePeerId: number | null = null;
  private peerName = `peer-${Math.floor(Math.random() * 10_000_000_000)}`;
  private ackCounter = 0;
  private heartbeatTimer: NodeJS.Timeout | null = null;
  private offerWatchdogTimer: NodeJS.Timeout | null = null;
  private openedAtMs = 0;
  private receivedMessageCount = 0;
  private sawAck = false;
  private sawHeartbeat = false;
  private sawPeerInfo = false;
  private sawOffer = false;
  private listeners = new Set<(event: MainToRendererSignalingEvent) => void>();
  private readonly verboseLogging = isGfnVerboseLoggingEnabled();

  constructor(
    private readonly signalingServer: string,
    private readonly sessionId: string,
    private readonly signalingUrl?: string,
    private readonly pairingId: string = sessionId,
    private readonly clientId?: string,
    private readonly deviceId?: string,
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

  private buildHandshakeHeaders(): Record<string, string> {
    return {
      ...buildGfnHeaders({
        origin: GFN_PLAY_ORIGIN,
      }),
      Host: this.signalingServer.includes(":") ? this.signalingServer : `${this.signalingServer}:443`,
    };
  }

  private redactProtocol(value: string): string {
    const [name, rest] = value.split(".", 2);
    if (!rest) {
      return value;
    }
    return `${name}.${rest.slice(0, 8)}${rest.length > 8 ? "…" : ""}`;
  }

  private summarizeHeaders(headers: Record<string, string>): string {
    return Object.entries(headers)
      .map(([name, value]) => {
        if (name === "User-Agent" || name === "Authorization") {
          return `${name}=set`;
        }
        if (name === "Host" || name === "Origin" || name === "Referer") {
          return `${name}=${value}`;
        }
        return `${name}=set`;
      })
      .join(", ");
  }

  private summarizeFrame(payload: Record<string, unknown>): string {
    const keys = Object.keys(payload);
    const parts = [
      `keys=${keys.join(",") || "none"}`,
      `ackid=${typeof payload.ackid === "number" ? payload.ackid : "n/a"}`,
      `ack=${typeof payload.ack === "number" ? payload.ack : "n/a"}`,
      `hb=${typeof payload.hb === "number" ? payload.hb : "n/a"}`,
      `peer_info=${payload.peer_info ? "yes" : "n"}`,
      `peer_msg=${payload.peer_msg ? "yes" : "n"}`,
    ];
    return parts.join(" ");
  }

  private clearOfferWatchdog(): void {
    if (this.offerWatchdogTimer) {
      clearTimeout(this.offerWatchdogTimer);
      this.offerWatchdogTimer = null;
    }
  }

  private startOfferWatchdog(): void {
    this.clearOfferWatchdog();
    const warnAfterMs = 12_000;
    this.offerWatchdogTimer = setTimeout(() => {
      if (this.sawOffer || !this.ws || this.ws.readyState !== WebSocket.OPEN) {
        return;
      }
      const elapsed = this.openedAtMs ? Math.max(0, Date.now() - this.openedAtMs) : warnAfterMs;
      console.warn(
        `[Signaling] No offer after ${Math.round(elapsed / 1000)}s ` +
          `messages=${this.receivedMessageCount} ack=${this.sawAck ? 1 : 0} hb=${this.sawHeartbeat ? 1 : 0} peer_info=${this.sawPeerInfo ? 1 : 0} remotePeer=${this.remotePeerId ?? "n/a"}`,
      );
    }, warnAfterMs);
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
    const headers = this.buildHandshakeHeaders();
    const parsedUrl = new URL(url);

    console.log(
      `[Signaling] Connecting host=${parsedUrl.host} protocol=${this.redactProtocol(protocol)} headers={${this.summarizeHeaders(headers)}} pairing=${this.pairingId.slice(0, 8)}…`,
    );

    await new Promise<void>((resolve, reject) => {
      const ws = new WebSocket(url, protocol, {
        rejectUnauthorized: false,
        headers,
      });

      this.ws = ws;

      ws.once("error", (error) => {
        this.emit({ type: "error", message: `Signaling connect failed: ${String(error)}` });
        reject(error);
      });

      ws.once("open", () => {
        this.openedAtMs = Date.now();
        this.receivedMessageCount = 0;
        this.sawAck = false;
        this.sawHeartbeat = false;
        this.sawPeerInfo = false;
        this.sawOffer = false;
        this.remotePeerId = null;
        console.log(`[Signaling] Socket open; sending peer_info and starting watchdog`);
        this.sendPeerInfo();
        this.setupHeartbeat();
        this.startOfferWatchdog();
        this.emit({ type: "connected" });
        resolve();
      });

      ws.on("message", (raw) => {
        const text = typeof raw === "string" ? raw : raw.toString("utf8");
        this.receivedMessageCount += 1;
        this.handleMessage(text);
      });

      ws.on("close", (code, reason) => {
        this.clearHeartbeat();
        this.clearOfferWatchdog();
        const reasonText = typeof reason === "string" ? reason : reason.toString("utf8");
        console.log(
          `[Signaling] Socket closed code=${code} reason=${reasonText || "n/a"} messages=${this.receivedMessageCount} sawOffer=${this.sawOffer ? 1 : 0}`,
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

    console.log(`[Signaling] Rx ${this.summarizeFrame(parsed as Record<string, unknown>)}`);

    if (typeof parsed.peer_info?.id === "number" && parsed.peer_info.id !== this.remotePeerId) {
      const prev = this.remotePeerId ?? "n/a";
      this.remotePeerId = parsed.peer_info.id;
      this.sawPeerInfo = true;
      console.log(`[Signaling] peer_info updated remote peer id ${prev} -> ${this.remotePeerId}`);
    } else if (parsed.peer_info) {
      this.sawPeerInfo = true;
    }

    if (typeof parsed.ackid === "number") {
      this.sawAck = true;
      this.sendJson({ ack: parsed.ackid });
    }

    if (parsed.hb) {
      this.sawHeartbeat = true;
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
      this.sawOffer = true;
      this.clearOfferWatchdog();
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
    this.clearOfferWatchdog();
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
  }
}
