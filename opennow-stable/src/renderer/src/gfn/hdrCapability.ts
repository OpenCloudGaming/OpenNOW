import type {
  HdrCapability,
  HdrPlatformSupport,
  HdrStreamingMode,
  HdrStreamState,
  HdrActiveStatus,
  ColorQuality,
  VideoCodec,
} from "@shared/gfn";

const api = (window as unknown as { openNow: { getOsHdrInfo: () => Promise<{ osHdrEnabled: boolean; platform: string }> } }).openNow;

export type HdrDetectionStatus =
  | "idle"
  | "probing"
  | "supported"
  | "unsupported"
  | "os_disabled"
  | "active"
  | "error";

let cachedCapability: HdrCapability | null = null;
let cachedDetectionStatus: HdrDetectionStatus = "idle";
let probeInFlight: Promise<HdrCapability> | null = null;

export function getHdrDetectionStatus(): HdrDetectionStatus {
  return cachedDetectionStatus;
}

export function getCachedHdrCapability(): HdrCapability | null {
  return cachedCapability;
}

function deriveDetectionStatus(cap: HdrCapability): HdrDetectionStatus {
  if (cap.platformSupport === "unsupported") return "unsupported";
  if (cap.platformSupport === "unknown") return "unsupported";
  if (!cap.decoder10BitCapable || !cap.displayHdrCapable) return "unsupported";
  if (cap.platform === "windows" && !cap.osHdrEnabled && cap.displayHdrCapable) return "os_disabled";
  if (cap.platformSupport === "supported" && cap.osHdrEnabled && cap.displayHdrCapable) return "active";
  if (cap.platformSupport === "best_effort" && cap.displayHdrCapable) return "supported";
  return "supported";
}

export function getHdrStatusLabel(
  mode: HdrStreamingMode,
  detectionStatus: HdrDetectionStatus,
): string {
  if (mode === "off") return "HDR off";

  if (mode === "auto") {
    switch (detectionStatus) {
      case "idle":
      case "probing":
        return "Detecting HDR\u2026";
      case "unsupported":
      case "error":
        return "HDR not supported";
      case "os_disabled":
        return "HDR supported (enable in OS settings)";
      case "active":
      case "supported":
        return "HDR active";
    }
  }

  if (mode === "on") {
    switch (detectionStatus) {
      case "idle":
      case "probing":
        return "Detecting HDR\u2026";
      case "unsupported":
      case "error":
        return "Forced HDR (may not be supported)";
      case "active":
      case "supported":
      case "os_disabled":
        return "HDR active (forced)";
    }
  }

  return "HDR off";
}

function detectPlatform(): "windows" | "macos" | "linux" | "unknown" {
  const ua = navigator.userAgent.toLowerCase();
  if (ua.includes("win")) return "windows";
  if (ua.includes("mac")) return "macos";
  if (ua.includes("linux")) return "linux";
  return "unknown";
}

function detectDisplayHdr(): { capable: boolean; notes: string[] } {
  const notes: string[] = [];

  if (typeof window.matchMedia === "function") {
    const hdrQuery = window.matchMedia("(dynamic-range: high)");
    if (hdrQuery.matches) {
      notes.push("Display reports dynamic-range: high");
      return { capable: true, notes };
    }
    notes.push("Display does not report dynamic-range: high");
  } else {
    notes.push("matchMedia not available");
  }

  const colorDepth = window.screen?.colorDepth ?? 24;
  if (colorDepth > 24) {
    notes.push(`Screen color depth: ${colorDepth} (>24, possible HDR)`);
    return { capable: true, notes };
  }
  notes.push(`Screen color depth: ${colorDepth}`);

  return { capable: false, notes };
}

async function detectDecoder10Bit(): Promise<{ capable: boolean; notes: string[] }> {
  const notes: string[] = [];

  if (typeof VideoDecoder === "undefined") {
    notes.push("VideoDecoder API not available, assuming hardware decode handles 10-bit");
    return { capable: true, notes };
  }

  const configs = [
    { codec: "hev1.2.4.L120.B0", label: "HEVC Main10" },
    { codec: "av01.0.08M.10", label: "AV1 10-bit" },
  ];

  for (const cfg of configs) {
    try {
      const result = await VideoDecoder.isConfigSupported({
        codec: cfg.codec,
        hardwareAcceleration: "prefer-hardware",
        codedWidth: 1920,
        codedHeight: 1080,
      });
      if (result.supported) {
        notes.push(`${cfg.label}: hardware decode supported`);
        return { capable: true, notes };
      }
    } catch {
      // ignore
    }

    try {
      const result = await VideoDecoder.isConfigSupported({
        codec: cfg.codec,
        hardwareAcceleration: "no-preference",
        codedWidth: 1920,
        codedHeight: 1080,
      });
      if (result.supported) {
        notes.push(`${cfg.label}: software decode supported`);
        return { capable: true, notes };
      }
    } catch {
      // ignore
    }
    notes.push(`${cfg.label}: not supported`);
  }

  return { capable: false, notes };
}

