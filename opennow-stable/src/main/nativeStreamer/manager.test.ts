import assert from "node:assert/strict";
import type { ChildProcessWithoutNullStreams } from "node:child_process";
import { EventEmitter } from "node:events";
import test from "node:test";

import type {
  MainToRendererSignalingEvent,
  NativeStreamerSessionContext,
} from "@shared/gfn";
import {
  NATIVE_STREAMER_PROTOCOL_VERSION,
  type NativeStreamerCapabilities,
  type NativeStreamerActiveTransportCapabilities,
  type NativeStreamerEvent,
  type NativeStreamerResponse,
} from "@shared/nativeStreamer";
import { NativeStreamerManager } from "./manager";
import type { NativeStreamerCommandInput } from "./protocol";

type WriteCallback = (error?: Error | null) => void;

class FakeStdin extends EventEmitter {
  destroyed = false;
  writable = true;
  writableEnded = false;
  writableLength = 0;
  writeImpl: (
    chunk: string,
    encoding: string,
    callback: WriteCallback,
  ) => boolean = () => true;

  write(chunk: string, encoding: string, callback: WriteCallback): boolean {
    return this.writeImpl(chunk, encoding, callback);
  }
}

interface FakeChild {
  child: ChildProcessWithoutNullStreams;
  stdin: FakeStdin;
  wasKilled(): boolean;
}

interface ManagerInternals {
  child: ChildProcessWithoutNullStreams | null;
  stdoutBuffer: string;
  activeSessionId: string | null;
  activeTransport: "webrtc" | "nvst" | null;
  capabilities: NativeStreamerCapabilities | null;
  activeTransportCapabilities: NativeStreamerActiveTransportCapabilities | null;
  inputReady: boolean;
  nativeInputOwner: "electron" | "native";
  nvstTransportReady: boolean;
  nvstReadinessError: Error | null;
  pending: Map<string, unknown>;
  request(
    input: NativeStreamerCommandInput,
    timeoutMs: number,
  ): Promise<NativeStreamerResponse>;
  installStdinErrorHandler(child: ChildProcessWithoutNullStreams): void;
  handleStdout(child: ChildProcessWithoutNullStreams, chunk: string): void;
  handleEvent(message: NativeStreamerEvent): void;
  ensureProcess(): Promise<void>;
}

test("stdout from a replaced native process cannot affect the current session", () => {
  const { internals } = createManager();
  const current = createFakeChild();
  const stale = createFakeChild();
  internals.child = current.child;

  internals.handleStdout(stale.child, "stale partial output");
  assert.equal(internals.stdoutBuffer, "");

  internals.handleStdout(current.child, "current partial output");
  assert.equal(internals.stdoutBuffer, "current partial output");
});

function createFakeChild(): FakeChild {
  const stdin = new FakeStdin();
  let killed = false;
  const child = {
    stdin,
    get killed() {
      return killed;
    },
    kill() {
      killed = true;
      return true;
    },
  } as unknown as ChildProcessWithoutNullStreams;
  return {
    child,
    stdin,
    wasKilled: () => killed,
  };
}

function createManager(emitted: MainToRendererSignalingEvent[] = []): {
  manager: NativeStreamerManager;
  internals: ManagerInternals;
} {
  const manager = new NativeStreamerManager({
    mainDir: "",
    getVideoBackendPreference: () => "auto",
    getExecutablePathOverride: () => "",
    getCloudGsyncMode: () => "auto",
    getD3dFullscreenMode: () => "auto",
    getExternalRendererEnabled: () => false,
    sendAnswer: async () => undefined,
    sendIceCandidate: async () => undefined,
    requestKeyframe: async () => undefined,
    emit: (event) => emitted.push(event),
    retryWithSoftwareDecoder: () => undefined,
  });
  return {
    manager,
    internals: manager as unknown as ManagerInternals,
  };
}

test("Linux decoder startup timeout requests one native software retry", () => {
  if (process.platform !== "linux") return;

  let recoveryMessage = "";
  const manager = new NativeStreamerManager({
    mainDir: "",
    getVideoBackendPreference: () => "nvdec",
    getExecutablePathOverride: () => "",
    getCloudGsyncMode: () => "auto",
    getD3dFullscreenMode: () => "auto",
    getExternalRendererEnabled: () => false,
    sendAnswer: async () => undefined,
    sendIceCandidate: async () => undefined,
    requestKeyframe: async () => undefined,
    emit: () => undefined,
    retryWithSoftwareDecoder: (message) => {
      recoveryMessage = message;
    },
  });

  (manager as unknown as ManagerInternals).handleEvent({
    type: "error",
    code: "native-video-decoder-startup-timeout",
    message: "decoder produced no frames",
  });

  assert.equal(recoveryMessage, "decoder produced no frames");
});

