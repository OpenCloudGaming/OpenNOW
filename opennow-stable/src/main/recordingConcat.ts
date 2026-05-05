import { execFile } from "node:child_process";
import { copyFile, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

/**
 * Concatenate same-codec container fragments with ffmpeg's concat demuxer (`-c copy`).
 * Each `partPath` must be a readable standalone file (e.g. complete WebM or MP4 segments).
 */
export async function concatMediaFilesWithFfmpegCopy(partPaths: string[], outPath: string): Promise<void> {
  if (partPaths.length === 0) {
    throw new Error("concatMediaFilesWithFfmpegCopy: no parts");
  }
  if (partPaths.length === 1) {
    await copyFile(partPaths[0]!, outPath);
    return;
  }

  const listDir = await mkdtemp(join(tmpdir(), "opennow-concat-"));
  const listPath = join(listDir, "list.txt");
  const lines = partPaths.map((p) => {
    const escaped = p.replace(/\\/g, "/").replace(/'/g, "'\\''");
    return `file '${escaped}'`;
  });
  await writeFile(listPath, `${lines.join("\n")}\n`, "utf8");

  try {
    await execFileAsync(
      "ffmpeg",
      [
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-f",
        "concat",
        "-safe",
        "0",
        "-i",
        listPath,
        "-c",
        "copy",
        outPath,
      ],
      { timeout: 600_000, maxBuffer: 20 * 1024 * 1024 },
    );
  } finally {
    await rm(listDir, { recursive: true }).catch(() => undefined);
  }
}
