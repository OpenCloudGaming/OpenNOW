import type { GameInfo } from "@shared/gfn";
import { getStoreDisplayName } from "./gameCardStores";
import { getResolvedSelectedVariantId } from "./gameCardVariants";
import { storeVariantIsOwned } from "./controllerCatalogUi";

export interface ConsoleStoreChoice {
  variantId: string;
  /** Raw store key, for icon lookup. */
  store: string;
  /** Human-readable store name. */
  label: string;
  isOwned: boolean;
  isActive: boolean;
}

/**
 * One entry per store a game can be launched from, for the console store
 * picker. Variants without a store are dropped, and duplicates of the same
 * store collapse to the owned one so a title never lists "Steam" twice.
 */
export function getConsoleStoreChoices(game: GameInfo, selectedVariantId?: string): ConsoleStoreChoice[] {
  const activeVariantId = getResolvedSelectedVariantId(game, selectedVariantId);
  const byStore = new Map<string, ConsoleStoreChoice>();

  for (const variant of game.variants) {
    const store = variant.store?.trim();
    if (!store) continue;

    const choice: ConsoleStoreChoice = {
      variantId: variant.id,
      store,
      label: getStoreDisplayName(store),
      isOwned: storeVariantIsOwned(variant),
      isActive: variant.id === activeVariantId,
    };

    const existing = byStore.get(choice.label);
    // Prefer whichever entry the user can actually use: the active one, then an
    // owned one, then the first seen.
    if (!existing || (!existing.isActive && (choice.isActive || (choice.isOwned && !existing.isOwned)))) {
      byStore.set(choice.label, choice);
    }
  }

  return [...byStore.values()].sort((left, right) => {
    if (left.isOwned !== right.isOwned) return left.isOwned ? -1 : 1;
    return left.label.localeCompare(right.label);
  });
}
