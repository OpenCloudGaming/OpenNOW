import type {
  NativeStreamerRuntimeStatus,
  NativeStreamerStatus,
  NativeVideoBackendCapability,
  NativeVideoBackendPreference,
} from "@shared/gfn";
import type { NativeStreamerCapabilities } from "@shared/nativeStreamer";

function formatVideoBackendName(backend: string | undefined): string {
  switch (backend) {
    case "d3d12":
      return "D3D12";
    case "d3d11":
      return "D3D11";
    case "videotoolbox":
      return "VideoToolbox";
    case "vaapi":
      return "VAAPI";
    case "v4l2":
      return "V4L2";
    case "vulkan":
      return "Vulkan";
    case "software":
      return "Software";
    default:
      return backend ?? "Unknown";
  }
}

function formatVideoCodec(codec: string): string {
  switch (codec.toLowerCase()) {
    case "h264":
      return "H.264";
    case "h265":
      return "H.265";
    case "av1":
      return "AV1";
    default:
      return codec.toUpperCase();
  }
}

function capabilityPlatform(platform: NodeJS.Platform): NativeVideoBackendCapability["platform"] | "other" {
  if (platform === "win32") return "windows";
  if (platform === "darwin") return "macos";
  if (platform === "linux") return "linux";
  return "other";
}

export function resolveActiveVideoBackend(
  videoBackends: NativeVideoBackendCapability[],
  preferredBackend: NativeVideoBackendPreference = "auto",
  platform = process.platform,
): NativeVideoBackendCapability | undefined {
  if (preferredBackend !== "auto") {
    const preferred = videoBackends.find((candidate) =>
      candidate.available && candidate.backend === preferredBackend,
    );
    if (preferred) return preferred;
  }

  const currentPlatform = capabilityPlatform(platform);
  return videoBackends.find((candidate) => candidate.available && candidate.platform === currentPlatform)
    ?? videoBackends.find((candidate) => candidate.available && candidate.platform === "cross-platform")
    ?? videoBackends.find((candidate) => candidate.available);
}

function summarizeCodecs(backend: NativeVideoBackendCapability | undefined): string {
  const codecs = backend?.codecs
    .filter((codec) => codec.available)
    .map((codec) => formatVideoCodec(codec.codec)) ?? [];
  return codecs.length > 0 ? codecs.join(", ") : "No hardware codec path";
}

function summarizeZeroCopy(backend: NativeVideoBackendCapability | undefined): string {
  if (!backend) {
    return "Not available";
  }
  return backend.zeroCopyModes.length > 0
    ? `Hardware memory: ${backend.zeroCopyModes.join(", ")}`
    : "System memory";
}

export function formatError(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function formatNativeStreamerDetectionFailure(
  error: unknown,
  runtime: NativeStreamerRuntimeStatus | null,
): string {
  const message = formatError(error);
  if (message.includes("3221225781") || message.toLowerCase().includes("0xc0000135")) {
    const location = runtime?.path ? ` at ${runtime.path}` : "";
    return `Native streamer could not load a required library${location}. The executable may be incomplete or blocked. ${message}`;
  }
  return `Native streamer was not detected: ${message}`;
}

export function createNativeStreamerStatus(
  capabilities: NativeStreamerCapabilities | null,
  runtimeStatus: NativeStreamerRuntimeStatus | null,
  preferredBackend: NativeVideoBackendPreference,
  platform = process.platform,
): NativeStreamerStatus {
  const backend = capabilities?.backend;
  const available = backend === "native"
    && capabilities?.supportsOfferAnswer === true
    && capabilities?.supportsVideoDecode === true
    && capabilities?.supportsVideoPresent === true;
  const videoBackends = capabilities?.videoBackends ?? [];
  const activeVideoBackend = resolveActiveVideoBackend(videoBackends, preferredBackend, platform);
  const runtime = runtimeStatus ?? {
    source: "unknown",
    selfContained: false,
    message: "Native streamer runtime has not been checked yet.",
  } satisfies NativeStreamerRuntimeStatus;

  return {
    detected: true,
    available,
    supportsOfferAnswer: capabilities?.supportsOfferAnswer === true,
    backend,
    fallbackReason: capabilities?.fallbackReason,
    videoBackends,
    activeVideoBackend,
    codecSummary: summarizeCodecs(activeVideoBackend),
    zeroCopySummary: summarizeZeroCopy(activeVideoBackend),
    runtime,
    message: available
      ? `${runtime.message} Video path: ${formatVideoBackendName(activeVideoBackend?.backend)}.`
      : capabilities?.fallbackReason ?? runtime.message,
  };
}

export function createNativeStreamerDetectionFailureStatus(
  error: unknown,
  runtimeStatus: NativeStreamerRuntimeStatus | null,
  _platform = process.platform,
): NativeStreamerStatus {
  const runtime = runtimeStatus ?? {
    source: "unknown",
    selfContained: false,
    message: "Native streamer runtime could not be checked because the executable did not start.",
  } satisfies NativeStreamerRuntimeStatus;
  return {
    detected: false,
    available: false,
    supportsOfferAnswer: false,
    runtime,
    message: formatNativeStreamerDetectionFailure(error, runtime),
  };
}
