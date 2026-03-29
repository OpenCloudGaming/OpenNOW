import { describe, expect, it, vi } from "vitest";

import { createStreamDiagnosticsStore } from "../../../src/renderer/src/utils/streamDiagnosticsStore";

const diagnostics = {
  connectionState: "connected",
  inputReady: true,
  connectedGamepads: 1,
  resolution: "1920x1080",
  codec: "H264",
  isHdr: false,
  bitrateKbps: 25000,
  decodeFps: 60,
  renderFps: 60,
  packetsLost: 0,
  packetsReceived: 100,
  packetLossPercent: 0,
  jitterMs: 1,
  rttMs: 20,
  framesReceived: 100,
  framesDecoded: 99,
  framesDropped: 1,
  decodeTimeMs: 4,
  renderTimeMs: 4,
  jitterBufferDelayMs: 0,
  inputQueueBufferedBytes: 0,
  inputQueuePeakBufferedBytes: 0,
  inputQueueDropCount: 0,
  inputQueueMaxSchedulingDelayMs: 0,
  lagReason: "none",
  lagReasonDetail: "",
} as const;

describe("streamDiagnosticsStore", () => {
  it("subscribes and emits only for new references", () => {
    const store = createStreamDiagnosticsStore(diagnostics as never);
    const listener = vi.fn();

    const unsubscribe = store.subscribe(listener);

    expect(store.getSnapshot()).toBe(diagnostics);
    expect(store.getServerSnapshot()).toBe(diagnostics);

    store.set(diagnostics as never);
    expect(listener).not.toHaveBeenCalled();

    const next = { ...diagnostics, bitrateKbps: 30000 };
    store.set(next as never);
    expect(listener).toHaveBeenCalledTimes(1);
    expect(store.getSnapshot()).toBe(next);

    unsubscribe();
    store.set({ ...next, bitrateKbps: 32000 } as never);
    expect(listener).toHaveBeenCalledTimes(1);
  });
});
