import type { VideoCodec } from "@shared/gfn";

export interface CodecTestResult {
  codec: string;
  webrtcSupported: boolean;
  decodeSupported: boolean;
  hwAccelerated: boolean;
  encodeSupported: boolean;
  encodeHwAccelerated: boolean;
  decodeVia: string;
  encodeVia: string;
  profiles: string[];
}

const CODEC_TEST_CONFIGS: {
  name: VideoCodec;
  webrtcMime: string;
  decodeContentType: string;
  encodeContentType: string;
}[] = [
  {
    name: "H264",
    webrtcMime: "video/H264",
    decodeContentType: 'video/mp4; codecs="avc1.42E01E"',
    encodeContentType: 'video/mp4; codecs="avc1.42E01E"',
  },
  {
    name: "H265",
    webrtcMime: "video/H265",
    decodeContentType: 'video/mp4; codecs="hev1.1.6.L93.B0"',
    encodeContentType: 'video/mp4; codecs="hev1.1.6.L93.B0"',
  },
  {
    name: "AV1",
    webrtcMime: "video/AV1",
    decodeContentType: 'video/mp4; codecs="av01.0.08M.08"',
    encodeContentType: 'video/mp4; codecs="av01.0.08M.08"',
  },
];

export const CODEC_TEST_RESULTS_STORAGE_KEY = "opennow.codec-test-results.v1";

