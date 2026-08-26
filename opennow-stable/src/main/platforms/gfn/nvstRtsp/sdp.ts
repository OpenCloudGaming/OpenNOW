import { randomBytes } from "node:crypto";

import { clampNativeStreamFps, type NvstAudioTrack } from "@shared/gfn";
import { deriveSrtpSaltHex } from "./srtp";

/** Negotiated NVST video payload size used by both ANNOUNCE and native FEC reconstruction. */
export const NVST_VIDEO_PACKET_SIZE = 1280;

/** Official macOS ANNOUNCE baseline. */
const ANNOUNCE_ALLOWLIST = {
  video: {
    clientViewportWd: "1920",
    clientViewportHt: "1080",
    videoSplitEncodeStripsPerFrame: "64",
    updateSplitEncodeStateDynamically: "1",
    packetSize: String(NVST_VIDEO_PACKET_SIZE),
    enableRtpNack: "1",
    rtpNackQueueLength: "2048",
    rtpNackQueueMaxPackets: "1024",
    rtpNackMaxPacketCount: "64",
    "framePacing.mode": "1",
    "framePacing.feedbackMode": "1",
    "framePacing.pid.minTargetFrameTimeUs": "7936",
    "adaptiveQuantization.spatialAQSetting": "7",
    "adaptiveQuantization.temporalAQSetting": "0",
    "adaptiveQuantization.spatialAQStrength": "12",
    "adaptiveQuantization.qpThresholdAdjPercent": "2",
    "adaptiveQuantization.saqAdaptMinQpThresholdPercent": "40",
    "adaptiveQuantization.saqAdaptMaxQpThresholdPercent": "100",
    "adaptiveQuantization.saqAdaptDecayStrengthX100": "250",
    "adaptiveQuantization.perfAdjEnablement": "1",
    enableAv1RcPrecisionFactor: "1",
  },
  vqos: {
    bitStreamFormat: "0",
    // Match the native GameStream/Moonlight resilience baseline. At 120 FPS a 5% reserve is only
    // a handful of packets and cannot absorb normal Wi-Fi scheduling bursts; 20% keeps repair
    // local while NACK remains available for larger outages.
    "fec.enable": "1",
    "fec.rateDropWindow": "10",
    "fec.minRequiredFecPackets": "2",
    "fec.repairPercent": "20",
    "fec.repairMinPercent": "20",
    "fec.repairMaxPercent": "35",
    "bllFec.enable": "0",
    // Official native Linux NVST config enables all H.264/H.265 GRC modes.
    // Keep the user's bitrate as a ceiling while allowing the server to react
    // to the RTCP loss/jitter feedback emitted by the native receiver.
    "grc.enable": "7",
    "drc.enable": "0",
    "dfc.adjustResAndFps": "0",
    calculateAvgVideoStreamingBitrate: "1",
  },
  packetPacing: {
    version: "3",
    mode: "1",
    numGroups: "5",
    maxDelayUs: "2000",
    minNumPacketsFrame: "10",
    minNumPacketsPerGroup: "15",
    enableAccurateSleep: "1",
    enableSmoothTransition: "1",
    allowFpsBasedToggle: "1",
  },
  ri: {
    partialReliableThresholdMs: "300",
    timestampsEnabled: "1",
    useMultipleGamepads: "1",
    usePartiallyReliableUdpChannel: "0",
    enablePartiallyReliableTransferGamepad: "255",
    enablePartiallyReliableTransferHid: "-1",
  },
  aqos: {
    enableRedundancy: "0",
    redundancyLevel: "0",
  },
  bwe: {
    useOwdCongestionControl: "1",
  },
  general: {
    rtspWebSocketPerConnection: "1",
    "enetControlChannel.mtuSize": "1191",
    pingIntervalBeforeConnectionMs: "20",
    pingIntervalAfterConnectionMs: "100",
  },
  runtime: {
    audioSrtp: "0",
    micSrtp: "0",
    // Match the installed native client's cursor policy. Without these the
    // server accepts cursor_channel but does not publish local cursor shapes.
    mouseCursorCapture: "3",
    mimicRemoteCursor: "0",
  },
} as const;

