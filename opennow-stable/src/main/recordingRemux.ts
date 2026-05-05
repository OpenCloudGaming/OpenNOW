import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { rename, unlink } from "node:fs/promises";

const execFileAsync = promisify(execFile);

/**
 * Optional post-save remux (`ffmpeg -c copy`) to normalize container duration/metadata.
 * Requires `ffmpeg` on PATH. On failure, leaves the original file unchanged.
 */
export async function maybeRemuxRecordingInPlace(
  filePath: string,
  mimeType: string,
  enabled: boolean,
): Promise<void> {
  if (!enabled) return;

  const isMp4 =
    mimeType.toLowerCase().includes("mp4") || filePath.toLowerCase().endsWith(".mp4");
  const tempPath = `${filePath}.remux.tmp`;
  const args = ["-hide_banner", "-loglevel", "error", "-y", "-i", filePath, "-c", "copy"];
  if (isMp4) {
    args.push("-movflags", "+faststart");
  }
  args.push(tempPath);

  try {
    await execFileAsync("ffmpeg", args, { timeout: 300_000, maxBuffer: 20 * 1024 * 1024 });
  } catch (err) {
    await unlink(tempPath).catch(() => undefined);
    console.warn("[Recording] ffmpeg remux skipped or failed:", err);
    return;
  }

  const backupPath = `${filePath}.bak`;
  try {
    await rename(filePath, backupPath);
  } catch {
    await unlink(tempPath).catch(() => undefined);
    return;
  }
  try {
    await rename(tempPath, filePath);
    await unlink(backupPath).catch(() => undefined);
  } catch {
    await rename(backupPath, filePath).catch(() => undefined);
    await unlink(tempPath).catch(() => undefined);
  }
}

/**
 * Instant replay: re-mux with stream copy and timestamp sanitization so players and metadata see a clip
 * that starts near t=0 (rolling buffer drops use the same init segment; cluster PTS may be offset).
 * Best-effort; leaves the file unchanged if ffmpeg is missing or fails.
 */
/** @returns true if the file was replaced successfully */
export async function finalizeInstantReplayFileInPlace(
  filePath: string,
  mimeType: string,
  addMp4Faststart: boolean,
): Promise<boolean> {
  const isMp4 =
    mimeType.toLowerCase().includes("mp4") || filePath.toLowerCase().endsWith(".mp4");
  const tempPath = `${filePath}.ireplay-finalize.tmp`;
  const args = [
    "-hide_banner",
    "-loglevel",
    "error",
    "-y",
    "-i",
    filePath,
    "-map",
    "0",
    "-c",
    "copy",
    "-avoid_negative_ts",
    "make_zero",
  ];
  if (isMp4 && addMp4Faststart) {
    args.push("-movflags", "+faststart");
  }
  args.push(tempPath);

  try {
    await execFileAsync("ffmpeg", args, { timeout: 300_000, maxBuffer: 20 * 1024 * 1024 });
  } catch (err) {
    await unlink(tempPath).catch(() => undefined);
    console.warn("[InstantReplay] ffmpeg finalize (timestamp normalize) skipped or failed:", err);
    return false;
  }

  const backupPath = `${filePath}.bak`;
  try {
    await rename(filePath, backupPath);
  } catch {
    await unlink(tempPath).catch(() => undefined);
    return false;
  }
  try {
    await rename(tempPath, filePath);
    await unlink(backupPath).catch(() => undefined);
    return true;
  } catch {
    await rename(backupPath, filePath).catch(() => undefined);
    await unlink(tempPath).catch(() => undefined);
    return false;
  }
}
