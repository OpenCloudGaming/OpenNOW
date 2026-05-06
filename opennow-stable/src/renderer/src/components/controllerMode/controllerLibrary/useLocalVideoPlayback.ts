import { useCallback, useState } from "react";
import type { MediaListingEntry } from "@shared/gfn";
import type { SoundKind } from "./types";

export type LocalVideoPlaybackState = { src: string; filePath: string } | null;

export function useLocalVideoPlayback(playUiSound: (kind: SoundKind) => void): {
  playback: LocalVideoPlaybackState;
  localVideoPlayerOpen: boolean;
  openFromEntry: (entry: MediaListingEntry) => Promise<void>;
  close: () => void;
} {
  const [playback, setPlayback] = useState<LocalVideoPlaybackState>(null);

  const close = useCallback(() => {
    setPlayback(null);
  }, []);

  const openFromEntry = useCallback(
    async (entry: MediaListingEntry) => {
      if (typeof window.openNow?.getMediaPlaybackUrl !== "function") {
        if (typeof window.openNow?.showMediaInFolder === "function") {
          void window.openNow.showMediaInFolder({ filePath: entry.filePath });
        }
        playUiSound("confirm");
        return;
      }
      const url = await window.openNow.getMediaPlaybackUrl({ filePath: entry.filePath });
      if (!url) {
        if (typeof window.openNow?.showMediaInFolder === "function") {
          void window.openNow.showMediaInFolder({ filePath: entry.filePath });
        }
        playUiSound("confirm");
        return;
      }
      setPlayback({ src: url, filePath: entry.filePath });
      playUiSound("confirm");
    },
    [playUiSound],
  );

  return {
    playback,
    localVideoPlayerOpen: playback !== null,
    openFromEntry,
    close,
  };
}