function parseResolution(resolution: string | undefined): { width: number; height: number } {
  const match = /^(\d+)\s*[xX]\s*(\d+)$/.exec(resolution?.trim() ?? "");
  if (!match) {
    return { width: 1920, height: 1080 };
  }
  return { width: Number(match[1]), height: Number(match[2]) };
}

export function extractNvstSdpAttribute(sdp: string, name: string): string | null {
  const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = new RegExp(`^a=(?:x-nv-)?${escaped}:([^\\r\\n]*)$`, "mi").exec(sdp);
  const value = match?.[1]?.trim();
  return value ? value : null;
}

export function buildAnnounceSdp(
  options: {
    resolution?: string;
    fps?: number;
    codec?: string;
    /** User-selected bitrate ceiling and initial native encoder target. */
    maxBitrateKbps?: number;
    encryptionKeyHex?: string;
    encryptionKeyId?: number;
    iceCredentials?: { usernameFragment: string; password: string };
    /** Server video port from SETUP / DESCRIBE. Capture uses `m=video 5004`. */
    videoPort?: number;
    clientPorts?: {
      video: number;
      audio?: number;
      mic?: number;
      control?: number;
      bundle?: number;
      session?: number;
      /** Official `general.clientPorts.localAddress` — routable NIC IPv4. */
      localAddress?: string;
      /** Use Bifrost's reserved Mjolnir port layout (video 49005, bundle 49006). */
      useReserved?: boolean;
      /** Permit Bifrost's dynamic-port fallback when the reserved pair is unavailable. */
      fallbackDynamic?: boolean;
    };
    /** Official `general.clientBundlePort` — ICE/DTLS socket, distinct from clientPorts.bundle. */
    clientBundlePort?: number;
    /** Official `general.clientTransport` form is `ip:port`. */
    clientTransport?: string;
    nativeRtcOnBundlePort?: string;
    /** Official `general.rtc{Video,Audio,DataChannel}OnNativeBundle` when unified. */
    rtcOnNativeBundle?: boolean;
    rtcVideoOnNativeBundle?: boolean;
    rtcAudioOnNativeBundle?: boolean;
    rtcMicOnNativeBundle?: boolean;
    rtcDataChannelOnNativeBundle?: boolean;
    enableUnifiedSocket?: boolean;
    /**
     * Official `general.rtcpOnSctp` gates RTCP feedback onto the `rtcp1` SCTP data
     * channel. We do not bring that channel up, so advertise 0 to keep feedback on
     * plain SRTCP over the Mjolnir socket (which the native receiver already sends).
     */
    rtcpOnSctp?: boolean;
    /**
     * Official skips Nvsc V1 `iceUsernameFragment` / `iceUsernamePwd` /
     * `dtlsFingerprint` and keeps V2 plus WebRTC `a=ice-*`.
     */
    includeNvscLegacyIce?: boolean;
    includeNvscLegacyDtls?: boolean;
    /** SHA-256 colon hex (95 chars). Written as V1 + V2 to match Bifrost/mall. */
    dtlsFingerprint?: string;
  } = {},
): string {
  const { width, height } = parseResolution(options.resolution);
  const fps = options.fps && Number.isFinite(options.fps) && options.fps > 0
    ? clampNativeStreamFps(options.fps)
    : 60;
  const videoPort = options.videoPort && options.videoPort > 0 ? options.videoPort : 0;
  const maximumBitrateKbps = Math.max(
    1_000,
    Math.min(150_000, Math.round(options.maxBitrateKbps ?? 100_000)),
  );
  // Start at the selected ceiling. GRC may reduce the encoder output from this
  // value when the receiver reports congestion, while DRC remains disabled so
  // resolution and frame rate stay predictable.
  const initialBitrateKbps = maximumBitrateKbps;
  const codec = options.codec?.trim().toUpperCase() ?? "H264";
  const bitStreamFormat = codec === "AV1" ? "2" : codec === "H265" || codec === "HEVC" ? "1" : "0";

  const lines: string[] = [
    "v=0",
    // Official macOS handshake origin username is "unknown", not "android".
    "o=unknown 0 14 IN IPv4 127.0.0.1",
    "s=NVIDIA Streaming Client",
  ];

  const pushGroup = (
    prefix: string,
    indexed: boolean,
    values: Record<string, string>,
  ): void => {
    for (const [key, value] of Object.entries(values)) {
      let nextValue = value;
      if (prefix === "video" && key === "clientViewportWd") {
        nextValue = String(width);
      } else if (prefix === "video" && key === "clientViewportHt") {
        nextValue = String(height);
      }
      const name = indexed ? `x-nv-${prefix}[0].${key}` : `x-nv-${prefix}.${key}`;
      lines.push(`a=${name}:${nextValue}`);
    }
  };

  pushGroup("video", true, ANNOUNCE_ALLOWLIST.video);
  // CloudMatch provisions the requested profile, but NVST still requires the
  // client frame-rate ceiling in ANNOUNCE. Without it, the server defaults to
  // a 60 FPS video stream even when the negotiated session profile says 120.
  lines.push(`a=x-nv-video[0].maxFPS:${fps}`);
  lines.push(`a=x-nv-video[0].initialBitrateKbps:${initialBitrateKbps}`);
  lines.push(`a=x-nv-video[0].initialPeakBitrateKbps:${initialBitrateKbps}`);
  pushGroup("vqos", true, {
    ...ANNOUNCE_ALLOWLIST.vqos,
    bitStreamFormat,
  });
  if (bitStreamFormat !== "2") {
    lines.push(`a=x-nv-clientSupportHevc:${bitStreamFormat === "1" ? "1" : "0"}`);
  }
  // Match the installed Windows client's native NVST policy: bitrate remains
  // adaptive while dynamic resolution/framerate stay disabled. Omitting these
  // fields leaves the server near its low default rate even when the UI ceiling
  // is much higher.
  lines.push(`a=x-nv-vqos[0].bw.maximumBitrateKbps:${maximumBitrateKbps}`);
  lines.push("a=x-nv-vqos[0].bw.minimumBitrateKbps:1000");
  lines.push("a=x-nv-vqos[0].drc.bitrateIirFilterFactor:128");
  lines.push("a=x-nv-vqos[0].resControl.bitrateIirFilterFactor:128");
  lines.push("a=x-nv-vqos[0].dynamicStreamingMode:0");
  // Five groups squeezed into 1 ms caused dense 120 FPS UDP bursts and large Wi-Fi receive gaps.
  // Spread high-refresh frames across less than half of their 8.33 ms frame interval; lower frame
  // rates retain the conservative baseline. This changes packet timing, not encode or display FPS.
  pushGroup("packetPacing", false, {
    ...ANNOUNCE_ALLOWLIST.packetPacing,
    maxDelayUs: fps >= 100 ? "4000" : ANNOUNCE_ALLOWLIST.packetPacing.maxDelayUs,
  });
  pushGroup("ri", false, ANNOUNCE_ALLOWLIST.ri);
  pushGroup("aqos", false, ANNOUNCE_ALLOWLIST.aqos);
  pushGroup("bwe", false, ANNOUNCE_ALLOWLIST.bwe);
  pushGroup("general", false, ANNOUNCE_ALLOWLIST.general);
  pushGroup("runtime", false, ANNOUNCE_ALLOWLIST.runtime);
  lines.push("a=x-nv-runtime.videoSrtp:1");
  if (options.encryptionKeyHex && options.encryptionKeyId !== undefined) {
    lines.push(`a=x-nv-runtime.encryptionKey:${options.encryptionKeyHex.toUpperCase()}`);
    // Official sends the keyId unsigned (u32) on the wire, and our salt derivation uses the
    // unsigned form too (deriveSrtpSaltHex does keyId >>> 0), so both ends derive the same salt.
    lines.push(`a=x-nv-runtime.encryptionKeyId:${options.encryptionKeyId >>> 0}`);
  }
  if (options.iceCredentials) {
    if (options.includeNvscLegacyIce !== false) {
      lines.push(`a=x-nv-general.iceUsernameFragment:${options.iceCredentials.usernameFragment}`);
      lines.push(`a=x-nv-general.iceUsernamePwd:${options.iceCredentials.password}`);
    }
    lines.push(`a=x-nv-general.iceUserNameFragmentV2:${options.iceCredentials.usernameFragment}`);
    lines.push(`a=x-nv-general.icePasswordV2:${options.iceCredentials.password}`);
  }
  if (options.clientPorts) {
    if (options.clientPorts.localAddress) {
      lines.push(`a=x-nv-general.clientPorts.localAddress:${options.clientPorts.localAddress}`);
    }
    if (options.clientPorts.useReserved !== undefined) {
      lines.push(`a=x-nv-general.clientPorts.useReserved:${options.clientPorts.useReserved ? 1 : 0}`);
    }
    if (options.clientPorts.fallbackDynamic !== undefined) {
      lines.push(`a=x-nv-general.clientPorts.fallbackDynamic:${options.clientPorts.fallbackDynamic ? 1 : 0}`);
    }
    if (options.clientPorts.session !== undefined) {
      lines.push(`a=x-nv-general.clientPorts.session:${options.clientPorts.session}`);
    }
    if (options.clientPorts.audio !== undefined) {
      lines.push(`a=x-nv-general.clientPorts.audio:${options.clientPorts.audio}`);
    }
    if (options.clientPorts.mic !== undefined) {
      lines.push(`a=x-nv-general.clientPorts.mic:${options.clientPorts.mic}`);
    }
    lines.push(`a=x-nv-general.clientPorts.video:${options.clientPorts.video}`);
    if (options.clientPorts.control !== undefined) {
      lines.push(`a=x-nv-general.clientPorts.control:${options.clientPorts.control}`);
    }
    if (options.clientPorts.bundle !== undefined) {
      lines.push(`a=x-nv-general.clientPorts.bundle:${options.clientPorts.bundle}`);
    }
  }
  if (options.clientBundlePort !== undefined) {
    lines.push(`a=x-nv-general.clientBundlePort:${options.clientBundlePort}`);
  }
  if (options.clientTransport) {
    lines.push(`a=x-nv-general.clientTransport:${options.clientTransport}`);
  }
  if (options.nativeRtcOnBundlePort) {
    lines.push(`a=x-nv-general.nativeRtcOnBundlePort:${options.nativeRtcOnBundlePort}`);
  }
  const rtcVideo = options.rtcVideoOnNativeBundle ?? (options.rtcOnNativeBundle ? true : undefined);
  const rtcAudio = options.rtcAudioOnNativeBundle ?? (options.rtcOnNativeBundle ? true : undefined);
  const rtcMic = options.rtcMicOnNativeBundle;
  const rtcData = options.rtcDataChannelOnNativeBundle ?? (options.rtcOnNativeBundle ? true : undefined);
  if (rtcVideo !== undefined) {
    lines.push(`a=x-nv-general.rtcVideoOnNativeBundle:${rtcVideo ? "1" : "0"}`);
  }
  if (rtcAudio !== undefined) {
    lines.push(`a=x-nv-general.rtcAudioOnNativeBundle:${rtcAudio ? "1" : "0"}`);
  }
  if (rtcMic !== undefined) {
    lines.push(`a=x-nv-general.rtcMicOnNativeBundle:${rtcMic ? "1" : "0"}`);
  }
  if (rtcData !== undefined) {
    lines.push(`a=x-nv-general.rtcDataChannelOnNativeBundle:${rtcData ? "1" : "0"}`);
  }
  if (options.enableUnifiedSocket !== undefined) {
    lines.push(`a=x-nv-general.enableUnifiedSocket:${options.enableUnifiedSocket ? "1" : "0"}`);
  }
  if (options.rtcpOnSctp !== undefined) {
    lines.push(`a=x-nv-general.rtcpOnSctp:${options.rtcpOnSctp ? "1" : "0"}`);
  }
  if (options.dtlsFingerprint) {
    if (options.includeNvscLegacyDtls !== false) {
      lines.push(`a=x-nv-general.dtlsFingerprint:${options.dtlsFingerprint}`);
    }
    lines.push(`a=x-nv-general.dtlsFingerprintV2:${options.dtlsFingerprint}`);
  }
  // Official doAnnounce also emits CreateAnswer WebRTC ICE/DTLS (a=ice-ufrag,
  // a=fingerprint, host a=candidate). NVST x-nv-general.* alone does not arm inbound UDP.
  if (options.iceCredentials) {
    lines.push("a=ice-options:trickle");
    lines.push(`a=ice-ufrag:${options.iceCredentials.usernameFragment}`);
    lines.push(`a=ice-pwd:${options.iceCredentials.password}`);
  }
  if (options.dtlsFingerprint) {
    lines.push(`a=fingerprint:sha-256 ${options.dtlsFingerprint}`);
    lines.push("a=setup:actpass");
  }
  const candidateAddress = options.clientPorts?.localAddress;
  const candidatePort = options.clientBundlePort
    ?? (options.clientPorts?.bundle && options.clientPorts.bundle > 0 ? options.clientPorts.bundle : undefined)
    ?? (options.clientPorts?.video && options.clientPorts.video > 0 ? options.clientPorts.video : undefined);
  if (candidateAddress && candidatePort) {
    // Official CreateLocalCandidate format string: `a=candidate:1 1 udp 2122260223 ` + ` typ host`.
    lines.push(`a=candidate:1 1 udp 2122260223 ${candidateAddress} ${candidatePort} typ host`);
  }
  lines.push("t=0 0");
  // Live capture ANNOUNCE uses the server video port, not SDP's "port 0 = unused".
  lines.push(`m=video ${videoPort}`);
  if (options.iceCredentials || options.dtlsFingerprint) {
    lines.push("c=IN IP4 0.0.0.0");
  }
  lines.push("i=DeviceString, DeviceName");
  lines.push("");
  return lines.join("\r\n");
}

