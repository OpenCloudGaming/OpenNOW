import { app } from "electron";
import { Buffer } from "node:buffer";
import { randomUUID } from "node:crypto";
import { createWriteStream } from "node:fs";
import { mkdir, readdir, readFile, rename, stat, unlink, writeFile } from "node:fs/promises";
import { join } from "node:path";
import type {
  RecordingAbortRequest,
  RecordingBeginRequest,
  RecordingBeginResult,
  RecordingChunkRequest,
  RecordingDeleteRequest,
  RecordingEntry,
  RecordingFinishRequest,
} from "@shared/gfn";
import {
  assertSafeMediaId,
  dataUrlToBuffer,
  sanitizeTitleForFileName,
} from "./mediaFiles";

const RECORDING_LIMIT = 20;

interface ActiveRecording {
  writeStream: ReturnType<typeof createWriteStream>;
  tempPath: string;
  mimeType: string;
}

const activeRecordings = new Map<string, ActiveRecording>();

export function getRecordingsDirectory(): string {
  return join(app.getPath("pictures"), "OpenNOW", "Recordings");
}

export async function ensureRecordingsDirectory(): Promise<string> {
  const dir = getRecordingsDirectory();
  await mkdir(dir, { recursive: true });
  return dir;
}

export function assertSafeRecordingId(id: string): void {
  assertSafeMediaId(id, "recording");
}

export function extFromMimeType(mimeType: string): ".mp4" | ".webm" {
  return mimeType.startsWith("video/mp4") ? ".mp4" : ".webm";
}

export async function listRecordings(): Promise<RecordingEntry[]> {
  const dir = await ensureRecordingsDirectory();
  const entries = await readdir(dir, { withFileTypes: true });
  const webmFiles = entries
    .filter((e) => e.isFile())
    .map((e) => e.name)
    .filter((name) => /\.(mp4|webm)$/i.test(name));

  const loaded = await Promise.all(
    webmFiles.map(async (fileName): Promise<RecordingEntry | null> => {
      const filePath = join(dir, fileName);
      try {
        const fileStats = await stat(filePath);
        const stem = fileName.replace(/\.(mp4|webm)$/i, "");
        const thumbName = `${stem}-thumb.jpg`;
        const thumbPath = join(dir, thumbName);

        let thumbnailDataUrl: string | undefined;
        try {
          const thumbBuf = await readFile(thumbPath);
          thumbnailDataUrl = `data:image/jpeg;base64,${thumbBuf.toString("base64")}`;
        } catch {
          // No thumbnail for this recording — that's fine
        }

        // Parse durationMs encoded in filename as last numeric segment before extension
        const durMatch = /-dur(\d+)\.(mp4|webm)$/i.exec(fileName);
        const durationMs = durMatch ? Number(durMatch[1]) : 0;

        // Parse game title from filename: {stamp}-{title}-{rand}[-dur{ms}].{ext}
        const titleMatch =
          /^[^-]+-[^-]+-([^-]+(?:-[^-]+)*?)-[a-f0-9]{6}(?:-dur\d+)?\.(mp4|webm)$/i.exec(
            fileName,
          );
        const gameTitle = titleMatch
          ? titleMatch[1].replace(/-/g, " ")
          : undefined;

        return {
          id: fileName,
          fileName,
          filePath,
          createdAtMs: fileStats.birthtimeMs || fileStats.mtimeMs,
          sizeBytes: fileStats.size,
          durationMs,
          gameTitle,
          thumbnailDataUrl,
        };
      } catch {
        return null;
      }
    }),
  );

  return loaded
    .filter((item): item is RecordingEntry => item !== null)
    .sort((a, b) => b.createdAtMs - a.createdAtMs)
    .slice(0, RECORDING_LIMIT);
}

export async function beginRecording(
  input: RecordingBeginRequest,
): Promise<RecordingBeginResult> {
  const dir = await ensureRecordingsDirectory();
  const recordingId = randomUUID();
  const ext = extFromMimeType(input.mimeType);
  const tempPath = join(dir, `${recordingId}${ext}.tmp`);
  const writeStream = createWriteStream(tempPath);
  activeRecordings.set(recordingId, {
    writeStream,
    tempPath,
    mimeType: input.mimeType,
  });
  return { recordingId };
}

