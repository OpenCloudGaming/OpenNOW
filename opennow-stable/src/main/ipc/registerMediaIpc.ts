import { randomUUID } from "node:crypto";
import { join, resolve, relative } from "node:path";
import { createWriteStream } from "node:fs";
import { readFile, rename, stat, unlink, writeFile, realpath } from "node:fs/promises";
import { IPC_CHANNELS } from "@shared/ipc";
import type {
  RecordingAbortRequest,
  RecordingBeginRequest,
  RecordingBeginResult,
  RecordingChunkRequest,
  RecordingDeleteRequest,
  RecordingEntry,
  RecordingFinishRequest,
  ScreenshotDeleteRequest,
  ScreenshotEntry,
  ScreenshotSaveAsRequest,
  ScreenshotSaveAsResult,
  ScreenshotSaveRequest,
} from "@shared/gfn";
import type { MainIpcDeps } from "./types";
import { assertSafeRecordingId } from "../lib/imageDataUrl";

export function registerMediaIpc(deps: MainIpcDeps): void {
  const {
    ipcMain,
    app,
    shell,
    activeRecordings,
    saveScreenshot,
    listScreenshots,
    deleteScreenshot,
    saveScreenshotAs,
    dataUrlToBuffer,
    sanitizeTitleForFileName,
    ensureRecordingsDirectory,
    getRecordingsDirectory,
    listRecordings,
    ensureThumbnailForMedia,
    extFromMimeType,
    RECORDING_LIMIT,
  } = deps;

  ipcMain.handle(IPC_CHANNELS.SCREENSHOT_SAVE, async (_event, input: ScreenshotSaveRequest): Promise<ScreenshotEntry> => {
    return saveScreenshot(input);
  });

  ipcMain.handle(IPC_CHANNELS.SCREENSHOT_LIST, async (): Promise<ScreenshotEntry[]> => {
    return listScreenshots();
  });

  ipcMain.handle(IPC_CHANNELS.MEDIA_LIST_BY_GAME, async (_event, payload: { gameTitle?: string } = {}) => {
    const title = (payload?.gameTitle || "").trim().toLowerCase();
    const screenshots = await listScreenshots();
    const recordings = await listRecordings();

    const normalize = (s?: string) => (s || "").replace(/[^a-z0-9]+/gi, "").toLowerCase();
    const needle = normalize(title);

    const matchedScreens = screenshots.filter((s) => {
      if (!needle) return true;
      const candidate = normalize(s.fileName) + normalize(s.filePath || "");
      return candidate.includes(needle);
    });

    const matchedRecordings = recordings.filter((r) => {
      if (!needle) return true;
      const candidate = normalize(r.gameTitle ?? r.fileName ?? "");
      return candidate.includes(needle);
    });

    return {
      screenshots: matchedScreens,
      videos: matchedRecordings,
    };
  });

  ipcMain.handle(IPC_CHANNELS.SCREENSHOT_DELETE, async (_event, input: ScreenshotDeleteRequest): Promise<void> => {
    return deleteScreenshot(input);
  });

  ipcMain.handle(
    IPC_CHANNELS.SCREENSHOT_SAVE_AS,
    async (_event, input: ScreenshotSaveAsRequest): Promise<ScreenshotSaveAsResult> => {
      return saveScreenshotAs(input);
    },
  );

  ipcMain.handle(IPC_CHANNELS.RECORDING_BEGIN, async (_event, input: RecordingBeginRequest): Promise<RecordingBeginResult> => {
    const dir = await ensureRecordingsDirectory();
    const recordingId = randomUUID();
    const ext = extFromMimeType(input.mimeType);
    const tempPath = join(dir, `${recordingId}${ext}.tmp`);
    const writeStream = createWriteStream(tempPath);
    activeRecordings.set(recordingId, { writeStream, tempPath, mimeType: input.mimeType });
    return { recordingId };
  });

  ipcMain.handle(IPC_CHANNELS.RECORDING_CHUNK, async (_event, input: RecordingChunkRequest): Promise<void> => {
    const rec = activeRecordings.get(input.recordingId);
    if (!rec) {
      throw new Error("Unknown recording id");
    }
    await new Promise<void>((resolveChunk, rejectChunk) => {
      rec.writeStream.write(Buffer.from(input.chunk), (err) => {
        if (err) rejectChunk(err);
        else resolveChunk();
      });
    });
  });

  ipcMain.handle(IPC_CHANNELS.RECORDING_FINISH, async (_event, input: RecordingFinishRequest): Promise<RecordingEntry> => {
    const rec = activeRecordings.get(input.recordingId);
    if (!rec) {
      throw new Error("Unknown recording id");
    }
    activeRecordings.delete(input.recordingId);

    await new Promise<void>((resolveEnd, rejectEnd) => {
      rec.writeStream.end((err?: Error | null) => {
        if (err) rejectEnd(err);
        else resolveEnd();
      });
    });

    const dir = getRecordingsDirectory();
    const stamp = new Date().toISOString().replace(/[:.]/g, "-");
    const title = sanitizeTitleForFileName(input.gameTitle);
    const rand = Math.random().toString(16).slice(2, 8);
    const durSuffix = input.durationMs > 0 ? `-dur${Math.round(input.durationMs)}` : "";
    const ext = extFromMimeType(rec.mimeType);
    const fileName = `${stamp}-${title}-${rand}${durSuffix}${ext}`;
    const finalPath = join(dir, fileName);

    await rename(rec.tempPath, finalPath);

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
  });

  ipcMain.handle(IPC_CHANNELS.RECORDING_ABORT, async (_event, input: RecordingAbortRequest): Promise<void> => {
    const rec = activeRecordings.get(input.recordingId);
    if (!rec) {
      return;
    }
    activeRecordings.delete(input.recordingId);
    rec.writeStream.destroy();
    await unlink(rec.tempPath).catch(() => undefined);
  });

  ipcMain.handle(IPC_CHANNELS.RECORDING_LIST, async (): Promise<RecordingEntry[]> => {
    return listRecordings();
  });

  ipcMain.handle(IPC_CHANNELS.RECORDING_DELETE, async (_event, input: RecordingDeleteRequest): Promise<void> => {
    assertSafeRecordingId(input.id);
    const dir = await ensureRecordingsDirectory();
    const filePath = join(dir, input.id);
    await unlink(filePath);
    const stem = input.id.replace(/\.(mp4|webm)$/i, "");
    await unlink(join(dir, `${stem}-thumb.jpg`)).catch(() => undefined);
  });

  ipcMain.handle(IPC_CHANNELS.RECORDING_SHOW_IN_FOLDER, async (_event, id: string): Promise<void> => {
    assertSafeRecordingId(id);
    const dir = await ensureRecordingsDirectory();
    shell.showItemInFolder(join(dir, id));
  });

  ipcMain.handle(IPC_CHANNELS.MEDIA_THUMBNAIL, async (_event, payload: { filePath: string }): Promise<string | null> => {
    const rawFp = payload?.filePath;
    if (typeof rawFp !== "string") return null;
    if (rawFp.length > 4096) return null;
    try {
      const allowedRoot = resolve(join(app.getPath("pictures"), "OpenNOW"));
      const fpResolved = resolve(rawFp);
      const allowedRootReal = await realpath(allowedRoot).catch(() => allowedRoot);
      const fpReal = await realpath(fpResolved).catch(() => fpResolved);
      const rel = relative(allowedRootReal, fpReal);
      if (rel.startsWith("..")) return null;

      const lower = fpReal.toLowerCase();
      if (lower.endsWith(".png") || lower.endsWith(".jpg") || lower.endsWith(".jpeg") || lower.endsWith(".webp")) {
        const buf = await readFile(fpReal);
        const extMatch = /\.([^.]+)$/.exec(fpReal);
        const ext = (extMatch?.[1] || "png").toLowerCase();
        const mime = ext === "jpg" || ext === "jpeg" ? "image/jpeg" : ext === "webp" ? "image/webp" : "image/png";
        return `data:${mime};base64,${buf.toString("base64")}`;
      }

      if (lower.endsWith(".mp4") || lower.endsWith(".webm") || lower.endsWith(".mkv") || lower.endsWith(".mov")) {
        const stem = fpReal.replace(/\.(mp4|webm|mkv|mov)$/i, "");
        const thumbPath = `${stem}-thumb.jpg`;
        try {
          const b = await readFile(thumbPath);
          return `data:image/jpeg;base64,${b.toString("base64")}`;
        } catch {
          // Try generating a cached thumbnail via ffmpeg
        }

        const gen = await ensureThumbnailForMedia(fpReal);
        if (gen) {
          try {
            const b2 = await readFile(gen);
            return `data:image/jpeg;base64,${b2.toString("base64")}`;
          } catch {
            return null;
          }
        }
        return null;
      }

      return null;
    } catch (err) {
      console.warn("MEDIA_THUMBNAIL error:", err);
      return null;
    }
  });

  ipcMain.handle(IPC_CHANNELS.MEDIA_SHOW_IN_FOLDER, async (_event, payload: { filePath: string }): Promise<void> => {
    const rawFp = payload?.filePath;
    if (typeof rawFp !== "string") return;
    try {
      const allowedRoot = resolve(join(app.getPath("pictures"), "OpenNOW"));
      const fpResolved = resolve(rawFp);
      const allowedRootReal = await realpath(allowedRoot).catch(() => allowedRoot);
      const fpReal = await realpath(fpResolved).catch(() => fpResolved);
      const rel = relative(allowedRootReal, fpReal);
      if (rel.startsWith("..")) return;
      shell.showItemInFolder(fpReal);
    } catch {
      return;
    }
  });
}
