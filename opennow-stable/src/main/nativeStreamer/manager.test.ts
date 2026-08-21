import assert from "node:assert/strict";
import type { ChildProcessWithoutNullStreams } from "node:child_process";
import { EventEmitter } from "node:events";
import test from "node:test";

import type {
  NativeStreamerCapabilities,
  NativeStreamerEvent,
  NativeStreamerResponse,
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
  capabilities: NativeStreamerCapabilities | null;
  pending: Map<string, unknown>;
  request(
    input: NativeStreamerCommandInput,
    timeoutMs: number,
  ): Promise<NativeStreamerResponse>;
  installStdinErrorHandler(child: ChildProcessWithoutNullStreams): void;
  handleStdout(child: ChildProcessWithoutNullStreams, chunk: string): void;
  handleEvent(message: NativeStreamerEvent): void;
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

function createManager(): {
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
    emit: () => undefined,
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
    protocolVersion: 4,
    backend: "native",
    supportsOfferAnswer: true,
    supportsRemoteIce: true,
    supportsLocalIce: true,
    supportsInput: true,
    supportsVideoDecode: true,
    supportsVideoPresent: true,
  };
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
    protocolVersion: 4,
    backend: "native",
    supportsOfferAnswer: true,
    supportsRemoteIce: true,
    supportsLocalIce: true,
    supportsInput: true,
    supportsVideoDecode: true,
    supportsVideoPresent: true,
  };

  assert.throws(
    () => manager.sendInput({ payloadBase64: "AQ==" }),
    (error) => error === programmingFailure,
  );
  assert.equal(internals.child, secondFake.child);
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