function writeError(code: string): NodeJS.ErrnoException {
  const error = new Error(`${code}: write failed`) as NodeJS.ErrnoException;
  error.code = code;
  return error;
}

test("command writes reject and clear pending state after a synchronous broken pipe", async () => {
  const { internals } = createManager();
  const fake = createFakeChild();
  const failure = writeError("EPIPE");
  fake.stdin.writeImpl = () => {
    throw failure;
  };
  internals.child = fake.child;

  await assert.rejects(
    internals.request({ type: "stop", reason: "test" }, 1_000),
    (error) => error === failure,
  );
  assert.equal(internals.pending.size, 0);
  assert.equal(internals.child, null);
  assert.equal(fake.wasKilled(), true);
});

test("stdin error events reject every pending request and remain handled after exit", async () => {
  const { internals } = createManager();
  const fake = createFakeChild();
  internals.child = fake.child;
  internals.installStdinErrorHandler(fake.child);

  const first = internals.request({ type: "stop", reason: "first" }, 1_000);
  const second = internals.request({ type: "stop", reason: "second" }, 1_000);
  assert.equal(internals.pending.size, 2);

  const failure = writeError("EIO");
  fake.stdin.emit("error", failure);

  await Promise.all([
    assert.rejects(first, (error) => error === failure),
    assert.rejects(second, (error) => error === failure),
  ]);
  assert.equal(internals.pending.size, 0);
  assert.equal(internals.child, null);
  assert.doesNotThrow(() => fake.stdin.emit("error", writeError("EPIPE")));
});

test("input writes tolerate a child exit race but still throw unrelated failures", () => {
  const { manager, internals } = createManager();
  const fake = createFakeChild();
  internals.child = fake.child;
  internals.activeSessionId = "session";
  internals.capabilities = {
    protocolVersion: NATIVE_STREAMER_PROTOCOL_VERSION,
    backend: "native",
    supportsOfferAnswer: true,
    supportsRemoteIce: true,
    supportsLocalIce: true,
    supportsInput: true,
    supportsVideoDecode: true,
    supportsVideoPresent: true,
  };
  internals.activeTransportCapabilities = activeInputCapabilities();
  internals.inputReady = true;
  fake.stdin.writeImpl = () => {
    throw writeError("ERR_STREAM_DESTROYED");
  };

  assert.doesNotThrow(() => manager.sendInput({
    payloadBase64: "AQ==",
    partiallyReliable: true,
  }));
  assert.equal(internals.child, null);

  const programmingFailure = new Error("bad writable implementation");
  const secondFake = createFakeChild();
  secondFake.stdin.writeImpl = () => {
    throw programmingFailure;
  };
  internals.child = secondFake.child;
  internals.activeSessionId = "session";
  internals.capabilities = {
    protocolVersion: NATIVE_STREAMER_PROTOCOL_VERSION,
    backend: "native",
    supportsOfferAnswer: true,
    supportsRemoteIce: true,
    supportsLocalIce: true,
    supportsInput: true,
    supportsVideoDecode: true,
    supportsVideoPresent: true,
  };
  internals.activeTransportCapabilities = activeInputCapabilities();
  internals.inputReady = true;

  assert.throws(
    () => manager.sendInput({ payloadBase64: "AQ==" }),
    (error) => error === programmingFailure,
  );
  assert.equal(internals.child, secondFake.child);
});

function activeInputCapabilities(): NativeStreamerActiveTransportCapabilities {
  return {
    supportsOfferAnswer: false,
    supportsRemoteIce: false,
    supportsLocalIce: false,
    supportsInput: true,
    supportsAudioDecode: true,
    supportsAudioOutput: true,
  };
}

test("input is suppressed until the active transport reports its channels ready", () => {
  const { manager, internals } = createManager();
  const fake = createFakeChild();
  let writes = 0;
  fake.stdin.writeImpl = () => {
    writes += 1;
    return true;
  };
  internals.child = fake.child;
  internals.activeSessionId = "session";
  internals.capabilities = {
    protocolVersion: NATIVE_STREAMER_PROTOCOL_VERSION,
    backend: "native",
    supportsOfferAnswer: true,
    supportsRemoteIce: true,
    supportsLocalIce: true,
    supportsInput: true,
    supportsVideoDecode: true,
    supportsVideoPresent: true,
  };
  internals.activeTransportCapabilities = activeInputCapabilities();

  manager.sendInput({ payloadBase64: "AQ==" });
  assert.equal(writes, 0);

  internals.handleEvent({ type: "input-ready", protocolVersion: 3 });
  manager.sendInput({ payloadBase64: "AQ==" });
  assert.equal(writes, 1);
});