export function loadStoredCodecResults(): CodecTestResult[] | null {
  try {
    const raw = window.sessionStorage.getItem(CODEC_TEST_RESULTS_STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return null;
    return parsed as CodecTestResult[];
  } catch {
    return null;
  }
}

export function saveStoredCodecResults(results: CodecTestResult[] | null): void {
  try {
    if (results && results.length > 0) {
      window.sessionStorage.setItem(CODEC_TEST_RESULTS_STORAGE_KEY, JSON.stringify(results));
      return;
    }
    window.sessionStorage.removeItem(CODEC_TEST_RESULTS_STORAGE_KEY);
  } catch {
  }
}

function isLinuxArmClient(): boolean {
  const platform = navigator.platform?.toLowerCase() ?? "";
  const ua = navigator.userAgent?.toLowerCase() ?? "";
  const linux = platform.includes("linux") || ua.includes("linux");
  const arm = /(aarch64|arm64|armv\d|arm)/.test(platform) || /(aarch64|arm64|armv\d|arm)/.test(ua);
  return linux && arm;
}

function guessDecodeBackend(hwAccelerated: boolean): string {
  if (!hwAccelerated) return "Software (CPU)";
  const platform = navigator.platform?.toLowerCase() ?? "";
  const ua = navigator.userAgent?.toLowerCase() ?? "";
  if (platform.includes("win") || ua.includes("windows")) return "D3D11 (GPU)";
  if (platform.includes("mac") || ua.includes("macintosh")) return "VideoToolbox (GPU)";
  if (platform.includes("linux") || ua.includes("linux")) {
    return isLinuxArmClient() ? "V4L2 (GPU)" : "VA-API (GPU)";
  }
  return "Hardware (GPU)";
}

function guessEncodeBackend(hwAccelerated: boolean): string {
  if (!hwAccelerated) return "Software (CPU)";
  const platform = navigator.platform?.toLowerCase() ?? "";
  const ua = navigator.userAgent?.toLowerCase() ?? "";
  if (platform.includes("win") || ua.includes("windows")) return "Media Foundation (GPU)";
  if (platform.includes("mac") || ua.includes("macintosh")) return "VideoToolbox (GPU)";
  if (platform.includes("linux") || ua.includes("linux")) {
    return isLinuxArmClient() ? "V4L2 (GPU)" : "VA-API (GPU)";
  }
  return "Hardware (GPU)";
}

export async function testCodecSupport(): Promise<CodecTestResult[]> {
  const results: CodecTestResult[] = [];
  const webrtcCaps = RTCRtpReceiver.getCapabilities?.("video");
  const webrtcCodecMimes = new Set(webrtcCaps?.codecs.map((codec) => codec.mimeType.toLowerCase()) ?? []);
  const webrtcProfiles = new Map<string, string[]>();

  if (webrtcCaps) {
    for (const codec of webrtcCaps.codecs) {
      const mime = codec.mimeType.toLowerCase();
      const sdpLine = (codec as unknown as Record<string, string>).sdpFmtpLine ?? "";
      if (!mime.includes("rtx") && !mime.includes("red") && !mime.includes("ulpfec")) {
        const existing = webrtcProfiles.get(mime) ?? [];
        if (sdpLine) existing.push(sdpLine);
        webrtcProfiles.set(mime, existing);
      }
    }
  }

  for (const config of CODEC_TEST_CONFIGS) {
    const webrtcSupported = webrtcCodecMimes.has(config.webrtcMime.toLowerCase());
    const profiles = webrtcProfiles.get(config.webrtcMime.toLowerCase()) ?? [];

    let decodeSupported = false;
    let hwAccelerated = false;
    try {
      const decodeResult = await navigator.mediaCapabilities.decodingInfo({
        type: "webrtc",
        video: {
          contentType: config.webrtcMime === "video/H265" ? "video/h265" : config.webrtcMime.toLowerCase(),
          width: 1920,
          height: 1080,
          framerate: 60,
          bitrate: 20_000_000,
        },
      });
      decodeSupported = decodeResult.supported;
      hwAccelerated = decodeResult.powerEfficient;
    } catch {
      try {
        const decodeResult = await navigator.mediaCapabilities.decodingInfo({
          type: "file",
          video: {
            contentType: config.decodeContentType,
            width: 1920,
            height: 1080,
            framerate: 60,
            bitrate: 20_000_000,
          },
        });
        decodeSupported = decodeResult.supported;
        hwAccelerated = decodeResult.powerEfficient;
      } catch {
      }
    }

    let encodeSupported = false;
    let encodeHwAccelerated = false;
    try {
      const encodeResult = await navigator.mediaCapabilities.encodingInfo({
        type: "webrtc",
        video: {
          contentType: config.webrtcMime === "video/H265" ? "video/h265" : config.webrtcMime.toLowerCase(),
          width: 1920,
          height: 1080,
          framerate: 60,
          bitrate: 20_000_000,
        },
      });
      encodeSupported = encodeResult.supported;
      encodeHwAccelerated = encodeResult.powerEfficient;
    } catch {
      try {
        const encodeResult = await navigator.mediaCapabilities.encodingInfo({
          type: "record",
          video: {
            contentType: config.encodeContentType,
            width: 1920,
            height: 1080,
            framerate: 60,
            bitrate: 20_000_000,
          },
        });
        encodeSupported = encodeResult.supported;
        encodeHwAccelerated = encodeResult.powerEfficient;
      } catch {
      }
    }

    const effectiveDecodeSupported = decodeSupported || webrtcSupported;

    results.push({
      codec: config.name,
      webrtcSupported,
      decodeSupported: effectiveDecodeSupported,
      hwAccelerated,
      encodeSupported,
      encodeHwAccelerated,
      decodeVia: effectiveDecodeSupported ? guessDecodeBackend(hwAccelerated) : "Unsupported",
      encodeVia: encodeSupported ? guessEncodeBackend(encodeHwAccelerated) : "Unsupported",
      profiles,
    });
  }

  return results;
}

export type CodecDecodeBadgeState = "gpu" | "cpu" | "testing" | null;

export function getCodecDecodeBadgeState(
  codec: VideoCodec,
  codecResults: CodecTestResult[] | null,
  codecTesting: boolean,
): CodecDecodeBadgeState {
  const result = codecResults?.find((entry) => entry.codec === codec);
  if (!result) {
    return codecTesting ? "testing" : null;
  }
  if (!result.decodeSupported) {
    return null;
  }
  return result.hwAccelerated ? "gpu" : "cpu";
}
