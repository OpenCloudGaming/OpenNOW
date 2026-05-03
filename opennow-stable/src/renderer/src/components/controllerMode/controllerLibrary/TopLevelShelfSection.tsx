import type { JSX, RefObject } from "react";
import type { GameInfo } from "@shared/gfn";
import { Clock, Calendar, Repeat2 } from "lucide-react";
import { SHELF_IMAGE_PROPS } from "./constants";
import { spotlightEntryHasGame } from "./helpers";
import type { SpotlightEntry } from "./types";
import { formatLastPlayed, formatPlaytime, type PlaytimeStore } from "../../../utils/usePlaytime";

interface TopLevelShelfSectionProps {
  topLevelShelfActive: boolean;
  focusMotionKey: string;
  selectedTopLevelItemLabel: string;
  topCategory: string;
  gameSubcategory: string;
  gamesRootPlane: "spotlight" | "categories";
  spotlightEntries: SpotlightEntry[];
  spotlightIndex: number;
  displayItems: Array<{ id?: string }>;
  topLevelShelfIndex: number;
  currentStreamingGame?: GameInfo | null;
  playtimeData: PlaytimeStore;
  gamesDualShelf: boolean;
  cloudSessionResumable?: boolean;
  onResumeCloudSession?: () => void;
  spotlightTrackRef: RefObject<HTMLDivElement | null>;
  spotlightShelfTranslateX: number;
  topLevelMenuTrack: JSX.Element;
}

export function TopLevelShelfSection({
  topLevelShelfActive,
  focusMotionKey,
  selectedTopLevelItemLabel,
  topCategory,
  gameSubcategory,
  gamesRootPlane,
  spotlightEntries,
  spotlightIndex,
  displayItems,
  topLevelShelfIndex,
  currentStreamingGame,
  playtimeData,
  gamesDualShelf,
  cloudSessionResumable,
  onResumeCloudSession,
  spotlightTrackRef,
  spotlightShelfTranslateX,
  topLevelMenuTrack,
}: TopLevelShelfSectionProps): JSX.Element | null {
  if (!topLevelShelfActive) return null;

  return (
    <div className="xmb-ps5-stack">
      <div className="xmb-ps5-focus-meta" aria-live="polite" key={focusMotionKey}>
        <h2 className="xmb-ps5-focus-title">{selectedTopLevelItemLabel}</h2>
        {topCategory === "all" && gameSubcategory === "root" && gamesRootPlane === "spotlight" ? (
          <p className="xmb-ps5-focus-subtitle">
            {(() => {
              const se = spotlightEntries[spotlightIndex];
              if (se?.kind === "cloudResume") {
                return se.busy
                  ? "Resuming your cloud session…"
                  : "Active cloud session · Enter continues from where you left off";
              }
              if (spotlightEntryHasGame(se)) {
                return "Recently played · Enter opens this title in your library";
              }
              return "Recently played · Empty slot — play games to fill your shelf";
            })()}
          </p>
        ) : null}
        {topCategory === "current" && displayItems[topLevelShelfIndex]?.id === "resume" && currentStreamingGame ? (
          <div className="xmb-ps5-focus-chips">
            {(() => {
              const record = playtimeData[currentStreamingGame.id];
              const totalSecs = record?.totalSeconds ?? 0;
              const lastPlayedAt = record?.lastPlayedAt ?? null;
              const sessionCount = record?.sessionCount ?? 0;
              return (
                <>
                  <span className="xmb-game-meta-chip xmb-game-meta-chip--playtime">
                    <Clock size={10} className="xmb-meta-icon" />
                    {formatPlaytime(totalSecs)}
                  </span>
                  <span className="xmb-game-meta-chip xmb-game-meta-chip--last-played">
                    <Calendar size={10} className="xmb-meta-icon" />
                    {formatLastPlayed(lastPlayedAt)}
                  </span>
                  {sessionCount > 0 ? (
                    <span className="xmb-game-meta-chip xmb-game-meta-chip--sessions">
                      <Repeat2 size={10} className="xmb-meta-icon" />
                      {sessionCount === 1 ? "1 session" : `${sessionCount} sessions`}
                    </span>
                  ) : null}
                </>
              );
            })()}
          </div>
        ) : null}
      </div>
      {gamesDualShelf ? (
        <div className="xmb-ps5-shelf-anchored">
          <div className="xmb-ps5-shelf-band xmb-ps5-shelf-band--spotlight">
            <div className={`xmb-ps5-shelf-label-row xmb-ps5-shelf-label-row--spotlight ${gamesRootPlane === "spotlight" ? "xmb-ps5-shelf-label-row--active" : ""}`}>
              <span className="xmb-ps5-shelf-label">
                {cloudSessionResumable && onResumeCloudSession ? "Resume & recently played" : "Recently played"}
              </span>
            </div>
            <div className="xmb-ps5-shelf-viewport xmb-ps5-shelf-viewport--spotlight">
              <div
                ref={spotlightTrackRef}
                className="xmb-ps5-shelf-track xmb-ps5-shelf-track--spotlight"
                role="listbox"
                aria-label="Recently played games"
                style={{ transform: `translateX(${spotlightShelfTranslateX}px)` }}
              >
                {spotlightEntries.map((entry, idx) => {
                  const isActive = gamesRootPlane === "spotlight" && idx === spotlightIndex;
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
          <div className="xmb-ps5-shelf-band xmb-ps5-shelf-band--library">
            <div className="xmb-ps5-shelf-viewport xmb-ps5-shelf-viewport--games-root">{topLevelMenuTrack}</div>
          </div>
        </div>
      ) : (
        <>
          {topCategory === "current" ? (
            <div className={`xmb-ps5-shelf-label-row xmb-ps5-shelf-label-row--library ${!(topCategory === "all" && gameSubcategory === "root") || gamesRootPlane === "categories" ? "xmb-ps5-shelf-label-row--active" : ""}`}>
              <span className="xmb-ps5-shelf-label">Current</span>
            </div>
          ) : null}
          <div className={`xmb-ps5-shelf-viewport ${topCategory === "all" && gameSubcategory === "root" ? "xmb-ps5-shelf-viewport--games-root" : ""}`}>{topLevelMenuTrack}</div>
        </>
      )}
    </div>
  );
}
