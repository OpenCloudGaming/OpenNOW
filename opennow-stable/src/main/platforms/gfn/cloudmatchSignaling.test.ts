import assert from "node:assert/strict";
import test from "node:test";

import type { CloudMatchResponse } from "./types";
import {
  buildSignalingUrl,
  normalizeIceServers,
  resolveMediaConnectionInfo,
} from "./cloudmatchSignaling";

test("normalizeIceServers preserves supplied hostnames, schemes, and credentials", async () => {
  const response = {
    session: {
      iceServerConfiguration: {
        iceServers: [{
          urls: ["turns:relay.example.test:443?transport=tcp"],
          username: "synthetic-user",
          credential: "synthetic-credential",
        }],
      },
    },
  } as CloudMatchResponse;

  assert.deepEqual(await normalizeIceServers(response), [{
    urls: ["turns:relay.example.test:443?transport=tcp"],
    username: "synthetic-user",
    credential: "synthetic-credential",
  }]);
});

test("buildSignalingUrl preserves the supplied authority, path, and query", () => {
  assert.deepEqual(
    buildSignalingUrl(
      "rtsps://signal.example.test:48322/custom/path?ticket=synthetic",
      "198.51.100.1",
    ),
    {
      signalingUrl: "wss://signal.example.test:48322/custom/path?ticket=synthetic",
      signalingHost: "signal.example.test:48322",
    },
  );
});

test("WebRTC media projection ignores native MEDIA and prefers legacy VIDEO over BUNDLE", () => {
  assert.deepEqual(
    resolveMediaConnectionInfo([
      { ip: "198.51.100.15", port: 49015, usage: 15 },
      { ip: "198.51.100.17", port: 49017, usage: 17 },
      { ip: "198.51.100.2", port: 49002, usage: 2 },
    ], "198.51.100.1"),
    { ip: "198.51.100.2", port: 49002, usage: 2 },
  );

  assert.equal(
    resolveMediaConnectionInfo([
      { ip: "198.51.100.14", port: 443, usage: 14 },
      { ip: "198.51.100.15", port: 49015, usage: 15 },
      { ip: "198.51.100.16", port: 48322, usage: 16 },
    ], "198.51.100.1", { logMissing: false }),
    undefined,
  );
});
