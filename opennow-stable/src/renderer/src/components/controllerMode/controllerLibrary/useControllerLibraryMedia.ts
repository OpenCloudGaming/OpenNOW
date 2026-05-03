import { useEffect, useMemo, useState } from "react";
import type { MediaListingEntry } from "@shared/gfn";
import {
  MEDIA_HUB_MIN_TILES,
  MEDIA_SCREENSHOT_PLACEHOLDER_TEMPLATES,
  MEDIA_VIDEO_PLACEHOLDER_TEMPLATES,
} from "./constants";
import type { MediaHubSlot, MediaSubcategory, TopCategory } from "./types";

type UseControllerLibraryMediaArgs = {
  topCategory: TopCategory;
  mediaSubcategory: MediaSubcategory;
  selectedMediaIndex: number;
  /** Increment to refetch listing and thumbnails after delete/regen. */
  mediaListRefreshNonce: number;
};

type UseControllerLibraryMediaResult = {
  mediaLoading: boolean;
  mediaError: string | null;
  mediaVideos: MediaListingEntry[];
  mediaScreenshots: MediaListingEntry[];
  mediaThumbById: Record<string, string>;
  mediaAssetItems: MediaListingEntry[];
  selectedMediaItem: MediaListingEntry | null;
  mediaHubSlots: MediaHubSlot[];
  mediaHubPlaceholderCount: number;
};

export function useControllerLibraryMedia({
  topCategory,
  mediaSubcategory,
  selectedMediaIndex,
  mediaListRefreshNonce,
}: UseControllerLibraryMediaArgs): UseControllerLibraryMediaResult {
  const [mediaLoading, setMediaLoading] = useState(false);
  const [mediaError, setMediaError] = useState<string | null>(null);
  const [mediaVideos, setMediaVideos] = useState<MediaListingEntry[]>([]);
  const [mediaScreenshots, setMediaScreenshots] = useState<MediaListingEntry[]>([]);
  const [mediaThumbById, setMediaThumbById] = useState<Record<string, string>>({});

  useEffect(() => {
    if (topCategory !== "media" || mediaSubcategory === "root") return;
    if (typeof window.openNow?.listMediaByGame !== "function") {
      setMediaVideos([]);
      setMediaScreenshots([]);
      setMediaThumbById({});
      setMediaError("Media API unavailable");
      setMediaLoading(false);
      return;
    }

    let cancelled = false;
    const loadMedia = async () => {
      try {
        setMediaLoading(true);
        setMediaError(null);
        const listing = await window.openNow.listMediaByGame({});
        if (cancelled) return;

        const videos = [...(listing.videos ?? [])].sort((a, b) => b.createdAtMs - a.createdAtMs);
        const screenshots = [...(listing.screenshots ?? [])].sort((a, b) => b.createdAtMs - a.createdAtMs);

        setMediaVideos(videos);
        setMediaScreenshots(screenshots);

        const allItems = [...videos, ...screenshots];
        const thumbEntries = await Promise.all(
          allItems.map(async (item): Promise<[string, string | null]> => {
            if (item.thumbnailDataUrl) return [item.id, item.thumbnailDataUrl];
            if (item.dataUrl) return [item.id, item.dataUrl];
            if (typeof window.openNow?.getMediaThumbnail === "function") {
              const generated = await window.openNow.getMediaThumbnail({ filePath: item.filePath });
              return [item.id, generated];
            }
            return [item.id, null];
          }),
        );

        if (cancelled) return;
        const thumbMap: Record<string, string> = {};
        for (const [id, url] of thumbEntries) {
          if (url) thumbMap[id] = url;
        }
        setMediaThumbById(thumbMap);
      } catch {
        if (cancelled) return;
        setMediaError("Failed to load media");
      } finally {
        if (!cancelled) setMediaLoading(false);
      }
    };

    void loadMedia();
    return () => {
      cancelled = true;
    };
  }, [topCategory, mediaSubcategory, mediaListRefreshNonce]);

  const mediaAssetItems = useMemo(() => {
    if (mediaSubcategory === "Videos") return mediaVideos;
    if (mediaSubcategory === "Screenshots") return mediaScreenshots;
    return [];
  }, [mediaSubcategory, mediaVideos, mediaScreenshots]);

  const selectedMediaItem =
    topCategory === "media" && mediaSubcategory !== "root" ? mediaAssetItems[selectedMediaIndex] ?? null : null;

  const mediaHubSlots = useMemo((): MediaHubSlot[] => {
    if (mediaLoading || mediaError || mediaSubcategory === "root") return [];
    const placeholdersNeeded = Math.max(0, MEDIA_HUB_MIN_TILES - mediaAssetItems.length);
    const filled: MediaHubSlot[] = mediaAssetItems.map((item) => ({ kind: "asset", item }));
    const templates =
      mediaSubcategory === "Videos" ? MEDIA_VIDEO_PLACEHOLDER_TEMPLATES : MEDIA_SCREENSHOT_PLACEHOLDER_TEMPLATES;
    const placeholders: MediaHubSlot[] = Array.from({ length: placeholdersNeeded }, (_, idx) => ({
      kind: "placeholder",
      id: `placeholder-${mediaSubcategory}-${idx}`,
      title: templates[idx % templates.length]?.title ?? "Capture slot available",
      subtitle: templates[idx % templates.length]?.subtitle ?? "Capture gameplay to populate",
    }));
    return [...filled, ...placeholders];
  }, [mediaAssetItems, mediaError, mediaLoading, mediaSubcategory]);

  const mediaHubPlaceholderCount = Math.max(0, mediaHubSlots.length - mediaAssetItems.length);

  return {
    mediaLoading,
    mediaError,
    mediaVideos,
    mediaScreenshots,
    mediaThumbById,
    mediaAssetItems,
    selectedMediaItem,
    mediaHubSlots,
    mediaHubPlaceholderCount,
  };
}
