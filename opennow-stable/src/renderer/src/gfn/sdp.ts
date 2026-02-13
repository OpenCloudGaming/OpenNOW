import type { VideoCodec } from "@shared/gfn";

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

export function preferCodec(sdp: string, codec: VideoCodec): string {
  console.log(`[SDP] preferCodec: filtering SDP for codec "${codec}"`);
  const lineEnding = sdp.includes("\r\n") ? "\r\n" : "\n";
  const lines = sdp.split(/\r?\n/);

  let inVideoSection = false;
  const payloadTypesByCodec = new Map<string, string[]>();

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
  }

  // Log all codecs found in the SDP
  for (const [name, pts] of payloadTypesByCodec.entries()) {
    console.log(`[SDP] preferCodec: found codec ${name} with payload types [${pts.join(", ")}]`);
  }

  const preferred = new Set(payloadTypesByCodec.get(codec) ?? []);
  if (preferred.size === 0) {
    console.log(`[SDP] preferCodec: codec "${codec}" NOT found in offer — returning SDP unmodified`);
    return sdp;
  }

  console.log(`[SDP] preferCodec: keeping payload types [${Array.from(preferred).join(", ")}] for ${codec}`);

  const filtered: string[] = [];
  inVideoSection = false;

  for (const line of lines) {
    if (line.startsWith("m=video")) {
      inVideoSection = true;
      const parts = line.split(/\s+/);
      const header = parts.slice(0, 3);
      const payloads = parts.slice(3).filter((pt) => preferred.has(pt));
      filtered.push(payloads.length > 0 ? [...header, ...payloads].join(" ") : line);
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
        if (pt && !preferred.has(pt)) {
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
  credentials: IceCredentials;
}

export function buildNvstSdp(params: NvstParams): string {
  console.log(`[SDP] buildNvstSdp: ${params.width}x${params.height}@${params.fps}fps, maxBitrate=${params.maxBitrateKbps}kbps`);
  console.log(`[SDP] buildNvstSdp: ICE ufrag=${params.credentials.ufrag}, pwd=${params.credentials.pwd.slice(0, 8)}..., fingerprint=${params.credentials.fingerprint.slice(0, 20)}...`);
  const minBitrate = Math.min(10000, Math.floor(params.maxBitrateKbps / 10));
  const isHighFps = params.fps >= 90;
  const is120Fps = params.fps === 120;
  const is240Fps = params.fps >= 240;

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

  // DFC (Dynamic Frame Control) for high FPS
  if (isHighFps) {
    lines.push(
      "a=vqos.dfc.enable:1",
      "a=vqos.dfc.decodeFpsAdjPercent:85",
      "a=vqos.dfc.targetDownCooldownMs:250",
      "a=vqos.dfc.dfcAlgoVersion:2",
      `a=vqos.dfc.minTargetFps:${is120Fps ? 100 : 60}`,
    );
  }

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

  // Viewport, FPS, and bitrate
  lines.push(
    `a=video.clientViewportWd:${params.width}`,
    `a=video.clientViewportHt:${params.height}`,
    `a=video.maxFPS:${params.fps}`,
    `a=video.initialBitrateKbps:${Math.floor((params.maxBitrateKbps * 3) / 4)}`,
    `a=video.initialPeakBitrateKbps:${params.maxBitrateKbps}`,
    `a=vqos.bw.maximumBitrateKbps:${params.maxBitrateKbps}`,
    `a=vqos.bw.minimumBitrateKbps:${minBitrate}`,
    `a=vqos.bw.peakBitrateKbps:${params.maxBitrateKbps}`,
    `a=vqos.bw.serverPeakBitrateKbps:${params.maxBitrateKbps}`,
    "a=vqos.bw.enableBandwidthEstimation:1",
    "a=vqos.bw.disableBitrateLimit:1",
    // GRC — disabled
    `a=vqos.grc.maximumBitrateKbps:${params.maxBitrateKbps}`,
    "a=vqos.grc.enable:0",
    // Encoder settings
    "a=video.maxNumReferenceFrames:4",
    "a=video.mapRtpTimestampsToFrames:1",
    "a=video.encoderCscMode:3",
    // Disable server-side scaling and prefilter (prevents resolution downgrade)
    "a=video.scalingFeature1:0",
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
    "a=ri.partialReliableThresholdMs:300",
    "",
  );

  return lines.join("\n");
}
