import type { GameInfo, GameVariant } from "@shared/gfn";
import { isOwnedVariant, normalizeGameStore } from "@shared/gfn";

export interface StoreOption {
  storeKey: string;
  store: string;
  variantId: string;
  isOwned: boolean;
  isActive: boolean;
}

function getResolvedSelectedVariantId(game: GameInfo, selectedVariantId?: string): string | undefined {
  if (selectedVariantId && game.variants.some((variant) => variant.id === selectedVariantId)) {
    return selectedVariantId;
  }

  return game.variants[game.selectedVariantIndex]?.id ?? game.variants[0]?.id;
}

function getVariantForStore(variants: GameVariant[], activeVariantId?: string): GameVariant | undefined {
  if (activeVariantId) {
    return variants.find((variant) => variant.id === activeVariantId) ?? variants[0];
  }

  return variants.find((variant) => isOwnedVariant(variant)) ?? variants[0];
}

export function getStoreOptions(game: GameInfo, selectedVariantId?: string): StoreOption[] {
  const resolvedSelectedVariantId = getResolvedSelectedVariantId(game, selectedVariantId);
  const variantsByStore = new Map<string, GameVariant[]>();

  for (const variant of game.variants) {
    const storeKey = normalizeGameStore(variant.store);
    if (storeKey === "UNKNOWN" || storeKey === "NONE") {
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
