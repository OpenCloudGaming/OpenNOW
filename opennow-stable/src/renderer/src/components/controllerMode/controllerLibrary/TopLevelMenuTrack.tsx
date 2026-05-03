import type { JSX, RefObject } from "react";
import { PREVIEW_TILE_COUNT, SHELF_IMAGE_PROPS } from "./constants";
import { clampRgbByte } from "./helpers";
import { StreamMenuMicLevelField } from "./StreamMenuMicLevelField";

interface TopLevelMenuTrackProps {
  itemsContainerRef: RefObject<HTMLDivElement | null>;
  topCategory: string;
  gameSubcategory: string;
  menuShelfTranslateX: number;
  displayItems: Array<{ id: string; label: string; value?: string }>;
  topLevelShelfIndex: number;
  gameCategoryPreviewById: Record<string, string[]>;
  currentStreamingImageUrl?: string;
  /** Home “Featured” tile poster (separate from resume snapshot). */
  featuredPreviewImageUrl?: string | null;
  settingsSubcategory: string;
  editingBandwidth: boolean;
  maxBitrateMbpsForTrack: number;
  onSettingChange?: ((key: any, value: any) => void) | undefined;
  themeRgbForTrack: { r: number; g: number; b: number };
  editingThemeChannel: null | "r" | "g" | "b";
  inStreamMenu: boolean;
  streamMenuMicLevel?: number;
  onStreamMenuMicLevelChange?: ((value: number) => void) | undefined;
  editingStreamMicLevel: boolean;
  streamMenuVolume?: number;
  onStreamMenuVolumeChange?: ((value: number) => void) | undefined;
  editingStreamVolume: boolean;
  controllerType: "ps" | "xbox" | "nintendo" | "generic";
  /** Live capture track while streaming; drives the mic test meter on the Mic level row. */
  streamMicTrack?: MediaStreamTrack | null;
}

