/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import type { Duplex } from "node:stream";

import {
  buildRtspRequest,
  extractVideoPeer,
  parseRtspResponse,
  RTSPS_WS_KEEPALIVE_INTERVAL_MS,
  RtspOverWssClient,
} from "./rtspClient";

test("RTSPS WebSocket keepalive uses the official two-second cadence", () => {
  assert.equal(RTSPS_WS_KEEPALIVE_INTERVAL_MS, 2_000);
});

class FakeSocket extends EventEmitter {
  destroyed = false;
  readonly writes: Buffer[] = [];

  write(chunk: Uint8Array | string): boolean {
    this.writes.push(Buffer.from(chunk));
    return true;
  }

  destroy(): this {
    if (!this.destroyed) {
      this.destroyed = true;
      this.emit("close");
    }
    return this;
  }
}

function serverFrame(opcode: number, payload: Buffer): Buffer {
  assert.ok(payload.length < 126);
  return Buffer.concat([Buffer.from([0x80 | opcode, payload.length]), payload]);
}

function rtspResponse(cseq: number): Buffer {
  return serverFrame(
    0x1,
    Buffer.from(`RTSP/1.0 200 OK\r\nCSeq: ${cseq}\r\nContent-Length: 0\r\n\r\n`),
  );
}

function createClient(timeoutMs = 50): { client: RtspOverWssClient; socket: FakeSocket } {
  const socket = new FakeSocket();
  const client = new RtspOverWssClient(
    "host.example",
    322,
    timeoutMs,
    undefined,
    async () => socket as unknown as Duplex,
  );
  return { client, socket };
}

test("extractVideoPeer prefers SETUP X-GS-ServerPort", () => {
  assert.deepEqual(
    extractVideoPeer("unicast;X-GS-ServerPort=5004-5005;source=80.250.97.37"),
    { ip: "80.250.97.37", port: 5004 },
  );
});

test("buildRtspRequest keeps empty Transport and only sends Content-Length with a body", () => {
  const setup = buildRtspRequest("SETUP", "streamid=video/0/0", {
    "X-GS-Version": "14.2",
    Host: "host.example:322",
    Session: "XNV2060633119",
    Transport: "",
  }, "", 3);
  assert.equal(
    setup,
    [
      "SETUP streamid=video/0/0 RTSP/1.0",
      "CSeq: 3",
      "Request-Id: 3",
      "X-GS-Version: 14.2",
      "Host: host.example:322",
      "Session: XNV2060633119",
      "Transport: ",
      "",
      "",
    ].join("\r\n"),
  );
  assert.match(setup, /^Transport: $/m);
  assert.doesNotMatch(setup, /Content-Length/);

  const announce = buildRtspRequest("ANNOUNCE", "rtsps://host.example:322", {
    "Content-Type": "application/sdp",
  }, "v=0\r\n", 4);
  assert.match(announce, /\r\nContent-Length: 5\r\n\r\nv=0\r\n$/);
});

test("parseRtspResponse preserves status, normalized headers, and SDP body", () => {
  const response = parseRtspResponse(
    "RTSP/1.0 200 OK\r\nCSeq: 2\r\nSession: session-id;timeout=60\r\nContent-Length: 5\r\n\r\nv=0\r\n",
  );
  assert.deepEqual(response, {
    statusCode: 200,
    statusText: "OK",
    headers: {
      cseq: "2",
      session: "session-id;timeout=60",
      "content-length": "5",
    },
    body: "v=0\n",
  });
});

test("RTSP client validates response CSeq and destroys mismatched connections", async () => {
  const { client, socket } = createClient();
  await client.connect("session");
  const response = client.request("OPTIONS", "rtsps://host.example:322");

  socket.emit("data", rtspResponse(9));

  await assert.rejects(response, /CSeq mismatch: expected 1, received 9/);
  assert.equal(client.isHealthy(), false);
  assert.equal(socket.destroyed, true);
});

test("late RTSP responses cannot satisfy a request after timeout", async () => {
  const { client, socket } = createClient(5);
  await client.connect("session");

  await assert.rejects(
    client.request("OPTIONS", "rtsps://host.example:322"),
    /timed out after 5ms/,
  );
  assert.equal(client.isHealthy(), false);
  assert.equal(socket.destroyed, true);

  socket.emit("data", rtspResponse(1));
  await assert.rejects(
    client.request("DESCRIBE", "rtsps://host.example:322"),
    /WebSocket is not open/,
  );
});

test("RTSP client becomes unhealthy when its WebSocket closes", async () => {
  const { client, socket } = createClient();
  await client.connect("session");
  assert.equal(client.isHealthy(), true);

  socket.emit("close");

  assert.equal(client.isHealthy(), false);
  await assert.rejects(
    client.request("OPTIONS", "rtsps://host.example:322"),
    /WebSocket is not open/,
  );
});

test("RTSP WebSocket replies to ping with a masked pong carrying the same payload", async () => {
  const { client, socket } = createClient();
  await client.connect("session");
  const pingPayload = Buffer.from("keepalive");

  socket.emit("data", serverFrame(0x9, pingPayload));

  assert.equal(socket.writes.length, 1);
  const pong = socket.writes[0]!;
  assert.equal(pong[0], 0x8a);
  assert.equal(pong[1]! & 0x80, 0x80);
  const length = pong[1]! & 0x7f;
  const mask = pong.subarray(2, 6);
  const decoded = Buffer.alloc(length);
  for (let index = 0; index < length; index += 1) {
    decoded[index] = pong[6 + index]! ^ mask[index % 4]!;
  }
  assert.deepEqual(decoded, pingPayload);
  assert.equal(client.isHealthy(), true);
});
