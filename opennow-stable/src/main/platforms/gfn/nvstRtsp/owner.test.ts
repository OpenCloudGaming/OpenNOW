/// <reference types="node" />

import assert from "node:assert/strict";
import test from "node:test";

import type { NativeStreamerSessionContext } from "@shared/gfn";
import {
  GfnNvstRtspSessionOwner,
  GfnNvstUnavailableError,
} from "./owner";
import type { NvstRtspSession } from "./probe";

function createContext(
  sessionId: string,
  transportMode: "nvst" | "webrtc" = "nvst",
  withEndpoints = true,
): NativeStreamerSessionContext {
  return {
    session: {
      sessionId,
      status: 2,
      zone: "test-zone",
      serverIp: "192.0.2.10",
      signalingServer: "signal.example",
      signalingUrl: "wss://signal.example/session",
      mediaConnectionInfo: { ip: "198.51.100.20", port: 5006, usage: 17 },
      rtspsEndpoints: withEndpoints ? [`rtsps://rtsp.example:322/${sessionId}`] : undefined,
      iceServers: [],
    },
    settings: {
      resolution: "1920x1080",
      fps: 60,
      maxBitrateMbps: 80,
      codec: "H265",
      transportMode,
    } as NativeStreamerSessionContext["settings"],
    shortcuts: {} as NativeStreamerSessionContext["shortcuts"],
  };
}

function createRtspSession(
  sessionId: string,
  onRelease: (reason: string) => void,
  isHealthy: () => boolean = () => true,
): NvstRtspSession {
  return {
    endpoint: `rtsps://rtsp.example:322/${sessionId}`,
    session: `rtsp-${sessionId}`,
    hmacSeedPresent: true,
    videoPeer: { ip: "192.0.2.20", port: 5004 },
    clientUdpPort: 45000,
    srtp: {
      aesKeyHex: "AA".repeat(32),
      keyId: 42,
      masterKeySaltHex: `${"AA".repeat(32)}${"00".repeat(11)}2A`,
      saltHex: `${"00".repeat(11)}2A`,
      clientGenerated: false,
    },
    videoSession: {
      clientUdpPort: 45000,
      videoPeerIp: "192.0.2.20",
      videoPeerPort: 5004,
      srtpAesKeyHex: "AA".repeat(32),
      srtpKeyId: 42,
      srtpSaltHex: `${"00".repeat(11)}2A`,
      codec: "H265",
    },
    steps: ["wss-open", "options", "describe", "setup-video", "announce", "play"],
    isHealthy,
    handoffVideoUdp: async () => undefined,
    release: async (reason = "released") => onRelease(reason),
  };
}

test("owner retains one negotiated control session and reuses its video handoff", async () => {
  const releases: string[] = [];
  let negotiations = 0;
  let codec: string | undefined;
  let fps: number | undefined;
  let maxBitrateKbps: number | undefined;
  let bundlePeer: { ip: string; port: number } | undefined;
  const owner = new GfnNvstRtspSessionOwner({
    negotiate: async (input) => {
      const { sessionId } = input;
      negotiations += 1;
      codec = input.codec;
      fps = input.fps;
      maxBitrateKbps = input.maxBitrateKbps;
      bundlePeer = input.bundlePeer;
      return createRtspSession(sessionId, (reason) => releases.push(reason));
    },
  });

  const first = await owner.prepare(createContext("same-session"));
  const duplicate = await owner.prepare(createContext("same-session"));

  assert.equal(negotiations, 1);
  assert.equal(codec, "H265");
  assert.equal(fps, 60);
  assert.equal(maxBitrateKbps, 80_000);
  assert.deepEqual(bundlePeer, { ip: "198.51.100.20", port: 5006, usage: 17 });
  assert.deepEqual(duplicate.nvstVideo, first.nvstVideo);
  assert.deepEqual(releases, []);

  await owner.release("stream stopped");
  await owner.release("duplicate stop");
  assert.deepEqual(releases, ["stream stopped"]);
});

test("owner caps native NVST negotiation at 240 FPS and prefers the negotiated profile", async () => {
  let fps: number | undefined;
  let codec: string | undefined;
  const owner = new GfnNvstRtspSessionOwner({
    negotiate: async (input) => {
      fps = input.fps;
      codec = input.codec;
      return createRtspSession(input.sessionId, () => undefined);
    },
  });
  const context = createContext("high-fps");
  context.settings.fps = 360;
  context.settings.codec = "H264";
  context.session.negotiatedStreamProfile = { fps: 300, codec: "AV1" };

  await owner.prepare(context);

  assert.equal(fps, 240);
  assert.equal(codec, "AV1");
  await owner.release("test complete");
});

test("owner tears down the previous session before negotiating its replacement", async () => {
  const events: string[] = [];
  const owner = new GfnNvstRtspSessionOwner({
    negotiate: async ({ sessionId }) => {
      events.push(`negotiate:${sessionId}`);
      return createRtspSession(sessionId, (reason) => {
        events.push(`release:${sessionId}:${reason}`);
      });
    },
  });

  await owner.prepare(createContext("first"));
  await owner.prepare(createContext("second"));

  assert.deepEqual(events, [
    "negotiate:first",
    "release:first:replaced by GFN session second",
    "negotiate:second",
  ]);
  await owner.release("test complete");
});

test("owner renegotiates instead of reusing an unhealthy same-session client", async () => {
  const events: string[] = [];
  let negotiations = 0;
  const owner = new GfnNvstRtspSessionOwner({
    negotiate: async ({ sessionId }) => {
      negotiations += 1;
      const negotiation = negotiations;
      return createRtspSession(
        sessionId,
        (reason) => events.push(`release:${negotiation}:${reason}`),
        () => negotiation > 1,
      );
    },
  });

  await owner.prepare(createContext("same-session"));
  await owner.prepare(createContext("same-session"));

  assert.equal(negotiations, 2);
  assert.deepEqual(events, ["release:1:replaced by GFN session same-session"]);
  await owner.release("test complete");
});

test("owner releases NVST when a WebRTC native context replaces it", async () => {
  const releases: string[] = [];
  const owner = new GfnNvstRtspSessionOwner({
    negotiate: async ({ sessionId }) =>
      createRtspSession(sessionId, (reason) => releases.push(reason)),
  });

  await owner.prepare(createContext("first"));
  const webRtcContext = await owner.prepare(createContext("second", "webrtc"));

  assert.equal(webRtcContext.nvstVideo, undefined);
  assert.deepEqual(releases, ["native transport is not NVST"]);
});

test("owner reports typed unavailability for missing endpoints and negotiation failure", async () => {
  const owner = new GfnNvstRtspSessionOwner({
    negotiate: async () => {
      throw new Error("synthetic RTSP failure");
    },
  });

  await assert.rejects(
    owner.prepare(createContext("missing", "nvst", false)),
    (error: unknown) =>
      error instanceof GfnNvstUnavailableError
      && error.code === "missing-rtsps-endpoints",
  );
  await assert.rejects(
    owner.prepare(createContext("failed")),
    (error: unknown) =>
      error instanceof GfnNvstUnavailableError
      && error.code === "negotiation-failed"
      && /synthetic RTSP failure/.test(error.message),
  );
});
