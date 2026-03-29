import { afterEach, describe, expect, it, vi } from "vitest";

import type { LoginProvider } from "@shared/gfn";

import {
  buildAuthUrl,
  decodeBase64Url,
  findAvailablePort,
  isExpired,
  isNearExpiry,
  normalizeProvider,
  parseJwtPayload,
  toExpiresAt,
} from "../../../src/main/gfn/authHelpers";

const provider: LoginProvider = {
  idpId: "provider-1",
  code: "NVIDIA",
  displayName: "NVIDIA",
  streamingServiceUrl: "https://prod.cloudmatchbeta.nvidiagrid.net",
  priority: 0,
};

function createJwt(payload: object): string {
  const encode = (value: object): string => Buffer.from(JSON.stringify(value)).toString("base64url");
  return `${encode({ alg: "HS256", typ: "JWT" })}.${encode(payload)}.signature`;
}

afterEach(() => {
  vi.useRealTimers();
});

describe("authHelpers", () => {
  it("normalizes provider urls with a trailing slash", () => {
    expect(normalizeProvider(provider).streamingServiceUrl).toBe("https://prod.cloudmatchbeta.nvidiagrid.net/");
    expect(normalizeProvider({ ...provider, streamingServiceUrl: "https://already.example/" }).streamingServiceUrl).toBe(
      "https://already.example/",
    );
  });

  it("parses base64url and jwt payloads", () => {
    expect(decodeBase64Url(Buffer.from('{"sub":"123"}').toString("base64url"))).toBe('{"sub":"123"}');
    expect(parseJwtPayload<{ sub: string; tier: string }>(createJwt({ sub: "123", tier: "ULTIMATE" }))).toEqual({
      sub: "123",
      tier: "ULTIMATE",
    });
  });

  it("returns null for invalid jwt payloads", () => {
    expect(parseJwtPayload("not-a-jwt")).toBeNull();
    expect(parseJwtPayload("a.invalid-json.c")).toBeNull();
  });

  it("applies expiry and near-expiry boundaries", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-03-29T12:00:00.000Z"));
    const now = Date.now();

    expect(toExpiresAt(60)).toBe(now + 60_000);
    expect(isExpired(now)).toBe(true);
    expect(isExpired(now + 1)).toBe(false);
    expect(isNearExpiry(now + 59_999, 60_000)).toBe(true);
    expect(isNearExpiry(now + 60_000, 60_000)).toBe(false);
    expect(isNearExpiry(undefined, 60_000)).toBe(true);
  });

  it("builds auth urls with required oauth query params", () => {
    const url = new URL(
      buildAuthUrl(provider, {
        challenge: "challenge-value",
        port: 2259,
        deviceId: "device-123",
        nonce: "nonce-456",
      }),
    );

    expect(url.searchParams.get("response_type")).toBe("code");
    expect(url.searchParams.get("device_id")).toBe("device-123");
    expect(url.searchParams.get("scope")).toBe("openid consent email tk_client age");
    expect(url.searchParams.get("client_id")).toBe("ZU7sPN-miLujMD95LfOQ453IB0AtjM8sMyvgJ9wCXEQ");
    expect(url.searchParams.get("redirect_uri")).toBe("http://localhost:2259");
    expect(url.searchParams.get("nonce")).toBe("nonce-456");
    expect(url.searchParams.get("code_challenge")).toBe("challenge-value");
    expect(url.searchParams.get("idp_id")).toBe("provider-1");
  });

  it("selects the first available redirect port using an injected availability check", async () => {
    const checked: number[] = [];
    const port = await findAvailablePort([2259, 6460, 7119], async (candidate) => {
      checked.push(candidate);
      return candidate === 6460;
    });

    expect(port).toBe(6460);
    expect(checked).toEqual([2259, 6460]);
  });

  it("throws when no redirect ports are available", async () => {
    await expect(findAvailablePort([2259, 6460], async () => false)).rejects.toThrow(
      "No available OAuth callback ports",
    );
  });
});
