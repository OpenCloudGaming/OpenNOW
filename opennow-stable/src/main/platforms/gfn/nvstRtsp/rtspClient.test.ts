/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import { buildRtspRequest, extractVideoPeer, parseRtspResponse } from "./rtspClient";

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
