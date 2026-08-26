/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import type { StreamSettings } from "@shared/gfn";
import {
  DEFAULT_KEYBOARD_LAYOUT,
  colorQualityBitDepth,
  colorQualityChromaFormat,
  resolveGfnKeyboardLayout,
} from "@shared/gfn";
import {
  appLaunchModeWireValue,
  buildRequestedStreamingFeatures,
  claimSession,
  createSession,
  extractServerInfoRegionBases,
  getActiveSessions,
  resolveRequestedCodecWireValue,
} from "./cloudmatch";
import { buildSessionRequestBody } from "./cloudmatchSessionRequest";
import { resolveNvstCreateStreamSku } from "./cloudmatchFeatures";
import { resolveGfnDeviceIdentity } from "./deviceIdentity";

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

test("CloudMatch uses official streaming feature enum values", () => {
  assert.equal(colorQualityBitDepth("8bit_420"), 0);
  assert.equal(colorQualityBitDepth("10bit_420"), 1);
  assert.equal(colorQualityChromaFormat("8bit_420"), 0);
  assert.equal(colorQualityChromaFormat("8bit_444"), 1);

  const features = buildRequestedStreamingFeatures(makeSettings({ enableL4S: true }), 1, 1, false);
  assert.deepEqual(features, {
    reflex: true,
    bitDepth: 1,
    cloudGsync: false,
    enabledL4S: true,
    supportedHidDevices: 0,
    profile: 0,
    fallbackToLogicalResolution: false,
    chromaFormat: 1,
    prefilterMode: 0,
    prefilterSharpness: 0,
    prefilterNoiseReduction: 0,
    hudStreamingMode: 0,
    maxBitrateKbps: 75000,
    codec: 2,
    vsync: false,
    dynamicStreamingMode: 3,
    audioChannelCount: 2,
  });
});

test("CloudMatch resolves codec preferences down the official capability ladder", () => {
  assert.equal(resolveRequestedCodecWireValue(3, [3, 2, 1]), 3);
  assert.equal(resolveRequestedCodecWireValue(3, [2, 1]), 2);
  assert.equal(resolveRequestedCodecWireValue(3, [1]), 1);
  assert.equal(resolveRequestedCodecWireValue(2, [2, 1]), 2);
  assert.equal(resolveRequestedCodecWireValue(2, [1]), 1);
  assert.equal(resolveRequestedCodecWireValue(1, [3, 2]), 1);
  assert.equal(resolveRequestedCodecWireValue(0, [3, 2, 1]), 0);
  assert.equal(resolveRequestedCodecWireValue(3, []), 3);
});

test("CloudMatch streaming features use the supported codec capability list", () => {
  assert.equal(
    buildRequestedStreamingFeatures(
      makeSettings({ codec: "AV1" }),
      0,
      0,
      false,
      ["AV1", "H265", "H264"],
    ).codec,
    3,
  );
  assert.equal(
    buildRequestedStreamingFeatures(
      makeSettings({ codec: "AV1" }),
      0,
      0,
      false,
      ["H265", "H264"],
    ).codec,
    2,
  );
  assert.equal(
    buildRequestedStreamingFeatures(
      makeSettings({ codec: "H265" }),
      0,
      0,
      false,
      ["H264"],
    ).codec,
    1,
  );
  assert.equal(
    buildRequestedStreamingFeatures(makeSettings({ codec: "AV1" }), 0, 0, false).codec,
    3,
  );
});

test("CloudMatch session request body carries supported codecs into the wire codec", () => {
  const body = buildSessionRequestBody(
    {
      appId: "1001",
      internalTitle: "Test Game",
      zone: "prod",
      settings: makeSettings({ codec: "AV1" }),
      supportedCodecs: ["H265", "H264"],
    },
    "device-id",
  );

  assert.equal(body.sessionRequestData.requestedStreamingFeatures.codec, 2);
});

