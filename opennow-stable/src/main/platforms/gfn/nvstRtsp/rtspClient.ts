import type { Duplex } from "node:stream";

import {
  connectNvstWss,
  encodeWsPingFrame,
  encodeWsPongFrame,
  encodeWsTextFrame,
  WsFrameReader,
} from "./websocketTransport";

export const RTSPS_WS_KEEPALIVE_INTERVAL_MS = 2_000;

export interface ParsedRtspResponse {
  statusCode: number;
  statusText: string;
  headers: Record<string, string>;
  body: string;
}

export function parseRtspResponse(raw: string): ParsedRtspResponse {
  const normalized = raw.replace(/\r\n/g, "\n");
  const splitAt = normalized.indexOf("\n\n");
  const headerBlock = splitAt >= 0 ? normalized.slice(0, splitAt) : normalized;
  const body = splitAt >= 0 ? normalized.slice(splitAt + 2) : "";
  const headerLines = headerBlock.split("\n").filter((line) => line.length > 0);
  const statusLine = headerLines[0] ?? "";
  const statusMatch = /^(?:RTSP|HTTP)\/\d(?:\.\d)?\s+(\d{3})\s*(.*)$/i.exec(statusLine);
  if (!statusMatch) {
    throw new Error(`Invalid RTSP status line: ${statusLine.slice(0, 120)}`);
  }
  const headers: Record<string, string> = {};
  for (const line of headerLines.slice(1)) {
    const idx = line.indexOf(":");
    if (idx <= 0) {
      continue;
    }
    headers[line.slice(0, idx).trim().toLowerCase()] = line.slice(idx + 1).trim();
  }
  return {
    statusCode: Number(statusMatch[1]),
    statusText: statusMatch[2]?.trim() ?? "",
    headers,
    body,
  };
}

export function header(headers: Record<string, string>, name: string): string | undefined {
  return headers[name.toLowerCase()];
}

/** Content-Length only when there is a body. Empty header values are kept (official SETUP sends `Transport: `). */
export function buildRtspRequest(
  method: string,
  uri: string,
  extraHeaders: Record<string, string> = {},
  body = "",
  cseq = 1,
): string {
  const headers: Record<string, string> = {
    CSeq: String(cseq),
    "Request-Id": String(cseq),
    ...extraHeaders,
  };
  if (body.length > 0) {
    headers["Content-Length"] = String(Buffer.byteLength(body, "utf8"));
  }
  let message = `${method} ${uri} RTSP/1.0\r\n`;
  for (const [key, value] of Object.entries(headers)) {
    message += `${key}: ${value}\r\n`;
  }
  message += "\r\n";
  if (body.length > 0) {
    message += body;
  }
  return message;
}

export function extractVideoPeer(
  transport: string | undefined,
): { ip: string; port: number } | undefined {
  if (!transport) {
    return undefined;
  }
  const portMatch = /X-GS-ServerPort=(\d+)/i.exec(transport);
  const sourceMatch = /source=([^;,\s]+)/i.exec(transport);
  if (!portMatch || !sourceMatch) {
    return undefined;
  }
  return { ip: sourceMatch[1]!, port: Number(portMatch[1]) };
}

export class RtspOverWssClient {
  private socket: Duplex | null = null;
  private frameReader = new WsFrameReader();
  private buffer = Buffer.alloc(0);
  private cseq = 0;
  private healthy = false;
  private keepaliveTimer: NodeJS.Timeout | null = null;
  private pending: {
    cseq: number;
    resolve: (response: ParsedRtspResponse) => void;
    reject: (error: Error) => void;
  } | null = null;

  constructor(
    private readonly host: string,
    private readonly port: number,
    private readonly timeoutMs: number,
    private readonly onLog?: (message: string) => void,
    private readonly connectSocket: typeof connectNvstWss = connectNvstWss,
  ) {}

  async connect(sessionId?: string): Promise<void> {
    const socket = await this.connectSocket(
      this.host,
      this.port,
      this.timeoutMs,
      sessionId,
      this.onLog,
    );
    this.socket = socket;
    this.frameReader = new WsFrameReader();
    this.buffer = Buffer.alloc(0);
    this.healthy = true;
    this.keepaliveTimer = setInterval(() => {
      if (this.isHealthy()) {
        this.socket?.write(encodeWsPingFrame());
      }
    }, RTSPS_WS_KEEPALIVE_INTERVAL_MS);
    this.keepaliveTimer.unref();
    socket.on("data", (chunk: Buffer) => this.onSocketData(chunk));
    socket.on("error", (error) => {
      this.failConnection(error instanceof Error ? error : new Error(String(error)), false);
    });
    socket.on("close", () => {
      this.failConnection(new Error("RTSPS WebSocket closed"), false);
    });
  }

