import { useEffect, useMemo, useState } from "react";
import type { CSSProperties, JSX } from "react";
import type { GameInfo } from "@shared/gfn";
import {
  countConsolePortraitPosterCandidates,
  getConsolePosterCandidates,
  getGameLogoUrl,
  getPrimaryStoreName,
  getSelectedVariant,
} from "../../lib/controllerCatalogUi";
import { withImageWidth } from "../../lib/consoleImageSizing";
import { useConsoleImageWidths } from "../../hooks/useConsoleImageWidths";
import { getStoreIconComponent } from "../GameCard";

export interface ConsolePosterCardProps {
  game: GameInfo;
  index: number;
  isFocused: boolean;
  selectedVariantId?: string;
  /** Rendered top-left, e.g. Owned / Not owned. */
  pill?: { label: string; tone?: "neutral" | "owned" | "session" };
  onSelect: () => void;
  onActivate: () => void;
}

/**
 * The single card used by every console shelf.
 *
 * Poster art is 2:3. Portrait sources (box art, then Steam's 600x900) are tried
 * first; `data-poster-fallback="landscape"` marks the cards that had to fall
 * back to a 16:9 banner so CSS can bias the crop and deepen the caption scrim.
 */
export function ConsolePosterCard({
  game,
  index,
  isFocused,
  selectedVariantId,
  pill,
  onSelect,
  onActivate,
}: ConsolePosterCardProps): JSX.Element {
  const imageWidths = useConsoleImageWidths();
  const candidates = useMemo(() => getConsolePosterCandidates(game), [game]);
  const portraitCount = useMemo(() => countConsolePortraitPosterCandidates(game), [game]);
  const [candidateIndex, setCandidateIndex] = useState(0);

  // A new game in the same slot must restart the candidate walk.
  useEffect(() => setCandidateIndex(0), [game.id]);

  // Ask the CDN for a card-sized image instead of the catalog's 1200px default.
  const artUrl = withImageWidth(candidates[candidateIndex], imageWidths.card);
  const isLandscapeFallback = artUrl !== undefined && candidateIndex >= portraitCount;
  const logoUrl = withImageWidth(getGameLogoUrl(game), imageWidths.card);
  const storeName = getPrimaryStoreName(game, selectedVariantId);
  const StoreIcon = getStoreIconComponent(getSelectedVariant(game, selectedVariantId)?.store ?? storeName);

  return (
    <button
      type="button"
      className={`console-card${isFocused ? " is-focused" : ""}`}
      style={{ "--i": index } as CSSProperties}
      data-console-column={index}
      data-poster-fallback={isLandscapeFallback ? "landscape" : undefined}
      aria-label={game.title}
      onClick={onSelect}
      onDoubleClick={onActivate}
    >
      {artUrl ? (
        <img
          className="console-card-art"
          src={artUrl}
          alt=""
          loading="lazy"
          decoding="async"
          onError={() => setCandidateIndex((current) => (current + 1 < candidates.length ? current + 1 : current))}
        />
      ) : (
        <span className="console-card-placeholder">{game.title.slice(0, 1)}</span>
      )}

      <span className="console-card-gradient" />

      {pill && (
        <span className={`console-card-pill${pill.tone && pill.tone !== "neutral" ? ` console-card-pill--${pill.tone}` : ""}`}>
          {pill.label}
        </span>
      )}

      <span className="console-card-badge" title={storeName}>
        <StoreIcon />
      </span>

      <span className="console-card-caption">
        {logoUrl
          ? <img className="console-card-logo" src={logoUrl} alt={game.title} loading="lazy" decoding="async" />
          : <span className="console-card-title">{game.title}</span>}
      </span>

      <span className="console-card-glow" />
    </button>
  );
}
