import { isOwnedLibraryStatus } from "@shared/gfn";
import type { GameInfo, GameVariant } from "@shared/gfn";
import { getStoreDisplayName } from "./gameCardStores";
import { getActiveGameVariant } from "./gameCardVariants";
import { isNumericId } from "./gameCatalog";

const CONTROLLER_HERO_BACKGROUND_KEYS = [
  "MARQUEE_HERO_IMAGE",
  "FEATURE_IMAGE",
  "HERO_IMAGE",
  "TV_BANNER",
  "KEY_ART",
  "KEY_IMAGE",
] as const;

/** The only NVIDIA image key that is reliably portrait. */
const CONSOLE_POSTER_PORTRAIT_KEYS = ["GAME_BOX_ART"] as const;

/**
 * Landscape keys used as a last resort for poster cards. Cards rendered from
 * these are composed rather than cropped — see `data-poster-fallback` handling
 * in ConsolePosterCard.
 */
const CONSOLE_POSTER_LANDSCAPE_KEYS = ["TV_BANNER", "KEY_ART", "KEY_IMAGE", "HERO_IMAGE"] as const;

const CONSOLE_LOGO_KEYS = ["GAME_LOGO", "LOGO", "TITLE_LOGO"] as const;

export function appendUnique(values: string[], candidate: string | undefined): void {
  if (!candidate || values.includes(candidate)) return;
  values.push(candidate);
}

export function appendImageType(values: string[], game: GameInfo, type: string): void {
  for (const candidate of game.imageUrlsByType?.[type] ?? []) {
    appendUnique(values, candidate);
  }
}

export function gameMatchesActiveSession(game: GameInfo, activeSessionAppIds: number[]): boolean {
  if (activeSessionAppIds.length === 0) return false;
  const appIds = new Set(activeSessionAppIds.map(String));
  if (game.launchAppId && appIds.has(game.launchAppId)) return true;
  if (appIds.has(game.id)) return true;
  return game.variants.some((variant) => appIds.has(variant.id));
}

export function getControllerHeroBackgroundCandidates(game: GameInfo): string[] {
  const candidates: string[] = [];
  for (const type of CONTROLLER_HERO_BACKGROUND_KEYS) {
    appendImageType(candidates, game, type);
  }
  appendUnique(candidates, game.heroImageUrl);
  appendUnique(candidates, game.imageUrl);
  for (const candidate of game.screenshotUrls ?? []) appendUnique(candidates, candidate);
  appendUnique(candidates, game.screenshotUrl);
  return candidates;
}

export function getControllerHeroLogoUrl(game: GameInfo): string | undefined {
  return game.imageUrlsByType?.GAME_LOGO?.find(Boolean);
}

export function getGameLogoUrl(game: GameInfo): string | undefined {
  for (const key of CONSOLE_LOGO_KEYS) {
    const candidate = game.imageUrlsByType?.[key]?.find(Boolean);
    if (candidate) return candidate;
  }
  return undefined;
}

function getSteamAppId(game: GameInfo): string | undefined {
  const steamVariant = game.variants.find(
    (variant) => isNumericId(variant.id) && variant.store.toUpperCase().includes("STEAM"),
  );
  return steamVariant?.id ?? (isNumericId(game.launchAppId) ? game.launchAppId : undefined);
}

export function getSteamHeaderUrl(game: GameInfo): string | undefined {
  const appId = getSteamAppId(game);
  return appId ? `https://cdn.cloudflare.steamstatic.com/steam/apps/${appId}/header.jpg` : undefined;
}

/** Steam's native 2:3 library art — the best portrait source outside GAME_BOX_ART. */
export function getSteamLibraryPosterUrl(game: GameInfo): string | undefined {
  const appId = getSteamAppId(game);
  return appId ? `https://cdn.cloudflare.steamstatic.com/steam/apps/${appId}/library_600x900.jpg` : undefined;
}

/**
 * Ordered poster sources for a 2:3 console card. Genuinely portrait art comes
 * first; landscape keys trail as a fallback the card composes rather than crops.
 */
export function getConsolePosterCandidates(game: GameInfo): string[] {
  const candidates: string[] = [];
  for (const key of CONSOLE_POSTER_PORTRAIT_KEYS) appendImageType(candidates, game, key);
  appendUnique(candidates, getSteamLibraryPosterUrl(game));
  for (const key of CONSOLE_POSTER_LANDSCAPE_KEYS) appendImageType(candidates, game, key);
  appendUnique(candidates, game.imageUrl);
  appendUnique(candidates, game.heroImageUrl);
  return candidates;
}

