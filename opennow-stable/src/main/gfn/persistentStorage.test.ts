/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import {
  fetchPersistentStorageLocations,
  resetPersistentStorage,
} from "./persistentStorage";

function jsonResponse(payload: unknown, init: ResponseInit = {}): Response {
  return new Response(JSON.stringify(payload), {
    ...init,
    headers: {
      "Content-Type": "application/json",
      ...init.headers,
    },
  });
}

function headerValue(headers: HeadersInit | undefined, name: string): string | undefined {
  if (!headers) {
    return undefined;
  }
  if (headers instanceof Headers) {
    return headers.get(name) ?? undefined;
  }
  if (Array.isArray(headers)) {
    const entry = headers.find(([key]) => key.toLowerCase() === name.toLowerCase());
    return entry?.[1];
  }
  return headers[name] ?? headers[name.toLowerCase()];
}

test("resetPersistentStorage exchanges the saved session token for an ETS Starfleet paywall token", async (t) => {
  const originalFetch = globalThis.fetch;
  const calls: Array<{ url: string; init?: RequestInit }> = [];
  t.after(() => {
    globalThis.fetch = originalFetch;
  });

  globalThis.fetch = (async (input, init) => {
    const url = String(input);
    calls.push({ url, init });

    if (url === "https://api-prod.nvidia.com/services/ets/v1/generate/starfleet/token") {
      assert.equal(headerValue(init?.headers, "idToken"), "reset-session-token");
      return jsonResponse({ token: "reset-starfleet-token" });
    }

    if (url === "https://api-prod.nvidia.com/gfn-paywall-api/api/v2/reset/storage?storageRegion=null") {
      assert.equal(init?.method, "POST");
      assert.equal(headerValue(init?.headers, "idToken"), "reset-starfleet-token");
      return jsonResponse({ message: "Reset complete." });
    }

    throw new Error(`Unexpected fetch: ${url}`);
  }) as typeof fetch;

  const result = await resetPersistentStorage({ idToken: "reset-session-token" });

  assert.deepEqual(result, {
    ok: true,
    storageRegion: null,
    message: "Reset complete.",
  });
  assert.deepEqual(calls.map((call) => call.url), [
    "https://api-prod.nvidia.com/services/ets/v1/generate/starfleet/token",
    "https://api-prod.nvidia.com/gfn-paywall-api/api/v2/reset/storage?storageRegion=null",
  ]);
});

test("fetchPersistentStorageLocations falls back to the live Netherlands North server id", async (t) => {
  const originalFetch = globalThis.fetch;
  t.after(() => {
    globalThis.fetch = originalFetch;
  });

  globalThis.fetch = (async (input, init) => {
    const url = String(input);

    if (url === "https://api-prod.nvidia.com/services/ets/v1/generate/starfleet/token") {
      assert.equal(headerValue(init?.headers, "idToken"), "locations-session-token");
      return jsonResponse({ token: "locations-starfleet-token" });
    }

    if (url.startsWith("https://api-prod.nvidia.com/gfn-paywall-api/api/v2/products")) {
      return jsonResponse(
        { errors: { errorMessage: "Starfleet idtoken was invalid" } },
        { status: 403 },
      );
    }

    if (url === "https://status.geforcenow.com/api/v2/components.json") {
      return new Response("", { status: 503 });
    }

    throw new Error(`Unexpected fetch: ${url}`);
  }) as typeof fetch;

  const result = await fetchPersistentStorageLocations({
    idToken: "locations-session-token",
    locale: "en_US",
  });

  assert.equal(
    result.locations.find((location) => location.name === "Netherlands North")?.code,
    "NP-AMS-07",
  );
});

