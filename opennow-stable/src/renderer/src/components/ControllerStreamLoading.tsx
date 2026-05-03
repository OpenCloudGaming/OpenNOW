import type { JSX, Ref } from "react";
import {
  getPreferredSessionAdMediaUrl,
  getSessionAdMessage,
  isSessionQueuePaused,
} from "@shared/gfn";
import type { SessionAdInfo, SessionAdState } from "@shared/gfn";
import { QueueAdPreview, type QueueAdPlaybackEvent, type QueueAdPreviewHandle } from "./QueueAdPreview";
import { Ps5ThreeDots } from "./Ps5LoadingScreen";

export interface ControllerStreamLoadingProps {
  gameTitle: string;
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
      return "Setting up stream...";
    default:
      return "Loading...";
  }
}

export function ControllerStreamLoading({
  gameTitle,
  status,
  queuePosition,
  adState,
  activeAd,
  activeAdMediaUrl,
  error,
  onAdPlaybackEvent,
  adPreviewRef,
}: ControllerStreamLoadingProps): JSX.Element {
  const statusMessage = getStatusMessage(status, queuePosition, adState);
  const cachedAdMediaUrl = activeAdMediaUrl ?? getPreferredSessionAdMediaUrl(activeAd);
  const adMessage = getSessionAdMessage(adState) ?? (isSessionQueuePaused(adState) ? "Resume ads to stay in queue." : undefined);
  const hasError = Boolean(error);

  return (
    <div className="controller-stream-loading">
      <div className="csl-backdrop" />

      <div className="csl-content-wrapper">
        <div className="csl-content">
          <div className="csl-info-section">
            <div className="csl-load-dots-wrap" aria-hidden>
              <Ps5ThreeDots size="lg" />
            </div>

            <div className="csl-title-stack">
              <h1 className="csl-title">{gameTitle}</h1>
              <p className="csl-status-line">{statusMessage}</p>
            </div>

            {hasError && error ? (
              <div className="csl-error-panel" role="alert">
                <div className="csl-error-title">{error.title}</div>
                <div className="csl-error-description">{error.description}</div>
                {error.code ? <div className="csl-error-code">{error.code}</div> : null}
              </div>
            ) : null}

            {!hasError && activeAd && cachedAdMediaUrl ? (
              <div className={`csl-ad-panel${isSessionQueuePaused(adState) ? " csl-ad-panel--paused" : ""}`}>
                <div className="csl-ad-copy">
                  <span className="csl-ad-chip">Ad Queue</span>
                  {adMessage ? <div className="csl-ad-message">{adMessage}</div> : null}
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
            ) : null}
          </div>
        </div>
      </div>
    </div>
  );
}
