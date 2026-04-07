import test from "node:test";
import assert from "node:assert/strict";

import { buildPeerInfoPayload, buildSignInUrl } from "./signaling.ts";

test("buildSignInUrl matches official sign_in query shape", () => {
  const url = new URL(buildSignInUrl({
    signalingServer: "example.test",
    sessionId: "session-123",
    peerName: "peer-abc",
    signalingUrl: "wss://sig.example.test/nvst/",
  }));

  assert.equal(url.pathname, "/nvst/sign_in");
  assert.equal(url.searchParams.get("peer_id"), "peer-abc");
  assert.equal(url.searchParams.get("version"), "2");
  assert.equal(url.searchParams.get("peer_role"), "1");
  assert.equal(url.searchParams.get("pairing_id"), "session-123");
});

test("buildPeerInfoPayload uses browser streaming peer role", () => {
  assert.deepEqual(buildPeerInfoPayload({
    peerId: 2,
    peerName: "peer-abc",
  }), {
    browser: "Chrome",
    browserVersion: "131",
    connected: true,
    id: 2,
    name: "peer-abc",
    peerRole: 1,
    resolution: "1920x1080",
    version: 2,
  });
});
