import type { ControllerThemeStyle } from "@shared/gfn";
import type { LibrarySortId, PlaceholderTemplate } from "./types";

export const LIBRARY_SORT_STORAGE_KEY = "opennow:controllerLibrarySort.v1";

export const LIBRARY_SORT_LABEL: Record<LibrarySortId, string> = {
  recent: "Recent",
  az: "A–Z",
  za: "Z–A",
  favoritesFirst: "Favorites first",
};

export const CATEGORY_STEP_PX = 160;
export const CATEGORY_ACTIVE_HALF_WIDTH_PX = 60;
export const GAME_ACTIVE_CENTER_OFFSET_X_PX = 320;
export const PREVIEW_TILE_COUNT = 6;
export const SPOTLIGHT_RECENT_COUNT = 5;
export const MEDIA_HUB_MIN_TILES = 8;

export const MEDIA_VIDEO_PLACEHOLDER_TEMPLATES: ReadonlyArray<PlaceholderTemplate> = [
  { title: "Recent Clip slot", subtitle: "Record gameplay moments" },
  { title: "Highlight Reel slot", subtitle: "Mark your best plays" },
  { title: "Shared Clip slot", subtitle: "Publish to your social feed" },
  { title: "Squad Clip slot", subtitle: "Capture co-op highlights" },
];

export const MEDIA_SCREENSHOT_PLACEHOLDER_TEMPLATES: ReadonlyArray<PlaceholderTemplate> = [
  { title: "Recent Screenshot slot", subtitle: "Capture gameplay stills" },
  { title: "Wallpaper slot", subtitle: "Save scenic moments" },
  { title: "Trophy Moment slot", subtitle: "Archive major unlocks" },
  { title: "Shared Screenshot slot", subtitle: "Publish to your social feed" },
];

export const SHELF_IMAGE_PROPS = { decoding: "async" as const, loading: "lazy" as const };
export const SHELF_IMAGE_WINDOW_RADIUS = 8;
export const SHELF_CONTENT_WINDOW_RADIUS = 14;

export const CONTROLLER_THEME_STYLE_ORDER: readonly ControllerThemeStyle[] = ["aurora", "nebula", "grid", "minimal", "pulse"];

export const CONTROLLER_THEME_STYLE_LABEL: Record<ControllerThemeStyle, string> = {
  aurora: "Aurora",
  nebula: "Nebula",
  grid: "Grid",
  minimal: "Minimal",
  pulse: "Pulse",
};