test("input readiness reports the native window owner to the renderer", () => {
  const emitted: MainToRendererSignalingEvent[] = [];
  const { internals } = createManager(emitted);
  internals.activeSessionId = "wayland-session";
  internals.activeTransportCapabilities = activeInputCapabilities();
  internals.nativeInputOwner = "native";

  internals.handleEvent({ type: "input-ready", protocolVersion: 3 });

  assert.deepEqual(emitted, [{
    type: "native-input-ready",
    protocolVersion: 3,
    inputOwner: "native",
  }]);
});

test("input unavailable clears readiness and forwards the native reason", () => {
  const emitted: MainToRendererSignalingEvent[] = [];
  const { internals } = createManager(emitted);
  internals.activeTransportCapabilities = activeInputCapabilities();
  internals.inputReady = true;

  internals.handleEvent({ type: "input-unavailable", reason: "reliable channel failed" });

  assert.equal(internals.inputReady, false);
  assert.deepEqual(emitted, [{
    type: "native-input-unavailable",
    reason: "reliable channel failed",
  }]);
});

test("native input responses preserve success and report SCTP write failures", () => {
  const emitted: MainToRendererSignalingEvent[] = [];
  const { manager, internals } = createManager(emitted);
  const fake = createFakeChild();
  const commandIds: string[] = [];
  fake.stdin.writeImpl = (chunk) => {
    commandIds.push((JSON.parse(chunk) as { id: string }).id);
    return true;
  };
  internals.child = fake.child;
  internals.activeSessionId = "session";
  internals.capabilities = {
    protocolVersion: NATIVE_STREAMER_PROTOCOL_VERSION,
    backend: "native",
    supportsOfferAnswer: true,
    supportsRemoteIce: true,
    supportsLocalIce: true,
    supportsInput: true,
    supportsVideoDecode: true,
    supportsVideoPresent: true,
  };
  internals.activeTransportCapabilities = activeInputCapabilities();
  internals.inputReady = true;

  manager.sendInput({ payloadBase64: "AQ==" });
  assert.equal(internals.pending.size, 1);
  internals.handleStdout(fake.child, `${JSON.stringify({
    id: commandIds[0],
    type: "ok",
  })}\n`);
  assert.equal(internals.pending.size, 0);
  assert.equal(internals.inputReady, true);
  assert.deepEqual(emitted, []);

  manager.sendInput({ payloadBase64: "Ag==" });
  assert.equal(internals.pending.size, 1);
  internals.handleStdout(fake.child, `${JSON.stringify({
    id: commandIds[1],
    type: "error",
    code: "input-write-failed",
    message: "reliable SCTP write failed",
  })}\n`);

  assert.equal(internals.pending.size, 0);
  assert.equal(internals.inputReady, false);
  assert.deepEqual(emitted, [{
    type: "native-input-unavailable",
    reason: "Native input write failed: input-write-failed: reliable SCTP write failed",
  }]);
});

test("terminal stopped clears ownership so same-session prepare starts again", async () => {
  const { manager, internals } = createManager();
  internals.activeSessionId = "same-session";
  internals.activeTransport = "nvst";
  internals.activeTransportCapabilities = activeInputCapabilities();
  internals.inputReady = true;
  internals.ensureProcess = async () => undefined;
  let starts = 0;
  internals.request = async (input) => {
    assert.equal(input.type, "start");
    starts += 1;
    return {
      id: "start",
      type: "ok",
      transport: "nvst",
      capabilities: activeInputCapabilities(),
    };
  };

  internals.handleEvent({ type: "status", status: "stopped", message: "remote ended" });
  assert.equal(internals.activeSessionId, null);
  assert.equal(internals.activeTransport, null);
  assert.equal(internals.activeTransportCapabilities, null);
  assert.equal(internals.inputReady, false);

  await manager.prepareForSession({
    session: { sessionId: "same-session" },
    settings: {
      resolution: "1920x1080",
      fps: 60,
      codec: "H264",
      transportMode: "nvst",
      enableCloudGsync: false,
    },
  } as NativeStreamerSessionContext);

  assert.equal(starts, 1);
  assert.equal(internals.activeSessionId, "same-session");
});

test("native NVST reservation release sends an idempotent unbind command", async () => {
  const { manager, internals } = createManager();
  internals.ensureProcess = async () => undefined;
  let unbinds = 0;
  internals.request = async (input) => {
    if (input.type === "nvst-bind") {
      return { id: "bind", type: "nvst-bound", port: 45_000 };
    }
    assert.equal(input.type, "nvst-unbind");
    unbinds += 1;
    return { id: "unbind", type: "ok" };
  };

  const reservation = await manager.reserveNvstUdp();
  await reservation.release();
  await reservation.release();
  assert.equal(unbinds, 1);
});

