/**
 * Loads screenshot image URLs for a library game title (newest first).
 * Uses embedded data URLs when present, otherwise resolves via getMediaThumbnail when available.
 */
export async function loadScreenshotUrlsForGameTitle(gameTitle: string): Promise<string[]> {
  const trimmed = gameTitle.trim();
  if (!trimmed) return [];
  if (typeof window.openNow?.listMediaByGame !== "function") return [];

  try {
    const listing = await window.openNow.listMediaByGame({ gameTitle: trimmed });
    const rows = [...(listing.screenshots ?? [])].sort((a, b) => b.createdAtMs - a.createdAtMs);
    const urls: string[] = [];

    for (const s of rows) {
      let u = s.thumbnailDataUrl || s.dataUrl;
      if (!u && typeof window.openNow?.getMediaThumbnail === "function") {
        try {
          u = (await window.openNow.getMediaThumbnail({ filePath: s.filePath })) ?? undefined;
        } catch {
          u = undefined;
        }
      }
      if (u) urls.push(u);
    }

    return urls;
  } catch {
    return [];
  }
}
