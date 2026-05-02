/** Safe filename segment from a game title or similar. */
export function sanitizeTitleForFileName(value: string | undefined): string {
  const source = (value ?? "").trim().toLowerCase();
  const compact = source.replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
  if (!compact) return "stream";
  return compact.slice(0, 48);
}

export function dataUrlToBuffer(dataUrl: string): { ext: "png" | "jpg" | "webp"; buffer: Buffer } {
  const match = /^data:image\/(png|jpeg|jpg|webp);base64,([a-z0-9+/=\s]+)$/i.exec(dataUrl);
  if (!match || !match[1] || !match[2]) {
    throw new Error("Invalid screenshot payload");
  }

  const rawExt = match[1].toLowerCase();
  const ext: "png" | "jpg" | "webp" = rawExt === "jpeg" ? "jpg" : (rawExt as "png" | "jpg" | "webp");
  const buffer = Buffer.from(match[2].replace(/\s+/g, ""), "base64");
  if (!buffer.length) {
    throw new Error("Empty screenshot payload");
  }

  return { ext, buffer };
}

export function buildScreenshotDataUrl(ext: string, buffer: Buffer): string {
  const mime = ext === "jpg" ? "image/jpeg" : ext === "webp" ? "image/webp" : "image/png";
  return `data:${mime};base64,${buffer.toString("base64")}`;
}

export function assertSafeScreenshotId(id: string): void {
  if (!id || id.includes("/") || id.includes("\\") || id.includes("..")) {
    throw new Error("Invalid screenshot id");
  }
}

export function assertSafeRecordingId(id: string): void {
  if (!id || id.includes("/") || id.includes("\\") || id.includes("..")) {
    throw new Error("Invalid recording id");
  }
}