test("native NVST start takes ownership without unbinding its reserved socket", async () => {
  const { manager, internals } = createManager();
  internals.ensureProcess = async () => undefined;
  const commands: string[] = [];
  internals.request = async (input) => {
    commands.push(input.type);
    if (input.type === "nvst-bind") {
      return { id: "bind", type: "nvst-bound", port: 45_000 };
    }
    if (input.type === "start") {
      return {
        id: "start",
        type: "ok",
        transport: "nvst",
        capabilities: activeInputCapabilities(),
      };
    }
    assert.fail(`Unexpected command after native NVST ownership transfer: ${input.type}`);
  };

  const reservation = await manager.reserveNvstUdp();
  await manager.prepareForSession({
    session: { sessionId: "native-owner" },
    settings: {
      resolution: "1920x1080",
      fps: 60,
      codec: "H264",
      transportMode: "nvst",
      enableCloudGsync: false,
    },
    nvstVideo: { clientUdpPort: 45_000 },
  } as NativeStreamerSessionContext);
  await reservation.release();

  assert.deepEqual(commands, ["nvst-bind", "start"]);
});

test("NVST readiness waits for the native SCTP transport event", async () => {
  const { manager, internals } = createManager();
  internals.activeSessionId = "ready-session";
  internals.activeTransport = "nvst";
  internals.activeTransportCapabilities = activeInputCapabilities();

  const ready = manager.waitForNvstTransportReady("ready-session", 1_000);
  internals.handleEvent({ type: "nvst-transport-ready", phase: "dtls" });
  assert.equal(internals.nvstTransportReady, false);
  internals.handleEvent({ type: "nvst-transport-ready", phase: "sctp" });
  await ready;
  assert.equal(internals.nvstTransportReady, true);
});

test("NVST readiness is bounded when native DTLS/SCTP never becomes ready", async () => {
  const { manager, internals } = createManager();
  internals.activeSessionId = "timeout-session";
  internals.activeTransport = "nvst";
  internals.activeTransportCapabilities = activeInputCapabilities();

  await assert.rejects(
    manager.waitForNvstTransportReady("timeout-session", 5),
    /DTLS\/SCTP readiness timed out after 5ms/,
  );
});

test("NVST readiness survives an input-ready event racing the start response", async () => {
  const { manager, internals } = createManager();
  internals.ensureProcess = async () => undefined;
  internals.request = async (input) => {
    assert.equal(input.type, "start");
    internals.handleEvent({ type: "input-ready", protocolVersion: 3 });
    return {
      id: "start",
      type: "ok",
      transport: "nvst",
      capabilities: activeInputCapabilities(),
    };
  };

  await manager.prepareForSession({
    session: { sessionId: "racing-session" },
    settings: {
      resolution: "1920x1080",
      fps: 60,
      codec: "H264",
      transportMode: "nvst",
      enableCloudGsync: false,
    },
  } as NativeStreamerSessionContext);

  await manager.waitForNvstTransportReady("racing-session", 5);
  assert.equal(internals.inputReady, true);
});

test("NVST readiness preserves input failure racing the start response", async () => {
  const { manager, internals } = createManager();
  internals.ensureProcess = async () => undefined;
  internals.request = async (input) => {
    assert.equal(input.type, "start");
    internals.handleEvent({ type: "input-unavailable", reason: "handshake failed" });
    return {
      id: "start",
      type: "ok",
      transport: "nvst",
      capabilities: activeInputCapabilities(),
    };
  };

  await manager.prepareForSession({
    session: { sessionId: "failing-race-session" },
    settings: {
      resolution: "1920x1080",
      fps: 60,
      codec: "H264",
      transportMode: "nvst",
      enableCloudGsync: false,
    },
  } as NativeStreamerSessionContext);

  await assert.rejects(
    manager.waitForNvstTransportReady("failing-race-session", 1_000),
    /handshake failed/,
  );
  assert.equal(internals.nvstReadinessError?.message.includes("handshake failed"), true);
});

test("unrelated synchronous command write failures reject only their request", async () => {
  const { internals } = createManager();
  const fake = createFakeChild();
  const failure = new Error("bad writable implementation");
  fake.stdin.writeImpl = () => {
    throw failure;
  };
  internals.child = fake.child;

  await assert.rejects(
    internals.request({ type: "stop", reason: "test" }, 1_000),
    (error) => error === failure,
  );
  assert.equal(internals.pending.size, 0);
  assert.equal(internals.child, fake.child);
});
