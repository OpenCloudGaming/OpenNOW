import type { ColorQuality, VideoCodec } from "@shared/gfn";

interface IceCredentials {
  ufrag: string;
  pwd: string;
  fingerprint: string;
}

/**
 * Convert dash-separated hostname to dotted IP if it matches the GFN pattern.
 * e.g. "80-250-97-40.cloudmatchbeta.nvidiagrid.net" -> "80.250.97.40"
 * e.g. "161-248-11-132.bpc.geforcenow.nvidiagrid.net" -> "161.248.11.132"
 */
export function extractPublicIp(hostOrIp: string): string | null {
  if (!hostOrIp) return null;

  // Already a dotted IP?
  if (/^\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}$/.test(hostOrIp)) {
    return hostOrIp;
  }

  // Dash-separated hostname: take the first label, convert dashes to dots
  const firstLabel = hostOrIp.split(".")[0] ?? "";
  const parts = firstLabel.split("-");
  if (parts.length === 4 && parts.every((p) => /^\d{1,3}$/.test(p))) {
    return parts.join(".");
  }

  return null;
}

/**
 * Fix 0.0.0.0 in the server's SDP offer with the actual server IP.
 * Matches Rust's fix_server_ip() — replaces "c=IN IP4 0.0.0.0" with real IP.
 * Also fixes a=candidate: lines that contain 0.0.0.0 as the candidate IP,
 * since Chrome's WebRTC stack treats those as unreachable and ICE fails.
 */
export function fixServerIp(sdp: string, serverIp: string): string {
  const ip = extractPublicIp(serverIp);
  if (!ip) {
    console.log(`[SDP] fixServerIp: could not extract IP from "${serverIp}"`);
    return sdp;
  }
  // 1. Fix connection lines: c=IN IP4 0.0.0.0
  const cCount = (sdp.match(/c=IN IP4 0\.0\.0\.0/g) ?? []).length;
  let fixed = sdp.replace(/c=IN IP4 0\.0\.0\.0/g, `c=IN IP4 ${ip}`);
  console.log(`[SDP] fixServerIp: replaced ${cCount} c= lines with ${ip}`);

  // 2. Fix ICE candidate lines: a=candidate:... 0.0.0.0 ...
  //    Format: a=candidate:<foundation> <component> <protocol> <priority> <ip> <port> typ <type>
  const candidateCount = (fixed.match(/(a=candidate:\S+\s+\d+\s+\w+\s+\d+\s+)0\.0\.0\.0(\s+)/g) ?? []).length;
  if (candidateCount > 0) {
    fixed = fixed.replace(
      /(a=candidate:\S+\s+\d+\s+\w+\s+\d+\s+)0\.0\.0\.0(\s+)/g,
      `$1${ip}$2`,
    );
    console.log(`[SDP] fixServerIp: replaced ${candidateCount} a=candidate lines with ${ip}`);
  }

  return fixed;
}

/**
 * Extract the server's ice-ufrag from the offer SDP.
 * Needed for manual ICE candidate injection (ice-lite servers).
 */
export function extractIceUfragFromOffer(sdp: string): string {
  const match = sdp.match(/a=ice-ufrag:([^\r\n]+)/);
  return match?.[1]?.trim() ?? "";
}

export function extractIceCredentials(sdp: string): IceCredentials {
  const ufrag = sdp
    .split(/\r?\n/)
    .find((line) => line.startsWith("a=ice-ufrag:"))
    ?.replace("a=ice-ufrag:", "")
    .trim();
  const pwd = sdp
    .split(/\r?\n/)
    .find((line) => line.startsWith("a=ice-pwd:"))
    ?.replace("a=ice-pwd:", "")
    .trim();
  const fingerprint = sdp
    .split(/\r?\n/)
    .find((line) => line.startsWith("a=fingerprint:sha-256 "))
    ?.replace("a=fingerprint:sha-256 ", "")
    .trim();

  return {
    ufrag: ufrag ?? "",
    pwd: pwd ?? "",
    fingerprint: fingerprint ?? "",
  };
}