  isHealthy(): boolean {
    return this.healthy && this.socket !== null && !this.socket.destroyed;
  }

  close(): void {
    this.failConnection(new Error("RTSPS probe closed"), true);
  }

  async request(
    method: string,
    uri: string,
    extraHeaders: Record<string, string> = {},
    body = "",
  ): Promise<ParsedRtspResponse> {
    if (!this.isHealthy()) {
      throw new Error("RTSPS WebSocket is not open");
    }
    if (this.pending) {
      throw new Error("Overlapping RTSP requests are not supported");
    }

    this.cseq += 1;
    const message = buildRtspRequest(method, uri, extraHeaders, body, this.cseq);

    return await new Promise<ParsedRtspResponse>((resolve, reject) => {
      const pending = {
        cseq: this.cseq,
        resolve: (response: ParsedRtspResponse) => {
          clearTimeout(timer);
          resolve(response);
        },
        reject: (error: Error) => {
          clearTimeout(timer);
          reject(error);
        },
      };
      const timer = setTimeout(() => {
        if (this.pending !== pending) {
          return;
        }
        this.failConnection(
          new Error(`RTSP ${method} timed out after ${this.timeoutMs}ms`),
          true,
        );
      }, this.timeoutMs);
      this.pending = pending;

      try {
        this.socket?.write(encodeWsTextFrame(Buffer.from(message, "utf8")));
      } catch (error) {
        clearTimeout(timer);
        this.pending = null;
        const writeError = error instanceof Error ? error : new Error(String(error));
        this.failConnection(writeError, true);
        reject(writeError);
      }
    });
  }

  private onSocketData(chunk: Buffer): void {
    if (!this.isHealthy()) {
      return;
    }
    try {
      const payloads = this.frameReader.push(chunk);
      for (const pingPayload of this.frameReader.drainPingPayloads()) {
        this.socket?.write(encodeWsPongFrame(pingPayload));
      }
      for (const payload of payloads) {
        this.buffer = Buffer.concat([this.buffer, payload]);
        this.tryCompleteResponse();
      }
    } catch (error) {
      this.failConnection(error instanceof Error ? error : new Error(String(error)), true);
    }
  }

  private tryCompleteResponse(): void {
    if (!this.pending) {
      return;
    }

    const text = this.buffer.toString("utf8");
    const crlfSplit = text.indexOf("\r\n\r\n");
    const lfSplit = text.indexOf("\n\n");
    let headerEnd = -1;
    let sepLen = 0;
    if (crlfSplit >= 0 && (lfSplit < 0 || crlfSplit <= lfSplit)) {
      headerEnd = crlfSplit;
      sepLen = 4;
    } else if (lfSplit >= 0) {
      headerEnd = lfSplit;
      sepLen = 2;
    }
    if (headerEnd < 0) {
      return;
    }

    const headerText = text.slice(0, headerEnd);
    const contentLengthMatch = /^Content-Length:\s*(\d+)\s*$/im.exec(headerText);
    const contentLength = contentLengthMatch ? Number(contentLengthMatch[1]) : 0;
    const bodyStart = headerEnd + sepLen;
    // Use byte length of remaining buffer after header separator.
    const headerByteLength = Buffer.byteLength(text.slice(0, bodyStart), "utf8");
    const availableBodyBytes = this.buffer.length - headerByteLength;
    if (availableBodyBytes < contentLength) {
      return;
    }

    const totalBytes = headerByteLength + contentLength;
    const raw = this.buffer.subarray(0, totalBytes).toString("utf8");
    this.buffer = this.buffer.subarray(totalBytes);

    try {
      const parsed = parseRtspResponse(raw);
      const responseCseq = header(parsed.headers, "cseq");
      if (responseCseq !== String(this.pending.cseq)) {
        throw new Error(
          `RTSP response CSeq mismatch: expected ${this.pending.cseq}, received ${responseCseq ?? "missing"}`,
        );
      }
      const pending = this.pending;
      this.pending = null;
      pending?.resolve(parsed);
    } catch (error) {
      this.failConnection(error instanceof Error ? error : new Error(String(error)), true);
    }
  }

  private failConnection(error: Error, destroySocket: boolean): void {
    if (this.keepaliveTimer) {
      clearInterval(this.keepaliveTimer);
      this.keepaliveTimer = null;
    }
    this.healthy = false;
    this.buffer = Buffer.alloc(0);
    const socket = this.socket;
    this.socket = null;
    const pending = this.pending;
    this.pending = null;
    pending?.reject(error);
    if (destroySocket) {
      try {
        socket?.destroy();
      } catch {
        // ignore
      }
    }
  }
}
