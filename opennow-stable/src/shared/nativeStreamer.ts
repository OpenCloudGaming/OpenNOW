import type {
  IceCandidatePayload,
  NativeStreamerBackend,
  NativeRenderSurface,
  NativeStreamerSessionContext,
  NativeVideoBackendCapability,
  SendAnswerRequest,
} from "./gfn";

export const NATIVE_STREAMER_PROTOCOL_VERSION = 5;

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
  supportsVideoDecode: boolean;
  supportsVideoPresent: boolean;
  supportsAudioDecode?: boolean;
  supportsAudioOutput?: boolean;
  videoBackends?: NativeVideoBackendCapability[];
}

export interface NativeStreamerInputPacket {
  payloadBase64: string;
  partiallyReliable?: boolean;
}

export interface NativeStreamerActiveTransportCapabilities {
  supportsOfferAnswer: boolean;
  supportsRemoteIce: boolean;
  supportsLocalIce: boolean;
  supportsInput: boolean;
  supportsAudioDecode: boolean;
  supportsAudioOutput: boolean;
  supportsOwnedNvstNegotiation?: boolean;
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
      type: "nvst-bind";
    }
  | {
      id: string;
      type: "nvst-unbind";
    }
  | {
      id: string;
      type: "nvst-send";
      host: string;
      port: number;
      payloadBase64: string;
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
      type: "input-paused";
      paused: boolean;
    }
  | {
      id: string;
      type: "surface";
      surface: NativeRenderSurface;
    }
  | {
      id: string;
      type: "stop";
      reason?: string;
    }
  | {
      id: string;
      type: "shutdown";
      reason?: string;
    };

export type NativeStreamerResponse =
  | {
      id: string;
      type: "ready";
      capabilities: NativeStreamerCapabilities;
      processId?: number;
    }
  | {
      id: string;
      type: "ok";
      transport?: "webrtc" | "nvst";
      capabilities?: NativeStreamerActiveTransportCapabilities;
    }
  | {
      id: string;
      type: "nvst-bound";
      port: number;
      mjolnirPort?: number;
      localAddress?: string;
      iceUsernameFragment?: string;
      icePassword?: string;
      dtlsFingerprint?: string;
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
      status: "starting" | "ready" | "streaming" | "paused" | "stopped";
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
      type: "nvst-transport-ready";
      phase: "dtls" | "sctp";
    }
  | {
      type: "input-unavailable";
      reason: string;
    }
  | {
      type: "error";
      code?: string;
      message: string;
    };

export type NativeStreamerMessage = NativeStreamerResponse | NativeStreamerEvent;