function normalizeCodec(name: string): string {
  const upper = name.toUpperCase();
  return upper === "HEVC" ? "H265" : upper;
}

export function rewriteH265TierFlag(
  sdp: string,
  tierFlag: 0 | 1,
): { sdp: string; replacements: number } {
  const lineEnding = sdp.includes("\r\n") ? "\r\n" : "\n";
  const lines = sdp.split(/\r?\n/);

  const h265Payloads = new Set<string>();
  let inVideoSection = false;

  for (const line of lines) {
    if (line.startsWith("m=video")) {
      inVideoSection = true;
      continue;
    }
    if (line.startsWith("m=") && inVideoSection) {
      inVideoSection = false;
    }
    if (!inVideoSection || !line.startsWith("a=rtpmap:")) {
      continue;
    }

    const [, rest = ""] = line.split(":", 2);
    const [pt = "", codecPart = ""] = rest.split(/\s+/, 2);
    const codecName = normalizeCodec((codecPart.split("/")[0] ?? "").trim());
    if (pt && codecName === "H265") {
      h265Payloads.add(pt);
    }
  }

  if (h265Payloads.size === 0) {
    return { sdp, replacements: 0 };
  }

  let replacements = 0;
  const rewritten = lines.map((line) => {
    if (!line.startsWith("a=fmtp:")) {
      return line;
    }

    const [, rest = ""] = line.split(":", 2);
    const [pt = ""] = rest.split(/\s+/, 1);
    if (!pt || !h265Payloads.has(pt)) {
      return line;
    }

    const next = line.replace(/tier-flag=1/gi, `tier-flag=${tierFlag}`);
    if (next !== line) {
      replacements += 1;
    }
    return next;
  });

  return {
    sdp: rewritten.join(lineEnding),
    replacements,
  };
}

export function rewriteH265LevelIdByProfile(
  sdp: string,
  maxLevelByProfile: Partial<Record<1 | 2, number>>,
): { sdp: string; replacements: number } {
  const lineEnding = sdp.includes("\r\n") ? "\r\n" : "\n";
  const lines = sdp.split(/\r?\n/);

  const h265Payloads = new Set<string>();
  let inVideoSection = false;

  for (const line of lines) {
    if (line.startsWith("m=video")) {
      inVideoSection = true;
      continue;
    }
    if (line.startsWith("m=") && inVideoSection) {
      inVideoSection = false;
    }
    if (!inVideoSection || !line.startsWith("a=rtpmap:")) {
      continue;
    }

    const [, rest = ""] = line.split(":", 2);
    const [pt = "", codecPart = ""] = rest.split(/\s+/, 2);
    const codecName = normalizeCodec((codecPart.split("/")[0] ?? "").trim());
    if (pt && codecName === "H265") {
      h265Payloads.add(pt);
    }
  }

  if (h265Payloads.size === 0) {
    return { sdp, replacements: 0 };
  }

  let replacements = 0;
  const rewritten = lines.map((line) => {
    if (!line.startsWith("a=fmtp:")) {
      return line;
    }

    const [, rest = ""] = line.split(":", 2);
    const [pt = "", params = ""] = rest.split(/\s+/, 2);
    if (!pt || !params || !h265Payloads.has(pt)) {
      return line;
    }

    const profileMatch = params.match(/(?:^|;)\s*profile-id=(\d+)/i);
    const levelMatch = params.match(/(?:^|;)\s*level-id=(\d+)/i);
    if (!profileMatch?.[1] || !levelMatch?.[1]) {
      return line;
    }

    const profileNum = Number.parseInt(profileMatch[1], 10) as 1 | 2;
    const offeredLevel = Number.parseInt(levelMatch[1], 10);
    const maxLevel = maxLevelByProfile[profileNum];
    if (!Number.isFinite(offeredLevel) || !maxLevel || offeredLevel <= maxLevel) {
      return line;
    }

    const next = line.replace(/(level-id=)(\d+)/i, `$1${maxLevel}`);
    if (next !== line) {
      replacements += 1;
    }
    return next;
  });

  return {
    sdp: rewritten.join(lineEnding),
    replacements,
  };
}

