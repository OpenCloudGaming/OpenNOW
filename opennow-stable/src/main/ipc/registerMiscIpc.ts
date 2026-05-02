import { IPC_CHANNELS } from "@shared/ipc";
import type { PingResult, StreamRegion } from "@shared/gfn";
import type { MainIpcDeps } from "./types";
import { fetchWithTimeout, withTimeout } from "../lib/httpFetch";

export function registerMiscIpc(deps: MainIpcDeps): void {
  const { ipcMain, app, refreshScheduler, cacheManager, net } = deps;

  ipcMain.handle(IPC_CHANNELS.CACHE_REFRESH_MANUAL, async (): Promise<void> => {
    await refreshScheduler.manualRefresh();
  });

  ipcMain.handle(IPC_CHANNELS.CACHE_DELETE_ALL, async (): Promise<void> => {
    await cacheManager.deleteAll();
    console.log("[IPC] Cache deletion completed successfully");
  });

  ipcMain.handle(IPC_CHANNELS.COMMUNITY_GET_THANKS, async () => {
    return deps.fetchThanksData();
  });

  async function tcpPing(hostname: string, port: number, timeoutMs: number = 3000): Promise<number | null> {
    return new Promise((resolve) => {
      const startTime = Date.now();
      const socket = new net.Socket();

      socket.setTimeout(timeoutMs);

      socket.once("connect", () => {
        const pingMs = Date.now() - startTime;
        socket.destroy();
        resolve(pingMs);
      });

      socket.once("timeout", () => {
        socket.destroy();
        resolve(null);
      });

      socket.once("error", () => {
        socket.destroy();
        resolve(null);
      });

      socket.connect(port, hostname);
    });
  }

  ipcMain.handle(IPC_CHANNELS.PING_REGIONS, async (_event, regions: StreamRegion[]): Promise<PingResult[]> => {
    const pingPromises = regions.map(async (region) => {
      try {
        const url = new URL(region.url);
        const hostname = url.hostname;
        const port = url.protocol === "https:" ? 443 : 80;

        const validPings: number[] = [];

        await tcpPing(hostname, port, 3000);

        for (let i = 0; i < 3; i++) {
          if (i > 0) {
            await new Promise<void>((resolveDelay) => setTimeout(resolveDelay, 100));
          }
          const pingMs = await tcpPing(hostname, port, 3000);
          if (pingMs !== null) {
            validPings.push(pingMs);
          }
        }

        if (validPings.length > 0) {
          const avgPing = Math.round(validPings.reduce((a, b) => a + b, 0) / validPings.length);
          return { url: region.url, pingMs: avgPing };
        }
        return {
          url: region.url,
          pingMs: null,
          error: "All ping tests failed",
        };
      } catch {
        return {
          url: region.url,
          pingMs: null,
          error: "Invalid URL",
        };
      }
    });

    return Promise.all(pingPromises);
  });

  ipcMain.handle(IPC_CHANNELS.PRINTEDWASTE_QUEUE_FETCH, async () => {
    const PRINTEDWASTE_QUEUE_TIMEOUT_MS = 7000;
    const version = app.getVersion();
    const response = await fetchWithTimeout(
      "https://api.printedwaste.com/gfn/queue/",
      {
        headers: {
          "User-Agent": `opennow/${version}`,
          Accept: "application/json",
        },
      },
      PRINTEDWASTE_QUEUE_TIMEOUT_MS,
      "PrintedWaste queue request",
    );
    if (!response.ok) {
      throw new Error(`PrintedWaste API returned HTTP ${response.status}`);
    }

    const body = await withTimeout(
      response.json() as Promise<unknown>,
      PRINTEDWASTE_QUEUE_TIMEOUT_MS,
      "PrintedWaste queue response parse",
    );
    if (!body || typeof body !== "object" || Array.isArray(body)) {
      throw new Error("PrintedWaste API response was not an object");
    }

    const apiBody = body as { status?: unknown; data?: unknown };
    if (typeof apiBody.status !== "boolean") {
      throw new Error("PrintedWaste API response missing boolean status");
    }
    if (!apiBody.status) {
      throw new Error("PrintedWaste API returned status:false");
    }
    if (!apiBody.data || typeof apiBody.data !== "object" || Array.isArray(apiBody.data)) {
      throw new Error("PrintedWaste API response missing data object");
    }

    const normalizedData: Record<string, { QueuePosition: number; "Last Updated": number; Region: string; eta?: number }> =
      {};
    for (const [zoneId, rawZone] of Object.entries(apiBody.data as Record<string, unknown>)) {
      if (!rawZone || typeof rawZone !== "object" || Array.isArray(rawZone)) {
        continue;
      }
      const zone = rawZone as Record<string, unknown>;
      const queuePosition = zone.QueuePosition;
      const lastUpdated = zone["Last Updated"];
      const region = zone.Region;
      const eta = zone.eta;

      if (typeof queuePosition !== "number" || !Number.isFinite(queuePosition)) {
        continue;
      }
      if (typeof lastUpdated !== "number" || !Number.isFinite(lastUpdated)) {
        continue;
      }
      if (typeof region !== "string" || region.length === 0) {
        continue;
      }
      if (eta !== undefined && (typeof eta !== "number" || !Number.isFinite(eta))) {
        continue;
      }

      normalizedData[zoneId] = {
        QueuePosition: queuePosition,
        "Last Updated": lastUpdated,
        Region: region,
        ...(typeof eta === "number" ? { eta } : {}),
      };
    }

    if (Object.keys(normalizedData).length === 0) {
      throw new Error("PrintedWaste API returned no valid zones");
    }
    return normalizedData;
  });

  ipcMain.handle(IPC_CHANNELS.PRINTEDWASTE_SERVER_MAPPING_FETCH, async () => {
    const PRINTEDWASTE_MAPPING_TIMEOUT_MS = 7000;
    const version = app.getVersion();
    const response = await fetchWithTimeout(
      "https://remote.printedwaste.com/config/GFN_SERVERID_TO_REGION_MAPPING",
      {
        headers: {
          "User-Agent": `opennow/${version}`,
          Accept: "application/json",
        },
      },
      PRINTEDWASTE_MAPPING_TIMEOUT_MS,
      "PrintedWaste server mapping request",
    );
    if (!response.ok) {
      throw new Error(`PrintedWaste server mapping returned HTTP ${response.status}`);
    }

    const body = await withTimeout(
      response.json() as Promise<unknown>,
      PRINTEDWASTE_MAPPING_TIMEOUT_MS,
      "PrintedWaste server mapping response parse",
    );
    if (!body || typeof body !== "object" || Array.isArray(body)) {
      throw new Error("PrintedWaste server mapping response was not an object");
    }

    const apiBody = body as { status?: unknown; data?: unknown };
    if (typeof apiBody.status !== "boolean") {
      throw new Error("PrintedWaste server mapping response missing boolean status");
    }
    if (!apiBody.status) {
      throw new Error("PrintedWaste server mapping returned status:false");
    }
    if (!apiBody.data || typeof apiBody.data !== "object" || Array.isArray(apiBody.data)) {
      throw new Error("PrintedWaste server mapping response missing data object");
    }

    const normalizedData: Record<
      string,
      { title?: string; region?: string; is4080Server?: boolean; is5080Server?: boolean; nuked?: boolean }
    > = {};

    for (const [zoneId, rawZone] of Object.entries(apiBody.data as Record<string, unknown>)) {
      if (!rawZone || typeof rawZone !== "object" || Array.isArray(rawZone)) {
        continue;
      }
      const zone = rawZone as Record<string, unknown>;
      const title = zone.title;
      const region = zone.region;
      const is4080Server = zone.is4080Server;
      const is5080Server = zone.is5080Server;
      const nuked = zone.nuked;

      normalizedData[zoneId] = {
        ...(typeof title === "string" ? { title } : {}),
        ...(typeof region === "string" ? { region } : {}),
        ...(typeof is4080Server === "boolean" ? { is4080Server } : {}),
        ...(typeof is5080Server === "boolean" ? { is5080Server } : {}),
        ...(typeof nuked === "boolean" ? { nuked } : {}),
      };
    }

    return normalizedData;
  });
}
