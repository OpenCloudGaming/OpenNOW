import type { JSX, RefObject } from "react";
import { SHELF_IMAGE_PROPS } from "./constants";
import type { SpotlightEntry } from "./types";

export interface SpotlightShelfBandProps {
  spotlightTrackRef: RefObject<HTMLDivElement | null>;
  spotlightShelfTranslateX: number;
  spotlightEntries: SpotlightEntry[];
  spotlightIndex: number;
  /** When true, spotlight row receives focus styling and tile `active` state. */
  spotlightPlaneActive: boolean;
  shelfLabel: string;
  ariaLabel?: string;
}

export function SpotlightShelfBand({
  spotlightTrackRef,
  spotlightShelfTranslateX,
  spotlightEntries,
  spotlightIndex,
  spotlightPlaneActive,
  shelfLabel,
  ariaLabel = "Recently played games",
}: SpotlightShelfBandProps): JSX.Element {
  return (
    <div className="xmb-ps5-shelf-band xmb-ps5-shelf-band--spotlight">
      <div className={`xmb-ps5-shelf-label-row xmb-ps5-shelf-label-row--spotlight ${spotlightPlaneActive ? "xmb-ps5-shelf-label-row--active" : ""}`}>
        <span className="xmb-ps5-shelf-label">{shelfLabel}</span>
      </div>
      <div className="xmb-ps5-shelf-viewport xmb-ps5-shelf-viewport--spotlight">
        <div
          ref={spotlightTrackRef}
          className="xmb-ps5-shelf-track xmb-ps5-shelf-track--spotlight"
          role="listbox"
          aria-label={ariaLabel}
          style={{ transform: `translateX(${spotlightShelfTranslateX}px)` }}
        >
          {spotlightEntries.map((entry, idx) => {
            const isActive = spotlightPlaneActive && idx === spotlightIndex;
            if (entry.kind === "cloudResume") {
              return (
                <div
                  key="spotlight-cloud-resume"
                  className={`xmb-ps5-tile xmb-ps5-tile--spotlight xmb-ps5-tile--spotlight-resume ${isActive ? "active" : ""} ${entry.busy ? "xmb-ps5-tile--spotlight-resume-busy" : ""}`.trim()}
                  role="option"
                  aria-selected={isActive}
                  aria-label={`Resume ${entry.title}`}
                >
                  <div className="xmb-ps5-tile-frame">
                    {entry.coverUrl ? (
                      <img src={entry.coverUrl} alt="" className="xmb-ps5-tile-cover" {...SHELF_IMAGE_PROPS} />
                    ) : (
                      <div className="xmb-ps5-tile-cover xmb-ps5-tile-cover--placeholder" />
                    )}
                    <div className="xmb-ps5-spotlight-resume-badge" aria-hidden>
                      <span className="xmb-ps5-spotlight-resume-label">{entry.busy ? "Connecting…" : "Resume"}</span>
                    </div>
                  </div>
                </div>
              );
            }
            const game = entry.game;
            const key = game ? game.id : `recent-empty-${idx}`;
            return (
              <div
                key={key}
                className={`xmb-ps5-tile xmb-ps5-tile--spotlight ${game ? "" : "xmb-ps5-tile--spotlight-empty"} ${isActive ? "active" : ""}`.trim()}
                role="option"
                aria-selected={isActive}
                aria-label={game ? game.title : "Empty recent slot"}
              >
                <div className="xmb-ps5-tile-frame">
                  {game?.imageUrl ? <img src={game.imageUrl} alt="" className="xmb-ps5-tile-cover" {...SHELF_IMAGE_PROPS} /> : <div className="xmb-ps5-tile-cover xmb-ps5-tile-cover--placeholder" />}
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
