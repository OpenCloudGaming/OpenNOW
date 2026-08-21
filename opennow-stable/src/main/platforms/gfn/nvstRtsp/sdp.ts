import { randomBytes } from "node:crypto";

import { deriveSrtpSaltHex } from "./srtp";

/** Minimal ANNOUNCE attrs from docs/research/nvst-announce-allowlist-1080p60.json */
const ANNOUNCE_ALLOWLIST = {
  video: {
    clientViewportWd: "1920",
    clientViewportHt: "1080",
    maxFPS: "60",
    videoSplitEncodeStripsPerFrame: "3",
    updateSplitEncodeStateDynamically: "1",
    packetSize: "1408",
    enableRtpNack: "1",
    rtpNackQueueLength: "2048",
    rtpNackQueueMaxPackets: "1024",
    rtpNackMaxPacketCount: "64",
    "framePacing.mode": "2",
    "framePacing.pid.minTargetFrameTimeUs": "16666",
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
    "fec.enable": "1",
    "fec.repairPercent": "20",
    "fec.repairMinPercent": "5",
    "fec.repairMaxPercent": "40",
    "bllFec.enable": "1",
    "grc.enable": "0",
    "drc.enable": "0",
    "dfc.adjustResAndFps": "0",
    calculateAvgVideoStreamingBitrate: "1",
  },
  packetPacing: {
    version: "3",
    mode: "1",
    numGroups: "5",
    maxDelayUs: "1000",
    minNumPacketsFrame: "10",
    minNumPacketsPerGroup: "0",
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
    enableRedundancy: "1",
    redundancyLevel: "2",
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
  const fps = options.fps && options.fps > 0 ? Math.round(options.fps) : 60;
  const frameTimeUs = String(Math.round(1_000_000 / fps));
  const videoPort = options.videoPort && options.videoPort > 0 ? options.videoPort : 0;

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
      } else if (prefix === "video" && key === "maxFPS") {
        nextValue = String(fps);
      } else if (prefix === "video" && key === "framePacing.pid.minTargetFrameTimeUs") {
        nextValue = frameTimeUs;
      }
      const name = indexed ? `x-nv-${prefix}[0].${key}` : `x-nv-${prefix}.${key}`;
      lines.push(`a=${name}:${nextValue}`);
    }
  };

  pushGroup("video", true, ANNOUNCE_ALLOWLIST.video);
  pushGroup("vqos", true, ANNOUNCE_ALLOWLIST.vqos);
  pushGroup("packetPacing", false, ANNOUNCE_ALLOWLIST.packetPacing);
  pushGroup("ri", false, ANNOUNCE_ALLOWLIST.ri);
  pushGroup("aqos", false, ANNOUNCE_ALLOWLIST.aqos);
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
    lines.push(`a=x-nv-general.clientPorts.video:${options.clientPorts.video}`);
    if (options.clientPorts.audio !== undefined) {
      lines.push(`a=x-nv-general.clientPorts.audio:${options.clientPorts.audio}`);
    }
    if (options.clientPorts.mic !== undefined) {
      lines.push(`a=x-nv-general.clientPorts.mic:${options.clientPorts.mic}`);
    }
    if (options.clientPorts.control !== undefined) {
      lines.push(`a=x-nv-general.clientPorts.control:${options.clientPorts.control}`);
    }
    if (options.clientPorts.bundle !== undefined) {
      lines.push(`a=x-nv-general.clientPorts.bundle:${options.clientPorts.bundle}`);
    }
    if (options.clientPorts.session !== undefined) {
      lines.push(`a=x-nv-general.clientPorts.session:${options.clientPorts.session}`);
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
