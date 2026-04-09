import { randomBytes } from "node:crypto";

import WebSocket from "ws";

import type {
  IceCandidatePayload,
  KeyframeRequest,
  MainToRendererSignalingEvent,
  SendAnswerRequest,
  SignalingDisconnectInfo,
  SignalingEstablishedRequest,
  SignalingSessionPhase,
} from "@shared/gfn";

const USER_AGENT =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/131.0.0.0 Safari/537.36";
const RECONNECT_DELAY_MS = 1000;
const MAX_RECONNECT_ATTEMPTS = 3;
const HOST_PEER_ID = 1;
const CLIENT_PEER_ID = 2;

type PeerPayload = Record<string, unknown>;

type OutboundEnvelope = {
  ackId: number;
  body: SignalingMessage;
  replayable: boolean;
  label: string;
};

interface SignalingMessage {
  ackid?: number;
  ack?: number;
  hb?: number;
  peer_info?: {
    browser?: string;
    browserVersion?: string;
    connected?: boolean;
    id: number;
    name?: string;
    peerRole?: number;
    resolution?: string;
    version?: number;
  };
  peer_msg?: {
    from: number;
    to: number;
    msg: string;
  };
}

export class GfnSignalingClient {
  private ws: WebSocket | null = null;
  private readonly peerId = CLIENT_PEER_ID;
  private readonly peerName = `peer-${Math.floor(Math.random() * 10_000_000_000)}`;
  private readonly listeners = new Set<(event: MainToRendererSignalingEvent) => void>();

  private isDisconnecting = false;
  private socketGeneration = 0;
  private nextOutboundAckId = 0;
  private maxReceivedAckId = 0;
  private sessionPhase: SignalingSessionPhase = "sign-in";
  private hasEverOpened = false;
  private reconnectAttempt = 0;
  private connectPromise: Promise<void> | null = null;
  private connectResolve: (() => void) | null = null;
  private connectReject: ((error: Error) => void) | null = null;
  private reconnectTimer: NodeJS.Timeout | null = null;
  private lastMessageAt = 0;
  private queuedOutbound: OutboundEnvelope[] = [];
  private unackedOutbound = new Map<number, OutboundEnvelope>();
  private establishmentReason: SignalingEstablishedRequest["reason"] | null = null;

  constructor(
    private readonly signalingServer: string,
    private readonly sessionId: string,
    private readonly signalingUrl?: string,
  ) {}

