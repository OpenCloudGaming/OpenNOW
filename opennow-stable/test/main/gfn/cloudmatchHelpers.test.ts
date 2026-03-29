import { describe, expect, it } from "vitest";

import {
  buildSessionRequestBody,
  buildSignalingUrl,
  extractHostFromUrl,
  requestHeaders,
  resolvePollStopBase,
  resolveStreamingBaseUrl,
  streamingServerIp,
} from "../../../src/main/gfn/cloudmatchHelpers";
import { buildCloudMatchResponse, buildSessionCreateRequest } from "../../fixtures/cloudmatch";

describe("cloudmatchHelpers", () => {
  it("selects signaling host with the expected priority", () => {
    const directIpResponse = buildCloudMatchResponse({}, {
      connectionInfo: [
        { usage: 14, port: 443, ip: "203.0.113.10", resourcePath: "rtsps://ignored.example:48010/stream" },
      ],
      sessionControlInfo: { ip: "198.51.100.5" as never },
    });
    const resourcePathResponse = buildCloudMatchResponse({}, {
      connectionInfo: [{ usage: 14, port: 48010, resourcePath: "rtsps://signal.example:48010/stream" }],
    });
    const fallbackResponse = buildCloudMatchResponse({}, {
      connectionInfo: [{ usage: 14, port: 48010, resourcePath: "invalid" }],
      sessionControlInfo: { ip: "198.51.100.25" as never },
    });

    expect(streamingServerIp(directIpResponse)).toBe("203.0.113.10");
    expect(streamingServerIp(resourcePathResponse)).toBe("signal.example");
    expect(streamingServerIp(fallbackResponse)).toBe("198.51.100.25");
  });

  it("extracts hosts and safely handles malformed urls", () => {
    expect(extractHostFromUrl("rtsps://stream.example:48010/foo")).toBe("stream.example");
    expect(extractHostFromUrl("wss://signal.example/nvst/")).toBe("signal.example");
    expect(extractHostFromUrl("https://api.example/v2/session")).toBe("api.example");
    expect(extractHostFromUrl("relative/path")).toBeNull();
    expect(extractHostFromUrl("rtsps://.broken:48010/foo")).toBeNull();
  });

  it("derives signaling urls from rtsp, wss, relative, and malformed paths", () => {
    expect(buildSignalingUrl("rtsps://stream.example:47998/some/path", "198.51.100.3")).toEqual({
      signalingUrl: "wss://stream.example/nvst/",
      signalingHost: "stream.example",
    });
    expect(buildSignalingUrl("wss://signal.example/custom", "198.51.100.3")).toEqual({
      signalingUrl: "wss://signal.example/custom",
      signalingHost: "signal.example",
    });
    expect(buildSignalingUrl("/nvst/", "198.51.100.3")).toEqual({
      signalingUrl: "wss://198.51.100.3:443/nvst/",
      signalingHost: null,
    });
    expect(buildSignalingUrl("rtsps://.broken:47998/path", "198.51.100.3")).toEqual({
      signalingUrl: "wss://198.51.100.3:443/nvst/",
      signalingHost: null,
    });
  });

  it("builds request headers with expected auth, platform, and origin behavior", () => {
    const defaultHeaders = requestHeaders({
      token: "jwt-token",
      clientId: "client-1",
      deviceId: "device-1",
      platform: "win32",
    });
    const noOriginHeaders = requestHeaders({
      token: "jwt-token",
      includeOrigin: false,
      clientId: "client-1",
      deviceId: "device-1",
      platform: "linux",
    });

    expect(defaultHeaders).toMatchObject({
      Authorization: "GFNJWT jwt-token",
      "nv-client-id": "client-1",
      "x-device-id": "device-1",
      "nv-client-type": "NATIVE",
      "nv-device-type": "DESKTOP",
      "nv-device-os": "WINDOWS",
      Origin: "https://play.geforcenow.com",
      Referer: "https://play.geforcenow.com/",
    });
    expect(noOriginHeaders.Origin).toBeUndefined();
    expect(noOriginHeaders.Referer).toBeUndefined();
    expect(noOriginHeaders["nv-device-os"]).toBe("LINUX");
  });

  it("builds session payloads with resolution fallback and feature derivation", () => {
    const highFps = buildSessionRequestBody(buildSessionCreateRequest(), {
      deviceHashId: "device-hash",
      subSessionId: "sub-session",
      now: new Date("2026-03-29T12:00:00.000Z"),
    });
    const fallback = buildSessionRequestBody(
      buildSessionCreateRequest({}, {
        resolution: "broken",
        fps: 60,
        colorQuality: "8bit_420",
        enableL4S: false,
      }),
      {
        deviceHashId: "device-hash",
        subSessionId: "sub-session",
      },
    );

    expect(highFps.sessionRequestData.clientRequestMonitorSettings[0]).toMatchObject({
      widthInPixels: 2560,
      heightInPixels: 1440,
      framesPerSecond: 120,
    });
    expect(highFps.sessionRequestData.requestedStreamingFeatures).toMatchObject({
      reflex: true,
      bitDepth: 10,
      chromaFormat: 2,
      enabledL4S: true,
    });
    expect(highFps.sessionRequestData.metaData).toContainEqual({ key: "SubSessionId", value: "sub-session" });
    expect(fallback.sessionRequestData.clientRequestMonitorSettings[0]).toMatchObject({
      widthInPixels: 1920,
      heightInPixels: 1080,
      framesPerSecond: 60,
    });
    expect(fallback.sessionRequestData.requestedStreamingFeatures).toMatchObject({
      reflex: false,
      bitDepth: 0,
      chromaFormat: 0,
      enabledL4S: false,
    });
  });

  it("trims provided streaming base urls and overrides only zone hosts with real server ips", () => {
    expect(resolveStreamingBaseUrl("np-ams-06", " https://partner.example/base/ ")).toBe("https://partner.example/base");
    expect(resolvePollStopBase("np-ams-06", undefined, "203.0.113.5")).toBe("https://203.0.113.5");
    expect(resolvePollStopBase("np-ams-06", "https://partner.example/base/", "203.0.113.5")).toBe("https://partner.example/base");
    expect(resolvePollStopBase("np-ams-06", undefined, "np-ams-07.cloudmatchbeta.nvidiagrid.net")).toBe(
      "https://np-ams-06.cloudmatchbeta.nvidiagrid.net",
    );
  });
});
