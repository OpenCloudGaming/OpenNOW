import assert from "node:assert/strict";
import test from "node:test";

import type { NativeStreamerMessage } from "@shared/nativeStreamer";
import {
  isNativeStreamerEvent,
  isNativeStreamerResponse,
} from "./protocol";

test("protocol guards distinguish request responses from events by id", () => {
  const response = { id: "request-1", type: "ok" } as NativeStreamerMessage;
  const event = { type: "status", status: "ready" } as NativeStreamerMessage;

  assert.equal(isNativeStreamerResponse(response), true);
  assert.equal(isNativeStreamerEvent(response), false);
  assert.equal(isNativeStreamerResponse(event), false);
  assert.equal(isNativeStreamerEvent(event), true);
});

test("NVST transport readiness is an id-less native event", () => {
  const event = {
    type: "nvst-transport-ready",
    phase: "sctp",
  } satisfies NativeStreamerMessage;

  assert.equal(isNativeStreamerEvent(event), true);
  assert.equal(isNativeStreamerResponse(event), false);
});
