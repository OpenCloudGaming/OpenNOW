import type { GameInfo, GameVariant } from "@shared/gfn";
import { isOwnedVariant, normalizeGameStore } from "@shared/gfn";
import { getResolvedSelectedVariantId } from "./gameCardVariants";

const STORE_DISPLAY_NAME: Record<string, string> = {
  STEAM: "Steam",
  EPIC_GAMES_STORE: "Epic",
  UPLAY: "Ubisoft",
  EA_APP: "EA",
  GOG: "GOG",
  XBOX: "Xbox",
  BATTLE_NET: "Battle.net",
};

/** Normalize an appStore value to the uppercase key used by the icon/name maps. */
export function normalizeStoreKey(raw: string): string {
  return normalizeGameStore(raw);
}

function formatStoreFallbackName(storeKey: string): string {
  return storeKey
    .toLowerCase()
    .replace(/_/g, " ")
    .replace(/\b\w/g, (match) => match.toUpperCase());
}

export function getStoreDisplayName(store: string): string {
  const key = normalizeStoreKey(store);
  return STORE_DISPLAY_NAME[key] ?? formatStoreFallbackName(key);
}

export interface StoreOption {
  storeKey: string;
  store: string;
  variantId: string;
  isOwned: boolean;
  isActive: boolean;
}

function normalizeStoreOptionKey(store: string): string | null {
  const trimmed = store.trim();
  if (!trimmed) {
    return null;
  }

  const key = normalizeGameStore(trimmed);
  return key === "NONE" ? null : key;
}

function getVariantForStore(variants: GameVariant[], activeVariantId?: string): GameVariant | undefined {
  const ownedVariant = variants.find((variant) => isOwnedVariant(variant));

  if (activeVariantId) {
    const activeVariant = variants.find((variant) => variant.id === activeVariantId);
    if (activeVariant && (isOwnedVariant(activeVariant) || !ownedVariant)) {
      return activeVariant;
    }
    return ownedVariant ?? activeVariant ?? variants[0];
  }

  return ownedVariant ?? variants[0];
}

export function getStoreOptions(game: GameInfo, selectedVariantId?: string): StoreOption[] {
  const resolvedSelectedVariantId = getResolvedSelectedVariantId(game, selectedVariantId);
  const variantsByStore = new Map<string, GameVariant[]>();

  for (const variant of game.variants) {
    const storeKey = normalizeStoreOptionKey(variant.store);
    if (!storeKey) {
      continue;
    }

    const existing = variantsByStore.get(storeKey);
    if (existing) {
      existing.push(variant);
    } else {
      variantsByStore.set(storeKey, [variant]);
    }
  }

  return [...variantsByStore.entries()].map(([storeKey, variants]) => {
    const activeVariantId = variants.find((variant) => variant.id === resolvedSelectedVariantId)?.id;
    const preferredVariant = getVariantForStore(variants, activeVariantId);

    return {
      storeKey,
      store: preferredVariant?.store ?? variants[0]?.store ?? storeKey,
      variantId: preferredVariant?.id ?? variants[0]?.id ?? storeKey,
      isOwned: variants.some((variant) => isOwnedVariant(variant)),
      isActive: Boolean(activeVariantId),
    };
  });
}
