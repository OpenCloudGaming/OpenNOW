import { app, protocol } from "electron";
import { createReadStream } from "node:fs";
import { join, resolve, relative } from "node:path";
import { Readable } from "node:stream";
import { realpath, stat } from "node:fs/promises";
import { isPlayableVideoFilePath } from "@shared/mediaPlayback";

const MAX_MEDIA_PATH_LENGTH = 4096;

const OPENNOW_MEDIA_HOST = "opennow";

let openNowMediaProtocolHandleInstalled = false;

function videoMimeTypeForPath(filePath: string): string {
  const lower = filePath.toLowerCase();
  if (lower.endsWith(".webm")) return "video/webm";
  if (lower.endsWith(".mp4")) return "video/mp4";
  if (lower.endsWith(".mov")) return "video/quicktime";
  if (lower.endsWith(".mkv")) return "video/x-matroska";
  return "application/octet-stream";
}

/**
 * Parse a single Range: bytes=… header. Returns inclusive start/end, or null if unsatisfiable.
 */
function parseByteRangeHeader(rangeHeader: string, fileSize: number): { start: number; end: number } | null {
  const m = /^bytes=(\d*)-(\d*)$/i.exec(rangeHeader.trim());
  if (!m) return null;
  const g1 = m[1];
  const g2 = m[2];
  if (g1 !== "" && g2 !== "") {
    const start = Number(g1);
    const end = Number(g2);
    if (!Number.isFinite(start) || !Number.isFinite(end) || start > end || start >= fileSize) return null;
    return { start, end: Math.min(end, fileSize - 1) };
  }
  if (g1 !== "" && g2 === "") {
    const start = Number(g1);
    if (!Number.isFinite(start) || start >= fileSize) return null;
    return { start, end: fileSize - 1 };
  }
  if (g1 === "" && g2 !== "") {
    const len = Number(g2);
    if (!Number.isFinite(len) || len <= 0) return null;
    if (len >= fileSize) return { start: 0, end: fileSize - 1 };
    return { start: fileSize - len, end: fileSize - 1 };
  }
  return { start: 0, end: fileSize - 1 };
}

/**
 * Resolve a user-supplied path to a real path under Pictures/OpenNOW, or null if unsafe / missing.
 */
export async function resolveTrustedOpenNowMediaPath(rawFp: string): Promise<string | null> {
  if (typeof rawFp !== "string" || rawFp.length > MAX_MEDIA_PATH_LENGTH) return null;
  try {
    const allowedRoot = resolve(join(app.getPath("pictures"), "OpenNOW"));
    const fpResolved = resolve(rawFp);
    const allowedRootReal = await realpath(allowedRoot).catch(() => allowedRoot);
    const fpReal = await realpath(fpResolved).catch(() => fpResolved);
    const rel = relative(allowedRootReal, fpReal);
    if (rel.startsWith("..")) return null;
    return fpReal;
  } catch {
    return null;
  }
}

/**
 * URL for in-renderer &lt;video src&gt;. Uses custom scheme (registered in main) because file:// is often blocked.
 */
export async function getTrustedVideoPlaybackFileUrl(rawFp: string): Promise<string | null> {
  const fpReal = await resolveTrustedOpenNowMediaPath(rawFp);
  if (!fpReal || !isPlayableVideoFilePath(fpReal)) return null;
  return `opennow-media://${OPENNOW_MEDIA_HOST}/playback?p=${encodeURIComponent(fpReal)}`;
}

/**
 * Must run during app startup (after ready), before windows load media URLs.
 */
export function registerOpenNowMediaProtocol(): void {
  if (openNowMediaProtocolHandleInstalled) return;
  openNowMediaProtocolHandleInstalled = true;

  protocol.handle("opennow-media", async (request) => {
    try {
      const url = new URL(request.url);
      if (url.hostname.toLowerCase() !== OPENNOW_MEDIA_HOST) {
        return new Response(null, { status: 404 });
      }
      const pathNorm = url.pathname.replace(/\/$/, "") || "/";
      if (!pathNorm.endsWith("/playback")) {
        return new Response(null, { status: 404 });
      }
      const p = url.searchParams.get("p");
      if (!p) return new Response(null, { status: 400 });
      const fpReal = await resolveTrustedOpenNowMediaPath(p);
      if (!fpReal || !isPlayableVideoFilePath(fpReal)) return new Response(null, { status: 404 });

      const mime = videoMimeTypeForPath(fpReal);
      const { size } = await stat(fpReal);
      const baseHeaders: Record<string, string> = {
        "Content-Type": mime,
        "Accept-Ranges": "bytes",
      };

      if (request.method === "HEAD") {
        return new Response(null, {
          status: 200,
          headers: {
            ...baseHeaders,
            "Content-Length": String(size),
          },
        });
      }

      const rangeRaw = request.headers.get("range");
      if (rangeRaw) {
        const firstRange = rangeRaw.split(",")[0]?.trim() ?? "";
        const parsed = firstRange ? parseByteRangeHeader(firstRange, size) : null;
        if (parsed) {
          const { start, end } = parsed;
          const chunkLength = end - start + 1;
          const nodeStream = createReadStream(fpReal, { start, end });
          const body = Readable.toWeb(nodeStream) as ReadableStream<Uint8Array>;
          return new Response(body, {
            status: 206,
            headers: {
              ...baseHeaders,
              "Content-Length": String(chunkLength),
              "Content-Range": `bytes ${start}-${end}/${size}`,
            },
          });
        }
      }

      const nodeStream = createReadStream(fpReal);
      const body = Readable.toWeb(nodeStream) as ReadableStream<Uint8Array>;
      return new Response(body, {
        status: 200,
        headers: {
          ...baseHeaders,
          "Content-Length": String(size),
        },
      });
    } catch (err) {
      console.warn("[opennow-media] protocol handler:", err);
      return new Response(null, { status: 500 });
    }
  });
}