export function extractHmacSeed(sdp: string): string | null {
  const match = /^k=HMAC:([0-9A-Fa-f]{64})\s*$/m.exec(sdp);
  return match?.[1] ?? null;
}

export function extractNvstIceCredentials(
  sdp: string,
): { usernameFragment: string; password: string } | null {
  const usernameFragment = /^(?:a=(?:x-nv-)?general\.iceUsernameFragment:|a=ice-ufrag:)([^\r\n]+)\s*$/mi
    .exec(sdp)?.[1]?.trim()
    ?? /^(?:a=(?:x-nv-)?general\.iceUserNameFragmentV2:)([^\r\n]+)\s*$/mi
      .exec(sdp)?.[1]?.trim();
  const password = /^(?:a=(?:x-nv-)?general\.iceUsernamePwd:|a=ice-pwd:)([^\r\n]+)\s*$/mi
    .exec(sdp)?.[1]?.trim()
    ?? /^(?:a=(?:x-nv-)?general\.icePasswordV2:)([^\r\n]+)\s*$/mi
      .exec(sdp)?.[1]?.trim();
  if (!usernameFragment || !password) {
    return null;
  }
  return { usernameFragment, password };
}

/**
 * Pack AES-256 key + keyId into libsrtp master key||salt (88 hex).
 * Salt = keyId as `%024x` (12 bytes BE). See docs/research/nvst-srtp-key-derivation.md.
 */