function detectHdrColorSpace(): { supported: boolean; notes: string[] } {
  const notes: string[] = [];

  const canvas = document.createElement("canvas");
  canvas.width = 1;
  canvas.height = 1;

  try {
    const ctx = canvas.getContext("2d", { colorSpace: "display-p3" } as CanvasRenderingContext2DSettings);
    if (ctx) {
      notes.push("Canvas supports wide gamut (display-p3)");
      return { supported: true, notes };
    }
  } catch {
    // not supported
  }

  try {
    const gl = canvas.getContext("webgl2");
    if (gl) {
      const ext = gl.getExtension("EXT_color_buffer_half_float");
      if (ext) {
        notes.push("WebGL2 supports half-float color buffers");
        return { supported: true, notes };
      }
    }
  } catch {
    // not supported
  }

  notes.push("No wide-gamut canvas or HDR color buffer support detected");
  return { supported: false, notes };
}

function getPlatformSupport(platform: string, osHdrEnabled: boolean, displayCapable: boolean): HdrPlatformSupport {
  if (platform === "windows") {
    if (osHdrEnabled && displayCapable) return "supported";
    if (displayCapable) return "best_effort";
    return "unsupported";
  }
  if (platform === "macos") {
    if (displayCapable) return "best_effort";
    return "unsupported";
  }
  if (platform === "linux") {
    return "unsupported";
  }
  return "unknown";
}

export async function probeHdrCapability(): Promise<HdrCapability> {
  if (probeInFlight) return probeInFlight;

  cachedDetectionStatus = "probing";
  probeInFlight = doProbe();

  try {
    const result = await probeInFlight;
    cachedCapability = result;
    cachedDetectionStatus = deriveDetectionStatus(result);
    return result;
  } catch {
    cachedDetectionStatus = "error";
    if (cachedCapability) return cachedCapability;
    throw new Error("HDR probe failed and no cached result available");
  } finally {
    probeInFlight = null;
  }
}

async function doProbe(): Promise<HdrCapability> {
  const platform = detectPlatform();
  const notes: string[] = [];

  let osHdrEnabled = false;
  try {
    const osInfo = await api.getOsHdrInfo();
    osHdrEnabled = osInfo.osHdrEnabled;
    notes.push(`OS HDR: ${osHdrEnabled ? "enabled" : "disabled"} (${osInfo.platform})`);
  } catch (e) {
    notes.push(`OS HDR detection failed: ${e instanceof Error ? e.message : String(e)}`);
  }

  const display = detectDisplayHdr();
  notes.push(...display.notes);

  const decoder = await detectDecoder10Bit();
  notes.push(...decoder.notes);

  const colorSpace = detectHdrColorSpace();
  notes.push(...colorSpace.notes);

  const platformSupport = getPlatformSupport(platform, osHdrEnabled, display.capable);

  return {
    platform,
    platformSupport,
    osHdrEnabled,
    displayHdrCapable: display.capable,
    decoder10BitCapable: decoder.capable,
    hdrColorSpaceSupported: colorSpace.supported,
    notes,
  };
}

export function shouldEnableHdr(
  mode: HdrStreamingMode,
  capability: HdrCapability,
  colorQuality: ColorQuality,
): { enable: boolean; reason: string } {
  if (mode === "off") {
    return { enable: false, reason: "HDR disabled in settings" };
  }

  const is10Bit = colorQuality.startsWith("10bit");
  if (!is10Bit) {
    return { enable: false, reason: "Color quality is 8-bit; 10-bit required for HDR" };
  }

  if (capability.platformSupport === "unsupported") {
    if (mode === "on") {
      return { enable: false, reason: `HDR unsupported on ${capability.platform}: ${capability.notes.slice(-1)[0] ?? "no HDR path"}` };
    }
    return { enable: false, reason: "Platform does not support HDR" };
  }

  if (capability.platformSupport === "unknown") {
    if (mode === "on") {
      return { enable: false, reason: "HDR support unknown on this platform" };
    }
    return { enable: false, reason: "HDR support could not be determined" };
  }

  if (!capability.decoder10BitCapable) {
    return { enable: false, reason: "No 10-bit decoder available" };
  }

  if (mode === "auto") {
    if (capability.platformSupport !== "supported") {
      return { enable: false, reason: `Platform HDR is best-effort on ${capability.platform}; set HDR to "On" to attempt` };
    }
    if (!capability.osHdrEnabled) {
      return { enable: false, reason: "OS HDR is not enabled" };
    }
    if (!capability.displayHdrCapable) {
      return { enable: false, reason: "Display does not report HDR capability" };
    }
    return { enable: true, reason: "All HDR conditions met (auto)" };
  }

  if (mode === "on") {
    if (!capability.osHdrEnabled && capability.platform === "windows") {
      return { enable: false, reason: "Windows OS HDR is disabled; enable HDR in Windows Display Settings" };
    }
    if (!capability.displayHdrCapable) {
      return { enable: false, reason: "Display does not report HDR capability" };
    }
    return { enable: true, reason: "HDR forced on by user" };
  }

  return { enable: false, reason: "Unknown HDR mode" };
}

