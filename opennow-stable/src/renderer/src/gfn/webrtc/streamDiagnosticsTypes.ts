import type { MicState } from "../microphoneManager";

export interface StreamDiagnostics {
  connectionState: RTCPeerConnectionState | "closed";
  inputReady: boolean;
  connectedGamepads: number;

  resolution: string;
  codec: string;
  isHdr: boolean;
  bitrateKbps: number;
  decodeFps: number;
  renderFps: number;

  packetsLost: number;
  packetsReceived: number;
  packetLossPercent: number;
  jitterMs: number;
  rttMs: number;

  framesReceived: number;
  framesDecoded: number;
  framesDropped: number;

  decodeTimeMs: number;
  renderTimeMs: number;
  jitterBufferDelayMs: number;

  inputQueueBufferedBytes: number;
  inputQueuePeakBufferedBytes: number;
  partiallyReliableInputQueueBufferedBytes: number;
  partiallyReliableInputQueuePeakBufferedBytes: number;
  inputQueueDropCount: number;
  inputQueueMaxSchedulingDelayMs: number;
  partiallyReliableInputOpen: boolean;
  mouseMoveTransport: "reliable" | "partially_reliable";
  mouseFlushIntervalMs: number;
  mousePacketsPerSecond: number;
  mouseResidualMagnitude: number;
  mouseAdaptiveFlushActive: boolean;

  lagReason: StreamLagReason;
  lagReasonDetail: string;

  gpuType: string;
  serverRegion: string;

  decoderPressureActive: boolean;
  decoderRecoveryAttempts: number;
  decoderRecoveryAction: string;

  micState: MicState;
  micEnabled: boolean;
}

export type StreamLagReason =
  | "unknown"
  | "stable"
  | "network"
  | "decoder"
  | "input_backpressure"
  | "render";

export interface StreamTimeWarning {
  code: 1 | 2 | 3;
  secondsLeft?: number;
}
