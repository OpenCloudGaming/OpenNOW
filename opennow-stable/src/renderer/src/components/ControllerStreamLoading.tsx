import { Loader2, Zap } from "lucide-react";
import type { JSX, Ref } from "react";
import {
  getPreferredSessionAdMediaUrl,
  getSessionAdDurationMs,
  getSessionAdGracePeriodSeconds,
  getSessionAdMessage,
  isSessionQueuePaused,
} from "@shared/gfn";
import type { SessionAdInfo, SessionAdState } from "@shared/gfn";
import { formatPlaytime } from "../utils/usePlaytime";
import type { PlaytimeStore } from "../utils/usePlaytime";
import { QueueAdPreview, type QueueAdPlaybackEvent, type QueueAdPreviewHandle } from "./QueueAdPreview";

export interface ControllerStreamLoadingProps {
  gameTitle: string;
  gamePoster?: string;
  gameDescription?: string;
  status: "queue" | "setup" | "starting" | "connecting";
  queuePosition?: number;
  adState?: SessionAdState;
  activeAd?: SessionAdInfo;
  activeAdMediaUrl?: string;
  error?: {
    title: string;
    description: string;
    code?: string;
  };
  onAdPlaybackEvent?: (event: QueueAdPlaybackEvent, adId: string) => void;
  adPreviewRef?: Ref<QueueAdPreviewHandle>;
  playtimeData?: PlaytimeStore;
  gameId?: string;
  enableBackgroundAnimations?: boolean;
}

function getStatusMessage(
  status: ControllerStreamLoadingProps["status"],
  queuePosition?: number,
  adState?: SessionAdState,
): string {
  if (isSessionQueuePaused(adState)) {
    return "Session queue paused";
  }
  switch (status) {
    case "queue":
      return queuePosition ? `Position #${queuePosition} in queue` : "Waiting in queue...";
    case "setup":
      return "Setting up your gaming rig...";
    case "starting":
      return "Starting stream...";
    case "connecting":
      return "Connecting to server...";
    default:
      return "Loading...";
  }
}

function getStatusPhase(
  status: ControllerStreamLoadingProps["status"],
): "queue" | "setup" | "launching" {
  switch (status) {
    case "queue":
      return "queue";
    case "setup":
      return "setup";
    case "starting":
    case "connecting":
      return "launching";
    default:
      return "queue";
  }
}

export function ControllerStreamLoading({
  gameTitle,
  gamePoster,
  gameDescription,
  status,
  queuePosition,
  adState,
  activeAd,
  activeAdMediaUrl,
  error,
  onAdPlaybackEvent,
  adPreviewRef,
  playtimeData = {},
  gameId,
  enableBackgroundAnimations = false,
}: ControllerStreamLoadingProps): JSX.Element {
  const statusMessage = getStatusMessage(status, queuePosition, adState);
  const statusPhase = getStatusPhase(status);
  const playtimeRecord = gameId ? playtimeData[gameId] : undefined;
  const totalSecs = playtimeRecord?.totalSeconds ?? 0;
  const playtimeLabel = formatPlaytime(totalSecs);
  const cachedAdMediaUrl = activeAdMediaUrl ?? getPreferredSessionAdMediaUrl(activeAd);
  const adDurationMs = getSessionAdDurationMs(activeAd);
  const adDurationSeconds = adDurationMs ? Math.round(adDurationMs / 1000) : undefined;
  const adMessage = getSessionAdMessage(adState) ?? (isSessionQueuePaused(adState) ? "Resume ads to stay in queue." : undefined);
  const gracePeriodSeconds = getSessionAdGracePeriodSeconds(adState);
  const hasError = Boolean(error);

  return (
    <div className="controller-stream-loading">
      {enableBackgroundAnimations && (
        <div className={`xmb-wrapper ${enableBackgroundAnimations ? "xmb-animate" : ""}`} aria-hidden>
          <div className="xmb-bg-layer">
            <div className="xmb-bg-gradient" />
            <div className="xmb-bg-overlay" />
          </div>
        </div>
      )}
      {/* Fade-to-black backdrop */}
      <div className="csl-backdrop" />

      {/* Content fade-in layer */}
      <div className="csl-content-wrapper">
        <div className="csl-content">
          {/* Left side: Game Poster */}
          <div className="csl-poster-section">
            {gamePoster ? (
              <img src={gamePoster} alt={gameTitle} className="csl-poster" />
            ) : (
              <div className="csl-poster-placeholder">
                <Zap size={48} />
              </div>
            )}
          </div>

          {/* Right side: Game Info and Status */}
          <div className="csl-info-section">
            {/* Game Title */}
            <div className="csl-title-container">
              <h1 className="csl-title">{gameTitle}</h1>
            </div>

            {/* Game Description */}
            {gameDescription && (
              <div className="csl-description-container">
                <p className="csl-description">{gameDescription}</p>
              </div>
            )}

            {/* Playtime */}
            {playtimeLabel !== "0h" && (
              <div className="csl-playtime-container">
                <span className="csl-playtime-label">Playtime:</span>
                <span className="csl-playtime-value">{playtimeLabel}</span>
              </div>
            )}

            {/* Network Status Section */}
            <div className="csl-status-container">
              <div className="csl-status-message">{statusMessage}</div>

              {!hasError && activeAd && cachedAdMediaUrl && (
                <div className={`csl-ad-panel${isSessionQueuePaused(adState) ? " csl-ad-panel--paused" : ""}`}>
                  <div className="csl-ad-copy">
                    <span className="csl-ad-chip">Ad Queue</span>
                    {adMessage && <div className="csl-ad-message">{adMessage}</div>}
                  </div>
                  <div className="csl-ad-media">
                    <QueueAdPreview
                      ref={adPreviewRef}
                      mediaUrl={cachedAdMediaUrl}
                      title={activeAd.title}
                      onPlaybackEvent={(event) => onAdPlaybackEvent?.(event, activeAd.adId)}
                    />
                  </div>
                </div>
              )}

              {hasError && error && (
                <div className="csl-error-panel" role="alert">
                  <div className="csl-error-title">{error.title}</div>
                  <div className="csl-error-description">{error.description}</div>
                  {error.code && <div className="csl-error-code">{error.code}</div>}
                </div>
              )}

              {/* Status Progress Indicator */}
              <div className="csl-progress-indicator">
                <div className={`csl-progress-step csl-progress-queue ${statusPhase !== "queue" ? "completed" : "active"}`}>
                  <span className="csl-progress-dot" />
                  <span className="csl-progress-label">Queue</span>
                </div>

                <div className={`csl-progress-connector ${statusPhase === "queue" ? "inactive" : ""}`} />

                <div
                  className={`csl-progress-step csl-progress-setup ${
                    statusPhase === "queue" ? "inactive" : statusPhase === "setup" ? "active" : "completed"
                  }`}
                >
                  <span className="csl-progress-dot" />
                  <span className="csl-progress-label">Setup</span>
                </div>

                <div
                  className={`csl-progress-connector ${statusPhase === "launching" ? "" : "inactive"}`}
                />

                <div
                  className={`csl-progress-step csl-progress-launching ${statusPhase === "launching" ? "active" : "inactive"}`}
                >
                  <span className="csl-progress-dot" />
                  <span className="csl-progress-label">Launching</span>
                </div>
              </div>

              {/* Loading Spinner */}
              <div className="csl-spinner-container">
                <Loader2 className="csl-spinner" size={32} />
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