export function packSrtpMasterKeySalt(aesKeyHex: string, keyId: number): string {
  const key = aesKeyHex.trim().toUpperCase();
  if (!/^[0-9A-F]{64}$/.test(key)) {
    throw new Error(`encryptionKey must be 64 hex chars, got length ${key.length}`);
  }
  return `${key}${deriveSrtpSaltHex(keyId)}`;
}

export function extractRuntimeEncryptionKey(
  sdp: string,
): { aesKeyHex: string; keyId: number } | null {
  const keyMatch = /^a=x-nv-runtime\.encryptionKey:([0-9A-Fa-f]{64})\s*$/m.exec(sdp);
  const idMatch = /^a=x-nv-runtime\.encryptionKeyId:(-?\d+)\s*$/m.exec(sdp);
  if (!keyMatch || !idMatch) {
    return null;
  }
  let keyId = Number(idMatch[1]);
  if (!Number.isFinite(keyId)) {
    return null;
  }
  // Signed i32 in SDP → unsigned u32 for salt packing.
  if (keyId < 0) {
    keyId = keyId + 0x1_0000_0000;
  }
  return { aesKeyHex: keyMatch[1]!.toUpperCase(), keyId: keyId >>> 0 };
}

export function extractMediaControl(sdp: string, mediaType: string): string | null {
  let currentMediaType: string | null = null;

  for (const rawLine of sdp.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (line.startsWith("m=")) {
      currentMediaType = line.slice(2).split(/\s+/, 1)[0]?.toLowerCase() ?? null;
      continue;
    }
    if (currentMediaType !== mediaType.toLowerCase() || !line.startsWith("a=control:")) {
      continue;
    }

    const control = line.slice("a=control:".length).trim();
    if (control && control !== "*") {
      return control;
    }
  }

  return null;
}

