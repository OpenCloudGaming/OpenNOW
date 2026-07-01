import type { VideoAccelerationPreference } from "@shared/gfn";

export interface BootstrapVideoPreferences {
  decoderPreference: VideoAccelerationPreference;
  encoderPreference: VideoAccelerationPreference;
}

export interface VideoAccelerationCommandLine {
  enableFeatures: string[];
  disableFeatures: string[];
  switches: Record<string, string | true>;
}

export function isAccelerationPreference(
  value: unknown,
): value is VideoAccelerationPreference {
  return value === "auto" || value === "hardware" || value === "software";
}

export function buildVideoAccelerationCommandLine(
  preferences: BootstrapVideoPreferences,
  platform: NodeJS.Platform,
  arch: NodeJS.Architecture,
): VideoAccelerationCommandLine {
  const enableFeatures = [
    "MediaRecorderEnableMp4Muxer",
    "Dav1dVideoDecoder",
    "HardwareMediaKeyHandling",
  ];
  const disableFeatures = ["WebRtcHideLocalIpsWithMdns"];
  const switches: Record<string, string | true> = {
    "ignore-gpu-blocklist": true,
  };
  const isLinuxArm = platform === "linux" && (arch === "arm64" || arch === "arm");
  const isDarwinArm64 = platform === "darwin" && arch === "arm64";

  if (platform === "win32") {
    if (preferences.decoderPreference !== "software") {
      enableFeatures.push("D3D11VideoDecoder");
    }
    if (preferences.decoderPreference !== "software" || preferences.encoderPreference !== "software") {
      enableFeatures.push("MediaFoundationD3D11VideoCapture");
    }
  } else if (platform === "linux") {
    if (isLinuxArm) {
      if (preferences.decoderPreference !== "software") {
        enableFeatures.push("UseChromeOSDirectVideoDecoder");
      }
    } else {
      if (preferences.decoderPreference !== "software") {
        enableFeatures.push(
          "VaapiVideoDecoder",
          "AcceleratedVideoDecodeLinuxGL",
          "AcceleratedVideoDecodeLinuxZeroCopyGL",
          "VaapiOnNvidiaGPUs",
        );
      }
      if (preferences.encoderPreference !== "software") {
        enableFeatures.push("VaapiVideoEncoder", "AcceleratedVideoEncoder");
      }
      if (preferences.decoderPreference !== "software" || preferences.encoderPreference !== "software") {
        enableFeatures.push("VaapiIgnoreDriverChecks");
      }
    }
  } else if (platform === "darwin") {
    // Always enable Metal GPU rasterization and IOSurface memory on macOS
    enableFeatures.push("CanvasOopRasterization", "Metal", "IOSurfaceMemory");
    switches["enable-gpu-rasterization"] = true;
    // Always enable accelerated MJPEG decode on macOS
    switches["enable-accelerated-mjpeg-decode"] = true;

    if (isDarwinArm64) {
      // Apple Silicon (M1/M2/M3/M4): Metal GPU + VideoToolbox with VP9/HEVC/Arm paths
      switches["use-gl"] = "metal";
      if (preferences.decoderPreference !== "software") {
        enableFeatures.push(
          "VideoToolboxVideoDecoder",
          "VideoToolboxVp9Decoding",
          "VideoToolboxHEVCDecoding",
          "VideoToolboxVp9DecodingOnArm",
          "VaapiIgnoreDriverChecks",
          "UseMetalVideoDecoder",
          "MetalANGLE",
          "UseEGLImageForMacVideoToolbox",
        );
      }
      if (preferences.encoderPreference !== "software") {
        enableFeatures.push("VideoToolboxVideoEncoder", "UseMetalVideoEncoder");
      }
    } else {
      // Intel Mac: ANGLE GL + VideoToolbox decode with VP9 support
      // Note: VideoToolboxVideoEncoder is Apple Silicon only; not added for Intel Mac
      switches["use-gl"] = "angle";
      if (preferences.decoderPreference !== "software") {
        enableFeatures.push(
          "VideoToolboxVideoDecoder",
          "VideoToolboxVp9Decoding",
        );
      }
    }
  }

  if (platform === "linux" && !isLinuxArm) {
    disableFeatures.push("UseChromeOSDirectVideoDecoder");
  }

  if (preferences.decoderPreference === "hardware") {
    switches["enable-accelerated-video-decode"] = true;
  } else if (preferences.decoderPreference === "software") {
    switches["disable-accelerated-video-decode"] = true;
  }

  if (preferences.encoderPreference === "hardware") {
    switches["enable-accelerated-video-encode"] = true;
  } else if (preferences.encoderPreference === "software") {
    switches["disable-accelerated-video-encode"] = true;
  }

  return { enableFeatures, disableFeatures, switches };
}

/**
 * Returns a human-readable label for the macOS platform variant.
 * @param arch - Node.js architecture string
 */
export function getMacPlatformLabel(arch: NodeJS.Architecture): string {
  if (arch === "arm64") return "Apple Silicon";
  if (arch === "x64") return "Intel Mac";
  return "Mac";
}
