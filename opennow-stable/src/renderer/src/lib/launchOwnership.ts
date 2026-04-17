import type { GameInfo, GameVariant } from "@shared/gfn";
import { isEpicStore, isGameInLibrary, isOwnedVariant } from "@shared/gfn";

export interface LaunchOwnershipError {
  title: string;
  description: string;
}

const EPIC_OWNERSHIP_ERROR: LaunchOwnershipError = {
  title: "Epic Games Library Required",
  description:
    "This Epic Games version is not in your GeForce NOW library. Add it to Epic, sync GeForce NOW, or choose another owned store.",
};

export function chooseAccountLinked(game: Pick<GameInfo, "playType" | "isInLibrary" | "variants">, selectedVariant?: GameVariant): boolean {
  if (game.playType === "INSTALL_TO_PLAY") {
    return false;
  }

  if (selectedVariant && isOwnedVariant(selectedVariant)) {
    return true;
  }

  if (game.isInLibrary === true) {
    return true;
  }

  return isGameInLibrary(game);
}

export function getEpicOwnershipLaunchError(selectedVariant?: GameVariant): LaunchOwnershipError | null {
  if (!selectedVariant) {
    return null;
  }

  if (!isEpicStore(selectedVariant.store)) {
    return null;
  }

  if (isOwnedVariant(selectedVariant)) {
    return null;
  }

  return EPIC_OWNERSHIP_ERROR;
}
