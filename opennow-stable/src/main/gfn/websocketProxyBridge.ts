import net from "node:net";

import WebSocket, { createWebSocketStream } from "ws";

const bridgePromises = new Map<string, Promise<string>>();
const websocketConnectPath = "/connect";

function websocketEndpoint(proxyUrl: string): string {
  const endpoint = new URL(proxyUrl);
  endpoint.pathname = websocketConnectPath;
  return endpoint.toString();
}

function bridgeConnection(socket: net.Socket, proxyUrl: string): void {
  socket.pause();
  const websocket = new WebSocket(websocketEndpoint(proxyUrl), {
    handshakeTimeout: 10_000,
    maxPayload: 1024 * 1024,
    perMessageDeflate: false,
  });

  const fail = (error: Error): void => {
    if (!socket.destroyed) socket.destroy(error);
  };

  websocket.once("open", () => {
    const tunnel = createWebSocketStream(websocket, { allowHalfOpen: false });
    tunnel.once("error", fail);
    socket.once("error", () => tunnel.destroy());
    socket.pipe(tunnel).pipe(socket);
    socket.resume();
  });
  websocket.once("error", fail);
  websocket.once("unexpected-response", (_request, response) => {
    fail(new Error(`Cloudflare WebSocket proxy returned HTTP ${response.statusCode ?? "unknown"}.`));
  });
  socket.once("close", () => {
    if (websocket.readyState === WebSocket.CONNECTING || websocket.readyState === WebSocket.OPEN) {
      websocket.close();
    }
  });
}

async function startBridge(proxyUrl: string): Promise<string> {
  const server = net.createServer({ allowHalfOpen: false }, (socket) => bridgeConnection(socket, proxyUrl));
  server.maxConnections = 128;

  await new Promise<void>((resolve, reject) => {
    const onError = (error: Error): void => reject(error);
    server.once("error", onError);
    server.listen({ host: "127.0.0.1", port: 0, exclusive: true }, () => {
      server.off("error", onError);
      resolve();
    });
  });
  server.on("error", (error) => console.error("[CloudflareProxy] Local bridge error:", error));
  server.unref();

  const address = server.address();
  if (!address || typeof address === "string") {
    server.close();
    throw new Error("Cloudflare proxy bridge did not obtain a local TCP port.");
  }
  return `http://127.0.0.1:${address.port}`;
}

export function localProxyForCloudflareTunnel(proxyUrl: string): Promise<string> {
  const existing = bridgePromises.get(proxyUrl);
  if (existing) return existing;

  const bridge = startBridge(proxyUrl).catch((error) => {
    bridgePromises.delete(proxyUrl);
    throw error;
  });
  bridgePromises.set(proxyUrl, bridge);
  return bridge;
}
