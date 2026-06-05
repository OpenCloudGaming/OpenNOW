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
