import type { SessionInfo, StreamSettings } from "./session";

export interface SignalingConnectRequest {
  sessionId: string;
  signalingServer: string;
  signalingUrl?: string;
  nativeStreamer?: NativeStreamerSessionContext;
}

export interface IceCandidatePayload {
  candidate: string;
  sdpMid?: string | null;
  sdpMLineIndex?: number | null;
  usernameFragment?: string | null;
}

export interface SendAnswerRequest {
  sdp: string;
  nvstSdp?: string;
}

export type NativeStreamerShortcutAction =
  | "toggleStats"
  | "togglePointerLock"
  | "toggleFullscreen"
  | "stopStream"
  | "toggleAntiAfk"
  | "toggleMicrophone"
  | "screenshot"
  | "toggleRecording";

export interface StreamShortcutInterceptionGate {
  streamActive: boolean;
  shortcutCaptureActive: boolean;
}

export interface NativeStreamerShortcutBindings {
  toggleStats: string;
  togglePointerLock: string;
  toggleFullscreen: string;
  stopStream: string;
  toggleAntiAfk: string;
  toggleMicrophone: string;
  screenshot: string;
  toggleRecording: string;
}

export interface NativeStreamerSessionContext {
  session: SessionInfo;
  settings: StreamSettings;
  shortcuts: NativeStreamerShortcutBindings;
  nvstVideo?: NvstVideoSession;
}

export type NvstSrtpProfile =
  | "AEAD_AES_128_GCM"
  | "AEAD_AES_256_GCM"
  | "AEAD_AES_128_GCM_8"
  | "AEAD_AES_256_GCM_8"
  | "AES_CM_128_HMAC_SHA1_32"
  | "AES_CM_128_HMAC_SHA1_80"
  | "AES_CM_256_HMAC_SHA1_32"
  | "AES_CM_256_HMAC_SHA1_80";

export interface NvstVideoSession {
  clientUdpPort: number;
  /** Negotiated NVST packet payload size. FEC covers this plus the RTP header allowance. */
  packetSize?: number;
  /**
   * Dedicated NATT-only video (Mjolnir) socket port reserved by the native
   * streamer. When present, the native streamer reads raw-SRTP video from this
   * socket while the ICE/DTLS bundle socket carries control/audio.
   */
  mjolnirUdpPort?: number;
  videoPeerIp: string;
  videoPeerPort: number;
  /** Routable CloudMatch endpoint for the ICE/DTLS control and audio bundle. */
  bundlePeerIp?: string;
  bundlePeerPort?: number;
  srtpAesKeyHex: string;
  srtpKeyId: number;
  srtpSaltHex: string;
  srtpProfile?: NvstSrtpProfile;
  pingPayload?: string;
  pingVersion?: number;
  localIceUsernameFragment?: string;
  localIcePassword?: string;
  remoteIceUsernameFragment?: string;
  remoteIcePassword?: string;
  /** SHA-256 colon hex from the local WebRtcTransport-equivalent cert. */
  localDtlsFingerprint?: string;
  /** SHA-256 colon hex advertised by DESCRIBE (`dtlsFingerprint` / `V2`). */
  remoteDtlsFingerprint?: string;
  /** True when DESCRIBE assigns all RTCP feedback to the `rtcp1` SCTP channel. */
  rtcpOnSctp?: boolean;
  codec?: string;
  audioTrack?: NvstAudioTrack;
  /** Idle receive timeout. Handshake needs longer than the 5s media default. */
  timeoutMs?: number;
}

export interface NvstAudioTrack {
  payloadType: number;
  codec: "opus";
  clockRateHz: number;
  channels: number;
  mid?: string;
  ssrc?: number;
}

export function buildNativeStreamerSessionContext(
  session: SessionInfo,
  settings: StreamSettings,
  shortcuts: NativeStreamerShortcutBindings,
  nvstVideo?: NvstVideoSession,
): NativeStreamerSessionContext {
  // CloudMatch does not consistently echo the chosen codec in its finalized
  // profile. Prefer an explicit server value, but otherwise preserve the
  // concrete codec selected by renderer capability resolution.
  const codec = session.negotiatedStreamProfile?.codec ?? settings.codec;
  const negotiatedStreamProfile = session.negotiatedStreamProfile
    ? {
      ...session.negotiatedStreamProfile,
      codec,
    }
    : { codec };

  return {
    session: {
      ...session,
      negotiatedStreamProfile,
    },
    settings: {
      ...settings,
      codec,
      enableCloudGsync:
        session.negotiatedStreamProfile?.enableCloudGsync ?? settings.enableCloudGsync,
    },
    shortcuts,
    ...(nvstVideo ? { nvstVideo } : {}),
  };
}

export interface NativeInputPacket {
  payload: ArrayBuffer | Uint8Array | number[];
  partiallyReliable?: boolean;
}

export interface NativeRenderSurfaceRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface NativeRenderSurfaceUpdate {
  rect: NativeRenderSurfaceRect | null;
  visible: boolean;
  deviceScaleFactor: number;
  showStats?: boolean;
}

export interface NativeRenderSurface extends NativeRenderSurfaceUpdate {
  windowHandle?: string;
  screenRect?: NativeRenderSurfaceRect;
}

export interface KeyframeRequest {
  reason: string;
  backlogFrames: number;
  attempt: number;
}

export type MainToRendererSignalingEvent =
  | { type: "connected" }
  | { type: "disconnected"; reason: string }
  | { type: "offer"; sdp: string }
  | { type: "remote-ice"; candidate: IceCandidatePayload }
  | { type: "native-stream-started"; message?: string }
  | { type: "native-stream-stopped"; reason?: string }
  | {
      type: "native-input-ready";
      protocolVersion: number;
      inputOwner?: "electron" | "native";
    }
  | { type: "native-input-unavailable"; reason: string }
  | { type: "error"; message: string }
  | { type: "log"; message: string };