  onEvent(listener: (event: MainToRendererSignalingEvent) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private emit(event: MainToRendererSignalingEvent): void {
    for (const listener of this.listeners) {
      listener(event);
    }
  }

  private log(message: string): void {
    console.log(`[Signaling] ${message}`);
    this.emit({ type: "log", message });
  }

  private resolveConnect(): void {
    this.connectResolve?.();
    this.connectResolve = null;
    this.connectReject = null;
    this.connectPromise = null;
  }

  private rejectConnect(error: Error): void {
    this.connectReject?.(error);
    this.connectResolve = null;
    this.connectReject = null;
    this.connectPromise = null;
  }

  private nextAckId(): number {
    this.nextOutboundAckId += 1;
    return this.nextOutboundAckId;
  }

  private buildSocketUrl(isReconnect: boolean): string {
    const fallbackHost = this.signalingServer.includes(":")
      ? this.signalingServer
      : `${this.signalingServer}:443`;
    const baseUrl = this.signalingUrl?.trim() || `wss://${fallbackHost}/nvst/`;
    const signInUrl = new URL(baseUrl);

    signInUrl.protocol = "wss:";
    signInUrl.pathname = `${signInUrl.pathname.replace(/\/?$/, "/")}sign_in`;
    signInUrl.search = "";
    signInUrl.searchParams.set("peer_id", this.peerName);
    signInUrl.searchParams.set("version", "2");
    if (isReconnect) {
      signInUrl.searchParams.set("reconnect", "1");
    }
    return signInUrl.toString();
  }

  private getSocketOptions(url: string): ConstructorParameters<typeof WebSocket>[2] {
    const urlHost = url.replace(/^wss?:\/\//, "").split("/")[0];
    return {
      rejectUnauthorized: false,
      headers: {
        Host: urlHost,
        Origin: "https://play.geforcenow.com",
        "User-Agent": USER_AGENT,
        "Sec-WebSocket-Key": randomBytes(16).toString("base64"),
      },
    };
  }

  private clearHeartbeat(): void {
    return;
  }

  private clearReconnectTimer(): void {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
  }

  private describeState(): string {
    return `gen=${this.socketGeneration} attempt=${this.reconnectAttempt} phase=${this.sessionPhase} establishedBy=${this.establishmentReason ?? "none"} maxInAck=${this.maxReceivedAckId} lastOutAck=${this.nextOutboundAckId} queued=${this.queuedOutbound.length} unacked=${this.unackedOutbound.size}`;
  }

  private describeOutboundList(items: OutboundEnvelope[]): string {
    if (items.length === 0) {
      return "none";
    }
    return items.map((item) => `${item.label}#${item.ackId}`).join(", ");
  }

  private sendImmediate(payload: SignalingMessage): boolean {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      return false;
    }
    this.ws.send(JSON.stringify(payload));
    return true;
  }

  private shouldQueueWhenOffline(): boolean {
    return !this.hasEverOpened || this.sessionPhase === "sign-in" || this.reconnectTimer !== null;
  }

  private flushQueuedOutbound(skipAckIdsInReplayStore = false): void {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN || this.queuedOutbound.length === 0) {
      return;
    }

    const queued = [...this.queuedOutbound];
    this.queuedOutbound = [];
    const flushed: OutboundEnvelope[] = [];
    for (const item of queued) {
      if (skipAckIdsInReplayStore && this.unackedOutbound.has(item.ackId)) {
        continue;
      }
      this.sendEnvelope(item, false);
      flushed.push(item);
    }
    if (flushed.length > 0) {
      this.log(`Flushed queued outbound: ${this.describeOutboundList(flushed)} ${this.describeState()}`);
    }
  }

  private replayUnackedOutbound(): number {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN || this.unackedOutbound.size === 0) {
      return 0;
    }

