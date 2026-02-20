import type { HevcCompatMode, VideoCodec } from "@shared/gfn";

export interface GpuInfo {
  vendor: "amd" | "nvidia" | "intel" | "unknown";
  renderer: string;
  unmaskedRenderer: string;
  isAmdPolarisOrVega: boolean;
}

const AMD_HEVC_PROBLEM_PATTERNS = [
  /RX\s*550/i,
  /RX\s*560/i,
  /RX\s*570/i,
  /RX\s*580/i,
  /RX\s*590/i,
  /RX\s*5[0-9]{2}\b/i,
  /Polaris/i,
  /Vega/i,
  /Ryzen.*Vega/i,
  /Radeon.*Vega/i,
  /Radeon\(TM\).*Graphics/i,
  /Radeon\s+Graphics/i,
  /gfx80[0-3]/i,
  /gfx90[0-2]/i,
];

function detectVendor(renderer: string): "amd" | "nvidia" | "intel" | "unknown" {
  const r = renderer.toLowerCase();
  if (r.includes("amd") || r.includes("radeon") || r.includes("ati")) return "amd";
  if (r.includes("nvidia") || r.includes("geforce") || r.includes("quadro") || r.includes("rtx") || r.includes("gtx")) return "nvidia";
  if (r.includes("intel") || r.includes("iris") || r.includes("uhd")) return "intel";
  return "unknown";
}

function isProblematicAmdGpu(renderer: string): boolean {
  if (detectVendor(renderer) !== "amd") return false;
  return AMD_HEVC_PROBLEM_PATTERNS.some((pattern) => pattern.test(renderer));
}

let cachedGpuInfo: GpuInfo | null = null;

export function detectGpu(): GpuInfo {
  if (cachedGpuInfo) return cachedGpuInfo;

  let renderer = "";
  let unmaskedRenderer = "";

  try {
    const canvas = document.createElement("canvas");
    const gl = canvas.getContext("webgl") ?? canvas.getContext("webgl2");
    if (gl) {
      renderer = gl.getParameter(gl.RENDERER) as string;
      const ext = gl.getExtension("WEBGL_debug_renderer_info");
      if (ext) {
        unmaskedRenderer = gl.getParameter(ext.UNMASKED_RENDERER_WEBGL) as string;
      }
      const loseCtx = gl.getExtension("WEBGL_lose_context");
      if (loseCtx) loseCtx.loseContext();
    }
  } catch {
    // WebGL not available
  }

  const effectiveRenderer = unmaskedRenderer || renderer;
  const vendor = detectVendor(effectiveRenderer);
  const isAmdPolarisOrVega = isProblematicAmdGpu(effectiveRenderer);

  cachedGpuInfo = { vendor, renderer, unmaskedRenderer, isAmdPolarisOrVega };
  return cachedGpuInfo;
}

export interface HevcCodecDecision {
  effectiveCodec: VideoCodec;
  reason: string;
  gpuInfo: GpuInfo;
  wasOverridden: boolean;
}

export function resolveHevcCompat(
  requestedCodec: VideoCodec,
  compatMode: HevcCompatMode,
): HevcCodecDecision {
  const gpuInfo = detectGpu();

  if (requestedCodec !== "H265") {
    return {
      effectiveCodec: requestedCodec,
      reason: `Codec is ${requestedCodec}, HEVC compat not applicable`,
      gpuInfo,
      wasOverridden: false,
    };
  }

  switch (compatMode) {
    case "force_h264":
      return {
        effectiveCodec: "H264",
        reason: "HEVC Compatibility Mode: Force H.264",
        gpuInfo,
        wasOverridden: true,
      };

    case "force_hevc":
      return {
        effectiveCodec: "H265",
        reason: "HEVC Compatibility Mode: Force HEVC (user override)",
        gpuInfo,
        wasOverridden: false,
      };

    case "hevc_software":
      return {
        effectiveCodec: "H265",
        reason: "HEVC Compatibility Mode: HEVC Software Decode requested",
        gpuInfo,
        wasOverridden: false,
      };

    case "auto":
    default: {
      if (gpuInfo.isAmdPolarisOrVega) {
        return {
          effectiveCodec: "H264",
          reason:
            `HEVC auto-disabled: AMD Polaris/Vega GPU detected (${gpuInfo.unmaskedRenderer || gpuInfo.renderer}). ` +
            `Known HEVC hardware decode issue (D3D11/DXVA green screen). Falling back to H.264.`,
          gpuInfo,
          wasOverridden: true,
        };
      }

      return {
        effectiveCodec: "H265",
        reason: "HEVC Compatibility Mode: Auto — no problematic GPU detected",
        gpuInfo,
        wasOverridden: false,
      };
    }
  }
}

export function shouldRequestSoftwareDecode(compatMode: HevcCompatMode, codec: VideoCodec): boolean {
  return compatMode === "hevc_software" && codec === "H265";
}