export async function appendRecordingChunk(
  input: RecordingChunkRequest,
): Promise<void> {
  const rec = activeRecordings.get(input.recordingId);
  if (!rec) {
    throw new Error("Unknown recording id");
  }
  await new Promise<void>((resolve, reject) => {
    rec.writeStream.write(Buffer.from(input.chunk), (err) => {
      if (err) reject(err);
      else resolve();
    });
  });
}

export async function finishRecording(
  input: RecordingFinishRequest,
): Promise<RecordingEntry> {
  const rec = activeRecordings.get(input.recordingId);
  if (!rec) {
    throw new Error("Unknown recording id");
  }
  activeRecordings.delete(input.recordingId);

  await new Promise<void>((resolve, reject) => {
    rec.writeStream.end((err?: Error | null) => {
      if (err) reject(err);
      else resolve();
    });
  });

  const dir = getRecordingsDirectory();
  const stamp = new Date().toISOString().replace(/[:.]/g, "-");
  const title = sanitizeTitleForFileName(input.gameTitle);
  const rand = Math.random().toString(16).slice(2, 8);
  const durSuffix =
    input.durationMs > 0 ? `-dur${Math.round(input.durationMs)}` : "";
  const ext = extFromMimeType(rec.mimeType);
  const fileName = `${stamp}-${title}-${rand}${durSuffix}${ext}`;
  const finalPath = join(dir, fileName);

  await rename(rec.tempPath, finalPath);

  // Save thumbnail if provided
  let thumbnailDataUrl: string | undefined;
  if (input.thumbnailDataUrl) {
    try {
      const { buffer } = dataUrlToBuffer(input.thumbnailDataUrl);
      const stem = fileName.replace(/\.(mp4|webm)$/i, "");
      const thumbPath = join(dir, `${stem}-thumb.jpg`);
      await writeFile(thumbPath, buffer);
      thumbnailDataUrl = input.thumbnailDataUrl;
    } catch {
      // Thumbnail save is best-effort — don't fail the recording
    }
  }

  // Enforce recording limit: delete oldest entries beyond RECORDING_LIMIT
  const all = await listRecordings();
  if (all.length > RECORDING_LIMIT) {
    const toDelete = all.slice(RECORDING_LIMIT);
    await Promise.all(
      toDelete.map(async (entry) => {
        await unlink(entry.filePath).catch(() => undefined);
        const stem = entry.fileName.replace(/\.(mp4|webm)$/i, "");
        await unlink(join(dir, `${stem}-thumb.jpg`)).catch(() => undefined);
      }),
    );
  }

  const fileStats = await stat(finalPath);
  return {
    id: fileName,
    fileName,
    filePath: finalPath,
    createdAtMs: Date.now(),
    sizeBytes: fileStats.size,
    durationMs: input.durationMs,
    gameTitle: input.gameTitle,
    thumbnailDataUrl,
  };
}

export async function abortRecording(
  input: RecordingAbortRequest,
): Promise<void> {
  const rec = activeRecordings.get(input.recordingId);
  if (!rec) {
    return;
  }
  activeRecordings.delete(input.recordingId);
  rec.writeStream.destroy();
  await unlink(rec.tempPath).catch(() => undefined);
}

export async function deleteRecording(
  input: RecordingDeleteRequest,
): Promise<void> {
  assertSafeRecordingId(input.id);
  const dir = await ensureRecordingsDirectory();
  const filePath = join(dir, input.id);
  await unlink(filePath);
  const stem = input.id.replace(/\.(mp4|webm)$/i, "");
  await unlink(join(dir, `${stem}-thumb.jpg`)).catch(() => undefined);
}

export async function getRecordingFilePath(id: string): Promise<string> {
  assertSafeRecordingId(id);
  const dir = await ensureRecordingsDirectory();
  return join(dir, id);
}
