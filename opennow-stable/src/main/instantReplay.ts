import { app } from "electron";
import { mkdir, readdir, stat, unlink, writeFile } from "node:fs/promises";
import { join } from "node:path";

import type { RecordingEntry } from "@shared/gfn";

const RECORDING_LIMIT = 20;

export interface InstantReplayBeginInput {
  sessionId: string;
  bufferWindowMs: number;
  segmentDurationMs: number;
}

export async function instantReplayBeginSession(input: InstantReplayBeginInput): Promise<void> {
  // No-op in the no-external implementation (ring buffer is renderer-only).
  void input;
}

export async function instantReplayAddSegment(input: { sessionId: string; data: Buffer }): Promise<void> {
  // No-op in the no-external implementation (ring buffer is renderer-only).
  void input;
}

export async function instantReplayEndSession(sessionId: string): Promise<void> {
  // No-op in the no-external implementation (ring buffer is renderer-only).
  void sessionId;
}

function dataUrlToBuffer(dataUrl: string): { buffer: Buffer } {
  const m = /^data:([^;]+);base64,(.*)$/s.exec(dataUrl);
  if (!m) {
    throw new Error("Invalid data URL");
  }
  return { buffer: Buffer.from(m[2]!, "base64") };
}

function sanitizeTitleForFileName(value: string | undefined): string {
  const raw = (value || "Game").trim() || "Game";
  return raw.replace(/[/\\?%*:|"<>]/g, "-").slice(0, 80);
}

async function ensureRecordingsDirectory(): Promise<string> {
  const dir = join(app.getPath("pictures"), "OpenNOW", "Recordings");
  await mkdir(dir, { recursive: true });
  return dir;
}

async function listRecordingFileNames(dir: string): Promise<string[]> {
  const entries = await readdir(dir, { withFileTypes: true });
  return entries
    .filter((e) => e.isFile())
    .map((e) => e.name)
    .filter((name) => /\.(mp4|webm)$/i.test(name));
}

async function trimRecordingsToLimit(): Promise<void> {
  const dir = await ensureRecordingsDirectory();
  const webmFiles = await listRecordingFileNames(dir);
  const loaded = await Promise.all(
    webmFiles.map(async (fileName) => {
      const filePath = join(dir, fileName);
      try {
        const fileStats = await stat(filePath);
        return { fileName, filePath, createdAtMs: fileStats.birthtimeMs || fileStats.mtimeMs };
      } catch {
        return null;
      }
    }),
  );
  const sorted = loaded
    .filter((item): item is NonNullable<typeof item> => item !== null)
    .sort((a, b) => b.createdAtMs - a.createdAtMs);
  if (sorted.length <= RECORDING_LIMIT) return;
  const toDelete = sorted.slice(RECORDING_LIMIT);
  await Promise.all(
    toDelete.map(async (entry) => {
      await unlink(entry.filePath).catch(() => undefined);
      const stem = entry.fileName.replace(/\.(mp4|webm)$/i, "");
      await unlink(join(dir, `${stem}-thumb.jpg`)).catch(() => undefined);
    }),
  );
}

export interface InstantReplaySaveInput {
  clipData: Buffer;
  mimeType: string;
  clipDurationMs: number;
  gameTitle?: string;
  thumbnailDataUrl?: string;
}

export async function instantReplaySave(input: InstantReplaySaveInput): Promise<RecordingEntry> {
  const clipMs = Math.max(1000, Math.round(input.clipDurationMs));
  const payload = input.clipData;
  if (!payload || payload.byteLength === 0) {
    throw new Error("No instant replay data to save");
  }

  const dir = await ensureRecordingsDirectory();
  const stamp = new Date().toISOString().replace(/[:.]/g, "-");
  const title = sanitizeTitleForFileName(input.gameTitle);
  const rand = Math.random().toString(16).slice(2, 8);
  const durSuffix = `-dur${Math.round(clipMs)}`;
  const ext = input.mimeType.startsWith("video/mp4") ? ".mp4" : ".webm";
  const fileName = `${stamp}-${title}-${rand}-replay${durSuffix}${ext}`;
  const finalPath = join(dir, fileName);
  await writeFile(finalPath, payload);

  let thumbnailDataUrl: string | undefined;
  if (input.thumbnailDataUrl) {
    try {
      const { buffer } = dataUrlToBuffer(input.thumbnailDataUrl);
      const stem = fileName.replace(/\.webm$/i, "");
      const thumbPath = join(dir, `${stem}-thumb.jpg`);
      await writeFile(thumbPath, buffer);
      thumbnailDataUrl = input.thumbnailDataUrl;
    } catch {
      // best-effort
    }
  }

  await trimRecordingsToLimit();

  const fileStats = await stat(finalPath);

  return {
    id: fileName,
    fileName,
    filePath: finalPath,
    createdAtMs: Date.now(),
    sizeBytes: fileStats.size,
    durationMs: clipMs,
    gameTitle: input.gameTitle,
    thumbnailDataUrl,
  };
}
