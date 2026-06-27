/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import type { StreamSettings } from "@shared/gfn";
import { buildRequestedStreamingFeatures, extractServerInfoRegionBases, getActiveSessions } from "./cloudmatch";

function makeSettings(overrides: Partial<StreamSettings> = {}): StreamSettings {
  return {
    resolution: "2560x1440",
    fps: 240,
    maxBitrateMbps: 75,
    codec: "H265",
    colorQuality: "8bit_420",
    keyboardLayout: "en-US",
    gameLanguage: "en_US",
    enableL4S: false,
    enableCloudGsync: false,
    clientMode: "native",
    ...overrides,
  };
}

test("CloudMatch requests resolved Cloud G-Sync value", () => {
  const off = buildRequestedStreamingFeatures(makeSettings({ enableCloudGsync: false }), 0, 0, false);
  const on = buildRequestedStreamingFeatures(makeSettings({ enableCloudGsync: true }), 0, 0, false);

  assert.equal(off.cloudGsync, false);
  assert.equal(on.cloudGsync, true);
});

test("CloudMatch reflex request follows official-style Cloud G-Sync gating", () => {
  const lowFpsNoVrr = buildRequestedStreamingFeatures(
    makeSettings({ fps: 60, enableCloudGsync: false }),
    0,
    0,
    false,
  );
  const lowFpsWithVrr = buildRequestedStreamingFeatures(
    makeSettings({ fps: 60, enableCloudGsync: true }),
    0,
    0,
    false,
  );
  const highFpsNoVrr = buildRequestedStreamingFeatures(
    makeSettings({ fps: 120, enableCloudGsync: false }),
    0,
    0,
    false,
  );

  assert.equal(lowFpsNoVrr.reflex, false);
  assert.equal(lowFpsWithVrr.reflex, true);
  assert.equal(highFpsNoVrr.reflex, true);
});

test("CloudMatch uses resolver Reflex decision when present", () => {
  const features = buildRequestedStreamingFeatures(
    makeSettings({
      fps: 60,
      enableCloudGsync: true,
      clientMode: "web",
      cloudGsyncResolution: {
        requested: true,
        enabled: true,
        reflexEnabled: false,
        reason: "web-mode",
        capabilities: {
          platformSupportsCloudGsync: false,
          isVrrCapableDisplay: false,
          isGsyncDisplay: false,
          minimumFpsForCloudGsync: 60,
          minimumFpsForReflexWithoutVrr: 120,
          detectionSource: "unsupported",
        },
      },
    }),
    0,
    0,
    false,
  );

  assert.equal(features.cloudGsync, true);
  assert.equal(features.reflex, false);
});

test("CloudMatch extracts local serverInfo region before fallback regions", () => {
  const bases = extractServerInfoRegionBases({
    metaData: [
      { key: "local-region", value: "TH BPC" },
      { key: "gfn-regions", value: "EU West, TH BPC, US East" },
      { key: "EU West", value: "https://np-eu.example.nvidiagrid.net/" },
      { key: "TH BPC", value: "https://th.bpc.geforcenow.nvidiagrid.net" },
      { key: "US East", value: "https://np-us.example.nvidiagrid.net/" },
    ],
  });

  assert.deepEqual(bases, [
    "https://th.bpc.geforcenow.nvidiagrid.net",
    "https://np-eu.example.nvidiagrid.net",
    "https://np-us.example.nvidiagrid.net",
  ]);
});

test("CloudMatch falls back to serverInfo local region when active-session HTTP request fails", async () => {
  const originalFetch = globalThis.fetch;
  const originalWarn = console.warn;
  const calls: string[] = [];

  console.warn = () => {};
  globalThis.fetch = (async (input) => {
    const url = String(input);
    calls.push(url);

    if (url === "https://prod.bpc.geforcenow.nvidiagrid.net/v2/session") {
      return new Response("bad gateway", { status: 502 });
    }

    if (url === "https://prod.bpc.geforcenow.nvidiagrid.net/v2/serverInfo") {
      return new Response(JSON.stringify({
        metaData: [
          { key: "local-region", value: "TH BPC" },
          { key: "gfn-regions", value: "TH BPC" },
          { key: "TH BPC", value: "https://th.bpc.geforcenow.nvidiagrid.net" },
        ],
      }), { status: 200 });
    }

    if (url === "https://th.bpc.geforcenow.nvidiagrid.net/v2/session") {
      return new Response(JSON.stringify({
        requestStatus: {
          statusCode: 1,
          statusDescription: "SUCCESS_STATUS",
        },
        sessions: [{
          sessionId: "session-1",
          status: 3,
          gpuType: "RTX",
          sessionRequestData: { appId: "1001" },
          sessionControlInfo: { ip: "th.bpc.geforcenow.nvidiagrid.net" },
          connectionInfo: [{ ip: "161.248.11.132", port: 443, usage: 14 }],
          monitorSettings: [{ widthInPixels: 1920, heightInPixels: 1080, framesPerSecond: 60 }],
        }],
      }), { status: 200 });
    }

    throw new Error(`Unexpected fetch: ${url}`);
  }) as typeof fetch;

  try {
    const sessions = await getActiveSessions("token", "https://prod.bpc.geforcenow.nvidiagrid.net/");

    assert.equal(sessions.length, 1);
    assert.equal(sessions[0].sessionId, "session-1");
    assert.equal(sessions[0].serverIp, "161.248.11.132");
    assert.deepEqual(calls, [
      "https://prod.bpc.geforcenow.nvidiagrid.net/v2/session",
      "https://prod.bpc.geforcenow.nvidiagrid.net/v2/serverInfo",
      "https://th.bpc.geforcenow.nvidiagrid.net/v2/session",
    ]);
  } finally {
    globalThis.fetch = originalFetch;
    console.warn = originalWarn;
  }
});
