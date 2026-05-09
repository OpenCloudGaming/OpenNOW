import * as net from "node:net";
import type { PingResult, StreamRegion } from "@shared/gfn";

export async function tcpPing(
  hostname: string,
  port: number,
  timeoutMs: number = 3000,
): Promise<number | null> {
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

export async function pingRegions(regions: StreamRegion[]): Promise<PingResult[]> {
  const pingPromises = regions.map(async (region) => {
    try {
      const url = new URL(region.url);
      const hostname = url.hostname;
      const port = url.protocol === "https:" ? 443 : 80;

      const validPings: number[] = [];

      // Warm-up ping (result discarded) to prime the TCP path before measuring.
      // The first cold-start connect includes DNS resolution and TCP SYN overhead
      // which inflates subsequent measurements if not accounted for.
      await tcpPing(hostname, port, 3000);

      // Run 3 measured ping tests with a brief delay between each to allow
      // the previous socket to fully close before opening the next connection.
      for (let i = 0; i < 3; i++) {
        if (i > 0) {
          await new Promise<void>((resolve) => setTimeout(resolve, 100));
        }
        const pingMs = await tcpPing(hostname, port, 3000);
        if (pingMs !== null) {
          validPings.push(pingMs);
        }
      }

      // Calculate average of successful pings
      if (validPings.length > 0) {
        const avgPing = Math.round(
          validPings.reduce((a, b) => a + b, 0) / validPings.length,
        );
        return { url: region.url, pingMs: avgPing };
      } else {
        return {
          url: region.url,
          pingMs: null,
          error: "All ping tests failed",
        };
      }
    } catch {
      return {
        url: region.url,
        pingMs: null,
        error: "Invalid URL",
      };
    }
  });

  return Promise.all(pingPromises);
}
