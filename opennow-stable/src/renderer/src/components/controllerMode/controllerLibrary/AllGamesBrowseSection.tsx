import type { JSX, RefObject } from "react";
import type { GameInfo } from "@shared/gfn";
import { Star, Clock, Calendar, Repeat2 } from "lucide-react";
import { getStoreDisplayName } from "../../GameCard";
import { formatLastPlayed, formatPlaytime, type PlaytimeStore } from "../../../utils/usePlaytime";
import { LIBRARY_SORT_LABEL, SHELF_IMAGE_PROPS } from "./constants";
import { isWithinContentWindow, isWithinImageWindow, sanitizeGenreName } from "./helpers";
import type { LibrarySortId } from "./types";

interface AllGamesBrowseSectionProps {
  isLoading: boolean;
  categorizedGames: GameInfo[];
  selectedGame?: GameInfo;
  focusMotionKey: string;
  gameSubcategory: string;
  librarySortId: LibrarySortId;
  playtimeData: PlaytimeStore;
  selectedVariantByGameId: Record<string, string>;
  favoriteGameIdSet: Set<string>;
  selectedIndex: number;
  itemsContainerRef: RefObject<HTMLDivElement | null>;
  listTranslateX: number;
}

export function AllGamesBrowseSection({
  isLoading,
  categorizedGames,
  selectedGame,
  focusMotionKey,
  gameSubcategory,
  librarySortId,
  playtimeData,
  selectedVariantByGameId,
  favoriteGameIdSet,
  selectedIndex,
  itemsContainerRef,
  listTranslateX,
}: AllGamesBrowseSectionProps): JSX.Element {
  return (
    <div className="xmb-ps5-stack xmb-ps5-media-hub">
      {!isLoading && categorizedGames.length === 0 ? (
        <div className="xmb-ps5-focus-meta" aria-live="polite" key="games-empty">
          <h2 className="xmb-ps5-focus-title">No games here</h2>
          <p className="xmb-ps5-focus-subtitle">Try another category or refresh your library.</p>
        </div>
      ) : selectedGame ? (
        <div className="xmb-ps5-focus-meta" aria-live="polite" key={focusMotionKey}>
          <h2 className="xmb-ps5-focus-title">{selectedGame.title}</h2>
          <div className="xmb-ps5-actions">
            <span className="xmb-ps5-action xmb-ps5-action--primary">Game hub</span>
            <span className="xmb-ps5-action">Options</span>
          </div>
          <div className="xmb-ps5-focus-chips">
            {gameSubcategory === "all" ? (
              <span className="xmb-game-meta-chip xmb-game-meta-chip--sort">Sort: {LIBRARY_SORT_LABEL[librarySortId]}</span>
            ) : null}
            {(() => {
              const record = playtimeData[selectedGame.id];
              const totalSecs = record?.totalSeconds ?? 0;
              const lastPlayedAt = record?.lastPlayedAt ?? null;
              const sessionCount = record?.sessionCount ?? 0;
              const playtimeLabel = formatPlaytime(totalSecs);
              const lastPlayedLabel = formatLastPlayed(lastPlayedAt);
              const vId = selectedVariantByGameId[selectedGame.id] || selectedGame.variants[0]?.id;
              const variant = selectedGame.variants.find((v) => v.id === vId) || selectedGame.variants[0];
              const storeName = getStoreDisplayName(variant?.store || "");
              const genres = selectedGame.genres?.slice(0, 3) ?? [];
              const tierLabel = selectedGame.membershipTierLabel;
              return (
                <>
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
                </>
              );
            })()}
          </div>
        </div>
      ) : null}
      <div className="xmb-ps5-shelf-label-row xmb-ps5-shelf-label-row--active xmb-ps5-shelf-label-row--games-list">
        <span className="xmb-ps5-shelf-label">Games</span>
      </div>
      <div className="xmb-ps5-shelf-viewport">
        <div
          ref={itemsContainerRef}
          className="xmb-ps5-shelf-track"
          role="listbox"
          aria-label="Game library"
          style={{ transform: `translateX(${listTranslateX}px)` }}
        >
          {!isLoading && categorizedGames.length === 0
            ? Array.from({ length: 6 }).map((_, idx) => (
              <div key={`game-empty-${idx}`} className={`xmb-ps5-tile ${idx === 0 ? "active" : ""}`} role="option" aria-selected={idx === 0} aria-label="Empty slot">
                <div className="xmb-ps5-tile-frame xmb-ps5-tile-frame--placeholder" />
              </div>
            ))
            : categorizedGames.map((game, idx) => {
              const isActive = idx === selectedIndex;
              const shouldRenderContent = isWithinContentWindow(idx, selectedIndex);
              const shouldRenderImage = isWithinImageWindow(idx, selectedIndex);
              const eagerLoadImage = Math.abs(idx - selectedIndex) <= 2;
              return (
                <div
                  key={game.id}
                  className={`xmb-ps5-tile ${isActive ? "active" : ""}`}
                  role="option"
                  aria-selected={isActive}
                  aria-label={game.title}
                >
                  {shouldRenderContent ? (
                    <>
                      {favoriteGameIdSet.has(game.id) ? <Star className="xmb-ps5-tile-fav" aria-hidden /> : null}
                      <div className="xmb-ps5-tile-frame">
                        {game.imageUrl && shouldRenderImage ? (
                          <img
                            src={game.imageUrl}
                            alt=""
                            className="xmb-ps5-tile-cover"
                            {...SHELF_IMAGE_PROPS}
                            loading={eagerLoadImage ? "eager" : "lazy"}
                          />
                        ) : <div className="xmb-ps5-tile-cover xmb-ps5-tile-cover--placeholder" />}
                      </div>
                    </>
                  ) : <div className="xmb-ps5-tile-frame xmb-ps5-tile-frame--virtualized" aria-hidden />}
                </div>
              );
            })}
        </div>
      </div>
    </div>
  );
}
