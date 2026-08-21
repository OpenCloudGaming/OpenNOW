/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import {
  buildEmptyPathUpgradeRequest,
  buildNvstWssUpgradeRequest,
  buildNvstWssUpgradeRequestTarget,
  encodeWsTextFrame,
  WsFrameReader,
} from "./websocketTransport";

test("buildNvstWssUpgradeRequest uses Bifrost-shaped GET / by default", () => {
  const request = buildNvstWssUpgradeRequest(
    "80-250-97-40.cloudmatchbeta.nvidiagrid.net",
    322,
    "dGVzdGtleTEyMzQ1Njc4OQ==",
  );
  const requestLine = request.split("\r\n")[0] ?? "";
  assert.equal(requestLine, "GET / HTTP/1.1");
  assert.equal(Buffer.from(requestLine, "utf8").toString("hex"), "474554202f20485454502f312e31");
  assert.equal(buildNvstWssUpgradeRequestTarget("host.example", 322, "slash"), "/");
  assert.equal(buildNvstWssUpgradeRequestTarget("host.example", 322, "rtspPath"), "/rtsp");
  assert.equal(
    buildNvstWssUpgradeRequestTarget("host.example", 322, "sessionPath", "abc-uuid"),
    "/v2/session/abc-uuid",
  );
  assert.match(request, /^GET \/ HTTP\/1\.1\r\nHost: 80-250-97-40\.cloudmatchbeta\.nvidiagrid\.net:322\r\n/);
  assert.match(request, /\r\nConnection: Upgrade\r\n/);
  assert.match(request, /\r\nUpgrade: websocket\r\n/);
  assert.match(request, /\r\nSec-WebSocket-Version: 13\r\n/);
  assert.match(request, /\r\nSec-WebSocket-Key: dGVzdGtleTEyMzQ1Njc4OQ==\r\n/);
  assert.match(request, /\r\nContent-Length: 0\r\n\r\n$/);
  assert.doesNotMatch(request, /Sec-WebSocket-Protocol/i);
  assert.doesNotMatch(request, /User-Agent/i);
  assert.doesNotMatch(request, /x-nv-sessionid/i);
});

test("buildNvstWssUpgradeRequest supports the /rtsp endpoint with session identity", () => {
  const request = buildNvstWssUpgradeRequest("host.example", 322, "key", {
    form: "rtspPath",
    sessionId: "sess-uuid",
  });
  assert.match(request, /^GET \/rtsp HTTP\/1\.1\r\n/);
  assert.match(request, /\r\nx-nv-sessionid: sess-uuid\r\n/);
});

test("buildEmptyPathUpgradeRequest keeps empty URI for research (live → 400)", () => {
  const request = buildEmptyPathUpgradeRequest(
    "80-250-97-40.cloudmatchbeta.nvidiagrid.net",
    322,
    "dGVzdGtleTEyMzQ1Njc4OQ==",
  );
  const requestLine = request.split("\r\n")[0] ?? "";
  assert.equal(requestLine, "GET  HTTP/1.1");
  assert.equal(Buffer.from(requestLine, "utf8").toString("hex"), "4745542020485454502f312e31");
});

test("buildNvstWssUpgradeRequest attaches x-nv-sessionid to upgrade requests", () => {
  const request = buildNvstWssUpgradeRequest("host.example", 322, "abc", {
    form: "slash",
    sessionId: "sess-uuid",
  });
  assert.match(request, /\r\nx-nv-sessionid: sess-uuid\r\n/);
});

test("WebSocket framing preserves masked client payload bytes", () => {
  const payload = Buffer.from("OPTIONS rtsps://host.example:322 RTSP/1.0\r\n\r\n");
  const frame = encodeWsTextFrame(payload);
  assert.equal(frame[0], 0x81);
  assert.equal(frame[1]! & 0x80, 0x80);

  const reader = new WsFrameReader();
  assert.deepEqual(reader.push(frame.subarray(0, 3)), []);
  assert.deepEqual(reader.push(frame.subarray(3)), [payload]);
});
