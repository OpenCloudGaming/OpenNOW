import type { JSX, RefObject } from "react";
import type { GameInfo, SubscriptionInfo } from "@shared/gfn";
import { Clock, Calendar, Repeat2, Star } from "lucide-react";
import { spotlightEntryHasGame } from "./helpers";
import type { HomeRootPlane, SpotlightEntry } from "./types";
import { formatLastPlayed, formatPlaytime, type PlaytimeStore } from "../../../utils/usePlaytime";
import { HomeSubscriptionMeta } from "./HomeSubscriptionMeta";
import { SpotlightShelfBand } from "./SpotlightShelfBand";

interface TopLevelShelfSectionProps {
  topLevelShelfActive: boolean;
  focusMotionKey: string;
  selectedTopLevelItemLabel: string;
  topCategory: string;
  gameSubcategory: string;
  gamesRootPlane: "spotlight" | "categories";
  homeRootPlane?: HomeRootPlane;
  spotlightEntries: SpotlightEntry[];
  spotlightIndex: number;
  displayItems: Array<{ id?: string }>;
  topLevelShelfIndex: number;
  currentTabGame?: GameInfo | null;
  featuredHomeGame?: GameInfo | null;
  /** When focused tile is featured, true if that game is in favorites. */
  featuredIsFavorite?: boolean;
  playtimeData: PlaytimeStore;
  gamesDualShelf: boolean;
  homeDualShelf?: boolean;
  inStreamMenu?: boolean;
  subscriptionInfo?: SubscriptionInfo | null;
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
  homeRootPlane = "spotlight",
  spotlightEntries,
  spotlightIndex,
  displayItems,
  topLevelShelfIndex,
  currentTabGame,
  featuredHomeGame = null,
  featuredIsFavorite = false,
  playtimeData,
  gamesDualShelf,
  homeDualShelf = false,
  inStreamMenu = false,
  subscriptionInfo = null,
  cloudSessionResumable,
  onResumeCloudSession,
  spotlightTrackRef,
  spotlightShelfTranslateX,
  topLevelMenuTrack,
}: TopLevelShelfSectionProps): JSX.Element | null {
  if (!topLevelShelfActive) return null;

  const showDualShelf = gamesDualShelf || homeDualShelf;
  const gamesSpotlightPlane = gamesRootPlane === "spotlight";
  const homeSpotlightPlane = homeRootPlane === "spotlight";
  const shelfLabel =
    cloudSessionResumable && onResumeCloudSession ? "Resume & recently played" : "Recently played";

  const focusedId = displayItems[topLevelShelfIndex]?.id;

  return (
    <div className="xmb-ps5-stack">
      <div className="xmb-ps5-focus-meta" aria-live="polite" key={focusMotionKey}>
        <h2 className="xmb-ps5-focus-title">{selectedTopLevelItemLabel}</h2>
        {topCategory === "all" && gameSubcategory === "root" && gamesDualShelf && gamesSpotlightPlane ? (
          <p className="xmb-ps5-focus-subtitle">
            {(() => {
              const se = spotlightEntries[spotlightIndex];
              if (se?.kind === "cloudResume") {
                return se.busy
                  ? "Resuming your cloud session…"
                  : "Active cloud session · Enter continues from where you left off";
              }
              if (spotlightEntryHasGame(se)) {
                return "Recently played";
              }
              return "Recently played · Empty slot — play games to fill your shelf";
            })()}
          </p>
        ) : null}
        {topCategory === "current" && homeSpotlightPlane && homeDualShelf ? (
          <p className="xmb-ps5-focus-subtitle">
            {(() => {
              const se = spotlightEntries[spotlightIndex];
              if (se?.kind === "cloudResume") {
                return se.busy
                  ? "Resuming your cloud session…"
                  : "Active cloud session · Enter continues from where you left off";
              }
              if (spotlightEntryHasGame(se)) {
                return "Recently played";
              }
              return "Recently played · Empty slot — play games to fill your shelf";
            })()}
          </p>
        ) : null}
        {topCategory === "current" && !inStreamMenu ? <HomeSubscriptionMeta subscriptionInfo={subscriptionInfo} /> : null}
        {topCategory === "current" && focusedId === "resume" && currentTabGame ? (
          <div className="xmb-ps5-focus-chips">
            {(() => {
              const record = playtimeData[currentTabGame.id];
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
        {topCategory === "current" && focusedId === "featured" && featuredHomeGame ? (
          <div className="xmb-ps5-focus-chips">
            {(() => {
              const record = playtimeData[featuredHomeGame.id];
              const totalSecs = record?.totalSeconds ?? 0;
              const genres = featuredHomeGame.genres?.filter(Boolean).slice(0, 2).join(" · ");
              return (
                <>
                  {totalSecs === 0 ? (
                    <span className="xmb-game-meta-chip xmb-game-meta-chip--playtime">Never played</span>
                  ) : (
                    <span className="xmb-game-meta-chip xmb-game-meta-chip--playtime">
                      <Clock size={10} className="xmb-meta-icon" />
                      {formatPlaytime(totalSecs)}
                    </span>
                  )}
                  <span className="xmb-game-meta-chip xmb-game-meta-chip--sessions">
                    <Star size={10} className="xmb-meta-icon" />
                    {featuredIsFavorite ? "Favorite · featured pick" : "Featured pick"}
                  </span>
                  {genres ? (
                    <span className="xmb-game-meta-chip xmb-game-meta-chip--last-played">{genres}</span>
                  ) : null}
                </>
              );
            })()}
          </div>
        ) : null}
      </div>
      {showDualShelf ? (
        <div className="xmb-ps5-shelf-anchored">
          <SpotlightShelfBand
            spotlightTrackRef={spotlightTrackRef}
            spotlightShelfTranslateX={spotlightShelfTranslateX}
            spotlightEntries={spotlightEntries}
            spotlightIndex={spotlightIndex}
            spotlightPlaneActive={
              gamesDualShelf ? gamesSpotlightPlane : Boolean(homeDualShelf && homeSpotlightPlane)
            }
            shelfLabel={shelfLabel}
            ariaLabel="Recently played games"
          />
          <div className="xmb-ps5-shelf-band xmb-ps5-shelf-band--library">
            <div className="xmb-ps5-shelf-viewport xmb-ps5-shelf-viewport--games-root">{topLevelMenuTrack}</div>
          </div>
        </div>
      ) : (
        <div className={`xmb-ps5-shelf-viewport ${topCategory === "all" && gameSubcategory === "root" ? "xmb-ps5-shelf-viewport--games-root" : ""}`}>{topLevelMenuTrack}</div>
      )}
    </div>
  );
}
