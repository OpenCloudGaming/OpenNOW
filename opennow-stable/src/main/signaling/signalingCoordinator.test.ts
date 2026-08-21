/// <reference types="node" />

import assert from "node:assert/strict";
import test from "node:test";

import type { NativeStreamerSessionContext } from "@shared/gfn";
import type { GfnNvstRtspOwner } from "../platforms/gfn/nvstRtsp/owner";
import type { NativeStreamerManager } from "../nativeStreamer/manager";
import type { SettingsManager } from "../settings";
import { SignalingCoordinator } from "./signalingCoordinator";

function createContext(): NativeStreamerSessionContext {
  return {
    session: {
      sessionId: "gfn-session",
      status: 2,
      zone: "test-zone",
      serverIp: "192.0.2.10",
      signalingServer: "signal.example",
      signalingUrl: "wss://signal.example/session",
      rtspsEndpoints: ["rtsps://rtsp.example:322/gfn-session"],
      iceServers: [],
    },
    settings: {
      resolution: "1920x1080",
      fps: 60,
      codec: "H265",
      transportMode: "nvst",
    } as NativeStreamerSessionContext["settings"],
    shortcuts: {} as NativeStreamerSessionContext["shortcuts"],
  };
}

interface CoordinatorInternals {
  nativeStreamerContext: NativeStreamerSessionContext | null;
  nativeStreamerManager: NativeStreamerManager | null;
  prepareNativeStreamerBeforeSignaling(): Promise<void>;
  routeSignalingEvent(event: { type: "offer"; sdp: string } | { type: "remote-ice"; candidate: string }): void;
}

function createCoordinator(
  owner: GfnNvstRtspOwner,
): { coordinator: SignalingCoordinator; internals: CoordinatorInternals } {
  const coordinator = new SignalingCoordinator({
    ipcMain: {} as never,
    mainDir: "",
    settingsManager: {
      get: (key: string) => key === "streamClientMode" ? "native" : undefined,
    } as unknown as SettingsManager,
    getMainWindow: () => null,
    gfnNvstRtspOwner: owner,
  });
  return {
    coordinator,
    internals: coordinator as unknown as CoordinatorInternals,
  };
}

test("coordinator starts native after NVST prepare without dropping the reserved socket", async () => {
  const events: string[] = [];
  const owner: GfnNvstRtspOwner = {
    prepare: async (context) => {
      events.push("nvst-prepare");
      return {
        ...context,
        nvstVideo: {
          clientUdpPort: 45000,
          videoPeerIp: "192.0.2.20",
          videoPeerPort: 5004,
          srtpAesKeyHex: "AA".repeat(32),
          srtpKeyId: 42,
          srtpSaltHex: `${"00".repeat(11)}2A`,
        },
      };
    },
    videoUdpFd: () => undefined,
    handoffVideoUdp: async () => {
      events.push("nvst-handoff-video-udp");
    },
    release: async (reason) => {
      events.push(`nvst-release:${reason}`);
    },
  };
  const { internals } = createCoordinator(owner);
  internals.nativeStreamerContext = createContext();
  internals.nativeStreamerManager = {
    prepareForSession: async (context: NativeStreamerSessionContext) => {
      assert.equal(context.nvstVideo?.clientUdpPort, 45000);
      events.push("native-prepare");
    },
  } as unknown as NativeStreamerManager;

  await internals.prepareNativeStreamerBeforeSignaling();

  assert.deepEqual(events, ["nvst-prepare", "native-prepare", "nvst-handoff-video-udp"]);
  assert.equal(internals.nativeStreamerContext?.nvstVideo?.videoPeerPort, 5004);
});

test("coordinator releases retained NVST control on explicit native stop", async () => {
  const events: string[] = [];
  const owner: GfnNvstRtspOwner = {
    prepare: async (context) => context,
    videoUdpFd: () => undefined,
    handoffVideoUdp: async () => undefined,
    release: async (reason) => {
      events.push(`nvst-release:${reason}`);
    },
  };
  const { coordinator, internals } = createCoordinator(owner);
  internals.nativeStreamerManager = {
    stop: async (reason: string) => {
      events.push(`native-stop:${reason}`);
    },
  } as unknown as NativeStreamerManager;

  await coordinator.stopNativeStreamer("test stop");

  assert.deepEqual(events, ["native-stop:test stop", "nvst-release:test stop"]);
});

test("coordinator tears down explicit NVST without falling back to WebRTC", async () => {
  const events: string[] = [];
  const owner: GfnNvstRtspOwner = {
    prepare: async (context) => {
      events.push("nvst-prepare");
      return context;
    },
    videoUdpFd: () => undefined,
    handoffVideoUdp: async () => {
      events.push("nvst-handoff-video-udp");
    },
    release: async (reason) => {
      events.push(`nvst-release:${reason}`);
    },
  };
  const { internals } = createCoordinator(owner);
  internals.nativeStreamerContext = createContext();
  internals.nativeStreamerManager = {
    prepareForSession: async () => {
      events.push("native-prepare");
      throw new Error("synthetic native start failure");
    },
    stop: async (reason: string) => {
      events.push(`native-stop:${reason}`);
    },
  } as unknown as NativeStreamerManager;

  await assert.rejects(
    internals.prepareNativeStreamerBeforeSignaling(),
    /synthetic native start failure/,
  );

  assert.deepEqual(events, [
    "nvst-prepare",
    "native-prepare",
    "native-stop:explicit NVST pre-attach startup failed",
    "nvst-release:explicit NVST pre-attach startup failed",
  ]);
});

test("coordinator does not renegotiate WebRTC after native NVST starts", () => {
  const owner: GfnNvstRtspOwner = {
    prepare: async (context) => context,
    videoUdpFd: () => undefined,
    handoffVideoUdp: async () => undefined,
    release: async () => undefined,
  };
  const { internals } = createCoordinator(owner);
  const context = createContext();
  context.nvstVideo = {
    clientUdpPort: 45000,
    videoPeerIp: "192.0.2.20",
    videoPeerPort: 5004,
    srtpAesKeyHex: "AA".repeat(32),
    srtpKeyId: 42,
    srtpSaltHex: `${"00".repeat(11)}2A`,
  };
  internals.nativeStreamerContext = context;
  internals.nativeStreamerManager = {
    isNvstSessionActive: (sessionId: string) => sessionId === "gfn-session",
    handleOffer: async () => assert.fail("NVST must not handle a WebRTC offer"),
    addRemoteIce: async () => assert.fail("NVST must not handle WebRTC ICE"),
  } as unknown as NativeStreamerManager;

  internals.routeSignalingEvent({ type: "offer", sdp: "v=0\r\n" });
  internals.routeSignalingEvent({ type: "remote-ice", candidate: "candidate:synthetic" });
});
