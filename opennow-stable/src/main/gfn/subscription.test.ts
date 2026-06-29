/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import { fetchSubscription } from "./subscription";

function jsonResponse(payload: unknown, init: ResponseInit = {}): Response {
  return new Response(JSON.stringify(payload), {
    ...init,
    headers: {
      "Content-Type": "application/json",
      ...init.headers,
    },
  });
}

test("fetchSubscription exposes only entitled resolution and fps profiles", async (t) => {
  const originalFetch = globalThis.fetch;
  t.after(() => {
    globalThis.fetch = originalFetch;
  });

  globalThis.fetch = (async (input) => {
    const url = new URL(String(input));
    assert.equal(url.origin + url.pathname, "https://mes.geforcenow.com/v4/subscriptions");
    assert.equal(url.searchParams.get("userId"), "user-1");
    assert.equal(url.searchParams.get("vpcId"), "NP-AMS-08");

    return jsonResponse({
      membershipTier: "FREE",
      subType: "LIMITED",
      features: {
        resolutions: [
          {
            widthInPixels: 1920,
            heightInPixels: 1080,
            framesPerSecond: 60,
            isEntitled: true,
          },
          {
            widthInPixels: 1920,
            heightInPixels: 1080,
            framesPerSecond: 240,
            isEntitled: false,
          },
          {
            widthInPixels: 3840,
            heightInPixels: 2160,
            framesPerSecond: 120,
            isEntitled: false,
          },
        ],
      },
    });
  }) as typeof fetch;

  const subscription = await fetchSubscription("token", "user-1", "NP-AMS-08");

  assert.deepEqual(subscription.entitledResolutions, [
    { width: 1920, height: 1080, fps: 60 },
  ]);
});
