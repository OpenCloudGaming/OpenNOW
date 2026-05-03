import type { JSX } from "react";
import type { GameInfo } from "@shared/gfn";
import { Clock, Calendar, Repeat2 } from "lucide-react";
import { getStoreDisplayName } from "../../GameCard";
import { SessionElapsedIndicator } from "../../ElapsedSessionIndicators";
import { formatPlaytime, formatLastPlayed, type PlaytimeStore } from "../../../utils/usePlaytime";
import { sanitizeGenreName } from "./helpers";

interface CurrentDetailPanelProps {
  topCategory: string;
  pendingSwitchGameCover?: string | null;
  currentStreamingGame?: GameInfo | null;
  attachPosterRef: (el: HTMLImageElement | null) => void;
  metaMaxWidth: number | null;
  selectedVariantByGameId: Record<string, string>;
  playtimeData: PlaytimeStore;
  sessionCounterEnabled: boolean;
  sessionStartedAtMs: number | null;
  isStreaming: boolean;
}

export function CurrentDetailPanel({
  topCategory,
  pendingSwitchGameCover,
  currentStreamingGame,
  attachPosterRef,
  metaMaxWidth,
  selectedVariantByGameId,
  playtimeData,
  sessionCounterEnabled,
  sessionStartedAtMs,
  isStreaming,
}: CurrentDetailPanelProps): JSX.Element | null {
  if (topCategory !== "current") return null;

  return (
    <div className="xmb-current-detail">
      <div className="xmb-current-poster">
        <img ref={attachPosterRef} src={pendingSwitchGameCover ?? currentStreamingGame?.imageUrl} alt={currentStreamingGame?.title ?? "Current"} />
      </div>
      <div className="xmb-current-info">
        <div className="xmb-game-title">{currentStreamingGame?.title ?? "Current Game"}</div>
        <div
          className="xmb-game-meta"
          style={{
            maxWidth: metaMaxWidth ? `${metaMaxWidth}px` : undefined,
            justifyContent: "flex-end",
          }}
        >
          {(() => {
            const cs = currentStreamingGame;
            if (!cs) return null;
            const vId = selectedVariantByGameId[cs.id] || cs.variants[0]?.id;
            const variant = cs.variants.find((v) => v.id === vId) || cs.variants[0];
            const storeName = getStoreDisplayName(variant?.store || "");
            const record = (playtimeData ?? {})[cs.id];
            const totalSecs = record?.totalSeconds ?? 0;
            const lastPlayed = record?.lastPlayedAt ?? null;
            const sessionCount = record?.sessionCount ?? 0;
            const playtimeLabel = formatPlaytime(totalSecs);
            const lastPlayedLabel = formatLastPlayed(lastPlayed);
            const genres = cs.genres?.slice(0, 2) ?? [];
            const tier = cs.membershipTierLabel;
            return (
              <>
                {storeName && <span className="xmb-game-meta-chip xmb-game-meta-chip--store">{storeName}</span>}
                {sessionCounterEnabled && (
                  <span className="xmb-game-meta-chip xmb-game-meta-chip--session">
                    <SessionElapsedIndicator startedAtMs={sessionStartedAtMs ?? null} active={isStreaming} />
                  </span>
                )}
                <span className="xmb-game-meta-chip xmb-game-meta-chip--playtime">
                  <Clock size={10} className="xmb-meta-icon" />
                  {playtimeLabel}
                </span>
                <span className="xmb-game-meta-chip xmb-game-meta-chip--last-played">
                  <Calendar size={10} className="xmb-meta-icon" />
                  {lastPlayedLabel}
                </span>
                {sessionCount > 0 && (
                  <span className="xmb-game-meta-chip xmb-game-meta-chip--sessions">
                    <Repeat2 size={10} className="xmb-meta-icon" />
                    {sessionCount === 1 ? "1 session" : `${sessionCount} sessions`}
                  </span>
                )}
                {genres.map((g) => (
                  <span key={g} className="xmb-game-meta-chip xmb-game-meta-chip--genre">{sanitizeGenreName(g)}</span>
                ))}
                {tier && <span className="xmb-game-meta-chip xmb-game-meta-chip--tier">{tier}</span>}
              </>
            );
          })()}
        </div>
      </div>
    </div>
  );
}