    const replayList = [...this.unackedOutbound.values()].sort((a, b) => a.ackId - b.ackId);
    for (const item of replayList) {
      this.sendEnvelope(item, false);
    }
    this.log(`Replayed unacked outbound: ${this.describeOutboundList(replayList)} ${this.describeState()}`);
    return replayList.length;
  }

  private sendEnvelope(envelope: OutboundEnvelope, allowQueue: boolean): void {
    const sent = this.sendImmediate(envelope.body);
    if (sent) {
      if (envelope.replayable) {
        this.unackedOutbound.set(envelope.ackId, envelope);
      }
      this.log(`Sent outbound ${envelope.label} ackid=${envelope.ackId} replayable=${envelope.replayable} ${this.describeState()}`);
      return;
    }

    if (!allowQueue) {
      return;
    }

    if (!this.shouldQueueWhenOffline()) {
      this.log(`Dropping outbound ${envelope.label} ackid=${envelope.ackId} because signaling is closed post-establishment ${this.describeState()}`);
      return;
    }

    this.queuedOutbound.push(envelope);
    if (envelope.replayable) {
      this.unackedOutbound.set(envelope.ackId, envelope);
    }
    this.log(
      `Queued outbound ${envelope.label} ackid=${envelope.ackId} ${this.describeState()}`,
    );
  }

  private sendTrackedMessage(body: Omit<SignalingMessage, "ackid">, label: string, replayable = true): void {
    const ackId = this.nextAckId();
    this.sendEnvelope(
      {
        ackId,
        body: { ...body, ackid: ackId },
        replayable,
        label,
      },
      true,
    );
  }

  private sendAck(ackId: number): void {
    const sent = this.sendImmediate({ ack: ackId });
    this.log(`Emitted ack=${ackId} sent=${sent} maxReceivedAckId=${this.maxReceivedAckId} ${this.describeState()}`);
  }

  private ackOutbound(ackId: number): void {
    const ackedIds = [...this.unackedOutbound.keys()].filter((candidate) => candidate <= ackId);
    for (const candidate of ackedIds) {
      this.unackedOutbound.delete(candidate);
      this.queuedOutbound = this.queuedOutbound.filter((item) => item.ackId !== candidate);
    }
    if (ackedIds.length > 0) {
      this.log(`Acked outbound <=${ackId} removed=${ackedIds.length} ${this.describeState()}`);
    }
  }

  private sendPeerInfo(): void {
    this.sendTrackedMessage(
      {
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
      },
      "peer_info",
      true,
    );
  }

  async connect(): Promise<void> {
    if (this.connectPromise) {
      return this.connectPromise;
    }
    if (this.ws && (this.ws.readyState === WebSocket.OPEN || this.ws.readyState === WebSocket.CONNECTING)) {
      return;
    }

    this.isDisconnecting = false;
    this.clearReconnectTimer();

    this.connectPromise = new Promise<void>((resolve, reject) => {
      this.connectResolve = resolve;
      this.connectReject = reject;
    });

    this.openSocket(false, false);
    return this.connectPromise;
  }

  private openSocket(isReconnect: boolean, resolveAsReconnect: boolean): void {
    const generation = this.socketGeneration + 1;
    this.socketGeneration = generation;
    const url = this.buildSocketUrl(isReconnect);
    const protocol = `x-nv-sessionid.${this.sessionId}`;

    this.log(`Opening socket ${isReconnect ? "reconnect" : "initial"} url=${url} ${this.describeState()}`);

    const ws = new WebSocket(url, protocol, this.getSocketOptions(url));
    this.ws = ws;

    let handshakeSettled = false;
    const settleConnect = (): void => {
      if (handshakeSettled) {
        return;
      }
      handshakeSettled = true;
      if (resolveAsReconnect) {
        return;
      }
      this.resolveConnect();
    };

    const failConnect = (error: Error): void => {
      if (handshakeSettled) {
        return;
      }
      handshakeSettled = true;
      if (resolveAsReconnect) {
        return;
      }
      this.rejectConnect(error);
    };

    const isCurrentSocket = (): boolean => this.ws === ws && this.socketGeneration === generation;

    let socketErrored = false;

    ws.on("open", () => {
      if (!isCurrentSocket()) {
        ws.close();
        return;
      }

      this.hasEverOpened = true;
      this.lastMessageAt = Date.now();
      this.log(`Proactive signaling heartbeat disabled; waiting for inbound traffic ${this.describeState()}`);

      let replayedCount = 0;
      if (isReconnect) {
        replayedCount = this.replayUnackedOutbound();
      }
      this.flushQueuedOutbound(isReconnect);
      if (!isReconnect) {
        this.sendPeerInfo();
      }

      this.log(
        `Socket open gen=${generation} reconnect=${isReconnect} replayed=${replayedCount} queuedFlushed=${this.queuedOutbound.length === 0} pendingReplay=${this.describeOutboundList([...this.unackedOutbound.values()].sort((a, b) => a.ackId - b.ackId))} ${this.describeState()}`,
      );

      if (isReconnect) {
        this.emit({
          type: "reconnected",
          socketGeneration: generation,
          attempt: this.reconnectAttempt,
          replayedCount,
          sessionPhase: this.sessionPhase,
        });
      } else {
        this.emit({ type: "connected", socketGeneration: generation, sessionPhase: this.sessionPhase });
      }

      settleConnect();
    });

    ws.on("error", (error) => {
      if (!isCurrentSocket()) {
        return;
      }
      socketErrored = true;
      this.log(`Socket error gen=${generation} reconnect=${isReconnect} ${String(error)} ${this.describeState()}`);
      this.emit({ type: "error", message: `Signaling socket error: ${String(error)}` });
      failConnect(error instanceof Error ? error : new Error(String(error)));
    });

    ws.on("message", (raw) => {
      if (!isCurrentSocket()) {
        return;
      }
      const text = typeof raw === "string" ? raw : raw.toString("utf8");
      this.lastMessageAt = Date.now();
      this.handleMessage(text);
    });

    ws.on("close", (code, reason) => {
      const reasonText = typeof reason === "string" ? reason : reason.toString("utf8");
      const detail = this.buildDisconnectInfo(code, reasonText, ws, generation, socketErrored);
      this.clearHeartbeat();
      if (isCurrentSocket()) {
        this.ws = null;
      }

      this.log(
        `Socket close gen=${generation} code=${detail.code} clean=${detail.wasClean} willRetry=${detail.willRetry} reason=${detail.reason || "<empty>"} ${this.describeState()}`,
      );

      if (this.isDisconnecting) {
        settleConnect();
        return;
      }

      failConnect(new Error(`Signaling closed before ready: code=${detail.code} reason=${detail.reason || "socket closed"}`));
      this.emit({ type: "disconnected", detail });

      if (detail.willRetry) {
        this.scheduleReconnect();
      } else if (this.hasEverOpened) {
        this.log(`Post-open signaling close will not reconnect; clearing queued signaling state ${this.describeState()}`);
        this.queuedOutbound = [];
        this.unackedOutbound.clear();
      }
    });
  }

  private buildDisconnectInfo(
    code: number,
    reason: string,
    ws: WebSocket,
    generation: number,
    socketErrored: boolean,
  ): SignalingDisconnectInfo {
    const normalizedCode = Number.isFinite(code) && code > 0 ? code : 1005;
    const frameReceived = Boolean((ws as WebSocket & { _closeFrameReceived?: boolean })._closeFrameReceived);
    const frameSent = Boolean((ws as WebSocket & { _closeFrameSent?: boolean })._closeFrameSent);
    const wasClean = frameReceived && frameSent && !socketErrored;
    const error = socketErrored || (normalizedCode >= 1002 && normalizedCode <= 1015);
    const willRetry =
      !this.isDisconnecting
      && this.sessionPhase === "sign-in"
      && this.reconnectAttempt < MAX_RECONNECT_ATTEMPTS
      && (error || normalizedCode === 1006 || normalizedCode === 1001);

    return {
      code: normalizedCode,
      reason,
      wasClean,
      error,
      attempt: this.reconnectAttempt,
      willRetry,
      socketGeneration: generation,
      sessionPhase: this.sessionPhase,
      lastInboundAckId: this.maxReceivedAckId,
      lastOutboundAckId: this.nextOutboundAckId,
    };
  }

  private scheduleReconnect(): void {
    this.clearReconnectTimer();
    this.reconnectAttempt += 1;
    const replayCount = this.unackedOutbound.size;
    this.emit({
      type: "reconnecting",
      socketGeneration: this.socketGeneration,
      attempt: this.reconnectAttempt,
      queuedReplayCount: replayCount,
      sessionPhase: this.sessionPhase,
    });
    this.log(
      `Scheduling reconnect attempt=${this.reconnectAttempt} replay=${replayCount} replayItems=${this.describeOutboundList([...this.unackedOutbound.values()].sort((a, b) => a.ackId - b.ackId))} ${this.describeState()}`,
    );
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      if (this.isDisconnecting) {
        return;
      }
      this.openSocket(true, true);
    }, RECONNECT_DELAY_MS);
  }

  markEstablished(reason: SignalingEstablishedRequest["reason"]): void {
    if (this.sessionPhase === "established") {
      if (this.establishmentReason === null) {
        this.establishmentReason = reason;
      }
      this.log(`Signaling establishment reaffirmed by ${reason} ${this.describeState()}`);
      return;
    }

    this.sessionPhase = "established";
    this.establishmentReason = reason;
    this.clearReconnectTimer();
    this.log(`Signaling session marked established by ${reason} ${this.describeState()}`);
  }

  private handleMessage(text: string): void {
    let parsed: SignalingMessage;
    try {
      parsed = JSON.parse(text) as SignalingMessage;
    } catch {
      this.log(`Ignoring non-JSON signaling packet: ${text.slice(0, 120)}`);
      return;
    }

    if (typeof parsed.hb === "number") {
      this.log(`Inbound signaling heartbeat ignored ${this.describeState()}`);
      return;
    }

    if (typeof parsed.ack === "number") {
      this.ackOutbound(parsed.ack);
    }

    let isDuplicateInboundAckId = false;
    if (typeof parsed.ackid === "number") {
      isDuplicateInboundAckId = parsed.ackid <= this.maxReceivedAckId;
      const previousMaxReceivedAckId = this.maxReceivedAckId;
      this.maxReceivedAckId = Math.max(this.maxReceivedAckId, parsed.ackid);
      this.log(
        `Inbound ackid=${parsed.ackid} maxReceivedAckId=${this.maxReceivedAckId} duplicate=${isDuplicateInboundAckId} prevMax=${previousMaxReceivedAckId} ${this.describeState()}`,
      );
      this.sendAck(this.maxReceivedAckId);
      if (isDuplicateInboundAckId) {
        this.log(`Ignoring duplicate inbound ackid=${parsed.ackid} ${this.describeState()}`);
      }
    }

    if (isDuplicateInboundAckId) {
      return;
    }

    if (!parsed.peer_msg?.msg) {
      return;
    }

    let peerPayload: PeerPayload;
    try {
      peerPayload = JSON.parse(parsed.peer_msg.msg) as PeerPayload;
    } catch {
      this.log("Received non-JSON peer payload");
      return;
    }

    if (peerPayload.type === "offer" && typeof peerPayload.sdp === "string") {
      this.log(`Received offer sdpLen=${peerPayload.sdp.length} ${this.describeState()}`);
      this.emit({ type: "offer", sdp: peerPayload.sdp });
      return;
    }

    if (typeof peerPayload.candidate === "string") {
      this.log(`Received remote ICE candidate ${peerPayload.candidate}`);
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
          usernameFragment:
            typeof peerPayload.usernameFragment === "string" || peerPayload.usernameFragment === null
              ? peerPayload.usernameFragment
              : undefined,
        },
      });
      return;
    }

    this.log(`Unhandled peer message keys=${Object.keys(peerPayload).join(",")}`);
  }

  async sendAnswer(payload: SendAnswerRequest): Promise<void> {
    this.log(`Sending answer sdpLen=${payload.sdp.length} nvstLen=${payload.nvstSdp?.length ?? 0}`);
    this.sendTrackedMessage(
      {
        peer_msg: {
          from: this.peerId,
          to: HOST_PEER_ID,
          msg: JSON.stringify({
            type: "answer",
            sdp: payload.sdp,
            ...(payload.nvstSdp ? { nvstSdp: payload.nvstSdp } : {}),
          }),
        },
      },
      "answer",
      true,
    );
  }

  async sendIceCandidate(candidate: IceCandidatePayload): Promise<void> {
    this.log(`Sending local ICE candidate ${candidate.candidate} sdpMid=${candidate.sdpMid ?? "?"}`);
    this.sendTrackedMessage(
      {
        peer_msg: {
          from: this.peerId,
          to: HOST_PEER_ID,
          msg: JSON.stringify({
            candidate: candidate.candidate,
            sdpMid: candidate.sdpMid,
            sdpMLineIndex: candidate.sdpMLineIndex,
            usernameFragment: candidate.usernameFragment,
          }),
        },
      },
      "ice_candidate",
      true,
    );
  }

  async requestKeyframe(payload: KeyframeRequest): Promise<void> {
    this.sendTrackedMessage(
      {
        peer_msg: {
          from: this.peerId,
          to: HOST_PEER_ID,
          msg: JSON.stringify({
            type: "request_keyframe",
            reason: payload.reason,
            backlogFrames: payload.backlogFrames,
            attempt: payload.attempt,
          }),
        },
      },
      "request_keyframe",
      true,
    );
    this.log(
      `Sent keyframe request reason=${payload.reason} backlog=${payload.backlogFrames} attempt=${payload.attempt}`,
    );
  }

  disconnect(): void {
    this.isDisconnecting = true;
    this.sessionPhase = "sign-in";
    this.reconnectAttempt = 0;
    this.clearHeartbeat();
    this.clearReconnectTimer();
    this.rejectConnect(new Error("Signaling disconnected by client"));
    if (this.ws) {
      this.ws.close(1000, "client disconnect");
      this.ws = null;
    }
    this.queuedOutbound = [];
    this.unackedOutbound.clear();
    this.maxReceivedAckId = 0;
    this.nextOutboundAckId = 0;
    this.establishmentReason = null;
  }
}
