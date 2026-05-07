import type { JSX } from "react";
import type { ControllerThemeStyle, GameInfo } from "@shared/gfn";
import { House, Settings as SettingsIcon, Library, Clapperboard } from "lucide-react";
import {
  CONTROLLER_THEME_STYLE_ORDER,
  LIBRARY_SORT_STORAGE_KEY,
  SHELF_CONTENT_WINDOW_RADIUS,
  SHELF_IMAGE_WINDOW_RADIUS,
} from "./constants";
import type { LibrarySortId, SpotlightEntry } from "./types";

export function spotlightEntryHasGame(entry: SpotlightEntry | undefined): entry is { kind: "recent"; game: GameInfo } {
  return entry?.kind === "recent" && entry.game != null;
}

export function readLibrarySortId(): LibrarySortId {
  try {
    const v = typeof sessionStorage !== "undefined" ? sessionStorage.getItem(LIBRARY_SORT_STORAGE_KEY) : null;
    if (v === "recent" || v === "az" || v === "za" || v === "favoritesFirst") return v;
  } catch {
  }
  return "recent";
}

export function isWithinImageWindow(index: number, activeIndex: number, radius: number = SHELF_IMAGE_WINDOW_RADIUS): boolean {
  return Math.abs(index - activeIndex) <= radius;
}

export function isWithinContentWindow(index: number, activeIndex: number, radius: number = SHELF_CONTENT_WINDOW_RADIUS): boolean {
  return Math.abs(index - activeIndex) <= radius;
}

export function computeShelfTranslateXToCenter(track: HTMLElement | null, activeIndex: number): number {
  if (!track) return 0;
  const viewport = track.parentElement;
  if (!(viewport instanceof HTMLElement)) return 0;
  const children = Array.from(track.children) as HTMLElement[];
  if (children.length === 0 || activeIndex < 0 || activeIndex >= children.length) return 0;
  const activeEl = children[activeIndex];
  const centerInTrack = activeEl.offsetLeft + activeEl.offsetWidth / 2;
  const halfVp = viewport.clientWidth / 2;
  return halfVp - track.offsetLeft - centerInTrack;
}

export function computeShelfTranslateXClamped(track: HTMLElement | null, activeIndex: number): number {
  if (!track) return 0;
  const viewport = track.parentElement;
  if (!(viewport instanceof HTMLElement)) return 0;
  const desired = computeShelfTranslateXToCenter(track, activeIndex);
  const maxTranslate = -track.offsetLeft;
  const minTranslate = viewport.clientWidth - (track.offsetLeft + track.scrollWidth);
  if (minTranslate > maxTranslate) return maxTranslate;
  return Math.max(minTranslate, Math.min(maxTranslate, desired));
}

export function sanitizeControllerThemeStyle(raw: string | undefined): ControllerThemeStyle {
  return CONTROLLER_THEME_STYLE_ORDER.includes(raw as ControllerThemeStyle) ? (raw as ControllerThemeStyle) : "aurora";
}

export function clampRgbByte(n: number): number {
  return Math.max(0, Math.min(255, Math.round(Number.isFinite(n) ? n : 0)));
}

export function sanitizeGenreName(raw: string): string {
  return raw
    .replace(/_/g, " ")
    .replace(/\b\w/g, (ch) => ch.toUpperCase());
}

export function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  return target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.tagName === "SELECT";
}

export function getCategoryLabel(categoryId: string): { label: string } {
  if (categoryId === "current") return { label: "Home" };
  if (categoryId === "all") return { label: "Games" };
  if (categoryId === "settings") return { label: "Settings" };
  if (categoryId === "media") return { label: "Media" };
  return { label: "Games" };
}

export function getCategoryIcon(categoryId: string): JSX.Element {
  if (categoryId === "current") return <House size={28} />;
  if (categoryId === "settings") return <SettingsIcon size={28} />;
  if (categoryId === "media") return <Clapperboard size={28} />;
  return <Library size={28} />;
}