test("CloudMatch requests secure RTSPS for explicit classic native sessions", () => {
  const nativeBody = buildSessionRequestBody(
    {
      appId: "1001",
      internalTitle: "Test Game",
      zone: "prod",
      settings: makeSettings({ transportMode: "nvst" }),
    },
    "device-id",
  );
  const webRtcBody = buildSessionRequestBody(
    {
      appId: "1001",
      internalTitle: "Test Game",
      zone: "prod",
      settings: makeSettings({ transportMode: "webrtc" }),
    },
    "device-id",
  );

  assert.equal(nativeBody.sessionRequestData.secureRTSPSupported, true);
  assert.equal(nativeBody.sessionRequestData.sdkVersion, "2.0");
  assert.equal(nativeBody.sessionRequestData.streamerVersion, "14");
  assert.equal(nativeBody.sessionRequestData.enhancedStreamMode, 0);
  assert.deepEqual(nativeBody.sessionRequestData.availableSupportedControllers, [2]);
  assert.equal(nativeBody.sessionRequestData.preferredController, 2);
  assert.equal(nativeBody.sessionRequestData.requestedAudioFormat, 0);
  assert.equal(nativeBody.sessionRequestData.partnerCustomData, null);
  assert.equal(nativeBody.sessionRequestData.transport, null);
  assert.equal(nativeBody.sessionRequestData.externalAppId, null);
  assert.equal(nativeBody.sessionRequestData.appId, 1001);
  assert.equal(nativeBody.sessionRequestData.internalTitle, null);
  assert.equal(nativeBody.sessionRequestData.accountLinked, false);
  assert.equal(nativeBody.sessionRequestData.deviceHashId, "device-id");
  assert.equal(nativeBody.sessionRequestData.userAge, 25);
  assert.ok((nativeBody.sessionRequestData.clientRequestMonitorSettings[0]?.dpi ?? 0) > 0);
  assert.equal(
    nativeBody.sessionRequestData.clientPlatformName,
    resolveGfnDeviceIdentity().clientPlatformName,
  );
  if (process.platform === "darwin") {
    assert.equal(nativeBody.sessionRequestData.clientPlatformName, "MacOSX");
    assert.equal(nativeBody.sessionRequestData.sdrHdrMode, 1);
    assert.equal(nativeBody.sessionRequestData.clientDisplayHdrCapabilities?.version, 2);
    assert.equal(
      nativeBody.sessionRequestData.metaData.find((entry) => entry.key === "networkType")?.value,
      "WiFi5.0",
    );
  }
  assert.equal(nativeBody.sessionRequestData.requestedStreamingFeatures.codec, 2);
  assert.equal(nativeBody.sessionRequestData.requestedStreamingFeatures.maxBitrateKbps, undefined);
  assert.equal(nativeBody.sessionRequestData.requestedStreamingFeatures.dynamicStreamingMode, undefined);
  assert.equal(nativeBody.sessionRequestData.requestedStreamingFeatures.audioChannelCount, undefined);
  assert.equal(nativeBody.sessionRequestData.requestedStreamingFeatures.trueHdr, false);
  assert.equal(nativeBody.sessionRequestData.requestedStreamingFeatures.bitDepth, 0);
  assert.equal(nativeBody.sessionRequestData.requestedStreamingFeatures.chromaFormat, 0);
  assert.equal(nativeBody.sessionRequestData.requestedStreamingFeatures.reflex, true);
  assert.equal(nativeBody.sessionRequestData.requestedStreamingFeatures.qosPolicy, 0);
  assert.deepEqual(resolveNvstCreateStreamSku(makeSettings({ transportMode: "nvst" })), {
    bitDepth: 0,
    chromaFormat: 0,
    reflex: true,
  });
  assert.equal(
    nativeBody.sessionRequestData.metaData.some((entry) => entry.key === "GSStreamerType"),
    false,
  );
  assert.equal(webRtcBody.sessionRequestData.secureRTSPSupported, false);
  assert.deepEqual(
    webRtcBody.sessionRequestData.metaData.find((entry) => entry.key === "GSStreamerType"),
    { key: "GSStreamerType", value: "WebRTC" },
  );
});

