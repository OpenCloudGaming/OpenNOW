/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import { buildRuntimeSnapshot } from "./runtimeSnapshot";
import { decideSignalingDisconnect, selectRecoveryCandidate } from "./streamRecoveryDecisions";

test("runtime snapshot prefers the live session and preserves resume identity", () => {
  const snapshot = buildRuntimeSnapshot({
    streamStatus: "streaming",
    session: {
      sessionId: "live-session",
      status: 2,
      zone: "prod",
      serverIp: "10.0.0.2",
      signalingServer: "signal.example",
      signalingUrl: "wss://signal.example",
      iceServers: [],
      clientId: "client-1",
      deviceId: "device-1",
      appLaunchMode: 1,
      enablePersistingInGameSettings: true,
    },
    navbarSession: {
      sessionId: "navbar-session",
      appId: 99,
      status: 2,
      serverIp: "10.0.0.3",
    },
    streamingGameId: "game-1",
    streamingStore: "STEAM",
    recoveryAppId: 42,
    updatedAt: 1234,
  });

  assert.deepEqual(snapshot, {
    version: 1,
    updatedAt: 1234,
    streamStatus: "streaming",
    sessionId: "live-session",
    sessionAppId: 42,
    streamingGameId: "game-1",
    streamingStore: "STEAM",
    recoveryAppId: 42,
    resumeContext: {
      sessionId: "live-session",
      serverIp: "10.0.0.2",
      streamingBaseUrl: undefined,
      signalingServer: "signal.example",
      signalingUrl: "wss://signal.example",
      appId: 42,
      appLaunchMode: 1,
      enablePersistingInGameSettings: true,
      clientId: "client-1",
      deviceId: "device-1",
    },
  });
});

test("runtime snapshot is absent only when every active context is idle", () => {
  assert.equal(buildRuntimeSnapshot({
    streamStatus: "idle",
    session: null,
    navbarSession: null,
    streamingGameId: null,
    streamingStore: null,
    recoveryAppId: null,
  }), null);

  const navbarSnapshot = buildRuntimeSnapshot({
    streamStatus: "idle",
    session: null,
    navbarSession: {
      sessionId: "paused-session",
      appId: 7,
      status: 3,
      serverIp: "10.0.0.7",
    },
    streamingGameId: null,
    streamingStore: null,
    recoveryAppId: null,
    updatedAt: 10,
  });
  assert.equal(navbarSnapshot?.sessionId, "paused-session");
  assert.equal(navbarSnapshot?.sessionAppId, 7);
  assert.equal(navbarSnapshot?.resumeContext?.serverIp, "10.0.0.7");
});

test("disconnect recovery honors remote ICE grace and controlled disconnect ordering", () => {
  assert.equal(decideSignalingDisconnect({
    appUnloading: false,
    streamStatus: "connecting",
    reason: "network lost",
    hasConfirmedRemoteIce: false,
    iceState: "new",
    pendingControlledDisconnects: 0,
  }), "fail-before-remote-ice");
  assert.equal(decideSignalingDisconnect({
    appUnloading: false,
    streamStatus: "streaming",
    reason: "network lost",
    hasConfirmedRemoteIce: true,
    iceState: "new",
    pendingControlledDisconnects: 0,
  }), "ignore-active-ice");
  assert.equal(decideSignalingDisconnect({
    appUnloading: false,
    streamStatus: "streaming",
    reason: "network lost",
    hasConfirmedRemoteIce: true,
    iceState: "failed",
    pendingControlledDisconnects: 1,
  }), "ignore-controlled-disconnect");
  assert.equal(decideSignalingDisconnect({
    appUnloading: false,
    streamStatus: "streaming",
    reason: "network lost",
    hasConfirmedRemoteIce: true,
    iceState: "failed",
    pendingControlledDisconnects: 0,
  }), "recover");
});

test("disconnect recovery only treats explicit remote peer termination as a session end", () => {
  for (const reason of ["BYE", "peerRemoved", "peer removed"]) {
    assert.equal(decideSignalingDisconnect({
      appUnloading: false,
      streamStatus: "streaming",
      reason,
      hasConfirmedRemoteIce: true,
      iceState: "failed",
      pendingControlledDisconnects: 0,
    }), "expected-session-close");
  }

  for (const reason of ["socket closed", "signaling disconnected: socket closed"]) {
    assert.equal(decideSignalingDisconnect({
      appUnloading: false,
      streamStatus: "streaming",
      reason,
      hasConfirmedRemoteIce: true,
      iceState: "failed",
      pendingControlledDisconnects: 0,
    }), "recover");
    assert.equal(decideSignalingDisconnect({
      appUnloading: false,
      streamStatus: "connecting",
      reason,
      hasConfirmedRemoteIce: false,
      iceState: "new",
      pendingControlledDisconnects: 0,
    }), "recover");
  }
});

test("recovery candidate stays on the same session before app or persisted fallbacks", () => {
  const result = selectRecoveryCandidate([
    { sessionId: "other", appId: 42, status: 2, serverIp: "10.0.0.3" },
    { sessionId: "current", appId: 7, status: 3, serverIp: "10.0.0.2" },
  ], "current", 42, null);
  assert.equal(result.candidate?.sessionId, "current");
  assert.equal(result.source, "active-session");

  const persisted = selectRecoveryCandidate(
    [{ sessionId: "current", appId: 42, status: 1 }],
    "current",
    42,
    {
      sessionId: "current",
      serverIp: "10.0.0.9",
      clientId: "persisted-client",
    },
  );
  assert.equal(persisted.candidate?.serverIp, "10.0.0.9");
  assert.equal(persisted.source, "persisted-resume-context");
});