interface PreferCodecOptions {
  preferHevcProfileId?: 1 | 2;
}

export function preferCodec(sdp: string, codec: VideoCodec, options?: PreferCodecOptions): string {
  console.log(`[SDP] preferCodec: filtering SDP for codec "${codec}"`);
  const lineEnding = sdp.includes("\r\n") ? "\r\n" : "\n";
  const lines = sdp.split(/\r?\n/);

  let inVideoSection = false;
  const payloadTypesByCodec = new Map<string, string[]>();
  const codecByPayloadType = new Map<string, string>();
  const rtxAptByPayloadType = new Map<string, string>();
  const fmtpByPayloadType = new Map<string, string>();

  for (const line of lines) {
    if (line.startsWith("m=video")) {
      inVideoSection = true;
      continue;
    }
    if (line.startsWith("m=") && inVideoSection) {
      inVideoSection = false;
    }

    if (!inVideoSection || !line.startsWith("a=rtpmap:")) {
      continue;
    }

    const [, rest = ""] = line.split("a=rtpmap:");
    const [pt, codecPart] = rest.split(/\s+/, 2);
    const codecName = normalizeCodec((codecPart ?? "").split("/")[0] ?? "");
    if (!pt || !codecName) {
      continue;
    }

    const list = payloadTypesByCodec.get(codecName) ?? [];
    list.push(pt);
    payloadTypesByCodec.set(codecName, list);
    codecByPayloadType.set(pt, codecName);

    continue;
  }

  // Parse RTX apt mappings from fmtp lines so we can keep RTX for chosen codec payloads
  inVideoSection = false;
  for (const line of lines) {
    if (line.startsWith("m=video")) {
      inVideoSection = true;
      continue;
    }
    if (line.startsWith("m=") && inVideoSection) {
      inVideoSection = false;
    }
    if (!inVideoSection || !line.startsWith("a=fmtp:")) {
      continue;
    }

    const [, rest = ""] = line.split(":", 2);
    const [pt = "", params = ""] = rest.split(/\s+/, 2);
    if (!pt || !params) {
      continue;
    }

    const aptMatch = params.match(/(?:^|;)\s*apt=(\d+)/i);
    if (aptMatch?.[1]) {
      rtxAptByPayloadType.set(pt, aptMatch[1]);
    }
    fmtpByPayloadType.set(pt, params);
  }

  // Log all codecs found in the SDP
  for (const [name, pts] of payloadTypesByCodec.entries()) {
    console.log(`[SDP] preferCodec: found codec ${name} with payload types [${pts.join(", ")}]`);
  }

  const preferredPayloads = payloadTypesByCodec.get(codec) ?? [];
  if (preferredPayloads.length === 0) {
    console.log(`[SDP] preferCodec: codec "${codec}" NOT found in offer — returning SDP unmodified`);
    return sdp;
  }

  // H265 often appears with multiple profiles in one offer.
  // Prefer profile-id=1 first (widest decoder compatibility), then others.
  const orderedPreferredPayloads = codec === "H265" && options?.preferHevcProfileId
    ? [...preferredPayloads].sort((a, b) => {
      const pa = fmtpByPayloadType.get(a) ?? "";
      const pb = fmtpByPayloadType.get(b) ?? "";
      const score = (fmtp: string): number => {
        const profile = fmtp.match(/(?:^|;)\s*profile-id=(\d+)/i)?.[1];
        if (profile === String(options.preferHevcProfileId)) return 0;
        if (!profile) return 1;
        return 2;
      };
      return score(pa) - score(pb);
    })
    : preferredPayloads;

  const preferred = new Set(orderedPreferredPayloads);

  const allowed = new Set<string>(preferred);

  // Keep RTX payloads linked to preferred payloads (apt mapping)
  for (const [rtxPt, apt] of rtxAptByPayloadType.entries()) {
    if (preferred.has(apt) && codecByPayloadType.get(rtxPt) === "RTX") {
      allowed.add(rtxPt);
    }
  }

  // Do NOT keep FLEXFEC/RED/ULPFEC during hard codec filtering.
  // Chromium can otherwise negotiate a "video" m-line with only FEC payloads
  // when primary codec intersection fails, causing black video with live audio.

  console.log(`[SDP] preferCodec: preferred ordered payloads [${orderedPreferredPayloads.join(", ")}] for ${codec}`);
  console.log(`[SDP] preferCodec: keeping payload types [${Array.from(allowed).join(", ")}] for ${codec}`);

  const filtered: string[] = [];
  inVideoSection = false;

  for (const line of lines) {
    if (line.startsWith("m=video")) {
      inVideoSection = true;
      const parts = line.split(/\s+/);
      const header = parts.slice(0, 3);
      const available = parts.slice(3).filter((pt) => allowed.has(pt));
      const ordered: string[] = [];

      for (const pt of orderedPreferredPayloads) {
        if (available.includes(pt)) {
          ordered.push(pt);
        }
      }
      for (const pt of available) {
        if (!preferred.has(pt)) {
          ordered.push(pt);
        }
      }

      filtered.push(ordered.length > 0 ? [...header, ...ordered].join(" ") : line);
      continue;
    }

    if (line.startsWith("m=") && inVideoSection) {
      inVideoSection = false;
    }

    if (inVideoSection) {
      if (
        line.startsWith("a=rtpmap:") ||
        line.startsWith("a=fmtp:") ||
        line.startsWith("a=rtcp-fb:")
      ) {
        const [, rest = ""] = line.split(":", 2);
        const [pt = ""] = rest.split(/\s+/, 1);
        if (pt && !allowed.has(pt)) {
          continue;
        }
      }
    }

    filtered.push(line);
  }

  return filtered.join(lineEnding);
}