test("CloudMatch caps native sessions at 240 FPS", () => {
  const body = buildSessionRequestBody(
    {
      appId: "1001",
      internalTitle: "Test Game",
      zone: "prod",
      settings: makeSettings({ fps: 360, transportMode: "nvst" }),
    },
    "device-id",
  );

  assert.equal(
    body.sessionRequestData.clientRequestMonitorSettings[0]?.framesPerSecond,
    240,
  );
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

test("CloudMatch creates a session without a network-test pin", async () => {
  const originalFetch = globalThis.fetch;
  const originalWarn = console.warn;
  const calls: string[] = [];
  type CapturedSessionRequestBody = {
    sessionRequestData: {
      appId?: string | number;
      internalTitle?: string | null;
      accountLinked?: boolean;
      deviceHashId?: string;
      networkTestSessionId?: string | null;
      appLaunchMode?: number;
      enablePersistingInGameSettings?: boolean;
      clientRequestMonitorSettings: Array<{
        framesPerSecond: number;
      }>;
      requestedStreamingFeatures: {
        bitDepth?: number;
        chromaFormat?: number;
        maxBitrateKbps?: number;
        codec?: number;
        vsync?: boolean;
        dynamicStreamingMode?: number;
        audioChannelCount?: number;
      };
    };
  };
  let requestBody: CapturedSessionRequestBody | null = null;
  const expectedSessionUrl = `https://np-lax-01.cloudmatchbeta.nvidiagrid.net/v2/session?${new URLSearchParams({
    keyboardLayout: resolveGfnKeyboardLayout(DEFAULT_KEYBOARD_LAYOUT, process.platform),
    languageCode: "en_US",
  }).toString()}`;

  console.warn = () => {};
  globalThis.fetch = (async (input, init) => {
    const url = String(input);
    calls.push(url);

    if (url === "https://prod.cloudmatchbeta.nvidiagrid.net/v2/serverInfo") {
      return new Response(JSON.stringify({
        requestStatus: { statusCode: 1, statusDescription: "SUCCESS_STATUS", serverId: "NP-LAX-01" },
        metaData: [
          { key: "local-region", value: "US West" },
          { key: "gfn-regions", value: "US West, US East" },
          { key: "US West", value: "https://np-lax-01.cloudmatchbeta.nvidiagrid.net/" },
          { key: "US East", value: "https://np-ash-01.cloudmatchbeta.nvidiagrid.net/" },
        ],
      }), { status: 200 });
    }

    if (url === expectedSessionUrl) {
      requestBody = JSON.parse(String(init?.body));
      const createdRequestBody = requestBody;
      if (!createdRequestBody) {
        throw new Error("Session request body was not captured");
      }
      return new Response(JSON.stringify({
        requestStatus: { statusCode: 1, statusDescription: "SUCCESS_STATUS" },
        session: {
          sessionId: "session-1",
          status: 1,
          seatSetupInfo: { seatSetupStep: 0 },
          sessionControlInfo: { ip: "np-lax-01.cloudmatchbeta.nvidiagrid.net" },
          connectionInfo: [],
          iceServerConfiguration: {
            iceServers: [{ urls: "stun:127.0.0.1:19302" }],
          },
          sessionRequestData: {
            clientRequestMonitorSettings: [{ widthInPixels: 2560, heightInPixels: 1440, framesPerSecond: 90 }],
            requestedStreamingFeatures: createdRequestBody.sessionRequestData.requestedStreamingFeatures,
            enablePersistingInGameSettings: createdRequestBody.sessionRequestData.enablePersistingInGameSettings,
          },
        },
      }), { status: 200 });
    }

    throw new Error(`Unexpected fetch: ${url}`);
  }) as typeof fetch;

  try {
    const session = await createSession({
      token: "token",
      streamingBaseUrl: "https://prod.cloudmatchbeta.nvidiagrid.net/",
      appId: "1001",
      internalTitle: "Test Game",
      accountLinked: true,
      zone: "prod",
      settings: makeSettings({ fps: 90, colorQuality: "10bit_444", enableL4S: true, appLaunchMode: "gamepadFriendly" }),
    });

    assert.equal(session.streamingBaseUrl, "https://np-lax-01.cloudmatchbeta.nvidiagrid.net");
    assert.equal(session.enablePersistingInGameSettings, false);
    assert.deepEqual(calls, [
      "https://prod.cloudmatchbeta.nvidiagrid.net/v2/serverInfo",
      expectedSessionUrl,
    ]);
    const capturedRequestBody = requestBody as CapturedSessionRequestBody | null;
    assert.ok(capturedRequestBody);
    assert.equal(capturedRequestBody.sessionRequestData.clientRequestMonitorSettings[0]?.framesPerSecond, 90);
    assert.equal(capturedRequestBody.sessionRequestData.requestedStreamingFeatures.bitDepth, 1);
    assert.equal(capturedRequestBody.sessionRequestData.requestedStreamingFeatures.chromaFormat, 1);
    assert.equal(capturedRequestBody.sessionRequestData.requestedStreamingFeatures.maxBitrateKbps, 75000);
    assert.equal(capturedRequestBody.sessionRequestData.requestedStreamingFeatures.codec, 2);
    assert.equal(capturedRequestBody.sessionRequestData.requestedStreamingFeatures.vsync, false);
    assert.equal(capturedRequestBody.sessionRequestData.requestedStreamingFeatures.dynamicStreamingMode, 3);
    assert.equal(capturedRequestBody.sessionRequestData.requestedStreamingFeatures.audioChannelCount, 2);
    assert.equal(capturedRequestBody.sessionRequestData.appLaunchMode, 2);
    assert.equal(capturedRequestBody.sessionRequestData.enablePersistingInGameSettings, false);
    assert.equal(capturedRequestBody.sessionRequestData.networkTestSessionId, null);
    assert.equal(capturedRequestBody.sessionRequestData.appId, 1001);
    assert.equal(capturedRequestBody.sessionRequestData.internalTitle, null);
    assert.equal(capturedRequestBody.sessionRequestData.accountLinked, false);
    assert.match(capturedRequestBody.sessionRequestData.deviceHashId ?? "", /^[0-9a-f]{64}$/);
  } finally {
    globalThis.fetch = originalFetch;
    console.warn = originalWarn;
  }
});

test("CloudMatch NVST create posts to regional host then sends official RESUME", async (t) => {
  const originalFetch = globalThis.fetch;
  const originalLog = console.log;
  const originalWarn = console.warn;
  const calls: string[] = [];
  let resumeBodyJson: unknown = null;

  const regionalBase = "https://eu-netherlands-north.cloudmatchbeta.nvidiagrid.net";
  const query = new URLSearchParams({
    keyboardLayout: resolveGfnKeyboardLayout(DEFAULT_KEYBOARD_LAYOUT, process.platform),
    languageCode: "en_US",
  }).toString();
  const expectedPostUrl = `${regionalBase}/v2/session?${query}`;
  const expectedResumeUrl = `${regionalBase}/v2/session/session-nvst-1?${query}`;

  console.log = () => {};
  console.warn = () => {};
  t.after(() => {
    globalThis.fetch = originalFetch;
    console.log = originalLog;
    console.warn = originalWarn;
  });

  globalThis.fetch = (async (input, init) => {
    const url = String(input);
    calls.push(url);

    if (url === "https://prod.cloudmatchbeta.nvidiagrid.net/v2/serverInfo") {
      return new Response(JSON.stringify({
        requestStatus: { statusCode: 1, statusDescription: "SUCCESS_STATUS", serverId: "NP-AMS-06" },
        metaData: [
          { key: "local-region", value: "EU Northwest" },
          { key: "gfn-regions", value: "EU Northwest, Netherlands North" },
          { key: "EU Northwest", value: "https://np-ams-06.cloudmatchbeta.nvidiagrid.net/" },
          { key: "Netherlands North", value: "https://eu-netherlands-north.cloudmatchbeta.nvidiagrid.net/" },
        ],
      }), { status: 200 });
    }

    if (url === expectedPostUrl && (init?.method ?? "GET") === "POST") {
      return new Response(JSON.stringify({
        requestStatus: { statusCode: 1, statusDescription: "SUCCESS_STATUS" },
        session: {
          sessionId: "session-nvst-1",
          status: 1,
          seatSetupInfo: { seatSetupStep: 0 },
          sessionControlInfo: { ip: "np-ams-06.cloudmatchbeta.nvidiagrid.net" },
          connectionInfo: [],
          iceServerConfiguration: {
            iceServers: [{ urls: "stun:127.0.0.1:19302" }],
          },
        },
      }), { status: 200 });
    }

    if (url === expectedResumeUrl && init?.method === "PUT") {
      resumeBodyJson = JSON.parse(String(init.body));
      return new Response(JSON.stringify({
        requestStatus: { statusCode: 1, statusDescription: "SUCCESS_STATUS" },
        session: { sessionId: "session-nvst-1", status: 1 },
      }), { status: 200 });
    }

    throw new Error(`Unexpected fetch: ${url} ${init?.method ?? "GET"}`);
  }) as typeof fetch;

  const session = await createSession({
    token: "token",
    streamingBaseUrl: "https://prod.cloudmatchbeta.nvidiagrid.net/",
    appId: "1001",
    internalTitle: "Test Game",
    zone: "prod",
    settings: makeSettings({ transportMode: "nvst", resolution: "2560x1600", fps: 120 }),
  });

  assert.deepEqual(calls, [
    "https://prod.cloudmatchbeta.nvidiagrid.net/v2/serverInfo",
    expectedPostUrl,
    expectedResumeUrl,
  ]);
  const resumeBody = resumeBodyJson as {
    action?: number;
    data?: string;
    sessionRequestData?: {
      requestedStreamingFeatures?: { bitDepth?: number; reflex?: boolean; chromaFormat?: number };
    };
  } | null;
  assert.equal(resumeBody?.action, 2);
  assert.equal(resumeBody?.data, "RESUME");
  assert.equal(resumeBody?.sessionRequestData?.requestedStreamingFeatures?.bitDepth, 0);
  assert.equal(resumeBody?.sessionRequestData?.requestedStreamingFeatures?.reflex, true);
  assert.equal(resumeBody?.sessionRequestData?.requestedStreamingFeatures?.chromaFormat, 0);
  assert.equal(session.sessionId, "session-nvst-1");
});

test("CloudMatch pins a manually selected zone before creating a session", async (t) => {
  const originalFetch = globalThis.fetch;
  const originalLog = console.log;
  const calls: string[] = [];
  let networkTestSessionId: string | null | undefined;
  const base = "https://np-mia-04.cloudmatchbeta.nvidiagrid.net";
  const expectedSessionUrl = `${base}/v2/session?${new URLSearchParams({
    keyboardLayout: resolveGfnKeyboardLayout(DEFAULT_KEYBOARD_LAYOUT, process.platform),
    languageCode: "en_US",
  }).toString()}`;

  console.log = () => {};
  t.after(() => {
    globalThis.fetch = originalFetch;
    console.log = originalLog;
  });

  globalThis.fetch = (async (input, init) => {
    const url = String(input);
    calls.push(url);

    if (url === expectedSessionUrl) {
      const body = JSON.parse(String(init?.body)) as {
        sessionRequestData: {
          networkTestSessionId?: string | null;
          requestedStreamingFeatures?: unknown;
        };
      };
      networkTestSessionId = body.sessionRequestData.networkTestSessionId;
      return new Response(JSON.stringify({
        requestStatus: { statusCode: 1, statusDescription: "SUCCESS_STATUS", serverId: "NP-MIA-04" },
        session: {
          sessionId: "session-mia-1",
          status: 1,
          seatSetupInfo: { seatSetupStep: 0 },
          sessionControlInfo: { ip: "np-mia-04.cloudmatchbeta.nvidiagrid.net" },
          connectionInfo: [],
          iceServerConfiguration: {
            iceServers: [{ urls: "stun:127.0.0.1:19302" }],
          },
          sessionRequestData: {
            requestedStreamingFeatures: body.sessionRequestData.requestedStreamingFeatures,
          },
        },
      }), { status: 200 });
    }

    throw new Error(`Unexpected fetch: ${url}`);
  }) as typeof fetch;

  await createSession({
    token: "manual-region-token",
    streamingBaseUrl: `${base}/`,
    appId: "1001",
    internalTitle: "Test Game",
    accountLinked: true,
    zone: "prod",
    settings: makeSettings({ fps: 60 }),
  });

  assert.deepEqual(calls, [
    expectedSessionUrl,
  ]);
  assert.equal(networkTestSessionId, null);
});

test("CloudMatch retries transient serverInfo failures before creating a session", async () => {
  const originalFetch = globalThis.fetch;
  const originalWarn = console.warn;
  const originalLog = console.log;
  const calls: string[] = [];
  const expectedSessionUrl = `https://np-lax-01.cloudmatchbeta.nvidiagrid.net/v2/session?${new URLSearchParams({
    keyboardLayout: resolveGfnKeyboardLayout(DEFAULT_KEYBOARD_LAYOUT, process.platform),
    languageCode: "en_US",
  }).toString()}`;

  console.warn = () => {};
  console.log = () => {};
  globalThis.fetch = (async (input, init) => {
    const url = String(input);
    calls.push(url);

    if (url === "https://prod.cloudmatchbeta.nvidiagrid.net/v2/serverInfo") {
      if (calls.filter((entry) => entry === url).length === 1) {
        return new Response("temporary", { status: 503 });
      }

      return new Response(JSON.stringify({
        requestStatus: { statusCode: 1, statusDescription: "SUCCESS_STATUS", serverId: "NP-LAX-01" },
        metaData: [
          { key: "local-region", value: "US West" },
          { key: "gfn-regions", value: "US West" },
          { key: "US West", value: "https://np-lax-01.cloudmatchbeta.nvidiagrid.net/" },
        ],
      }), { status: 200 });
    }

    if (url === expectedSessionUrl) {
      const body = JSON.parse(String(init?.body)) as {
        sessionRequestData: {
          requestedStreamingFeatures?: unknown;
        };
      };
      return new Response(JSON.stringify({
        requestStatus: { statusCode: 1, statusDescription: "SUCCESS_STATUS" },
        session: {
          sessionId: "session-1",
          status: 1,
          seatSetupInfo: { seatSetupStep: 0 },
          sessionControlInfo: { ip: "np-lax-01.cloudmatchbeta.nvidiagrid.net" },
          connectionInfo: [],
          iceServerConfiguration: {
            iceServers: [{ urls: "stun:127.0.0.1:19302" }],
          },
          sessionRequestData: {
            requestedStreamingFeatures: body.sessionRequestData.requestedStreamingFeatures,
          },
        },
      }), { status: 200 });
    }

    throw new Error(`Unexpected fetch: ${url}`);
  }) as typeof fetch;

  try {
    const session = await createSession({
      token: "token",
      streamingBaseUrl: "https://prod.cloudmatchbeta.nvidiagrid.net/",
      appId: "1001",
      internalTitle: "Test Game",
      accountLinked: true,
      zone: "prod",
      settings: makeSettings({ fps: 120 }),
    });

    assert.equal(session.streamingBaseUrl, "https://np-lax-01.cloudmatchbeta.nvidiagrid.net");
    assert.deepEqual(calls, [
      "https://prod.cloudmatchbeta.nvidiagrid.net/v2/serverInfo",
      "https://prod.cloudmatchbeta.nvidiagrid.net/v2/serverInfo",
      expectedSessionUrl,
    ]);
  } finally {
    globalThis.fetch = originalFetch;
    console.warn = originalWarn;
    console.log = originalLog;
  }
});

test("CloudMatch only sends in-game settings persistence when user opt-in and game support are both true", async (t) => {
  const originalFetch = globalThis.fetch;
  const originalLog = console.log;
  console.log = () => {};

  t.after(() => {
    globalThis.fetch = originalFetch;
    console.log = originalLog;
  });

  const persistenceFlags: boolean[] = [];
  const expectedSessionUrl = `https://np-test.example.test/v2/session?${new URLSearchParams({
    keyboardLayout: resolveGfnKeyboardLayout(DEFAULT_KEYBOARD_LAYOUT, process.platform),
    languageCode: "en_US",
  }).toString()}`;

  globalThis.fetch = (async (input, init) => {
    const url = String(input);
    if (url === "https://np-test.example.test/v2/nettestsession") {
      return new Response(JSON.stringify({
        requestStatus: { statusCode: 1, statusDescription: "SUCCESS_STATUS", serverId: "NP-TEST" },
        netTestSession: {
          sessionId: "nettest-persistence",
        },
      }), { status: 200 });
    }

    if (url !== expectedSessionUrl) {
      throw new Error(`Unexpected fetch: ${url}`);
    }

    const body = JSON.parse(String(init?.body)) as {
      sessionRequestData: {
        enablePersistingInGameSettings?: boolean;
      };
    };
    const enablePersistingInGameSettings = body.sessionRequestData.enablePersistingInGameSettings === true;
    persistenceFlags.push(enablePersistingInGameSettings);

    return new Response(JSON.stringify({
      requestStatus: { statusCode: 1, statusDescription: "SUCCESS_STATUS" },
      session: {
        sessionId: `session-${persistenceFlags.length}`,
        status: 1,
        seatSetupInfo: { seatSetupStep: 0 },
        sessionControlInfo: { ip: "np-test.example.test" },
        connectionInfo: [],
        iceServerConfiguration: {
          iceServers: [{ urls: "stun:127.0.0.1:19302" }],
        },
        sessionRequestData: {
          enablePersistingInGameSettings,
        },
      },
    }), { status: 200 });
  }) as typeof fetch;

  const baseRequest = {
    token: "token",
    streamingBaseUrl: "https://np-test.example.test/",
    appId: "1001",
    internalTitle: "Test Game",
    accountLinked: true,
    zone: "prod",
    settings: makeSettings(),
  };

  await createSession({
    ...baseRequest,
    enablePersistingInGameSettings: true,
    supportsInGameSettingsPersistence: false,
  });
  await createSession({
    ...baseRequest,
    enablePersistingInGameSettings: true,
    supportsInGameSettingsPersistence: true,
  });
  await createSession({
    ...baseRequest,
    enablePersistingInGameSettings: false,
    supportsInGameSettingsPersistence: true,
  });

  assert.deepEqual(persistenceFlags, [false, true, false]);
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
          sessionRequestData: { appId: "1001", enablePersistingInGameSettings: true },
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
    assert.equal(sessions[0].enablePersistingInGameSettings, true);
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

test("CloudMatch appLaunchMode maps to official wire values", () => {
  assert.equal(appLaunchModeWireValue(undefined), 1);
  assert.equal(appLaunchModeWireValue("default"), 1);
  assert.equal(appLaunchModeWireValue("gamepadFriendly"), 2);
  assert.equal(appLaunchModeWireValue("touchFriendly"), 3);
});

test("CloudMatch claim keeps the session-stable appLaunchMode over live settings", async () => {
  const originalFetch = globalThis.fetch;
  const originalWarn = console.warn;
  const originalLog = console.log;
  console.warn = () => {};
  console.log = () => {};

  const claimBodies: Array<{
    sessionRequestData: {
      appLaunchMode?: number;
      enablePersistingInGameSettings?: boolean;
    };
  }> = [];

  const readySessionResponse = JSON.stringify({
    requestStatus: { statusCode: 1, statusDescription: "SUCCESS_STATUS" },
    session: {
      sessionId: "sess-1",
      subSessionId: "subsess-1",
      status: 3,
      sessionControlInfo: { ip: "203.0.113.10" },
      connectionInfo: [
        { ip: "203.0.113.10", port: 443, usage: 14, resourcePath: "/nvst/" },
        { ip: "203.0.113.11", port: 49006, usage: 15, protocol: 2 },
        { ip: "203.0.113.13", port: 49007, usage: 17, protocol: 2 },
        {
          ip: "203.0.113.12",
          port: 48322,
          usage: 16,
          protocol: 1,
          appLevelProtocol: 6,
          resourcePath: "rtsps://203.0.113.12:48322/session",
        },
      ],
      iceServerConfiguration: {
        iceServers: [{ urls: "stun:127.0.0.1:19302" }],
      },
      sessionRequestData: {},
    },
  });

  globalThis.fetch = (async (input, init) => {
    const url = String(input);
    if (url.startsWith("https://203.0.113.10/v2/session/sess-1")) {
      if (init?.method === "PUT") {
        claimBodies.push(JSON.parse(String(init.body)));
      }
      return new Response(readySessionResponse, { status: 200 });
    }
    throw new Error(`Unexpected fetch: ${url}`);
  }) as typeof fetch;

  try {
    // Session created as gamepad-friendly (wire 2) must stay gamepad-friendly on
    // resume even if the live settings toggles now say "default".
    const claimed = await claimSession({
      token: "token",
      sessionId: "sess-1",
      serverIp: "203.0.113.10",
      appId: "1001",
      appLaunchMode: 2,
      settings: makeSettings(),
    });
    assert.deepEqual(claimed.connectionInfo, [
      { ip: "203.0.113.10", port: 443, usage: 14, resourcePath: "/nvst/" },
      { ip: "203.0.113.11", port: 49006, usage: 15, protocol: 2 },
      { ip: "203.0.113.13", port: 49007, usage: 17, protocol: 2 },
      {
        ip: "203.0.113.12",
        port: 48322,
        usage: 16,
        protocol: 1,
        appLevelProtocol: 6,
        resourcePath: "rtsps://203.0.113.12:48322/session",
      },
    ]);
    assert.equal(claimed.subSessionId, "subsess-1");
    assert.deepEqual(claimed.mediaConnectionInfo, {
      ip: "203.0.113.13",
      port: 49007,
      usage: 17,
    });
    assert.deepEqual(claimed.rtspsEndpoints, ["rtsps://203.0.113.12:48322/session"]);

    // Without a session-stable value the claim falls back to the settings-derived mode.
    await claimSession({
      token: "token",
      sessionId: "sess-1",
      serverIp: "203.0.113.10",
      appId: "1001",
      enablePersistingInGameSettings: true,
      settings: makeSettings({ appLaunchMode: "gamepadFriendly" }),
    });

    await claimSession({
      token: "token",
      sessionId: "sess-1",
      serverIp: "203.0.113.10",
      appId: "1001",
      settings: makeSettings(),
    });

    assert.equal(claimBodies.length, 3);
    assert.equal(claimBodies[0].sessionRequestData.appLaunchMode, 2);
    assert.equal(claimBodies[1].sessionRequestData.appLaunchMode, 2);
    assert.equal(claimBodies[2].sessionRequestData.appLaunchMode, 1);
    assert.equal(claimBodies[0].sessionRequestData.enablePersistingInGameSettings, false);
    assert.equal(claimBodies[1].sessionRequestData.enablePersistingInGameSettings, true);
    assert.equal(claimBodies[2].sessionRequestData.enablePersistingInGameSettings, false);
  } finally {
    globalThis.fetch = originalFetch;
    console.warn = originalWarn;
    console.log = originalLog;
  }
});
