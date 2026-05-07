import { app, type BrowserWindow } from "electron";
import { join } from "node:path";
import { copyFile, mkdir, readdir, readFile, stat, unlink, writeFile } from "node:fs/promises";
import type {
  ScreenshotDeleteRequest,
  ScreenshotEntry,
  ScreenshotSaveAsRequest,
  ScreenshotSaveAsResult,
  ScreenshotSaveRequest,
} from "@shared/gfn";
import {
  assertSafeMediaId,
  dataUrlToBuffer,
  sanitizeTitleForFileName,
} from "./mediaFiles";

const SCREENSHOT_LIMIT = 60;

export interface ScreenshotSaveAsDeps {
  dialog: Electron.Dialog;
  getMainWindow(): BrowserWindow | null;
}

export function getScreenshotDirectory(): string {
  return join(app.getPath("pictures"), "OpenNOW", "Screenshots");
}

export async function ensureScreenshotDirectory(): Promise<string> {
  const dir = getScreenshotDirectory();
  await mkdir(dir, { recursive: true });
  return dir;
}

export function buildScreenshotDataUrl(ext: string, buffer: Buffer): string {
  const mime =
    ext === "jpg" ? "image/jpeg" : ext === "webp" ? "image/webp" : "image/png";
  return `data:${mime};base64,${buffer.toString("base64")}`;
}

export function assertSafeScreenshotId(id: string): void {
  assertSafeMediaId(id, "screenshot");
}

export async function listScreenshots(): Promise<ScreenshotEntry[]> {
  const dir = await ensureScreenshotDirectory();
  const entries = await readdir(dir, { withFileTypes: true });
  const screenshotFiles = entries
    .filter((entry) => entry.isFile())
    .map((entry) => entry.name)
    .filter((name) => /\.(png|jpg|jpeg|webp)$/i.test(name));

  const loaded = await Promise.all(
    screenshotFiles.map(async (fileName): Promise<ScreenshotEntry | null> => {
      const filePath = join(dir, fileName);
      try {
        const fileStats = await stat(filePath);
        const fileBuffer = await readFile(filePath);
        const extMatch = /\.([^.]+)$/.exec(fileName);
        const ext = (extMatch?.[1] ?? "png").toLowerCase();

        return {
          id: fileName,
          fileName,
          filePath,
          createdAtMs: fileStats.birthtimeMs || fileStats.mtimeMs,
          sizeBytes: fileStats.size,
          dataUrl: buildScreenshotDataUrl(ext, fileBuffer),
        };
      } catch {
        return null;
      }
    }),
  );

  return loaded
    .filter((item): item is ScreenshotEntry => item !== null)
    .sort((a, b) => b.createdAtMs - a.createdAtMs)
    .slice(0, SCREENSHOT_LIMIT);
}

export async function saveScreenshot(
  input: ScreenshotSaveRequest,
): Promise<ScreenshotEntry> {
  const { ext, buffer } = dataUrlToBuffer(input.dataUrl);
  const dir = await ensureScreenshotDirectory();
  const stamp = new Date().toISOString().replace(/[:.]/g, "-");
  const title = sanitizeTitleForFileName(input.gameTitle);
  const fileName = `${stamp}-${title}-${Math.random().toString(16).slice(2, 8)}.${ext}`;
  const filePath = join(dir, fileName);

  await writeFile(filePath, buffer);

  return {
    id: fileName,
    fileName,
    filePath,
    createdAtMs: Date.now(),
    sizeBytes: buffer.byteLength,
    dataUrl: buildScreenshotDataUrl(ext, buffer),
  };
}

export async function deleteScreenshot(
  input: ScreenshotDeleteRequest,
): Promise<void> {
  assertSafeScreenshotId(input.id);
  const dir = await ensureScreenshotDirectory();
  const filePath = join(dir, input.id);
  await unlink(filePath);
}

export async function saveScreenshotAs(
  input: ScreenshotSaveAsRequest,
  deps: ScreenshotSaveAsDeps,
): Promise<ScreenshotSaveAsResult> {
  assertSafeScreenshotId(input.id);
  const dir = await ensureScreenshotDirectory();
  const sourcePath = join(dir, input.id);

  const saveDialogOptions = {
    title: "Save Screenshot",
    defaultPath: join(app.getPath("pictures"), input.id),
    filters: [
      { name: "PNG Image", extensions: ["png"] },
      { name: "JPEG Image", extensions: ["jpg", "jpeg"] },
      { name: "WebP Image", extensions: ["webp"] },
      { name: "All Files", extensions: ["*"] },
    ],
  };
  const mainWindow = deps.getMainWindow();
  const target =
    mainWindow && !mainWindow.isDestroyed()
      ? await deps.dialog.showSaveDialog(mainWindow, saveDialogOptions)
      : await deps.dialog.showSaveDialog(saveDialogOptions);

  if (target.canceled || !target.filePath) {
    return { saved: false };
  }

  await copyFile(sourcePath, target.filePath);
  return { saved: true, filePath: target.filePath };
}
