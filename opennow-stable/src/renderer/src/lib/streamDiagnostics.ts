import type { NativeStreamStats } from "@shared/gfn";

import type { StreamDiagnostics } from "../gfn/webrtcClient";

export function defaultDiagnostics(): StreamDiagnostics {
  return {
    connectionState: "closed",
    inputReady: false,
    nativeRendererActive: false,
    connectedGamepads: 0,
    resolution: "",
    codec: "",
    hardwareAcceleration: "",
    colorCodec: "",
    isHdr: false,
    bitrateKbps: 0,
    targetBitrateKbps: 0,
    decodeFps: 0,
    renderFps: 0,
    packetsLost: 0,
    packetsReceived: 0,
    packetLossPercent: 0,
    jitterMs: 0,
    rttMs: 0,
    framesReceived: 0,
    framesDecoded: 0,
    framesDropped: 0,
    decodeTimeMs: 0,
    renderTimeMs: 0,
    jitterBufferDelayMs: 0,
    inputQueueBufferedBytes: 0,
    inputQueuePeakBufferedBytes: 0,
    partiallyReliableInputQueueBufferedBytes: 0,
    partiallyReliableInputQueuePeakBufferedBytes: 0,
    inputQueueDropCount: 0,
    inputQueueMaxSchedulingDelayMs: 0,
    partiallyReliableInputOpen: false,
    mouseMoveTransport: "reliable",
    mouseFlushIntervalMs: 8,
    mousePacketsPerSecond: 0,
    mouseResidualMagnitude: 0,
    mouseAdaptiveFlushActive: false,
    lagReason: "unknown",
    lagReasonDetail: "Waiting for stream stats",
    gpuType: "",
    serverRegion: "",
    decoderPressureActive: false,
    decoderRecoveryAttempts: 0,
    decoderRecoveryAction: "none",
    nativeRequestedFps: undefined,
    nativeCapsFramerate: undefined,
    nativeQueueMode: undefined,
    nativeFramesPendingToPresent: undefined,
    nativePartialFlushCount: undefined,
    nativeCompleteFlushCount: undefined,
    nativeTransitionSummary: undefined,
    nativeRequestedStreamingFeaturesSummary: undefined,
    nativeFinalizedStreamingFeaturesSummary: undefined,
    micState: "uninitialized",
    micEnabled: false,
  };
}

export function mergeNativeStreamStats(
  current: StreamDiagnostics,
  stats: NativeStreamStats,
): StreamDiagnostics {
  const sinkDropped = stats.sinkDropped ?? 0;
  const sinkRendered = stats.sinkRendered ?? stats.framesRendered;
  const totalSinkFrames = sinkRendered + sinkDropped;
  const dropPercent = totalSinkFrames > 0 ? (sinkDropped / totalSinkFrames) * 100 : 0;
  const hardwareAcceleration = [
    stats.hardwareAcceleration || "GStreamer native decode",
    stats.zeroCopy && stats.memoryMode ? `${stats.memoryMode} zero-copy` : "",
    !stats.zeroCopy && stats.memoryMode ? stats.memoryMode : "",
    !stats.memoryMode && stats.zeroCopyD3D12 ? "D3D12 zero-copy" : "",
    !stats.memoryMode && stats.zeroCopyD3D11 ? "D3D11 zero-copy" : "",
  ].filter(Boolean).join(" · ");

  return {
    ...current,
    connectionState: "connected",
    inputReady: current.inputReady,
    nativeRendererActive: true,
    resolution: stats.resolution || current.resolution,
    codec: stats.codec || current.codec,
    hardwareAcceleration,
    bitrateKbps: stats.bitrateKbps,
    targetBitrateKbps: stats.targetBitrateKbps,
    decodeFps: Math.round(stats.decodedFps),
    renderFps: Math.round(stats.renderFps),
    framesReceived: stats.framesDecoded,
    framesDecoded: stats.framesDecoded,
    framesDropped: sinkDropped,
    packetLossPercent: dropPercent,
    lagReason: dropPercent > 1 ? "render" : "stable",
    lagReasonDetail: stats.lastTransitionSummary
      ? `Native bitrate ${stats.bitratePerformancePercent.toFixed(0)}% of target · ${stats.lastTransitionSummary}`
      : `Native bitrate ${stats.bitratePerformancePercent.toFixed(0)}% of target`,
    decoderPressureActive: false,
    nativeRequestedFps: stats.requestedFps,
    nativeCapsFramerate: stats.capsFramerate,
    nativeQueueMode: stats.queueMode,
    nativeFramesPendingToPresent: stats.framesPendingToPresent,
    nativePartialFlushCount: stats.partialFlushCount,
    nativeCompleteFlushCount: stats.completeFlushCount,
    nativeTransitionSummary: stats.lastTransitionSummary,
    nativeRequestedStreamingFeaturesSummary: stats.requestedStreamingFeaturesSummary,
    nativeFinalizedStreamingFeaturesSummary: stats.finalizedStreamingFeaturesSummary,
  };
}
