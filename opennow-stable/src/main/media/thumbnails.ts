import { app } from "electron";
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { join } from "node:path";
import { copyFile, mkdir, readFile, stat, unlink, writeFile } from "node:fs/promises";
import { buildImageDataUrl } from "./mediaFiles";

export function getThumbnailCacheDirectory(): string {
  return join(app.getPath("userData"), "media-thumbs");
}

export async function ensureThumbnailCacheDirectory(): Promise<string> {
  const dir = getThumbnailCacheDirectory();
  await mkdir(dir, { recursive: true });
  return dir;
}

export function md5(input: string): string {
  return createHash("md5").update(input).digest("hex");
}

/** Seconds; null if ffprobe missing or unreadable. */
export async function probeVideoDurationSeconds(
  sourcePath: string,
): Promise<number | null> {
  return new Promise((resolve) => {
    const args = [
      "-v",
      "error",
      "-show_entries",
      "format=duration",
      "-of",
      "default=noprint_wrappers=1:nokey=1",
      sourcePath,
    ];
    const child = spawn("ffprobe", args, {
      stdio: ["ignore", "pipe", "ignore"],
    });
    let out = "";
    child.stdout?.on("data", (chunk: Buffer) => {
      out += chunk.toString();
    });
    child.on("error", () => resolve(null));
    child.on("close", (code) => {
      if (code !== 0) {
        resolve(null);
        return;
      }
      const n = Number.parseFloat(out.trim());
      resolve(Number.isFinite(n) && n > 0 ? n : null);
    });
  });
}

export function randomThumbnailSeekSeconds(durationSec: number | null): number {
  if (durationSec !== null && durationSec > 0.2) {
    const margin = Math.min(0.35, durationSec * 0.08);
    const hi = Math.max(durationSec - margin, margin + 0.05);
    const lo = Math.min(margin, hi * 0.5);
    return lo + Math.random() * (hi - lo);
  }
  return 0.2 + Math.random() * 4.8;
}

export function ffmpegExtractOneFrame(
  sourcePath: string,
  outPath: string,
  seekSec: number,
): Promise<boolean> {
  const ss = seekSec.toFixed(3);
  return new Promise<boolean>((resolve) => {
    const args = [
      "-y",
      "-ss",
      ss,
      "-i",
      sourcePath,
      "-frames:v",
      "1",
      "-q:v",
      "2",
      outPath,
    ];
    const child = spawn("ffmpeg", args, { stdio: "ignore" });
    child.on("error", () => resolve(false));
    child.on("close", (code) => {
      resolve(code === 0);
    });
  });
}

export async function generateVideoThumbnail(
  sourcePath: string,
  outPath: string,
): Promise<boolean> {
  const durationSec = await probeVideoDurationSeconds(sourcePath);
  const seekSec = randomThumbnailSeekSeconds(durationSec);
  if (await ffmpegExtractOneFrame(sourcePath, outPath, seekSec)) return true;
  if (seekSec > 0.02) return ffmpegExtractOneFrame(sourcePath, outPath, 0);
  return false;
}

export async function ensureThumbnailForMedia(
  filePath: string,
): Promise<string | null> {
  try {
    const stats = await stat(filePath);
    const key = md5(`${filePath}|${stats.mtimeMs}`);
    const cacheDir = await ensureThumbnailCacheDirectory();
    const outPath = join(cacheDir, `${key}.jpg`);
    // If cached, return
    try {
      await stat(outPath);
      return outPath;
    } catch {
      // not exists
    }

    const lower = filePath.toLowerCase();
    if (isVideoMediaFilePath(lower)) {
      const ok = await generateVideoThumbnail(filePath, outPath);
      if (ok) return outPath;
      // generation failed
      return null;
    }

    // For images, copy into cache (no re-encoding)
    if (isImageMediaFilePath(lower)) {
      try {
        const buf = await readFile(filePath);
        await writeFile(outPath, buf);
        return outPath;
      } catch {
        return null;
      }
    }

    return null;
  } catch (err) {
    console.warn("ensureThumbnailForMedia error:", err);
    return null;
  }
}

export function isImageMediaFilePath(filePath: string): boolean {
  const lower = filePath.toLowerCase();
  return (
    lower.endsWith(".png") ||
    lower.endsWith(".jpg") ||
    lower.endsWith(".jpeg") ||
    lower.endsWith(".webp")
  );
}

export function isVideoMediaFilePath(filePath: string): boolean {
  const lower = filePath.toLowerCase();
  return (
    lower.endsWith(".mp4") ||
    lower.endsWith(".webm") ||
    lower.endsWith(".mkv") ||
    lower.endsWith(".mov")
  );
}

export async function readThumbnailDataUrlForTrustedPath(
  filePath: string,
): Promise<string | null> {
  const lower = filePath.toLowerCase();
  if (isImageMediaFilePath(lower)) {
    const buf = await readFile(filePath);
    const extMatch = /\.([^.]+)$/.exec(filePath);
    const ext = (extMatch?.[1] || "png").toLowerCase();
    return buildImageDataUrl(ext, buf);
  }

  if (isVideoMediaFilePath(lower)) {
    const stem = filePath.replace(/\.(mp4|webm|mkv|mov)$/i, "");
    const thumbPath = `${stem}-thumb.jpg`;
    try {
      const b = await readFile(thumbPath);
      return `data:image/jpeg;base64,${b.toString("base64")}`;
    } catch {
      // Try generating a cached thumbnail via ffmpeg
    }

    const gen = await ensureThumbnailForMedia(filePath);
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
}

export async function deleteThumbnailArtifactsForTrustedPath(
  filePath: string,
): Promise<boolean> {
  let st: Awaited<ReturnType<typeof stat>>;
  try {
    st = await stat(filePath);
  } catch {
    return false;
  }
  const key = md5(`${filePath}|${st.mtimeMs}`);
  const cacheDir = await ensureThumbnailCacheDirectory();
  await unlink(join(cacheDir, `${key}.jpg`)).catch(() => undefined);
  const stem = filePath.replace(
    /\.(mp4|webm|mkv|mov|png|jpg|jpeg|webp)$/i,
    "",
  );
  await unlink(`${stem}-thumb.jpg`).catch(() => undefined);
  return true;
}

export async function regenerateThumbnailForTrustedPath(
  filePath: string,
): Promise<{ ok: boolean; thumbnailDataUrl: string | null }> {
  const st = await stat(filePath);
  const key = md5(`${filePath}|${st.mtimeMs}`);
  const cacheDir = await ensureThumbnailCacheDirectory();
  await unlink(join(cacheDir, `${key}.jpg`)).catch(() => undefined);
  if (/\.(mp4|webm|mkv|mov)$/i.test(filePath)) {
    const videoStem = filePath.replace(/\.(mp4|webm|mkv|mov)$/i, "");
    await unlink(`${videoStem}-thumb.jpg`).catch(() => undefined);
  }
  const genPath = await ensureThumbnailForMedia(filePath);
  if (!genPath) return { ok: false, thumbnailDataUrl: null };

  if (/\.(mp4|webm|mkv|mov)$/i.test(filePath)) {
    const videoStem = filePath.replace(/\.(mp4|webm|mkv|mov)$/i, "");
    const sidecar = `${videoStem}-thumb.jpg`;
    await copyFile(genPath, sidecar).catch((err) => {
      console.warn("MEDIA_REGEN_THUMBNAIL sidecar copy:", err);
    });
  }

  const b = await readFile(genPath);
  return {
    ok: true,
    thumbnailDataUrl: `data:image/jpeg;base64,${b.toString("base64")}`,
  };
}
