const STORAGE_KEY = "opennow:controllerOverlayNav.v1";

/** Mirrors subset of ControllerLibraryPage navigation state */
export interface ControllerOverlayNavSnapshot {
  categoryIndex: number;
  gameSubcategory: string;
  mediaSubcategory: string;
  settingsSubcategory: string;
  gamesRootPlane: "spotlight" | "categories";
  spotlightIndex: number;
  selectedGameSubcategoryIndex: number;
  selectedSettingIndex: number;
  selectedMediaIndex: number;
  ps5Row: "top" | "main" | "detail";
}

export function readControllerOverlayNav(): ControllerOverlayNavSnapshot | null {
  try {
    const raw = typeof sessionStorage !== "undefined" ? sessionStorage.getItem(STORAGE_KEY) : null;
    if (!raw) return null;
    const parsed = JSON.parse(raw) as ControllerOverlayNavSnapshot;
    if (typeof parsed.categoryIndex !== "number") return null;
    return parsed;
  } catch {
    return null;
  }
}

export function writeControllerOverlayNav(snapshot: ControllerOverlayNavSnapshot): void {
  try {
    if (typeof sessionStorage === "undefined") return;
    sessionStorage.setItem(STORAGE_KEY, JSON.stringify(snapshot));
  } catch {
    // ignore quota / private mode
  }
}

export function clearControllerOverlayNav(): void {
  try {
    if (typeof sessionStorage === "undefined") return;
    sessionStorage.removeItem(STORAGE_KEY);
  } catch {
    // ignore
  }
}