export function TopLevelMenuTrack({
  itemsContainerRef,
  topCategory,
  gameSubcategory,
  menuShelfTranslateX,
  displayItems,
  topLevelShelfIndex,
  gameCategoryPreviewById,
  currentStreamingImageUrl,
  featuredPreviewImageUrl = null,
  settingsSubcategory,
  editingBandwidth,
  maxBitrateMbpsForTrack,
  onSettingChange,
  themeRgbForTrack,
  editingThemeChannel,
  inStreamMenu,
  streamMenuMicLevel,
  onStreamMenuMicLevelChange,
  editingStreamMicLevel,
  streamMenuVolume,
  onStreamMenuVolumeChange,
  editingStreamVolume,
  controllerType,
  streamMicTrack = null,
}: TopLevelMenuTrackProps): JSX.Element {
  return (
    <div
      ref={itemsContainerRef}
      className={`xmb-ps5-shelf-track xmb-ps5-shelf-track--menu ${topCategory === "all" && gameSubcategory === "root" ? "xmb-ps5-shelf-track--games-root" : ""} ${topCategory === "settings" ? "xmb-ps5-shelf-track--settings" : ""}`}
      role="listbox"
      aria-label={
        topCategory === "current" ? "Home actions" : topCategory === "settings" ? "Controller settings" : topCategory === "all" ? "Game categories" : "Media categories"
      }
      style={{ transform: `translateX(${menuShelfTranslateX}px)` }}
    >
      {displayItems.map((item, idx) => {
        const isActive = idx === topLevelShelfIndex;
        const themeChannelForRow = item.id === "themeR" ? "r" : item.id === "themeG" ? "g" : item.id === "themeB" ? "b" : null;
        const isGameRootTile = topCategory === "all" && gameSubcategory === "root";
        const isCurrentResumeTile = topCategory === "current" && item.id === "resume";
        const isCurrentFeaturedTile = topCategory === "current" && item.id === "featured";
        const isSettingsTile = topCategory === "settings";
        const previewThumbs = isGameRootTile ? (gameCategoryPreviewById[item.id] ?? []) : [];
        return (
          <div
            key={item.id}
            className={`xmb-ps5-menu-tile ${isActive ? "active" : ""} ${isCurrentResumeTile || isCurrentFeaturedTile ? "xmb-ps5-menu-tile--resume" : ""} ${isSettingsTile ? "xmb-ps5-menu-tile--settings" : ""} ${isSettingsTile && settingsSubcategory === "root" ? "xmb-ps5-menu-tile--settings-root" : ""}`}
            role="option"
            aria-selected={isActive}
            {...(isCurrentResumeTile || isCurrentFeaturedTile ? { "aria-label": item.label } : {})}
          >
            {isCurrentResumeTile ? (
              <div className="xmb-ps5-menu-resume-preview" aria-hidden>
                {currentStreamingImageUrl ? (
                  <img src={currentStreamingImageUrl} alt="" className="xmb-ps5-menu-resume-image" decoding="async" />
                ) : (
                  <div className="xmb-ps5-menu-resume-image xmb-ps5-menu-resume-image--placeholder" />
                )}
                <div className="xmb-ps5-menu-resume-overlay">
                  <span className="xmb-ps5-menu-resume-badge">Last played</span>
                </div>
              </div>
            ) : null}
            {isCurrentFeaturedTile ? (
              <div className="xmb-ps5-menu-resume-preview" aria-hidden>
                {featuredPreviewImageUrl ? (
                  <img src={featuredPreviewImageUrl} alt="" className="xmb-ps5-menu-resume-image" decoding="async" />
                ) : (
                  <div className="xmb-ps5-menu-resume-image xmb-ps5-menu-resume-image--placeholder" />
                )}
                <div className="xmb-ps5-menu-resume-overlay">
                  <span className="xmb-ps5-menu-resume-badge">Featured</span>
                </div>
              </div>
            ) : null}
            {isGameRootTile ? (
              <div className="xmb-ps5-menu-thumb-row" aria-hidden>
                {previewThumbs.map((src, i) => (
                  <div key={`${item.id}-${i}`} className="xmb-ps5-menu-thumb">
                    <img src={src} alt="" className="xmb-ps5-menu-thumb-img" {...SHELF_IMAGE_PROPS} />
                  </div>
                ))}
                {Array.from({ length: Math.max(0, PREVIEW_TILE_COUNT - previewThumbs.length) }).map((_, i) => (
                  <div key={`${item.id}-empty-${i}`} className="xmb-ps5-menu-thumb xmb-ps5-menu-thumb--empty" />
                ))}
              </div>
            ) : null}
            {isCurrentResumeTile || isCurrentFeaturedTile ? null : <div className="xmb-ps5-menu-title">{item.label}</div>}
            {item.value ? (
              <div className="xmb-ps5-menu-meta">
                {item.id === "bandwidth" && settingsSubcategory !== "root" ? (
                  <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
                    <input
                      type="range"
                      min={1}
                      max={150}
                      step={1}
                      value={maxBitrateMbpsForTrack}
                      onChange={(e) => onSettingChange && onSettingChange("maxBitrateMbps" as any, Number(e.target.value) as any)}
                      aria-label="Bandwidth Limit (Mbps)"
                      style={editingBandwidth ? { outline: "2px solid rgba(255,255,255,0.2)" } : undefined}
                    />
                    <span className="xmb-game-meta-chip">{`${maxBitrateMbpsForTrack} Mbps`}{editingBandwidth ? " • Editing" : ""}</span>
                  </div>
                ) : themeChannelForRow && settingsSubcategory === "ThemeColor" ? (
                  <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
                    <input
                      type="range"
                      min={0}
                      max={255}
                      step={1}
                      value={themeRgbForTrack[themeChannelForRow]}
                      onChange={(e) => onSettingChange && onSettingChange("controllerThemeColor", { ...themeRgbForTrack, [themeChannelForRow]: clampRgbByte(Number(e.target.value)) })}
                      aria-label={`Theme ${item.label}`}
                      style={editingThemeChannel === themeChannelForRow ? { outline: "2px solid rgba(255,255,255,0.2)" } : undefined}
                    />
                    <span className="xmb-game-meta-chip">
                      {item.value}
                      {editingThemeChannel === themeChannelForRow ? " • Editing" : ""}
                    </span>
                  </div>
                ) : item.id === "streamMicLevel" && inStreamMenu ? (
                  <StreamMenuMicLevelField
                    streamMenuMicLevel={streamMenuMicLevel}
                    onStreamMenuMicLevelChange={onStreamMenuMicLevelChange}
                    editingStreamMicLevel={editingStreamMicLevel}
                    isRowSelected={isActive}
                    micTrack={streamMicTrack}
                    controllerType={controllerType}
                  />
                ) : item.id === "streamVolume" && inStreamMenu ? (
                  <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
                    <input
                      type="range"
                      min={0}
                      max={100}
                      step={1}
                      value={Math.round((streamMenuVolume ?? 1) * 100)}
                      onChange={(e) => onStreamMenuVolumeChange?.(Math.max(0, Math.min(1, Number(e.target.value) / 100)))}
                      aria-label="Stream volume"
                      style={editingStreamVolume ? { outline: "2px solid rgba(255,255,255,0.2)" } : undefined}
                    />
                    <span className="xmb-game-meta-chip">
                      {`${Math.round((streamMenuVolume ?? 1) * 100)}%`}
                      {editingStreamVolume ? " • Editing ←/→" : controllerType === "ps" ? " • □ to adjust" : " • X to adjust"}
                    </span>
                  </div>
                ) : (
                  <span className="xmb-game-meta-chip">{item.value}</span>
                )}
              </div>
            ) : null}
          </div>
        );
      })}
    </div>
  );
}