/** How many leading entries of `getConsolePosterCandidates` are true 2:3 art. */
export function countConsolePortraitPosterCandidates(game: GameInfo): number {
  const portrait: string[] = [];
  for (const key of CONSOLE_POSTER_PORTRAIT_KEYS) appendImageType(portrait, game, key);
  appendUnique(portrait, getSteamLibraryPosterUrl(game));
  return portrait.length;
}

/**
 * Decodes an image off-screen so a hero swap never flashes a half-loaded frame.
 * Resolves false when the source is unusable.
 */
export function preloadControllerHeroImage(imageUrl: string): Promise<boolean> {
  return new Promise((resolve) => {
    const image = new Image();
    image.decoding = "async";
    image.onload = () => {
      void image.decode()
        .catch(() => undefined)
        .then(() => resolve(image.naturalWidth > 0));
    };
    image.onerror = () => resolve(false);
    image.src = imageUrl;
  });
}

export function getSelectedVariant(game: GameInfo, selectedVariantId?: string): GameVariant | undefined {
  return getActiveGameVariant(game, selectedVariantId);
}

export function storeVariantIsOwned(variant: GameVariant | undefined): boolean {
  return Boolean(variant?.inLibrary || variant?.librarySelected || isOwnedLibraryStatus(variant?.libraryStatus));
}

export function gameNeedsPurchase(game: GameInfo, selectedVariantId?: string): boolean {
  return !storeVariantIsOwned(getSelectedVariant(game, selectedVariantId));
}

export function getNextVariantId(game: GameInfo, selectedVariantId?: string): string | undefined {
  if (game.variants.length === 0) return undefined;
  const activeIndex = Math.max(0, game.variants.findIndex((variant) => variant.id === selectedVariantId));
  return game.variants[(activeIndex + 1) % game.variants.length]?.id;
}

export function getVariantDisplayName(variant: GameVariant | undefined, fallback: string): string {
  return variant?.store ? getStoreDisplayName(variant.store) : fallback;
}

export function getPrimaryGenre(game: GameInfo): string | undefined {
  return game.genres?.[0] ?? game.playType ?? undefined;
}

export function getPrimaryStoreName(game: GameInfo, selectedVariantId?: string): string {
  const store = getSelectedVariant(game, selectedVariantId)?.store ?? game.availableStores?.[0] ?? "Cloud";
  const upper = store.toUpperCase();
  if (upper.includes("STEAM")) return "Steam";
  if (upper.includes("BATTLE")) return "Battle.net";
  if (upper.includes("UBISOFT") || upper.includes("UPLAY")) return "Ubisoft";
  if (upper.includes("XBOX")) return "Xbox";
  if (upper.includes("EPIC")) return "Epic";
  if (upper.includes("EA")) return "EA";
  return getStoreDisplayName(store);
}

export function getControllerFeaturedGames(featuredGames: GameInfo[], fallbackGames: GameInfo[]): GameInfo[] {
  const source = featuredGames.length > 0 ? featuredGames : fallbackGames;
  return source.slice(0, 6);
}

export function getGameStoreSummary(game: GameInfo, fallback: string): string {
  const stores = [...new Set((game.availableStores?.length ? game.availableStores : game.variants.map((variant) => variant.store)).filter(Boolean))];
  if (stores.length === 0) return fallback;
  const visible = stores.slice(0, 3).join(", ");
  return stores.length > 3 ? `${visible} +${stores.length - 3}` : visible;
}

export function getSelectedVariantStoreLabel(game: GameInfo, selectedVariantId: string | undefined, fallback: string): string {
  const selectedVariant = game.variants.find((variant) => variant.id === selectedVariantId)
    ?? game.variants[game.selectedVariantIndex]
    ?? game.variants[0];
  return selectedVariant?.store ? getStoreDisplayName(selectedVariant.store) : fallback;
}

export function getPlayerSummary(game: GameInfo): string | null {
  const parts: string[] = [];
  if (game.maxLocalPlayers && game.maxLocalPlayers > 0) parts.push(`Local ${game.maxLocalPlayers}`);
  if (game.maxOnlinePlayers && game.maxOnlinePlayers > 0) parts.push(`Online ${game.maxOnlinePlayers}`);
  return parts.length > 0 ? parts.join(" / ") : null;
}