interface NvstParams {
  width: number;
  height: number;
  fps: number;
  maxBitrateKbps: number;
  partialReliableThresholdMs: number;
  codec: VideoCodec;
  colorQuality: ColorQuality;
  credentials: IceCredentials;
}

/**
 * Munge an SDP answer to inject bitrate limits and optimize audio codec params.
 * 
 * This matches what the official GFN browser client does:
 * 1. Adds "b=AS:<kbps>" after each m= line to signal our max receive bitrate
 * 2. Adds "stereo=1" to the opus fmtp line for stereo audio support
 * 
 * These are hints to the server encoder — they don't enforce limits client-side
 * but help the server avoid overshooting our link capacity.
 */
export function mungeAnswerSdp(sdp: string, maxBitrateKbps: number): string {
  const lineEnding = sdp.includes("\r\n") ? "\r\n" : "\n";
  const lines = sdp.split(/\r?\n/);
  const result: string[] = [];

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    result.push(line);

    // After each m= line, inject b=AS: if not already present
    if (line.startsWith("m=video") || line.startsWith("m=audio")) {
      const bitrateForSection = line.startsWith("m=video")
        ? maxBitrateKbps
        : 128; // 128 kbps for audio is plenty for opus stereo
      const nextLine = lines[i + 1] ?? "";
      if (!nextLine.startsWith("b=")) {
        result.push(`b=AS:${bitrateForSection}`);
      }
    }

    // Append stereo=1 to opus fmtp line if not already present
    if (line.startsWith("a=fmtp:") && line.includes("minptime=") && !line.includes("stereo=1")) {
      // Replace the line we just pushed with the stereo-augmented version
      result[result.length - 1] = line + ";stereo=1";
    }
  }

  console.log(`[SDP] mungeAnswerSdp: injected b=AS:${maxBitrateKbps} for video, b=AS:128 for audio, stereo=1 for opus`);
  return result.join(lineEnding);
}

