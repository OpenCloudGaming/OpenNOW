import type {
  IceCandidatePayload,
  NativeStreamerBackend,
  NativeStreamStats,
  NativeRenderSurface,
  NativeStreamerSessionContext,
  NativeVideoBackendCapability,
  SendAnswerRequest,
} from "./gfn";

export const NATIVE_STREAMER_PROTOCOL_VERSION = 1;

export type { NativeStreamerBackend };

export interface NativeStreamerCapabilities {
  protocolVersion: number;
  backend: NativeStreamerBackend;
  requestedBackend?: string;
  fallbackReason?: string;
  supportsOfferAnswer: boolean;
  supportsRemoteIce: boolean;
  supportsLocalIce: boolean;
  supportsInput: boolean;
  videoBackends?: NativeVideoBackendCapability[];
}

export interface NativeStreamerInputPacket {
  payloadBase64: string;
  partiallyReliable?: boolean;
}

export type NativeStreamerCommand =
  | {
      id: string;
      type: "hello";
      protocolVersion: number;
    }
  | {
      id: string;
      type: "start";
      context: NativeStreamerSessionContext;
    }
  | {
      id: string;
      type: "offer";
      sdp: string;
      context: NativeStreamerSessionContext;
    }
  | {
      id: string;
      type: "remote-ice";
      candidate: IceCandidatePayload;
    }
  | {
      id: string;
      type: "input";
      input: NativeStreamerInputPacket;
    }
  | {
      id: string;
      type: "surface";
      surface: NativeRenderSurface;
    }
  | {
      id: string;
      type: "bitrate";
      maxBitrateKbps: number;
    }
  | {
      id: string;
      type: "stop";
      reason?: string;
    };

export type NativeStreamerResponse =
  | {
      id: string;
      type: "ready";
      capabilities: NativeStreamerCapabilities;
    }
  | {
      id: string;
      type: "ok";
    }
  | {
      id: string;
      type: "answer";
      answer: SendAnswerRequest;
    }
  | {
      id: string;
      type: "error";
      code?: string;
      message: string;
    };

export type NativeStreamerEvent =
  | {
      type: "log";
      level: "debug" | "info" | "warn" | "error";
      message: string;
    }
  | {
      type: "status";
      status: "starting" | "ready" | "streaming" | "stopped";
      message?: string;
    }
  | {
      type: "local-ice";
      candidate: IceCandidatePayload;
    }
  | {
      type: "input-ready";
      protocolVersion: number;
    }
  | {
      type: "video-stall";
      stallMs: number;
      encodedKbps?: number;
      decodedFps: number;
      sinkFps: number;
      encodedAgeMs?: number;
      decodedAgeMs?: number;
      sinkAgeMs?: number;
      likelyStage?: string;
      sinkRendered?: number;
      sinkDropped?: number;
      memoryMode?: string;
      zeroCopy?: boolean;
      zeroCopyD3D11: boolean;
      zeroCopyD3D12: boolean;
      recoveryAttempt: number;
    }
  | {
      type: "stats";
      stats: NativeStreamStats;
    }
  | {
      type: "error";
      code?: string;
      message: string;
    };

export type NativeStreamerMessage = NativeStreamerResponse | NativeStreamerEvent;
