import type { GameInfo } from "@shared/gfn";
import { appendUnique } from "./controllerCatalogUi";

const MAX_SCREENSHOTS = 8;

/**
 * Still media for a game's detail sheet.
 *
 * NVIDIA's catalog exposes no trailer or video for a title — only SCREENSHOTS
 * plus the marketing art keys — so this is the full extent of the media a
 * detail sheet can show.
 */
export function getGameScreenshots(game: GameInfo): string[] {
  const shots: string[] = [];
  for (const candidate of game.imageUrlsByType?.SCREENSHOTS ?? []) appendUnique(shots, candidate);
  for (const candidate of game.screenshotUrls ?? []) appendUnique(shots, candidate);
  appendUnique(shots, game.screenshotUrl);

  // Marketing art is a reasonable stand-in when a title ships no screenshots.
  if (shots.length === 0) {
    for (const key of ["MARQUEE_HERO_IMAGE", "HERO_IMAGE", "FEATURE_IMAGE", "KEY_ART"]) {
      for (const candidate of game.imageUrlsByType?.[key] ?? []) appendUnique(shots, candidate);
    }
  }

  return shots.slice(0, MAX_SCREENSHOTS);
}