export function buildNvstSdp(params: NvstParams): string {
  console.log(`[SDP] buildNvstSdp: ${params.width}x${params.height}@${params.fps}fps, codec=${params.codec}, colorQuality=${params.colorQuality}, maxBitrate=${params.maxBitrateKbps}kbps`);
  console.log(`[SDP] buildNvstSdp: ICE ufrag=${params.credentials.ufrag}, pwd=${params.credentials.pwd.slice(0, 8)}..., fingerprint=${params.credentials.fingerprint.slice(0, 20)}...`);
  // Adaptive profile:
  // allow bitrate to scale down under congestion to reduce stutter and input lag.
  const minBitrate = Math.max(5000, Math.floor(params.maxBitrateKbps * 0.35));
  const initialBitrate = Math.max(minBitrate, Math.floor(params.maxBitrateKbps * 0.7));
  const isHighFps = params.fps >= 90;
  const is120Fps = params.fps === 120;
  const is240Fps = params.fps >= 240;
  const isAv1 = params.codec === "AV1";
  const bitDepth = params.colorQuality.startsWith("10bit") ? 10 : 8;

  const lines: string[] = [
    "v=0",
    "o=SdpTest test_id_13 14 IN IPv4 127.0.0.1",
    "s=-",
    "t=0 0",
    `a=general.icePassword:${params.credentials.pwd}`,
    `a=general.iceUserNameFragment:${params.credentials.ufrag}`,
    `a=general.dtlsFingerprint:${params.credentials.fingerprint}`,
    "m=video 0 RTP/AVP",
    "a=msid:fbc-video-0",
    // FEC settings
    "a=vqos.fec.rateDropWindow:10",
    "a=vqos.fec.minRequiredFecPackets:2",
    "a=vqos.fec.repairMinPercent:5",
    "a=vqos.fec.repairPercent:5",
    "a=vqos.fec.repairMaxPercent:35",
    // DRC — always disabled to allow full bitrate
    "a=vqos.drc.enable:0",
  ];

  // Force-disable dynamic frame control to avoid server-side FPS/resolution adaptation.
  lines.push("a=vqos.dfc.enable:0");

  // Video encoder settings
  lines.push(
    "a=video.dx9EnableNv12:1",
    "a=video.dx9EnableHdr:1",
    "a=vqos.qpg.enable:1",
    "a=vqos.resControl.qp.qpg.featureSetting:7",
    "a=bwe.useOwdCongestionControl:1",
    "a=video.enableRtpNack:1",
    "a=vqos.bw.txRxLag.minFeedbackTxDeltaMs:200",
    "a=vqos.drc.bitrateIirFilterFactor:18",
    "a=video.packetSize:1140",
    "a=packetPacing.minNumPacketsPerGroup:15",
  );

  // High FPS optimizations
  if (isHighFps) {
    lines.push(
      "a=bwe.iirFilterFactor:8",
      "a=video.encoderFeatureSetting:47",
      "a=video.encoderPreset:6",
      "a=vqos.resControl.cpmRtc.badNwSkipFramesCount:600",
      "a=vqos.resControl.cpmRtc.decodeTimeThresholdMs:9",
      `a=video.fbcDynamicFpsGrabTimeoutMs:${is120Fps ? 6 : 18}`,
      `a=vqos.resControl.cpmRtc.serverResolutionUpdateCoolDownCount:${is120Fps ? 6000 : 12000}`,
    );
  }

  // 240+ FPS optimizations
  if (is240Fps) {
    lines.push(
      "a=video.enableNextCaptureMode:1",
      "a=vqos.maxStreamFpsEstimate:240",
      "a=video.videoSplitEncodeStripsPerFrame:3",
      "a=video.updateSplitEncodeStateDynamically:1",
    );
  }

  // Out-of-focus handling + disable ALL dynamic resolution control
  lines.push(
    "a=vqos.adjustStreamingFpsDuringOutOfFocus:1",
    "a=vqos.resControl.cpmRtc.ignoreOutOfFocusWindowState:1",
    "a=vqos.resControl.perfHistory.rtcIgnoreOutOfFocusWindowState:1",
    // Disable CPM-based resolution changes (prevents SSRC switches)
    "a=vqos.resControl.cpmRtc.featureMask:0",
    "a=vqos.resControl.cpmRtc.enable:0",
    // Never scale down resolution
    "a=vqos.resControl.cpmRtc.minResolutionPercent:100",
    // Infinite cooldown to prevent resolution changes
    "a=vqos.resControl.cpmRtc.resolutionChangeHoldonMs:999999",
    // Packet pacing
    `a=packetPacing.numGroups:${is120Fps ? 3 : 5}`,
    "a=packetPacing.maxDelayUs:1000",
    "a=packetPacing.minNumPacketsFrame:10",
    // NACK queue settings
    "a=video.rtpNackQueueLength:1024",
    "a=video.rtpNackQueueMaxPackets:512",
    "a=video.rtpNackMaxPacketCount:25",
    // Resolution/quality thresholds — high values prevent downscaling
    "a=vqos.drc.qpMaxResThresholdAdj:4",
    "a=vqos.grc.qpMaxResThresholdAdj:4",
    "a=vqos.drc.iirFilterFactor:100",
  );

  // AV1-specific DRC/GRC tuning (mirrors official client intent):
  // bias towards QP adaptation before resolution downgrade.
  if (isAv1) {
    lines.push(
      "a=vqos.drc.minQpHeadroom:20",
      "a=vqos.drc.lowerQpThreshold:100",
      "a=vqos.drc.upperQpThreshold:200",
      "a=vqos.drc.minAdaptiveQpThreshold:180",
      "a=vqos.drc.qpCodecThresholdAdj:0",
      // official client scales this up for AV1
      "a=vqos.drc.qpMaxResThresholdAdj:20",
      // mirror to DFC/GRC
      "a=vqos.dfc.minQpHeadroom:20",
      "a=vqos.dfc.qpLowerLimit:100",
      "a=vqos.dfc.qpMaxUpperLimit:200",
      "a=vqos.dfc.qpMinUpperLimit:180",
      "a=vqos.dfc.qpMaxResThresholdAdj:20",
      "a=vqos.dfc.qpCodecThresholdAdj:0",
      "a=vqos.grc.minQpHeadroom:20",
      "a=vqos.grc.lowerQpThreshold:100",
      "a=vqos.grc.upperQpThreshold:200",
      "a=vqos.grc.minAdaptiveQpThreshold:180",
      "a=vqos.grc.qpMaxResThresholdAdj:20",
      "a=vqos.grc.qpCodecThresholdAdj:0",
      "a=video.minQp:25",
      // official client can enable this for AV1 depending on resolution class
      "a=video.enableAv1RcPrecisionFactor:1",
    );
  }

  // Viewport, FPS, and bitrate
  lines.push(
    `a=video.clientViewportWd:${params.width}`,
    `a=video.clientViewportHt:${params.height}`,
    `a=video.maxFPS:${params.fps}`,
    `a=video.initialBitrateKbps:${initialBitrate}`,
    `a=video.initialPeakBitrateKbps:${params.maxBitrateKbps}`,
    `a=vqos.bw.maximumBitrateKbps:${params.maxBitrateKbps}`,
    `a=vqos.bw.minimumBitrateKbps:${minBitrate}`,
    `a=vqos.bw.peakBitrateKbps:${params.maxBitrateKbps}`,
    `a=vqos.bw.serverPeakBitrateKbps:${params.maxBitrateKbps}`,
    "a=vqos.bw.enableBandwidthEstimation:1",
    "a=vqos.bw.disableBitrateLimit:0",
    // GRC — disabled
    `a=vqos.grc.maximumBitrateKbps:${params.maxBitrateKbps}`,
    "a=vqos.grc.enable:0",
    // Encoder settings
    "a=video.maxNumReferenceFrames:4",
    "a=video.mapRtpTimestampsToFrames:1",
    "a=video.encoderCscMode:3",
    "a=video.dynamicRangeMode:0",
    `a=video.bitDepth:${bitDepth}`,
    // Disable server-side scaling and prefilter (prevents resolution downgrade)
    `a=video.scalingFeature1:${isAv1 ? 1 : 0}`,
    "a=video.prefilterParams.prefilterModel:0",
    // Audio track
    "m=audio 0 RTP/AVP",
    "a=msid:audio",
    // Mic track
    "m=mic 0 RTP/AVP",
    "a=msid:mic",
    // Input/application track
    "m=application 0 RTP/AVP",
    "a=msid:input_1",
    `a=ri.partialReliableThresholdMs:${params.partialReliableThresholdMs}`,
    "",
  );

  return lines.join("\n");
}
