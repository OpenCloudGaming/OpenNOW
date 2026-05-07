import type { JSX } from "react";
import type { GameInfo } from "@shared/gfn";
import { Clock, Calendar, Repeat2 } from "lucide-react";
import { getStoreDisplayName } from "../GameCard";
import { formatPlaytime, formatLastPlayed, type PlaytimeStore } from "../../utils/usePlaytime";

export type GameHubTile = {
  id: string;
  title: string;
  subtitle: string;
  disabled?: boolean;
};

function sanitizeGenreName(raw: string): string {
  return raw
    .replace(/_/g, " ")
    .replace(/\b\w/g, (ch) => ch.toUpperCase());
}

export interface ControllerGameHubProps {
  game: GameInfo;
  /** Local captures for this title (newest first); hub prefers these over poster art */
  screenshotUrls: string[];
  playtimeData: PlaytimeStore;
  selectedVariantId: string;
  currentStreamingGame: GameInfo | null | undefined;
  librarySortLabel?: string | null;
  tiles: GameHubTile[];
  focusIndex: number;
  /** In-stream overlay: copy emphasizes switching away from the active title */
  inStreamMenu?: boolean;
  /** When false, omit the full-bleed blurred hero; poster row and copy remain */
  showHeroBackdrop?: boolean;
}

export function ControllerGameHub({
  game,
  screenshotUrls,
  playtimeData,
  selectedVariantId,
  currentStreamingGame,
  librarySortLabel,
  tiles,
  focusIndex,
  inStreamMenu = false,
  showHeroBackdrop = true,
}: ControllerGameHubProps): JSX.Element {
  const record = playtimeData[game.id];
  const totalSecs = record?.totalSeconds ?? 0;
  const lastPlayedAt = record?.lastPlayedAt ?? null;
  const sessionCount = record?.sessionCount ?? 0;
  const playtimeLabel = formatPlaytime(totalSecs);
  const lastPlayedLabel = formatLastPlayed(lastPlayedAt);
  const variant = game.variants.find((v) => v.id === selectedVariantId) || game.variants[0];
  const storeName = getStoreDisplayName(variant?.store || "");
  const genres = game.genres?.slice(0, 4) ?? [];
  const tierLabel = game.membershipTierLabel;
  const description =
    game.longDescription?.trim() || game.description?.trim() || `${game.title} is ready to launch from your library.`;

  const primaryVisualUrl =
    screenshotUrls[0] ?? game.screenshotUrl ?? game.imageUrl ?? null;
  const heroBackdropUrl = primaryVisualUrl;

  const safeFocus = Math.max(0, Math.min(tiles.length - 1, focusIndex));
  const extraShots = screenshotUrls.slice(1, 8);

  return (
    <div className="xmb-ps5-game-hub" role="region" aria-label={`${game.title} game hub`}>
      <div className="xmb-ps5-game-hub-bg" aria-hidden>
        {showHeroBackdrop && heroBackdropUrl ? (
          <div className="xmb-ps5-game-hub-hero" style={{ backgroundImage: `url(${heroBackdropUrl})` }} />
        ) : null}
        <div className="xmb-ps5-game-hub-scrim" />
      </div>

      <div className="xmb-ps5-game-hub-content">
        {primaryVisualUrl ? (
          <div className="xmb-ps5-game-hub-poster-row">
            <div className="xmb-ps5-game-hub-poster-frame">
              <img
                src={primaryVisualUrl}
                alt=""
                className="xmb-ps5-game-hub-poster-img"
                decoding="async"
              />
            </div>
            {extraShots.length > 0 ? (
              <div className="xmb-ps5-game-hub-shot-strip" aria-hidden>
                {extraShots.map((src, i) => (
                  <img
                    key={`hub-strip-${i}`}
                    src={src}
                    alt=""
                    className="xmb-ps5-game-hub-shot-thumb"
                    decoding="async"
                  />
                ))}
              </div>
            ) : null}
          </div>
        ) : null}

        <header className="xmb-ps5-game-hub-header">
          <h1 className="xmb-ps5-game-hub-title">{game.title}</h1>
          <div className="xmb-ps5-game-hub-chips">
            {librarySortLabel ? (
              <span className="xmb-game-meta-chip xmb-game-meta-chip--sort">Sort: {librarySortLabel}</span>
            ) : null}
            {storeName ? <span className="xmb-game-meta-chip xmb-game-meta-chip--store">{storeName}</span> : null}
            <span className="xmb-game-meta-chip xmb-game-meta-chip--playtime">
              <Clock size={10} className="xmb-meta-icon" />
              {playtimeLabel}
            </span>
            <span className="xmb-game-meta-chip xmb-game-meta-chip--last-played">
              <Calendar size={10} className="xmb-meta-icon" />
              {lastPlayedLabel}
            </span>
            {sessionCount > 0 ? (
              <span className="xmb-game-meta-chip xmb-game-meta-chip--sessions">
                <Repeat2 size={10} className="xmb-meta-icon" />
                {sessionCount === 1 ? "1 session" : `${sessionCount} sessions`}
              </span>
            ) : null}
            {genres.map((g) => (
              <span key={g} className="xmb-game-meta-chip xmb-game-meta-chip--genre">
                {sanitizeGenreName(g)}
              </span>
            ))}
            {tierLabel ? <span className="xmb-game-meta-chip xmb-game-meta-chip--tier">{tierLabel}</span> : null}
          </div>
        </header>

        <p className="xmb-ps5-game-hub-description">{description}</p>

        <div className="xmb-ps5-game-hub-actions" role="listbox" aria-label="Game hub actions">
          {tiles.map((tile, idx) => {
            const active = idx === safeFocus;
            const primary = tile.id === "play";
            return (
              <div
                key={tile.id}
                role="option"
                aria-selected={active}
                aria-disabled={tile.disabled ?? false}
                className={`xmb-ps5-game-hub-tile ${active ? "active" : ""} ${tile.disabled ? "xmb-ps5-game-hub-tile--disabled" : ""} ${primary ? "xmb-ps5-game-hub-tile--primary" : ""}`.trim()}
              >
                <div className="xmb-ps5-game-hub-tile-body">
                  <span className="xmb-ps5-game-hub-tile-title">{tile.title}</span>
                  <span className="xmb-ps5-game-hub-tile-sub">{tile.subtitle}</span>
                </div>
              </div>
            );
          })}
        </div>

        <div className="xmb-ps5-game-hub-stream-hint" aria-hidden>
          {currentStreamingGame && currentStreamingGame.id !== game.id ? (
            <span>
              {inStreamMenu
                ? `In-stream: leaving ${currentStreamingGame.title} switches to ${game.title}`
                : `Streaming another title — Play switches to ${game.title}`}
            </span>
          ) : (
            <span>Select an action above</span>
          )}
        </div>
      </div>
    </div>
  );
}
