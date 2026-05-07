import type { BrowserWindow, IpcMain, Shell } from "electron";
import { unlink } from "node:fs/promises";
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
import {
  getTrustedVideoPlaybackFileUrl,
  resolveTrustedOpenNowMediaPath,
} from "../mediaPaths";
import {
  deleteScreenshot,
  listScreenshots,
  saveScreenshot,
  saveScreenshotAs,
} from "../media/screenshots";
import {
  abortRecording,
  appendRecordingChunk,
  beginRecording,
  deleteRecording,
  getRecordingFilePath,
  listRecordings,
  finishRecording,
} from "../media/recordings";
import {
  deleteThumbnailArtifactsForTrustedPath,
  readThumbnailDataUrlForTrustedPath,
  regenerateThumbnailForTrustedPath,
} from "../media/thumbnails";

export interface MediaIpcHandlerDeps {
  ipcMain: IpcMain;
  dialog: Electron.Dialog;
  shell: Shell;
  getMainWindow(): BrowserWindow | null;
}

export function registerMediaIpcHandlers(deps: MediaIpcHandlerDeps): void {
  deps.ipcMain.handle(
    IPC_CHANNELS.SCREENSHOT_SAVE,
    async (_event, input: ScreenshotSaveRequest): Promise<ScreenshotEntry> => {
      return saveScreenshot(input);
    },
  );

  deps.ipcMain.handle(
    IPC_CHANNELS.SCREENSHOT_LIST,
    async (): Promise<ScreenshotEntry[]> => {
      return listScreenshots();
    },
  );

  deps.ipcMain.handle(
    IPC_CHANNELS.MEDIA_LIST_BY_GAME,
    async (_event, payload: { gameTitle?: string } = {}) => {
      const title = (payload?.gameTitle || "").trim().toLowerCase();
      const screenshots = await listScreenshots();
      const recordings = await listRecordings();

      const normalize = (s?: string) =>
        (s || "").replace(/[^a-z0-9]+/gi, "").toLowerCase();
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
    },
  );

  deps.ipcMain.handle(
    IPC_CHANNELS.SCREENSHOT_DELETE,
    async (_event, input: ScreenshotDeleteRequest): Promise<void> => {
      return deleteScreenshot(input);
    },
  );

  deps.ipcMain.handle(
    IPC_CHANNELS.SCREENSHOT_SAVE_AS,
    async (
      _event,
      input: ScreenshotSaveAsRequest,
    ): Promise<ScreenshotSaveAsResult> => {
      return saveScreenshotAs(input, deps);
    },
  );

  deps.ipcMain.handle(
    IPC_CHANNELS.RECORDING_BEGIN,
    async (
      _event,
      input: RecordingBeginRequest,
    ): Promise<RecordingBeginResult> => {
      return beginRecording(input);
    },
  );

  deps.ipcMain.handle(
    IPC_CHANNELS.RECORDING_CHUNK,
    async (_event, input: RecordingChunkRequest): Promise<void> => {
      return appendRecordingChunk(input);
    },
  );

  deps.ipcMain.handle(
    IPC_CHANNELS.RECORDING_FINISH,
    async (_event, input: RecordingFinishRequest): Promise<RecordingEntry> => {
      return finishRecording(input);
    },
  );

  deps.ipcMain.handle(
    IPC_CHANNELS.RECORDING_ABORT,
    async (_event, input: RecordingAbortRequest): Promise<void> => {
      return abortRecording(input);
    },
  );

  deps.ipcMain.handle(
    IPC_CHANNELS.RECORDING_LIST,
    async (): Promise<RecordingEntry[]> => {
      return listRecordings();
    },
  );

  deps.ipcMain.handle(
    IPC_CHANNELS.RECORDING_DELETE,
    async (_event, input: RecordingDeleteRequest): Promise<void> => {
      return deleteRecording(input);
    },
  );

  deps.ipcMain.handle(
    IPC_CHANNELS.RECORDING_SHOW_IN_FOLDER,
    async (_event, id: string): Promise<void> => {
      deps.shell.showItemInFolder(await getRecordingFilePath(id));
    },
  );

  deps.ipcMain.handle(
    IPC_CHANNELS.MEDIA_THUMBNAIL,
    async (_event, payload: { filePath: string }): Promise<string | null> => {
      const rawFp = payload?.filePath;
      if (typeof rawFp !== "string") return null;
      try {
        const fpReal = await resolveTrustedOpenNowMediaPath(rawFp);
        if (!fpReal) return null;
        return readThumbnailDataUrlForTrustedPath(fpReal);
      } catch (err) {
        console.warn("MEDIA_THUMBNAIL error:", err);
        return null;
      }
    },
  );

  deps.ipcMain.handle(
    IPC_CHANNELS.MEDIA_SHOW_IN_FOLDER,
    async (_event, payload: { filePath: string }): Promise<void> => {
      const rawFp = payload?.filePath;
      if (typeof rawFp !== "string") return;
      try {
        const fpReal = await resolveTrustedOpenNowMediaPath(rawFp);
        if (!fpReal) return;
        deps.shell.showItemInFolder(fpReal);
      } catch {
        return;
      }
    },
  );

  deps.ipcMain.handle(
    IPC_CHANNELS.MEDIA_PLAYBACK_URL,
    async (_event, payload: { filePath: string }): Promise<string | null> => {
      const rawFp = payload?.filePath;
      if (typeof rawFp !== "string") return null;
      try {
        return await getTrustedVideoPlaybackFileUrl(rawFp);
      } catch (err) {
        console.warn("MEDIA_PLAYBACK_URL error:", err);
        return null;
      }
    },
  );

  deps.ipcMain.handle(
    IPC_CHANNELS.MEDIA_DELETE_FILE,
    async (_event, payload: { filePath: string }): Promise<{ ok: boolean }> => {
      const rawFp = payload?.filePath;
      if (typeof rawFp !== "string") return { ok: false };
      try {
        const fpReal = await resolveTrustedOpenNowMediaPath(rawFp);
        if (!fpReal) return { ok: false };
        const mediaFileExists = await deleteThumbnailArtifactsForTrustedPath(fpReal);
        if (!mediaFileExists) return { ok: false };
        await unlink(fpReal);
        return { ok: true };
      } catch (err) {
        console.warn("MEDIA_DELETE_FILE error:", err);
        return { ok: false };
      }
    },
  );

  deps.ipcMain.handle(
    IPC_CHANNELS.MEDIA_REGEN_THUMBNAIL,
    async (
      _event,
      payload: { filePath: string },
    ): Promise<{ ok: boolean; thumbnailDataUrl: string | null }> => {
      const rawFp = payload?.filePath;
      if (typeof rawFp !== "string")
        return { ok: false, thumbnailDataUrl: null };
      try {
        const fpReal = await resolveTrustedOpenNowMediaPath(rawFp);
        if (!fpReal) return { ok: false, thumbnailDataUrl: null };
        return regenerateThumbnailForTrustedPath(fpReal);
      } catch (err) {
        console.warn("MEDIA_REGEN_THUMBNAIL error:", err);
        return { ok: false, thumbnailDataUrl: null };
      }
    },
  );
}