export function buildInitialHdrState(): HdrStreamState {
  return {
    status: "inactive",
    bitDepth: 8,
    colorPrimaries: "BT.709",
    transferFunction: "SDR",
    matrixCoefficients: "BT.709",
    codecProfile: "",
    overlayForcesSdr: false,
    fallbackReason: null,
  };
}

export function buildActiveHdrState(
  codecProfile: string,
  overlayForcesSdr: boolean,
): HdrStreamState {
  if (overlayForcesSdr) {
    return {
      status: "fallback_sdr",
      bitDepth: 10,
      colorPrimaries: "BT.2020",
      transferFunction: "PQ",
      matrixCoefficients: "BT.2020",
      codecProfile,
      overlayForcesSdr: true,
      fallbackReason: "Overlay compositing forces SDR conversion",
    };
  }
  return {
    status: "active",
    bitDepth: 10,
    colorPrimaries: "BT.2020",
    transferFunction: "PQ",
    matrixCoefficients: "BT.2020",
    codecProfile,
    overlayForcesSdr: false,
    fallbackReason: null,
  };
}

export function buildFallbackHdrState(reason: string): HdrStreamState {
  return {
    status: "fallback_sdr",
    bitDepth: 8,
    colorPrimaries: "BT.709",
    transferFunction: "SDR",
    matrixCoefficients: "BT.709",
    codecProfile: "",
    overlayForcesSdr: false,
    fallbackReason: reason,
  };
}

export function buildUnsupportedHdrState(reason: string): HdrStreamState {
  return {
    status: "unsupported",
    bitDepth: 8,
    colorPrimaries: "BT.709",
    transferFunction: "SDR",
    matrixCoefficients: "BT.709",
    codecProfile: "",
    overlayForcesSdr: false,
    fallbackReason: reason,
  };
}

export function verifyHdrFromVideoTrack(
  stats: RTCStatsReport | null,
): { verified: boolean; codecProfile: string; notes: string[] } {
  const notes: string[] = [];
  let codecProfile = "";
  let verified = false;

  if (!stats) {
    notes.push("No RTC stats available");
    return { verified, codecProfile, notes };
  }

  stats.forEach((report) => {
    if (report.type === "inbound-rtp" && report.kind === "video") {
      const decoderImpl = (report as Record<string, unknown>).decoderImplementation;
      if (typeof decoderImpl === "string") {
        notes.push(`Decoder: ${decoderImpl}`);
      }
    }

    if (report.type === "codec" && (report as Record<string, unknown>).mimeType) {
      const mime = (report as Record<string, unknown>).mimeType as string;
      const sdpFmtpLine = (report as Record<string, unknown>).sdpFmtpLine as string | undefined;
      notes.push(`Codec: ${mime}`);

      if (sdpFmtpLine) {
        notes.push(`SDP fmtp: ${sdpFmtpLine}`);

        if (sdpFmtpLine.includes("profile-id=2")) {
          codecProfile = "HEVC Main10";
          verified = true;
        } else if (sdpFmtpLine.includes("profile=2")) {
          codecProfile = "AV1 Main 10-bit";
          verified = true;
        } else if (mime.includes("H265") || mime.includes("hevc")) {
          codecProfile = "HEVC";
          if (sdpFmtpLine.includes("level-id=") && sdpFmtpLine.includes("profile-space=")) {
            verified = true;
          }
        }
      }
    }
  });

  if (!verified) {
    notes.push("Could not verify 10-bit HDR codec profile from RTC stats");
  }

  return { verified, codecProfile, notes };
}
