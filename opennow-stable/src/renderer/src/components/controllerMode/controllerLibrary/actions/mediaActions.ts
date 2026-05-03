import type { MediaActivateContext, MediaCancelContext } from "./contracts";

export function handleMediaActivateAction(context: MediaActivateContext): boolean {
  const {
    mediaSubcategory,
    displayItems,
    selectedMediaIndex,
    setLastRootMediaIndex,
    setMediaSubcategory,
    setSelectedMediaIndex,
    mediaAssetItems,
    playUiSound,
  } = context;

  const item = displayItems[selectedMediaIndex];
  if (mediaSubcategory === "root" && item && (item.id === "videos" || item.id === "screenshots")) {
    setLastRootMediaIndex(selectedMediaIndex);
    setMediaSubcategory(item.id === "videos" ? "Videos" : "Screenshots");
    setSelectedMediaIndex(0);
    playUiSound("confirm");
    return true;
  }

  if (mediaSubcategory !== "root") {
    const selectedMedia = mediaAssetItems[selectedMediaIndex];
    if (selectedMedia && typeof window.openNow?.showMediaInFolder === "function") {
      void window.openNow.showMediaInFolder({ filePath: selectedMedia.filePath });
      playUiSound("confirm");
      return true;
    }
  }

  playUiSound("confirm");
  return true;
}

export function handleMediaCancelAction(context: MediaCancelContext): boolean {
  const { setMediaSubcategory, setSelectedMediaIndex, lastRootMediaIndex, playUiSound } = context;
  setMediaSubcategory("root");
  setSelectedMediaIndex(lastRootMediaIndex);
  playUiSound("move");
  return true;
}