export function extractNvstOpusAudioTrack(sdp: string): NvstAudioTrack | null {
  let inAudioSection = false;
  let offeredPayloadTypes: number[] = [];
  let mid: string | undefined;
  let ssrc: number | undefined;
  const codecs = new Map<number, { codec: string; clockRateHz: number; channels: number }>();

  for (const rawLine of sdp.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (line.startsWith("m=")) {
      if (inAudioSection) {
        break;
      }
      const parts = line.slice(2).split(/\s+/);
      inAudioSection = parts[0]?.toLowerCase() === "audio";
      offeredPayloadTypes = inAudioSection
        ? parts.slice(3).map(Number).filter((value) => Number.isInteger(value) && value >= 0 && value <= 127)
        : [];
      continue;
    }
    if (!inAudioSection) {
      continue;
    }
    if (line.startsWith("a=mid:")) {
      mid = line.slice("a=mid:".length).trim() || undefined;
      continue;
    }
    if (line.startsWith("a=ssrc:")) {
      const value = Number(line.slice("a=ssrc:".length).split(/\s+/, 1)[0]);
      if (Number.isInteger(value) && value > 0 && value <= 0xffff_ffff) {
        ssrc = value;
      }
      continue;
    }
    const rtpmap = /^a=rtpmap:(\d+)\s+([^/\s]+)\/(\d+)\/(\d+)$/i.exec(line);
    if (!rtpmap) {
      continue;
    }
    const payloadType = Number(rtpmap[1]);
    const clockRateHz = Number(rtpmap[3]);
    const channels = Number(rtpmap[4]);
    if (
      Number.isInteger(payloadType)
      && payloadType >= 0
      && payloadType <= 127
      && Number.isInteger(clockRateHz)
      && clockRateHz > 0
      && Number.isInteger(channels)
      && channels > 0
    ) {
      codecs.set(payloadType, {
        codec: rtpmap[2]!.toLowerCase(),
        clockRateHz,
        channels,
      });
    }
  }

  for (const payloadType of offeredPayloadTypes) {
    const track = codecs.get(payloadType);
    if (track?.codec !== "opus" || track.clockRateHz !== 48_000 || track.channels > 2) {
      continue;
    }
    return {
      payloadType,
      codec: "opus",
      clockRateHz: track.clockRateHz,
      channels: track.channels,
      ...(mid ? { mid } : {}),
      ...(ssrc ? { ssrc } : {}),
    };
  }
  return null;
}

export function generateClientEncryptionKey(): { aesKeyHex: string; keyId: number } {
  const aesKeyHex = randomBytes(32).toString("hex").toUpperCase();
  const keyId = randomBytes(4).readUInt32BE(0);
  return { aesKeyHex, keyId };
}

export function generateNvstIceCredentials(): { usernameFragment: string; password: string } {
  const alphabet = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789+/";
  const random = randomBytes(26);
  const encode = (start: number, length: number): string => Array.from(
    random.subarray(start, start + length),
    (value) => alphabet[value & 0x3f],
  ).join("");
  return {
    usernameFragment: encode(0, 4),
    password: encode(4, 22),
  };
}

export function redactKey(aesKeyHex: string): string {
  if (aesKeyHex.length < 8) {
    return "****";
  }
  return `${aesKeyHex.slice(0, 4)}…${aesKeyHex.slice(-4)}`;
}
