import type { JSX, RefObject } from "react";
import type { MediaListingEntry } from "@shared/gfn";
import { SHELF_IMAGE_PROPS } from "./constants";
import { isWithinContentWindow, isWithinImageWindow } from "./helpers";
import type { MediaHubSlot, MediaSubcategory } from "./types";

interface MediaHubSectionProps {
  focusMotionKey: string;
  selectedMediaItem: MediaListingEntry | null;
  mediaSubcategory: MediaSubcategory;
  mediaAssetItems: MediaListingEntry[];
  mediaHubPlaceholderCount: number;
  itemsContainerRef: RefObject<HTMLDivElement | null>;
  listTranslateX: number;
  mediaLoading: boolean;
  mediaError: string | null;
  mediaHubSlots: MediaHubSlot[];
  selectedMediaIndex: number;
  mediaThumbById: Record<string, string>;
}

export function MediaHubSection({
  focusMotionKey,
  selectedMediaItem,
  mediaSubcategory,
  mediaAssetItems,
  mediaHubPlaceholderCount,
  itemsContainerRef,
  listTranslateX,
  mediaLoading,
  mediaError,
  mediaHubSlots,
  selectedMediaIndex,
  mediaThumbById,
}: MediaHubSectionProps): JSX.Element {
  return (
    <div className="xmb-ps5-stack xmb-ps5-media-hub">
      <div className="xmb-ps5-focus-meta" aria-live="polite" key={focusMotionKey}>
        <h2 className="xmb-ps5-focus-title">
          {selectedMediaItem?.gameTitle || selectedMediaItem?.fileName || mediaSubcategory}
        </h2>
        <p className="xmb-ps5-media-hub-subtitle">
          {mediaAssetItems.length} ready · {mediaHubPlaceholderCount} reserved slots
        </p>
        <div className="xmb-ps5-actions">
          <span className="xmb-ps5-action xmb-ps5-action--primary">Open Folder</span>
          <span className="xmb-ps5-action">Options</span>
        </div>
      </div>
      <div className="xmb-ps5-shelf-viewport">
        <div
          ref={itemsContainerRef}
          className="xmb-ps5-shelf-track xmb-ps5-shelf-track--media"
          role="listbox"
          aria-label={`${mediaSubcategory} media`}
          style={{ transform: `translateX(${listTranslateX}px)` }}
        >
          {mediaLoading && Array.from({ length: 8 }).map((_, idx) => (
            <div key={`media-loading-${idx}`} className={`xmb-ps5-media-tile ${idx === 0 ? "active" : ""}`} role="option" aria-selected={idx === 0}>
              <div className="xmb-ps5-media-frame xmb-ps5-media-frame--placeholder" />
              <div className="xmb-ps5-media-caption">Loading {mediaSubcategory}...</div>
            </div>
          ))}

          {!mediaLoading && mediaError && (
            <div className="xmb-ps5-media-tile active" role="option" aria-selected>
              <div className="xmb-ps5-media-frame xmb-ps5-media-frame--placeholder" />
              <div className="xmb-ps5-media-caption">{mediaError}</div>
            </div>
          )}

          {!mediaLoading && !mediaError && mediaAssetItems.length === 0 && Array.from({ length: 6 }).map((_, idx) => (
            <div key={`media-empty-${idx}`} className={`xmb-ps5-media-tile ${idx === 0 ? "active" : ""}`} role="option" aria-selected={idx === 0}>
              <div className="xmb-ps5-media-frame xmb-ps5-media-frame--placeholder" />
              <div className="xmb-ps5-media-caption">
                {idx === 0 ? `No ${mediaSubcategory.toLowerCase()} found` : "Capture more to fill this shelf"}
              </div>
            </div>
          ))}

          {!mediaLoading && !mediaError && mediaHubSlots.map((slot, idx) => {
            const isAsset = slot.kind === "asset";
            const isActive = isAsset && idx === selectedMediaIndex;
            if (!isAsset) {
              return (
                <div key={slot.id} className="xmb-ps5-media-tile xmb-ps5-media-tile--placeholder-slot" role="option" aria-selected={false}>
                  <div className="xmb-ps5-media-frame xmb-ps5-media-frame--placeholder">
                    <div className="xmb-ps5-media-slot-overlay">
                      <span className="xmb-ps5-media-slot-badge">Empty Slot</span>
                    </div>
                  </div>
                  <div className="xmb-ps5-media-caption">{slot.title}</div>
                  <div className="xmb-ps5-media-meta">
                    <span className="xmb-game-meta-chip">{slot.subtitle}</span>
                  </div>
                </div>
              );
            }

            const item = slot.item;
            const shouldRenderContent = isWithinContentWindow(idx, selectedMediaIndex);
            const shouldRenderImage = isWithinImageWindow(idx, selectedMediaIndex);
            const eagerLoadImage = Math.abs(idx - selectedMediaIndex) <= 1;
            const thumb = mediaThumbById[item.id];
            const dateLabel = new Date(item.createdAtMs).toLocaleDateString();
            const durationMs = item.durationMs ?? 0;
            const hasDuration = durationMs > 0;
            const durationLabel = hasDuration ? `${Math.max(1, Math.round(durationMs / 1000))}s` : "Screenshot";

            return (
              <div key={item.id} className={`xmb-ps5-media-tile ${isActive ? "active" : ""}`} role="option" aria-selected={isActive}>
                {shouldRenderContent ? (
                  <>
                    <div className="xmb-ps5-media-frame">
                      {thumb && shouldRenderImage ? (
                        <img
                          src={thumb}
                          alt=""
                          className="xmb-ps5-media-image"
                          {...SHELF_IMAGE_PROPS}
                          loading={eagerLoadImage ? "eager" : "lazy"}
                        />
                      ) : <div className="xmb-ps5-media-image xmb-ps5-media-image--placeholder" />}
                    </div>
                    <div className="xmb-ps5-media-caption">{item.gameTitle || item.fileName}</div>
                    <div className="xmb-ps5-media-meta">
                      <span className="xmb-game-meta-chip">{durationLabel}</span>
                      <span className="xmb-game-meta-chip">{dateLabel}</span>
                    </div>
                  </>
                ) : (
                  <div className="xmb-ps5-media-frame xmb-ps5-media-frame--virtualized" aria-hidden />
                )}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
