import type { SessionCreateRequest } from "@shared/gfn";

import type { CloudMatchResponse } from "../../src/main/gfn/types";

export function buildCloudMatchResponse(
  overrides: Partial<CloudMatchResponse> = {},
  sessionOverrides: Partial<CloudMatchResponse["session"]> = {},
): CloudMatchResponse {
  return {
    requestStatus: {
      statusCode: 1,
      ...overrides.requestStatus,
    },
    session: {
      sessionId: "session-1",
      status: 2,
      connectionInfo: [],
      sessionControlInfo: {},
      iceServerConfiguration: {
        iceServers: [],
      },
      ...overrides.session,
      ...sessionOverrides,
    },
  };
}

export function buildSessionCreateRequest(
  overrides: Partial<SessionCreateRequest> = {},
  settingsOverrides: Partial<SessionCreateRequest["settings"]> = {},
): SessionCreateRequest {
  return {
    token: "token",
    zone: "np-ams-06",
    appId: "12345",
    internalTitle: "OpenNOW Test",
    streamingBaseUrl: "https://np-ams-06.cloudmatchbeta.nvidiagrid.net/",
    settings: {
      resolution: "2560x1440",
      fps: 120,
      maxBitrateMbps: 75,
      codec: "H264",
      colorQuality: "10bit_444",
      gameLanguage: "en_US",
      enableL4S: true,
      ...settingsOverrides,
    },
    ...overrides,
  };
}
